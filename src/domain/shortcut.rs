//! Portable global-shortcut representation.

use std::{fmt, str::FromStr};

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
            formatter.write_str("Win+")?;
        }

        match self.key {
            ShortcutKey::Function(number) => write!(formatter, "F{number}"),
            ShortcutKey::Character('+') => formatter.write_str("Plus"),
            ShortcutKey::Character(character) => write!(formatter, "{character}"),
            ShortcutKey::Space => formatter.write_str("Space"),
        }
    }
}

impl FromStr for Shortcut {
    type Err = ShortcutError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut modifiers = ModifierSet::default();
        let mut key = None;
        for part in value
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
        {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers.control = true,
                "alt" => modifiers.alt = true,
                "shift" => modifiers.shift = true,
                "meta" | "cmd" | "win" => modifiers.meta = true,
                "space" => key = Some(ShortcutKey::Space),
                "plus" => key = Some(ShortcutKey::Character('+')),
                other
                    if other.len() > 1
                        && other.starts_with('f')
                        && other[1..]
                            .chars()
                            .all(|character| character.is_ascii_digit()) =>
                {
                    let number = other[1..]
                        .parse::<u8>()
                        .map_err(|_| ShortcutError::FunctionKeyOutOfRange)?;
                    key = Some(ShortcutKey::Function(number));
                }
                other if other.chars().count() == 1 => {
                    key = other.chars().next().map(ShortcutKey::Character);
                }
                _ => return Err(ShortcutError::UnsupportedCharacter),
            }
        }
        Self::new(modifiers, key.ok_or(ShortcutError::UnsupportedCharacter)?)
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

        let windows_shortcut = Shortcut::new(
            ModifierSet {
                meta: true,
                ..ModifierSet::default()
            },
            ShortcutKey::Function(8),
        )
        .unwrap();
        assert_eq!(windows_shortcut.to_string(), "Win+F8");
        assert_eq!("Win+F8".parse::<Shortcut>().unwrap(), windows_shortcut);
    }

    #[test]
    fn character_shortcuts_normalize_to_uppercase() {
        let shortcut = Shortcut::new(ModifierSet::default(), ShortcutKey::Character('q'))
            .expect("printable ASCII should be supported");
        assert_eq!(shortcut.to_string(), "Q");
    }

    #[test]
    fn display_form_round_trips_through_parser() {
        let shortcut: Shortcut = "Ctrl+Alt+F8".parse().unwrap();
        assert_eq!(shortcut.to_string(), "Ctrl+Alt+F8");
        assert!("F13".parse::<Shortcut>().is_err());
    }

    #[test]
    fn plus_and_letter_f_shortcuts_round_trip_without_ambiguity() {
        let plus = Shortcut::new(
            ModifierSet {
                control: true,
                ..ModifierSet::default()
            },
            ShortcutKey::Character('+'),
        )
        .unwrap();
        assert_eq!(plus.to_string(), "Ctrl+Plus");
        assert_eq!("Ctrl+Plus".parse::<Shortcut>().unwrap(), plus);

        let letter_f = Shortcut::new(
            ModifierSet {
                alt: true,
                ..ModifierSet::default()
            },
            ShortcutKey::Character('F'),
        )
        .unwrap();
        assert_eq!("Alt+F".parse::<Shortcut>().unwrap(), letter_f);
    }
}
