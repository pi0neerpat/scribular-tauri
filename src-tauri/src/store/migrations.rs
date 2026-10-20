//! Ordered schema migrations, tracked in `PRAGMA user_version`. Each one runs
//! in its own transaction together with its version bump.

use rusqlite::{Connection, Transaction};

use super::{prompts, settings, Result, StoreError};

type Migration = fn(&Transaction) -> Result<()>;

/// Append only. Never edit a migration that has shipped.
const MIGRATIONS: &[Migration] = &[v1_initial];

pub(super) fn migrate(conn: &mut Connection) -> Result<()> {
    let known = MIGRATIONS.len() as u32;
    let found: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if found > known {
        return Err(StoreError::SchemaTooNew { found, known });
    }
    for (version, migration) in (1..).zip(MIGRATIONS).skip(found as usize) {
        let tx = conn.transaction()?;
        migration(&tx)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

/// `plans/TAURI_MVP.md` §2 data model, minus `audio_kept` (KTD2), plus the
/// KTD16 `timings` JSON column; seeds the Electron default prompts.
fn v1_initial(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        "CREATE TABLE sessions (
             id          TEXT PRIMARY KEY,
             title       TEXT NOT NULL DEFAULT '',
             created_at  INTEGER NOT NULL,
             input_mode  TEXT NOT NULL,
             status      TEXT NOT NULL,
             error       TEXT,
             prompt_id   TEXT,
             model_id    TEXT,
             transcript  TEXT NOT NULL DEFAULT '',
             summary     TEXT NOT NULL DEFAULT '',
             duration_ms INTEGER,
             timings     TEXT
         );
         CREATE INDEX sessions_created_at ON sessions(created_at);
         CREATE TABLE segments (
             session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
             source     TEXT NOT NULL,
             start_ms   INTEGER NOT NULL,
             end_ms     INTEGER NOT NULL,
             text       TEXT
         );
         CREATE INDEX segments_session ON segments(session_id, source, start_ms);
         CREATE TABLE audio_chunks (
             session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
             source     TEXT NOT NULL,
             seq        INTEGER NOT NULL,
             pcm        BLOB NOT NULL,
             PRIMARY KEY (session_id, source, seq)
         );
         CREATE TABLE prompts (
             id          TEXT PRIMARY KEY,
             name        TEXT NOT NULL,
             description TEXT NOT NULL,
             text        TEXT NOT NULL,
             is_default  INTEGER NOT NULL,
             starred     INTEGER NOT NULL
         );
         CREATE TABLE settings (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )?;
    let defaults = prompts::defaults()?;
    for p in &defaults.prompts {
        prompts::insert(tx, p)?;
    }
    settings::insert(tx, settings::ACTIVE_PROMPT_ID, &defaults.active_prompt_id)?;
    Ok(())
}
