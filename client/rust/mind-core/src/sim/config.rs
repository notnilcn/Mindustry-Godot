// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SimConfig` — the fixed-step and determinism knobs (plan 05 §3.4).
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`maxDeltaClient/Server`)
//! and the plan-00 fixed-step accumulator.

use crate::constants::{MAX_TICKS_PER_FRAME, TICKS_PER_SECOND};

/// Deterministic simulation configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimConfig {
    /// Fixed simulation rate in Hz (D8: 60).
    pub fixed_hz: u32,
    /// Maximum catch-up steps per rendered frame.
    pub max_ticks_per_frame: u32,
    /// Master deterministic seed.
    pub seed: u64,
    /// Worker count for `AsyncCore` (`1` in the determinism harness).
    pub workers: usize,
    /// Whether to record the per-tick set execution trace (`trace order`).
    pub trace_order: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl SimConfig {
    /// Creates the 60 Hz defaults.
    pub const fn new() -> Self {
        Self {
            fixed_hz: TICKS_PER_SECOND,
            max_ticks_per_frame: MAX_TICKS_PER_FRAME,
            seed: 0,
            workers: 1,
            trace_order: false,
        }
    }

    /// Builder: sets the master seed.
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Builder: sets the worker count (`0` clamps to `1`).
    pub fn with_workers(mut self, workers: usize) -> Self {
        self.workers = workers.max(1);
        self
    }

    /// Builder: enables the order trace.
    pub fn with_trace(mut self, trace: bool) -> Self {
        self.trace_order = trace;
        self
    }

    /// Seconds per fixed step.
    pub fn step_seconds(&self) -> f64 {
        if self.fixed_hz == 0 {
            1.0 / f64::from(TICKS_PER_SECOND)
        } else {
            1.0 / f64::from(self.fixed_hz)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_locked_60hz_values() {
        let config = SimConfig::new();
        assert_eq!(config.fixed_hz, 60);
        assert_eq!(config.max_ticks_per_frame, 4);
        assert_eq!(config.workers, 1);
        assert!(!config.trace_order);
        assert!((config.step_seconds() - 1.0 / 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn worker_zero_clamps_and_builders_chain() {
        let config = SimConfig::new()
            .with_seed(9)
            .with_workers(0)
            .with_trace(true);
        assert_eq!(config.seed, 9);
        assert_eq!(config.workers, 1);
        assert!(config.trace_order);
    }
}
