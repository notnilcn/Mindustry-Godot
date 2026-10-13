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

use crate::render::drawf;

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

/// `UnitType.drawShadow` world offset (`UnitType.java:1652-1654,1662`):
/// `(shadowTX * e, shadowTY * e)` with `shadowTX = -12`, `shadowTY = -13`
/// (`:44`) and `e = clamp(elevation, shadowElevation, 1) * shadowElevationScl`
/// (flying units resolve `elevation = 1`, `shadowElevationScl = 1`, so `e = 1`).
pub fn flying_shadow_offset(elevation: f32) -> (f32, f32) {
    let e = elevation.clamp(0.0, 1.0);
    (-12.0 * e, -13.0 * e)
}

/// One unit's view-only state (G03 T5; the port has no sim `hitTime`/`itemTime`).
#[derive(Debug, Clone, Copy, Default)]
pub struct UnitViewState {
    /// `HealthComp.hitTime` view mirror (1 on a health drop, `-delta/9`/tick).
    pub hit_time: f32,
    /// `ItemsComp.itemTime` view mirror (`lerpDelta(.., 0.05)`).
    pub item_time: f32,
    /// Last health seen (drives the hit flash).
    pub last_health: f32,
    /// Whether `last_health` has been seeded (the first frame never flashes).
    pub first: bool,
}

/// Per-frame view advance (`HealthComp.java:9,26,75`, `ItemsComp.java:18`):
/// a health drop sets the 9-tick hit flash (`hitTime = 1`, decay
/// `Time.delta / 9`); `itemTime` lerps toward `has_item` by `lerpDelta(.., 0.05)`.
pub fn advance_view(state: &mut UnitViewState, health: f32, has_item: bool, delta_ticks: f32) {
    state.hit_time = (state.hit_time - delta_ticks / 9.0).max(0.0);
    if !state.first {
        state.first = true;
        state.last_health = health;
    } else if health < state.last_health - 0.001 {
        state.hit_time = 1.0;
        state.last_health = health;
    } else {
        state.last_health = health;
    }
    let target = f32::from(u8::from(has_item));
    let t = 1.0 - (1.0f32 - 0.05).powf(delta_ticks);
    state.item_time += (target - state.item_time) * t;
}

/// `Angles.trnsx/trnsy(angle, x, y)` (rotate a 2D point).
pub fn rotate_point(angle_deg: f32, x: f32, y: f32) -> (f32, f32) {
    let r = angle_deg.to_radians();
    (r.cos() * x - r.sin() * y, r.sin() * x + r.cos() * y)
}

/// `UnitEngine.draw` (`UnitType.java:2018-2050`): `((outer x, y, r),
/// (inner x, y, r))`; `engine` is `(x, y, radius, rotation)`. `None` when the
/// elevation scale is ~0.
pub type EngineCircles = ((f32, f32, f32), (f32, f32, f32));

pub fn engine_circles(
    x: f32,
    y: f32,
    rotation: f32,
    elevation: f32,
    use_elev: bool,
    engine: (f32, f32, f32, f32),
    time: f32,
) -> Option<EngineCircles> {
    let scale = if use_elev { elevation } else { 1.0 };
    if scale <= 0.0001 {
        return None;
    }
    let rot = rotation - 90.0;
    let (ex, ey) = rotate_point(rot, engine.0 * scale, engine.1 * scale);
    let (ex, ey) = (x + ex, y + ey);
    let rad = (engine.2 + drawf::absin(time, 2.0, engine.2 / 4.0)) * scale;
    let (ix, iy) = rotate_point(rot + engine.3, rad / 4.0, 0.0);
    Some(((ex, ey, rad), (ex - ix, ey - iy, rad / 2.0)))
}

/// `UnitType.drawMining` (`UnitType.java:1599-1631`): `(emitter, target)`
/// world points with the `absin` swing and the tile shimmer.
pub fn mine_beam_points(
    x: f32,
    y: f32,
    rotation: f32,
    offset: f32,
    tile: (i16, i16),
    time: f32,
) -> ((f32, f32), (f32, f32)) {
    let focus = offset + drawf::absin(time, 1.1, 0.5);
    let (fx, fy) = drawf::trns(rotation, focus);
    let (tx, ty) = (tile.0 as f32 * 8.0, tile.1 as f32 * 8.0);
    (
        (x + fx, y + fy),
        (
            tx + drawf::sin_scl(time + 48.0, 12.0, 1.0),
            ty + drawf::sin_scl(time + 48.0, 14.0, 1.0),
        ),
    )
}

/// `UnitType.drawMining` beam colour: `lerp(lightGray, white, 0.7 +
/// absin(time, 0.5, 0.3))` in `[f32; 4]`.
pub fn mine_beam_color(time: f32) -> [f32; 4] {
    drawf::lerp(
        drawf::pal::LIGHT_GRAY,
        [1.0, 1.0, 1.0, 1.0],
        0.7 + drawf::absin(time, 0.5, 0.3),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `drawShadow` offsets the silhouette down-left by `(-12, -13) * e`
    /// (`UnitType.java:44,1652-1654`).
    #[test]
    fn flying_shadow_offset_scales_with_elevation() {
        assert_eq!(flying_shadow_offset(1.0), (-12.0, -13.0));
        assert_eq!(flying_shadow_offset(0.0), (0.0, 0.0));
        assert_eq!(flying_shadow_offset(0.5), (-6.0, -6.5));
        // Out-of-range elevations clamp into 0..=1.
        assert_eq!(flying_shadow_offset(2.0), (-12.0, -13.0));
        assert_eq!(flying_shadow_offset(-1.0), (0.0, 0.0));
    }

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

    #[test]
    fn hit_flash_decays_in_nine_ticks() {
        let mut state = UnitViewState::default();
        advance_view(&mut state, 10.0, false, 1.0);
        assert_eq!(state.hit_time, 0.0);
        advance_view(&mut state, 8.0, false, 1.0);
        assert!((state.hit_time - 1.0).abs() < 1e-4, "health drop flashes");
        advance_view(&mut state, 8.0, false, 4.5);
        assert!(
            (state.hit_time - 0.5).abs() < 1e-4,
            "half the flash after 4.5 ticks: {}",
            state.hit_time
        );
        advance_view(&mut state, 8.0, false, 4.5);
        assert_eq!(state.hit_time, 0.0);
    }

    #[test]
    fn engine_circles_alpha_default() {
        // `alpha` default `engineOffset 5`, `engineSize 2.5`; rotation 90 => rot 0.
        let Some((outer, _inner)) =
            engine_circles(10.0, 20.0, 90.0, 1.0, true, (0.0, -5.0, 2.5, -90.0), 0.0)
        else {
            panic!("engine circle");
        };
        assert!((outer.0 - 10.0).abs() < 1e-4);
        assert!(
            (outer.1 - 15.0).abs() < 1e-4,
            "engine sits 5 down: {outer:?}"
        );
        assert!((outer.2 - 2.8125).abs() < 1e-4, "radius pulse: {outer:?}");
    }

    #[test]
    fn mine_beam_target_swing() {
        // time 0: ex = tx*8 + sin(48/12)*1.
        let (_emitter, target) = mine_beam_points(0.0, 0.0, 0.0, 4.0, (130, 48), 0.0);
        assert!((target.0 - (130.0 * 8.0 - 0.756_802_4)).abs() < 1e-3);
        assert!((target.1 - (48.0 * 8.0 - 0.283_626_5)).abs() < 1e-3);
    }
}
