// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RepairAI` (plan G01 T10b), ported from
//! `core/src/mindustry/ai/types/RepairAI.java:15-80`. The mega's heal bolts do
//! the actual repair (`BulletType.hitTile`, T4); this controller only picks the
//! closest damaged friendly building, aims the mount at it, closes to
//! `range * 0.65` and retreats to the core when idle under fire.

use bevy_ecs::entity::Entity;

use crate::entities::comp::building::Building;
use crate::entities::comp::{Health, Pos, TeamComp};

use super::super::ai_controller::AiCtx;

/// `RepairAI.retreatDst`: stop distance from the friendly core.
pub const RETREAT_DST: f32 = 160.0;
/// `RepairAI.fleeRange`: enemy scan radius while idle.
pub const FLEE_RANGE: f32 = 310.0;
/// `RepairAI.retreatDelay` (`Time.toSeconds * 3` at 60 Hz).
pub const RETREAT_DELAY: f32 = 180.0;

/// `RepairAI` per-unit state (`avoid`, `retreatTimer`, `damagedTarget` and the
/// `timerTarget`/`timerTarget4` intervals).
#[derive(Debug, Clone, Copy, bevy_ecs::component::Component)]
pub struct RepairAiState {
    /// Nearest enemy while idle (`avoid`).
    pub avoid: Option<Entity>,
    /// Idle timer (`retreatTimer`).
    pub retreat_timer: f32,
    /// `timer.get(timerTarget, 15)` countdown for the damaged-building scan.
    pub timer15: f32,
    /// `timer.get(timerTarget4, 40)` countdown for the enemy scan.
    pub timer40: f32,
    /// Closest damaged friendly building (`damagedTarget`).
    pub target: Option<Entity>,
}

impl Default for RepairAiState {
    fn default() -> Self {
        Self {
            avoid: None,
            retreat_timer: 0.0,
            timer15: 15.0,
            timer40: 40.0,
            target: None,
        }
    }
}

/// `Units.findDamagedTile(team, x, y)` (`Units.java:212-214`): the closest
/// damaged building of `team`, any distance; `ConstructBlock`s are skipped
/// (`RepairAI.updateTargeting`, RepairAI.java:72-74).
pub fn find_damaged_tile(
    world: &bevy_ecs::world::World,
    content: &crate::content::ContentRegistry,
    team: u8,
    x: f32,
    y: f32,
) -> Option<Entity> {
    let mut best: Option<(f32, Entity)> = None;
    for entity_ref in world.iter_entities() {
        let Some(building) = entity_ref.get::<Building>() else {
            continue;
        };
        let Some(health) = entity_ref.get::<Health>() else {
            continue;
        };
        if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) || !health.damaged() {
            continue;
        }
        if content
            .block(building.block)
            .is_some_and(|def| def.kind == crate::content::BlockKind::ConstructBlock)
        {
            continue;
        }
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dx = pos.x - x;
        let dy = pos.y - y;
        let dist2 = dx * dx + dy * dy;
        let id = entity_ref.id();
        let replace = match best {
            None => true,
            Some((best_dist, best_entity)) => {
                dist2 < best_dist || (dist2 == best_dist && id.index() < best_entity.index())
            }
        };
        if replace {
            best = Some((dist2, id));
        }
    }
    best.map(|(_, entity)| entity)
}

/// `RepairAI.updateMovement` + `updateTargeting` (RepairAI.java:19-80).
pub fn update_repair_ai(ctx: &mut AiCtx, unit: Entity, state: &mut RepairAiState) {
    let Some(def) = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .cloned()
    else {
        return;
    };
    let team = ctx.team;
    let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
        return;
    };

    // `updateTargeting`: every 15 ticks rescan the damaged buildings.
    state.timer15 -= 1.0;
    if state.timer15 <= 0.0 {
        state.timer15 = 15.0;
        state.target = find_damaged_tile(ctx.world, ctx.content, team, pos.x, pos.y);
    }

    let ts = crate::config::TILESIZE as f32;
    let target_building = state
        .target
        .filter(|target| ctx.world.get::<Building>(*target).is_some());

    if let Some(target) = target_building {
        // `unit.aim(target); controlWeapons(shoot)` — the port feeds the mount
        // target and lets `set_mount_targets` gate `shoot` by weapon range.
        crate::sim::unit_runtime::set_mount_targets(
            ctx.world,
            ctx.content,
            unit,
            Some(target),
            None,
        );
    } else {
        // `super.updateTargeting()`: enemy targeting for the mounts.
        let enemy = ctx.find_target(unit, def.max_range, true, true);
        crate::sim::unit_runtime::set_mount_targets(ctx.world, ctx.content, unit, enemy, None);
    }

    // `boolean hold = hasStance(UnitStance.holdPosition)` (T11).
    let hold = ctx
        .world
        .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(unit)
        .is_some_and(|stances| stances.has_name(ctx.content, "holdposition"));

    if let Some(target) = target_building {
        if !hold {
            if def.circle_target {
                crate::ai::types::flying::circle_attack(
                    ctx.world,
                    ctx.grid,
                    ctx.content,
                    unit,
                    target,
                    def.circle_target_radius,
                );
            } else {
                // `if(!target.within(unit, range * 0.65f)) moveTo(target, range * 0.65f)`.
                let within = ctx.world.get::<Pos>(target).is_some_and(|tpos| {
                    let dx = tpos.x - pos.x;
                    let dy = tpos.y - pos.y;
                    dx * dx + dy * dy <= (def.range * 0.65).powi(2)
                });
                if !within {
                    crate::ai::types::flying::move_to(
                        ctx.world,
                        ctx.grid,
                        ctx.content,
                        unit,
                        target,
                        def.range * 0.65,
                    );
                } else {
                    // `unit.lookAt(target)`.
                    if let Some(tpos) = ctx.world.get::<Pos>(target).copied()
                        && let Some(mut core) = ctx
                            .world
                            .get_mut::<crate::entities::comp::unit::comp::UnitCore>(unit)
                    {
                        core.rotation = crate::ai::types::missile::move_toward_angle(
                            core.rotation,
                            (tpos.y - pos.y).atan2(tpos.x - pos.x).to_degrees(),
                            def.rotate_speed,
                        );
                    }
                }
            }
        }
        // Repairing resets the retreat timer (`RepairAI.java:63-64`).
        state.retreat_timer = 0.0;
        return;
    }

    // `not repairing`: scan for enemies and retreat toward the core.
    if !hold {
        state.timer40 -= 1.0;
        if state.timer40 <= 0.0 {
            state.timer40 = 40.0;
            state.avoid = ctx.find_target(unit, FLEE_RANGE, true, true);
        }
        state.retreat_timer += 1.0;
        if state.retreat_timer >= RETREAT_DELAY
            && state.avoid.is_some()
            && let Some((core, cx, cy)) =
                crate::entities::comp::unit::miner::closest_core(ctx.world, pos.x, pos.y, team)
        {
            let dx = cx - pos.x;
            let dy = cy - pos.y;
            if dx * dx + dy * dy > RETREAT_DST * RETREAT_DST {
                let _ = core;
                crate::ai::types::flying::move_to_point(
                    ctx.world,
                    ctx.grid,
                    ctx.content,
                    unit,
                    cx,
                    cy,
                    RETREAT_DST,
                    100.0,
                    false,
                );
            }
        }
    } else {
        state.retreat_timer = 0.0;
    }
    let _ = ts;
}
