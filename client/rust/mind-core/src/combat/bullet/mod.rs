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
pub mod line_hit;
pub mod raycast;
pub mod spawn;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::audio::{AudioSinkRes, sim as audio_sim};
use crate::content::{BlockKind, BulletId, ContentRegistry};
use crate::determinism::{RngStream, SimRng};
use crate::ecs::BuildingComp;
use crate::entities::comp::{Health, Pos, TeamComp, Vel};
use crate::fx::TrailChannelId;
use crate::world::WorldGrid;

pub use behavior::{BulletBehavior, behavior_for};
pub use line_hit::{
    collide_laser, collide_line_bullet, find_laser_length, find_length, find_pierce_length,
    linecast_enemy,
};
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
/// Bullet flag: lifetime expiry requested removal without a hit
/// (`TimedComp.update` → `remove()`; upstream `hit == false`). Kept distinct
/// from [`HIT`] so `BulletType.despawned` (and its `despawnSound`) runs for
/// lifetime removals, matching `BulletComp.remove`.
pub const EXPIRED: u16 = 1 << 5;

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
    /// Owning entity (`Bullet.owner`; healed by `BulletType.lifesteal`).
    pub owner: Option<Entity>,
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
    /// Deterministic per-tick mover (`BulletComp.mover`; e.g. `ShootHelix`).
    pub mover: Option<ShotMover>,
}

/// Per-tick bullet mover (`BulletComp.mover` lambda replacements).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShotMover {
    /// `ShootHelix` mover: `b.moveRelative(0, Mathf.sin(b.time + offset, scl, mag * sign))`.
    Helix {
        /// Time scale.
        scl: f32,
        /// Displacement magnitude.
        mag: f32,
        /// Phase offset.
        offset: f32,
        /// `+1`/`-1` twin direction (`Mathf.signs`).
        sign: f32,
    },
}

impl ShotMover {
    /// Applies the mover to a bullet position for one tick (`BulletComp.moveRelative`).
    pub fn apply(&self, pos: &mut Pos, rotation: f32, time: f32) {
        match *self {
            ShotMover::Helix {
                scl,
                mag,
                offset,
                sign,
            } => {
                let rel = ((time + offset) * scl).sin() * mag * sign;
                let rad = rotation.to_radians();
                // `Angles.trnsx(rot, 0, rel)` / `Angles.trnsy(rot, 0, rel)`.
                pos.x += rel * rad.sin();
                pos.y += -rel * rad.cos();
            }
        }
    }
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

    /// Whether the bullet should be removed (`hit`, absorbed or expired).
    #[inline]
    pub fn finished(&self) -> bool {
        self.has(HIT) || self.has(ABSORBED) || self.has(EXPIRED)
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
    /// Plan-18 audio sink (sim call sites emit `Sound.at` here).
    pub audio: &'a AudioSinkRes,
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
        apply_kind_init(self, entity);
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

/// Runs the kind-specific `init(Bullet)` half (`BulletType.init` overrides).
///
/// Vanilla laser bullets deal their line damage immediately at spawn and are
/// removed (`LaserBulletType.init`); point/multi kinds teleport/spawn children.
pub fn apply_kind_init(ctx: &mut CombatCtx<'_>, entity: Entity) {
    let Some(kind) = ctx
        .bullet(entity)
        .and_then(|state| ctx.content.bullet(state.def))
        .map(|def| def.kind)
    else {
        return;
    };
    behavior_for(kind).init(ctx, entity);
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

    // Per-tick mover (`BulletComp.mover`; e.g. `ShootHelix`).
    if let Some(mover) = ctx.bullet(entity).and_then(|b| b.mover) {
        let (time, rotation) = ctx
            .bullet(entity)
            .map(|b| (b.time, b.rotation))
            .unwrap_or((0.0, 0.0));
        if let Some(mut pos) = ctx.world.get_mut::<Pos>(entity) {
            mover.apply(&mut pos, rotation, time);
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

    // Kind-specific `BulletType.update(b)` (after `super.update`).
    {
        let kind = ctx
            .bullet(entity)
            .and_then(|b| ctx.content.bullet(b.def))
            .map(|def| def.kind);
        if let Some(kind) = kind {
            behavior_for(kind).update(ctx, entity);
            if !bullet_alive(ctx.world, entity) {
                return false;
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
        // Plan 17 §3.9: key the trail by the bullet's stable entity sequence so
        // `FxEvent::Trail` can resolve the per-channel `TrailRegistry` tint.
        let channel = ctx
            .world
            .get::<crate::ecs::EntitySeq>(entity)
            .map(|seq| TrailChannelId(seq.0 as u32))
            .unwrap_or(TrailChannelId(0));
        ctx.fx.trail_channel(
            channel,
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

    // Lifetime (`TimedComp.update`): expiry is a despawn, never a hit, so
    // `despawned`/`despawnSound` still runs in `finish_bullet`.
    let (time, lifetime) = ctx
        .bullet(entity)
        .map(|b| (b.time, b.lifetime))
        .unwrap_or((0.0, 0.0));
    if time >= lifetime {
        if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(entity) {
            bullet.set(EXPIRED);
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
    let tile = building.tile;
    let team = world_team(ctx.world, target);
    let applied = super::damage::armor::apply_armor_opt(damage, armor, pierce_armor);
    super::damage::area::apply_health(ctx.world, target, applied);
    // `BuildingComp.damage(Bullet, ..)` -> `BuildDamageEvent` (G01 T15).
    crate::world::events::push_build_damage(
        ctx.world,
        crate::world::events::BuildDamageEvent {
            x: tile.x(),
            y: tile.y(),
            amount: applied,
            team,
        },
    );
    applied
}

/// The entity's team id (0 without a `TeamComp`).
fn world_team(world: &bevy_ecs::world::World, entity: Entity) -> u8 {
    world
        .get::<crate::entities::comp::TeamComp>(entity)
        .map(|team| team.team)
        .unwrap_or(0)
}

/// `Wall.WallBuild.collision` (`Wall.java:164-208`): surge-wall lightning and
/// phase-wall bullet deflection.
///
/// Called right after [`hit_building`] (Java `Wall.collision` runs
/// `super.collision(bullet)` = `BuildingComp.collision`, which applies the
/// damage). Returns Java's "remove" flag: `true` normally, `false` when a phase
/// wall deflected the bullet so it keeps flying. `hit = 1f` (the white flash)
/// and the lightning/deflect sounds are view/audio (H03/H07).
pub fn wall_collision(ctx: &mut CombatCtx<'_>, bullet: Entity, target: Entity) -> bool {
    let Some(block) = ctx
        .world
        .get::<crate::ecs::BuildingComp>(target)
        .map(|building| building.block)
    else {
        return true;
    };
    // `Wall.*` knobs; every other block uses `Block.collision` (returns true).
    let Some((lightning_chance, lightning_damage, lightning_length, chance_deflect, flash_hit)) =
        ctx.world
            .get_resource::<crate::world::block::BlockTable>()
            .and_then(|table| table.instance(block))
            .and_then(|inst| match &inst.kind_data {
                crate::world::BlockKindData::Wall(def) => Some((
                    def.lightning_chance,
                    def.lightning_damage,
                    def.lightning_length,
                    def.chance_deflect,
                    def.flash_hit,
                )),
                _ => None,
            })
    else {
        return true;
    };

    // `Wall.WallBuild.collision` (`Wall.java:164-166`): `hit = 1f` on every
    // collision with a `flashHit` wall (phase walls), deflected or not; the
    // renderer owns the decay (I02 T4).
    if flash_hit
        && let Some(tile) = ctx
            .world
            .get::<crate::entities::comp::Building>(target)
            .map(|build| build.tile)
    {
        crate::world::events::push_wall_flash(ctx.world, tile.x(), tile.y());
    }

    // `Wall.java:169-175`: `Lightning.create(team, lightningColor, damage, x, y,
    // bullet.rotation() + 180f, length)`; `lightningColor = Pal.surge`.
    if lightning_chance > 0.0 && ctx.rng.chance(RngStream::Sim, lightning_chance as f64) {
        let (x, y) = ctx.pos(target).unwrap_or((0.0, 0.0));
        let rotation = ctx.bullet(bullet).map(|b| b.rotation).unwrap_or(0.0);
        let team = ctx.team(target);
        let _ = crate::combat::lightning::create(
            ctx,
            team,
            crate::content::registries::pal::SURGE,
            lightning_damage,
            x,
            y,
            rotation + 180.0,
            lightning_length,
        );
    }

    // `Wall.java:177-207`: deflected bullets reverse direction.
    if chance_deflect > 0.0 {
        let Some((vx, vy)) = ctx.world.get::<Vel>(bullet).map(|v| (v.x, v.y)) else {
            return true;
        };
        let speed = (vx * vx + vy * vy).sqrt();
        let reflectable = ctx
            .bullet(bullet)
            .and_then(|b| ctx.content.bullet(b.def))
            .is_some_and(|def| def.reflectable);
        // Slow and non-reflectable bullets are never deflected.
        if speed <= 0.1 || !reflectable {
            return true;
        }
        let damage = ctx.bullet(bullet).map(|b| b.damage).unwrap_or(0.0);
        // Reflection chance falls with bullet damage (`chanceDeflect / damage`).
        if !ctx
            .rng
            .chance(RngStream::Sim, (chance_deflect / damage) as f64)
        {
            return true;
        }
        // `bullet.trns(-vel)` then flip the dominant penetration axis.
        let (bx, by) = ctx.pos(bullet).unwrap_or((0.0, 0.0));
        let (nx, ny) = (bx - vx, by - vy);
        if let Some(mut pos) = ctx.world.get_mut::<Pos>(bullet) {
            pos.x = nx;
            pos.y = ny;
        }
        let (wx, wy) = ctx.pos(target).unwrap_or((nx, ny));
        let pen_x = (wx - nx).abs();
        let pen_y = (wy - ny).abs();
        if let Some(mut vel) = ctx.world.get_mut::<Vel>(bullet) {
            if pen_x > pen_y {
                vel.x = -vel.x;
            } else {
                vel.y = -vel.y;
            }
        }
        let team = ctx.team(target);
        if let Some(mut state) = ctx.world.get_mut::<Bullet>(bullet) {
            state.owner = Some(target);
            state.time += 1.0;
        }
        if let Some(mut comp) = ctx.world.get_mut::<TeamComp>(bullet) {
            comp.team = team;
        }
        return false;
    }
    true
}

/// `BulletType.hitTile` (`BulletType.java:465-486`): create a fire on enemy
/// hits, heal friendly non-construct buildings, or run the `hit` pipeline for
/// direct enemy hits. Pierce bookkeeping stays in the callers
/// (`collide_bullet`/`collide_line_bullet`).
pub(crate) fn hit_tile(
    ctx: &mut CombatCtx<'_>,
    bullet: Entity,
    target: Entity,
    x: f32,
    y: f32,
    _initial_health: f32,
    direct: bool,
) {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    let bullet_team = ctx.team(bullet);
    let target_team = ctx
        .world
        .get::<TeamComp>(target)
        .map_or(bullet_team, |t| t.team);
    let same_team = target_team == bullet_team;
    let (make_fire, heals, heal_percent, heal_amount) = (
        def.make_fire,
        def.heals(),
        def.heal_percent,
        def.heal_amount,
    );
    if make_fire && !same_team {
        // `Fires.create(build.tile)`.
        let tile = ctx
            .world
            .get::<crate::entities::comp::Building>(target)
            .map(|build| build.tile);
        if let Some(tile) = tile {
            let _ = crate::combat::fires::create(ctx.world, tile.x(), tile.y(), ctx.rng);
        }
    }
    let construct = ctx
        .world
        .get::<crate::entities::comp::Building>(target)
        .and_then(|build| ctx.content.block(build.block))
        .is_some_and(|block| block.kind == BlockKind::ConstructBlock);
    if heals && same_team && !construct {
        let max_health = ctx
            .world
            .get::<Health>(target)
            .map_or(0.0, |health| health.max_health);
        super::damage::area::heal_health(
            ctx.world,
            target,
            heal_percent / 100.0 * max_health + heal_amount,
        );
        // `BulletType.hitTile` (`BulletType.java:470-475`): `healSound.at(build,
        // 1 + Mathf.range(0.1f), healSoundVolume)` at the building centre.
        if let Some((hx, hy)) = crate::audio::sim::building_centre_of(ctx.world, target) {
            let pitch = 1.0 + (ctx.rng.next_float(RngStream::Fx) * 2.0 - 1.0) * 0.1;
            audio_sim::emit_bullet_heal(ctx.audio, hx, hy, pitch);
        }
        behavior_hit(ctx, bullet, x, y, true);
    } else if !same_team && direct {
        behavior_hit(ctx, bullet, x, y, true);
    }
}

/// `BulletType.hit` effect pipeline (frags/splash; kind-specific overrides via
/// [`BulletBehavior::hit`]).
pub(crate) fn hit_bullet(
    ctx: &mut CombatCtx<'_>,
    bullet: Entity,
    x: f32,
    y: f32,
    create_frags: bool,
) {
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
    // `BulletType.hit`: `hitSound.at(x, y, hitSoundPitch +
    // Mathf.range(hitSoundPitchRange), hitSoundVolume)` (`BulletType.java:564`).
    // The Fx draw happens only for audible bullets: Java draws from the global
    // unseeded `Mathf`, so skipping the draw for `none`/`unset` keeps the
    // view-only `Fx` stream minimal (deviation).
    if !def.hit_sound.is_none_or_unset() {
        let pitch = def.hit_sound_pitch
            + (ctx.rng.next_float(RngStream::Fx) * 2.0 - 1.0) * def.hit_sound_pitch_range;
        audio_sim::emit_bullet_hit(ctx.audio, def.hit_sound, x, y, def.hit_sound_volume, pitch);
    }
    if create_frags && def.frag_on_hit {
        create_frags_of(ctx, bullet, x, y);
    }
    // `BulletType.hit` order: frags, puddles, incend, units, suppression,
    // splash (Java `BulletType.java:562-580`).
    create_puddles(ctx, bullet, x, y);
    create_incend(ctx, bullet, x, y);
    create_units(ctx, bullet, x, y);
    // `if(suppressionRange > 0) Damage.applySuppression(b.team, b.x, b.y,
    // suppressionRange, suppressionDuration, ...)` (`BulletType.java:572-575`):
    // centred on the bullet, not the hit point (L06 T3).
    if def.suppression_range > 0.0 {
        let (range, duration) = (def.suppression_range, def.suppression_duration);
        let team = ctx.team(bullet);
        let (bx, by) = ctx.pos(bullet).unwrap_or((x, y));
        crate::world::blocks::defense::projectors::apply_suppression(
            ctx.world, team, bx, by, range, duration,
        );
    }
    create_splash_damage(ctx, bullet, x, y);
}

/// Calls the kind-specific `BulletType.hit` override (or the base pipeline).
pub(crate) fn behavior_hit(
    ctx: &mut CombatCtx<'_>,
    bullet: Entity,
    x: f32,
    y: f32,
    create_frags: bool,
) {
    let Some(kind) = ctx
        .bullet(bullet)
        .and_then(|state| ctx.content.bullet(state.def))
        .map(|def| def.kind)
    else {
        return;
    };
    behavior_for(kind).hit(ctx, bullet, x, y, create_frags);
}

/// `BulletType.despawned`.
pub(crate) fn despawn_bullet(ctx: &mut CombatCtx<'_>, bullet: Entity) {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    let despawn_hit = def.despawn_hit;
    let despawn_effect = def.despawn_effect.clone();
    let hit_color = def.hit_color;
    let despawn_shake = def.despawn_shake;
    let despawn_sound = def.despawn_sound;
    let (x, y) = ctx.pos(bullet).unwrap_or((state.last.0, state.last.1));
    if despawn_hit {
        behavior_hit(ctx, bullet, x, y, false);
    } else {
        // `BulletType.despawned`: `createUnits(b, b.x, b.y)` when not a hit.
        create_units(ctx, bullet, x, y);
    }
    if despawn_effect != crate::content::registries::fx_meta::EffectRef::default() {
        ctx.fx
            .effect(&despawn_effect, x, y, state.rotation, hit_color);
    }
    if despawn_shake > 0.0 {
        ctx.fx.shake(despawn_shake);
    }
    // `BulletType.despawned` (`BulletType.java:658`): `despawnSound.at(b,
    // 1f + Mathf.range(hitSoundPitchRange))`.
    if !despawn_sound.is_none_or_unset() {
        let pitch =
            1.0 + (ctx.rng.next_float(RngStream::Fx) * 2.0 - 1.0) * def.hit_sound_pitch_range;
        audio_sim::emit_bullet_despawn(ctx.audio, despawn_sound, x, y, 1.0, pitch);
    }
}

/// `BulletType.removed`.
pub(crate) fn remove_bullet_hook(ctx: &mut CombatCtx<'_>, bullet: Entity) {
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
        source_is_bullet: true,
        // `Damage.damage(.., collidesAir, collidesGround, ..)`
        // (`BulletType.java:608`).
        air: def.collides_air,
        ground: def.collides_ground,
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

/// `BulletType.createPuddles` (`BulletType.java:597-604`): deposit `puddleAmount`
/// puddles of `puddleLiquid` scattered within `puddleRange`.
fn create_puddles(ctx: &mut CombatCtx<'_>, bullet: Entity, x: f32, y: f32) {
    let Some(state) = ctx.bullet(bullet) else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    let (puddles, range, liquid, amount) = (
        def.puddles,
        def.puddle_range,
        def.puddle_liquid,
        def.puddle_amount,
    );
    if puddles <= 0 {
        return;
    }
    for _ in 0..puddles {
        let px = x + ctx.rng.range(RngStream::Sim, -range, range);
        let py = y + ctx.rng.range(RngStream::Sim, -range, range);
        let _ = super::puddles::deposit(
            ctx.world,
            &ctx.grid.tiles,
            ctx.content,
            WorldGrid::to_tile(px) as i16,
            WorldGrid::to_tile(py) as i16,
            liquid,
            amount,
            ctx.rng,
        );
    }
}

/// `BulletType.createIncend` (`BulletType.java:591-595`) via
/// `Damage.createIncend` (`Damage.java:135-144`): chance-gated fires scattered
/// within `incendSpread`.
fn create_incend(ctx: &mut CombatCtx<'_>, bullet: Entity, x: f32, y: f32) {
    let Some(state) = ctx.bullet(bullet) else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    let (chance, amount, spread) = (def.incend_chance, def.incend_amount, def.incend_spread);
    if chance <= 0.0 || !ctx.rng.chance(RngStream::Sim, chance as f64) {
        return;
    }
    for _ in 0..amount.max(0) {
        let px = x + ctx.rng.range(RngStream::Sim, -spread, spread);
        let py = y + ctx.rng.range(RngStream::Sim, -spread, spread);
        let (tx, ty) = (WorldGrid::to_tile(px), WorldGrid::to_tile(py));
        if ctx.grid.tiles.in_bounds(tx, ty) {
            let _ = super::fires::create(ctx.world, tx as i16, ty as i16, ctx.rng);
        }
    }
}

/// `BulletType.createUnits` (`BulletType.java:638-647`): spawn
/// `despawnUnitCount` units of `despawnUnit` scattered within
/// `despawnUnitRadius`, facing outward or along the bullet rotation.
fn create_units(ctx: &mut CombatCtx<'_>, bullet: Entity, x: f32, y: f32) {
    let Some((def_id, rotation)) = ctx.bullet(bullet).map(|state| (state.def, state.rotation))
    else {
        return;
    };
    let Some(def) = ctx.content.bullet(def_id) else {
        return;
    };
    let Some(unit_id) = def.despawn_unit else {
        return;
    };
    if def.despawn_unit_count <= 0
        || !ctx
            .rng
            .chance(RngStream::Sim, def.despawn_unit_chance as f64)
    {
        return;
    }
    let Some(udef) = ctx.content.unit(unit_id) else {
        return;
    };
    let (count, radius, face_outwards, team) = (
        def.despawn_unit_count,
        def.despawn_unit_radius,
        def.face_outwards,
        ctx.team(bullet),
    );
    for _ in 0..count {
        // `Tmp.v1.rnd(Mathf.random(despawnUnitRadius))`.
        let len = ctx.rng.range(RngStream::Sim, 0.0, radius);
        let angle = ctx.rng.range(RngStream::Sim, 0.0, 360.0);
        let rad = angle.to_radians();
        let (sx, sy) = (x + rad.cos() * len, y + rad.sin() * len);
        let spawn_rotation = if face_outwards { angle } else { rotation };
        let seq = *ctx.seq;
        *ctx.seq = seq.wrapping_add(1);
        let entity = crate::entities::comp::unit::lifecycle::spawn_unit_def(
            ctx.world,
            seq,
            udef,
            team,
            sx,
            sy,
            spawn_rotation,
        );
        ctx.world.entity_mut(entity).insert(spawn::SpawnedByBullet);
    }
}

/// Resolves bullet-vs-world collision for one bullet (`BulletComp.tileRaycast`).
pub fn collide_bullet(ctx: &mut CombatCtx<'_>, bullet: Entity) -> CollisionOutcome {
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return CollisionOutcome::None;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return CollisionOutcome::None;
    };
    let collides = def.collides;
    let collide_team = def.collides_team;
    let collide_terrain = def.collide_terrain;
    let collide_floor = def.collide_floor;
    let pierce_building = def.pierce_building;
    let pierce_damage_factor = def.pierce_damage_factor;
    let remove_after_pierce = def.remove_after_pierce;
    let pierce_cap = def.pierce_cap;
    let def_sticky = def.sticky;
    let hit_under = def.hit_under;
    let piercing = def.pierce;
    let max_damage_fraction = def.max_damage_fraction;
    let building_damage_multiplier = def.building_damage_multiplier;
    let _ = (max_damage_fraction, building_damage_multiplier);
    let sticky = state.sticky;
    // The bullet's team is re-read per tile below: a `Wall` deflection steals
    // the bullet for the wall's team mid-tick.
    let tile_size = crate::config::TILESIZE as f32;
    // `!collides` bullets (fire, liquid, lasers, empty) never hit buildings.
    if !collides && !collide_terrain && !collide_floor {
        return CollisionOutcome::None;
    }

    // `BulletComp.tileRaycast(World.toTile(lastX), World.toTile(lastY), tileX(), tileY())`
    // (BulletComp.java:187): `World.toTile` = `Math.round(c / 8)` (World.java:174).
    let to_tile = |v: f32| crate::world::WorldGrid::to_tile(v);
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
            // A `Wall` deflection (`wall_collision`) steals the bullet for the
            // wall's team mid-tick, so the friendly checks below must use the
            // bullet's live team (`BulletComp.tileRaycast` re-reads `team`).
            let bullet_team = ctx.team(bullet);
            // `BulletComp.checkUnderBuild` (BulletComp.java:212-225): a bullet
            // passes over `underBullets` buildings unless it was aimed at that
            // building, the def allows hitting under builds (`hitUnder`), the
            // building is friendly, a piercing bullet has overshot its aim
            // tile, or the bullet had no aim at all.
            if !passes_under_build(
                ctx,
                &state,
                bullet,
                target,
                hit_under,
                piercing,
                bullet_team,
            ) {
                // Skip this building; the raycast continues.
                return false;
            }
            // `build.collide(self())` (`BulletComp.java:251`): `LogicBuild`
            // returns false for a privileged non-destructible processor, so
            // the bullet passes through without damage (`LogicBlock.java:447`).
            if crate::world::build::logic_processor_invulnerable(ctx.world, target) {
                return false;
            }
            // `BulletComp.collision`: `BulletType.sticky` bullets attach on
            // first contact (`stickTo`); an already-attached bullet does not
            // re-hit its target. A pre-assigned *aim* target is not sticky
            // (`BulletType.create` seeds `aimTile` only).
            if sticky == Some(target) || (def_sticky && sticky.is_none()) {
                // Java `stickTo`: the offset is relative to the target's
                // position (`x - other.x`).
                let (bx, by) = ctx.pos(bullet).unwrap_or((0.0, 0.0));
                let (ox, oy) = ctx
                    .pos(target)
                    .map(|(tx, ty)| (bx - tx, by - ty))
                    .unwrap_or((bx - tx as f32 * tile_size, by - ty as f32 * tile_size));
                if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                    b.sticky = Some(target);
                    b.sticky_x = ox;
                    b.sticky_y = oy;
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
            let (hit_x, hit_y) = ctx.pos(bullet).unwrap_or(state.last);
            // `BulletComp.collision` (`BulletComp.java:269-293`): enemy hits run
            // `build.collision(self())`, which damages the building
            // (`BuildingComp.collision`) and may return "do not remove" (phase
            // wall deflection, `Wall.java:164-208`).
            let remove = if same_team {
                false
            } else {
                hit_building(ctx, bullet, target);
                wall_collision(ctx, bullet, target)
            };
            // `BulletType.hitTile` (`BulletType.java:465-486`): enemy direct
            // hits run the effect pipeline, friendly heals apply, pierce stays
            // below. It runs even when the bullet is deflected.
            hit_tile(ctx, bullet, target, hit_x, hit_y, initial_health, true);
            if !remove && !collide_team {
                // Deflected: Java skips the removal block and keeps raycasting.
                return false;
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
        // The raycast may have deflected the bullet onto the wall's team, so
        // the friendly checks use its live team like the closure above.
        let bullet_team = ctx.team(bullet);
        if let Some(target) = ctx
            .grid
            .tiles
            .getn(end_x, end_y)
            .and_then(|tile| tile.build)
            && target != bullet
            && passes_under_build(
                ctx,
                &state,
                bullet,
                target,
                hit_under,
                piercing,
                bullet_team,
            )
            // Same `build.collide(self())` gate as the raycast above.
            && !crate::world::build::logic_processor_invulnerable(ctx.world, target)
        {
            let same_team = ctx.world.get::<TeamComp>(target).map(|t| t.team) == Some(bullet_team);
            if !same_team || collide_team {
                let initial_health = ctx
                    .world
                    .get::<Health>(target)
                    .map(|h| h.health)
                    .unwrap_or(0.0);
                let (x, y) = ctx.pos(bullet).unwrap_or(state.last);
                let remove = if same_team {
                    false
                } else {
                    hit_building(ctx, bullet, target);
                    wall_collision(ctx, bullet, target)
                };
                hit_tile(ctx, bullet, target, x, y, initial_health, true);
                if remove || collide_team {
                    if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                        b.set(HIT);
                    }
                    outcome = CollisionOutcome::Hit;
                }
            }
        }
    }
    outcome
}

/// `BulletComp.checkUnderBuild` (BulletComp.java:212-225): whether a bullet is
/// allowed to hit `target` when it is an `underBullets` building.
///
/// A bullet passes over such a building unless it was aimed at it
/// (`aimTile.build == build`), its def sets `hitUnder`, the building is
/// friendly, a piercing bullet has already overshot its aim tile, or the bullet
/// had no aim at all.
pub(crate) fn passes_under_build(
    ctx: &CombatCtx<'_>,
    state: &Bullet,
    bullet: Entity,
    target: Entity,
    hit_under: bool,
    piercing: bool,
    bullet_team: u8,
) -> bool {
    let under = ctx
        .world
        .get::<BuildingComp>(target)
        .and_then(|comp| ctx.content.block(comp.block))
        .is_some_and(|def| def.under_bullets);
    if !under {
        return true;
    }
    let aimed = state.aim_tile.is_some_and(|(ax, ay)| {
        ctx.grid
            .tiles
            .getn(ax as i32, ay as i32)
            .and_then(|tile| tile.build)
            == Some(target)
    });
    if aimed || hit_under {
        return true;
    }
    if ctx.world.get::<TeamComp>(target).map(|t| t.team) == Some(bullet_team) {
        return true;
    }
    // `type.pierce && aimTile != null && Mathf.dst(x, y, originX, originY) >
    // aimTile.dst(originX, originY) + 2f` — the tile distance uses the tile
    // corner (`Tile.worldx/worldy`).
    let tile_size = crate::config::TILESIZE as f32;
    let overshot = piercing
        && state.aim_tile.is_some_and(|(ax, ay)| {
            let (ox, oy) = state.origin;
            let (tx, ty) = (ax as f32 * tile_size, ay as f32 * tile_size);
            let aim_dst = ((tx - ox).powi(2) + (ty - oy).powi(2)).sqrt();
            let (px, py) = ctx.pos(bullet).unwrap_or(state.last);
            ((px - ox).powi(2) + (py - oy).powi(2)).sqrt() > aim_dst + 2.0
        });
    overshot || state.aim == (-1.0, -1.0)
}

/// Whether a unit counts as flying for bullet/target gates
/// (`UnitComp.isFlying`: `elevation >= 0.09`; flying unit types hover at
/// elevation 1, mirrored here by [`PhysicsComp::flying`]).
///
/// [`PhysicsComp::flying`]: crate::entities::comp::unit::comp::PhysicsComp
pub fn unit_is_flying(world: &World, unit: Entity) -> bool {
    use crate::entities::comp::unit::comp::{PhysicsComp, UnitCore};
    world.get::<PhysicsComp>(unit).is_some_and(|p| p.flying)
        || world
            .get::<UnitCore>(unit)
            .is_some_and(|c| c.elevation >= 0.09)
}

/// `EntityCollisions.collide` swept AABB test (Arc port): `r1` moves by
/// `(vx1, vy1)`, `r2` by `(vx2, vy2)`; returns the entry point of `r1`'s
/// center on contact.
#[allow(clippy::too_many_arguments)]
fn swept_collide(
    x1: f32,
    y1: f32,
    w1: f32,
    h1: f32,
    vx1: f32,
    vy1: f32,
    x2: f32,
    y2: f32,
    w2: f32,
    h2: f32,
    vx2: f32,
    vy2: f32,
) -> Option<(f32, f32)> {
    let (px, py) = (vx1, vy1);
    let vx1 = vx1 - vx2;
    let vy1 = vy1 - vy2;
    let (x_inv_entry, x_inv_exit) = if vx1 > 0.0 {
        (x2 - (x1 + w1), (x2 + w2) - x1)
    } else {
        ((x2 + w2) - x1, x2 - (x1 + w1))
    };
    let (y_inv_entry, y_inv_exit) = if vy1 > 0.0 {
        (y2 - (y1 + h1), (y2 + h2) - y1)
    } else {
        ((y2 + h2) - y1, y2 - (y1 + h1))
    };
    let x_entry = x_inv_entry / vx1;
    let x_exit = x_inv_exit / vx1;
    let y_entry = y_inv_entry / vy1;
    let y_exit = y_inv_exit / vy1;
    let entry = x_entry.max(y_entry);
    let exit = x_exit.min(y_exit);
    // Java float semantics: NaN comparisons are false, so `NaN` never rejects.
    if entry > exit || x_exit < 0.0 || y_exit < 0.0 || x_entry > 1.0 || y_entry > 1.0 {
        None
    } else {
        Some((x1 + w1 / 2.0 + px * entry, y1 + h1 / 2.0 + py * entry))
    }
}

/// `Groups.bullet.collide()` unit half (`EntityCollisions.updateCollision` +
/// `checkCollide`): tests one bullet's swept hitbox against the live enemy
/// units and runs `BulletComp.collision` / `BulletType.hitEntity` on contact.
///
/// `units` must be in a stable (spawn/seq) order; the pass stops at the first
/// non-piercing hit (`if(!solid.isAdded()) return`). Returns whether any unit
/// was hit (PAR-0141).
pub fn collide_units(ctx: &mut CombatCtx<'_>, bullet: Entity, units: &[Entity]) -> bool {
    use crate::entities::comp::unit::comp::{HitboxComp, UnitCore, UnitTypeComp};
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return false;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return false;
    };
    if !def.collides || state.sticky.is_some() || state.finished() {
        return false;
    }
    let (collides_air, collides_ground, pierce, def_sticky) = (
        def.collides_air,
        def.collides_ground,
        def.pierce,
        def.sticky,
    );
    let team = ctx.team(bullet);
    let Some((bx, by)) = ctx.pos(bullet) else {
        return false;
    };
    let (lx, ly) = state.last;
    let size = state.hit_size;
    // `r1`: the bullet hitbox at its last position; `r2` merged with the
    // current one bounds the broad phase (`updateCollision`).
    let r1 = (lx - size / 2.0, ly - size / 2.0);
    let (vax, vay) = (bx - lx, by - ly);
    let (min_x, min_y) = (r1.0.min(bx - size / 2.0), r1.1.min(by - size / 2.0));
    let (max_x, max_y) = (
        (lx + size / 2.0).max(bx + size / 2.0),
        (ly + size / 2.0).max(by + size / 2.0),
    );
    let mut any = false;
    for &unit in units {
        if unit == bullet || ctx.world.get_entity(unit).is_err() {
            continue;
        }
        if ctx
            .world
            .get::<UnitCore>(unit)
            .is_some_and(|core| core.dead)
            || ctx.world.get::<Health>(unit).is_none_or(|h| h.dead)
        {
            continue;
        }
        // `Bullet.collides`: enemy team, air/ground gate, pierce memory.
        if ctx.world.get::<TeamComp>(unit).map(|t| t.team) == Some(team) {
            continue;
        }
        let flying = unit_is_flying(ctx.world, unit);
        if !((flying && collides_air) || (!flying && collides_ground)) {
            continue;
        }
        if pierce
            && ctx
                .bullet(bullet)
                .is_some_and(|b| b.collided.contains(&unit))
        {
            continue;
        }
        // `Unit.collides`: `type.hittable(unit)`.
        let hittable = ctx
            .world
            .get::<UnitTypeComp>(unit)
            .and_then(|comp| ctx.content.unit(comp.type_id))
            .is_none_or(|def| def.hittable);
        if !hittable {
            continue;
        }
        let Some(upos) = ctx.world.get::<Pos>(unit).copied() else {
            continue;
        };
        let usize_ = ctx
            .world
            .get::<HitboxComp>(unit)
            .map(|h| h.hit_size)
            .unwrap_or(8.0);
        let r2 = (upos.x - usize_ / 2.0, upos.y - usize_ / 2.0);
        // Broad phase: the unit hitbox must overlap the merged bullet sweep.
        if r2.0 > max_x || r2.0 + usize_ < min_x || r2.1 > max_y || r2.1 + usize_ < min_y {
            continue;
        }
        let overlaps = r1.0 < r2.0 + usize_
            && r1.0 + size > r2.0
            && r1.1 < r2.1 + usize_
            && r1.1 + size > r2.1;
        let contact = if overlaps {
            Some((bx, by))
        } else {
            swept_collide(
                r1.0, r1.1, size, size, vax, vay, r2.0, r2.1, usize_, usize_, 0.0, 0.0,
            )
        };
        let Some((cx, cy)) = contact else {
            continue;
        };
        any = true;
        // `BulletComp.collision`.
        if def_sticky {
            if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                b.sticky = Some(unit);
                b.sticky_x = cx - upos.x;
                b.sticky_y = cy - upos.y;
            }
            return true;
        }
        behavior_hit(ctx, bullet, cx, cy, true);
        let health = ctx.world.get::<Health>(unit).map_or(0.0, |h| h.health);
        if pierce {
            if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
                b.collided.push(unit);
            }
        } else if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
            b.set(HIT);
        }
        hit_entity(ctx, bullet, unit, health);
        if ctx.bullet(bullet).is_none_or(Bullet::finished) {
            break;
        }
    }
    any
}

/// `BulletType.hitEntity` for a unit target: capped/armored damage, knockback,
/// status and `handlePierce`.
pub fn hit_entity(ctx: &mut CombatCtx<'_>, bullet: Entity, unit: Entity, health: f32) {
    use crate::entities::comp::unit::ShieldComp;
    use crate::entities::comp::unit::comp::UnitTypeComp;
    let Some(state) = ctx.bullet(bullet).cloned() else {
        return;
    };
    let Some(def) = ctx.content.bullet(state.def) else {
        return;
    };
    let (max_fraction, pierce_armor, armor_mult) = (
        def.max_damage_fraction,
        def.pierce_armor,
        def.armor_multiplier,
    );
    let (knockback, impact, status, status_duration, status_chance) = (
        def.knockback,
        def.impact,
        def.status,
        def.status_duration,
        def.status_chance,
    );
    let (pierce_damage_factor, remove_after_pierce) =
        (def.pierce_damage_factor, def.remove_after_pierce);
    let mut damage = state.damage;
    let mut health = health;
    let shield = ctx
        .world
        .get::<ShieldComp>(unit)
        .map_or(0.0, |s| s.shield.max(0.0));
    if max_fraction > 0.0 {
        let max_health = ctx.world.get::<Health>(unit).map_or(0.0, |h| h.max_health);
        let cap = max_health * max_fraction + shield;
        damage = damage.min(cap);
        health = health.min(cap);
    } else {
        health += shield;
    }
    // `if(lifesteal > 0f && b.owner instanceof Healthc o) o.heal(result *
    // lifesteal)` with `result = max(min(h.health(), damage), 0)` — the
    // pre-damage health (`BulletType.java:502-505`).
    let lifesteal_heal = if def.lifesteal > 0.0 {
        match (state.owner, ctx.world.get::<Health>(unit).map(|h| h.health)) {
            (Some(owner), Some(target_health)) => {
                Some((owner, target_health.min(damage).max(0.0) * def.lifesteal))
            }
            _ => None,
        }
    } else {
        None
    };
    let _ = super::damage::area::damage_entity(
        ctx.world,
        ctx.content,
        unit,
        damage,
        pierce_armor,
        armor_mult,
    );
    if let Some((owner, amount)) = lifesteal_heal {
        super::damage::area::heal_health(ctx.world, owner, amount);
    }
    // Knockback impulse (`unit.impulse(v)`: `vel += v / mass`, `mass = hitSize² π`).
    let type_def = ctx
        .world
        .get::<UnitTypeComp>(unit)
        .and_then(|comp| ctx.content.unit(comp.type_id));
    let knock_mult = type_def.map_or(1.0, |d| d.knockback_multiplier);
    if knock_mult > 0.0 && knockback != 0.0 {
        let (ux, uy) = ctx
            .world
            .get::<Pos>(unit)
            .map_or((0.0, 0.0), |p| (p.x, p.y));
        let (bx, by) = ctx.pos(bullet).unwrap_or(state.last);
        let len = knockback * 80.0 * knock_mult;
        let angle = if impact {
            (state.rotation + if knockback < 0.0 { 180.0 } else { 0.0 }).to_radians()
        } else {
            (uy - by).atan2(ux - bx)
        };
        let (ix, iy) = if impact {
            (angle.cos() * len.abs(), angle.sin() * len.abs())
        } else {
            (angle.cos() * len, angle.sin() * len)
        };
        crate::entities::comp::unit::velocity::impulse(ctx.world, unit, ix, iy);
    }
    if status_chance >= 1.0 || ctx.rng.chance(RngStream::Sim, f64::from(status_chance)) {
        super::damage::status::apply_status(ctx.world, unit, status, status_duration);
    }
    // `handlePierce`.
    let sub = if pierce_damage_factor == 0.0 {
        0.0
    } else {
        (health * pierce_damage_factor).max(0.0)
    };
    if let Some(mut b) = ctx.world.get_mut::<Bullet>(bullet) {
        b.damage -= if sub.is_nan() {
            b.damage
        } else {
            sub.min(b.damage)
        };
        if remove_after_pierce && b.damage <= 0.0 {
            b.set(HIT);
        }
    }
}

/// Runs `despawned`/`removed` for a finishing bullet, then despawns it.
pub fn finish_bullet(ctx: &mut CombatCtx<'_>, entity: Entity) {
    if let Some(state) = ctx.bullet(entity).cloned()
        && !state.has(HIT)
    {
        let kind = ctx
            .bullet(entity)
            .and_then(|s| ctx.content.bullet(s.def))
            .map(|d| d.kind);
        if let Some(kind) = kind {
            behavior_for(kind).despawned(ctx, entity);
        }
    }
    let kind = ctx
        .bullet(entity)
        .and_then(|s| ctx.content.bullet(s.def))
        .map(|d| d.kind);
    if let Some(kind) = kind {
        behavior_for(kind).removed(ctx, entity);
    }
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
        // `World.toTile` rounds: spawn on the tile's `worldx` (row 10).
        let (x, y) = CombatHarness::tile_origin(8, 10);
        assert!(harness.spawn_bullet("fuse", x, y, 0.0, 1).is_some());
        for _ in 0..120 {
            harness.tick();
        }
        let after = harness.building_health_at(10, 10);
        assert!(after < before, "wall took damage: {before} -> {after}");
    }

    /// T1: `BulletComp.tileRaycast` uses `World.toTile` = `Math.round(c / 8)`
    /// (World.java:174). A bullet at y = 79 rounds into row 10, so it hits the
    /// wall on tile row 10; the old `floor` lookup gave row 9 and passed under.
    #[test]
    fn bullet_row_lookup_uses_round() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(11, 10, wall, 0, true));
        let before = harness.building_health_at(11, 10);
        assert_eq!(before, 320.0, "full copper-wall health");
        assert!(
            harness
                .spawn_bullet("fuse", 100.0, 79.0, 180.0, 1)
                .is_some()
        );
        for _ in 0..30 {
            harness.step_bullets_only();
        }
        let after = harness.building_health_at(11, 10);
        assert!(
            after < before,
            "row 10 wall took damage: {before} -> {after}"
        );
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

    /// Plan 18 §7e: a lifetime removal (`TimedComp.update` → `remove()` with
    /// `!hit`) runs `BulletType.despawned` (emitting `despawnSound`) before
    /// `removed`, matching `BulletComp.remove`; a collision runs `hit` instead.
    #[test]
    fn despawn_sound_fires_on_lifetime_not_hit() {
        use crate::audio::SharedAudioLog;
        use crate::content::registries::sound_meta::SoundId;

        // Lifetime expiry with no target: despawnSound, never hitSound.
        let log = SharedAudioLog::new();
        let mut harness = CombatHarness::new(16, 16, 1);
        harness.build.set_audio_sink(log.clone());
        let (x, y) = CombatHarness::tile_center(4, 4);
        let e = harness.spawn_bullet("fuse", x, y, 0.0, 0).expect("spawn");
        for _ in 0..200 {
            harness.step_bullets_only();
        }
        assert!(harness.build.world.get_entity(e).is_err(), "removed");
        let events = log.events();
        let despawn = events
            .iter()
            .filter(|event| event.event.sound() == Some(SoundId::EXPLOSION))
            .count();
        let hit = events
            .iter()
            .filter(|event| event.event.sound() == Some(SoundId::EXPLOSION_ARTILLERY))
            .count();
        assert_eq!(
            despawn, 1,
            "lifetime despawnSound emitted once, before removed"
        );
        assert_eq!(hit, 0, "lifetime removal is not a hit");

        // Collision: hitSound, never despawnSound.
        let log = SharedAudioLog::new();
        let mut harness = CombatHarness::new(32, 32, 2);
        harness.build.set_audio_sink(log.clone());
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.build.place(10, 8, wall, 0, true));
        // `World.toTile` rounds: spawn on the tile's `worldx` (row 8).
        let (x, y) = CombatHarness::tile_origin(4, 8);
        let _ = harness.spawn_bullet("fuse", x, y, 0.0, 1).expect("spawn");
        for _ in 0..40 {
            harness.step_bullets_only();
        }
        let events = log.events();
        assert!(
            events
                .iter()
                .any(|event| event.event.sound() == Some(SoundId::EXPLOSION_ARTILLERY)),
            "collision emits hitSound"
        );
        assert!(
            !events
                .iter()
                .any(|event| event.event.sound() == Some(SoundId::EXPLOSION)),
            "collision must not emit despawnSound"
        );
    }

    /// H07 T4 (`BulletType.java:564`): `hitSound` pitch =
    /// `hitSoundPitch + Mathf.range(hitSoundPitchRange)`; 50 hits with the
    /// default `1 ± 0.1` must stay in range and vary.
    #[test]
    fn bullet_hit_pitch_varies_in_range() {
        use crate::audio::{AudioEvent, SharedAudioLog};
        use crate::content::registries::sound_meta::SoundId;

        let log = SharedAudioLog::new();
        let mut harness = CombatHarness::new(64, 16, 2);
        harness.build.set_audio_sink(log.clone());
        let wall = harness.content().block_id("copper-wall").expect("wall");
        // One fresh wall per hit (a wall survives a single fuse hit).
        for x in 10..60 {
            assert!(harness.place(x, 8, wall, 0, true), "wall at ({x}, 8)");
        }
        for i in 0..50 {
            let (x, y) = CombatHarness::tile_origin(9 + i, 8);
            let _ = harness.spawn_bullet("fuse", x, y, 0.0, 1).expect("spawn");
            for _ in 0..40 {
                harness.step_bullets_only();
            }
        }
        let pitches: Vec<f32> = log
            .events()
            .iter()
            .filter_map(|ticked| match &ticked.event {
                AudioEvent::At { sound, pitch, .. } if *sound == SoundId::EXPLOSION_ARTILLERY => {
                    Some(*pitch)
                }
                _ => None,
            })
            .collect();
        assert!(pitches.len() >= 2, "hit sounds recorded: {pitches:?}");
        for pitch in &pitches {
            assert!((0.9..=1.1).contains(pitch), "pitch {pitch} outside range");
        }
        let first = pitches[0];
        assert!(
            pitches.iter().any(|pitch| (*pitch - first).abs() > 1.0e-6),
            "pitches must vary: {pitches:?}"
        );
    }

    /// H07 T4 (`BulletType.java:470-475`): a healing bullet hitting a damaged
    /// friendly building plays `blockHeal` at the building centre
    /// (`healSoundVolume = 0.9`, `pitch = 1 + Mathf.range(0.1)`).
    #[test]
    fn heal_bullet_plays_block_heal() {
        use crate::audio::{AudioEvent, SharedAudioLog};
        use crate::content::registries::bullets::{BulletDef, BulletKind};
        use crate::content::registries::sound_meta::SoundId;

        let log = SharedAudioLog::new();
        let mut harness = CombatHarness::new(32, 32, 5);
        harness.build.set_audio_sink(log.clone());
        harness.build.rules.default_team = 1;
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, true), "friendly wall");
        let entity = harness.build_at(10, 8).expect("wall entity");
        harness
            .build
            .world
            .get_mut::<Health>(entity)
            .expect("health")
            .health = 100.0;
        let mut def = BulletDef::new(BulletKind::Basic);
        def.speed = 4.0;
        def.lifetime = 60.0;
        def.damage = 1.0;
        def.heal_percent = 5.0;
        def.collides_team = true;
        let bullet = harness
            .build
            .content
            .add_bullet(def)
            .expect("heal bullet def");
        let (x, y) = CombatHarness::tile_origin(9, 8);
        let _ = harness.spawn_def(bullet, x, y, 0.0, 1).expect("spawn");
        for _ in 0..40 {
            harness.step_bullets_only();
        }
        let events = log.events();
        let heals: Vec<&AudioEvent> = events
            .iter()
            .filter_map(|ticked| match &ticked.event {
                AudioEvent::At { sound, .. } if *sound == SoundId::BLOCK_HEAL => {
                    Some(&ticked.event)
                }
                _ => None,
            })
            .collect();
        assert_eq!(heals.len(), 1, "one blockHeal: {events:?}");
        let AudioEvent::At {
            x,
            y,
            pitch,
            volume,
            ..
        } = heals[0]
        else {
            unreachable!()
        };
        // Size-1 wall at (10,8): centre = tile*8, offset 0.
        assert_eq!((*x, *y), (80.0, 64.0), "building centre");
        assert_eq!(*volume, 0.9);
        assert!((0.9..=1.1).contains(pitch), "blockHeal pitch {pitch}");
    }

    #[test]
    fn pierce_damages_each_wall_in_order() {
        let mut harness = CombatHarness::new(48, 16, 11);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        for x in 12..=14 {
            assert!(harness.place(x, 8, wall, 0, true));
        }
        // `World.toTile` rounds, so spawn on the tile corner (`y = 8 * 8`) to
        // keep the beam inside row 8; a tile-centre y (68) rounds up to row 9.
        let (x, _) = CombatHarness::tile_center(4, 8);
        let y = 8.0 * 8.0;
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
        // `World.toTile` rounds: spawn on the tile's `worldx` (row 10).
        let (x, y) = CombatHarness::tile_origin(8, 10);
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
    fn laser_instant_collide() {
        let mut harness = CombatHarness::new(48, 16, 2);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(12, 8, wall, 0, true));
        let before = harness.building_health_at(12, 8);
        // `World.toTile` rounds; spawn on the tile corner to stay in row 8.
        let (x, _) = CombatHarness::tile_center(6, 8);
        let y = 8.0 * 8.0;
        let e = harness.spawn_bullet("laser", x, y, 0.0, 1).expect("spawn");
        assert!(
            harness.building_health_at(12, 8) < before,
            "laser dealt damage on spawn"
        );
        assert!(!bullet_alive(&harness.build.world, e));
    }

    /// G01 T4: `BulletType.hitTile` heals a friendly building by
    /// `healPercent / 100 * maxHealth + healAmount` (nova's bolt shape).
    #[test]
    fn heal_bolt_heals_friendly_wall() {
        use crate::entities::comp::{Health, TeamComp};

        let mut harness = CombatHarness::new(32, 16, 4);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, true));
        let wall_entity = harness.build_at(10, 8).expect("wall entity");
        // Harness buildings default to team 0; make the wall friendly to the
        // team-1 bolt.
        if let Some(mut team) = harness.build.world.get_mut::<TeamComp>(wall_entity) {
            team.team = 1;
        }
        if let Some(mut health) = harness.build.world.get_mut::<Health>(wall_entity) {
            health.health = 100.0;
        }
        // `World.toTile` rounds: spawn on the tile's `worldx` (row 8).
        let (x, y) = CombatHarness::tile_origin(6, 8);
        let _ = harness
            .spawn_bullet("heal_bolt", x, y, 0.0, 1)
            .expect("spawn");
        for _ in 0..30 {
            harness.step_bullets_only();
        }
        assert_eq!(
            harness.building_health_at(10, 8),
            116.0,
            "healed by 5% of the 320 max health"
        );
    }

    /// G01 T4: an enemy wall takes the bolt's raw damage instead.
    #[test]
    fn heal_bolt_damages_enemy_wall() {
        let mut harness = CombatHarness::new(32, 16, 4);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, true));
        // `World.toTile` rounds: spawn on the tile's `worldx` (row 8).
        let (x, y) = CombatHarness::tile_origin(6, 8);
        let _ = harness
            .spawn_bullet("heal_bolt", x, y, 0.0, 1)
            .expect("spawn");
        for _ in 0..30 {
            harness.step_bullets_only();
        }
        assert_eq!(
            harness.building_health_at(10, 8),
            320.0 - 13.0,
            "enemy wall takes the bolt damage"
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

    fn wall(h: &CombatHarness) -> crate::content::BlockId {
        h.content().block_id("copper-wall").expect("wall")
    }

    #[test]
    fn point_bullet_teleports_and_hits() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let wall = wall(&harness);
        // speed 4 * lifetime 20 = 80 px from x=32 -> x=112 (tile 14).
        assert!(harness.place(14, 8, wall, 0, true));
        let before = harness.building_health_at(14, 8);
        // `World.toTile` rounds: spawn on the tile's `worldx` (row 8).
        let (x, y) = CombatHarness::tile_origin(4, 8);
        let e = harness.spawn_bullet("point", x, y, 0.0, 1).expect("spawn");
        assert!(
            harness.building_health_at(14, 8) < before,
            "point bullet hit the endpoint building at spawn"
        );
        assert!(!bullet_alive(&harness.build.world, e));
    }

    #[test]
    fn multi_spawns_children() {
        let mut harness = CombatHarness::new(16, 16, 7);
        let before = harness.bullets_created;
        let (x, y) = CombatHarness::tile_center(4, 4);
        let _ = harness.spawn_bullet("multi", x, y, 0.0, 1).expect("spawn");
        // 1 multi marker + 1 child * repeat 2.
        assert!(
            harness.bullets_created >= before + 3,
            "multi spawned children (created={})",
            harness.bullets_created
        );
    }

    #[test]
    fn emp_area_damages_neighbor_building() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let wall = wall(&harness);
        assert!(harness.place(10, 10, wall, 0, true));
        assert!(harness.place(12, 10, wall, 0, true));
        // `World.toTile` rounds: spawn on the tile's `worldx` (row 10).
        let (x, y) = CombatHarness::tile_origin(8, 10);
        let _ = harness.spawn_bullet("emp", x, y, 0.0, 1).expect("spawn");
        for _ in 0..20 {
            harness.tick();
        }
        assert!(harness.building_health_at(10, 10) < 320.0, "direct hit");
        assert!(
            harness.building_health_at(12, 10) < 320.0,
            "EMP area damage reached the neighbor"
        );
    }

    #[test]
    fn flak_airburst_explodes() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let wall = wall(&harness);
        assert!(harness.place(10, 10, wall, 0, true));
        // `World.toTile` rounds: spawn on the tile's `worldx` (row 10).
        let (x, y) = CombatHarness::tile_origin(8, 10);
        let _ = harness.spawn_bullet("flak", x, y, 0.0, 1).expect("spawn");
        for _ in 0..40 {
            harness.tick();
        }
        assert!(harness.building_health_at(10, 10) < 320.0);
    }

    #[test]
    fn sap_instant_line_hit() {
        let mut harness = CombatHarness::new(32, 16, 3);
        let wall = wall(&harness);
        assert!(harness.place(10, 8, wall, 0, true));
        let before = harness.building_health_at(10, 8);
        // `World.toTile` rounds; spawn on the tile corner to stay in row 8.
        let (x, _) = CombatHarness::tile_center(4, 8);
        let y = 8.0 * 8.0;
        let e = harness.spawn_bullet("sap", x, y, 0.0, 1).expect("spawn");
        assert!(harness.building_health_at(10, 8) < before, "sap damaged");
        assert!(!bullet_alive(&harness.build.world, e));
    }

    #[test]
    fn shrapnel_instant_line_damages_all() {
        let mut harness = CombatHarness::new(32, 16, 3);
        let wall = wall(&harness);
        assert!(harness.place(8, 8, wall, 0, true));
        assert!(harness.place(10, 8, wall, 0, true));
        // `World.toTile` rounds; spawn on the tile corner to stay in row 8.
        let (x, _) = CombatHarness::tile_center(4, 8);
        let y = 8.0 * 8.0;
        let _ = harness
            .spawn_bullet("shrapnel", x, y, 0.0, 1)
            .expect("spawn");
        assert!(harness.building_health_at(8, 8) < 320.0);
        assert!(harness.building_health_at(10, 8) < 320.0);
    }

    #[test]
    fn interceptor_removes_target_bullet() {
        let mut harness = CombatHarness::new(16, 16, 7);
        let (x, y) = CombatHarness::tile_center(4, 4);
        let interceptor = harness
            .spawn_bullet("interceptor", x, y, 0.0, 1)
            .expect("spawn");
        let target = harness.spawn_bullet("fuse", x, y, 0.0, 1).expect("spawn");
        // Freeze both so the overlap test is deterministic.
        for e in [interceptor, target] {
            if let Some(mut vel) = harness.build.world.get_mut::<Vel>(e) {
                vel.x = 0.0;
                vel.y = 0.0;
            }
        }
        harness
            .build
            .world
            .get_mut::<Bullet>(interceptor)
            .expect("bullet")
            .data = BulletData::Bullet(target);
        harness.step_bullets_only();
        // 40 - 15 = 25 remaining damage.
        assert_eq!(
            harness.build.world.get::<Bullet>(target).map(|b| b.damage),
            Some(25.0)
        );
    }

    #[test]
    fn continuous_line_damages_each_interval() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let wall = wall(&harness);
        assert!(harness.place(10, 8, wall, 0, true));
        // `World.toTile` rounds; spawn on the tile corner to stay in row 8.
        let (x, _) = CombatHarness::tile_center(4, 8);
        let y = 8.0 * 8.0;
        let _ = harness
            .spawn_bullet("continuous", x, y, 0.0, 1)
            .expect("spawn");
        for _ in 0..6 {
            harness.step_bullets_only();
        }
        assert!(harness.building_health_at(10, 8) < 320.0);
    }

    #[test]
    fn empty_bullet_ignores_buildings() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let wall = wall(&harness);
        assert!(harness.place(10, 8, wall, 0, true));
        let before = harness.building_health_at(10, 8);
        let (x, y) = CombatHarness::tile_center(8, 8);
        let _ = harness.spawn_bullet("empty", x, y, 0.0, 1).expect("spawn");
        for _ in 0..30 {
            harness.step_bullets_only();
        }
        assert_eq!(harness.building_health_at(10, 8), before);
    }

    /// I03 T6: `LogicBuild.collide(Bullet)`/`LogicBuild.damage(float)`
    /// (`LogicBlock.java:437-451`): a privileged, non-destructible processor
    /// ignores direct damage and lets bullets pass through untouched.
    #[test]
    fn privileged_processor_immune() {
        let mut harness = CombatHarness::new(32, 16, 7);
        let processor = harness
            .content()
            .block_id("world-processor")
            .expect("processor");
        assert!(harness.place(10, 8, processor, 0, true));
        let target = harness.build.build_at(10, 8).expect("building");
        assert!(
            crate::world::build::logic_processor_invulnerable(&harness.build.world, target),
            "world-processor is privileged and not destructible"
        );
        let before = harness.building_health_at(10, 8);
        assert!(before > 0.0, "placed processor has health");
        crate::combat::damage::area::apply_health(&mut harness.build.world, target, 50.0);
        assert_eq!(
            harness.building_health_at(10, 8),
            before,
            "direct damage is ignored"
        );
        // A fuse bullet fired along the row crosses the processor tile.
        let (x, y) = CombatHarness::tile_origin(6, 8);
        let bullet = harness.spawn_bullet("fuse", x, y, 0.0, 1).expect("spawn");
        let mut crossed = false;
        for _ in 0..120 {
            harness.step_bullets_only();
            if !bullet_alive(&harness.build.world, bullet) {
                break;
            }
            let (bx, _) = bullet_pos(&harness.build.world, bullet).unwrap_or((0.0, 0.0));
            if bx > 96.0 {
                crossed = true;
                break;
            }
        }
        assert!(crossed, "the bullet flew past the processor tile");
        assert!(
            bullet_alive(&harness.build.world, bullet),
            "the bullet did not collide"
        );
        assert_eq!(
            harness.building_health_at(10, 8),
            before,
            "bullet passed through without damage"
        );
    }
}
