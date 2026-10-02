// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawPart` framework — specs + progressors (plan 17 §3.7).
//!
//! The data-side of `entities/part/*`: [`PartParams`]/[`PartMove`] and the
//! [`PartProgressSpec`] tree. The region/shape/halo/… draw emit is resolved by
//! `mind-gdext` (parts program) and is filled in with the M5 weapons pass; the
//! spec fields below mirror the Java classes so content data is lossless.

pub mod draw;
pub mod params;
pub mod progress;

pub use params::{PartMove, PartParams};
pub use progress::{PartFunc, PartProgressSpec};

use crate::content::Rgba;
use crate::render::draw::RegionKey;

/// One draw part (`DrawPart` subtype data).
#[derive(Clone, Debug, PartialEq)]
pub enum PartSpec {
    /// `RegionPart`.
    Region(Box<RegionPartSpec>),
    /// `ShapePart`.
    Shape(ShapePartSpec),
    /// `HaloPart`.
    Halo(HaloPartSpec),
    /// `HoverPart`.
    Hover(HoverPartSpec),
    /// `FlarePart`.
    Flare(FlarePartSpec),
    /// `EffectSpawnerPart`.
    EffectSpawner(EffectSpawnerPartSpec),
}

/// `RegionPart` fields (`entities/part/RegionPart.java`).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct RegionPartSpec {
    /// Region suffix (`""` uses the content region).
    pub suffix: &'static str,
    /// Explicit region name.
    pub name: Option<&'static str>,
    /// Mirror `-r`/`-l`.
    pub mirror: bool,
    /// Draw an outline.
    pub outline: bool,
    /// Replace the outline.
    pub replace_outline: bool,
    /// Draw the region.
    pub draw_region: bool,
    /// Emit a heat light.
    pub heat_light: bool,
    /// Clamp progress.
    pub clamp_progress: bool,
    /// Main progress.
    pub progress: PartProgressSpec,
    /// Grow progress.
    pub grow_progress: PartProgressSpec,
    /// Heat progress.
    pub heat_progress: PartProgressSpec,
    /// Blending (`Additive` for heat).
    pub additive: bool,
    /// Layer.
    pub layer: f32,
    /// Layer offset.
    pub layer_offset: f32,
    /// Heat layer offset.
    pub heat_layer_offset: f32,
    /// Use the turret heat layer.
    pub turret_heat_layer: bool,
    /// Outline layer offset.
    pub outline_layer_offset: f32,
    /// X offset.
    pub x: f32,
    /// Y offset.
    pub y: f32,
    /// X scale.
    pub x_scl: f32,
    /// Y scale.
    pub y_scl: f32,
    /// Rotation.
    pub rotation: f32,
    /// Origin x (`-1` = center).
    pub origin_x: f32,
    /// Origin y.
    pub origin_y: f32,
    /// Move x.
    pub move_x: f32,
    /// Move y.
    pub move_y: f32,
    /// Grow x.
    pub grow_x: f32,
    /// Grow y.
    pub grow_y: f32,
    /// Move rotation.
    pub move_rot: f32,
    /// Heat light opacity.
    pub heat_light_opacity: f32,
    /// Color.
    pub color: Rgba,
    /// Color to.
    pub color_to: Rgba,
    /// Mix color.
    pub mix_color: Option<Rgba>,
    /// Mix color to.
    pub mix_color_to: Option<Rgba>,
    /// Heat color.
    pub heat_color: Rgba,
    /// Child parts.
    pub children: Vec<PartSpec>,
    /// Moves.
    pub moves: Vec<PartMove>,
}

/// `ShapePart` fields.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapePartSpec {
    /// Fill color.
    pub color: Rgba,
    /// Fill color to.
    pub color_to: Rgba,
    /// Circle (`true`) or square.
    pub circle: bool,
    /// Hollow.
    pub hollow: bool,
    /// Sides (poly).
    pub sides: i32,
    /// Radius.
    pub radius: f32,
    /// Radius to.
    pub radius_to: f32,
    /// Stroke.
    pub stroke: f32,
    /// Stroke to.
    pub stroke_to: f32,
    /// Rotation.
    pub rotation: f32,
    /// Rotate speed (deg/tick).
    pub rotate_speed: f32,
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Layer.
    pub layer: f32,
    /// Layer offset.
    pub layer_offset: f32,
    /// Progress.
    pub progress: PartProgressSpec,
    /// Children.
    pub children: Vec<PartSpec>,
}

impl Default for ShapePartSpec {
    fn default() -> Self {
        Self {
            color: Rgba::WHITE,
            color_to: Rgba::WHITE,
            circle: true,
            hollow: false,
            sides: 0,
            radius: 3.0,
            radius_to: 3.0,
            stroke: 1.0,
            stroke_to: 1.0,
            rotation: 0.0,
            rotate_speed: 0.0,
            x: 0.0,
            y: 0.0,
            layer: crate::render::layer::Layer::Effect.z(),
            layer_offset: 0.0,
            progress: PartProgressSpec::Warmup,
            children: Vec::new(),
        }
    }
}

/// `HaloPart` fields (`entities/part/HaloPart.java`).
#[derive(Clone, Debug, PartialEq)]
pub struct HaloPartSpec {
    /// `hollow`.
    pub hollow: bool,
    /// `tri`.
    pub tri: bool,
    /// Shape count (`shapes`).
    pub shapes: i32,
    /// Shape sides (`sides`).
    pub sides: i32,
    /// Shape radius.
    pub radius: f32,
    /// Shape radius target (`<0` = none).
    pub radius_to: f32,
    /// Stroke.
    pub stroke: f32,
    /// Stroke target (`<0` = none).
    pub stroke_to: f32,
    /// Tri length.
    pub tri_length: f32,
    /// Tri length target (`<0` = none).
    pub tri_length_to: f32,
    /// Halo radius.
    pub halo_radius: f32,
    /// Halo radius target (`<0` = none).
    pub halo_radius_to: f32,
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Shape rotation.
    pub shape_rotation: f32,
    /// Move x.
    pub move_x: f32,
    /// Move y.
    pub move_y: f32,
    /// Shape move rotation.
    pub shape_move_rot: f32,
    /// Halo rotate speed.
    pub halo_rotate_speed: f32,
    /// Halo rotation.
    pub halo_rotation: f32,
    /// Shape rotate speed.
    pub rotate_speed: f32,
    /// Color.
    pub color: Rgba,
    /// Color to.
    pub color_to: Option<Rgba>,
    /// Mirror.
    pub mirror: bool,
    /// Clamp progress.
    pub clamp_progress: bool,
    /// Layer.
    pub layer: f32,
    /// Layer offset.
    pub layer_offset: f32,
    /// Progress.
    pub progress: PartProgressSpec,
    /// Children.
    pub children: Vec<PartSpec>,
}

impl Default for HaloPartSpec {
    fn default() -> Self {
        Self {
            hollow: false,
            tri: false,
            shapes: 3,
            sides: 3,
            radius: 3.0,
            radius_to: -1.0,
            stroke: 1.0,
            stroke_to: -1.0,
            tri_length: 1.0,
            tri_length_to: -1.0,
            halo_radius: 10.0,
            halo_radius_to: -1.0,
            x: 0.0,
            y: 0.0,
            shape_rotation: 0.0,
            move_x: 0.0,
            move_y: 0.0,
            shape_move_rot: 0.0,
            halo_rotate_speed: 0.0,
            halo_rotation: 0.0,
            rotate_speed: 0.0,
            color: Rgba::WHITE,
            color_to: None,
            mirror: false,
            clamp_progress: true,
            layer: -1.0,
            layer_offset: 0.0,
            progress: PartProgressSpec::Warmup,
            children: Vec::new(),
        }
    }
}

/// `HoverPart` fields.
#[derive(Clone, Debug, PartialEq)]
pub struct HoverPartSpec {
    /// Radius.
    pub radius: f32,
    /// Stroke.
    pub stroke: f32,
    /// Phase.
    pub phase: f32,
    /// Color.
    pub color: Rgba,
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Layer.
    pub layer: f32,
    /// Progress.
    pub progress: PartProgressSpec,
    /// Children.
    pub children: Vec<PartSpec>,
}

impl Default for HoverPartSpec {
    fn default() -> Self {
        Self {
            radius: 3.0,
            stroke: 1.0,
            phase: 0.0,
            color: Rgba::WHITE,
            x: 0.0,
            y: 0.0,
            layer: crate::render::layer::Layer::Effect.z(),
            progress: PartProgressSpec::Warmup,
            children: Vec::new(),
        }
    }
}

/// `FlarePart` fields.
#[derive(Clone, Debug, PartialEq)]
pub struct FlarePartSpec {
    /// Color 1.
    pub color1: Rgba,
    /// Color 2.
    pub color2: Rgba,
    /// Sides.
    pub sides: i32,
    /// Radius.
    pub radius: f32,
    /// Radius to.
    pub radius_to: f32,
    /// Stroke.
    pub stroke: f32,
    /// Stroke to.
    pub stroke_to: f32,
    /// Inner radius.
    pub inner_radius: f32,
    /// Inner radius to.
    pub inner_radius_to: f32,
    /// Rotation.
    pub rotation: f32,
    /// Rotate speed.
    pub rotate_speed: f32,
    /// Follow rotation.
    pub follow_rotation: bool,
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Layer.
    pub layer: f32,
    /// Progress.
    pub progress: PartProgressSpec,
    /// Children.
    pub children: Vec<PartSpec>,
}

impl Default for FlarePartSpec {
    fn default() -> Self {
        Self {
            color1: Rgba::WHITE,
            color2: Rgba::WHITE,
            sides: 4,
            radius: 3.0,
            radius_to: 3.0,
            stroke: 1.0,
            stroke_to: 1.0,
            inner_radius: 0.0,
            inner_radius_to: 0.0,
            rotation: 0.0,
            rotate_speed: 0.0,
            follow_rotation: false,
            x: 0.0,
            y: 0.0,
            layer: crate::render::layer::Layer::Effect.z(),
            progress: PartProgressSpec::Warmup,
            children: Vec::new(),
        }
    }
}

/// `EffectSpawnerPart` fields (`entities/part/EffectSpawnerPart.java`).
#[derive(Clone, Debug, PartialEq)]
pub struct EffectSpawnerPartSpec {
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Spawn rect width.
    pub width: f32,
    /// Spawn rect height.
    pub height: f32,
    /// Rotation offset.
    pub rotation: f32,
    /// Mirror across the center.
    pub mirror: bool,
    /// Effect rotation offset.
    pub effect_rot: f32,
    /// Effect rotation random range.
    pub effect_rand_rot: f32,
    /// Interval in ticks (`>0` = timed).
    pub effect_interval: f32,
    /// Interval at zero progress (`>0` overrides `effect_interval`).
    pub effect_interval_from: f32,
    /// Spawn chance per tick.
    pub effect_chance: f32,
    /// Effect id.
    pub effect: crate::content::EffectId,
    /// Effect color.
    pub effect_color: Rgba,
    /// Scale chance by progress.
    pub use_progress: bool,
    /// Progress source.
    pub progress: PartProgressSpec,
    /// Debug rect.
    pub debug_draw: bool,
}

impl Default for EffectSpawnerPartSpec {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            rotation: 0.0,
            mirror: false,
            effect_rot: 0.0,
            effect_rand_rot: 0.0,
            effect_interval: 0.0,
            effect_interval_from: 0.0,
            effect_chance: 0.1,
            effect: crate::content::EffectId::NONE,
            effect_color: Rgba::WHITE,
            use_progress: true,
            progress: PartProgressSpec::Warmup,
            debug_draw: false,
        }
    }
}

/// A resolved region name (`load()` output), kept content-name independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedRegion(pub RegionKey);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_default_are_constructible() {
        let region = RegionPartSpec::default();
        assert_eq!(region.progress, PartProgressSpec::Warmup);
        let part = PartSpec::Shape(ShapePartSpec::default());
        assert!(matches!(part, PartSpec::Shape(_)));
        assert_eq!(EffectSpawnerPartSpec::default().effect_chance, 0.1);
        assert_eq!(HaloPartSpec::default().halo_radius, 10.0);
    }
}
