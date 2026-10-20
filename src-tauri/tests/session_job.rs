//! U8: the batch session loop with fake audio and engines — chunks written
//! while recording, the Recording → Finalizing → Summarizing → Done path,
//! cancel during transcription, interrupted sessions, and source selection.
#![cfg(feature = "fake-engines")]

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

use scribular_lib::audio::Format;
use scribular_lib::engines::fake::{
    FakeLoader, FakeMicrophone, FakeSource, FakeSummarizer, FakeSystemAudio, FakeTranscriber,
    FakeTranscriberLoader,
};
use scribular_lib::jobs::{
    JobEvents, JobQueue, RecordError, Recorder, SessionProgress, SummaryDelta,
};
use scribular_lib::llm::LlmError;
use scribular_lib::store::{AudioSource, InputMode, Session, SessionStatus, Store};
use scribular_lib::stt::TranscriptSegment;

const WAIT: Duration = Duration::from_secs(10);
const APP: &str = "Zoom";
/// Native 48 kHz stereo: one second is 96,000 interleaved samples.
const NATIVE: Format = Format {
    sample_rate: 48_000,
    channels: 2,
};
const SECOND: usize = 96_000;
/// The fake segments, joined; 12 words clears the summary word floor.
const TRANSCRIPT: &str = "hello there, how are you today? I am fine, thanks for asking.";

/// A fresh database path, removed (with its WAL files) on drop.
struct TempDb(PathBuf);

impl TempDb {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "scribular-session-{}-{}.db",
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

/// Records progress events and checks that the status row already says
/// what each event says.
struct Events {
    store: Arc<Store>,
    progress: Mutex<Vec<SessionProgress>>,
    stale_rows: Mutex<Vec<String>>,
    changed: Condvar,
}

impl Events {
    fn states(&self, id: &str) -> Vec<SessionStatus> {
        let mut states: Vec<SessionStatus> = self
            .progress
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.session_id == id)
            .map(|p| p.state)
            .collect();
        states.dedup();
        states
    }

    fn wait_for(&self, id: &str, state: SessionStatus) {
        let events = self.progress.lock().unwrap();
        let (events, timeout) = self
            .changed
            .wait_timeout_while(events, WAIT, |e| {
                !e.iter().any(|p| p.session_id == id && p.state == state)
            })
            .unwrap();
        assert!(!timeout.timed_out(), "no {state:?} event in {:?}", *events);
    }
}

impl JobEvents for Events {
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

    fn summary_delta(&self, _: SummaryDelta) {}
}

fn segment(start_ms: i64, text: &str) -> TranscriptSegment {
    TranscriptSegment {
        start_ms,
        end_ms: start_ms + 1000,
        text: text.into(),
    }
}

struct Harness {
    db: TempDb,
    store: Arc<Store>,
    events: Arc<Events>,
    stt: Arc<FakeTranscriber>,
    summarizer: Arc<FakeLoader>,
    mic: Arc<FakeSource>,
    system: Arc<FakeSource>,
    queue: Arc<JobQueue>,
    recorder: Recorder,
}

impl Harness {
    fn new() -> Self {
        Self::with(|_| {}, |_| {})
    }

    fn with(
        configure_stt: impl FnOnce(&mut FakeTranscriber),
        configure_llm: impl FnOnce(&mut FakeLoader),
    ) -> Self {
        let db = TempDb::new();
        let store = Arc::new(Store::open(&db.0).unwrap());
        let events = Arc::new(Events {
            store: store.clone(),
            progress: Mutex::default(),
            stale_rows: Mutex::default(),
            changed: Condvar::new(),
        });
        let mut stt = FakeTranscriber::new(vec![
            segment(0, "hello there, how are you today?"),
            segment(1000, "I am fine, thanks for asking."),
        ]);
        configure_stt(&mut stt);
        let stt = Arc::new(stt);
        let mut summarizer = FakeLoader::new(FakeSummarizer::new(8192));
        configure_llm(&mut summarizer);
        let summarizer = Arc::new(summarizer);
        let queue = Arc::new(JobQueue::new(
            store.clone(),
            summarizer.clone(),
            events.clone(),
        ));
        let mic = Arc::new(FakeSource::new(NATIVE));
        let system = Arc::new(FakeSource::new(NATIVE));
        let recorder = Recorder::new(
            store.clone(),
            queue.clone(),
            Arc::new(FakeTranscriberLoader(stt.clone())),
            events.clone(),
            Arc::new(FakeMicrophone(mic.clone())),
            Arc::new(FakeSystemAudio {
                source: system.clone(),
                app: APP.into(),
            }),
        );
        Self {
            db,
            store,
            events,
            stt,
            summarizer,
            mic,
            system,
            queue,
            recorder,
        }
    }

    fn start(&self, mode: InputMode) -> String {
        let app = (mode != InputMode::Mic).then_some(APP);
        self.recorder.start(mode, app, "Session").unwrap().id
    }

    /// Feeds `seconds` of a 440 Hz tone to every open source.
    fn feed(&self, seconds: usize) {
        let tone: Vec<f32> = (0..SECOND / 2)
            .flat_map(|i| {
                let s = (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [s, s]
            })
            .collect();
        for _ in 0..seconds {
            self.mic.feed(&tone);
            self.system.feed(&tone);
        }
    }

    fn chunks(&self, id: &str) -> u32 {
        self.store.audio_chunk_count(id).unwrap()
    }

    fn wait_chunks(&self, id: &str, at_least: u32) {
        let deadline = Instant::now() + WAIT;
        while self.chunks(id) < at_least {
            assert!(Instant::now() < deadline, "only {} chunks", self.chunks(id));
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn get(&self, id: &str) -> Session {
        self.store.get_session(id).unwrap().unwrap()
    }

    fn assert_rows_matched_events(&self) {
        assert_eq!(
            *self.events.stale_rows.lock().unwrap(),
            Vec::<String>::new()
        );
    }
}

#[test]
fn a_session_records_transcribes_and_summarizes() {
    let h = Harness::new();
    let id = h.start(InputMode::Mic);
    assert_eq!(h.recorder.current().as_deref(), Some(id.as_str()));
    h.feed(12);
    // 12 s is two whole 5 s chunks before Stop; the rest flushes on Stop.
    h.wait_chunks(&id, 2);
    assert_eq!(h.get(&id).status, SessionStatus::Recording);

    assert_eq!(h.recorder.stop().unwrap(), id);
    assert_eq!(h.recorder.current(), None);
    h.events.wait_for(&id, SessionStatus::Done);
    assert!(h.queue.wait_idle(WAIT));

    let s = h.get(&id);
    assert_eq!(s.status, SessionStatus::Done);
    assert_eq!(s.transcript, TRANSCRIPT);
    assert_eq!(s.duration_ms, Some(12_000));
    assert!(!s.summary.is_empty());
    assert!(s.timings.unwrap().tail_transcription_ms.is_some());
    assert_eq!(h.chunks(&id), 0, "audio is deleted with the transcript");
    // All 12 s reached the transcriber as 16 kHz mono.
    assert_eq!(h.stt.calls(), vec![12 * 16_000]);
    assert_eq!(
        h.events.states(&id),
        [
            SessionStatus::Recording,
            SessionStatus::Finalizing,
            SessionStatus::Summarizing,
            SessionStatus::Done
        ]
    );
    h.assert_rows_matched_events();
}

#[test]
fn chunks_are_written_while_recording() {
    let h = Harness::new();
    let id = h.start(InputMode::Mic);
    assert_eq!(h.chunks(&id), 0);
    h.feed(5);
    h.wait_chunks(&id, 1);
    h.feed(5);
    h.wait_chunks(&id, 2);
    assert_eq!(h.get(&id).status, SessionStatus::Recording);
}

#[test]
fn quitting_mid_recording_leaves_an_interrupted_session_with_its_audio() {
    let h = Harness::new();
    let id = h.start(InputMode::MicAndSystem);
    h.feed(7);
    h.wait_chunks(&id, 2);
    let Harness {
        db,
        store,
        events,
        queue,
        recorder,
        ..
    } = h;
    drop(recorder);
    drop(queue);
    drop(events);
    drop(store);

    let store = Store::open(&db.0).unwrap();
    let s = store.get_session(&id).unwrap().unwrap();
    assert_eq!(s.status, SessionStatus::Failed);
    assert_eq!(s.error.as_deref(), Some("interrupted"));
    // Both sources' whole chunks plus their flushed 2 s tails.
    assert_eq!(store.audio_chunk_count(&id).unwrap(), 4);
    assert_eq!(
        store.audio_samples(&id, AudioSource::Mic).unwrap().len(),
        7 * 16_000
    );
}

#[test]
fn cancelling_transcription_keeps_the_partial_transcript() {
    // The system source transcribes; the mic source waits for the cancel.
    let h = Harness::with(|stt| stt.block_from = 1, |_| {});
    let id = h.start(InputMode::MicAndSystem);
    h.feed(3);
    h.recorder.stop().unwrap();
    let deadline = Instant::now() + WAIT;
    while h.stt.calls().len() < 2 {
        assert!(Instant::now() < deadline, "the mic was never transcribed");
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(h.get(&id).status, SessionStatus::Finalizing);
    assert!(h.recorder.cancel(&id));
    h.events.wait_for(&id, SessionStatus::Cancelled);

    let s = h.get(&id);
    assert_eq!(s.status, SessionStatus::Cancelled);
    assert_eq!(s.transcript, format!("Client: {TRANSCRIPT}"));
    assert_eq!(h.chunks(&id), 0);
    assert_eq!(h.summarizer.loads.load(Ordering::Relaxed), 0, "no summary");
    assert!(!h.recorder.cancel(&id), "nothing left to cancel");
    h.assert_rows_matched_events();
}

#[test]
fn each_mode_records_its_sources() {
    for (mode, expected) in [
        (InputMode::Mic, &[AudioSource::Mic][..]),
        (InputMode::System, &[AudioSource::System][..]),
        (
            InputMode::MicAndSystem,
            &[AudioSource::Mic, AudioSource::System][..],
        ),
    ] {
        let h = Harness::new();
        let id = h.start(mode);
        assert_eq!(
            h.mic.is_open(),
            expected.contains(&AudioSource::Mic),
            "{mode:?}"
        );
        assert_eq!(
            h.system.is_open(),
            expected.contains(&AudioSource::System),
            "{mode:?}"
        );
        h.feed(1);
        h.recorder.stop().unwrap();
        assert!(!h.mic.is_open() && !h.system.is_open(), "{mode:?}");
        h.events.wait_for(&id, SessionStatus::Done);
        for source in [AudioSource::Mic, AudioSource::System] {
            let segments = h.store.segments(&id, source).unwrap();
            assert_eq!(
                segments.len(),
                if expected.contains(&source) { 2 } else { 0 },
                "{mode:?} {source:?}"
            );
        }
        assert_eq!(h.stt.calls().len(), expected.len(), "{mode:?}");
    }
}

#[test]
fn a_failed_summary_keeps_the_transcript_and_deletes_the_audio() {
    let h = Harness::with(
        |_| {},
        |llm| llm.fail = Some(LlmError::Engine("out of memory".into())),
    );
    let id = h.start(InputMode::Mic);
    h.feed(6);
    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::Failed);

    let s = h.get(&id);
    assert_eq!(s.status, SessionStatus::Failed);
    assert_eq!(s.transcript, TRANSCRIPT);
    assert_eq!(h.chunks(&id), 0);
}

#[test]
fn a_lost_source_fails_the_recording_and_keeps_its_audio() {
    let h = Harness::new();
    let id = h.start(InputMode::MicAndSystem);
    h.feed(6);
    h.wait_chunks(&id, 2);
    h.system.lose();
    h.events.wait_for(&id, SessionStatus::Failed);

    let s = h.get(&id);
    assert_eq!(
        s.error.as_deref(),
        Some("recording stopped: the app's audio ended")
    );
    assert_eq!(h.recorder.current(), None);
    assert!(!h.mic.is_open(), "the other source is stopped too");
    // Both sources' 5 s chunks plus their flushed 1 s tails.
    assert_eq!(h.chunks(&id), 4);
    assert!(h.stt.calls().is_empty(), "nothing is transcribed");
    assert!(matches!(h.recorder.stop(), Err(RecordError::NotRecording)));
    h.assert_rows_matched_events();
}

#[test]
fn stopping_with_no_audio_fails_the_session() {
    let h = Harness::new();
    let id = h.start(InputMode::Mic);
    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::Failed);

    let s = h.get(&id);
    assert_eq!(s.error.as_deref(), Some("no audio was recorded"));
    assert!(h.stt.calls().is_empty());
    assert_eq!(h.summarizer.loads.load(Ordering::Relaxed), 0);
}

#[test]
fn a_refused_source_leaves_no_session() {
    let h = Harness::new();
    h.mic.deny.store(true, Ordering::Relaxed);
    let err = h
        .recorder
        .start(InputMode::MicAndSystem, Some(APP), "Session")
        .unwrap_err();
    assert!(err.to_string().contains("permission denied"), "{err}");
    assert!(
        !h.system.is_open(),
        "the system capture that started is stopped"
    );
    assert!(h.store.list_sessions().unwrap().is_empty());
    assert_eq!(h.recorder.current(), None);
}

#[test]
fn system_audio_needs_an_app_and_one_recording_at_a_time() {
    let h = Harness::new();
    assert!(matches!(
        h.recorder.start(InputMode::System, None, "Session"),
        Err(RecordError::NoTargetApp)
    ));
    assert!(matches!(h.recorder.stop(), Err(RecordError::NotRecording)));
    h.start(InputMode::Mic);
    assert!(matches!(
        h.recorder.start(InputMode::Mic, None, "Again"),
        Err(RecordError::AlreadyRecording)
    ));
}
