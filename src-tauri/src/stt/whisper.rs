//! The real transcriber: whisper-base through whisper.cpp with Metal.
//!
//! whisper.cpp and its ggml link statically into the main binary
//! (`plans/DEPENDENCY_REVIEW_ENGINES.md`, fallback A). Settings match the
//! `whisper-cli --suppress-nst` run the Electron app spawned.

use std::{
    ffi::c_void,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::{Result, SttError, Transcriber, TranscriberLoader, TranscriptSegment};
use crate::models::{ModelManager, ModelState, WHISPER_BASE};

/// whisper-cli defaults: beam search of 5, English, at most 4 threads.
const BEAM_SIZE: i32 = 5;
const LANGUAGE: &str = "en";
const MAX_THREADS: usize = 4;

fn engine(e: impl std::fmt::Display) -> SttError {
    SttError::Engine(e.to_string())
}

pub struct WhisperLoader {
    models: Arc<ModelManager>,
}

impl WhisperLoader {
    pub fn new(models: Arc<ModelManager>) -> Self {
        Self { models }
    }
}

impl TranscriberLoader for WhisperLoader {
    fn load(&self) -> Result<Arc<dyn Transcriber>> {
        if self.models.readiness.get(WHISPER_BASE) != ModelState::Ready {
            return Err(SttError::NotReady);
        }
        let path = self.models.path(WHISPER_BASE).ok_or(SttError::NotReady)?;
        Ok(Arc::new(WhisperTranscriber::load(&path)?))
    }
}

pub struct WhisperTranscriber {
    ctx: WhisperContext,
}

impl WhisperTranscriber {
    /// Loads a ggml whisper model on the GPU.
    pub fn load(path: &Path) -> Result<Self> {
        // Without a `log` backend feature this drops whisper.cpp's and its
        // ggml's logs instead of printing them to stderr.
        whisper_rs::install_logging_hooks();
        let ctx = WhisperContext::new_with_params(path, WhisperContextParameters::default())
            .map_err(engine)?;
        Ok(Self { ctx })
    }
}

/// whisper.cpp polls this between graph computations while `full` runs.
unsafe extern "C" fn aborted(cancel: *mut c_void) -> bool {
    // SAFETY: `transcribe` points this at its `cancel` borrow, which outlives
    // the `full` call that is the only caller.
    unsafe { &*(cancel as *const AtomicBool) }.load(Ordering::Relaxed)
}

impl Transcriber for WhisperTranscriber {
    fn transcribe(&self, samples: &[f32], cancel: &AtomicBool) -> Result<Vec<TranscriptSegment>> {
        if cancel.load(Ordering::Relaxed) {
            return Err(SttError::Cancelled);
        }
        if samples.is_empty() {
            return Ok(Vec::new());
        }
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: BEAM_SIZE,
            patience: -1.0,
        });
        let threads = std::thread::available_parallelism().map_or(MAX_THREADS, |n| n.get());
        params.set_n_threads(threads.min(MAX_THREADS) as i32);
        params.set_language(Some(LANGUAGE));
        params.set_suppress_nst(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_print_special(false);
        // SAFETY: `aborted` only reads the AtomicBool, and `cancel` outlives
        // `params`, which is consumed by the `full` call below.
        unsafe {
            params.set_abort_callback(Some(aborted));
            params.set_abort_callback_user_data(cancel as *const AtomicBool as *mut c_void);
        }

        let mut state = self.ctx.create_state().map_err(engine)?;
        let ran = state.full(params, samples);
        if cancel.load(Ordering::Relaxed) {
            return Err(SttError::Cancelled);
        }
        ran.map_err(engine)?;

        state
            .as_iter()
            .map(|s| {
                // whisper reports centiseconds.
                Ok(TranscriptSegment {
                    start_ms: s.start_timestamp() * 10,
                    end_ms: s.end_timestamp() * 10,
                    text: s.to_str_lossy().map_err(engine)?.trim().to_string(),
                })
            })
            .collect()
    }
}
