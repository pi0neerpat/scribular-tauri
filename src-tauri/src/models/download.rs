//! Resumable, verified download of one model file.
//!
//! Bytes go to `<file>.part`; an interrupted download resumes from the
//! partial file's length with a `Range` request. Once complete, the file is
//! hashed, and only a matching sha256 renames it into place. A `<file>.sha256`
//! stamp then lets later launches skip re-hashing gigabytes.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use reqwest::{header, StatusCode};
use sha2::{Digest, Sha256};

use super::{ModelProgress, ModelSpec, ModelState};

/// Emit progress at most this often, in bytes.
const PROGRESS_STEP: u64 = 1 << 20;

#[derive(Debug)]
pub enum DownloadError {
    Io(io::Error),
    Http(reqwest::Error),
    Status(StatusCode),
    /// The server sent more bytes than the manifest size.
    TooLarge,
    /// The server closed the connection before sending every byte.
    Incomplete {
        got: u64,
        expected: u64,
    },
    HashMismatch {
        expected: String,
        actual: String,
    },
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "file error: {e}"),
            Self::Http(e) => write!(f, "network error: {e}"),
            Self::Status(s) => write!(f, "server responded {s}"),
            Self::TooLarge => write!(f, "server sent more data than expected"),
            Self::Incomplete { got, expected } => {
                write!(f, "download stopped at {got} of {expected} bytes")
            }
            Self::HashMismatch { expected, actual } => {
                write!(f, "checksum mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for DownloadError {}

impl From<io::Error> for DownloadError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<reqwest::Error> for DownloadError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e)
    }
}

pub fn model_path(dir: &Path, spec: &ModelSpec) -> PathBuf {
    dir.join(&spec.file_name)
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 1 << 20];
    loop {
        match file.read(&mut buf)? {
            0 => return Ok(hex(&hasher.finalize())),
            n => hasher.update(&buf[..n]),
        }
    }
}

/// Whether the final file is present with the right size and either a
/// matching stamp or, failing that, a matching hash. Never touches the
/// network. A present file that fails verification is deleted.
pub fn verify_existing(dir: &Path, spec: &ModelSpec) -> io::Result<bool> {
    let path = model_path(dir, spec);
    let stamp = with_suffix(&path, ".sha256");
    match fs::metadata(&path) {
        Ok(m) if m.len() == spec.size => {}
        Ok(_) => {
            fs::remove_file(&path)?;
            let _ = fs::remove_file(&stamp);
            return Ok(false);
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    }
    if fs::read_to_string(&stamp).is_ok_and(|s| s.trim() == spec.sha256) {
        return Ok(true);
    }
    if sha256_file(&path)? == spec.sha256 {
        fs::write(&stamp, &spec.sha256)?;
        Ok(true)
    } else {
        fs::remove_file(&path)?;
        let _ = fs::remove_file(&stamp);
        Ok(false)
    }
}

fn progress(spec: &ModelSpec, state: ModelState, downloaded: u64) -> ModelProgress {
    ModelProgress {
        model_id: spec.id.clone(),
        state,
        downloaded_bytes: downloaded,
        total_bytes: spec.size,
        error: None,
    }
}

/// Makes `spec` present and verified in `dir`, downloading or resuming as
/// needed. `on_progress` sees `Downloading` (non-decreasing byte counts),
/// then `Verifying`, then `Ready`; failures are returned, not reported.
pub async fn ensure(
    client: &reqwest::Client,
    dir: &Path,
    spec: &ModelSpec,
    on_progress: &(dyn Fn(ModelProgress) + Send + Sync),
) -> Result<PathBuf, DownloadError> {
    fs::create_dir_all(dir)?;
    let path = model_path(dir, spec);
    if verify_existing(dir, spec)? {
        on_progress(progress(spec, ModelState::Ready, spec.size));
        return Ok(path);
    }

    let part = with_suffix(&path, ".part");
    let mut offset = fs::metadata(&part).map_or(0, |m| m.len());
    if offset > spec.size {
        fs::remove_file(&part)?;
        offset = 0;
    }

    if offset < spec.size {
        let mut request = client.get(&spec.url);
        if offset > 0 {
            request = request.header(header::RANGE, format!("bytes={offset}-"));
        }
        let mut response = request.send().await?;
        let mut file = match response.status() {
            StatusCode::PARTIAL_CONTENT if offset > 0 => {
                OpenOptions::new().append(true).open(&part)?
            }
            // The server ignored the range; start over.
            StatusCode::OK => {
                offset = 0;
                File::create(&part)?
            }
            status => return Err(DownloadError::Status(status)),
        };
        on_progress(progress(spec, ModelState::Downloading, offset));
        let mut reported = offset;
        loop {
            let chunk = match response.chunk().await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(e) => {
                    file.flush()?;
                    return Err(e.into());
                }
            };
            offset += chunk.len() as u64;
            if offset > spec.size {
                drop(file);
                fs::remove_file(&part)?;
                return Err(DownloadError::TooLarge);
            }
            file.write_all(&chunk)?;
            if offset - reported >= PROGRESS_STEP {
                reported = offset;
                on_progress(progress(spec, ModelState::Downloading, offset));
            }
        }
        file.sync_all()?;
        if offset < spec.size {
            return Err(DownloadError::Incomplete {
                got: offset,
                expected: spec.size,
            });
        }
        on_progress(progress(spec, ModelState::Downloading, offset));
    }

    on_progress(progress(spec, ModelState::Verifying, spec.size));
    let actual = sha256_file(&part)?;
    if actual != spec.sha256 {
        fs::remove_file(&part)?;
        return Err(DownloadError::HashMismatch {
            expected: spec.sha256.clone(),
            actual,
        });
    }
    fs::rename(&part, &path)?;
    fs::write(with_suffix(&path, ".sha256"), &spec.sha256)?;
    on_progress(progress(spec, ModelState::Ready, spec.size));
    Ok(path)
}
