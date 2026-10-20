//! The on-device LLM behind the [`Summarizer`] trait (KTD9), so the job,
//! queue and cancel logic can be tested with a fake engine.

pub mod prompt;

#[cfg(not(feature = "fake-engines"))]
pub mod llama;

use std::{
    fmt,
    sync::{atomic::AtomicBool, Arc},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// The cancel token was set; no output is kept.
    Cancelled,
    /// The model isn't downloaded and verified yet.
    NotReady,
    Engine(String),
}

impl fmt::Display for LlmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::NotReady => f.write_str("The summary model isn't downloaded yet."),
            Self::Engine(e) => write!(f, "summary model error: {e}"),
        }
    }
}

impl std::error::Error for LlmError {}

pub type Result<T> = std::result::Result<T, LlmError>;

/// What [`Summarizer::generate`] reports while it decodes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenEvent<'a> {
    /// Sequence `seq` produced `text` (one token's worth of UTF-8).
    Token { seq: usize, text: &'a str },
    /// Sequence `seq` hit end-of-turn or its `max_tokens`.
    Finished { seq: usize },
}

/// One completion request in a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    /// The user turn; the engine wraps it in the model's chat template.
    pub prompt: String,
    /// Stop after this many generated tokens even without end-of-turn.
    pub max_tokens: usize,
}

pub trait Summarizer: Send + Sync {
    /// Tokens per sequence the context holds (prompt plus output).
    fn context_tokens(&self) -> usize;

    /// Tokens in `text` on its own, as the model's tokenizer counts them.
    fn count_tokens(&self, text: &str) -> Result<usize>;

    /// Tokens `prompt` occupies once wrapped in the chat template.
    fn prompt_tokens(&self, prompt: &str) -> Result<usize>;

    /// Decodes every request as a parallel sequence in one batch and returns
    /// the outputs in request order, reporting tokens and finished sequences
    /// through `on_event`. Checks `cancel` on every token and returns `Cancelled` once set.
    fn generate(
        &self,
        requests: &[Generation],
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(GenEvent<'_>),
    ) -> Result<Vec<String>>;
}

/// Loads the summarizer on demand, so the model only takes memory once a
/// job needs it (KTD5).
pub trait SummarizerLoader: Send + Sync {
    /// The model id stored on sessions summarized with this engine.
    fn model_id(&self) -> &str;

    fn load(&self) -> Result<Arc<dyn Summarizer>>;
}
