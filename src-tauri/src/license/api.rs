//! The one network call this spec adds: `POST /api/validate` (R5). Behind a
//! trait so tests never reach the network (R14).

use std::time::Duration;

/// Release builds always talk to production. Debug builds may point at a
/// local server, as Electron's `apiConfig.js` does.
pub fn api_root() -> String {
    #[cfg(debug_assertions)]
    if let Ok(root) = std::env::var("SCRIBULAR_API_ROOT") {
        return root;
    }
    if cfg!(debug_assertions) {
        "http://localhost:3000".into()
    } else {
        "https://www.scribular.com".into()
    }
}

pub struct HttpReply {
    pub status: u16,
    pub body: String,
}

pub trait LicenseApi: Send + Sync {
    /// `Err` is a transport failure (no connection, timeout, truncated body).
    /// A redirect to another origin (scheme, host or port) is not followed; it
    /// comes back as its 3xx reply.
    fn post_validate(&self, key: &str, device_id: &str) -> Result<HttpReply, String>;
}

pub struct HttpLicenseApi {
    client: reqwest::Client,
    url: reqwest::Url,
}

impl HttpLicenseApi {
    pub fn new(root: &str) -> Result<Self, String> {
        let url = reqwest::Url::parse(root)
            .and_then(|u| u.join("/api/validate"))
            .map_err(|e| format!("bad API root: {e}"))?;
        let origin = url.origin();
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            // Same scheme, host and port only: no downgrade to http, no other
            // service (www → apex style hops stay refused too).
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 5 {
                    attempt.error("too many redirects")
                } else if attempt.url().origin() == origin {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self { client, url })
    }
}

impl LicenseApi for HttpLicenseApi {
    /// Blocks: call from a blocking thread, never from the async runtime.
    fn post_validate(&self, key: &str, device_id: &str) -> Result<HttpReply, String> {
        let body = serde_json::json!({ "licenseKey": key, "deviceId": device_id }).to_string();
        tauri::async_runtime::block_on(async {
            let resp = self
                .client
                .post(self.url.clone())
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body)
                .send()
                .await
                .map_err(|e| e.without_url().to_string())?;
            let status = resp.status().as_u16();
            // A body cut off mid-read is a transport failure, not a reply.
            let body = resp.text().await.map_err(|e| e.without_url().to_string())?;
            Ok(HttpReply { status, body })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Serves each connection the canned `response` after reading the request.
    /// Returns the root URL and a count of requests served.
    fn serve(response: String) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let root = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
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
                let _ = reader.read_exact(&mut body);
                counter.fetch_add(1, Ordering::SeqCst);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (root, hits)
    }

    #[test]
    fn cross_origin_redirect_is_not_followed() {
        let (other, other_hits) =
            serve("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".into());
        // Same host, different port: a different origin.
        let (root, _) = serve(format!(
            "HTTP/1.1 302 Found\r\nLocation: {other}/api/validate\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ));
        let reply = HttpLicenseApi::new(&root)
            .unwrap()
            .post_validate("k", "d")
            .unwrap();
        assert_eq!(reply.status, 302);
        assert_eq!(
            other_hits.load(Ordering::SeqCst),
            0,
            "redirect target never contacted"
        );
    }

    #[test]
    fn truncated_body_is_a_transport_error() {
        let (root, _) = serve(
            "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{\"valid\":".into(),
        );
        let api = HttpLicenseApi::new(&root).unwrap();
        assert!(api.post_validate("k", "d").is_err());
    }
}

/// Replies queued by a test, in order. Records every call.
#[cfg(any(test, feature = "fake-engines"))]
#[derive(Default)]
pub struct FakeLicenseApi {
    pub replies: std::sync::Mutex<std::collections::VecDeque<Result<HttpReply, String>>>,
    pub calls: std::sync::Mutex<Vec<(String, String)>>,
}

#[cfg(any(test, feature = "fake-engines"))]
impl FakeLicenseApi {
    pub fn reply(&self, status: u16, body: &str) {
        self.replies.lock().unwrap().push_back(Ok(HttpReply {
            status,
            body: body.into(),
        }));
    }

    pub fn offline(&self) {
        self.replies
            .lock()
            .unwrap()
            .push_back(Err("connection refused".into()));
    }

    pub fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[cfg(any(test, feature = "fake-engines"))]
impl LicenseApi for FakeLicenseApi {
    fn post_validate(&self, key: &str, device_id: &str) -> Result<HttpReply, String> {
        self.calls
            .lock()
            .unwrap()
            .push((key.into(), device_id.into()));
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err("no reply queued".into()))
    }
}
