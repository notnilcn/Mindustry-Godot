// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bullet spawning (`BulletType.create(...)` + `Call.createBullet` handshake).
//!
//! All `create` overloads funnel through [`create`]. `create_net` is the same
//! code path with a relay command attached (plan 21 owns `SimCommand::Bullet`);
//! this plan owns only the payload shape ([`BulletSpawn`]) and guarantees that
//! relayed and local creation are identical (plan 10 §3.5).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BulletId, ContentRegistry};
use crate::determinism::SimRng;

use super::{Bullet, BulletData, JUST_SPAWNED, bullet_bundle};

/// Complete bullet-creation request (`BulletType.create` argument set).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BulletSpawn {
    /// Bullet content id.
    pub def: BulletId,
    /// Owning entity (usually the shooter's owner for `owner`-relative data).
    pub owner: Option<Entity>,
    /// Shooter entity.
    pub shooter: Option<Entity>,
    /// Team id.
    pub team: u8,
    /// Spawn x (world units).
    pub x: f32,
    /// Spawn y (world units).
    pub y: f32,
    /// Aim angle in degrees.
    pub angle: f32,
    /// Damage (`< 0` = def default).
    pub damage: f32,
    /// Global damage multiplier (`Damage.damageMultiplier`).
    pub damage_multiplier: f32,
    /// Velocity scale.
    pub velocity_scl: f32,
    /// Lifetime scale.
    pub lifetime_scl: f32,
    /// Transient payload.
    pub data: BulletData,
    /// Aim point (`-1` = none).
    pub aim_x: f32,
    /// Aim point (`-1` = none).
    pub aim_y: f32,
    /// Aim tile.
    pub aim_tile: Option<(i16, i16)>,
    /// Sticky target (plan 10 §3.3).
    pub target: Option<Entity>,
    /// Deterministic per-tick mover (`BulletComp.mover`; plan 10 M4).
    pub mover: Option<super::ShotMover>,
}

impl Default for BulletSpawn {
    fn default() -> Self {
        Self {
            def: BulletId::new(0),
            owner: None,
            shooter: None,
            team: 0,
            x: 0.0,
            y: 0.0,
            angle: 0.0,
            damage: -1.0,
            damage_multiplier: 1.0,
            velocity_scl: 1.0,
            lifetime_scl: 1.0,
            data: BulletData::None,
            aim_x: -1.0,
            aim_y: -1.0,
            aim_tile: None,
            target: None,
            mover: None,
        }
    }
}

/// Creates a bullet entity (`BulletType.create` core), returning its handle.
///
/// `seq` is the harness's deterministic entity sequence. Returns `None` when the
/// def is unknown or has been removed.
pub fn create(
    world: &mut World,
    content: &ContentRegistry,
    rng: &mut SimRng,
    seq: u64,
    spawn: &BulletSpawn,
) -> Option<Entity> {
    let def = content.bullet(spawn.def)?;
    if def.removed {
        return None;
    }

    let damage = if spawn.damage < 0.0 {
        def.damage
    } else {
        spawn.damage
    } * spawn.damage_multiplier;

    // `BulletType.create`: angle offset, random offset, create chance.
    let mut angle = spawn.angle + def.angle_offset;
    if def.random_angle_offset != 0.0 {
        angle += rng.range(
            crate::determinism::RngStream::Sim,
            -def.random_angle_offset,
            def.random_angle_offset,
        );
    }
    if def.create_chance < 1.0
        && !rng.chance(crate::determinism::RngStream::Sim, def.create_chance as f64)
    {
        return None;
    }
    if def.ignore_spawn_angle {
        angle = 0.0;
    }

    let velocity_rand = if def.velocity_scale_rand_min != 1.0 || def.velocity_scale_rand_max != 1.0
    {
        rng.range(
            crate::determinism::RngStream::Sim,
            def.velocity_scale_rand_min,
            def.velocity_scale_rand_max,
        )
    } else {
        1.0
    };
    let speed = def.speed * spawn.velocity_scl * velocity_rand;
    // Deterministic inaccuracy from the sim stream (view-only angle jitter
    // beyond the def's `inaccuracy` is applied by the weapon layer).
    let jitter = if def.inaccuracy > 0.0 {
        rng.range(
            crate::determinism::RngStream::Sim,
            -def.inaccuracy,
            def.inaccuracy,
        )
    } else {
        0.0
    };
    let angle = angle + jitter;
    let rad = angle.to_radians();
    let vel = (rad.cos() * speed, rad.sin() * speed);

    let life_rand = if def.life_scale_rand_min != 1.0 || def.life_scale_rand_max != 1.0 {
        rng.range(
            crate::determinism::RngStream::Sim,
            def.life_scale_rand_min,
            def.life_scale_rand_max,
        )
    } else {
        1.0
    };
    let lifetime = def.lifetime * spawn.lifetime_scl * life_rand;

    let bullet = Bullet {
        def: spawn.def,
        damage,
        building_damage_multiplier: def.building_damage_multiplier,
        data: spawn.data,
        fdata: 0.0,
        rotation: angle,
        hit_size: def.hit_size,
        last: (spawn.x, spawn.y),
        aim: (spawn.aim_x, spawn.aim_y),
        origin: (spawn.x, spawn.y),
        aim_tile: spawn.aim_tile,
        time: 0.0,
        lifetime,
        collided: smallvec::SmallVec::new(),
        sticky: spawn.target,
        sticky_x: 0.0,
        sticky_y: 0.0,
        flags: JUST_SPAWNED,
        frags: 0,
        mover: spawn.mover,
    };

    let entity = world
        .spawn(bullet_bundle(
            seq, spawn.x, spawn.y, vel, spawn.team, bullet,
        ))
        .id();
    // Owner/shooter are retained on the spawn request for plan 11/21 consumers.
    let _ = (spawn.owner, spawn.shooter);

    // `init(Bullet)` hook (kind-specific immediate effects).
    let behavior = super::behavior_for(def.kind);
    let _ = behavior;
    Some(entity)
}

/// Relay variant of [`create`] (plan 21 `SimCommand::Bullet` handshake).
///
/// The local creation path is identical; the command is emitted by plan 21's
/// authoritative relay. Until plan 21 lands this is a thin alias so callers can
/// express intent without a second code path.
pub fn create_net(
    world: &mut World,
    content: &ContentRegistry,
    rng: &mut SimRng,
    seq: u64,
    spawn: &BulletSpawn,
) -> Option<Entity> {
    create(world, content, rng, seq, spawn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn create_applies_default_damage_and_velocity() {
        use crate::content::BulletKind;
        use crate::content::registries::bullets::BulletDef;
        let mut content = test_registry();
        let mut def = BulletDef::new(BulletKind::Basic);
        def.speed = 4.0;
        def.lifetime = 50.0;
        def.damage = 25.0;
        def.drag = 0.0;
        let id = content.add_bullet(def).expect("add bullet");
        let mut world = World::new();
        let mut rng = SimRng::new(7);
        let spawn = BulletSpawn {
            def: id,
            x: 8.0,
            y: 8.0,
            angle: 0.0,
            velocity_scl: 1.0,
            lifetime_scl: 1.0,
            ..BulletSpawn::default()
        };
        let entity = create(&mut world, &content, &mut rng, 0, &spawn).expect("created");
        let bullet = world.get::<Bullet>(entity).expect("bullet");
        assert_eq!(bullet.damage, 25.0);
        assert!(bullet.has(JUST_SPAWNED));
        let vel = world
            .get::<crate::entities::comp::Vel>(entity)
            .expect("vel");
        assert!(vel.x > 0.0);
        assert_eq!(vel.y, 0.0);
    }
}
