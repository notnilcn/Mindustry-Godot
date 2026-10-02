// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RepairAI` (plan 11 §4.2). Ported from
//! `core/src/mindustry/ai/types/RepairAI.java`: the unit finds the nearest
//! damaged friendly building, walks into range and heals it. The retreat-under-
//! fire branch reads plan-10 damage events (marked below).

use bevy_ecs::entity::Entity;

use crate::entities::comp::building::Building;
use crate::entities::comp::{Health, Pos, TeamComp};

use super::super::ai_controller::AiCtx;
use super::super::types::ground::tile_center;

/// Heal applied per tick when in range (`RepairAI.healAmount`-adjacent).
pub const REPAIR_AMOUNT: f32 = 1.0;

/// `RepairAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct RepairAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current repair target building.
    pub target: Option<Entity>,
    /// Whether the unit healed this tick.
    pub repairing: bool,
}

/// `RepairAI.updateUnit`: acquire a damaged ally and heal it.
///
/// Returns whether a target was found.
pub fn update_repair(ctx: &mut AiCtx, unit: Entity, state: &mut RepairAi) -> bool {
    let (range, build_range) = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .map(|def| (def.range.max(60.0), def.build_range.max(8.0)))
        .unwrap_or((60.0, 8.0));
    let team = ctx
        .world
        .get::<TeamComp>(unit)
        .map(|team| team.team)
        .unwrap_or(0);
    let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
        return false;
    };

    // Pick the nearest damaged friendly building within `range`.
    let candidates: Vec<(Entity, f32, crate::world::TilePos)> = {
        let mut query = ctx.world.query::<(Entity, &Building, &Health, &TeamComp)>();
        query
            .iter(ctx.world)
            .filter(|(_, _, health, bteam)| bteam.team == team && health.damaged())
            .filter_map(|(entity, building, _, _)| {
                let (cx, cy) = tile_center(building.tile.x() as i32, building.tile.y() as i32);
                let dx = cx - pos.x;
                let dy = cy - pos.y;
                let dist2 = dx * dx + dy * dy;
                (dist2 <= range * range).then_some((entity, dist2, building.tile))
            })
            .collect()
    };
    let target = candidates.into_iter().min_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.index().cmp(&b.0.index()))
    });

    let Some((building, _, tile)) = target else {
        state.target = None;
        state.repairing = false;
        ctx.stop_shooting(unit);
        return false;
    };
    state.target = Some(building);

    let (cx, cy) = tile_center(tile.x() as i32, tile.y() as i32);
    let dx = cx - pos.x;
    let dy = cy - pos.y;
    let dist = (dx * dx + dy * dy).sqrt();
    state.repairing = dist <= build_range;
    if state.repairing {
        if let Some(mut health) = ctx.world.get_mut::<Health>(building) {
            health.health = (health.health + REPAIR_AMOUNT).min(health.max_health);
        }
    } else {
        ctx.pathfind(unit, tile, build_range);
    }
    // TODO(plan 10/12): retreat when under fire (`RepairAI` reads
    // `wasDamaged`/`lastDamageTime` and flees; needs the damage event feed).
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn repair_heals_a_damaged_building() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.build.place(6, 6, wall, 0, true));
        let building = harness.build.build_at(6, 6).expect("building");
        {
            let mut health = harness.build.world.get_mut::<Health>(building).unwrap();
            health.health = health.max_health * 0.5;
        }
        let unit = harness.spawn("dagger", 0, 48.0, 48.0, 0.0).expect("dagger");
        let mut state = RepairAi {
            unit: Some(unit),
            ..Default::default()
        };
        let mut healed = false;
        for _ in 0..600 {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            if update_repair(&mut ctx, unit, &mut state) && state.repairing {
                healed = true;
                break;
            }
        }
        assert!(healed, "repair unit reached and healed the wall");
        assert!(harness.build.world.get::<Health>(building).unwrap().health > 25.0);
    }
}
