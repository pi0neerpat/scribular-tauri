//! Ordered schema migrations, tracked in `PRAGMA user_version`. Each one runs
//! in its own transaction together with its version bump.

use rusqlite::{Connection, Transaction};

use super::{prompts, settings, Result, StoreError};

type Migration = fn(&Transaction) -> Result<()>;

/// Append only. Never edit a migration that has shipped.
const MIGRATIONS: &[Migration] = &[
    v1_initial,
    v2_summary_queue,
    v3_queue_restore_status,
    v4_audio_kept,
    v5_diarization,
    v6_sample_sessions,
];

fn v6_sample_sessions(tx: &Transaction) -> Result<()> {
    tx.execute_batch("ALTER TABLE sessions ADD COLUMN is_sample INTEGER NOT NULL DEFAULT 0;")?;
    Ok(())
}

fn v2_summary_queue(tx: &Transaction) -> Result<()> {
    tx.execute_batch("CREATE TABLE summary_queue (position INTEGER PRIMARY KEY AUTOINCREMENT, session_id TEXT NOT NULL UNIQUE REFERENCES sessions(id) ON DELETE CASCADE, prompt_id TEXT NOT NULL);")?;
    Ok(())
}

fn v3_queue_restore_status(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE summary_queue ADD COLUMN restore_status TEXT NOT NULL DEFAULT 'finalizing';",
    )?;
    Ok(())
}

/// Parity spec 01 R13b: whether the session's audio was kept on purpose.
fn v4_audio_kept(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE sessions ADD COLUMN audio_kept INTEGER NOT NULL DEFAULT 0;
         CREATE INDEX sessions_audio_kept ON sessions(audio_kept, created_at) WHERE audio_kept = 1;",
    )?;
    Ok(())
}

fn v5_diarization(tx: &Transaction) -> Result<()> {
    tx.execute_batch(
        "ALTER TABLE sessions ADD COLUMN plain_transcript TEXT;
         ALTER TABLE sessions ADD COLUMN raw_transcript TEXT;
         ALTER TABLE sessions ADD COLUMN speaker_map TEXT;
         ALTER TABLE sessions ADD COLUMN diarization_state TEXT NOT NULL DEFAULT 'none';",
    )?;
    Ok(())
}

pub(super) fn known() -> u32 {
    MIGRATIONS.len() as u32
}

pub(super) fn migrate(conn: &mut Connection) -> Result<()> {
    let known = known();
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

#[cfg(test)]
mod tests {
    use super::v6_sample_sessions;
    use rusqlite::Connection;

    #[test]
    fn v6_marks_existing_sessions_as_non_samples_without_changing_them() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE sessions (id TEXT PRIMARY KEY, title TEXT); INSERT INTO sessions VALUES ('existing', 'Keep me');").unwrap();
        let tx = conn.transaction().unwrap();
        v6_sample_sessions(&tx).unwrap();
        tx.commit().unwrap();

        let (title, is_sample): (String, bool) = conn
            .query_row(
                "SELECT title, is_sample FROM sessions WHERE id = 'existing'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(title, "Keep me");
        assert!(!is_sample);
    }
}
