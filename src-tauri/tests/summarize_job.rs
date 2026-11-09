//! U5: the summarization job against the fake engine (KTD9) — progress
//! order, cancel, the single-chunk path, the word floor, queueing, timings,
//! and the exact Electron prompts.
#![cfg(feature = "fake-engines")]

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

use scribular_lib::engines::fake::{FakeLoader, FakeSummarizer, TEMPLATE_TOKENS};
use scribular_lib::jobs::{
    JobError, JobEvents, JobQueue, SessionProgress, SessionSegments, SummaryDelta,
};
use scribular_lib::llm::prompt::{chunk_prompt, combine_prompt, CHUNK_NOTE_TOKENS};
use scribular_lib::llm::{Generation, LlmError};
use scribular_lib::store::key::MemoryKeyStore;
use scribular_lib::store::{InputMode, NewSession, Session, SessionStatus, Store, Timings};

const PROMPT_ID: &str = "default-therapy-dora";
const WAIT: Duration = Duration::from_secs(10);

/// A fresh database path, removed (with its WAL files) on drop.
struct TempDb(PathBuf);

impl TempDb {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "scribular-jobs-{}-{}.db",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        );
        let db = Self(std::env::temp_dir().join(name));
        db.remove();
        db
    }

    fn remove(&self) {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = self.0.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        self.remove();
    }
}

/// Records every event, and checks that the status row already says what
/// the event says when the event is emitted.
struct Recorder {
    store: Arc<Store>,
    progress: Mutex<Vec<SessionProgress>>,
    deltas: Mutex<Vec<SummaryDelta>>,
    stale_rows: Mutex<Vec<String>>,
    changed: Condvar,
}

impl Recorder {
    fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            progress: Mutex::default(),
            deltas: Mutex::default(),
            stale_rows: Mutex::default(),
            changed: Condvar::new(),
        }
    }

    fn progress(&self) -> Vec<SessionProgress> {
        self.progress.lock().unwrap().clone()
    }

    fn messages(&self, session_id: &str) -> Vec<(u32, u32, String)> {
        self.progress()
            .into_iter()
            .filter(|p| p.session_id == session_id)
            .map(|p| (p.step_index, p.step_count, p.message))
            .collect()
    }

    fn streamed(&self, session_id: &str) -> String {
        self.deltas
            .lock()
            .unwrap()
            .iter()
            .filter(|d| d.session_id == session_id)
            .map(|d| d.text.as_str())
            .collect()
    }

    /// Blocks until some progress event matches `pred`.
    fn wait_for(&self, pred: impl Fn(&SessionProgress) -> bool) {
        let events = self.progress.lock().unwrap();
        let (events, timeout) = self
            .changed
            .wait_timeout_while(events, WAIT, |e| !e.iter().any(&pred))
            .unwrap();
        assert!(!timeout.timed_out(), "no matching event in {:?}", *events);
    }
}

impl JobEvents for Recorder {
    fn progress(&self, p: SessionProgress) {
        let row = self.store.get_session(&p.session_id).unwrap().unwrap();
        if row.status != p.state {
            self.stale_rows.lock().unwrap().push(format!(
                "{:?} emitted while the row said {:?}",
                p.message, row.status
            ));
        }
        self.progress.lock().unwrap().push(p);
        self.changed.notify_all();
    }

    fn summary_delta(&self, d: SummaryDelta) {
        self.deltas.lock().unwrap().push(d);
    }

    fn segments(&self, _: SessionSegments) {}
}

struct Harness {
    _db: TempDb,
    store: Arc<Store>,
    loader: Arc<FakeLoader>,
    events: Arc<Recorder>,
    queue: JobQueue,
    base: String,
}

impl Harness {
    /// A context sized so each chunk holds exactly `budget` transcript words.
    fn with_budget(budget: usize, configure: impl FnOnce(&mut FakeSummarizer)) -> Self {
        Self::with_loader(budget, configure, |_| {})
    }

    fn with_loader(
        budget: usize,
        configure: impl FnOnce(&mut FakeSummarizer),
        configure_loader: impl FnOnce(&mut FakeLoader),
    ) -> Self {
        let db = TempDb::new();
        let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
        let base = store.get_prompt(PROMPT_ID).unwrap().unwrap().text;
        let empty_prompt = chunk_prompt(&base, "").split_whitespace().count() + TEMPLATE_TOKENS;
        // `chunk_budget` also reserves 16 tokens of boundary slack.
        let mut fake = FakeSummarizer::new(empty_prompt + CHUNK_NOTE_TOKENS + 16 + budget);
        configure(&mut fake);
        let mut loader = FakeLoader::new(fake);
        configure_loader(&mut loader);
        let loader = Arc::new(loader);
        let events = Arc::new(Recorder::new(store.clone()));
        let queue = JobQueue::new(store.clone(), loader.clone(), events.clone());
        Self {
            _db: db,
            store,
            loader,
            events,
            queue,
            base,
        }
    }

    /// A finished session holding `transcript` and `summary`.
    fn session(&self, transcript: &str, summary: &str) -> String {
        let s = self
            .store
            .create_session(&NewSession {
                title: "t".into(),
                input_mode: InputMode::Mic,
                prompt_id: None,
                model_id: None,
            })
            .unwrap();
        self.store.save_transcript(&s.id, transcript, 1000).unwrap();
        self.store
            .set_summary(&s.id, summary, &Timings::default())
            .unwrap();
        self.store
            .set_status(&s.id, SessionStatus::Done, None)
            .unwrap();
        s.id
    }

    fn get(&self, id: &str) -> Session {
        self.store.get_session(id).unwrap().unwrap()
    }

    fn wait_idle(&self) {
        assert!(self.queue.wait_idle(WAIT), "queue never went idle");
    }

    fn calls(&self) -> Vec<Vec<Generation>> {
        self.loader.summarizer.calls()
    }

    fn assert_rows_written_first(&self) {
        let stale = self.events.stale_rows.lock().unwrap();
        assert!(stale.is_empty(), "{stale:?}");
    }
}

fn words(n: usize) -> String {
    (0..n)
        .map(|i| format!("w{i}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn reply(seq: usize, tokens: usize) -> String {
    (0..tokens)
        .map(|t| format!("s{seq}t{t}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn msg(step: u32, count: u32, text: &str) -> (u32, u32, String) {
    (step, count, text.to_string())
}

#[test]
fn regenerate_reports_each_chunk_then_combining_then_done() {
    let h = Harness::with_budget(100, |_| {});
    let transcript = words(250);
    let id = h.session(&transcript, "old");

    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.wait_idle();

    assert_eq!(
        h.events.messages(&id),
        [
            msg(1, 1, "Loading summary model..."),
            msg(2, 5, "Summarizing chunk 1 of 3..."),
            msg(3, 5, "Summarizing chunk 2 of 3..."),
            msg(4, 5, "Summarizing chunk 3 of 3..."),
            msg(5, 5, "Combining chunk summaries..."),
            msg(5, 5, "Session ready."),
        ]
    );
    let states: Vec<_> = h.events.progress().iter().map(|p| p.state).collect();
    assert_eq!(states.last(), Some(&SessionStatus::Done));
    assert!(states[..states.len() - 1]
        .iter()
        .all(|&s| s == SessionStatus::Summarizing));
    h.assert_rows_written_first();

    // Chunks decode as one batch, then one combine pass, with the exact
    // Electron prompts.
    let calls = h.calls();
    assert_eq!(calls.len(), 2);
    let chunks: Vec<String> = [0..100, 100..200, 200..250]
        .into_iter()
        .map(|r| r.map(|i| format!("w{i}")).collect::<Vec<_>>().join(" "))
        .collect();
    let expected_chunks: Vec<_> = chunks
        .iter()
        .map(|c| Generation {
            prompt: chunk_prompt(&h.base, c),
            max_tokens: CHUNK_NOTE_TOKENS,
        })
        .collect();
    assert_eq!(calls[0], expected_chunks);
    let notes: Vec<String> = (0..3).map(|seq| reply(seq, 5)).collect();
    assert_eq!(calls[1].len(), 1);
    assert_eq!(calls[1][0].prompt, combine_prompt(&h.base, &notes));

    let session = h.get(&id);
    assert_eq!(session.status, SessionStatus::Done);
    assert_eq!(session.error, None);
    assert_eq!(session.summary, reply(0, 5));
    assert_eq!(session.prompt_id.as_deref(), Some(PROMPT_ID));
    assert_eq!(session.model_id.as_deref(), Some("fake-llm"));
    assert_eq!(session.transcript, transcript);
    // Only the final pass streams, and the stream adds up to what is saved.
    assert_eq!(h.events.streamed(&id), session.summary);
}

#[test]
fn cancel_mid_chunk_stops_fast_and_restores_the_previous_summary() {
    let h = Harness::with_budget(100, |f| {
        f.reply_tokens = 10_000;
        f.token_delay = Duration::from_millis(20);
    });
    let transcript = words(250);
    let id = h.session(&transcript, "previous summary");

    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.events
        .wait_for(|p| p.message == "Summarizing chunk 1 of 3...");
    std::thread::sleep(Duration::from_millis(100));

    let asked = Instant::now();
    assert!(h.queue.cancel(&id));
    h.wait_idle();
    assert!(
        asked.elapsed() < Duration::from_secs(1),
        "{:?}",
        asked.elapsed()
    );

    let session = h.get(&id);
    assert_eq!(session.status, SessionStatus::Cancelled);
    assert_eq!(session.summary, "previous summary");
    assert_eq!(session.transcript, transcript);
    assert_eq!(session.error, None);
    let last = h.events.progress().pop().unwrap();
    assert_eq!(last.state, SessionStatus::Cancelled);
    assert_eq!(last.message, "Summarization stopped.");
    h.assert_rows_written_first();
}

#[test]
fn one_chunk_skips_the_combine_pass() {
    let h = Harness::with_budget(100, |_| {});
    let transcript = words(60);
    let id = h.session(&transcript, "");

    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.wait_idle();

    assert_eq!(
        h.events.messages(&id),
        [
            msg(1, 1, "Loading summary model..."),
            msg(2, 2, "Generating summary..."),
            msg(2, 2, "Session ready."),
        ]
    );
    let calls = h.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].len(), 1);
    assert_eq!(calls[0][0].prompt, chunk_prompt(&h.base, &transcript));
    let session = h.get(&id);
    assert_eq!(session.status, SessionStatus::Done);
    assert_eq!(session.summary, reply(0, 5));
    assert_eq!(session.timings.unwrap().combine_ms, None);
    assert_eq!(h.events.streamed(&id), session.summary);
}

#[test]
fn under_ten_words_fails_without_touching_the_model() {
    let h = Harness::with_budget(100, |_| {});
    let id = h.session("only nine words in this one so it fails", "kept");

    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.wait_idle();

    let session = h.get(&id);
    assert_eq!(session.status, SessionStatus::Failed);
    let message = "Audio capture insufficient. Please record a bit more.";
    assert_eq!(session.error.as_deref(), Some(message));
    assert_eq!(session.summary, "kept");
    assert_eq!(h.loader.loads.load(Ordering::SeqCst), 0);
    assert!(h.calls().is_empty());
    let last = h.events.progress().pop().unwrap();
    assert_eq!(
        (last.state, last.message.as_str()),
        (SessionStatus::Failed, message)
    );
    h.assert_rows_written_first();
}

#[test]
fn a_second_regenerate_waits_for_the_first() {
    let h = Harness::with_budget(100, |f| f.token_delay = Duration::from_millis(20));
    let a = h.session(&words(250), "");
    let b = h.session(&words(250), "");

    assert_eq!(h.queue.while_idle(|| 7), Some(7));
    h.queue.regenerate(&a, PROMPT_ID).unwrap();
    h.events
        .wait_for(|p| p.session_id == a && p.state == SessionStatus::Summarizing);
    assert!(h.queue.while_idle(|| ()).is_none());
    h.queue.regenerate(&b, PROMPT_ID).unwrap();
    h.wait_idle();

    let summarizer = &h.loader.summarizer;
    assert_eq!(summarizer.max_active.load(Ordering::SeqCst), 1);
    assert_eq!(h.get(&a).status, SessionStatus::Done);
    assert_eq!(h.get(&b).status, SessionStatus::Done);
    // Every event for `a` comes before the first one for `b`.
    let order: Vec<_> = h
        .events
        .progress()
        .into_iter()
        .map(|p| p.session_id)
        .collect();
    let first_b = order.iter().position(|s| *s == b).unwrap();
    assert!(order[..first_b].iter().all(|s| *s == a));
    assert!(order[first_b..].iter().all(|s| *s == b));
    // The model loads once and stays loaded for the next job.
    assert_eq!(h.loader.loads.load(Ordering::SeqCst), 1);
}

#[test]
fn whisper_load_does_not_wait_for_a_summary_waiting_on_its_download() {
    let h = Harness::with_loader(
        100,
        |_| {},
        |loader| {
            loader.ready.store(false, Ordering::SeqCst);
        },
    );
    let id = h.session(&words(20), "");
    h.queue.regenerate(&id, PROMPT_ID).unwrap();

    assert_eq!(
        h.queue.while_transcription_safe(
            &AtomicBool::new(false),
            || panic!("idle queue waited"),
            || "loaded"
        ),
        Some("loaded")
    );
    assert_eq!(h.get(&id).status, SessionStatus::AwaitingModel);

    h.loader.ready.store(true, Ordering::SeqCst);
    h.wait_idle();
}

#[test]
fn whisper_load_waits_for_a_running_summary_but_stops_when_cancelled() {
    let h = Harness::with_budget(100, |fake| {
        fake.token_delay = Duration::from_millis(300);
    });
    let id = h.session(&words(20), "");
    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.events
        .wait_for(|p| p.session_id == id && p.state == SessionStatus::Summarizing);

    let cancelled = Arc::new(AtomicBool::new(false));
    std::thread::scope(|scope| {
        let (wait_tx, wait_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker_cancel = Arc::clone(&cancelled);
        let queue = &h.queue;
        let callback_queue = &h.queue;
        scope.spawn(move || {
            let result = queue.while_transcription_safe(
                &worker_cancel,
                || {
                    assert!(callback_queue.is_busy());
                    wait_tx.send(()).unwrap();
                },
                || "loaded",
            );
            done_tx.send(result).unwrap();
        });
        wait_rx.recv_timeout(WAIT).unwrap();
        cancelled.store(true, Ordering::Relaxed);
        assert_eq!(done_rx.recv_timeout(WAIT).unwrap(), None);
    });
    h.wait_idle();
}

#[test]
fn whisper_load_wait_callback_runs_and_operation_resumes_after_summary() {
    let h = Harness::with_budget(100, |fake| {
        fake.token_delay = Duration::from_millis(100);
    });
    let id = h.session(&words(20), "");
    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.events
        .wait_for(|p| p.session_id == id && p.state == SessionStatus::Summarizing);

    let (wait_tx, wait_rx) = mpsc::channel();
    let result = h.queue.while_transcription_safe(
        &AtomicBool::new(false),
        || wait_tx.send(()).unwrap(),
        || "loaded",
    );
    assert!(wait_rx.recv_timeout(WAIT).is_ok());
    assert_eq!(result, Some("loaded"));
    assert_eq!(h.get(&id).status, SessionStatus::Done);
}

#[test]
fn whisper_load_wait_callback_runs_for_ready_pending_job_and_active_whisper_load() {
    let h = Harness::with_budget(100, |_| {});
    let id = h.session(&words(20), "");
    let queue = &h.queue;
    let (load_tx, load_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            assert_eq!(
                queue.while_transcription_safe(
                    &AtomicBool::new(false),
                    || {},
                    || {
                        load_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                    }
                ),
                Some(())
            );
        });
        load_rx.recv_timeout(WAIT).unwrap();
        h.queue.regenerate(&id, PROMPT_ID).unwrap();

        let (wait_tx, wait_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let queue = &h.queue;
        scope.spawn(move || {
            let result = queue.while_transcription_safe(
                &AtomicBool::new(false),
                || wait_tx.send(()).unwrap(),
                || "ready",
            );
            done_tx.send(result).unwrap();
        });
        wait_rx.recv_timeout(WAIT).unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(done_rx.recv_timeout(WAIT).unwrap(), Some("ready"));
    });
    assert!(h.queue.wait_idle(WAIT));
}

#[test]
fn whisper_load_only_wait_does_not_call_wait_notice() {
    let h = Harness::with_budget(100, |_| {});
    let queue = &h.queue;
    std::thread::scope(|scope| {
        let (load_tx, load_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        scope.spawn(move || {
            assert_eq!(
                queue.while_transcription_safe(
                    &AtomicBool::new(false),
                    || panic!("no summary is waiting"),
                    || {
                        load_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                    }
                ),
                Some(())
            );
        });
        load_rx.recv_timeout(WAIT).unwrap();

        let (notice_tx, notice_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let queue = &h.queue;
        scope.spawn(move || {
            let result = queue.while_transcription_safe(
                &AtomicBool::new(false),
                || notice_tx.send(()).unwrap(),
                || "ready",
            );
            done_tx.send(result).unwrap();
        });
        release_tx.send(()).unwrap();
        assert_eq!(done_rx.recv_timeout(WAIT).unwrap(), Some("ready"));
        assert!(notice_rx.try_recv().is_err());
    });
}

#[test]
fn summary_execution_waits_until_whisper_loading_finishes() {
    let h = Harness::with_budget(100, |_| {});
    let id = h.session(&words(20), "");
    let cancel = AtomicBool::new(false);

    std::thread::scope(|scope| {
        let (loading_tx, loading_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let queue = &h.queue;
        scope.spawn(move || {
            assert_eq!(
                queue.while_transcription_safe(
                    &cancel,
                    || {},
                    || {
                        loading_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                    }
                ),
                Some(())
            );
        });
        loading_rx.recv_timeout(WAIT).unwrap();
        h.queue.preload();

        let (admitted_tx, admitted_rx) = mpsc::channel();
        let queue = &h.queue;
        let id = id.clone();
        scope.spawn(move || {
            queue.regenerate(&id, PROMPT_ID).unwrap();
            admitted_tx.send(()).unwrap();
        });
        admitted_rx.recv_timeout(WAIT).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert!(h.calls().is_empty(), "summary ran during Whisper loading");
        assert_eq!(
            h.loader.loads.load(Ordering::SeqCst),
            0,
            "summary preloaded during Whisper loading"
        );
        assert!(
            !h.queue.summarizing(),
            "summary started during Whisper loading"
        );
        release_tx.send(()).unwrap();
    });
    h.wait_idle();
}

#[test]
fn cancelling_a_queued_job_drops_it_without_running() {
    let h = Harness::with_budget(100, |f| {
        f.reply_tokens = 10_000;
        f.token_delay = Duration::from_millis(5);
    });
    let a = h.session(&words(250), "");
    let b = h.session(&words(250), "b before");

    h.queue.regenerate(&a, PROMPT_ID).unwrap();
    h.queue.regenerate(&b, PROMPT_ID).unwrap();
    assert!(h.queue.cancel(&b));
    assert!(h.queue.cancel(&a));
    h.wait_idle();

    assert!(h.events.progress().iter().all(|p| p.session_id == a));
    let b = h.get(&b);
    assert_eq!(b.status, SessionStatus::Done);
    assert_eq!(b.summary, "b before");
    assert!(!h.queue.cancel("no-such-session"));
}

#[test]
fn done_records_a_timing_for_every_stage() {
    let h = Harness::with_budget(100, |_| {});
    let id = h.session(&words(250), "");

    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.wait_idle();

    let t = h.get(&id).timings.unwrap();
    assert!(t.model_load_ms.is_some());
    assert_eq!(t.chunk_ms.len(), 3);
    assert!(t.combine_ms.is_some());
    assert!(t.save_ms.is_some());
    // A regenerate transcribes nothing.
    assert_eq!(t.tail_transcription_ms, None);
}

#[test]
fn model_load_failure_fails_the_session_with_its_message() {
    let h = Harness::with_loader(100, |_| {}, |l| l.fail = Some(LlmError::NotReady));
    let id = h.session(&words(50), "kept");

    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.wait_idle();

    let session = h.get(&id);
    assert_eq!(session.status, SessionStatus::Failed);
    assert_eq!(
        session.error.as_deref(),
        Some(LlmError::NotReady.to_string().as_str())
    );
    assert_eq!(session.summary, "kept");
    h.assert_rows_written_first();
}

#[test]
fn regenerate_rejects_a_session_still_recording_or_missing() {
    let h = Harness::with_budget(100, |_| {});
    let recording = h
        .store
        .create_session(&NewSession {
            title: "r".into(),
            input_mode: InputMode::Mic,
            prompt_id: None,
            model_id: None,
        })
        .unwrap();
    assert!(h.queue.regenerate(&recording.id, PROMPT_ID).is_err());
    assert!(h.queue.regenerate("missing", PROMPT_ID).is_err());
    assert!(h
        .queue
        .regenerate(&h.session(&words(20), ""), "no-such-prompt")
        .is_err());
}

/// Parity spec 03 AS8: a deleted style cannot be picked for a regenerate,
/// and a job queued with it falls back to the active style.
#[test]
fn as8_a_deleted_style_is_refused_then_falls_back_to_active() {
    let h = Harness::with_budget(100, |_| {});
    let x = h.store.create_prompt("X", "Summarize briefly.").unwrap();
    let id = h.session(&words(50), "old");

    h.queue.regenerate(&id, &x.id).unwrap();
    h.wait_idle();
    assert_eq!(h.get(&id).prompt_id.as_deref(), Some(x.id.as_str()));

    // Queued while the model is "downloading", deleted before it runs.
    h.loader.ready.store(false, Ordering::SeqCst);
    h.queue.regenerate(&id, &x.id).unwrap();
    h.store.delete_prompt(&x.id).unwrap();
    assert!(matches!(
        h.queue.regenerate(&id, &x.id),
        Err(JobError::PromptNotFound(p)) if p == x.id
    ));
    h.loader.ready.store(true, Ordering::SeqCst);
    h.wait_idle();

    let s = h.get(&id);
    assert_eq!(s.status, SessionStatus::Done);
    assert_eq!(s.prompt_id.as_deref(), Some(PROMPT_ID));
}

/// Parity spec 03 AS15: a changed summary model is loaded once, replaces
/// the cached one, and is recorded on the session.
#[test]
fn as15_switching_the_model_reloads_once_and_records_the_new_id() {
    let h = Harness::with_budget(100, |_| {});
    let id = h.session(&words(50), "");

    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.wait_idle();
    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.wait_idle();
    assert_eq!(
        h.loader.loads.load(Ordering::SeqCst),
        1,
        "cached model reused"
    );
    assert_eq!(h.get(&id).model_id.as_deref(), Some("fake-llm"));

    *h.loader.model.lock().unwrap() = "other-llm".into();
    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.wait_idle();
    h.queue.regenerate(&id, PROMPT_ID).unwrap();
    h.wait_idle();

    assert_eq!(h.loader.loads.load(Ordering::SeqCst), 2);
    assert_eq!(
        *h.loader.loaded_model_ids.lock().unwrap(),
        ["fake-llm", "other-llm"]
    );
    let s = h.get(&id);
    assert_eq!(s.status, SessionStatus::Done);
    assert_eq!(s.model_id.as_deref(), Some("other-llm"));
}
