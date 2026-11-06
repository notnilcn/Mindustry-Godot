// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Pure `BuildTurret` team rebuild-queue selection
//! (`world/blocks/defense/BuildTurret.java:158-176`).
//!
//! Java's `BuildTurretBuild.updateTile` walks `team.data().plans` for the first
//! entry that is within `range`, legal to place and affordable, then:
//!
//! ```java
//! unit.addBuild(new BuildPlan(block.x, block.y, block.rotation, block.block, block.config));
//! //shift build plan to tail so next unit builds something else
//! blocks.addLast(blocks.removeIndex(i));
//! lastPlan = block;
//! ```
//!
//! The ECS tick lives in `turrets/mod.rs::update_build` (plan 10/11 wiring); the
//! `within` / `Build.validPlace` / core-inventory predicates need the live
//! world, so they stay caller closures here. This module owns the pure
//! selection + tail rotation so the scan cannot drift from Java.

use std::collections::VecDeque;

use crate::game::teams::BlockPlan;
use crate::world::config::ConfigValue;
use crate::world::plan::BuildPlan;

/// `new BuildPlan(block.x, block.y, block.rotation, block.block, block.config)`
/// (`BuildTurret.java:169`): a team rebuild-queue entry as a unit build plan.
/// `Teams.BlockPlan.rotation` is an `i8`; negative values clamp to 0.
pub fn rebuild_plan(entry: &BlockPlan) -> BuildPlan {
    BuildPlan {
        x: i32::from(entry.x),
        y: i32::from(entry.y),
        rotation: entry.rotation.max(0) as u8,
        block: entry.block,
        config: entry.config.clone().unwrap_or(ConfigValue::None),
        // Team rebuild entries are always placements (`Teams.queueRebuild`).
        breaking: false,
    }
}

/// `BuildTurret.java:158-176`: the first queue entry that is still queued
/// (`!removed`), within range, and legal + affordable (the caller's
/// `Build.validPlace`/`infiniteResources`/`items.has` conjunction).
///
/// Returns the entry index (for [`rotate_plan_to_tail`]) and the converted
/// [`BuildPlan`].
pub fn scan_team_rebuild_queue(
    plans: &VecDeque<BlockPlan>,
    mut within: impl FnMut(&BuildPlan) -> bool,
    mut placeable: impl FnMut(&BuildPlan) -> bool,
) -> Option<(usize, BuildPlan)> {
    for (index, entry) in plans.iter().enumerate() {
        if entry.removed {
            continue;
        }
        let plan = rebuild_plan(entry);
        if within(&plan) && placeable(&plan) {
            return Some((index, plan));
        }
    }
    None
}

/// `blocks.addLast(blocks.removeIndex(i))` (`BuildTurret.java:172`): the
/// selected plan is moved to the tail so the next tower picks a different one.
/// Returns the rotated entry (Java keeps it as `lastPlan`).
pub fn rotate_plan_to_tail(plans: &mut VecDeque<BlockPlan>, index: usize) -> Option<BlockPlan> {
    let plan = plans.remove(index)?;
    plans.push_back(plan.clone());
    Some(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;

    fn entry(x: i16, y: i16, rotation: i8, block: BlockId) -> BlockPlan {
        BlockPlan {
            x,
            y,
            rotation,
            block,
            config: None,
            removed: false,
        }
    }

    /// `BuildTurret.java:158-176`: a removed entry is skipped and the first
    /// in-range + placeable one wins.
    #[test]
    fn scan_skips_removed_out_of_range_and_illegal_entries() {
        let wall = BlockId::new(1);
        let mut plans = VecDeque::from(vec![
            entry(0, 0, 0, wall),
            entry(40, 40, 1, wall),
            entry(6, 6, 3, wall),
        ]);
        plans[0].removed = true;
        // Only (6,6) is in range; (40,40) is in range but not placeable.
        let selected = scan_team_rebuild_queue(
            &plans,
            |plan| plan.x <= 8 && plan.y <= 8,
            |plan| plan.x != 40,
        );
        let (index, plan) = selected.expect("a plan is selected");
        assert_eq!(index, 2);
        assert_eq!((plan.x, plan.y), (6, 6));
        assert_eq!(plan.rotation, 3);
        assert!(!plan.breaking);
    }

    /// The range + `Build.validPlace` gates both have to pass; an affordable
    /// but illegal plan does not block a later legal one.
    #[test]
    fn scan_requires_both_range_and_placeable() {
        let wall = BlockId::new(1);
        let plans = VecDeque::from(vec![entry(4, 4, 0, wall), entry(5, 5, 0, wall)]);
        let selected = scan_team_rebuild_queue(&plans, |_| true, |plan| (plan.x, plan.y) != (4, 4))
            .expect("second plan");
        assert_eq!(selected.0, 1);
        // Nothing legal/affordable: no selection.
        assert!(scan_team_rebuild_queue(&plans, |_| true, |_| false).is_none());
    }

    /// `blocks.addLast(blocks.removeIndex(i))` keeps every entry and moves the
    /// built one to the tail.
    #[test]
    fn rotate_moves_the_built_plan_to_the_tail() {
        let wall = BlockId::new(1);
        let mut plans = VecDeque::from(vec![
            entry(1, 1, 0, wall),
            entry(2, 2, 0, wall),
            entry(3, 3, 0, wall),
        ]);
        let rotated = rotate_plan_to_tail(&mut plans, 0).expect("rotated");
        assert_eq!((rotated.x, rotated.y), (1, 1));
        let order: Vec<(i16, i16)> = plans.iter().map(|plan| (plan.x, plan.y)).collect();
        assert_eq!(order, vec![(2, 2), (3, 3), (1, 1)]);
        // Out-of-bounds index is a no-op.
        assert!(rotate_plan_to_tail(&mut plans, 99).is_none());
        assert_eq!(plans.len(), 3);
    }

    /// `BuildPlan(..., block.config)` carries the queue config verbatim.
    #[test]
    fn rebuild_plan_carries_config() {
        let wall = BlockId::new(1);
        let mut plan = entry(7, 9, -1, wall);
        plan.config = Some(ConfigValue::Bool(true));
        let converted = rebuild_plan(&plan);
        assert_eq!((converted.x, converted.y), (7, 9));
        assert_eq!(converted.rotation, 0, "negative rotation clamps");
        assert_eq!(converted.config, ConfigValue::Bool(true));
        assert_eq!(converted.block, wall);
    }
}
