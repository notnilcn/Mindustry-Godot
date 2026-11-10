// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The `AIController` weapon pass: mount targeting, aim and the shoot gate.
//! Upstream: core/src/mindustry/entities/units/AIController.java
//! (updateWeapons)

use super::targeting::{flying_main_target, invalidate_target, target_hit_size};
use super::*;

/// `Unit.controlWeapons(shoot)` (`UnitComp.java`): aim every mount at `target`
/// and gate firing. `RepairAI.updateMovement` uses this with an own-team
/// building, which `Units.invalidateTarget` rejects as an AI target.
pub(crate) fn control_weapons_for(world: &mut World, entity: Entity, target: Entity, shoot: bool) {
    let Some(tpos) = world.get::<Pos>(target).copied() else {
        return;
    };
    let Some(mut weapons) = world.get_mut::<UnitWeapons>(entity) else {
        return;
    };
    for mount in weapons.mounts.iter_mut() {
        mount.target = Some(target);
        mount.rotate = true;
        mount.shoot = shoot;
        mount.aim_x = tpos.x;
        mount.aim_y = tpos.y;
    }
}

/// [`update_ai_weapons`] for callers that hold an [`AiCtx`] and a target but no
/// per-tick [`TargetQueries`] (`ai/types/repair.rs`): builds a one-off query
/// set, like `core/weapons/mod.rs::update_weapon` does for autonomous mounts.
pub(crate) fn update_ai_weapons_for(
    ctx: &mut crate::ai::ai_controller::AiCtx<'_>,
    unit: Entity,
    target: Option<Entity>,
) {
    let queries = TargetQueries::build(ctx.world, ctx.content);
    let team = ctx.team;
    let content = ctx.content;
    update_ai_weapons(
        ctx.world,
        content,
        &queries,
        unit,
        team,
        AiWeaponCtx {
            main_target: target,
            retarget: true,
            flying_find: false,
            core_override: None,
            should_shoot: true,
            ..Default::default()
        },
    );
}

/// `AIController.updateWeapons` for AI kinds without a targeting timer
/// (Command/Builder/Suicide fallbacks): ticks the 40/90-tick `retarget()`
/// cadence and, when it fires, resolves the main target with
/// `Units.closestTarget` at the unit centre (`AIController.findMainTarget`
/// delegates to `target`, AIController.java:281-287). The pre-T2
/// "nearest enemy building within range + 16" anchor fallback is gone (it has
/// no Java equivalent).
pub(super) fn command_ai_weapon_pass(
    world: &mut World,
    content: &ContentRegistry,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
    should_shoot: bool,
) -> Option<Entity> {
    let pos = world.get::<Pos>(entity).copied()?;
    let def = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))?;
    let mut state = world
        .get::<AiWeaponsState>(entity)
        .copied()
        .unwrap_or_default();
    let interval = if state.main_target.is_none() {
        40.0
    } else {
        90.0
    };
    state.retarget += 1.0;
    let retarget = state.retarget >= interval;
    if retarget {
        state.retarget = 0.0;
        state.main_target = queries.closest_target(
            world,
            team,
            pos.x,
            pos.y,
            def.max_range.max(def.range),
            def.target_air,
            def.target_ground,
            true,
            def.target_under_blocks,
        );
    }
    let main_target = state.main_target;
    if let Some(mut stored) = world.get_mut::<AiWeaponsState>(entity) {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
    update_ai_weapons(
        world,
        content,
        queries,
        entity,
        team,
        AiWeaponCtx {
            main_target,
            retarget,
            flying_find: false,
            core_override: None,
            should_shoot,
            ..Default::default()
        },
    );
    main_target
}

/// `LogicAI`'s `updateWeapons` pass (T1): `target()` returns `posTarget`/
/// `mainTarget`, `invalid`/`checkTarget` return false, every mount uses the
/// controller target, and `shouldShoot() = shoot && !(canBoost && boost)`.
#[allow(clippy::too_many_arguments)]
pub(super) fn logic_ai_weapon_pass(
    world: &mut World,
    content: &ContentRegistry,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
    main_target: Option<Entity>,
    pos_target: Option<(f32, f32)>,
    should_shoot: bool,
    explicit_target: bool,
) {
    update_ai_weapons(
        world,
        content,
        queries,
        entity,
        team,
        AiWeaponCtx {
            main_target,
            retarget: true,
            flying_find: false,
            core_override: None,
            should_shoot,
            explicit_target,
            pos_target,
        },
    );
}

/// `AIController.rotateBackTimer` (AIController.java:22).
pub(super) const ROTATE_BACK_TIMER: f32 = 60.0 * 5.0;

/// One mount's computed `(target, shoot, rotate, aim)` for this tick.
pub(super) type MountUpdate = (Option<Entity>, bool, bool, Option<(f32, f32)>);

/// `AIController` weapon state that is not per mount (`bomberTarget`,
/// `noTargetTime`); inserted lazily like the other AI state components.
#[derive(Debug, Clone, Copy, bevy_ecs::component::Component)]
pub(crate) struct AiWeaponsState {
    /// `AIController.bomberTarget`.
    pub bomber_target: Option<Entity>,
    /// `AIController.noTargetTime`.
    pub no_target_time: f32,
    /// `AIController.timer(timerTarget)` countdown for kinds without their own
    /// targeting timer (Command/Builder/Suicide). Seeded 39 so the first tick
    /// fires, like `GroundAiState`.
    pub retarget: f32,
    /// `AIController.target` for those kinds.
    pub main_target: Option<Entity>,
}

impl Default for AiWeaponsState {
    fn default() -> Self {
        Self {
            bomber_target: None,
            no_target_time: 0.0,
            retarget: 39.0,
            main_target: None,
        }
    }
}

/// One `AIController.updateWeapons` pass (`AIController.java:172-257`).
#[derive(Default)]
pub(crate) struct AiWeaponCtx {
    /// The controller's main target (`AIController.target`).
    pub main_target: Option<Entity>,
    /// Whether the 40/90-tick retarget timer fired this tick (`ret`).
    pub retarget: bool,
    /// `FlyingAI.findTarget` semantics for the per-mount search.
    pub flying_find: bool,
    /// `GroundAI.updateMovement`'s in-range enemy core override.
    pub core_override: Option<Entity>,
    /// `shouldShoot()` (`false` for a building `BuilderAI`; `true` otherwise).
    pub should_shoot: bool,
    /// `LogicAI.target()` override: every controllable mount uses
    /// `main_target` instead of the per-mount `findTarget` query, and
    /// `invalid`/`checkTarget` return false so the target is never dropped.
    pub explicit_target: bool,
    /// `LogicAI.aimControl == target`: an aim point with no entity
    /// (`posTarget`); `mount.target` stays null while aim/shoot use the point.
    pub pos_target: Option<(f32, f32)>,
}

/// `AIController.target`: per-mount targets, aim and the shoot gate
/// (`AIController.updateWeapons`, AIController.java:172-257).
pub(crate) fn update_ai_weapons(
    world: &mut World,
    content: &ContentRegistry,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
    ctx: AiWeaponCtx,
) {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return;
    };
    let (single_target, auto_drop_bombs, hit_size, target_under_blocks) = (
        def.single_target,
        def.auto_drop_bombs,
        def.hit_size,
        def.target_under_blocks,
    );
    let rotation = world
        .get::<UnitCore>(entity)
        .map(|core| core.rotation)
        .unwrap_or(0.0);
    let src_vel = world
        .get::<Vel>(entity)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });
    // `CommandAI.shouldFire()`: `!hasStance(holdFire)`.
    let hold_fire = world
        .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(entity)
        .is_some_and(|stances| stances.has_name(content, "holdfire"));

    let mut state = world
        .get::<AiWeaponsState>(entity)
        .copied()
        .unwrap_or_default();
    // `noTargetTime += Time.delta; if(invalid(target)){ ... target = null; }
    //  else noTargetTime = 0f;` — `LogicAI.invalid` always returns false, so an
    // explicit-target pass never drops the target nor ages `noTargetTime`.
    let target = if ctx.explicit_target || ctx.pos_target.is_some() {
        state.no_target_time = 0.0;
        ctx.main_target
    } else {
        match ctx.main_target {
            Some(target) => {
                if invalidate_target(world, content, team, Some(target), pos.x, pos.y, f32::MAX) {
                    state.no_target_time += 1.0;
                    None
                } else {
                    state.no_target_time = 0.0;
                    Some(target)
                }
            }
            None => {
                state.no_target_time += 1.0;
                None
            }
        }
    };

    let mounts: Vec<(Option<Entity>, f32)> = world
        .get::<UnitWeapons>(entity)
        .map(|weapons| {
            weapons
                .mounts
                .iter()
                .map(|mount| (mount.target, mount.rotation))
                .collect()
        })
        .unwrap_or_default();
    // (target, shoot, rotate, aim) per mount; applied in one borrow below.
    let mut updates: Vec<MountUpdate> = Vec::with_capacity(mounts.len());
    for (index, (prev_target, mount_rotation)) in mounts.iter().copied().enumerate() {
        let Some(weapon) = def.weapons.get(index) else {
            updates.push((prev_target, false, false, None));
            continue;
        };
        if !weapon.controllable || weapon.no_attack {
            // `if(!weapon.controllable || weapon.noAttack) continue;`
            updates.push((prev_target, false, false, None));
            continue;
        }
        if !weapon.ai_controllable {
            // `if(!weapon.aiControllable){ mount.rotate = false; continue; }`
            updates.push((prev_target, false, false, None));
            continue;
        }
        let mount_x = pos.x + crate::weapons::trnsx(rotation - 90.0, weapon.x, weapon.y);
        let mount_y = pos.y + crate::weapons::trnsy(rotation - 90.0, weapon.x, weapon.y);
        let wrange = weapon.range();
        let (collides_air, collides_ground) = content
            .bullet(weapon.bullet.id)
            .map(|bullet| (bullet.collides_air, bullet.collides_ground))
            .unwrap_or((false, true));
        let core_override = ctx.core_override.filter(|_| collides_ground).or(None);
        let mut mount_target = prev_target;
        if let Some(core) = core_override {
            // `GroundAI.updateMovement`: controllable ground weapons lock the
            // core while it is in range.
            mount_target = Some(core);
        } else if single_target {
            mount_target = target;
        } else if ctx.explicit_target {
            // `LogicAI.findTarget` = `target(...)`: every mount uses the
            // posTarget/mainTarget, never the per-mount query, and
            // `LogicAI.checkTarget` always returns false.
            mount_target = target;
        } else {
            if ctx.retarget {
                mount_target = if ctx.flying_find {
                    // `FlyingAI.findTarget`: keep a valid in-range main target,
                    // else the closest in-range target.
                    let main = flying_main_target(
                        world,
                        queries,
                        team,
                        mount_x,
                        mount_y,
                        wrange,
                        collides_air,
                        collides_ground,
                        target_under_blocks,
                        &def.target_flags,
                    );
                    if invalidate_target(world, content, team, main, mount_x, mount_y, wrange) {
                        queries.closest_target(
                            world,
                            team,
                            mount_x,
                            mount_y,
                            wrange,
                            collides_air,
                            collides_ground,
                            true,
                            target_under_blocks,
                        )
                    } else {
                        main
                    }
                } else {
                    queries.closest_target(
                        world,
                        team,
                        mount_x,
                        mount_y,
                        wrange,
                        collides_air,
                        collides_ground,
                        true,
                        target_under_blocks,
                    )
                };
            }
            // `if(checkTarget(mount.target, mountX, mountY, wrange)) mount.target = null;`
            // (AIController.java:220) - only outside the `singleTarget` branch.
            if invalidate_target(world, content, team, mount_target, mount_x, mount_y, wrange) {
                mount_target = None;
            }
        }
        let mut shoot = false;
        let mut aim = None;
        if let Some((px, py)) = ctx.pos_target {
            // `LogicAI.aimControl == target`: `mount.target` is the `posTarget`
            // object (no entity in the port); shoot when the point is in range,
            // aim at the static-point intercept.
            let dx = px - mount_x;
            let dy = py - mount_y;
            shoot = dx * dx + dy * dy <= wrange * wrange && ctx.should_shoot;
            let (bullet_speed, keep_velocity) = content
                .bullet(weapon.bullet.id)
                .map(|bullet| (bullet.speed, bullet.keep_velocity))
                .unwrap_or((0.0, true));
            aim = Some(crate::combat::targeting::intercept_positions(
                (pos.x, pos.y),
                (src_vel.x, src_vel.y),
                (px, py),
                (0.0, 0.0),
                keep_velocity,
                bullet_speed,
            ));
        } else if let Some(chosen) = mount_target {
            let hit = target_hit_size(world, content, chosen);
            shoot = world.get::<Pos>(chosen).is_some_and(|tpos| {
                let dx = tpos.x - mount_x;
                let dy = tpos.y - mount_y;
                let r = wrange + hit / 2.0;
                dx * dx + dy * dy <= r * r
            }) && ctx.should_shoot;
            if auto_drop_bombs && !shoot {
                // `Units.closestTarget(team, x, y, hitSize, u -> !u.isFlying(),
                //  t -> true)` with the `bomberTarget` cache (AIController.java:225-229).
                let valid = state.bomber_target.is_some_and(|bomber| {
                    world.get_entity(bomber).is_ok()
                        && world.get::<Pos>(bomber).is_some_and(|bpos| {
                            let dx = bpos.x - pos.x;
                            let dy = bpos.y - pos.y;
                            let r = hit_size / 2.0 + target_hit_size(world, content, bomber) / 2.0;
                            dx * dx + dy * dy <= r * r
                        })
                });
                if !valid {
                    state.bomber_target = queries.closest_target(
                        world, team, pos.x, pos.y, hit_size, false, true, true, true,
                    );
                }
                shoot = state.bomber_target.is_some();
            }
            if let Some(tpos) = world.get::<Pos>(chosen).copied() {
                let vel = world
                    .get::<Vel>(chosen)
                    .copied()
                    .unwrap_or(Vel { x: 0.0, y: 0.0 });
                let (bullet_speed, keep_velocity) = content
                    .bullet(weapon.bullet.id)
                    .map(|bullet| (bullet.speed, bullet.keep_velocity))
                    .unwrap_or((0.0, true));
                aim = Some(crate::combat::targeting::intercept_positions(
                    (pos.x, pos.y),
                    (src_vel.x, src_vel.y),
                    (tpos.x, tpos.y),
                    (vel.x, vel.y),
                    keep_velocity,
                    bullet_speed,
                ));
            }
        }
        // `mount.shoot = mount.rotate = shoot; if(!shouldFire()) mount.shoot = false;`
        let mut rotate = shoot;
        if hold_fire {
            shoot = false;
        }
        if mount_target.is_none()
            && !shoot
            && crate::weapons::angle_dist(mount_rotation, weapon.base_rotation) >= 0.01
            && state.no_target_time >= ROTATE_BACK_TIMER
        {
            rotate = true;
            let x = mount_x + crate::weapons::trnsx(rotation + weapon.base_rotation, 5.0, 0.0);
            let y = mount_y + crate::weapons::trnsy(rotation + weapon.base_rotation, 5.0, 0.0);
            aim = Some((x, y));
        }
        updates.push((mount_target, shoot, rotate, aim));
    }

    if let Some(mut stored) = world.get_mut::<AiWeaponsState>(entity) {
        *stored = state;
    } else if world.get_entity(entity).is_ok() {
        world.entity_mut(entity).insert(state);
    }
    let Some(mut weapons) = world.get_mut::<UnitWeapons>(entity) else {
        return;
    };
    for (index, (mount_target, shoot, rotate, aim)) in updates.into_iter().enumerate() {
        let Some(mount) = weapons.mounts.get_mut(index) else {
            continue;
        };
        mount.target = mount_target;
        mount.shoot = shoot;
        mount.rotate = rotate;
        if let Some((x, y)) = aim {
            mount.aim_x = x;
            mount.aim_y = y;
        }
    }
}
