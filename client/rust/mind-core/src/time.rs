// SPDX-License-Identifier: GPL-3.0-only

//! Arc `Time` parity subset.
//!
//! Ported from `arc-core/src/arc/util/Time.java` (`update`, `run`,
//! `setDeltaProvider`). At P0 `Time` is owned by a [`crate::sim::Sim`] and its
//! delta defaults to `1.0` per tick; tests can swap the delta provider to
//! reproduce the Mindustry `ApplicationTests` timer semantics.

/// A queued delayed task.
struct DelayRun {
    delay: f32,
    finish: Box<dyn FnMut()>,
}

/// Simulation clock with Arc-compatible delayed-run queue.
pub struct Time {
    /// Global delta for the current update (Arc `Time.delta`).
    pub delta: f32,
    /// Accumulated float time in ticks (Arc `Time.time`).
    pub time: f32,
    /// Number of [`Time::update`] calls (monotonic tick counter).
    pub ticks: u64,
    time_raw: f64,
    runs: Vec<DelayRun>,
    delta_provider: Box<dyn FnMut() -> f32>,
}

impl Default for Time {
    fn default() -> Self {
        Self::new()
    }
}

impl Time {
    /// Creates a clock whose delta is `1.0` per update (one sim tick).
    pub fn new() -> Self {
        Self {
            delta: 1.0,
            time: 0.0,
            ticks: 0,
            time_raw: 0.0,
            runs: Vec::new(),
            delta_provider: Box::new(|| 1.0),
        }
    }

    /// Schedules `finish` to run once its delay has been consumed.
    /// Ported from Arc `Time.run(float, Runnable)`.
    pub fn run(&mut self, delay: f32, finish: impl FnMut() + 'static) {
        self.runs.push(DelayRun {
            delay,
            finish: Box::new(finish),
        });
    }

    /// Replaces the delta provider and immediately refreshes [`Time::delta`]
    /// (Arc `Time.setDeltaProvider`).
    pub fn set_delta_provider(&mut self, provider: impl FnMut() -> f32 + 'static) {
        let mut provider = provider;
        self.delta = provider();
        self.delta_provider = Box::new(provider);
    }

    /// Advances time by [`Time::delta`] and fires every run whose delay is spent.
    /// Ported from Arc `Time.update()`.
    pub fn update(&mut self) {
        self.delta = (self.delta_provider)();
        self.time_raw += self.delta as f64;
        if !self.time_raw.is_finite() {
            // Arc resets both float and double time when it goes inf/NaN.
            self.time_raw = 0.0;
        }
        self.time = self.time_raw as f32;
        self.ticks = self.ticks.wrapping_add(1);

        let mut runs = std::mem::take(&mut self.runs);
        runs.retain_mut(|run| {
            run.delay -= self.delta;
            if run.delay <= 0.0 {
                (run.finish)();
                false
            } else {
                true
            }
        });
        // Tasks queued by a callback during this update survive to the next one
        // (callbacks cannot borrow `Time`, so this only re-adds nothing today,
        // but keeps the queue semantics explicit).
        runs.append(&mut self.runs);
        self.runs = runs;
    }

    /// Cancels every queued run (Arc `Time.clear`).
    pub fn clear(&mut self) {
        self.runs.clear();
    }

    /// Number of queued runs.
    pub fn runs_len(&self) -> usize {
        self.runs.len()
    }

    /// Raw double time accumulator (Arc `Time.getInternalTime`).
    pub fn get_internal_time(&self) -> f64 {
        self.time_raw
    }

    /// Sets the raw double time accumulator (Arc `Time.setInternalTime`).
    pub fn set_internal_time(&mut self, time: f64) {
        self.time_raw = time;
        self.time = time as f32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Ported from `tests/src/test/java/ApplicationTests.java` `timers()`.
    #[test]
    fn timer_runs_after_two_updates() {
        let mut time = Time::new();
        let ran = Rc::new(Cell::new(false));
        let flag = ran.clone();
        time.run(1.9999f32, move || flag.set(true));

        time.update();
        assert!(!ran.get());
        time.update();
        assert!(ran.get());
    }

    /// Ported from `tests/src/test/java/ApplicationTests.java` `manyTimers()`.
    #[test]
    fn many_timers_same_update() {
        const RUNS: u32 = 100_000;
        let mut time = Time::new();
        let total = Rc::new(Cell::new(0u32));
        for _ in 0..RUNS {
            let total = total.clone();
            time.run(0.999f32, move || total.set(total.get() + 1));
        }
        assert_eq!(total.get(), 0);
        time.update();
        assert_eq!(total.get(), RUNS);
    }

    /// Ported from `tests/src/test/java/ApplicationTests.java` `longTimers()`.
    #[test]
    fn long_timers_catch_up() {
        let mut time = Time::new();
        // Burn an update with an enormous delta (Arc test uses Float.MAX_VALUE).
        time.set_delta_provider(|| f32::MAX);
        time.update();

        const STEPS: u32 = 100;
        let delay = 100_000f32;
        time.set_delta_provider(move || delay / STEPS as f32 + 0.01);

        const RUNS: u32 = 100_000;
        let total = Rc::new(Cell::new(0u32));
        for _ in 0..RUNS {
            let total = total.clone();
            time.run(delay, move || total.set(total.get() + 1));
        }
        assert_eq!(total.get(), 0);
        for _ in 0..STEPS {
            time.update();
        }
        assert_eq!(total.get(), RUNS);
    }

    #[test]
    fn delta_one_and_tick_counter() {
        let mut time = Time::new();
        assert_eq!(time.delta, 1.0);
        time.update();
        time.update();
        assert_eq!(time.ticks, 2);
        assert_eq!(time.time, 2.0);
    }
}
