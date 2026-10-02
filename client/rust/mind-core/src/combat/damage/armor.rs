// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Armor math (`core/src/mindustry/entities/Damage.java` `applyArmor`).
//!
//! `Damage.applyArmor(damage, armor)` returns
//! `max(damage - armor, damage * minArmorDamage)` where `minArmorDamage = 0.15`.

/// Fraction of raw damage that always gets through armor (`Damage.minArmorDamage`).
pub const MIN_ARMOR_DAMAGE: f32 = 0.15;

/// Applies armor to incoming damage (`Damage.applyArmor`).
#[inline]
pub fn apply_armor(damage: f32, armor: f32) -> f32 {
    (damage - armor).max(damage * MIN_ARMOR_DAMAGE)
}

/// Applies armor unless the source pierces it.
#[inline]
pub fn apply_armor_opt(damage: f32, armor: f32, pierce_armor: bool) -> f32 {
    if pierce_armor {
        damage
    } else {
        apply_armor(damage, armor)
    }
}

/// `Damage.damageArmorMult(damage)` (`damage > 0` scale guard).
#[inline]
pub fn damage_armor_mult(damage: f32) -> f32 {
    if damage > 0.0 { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn armor_subtracts_but_keeps_floor() {
        assert!(close(apply_armor(100.0, 10.0), 90.0));
        // 0.15 * 100 = 15 > 100 - 200 = -100.
        assert!(close(apply_armor(100.0, 200.0), 15.0));
        // Floor still passes through a fraction of the raw damage.
        assert!(close(apply_armor(20.0, 17.0), 3.0));
    }

    #[test]
    fn pierce_armor_bypasses() {
        assert!(close(apply_armor_opt(100.0, 200.0, true), 100.0));
        assert!(close(apply_armor_opt(100.0, 200.0, false), 15.0));
    }
}
