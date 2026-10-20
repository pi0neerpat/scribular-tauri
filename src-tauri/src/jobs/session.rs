//! The batch session loop (KTD2, KTD7): each source records into
//! `audio_chunks`; Stop transcribes the stored audio, saves the transcript
//! (deleting the audio with it) and queues the summary.

use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread::JoinHandle,
    time::Instant,
};

use super::summarize::ms;
use super::{JobEvents, JobQueue, SessionProgress};
use crate::audio::{chunk_writer, AudioError, Capture, Microphone, SystemAudio};
use crate::store::{
    self, AudioSource, InputMode, NewSession, Session, SessionStatus, Store, StoreError, Timings,
    ACTIVE_PROMPT_ID,
};
use crate::stt::{SttError, TranscriberLoader, SAMPLE_RATE};
use crate::text::{clean_transcript, merge_diarized_segments, Segment};

#[derive(Debug)]
pub enum RecordError {
    AlreadyRecording,
    NotRecording,
    /// System audio was asked for without an app to capture.
    NoTargetApp,
    Audio(AudioError),
    Store(StoreError),
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRecording => f.write_str("a recording is already running"),
            Self::NotRecording => f.write_str("nothing is recording"),
            Self::NoTargetApp => f.write_str("choose an app to record in Settings"),
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

struct Source {
    capture: Box<dyn Capture>,
    writer: JoinHandle<store::Result<u64>>,
}

impl Source {
    /// Stops capturing and waits for the last chunk to be written. Returns
    /// the 16 kHz samples stored.
    fn finish(self) -> Result<u64, String> {
        drop(self.capture);
        match self.writer.join() {
            Ok(written) => written.map_err(|e| e.to_string()),
            Err(_) => Err("the audio writer crashed".into()),
        }
    }
}

struct Recording {
    session_id: String,
    sources: Vec<Source>,
}

#[derive(Default)]
struct State {
    /// Cancel flags of sessions being transcribed.
    finalizing: HashMap<String, Arc<AtomicBool>>,
    shutdown: bool,
}

struct Inner {
    store: Arc<Store>,
    jobs: Arc<JobQueue>,
    stt: Arc<dyn TranscriberLoader>,
    events: Arc<dyn JobEvents>,
    mic: Arc<dyn Microphone>,
    system: Arc<dyn SystemAudio>,
    recording: Mutex<Option<Recording>>,
    state: Mutex<State>,
    /// One transcription at a time, so two never hold the model (KTD5).
    transcribing: Mutex<()>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

pub struct Recorder {
    inner: Arc<Inner>,
    finalizers: Mutex<Vec<JoinHandle<()>>>,
}

impl Recorder {
    pub fn new(
        store: Arc<Store>,
        jobs: Arc<JobQueue>,
        stt: Arc<dyn TranscriberLoader>,
        events: Arc<dyn JobEvents>,
        mic: Arc<dyn Microphone>,
        system: Arc<dyn SystemAudio>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                jobs,
                stt,
                events,
                mic,
                system,
                recording: Mutex::default(),
                state: Mutex::default(),
                transcribing: Mutex::default(),
            }),
            finalizers: Mutex::default(),
        }
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

    /// Creates a session in `Recording` and starts every source `mode`
    /// needs. If a source fails to start, the session is deleted again.
    pub fn start(
        &self,
        mode: InputMode,
        app: Option<&str>,
        title: &str,
    ) -> Result<Session, RecordError> {
        let inner = &self.inner;
        let mut recording = lock(&inner.recording);
        if recording.is_some() {
            return Err(RecordError::AlreadyRecording);
        }
        let app = app.filter(|a| !a.is_empty());
        if sources(mode).contains(&AudioSource::System) && app.is_none() {
            return Err(RecordError::NoTargetApp);
        }
        let session = inner.store.create_session(&NewSession {
            title: title.into(),
            input_mode: mode,
            prompt_id: inner.store.get_setting(ACTIVE_PROMPT_ID)?,
            model_id: None,
        })?;
        let mut started = Vec::new();
        for &source in sources(mode) {
            match inner.open(&session.id, source, app) {
                Ok(s) => started.push(s),
                Err(e) => {
                    for s in started {
                        let _ = s.finish();
                    }
                    if let Err(e) = inner.store.delete_session(&session.id) {
                        eprintln!("record {}: deleting failed start: {e}", session.id);
                    }
                    return Err(e);
                }
            }
        }
        *recording = Some(Recording {
            session_id: session.id.clone(),
            sources: started,
        });
        inner.emit(&session.id, SessionStatus::Recording, "Recording...");
        Ok(session)
    }

    /// Stops the recording and transcribes it in the background. Returns
    /// the session id; progress arrives as [`SessionProgress`] events.
    pub fn stop(&self) -> Result<String, RecordError> {
        let inner = &self.inner;
        let recording = lock(&inner.recording)
            .take()
            .ok_or(RecordError::NotRecording)?;
        let id = recording.session_id;
        let mut samples = 0;
        let mut failure = None;
        for source in recording.sources {
            match source.finish() {
                Ok(n) => samples = samples.max(n),
                Err(e) => {
                    failure.get_or_insert(e);
                }
            }
        }
        if let Some(e) = failure {
            // The chunks already written stay for a retry.
            inner.end(&id, SessionStatus::Failed, &e);
            return Ok(id);
        }
        let duration_ms =
            u32::try_from(samples * 1000 / u64::from(SAMPLE_RATE)).unwrap_or(u32::MAX);

        let cancel = Arc::new(AtomicBool::new(false));
        lock(&inner.state)
            .finalizing
            .insert(id.clone(), Arc::clone(&cancel));
        if let Err(e) = inner.store.set_status(&id, SessionStatus::Finalizing, None) {
            lock(&inner.state).finalizing.remove(&id);
            return Err(e.into());
        }
        inner.emit(&id, SessionStatus::Finalizing, "Transcribing...");

        let worker = {
            let inner = Arc::clone(inner);
            let id = id.clone();
            std::thread::Builder::new()
                .name("finalize".into())
                .spawn(move || {
                    inner.finalize(&id, duration_ms, &cancel);
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
                inner.end(&id, SessionStatus::Failed, &e.to_string());
            }
        }
        Ok(id)
    }

    /// Stops `session_id`'s transcription (keeping what is done so far) or
    /// its summary job. Returns whether it had either.
    pub fn cancel(&self, session_id: &str) -> bool {
        if let Some(flag) = lock(&self.inner.state).finalizing.get(session_id) {
            flag.store(true, Ordering::Relaxed);
            return true;
        }
        self.inner.jobs.cancel(session_id)
    }
}

impl Drop for Recorder {
    /// Quitting mid-recording leaves the session `Recording` and mid-
    /// transcription leaves it `Finalizing`; either way the next launch marks
    /// it interrupted and keeps its audio (KTD2).
    fn drop(&mut self) {
        if let Some(recording) = lock(&self.inner.recording).take() {
            for source in recording.sources {
                let _ = source.finish();
            }
        }
        {
            let mut state = lock(&self.inner.state);
            state.shutdown = true;
            for flag in state.finalizing.values() {
                flag.store(true, Ordering::Relaxed);
            }
        }
        for worker in lock(&self.finalizers).drain(..) {
            let _ = worker.join();
        }
    }
}

impl Inner {
    fn open(
        &self,
        session_id: &str,
        source: AudioSource,
        app: Option<&str>,
    ) -> Result<Source, RecordError> {
        let (sink, rx) = chunk_writer::channel();
        let capture = match source {
            AudioSource::Mic => self.mic.open(sink)?,
            AudioSource::System => self
                .system
                .open(app.ok_or(RecordError::NoTargetApp)?, sink)?,
        };
        let writer = chunk_writer::spawn(
            Arc::clone(&self.store),
            session_id.into(),
            source,
            capture.format(),
            rx,
        )?;
        Ok(Source { capture, writer })
    }

    /// Writes the status row, then emits the matching event.
    fn end(&self, id: &str, status: SessionStatus, error: &str) {
        let error = (status == SessionStatus::Failed).then_some(error);
        if let Err(e) = self.store.set_status(id, status, error) {
            eprintln!("record {id}: saving {status:?} failed: {e}");
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

    fn finalize(&self, id: &str, duration_ms: u32, cancel: &AtomicBool) {
        let started = Instant::now();
        let transcribed = {
            let _turn = lock(&self.transcribing);
            self.transcribe(id, cancel)
        };
        let (transcript, partial) = match transcribed {
            Ok(t) => t,
            Err(e) => return self.end(id, SessionStatus::Failed, &e),
        };
        if lock(&self.state).shutdown {
            return;
        }
        if let Err(e) = self.store.save_transcript(id, &transcript, duration_ms) {
            return self.end(id, SessionStatus::Failed, &e.to_string());
        }
        let timings = Timings {
            tail_transcription_ms: Some(ms(started)),
            ..Timings::default()
        };
        if let Err(e) = self.store.set_timings(id, &timings) {
            eprintln!("record {id}: saving timings failed: {e}");
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

    /// Transcribes each source and stores its segments. Returns the cleaned
    /// transcript and whether a cancel cut it short.
    fn transcribe(&self, id: &str, cancel: &AtomicBool) -> Result<(String, bool), String> {
        let store = &self.store;
        let err = |e: StoreError| e.to_string();
        let session = store
            .get_session(id)
            .map_err(err)?
            .ok_or_else(|| format!("session {id} not found"))?;
        let mut partial = cancel.load(Ordering::Relaxed);
        let stt = if partial {
            None
        } else {
            Some(self.stt.load().map_err(|e| e.to_string())?)
        };
        let mut transcripts: Vec<(AudioSource, Vec<Segment>)> = Vec::new();
        for &source in sources(session.input_mode) {
            let samples: Vec<f32> = store
                .audio_samples(id, source)
                .map_err(err)?
                .into_iter()
                .map(|s| f32::from(s) / 32768.0)
                .collect();
            let segments = match &stt {
                Some(stt) if !partial && !samples.is_empty() => {
                    match stt.transcribe(&samples, cancel) {
                        Ok(segments) => segments,
                        Err(SttError::Cancelled) => {
                            partial = true;
                            Vec::new()
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                }
                _ => Vec::new(),
            };
            let segments: Vec<Segment> = segments
                .into_iter()
                .map(|s| Segment {
                    start_ms: s.start_ms,
                    end_ms: s.end_ms,
                    text: Some(s.text),
                })
                .collect();
            for segment in &segments {
                store.add_segment(id, source, segment).map_err(err)?;
            }
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
        Ok((clean_transcript(&transcript), partial))
    }
}
