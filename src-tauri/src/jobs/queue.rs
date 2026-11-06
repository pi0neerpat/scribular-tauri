//! One worker thread draining a FIFO of summarization jobs, so two
//! summaries never compete for the model or for memory.

use std::{
    collections::VecDeque,
    fmt,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, MutexGuard,
    },
    thread::JoinHandle,
    time::Duration,
};

use super::{state::can_transition, summarize, JobEvents, SessionProgress};
use crate::llm::{Summarizer, SummarizerLoader};
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

#[derive(Default)]
struct State {
    pending: VecDeque<Job>,
    running: Option<Running>,
    /// Load the model now, ahead of the job a finishing recording queues.
    preload: bool,
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
            Err(e) => eprintln!("restoring summary queue failed: {e}"),
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

    /// Whether a summary is running now; queued jobs don't count.
    pub fn summarizing(&self) -> bool {
        self.shared.lock().running.is_some()
    }

    pub fn is_busy(&self) -> bool {
        let state = self.shared.lock();
        !state.pending.is_empty() || state.running.is_some()
    }

    /// Serializes model changes with summary admission and start. Returns
    /// `None` while a summary is running. The closure runs under the queue
    /// lock and must not call back into this queue.
    pub fn while_idle<T>(&self, operation: impl FnOnce() -> T) -> Option<T> {
        let state = self.shared.lock();
        if state.running.is_some() {
            return None;
        }
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
            self.shared.changed.notify_all();
            return true;
        }
        match &mut state.running {
            Some(r) if r.session_id == session_id => {
                if let Err(e) = self.shared.store.cancel_running_summary(session_id) {
                    eprintln!("cancel {session_id}: persisting cancellation failed: {e}");
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
                    eprintln!("clear sessions: deleting {} failed: {e}", session.id);
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
                eprintln!("cancel {session_id}: removing queued summary failed: {e}");
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
                !s.pending.is_empty() || s.running.is_some()
            })
            .unwrap_or_else(|p| p.into_inner());
        state.pending.is_empty() && state.running.is_none()
    }
}

impl Drop for JobQueue {
    fn drop(&mut self) {
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
                    (s.pending.is_empty() || !shared.loader.ready()) && !s.preload && !s.shutdown
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
                    match catch_unwind(AssertUnwindSafe(|| shared.loader.load(&id))) {
                        Ok(Ok(model)) => llm = Some((id, model)),
                        // The job loads it again and reports the error.
                        Ok(Err(e)) => eprintln!("preloading the summary model failed: {e}"),
                        Err(_) => eprintln!("preloading the summary model crashed"),
                    }
                }
                continue;
            }
            if !shared.loader.ready() {
                continue;
            }
            if state.pending.is_empty() {
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
        let ran = catch_unwind(AssertUnwindSafe(|| {
            summarize::run(shared, &job, &cancel, &mut llm)
        }));
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
                eprintln!(
                    "queue {}: preserving interrupted job failed: {e}",
                    job.session_id
                );
            }
        } else if !requeued {
            if let Err(e) = shared.store.dequeue_summary(&job.session_id) {
                eprintln!(
                    "queue {}: removing persisted job failed: {e}",
                    job.session_id
                );
            }
        }
        state.running = None;
        drop(state);
        shared.changed.notify_all();
    }
}
