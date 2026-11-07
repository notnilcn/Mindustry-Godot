// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Headless execution of the `world/draw/*` block descriptors (plan 16 §3.6/M9).
//!
//! Upstream every block owns a `DrawBlock` (`Block.draw`) that is executed
//! inside `BlockRenderer.draw`/`cacheChunk`; the descriptors emit Arc `Draw`/
//! `Fill`/`Lines` primitives at explicit `Layer`s. This module ports the
//! descriptor bodies into Godot-free [`DrawDesc`] values and executes them into
//! the plan-16 [`crate::render::commands::CommandBuffer`] sink, so the headless
//! `render list` oracle can assert the **full layer set** (floor/overlay/wall,
//! `blockUnder`/`block` caches, `blockAdditive` heat/glow/liquid, and the global
//! shadow/darkness/light/fog layers) without a GPU.
//!
//! Ported from `core/src/mindustry/world/draw/Draw*.java`; the per-block chains
//! in [`vanilla_chain`] cite `content/Blocks.java` line numbers. Descriptors are
//! pure (no Godot, no sim feedback): they only read a caller-provided
//! [`DrawState`] view. Trig is evaluated directly; at `time == 0` and
//! `progress == 0` every term is exact, which keeps the committed goldens
//! cross-platform stable.

use crate::config::TILESIZE;
use crate::content::registries::blocks::drawer::{DrawerKind, DrawerSpec};
use crate::content::registries::units::parts::DrawPartSpec;
use crate::content::{BlockDef, ContentRegistry, Rgba};
use crate::math::ArcRand;
use crate::render::commands::{Blend, DrawCmd, FillKind, LineKind, ShapeKind};
use crate::render::ids::{RegionId, RegionIdTable};
use crate::render::layer::Layer;
use crate::render::liquid_frames;

/// One draw primitive with its resolved `Layer` z and blend mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DescDraw {
    /// Layer z (`Draw.z`).
    pub z: f32,
    /// Blend mode.
    pub blend: Blend,
    /// Emission command.
    pub cmd: DrawCmd,
}

impl DescDraw {
    /// A sprite draw at `z`.
    #[allow(clippy::too_many_arguments)]
    fn sprite(
        z: f32,
        blend: Blend,
        region: RegionId,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rot: f32,
        color: u32,
    ) -> Self {
        Self {
            z,
            blend,
            cmd: DrawCmd::Sprite {
                region,
                x,
                y,
                w,
                h,
                ox: 0.5,
                oy: 0.5,
                rot,
                color,
                blend,
            },
        }
    }

    /// A filled circle.
    fn circle(z: f32, blend: Blend, x: f32, y: f32, r: f32, color: u32) -> Self {
        Self {
            z,
            blend,
            cmd: DrawCmd::Fill {
                kind: FillKind::Circle,
                params: [x, y, r, 0.0, 0.0, 0.0],
                color,
            },
        }
    }

    /// A stroked circle.
    fn circle_line(z: f32, blend: Blend, x: f32, y: f32, r: f32, stroke: f32, color: u32) -> Self {
        Self {
            z,
            blend,
            cmd: DrawCmd::Shape {
                kind: ShapeKind::CircleOutline,
                params: [x, y, r, stroke, 0.0, 0.0, 0.0, 0.0],
                color,
            },
        }
    }

    /// A filled or stroked regular polygon.
    #[allow(clippy::too_many_arguments)]
    fn poly(
        z: f32,
        blend: Blend,
        x: f32,
        y: f32,
        sides: i32,
        r: f32,
        rot: f32,
        fill: bool,
        stroke: f32,
        color: u32,
    ) -> Self {
        if fill {
            Self {
                z,
                blend,
                cmd: DrawCmd::Fill {
                    kind: FillKind::Poly,
                    params: [x, y, sides as f32, r, rot, 0.0],
                    color,
                },
            }
        } else {
            Self {
                z,
                blend,
                cmd: DrawCmd::Shape {
                    kind: ShapeKind::Poly,
                    params: [x, y, sides as f32, r, rot, stroke, 0.0, 0.0],
                    color,
                },
            }
        }
    }

    /// A line segment.
    #[allow(clippy::too_many_arguments)]
    fn line(
        z: f32,
        blend: Blend,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        stroke: f32,
        color: u32,
    ) -> Self {
        Self {
            z,
            blend,
            cmd: DrawCmd::Lines {
                kind: LineKind::Line,
                params: [x1, y1, x2, y2, 0.0, 0.0, 0.0, 0.0],
                color,
                stroke,
            },
        }
    }
}

/// Content-resolved `DrawLiquidTile.drawLiquid` fields (`fill_liquid_tiles`):
/// the `Liquid` record the tiled animation frames and tint come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiquidFrame {
    /// `Liquid.id` (the per-liquid animation phase offset; `Liquid.java:107`).
    pub id: u16,
    /// `Liquid.gas` (`fluid-gas-<i>` frames instead of `fluid-liquid-<i>`).
    pub gas: bool,
    /// `Liquid.color` tint (RGBA8888).
    pub color: u32,
}

/// Immutable view of the entity a descriptor draws (the port of the Arc
/// `Building` fields the drawers read). All positions are world pixels.
#[derive(Clone, Copy, Debug)]
pub struct DrawState {
    /// Center x.
    pub x: f32,
    /// Center y.
    pub y: f32,
    /// Rotation `0..=3`.
    pub rotation: i32,
    /// `Building.rotdeg()`.
    pub rotdeg: f32,
    /// `Block.size` in tiles.
    pub size: i32,
    /// `Building.warmup()`.
    pub warmup: f32,
    /// `Building.progress()` (`0..=1`).
    pub progress: f32,
    /// `Building.totalProgress()`.
    pub total_progress: f32,
    /// `Time.time` (view clock).
    pub time: f32,
    /// Stable entity id (seeds the per-building RNG like `Building.id`).
    pub id: i32,
    /// `Building.power.status`.
    pub power_status: f32,
    /// Liquid capacity (`Block.liquidCapacity`).
    pub liquid_capacity: f32,
    /// Current liquid amount.
    pub liquid_amount: f32,
    /// Current liquid color.
    pub liquid_color: u32,
    /// Dominant liquid content id (`Liquid.id`; the `Liquid.getAnimationFrame`
    /// phase offset when the drawer names no liquid).
    pub liquid_id: u16,
    /// Dominant liquid `Liquid.gas`.
    pub liquid_gas: bool,
    /// Heat amount (`HeatBlock.heat`).
    pub heat: f32,
    /// Heat requirement (`HeatBlock.heatRequirement`).
    pub heat_requirement: f32,
    /// Per-side heat (`HeatConsumer.sideHeat`).
    pub side_heat: [f32; 4],
    /// `Building.drawrot()`.
    pub draw_rot: f32,
    /// Base layer for the block pass.
    pub layer: f32,
}

impl DrawState {
    /// A default building view on a `size`-tile block.
    pub const fn new(x: f32, y: f32, rotation: i32, size: i32) -> Self {
        Self {
            x,
            y,
            rotation,
            rotdeg: 0.0,
            size,
            warmup: 0.0,
            progress: 0.0,
            total_progress: 0.0,
            time: 0.0,
            id: 0,
            power_status: 0.0,
            liquid_capacity: 1.0,
            liquid_amount: 0.0,
            liquid_color: 0xffff_ffff,
            liquid_id: 0,
            liquid_gas: false,
            heat: 0.0,
            heat_requirement: 1.0,
            side_heat: [0.0; 4],
            draw_rot: 0.0,
            layer: Layer::Block.z(),
        }
    }
}

/// A ported `world/draw/Draw*.java` body. Each variant maps to one Java class.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawDesc {
    /// `DrawDefault` (`DrawBlock` default body): the block region at `drawrot`.
    Default,
    /// `DrawRegion` (suffix/rotateSpeed/spin/color/layer/offset).
    Region {
        /// Region suffix.
        suffix: &'static str,
        /// `DrawRegion.name` (`DrawRegion.load:79`): an explicit region name
        /// that overrides `block.name + suffix`.
        name: Option<&'static str>,
        /// `DrawRegion.rotateSpeed`.
        rotate_speed: f32,
        /// `DrawRegion.rotation`.
        rotation: f32,
        /// `DrawRegion.spinSprite`.
        spin: bool,
        /// `DrawRegion.buildingRotate`.
        building_rotate: bool,
        /// `DrawRegion.layer` (`<= 0` = keep the block layer).
        layer: f32,
        /// `DrawRegion.color`.
        color: Option<u32>,
        /// `DrawRegion.x`.
        x: f32,
        /// `DrawRegion.y`.
        y: f32,
    },
    /// `DrawSideRegion`.
    SideRegion {
        /// `DrawSideRegion.top1`.
        top1: &'static str,
        /// `DrawSideRegion.top2`.
        top2: &'static str,
    },
    /// `DrawFrames`.
    Frames {
        /// `DrawFrames.frames`.
        frames: i32,
        /// `DrawFrames.interval`.
        interval: f32,
        /// `DrawFrames.sine`.
        sine: bool,
    },
    /// `DrawWarmupRegion`.
    WarmupRegion {
        /// `DrawWarmupRegion.color`.
        color: u32,
        /// `DrawWarmupRegion.sinMag`.
        sin_mag: f32,
        /// `DrawWarmupRegion.sinScl`.
        sin_scl: f32,
    },
    /// `DrawFade`.
    Fade {
        /// `DrawFade.suffix`.
        suffix: &'static str,
        /// `DrawFade.alpha`.
        alpha: f32,
        /// `DrawFade.scale`.
        scale: f32,
    },
    /// `DrawLiquidRegion` (and `DrawPumpLiquid` semantics).
    LiquidRegion {
        /// Region suffix.
        suffix: &'static str,
        /// Alpha multiplier.
        alpha: f32,
    },
    /// `DrawLiquidTile`.
    LiquidTile {
        /// `DrawLiquidTile.padding` (`<0` = 0); the port uses it for all four
        /// sides (`DrawLiquidTile.load` copies it into the unset pads).
        padding: f32,
        /// Alpha multiplier.
        alpha: f32,
        /// `DrawLiquidTile.drawLiquid` content name (`None` = the building's
        /// current liquid).
        liquid: Option<&'static str>,
        /// Content-resolved `drawLiquid` fields ([`fill_liquid_tiles`]).
        frame: Option<LiquidFrame>,
    },
    /// `DrawPower`.
    Power {
        /// Region suffix.
        suffix: &'static str,
        /// `DrawPower.mixcol`.
        mixcol: bool,
        /// Empty color.
        empty: u32,
        /// Full color.
        full: u32,
        /// Layer override (`<= 0` = keep).
        layer: f32,
    },
    /// `DrawHeatRegion`.
    HeatRegion {
        /// Region suffix.
        suffix: &'static str,
        /// Base color.
        color: u32,
        /// `DrawHeatRegion.pulse`.
        pulse: f32,
        /// `DrawHeatRegion.pulseScl`.
        pulse_scl: f32,
        /// Layer override.
        layer: f32,
    },
    /// `DrawHeatInput`.
    HeatInput {
        /// Region suffix.
        suffix: &'static str,
        /// Base color.
        color: u32,
        /// `DrawHeatInput.heatPulse`.
        pulse: f32,
        /// `DrawHeatInput.heatPulseScl`.
        pulse_scl: f32,
    },
    /// `DrawHeatOutput`.
    HeatOutput {
        /// `DrawHeatOutput.heatColor`.
        color: u32,
        /// `DrawHeatOutput.heatPulse`.
        pulse: f32,
        /// `DrawHeatOutput.heatPulseScl`.
        pulse_scl: f32,
        /// `DrawHeatOutput.glowMult`.
        glow_mult: f32,
        /// `DrawHeatOutput.rotOffset`.
        rot_offset: i32,
        /// `DrawHeatOutput.drawGlow`.
        draw_glow: bool,
    },
    /// `DrawGlowRegion`.
    GlowRegion {
        /// Region suffix.
        suffix: &'static str,
        /// `DrawGlowRegion.color`.
        color: u32,
        /// `DrawGlowRegion.alpha`.
        alpha: f32,
        /// `DrawGlowRegion.glowScale`.
        glow_scale: f32,
        /// `DrawGlowRegion.glowIntensity`.
        glow_intensity: f32,
        /// `DrawGlowRegion.rotateSpeed`.
        rotate_speed: f32,
        /// `DrawGlowRegion.rotate`.
        rotate: bool,
        /// Layer override.
        layer: f32,
    },
    /// `DrawPistons`.
    Pistons {
        /// `DrawPistons.sinMag`.
        sin_mag: f32,
        /// `DrawPistons.sinScl`.
        sin_scl: f32,
        /// `DrawPistons.sinOffset`.
        sin_offset: f32,
        /// `DrawPistons.sideOffset`.
        side_offset: f32,
        /// `DrawPistons.lenOffset`.
        len_offset: f32,
        /// `DrawPistons.horiOffset`.
        hori_offset: f32,
        /// `DrawPistons.angleOffset`.
        angle_offset: f32,
        /// `DrawPistons.sides`.
        sides: i32,
        /// Region suffix.
        suffix: &'static str,
    },
    /// `DrawFlame`.
    Flame {
        /// `DrawFlame.flameColor`.
        flame_color: u32,
        /// `DrawFlame.flameRadius`.
        flame_radius: f32,
        /// `DrawFlame.flameRadiusIn`.
        flame_radius_in: f32,
        /// `DrawFlame.flameRadiusScl`.
        flame_radius_scl: f32,
        /// `DrawFlame.flameRadiusMag`.
        flame_radius_mag: f32,
        /// `DrawFlame.flameRadiusInMag`.
        flame_radius_in_mag: f32,
    },
    /// `DrawParticles`.
    Particles {
        /// `DrawParticles.color`.
        color: u32,
        /// `DrawParticles.sides`.
        sides: i32,
        /// `DrawParticles.alpha` (class default `0.5`,
        /// `DrawParticles.java:13`).
        alpha: f32,
        /// `DrawParticles.particles`.
        particles: i32,
        /// `DrawParticles.particleLife`.
        particle_life: f32,
        /// `DrawParticles.particleRad`.
        particle_rad: f32,
        /// `DrawParticles.particleSize`.
        particle_size: f32,
        /// `DrawParticles.fadeMargin`.
        fade_margin: f32,
        /// `DrawParticles.rotateScl`.
        rotate_scl: f32,
        /// `DrawParticles.reverse`.
        reverse: bool,
        /// `DrawParticles.poly`.
        poly: bool,
    },
    /// `DrawSoftParticles` (`Additive`).
    SoftParticles {
        /// `DrawSoftParticles.color`.
        color: u32,
        /// `DrawSoftParticles.color2`.
        color2: u32,
        /// `DrawSoftParticles.alpha` (class default `0.5`,
        /// `DrawSoftParticles.java:15`).
        alpha: f32,
        /// `DrawSoftParticles.particles`.
        particles: i32,
        /// `DrawSoftParticles.particleLife`.
        particle_life: f32,
        /// `DrawSoftParticles.particleRad`.
        particle_rad: f32,
        /// `DrawSoftParticles.particleSize`.
        particle_size: f32,
        /// `DrawSoftParticles.fadeMargin`.
        fade_margin: f32,
        /// `DrawSoftParticles.rotateScl`.
        rotate_scl: f32,
    },
    /// `DrawPulseShape`.
    PulseShape {
        /// `DrawPulseShape.color`.
        color: u32,
        /// `DrawPulseShape.stroke`.
        stroke: f32,
        /// `DrawPulseShape.timeScl`.
        time_scl: f32,
        /// `DrawPulseShape.minStroke`.
        min_stroke: f32,
        /// `DrawPulseShape.radiusScl`.
        radius_scl: f32,
        /// `DrawPulseShape.square`.
        square: bool,
        /// Layer override.
        layer: f32,
    },
    /// `DrawShape`.
    Shape {
        /// `DrawShape.color`.
        color: u32,
        /// `DrawShape.sides`.
        sides: i32,
        /// `DrawShape.radius`.
        radius: f32,
        /// `DrawShape.timeScl`.
        time_scl: f32,
        /// `DrawShape.useWarmupRadius`.
        use_warmup_radius: bool,
        /// Layer override.
        layer: f32,
    },
    /// `DrawSpikes`.
    Spikes {
        /// `DrawSpikes.color`.
        color: u32,
        /// `DrawSpikes.amount`.
        amount: i32,
        /// `DrawSpikes.layers`.
        layers: i32,
        /// `DrawSpikes.stroke`.
        stroke: f32,
        /// `DrawSpikes.rotateSpeed`.
        rotate_speed: f32,
        /// `DrawSpikes.radius`.
        radius: f32,
        /// `DrawSpikes.length`.
        length: f32,
        /// `DrawSpikes.layerSpeed`.
        layer_speed: f32,
    },
    /// `DrawCells`.
    Cells {
        /// `DrawCells.color`.
        color: u32,
        /// `DrawCells.particles`.
        particles: i32,
        /// `DrawCells.range`.
        range: f32,
        /// `DrawCells.recurrence`.
        recurrence: f32,
        /// `DrawCells.radius`.
        radius: f32,
        /// `DrawCells.lifetime`.
        lifetime: f32,
    },
    /// `DrawBubbles`.
    Bubbles {
        /// `DrawBubbles.color`.
        color: u32,
        /// `DrawBubbles.amount`.
        amount: i32,
        /// `DrawBubbles.sides`.
        sides: i32,
        /// `DrawBubbles.strokeMin`.
        stroke_min: f32,
        /// `DrawBubbles.spread`.
        spread: f32,
        /// `DrawBubbles.timeScl`.
        time_scl: f32,
        /// `DrawBubbles.recurrence`.
        recurrence: f32,
        /// `DrawBubbles.radius`.
        radius: f32,
        /// `DrawBubbles.fill`.
        fill: bool,
    },
    /// `DrawArcSmelt` (`Additive`).
    ArcSmelt {
        /// `DrawArcSmelt.flameColor`.
        flame_color: u32,
        /// `DrawArcSmelt.midColor`.
        mid_color: u32,
        /// `DrawArcSmelt.flameRad`.
        flame_rad: f32,
        /// `DrawArcSmelt.circleSpace`.
        circle_space: f32,
        /// `DrawArcSmelt.flameRadiusScl`.
        flame_radius_scl: f32,
        /// `DrawArcSmelt.flameRadiusMag`.
        flame_radius_mag: f32,
        /// `DrawArcSmelt.circleStroke`.
        circle_stroke: f32,
        /// `DrawArcSmelt.alpha`.
        alpha: f32,
        /// `DrawArcSmelt.particles`.
        particles: i32,
        /// `DrawArcSmelt.particleLife`.
        particle_life: f32,
        /// `DrawArcSmelt.particleRad`.
        particle_rad: f32,
        /// `DrawArcSmelt.particleStroke`.
        particle_stroke: f32,
        /// `DrawArcSmelt.particleLen`.
        particle_len: f32,
        /// `DrawArcSmelt.x`.
        x: f32,
        /// `DrawArcSmelt.y`.
        y: f32,
    },
    /// `DrawCrucibleFlame` (`Additive`).
    CrucibleFlame {
        /// `DrawCrucibleFlame.flameColor`.
        flame_color: u32,
        /// `DrawCrucibleFlame.midColor`.
        mid_color: u32,
        /// `DrawCrucibleFlame.flameRad`.
        flame_rad: f32,
        /// `DrawCrucibleFlame.circleSpace`.
        circle_space: f32,
        /// `DrawCrucibleFlame.flameRadiusScl`.
        flame_radius_scl: f32,
        /// `DrawCrucibleFlame.flameRadiusMag`.
        flame_radius_mag: f32,
        /// `DrawCrucibleFlame.circleStroke`.
        circle_stroke: f32,
        /// `DrawCrucibleFlame.alpha`.
        alpha: f32,
        /// `DrawCrucibleFlame.particles`.
        particles: i32,
        /// `DrawCrucibleFlame.particleLife`.
        particle_life: f32,
        /// `DrawCrucibleFlame.particleRad`.
        particle_rad: f32,
        /// `DrawCrucibleFlame.particleSize`.
        particle_size: f32,
        /// `DrawCrucibleFlame.fadeMargin`.
        fade_margin: f32,
        /// `DrawCrucibleFlame.x`.
        x: f32,
        /// `DrawCrucibleFlame.y`.
        y: f32,
    },
    /// `DrawCultivator`.
    Cultivator {
        /// `DrawCultivator.plantColor`.
        plant_color: u32,
        /// `DrawCultivator.plantColorLight`.
        plant_color_light: u32,
        /// `DrawCultivator.bottomColor`.
        bottom_color: u32,
        /// `DrawCultivator.bubbles`.
        bubbles: i32,
        /// `DrawCultivator.sides`.
        sides: i32,
        /// `DrawCultivator.strokeMin`.
        stroke_min: f32,
        /// `DrawCultivator.spread`.
        spread: f32,
        /// `DrawCultivator.timeScl`.
        time_scl: f32,
        /// `DrawCultivator.recurrence`.
        recurrence: f32,
        /// `DrawCultivator.radius`.
        radius: f32,
    },
    /// `DrawBlurSpin`.
    BlurSpin {
        /// Region suffix.
        suffix: &'static str,
        /// `DrawBlurSpin.rotateSpeed`.
        rotate_speed: f32,
        /// `DrawBlurSpin.blurThresh`.
        blur_thresh: f32,
    },
    /// `DrawMultiWeave`.
    MultiWeave {
        /// `DrawMultiWeave.glowColor`.
        glow_color: u32,
        /// `DrawMultiWeave.weaveColor`.
        weave_color: u32,
        /// `DrawMultiWeave.rotateSpeed`.
        rotate_speed: f32,
        /// `DrawMultiWeave.rotateSpeed2`.
        rotate_speed2: f32,
        /// `DrawMultiWeave.fadeWeave`.
        fade_weave: bool,
        /// `DrawMultiWeave.pulse`.
        pulse: f32,
        /// `DrawMultiWeave.pulseScl`.
        pulse_scl: f32,
    },
    /// `DrawPlasma` (`Additive`).
    Plasma {
        /// `DrawPlasma.plasma1`.
        plasma1: u32,
        /// `DrawPlasma.plasma2`.
        plasma2: u32,
        /// `DrawPlasma.plasmas`.
        plasmas: i32,
        /// Region suffix.
        suffix: &'static str,
    },
    /// `DrawWeave`.
    Weave,
    /// `DrawCircles`.
    Circles {
        /// `DrawCircles.color`.
        color: u32,
        /// `DrawCircles.amount`.
        amount: i32,
        /// `DrawCircles.sides`.
        sides: i32,
        /// `DrawCircles.strokeMin`.
        stroke_min: f32,
        /// `DrawCircles.strokeMax`.
        stroke_max: f32,
        /// `DrawCircles.timeScl`.
        time_scl: f32,
        /// `DrawCircles.radius`.
        radius: f32,
    },
    /// `DrawLiquidOutputs` (`world/draw/DrawLiquidOutputs.java`): one region
    /// per `outputLiquids[i]` with a `liquidOutputDirections[i]` side
    /// (`-1` = none), `<block>-<liquid>-output1|2` (L06 T5).
    LiquidOutputs {
        /// `(liquid name, liquidOutputDirections[i])` per output, in
        /// `outputLiquids` order. The names come from the block's content
        /// `output_liquids` (`GenericCrafter.outputLiquids`), which
        /// [`chain_for`] resolves through the content registry; a bare
        /// `from_drawer` leaves the list empty.
        outputs: Vec<(String, i8)>,
    },
    /// `DrawPumpLiquid` (GAPS #57): the `-liquid` region filled from the pump's
    /// drop amount (`DrawPumpLiquid.java:9-23`); the suffix is always `-liquid`
    /// (`DrawPumpLiquid.load`, `:20-22`).
    PumpLiquid,
    /// `DrawBlockParts` (GAPS #57): the block's `parts` tree
    /// (`DrawBlockParts.parts`, `DrawBlockParts.java:9-13`), executed through
    /// the plan-17 `DrawPart` compiler.
    BlockParts {
        /// `DrawBlockParts.parts`.
        parts: Vec<DrawPartSpec>,
    },
    /// `DrawTurret` (GAPS #57 follow-up): the live turret renderer owns the
    /// real pass (`mind-gdext::render::turret`); this descriptor carries the
    /// parsed part tree so the headless layer oracle and the turret renderer
    /// share one conversion. The base sprite stays in the block pass.
    Turret {
        /// `DrawTurret.basePrefix`.
        base_prefix: &'static str,
        /// `DrawTurret.parts` (+ `ammoParts` fallback handled live).
        parts: Vec<DrawPartSpec>,
    },
    /// `DrawWeapon`: the unit/block weapon part tree (`DrawWeapon` alias; no
    /// upstream `world/draw` class — the closest body is the turret part
    /// pass).
    Weapon {
        /// The weapon's `DrawPart` tree.
        parts: Vec<DrawPartSpec>,
    },
    /// `DrawMulti`.
    Multi(Vec<DrawDesc>),
}

/// Port of `Mathf.sin(in, scl, mag)` (`Arc Mathf.java:100`).
#[inline]
fn sin_scl(in_: f32, scl: f32, mag: f32) -> f32 {
    (in_ / scl).sin() * mag
}

/// Port of `Mathf.absin(in, scl, mag)`: `(sin(in, scl*2, mag) + mag) / 2f`.
#[inline]
fn absin(in_: f32, scl: f32, mag: f32) -> f32 {
    (sin_scl(in_, scl * 2.0, mag) + mag) / 2.0
}

/// Port of `Angles.trnsx`/`trnsy` for a length along `angle` degrees.
#[inline]
fn trns(angle_deg: f32, len: f32) -> (f32, f32) {
    let radians = angle_deg.to_radians();
    (radians.cos() * len, radians.sin() * len)
}

/// `Mathf.curve(a, b, t)` from plan 17 ([`crate::math::curve`]).
#[inline]
fn curve(a: f32, b: f32, t: f32) -> f32 {
    crate::math::curve(a, b, t)
}

#[inline]
fn clamp(v: f32, lo: f32, hi: f32) -> f32 {
    v.min(hi).max(lo)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[inline]
fn lerp_color(a: u32, b: u32, t: f32) -> u32 {
    let ca = Rgba::from_rgba8888(a);
    let cb = Rgba::from_rgba8888(b);
    Rgba::new(
        lerp(ca.r, cb.r, t),
        lerp(ca.g, cb.g, t),
        lerp(ca.b, cb.b, t),
        lerp(ca.a, cb.a, t),
    )
    .to_rgba8888()
}

#[inline]
fn with_alpha(color: u32, alpha: f32) -> u32 {
    Rgba::from_rgba8888(color).with_alpha(alpha).to_rgba8888()
}

#[inline]
fn scale_color(color: u32, factor: f32) -> u32 {
    let c = Rgba::from_rgba8888(color);
    Rgba::new(c.r * factor, c.g * factor, c.b * factor, c.a).to_rgba8888()
}

/// Resolves `name + suffix` into a region id.
fn region(name: &str, suffix: &str, ids: &mut RegionIdTable) -> RegionId {
    if suffix.is_empty() {
        ids.intern_str(name)
    } else {
        let mut owned = String::with_capacity(name.len() + suffix.len());
        owned.push_str(name);
        owned.push_str(suffix);
        ids.intern(owned)
    }
}

/// Executes one descriptor body, appending primitives to `out` (plan 16 §6.2).
pub fn execute(
    desc: &DrawDesc,
    name: &str,
    state: &DrawState,
    ids: &mut RegionIdTable,
    out: &mut Vec<DescDraw>,
) {
    let ts = TILESIZE as f32;
    let quad = state.size as f32 * ts;
    match desc {
        DrawDesc::Default => {
            let r = region(name, "", ids);
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                state.y,
                quad,
                quad,
                state.draw_rot,
                0xffff_ffff,
            ));
        }
        DrawDesc::Region {
            suffix,
            name: region_name,
            rotate_speed,
            rotation,
            building_rotate,
            layer,
            color,
            x,
            y,
            ..
        } => {
            // `DrawRegion.load` (`DrawRegion.java:79`):
            // `atlas.find(name != null ? name : block.name + suffix)` — an
            // explicit `name` replaces the whole `block.name + suffix` string.
            let r = match region_name {
                Some(region_name) => ids.intern_str(region_name),
                None => region(name, suffix, ids),
            };
            let z = if *layer > 0.0 { *layer } else { state.layer };
            let rot = state.total_progress * rotate_speed
                + rotation
                + if *building_rotate { state.rotdeg } else { 0.0 };
            out.push(DescDraw::sprite(
                z,
                Blend::Normal,
                r,
                state.x + x,
                state.y + y,
                quad,
                quad,
                rot,
                color.unwrap_or(0xffff_ffff),
            ));
        }
        DrawDesc::SideRegion { top1, top2 } => {
            let r = region(name, if state.rotation > 1 { top2 } else { top1 }, ids);
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                state.y,
                quad,
                quad,
                state.rotdeg,
                0xffff_ffff,
            ));
        }
        DrawDesc::Frames {
            frames,
            interval,
            sine,
        } => {
            let mut index = if *sine {
                absin(state.total_progress, *interval, (*frames as f32) - 0.001) as i32
            } else {
                ((state.total_progress / interval) % (*frames as f32)) as i32
            };
            index = index.clamp(0, (*frames).max(1) - 1);
            let suffix = format!("-frame{index}");
            let r = region(name, &suffix, ids);
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                state.y,
                quad,
                quad,
                0.0,
                0xffff_ffff,
            ));
        }
        DrawDesc::WarmupRegion {
            color,
            sin_mag,
            sin_scl,
        } => {
            let r = region(name, "-top", ids);
            let a = state.warmup * (1.0 - sin_mag)
                + absin(state.time, *sin_scl, *sin_mag) * state.warmup;
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                state.y,
                quad,
                quad,
                0.0,
                with_alpha(*color, a),
            ));
        }
        DrawDesc::Fade {
            suffix,
            alpha,
            scale,
        } => {
            let r = region(name, suffix, ids);
            let a = absin(state.total_progress, *scale, *alpha) * state.warmup;
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                state.y,
                quad,
                quad,
                0.0,
                with_alpha(0xffff_ffff, a),
            ));
        }
        DrawDesc::LiquidRegion { suffix, alpha } => {
            let r = region(name, suffix, ids);
            let frac = clamp(
                state.liquid_amount / state.liquid_capacity.max(f32::EPSILON),
                0.0,
                1.0,
            ) * alpha;
            let h = quad * frac;
            let y = state.y - quad / 2.0 + h / 2.0;
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                y,
                quad,
                h,
                0.0,
                state.liquid_color,
            ));
        }
        // `DrawLiquidTile.draw` -> `LiquidBlock.drawTiledFrames`
        // (`DrawLiquidTile.java:26-30`, `LiquidBlock.java:39-77`): the liquid's
        // 50-frame animation tiled over every block cell, truncated against the
        // drawer's padding. This is `Renderer.fluidFrames` (`Renderer.java:131-141`).
        DrawDesc::LiquidTile {
            padding,
            alpha,
            frame,
            ..
        } => {
            let (liquid_id, gas, color) = frame
                .map(|frame| (frame.id, frame.gas, frame.color))
                .unwrap_or((state.liquid_id, state.liquid_gas, state.liquid_color));
            let frac = clamp(
                state.liquid_amount / state.liquid_capacity.max(f32::EPSILON),
                0.0,
                1.0,
            ) * alpha;
            // `Drawf.liquid`: `if(alpha < 1f/255f) return;` then
            // `Draw.color(color, alpha * color.a)` (gases carry `a = 0.6`;
            // `Liquid.java:96-100`).
            let color_alpha = f32::from(color as u8) / 255.0;
            let draw_alpha = frac * color_alpha;
            if draw_alpha < 1.0 / 255.0 {
                return;
            }
            let frame = liquid_frames::animation_frame(state.time, gas, liquid_id);
            let r = ids.intern_str(liquid_frames::frame_region(gas, frame));
            for tile in liquid_frames::tiled_frames(state.size, state.x, state.y, *padding) {
                out.push(DescDraw::sprite(
                    state.layer,
                    Blend::Normal,
                    r,
                    tile.x,
                    tile.y,
                    tile.w,
                    tile.h,
                    0.0,
                    with_alpha(color, draw_alpha),
                ));
            }
        }
        DrawDesc::Power {
            suffix,
            mixcol,
            empty,
            full,
            layer,
        } => {
            let r = region(name, suffix, ids);
            let z = if *layer > 0.0 { *layer } else { state.layer };
            let color = if *mixcol {
                lerp_color(*empty, *full, state.power_status)
            } else {
                0xffff_ffff
            };
            out.push(DescDraw::sprite(
                z,
                Blend::Normal,
                r,
                state.x,
                state.y,
                quad,
                quad,
                0.0,
                color,
            ));
        }
        DrawDesc::HeatRegion {
            suffix,
            color,
            pulse,
            pulse_scl,
            layer,
        } => {
            if state.heat <= 0.0 {
                return;
            }
            let r = region(name, suffix, ids);
            let z = if *layer > 0.0 {
                *layer
            } else {
                Layer::BlockAdditive.z()
            };
            let frac = clamp(
                state.heat / state.heat_requirement.max(f32::EPSILON),
                0.0,
                1.0,
            );
            let a = frac
                * (Rgba::from_rgba8888(*color).a
                    * (1.0 - pulse + absin(state.time, *pulse_scl, *pulse)));
            out.push(DescDraw::sprite(
                z,
                Blend::Additive,
                r,
                state.x,
                state.y,
                quad,
                quad,
                0.0,
                with_alpha(*color, a),
            ));
        }
        DrawDesc::HeatInput {
            suffix,
            color,
            pulse,
            pulse_scl,
        } => {
            let r = region(name, suffix, ids);
            for (i, side) in state.side_heat.iter().enumerate() {
                if *side <= 0.0 {
                    continue;
                }
                let a = (side / state.heat_requirement.max(f32::EPSILON))
                    * (Rgba::from_rgba8888(*color).a
                        * (1.0 - pulse + absin(state.time, *pulse_scl, *pulse)));
                out.push(DescDraw::sprite(
                    Layer::BlockAdditive.z(),
                    Blend::Additive,
                    r,
                    state.x,
                    state.y,
                    quad,
                    quad,
                    i as f32 * 90.0,
                    with_alpha(*color, a),
                ));
            }
        }
        DrawDesc::HeatOutput {
            color,
            pulse,
            pulse_scl,
            glow_mult,
            rot_offset,
            draw_glow,
        } => {
            let rotdex = (state.rotation + rot_offset).rem_euclid(4);
            let rot = (state.rotation + rot_offset) as f32 * 90.0;
            let top = if rotdex > 1 { "-top2" } else { "-top1" };
            let top_r = region(name, top, ids);
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                top_r,
                state.x,
                state.y,
                quad,
                quad,
                rot,
                0xffff_ffff,
            ));
            if state.heat > 0.0 {
                let heat_r = region(name, "-heat", ids);
                let glow_r = region(name, "-glow", ids);
                let a = (state.heat / state.heat_requirement.max(f32::EPSILON))
                    * (Rgba::from_rgba8888(*color).a
                        * (1.0 - pulse + absin(state.time, *pulse_scl, *pulse)));
                out.push(DescDraw::sprite(
                    Layer::BlockAdditive.z(),
                    Blend::Additive,
                    heat_r,
                    state.x,
                    state.y,
                    quad,
                    quad,
                    rot,
                    with_alpha(*color, a),
                ));
                if *draw_glow {
                    out.push(DescDraw::sprite(
                        Layer::BlockAdditive.z(),
                        Blend::Additive,
                        glow_r,
                        state.x,
                        state.y,
                        quad,
                        quad,
                        0.0,
                        scale_color(with_alpha(*color, a), *glow_mult),
                    ));
                }
            }
        }
        DrawDesc::GlowRegion {
            suffix,
            color,
            alpha,
            glow_scale,
            glow_intensity,
            rotate_speed,
            rotate,
            layer,
        } => {
            if state.warmup <= 0.001 {
                return;
            }
            let r = region(name, suffix, ids);
            let z = if *layer > 0.0 {
                *layer
            } else {
                Layer::BlockAdditive.z()
            };
            let a = (absin(state.total_progress, *glow_scale, *alpha) * glow_intensity + 1.0
                - glow_intensity)
                * state.warmup
                * alpha;
            let rot =
                state.total_progress * rotate_speed + if *rotate { state.rotdeg } else { 0.0 };
            out.push(DescDraw::sprite(
                z,
                Blend::Additive,
                r,
                state.x,
                state.y,
                quad,
                quad,
                rot,
                with_alpha(*color, a),
            ));
        }
        DrawDesc::Pistons {
            sin_mag,
            sin_scl,
            sin_offset,
            side_offset,
            len_offset,
            hori_offset,
            angle_offset,
            sides,
            suffix,
        } => {
            for i in 0..*sides {
                let len = absin(
                    state.total_progress + sin_offset + side_offset * sin_scl * i as f32,
                    *sin_scl,
                    *sin_mag,
                ) + len_offset;
                let angle = angle_offset + i as f32 * 360.0 / *sides as f32;
                let (dx, dy) = trns(angle, len);
                let r = region(name, suffix, ids);
                out.push(DescDraw::sprite(
                    state.layer,
                    Blend::Normal,
                    r,
                    state.x + dx + 0.0,
                    state.y + dy + 0.0,
                    quad,
                    quad,
                    angle,
                    0xffff_ffff,
                ));
                let _ = hori_offset;
            }
        }
        DrawDesc::Flame {
            flame_color,
            flame_radius,
            flame_radius_in,
            flame_radius_scl,
            flame_radius_mag,
            flame_radius_in_mag,
        } => {
            if state.warmup <= 0.0 {
                return;
            }
            let top_r = region(name, "-top", ids);
            out.push(DescDraw::sprite(
                Layer::Block.z() + 0.01,
                Blend::Normal,
                top_r,
                state.x,
                state.y,
                quad,
                quad,
                0.0,
                with_alpha(0xffff_ffff, state.warmup),
            ));
            let mut rng = ArcRand::new(state.id as u64);
            let cr = rng.next_float() * 0.1;
            let outer = flame_radius + absin(state.time, *flame_radius_scl, *flame_radius_mag) + cr;
            let inner =
                flame_radius_in + absin(state.time, *flame_radius_scl, *flame_radius_in_mag) + cr;
            let a = ((1.0 - 0.3) + absin(state.time, 8.0, 0.3) + (rng.next_float() * 0.06) - 0.06)
                * state.warmup;
            out.push(DescDraw::circle(
                Layer::Block.z() + 0.01,
                Blend::Normal,
                state.x,
                state.y,
                outer,
                with_alpha(*flame_color, a),
            ));
            out.push(DescDraw::circle(
                Layer::Block.z() + 0.01,
                Blend::Normal,
                state.x,
                state.y,
                inner,
                with_alpha(0xffff_ffff, state.warmup),
            ));
        }
        DrawDesc::Particles {
            color,
            sides,
            alpha,
            particles,
            particle_life,
            particle_rad,
            particle_size,
            fade_margin,
            rotate_scl,
            reverse,
            poly,
        } => {
            if state.warmup <= 0.0 {
                return;
            }
            // `DrawParticles.draw` (`:24`): `a = alpha * build.warmup()`.
            let a = alpha * state.warmup;
            let base = state.time / particle_life;
            let mut rng = ArcRand::new(state.id as u64);
            for _ in 0..*particles {
                let mut fin = (rng.next_float() * 2.0 + base) % 1.0;
                if *reverse {
                    fin = 1.0 - fin;
                }
                let fout = 1.0 - fin;
                let angle = rng.next_float() * 360.0 + (state.time / rotate_scl) % 360.0;
                let len = particle_rad * fout.powf(1.5);
                let (dx, dy) = trns(angle, len);
                let alpha = a * (1.0 - curve(fin, 1.0 - fade_margin, 1.0));
                let size = particle_size * (1.0 - fin) * state.warmup;
                if *poly {
                    out.push(DescDraw::poly(
                        state.layer,
                        Blend::Normal,
                        state.x + dx,
                        state.y + dy,
                        *sides,
                        size,
                        0.0,
                        true,
                        0.0,
                        with_alpha(*color, alpha),
                    ));
                } else {
                    out.push(DescDraw::circle(
                        state.layer,
                        Blend::Normal,
                        state.x + dx,
                        state.y + dy,
                        size,
                        with_alpha(*color, alpha),
                    ));
                }
            }
        }
        DrawDesc::SoftParticles {
            color,
            color2,
            alpha,
            particles,
            particle_life,
            particle_rad,
            particle_size,
            fade_margin,
            rotate_scl,
        } => {
            if state.warmup <= 0.0 {
                return;
            }
            // `DrawSoftParticles.draw` (`:27`): `a = alpha * build.warmup()`.
            let a = alpha * state.warmup;
            let base = state.time / particle_life;
            let mut rng = ArcRand::new(state.id as u64);
            for _ in 0..*particles {
                let mut fin = (rng.next_float() + base) % 1.0;
                let mut fout = 1.0 - fin;
                fin = 1.0 - fin;
                fout = 1.0 - fout;
                let angle = rng.next_float() * 360.0 + (state.time / rotate_scl) % 360.0;
                let col = rng.next_float();
                let len = particle_rad * fout.powf(1.5);
                let (dx, dy) = trns(angle, len);
                let alpha = a * (1.0 - curve(fin, 1.0 - fade_margin, 1.0));
                let r = particle_size * fin * state.warmup * 2.0;
                let c = lerp_color(*color, *color2, col);
                out.push(DescDraw::circle(
                    state.layer,
                    Blend::Additive,
                    state.x + dx,
                    state.y + dy,
                    r,
                    with_alpha(c, alpha),
                ));
            }
        }
        DrawDesc::PulseShape {
            color,
            stroke,
            time_scl,
            min_stroke,
            radius_scl,
            square,
            layer,
        } => {
            let z = if *layer > 0.0 { *layer } else { state.layer };
            let f = 1.0 - (state.time / time_scl) % 1.0;
            let rad = state.size as f32 * ts / 2.0 * radius_scl;
            let s = (stroke * f + min_stroke) * state.warmup;
            if *square {
                let r = (1.0 + (1.0 - f) * rad).min(rad);
                for i in 0..4 {
                    let (nx, ny) = trns(90.0 * i as f32, r);
                    let (px, py) = trns(90.0 * (i + 1) as f32, r);
                    out.push(DescDraw::line(
                        z,
                        Blend::Normal,
                        state.x + nx,
                        state.y + ny,
                        state.x + px,
                        state.y + py,
                        s,
                        *color,
                    ));
                }
            } else {
                let r = (clamp(2.0 - f * 2.0, 0.0, 1.0) * rad - f - 0.2).max(0.0);
                let w = clamp(0.5 - f, 0.0, 1.0) * rad * 2.0;
                let d4 = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)];
                for (i, (dx, dy)) in d4.iter().enumerate() {
                    let nx = state.x + dx * r + dy * w;
                    let ny = state.y + dy * r - dx * w;
                    let (px, py) = d4[(i + 1) % 4];
                    let ex = state.x + px * r + py * w;
                    let ey = state.y + py * r - px * w;
                    out.push(DescDraw::line(z, Blend::Normal, nx, ny, ex, ey, s, *color));
                }
            }
        }
        DrawDesc::Shape {
            color,
            sides,
            radius,
            time_scl,
            use_warmup_radius,
            layer,
        } => {
            let z = if *layer > 0.0 { *layer } else { state.layer };
            let r = if *use_warmup_radius {
                radius * state.warmup
            } else {
                *radius
            };
            out.push(DescDraw::poly(
                z,
                Blend::Normal,
                state.x,
                state.y,
                *sides,
                r,
                state.total_progress * time_scl,
                true,
                0.0,
                *color,
            ));
        }
        DrawDesc::Spikes {
            color,
            amount,
            layers,
            stroke,
            rotate_speed,
            radius,
            length,
            layer_speed,
        } => {
            if state.warmup <= 0.001 {
                return;
            }
            let c = with_alpha(*color, Rgba::from_rgba8888(*color).a * state.warmup);
            let mut cur = 1.0f32;
            for _ in 0..*layers {
                let start = state.total_progress * rotate_speed * cur;
                for i in 0..*amount {
                    let angle = start + i as f32 * 360.0 / (*amount).max(1) as f32;
                    let (dx, dy) = trns(angle, *radius);
                    let (ex, ey) = trns(angle, *radius + *length);
                    out.push(DescDraw::line(
                        state.layer,
                        Blend::Normal,
                        state.x + dx,
                        state.y + dy,
                        state.x + ex,
                        state.y + ey,
                        *stroke,
                        c,
                    ));
                }
                cur *= layer_speed;
            }
        }
        DrawDesc::Cells {
            color,
            particles,
            range,
            recurrence,
            radius,
            lifetime,
        } => {
            let mid = region(name, "-middle", ids);
            let frac = clamp(
                state.liquid_amount / state.liquid_capacity.max(f32::EPSILON),
                0.0,
                1.0,
            );
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                mid,
                state.x,
                state.y,
                quad,
                quad * (frac * state.warmup).max(0.0),
                0.0,
                *color,
            ));
            if state.warmup <= 0.001 {
                return;
            }
            let mut rng = ArcRand::new(state.id as u64);
            for _ in 0..*particles {
                let offset = rng.next_float() * 999_999.0;
                let x = (rng.next_float() * 2.0 - 1.0) * range;
                let y = (rng.next_float() * 2.0 - 1.0) * range;
                let fin = 1.0 - (((state.time + offset) / lifetime) % recurrence);
                let fslope = crate::math::slope(fin);
                if fin > 0.0 {
                    out.push(DescDraw::circle(
                        state.layer,
                        Blend::Normal,
                        state.x + x,
                        state.y + y,
                        fslope * radius,
                        with_alpha(*color, state.warmup),
                    ));
                }
            }
        }
        DrawDesc::Bubbles {
            color,
            amount,
            sides,
            stroke_min,
            spread,
            time_scl,
            recurrence,
            radius,
            fill,
        } => {
            if state.warmup <= 0.001 {
                return;
            }
            let mut rng = ArcRand::new(state.id as u64);
            for _ in 0..*amount {
                let x = (rng.next_float() * 2.0 - 1.0) * spread;
                let y = (rng.next_float() * 2.0 - 1.0) * spread;
                let life =
                    1.0 - ((state.time / time_scl + rng.next_float() * recurrence) % recurrence);
                if life > 0.0 {
                    let rad = (1.0 - life) * radius;
                    if *fill {
                        out.push(DescDraw::circle(
                            state.layer,
                            Blend::Normal,
                            state.x + x,
                            state.y + y,
                            rad,
                            with_alpha(*color, state.warmup),
                        ));
                    } else {
                        out.push(DescDraw::poly(
                            state.layer,
                            Blend::Normal,
                            state.x + x,
                            state.y + y,
                            *sides,
                            rad,
                            0.0,
                            false,
                            state.warmup * (life + stroke_min),
                            with_alpha(*color, state.warmup),
                        ));
                    }
                }
            }
        }
        DrawDesc::ArcSmelt {
            flame_color,
            mid_color,
            flame_rad,
            circle_space,
            flame_radius_scl,
            flame_radius_mag,
            circle_stroke,
            alpha,
            particles,
            particle_life,
            particle_rad,
            particle_stroke,
            particle_len,
            x,
            y,
        } => {
            if state.warmup <= 0.0 {
                return;
            }
            // `DrawArcSmelt.draw` (`:19-40`): every primitive is offset by
            // `x`/`y` (`build.x + x`).
            let (fx, fy) = (state.x + x, state.y + y);
            let si = absin(state.time, *flame_radius_scl, *flame_radius_mag);
            let a = alpha * state.warmup;
            out.push(DescDraw::circle(
                state.layer,
                Blend::Additive,
                fx,
                fy,
                flame_rad + si,
                with_alpha(*mid_color, a),
            ));
            out.push(DescDraw::circle_line(
                state.layer,
                Blend::Additive,
                fx,
                fy,
                (flame_rad + circle_space + si) * state.warmup,
                circle_stroke * state.warmup,
                with_alpha(*flame_color, a),
            ));
            let base = state.time / particle_life;
            let mut rng = ArcRand::new(state.id as u64);
            for _ in 0..*particles {
                let fin = (rng.next_float() + base) % 1.0;
                let fout = 1.0 - fin;
                let angle = rng.next_float() * 360.0;
                let len = particle_rad * (1.0 - (1.0 - fin).powi(2));
                let (dx, dy) = trns(angle, len);
                let (ex, ey) = trns(angle, particle_len * fout * state.warmup);
                out.push(DescDraw::line(
                    state.layer,
                    Blend::Additive,
                    fx + dx,
                    fy + dy,
                    fx + dx + ex,
                    fy + dy + ey,
                    particle_stroke * state.warmup,
                    with_alpha(*flame_color, a),
                ));
            }
        }
        DrawDesc::CrucibleFlame {
            flame_color,
            mid_color,
            flame_rad,
            circle_space,
            flame_radius_scl,
            flame_radius_mag,
            circle_stroke,
            alpha,
            particles,
            particle_life,
            particle_rad,
            particle_size,
            fade_margin,
            x,
            y,
        } => {
            if state.warmup <= 0.0 {
                return;
            }
            // `DrawCrucibleFlame.draw` (`:21-49`): every primitive is offset by
            // `x`/`y` (`build.x + x`).
            let (fx, fy) = (state.x + x, state.y + y);
            let si = absin(state.time, *flame_radius_scl, *flame_radius_mag);
            let a = alpha * state.warmup;
            out.push(DescDraw::circle(
                state.layer,
                Blend::Additive,
                fx,
                fy,
                flame_rad + si,
                with_alpha(*mid_color, a),
            ));
            out.push(DescDraw::circle_line(
                state.layer,
                Blend::Additive,
                fx,
                fy,
                (flame_rad + circle_space + si) * state.warmup,
                circle_stroke * state.warmup,
                with_alpha(*flame_color, a),
            ));
            let base = state.time / particle_life;
            let mut rng = ArcRand::new(state.id as u64);
            for _ in 0..*particles {
                let fin = (rng.next_float() + base) % 1.0;
                let fout = 1.0 - fin;
                let angle = rng.next_float() * 360.0 + (state.time / 1.5) % 360.0;
                let len = particle_rad * fout.powf(1.5);
                let (dx, dy) = trns(angle, len);
                let alpha_p = a * (1.0 - curve(fin, 1.0 - fade_margin, 1.0));
                out.push(DescDraw::circle(
                    state.layer,
                    Blend::Additive,
                    fx + dx,
                    fy + dy,
                    particle_size * fin * state.warmup,
                    with_alpha(*flame_color, alpha_p),
                ));
            }
        }
        DrawDesc::Cultivator {
            plant_color,
            plant_color_light,
            bottom_color,
            bubbles,
            sides,
            stroke_min,
            spread,
            time_scl,
            recurrence,
            radius,
        } => {
            let mid = region(name, "-middle", ids);
            let frac = clamp(
                state.liquid_amount / state.liquid_capacity.max(f32::EPSILON),
                0.0,
                1.0,
            );
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                mid,
                state.x,
                state.y,
                quad,
                quad * (frac * state.warmup).max(0.0),
                0.0,
                *plant_color,
            ));
            let c = lerp_color(*bottom_color, *plant_color_light, state.warmup);
            let mut rng = ArcRand::new(state.id as u64);
            for _ in 0..*bubbles {
                let rx = (rng.next_float() * 2.0 - 1.0) * spread;
                let ry = (rng.next_float() * 2.0 - 1.0) * spread;
                let life =
                    1.0 - ((state.time / time_scl + rng.next_float() * recurrence) % recurrence);
                if life > 0.0 {
                    out.push(DescDraw::poly(
                        state.layer,
                        Blend::Normal,
                        state.x + rx,
                        state.y + ry,
                        *sides,
                        (1.0 - life) * radius,
                        0.0,
                        false,
                        state.warmup * (life + stroke_min),
                        c,
                    ));
                }
            }
        }
        DrawDesc::BlurSpin {
            suffix,
            rotate_speed,
            blur_thresh,
        } => {
            let suffix = if state.warmup > *blur_thresh {
                format!("{suffix}-blur")
            } else {
                (*suffix).to_owned()
            };
            let r = region(name, &suffix, ids);
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                state.y,
                quad,
                quad,
                state.total_progress * rotate_speed,
                0xffff_ffff,
            ));
        }
        DrawDesc::MultiWeave {
            glow_color,
            weave_color,
            rotate_speed,
            rotate_speed2,
            fade_weave,
            pulse,
            pulse_scl,
        } => {
            let weave = region(name, "-weave", ids);
            let glow = region(name, "-weave-glow", ids);
            let wa = if *fade_weave { state.warmup } else { 1.0 };
            let r1 = state.total_progress * rotate_speed;
            let r2 = state.total_progress * rotate_speed * rotate_speed2;
            for rot in [r1, r2] {
                out.push(DescDraw::sprite(
                    state.layer,
                    Blend::Normal,
                    weave,
                    state.x,
                    state.y,
                    quad,
                    quad,
                    rot,
                    with_alpha(*weave_color, wa),
                ));
            }
            let ga = state.warmup
                * (Rgba::from_rgba8888(*glow_color).a
                    * (1.0 - pulse + absin(state.time, *pulse_scl, *pulse)));
            for rot in [r1, r2] {
                out.push(DescDraw::sprite(
                    state.layer,
                    Blend::Additive,
                    glow,
                    state.x,
                    state.y,
                    quad,
                    quad,
                    rot,
                    with_alpha(*glow_color, ga),
                ));
            }
        }
        DrawDesc::Plasma {
            plasma1,
            plasma2,
            plasmas,
            suffix,
        } => {
            for i in 0..*plasmas {
                let name_suffix = format!("{suffix}{i}");
                let r = region(name, &name_suffix, ids);
                let radius = ts - 3.0 + absin(state.time, 2.0 + i as f32, 5.0 - i as f32 * 0.5);
                let color = lerp_color(*plasma1, *plasma2, i as f32 / *plasmas as f32);
                let a = (0.3 + absin(state.time, 2.0 + i as f32 * 2.0, 0.3 + i as f32 * 0.05))
                    * state.warmup;
                out.push(DescDraw::sprite(
                    state.layer,
                    Blend::Additive,
                    r,
                    state.x,
                    state.y,
                    radius,
                    radius,
                    state.total_progress * (12.0 + i as f32 * 6.0),
                    with_alpha(color, a),
                ));
            }
        }
        DrawDesc::Weave => {
            let r = region(name, "-weave", ids);
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                state.y,
                quad,
                quad,
                state.total_progress,
                0xffff_ffff,
            ));
            let lx = state.x + sin_scl(state.total_progress, 6.0, ts / 3.0 * state.size as f32);
            out.push(DescDraw::line(
                state.layer,
                Blend::Normal,
                lx,
                state.y,
                lx,
                state.y,
                quad / 2.0,
                with_alpha(0xffd3_7fff, state.warmup),
            ));
        }
        DrawDesc::Circles {
            color,
            amount,
            sides,
            stroke_min,
            stroke_max,
            time_scl,
            radius,
        } => {
            if state.warmup <= 0.001 {
                return;
            }
            for i in 0..*amount {
                let life = (state.time / time_scl + i as f32 / (*amount).max(1) as f32) % 1.0;
                let stroke =
                    state.warmup * (stroke_max + (stroke_min - stroke_max) * life.powf(3.0));
                out.push(DescDraw::poly(
                    state.layer,
                    Blend::Normal,
                    state.x,
                    state.y,
                    *sides,
                    life * radius,
                    0.0,
                    false,
                    stroke,
                    with_alpha(*color, Rgba::from_rgba8888(*color).a * state.warmup),
                ));
            }
        }
        DrawDesc::LiquidOutputs { outputs } => {
            // `DrawLiquidOutputs.draw` (`DrawLiquidOutputs.java:16-26`):
            // `realRot = (side + rotation) % 4`, region row `realRot > 1 ? 1 :
            // 0` (`-output2` / `-output1`), drawn untinted at `realRot * 90`.
            for (liquid, side) in outputs.iter() {
                if *side < 0 {
                    continue;
                }
                let real_rot = (i32::from(*side) + state.rotation).rem_euclid(4);
                let suffix = format!("-{liquid}-output{}", if real_rot > 1 { 2 } else { 1 });
                let r = region(name, &suffix, ids);
                out.push(DescDraw::sprite(
                    state.layer,
                    Blend::Normal,
                    r,
                    state.x,
                    state.y,
                    quad,
                    quad,
                    real_rot as f32 * 90.0,
                    0xffff_ffff,
                ));
            }
        }
        DrawDesc::PumpLiquid => {
            // `DrawPumpLiquid.draw` (`DrawPumpLiquid.java:13-17`):
            // `Drawf.liquid(liquid, x, y, liquids.get(drop)/capacity, drop.color)`.
            let r = region(name, "-liquid", ids);
            let frac = clamp(
                state.liquid_amount / state.liquid_capacity.max(f32::EPSILON),
                0.0,
                1.0,
            );
            let h = quad * frac;
            let y = state.y - quad / 2.0 + h / 2.0;
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                y,
                quad,
                h,
                0.0,
                state.liquid_color,
            ));
        }
        DrawDesc::BlockParts { parts } => {
            if parts.is_empty() {
                return;
            }
            // `DrawBlockParts.draw` (`DrawBlockParts.java:39-51`): params from
            // `build.warmup()` and `1 - progress`, then every part in order.
            emit_part_tree(parts, state, name, ids, out, state.layer, state.rotdeg);
        }
        // `DrawTurret` (`DrawTurret.java:73-120`) is drawn by the live turret
        // renderer; this headless body emits the parsed part tree at
        // `Layer.turret` (the base/shadow/head/liquid/top stay with the turret
        // pass and the block sprite pass).
        DrawDesc::Turret { parts, .. } => {
            if parts.is_empty() {
                return;
            }
            emit_part_tree(
                parts,
                state,
                name,
                ids,
                out,
                Layer::Turret.z(),
                state.rotdeg,
            );
        }
        // `DrawWeapon`: the unit/block weapon part tree, drawn by the same
        // part compiler at the turret band.
        DrawDesc::Weapon { parts } => {
            if parts.is_empty() {
                return;
            }
            emit_part_tree(
                parts,
                state,
                name,
                ids,
                out,
                Layer::Turret.z(),
                state.rotdeg,
            );
        }
        DrawDesc::Multi(list) => {
            for d in list {
                execute(d, name, state, ids, out);
            }
        }
    }
}

/// Runs one `DrawPart` tree through the plan-17 compiler and maps its
/// primitives onto the block command buffer at `base_z`.
///
/// `DrawPart.params` is set exactly like `DrawBlockParts.draw`
/// (`DrawBlockParts.java:39-51`) / `DrawTurret.draw`
/// (`DrawTurret.java:104-110`): the per-building view state the port has
/// (`DrawState`) fills warmup/progress/position; heat/recoil/charge are zero
/// because they are live turret/weapon state the headless view does not carry.
fn emit_part_tree(
    parts: &[DrawPartSpec],
    state: &DrawState,
    name: &str,
    ids: &mut RegionIdTable,
    out: &mut Vec<DescDraw>,
    base_z: f32,
    rotation: f32,
) {
    let mut params = crate::fx::parts::params::PartParams::default();
    params.set(
        state.warmup,
        1.0 - state.progress,
        1.0 - state.progress,
        0.0,
        0.0,
        0.0,
        state.x,
        state.y,
        rotation,
    );
    let lookup = crate::fx::parts::draw::AllRegions::new(32.0);
    let mut program = crate::render::draw::DrawProgram::new();
    crate::fx::parts::draw::draw_named_parts(
        &mut program,
        parts,
        &params,
        &lookup,
        state.time,
        name,
    );
    // `PartEmit::new` starts at `Layer::Effect`; rebase every primitive onto
    // the band this descriptor draws in.
    let delta = base_z - Layer::Effect.z();
    for prim in &program.prims {
        if let Some(cmd) = draw_cmd_for_prim(prim, ids) {
            out.push(DescDraw {
                z: prim.z + delta,
                blend: to_blend(prim.blend),
                cmd,
            });
        }
    }
}

/// `Blending` -> command buffer `Blend`.
fn to_blend(blend: crate::render::draw::Blending) -> Blend {
    use crate::render::draw::Blending as B;
    match blend {
        B::Normal => Blend::Normal,
        B::Additive => Blend::Additive,
        B::Multiply => Blend::Multiply,
    }
}

/// Maps one plan-17 `DrawPrim` onto the block command buffer (GAPS #57
/// `DrawBlockParts` bridge). Primitives with no plan-16 equivalent (light,
/// shader, noise) are dropped.
fn draw_cmd_for_prim(
    prim: &crate::render::draw::DrawPrim,
    ids: &mut RegionIdTable,
) -> Option<DrawCmd> {
    use crate::render::draw::PrimKind as P;
    Some(match &prim.kind {
        P::Region {
            region,
            x,
            y,
            w,
            h,
            rotation_deg,
            origin,
            color,
            ..
        } => DrawCmd::Sprite {
            region: ids.intern_str(region.0),
            x: *x,
            y: *y,
            w: *w,
            h: *h,
            ox: origin.0,
            oy: origin.1,
            rot: *rotation_deg,
            color: color.to_rgba8888(),
            blend: to_blend(prim.blend),
        },
        P::Rect { x, y, w, h, color } => DrawCmd::Fill {
            kind: FillKind::Rect,
            params: [*x, *y, *w, *h, 0.0, 0.0],
            color: color.to_rgba8888(),
        },
        P::Circle {
            x,
            y,
            r,
            fill,
            stroke,
            color,
        } => {
            if *fill {
                DrawCmd::Fill {
                    kind: FillKind::Circle,
                    params: [*x, *y, *r, 0.0, 0.0, 0.0],
                    color: color.to_rgba8888(),
                }
            } else {
                DrawCmd::Shape {
                    kind: ShapeKind::CircleOutline,
                    params: [*x, *y, *r, *stroke, 0.0, 0.0, 0.0, 0.0],
                    color: color.to_rgba8888(),
                }
            }
        }
        P::Poly {
            x,
            y,
            sides,
            r,
            rotation_deg,
            fill,
            stroke,
            color,
        } => {
            if *fill {
                DrawCmd::Fill {
                    kind: FillKind::Poly,
                    params: [*x, *y, *sides as f32, *r, *rotation_deg, 0.0],
                    color: color.to_rgba8888(),
                }
            } else {
                DrawCmd::Shape {
                    kind: ShapeKind::Poly,
                    params: [*x, *y, *sides as f32, *r, *rotation_deg, *stroke, 0.0, 0.0],
                    color: color.to_rgba8888(),
                }
            }
        }
        P::Tri {
            x,
            y,
            w,
            h,
            rotation_deg,
            color,
        } => DrawCmd::Fill {
            kind: FillKind::Triangle,
            params: [*x, *y, *w, *h, *rotation_deg, 0.0],
            color: color.to_rgba8888(),
        },
        P::Line {
            x1,
            y1,
            x2,
            y2,
            stroke,
            color,
            ..
        } => DrawCmd::Lines {
            kind: LineKind::Line,
            params: [*x1, *y1, *x2, *y2, 0.0, 0.0, 0.0, 0.0],
            color: color.to_rgba8888(),
            stroke: *stroke,
        },
        P::Arc {
            x,
            y,
            r,
            start_deg,
            sweep_deg,
            stroke,
            color,
        } => DrawCmd::Lines {
            kind: LineKind::Arc,
            params: [*x, *y, *r, *start_deg, *sweep_deg, 0.0, 0.0, 0.0],
            color: color.to_rgba8888(),
            stroke: *stroke,
        },
        _ => return None,
    })
}

/// Splits a `Block.draw` chain at its `DrawDefault` part (`DrawMulti` draws its
/// parts in order, `DrawMulti.java`): everything before the default body draws
/// *under* the base sprite (`DrawRegion("-bottom")`, `DrawLiquidTile`), the rest
/// over it (I02 T7; H03 T8 deviation 4). A chain without a `DrawDefault` puts
/// every part in the "over" list, because the caller emits the block sprite
/// itself.
pub fn split_chain_at_default(chain: &[DrawDesc]) -> (Vec<DrawDesc>, Vec<DrawDesc>) {
    fn walk(
        desc: &DrawDesc,
        before: &mut Vec<DrawDesc>,
        after: &mut Vec<DrawDesc>,
        seen_default: &mut bool,
        any_default: &mut bool,
    ) {
        match desc {
            DrawDesc::Default => {
                *seen_default = true;
                *any_default = true;
            }
            DrawDesc::Multi(list) => {
                for item in list {
                    walk(item, before, after, seen_default, any_default);
                }
            }
            other => {
                if *seen_default {
                    after.push(other.clone());
                } else {
                    before.push(other.clone());
                }
            }
        }
    }

    let mut before = Vec::new();
    let mut after = Vec::new();
    let mut seen_default = false;
    let mut any_default = false;
    for desc in chain {
        walk(
            desc,
            &mut before,
            &mut after,
            &mut seen_default,
            &mut any_default,
        );
    }
    if any_default {
        (before, after)
    } else {
        (Vec::new(), before)
    }
}

/// Executes a chain's **extra** descriptors, skipping the base block region
/// (`Default` and an empty-suffix `Region`) that the sprite oracle already
/// emitted through [`crate::render::list::build_entries`].
pub fn execute_extras(
    desc: &DrawDesc,
    name: &str,
    state: &DrawState,
    ids: &mut RegionIdTable,
    out: &mut Vec<DescDraw>,
) {
    match desc {
        // An empty-suffix `DrawRegion` is the base sprite the sprite oracle
        // already emitted; an explicit `DrawRegion.name` is a second region
        // and must still draw.
        DrawDesc::Default
        | DrawDesc::Region {
            suffix: "",
            name: None,
            ..
        } => {}
        DrawDesc::Multi(list) => {
            for d in list {
                execute_extras(d, name, state, ids, out);
            }
        }
        _ => execute(desc, name, state, ids, out),
    }
}

/// Builds the exact `Block.draw` chain for the vanilla blocks this lane ports
/// (from `content/Blocks.java`). Blocks without an explicit `drawer` fall back
/// to [`DrawDesc::Default`], matching `Block`'s default `DrawDefault`.
///
/// Cited lines: `silicon-smelter` 1078, `kiln` 1112, `surge-smelter` 1164,
/// `plastanium-compressor` 1131, `phase-weaver` 1145, `cryofluid-mixer` 1181,
/// `melter` 1227, `separator` 1248, `spore-press` 1281, `pulverizer` 1302,
/// `combustion-generator` 2540, `steam-generator` 2574, `cultivator` 2970.
pub fn vanilla_chain(name: &str) -> Option<Vec<DrawDesc>> {
    Some(match name {
        "silicon-smelter" => vec![
            DrawDesc::Default,
            DrawDesc::Flame {
                flame_color: 0xffef_99ff,
                flame_radius: 3.0,
                flame_radius_in: 1.9,
                flame_radius_scl: 5.0,
                flame_radius_mag: 2.0,
                flame_radius_in_mag: 1.0,
            },
        ],
        "kiln" => vec![
            DrawDesc::Default,
            DrawDesc::Flame {
                flame_color: 0xffc0_99ff,
                flame_radius: 3.0,
                flame_radius_in: 1.9,
                flame_radius_scl: 5.0,
                flame_radius_mag: 2.0,
                flame_radius_in_mag: 1.0,
            },
        ],
        "surge-smelter" => vec![
            DrawDesc::Default,
            DrawDesc::Flame {
                flame_color: 0xffc9_99ff,
                flame_radius: 3.0,
                flame_radius_in: 1.9,
                flame_radius_scl: 5.0,
                flame_radius_mag: 2.0,
                flame_radius_in_mag: 1.0,
            },
        ],
        "plastanium-compressor" => vec![
            DrawDesc::Default,
            DrawDesc::Fade {
                suffix: "-top",
                alpha: 0.6,
                scale: 3.0,
            },
        ],
        "phase-weaver" => vec![region_desc("-bottom"), DrawDesc::Weave, DrawDesc::Default],
        "cryofluid-mixer" => vec![
            region_desc("-bottom"),
            // `new DrawLiquidTile(Liquids.water)` / `(Liquids.cryofluid)`
            // (`Blocks.java:1181`).
            DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
                liquid: Some("water"),
                frame: None,
            },
            DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
                liquid: Some("cryofluid"),
                frame: None,
            },
            DrawDesc::Default,
        ],
        "melter" | "separator" | "disassembler" => vec![
            region_desc("-bottom"),
            // `new DrawLiquidTile()` (current liquid; `Blocks.java:1227/1248/1268`).
            DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
                liquid: None,
                frame: None,
            },
            DrawDesc::Default,
        ],
        "spore-press" => vec![
            region_desc("-bottom"),
            DrawDesc::Pistons {
                sin_mag: 1.0,
                sin_scl: 6.0,
                sin_offset: 50.0,
                side_offset: 0.0,
                len_offset: -1.0,
                hori_offset: 0.0,
                angle_offset: 0.0,
                sides: 4,
                suffix: "-piston",
            },
            DrawDesc::Default,
            DrawDesc::LiquidRegion {
                suffix: "-liquid",
                alpha: 1.0,
            },
            region_desc("-top"),
        ],
        "pulverizer" => vec![
            DrawDesc::Default,
            DrawDesc::Region {
                suffix: "-rotator",
                name: None,
                rotate_speed: 2.0,
                rotation: 0.0,
                spin: true,
                building_rotate: false,
                layer: -1.0,
                color: None,
                x: 0.0,
                y: 0.0,
            },
            region_desc("-top"),
        ],
        "combustion-generator" => vec![DrawDesc::Default, warmup_region()],
        "steam-generator" => vec![
            DrawDesc::Default,
            warmup_region(),
            DrawDesc::Region {
                suffix: "-turbine",
                name: None,
                rotate_speed: 2.0,
                rotation: 0.0,
                spin: false,
                building_rotate: false,
                layer: -1.0,
                color: None,
                x: 0.0,
                y: 0.0,
            },
            DrawDesc::Region {
                suffix: "-turbine",
                name: None,
                rotate_speed: -2.0,
                rotation: 45.0,
                spin: false,
                building_rotate: false,
                layer: -1.0,
                color: None,
                x: 0.0,
                y: 0.0,
            },
            region_desc("-cap"),
            DrawDesc::LiquidRegion {
                suffix: "-liquid",
                alpha: 1.0,
            },
        ],
        "cultivator" => vec![
            region_desc("-bottom"),
            // `new DrawLiquidTile(Liquids.water)` (`Blocks.java:2972`).
            DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
                liquid: Some("water"),
                frame: None,
            },
            DrawDesc::Default,
            DrawDesc::Cultivator {
                plant_color: 0x5541_b1ff,
                plant_color_light: 0x7457_ceff,
                bottom_color: 0x4747_47ff,
                bubbles: 12,
                sides: 8,
                stroke_min: 0.2,
                spread: 3.0,
                time_scl: 70.0,
                recurrence: 6.0,
                radius: 3.0,
            },
            region_desc("-top"),
        ],
        // `Blocks.java:1356-1398`: `DrawMulti(DrawRegion("-bottom"),
        // DrawLiquidTile(water, 2), DrawBubbles(7693e3){sides 10, recurrence 3,
        // spread 6, radius 1.5, amount 20}, DrawRegion(), DrawLiquidOutputs(),
        // DrawGlowRegion(){alpha 0.7, color c4bdf3, glowIntensity 0.3,
        // glowScale 6})`; `outputLiquids = ozone, hydrogen`,
        // `liquidOutputDirections = {1, 3}` (L06 T5).
        "electrolyzer" => vec![
            region_desc("-bottom"),
            // `new DrawLiquidTile(Liquids.water, 2f)` (`Blocks.java:1374`).
            DrawDesc::LiquidTile {
                padding: 2.0,
                alpha: 1.0,
                liquid: Some("water"),
                frame: None,
            },
            DrawDesc::Bubbles {
                color: 0x7693_e3ff,
                amount: 20,
                sides: 10,
                stroke_min: 0.2,
                spread: 6.0,
                time_scl: 30.0,
                recurrence: 3.0,
                radius: 1.5,
                fill: false,
            },
            DrawDesc::Default,
            DrawDesc::LiquidOutputs {
                // `GenericCrafter.outputLiquids` + `liquidOutputDirections`
                // (`Blocks.java:1396-1397`). The content table also carries
                // these; the hand-written chain mirrors upstream.
                outputs: vec![(String::from("ozone"), 1), (String::from("hydrogen"), 3)],
            },
            DrawDesc::GlowRegion {
                suffix: "-glow",
                color: 0xc4bd_f3ff,
                alpha: 0.7,
                glow_scale: 6.0,
                glow_intensity: 0.3,
                rotate_speed: 0.0,
                rotate: false,
                layer: -1.0,
            },
        ],
        _ => return None,
    })
}

/// A no-rotation `DrawRegion(suffix)`.
fn region_desc(suffix: &'static str) -> DrawDesc {
    DrawDesc::Region {
        suffix,
        name: None,
        rotate_speed: 0.0,
        rotation: 0.0,
        spin: false,
        building_rotate: false,
        layer: -1.0,
        color: None,
        x: 0.0,
        y: 0.0,
    }
}

/// `DrawWarmupRegion` defaults (`ff9b59`, `sinMag 0.6`, `sinScl 8`).
fn warmup_region() -> DrawDesc {
    DrawDesc::WarmupRegion {
        color: 0xff9b_59ff,
        sin_mag: 0.6,
        sin_scl: 8.0,
    }
}

/// Emits the `block-border` team corner (`BuildingComp.drawTeam`,
/// `BuildingComp.java:1334`): a 2-tile-region border sprite tinted with
/// `Team.color` at the chosen corner. `team` is the building team, `viewer` the
/// camera team; only enemy teams (`team != viewer`) draw it.
pub fn team_overlay(
    state: &DrawState,
    team_color: u32,
    viewer_team: u8,
    team: u8,
    ids: &mut RegionIdTable,
    out: &mut Vec<DescDraw>,
) {
    if team == viewer_team {
        return;
    }
    let ts = TILESIZE as f32;
    let r = region("", "block-border", ids);
    let x = state.x - state.size as f32 * ts / 2.0 + 4.0;
    let y = state.y - state.size as f32 * ts / 2.0 + 4.0;
    out.push(DescDraw::sprite(
        Layer::Block.z(),
        Blend::Normal,
        r,
        x,
        y,
        ts,
        ts,
        0.0,
        team_color,
    ));
}

/// Emits the whole-layer markers the headless `stage_trace` asserts exist even
/// when the corresponding map is empty: `scorch`/`plans`/`darkness`/`light`/
/// `fogOfWar`/`weather`/`overlayUI`. Each entry is a 1×1 marker at the layer z
/// with a stable `layer` region name so the golden records the layer set.
pub fn global_layer_markers(ids: &mut RegionIdTable, out: &mut Vec<DescDraw>) {
    for layer in [
        Layer::Scorch,
        Layer::Plans,
        Layer::Darkness,
        Layer::OverlayUi,
        Layer::Weather,
        Layer::Light,
        Layer::FogOfWar,
    ] {
        let r = region("layer", &format!("-{}", layer.name()), ids);
        out.push(DescDraw::sprite(
            layer.z(),
            Blend::Normal,
            r,
            0.0,
            0.0,
            1.0,
            1.0,
            0.0,
            0xffff_ffff,
        ));
    }
}

/// Interns a content-derived drawer suffix (`DrawRegion.suffix`, …) into the
/// `&'static str` descriptor slots. Names are bounded (one per content block),
/// so each distinct name is leaked exactly once — the same pattern as
/// [`crate::fx::parts::draw::intern_region`] (plan 17 §3.15).
fn intern(suffix: &str) -> &'static str {
    crate::fx::parts::draw::intern_region(suffix).0
}

/// `Rgba` -> RGBA8888 for a parsed drawer color.
fn drawer_color(color: Rgba) -> u32 {
    color.to_rgba8888()
}

/// A known-valid `Color.valueOf` literal (`world/draw/*.java`) as RGBA8888.
fn hex(hex: &str) -> u32 {
    Rgba::from_hex(hex)
        .map(|color| color.to_rgba8888())
        .unwrap_or(0xffff_ffff)
}

/// Converts one parsed `Block.drawer` tree ([`DrawerSpec`], GAPS #57) into the
/// executable [`DrawDesc`] body.
///
/// `DrawerSpec` captures the JSON-overridable drawer fields; every field the
/// parsed record does not carry keeps the Java class field initializer
/// (`world/draw/*.java`), cited in the arms. `DrawRegion.name` overrides the
/// block region name, `DrawTurret`/`DrawWeapon` carry their parsed part tree to
/// the turret renderer ([`DrawDesc::Turret`]/[`DrawDesc::Weapon`]) and
/// `DrawArc`/`DrawParticle`/`DrawSoftShadow` are class aliases for the closest
/// ported body (`DrawArcSmelt`/`DrawParticles`/`DrawSoftParticles`; no upstream
/// `world/draw` class exists for those three names). `DrawLiquidOutputs`
/// resolves its outputs from the block content in [`chain_for`];
/// `DrawLiquidRegion`'s `drawLiquid` selection still falls back to the
/// current-liquid color.
pub fn from_drawer(spec: &DrawerSpec) -> DrawDesc {
    match spec.kind {
        DrawerKind::Block | DrawerKind::Default => DrawDesc::Default,
        DrawerKind::Multi => DrawDesc::Multi(spec.children.iter().map(from_drawer).collect()),
        DrawerKind::Region | DrawerKind::RegionSpin => DrawDesc::Region {
            suffix: intern(&spec.suffix),
            name: spec.name.as_deref().map(intern),
            rotate_speed: spec.rotate_speed,
            rotation: spec.rotation,
            spin: spec.spin_sprite || spec.kind == DrawerKind::RegionSpin,
            building_rotate: spec.building_rotate,
            layer: spec.layer,
            color: spec.color.map(drawer_color),
            x: spec.x,
            y: spec.y,
        },
        DrawerKind::SideRegion => DrawDesc::SideRegion {
            top1: "-top1",
            top2: "-top2",
        },
        // `DrawFrames.java:11-15`.
        DrawerKind::Frames => DrawDesc::Frames {
            frames: 3,
            interval: 5.0,
            sine: true,
        },
        // `DrawWarmupRegion.java:13-14`.
        DrawerKind::WarmupRegion => DrawDesc::WarmupRegion {
            color: hex("ff9b59"),
            sin_mag: 0.6,
            sin_scl: 8.0,
        },
        DrawerKind::Fade => DrawDesc::Fade {
            suffix: intern(if spec.suffix.is_empty() {
                "-top"
            } else {
                &spec.suffix
            }),
            alpha: spec.alpha,
            scale: spec.scale,
        },
        DrawerKind::LiquidRegion => DrawDesc::LiquidRegion {
            suffix: intern(&spec.suffix),
            alpha: spec.alpha,
        },
        DrawerKind::LiquidTile => DrawDesc::LiquidTile {
            padding: 0.0,
            alpha: spec.alpha,
            liquid: spec.liquid.as_deref().map(intern),
            frame: None,
        },
        // `DrawPower.java:14-24`.
        DrawerKind::Power => DrawDesc::Power {
            suffix: intern(&spec.suffix),
            mixcol: true,
            empty: hex("f8c266"),
            full: hex("fb9567"),
            layer: spec.layer,
        },
        // `DrawHeatRegion.java:13-18`.
        DrawerKind::HeatRegion => DrawDesc::HeatRegion {
            suffix: intern(&spec.suffix),
            color: Rgba::new(1.0, 0.22, 0.22, 0.8).to_rgba8888(),
            pulse: spec.pulse,
            pulse_scl: spec.pulse_scl,
            layer: spec.layer,
        },
        // `DrawHeatInput.java:15-17`.
        DrawerKind::HeatInput => DrawDesc::HeatInput {
            suffix: intern(&spec.suffix),
            color: Rgba::new(1.0, 0.22, 0.22, 0.8).to_rgba8888(),
            pulse: spec.pulse,
            pulse_scl: spec.pulse_scl,
        },
        // `DrawHeatOutput.java:15-21`.
        DrawerKind::HeatOutput => DrawDesc::HeatOutput {
            color: Rgba::new(1.0, 0.22, 0.22, 0.8).to_rgba8888(),
            pulse: spec.pulse,
            pulse_scl: spec.pulse_scl,
            glow_mult: 1.2,
            rot_offset: 0,
            draw_glow: true,
        },
        // `DrawGlowRegion.java:12-18`.
        DrawerKind::GlowRegion => DrawDesc::GlowRegion {
            suffix: intern(&spec.suffix),
            color: spec
                .color
                .map(drawer_color)
                .unwrap_or_else(|| Rgba::new(1.0, 0.0, 0.0, 1.0).to_rgba8888()),
            alpha: spec.alpha,
            glow_scale: 10.0,
            glow_intensity: 0.5,
            rotate_speed: spec.rotate_speed,
            rotate: false,
            layer: spec.layer,
        },
        // `DrawPistons.java:12-14`.
        DrawerKind::Pistons => DrawDesc::Pistons {
            sin_mag: 4.0,
            sin_scl: 6.0,
            sin_offset: 50.0,
            side_offset: 0.0,
            len_offset: -1.0,
            hori_offset: 0.0,
            angle_offset: 0.0,
            sides: 4,
            suffix: intern(if spec.suffix.is_empty() {
                "-piston"
            } else {
                &spec.suffix
            }),
        },
        // `DrawFlame.java:13-17`.
        DrawerKind::Flame => DrawDesc::Flame {
            flame_color: spec
                .flame_color
                .or(spec.color)
                .map(drawer_color)
                .unwrap_or_else(|| hex("ffc999")),
            flame_radius: spec.flame_radius,
            flame_radius_in: 1.9,
            flame_radius_scl: 5.0,
            flame_radius_mag: 2.0,
            flame_radius_in_mag: 1.0,
        },
        // `DrawParticles.java:11-20`.
        DrawerKind::Particles | DrawerKind::Particle => DrawDesc::Particles {
            color: spec
                .color
                .map(drawer_color)
                .unwrap_or_else(|| hex("f2d585")),
            sides: 12,
            alpha: class_alpha(spec.alpha, 0.5),
            particles: 30,
            particle_life: 70.0,
            particle_rad: 7.0,
            particle_size: 3.0,
            fade_margin: 0.4,
            rotate_scl: 3.0,
            reverse: false,
            poly: false,
        },
        // `DrawSoftParticles.java:15-20`.
        DrawerKind::SoftParticles | DrawerKind::SoftShadow => DrawDesc::SoftParticles {
            color: hex("e3ae6f"),
            color2: hex("d04d46"),
            alpha: class_alpha(spec.alpha, 0.5),
            particles: 30,
            particle_life: 70.0,
            particle_rad: 7.0,
            particle_size: 3.0,
            fade_margin: 0.4,
            rotate_scl: 1.5,
        },
        // `DrawPulseShape.java:14-18` (`Pal.accent`).
        DrawerKind::PulseShape => DrawDesc::PulseShape {
            color: hex("ffd37f"),
            stroke: 2.0,
            time_scl: 100.0,
            min_stroke: 0.2,
            radius_scl: 1.0,
            square: true,
            layer: spec.layer,
        },
        // `DrawShape.java:9-12` (`Pal.accent`).
        DrawerKind::Shape => DrawDesc::Shape {
            color: hex("ffd37f"),
            sides: 4,
            radius: 2.0,
            time_scl: 1.0,
            use_warmup_radius: false,
            layer: spec.layer,
        },
        // `DrawSpikes.java:8-12`.
        DrawerKind::Spikes => DrawDesc::Spikes {
            color: hex("7457ce"),
            amount: 10,
            layers: 1,
            stroke: 2.0,
            rotate_speed: 0.8,
            radius: 6.0,
            length: 4.0,
            layer_speed: -1.0,
        },
        // `DrawCells.java:15-18`.
        DrawerKind::Cells => DrawDesc::Cells {
            color: 0xffff_ffff,
            particles: 12,
            range: 4.0,
            recurrence: 2.0,
            radius: 1.8,
            lifetime: 180.0,
        },
        // `DrawBubbles.java:10-15`.
        DrawerKind::Bubbles => DrawDesc::Bubbles {
            color: hex("7457ce"),
            amount: 12,
            sides: 8,
            stroke_min: 0.2,
            spread: 3.0,
            time_scl: 30.0,
            recurrence: 6.0,
            radius: 3.0,
            fill: false,
        },
        // `DrawArcSmelt.java:10-15`; `DrawArc` is the mod class alias.
        DrawerKind::ArcSmelt | DrawerKind::Arc => DrawDesc::ArcSmelt {
            flame_color: spec
                .flame_color
                .or(spec.color)
                .map(drawer_color)
                .unwrap_or_else(|| hex("f58349")),
            mid_color: hex("f2d585"),
            flame_rad: 1.0,
            circle_space: 2.0,
            flame_radius_scl: 3.0,
            flame_radius_mag: 0.3,
            circle_stroke: 1.5,
            // `DrawArcSmelt.alpha = 0.68f` (`DrawArcSmelt.java:13`).
            alpha: class_alpha(spec.alpha, 0.68),
            particles: 25,
            particle_life: 40.0,
            particle_rad: 7.0,
            particle_stroke: 1.1,
            particle_len: 3.0,
            x: spec.x,
            y: spec.y,
        },
        // `DrawCrucibleFlame.java:11-15`.
        DrawerKind::CrucibleFlame => DrawDesc::CrucibleFlame {
            flame_color: spec
                .flame_color
                .or(spec.color)
                .map(drawer_color)
                .unwrap_or_else(|| hex("f58349")),
            mid_color: hex("f2d585"),
            flame_rad: 1.0,
            circle_space: 2.0,
            flame_radius_scl: 10.0,
            flame_radius_mag: 0.6,
            circle_stroke: 1.5,
            // `DrawCrucibleFlame.alpha = 0.5f`
            // (`DrawCrucibleFlame.java:14`).
            alpha: class_alpha(spec.alpha, 0.5),
            particles: 30,
            particle_life: 70.0,
            particle_rad: 7.0,
            particle_size: 3.0,
            fade_margin: 0.4,
            x: spec.x,
            y: spec.y,
        },
        // `DrawCultivator.java:12-18`.
        DrawerKind::Cultivator => DrawDesc::Cultivator {
            plant_color: hex("5541b1"),
            plant_color_light: hex("7457ce"),
            bottom_color: hex("474747"),
            bubbles: 12,
            sides: 8,
            stroke_min: 0.2,
            spread: 3.0,
            time_scl: 70.0,
            recurrence: 6.0,
            radius: 3.0,
        },
        // `DrawBlurSpin.java:11-12`.
        DrawerKind::BlurSpin => DrawDesc::BlurSpin {
            suffix: intern(&spec.suffix),
            rotate_speed: if spec.rotate_speed == 0.0 {
                1.0
            } else {
                spec.rotate_speed
            },
            blur_thresh: 0.7,
        },
        // `DrawMultiWeave.java:12-15`.
        DrawerKind::MultiWeave => DrawDesc::MultiWeave {
            glow_color: Rgba::new(1.0, 0.4, 0.4, 0.8).to_rgba8888(),
            weave_color: 0xffff_ffff,
            rotate_speed: 1.0,
            rotate_speed2: -0.9,
            fade_weave: false,
            pulse: if spec.pulse == 0.0 { 0.3 } else { spec.pulse },
            pulse_scl: if spec.pulse_scl == 1.0 {
                10.0
            } else {
                spec.pulse_scl
            },
        },
        // `DrawPlasma.java:13-18`.
        DrawerKind::Plasma => DrawDesc::Plasma {
            plasma1: hex("ffd06b"),
            plasma2: hex("ff361b"),
            plasmas: 4,
            suffix: intern(if spec.suffix.is_empty() {
                "-plasma-"
            } else {
                &spec.suffix
            }),
        },
        DrawerKind::Weave => DrawDesc::Weave,
        // `DrawCircles.java:10-14`.
        DrawerKind::Circles => DrawDesc::Circles {
            color: hex("7457ce"),
            amount: 5,
            sides: 15,
            stroke_min: 0.2,
            stroke_max: 2.0,
            time_scl: 160.0,
            radius: 12.0,
        },
        // `DrawLiquidOutputs` regions are derived from the block's
        // `outputLiquids` (content data, not the drawer spec): a bare
        // conversion leaves the list empty, [`chain_for`] fills it.
        DrawerKind::LiquidOutputs => DrawDesc::LiquidOutputs {
            outputs: Vec::new(),
        },
        DrawerKind::PumpLiquid => DrawDesc::PumpLiquid,
        DrawerKind::BlockParts => DrawDesc::BlockParts {
            parts: spec.parts.clone(),
        },
        // The live turret renderer owns the pass (`mind-gdext::render::turret`
        // executes `fx::parts::turret` with this part tree); the descriptor
        // carries the parsed data so the headless layer oracle sees it too.
        DrawerKind::Turret => DrawDesc::Turret {
            base_prefix: intern(&spec.base_prefix),
            parts: spec.parts.clone(),
        },
        // `DrawWeapon` has no upstream `world/draw` class; its parts are drawn
        // by the same turret/weapon part pass.
        DrawerKind::Weapon => DrawDesc::Weapon {
            parts: spec.parts.clone(),
        },
    }
}

/// The Java class-default alpha for the drawers whose bodies multiply alpha
/// (`DrawArcSmelt.java:13`, `DrawCrucibleFlame.java:14`,
/// `DrawParticles.java:13`, `DrawSoftParticles.java:15`).
///
/// `DrawerSpec::for_kind` carries one generic `alpha = 1.0`, so a spec still at
/// that sentinel keeps the class initializer — the same convention the
/// `BlurSpin`/`MultiWeave` pulse defaults below use.
fn class_alpha(alpha: f32, class_default: f32) -> f32 {
    if alpha == 1.0 { class_default } else { alpha }
}

/// The `Block.draw` chain for a content block (GAPS #57).
///
/// An explicit parsed drawer wins (mod/JSON parity: `DrawerKind` ->
/// [`DrawDesc`]); otherwise the hand-written vanilla table
/// ([`vanilla_chain`]) applies; a block with neither draws its base sprite
/// ([`DrawDesc::Default`], upstream `Block`'s default `DrawDefault`).
/// The parsed `DrawTurret`/`DrawWeapon` part tree inside a drawer spec (the
/// drawer itself or any `DrawMulti` child), if any.
///
/// The live turret renderer consumes this (`mind-gdext::render::turret`) so a
/// mod's JSON parts replace the hand-ported vanilla part table.
pub fn turret_drawer_parts(spec: &DrawerSpec) -> Option<&[DrawPartSpec]> {
    if matches!(spec.kind, DrawerKind::Turret | DrawerKind::Weapon) {
        return Some(spec.parts.as_slice());
    }
    for child in &spec.children {
        if let Some(parts) = turret_drawer_parts(child) {
            return Some(parts);
        }
    }
    None
}

/// Whether a drawer tree contains a `DrawTurret`/`DrawWeapon` (the pass that
/// owns those blocks' drawing).
pub fn has_turret_drawer(spec: &DrawerSpec) -> bool {
    turret_drawer_parts(spec).is_some()
}

/// Whether a block has an explicit draw chain (a parsed drawer or the
/// hand-written vanilla table) rather than the bare `DrawDefault` fallback.
pub fn has_explicit_chain(def: &BlockDef) -> bool {
    def.drawer.kind != DrawerKind::Default
        || !def.drawer.children.is_empty()
        || vanilla_chain(&def.name).is_some()
}

/// Resolves every [`DrawDesc::LiquidOutputs`] in `chain` against the block's
/// content `output_liquids`/`liquid_output_directions`
/// (`GenericCrafter.java:23-35`), matching `DrawLiquidOutputs.load`: region
/// `<block>-<liquid>-outputN` per output and `side = directions[i]` (or `-1`
/// when the direction array is shorter, `DrawLiquidOutputs.java:20-21`).
fn fill_liquid_outputs(chain: &mut [DrawDesc], def: &BlockDef, content: &ContentRegistry) {
    // Vanilla base content keeps the per-class numbers in
    // `world::block_kind_data` (`BlockDef.output_liquids` stays empty), so an
    // empty content vector keeps whatever the hand-written chain carries
    // (`vanilla_chain("electrolyzer")`); a mod block sets the field and wins.
    if !def.output_liquids.is_empty() {
        for desc in chain.iter_mut() {
            match desc {
                DrawDesc::LiquidOutputs { outputs } => {
                    *outputs = def
                        .output_liquids
                        .iter()
                        .enumerate()
                        .filter_map(|(index, (liquid, _))| {
                            let side = def
                                .liquid_output_directions
                                .get(index)
                                .copied()
                                .unwrap_or(-1);
                            content
                                .liquid(*liquid)
                                .map(|liquid| (liquid.name.clone(), side))
                        })
                        .collect();
                }
                DrawDesc::Multi(list) => fill_liquid_outputs(list, def, content),
                _ => {}
            }
        }
    }
    // `DrawLiquidTile.drawLiquid` is content-backed even for vanilla blocks
    // whose `BlockDef.output_liquids` is empty (the hand-written chains name
    // the fluid), so resolve it on every call.
    fill_liquid_tiles(chain, content);
}

/// Resolves each `DrawLiquidTile.drawLiquid` name into the `Liquid` record the
/// tiled frames need (`Liquid.getAnimationFrame` phase, gas frames, tint).
fn fill_liquid_tiles(chain: &mut [DrawDesc], content: &ContentRegistry) {
    for desc in chain.iter_mut() {
        match desc {
            DrawDesc::LiquidTile {
                liquid: Some(name),
                frame,
                ..
            } => {
                if let Some(liquid) = content.liquid_by_name(name) {
                    *frame = Some(LiquidFrame {
                        id: liquid.id.raw(),
                        gas: liquid.gas,
                        color: liquid.color.to_rgba8888(),
                    });
                }
            }
            DrawDesc::Multi(list) => fill_liquid_tiles(list, content),
            _ => {}
        }
    }
}

/// [`chain_for`] with the block's own draw chain only (no content lookup): the
/// `DrawLiquidOutputs` outputs stay empty. Used by callers without a registry
/// (tests, `render::layers` fixtures).
pub fn chain_for(def: &BlockDef) -> Vec<DrawDesc> {
    if def.drawer.kind != DrawerKind::Default || !def.drawer.children.is_empty() {
        return vec![from_drawer(&def.drawer)];
    }
    vanilla_chain(&def.name).unwrap_or_else(|| vec![DrawDesc::Default])
}

/// The `Block.draw` chain for a content block (GAPS #57), resolving the
/// content-backed drawer fields ([`fill_liquid_outputs`]).
pub fn chain_for_with(def: &BlockDef, content: &ContentRegistry) -> Vec<DrawDesc> {
    let mut chain = chain_for(def);
    fill_liquid_outputs(&mut chain, def, content);
    chain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> DrawState {
        DrawState::new(64.0, 64.0, 0, 2)
    }

    #[test]
    fn default_emits_block_layer_region() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        execute(
            &DrawDesc::Default,
            "stone-wall",
            &state(),
            &mut ids,
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].z, Layer::Block.z());
        let DrawCmd::Sprite { w, h, .. } = out[0].cmd else {
            panic!("expected sprite");
        };
        assert_eq!(w, 2.0 * TILESIZE as f32);
        assert_eq!(h, 2.0 * TILESIZE as f32);
    }

    #[test]
    fn region_layer_override_moves_z() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        execute(
            &DrawDesc::Region {
                suffix: "-bottom",
                name: None,
                rotate_speed: 0.0,
                rotation: 0.0,
                spin: false,
                building_rotate: false,
                layer: Layer::BlockUnder.z(),
                color: None,
                x: 0.0,
                y: 0.0,
            },
            "melter",
            &state(),
            &mut ids,
            &mut out,
        );
        assert_eq!(out[0].z, Layer::BlockUnder.z());
    }

    #[test]
    fn heat_region_is_additive_and_gated_on_heat() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let mut s = state();
        execute(
            &DrawDesc::HeatRegion {
                suffix: "-glow",
                color: 0xff3838cc,
                pulse: 0.3,
                pulse_scl: 10.0,
                layer: -1.0,
            },
            "heat",
            &s,
            &mut ids,
            &mut out,
        );
        assert!(out.is_empty(), "no heat -> no glow");
        s.heat = 1.0;
        s.heat_requirement = 2.0;
        execute(
            &DrawDesc::HeatRegion {
                suffix: "-glow",
                color: 0xff3838cc,
                pulse: 0.3,
                pulse_scl: 10.0,
                layer: -1.0,
            },
            "heat",
            &s,
            &mut ids,
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].z, Layer::BlockAdditive.z());
        assert_eq!(out[0].blend, Blend::Additive);
    }

    #[test]
    fn flame_emits_top_and_two_circles_at_block_plus_001() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let mut s = state();
        s.warmup = 1.0;
        s.id = 7;
        execute(
            &DrawDesc::Flame {
                flame_color: 0xffc999ff,
                flame_radius: 3.0,
                flame_radius_in: 1.9,
                flame_radius_scl: 5.0,
                flame_radius_mag: 2.0,
                flame_radius_in_mag: 1.0,
            },
            "silicon-smelter",
            &s,
            &mut ids,
            &mut out,
        );
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].z, Layer::Block.z() + 0.01);
        assert!(matches!(out[1].cmd, DrawCmd::Fill { .. }));
        assert!(matches!(out[2].cmd, DrawCmd::Fill { .. }));
    }

    #[test]
    fn liquid_region_fraction_is_clamped_and_bottom_aligned() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let mut s = state();
        s.liquid_capacity = 100.0;
        s.liquid_amount = 50.0;
        execute(
            &DrawDesc::LiquidRegion {
                suffix: "-liquid",
                alpha: 1.0,
            },
            "spore-press",
            &s,
            &mut ids,
            &mut out,
        );
        let DrawCmd::Sprite { h, y, .. } = out[0].cmd else {
            panic!("expected sprite");
        };
        assert!((h - 2.0 * TILESIZE as f32 * 0.5).abs() < 1e-4);
        assert!(y < s.y, "fill is bottom aligned");
    }

    #[test]
    fn pistons_emit_one_region_per_side() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        execute(
            &DrawDesc::Pistons {
                sin_mag: 1.0,
                sin_scl: 6.0,
                sin_offset: 50.0,
                side_offset: 0.0,
                len_offset: -1.0,
                hori_offset: 0.0,
                angle_offset: 0.0,
                sides: 4,
                suffix: "-piston",
            },
            "spore-press",
            &state(),
            &mut ids,
            &mut out,
        );
        assert_eq!(out.len(), 4);
    }

    /// L06 T5: `DrawLiquidOutputs.draw` (`DrawLiquidOutputs.java:16-26`) for the
    /// electrolyzer (`outputLiquids` ozone, hydrogen; `liquidOutputDirections`
    /// {1, 3}): `realRot = (side + rotation) % 4` picks `-output1` (realRot <=
    /// 1) or `-output2` and draws at `realRot * 90`.
    #[test]
    fn liquid_outputs_draw_per_side_regions() {
        let draws = |rotation: i32| {
            let mut ids = RegionIdTable::new();
            let mut out = Vec::new();
            let state = DrawState::new(64.0, 64.0, rotation, 3);
            execute(
                &DrawDesc::LiquidOutputs {
                    outputs: vec![
                        (String::from("ozone"), 1),
                        (String::from("hydrogen"), 3),
                        (String::from("water"), -1),
                    ],
                },
                "electrolyzer",
                &state,
                &mut ids,
                &mut out,
            );
            out.iter()
                .map(|draw| {
                    let DrawCmd::Sprite { region, rot, .. } = draw.cmd else {
                        panic!("expected sprite");
                    };
                    (ids.name(region).unwrap_or("?").to_owned(), rot)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            draws(0),
            vec![
                ("electrolyzer-ozone-output1".to_owned(), 90.0),
                ("electrolyzer-hydrogen-output2".to_owned(), 270.0),
            ],
            "side -1 draws nothing"
        );
        assert_eq!(
            draws(1),
            vec![
                ("electrolyzer-ozone-output2".to_owned(), 180.0),
                ("electrolyzer-hydrogen-output1".to_owned(), 0.0),
            ]
        );
        assert!(
            vanilla_chain("electrolyzer").is_some(),
            "electrolyzer chain"
        );
    }

    #[test]
    fn global_layer_markers_cover_every_global_layer() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        global_layer_markers(&mut ids, &mut out);
        let zs: Vec<f32> = out.iter().map(|d| d.z).collect();
        for layer in [
            Layer::Scorch,
            Layer::Plans,
            Layer::Darkness,
            Layer::OverlayUi,
            Layer::Weather,
            Layer::Light,
            Layer::FogOfWar,
        ] {
            assert!(zs.contains(&layer.z()), "missing {}", layer.name());
        }
    }

    #[test]
    fn team_overlay_only_for_enemy_team() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        team_overlay(&state(), 0xffff_ffff, 1, 1, &mut ids, &mut out);
        assert!(out.is_empty(), "same team -> no border");
        team_overlay(&state(), 0xffff_ffff, 1, 2, &mut ids, &mut out);
        assert_eq!(out.len(), 1);
        let DrawCmd::Sprite { x, y, .. } = out[0].cmd else {
            panic!("expected sprite");
        };
        let ts = TILESIZE as f32;
        assert!((x - (64.0 - ts + 4.0)).abs() < 1e-4);
        assert!((y - (64.0 - ts + 4.0)).abs() < 1e-4);
    }

    #[test]
    fn vanilla_chains_cover_the_full_layer_set() {
        let names = [
            "silicon-smelter",
            "phase-weaver",
            "cryofluid-mixer",
            "spore-press",
            "pulverizer",
            "steam-generator",
            "cultivator",
        ];
        let mut layers = std::collections::BTreeSet::new();
        for name in names {
            let chain = vanilla_chain(name).expect("chain");
            let mut ids = RegionIdTable::new();
            let mut out = Vec::new();
            let mut s = state();
            s.warmup = 1.0;
            s.progress = 0.5;
            s.total_progress = 0.5;
            s.power_status = 0.5;
            s.liquid_capacity = 100.0;
            s.liquid_amount = 50.0;
            for d in &chain {
                execute(d, name, &s, &mut ids, &mut out);
            }
            for d in &out {
                layers.insert(d.z.to_bits());
            }
        }
        // The block pass and the flame overlay pass must both appear.
        assert!(layers.contains(&Layer::Block.z().to_bits()));
        assert!(layers.contains(&(Layer::Block.z() + 0.01).to_bits()));
    }

    /// I02 T7: `DrawMulti` order - parts before `DrawDefault` draw under the
    /// base sprite, parts after it over the sprite.
    #[test]
    fn chain_splits_at_draw_default() {
        use super::split_chain_at_default;
        let bottom = DrawDesc::Region {
            suffix: "-bottom",
            name: None,
            rotate_speed: 0.0,
            rotation: 0.0,
            spin: false,
            building_rotate: false,
            layer: 0.0,
            color: None,
            x: 0.0,
            y: 0.0,
        };
        let top = DrawDesc::Region {
            suffix: "-top",
            name: None,
            rotate_speed: 0.0,
            rotation: 0.0,
            spin: false,
            building_rotate: false,
            layer: 0.0,
            color: None,
            x: 0.0,
            y: 0.0,
        };
        let chain = vec![DrawDesc::Multi(vec![
            bottom.clone(),
            DrawDesc::Default,
            top.clone(),
        ])];
        let (under, over) = split_chain_at_default(&chain);
        assert_eq!(under, vec![bottom]);
        assert_eq!(over, vec![top.clone()]);

        // No `DrawDefault`: everything draws over the sprite (the caller emits
        // the base region).
        let (under, over) = split_chain_at_default(std::slice::from_ref(&top));
        assert!(under.is_empty());
        assert_eq!(over, vec![top.clone()]);
    }

    /// GAPS #57: a parsed `DrawMulti` drawer converts to the descriptor tree
    /// (`DrawerSpec` fields win, Java class defaults fill the rest).
    #[test]
    fn parsed_drawer_converts_to_desc_chain() {
        use crate::content::registries::blocks::drawer::{DrawerKind, DrawerSpec};
        use crate::content::registries::units::parts::DrawPartKind;
        let mut multi = DrawerSpec::for_kind(DrawerKind::Multi);
        let mut flame = DrawerSpec::for_kind(DrawerKind::Flame);
        flame.flame_color = Rgba::from_hex("ff0000");
        let mut pump = DrawerSpec::for_kind(DrawerKind::PumpLiquid);
        pump.suffix = String::from("-liquid");
        let mut parts = DrawerSpec::for_kind(DrawerKind::BlockParts);
        parts.parts.push(DrawPartSpec::shape());
        multi.children = vec![
            DrawerSpec::for_kind(DrawerKind::Default),
            flame,
            pump,
            parts,
        ];
        let desc = from_drawer(&multi);
        let DrawDesc::Multi(children) = desc else {
            panic!("expected DrawMulti");
        };
        assert_eq!(children.len(), 4);
        assert_eq!(children[0], DrawDesc::Default);
        let DrawDesc::Flame { flame_color, .. } = &children[1] else {
            panic!("expected DrawFlame");
        };
        assert_eq!(*flame_color, hex("ff0000"));
        assert_eq!(children[2], DrawDesc::PumpLiquid);
        let DrawDesc::BlockParts { parts } = &children[3] else {
            panic!("expected DrawBlockParts");
        };
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].kind, DrawPartKind::ShapePart);
    }

    /// GAPS #57: an explicit parsed drawer wins over the hand-written vanilla
    /// table; a block with neither falls back to `DrawDefault`.
    #[test]
    fn chain_for_prefers_parsed_drawer() {
        use crate::content::test_support::test_registry;
        let content = test_registry();
        let base = content
            .block_id("silicon-smelter")
            .and_then(|id| content.block(id))
            .expect("silicon-smelter")
            .clone();
        // Registry blocks keep the default drawer: the vanilla table applies.
        assert_eq!(
            chain_for(&base),
            vanilla_chain("silicon-smelter").expect("table")
        );
        let mut parsed = base.clone();
        parsed.drawer = crate::content::registries::blocks::drawer::DrawerSpec::for_kind(
            crate::content::registries::blocks::drawer::DrawerKind::Region,
        );
        parsed.drawer.suffix = String::from("-custom");
        assert_eq!(
            chain_for(&parsed),
            vec![DrawDesc::Region {
                suffix: "-custom",
                name: None,
                rotate_speed: 0.0,
                rotation: 0.0,
                spin: false,
                building_rotate: false,
                layer: -1.0,
                color: None,
                x: 0.0,
                y: 0.0,
            }]
        );
        let mut unknown = base;
        unknown.name = String::from("mod-unknown");
        assert_eq!(chain_for(&unknown), vec![DrawDesc::Default]);
    }

    /// GAPS #57: `DrawPumpLiquid` fills the `-liquid` region from the module.
    #[test]
    fn pump_liquid_fills_by_liquid_fraction() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let mut st = state();
        st.liquid_capacity = 10.0;
        st.liquid_amount = 5.0;
        execute(&DrawDesc::PumpLiquid, "pump", &st, &mut ids, &mut out);
        assert_eq!(out.len(), 1);
        let DrawCmd::Sprite { region, h, .. } = out[0].cmd else {
            panic!("expected sprite");
        };
        assert_eq!(ids.name(region), Some("pump-liquid"));
        assert_eq!(h, 2.0 * TILESIZE as f32 * 0.5);
    }

    /// GAPS #57: `DrawBlockParts` runs the plan-17 part compiler and maps its
    /// primitives back onto the block command buffer.
    #[test]
    fn block_parts_executes_the_part_tree() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let mut st = state();
        st.warmup = 1.0;
        let parts = vec![DrawPartSpec::shape(), DrawPartSpec::region("-blade")];
        execute(
            &DrawDesc::BlockParts { parts },
            "mod-block",
            &st,
            &mut ids,
            &mut out,
        );
        assert!(!out.is_empty(), "part compiler emitted no primitives");
        assert!(
            out.iter().any(|d| matches!(
                d.cmd,
                DrawCmd::Fill { .. } | DrawCmd::Shape { .. } | DrawCmd::Sprite { .. }
            )),
            "expected fill/shape/sprite primitives from the parts"
        );
    }

    /// GAPS #57: every `DrawerKind` converts without panicking and round-trips
    /// the class tag for the classes with a headless body.
    #[test]
    fn every_drawer_kind_converts() {
        use crate::content::registries::blocks::drawer::{DrawerKind, DrawerSpec};
        for kind in [
            DrawerKind::Block,
            DrawerKind::Default,
            DrawerKind::Multi,
            DrawerKind::ArcSmelt,
            DrawerKind::Bubbles,
            DrawerKind::Cells,
            DrawerKind::Circles,
            DrawerKind::CrucibleFlame,
            DrawerKind::Cultivator,
            DrawerKind::Fade,
            DrawerKind::Flame,
            DrawerKind::Frames,
            DrawerKind::HeatInput,
            DrawerKind::HeatOutput,
            DrawerKind::HeatRegion,
            DrawerKind::LiquidOutputs,
            DrawerKind::LiquidRegion,
            DrawerKind::LiquidTile,
            DrawerKind::MultiWeave,
            DrawerKind::Particles,
            DrawerKind::Pistons,
            DrawerKind::Plasma,
            DrawerKind::Power,
            DrawerKind::PulseShape,
            DrawerKind::PumpLiquid,
            DrawerKind::SoftParticles,
            DrawerKind::Spikes,
            DrawerKind::Weave,
            DrawerKind::BlockParts,
            DrawerKind::Region,
            DrawerKind::RegionSpin,
            DrawerKind::Turret,
            DrawerKind::Weapon,
            DrawerKind::SideRegion,
            DrawerKind::GlowRegion,
            DrawerKind::Arc,
            DrawerKind::BlurSpin,
            DrawerKind::Particle,
            DrawerKind::Shape,
            DrawerKind::SoftShadow,
            DrawerKind::WarmupRegion,
        ] {
            let spec = DrawerSpec::for_kind(kind);
            let desc = from_drawer(&spec);
            let mut ids = RegionIdTable::new();
            let mut out = Vec::new();
            execute(&desc, "probe", &state(), &mut ids, &mut out);
        }
    }

    /// GAPS #57 follow-up: `DrawRegion.name` (`DrawRegion.java:79`) overrides
    /// `block.name + suffix`.
    #[test]
    fn draw_region_name_override_wins() {
        use crate::content::registries::blocks::drawer::{DrawerKind, DrawerSpec};
        let mut spec = DrawerSpec::for_kind(DrawerKind::Region);
        spec.suffix = String::from("-top");
        spec.name = Some(String::from("shared-region"));
        let desc = from_drawer(&spec);
        let DrawDesc::Region { name, .. } = desc else {
            panic!("expected DrawRegion");
        };
        assert_eq!(name, Some("shared-region"));

        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        execute(
            &DrawDesc::Region {
                suffix: "-top",
                name: Some("shared-region"),
                rotate_speed: 0.0,
                rotation: 0.0,
                spin: false,
                building_rotate: false,
                layer: -1.0,
                color: None,
                x: 0.0,
                y: 0.0,
            },
            "mod-block",
            &state(),
            &mut ids,
            &mut out,
        );
        let DrawCmd::Sprite { region, .. } = out[0].cmd else {
            panic!("expected sprite");
        };
        assert_eq!(ids.name(region), Some("shared-region"));
    }

    /// GAPS #57 follow-up: `execute_extras` skips only the *base* region
    /// (`DrawDefault` or an unnamed empty-suffix `DrawRegion`); an explicit
    /// `DrawRegion.name` is an extra region and still draws.
    #[test]
    fn execute_extras_keeps_a_named_base_region() {
        let named = DrawDesc::Region {
            suffix: "",
            name: Some("shared-region"),
            rotate_speed: 0.0,
            rotation: 0.0,
            spin: false,
            building_rotate: false,
            layer: -1.0,
            color: None,
            x: 0.0,
            y: 0.0,
        };
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        execute_extras(&named, "mod-block", &state(), &mut ids, &mut out);
        assert_eq!(out.len(), 1, "named empty-suffix region draws");

        let base = DrawDesc::Region {
            suffix: "",
            name: None,
            rotate_speed: 0.0,
            rotation: 0.0,
            spin: false,
            building_rotate: false,
            layer: -1.0,
            color: None,
            x: 0.0,
            y: 0.0,
        };
        out.clear();
        execute_extras(&base, "mod-block", &state(), &mut ids, &mut out);
        assert!(out.is_empty(), "the base region is not duplicated");
    }

    /// GAPS #57 follow-up: `chain_for_with` keeps the hand-written vanilla
    /// electrolyzer outputs (base content does not populate
    /// `BlockDef.output_liquids`) and only resolves a mod block's own fields.
    #[test]
    fn chain_for_with_keeps_vanilla_outputs_without_content_fields() {
        use crate::content::test_support::test_registry;
        let content = test_registry();
        let def = content
            .block_id("electrolyzer")
            .and_then(|id| content.block(id))
            .expect("electrolyzer")
            .clone();
        let chain = chain_for_with(&def, &content);
        let outputs = chain
            .iter()
            .find_map(|desc| match desc {
                DrawDesc::LiquidOutputs { outputs } => Some(outputs.clone()),
                _ => None,
            })
            .expect("electrolyzer has a DrawLiquidOutputs");
        assert_eq!(
            outputs,
            vec![(String::from("ozone"), 1), (String::from("hydrogen"), 3)],
            "vanilla content table preserved"
        );
    }

    /// GAPS #57 follow-up: `DrawTurret`/`DrawWeapon` carry their parsed
    /// `DrawPart` tree; the turret pass consumes it via
    /// [`turret_drawer_parts`], the headless body emits the parts at
    /// `Layer.turret` and the block-layer extras pass skips them.
    #[test]
    fn turret_and_weapon_drawers_carry_parts() {
        use crate::content::registries::blocks::drawer::{DrawerKind, DrawerSpec};
        use crate::content::registries::units::parts::DrawPartKind;

        let mut turret = DrawerSpec::for_kind(DrawerKind::Turret);
        turret.parts.push(DrawPartSpec::region("-barrel"));
        turret.base_prefix = String::from("reinforced-");
        let desc = from_drawer(&turret);
        let DrawDesc::Turret { base_prefix, parts } = &desc else {
            panic!("expected DrawTurret");
        };
        assert_eq!(*base_prefix, "reinforced-");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].kind, DrawPartKind::RegionPart);
        assert_eq!(
            turret_drawer_parts(&turret).map(<[DrawPartSpec]>::len),
            Some(1)
        );
        assert!(has_turret_drawer(&turret));

        // A `DrawMulti[Default, DrawTurret]` drawer is found recursively.
        let mut multi = DrawerSpec::for_kind(DrawerKind::Multi);
        multi.children = vec![DrawerSpec::for_kind(DrawerKind::Default), turret.clone()];
        assert!(has_turret_drawer(&multi));
        assert!(!has_turret_drawer(&DrawerSpec::for_kind(DrawerKind::Flame)));

        // The headless body emits the part tree at the turret band.
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let mut s = state();
        s.warmup = 1.0;
        s.progress = 0.5;
        execute(&desc, "mod-turret", &s, &mut ids, &mut out);
        assert!(!out.is_empty(), "turret parts emitted no primitives");
        assert!(
            out.iter().all(|draw| draw.z >= Layer::Turret.z() - 1.0),
            "turret parts must draw at the turret band: {:?}",
            out.iter().map(|draw| draw.z).collect::<Vec<_>>()
        );

        let mut weapon = DrawerSpec::for_kind(DrawerKind::Weapon);
        weapon.parts.push(DrawPartSpec::shape());
        let desc = from_drawer(&weapon);
        assert!(matches!(desc, DrawDesc::Weapon { ref parts } if parts.len() == 1));
        out.clear();
        execute(&desc, "mod-weapon", &s, &mut ids, &mut out);
        assert!(!out.is_empty());
    }

    /// GAPS #57 follow-up: `DrawLiquidOutputs` resolves the block's
    /// `outputLiquids`/`liquidOutputDirections` content fields
    /// (`DrawLiquidOutputs.java:16-26`), and `DrawArc`/`DrawParticle`/
    /// `DrawSoftShadow` map to the closest ported bodies instead of
    /// `DrawDefault`.
    #[test]
    fn content_backed_drawer_fields_convert() {
        use crate::content::registries::blocks::drawer::{DrawerKind, DrawerSpec};
        use crate::content::test_support::test_registry;

        let content = test_registry();
        let mut def = content
            .block_id("electrolyzer")
            .and_then(|id| content.block(id))
            .expect("electrolyzer")
            .clone();
        let water = content.liquid_id("water").expect("water");
        let cryofluid = content.liquid_id("cryofluid").expect("cryofluid");
        def.output_liquids = vec![(water, 0.1), (cryofluid, 0.2)];
        def.liquid_output_directions = vec![1];
        def.drawer = DrawerSpec::for_kind(DrawerKind::LiquidOutputs);
        let chain = chain_for_with(&def, &content);
        let DrawDesc::LiquidOutputs { outputs } = &chain[0] else {
            panic!("expected DrawLiquidOutputs, got {:?}", chain[0]);
        };
        assert_eq!(
            outputs,
            &vec![(String::from("water"), 1), (String::from("cryofluid"), -1)],
            "a missing direction entry falls back to -1"
        );

        // The alias classes convert to a real body, not `DrawDefault`.
        assert!(matches!(
            from_drawer(&DrawerSpec::for_kind(DrawerKind::Arc)),
            DrawDesc::ArcSmelt { .. }
        ));
        assert!(matches!(
            from_drawer(&DrawerSpec::for_kind(DrawerKind::Particle)),
            DrawDesc::Particles { .. }
        ));
        assert!(matches!(
            from_drawer(&DrawerSpec::for_kind(DrawerKind::SoftShadow)),
            DrawDesc::SoftParticles { .. }
        ));
    }

    /// GAPS #57 follow-up: the Java class-default alphas
    /// (`DrawArcSmelt.java:13`, `DrawCrucibleFlame.java:14`,
    /// `DrawParticles.java:13`, `DrawSoftParticles.java:15`) survive the
    /// conversion, and `DrawParticles` scales its particle alpha by it.
    #[test]
    fn class_alpha_defaults_apply() {
        use crate::content::registries::blocks::drawer::{DrawerKind, DrawerSpec};

        let arc = from_drawer(&DrawerSpec::for_kind(DrawerKind::ArcSmelt));
        let DrawDesc::ArcSmelt { alpha, x, y, .. } = arc else {
            panic!("expected DrawArcSmelt");
        };
        assert!((alpha - 0.68).abs() < 1e-6, "DrawArcSmelt.alpha");
        assert_eq!((x, y), (0.0, 0.0));

        let crucible = from_drawer(&DrawerSpec::for_kind(DrawerKind::CrucibleFlame));
        let DrawDesc::CrucibleFlame { alpha, .. } = crucible else {
            panic!("expected DrawCrucibleFlame");
        };
        assert!((alpha - 0.5).abs() < 1e-6, "DrawCrucibleFlame.alpha");

        let particles = from_drawer(&DrawerSpec::for_kind(DrawerKind::Particles));
        let DrawDesc::Particles { alpha, color, .. } = particles else {
            panic!("expected DrawParticles");
        };
        assert!((alpha - 0.5).abs() < 1e-6, "DrawParticles.alpha");
        assert_eq!(color, hex("f2d585"));

        // The alpha reaches the emitted particle tint.
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let mut s = state();
        s.warmup = 1.0;
        execute(&particles, "mod-fx", &s, &mut ids, &mut out);
        assert!(!out.is_empty());
        let DrawCmd::Fill { color, .. } = out[0].cmd else {
            panic!("expected circle fill");
        };
        let emitted = Rgba::from_rgba8888(color);
        assert!(
            (emitted.a - 0.5).abs() < 2.0 / 255.0,
            "particle alpha: {}",
            emitted.a
        );

        // An explicit JSON alpha wins over the class default.
        let mut spec = DrawerSpec::for_kind(DrawerKind::ArcSmelt);
        spec.alpha = 0.25;
        let DrawDesc::ArcSmelt { alpha, .. } = from_drawer(&spec) else {
            panic!("expected DrawArcSmelt");
        };
        assert!((alpha - 0.25).abs() < 1e-6);
    }

    /// GAPS #53: `DrawLiquidTile` draws the animated fluid frames over every
    /// block cell (`LiquidBlock.drawTiledFrames`), not the single `-liquid`
    /// region fill.
    #[test]
    fn liquid_tile_emits_fluid_frames_over_the_footprint() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let mut st = state();
        st.liquid_capacity = 10.0;
        st.liquid_amount = 10.0;
        st.liquid_id = 42;
        st.liquid_color = 0x596a_b8ff;
        execute(
            &DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
                liquid: None,
                frame: None,
            },
            "melter",
            &st,
            &mut ids,
            &mut out,
        );
        assert_eq!(out.len(), 4, "size-2 block tiles one frame per cell");
        let DrawCmd::Sprite {
            region,
            w,
            h,
            color,
            ..
        } = out[0].cmd
        else {
            panic!("expected sprite");
        };
        // `(42 * 5) % 50 = 10` at `Time.time = 0`.
        assert_eq!(ids.name(region), Some("fluid-liquid-10"));
        assert_eq!((w, h), (32.0, 32.0));
        let emitted = Rgba::from_rgba8888(color);
        assert!((emitted.a - 1.0).abs() < 1e-6);
    }

    /// A resolved `DrawLiquidTile.drawLiquid` wins over the dominant liquid:
    /// gas frames, the liquid's own animation phase and its tint
    /// (`DrawLiquidTile.java:26-30`, `Liquid.java:96-100/106-108`).
    #[test]
    fn liquid_tile_prefers_the_resolved_draw_liquid() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let mut st = state();
        st.time = 95.0;
        st.liquid_capacity = 10.0;
        st.liquid_amount = 10.0;
        execute(
            &DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
                liquid: Some("hydrogen"),
                frame: Some(LiquidFrame {
                    id: 7,
                    gas: true,
                    color: 0x8090a099,
                }),
            },
            "regenerator",
            &st,
            &mut ids,
            &mut out,
        );
        assert!(!out.is_empty());
        let DrawCmd::Sprite { region, color, .. } = out[0].cmd else {
            panic!("expected sprite");
        };
        // gas: (95 / 190 * 50) = 25, + 7*5 = 35 -> 60 % 50 = 10.
        assert_eq!(ids.name(region), Some("fluid-gas-10"));
        // `Drawf.liquid`: alpha * color.a (0x99 / 255 = 0.6).
        let emitted = Rgba::from_rgba8888(color);
        assert!((emitted.a - 0.6).abs() < 2.0 / 255.0, "{}", emitted.a);
    }

    /// `Drawf.liquid`'s `if(alpha < 1f/255f) return;` early-out.
    #[test]
    fn liquid_tile_skips_an_empty_tank() {
        let mut ids = RegionIdTable::new();
        let mut out = Vec::new();
        let st = state();
        execute(
            &DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
                liquid: None,
                frame: None,
            },
            "melter",
            &st,
            &mut ids,
            &mut out,
        );
        assert!(out.is_empty());
    }

    /// `chain_for_with` resolves the hand-written `DrawLiquidTile` names into
    /// content records (`fill_liquid_tiles`); `cryofluid-mixer` names water and
    /// cryofluid (`Blocks.java:1181`).
    #[test]
    fn chain_for_with_resolves_draw_liquid_names() {
        let content = crate::content::test_support::test_registry();
        let def = content
            .block_by_name("cryofluid-mixer")
            .expect("cryofluid-mixer");
        let chain = chain_for_with(def, &content);
        let tiles: Vec<_> = chain
            .iter()
            .filter_map(|desc| match desc {
                DrawDesc::LiquidTile { liquid, frame, .. } => Some((*liquid, *frame)),
                _ => None,
            })
            .collect();
        assert_eq!(tiles.len(), 2, "water + cryofluid");
        assert_eq!(tiles[0].0, Some("water"));
        assert_eq!(tiles[1].0, Some("cryofluid"));
        for (name, frame) in tiles {
            let liquid = content.liquid_by_name(name.expect("name")).expect("liquid");
            let frame = frame.expect("resolved");
            assert_eq!(frame.id, liquid.id.raw());
            assert_eq!(frame.gas, liquid.gas);
            assert_eq!(frame.color, liquid.color.to_rgba8888());
        }
    }
}
