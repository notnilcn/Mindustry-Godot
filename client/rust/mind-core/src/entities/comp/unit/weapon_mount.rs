// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Weapon-mount storage (plan 11 §3.6/§3.9).
//!
//! Plan 11 owns where mounts live on a unit; plan 10 (`mind-core/src/weapons/`)
//! owns the mount state shape, the reload/warmup/rotation math and the shoot
//! patterns. The plan-10 engine is merged on `main`, so this module is now a thin
//! reconciliation layer: it re-exports plan 10's [`UnitWeapons`] /
//! [`WeaponMount`] / [`UnitState`] and provides `setup_weapons`, which lays out
//! one mount per (already mirror-expanded) weapon def.
//!
//! Plan 02's `UnitTypeDef.init` performs the mirror pass, so `setup_weapons`
//! must **not** mirror again; it only constructs the per-weapon mounts.

use crate::content::registries::units::UnitTypeDef;

pub use crate::weapons::{UnitState, UnitWeapons, WeaponMount};

/// Legacy name for the unit weapon/mount component (`Unit.weapons`).
///
/// Kept as an alias so the plan-11 component vocabulary stays stable; the real
/// storage is plan 10's [`UnitWeapons`].
pub type WeaponsComp = UnitWeapons;

/// Builds the mount array for `unit` (`UnitType.setupWeapons`).
///
/// `unit.weapons` is already mirror-expanded by plan 02's `UnitTypeDef.init`.
/// Mount rotations start at each weapon's relative `baseRotation` (plan-10
/// [`WeaponMount::new`]) and are driven by the plan-10 update pass.
pub fn setup_weapons(unit: &UnitTypeDef, rotation: f32) -> UnitWeapons {
    let mounts = unit.weapons.iter().map(WeaponMount::new).collect();
    let _ = rotation;
    UnitWeapons {
        weapons: unit.weapons.clone(),
        mounts,
        pending: Vec::new(),
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
            assert_eq!(mount.rotation, def.base_rotation);
            assert_eq!(mount.reload, 0.0);
        }
    }

    #[test]
    fn unarmed_content_has_no_mounts() {
        let content = content();
        let unit = content.unit_by_name("block").expect("block");
        assert!(setup_weapons(unit, 0.0).mounts.is_empty());
    }
}
