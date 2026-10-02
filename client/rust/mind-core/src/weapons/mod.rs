// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Weapon behavior engine (`core/src/mindustry/type/Weapon.java` behavior half).
//!
//! Plan 02 owns [`WeaponDef`] metadata and [`ShootPatternSpec`] data; this module
//! owns `Weapon.update`/`shoot`/`bullet`/`flip`, [`WeaponMount`] state and the
//! shoot-pattern execution. Mounts are stored on the [`UnitWeapons`] component
//! (plan 11 will own the real unit-side storage and drive
//! [`update_weapons`] from the unit update set).

pub mod build_weapon;
pub mod mine_weapon;
pub mod mount;
pub mod pattern;
pub mod point_defense_bullet_weapon;
pub mod point_defense_weapon;
pub mod repair_beam_weapon;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;

use crate::combat::bullet::{Bullet, BulletSpawn, CombatCtx};
use crate::content::Rgba;
use crate::content::registries::units::weapon::{ShootPatternSpec, WeaponDef, WeaponKind};
use crate::determinism::RngStream;
use crate::entities::comp::{Health, Pos, TeamComp, Vel};

pub use mount::{HealBeamMount, WeaponMount};
pub use pattern::{BulletHandler, PatternShot, ShotBuffer};

/// Unit-side weapon/controller state the plan-11 unit will provide; the M4
/// fixture stores it directly on the test entity.
#[derive(Debug, Clone, Copy, Component)]
pub struct UnitState {
    /// Unit rotation in degrees (`Unit.rotation`).
    pub rotation: f32,
    /// Reload multiplier (`Unit.reloadMultiplier`).
    pub reload_multiplier: f32,
    /// Whether the unit can act (`Unit.canShoot()`; stunned/disabled gate).
    pub can_shoot: bool,
    /// Whether the unit is remotely simulated (`isRemote`).
    pub is_remote: bool,
    /// Movement delta length (`deltaLen`); used for shoot-velocity gates.
    pub delta_len: f32,
    /// Active build-plan draw position (`unit.buildPlan().drawx/y`; plan 11).
    pub build_plan: Option<(f32, f32)>,
    /// Active mine tile draw position (`Tile.drawx/y`; plan 11).
    pub mine_tile: Option<(f32, f32)>,
}

impl Default for UnitState {
    fn default() -> Self {
        Self {
            rotation: 0.0,
            reload_multiplier: 1.0,
            can_shoot: true,
            is_remote: false,
            delta_len: 0.0,
            build_plan: None,
            mine_tile: None,
        }
    }
}

/// Weapon + mount storage for one unit (plan-11 hand-off placeholder).
#[derive(Debug, Clone, Component)]
pub struct UnitWeapons {
    /// Resolved weapons (mirror pass already applied).
    pub weapons: Vec<WeaponDef>,
    /// Per-weapon mount state (same indices as `weapons`).
    pub mounts: Vec<WeaponMount>,
    /// Delayed shots queued by `ShootPattern` `shotDelay`/`firstShotDelay`.
    pub pending: Vec<PendingShot>,
}

/// A shot scheduled for a future tick (`Weapon.shoot` `Time.run` delay branch).
#[derive(Debug, Clone, Copy)]
pub struct PendingShot {
    /// Remaining delay in ticks.
    pub delay: f32,
    /// Owning unit.
    pub unit: Entity,
    /// Weapon index on the unit.
    pub index: usize,
    /// Pattern shot payload.
    pub shot: PatternShot,
    /// Base shoot angle at emission time (`bulletRotation`).
    pub rotation: f32,
}

impl UnitWeapons {
    /// Builds weapon/mount lists from unit weapon defs, porting the
    /// `UnitType.init` mirror pass (`type/UnitType.java:1037`).
    pub fn from_defs(defs: &[WeaponDef]) -> Self {
        let mut mapped: Vec<WeaponDef> = Vec::new();
        for def in defs {
            let mut weapon = def.clone();
            if weapon.recoil_time < 0.0 {
                weapon.recoil_time = weapon.reload;
            }
            mapped.push(weapon.clone());
            if weapon.mirror {
                let mut copy = weapon.clone();
                copy.flip();
                mapped.push(copy);
                let len = mapped.len();
                mapped[len - 2].recoil_time *= 2.0;
                mapped[len - 1].recoil_time *= 2.0;
                mapped[len - 2].reload *= 2.0;
                mapped[len - 1].reload *= 2.0;
                mapped[len - 2].other_side = (len - 1) as i32;
                mapped[len - 1].other_side = (len - 2) as i32;
            }
        }
        for weapon in &mut mapped {
            weapon.init();
        }
        let mounts = mapped.iter().map(WeaponMount::new).collect();
        Self {
            weapons: mapped,
            mounts,
            pending: Vec::new(),
        }
    }

    /// Access a mount by index.
    pub fn mount(&self, index: usize) -> Option<&WeaponMount> {
        self.mounts.get(index)
    }

    /// Mutable access to a mount by index.
    pub fn mount_mut(&mut self, index: usize) -> Option<&mut WeaponMount> {
        self.mounts.get_mut(index)
    }

    /// Advances every mount one tick (`Weapon.update`) and fires due shots.
    pub fn update(&mut self, ctx: &mut CombatCtx<'_>, unit: Entity, state: &UnitState) {
        // Process delayed shots first (deterministic tick ordering).
        let due: Vec<PendingShot> = {
            let mut due = Vec::new();
            let mut remain = Vec::with_capacity(self.pending.len());
            for mut pending in std::mem::take(&mut self.pending) {
                pending.delay -= 1.0;
                if pending.delay <= 0.0 {
                    due.push(pending);
                } else {
                    remain.push(pending);
                }
            }
            self.pending = remain;
            due
        };

        let UnitWeapons {
            weapons,
            mounts,
            pending,
        } = self;
        for shot in due {
            if let Some(weapon) = weapons.get(shot.index) {
                let mount = &mut mounts[shot.index];
                spawn_weapon_bullet(ctx, shot.unit, weapon, mount, &shot.shot, shot.rotation);
            }
        }
        for index in 0..weapons.len() {
            // Visual-only weapons aim first, then run the shared engine (which
            // skips firing because `mount.shoot == false`).
            match weapons[index].kind {
                WeaponKind::BuildWeapon | WeaponKind::MineWeapon => {
                    let weapon = &weapons[index];
                    let unit_rotation = state.rotation;
                    let (unit_x, unit_y) = ctx.pos(unit).unwrap_or((0.0, 0.0));
                    let mount_x = unit_x + trnsx(unit_rotation - 90.0, weapon.x, weapon.y);
                    let mount_y = unit_y + trnsy(unit_rotation - 90.0, weapon.x, weapon.y);
                    let weapon_rotation = unit_rotation - 90.0
                        + if weapon.rotate {
                            mounts[index].rotation
                        } else {
                            weapon.base_rotation
                        };
                    let front = (
                        mount_x + trnsx(weapon_rotation, weapon.shoot_x, weapon.shoot_y),
                        mount_y + trnsy(weapon_rotation, weapon.shoot_x, weapon.shoot_y),
                    );
                    if weapons[index].kind == WeaponKind::BuildWeapon {
                        build_weapon::aim(&mut mounts[index], front, state.build_plan);
                    } else {
                        mine_weapon::aim(&mut mounts[index], front, state.mine_tile);
                    }
                }
                _ => {}
            }
            update_weapon(ctx, unit, state, weapons, mounts, pending, index);
            if weapons[index].kind == WeaponKind::RepairBeamWeapon {
                repair_beam_weapon::update(ctx, unit, &weapons[index], &mut mounts[index]);
            }
        }
    }

    /// Mutable access used by tests for direct setup.
    pub fn set_pending(&mut self, pending: Vec<PendingShot>) {
        self.pending = pending;
    }

    /// Number of queued delayed shots.
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

// ---- math helpers (`Angles`/`Mathf` subset, delta == 1) ----

fn trnsx(angle_deg: f32, x: f32, y: f32) -> f32 {
    let r = angle_deg.to_radians();
    x * r.cos() + y * r.sin()
}

fn trnsy(angle_deg: f32, x: f32, y: f32) -> f32 {
    let r = angle_deg.to_radians();
    x * r.sin() - y * r.cos()
}

fn angle_to(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    (y2 - y1).atan2(x2 - x1).to_degrees()
}

/// `Angles.angleDist(a, b)`.
pub fn angle_dist(a: f32, b: f32) -> f32 {
    let d = (b - a).rem_euclid(360.0);
    let d = if d > 180.0 { 360.0 - d } else { d };
    d.abs()
}

/// `Angles.within(from, to, angle)`.
pub fn within(from: f32, to: f32, angle: f32) -> bool {
    angle_dist(from, to) <= angle
}

/// `Angles.moveToward(from, to, speed)`.
fn move_toward(from: f32, to: f32, speed: f32) -> f32 {
    if from < to {
        (from + speed).min(to)
    } else {
        (from - speed).max(to)
    }
}

/// `Mathf.approachDelta(from, to, speed)` with `Time.delta == 1`.
fn approach_delta(from: f32, to: f32, speed: f32) -> f32 {
    move_toward(from, to, speed)
}

/// `Mathf.lerpDelta(from, to, alpha)` with `Time.delta == 1`.
fn lerp_delta(from: f32, to: f32, alpha: f32) -> f32 {
    from + (to - from) * alpha.clamp(0.0, 1.0)
}

fn sign(value: bool) -> f32 {
    if value { -1.0 } else { 1.0 }
}

/// `Weapon.bulletRotation`.
#[allow(clippy::too_many_arguments)]
fn bullet_rotation(
    unit_rotation: f32,
    weapon: &WeaponDef,
    mount_rotation: f32,
    aim_x: f32,
    aim_y: f32,
    unit_x: f32,
    unit_y: f32,
    bullet_x: f32,
    bullet_y: f32,
) -> f32 {
    if weapon.rotate {
        unit_rotation + mount_rotation
    } else {
        angle_to(bullet_x, bullet_y, aim_x, aim_y)
            + (unit_rotation - angle_to(unit_x, unit_y, aim_x, aim_y))
            + weapon.base_rotation
    }
}

/// Ports `Weapon.update(Unit, WeaponMount)`.
#[allow(clippy::too_many_arguments)]
pub fn update_weapon(
    ctx: &mut CombatCtx<'_>,
    unit: Entity,
    state: &UnitState,
    defs: &[WeaponDef],
    mounts: &mut [WeaponMount],
    pending: &mut Vec<PendingShot>,
    index: usize,
) {
    let Some(weapon) = defs.get(index) else {
        return;
    };
    let can = state.can_shoot;
    let delta = 1.0f32;
    let (unit_x, unit_y) = ctx.pos(unit).unwrap_or((0.0, 0.0));
    let (vel_x, vel_y) = ctx
        .world
        .get::<Vel>(unit)
        .map(|v| (v.x, v.y))
        .unwrap_or((0.0, 0.0));
    let unit_rotation = state.rotation;
    let reload = weapon.reload.max(0.0001);
    let recoil_time = if weapon.recoil_time <= 0.0 {
        reload
    } else {
        weapon.recoil_time
    }
    .max(0.0001);

    let last_reload = {
        let mount = &mut mounts[index];
        let last_reload = mount.reload;
        mount.reload = (mount.reload - delta * state.reload_multiplier).max(0.0);
        mount.recoil = approach_delta(mount.recoil, 0.0, state.reload_multiplier / recoil_time);
        if weapon.recoils > 0 {
            let count = weapon.recoils as usize;
            if mount.recoils.len() < count {
                mount.recoils.resize(count, 0.0);
            }
            for i in 0..count {
                mount.recoils[i] =
                    approach_delta(mount.recoils[i], 0.0, state.reload_multiplier / recoil_time);
            }
        }
        mount.smooth_reload = lerp_delta(
            mount.smooth_reload,
            mount.reload / reload,
            weapon.smooth_reload_speed,
        );
        mount.charge = if mount.charging && weapon.shoot.first_shot_delay > 0.0 {
            approach_delta(mount.charge, 1.0, 1.0 / weapon.shoot.first_shot_delay)
        } else {
            0.0
        };
        let warmup_target = if (can && mount.shoot)
            || (weapon.continuous && mount.bullet.is_some())
            || mount.charging
        {
            1.0
        } else {
            0.0
        };
        if weapon.linear_warmup {
            mount.warmup = approach_delta(mount.warmup, warmup_target, weapon.shoot_warmup_speed);
        } else {
            mount.warmup = lerp_delta(mount.warmup, warmup_target, weapon.shoot_warmup_speed);
        }
        last_reload
    };

    let mount_x = unit_x + trnsx(unit_rotation - 90.0, weapon.x, weapon.y);
    let mount_y = unit_y + trnsy(unit_rotation - 90.0, weapon.x, weapon.y);

    // Auto-targeting (requires `controllable = false`). The actual target
    // *search* is plan 11's `TargetQueries`; this keeps the stored target and
    // validates it against range, matching the `Weapon.update` control flow.
    if !weapon.controllable && weapon.auto_target {
        {
            let mount = &mut mounts[index];
            mount.retarget -= delta;
            if mount.retarget <= 0.0 {
                mount.retarget = if mount.target.is_none() {
                    weapon.target_interval
                } else {
                    weapon.target_switch_interval
                };
            }
        }
        let mut shoot = false;
        if let Some(target) = mounts[index].target
            && let Some((target_x, target_y)) = ctx.pos(target)
        {
            let range = weapon.range() + weapon.shoot_y.abs();
            let dst = ((target_x - mount_x).powi(2) + (target_y - mount_y).powi(2)).sqrt();
            shoot = can && dst <= range;
            let mount = &mut mounts[index];
            mount.aim_x = target_x;
            mount.aim_y = target_y;
        }
        let mount = &mut mounts[index];
        mount.shoot = shoot;
        mount.rotate = shoot;
    }

    // Rotation.
    if weapon.rotate && (mounts[index].rotate || mounts[index].shoot) && can {
        let axis_x = unit_x + trnsx(unit_rotation - 90.0, weapon.x, weapon.y);
        let axis_y = unit_y + trnsy(unit_rotation - 90.0, weapon.x, weapon.y);
        let aim = (mounts[index].aim_x, mounts[index].aim_y);
        let mount = &mut mounts[index];
        mount.target_rotation = angle_to(axis_x, axis_y, aim.0, aim.1) - unit_rotation;
        mount.rotation = move_toward(
            mount.rotation,
            mount.target_rotation,
            weapon.rotate_speed * delta,
        );
        if weapon.rotation_limit < 360.0 {
            let dst = angle_dist(mount.rotation, weapon.base_rotation);
            if dst > weapon.rotation_limit / 2.0 {
                mount.rotation = move_toward(
                    mount.rotation,
                    weapon.base_rotation,
                    dst - weapon.rotation_limit / 2.0,
                );
            }
        }
    } else if !weapon.rotate {
        let aim = (mounts[index].aim_x, mounts[index].aim_y);
        let mount = &mut mounts[index];
        mount.rotation = weapon.base_rotation;
        mount.target_rotation = angle_to(unit_x, unit_y, aim.0, aim.1);
    }

    let weapon_rotation = unit_rotation - 90.0
        + if weapon.rotate {
            mounts[index].rotation
        } else {
            weapon.base_rotation
        };
    let bullet_x = mount_x + trnsx(weapon_rotation, weapon.shoot_x, weapon.shoot_y);
    let bullet_y = mount_y + trnsy(weapon_rotation, weapon.shoot_x, weapon.shoot_y);
    let shoot_angle = bullet_rotation(
        unit_rotation,
        weapon,
        mounts[index].rotation,
        mounts[index].aim_x,
        mounts[index].aim_y,
        unit_x,
        unit_y,
        bullet_x,
        bullet_y,
    );

    if weapon.always_shooting {
        mounts[index].shoot = true;
    }

    // Continuous state (`Weapon.update` continuous branch).
    if weapon.continuous && mounts[index].bullet.is_some() {
        let bullet = mounts[index].bullet;
        let valid = bullet
            .and_then(|b| {
                ctx.world
                    .get::<Bullet>(b)
                    .map(|state| state.def == weapon.bullet.id && state.time < state.lifetime)
            })
            .unwrap_or(false);
        if !valid {
            mounts[index].bullet = None;
        } else if let Some(bullet) = bullet {
            let gun_rotation = unit_rotation - 90.0
                + if weapon.rotate {
                    mounts[index].rotation
                } else {
                    weapon.base_rotation
                };
            let gun_x = unit_x + trnsx(unit_rotation - 90.0, weapon.x, weapon.y);
            let gun_y = unit_y + trnsy(unit_rotation - 90.0, weapon.x, weapon.y);
            let bx = gun_x + trnsx(gun_rotation, weapon.shoot_x, weapon.shoot_y);
            let by = gun_y + trnsy(gun_rotation, weapon.shoot_x, weapon.shoot_y);
            let aim = (mounts[index].aim_x, mounts[index].aim_y);
            let (last_length, barrel_counter) =
                (mounts[index].last_length, mounts[index].barrel_counter);
            let range = weapon.range();
            let shoot_length = (((aim.0 - bx).powi(2) + (aim.1 - by).powi(2)).sqrt()).min(range);
            let cur_length = ctx
                .world
                .get::<Bullet>(bullet)
                .map(|state| ((state.aim.0 - bx).powi(2) + (state.aim.1 - by).powi(2)).sqrt())
                .unwrap_or(0.0);
            let result_length = move_toward(cur_length, shoot_length, weapon.aim_change_speed);
            let end_x = bx + trnsx(shoot_angle, result_length, 0.0);
            let end_y = by + trnsy(shoot_angle, result_length, 0.0);
            if let Some(mut state) = ctx.world.get_mut::<Bullet>(bullet) {
                state.rotation = gun_rotation + 90.0;
                state.aim = (end_x, end_y);
                if weapon.always_continuous && mounts[index].shoot {
                    let optimal = ctx
                        .content
                        .bullet(weapon.bullet.id)
                        .map(|def| def.optimal_life_fract)
                        .unwrap_or(1.0);
                    state.time = state.lifetime * optimal * mounts[index].warmup;
                    state.set(crate::combat::bullet::KEEP_ALIVE);
                }
            }
            if let Some(mut pos) = ctx.world.get_mut::<Pos>(bullet) {
                pos.x = bx;
                pos.y = by;
            }
            let mount = &mut mounts[index];
            mount.reload = weapon.reload;
            mount.recoil = 1.0;
            mount.last_length = result_length;
            let _ = (last_length, barrel_counter);
            let bullet_recoil = ctx
                .content
                .bullet(weapon.bullet.id)
                .map(|def| def.recoil)
                .unwrap_or(0.0);
            if let Some(mut vel) = ctx.world.get_mut::<Vel>(unit) {
                vel.x += trnsx(shoot_angle + 180.0, bullet_recoil * delta, 0.0);
                vel.y += trnsy(shoot_angle + 180.0, bullet_recoil * delta, 0.0);
            }
        }
    } else {
        let mount = &mut mounts[index];
        mount.heat = (mount.heat
            - delta * state.reload_multiplier / weapon.cooldown_time.max(0.0001))
        .max(0.0);
    }

    // Side flip for alternating mirrored weapons.
    let was_flipped = mounts[index].side;
    if weapon.other_side >= 0 && weapon.alternate {
        let other = weapon.other_side as usize;
        let reload_half = weapon.reload / 2.0;
        if other < mounts.len()
            && mounts[index].side == weapon.flip_sprite
            && mounts[index].reload <= reload_half
            && last_reload > reload_half
        {
            mounts[other].side = !mounts[other].side;
            mounts[index].side = !mounts[index].side;
        }
    }

    let vel_len = if state.is_remote {
        (vel_x * vel_x + vel_y * vel_y).sqrt()
    } else {
        state.delta_len
    };

    let rotate_ref = if weapon.rotate {
        mounts[index].rotation
    } else {
        unit_rotation + weapon.base_rotation
    };
    let within_cone = weapon.always_shooting
        || within(rotate_ref, mounts[index].target_rotation, weapon.shoot_cone);

    let can_fire = mounts[index].shoot
        && can
        && !(weapon.bullet.kill_shooter && mounts[index].total_shots > 0)
        && (!weapon.alternate || was_flipped == weapon.flip_sprite)
        && mounts[index].warmup >= weapon.min_warmup
        && (weapon.min_shoot_velocity < 0.0 || vel_len >= weapon.min_shoot_velocity)
        && (weapon.max_shoot_velocity == -1.0 || vel_len <= weapon.max_shoot_velocity)
        && (mounts[index].reload <= 0.0001
            || (weapon.always_continuous && mounts[index].bullet.is_none()))
        && within_cone;

    if can_fire {
        fire(
            ctx,
            unit,
            defs,
            mounts,
            pending,
            index,
            bullet_x,
            bullet_y,
            shoot_angle,
        );
        mounts[index].reload = weapon.reload;
    }
}

/// Ports `Weapon.shoot` (pattern execution + reload).
#[allow(clippy::too_many_arguments)]
fn fire(
    ctx: &mut CombatCtx<'_>,
    unit: Entity,
    defs: &[WeaponDef],
    mounts: &mut [WeaponMount],
    pending: &mut Vec<PendingShot>,
    index: usize,
    _shoot_x: f32,
    _shoot_y: f32,
    rotation: f32,
) {
    let weapon = &defs[index];
    // `RepairBeamWeapon.shoot` is a no-op; healing runs in `update`.
    if weapon.kind == WeaponKind::RepairBeamWeapon {
        return;
    }
    if weapon.shoot.first_shot_delay > 0.0 {
        mounts[index].charging = true;
        let pitch = ctx.rng.range(
            RngStream::Sim,
            weapon.sound_pitch_min,
            weapon.sound_pitch_max,
        );
        ctx.fx.sound(weapon.charge_sound, 1.0, pitch);
    }
    let mut buffer = ShotBuffer::default();
    let mut barrel = mounts[index].barrel_counter;
    let total = mounts[index].total_shots;
    pattern::emit(&weapon.shoot, total, &mut buffer, &mut barrel, ctx.rng);
    mounts[index].barrel_counter = barrel;
    mounts[index].total_shots += buffer.shots.len() as i32;
    for shot in &buffer.shots {
        if shot.delay > 0.0 {
            pending.push(PendingShot {
                delay: shot.delay,
                unit,
                index,
                shot: *shot,
                rotation,
            });
        } else {
            spawn_weapon_bullet(ctx, unit, weapon, &mut mounts[index], shot, rotation);
        }
    }
}

/// Ports `Weapon.bullet` (world-space muzzle transform + `BulletType.create`).
fn spawn_weapon_bullet(
    ctx: &mut CombatCtx<'_>,
    unit: Entity,
    weapon: &WeaponDef,
    mount: &mut WeaponMount,
    shot: &PatternShot,
    _base_angle: f32,
) {
    if ctx.world.get_entity(unit).is_err() {
        return;
    }
    mount.charging = false;
    let (unit_x, unit_y) = ctx.pos(unit).unwrap_or((0.0, 0.0));
    let unit_rotation = ctx
        .world
        .get::<UnitState>(unit)
        .map(|state| state.rotation)
        .unwrap_or(0.0);
    let team = ctx.team(unit);
    let x_spread = ctx.rng.range(RngStream::Sim, -weapon.x_rand, weapon.x_rand);
    let y_spread = ctx.rng.range(RngStream::Sim, -weapon.y_rand, weapon.y_rand);
    let weapon_rotation = unit_rotation - 90.0
        + if weapon.rotate {
            mount.rotation
        } else {
            weapon.base_rotation
        };
    let mount_x = unit_x + trnsx(unit_rotation - 90.0, weapon.x, weapon.y);
    let mount_y = unit_y + trnsy(unit_rotation - 90.0, weapon.x, weapon.y);
    let bullet_x = mount_x
        + trnsx(
            weapon_rotation,
            weapon.shoot_x + shot.x + x_spread,
            weapon.shoot_y + shot.y + y_spread,
        );
    let bullet_y = mount_y
        + trnsy(
            weapon_rotation,
            weapon.shoot_x + shot.x + x_spread,
            weapon.shoot_y + shot.y + y_spread,
        );
    let shoot_angle = bullet_rotation(
        unit_rotation,
        weapon,
        mount.rotation,
        mount.aim_x,
        mount.aim_y,
        unit_x,
        unit_y,
        bullet_x,
        bullet_y,
    ) + shot.rotation;

    let bullet_def = ctx.content.bullet(weapon.bullet.id);
    let bullet_inaccuracy = bullet_def.map(|def| def.inaccuracy).unwrap_or(0.0);
    let bullet_recoil = bullet_def.map(|def| def.recoil).unwrap_or(0.0);
    let scale_life = bullet_def.map(|def| def.scale_life).unwrap_or(false);
    let range = weapon.range();
    let base_life = (1.0 - weapon.life_rnd)
        + ctx.rng.range(RngStream::Sim, 0.0, weapon.life_rnd)
        + weapon.extra_life;
    let life_scl = if scale_life && range > 0.0 {
        let dst = ((mount.aim_x - bullet_x).powi(2) + (mount.aim_y - bullet_y).powi(2)).sqrt();
        base_life * (dst / range).clamp(0.0, 1.0)
    } else {
        base_life
    };
    let velocity_scl = (1.0 - weapon.velocity_rnd)
        + ctx.rng.range(RngStream::Sim, 0.0, weapon.velocity_rnd)
        + weapon.extra_velocity;
    let inaccuracy = ctx.rng.range(
        RngStream::Sim,
        -(weapon.inaccuracy + bullet_inaccuracy),
        weapon.inaccuracy + bullet_inaccuracy,
    );
    let angle = shoot_angle + inaccuracy;

    let spawn = BulletSpawn {
        def: weapon.bullet.id,
        owner: Some(unit),
        shooter: Some(unit),
        team,
        x: bullet_x,
        y: bullet_y,
        angle,
        velocity_scl,
        lifetime_scl: life_scl,
        mover: shot.mover,
        aim_x: mount.aim_x,
        aim_y: mount.aim_y,
        target: mount.target,
        ..BulletSpawn::default()
    };
    let created = ctx.spawn(&spawn);
    if weapon.continuous {
        mount.bullet = created;
    }

    let pitch = ctx.rng.range(
        RngStream::Sim,
        weapon.sound_pitch_min,
        weapon.sound_pitch_max,
    );
    if weapon.continuous {
        ctx.fx
            .sound(weapon.initial_shoot_sound, weapon.shoot_sound_volume, pitch);
    } else {
        ctx.fx
            .sound(weapon.shoot_sound, weapon.shoot_sound_volume, pitch);
    }

    if mount.allow_shoot_effects {
        ctx.fx.effect(
            &weapon.eject_effect,
            mount_x,
            mount_y,
            angle * sign(weapon.x < 0.0),
            Rgba::WHITE,
        );
    }
    ctx.fx.shake(weapon.shake);

    if let Some(mut vel) = ctx.world.get_mut::<Vel>(unit) {
        vel.x += trnsx(shoot_angle + 180.0, bullet_recoil, 0.0);
        vel.y += trnsy(shoot_angle + 180.0, bullet_recoil, 0.0);
    }
    mount.recoil = 1.0;
    if weapon.recoils > 0 {
        let count = weapon.recoils as usize;
        if mount.recoils.len() >= count {
            mount.recoils[mount.barrel_counter.rem_euclid(count as i32) as usize] = 1.0;
        }
    }
    mount.heat = 1.0;
}

/// Updates every unit in the world that has both [`UnitWeapons`] and
/// [`UnitState`] (`Groups.unit.update` weapon half; plan 11 drives this).
pub fn update_weapons(ctx: &mut CombatCtx<'_>) {
    let units: Vec<Entity> = ctx
        .world
        .iter_entities()
        .filter(|entity| {
            entity.get::<UnitWeapons>().is_some() && entity.get::<UnitState>().is_some()
        })
        .map(|entity| entity.id())
        .collect();
    for unit in units {
        let Some(mut weapons) = ctx.world.entity_mut(unit).take::<UnitWeapons>() else {
            continue;
        };
        let state = ctx
            .world
            .get::<UnitState>(unit)
            .copied()
            .unwrap_or_default();
        weapons.update(ctx, unit, &state);
        ctx.world.entity_mut(unit).insert(weapons);
    }
}

/// Health of a target entity (heal/repair hooks).
pub fn target_health(world: &bevy_ecs::world::World, entity: Entity) -> Option<f32> {
    world.get::<Health>(entity).map(|health| health.health)
}

/// Team of an entity.
pub fn target_team(world: &bevy_ecs::world::World, entity: Entity) -> Option<u8> {
    world.get::<TeamComp>(entity).map(|team| team.team)
}

/// Helper for tests: formats the resolved shoot pattern.
pub fn pattern_of(weapon: &WeaponDef) -> &ShootPatternSpec {
    &weapon.shoot
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;
    use crate::content::registries::units::weapon::{ShootPatternSpec, WeaponSpec};

    fn test_weapon(reload: f32, spec: ShootPatternSpec) -> WeaponDef {
        // The fixture bullet `fuse` is registered on the combat harness; use its
        // id 0 placeholder here and let the harness patch it in tests via
        // `weapon_with_bullet`.
        let registry = crate::content::test_support::test_registry();
        WeaponDef::from_spec(
            WeaponSpec {
                name: "test",
                reload: Some(reload),
                x: Some(0.0),
                shoot_y: Some(0.0),
                recoil: Some(0.0),
                rotate: Some(false),
                mirror: Some(false),
                alternate: Some(false),
                shoot: Some(spec),
                ..WeaponSpec::default()
            },
            crate::content::registries::units::ResolvedBullet {
                id: crate::content::BulletId::new(0),
                range: 120.0,
                heals: false,
                kill_shooter: false,
                dps: 0.0,
            },
            &registry,
        )
        .expect("weapon")
    }

    fn weapon_with_bullet(
        harness: &CombatHarness,
        reload: f32,
        spec: ShootPatternSpec,
        bullet_name: &str,
    ) -> WeaponDef {
        let mut weapon = test_weapon(reload, spec);
        let id = harness.bullet_id(bullet_name).expect("fixture bullet");
        weapon.bullet = crate::content::registries::units::ResolvedBullet {
            id,
            range: 120.0,
            heals: false,
            kill_shooter: false,
            dps: 0.0,
        };
        weapon
    }

    #[test]
    fn mirror_pass_doubles_reload_and_links_sides() {
        let mut weapon = test_weapon(10.0, ShootPatternSpec::plain(1, 0.0, 0.0));
        weapon.mirror = true;
        let unit = UnitWeapons::from_defs(&[weapon]);
        assert_eq!(unit.weapons.len(), 2);
        assert_eq!(unit.weapons[0].reload, 20.0);
        assert_eq!(unit.weapons[1].reload, 20.0);
        assert_eq!(unit.weapons[0].other_side, 1);
        assert_eq!(unit.weapons[1].other_side, 0);
        assert!(unit.weapons[1].flip_sprite);
    }

    #[test]
    fn flip_mirrors_geometry() {
        let mut weapon = test_weapon(10.0, ShootPatternSpec::plain(1, 0.0, 0.0));
        weapon.x = 5.0;
        weapon.shoot_x = 3.0;
        let mut flipped = weapon.clone();
        flipped.flip();
        assert_eq!(flipped.x, -5.0);
        assert_eq!(flipped.shoot_x, -3.0);
        assert!(flipped.flip_sprite);
    }

    #[test]
    fn reload_decays_then_fires() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(12, 8, wall, 0, true));
        let (ux, uy) = CombatHarness::tile_center(4, 8);
        let weapon =
            weapon_with_bullet(&harness, 5.0, ShootPatternSpec::plain(1, 0.0, 0.0), "fuse");
        let unit = harness.spawn_test_unit(ux, uy, 1, vec![weapon]);
        harness.set_unit_aim(unit, 0, CombatHarness::tile_center(12, 8));
        harness.set_unit_shoot(unit, 0, true);
        let before = harness.building_health_at(12, 8);
        for _ in 0..40 {
            harness.tick();
        }
        assert!(
            harness.building_health_at(12, 8) < before,
            "weapon fired and hit the wall"
        );
    }

    #[test]
    fn burst_timing_queues_delays() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let weapon =
            weapon_with_bullet(&harness, 10.0, ShootPatternSpec::plain(3, 4.0, 0.0), "fuse");
        let (ux, uy) = CombatHarness::tile_center(4, 8);
        let unit = harness.spawn_test_unit(ux, uy, 1, vec![weapon]);
        harness.set_unit_aim(unit, 0, CombatHarness::tile_center(12, 8));
        harness.set_unit_shoot(unit, 0, true);
        // One update fires the first shot (delay 0) and queues the other two.
        harness.update_weapons_only();
        let pending = harness
            .unit_weapons(unit)
            .map(|weapons| weapons.pending_len())
            .unwrap_or(0);
        assert_eq!(pending, 2, "delayed shots queued");
    }

    #[test]
    fn min_max_shoot_velocity_gate() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let mut weapon =
            weapon_with_bullet(&harness, 1.0, ShootPatternSpec::plain(1, 0.0, 0.0), "fuse");
        weapon.min_shoot_velocity = 5.0;
        let (ux, uy) = CombatHarness::tile_center(4, 8);
        let unit = harness.spawn_test_unit(ux, uy, 1, vec![weapon]);
        harness.set_unit_aim(unit, 0, CombatHarness::tile_center(12, 8));
        harness.set_unit_shoot(unit, 0, true);
        // delta_len = 0 < 5 => no shot.
        harness.update_weapons_only();
        assert_eq!(harness.unit_weapons(unit).unwrap().mounts[0].total_shots, 0);
        harness.set_unit_delta_len(unit, 10.0);
        harness.update_weapons_only();
        assert!(harness.unit_weapons(unit).unwrap().mounts[0].total_shots > 0);
    }

    #[test]
    fn shoot_on_death_gate_stops_after_first() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let mut weapon =
            weapon_with_bullet(&harness, 1.0, ShootPatternSpec::plain(1, 0.0, 0.0), "fuse");
        weapon.bullet.kill_shooter = true;
        let (ux, uy) = CombatHarness::tile_center(4, 8);
        let unit = harness.spawn_test_unit(ux, uy, 1, vec![weapon]);
        harness.set_unit_aim(unit, 0, CombatHarness::tile_center(12, 8));
        harness.set_unit_shoot(unit, 0, true);
        for _ in 0..5 {
            harness.update_weapons_only();
        }
        assert_eq!(harness.unit_weapons(unit).unwrap().mounts[0].total_shots, 1);
    }

    #[test]
    fn alternate_side_flip_toggles_mounts() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let mut weapon =
            weapon_with_bullet(&harness, 10.0, ShootPatternSpec::plain(1, 0.0, 0.0), "fuse");
        weapon.mirror = true;
        weapon.alternate = true;
        let (ux, uy) = CombatHarness::tile_center(4, 8);
        let unit = harness.spawn_test_unit(ux, uy, 1, vec![weapon]);
        assert_eq!(harness.unit_weapons(unit).unwrap().mounts.len(), 2);
        harness.set_unit_aim(unit, 0, CombatHarness::tile_center(12, 8));
        harness.set_unit_aim(unit, 1, CombatHarness::tile_center(12, 8));
        harness.set_unit_shoot(unit, 0, true);
        harness.set_unit_shoot(unit, 1, true);
        for _ in 0..14 {
            harness.update_weapons_only();
        }
        // Alternating mirrored mounts trade shots: both fire over the cycle.
        let mounts = &harness.unit_weapons(unit).unwrap().mounts;
        assert!(mounts[0].total_shots >= 1, "first mount fired");
        assert!(mounts[1].total_shots >= 1, "second mount alternated");
        assert!(mounts[0].side, "side flipped after the half-reload");
    }

    #[test]
    fn interpolate_life_scale_clamps_to_aim() {
        // `lifeScale` clamps the lifetime fraction by aim distance / range.
        let mut harness = CombatHarness::new(64, 16, 7);
        let mut weapon = weapon_with_bullet(
            &harness,
            1.0,
            ShootPatternSpec::plain(1, 0.0, 0.0),
            "fuse_scale",
        );
        // Aim at half the resolved range so the lifetime fraction is ~0.5.
        weapon.bullet.range = 60.0;
        let (ux, uy) = CombatHarness::tile_center(4, 8);
        let unit = harness.spawn_test_unit(ux, uy, 1, vec![weapon]);
        harness.set_unit_aim(unit, 0, (ux + 30.0, uy));
        harness.set_unit_shoot(unit, 0, true);
        harness.update_weapons_only();
        let bullet = *harness.bullets.last().expect("bullet spawned");
        let lifetime = harness
            .build
            .world
            .get::<Bullet>(bullet)
            .map(|b| b.lifetime)
            .unwrap_or(0.0);
        // def lifetime 100 * clamp(30/60) = 50.
        assert!((lifetime - 50.0).abs() < 1.0, "lifetime={lifetime}");
    }
}
