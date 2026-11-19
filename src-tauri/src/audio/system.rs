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
    fn open(&self, app: &str, sink: Sink, cancel: &AtomicBool) -> Result<Box<dyn Capture>>;
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

    fn sources(&self, cancel: &AtomicBool) -> Result<Vec<AudioApp>> {
        check_cancel(cancel)?;
        let mut child = Command::new(&self.path)
            .arg("--list-sources")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| {
                AudioError::Device(
                    "Couldn't launch AudioCapCLI. Reinstall Scribular and try again.".into(),
                )
            })?;
        let mut stdout = child.stdout.take().expect("stdout is piped");
        let mut stderr = child.stderr.take().expect("stderr is piped");
        let (status, stdout, stderr) = thread::scope(|scope| {
            let output = scope.spawn(move || {
                let mut text = Vec::new();
                stdout.read_to_end(&mut text).map(|_| text)
            });
            let errors = scope.spawn(move || {
                let mut text = Vec::new();
                stderr.read_to_end(&mut text).map(|_| text)
            });
            let deadline = Instant::now() + Duration::from_secs(3);
            let status = (|| loop {
                check_cancel(cancel)?;
                if let Some(status) = child.try_wait()? {
                    break Ok(status);
                }
                if Instant::now() >= deadline {
                    break Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "Listing app audio sources timed out. Refresh the app list to try again.",
                    ));
                }
                thread::sleep(Duration::from_millis(20));
            })();
            if status.is_err() {
                let _ = child.kill();
                let _ = child.wait();
            }
            Ok::<_, std::io::Error>((
                status?,
                output.join().expect("source stdout reader")?,
                errors.join().expect("source stderr reader")?,
            ))
        })?;
        let stderr = String::from_utf8_lossy(&stderr);
        if !status.success() {
            if let Some(message) = source_list_failure(&stderr) {
                return Err(AudioError::Device(message.into()));
            }
            if is_permission_denied(&stderr) {
                return Err(AudioError::PermissionDenied(Privacy::SystemAudio));
            }
            // AudioCapCLI exits silently when macOS denies System Audio permission.
            if stderr.trim().is_empty() {
                return Err(AudioError::PermissionDenied(Privacy::SystemAudio));
            }
            return Err(AudioError::Device(
                "Couldn't list app audio sources. Refresh the app list or restart Scribular."
                    .into(),
            ));
        }
        Ok(parse_sources(&String::from_utf8_lossy(&stdout)))
    }
}

fn source_list_failure(stderr: &str) -> Option<&'static str> {
    for line in stderr.lines().map(str::trim) {
        if line.starts_with("Error reading process list")
            || line.starts_with("Error reading data")
            || line.starts_with("Error reading array")
            || line.starts_with("Failed to read format")
        {
            return Some("Couldn't read macOS audio sources. Check your sound output and refresh the app list.");
        }
        if line.starts_with("Usage:") || line.starts_with("dyld[") {
            return Some("The installed AudioCapCLI couldn't list sources. Reinstall Scribular and try again.");
        }
    }
    None
}

fn check_cancel(cancel: &AtomicBool) -> std::io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(std::io::ErrorKind::Interrupted.into());
    }
    Ok(())
}

impl SystemAudio for AudioCapCli {
    fn list_apps(&self) -> Result<Vec<AudioApp>> {
        self.sources(&AtomicBool::new(false))
    }

    fn open(&self, app: &str, mut sink: Sink, cancel: &AtomicBool) -> Result<Box<dyn Capture>> {
        let found = self
            .sources(cancel)?
            .into_iter()
            .find(|a| a.name == app)
            .ok_or_else(|| AudioError::AppNotFound(app.to_owned()))?;
        let format = Format {
            sample_rate: found.sample_rate,
            channels: found.channels,
        };
        check_cancel(cancel)?;
        let child = Command::new(&self.path)
            .args(["--source", app])
            // Match Electron's spawn default: AudioCapCLI delivers no PCM
            // with null/closed stdin. Child owns this unused pipe until drop.
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // From here, any early return drops `capture`, which kills the CLI.
        let mut capture = CliCapture {
            format,
            child,
            threads: Vec::new(),
        };
        let mut stdout = capture.child.stdout.take().expect("stdout is piped");
        let stderr = capture.child.stderr.take().expect("stderr is piped");

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
                            sink.push(&samples);
                        }
                    }
                })?
        };
        capture.threads.push(reader);
        let errors = Arc::new(Mutex::new(String::new()));
        capture
            .threads
            .push(spawn_collector(stderr, Arc::clone(&errors))?);

        let deadline = Instant::now() + STARTUP_GRACE;
        while Instant::now() < deadline && !flowing.load(Ordering::Relaxed) {
            check_cancel(cancel)?;
            if capture.child.try_wait()?.is_some() {
                drop(capture);
                let errors = errors.lock().unwrap_or_else(|e| e.into_inner());
                return Err(if is_permission_denied(&errors) {
                    AudioError::PermissionDenied(Privacy::SystemAudio)
                } else {
                    AudioError::Device("app audio capture ended during startup".into())
                });
            }
            thread::sleep(Duration::from_millis(20));
        }
        check_cancel(cancel)?;
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
/// A format the line doesn't state falls back to 44.1 kHz stereo. Apps are
/// captured by name, so a repeated name (a browser's helper processes) is
/// listed once.
fn parse_sources(stdout: &str) -> Vec<AudioApp> {
    let mut seen = std::collections::HashSet::new();
    stdout
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '|');
            let name = parts.next()?.trim();
            if name.is_empty() || !seen.insert(name) {
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

    #[cfg(unix)]
    struct FakeHelper(PathBuf);

    #[cfg(unix)]
    impl FakeHelper {
        fn new(test: &str) -> Self {
            Self(
                std::env::temp_dir()
                    .join(format!("scribular-audio-{test}-{}.sh", std::process::id())),
            )
        }

        fn cli(&self, script: &str) -> AudioCapCli {
            use std::{fs, os::unix::fs::PermissionsExt};

            fs::write(&self.0, script).unwrap();
            fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700)).unwrap();
            AudioCapCli {
                path: self.0.clone(),
            }
        }
    }

    #[cfg(unix)]
    impl Drop for FakeHelper {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

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
    fn lists_a_repeated_name_once() {
        let out = "Brave Browser|48000 Hz, 2 ch|\n\
                   Brave Browser Helper|48000 Hz, 2 ch|a\n\
                   Brave Browser Helper|44100 Hz, 1 ch|b\n";
        let apps = parse_sources(out);
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Brave Browser", "Brave Browser Helper"]);
        assert_eq!(apps[1].icon.as_deref(), Some("a"));
    }

    /// AudioCapCLI needs a live stdin pipe even though we send it no commands.
    /// Model that contract without depending on macOS permissions or playback.
    #[cfg(unix)]
    #[test]
    fn keeps_sidecar_stdin_open_while_delivering_pcm() {
        let fixture = FakeHelper::new("stdin");
        let cli = fixture.cli(
            "#!/bin/sh\n\
              if [ \"$1\" = --list-sources ]; then\n\
                printf 'Fixture|16000 Hz, 1 ch|\\n'\n\
                exit 0\n\
              fi\n\
              [ -p /dev/stdin ] || { echo 'stdin must stay piped' >&2; exit 1; }\n\
              printf '\\000\\000\\000\\077\\000\\000\\000\\277'\n\
              exec sleep 30\n",
        );
        let (sink, mut consumer) = super::super::ring::ring(64);
        let capture = cli.open("Fixture", sink, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            capture.format(),
            Format {
                sample_rate: 16_000,
                channels: 1
            }
        );
        let mut samples = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        while samples.len() < 2 && Instant::now() < deadline {
            consumer.drain(&mut samples);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(samples, [0.5, -0.5]);
        drop(capture);
        assert!(consumer.drain(&mut Vec::new()).closed);
    }

    #[cfg(unix)]
    #[test]
    fn source_list_distinguishes_empty_permission_and_safe_failures() {
        let fixture = FakeHelper::new("list");
        let cli = AudioCapCli {
            path: fixture.0.clone(),
        };
        assert!(cli
            .list_apps()
            .unwrap_err()
            .to_string()
            .contains("Reinstall Scribular"));

        let cli = fixture.cli("#!/bin/sh\nexit 0\n");
        assert!(cli.list_apps().unwrap().is_empty());

        let cli = fixture.cli("#!/bin/sh\necho 'Audio recording permission denied.' >&2\nexit 1\n");
        assert!(matches!(
            cli.list_apps(),
            Err(AudioError::PermissionDenied(Privacy::SystemAudio))
        ));

        let cli = fixture.cli("#!/bin/sh\nexit 1\n");
        assert!(matches!(
            cli.list_apps(),
            Err(AudioError::PermissionDenied(Privacy::SystemAudio))
        ));

        for (stderr, action) in [
            (
                "Error reading process list: Private Browser",
                "Check your sound output",
            ),
            ("Usage: Private Browser", "Reinstall Scribular"),
            (
                "Usage: Private Browser permission denied",
                "Reinstall Scribular",
            ),
            (
                "Unexpected failure from Private Browser",
                "restart Scribular",
            ),
        ] {
            let cli = fixture.cli(&format!("#!/bin/sh\necho '{stderr}' >&2\nexit 1\n"));
            let error = cli.list_apps().unwrap_err();
            assert!(matches!(error, AudioError::Device(_)));
            let message = error.to_string();
            assert!(message.contains(action), "{message}");
            assert!(!message.contains("Private Browser"), "{message}");
        }
    }

    /// Opt-in native check: needs audio-capture permission and an output device.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "plays a local WAV and requires macOS audio-capture permission"]
    fn bundled_helper_delivers_audio_during_playback() {
        struct Player(Child);
        impl Drop for Player {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mut player = Player(
            Command::new("/usr/bin/afplay")
                .arg(root.join("tests/fixtures/audio/2-speakers-12s.wav"))
                .spawn()
                .unwrap(),
        );
        thread::sleep(Duration::from_millis(300));
        let cli = AudioCapCli {
            path: root.join("binaries/AudioCapCLI-aarch64-apple-darwin"),
        };
        let (sink, mut consumer) = super::super::ring::ring(1 << 20);
        let capture = cli.open("afplay", sink, &AtomicBool::new(false)).unwrap();
        let mut samples = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            consumer.drain(&mut samples);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            player.0.try_wait().unwrap().is_none(),
            "playback ended early"
        );
        drop(capture);
        consumer.drain(&mut samples);
        assert!(!samples.is_empty(), "helper delivered no PCM");
        assert!(samples.iter().all(|x| x.is_finite()), "invalid PCM");
        let rms = (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt();
        assert!(rms > 0.002, "helper delivered silence: RMS {rms}");
        assert!(consumer.drain(&mut Vec::new()).closed);
    }

    #[test]
    fn detects_permission_denied() {
        assert!(is_permission_denied("Audio recording permission denied.\n"));
        assert!(!is_permission_denied("no such source"));
    }
}
