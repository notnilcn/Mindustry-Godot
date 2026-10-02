// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Weapon-mount storage (plan 11 §3.6/§3.9).
//!
//! Plan 11 owns unit **state**; plan 10 owns firing behavior
//! (`weapons/mount.rs::update_weapon`, bullet spawn, recoil/heat math). Plan 10
//! M4 is **not on this branch**, so this module provides the storage shape
//! ([`WeaponMount`], [`WeaponsComp`]) and `setupWeapons` mount layout exactly as
//! plan 10 §3.7 specifies. The behavioral half is a marked seam:
//!
//! ```text
//! TODO(plan 10 M4): weapons::update_weapon(world, unit, weapon, mount)
//! TODO(plan 10 M4): Weapon::shoot -> Bullet creation, SimClock delay, barrel counter
//! TODO(plan 10 M4): handle_bullet for PointDefense/Continuous, HealBeamMount
//! ```
//!
//! Mirrored mounts and doubled reload are derived by plan 02's `UnitTypeDef.init`
//! (the `weapons` vec is already mirror-expanded), so `setup_weapons` only lays
//! out `unit.weapons.len()` mounts.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use smallvec::SmallVec;

use crate::content::registries::units::UnitTypeDef;
use crate::content::registries::units::weapon::WeaponDef;

/// Runtime weapon mount (`mindustry.gen.WeaponMount`; plan 10 §3.7 shape).
#[derive(Debug, Clone, PartialEq, Component)]
pub struct WeaponMount {
    /// Reload counter in ticks (`reload`).
    pub reload: f32,
    /// Recoil offset (`recoil`).
    pub recoil: f32,
    /// Per-barrel recoil offsets (`recoils`).
    pub recoils: SmallVec<[f32; 4]>,
    /// Smooth-reload interpolation (`smoothReload`).
    pub smooth_reload: f32,
    /// Charge fraction (`charge`).
    pub charge: f32,
    /// Warmup fraction (`warmup`).
    pub warmup: f32,
    /// Heat fraction (`heat`).
    pub heat: f32,
    /// Current mount rotation (`rotation`).
    pub rotation: f32,
    /// Desired mount rotation (`targetRotation`).
    pub target_rotation: f32,
    /// Aim position (`aimX`, `aimY`).
    pub aim: (f32, f32),
    /// Current target (`target`).
    pub target: Option<Entity>,
    /// Retarget countdown (`retarget`).
    pub retarget: f32,
    /// Whether the mount is firing (`shoot`).
    pub shoot: bool,
    /// Whether the mount rotates (`rotate`).
    pub rotate: bool,
    /// Alternate-side flag (`side`).
    pub side: bool,
    /// Charging flag (`charging`).
    pub charging: bool,
    /// Barrel counter (`barrelCounter`).
    pub barrel_counter: i32,
    /// Shots fired this burst (`totalShots`).
    pub total_shots: i32,
    /// Live bullet entity (`bullet`).
    pub bullet: Option<Entity>,
    /// Continuous-weapon last length (`lastLength`).
    pub last_length: f32,
    /// Whether muzzle effects may fire (`allowShootEffects`).
    pub allow_shoot_effects: bool,
}

impl Default for WeaponMount {
    fn default() -> Self {
        Self {
            reload: 0.0,
            recoil: 0.0,
            recoils: SmallVec::new(),
            smooth_reload: 0.0,
            charge: 0.0,
            warmup: 0.0,
            heat: 0.0,
            rotation: 0.0,
            target_rotation: 0.0,
            aim: (0.0, 0.0),
            target: None,
            retarget: 0.0,
            shoot: false,
            rotate: true,
            side: false,
            charging: false,
            barrel_counter: 0,
            total_shots: 0,
            bullet: None,
            last_length: 0.0,
            allow_shoot_effects: true,
        }
    }
}

/// Unit weapon array + shared aiming state (`WeaponsComp`).
#[derive(Debug, Clone, PartialEq, Component, Default)]
pub struct WeaponsComp {
    /// One mount per (mirror-expanded) weapon def.
    pub mounts: Vec<WeaponMount>,
    /// Shared aim x (`aimX`).
    pub aim_x: f32,
    /// Shared aim y (`aimY`).
    pub aim_y: f32,
    /// Whether any mount is shooting (`isShooting`).
    pub is_shooting: bool,
}

impl WeaponsComp {
    /// Whether the unit has any mount.
    pub fn has_weapons(&self) -> bool {
        !self.mounts.is_empty()
    }
}

/// Builds the mount array for `unit` (`UnitType.setupWeapons`).
///
/// Mount `rotation` starts at `unit rotation + weapon.baseRotation`;
/// `rotate`/`alternate` mirror the def. Firing behavior is plan 10's.
pub fn setup_weapons(unit: &UnitTypeDef, rotation: f32) -> WeaponsComp {
    let mounts = unit
        .weapons
        .iter()
        .map(|weapon| mount_for(weapon, rotation))
        .collect();
    WeaponsComp {
        mounts,
        aim_x: 0.0,
        aim_y: 0.0,
        is_shooting: false,
    }
}

fn mount_for(weapon: &WeaponDef, rotation: f32) -> WeaponMount {
    let mount_rotation = rotation + weapon.base_rotation;
    WeaponMount {
        rotation: mount_rotation,
        target_rotation: mount_rotation,
        rotate: weapon.rotate,
        side: false,
        recoils: smallvec::smallvec![0.0; weapon.recoils.max(1) as usize],
        ..WeaponMount::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore};

    fn content() -> crate::content::ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn mounts_match_weapon_count_and_base_rotation() {
        let content = content();
        let unit = content.unit_by_name("dagger").expect("dagger");
        let weapons = setup_weapons(unit, 90.0);
        assert_eq!(weapons.mounts.len(), unit.weapons.len());
        for (mount, def) in weapons.mounts.iter().zip(&unit.weapons) {
            assert_eq!(mount.rotation, 90.0 + def.base_rotation);
            assert_eq!(mount.rotate, def.rotate);
            assert!(!mount.recoils.is_empty());
            assert!(!mount.shoot);
        }
    }

    #[test]
    fn unarmed_content_has_no_mounts() {
        let content = content();
        let unit = content.unit_by_name("block").expect("block");
        assert!(setup_weapons(unit, 0.0).mounts.is_empty());
    }
}
