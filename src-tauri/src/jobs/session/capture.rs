//! Capture spans share one active timeline. The microphone is the sample clock
//! for app audio, while the monotonic clock drives the UI and hearing warning.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, RecvTimeoutError},
        Arc, OnceLock,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use super::{lock, Inner, RecordError, Transcription};
use crate::{
    audio::{chunk_writer, ring, AudioError, Capture, Format},
    jobs::{
        AppAudioStatus, HearingState, JobEvents, RecordingState, RecordingStatus, SessionLevel,
    },
    store::{self, AudioSource, SessionStatus},
    stt::{stream::SILENCE_RMS, SAMPLE_RATE},
};

const WATCH_INTERVAL: Duration = Duration::from_millis(100);
const RETRY_INTERVAL: Duration = Duration::from_secs(1);
const CALL_WARNING_MS: u32 = 20_000;
const MIC_LOST: &str = "Recording stopped: the microphone was lost.";
const WRITE_FAILED: &str = "Recording stopped: couldn't save audio.";

pub(super) type Streamer = (mpsc::Sender<SpanAudio>, JoinHandle<Result<bool, String>>);

pub(super) struct SpanAudio {
    pub(super) live: mpsc::Receiver<(usize, Vec<f32>)>,
    pub(super) saved_end: Arc<OnceLock<usize>>,
}

pub(super) struct Source {
    capture: Box<dyn Capture>,
    consumer: ring::Consumer,
    format: Format,
}

impl Source {
    pub(super) fn prepare(
        open: impl FnOnce(crate::audio::Sink) -> crate::audio::Result<Box<dyn Capture>>,
    ) -> Result<Self, AudioError> {
        let (sink, consumer) = chunk_writer::channel();
        let capture = open(sink)?;
        let format = capture.format();
        if format.sample_rate == 0 {
            return Err(AudioError::Device(
                "capture reports a 0 Hz sample rate".into(),
            ));
        }
        if format.channels == 0 {
            return Err(AudioError::Device(
                "capture reports no audio channels".into(),
            ));
        }
        Ok(Self {
            capture,
            consumer,
            format,
        })
    }
}

fn samples_16k(format: Format, interleaved: usize) -> usize {
    (interleaved as u64 / u64::from(format.channels) * u64::from(SAMPLE_RATE)
        / u64::from(format.sample_rate)) as usize
}

#[derive(Clone)]
struct SamplePosition {
    clock: ring::SampleClock,
    format: Format,
    start: usize,
}

impl SamplePosition {
    fn now(&self) -> usize {
        self.at(self.clock.received())
    }

    fn at(&self, native_samples: usize) -> usize {
        self.start + samples_16k(self.format, native_samples)
    }
}

struct Span {
    capture: Box<dyn Capture>,
    position: SamplePosition,
    writer: SpanWriter,
}

struct SpanWriter {
    worker: JoinHandle<store::Result<chunk_writer::Timeline>>,
    saved_end: Arc<OnceLock<usize>>,
}

#[derive(Default)]
struct Track {
    span: Option<Span>,
    stream: Option<Streamer>,
    timeline: chunk_writer::Timeline,
    level: Arc<AtomicU32>,
}

impl Track {
    fn attach(
        &mut self,
        inner: &Arc<Inner>,
        id: &str,
        source: AudioSource,
        ready: Source,
        align: Option<Box<dyn FnOnce() -> usize + Send>>,
        transcription: &Arc<Transcription>,
    ) -> Result<(), RecordError> {
        let Source {
            capture,
            consumer,
            format,
        } = ready;
        let position = SamplePosition {
            clock: consumer.clock(),
            format,
            start: self.timeline.samples,
        };
        self.level.store(0, Ordering::Relaxed);
        let (sender, live) = mpsc::channel();
        let saved_end = Arc::new(OnceLock::new());
        if self.stream.is_none() {
            let (next_span, spans) = mpsc::channel();
            let inner = Arc::clone(inner);
            let id = id.to_owned();
            let transcription = Arc::clone(transcription);
            let worker = thread::Builder::new()
                .name(format!("stream-{}", source.as_str()))
                .spawn(move || inner.stream_spans(&id, source, &spans, &transcription))?;
            self.stream = Some((next_span, worker));
        }
        let worker = chunk_writer::spawn(
            Arc::clone(&inner.store),
            id.into(),
            source,
            chunk_writer::Input {
                format,
                consumer,
                timeline: self.timeline.clone(),
                level: Arc::clone(&self.level),
                align,
            },
            sender,
        )?;
        let (next_span, _) = self.stream.as_ref().expect("track has a streamer");
        // Capture must keep saving PCM if transcription is cancelled or fails to save segments.
        let _ = next_span.send(SpanAudio {
            live,
            saved_end: Arc::clone(&saved_end),
        });
        self.timeline = chunk_writer::Timeline::default();
        self.span = Some(Span {
            capture,
            position,
            writer: SpanWriter { worker, saved_end },
        });
        Ok(())
    }

    fn lost(&self) -> bool {
        self.span
            .as_ref()
            .is_some_and(|span| span.writer.worker.is_finished())
    }

    fn stop_capture(&mut self) -> Option<SpanWriter> {
        let Span {
            capture, writer, ..
        } = self.span.take()?;
        drop(capture);
        Some(writer)
    }

    fn flush(&mut self, writer: Option<SpanWriter>) -> Result<(), &'static str> {
        if let Some(writer) = writer {
            match writer.worker.join() {
                Ok(Ok(timeline)) => {
                    let _ = writer.saved_end.set(timeline.samples);
                    self.timeline = timeline;
                }
                Ok(Err(error)) => {
                    crate::diag::log::log("ERROR", "recording", crate::diag::log::kind(&error));
                    return Err(WRITE_FAILED);
                }
                Err(_) => return Err(WRITE_FAILED),
            }
        }
        Ok(())
    }
}

struct AppAttempt {
    cancel: Arc<AtomicBool>,
    worker: JoinHandle<Result<Source, AudioError>>,
    earliest_sample: usize,
    mic: SamplePosition,
    first_block: Arc<OnceLock<(usize, usize)>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AttachFailure {
    PermissionRequired,
    SourceUnavailable,
    AttachFailed,
    WorkerFailed,
    AudioWorkerFailed,
}

impl AttachFailure {
    fn log_kind(self) -> &'static str {
        match self {
            Self::PermissionRequired => "app_audio_permission_denied",
            Self::SourceUnavailable => "app_source_unavailable",
            Self::AttachFailed => "app_attach_failed",
            Self::WorkerFailed => "app_attach_worker_failed",
            Self::AudioWorkerFailed => "app_audio_worker_failed",
        }
    }
}

pub(super) struct Recording {
    pub(super) session_id: String,
    pub(super) multi_speaker: bool,
    pub(super) transcription: Arc<Transcription>,
    mic: Track,
    system: Track,
    target_app: Option<String>,
    attach: Option<AppAttempt>,
    retry_at: Instant,
    running_since: Option<Instant>,
    elapsed: Duration,
    last_call_heard_ms: u32,
    last_attach_error: Option<AttachFailure>,
    /// Dropped with the recording, which ends its watchdog.
    _watched: mpsc::Sender<()>,
}

impl Recording {
    pub(super) fn start(
        inner: &Arc<Inner>,
        id: &str,
        multi_speaker: bool,
        target_app: Option<String>,
        mic: Source,
    ) -> Result<Self, RecordError> {
        let (watched, unwatched) = mpsc::channel();
        let mut recording = Self {
            session_id: id.into(),
            multi_speaker,
            transcription: Arc::default(),
            mic: Track::default(),
            system: Track::default(),
            target_app,
            attach: None,
            retry_at: Instant::now(),
            running_since: None,
            elapsed: Duration::ZERO,
            last_call_heard_ms: 0,
            last_attach_error: None,
            _watched: watched,
        };
        {
            let inner = Arc::clone(inner);
            let id = id.to_owned();
            thread::Builder::new()
                .name("record-watch".into())
                .spawn(move || inner.watch(&id, &unwatched))?;
        }
        recording.start_mic(inner, mic)?;
        Ok(recording)
    }

    fn start_mic(&mut self, inner: &Arc<Inner>, mic: Source) -> Result<(), RecordError> {
        self.mic.attach(
            inner,
            &self.session_id,
            AudioSource::Mic,
            mic,
            None,
            &self.transcription,
        )?;
        self.running_since = Some(Instant::now());
        Ok(())
    }

    pub(super) fn running(&self) -> bool {
        self.running_since.is_some()
    }

    fn elapsed_ms(&self) -> u32 {
        let elapsed = self.elapsed + self.running_since.map_or(Duration::ZERO, |t| t.elapsed());
        u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX)
    }

    pub(super) fn snapshot(&self) -> RecordingState {
        RecordingState {
            session_id: self.session_id.clone(),
            status: if self.running() {
                RecordingStatus::Running
            } else {
                RecordingStatus::Paused
            },
            active_elapsed_ms: self.elapsed_ms(),
            target_app: self.target_app.clone(),
            app_audio_status: self.target_app.as_ref().map(|_| {
                if self.system.span.is_some() {
                    AppAudioStatus::Capturing
                } else if self.last_attach_error == Some(AttachFailure::PermissionRequired) {
                    AppAudioStatus::PermissionRequired
                } else if self.last_attach_error.is_some() {
                    AppAudioStatus::Unavailable
                } else {
                    AppAudioStatus::Pending
                }
            }),
        }
    }

    pub(super) fn samples(&self) -> usize {
        self.mic.timeline.samples.max(self.system.timeline.samples)
    }

    pub(super) fn take_streamers(&mut self) -> Vec<Streamer> {
        [&mut self.system, &mut self.mic]
            .into_iter()
            .filter_map(|track| track.stream.take())
            .collect()
    }

    pub(super) fn pause(&mut self, inner: &Arc<Inner>) -> Result<(), &'static str> {
        if let Some(start) = self.running_since.take() {
            self.elapsed += start.elapsed();
        }
        self.transcription.pause();
        self.close(inner)
    }

    pub(super) fn resume(&mut self, inner: &Arc<Inner>) -> Result<(), RecordError> {
        let mic = Source::prepare(|sink| inner.mic.open(sink))?;
        self.start_mic(inner, mic)?;
        self.retry_at = Instant::now();
        self.transcription.resume();
        Ok(())
    }

    /// Ends the current spans but keeps their chunks and transcription windows.
    pub(super) fn close(&mut self, inner: &Arc<Inner>) -> Result<(), &'static str> {
        let mic_lost = self.mic.lost();
        let mic = self.mic.stop_capture();
        self.poll_attach(inner, true);
        let system = self.system.stop_capture();
        let mic = self.mic.flush(mic);
        let system = self.system.flush(system);
        mic.and(system)?;
        if mic_lost {
            return Err(MIC_LOST);
        }
        Ok(())
    }

    fn poll_attach(&mut self, inner: &Arc<Inner>, closing: bool) {
        if self
            .attach
            .as_ref()
            .is_some_and(|attempt| closing || attempt.worker.is_finished())
        {
            let attempt = self.attach.take().expect("checked attach worker");
            if closing {
                attempt.cancel.store(true, Ordering::Relaxed);
            }
            self.retry_at = Instant::now() + RETRY_INTERVAL;
            match attempt.worker.join() {
                Ok(Ok(ready)) => {
                    let format = ready.format;
                    let align = Box::new(move || {
                        attempt.first_block.get().map_or(
                            attempt.earliest_sample,
                            |&(mic_samples, app_samples)| {
                                attempt
                                    .mic
                                    .at(mic_samples)
                                    .saturating_sub(samples_16k(format, app_samples))
                                    .max(attempt.earliest_sample)
                            },
                        )
                    });
                    match self.system.attach(
                        inner,
                        &self.session_id,
                        AudioSource::System,
                        ready,
                        Some(align),
                        &self.transcription,
                    ) {
                        Ok(()) => self.last_attach_error = None,
                        Err(_) => self.log_attach_failure(AttachFailure::AudioWorkerFailed),
                    }
                }
                Ok(Err(error)) if !closing => {
                    let kind = match error {
                        AudioError::AppNotFound(_) => AttachFailure::SourceUnavailable,
                        AudioError::PermissionDenied(_) => AttachFailure::PermissionRequired,
                        _ => AttachFailure::AttachFailed,
                    };
                    self.log_attach_failure(kind);
                }
                Err(_) if !closing => self.log_attach_failure(AttachFailure::WorkerFailed),
                _ => {}
            }
        }
        if closing
            || self.system.span.is_some()
            || self.attach.is_some()
            || Instant::now() < self.retry_at
        {
            return;
        }
        let Some(app) = self.target_app.clone() else {
            return;
        };
        let mic = self
            .mic
            .span
            .as_ref()
            .expect("attach needs the microphone")
            .position
            .clone();
        let earliest_sample = mic.now();
        let mic_clock = mic.clock.clone();
        let first_block = Arc::new(OnceLock::new());
        let observed = Arc::clone(&first_block);
        let system = Arc::clone(&inner.system);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&cancel);
        match thread::Builder::new()
            .name("attach-app-audio".into())
            .spawn(move || {
                Source::prepare(|mut sink| {
                    sink.align_to(mic_clock, observed);
                    system.open(&app, sink, &cancelled)
                })
            }) {
            Ok(worker) => {
                self.attach = Some(AppAttempt {
                    worker,
                    cancel,
                    earliest_sample,
                    mic,
                    first_block,
                })
            }
            Err(_) => {
                self.retry_at = Instant::now() + RETRY_INTERVAL;
                self.log_attach_failure(AttachFailure::WorkerFailed);
            }
        }
    }

    fn log_attach_failure(&mut self, kind: AttachFailure) {
        if self.last_attach_error != Some(kind) {
            self.last_attach_error = Some(kind);
            crate::diag::log::log("WARN", "recording", kind.log_kind());
        }
    }

    fn tick(&mut self, inner: &Arc<Inner>) -> Result<(), &'static str> {
        if !self.running() {
            return Ok(());
        }
        self.emit_levels(&*inner.events);
        if self.transcription.store_failed.load(Ordering::Relaxed) {
            return Err("Recording stopped: couldn't save transcript segments.");
        }
        if self.mic.lost() {
            return Err(MIC_LOST);
        }
        if self.system.lost() {
            let writer = self.system.stop_capture();
            self.system.flush(writer)?;
        }
        self.poll_attach(inner, false);
        Ok(())
    }

    fn emit_levels(&mut self, events: &dyn JobEvents) {
        let elapsed = self.elapsed_ms();
        for (source, track) in [
            (AudioSource::Mic, &self.mic),
            (AudioSource::System, &self.system),
        ] {
            if source == AudioSource::System && self.target_app.is_none() {
                continue;
            }
            let level = f32::from_bits(track.level.swap(0, Ordering::Relaxed));
            let hearing = hearing(source, level, elapsed, &mut self.last_call_heard_ms);
            events.level(SessionLevel {
                session_id: self.session_id.clone(),
                source,
                level,
                hearing,
                active_elapsed_ms: elapsed,
            });
        }
    }
}

fn hearing(
    source: AudioSource,
    level: f32,
    elapsed_ms: u32,
    last_call_heard_ms: &mut u32,
) -> HearingState {
    if level >= SILENCE_RMS {
        if source == AudioSource::System {
            *last_call_heard_ms = elapsed_ms;
        }
        HearingState::Hearing
    } else if source == AudioSource::System && elapsed_ms - *last_call_heard_ms >= CALL_WARNING_MS {
        HearingState::Warning
    } else {
        HearingState::Silent
    }
}

#[cfg(test)]
#[test]
fn call_silence_warns_after_twenty_active_seconds_and_hearing_resets_it() {
    let mut last_heard = 0;
    let mut call = |level, elapsed| hearing(AudioSource::System, level, elapsed, &mut last_heard);
    assert_eq!(call(0.0, 19_999), HearingState::Silent);
    assert_eq!(call(0.0, 20_000), HearingState::Warning);
    assert_eq!(call(0.5, 20_100), HearingState::Hearing);
    assert_eq!(call(0.0, 20_200), HearingState::Silent);
    assert_eq!(call(0.0, 40_100), HearingState::Warning);
    assert_eq!(
        hearing(AudioSource::Mic, 0.0, 40_100, &mut last_heard),
        HearingState::Silent
    );
}

impl Inner {
    fn watch(self: &Arc<Self>, id: &str, recording_ends: &mpsc::Receiver<()>) {
        loop {
            {
                let mut current = lock(&self.recording);
                let Some(recording) = current.as_mut().filter(|r| r.session_id == id) else {
                    return;
                };
                let before = recording.snapshot().app_audio_status;
                if let Err(error) = recording.tick(self) {
                    self.interrupt(current.take().expect("active recording"), error);
                    return;
                }
                if recording.snapshot().app_audio_status != before {
                    self.events.recording_state(Some(recording.snapshot()));
                }
            }
            if !matches!(
                recording_ends.recv_timeout(WATCH_INTERVAL),
                Err(RecvTimeoutError::Timeout)
            ) {
                return;
            }
        }
    }

    pub(super) fn recording_ended(&self) {
        self.events.recording_changed(false, false);
        self.events.recording_state(None);
    }

    pub(super) fn interrupt(self: &Arc<Self>, mut recording: Recording, error: &'static str) {
        self.recording_ended();
        recording.transcription.cancel();
        let error = recording
            .close(self)
            .err()
            .filter(|&reason| reason != MIC_LOST)
            .unwrap_or(error);
        self.interrupt_audio(&recording.session_id, error, &recording.transcription);
    }

    pub(super) fn interrupt_audio(
        &self,
        id: &str,
        error: &'static str,
        transcription: &Transcription,
    ) {
        transcription.cancel();
        let has_audio = self
            .store
            .audio_chunk_count(id)
            .is_ok_and(|count| count > 0);
        if has_audio {
            if let Err(error) = self.store.set_recovery_status(id, SessionStatus::Failed) {
                crate::diag::log::log("ERROR", "recording", crate::diag::log::kind(&error));
            }
            self.emit(id, SessionStatus::Failed, error);
        } else {
            self.end(id, SessionStatus::Failed, error);
        }
    }
}
