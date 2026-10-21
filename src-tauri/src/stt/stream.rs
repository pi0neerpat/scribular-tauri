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
/// A segment ending this close to a window's end was likely cut off there.
const EDGE_MS: i64 = 1_000;

fn ms(samples: usize) -> i64 {
    (samples as u64 * 1000 / SAMPLE_RATE as u64) as i64
}

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
    /// The tail queued by [`Stream::finish`]; no window follows it.
    pub last: bool,
}

impl Window {
    pub fn offset_ms(&self) -> i64 {
        ms(self.start)
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
    pub fn keep(
        &mut self,
        offset_ms: i64,
        segments: Vec<TranscriptSegment>,
    ) -> Vec<TranscriptSegment> {
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

    fn cut(&mut self, start: usize, end: usize, last: bool) {
        let samples = self.audio[start - self.base..end - self.base].to_vec();
        self.queue.push_back(Window {
            start,
            samples,
            last,
        });
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
            self.cut(end.saturating_sub(WINDOW), end, false);
        }
    }

    /// Queues the audio after the last window, with its overlap. A recording
    /// that ends on a hop still gets its last `OVERLAP` as a tail, since
    /// the last window left its edge segments to the window after it.
    pub fn finish(&mut self) {
        let end = self.received();
        if end > 0 {
            self.cut(self.covered.saturating_sub(OVERLAP), end, true);
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
    /// A silent window returns nothing without calling `stt`. A segment cut
    /// off at the end of a window is left to the next one, whose overlap
    /// holds it whole.
    pub fn transcribe(
        &mut self,
        window: &Window,
        stt: &dyn Transcriber,
        cancel: &AtomicBool,
    ) -> Result<Vec<TranscriptSegment>> {
        if rms(&window.samples) < SILENCE_RMS {
            return Ok(Vec::new());
        }
        let mut segments = stt.transcribe(&window.samples, cancel)?;
        if !window.last {
            let end = ms(window.samples.len());
            let overlap = end - ms(OVERLAP);
            while segments
                .last()
                .is_some_and(|s| s.end_ms > end - EDGE_MS && s.start_ms >= overlap)
            {
                segments.pop();
            }
        }
        Ok(self.dedup.keep(window.offset_ms(), segments))
    }
}
