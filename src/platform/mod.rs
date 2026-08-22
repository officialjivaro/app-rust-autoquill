//! Platform boundary for foreground-window validation, native input, and global activation keys.

use std::{error::Error, fmt};

use crate::typing::{Instruction, SpecialKey};

#[cfg(not(windows))]
use crate::typing::PreviewOperation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeInputError {
    Unsupported,
    NoTarget,
    OwnWindow,
    TargetClosed,
    TargetChanged,
    HotkeyUnavailable(String),
    InputFailed(String),
}

impl fmt::Display for NativeInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => formatter.write_str(
                "Real Typing is currently available on Windows only. Simulation remains available.",
            ),
            Self::NoTarget => formatter.write_str(
                "No foreground target was found. Focus the destination app and press the activation key again.",
            ),
            Self::OwnWindow => formatter.write_str(
                "AutoQuill cannot type into itself. Focus the destination app and press the activation key again.",
            ),
            Self::TargetClosed => {
                formatter.write_str("The target window closed, so Real Typing stopped immediately.")
            }
            Self::TargetChanged => formatter.write_str(
                "Foreground focus changed, so Real Typing stopped immediately. Refocus the destination and start again.",
            ),
            Self::HotkeyUnavailable(message) | Self::InputFailed(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl Error for NativeInputError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundTarget {
    raw_handle: usize,
    process_id: u32,
    label: String,
}

impl ForegroundTarget {
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Registered(u8),
    RegistrationFailed {
        key: u8,
        retained_key: Option<u8>,
        message: String,
    },
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{ForegroundBackend, HotkeyService};

#[cfg(not(windows))]
#[derive(Debug, Default)]
pub struct ForegroundBackend;

#[cfg(not(windows))]
impl ForegroundBackend {
    #[must_use]
    pub const fn available() -> bool {
        false
    }

    pub fn capture_foreground(&self) -> Result<ForegroundTarget, NativeInputError> {
        Err(NativeInputError::Unsupported)
    }

    pub fn validate(&self, _target: &ForegroundTarget) -> Result<(), NativeInputError> {
        Err(NativeInputError::Unsupported)
    }

    pub fn emit(
        &self,
        _target: &ForegroundTarget,
        _operation: &PreviewOperation,
    ) -> Result<(), NativeInputError> {
        Err(NativeInputError::Unsupported)
    }
}

#[cfg(not(windows))]
#[derive(Debug)]
pub struct HotkeyService;

#[cfg(not(windows))]
impl HotkeyService {
    pub fn start(
        _function_key: u8,
    ) -> Result<(Self, std::sync::mpsc::Receiver<HotkeyEvent>), NativeInputError> {
        Err(NativeInputError::Unsupported)
    }

    pub fn set_key(&self, _function_key: u8) -> Result<(), NativeInputError> {
        Err(NativeInputError::Unsupported)
    }
}

#[must_use]
pub fn document_uses_activation_key(instructions: &[Instruction], function_key: u8) -> bool {
    instructions.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::SpecialKey(SpecialKey::Function(number)) if *number == function_key
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_activation_key_inside_document() {
        let instructions = vec![
            Instruction::Character('A'),
            Instruction::SpecialKey(SpecialKey::Function(4)),
        ];
        assert!(document_uses_activation_key(&instructions, 4));
        assert!(!document_uses_activation_key(&instructions, 5));
    }
}
