// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `GroundAI` movement/targeting/weapons plus the stuck propagation.
//! Upstream: core/src/mindustry/ai/types/GroundAI.java
//! core/src/mindustry/entities/units/AIController.java

use super::super::ai_weapons::{AiWeaponCtx, update_ai_weapons};
use super::super::physics::collision_layer;
use super::super::targeting::{
    closest_spawn_tile, enemy_core_targets, face_target, unit_flowfield_cost, within_spawn,
};
use super::*;

/// The `state.rules` fields `GroundAI` reads (`Rules.waves`, `defaultTeam`,
/// `dropZoneRadius`); `wave_team`/`pvp` serve T9's `FlyingFollowAI.useFallback`
/// and `DefenderAI`'s enemy-core fallback.
#[derive(Debug, Clone, Copy)]
pub(in crate::sim::unit_runtime) struct GroundAiRules {
    pub(in crate::sim::unit_runtime) waves: bool,
    pub(in crate::sim::unit_runtime) default_team: u8,
    pub(in crate::sim::unit_runtime) drop_zone_radius: f32,
    pub(in crate::sim::unit_runtime) wave_team: u8,
    pub(in crate::sim::unit_runtime) pvp: bool,
}

impl Default for GroundAiRules {
    fn default() -> Self {
        Self {
            waves: false,
            default_team: 1,
            drop_zone_radius: 300.0,
            wave_team: 2,
            pvp: false,
        }
    }
}

/// `GroundAI.updateMovement` (GroundAI.java:21-85) plus the ground half of
/// `AIController.updateTargeting/updateWeapons` (AIController.java:113-260).
#[allow(clippy::too_many_arguments)]
pub(in crate::sim::unit_runtime) fn update_ground_ai(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
    slot: ControllerSlot,
    rules: GroundAiRules,
    core_targets: &mut BTreeMap<u8, Vec<TilePos>>,
) {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let Some(def) = unit_def(world, &runtime.content, entity) else {
        return;
    };
    // Copy out of the content borrow before touching the rest of the runtime.
    let ground_layer = collision_layer(world, def, entity) == 0;
    let (
        max_range,
        type_range,
        target_air,
        target_ground,
        target_under_blocks,
        cost,
        speed,
        hit_size,
    ) = (
        def.max_range,
        def.range,
        def.target_air,
        def.target_ground,
        def.target_under_blocks,
        unit_flowfield_cost(def),
        def.speed,
        def.hit_size,
    );

    let mut state = world
        .get::<GroundAiState>(entity)
        .copied()
        .unwrap_or_default();
    // `GroundAI.stuckRange/stuckThreshold` (`GroundAI.java:18,24`): "if it
    // hasn't moved the stuck range in twice the time it should have taken,
    // it's stuck".
    let stuck_range = TILE_SIZE * 1.5;
    let stuck_threshold = (stuck_range * 2.0 / speed).max(1.0);
    // `pathfind(fieldCore, true, stuckTime >= stuckThreshold)`
    // (`GroundAI.java:54`) with `avoidance && unit.collisionLayer() ==
    // layerGround` (`AIController.java:155`).
    let avoid = ground_layer && state.stuck_time >= stuck_threshold;

    // `AIController.retarget()` (AIController.java:289): 40 ticks without a
    // target, 90 with one.
    state.retarget += 1.0;
    let mut retargeted = false;
    if state.retarget >= if state.target.is_none() { 40.0 } else { 90.0 } {
        state.retarget = 0.0;
        state.target = queries.closest_target(
            world,
            team,
            pos.x,
            pos.y,
            max_range,
            target_air,
            target_ground,
            true,
            target_under_blocks,
        );
        retargeted = true;
    }
    // `invalid(target)` (AIController.java:207-213) -> `Units.invalidateTarget`
    // with `Float.MAX_VALUE` (no distance check): dead/despawned, own team, or
    // a non-targetable unit.
    if let Some(target) = state.target
        && invalid_target(world, &runtime.content, team, target)
    {
        state.target = None;
    }

    // `GroundAI.updateMovement`: core override at
    // `unit.range() / 1.3 + core.block.size * tilesize / 2`.
    let core = closest_enemy_core(world, team, pos.x, pos.y);
    let mut core_in_range = false;
    if let Some(core_entity) = core
        && let Some(core_pos) = world.get::<Pos>(core_entity).copied()
    {
        let size = world
            .get::<BuildingComp>(core_entity)
            .and_then(|comp| runtime.content.block(comp.block))
            .map(|def| def.size as f32)
            .unwrap_or(0.0);
        let radius = max_range / 1.3 + size * TILE_SIZE / 2.0;
        let dx = core_pos.x - pos.x;
        let dy = core_pos.y - pos.y;
        if dx * dx + dy * dy <= radius * radius {
            state.target = Some(core_entity);
            core_in_range = true;
        }
    }

    // Movement.
    let mut moved = false;
    if let Some(target_tile) = slot.target {
        // Commanded move order (`CommandAI.targetPos`): the explicit order wins
        // over the field walk (plan G01 T6a change 2).
        let arrived = {
            let UnitRuntime {
                grid,
                content,
                pathfinder,
                ..
            } = &mut *runtime;
            update_ground(world, grid, content, pathfinder, team, entity, target_tile)
        };
        if arrived && let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
            stored.target = None;
        }
    } else {
        // `if(core == null || !unit.within(core, unit.type.range * 0.5f))`.
        let within_half = core.is_some_and(|core_entity| {
            world.get::<Pos>(core_entity).is_some_and(|core_pos| {
                let dx = core_pos.x - pos.x;
                let dy = core_pos.y - pos.y;
                dx * dx + dy * dy <= (type_range * 0.5).powi(2)
            })
        });
        if core.is_none() || !within_half {
            let spawner = closest_spawn_tile(runtime, pos.x, pos.y);
            let mut should_move = true;
            if rules.waves && team == rules.default_team {
                if spawner.is_some_and(|tile| {
                    within_spawn(tile, pos.x, pos.y, rules.drop_zone_radius + 120.0)
                }) {
                    should_move = false;
                }
                if spawner.is_none() && core.is_none() {
                    should_move = false;
                }
            }
            // No reason to move if there's nothing there.
            if core.is_none() && (!rules.waves || spawner.is_none()) {
                should_move = false;
            }
            moved = should_move;
            if should_move {
                let targets = core_targets
                    .entry(team)
                    .or_insert_with(|| enemy_core_targets(world, runtime, team, Some(rules)));
                // `AvoidanceProcess.getAvoidance()` lazily activates on the
                // first avoidance request (`AvoidanceProcess.java:40-48`).
                if avoid {
                    runtime.avoidance_active = true;
                }
                let avoid_id = world
                    .get::<EntitySeq>(entity)
                    .map_or(0, |seq| (seq.0 & 0x7fff_ffff) as i32);
                let UnitRuntime {
                    grid,
                    content,
                    pathfinder,
                    avoidance,
                    ..
                } = &mut *runtime;
                let avoid_arg = avoid.then_some((avoidance.as_slice(), avoid_id));
                let _ = pathfind_field(
                    world, grid, content, pathfinder, team, entity, cost, targets, true, avoid_arg,
                );
            }
        }
    }

    // Stuck bookkeeping (`GroundAI.java:63-85`): the threshold-crossing tick
    // propagates the stuck marker to nearby same-team GroundAI units.
    if moved {
        let dx = pos.x - state.stuck_x;
        let dy = pos.y - state.stuck_y;
        if dx * dx + dy * dy <= stuck_range * stuck_range {
            let before = state.stuck_time;
            state.stuck_time += 1.0;
            if before < stuck_threshold && state.stuck_time >= stuck_threshold {
                propagate_stuck(world, runtime, entity, team, hit_size, pos.x, pos.y);
            }
        } else {
            state.stuck_x = pos.x;
            state.stuck_y = pos.y;
            state.stuck_time = 0.0;
        }
    } else {
        state.stuck_time = 0.0;
    }

    // `AIController.updateWeapons`: mount targets and the ground core override
    // (`GroundAI.java:38-44`: controllable ground weapons lock the core).
    update_ai_weapons(
        world,
        &runtime.content,
        queries,
        entity,
        team,
        AiWeaponCtx {
            main_target: state.target,
            retarget: retargeted,
            flying_find: false,
            core_override: core_in_range.then_some(core).flatten(),
            should_shoot: true,
            ..Default::default()
        },
    );
    // `GroundAI.updateMovement` ends with `faceTarget()` (GroundAI.java:61).
    face_target(world, &runtime.content, entity, state.target);

    if let Some(mut stored) = world.get_mut::<GroundAiState>(entity) {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
}

/// `GroundAI.java:67-75`: on the threshold-crossing tick, mark nearby
/// same-team `GroundAI` units as stuck at their own position with
/// `max(1, stuckRange * 2 / other.speed) + 1`. Java's `Units.nearby` is a
/// quadtree query over the team's units (box `±radius/2`, inclusive).
pub(in crate::sim::unit_runtime) fn propagate_stuck(
    world: &mut World,
    runtime: &UnitRuntime,
    entity: Entity,
    team: u8,
    hit_size: f32,
    x: f32,
    y: f32,
) {
    // `radius = unit.hitSize * Vars.unitCollisionRadiusScale * 2f`.
    let scale = crate::ai::unit_group::UNIT_COLLISION_RADIUS_SCALE;
    let radius = hit_size * scale * 2.0;
    let half = radius / 2.0;
    let mut writes: Vec<(Entity, f32, f32, f32)> = Vec::new();
    for &other in &runtime.units {
        if other == entity {
            continue;
        }
        if team_of(world, other) != Some(team) {
            continue;
        }
        // `other.controller() instanceof GroundAI` (`GroundAI.java:70`).
        if world.get::<ControllerSlot>(other).map(|slot| slot.kind) != Some(AiKind::Ground) {
            continue;
        }
        let Some(other_pos) = world.get::<Pos>(other).copied() else {
            continue;
        };
        if (other_pos.x - x).abs() > half || (other_pos.y - y).abs() > half {
            continue;
        }
        let Some(other_def) = unit_def(world, &runtime.content, other) else {
            continue;
        };
        // `other.within(unit, radius + other.hitSize * unitCollisionRadiusScale)`.
        let reach = radius + other_def.hit_size * scale;
        let dx = other_pos.x - x;
        let dy = other_pos.y - y;
        if dx * dx + dy * dy > reach * reach {
            continue;
        }
        let threshold = (TILE_SIZE * 1.5 * 2.0 / other_def.speed).max(1.0) + 1.0;
        writes.push((other, other_pos.x, other_pos.y, threshold));
    }
    for (other, stuck_x, stuck_y, threshold) in writes {
        if let Some(mut state) = world.get_mut::<GroundAiState>(other) {
            state.stuck_x = stuck_x;
            state.stuck_y = stuck_y;
            state.stuck_time = threshold;
        }
    }
}
