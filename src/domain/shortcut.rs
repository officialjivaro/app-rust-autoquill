//! Portable global-shortcut representation.

use std::fmt;

/// Modifier keys supported by the expanded shortcut recorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierSet {
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl ModifierSet {
    /// Return whether the shortcut has at least one modifier.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        !self.control && !self.alt && !self.shift && !self.meta
    }
}

/// A portable shortcut key. Platform backends translate this into native key codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutKey {
    Function(u8),
    Character(char),
    Space,
}

/// A validated global shortcut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    pub modifiers: ModifierSet,
    pub key: ShortcutKey,
}

/// Why a proposed shortcut cannot be represented safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutError {
    FunctionKeyOutOfRange,
    UnsupportedCharacter,
}

impl fmt::Display for ShortcutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FunctionKeyOutOfRange => {
                formatter.write_str("function key must be F1 through F12")
            }
            Self::UnsupportedCharacter => {
                formatter.write_str("shortcut character must be a printable ASCII character")
            }
        }
    }
}

impl std::error::Error for ShortcutError {}

impl Shortcut {
    /// Create a shortcut after validating its portable representation.
    pub fn new(modifiers: ModifierSet, key: ShortcutKey) -> Result<Self, ShortcutError> {
        let key = match key {
            ShortcutKey::Function(number @ 1..=12) => ShortcutKey::Function(number),
            ShortcutKey::Function(_) => return Err(ShortcutError::FunctionKeyOutOfRange),
            ShortcutKey::Character(character)
                if character.is_ascii_graphic() && !character.is_ascii_control() =>
            {
                ShortcutKey::Character(character.to_ascii_uppercase())
            }
            ShortcutKey::Character(_) => return Err(ShortcutError::UnsupportedCharacter),
            ShortcutKey::Space => ShortcutKey::Space,
        };

        Ok(Self { modifiers, key })
    }

    /// Parse the F1–F12 values used by AutoQuill v0.13 profiles.
    #[must_use]
    pub fn from_legacy_function_key(value: &str) -> Option<Self> {
        let number = value.trim().strip_prefix(['F', 'f'])?.parse::<u8>().ok()?;
        Self::new(ModifierSet::default(), ShortcutKey::Function(number)).ok()
    }
}

impl Default for Shortcut {
    fn default() -> Self {
        Self {
            modifiers: ModifierSet::default(),
            key: ShortcutKey::Function(1),
        }
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.control {
            formatter.write_str("Ctrl+")?;
        }
        if self.modifiers.alt {
            formatter.write_str("Alt+")?;
        }
        if self.modifiers.shift {
            formatter.write_str("Shift+")?;
        }
        if self.modifiers.meta {
            formatter.write_str("Meta+")?;
        }

        match self.key {
            ShortcutKey::Function(number) => write!(formatter, "F{number}"),
            ShortcutKey::Character(character) => write!(formatter, "{character}"),
            ShortcutKey::Space => formatter.write_str("Space"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preserves_the_original_f1_shortcut() {
        assert_eq!(Shortcut::default().to_string(), "F1");
    }

    #[test]
    fn legacy_function_keys_accept_case_and_reject_out_of_range_values() {
        assert_eq!(
            Shortcut::from_legacy_function_key("f12").map(|shortcut| shortcut.to_string()),
            Some("F12".to_owned())
        );
        assert_eq!(Shortcut::from_legacy_function_key("F13"), None);
        assert_eq!(Shortcut::from_legacy_function_key("Space"), None);
    }

    #[test]
    fn modifier_shortcuts_have_a_stable_display_form() {
        let shortcut = Shortcut::new(
            ModifierSet {
                control: true,
                shift: true,
                ..ModifierSet::default()
            },
            ShortcutKey::Space,
        )
        .expect("Ctrl+Shift+Space should be supported");

        assert_eq!(shortcut.to_string(), "Ctrl+Shift+Space");
    }

    #[test]
    fn character_shortcuts_normalize_to_uppercase() {
        let shortcut = Shortcut::new(ModifierSet::default(), ShortcutKey::Character('q'))
            .expect("printable ASCII should be supported");
        assert_eq!(shortcut.to_string(), "Q");
    }
}
