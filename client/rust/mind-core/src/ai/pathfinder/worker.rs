// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic incremental pathfinder budget (plan 11 §3.7/§6.4, OD-11-A).
//!
//! Replaces upstream's wall-clock `maxUpdate = 8 ms` daemon budgets with fixed
//! work units per sim tick so checksums are stable when `workers` changes. M0
//! computes fields synchronously and records the queue shape here; M3 consumes
//! the budget in [`PathfindQueue`](super::queue::PathfindQueue) order.

/// Frontier pops processed per tick (`FLOWFIELD_NODES_PER_TICK`).
pub const FLOWFIELD_NODES_PER_TICK: u32 = 6_000;
/// Main-list refresh interval in ticks (`REFRESH_INTERVAL_TICKS = 100 ms`).
pub const REFRESH_INTERVAL_TICKS: u64 = 6;
/// Control-pathfinder node budget per tick (`CONTROL_NODES_PER_TICK`).
pub const CONTROL_NODES_PER_TICK: u32 = 8_000;
/// Control invalidate interval in ticks (`CONTROL_INVALIDATE_INTERVAL_TICKS`).
pub const CONTROL_INVALIDATE_INTERVAL_TICKS: u64 = 60;
/// Request idle timeout in update ids (`REQUEST_IDLE_TIMEOUT_TICKS`).
pub const REQUEST_IDLE_TIMEOUT_TICKS: u64 = 10;
/// Field idle timeout in update ids (`FIELD_IDLE_TIMEOUT_TICKS`).
pub const FIELD_IDLE_TIMEOUT_TICKS: u64 = 30;
/// Control-pathfinder cluster size (`ControlPathfinder.clusterSize`).
pub const CLUSTER_SIZE: i32 = 12;
/// Max `CommandAI` command queue length.
pub const MAX_COMMAND_QUEUE: usize = 50;
/// `CommandAI` avoidance update interval in ticks (`avoidInterval`).
pub const AVOID_INTERVAL: u64 = 10;

/// One unit of deterministic pathfinder work (FIFO consumed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowfieldOp {
    /// Register/build a field for `(team, cost)`.
    Register {
        /// Team id.
        team: u8,
        /// Cost id.
        cost: u8,
    },
    /// Refresh the targets of an existing field.
    RefreshTargets {
        /// Team id.
        team: u8,
        /// Cost id.
        cost: u8,
    },
    /// Advance a field by up to `budget` frontier pops.
    Step {
        /// Team id.
        team: u8,
        /// Cost id.
        cost: u8,
        /// Node budget for this step.
        budget: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_constants_match_plan() {
        assert_eq!(FLOWFIELD_NODES_PER_TICK, 6_000);
        assert_eq!(REFRESH_INTERVAL_TICKS, 6);
        assert_eq!(CONTROL_NODES_PER_TICK, 8_000);
        assert_eq!(CLUSTER_SIZE, 12);
        assert_eq!(MAX_COMMAND_QUEUE, 50);
    }
}
