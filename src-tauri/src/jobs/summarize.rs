//! One summarization run (KTD3): token-budgeted chunks decoded as parallel
//! sequences in one batch, then one combine pass; a single chunk is the
//! summary itself. Steps and messages match Electron's `summarize.js`.

use std::{
    sync::{atomic::AtomicBool, Arc},
    time::Instant,
};

use super::{queue::Job, queue::Shared, SessionProgress, SummaryDelta};
use crate::llm::prompt::{chunk_budget, chunk_prompt, combine_prompt, CHUNK_NOTE_TOKENS};
use crate::llm::{GenEvent, Generation, LlmError, Summarizer};
use crate::store::{SessionStatus, StoreError, Timings, ACTIVE_PROMPT_ID};
use crate::text::{chunk_by_tokens, ChunkError, MIN_WORDS};

use super::state::can_transition;

/// Why a run ended without a summary.
enum Stop {
    Cancelled,
    Failed(String),
}

impl From<LlmError> for Stop {
    fn from(e: LlmError) -> Self {
        match e {
            LlmError::Cancelled => Self::Cancelled,
            e => Self::Failed(e.to_string()),
        }
    }
}

impl From<StoreError> for Stop {
    fn from(e: StoreError) -> Self {
        Self::Failed(e.to_string())
    }
}

impl From<ChunkError> for Stop {
    fn from(e: ChunkError) -> Self {
        Self::Failed(e.to_string())
    }
}

/// Writes the status row, then emits the matching event.
struct Reporter<'a> {
    shared: &'a Shared,
    session_id: &'a str,
    step_index: u32,
    step_count: u32,
}

impl Reporter<'_> {
    fn emit(&mut self, state: SessionStatus, step_index: u32, step_count: u32, message: &str) {
        self.step_index = step_index;
        self.step_count = step_count;
        self.shared.events.progress(SessionProgress {
            session_id: self.session_id.into(),
            state,
            step_index,
            step_count,
            message: message.into(),
        });
    }

    fn step(&mut self, step_index: u32, step_count: u32, message: &str) {
        self.emit(SessionStatus::Summarizing, step_index, step_count, message);
    }

    fn delta(&self, text: &str) {
        self.shared.events.summary_delta(SummaryDelta {
            session_id: self.session_id.into(),
            text: text.into(),
        });
    }

    /// Records the end state; the stored summary is left as it was.
    fn stop(&mut self, stop: Stop) {
        let (status, error, message) = match &stop {
            Stop::Cancelled => (SessionStatus::Cancelled, None, "Summarization stopped."),
            Stop::Failed(e) => (SessionStatus::Failed, Some(e.as_str()), e.as_str()),
        };
        if let Err(e) = self.shared.store.set_status(self.session_id, status, error) {
            eprintln!(
                "summarize {}: saving {status:?} failed: {e}",
                self.session_id
            );
        }
        let (step_index, step_count) = (self.step_index, self.step_count);
        self.emit(status, step_index, step_count, message);
    }
}

fn ms(since: Instant) -> u32 {
    u32::try_from(since.elapsed().as_millis()).unwrap_or(u32::MAX)
}

pub(super) fn run(
    shared: &Shared,
    job: &Job,
    cancel: &AtomicBool,
    llm: &mut Option<Arc<dyn Summarizer>>,
) {
    let mut reporter = Reporter {
        shared,
        session_id: &job.session_id,
        step_index: 1,
        step_count: 1,
    };
    match summarize(&mut reporter, job, cancel, llm) {
        Ok(()) | Err(None) => {}
        Err(Some(stop)) => reporter.stop(stop),
    }
}

/// Marks the session failed after the run panicked.
pub(super) fn fail_after_panic(shared: &Shared, job: &Job) {
    let mut reporter = Reporter {
        shared,
        session_id: &job.session_id,
        step_index: 1,
        step_count: 1,
    };
    reporter.stop(Stop::Failed("Summarization crashed.".into()));
}

/// `Err(None)` means the job was skipped without touching the session.
fn summarize(
    r: &mut Reporter<'_>,
    job: &Job,
    cancel: &AtomicBool,
    llm: &mut Option<Arc<dyn Summarizer>>,
) -> Result<(), Option<Stop>> {
    let store = &r.shared.store;
    let Some(session) = store.get_session(&job.session_id).map_err(stop)? else {
        return Err(None);
    };
    // The session was deleted, or moved on, while the job waited.
    if !can_transition(session.status, SessionStatus::Summarizing) {
        return Err(None);
    }
    // A prompt deleted while waiting falls back to the active one, as in Electron.
    let prompt = match store.get_prompt(&job.prompt_id).map_err(stop)? {
        Some(p) => p,
        None => store
            .get_setting(ACTIVE_PROMPT_ID)
            .map_err(stop)?
            .and_then(|id| store.get_prompt(&id).transpose())
            .transpose()
            .map_err(stop)?
            .ok_or_else(|| Stop::Failed(format!("prompt {} not found", job.prompt_id)))?,
    };
    store
        .set_status(&job.session_id, SessionStatus::Summarizing, None)
        .map_err(stop)?;

    // Electron's word floor, checked before the model takes any memory.
    if session.transcript.split(' ').count() < MIN_WORDS {
        return Err(Some(ChunkError::InsufficientAudio.into()));
    }

    // Transcription happened before the job; its time carries over.
    let mut timings = Timings {
        tail_transcription_ms: session
            .timings
            .as_ref()
            .and_then(|t| t.tail_transcription_ms),
        ..Timings::default()
    };
    r.step(1, 1, "Loading summary model...");
    let model = match llm {
        Some(model) => {
            timings.model_load_ms = Some(0);
            Arc::clone(model)
        }
        None => {
            let started = Instant::now();
            let model = r.shared.loader.load().map_err(stop)?;
            timings.model_load_ms = Some(ms(started));
            *llm = Some(Arc::clone(&model));
            model
        }
    };
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(Some(Stop::Cancelled));
    }

    let base = &prompt.text;
    let context = model.context_tokens();
    let budget = chunk_budget(
        context,
        model.prompt_tokens(&chunk_prompt(base, "")).map_err(stop)?,
    )
    .ok_or_else(|| Stop::Failed("The summary prompt is too long for the model.".into()))?;
    let mut count_error = None;
    let chunks = chunk_by_tokens(&session.transcript, budget, |text| {
        model.count_tokens(text).unwrap_or_else(|e| {
            count_error.get_or_insert(e);
            0
        })
    });
    if let Some(e) = count_error {
        return Err(Some(e.into()));
    }
    let chunks = chunks.map_err(stop)?;
    let n = chunks.len();

    let summary = if n == 1 {
        r.step(2, 2, "Generating summary...");
        let started = Instant::now();
        let summary = final_pass(r, &*model, chunk_prompt(base, &chunks[0]), cancel)?;
        timings.chunk_ms.push(ms(started));
        summary
    } else {
        let count = n as u32 + 2;
        r.step(2, count, &format!("Summarizing chunk 1 of {n}..."));
        let requests: Vec<Generation> = chunks
            .iter()
            .map(|c| Generation {
                prompt: chunk_prompt(base, c),
                max_tokens: CHUNK_NOTE_TOKENS,
            })
            .collect();
        let started = Instant::now();
        let mut chunk_ms = vec![0; n];
        let mut finished = 0;
        let notes = model
            .generate(&requests, cancel, &mut |event| {
                if let GenEvent::Finished { seq } = event {
                    chunk_ms[seq] = ms(started);
                    finished += 1;
                    if finished < n {
                        let next = finished + 1;
                        r.step(
                            next as u32 + 1,
                            count,
                            &format!("Summarizing chunk {next} of {n}..."),
                        );
                    }
                }
            })
            .map_err(stop)?;
        timings.chunk_ms = chunk_ms;

        r.step(count, count, "Combining chunk summaries...");
        let started = Instant::now();
        let summary = final_pass(r, &*model, combine_prompt(base, &notes), cancel)?;
        timings.combine_ms = Some(ms(started));
        summary
    };

    let started = Instant::now();
    store
        .finish_summary(
            &job.session_id,
            &summary,
            &prompt.id,
            r.shared.loader.model_id(),
            &timings,
        )
        .map_err(stop)?;
    timings.save_ms = Some(ms(started));
    if let Err(e) = store.set_timings(&job.session_id, &timings) {
        eprintln!("summarize {}: saving timings failed: {e}", job.session_id);
    }
    let (step, count) = (r.step_count, r.step_count);
    r.emit(SessionStatus::Done, step, count, "Session ready.");
    Ok(())
}

fn stop(e: impl Into<Stop>) -> Option<Stop> {
    Some(e.into())
}

/// Decodes the pass whose output is the summary, streaming it as it comes,
/// with whatever room the context has left.
fn final_pass(
    r: &Reporter<'_>,
    model: &dyn Summarizer,
    prompt: String,
    cancel: &AtomicBool,
) -> Result<String, Option<Stop>> {
    let used = model.prompt_tokens(&prompt).map_err(stop)?;
    let max_tokens = model
        .context_tokens()
        .checked_sub(used)
        .filter(|&m| m > 0)
        .ok_or_else(|| Stop::Failed("The transcript is too long to summarize.".into()))?;
    let mut out = model
        .generate(&[Generation { prompt, max_tokens }], cancel, &mut |event| {
            if let GenEvent::Token { text, .. } = event {
                r.delta(text);
            }
        })
        .map_err(stop)?;
    Ok(out.remove(0))
}
