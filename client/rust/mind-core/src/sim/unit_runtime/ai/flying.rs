// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FlyingAI` movement plus the flying half of targeting/weapons.
//! Upstream: core/src/mindustry/ai/types/FlyingAI.java
//! core/src/mindustry/entities/units/AIController.java

use super::super::ai_weapons::{AiWeaponCtx, update_ai_weapons};
use super::super::targeting::{closest_spawn_tile, flying_main_target};
use super::ground::GroundAiRules;
use super::*;

/// `FlyingAI` (FlyingAI.java:16-72) with the flying half of
/// `AIController.updateTargeting/updateWeapons` (the port's single-target mount
/// model: the mount target is the Java `findTarget` result).
#[allow(clippy::too_many_arguments)]
pub(in crate::sim::unit_runtime) fn update_flying_ai(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
    slot: ControllerSlot,
    rules: GroundAiRules,
) {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| runtime.content.unit(comp.type_id))
    else {
        return;
    };
    let (
        max_range,
        type_range,
        target_air,
        target_ground,
        target_under_blocks,
        circle_target,
        circle_target_radius,
        has_weapons,
        target_flags,
    ) = (
        def.max_range,
        def.range,
        def.target_air,
        def.target_ground,
        def.target_under_blocks,
        def.circle_target,
        def.circle_target_radius,
        !def.weapons.is_empty(),
        def.target_flags.clone(),
    );

    let mut state = world
        .get::<FlyingAiState>(entity)
        .copied()
        .unwrap_or_default();
    // `AIController.retarget()`: 40 ticks without a target, 90 with one.
    state.retarget += 1.0;
    let mut retargeted = false;
    if state.retarget >= if state.target.is_none() { 40.0 } else { 90.0 } {
        state.retarget = 0.0;
        retargeted = true;
        state.target = flying_main_target(
            world,
            queries,
            team,
            pos.x,
            pos.y,
            max_range,
            target_air,
            target_ground,
            target_under_blocks,
            &target_flags,
        );
    }
    if let Some(target) = state.target
        && invalid_target(world, &runtime.content, team, target)
    {
        state.target = None;
    }

    // `AIController.updateWeapons` with `FlyingAI.findTarget` per mount:
    // "if the main target is in range, use it, otherwise target whatever is
    // closest" (FlyingAI.java:34-39).
    update_ai_weapons(
        world,
        &runtime.content,
        queries,
        entity,
        team,
        AiWeaponCtx {
            main_target: state.target,
            retarget: retargeted,
            flying_find: true,
            core_override: None,
            should_shoot: true,
            ..Default::default()
        },
    );

    // Movement (`FlyingAI.updateMovement`).
    if let Some(target_tile) = slot.target {
        // Commanded move order: keep the fly-to-tile path.
        let arrived = update_flying(world, &runtime.grid, &runtime.content, entity, target_tile);
        if arrived && let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
            stored.target = None;
        }
    } else if let Some(target) = state.target {
        if has_weapons {
            if circle_target {
                circle_attack(
                    world,
                    &runtime.grid,
                    &runtime.content,
                    entity,
                    target,
                    circle_target_radius,
                );
            } else {
                move_to(
                    world,
                    &runtime.grid,
                    &runtime.content,
                    entity,
                    target,
                    type_range * 0.8,
                );
            }
        }
    } else if rules.waves && team == rules.default_team {
        // `moveTo(getClosestSpawner(), dropZoneRadius + 130f)`.
        if let Some(spawn) = closest_spawn_tile(runtime, pos.x, pos.y) {
            move_to_point(
                world,
                &runtime.grid,
                &runtime.content,
                entity,
                // `Tile.worldx()/worldy()` (Tile.java:166-171).
                spawn.x() as f32 * TILE_SIZE,
                spawn.y() as f32 * TILE_SIZE,
                rules.drop_zone_radius + 130.0,
                100.0,
                false,
            );
        }
    }

    if let Some(mut stored) = world.get_mut::<FlyingAiState>(entity) {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
}
