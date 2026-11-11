// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Suicide` dispatch arm: ram the nearest hostile and fire the close-range
//! death weapon.
//! Upstream: core/src/mindustry/ai/types/SuicideAI.java

use super::super::ai_weapons::{AiWeaponCtx, AiWeaponsState, update_ai_weapons};
use super::*;

/// `SuicideAI.updateUnit`: straight-line ram (terrain ignored, `shootCone 180`).
pub(in crate::sim::unit_runtime) fn suicide_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
) {
    // `SuicideAI.updateUnit` (`ai/types/suicide.rs`): ram the nearest
    // hostile in a straight line (terrain ignored) and fire the
    // close-range death weapon (`shootCone 180`).
    let mount_target = {
        let pos = world.get::<Pos>(entity).copied();
        let def = world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| runtime.content.unit(comp.type_id));
        match (pos, def) {
            (Some(pos), Some(def)) => queries.closest_target(
                world,
                team,
                pos.x,
                pos.y,
                def.max_range.max(def.range),
                def.target_air,
                def.target_ground,
                true,
                def.target_under_blocks,
            ),
            _ => None,
        }
    };
    if let Some(mut state) = world.get_mut::<AiWeaponsState>(entity) {
        state.main_target = mount_target;
        state.retarget = 0.0;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(AiWeaponsState {
            main_target: mount_target,
            retarget: 0.0,
            ..AiWeaponsState::default()
        });
    }
    update_ai_weapons(
        world,
        &runtime.content,
        queries,
        entity,
        team,
        AiWeaponCtx {
            main_target: mount_target,
            retarget: false,
            flying_find: false,
            core_override: None,
            should_shoot: true,
            ..Default::default()
        },
    );
    let mut state = crate::ai::types::suicide::SuicideAi {
        unit: Some(entity),
        target: mount_target,
        suicidal: false,
    };
    {
        let UnitRuntime {
            content,
            grid,
            pathfinder,
            ..
        } = &mut *runtime;
        let mut ctx = crate::ai::ai_controller::AiCtx {
            world,
            grid,
            content,
            pathfinder,
            team,
        };
        let _ = crate::ai::types::suicide::update_suicide(&mut ctx, entity, &mut state);
    }
}
