// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/entities/part/{DrawPart,RegionPart,ShapePart,
//         HoverPart,FlarePart}.java (fields only; draw code is plan 17).

//! Unit/weapon draw-part metadata (plan 02 M5).
//!
//! Parts are pure visual data; plan 17 owns `DrawPart` behavior. Only the part
//! kinds used by vanilla `UnitTypes.java` are modelled (`HaloPart`,
//! `EffectSpawnerPart` are unused by units — the kind enum is append-only).

use crate::content::color::Rgba;
use crate::content::registries::pal;

/// Draw-part class tag (`DrawPartKind` content ABI; append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawPartKind {
    /// `RegionPart`.
    RegionPart = 0,
    /// `ShapePart`.
    ShapePart = 1,
    /// `HoverPart`.
    HoverPart = 2,
    /// `FlarePart`.
    FlarePart = 3,
}

impl DrawPartKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            DrawPartKind::RegionPart => "RegionPart",
            DrawPartKind::ShapePart => "ShapePart",
            DrawPartKind::HoverPart => "HoverPart",
            DrawPartKind::FlarePart => "FlarePart",
        }
    }
}

/// `Blending` tag used by `RegionPart.blending`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlendingKind {
    /// `Blending.normal`.
    #[default]
    Normal = 0,
    /// `Blending.additive`.
    Additive = 1,
}

impl BlendingKind {
    /// Java field name.
    pub const fn name(self) -> &'static str {
        match self {
            BlendingKind::Normal => "normal",
            BlendingKind::Additive => "additive",
        }
    }
}

/// `Interp` curve tag (used by `PartProgressSpec::Curve` and bullet trail
/// interpolation; append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InterpKind {
    /// `Interp.linear`.
    Linear = 0,
    /// `Interp.one` (constant 1).
    One = 1,
    /// `Interp.slope`.
    Slope = 2,
    /// `Interp.pow2In`.
    Pow2In = 3,
    /// `Interp.pow5In`.
    Pow5In = 4,
}

impl InterpKind {
    /// Java field name.
    pub const fn name(self) -> &'static str {
        match self {
            InterpKind::Linear => "linear",
            InterpKind::One => "one",
            InterpKind::Slope => "slope",
            InterpKind::Pow2In => "pow2In",
            InterpKind::Pow5In => "pow5In",
        }
    }
}

/// `DrawPart.PartProgress` expression tree (metadata half).
///
/// Mirrors the fluent builders on `PartProgress` (`DrawPart.java:73+`); the
/// `CompatFix.*` lambdas they construct are plan-17 behavior — this captures the
/// expression itself so the data round-trips.
#[derive(Debug, Clone, PartialEq)]
pub enum PartProgressSpec {
    /// `PartProgress.reload`.
    Reload,
    /// `PartProgress.smoothReload`.
    SmoothReload,
    /// `PartProgress.warmup`.
    Warmup,
    /// `PartProgress.charge`.
    Charge,
    /// `PartProgress.recoil`.
    Recoil,
    /// `PartProgress.heat`.
    Heat,
    /// `PartProgress.life`.
    Life,
    /// `PartProgress.time`.
    Time,
    /// `PartProgress.constant(value)`.
    Constant(f32),
    /// `base.inv()`.
    Inv(Box<PartProgressSpec>),
    /// `base.slope()`.
    Slope(Box<PartProgressSpec>),
    /// `base.clamp()`.
    Clamp(Box<PartProgressSpec>),
    /// `base.add(amount)`.
    Add(Box<PartProgressSpec>, f32),
    /// `base.add(other)`.
    AddProgress(Box<PartProgressSpec>, Box<PartProgressSpec>),
    /// `base.delay(amount)`.
    Delay(Box<PartProgressSpec>, f32),
    /// `base.curve(offset, duration)`.
    CurveRange(Box<PartProgressSpec>, f32, f32),
    /// `base.curve(interp)`.
    CurveInterp(Box<PartProgressSpec>, InterpKind),
    /// `base.sustain(offset, grow, sustain)`.
    Sustain(Box<PartProgressSpec>, f32, f32, f32),
    /// `base.shorten(amount)`.
    Shorten(Box<PartProgressSpec>, f32),
    /// `base.compress(start, end)`.
    Compress(Box<PartProgressSpec>, f32, f32),
    /// `base.blend(other, amount)`.
    Blend(Box<PartProgressSpec>, Box<PartProgressSpec>, f32),
    /// `base.mul(other)`.
    MulProgress(Box<PartProgressSpec>, Box<PartProgressSpec>),
    /// `base.mul(amount)`.
    Mul(Box<PartProgressSpec>, f32),
    /// `base.min(other)`.
    Min(Box<PartProgressSpec>, Box<PartProgressSpec>),
    /// `base.sin(offset, scl, mag)`.
    Sin(Box<PartProgressSpec>, f32, f32, f32),
    /// `base.absin(scl, mag)`.
    Absin(Box<PartProgressSpec>, f32, f32),
    /// `base.mod(amount)`.
    Mod(Box<PartProgressSpec>, f32),
    /// `base.loop(time)`.
    Loop(Box<PartProgressSpec>, f32),
    /// `p -> Mathf.absin(Time.time + offset, scl, mag)` (custom time-wave
    /// progress used by the anthicus blades).
    AbsinTime {
        /// Time offset.
        offset: f32,
        /// Period scale.
        scl: f32,
        /// Amplitude.
        mag: f32,
    },
}

/// One entry of `RegionPart.moves` (`DrawPart.PartMove`).
#[derive(Debug, Clone, PartialEq)]
pub struct PartMoveSpec {
    /// Progress source.
    pub progress: PartProgressSpec,
    /// Move offsets.
    pub x: f32,
    /// Move Y offset.
    pub y: f32,
    /// Grow X offset.
    pub gx: f32,
    /// Grow Y offset.
    pub gy: f32,
    /// Rotation offset.
    pub rot: f32,
}

/// Draw-part metadata record.
///
/// The record is the union of the `DrawPart` base fields and the four part
/// kinds' fields; fields that do not apply to a kind keep that kind's class
/// defaults (see [`DrawPartSpec::for_kind`]).
#[derive(Debug, Clone, PartialEq)]
pub struct DrawPartSpec {
    /// Part class tag.
    pub kind: DrawPartKind,
    /// `DrawPart.under` (drawn under the unit).
    pub under: bool,
    /// `DrawPart.turretShading`.
    pub turret_shading: bool,
    /// `DrawPart.weaponIndex`.
    pub weapon_index: i32,
    /// `DrawPart.recoilIndex`.
    pub recoil_index: i32,
    /// `RegionPart.suffix` (region suffix, e.g. `"-blade"`).
    pub suffix: String,
    /// `RegionPart.mirror` (drawn on both sides).
    pub mirror: bool,
    /// `RegionPart.outline`.
    pub outline: bool,
    /// `RegionPart.drawRegion`.
    pub draw_region: bool,
    /// `RegionPart.heatLight`.
    pub heat_light: bool,
    /// Progress source.
    pub progress: PartProgressSpec,
    /// `RegionPart.growProgress`.
    pub grow_progress: PartProgressSpec,
    /// `RegionPart.heatProgress`.
    pub heat_progress: PartProgressSpec,
    /// `RegionPart.blending`.
    pub blending: BlendingKind,
    /// Draw layer (`-1` = default).
    pub layer: f32,
    /// Layer offset.
    pub layer_offset: f32,
    /// `RegionPart.heatLayerOffset`.
    pub heat_layer_offset: f32,
    /// `RegionPart.outlineLayerOffset` (plan 17 additive field).
    pub outline_layer_offset: f32,
    /// `RegionPart.turretHeatLayer` (plan 17 additive field; boolean, uses
    /// `Layer.turretHeat` when set).
    pub turret_heat_layer: bool,
    /// `RegionPart.originX` (plan 17 additive field).
    pub origin_x: f32,
    /// `RegionPart.originY` (plan 17 additive field).
    pub origin_y: f32,
    /// `DrawPart.PartProgress.getClamp` gate (`clampProgress`; plan 17 additive).
    pub clamp_progress: bool,
    /// `RegionPart.replaceOutline` (plan 17 additive field).
    pub replace_outline: bool,
    /// `RegionPart.heatLightOpacity` (plan 17 additive field).
    pub heat_light_opacity: f32,
    /// X offset.
    pub x: f32,
    /// Y offset.
    pub y: f32,
    /// `RegionPart.xScl`.
    pub x_scl: f32,
    /// `RegionPart.yScl`.
    pub y_scl: f32,
    /// Rotation offset.
    pub rotation: f32,
    /// Progress-driven X move.
    pub move_x: f32,
    /// Progress-driven Y move.
    pub move_y: f32,
    /// Progress-driven grow X.
    pub grow_x: f32,
    /// Progress-driven grow Y.
    pub grow_y: f32,
    /// Progress-driven rotation move.
    pub move_rot: f32,
    /// `RegionPart.heatColor` (nullable upstream).
    pub heat_color: Option<Rgba>,
    /// Tint color (`RegionPart.color`/`ShapePart.color`/`HoverPart.color`).
    pub color: Option<Rgba>,
    /// Progress-lerp target color (`colorTo`).
    pub color_to: Option<Rgba>,
    /// `RegionPart.mixColor` (nullable upstream).
    pub mix_color: Option<Rgba>,
    /// `RegionPart.mixColorTo` (nullable upstream).
    pub mix_color_to: Option<Rgba>,
    /// Child parts (`RegionPart.children`).
    pub children: Vec<DrawPartSpec>,
    /// Extra moves (`RegionPart.moves`).
    pub moves: Vec<PartMoveSpec>,
    /// `ShapePart.circle`.
    pub circle: bool,
    /// `ShapePart.hollow`.
    pub hollow: bool,
    /// Side count (`ShapePart.sides`, `HoverPart.sides`, `FlarePart.sides`).
    pub sides: i32,
    /// Radius (`ShapePart.radius`, `HoverPart.radius`, `FlarePart.radius`).
    pub radius: f32,
    /// Progress target radius (`radiusTo`, `<0` = none).
    pub radius_to: f32,
    /// Stroke width.
    pub stroke: f32,
    /// Progress target stroke (`<0` = none).
    pub stroke_to: f32,
    /// `ShapePart.rotateSpeed`.
    pub rotate_speed: f32,
    /// `HoverPart.phase`.
    pub phase: f32,
    /// `HoverPart.minStroke`.
    pub min_stroke: f32,
    /// `HoverPart.circles`.
    pub circles: i32,
    /// `FlarePart.innerScl`.
    pub inner_scl: f32,
    /// `FlarePart.innerRadScl`.
    pub inner_rad_scl: f32,
    /// `FlarePart.rotMove`.
    pub rot_move: f32,
    /// `FlarePart.spinSpeed`.
    pub spin_speed: f32,
    /// `FlarePart.followRotation`.
    pub follow_rotation: bool,
    /// `FlarePart.color1`.
    pub color1: Rgba,
    /// `FlarePart.color2`.
    pub color2: Rgba,
}

impl DrawPartSpec {
    /// Class defaults for `kind` (`part/*.java` field initializers).
    pub fn for_kind(kind: DrawPartKind) -> Self {
        let mut spec = Self {
            kind,
            under: false,
            turret_shading: false,
            weapon_index: 0,
            recoil_index: -1,
            suffix: String::new(),
            mirror: false,
            outline: true,
            draw_region: true,
            heat_light: false,
            progress: PartProgressSpec::Warmup,
            grow_progress: PartProgressSpec::Warmup,
            heat_progress: PartProgressSpec::Heat,
            blending: BlendingKind::Normal,
            layer: -1.0,
            layer_offset: 0.0,
            heat_layer_offset: 1.0,
            outline_layer_offset: -0.001,
            turret_heat_layer: false,
            origin_x: 0.0,
            origin_y: 0.0,
            clamp_progress: true,
            replace_outline: false,
            heat_light_opacity: 0.3,
            x: 0.0,
            y: 0.0,
            x_scl: 1.0,
            y_scl: 1.0,
            rotation: 0.0,
            move_x: 0.0,
            move_y: 0.0,
            grow_x: 0.0,
            grow_y: 0.0,
            move_rot: 0.0,
            heat_color: Some(pal::TURRET_HEAT),
            color: None,
            color_to: None,
            mix_color: None,
            mix_color_to: None,
            children: Vec::new(),
            moves: Vec::new(),
            circle: false,
            hollow: false,
            sides: 3,
            radius: 3.0,
            radius_to: -1.0,
            stroke: 1.0,
            stroke_to: -1.0,
            rotate_speed: 0.0,
            phase: 50.0,
            min_stroke: 0.12,
            circles: 2,
            inner_scl: 0.5,
            inner_rad_scl: 0.33,
            rot_move: 0.0,
            spin_speed: 0.0,
            follow_rotation: false,
            color1: pal::TECH_BLUE,
            color2: Rgba::WHITE,
        };
        match kind {
            DrawPartKind::RegionPart => {}
            DrawPartKind::ShapePart => {
                spec.color = Some(Rgba::WHITE);
            }
            DrawPartKind::HoverPart => {
                spec.radius = 4.0;
                spec.stroke = 3.0;
                spec.sides = 4;
                spec.color = Some(Rgba::WHITE);
            }
            DrawPartKind::FlarePart => {
                spec.sides = 4;
                spec.radius = 100.0;
                spec.stroke = 6.0;
                // `FlarePart.layer = Layer.effect` (110).
                spec.layer = 110.0;
            }
        }
        spec
    }

    /// `new RegionPart(suffix)` with default fields.
    pub fn region(suffix: &str) -> Self {
        let mut spec = Self::for_kind(DrawPartKind::RegionPart);
        spec.suffix = suffix.to_owned();
        spec
    }

    /// `new ShapePart()` with default fields.
    pub fn shape() -> Self {
        Self::for_kind(DrawPartKind::ShapePart)
    }

    /// `new HoverPart()` with default fields.
    pub fn hover() -> Self {
        Self::for_kind(DrawPartKind::HoverPart)
    }

    /// `new FlarePart()` with default fields.
    pub fn flare() -> Self {
        Self::for_kind(DrawPartKind::FlarePart)
    }
}
