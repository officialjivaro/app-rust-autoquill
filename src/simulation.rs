//! Deterministic, side-effect-free preview of compiled instructions.

use crate::{domain::SessionState, typing::Compilation};

#[derive(Debug, Clone, PartialEq)]
pub struct SimulationUpdate {
    pub state: SessionState,
    pub progress: f32,
    pub preview_text: String,
    pub current_action: String,
    pub completed_instructions: usize,
    pub total_instructions: usize,
}

/// A UI-agnostic controller that cannot call any platform input API.
#[derive(Debug, Default)]
pub struct SimulationController {
    state: SessionState,
    instructions: Vec<crate::typing::Instruction>,
    cursor: usize,
    preview_text: String,
}

impl SimulationController {
    pub fn start(&mut self, compilation: Compilation) -> SimulationUpdate {
        self.instructions = compilation.instructions;
        self.cursor = 0;
        self.preview_text.clear();
        self.state = if self.instructions.is_empty() {
            SessionState::Completed
        } else {
            SessionState::Typing
        };
        self.snapshot(if self.instructions.is_empty() {
            "Nothing to preview"
        } else {
            "Ready"
        })
    }

    pub fn tick(&mut self) -> SimulationUpdate {
        if self.state != SessionState::Typing {
            return self.snapshot("Waiting");
        }

        let instruction = self.instructions[self.cursor].clone();
        self.preview_text.push_str(&instruction.preview_fragment());
        self.cursor += 1;
        if self.cursor == self.instructions.len() {
            self.state = SessionState::Completed;
        }
        self.snapshot(&instruction.action_label())
    }

    pub fn stop(&mut self) -> SimulationUpdate {
        if self.state == SessionState::Typing {
            self.state = SessionState::Idle;
        }
        self.snapshot("Stopped safely")
    }

    #[must_use]
    pub const fn is_running(&self) -> bool {
        matches!(self.state, SessionState::Typing)
    }

    fn snapshot(&self, current_action: &str) -> SimulationUpdate {
        let total = self.instructions.len();
        let progress = if total == 0 {
            if self.state == SessionState::Completed {
                1.0
            } else {
                0.0
            }
        } else {
            self.cursor as f32 / total as f32
        };
        SimulationUpdate {
            state: self.state,
            progress,
            preview_text: self.preview_text.clone(),
            current_action: current_action.into(),
            completed_instructions: self.cursor,
            total_instructions: total,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typing::{Instruction, SpecialKey};

    fn sample_compilation() -> Compilation {
        Compilation {
            expanded_text: "a[ENTER]".into(),
            instructions: vec![
                Instruction::Character('a'),
                Instruction::SpecialKey(SpecialKey::Enter),
            ],
            warnings: Vec::new(),
        }
    }

    #[test]
    fn simulation_progresses_and_completes_without_external_effects() {
        let mut controller = SimulationController::default();
        assert_eq!(controller.start(sample_compilation()).progress, 0.0);
        let first = controller.tick();
        assert_eq!(first.preview_text, "a");
        assert_eq!(first.progress, 0.5);
        let second = controller.tick();
        assert_eq!(second.preview_text, "a‹ENTER›");
        assert_eq!(second.progress, 1.0);
        assert_eq!(second.state, SessionState::Completed);
    }

    #[test]
    fn stop_is_immediate_and_restart_discards_old_progress() {
        let mut controller = SimulationController::default();
        controller.start(sample_compilation());
        controller.tick();
        let stopped = controller.stop();
        assert_eq!(stopped.state, SessionState::Idle);
        assert!(!controller.is_running());
        let restarted = controller.start(sample_compilation());
        assert_eq!(restarted.preview_text, "");
        assert_eq!(restarted.completed_instructions, 0);
    }
}
