// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/world/draw/*.java (fields only; draw bodies are
//         plan 17).

//! Block drawer metadata (`Block.drawer` JSON field, plan 20 M2b).
//!
//! Upstream every block owns a `DrawBlock` tree (`ContentParser` `DrawBlock`
//! class parser, `ContentParser.java:182-198`): a class name string, an array
//! shorthand for `DrawMulti`, or an object with a `type` (default
//! `DrawDefault`) and reflective fields. The renderer body is plan 17; this
//! module keeps the parsed tree so the data survives mod loading and the
//! renderer can consume it later.
//!
//! Only the fields the ported drawer models use are captured. Upstream
//! `ignoreUnknownFields = true`; the parser accepts (and drops) the rest.

use crate::content::color::Rgba;
use crate::content::registries::units::parts::DrawPartSpec;

/// Java drawer class tag (`world/draw/*`; append-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DrawerKind {
    /// `DrawBlock` (the class base; behaves like `DrawDefault`).
    Block = 0,
    /// `DrawDefault`.
    #[default]
    Default = 1,
    /// `DrawMulti`.
    Multi = 2,
    /// `DrawArcSmelt`.
    ArcSmelt = 3,
    /// `DrawBubbles`.
    Bubbles = 4,
    /// `DrawCells`.
    Cells = 5,
    /// `DrawCircles`.
    Circles = 6,
    /// `DrawCrucibleFlame`.
    CrucibleFlame = 7,
    /// `DrawCultivator`.
    Cultivator = 8,
    /// `DrawFade`.
    Fade = 9,
    /// `DrawFlame`.
    Flame = 10,
    /// `DrawFrames`.
    Frames = 11,
    /// `DrawHeatInput`.
    HeatInput = 12,
    /// `DrawHeatOutput`.
    HeatOutput = 13,
    /// `DrawHeatRegion`.
    HeatRegion = 14,
    /// `DrawLiquidOutputs`.
    LiquidOutputs = 15,
    /// `DrawLiquidRegion`.
    LiquidRegion = 16,
    /// `DrawLiquidTile`.
    LiquidTile = 17,
    /// `DrawMultiWeave`.
    MultiWeave = 18,
    /// `DrawParticles`.
    Particles = 19,
    /// `DrawPistons`.
    Pistons = 20,
    /// `DrawPlasma`.
    Plasma = 21,
    /// `DrawPower`.
    Power = 22,
    /// `DrawPulseShape`.
    PulseShape = 23,
    /// `DrawPumpLiquid`.
    PumpLiquid = 24,
    /// `DrawSoftParticles`.
    SoftParticles = 25,
    /// `DrawSpikes`.
    Spikes = 26,
    /// `DrawWeave`.
    Weave = 27,
    /// `DrawBlockParts`.
    BlockParts = 28,
    /// `DrawRegion`.
    Region = 29,
    /// `DrawRegionSpin`.
    RegionSpin = 30,
    /// `DrawTurret`.
    Turret = 31,
    /// `DrawWeapon`.
    Weapon = 32,
    /// `DrawSideRegion`.
    SideRegion = 33,
    /// `DrawGlowRegion`.
    GlowRegion = 34,
    /// `DrawArc`.
    Arc = 35,
    /// `DrawBlurSpin`.
    BlurSpin = 36,
    /// `DrawParticle`.
    Particle = 37,
    /// `DrawShape`.
    Shape = 38,
    /// `DrawSoftShadow`.
    SoftShadow = 39,
    /// `DrawWarmupRegion`.
    WarmupRegion = 40,
}

impl DrawerKind {
    /// Java class name.
    pub const fn name(self) -> &'static str {
        match self {
            DrawerKind::Block => "DrawBlock",
            DrawerKind::Default => "DrawDefault",
            DrawerKind::Multi => "DrawMulti",
            DrawerKind::ArcSmelt => "DrawArcSmelt",
            DrawerKind::Bubbles => "DrawBubbles",
            DrawerKind::Cells => "DrawCells",
            DrawerKind::Circles => "DrawCircles",
            DrawerKind::CrucibleFlame => "DrawCrucibleFlame",
            DrawerKind::Cultivator => "DrawCultivator",
            DrawerKind::Fade => "DrawFade",
            DrawerKind::Flame => "DrawFlame",
            DrawerKind::Frames => "DrawFrames",
            DrawerKind::HeatInput => "DrawHeatInput",
            DrawerKind::HeatOutput => "DrawHeatOutput",
            DrawerKind::HeatRegion => "DrawHeatRegion",
            DrawerKind::LiquidOutputs => "DrawLiquidOutputs",
            DrawerKind::LiquidRegion => "DrawLiquidRegion",
            DrawerKind::LiquidTile => "DrawLiquidTile",
            DrawerKind::MultiWeave => "DrawMultiWeave",
            DrawerKind::Particles => "DrawParticles",
            DrawerKind::Pistons => "DrawPistons",
            DrawerKind::Plasma => "DrawPlasma",
            DrawerKind::Power => "DrawPower",
            DrawerKind::PulseShape => "DrawPulseShape",
            DrawerKind::PumpLiquid => "DrawPumpLiquid",
            DrawerKind::SoftParticles => "DrawSoftParticles",
            DrawerKind::Spikes => "DrawSpikes",
            DrawerKind::Weave => "DrawWeave",
            DrawerKind::BlockParts => "DrawBlockParts",
            DrawerKind::Region => "DrawRegion",
            DrawerKind::RegionSpin => "DrawRegionSpin",
            DrawerKind::Turret => "DrawTurret",
            DrawerKind::Weapon => "DrawWeapon",
            DrawerKind::SideRegion => "DrawSideRegion",
            DrawerKind::GlowRegion => "DrawGlowRegion",
            DrawerKind::Arc => "DrawArc",
            DrawerKind::BlurSpin => "DrawBlurSpin",
            DrawerKind::Particle => "DrawParticle",
            DrawerKind::Shape => "DrawShape",
            DrawerKind::SoftShadow => "DrawSoftShadow",
            DrawerKind::WarmupRegion => "DrawWarmupRegion",
        }
    }
}

/// Parsed drawer tree, the data half of one `DrawBlock` instance.
///
/// The union of the ported drawer classes' fields is kept flat (upstream stores
/// them per class); fields that do not apply to `kind` stay at the class
/// defaults from [`DrawerSpec::for_kind`].
#[derive(Debug, Clone, PartialEq)]
pub struct DrawerSpec {
    /// Drawer class.
    pub kind: DrawerKind,
    /// `DrawMulti.drawers` children.
    pub children: Vec<DrawerSpec>,
    /// `DrawTurret.parts` / `DrawBlockParts.parts`.
    pub parts: Vec<DrawPartSpec>,
    /// Region suffix (`DrawRegion.suffix`, `DrawFlame`/heat/fade/power drawers).
    pub suffix: String,
    /// `DrawRegion.name` (explicit region override).
    pub name: Option<String>,
    /// Tint color (`DrawRegion.color`, `DrawFlame.flameColor`, …).
    pub color: Option<Rgba>,
    /// `DrawFlame.flameColor`.
    pub flame_color: Option<Rgba>,
    /// `DrawLiquidRegion.drawLiquid`/`DrawLiquidTile.drawLiquid` name.
    pub liquid: Option<String>,
    /// `DrawTurret.basePrefix`.
    pub base_prefix: String,
    /// X offset.
    pub x: f32,
    /// Y offset.
    pub y: f32,
    /// Rotation offset.
    pub rotation: f32,
    /// Spin speed (`DrawRegion.rotateSpeed`).
    pub rotate_speed: f32,
    /// Draw layer (`<0` = default).
    pub layer: f32,
    /// Alpha (`DrawLiquidRegion`/`DrawFade`).
    pub alpha: f32,
    /// Scale (`DrawFade`).
    pub scale: f32,
    /// Pulse amplitude (`DrawHeatInput`/`DrawHeatOutput`/`DrawHeatRegion`).
    pub pulse: f32,
    /// Pulse period scale.
    pub pulse_scl: f32,
    /// `DrawFlame.flameRadius`.
    pub flame_radius: f32,
    /// `DrawRegion.spinSprite`.
    pub spin_sprite: bool,
    /// `DrawRegion.drawPlan`/`DrawPower.drawPlan`.
    pub draw_plan: bool,
    /// `DrawRegion.buildingRotate`.
    pub building_rotate: bool,
}

impl Default for DrawerSpec {
    fn default() -> Self {
        Self::for_kind(DrawerKind::Default)
    }
}

impl DrawerSpec {
    /// Class defaults for `kind` (`world/draw/*.java` field initializers).
    pub fn for_kind(kind: DrawerKind) -> Self {
        let mut spec = Self {
            kind,
            children: Vec::new(),
            parts: Vec::new(),
            suffix: String::new(),
            name: None,
            color: None,
            flame_color: None,
            liquid: None,
            base_prefix: String::new(),
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            rotate_speed: 0.0,
            layer: -1.0,
            alpha: 1.0,
            scale: 1.0,
            pulse: 0.0,
            pulse_scl: 1.0,
            flame_radius: 0.0,
            spin_sprite: false,
            draw_plan: true,
            building_rotate: false,
        };
        match kind {
            DrawerKind::Flame => {
                spec.flame_color = Rgba::from_hex("ffc999");
                spec.flame_radius = 3.0;
            }
            DrawerKind::Fade => {
                spec.suffix = String::from("-top");
                spec.alpha = 0.6;
                spec.scale = 3.0;
            }
            DrawerKind::LiquidRegion | DrawerKind::LiquidTile | DrawerKind::PumpLiquid => {
                spec.suffix = String::from("-liquid");
            }
            DrawerKind::Power => spec.suffix = String::from("-power"),
            DrawerKind::HeatInput => {
                spec.suffix = String::from("-heat");
                spec.pulse = 0.3;
                spec.pulse_scl = 10.0;
            }
            DrawerKind::HeatOutput => {
                spec.pulse = 0.3;
                spec.pulse_scl = 10.0;
            }
            DrawerKind::HeatRegion => {
                spec.suffix = String::from("-glow");
                spec.pulse = 0.3;
                spec.pulse_scl = 10.0;
            }
            DrawerKind::Pistons => spec.suffix = String::from("-piston"),
            _ => {}
        }
        spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawer_kind_names_round_trip_the_java_classes() {
        for kind in [
            DrawerKind::Default,
            DrawerKind::Multi,
            DrawerKind::Region,
            DrawerKind::Turret,
            DrawerKind::BlockParts,
        ] {
            assert!(!kind.name().is_empty());
        }
        assert_eq!(DrawerSpec::default().kind, DrawerKind::Default);
        assert_eq!(DrawerSpec::for_kind(DrawerKind::Flame).flame_radius, 3.0);
        assert_eq!(DrawerSpec::for_kind(DrawerKind::Fade).suffix, "-top");
    }
}
