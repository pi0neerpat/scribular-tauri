//! The session loop (KTD2, KTD7, M5b): each source records into
//! `audio_chunks` and is transcribed in overlapping windows while it records.
//! Stop transcribes only what is left, saves the transcript (deleting the
//! audio with it) and queues the summary, whose model starts loading at Stop.

use std::{
    collections::HashMap,
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError, TryRecvError},
        Arc, Mutex, MutexGuard,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use super::summarize::ms;
use super::{JobEvents, JobQueue, SessionProgress, SessionSegments};
use crate::audio::{chunk_writer, AudioError, Capture, Microphone, SystemAudio};
use crate::store::{
    self, AudioSource, InputMode, NewSession, Session, SessionStatus, Store, StoreError, Timings,
    ACTIVE_PROMPT_ID,
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

/// How often a recording checks that its sources still write.
const WATCH_INTERVAL: Duration = Duration::from_millis(250);

/// A source's streaming transcription; joins to its windows still queued.
type Streamer = JoinHandle<Result<Stream, String>>;

struct Source {
    source: AudioSource,
    capture: Box<dyn Capture>,
    writer: JoinHandle<store::Result<u64>>,
    streamer: Streamer,
}

impl Source {
    /// Stops capturing and waits for the last chunk to be written. Returns
    /// the 16 kHz samples stored, and the streamer, which ends once it has
    /// taken the last of the audio.
    fn finish(self) -> (Result<u64, String>, Streamer) {
        drop(self.capture);
        let written = match self.writer.join() {
            Ok(written) => written.map_err(|e| e.to_string()),
            Err(_) => Err("the audio writer crashed".into()),
        };
        (written, self.streamer)
    }
}

struct Recording {
    session_id: String,
    sources: Vec<Source>,
    /// Stops the streamers of a recording that is abandoned or, once
    /// stopped, cancelled.
    cancel: Arc<AtomicBool>,
    /// Dropped with the recording, which ends its watchdog.
    _watched: mpsc::Sender<()>,
}

/// Stops `sources` without transcribing what they recorded. With `wait`,
/// returns once their streamers have stopped too.
fn abandon(sources: Vec<Source>, cancel: &AtomicBool, wait: bool) -> Option<String> {
    cancel.store(true, Ordering::Relaxed);
    let mut error = None;
    for source in sources {
        let (written, streamer) = source.finish();
        if let Err(e) = written {
            error.get_or_insert(e);
        }
        if wait {
            let _ = streamer.join();
        }
    }
    error
}

/// Whether this Mac has 8 GB of memory or less, where whisper is unloaded
/// before the summary model runs. Unknown counts as low.
fn low_memory() -> bool {
    std::process::Command::new("/usr/sbin/sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map_or(true, |bytes| bytes <= 8 << 30)
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
    /// One transcription at a time: the sources take turns (KTD5).
    transcribing: Mutex<()>,
    /// Whisper, loaded when a recording starts.
    model: Mutex<Option<Arc<dyn Transcriber>>>,
    /// Drop whisper once a transcript is done, before the summary loads.
    unload_whisper: AtomicBool,
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
                model: Mutex::default(),
                unload_whisper: AtomicBool::new(low_memory()),
            }),
            finalizers: Mutex::default(),
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
        let cancel = Arc::new(AtomicBool::new(false));
        let mut started = Vec::new();
        for &source in sources(mode) {
            match inner.open(&session.id, source, app, &cancel) {
                Ok(s) => started.push(s),
                Err(e) => {
                    // Not waiting: a streamer may be loading whisper.
                    abandon(started, &cancel, false);
                    if let Err(e) = inner.store.delete_session(&session.id) {
                        eprintln!("record {}: deleting failed start: {e}", session.id);
                    }
                    return Err(e);
                }
            }
        }
        let (watched, unwatched) = mpsc::channel();
        let watchdog = {
            let inner = Arc::clone(inner);
            let id = session.id.clone();
            std::thread::Builder::new()
                .name("record-watch".into())
                .spawn(move || inner.watch(&id, &unwatched))
        };
        if let Err(e) = watchdog {
            abandon(started, &cancel, false);
            if let Err(e) = inner.store.delete_session(&session.id) {
                eprintln!("record {}: deleting failed start: {e}", session.id);
            }
            return Err(e.into());
        }
        *recording = Some(Recording {
            session_id: session.id.clone(),
            sources: started,
            cancel,
            _watched: watched,
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
        let mut streamers = Vec::new();
        for source in recording.sources {
            let (written, streamer) = source.finish();
            match written {
                Ok(n) => samples = samples.max(n),
                Err(e) => {
                    failure.get_or_insert(e);
                }
            }
            streamers.push(streamer);
        }
        if failure.is_some() || samples == 0 {
            recording.cancel.store(true, Ordering::Relaxed);
            // The chunks already written stay for a retry.
            let e = failure.unwrap_or_else(|| "no audio was recorded".into());
            inner.end(&id, SessionStatus::Failed, &e);
            return Ok(id);
        }
        let duration_ms =
            u32::try_from(samples * 1000 / u64::from(SAMPLE_RATE)).unwrap_or(u32::MAX);

        // The streamers' flag, so a cancel also stops the window in flight.
        let cancel = Arc::clone(&recording.cancel);
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
                    inner.finalize(&id, duration_ms, streamers, &cancel);
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
        // Not `if let Some(..) = lock(..).take()`: the guard would live through
        // the block, and a streamer locks `recording` before it can stop.
        let recording = lock(&self.inner.recording).take();
        if let Some(recording) = recording {
            abandon(recording.sources, &recording.cancel, true);
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
        self: &Arc<Self>,
        session_id: &str,
        source: AudioSource,
        app: Option<&str>,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Source, RecordError> {
        let (sink, rx) = chunk_writer::channel();
        let (live, audio) = mpsc::channel();
        let capture = match source {
            AudioSource::Mic => self.mic.open(sink)?,
            AudioSource::System => self
                .system
                .open(app.ok_or(RecordError::NoTargetApp)?, sink)?,
        };
        let format = capture.format();
        // A zero rate would stall the resampler forever.
        if format.sample_rate == 0 {
            return Err(AudioError::Device(format!(
                "{} reports a 0 Hz sample rate",
                source.as_str()
            ))
            .into());
        }
        let writer = chunk_writer::spawn(
            Arc::clone(&self.store),
            session_id.into(),
            source,
            format,
            rx,
            live,
        )?;
        let streamer = {
            let inner = Arc::clone(self);
            let id = session_id.to_owned();
            let cancel = Arc::clone(cancel);
            std::thread::Builder::new()
                .name(format!("stream-{}", source.as_str()))
                .spawn(move || inner.stream(&id, source, &audio, &cancel))?
        };
        Ok(Source {
            source,
            capture,
            writer,
            streamer,
        })
    }

    /// Whisper, loading it if no recording has yet.
    fn transcriber(&self) -> Result<Arc<dyn Transcriber>, String> {
        let mut model = lock(&self.model);
        if let Some(model) = &*model {
            return Ok(Arc::clone(model));
        }
        let loaded = self.stt.load().map_err(|e| e.to_string())?;
        *model = Some(Arc::clone(&loaded));
        Ok(loaded)
    }

    /// Transcribes `source` while it records, one window per 15 s of audio
    /// (M5b). Returns once the audio ends, with the windows not yet done.
    fn stream(
        &self,
        id: &str,
        source: AudioSource,
        audio: &mpsc::Receiver<Vec<f32>>,
        cancel: &AtomicBool,
    ) -> Result<Stream, String> {
        let stt = self.transcriber()?;
        let mut stream = Stream::default();
        let mut catching_up = false;
        loop {
            if stream.queued() == 0 {
                let Ok(samples) = audio.recv() else {
                    return Ok(stream);
                };
                stream.push(&samples);
            }
            loop {
                match audio.try_recv() {
                    Ok(samples) => stream.push(&samples),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(stream),
                }
            }
            if cancel.load(Ordering::Relaxed) {
                return Ok(stream);
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
                if recording.as_ref().is_some_and(|r| r.session_id == id) {
                    self.emit(id, SessionStatus::Recording, message);
                }
            }
            let Some(window) = stream.next() else {
                continue;
            };
            if !self.window(id, source, &mut stream, &window, &*stt, cancel)? {
                return Ok(stream);
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
        cancel: &AtomicBool,
    ) -> Result<bool, String> {
        let segments = {
            let _turn = lock(&self.transcribing);
            stream.transcribe(window, stt, cancel)
        };
        let segments = match segments {
            Ok(segments) => segments,
            Err(SttError::Cancelled) => return Ok(false),
            Err(e) => return Err(e.to_string()),
        };
        if segments.is_empty() {
            return Ok(true);
        }
        for s in &segments {
            let segment = Segment {
                start_ms: s.start_ms,
                end_ms: s.end_ms,
                text: Some(s.text.clone()),
            };
            self.store
                .add_segment(id, source, &segment)
                .map_err(|e| e.to_string())?;
        }
        self.events.segments(SessionSegments {
            session_id: id.into(),
            source,
            segments,
        });
        Ok(true)
    }

    /// Fails the recording of `id` as soon as one of its sources stops
    /// writing — the device or app went away, or a chunk failed to save —
    /// keeping the chunks written so far. Returns when the recording ends.
    fn watch(&self, id: &str, recording_ends: &mpsc::Receiver<()>) {
        while let Err(RecvTimeoutError::Timeout) = recording_ends.recv_timeout(WATCH_INTERVAL) {
            let mut current = lock(&self.recording);
            let stopped = current.as_ref().and_then(|r| {
                (r.session_id == id)
                    .then(|| r.sources.iter().find(|s| s.writer.is_finished()))
                    .flatten()
                    .map(|s| s.source)
            });
            let Some(stopped) = stopped else { continue };
            let Some(recording) = current.take() else {
                return;
            };
            drop(current);
            let error = abandon(recording.sources, &recording.cancel, true);
            let error = error.unwrap_or_else(|| match stopped {
                AudioSource::Mic => "recording stopped: the microphone was lost".into(),
                AudioSource::System => "recording stopped: the app's audio ended".into(),
            });
            return self.end(id, SessionStatus::Failed, &error);
        }
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

    fn finalize(&self, id: &str, duration_ms: u32, streamers: Vec<Streamer>, cancel: &AtomicBool) {
        let started = Instant::now();
        // The summary model loads while the tail transcribes (M5b).
        self.jobs.preload();
        let drained = self.drain(id, streamers, cancel);
        if self.unload_whisper.load(Ordering::Relaxed) && lock(&self.recording).is_none() {
            lock(&self.model).take();
        }
        let (transcript, partial) =
            match drained.and_then(|partial| self.transcript(id).map(|t| (t, partial))) {
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

    /// Transcribes what each source's streamer left: its queued windows and
    /// the tail after them. Returns whether a cancel cut it short.
    fn drain(
        &self,
        id: &str,
        streamers: Vec<Streamer>,
        cancel: &AtomicBool,
    ) -> Result<bool, String> {
        // Joined first, so none outlives a failure.
        let streams: Vec<Result<Stream, String>> = streamers
            .into_iter()
            .map(|s| {
                s.join()
                    .unwrap_or_else(|_| Err("the transcriber crashed".into()))
            })
            .collect();
        let session = self.session(id)?;
        let mut partial = false;
        for (&source, stream) in sources(session.input_mode).iter().zip(streams) {
            let mut stream = stream?;
            stream.finish();
            while !partial {
                let Some(window) = stream.next() else { break };
                if cancel.load(Ordering::Relaxed) {
                    partial = true;
                    break;
                }
                // Loaded on demand: a recording that streamed every window
                // needs no model here.
                let stt = self.transcriber()?;
                partial = !self.window(id, source, &mut stream, &window, &*stt, cancel)?;
            }
        }
        Ok(partial)
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
