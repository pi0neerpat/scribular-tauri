//! U9: the streaming window logic — overlapping windows that never drop
//! audio, per-source dedup of the overlap, and the silent-window skip.

use std::{
    collections::VecDeque,
    sync::{atomic::AtomicBool, Mutex},
};

use scribular_lib::stt::stream::{Dedup, Stream, Window, HOP, WINDOW};
use scribular_lib::stt::{Result, Transcriber, TranscriptSegment, SAMPLE_RATE};

const SECOND: usize = SAMPLE_RATE as usize;

fn segment(start_ms: i64, end_ms: i64, text: &str) -> TranscriptSegment {
    TranscriptSegment {
        start_ms,
        end_ms,
        text: text.into(),
    }
}

fn tone(seconds: usize) -> Vec<f32> {
    (0..seconds * SECOND)
        .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 16_000.0).sin() * 0.5)
        .collect()
}

/// Returns its scripted segments and records each call's sample count.
struct Scripted {
    segments: Vec<TranscriptSegment>,
    calls: Mutex<Vec<usize>>,
}

impl Transcriber for Scripted {
    fn transcribe(&self, samples: &[f32], _: &AtomicBool) -> Result<Vec<TranscriptSegment>> {
        self.calls.lock().unwrap().push(samples.len());
        Ok(self.segments.clone())
    }
}

/// Returns each call's segments in turn.
struct Sequence(Mutex<VecDeque<Vec<TranscriptSegment>>>);

impl Transcriber for Sequence {
    fn transcribe(&self, _: &[f32], _: &AtomicBool) -> Result<Vec<TranscriptSegment>> {
        Ok(self.0.lock().unwrap().pop_front().unwrap_or_default())
    }
}

fn spans(windows: &[Window]) -> Vec<(usize, usize)> {
    windows
        .iter()
        .map(|w| (w.start / SECOND, (w.start + w.samples.len()) / SECOND))
        .collect()
}

#[test]
fn windows_overlap_by_five_seconds_and_cover_all_audio() {
    assert_eq!((WINDOW, HOP), (20 * SECOND, 15 * SECOND));
    let mut stream = Stream::default();
    stream.push(&tone(14));
    assert_eq!(stream.queued(), 0, "the first window needs 15 s");
    stream.push(&tone(48));
    stream.finish();
    let mut windows = Vec::new();
    while let Some(w) = stream.next() {
        windows.push(w);
    }
    // 62 s: four full windows, then the 7 s tail with its 5 s overlap.
    assert_eq!(
        spans(&windows),
        [(0, 15), (10, 30), (25, 45), (40, 60), (55, 62)]
    );
}

#[test]
fn more_than_two_queued_windows_is_catching_up() {
    let mut stream = Stream::default();
    stream.push(&tone(45));
    assert_eq!(stream.queued(), 3);
    assert!(stream.catching_up());
    stream.next();
    assert!(!stream.catching_up());
}

#[test]
fn finishing_queues_the_tail() {
    let mut stream = Stream::default();
    stream.push(&tone(3));
    stream.finish();
    let tail = stream.next().unwrap();
    assert_eq!((tail.start, tail.samples.len()), (0, 3 * SECOND));
    assert_eq!(stream.next().map(|w| w.start), None);
}

#[test]
fn finishing_on_a_hop_queues_the_last_overlap_as_the_tail() {
    let mut stream = Stream::default();
    stream.push(&tone(30));
    stream.finish();
    let mut windows = Vec::new();
    while let Some(w) = stream.next() {
        windows.push(w);
    }
    assert_eq!(spans(&windows), [(0, 15), (10, 30), (25, 30)]);
    assert_eq!(
        windows.iter().map(|w| w.last).collect::<Vec<_>>(),
        [false, false, true]
    );
}

#[test]
fn nothing_is_queued_for_an_empty_recording() {
    let mut stream = Stream::default();
    stream.finish();
    assert_eq!(stream.queued(), 0);
}

#[test]
fn overlap_segments_at_or_before_the_last_saved_end_are_dropped() {
    let mut dedup = Dedup::default();
    let first = dedup.keep(
        0,
        vec![segment(0, 4_000, "one"), segment(11_000, 14_800, "two")],
    );
    assert_eq!(first.len(), 2);
    // The next window starts at 10 s; its segments are relative to that.
    let second = dedup.keep(
        10_000,
        vec![
            segment(1_000, 4_800, "two again"),
            segment(4_800, 6_000, "touching"),
            segment(4_900, 7_000, "three"),
        ],
    );
    assert_eq!(second, [segment(14_900, 17_000, "three")]);
}

#[test]
fn a_silent_window_is_skipped() {
    let stt = Scripted {
        segments: vec![segment(0, 1_000, "Thank you for watching")],
        calls: Mutex::default(),
    };
    let cancel = AtomicBool::new(false);
    let mut stream = Stream::default();
    stream.push(&vec![0.0005; 15 * SECOND]);
    let window = stream.next().unwrap();
    assert_eq!(stream.transcribe(&window, &stt, &cancel), Ok(Vec::new()));
    assert!(stt.calls.lock().unwrap().is_empty());

    stream.push(&tone(15));
    let window = stream.next().unwrap();
    assert_eq!(
        stream.transcribe(&window, &stt, &cancel),
        Ok(vec![segment(10_000, 11_000, "Thank you for watching")])
    );
    assert_eq!(*stt.calls.lock().unwrap(), [20 * SECOND]);
}

#[test]
fn a_segment_cut_off_at_a_window_end_comes_whole_from_the_next_window() {
    let stt = Sequence(Mutex::new(VecDeque::from([
        // 0–15 s: the last segment runs into the window's end.
        vec![
            segment(0, 11_000, "one"),
            segment(11_500, 15_000, "we should ship the"),
        ],
        // The 10–25 s tail: that speech whole, and a segment at its end.
        vec![
            segment(0, 1_000, "one"),
            segment(1_500, 8_000, "we should ship the release on Friday"),
            segment(13_000, 15_000, "bye"),
        ],
    ])));
    let cancel = AtomicBool::new(false);
    let mut stream = Stream::default();
    stream.push(&tone(25));
    stream.finish();
    let mut kept = Vec::new();
    while let Some(window) = stream.next() {
        kept.extend(stream.transcribe(&window, &stt, &cancel).unwrap());
    }
    assert_eq!(
        kept,
        [
            segment(0, 11_000, "one"),
            segment(11_500, 18_000, "we should ship the release on Friday"),
            segment(23_000, 25_000, "bye"),
        ]
    );
}

#[test]
fn a_segment_cut_off_at_the_end_of_a_recording_on_a_hop_is_kept_from_the_tail() {
    let stt = Sequence(Mutex::new(VecDeque::from([
        vec![segment(0, 11_000, "one")],
        // The 10–30 s window: a segment runs into its end and is left to the tail.
        vec![
            segment(1_500, 5_000, "two"),
            segment(16_000, 20_000, "we should ship the"),
        ],
        // The 25–30 s tail holds that speech whole.
        vec![segment(1_000, 5_000, "we should ship the release")],
    ])));
    let cancel = AtomicBool::new(false);
    let mut stream = Stream::default();
    stream.push(&tone(30));
    stream.finish();
    let mut kept = Vec::new();
    while let Some(window) = stream.next() {
        kept.extend(stream.transcribe(&window, &stt, &cancel).unwrap());
    }
    assert_eq!(
        kept,
        [
            segment(0, 11_000, "one"),
            segment(11_500, 15_000, "two"),
            segment(26_000, 30_000, "we should ship the release"),
        ]
    );
}
