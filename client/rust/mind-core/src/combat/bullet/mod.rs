// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bullet entity, lifecycle, motion and collision
//! (`core/src/mindustry/entities/comp/BulletComp.java`).
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
use crate::entities::comp::{Health, Pos, TeamComp, Vel};

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
    /// Sticky-attached target (plan 10 §3.3).
    pub sticky: Option<Entity>,
    /// Flag bitfield ([`KEEP_ALIVE`] etc.).
    pub flags: u16,
    /// Frag count already created.
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

/// Advances one bullet (`BulletComp.update` motion half).
///
/// Returns `true` when the bullet is still alive after the update.
pub fn update_bullet(
    world: &mut World,
    content: &ContentRegistry,
    entity: Entity,
    fx: &dyn crate::combat::view::FxSink,
) -> bool {
    let Some(def_id) = world.get::<Bullet>(entity).map(|b| b.def) else {
        return false;
    };
    let Some(def) = content.bullet(def_id) else {
        return false;
    };
    let instant_disappear = def.instant_disappear;
    let drag = def.drag;
    let accel = def.accel;
    let homing_power = def.homing_power;
    let homing_range = def.homing_range;
    let weave_scale = def.weave_scale;
    let trail = def.trail_length > 0;

    // Snapshot the previous position and the spawn flag.
    let (px, py, just_spawned, data, rotation) = {
        let Some(bullet) = world.get::<Bullet>(entity) else {
            return false;
        };
        let Some(pos) = world.get::<Pos>(entity) else {
            return false;
        };
        (
            pos.x,
            pos.y,
            bullet.has(JUST_SPAWNED),
            bullet.data,
            bullet.rotation,
        )
    };

    if !instant_disappear && let Some(mut bullet) = world.get_mut::<Bullet>(entity) {
        bullet.time += 1.0;
    }
    if let Some(mut bullet) = world.get_mut::<Bullet>(entity) {
        bullet.last = (px, py);
        bullet.clear(JUST_SPAWNED);
    }

    // Move + drag (skipped on the first tick for `justSpawned`, matching
    // `BulletComp.update`; drag is applied only when the bullet moves).
    if !just_spawned && !instant_disappear {
        let (vx, vy) = world
            .get::<Vel>(entity)
            .map(|v| (v.x, v.y))
            .unwrap_or((0.0, 0.0));
        if let Some(mut pos) = world.get_mut::<Pos>(entity) {
            pos.x += vx;
            pos.y += vy;
        }
        if drag != 0.0
            && let Some(mut vel) = world.get_mut::<Vel>(entity)
        {
            let s = (1.0 - drag).max(0.0);
            vel.x *= s;
            vel.y *= s;
        }
    }

    // Homing toward `aim` (the caller resolves `aim` from the target each tick).
    if homing_power > 0.0 {
        let (aim_x, aim_y) = world
            .get::<Bullet>(entity)
            .map(|b| b.aim)
            .unwrap_or((-1.0, -1.0));
        if aim_x >= 0.0 && aim_y >= 0.0 {
            let (cx, cy) = world
                .get::<Pos>(entity)
                .map(|p| (p.x, p.y))
                .unwrap_or((0.0, 0.0));
            let (vx, vy) = world
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
                    && let Some(mut vel) = world.get_mut::<Vel>(entity)
                {
                    vel.x = nvx / nlen * speed;
                    vel.y = nvy / nlen * speed;
                }
            }
        }
    }

    // ACCEL: `vel.setLength(vel.len() + accel)` (`BulletComp.update`).
    if accel != 0.0 {
        let (vx, vy) = world
            .get::<Vel>(entity)
            .map(|v| (v.x, v.y))
            .unwrap_or((0.0, 0.0));
        let len = (vx * vx + vy * vy).sqrt();
        if len > 0.0
            && let Some(mut vel) = world.get_mut::<Vel>(entity)
        {
            let scale = (len + accel) / len;
            vel.x = vx * scale;
            vel.y = vy * scale;
        }
    }

    // Weave (sinusoidal lateral velocity offset; view-visible only in practice).
    if weave_scale > 0.0 {
        // Deterministic: weave phase is derived from the bullet's lifetime.
        let time = world.get::<Bullet>(entity).map(|b| b.time).unwrap_or(0.0);
        let angle = rotation.to_radians();
        let offset = (time * 0.1).sin() * def.weave_mag;
        if let Some(mut vel) = world.get_mut::<Vel>(entity) {
            vel.x += -angle.sin() * offset * weave_scale;
            vel.y += angle.cos() * offset * weave_scale;
        }
    }

    if trail {
        let (x, y) = world
            .get::<Pos>(entity)
            .map(|p| (p.x, p.y))
            .unwrap_or((0.0, 0.0));
        fx.trail(
            x,
            y,
            rotation,
            def.trail_color,
            def.trail_width,
            def.trail_length as f32,
        );
    }

    // Payload/target data can be cleared by the caller (`PointBullet`, etc.).
    let _ = data;

    // `removeAfterPierce` cap check (`BulletComp.update`).
    let pierce_cap = def.pierce_cap;
    let remove_after_pierce = def.remove_after_pierce;
    if remove_after_pierce && pierce_cap != -1 {
        let over_cap = world
            .get::<Bullet>(entity)
            .map(|b| b.collided.len() >= pierce_cap as usize)
            .unwrap_or(false);
        if over_cap {
            if let Some(mut bullet) = world.get_mut::<Bullet>(entity) {
                bullet.set(HIT);
            }
            return false;
        }
    }

    // Lifetime.
    let (time, lifetime) = world
        .get::<Bullet>(entity)
        .map(|b| (b.time, b.lifetime))
        .unwrap_or((0.0, 0.0));
    if time >= lifetime {
        if let Some(mut bullet) = world.get_mut::<Bullet>(entity) {
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
    /// Nothing hit.
    None,
}

/// Applies a direct hit to a building (`Health`), returning damage dealt.
pub fn hit_building(
    world: &mut World,
    content: &ContentRegistry,
    bullet: Entity,
    target: Entity,
    building_multiplier: bool,
) -> f32 {
    let Some(bullet_state) = world.get::<Bullet>(bullet) else {
        return 0.0;
    };
    let (pierce_armor, armor_mult) = content
        .bullet(bullet_state.def)
        .map(|def| (def.pierce_armor, def.armor_multiplier))
        .unwrap_or((false, 1.0));
    let damage = if building_multiplier {
        bullet_state.damage * bullet_state.building_damage_multiplier
    } else {
        bullet_state.damage
    };
    let Some(building) = world.get::<crate::entities::comp::Building>(target) else {
        // Units: armor is supplied by plan 11; use zero here.
        let applied = super::damage::armor::apply_armor_opt(damage, 0.0, pierce_armor);
        super::damage::area::apply_health(world, target, applied);
        return applied;
    };
    let armor = content
        .block(building.block)
        .map(|def| def.armor * armor_mult)
        .unwrap_or(0.0);
    let applied = super::damage::armor::apply_armor_opt(damage, armor, pierce_armor);
    super::damage::area::apply_health(world, target, applied);
    applied
}

/// Resolves bullet-vs-world collision for one bullet.
///
/// Walks the tile line from `last` (previous position) to the current position,
/// testing buildings (`Building` entity on the tile) and, when the bullet def
/// requests it, terrain/floor walls. Returns the outcome.
pub fn collide_bullet(
    world: &mut World,
    content: &ContentRegistry,
    grid: &crate::world::WorldGrid,
    bullet: Entity,
) -> CollisionOutcome {
    let Some(state) = world.get::<Bullet>(bullet).cloned() else {
        return CollisionOutcome::None;
    };
    let Some(def) = content.bullet(state.def) else {
        return CollisionOutcome::None;
    };
    let collide_team = def.collides_team;
    let collide_terrain = def.collide_terrain;
    let pierce_building = def.pierce_building;
    let pierce_damage_factor = def.pierce_damage_factor;
    let remove_after_pierce = def.remove_after_pierce;
    let pierce_cap = def.pierce_cap;
    let bullet_team = world.get::<TeamComp>(bullet).map(|t| t.team).unwrap_or(0);
    let tile_size = crate::config::TILESIZE as f32;

    // Tile centers sit on `x.5`; `floor` maps a center to its own tile.
    let to_tile = |v: f32| (v / tile_size).floor() as i32;
    let start_x = to_tile(state.last.0);
    let start_y = to_tile(state.last.1);
    let end_x = to_tile(
        world
            .get::<Pos>(bullet)
            .map(|p| p.x)
            .unwrap_or(state.last.0),
    );
    let end_y = to_tile(
        world
            .get::<Pos>(bullet)
            .map(|p| p.y)
            .unwrap_or(state.last.1),
    );

    let mut outcome = CollisionOutcome::None;
    crate::world::raycast::raycast_each(start_x, start_y, end_x, end_y, |tx, ty| {
        if !grid.tiles.in_bounds(tx, ty) {
            // Bullets leave the map; treat as terrain stop for terrain-colliding.
            if collide_terrain {
                outcome = CollisionOutcome::Terrain;
                return true;
            }
            return false;
        }
        let tile = grid.tiles.get(tx, ty);
        if let Some(target) = tile.build {
            let same_team = world.get::<TeamComp>(target).map(|t| t.team) == Some(bullet_team);
            if same_team && !collide_team {
                return false;
            }
            let already = world
                .get::<Bullet>(bullet)
                .is_some_and(|b| b.collided.contains(&target));
            if pierce_building && already {
                return false;
            }
            // Direct damage happens first (`Building.collide(bullet)`), then the
            // pierce bookkeeping (`BulletType.handlePierce`).
            let initial_health = world.get::<Health>(target).map(|h| h.health).unwrap_or(0.0);
            hit_building(world, content, bullet, target, true);
            if pierce_building {
                let sub = if pierce_damage_factor == 0.0 {
                    0.0
                } else {
                    (initial_health * pierce_damage_factor).max(0.0)
                };
                let mut remove = false;
                if let Some(mut b) = world.get_mut::<Bullet>(bullet) {
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
                    if let Some(mut b) = world.get_mut::<Bullet>(bullet) {
                        b.set(HIT);
                    }
                    outcome = CollisionOutcome::Hit;
                } else {
                    outcome = CollisionOutcome::Pierced;
                }
                // Upstream stops raycasting after a pierceBuilding hit.
                true
            } else {
                if let Some(mut b) = world.get_mut::<Bullet>(bullet) {
                    b.set(HIT);
                }
                outcome = CollisionOutcome::Hit;
                true
            }
        } else if tile.block != crate::content::BlockId::AIR && collide_terrain {
            outcome = CollisionOutcome::Terrain;
            true
        } else {
            false
        }
    });

    if outcome == CollisionOutcome::None {
        // Direct overlap fallback (fast bullets that land on the same tile).
        if let Some(target) = grid.tiles.getn(end_x, end_y).and_then(|tile| tile.build)
            && target != bullet
        {
            let same_team = world.get::<TeamComp>(target).map(|t| t.team) == Some(bullet_team);
            if !same_team || collide_team {
                hit_building(world, content, bullet, target, true);
                if let Some(mut bullet) = world.get_mut::<Bullet>(bullet) {
                    bullet.set(HIT);
                }
                outcome = CollisionOutcome::Hit;
            }
        }
    }
    outcome
}

/// Despawns a bullet entity and clears any tile references (bullets have none).
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
        let target = harness.build.build_at(10, 10);
        let before = harness.building_health_at(10, 10);
        // Spawn a `fuse` bullet traveling east from tile (8,10).
        let (x, y) = CombatHarness::tile_center(8, 10);
        // Team 1 (enemy of the team-0 wall) so the bullet does not pass through.
        let spawned = harness.spawn_bullet("fuse", x, y, 0.0, 1);
        assert!(spawned.is_some());
        for _ in 0..120 {
            harness.tick();
        }
        let bullet_gone = harness.bullets.is_empty();
        assert!(bullet_gone, "bullet should have hit and despawned");
        let after = harness.building_health_at(10, 10);
        assert!(after < before, "wall took damage: {before} -> {after}");
        assert!(before - after > 0.0);
        assert!(target.is_some());
    }

    #[test]
    fn pierce_damages_each_wall_in_order() {
        let mut harness = CombatHarness::new(48, 16, 11);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        for x in 12..=14 {
            assert!(harness.place(x, 8, wall, 0, true));
        }
        assert!(harness.build_at(12, 8).is_some(), "wall placed");
        let (x, y) = CombatHarness::tile_center(4, 8);
        let entity = harness.spawn_bullet("rail", x, y, 0.0, 1).expect("spawn");
        for _ in 0..80 {
            harness.tick();
        }
        let damaged: Vec<i32> = (12..=14)
            .filter(|x| harness.building_health_at(*x, 8) < 320.0)
            .collect();
        assert!(
            damaged.len() >= 2,
            "expected pierce through multiple walls, damaged: {damaged:?}, rail alive: {}",
            harness.build.world.get_entity(entity).is_ok()
        );
    }

    #[test]
    fn just_spawned_does_not_move_first_tick() {
        let mut harness = CombatHarness::new(16, 16, 1);
        let (x, y) = CombatHarness::tile_center(4, 4);
        let e = harness.spawn_bullet("fuse", x, y, 0.0, 0).expect("spawn");
        let start = bullet_pos(&harness.build.world, e);
        // First tick initializes state but does not move (`justSpawned`).
        harness.step_bullets_only();
        let after_first = bullet_pos(&harness.build.world, e);
        assert_eq!(start, after_first);
        harness.step_bullets_only();
        let after_second = bullet_pos(&harness.build.world, e);
        assert_ne!(after_first, after_second);
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
}
