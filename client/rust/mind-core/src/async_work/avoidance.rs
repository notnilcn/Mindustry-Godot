// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `AvoidanceProcess` — deterministic per-shard avoidance work (plan 05 §3.10).
//!
//! Ported from `core/src/mindustry/async/AvoidanceProcess.java`. Plan 11
//! supplies the pathfinder sampling; this owns the shard/join contract and a
//! stand-in max-reduction used for determinism verification.

use std::sync::Mutex;

use bevy_ecs::world::World;

use super::{AsyncProcess, PhysicsSnapshot, ShardId};

/// Deterministic avoidance process (stub algorithm: per-shard slot max).
pub struct AvoidanceProcess {
    shards: usize,
    inputs: Vec<u32>,
    outputs: Mutex<Vec<u64>>,
}

impl std::fmt::Debug for AvoidanceProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AvoidanceProcess")
            .field("shards", &self.shards)
            .finish()
    }
}

impl AvoidanceProcess {
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

impl AsyncProcess for AvoidanceProcess {
    fn name(&self) -> &'static str {
        "avoidance"
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
        let max = self
            .inputs
            .iter()
            .enumerate()
            .filter(|(index, _)| index % self.shards == shard)
            .map(|(_, value)| u64::from(*value))
            .max()
            .unwrap_or(0);
        if let Ok(mut outputs) = self.outputs.lock()
            && let Some(slot) = outputs.get_mut(shard)
        {
            *slot = max;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avoidance_is_shard_deterministic() {
        let slots: Vec<u32> = (0..16).collect();
        let mut one = AvoidanceProcess::new(1);
        one.begin(&PhysicsSnapshot::new(slots.clone()));
        one.process(0);
        let mut four = AvoidanceProcess::new(4);
        four.begin(&PhysicsSnapshot::new(slots));
        for shard in 0..4 {
            four.process(shard);
        }
        assert_eq!(one.shard_outputs(), vec![15]);
        assert_eq!(four.shard_outputs(), vec![12, 13, 14, 15]);
        assert_eq!(four.total(), 54);
    }
}
