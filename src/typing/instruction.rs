//! Portable instructions produced by the text compiler.

use std::fmt;

/// A canonical non-text key understood by AutoQuill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpecialKey {
    Enter,
    Tab,
    Backspace,
    Space,
    Escape,
    Control,
    Shift,
    Alt,
    CapsLock,
    NumLock,
    ScrollLock,
    Pause,
    Insert,
    Delete,
    PrintScreen,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Up,
    Right,
    Down,
    LeftWindows,
    RightWindows,
    Applications,
    Function(u8),
    NumpadDigit(u8),
    NumpadMultiply,
    NumpadAdd,
    NumpadSubtract,
    NumpadDecimal,
    NumpadDivide,
}

impl SpecialKey {
    /// Parse the names and aliases accepted by AutoQuill v0.13.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let key = match name {
            "ENTER" => Self::Enter,
            "TAB" => Self::Tab,
            "BACKSPACE" => Self::Backspace,
            "SPACE" | "SPACEBAR" => Self::Space,
            "ESC" | "ESCAPE" => Self::Escape,
            "CTRL" => Self::Control,
            "SHIFT" => Self::Shift,
            "ALT" => Self::Alt,
            "CAPSLOCK" => Self::CapsLock,
            "NUMLOCK" => Self::NumLock,
            "SCROLLLOCK" => Self::ScrollLock,
            "PAUSE" => Self::Pause,
            "INS" | "INSERT" => Self::Insert,
            "DEL" | "DELETE" => Self::Delete,
            "PRTSC" | "PRINTSCREEN" => Self::PrintScreen,
            "HOME" => Self::Home,
            "END" => Self::End,
            "PAGEUP" => Self::PageUp,
            "PAGEDOWN" => Self::PageDown,
            "LEFT" => Self::Left,
            "UP" => Self::Up,
            "RIGHT" => Self::Right,
            "DOWN" => Self::Down,
            "LWIN" => Self::LeftWindows,
            "RWIN" => Self::RightWindows,
            "APPS" => Self::Applications,
            "NUMMULTIPLY" => Self::NumpadMultiply,
            "NUMADD" => Self::NumpadAdd,
            "NUMSUB" => Self::NumpadSubtract,
            "NUMDECIMAL" => Self::NumpadDecimal,
            "NUMDIVIDE" => Self::NumpadDivide,
            _ => {
                if let Some(number) = name.strip_prefix('F').and_then(parse_small_number)
                    && (1..=12).contains(&number)
                {
                    Self::Function(number)
                } else if let Some(number) = name.strip_prefix("NUM").and_then(parse_small_number)
                    && number <= 9
                {
                    Self::NumpadDigit(number)
                } else {
                    return None;
                }
            }
        };
        Some(key)
    }

    /// Stable name used by the preview and future native input backends.
    #[must_use]
    pub fn canonical_name(self) -> String {
        match self {
            Self::Enter => "ENTER".into(),
            Self::Tab => "TAB".into(),
            Self::Backspace => "BACKSPACE".into(),
            Self::Space => "SPACE".into(),
            Self::Escape => "ESCAPE".into(),
            Self::Control => "CTRL".into(),
            Self::Shift => "SHIFT".into(),
            Self::Alt => "ALT".into(),
            Self::CapsLock => "CAPSLOCK".into(),
            Self::NumLock => "NUMLOCK".into(),
            Self::ScrollLock => "SCROLLLOCK".into(),
            Self::Pause => "PAUSE".into(),
            Self::Insert => "INSERT".into(),
            Self::Delete => "DELETE".into(),
            Self::PrintScreen => "PRINTSCREEN".into(),
            Self::Home => "HOME".into(),
            Self::End => "END".into(),
            Self::PageUp => "PAGEUP".into(),
            Self::PageDown => "PAGEDOWN".into(),
            Self::Left => "LEFT".into(),
            Self::Up => "UP".into(),
            Self::Right => "RIGHT".into(),
            Self::Down => "DOWN".into(),
            Self::LeftWindows => "LWIN".into(),
            Self::RightWindows => "RWIN".into(),
            Self::Applications => "APPS".into(),
            Self::Function(number) => format!("F{number}"),
            Self::NumpadDigit(number) => format!("NUM{number}"),
            Self::NumpadMultiply => "NUMMULTIPLY".into(),
            Self::NumpadAdd => "NUMADD".into(),
            Self::NumpadSubtract => "NUMSUB".into(),
            Self::NumpadDecimal => "NUMDECIMAL".into(),
            Self::NumpadDivide => "NUMDIVIDE".into(),
        }
    }
}

fn parse_small_number(value: &str) -> Option<u8> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| value.parse().ok())
        .flatten()
}

impl fmt::Display for SpecialKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.canonical_name())
    }
}

/// One safe, platform-neutral action in a compiled document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    Character(char),
    SpecialKey(SpecialKey),
}

impl Instruction {
    /// Human-readable action text for the simulation UI.
    #[must_use]
    pub fn action_label(&self) -> String {
        match self {
            Self::Character(' ') => "Character · Space".into(),
            Self::Character(character) => format!("Character · {character}"),
            Self::SpecialKey(key) => format!("Special key · {}", key.canonical_name()),
        }
    }

    /// Safe visual representation that never emits native input.
    #[must_use]
    pub fn preview_fragment(&self) -> String {
        match self {
            Self::Character(character) => character.to_string(),
            Self::SpecialKey(key) => format!("‹{}›", key.canonical_name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_share_canonical_keys() {
        assert_eq!(SpecialKey::from_name("ESC"), Some(SpecialKey::Escape));
        assert_eq!(SpecialKey::from_name("ESCAPE"), Some(SpecialKey::Escape));
        assert_eq!(SpecialKey::from_name("SPACEBAR"), Some(SpecialKey::Space));
        assert_eq!(SpecialKey::from_name("DEL"), Some(SpecialKey::Delete));
        assert_eq!(
            SpecialKey::from_name("PRTSC"),
            Some(SpecialKey::PrintScreen)
        );
    }

    #[test]
    fn function_and_numpad_ranges_are_bounded() {
        assert_eq!(SpecialKey::from_name("F12"), Some(SpecialKey::Function(12)));
        assert_eq!(
            SpecialKey::from_name("NUM9"),
            Some(SpecialKey::NumpadDigit(9))
        );
        assert_eq!(SpecialKey::from_name("F13"), None);
        assert_eq!(SpecialKey::from_name("NUM10"), None);
    }

    #[test]
    fn every_python_v013_special_key_name_is_supported() {
        let fixed_names = [
            "ENTER",
            "TAB",
            "BACKSPACE",
            "SPACE",
            "SPACEBAR",
            "ESC",
            "ESCAPE",
            "CTRL",
            "SHIFT",
            "ALT",
            "CAPSLOCK",
            "NUMLOCK",
            "SCROLLLOCK",
            "PAUSE",
            "INS",
            "INSERT",
            "DEL",
            "DELETE",
            "PRTSC",
            "PRINTSCREEN",
            "HOME",
            "END",
            "PAGEUP",
            "PAGEDOWN",
            "LEFT",
            "UP",
            "RIGHT",
            "DOWN",
            "LWIN",
            "RWIN",
            "APPS",
            "NUMMULTIPLY",
            "NUMADD",
            "NUMSUB",
            "NUMDECIMAL",
            "NUMDIVIDE",
        ];
        for name in fixed_names {
            assert!(SpecialKey::from_name(name).is_some(), "missing {name}");
        }
        for number in 1..=12 {
            assert!(SpecialKey::from_name(&format!("F{number}")).is_some());
        }
        for number in 0..=9 {
            assert!(SpecialKey::from_name(&format!("NUM{number}")).is_some());
        }
    }
}
