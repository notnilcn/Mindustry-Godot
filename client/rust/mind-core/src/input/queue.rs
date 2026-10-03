// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Minimal `BuildQueue` seam over plan 11's `BuilderComp.plans` (plan 15 §3.7).
//!
//! Plan 11 ships only the `BuilderComp` size field today; this module provides
//! the documented insertion/replacement semantics input needs
//! (`BuilderComp.addBuild` same-position replacement, front/back insertion) so
//! plan 15 never appends to sim plans directly. When plan 11 exports the real
//! queue, this type is replaced in place without changing call sites.
//!
//! Construct-progress carry-over (`place.progress = tile.build.progress`) needs
//! plan 07's `ConstructState`; it is the named plan-11 seam.

use crate::content::BlockId;

use super::plan::ClientPlan;

/// FIFO/priority build-plan queue (`BuilderComp.plans`).
#[derive(Debug, Clone, Default)]
pub struct BuildQueue {
    plans: Vec<ClientPlan>,
}

/// Outcome of one [`BuildQueue::add_build`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AddOutcome {
    /// Whether a same-position plan was replaced.
    pub replaced: bool,
    /// Index the plan was inserted at.
    pub index: usize,
}

impl BuildQueue {
    /// Empty queue.
    pub fn new() -> Self {
        Self::default()
    }

    /// `BuilderComp.addBuild(plan, front)`: removes a same-position plan
    /// (replacement), then inserts at the front (`front = true`) or back.
    pub fn add_build(&mut self, plan: ClientPlan, front: bool) -> AddOutcome {
        let replaced = self.remove_plan(plan.x, plan.y);
        let index = if front {
            self.plans.insert(0, plan);
            0
        } else {
            self.plans.push(plan);
            self.plans.len() - 1
        };
        AddOutcome { replaced, index }
    }

    /// Removes the plan at `(x, y)`; returns whether one existed.
    pub fn remove_plan(&mut self, x: i32, y: i32) -> bool {
        let before = self.plans.len();
        self.plans.retain(|plan| plan.x != x || plan.y != y);
        self.plans.len() != before
    }

    /// Removes every deconstruction plan (`clear_building`).
    pub fn remove_breaking(&mut self) {
        self.plans.retain(|plan| !plan.breaking);
    }

    /// Clears the queue.
    pub fn clear(&mut self) {
        self.plans.clear();
    }

    /// Number of queued plans.
    pub fn len(&self) -> usize {
        self.plans.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.plans.is_empty()
    }

    /// Iterates in queue order.
    pub fn iter(&self) -> impl Iterator<Item = &ClientPlan> + '_ {
        self.plans.iter()
    }

    /// The queued plan at `(x, y)`.
    pub fn get(&self, x: i32, y: i32) -> Option<&ClientPlan> {
        self.plans.iter().find(|plan| plan.x == x && plan.y == y)
    }

    /// The backing slice (plan 11's `Player.plans` shape).
    pub fn plans(&self) -> &[ClientPlan] {
        &self.plans
    }

    /// Converts every plan into plan 07's `BuildPlan`.
    pub fn to_build_plans(&self) -> Vec<crate::world::plan::BuildPlan> {
        self.plans.iter().map(ClientPlan::to_build_plan).collect()
    }

    /// Reserved for the plan-11 `BuilderComp` accessor seam.
    pub fn block_of(&self, x: i32, y: i32) -> Option<BlockId> {
        self.get(x, y).map(|plan| plan.block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_replaces_same_position() {
        let mut queue = BuildQueue::new();
        queue.add_build(ClientPlan::place(1, 1, 0, BlockId::STONE_WALL), false);
        let outcome = queue.add_build(ClientPlan::place(1, 1, 2, BlockId::STONE_WALL), false);
        assert!(outcome.replaced);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.get(1, 1).map(|plan| plan.rotation), Some(2));
    }

    #[test]
    fn front_and_back_insertion() {
        let mut queue = BuildQueue::new();
        queue.add_build(ClientPlan::place(1, 1, 0, BlockId::STONE_WALL), false);
        queue.add_build(ClientPlan::place(2, 1, 0, BlockId::STONE_WALL), true);
        let order: Vec<(i32, i32)> = queue.iter().map(|plan| (plan.x, plan.y)).collect();
        assert_eq!(order, vec![(2, 1), (1, 1)]);
        assert_eq!(queue.to_build_plans().len(), 2);
        assert!(queue.remove_plan(1, 1));
        assert!(!queue.remove_plan(1, 1));
    }

    #[test]
    fn breaking_plans_removed() {
        let mut queue = BuildQueue::new();
        queue.add_build(ClientPlan::place(1, 1, 0, BlockId::STONE_WALL), false);
        queue.add_build(ClientPlan::break_plan(2, 2), false);
        queue.remove_breaking();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.block_of(1, 1), Some(BlockId::STONE_WALL));
    }
}
