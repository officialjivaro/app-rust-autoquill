//! Small deterministic random source used by portable scheduling.

use std::time::{SystemTime, UNIX_EPOCH};

/// Minimal interface that keeps scheduler tests deterministic without a large RNG dependency.
pub trait RandomSource {
    fn next_u64(&mut self) -> u64;

    fn integer_inclusive(&mut self, minimum: u32, maximum: u32) -> u32 {
        let (minimum, maximum) = if minimum <= maximum {
            (minimum, maximum)
        } else {
            (maximum, minimum)
        };
        let width = u64::from(maximum - minimum) + 1;
        minimum + (self.next_u64() % width) as u32
    }

    fn float_inclusive(&mut self, minimum: f64, maximum: f64) -> f64 {
        let (minimum, maximum) = if minimum <= maximum {
            (minimum, maximum)
        } else {
            (maximum, minimum)
        };
        let unit = (self.next_u64() >> 11) as f64 / ((1_u64 << 53) - 1) as f64;
        minimum + (maximum - minimum) * unit
    }
}

/// SplitMix64: compact, fast, reproducible, and sufficient for humanization timing.
#[derive(Debug, Clone)]
pub struct SeededRandom {
    state: u64,
}

impl SeededRandom {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    #[must_use]
    pub fn from_system_time() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0xA076_1D64_78BD_642F, |duration| {
                duration.as_nanos() as u64 ^ duration.as_secs().rotate_left(17)
            });
        Self::new(seed)
    }
}

impl RandomSource for SeededRandom {
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        value ^ (value >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_seeds_produce_equal_sequences() {
        let mut left = SeededRandom::new(42);
        let mut right = SeededRandom::new(42);
        for _ in 0..20 {
            assert_eq!(left.next_u64(), right.next_u64());
        }
    }

    #[test]
    fn normalized_ranges_accept_reversed_input() {
        let mut random = SeededRandom::new(7);
        for _ in 0..100 {
            assert!((3..=9).contains(&random.integer_inclusive(9, 3)));
            assert!((0.8..=1.2).contains(&random.float_inclusive(1.2, 0.8)));
        }
    }
}
