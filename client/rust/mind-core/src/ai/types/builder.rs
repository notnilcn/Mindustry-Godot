// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuilderAI` (plan 11 §4.2). Ported from
//! `core/src/mindustry/ai/types/BuilderAI.java`. The plan-12 `Schematic`/build
//! plan machinery is not on this branch, so this milestone implements the
//! deterministic movement half: the builder paths to its assigned build target
//! and repairs the placed building once in range. `onlyAssist` gates plan
//! placement (plan 12).

use bevy_ecs::entity::Entity;

use crate::entities::comp::Health;
use crate::entities::comp::building::Building;
use crate::world::TilePos;

use super::super::ai_controller::AiCtx;
use super::super::controller::ControllerSlot;

/// `BuilderAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct BuilderAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Assist-only builder (`BuilderAI(true, ...)`; never places new blocks).
    pub only_assist: bool,
    /// Current build target tile.
    pub target: Option<TilePos>,
    /// Whether the builder is in range of its target this tick.
    pub in_range: bool,
}

/// `BuilderAI.defaultBehavior`: walk to the assigned plan and repair it.
///
/// Returns `true` when the builder reached (and healed) its target.
pub fn update_builder(ctx: &mut AiCtx, unit: Entity, state: &mut BuilderAi) -> bool {
    let assigned = state.target.or_else(|| {
        ctx.world
            .get::<ControllerSlot>(unit)
            .and_then(|slot| slot.target)
    });
    let Some(target) = assigned else {
        ctx.stop_shooting(unit);
        return false;
    };
    state.target = Some(target);
    let build_range = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .map(|def| def.build_range.max(8.0))
        .unwrap_or(40.0);

    // Heal an existing damaged building at the target tile (`Build.validPlace`
    // placement itself is plan 12's).
    if let Some(building) = find_building_at(ctx, target)
        && let Some(mut health) = ctx.world.get_mut::<Health>(building)
    {
        let before = health.health;
        health.health = (health.health + 1.0).min(health.max_health);
        state.in_range = before != health.health;
    }

    let arrived = ctx.pathfind(unit, target, build_range);
    state.in_range |= arrived;
    arrived
}

/// Finds the building entity whose center tile is `target`.
fn find_building_at(ctx: &mut AiCtx, target: TilePos) -> Option<Entity> {
    let mut query = ctx.world.query::<(Entity, &Building)>();
    query
        .iter(ctx.world)
        .find(|(_, building)| building.tile == target)
        .map(|(entity, _)| entity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn builder_paths_to_assigned_target() {
        let mut harness = UnitHarness::new(64, 64, 1);
        let unit = harness.spawn("dagger", 0, 44.0, 44.0, 0.0).expect("dagger");
        harness.command_move(unit, 60, 60);
        let mut state = BuilderAi {
            unit: Some(unit),
            ..Default::default()
        };
        let mut arrived = false;
        for _ in 0..1500 {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            if update_builder(&mut ctx, unit, &mut state) {
                arrived = true;
                break;
            }
        }
        assert!(arrived);
    }
}
