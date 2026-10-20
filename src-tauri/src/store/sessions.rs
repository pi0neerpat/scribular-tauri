//! Sessions, their transcript segments, and the raw audio kept until the
//! transcript is saved (KTD2).

use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, OptionalExtension, Row};
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
    /// Microphone plus system audio, diarized by source.
    MicAndSystem,
}

impl InputMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Mic => "mic",
            Self::MicAndSystem => "micAndSystem",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "mic" => Some(Self::Mic),
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
    fn as_str(self) -> &'static str {
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

/// Statuses a crash can strand; reopening turns them into `Failed`.
const IN_FLIGHT: [SessionStatus; 2] = [SessionStatus::Recording, SessionStatus::Finalizing];
pub const INTERRUPTED: &str = "interrupted";

const COLUMNS: &str = "id, title, created_at, input_mode, status, error, prompt_id, model_id, \
                       transcript, summary, duration_ms, timings";

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
    Ok(Session {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
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
    })
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn session_not_found(id: &str) -> StoreError {
    StoreError::NotFound(format!("session {id}"))
}

impl Store {
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
            let mut stmt = c.prepare(
                "SELECT id, title, created_at, status, duration_ms FROM sessions
                 ORDER BY created_at DESC, rowid DESC",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok(SessionListItem {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    created_at: r.get(2)?,
                    status: status_at(r, 3)?,
                    duration_ms: r.get(4)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<_>>()?)
        })
    }

    pub fn get_session(&self, id: &str) -> Result<Option<Session>> {
        self.with(|c| {
            Ok(c.query_row(
                &format!("SELECT {COLUMNS} FROM sessions WHERE id = ?1"),
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

    /// Saves the final transcript and deletes the session's audio in the
    /// same transaction (KTD2), so audio never outlives a saved transcript
    /// and is never lost without one.
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
            tx.execute("DELETE FROM audio_chunks WHERE session_id = ?1", [id])?;
            tx.commit()?;
            Ok(())
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
        self.with(|c| {
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
        })
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

    /// KTD2 launch recovery: a session still `Recording` or `Finalizing`
    /// means the app died mid-capture. It becomes `Failed("interrupted")`
    /// and keeps its audio so U10 can offer to transcribe it.
    pub(super) fn recover_interrupted(&self) -> Result<usize> {
        self.with(|c| {
            Ok(c.execute(
                "UPDATE sessions SET status = ?1, error = ?2 WHERE status IN (?3, ?4)",
                params![
                    SessionStatus::Failed.as_str(),
                    INTERRUPTED,
                    IN_FLIGHT[0].as_str(),
                    IN_FLIGHT[1].as_str(),
                ],
            )?)
        })
    }
}
