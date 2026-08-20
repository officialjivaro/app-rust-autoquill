//! Session state shared by the controller and user interface.

/// The complete high-level lifecycle of one typing session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SessionState {
    /// No session is active.
    #[default]
    Idle,
    /// Settings and the requested target are being validated.
    Preparing,
    /// The optional startup countdown is active.
    Countdown,
    /// Instructions are being processed.
    Typing,
    /// A configured loop delay is active before the next pass.
    LoopWait,
    /// A user-paused session that can resume.
    Paused,
    /// Cancellation has been requested and the worker is winding down.
    Stopping,
    /// The requested instructions completed successfully.
    Completed,
    /// A terminal validation, target, or input failure occurred.
    Failed,
}

impl SessionState {
    /// Return whether this state belongs to a session that still owns a worker.
    #[must_use]
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Preparing
                | Self::Countdown
                | Self::Typing
                | Self::LoopWait
                | Self::Paused
                | Self::Stopping
        )
    }

    /// Return whether this is a terminal result for a completed worker.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::SessionState;

    #[test]
    fn active_and_terminal_states_are_unambiguous() {
        assert!(SessionState::Typing.is_active());
        assert!(SessionState::LoopWait.is_active());
        assert!(SessionState::Paused.is_active());
        assert!(!SessionState::Idle.is_active());
        assert!(SessionState::Completed.is_terminal());
        assert!(SessionState::Failed.is_terminal());
        assert!(!SessionState::Stopping.is_terminal());
    }
}
