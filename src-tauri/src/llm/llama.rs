//! The real summarizer: gemma-2-2b-it through llama.cpp with Metal.
//!
//! Only the safe path from `plans/DEPENDENCY_REVIEW_ENGINES.md` is used
//! (backend init, model load, context, tokenize, decode, sample); the calls
//! that would reach whisper's ggml are denied in `clippy.toml`.

use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use llama_cpp_2::{
    context::{params::LlamaContextParams, LlamaContext},
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, LlamaChatMessage, LlamaChatTemplate, LlamaModel},
    sampling::LlamaSampler,
    token::LlamaToken,
};

use super::{GenEvent, Generation, LlmError, Result, Summarizer, SummarizerLoader};
use crate::models::{ModelManager, ModelState, GEMMA_2_2B};

/// gemma-2's trained context; KTD3 budgets each sequence against it.
const CONTEXT_TOKENS: u32 = 8192;
/// Tokens per `llama_decode` call while prefilling.
const BATCH_TOKENS: usize = 2048;

/// Ollama's defaults for `gemma2:2b` (KTD4): its Modelfile sets only stop
/// words, so the runner defaults apply.
const TOP_K: i32 = 40;
const TOP_P: f32 = 0.9;
const TEMPERATURE: f32 = 0.8;
const REPEAT_LAST_N: i32 = 64;
const REPEAT_PENALTY: f32 = 1.1;

fn engine(e: impl std::fmt::Display) -> LlmError {
    LlmError::Engine(e.to_string())
}

/// llama.cpp allows one backend per process.
fn backend() -> Result<&'static LlamaBackend> {
    static BACKEND: OnceLock<std::result::Result<LlamaBackend, String>> = OnceLock::new();
    BACKEND
        .get_or_init(|| {
            LlamaBackend::init()
                .map(|mut b| {
                    b.void_logs();
                    b
                })
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| LlmError::Engine(e.clone()))
}

pub struct LlamaLoader {
    models: Arc<ModelManager>,
}

impl LlamaLoader {
    pub fn new(models: Arc<ModelManager>) -> Self {
        Self { models }
    }
}

impl SummarizerLoader for LlamaLoader {
    fn model_id(&self) -> &str {
        GEMMA_2_2B
    }

    fn load(&self) -> Result<Arc<dyn Summarizer>> {
        if self.models.readiness.get(GEMMA_2_2B) != ModelState::Ready {
            return Err(LlmError::NotReady);
        }
        let path = self.models.path(GEMMA_2_2B).ok_or(LlmError::NotReady)?;
        Ok(Arc::new(LlamaSummarizer::load(&path)?))
    }
}

pub struct LlamaSummarizer {
    model: LlamaModel,
    template: LlamaChatTemplate,
}

impl LlamaSummarizer {
    /// Loads a GGUF with every layer on the GPU.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let params = LlamaModelParams::default().with_n_gpu_layers(u32::MAX);
        let model = LlamaModel::load_from_file(backend()?, path, &params).map_err(engine)?;
        let template = model.chat_template(None).map_err(engine)?;
        Ok(Self { model, template })
    }

    /// `prompt` as the user turn of the GGUF chat template, ready for the
    /// model's reply.
    fn format(&self, prompt: &str) -> Result<Vec<LlamaToken>> {
        let message = LlamaChatMessage::new("user".into(), prompt.into()).map_err(engine)?;
        let text = self
            .model
            .apply_chat_template(&self.template, &[message], true)
            .map_err(engine)?;
        Ok(self.model.vocab().tokenize(text.as_bytes(), true, true))
    }

    fn sampler(&self) -> LlamaSampler {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        LlamaSampler::chain_simple([
            LlamaSampler::penalties(
                self.model.n_vocab(),
                REPEAT_LAST_N,
                REPEAT_PENALTY,
                0.0,
                0.0,
            ),
            LlamaSampler::top_k(TOP_K),
            LlamaSampler::top_p(TOP_P, 1),
            LlamaSampler::temp(TEMPERATURE),
            LlamaSampler::dist(seed),
        ])
    }
}

/// One sequence while it decodes.
struct Seq {
    sampler: LlamaSampler,
    /// Position of the next token.
    pos: i32,
    /// Where this sequence's logits sit in the last decoded batch.
    logits: i32,
    generated: usize,
    max_tokens: usize,
    /// Bytes of a UTF-8 character split across tokens.
    pending: Vec<u8>,
    out: String,
    done: bool,
}

impl Seq {
    /// Moves the complete UTF-8 prefix of `pending` to `out` and returns it.
    fn take_text(&mut self, flush: bool) -> &str {
        let valid = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            Err(e) if e.error_len().is_none() && !flush => e.valid_up_to(),
            Err(_) => self.pending.len(),
        };
        let start = self.out.len();
        self.out
            .push_str(&String::from_utf8_lossy(&self.pending[..valid]));
        self.pending.drain(..valid);
        &self.out[start..]
    }
}

fn decode(ctx: &mut LlamaContext<'_>, batch: &mut LlamaBatch) -> Result<()> {
    ctx.decode(batch).map_err(engine)
}

impl Summarizer for LlamaSummarizer {
    fn context_tokens(&self) -> usize {
        self.model.n_ctx_train().min(CONTEXT_TOKENS) as usize
    }

    fn count_tokens(&self, text: &str) -> Result<usize> {
        Ok(self
            .model
            .vocab()
            .tokenize(text.as_bytes(), false, false)
            .len())
    }

    fn prompt_tokens(&self, prompt: &str) -> Result<usize> {
        Ok(self.format(prompt)?.len())
    }

    fn generate(
        &self,
        requests: &[Generation],
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(GenEvent<'_>),
    ) -> Result<Vec<String>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        if requests.len() > BATCH_TOKENS {
            return Err(LlmError::Engine("too many sequences in one batch".into()));
        }
        let prompts = requests
            .iter()
            .map(|r| self.format(&r.prompt))
            .collect::<Result<Vec<_>>>()?;
        let per_seq = self.context_tokens();
        for (prompt, request) in prompts.iter().zip(requests) {
            if prompt.is_empty() || prompt.len() + request.max_tokens > per_seq {
                return Err(LlmError::Engine("prompt doesn't fit the context".into()));
            }
        }

        // One KV buffer holding every sequence at once (KTD5).
        let cells: usize = prompts
            .iter()
            .zip(requests)
            .map(|(p, r)| p.len() + r.max_tokens)
            .sum();
        let n_ctx = u32::try_from(cells).map_err(engine)?;
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx))
            .with_n_batch(BATCH_TOKENS as u32)
            .with_n_seq_max(requests.len() as u32)
            .with_kv_unified(true);
        let mut ctx = self.model.new_context(backend()?, params).map_err(engine)?;
        let mut batch = LlamaBatch::new(BATCH_TOKENS, 1);

        // Prefill all but each prompt's last token, which goes in the
        // first shared batch so every sequence has logits to sample from.
        for (seq_id, prompt) in prompts.iter().enumerate() {
            let head = &prompt[..prompt.len() - 1];
            for (start, piece) in (0..).step_by(BATCH_TOKENS).zip(head.chunks(BATCH_TOKENS)) {
                if cancel.load(Ordering::Relaxed) {
                    return Err(LlmError::Cancelled);
                }
                batch.clear();
                for (i, &token) in piece.iter().enumerate() {
                    batch
                        .add(token, (start + i) as i32, &[seq_id as i32], false)
                        .map_err(engine)?;
                }
                decode(&mut ctx, &mut batch)?;
            }
        }
        batch.clear();
        let mut seqs = Vec::with_capacity(requests.len());
        for (seq_id, (prompt, request)) in prompts.iter().zip(requests).enumerate() {
            let pos = prompt.len() as i32 - 1;
            seqs.push(Seq {
                sampler: self.sampler(),
                pos: pos + 1,
                logits: batch.n_tokens(),
                generated: 0,
                max_tokens: request.max_tokens,
                pending: Vec::new(),
                out: String::new(),
                done: request.max_tokens == 0,
            });
            batch
                .add(prompt[prompt.len() - 1], pos, &[seq_id as i32], true)
                .map_err(engine)?;
        }
        decode(&mut ctx, &mut batch)?;
        for (seq, s) in seqs.iter().enumerate() {
            if s.done {
                on_event(GenEvent::Finished { seq });
            }
        }

        let vocab = self.model.vocab();
        loop {
            batch.clear();
            for (seq_id, s) in seqs.iter_mut().enumerate() {
                if s.done {
                    continue;
                }
                if cancel.load(Ordering::Relaxed) {
                    return Err(LlmError::Cancelled);
                }
                let token = s.sampler.sample(&ctx, s.logits);
                if !vocab.is_eog(token) {
                    s.generated += 1;
                    let piece = vocab.token_to_piece(token, false, None);
                    s.pending.extend_from_slice(&piece);
                    let text = s.take_text(false);
                    if !text.is_empty() {
                        on_event(GenEvent::Token { seq: seq_id, text });
                    }
                }
                if vocab.is_eog(token) || s.generated >= s.max_tokens {
                    s.done = true;
                    let text = s.take_text(true);
                    if !text.is_empty() {
                        on_event(GenEvent::Token { seq: seq_id, text });
                    }
                    on_event(GenEvent::Finished { seq: seq_id });
                    continue;
                }
                s.logits = batch.n_tokens();
                batch
                    .add(token, s.pos, &[seq_id as i32], true)
                    .map_err(engine)?;
                s.pos += 1;
            }
            if batch.n_tokens() == 0 {
                break;
            }
            decode(&mut ctx, &mut batch)?;
        }
        Ok(seqs.into_iter().map(|s| s.out).collect())
    }
}
