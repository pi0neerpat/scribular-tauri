//! Sessions, their transcript segments, and their raw audio: kept until the
//! transcript is saved, or for 7 days when the user keeps audio (parity spec 01).

use std::time::{SystemTime, UNIX_EPOCH};

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
    pub summary: String,
    pub duration_ms: Option<u32>,
    pub timings: Option<Timings>,
    /// Kept audio exists and can be played.
    pub has_audio: bool,
    /// When the sweep deletes the kept audio, in ms since the Unix epoch.
    #[ts(type = "number | null")]
    pub audio_expires_at: Option<i64>,
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
                       transcript, summary, duration_ms, timings";

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
    let has_audio: bool = row.get(12)?;
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
                "UPDATE sessions SET status = 'cancelled', error = NULL
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
                "SELECT id, title, created_at, status, duration_ms, {HAS_AUDIO} FROM sessions
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
        let changed = self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET status = ?2, error = ?3 WHERE id = ?1",
                params![id, status.as_str(), error],
            )?)
        })?;
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
        let changed = self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET summary = ?2, prompt_id = ?3, model_id = ?4, timings = ?5,
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
            if super::settings::select(&tx, super::settings::KEEP_AUDIO)?.as_deref() == Some("1") {
                tx.execute("UPDATE sessions SET audio_kept = 1 WHERE id = ?1", [id])?;
            } else {
                tx.execute("DELETE FROM audio_chunks WHERE session_id = ?1", [id])?;
            }
            tx.commit()?;
            Ok(())
        })
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
                "UPDATE sessions SET status = ?1, error = ?2
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
