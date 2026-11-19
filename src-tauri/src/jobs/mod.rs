//! Summarization jobs (KTD7): an in-memory FIFO queue with one worker, the
//! session state machine, and the progress events the UI listens to.

mod queue;
mod retention;
mod session;
pub mod state;
mod summarize;

use serde::Serialize;
use ts_rs::TS;

use crate::store::{AudioSource, SessionStatus};
use crate::stt::TranscriptSegment;

pub(crate) use queue::PracticeJob;
pub use queue::{JobError, JobQueue};
pub use retention::{spawn_retention, SWEEP_EVERY};
pub use session::{RecordError, Recorder};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum RecordingStatus {
    Running,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RecordingState {
    pub session_id: String,
    pub status: RecordingStatus,
    pub active_elapsed_ms: u32,
    pub target_app: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum HearingState {
    Silent,
    Hearing,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionLevel {
    pub session_id: String,
    pub source: AudioSource,
    pub level: f32,
    pub hearing: HearingState,
    pub active_elapsed_ms: u32,
}

/// Payload of `session://progress`. The session's status row already says
/// `state` by the time this is emitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionProgress {
    pub session_id: String,
    pub state: SessionStatus,
    /// 1-based; Electron's `stepCount` is `1 + chunks + (chunks > 1 ? 1 : 0)`.
    pub step_index: u32,
    pub step_count: u32,
    pub message: String,
}

/// Payload of `session://summary-delta`: text of the final pass (the only
/// chunk, or the combine pass) as it decodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SummaryDelta {
    pub session_id: String,
    pub text: String,
}

/// Payload of `session://segments`: one source's new transcript segments,
/// in milliseconds from the start of the recording.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SessionSegments {
    pub session_id: String,
    pub source: AudioSource,
    pub segments: Vec<TranscriptSegment>,
}

/// Whether a recording's live transcription is waiting for summary work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptWaiting {
    pub session_id: String,
    pub waiting: bool,
}

/// Where jobs report; the app emits Tauri events, tests record.
pub trait JobEvents: Send + Sync {
    fn recording_starting(&self) {}
    fn summary_queue_changed(&self) {}
    /// Called under the recording lock to keep tray updates in transition order.
    fn recording_changed(&self, _recording: bool, _multi_speaker: bool) {}
    fn recording_state(&self, _: Option<RecordingState>) {}
    fn level(&self, _: SessionLevel) {}
    fn progress(&self, progress: SessionProgress);
    fn summary_delta(&self, delta: SummaryDelta);
    fn segments(&self, segments: SessionSegments);
    fn transcript_waiting(&self, _: TranscriptWaiting) {}
}
