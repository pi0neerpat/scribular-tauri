//! Turns one source's captured samples into `audio_chunks` rows while it
//! records, so a crash loses at most the last few seconds (KTD2).

use std::{
    sync::{mpsc, Arc},
    thread::{self, JoinHandle},
};

use super::{
    resample::{to_i16, Resampler},
    Format, Sink,
};
use crate::store::{self, AudioSource, Store};
use crate::stt::SAMPLE_RATE;

/// Five seconds of 16 kHz mono per row.
pub const CHUNK_SAMPLES: usize = 5 * SAMPLE_RATE as usize;

/// Samples per buffer handed from the capture callback to the writer.
const BLOCK_SAMPLES: usize = 4096;
/// Buffers allocated up front: over a second of 48 kHz stereo, however the
/// source sizes its callbacks.
const PREALLOCATED: usize = 128;
/// Buffers the writer may fall behind by, about 3 minutes of 48 kHz stereo,
/// before the callback drops samples rather than block.
const MAX_QUEUED: usize = 8192;

/// The writer's end of [`channel`]: filled buffers in, empty ones back.
pub struct Blocks {
    filled: mpsc::Receiver<Vec<f32>>,
    free: mpsc::SyncSender<Vec<f32>>,
}

/// A sink that hands each callback's samples to the writer thread. The
/// capture callback only copies into buffers the writer hands back, so it
/// allocates only while the writer is behind; resampling and SQLite stay
/// off it.
pub fn channel() -> (Sink, Blocks) {
    let (tx, filled) = mpsc::sync_channel(MAX_QUEUED);
    let (free, pool) = mpsc::sync_channel(MAX_QUEUED);
    for _ in 0..PREALLOCATED {
        let _ = free.try_send(Vec::with_capacity(BLOCK_SAMPLES));
    }
    let sink: Sink = Box::new(move |data: &[f32]| {
        for piece in data.chunks(BLOCK_SAMPLES) {
            let mut block = pool
                .try_recv()
                .unwrap_or_else(|_| Vec::with_capacity(BLOCK_SAMPLES));
            block.extend_from_slice(piece);
            let _ = tx.try_send(block);
        }
    });
    (sink, Blocks { filled, free })
}

/// Writes until every sender is gone (the capture was dropped), then flushes
/// the partial last chunk. Joins to the number of 16 kHz samples written.
/// Each resampled slice also goes to `live`, for streaming transcription;
/// if nothing receives it any more, recording carries on.
pub fn spawn(
    store: Arc<Store>,
    session_id: String,
    source: AudioSource,
    format: Format,
    blocks: Blocks,
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
            for mut data in blocks.filled {
                let before = pending.len();
                resampler.process(&data, &mut pending);
                data.clear();
                let _ = blocks.free.try_send(data);
                let _ = live.send(pending[before..].to_vec());
                while pending.len() >= CHUNK_SAMPLES {
                    write(&pending[..CHUNK_SAMPLES], &mut seq)?;
                    pending.drain(..CHUNK_SAMPLES);
                }
            }
            if !pending.is_empty() {
                write(&pending, &mut seq)?;
            }
            Ok(total)
        })
}
