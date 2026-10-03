// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RepairAI` (plan 11 §4.2). Ported from
//! `core/src/mindustry/ai/types/RepairAI.java`: the unit finds the nearest
//! damaged friendly building, walks into range and heals it. The retreat-under-
//! fire branch reads plan-10 damage events (marked below).

use bevy_ecs::entity::Entity;

use crate::content::BlockFlag;
use crate::entities::comp::building::Building;
use crate::entities::comp::{Health, Pos, TeamComp};
use crate::world::BlockTable;

use super::super::ai_controller::AiCtx;
use super::super::types::ground::tile_center;

/// Heal applied per tick when in range (`RepairAI.healAmount`-adjacent).
pub const REPAIR_AMOUNT: f32 = 1.0;
/// `RepairAI.retreatDst`: stop distance from the friendly core.
pub const RETREAT_DST: f32 = 160.0;
/// `RepairAI.fleeRange`: enemy scan radius while idle.
pub const FLEE_RANGE: f32 = 310.0;
/// `RepairAI.retreatDelay` (`Time.toSeconds * 3` at 60 Hz).
pub const RETREAT_DELAY: f32 = 180.0;
/// `timerTarget4` interval for the enemy scan.
pub const AVOID_INTERVAL: f32 = 40.0;

/// `RepairAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct RepairAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current repair target building.
    pub target: Option<Entity>,
    /// Whether the unit healed this tick.
    pub repairing: bool,
    /// Idle timer (`retreatTimer`).
    pub retreat_timer: f32,
    /// Nearest enemy while idle (`avoid`).
    pub avoid: Option<Entity>,
    /// Countdown to the next enemy scan.
    pub avoid_timer: f32,
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
        // Idle retreat (`RepairAI.updateMovement` else-branch): scan for an enemy
        // every 40 ticks, and after `retreatDelay` of idleness flee to the closest
        // friendly core. This reads enemy proximity, not a damage feed.
        state.avoid_timer -= 1.0;
        if state.avoid_timer <= 0.0 {
            state.avoid_timer = AVOID_INTERVAL;
            state.avoid = ctx.find_target(unit, FLEE_RANGE, true, true);
        }
        state.retreat_timer += 1.0;
        if state.retreat_timer >= RETREAT_DELAY
            && state.avoid.is_some()
            && let Some((core_x, core_y)) = closest_core(ctx, unit, pos.x, pos.y, team)
        {
            let dist = ((core_x - pos.x).powi(2) + (core_y - pos.y).powi(2)).sqrt();
            if dist > RETREAT_DST {
                ctx.retreat(unit, core_x, core_y);
            }
        }
        return false;
    };
    state.target = Some(building);
    state.retreat_timer = 0.0;

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
    // Owner note (plan 10/11): `RepairAI.updateMovement`'s idle retreat reads
    // `wasDamaged`/`lastDamageTime` (a plan-10 damage-event feed) and flees to
    // `unit.closestCore()` after `retreatDelay = 3 s`. The heal/target half is
    // live; the retreat half is deferred until the unit damage-timer feed lands.
    true
}

/// Nearest friendly core building center (`Unit.closestCore`).
fn closest_core(ctx: &mut AiCtx, _unit: Entity, x: f32, y: f32, team: u8) -> Option<(f32, f32)> {
    let candidates: Vec<(crate::content::BlockId, crate::world::TilePos)> = {
        let mut query = ctx.world.query::<(Entity, &Building, &TeamComp)>();
        query
            .iter(ctx.world)
            .filter(|(_, _, bteam)| bteam.team == team)
            .map(|(_, building, _)| (building.block, building.tile))
            .collect()
    };
    let table = ctx.world.get_resource::<BlockTable>()?;
    let mut best: Option<(f32, (f32, f32))> = None;
    for (block, tile) in candidates {
        let is_core = table
            .get(block)
            .map(|inst| inst.def.flags.contains(&BlockFlag::Core))
            .unwrap_or(false);
        if !is_core {
            continue;
        }
        let (cx, cy) = tile_center(tile.x() as i32, tile.y() as i32);
        let dist2 = (cx - x).powi(2) + (cy - y).powi(2);
        if best.is_none_or(|(best_dist, _)| dist2 < best_dist) {
            best = Some((dist2, (cx, cy)));
        }
    }
    best.map(|(_, pos)| pos)
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

    #[test]
    fn repair_retreats_to_core_when_idle_under_threat() {
        let mut harness = UnitHarness::new(64, 64, 1);
        let core = harness.content().block_id("core-shard").expect("core");
        assert!(harness.build.place(2, 32, core, 0, true), "place core");
        let _enemy = harness
            .spawn("dagger", 1, 200.0, 256.0, 0.0)
            .expect("enemy");
        let unit = harness.spawn("dagger", 0, 256.0, 256.0, 0.0).expect("unit");
        let start = harness.snapshot(unit).expect("alive");
        let mut state = RepairAi {
            unit: Some(unit),
            ..Default::default()
        };
        for _ in 0..(RETREAT_DELAY as u32 + 240) {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            update_repair(&mut ctx, unit, &mut state);
        }
        let end = harness.snapshot(unit).expect("alive");
        assert!(
            end.x < start.x - 10.0,
            "idle repair unit retreated toward the core ({} -> {})",
            start.x,
            end.x
        );
    }
}
