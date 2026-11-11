// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Missile` dispatch arm: steer at the shooter's aim, always move forward, die
//! on enemy-building contact or at the `TimedKill` lifetime.
//! Upstream: core/src/mindustry/ai/types/MissileAI.java
//! core/src/mindustry/entities/units/AIController.java (updateTargeting)

use super::super::ai_weapons::{AiWeaponCtx, update_ai_weapons};
use super::*;

/// `MissileAI.updateMovement` (live dispatch, PAR-0088) plus the
/// `AIController.updateTargeting` weapon pass a controllable missile runs.
pub(in crate::sim::unit_runtime) fn missile_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
) {
    // `MissileAI.updateMovement` (live dispatch, PAR-0088): steer at
    // the shooter's aim, always move forward, die on enemy-building
    // contact or at the `TimedKill` lifetime.
    let shooter = world
        .get::<MissileShooter>(entity)
        .map(|shooter| shooter.0)
        .unwrap_or(None);
    // `AIController.target` snapshot from the shot's aimed entity
    // (`BulletType.create` `target`; `MissileAI.java:33-36`).
    let target = world
        .get::<crate::ai::types::missile::MissileTarget>(entity)
        .map(|target| target.0)
        .unwrap_or(None);
    let mut state = MissileAi {
        unit: Some(entity),
        target,
        shooter,
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
        let _ = update_missile(&mut ctx, entity, &mut state);
    }
    if let Some(mut stored) = world.get_mut::<MissileShooter>(entity) {
        stored.0 = state.shooter;
    }
    // `AIController.updateTargeting` -> `updateWeapons`
    // (AIController.java:114-116, 172-238): a controllable weapon finds
    // its own target within `weapon.range()`. For the scathe missile the
    // shoot-on-death explosion is a proximity fuse against ground units
    // (`findTarget(mountX, mountY, wrange, bullet.collidesAir,
    // bullet.collidesGround)`).
    let mount_target = {
        let def = world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| runtime.content.unit(comp.type_id));
        def.and_then(|def| {
            let weapon = def.weapons.first()?;
            let range = weapon.range();
            let (air, ground) = runtime
                .content
                .bullet(weapon.bullet.id)
                .map(|bullet| (bullet.collides_air, bullet.collides_ground))
                .unwrap_or((false, true));
            let (x, y) = world.get::<Pos>(entity).map(|pos| (pos.x, pos.y))?;
            queries.closest_target(
                world,
                team,
                x,
                y,
                range,
                air,
                ground,
                true,
                def.target_under_blocks,
            )
        })
    };
    update_ai_weapons(
        world,
        &runtime.content,
        &queries,
        entity,
        team,
        AiWeaponCtx {
            main_target: mount_target,
            retarget: true,
            flying_find: false,
            core_override: None,
            should_shoot: true,
            ..Default::default()
        },
    );
}
