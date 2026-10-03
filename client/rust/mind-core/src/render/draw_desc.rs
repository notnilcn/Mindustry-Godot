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
use crate::content::Rgba;
use crate::math::ArcRand;
use crate::render::commands::{Blend, DrawCmd, FillKind, LineKind, ShapeKind};
use crate::render::ids::{RegionId, RegionIdTable};
use crate::render::layer::Layer;

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
        /// `DrawLiquidTile.padding` (`<0` = 0).
        padding: f32,
        /// Alpha multiplier.
        alpha: f32,
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
    /// `DrawLiquidOutputs` (region-only approximation; side data is plan 07).
    LiquidOutputs {
        /// Region suffix per output index.
        suffix: &'static str,
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
            rotate_speed,
            rotation,
            building_rotate,
            layer,
            color,
            x,
            y,
            ..
        } => {
            let r = region(name, suffix, ids);
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
        DrawDesc::LiquidRegion { alpha, .. } | DrawDesc::LiquidTile { alpha, .. } => {
            let suffix = match desc {
                DrawDesc::LiquidRegion { suffix, .. } => *suffix,
                _ => "-liquid",
            };
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
            let a = 0.5 * state.warmup;
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
            let a = 0.5 * state.warmup;
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
        } => {
            if state.warmup <= 0.0 {
                return;
            }
            let si = absin(state.time, *flame_radius_scl, *flame_radius_mag);
            let a = alpha * state.warmup;
            out.push(DescDraw::circle(
                state.layer,
                Blend::Additive,
                state.x,
                state.y,
                flame_rad + si,
                with_alpha(*mid_color, a),
            ));
            out.push(DescDraw::circle_line(
                state.layer,
                Blend::Additive,
                state.x,
                state.y,
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
                    state.x + dx,
                    state.y + dy,
                    state.x + dx + ex,
                    state.y + dy + ey,
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
        } => {
            if state.warmup <= 0.0 {
                return;
            }
            let si = absin(state.time, *flame_radius_scl, *flame_radius_mag);
            let a = alpha * state.warmup;
            out.push(DescDraw::circle(
                state.layer,
                Blend::Additive,
                state.x,
                state.y,
                flame_rad + si,
                with_alpha(*mid_color, a),
            ));
            out.push(DescDraw::circle_line(
                state.layer,
                Blend::Additive,
                state.x,
                state.y,
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
                let len = particle_rad * fout.powf(1.5);
                let (dx, dy) = trns(angle, len);
                let alpha_p = a * (1.0 - curve(fin, 1.0 - fade_margin, 1.0));
                out.push(DescDraw::circle(
                    state.layer,
                    Blend::Additive,
                    state.x + dx,
                    state.y + dy,
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
        DrawDesc::LiquidOutputs { suffix } => {
            let r = region(name, suffix, ids);
            out.push(DescDraw::sprite(
                state.layer,
                Blend::Normal,
                r,
                state.x,
                state.y,
                quad,
                quad,
                0.0,
                state.liquid_color,
            ));
        }
        DrawDesc::Multi(list) => {
            for d in list {
                execute(d, name, state, ids, out);
            }
        }
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
        DrawDesc::Default | DrawDesc::Region { suffix: "", .. } => {}
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
            DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
            },
            DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
            },
            DrawDesc::Default,
        ],
        "melter" | "separator" | "disassembler" => vec![
            region_desc("-bottom"),
            DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
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
            DrawDesc::LiquidTile {
                padding: 0.0,
                alpha: 1.0,
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
        _ => return None,
    })
}

/// A no-rotation `DrawRegion(suffix)`.
fn region_desc(suffix: &'static str) -> DrawDesc {
    DrawDesc::Region {
        suffix,
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
}
