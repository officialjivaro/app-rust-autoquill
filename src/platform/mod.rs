//! Platform boundary for foreground-window validation, native input, and global activation keys.

use std::{error::Error, fmt};

use crate::{
    domain::{Shortcut, ShortcutKey},
    typing::{Instruction, SpecialKey},
};

#[cfg(not(windows))]
use crate::domain::TargetIntent;

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
                "The captured target changed or lost required focus, so Real Typing stopped immediately. Refocus the destination and start again.",
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
    root_handle: usize,
    input_handle: usize,
    root_process_id: u32,
    input_process_id: u32,
    root_class: String,
    input_class: String,
    label: String,
    delivery: DeliveryStrategy,
}

impl ForegroundTarget {
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub const fn delivery(&self) -> DeliveryStrategy {
        self.delivery
    }

    #[must_use]
    pub const fn delivery_label(&self) -> &'static str {
        self.delivery.label()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStrategy {
    ForegroundProtected,
    NativeBackground,
}

impl DeliveryStrategy {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ForegroundProtected => "FOREGROUND PROTECTED",
            Self::NativeBackground => "STICKY BACKGROUND",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    EmergencyStop,
    Registered(Shortcut),
    RegistrationFailed {
        shortcut: Shortcut,
        retained_shortcut: Option<Shortcut>,
        message: String,
    },
    EscapeRegistrationFailed(String),
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

    pub fn capture(
        &self,
        _intent: TargetIntent,
        _instructions: &[Instruction],
    ) -> Result<ForegroundTarget, NativeInputError> {
        Err(NativeInputError::Unsupported)
    }

    pub fn capture_foreground(&self) -> Result<ForegroundTarget, NativeInputError> {
        self.capture(TargetIntent::Foreground, &[])
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
        _shortcut: Shortcut,
    ) -> Result<(Self, std::sync::mpsc::Receiver<HotkeyEvent>), NativeInputError> {
        Err(NativeInputError::Unsupported)
    }

    pub fn set_shortcut(&self, _shortcut: Shortcut) -> Result<(), NativeInputError> {
        Err(NativeInputError::Unsupported)
    }

    pub fn set_escape_active(&self, _active: bool) -> Result<(), NativeInputError> {
        Err(NativeInputError::Unsupported)
    }
}

#[must_use]
pub fn document_uses_activation_shortcut(instructions: &[Instruction], shortcut: Shortcut) -> bool {
    let Shortcut {
        modifiers,
        key: ShortcutKey::Function(function_key),
    } = shortcut
    else {
        return false;
    };
    if !modifiers.is_empty() {
        return false;
    }

    instructions.iter().any(|instruction| {
        matches!(
            instruction,
            Instruction::SpecialKey(SpecialKey::Function(number)) if *number == function_key
        )
    })
}

#[must_use]
pub(crate) fn instructions_support_background_delivery(instructions: &[Instruction]) -> bool {
    instructions.iter().all(|instruction| match instruction {
        Instruction::Character(_) => true,
        Instruction::SpecialKey(key) => matches!(
            key,
            SpecialKey::Enter
                | SpecialKey::Backspace
                | SpecialKey::Space
                | SpecialKey::Delete
                | SpecialKey::Home
                | SpecialKey::End
                | SpecialKey::PageUp
                | SpecialKey::PageDown
                | SpecialKey::Up
                | SpecialKey::Down
                | SpecialKey::Left
                | SpecialKey::Right
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ModifierSet;

    #[test]
    fn detects_activation_key_inside_document() {
        let instructions = vec![
            Instruction::Character('A'),
            Instruction::SpecialKey(SpecialKey::Function(4)),
        ];
        assert!(document_uses_activation_shortcut(
            &instructions,
            Shortcut::new(ModifierSet::default(), ShortcutKey::Function(4)).unwrap()
        ));
        assert!(!document_uses_activation_shortcut(
            &instructions,
            Shortcut::new(ModifierSet::default(), ShortcutKey::Function(5)).unwrap()
        ));
        assert!(!document_uses_activation_shortcut(
            &instructions,
            Shortcut::new(
                ModifierSet {
                    control: true,
                    ..ModifierSet::default()
                },
                ShortcutKey::Function(4),
            )
            .unwrap()
        ));
    }

    #[test]
    fn background_delivery_rejects_unsafe_special_keys() {
        assert!(instructions_support_background_delivery(&[
            Instruction::Character('A'),
            Instruction::SpecialKey(SpecialKey::Left),
        ]));
        assert!(!instructions_support_background_delivery(&[
            Instruction::SpecialKey(SpecialKey::Tab),
        ]));
    }
}
