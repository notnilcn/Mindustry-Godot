// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Exact `arc.math.Rand` port (xorshift128+), required for `Waves.generate`
//! bit-parity (plan 11 §3.10 / R5, `HIGH_LEVEL_PLAN` §9).
//!
//! Ported from `Arc/arc-core/src/arc/math/Rand.java`. Unlike
//! [`crate::random::Rand`] (a `java.util.Random` wrapper used at P0), this is the
//! 128-bit xorshift state machine `Waves.generate` is seeded with. All arithmetic
//! is wrapping 64-bit two's complement, matching Java `long`.

/// Normalization constant for `next_double` (`1.0 / (1L << 53)`).
const NORM_DOUBLE: f64 = 1.0 / (1u64 << 53) as f64;
/// Normalization constant for `next_float` (`1.0 / (1L << 24)`).
const NORM_FLOAT: f64 = 1.0 / (1u64 << 24) as f64;

/// `arc.math.Rand` — xorshift128+ generator (bit-exact port).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArcRand {
    /// First half of the internal state (`seed0`).
    seed0: u64,
    /// Second half of the internal state (`seed1`).
    seed1: u64,
}

impl ArcRand {
    /// `new Rand(long seed)`.
    pub fn new(seed: u64) -> Self {
        let mut rand = Self { seed0: 0, seed1: 0 };
        rand.set_seed(seed);
        rand
    }

    /// `new Rand(long seed0, long seed1)`.
    pub fn with_state(seed0: u64, seed1: u64) -> Self {
        Self { seed0, seed1 }
    }

    /// `Rand.murmurHash3(long)`.
    fn murmur_hash3(mut x: u64) -> u64 {
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^= x >> 33;
        x
    }

    /// `Rand.setSeed(long)`: hashes the seed (and the all-zero special case).
    pub fn set_seed(&mut self, seed: u64) {
        let seed0 = Self::murmur_hash3(if seed == 0 { 1u64 << 63 } else { seed });
        self.set_state(seed0, Self::murmur_hash3(seed0));
    }

    /// `Rand.setState(seed0, seed1)`.
    pub fn set_state(&mut self, seed0: u64, seed1: u64) {
        self.seed0 = seed0;
        self.seed1 = seed1;
    }

    /// `Rand.getState(seed)`.
    pub const fn state(&self, seed: usize) -> u64 {
        if seed == 0 { self.seed0 } else { self.seed1 }
    }

    /// `Rand.nextLong()` (xorshift128+ core).
    pub fn next_long(&mut self) -> u64 {
        let mut s1 = self.seed0;
        let s0 = self.seed1;
        self.seed0 = s0;
        s1 ^= s1 << 23;
        self.seed1 = s1 ^ s0 ^ (s1 >> 17) ^ (s0 >> 26);
        self.seed1.wrapping_add(s0)
    }

    /// `Rand.nextInt()`.
    pub fn next_int(&mut self) -> i32 {
        self.next_long() as i32
    }

    /// `Rand.nextLong(long n)` (rejection sampling).
    ///
    /// # Panics
    /// Panics if `n <= 0`, matching Arc's `IllegalArgumentException`.
    pub fn next_long_bound(&mut self, n: u64) -> u64 {
        assert!(n > 0, "n must be positive");
        loop {
            let bits = self.next_long() >> 1;
            let value = bits % n;
            // Signed Java comparison: `bits - value + (n - 1) >= 0`.
            let check = (bits as i64)
                .wrapping_sub(value as i64)
                .wrapping_add(n as i64 - 1);
            if check >= 0 {
                return value;
            }
        }
    }

    /// `Rand.nextInt(int n)`.
    ///
    /// # Panics
    /// Panics if `n <= 0`.
    pub fn next_int_bound(&mut self, n: i32) -> i32 {
        assert!(n > 0, "n must be positive");
        self.next_long_bound(n as u64) as i32
    }

    /// `Rand.nextDouble()`.
    pub fn next_double(&mut self) -> f64 {
        (self.next_long() >> 11) as f64 * NORM_DOUBLE
    }

    /// `Rand.nextFloat()`.
    pub fn next_float(&mut self) -> f32 {
        ((self.next_long() >> 40) as f64 * NORM_FLOAT) as f32
    }

    /// `Rand.nextBoolean()`.
    pub fn next_boolean(&mut self) -> bool {
        (self.next_long() & 1) != 0
    }

    /// `Rand.chance(double)`.
    pub fn chance(&mut self, chance: f64) -> bool {
        self.next_double() < chance
    }

    /// `Rand.random(int max)` — inclusive `[0, max]`.
    pub fn random_inclusive(&mut self, max: i32) -> i32 {
        self.next_int_bound(max + 1)
    }

    /// `Rand.random(int min, int max)`.
    pub fn random_range_int(&mut self, min: i32, max: i32) -> i32 {
        if min >= max {
            min
        } else {
            min + self.next_int_bound(max - min + 1)
        }
    }

    /// `Rand.random(float max)`.
    pub fn random_float(&mut self, max: f32) -> f32 {
        self.next_float() * max
    }

    /// `Rand.random(float min, float max)`.
    pub fn random_range_float(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_float()
    }

    /// `Rand.range(float amount)` — symmetric `[-amount, amount]`.
    pub fn range_float(&mut self, amount: f32) -> f32 {
        self.next_float() * amount * 2.0 - amount
    }

    /// `Rand.range(int amount)` — symmetric `[-amount, amount]`.
    pub fn range_int(&mut self, amount: i32) -> i32 {
        self.next_int_bound(amount * 2 + 1) - amount
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vectors captured from Arc `Rand` on the JVM (`parity/java/DumpWaves.java`);
    /// see `tests/golden/wave_generate.json`. These pin the xorshift128+ state
    /// machine and the float/int draw helpers.
    #[test]
    fn known_vectors_match_arc() {
        // Captured from Arc `Rand` on the JVM by `parity/java/DumpWaves.java`.
        let mut rand = ArcRand::new(1);
        assert_eq!(rand.next_long(), 3787875997830008111);
        let mut rand = ArcRand::new(42);
        assert_eq!(rand.next_float().to_bits(), 1044726120);
        let mut rand = ArcRand::new(7);
        assert_eq!(
            [
                rand.next_int_bound(100),
                rand.next_int_bound(100),
                rand.next_int_bound(100)
            ],
            [55, 33, 95]
        );
    }

    #[test]
    fn float_is_in_unit_interval() {
        let mut rand = ArcRand::new(42);
        for _ in 0..1000 {
            let value = rand.next_float();
            assert!((0.0..1.0).contains(&value), "value={value}");
            assert!((0.0..1.0).contains(&(rand.next_double() as f32)));
        }
    }

    #[test]
    fn int_bounds_are_inclusive() {
        let mut rand = ArcRand::new(9);
        for _ in 0..1000 {
            assert!((0..=5).contains(&rand.random_inclusive(5)));
            assert!((3..=7).contains(&rand.random_range_int(3, 7)));
            assert!((-4..=4).contains(&rand.range_int(4)));
        }
    }

    #[test]
    fn state_roundtrip() {
        let mut rand = ArcRand::new(7);
        let (a, b) = (rand.state(0), rand.state(1));
        let _ = rand.next_long();
        rand.set_state(a, b);
        let mut other = ArcRand::new(7);
        assert_eq!(rand.next_long(), other.next_long());
    }
}
