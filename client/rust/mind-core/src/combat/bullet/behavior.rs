// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Bullet behavior dispatch (`entities/bullet/BulletType.java` behavior half).
//!
//! Java's inheritance hierarchy becomes kind dispatch (plan 10 §2.4.1): vanilla
//! bullets are field-configured only, so a [`BulletBehavior`] supplies the
//! per-kind default hooks and the behavior table maps [`BulletKind`] to one
//! static implementation. Field-driven differences are read from the def.
//!
//! Hook signatures take [`CombatCtx`] so behaviors can spawn child bullets,
//! query the grid and mutate the ECS world exactly like `BulletType` methods do.

use bevy_ecs::entity::Entity;

use crate::content::{BulletKind, ContentRegistry};

use super::CombatCtx;

/// Per-kind bullet behavior hooks (`BulletType` virtual methods).
///
/// Hooks default to the base `BulletType` bodies; subclasses override the ones
/// that differ. Simulation code calls these through [`behavior_for`].
pub trait BulletBehavior: Sync + 'static {
    /// `BulletType.init(Bullet)`.
    fn init(&self, _ctx: &mut CombatCtx<'_>, _b: Entity) {}

    /// `BulletType.update(Bullet)` (kind-specific half, runs after motion).
    fn update(&self, _ctx: &mut CombatCtx<'_>, _b: Entity) {}

    /// `BulletType.hit(Bullet, float, float, boolean)`.
    fn hit(&self, ctx: &mut CombatCtx<'_>, b: Entity, x: f32, y: f32, create_frags: bool) {
        super::hit_bullet(ctx, b, x, y, create_frags);
    }

    /// `BulletType.hitTile(Bullet, Building, float, float, float, boolean)`.
    #[allow(clippy::too_many_arguments)]
    fn hit_tile(
        &self,
        ctx: &mut CombatCtx<'_>,
        b: Entity,
        _build: Entity,
        x: f32,
        y: f32,
        _initial_health: f32,
        _direct: bool,
    ) {
        super::hit_bullet(ctx, b, x, y, true);
    }

    /// `BulletType.despawned(Bullet)`.
    fn despawned(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        super::despawn_bullet(ctx, b);
    }

    /// `BulletType.removed(Bullet)`.
    fn removed(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        super::remove_bullet_hook(ctx, b);
    }

    /// `BulletType.testCollision(bullet, tile)`.
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

    /// `BulletType.continuousDamage()` (`-1` = not continuous).
    fn continuous_damage(&self, _damage: f32, _damage_interval: f32) -> f32 {
        -1.0
    }

    /// `BulletType.currentLength(bullet)` (continuous bullets override).
    fn current_length(&self, _ctx: &CombatCtx<'_>, _b: Entity) -> f32 {
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
pub fn behavior_for(kind: BulletKind) -> &'static dyn BulletBehavior {
    use BulletKind as K;
    match kind {
        K::Basic | K::LaserBolt => &super::kinds::basic::BASIC,
        K::Point => &super::kinds::point::POINT,
        K::Multi => &super::kinds::multi::MULTI,
        K::Emp => &super::kinds::emp::EMP,
        K::Flak => &super::kinds::flak::FLAK,
        K::Sap => &super::kinds::sap::SAP,
        K::Shrapnel => &super::kinds::shrapnel::SHRAPNEL,
        K::Interceptor => &super::kinds::interceptor::INTERCEPTOR,
        K::MassDriver => &super::kinds::mass_driver::MASS_DRIVER,
        K::Empty => &super::kinds::empty::EMPTY,
        K::Continuous | K::ContinuousLaser | K::ContinuousFlame | K::PointLaser => {
            &super::kinds::continuous::CONTINUOUS
        }
        K::Lightning => &super::kinds::lightning::LIGHTNING,
        K::Liquid | K::SpaceLiquid => &super::kinds::liquid::LIQUID,
        K::Fire => &super::kinds::fire::FIRE,
        K::Laser => &super::kinds::laser::LASER,
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
            BulletKind::Continuous,
            BulletKind::ContinuousLaser,
            BulletKind::Shrapnel,
            BulletKind::Liquid,
            BulletKind::Emp,
            BulletKind::Bomb,
            BulletKind::Multi,
            BulletKind::Point,
            BulletKind::PointLaser,
            BulletKind::ContinuousFlame,
            BulletKind::Interceptor,
            BulletKind::MassDriver,
            BulletKind::Empty,
        ] {
            let _ = behavior_for(kind);
        }
    }

    #[test]
    fn building_damage_multiplies() {
        assert_eq!(BASE.building_damage(10.0, 2.0), 20.0);
    }
}
