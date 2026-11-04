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

use scribular_lib::store::key::MemoryKeyStore;
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
    pause_at: Option<SessionStatus>,
    done_entered: Mutex<bool>,
    pause_entered: Condvar,
    release_done: Mutex<bool>,
    release: Condvar,
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
        if self.pause_at == Some(p.state) {
            *self.done_entered.lock().unwrap() = true;
            self.pause_entered.notify_all();
            let released = self.release_done.lock().unwrap();
            let _released = self
                .release
                .wait_while(released, |released| !*released)
                .unwrap();
        }
        self.progress.lock().unwrap().push(p);
        self.changed.notify_all();
    }
    fn summary_delta(&self, _: SummaryDelta) {}
    fn segments(&self, _: SessionSegments) {}
}

impl Events {
    fn paused() -> Self {
        Self::paused_at(SessionStatus::Done)
    }

    fn paused_at(state: SessionStatus) -> Self {
        Self {
            pause_at: Some(state),
            ..Self::default()
        }
    }

    fn wait_done_callback(&self) {
        let entered = self.done_entered.lock().unwrap();
        let (entered, result) = self
            .pause_entered
            .wait_timeout_while(entered, WAIT, |entered| !*entered)
            .unwrap();
        assert!(*entered && !result.timed_out());
    }

    fn release_done_callback(&self) {
        *self.release_done.lock().unwrap() = true;
        self.release.notify_all();
    }
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
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
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
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
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
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
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
    let reopened = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
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
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let first = session(&store);
    let second = session(&store);
    store
        .queue_status(
            &first,
            PROMPT,
            SessionStatus::Summarizing,
            SessionStatus::Finalizing,
            false,
        )
        .unwrap();
    store
        .queue_status(
            &second,
            PROMPT,
            SessionStatus::Finalizing,
            SessionStatus::Finalizing,
            false,
        )
        .unwrap();
    drop(store);

    let reopened = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
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
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
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
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
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

#[test]
fn updating_a_waiting_job_keeps_its_original_restore_status() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    loader.ready.store(false, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(store.clone(), loader, events);
    let id = session(&store);
    store.set_status(&id, SessionStatus::Done, None).unwrap();

    queue.regenerate(&id, PROMPT).unwrap();
    queue.regenerate(&id, PROMPT).unwrap();
    assert!(queue.cancel(&id));

    assert_eq!(
        store.get_session(&id).unwrap().unwrap().status,
        SessionStatus::Done
    );
    assert!(store.queued_summaries().unwrap().is_empty());
}

#[test]
fn requeue_during_completion_keeps_the_new_durable_row() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    let events = Arc::new(Events::paused());
    let id = session(&store);
    {
        let queue = JobQueue::new(store.clone(), loader.clone(), events.clone());
        queue.regenerate(&id, PROMPT).unwrap();
        events.wait_done_callback();

        loader.ready.store(false, Ordering::SeqCst);
        queue.regenerate(&id, PROMPT).unwrap();
        events.release_done_callback();
        assert!(!queue.wait_idle(Duration::from_millis(50)));
    }
    assert_eq!(store.queued_summaries().unwrap().len(), 1);

    loader.ready.store(true, Ordering::SeqCst);
    let resumed_events = Arc::new(Events::default());
    let queue = JobQueue::new(store.clone(), loader, resumed_events.clone());
    assert!(queue.wait_idle(WAIT));
    assert_eq!(
        store.get_session(&id).unwrap().unwrap().status,
        SessionStatus::Done
    );
    assert!(store.queued_summaries().unwrap().is_empty());
}

#[test]
fn dropping_during_a_summary_keeps_the_job_for_relaunch() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    let events = Arc::new(Events::paused_at(SessionStatus::Summarizing));
    let id = session(&store);
    let queue = JobQueue::new(store.clone(), loader.clone(), events.clone());
    queue.regenerate(&id, PROMPT).unwrap();
    events.wait_done_callback();

    std::thread::scope(|scope| {
        scope.spawn(move || drop(queue));
        std::thread::sleep(Duration::from_millis(50));
        events.release_done_callback();
    });
    assert_eq!(store.queued_summaries().unwrap().len(), 1);
    drop(store);
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());

    let resumed_events = Arc::new(Events::default());
    let queue = JobQueue::new(store.clone(), loader, resumed_events.clone());
    assert!(queue.wait_idle(WAIT));
    assert_eq!(
        store.get_session(&id).unwrap().unwrap().status,
        SessionStatus::Done
    );
    assert!(store.queued_summaries().unwrap().is_empty());
}

#[test]
fn cancelling_a_requeue_during_completion_restores_the_new_done_status() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    let events = Arc::new(Events::paused());
    let id = session(&store);
    let queue = JobQueue::new(store.clone(), loader.clone(), events.clone());
    queue.regenerate(&id, PROMPT).unwrap();
    events.wait_done_callback();
    let summary = store.get_session(&id).unwrap().unwrap().summary;
    loader.ready.store(false, Ordering::SeqCst);
    queue.regenerate(&id, PROMPT).unwrap();
    let cancelled = queue.cancel(&id);
    events.release_done_callback();
    assert!(queue.wait_idle(WAIT));
    drop(queue);
    drop(store);
    let reopened = Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap();
    let saved = reopened.get_session(&id).unwrap().unwrap();
    assert!(cancelled);
    assert_eq!(saved.status, SessionStatus::Done);
    assert_eq!(saved.summary, summary);
    assert!(reopened.queued_summaries().unwrap().is_empty());
}

#[test]
fn requeue_during_completion_moves_to_the_tail_across_restart() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    let events = Arc::new(Events::paused());
    let first = session(&store);
    let second = session(&store);
    let queue = JobQueue::new(store.clone(), loader.clone(), events.clone());
    queue.regenerate(&first, PROMPT).unwrap();
    events.wait_done_callback();
    loader.ready.store(false, Ordering::SeqCst);
    queue.regenerate(&second, PROMPT).unwrap();
    queue.regenerate(&first, PROMPT).unwrap();
    events.release_done_callback();
    drop(queue);
    drop(store);
    let reopened = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    loader.ready.store(true, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(reopened, loader, events.clone());
    assert!(queue.wait_idle(WAIT));
    assert_eq!(events.wait_done(2), [second, first]);
}

#[test]
fn explicit_cancel_then_quit_does_not_resume_on_reopen() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let mut model = FakeSummarizer::new(8192);
    model.token_delay = Duration::from_millis(100);
    let loader = Arc::new(FakeLoader::new(model));
    let events = Arc::new(Events::paused_at(SessionStatus::Cancelled));
    let id = session(&store);
    let queue = JobQueue::new(store.clone(), loader.clone(), events.clone());
    queue.regenerate(&id, PROMPT).unwrap();
    // Wait for a running job using the loading event before asking it to stop.
    {
        let progress = events.progress.lock().unwrap();
        let (_progress, result) = events
            .changed
            .wait_timeout_while(progress, WAIT, |p| {
                !p.iter().any(|p| p.state == SessionStatus::Summarizing)
            })
            .unwrap();
        assert!(!result.timed_out());
    }
    assert!(queue.cancel(&id));
    events.wait_done_callback();
    let queued_before_shutdown = store.queued_summaries().unwrap();
    std::thread::scope(|scope| {
        scope.spawn(move || drop(queue));
        std::thread::sleep(Duration::from_millis(50));
        events.release_done_callback();
    });
    drop(store);
    let reopened = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(reopened.clone(), loader, events.clone());
    assert!(queue.wait_idle(WAIT));
    assert_eq!(
        reopened.get_session(&id).unwrap().unwrap().status,
        SessionStatus::Cancelled
    );
    assert!(reopened.queued_summaries().unwrap().is_empty());
    assert!(events.progress.lock().unwrap().is_empty());
    assert!(
        queued_before_shutdown.is_empty(),
        "user cancellation must be durable before worker cleanup or Drop"
    );
}

#[test]
fn updating_the_same_pending_job_preserves_fifo_across_restart() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    loader.ready.store(false, Ordering::SeqCst);
    let first = session(&store);
    let second = session(&store);
    let prompt = store
        .list_prompts()
        .unwrap()
        .into_iter()
        .find(|p| p.id != PROMPT)
        .unwrap()
        .id;
    let queue = JobQueue::new(store.clone(), loader.clone(), Arc::new(Events::default()));
    queue.regenerate(&first, PROMPT).unwrap();
    queue.regenerate(&second, PROMPT).unwrap();
    queue.regenerate(&first, &prompt).unwrap();
    drop(queue);
    drop(store);
    let reopened = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    assert_eq!(
        reopened.queued_summaries().unwrap(),
        [
            (first.clone(), prompt.clone()),
            (second.clone(), PROMPT.into()),
        ]
    );
    loader.ready.store(true, Ordering::SeqCst);
    let events = Arc::new(Events::default());
    let queue = JobQueue::new(reopened.clone(), loader, events.clone());
    assert!(queue.wait_idle(WAIT));
    assert_eq!(events.wait_done(2), [first.clone(), second]);
    assert_eq!(
        reopened.get_session(&first).unwrap().unwrap().prompt_id,
        Some(prompt)
    );
}

#[test]
fn explicit_cancel_is_durable_before_the_worker_can_finish() {
    let db = Db::new();
    let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
    let loader = Arc::new(FakeLoader::new(FakeSummarizer::new(8192)));
    let events = Arc::new(Events::paused_at(SessionStatus::Summarizing));
    let id = session(&store);
    let queue = JobQueue::new(store.clone(), loader, events.clone());
    queue.regenerate(&id, PROMPT).unwrap();
    events.wait_done_callback();
    let cancelled = queue.cancel(&id);
    let requeue = queue.regenerate(&id, PROMPT);
    // A process exit may skip Drop and worker cleanup entirely.
    let reopened = Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap();
    let status = reopened.get_session(&id).unwrap().unwrap().status;
    let pending = reopened.queued_summaries().unwrap();
    events.release_done_callback();
    assert!(queue.wait_idle(WAIT));
    assert!(cancelled);
    assert!(
        requeue.is_err(),
        "the cancelled worker still owns this session"
    );
    assert_eq!(status, SessionStatus::Cancelled);
    assert!(pending.is_empty());
}

#[test]
fn durable_running_cancellation_covers_the_gap_before_summarizing() {
    for (before, after) in [
        (SessionStatus::Finalizing, SessionStatus::Cancelled),
        (SessionStatus::AwaitingModel, SessionStatus::Cancelled),
        (SessionStatus::Summarizing, SessionStatus::Cancelled),
        (SessionStatus::Done, SessionStatus::Done),
        (SessionStatus::Failed, SessionStatus::Failed),
        (SessionStatus::Cancelled, SessionStatus::Cancelled),
    ] {
        let db = Db::new();
        let store = Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap();
        let id = session(&store);
        store
            .queue_status(&id, PROMPT, before, SessionStatus::Finalizing, false)
            .unwrap();
        store.cancel_running_summary(&id).unwrap();
        drop(store);
        let reopened = Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap();
        assert_eq!(
            reopened.get_session(&id).unwrap().unwrap().status,
            after,
            "{before:?}"
        );
        assert!(reopened.queued_summaries().unwrap().is_empty());
    }
}
