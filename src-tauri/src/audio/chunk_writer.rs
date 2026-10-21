//! Turns one source's captured samples into `audio_chunks` rows while it
//! records, so a crash loses at most the last few seconds (KTD2).

use std::{
    sync::{mpsc, Arc},
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

/// Writes until the producer is gone (the capture was dropped), then flushes
/// the partial last chunk. Joins to the number of 16 kHz samples written.
/// Each resampled slice also goes to `live`, for streaming transcription;
/// if nothing receives it any more, recording carries on.
pub fn spawn(
    store: Arc<Store>,
    session_id: String,
    source: AudioSource,
    format: Format,
    mut consumer: Consumer,
    live: mpsc::Sender<Vec<f32>>,
) -> std::io::Result<JoinHandle<store::Result<u64>>> {
    thread::Builder::new()
        .name(format!("chunks-{}", source.as_str()))
        .spawn(move || {
            let mut resampler = Resampler::new(format);
            let mut pending: Vec<f32> = Vec::with_capacity(CHUNK_SAMPLES * 2);
            let mut seq = 0u32;
            let mut total = 0u64;
            let mut write = |samples: &[f32], seq: &mut u32| -> store::Result<()> {
                let pcm: Vec<i16> = samples.iter().copied().map(to_i16).collect();
                store.append_audio_chunk(&session_id, source, *seq, &pcm)?;
                *seq += 1;
                total += pcm.len() as u64;
                Ok(())
            };
            {
                let mut process = |samples: &[f32]| -> store::Result<()> {
                    let before = pending.len();
                    resampler.process(samples, &mut pending);
                    let _ = live.send(pending[before..].to_vec());
                    while pending.len() >= CHUNK_SAMPLES {
                        write(&pending[..CHUNK_SAMPLES], &mut seq)?;
                        pending.drain(..CHUNK_SAMPLES);
                    }
                    Ok(())
                };
                let mut data = Vec::new();
                let silence = [0.0; 4096];
                loop {
                    data.clear();
                    let drained = consumer.drain(&mut data);
                    if !data.is_empty() {
                        process(&data)?;
                    }
                    if drained.dropped > 0 {
                        eprintln!(
                            "{} samples of {} dropped for session {}",
                            drained.dropped,
                            source.as_str(),
                            session_id
                        );
                        let mut remaining = drained.dropped;
                        while remaining > 0 {
                            let n = remaining.min(silence.len());
                            process(&silence[..n])?;
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
            }
            if !pending.is_empty() {
                write(&pending, &mut seq)?;
            }
            Ok(total)
        })
}
