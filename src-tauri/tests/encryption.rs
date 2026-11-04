//! Parity spec 01: the database is encrypted at rest (AS1-AS5), and a
//! prototype plaintext database is migrated safely, including after a crash
//! at any step (AS9-AS11).

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};

use scribular_lib::store::key::{Key, KeyStore, LockReason, MemoryKeyStore};
use scribular_lib::store::{InputMode, NewSession, Store, StoreError};

const CANARY: &str = "ZEBRA-CANARY-7731";
const MAGIC: &[u8] = b"SQLite format 3\0";
const KEY: Key = [7; 32];

/// A fresh directory, removed on drop, holding `scribular.db`.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "scribular-enc-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn db(&self) -> PathBuf {
        self.0.join("scribular.db")
    }

    fn sibling(&self, suffix: &str) -> PathBuf {
        self.0.join(format!("scribular.db{suffix}"))
    }

    fn names(&self) -> Vec<String> {
        let mut v: Vec<_> = std::fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        v.sort();
        v
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Counts `set` calls, so AS4 can show the key is never replaced.
#[derive(Default)]
struct Counting {
    inner: MemoryKeyStore,
    sets: AtomicUsize,
}

impl KeyStore for Counting {
    fn get(&self) -> Result<Option<Key>, String> {
        self.inner.get()
    }
    fn set(&self, key: &Key) -> Result<(), String> {
        self.sets.fetch_add(1, Ordering::Relaxed);
        self.inner.set(key)
    }
}

fn add_canary_session(store: &Store) -> String {
    let s = store
        .create_session(&NewSession {
            title: format!("{CANARY} title"),
            input_mode: InputMode::MicAndSystem,
            prompt_id: None,
            model_id: None,
        })
        .unwrap();
    store
        .save_transcript(&s.id, &format!("said {CANARY} twice {CANARY}"), 1_000)
        .unwrap();
    s.id
}

fn contains(path: &Path, needle: &str) -> bool {
    std::fs::read(path)
        .map(|b| b.windows(needle.len()).any(|w| w == needle.as_bytes()))
        .unwrap_or(false)
}

fn hash(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

fn locked(r: Result<Store, StoreError>) -> LockReason {
    match r {
        Err(StoreError::Locked(reason)) => reason,
        Err(e) => panic!("expected Locked, got {e}"),
        Ok(_) => panic!("expected Locked, got an open store"),
    }
}

/// A plaintext database at the current schema with one canary session, as the
/// prototype left it: built encrypted, then exported with an empty key.
fn plaintext_fixture(dir: &TempDir) -> String {
    let enc = dir.sibling(".fixture");
    let id = {
        let store = Store::open(&enc, &MemoryKeyStore::with_key(KEY)).unwrap();
        add_canary_session(&store)
    };
    let conn = rusqlite::Connection::open(&enc).unwrap();
    conn.execute_batch(&format!(
        "PRAGMA key = \"x'{}'\";",
        KEY.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
    .unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    conn.execute(
        "ATTACH DATABASE ?1 AS plain KEY ''",
        [dir.db().to_str().unwrap()],
    )
    .unwrap();
    conn.execute_batch(&format!(
        "SELECT sqlcipher_export('plain'); PRAGMA plain.user_version = {version}; DETACH plain;"
    ))
    .unwrap();
    drop(conn);
    for s in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(dir.sibling(&format!(".fixture{s}")));
    }
    assert!(hash(&dir.db()).starts_with(MAGIC));
    assert!(contains(&dir.db(), CANARY));
    id
}

#[test]
fn as1_no_plaintext_on_disk_before_or_after_checkpoint() {
    let dir = TempDir::new();
    let store = Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap();
    add_canary_session(&store);
    for s in ["", "-wal", "-shm"] {
        assert!(
            !contains(&dir.sibling(s), CANARY),
            "canary in db{s} while open"
        );
    }
    drop(store); // closing checkpoints the WAL into the main file
    for s in ["", "-wal", "-shm"] {
        assert!(
            !contains(&dir.sibling(s), CANARY),
            "canary in db{s} after close"
        );
    }
    assert!(!hash(&dir.db()).starts_with(MAGIC));
}

#[test]
fn as2_as4_wrong_or_missing_key_locks_and_leaves_the_file_alone() {
    let dir = TempDir::new();
    drop(Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap());
    let before = hash(&dir.db());

    let wrong = MemoryKeyStore::with_key([9; 32]);
    assert_eq!(locked(Store::open(&dir.db(), &wrong)), LockReason::WrongKey);

    let none = Counting::default();
    assert_eq!(
        locked(Store::open(&dir.db(), &none)),
        LockReason::KeyMissing
    );
    assert_eq!(
        none.sets.load(Ordering::Relaxed),
        0,
        "no new key over existing data"
    );

    assert_eq!(hash(&dir.db()), before);
    assert_eq!(dir.names(), ["scribular.db"]);
}

#[test]
fn as3_runs_on_sqlcipher_with_commoncrypto() {
    let dir = TempDir::new();
    let store = Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap();
    let (version, provider) = store.cipher().unwrap();
    assert!(!version.is_empty());
    if cfg!(target_os = "macos") {
        assert_eq!(provider, "commoncrypto");
    }
}

#[test]
fn as5_keychain_write_failure_creates_no_database() {
    let dir = TempDir::new();
    let mut keys = MemoryKeyStore::default();
    keys.fail_set = true;
    assert_eq!(
        locked(Store::open(&dir.db(), &keys)),
        LockReason::KeychainWriteFailed
    );
    assert!(dir.names().is_empty(), "left {:?}", dir.names());
}

#[test]
fn as9_migrates_a_plaintext_database_once() {
    let dir = TempDir::new();
    let id = plaintext_fixture(&dir);
    let keys = Counting::default();

    let store = Store::open(&dir.db(), &keys).unwrap();
    assert_eq!(keys.sets.load(Ordering::Relaxed), 1);
    let s = store.get_session(&id).unwrap().unwrap();
    assert!(s.transcript.contains(CANARY));
    assert!(store.take_encryption_notice().unwrap());
    assert!(
        !store.take_encryption_notice().unwrap(),
        "notice shows once"
    );
    drop(store);

    assert!(!hash(&dir.db()).starts_with(MAGIC));
    assert!(dir
        .names()
        .iter()
        .all(|n| !n.contains("plaintext") && !n.contains("migrating")));
    for name in dir.names() {
        assert!(!contains(&dir.0.join(&name), CANARY), "canary in {name}");
    }
    let store = Store::open(&dir.db(), &keys).unwrap();
    assert_eq!(
        keys.sets.load(Ordering::Relaxed),
        1,
        "reopen reuses the key"
    );
    assert!(!store.take_encryption_notice().unwrap());
}

#[test]
fn as10_leftover_partial_copy_is_discarded_and_migration_reruns() {
    let dir = TempDir::new();
    let id = plaintext_fixture(&dir);
    std::fs::write(dir.sibling(".migrating"), b"half an export").unwrap();
    std::fs::write(dir.sibling(".migrating-journal"), b"x").unwrap();

    let store = Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap();
    assert!(store.get_session(&id).unwrap().is_some());
    drop(store);
    assert!(dir.names().iter().all(|n| !n.contains("migrating")));
}

#[test]
fn as10_crash_between_renames_restores_the_plaintext_and_migrates() {
    let dir = TempDir::new();
    let id = plaintext_fixture(&dir);
    std::fs::rename(dir.db(), dir.sibling(".plaintext-old")).unwrap();

    let store = Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap();
    assert!(store.get_session(&id).unwrap().is_some());
    drop(store);
    assert!(!dir.sibling(".plaintext-old").exists());
    assert!(!hash(&dir.db()).starts_with(MAGIC));
}

#[test]
fn as10_crash_before_scrub_removes_the_old_plaintext() {
    let dir = TempDir::new();
    plaintext_fixture(&dir);
    std::fs::copy(dir.db(), dir.sibling(".plaintext-old")).unwrap();
    std::fs::remove_file(dir.db()).unwrap();
    drop(Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap());
    // Now: encrypted db, no old copy. Put one back, as if the scrub was cut off.
    std::fs::write(dir.sibling(".plaintext-old"), CANARY).unwrap();

    drop(Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap());
    assert!(!dir.sibling(".plaintext-old").exists());
}

#[test]
fn as10_unreadable_plaintext_fails_and_is_left_untouched() {
    let dir = TempDir::new();
    let mut bytes = MAGIC.to_vec();
    bytes.extend(std::iter::repeat_n(0xAB, 4096));
    std::fs::write(dir.db(), &bytes).unwrap();

    let keys = MemoryKeyStore::with_key(KEY);
    assert_eq!(
        locked(Store::open(&dir.db(), &keys)),
        LockReason::MigrationFailed
    );
    assert_eq!(hash(&dir.db()), bytes);
    assert_eq!(dir.names(), ["scribular.db"]);
}

/// AS11: migration logs name files and steps, never their content. Runs the
/// migration in a child copy of this test binary to capture its stderr.
#[test]
fn as11_migration_logs_no_session_content() {
    if std::env::var_os("SCRIBULAR_AS11_CHILD").is_some() {
        let dir = TempDir::new();
        plaintext_fixture(&dir);
        std::fs::write(dir.sibling(".plaintext-old"), CANARY).unwrap();
        std::fs::rename(dir.db(), dir.sibling(".restore")).unwrap();
        // Corrupt-source failure, then a crash-recovery path, then a full migration.
        std::fs::write(dir.db(), [MAGIC, &[0xAB; 4096]].concat()).unwrap();
        let _ = Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY));
        std::fs::remove_file(dir.sibling(".plaintext-old")).unwrap();
        std::fs::rename(dir.sibling(".restore"), dir.db()).unwrap();
        let store = Store::open(&dir.db(), &MemoryKeyStore::default()).unwrap();
        eprintln!("child finished {}", store.schema_version().unwrap());
        return;
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "as11_migration_logs_no_session_content",
            "--nocapture",
        ])
        .env("SCRIBULAR_AS11_CHILD", "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "child failed: {stderr}");
    assert!(
        stderr.contains("child finished"),
        "child didn't run: {stderr}"
    );
    assert!(!stderr.contains(CANARY) && !stdout.contains(CANARY));
}

#[test]
fn start_fresh_sets_the_old_file_aside_under_a_new_key() {
    let dir = TempDir::new();
    {
        let store = Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap();
        add_canary_session(&store);
    }
    let old = hash(&dir.db());
    let keys = MemoryKeyStore::with_key([9; 32]);
    assert_eq!(locked(Store::open(&dir.db(), &keys)), LockReason::WrongKey);

    let store = Store::start_fresh(&dir.db(), &keys).unwrap();
    assert!(store.list_sessions().unwrap().is_empty());
    let new_key = keys.get().unwrap().unwrap();
    assert_ne!(new_key, [9; 32]);
    drop(store);

    let aside: Vec<_> = dir
        .names()
        .into_iter()
        .filter(|n| n.contains("unreadable"))
        .collect();
    assert!(!aside.is_empty(), "{:?}", dir.names());
    assert!(aside.iter().any(|n| hash(&dir.0.join(n)) == old));
    assert!(Store::open(&dir.db(), &keys).is_ok());
}
