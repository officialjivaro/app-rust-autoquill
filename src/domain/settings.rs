//! Typed AutoQuill settings, defaults, and normalization.

use super::Shortcut;

const MAX_LOOP_SECONDS: u32 = 86_400;
const MAX_BREAK_WORDS: u32 = 500;
const MAX_BREAK_SECONDS: f64 = 60.0;
const MAX_UNBOUNDED_INTEGER: i64 = u32::MAX as i64;

/// Words per minute using the standard five-characters-per-word convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Wpm(u16);

impl Wpm {
    pub const MIN: u16 = 1;
    pub const MAX: u16 = 200;
    pub const DEFAULT: u16 = 60;

    /// Clamp a raw value to AutoQuill's supported range.
    #[must_use]
    pub fn new(raw: i64) -> Self {
        Self(raw.clamp(i64::from(Self::MIN), i64::from(Self::MAX)) as u16)
    }

    /// Return the validated numeric value.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }

    /// Convert WPM into the average delay per intended character in seconds.
    #[must_use]
    pub fn character_delay_seconds(self) -> f64 {
        60.0 / (f64::from(self.0) * 5.0)
    }
}

impl Default for Wpm {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

/// User intent for where a future native backend should type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TargetIntent {
    #[default]
    Foreground,
    StickyAuto,
}

impl TargetIntent {
    /// Normalize the legacy sticky flag and temporary target-mode strings used by v0.13.
    #[must_use]
    pub fn from_legacy(sticky_typing: bool, _target_mode: Option<&str>) -> Self {
        if sticky_typing {
            Self::StickyAuto
        } else {
            Self::Foreground
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StopAfterSettings {
    pub enabled: bool,
    pub seconds: u32,
}

impl Default for StopAfterSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            seconds: 60,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopSettings {
    pub enabled: bool,
    pub min_seconds: u32,
    pub max_seconds: u32,
}

impl Default for LoopSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            min_seconds: 5,
            max_seconds: 10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorSettings {
    pub enabled: bool,
    pub min_interval: u32,
    pub max_interval: u32,
    pub min_errors: u32,
    pub max_errors: u32,
}

impl Default for ErrorSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            min_interval: 15,
            max_interval: 40,
            min_errors: 1,
            max_errors: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreakSettings {
    pub enabled: bool,
    pub min_words: u32,
    pub max_words: u32,
    pub min_seconds: f64,
    pub max_seconds: f64,
}

impl Default for BreakSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            min_words: 18,
            max_words: 42,
            min_seconds: 2.0,
            max_seconds: 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PauseSettings {
    pub enabled: bool,
    pub min_characters: u32,
    pub max_characters: u32,
    pub min_seconds: f64,
    pub max_seconds: f64,
}

impl Default for PauseSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            min_characters: 120,
            max_characters: 250,
            min_seconds: 0.6,
            max_seconds: 1.8,
        }
    }
}

/// Fully normalized settings used to create an immutable session snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct TypingSettings {
    pub shortcut: Shortcut,
    pub wpm: Wpm,
    pub target: TargetIntent,
    pub startup_delay_enabled: bool,
    pub startup_delay_seconds: u32,
    pub stop_after: StopAfterSettings,
    pub looping: LoopSettings,
    pub errors: ErrorSettings,
    pub breaks: BreakSettings,
    pub pauses: PauseSettings,
}

impl Default for TypingSettings {
    fn default() -> Self {
        Self {
            shortcut: Shortcut::default(),
            wpm: Wpm::default(),
            target: TargetIntent::Foreground,
            startup_delay_enabled: false,
            startup_delay_seconds: 2,
            stop_after: StopAfterSettings::default(),
            looping: LoopSettings::default(),
            errors: ErrorSettings::default(),
            breaks: BreakSettings::default(),
            pauses: PauseSettings::default(),
        }
    }
}

/// Untrusted settings input from UI fields or profile data before normalization.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsDraft {
    pub shortcut: Shortcut,
    pub wpm: Option<i64>,
    pub sticky_typing: bool,
    pub legacy_target_mode: Option<String>,
    pub startup_delay_enabled: bool,
    pub stop_after_enabled: bool,
    pub stop_after_seconds: Option<i64>,
    pub loop_enabled: bool,
    pub loop_min_seconds: Option<i64>,
    pub loop_max_seconds: Option<i64>,
    pub errors_enabled: bool,
    pub error_min_interval: Option<i64>,
    pub error_max_interval: Option<i64>,
    pub error_min_count: Option<i64>,
    pub error_max_count: Option<i64>,
    pub breaks_enabled: bool,
    pub break_min_words: Option<i64>,
    pub break_max_words: Option<i64>,
    pub break_min_seconds: Option<f64>,
    pub break_max_seconds: Option<f64>,
    pub pauses_enabled: bool,
    pub pause_min_characters: Option<i64>,
    pub pause_max_characters: Option<i64>,
    pub pause_min_seconds: Option<f64>,
    pub pause_max_seconds: Option<f64>,
}

impl Default for SettingsDraft {
    fn default() -> Self {
        let defaults = TypingSettings::default();
        Self {
            shortcut: defaults.shortcut,
            wpm: Some(i64::from(defaults.wpm.get())),
            sticky_typing: false,
            legacy_target_mode: None,
            startup_delay_enabled: defaults.startup_delay_enabled,
            stop_after_enabled: defaults.stop_after.enabled,
            stop_after_seconds: Some(i64::from(defaults.stop_after.seconds)),
            loop_enabled: defaults.looping.enabled,
            loop_min_seconds: Some(i64::from(defaults.looping.min_seconds)),
            loop_max_seconds: Some(i64::from(defaults.looping.max_seconds)),
            errors_enabled: defaults.errors.enabled,
            error_min_interval: Some(i64::from(defaults.errors.min_interval)),
            error_max_interval: Some(i64::from(defaults.errors.max_interval)),
            error_min_count: Some(i64::from(defaults.errors.min_errors)),
            error_max_count: Some(i64::from(defaults.errors.max_errors)),
            breaks_enabled: defaults.breaks.enabled,
            break_min_words: Some(i64::from(defaults.breaks.min_words)),
            break_max_words: Some(i64::from(defaults.breaks.max_words)),
            break_min_seconds: Some(defaults.breaks.min_seconds),
            break_max_seconds: Some(defaults.breaks.max_seconds),
            pauses_enabled: defaults.pauses.enabled,
            pause_min_characters: Some(i64::from(defaults.pauses.min_characters)),
            pause_max_characters: Some(i64::from(defaults.pauses.max_characters)),
            pause_min_seconds: Some(defaults.pauses.min_seconds),
            pause_max_seconds: Some(defaults.pauses.max_seconds),
        }
    }
}

impl SettingsDraft {
    /// Normalize all untrusted values into a safe immutable settings snapshot.
    #[must_use]
    pub fn normalize(&self) -> TypingSettings {
        let defaults = TypingSettings::default();
        let (loop_min, loop_max) = normalize_integer_pair(
            self.loop_min_seconds,
            self.loop_max_seconds,
            defaults.looping.min_seconds,
            defaults.looping.max_seconds,
            1,
            MAX_LOOP_SECONDS,
        );
        let (error_min_interval, error_max_interval) = normalize_integer_pair(
            self.error_min_interval,
            self.error_max_interval,
            defaults.errors.min_interval,
            defaults.errors.max_interval,
            1,
            u32::MAX,
        );
        let (error_min_count, error_max_count) = normalize_integer_pair(
            self.error_min_count,
            self.error_max_count,
            defaults.errors.min_errors,
            defaults.errors.max_errors,
            1,
            u32::MAX,
        );
        let (break_min_words, break_max_words) = normalize_integer_pair(
            self.break_min_words,
            self.break_max_words,
            defaults.breaks.min_words,
            defaults.breaks.max_words,
            1,
            MAX_BREAK_WORDS,
        );
        let (break_min_seconds, break_max_seconds) = normalize_float_pair(
            self.break_min_seconds,
            self.break_max_seconds,
            defaults.breaks.min_seconds,
            defaults.breaks.max_seconds,
            0.0,
            Some(MAX_BREAK_SECONDS),
        );
        let (pause_min_characters, pause_max_characters) = normalize_integer_pair(
            self.pause_min_characters,
            self.pause_max_characters,
            defaults.pauses.min_characters,
            defaults.pauses.max_characters,
            1,
            u32::MAX,
        );
        let (pause_min_seconds, pause_max_seconds) = normalize_float_pair(
            self.pause_min_seconds,
            self.pause_max_seconds,
            defaults.pauses.min_seconds,
            defaults.pauses.max_seconds,
            0.0,
            None,
        );

        TypingSettings {
            shortcut: self.shortcut,
            wpm: Wpm::new(self.wpm.unwrap_or(i64::from(Wpm::DEFAULT))),
            target: TargetIntent::from_legacy(
                self.sticky_typing,
                self.legacy_target_mode.as_deref(),
            ),
            startup_delay_enabled: self.startup_delay_enabled,
            startup_delay_seconds: 2,
            stop_after: StopAfterSettings {
                enabled: self.stop_after_enabled,
                seconds: normalize_integer(
                    self.stop_after_seconds,
                    defaults.stop_after.seconds,
                    1,
                    MAX_LOOP_SECONDS,
                ),
            },
            looping: LoopSettings {
                enabled: self.loop_enabled,
                min_seconds: loop_min,
                max_seconds: loop_max,
            },
            errors: ErrorSettings {
                enabled: self.errors_enabled,
                min_interval: error_min_interval,
                max_interval: error_max_interval,
                min_errors: error_min_count,
                max_errors: error_max_count,
            },
            breaks: BreakSettings {
                enabled: self.breaks_enabled,
                min_words: break_min_words,
                max_words: break_max_words,
                min_seconds: break_min_seconds,
                max_seconds: break_max_seconds,
            },
            pauses: PauseSettings {
                enabled: self.pauses_enabled,
                min_characters: pause_min_characters,
                max_characters: pause_max_characters,
                min_seconds: pause_min_seconds,
                max_seconds: pause_max_seconds,
            },
        }
    }
}

fn normalize_integer(raw: Option<i64>, fallback: u32, minimum: u32, maximum: u32) -> u32 {
    let maximum = i64::from(maximum).min(MAX_UNBOUNDED_INTEGER);
    raw.unwrap_or(i64::from(fallback))
        .clamp(i64::from(minimum), maximum) as u32
}

fn normalize_integer_pair(
    raw_minimum: Option<i64>,
    raw_maximum: Option<i64>,
    fallback_minimum: u32,
    fallback_maximum: u32,
    minimum: u32,
    maximum: u32,
) -> (u32, u32) {
    let mut normalized_minimum = normalize_integer(raw_minimum, fallback_minimum, minimum, maximum);
    let mut normalized_maximum = normalize_integer(raw_maximum, fallback_maximum, minimum, maximum);
    if normalized_minimum > normalized_maximum {
        std::mem::swap(&mut normalized_minimum, &mut normalized_maximum);
    }
    (normalized_minimum, normalized_maximum)
}

fn normalize_float(raw: Option<f64>, fallback: f64, minimum: f64, maximum: Option<f64>) -> f64 {
    let mut value = raw.filter(|value| value.is_finite()).unwrap_or(fallback);
    value = value.max(minimum);
    if let Some(maximum) = maximum {
        value = value.min(maximum);
    }
    value
}

fn normalize_float_pair(
    raw_minimum: Option<f64>,
    raw_maximum: Option<f64>,
    fallback_minimum: f64,
    fallback_maximum: f64,
    minimum: f64,
    maximum: Option<f64>,
) -> (f64, f64) {
    let mut normalized_minimum = normalize_float(raw_minimum, fallback_minimum, minimum, maximum);
    let mut normalized_maximum = normalize_float(raw_maximum, fallback_maximum, minimum, maximum);
    if normalized_minimum > normalized_maximum {
        std::mem::swap(&mut normalized_minimum, &mut normalized_maximum);
    }
    (normalized_minimum, normalized_maximum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_autoquill_v013() {
        let settings = TypingSettings::default();
        assert_eq!(settings.wpm.get(), 60);
        assert_eq!(settings.shortcut.to_string(), "F1");
        assert_eq!(settings.target, TargetIntent::Foreground);
        assert_eq!(settings.startup_delay_seconds, 2);
        assert_eq!(settings.stop_after, StopAfterSettings::default());
        assert_eq!(settings.looping, LoopSettings::default());
        assert_eq!(settings.errors, ErrorSettings::default());
        assert_eq!(settings.breaks, BreakSettings::default());
        assert_eq!(settings.pauses, PauseSettings::default());
    }

    #[test]
    fn wpm_clamps_and_converts_using_five_characters_per_word() {
        assert_eq!(Wpm::new(-20).get(), 1);
        assert_eq!(Wpm::new(999).get(), 200);
        assert!((Wpm::new(60).character_delay_seconds() - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn missing_values_use_defaults_and_invalid_floats_are_finite() {
        let draft = SettingsDraft {
            wpm: None,
            break_min_seconds: Some(f64::NAN),
            pause_max_seconds: Some(f64::INFINITY),
            ..SettingsDraft::default()
        };
        let settings = draft.normalize();
        assert_eq!(settings.wpm, Wpm::default());
        assert_eq!(settings.breaks.min_seconds, 2.0);
        assert_eq!(settings.pauses.max_seconds, 1.8);
    }

    #[test]
    fn reversed_ranges_are_swapped_after_clamping() {
        let draft = SettingsDraft {
            loop_min_seconds: Some(30),
            loop_max_seconds: Some(10),
            break_min_words: Some(900),
            break_max_words: Some(-10),
            pause_min_seconds: Some(3.5),
            pause_max_seconds: Some(0.5),
            ..SettingsDraft::default()
        };
        let settings = draft.normalize();
        assert_eq!(
            (settings.looping.min_seconds, settings.looping.max_seconds),
            (10, 30)
        );
        assert_eq!(
            (settings.breaks.min_words, settings.breaks.max_words),
            (1, 500)
        );
        assert_eq!(
            (settings.pauses.min_seconds, settings.pauses.max_seconds),
            (0.5, 3.5)
        );
    }

    #[test]
    fn stop_after_and_loop_values_use_the_existing_day_cap() {
        let draft = SettingsDraft {
            stop_after_seconds: Some(0),
            loop_min_seconds: Some(90_000),
            loop_max_seconds: Some(100_000),
            ..SettingsDraft::default()
        };
        let settings = draft.normalize();
        assert_eq!(settings.stop_after.seconds, 1);
        assert_eq!(settings.looping.min_seconds, 86_400);
        assert_eq!(settings.looping.max_seconds, 86_400);
    }

    #[test]
    fn legacy_target_modes_follow_the_original_sticky_checkbox() {
        assert_eq!(
            TargetIntent::from_legacy(false, Some("native_background")),
            TargetIntent::Foreground
        );
        assert_eq!(
            TargetIntent::from_legacy(true, Some("browser_foreground_assist")),
            TargetIntent::StickyAuto
        );
    }
}
