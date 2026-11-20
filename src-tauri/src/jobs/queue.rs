//! One worker thread draining a FIFO of summarization jobs, so two
//! summaries never compete for the model or for memory.

use std::{
    collections::VecDeque,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, MutexGuard,
    },
    thread::JoinHandle,
    time::Duration,
};

use super::{state::can_transition, summarize, JobEvents, SessionProgress};
use crate::coach::{
    practice::{self, PracticeNote, PracticeSource, MAX_ATTEMPTS, NOTE_TOKENS},
    StyleSpec,
};
use crate::llm::{LlmError, Summarizer, SummarizerLoader};
use crate::store::{SessionStatus, Store, StoreError};

#[derive(Debug)]
pub enum JobError {
    Store(StoreError),
    SessionNotFound(String),
    PromptNotFound(String),
    /// The session is recording or already summarizing.
    Busy(SessionStatus),
    CannotDelete(SessionStatus),
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Store(e) => e.fmt(f),
            Self::SessionNotFound(id) => write!(f, "session {id} not found"),
            Self::PromptNotFound(id) => write!(f, "prompt {id} not found"),
            Self::Busy(status) => {
                write!(f, "session can't be summarized while {}", status.as_str())
            }
            Self::CannotDelete(status) => {
                write!(f, "session can't be deleted while {}", status.as_str())
            }
        }
    }
}

impl std::error::Error for JobError {}

impl From<StoreError> for JobError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

pub(super) struct Job {
    pub session_id: String,
    pub prompt_id: String,
}

struct Running {
    session_id: String,
    cancel: Arc<AtomicBool>,
    cancelled_by_user: bool,
}

type PracticeResult = Result<PracticeNote, &'static str>;

pub(crate) struct PracticeJob {
    pub spec: StyleSpec,
    pub instructions: String,
    pub source: PracticeSource<'static>,
    pub cancel: Arc<AtomicBool>,
    pub progress: Box<dyn Fn(u8) + Send>,
    pub complete: Box<dyn FnOnce(PracticeResult) + Send>,
}

pub(crate) struct SessionCoachJob {
    pub prompt: String,
    pub cancel: Arc<AtomicBool>,
    pub complete: Box<dyn FnOnce(Result<String, &'static str>) + Send>,
}

struct PracticeRun {
    job: PracticeJob,
    attempts: u8,
    instructions: String,
    best: Option<PracticeNote>,
}

impl PracticeRun {
    fn attempt(&mut self, model: &dyn Summarizer) -> Result<bool, summarize::Stop> {
        self.attempts += 1;
        (self.job.progress)(self.attempts);
        let text = summarize::generate(
            model,
            &self.instructions,
            self.job.source.transcript,
            NOTE_TOKENS,
            &self.job.cancel,
            &mut Default::default(),
            &mut |_| {},
        )?;
        if self.job.cancel.load(Ordering::Relaxed) {
            return Err(summarize::Stop::Cancelled);
        }
        let empty = text.trim().is_empty();
        let candidate = practice::check(&self.job.spec, &self.job.source, text);
        let done = candidate.failed.is_empty() || self.attempts == MAX_ATTEMPTS;
        self.instructions = practice::corrections(&self.job.instructions, &candidate.failed);
        if !empty
            && self
                .best
                .as_ref()
                .is_none_or(|best| best.passed < candidate.passed)
        {
            self.best = Some(candidate);
        }
        Ok(done)
    }
}

const PRACTICE_FAILED: &str = "I couldn’t write the practice note. Try again.";

#[derive(Default)]
struct State {
    pending: VecDeque<Job>,
    running: Option<Running>,
    practice: Option<PracticeRun>,
    running_practice: Option<Arc<AtomicBool>>,
    session_coach: Option<SessionCoachJob>,
    running_session_coach: Option<Arc<AtomicBool>>,
    /// Load the model now, ahead of the job a finishing recording queues.
    preload: bool,
    /// A Whisper load reserved the queue from starting a summary.
    whisper_loading: bool,
    shutdown: bool,
}

pub(super) struct Shared {
    pub store: Arc<Store>,
    pub loader: Arc<dyn SummarizerLoader>,
    pub events: Arc<dyn JobEvents>,
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }
}

struct TranscriptionLoad<'a>(&'a Shared);

impl Drop for TranscriptionLoad<'_> {
    fn drop(&mut self) {
        self.0.lock().whisper_loading = false;
        self.0.changed.notify_all();
    }
}

pub struct JobQueue {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl JobQueue {
    pub fn new(
        store: Arc<Store>,
        loader: Arc<dyn SummarizerLoader>,
        events: Arc<dyn JobEvents>,
    ) -> Self {
        let shared = Arc::new(Shared {
            store,
            loader,
            events,
            state: Mutex::default(),
            changed: Condvar::new(),
        });
        match shared.store.queued_summaries() {
            Ok(queued) => {
                shared
                    .lock()
                    .pending
                    .extend(queued.into_iter().map(|(session_id, prompt_id)| Job {
                        session_id,
                        prompt_id,
                    }))
            }
            Err(e) => crate::diag::log::log(
                "ERROR",
                "jobs",
                &format!(
                    "restoring summary queue failed ({})",
                    crate::diag::log::kind(&e)
                ),
            ),
        }
        let worker = {
            let shared = Arc::clone(&shared);
            std::thread::Builder::new()
                .name("summarize".into())
                .spawn(move || work(&shared))
                .expect("spawn summarize worker")
        };
        Self {
            shared,
            worker: Some(worker),
        }
    }

    /// Queues a fresh summary of `session_id` with `prompt_id`. A session
    /// already waiting keeps its place and takes the new prompt.
    pub fn regenerate(&self, session_id: &str, prompt_id: &str) -> Result<(), JobError> {
        let store = &self.shared.store;
        if store.get_prompt(prompt_id)?.is_none() {
            return Err(JobError::PromptNotFound(prompt_id.into()));
        }
        let mut state = self.shared.lock();
        let session = store
            .get_session(session_id)?
            .ok_or_else(|| JobError::SessionNotFound(session_id.into()))?;
        if state
            .running
            .as_ref()
            .is_some_and(|r| r.session_id == session_id && r.cancelled_by_user)
            || !can_transition(session.status, SessionStatus::Summarizing)
        {
            return Err(JobError::Busy(session.status));
        }
        let waiting = !self.shared.loader.ready();
        let queued_status = if waiting {
            SessionStatus::AwaitingModel
        } else {
            session.status
        };
        if waiting && session.status != queued_status {
            debug_assert!(can_transition(session.status, queued_status));
        }
        let already_pending = state.pending.iter().any(|j| j.session_id == session_id);
        self.shared.store.queue_status(
            session_id,
            prompt_id,
            queued_status,
            session.status,
            already_pending,
        )?;
        if let Some(job) = state.session_coach.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &state.running_session_coach {
            cancel.store(true, Ordering::Relaxed);
        }
        match state
            .pending
            .iter_mut()
            .find(|j| j.session_id == session_id)
        {
            Some(job) => job.prompt_id = prompt_id.into(),
            None => state.pending.push_back(Job {
                session_id: session_id.into(),
                prompt_id: prompt_id.into(),
            }),
        }
        drop(state);
        self.shared.events.session_coach_cancelled();
        self.shared.events.summary_queue_changed();
        if waiting {
            self.shared.events.progress(SessionProgress {
                session_id: session_id.into(),
                state: SessionStatus::AwaitingModel,
                step_index: 1,
                step_count: 1,
                message: "Waiting for the summary model download…".into(),
            });
        }
        self.shared.changed.notify_all();
        Ok(())
    }

    pub(crate) fn enqueue_practice(&self, job: PracticeJob) -> Result<(), &'static str> {
        let mut state = self.shared.lock();
        if state.running.is_some() || !state.pending.is_empty() {
            return Err(crate::coach::CoachBlock::Busy.message());
        }
        if state.session_coach.is_some() || state.running_session_coach.is_some() {
            return Err(crate::coach::CoachBlock::Busy.message());
        }
        if state.practice.is_some() || state.running_practice.is_some() {
            return Err("The previous practice note is still stopping. Try again.");
        }
        if state.shutdown || !self.shared.loader.ready() {
            return Err(PRACTICE_FAILED);
        }
        state.practice = Some(PracticeRun {
            instructions: job.instructions.clone(),
            job,
            attempts: 0,
            best: None,
        });
        self.shared.changed.notify_all();
        Ok(())
    }

    pub(crate) fn enqueue_session_coach(&self, job: SessionCoachJob) -> Result<(), &'static str> {
        let mut state = self.shared.lock();
        if state.shutdown || state.running.is_some() || !state.pending.is_empty() {
            return Err(crate::coach::CoachBlock::Busy.message());
        }
        if state.practice.is_some()
            || state.running_practice.is_some()
            || state.session_coach.is_some()
            || state.running_session_coach.is_some()
        {
            return Err("The coach is still working. Try again.");
        }
        if !self.shared.loader.ready() {
            return Err("The coach isn’t available right now.");
        }
        state.session_coach = Some(job);
        self.shared.changed.notify_all();
        Ok(())
    }

    pub(crate) fn cancel_session_coach(&self) {
        let mut state = self.shared.lock();
        if let Some(job) = state.session_coach.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &state.running_session_coach {
            cancel.store(true, Ordering::Relaxed);
        }
        self.shared.changed.notify_all();
    }

    pub(crate) fn cancel_practice(&self) {
        let mut state = self.shared.lock();
        if let Some(run) = state.practice.take() {
            run.job.cancel.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &state.running_practice {
            cancel.store(true, Ordering::Relaxed);
        }
        self.shared.changed.notify_all();
    }

    /// Whether a summary is running now; queued jobs don't count.
    pub fn summarizing(&self) -> bool {
        self.shared.lock().running.is_some()
    }

    pub fn is_busy(&self) -> bool {
        let state = self.shared.lock();
        !state.pending.is_empty()
            || state.running.is_some()
            || state.session_coach.is_some()
            || state.running_session_coach.is_some()
    }

    pub(crate) fn while_real_idle<T>(&self, operation: impl FnOnce() -> T) -> Option<T> {
        let state = self.shared.lock();
        if state.running.is_some() || !state.pending.is_empty() {
            return None;
        }
        Some(operation())
    }

    /// Serializes model changes with summary admission and start. Returns
    /// `None` while a summary is running. The closure runs under the queue
    /// lock and must not call back into this queue.
    pub fn while_idle<T>(&self, operation: impl FnOnce() -> T) -> Option<T> {
        let state = self.shared.lock();
        if state.running.is_some()
            || state.running_practice.is_some()
            || state.session_coach.is_some()
            || state.running_session_coach.is_some()
        {
            return None;
        }
        Some(operation())
    }

    /// Waits for active or ready summaries before loading Whisper, while
    /// allowing transcription during a summary-model download. Reserves summary
    /// start during Whisper loading without holding the queue lock.
    pub fn while_transcription_safe<T>(
        &self,
        cancel: &AtomicBool,
        on_wait: impl FnOnce(),
        operation: impl FnOnce() -> T,
    ) -> Option<T> {
        let mut state = self.shared.lock();
        let summary_waiting =
            state.running.is_some() || (!state.pending.is_empty() && self.shared.loader.ready());
        let waiting = summary_waiting
            || state.whisper_loading
            || state.running_practice.is_some()
            || state.running_session_coach.is_some();
        if waiting {
            // Lock order: release the queue before callbacks that may lock recording or emit events.
            drop(state);
            if summary_waiting {
                on_wait();
            }
            state = self.shared.lock();
        }
        while !cancel.load(Ordering::Relaxed)
            && (state.running.is_some()
                || (!state.pending.is_empty() && self.shared.loader.ready())
                || state.whisper_loading
                || state.running_practice.is_some()
                || state.running_session_coach.is_some())
        {
            state = self
                .shared
                .changed
                .wait_timeout(state, Duration::from_millis(100))
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        state.whisper_loading = true;
        drop(state);
        let _loading = TranscriptionLoad(&self.shared);
        Some(operation())
    }

    /// Starts loading the summary model in the background, so a recording's
    /// summary doesn't wait for it after the transcript is done (M5b).
    pub fn preload(&self) {
        if self.shared.loader.ready() {
            self.shared.lock().preload = true;
        }
        self.shared.changed.notify_all();
    }

    /// Drops a waiting job or stops the running one. Returns whether
    /// `session_id` had a job. A dropped job's session keeps its status,
    /// except one still `Finalizing` from its recording, which would
    /// otherwise never leave it.
    pub fn cancel(&self, session_id: &str) -> bool {
        let mut state = self.shared.lock();
        if let Some(i) = state
            .pending
            .iter()
            .position(|j| j.session_id == session_id)
        {
            state.pending.remove(i);
            self.cancel_finalizing(session_id);
            drop(state);
            self.shared.events.summary_queue_changed();
            self.shared.changed.notify_all();
            return true;
        }
        match &mut state.running {
            Some(r) if r.session_id == session_id => {
                if let Err(e) = self.shared.store.cancel_running_summary(session_id) {
                    crate::diag::log::log(
                        "ERROR",
                        "jobs",
                        &format!(
                            "persisting cancellation for {session_id} failed ({})",
                            crate::diag::log::kind(&e)
                        ),
                    );
                    return false;
                }
                r.cancelled_by_user = true;
                r.cancel.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    pub fn delete_session_safe(&self, session_id: &str) -> Result<bool, JobError> {
        let Some(session) = self.shared.store.get_session(session_id)? else {
            return Ok(false);
        };
        if matches!(
            session.status,
            SessionStatus::Recording | SessionStatus::Finalizing
        ) {
            return Err(JobError::CannotDelete(session.status));
        }
        self.cancel(session_id);
        Ok(self.shared.store.delete_session(session_id)?)
    }

    pub fn clear_all_sessions(&self) -> Result<(usize, usize), JobError> {
        let sessions = self.shared.store.list_sessions()?;
        let mut deleted = 0;
        let mut skipped = 0;
        for session in sessions {
            if matches!(
                session.status,
                SessionStatus::Recording | SessionStatus::Finalizing
            ) {
                skipped += 1;
                continue;
            }
            self.cancel(&session.id);
            match self.shared.store.delete_session(&session.id) {
                Ok(true) => deleted += 1,
                Ok(false) => skipped += 1,
                Err(e) => {
                    crate::diag::log::log(
                        "ERROR",
                        "queue",
                        &format!(
                            "clear sessions: deleting {} failed ({})",
                            session.id,
                            crate::diag::log::kind(&e)
                        ),
                    );
                    skipped += 1;
                }
            }
        }
        self.shared.store.vacuum()?;
        Ok((deleted, skipped))
    }

    fn cancel_finalizing(&self, session_id: &str) {
        let store = &self.shared.store;
        let status = match store.cancel_queued_summary(session_id) {
            Ok(Some(status)) => status,
            Ok(None) => return,
            Err(e) => {
                crate::diag::log::log(
                    "ERROR",
                    "jobs",
                    &format!(
                        "removing queued summary {session_id} failed ({})",
                        crate::diag::log::kind(&e)
                    ),
                );
                return;
            }
        };
        if status != SessionStatus::Cancelled {
            return;
        }
        self.shared.events.progress(SessionProgress {
            session_id: session_id.into(),
            state: SessionStatus::Cancelled,
            step_index: 1,
            step_count: 1,
            message: "Summarization stopped.".into(),
        });
    }

    /// Blocks until nothing is queued or running, or `timeout` passes.
    /// Returns whether the queue went idle.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let state = self.shared.lock();
        let (state, _) = self
            .shared
            .changed
            .wait_timeout_while(state, timeout, |s| {
                !s.pending.is_empty()
                    || s.running.is_some()
                    || s.session_coach.is_some()
                    || s.running_session_coach.is_some()
            })
            .unwrap_or_else(|p| p.into_inner());
        state.pending.is_empty()
            && state.running.is_none()
            && state.session_coach.is_none()
            && state.running_session_coach.is_none()
    }
}

impl Drop for JobQueue {
    fn drop(&mut self) {
        self.cancel_practice();
        self.cancel_session_coach();
        {
            let mut state = self.shared.lock();
            state.shutdown = true;
            if let Some(r) = &state.running {
                r.cancel.store(true, Ordering::Relaxed);
            }
            self.shared.changed.notify_all();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn work(shared: &Shared) {
    // The model stays loaded between jobs (KTD5), keyed by its id so a
    // switch of summary model loads the new one (spec 03 R20).
    let mut llm: Option<(String, Arc<dyn Summarizer>)> = None;
    loop {
        let (job, cancel) = {
            let state = shared.lock();
            let (mut state, _) = shared
                .changed
                .wait_timeout_while(state, Duration::from_millis(200), |s| {
                    (s.whisper_loading
                        || (((s.pending.is_empty()
                            && s.practice.is_none()
                            && s.session_coach.is_none())
                            || !shared.loader.ready())
                            && !s.preload))
                        && !s.shutdown
                })
                .unwrap_or_else(|p| p.into_inner());
            if state.shutdown {
                return;
            }
            if std::mem::take(&mut state.preload) {
                drop(state);
                let id = shared.loader.model_id();
                if llm.as_ref().is_none_or(|(loaded, _)| *loaded != id) && shared.loader.ready() {
                    llm = None;
                    match crate::diag::panic::catch_unwind(|| shared.loader.load(&id)) {
                        Ok(Ok(model)) => llm = Some((id, model)),
                        // The job loads it again and reports the error.
                        Ok(Err(e)) => crate::diag::log::log(
                            "ERROR",
                            "jobs",
                            &format!(
                                "preloading summary model failed ({})",
                                crate::diag::log::kind(&e)
                            ),
                        ),
                        Err(_) => crate::diag::log::log(
                            "ERROR",
                            "jobs",
                            "preloading summary model panicked",
                        ),
                    }
                }
                continue;
            }
            if state.whisper_loading || !shared.loader.ready() {
                continue;
            }
            if state.pending.is_empty() {
                if let Some(run) = state.practice.take() {
                    state.running_practice = Some(Arc::clone(&run.job.cancel));
                    drop(state);
                    run_practice(shared, run, &mut llm);
                } else if let Some(job) = state.session_coach.take() {
                    state.running_session_coach = Some(Arc::clone(&job.cancel));
                    drop(state);
                    run_session_coach(shared, job, &mut llm);
                }
                continue;
            }
            let job = state.pending.pop_front().expect("pending is non-empty");
            let cancel = Arc::new(AtomicBool::new(false));
            state.running = Some(Running {
                session_id: job.session_id.clone(),
                cancel: Arc::clone(&cancel),
                cancelled_by_user: false,
            });
            (job, cancel)
        };
        let ran =
            crate::diag::panic::catch_unwind(|| summarize::run(shared, &job, &cancel, &mut llm));
        if ran.is_err() {
            // The engine may be in a bad state; load it again next time.
            llm = None;
            summarize::fail_after_panic(shared, &job);
        }
        let mut state = shared.lock();
        let requeued = state.pending.iter().any(|j| j.session_id == job.session_id);
        let interrupted_by_shutdown = !requeued
            && state.shutdown
            && state.running.as_ref().is_some_and(|r| !r.cancelled_by_user)
            && shared
                .store
                .get_session(&job.session_id)
                .ok()
                .flatten()
                .is_some_and(|s| s.status == SessionStatus::Cancelled);
        if interrupted_by_shutdown {
            if let Err(e) =
                shared
                    .store
                    .set_status(&job.session_id, SessionStatus::Summarizing, None)
            {
                crate::diag::log::log(
                    "ERROR",
                    "jobs",
                    &format!(
                        "preserving interrupted job {} failed ({})",
                        job.session_id,
                        crate::diag::log::kind(&e)
                    ),
                );
            }
        } else if !requeued {
            if let Err(e) = shared.store.dequeue_summary(&job.session_id) {
                crate::diag::log::log(
                    "ERROR",
                    "jobs",
                    &format!(
                        "removing persisted job {} failed ({})",
                        job.session_id,
                        crate::diag::log::kind(&e)
                    ),
                );
            }
        }
        state.running = None;
        drop(state);
        shared.events.summary_queue_changed();
        shared.changed.notify_all();
    }
}

fn run_session_coach(
    shared: &Shared,
    job: SessionCoachJob,
    llm: &mut Option<(String, Arc<dyn Summarizer>)>,
) {
    let result = crate::diag::panic::catch_unwind(|| {
        if job.cancel.load(Ordering::Relaxed) {
            return Err("The coach stopped. Try again.");
        }
        let id = shared.loader.model_id();
        if llm.as_ref().is_none_or(|(loaded, _)| *loaded != id) {
            *llm = None;
            *llm = Some((
                id.clone(),
                shared
                    .loader
                    .load(&id)
                    .map_err(|_| "The coach couldn’t load. Try again.")?,
            ));
        }
        let model = llm.as_ref().unwrap().1.as_ref();
        // shortcut: an oversized session is rejected; chunk it once real-model sessions require it.
        if model
            .prompt_tokens(&job.prompt)
            .map_err(|_| "The coach couldn’t read this note.")?
            + 512
            > model.context_tokens()
        {
            return Err("This session is too long for the coach.");
        }
        let output = model
            .generate(
                &[crate::llm::Generation {
                    prompt: job.prompt.clone(),
                    max_tokens: 512,
                }],
                &job.cancel,
                &mut |_| {},
            )
            .map_err(|_| "The coach couldn’t revise the note. Try again.")?;
        output
            .into_iter()
            .next()
            .ok_or("The coach couldn’t revise the note. Try again.")
    });
    if result.is_err() {
        *llm = None;
    }
    let shutdown = shared.lock().shutdown;
    if !job.cancel.load(Ordering::Relaxed) && !shutdown {
        (job.complete)(result.unwrap_or(Err("The coach couldn’t revise the note. Try again.")));
    }
    shared.lock().running_session_coach = None;
    shared.events.summary_queue_changed();
    shared.changed.notify_all();
}

fn run_practice(
    shared: &Shared,
    mut run: PracticeRun,
    llm: &mut Option<(String, Arc<dyn Summarizer>)>,
) {
    let result = crate::diag::panic::catch_unwind(|| {
        if run.job.cancel.load(Ordering::Relaxed) {
            return Err(summarize::Stop::Cancelled);
        }
        let id = shared.loader.model_id();
        if llm.as_ref().is_none_or(|(loaded, _)| *loaded != id) {
            *llm = None;
            let model = shared.loader.load(&id).map_err(|e| match e {
                LlmError::Cancelled => summarize::Stop::Cancelled,
                _ => summarize::Stop::Failed(PRACTICE_FAILED.into()),
            })?;
            *llm = Some((id, model));
        }
        run.attempt(llm.as_ref().unwrap().1.as_ref())
    });
    let mut state = shared.lock();
    state.running_practice = None;
    shared.changed.notify_all();
    if result.is_err() {
        *llm = None;
    }
    if run.job.cancel.load(Ordering::Relaxed) || state.shutdown {
        return;
    }
    if matches!(result, Ok(Ok(false))) {
        // Put the retry back behind any real summary that arrived during this attempt.
        state.practice = Some(run);
        return;
    }
    drop(state);
    let outcome = match result {
        Ok(Ok(true)) => run.best.ok_or(PRACTICE_FAILED),
        _ => Err(PRACTICE_FAILED),
    };
    (run.job.complete)(outcome);
}

#[cfg(test)]
#[path = "queue_coach_tests.rs"]
mod coach_tests;
