//! Streaming transcription (U9, M5b): one source's audio is cut into 20 s
//! windows every 15 s, so each window repeats the last 5 s of the one before
//! and no word is cut at a boundary. Segments in the overlap are dropped by
//! [`Dedup`]. Windows queue rather than drop, so a slow machine catches up
//! after the recording stops.

use std::{collections::VecDeque, sync::atomic::AtomicBool};

use super::{Result, Transcriber, TranscriptSegment, SAMPLE_RATE};

const SECOND: usize = SAMPLE_RATE as usize;
/// The length of a full window.
pub const WINDOW: usize = 20 * SECOND;
/// How often a window is cut.
pub const HOP: usize = 15 * SECOND;
/// The audio each window shares with the one before.
pub const OVERLAP: usize = WINDOW - HOP;
/// Windows quieter than this are not transcribed; whisper invents text in
/// silence.
pub const SILENCE_RMS: f32 = 0.002;
/// More queued windows than this shows "Catching up...".
pub const CATCHING_UP: usize = 2;

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    (sum / samples.len() as f32).sqrt()
}

/// A span of audio to transcribe; `start` counts samples from the first one
/// recorded.
#[derive(Debug, Clone)]
pub struct Window {
    pub start: usize,
    pub samples: Vec<f32>,
}

impl Window {
    pub fn offset_ms(&self) -> i64 {
        (self.start as u64 * 1000 / SAMPLE_RATE as u64) as i64
    }
}

/// Keeps the segments that start after those kept from earlier windows.
/// Segments within one window touch end to start, so they never drop each
/// other.
#[derive(Debug, Default)]
pub struct Dedup {
    last_end_ms: Option<i64>,
}

impl Dedup {
    /// Shifts a window's segments by `offset_ms` to recording time and
    /// returns the new ones.
    pub fn keep(&mut self, offset_ms: i64, segments: Vec<TranscriptSegment>) -> Vec<TranscriptSegment> {
        let seen = self.last_end_ms;
        let kept: Vec<TranscriptSegment> = segments
            .into_iter()
            .map(|mut segment| {
                segment.start_ms += offset_ms;
                segment.end_ms += offset_ms;
                segment
            })
            .filter(|segment| seen.is_none_or(|last| segment.start_ms > last))
            .collect();
        if let Some(end) = kept.iter().map(|s| s.end_ms).max() {
            self.last_end_ms = Some(seen.map_or(end, |last| last.max(end)));
        }
        kept
    }
}

/// One source's windows: audio in, queued windows out.
#[derive(Debug, Default)]
pub struct Stream {
    /// Audio from `base` on; older samples no window needs are dropped.
    audio: Vec<f32>,
    base: usize,
    /// Where the last window ended.
    covered: usize,
    queue: VecDeque<Window>,
    dedup: Dedup,
}

impl Stream {
    fn received(&self) -> usize {
        self.base + self.audio.len()
    }

    fn cut(&mut self, start: usize, end: usize) {
        let samples = self.audio[start - self.base..end - self.base].to_vec();
        self.queue.push_back(Window { start, samples });
        self.covered = end;
        let keep_from = end.saturating_sub(OVERLAP).max(self.base);
        self.audio.drain(..keep_from - self.base);
        self.base = keep_from;
    }

    /// Adds 16 kHz mono audio, queueing a window each time another `HOP`
    /// has arrived.
    pub fn push(&mut self, samples: &[f32]) {
        self.audio.extend_from_slice(samples);
        while self.received() >= self.covered + HOP {
            let end = self.covered + HOP;
            self.cut(end.saturating_sub(WINDOW), end);
        }
    }

    /// Queues the audio after the last window, with its overlap.
    pub fn finish(&mut self) {
        let end = self.received();
        if end > self.covered {
            self.cut(self.covered.saturating_sub(OVERLAP), end);
        }
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    pub fn catching_up(&self) -> bool {
        self.queued() > CATCHING_UP
    }

    pub fn next(&mut self) -> Option<Window> {
        self.queue.pop_front()
    }

    /// Transcribes `window` and returns its new segments in recording time.
    /// A silent window returns nothing without calling `stt`.
    pub fn transcribe(
        &mut self,
        window: &Window,
        stt: &dyn Transcriber,
        cancel: &AtomicBool,
    ) -> Result<Vec<TranscriptSegment>> {
        if rms(&window.samples) < SILENCE_RMS {
            return Ok(Vec::new());
        }
        let segments = stt.transcribe(&window.samples, cancel)?;
        Ok(self.dedup.keep(window.offset_ms(), segments))
    }
}
