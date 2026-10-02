// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `AsyncCore` — deterministic physics/avoidance workers (plan 05 §3.10).
//!
//! Ported from `core/src/mindustry/async/AsyncCore.java`. No tokio: workers are
//! `std::thread` scoped to the run call. Determinism comes from *disjoint,
//! shard-indexed outputs* joined in ascending shard order, so the result is
//! identical for any worker count.

pub mod avoidance;
pub mod physics;

pub use avoidance::AvoidanceProcess;
pub use physics::{PhysicsProcess, PhysicsSnapshot};

use bevy_ecs::world::World;

/// A shard index (`slot % shards`).
pub type ShardId = usize;

/// One async process (port of `AsyncProcess`).
pub trait AsyncProcess: Send + Sync {
    /// Process name (`physics`/`avoidance`).
    fn name(&self) -> &'static str;
    /// One-time init (`AsyncCore.init`).
    fn init(&mut self, _world: &mut World) {}
    /// Clears per-run state (`AsyncCore.reset`).
    fn reset(&mut self);
    /// Builds disjoint work items for the next run.
    fn begin(&mut self, snapshot: &PhysicsSnapshot);
    /// Runs one shard on a worker (must write only its own shard slot).
    fn process(&self, shard: ShardId);
    /// Joins results in ascending shard order.
    fn end(&mut self, _world: &mut World);
    /// Whether this process has work this tick.
    fn should_process(&self) -> bool;
    /// Deterministic digest of the joined state (checksum/determinism input).
    fn state(&self) -> u64 {
        0
    }
}

/// Owns the worker processes and dispatches shards.
#[derive(Default)]
pub struct AsyncCore {
    processes: Vec<Box<dyn AsyncProcess>>,
    shards: usize,
    last_workers: usize,
}

impl std::fmt::Debug for AsyncCore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncCore")
            .field("processes", &self.processes.len())
            .field("shards", &self.shards)
            .finish()
    }
}

impl AsyncCore {
    /// Creates a core with `shards` disjoint slots (`>= 1`).
    pub fn new(shards: usize) -> Self {
        Self {
            processes: Vec::new(),
            shards: shards.max(1),
            last_workers: 1,
        }
    }

    /// Registers a process.
    pub fn add(&mut self, process: Box<dyn AsyncProcess>) {
        self.processes.push(process);
    }

    /// Number of registered processes.
    pub fn process_count(&self) -> usize {
        self.processes.len()
    }

    /// Shard count.
    pub const fn shards(&self) -> usize {
        self.shards
    }

    /// Worker count used by the last run.
    pub const fn last_workers(&self) -> usize {
        self.last_workers
    }

    /// Initializes all processes (`AsyncCore.init`).
    pub fn init(&mut self, world: &mut World) {
        for process in &mut self.processes {
            process.init(world);
        }
    }

    /// Resets all processes (`AsyncCore.reset`).
    pub fn reset(&mut self) {
        for process in &mut self.processes {
            process.reset();
        }
    }

    /// Begins a run: every process builds its disjoint work items.
    pub fn begin(&mut self, snapshot: &PhysicsSnapshot) {
        for process in &mut self.processes {
            process.begin(snapshot);
        }
    }

    /// Runs all shards across `workers` OS threads. Outputs are disjoint, so
    /// the joined result is independent of `workers`.
    pub fn run(&self, workers: usize) -> usize {
        let workers = workers.clamp(1, self.shards);
        std::thread::scope(|scope| {
            for worker in 0..workers {
                let processes = &self.processes;
                let shards = self.shards;
                scope.spawn(move || {
                    let mut shard = worker;
                    while shard < shards {
                        for process in processes {
                            if process.should_process() {
                                process.process(shard);
                            }
                        }
                        shard += workers;
                    }
                });
            }
        });
        workers
    }

    /// Joins results in ascending shard order (`AsyncCore.end`).
    pub fn end(&mut self, world: &mut World) {
        for process in &mut self.processes {
            process.end(world);
        }
    }

    /// Full pipeline: begin → run → end.
    pub fn step(&mut self, world: &mut World, snapshot: &PhysicsSnapshot, workers: usize) {
        self.begin(snapshot);
        self.last_workers = self.run(workers);
        self.end(world);
    }

    /// Combined deterministic digest of all process states.
    pub fn state_sum(&self) -> u64 {
        self.processes.iter().map(|process| process.state()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_join_order_and_worker_equality() {
        let slots: Vec<u32> = (0..64).collect();
        let snapshot = PhysicsSnapshot::new(slots);
        let mut world = World::new();

        let mut core = AsyncCore::new(4);
        core.add(Box::new(PhysicsProcess::new(4)));
        core.step(&mut world, &snapshot, 1);
        assert_eq!(core.process_count(), 1);
        assert_eq!(core.last_workers(), 1);

        // Same fixed shards, 4 workers → identical joined state.
        let mut threaded = AsyncCore::new(4);
        threaded.add(Box::new(PhysicsProcess::new(4)));
        threaded.step(&mut world, &snapshot, 4);
        assert_eq!(threaded.last_workers(), 4);
        assert_eq!(core.state_sum(), threaded.state_sum());
        // Sum of 0..64 == 2016.
        assert_eq!(core.state_sum(), 2016);
    }

    #[test]
    fn shard_partition_is_deterministic() {
        // Direct process test: outputs depend only on shard assignment.
        let slots: Vec<u32> = (0..100).collect();
        let mut one = PhysicsProcess::new(1);
        one.begin(&PhysicsSnapshot::new(slots.clone()));
        for shard in 0..1 {
            one.process(shard);
        }
        one.end(&mut World::new());

        let mut four = PhysicsProcess::new(4);
        four.begin(&PhysicsSnapshot::new(slots.clone()));
        for shard in 0..4 {
            four.process(shard);
        }
        four.end(&mut World::new());

        assert_eq!(one.total(), four.total());
        assert_ne!(one.shard_outputs(), four.shard_outputs());
        assert_eq!(one.shard_outputs(), vec![4950]);
    }
}
