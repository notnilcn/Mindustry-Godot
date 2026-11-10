// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `HugAI`: circle the enemy core with a `d4c` canPass ray-check and the
//! `fieldCore` pathfall.
//! Upstream: core/src/mindustry/ai/types/HugAI.java

use super::super::ai_weapons::{AiWeaponCtx, update_ai_weapons};
use super::super::targeting::{
    closest_spawn_tile, enemy_core_targets, face_target, unit_flowfield_cost, within_spawn,
};
use super::ground::GroundAiRules;
use super::*;

/// `HugAI` (`HugAI.java:16-61`; H01 T9): `AIController` targeting + the enemy
/// core override, a `Geometry.d4c` `canPass` ray-check of the approach,
/// straight/rotated `movePref` and the `fieldCore` pathfall.
#[allow(clippy::too_many_arguments)]
pub(in crate::sim::unit_runtime) fn update_hug_ai(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
    rules: GroundAiRules,
    core_targets: &mut BTreeMap<u8, Vec<TilePos>>,
) {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let Some(type_id) = world.get::<UnitTypeComp>(entity).map(|comp| comp.type_id) else {
        return;
    };
    let Some((
        max_range,
        type_range,
        target_air,
        target_ground,
        target_under_blocks,
        hit_size,
        cost,
    )) = runtime.content.unit(type_id).map(|def| {
        (
            def.max_range,
            def.range,
            def.target_air,
            def.target_ground,
            def.target_under_blocks,
            def.hit_size,
            unit_flowfield_cost(def),
        )
    })
    else {
        return;
    };

    // `AIController.updateWeapons`'s main-target half (`findMainTarget` =
    // `target()` = `Units.closestTarget`).
    let mut state = world
        .get::<crate::ai::types::hug::HugAiState>(entity)
        .copied()
        .unwrap_or_default();
    state.retarget += 1.0;
    let mut retargeted = false;
    if state.retarget >= if state.target.is_none() { 40.0 } else { 90.0 } {
        state.retarget = 0.0;
        retargeted = true;
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
    }
    if let Some(target) = state.target
        && invalid_target(world, &runtime.content, team, target)
    {
        state.target = None;
    }

    // `Building core = unit.closestEnemyCore();` — in
    // `unit.range() / 1.1f + core.block.size * tilesize / 2f` the unit locks it.
    let core = closest_enemy_core(world, team, pos.x, pos.y);
    let mut core_in_range = false;
    if let Some(core_entity) = core
        && let Some(cpos) = world.get::<Pos>(core_entity).copied()
    {
        let size = world
            .get::<BuildingComp>(core_entity)
            .and_then(|comp| runtime.content.block(comp.block))
            .map(|def| def.size as f32)
            .unwrap_or(0.0);
        let radius = max_range / 1.1 + size * TILE_SIZE / 2.0;
        let dx = cpos.x - pos.x;
        let dy = cpos.y - pos.y;
        if dx * dx + dy * dy <= radius * radius {
            state.target = Some(core_entity);
            core_in_range = true;
        }
    }

    // `boolean move = true;` + the default-team drop-zone stop.
    let mut move_toward = true;
    if rules.waves
        && team == rules.default_team
        && let Some(spawner) = closest_spawn_tile(runtime, pos.x, pos.y)
        && within_spawn(spawner, pos.x, pos.y, rules.drop_zone_radius + 120.0)
    {
        move_toward = false;
    }

    // Ray-checked approach (`World.raycast(.., p -> d4c canPass)`): with a
    // clear line, circle at contact or steer straight; otherwise path the core
    // field.
    let mut direct = false;
    if let Some(target) = state.target
        && let Some(tpos) = world.get::<Pos>(target).copied()
    {
        let dx = tpos.x - pos.x;
        let dy = tpos.y - pos.y;
        let dist2 = dx * dx + dy * dy;
        if dist2 <= type_range * type_range {
            let (ux, uy) = (WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y));
            let (tx, ty) = (WorldGrid::to_tile(tpos.x), WorldGrid::to_tile(tpos.y));
            let mut blocked = false;
            crate::world::raycast::raycast_each(ux, uy, tx, ty, |x, y| {
                for (ox, oy) in [(1, 0), (0, 1), (-1, 0), (0, -1), (0, 0)] {
                    if !crate::ai::types::ground::unit_can_pass(
                        world,
                        &runtime.grid,
                        &runtime.content,
                        entity,
                        x + ox,
                        y + oy,
                    ) {
                        blocked = true;
                        return true;
                    }
                }
                false
            });
            if !blocked {
                direct = true;
                let speed = crate::entities::comp::unit::movement::unit_speed(
                    world,
                    &runtime.grid,
                    &runtime.content,
                    entity,
                );
                let dst = dist2.sqrt();
                let (nx, ny) = if dst > 0.0001 {
                    (dx / dst, dy / dst)
                } else {
                    (0.0, 0.0)
                };
                let target_hit = target_hit_size_of(world, &runtime.content, target);
                if dist2 <= ((hit_size + target_hit) * 0.5).powi(2) {
                    // `unit.movePref(vec.set(target).sub(unit).rotate(90f).setLength(unit.speed()))`.
                    crate::entities::comp::unit::velocity::move_pref(
                        world,
                        &runtime.content,
                        entity,
                        -ny * speed,
                        nx * speed,
                    );
                } else {
                    // `unit.movePref(vec.set(target).sub(unit).limit(unit.speed()))`.
                    let limit = speed.min(dst);
                    crate::entities::comp::unit::velocity::move_pref(
                        world,
                        &runtime.content,
                        entity,
                        nx * limit,
                        ny * limit,
                    );
                }
            }
        }
    }
    if !direct && move_toward {
        // `pathfind(Pathfinder.fieldCore)` (`stopAtTarget = true`).
        let targets = core_targets
            .entry(team)
            .or_insert_with(|| enemy_core_targets(world, runtime, team, Some(rules)));
        let UnitRuntime {
            grid,
            content,
            pathfinder,
            ..
        } = &mut *runtime;
        let _ = pathfind_field(
            world, grid, content, pathfinder, team, entity, cost, targets, true, None,
        );
    }

    // `AIController.updateWeapons`: mounts; hugging weapons lock the core.
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
    // `faceTarget()`.
    face_target(world, &runtime.content, entity, state.target);

    if let Some(mut stored) = world.get_mut::<crate::ai::types::hug::HugAiState>(entity) {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
}
