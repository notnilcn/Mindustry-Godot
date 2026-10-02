// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bullet behavior dispatch (`entities/bullet/BulletType.java` behavior half).
//!
//! Java's inheritance hierarchy becomes kind dispatch (plan 10 §2.4.1): vanilla
//! bullets are field-configured only, so a [`BulletBehavior`] supplies the
//! per-kind default hooks and the behavior table maps [`BulletKind`] to one
//! static implementation. Field-driven differences are read from the def.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BulletKind, ContentRegistry};

/// Borrow bundle handed to bullet behavior hooks (plan 10 §3.4).
pub struct BulletWorld<'a> {
    /// Live ECS world.
    pub world: &'a mut World,
    /// Content registry.
    pub content: &'a ContentRegistry,
    /// Tile grid.
    pub grid: &'a crate::world::WorldGrid,
    /// FX sink seam (plan 17).
    pub fx: &'a dyn crate::combat::view::FxSink,
    /// Deterministic combat RNG.
    pub rng: &'a mut crate::determinism::SimRng,
}

/// Per-kind bullet behavior hooks (`BulletType` virtual methods).
///
/// Hooks default to the base `BulletType` bodies; subclasses override the ones
/// that differ. Simulation code calls these through [`behavior_for`].
pub trait BulletBehavior: Sync + 'static {
    /// `BulletType.init(Bullet)`.
    fn init(&self, _w: &mut BulletWorld<'_>, _b: Entity) {}

    /// `BulletType.update(Bullet)` (kind-specific half).
    fn update(&self, _w: &mut BulletWorld<'_>, _b: Entity) {}

    /// `BulletType.hit(Bullet, float, float, boolean)`.
    fn hit(&self, _w: &mut BulletWorld<'_>, _b: Entity, _x: f32, _y: f32, _create_frags: bool) {}

    /// `BulletType.despawned(Bullet)`.
    fn despawned(&self, _w: &mut BulletWorld<'_>, _b: Entity) {}

    /// `BulletType.removed(Bullet)`.
    fn removed(&self, _w: &mut BulletWorld<'_>, _b: Entity) {}

    /// `BulletType.testCollision(bullet, build)`.
    fn test_collision(
        &self,
        _content: &ContentRegistry,
        _def: crate::content::BulletId,
        _team: u8,
        _target_team: u8,
    ) -> bool {
        true
    }

    /// `BulletType.buildingDamage(bullet)`.
    fn building_damage(&self, bullet_damage: f32, multiplier: f32) -> f32 {
        bullet_damage * multiplier
    }

    /// `BulletType.shieldDamage(bullet)`.
    fn shield_damage(&self, bullet_damage: f32, multiplier: f32) -> f32 {
        bullet_damage * multiplier
    }

    /// `BulletType.currentLength(bullet)` (continuous bullets override).
    fn current_length(&self, _def_range: f32) -> f32 {
        0.0
    }

    /// `BulletType.range()` (derived range from the def).
    fn range(&self, def_range: f32) -> f32 {
        def_range
    }
}

/// Base `BulletType` behavior (all hooks at upstream base defaults).
#[derive(Debug, Default, Clone, Copy)]
pub struct BaseBehavior;

impl BulletBehavior for BaseBehavior {}

/// Looks up the behavior implementation for a bullet kind.
///
/// Kinds without a dedicated implementation yet fall back to the base
/// `BulletType` behavior; later milestones add their `kinds/*` statics here.
pub fn behavior_for(kind: BulletKind) -> &'static dyn BulletBehavior {
    use BulletKind as K;
    match kind {
        K::Basic | K::LaserBolt => &super::kinds::basic::BASIC,
        _ => &BASE,
    }
}

static BASE: BaseBehavior = BaseBehavior;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn behavior_table_covers_every_kind() {
        // Every kind resolves to a behavior without panicking.
        for kind in [
            BulletKind::Plain,
            BulletKind::Basic,
            BulletKind::Fire,
            BulletKind::SpaceLiquid,
            BulletKind::Artillery,
            BulletKind::Missile,
            BulletKind::LaserBolt,
            BulletKind::Sap,
            BulletKind::Lightning,
            BulletKind::Laser,
            BulletKind::Flak,
            BulletKind::Explosion,
            BulletKind::Rail,
            BulletKind::ContinuousLaser,
            BulletKind::Shrapnel,
            BulletKind::Liquid,
            BulletKind::Emp,
            BulletKind::Bomb,
        ] {
            let _ = behavior_for(kind);
        }
    }

    #[test]
    fn building_damage_multiplies() {
        assert_eq!(BASE.building_damage(10.0, 2.0), 20.0);
    }
}
