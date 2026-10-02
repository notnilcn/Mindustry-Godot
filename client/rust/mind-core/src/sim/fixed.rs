// SPDX-License-Identifier: GPL-3.0-only

//! Fixed 60 Hz step accumulator (D8).
//!
//! Mirrors the Arc/LibGDX `Gdx.graphics.getDeltaTime()` clamp behavior used by
//! Mindustry's `Logic`: never simulate more than `max_catchup` steps per frame,
//! dropping the backlog instead of spiraling.

/// The fixed simulation step: 1/60 s.
pub const SIM_STEP: f64 = 1.0 / 60.0;

/// Accumulates frame time and yields whole simulation steps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedStepRunner {
    /// Leftover un-simulated time.
    pub accumulator: f64,
    /// Fixed step size in seconds.
    pub step: f64,
    /// Maximum steps produced per `advance` call.
    pub max_catchup: u32,
}

impl Default for FixedStepRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl FixedStepRunner {
    /// Creates a runner with `step = 1/60` and `max_catchup = 5`.
    pub fn new() -> Self {
        Self {
            accumulator: 0.0,
            step: SIM_STEP,
            max_catchup: 5,
        }
    }

    /// Creates a runner matching a `SimConfig` (`Logic.maxDeltaClient = 4`).
    pub fn for_rate(hz: u32, max_catchup: u32) -> Self {
        Self {
            accumulator: 0.0,
            step: if hz == 0 {
                SIM_STEP
            } else {
                1.0 / f64::from(hz)
            },
            max_catchup: max_catchup.max(1),
        }
    }

    /// Adds `frame_dt` seconds and returns how many sim steps to run now.
    ///
    /// Non-finite/negative deltas are ignored; the backlog is capped at
    /// `max_catchup` steps (excess time is dropped).
    pub fn advance(&mut self, frame_dt: f64) -> u32 {
        if frame_dt.is_finite() && frame_dt > 0.0 {
            self.accumulator += frame_dt;
        }
        let mut steps = 0u32;
        while self.accumulator + f64::EPSILON >= self.step && steps < self.max_catchup {
            self.accumulator -= self.step;
            steps += 1;
        }
        if steps == self.max_catchup && self.accumulator >= self.step {
            self.accumulator %= self.step;
        }
        steps
    }

    /// Interpolation alpha in `[0, 1)` for render smoothing (view-only).
    pub fn alpha(&self) -> f64 {
        if self.step > 0.0 {
            self.accumulator / self.step
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_sixty_steps_per_simulated_second() {
        let mut runner = FixedStepRunner::new();
        let mut steps = 0;
        for _ in 0..60 {
            steps += runner.advance(1.0 / 60.0);
        }
        assert_eq!(steps, 60);
        assert!(runner.alpha() < 1.0 / 60.0);
    }

    #[test]
    fn clamps_large_frame_deltas() {
        let mut runner = FixedStepRunner::new();
        assert_eq!(runner.advance(10.0), runner.max_catchup);
        assert!(runner.accumulator < runner.step);
    }

    #[test]
    fn ignores_invalid_deltas() {
        let mut runner = FixedStepRunner::new();
        assert_eq!(runner.advance(f64::NAN), 0);
        assert_eq!(runner.advance(-1.0), 0);
        assert_eq!(runner.advance(f64::INFINITY), 0);
        assert_eq!(runner.accumulator, 0.0);
    }
}
