// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MineWeapon` behavior (`core/src/mindustry/type/weapons/MineWeapon.java`).
//!
//! Purely visual: aims at the unit's mine tile and never shoots. The caller
//! computes the straight-ahead fallback muzzle point (`MineWeapon.update`).

use super::WeaponMount;

/// Ports the non-rendering half of `MineWeapon.update`.
///
/// `front` is the fallback aim point straight ahead; `mine_tile` is the
/// `Tile.drawx/drawy` of the active mine tile (plan 11).
pub fn aim(mount: &mut WeaponMount, front: (f32, f32), mine_tile: Option<(f32, f32)>) {
    mount.shoot = false;
    mount.rotate = true;
    let (x, y) = mine_tile.unwrap_or(front);
    mount.aim_x = x;
    mount.aim_y = y;
}
