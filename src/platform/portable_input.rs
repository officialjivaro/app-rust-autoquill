//! Shared macOS/X11 input translation. Runtime target validation remains platform-specific.

use std::{cell::RefCell, fmt};

use enigo::{Direction, Enigo, Key, Keyboard, Settings};

use crate::typing::{Instruction, PreviewOperation, SpecialKey};

use super::NativeInputError;

#[derive(Default)]
pub(super) struct NativeKeyboard {
    input: RefCell<Option<Enigo>>,
}

impl fmt::Debug for NativeKeyboard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeKeyboard")
            .field("initialized", &self.input.borrow().is_some())
            .finish()
    }
}

impl NativeKeyboard {
    pub(super) fn ensure_ready(&self) -> Result<(), NativeInputError> {
        if self.input.borrow().is_some() {
            return Ok(());
        }

        let settings = Settings {
            open_prompt_to_get_permissions: false,
            ..Settings::default()
        };
        let input = Enigo::new(&settings).map_err(|error| {
            NativeInputError::InputFailed(format!(
                "The native input backend could not initialize ({error}). Simulation remains available."
            ))
        })?;
        *self.input.borrow_mut() = Some(input);
        Ok(())
    }

    pub(super) fn emit(&self, operation: &PreviewOperation) -> Result<(), NativeInputError> {
        self.ensure_ready()?;
        let mut input = self.input.borrow_mut();
        let input = input.as_mut().ok_or_else(|| {
            NativeInputError::InputFailed(
                "The native input backend was not initialized. Real Typing stopped.".into(),
            )
        })?;

        match operation {
            PreviewOperation::Intended(Instruction::Character(character))
            | PreviewOperation::TypoCharacter(character) => {
                let mut encoded = [0_u8; 4];
                input
                    .text(character.encode_utf8(&mut encoded))
                    .map_err(input_error)
            }
            PreviewOperation::Intended(Instruction::SpecialKey(key)) => emit_special(input, *key),
            PreviewOperation::CorrectionBackspace => input
                .key(Key::Backspace, Direction::Click)
                .map_err(input_error),
        }
    }
}

fn emit_special(input: &mut Enigo, special: SpecialKey) -> Result<(), NativeInputError> {
    let key = special_key(special).ok_or_else(|| {
        NativeInputError::InputFailed(format!(
            "Special key {special:?} is not safely mapped by this experimental native backend. Real Typing stopped before emitting it."
        ))
    })?;
    input.key(key, Direction::Click).map_err(input_error)
}

fn input_error(error: enigo::InputError) -> NativeInputError {
    NativeInputError::InputFailed(format!(
        "The native input backend rejected an event ({error}). Real Typing stopped immediately."
    ))
}

fn special_key(key: SpecialKey) -> Option<Key> {
    match key {
        SpecialKey::Enter => Some(Key::Return),
        SpecialKey::Tab => Some(Key::Tab),
        SpecialKey::Backspace => Some(Key::Backspace),
        SpecialKey::Space => Some(Key::Space),
        SpecialKey::Escape => Some(Key::Escape),
        SpecialKey::Control => Some(Key::Control),
        SpecialKey::Shift => Some(Key::Shift),
        SpecialKey::Alt => Some(Key::Alt),
        SpecialKey::CapsLock => Some(Key::CapsLock),
        SpecialKey::Delete => Some(Key::Delete),
        SpecialKey::Home => Some(Key::Home),
        SpecialKey::End => Some(Key::End),
        SpecialKey::PageUp => Some(Key::PageUp),
        SpecialKey::PageDown => Some(Key::PageDown),
        SpecialKey::Up => Some(Key::UpArrow),
        SpecialKey::Down => Some(Key::DownArrow),
        SpecialKey::Left => Some(Key::LeftArrow),
        SpecialKey::Right => Some(Key::RightArrow),
        SpecialKey::LeftWindows | SpecialKey::RightWindows => Some(Key::Meta),
        SpecialKey::Function(number) => function_key(number),
        SpecialKey::NumpadDigit(number) => numpad_key(number),
        SpecialKey::NumpadMultiply => Some(Key::Multiply),
        SpecialKey::NumpadAdd => Some(Key::Add),
        SpecialKey::NumpadSubtract => Some(Key::Subtract),
        SpecialKey::NumpadDecimal => Some(Key::Decimal),
        SpecialKey::NumpadDivide => Some(Key::Divide),
        SpecialKey::NumLock => linux_only_key(LinuxOnlyKey::NumLock),
        SpecialKey::ScrollLock => linux_only_key(LinuxOnlyKey::ScrollLock),
        SpecialKey::Pause => linux_only_key(LinuxOnlyKey::Pause),
        SpecialKey::Insert => linux_only_key(LinuxOnlyKey::Insert),
        SpecialKey::PrintScreen => linux_only_key(LinuxOnlyKey::PrintScreen),
        SpecialKey::Applications => None,
    }
}

#[derive(Debug, Clone, Copy)]
enum LinuxOnlyKey {
    NumLock,
    ScrollLock,
    Pause,
    Insert,
    PrintScreen,
}

fn linux_only_key(key: LinuxOnlyKey) -> Option<Key> {
    #[cfg(target_os = "linux")]
    {
        return Some(match key {
            LinuxOnlyKey::NumLock => Key::Numlock,
            LinuxOnlyKey::ScrollLock => Key::ScrollLock,
            LinuxOnlyKey::Pause => Key::Pause,
            LinuxOnlyKey::Insert => Key::Insert,
            LinuxOnlyKey::PrintScreen => Key::PrintScr,
        });
    }

    #[cfg(target_os = "macos")]
    {
        let _ = key;
        None
    }
}

fn function_key(number: u8) -> Option<Key> {
    Some(match number {
        1 => Key::F1,
        2 => Key::F2,
        3 => Key::F3,
        4 => Key::F4,
        5 => Key::F5,
        6 => Key::F6,
        7 => Key::F7,
        8 => Key::F8,
        9 => Key::F9,
        10 => Key::F10,
        11 => Key::F11,
        12 => Key::F12,
        _ => return None,
    })
}

fn numpad_key(number: u8) -> Option<Key> {
    Some(match number {
        0 => Key::Numpad0,
        1 => Key::Numpad1,
        2 => Key::Numpad2,
        3 => Key::Numpad3,
        4 => Key::Numpad4,
        5 => Key::Numpad5,
        6 => Key::Numpad6,
        7 => Key::Numpad7,
        8 => Key::Numpad8,
        9 => Key::Numpad9,
        _ => return None,
    })
}
