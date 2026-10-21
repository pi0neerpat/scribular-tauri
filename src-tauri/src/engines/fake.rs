//! Deterministic engines for tests (KTD9): a [`Summarizer`] where one
//! whitespace-separated word is one token, so tests can reason about budgets
//! and chunk counts exactly, a [`Transcriber`] with scripted segments, and
//! audio sources a test feeds by hand.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use crate::audio::{
    AudioApp, AudioError, Capture, Format, Microphone, Privacy, Result as AudioResult, Sink,
    SystemAudio,
};
use crate::llm::{GenEvent, Generation, LlmError, Result, Summarizer, SummarizerLoader};
use crate::stt::{
    Result as SttResult, SttError, Transcriber, TranscriberLoader, TranscriptSegment,
};

/// Tokens the fake chat template adds around a user turn.
pub const TEMPLATE_TOKENS: usize = 4;

pub struct FakeSummarizer {
    pub context: usize,
    /// Tokens each sequence produces before its end-of-turn.
    pub reply_tokens: usize,
    pub token_delay: Duration,
    /// Every request passed to `generate`, call by call.
    pub calls: Mutex<Vec<Vec<Generation>>>,
    active: AtomicUsize,
    /// Most `generate` calls that were ever running at once.
    pub max_active: AtomicUsize,
}

impl FakeSummarizer {
    pub fn new(context: usize) -> Self {
        Self {
            context,
            reply_tokens: 5,
            token_delay: Duration::ZERO,
            calls: Mutex::default(),
            active: AtomicUsize::new(0),
            max_active: AtomicUsize::new(0),
        }
    }

    pub fn calls(&self) -> Vec<Vec<Generation>> {
        self.calls.lock().unwrap().clone()
    }
}

impl Summarizer for FakeSummarizer {
    fn context_tokens(&self) -> usize {
        self.context
    }

    fn count_tokens(&self, text: &str) -> Result<usize> {
        Ok(text.split_whitespace().count())
    }

    fn prompt_tokens(&self, prompt: &str) -> Result<usize> {
        Ok(self.count_tokens(prompt)? + TEMPLATE_TOKENS)
    }

    /// Decodes round-robin, one token per live sequence per step, so the
    /// shortest sequence finishes first. Output for sequence `i` is
    /// `s{i}t0 s{i}t1 …`.
    fn generate(
        &self,
        requests: &[Generation],
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(GenEvent<'_>),
    ) -> Result<Vec<String>> {
        self.calls.lock().unwrap().push(requests.to_vec());
        let now = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_active.fetch_max(now, Ordering::SeqCst);
        struct Leave<'a>(&'a AtomicUsize);
        impl Drop for Leave<'_> {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::SeqCst);
            }
        }
        let _leave = Leave(&self.active);

        for r in requests {
            if self.prompt_tokens(&r.prompt)? + r.max_tokens > self.context {
                return Err(LlmError::Engine("sequence exceeds the context".into()));
            }
        }
        let lens: Vec<usize> = requests
            .iter()
            .map(|r| r.max_tokens.min(self.reply_tokens))
            .collect();
        let mut outputs = vec![String::new(); requests.len()];
        for step in 0..lens.iter().copied().max().unwrap_or(0) {
            for (seq, &len) in lens.iter().enumerate() {
                if step >= len {
                    continue;
                }
                if cancel.load(Ordering::Relaxed) {
                    return Err(LlmError::Cancelled);
                }
                std::thread::sleep(self.token_delay);
                let text = format!("{}s{seq}t{step}", if step == 0 { "" } else { " " });
                outputs[seq].push_str(&text);
                on_event(GenEvent::Token { seq, text: &text });
                if step + 1 == len {
                    on_event(GenEvent::Finished { seq });
                }
            }
        }
        Ok(outputs)
    }
}

/// Hands out one shared [`FakeSummarizer`] and counts loads.
pub struct FakeLoader {
    pub summarizer: Arc<FakeSummarizer>,
    pub loads: AtomicUsize,
    /// When set, `load` fails with this error instead.
    pub fail: Option<LlmError>,
}

impl FakeLoader {
    pub fn new(summarizer: FakeSummarizer) -> Self {
        Self {
            summarizer: Arc::new(summarizer),
            loads: AtomicUsize::new(0),
            fail: None,
        }
    }
}

impl SummarizerLoader for FakeLoader {
    fn model_id(&self) -> &str {
        "fake-llm"
    }

    fn load(&self) -> Result<Arc<dyn Summarizer>> {
        self.loads.fetch_add(1, Ordering::SeqCst);
        match &self.fail {
            Some(e) => Err(e.clone()),
            None => Ok(self.summarizer.clone()),
        }
    }
}

/// A [`Transcriber`] that returns the same scripted segments for any audio.
pub struct FakeTranscriber {
    pub segments: Vec<TranscriptSegment>,
    /// The sample count of every `transcribe` call.
    pub calls: Mutex<Vec<usize>>,
    /// Calls from this index on wait for their cancel flag, then return
    /// `Cancelled`.
    pub block_from: usize,
    /// While set, calls wait before answering, as a slow machine would.
    pub hold: AtomicBool,
    /// How many times the loader handed this transcriber out.
    pub loads: AtomicUsize,
}

impl FakeTranscriber {
    pub fn new(segments: Vec<TranscriptSegment>) -> Self {
        Self {
            segments,
            calls: Mutex::default(),
            block_from: usize::MAX,
            hold: AtomicBool::new(false),
            loads: AtomicUsize::new(0),
        }
    }

    pub fn calls(&self) -> Vec<usize> {
        self.calls.lock().unwrap().clone()
    }
}

impl Transcriber for FakeTranscriber {
    fn transcribe(
        &self,
        samples: &[f32],
        cancel: &AtomicBool,
    ) -> SttResult<Vec<TranscriptSegment>> {
        let index = {
            let mut calls = self.calls.lock().unwrap();
            calls.push(samples.len());
            calls.len() - 1
        };
        while self.hold.load(Ordering::SeqCst) && !cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(2));
        }
        if index >= self.block_from {
            while !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(SttError::Cancelled);
        }
        Ok(self.segments.clone())
    }
}

/// Hands out one shared [`FakeTranscriber`] and counts loads.
pub struct FakeTranscriberLoader(pub Arc<FakeTranscriber>);

impl TranscriberLoader for FakeTranscriberLoader {
    fn load(&self) -> SttResult<Arc<dyn Transcriber>> {
        self.0.loads.fetch_add(1, Ordering::SeqCst);
        Ok(self.0.clone())
    }
}

/// An audio source the test drives: `feed` calls the open capture's sink.
pub struct FakeSource {
    pub format: Format,
    sink: Arc<Mutex<Option<Sink>>>,
    /// Makes `open` fail as if macOS refused access.
    pub deny: AtomicBool,
    /// How many times `open` succeeded.
    pub opened: AtomicUsize,
}

impl FakeSource {
    pub fn new(format: Format) -> Self {
        Self {
            format,
            sink: Arc::default(),
            deny: AtomicBool::new(false),
            opened: AtomicUsize::new(0),
        }
    }

    /// Delivers `samples` (interleaved, native format). Returns whether a
    /// capture accepted them, retrying backpressure for at most ten seconds.
    pub fn feed(&self, samples: &[f32]) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            {
                let mut capture = self.sink.lock().unwrap();
                match capture.as_mut() {
                    Some(sink) => {
                        if sink.try_push(samples) {
                            return true;
                        }
                    }
                    None => return false,
                }
            }
            // Let lose() and capture drop take the mutex between attempts.
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Stops delivering, as when the device or app goes away mid-recording.
    pub fn lose(&self) {
        self.sink.lock().unwrap().take();
    }

    pub fn is_open(&self) -> bool {
        self.sink.lock().unwrap().is_some()
    }

    fn open(&self, privacy: Privacy, sink: Sink) -> AudioResult<Box<dyn Capture>> {
        if self.deny.load(Ordering::Relaxed) {
            return Err(AudioError::PermissionDenied(privacy));
        }
        *self.sink.lock().unwrap() = Some(sink);
        self.opened.fetch_add(1, Ordering::Relaxed);
        Ok(Box::new(FakeCapture {
            format: self.format,
            sink: Arc::clone(&self.sink),
        }))
    }
}

struct FakeCapture {
    format: Format,
    sink: Arc<Mutex<Option<Sink>>>,
}

impl Capture for FakeCapture {
    fn format(&self) -> Format {
        self.format
    }
}

impl Drop for FakeCapture {
    fn drop(&mut self) {
        self.sink.lock().unwrap().take();
    }
}

pub struct FakeMicrophone(pub Arc<FakeSource>);

impl Microphone for FakeMicrophone {
    fn open(&self, sink: Sink) -> AudioResult<Box<dyn Capture>> {
        self.0.open(Privacy::Microphone, sink)
    }
}

/// System audio where only `app` is playing.
pub struct FakeSystemAudio {
    pub source: Arc<FakeSource>,
    pub app: String,
}

impl SystemAudio for FakeSystemAudio {
    fn list_apps(&self) -> AudioResult<Vec<AudioApp>> {
        Ok(vec![AudioApp {
            name: self.app.clone(),
            sample_rate: self.source.format.sample_rate,
            channels: self.source.format.channels,
            icon: None,
        }])
    }

    fn open(&self, app: &str, sink: Sink) -> AudioResult<Box<dyn Capture>> {
        if app != self.app {
            return Err(AudioError::AppNotFound(app.into()));
        }
        self.source.open(Privacy::SystemAudio, sink)
    }
}
