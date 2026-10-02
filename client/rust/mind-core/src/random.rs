// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic random number generation.
//!
//! [`JavaRandom`] is an exact port of `java.util.Random`'s 48-bit LCG (as required
//! for Java parity); [`Rand`] adds the small `arc.math.Rand` helper subset used at
//! P0. Ported from the OpenJDK `java.base/java/util/Random.java` documentation and
//! `arc-core/src/arc/math/Rand.java`.

/// Normalization constant for `next_double`.
const NORM_DOUBLE: f64 = 1.0 / (1u64 << 53) as f64;

/// Exact port of `java.util.Random` (48-bit linear congruential generator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5_DEEC_E66D;
    const ADDEND: u64 = 0xB;
    const MASK: u64 = (1u64 << 48) - 1;

    /// Creates a generator seeded exactly like `new java.util.Random(seed)`.
    pub fn new(seed: u64) -> Self {
        let mut random = Self { seed: 0 };
        random.set_seed(seed);
        random
    }

    /// Re-seeds exactly like `java.util.Random.setSeed(long)`.
    pub fn set_seed(&mut self, seed: u64) {
        self.seed = (seed ^ Self::MULTIPLIER) & Self::MASK;
    }

    /// Java's `next(bits)`.
    fn next(&mut self, bits: u32) -> i32 {
        debug_assert!(bits > 0 && bits <= 32);
        self.seed = self
            .seed
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::ADDEND)
            & Self::MASK;
        (self.seed >> (48 - bits)) as i32
    }

    /// Java's `nextInt()`.
    pub fn next_int(&mut self) -> i32 {
        self.next(32)
    }

    /// Java's `nextInt(bound)`.
    ///
    /// # Panics
    /// Panics if `bound <= 0`, matching `IllegalArgumentException` (this is a
    /// programming invariant, never scenario data).
    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        assert!(bound > 0, "bound must be positive");
        let mut r = self.next(31);
        let m = bound - 1;
        if (bound & m) == 0 {
            // bound is a power of two
            r = ((bound as i64 * r as i64) >> 31) as i32;
        } else {
            let mut u = r;
            loop {
                let candidate = u % bound;
                if u.wrapping_sub(candidate).wrapping_add(m) >= 0 {
                    r = candidate;
                    break;
                }
                u = self.next(31);
            }
        }
        r
    }

    /// Java's `nextLong()`.
    pub fn next_long(&mut self) -> i64 {
        ((self.next(32) as i64) << 32) + self.next(32) as i64
    }

    /// Java's `nextBoolean()`.
    pub fn next_boolean(&mut self) -> bool {
        self.next(1) != 0
    }

    /// Java's `nextFloat()`.
    pub fn next_float(&mut self) -> f32 {
        self.next(24) as f32 / (1u32 << 24) as f32
    }

    /// Java's `nextDouble()`.
    pub fn next_double(&mut self) -> f64 {
        (((self.next(26) as i64) << 27) + self.next(27) as i64) as f64 * NORM_DOUBLE
    }

    /// The raw 48-bit LCG state (checksum/snapshot input).
    pub const fn state(&self) -> u64 {
        self.seed
    }

    /// Restores the raw 48-bit LCG state (snapshot restore).
    pub fn set_state(&mut self, state: u64) {
        self.seed = state & Self::MASK;
    }
}

/// `arc.math.Rand` helper subset: `random()`, `range`, `chance`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rand {
    inner: JavaRandom,
}

impl Rand {
    /// Creates a helper around `new java.util.Random(seed)`.
    pub fn new(seed: u64) -> Self {
        Self {
            inner: JavaRandom::new(seed),
        }
    }

    /// Uniform float in `[0, 1)`.
    pub fn random(&mut self) -> f32 {
        self.inner.next_float()
    }

    /// Uniform float in `[min, max)`.
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.random()
    }

    /// `true` with probability `p` (Java `nextDouble` under the hood, like Arc).
    pub fn chance(&mut self, p: f64) -> bool {
        self.inner.next_double() < p
    }

    /// Uniform integer in `[0, bound)`.
    pub fn int_bound(&mut self, bound: i32) -> i32 {
        self.inner.next_int_bound(bound)
    }

    /// Uniform integer in `[min, max]` (Arc `random(int, int)`).
    pub fn int_range(&mut self, min: i32, max: i32) -> i32 {
        if min >= max {
            min
        } else {
            min + self.inner.next_int_bound(max - min + 1)
        }
    }

    /// Uniform float in `[-amount, amount]` (Arc `range(float)`).
    pub fn range_symmetric(&mut self, amount: f32) -> f32 {
        self.random() * amount * 2.0 - amount
    }

    /// Access to the underlying `java.util.Random` state.
    pub fn java_random(&mut self) -> &mut JavaRandom {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-checked against an independent Python implementation of the
    /// documented OpenJDK algorithm and published `new Random(42).nextInt()`
    /// values; if these drift, the port is no longer `java.util.Random`.
    #[test]
    fn java_parity_vectors() {
        let mut random = JavaRandom::new(42);
        assert_eq!(random.next_int(), -1170105035);
        assert_eq!(random.next_int(), 234785527);
        assert_eq!(random.next_int(), -1360544799);
        assert_eq!(random.next_int(), 205897768);
        assert_eq!(random.next_int(), 1325939940);

        let mut random = JavaRandom::new(42);
        assert_eq!(random.next_int_bound(100), 30);

        let mut random = JavaRandom::new(42);
        assert_eq!(random.next_long(), -5025562857975149833);

        let mut random = JavaRandom::new(42);
        assert!(random.next_boolean());
        assert!(!random.next_boolean());

        let mut random = JavaRandom::new(42);
        assert_eq!(random.next_float().to_bits(), 0x3f3a_419d);

        let mut random = JavaRandom::new(42);
        assert_eq!(random.next_double(), 0.7275636800328681);

        let mut random = JavaRandom::new(42);
        assert_eq!(
            (0..5)
                .map(|_| random.next_int_bound(1000))
                .collect::<Vec<_>>(),
            vec![130, 763, 248, 884, 970]
        );

        let mut random = JavaRandom::new(1);
        assert_eq!(
            (0..5).map(|_| random.next_int_bound(7)).collect::<Vec<_>>(),
            vec![4, 4, 1, 0, 6]
        );
    }

    #[test]
    fn power_of_two_bound_uses_multiplication_path() {
        let mut random = JavaRandom::new(42);
        let value = random.next_int_bound(64);
        assert!((0..64).contains(&value));
    }

    #[test]
    fn rand_helpers_stay_in_range() {
        let mut rand = Rand::new(9);
        for _ in 0..1000 {
            let value = rand.random();
            assert!((0.0..1.0).contains(&value));
            let ranged = rand.range(-3.0, 7.0);
            assert!((-3.0..7.0).contains(&ranged));
            assert_eq!(rand.int_range(4, 4), 4);
            assert!((0..5).contains(&rand.int_bound(5)));
        }
        let mut rand = Rand::new(9);
        assert!(rand.chance(1.0));
        let mut rand = Rand::new(9);
        assert!(!rand.chance(0.0));
    }
}
