//! WPM pacing and pause compensation shared by simulation and future native backends.

use std::time::Duration;

use crate::domain::{BreakSettings, PauseSettings, Wpm};

use super::RandomSource;

pub const MINIMUM_COMPENSATED_DELAY: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, Copy)]
pub struct TimingModel {
    effective_character_delay: Duration,
}

impl TimingModel {
    #[must_use]
    pub fn new(wpm: Wpm, breaks: BreakSettings, pauses: PauseSettings) -> Self {
        let base = wpm.character_delay_seconds();
        let extra = estimated_pause_overhead_per_character(breaks, pauses);
        let effective = if extra > 0.0 {
            (base - extra).max(MINIMUM_COMPENSATED_DELAY.as_secs_f64())
        } else {
            base
        };
        Self {
            effective_character_delay: Duration::from_secs_f64(effective),
        }
    }

    #[must_use]
    pub const fn effective_character_delay(self) -> Duration {
        self.effective_character_delay
    }

    pub fn next_operation_delay(&self, random: &mut impl RandomSource) -> Duration {
        let variance = random.float_inclusive(0.8, 1.2);
        self.effective_character_delay.mul_f64(variance)
    }
}

#[must_use]
pub fn estimated_pause_overhead_per_character(breaks: BreakSettings, pauses: PauseSettings) -> f64 {
    let mut extra = 0.0;
    if pauses.enabled {
        let average_interval =
            (f64::from(pauses.min_characters) + f64::from(pauses.max_characters)) / 2.0;
        let average_pause = (pauses.min_seconds + pauses.max_seconds) / 2.0;
        extra += average_pause / average_interval.max(1.0);
    }
    if breaks.enabled {
        let average_words = (f64::from(breaks.min_words) + f64::from(breaks.max_words)) / 2.0;
        let average_break = (breaks.min_seconds + breaks.max_seconds) / 2.0;
        extra += average_break / (average_words * 5.0).max(1.0);
    }
    extra
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_humanization_preserves_the_wpm_delay() {
        let timing = TimingModel::new(
            Wpm::new(60),
            BreakSettings::default(),
            PauseSettings::default(),
        );
        assert_eq!(
            timing.effective_character_delay(),
            Duration::from_millis(200)
        );
    }

    #[test]
    fn aggressive_pauses_never_reduce_the_base_below_five_milliseconds() {
        let timing = TimingModel::new(
            Wpm::new(200),
            BreakSettings {
                enabled: true,
                min_words: 1,
                max_words: 1,
                min_seconds: 60.0,
                max_seconds: 60.0,
            },
            PauseSettings {
                enabled: true,
                min_characters: 1,
                max_characters: 1,
                min_seconds: 10.0,
                max_seconds: 10.0,
            },
        );
        assert_eq!(
            timing.effective_character_delay(),
            MINIMUM_COMPENSATED_DELAY
        );
    }

    #[test]
    fn maximum_character_intervals_do_not_overflow() {
        let overhead = estimated_pause_overhead_per_character(
            BreakSettings::default(),
            PauseSettings {
                enabled: true,
                min_characters: u32::MAX,
                max_characters: u32::MAX,
                ..PauseSettings::default()
            },
        );
        assert!(overhead.is_finite());
        assert!(overhead > 0.0);
    }
}
