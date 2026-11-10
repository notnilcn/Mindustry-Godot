// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret update engine (`TurretBuild.updateTile` reload/target/shoot pipeline).
//! Upstream: core/src/mindustry/world/blocks/defense/turrets/Turret.java

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::ai::types::player::PlayerInput;
use crate::combat::bullet::CombatCtx;
use crate::combat::targeting::TargetQueries;
use crate::entities::comp::unit::comp::UnitCore;
use crate::entities::comp::{Pos, TeamComp};
use crate::world::blocks::defense::turrets::advanced;
use crate::world::modules::LiquidModule;

use super::ammo::{can_consume, has_ammo_state, peek_ammo_state, turret_range_of};
use super::build_turret::update_build;
use super::math::{
    angle_dist, angle_to, approach_delta, cheating, efficiency, lerp_delta, move_toward, position,
};
use super::targeting::{
    bullet, find_target, shoot, target_position, validate_target, within_target_range,
};
use super::{PendingShot, TurretAmmo, TurretConfig, TurretKind, TurretState, turret};

// ---- update engine ----

/// Updates every turret entity one tick (`Groups.build` turret half).
pub fn update_turrets(ctx: &mut CombatCtx<'_>) {
    // Plan-10 §3.2/§3.7: build the plan-11 `BlockIndexer` + unit snapshot once
    // per pass and query it deterministically instead of scanning all entities.
    let targets = TargetQueries::build(ctx.world, ctx.content);
    let turrets: Vec<Entity> = ctx
        .world
        .iter_entities()
        .filter(|entity| entity.contains::<TurretState>())
        .map(|entity| entity.id())
        .collect();
    for e in turrets {
        if turret(ctx.world, e).is_none() {
            continue;
        }
        // `Building.update()`: `if(enabled || !block.noUpdateDisabled)
        // updateTile();` — `Turret` keeps the default `noUpdateDisabled =
        // false`, so a disabled turret does nothing (H09 T4). Fixture entities
        // without a `Building` component (harness turrets) still update.
        if ctx
            .world
            .get::<crate::entities::comp::Building>(e)
            .is_some_and(|building| !building.enabled)
        {
            continue;
        }
        let Some(mut state) = ctx.world.entity_mut(e).take::<TurretState>() else {
            continue;
        };
        update_turret(ctx, e, &mut state, &targets);
        ctx.world.entity_mut(e).insert(state);
    }
}

pub(super) fn update_turret(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    targets: &TargetQueries,
) {
    let config = state.config.clone();
    let (x, y) = position(ctx.world, e).unwrap_or((0.0, 0.0));
    let team = ctx.team(e);
    // `TurretBuild.updateTile` (`Turret.java:544-546`): `heatReq =
    // calculateHeat(sideHeat)` — the heat network pulls from adjacent
    // producers/conductors (plan 09 `heat::calculate_heat`).
    if config.heat_requirement > 0.0 {
        let update_id = crate::world::update::build_time(ctx.world) as u64;
        let mut side_heat = [0.0f32; 4];
        let mut came_from = crate::util::IdSet::new();
        state.heat_req = crate::world::blocks::heat::calculate_heat(
            ctx.world,
            e,
            &mut side_heat,
            &mut came_from,
            update_id,
            0,
        );
    }
    // `TurretBuild.updateEfficiencyMultiplier` (`Turret.java:677-681`).
    let eff = update_efficiency_multiplier(ctx.world, e, efficiency(ctx.world, e), state, &config);

    // `Time.run(delay, …)` tasks run at the start of the frame before the
    // building update (Arc `Time.update`), so a delayed shot that comes due this
    // tick fires before reload/charge/shoot (`Turret.java:763-778`).
    update_pending_shots(ctx, e, state);

    // `PayloadTurretBuild.updateTile`: `totalAmmo = payloads.total()`.
    if let TurretAmmo::Payload(_) = &config.ammo {
        state.total_ammo = state.payloads.iter().map(|stack| stack.count).sum();
    }

    // `TurretBuild.updateTile`: `unit.tile(this); unit.rotation(rotation);
    // unit.team(team);` (`Turret.java:535-537`) — keep the detached proxy on
    // the turret (I01 T12).
    if let Some(unit) = state.block_unit
        && ctx.world.get_entity(unit).is_ok()
    {
        if let Some(mut pos) = ctx.world.get_mut::<Pos>(unit) {
            pos.x = x;
            pos.y = y;
        }
        if let Some(mut core) = ctx.world.get_mut::<UnitCore>(unit) {
            core.rotation = state.rotation;
        }
        if let Some(mut unit_team) = ctx.world.get_mut::<TeamComp>(unit) {
            unit_team.team = team;
        }
        if let Some(building) = ctx.world.get::<crate::entities::comp::Building>(e) {
            let packed = building.tile.pack();
            ctx.world
                .entity_mut(unit)
                .insert(crate::entities::comp::unit::comp::BlockUnitComp { tile: packed });
        }
    }

    // `TurretBuild.controlled()` = `unit.isPlayer()` (`Turret.java:448-450`):
    // the proxy carries the `Player` marker while possessed; the player's aim
    // and shoot button come from the shared input resource
    // (`PlayerInput.aim_x/aim_y/shooting`).
    let controlled = state.block_unit.is_some_and(|unit| {
        ctx.world
            .get::<crate::entities::comp::Player>(unit)
            .is_some()
    });
    let player_aim = ctx
        .world
        .get_resource::<PlayerInput>()
        .map(|input| (input.aim_x, input.aim_y, input.shooting))
        .unwrap_or((0.0, 0.0, false));

    // Target validation + shooting intent.
    // `Turret.updateTile` (`Turret.java:540-542`): the logic cooldown ticks
    // down at the top of the update while active.
    if state.logic_control_time > 0.0 {
        state.logic_control_time -= 1.0;
    }
    let logic_controlled = state.logic_control_time > 0.0;
    if state.target.is_some() && !logic_controlled && !validate_target(ctx, e, state) {
        state.target = None;
    }
    // `isShooting = alwaysShooting || (isControlled() ? unit.isShooting() :
    //  logicControlled() ? logicShooting : target != null)`
    // (`Turret.java:555`; the port has no player-controlled turret yet).
    state.is_shooting = config.always_shooting
        || if controlled {
            // `unit.controller() instanceof Player ? unit.isShooting()`
            // (`Turret.java:555`).
            player_aim.2
        } else if logic_controlled {
            state.logic_shooting
        } else {
            state.target.is_some()
        };

    // Warmup.
    let mut warmup_target =
        if (state.is_shooting && can_consume(state, ctx.world, e, &config)) || charging(state) {
            1.0
        } else {
            0.0
        };
    if warmup_target > 0.0 {
        state.warmup_hold = 1.0;
    }
    if state.warmup_hold > 0.0 {
        if config.warmup_maintain_time > 0.0 {
            state.warmup_hold -= 1.0 / config.warmup_maintain_time;
        } else {
            state.warmup_hold = 0.0;
        }
        state.warmup_hold = state.warmup_hold.max(0.0);
        warmup_target = 1.0;
    }
    let warmup_speed = config.shoot_warmup_speed * if warmup_target > 0.0 { eff } else { 1.0 };
    state.shoot_warmup = if config.linear_warmup {
        approach_delta(state.shoot_warmup, warmup_target, warmup_speed)
    } else {
        lerp_delta(state.shoot_warmup, warmup_target, warmup_speed)
    };

    state.was_shooting = false;
    let recoil_time = if config.recoil_time <= 0.0 {
        config.reload
    } else {
        config.recoil_time
    }
    .max(0.0001);
    state.cur_recoil = approach_delta(state.cur_recoil, 0.0, 1.0 / recoil_time);
    if config.recoils > 0 {
        let count = config.recoils as usize;
        if state.cur_recoils.len() < count {
            state.cur_recoils.resize(count, 0.0);
        }
        for i in 0..count {
            state.cur_recoils[i] = approach_delta(state.cur_recoils[i], 0.0, 1.0 / recoil_time);
        }
    }
    let cooldown = if config.cooldown_time <= 0.0 {
        config.reload
    } else {
        config.cooldown_time
    }
    .max(0.0001);
    state.heat = approach_delta(state.heat, 0.0, 1.0 / cooldown);
    state.charge = if charging(state) && config.shoot.first_shot_delay > 0.0 {
        approach_delta(state.charge, 1.0, 1.0 / config.shoot.first_shot_delay)
    } else {
        0.0
    };

    // Reload + coolant (always runs; upstream `handleReload` precedes activation).
    handle_reload(ctx, e, state, &config, eff);

    if state.activation_timer > 0.0 {
        state.activation_timer -= 1.0;
        return;
    }

    // M6 advanced turret classes dispatch before the generic item/liquid/power
    // shooting pipeline (`PointDefenseTurret`/`TractorBeamTurret`/
    // `ContinuousTurret`/`LaserTurret`).
    match config.kind {
        TurretKind::PointDefense => {
            advanced::update_point_defense(ctx, e, state, &config, x, y, team, eff);
            return;
        }
        TurretKind::TractorBeam => {
            advanced::update_tractor(ctx, e, state, &config, x, y, team, eff, targets);
            return;
        }
        TurretKind::Continuous => {
            advanced::update_continuous(ctx, e, state, &config, x, y, team, eff, targets);
            return;
        }
        TurretKind::Laser => {
            advanced::update_laser(ctx, e, state, &config, x, y, team, eff, targets);
            return;
        }
        TurretKind::Build => {
            update_build(ctx, e, state, &config, x, y, eff);
            return;
        }
        _ => {}
    }

    if !has_ammo_state(state, ctx.world, e) {
        return;
    }

    // Target acquisition on the interval timer: `timer(timerTarget, target !=
    // null ? newTargetInterval : targetInterval)` (`Turret.java:571`).
    state.target_timer -= 1.0;
    if state.target_timer <= 0.0 {
        find_target(ctx, e, state, team, x, y, targets);
        state.target_timer = if state.target.is_some() {
            config.new_target_interval.max(0.0001)
        } else {
            config.target_interval
        };
    }

    // `Turret.updateTile` (`Turret.java:571-605`): with a valid target the aim
    // point is re-derived every tick (`targetPosition` leads through
    // `Predict.intercept`); logic control drives firing and keeps whatever aim
    // point logic last set (`validateTarget` holds a null target while
    // `logicControlled()`).
    let can_shoot = if controlled {
        // `Turret.java:575-582` player behavior: `targetPos.set(unit.aimX(),
        // unit.aimY()); canShoot = unit.isShooting();`.
        state.target_pos = (player_aim.0, player_aim.1);
        player_aim.2
    } else if state.logic_control_time > 0.0 {
        state.logic_shooting
    } else {
        let Some(target) = state.target else {
            return;
        };
        target_position(ctx, e, state, x, y, target);
        within_target_range(ctx, target, x, y, turret_range_of(state, ctx.world, e))
    };

    // Turn toward the aim point (`if(shouldTurn()) turnToTarget(targetRot)`,
    // `Turret.java:592-594`).
    let target_rot = angle_to(x, y, state.target_pos.0, state.target_pos.1);
    if should_turn(state) {
        state.rotation = move_toward(state.rotation, target_rot, config.rotate_speed * eff);
    }

    let in_cone = angle_dist(state.rotation, target_rot) < config.shoot_cone;
    if config.always_shooting || (in_cone && can_shoot) {
        state.was_shooting = true;
        update_shooting(ctx, e, state);
    }
}

/// `TurretBuild.updateEfficiencyMultiplier` (`Turret.java:677-681`):
/// `efficiency *= min(max(heatReq / heatRequirement, cheating() ? 1 : 0),
/// maxHeatEfficiency)`.
pub(super) fn update_efficiency_multiplier(
    world: &World,
    e: Entity,
    eff: f32,
    state: &TurretState,
    config: &TurretConfig,
) -> f32 {
    if config.heat_requirement <= 0.0 {
        return eff;
    }
    let floor = if cheating(world, e) { 1.0 } else { 0.0 };
    eff * (state.heat_req / config.heat_requirement)
        .max(floor)
        .min(config.max_heat_efficiency)
}

/// `TurretBuild.handleReload` + `updateReload` + `updateCooling`.
pub(super) fn handle_reload(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    eff: f32,
) {
    // `TurretBuild.handleReload`: `(reloadWhileCharging || !charging()) &&
    // reloadCounter < reload`. `reloadWhileCharging` is the class default true
    // (`Turret.java:88`) and no vanilla block overrides it, so charging does not
    // pause reload here.
    // `baseReloadSpeed() == efficiency`, which is zero without power/heat
    // (`TurretBuild.updateReload` × `updateEfficiencyMultiplier`).
    if !can_consume(state, ctx.world, e, config) {
        return;
    }
    if state.reload_counter < config.reload {
        let ammo = peek_ammo_state(state, ctx.world, e);
        state.reload_counter += config.ammo_reload_multiplier(ammo, ctx.content) * eff;
        // `ReloadTurretBuild.updateCooling` (`coolant.efficiency > 0 &&
        // efficiency > 0`): `getConsumed` picks the module's CURRENT filter
        // liquid first (`ConsumeLiquidFilter.java:57-71`), `coolant.update`
        // drains it and the reload gains
        // `amount * edelta * heatCapacity * coolantMultiplier * ammoReloadMultiplier`.
        if config.uses_coolant() && eff > 0.0 {
            let consumed = ctx.world.get::<LiquidModule>(e).and_then(|liquids| {
                crate::world::update::consumed_liquid(liquids, &|liquid| {
                    config.coolant_liquids.contains(&liquid)
                })
                .map(|l| (l, liquids.get(l)))
            });
            if let Some((liquid, stored)) = consumed {
                let needed = config.coolant_amount * eff;
                let coolant_eff = (stored / needed.max(1e-8)).min(1.0);
                let capacity = ctx.content.liquid(liquid).map_or(0.4, |l| l.heat_capacity);
                if let Some(mut liquids) = ctx.world.get_mut::<LiquidModule>(e) {
                    liquids.remove(liquid, needed);
                }
                state.reload_counter += config.coolant_amount
                    * coolant_eff
                    * eff
                    * capacity
                    * config.coolant_multiplier
                    * config.ammo_reload_multiplier(ammo, ctx.content);
            }
        }
    }
}

/// `TurretBuild.charging()` (`Turret.java:718-720`): `queuedBullets > 0 &&
/// shoot.firstShotDelay > 0` — a shot is waiting out its pattern delay.
pub(super) fn charging(state: &TurretState) -> bool {
    !state.pending.is_empty() && state.config.shoot.first_shot_delay > 0.0
}

/// `TurretBuild.shouldTurn()` (`Turret.java:672-674`): `moveWhileCharging ||
/// !charging()`; `lancer` sets `moveWhileCharging = false` (`Blocks.java:3594`).
pub(super) fn should_turn(state: &TurretState) -> bool {
    state.config.move_while_charging || !charging(state)
}

/// Fires the queued shots whose `delay` elapsed this tick (`Time.run` task
/// pass). Order is insertion order, so the pattern's per-shot draws stay
/// deterministic.
pub(super) fn update_pending_shots(ctx: &mut CombatCtx<'_>, e: Entity, state: &mut TurretState) {
    if state.pending.is_empty() {
        return;
    }
    let mut due: SmallVec<[PendingShot; 4]> = SmallVec::new();
    let mut remain: SmallVec<[PendingShot; 4]> = SmallVec::new();
    for mut shot in std::mem::take(&mut state.pending) {
        shot.delay -= 1.0;
        if shot.delay <= 0.0 {
            due.push(shot);
        } else {
            remain.push(shot);
        }
    }
    state.pending = remain;
    for shot in due {
        // `bullet(type, xOffset, yOffset, angle, mover)` with the barrel counter
        // restored (`Turret.java:788-795`).
        bullet(ctx, e, state, shot.bullet, &shot.shot, shot.barrel);
    }
}

/// `TurretBuild.updateShooting`.
pub(super) fn update_shooting(ctx: &mut CombatCtx<'_>, e: Entity, state: &mut TurretState) {
    if state.reload_counter >= state.config.reload
        && !charging(state)
        && state.shoot_warmup >= state.config.min_warmup
    {
        if let Some(bullet) = peek_ammo_state(state, ctx.world, e) {
            shoot(ctx, e, state, bullet);
        }
        let reload = state.config.reload.max(0.0001);
        state.reload_counter %= reload;
    }
}
