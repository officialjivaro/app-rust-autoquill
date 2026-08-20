//! Non-blocking settings warnings shown beside the relevant controls.

use super::SettingsDraft;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningCode {
    VeryHighWpm,
    VeryShortStopAfter,
    ReversedLoopRange,
    EmptyLoopText,
    ReversedBreakWordRange,
    ReversedBreakDurationRange,
    ReversedPauseIntervalRange,
    ReversedPauseDurationRange,
    StickyTargetBehavior,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsWarning {
    pub code: WarningCode,
    pub message: &'static str,
}

/// Reproduce the live, non-blocking warning rules from AutoQuill v0.13.
#[must_use]
pub fn collect_warnings(draft: &SettingsDraft, typing_text_is_empty: bool) -> Vec<SettingsWarning> {
    let mut warnings = Vec::new();

    if draft.wpm.is_some_and(|wpm| wpm >= 140) {
        warnings.push(SettingsWarning {
            code: WarningCode::VeryHighWpm,
            message: "Very high WPM can look less human and may type less reliably in some apps.",
        });
    }

    if draft.stop_after_enabled && draft.stop_after_seconds.is_some_and(|seconds| seconds <= 5) {
        warnings.push(SettingsWarning {
            code: WarningCode::VeryShortStopAfter,
            message: "Stop after is set very short. Typing may stop before much text is entered.",
        });
    }

    if draft.loop_enabled {
        push_reversed_integer_warning(
            &mut warnings,
            draft.loop_min_seconds,
            draft.loop_max_seconds,
            WarningCode::ReversedLoopRange,
            "Loop wait range is reversed. AutoQuill will swap the values when it runs.",
        );
        if typing_text_is_empty {
            warnings.push(SettingsWarning {
                code: WarningCode::EmptyLoopText,
                message: "Loop is enabled, but the main text box is empty.",
            });
        }
    }

    if draft.breaks_enabled {
        push_reversed_integer_warning(
            &mut warnings,
            draft.break_min_words,
            draft.break_max_words,
            WarningCode::ReversedBreakWordRange,
            "Simulate Breaks word range is reversed. AutoQuill will swap the values when it runs.",
        );
        push_reversed_float_warning(
            &mut warnings,
            draft.break_min_seconds,
            draft.break_max_seconds,
            WarningCode::ReversedBreakDurationRange,
            "Simulate Breaks pause range is reversed. AutoQuill will swap the values when it runs.",
        );
    }

    if draft.pauses_enabled {
        push_reversed_integer_warning(
            &mut warnings,
            draft.pause_min_characters,
            draft.pause_max_characters,
            WarningCode::ReversedPauseIntervalRange,
            "Simulate Pauses frequency range is reversed. AutoQuill will swap the values when it runs.",
        );
        push_reversed_float_warning(
            &mut warnings,
            draft.pause_min_seconds,
            draft.pause_max_seconds,
            WarningCode::ReversedPauseDurationRange,
            "Simulate Pauses duration range is reversed. AutoQuill will swap the values when it runs.",
        );
    }

    if draft.sticky_typing {
        warnings.push(SettingsWarning {
            code: WarningCode::StickyTargetBehavior,
            message: "Sticky typing auto-detects the target at start. Classic apps use background typing; browser fields use browser-safe foreground assist.",
        });
    }

    warnings
}

fn push_reversed_integer_warning(
    warnings: &mut Vec<SettingsWarning>,
    minimum: Option<i64>,
    maximum: Option<i64>,
    code: WarningCode,
    message: &'static str,
) {
    if minimum
        .zip(maximum)
        .is_some_and(|(minimum, maximum)| minimum > maximum)
    {
        warnings.push(SettingsWarning { code, message });
    }
}

fn push_reversed_float_warning(
    warnings: &mut Vec<SettingsWarning>,
    minimum: Option<f64>,
    maximum: Option<f64>,
    code: WarningCode,
    message: &'static str,
) {
    if minimum.zip(maximum).is_some_and(|(minimum, maximum)| {
        minimum.is_finite() && maximum.is_finite() && minimum > maximum
    }) {
        warnings.push(SettingsWarning { code, message });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(draft: &SettingsDraft, text_is_empty: bool) -> Vec<WarningCode> {
        collect_warnings(draft, text_is_empty)
            .into_iter()
            .map(|warning| warning.code)
            .collect()
    }

    #[test]
    fn defaults_have_no_warnings() {
        assert!(collect_warnings(&SettingsDraft::default(), false).is_empty());
    }

    #[test]
    fn threshold_warnings_match_v013() {
        let draft = SettingsDraft {
            wpm: Some(140),
            stop_after_enabled: true,
            stop_after_seconds: Some(5),
            ..SettingsDraft::default()
        };
        let warning_codes = codes(&draft, false);
        assert!(warning_codes.contains(&WarningCode::VeryHighWpm));
        assert!(warning_codes.contains(&WarningCode::VeryShortStopAfter));
    }

    #[test]
    fn enabled_features_report_reversed_ranges() {
        let draft = SettingsDraft {
            loop_enabled: true,
            loop_min_seconds: Some(20),
            loop_max_seconds: Some(10),
            breaks_enabled: true,
            break_min_words: Some(42),
            break_max_words: Some(18),
            break_min_seconds: Some(5.0),
            break_max_seconds: Some(2.0),
            pauses_enabled: true,
            pause_min_characters: Some(250),
            pause_max_characters: Some(120),
            pause_min_seconds: Some(1.8),
            pause_max_seconds: Some(0.6),
            ..SettingsDraft::default()
        };
        let warning_codes = codes(&draft, false);
        assert!(warning_codes.contains(&WarningCode::ReversedLoopRange));
        assert!(warning_codes.contains(&WarningCode::ReversedBreakWordRange));
        assert!(warning_codes.contains(&WarningCode::ReversedBreakDurationRange));
        assert!(warning_codes.contains(&WarningCode::ReversedPauseIntervalRange));
        assert!(warning_codes.contains(&WarningCode::ReversedPauseDurationRange));
    }

    #[test]
    fn empty_loop_and_sticky_target_are_explained() {
        let draft = SettingsDraft {
            loop_enabled: true,
            sticky_typing: true,
            ..SettingsDraft::default()
        };
        let warning_codes = codes(&draft, true);
        assert!(warning_codes.contains(&WarningCode::EmptyLoopText));
        assert!(warning_codes.contains(&WarningCode::StickyTargetBehavior));
    }
}
