// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawTurret` execution against live turret draw state (plan 17 M5).
//!
//! Pure adapter: `mind-core::fx::parts::turret` owns the draw order; this module
//! converts a plan-10 [`TurretDrawState`] plus the block's resident part data
//! into a [`TurretDraw`] and appends the compiled prims to the frame program.
//!
//! The in-engine caller (`MindFx`) has no direct sim handle (effects are a
//! one-way view), so the state is pushed as an owned snapshot via the
//! `spawn_turret_draw` probe; MCP drives it and the live plan-10 pass can feed
//! the same struct when the plan-16 executor lands.

use mind_core::combat::view::TurretDrawState;
use mind_core::content::Rgba;
use mind_core::content::registries::units::parts::DrawPartSpec;
use mind_core::fx::parts::draw::{PartEmit, RegionLookup};
use mind_core::fx::parts::turret::{TurretDraw, draw_turret};
use mind_core::render::draw::DrawProgram;
use mind_core::render::layer::Layer;

/// Owned turret draw snapshot (the `&str`/`&[DrawPartSpec]` fields of
/// [`TurretDraw`] made `'static`).
#[derive(Clone, Debug)]
pub struct TurretDrawInput {
    /// Probe handle (stable while alive).
    pub handle: i64,
    /// Block name / region prefix.
    pub name: String,
    /// Building center x.
    pub x: f32,
    /// Building center y.
    pub y: f32,
    /// Turret rotation in degrees (`tb.drawrot()`).
    pub drawrot: f32,
    /// Recoil offset.
    pub recoil_offset: (f32, f32),
    /// Turret elevation.
    pub elevation: f32,
    /// Heat `0..1`.
    pub heat: f32,
    /// Warmup `0..1`.
    pub warmup: f32,
    /// Current recoil.
    pub cur_recoil: f32,
    /// Per-barrel recoil counters.
    pub cur_recoils: Vec<f32>,
    /// Charge `0..1`.
    pub charge: f32,
    /// Fire progress `0..1`.
    pub progress: f32,
    /// Liquid fill fraction `0..1`.
    pub liquid_fraction: f32,
    /// Liquid tint.
    pub liquid_color: Rgba,
    /// Heat-region tint (`block.heatColor`).
    pub heat_color: Rgba,
    /// Base part list.
    pub parts: Vec<DrawPartSpec>,
    /// Ammo part list for the selected ammo.
    pub ammo_parts: Vec<DrawPartSpec>,
    /// `turretLayer`.
    pub turret_layer: f32,
    /// `shadowLayer`.
    pub shadow_layer: f32,
    /// `heatLayer`.
    pub heat_layer: f32,
}

impl TurretDrawInput {
    /// Builds a snapshot from the plan-10 draw state and block metadata.
    #[allow(clippy::too_many_arguments)]
    pub fn from_state(
        handle: i64,
        name: impl Into<String>,
        x: f32,
        y: f32,
        state: TurretDrawState,
        progress: f32,
        liquid_fraction: f32,
        liquid_color: Rgba,
        heat_color: Rgba,
    ) -> Self {
        Self {
            handle,
            name: name.into(),
            x,
            y,
            drawrot: state.rotation,
            recoil_offset: (0.0, 0.0),
            elevation: 0.0,
            heat: state.heat,
            warmup: state.warmup,
            cur_recoil: state.recoil,
            cur_recoils: Vec::new(),
            charge: state.charge,
            progress,
            liquid_fraction,
            liquid_color,
            heat_color,
            parts: Vec::new(),
            ammo_parts: Vec::new(),
            turret_layer: Layer::Turret.z(),
            shadow_layer: Layer::Turret.z() - 0.5,
            heat_layer: Layer::TurretHeat.z(),
        }
    }

    /// Materializes the borrowed [`TurretDraw`] view.
    pub fn as_draw(&self) -> TurretDraw<'_> {
        TurretDraw {
            name: &self.name,
            x: self.x,
            y: self.y,
            drawrot: self.drawrot,
            recoil_offset: self.recoil_offset,
            elevation: self.elevation,
            heat: self.heat,
            warmup: self.warmup,
            cur_recoil: self.cur_recoil,
            cur_recoils: &self.cur_recoils,
            charge: self.charge,
            progress: self.progress,
            liquid_fraction: self.liquid_fraction,
            liquid_color: self.liquid_color,
            heat_color: self.heat_color,
            parts: &self.parts,
            ammo_parts: &self.ammo_parts,
            turret_layer: self.turret_layer,
            shadow_layer: self.shadow_layer,
            heat_layer: self.heat_layer,
        }
    }

    /// The part params `DrawTurret.draw` would evaluate for the current state.
    pub fn part_params(&self) -> mind_core::fx::parts::PartParams {
        let mut params = mind_core::fx::parts::PartParams::default();
        params.set(
            self.warmup,
            1.0 - self.progress,
            1.0 - self.progress,
            self.heat,
            self.cur_recoil,
            self.charge,
            self.x + self.recoil_offset.0,
            self.y + self.recoil_offset.1,
            self.drawrot,
        );
        params
    }
}

/// Appends a turret's prims to `program` at the block layer.
pub fn build_turret(
    program: &mut DrawProgram,
    lookup: &dyn RegionLookup,
    tick: f32,
    z: f32,
    input: &TurretDrawInput,
) {
    let mut emit = PartEmit::new(program, lookup, tick);
    emit.set_z(z);
    draw_turret(&mut emit, &input.as_draw());
}
