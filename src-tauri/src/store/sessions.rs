//! Sessions, their transcript segments, and their raw audio: kept until the
//! transcript is saved, or for 7 days when the user keeps audio (parity spec 01).

use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Result, Store, StoreError};

/// The KTD7 job states. `Failed` keeps its reason in `Session::error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum SessionStatus {
    Recording,
    Finalizing,
    AwaitingModel,
    Summarizing,
    Done,
    Cancelled,
    Failed,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Recording => "recording",
            Self::Finalizing => "finalizing",
            Self::AwaitingModel => "awaitingModel",
            Self::Summarizing => "summarizing",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "recording" => Self::Recording,
            "finalizing" => Self::Finalizing,
            "awaitingModel" => Self::AwaitingModel,
            "summarizing" => Self::Summarizing,
            "done" => Self::Done,
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum InputMode {
    /// Microphone only.
    Mic,
    /// One app's audio only.
    System,
    /// Microphone plus system audio, diarized by source.
    MicAndSystem,
}

impl InputMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Mic => "mic",
            Self::System => "system",
            Self::MicAndSystem => "micAndSystem",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "mic" => Some(Self::Mic),
            "system" => Some(Self::System),
            "micAndSystem" => Some(Self::MicAndSystem),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AudioSource {
    Mic,
    System,
}

impl AudioSource {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Mic => "mic",
            Self::System => "system",
        }
    }
}

/// KTD16 stage timings for one summarization run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Timings {
    pub tail_transcription_ms: Option<u32>,
    pub model_load_ms: Option<u32>,
    pub chunk_ms: Vec<u32>,
    pub combine_ms: Option<u32>,
    pub save_ms: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionListItem {
    pub id: String,
    pub title: String,
    #[ts(type = "number")]
    pub created_at: i64,
    pub status: SessionStatus,
    pub duration_ms: Option<u32>,
    pub has_audio: bool,
    pub snippet: String,
    pub interrupted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub title: String,
    /// Milliseconds since the Unix epoch.
    #[ts(type = "number")]
    pub created_at: i64,
    pub input_mode: InputMode,
    pub status: SessionStatus,
    pub error: Option<String>,
    pub prompt_id: Option<String>,
    pub model_id: Option<String>,
    pub transcript: String,
    pub plain_transcript: Option<String>,
    pub raw_transcript: Option<String>,
    pub speaker_map: Option<BTreeMap<String, String>>,
    pub diarization_state: DiarizationState,
    pub summary: String,
    pub duration_ms: Option<u32>,
    pub timings: Option<Timings>,
    /// Kept audio exists and can be played.
    pub has_audio: bool,
    /// When the sweep deletes the kept audio, in ms since the Unix epoch.
    #[ts(type = "number | null")]
    pub audio_expires_at: Option<i64>,
    pub previous_summary: Option<String>,
    pub previous_prompt_id: Option<String>,
    pub previous_model_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DiarizationState {
    None,
    Pending,
    Confirmed,
    Skipped,
}

impl DiarizationState {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "none" => Self::None,
            "pending" => Self::Pending,
            "confirmed" => Self::Confirmed,
            "skipped" => Self::Skipped,
            _ => return None,
        })
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Skipped => "skipped",
        }
    }
}

/// User edits from the session view. Absent fields are left unchanged.
#[derive(Debug, Clone, Default, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionPatch {
    #[serde(default)]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub transcript: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewSession {
    pub title: String,
    pub input_mode: InputMode,
    pub prompt_id: Option<String>,
    pub model_id: Option<String>,
}

/// A capture without a queued transcript cannot safely resume after restart.
const IN_FLIGHT: [SessionStatus; 2] = [SessionStatus::Recording, SessionStatus::Finalizing];
pub const INTERRUPTED: &str = "interrupted";

/// Parity spec 01 R15: kept audio lives this long after the session started.
pub const AUDIO_RETENTION_DAYS: i64 = 7;
pub const AUDIO_RETENTION_MS: i64 = AUDIO_RETENTION_DAYS * 24 * 60 * 60 * 1000;

/// About 4 hours of one 16 kHz i16 stream (R24); longer is not played in-app.
pub const MAX_PLAYBACK_BYTES: i64 = 4 * 60 * 60 * 16_000 * 2;

/// R13b: `audio_kept` is the source of truth, and rows must still exist.
const HAS_AUDIO: &str = "(audio_kept = 1 AND EXISTS \
                         (SELECT 1 FROM audio_chunks a WHERE a.session_id = sessions.id))";

const COLUMNS: &str = "id, title, created_at, input_mode, status, error, prompt_id, model_id, \
                       transcript, summary, duration_ms, timings, plain_transcript, raw_transcript, \
                       speaker_map, diarization_state, previous_summary, previous_prompt_id, previous_model_id";

/// What [`Store::kept_audio`] found for a session.
#[derive(Debug, PartialEq)]
pub enum KeptAudio {
    NotFound,
    Expired,
    TooLarge,
    Streams { mic: Vec<i16>, system: Vec<i16> },
}

fn bad_column(idx: usize, value: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        idx,
        rusqlite::types::Type::Text,
        format!("unknown value {value:?}").into(),
    )
}

fn status_at(row: &Row, idx: usize) -> rusqlite::Result<SessionStatus> {
    let s: String = row.get(idx)?;
    SessionStatus::parse(&s).ok_or_else(|| bad_column(idx, s))
}

fn from_row(row: &Row) -> rusqlite::Result<Session> {
    let mode: String = row.get(3)?;
    let timings: Option<String> = row.get(11)?;
    let created_at: i64 = row.get(2)?;
    let has_audio: bool = row.get(19)?;
    let speaker_map: Option<String> = row.get(14)?;
    let diarization_state: String = row.get(15)?;
    Ok(Session {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at,
        input_mode: InputMode::parse(&mode).ok_or_else(|| bad_column(3, mode))?,
        status: status_at(row, 4)?,
        error: row.get(5)?,
        prompt_id: row.get(6)?,
        model_id: row.get(7)?,
        transcript: row.get(8)?,
        plain_transcript: row.get(12)?,
        raw_transcript: row.get(13)?,
        speaker_map: speaker_map
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, e.into())
            })?,
        diarization_state: DiarizationState::parse(&diarization_state)
            .ok_or_else(|| bad_column(15, diarization_state))?,
        summary: row.get(9)?,
        duration_ms: row.get(10)?,
        timings: timings
            .map(|t| serde_json::from_str(&t))
            .transpose()
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, e.into())
            })?,
        has_audio,
        audio_expires_at: has_audio.then_some(created_at + AUDIO_RETENTION_MS),
        previous_summary: row.get(16)?,
        previous_prompt_id: row.get(17)?,
        previous_model_id: row.get(18)?,
    })
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn samples(c: &Connection, id: &str, source: AudioSource) -> Result<Vec<i16>> {
    let mut stmt = c.prepare(
        "SELECT pcm FROM audio_chunks WHERE session_id = ?1 AND source = ?2 ORDER BY seq",
    )?;
    let mut rows = stmt.query(params![id, source.as_str()])?;
    let mut samples = Vec::new();
    while let Some(row) = rows.next()? {
        let bytes = row.get_ref(0)?.as_blob().map_err(rusqlite::Error::from)?;
        samples.extend(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b)),
        );
    }
    Ok(samples)
}

fn sweep(c: &mut Connection, now_ms: i64) -> Result<usize> {
    let cutoff = now_ms - AUDIO_RETENTION_MS;
    let tx = c.transaction()?;
    let deleted = tx.execute(
        "DELETE FROM audio_chunks WHERE session_id IN (SELECT id FROM sessions
         WHERE created_at < ?1 AND status NOT IN ('recording', 'finalizing'))",
        [cutoff],
    )?;
    tx.execute(
        "UPDATE sessions SET audio_kept = 0 WHERE audio_kept = 1 AND created_at < ?1
         AND status NOT IN ('recording', 'finalizing')",
        [cutoff],
    )?;
    tx.commit()?;
    if deleted > 0 {
        // Freed pages leave the WAL file too (R17).
        c.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    }
    Ok(deleted)
}

fn session_not_found(id: &str) -> StoreError {
    StoreError::NotFound(format!("session {id}"))
}

impl Store {
    pub fn queued_summaries(&self) -> Result<Vec<(String, String)>> {
        self.with(|c| {
            let mut s =
                c.prepare("SELECT session_id, prompt_id FROM summary_queue ORDER BY position")?;
            let rows = s
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        })
    }

    pub fn dequeue_summary(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM summary_queue WHERE session_id = ?1", [id])?;
            Ok(())
        })
    }

    pub fn queue_status(
        &self,
        id: &str,
        prompt_id: &str,
        status: SessionStatus,
        restore_status: SessionStatus,
        already_pending: bool,
    ) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            // A new job replaces a finishing job's row and takes a fresh FIFO position.
            if !already_pending {
                tx.execute("DELETE FROM summary_queue WHERE session_id = ?1", [id])?;
            }
            tx.execute("INSERT INTO summary_queue (session_id, prompt_id, restore_status) VALUES (?1, ?2, ?3) ON CONFLICT(session_id) DO UPDATE SET prompt_id = excluded.prompt_id", params![id, prompt_id, restore_status.as_str()])?;
            tx.execute("UPDATE sessions SET status = ?2 WHERE id = ?1", params![id, status.as_str()])?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Persists explicit cancellation even if the process exits before the worker does.
    pub fn cancel_running_summary(&self, id: &str) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            tx.execute("DELETE FROM summary_queue WHERE session_id = ?1", [id])?;
            tx.execute(
                "UPDATE sessions SET status = CASE WHEN error = 'interrupted' THEN 'failed' ELSE 'cancelled' END,
                    error = CASE WHEN error = 'interrupted' THEN error ELSE NULL END
                 WHERE id = ?1 AND status IN ('finalizing', 'awaitingModel', 'summarizing')",
                [id],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    /// Cancels durable queued work, restoring an earlier finished result when
    /// this was a queued re-summary.
    pub fn cancel_queued_summary(&self, id: &str) -> Result<Option<SessionStatus>> {
        self.with(|c| {
            let tx = c.transaction()?;
            let restore: Option<String> = tx
                .query_row(
                    "SELECT restore_status FROM summary_queue WHERE session_id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(restore) = restore else {
                return Ok(None);
            };
            let restore = SessionStatus::parse(&restore).ok_or_else(|| bad_column(0, restore))?;
            let status = if matches!(
                restore,
                SessionStatus::Done | SessionStatus::Cancelled | SessionStatus::Failed
            ) {
                restore
            } else {
                SessionStatus::Cancelled
            };
            tx.execute("DELETE FROM summary_queue WHERE session_id = ?1", [id])?;
            tx.execute(
                "UPDATE sessions SET status = ?2 WHERE id = ?1",
                params![id, status.as_str()],
            )?;
            tx.commit()?;
            Ok(Some(status))
        })
    }

    /// Inserts a session in `Recording` and returns it. The id comes from
    /// SQLite's CSPRNG, so no UUID crate is needed.
    pub fn create_session(&self, new: &NewSession) -> Result<Session> {
        let id: String = self.with(|c| {
            Ok(c.query_row(
                "INSERT INTO sessions (id, title, created_at, input_mode, status, prompt_id, model_id)
                 VALUES (lower(hex(randomblob(16))), ?1, ?2, ?3, ?4, ?5, ?6)
                 RETURNING id",
                params![
                    new.title,
                    now_ms(),
                    new.input_mode.as_str(),
                    SessionStatus::Recording.as_str(),
                    new.prompt_id,
                    new.model_id,
                ],
                |r| r.get(0),
            )?)
        })?;
        self.get_session(&id)?.ok_or_else(|| session_not_found(&id))
    }

    /// Newest first.
    pub fn list_sessions(&self) -> Result<Vec<SessionListItem>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT id, title, created_at, status, duration_ms, {HAS_AUDIO},
                    CASE WHEN status = 'summarizing' THEN 'Summarizing…'
                         WHEN length(COALESCE(NULLIF(summary, ''), NULLIF(transcript, ''))) > 100
                         THEN substr(COALESCE(NULLIF(summary, ''), NULLIF(transcript, '')), 1, 100) || '…'
                         ELSE COALESCE(NULLIF(summary, ''), NULLIF(transcript, ''), 'No summary available') END,
                    (status = 'failed' AND error = 'interrupted')
                 FROM sessions
                     ORDER BY created_at DESC, rowid DESC"
            ))?;
            let rows = stmt.query_map([], |r| {
                Ok(SessionListItem {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    created_at: r.get(2)?,
                    status: status_at(r, 3)?,
                    duration_ms: r.get(4)?,
                    has_audio: r.get(5)?,
                    snippet: r.get(6)?,
                    interrupted: r.get(7)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        self.with(|c| {
            Ok(c.query_row(
                &format!("SELECT {COLUMNS}, {HAS_AUDIO} FROM sessions WHERE id = ?1"),
                [id],
                from_row,
            )
            .optional()?)
        })
    }

    /// Applies the user's edits and returns the updated session.
    pub fn update_session(&self, id: &str, patch: &SessionPatch) -> Result<Session> {
        let changed = self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET
                     title      = COALESCE(?2, title),
                     transcript = COALESCE(?3, transcript),
                     summary    = COALESCE(?4, summary)
                 WHERE id = ?1",
                params![id, patch.title, patch.transcript, patch.summary],
            )?)
        })?;
        if changed == 0 {
            return Err(session_not_found(id));
        }
        self.get_session(id)?.ok_or_else(|| session_not_found(id))
    }

    /// Moves a session to `status`, recording `error` (cleared when `None`).
    pub fn set_status(&self, id: &str, status: SessionStatus, error: Option<&str>) -> Result<()> {
        let changed = self.with(|c| Ok(c.execute(
                "UPDATE sessions SET
                    status = CASE WHEN error = 'interrupted' AND ?2 = 'cancelled' THEN 'failed' ELSE ?2 END,
                    error = CASE WHEN error = 'interrupted' THEN error ELSE ?3 END
                 WHERE id = ?1",
                params![id, status.as_str(), error],
            )?))?;
        if changed == 0 {
            return Err(session_not_found(id));
        }
        Ok(())
    }

    pub fn set_summary(&self, id: &str, summary: &str, timings: &Timings) -> Result<()> {
        let timings = serde_json::to_string(timings)?;
        let changed = self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET summary = ?2, timings = ?3 WHERE id = ?1",
                params![id, summary, timings],
            )?)
        })?;
        if changed == 0 {
            return Err(session_not_found(id));
        }
        Ok(())
    }

    /// Stores a finished summary with its provenance and timings and moves
    /// the session to `Done`, all in one statement, so a reader never sees
    /// `Done` without the summary that goes with it.
    pub fn finish_summary(
        &self,
        id: &str,
        summary: &str,
        prompt_id: &str,
        model_id: &str,
        timings: &Timings,
    ) -> Result<()> {
        let timings = serde_json::to_string(timings)?;
        self.with(|c| {
            let tx = c.transaction()?;
            let recovering: bool = tx
                .query_row(
                    "SELECT COALESCE(error = 'interrupted', 0) FROM sessions WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or(false);
            let changed = tx.execute(
                "UPDATE sessions SET
                     previous_summary = CASE WHEN summary <> '' THEN summary ELSE NULL END,
                     previous_prompt_id = CASE WHEN summary <> '' THEN prompt_id ELSE NULL END,
                     previous_model_id = CASE WHEN summary <> '' THEN model_id ELSE NULL END,
                     summary = ?2, prompt_id = ?3, model_id = ?4, timings = ?5,
                     status = ?6, error = NULL
                 WHERE id = ?1",
                params![
                    id,
                    summary,
                    prompt_id,
                    model_id,
                    timings,
                    SessionStatus::Done.as_str()
                ],
            )?;
            if changed == 0 {
                return Err(session_not_found(id));
            }
            if recovering {
                retain_or_delete_audio(&tx, id)?;
            }
            tx.commit()?;
            Ok(())
        })?;
        Ok(())
    }

    pub fn undo_summary(&self, id: &str) -> Result<Session> {
        let changed = self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET summary = previous_summary,
                 prompt_id = COALESCE(previous_prompt_id, prompt_id), model_id = previous_model_id,
                 previous_summary = NULL, previous_prompt_id = NULL, previous_model_id = NULL
             WHERE id = ?1 AND previous_summary IS NOT NULL
                 AND status NOT IN ('recording', 'finalizing', 'awaitingModel', 'summarizing')",
                [id],
            )?)
        })?;
        if changed == 0 {
            let Some(session) = self.get_session(id)? else {
                return Err(session_not_found(id));
            };
            if matches!(
                session.status,
                SessionStatus::Recording
                    | SessionStatus::Finalizing
                    | SessionStatus::AwaitingModel
                    | SessionStatus::Summarizing
            ) {
                return Err(super::StoreError::Busy);
            }
            return Err(StoreError::NotFound(format!(
                "previous summary for session {id}"
            )));
        }
        self.get_session(id)?.ok_or_else(|| session_not_found(id))
    }

    pub fn clear_segments(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("DELETE FROM segments WHERE session_id = ?1", [id])?;
            Ok(())
        })
    }

    pub fn save_recovered_transcript(
        &self,
        id: &str,
        transcript: &str,
        duration_ms: u32,
        diarization: Option<(&str, &BTreeMap<String, String>)>,
    ) -> Result<()> {
        let map = diarization
            .map(|(_, map)| serde_json::to_string(map))
            .transpose()?;
        let raw = diarization.map(|(raw, _)| raw);
        self.with(|c| {
            let changed = c.execute(
                "UPDATE sessions SET transcript = ?2, plain_transcript = ?2, raw_transcript = ?3,
                    speaker_map = ?4, diarization_state = CASE WHEN ?3 IS NULL THEN 'none' ELSE 'pending' END,
                    duration_ms = ?5, status = 'failed', error = 'interrupted' WHERE id = ?1",
                params![id, transcript, raw, map, duration_ms],
            )?;
            if changed == 0 { return Err(session_not_found(id)); }
            Ok(())
        })
    }

    pub fn set_recovery_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        let changed = self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET status = ?2, error = 'interrupted' WHERE id = ?1",
                params![id, status.as_str()],
            )?)
        })?;
        if changed == 0 {
            return Err(session_not_found(id));
        }
        Ok(())
    }

    pub fn claim_recovery(&self, id: &str) -> Result<bool> {
        self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET status = 'finalizing'
             WHERE id = ?1 AND status = 'failed' AND error = 'interrupted'
             AND EXISTS (SELECT 1 FROM audio_chunks a WHERE a.session_id = sessions.id)",
                [id],
            )? > 0)
        })
    }

    pub fn fail_recovery(&self, id: &str, error: &str) -> Result<()> {
        let changed = self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET status = 'failed', error = ?2 WHERE id = ?1",
                params![id, error],
            )?)
        })?;
        if changed == 0 {
            return Err(session_not_found(id));
        }
        Ok(())
    }

    pub fn set_timings(&self, id: &str, timings: &Timings) -> Result<()> {
        let timings = serde_json::to_string(timings)?;
        let changed = self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET timings = ?2 WHERE id = ?1",
                params![id, timings],
            )?)
        })?;
        if changed == 0 {
            return Err(session_not_found(id));
        }
        Ok(())
    }

    /// Saves the final transcript and, in the same transaction, keeps or
    /// deletes the session's audio by the "Save session audio" setting as it
    /// is now (parity spec 01 R13a/b), so unkept audio never outlives a saved
    /// transcript and is never lost without one.
    pub fn save_transcript(&self, id: &str, transcript: &str, duration_ms: u32) -> Result<()> {
        self.with(|c| {
            let tx = c.transaction()?;
            let changed = tx.execute(
                "UPDATE sessions SET transcript = ?2, duration_ms = ?3 WHERE id = ?1",
                params![id, transcript, duration_ms],
            )?;
            if changed == 0 {
                return Err(session_not_found(id));
            }
            retain_or_delete_audio(&tx, id)?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn save_diarization(
        &self,
        id: &str,
        plain: &str,
        raw: &str,
        map: &BTreeMap<String, String>,
        duration_ms: u32,
    ) -> Result<()> {
        let map = serde_json::to_string(map)?;
        self.with(|c| {
            let tx = c.transaction()?;
            let changed = tx.execute(
                "UPDATE sessions SET transcript = ?2, plain_transcript = ?2, raw_transcript = ?3, speaker_map = ?4, diarization_state = 'pending', duration_ms = ?5 WHERE id = ?1",
                params![id, plain, raw, map, duration_ms],
            )?;
            if changed == 0 { return Err(session_not_found(id)); }
            retain_or_delete_audio(&tx, id)?;
            tx.commit()?;
            Ok(())
        })
    }

    pub fn update_diarization(
        &self,
        id: &str,
        transcript: &str,
        map: &BTreeMap<String, String>,
        state: DiarizationState,
        summary: Option<&str>,
    ) -> Result<()> {
        let map = serde_json::to_string(map)?;
        let changed = self.with(|c| Ok(c.execute(
            "UPDATE sessions SET transcript = ?2, speaker_map = ?3, diarization_state = ?4, summary = COALESCE(?5, summary) WHERE id = ?1",
            params![id, transcript, map, state.as_str(), summary],
        )?))?;
        if changed == 0 {
            return Err(session_not_found(id));
        }
        Ok(())
    }

    pub fn confirm_speaker_labels(
        &self,
        id: &str,
        map: &BTreeMap<String, String>,
    ) -> Result<String> {
        let session = self.get_session(id)?.ok_or_else(|| session_not_found(id))?;
        let raw = session
            .raw_transcript
            .as_deref()
            .ok_or_else(|| session_not_found(id))?;
        let transcript = crate::text::apply_speaker_map(raw, map);
        self.update_diarization(id, &transcript, map, DiarizationState::Confirmed, None)?;
        Ok(transcript)
    }

    pub fn skip_diarization(&self, id: &str) -> Result<String> {
        let session = self.get_session(id)?.ok_or_else(|| session_not_found(id))?;
        let plain = session
            .plain_transcript
            .as_deref()
            .ok_or_else(|| session_not_found(id))?;
        let transcript = plain.to_string();
        let map = session.speaker_map.unwrap_or_default();
        self.update_diarization(id, &transcript, &map, DiarizationState::Skipped, None)?;
        Ok(transcript)
    }

    pub fn update_speaker_map(&self, id: &str, map: &BTreeMap<String, String>) -> Result<String> {
        let session = self.get_session(id)?.ok_or_else(|| session_not_found(id))?;
        let raw = session
            .raw_transcript
            .as_deref()
            .ok_or_else(|| session_not_found(id))?;
        let old_map = session.speaker_map.unwrap_or_default();
        let transcript = crate::text::apply_speaker_map(raw, map);
        let summary = crate::text::substitute_speaker_names(&session.summary, &old_map, map);
        self.update_diarization(
            id,
            &transcript,
            map,
            session.diarization_state,
            Some(&summary),
        )?;
        Ok(transcript)
    }

    /// R16/R17: deletes the audio of every session that started more than
    /// [`AUDIO_RETENTION_DAYS`] before `now_ms`, kept or left by a crash, and
    /// clears `audio_kept`. Text is untouched. Recording/Finalizing rows are
    /// skipped: after `open` they can only be the live capture. Returns the
    /// number of chunk rows deleted.
    pub fn sweep_expired_audio(&self, now_ms: i64) -> Result<usize> {
        self.with(|c| sweep(c, now_ms))
    }

    /// R20: a session's kept audio for playback, sweeping first if it has
    /// expired. Rows of a session that never chose to keep audio are not
    /// playable.
    pub fn kept_audio(&self, id: &str, now_ms: i64) -> Result<KeptAudio> {
        self.with(|c| {
            let kept: Option<(bool, i64)> = c
                .query_row(
                    "SELECT audio_kept = 1, created_at FROM sessions WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((true, created_at)) = kept else {
                return Ok(KeptAudio::NotFound);
            };
            if created_at < now_ms - AUDIO_RETENTION_MS {
                sweep(c, now_ms)?;
                return Ok(KeptAudio::Expired);
            }
            let longest: Option<i64> = c.query_row(
                "SELECT MAX(n) FROM (SELECT SUM(length(pcm)) AS n FROM audio_chunks
                 WHERE session_id = ?1 GROUP BY source)",
                [id],
                |r| r.get(0),
            )?;
            match longest {
                None => return Ok(KeptAudio::NotFound),
                Some(n) if n > MAX_PLAYBACK_BYTES => return Ok(KeptAudio::TooLarge),
                Some(_) => {}
            }
            Ok(KeptAudio::Streams {
                mic: samples(c, id, AudioSource::Mic)?,
                system: samples(c, id, AudioSource::System)?,
            })
        })
    }

    /// Deletes the session; segments and audio go with it via `ON DELETE
    /// CASCADE`. Returns whether a session existed.
    pub fn delete_session(&self, id: &str) -> Result<bool> {
        self.with(|c| Ok(c.execute("DELETE FROM sessions WHERE id = ?1", [id])? > 0))
    }

    pub fn vacuum(&self) -> Result<()> {
        self.with(|c| {
            c.execute_batch("VACUUM")?;
            Ok(())
        })
    }

    pub fn add_segment(
        &self,
        id: &str,
        source: AudioSource,
        segment: &crate::text::Segment,
    ) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO segments (session_id, source, start_ms, end_ms, text)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id,
                    source.as_str(),
                    segment.start_ms,
                    segment.end_ms,
                    segment.text
                ],
            )?;
            Ok(())
        })
    }

    /// One source's segments in time order.
    pub fn segments(&self, id: &str, source: AudioSource) -> Result<Vec<crate::text::Segment>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT start_ms, end_ms, text FROM segments
                 WHERE session_id = ?1 AND source = ?2
                 ORDER BY start_ms, rowid",
            )?;
            let rows = stmt.query_map(params![id, source.as_str()], |r| {
                Ok(crate::text::Segment {
                    start_ms: r.get(0)?,
                    end_ms: r.get(1)?,
                    text: r.get(2)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    /// Stores one chunk of 16 kHz mono i16 PCM, little-endian.
    pub fn append_audio_chunk(
        &self,
        id: &str,
        source: AudioSource,
        seq: u32,
        pcm: &[i16],
    ) -> Result<()> {
        let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
        self.with(|c| {
            c.execute(
                "INSERT INTO audio_chunks (session_id, source, seq, pcm) VALUES (?1, ?2, ?3, ?4)",
                params![id, source.as_str(), seq, bytes],
            )?;
            Ok(())
        })
    }

    /// All of one source's stored audio, in chunk order.
    pub fn audio_samples(&self, id: &str, source: AudioSource) -> Result<Vec<i16>> {
        self.with(|c| samples(c, id, source))
    }

    /// Reads only the samples needed for one timestamped transcript segment.
    pub fn audio_samples_range(
        &self,
        id: &str,
        source: AudioSource,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<i16>> {
        self.audio_range_reader(id)
            .samples(source, start_ms, end_ms)
    }

    pub(crate) fn audio_range_reader(&self, id: &str) -> AudioRangeReader<'_> {
        AudioRangeReader {
            store: self,
            id: id.to_owned(),
            source: None,
            chunks: Vec::new(),
            #[cfg(test)]
            queried_chunks: 0,
            #[cfg(test)]
            decoded_chunks: 0,
        }
    }

    pub fn audio_chunk_count(&self, id: &str) -> Result<u32> {
        self.with(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*) FROM audio_chunks WHERE session_id = ?1",
                [id],
                |r| r.get(0),
            )?)
        })
    }

    /// Capture without a queued transcript is interrupted; queued work returns
    /// to AwaitingModel in its original FIFO position after a crash.
    pub(super) fn recover_interrupted(&self) -> Result<usize> {
        self.with(|c| {
            let tx = c.transaction()?;
            let resumed = tx.execute(
                "UPDATE sessions SET status = ?1, error = NULL
                 WHERE status IN (?2, ?3) AND id IN (SELECT session_id FROM summary_queue)",
                params![
                    SessionStatus::AwaitingModel.as_str(),
                    SessionStatus::Finalizing.as_str(),
                    SessionStatus::Summarizing.as_str(),
                ],
            )?;
            let interrupted = tx.execute(
                "UPDATE sessions SET status = ?1, error = ?2,
                    audio_kept = CASE WHEN EXISTS (SELECT 1 FROM audio_chunks a WHERE a.session_id = sessions.id) THEN 1 ELSE audio_kept END
                 WHERE status IN (?3, ?4)
                 AND (status = ?3 OR id NOT IN (SELECT session_id FROM summary_queue))",
                params![
                    SessionStatus::Failed.as_str(),
                    INTERRUPTED,
                    IN_FLIGHT[0].as_str(),
                    IN_FLIGHT[1].as_str(),
                ],
            )?;
            tx.commit()?;
            Ok(resumed + interrupted)
        })
    }
}

/// Reads diarization ranges with only chunks still intersecting the ordered
/// segment overlap window in memory. The reader lives for one diarization pass.
pub(crate) struct AudioRangeReader<'a> {
    store: &'a Store,
    id: String,
    source: Option<AudioSource>,
    chunks: Vec<(u64, Vec<i16>)>,
    #[cfg(test)]
    queried_chunks: usize,
    #[cfg(test)]
    decoded_chunks: usize,
}

impl AudioRangeReader<'_> {
    pub(crate) fn samples(
        &mut self,
        source: AudioSource,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<i16>> {
        let start = start_ms.max(0).saturating_mul(16) as u64;
        let end = end_ms.max(0).saturating_mul(16) as u64;
        if end <= start {
            return Ok(Vec::new());
        }
        if self.source != Some(source) {
            self.source = Some(source);
            self.chunks.clear();
        }
        let chunk_size = crate::audio::CHUNK_SAMPLES as u64;
        // Segment starts are ordered; retain only chunks that overlap this start.
        self.chunks
            .retain(|(seq, _)| (seq + 1) * chunk_size > start);
        self.store.with(|c| {
            let first = start / chunk_size;
            let last = end.saturating_sub(1) / chunk_size;
            let mut samples = Vec::with_capacity((end - start) as usize);
            for seq in first..=last {
                if !self.chunks.iter().any(|(cached, _)| *cached == seq) {
                    #[cfg(test)]
                    { self.queried_chunks += 1; }
                    let pcm = c
                        .query_row(
                            "SELECT pcm FROM audio_chunks WHERE session_id = ?1 AND source = ?2 AND seq = ?3",
                            params![self.id, source.as_str(), seq as i64],
                            |row| row.get::<_, Vec<u8>>(0),
                        )
                        .optional()?;
                    let Some(bytes) = pcm else { continue };
                    self.chunks.push((
                        seq,
                        bytes.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect(),
                    ));
                    #[cfg(test)]
                    { self.decoded_chunks += 1; }
                }
                let Some((_, chunk)) = self.chunks.iter().find(|(cached, _)| *cached == seq) else {
                    continue;
                };
                let chunk_start = seq * chunk_size;
                let from = start.saturating_sub(chunk_start).min(chunk.len() as u64) as usize;
                let to = end.saturating_sub(chunk_start).min(chunk.len() as u64) as usize;
                if from < to {
                    samples.extend_from_slice(&chunk[from..to]);
                }
            }
            Ok(samples)
        })
    }
}

fn retain_or_delete_audio(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<()> {
    if super::settings::select(tx, super::settings::KEEP_AUDIO)?.as_deref() == Some("1") {
        tx.execute("UPDATE sessions SET audio_kept = 1 WHERE id = ?1", [id])?;
    } else {
        tx.execute("DELETE FROM audio_chunks WHERE session_id = ?1", [id])?;
    }
    Ok(())
}

#[cfg(test)]
mod audio_range_tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn store() -> Store {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE audio_chunks (session_id TEXT, source TEXT, seq INTEGER, pcm BLOB, PRIMARY KEY(session_id, source, seq))",
            [],
        )
        .unwrap();
        for source in [AudioSource::Mic, AudioSource::System] {
            for seq in 0..6 {
                let pcm = vec![seq as i16; crate::audio::CHUNK_SAMPLES];
                let bytes = pcm
                    .iter()
                    .flat_map(|sample| sample.to_le_bytes())
                    .collect::<Vec<_>>();
                conn.execute(
                    "INSERT INTO audio_chunks VALUES ('session-1', ?1, ?2, ?3)",
                    params![source.as_str(), seq, bytes],
                )
                .unwrap();
            }
        }
        Store {
            conn: Mutex::new(conn),
        }
    }

    #[test]
    fn overlapping_multichunk_ranges_reuse_decodes_and_evict_outside_the_active_window() {
        let store = store();
        let mut reader = store.audio_range_reader("session-1");
        assert_eq!(
            reader.samples(AudioSource::Mic, 0, 16_000).unwrap().len(),
            256_000
        );
        assert_eq!(reader.decoded_chunks, 4);
        assert_eq!(reader.queried_chunks, 4);
        assert_eq!(
            reader
                .samples(AudioSource::Mic, 4_000, 20_000)
                .unwrap()
                .len(),
            256_000
        );
        assert_eq!(reader.decoded_chunks, 4);
        assert_eq!(reader.queried_chunks, 4);
        assert_eq!(reader.chunks.len(), 4);
        reader.samples(AudioSource::Mic, 20_000, 21_000).unwrap();
        assert_eq!(reader.decoded_chunks, 5);
        assert_eq!(reader.queried_chunks, 5);
        assert_eq!(reader.chunks.len(), 1);
    }

    #[test]
    fn source_and_reader_sessions_do_not_share_cached_audio() {
        let store = store();
        let mut reader = store.audio_range_reader("session-1");
        reader.samples(AudioSource::Mic, 0, 100).unwrap();
        reader.samples(AudioSource::System, 0, 100).unwrap();
        assert_eq!(reader.decoded_chunks, 2);
        assert_eq!(reader.queried_chunks, 2);
        assert_eq!(reader.chunks.len(), 1);

        let mut other_session = store.audio_range_reader("session-2");
        other_session.samples(AudioSource::Mic, 0, 100).unwrap();
        assert_eq!(other_session.decoded_chunks, 0);
        assert_eq!(other_session.queried_chunks, 1);
        assert!(other_session.chunks.is_empty());
    }

    #[test]
    fn cache_is_dropped_before_retention_deletes_audio() {
        let store = store();
        {
            let mut reader = store.audio_range_reader("session-1");
            reader.samples(AudioSource::Mic, 0, 100).unwrap();
            assert_eq!(reader.decoded_chunks, 1);
        }
        store
            .with(|conn| {
                conn.execute(
                    "DELETE FROM audio_chunks WHERE session_id = 'session-1'",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        let mut reader = store.audio_range_reader("session-1");
        assert!(reader.samples(AudioSource::Mic, 0, 100).unwrap().is_empty());
        assert_eq!(reader.decoded_chunks, 0);
    }
}
