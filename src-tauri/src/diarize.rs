use std::sync::atomic::{AtomicBool, Ordering};

use crate::{
    models::ModelManager,
    store::{AudioSource, InputMode},
    text::{build_raw_transcript, default_speaker_map, Segment, SpeakerMap},
};

#[cfg(not(feature = "fake-engines"))]
mod sherpa;

pub const SPEAKER_THRESHOLD: f32 = 0.7;
const MIN_SEGMENT_MS: i64 = 500;

pub trait SpeakerEmbedder: Send + Sync {
    fn embed(&self, audio: &[f32]) -> Result<Vec<f32>, String>;
}

pub trait SpeakerEmbedderLoader: Send + Sync {
    fn load(&self) -> Result<Box<dyn SpeakerEmbedder>, String>;
}

#[cfg(not(feature = "fake-engines"))]
pub use sherpa::SherpaLoader;

pub fn default_loader(_models: &ModelManager) -> Box<dyn SpeakerEmbedderLoader> {
    #[cfg(not(feature = "fake-engines"))]
    {
        Box::new(SherpaLoader::new(
            _models.path(crate::models::SPEAKER_CAMPPLUS),
        ))
    }
    #[cfg(feature = "fake-engines")]
    {
        Box::new(FakeLoader)
    }
}

#[cfg(feature = "fake-engines")]
struct FakeLoader;

#[cfg(feature = "fake-engines")]
impl SpeakerEmbedderLoader for FakeLoader {
    fn load(&self) -> Result<Box<dyn SpeakerEmbedder>, String> {
        Ok(Box::new(FakeEmbedder))
    }
}

#[cfg(feature = "fake-engines")]
struct FakeEmbedder;

#[cfg(feature = "fake-engines")]
impl SpeakerEmbedder for FakeEmbedder {
    fn embed(&self, audio: &[f32]) -> Result<Vec<f32>, String> {
        Ok(if audio.first().copied().unwrap_or_default() < 0.0 {
            vec![0.0, 1.0]
        } else {
            vec![1.0, 0.0]
        })
    }
}

pub fn diarize(
    mode: InputMode,
    source_segments: &[(AudioSource, Vec<Segment>)],
    source_audio: &[(AudioSource, Vec<i16>)],
    max_speakers: Option<u8>,
    loader: &dyn SpeakerEmbedderLoader,
    cancel: &AtomicBool,
) -> Result<(String, SpeakerMap), String> {
    let embedder = loader.load()?;
    let mut turns = Vec::new();
    let mut offset = 0;
    for (source, segments) in source_segments {
        let samples = source_audio
            .iter()
            .find(|(s, _)| s == source)
            .map_or(&[][..], |(_, a)| a.as_slice());
        let cap = if mode == InputMode::MicAndSystem {
            max_speakers.map(|n| n.saturating_sub(1)).or(Some(4))
        } else {
            max_speakers
        };
        let (labeled, count) = label_segments(segments, samples, cap, offset, &*embedder, cancel)?;
        turns.extend(labeled);
        if mode == InputMode::MicAndSystem && *source == AudioSource::System {
            offset = count;
        }
    }
    turns.sort_by_key(|(_, s)| s.start_ms);
    let ids: Vec<String> = turns.iter().map(|(id, _)| id.clone()).collect();
    let map = default_speaker_map(ids);
    Ok((build_raw_transcript(&turns), map))
}

fn label_segments(
    segments: &[Segment],
    samples: &[i16],
    max_speakers: Option<u8>,
    offset: usize,
    embedder: &dyn SpeakerEmbedder,
    cancel: &AtomicBool,
) -> Result<(Vec<(String, Segment)>, usize), String> {
    let mut speakers: Vec<Vec<f32>> = Vec::new();
    let mut labeled = Vec::new();
    let mut previous = None;
    for segment in segments {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let speaker = if segment.end_ms - segment.start_ms < MIN_SEGMENT_MS {
            previous.unwrap_or(offset)
        } else {
            let start = usize::try_from(segment.start_ms.max(0))
                .unwrap_or(usize::MAX)
                .saturating_mul(16);
            let end = usize::try_from(segment.end_ms.max(0))
                .unwrap_or(usize::MAX)
                .saturating_mul(16)
                .min(samples.len());
            let audio: Vec<f32> = samples
                .get(start.min(end)..end)
                .unwrap_or(&[])
                .iter()
                .map(|s| f32::from(*s) / 32768.0)
                .collect();
            let embedding = embedder.embed(&audio)?;
            let best = speakers
                .iter()
                .enumerate()
                .map(|(i, known)| (i, cosine(&embedding, known)))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            let local = match best {
                Some((i, score)) if score >= SPEAKER_THRESHOLD => i,
                Some((i, _))
                    if max_speakers.is_some_and(|cap| speakers.len() >= usize::from(cap)) =>
                {
                    i
                }
                _ => {
                    speakers.push(embedding);
                    speakers.len() - 1
                }
            };
            local + offset
        };
        previous = Some(speaker);
        labeled.push((format!("SPEAKER_{speaker}"), segment.clone()));
    }
    let count = labeled
        .iter()
        .map(|(id, _)| {
            id.strip_prefix("SPEAKER_")
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(offset)
                .saturating_sub(offset)
                + 1
        })
        .max()
        .unwrap_or(0);
    Ok((labeled, count))
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return -1.0;
    }
    let (mut dot, mut an, mut bn) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().zip(b) {
        dot += x * y;
        an += x * x;
        bn += y * y;
    }
    if an == 0.0 || bn == 0.0 {
        -1.0
    } else {
        dot / (an.sqrt() * bn.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::VecDeque, sync::Mutex};

    struct Scripted(Mutex<VecDeque<Vec<f32>>>);
    impl SpeakerEmbedder for Scripted {
        fn embed(&self, _: &[f32]) -> Result<Vec<f32>, String> {
            Ok(self.0.lock().unwrap().pop_front().unwrap())
        }
    }
    struct Loader(Mutex<Option<VecDeque<Vec<f32>>>>);
    impl SpeakerEmbedderLoader for Loader {
        fn load(&self) -> Result<Box<dyn SpeakerEmbedder>, String> {
            Ok(Box::new(Scripted(Mutex::new(
                self.0.lock().unwrap().take().unwrap(),
            ))))
        }
    }
    fn seg(start_ms: i64, end_ms: i64, text: &str) -> Segment {
        Segment {
            start_ms,
            end_ms,
            text: Some(text.into()),
        }
    }

    #[test]
    fn greedy_labels_caps_and_short_segments() {
        let segments = vec![
            seg(0, 600, "a"),
            seg(600, 1200, "b"),
            seg(1200, 1800, "a2"),
            seg(1800, 2400, "c"),
        ];
        let embedder = Scripted(Mutex::new(VecDeque::from([
            vec![1., 0.],
            vec![0., 1.],
            vec![1., 0.],
            vec![0., -1.],
        ])));
        let (turns, count) = label_segments(
            &segments,
            &[1; 38_400],
            Some(2),
            0,
            &embedder,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            turns.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["SPEAKER_0", "SPEAKER_1", "SPEAKER_0", "SPEAKER_0"]
        );
        let raw = build_raw_transcript(&turns);
        assert_eq!(raw, "SPEAKER_0: a\nSPEAKER_1: b\nSPEAKER_0: a2 c");
    }

    #[test]
    fn cancelled_during_segments_returns_no_data() {
        let cancel = AtomicBool::new(true);
        assert!(label_segments(
            &[seg(0, 600, "x")],
            &[0; 9600],
            None,
            0,
            &Scripted(Mutex::new(VecDeque::new())),
            &cancel
        )
        .is_err());
    }

    #[test]
    fn leading_and_later_short_segments_follow_the_previous_label() {
        let segments = vec![
            seg(0, 100, "short"),
            seg(100, 700, "voice"),
            seg(700, 800, "short"),
        ];
        let embedder = Scripted(Mutex::new(VecDeque::from([vec![1., 0.]])));
        let (turns, count) = label_segments(
            &segments,
            &[0; 12_800],
            None,
            0,
            &embedder,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(count, 1);
        assert!(turns.iter().all(|(id, _)| id == "SPEAKER_0"));
    }

    #[test]
    fn uncapped_mode_keeps_four_distinct_embeddings() {
        let segments = (0..4)
            .map(|i| seg(i * 600, (i + 1) * 600, "voice"))
            .collect::<Vec<_>>();
        let embedder = Scripted(Mutex::new(VecDeque::from([
            vec![1., 0.],
            vec![0., 1.],
            vec![-1., 0.],
            vec![0., -1.],
        ])));
        let (_, count) = label_segments(
            &segments,
            &[0; 38_400],
            None,
            0,
            &embedder,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn two_sources_offset_mic_ids_and_merge_by_start_time() {
        let system = vec![seg(500, 1100, "remote")];
        let mic = vec![seg(0, 600, "local")];
        let segments = vec![(AudioSource::System, system), (AudioSource::Mic, mic)];
        let audio = vec![
            (AudioSource::System, vec![0; 17_600]),
            (AudioSource::Mic, vec![0; 9_600]),
        ];
        let loader = Loader(Mutex::new(Some(VecDeque::from([
            vec![1., 0.],
            vec![0., 1.],
        ]))));
        let (raw, map) = diarize(
            InputMode::MicAndSystem,
            &segments,
            &audio,
            Some(3),
            &loader,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(raw, "SPEAKER_1: local\nSPEAKER_0: remote");
        assert_eq!(map["SPEAKER_0"], "Speaker 1");
        assert_eq!(map["SPEAKER_1"], "Speaker 2");
    }
}
