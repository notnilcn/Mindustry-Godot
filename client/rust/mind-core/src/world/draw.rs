// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawBlock` framework (Godot-free draw descriptors).
//!
//! Ported from `core/src/mindustry/world/draw/*.java`. Plan 07 emits an
//! allocation-free [`DrawCommands`] buffer per frame; plan 16 executes it through
//! the Godot block renderer (plan 07 §2.4.10/§3.11). Turret/laser drawers are
//! plan 10's.

use smallvec::SmallVec;

use crate::content::{BlockId, Rgba};

/// Resolved region name (sprite slot).
pub type RegionName = String;

/// One emitted sprite command.
#[derive(Debug, Clone, PartialEq)]
pub struct Sprite {
    /// Region name.
    pub region: RegionName,
    /// World x in pixels.
    pub x: f32,
    /// World y in pixels.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Tint.
    pub color: Rgba,
    /// Blend mode index.
    pub blend: u8,
    /// Z offset.
    pub z: f32,
}

/// A shape primitive (`Lines`, `Fill`, `Poly`).
#[derive(Debug, Clone, PartialEq)]
pub enum ShapeCommand {
    /// Filled rectangle.
    Fill {
        /// x.
        x: f32,
        /// y.
        y: f32,
        /// width.
        w: f32,
        /// height.
        h: f32,
        /// Color.
        color: Rgba,
    },
    /// A polyline.
    Lines {
        /// Point count.
        count: usize,
        /// Color.
        color: Rgba,
    },
}

/// One combined draw command.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    /// Sprite.
    Sprite(Sprite),
    /// Shape.
    Shape(ShapeCommand),
    /// Set z layer.
    SetZ(f32),
    /// Set blend mode.
    SetBlend(u8),
}

/// Allocation-free per-frame command buffer (reused, never feeds the sim).
#[derive(Debug, Default)]
pub struct DrawCommands {
    /// Command list.
    pub commands: Vec<DrawCommand>,
    /// Current z.
    pub z: f32,
    /// Current blend.
    pub blend: u8,
}

impl DrawCommands {
    /// Clears for the next frame.
    pub fn clear(&mut self) {
        self.commands.clear();
        self.z = 0.0;
        self.blend = 0;
    }

    /// Pushes a sprite at the current z/blend.
    pub fn sprite(&mut self, region: RegionName, x: f32, y: f32, rotation: f32, color: Rgba) {
        self.commands.push(DrawCommand::Sprite(Sprite {
            region,
            x,
            y,
            rotation,
            color,
            blend: self.blend,
            z: self.z,
        }));
    }

    /// Pushes a fill.
    pub fn fill(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba) {
        self.commands
            .push(DrawCommand::Shape(ShapeCommand::Fill { x, y, w, h, color }));
    }

    /// Sets the z layer.
    pub fn set_z(&mut self, z: f32) {
        self.z = z;
        self.commands.push(DrawCommand::SetZ(z));
    }
}

/// Context passed to [`DrawBlock::load`] (plan 03 region resolver).
#[derive(Debug, Default)]
pub struct BlockLoadCtx {
    /// Regions resolved for this block, in load order.
    pub regions: Vec<RegionName>,
}

/// Minimal build draw data (`BuildDrawData`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildDrawData {
    /// Center x in pixels.
    pub x: f32,
    /// Center y in pixels.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Block size in tiles.
    pub size: f32,
    /// Health fraction.
    pub health_fraction: f32,
    /// Current power status.
    pub power_status: f32,
}

/// Minimal plan draw data (`BuildPlanDrawData`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildPlanDrawData {
    /// Center x in pixels.
    pub x: f32,
    /// Center y in pixels.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Block size in tiles.
    pub size: f32,
}

/// The draw descriptor trait (`DrawBlock`).
pub trait DrawBlock: Send + Sync {
    /// Resolves regions through plan 03.
    fn load(&mut self, block: BlockId, load: &mut BlockLoadCtx) {
        let _ = (block, load);
    }

    /// Emits the main draw.
    fn draw(&self, out: &mut DrawCommands, data: &BuildDrawData);

    /// Emits the light overlay.
    fn draw_light(&self, out: &mut DrawCommands, data: &BuildDrawData) {
        let _ = (out, data);
    }

    /// Emits the ghost/plan draw.
    fn draw_plan(&self, out: &mut DrawCommands, data: &BuildPlanDrawData) {
        let _ = (out, data);
    }

    /// Icon region descriptors.
    fn icons(&self) -> SmallVec<[RegionName; 4]> {
        SmallVec::new()
    }

    /// Outline region descriptors.
    fn regions_to_outline(&self) -> SmallVec<[RegionName; 2]> {
        SmallVec::new()
    }
}

/// `DrawDefault`: draws the block's base region at the center.
#[derive(Debug, Clone, Default)]
pub struct DrawDefault;

impl DrawBlock for DrawDefault {
    fn draw(&self, out: &mut DrawCommands, data: &BuildDrawData) {
        out.sprite(String::new(), data.x, data.y, data.rotation, Rgba::WHITE);
    }
}

/// Data description of a draw tree (plan 07 §3.11 `DrawSpec`).
#[derive(Debug, Clone, PartialEq, Default)]
pub enum DrawSpec {
    /// `DrawDefault`.
    #[default]
    Default,
    /// `DrawMulti`. (Recursive payload; a `Vec` keeps the type sized — the
    /// plan's `SmallVec<[DrawSpec; 4]>` cannot be recursive.)
    Multi(Vec<DrawSpec>),
    /// `DrawRegion`.
    Region {
        /// Region suffix (`@`/`-suffix`).
        suffix: String,
        /// Spin speed.
        rotate_speed: f32,
        /// Whether it spins.
        spin: bool,
        /// Tint.
        color: Rgba,
        /// Draw layer.
        layer: f32,
    },
    /// `DrawFlame`.
    Flame {
        /// Flame color.
        color: Rgba,
        /// Flame radius.
        radius: f32,
    },
    /// `DrawLiquidRegion`.
    LiquidRegion {
        /// Liquid capacity scale.
        alpha: f32,
    },
    /// `DrawHeatInput`/`DrawHeatOutput`.
    HeatInput,
    /// `DrawHeatOutput`.
    HeatOutput,
    /// `DrawPistons`.
    Pistons,
    /// `DrawFade`.
    Fade,
    /// `DrawWeave`.
    Weave,
}

impl DrawSpec {
    /// Creates a multi-draw spec.
    pub fn multi(specs: impl IntoIterator<Item = DrawSpec>) -> Self {
        DrawSpec::Multi(specs.into_iter().collect())
    }
}

/// A [`DrawBlock`] backed by a [`DrawSpec`] tree.
#[derive(Debug, Clone, Default)]
pub struct SpecDraw {
    /// The tree.
    pub spec: DrawSpec,
}

impl DrawBlock for SpecDraw {
    fn draw(&self, out: &mut DrawCommands, data: &BuildDrawData) {
        draw_spec(&self.spec, out, data);
    }
}

fn draw_spec(spec: &DrawSpec, out: &mut DrawCommands, data: &BuildDrawData) {
    match spec {
        DrawSpec::Default => out.sprite(String::new(), data.x, data.y, data.rotation, Rgba::WHITE),
        DrawSpec::Multi(specs) => {
            for child in specs {
                draw_spec(child, out, data);
            }
        }
        DrawSpec::Region { suffix, color, .. } => {
            out.sprite(suffix.clone(), data.x, data.y, data.rotation, *color);
        }
        DrawSpec::Flame { color, .. } => {
            out.sprite(String::from("-flame"), data.x, data.y, 0.0, *color);
        }
        DrawSpec::LiquidRegion { .. } => {
            out.sprite(String::from("-liquid"), data.x, data.y, 0.0, Rgba::WHITE);
        }
        DrawSpec::HeatInput
        | DrawSpec::HeatOutput
        | DrawSpec::Pistons
        | DrawSpec::Fade
        | DrawSpec::Weave => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_multi_emits_child_commands_in_order() {
        let spec = DrawSpec::multi([
            DrawSpec::Region {
                suffix: String::from("-a"),
                rotate_speed: 0.0,
                spin: false,
                color: Rgba::WHITE,
                layer: 0.0,
            },
            DrawSpec::Region {
                suffix: String::from("-b"),
                rotate_speed: 0.0,
                spin: false,
                color: Rgba::WHITE,
                layer: 1.0,
            },
        ]);
        let drawer = SpecDraw { spec };
        let mut out = DrawCommands::default();
        drawer.draw(
            &mut out,
            &BuildDrawData {
                x: 1.0,
                y: 2.0,
                rotation: 0.0,
                size: 1.0,
                health_fraction: 1.0,
                power_status: 0.0,
            },
        );
        assert_eq!(out.commands.len(), 2);
        match (&out.commands[0], &out.commands[1]) {
            (DrawCommand::Sprite(a), DrawCommand::Sprite(b)) => {
                assert_eq!(a.region, "-a");
                assert_eq!(b.region, "-b");
            }
            _ => panic!("expected sprites"),
        }
    }
}
