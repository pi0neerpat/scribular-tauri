//! Recording (U8): the microphone through cpal, system audio through the
//! bundled AudioCapCLI (KTD11). Each source is resampled to 16 kHz mono and
//! written to `audio_chunks` while it records (KTD2).

pub mod chunk_writer;
pub mod mic;
pub mod resample;
pub mod system;

use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use mic::{CpalMicrophone, Microphone};
pub use system::{AudioApp, AudioCapCli, SystemAudio};

#[derive(Debug)]
pub enum AudioError {
    NoInputDevice,
    /// macOS refused access (TCC); the pane says where to grant it.
    PermissionDenied(Privacy),
    /// No running app plays audio under this name.
    AppNotFound(String),
    Device(String),
    Io(std::io::Error),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoInputDevice => f.write_str("no microphone found"),
            Self::PermissionDenied(Privacy::Microphone) => {
                f.write_str("microphone permission denied")
            }
            Self::PermissionDenied(Privacy::SystemAudio) => {
                f.write_str("system audio recording permission denied")
            }
            Self::AppNotFound(name) => write!(f, "{name} is not playing audio"),
            Self::Device(e) => write!(f, "audio device error: {e}"),
            Self::Io(e) => write!(f, "audio capture error: {e}"),
        }
    }
}

impl std::error::Error for AudioError {}

impl From<std::io::Error> for AudioError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// The System Settings privacy pane that grants a source's access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Privacy {
    Microphone,
    SystemAudio,
}

impl Privacy {
    /// The `x-apple.systempreferences:` URL of the pane.
    pub fn settings_url(self) -> &'static str {
        match self {
            Self::Microphone => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            Self::SystemAudio => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
        }
    }
}

pub type Result<T> = std::result::Result<T, AudioError>;

/// A capture's native format; samples arrive interleaved as f32.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub sample_rate: u32,
    pub channels: u16,
}

/// Receives interleaved samples, on whichever thread the source delivers.
pub type Sink = Box<dyn FnMut(&[f32]) + Send>;

/// A running capture. Dropping it stops the source and drops its sink, and
/// returns only once the sink will not be called again.
pub trait Capture: Send {
    fn format(&self) -> Format;
}
