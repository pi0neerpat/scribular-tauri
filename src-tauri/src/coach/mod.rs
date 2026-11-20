pub(crate) mod commands;
mod extract;
mod interpret;
pub(crate) mod practice;
pub(crate) mod session;
mod spec;

use std::sync::{atomic::AtomicBool, mpsc, Arc, Mutex, MutexGuard};

use serde::Serialize;
use ts_rs::TS;

use crate::{models::SummaryAvailability, store::Prompt};

pub(crate) use commands::{discard, is_open, refresh_availability, signal};
pub(crate) use extract::{CoachQuestion, CoachSample};
pub use spec::{compile, StyleSpec};

use extract::Uncertain;
use practice::PracticeNote;

pub const AVAILABILITY_EVENT: &str = "coach-availability";
pub const PROGRESS_EVENT: &str = "coach-progress";
pub const RESULT_EVENT: &str = "coach-result";
pub const CANCELLED_EVENT: &str = "coach-cancelled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CoachBlock {
    Recording,
    AccessDenied,
    Busy,
    Downloading,
    Unsupported,
    LoadFailed,
}

impl CoachBlock {
    pub fn message(self) -> &'static str {
        match self {
            Self::Recording => "The coach isn't available while you're recording.",
            Self::AccessDenied => "License ended · Coach unavailable.",
            Self::Busy => "The coach opens when your other notes are done.",
            Self::Downloading => "Summaries are downloading. Coach unavailable.",
            Self::Unsupported => {
                "Summaries aren't available on this Mac. Recording and transcription still work."
            }
            Self::LoadFailed => "Summaries couldn't load. Recording and transcription still work.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CoachRetry {
    License,
    TryAgain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CoachAvailability {
    reason: Option<CoachBlock>,
    pub(super) message: Option<&'static str>,
    session_message: Option<&'static str>,
    retry: Option<CoachRetry>,
}

impl CoachAvailability {
    fn from_reason(reason: Option<CoachBlock>) -> Self {
        use CoachBlock::*;
        Self {
            reason,
            message: reason.map(CoachBlock::message),
            session_message: reason.map(|reason| match reason {
                Recording => {
                    "Recording in progress · The coach isn’t available while you’re recording."
                }
                Unsupported => "Summaries unavailable · Coach unavailable.",
                LoadFailed => "Summaries couldn't load · Coach unavailable.",
                _ => reason.message(),
            }),
            retry: reason.and_then(|reason| match reason {
                AccessDenied => Some(CoachRetry::License),
                LoadFailed => Some(CoachRetry::TryAgain),
                _ => None,
            }),
        }
    }

    fn current(recording: bool, access: bool, busy: bool, summary: SummaryAvailability) -> Self {
        use CoachBlock::*;
        let reason = if recording {
            Some(Recording)
        } else if !access {
            Some(AccessDenied)
        } else if busy {
            Some(Busy)
        } else {
            match summary {
                SummaryAvailability::Ready => None,
                SummaryAvailability::Downloading => Some(Downloading),
                SummaryAvailability::Unsupported => Some(Unsupported),
                SummaryAvailability::LoadFailed => Some(LoadFailed),
            }
        };
        Self::from_reason(reason)
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CoachProposal {
    id: String,
    change: String,
    note: PracticeNote,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CoachDraft {
    id: String,
    name: String,
    is_adjustment: bool,
    accepted_change: bool,
    spec: Option<StyleSpec>,
    question: Option<CoachQuestion>,
    note: Option<PracticeNote>,
    proposal: Option<CoachProposal>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CoachReply {
    Answer {
        draft: CoachDraft,
        message: String,
    },
    Question {
        question: CoachQuestion,
    },
    Unsupported {
        suggestions: Vec<String>,
    },
    Writing {
        #[serde(rename = "jobId")]
        job_id: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CoachProgress {
    draft_id: String,
    job_id: Option<String>,
    attempt: u8,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CoachResult {
    job_id: String,
    draft: CoachDraft,
    error: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CoachCancelReason {
    Cancelled,
    Recording,
    AccessDenied,
    Closed,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CoachCancelled {
    draft_id: String,
    reason: CoachCancelReason,
}

pub struct Coach {
    draft: Mutex<Option<Draft>>,
    updates: mpsc::SyncSender<()>,
}

struct Draft {
    id: String,
    original: Option<Prompt>,
    spec: Option<StyleSpec>,
    questions: Vec<Uncertain>,
    reading: bool,
    accepted_change: bool,
    note: Option<PracticeNote>,
    proposal: Option<(CoachProposal, StyleSpec)>,
    writing: Option<Writing>,
}

struct Writing {
    id: String,
    cancel: Arc<AtomicBool>,
}

impl Coach {
    pub fn new() -> (Self, mpsc::Receiver<()>) {
        let (updates, changed) = mpsc::sync_channel(1);
        (
            Self {
                draft: Mutex::default(),
                updates,
            },
            changed,
        )
    }

    fn signal(&self) {
        let _ = self.updates.try_send(());
    }

    fn lock(&self) -> MutexGuard<'_, Option<Draft>> {
        self.draft.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl Draft {
    fn new(id: String, original: Option<Prompt>) -> Self {
        Self {
            id,
            spec: original.as_ref().and_then(|p| p.style_spec.clone()),
            original,
            questions: Vec::new(),
            reading: false,
            accepted_change: false,
            note: None,
            proposal: None,
            writing: None,
        }
    }

    fn snapshot(&self) -> CoachDraft {
        CoachDraft {
            id: self.id.clone(),
            name: self
                .original
                .as_ref()
                .map_or("My note style", |p| &p.name)
                .into(),
            is_adjustment: self.original.is_some(),
            accepted_change: self.accepted_change,
            spec: self.spec.clone(),
            question: self
                .questions
                .first()
                .zip(self.spec.as_ref())
                .map(|(question, spec)| question.question(spec)),
            note: self.note.clone(),
            proposal: self.proposal.as_ref().map(|(proposal, _)| proposal.clone()),
        }
    }
}

#[cfg(test)]
pub(crate) fn test_style() -> StyleSpec {
    let (mut spec, _) = extract::extract(extract::SAMPLES[1].text).unwrap();
    spec.sentence_min = 1;
    spec.sentence_max = 1;
    spec
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate_precedence_and_retry_are_shared() {
        use CoachBlock::*;
        use SummaryAvailability as S;
        for (recording, access, busy, summary, reason, retry) in [
            (true, false, true, S::LoadFailed, Some(Recording), None),
            (
                false,
                false,
                true,
                S::Ready,
                Some(AccessDenied),
                Some(CoachRetry::License),
            ),
            (false, true, true, S::Unsupported, Some(Busy), None),
            (
                false,
                true,
                false,
                S::Downloading,
                Some(CoachBlock::Downloading),
                None,
            ),
            (
                false,
                true,
                false,
                S::Unsupported,
                Some(CoachBlock::Unsupported),
                None,
            ),
            (
                false,
                true,
                false,
                S::LoadFailed,
                Some(CoachBlock::LoadFailed),
                Some(CoachRetry::TryAgain),
            ),
            (false, true, false, S::Ready, None, None),
        ] {
            let state = CoachAvailability::current(recording, access, busy, summary);
            assert_eq!(state.reason, reason);
            assert_eq!(state.message, reason.map(CoachBlock::message));
            assert_eq!(state.retry, retry);
        }
    }
}
