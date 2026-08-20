//! Deterministic schedulers for breaks, short pauses, and simulated mistakes.

use std::time::Duration;

use crate::domain::{BreakSettings, ErrorSettings, PauseSettings};

use super::{Instruction, RandomSource};

const BOUNDARY_PUNCTUATION: &[char] = &['.', ',', '!', '?', ':', ';', ')', ']'];

#[derive(Debug, Clone)]
pub struct BreakScheduler {
    settings: BreakSettings,
    in_word: bool,
    units_since_break: u32,
    next_target: u32,
}

impl BreakScheduler {
    pub fn new(settings: BreakSettings, random: &mut impl RandomSource) -> Self {
        let mut scheduler = Self {
            settings,
            in_word: false,
            units_since_break: 0,
            next_target: 0,
        };
        scheduler.refresh_target(random);
        scheduler
    }

    pub fn reset_after_loop(&mut self, random: &mut impl RandomSource) {
        self.in_word = false;
        self.units_since_break = 0;
        self.refresh_target(random);
    }

    pub fn step(
        &mut self,
        instruction: &Instruction,
        random: &mut impl RandomSource,
    ) -> Option<Duration> {
        if !self.settings.enabled {
            return None;
        }

        match instruction {
            Instruction::Character(character) if is_boundary(*character) => {
                if !self.in_word {
                    return None;
                }
                self.in_word = false;
                self.units_since_break += 1;
                self.maybe_pause(random)
            }
            Instruction::Character(_) => {
                self.in_word = true;
                None
            }
            Instruction::SpecialKey(_) => {
                let word_pause = if self.in_word {
                    self.units_since_break += 1;
                    self.maybe_pause(random)
                } else {
                    None
                };
                self.in_word = false;
                self.units_since_break += 1;
                word_pause.or_else(|| self.maybe_pause(random))
            }
        }
    }

    fn maybe_pause(&mut self, random: &mut impl RandomSource) -> Option<Duration> {
        if self.units_since_break < self.next_target {
            return None;
        }
        let seconds = random.float_inclusive(self.settings.min_seconds, self.settings.max_seconds);
        self.units_since_break = 0;
        self.in_word = false;
        self.refresh_target(random);
        Some(Duration::from_secs_f64(seconds.max(0.0)))
    }

    fn refresh_target(&mut self, random: &mut impl RandomSource) {
        self.next_target = if self.settings.enabled {
            random.integer_inclusive(self.settings.min_words, self.settings.max_words)
        } else {
            0
        };
    }
}

fn is_boundary(character: char) -> bool {
    character.is_whitespace() || BOUNDARY_PUNCTUATION.contains(&character)
}

#[derive(Debug, Clone)]
pub struct ShortPauseScheduler {
    settings: PauseSettings,
    characters_since_pause: u32,
    next_target: u32,
}

impl ShortPauseScheduler {
    pub fn new(settings: PauseSettings, random: &mut impl RandomSource) -> Self {
        let mut scheduler = Self {
            settings,
            characters_since_pause: 0,
            next_target: 0,
        };
        scheduler.reset(random);
        scheduler
    }

    pub fn reset(&mut self, random: &mut impl RandomSource) {
        self.characters_since_pause = 0;
        self.next_target = if self.settings.enabled {
            random.integer_inclusive(self.settings.min_characters, self.settings.max_characters)
        } else {
            0
        };
    }

    pub fn step_intended_character(&mut self, random: &mut impl RandomSource) -> Option<Duration> {
        if !self.settings.enabled {
            return None;
        }
        self.characters_since_pause += 1;
        if self.characters_since_pause < self.next_target {
            return None;
        }
        let seconds = random.float_inclusive(self.settings.min_seconds, self.settings.max_seconds);
        self.reset(random);
        Some(Duration::from_secs_f64(seconds.max(0.0)))
    }
}

#[derive(Debug, Clone)]
pub struct ErrorScheduler {
    settings: ErrorSettings,
    tokens_since_error: u32,
    next_target: u32,
    next_count: u32,
}

impl ErrorScheduler {
    pub fn new(settings: ErrorSettings, random: &mut impl RandomSource) -> Self {
        let mut scheduler = Self {
            settings,
            tokens_since_error: 0,
            next_target: 0,
            next_count: 0,
        };
        scheduler.reset(random);
        scheduler
    }

    pub fn reset(&mut self, random: &mut impl RandomSource) {
        self.tokens_since_error = 0;
        if self.settings.enabled {
            self.next_target =
                random.integer_inclusive(self.settings.min_interval, self.settings.max_interval);
            self.next_count =
                random.integer_inclusive(self.settings.min_errors, self.settings.max_errors);
        } else {
            self.next_target = 0;
            self.next_count = 0;
        }
    }

    pub fn step_intended_token(&mut self, random: &mut impl RandomSource) -> Option<u32> {
        if !self.settings.enabled {
            return None;
        }
        self.tokens_since_error += 1;
        if self.tokens_since_error < self.next_target {
            return None;
        }
        let count = self.next_count;
        self.reset(random);
        Some(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typing::SpecialKey;

    #[derive(Default)]
    struct MinimumRandom;

    impl RandomSource for MinimumRandom {
        fn next_u64(&mut self) -> u64 {
            0
        }
    }

    #[test]
    fn breaks_happen_only_after_word_boundaries() {
        let mut random = MinimumRandom;
        let mut scheduler = BreakScheduler::new(
            BreakSettings {
                enabled: true,
                min_words: 1,
                max_words: 1,
                min_seconds: 2.0,
                max_seconds: 2.0,
            },
            &mut random,
        );
        for character in "hello".chars() {
            assert!(
                scheduler
                    .step(&Instruction::Character(character), &mut random)
                    .is_none()
            );
        }
        assert_eq!(
            scheduler.step(&Instruction::Character(' '), &mut random),
            Some(Duration::from_secs(2))
        );
    }

    #[test]
    fn each_special_key_is_a_unit_and_a_boundary() {
        let mut random = MinimumRandom;
        let mut scheduler = BreakScheduler::new(
            BreakSettings {
                enabled: true,
                min_words: 1,
                max_words: 1,
                min_seconds: 1.0,
                max_seconds: 1.0,
            },
            &mut random,
        );
        assert_eq!(
            scheduler.step(&Instruction::SpecialKey(SpecialKey::Enter), &mut random),
            Some(Duration::from_secs(1))
        );
    }
}
