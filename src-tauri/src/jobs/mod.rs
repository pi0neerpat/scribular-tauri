//! Summarization jobs (KTD7): an in-memory FIFO queue with one worker, the
//! session state machine, and the progress events the UI listens to.

mod queue;
mod session;
pub mod state;
mod summarize;

use serde::Serialize;
use ts_rs::TS;

use crate::store::{AudioSource, SessionStatus};
use crate::stt::TranscriptSegment;

pub use queue::{JobError, JobQueue};
pub use session::{RecordError, Recorder};

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

/// Where jobs report; the app emits Tauri events, tests record.
pub trait JobEvents: Send + Sync {
    fn progress(&self, progress: SessionProgress);
    fn summary_delta(&self, delta: SummaryDelta);
    fn segments(&self, segments: SessionSegments);
}
