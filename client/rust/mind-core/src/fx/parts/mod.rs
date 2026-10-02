// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawPart` framework — specs + progressors (plan 17 §3.7).
//!
//! The data-side of `entities/part/*`: [`PartParams`]/[`PartMove`] and the
//! [`PartProgressSpec`] tree. The region/shape/halo/… draw emit is resolved by
//! `mind-gdext` (parts program) and is filled in with the M5 weapons pass; the
//! spec fields below mirror the Java classes so content data is lossless.

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

/// `HaloPart` fields.
#[derive(Clone, Debug, PartialEq)]
pub struct HaloPartSpec {
    /// Color.
    pub color: Rgba,
    /// Color to.
    pub color_to: Rgba,
    /// Radius.
    pub radius: f32,
    /// Radius to.
    pub radius_to: f32,
    /// Stroke.
    pub stroke: f32,
    /// Stroke to.
    pub stroke_to: f32,
    /// Shapes (orbit count).
    pub shapes: i32,
    /// Shape sides.
    pub shape_sides: i32,
    /// Shape radius.
    pub shape_radius: f32,
    /// Shape radius to.
    pub shape_radius_to: f32,
    /// Shape rotate speed.
    pub shape_rotate_speed: f32,
    /// Rotation.
    pub rotation: f32,
    /// Halo rotate speed.
    pub halo_rotate_speed: f32,
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

impl Default for HaloPartSpec {
    fn default() -> Self {
        Self {
            color: Rgba::WHITE,
            color_to: Rgba::WHITE,
            radius: 3.0,
            radius_to: 3.0,
            stroke: 1.0,
            stroke_to: 1.0,
            shapes: 0,
            shape_sides: 0,
            shape_radius: 2.0,
            shape_radius_to: 2.0,
            shape_rotate_speed: 0.0,
            rotation: 0.0,
            halo_rotate_speed: 0.0,
            x: 0.0,
            y: 0.0,
            layer: crate::render::layer::Layer::Effect.z(),
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

/// `EffectSpawnerPart` fields.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectSpawnerPartSpec {
    /// Effect id.
    pub effect: crate::content::EffectId,
    /// X.
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Rotation offset.
    pub rotation: f32,
    /// Interval in ticks.
    pub interval: f32,
    /// Effect lifetime.
    pub effect_life: f32,
    /// Chance.
    pub chance: f32,
    /// Chance delta per tick.
    pub chance_delta: f32,
    /// Mirror the x offset.
    pub mirror: bool,
}

impl Default for EffectSpawnerPartSpec {
    fn default() -> Self {
        Self {
            effect: crate::content::EffectId::NONE,
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            interval: 5.0,
            effect_life: 30.0,
            chance: 1.0,
            chance_delta: 0.0,
            mirror: false,
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
        assert_eq!(EffectSpawnerPartSpec::default().interval, 5.0);
    }
}
