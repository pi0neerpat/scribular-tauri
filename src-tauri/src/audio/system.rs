//! System audio through the AudioCapCLI sidecar (KTD11), the same binary the
//! Electron app ships. It is spawned with `std::process::Command` from next
//! to the app executable, so the webview gets no shell permission.

use std::{
    io::Read,
    path::PathBuf,
    process::{Child, ChildStderr, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use serde::Serialize;
use ts_rs::TS;

use super::{AudioError, Capture, Format, Privacy, Result, Sink};

/// An app that is currently playing audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AudioApp {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// The app icon as a base64 PNG.
    pub icon: Option<String>,
}

pub trait SystemAudio: Send + Sync {
    fn list_apps(&self) -> Result<Vec<AudioApp>>;
    /// Captures `app`, which must appear in `list_apps`.
    fn open(&self, app: &str, sink: Sink) -> Result<Box<dyn Capture>>;
}

pub struct AudioCapCli {
    path: PathBuf,
}

/// How long `open` waits for the CLI to fail on a TCC refusal before
/// treating the capture as started.
const STARTUP_GRACE: Duration = Duration::from_millis(500);

impl AudioCapCli {
    /// The sidecar Tauri's `externalBin` places next to the app executable.
    pub fn bundled() -> std::io::Result<Self> {
        let exe = std::env::current_exe()?;
        let dir = exe
            .parent()
            .ok_or_else(|| std::io::Error::other("executable has no parent directory"))?;
        Ok(Self {
            path: dir.join("AudioCapCLI"),
        })
    }
}

impl SystemAudio for AudioCapCli {
    fn list_apps(&self) -> Result<Vec<AudioApp>> {
        let out = Command::new(&self.path)
            .arg("--list-sources")
            .stdin(Stdio::null())
            .output()?;
        let stderr = String::from_utf8_lossy(&out.stderr);
        if is_permission_denied(&stderr) {
            return Err(AudioError::PermissionDenied(Privacy::SystemAudio));
        }
        if !out.status.success() {
            return Err(AudioError::Device(format!(
                "AudioCapCLI --list-sources failed: {}",
                stderr.trim()
            )));
        }
        Ok(parse_sources(&String::from_utf8_lossy(&out.stdout)))
    }

    fn open(&self, app: &str, mut sink: Sink) -> Result<Box<dyn Capture>> {
        let found = self
            .list_apps()?
            .into_iter()
            .find(|a| a.name == app)
            .ok_or_else(|| AudioError::AppNotFound(app.to_owned()))?;
        let format = Format {
            sample_rate: found.sample_rate,
            channels: found.channels,
        };
        let mut child = Command::new(&self.path)
            .args(["--source", app])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");

        let flowing = Arc::new(AtomicBool::new(false));
        let reader = {
            let flowing = Arc::clone(&flowing);
            thread::Builder::new()
                .name("system-audio".into())
                .spawn(move || {
                    let mut buf = vec![0u8; 16 * 1024];
                    // Bytes of a sample split across two reads.
                    let mut carry = Vec::with_capacity(4);
                    let mut samples = Vec::new();
                    while let Ok(n) = stdout.read(&mut buf) {
                        if n == 0 {
                            break;
                        }
                        flowing.store(true, Ordering::Relaxed);
                        carry.extend_from_slice(&buf[..n]);
                        let (whole, _) = carry.as_chunks::<4>();
                        let used = whole.len() * 4;
                        samples.clear();
                        samples.extend(whole.iter().map(|b| f32::from_le_bytes(*b)));
                        carry.drain(..used);
                        if !samples.is_empty() {
                            sink(&samples);
                        }
                    }
                })?
        };
        let errors = Arc::new(Mutex::new(String::new()));
        let collector = spawn_collector(stderr, Arc::clone(&errors))?;

        let mut capture = CliCapture {
            format,
            child,
            threads: vec![reader, collector],
        };
        let deadline = Instant::now() + STARTUP_GRACE;
        while Instant::now() < deadline && !flowing.load(Ordering::Relaxed) {
            if capture.child.try_wait()?.is_some() {
                drop(capture);
                let errors = errors.lock().unwrap_or_else(|e| e.into_inner());
                return Err(if is_permission_denied(&errors) {
                    AudioError::PermissionDenied(Privacy::SystemAudio)
                } else {
                    AudioError::Device(format!("AudioCapCLI exited: {}", errors.trim()))
                });
            }
            thread::sleep(Duration::from_millis(20));
        }
        Ok(Box::new(capture))
    }
}

fn spawn_collector(
    mut stderr: ChildStderr,
    into: Arc<Mutex<String>>,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("system-audio-stderr".into())
        .spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            *into.lock().unwrap_or_else(|e| e.into_inner()) = text;
        })
}

struct CliCapture {
    format: Format,
    child: Child,
    threads: Vec<JoinHandle<()>>,
}

impl Capture for CliCapture {
    fn format(&self) -> Format {
        self.format
    }
}

impl Drop for CliCapture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

fn is_permission_denied(stderr: &str) -> bool {
    stderr.to_lowercase().contains("permission denied")
}

/// Parses `--list-sources` lines: `name|<rate> Hz, <n> ch|<base64 png>`.
/// A format the line doesn't state falls back to 44.1 kHz stereo.
fn parse_sources(stdout: &str) -> Vec<AudioApp> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '|');
            let name = parts.next()?.trim();
            if name.is_empty() {
                return None;
            }
            let format = parts.next().unwrap_or("");
            let icon = parts
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned);
            let number_before = |unit: &str| {
                format.split(',').find_map(|field| {
                    field
                        .trim()
                        .strip_suffix(unit)
                        .and_then(|n| n.trim().parse::<f64>().ok())
                })
            };
            Some(AudioApp {
                name: name.to_owned(),
                sample_rate: number_before("Hz").map_or(44_100, |r| r as u32),
                channels: number_before("ch").map_or(2, |c| c as u16),
                icon,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_source_lines() {
        let out = "Zoom|48000 Hz, 2 ch|iVBORw0K\nSafari|44100.0 Hz, 1 ch|\n\nMusic\n";
        let apps = parse_sources(out);
        assert_eq!(
            apps,
            vec![
                AudioApp {
                    name: "Zoom".into(),
                    sample_rate: 48_000,
                    channels: 2,
                    icon: Some("iVBORw0K".into()),
                },
                AudioApp {
                    name: "Safari".into(),
                    sample_rate: 44_100,
                    channels: 1,
                    icon: None,
                },
                AudioApp {
                    name: "Music".into(),
                    sample_rate: 44_100,
                    channels: 2,
                    icon: None,
                },
            ]
        );
    }

    #[test]
    fn detects_permission_denied() {
        assert!(is_permission_denied("Audio recording permission denied.\n"));
        assert!(!is_permission_denied("no such source"));
    }
}
