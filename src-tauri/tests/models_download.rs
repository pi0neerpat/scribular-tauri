//! U4: model downloads against a local HTTP server — progress, resume,
//! checksum failure, the no-network fast path, and download order.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, Mutex,
    },
};

use scribular_lib::models::{
    ensure, DownloadError, ModelKind, ModelManager, ModelProgress, ModelSpec, ModelState,
};
use sha2::{Digest, Sha256};

/// One request as the server saw it: path and `Range` header.
type Seen = (String, Option<String>);

/// Serves fixed bodies over HTTP/1.1, honouring `Range: bytes=N-`. The first
/// response for a path listed in `cut_after` stops after that many body
/// bytes and drops the connection.
struct Server {
    base: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Server {
    fn start(bodies: HashMap<String, Vec<u8>>, cut_after: HashMap<String, usize>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let cuts = Arc::new(Mutex::new(cut_after));
        let log = seen.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                serve(stream, &bodies, &cuts, &log);
            }
        });
        Self { base, seen }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{path}", self.base)
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn serve(
    mut stream: TcpStream,
    bodies: &HashMap<String, Vec<u8>>,
    cuts: &Mutex<HashMap<String, usize>>,
    log: &Mutex<Vec<Seen>>,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let path = line
        .split_whitespace()
        .nth(1)
        .unwrap()
        .trim_start_matches('/')
        .to_string();
    let mut range = None;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("range") {
                range = Some(value.trim().to_string());
            }
        }
    }
    log.lock().unwrap().push((path.clone(), range.clone()));

    let Some(body) = bodies.get(&path) else {
        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        return;
    };
    let start: usize = range
        .as_deref()
        .and_then(|r| r.strip_prefix("bytes=")?.strip_suffix('-')?.parse().ok())
        .unwrap_or(0);
    let head = if range.is_some() {
        format!(
            "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{}\r\n",
            body.len() - 1,
            body.len()
        )
    } else {
        "HTTP/1.1 200 OK\r\n".to_string()
    };
    let rest = &body[start..];
    let _ = stream.write_all(
        format!(
            "{head}Content-Length: {}\r\nConnection: close\r\n\r\n",
            rest.len()
        )
        .as_bytes(),
    );
    let send = match cuts.lock().unwrap().remove(&path) {
        Some(cut) => &rest[..cut],
        None => rest,
    };
    let _ = stream.write_all(send);
    let _ = stream.flush();
}

fn body(len: usize, seed: u8) -> Vec<u8> {
    (0..len)
        .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn spec(server: &Server, id: &str, kind: ModelKind, data: &[u8]) -> ModelSpec {
    ModelSpec {
        id: id.into(),
        kind,
        display_name: id.into(),
        file_name: format!("{id}.bin"),
        url: server.url(&format!("{id}.bin")),
        size: data.len() as u64,
        sha256: sha256_hex(data),
        license: "test".into(),
        license_url: "https://example.invalid".into(),
    }
}

/// A fresh, empty models directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "scribular-models-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

fn recorder() -> (
    Arc<Mutex<Vec<ModelProgress>>>,
    impl Fn(ModelProgress) + Send + Sync,
) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    (events, move |p| sink.lock().unwrap().push(p))
}

const MB: usize = 1 << 20;

#[test]
fn download_reports_increasing_progress_then_ready() {
    let data = body(3 * MB + 123, 1);
    let server = Server::start(
        HashMap::from([("m.bin".into(), data.clone())]),
        HashMap::new(),
    );
    let spec = spec(&server, "m", ModelKind::Llm, &data);
    let dir = TempDir::new();
    let (events, on_progress) = recorder();

    let path = run(ensure(&reqwest::Client::new(), &dir.0, &spec, &on_progress)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), data);

    let events = events.lock().unwrap();
    let downloading: Vec<_> = events
        .iter()
        .filter(|p| p.state == ModelState::Downloading)
        .map(|p| p.downloaded_bytes)
        .collect();
    assert!(downloading.len() >= 3, "{downloading:?}");
    assert!(
        downloading.windows(2).all(|w| w[0] <= w[1]),
        "{downloading:?}"
    );
    assert_eq!(*downloading.last().unwrap(), data.len() as u64);
    let states: Vec<_> = events.iter().map(|p| p.state).collect();
    assert_eq!(
        &states[states.len() - 2..],
        [ModelState::Verifying, ModelState::Ready]
    );
    assert!(events.iter().all(|p| p.total_bytes == data.len() as u64));
}

#[test]
fn interrupted_download_resumes_with_a_range_request() {
    let data = body(2 * MB, 2);
    let cut = MB + 7;
    let server = Server::start(
        HashMap::from([("m.bin".into(), data.clone())]),
        HashMap::from([("m.bin".into(), cut)]),
    );
    let spec = spec(&server, "m", ModelKind::Llm, &data);
    let dir = TempDir::new();
    let client = reqwest::Client::new();
    let (_, on_progress) = recorder();

    let err = run(ensure(&client, &dir.0, &spec, &on_progress)).unwrap_err();
    assert!(
        matches!(
            err,
            DownloadError::Http(_) | DownloadError::Incomplete { .. }
        ),
        "{err}"
    );
    let part = dir.0.join("m.bin.part");
    assert_eq!(std::fs::metadata(&part).unwrap().len(), cut as u64);

    let path = run(ensure(&client, &dir.0, &spec, &on_progress)).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), data);
    assert!(!part.exists());
    assert_eq!(
        server.seen(),
        [
            ("m.bin".to_string(), None),
            ("m.bin".to_string(), Some(format!("bytes={cut}-"))),
        ]
    );
}

#[test]
fn checksum_mismatch_deletes_the_file_and_never_marks_ready() {
    let data = body(MB / 2, 3);
    let server = Server::start(
        HashMap::from([("m.bin".into(), data.clone())]),
        HashMap::new(),
    );
    let mut spec = spec(&server, "m", ModelKind::Llm, &data);
    spec.sha256 = "0".repeat(64);
    let dir = TempDir::new();
    let manager = ModelManager::new(dir.0.clone(), vec![spec]);
    let (events, on_progress) = recorder();

    run(manager.ensure_all(&on_progress));

    assert_eq!(manager.readiness.get("m"), ModelState::Failed);
    let events = events.lock().unwrap();
    assert!(events.iter().all(|p| p.state != ModelState::Ready));
    let last = events.last().unwrap();
    assert_eq!(last.state, ModelState::Failed);
    assert!(
        last.error.as_deref().unwrap().contains("checksum"),
        "{last:?}"
    );
    assert!(!dir.0.join("m.bin").exists());
    assert!(!dir.0.join("m.bin.part").exists());
}

#[test]
fn verified_model_is_ready_without_a_request() {
    let data = body(MB, 4);
    let server = Server::start(
        HashMap::from([("m.bin".into(), data.clone())]),
        HashMap::new(),
    );
    let spec = spec(&server, "m", ModelKind::Speech, &data);
    let dir = TempDir::new();
    let client = reqwest::Client::new();
    let (_, on_progress) = recorder();
    run(ensure(&client, &dir.0, &spec, &on_progress)).unwrap();
    assert_eq!(server.seen().len(), 1);

    // With the stamp, and again after losing it (the file is re-hashed).
    for remove_stamp in [false, true] {
        if remove_stamp {
            std::fs::remove_file(dir.0.join("m.bin.sha256")).unwrap();
        }
        let manager = ModelManager::new(dir.0.clone(), vec![spec.clone()]);
        let (events, on_progress) = recorder();
        run(manager.ensure_all(&on_progress));
        assert_eq!(manager.readiness.get("m"), ModelState::Ready);
        let states: Vec<_> = events.lock().unwrap().iter().map(|p| p.state).collect();
        assert_eq!(states, [ModelState::Ready]);
    }
    assert_eq!(server.seen().len(), 1);
}

#[test]
fn speech_model_downloads_before_the_llm() {
    let speech = body(MB / 4, 5);
    let llm = body(MB / 4, 6);
    let server = Server::start(
        HashMap::from([
            ("speech.bin".into(), speech.clone()),
            ("llm.bin".into(), llm.clone()),
        ]),
        HashMap::new(),
    );
    let dir = TempDir::new();
    let manager = ModelManager::new(
        dir.0.clone(),
        vec![
            spec(&server, "speech", ModelKind::Speech, &speech),
            spec(&server, "llm", ModelKind::Llm, &llm),
        ],
    );
    let (_, on_progress) = recorder();
    run(manager.ensure_all(&on_progress));

    let paths: Vec<_> = server.seen().into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, ["speech.bin", "llm.bin"]);
    assert_eq!(
        manager
            .readiness
            .wait_settled("llm", std::time::Duration::ZERO),
        ModelState::Ready
    );
}
