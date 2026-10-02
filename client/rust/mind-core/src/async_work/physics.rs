// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PhysicsProcess` — deterministic per-shard physics work (plan 05 §3.10).
//!
//! Ported from `core/src/mindustry/async/PhysicsProcess.java`. The real
//! collision resolution lands with plan 10; this process owns the snapshot,
//! shard partitioning and ordered join contract that plan 10 fills in.

use std::sync::Mutex;

use bevy_ecs::world::World;

use super::{AsyncProcess, ShardId};

/// Immutable per-tick physics input (entity slots, in group order).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhysicsSnapshot {
    slots: Vec<u32>,
}

impl PhysicsSnapshot {
    /// Creates a snapshot from slot values.
    pub fn new(slots: Vec<u32>) -> Self {
        Self { slots }
    }

    /// The slot values.
    pub fn slots(&self) -> &[u32] {
        &self.slots
    }

    /// Number of slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the snapshot is empty.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

/// Deterministic physics process (stub algorithm: per-shard slot sum).
pub struct PhysicsProcess {
    shards: usize,
    inputs: Vec<u32>,
    outputs: Mutex<Vec<u64>>,
}

impl std::fmt::Debug for PhysicsProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PhysicsProcess")
            .field("shards", &self.shards)
            .finish()
    }
}

impl PhysicsProcess {
    /// Creates a process with `shards` slots.
    pub fn new(shards: usize) -> Self {
        let shards = shards.max(1);
        Self {
            shards,
            inputs: Vec::new(),
            outputs: Mutex::new(vec![0; shards]),
        }
    }

    /// Per-shard outputs after `end`.
    pub fn shard_outputs(&self) -> Vec<u64> {
        self.outputs
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// Sum of all shard outputs (joined result).
    pub fn total(&self) -> u64 {
        self.shard_outputs().iter().sum()
    }
}

impl AsyncProcess for PhysicsProcess {
    fn name(&self) -> &'static str {
        "physics"
    }

    fn reset(&mut self) {
        self.inputs.clear();
        if let Ok(mut outputs) = self.outputs.lock() {
            outputs.iter_mut().for_each(|slot| *slot = 0);
        }
    }

    fn begin(&mut self, snapshot: &PhysicsSnapshot) {
        self.inputs = snapshot.slots().to_vec();
        if let Ok(mut outputs) = self.outputs.lock() {
            outputs.clear();
            outputs.resize(self.shards, 0);
        }
    }

    fn process(&self, shard: ShardId) {
        let sum: u64 = self
            .inputs
            .iter()
            .enumerate()
            .filter(|(index, _)| index % self.shards == shard)
            .map(|(_, value)| u64::from(*value))
            .sum();
        if let Ok(mut outputs) = self.outputs.lock()
            && let Some(slot) = outputs.get_mut(shard)
        {
            *slot = sum;
        }
    }

    fn end(&mut self, _world: &mut World) {}

    fn should_process(&self) -> bool {
        !self.inputs.is_empty()
    }

    fn state(&self) -> u64 {
        self.total()
    }
}
