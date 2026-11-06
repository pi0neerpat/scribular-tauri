//! Parity spec 01: the database is encrypted at rest (AS1-AS5), and a
//! prototype plaintext database is migrated safely, including after a crash
//! at any step (AS9-AS11).

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, AtomicUsize, Ordering},
};

use scribular_lib::store::key::{Key, KeyStore, LockReason, MemoryKeyStore};
use scribular_lib::store::{
    AudioSource, InputMode, NewSession, SessionStatus, Store, StoreError, Timings,
};
use scribular_lib::text::Segment;

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

/// A prototype plaintext database at schema v3 (before `audio_kept`): a canary
/// session with a segment, an interrupted capture that still has its audio
/// (R10) and a setting. Built encrypted, taken back to v3, then exported with
/// an empty key. Returns the canary session's id.
fn plaintext_fixture(dir: &TempDir) -> String {
    let enc = dir.sibling(".fixture");
    let id = {
        let store = Store::open(&enc, &MemoryKeyStore::with_key(KEY)).unwrap();
        let id = add_canary_session(&store);
        let segment = Segment {
            start_ms: 0,
            end_ms: 900,
            text: Some(format!("{CANARY} segment")),
        };
        store.add_segment(&id, AudioSource::Mic, &segment).unwrap();
        let failed = store
            .create_session(&NewSession {
                title: "interrupted".into(),
                input_mode: InputMode::Mic,
                prompt_id: None,
                model_id: None,
            })
            .unwrap();
        store
            .append_audio_chunk(&failed.id, AudioSource::Mic, 0, &[1; 1600])
            .unwrap();
        store
            .set_status(&failed.id, SessionStatus::Failed, Some("interrupted"))
            .unwrap();
        store.set_setting("fixture_setting", "kept").unwrap();
        id
    };
    let conn = rusqlite::Connection::open(&enc).unwrap();
    conn.execute_batch(&format!(
        "PRAGMA key = \"x'{}'\";",
        KEY.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
    .unwrap();
    // Undo the v4 through v7 additions to make a genuine v3 plaintext database.
    conn.execute_batch(
        "ALTER TABLE sessions DROP COLUMN is_sample;
         DROP INDEX sessions_audio_kept;
         ALTER TABLE sessions DROP COLUMN audio_kept;
         ALTER TABLE sessions DROP COLUMN diarization_state;
         ALTER TABLE sessions DROP COLUMN speaker_map;
         ALTER TABLE sessions DROP COLUMN raw_transcript;
         ALTER TABLE sessions DROP COLUMN plain_transcript;
         ALTER TABLE sessions DROP COLUMN previous_summary;
         ALTER TABLE sessions DROP COLUMN previous_prompt_id;
         ALTER TABLE sessions DROP COLUMN previous_model_id;
         PRAGMA user_version = 3;",
    )
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
    assert_eq!(store.schema_version().unwrap(), 7, "v3 → latest");
    assert_eq!(store.segments(&id, AudioSource::Mic).unwrap().len(), 1);
    let items = store.list_sessions().unwrap();
    assert_eq!(items.len(), 2);
    let failed = items.iter().find(|i| i.title == "interrupted").unwrap();
    assert_eq!(failed.status, SessionStatus::Failed);
    assert_eq!(store.audio_chunk_count(&failed.id).unwrap(), 1, "R10");
    assert_eq!(
        store.get_setting("fixture_setting").unwrap().as_deref(),
        Some("kept")
    );
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
fn as10_plaintext_copy_that_cannot_be_removed_keeps_the_store_locked() {
    let dir = TempDir::new();
    plaintext_fixture(&dir);
    let store = Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap();
    // Stand-in for a first run whose scrub failed: its notice was never set.
    assert!(store.take_encryption_notice().unwrap());
    drop(store);
    // A non-empty directory can't be unlinked, so the scrub fails.
    let old = dir.sibling(".plaintext-old");
    std::fs::create_dir(&old).unwrap();
    std::fs::write(old.join("x"), CANARY).unwrap();

    let keys = MemoryKeyStore::with_key(KEY);
    assert_eq!(
        locked(Store::open(&dir.db(), &keys)),
        LockReason::MigrationFailed
    );
    // The retry: the leftover goes and the one-time notice still shows.
    std::fs::remove_dir_all(&old).unwrap();
    std::fs::write(&old, CANARY).unwrap();
    let store = Store::open(&dir.db(), &keys).unwrap();
    assert!(!old.exists());
    assert!(store.take_encryption_notice().unwrap());
    assert!(!store.take_encryption_notice().unwrap());
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
        // Store-level paths that carry session text: create, transcript,
        // segment, summary, regenerate, delete.
        let s = store
            .create_session(&NewSession {
                title: format!("{CANARY} title"),
                input_mode: InputMode::Mic,
                prompt_id: None,
                model_id: None,
            })
            .unwrap();
        store.save_transcript(&s.id, CANARY, 1000).unwrap();
        store
            .add_segment(
                &s.id,
                AudioSource::Mic,
                &Segment {
                    start_ms: 0,
                    end_ms: 1000,
                    text: Some(CANARY.into()),
                },
            )
            .unwrap();
        store
            .set_summary(&s.id, &format!("{CANARY} summary"), &Timings::default())
            .unwrap();
        store
            .set_summary(&s.id, &format!("{CANARY} regenerated"), &Timings::default())
            .unwrap();
        store.delete_session(&s.id).unwrap();
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

#[test]
fn start_fresh_that_fails_puts_the_old_file_back() {
    let dir = TempDir::new();
    {
        let store = Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).unwrap();
        add_canary_session(&store);
    }
    let old = hash(&dir.db());
    let mut keys = MemoryKeyStore::with_key([9; 32]);
    keys.fail_set = true;
    assert_eq!(
        locked(Store::start_fresh(&dir.db(), &keys)),
        LockReason::KeychainWriteFailed
    );
    assert_eq!(hash(&dir.db()), old);
    assert!(
        dir.names().iter().all(|n| !n.contains("unreadable")),
        "{:?}",
        dir.names()
    );
    assert!(Store::open(&dir.db(), &MemoryKeyStore::with_key(KEY)).is_ok());
}
