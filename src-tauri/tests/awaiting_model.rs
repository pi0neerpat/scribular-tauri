//! U10: transcripts remain viewable while the summary model downloads, and
//! the persistent summary queue drains FIFO after download or relaunch.
#![cfg(feature = "fake-engines")]

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Condvar, Mutex,
    },
    time::Duration,
};

use scribular_lib::{
    engines::fake::{FakeLoader, FakeSummarizer},
    jobs::{JobEvents, JobQueue, SessionProgress, SessionSegments, SummaryDelta},
    store::{InputMode, NewSession, SessionStatus, Store},
};

const PROMPT: &str = "default-therapy-dora";
const WAIT: Duration = Duration::from_secs(10);

struct Db(PathBuf);
impl Db {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let p = std::env::temp_dir().join(format!(
            "scribular-awaiting-{}-{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        Self(p)
    }
}
impl Drop for Db {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let mut path = self.0.clone().into_os_string();
            path.push(suffix);
            let _ = std::fs::remove_file(path);
        }
    }
}

#[derive(Default)]
struct Events {
    progress: Mutex<Vec<SessionProgress>>,
    changed: Condvar,
}
impl Events {
    fn wait_done(&self, n: usize) -> Vec<String> {
        let events = self.progress.lock().unwrap();
        let (events, result) = self
            .changed
            .wait_timeout_while(events, WAIT, |e| {
                e.iter().filter(|p| p.state == SessionStatus::Done).count() < n
            })
            .unwrap();
        assert!(!result.timed_out(), "summary did not finish: {events:?}");
        events
            .iter()
            .filter(|p| p.state == SessionStatus::Done)
            .map(|p| p.session_id.clone())
            .collect()
    }
}
impl JobEvents for Events {
    fn progress(&self, p: SessionProgress) {
        self.progress.lock().unwrap().push(p);
        self.changed.notify_all();
    }
    fn summary_delta(&self, _: SummaryDelta) {}
    fn segments(&self, _: SessionSegments) {}
}

fn session(store: &Store) -> String {
    let s = store
        .create_session(&NewSession {
            title: "waiting session".into(),
            input_mode: InputMode::Mic,
            prompt_id: Some(PROMPT.into()),
            model_id: None,
        })
        .unwrap();
    store
        .save_transcript(
            &s.id,
            "one two three four five six seven eight nine ten eleven twelve",
            1000,
        )
        .unwrap();
    store
        .set_status(&s.id, SessionStatus::Finalizing, None)
        .unwrap();
    s.id
}

#[test]
fn transcript_waits_at_download_progress_and_summarizes_when_ready() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    loader.ready.store(false, Ordering::SeqCst); // deterministic stand-in for a partial (60%) download
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(store.clone(), loader.clone(), events.clone());
    let id = session(&store);
    queue.regenerate(&id, PROMPT).unwrap();
    assert_eq!(
        store.get_session(&id).unwrap().unwrap().status,
        SessionStatus::AwaitingModel
    );
    assert_eq!(
        store.get_session(&id).unwrap().unwrap().transcript,
        "one two three four five six seven eight nine ten eleven twelve"
    );
    assert_eq!(
        events.progress.lock().unwrap()[0].state,
        SessionStatus::AwaitingModel
    );
    loader.ready.store(true, Ordering::SeqCst);
    assert!(queue.wait_idle(WAIT));
    assert_eq!(
        store.get_session(&id).unwrap().unwrap().status,
        SessionStatus::Done
    );
    assert_eq!(events.wait_done(1), [id]);
}

#[test]
fn multiple_waiting_sessions_summarize_in_stop_order() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    loader.ready.store(false, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(store.clone(), loader.clone(), events.clone());
    let first = session(&store);
    let second = session(&store);
    queue.regenerate(&first, PROMPT).unwrap();
    queue.regenerate(&second, PROMPT).unwrap();
    loader.ready.store(true, Ordering::SeqCst);
    assert!(queue.wait_idle(WAIT));
    assert_eq!(events.wait_done(2), [first, second]);
}

#[test]
fn waiting_sessions_resume_from_persistent_queue_after_relaunch() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    loader.ready.store(false, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let first = session(&store);
    let second = session(&store);
    {
        let queue = JobQueue::new(store.clone(), loader.clone(), events.clone());
        queue.regenerate(&first, PROMPT).unwrap();
        queue.regenerate(&second, PROMPT).unwrap();
    }
    drop(events);
    let reopened = Arc::new(Store::open(&db.0).unwrap());
    loader.ready.store(true, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(reopened.clone(), loader, events.clone());
    assert!(queue.wait_idle(WAIT));
    assert_eq!(
        reopened.get_session(&first).unwrap().unwrap().status,
        SessionStatus::Done
    );
    assert_eq!(
        reopened.get_session(&second).unwrap().unwrap().status,
        SessionStatus::Done
    );
    assert_eq!(events.wait_done(2), [first, second]);
}

#[test]
fn restart_recovers_a_queued_summarizing_session_and_keeps_fifo_order() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0).unwrap());
    let first = session(&store);
    let second = session(&store);
    store
        .queue_status(
            &first,
            PROMPT,
            SessionStatus::Summarizing,
            SessionStatus::Finalizing,
        )
        .unwrap();
    store
        .queue_status(
            &second,
            PROMPT,
            SessionStatus::Finalizing,
            SessionStatus::Finalizing,
        )
        .unwrap();
    drop(store);

    let reopened = Arc::new(Store::open(&db.0).unwrap());
    assert_eq!(
        reopened.get_session(&first).unwrap().unwrap().status,
        SessionStatus::AwaitingModel
    );
    assert_eq!(
        reopened.get_session(&second).unwrap().unwrap().status,
        SessionStatus::AwaitingModel
    );
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(reopened, loader, events.clone());
    assert!(queue.wait_idle(WAIT));
    assert_eq!(events.wait_done(2), [first, second]);
}

#[test]
fn cancelling_a_waiting_session_removes_its_persisted_job() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    loader.ready.store(false, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(store.clone(), loader, events);
    let id = session(&store);
    queue.regenerate(&id, PROMPT).unwrap();

    assert!(queue.cancel(&id));
    assert_eq!(
        store.get_session(&id).unwrap().unwrap().status,
        SessionStatus::Cancelled
    );
    assert!(store.queued_summaries().unwrap().is_empty());
}

#[test]
fn cancelling_a_queued_resummary_keeps_the_completed_session_and_drops_the_row() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    loader.ready.store(false, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(store.clone(), loader, events);
    let id = session(&store);
    store.set_status(&id, SessionStatus::Done, None).unwrap();
    queue.regenerate(&id, PROMPT).unwrap();

    assert!(queue.cancel(&id));
    assert_eq!(
        store.get_session(&id).unwrap().unwrap().status,
        SessionStatus::Done
    );
    assert!(store.queued_summaries().unwrap().is_empty());
}
