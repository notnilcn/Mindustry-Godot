// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FlyingFollowAI`: wave-team flyers follow the healthiest allied unit.
//! Upstream: core/src/mindustry/ai/types/FlyingFollowAI.java

use super::super::ai_weapons::{AiWeaponCtx, update_ai_weapons};
use super::super::targeting::{entity_pos_within, flying_main_target, look_at_world};
use super::flying::update_flying_ai;
use super::ground::GroundAiRules;
use super::*;

/// `FlyingFollowAI` (`FlyingFollowAI.java:11-60`; H01 T9): wave-team flyers
/// follow the healthiest allied non-follower (30-tick re-pick) with the
/// inherited `FlyingAI` targeting/weapons; all other teams run `FlyingAI`
/// (`useFallback()` = `rules.pvp || rules.waveTeam != unit.team`).
#[allow(clippy::too_many_arguments)]
pub(in crate::sim::unit_runtime) fn update_flying_follow_ai(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
    slot: ControllerSlot,
    rules: GroundAiRules,
) {
    // `if(useFallback() && fallback() != null){ fallback.updateUnit(); return; }`
    if rules.pvp || rules.wave_team != team {
        update_flying_ai(world, runtime, queries, entity, team, slot, rules);
        return;
    }
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
        target_flags,
        has_weapons,
        hit_size,
    )) = runtime.content.unit(type_id).map(|def| {
        (
            def.max_range,
            def.range,
            def.target_air,
            def.target_ground,
            def.target_under_blocks,
            def.target_flags.clone(),
            !def.weapons.is_empty(),
            def.hit_size,
        )
    })
    else {
        return;
    };

    // Inherited `FlyingAI` targeting (`findMainTarget` + 40/90 retarget).
    let mut fly = world
        .get::<FlyingAiState>(entity)
        .copied()
        .unwrap_or_default();
    fly.retarget += 1.0;
    let mut retargeted = false;
    if fly.retarget >= if fly.target.is_none() { 40.0 } else { 90.0 } {
        fly.retarget = 0.0;
        retargeted = true;
        fly.target = flying_main_target(
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
    if let Some(target) = fly.target
        && invalid_target(world, &runtime.content, team, target)
    {
        fly.target = None;
    }
    // `AIController.updateWeapons` with `FlyingAI.findTarget` per mount.
    update_ai_weapons(
        world,
        &runtime.content,
        queries,
        entity,
        team,
        AiWeaponCtx {
            main_target: fly.target,
            retarget: retargeted,
            flying_find: true,
            core_override: None,
            should_shoot: true,
            ..Default::default()
        },
    );
    if let Some(mut stored) = world.get_mut::<FlyingAiState>(entity) {
        *stored = fly;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(fly);
    }

    // `timer.get(timerTarget3, 30f)` -> re-pick `following`, then movement.
    let mut state = world
        .get::<crate::ai::types::flying_follow::FlyingFollowAiState>(entity)
        .copied()
        .unwrap_or_default();
    state.follow_timer += 1.0;
    if state.follow_timer >= 30.0 {
        state.follow_timer = 0.0;
        state.following = crate::ai::types::flying_follow::flying_follow_closest(
            world, team, pos.x, pos.y, type_range, type_id,
        );
    }
    if let Some(following) = state.following
        && world.get_entity(following).is_err()
    {
        state.following = None;
    }
    let mut look: Option<(f32, f32)> = None;
    if let Some(following) = state.following {
        if let Some(fpos) = world.get::<Pos>(following).copied() {
            let target_hit = target_hit_size_of(world, &runtime.content, following);
            move_to_point(
                world,
                &runtime.grid,
                &runtime.content,
                entity,
                fpos.x,
                fpos.y,
                target_hit / 2.0 * 1.1 + hit_size / 2.0 + 15.0,
                50.0,
                false,
            );
            look = Some((fpos.x, fpos.y));
        }
    } else if let Some(target) = fly.target
        && has_weapons
        && let Some(tpos) = world.get::<Pos>(target).copied()
    {
        // `else if(target != null && unit.hasWeapons()){ moveTo(target, 80f); }`
        move_to_point(
            world,
            &runtime.grid,
            &runtime.content,
            entity,
            tpos.x,
            tpos.y,
            80.0,
            100.0,
            false,
        );
    }
    // `shouldFaceTarget()`: `target != null && (following == null ||
    // unit.within(target, unit.range()))`.
    let face = fly.target.is_some_and(|target| {
        state.following.is_none() || entity_pos_within(world, entity, target, max_range)
    });
    if face {
        if let Some(target) = fly.target
            && let Some(tpos) = world.get::<Pos>(target).copied()
        {
            look_at_world(world, entity, tpos.x, tpos.y);
        }
    } else if let Some((lx, ly)) = look {
        look_at_world(world, entity, lx, ly);
    }
    if let Some(mut stored) =
        world.get_mut::<crate::ai::types::flying_follow::FlyingFollowAiState>(entity)
    {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
}
