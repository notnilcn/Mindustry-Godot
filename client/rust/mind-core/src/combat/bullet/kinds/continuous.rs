// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Continuous (line) bullet behaviors
//! (`entities/bullet/{ContinuousBulletType,ContinuousLaserBulletType,
//! ContinuousFlameBulletType,PointLaserBulletType}.java`).
//!
//! A continuous bullet does not collide as a projectile; instead it damages
//! along a line every `damageInterval` ticks (`Damage.collideLine`). The
//! concrete subclasses differ in draw data and length interpolation, which
//! plans 16/17 read from the def.

use bevy_ecs::entity::Entity;

use crate::content::BulletKind;

use super::super::behavior::BulletBehavior;
use super::super::{Bullet, CombatCtx, collide_line_bullet};
use super::motion;

/// `ContinuousBulletType` behavior (shared by all continuous subclasses).
#[derive(Debug, Default, Clone, Copy)]
pub struct ContinuousBehavior;

impl BulletBehavior for ContinuousBehavior {
    fn update(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(def_id) = ctx.bullet(b).map(|bullet| bullet.def) else {
            return;
        };
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let interval = def.damage_interval;
        let time = ctx.bullet(b).map(|bullet| bullet.time).unwrap_or(0.0);
        if interval <= 0.0 || (time % interval).abs() >= 0.5 {
            return;
        }
        let pierce_cap = def.pierce_cap;
        let large = def.large_hit;
        let Some(((x, y), _vel, rotation)) = motion(ctx, b) else {
            return;
        };
        let length = self.current_length(ctx, b);
        // `ContinuousBulletType.applyDamage` (`ContinuousBulletType.java:82-90`):
        // the pierce falloff mutates `b.damage`, so save and restore it around
        // the line collision.
        let damage = ctx.bullet(b).map(|bullet| bullet.damage).unwrap_or(0.0);
        // `Damage.collideLine(..., largeHit, laserAbsorb, pierceCap)`
        // (`ContinuousBulletType.java:88`): `findLength` cuts the beam (L06 T3).
        let laser = def.laser_absorb;
        let _ = collide_line_bullet(ctx, b, x, y, rotation, length, large, laser, pierce_cap);
        if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(b) {
            bullet.damage = damage;
        }
    }

    fn continuous_damage(&self, damage: f32, damage_interval: f32) -> f32 {
        if damage_interval <= 0.0 {
            return -1.0;
        }
        damage / damage_interval * 60.0
    }

    fn current_length(&self, ctx: &CombatCtx<'_>, b: Entity) -> f32 {
        let Some(bullet) = ctx.bullet(b) else {
            return 0.0;
        };
        let Some(def) = ctx.content.bullet(bullet.def) else {
            return 0.0;
        };
        match def.kind {
            // `ContinuousLaserBulletType.currentLength` (`:94-98`): the beam
            // fades over the last `fadeTime` ticks (`fadeTime` has no vanilla
            // override, so the class default is exact here).
            BulletKind::ContinuousLaser => {
                const FADE_TIME: f32 = 16.0;
                let fout = if bullet.time > bullet.lifetime - FADE_TIME {
                    (1.0 - (bullet.time - (def.lifetime - FADE_TIME)) / FADE_TIME).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                def.length * fout
            }
            // `ContinuousFlameBulletType.currentLength` (`:96-98`):
            // `length * b.fin(lengthInterp)` with the class default
            // `Interp.slope` (`1 - |fin - 0.5| * 2`).
            BulletKind::ContinuousFlame => {
                let fin = if bullet.lifetime <= 0.0 {
                    1.0
                } else {
                    (bullet.time / bullet.lifetime).clamp(0.0, 1.0)
                };
                def.length * (1.0 - (fin - 0.5).abs() * 2.0)
            }
            _ => def.length.max(def.range),
        }
    }

    fn range(&self, def_range: f32) -> f32 {
        def_range
    }
}

/// Static continuous-behavior instance.
pub static CONTINUOUS: ContinuousBehavior = ContinuousBehavior;

/// `PointLaserBulletType` behavior (`PointLaserBulletType.java:78-95`).
///
/// Every `damageInterval` ticks the bullet damages the aim point
/// (`Damage.collidePoint`); the beam itself is view-only. The owning
/// `ContinuousTurret` rewrites `time` every tick, so the interval accumulator
/// lives in `fdata` (unused for this kind).
#[derive(Debug, Default, Clone, Copy)]
pub struct PointLaserBehavior;

impl BulletBehavior for PointLaserBehavior {
    fn update(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(state) = ctx.bullet(b) else {
            return;
        };
        let (def_id, aim, accrued) = (state.def, state.aim, state.fdata + 1.0);
        let Some(def) = ctx.content.bullet(def_id) else {
            return;
        };
        let interval = if def.damage_interval > 0.0 {
            def.damage_interval
        } else {
            5.0
        };
        let fire = accrued >= interval;
        if let Some(mut state) = ctx.world.get_mut::<Bullet>(b) {
            state.fdata = if fire { accrued - interval } else { accrued };
        }
        if !fire {
            return;
        }
        // `Damage.collidePoint(b, b.team, hitEffect, b.aimX, b.aimY)`.
        let (x, y) = if aim.0 >= 0.0 && aim.1 >= 0.0 {
            aim
        } else {
            ctx.pos(b).unwrap_or((0.0, 0.0))
        };
        crate::combat::damage::line::collide_point(ctx, b, x, y);
    }
}

/// Static point-laser behavior instance.
pub static POINT_LASER: PointLaserBehavior = PointLaserBehavior;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::bullet::behavior_for;
    use crate::combat::harness::CombatHarness;
    use crate::entities::comp::Health;

    /// `ContinuousLaserBulletType.currentLength` (`:94-98`): the beam fades
    /// linearly over the last `fadeTime` (16) ticks.
    #[test]
    fn continuous_laser_fades_over_the_last_fade_time() {
        let mut h = CombatHarness::new(16, 16, 7);
        let meltdown = h.content().bullet_by_name("meltdown_laser").expect("ammo");
        let bullet = h.spawn_def(meltdown, 50.0, 50.0, 0.0, 1).expect("bullet");
        let length = |h: &mut CombatHarness, time: f32| {
            h.build
                .world
                .get_mut::<Bullet>(bullet)
                .expect("bullet state")
                .time = time;
            let ctx = h.combat_ctx();
            behavior_for(BulletKind::ContinuousLaser).current_length(&ctx, bullet)
        };
        assert_eq!(length(&mut h, 0.0), 200.0, "full length before the fade");
        assert_eq!(length(&mut h, 8.0), 100.0, "half faded at time 8");
        assert_eq!(length(&mut h, 16.0), 0.0, "fully faded at the lifetime");
    }

    /// `ContinuousFlameBulletType.currentLength` (`:96-98`): `length *
    /// b.fin(Interp.slope)` — zero at both ends, full at the lifecycle midpoint.
    #[test]
    fn continuous_flame_uses_the_length_interp() {
        let mut h = CombatHarness::new(16, 16, 7);
        let ozone = h.content().bullet_by_name("sublimate_ozone").expect("ammo");
        let bullet = h.spawn_def(ozone, 50.0, 50.0, 0.0, 1).expect("bullet");
        let length = |h: &mut CombatHarness, time: f32| {
            h.build
                .world
                .get_mut::<Bullet>(bullet)
                .expect("bullet state")
                .time = time;
            let ctx = h.combat_ctx();
            behavior_for(BulletKind::ContinuousFlame).current_length(&ctx, bullet)
        };
        assert_eq!(length(&mut h, 0.0), 0.0, "slope starts at zero");
        assert_eq!(length(&mut h, 8.0), 130.0, "peak at the midpoint");
        assert_eq!(length(&mut h, 16.0), 0.0, "slope ends at zero");
    }

    /// `PointLaserBulletType.update` (`PointLaserBulletType.java:78-95`):
    /// `Damage.collidePoint(b, b.team, hitEffect, b.aimX, b.aimY)` every
    /// `damageInterval` ticks — damage at the **aim point**, not the muzzle.
    #[test]
    fn point_laser_damages_at_the_aim_point() {
        let mut h = CombatHarness::new(64, 64, 7);
        let near_muzzle = h.spawn_test_unit(100.0, 100.0, 2, Vec::new());
        let at_aim = h.spawn_test_unit(140.0, 100.0, 2, Vec::new());
        let lustre = h.content().bullet_by_name("lustre_laser").expect("ammo");
        let bullet = {
            let mut ctx = h.combat_ctx();
            ctx.spawn(&crate::combat::bullet::BulletSpawn {
                def: lustre,
                // Explicit damage: the registered ammo is Erekir-rebalanced
                // (`ErekirTechTree.rebalance` part b), the formula under test is
                // `Damage.collidePoint`.
                damage: 210.0,
                x: 100.0,
                y: 100.0,
                team: 1,
                aim_x: 140.0,
                aim_y: 100.0,
                ..crate::combat::bullet::BulletSpawn::default()
            })
        }
        .expect("beam");
        h.claim_scratch_spawned();
        for _ in 0..5 {
            h.step_bullets_only();
        }
        let aim_health = h
            .build
            .world
            .get::<Health>(at_aim)
            .expect("aim health")
            .health;
        let muzzle_health = h
            .build
            .world
            .get::<Health>(near_muzzle)
            .expect("muzzle health")
            .health;
        assert_eq!(
            muzzle_health, 1_000.0,
            "the unit at the muzzle is not on the aim point"
        );
        assert_eq!(
            aim_health, 790.0,
            "the unit at the aim point takes the point-laser damage"
        );
        // A second interval applies another hit (lifetime 20, interval 5).
        for _ in 0..5 {
            h.step_bullets_only();
        }
        let aim_health = h
            .build
            .world
            .get::<Health>(at_aim)
            .expect("aim health")
            .health;
        assert_eq!(aim_health, 580.0, "damage repeats every 5 ticks");
        let _ = bullet;
    }
}
