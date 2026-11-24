//! The session loop (KTD2, KTD7, M5b): each source records into
//! `audio_chunks` and is transcribed in overlapping windows while it records.
//! Stop transcribes only what is left, saves the transcript (deleting the
//! audio with it) and queues the summary, whose model starts loading at Stop.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, TryRecvError},
        Arc, Condvar, Mutex, MutexGuard, OnceLock,
    },
    thread::JoinHandle,
    time::Instant,
};

use super::summarize::ms;
use super::{
    JobEvents, JobQueue, RecordingState, SessionProgress, SessionSegments, TranscriptWaiting,
};
use crate::audio::{AudioError, Microphone, SystemAudio};
use crate::diarize::SpeakerEmbedderLoader;
use crate::store::{
    self, valid_target_app, AudioSource, InputMode, NewSession, Session, SessionStatus, Store,
    StoreError, Timings, ACTIVE_PROMPT_ID,
};
use crate::stt::{
    stream::{Stream, Window},
    SttError, Transcriber, TranscriberLoader, SAMPLE_RATE,
};
use crate::text::{clean_transcript, merge_diarized_segments, Segment};

#[derive(Debug)]
pub enum RecordError {
    AlreadyRecording,
    NotRecording,
    SpeechModelNotReady,
    NoAudio,
    StopPreparationFailed,
    /// System audio was asked for without an app to capture.
    NoTargetApp,
    UnsupportedMode,
    Interrupted,
    Audio(AudioError),
    Store(StoreError),
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRecording => f.write_str("a recording is already running"),
            Self::NotRecording => f.write_str("nothing is recording"),
            Self::SpeechModelNotReady => f.write_str("The speech model isn't downloaded yet."),
            Self::NoAudio => f.write_str("No audio was recorded."),
            Self::StopPreparationFailed => {
                f.write_str("Couldn’t prepare to stop. Recording is still running.")
            }
            Self::NoTargetApp => f.write_str("Choose an app to record."),
            Self::UnsupportedMode => f.write_str("Choose In the room or On a call to record."),
            Self::Interrupted => f.write_str("Recording stopped. Saved audio can be recovered."),
            Self::Audio(e) => e.fmt(f),
            Self::Store(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for RecordError {}

impl From<AudioError> for RecordError {
    fn from(e: AudioError) -> Self {
        Self::Audio(e)
    }
}

impl From<std::io::Error> for RecordError {
    fn from(e: std::io::Error) -> Self {
        Self::Audio(AudioError::Io(e))
    }
}

impl From<StoreError> for RecordError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

/// The sources a mode records, in transcription order.
fn sources(mode: InputMode) -> &'static [AudioSource] {
    match mode {
        InputMode::Mic => &[AudioSource::Mic],
        InputMode::System => &[AudioSource::System],
        InputMode::MicAndSystem => &[AudioSource::System, AudioSource::Mic],
    }
}

mod capture;

use capture::{Recording, Source, SpanAudio, Streamer};

struct PendingSpan {
    stream: Stream,
    /// Cleared once all saved PCM has been read and the tail queued.
    saved_end: Option<Arc<OnceLock<usize>>>,
}

#[derive(Default)]
struct Transcription {
    cancelled: AtomicBool,
    store_failed: AtomicBool,
    live: Mutex<LiveTranscription>,
    changed: Condvar,
}

#[derive(Default)]
struct LiveTranscription {
    paused: bool,
    window_cancel: Arc<AtomicBool>,
}

impl Transcription {
    fn pause(&self) {
        let mut live = lock(&self.live);
        live.paused = true;
        live.window_cancel.store(true, Ordering::Relaxed);
    }

    fn resume(&self) {
        let mut live = lock(&self.live);
        if live.paused {
            live.window_cancel = Arc::default();
            live.paused = false;
            self.changed.notify_all();
        }
    }

    fn cancel(&self) {
        let live = lock(&self.live);
        self.cancelled.store(true, Ordering::Relaxed);
        live.window_cancel.store(true, Ordering::Relaxed);
        self.changed.notify_all();
    }

    fn running(&self) -> Option<MutexGuard<'_, LiveTranscription>> {
        let live = self
            .changed
            .wait_while(lock(&self.live), |live| {
                live.paused && !self.cancelled.load(Ordering::Relaxed)
            })
            .unwrap_or_else(|p| p.into_inner());
        (!self.cancelled.load(Ordering::Relaxed)).then_some(live)
    }
}

/// Whisper is unloaded at 8 GiB regardless of the summary model's RAM gate.
const WHISPER_UNLOAD_RAM_BYTES: u64 = 8 << 30;

/// Whether this Mac has 8 GiB of memory or less. Unknown counts as low.
fn low_memory() -> bool {
    crate::hardware::physical_memory_bytes().is_none_or(|bytes| bytes <= WHISPER_UNLOAD_RAM_BYTES)
}

#[derive(Default)]
struct State {
    /// Cancel flags of sessions being transcribed.
    finalizing: HashMap<String, Arc<Transcription>>,
    shutdown: bool,
}

struct Inner {
    store: Arc<Store>,
    jobs: Arc<JobQueue>,
    stt: Arc<dyn TranscriberLoader>,
    speakers: Arc<dyn SpeakerEmbedderLoader>,
    events: Arc<dyn JobEvents>,
    mic: Arc<dyn Microphone>,
    system: Arc<dyn SystemAudio>,
    recording: Mutex<Option<Recording>>,
    state: Mutex<State>,
    /// One transcription at a time: the sources take turns (KTD5).
    transcribing: Mutex<()>,
    /// Whisper, loaded when a recording starts.
    model: Mutex<Option<Arc<dyn Transcriber>>>,
    /// Drop whisper once a transcript is done, before the summary loads.
    unload_whisper: AtomicBool,
    /// Sessions already shown as waiting; the first source to acquire Whisper clears it.
    transcript_waiting: Mutex<HashSet<String>>,
    /// A second source must not re-show the wait after the first source acquires Whisper.
    transcript_started: Mutex<HashSet<String>>,
}

struct UnavailableSpeakerLoader;

impl SpeakerEmbedderLoader for UnavailableSpeakerLoader {
    fn load(&self) -> Result<Box<dyn crate::diarize::SpeakerEmbedder>, String> {
        Err("speaker model isn't ready".into())
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub struct Recorder {
    inner: Arc<Inner>,
    finalizers: Mutex<Vec<JoinHandle<()>>>,
    install_gate: Arc<Mutex<()>>,
    models: Mutex<Option<Arc<crate::models::ModelManager>>>,
}

impl Recorder {
    pub(crate) fn while_not_recording<T>(&self, operation: impl FnOnce() -> T) -> Option<T> {
        let recording = lock(&self.inner.recording);
        if recording.is_some() {
            return None;
        }
        Some(operation())
    }

    pub fn is_recording(&self) -> bool {
        lock(&self.inner.recording).is_some()
    }

    pub fn new(
        store: Arc<Store>,
        jobs: Arc<JobQueue>,
        stt: Arc<dyn TranscriberLoader>,
        events: Arc<dyn JobEvents>,
        mic: Arc<dyn Microphone>,
        system: Arc<dyn SystemAudio>,
    ) -> Self {
        Self::new_with_speakers(
            store,
            jobs,
            stt,
            events,
            mic,
            system,
            Arc::new(UnavailableSpeakerLoader),
        )
    }

    pub fn new_with_speakers(
        store: Arc<Store>,
        jobs: Arc<JobQueue>,
        stt: Arc<dyn TranscriberLoader>,
        events: Arc<dyn JobEvents>,
        mic: Arc<dyn Microphone>,
        system: Arc<dyn SystemAudio>,
        speakers: Arc<dyn SpeakerEmbedderLoader>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                jobs,
                stt,
                speakers,
                events,
                mic,
                system,
                recording: Mutex::default(),
                state: Mutex::default(),
                transcribing: Mutex::default(),
                model: Mutex::default(),
                unload_whisper: AtomicBool::new(low_memory()),
                transcript_waiting: Mutex::default(),
                transcript_started: Mutex::default(),
            }),
            finalizers: Mutex::default(),
            install_gate: Arc::new(Mutex::new(())),
            models: Mutex::default(),
        }
    }

    /// Overrides whether whisper is unloaded after each transcript, which
    /// defaults to whether this Mac has 8 GB or less.
    pub fn set_unload_whisper(&self, unload: bool) {
        self.inner.unload_whisper.store(unload, Ordering::Relaxed);
    }

    pub fn system_audio(&self) -> &dyn SystemAudio {
        &*self.inner.system
    }

    /// The session being recorded, if any.
    pub fn current(&self) -> Option<String> {
        lock(&self.inner.recording)
            .as_ref()
            .map(|r| r.session_id.clone())
    }

    pub fn install_gate(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.install_gate)
    }

    pub fn set_model_manager(&self, models: Arc<crate::models::ModelManager>) {
        models.set_install_gate(self.install_gate());
        *lock(&self.models) = Some(models);
    }

    pub fn is_busy(&self) -> bool {
        let recording = lock(&self.inner.recording).is_some();
        recording
            || !lock(&self.inner.state).finalizing.is_empty()
            || self.inner.jobs.is_busy()
            || lock(&self.finalizers)
                .iter()
                .any(|worker| !worker.is_finished())
            || lock(&self.models)
                .as_ref()
                .is_some_and(|models| models.is_busy())
    }

    /// Whether quitting would interrupt a recording or a running summary.
    pub fn quit_confirmation_needed(&self) -> bool {
        self.is_recording() || self.inner.jobs.summarizing()
    }

    /// Runs `operation` while preventing a recording from starting.
    pub fn while_idle<T>(&self, operation: impl FnOnce() -> T) -> Result<T, RecordError> {
        let recording = lock(&self.inner.recording);
        if recording.is_some() {
            return Err(RecordError::AlreadyRecording);
        }
        Ok(operation())
    }

    pub fn recording_state(&self) -> Option<RecordingState> {
        lock(&self.inner.recording)
            .as_ref()
            .map(Recording::snapshot)
    }

    pub(crate) fn tray_state(&self) -> (bool, bool) {
        let recording = lock(&self.inner.recording);
        recording
            .as_ref()
            .map_or((false, false), |r| (true, r.multi_speaker))
    }

    /// Creates a session and starts the microphone. App audio attaches in the background.
    pub fn start(
        &self,
        mode: InputMode,
        app: Option<&str>,
        title: &str,
    ) -> Result<Session, RecordError> {
        let target_app = match mode {
            InputMode::Mic => None,
            InputMode::System => return Err(RecordError::UnsupportedMode),
            InputMode::MicAndSystem => Some(
                app.filter(|app| valid_target_app(app))
                    .ok_or(RecordError::NoTargetApp)?
                    .to_owned(),
            ),
        };
        let _gate = lock(&self.install_gate);
        let inner = &self.inner;
        if !inner.stt.ready() {
            return Err(RecordError::SpeechModelNotReady);
        }
        let mut current = lock(&inner.recording);
        if current.is_some() {
            return Err(RecordError::AlreadyRecording);
        }
        let settings = inner.store.recording_settings()?;
        let mic = Source::prepare(|sink| inner.mic.open(sink))?;
        let session = inner.store.create_session(&NewSession {
            title: title.into(),
            input_mode: mode,
            prompt_id: settings.prompt_id,
            model_id: None,
        })?;
        let recording =
            match Recording::start(inner, &session.id, settings.multi_speaker, target_app, mic) {
                Ok(recording) => recording,
                Err(error) => {
                    if let Err(e) = inner.store.delete_session(&session.id) {
                        crate::diag::log::log("ERROR", "recording", crate::diag::log::kind(&e));
                    }
                    return Err(error);
                }
            };
        inner.jobs.cancel_practice();
        inner.jobs.cancel_session_coach();
        inner.events.recording_starting();
        lock(&inner.transcript_waiting).clear();
        lock(&inner.transcript_started).clear();
        inner.events.recording_changed(true, settings.multi_speaker);
        inner.events.recording_state(Some(recording.snapshot()));
        *current = Some(recording);
        inner.emit(&session.id, SessionStatus::Recording, "Recording...");
        Ok(session)
    }

    pub fn pause(&self) -> Result<(), RecordError> {
        let mut current = lock(&self.inner.recording);
        let recording = current.as_mut().ok_or(RecordError::NotRecording)?;
        if !recording.running() {
            return Ok(());
        }
        if let Err(error) = recording.pause(&self.inner) {
            self.inner
                .interrupt(current.take().expect("active recording"), error);
            return Err(RecordError::Interrupted);
        }
        self.inner
            .events
            .recording_state(Some(recording.snapshot()));
        Ok(())
    }

    pub fn resume(&self) -> Result<(), RecordError> {
        let mut current = lock(&self.inner.recording);
        let recording = current.as_mut().ok_or(RecordError::NotRecording)?;
        if recording.running() {
            return Ok(());
        }
        if let Err(error) = recording.resume(&self.inner) {
            self.inner.interrupt(
                current.take().expect("active recording"),
                "Recording stopped: couldn't resume the microphone.",
            );
            return Err(error);
        }
        self.inner
            .events
            .recording_state(Some(recording.snapshot()));
        Ok(())
    }

    /// Stops the recording and transcribes it in the background. Returns
    /// the session id; progress arrives as [`SessionProgress`] events.
    pub fn stop(&self) -> Result<String, RecordError> {
        self.stop_with_speakers(None)
    }

    pub fn stop_with_speakers(&self, num_speakers: Option<u8>) -> Result<String, RecordError> {
        let _gate = lock(&self.install_gate);
        let inner = &self.inner;
        let mut current = lock(&inner.recording);
        let id = current
            .as_ref()
            .ok_or(RecordError::NotRecording)?
            .session_id
            .clone();
        if let Err(error) = inner.store.snapshot_recording_prompt(&id) {
            crate::diag::log::log("ERROR", "recording", crate::diag::log::kind(&error));
            return Err(RecordError::StopPreparationFailed);
        }
        let mut recording = current.take().expect("active recording");
        inner.recording_ended();
        let diarize = recording.multi_speaker && num_speakers != Some(0);
        if let Err(error) = recording.close(inner) {
            inner.interrupt_audio(&id, error, &recording.transcription);
            return Err(RecordError::Interrupted);
        }
        let samples = recording.samples();
        if samples == 0 {
            recording.transcription.cancel();
            inner.end(&id, SessionStatus::Failed, "no audio was recorded");
            return Err(RecordError::NoAudio);
        }
        let duration_ms =
            u32::try_from(samples as u64 * 1000 / u64::from(SAMPLE_RATE)).unwrap_or(u32::MAX);
        let streamers = recording.take_streamers();
        let transcription = recording.transcription;
        lock(&inner.state)
            .finalizing
            .insert(id.clone(), Arc::clone(&transcription));
        if let Err(e) = inner.store.set_status(&id, SessionStatus::Finalizing, None) {
            lock(&inner.state).finalizing.remove(&id);
            inner.interrupt_audio(
                &id,
                "Recording stopped: couldn't save the session.",
                &transcription,
            );
            return Err(e.into());
        }
        drop(current);
        inner.emit(&id, SessionStatus::Finalizing, "Transcribing...");
        transcription.resume();

        let worker = {
            let inner = Arc::clone(inner);
            let id = id.clone();
            let transcription = Arc::clone(&transcription);
            std::thread::Builder::new()
                .name("finalize".into())
                .spawn(move || {
                    inner.finalize(
                        &id,
                        duration_ms,
                        streamers,
                        &transcription,
                        diarize,
                        num_speakers,
                    );
                    lock(&inner.state).finalizing.remove(&id);
                })
        };
        match worker {
            Ok(worker) => {
                let mut finalizers = lock(&self.finalizers);
                finalizers.retain(|f| !f.is_finished());
                finalizers.push(worker);
            }
            Err(e) => {
                lock(&inner.state).finalizing.remove(&id);
                crate::diag::log::log("ERROR", "recording", crate::diag::log::kind(&e));
                inner.interrupt_audio(
                    &id,
                    "Recording stopped: couldn't start transcription.",
                    &transcription,
                );
                return Err(RecordError::Interrupted);
            }
        }
        Ok(id)
    }

    /// Stops `session_id`'s transcription (keeping what is done so far) or
    /// its summary job. Returns whether it had either.
    pub fn cancel(&self, session_id: &str) -> bool {
        if let Some(flag) = lock(&self.inner.state).finalizing.get(session_id) {
            flag.cancel();
            return true;
        }
        self.inner.jobs.cancel(session_id)
    }

    pub fn recover(&self, session_id: &str) -> Result<(), String> {
        let mut state = lock(&self.inner.state);
        if state.shutdown {
            return Err("app is shutting down".into());
        }
        if state.finalizing.contains_key(session_id) {
            return Err("session recovery is already running".into());
        }
        let session = self
            .inner
            .store
            .get_session(session_id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("session {session_id} not found"))?;
        if session.status != SessionStatus::Failed
            || session.error.as_deref() != Some(store::INTERRUPTED)
        {
            return Err("only interrupted sessions with saved audio can be recovered".into());
        }
        let transcription = Arc::new(Transcription::default());
        if !self
            .inner
            .store
            .claim_recovery(session_id)
            .map_err(|e| e.to_string())?
        {
            return Err("only interrupted sessions with saved audio can be recovered".into());
        }
        state
            .finalizing
            .insert(session_id.into(), Arc::clone(&transcription));
        drop(state);
        self.inner.emit(
            session_id,
            SessionStatus::Finalizing,
            "Recovering transcript…",
        );
        let inner = Arc::clone(&self.inner);
        let id = session_id.to_owned();
        let worker = std::thread::Builder::new()
            .name("recover-session".into())
            .spawn(move || {
                inner.recover(&id, &transcription);
                lock(&inner.state).finalizing.remove(&id);
            });
        let worker = match worker {
            Ok(worker) => worker,
            Err(e) => {
                lock(&self.inner.state).finalizing.remove(session_id);
                let _ = self
                    .inner
                    .store
                    .set_recovery_status(session_id, SessionStatus::Failed);
                return Err(e.to_string());
            }
        };
        let mut finalizers = lock(&self.finalizers);
        finalizers.retain(|f| !f.is_finished());
        finalizers.push(worker);
        Ok(())
    }

    /// Stops the recorder once, preserving in-flight sessions for launch recovery.
    pub fn shutdown(&self) {
        let _gate = lock(&self.install_gate);
        {
            let mut state = lock(&self.inner.state);
            if state.shutdown {
                return;
            }
            state.shutdown = true;
            for flag in state.finalizing.values() {
                flag.cancel();
            }
        }
        let recording = lock(&self.inner.recording).take();
        if let Some(mut recording) = recording {
            recording.transcription.cancel();
            let _ = recording.close(&self.inner);
            let _ = self.inner.drain(recording.take_streamers());
        }
        for worker in lock(&self.finalizers).drain(..) {
            let _ = worker.join();
        }
    }
}

impl Drop for Recorder {
    /// Quitting mid-recording leaves the session `Recording` and mid-
    /// transcription leaves it `Finalizing`; either way the next launch marks
    /// it interrupted and keeps its audio (KTD2).
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Inner {
    fn recover(&self, id: &str, transcription: &Transcription) {
        let cancel = &transcription.cancelled;
        let insufficient = crate::text::ChunkError::InsufficientAudio.to_string();
        let failed = |inner: &Self, message: &str| {
            let persisted = if message == insufficient {
                message
            } else {
                store::INTERRUPTED
            };
            if let Err(e) = inner.store.fail_recovery(id, persisted) {
                crate::diag::log::log(
                    "ERROR",
                    "recover",
                    &format!("recover {id} ({})", crate::diag::log::kind(&e)),
                );
            }
            inner.emit(id, SessionStatus::Failed, message);
        };
        let session = match self.session(id) {
            Ok(s) => s,
            Err(e) => return failed(self, &e),
        };
        if let Err(e) = self.store.clear_segments(id) {
            return failed(self, &e.to_string());
        }
        let mut sample_count = 0usize;
        for &source in sources(session.input_mode) {
            let audio = match self.store.audio_samples(id, source) {
                Ok(v) => v,
                Err(e) => return failed(self, &e.to_string()),
            };
            sample_count = sample_count.max(audio.len());
            let mut stream = Stream::default();
            for chunk in audio.chunks(crate::audio::CHUNK_SAMPLES) {
                let pcm = chunk
                    .iter()
                    .map(|sample| *sample as f32 / 32768.0)
                    .collect::<Vec<_>>();
                stream.push(&pcm);
            }
            stream.finish();
            while let Some(window) = stream.next() {
                if cancel.load(Ordering::Relaxed) {
                    return failed(self, store::INTERRUPTED);
                }
                let stt = match self.transcriber(id, cancel) {
                    Ok(s) => s,
                    Err(e) => return failed(self, &e),
                };
                match self.window(id, source, &mut stream, &window, &*stt, transcription) {
                    Ok(true) => {}
                    Ok(false) => return failed(self, store::INTERRUPTED),
                    Err(e) => return failed(self, &e),
                }
            }
        }
        if cancel.load(Ordering::Relaxed) {
            return failed(self, store::INTERRUPTED);
        }
        let transcript = match self.transcript(id) {
            Ok(t) => t,
            Err(e) => return failed(self, &e),
        };
        if transcript.split_whitespace().count() < crate::text::MIN_WORDS {
            return failed(self, &insufficient);
        }
        let duration_ms =
            u32::try_from(sample_count as u64 * 1000 / u64::from(SAMPLE_RATE)).unwrap_or(u32::MAX);
        let diarized = match self.store.recording_settings() {
            Ok(settings) if settings.multi_speaker => {
                let segments = sources(session.input_mode)
                    .iter()
                    .map(|&source| self.store.segments(id, source).map(|s| (source, s)))
                    .collect::<Result<Vec<_>, _>>();
                match segments {
                    Ok(segments) => {
                        let mut audio = self.store.audio_range_reader(id);
                        match crate::diarize::diarize(
                            session.input_mode,
                            &segments,
                            &mut |source, start, end| {
                                audio.samples(source, start, end).map_err(|e| e.to_string())
                            },
                            None,
                            &*self.speakers,
                            cancel,
                        ) {
                            Ok(value) => Some(value),
                            Err(_) => {
                                if !cancel.load(Ordering::Relaxed) {
                                    crate::diag::log::log("WARN", "recover", &format!("recover {id}: diarization unavailable (diarization_error)"));
                                    self.emit(id, SessionStatus::Finalizing, "Speaker labels unavailable; continuing with the transcript.");
                                }
                                None
                            }
                        }
                    }
                    Err(e) => {
                        crate::diag::log::log(
                            "WARN",
                            "recover",
                            &format!(
                                "recover {id}: diarization data unavailable ({})",
                                crate::diag::log::kind(&e)
                            ),
                        );
                        self.emit(
                            id,
                            SessionStatus::Finalizing,
                            "Speaker labels unavailable; continuing with the transcript.",
                        );
                        None
                    }
                }
            }
            _ => None,
        };
        if cancel.load(Ordering::Relaxed) {
            return failed(self, store::INTERRUPTED);
        }
        if let Err(e) = self.store.save_recovered_transcript(
            id,
            &transcript,
            duration_ms,
            diarized.as_ref().map(|(raw, map)| (raw.as_str(), map)),
        ) {
            return failed(self, &e.to_string());
        }
        if let Err(e) = self.store.set_timings(id, &Timings::default()) {
            crate::diag::log::log(
                "WARN",
                "recover",
                &format!(
                    "recover {id}: saving timings failed ({})",
                    crate::diag::log::kind(&e)
                ),
            );
        }
        if self.unload_whisper.load(Ordering::Relaxed) && lock(&self.recording).is_none() {
            lock(&self.model).take();
        }
        let queued = {
            let mut state = lock(&self.state);
            if cancel.load(Ordering::Relaxed) {
                state.finalizing.remove(id);
                Err(store::INTERRUPTED.to_owned())
            } else {
                let result = self.prompt_for(id).and_then(|prompt| {
                    self.jobs.regenerate(id, &prompt).map_err(|e| e.to_string())
                });
                state.finalizing.remove(id);
                result
            }
        };
        if let Err(e) = queued {
            return failed(self, &e);
        }
    }

    /// Whisper, loading it if no recording has yet.
    fn transcriber(&self, id: &str, cancel: &AtomicBool) -> Result<Arc<dyn Transcriber>, String> {
        if self.unload_whisper.load(Ordering::Relaxed) {
            let result = self
                .jobs
                .while_transcription_safe(
                    cancel,
                    || self.transcript_waiting(id, true),
                    || self.load_transcriber(),
                )
                .unwrap_or_else(|| Err("cancelled".into()));
            self.transcript_waiting(id, false);
            return result;
        }
        self.load_transcriber()
    }

    fn transcript_waiting(&self, id: &str, waiting: bool) {
        if waiting
            && !lock(&self.recording)
                .as_ref()
                .is_some_and(|r| r.session_id == id)
        {
            return;
        }
        let mut shown = lock(&self.transcript_waiting);
        let mut started = lock(&self.transcript_started);
        let changed = if waiting {
            !started.contains(id) && shown.insert(id.to_owned())
        } else {
            started.insert(id.to_owned()) && shown.remove(id)
        };
        if changed {
            self.events.transcript_waiting(TranscriptWaiting {
                session_id: id.into(),
                waiting,
            });
        }
    }

    fn load_transcriber(&self) -> Result<Arc<dyn Transcriber>, String> {
        let mut model = lock(&self.model);
        if let Some(model) = &*model {
            return Ok(Arc::clone(model));
        }
        let loaded = self.stt.load().map_err(|e| e.to_string())?;
        *model = Some(Arc::clone(&loaded));
        Ok(loaded)
    }

    /// Finish each closed span before publishing from the next one on this track.
    fn stream_spans(
        &self,
        id: &str,
        source: AudioSource,
        spans: &mpsc::Receiver<SpanAudio>,
        transcription: &Transcription,
    ) -> Result<bool, String> {
        let cancel = &transcription.cancelled;
        let mut pending = VecDeque::<PendingSpan>::new();
        loop {
            let next = spans.recv().ok();
            while let Some(span) = pending.front_mut() {
                let mut finish = || self.finish_span(id, source, span, transcription);
                let result = match finish() {
                    Err(_) if cancel.load(Ordering::Relaxed) => return Ok(true),
                    Err(_) if !transcription.store_failed.load(Ordering::Relaxed) => {
                        if next.is_some() {
                            break;
                        }
                        finish()
                    }
                    result => result,
                };
                if result? {
                    return Ok(true);
                }
                pending.pop_front();
            }
            let Some(SpanAudio { live, saved_end }) = next else {
                return Ok(cancel.load(Ordering::Relaxed));
            };
            // Keep the eager model load on an unblocked track, even before its first PCM.
            let stt = pending.is_empty().then(|| self.transcriber(id, cancel));
            let Ok((start, samples)) = live.recv() else {
                continue;
            };
            let mut stream = Stream::at(start);
            if let Some(stt) = stt {
                stream.push(&samples);
                let result = stt.and_then(|stt| {
                    self.stream_span(id, source, live, &mut stream, &*stt, transcription)
                });
                if let Err(error) = result {
                    if transcription.store_failed.load(Ordering::Relaxed) {
                        return Err(error);
                    }
                }
            }
            // On failure, drop live PCM and use the committed span at the next attach or Stop.
            pending.push_back(PendingSpan {
                stream,
                saved_end: Some(saved_end),
            });
        }
    }

    /// Transcribes a capture span while it records, leaving remaining windows for the drain.
    fn stream_span(
        &self,
        id: &str,
        source: AudioSource,
        audio: mpsc::Receiver<(usize, Vec<f32>)>,
        stream: &mut Stream,
        stt: &dyn Transcriber,
        transcription: &Transcription,
    ) -> Result<(), String> {
        let cancel = &transcription.cancelled;
        let mut catching_up = false;
        loop {
            if stream.queued() == 0 {
                let Ok((_, samples)) = audio.recv() else {
                    return Ok(());
                };
                stream.push(&samples);
            }
            loop {
                match audio.try_recv() {
                    Ok((_, samples)) => stream.push(&samples),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(()),
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return Ok(());
            }
            if stream.catching_up() != catching_up {
                catching_up = !catching_up;
                let message = if catching_up {
                    "Catching up..."
                } else {
                    "Recording..."
                };
                // Only while still recording, so the row matches the event.
                let recording = lock(&self.recording);
                if recording
                    .as_ref()
                    .is_some_and(|r| r.session_id == id && r.running())
                {
                    self.emit(id, SessionStatus::Recording, message);
                }
            }
            let Some(window) = stream.next() else {
                continue;
            };
            match self.window(id, source, stream, &window, stt, transcription) {
                Ok(true) => {}
                Ok(false) => return Ok(()),
                Err(error) => {
                    stream.retry(window);
                    return Err(error);
                }
            }
        }
    }

    /// Transcribes one window, saving and emitting its new segments.
    /// Returns false if it was cancelled.
    fn window(
        &self,
        id: &str,
        source: AudioSource,
        stream: &mut Stream,
        window: &Window,
        stt: &dyn Transcriber,
        transcription: &Transcription,
    ) -> Result<bool, String> {
        let segments = loop {
            let Some(running) = transcription.running() else {
                return Ok(false);
            };
            let cancel = Arc::clone(&running.window_cancel);
            drop(running);
            let segments = {
                let _turn = lock(&self.transcribing);
                if cancel.load(Ordering::Relaxed) {
                    continue;
                }
                stream.transcribe(window, stt, &cancel)
            };
            match segments {
                Ok(segments) => break segments,
                Err(SttError::Cancelled) if cancel.load(Ordering::Relaxed) => continue,
                Err(SttError::Cancelled) => return Ok(false),
                Err(e) => return Err(e.to_string()),
            }
        };
        if segments.is_empty() {
            return Ok(true);
        }
        // Serialize segment commits with pause. A window that just finished
        // when pause arrived can publish its result after resume.
        let Some(_running) = transcription.running() else {
            return Ok(false);
        };
        for s in &segments {
            let segment = Segment {
                start_ms: s.start_ms,
                end_ms: s.end_ms,
                text: Some(s.text.clone()),
            };
            self.store.add_segment(id, source, &segment).map_err(|e| {
                transcription.store_failed.store(true, Ordering::Relaxed);
                crate::diag::log::log("ERROR", "recording", crate::diag::log::kind(&e));
                "Couldn't save transcript segments.".to_owned()
            })?;
        }
        self.events.segments(SessionSegments {
            session_id: id.into(),
            source,
            segments,
        });
        Ok(true)
    }

    /// Writes the status row, then emits the matching event.
    fn end(&self, id: &str, status: SessionStatus, error: &str) {
        if status == SessionStatus::Failed {
            crate::diag::telemetry::report(Arc::clone(&self.store), "recording");
        }
        let error = (status == SessionStatus::Failed).then_some(error);
        if let Err(e) = self.store.set_status(id, status, error) {
            crate::diag::log::log(
                "ERROR",
                "recording",
                &format!(
                    "saving {status:?} for {id} failed ({})",
                    crate::diag::log::kind(&e)
                ),
            );
        }
        self.emit(id, status, error.unwrap_or("Transcription stopped."));
    }

    fn emit(&self, id: &str, state: SessionStatus, message: &str) {
        self.events.progress(SessionProgress {
            session_id: id.into(),
            state,
            step_index: 1,
            step_count: 1,
            message: message.into(),
        });
    }

    fn finalize(
        &self,
        id: &str,
        duration_ms: u32,
        streamers: Vec<Streamer>,
        transcription: &Transcription,
        diarize: bool,
        num_speakers: Option<u8>,
    ) {
        let cancel = &transcription.cancelled;
        let started = Instant::now();
        // The summary model loads while the tail transcribes (M5b).
        self.jobs.preload();
        let drained = self.drain(streamers);
        if transcription.store_failed.load(Ordering::Relaxed) {
            return self.interrupt_audio(id, "Couldn't save transcript segments.", transcription);
        }
        if self.unload_whisper.load(Ordering::Relaxed) && lock(&self.recording).is_none() {
            lock(&self.model).take();
        }
        let (transcript, partial) =
            match drained.and_then(|partial| self.transcript(id).map(|t| (t, partial))) {
                Ok(t) => t,
                Err(_) => {
                    return self.interrupt_audio(
                        id,
                        "Couldn't finish transcribing. Saved audio can be recovered.",
                        transcription,
                    )
                }
            };
        if lock(&self.state).shutdown {
            return;
        }
        let session = match self.session(id) {
            Ok(s) => s,
            Err(_) => {
                return self.interrupt_audio(
                    id,
                    "Couldn't load the session for transcription.",
                    transcription,
                )
            }
        };
        let diarization = if diarize {
            self.emit(id, SessionStatus::Finalizing, "Identifying speakers…");
            let segments = sources(session.input_mode)
                .iter()
                .map(|&source| self.store.segments(id, source).map(|s| (source, s)))
                .collect::<Result<Vec<_>, _>>();
            match segments {
                Ok(segments) => {
                    let mut audio = self.store.audio_range_reader(id);
                    match crate::diarize::diarize(
                        session.input_mode,
                        &segments,
                        &mut |source, start, end| {
                            audio.samples(source, start, end).map_err(|e| e.to_string())
                        },
                        num_speakers,
                        &*self.speakers,
                        cancel,
                    ) {
                        Ok(value) => Some(value),
                        Err(_e) => {
                            if !cancel.load(Ordering::Relaxed) {
                                crate::diag::log::log(
                                    "WARN",
                                    "diarization",
                                    &format!(
                                        "speaker identification for {id} failed ({})",
                                        "engine_error"
                                    ),
                                );
                                self.emit(
                                    id,
                                    SessionStatus::Finalizing,
                                    "Speaker labels unavailable; continuing with the transcript.",
                                );
                            }
                            None
                        }
                    }
                }
                Err(e) => {
                    crate::diag::log::log(
                        "WARN",
                        "diarization",
                        &format!(
                            "speaker data for {id} unavailable ({})",
                            crate::diag::log::kind(&e)
                        ),
                    );
                    None
                }
            }
        } else {
            None
        };
        let saved = match diarization {
            Some((raw, map)) => {
                self.store
                    .save_diarization(id, &transcript, &raw, &map, duration_ms)
            }
            None => self.store.save_transcript(id, &transcript, duration_ms),
        };
        if let Err(e) = saved {
            crate::diag::log::log("ERROR", "recording", crate::diag::log::kind(&e));
            return self.interrupt_audio(id, "Couldn't save the transcript.", transcription);
        }
        crate::diag::telemetry::record_session(Arc::clone(&self.store), duration_ms);
        let timings = Timings {
            tail_transcription_ms: Some(ms(started)),
            ..Timings::default()
        };
        if let Err(e) = self.store.set_timings(id, &timings) {
            crate::diag::log::log(
                "ERROR",
                "recording",
                &format!(
                    "saving recording timings for {id} failed ({})",
                    crate::diag::log::kind(&e)
                ),
            );
        }

        // Under the state lock, so a cancel lands either on the flag or on
        // the queued job, never between them.
        let state = lock(&self.state);
        if partial || cancel.load(Ordering::Relaxed) {
            drop(state);
            return self.end(id, SessionStatus::Cancelled, "");
        }
        let queued = self
            .prompt_for(id)
            .and_then(|prompt| self.jobs.regenerate(id, &prompt).map_err(|e| e.to_string()));
        drop(state);
        if let Err(e) = queued {
            self.end(id, SessionStatus::Failed, &e);
        }
    }

    /// The session's prompt, or the active one if it had none.
    fn prompt_for(&self, id: &str) -> Result<String, String> {
        let session = self.store.get_session(id).map_err(|e| e.to_string())?;
        match session.and_then(|s| s.prompt_id) {
            Some(p) => Ok(p),
            None => self
                .store
                .get_setting(ACTIVE_PROMPT_ID)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "no summary prompt is selected".into()),
        }
    }

    /// Joins every source's remaining transcription even if one fails.
    fn drain(&self, streamers: Vec<Streamer>) -> Result<bool, String> {
        let mut drained = Ok(false);
        // Complete the call track first so cancellation keeps its finished tail.
        for (spans, streamer) in streamers {
            drop(spans);
            let result = streamer
                .join()
                .unwrap_or_else(|_| Err("the transcriber crashed".into()));
            drained = drained.and_then(|partial| result.map(|other| partial || other));
        }
        drained
    }

    fn finish_span(
        &self,
        id: &str,
        source: AudioSource,
        span: &mut PendingSpan,
        transcription: &Transcription,
    ) -> Result<bool, String> {
        if let Some(saved_end) = &span.saved_end {
            if transcription.cancelled.load(Ordering::Relaxed) {
                return Ok(true);
            }
            let end = *saved_end
                .get()
                .ok_or("The transcription span was not saved.")?;
            if self.replay_span(id, source, &mut span.stream, end, transcription)? {
                return Ok(true);
            }
            span.stream.finish();
            span.saved_end = None;
        }
        self.drain_windows(id, source, &mut span.stream, transcription)
    }

    fn replay_span(
        &self,
        id: &str,
        source: AudioSource,
        stream: &mut Stream,
        end: usize,
        transcription: &Transcription,
    ) -> Result<bool, String> {
        let mut reader = self.store.audio_range_reader(id);
        loop {
            if self.drain_windows(id, source, stream, transcription)? {
                return Ok(true);
            }
            let start = stream.received_end();
            if start == end {
                return Ok(false);
            }
            let stop = end.min(start + crate::audio::CHUNK_SAMPLES);
            let samples = reader
                .sample_range(source, start as u64, stop as u64)
                .map_err(|error| error.to_string())?;
            if stop.checked_sub(start) != Some(samples.len()) {
                return Err("The failed transcription span is missing saved audio.".into());
            }
            let pcm = samples
                .into_iter()
                .map(|sample| f32::from(sample) / 32768.0)
                .collect::<Vec<_>>();
            stream.push(&pcm);
        }
    }

    fn drain_windows(
        &self,
        id: &str,
        source: AudioSource,
        stream: &mut Stream,
        transcription: &Transcription,
    ) -> Result<bool, String> {
        let cancel = &transcription.cancelled;
        while let Some(window) = stream.next() {
            if cancel.load(Ordering::Relaxed) {
                return Ok(true);
            }
            let result = (|| {
                let stt = self.transcriber(id, cancel)?;
                self.window(id, source, stream, &window, &*stt, transcription)
            })();
            match result {
                Ok(true) => {}
                Ok(false) => return Ok(true),
                Err(error) => {
                    stream.retry(window);
                    return Err(error);
                }
            }
        }
        Ok(false)
    }

    fn session(&self, id: &str) -> Result<Session, String> {
        self.store
            .get_session(id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("session {id} not found"))
    }

    /// The cleaned transcript of the segments stored for `id`.
    fn transcript(&self, id: &str) -> Result<String, String> {
        let session = self.session(id)?;
        let mut transcripts: Vec<(AudioSource, Vec<Segment>)> = Vec::new();
        for &source in sources(session.input_mode) {
            let segments = self.store.segments(id, source).map_err(|e| e.to_string())?;
            transcripts.push((source, segments));
        }
        let of = |source| {
            transcripts
                .iter()
                .find(|(s, _)| *s == source)
                .map_or(&[][..], |(_, segs)| segs.as_slice())
        };
        // As in Electron's audioWorker: two sources merge into speaker
        // turns, one source is its segments joined.
        let transcript = match session.input_mode {
            InputMode::MicAndSystem => {
                merge_diarized_segments(of(AudioSource::System), of(AudioSource::Mic))
            }
            _ => transcripts
                .iter()
                .flat_map(|(_, segs)| segs)
                .filter_map(|s| s.text.as_deref())
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_owned(),
        };
        Ok(clean_transcript(&transcript))
    }
}
