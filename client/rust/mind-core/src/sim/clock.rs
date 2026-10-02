// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SimClock` / `RunQueue` — deterministic tick clock and delayed tasks.
//!
//! Ported from Arc `arc-core/src/arc/util/Time.java` (`delta`, `time`,
//! `globalTime`, `run(delay, task)`, `clear`). `Time.delta` is fixed at `1.0`
//! inside a tick (plan 05 §3.8); delays are in ticks. The run queue is a
//! `BinaryHeap<Reverse<(finish_units, seq)>>` with lazy cancellation, so task
//! order is deterministic by `(finish, sequence)`.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use bevy_ecs::world::World;

/// Internal time resolution: micro-ticks per tick (exact for `f32` tick delays).
const UNITS_PER_TICK: u64 = 1_000_000;

/// A delayed task.
type Task = Box<dyn FnOnce(&mut World) + 'static>;

/// Deterministic delayed-run queue (port of Arc `Time.timers`/`manyTimers`).
#[derive(Default)]
pub struct RunQueue {
    heap: BinaryHeap<Reverse<(u64, u64)>>,
    tasks: BTreeMap<u64, Task>,
    next_seq: u64,
}

impl std::fmt::Debug for RunQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RunQueue")
            .field("pending", &self.tasks.len())
            .finish()
    }
}

impl RunQueue {
    /// Creates an empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Schedules `task` to run when `now_units <= finish_units`.
    pub fn schedule(&mut self, finish_units: u64, task: Task) -> u64 {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        self.heap.push(Reverse((finish_units, seq)));
        self.tasks.insert(seq, task);
        seq
    }

    /// Lazily cancels a scheduled task.
    pub fn cancel(&mut self, seq: u64) {
        self.tasks.remove(&seq);
    }

    /// Number of pending tasks (cancelled entries excluded).
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Whether no tasks are pending.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Removes and returns every task due at `now_units`, in `(finish, seq)` order.
    pub fn drain_due(&mut self, now_units: u64) -> Vec<Task> {
        let mut due = Vec::new();
        while let Some(Reverse((finish, seq))) = self.heap.peek().copied() {
            if finish > now_units {
                break;
            }
            self.heap.pop();
            if let Some(task) = self.tasks.remove(&seq) {
                due.push(task);
            }
        }
        due
    }
}

/// The simulation clock (owned by [`crate::sim::Sim`]).
#[derive(Debug, Default)]
pub struct SimClock {
    /// Fixed delta (`1.0` inside a tick).
    pub delta: f32,
    /// Accumulated time in ticks.
    pub time: f64,
    /// Accumulated time, including while paused (Arc `globalTime`).
    pub global_time: f64,
    /// Number of completed ticks (`GameState.updateId` mirror).
    pub update_id: u64,
    units: u64,
    runs: RunQueue,
}

impl SimClock {
    /// Creates a clock at tick 0.
    pub fn new() -> Self {
        Self {
            delta: 1.0,
            ..Self::default()
        }
    }

    /// Schedules `task` after `delay_ticks` ticks from now, returning its
    /// cancellation handle.
    pub fn run(&mut self, delay_ticks: f32, task: impl FnOnce(&mut World) + 'static) -> u64 {
        let delay = if delay_ticks.is_finite() && delay_ticks > 0.0 {
            (f64::from(delay_ticks) * UNITS_PER_TICK as f64).round() as u64
        } else {
            0
        };
        self.runs
            .schedule(self.units.saturating_add(delay), Box::new(task))
    }

    /// Cancels a task by the handle returned from a prior schedule.
    pub fn cancel(&mut self, seq: u64) {
        self.runs.cancel(seq);
    }

    /// Number of pending delayed tasks.
    pub fn runs_len(&self) -> usize {
        self.runs.len()
    }

    /// Advances one tick and runs every due task (Arc `Time.update()`).
    pub fn update(&mut self, world: &mut World) {
        self.delta = 1.0;
        self.units = self.units.saturating_add(UNITS_PER_TICK);
        self.time = self.units as f64 / UNITS_PER_TICK as f64;
        self.global_time += 1.0;
        self.update_id = self.update_id.wrapping_add(1);
        let due = self.runs.drain_due(self.units);
        for task in due {
            task(world);
        }
    }

    /// Clears queued tasks (Arc `Time.clear()`).
    pub fn clear(&mut self) {
        self.runs = RunQueue::new();
    }

    /// Resets the clock to tick 0 (used by `reset`).
    pub fn reset(&mut self) {
        self.delta = 1.0;
        self.time = 0.0;
        self.global_time = 0.0;
        self.update_id = 0;
        self.units = 0;
        self.runs = RunQueue::new();
    }

    /// Internal time in micro-ticks (checksum input).
    pub const fn time_units(&self) -> u64 {
        self.units
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// Ported from `ApplicationTests.timers`.
    #[test]
    fn timers() {
        let mut clock = SimClock::new();
        let mut world = World::new();
        let ran = Arc::new(AtomicU32::new(0));
        let flag = ran.clone();
        clock.run(1.9999, move |_| {
            flag.fetch_add(1, Ordering::SeqCst);
        });
        clock.update(&mut world);
        assert_eq!(ran.load(Ordering::SeqCst), 0);
        clock.update(&mut world);
        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    /// Ported from `ApplicationTests.manyTimers`.
    #[test]
    fn many_timers() {
        let mut clock = SimClock::new();
        let mut world = World::new();
        let total = Arc::new(AtomicU32::new(0));
        for _ in 0..100_000 {
            let total = total.clone();
            clock.run(0.999, move |_| {
                total.fetch_add(1, Ordering::SeqCst);
            });
        }
        assert_eq!(total.load(Ordering::SeqCst), 0);
        clock.update(&mut world);
        assert_eq!(total.load(Ordering::SeqCst), 100_000);
    }

    /// Ported from `ApplicationTests.longTimers`.
    #[test]
    fn long_timers() {
        let mut clock = SimClock::new();
        let mut world = World::new();
        let total = Arc::new(AtomicU32::new(0));
        let delay = 100.0f32;
        for _ in 0..10_000 {
            let total = total.clone();
            clock.run(delay, move |_| {
                total.fetch_add(1, Ordering::SeqCst);
            });
        }
        for _ in 0..100 {
            clock.update(&mut world);
        }
        assert_eq!(total.load(Ordering::SeqCst), 10_000);
    }

    #[test]
    fn cancel_is_lazy_and_deterministic() {
        let mut clock = SimClock::new();
        let mut world = World::new();
        let total = Arc::new(AtomicU32::new(0));
        let first = total.clone();
        clock.run(1.0, move |_| {
            first.fetch_add(1, Ordering::SeqCst);
        });
        let cancelled = total.clone();
        let seq = clock.run(1.0, move |_| {
            cancelled.fetch_add(100, Ordering::SeqCst);
        });
        clock.cancel(seq);
        clock.update(&mut world);
        clock.update(&mut world);
        assert_eq!(total.load(Ordering::SeqCst), 1);
    }
}
