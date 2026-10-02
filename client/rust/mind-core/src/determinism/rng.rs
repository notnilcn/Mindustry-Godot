// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic RNG streams (plan 05 §3.11).
//!
//! Only [`RngStream::Sim`] participates in [`crate::determinism::checksum`];
//! `MapGen`/`Waves`/`Fx` are isolated so view/content randomness can never
//! desync the sim. Backed by the exact `java.util.Random` LCG in
//! [`crate::random::JavaRandom`], seeded per-stream so streams never alias.

use crate::random::JavaRandom;

/// Named, isolated deterministic streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RngStream {
    /// Simulation stream (in checksum).
    Sim = 0,
    /// Map generation (outside sim timing).
    MapGen,
    /// Wave composition.
    Waves,
    /// Effects/view only (never in checksum).
    Fx,
}

/// Every stream, in index order.
pub const ALL_STREAMS: [RngStream; 4] = [
    RngStream::Sim,
    RngStream::MapGen,
    RngStream::Waves,
    RngStream::Fx,
];

impl RngStream {
    /// Dense index into [`SimRng`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Parity name.
    pub const fn name(self) -> &'static str {
        match self {
            RngStream::Sim => "sim",
            RngStream::MapGen => "mapgen",
            RngStream::Waves => "waves",
            RngStream::Fx => "fx",
        }
    }
}

/// Deterministic multi-stream RNG.
#[derive(Debug, Clone)]
pub struct SimRng {
    streams: [JavaRandom; 4],
    seed: u64,
}

impl SimRng {
    /// Creates all streams from a master seed.
    ///
    /// Stream `i` is seeded with `seed ^ (i+1)*GOLDEN` (SplitMix64-style
    /// mixing) so changing one stream's consumption cannot shift another.
    pub fn new(seed: u64) -> Self {
        const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;
        let make = |index: u64| {
            let mixed = seed
                .wrapping_add((index + 1).wrapping_mul(GOLDEN))
                .wrapping_mul(GOLDEN);
            JavaRandom::new(mixed)
        };
        let streams = [make(0), make(1), make(2), make(3)];
        Self { streams, seed }
    }

    /// The master seed.
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// Mutable access to one stream.
    pub fn stream_mut(&mut self, stream: RngStream) -> &mut JavaRandom {
        &mut self.streams[stream.index()]
    }

    /// Read-only access to one stream.
    pub fn stream(&self, stream: RngStream) -> &JavaRandom {
        &self.streams[stream.index()]
    }

    /// Uniform float in `[0, 1)` on a stream.
    pub fn next_float(&mut self, stream: RngStream) -> f32 {
        self.stream_mut(stream).next_float()
    }

    /// Uniform int in `[0, bound)` on a stream (`bound <= 0` returns 0).
    pub fn random(&mut self, stream: RngStream, bound: i32) -> i32 {
        if bound <= 0 {
            0
        } else {
            self.stream_mut(stream).next_int_bound(bound)
        }
    }

    /// Uniform float in `[min, max)`.
    pub fn range(&mut self, stream: RngStream, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_float(stream)
    }

    /// `true` with probability `p`.
    pub fn chance(&mut self, stream: RngStream, p: f64) -> bool {
        self.stream_mut(stream).next_double() < p
    }

    /// Raw LCG state of a stream (checksum/snapshot input).
    pub const fn stream_state(&self, stream: RngStream) -> u64 {
        self.streams[stream.index()].state()
    }

    /// Restores a stream's raw LCG state (snapshot restore).
    pub fn set_stream_state(&mut self, stream: RngStream, state: u64) {
        self.stream_mut(stream).set_state(state);
    }

    /// Sim-stream FNV-1a fold of the raw state (checksum helper).
    pub fn sim_state(&self) -> u64 {
        self.stream_state(RngStream::Sim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan 05 §7.1 `rng::tests::stream_isolation`.
    #[test]
    fn stream_isolation() {
        let mut rng = SimRng::new(42);
        // Drawing from MapGen must not affect Sim.
        let sim_before = rng.stream_state(RngStream::Sim);
        let _ = rng.random(RngStream::MapGen, 1_000);
        let _ = rng.random(RngStream::Fx, 1_000);
        assert_eq!(rng.stream_state(RngStream::Sim), sim_before);
        let first = rng.random(RngStream::Sim, 1_000);
        assert_ne!(rng.stream_state(RngStream::Sim), sim_before);
        assert!((0..1_000).contains(&first));
    }

    #[test]
    fn same_seed_same_streams() {
        let mut a = SimRng::new(7);
        let mut b = SimRng::new(7);
        for stream in ALL_STREAMS {
            assert_eq!(a.random(stream, 100), b.random(stream, 100));
        }
        assert_eq!(
            a.stream_state(RngStream::Sim),
            b.stream_state(RngStream::Sim)
        );
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = SimRng::new(1);
        let mut b = SimRng::new(2);
        assert_ne!(a.sim_state(), b.sim_state());
        assert_ne!(
            a.random(RngStream::Sim, 1_000_000),
            b.random(RngStream::Sim, 1_000_000)
        );
    }

    #[test]
    fn chance_bounds() {
        let mut rng = SimRng::new(3);
        assert!(rng.chance(RngStream::Fx, 1.0));
        assert!(!rng.chance(RngStream::Fx, 0.0));
    }

    #[test]
    fn state_roundtrip() {
        let mut rng = SimRng::new(11);
        let saved = rng.stream_state(RngStream::Sim);
        let expected = rng.random(RngStream::Sim, 1_000_000);
        rng.set_stream_state(RngStream::Sim, saved);
        assert_eq!(rng.random(RngStream::Sim, 1_000_000), expected);
    }
}
