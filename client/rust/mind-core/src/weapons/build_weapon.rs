// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuildWeapon` behavior (`core/src/mindustry/type/weapons/BuildWeapon.java`).
//!
//! Purely visual: aims at the unit's build plan and never shoots. The caller
//! computes the straight-ahead fallback muzzle point (`BuildWeapon.update`).

use super::WeaponMount;

/// Ports the non-rendering half of `BuildWeapon.update`.
///
/// `front` is the fallback aim point straight ahead of the weapon; `plan` is the
/// active build-plan draw position (`unit.buildPlan().drawx/y`, plan 11).
pub fn aim(mount: &mut WeaponMount, front: (f32, f32), plan: Option<(f32, f32)>) {
    mount.shoot = false;
    mount.rotate = true;
    let (x, y) = plan.unwrap_or(front);
    mount.aim_x = x;
    mount.aim_y = y;
}
