// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit draw math (`UnitType.drawMech`/`drawCell`; parity plan §3.4).
//!
//! `UnitType.java:1939-1970` `drawMech`:
//!
//! ```java
//! float e = unit.elevation;
//! float sin = Mathf.lerp(Mathf.sin(mech.walkExtend(true), 2f/Mathf.PI, 1f), 0f, e);
//! float extension = Mathf.lerp(mech.walkExtend(false), 0, e);
//! float boostTrns = e * 2f;
//! for(int i : Mathf.signs){          // {-1, 1}
//!     Draw.rect(legRegion,
//!         x + Angles.trnsx(baseRotation, extension*i - boostTrns, -boostTrns*i),
//!         y + Angles.trnsy(baseRotation, extension*i - boostTrns, -boostTrns*i),
//!         legRegion.width  * legRegion.scl() * i,
//!         legRegion.height * legRegion.scl() * (1 - Math.max(-sin*i, 0)*0.5f),
//!         baseRotation - 90 + 35f*i*e);
//! }
//! ```
//!
//! `MechComp.walkExtend` (`MechComp.java:75-88`) drives the walk cycle;
//! `UnitType.cellColor` (`:1796-1799`) is `black.lerp(team.color, f +
//! Mathf.absin(Time.time, max(f*5, 1), 1-f))` with Arc's
//! `absin(in, scl, mag) = (sin(in/(scl*2))*mag + mag)/2`.

/// `MechComp.walkExtend` (`MechComp.java:75-88`).
///
/// `raw = walkTime % (stride * 4)`; scaled divides by the stride, otherwise the
/// raw cycle is folded into `-stride..=stride`.
pub fn mech_walk_extend(walk_time: f32, stride: f32, scaled: bool) -> f32 {
    if stride <= 0.0 {
        return 0.0;
    }
    let mut raw = walk_time % (stride * 4.0);
    if scaled {
        return raw / stride;
    }
    if raw > stride * 3.0 {
        raw -= stride * 4.0;
    } else if raw > stride {
        // Java splits `raw > stride*2` and `raw > stride` into two branches
        // with the same body; folded here (clippy::if_same_then_else).
        raw = stride * 2.0 - raw;
    }
    raw
}

/// `Mathf.sin(radians, scl, mag)` (Arc: `sin(radians / scl) * mag`).
fn sin_scaled(radians: f32, scl: f32, mag: f32) -> f32 {
    (radians / scl).sin() * mag
}

/// `Mathf.absin(in, scl, mag)` (Arc: `(sin(in, scl*2, mag) + mag) / 2`).
fn absin(radians: f32, scl: f32, mag: f32) -> f32 {
    (sin_scaled(radians, scl * 2.0, mag) + mag) / 2.0
}

/// `Angles.trnsx/trnsy` — rotate `(x, y)` by `angle` degrees.
fn trns(angle_deg: f32, x: f32, y: f32) -> (f32, f32) {
    let (sin, cos) = angle_deg.to_radians().sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}

/// One mech-leg transform: `(dx, dy, sx, sy, rotation_deg)` for the leg sprite
/// (`UnitType.drawMech` loop body). `sx` is negative for the left leg, which
/// mirrors the sprite.
pub fn mech_leg_transform(
    base_rot: f32,
    extension: f32,
    sin: f32,
    elevation: f32,
    i: f32,
) -> (f32, f32, f32, f32, f32) {
    let boost = elevation * 2.0;
    let (dx, dy) = trns(base_rot, extension * i - boost, -boost * i);
    let sx = i;
    let sy = 1.0 - (-sin * i).max(0.0) * 0.5;
    let rot = base_rot - 90.0 + 35.0 * i * elevation;
    (dx, dy, sx, sy, rot)
}

/// The two mech-leg transforms (`Mathf.signs = {-1, 1}`) from the live mech
/// state. `elevation` is `UnitCore.elevation`.
pub fn mech_leg_transforms(
    base_rot: f32,
    walk_time: f32,
    stride: f32,
    elevation: f32,
) -> [(f32, f32, f32, f32, f32); 2] {
    let e = elevation.clamp(0.0, 1.0);
    let sin = lerp(
        sin_scaled(
            mech_walk_extend(walk_time, stride, true),
            2.0 / std::f32::consts::PI,
            1.0,
        ),
        0.0,
        e,
    );
    let extension = lerp(mech_walk_extend(walk_time, stride, false), 0.0, e);
    [
        mech_leg_transform(base_rot, extension, sin, e, -1.0),
        mech_leg_transform(base_rot, extension, sin, e, 1.0),
    ]
}

/// `Mathf.lerp(from, to, progress)`.
fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

/// `Weapon.draw` mount placement (`Weapon.java:218-228`): the world position and
/// sprite rotation of a weapon at unit-relative `(x, y)`. `recoil` is folded in
/// as the Java `trns(weaponRotation, 0, -recoil)` term.
#[allow(clippy::too_many_arguments)]
pub fn weapon_mount_transform(
    unit_x: f32,
    unit_y: f32,
    unit_rotation: f32,
    x: f32,
    y: f32,
    rotate: bool,
    mount_rotation: f32,
    base_rotation: f32,
    recoil: f32,
) -> (f32, f32, f32) {
    let rotation = unit_rotation - 90.0;
    let weapon_rotation = rotation
        + if rotate {
            mount_rotation
        } else {
            base_rotation
        };
    let (ox, oy) = trns(rotation, x, y);
    let (rx, ry) = trns(weapon_rotation, 0.0, -recoil);
    (unit_x + ox + rx, unit_y + oy + ry, weapon_rotation)
}

/// `UnitType.cellColor`: `black.lerp(team.color, f + absin(time, max(f*5, 1),
/// 1 - f))`, RGBA8888 out.
pub fn cell_color(team_rgba: u32, healthf: f32, time: f32) -> u32 {
    let f = healthf.clamp(0.0, 1.0);
    let t = (f + absin(time, (f * 5.0).max(1.0), 1.0 - f)).clamp(0.0, 1.0);
    let channel = |shift: u32| {
        let value = ((team_rgba >> shift) & 0xff) as f32 / 255.0;
        (value * t * 255.0 + 0.5) as u32
    };
    (channel(24) << 24) | (channel(16) << 16) | (channel(8) << 8) | 0xff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_extend_folds_and_scales() {
        // Java: stride 8 -> 0, 4, 8, 8, 0, -8 over walkTime 0,4,8,12,16,20.
        assert_eq!(mech_walk_extend(0.0, 8.0, false), 0.0);
        assert_eq!(mech_walk_extend(4.0, 8.0, false), 4.0);
        assert_eq!(mech_walk_extend(8.0, 8.0, false), 8.0);
        assert_eq!(mech_walk_extend(12.0, 8.0, false), 4.0);
        assert_eq!(mech_walk_extend(20.0, 8.0, false), -4.0);
        assert_eq!(mech_walk_extend(28.0, 8.0, true), 3.5);
    }

    #[test]
    fn legs_start_at_the_centre() {
        // walkTime 0 -> extension 0, sin 0: both legs sit on the unit centre,
        // mirrored in x (`sx = i`), full height, `baseRotation - 90`.
        let legs = mech_leg_transforms(90.0, 0.0, 8.0, 0.0);
        for (leg, i) in legs.iter().zip([-1.0f32, 1.0]) {
            assert!(leg.0.abs() < 1e-5, "dx {}", leg.0);
            assert!(leg.1.abs() < 1e-5, "dy {}", leg.1);
            assert_eq!(leg.2, i);
            assert_eq!(leg.3, 1.0);
            assert_eq!(leg.4, 0.0);
        }
    }

    #[test]
    fn extension_moves_legs_along_the_rotation_axis() {
        // At stride 8 the cycle peaks at walkTime 8 (raw 8): extension 8 along
        // `baseRotation` (90deg -> +y), legs opposite per sign.
        let legs = mech_leg_transforms(90.0, 8.0, 8.0, 0.0);
        assert!(legs[0].0.abs() < 1e-4 && (legs[0].1 + 8.0).abs() < 1e-4);
        assert!(legs[1].0.abs() < 1e-4 && (legs[1].1 - 8.0).abs() < 1e-4);
    }

    #[test]
    fn flying_mechs_boost_and_pin_legs() {
        // e = 1 drops sin/extension to 0 and adds the boost translation
        // (-boostTrns*i on y for rotation 0) plus the 35deg spread.
        let legs = mech_leg_transforms(0.0, 8.0, 8.0, 1.0);
        assert!((legs[0].1 - 2.0).abs() < 1e-4, "leg0 dy {}", legs[0].1);
        assert!((legs[1].1 + 2.0).abs() < 1e-4, "leg1 dy {}", legs[1].1);
        assert!((legs[0].4 + 125.0).abs() < 1e-4, "leg0 rot {}", legs[0].4);
        assert!((legs[1].4 + 55.0).abs() < 1e-4, "leg1 rot {}", legs[1].4);
    }

    #[test]
    fn weapon_mount_rotates_with_the_body() {
        // Unit facing +x (rotation 90 -> sprite rotation 0): a weapon at
        // (x=4, y=0) sits 4 units ahead; a rotating mount aims with `-90`.
        let (wx, wy, wr) =
            weapon_mount_transform(100.0, 200.0, 90.0, 4.0, 0.0, true, -90.0, 0.0, 0.0);
        assert!((wx - 104.0).abs() < 1e-4 && (wy - 200.0).abs() < 1e-4);
        assert!((wr + 90.0).abs() < 1e-4);
    }

    #[test]
    fn full_health_cell_is_the_team_colour() {
        // f = 1 -> absin amplitude 0 -> lerp factor 1 -> pure team colour.
        assert_eq!(cell_color(0x4a7ba6ff, 1.0, 12.34), 0x4a7ba6ff);
    }

    #[test]
    fn zero_health_cell_pulses_between_black_and_team() {
        // f = 0 -> `t = absin(time, 1, 1)`: time 0 -> (sin(0) + 1)/2 = 0.5,
        // so a white team gives a half-intensity grey.
        assert_eq!(cell_color(0xffffffff, 0.0, 0.0), 0x808080ff);
    }
}
