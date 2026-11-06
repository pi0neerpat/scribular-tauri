use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use crate::store::{now_ms, Store};
use serde::Serialize;

pub const TELEMETRY_ENABLED: &str = "telemetry_enabled";
const INSTALL_ID: &str = "install_id";
const WEEK: &str = "usage_week_start";
const SESSIONS: &str = "usage_sessions";
const SECONDS: &str = "usage_total_seconds";
const LAUNCHES: &str = "usage_launches";
const PENDING: &str = "pending_report";
static COUNTERS: OnceLock<Mutex<()>> = OnceLock::new();
static REPORTS: OnceLock<Mutex<(VecDeque<Instant>, HashMap<&'static str, Instant>)>> =
    OnceLock::new();

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UsagePayload {
    device_id: String,
    sessions_this_week: u32,
    avg_session_duration_minutes: f64,
    app_launches_this_week: u32,
    app_version: &'static str,
    platform: &'static str,
    week_start: String,
}

#[derive(Serialize)]
struct ErrorPayload<'a> {
    context: &'a str,
    message: &'a str,
    metadata: ErrorMetadata<'a>,
    timestamp: String,
    #[serde(rename = "deviceId")]
    device_id: String,
}

#[derive(Serialize)]
struct ErrorMetadata<'a> {
    version: &'static str,
    platform: &'static str,
    arch: &'static str,
    code: &'a str,
}

pub fn send_allowed(store: &Store) -> bool {
    store
        .get_setting(TELEMETRY_ENABLED)
        .ok()
        .flatten()
        .as_deref()
        == Some("1")
}

pub fn set_enabled(store: &Store, enabled: bool) -> Result<(), String> {
    let _guard = COUNTERS
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    store
        .set_setting(TELEMETRY_ENABLED, if enabled { "1" } else { "0" })
        .map_err(|_| "could not save telemetry setting".to_string())?;
    if !enabled {
        for key in [INSTALL_ID, WEEK, PENDING] {
            store
                .delete_setting(key)
                .map_err(|_| "could not clear telemetry data".to_string())?;
        }
        for key in [SESSIONS, SECONDS, LAUNCHES] {
            store
                .set_setting(key, "0")
                .map_err(|_| "could not clear telemetry data".to_string())?;
        }
    }
    Ok(())
}

fn ensure_week(store: &Store) -> Result<String, String> {
    let week = week_start(now_ms());
    if store
        .get_setting(WEEK)
        .map_err(|_| "settings unavailable".to_string())?
        .as_deref()
        != Some(&week)
    {
        for key in [SESSIONS, SECONDS, LAUNCHES] {
            store
                .set_setting(key, "0")
                .map_err(|_| "settings unavailable".to_string())?;
        }
        store
            .set_setting(WEEK, &week)
            .map_err(|_| "settings unavailable".to_string())?;
    }
    Ok(week)
}

fn increment(store: &Store, key: &str, amount: u64) {
    let current = store
        .get_setting(key)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let _ = store.set_setting(key, &current.saturating_add(amount).to_string());
}

fn install_id(store: &Store) -> Option<String> {
    if let Some(id) = store.get_setting(INSTALL_ID).ok().flatten() {
        return Some(id);
    }
    let key = crate::store::key::new_key().ok()?;
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&key[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let id = format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    );
    store.set_setting(INSTALL_ID, &id).ok()?;
    Some(id)
}

fn usage(store: &Store) -> Option<UsagePayload> {
    if !send_allowed(&store) {
        return None;
    }
    let _guard = COUNTERS
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if !send_allowed(store) {
        return None;
    }
    let week_start = ensure_week(store).ok()?;
    let sessions = parse_counter(store, SESSIONS).min(200) as u32;
    let seconds = parse_counter(store, SECONDS);
    let average = if sessions == 0 {
        0.0
    } else {
        (seconds as f64 / parse_counter(store, SESSIONS) as f64 / 60.0).min(480.0)
    };
    Some(UsagePayload {
        device_id: install_id(store)?,
        sessions_this_week: sessions,
        avg_session_duration_minutes: (average * 100.0).round() / 100.0,
        app_launches_this_week: parse_counter(store, LAUNCHES).min(500) as u32,
        app_version: env!("CARGO_PKG_VERSION"),
        platform: "darwin-arm64",
        week_start,
    })
}

fn parse_counter(store: &Store, key: &str) -> u64 {
    store
        .get_setting(key)
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

pub fn record_launch(store: Arc<Store>) {
    if !send_allowed(&store) {
        return;
    }
    let _guard = COUNTERS
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if !send_allowed(&store) {
        return;
    }
    let _ = ensure_week(&store);
    increment(&store, LAUNCHES, 1);
    drop(_guard);
    send_usage(Arc::clone(&store));
    send_pending(store);
}

pub fn record_session(store: Arc<Store>, duration_ms: u32) {
    if !send_allowed(&store) {
        return;
    }
    let _guard = COUNTERS
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if !send_allowed(&store) {
        return;
    }
    if ensure_week(&store).is_err() {
        return;
    }
    increment(&store, SESSIONS, 1);
    increment(&store, SECONDS, u64::from(duration_ms / 1000));
    drop(_guard);
    send_usage(store);
}

pub fn send_usage(store: Arc<Store>) {
    let Some(payload) = usage(&store) else {
        return;
    };
    let Ok(body) = serde_json::to_value(payload) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        if !send_allowed(&store) {
            return;
        }
        if let Some(root) = api_root() {
            if let Ok(client) = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
            {
                let _ = client
                    .post(format!("{root}/api/usage"))
                    .json(&body)
                    .send()
                    .await;
            }
        }
    });
}

fn api_root() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        std::env::var("SCRIBULAR_API_ROOT")
            .ok()
            .filter(|s| !s.is_empty())
    }
    #[cfg(not(debug_assertions))]
    {
        Some("https://www.scribular.com".into())
    }
}

pub fn report(store: Arc<Store>, code: &'static str) {
    if !send_allowed(&store) {
        return;
    }
    if !matches!(
        code,
        "summarize" | "recording" | "dialog" | "unhandled" | "test"
    ) {
        return;
    }
    {
        let mut state = REPORTS
            .get_or_init(|| Mutex::new((VecDeque::new(), HashMap::new())))
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        state
            .0
            .retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        if state.0.len() >= 30
            || state
                .1
                .get(code)
                .is_some_and(|t| now.duration_since(*t) < Duration::from_secs(600))
        {
            return;
        }
        state.0.push_back(now);
        state.1.insert(code, now);
    }
    let _guard = COUNTERS
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if !send_allowed(&store) {
        return;
    }
    let Some(device_id) = install_id(&store) else {
        return;
    };
    let Ok(body) = serde_json::to_value(error_payload(code, device_id)) else {
        return;
    };
    let _ = store.set_setting(PENDING, &body.to_string());
    drop(_guard);
    send_pending(store);
}

fn error_payload(code: &'static str, device_id: String) -> ErrorPayload<'static> {
    let context = match code {
        "summarize" => "summarize",
        "recording" => "recording",
        "dialog" => "dialog",
        _ => "unhandled",
    };
    let message = match code {
        "summarize" => "Summarization failed.",
        "recording" => "Recording failed.",
        "dialog" => "A model operation failed.",
        _ => "An unexpected application error occurred.",
    };
    ErrorPayload {
        context,
        message,
        metadata: ErrorMetadata {
            version: env!("CARGO_PKG_VERSION"),
            platform: "darwin",
            arch: if cfg!(target_arch = "aarch64") {
                "arm64"
            } else {
                "x64"
            },
            code,
        },
        timestamp: timestamp(),
        device_id,
    }
}

fn send_pending(store: Arc<Store>) {
    if !send_allowed(&store) {
        return;
    }
    let Some(body) = store
        .get_setting(PENDING)
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_str::<serde_json::Value>(&v).ok())
    else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        if !send_allowed(&store) {
            return;
        }
        if let Some(root) = api_root() {
            if let Ok(client) = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
            {
                if client
                    .post(format!("{root}/api/errors"))
                    .json(&body)
                    .send()
                    .await
                    .is_ok_and(|response| response.status().is_success())
                    && send_allowed(&store)
                {
                    let _ = store.delete_setting(PENDING);
                }
            }
        }
    });
}

fn timestamp() -> String {
    super::log::timestamp()
}

fn week_start(ms: i64) -> String {
    let days = (ms / 86_400_000).saturating_sub((ms / 86_400_000 + 3).rem_euclid(7));
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Arc<Store>, std::path::PathBuf) {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "scribular-telemetry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(
            &dir.join("db"),
            &crate::store::key::MemoryKeyStore::with_key([7; 32]),
        )
        .unwrap();
        (Arc::new(store), dir)
    }

    #[test]
    fn undecided_never_creates_an_install_id_or_usage_payload() {
        let (store, dir) = store();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        record_launch(Arc::clone(&store));
        record_session(Arc::clone(&store), 60_000);
        report(Arc::clone(&store), "unhandled");
        send_usage(Arc::clone(&store));
        assert!(!send_allowed(&store));
        assert!(usage(&store).is_none());
        assert_eq!(store.get_setting(INSTALL_ID).unwrap(), None);
        assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn usage_payload_has_the_backend_shape_and_clamps() {
        let (store, dir) = store();
        set_enabled(&store, true).unwrap();
        store.set_setting(WEEK, &week_start(now_ms())).unwrap();
        store.set_setting(SESSIONS, "250").unwrap();
        store.set_setting(SECONDS, "9999999").unwrap();
        store.set_setting(LAUNCHES, "700").unwrap();
        let value = serde_json::to_value(usage(&store).unwrap()).unwrap();
        assert_eq!(value["sessionsThisWeek"], 200);
        assert_eq!(value["avgSessionDurationMinutes"], 480.0);
        assert_eq!(value["appLaunchesThisWeek"], 500);
        assert_eq!(value["platform"], "darwin-arm64");
        assert!(value["deviceId"].as_str().is_some_and(|id| id.len() == 36));
        assert_eq!(value.as_object().unwrap().len(), 7);
        set_enabled(&store, false).unwrap();
        assert_eq!(store.get_setting(INSTALL_ID).unwrap(), None);
        assert_eq!(store.get_setting(SESSIONS).unwrap().as_deref(), Some("0"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn week_start_is_utc_monday() {
        assert_eq!(week_start(0), "1969-12-29");
        assert_eq!(week_start(4 * 86_400_000), "1970-01-05");
    }

    #[test]
    fn error_payload_uses_only_closed_content_free_fields() {
        let value = serde_json::to_value(error_payload("summarize", "device-id".into())).unwrap();
        assert_eq!(value["context"], "summarize");
        assert_eq!(value["message"], "Summarization failed.");
        assert_eq!(value["metadata"]["code"], "summarize");
        assert!(!value.to_string().contains("CANARY-CLIENT-JANE"));
        assert!(value.get("stack").is_none());
    }
}
