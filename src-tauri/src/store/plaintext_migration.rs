//! Prototype plaintext DB → SQLCipher (parity spec 01 R8-R10). Runs before the
//! numbered migrations. Every step leaves a file state that [`recover`] turns
//! back into "plaintext DB only" or "encrypted DB only".

use std::{
    fs::{self, File, OpenOptions},
    io::{Seek, Write},
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection, OpenFlags};

use super::key::{apply_key, raw_key, DbState, Key};

pub(super) fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Undo or finish a migration a crash interrupted (R8, last paragraph).
pub(super) fn recover(path: &Path) -> Result<(), String> {
    let old = sibling(path, ".plaintext-old");
    // Crash between the two renames: put the plaintext DB back and redo.
    if !path.exists() && old.exists() {
        fs::rename(&old, path).map_err(|e| format!("restore plaintext DB: {e}"))?;
        sync_dir(path);
    }
    for suffix in [
        ".migrating",
        ".migrating-journal",
        ".migrating-wal",
        ".migrating-shm",
    ] {
        let f = sibling(path, suffix);
        if f.exists() {
            fs::remove_file(&f).map_err(|e| format!("remove stale {suffix}: {e}"))?;
        }
    }
    // Crash after the swap: the encrypted DB is live, the old copy goes.
    if old.exists() && DbState::of(path).map_err(|e| e.to_string())? == DbState::Encrypted {
        scrub(&old)?;
    }
    Ok(())
}

/// Export the plaintext DB at `path` into an encrypted copy, verify it, swap it
/// in and scrub the plaintext. An error before the swap leaves the plaintext
/// file untouched; a failed scrub after it errors too, so the store stays
/// locked and the next [`recover`] retries the scrub.
pub(super) fn migrate(path: &Path, key: &Key, known: u32) -> Result<(), String> {
    let tmp = sibling(path, ".migrating");
    let result = export_and_verify(path, &tmp, key, known);
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    // The plaintext -wal/-shm would otherwise attach to the new file's name.
    scrub(&sibling(path, "-wal"))?;
    scrub(&sibling(path, "-shm"))?;
    let old = sibling(path, ".plaintext-old");
    fs::rename(path, &old).map_err(|e| format!("rename plaintext DB: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| format!("rename encrypted DB: {e}"))?;
    sync_dir(path);
    scrub(&old)
}

fn export_and_verify(path: &Path, tmp: &Path, key: &Key, known: u32) -> Result<(), String> {
    let src = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(err("open plaintext"))?;
    let version: u32 = src
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(err("read user_version"))?;
    if !(1..=known).contains(&version) {
        return Err(format!("user_version {version} outside 1..={known}"));
    }
    src.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
        .map_err(err("checkpoint"))?;
    let before = row_counts(&src).map_err(err("count source"))?;
    let tmp_str = tmp.to_str().ok_or("non-UTF-8 data path")?;
    // ATTACH inherits the source's no-create flag, so make the empty file first.
    std::fs::File::create_new(tmp).map_err(|e| format!("create copy: {e}"))?;
    let raw = raw_key(key);
    src.execute(
        "ATTACH DATABASE ?1 AS encrypted KEY ?2",
        params![tmp_str, raw.0.as_str()],
    )
    .map_err(err("attach"))?;
    src.query_row("SELECT sqlcipher_export('encrypted')", [], |_| Ok(()))
        .map_err(err("export"))?;
    src.pragma_update(Some("encrypted"), "user_version", version)
        .map_err(err("set user_version"))?;
    src.execute("DETACH DATABASE encrypted", [])
        .map_err(err("detach"))?;
    src.close()
        .map_err(|(_, e)| format!("close plaintext: {e}"))?;

    let dst = Connection::open(tmp).map_err(err("open encrypted"))?;
    apply_key(&dst, key).map_err(err("key"))?;
    let check: String = dst
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(err("integrity_check"))?;
    if check != "ok" {
        return Err(format!("integrity_check: {check}"));
    }
    let v: u32 = dst
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(err("read new user_version"))?;
    let after = row_counts(&dst).map_err(err("count copy"))?;
    if v != version || after != before {
        return Err("copy does not match the source".into());
    }
    dst.close()
        .map_err(|(_, e)| format!("close encrypted: {e}"))?;
    File::open(tmp)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("fsync copy: {e}"))
}

fn err(step: &'static str) -> impl Fn(rusqlite::Error) -> String {
    move |e| format!("{step}: {e}")
}

/// Every table's row count, by name, so the copy is compared table by table.
fn row_counts(c: &Connection) -> rusqlite::Result<Vec<(String, i64)>> {
    let names: Vec<String> = c
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    names
        .into_iter()
        .map(|n| {
            let count = c.query_row(&format!("SELECT count(*) FROM \"{n}\""), [], |r| r.get(0))?;
            Ok((n, count))
        })
        .collect()
}

/// R9: overwrite with zeros, fsync, unlink. Best effort: on APFS/SSD the old
/// blocks may survive; FileVault is what protects them. Logs no content.
/// Only a failed unlink is an error: the whole plaintext file would remain.
pub(super) fn scrub(path: &Path) -> Result<(), String> {
    let Ok(meta) = fs::metadata(path) else {
        return Ok(());
    };
    let overwrite = || -> std::io::Result<()> {
        let mut f = OpenOptions::new().write(true).open(path)?;
        f.rewind()?;
        let zeros = vec![0u8; 64 * 1024];
        let mut left = meta.len();
        while left > 0 {
            let n = left.min(zeros.len() as u64) as usize;
            f.write_all(&zeros[..n])?;
            left -= n as u64;
        }
        f.sync_all()
    };
    let overwritten = overwrite();
    let removed = fs::remove_file(path);
    eprintln!(
        "plaintext scrub {}: overwrite {}, unlink {}",
        path.file_name().unwrap_or_default().to_string_lossy(),
        if overwritten.is_ok() { "ok" } else { "failed" },
        if removed.is_ok() { "ok" } else { "failed" },
    );
    removed.map_err(|e| format!("remove plaintext copy: {e}"))
}

fn sync_dir(path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = File::open(dir).and_then(|d| d.sync_all());
    }
}
