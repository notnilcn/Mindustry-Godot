// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bullet entity, lifecycle, motion, collision and hit effects
//! (`core/src/mindustry/entities/comp/BulletComp.java` +
//! `core/src/mindustry/entities/bullet/BulletType.java`).
//!
//! Plan 10 owns this component (it replaces the plan-05 placeholder marker use).
//! Bullets are pooled/`serialize = false` upstream and never appear in saves
//! (plan 10 §2.3), so the component is plain ECS state; ordering is provided by
//! the harness's insertion-ordered entity list and [`crate::ecs::EntitySeq`].

pub mod behavior;
pub mod kinds;
pub mod raycast;
pub mod spawn;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::{BulletId, ContentRegistry};
use crate::determinism::{RngStream, SimRng};
use crate::entities::comp::{Health, Pos, TeamComp, Vel};
use crate::world::WorldGrid;

pub use behavior::{BulletBehavior, behavior_for};
pub use spawn::{BulletSpawn, create};

/// Bullet flag: `keepAlive` was requested (lifetime extension).
pub const KEEP_ALIVE: u16 = 1 << 0;
/// Bullet flag: first tick after spawn (`justSpawned`); motion is skipped once.
pub const JUST_SPAWNED: u16 = 1 << 1;
/// Bullet flag: absorbed by a shield (`absorbed`).
pub const ABSORBED: u16 = 1 << 2;
/// Bullet flag: the bullet has hit and should be removed.
pub const HIT: u16 = 1 << 3;
/// Bullet flag: locally owned (`owner` present; view/interp skip).
pub const OWNER_LOCAL: u16 = 1 << 4;

/// Transient typed payload (`BulletComp.data`; plan 10 §2.4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BulletData {
    /// No payload.
    #[default]
    None,
    /// Interceptor target bullet.
    Bullet(Entity),
    /// Mass-driver payload carrier (plan 08 `MassDriverPayload`).
    MassDriver(Entity),
    /// Logic/custom slot (`shootp` etc.).
    Custom(u32),
}

/// Bullet entity state (`BulletComp` behavior half).
#[derive(Debug, Clone, Component)]
pub struct Bullet {
    /// Content id of the [`crate::content::BulletDef`].
    pub def: BulletId,
    /// Instance damage (already multiplied by the spawn `damageMultiplier`).
    pub damage: f32,
    /// Multiplier vs buildings (`BulletType.buildingDamageMultiplier`).
    pub building_damage_multiplier: f32,
    /// Typed payload.
    pub data: BulletData,
    /// Effect parameter (`Bullet.fdata`).
    pub fdata: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Collision size (world units).
    pub hit_size: f32,
    /// Previous position (start of this tick's segment).
    pub last: (f32, f32),
    /// Aim point (`-1` = unset).
    pub aim: (f32, f32),
    /// Spawn origin.
    pub origin: (f32, f32),
    /// Aimed tile (plan 11 hook).
    pub aim_tile: Option<(i16, i16)>,
    /// Ticks lived.
    pub time: f32,
    /// Ticks to live.
    pub lifetime: f32,
    /// Pierced entities in collision order.
    pub collided: SmallVec<[Entity; 6]>,
    /// Sticky-attached target (`None` = free-flying).
    pub sticky: Option<Entity>,
    /// Sticky x offset relative to the target.
    pub sticky_x: f32,
    /// Sticky y offset relative to the target.
    pub sticky_y: f32,
    /// Flag bitfield ([`KEEP_ALIVE`] etc.).
    pub flags: u16,
    /// Frag groups already created (`Bullet.frags`).
    pub frags: i32,
}

impl Bullet {
    /// Whether `flag` is set.
    #[inline]
    pub fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    /// Sets `flag`.
    #[inline]
    pub fn set(&mut self, flag: u16) {
        self.flags |= flag;
    }

    /// Clears `flag`.
    #[inline]
    pub fn clear(&mut self, flag: u16) {
        self.flags &= !flag;
    }

    /// Whether the bullet should be removed (`hit` or absorbed).
    #[inline]
    pub fn finished(&self) -> bool {
        self.has(HIT) || self.has(ABSORBED)
    }
}

/// Borrow bundle for bullet systems (`BulletWorld`/`CombatFx`/spawn plumbing).
pub struct CombatCtx<'a> {
    /// Live ECS world.
    pub world: &'a mut World,
    /// Content registry.
    pub content: &'a ContentRegistry,
    /// Tile grid.
    pub grid: &'a WorldGrid,
    /// Deterministic combat RNG.
    pub rng: &'a mut SimRng,
    /// FX sink seam (plan 17).
    pub fx: &'a dyn crate::combat::view::FxSink,
    /// Monotonic bullet entity sequence.
    pub seq: &'a mut u64,
    /// Newly spawned bullets (appended by the harness after the pass).
    pub spawned: &'a mut Vec<Entity>,
}

impl CombatCtx<'_> {
    /// Spawns a child/frag/interval bullet and records it for the harness.
    pub fn spawn(&mut self, spawn: &BulletSpawn) -> Option<Entity> {
        let seq = *self.seq;
        *self.seq = seq.wrapping_add(1);
        let entity = spawn::create(self.world, self.content, self.rng, seq, spawn)?;
        self.spawned.push(entity);
        Some(entity)
    }

    /// Bullet state.
    pub fn bullet(&self, entity: Entity) -> Option<&Bullet> {
        self.world.get::<Bullet>(entity)
    }

    /// Bullet position.
    pub fn pos(&self, entity: Entity) -> Option<(f32, f32)> {
        self.world.get::<Pos>(entity).map(|p| (p.x, p.y))
    }

    /// Bullet team.
    pub fn team(&self, entity: Entity) -> u8 {
        self.world
            .get::<TeamComp>(entity)
            .map(|t| t.team)
            .unwrap_or(0)
    }
}

/// Creates the ECS bundle for a bullet (base components without group wiring).
#[allow(clippy::too_many_arguments)]
pub fn bullet_bundle(
    seq: u64,
    x: f32,
    y: f32,
    vel: (f32, f32),
    team: u8,
    bullet: Bullet,
) -> (crate::ecs::EntitySeq, Pos, Vel, TeamComp, Bullet) {
    (
        crate::ecs::EntitySeq(seq),
        Pos { x, y },
        Vel { x: vel.0, y: vel.1 },
        TeamComp { team },
        bullet,
    )
}

/// Advances one bullet (`BulletComp.update` + `BulletType.update`).
///
/// Returns `true` when the bullet is still alive after the update.
pub fn update_bullet(ctx: &mut CombatCtx<'_>, entity: Entity) -> bool {
    let Some(def_id) = ctx.bullet(entity).map(|b| b.def) else {
        return false;
    };
    let Some(def) = ctx.content.bullet(def_id) else {
        return false;
    };
    let instant_disappear = def.instant_disappear;
    let drag = def.drag;
    let accel = def.accel;
    let homing_power = def.homing_power;
    let homing_range = def.homing_range;
    let weave_scale = def.weave_scale;
    let trail = def.trail_length > 0;
    let interval_bullet = def.interval_bullet;
    let bullet_interval = def.bullet_interval;
    let interval_bullets = def.interval_bullets;

    let (px, py, just_spawned, rotation, sticky) = {
        let Some(bullet) = ctx.bullet(entity) else {
            return false;
        };
        let Some(pos) = ctx.pos(entity) else {
            return false;
        };
        (
            pos.0,
            pos.1,
            bullet.has(JUST_SPAWNED),
            bullet.rotation,
            bullet.sticky,
        )
    };

    if !instant_disappear && let Some(mut bullet) = ctx.world.get_mut::<Bullet>(entity) {
        bullet.time += 1.0;
    }
    if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(entity) {
        bullet.last = (px, py);
        bullet.clear(JUST_SPAWNED);
    }

    // Move + drag (skipped on the first tick for `justSpawned`).
    if !just_spawned && !instant_disappear {
        let (vx, vy) = ctx
            .world
            .get::<Vel>(entity)
            .map(|v| (v.x, v.y))
            .unwrap_or((0.0, 0.0));
        if let Some(mut pos) = ctx.world.get_mut::<Pos>(entity) {
            pos.x += vx;
            pos.y += vy;
        }
        if drag != 0.0
            && let Some(mut vel) = ctx.world.get_mut::<Vel>(entity)
        {
            let s = (1.0 - drag).max(0.0);
            vel.x *= s;
            vel.y *= s;
        }
    }

    // Homing toward `aim`.
    if homing_power > 0.0 {
        let (aim_x, aim_y) = ctx.bullet(entity).map(|b| b.aim).unwrap_or((-1.0, -1.0));
        if aim_x >= 0.0 && aim_y >= 0.0 {
            let (cx, cy) = ctx.pos(entity).unwrap_or((0.0, 0.0));
            let (vx, vy) = ctx
                .world
                .get::<Vel>(entity)
                .map(|v| (v.x, v.y))
                .unwrap_or((0.0, 0.0));
            let (dx, dy) = (aim_x - cx, aim_y - cy);
            let dst = (dx * dx + dy * dy).sqrt();
            let speed = (vx * vx + vy * vy).sqrt().max(0.001);
            if dst <= homing_range && dst > 0.0001 {
                let (nx, ny) = (dx / dst, dy / dst);
                let turn = homing_power.min(speed);
                let nvx = vx + (nx - vx / speed) * turn;
                let nvy = vy + (ny - vy / speed) * turn;
                let nlen = (nvx * nvx + nvy * nvy).sqrt();
                if nlen > 0.0
                    && let Some(mut vel) = ctx.world.get_mut::<Vel>(entity)
                {
                    vel.x = nvx / nlen * speed;
                    vel.y = nvy / nlen * speed;
                }
            }
        }
    }

    // ACCEL: `vel.setLength(vel.len() + accel)`.
    if accel != 0.0 {
        let (vx, vy) = ctx
            .world
            .get::<Vel>(entity)
            .map(|v| (v.x, v.y))
            .unwrap_or((0.0, 0.0));
        let len = (vx * vx + vy * vy).sqrt();
        if len > 0.0
            && let Some(mut vel) = ctx.world.get_mut::<Vel>(entity)
        {
            let scale = (len + accel) / len;
            vel.x = vx * scale;
            vel.y = vy * scale;
        }
    }

    // Weave (deterministic lateral oscillation).
    if weave_scale > 0.0 {
        let time = ctx.bullet(entity).map(|b| b.time).unwrap_or(0.0);
        let angle = rotation.to_radians();
        let offset = (time * 0.1).sin() * def.weave_mag;
        if let Some(mut vel) = ctx.world.get_mut::<Vel>(entity) {
            vel.x += -angle.sin() * offset * weave_scale;
            vel.y += angle.cos() * offset * weave_scale;
        }
    }

    // Sticky: reposition relative to the target, or drop when it disappears.
    if let Some(target) = sticky {
        if ctx.world.get_entity(target).is_err() {
            if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(entity) {
                bullet.sticky = None;
                bullet.set(HIT);
            }
            return false;
        }
        let (tx, ty) = ctx.pos(target).unwrap_or((px, py));
        if let Some(bullet) = ctx.bullet(entity) {
            let (ox, oy) = (bullet.sticky_x, bullet.sticky_y);
            if let Some(mut pos) = ctx.world.get_mut::<Pos>(entity) {
                pos.x = tx + ox;
                pos.y = ty + oy;
            }
        }
    }

    // Interval bullets.
    if let Some(child) = interval_bullet
        && bullet_interval > 0.0
    {
        let time = ctx.bullet(entity).map(|b| b.time).unwrap_or(0.0);
        if (time % bullet_interval).abs() < 0.5 {
            let (x, y) = ctx.pos(entity).unwrap_or((px, py));
            let team = ctx.team(entity);
            for _ in 0..interval_bullets.max(1) {
                let spread = def.interval_random_spread;
                let angle =
                    rotation + def.interval_angle + ctx.rng.range(RngStream::Sim, -spread, spread);
                let child_spawn = BulletSpawn {
                    def: child,
                    x,
                    y,
                    angle,
                    team,
                    ..BulletSpawn::default()
                };
                let _ = ctx.spawn(&child_spawn);
            }
        }
    }

    if trail {
        let (x, y) = ctx.pos(entity).unwrap_or((px, py));
        ctx.fx.trail(
            x,
            y,
            rotation,
            def.trail_color,
            def.trail_width,
            def.trail_length as f32,
        );
    }

    // `removeAfterPierce` cap check (`BulletComp.update`).
    let pierce_cap = def.pierce_cap;
    let remove_after_pierce = def.remove_after_pierce;
    if remove_after_pierce
        && pierce_cap != -1
        && ctx
            .bullet(entity)
            .is_some_and(|b| b.collided.len() >= pierce_cap as usize)
    {
        if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(entity) {
            bullet.set(HIT);
        }
        return false;
    }

    // Lifetime.
    let (time, lifetime) = ctx
        .bullet(entity)
        .map(|b| (b.time, b.lifetime))
        .unwrap_or((0.0, 0.0));
    if time >= lifetime {
        if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(entity) {
            bullet.set(HIT);
        }
        return false;
    }
    true
}

/// Result of one bullet collision pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionOutcome {
    /// The bullet hit and should be removed.
    Hit,
    /// The bullet pierced the target and stays alive.
    Pierced,
    /// The bullet hit terrain with no damageable target.
    Terrain,
    /// The bullet attached to a sticky target.
    Stuck,
    /// Nothing hit.
    None,
}

/// Applies a direct hit to a building (`Health`), returning damage dealt.
pub fn hit_building(ctx: &mut CombatCtx<'_>, bullet: Entity, target: Entity) -> f32 {
    let Some(state) = ctx.bullet(bullet) else {
        return 0.0;
    };
    let (pierce_armor, armor_mult, block_armor_mult) = ctx
        .content
        .bullet(state.def)
        .map(|def| {
            (
                def.pierce_armor,
                def.armor_multiplier,
                def.block_armor_multiplier,
            )
        })
        .unwrap_or((false, 1.0, 1.0));
    let damage = state.damage * state.building_damage_multiplier;
    let Some(building) = ctx.world.get::<crate::entities::comp::Building>(target) else {
        // Units: armor is supplied by plan 11; zero here.
        let applied = super::damage::armor::apply_armor_opt(damage, 0.0, pierce_armor);
        super::damage::area::apply_health(ctx.world, target, applied);
        return applied;
    };
    let armor = ctx
        .content
        .block(building.block)
        .map(|def| def.armor * armor_mult * block_armor_mult)
        .unwrap_or(0.0);
    let applied = super::damage::armor::apply_armor_opt(damage, armor, pierce_armor);
    super::damage::area::apply_health(ctx.world, target, applied);
    applied
}

/// `BulletType.hit` effect pipeline (frags/splash; puddles/incend/units in 10/11).
fn hit(ctx: &mut CombatCtx<'_>, bullet: Entity, x: f32, y: f32, create_frags: bool) {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    if def.hit_effect != crate::content::registries::fx_meta::EffectRef::default() {
        ctx.fx
            .effect(&def.hit_effect, x, y, state.rotation, def.hit_color);
    }
    if def.hit_shake > 0.0 {
        ctx.fx.shake(def.hit_shake);
    }
    if create_frags && def.frag_on_hit {
        create_frags_of(ctx, bullet, x, y);
    }
    create_splash_damage(ctx, bullet, x, y);
    // createPuddles/createIncend/createUnits/suppression/lightning: M2/M3/11.
}

/// `BulletType.despawned`.
fn despawned(ctx: &mut CombatCtx<'_>, bullet: Entity) {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    let (x, y) = ctx.pos(bullet).unwrap_or((state.last.0, state.last.1));
    if def.despawn_hit {
        hit(ctx, bullet, x, y, false);
    }
    if def.despawn_effect != crate::content::registries::fx_meta::EffectRef::default() {
        ctx.fx
            .effect(&def.despawn_effect, x, y, state.rotation, def.hit_color);
    }
    if def.despawn_shake > 0.0 {
        ctx.fx.shake(def.despawn_shake);
    }
}

/// `BulletType.removed`.
fn removed(ctx: &mut CombatCtx<'_>, bullet: Entity) {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    if state.frags == 0 && def.frag_on_despawn && def.frag_bullet.is_some() {
        let (x, y) = ctx.pos(bullet).unwrap_or((state.last.0, state.last.1));
        create_frags_of(ctx, bullet, x, y);
    }
}

/// `BulletType.createFrags` (deterministic seeded spread).
fn create_frags_of(ctx: &mut CombatCtx<'_>, bullet: Entity, x: f32, y: f32) {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    let Some(frag) = def.frag_bullet else {
        return;
    };
    let (frag_bullets, offset_min, offset_max, random_spread) = (
        def.frag_bullets,
        def.frag_offset_min,
        def.frag_offset_max,
        def.frag_random_spread,
    );
    let (angle_off, spread, vmin, vmax, lmin, lmax) = (
        def.frag_angle,
        def.frag_spread,
        def.frag_velocity_min,
        def.frag_velocity_max,
        def.frag_life_min,
        def.frag_life_max,
    );
    if def.pierce_frag_cap >= 0 && state.frags >= def.pierce_frag_cap {
        return;
    }
    for i in 0..frag_bullets.max(0) {
        let len = ctx.rng.range(RngStream::Sim, offset_min, offset_max);
        let a = state.rotation
            + ctx
                .rng
                .range(RngStream::Sim, -random_spread / 2.0, random_spread / 2.0)
            + angle_off
            + spread * i as f32
            - (frag_bullets - 1) as f32 * spread / 2.0;
        let radians = a.to_radians();
        let fx = x + radians.cos() * len;
        let fy = y + radians.sin() * len;
        let velocity = ctx.rng.range(RngStream::Sim, vmin, vmax);
        let life_scl = ctx.rng.range(RngStream::Sim, lmin, lmax);
        let spawn = BulletSpawn {
            def: frag,
            x: fx,
            y: fy,
            angle: a,
            team: ctx.team(bullet),
            velocity_scl: velocity,
            lifetime_scl: life_scl,
            ..BulletSpawn::default()
        };
        let _ = ctx.spawn(&spawn);
    }
    if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
        b.frags += 1;
    }
}

/// `BulletType.createSplashDamage`: damages enemies of the bullet's team.
fn create_splash_damage(ctx: &mut CombatCtx<'_>, bullet: Entity, x: f32, y: f32) {
    let Some(state) = ctx.bullet(bullet) else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    if def.splash_damage <= 0.0 || def.splash_damage_radius <= 0.0 {
        return;
    }
    let team = ctx.team(bullet);
    let opts = super::damage::area::DamageOptions {
        pierce_armor: def.pierce_armor,
        armor_multiplier: def.armor_multiplier,
        scaled: def.scaled_splash_damage,
    };
    let _ = super::damage::area::damage_area(
        ctx.world,
        ctx.content,
        Some(team),
        x,
        y,
        def.splash_damage_radius,
        def.splash_damage,
        opts,
    );
}

/// Resolves bullet-vs-world collision for one bullet (`BulletComp.tileRaycast`).
pub fn collide_bullet(ctx: &mut CombatCtx<'_>, bullet: Entity) -> CollisionOutcome {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return CollisionOutcome::None;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return CollisionOutcome::None;
    };
    let collide_team = def.collides_team;
    let collide_terrain = def.collide_terrain;
    let collide_floor = def.collide_floor;
    let pierce_building = def.pierce_building;
    let pierce_damage_factor = def.pierce_damage_factor;
    let remove_after_pierce = def.remove_after_pierce;
    let pierce_cap = def.pierce_cap;
    let sticky = state.sticky;
    let bullet_team = ctx.team(bullet);
    let tile_size = crate::config::TILESIZE as f32;

    let to_tile = |v: f32| (v / tile_size).floor() as i32;
    let start_x = to_tile(state.last.0);
    let start_y = to_tile(state.last.1);
    let (ex, ey) = ctx.pos(bullet).unwrap_or(state.last);
    let end_x = to_tile(ex);
    let end_y = to_tile(ey);

    let mut outcome = CollisionOutcome::None;
    crate::world::raycast::raycast_each(start_x, start_y, end_x, end_y, |tx, ty| {
        if !ctx.grid.tiles.in_bounds(tx, ty) {
            if collide_terrain {
                if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                    b.set(HIT);
                }
                outcome = CollisionOutcome::Terrain;
                return true;
            }
            return false;
        }
        let tile = ctx.grid.tiles.get(tx, ty);
        // Floor/terrain stop (`collideFloor`/`collideTerrain`).
        if tile.build.is_none()
            && tile.block != crate::content::BlockId::AIR
            && (collide_terrain || collide_floor)
        {
            if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                b.set(HIT);
            }
            outcome = CollisionOutcome::Terrain;
            return true;
        }
        if let Some(target) = tile.build {
            // Sticky bullets attach instead of hitting.
            if sticky == Some(target) {
                let offset = (
                    ctx.pos(bullet).unwrap_or((0.0, 0.0)).0 - tx as f32 * tile_size,
                    ctx.pos(bullet).unwrap_or((0.0, 0.0)).1 - ty as f32 * tile_size,
                );
                if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                    b.sticky_x = offset.0;
                    b.sticky_y = offset.1;
                }
                outcome = CollisionOutcome::Stuck;
                return true;
            }
            let same_team = ctx.world.get::<TeamComp>(target).map(|t| t.team) == Some(bullet_team);
            if same_team && !collide_team {
                return false;
            }
            let already = ctx
                .bullet(bullet)
                .is_some_and(|b| b.collided.contains(&target));
            if pierce_building && already {
                return false;
            }
            let initial_health = ctx
                .world
                .get::<Health>(target)
                .map(|h| h.health)
                .unwrap_or(0.0);
            hit_building(ctx, bullet, target);
            // Direct enemy hit runs the effect pipeline (splash/frags).
            if !same_team {
                let (x, y) = ctx.pos(bullet).unwrap_or(state.last);
                hit(ctx, bullet, x, y, true);
            }
            if pierce_building {
                let sub = if pierce_damage_factor == 0.0 {
                    0.0
                } else {
                    (initial_health * pierce_damage_factor).max(0.0)
                };
                let mut remove = false;
                if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                    if !b.collided.contains(&target) {
                        b.collided.push(target);
                    }
                    b.damage -= if sub.is_nan() {
                        b.damage
                    } else {
                        sub.min(b.damage)
                    };
                    if remove_after_pierce && b.damage <= 0.0 {
                        remove = true;
                    }
                    if remove_after_pierce
                        && pierce_cap != -1
                        && b.collided.len() >= pierce_cap as usize
                    {
                        remove = true;
                    }
                }
                if remove {
                    if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                        b.set(HIT);
                    }
                    outcome = CollisionOutcome::Hit;
                } else {
                    outcome = CollisionOutcome::Pierced;
                }
                // Upstream stops raycasting after a pierceBuilding hit.
                true
            } else {
                if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                    b.set(HIT);
                }
                outcome = CollisionOutcome::Hit;
                true
            }
        } else {
            false
        }
    });

    if outcome == CollisionOutcome::None {
        // Direct overlap fallback (fast bullets that land on the same tile).
        if let Some(target) = ctx
            .grid
            .tiles
            .getn(end_x, end_y)
            .and_then(|tile| tile.build)
            && target != bullet
        {
            let same_team = ctx.world.get::<TeamComp>(target).map(|t| t.team) == Some(bullet_team);
            if !same_team || collide_team {
                hit_building(ctx, bullet, target);
                let (x, y) = ctx.pos(bullet).unwrap_or(state.last);
                hit(ctx, bullet, x, y, true);
                if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                    b.set(HIT);
                }
                outcome = CollisionOutcome::Hit;
            }
        }
    }
    outcome
}

/// Runs `despawned`/`removed` for a finishing bullet, then despawns it.
pub fn finish_bullet(ctx: &mut CombatCtx<'_>, entity: Entity) {
    if let Some(state) = ctx.bullet(entity).cloned()
        && !state.has(HIT)
    {
        despawned(ctx, entity);
    }
    removed(ctx, entity);
    let _ = ctx.world.despawn(entity);
}

/// Despawns a bullet entity without running hooks (harness cleanup).
pub fn remove_bullet(world: &mut World, entity: Entity) {
    let _ = world.despawn(entity);
}

/// Reads an entity's current `Pos` if present.
pub fn bullet_pos(world: &World, entity: Entity) -> Option<(f32, f32)> {
    world.get::<Pos>(entity).map(|p| (p.x, p.y))
}

/// Whether a bullet entity is alive and not finished.
pub fn bullet_alive(world: &World, entity: Entity) -> bool {
    world
        .get::<Bullet>(entity)
        .is_some_and(|bullet| !bullet.finished())
}

/// Health accessor used by tests/dumps.
pub fn health_of(world: &World, entity: Entity) -> f32 {
    world
        .get::<Health>(entity)
        .map(|health| health.health)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn basic_bullet_travel_hits_building() {
        let mut harness = CombatHarness::new(32, 32, 7);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.build.place(10, 10, wall, 0, true));
        let before = harness.building_health_at(10, 10);
        let (x, y) = CombatHarness::tile_center(8, 10);
        assert!(harness.spawn_bullet("fuse", x, y, 0.0, 1).is_some());
        for _ in 0..120 {
            harness.tick();
        }
        let after = harness.building_health_at(10, 10);
        assert!(after < before, "wall took damage: {before} -> {after}");
    }

    #[test]
    fn just_spawned_does_not_move_first_tick() {
        let mut harness = CombatHarness::new(16, 16, 1);
        let (x, y) = CombatHarness::tile_center(4, 4);
        let e = harness.spawn_bullet("fuse", x, y, 0.0, 0).expect("spawn");
        let start = bullet_pos(&harness.build.world, e);
        harness.step_bullets_only();
        assert_eq!(start, bullet_pos(&harness.build.world, e));
        harness.step_bullets_only();
        assert_ne!(start, bullet_pos(&harness.build.world, e));
    }

    #[test]
    fn remove_after_lifetime() {
        let mut harness = CombatHarness::new(16, 16, 1);
        let (x, y) = CombatHarness::tile_center(4, 4);
        let e = harness.spawn_bullet("fuse", x, y, 90.0, 0).expect("spawn");
        for _ in 0..600 {
            harness.step_bullets_only();
        }
        assert!(harness.build.world.get_entity(e).is_err());
    }

    #[test]
    fn pierce_damages_each_wall_in_order() {
        let mut harness = CombatHarness::new(48, 16, 11);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        for x in 12..=14 {
            assert!(harness.place(x, 8, wall, 0, true));
        }
        let (x, y) = CombatHarness::tile_center(4, 8);
        let _ = harness.spawn_bullet("rail", x, y, 0.0, 1).expect("spawn");
        for _ in 0..80 {
            harness.tick();
        }
        let damaged = (12..=14)
            .filter(|x| harness.building_health_at(*x, 8) < 320.0)
            .count();
        assert!(damaged >= 2, "damaged {damaged} walls");
    }

    #[test]
    fn remove_after_pierce_cap_order() {
        let mut harness = CombatHarness::new(64, 16, 3);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        for x in 10..=19 {
            assert!(harness.place(x, 8, wall, 0, true));
        }
        let (x, y) = CombatHarness::tile_center(4, 8);
        let e = harness.spawn_bullet("rail", x, y, 0.0, 1).expect("spawn");
        for _ in 0..120 {
            harness.tick();
        }
        // rail has pierceCap 3 + removeAfterPierce: it damages at most 3 walls.
        let hit = (10..=19)
            .filter(|x| harness.building_health_at(*x, 8) < 320.0)
            .count();
        assert!(hit <= 3, "pierce cap exceeded: {hit}");
        assert!(harness.build.world.get_entity(e).is_err());
    }

    #[test]
    fn frag_bullets_spawn_on_hit() {
        let mut harness = CombatHarness::new(32, 32, 5);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, wall, 0, true));
        let (x, y) = CombatHarness::tile_center(8, 10);
        let created_before = harness.bullets_created;
        assert!(harness.spawn_bullet("frag", x, y, 0.0, 1).is_some());
        for _ in 0..12 {
            harness.step_bullets_only();
        }
        assert!(
            harness.bullets_created > created_before + 1,
            "frag children spawned (created={})",
            harness.bullets_created
        );
    }

    #[test]
    fn terrain_bullet_stops_at_wall() {
        let mut harness = CombatHarness::new(32, 16, 9);
        // `stone-wall` is a static (building-less) block.
        let wall = harness
            .content()
            .block_id("stone-wall")
            .expect("stone-wall");
        assert!(harness.place(10, 8, wall, 0, true));
        assert!(
            harness.build_at(10, 8).is_none(),
            "static wall has no building"
        );
        let (x, y) = CombatHarness::tile_center(6, 8);
        let e = harness
            .spawn_bullet("terrain", x, y, 0.0, 1)
            .expect("spawn");
        for _ in 0..40 {
            harness.step_bullets_only();
        }
        assert!(harness.build.world.get_entity(e).is_err(), "bullet stopped");
    }

    #[test]
    fn sticky_bullet_removed_when_target_removed() {
        let mut harness = CombatHarness::new(32, 32, 4);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, wall, 0, true));
        let target = harness.build.build_at(10, 10).expect("target");
        let (x, y) = CombatHarness::tile_center(8, 10);
        let e = harness.spawn_bullet("sticky", x, y, 0.0, 1).expect("spawn");
        harness
            .build
            .world
            .get_mut::<Bullet>(e)
            .expect("bullet")
            .sticky = Some(target);
        for _ in 0..5 {
            harness.step_bullets_only();
        }
        assert!(harness.build.world.get_entity(e).is_ok(), "attached");
        // Remove the target; the sticky bullet drops and is removed.
        assert!(harness.build.break_block(10, 10, true));
        for _ in 0..3 {
            harness.step_bullets_only();
        }
        assert!(harness.build.world.get_entity(e).is_err());
    }
}
