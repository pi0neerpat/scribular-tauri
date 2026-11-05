//! Licensing and the free trial (parity spec 04). Rust alone decides access
//! (R1); the webview only shows [`AccessState`].
//!
//! Secrets (`{license_key, device_id, trial_started_at}`) live in one Keychain
//! item, account `license` (R10). The trial start and device ID are mirrored
//! in `settings` (R3/R4); verification times are `settings` only.

pub mod api;

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::store::{key::KeyStore, Store};
use api::LicenseApi;

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;
pub const TRIAL_DAYS: i64 = 14;
/// `TrialWarning` from this many days left.
pub const WARN_DAYS: i64 = 3;
/// Offline days after the last good check that still count as `Licensed`.
pub const GRACE_DAYS: i64 = 30;
/// Further days of `LicensedGrace` before new recordings are refused (R7).
pub const SOFT_DAYS: i64 = 7;

/// Keychain account for the licence item; the service is the bundle id.
pub const KEYCHAIN_ACCOUNT: &str = "license";

const TRIAL_MIRROR: &str = "license.trial_started_at";
const DEVICE_MIRROR: &str = "license.device_id";
const LAST_SEEN: &str = "license.last_seen_at";
const LAST_VERIFIED: &str = "license.last_verified_at";
const EXPIRES: &str = "license.expires_at";
const INVALID_REASON: &str = "license.invalid_reason";
/// Only written when the Keychain refuses the item (R10 fallback).
const KEY_FALLBACK: &str = "license.key_fallback";

/// Why a key was refused by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Rejection {
    InvalidKey,
    Expired,
    DeviceLimit,
    BadFormat,
}

impl Rejection {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidKey => "invalidKey",
            Self::Expired => "expired",
            Self::DeviceLimit => "deviceLimit",
            Self::BadFormat => "badFormat",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        [
            Self::InvalidKey,
            Self::Expired,
            Self::DeviceLimit,
            Self::BadFormat,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }
}

/// R8. Times are ms since the epoch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AccessState {
    Trial {
        #[ts(type = "number")]
        days_left: i64,
    },
    TrialWarning {
        #[ts(type = "number")]
        days_left: i64,
    },
    TrialEnded,
    Licensed {
        #[ts(type = "number | null")]
        expires_at: Option<i64>,
        #[ts(type = "number")]
        verified_at: i64,
    },
    /// Days until new recordings are refused.
    LicensedGrace {
        #[ts(type = "number")]
        days_left: i64,
    },
    LicenseInvalid {
        reason: Rejection,
    },
    LicenseLapsed,
}

impl AccessState {
    /// Only these three deny; warnings never do (R8).
    pub fn allows(&self) -> bool {
        !matches!(
            self,
            Self::TrialEnded | Self::LicenseInvalid { .. } | Self::LicenseLapsed
        )
    }
}

/// R5: the result of `activate_license`, one case per thing the user can do
/// something about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ActivationResult {
    Valid {
        #[ts(type = "number | null")]
        expires_at: Option<i64>,
    },
    InvalidKey,
    Expired,
    DeviceLimit,
    BadFormat,
    Offline,
    ServerError,
}

impl ActivationResult {
    fn rejection(&self) -> Option<Rejection> {
        Some(match self {
            Self::InvalidKey => Rejection::InvalidKey,
            Self::Expired => Rejection::Expired,
            Self::DeviceLimit => Rejection::DeviceLimit,
            Self::BadFormat => Rejection::BadFormat,
            _ => return None,
        })
    }
}

/// R9: the error `start_recording` and `transcribe_file` return. Crosses IPC
/// as a JSON string, read back by `accessDenied()` in `api.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename = "accessDenied")]
pub struct AccessDenied {
    pub state: AccessState,
}

impl From<AccessDenied> for String {
    fn from(e: AccessDenied) -> Self {
        serde_json::to_string(&e).expect("AccessDenied serializes")
    }
}

/// What [`access_status`] reads. Times are ms since the epoch.
#[derive(Debug, Clone, Default)]
pub struct LicenseRecord {
    pub trial_started_at: i64,
    /// High-water mark of the clock, so setting it back gains nothing (R2).
    pub last_seen_at: i64,
    pub has_key: bool,
    pub verified_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub invalid_reason: Option<Rejection>,
}

/// R2/R7/R8 as a pure function of the clock and the record.
pub fn access_status(now: i64, r: &LicenseRecord) -> AccessState {
    let now = now.max(r.last_seen_at);
    if r.has_key {
        let Some(verified) = r.verified_at else {
            return AccessState::LicenseInvalid {
                reason: r.invalid_reason.unwrap_or(Rejection::InvalidKey),
            };
        };
        if r.expires_at.is_some_and(|e| e < now) {
            return AccessState::LicenseLapsed;
        }
        let since = (now - verified).max(0) / DAY_MS;
        return if since < GRACE_DAYS {
            AccessState::Licensed {
                expires_at: r.expires_at,
                verified_at: verified,
            }
        } else if since < GRACE_DAYS + SOFT_DAYS {
            AccessState::LicensedGrace {
                days_left: GRACE_DAYS + SOFT_DAYS - since,
            }
        } else {
            AccessState::LicenseLapsed
        };
    }
    let elapsed = (now - r.trial_started_at).max(0) / DAY_MS;
    let days_left = TRIAL_DAYS - elapsed;
    if days_left <= 0 {
        AccessState::TrialEnded
    } else if days_left <= WARN_DAYS {
        AccessState::TrialWarning { days_left }
    } else {
        AccessState::Trial { days_left }
    }
}

/// Trims and lowercases a key, or `None` if it does not match the server's
/// `^[a-z]+-[a-z]+-[a-z]+-\d{3}$` (scribular-saas-project `lib/license.ts`).
pub fn normalize_key(raw: &str) -> Option<String> {
    let key = raw.trim().to_ascii_lowercase();
    let parts: Vec<&str> = key.split('-').collect();
    let ok = parts.len() == 4
        && parts[..3]
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase()))
        && parts[3].len() == 3
        && parts[3].bytes().all(|b| b.is_ascii_digit());
    ok.then_some(key)
}

/// `first4***last4`, the only form of a key that is logged or shown.
pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "***".into();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}***{tail}")
}

/// Maps a `/api/validate` reply. Never a revocation on 5xx or junk (R5).
pub fn classify(status: u16, body: &str) -> ActivationResult {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Reply {
        valid: Option<bool>,
        message: Option<String>,
        expiration_date: Option<String>,
    }
    if !(200..300).contains(&status) {
        return ActivationResult::ServerError;
    }
    let Ok(reply) = serde_json::from_str::<Reply>(body) else {
        return ActivationResult::ServerError;
    };
    match reply.valid {
        Some(true) => ActivationResult::Valid {
            expires_at: reply.expiration_date.as_deref().and_then(parse_date_ms),
        },
        Some(false) => match reply.message.as_deref().unwrap_or("") {
            "Invalid license key format" => ActivationResult::BadFormat,
            "License has expired" => ActivationResult::Expired,
            m if m.starts_with("License limit reached") => ActivationResult::DeviceLimit,
            // Our device ID is always well formed, so this is our bug.
            "Invalid device ID format" => ActivationResult::ServerError,
            _ => ActivationResult::InvalidKey,
        },
        None => ActivationResult::ServerError,
    }
}

/// True only in debug builds with `SCRIBULAR_SKIP_LICENSE` set (R14).
pub fn skip_requested(debug_build: bool, env: Option<&str>) -> bool {
    debug_build && env.is_some_and(|v| !v.is_empty() && v != "0")
}

/// The dev override. Release builds never read the variable.
pub fn dev_skip() -> bool {
    #[cfg(debug_assertions)]
    let env = std::env::var("SCRIBULAR_SKIP_LICENSE").ok();
    #[cfg(not(debug_assertions))]
    let env: Option<String> = None;
    skip_requested(cfg!(debug_assertions), env.as_deref())
}

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        crate::store::now_ms()
    }
}

/// A clock tests move by hand.
#[cfg(any(test, feature = "fake-engines"))]
pub struct ManualClock(pub std::sync::atomic::AtomicI64);

#[cfg(any(test, feature = "fake-engines"))]
impl ManualClock {
    pub fn at(ms: i64) -> Self {
        Self(ms.into())
    }

    pub fn set(&self, ms: i64) {
        self.0.store(ms, std::sync::atomic::Ordering::SeqCst);
    }
}

#[cfg(any(test, feature = "fake-engines"))]
impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// The Keychain item's contents. No `Debug`, so the key can't be printed.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
struct Secrets {
    license_key: Option<String>,
    device_id: String,
    /// RFC 3339, UTC.
    trial_started_at: String,
}

pub type Log = Box<dyn Fn(&str) + Send + Sync>;

pub struct Licensing {
    store: Arc<Store>,
    keys: Arc<dyn KeyStore>,
    api: Arc<dyn LicenseApi>,
    clock: Arc<dyn Clock>,
    log: Log,
    secrets: Mutex<Secrets>,
    skip: bool,
}

impl Licensing {
    /// Loads or creates the trial start and device ID (R2-R4). Never touches
    /// the network.
    pub fn init(
        store: Arc<Store>,
        keys: Arc<dyn KeyStore>,
        api: Arc<dyn LicenseApi>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self::init_with_log(
            store,
            keys,
            api,
            clock,
            Box::new(|m| eprintln!("[license] {m}")),
        )
    }

    pub fn init_with_log(
        store: Arc<Store>,
        keys: Arc<dyn KeyStore>,
        api: Arc<dyn LicenseApi>,
        clock: Arc<dyn Clock>,
        log: Log,
    ) -> Self {
        let now = clock.now_ms();
        let stored = match keys.get_bytes() {
            Ok(Some(bytes)) => serde_json::from_slice::<Secrets>(&bytes)
                .inspect_err(|_| log("Keychain licence item unreadable; rebuilding it"))
                .ok(),
            Ok(None) => None,
            Err(e) => {
                log(&format!("Keychain unavailable, using settings: {e}"));
                None
            }
        };
        let setting = |k: &str| store.get_setting(k).ok().flatten();

        let trial = [
            stored
                .as_ref()
                .and_then(|s| parse_date_ms(&s.trial_started_at)),
            setting(TRIAL_MIRROR).as_deref().and_then(parse_date_ms),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(now);
        let device_id = stored
            .as_ref()
            .map(|s| s.device_id.clone())
            .filter(|d| is_uuid(d))
            .or_else(|| setting(DEVICE_MIRROR).filter(|d| is_uuid(d)))
            .unwrap_or_else(new_uuid_v4);
        let secrets = Secrets {
            license_key: stored
                .as_ref()
                .and_then(|s| s.license_key.clone())
                .or_else(|| setting(KEY_FALLBACK)),
            device_id,
            trial_started_at: rfc3339(trial),
        };

        let this = Self {
            store,
            keys,
            api,
            clock,
            log,
            secrets: Mutex::new(Secrets::default()),
            skip: false,
        };
        // Only rewrite the Keychain when something changed: each write can
        // prompt the user on an unsigned build.
        if stored.as_ref() != Some(&secrets) {
            this.save(&secrets);
        }
        this.set(TRIAL_MIRROR, &secrets.trial_started_at);
        this.set(DEVICE_MIRROR, &secrets.device_id);
        *this.secrets.lock().unwrap() = secrets;
        this.touch();
        this
    }

    /// The debug-only override from [`dev_skip`].
    pub fn with_skip(mut self, skip: bool) -> Self {
        self.skip = skip;
        if skip {
            (self.log)("SCRIBULAR_SKIP_LICENSE set: access checks skipped (debug build)");
        }
        self
    }

    pub fn device_id(&self) -> String {
        self.secrets.lock().unwrap().device_id.clone()
    }

    pub fn masked_key(&self) -> Option<String> {
        self.secrets
            .lock()
            .unwrap()
            .license_key
            .as_deref()
            .map(mask_key)
    }

    fn record(&self) -> LicenseRecord {
        let s = self.secrets.lock().unwrap();
        let num = |k: &str| {
            self.store
                .get_setting(k)
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok())
        };
        LicenseRecord {
            trial_started_at: parse_date_ms(&s.trial_started_at).unwrap_or(0),
            last_seen_at: num(LAST_SEEN).unwrap_or(0),
            has_key: s.license_key.is_some(),
            verified_at: num(LAST_VERIFIED),
            expires_at: num(EXPIRES),
            invalid_reason: self
                .store
                .get_setting(INVALID_REASON)
                .ok()
                .flatten()
                .and_then(|r| Rejection::parse(&r)),
        }
    }

    pub fn state(&self) -> AccessState {
        access_status(self.clock.now_ms(), &self.record())
    }

    /// The R9 gate. Call it only where a new session is created.
    pub fn require_access(&self) -> Result<(), AccessDenied> {
        let state = self.state();
        if self.skip || state.allows() {
            Ok(())
        } else {
            Err(AccessDenied { state })
        }
    }

    /// Raises the clock high-water mark (R2). On launch and hourly.
    pub fn touch(&self) {
        let seen = self.record().last_seen_at.max(self.clock.now_ms());
        self.set(LAST_SEEN, &seen.to_string());
    }

    /// R5. Blocks on the network: call from a blocking thread.
    pub fn activate(&self, raw: &str) -> ActivationResult {
        let Some(key) = normalize_key(raw) else {
            (self.log)("activation refused locally: bad key format");
            return ActivationResult::BadFormat;
        };
        let result = self.check(&key);
        (self.log)(&format!("activation of {}: {result:?}", mask_key(&key)));
        if let ActivationResult::Valid { expires_at } = result {
            let mut next = self.secrets.lock().unwrap().clone();
            next.license_key = Some(key);
            self.save(&next);
            *self.secrets.lock().unwrap() = next;
            self.mark_verified(expires_at);
        }
        result
    }

    /// R6. `None` with no key stored. Only a definitive answer changes state.
    pub fn revalidate(&self) -> Option<ActivationResult> {
        let key = self.secrets.lock().unwrap().license_key.clone()?;
        let result = self.check(&key);
        (self.log)(&format!("re-validation of {}: {result:?}", mask_key(&key)));
        match &result {
            ActivationResult::Valid { expires_at } => self.mark_verified(*expires_at),
            r => {
                if let Some(reason) = r.rejection() {
                    self.delete(LAST_VERIFIED);
                    self.set(INVALID_REASON, reason.as_str());
                }
            }
        }
        Some(result)
    }

    /// R13. No network call: the server keeps the device slot. The trial
    /// start and device ID stay in the item, so removal can't restart a trial.
    pub fn deactivate(&self) {
        let mut next = self.secrets.lock().unwrap().clone();
        let had = next.license_key.take();
        self.save(&next);
        *self.secrets.lock().unwrap() = next;
        for k in [LAST_VERIFIED, EXPIRES, INVALID_REASON, KEY_FALLBACK] {
            self.delete(k);
        }
        if let Some(key) = had {
            (self.log)(&format!("licence {} removed", mask_key(&key)));
        }
    }

    fn check(&self, key: &str) -> ActivationResult {
        let device = self.device_id();
        match self.api.post_validate(key, &device) {
            Ok(reply) => classify(reply.status, &reply.body),
            Err(e) => {
                (self.log)(&format!("licence server unreachable: {e}"));
                ActivationResult::Offline
            }
        }
    }

    fn mark_verified(&self, expires_at: Option<i64>) {
        self.set(LAST_VERIFIED, &self.clock.now_ms().to_string());
        match expires_at {
            Some(e) => self.set(EXPIRES, &e.to_string()),
            None => self.delete(EXPIRES),
        }
        self.delete(INVALID_REASON);
    }

    /// Writes the Keychain item, or falls back to `settings` (R10).
    fn save(&self, secrets: &Secrets) {
        let json = serde_json::to_vec(secrets).expect("secrets serialize");
        match self.keys.set_bytes(&json) {
            Ok(()) => self.delete(KEY_FALLBACK),
            Err(e) => {
                (self.log)(&format!("Keychain write failed, using settings: {e}"));
                match &secrets.license_key {
                    Some(k) => self.set(KEY_FALLBACK, k),
                    None => self.delete(KEY_FALLBACK),
                }
            }
        }
    }

    fn set(&self, key: &str, value: &str) {
        if let Err(e) = self.store.set_setting(key, value) {
            (self.log)(&format!("could not save {key}: {e}"));
        }
    }

    fn delete(&self, key: &str) {
        if let Err(e) = self.store.delete_setting(key) {
            (self.log)(&format!("could not clear {key}: {e}"));
        }
    }
}

/// A random UUIDv4 from the OS CSPRNG (R4).
fn new_uuid_v4() -> String {
    let key = crate::store::key::new_key().expect("OS random source");
    let mut b = [0u8; 16];
    b.copy_from_slice(&key[..16]);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// The server's check: 8-4-4-4-12 hex, version 1-5, variant 8/9/a/b.
fn is_uuid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
        && (b'1'..=b'5').contains(&b[14])
        && matches!(b[19].to_ascii_lowercase(), b'8' | b'9' | b'a' | b'b')
}

// Howard Hinnant's civil-date algorithms: UTC dates without a date crate.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// `YYYY-MM-DDTHH:MM:SSZ`.
fn rfc3339(ms: i64) -> String {
    let (y, m, d) = civil_from_days(ms.div_euclid(DAY_MS));
    let s = ms.rem_euclid(DAY_MS) / 1000;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// `YYYY-MM-DD`, optionally followed by `THH:MM:SS[.fff]Z`, as UTC.
// shortcut: numeric offsets (+02:00) are not applied; the server and this
// file only ever write UTC. Handle them if another writer appears.
fn parse_date_ms(s: &str) -> Option<i64> {
    let num = |r: std::ops::Range<usize>| -> Option<i64> {
        let part = s.get(r)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())?
    };
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    if s.get(4..5) != Some("-") || s.get(7..8) != Some("-") || !(1..=12).contains(&m) {
        return None;
    }
    if !(1..=31).contains(&d) {
        return None;
    }
    let mut ms = days_from_civil(y, m, d) * DAY_MS;
    if s.len() > 10 {
        if s.get(10..11) != Some("T") {
            return None;
        }
        let (hh, mm, ss) = (num(11..13)?, num(14..16)?, num(17..19)?);
        ms += ((hh * 60 + mm) * 60 + ss) * 1000;
    }
    Some(ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::key::MemoryKeyStore;
    use api::FakeLicenseApi;

    const T0: i64 = 1_790_000_000_000;
    const H: i64 = 60 * 60 * 1000;
    const KEY: &str = "brave-quiet-river-042";

    struct TempDb(std::path::PathBuf);

    impl TempDb {
        fn new() -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "scribular-license-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    struct Fixture {
        lic: Licensing,
        store: Arc<Store>,
        keys: Arc<MemoryKeyStore>,
        api: Arc<FakeLicenseApi>,
        clock: Arc<ManualClock>,
        logs: Arc<Mutex<Vec<String>>>,
        _db: TempDb,
    }

    impl Fixture {
        fn new() -> Self {
            let db = TempDb::new();
            let store = Arc::new(
                Store::open(&db.0.join("db"), &MemoryKeyStore::with_key([7; 32])).unwrap(),
            );
            Self::on(store, Arc::new(MemoryKeyStore::default()), db)
        }

        fn on(store: Arc<Store>, keys: Arc<MemoryKeyStore>, db: TempDb) -> Self {
            let api = Arc::new(FakeLicenseApi::default());
            let clock = Arc::new(ManualClock::at(T0));
            let logs = Arc::new(Mutex::new(Vec::new()));
            let sink = logs.clone();
            let lic = Licensing::init_with_log(
                store.clone(),
                keys.clone(),
                api.clone(),
                clock.clone(),
                Box::new(move |m| sink.lock().unwrap().push(m.into())),
            );
            Self {
                lic,
                store,
                keys,
                api,
                clock,
                logs,
                _db: db,
            }
        }

        fn secrets(&self) -> Secrets {
            serde_json::from_slice(&self.keys.get_bytes().unwrap().unwrap()).unwrap()
        }

        fn licensed(&self) {
            self.api
                .reply(200, r#"{"valid":true,"expirationDate":"2099-12-31"}"#);
            assert!(matches!(
                self.lic.activate(KEY),
                ActivationResult::Valid { .. }
            ));
        }
    }

    fn trial(start: i64) -> LicenseRecord {
        LicenseRecord {
            trial_started_at: start,
            ..Default::default()
        }
    }

    #[test]
    fn as1_trial_start_is_written_once_and_device_id_is_stable() {
        let f = Fixture::new();
        let first = f.secrets();
        assert_eq!(first.trial_started_at, rfc3339(T0));
        assert!(is_uuid(&first.device_id), "{}", first.device_id);
        assert_eq!(&first.device_id[14..15], "4", "version 4");
        assert_eq!(
            f.store.get_setting(TRIAL_MIRROR).unwrap().as_deref(),
            Some(first.trial_started_at.as_str())
        );

        f.clock.set(T0 + 5 * DAY_MS);
        let again = Licensing::init(
            f.store.clone(),
            f.keys.clone(),
            f.api.clone(),
            f.clock.clone(),
        );
        assert_eq!(f.secrets().trial_started_at, first.trial_started_at);
        assert_eq!(again.device_id(), first.device_id);
        assert_eq!(again.state(), AccessState::Trial { days_left: 9 });
        assert_eq!(f.api.call_count(), 0, "init never calls the network");
    }

    #[test]
    fn trial_survives_losing_either_copy() {
        let f = Fixture::new();
        let first = f.secrets();
        // App data wiped: a fresh settings table, same Keychain.
        let db = TempDb::new();
        let store =
            Arc::new(Store::open(&db.0.join("db"), &MemoryKeyStore::with_key([7; 32])).unwrap());
        f.clock.set(T0 + DAY_MS);
        let wiped = Licensing::init(store, f.keys.clone(), f.api.clone(), f.clock.clone());
        assert_eq!(wiped.state(), AccessState::Trial { days_left: 13 });
        assert_eq!(wiped.device_id(), first.device_id);
        // Keychain item gone: the settings mirror keeps the start.
        let keys = Arc::new(MemoryKeyStore::default());
        let lost = Licensing::init(f.store.clone(), keys, f.api.clone(), f.clock.clone());
        assert_eq!(lost.state(), AccessState::Trial { days_left: 13 });
        assert_eq!(lost.device_id(), first.device_id);
    }

    #[test]
    fn as2_trial_boundaries() {
        let now = T0 + 20 * DAY_MS;
        let cases = [
            (now, AccessState::Trial { days_left: 14 }),
            (now - 10 * DAY_MS, AccessState::Trial { days_left: 4 }),
            (
                now - 11 * DAY_MS,
                AccessState::TrialWarning { days_left: 3 },
            ),
            (
                now - 13 * DAY_MS - 23 * H,
                AccessState::TrialWarning { days_left: 1 },
            ),
            (now - 14 * DAY_MS, AccessState::TrialEnded),
            (now - 400 * DAY_MS, AccessState::TrialEnded),
            // Clock before the start: elapsed is zero, not negative.
            (now + 3 * DAY_MS, AccessState::Trial { days_left: 14 }),
        ];
        for (start, want) in cases {
            let got = access_status(now, &trial(start));
            assert_eq!(got, want, "start = now - {}h", (now - start) / H);
        }
        assert!(AccessState::TrialWarning { days_left: 1 }.allows());
        assert!(!AccessState::TrialEnded.allows());
    }

    #[test]
    fn as3_setting_the_clock_back_does_not_extend_the_trial() {
        let rec = LicenseRecord {
            last_seen_at: T0 + 12 * DAY_MS,
            ..trial(T0)
        };
        let back_10_days = T0 + 2 * DAY_MS;
        assert_eq!(
            access_status(back_10_days, &rec),
            AccessState::TrialWarning { days_left: 2 }
        );

        let f = Fixture::new();
        f.clock.set(T0 + 12 * DAY_MS);
        f.lic.touch();
        f.clock.set(T0 + 2 * DAY_MS);
        f.lic.touch();
        assert_eq!(f.lic.state(), AccessState::TrialWarning { days_left: 2 });
    }

    #[test]
    fn as4_valid_key_is_persisted_and_licensed() {
        let f = Fixture::new();
        f.api
            .reply(200, r#"{"valid":true,"expirationDate":"2099-12-31"}"#);
        let r = f.lic.activate("  Brave-Quiet-River-042 ");
        let exp = days_from_civil(2099, 12, 31) * DAY_MS;
        assert_eq!(
            r,
            ActivationResult::Valid {
                expires_at: Some(exp)
            }
        );
        assert_eq!(f.secrets().license_key.as_deref(), Some(KEY));
        let calls = f.api.calls.lock().unwrap().clone();
        assert_eq!(calls, vec![(KEY.to_string(), f.lic.device_id())]);
        assert_eq!(
            f.lic.state(),
            AccessState::Licensed {
                expires_at: Some(exp),
                verified_at: T0
            }
        );
        assert_eq!(f.lic.masked_key().as_deref(), Some("brav***-042"));
    }

    #[test]
    fn as5_rejections_persist_nothing() {
        let cases = [
            (
                "License limit reached. This license is already active on 2 devices.",
                ActivationResult::DeviceLimit,
            ),
            ("License key not found", ActivationResult::InvalidKey),
            ("License has expired", ActivationResult::Expired),
            ("Invalid license key format", ActivationResult::BadFormat),
        ];
        for (message, want) in cases {
            let f = Fixture::new();
            let before = f.secrets();
            let body = serde_json::json!({ "valid": false, "message": message }).to_string();
            f.api.reply(200, &body);
            assert_eq!(f.lic.activate(KEY), want, "{message}");
            assert!(f.secrets() == before, "{message}: Keychain unchanged");
            assert_eq!(f.store.get_setting(LAST_VERIFIED).unwrap(), None);
            assert_eq!(f.lic.state(), AccessState::Trial { days_left: 14 });
        }
        // A licensed install keeps its licence when another key is refused.
        let f = Fixture::new();
        f.licensed();
        let state = f.lic.state();
        f.api
            .reply(200, r#"{"valid":false,"message":"License key not found"}"#);
        assert_eq!(
            f.lic.activate("other-quiet-river-001"),
            ActivationResult::InvalidKey
        );
        assert_eq!(f.lic.state(), state);
        assert_eq!(f.secrets().license_key.as_deref(), Some(KEY));
    }

    #[test]
    fn as6_malformed_keys_never_reach_the_network() {
        let f = Fixture::new();
        for bad in [
            "abc",
            "",
            "brave-quiet-042",
            "brave-quiet-river-42",
            "brave-quiet-river-0042",
            "brave-quiet-river-04a",
            "brave--river-042",
            "bräve-quiet-river-042",
            "brave-quiet-river-042-x",
            "brave quiet-river-042",
        ] {
            assert_eq!(f.lic.activate(bad), ActivationResult::BadFormat, "{bad:?}");
        }
        assert_eq!(f.api.call_count(), 0);
        assert_eq!(normalize_key(" A-B-C-123\n").as_deref(), Some("a-b-c-123"));
    }

    #[test]
    fn as7_offline_within_grace_stays_licensed() {
        let f = Fixture::new();
        f.licensed();
        f.clock.set(T0 + 10 * DAY_MS);
        f.api.offline();
        assert_eq!(f.lic.revalidate(), Some(ActivationResult::Offline));
        assert!(matches!(f.lic.state(), AccessState::Licensed { .. }));
        assert!(f.lic.require_access().is_ok());
        assert_eq!(
            f.store.get_setting(LAST_VERIFIED).unwrap(),
            Some(T0.to_string())
        );
    }

    #[test]
    fn as8_grace_then_lapse() {
        let rec = LicenseRecord {
            has_key: true,
            verified_at: Some(T0),
            ..trial(T0)
        };
        let at = |d: i64| access_status(T0 + d * DAY_MS, &rec);
        assert!(matches!(at(29), AccessState::Licensed { .. }));
        assert_eq!(at(30), AccessState::LicensedGrace { days_left: 7 });
        assert_eq!(at(31), AccessState::LicensedGrace { days_left: 6 });
        assert!(at(31).allows());
        assert_eq!(at(36), AccessState::LicensedGrace { days_left: 1 });
        assert_eq!(at(37), AccessState::LicenseLapsed);
        assert_eq!(at(38), AccessState::LicenseLapsed);
        assert!(!at(38).allows());

        // A verified expiry in the past is lapsed even when just verified.
        let expired = LicenseRecord {
            expires_at: Some(T0 + DAY_MS),
            ..rec
        };
        assert_eq!(
            access_status(T0 + 2 * DAY_MS, &expired),
            AccessState::LicenseLapsed
        );

        let f = Fixture::new();
        f.licensed();
        f.clock.set(T0 + 38 * DAY_MS);
        f.api.offline();
        f.lic.revalidate();
        assert_eq!(
            f.lic.require_access(),
            Err(AccessDenied {
                state: AccessState::LicenseLapsed
            })
        );
    }

    #[test]
    fn as9_definitive_rejection_invalidates_but_outages_do_not() {
        for (status, body) in [
            (503, "Service Unavailable"),
            (500, r#"{"error":"Internal server error"}"#),
            (200, "<html>captive portal</html>"),
            (302, ""),
        ] {
            let f = Fixture::new();
            f.licensed();
            let state = f.lic.state();
            f.api.reply(status, body);
            assert_eq!(f.lic.revalidate(), Some(ActivationResult::ServerError));
            assert_eq!(f.lic.state(), state, "{status}");
        }
        let f = Fixture::new();
        f.licensed();
        f.api.offline();
        f.lic.revalidate();
        assert!(f.lic.require_access().is_ok(), "timeout changes nothing");

        f.api
            .reply(200, r#"{"valid":false,"message":"License has expired"}"#);
        assert_eq!(f.lic.revalidate(), Some(ActivationResult::Expired));
        let denied = AccessState::LicenseInvalid {
            reason: Rejection::Expired,
        };
        assert_eq!(f.lic.state(), denied);
        assert_eq!(
            f.lic.require_access(),
            Err(AccessDenied {
                state: denied.clone()
            })
        );

        // A later good answer restores the licence.
        f.api.reply(200, r#"{"valid":true,"expirationDate":null}"#);
        f.lic.revalidate();
        assert!(matches!(
            f.lic.state(),
            AccessState::Licensed {
                expires_at: None,
                ..
            }
        ));
    }

    #[test]
    fn as12_logs_carry_the_masked_key_only() {
        let f = Fixture::new();
        f.licensed();
        f.api.offline();
        f.lic.revalidate();
        f.api
            .reply(200, r#"{"valid":false,"message":"License key not found"}"#);
        f.lic.activate("Brave-Quiet-River-042");
        f.lic.deactivate();
        let logs = f.logs.lock().unwrap().join("\n");
        assert!(logs.contains("brav***-042"), "{logs}");
        assert!(!logs.to_lowercase().contains(KEY), "{logs}");
    }

    #[test]
    fn deactivate_clears_the_key_and_keeps_the_trial() {
        let f = Fixture::new();
        let before = f.secrets();
        f.licensed();
        f.lic.deactivate();
        let after = f.secrets();
        assert_eq!(after.license_key, None);
        assert_eq!(after.device_id, before.device_id);
        assert_eq!(after.trial_started_at, before.trial_started_at);
        assert_eq!(f.store.get_setting(LAST_VERIFIED).unwrap(), None);
        assert_eq!(f.store.get_setting(EXPIRES).unwrap(), None);
        assert_eq!(f.lic.state(), AccessState::Trial { days_left: 14 });
        assert_eq!(f.api.call_count(), 1, "only the activation");
    }

    #[test]
    fn keychain_failure_falls_back_to_settings() {
        let db = TempDb::new();
        let store =
            Arc::new(Store::open(&db.0.join("db"), &MemoryKeyStore::with_key([7; 32])).unwrap());
        let keys = Arc::new(MemoryKeyStore::refusing());
        let f = Fixture::on(store, keys, db);
        f.licensed();
        assert_eq!(
            f.store.get_setting(KEY_FALLBACK).unwrap().as_deref(),
            Some(KEY)
        );
        let again = Licensing::init(
            f.store.clone(),
            f.keys.clone(),
            f.api.clone(),
            f.clock.clone(),
        );
        assert!(matches!(again.state(), AccessState::Licensed { .. }));
        assert!(f
            .logs
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("Keychain")));
    }

    #[test]
    fn release_builds_ignore_the_skip_variable() {
        assert!(!skip_requested(false, Some("1")));
        assert!(!skip_requested(true, None));
        assert!(!skip_requested(true, Some("0")));
        assert!(skip_requested(true, Some("1")));

        let f = Fixture::new();
        f.clock.set(T0 + 20 * DAY_MS);
        assert!(f.lic.require_access().is_err());
        let skipping = Licensing::init(
            f.store.clone(),
            f.keys.clone(),
            f.api.clone(),
            f.clock.clone(),
        )
        .with_skip(true);
        assert!(skipping.require_access().is_ok());
    }

    #[test]
    fn classify_maps_every_server_reply() {
        use ActivationResult::*;
        let cases = [
            (
                200,
                r#"{"valid":true,"expirationDate":"2099-12-31"}"#,
                Valid {
                    expires_at: Some(days_from_civil(2099, 12, 31) * DAY_MS),
                },
            ),
            (
                200,
                r#"{"valid":true,"expirationDate":"2027-01-02T03:04:05.678Z"}"#,
                Valid {
                    expires_at: Some(
                        days_from_civil(2027, 1, 2) * DAY_MS + ((3 * 60 + 4) * 60 + 5) * 1000,
                    ),
                },
            ),
            (200, r#"{"valid":true}"#, Valid { expires_at: None }),
            (
                200,
                r#"{"valid":false,"message":"Invalid device ID format"}"#,
                ServerError,
            ),
            (
                200,
                r#"{"valid":false,"message":"something new"}"#,
                InvalidKey,
            ),
            (200, r#"{"error":"x"}"#, ServerError),
            (
                404,
                r#"{"valid":false,"message":"License key not found"}"#,
                ServerError,
            ),
            (500, r#"{"error":"Internal server error"}"#, ServerError),
        ];
        for (status, body, want) in cases {
            assert_eq!(classify(status, body), want, "{body}");
        }
    }

    #[test]
    fn dates_round_trip() {
        for ms in [
            0,
            T0,
            T0 + 12_345_000,
            days_from_civil(2099, 12, 31) * DAY_MS,
        ] {
            assert_eq!(parse_date_ms(&rfc3339(ms)), Some(ms / 1000 * 1000));
        }
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(
            parse_date_ms("2024-02-29"),
            Some(days_from_civil(2024, 2, 29) * DAY_MS)
        );
        for bad in [
            "",
            "2024",
            "2024-13-01",
            "2024-01-00",
            "24-01-01",
            "2024-01-01 10:00",
        ] {
            assert_eq!(parse_date_ms(bad), None, "{bad}");
        }
        assert!(!is_uuid("not-a-uuid"));
        assert!(
            !is_uuid("123e4567-e89b-02d3-a456-426614174000"),
            "version 0"
        );
    }

    #[test]
    fn access_denied_crosses_ipc_as_tagged_json() {
        let s: String = AccessDenied {
            state: AccessState::TrialEnded,
        }
        .into();
        assert_eq!(
            s,
            r#"{"kind":"accessDenied","state":{"kind":"trialEnded"}}"#
        );
        let s = serde_json::to_string(&AccessState::TrialWarning { days_left: 2 }).unwrap();
        assert_eq!(s, r#"{"kind":"trialWarning","daysLeft":2}"#);
    }
}
