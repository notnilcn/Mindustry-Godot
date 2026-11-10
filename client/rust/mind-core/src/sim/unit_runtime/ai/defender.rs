// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DefenderAI`: retarget the healthiest allied player-controllable unit and
//! keep formation distance.
//! Upstream: core/src/mindustry/ai/types/DefenderAI.java

use super::ground::GroundAiRules;
use super::*;

/// `DefenderAI` (`DefenderAI.java:12-45`; H01 T9): retarget the healthiest
/// player-controllable ally, keep `targetHitSize/2 * 1.1 + ownHitSize/2 + 15`
/// px away (smooth 50), look at it. No weapons pass (Java has none).
pub(in crate::sim::unit_runtime) fn update_defender_ai(
    world: &mut World,
    runtime: &mut UnitRuntime,
    entity: Entity,
    team: u8,
    rules: GroundAiRules,
) {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let Some(type_id) = world.get::<UnitTypeComp>(entity).map(|comp| comp.type_id) else {
        return;
    };
    let Some((max_range, hit_size)) = runtime
        .content
        .unit(type_id)
        .map(|def| (def.max_range, def.hit_size))
    else {
        return;
    };
    let mut state = world
        .get::<crate::ai::types::defender::DefenderAiState>(entity)
        .copied()
        .unwrap_or_default();
    // `DefenderAI.updateTargeting`: `if(retarget()) target = findTarget(...)`.
    state.retarget += 1.0;
    if state.retarget >= if state.target.is_none() { 40.0 } else { 90.0 } {
        state.retarget = 0.0;
        state.target = crate::ai::types::defender::defender_find_target(
            world,
            &runtime.content,
            team,
            pos.x,
            pos.y,
            max_range,
            type_id,
            rules.waves,
            rules.wave_team,
        );
    }
    // `DefenderAI.updateMovement`: `moveTo(target, keepDistance, 50f)` and
    // `unit.lookAt(target)`.
    if let Some(target) = state.target
        && let Some(tpos) = world.get::<Pos>(target).copied()
    {
        let target_hit = target_hit_size_of(world, &runtime.content, target);
        move_to_point(
            world,
            &runtime.grid,
            &runtime.content,
            entity,
            tpos.x,
            tpos.y,
            target_hit / 2.0 * 1.1 + hit_size / 2.0 + 15.0,
            50.0,
            true,
        );
    }
    // `DefenderAI.updateTargeting` sets `target` directly — Java never runs
    // `AIController.updateWeapons`'s `invalid(target)` clear for this
    // controller (the target is a same-team ally, which that check would drop).
    if let Some(mut stored) = world.get_mut::<crate::ai::types::defender::DefenderAiState>(entity) {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
}
