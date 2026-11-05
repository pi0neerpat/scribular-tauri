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
    /// `Err` is a transport failure (no connection, timeout). A redirect to
    /// another host is not followed; it comes back as its 3xx reply.
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
