//! KTD7 session state machine: which status moves are legal.

use crate::store::SessionStatus;

use SessionStatus::*;

/// Whether a session may move from `from` to `to`. Finished sessions
/// (`Done`, `Cancelled`, `Failed`) re-enter `Summarizing` on regenerate.
pub fn can_transition(from: SessionStatus, to: SessionStatus) -> bool {
    matches!(
        (from, to),
        (Recording, Finalizing | Failed)
            | (Finalizing, AwaitingModel | Summarizing | Cancelled | Failed)
            | (AwaitingModel, Summarizing | Cancelled | Failed)
            | (Summarizing, Done | Cancelled | Failed)
            | (Done | Cancelled | Failed, Summarizing)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regenerate_only_from_a_finished_session() {
        for from in [Done, Cancelled, Failed, Finalizing, AwaitingModel] {
            assert!(can_transition(from, Summarizing), "{from:?}");
        }
        for from in [Recording, Summarizing] {
            assert!(!can_transition(from, Summarizing), "{from:?}");
        }
    }

    #[test]
    fn summarizing_ends_in_a_terminal_state() {
        for to in [Done, Cancelled, Failed] {
            assert!(can_transition(Summarizing, to));
        }
        assert!(!can_transition(Summarizing, Recording));
        assert!(!can_transition(Done, Done));
    }
}
