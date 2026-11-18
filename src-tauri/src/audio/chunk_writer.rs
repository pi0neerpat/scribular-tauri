//! Turns one source's captured samples into `audio_chunks` rows while it
//! records, so a crash loses at most the last few seconds (KTD2).

use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Arc,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use super::{
    resample::{to_i16, Resampler},
    ring::{ring, Consumer},
    Format, Sink,
};
use crate::store::{self, AudioSource, Store};
use crate::stt::SAMPLE_RATE;

/// Five seconds of 16 kHz mono per row.
pub const CHUNK_SAMPLES: usize = 5 * SAMPLE_RATE as usize;

/// 8 MB per source: the writer may stall (SQLite, a slow disk) for about
/// 22 seconds of 48 kHz stereo before audio is dropped.
pub const RING_SAMPLES: usize = 1 << 21;

const POLL: Duration = Duration::from_millis(5);

/// A sink that only copies callback samples into a preallocated bounded ring;
/// resampling and SQLite stay on the writer thread.
pub fn channel() -> (Sink, Consumer) {
    channel_with_capacity(RING_SAMPLES)
}

/// Like `channel`, with a sample capacity rounded up to a power of two.
pub fn channel_with_capacity(capacity: usize) -> (Sink, Consumer) {
    ring(capacity)
}

/// The next position and the partial chunk already committed at the end of a span.
#[derive(Clone, Default)]
pub struct Timeline {
    pub samples: usize,
    tail: Vec<i16>,
}

pub struct Input {
    pub format: Format,
    pub consumer: Consumer,
    pub timeline: Timeline,
    pub level: Arc<AtomicU32>,
    /// For app audio, maps the first batch onto the microphone's sample clock.
    pub align: Option<Box<dyn FnOnce() -> usize + Send>>,
}

/// Flushes the final partial chunk when the capture closes. A later span can
/// extend it, keeping the store's fixed-size chunk addressing intact.
pub fn spawn(
    store: Arc<Store>,
    session_id: String,
    source: AudioSource,
    input: Input,
    live: mpsc::Sender<(usize, Vec<f32>)>,
) -> std::io::Result<JoinHandle<store::Result<Timeline>>> {
    thread::Builder::new()
        .name(format!("chunks-{}", source.as_str()))
        .spawn(move || {
            let Input {
                format,
                mut consumer,
                mut timeline,
                level,
                mut align,
            } = input;
            let mut resampler = Resampler::new(format);
            let mut seq = (timeline.samples / CHUNK_SAMPLES) as u32;
            let mut saved = timeline.tail.len();
            let mut write = |pcm: &[i16], saved: &mut usize| -> store::Result<()> {
                if *saved == 0 {
                    store.append_audio_chunk(&session_id, source, seq, pcm)?;
                } else {
                    store.extend_audio_chunk(&session_id, source, seq, pcm)?;
                }
                seq += 1;
                *saved = 0;
                Ok(())
            };
            let mut data = Vec::new();
            let mut pcm = Vec::new();
            let silence = [0.0; 4096];
            loop {
                data.clear();
                let drained = consumer.drain(&mut data);
                if !data.is_empty() || drained.dropped > 0 {
                    if let Some(align) = align.take() {
                        let offset = align();
                        while offset > timeline.samples {
                            let n = (offset - timeline.samples)
                                .min(CHUNK_SAMPLES - timeline.tail.len());
                            timeline.tail.resize(timeline.tail.len() + n, 0);
                            timeline.samples += n;
                            if timeline.tail.len() == CHUNK_SAMPLES {
                                write(&timeline.tail, &mut saved)?;
                                timeline.tail.clear();
                            }
                        }
                    }
                    let mut process = |samples: &[f32], meter: bool| -> store::Result<()> {
                        pcm.clear();
                        resampler.process(samples, &mut pcm);
                        if pcm.is_empty() {
                            return Ok(());
                        }
                        if meter {
                            let rms = crate::stt::stream::rms(&pcm);
                            if rms.is_finite() {
                                level.fetch_max(rms.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
                            }
                        }
                        let start = timeline.samples;
                        while timeline.samples - start < pcm.len() {
                            let from = timeline.samples - start;
                            let n = (pcm.len() - from).min(CHUNK_SAMPLES - timeline.tail.len());
                            timeline
                                .tail
                                .extend(pcm[from..from + n].iter().copied().map(to_i16));
                            timeline.samples += n;
                            if timeline.tail.len() == CHUNK_SAMPLES {
                                write(&timeline.tail, &mut saved)?;
                                timeline.tail.clear();
                            }
                        }
                        let _ = live.send((start, std::mem::take(&mut pcm)));
                        Ok(())
                    };
                    if !data.is_empty() {
                        process(&data, true)?;
                    }
                    let mut remaining = drained.dropped;
                    if remaining > 0 {
                        crate::diag::log::log(
                            "WARN",
                            "audio",
                            &format!("{} samples of {} dropped", remaining, source.as_str()),
                        );
                    }
                    while remaining > 0 {
                        let n = remaining.min(silence.len());
                        process(&silence[..n], false)?;
                        remaining -= n;
                    }
                }
                if drained.closed {
                    break;
                }
                if data.is_empty() && drained.dropped == 0 {
                    thread::sleep(POLL);
                }
            }
            if timeline.tail.len() > saved {
                write(&timeline.tail, &mut saved)?;
            }
            Ok(timeline)
        })
}
