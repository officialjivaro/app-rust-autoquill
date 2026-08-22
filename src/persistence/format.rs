//! JSON profile and preference formats, including non-destructive legacy parsing.

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::{
    CURRENT_PROFILE_SCHEMA_VERSION, Profile, SettingsDraft, Shortcut, TargetIntent, TypingSettings,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileStatus {
    Current,
    Legacy,
    Newer(u32),
    Invalid(String),
}

impl ProfileStatus {
    #[must_use]
    pub fn badge(&self) -> &'static str {
        match self {
            Self::Current => "CURRENT",
            Self::Legacy => "LEGACY",
            Self::Newer(_) => "NEWER",
            Self::Invalid(_) => "INVALID",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub schema_version: u32,
    pub last_profile: Option<String>,
    pub default_profile: Option<String>,
    pub imported_profiles: Vec<String>,
    pub window: WindowPreferences,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowPreferences {
    pub width: u32,
    pub height: u32,
    pub x: Option<i32>,
    pub y: Option<i32>,
}

impl Default for WindowPreferences {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            x: None,
            y: None,
        }
    }
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: 2,
            last_profile: None,
            default_profile: None,
            imported_profiles: Vec::new(),
            window: WindowPreferences::default(),
        }
    }
}

pub(crate) fn inspect_profile(bytes: &[u8]) -> ProfileStatus {
    let value: Value = match serde_json::from_slice(bytes) {
        Ok(value) => value,
        Err(error) => return ProfileStatus::Invalid(error.to_string()),
    };
    let Some(object) = value.as_object() else {
        return ProfileStatus::Invalid("profile JSON must be an object".to_owned());
    };
    match object.get("schema_version") {
        None => ProfileStatus::Legacy,
        Some(Value::Number(number)) => match number.as_u64() {
            Some(version) if version == u64::from(CURRENT_PROFILE_SCHEMA_VERSION) => {
                ProfileStatus::Current
            }
            Some(version) if version > u64::from(CURRENT_PROFILE_SCHEMA_VERSION) => {
                ProfileStatus::Newer(u32::try_from(version).unwrap_or(u32::MAX))
            }
            Some(_) => ProfileStatus::Legacy,
            None => ProfileStatus::Invalid("schema_version must be a positive integer".to_owned()),
        },
        Some(_) => ProfileStatus::Invalid("schema_version must be an integer".to_owned()),
    }
}

pub(crate) fn parse_profile(name: &str, bytes: &[u8]) -> Result<Profile, String> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    match inspect_profile(bytes) {
        ProfileStatus::Current => parse_current(name, &value),
        ProfileStatus::Legacy => parse_legacy(name, &value),
        ProfileStatus::Newer(version) => Err(format!(
            "profile schema {version} is newer than supported schema {CURRENT_PROFILE_SCHEMA_VERSION}"
        )),
        ProfileStatus::Invalid(message) => Err(message),
    }
}

pub(crate) fn serialize_profile(profile: &Profile) -> Result<Vec<u8>, String> {
    let settings = &profile.settings;
    let target = match settings.target {
        TargetIntent::Foreground => "foreground",
        TargetIntent::StickyAuto => "sticky_auto",
    };
    let value = json!({
        "schema_version": CURRENT_PROFILE_SCHEMA_VERSION,
        "typing_text": profile.typing_text,
        "settings": {
            "shortcut": settings.shortcut.to_string(),
            "wpm": settings.wpm.get(),
            "target": target,
            "startup_delay_enabled": settings.startup_delay_enabled,
            "stop_after": {
                "enabled": settings.stop_after.enabled,
                "seconds": settings.stop_after.seconds,
            },
            "looping": {
                "enabled": settings.looping.enabled,
                "min_seconds": settings.looping.min_seconds,
                "max_seconds": settings.looping.max_seconds,
            },
            "errors": {
                "enabled": settings.errors.enabled,
                "min_interval": settings.errors.min_interval,
                "max_interval": settings.errors.max_interval,
                "min_errors": settings.errors.min_errors,
                "max_errors": settings.errors.max_errors,
            },
            "breaks": {
                "enabled": settings.breaks.enabled,
                "min_words": settings.breaks.min_words,
                "max_words": settings.breaks.max_words,
                "min_seconds": settings.breaks.min_seconds,
                "max_seconds": settings.breaks.max_seconds,
            },
            "pauses": {
                "enabled": settings.pauses.enabled,
                "min_characters": settings.pauses.min_characters,
                "max_characters": settings.pauses.max_characters,
                "min_seconds": settings.pauses.min_seconds,
                "max_seconds": settings.pauses.max_seconds,
            }
        }
    });
    serde_json::to_vec_pretty(&value).map_err(|error| error.to_string())
}

fn parse_current(name: &str, value: &Value) -> Result<Profile, String> {
    let settings = value
        .get("settings")
        .and_then(Value::as_object)
        .ok_or_else(|| "current profile is missing a settings object".to_owned())?;
    let defaults = TypingSettings::default();
    let shortcut = settings
        .get("shortcut")
        .and_then(Value::as_str)
        .and_then(|value| Shortcut::from_str(value).ok())
        .unwrap_or(defaults.shortcut);
    let target = settings.get("target").and_then(Value::as_str);
    let stop = object(settings.get("stop_after"));
    let looping = object(settings.get("looping"));
    let errors = object(settings.get("errors"));
    let breaks = object(settings.get("breaks"));
    let pauses = object(settings.get("pauses"));
    let draft = SettingsDraft {
        shortcut,
        wpm: integer(settings.get("wpm")),
        sticky_typing: target == Some("sticky_auto"),
        startup_delay_enabled: boolean(settings.get("startup_delay_enabled"), false),
        stop_after_enabled: boolean(stop.and_then(|value| value.get("enabled")), false),
        stop_after_seconds: integer(stop.and_then(|value| value.get("seconds"))),
        loop_enabled: boolean(looping.and_then(|value| value.get("enabled")), false),
        loop_min_seconds: integer(looping.and_then(|value| value.get("min_seconds"))),
        loop_max_seconds: integer(looping.and_then(|value| value.get("max_seconds"))),
        errors_enabled: boolean(errors.and_then(|value| value.get("enabled")), false),
        error_min_interval: integer(errors.and_then(|value| value.get("min_interval"))),
        error_max_interval: integer(errors.and_then(|value| value.get("max_interval"))),
        error_min_count: integer(errors.and_then(|value| value.get("min_errors"))),
        error_max_count: integer(errors.and_then(|value| value.get("max_errors"))),
        breaks_enabled: boolean(breaks.and_then(|value| value.get("enabled")), false),
        break_min_words: integer(breaks.and_then(|value| value.get("min_words"))),
        break_max_words: integer(breaks.and_then(|value| value.get("max_words"))),
        break_min_seconds: number(breaks.and_then(|value| value.get("min_seconds"))),
        break_max_seconds: number(breaks.and_then(|value| value.get("max_seconds"))),
        pauses_enabled: boolean(pauses.and_then(|value| value.get("enabled")), false),
        pause_min_characters: integer(pauses.and_then(|value| value.get("min_characters"))),
        pause_max_characters: integer(pauses.and_then(|value| value.get("max_characters"))),
        pause_min_seconds: number(pauses.and_then(|value| value.get("min_seconds"))),
        pause_max_seconds: number(pauses.and_then(|value| value.get("max_seconds"))),
        ..SettingsDraft::default()
    };
    Ok(Profile::new(name, draft.normalize(), typing_text(value)))
}

fn parse_legacy(name: &str, value: &Value) -> Result<Profile, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "legacy profile JSON must be an object".to_owned())?;
    let defaults = TypingSettings::default();
    let shortcut = object
        .get("function_key")
        .and_then(Value::as_str)
        .and_then(Shortcut::from_legacy_function_key)
        .unwrap_or(defaults.shortcut);
    let draft = SettingsDraft {
        shortcut,
        wpm: integer(object.get("wpm")),
        sticky_typing: boolean(object.get("sticky_typing"), false),
        legacy_target_mode: object
            .get("target_mode")
            .and_then(Value::as_str)
            .map(str::to_owned),
        startup_delay_enabled: boolean(object.get("delay_before"), false),
        stop_after_enabled: boolean(object.get("stop_after_enabled"), false),
        stop_after_seconds: integer(object.get("stop_after_seconds")),
        loop_enabled: boolean(object.get("loop_enabled"), false),
        loop_min_seconds: integer(object.get("loop_min")),
        loop_max_seconds: integer(object.get("loop_max")),
        errors_enabled: boolean(object.get("simulate_human_errors"), false),
        error_min_interval: integer(object.get("min_interval")),
        error_max_interval: integer(object.get("max_interval")),
        error_min_count: integer(object.get("min_errors")),
        error_max_count: integer(object.get("max_errors")),
        breaks_enabled: boolean(object.get("breaks_enabled"), false),
        break_min_words: integer(object.get("breaks_word_min")),
        break_max_words: integer(object.get("breaks_word_max")),
        break_min_seconds: number(object.get("breaks_sec_min")),
        break_max_seconds: number(object.get("breaks_sec_max")),
        pauses_enabled: boolean(object.get("simulate_pauses_enabled"), false),
        pause_min_characters: integer(object.get("pause_every_min_chars")),
        pause_max_characters: integer(object.get("pause_every_max_chars")),
        pause_min_seconds: number(object.get("pause_min_seconds")),
        pause_max_seconds: number(object.get("pause_max_seconds")),
    };
    Ok(Profile::from_legacy(
        name,
        draft.normalize(),
        typing_text(value),
    ))
}

fn typing_text(value: &Value) -> String {
    value
        .get("typing_text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn object(value: Option<&Value>) -> Option<&serde_json::Map<String, Value>> {
    value.and_then(Value::as_object)
}

fn boolean(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

fn integer(value: Option<&Value>) -> Option<i64> {
    value.and_then(Value::as_i64)
}

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_format_round_trips_unicode_and_all_settings() {
        let mut draft = SettingsDraft {
            shortcut: "Ctrl+F8".parse().unwrap(),
            wpm: Some(111),
            sticky_typing: true,
            startup_delay_enabled: true,
            stop_after_enabled: true,
            stop_after_seconds: Some(77),
            loop_enabled: true,
            loop_min_seconds: Some(4),
            loop_max_seconds: Some(9),
            errors_enabled: true,
            breaks_enabled: true,
            pauses_enabled: true,
            ..SettingsDraft::default()
        };
        draft.pause_min_seconds = Some(0.75);
        let original = Profile::new("日本語", draft.normalize(), "Hello 🌍\n日本語");
        let bytes = serialize_profile(&original).unwrap();
        let restored = parse_profile("日本語", &bytes).unwrap();
        assert_eq!(restored.typing_text, original.typing_text);
        assert_eq!(restored.settings, original.settings);
        assert_eq!(inspect_profile(&bytes), ProfileStatus::Current);
    }

    #[test]
    fn legacy_flat_format_maps_every_setting_family() {
        let bytes = br#"{
          "typing_text":"legacy", "function_key":"F9", "wpm":88,
          "sticky_typing":true, "target_mode":"native_background", "delay_before":true,
          "stop_after_enabled":true, "stop_after_seconds":12,
          "loop_enabled":true, "loop_min":3, "loop_max":7,
          "simulate_human_errors":true, "min_interval":9, "max_interval":19,
          "min_errors":2, "max_errors":5,
          "breaks_enabled":true, "breaks_word_min":10, "breaks_word_max":20,
          "breaks_sec_min":1.5, "breaks_sec_max":3.5,
          "simulate_pauses_enabled":true, "pause_every_min_chars":30,
          "pause_every_max_chars":60, "pause_min_seconds":0.2, "pause_max_seconds":0.8
        }"#;
        let profile = parse_profile("Old", bytes).unwrap();
        assert!(profile.requires_upgrade());
        assert_eq!(profile.settings.shortcut.to_string(), "F9");
        assert_eq!(profile.settings.wpm.get(), 88);
        assert_eq!(profile.settings.target, TargetIntent::StickyAuto);
        assert_eq!(
            (
                profile.settings.looping.min_seconds,
                profile.settings.looping.max_seconds
            ),
            (3, 7)
        );
        assert_eq!(
            (
                profile.settings.errors.min_errors,
                profile.settings.errors.max_errors
            ),
            (2, 5)
        );
        assert_eq!(
            (
                profile.settings.breaks.min_words,
                profile.settings.breaks.max_words
            ),
            (10, 20)
        );
        assert_eq!(
            (
                profile.settings.pauses.min_characters,
                profile.settings.pauses.max_characters
            ),
            (30, 60)
        );
    }

    #[test]
    fn future_schema_is_inspected_but_not_loaded() {
        let bytes = br#"{"schema_version":99,"settings":{}}"#;
        assert_eq!(inspect_profile(bytes), ProfileStatus::Newer(99));
        assert!(parse_profile("Future", bytes).is_err());
    }

    #[test]
    fn legacy_preferences_gain_safe_window_defaults() {
        let preferences: Preferences = serde_json::from_slice(
            br#"{"schema_version":1,"last_profile":"Daily","default_profile":null}"#,
        )
        .unwrap();
        assert_eq!(preferences.last_profile.as_deref(), Some("Daily"));
        assert_eq!(preferences.window, WindowPreferences::default());
    }
}
