//! Test builds only (`remote-llm`): summaries from a host `llama-server`, so
//! uitest runs in the VM don't wait on its CPU. Same prompt format, context
//! and sampling as [`super::llama`]; never compiled into shipping builds.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

use serde::{Deserialize, Serialize};

use super::{
    llama::{self, format_gemma4_user_prompt, LlamaLoader, Params},
    GenEvent, Generation, LlmError, Result, Summarizer, SummarizerLoader,
};
use crate::models::ModelManager;

/// `SCRIBULAR_LLM_URL`'s value, treating empty as unset.
pub fn remote_url(env: Option<String>) -> Option<String> {
    env.filter(|url| !url.is_empty())
}

/// Readiness and model id come from the on-device loader, so onboarding and
/// session records stay the same; only generation goes to the server.
pub struct RemoteLoader {
    local: LlamaLoader,
    base: String,
}

impl RemoteLoader {
    pub fn new(models: Arc<ModelManager>, base: String) -> Self {
        Self {
            local: LlamaLoader::new(models),
            base,
        }
    }
}

impl SummarizerLoader for RemoteLoader {
    fn model_id(&self) -> String {
        self.local.model_id()
    }

    fn ready(&self) -> bool {
        self.local.ready()
    }

    fn load(&self, id: &str) -> Result<Arc<dyn Summarizer>> {
        if !self.ready() {
            return Err(LlmError::NotReady);
        }
        Ok(Arc::new(RemoteSummarizer::new(
            &self.base,
            llama::params(id),
        )?))
    }
}

pub struct RemoteSummarizer {
    base: reqwest::Url,
    client: reqwest::Client,
    params: Params,
}

enum Piece {
    Text(String),
    Done,
    Err(String),
}

/// A struct, not `json!`, so `f32` sampling values keep their f32 text (0.2, not 0.2000000029).
#[derive(Serialize)]
struct CompletionBody {
    prompt: String,
    n_predict: usize,
    temperature: f32,
    top_p: f32,
    top_k: i32,
    repeat_last_n: i32,
    repeat_penalty: f32,
    stream: bool,
    cache_prompt: bool,
}

#[derive(Deserialize)]
struct Tokens {
    tokens: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Chunk {
    #[serde(default)]
    content: String,
    #[serde(default)]
    stop: bool,
}

impl RemoteSummarizer {
    pub fn new(base: &str, params: Params) -> Result<Self> {
        let mut url =
            reqwest::Url::parse(base).map_err(|e| LlmError::Engine(format!("{base}: {e}")))?;
        // `join` replaces the last path segment unless the base ends in '/'.
        if !url.path().ends_with('/') {
            url.set_path(&format!("{}/", url.path()));
        }
        let client = reqwest::Client::builder()
            // A wrong host fails the job instead of hanging it.
            .connect_timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| LlmError::Engine(format!("{base}: {e}")))?;
        Ok(Self {
            base: url,
            client,
            params,
        })
    }

    fn endpoint(&self, path: &str) -> reqwest::Url {
        self.base.join(path).expect("fixed relative path")
    }

    fn tokenize(&self, content: &str) -> Result<usize> {
        let url = self.endpoint("tokenize");
        let body = serde_json::json!({ "content": content }).to_string();
        let reply = tauri::async_runtime::block_on(async {
            let resp = post(&self.client, &url, body).await?;
            resp.bytes().await.map_err(|e| e.without_url().to_string())
        });
        reply
            .and_then(|b| {
                serde_json::from_slice::<Tokens>(&b)
                    .map(|t| t.tokens.len())
                    .map_err(|e| e.to_string())
            })
            .map_err(|e| LlmError::Engine(format!("{url}: {e}")))
    }

    fn completion_body(&self, request: &Generation) -> String {
        let p = &self.params;
        serde_json::to_string(&CompletionBody {
            prompt: format_gemma4_user_prompt(&request.prompt),
            n_predict: request.max_tokens,
            temperature: p.temperature,
            top_p: p.top_p,
            top_k: p.top_k,
            repeat_last_n: p.repeat_last_n,
            repeat_penalty: p.repeat_penalty,
            stream: true,
            cache_prompt: true,
        })
        .expect("plain struct serializes")
    }
}

async fn post(
    client: &reqwest::Client,
    url: &reqwest::Url,
    body: String,
) -> std::result::Result<reqwest::Response, String> {
    let resp = client
        .post(url.clone())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(body)
        .send()
        .await
        .map_err(|e| e.without_url().to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    Ok(resp)
}

/// Streams one `/completion` into `tx` as `seq`'s pieces, ending with `Done`.
async fn stream(
    client: reqwest::Client,
    url: reqwest::Url,
    body: String,
    seq: usize,
    tx: mpsc::Sender<(usize, Piece)>,
) {
    let result = async {
        let mut resp = post(&client, &url, body).await?;
        let mut buf = Vec::new();
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| e.without_url().to_string())?
        {
            buf.extend_from_slice(&chunk);
            // Decode only whole events, so a UTF-8 char split across chunks stays intact.
            while let Some(end) = buf.windows(2).position(|w| w == b"\n\n") {
                let event: Vec<u8> = buf.drain(..end + 2).collect();
                for line in event.split(|&b| b == b'\n') {
                    let Some(data) = line.strip_prefix(b"data:") else {
                        continue;
                    };
                    let chunk: Chunk =
                        serde_json::from_slice(data.trim_ascii()).map_err(|e| e.to_string())?;
                    if !chunk.content.is_empty() {
                        let _ = tx.send((seq, Piece::Text(chunk.content)));
                    }
                    if chunk.stop {
                        return Ok(());
                    }
                }
            }
        }
        Err("stream ended before stop".to_string())
    }
    .await;
    let _ = tx.send((seq, result.map_or_else(Piece::Err, |()| Piece::Done)));
}

impl Summarizer for RemoteSummarizer {
    fn context_tokens(&self) -> usize {
        self.params.context_tokens as usize
    }

    fn count_tokens(&self, text: &str) -> Result<usize> {
        self.tokenize(text)
    }

    fn prompt_tokens(&self, prompt: &str) -> Result<usize> {
        self.tokenize(&format_gemma4_user_prompt(prompt))
    }

    /// The caller is a plain worker thread: the streams run on tauri's
    /// runtime while this thread drains them, so `cancel` is seen promptly.
    fn generate(
        &self,
        requests: &[Generation],
        cancel: &AtomicBool,
        on_event: &mut dyn FnMut(GenEvent<'_>),
    ) -> Result<Vec<String>> {
        let url = self.endpoint("completion");
        let (tx, rx) = mpsc::channel();
        let tasks: Vec<_> = requests
            .iter()
            .enumerate()
            .map(|(seq, request)| {
                tauri::async_runtime::spawn(stream(
                    self.client.clone(),
                    url.clone(),
                    self.completion_body(request),
                    seq,
                    tx.clone(),
                ))
            })
            .collect();
        drop(tx);
        let mut outputs = vec![String::new(); requests.len()];
        let mut left = requests.len();
        let result = loop {
            if left == 0 {
                break Ok(std::mem::take(&mut outputs));
            }
            if cancel.load(Ordering::Relaxed) {
                break Err(LlmError::Cancelled);
            }
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok((seq, Piece::Text(text))) => {
                    on_event(GenEvent::Token { seq, text: &text });
                    outputs[seq].push_str(&text);
                }
                Ok((seq, Piece::Done)) => {
                    left -= 1;
                    on_event(GenEvent::Finished { seq });
                }
                Ok((_, Piece::Err(e))) => break Err(LlmError::Engine(format!("{url}: {e}"))),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    break Err(LlmError::Engine(format!("{url}: stream stopped")))
                }
            }
        };
        for task in tasks {
            task.abort();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Mutex;
    use std::time::Instant;

    type Handler = dyn Fn(&str, &str, &mut TcpStream) + Send + Sync;

    /// Serves each connection on its own thread: reads the request, records
    /// its body, and lets `handler(path, body, stream)` write the reply.
    fn stub(
        handler: impl Fn(&str, &str, &mut TcpStream) + Send + Sync + 'static,
    ) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let root = format!("http://{}", listener.local_addr().unwrap());
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&bodies);
        let handler: Arc<Handler> = Arc::new(handler);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let handler = Arc::clone(&handler);
                let recorded = Arc::clone(&recorded);
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut request_line = String::new();
                    reader.read_line(&mut request_line).unwrap();
                    let path = request_line.split(' ').nth(1).unwrap_or("").to_string();
                    let mut len = 0;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                            break;
                        }
                        if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                            len = v.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0; len];
                    reader.read_exact(&mut body).unwrap();
                    let body = String::from_utf8(body).unwrap();
                    recorded.lock().unwrap().push(body.clone());
                    handler(&path, &body, &mut stream);
                });
            }
        });
        (root, bodies)
    }

    fn json_reply(stream: &mut TcpStream, body: &str) {
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    }

    /// Writes the SSE `body` in `slice`-byte pieces, flushing between them.
    fn sse_reply(stream: &mut TcpStream, body: &[u8], slice: usize) {
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
        );
        for piece in body.chunks(slice) {
            if stream.write_all(piece).is_err() {
                return;
            }
            let _ = stream.flush();
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn sse(pieces: &[&str]) -> Vec<u8> {
        let mut body = String::new();
        for p in pieces {
            body += &format!(
                "data: {}\n\n",
                serde_json::json!({ "content": p, "stop": false })
            );
        }
        body += "data: {\"content\":\"\",\"stop\":true}\n\n";
        body.into_bytes()
    }

    fn summarizer(root: &str) -> RemoteSummarizer {
        RemoteSummarizer::new(root, llama::params(crate::models::GEMMA_4_E2B)).unwrap()
    }

    fn request(prompt: &str) -> Generation {
        Generation {
            prompt: prompt.into(),
            max_tokens: 77,
        }
    }

    fn run(s: &RemoteSummarizer, requests: &[Generation]) -> (Result<Vec<String>>, Vec<String>) {
        let mut events = Vec::new();
        let out = s.generate(requests, &AtomicBool::new(false), &mut |e| {
            events.push(format!("{e:?}"))
        });
        (out, events)
    }

    #[test]
    fn count_tokens_uses_tokenize() {
        let (root, _) = stub(|path, _, s| {
            assert_eq!(path, "/tokenize");
            json_reply(s, r#"{"tokens":[1,2,3,4,5,6]}"#)
        });
        assert_eq!(summarizer(&root).count_tokens("hello"), Ok(6));
    }

    #[test]
    fn prompt_tokens_counts_formatted_turn() {
        let (root, bodies) = stub(|_, _, s| json_reply(s, r#"{"tokens":[1]}"#));
        summarizer(&root).prompt_tokens("hi").unwrap();
        let body: serde_json::Value = serde_json::from_str(&bodies.lock().unwrap()[0]).unwrap();
        assert_eq!(body["content"], "<|turn>user\nhi\n<turn|>\n<|turn>model\n");
    }

    #[test]
    fn context_is_params_cap() {
        assert_eq!(summarizer("http://127.0.0.1:1").context_tokens(), 8192);
    }

    #[test]
    fn generate_streams_tokens_and_finishes() {
        let (root, _) = stub(|path, _, s| {
            assert_eq!(path, "/completion");
            sse_reply(s, &sse(&["po", "ng"]), 4096)
        });
        let (out, events) = run(&summarizer(&root), &[request("ping")]);
        assert_eq!(out, Ok(vec!["pong".to_string()]));
        assert_eq!(
            events,
            [
                format!("{:?}", GenEvent::Token { seq: 0, text: "po" }),
                format!("{:?}", GenEvent::Token { seq: 0, text: "ng" }),
                format!("{:?}", GenEvent::Finished { seq: 0 }),
            ]
        );
    }

    #[test]
    fn events_split_across_chunks() {
        let (root, _) = stub(|_, _, s| sse_reply(s, &sse(&["po", "n", "é"]), 3));
        let (out, _) = run(&summarizer(&root), &[request("ping")]);
        assert_eq!(out, Ok(vec!["poné".to_string()]));
    }

    #[test]
    fn batch_returns_in_request_order() {
        let (root, _) = stub(|_, body, s| {
            if body.contains("prompt A") {
                std::thread::sleep(Duration::from_millis(300));
                sse_reply(s, &sse(&["a text"]), 4096)
            } else {
                sse_reply(s, &sse(&["b text"]), 4096)
            }
        });
        let (out, events) = run(
            &summarizer(&root),
            &[request("prompt A"), request("prompt B")],
        );
        assert_eq!(out, Ok(vec!["a text".to_string(), "b text".to_string()]));
        assert_eq!(
            events.first().unwrap(),
            &format!(
                "{:?}",
                GenEvent::Token {
                    seq: 1,
                    text: "b text"
                }
            ),
            "B finished first"
        );
    }

    #[test]
    fn cancel_stops_generation() {
        let (root, _) = stub(|_, _, s| {
            let _ = s.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n",
            );
            while s
                .write_all(b"data: {\"content\":\"x\",\"stop\":false}\n\n")
                .is_ok()
            {
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        let out = summarizer(&root).generate(&[request("ping")], &cancel, &mut |_| {
            cancel.store(true, Ordering::Relaxed)
        });
        assert_eq!(out, Err(LlmError::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn unreachable_server_is_engine_error() {
        let s = summarizer("http://127.0.0.1:1");
        for result in [
            s.count_tokens("x").map(|_| ()),
            run(&s, &[request("x")]).0.map(|_| ()),
        ] {
            match result {
                Err(LlmError::Engine(msg)) => assert!(msg.contains("127.0.0.1:1"), "{msg}"),
                other => panic!("expected engine error, got {other:?}"),
            }
        }
    }

    #[test]
    fn completion_body_carries_params() {
        let (root, bodies) = stub(|_, _, s| sse_reply(s, &sse(&["ok"]), 4096));
        run(&summarizer(&root), &[request("ping")]).0.unwrap();
        let body = bodies.lock().unwrap()[0].clone();
        for field in [
            r#""n_predict":77"#,
            r#""temperature":0.2"#,
            r#""top_p":0.9"#,
            r#""stream":true"#,
        ] {
            assert!(body.contains(field), "{field} missing from {body}");
        }
    }

    #[test]
    fn remote_url_reads_env() {
        assert_eq!(remote_url(None), None);
        assert_eq!(remote_url(Some(String::new())), None);
        assert_eq!(
            remote_url(Some("http://h:5801".into())),
            Some("http://h:5801".into())
        );
    }
}
