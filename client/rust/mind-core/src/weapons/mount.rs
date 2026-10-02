// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Weapon mount state (`core/src/mindustry/entities/units/WeaponMount.java`).
//!
//! Plan 11 owns where mounts are stored on units; this module owns the state
//! shape and the `HealBeamMount` subclass (`RepairBeamWeapon.HealBeamMount`).

use bevy_ecs::entity::Entity;
use smallvec::SmallVec;

use crate::content::registries::units::weapon::WeaponDef;

/// Live per-weapon mount state (`WeaponMount`).
#[derive(Debug, Clone, PartialEq)]
pub struct WeaponMount {
    /// Reload in ticks; `0` means ready to fire.
    pub reload: f32,
    /// Rotation relative to the unit, in degrees.
    pub rotation: f32,
    /// Visual recoil `0..1`.
    pub recoil: f32,
    /// Per-barrel recoil counters (`Weapon.recoils`).
    pub recoils: SmallVec<[f32; 4]>,
    /// Destination rotation; do not modify externally.
    pub target_rotation: f32,
    /// Current heat `0..1`.
    pub heat: f32,
    /// Lerps to `1` when shooting, `0` when not.
    pub warmup: f32,
    /// Whether the weapon is actively charging.
    pub charging: bool,
    /// Charge `0..1`.
    pub charge: f32,
    /// Lerps to the reload fraction.
    pub smooth_reload: f32,
    /// Aiming position in world coordinates (`-1` = unset).
    pub aim_x: f32,
    /// Aiming position in world coordinates (`-1` = unset).
    pub aim_y: f32,
    /// Whether to shoot right now.
    pub shoot: bool,
    /// Whether to allow shooting effects.
    pub allow_shoot_effects: bool,
    /// Whether to rotate toward the target right now.
    pub rotate: bool,
    /// Extra state for alternating mirrored weapons.
    pub side: bool,
    /// Total bullets fired from this mount.
    pub total_shots: i32,
    /// Barrel counter used by alternating patterns.
    pub barrel_counter: i32,
    /// Last aim length (point lasers / continuous weapons).
    pub last_length: f32,
    /// Current continuous bullet.
    pub bullet: Option<Entity>,
    /// Current target; used for autonomous weapons and AI.
    pub target: Option<Entity>,
    /// Retarget counter.
    pub retarget: f32,
    /// Extra `HealBeamMount` state (`RepairBeamWeapon`).
    pub heal: HealBeamMount,
}

impl WeaponMount {
    /// `new WeaponMount(weapon)`.
    pub fn new(weapon: &WeaponDef) -> Self {
        Self {
            reload: 0.0,
            rotation: weapon.base_rotation,
            recoil: 0.0,
            recoils: SmallVec::new(),
            target_rotation: 0.0,
            heat: 0.0,
            warmup: 0.0,
            charging: false,
            charge: 0.0,
            smooth_reload: 0.0,
            aim_x: -1.0,
            aim_y: -1.0,
            shoot: false,
            allow_shoot_effects: true,
            rotate: false,
            side: false,
            total_shots: 0,
            barrel_counter: 0,
            last_length: 0.0,
            bullet: None,
            target: None,
            retarget: 0.0,
            heal: HealBeamMount::default(),
        }
    }
}

/// `RepairBeamWeapon.HealBeamMount` extra state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HealBeamMount {
    /// Beam start offset from the mount.
    pub offset: (f32, f32),
    /// Last beam end point.
    pub last_end: (f32, f32),
    /// Smoothed heal strength.
    pub strength: f32,
    /// Effect timer.
    pub effect_timer: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::registries::units::weapon::WeaponSpec;

    fn weapon(base_rotation: f32) -> WeaponDef {
        let spec = WeaponSpec {
            base_rotation: Some(base_rotation),
            ..WeaponSpec::default()
        };
        let registry = crate::content::test_support::test_registry();
        WeaponDef::from_spec(
            spec,
            crate::content::registries::units::ResolvedBullet {
                id: crate::content::BulletId::new(0),
                range: 0.0,
                heals: false,
                kill_shooter: false,
                dps: 0.0,
            },
            &registry,
        )
        .expect("weapon")
    }

    #[test]
    fn mount_starts_at_base_rotation() {
        let mount = WeaponMount::new(&weapon(90.0));
        assert_eq!(mount.rotation, 90.0);
        assert_eq!(mount.reload, 0.0);
        assert!(mount.target.is_none());
    }
}
