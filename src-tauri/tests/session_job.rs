//! U8/U9: the session loop with fake audio and engines — chunks written and
//! transcribed while recording, the Recording → Finalizing → Summarizing →
//! Done path, catching up, cancel during transcription, interrupted
//! sessions, source selection, and when each model loads.
#![cfg(feature = "fake-engines")]

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

use scribular_lib::audio::Format;
use scribular_lib::diarize::{SpeakerEmbedder, SpeakerEmbedderLoader};
use scribular_lib::engines::fake::{
    FakeLoader, FakeMicrophone, FakeSource, FakeSummarizer, FakeSystemAudio, FakeTranscriber,
    FakeTranscriberLoader,
};
use scribular_lib::jobs::{
    HearingState, JobEvents, JobQueue, RecordError, Recorder, RecordingState, RecordingStatus,
    SessionLevel, SessionProgress, SessionSegments, SummaryDelta, TranscriptWaiting,
};
use scribular_lib::llm::LlmError;
use scribular_lib::store::key::MemoryKeyStore;
use scribular_lib::store::{AudioSource, InputMode, NewSession, Session, SessionStatus, Store};
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
    recording_changes: Mutex<Vec<(bool, bool)>>,
    progress: Mutex<Vec<SessionProgress>>,
    recording_states: Mutex<Vec<Option<RecordingState>>>,
    levels: Mutex<Vec<SessionLevel>>,
    segments: Mutex<Vec<SessionSegments>>,
    waiting: Mutex<Vec<TranscriptWaiting>>,
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

    fn messages(&self, id: &str) -> Vec<String> {
        let mut messages: Vec<String> = self
            .progress
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.session_id == id)
            .map(|p| p.message.clone())
            .collect();
        messages.dedup();
        messages
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
    fn recording_changed(&self, recording: bool, multi_speaker: bool) {
        self.recording_changes
            .lock()
            .unwrap()
            .push((recording, multi_speaker));
    }

    fn recording_state(&self, state: Option<RecordingState>) {
        self.recording_states.lock().unwrap().push(state);
    }

    fn level(&self, level: SessionLevel) {
        self.levels.lock().unwrap().push(level);
    }

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

    fn segments(&self, s: SessionSegments) {
        self.segments.lock().unwrap().push(s);
    }

    fn transcript_waiting(&self, state: TranscriptWaiting) {
        self.waiting.lock().unwrap().push(state);
        self.changed.notify_all();
    }
}

fn wait_until(what: &str, done: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn segment(start_ms: i64, text: &str) -> TranscriptSegment {
    TranscriptSegment {
        start_ms,
        end_ms: start_ms + 1000,
        text: text.into(),
    }
}

struct Harness {
    store: Arc<Store>,
    events: Arc<Events>,
    stt: Arc<FakeTranscriber>,
    summarizer: Arc<FakeLoader>,
    mic: Arc<FakeSource>,
    system: Arc<FakeSource>,
    queue: Arc<JobQueue>,
    recorder: Recorder,
    /// Last: fields drop in order, so the files go after every connection.
    db: TempDb,
}

impl Harness {
    fn new() -> Self {
        Self::with(|_| {}, |_| {})
    }

    fn with(
        configure_stt: impl FnOnce(&mut FakeTranscriber),
        configure_llm: impl FnOnce(&mut FakeLoader),
    ) -> Self {
        Self::with_format(NATIVE, configure_stt, configure_llm)
    }

    fn with_format(
        format: Format,
        configure_stt: impl FnOnce(&mut FakeTranscriber),
        configure_llm: impl FnOnce(&mut FakeLoader),
    ) -> Self {
        Self::with_format_and_speakers(
            format,
            configure_stt,
            configure_llm,
            Arc::new(ErrorSpeakerLoader),
        )
    }

    fn with_speakers(speakers: Arc<dyn SpeakerEmbedderLoader>) -> Self {
        Self::with_format_and_speakers(NATIVE, |_| {}, |_| {}, speakers)
    }

    fn with_format_and_speakers(
        format: Format,
        configure_stt: impl FnOnce(&mut FakeTranscriber),
        configure_llm: impl FnOnce(&mut FakeLoader),
        speakers: Arc<dyn SpeakerEmbedderLoader>,
    ) -> Self {
        let db = TempDb::new();
        let store = Arc::new(Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap());
        let events = Arc::new(Events {
            store: store.clone(),
            recording_changes: Mutex::default(),
            progress: Mutex::default(),
            recording_states: Mutex::default(),
            levels: Mutex::default(),
            segments: Mutex::default(),
            waiting: Mutex::default(),
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
        let mic = Arc::new(FakeSource::new(format));
        let system = Arc::new(FakeSource::new(format));
        let recorder = Recorder::new_with_speakers(
            store.clone(),
            queue.clone(),
            Arc::new(FakeTranscriberLoader(stt.clone())),
            events.clone(),
            Arc::new(FakeMicrophone(mic.clone())),
            Arc::new(FakeSystemAudio {
                source: system.clone(),
                app: APP.into(),
            }),
            speakers,
        );
        Self {
            store,
            events,
            stt,
            summarizer,
            mic,
            system,
            queue,
            recorder,
            db,
        }
    }

    fn start(&self, mode: InputMode) -> String {
        let app = (mode == InputMode::MicAndSystem).then_some(APP);
        let id = self.recorder.start(mode, app, "Session").unwrap().id;
        if app.is_some() {
            wait_until("app audio opens", || self.system.is_open());
        }
        id
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

struct ErrorSpeakerLoader;
impl SpeakerEmbedderLoader for ErrorSpeakerLoader {
    fn load(&self) -> Result<Box<dyn SpeakerEmbedder>, String> {
        Err("speaker model isn't ready".into())
    }
}

struct EmbedGate {
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

impl EmbedGate {
    fn new() -> Self {
        Self {
            state: Mutex::new((false, false)),
            changed: Condvar::new(),
        }
    }

    fn wait_entered(&self) {
        let state = self.state.lock().unwrap();
        let (state, timeout) = self
            .changed
            .wait_timeout_while(state, WAIT, |s| !s.0)
            .unwrap();
        assert!(
            state.0 && !timeout.timed_out(),
            "diarization never reached embedding"
        );
    }

    fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}

struct BlockingSpeakerLoader(Arc<EmbedGate>);
impl SpeakerEmbedderLoader for BlockingSpeakerLoader {
    fn load(&self) -> Result<Box<dyn SpeakerEmbedder>, String> {
        Ok(Box::new(BlockingEmbedder(self.0.clone())))
    }
}

struct BlockingEmbedder(Arc<EmbedGate>);
impl SpeakerEmbedder for BlockingEmbedder {
    fn embed(&self, _: &[f32]) -> Result<Vec<f32>, String> {
        let mut state = self.0.state.lock().unwrap();
        state.0 = true;
        self.0.changed.notify_all();
        let (state, _) = self
            .0
            .changed
            .wait_timeout_while(state, WAIT, |state| !state.1)
            .unwrap();
        assert!(state.1, "test did not release speaker embedding");
        Ok(vec![1.0, 0.0])
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
fn low_memory_stream_announces_summary_wait_then_catches_up_without_losing_audio() {
    let h = Harness::with(
        |_| {},
        |loader| {
            Arc::get_mut(&mut loader.summarizer).unwrap().token_delay = Duration::from_millis(500);
        },
    );
    let first = h.start(InputMode::Mic);
    h.feed(12);
    h.wait_chunks(&first, 2);
    h.recorder.stop().unwrap();
    wait_until("summary running", || h.queue.summarizing());

    h.recorder.set_unload_whisper(true);
    let id = h.start(InputMode::MicAndSystem);
    h.feed(12);
    h.wait_chunks(&id, 2);
    wait_until("transcript waiting event", || {
        h.events
            .waiting
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.session_id == id && event.waiting)
    });
    assert_eq!(h.get(&id).status, SessionStatus::Recording);

    assert!(h.queue.wait_idle(WAIT));
    wait_until("transcript waiting clear", || {
        h.events
            .waiting
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.session_id == id && !event.waiting)
    });
    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::Done);
    let transcript = h.get(&id).transcript;
    assert!(transcript.contains("hello there, how are you today?"));
    assert!(transcript.contains("I am fine, thanks for asking."));
    assert!(h
        .events
        .segments
        .lock()
        .unwrap()
        .iter()
        .any(|event| event.session_id == id));
    assert_eq!(
        h.events
            .waiting
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.session_id == id)
            .map(|event| event.waiting)
            .collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn no_summary_wait_emits_no_transcript_waiting_event() {
    let h = Harness::new();
    h.recorder.set_unload_whisper(true);
    let id = h.start(InputMode::Mic);
    h.feed(12);
    h.wait_chunks(&id, 2);
    assert!(h.events.waiting.lock().unwrap().is_empty());
    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::Done);
    assert_eq!(h.get(&id).transcript, TRANSCRIPT);
}

#[test]
fn stopping_while_the_llm_is_downloading_saves_transcript_and_resumes() {
    let h = Harness::with(|_| {}, |loader| loader.ready.store(false, Ordering::SeqCst));
    let id = h.start(InputMode::Mic);
    h.feed(12);
    h.wait_chunks(&id, 2);

    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::AwaitingModel);
    let waiting = h.get(&id);
    assert_eq!(waiting.status, SessionStatus::AwaitingModel);
    assert_eq!(waiting.transcript, TRANSCRIPT);
    assert!(waiting.summary.is_empty());
    assert_eq!(h.chunks(&id), 0);
    assert!(!h.recorder.quit_confirmation_needed());

    h.summarizer.ready.store(true, Ordering::SeqCst);
    assert!(h.queue.wait_idle(WAIT));
    assert_eq!(h.get(&id).status, SessionStatus::Done);
    assert_eq!(
        h.events.states(&id),
        [
            SessionStatus::Recording,
            SessionStatus::Finalizing,
            SessionStatus::AwaitingModel,
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
fn quit_confirmation_only_for_recording_and_running_summary() {
    let h = Harness::with(
        |_| {},
        |loader| {
            Arc::get_mut(&mut loader.summarizer).unwrap().token_delay = Duration::from_millis(500);
        },
    );
    assert!(!h.recorder.quit_confirmation_needed());
    let id = h.start(InputMode::Mic);
    assert!(h.recorder.quit_confirmation_needed());
    h.feed(12);
    h.wait_chunks(&id, 2);
    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::Summarizing);
    assert!(h.recorder.quit_confirmation_needed());
    assert!(h.queue.wait_idle(WAIT));
    assert_eq!(h.get(&id).status, SessionStatus::Done);
    assert!(!h.recorder.quit_confirmation_needed());
}

#[test]
fn quitting_while_paused_leaves_the_same_interrupted_session_with_its_audio() {
    let h = Harness::new();
    let id = h.start(InputMode::MicAndSystem);
    h.feed(7);
    h.wait_chunks(&id, 2);
    h.recorder.pause().unwrap();
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

    let store = Store::open(&db.0, &MemoryKeyStore::with_key([7; 32])).unwrap();
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
fn shutdown_is_idempotent_and_preserves_the_recording_for_recovery() {
    let h = Harness::new();
    let id = h.start(InputMode::Mic);
    h.feed(10);
    h.wait_chunks(&id, 2);

    h.recorder.shutdown();
    h.recorder.shutdown();

    assert_eq!(h.get(&id).status, SessionStatus::Recording);
    assert_eq!(
        h.store.audio_samples(&id, AudioSource::Mic).unwrap().len(),
        10 * 16_000
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
    assert!(h.summarizer.summarizer.calls().is_empty(), "no summary");
    assert!(!h.recorder.cancel(&id), "nothing left to cancel");
    h.assert_rows_matched_events();
}

#[test]
fn cancelling_during_speaker_identification_saves_plain_transcript() {
    let gate = Arc::new(EmbedGate::new());
    let h = Harness::with_speakers(Arc::new(BlockingSpeakerLoader(gate.clone())));
    let mut settings = h.store.recording_settings().unwrap();
    settings.multi_speaker = true;
    h.store.set_recording_settings(&settings).unwrap();
    let id = h.start(InputMode::Mic);
    h.feed(3);
    h.recorder.stop().unwrap();

    gate.wait_entered();
    assert!(h.recorder.cancel(&id));
    gate.release();
    h.events.wait_for(&id, SessionStatus::Cancelled);

    let session = h.get(&id);
    assert_eq!(session.status, SessionStatus::Cancelled);
    assert_eq!(session.transcript, TRANSCRIPT);
    assert_eq!(session.raw_transcript, None);
    assert_eq!(
        *h.events.recording_changes.lock().unwrap(),
        [(true, true), (false, false)]
    );
    assert!(
        h.summarizer.summarizer.calls().is_empty(),
        "cancelled sessions are not summarized"
    );
}

#[test]
fn cancelling_reaches_a_window_still_transcribing_from_the_recording() {
    let h = Harness::with(|stt| stt.block_from = 0, |_| {});
    let id = h.start(InputMode::Mic);
    h.feed(16);
    wait_until("a window is in flight", || h.stt.calls().len() == 1);
    wait_until("the first window is in flight", || h.stt.calls().len() == 1);
    h.recorder.stop().unwrap();
    assert!(h.recorder.cancel(&id));
    h.events.wait_for(&id, SessionStatus::Cancelled);
    assert_eq!(h.get(&id).status, SessionStatus::Cancelled);
    assert!(h.summarizer.summarizer.calls().is_empty(), "no summary");
    h.assert_rows_matched_events();
}

#[test]
fn each_mode_records_its_sources() {
    for (mode, expected) in [
        (InputMode::Mic, &[AudioSource::Mic][..]),
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
        h.feed(2);
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
fn microphone_loss_interrupts_and_the_same_session_recovers() {
    let h = Harness::new();
    let id = h.start(InputMode::MicAndSystem);
    h.feed(6);
    h.wait_chunks(&id, 2);
    h.mic.lose();
    h.events.wait_for(&id, SessionStatus::Failed);

    assert_eq!(
        *h.events.recording_changes.lock().unwrap(),
        [(true, false), (false, false)]
    );
    assert_eq!(h.get(&id).error.as_deref(), Some("interrupted"));
    assert!(h
        .events
        .messages(&id)
        .last()
        .unwrap()
        .contains("microphone was lost"));
    assert_eq!(h.recorder.current(), None);
    assert!(!h.system.is_open());
    assert_eq!(
        h.chunks(&id),
        4,
        "both full chunks and flushed tails survive"
    );
    assert!(matches!(h.recorder.stop(), Err(RecordError::NotRecording)));
    h.recorder.recover(&id).unwrap();
    h.events.wait_for(&id, SessionStatus::Done);
    assert!(!h.get(&id).summary.is_empty());
    h.assert_rows_matched_events();
}

#[test]
fn app_audio_attaches_late_and_reattaches_on_the_mic_timeline() {
    let h = Harness::new();
    h.store.set_setting("keep_audio", "1").unwrap();
    h.system.available.store(false, Ordering::Relaxed);
    let id = h
        .recorder
        .start(InputMode::MicAndSystem, Some(APP), "Session")
        .unwrap()
        .id;
    assert!(h.mic.is_open());
    h.feed(6);
    h.wait_chunks(&id, 1);
    wait_until("the unattached call meter is silent", || {
        h.events.levels.lock().unwrap().iter().any(|e| {
            e.source == AudioSource::System && e.hearing == HearingState::Silent && e.level == 0.0
        })
    });

    h.system.available.store(true, Ordering::Relaxed);
    wait_until("first app attachment", || h.system.is_open());
    h.feed(3);
    wait_until("app meter hears audio", || {
        h.events.levels.lock().unwrap().iter().any(|e| {
            e.source == AudioSource::System && e.hearing == HearingState::Hearing && e.level > 0.0
        })
    });
    h.system.available.store(false, Ordering::Relaxed);
    h.system.lose();
    let samples = |source| h.store.audio_samples(&id, source).unwrap();
    wait_until("app tail committed", || {
        samples(AudioSource::System).len() == 9 * 16_000
    });
    assert!(h.mic.is_open());
    h.feed(6);
    h.system.available.store(true, Ordering::Relaxed);
    wait_until("app reattachment", || h.system.is_open());
    h.feed(3);
    assert_eq!(h.recorder.stop().unwrap(), id);
    h.events.wait_for(&id, SessionStatus::Done);

    let mic = samples(AudioSource::Mic);
    let app = samples(AudioSource::System);
    assert_eq!(mic.len(), 18 * 16_000);
    assert_eq!(app.len(), mic.len());
    let silent = |s: &[i16]| s.iter().all(|&sample| sample == 0);
    assert!(!silent(&mic[..6 * 16_000]));
    assert!(!silent(&mic[9 * 16_000..15 * 16_000]));
    assert!(silent(&app[..6 * 16_000]));
    assert!(!silent(&app[6 * 16_000..9 * 16_000]));
    assert!(silent(&app[9 * 16_000..15 * 16_000]));
    assert!(!silent(&app[15 * 16_000..]));
    assert_eq!(h.get(&id).duration_ms, Some(18_000));
    let starts: Vec<_> = h
        .store
        .segments(&id, AudioSource::System)
        .unwrap()
        .into_iter()
        .map(|s| s.start_ms)
        .collect();
    assert_eq!(starts, [6_000, 7_000, 15_000, 16_000]);
    assert_eq!(h.store.session_count().unwrap(), 1);
    assert_eq!(h.system.opened.load(Ordering::Relaxed), 2);
    assert!(h
        .events
        .levels
        .lock()
        .unwrap()
        .iter()
        .all(|e| { e.session_id == id && (0.0..=1.0).contains(&e.level) }));
}

#[test]
fn pause_freezes_capture_and_live_transcription_and_stop_accepts_paused_state() {
    let h = Harness::new();
    assert!(matches!(h.recorder.pause(), Err(RecordError::NotRecording)));
    assert!(matches!(
        h.recorder.resume(),
        Err(RecordError::NotRecording)
    ));
    h.store.set_setting("keep_audio", "1").unwrap();
    let id = h.start(InputMode::Mic);
    h.recorder.resume().unwrap();
    h.stt.hold.store(true, Ordering::SeqCst);
    h.feed(16);
    wait_until("Whisper starts a live window", || h.stt.calls().len() == 1);
    h.recorder.pause().unwrap();
    h.recorder.pause().unwrap();
    let paused = h.recorder.recording_state().unwrap();
    assert_eq!(paused.status, RecordingStatus::Paused);
    assert_eq!(paused.session_id, id);
    assert_eq!(paused.target_app, None);
    assert!(!h.mic.feed(&[0.5; SECOND]));
    h.stt.hold.store(false, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(h.recorder.recording_state(), Some(paused.clone()));
    assert_eq!(h.stt.calls().len(), 1);
    assert!(h.events.segments.lock().unwrap().is_empty());
    assert_eq!(
        h.store.audio_samples(&id, AudioSource::Mic).unwrap().len(),
        16 * 16_000
    );
    assert!(h
        .events
        .levels
        .lock()
        .unwrap()
        .iter()
        .all(|e| e.source == AudioSource::Mic));

    h.recorder.resume().unwrap();
    h.recorder.resume().unwrap();
    wait_until("the interrupted window is retried", || {
        h.stt.calls().len() >= 2
    });
    h.feed(3);
    h.recorder.pause().unwrap();
    assert_eq!(h.recorder.stop().unwrap(), id);
    h.events.wait_for(&id, SessionStatus::Done);
    assert_eq!(h.get(&id).duration_ms, Some(19_000));
    assert_eq!(
        h.store.audio_samples(&id, AudioSource::Mic).unwrap().len(),
        19 * 16_000
    );
    let starts: Vec<_> = h
        .store
        .segments(&id, AudioSource::Mic)
        .unwrap()
        .into_iter()
        .map(|s| s.start_ms)
        .collect();
    assert!(starts.contains(&16_000));
    assert_eq!(h.recorder.recording_state(), None);
    assert_eq!(h.mic.opened.load(Ordering::Relaxed), 2);
    let states: Vec<_> = h
        .events
        .recording_states
        .lock()
        .unwrap()
        .iter()
        .map(|s| s.as_ref().map(|s| s.status))
        .collect();
    assert_eq!(
        states,
        [
            Some(RecordingStatus::Running),
            Some(RecordingStatus::Paused),
            Some(RecordingStatus::Running),
            Some(RecordingStatus::Paused),
            None,
        ]
    );
}

#[test]
fn pause_suspends_app_attempts_and_failed_app_resume_keeps_mic_running() {
    let h = Harness::new();
    let id = h.start(InputMode::MicAndSystem);
    h.feed(2);
    h.recorder.pause().unwrap();
    assert!(!h.mic.is_open() && !h.system.is_open());
    let paused = h.recorder.recording_state().unwrap();
    assert_eq!(paused.target_app.as_deref(), Some(APP));
    assert_eq!(paused.session_id, id);
    h.system.deny.store(true, Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(h.system.opened.load(Ordering::Relaxed), 1);
    assert_eq!(h.recorder.recording_state(), Some(paused));
    h.recorder.resume().unwrap();
    h.feed(3);
    assert!(h.mic.is_open() && !h.system.is_open());
    assert_eq!(h.recorder.stop().unwrap(), id);
    h.events.wait_for(&id, SessionStatus::Done);
    assert_eq!(h.get(&id).duration_ms, Some(5_000));
    assert_eq!(h.store.session_count().unwrap(), 1);
}

#[test]
fn a_failed_mic_resume_keeps_paused_audio_recoverable() {
    let h = Harness::new();
    let id = h.start(InputMode::Mic);
    h.feed(3);
    h.recorder.pause().unwrap();
    h.mic.deny.store(true, Ordering::Relaxed);
    assert!(matches!(h.recorder.resume(), Err(RecordError::Audio(_))));
    assert_eq!(h.recorder.recording_state(), None);
    assert_eq!(h.get(&id).error.as_deref(), Some("interrupted"));
    assert_eq!(
        h.store.audio_samples(&id, AudioSource::Mic).unwrap().len(),
        3 * 16_000
    );
    h.recorder.recover(&id).unwrap();
    h.events.wait_for(&id, SessionStatus::Done);
}

#[test]
fn mic_starts_without_waiting_for_an_in_flight_app_attach() {
    use scribular_lib::audio::{self, AudioApp, AudioError, Capture, Sink, SystemAudio};

    #[derive(Default)]
    struct PendingApp {
        attempts: AtomicU32,
        cancelled: AtomicU32,
    }

    impl SystemAudio for PendingApp {
        fn list_apps(&self) -> audio::Result<Vec<AudioApp>> {
            Ok(Vec::new())
        }

        fn open(&self, _: &str, _: Sink, cancel: &AtomicBool) -> audio::Result<Box<dyn Capture>> {
            self.attempts.fetch_add(1, Ordering::Relaxed);
            let deadline = Instant::now() + WAIT;
            while !cancel.load(Ordering::Relaxed) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
            if cancel.load(Ordering::Relaxed) {
                self.cancelled.fetch_add(1, Ordering::Relaxed);
            }
            Err(AudioError::Device(
                "fake attachment did not complete".into(),
            ))
        }
    }

    let h = Harness::new();
    let app = Arc::new(PendingApp::default());
    let recorder = Recorder::new(
        h.store.clone(),
        h.queue.clone(),
        Arc::new(FakeTranscriberLoader(h.stt.clone())),
        h.events.clone(),
        Arc::new(FakeMicrophone(h.mic.clone())),
        app.clone(),
    );
    let id = recorder
        .start(InputMode::MicAndSystem, Some(APP), "Session")
        .unwrap()
        .id;
    wait_until("the pending attach starts", || {
        app.attempts.load(Ordering::Relaxed) == 1
    });
    assert!(h.mic.is_open());
    h.feed(2);
    recorder.pause().unwrap();
    assert_eq!(app.cancelled.load(Ordering::Relaxed), 1);
    assert!(!h.mic.is_open());
    recorder.resume().unwrap();
    wait_until("resume retries attachment", || {
        app.attempts.load(Ordering::Relaxed) == 2
    });
    h.feed(2);
    assert_eq!(recorder.stop().unwrap(), id);
    assert_eq!(app.cancelled.load(Ordering::Relaxed), 2);
    h.events.wait_for(&id, SessionStatus::Done);
    assert_eq!(h.get(&id).duration_ms, Some(4_000));
}

#[test]
fn capture_faults_log_metadata_and_keep_committed_audio() {
    use scribular_lib::diag::log;
    use scribular_lib::store::key::apply_key;

    let h = Harness::new();
    let log_dir = h.db.0.with_extension("logs");
    log::init(&log_dir).unwrap();
    let app = "PRIVATE-CALL-TARGET";
    let id = h
        .recorder
        .start(InputMode::MicAndSystem, Some(app), "PRIVATE-SESSION-TITLE")
        .unwrap()
        .id;
    h.feed(5);
    h.wait_chunks(&id, 1);
    let read_log = || std::fs::read_to_string(log_dir.join("scribular.log")).unwrap();
    wait_until("app attach failure reported", || {
        read_log().contains("app_source_unavailable")
    });

    let connection = rusqlite::Connection::open(&h.db.0).unwrap();
    apply_key(&connection, &[7; 32]).unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER fail_chunk BEFORE INSERT ON audio_chunks
         WHEN NEW.seq = 1 BEGIN SELECT RAISE(FAIL, 'PRIVATE-STORE-DETAIL'); END;",
        )
        .unwrap();
    h.feed(2);
    assert!(matches!(h.recorder.pause(), Err(RecordError::Interrupted)));
    assert_eq!(h.get(&id).error.as_deref(), Some("interrupted"));
    assert_eq!(h.chunks(&id), 1);
    assert_eq!(h.recorder.recording_state(), None);
    assert!(read_log().contains("database_error"));
    let diagnostics = read_log();
    for private in [
        app,
        "PRIVATE-SESSION-TITLE",
        "PRIVATE-STORE-DETAIL",
        TRANSCRIPT,
    ] {
        assert!(!diagnostics.contains(private));
    }
    h.recorder.recover(&id).unwrap();
    h.events.wait_for(&id, SessionStatus::Done);
    assert!(!h.get(&id).summary.is_empty());
    std::fs::remove_dir_all(log_dir).unwrap();
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
    assert_eq!(err.to_string(), "microphone permission denied");
    assert!(
        !h.system.is_open(),
        "the system capture that started is stopped"
    );
    assert!(h.store.list_sessions().unwrap().is_empty());
    assert_eq!(h.recorder.current(), None);
}

#[test]
fn refused_app_audio_leaves_capture_and_summary_available() {
    let h = Harness::new();
    h.system.deny.store(true, Ordering::Relaxed);
    let id = h
        .recorder
        .start(InputMode::MicAndSystem, Some(APP), "Session")
        .unwrap()
        .id;
    assert!(h.mic.is_open());
    h.feed(3);
    assert_eq!(h.recorder.stop().unwrap(), id);
    h.events.wait_for(&id, SessionStatus::Done);
    assert!(h.get(&id).transcript.contains(TRANSCRIPT));
    assert!(!h.get(&id).summary.is_empty());
    assert!(h
        .store
        .segments(&id, AudioSource::System)
        .unwrap()
        .is_empty());
}

#[test]
fn a_zero_sample_rate_is_refused() {
    let zero = Format {
        sample_rate: 0,
        channels: 2,
    };
    let h = Harness::with_format(zero, |_| {}, |_| {});
    let err = h
        .recorder
        .start(InputMode::Mic, None, "Session")
        .unwrap_err();
    assert!(err.to_string().contains("0 Hz"), "{err}");
    assert!(!h.mic.is_open(), "the capture is stopped");
    assert!(h.store.list_sessions().unwrap().is_empty());
}

#[test]
fn a_call_needs_an_app_and_system_only_is_rejected() {
    let h = Harness::new();
    assert!(matches!(
        h.recorder.start(InputMode::System, Some(APP), "Session"),
        Err(RecordError::UnsupportedMode)
    ));
    assert!(matches!(
        h.recorder
            .start(InputMode::MicAndSystem, Some(" "), "Session"),
        Err(RecordError::NoTargetApp)
    ));
    assert!(matches!(h.recorder.stop(), Err(RecordError::NotRecording)));
    assert!(h.store.list_sessions().unwrap().is_empty());
    h.start(InputMode::Mic);
    assert!(matches!(
        h.recorder.start(InputMode::Mic, None, "Again"),
        Err(RecordError::AlreadyRecording)
    ));
}

const WINDOW_CALLS: [usize; 5] = [
    15 * 16_000,
    20 * 16_000,
    20 * 16_000,
    20 * 16_000,
    7 * 16_000,
];

fn loads(h: &Harness) -> usize {
    h.stt.loads.load(Ordering::SeqCst)
}

#[test]
fn a_slow_transcriber_catches_up_without_dropping_audio() {
    let h = Harness::new();
    h.stt.hold.store(true, Ordering::SeqCst);
    let id = h.start(InputMode::Mic);
    h.feed(62);
    h.wait_chunks(&id, 12);
    h.stt.hold.store(false, Ordering::SeqCst);
    // Three windows queued behind the first, then all done while recording.
    wait_until("the queue drains", || h.stt.calls().len() == 4);
    wait_until("caught up", || {
        h.events.messages(&id).last().map(String::as_str) == Some("Recording...")
    });
    assert_eq!(
        h.events.messages(&id),
        ["Recording...", "Catching up...", "Recording..."]
    );

    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::Done);
    // 0–15, 10–30, 25–45, 40–60, then the 55–62 tail at Stop.
    assert_eq!(h.stt.calls(), WINDOW_CALLS);
    assert_eq!(h.get(&id).duration_ms, Some(62_000));
    h.assert_rows_matched_events();
}

#[test]
fn stop_drains_queued_windows_while_the_summary_model_loads() {
    let h = Harness::new();
    h.stt.hold.store(true, Ordering::SeqCst);
    let id = h.start(InputMode::Mic);
    h.feed(62);
    h.wait_chunks(&id, 12);
    h.recorder.stop().unwrap();
    assert_eq!(h.get(&id).status, SessionStatus::Finalizing);
    // The summary model loads at Stop, while whisper is still busy.
    wait_until("the summary model loads", || {
        h.summarizer.loads.load(Ordering::SeqCst) == 1
    });
    assert_eq!(h.get(&id).status, SessionStatus::Finalizing);
    assert_eq!(h.stt.calls().len(), 1);

    h.stt.hold.store(false, Ordering::SeqCst);
    h.events.wait_for(&id, SessionStatus::Done);
    assert!(h.queue.wait_idle(WAIT));
    assert_eq!(h.stt.calls(), WINDOW_CALLS);
    assert_eq!(h.summarizer.loads.load(Ordering::SeqCst), 1, "loaded once");
    assert_eq!(h.get(&id).timings.unwrap().model_load_ms, Some(0));
    h.assert_rows_matched_events();
}

#[test]
fn segments_are_saved_and_emitted_in_recording_time() {
    let h = Harness::new();
    let id = h.start(InputMode::Mic);
    h.feed(32);
    wait_until("two windows", || {
        h.events.segments.lock().unwrap().len() == 2
    });
    assert_eq!(h.get(&id).status, SessionStatus::Recording);
    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::Done);

    let starts: Vec<i64> = h
        .events
        .segments
        .lock()
        .unwrap()
        .iter()
        .inspect(|e| {
            assert_eq!(
                (e.session_id.as_str(), e.source),
                (id.as_str(), AudioSource::Mic)
            )
        })
        .flat_map(|e| e.segments.iter().map(|s| s.start_ms))
        .collect();
    // Windows at 0 s, 10 s and the 25 s tail, each saying both segments.
    assert_eq!(starts, [0, 1000, 10_000, 11_000, 25_000, 26_000]);
    let saved: Vec<i64> = h
        .store
        .segments(&id, AudioSource::Mic)
        .unwrap()
        .iter()
        .map(|s| s.start_ms)
        .collect();
    assert_eq!(saved, starts);
    assert_eq!(h.get(&id).transcript, [TRANSCRIPT; 3].join(" "));
}

#[test]
fn low_memory_recording_waits_for_a_running_summary_before_loading_whisper() {
    let h = Harness::with(
        |_| {},
        |loader| {
            Arc::get_mut(&mut loader.summarizer).unwrap().token_delay = Duration::from_millis(500);
        },
    );
    h.recorder.set_unload_whisper(true);

    let first = h.start(InputMode::Mic);
    h.feed(3);
    h.recorder.stop().unwrap();
    wait_until("the first summary starts", || h.queue.summarizing());

    let second = h.start(InputMode::Mic);
    h.feed(20);
    h.wait_chunks(&second, 1);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(loads(&h), 1, "Whisper must wait until Gemma is idle");

    h.recorder.stop().unwrap();
    h.events.wait_for(&first, SessionStatus::Done);
    h.events.wait_for(&second, SessionStatus::Done);
    assert!(h.queue.wait_idle(WAIT));
    assert_eq!(loads(&h), 2, "Whisper loads after the summary finishes");
}

#[test]
fn cancelling_while_waiting_for_summary_to_finish_preserves_cancelled_session() {
    let h = Harness::with(
        |_| {},
        |loader| {
            Arc::get_mut(&mut loader.summarizer).unwrap().token_delay = Duration::from_millis(500);
        },
    );
    h.recorder.set_unload_whisper(true);

    let first = h.start(InputMode::Mic);
    h.feed(3);
    h.recorder.stop().unwrap();
    wait_until("the first summary starts", || h.queue.summarizing());

    let second = h.start(InputMode::Mic);
    h.feed(20);
    h.wait_chunks(&second, 1);
    wait_until("transcript waiting event", || {
        h.events
            .waiting
            .lock()
            .unwrap()
            .iter()
            .any(|event| event.session_id == second && event.waiting)
    });
    h.recorder.stop().unwrap();
    h.events.wait_for(&second, SessionStatus::Finalizing);
    assert!(h.recorder.cancel(&second));

    h.events.wait_for(&second, SessionStatus::Cancelled);
    h.events.wait_for(&first, SessionStatus::Done);
    assert_eq!(h.get(&second).status, SessionStatus::Cancelled);
    assert_eq!(h.get(&second).error, None);
    assert_eq!(
        h.events
            .waiting
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.session_id == second)
            .map(|event| event.waiting)
            .collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn high_memory_live_transcript_continues_during_a_running_summary_without_wait_events() {
    let h = Harness::with(
        |_| {},
        |loader| {
            Arc::get_mut(&mut loader.summarizer).unwrap().token_delay = Duration::from_millis(500);
        },
    );
    h.recorder.set_unload_whisper(false);

    let first = h.start(InputMode::Mic);
    h.feed(3);
    h.recorder.stop().unwrap();
    wait_until("the summary starts", || h.queue.summarizing());

    let second = h.start(InputMode::Mic);
    h.feed(16);
    h.wait_chunks(&second, 3);
    wait_until("live transcript segments during the summary", || {
        h.queue.summarizing()
            && h.events
                .segments
                .lock()
                .unwrap()
                .iter()
                .any(|event| event.session_id == second)
    });
    assert!(h.events.waiting.lock().unwrap().is_empty());

    h.recorder.stop().unwrap();
    h.events.wait_for(&second, SessionStatus::Done);
    h.events.wait_for(&first, SessionStatus::Done);
    assert!(h.get(&second).transcript.contains(TRANSCRIPT));
}

#[test]
fn whisper_loading_does_not_hold_the_summary_queue_lock() {
    let h = Harness::new();
    let queue = Arc::clone(&h.queue);
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let cancel = AtomicBool::new(false);
    let loading = std::thread::spawn(move || {
        queue.while_transcription_safe(
            &cancel,
            || {},
            || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            },
        )
    });
    started_rx.recv_timeout(WAIT).unwrap();

    let queue = Arc::clone(&h.queue);
    let (query_tx, query_rx) = std::sync::mpsc::channel();
    let query = std::thread::spawn(move || query_tx.send(queue.is_busy()).unwrap());
    let responsive = query_rx.recv_timeout(Duration::from_millis(100)).is_ok();
    release_tx.send(()).unwrap();
    query.join().unwrap();
    assert_eq!(loading.join().unwrap(), Some(()));
    assert!(responsive, "queue status query waited for Whisper loading");
}

#[test]
fn whisper_loads_when_recording_starts_and_unloads_on_small_macs() {
    let h = Harness::new();
    let record = |mode| {
        let id = h.start(mode);
        h.feed(3);
        h.recorder.stop().unwrap();
        h.events.wait_for(&id, SessionStatus::Done);
    };

    h.recorder.set_unload_whisper(false);
    let id = h.start(InputMode::MicAndSystem);
    wait_until("whisper loads", || loads(&h) == 1);
    assert!(h.stt.calls().is_empty(), "loaded before any audio");
    h.feed(3);
    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::Done);
    record(InputMode::Mic);
    assert_eq!(loads(&h), 1, "both sources and sessions share one load");

    h.recorder.set_unload_whisper(true);
    // The first reuses the loaded model and unloads it; the next reloads.
    record(InputMode::Mic);
    record(InputMode::Mic);
    assert_eq!(loads(&h), 2, "unloaded after the transcript");
}

#[test]
fn first_run_refuses_recording_until_speech_is_ready_without_creating_a_session() {
    struct DownloadingSpeech;
    impl scribular_lib::stt::TranscriberLoader for DownloadingSpeech {
        fn ready(&self) -> bool {
            false
        }
        fn load(&self) -> scribular_lib::stt::Result<Arc<dyn scribular_lib::stt::Transcriber>> {
            Err(scribular_lib::stt::SttError::NotReady)
        }
    }
    let h = Harness::new();
    let recorder = Recorder::new(
        h.store.clone(),
        h.queue.clone(),
        Arc::new(DownloadingSpeech),
        h.events.clone(),
        Arc::new(FakeMicrophone(h.mic.clone())),
        Arc::new(FakeSystemAudio {
            source: h.system.clone(),
            app: APP.into(),
        }),
    );
    assert!(matches!(
        recorder.start(InputMode::MicAndSystem, Some(APP), "First recording"),
        Err(RecordError::SpeechModelNotReady)
    ));
    assert!(h.store.list_sessions().unwrap().is_empty());
    assert!(!h.mic.is_open());
    assert!(!h.system.is_open());
    assert_eq!(recorder.current(), None);
}

/// Spec 04 AS11: access lapsing mid-recording never touches the session.
/// The gate sits only in the `start_recording` command, so here it is
/// checked directly; stop and the summary job never consult it.
#[test]
fn a_lapse_mid_recording_still_saves_and_summarizes() {
    use scribular_lib::license::{
        api::FakeLicenseApi, AccessState, Licensing, ManualClock, DAY_MS,
    };

    let h = Harness::new();
    let clock = Arc::new(ManualClock::at(1_790_000_000_000));
    let licensing = Licensing::init(
        h.store.clone(),
        Arc::new(MemoryKeyStore::default()),
        Arc::new(FakeLicenseApi::default()),
        clock.clone(),
    );
    assert!(licensing.require_access().is_ok());
    let id = h.start(InputMode::Mic);
    h.feed(6);

    clock.set(1_790_000_000_000 + 15 * DAY_MS);
    let denied = licensing.require_access().unwrap_err();
    assert_eq!(denied.state, AccessState::TrialEnded);

    assert_eq!(h.recorder.stop().unwrap(), id);
    h.events.wait_for(&id, SessionStatus::Done);
    assert!(h.queue.wait_idle(WAIT));
    let s = h.get(&id);
    assert_eq!(s.transcript, TRANSCRIPT);
    assert!(!s.summary.is_empty());
}

fn interrupted_with_audio(h: &Harness) -> String {
    let session = h
        .store
        .create_session(&NewSession {
            title: "Interrupted".into(),
            input_mode: InputMode::Mic,
            prompt_id: Some("default-therapy-dora".into()),
            model_id: None,
        })
        .unwrap();
    h.store
        .append_audio_chunk(&session.id, AudioSource::Mic, 0, &vec![10_000; 80_000])
        .unwrap();
    h.store
        .set_status(&session.id, SessionStatus::Failed, Some("interrupted"))
        .unwrap();
    session.id
}

#[test]
fn interrupted_audio_recovers_transcript_then_summarizes_and_applies_retention() {
    let h = Harness::new();
    let id = interrupted_with_audio(&h);
    h.recorder.recover(&id).unwrap();
    h.events.wait_for(&id, SessionStatus::Done);
    assert!(h.queue.wait_idle(WAIT));
    let recovered = h.get(&id);
    assert_eq!(recovered.status, SessionStatus::Done);
    assert_eq!(recovered.transcript, TRANSCRIPT);
    assert!(!recovered.summary.is_empty());
    assert_eq!(
        h.chunks(&id),
        0,
        "audio is removed after successful recovery summary"
    );
}

#[test]
fn cancelling_recovery_transcription_keeps_audio_and_marks_it_interrupted() {
    let h = Harness::with(|stt| stt.block_from = 0, |_| {});
    let id = interrupted_with_audio(&h);
    h.recorder.recover(&id).unwrap();
    wait_until("recovery transcription", || !h.stt.calls().is_empty());
    assert!(h.recorder.cancel(&id));
    wait_until("interrupted recovery state", || {
        let session = h.get(&id);
        session.status == SessionStatus::Failed && session.error.as_deref() == Some("interrupted")
    });
    assert_eq!(h.chunks(&id), 1);
}

#[test]
fn a_recovery_can_be_claimed_only_once_while_transcription_runs() {
    let h = Harness::with(|stt| stt.block_from = 0, |_| {});
    let id = interrupted_with_audio(&h);
    h.recorder.recover(&id).unwrap();
    wait_until("recovery transcription", || !h.stt.calls().is_empty());
    assert!(h.recorder.recover(&id).is_err());
    assert_eq!(h.stt.calls().len(), 1);
    assert!(h.recorder.cancel(&id));
    wait_until("recovery cleanup", || {
        h.get(&id).status == SessionStatus::Failed
    });
}

#[test]
fn a_failed_recovery_summary_keeps_its_audio_for_another_attempt() {
    let h = Harness::with(
        |_| {},
        |loader| loader.fail = Some(LlmError::Engine("offline".into())),
    );
    let id = interrupted_with_audio(&h);
    h.recorder.recover(&id).unwrap();
    h.events.wait_for(&id, SessionStatus::Summarizing);
    assert!(h.queue.wait_idle(WAIT));
    assert_eq!(h.get(&id).status, SessionStatus::Failed);
    assert_eq!(h.get(&id).error.as_deref(), Some("interrupted"));
    assert_eq!(h.chunks(&id), 1);
}

#[test]
fn cancelling_a_recovery_summary_restores_the_interrupted_state_and_audio() {
    let h = Harness::with(
        |_| {},
        |loader| {
            Arc::get_mut(&mut loader.summarizer).unwrap().token_delay = Duration::from_millis(100);
        },
    );
    let id = interrupted_with_audio(&h);
    h.recorder.recover(&id).unwrap();
    h.events.wait_for(&id, SessionStatus::Summarizing);
    assert!(h.recorder.cancel(&id));
    wait_until("cancelled recovery summary", || {
        let s = h.get(&id);
        s.status == SessionStatus::Failed && s.error.as_deref() == Some("interrupted")
    });
    assert_eq!(h.chunks(&id), 1);
}

#[test]
fn recovery_with_insufficient_speech_keeps_audio_and_reports_why() {
    let h = Harness::with(|stt| stt.segments = vec![segment(0, "too few")], |_| {});
    let id = interrupted_with_audio(&h);
    h.recorder.recover(&id).unwrap();
    wait_until("insufficient recovery speech", || {
        h.get(&id).error.as_deref() == Some("Audio capture insufficient. Please record a bit more.")
    });
    assert_eq!(h.get(&id).status, SessionStatus::Failed);
    assert_eq!(h.chunks(&id), 1);
}

#[test]
fn safe_delete_refuses_live_capture_and_clear_all_reports_skipped_sessions() {
    let h = Harness::new();
    assert_eq!(h.store.session_count().unwrap(), 0);
    let recording = h.start(InputMode::Mic);
    assert!(h
        .queue
        .delete_session_safe(&recording)
        .unwrap_err()
        .to_string()
        .contains("can't be deleted"));
    assert!(h.get(&recording).status == SessionStatus::Recording);
    let completed = h
        .store
        .create_session(&NewSession {
            title: "Done".into(),
            input_mode: InputMode::Mic,
            prompt_id: Some("default-therapy-dora".into()),
            model_id: None,
        })
        .unwrap();
    h.store
        .finish_summary(
            &completed.id,
            "summary",
            "style",
            "model",
            &Default::default(),
        )
        .unwrap();
    assert_eq!(h.store.session_count().unwrap(), 2);
    let (deleted, skipped) = h.queue.clear_all_sessions().unwrap();
    assert_eq!((deleted, skipped), (1, 1));
    assert_eq!(h.store.session_count().unwrap(), 1);
    assert_eq!(
        h.store
            .list_sessions()
            .unwrap()
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        [recording.as_str()]
    );
}

/// Parity spec 03 AS9: changing the active style mid-recording does not
/// change the style of the recording in progress.
#[test]
fn as9_a_recording_keeps_the_style_active_at_start() {
    let h = Harness::new();
    let at_start = h.store.get_active_prompt_id().unwrap().unwrap();
    let id = h.start(InputMode::Mic);
    assert!(matches!(
        h.recorder.while_idle(|| ()),
        Err(RecordError::AlreadyRecording)
    ));
    h.feed(2);
    h.store
        .set_active_prompt("default-narrative-therapy")
        .unwrap();

    h.recorder.stop().unwrap();
    h.events.wait_for(&id, SessionStatus::Done);
    assert!(h.queue.wait_idle(WAIT));

    assert_eq!(h.get(&id).prompt_id, Some(at_start));
}
