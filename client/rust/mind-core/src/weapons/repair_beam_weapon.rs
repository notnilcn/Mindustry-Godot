// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RepairBeamWeapon` behavior (`core/src/mindustry/type/weapons/RepairBeamWeapon.java`).
//!
//! Healing is applied in `update` (the shooting path does nothing). Target
//! *selection* (`findTarget`/`checkTarget`) is plan 11's `TargetQueries`; this
//! module ports `HealBeamMount` smoothing and the heal application.

use bevy_ecs::entity::Entity;

use crate::combat::bullet::CombatCtx;
use crate::content::registries::units::weapon::WeaponDef;
use crate::entities::comp::Health;

use super::WeaponMount;

/// Ports `HealBeamMount` healing portion of `RepairBeamWeapon.update`.
///
/// Returns whether a heal was applied this tick.
pub fn update(
    ctx: &mut CombatCtx<'_>,
    _unit: Entity,
    weapon: &WeaponDef,
    mount: &mut WeaponMount,
) -> bool {
    let can_shoot = mount.shoot;
    let has_target = mount.target.is_some();
    let strength_target = if has_target && can_shoot { 1.0 } else { 0.0 };
    // `Mathf.lerpDelta(heal.strength, ..., 0.2f)`.
    mount.heal.strength += (strength_target - mount.heal.strength) * 0.2;

    // Periodic heal effect (`healEffect.at`).
    if can_shoot && has_target {
        mount.heal.effect_timer += 1.0;
        if mount.heal.effect_timer >= weapon.reload {
            if let Some(target) = mount.target
                && let Some((x, y)) = ctx.pos(target)
            {
                ctx.fx.effect(
                    &crate::content::registries::fx_meta::EffectRef::Named(
                        crate::content::registries::fx_meta::EffectId::HEAL_BLOCK,
                    ),
                    x,
                    y,
                    0.0,
                    weapon.heal_color,
                );
            }
            mount.heal.effect_timer = 0.0;
        }
    }

    if !can_shoot {
        return false;
    }
    let Some(target) = mount.target else {
        return false;
    };
    let Some(max_health) = ctx
        .world
        .get::<Health>(target)
        .map(|health| health.max_health)
    else {
        return false;
    };
    let base_amount = weapon.repair_speed * mount.heal.strength
        + weapon.fraction_repair_speed * mount.heal.strength * max_health / 100.0;
    if let Some(mut health) = ctx.world.get_mut::<Health>(target) {
        health.health = (health.health + base_amount).min(health.max_health);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn repair_beam_heals_damaged_ally() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 8, wall, 0, true));
        let target = harness.build_at(10, 8).expect("target");
        // Damage the wall.
        harness
            .build
            .world
            .get_mut::<Health>(target)
            .unwrap()
            .health = 100.0;
        let mut mount = WeaponMount::new(&repair_weapon());
        mount.target = Some(target);
        mount.shoot = true;
        {
            let mut ctx = harness.combat_ctx();
            assert!(update(&mut ctx, target, &repair_weapon(), &mut mount));
        }
        harness.claim_scratch_spawned();
        let healed = harness.build.world.get::<Health>(target).unwrap().health;
        assert!(healed > 100.0, "healed to {healed}");
    }

    fn repair_weapon() -> WeaponDef {
        use crate::content::registries::units::weapon::{WeaponKind, WeaponSpec};
        let registry = crate::content::test_support::test_registry();
        WeaponDef::from_spec(
            WeaponSpec {
                name: "repair",
                kind: WeaponKind::RepairBeamWeapon,
                ..WeaponSpec::default()
            },
            crate::content::registries::units::ResolvedBullet {
                id: crate::content::BulletId::new(0),
                range: 120.0,
                heals: true,
                kill_shooter: false,
                dps: 0.0,
            },
            &registry,
        )
        .expect("repair weapon")
    }
}
