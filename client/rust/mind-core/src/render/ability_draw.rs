// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Ability.draw(Unit)` math for the ability kinds the port draws outside the
//! shield/field passes (L04 T2 / PAR-0366 residual).
//!
//! Three abilities still had no draw half:
//!
//! * `ArmorPlateAbility.draw` (`ArmorPlateAbility.java:36-56`) — the
//!   `<unit>-armor` region through `Shaders.armor`, gated on `warmup > 0.001`
//!   and modulated by `progress = warmup` and the unitarmor scan band
//!   `step(abs(sin(coords.y*3 + time)), 0.9)`.
//! * `UnitSpawnAbility.draw` (`UnitSpawnAbility.java:64-71`) — `Drawf.construct`
//!   of the spawned unit's full icon at the rotated spawn offset, with
//!   `progress = timer / spawnTime` and `time = timer`.
//! * `MoveLightningAbility.draw` (`MoveLightningAbility.java:89-98`) — the
//!   `heatRegion` sprite through additive blending, alpha `scl / 2` where
//!   `scl = clamp((|vel| - minSpeed) / (maxSpeed - minSpeed))`, jittered by
//!   `Mathf.range(scl / 2)` on both axes.
//!
//! Everything here is pure so the curves are unit-testable without an engine;
//! `mind-gdext::render` owns the region draws. `Shaders.armor`/`Shaders.build`
//! are Godot shaders, so the render pass consumes the uniforms computed here
//! (`armor_shader_time`/`construct_shader_time`) rather than re-deriving them.

use crate::content::Rgba;
use crate::content::registries::pal;

use super::drawf;

/// `ArmorPlateAbility.color` (`ArmorPlateAbility.java:15`,
/// `Color.valueOf("d1efff")` → `d1efffff`; the same value as `Pal.regen` in
/// [`super::drawf::pal`], which is not re-exported through the content
/// palette).
pub const ARMOR_PLATE_COLOR: Rgba = Rgba::from_rgba8888(0xd1efffff);
/// `ArmorPlateAbility.draw` gate (`ArmorPlateAbility.java:37`).
pub const ARMOR_PLATE_MIN_WARMUP: f32 = 0.001;
/// `ArmorPlateAbility.z` (`ArmorPlateAbility.java:18`, `Layer.effect`).
pub const ARMOR_PLATE_DEFAULT_LAYER: f32 = crate::render::layer::Layer::Effect.z();
/// `MoveLightningAbility.color` (`MoveLightningAbility.java:24`,
/// `Color.valueOf("a9d8ff")`; the same value as `Pal.lancerLaser`).
pub const MOVE_LIGHTNING_COLOR: Rgba = pal::LANCER_LASER;
/// `MoveLightningAbility.heatRegion` default (`MoveLightningAbility.java:32`).
pub const MOVE_LIGHTNING_HEAT_REGION: &str = "error";
/// `MoveLightningAbility.draw` gate (`MoveLightningAbility.java:92`).
pub const MOVE_LIGHTNING_MIN_SCL: f32 = 0.00001;
/// `MoveLightningAbility` constructor defaults (`:14-25`) for the fields the
/// port's [`crate::content::registries::units::ability::AbilitySpec`] does not
/// carry (the kind is mod-only).
pub const MOVE_LIGHTNING_DEFAULT_MIN_SPEED: f32 = 0.8;
/// `MoveLightningAbility.maxSpeed` default (`:19`).
pub const MOVE_LIGHTNING_DEFAULT_MAX_SPEED: f32 = 1.2;
/// `MoveLightningAbility.y` default (`:21`); the ability spawn offset is read
/// off `AbilitySpec::spawn_offset`.
pub const MOVE_LIGHTNING_DEFAULT_OFFSET: f32 = 0.0;

/// `MoveLightningAbility.draw` speed scale:
/// `clamp((unit.vel().len() - minSpeed) / (maxSpeed - minSpeed))`
/// (`MoveLightningAbility.java:90`).
///
/// `minSpeed >= maxSpeed` would divide by zero upstream; the port clamps to
/// the `0`/`1` endpoints instead of producing NaN.
pub fn move_lightning_scale(vel_len: f32, min_speed: f32, max_speed: f32) -> f32 {
    let span = max_speed - min_speed;
    if span <= 0.0 {
        return if vel_len >= max_speed { 1.0 } else { 0.0 };
    }
    ((vel_len - min_speed) / span).clamp(0.0, 1.0)
}

/// `Draw.alpha(scl / 2f)` (`MoveLightningAbility.java:94`).
pub fn move_lightning_alpha(scl: f32) -> f32 {
    scl / 2.0
}

/// `Mathf.range(range)` mapped through a `[0, 1)` roll: a value in
/// `[-range, range)` (`Mathf.java:range`). `MoveLightningAbility.draw` jitters
/// both axes by `Mathf.range(scl / 2f)` (`:106`), which is
/// `move_lightning_jitter(scl, ..)`.
pub fn range_from_roll(roll: f32, range: f32) -> f32 {
    (roll * 2.0 - 1.0) * range
}

/// `(Mathf.range(scl / 2f), Mathf.range(scl / 2f))` for the heat-sprite
/// jitter (`MoveLightningAbility.java:96`).
pub fn move_lightning_jitter(scl: f32, roll_x: f32, roll_y: f32) -> (f32, f32) {
    let range = scl / 2.0;
    (
        range_from_roll(roll_x, range),
        range_from_roll(roll_y, range),
    )
}

/// `ArmorPlateAbility.draw` gate + alpha: `warmup > 0.001f` draws the shell,
/// through `Shaders.armor.progress = warmup` (`ArmorPlateAbility.java:36-54`).
pub fn armor_plate_alpha(warmup: f32) -> Option<f32> {
    (warmup > ARMOR_PLATE_MIN_WARMUP).then_some(warmup)
}

/// `Shaders.armor.time = -Time.time / 20f` (`ArmorPlateAbility.java:45`).
pub fn armor_shader_time(time: f32) -> f32 {
    -time / 20.0
}

/// `unitarmor.frag` scan band:
/// `step(abs(sin(coords.y * 3.0 + u_time)), 0.9)`.
///
/// `coords_y` is the region-local normalized V in `[0, 1]`; the band moves
/// with `u_time` (`armor_shader_time`). The fragment multiplies the sampled
/// alpha by this mask, so `0.0` drops the strip.
pub fn armor_scanline(coords_y: f32, time: f32) -> f32 {
    if (coords_y * 3.0 + time).sin().abs() <= 0.9 {
        1.0
    } else {
        0.0
    }
}

/// `ArmorPlateAbility.draw` region chain: `Core.atlas.find(unit.type.name +
/// "-armor", unit.type.region)` (`ArmorPlateAbility.java:39`). The port's
/// body region name is the unit name (see `render/units.rs`), so the fallback
/// repeats it.
pub fn armor_region_candidates(unit_region: &str) -> [String; 2] {
    [format!("{unit_region}-armor"), unit_region.to_owned()]
}

/// `UnitSpawnAbility.draw` spawn offset
/// (`UnitSpawnAbility.java:67`): `Angles.trnsx(unit.rotation, spawnY, spawnX)`
/// — note the swapped `(y, x)` argument order, so `spawnY` is the forward
/// offset and `spawnX` the lateral one.
pub fn unit_spawn_offset(rotation: f32, spawn_x: f32, spawn_y: f32) -> (f32, f32) {
    (
        crate::fx::angles::trnsx_vec(rotation, spawn_y, spawn_x),
        crate::fx::angles::trnsy_vec(rotation, spawn_y, spawn_x),
    )
}

/// `Drawf.construct(.., timer / spawnTime, 1f, timer)`
/// (`UnitSpawnAbility.java:68`): the icon quad's `Shaders.build.progress`.
///
/// A non-positive `spawnTime` has no upstream meaning (the constructor default
/// is 60); the port reports a finished construct rather than `NaN`.
pub fn unit_spawn_progress(timer: f32, spawn_time: f32) -> f32 {
    if spawn_time <= 0.0 {
        1.0
    } else {
        (timer / spawn_time).clamp(0.0, 1.0)
    }
}

/// `Drawf.construct`'s `Shaders.build.time = -time / 20f`
/// (`Drawf.java:445`), with `time = timer` for `UnitSpawnAbility`.
pub fn construct_shader_time(time: f32) -> f32 {
    -time / 20.0
}

/// `Drawf.construct`'s `u_color` tint: `Pal.accent` at `alpha = 1f`
/// (`UnitSpawnAbility.java:68` passes the default `alpha = 1f`).
pub fn construct_color() -> Rgba {
    pal::ACCENT
}

/// The spawned unit's `fullIcon` region the construct draws (the port's
/// `UnitType` full-icon naming used by `unit_build.rs`).
pub fn unit_full_icon_region(name: &str) -> String {
    format!("unit-{name}-full")
}

/// `MoveLightningAbility.draw`'s velocity gate: the sprite/haze is invisible
/// until the unit outruns `minSpeed` (`:103-104`). Kept separate from
/// [`move_lightning_scale`] so the pass can skip the region lookup entirely.
pub fn move_lightning_visible(scl: f32) -> bool {
    scl > MOVE_LIGHTNING_MIN_SCL
}

/// `Mathf.absin` re-export so the render pass and the tests share one formula
/// (used by the field abilities' blink curves; kept here for the ability-draw
/// module's callers).
pub fn absin(x: f32, scl: f32, mag: f32) -> f32 {
    drawf::absin(x, scl, mag)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `MoveLightningAbility.draw`: the scale saturates at `maxSpeed`, is zero
    /// at `minSpeed`, and half-way exactly between them (`:101`).
    #[test]
    fn move_lightning_scale_is_the_upstream_clamp() {
        assert_eq!(move_lightning_scale(0.0, 0.8, 1.2), 0.0);
        assert_eq!(move_lightning_scale(0.8, 0.8, 1.2), 0.0);
        assert!((move_lightning_scale(1.0, 0.8, 1.2) - 0.5).abs() < 1e-6);
        assert_eq!(move_lightning_scale(1.2, 0.8, 1.2), 1.0);
        assert_eq!(move_lightning_scale(9.0, 0.8, 1.2), 1.0);
        // Degenerate span (mod override) must not divide by zero.
        assert_eq!(move_lightning_scale(2.0, 1.0, 1.0), 1.0);
        assert_eq!(move_lightning_scale(0.5, 1.0, 1.0), 0.0);
    }

    /// `Draw.alpha(scl / 2f)` (`MoveLightningAbility.java:94`).
    #[test]
    fn move_lightning_alpha_is_half_the_scale() {
        assert_eq!(move_lightning_alpha(0.0), 0.0);
        assert!((move_lightning_alpha(1.0) - 0.5).abs() < 1e-6);
        assert!((move_lightning_alpha(0.4) - 0.2).abs() < 1e-6);
    }

    /// `Mathf.range(x)` spans `[-x, x)` and is symmetric about zero.
    #[test]
    fn lightning_jitter_matches_mathf_range() {
        assert!((range_from_roll(0.0, 4.0) + 4.0).abs() < 1e-6);
        assert!(range_from_roll(0.5, 4.0).abs() < 1e-6);
        assert!((range_from_roll(1.0, 4.0) - 4.0).abs() < 1e-6);
        // `scl = 1` -> range = 0.5 on both axes.
        let (jx, jy) = move_lightning_jitter(1.0, 0.0, 1.0);
        assert!((jx + 0.5).abs() < 1e-6);
        assert!((jy - 0.5).abs() < 1e-6);
        // The gate is `scl > 0.00001f` (`:104`).
        assert!(!move_lightning_visible(0.000_01));
        assert!(move_lightning_visible(0.000_02));
    }

    /// `ArmorPlateAbility.draw` only draws above `warmup > 0.001f` and uses
    /// `warmup` as the shader progress/alpha (`:46-52`).
    #[test]
    fn armor_plate_alpha_gate_and_progress() {
        assert_eq!(armor_plate_alpha(0.0), None);
        assert_eq!(armor_plate_alpha(0.001), None);
        assert_eq!(armor_plate_alpha(0.0011), Some(0.0011));
        assert_eq!(armor_plate_alpha(0.5), Some(0.5));
        assert_eq!(armor_plate_alpha(1.0), Some(1.0));
    }

    /// `Shaders.armor.time = -Time.time / 20f` (`ArmorPlateAbility.java:45`).
    #[test]
    fn armor_shader_time_is_negated_twentieths() {
        assert_eq!(armor_shader_time(0.0), 0.0);
        assert!((armor_shader_time(20.0) + 1.0).abs() < 1e-6);
        assert!((armor_shader_time(100.0) + 5.0).abs() < 1e-6);
    }

    /// `unitarmor.frag`: `step(abs(sin(coords.y*3 + time)), 0.9)` — a solid
    /// band except the band where `|sin| > 0.9` (`coords.y*3+time` near
    /// `PI/2`).
    #[test]
    fn armor_scanline_band_follows_time() {
        // y = 0 at t = 0: sin(0) = 0 -> visible.
        assert_eq!(armor_scanline(0.0, 0.0), 1.0);
        // y = 1: sin(3) = 0.141 -> visible at t = 0.
        assert_eq!(armor_scanline(1.0, 0.0), 1.0);
        // y = PI/6: sin(PI/2) = 1 -> hidden.
        assert_eq!(armor_scanline(std::f32::consts::FRAC_PI_6, 0.0), 0.0);
        // The same y becomes visible once the time shifts sin out of the band.
        let shifted = std::f32::consts::FRAC_PI_6;
        assert_eq!(armor_scanline(shifted, std::f32::consts::FRAC_PI_2), 1.0);
        // `time` here is the raw `u_time` (already `-Time.time/20`).
        assert_eq!(armor_scanline(0.0, armor_shader_time(0.0)), 1.0);
    }

    /// `Core.atlas.find(name + "-armor", unit.type.region)` (`:48-50`).
    #[test]
    fn armor_region_falls_back_to_the_body() {
        assert_eq!(
            armor_region_candidates("omura"),
            ["omura-armor".to_owned(), "omura".to_owned()]
        );
    }

    /// The armor plate colour is Java's `d1efff` (`:18`).
    #[test]
    fn armor_plate_color_is_d1efff() {
        assert_eq!(ARMOR_PLATE_COLOR, Rgba::from_rgba8888(0xd1efffff));
        assert!((ARMOR_PLATE_COLOR.r - 0xd1 as f32 / 255.0).abs() < 1e-6);
        assert!((ARMOR_PLATE_COLOR.g - 0xef as f32 / 255.0).abs() < 1e-6);
        assert!((ARMOR_PLATE_COLOR.b - 0xff as f32 / 255.0).abs() < 1e-6);
        assert_eq!(ARMOR_PLATE_COLOR.a, 1.0);
        // Same value as `Pal.regen` (`render::drawf::pal`).
        assert_eq!(
            [
                (ARMOR_PLATE_COLOR.r * 255.0).round() as u32,
                (ARMOR_PLATE_COLOR.g * 255.0).round() as u32,
                (ARMOR_PLATE_COLOR.b * 255.0).round() as u32,
            ],
            [0xd1, 0xef, 0xff]
        );
    }

    /// `UnitSpawnAbility.draw`'s offset uses `Angles.trnsx(rotation, spawnY,
    /// spawnX)` (`:67`): at rotation 0 the offset is `(spawnY, spawnX)`.
    #[test]
    fn unit_spawn_offset_swaps_axes() {
        let (dx, dy) = unit_spawn_offset(0.0, 19.25, -31.75);
        assert!((dx + 31.75).abs() < 1e-4, "dx {dx}");
        assert!((dy - 19.25).abs() < 1e-4, "dy {dy}");

        // At rotation 90 the lateral axis turns into -x.
        let (dx, dy) = unit_spawn_offset(90.0, 19.25, 0.0);
        assert!((dx + 19.25).abs() < 1e-4, "dx {dx}");
        assert!(dy.abs() < 1e-4, "dy {dy}");

        // Forward offset is spawnY: at rotation 90 that is +y.
        let (dx, dy) = unit_spawn_offset(90.0, 0.0, -31.75);
        assert!(dx.abs() < 1e-4, "dx {dx}");
        assert!((dy + 31.75).abs() < 1e-4, "dy {dy}");
    }

    /// `Drawf.construct(.., timer / spawnTime, ..)` (`:71`).
    #[test]
    fn unit_spawn_progress_is_the_timer_fraction() {
        assert_eq!(unit_spawn_progress(0.0, 60.0), 0.0);
        assert!((unit_spawn_progress(30.0, 60.0) - 0.5).abs() < 1e-6);
        assert!((unit_spawn_progress(60.0, 60.0) - 1.0).abs() < 1e-6);
        // Degenerate spawn time: finished rather than NaN.
        assert_eq!(unit_spawn_progress(5.0, 0.0), 1.0);
    }

    /// `Drawf.construct`'s time uniform (`Drawf.java:445`) and the accent
    /// tint (`UnitSpawnAbility.java:68`).
    #[test]
    fn construct_uniforms_match_drawf() {
        assert!((construct_shader_time(0.0)).abs() < 1e-6);
        assert!((construct_shader_time(20.0) + 1.0).abs() < 1e-6);
        assert_eq!(construct_color(), pal::ACCENT);
        assert_eq!(unit_full_icon_region("flare"), "unit-flare-full");
    }

    /// MoveLightning's default colour is `a9d8ff` (`:23`).
    #[test]
    fn move_lightning_default_color() {
        assert_eq!(MOVE_LIGHTNING_COLOR, pal::LANCER_LASER);
        assert_eq!(MOVE_LIGHTNING_HEAT_REGION, "error");
        assert_eq!(MOVE_LIGHTNING_DEFAULT_MIN_SPEED, 0.8);
        assert_eq!(MOVE_LIGHTNING_DEFAULT_MAX_SPEED, 1.2);
    }
}
