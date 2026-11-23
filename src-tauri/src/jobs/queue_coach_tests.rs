use super::*;
use crate::{
    coach::{compile, practice::PRACTICE_SOURCE},
    llm::{GenEvent, Generation},
    store::{key::MemoryKeyStore, InputMode, NewSession, SessionPatch},
};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicUsize, mpsc, OnceLock},
};

const WAIT: Duration = Duration::from_secs(5);
const STYLE: &str = "default-therapy-dora";
const INSTRUCTIONS: &str = "Practice job instructions";
const BEST: &str =
    "Preamble\nData:\nThe client took a break.\nAssessment:\nThe client found it helpful.";
const PASS: &str = "Data:\nThe client took a break.\nAssessment:\nThe client found it helpful.";

struct TestStore(PathBuf);

impl TestStore {
    fn new() -> (Self, Arc<Store>) {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "scribular-practice-{}-{}.db",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        let store = Store::open(&fixture.0, &MemoryKeyStore::with_key([7; 32])).unwrap();
        (fixture, Arc::new(store))
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        for suffix in ["", "-shm", "-wal"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", self.0.display()));
        }
    }
}

struct TestLogDir(PathBuf);

impl Drop for TestLogDir {
    fn drop(&mut self) {
        crate::diag::log::reset_for_tests();
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Silent;

impl JobEvents for Silent {
    fn progress(&self, _: SessionProgress) {}
    fn summary_delta(&self, _: crate::jobs::SummaryDelta) {}
    fn segments(&self, _: crate::jobs::SessionSegments) {}
}

struct AvailabilityChanged(mpsc::Sender<()>);

impl JobEvents for AvailabilityChanged {
    fn progress(&self, _: SessionProgress) {}
    fn summary_delta(&self, _: crate::jobs::SummaryDelta) {}
    fn segments(&self, _: crate::jobs::SessionSegments) {}
    fn summary_queue_changed(&self) {
        self.0.send(()).unwrap();
    }
}

struct CoachCancelled(mpsc::Sender<()>);

impl JobEvents for CoachCancelled {
    fn progress(&self, _: SessionProgress) {}
    fn summary_delta(&self, _: crate::jobs::SummaryDelta) {}
    fn segments(&self, _: crate::jobs::SessionSegments) {}
    fn session_coach_cancelled(&self) {
        self.0.send(()).unwrap();
    }
}

struct Scripted {
    attempts: AtomicUsize,
    calls: Mutex<Vec<Vec<String>>>,
    limits: Mutex<Vec<Vec<usize>>>,
    first: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    pass: bool,
}

impl Summarizer for Scripted {
    fn context_tokens(&self) -> usize {
        1100
    }

    fn count_tokens(&self, text: &str) -> crate::llm::Result<usize> {
        Ok(text.split_whitespace().count())
    }

    fn prompt_tokens(&self, text: &str) -> crate::llm::Result<usize> {
        self.count_tokens(text)
    }

    fn generate(
        &self,
        requests: &[Generation],
        cancel: &AtomicBool,
        _: &mut dyn FnMut(GenEvent<'_>),
    ) -> crate::llm::Result<Vec<String>> {
        self.calls
            .lock()
            .unwrap()
            .push(requests.iter().map(|r| r.prompt.clone()).collect());
        self.limits
            .lock()
            .unwrap()
            .push(requests.iter().map(|r| r.max_tokens).collect());
        let prompt = &requests[0].prompt;
        if !prompt.contains(INSTRUCTIONS) {
            return Ok(vec!["Real summary.".into()]);
        }
        if !prompt.contains("Combine them into a single") {
            assert!(requests.len() > 1);
            return Ok(vec!["Intermediate practice note.".into(); requests.len()]);
        }
        let index = self.attempts.fetch_add(1, Ordering::Relaxed);
        if index == 0 {
            if let Some((arrived, release)) = self.first.lock().unwrap().take() {
                arrived.send(()).unwrap();
                release.recv_timeout(WAIT).unwrap();
            }
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(LlmError::Cancelled);
        }
        let answer = match index {
            0 => BEST,
            2 if self.pass => PASS,
            _ => "Incomplete note.",
        };
        Ok(vec![answer.into()])
    }
}

struct Loader(Arc<Scripted>);

impl SummarizerLoader for Loader {
    fn model_id(&self) -> String {
        "fixture".into()
    }

    fn load(&self, _: &str) -> crate::llm::Result<Arc<dyn Summarizer>> {
        Ok(self.0.clone())
    }
}

fn fixture_job() -> (PracticeJob, mpsc::Receiver<PracticeResult>) {
    static LONG_TRANSCRIPT: OnceLock<String> = OnceLock::new();
    let spec = crate::coach::test_style();
    let transcript = LONG_TRANSCRIPT.get_or_init(|| PRACTICE_SOURCE.transcript.repeat(15));
    let (done, result) = mpsc::channel();
    let job = PracticeJob {
        instructions: format!("{INSTRUCTIONS}\n{}", compile(&spec).unwrap()),
        spec,
        source: PracticeSource {
            transcript,
            ..PRACTICE_SOURCE
        },
        cancel: Arc::default(),
        progress: Box::new(|_| {}),
        complete: Box::new(move |note| {
            let _ = done.send(note);
        }),
    };
    (job, result)
}

fn engine(pass: bool) -> (Arc<Scripted>, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (arrived, started) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    (
        Arc::new(Scripted {
            attempts: AtomicUsize::new(0),
            calls: Mutex::default(),
            limits: Mutex::default(),
            first: Mutex::new(Some((arrived, wait))),
            pass,
        }),
        started,
        release,
    )
}

#[test]
fn practice_uses_chunk_and_combine_with_retries_and_real_work_first() {
    for pass in [true, false] {
        let (_db, store) = TestStore::new();
        let (engine, started, release) = engine(pass);
        let queue = JobQueue::new(
            store.clone(),
            Arc::new(Loader(engine.clone())),
            Arc::new(Silent),
        );
        let (job, result) = fixture_job();
        let instructions = job.instructions.clone();
        queue.enqueue_practice(job).unwrap();
        started.recv_timeout(WAIT).unwrap();

        let real = store
            .create_session(&NewSession {
                title: "Real session".into(),
                input_mode: InputMode::Mic,
                prompt_id: Some(STYLE.into()),
                model_id: None,
            })
            .unwrap();
        store
            .save_transcript(&real.id, &"transcript ".repeat(100), 1_000)
            .unwrap();
        store
            .set_status(&real.id, SessionStatus::Done, None)
            .unwrap();
        queue.regenerate(&real.id, STYLE).unwrap();
        assert!(
            matches!(queue.enqueue_practice(fixture_job().0), Err(error) if error == crate::coach::CoachBlock::Busy.message())
        );
        release.send(()).unwrap();

        let note = result.recv_timeout(WAIT).unwrap().unwrap();
        assert_eq!(note.total, 9);
        assert_eq!(note.passed, if pass { 9 } else { 8 });
        assert_eq!(note.text, if pass { PASS } else { BEST });
        let calls = engine.calls.lock().unwrap();
        assert_eq!(calls.len(), 7);
        assert!(
            calls[0].len() > 1
                && calls[0]
                    .iter()
                    .all(|prompt| prompt.starts_with(&instructions))
        );
        assert!(
            calls[1][0].starts_with(&instructions)
                && calls[1][0].contains("Intermediate practice note.")
        );
        assert!(
            !calls[2][0].contains(INSTRUCTIONS),
            "the real summary runs before attempt two"
        );
        assert!(calls[3][0].contains(practice::Check::NoPreamble.correction()));
        assert!(calls[5][0].contains(practice::Check::SectionsPresent.correction()));
        let limits = engine.limits.lock().unwrap();
        assert_eq!(limits[1][0], practice::NOTE_TOKENS);
        assert!(
            limits[2][0] > practice::NOTE_TOKENS,
            "real summaries keep their context budget"
        );
        assert_eq!(
            store.get_session(&real.id).unwrap().unwrap().summary,
            "Real summary."
        );
        assert_eq!(store.list_sessions().unwrap().len(), 1);
        assert!(store.queued_summaries().unwrap().is_empty());
    }
}

#[test]
fn cancelled_practice_drops_the_result_and_has_no_persisted_job() {
    let (_db, store) = TestStore::new();
    let (engine, started, release) = engine(true);
    let queue = JobQueue::new(store.clone(), Arc::new(Loader(engine)), Arc::new(Silent));
    let (job, result) = fixture_job();
    let cancel = job.cancel.clone();
    queue.enqueue_practice(job).unwrap();
    started.recv_timeout(WAIT).unwrap();
    queue.cancel_practice();
    assert!(cancel.load(Ordering::Relaxed));
    release.send(()).unwrap();
    assert!(matches!(
        result.recv_timeout(WAIT),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    assert_eq!(store.list_sessions().unwrap().len(), 0);
    assert!(store.queued_summaries().unwrap().is_empty());
}

#[test]
fn session_coach_job_leaves_saved_session_unchanged() {
    let _log_guard = crate::diag::log::TEST_LOG_LOCK.lock().unwrap();
    let (_db, store) = TestStore::new();
    let log_dir = std::env::temp_dir().join(format!(
        "scribular-session-coach-logs-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&log_dir);
    let _log_cleanup = TestLogDir(log_dir.clone());
    crate::diag::log::init(&log_dir).unwrap();
    crate::diag::telemetry::set_enabled(&store, true).unwrap();
    let (engine, _, _) = engine(true);
    let queue = JobQueue::new(store.clone(), Arc::new(Loader(engine)), Arc::new(Silent));
    let session = store
        .create_session(&NewSession {
            title: "Fixture".into(),
            input_mode: InputMode::Mic,
            prompt_id: Some(STYLE.into()),
            model_id: None,
        })
        .unwrap();
    let sentinel = "Client Akoya reported violet lanterns.";
    store
        .update_session(
            &session.id,
            &SessionPatch {
                title: None,
                transcript: Some(sentinel.into()),
                summary: Some(sentinel.into()),
            },
        )
        .unwrap();
    store
        .set_status(&session.id, SessionStatus::Done, None)
        .unwrap();
    let before = store.get_session(&session.id).unwrap().unwrap();
    let (done, result) = mpsc::channel();
    queue
        .enqueue_session_coach(SessionCoachJob {
            prompt: format!("Shorten this: {sentinel}"),
            cancel: Arc::default(),
            complete: Box::new(move |result| {
                done.send(result).unwrap();
            }),
        })
        .unwrap();
    assert_eq!(result.recv_timeout(WAIT).unwrap().unwrap(), "Real summary.");
    let after = store.get_session(&session.id).unwrap().unwrap();
    assert_eq!(after.summary, before.summary);
    assert_eq!(after.transcript, before.transcript);
    assert!(store.queued_summaries().unwrap().is_empty());
    assert!(queue.wait_idle(WAIT));
    let diagnostics = crate::diag::info::diagnostics(
        &store,
        &crate::models::ModelManager::new(log_dir.join("models"), crate::models::manifest()),
        &log_dir,
    )
    .unwrap();
    let logs = std::fs::read_to_string(log_dir.join("scribular.log")).unwrap();
    for text in [diagnostics, logs] {
        assert!(!text.contains(sentinel));
        assert!(!text.contains("Shorten this"));
    }
    assert_eq!(store.get_setting("usage_sessions").unwrap(), None);
    assert_eq!(store.get_setting("usage_total_seconds").unwrap(), None);
    assert_eq!(store.get_setting("pending_report").unwrap(), None);
}

#[test]
fn session_coach_completion_signals_availability() {
    let (_db, store) = TestStore::new();
    let (engine, _, _) = engine(true);
    let (changed, signals) = mpsc::channel();
    let queue = JobQueue::new(
        store,
        Arc::new(Loader(engine)),
        Arc::new(AvailabilityChanged(changed)),
    );
    let (done, result) = mpsc::channel();
    queue
        .enqueue_session_coach(SessionCoachJob {
            prompt: "Shorten the note".into(),
            cancel: Arc::default(),
            complete: Box::new(move |result| {
                done.send(result).unwrap();
            }),
        })
        .unwrap();
    assert_eq!(result.recv_timeout(WAIT).unwrap().unwrap(), "Real summary.");
    signals.recv_timeout(WAIT).unwrap();
    assert!(!queue.is_busy());
}

#[test]
fn session_coach_completion_allows_follow_up_but_is_still_cancellable_and_running() {
    let (_db, store) = TestStore::new();
    let (engine, _, _) = engine(true);
    let queue = JobQueue::new(store, Arc::new(Loader(engine)), Arc::new(Silent));
    let (completing, started) = mpsc::channel();
    let (release, finish) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    queue
        .enqueue_session_coach(SessionCoachJob {
            prompt: "Shorten the note".into(),
            cancel: cancel.clone(),
            complete: Box::new(move |_| {
                completing.send(()).unwrap();
                finish.recv_timeout(WAIT).unwrap();
            }),
        })
        .unwrap();
    started.recv_timeout(WAIT).unwrap();
    assert!(!queue.coach_busy());
    assert!(queue.is_busy());
    assert!(!queue.wait_idle(Duration::ZERO));
    queue.cancel_session_coach();
    assert!(cancel.load(Ordering::Relaxed));

    let (done, result) = mpsc::channel();
    queue
        .enqueue_session_coach(SessionCoachJob {
            prompt: "Keep shortening the note".into(),
            cancel: Arc::default(),
            complete: Box::new(move |result| done.send(result).unwrap()),
        })
        .unwrap();
    release.send(()).unwrap();
    assert_eq!(result.recv_timeout(WAIT).unwrap().unwrap(), "Real summary.");
    assert!(queue.wait_idle(WAIT));
}

#[test]
fn cancelled_session_coach_drops_late_result() {
    let (_db, store) = TestStore::new();
    let (engine, started, release) = engine(true);
    let queue = JobQueue::new(store, Arc::new(Loader(engine)), Arc::new(Silent));
    let (done, result) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    queue
        .enqueue_session_coach(SessionCoachJob {
            prompt: format!("{INSTRUCTIONS}\nCombine them into a single"),
            cancel: cancel.clone(),
            complete: Box::new(move |result| {
                done.send(result).unwrap();
            }),
        })
        .unwrap();
    started.recv_timeout(WAIT).unwrap();
    assert!(queue.coach_busy());
    queue.cancel_session_coach();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(!queue.coach_busy());
    assert!(queue.while_real_idle(|| ()).is_some());
    assert!(queue.is_busy());
    assert!(queue.while_idle(|| ()).is_none());
    assert!(queue.enqueue_practice(fixture_job().0).is_err());
    assert!(!queue.wait_idle(Duration::from_millis(10)));
    let (next_done, next_result) = mpsc::channel();
    queue
        .enqueue_session_coach(SessionCoachJob {
            prompt: "Shorten the note".into(),
            cancel: Arc::default(),
            complete: Box::new(move |result| next_done.send(result).unwrap()),
        })
        .unwrap();
    assert!(queue.coach_busy());
    release.send(()).unwrap();
    assert!(matches!(
        result.recv_timeout(WAIT),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    assert_eq!(next_result.recv_timeout(WAIT).unwrap().unwrap(), "Real summary.");
    assert!(queue.wait_idle(WAIT));
}

#[test]
fn real_summary_preempts_running_session_coach() {
    let (_db, store) = TestStore::new();
    let (engine, started, release) = engine(true);
    let (cancelled, signal) = mpsc::channel();
    let queue = JobQueue::new(
        store.clone(),
        Arc::new(Loader(engine)),
        Arc::new(CoachCancelled(cancelled)),
    );
    let session = store
        .create_session(&NewSession {
            title: "Real".into(),
            input_mode: InputMode::Mic,
            prompt_id: Some(STYLE.into()),
            model_id: None,
        })
        .unwrap();
    store
        .save_transcript(&session.id, &"word ".repeat(100), 1000)
        .unwrap();
    store
        .set_status(&session.id, SessionStatus::Done, None)
        .unwrap();
    let (done, result) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    queue
        .enqueue_session_coach(SessionCoachJob {
            prompt: format!("{INSTRUCTIONS}\nCombine them into a single"),
            cancel: cancel.clone(),
            complete: Box::new(move |result| {
                done.send(result).unwrap();
            }),
        })
        .unwrap();
    started.recv_timeout(WAIT).unwrap();
    queue.regenerate(&session.id, STYLE).unwrap();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(queue.coach_busy());
    assert!(queue.while_real_idle(|| ()).is_none());
    signal.recv_timeout(WAIT).unwrap();
    release.send(()).unwrap();
    assert!(matches!(
        result.recv_timeout(WAIT),
        Err(mpsc::RecvTimeoutError::Disconnected)
    ));
    assert!(queue.wait_idle(WAIT));
    assert_eq!(
        store.get_session(&session.id).unwrap().unwrap().summary,
        "Real summary."
    );
}
