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
        let session = store
            .get_session(session_id)?
            .ok_or_else(|| JobError::SessionNotFound(session_id.into()))?;
        if store.get_prompt(prompt_id)?.is_none() {
            return Err(JobError::PromptNotFound(prompt_id.into()));
        }
        if !can_transition(session.status, SessionStatus::Summarizing) {
            return Err(JobError::Busy(session.status));
        }
        let mut state = self.shared.lock();
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
        self.shared.changed.notify_all();
        Ok(())
    }

    /// Starts loading the summary model in the background, so a recording's
    /// summary doesn't wait for it after the transcript is done (M5b).
    pub fn preload(&self) {
        self.shared.lock().preload = true;
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
            self.shared.changed.notify_all();
            drop(state);
            self.cancel_finalizing(session_id);
            return true;
        }
        match &state.running {
            Some(r) if r.session_id == session_id => {
                r.cancel.store(true, Ordering::Relaxed);
                true
            }
            _ => false,
        }
    }

    fn cancel_finalizing(&self, session_id: &str) {
        let store = &self.shared.store;
        match store.get_session(session_id) {
            Ok(Some(s)) if s.status == SessionStatus::Finalizing => {}
            _ => return,
        }
        if let Err(e) = store.set_status(session_id, SessionStatus::Cancelled, None) {
            eprintln!("cancel {session_id}: saving Cancelled failed: {e}");
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
        let (_state, result) = self
            .shared
            .changed
            .wait_timeout_while(state, timeout, |s| {
                !s.pending.is_empty() || s.running.is_some()
            })
            .unwrap_or_else(|p| p.into_inner());
        !result.timed_out()
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
    // The model stays loaded between jobs (KTD5).
    let mut llm: Option<Arc<dyn Summarizer>> = None;
    loop {
        let (job, cancel) = {
            let state = shared.lock();
            let mut state = shared
                .changed
                .wait_while(state, |s| s.pending.is_empty() && !s.preload && !s.shutdown)
                .unwrap_or_else(|p| p.into_inner());
            if state.shutdown {
                return;
            }
            if std::mem::take(&mut state.preload) {
                drop(state);
                if llm.is_none() {
                    match catch_unwind(AssertUnwindSafe(|| shared.loader.load())) {
                        Ok(Ok(model)) => llm = Some(model),
                        // The job loads it again and reports the error.
                        Ok(Err(e)) => eprintln!("preloading the summary model failed: {e}"),
                        Err(_) => eprintln!("preloading the summary model crashed"),
                    }
                }
                continue;
            }
            let job = state.pending.pop_front().expect("pending is non-empty");
            let cancel = Arc::new(AtomicBool::new(false));
            state.running = Some(Running {
                session_id: job.session_id.clone(),
                cancel: Arc::clone(&cancel),
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
        shared.lock().running = None;
        shared.changed.notify_all();
    }
}
