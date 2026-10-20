//! Speech-to-text behind the [`Transcriber`] trait (KTD9), so jobs can be
//! tested with a fake engine. Engines take 16 kHz mono samples from memory;
//! [`read_wav`] exists for fixtures and the `transcribe_file` dev command.

#[cfg(not(feature = "fake-engines"))]
pub mod whisper;

use std::{
    fmt, io,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use serde::Serialize;
use ts_rs::TS;

/// The sample rate every engine expects.
pub const SAMPLE_RATE: u32 = 16_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SttError {
    /// The cancel token was set; no output is kept.
    Cancelled,
    /// The model isn't downloaded and verified yet.
    NotReady,
    Engine(String),
}

impl fmt::Display for SttError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::NotReady => f.write_str("The speech model isn't downloaded yet."),
            Self::Engine(e) => write!(f, "speech model error: {e}"),
        }
    }
}

impl std::error::Error for SttError {}

pub type Result<T> = std::result::Result<T, SttError>;

/// One timed piece of a transcript, relative to the first sample.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    #[ts(type = "number")]
    pub start_ms: i64,
    #[ts(type = "number")]
    pub end_ms: i64,
    pub text: String,
}

pub trait Transcriber: Send + Sync {
    /// Transcribes 16 kHz mono `samples` into segments in time order.
    /// Returns `Cancelled` once `cancel` is set, without the partial output.
    fn transcribe(&self, samples: &[f32], cancel: &AtomicBool) -> Result<Vec<TranscriptSegment>>;
}

/// Loads the transcriber on demand, so the model only takes memory while
/// audio needs it (KTD5).
pub trait TranscriberLoader: Send + Sync {
    fn load(&self) -> Result<Arc<dyn Transcriber>>;
}

/// Reads a 16-bit PCM, 16 kHz mono WAV file as samples in `-1.0..=1.0`.
pub fn read_wav(path: &Path) -> io::Result<Vec<f32>> {
    let invalid = |msg: &str| io::Error::new(io::ErrorKind::InvalidData, msg.to_string());
    let bytes = std::fs::read(path)?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(invalid("not a RIFF/WAVE file"));
    }
    let mut format_ok = false;
    let mut rest = &bytes[12..];
    while rest.len() >= 8 {
        let size = u32::from_le_bytes(rest[4..8].try_into().unwrap()) as usize;
        let body = rest
            .get(8..8 + size)
            .ok_or_else(|| invalid("truncated chunk"))?;
        match &rest[0..4] {
            b"fmt " if body.len() >= 16 => {
                let u16_at = |i: usize| u16::from_le_bytes([body[i], body[i + 1]]);
                let rate = u32::from_le_bytes(body[4..8].try_into().unwrap());
                format_ok = u16_at(0) == 1 && u16_at(2) == 1 && rate == SAMPLE_RATE && u16_at(14) == 16;
            }
            b"data" => {
                if !format_ok {
                    return Err(invalid("expected 16-bit PCM, 16 kHz mono"));
                }
                return Ok(body
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&b| f32::from(i16::from_le_bytes(b)) / 32768.0)
                    .collect());
            }
            _ => {}
        }
        // Chunks are padded to an even length.
        rest = rest.get(8 + size + size % 2..).unwrap_or_default();
    }
    Err(invalid("no data chunk"))
}
