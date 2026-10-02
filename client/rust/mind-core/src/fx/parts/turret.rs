// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DrawTurret` geometry (plan 17 M5).
//!
//! Pure port of `world/draw/DrawTurret.java` draw order; state comes from plan
//! 10's turret draw state. `mind-gdext::fx::draw_turret` executes this against
//! live buildings.

use crate::content::Rgba;
use crate::content::registries::units::parts::DrawPartSpec;
use crate::render::draw::{Blending, PrimKind};
use crate::render::layer::Layer;

use super::draw::PartEmit;
use super::params::PartParams;

/// `Pal.shadow` (used by `Drawf.shadow`).
const SHADOW: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.22);

/// A live turret's draw inputs (mirrors the `DrawTurret.draw` reads).
#[derive(Clone, Debug)]
pub struct TurretDraw<'a> {
    /// Block name (region prefix).
    pub name: &'a str,
    /// Building center.
    pub x: f32,
    /// Building center.
    pub y: f32,
    /// Turret rotation in degrees (`tb.drawrot()`).
    pub drawrot: f32,
    /// Recoil offset (`tb.recoilOffset`).
    pub recoil_offset: (f32, f32),
    /// Turret elevation (`turret.elevation`).
    pub elevation: f32,
    /// Heat `0..1`.
    pub heat: f32,
    /// Warmup `0..1`.
    pub warmup: f32,
    /// Current recoil (`tb.curRecoil`).
    pub cur_recoil: f32,
    /// Per-barrel recoil counters.
    pub cur_recoils: &'a [f32],
    /// Charge `0..1`.
    pub charge: f32,
    /// Fire progress `0..1` (`tb.progress()`).
    pub progress: f32,
    /// Liquid fill fraction `0..1`.
    pub liquid_fraction: f32,
    /// Liquid tint.
    pub liquid_color: Rgba,
    /// Heat-region tint (`block.heatColor`).
    pub heat_color: Rgba,
    /// Base-part list.
    pub parts: &'a [DrawPartSpec],
    /// Selected ammo part list (`ammoParts` for the current ammo).
    pub ammo_parts: &'a [DrawPartSpec],
    /// `DrawTurret.turretLayer`.
    pub turret_layer: f32,
    /// `DrawTurret.shadowLayer`.
    pub shadow_layer: f32,
    /// `DrawTurret.heatLayer`.
    pub heat_layer: f32,
}

impl Default for TurretDraw<'static> {
    fn default() -> Self {
        Self {
            name: "",
            x: 0.0,
            y: 0.0,
            drawrot: 0.0,
            recoil_offset: (0.0, 0.0),
            elevation: 0.0,
            heat: 0.0,
            warmup: 0.0,
            cur_recoil: 0.0,
            cur_recoils: &[],
            charge: 0.0,
            progress: 0.0,
            liquid_fraction: 0.0,
            liquid_color: Rgba::WHITE,
            heat_color: Rgba::WHITE,
            parts: &[],
            ammo_parts: &[],
            turret_layer: Layer::Turret.z(),
            shadow_layer: Layer::Turret.z() - 0.5,
            heat_layer: Layer::TurretHeat.z(),
        }
    }
}

/// Ports `DrawTurret.draw` (`DrawTurret.java:73-120`).
pub fn draw_turret(emit: &mut PartEmit, t: &TurretDraw) {
    let (rx, ry) = (t.x + t.recoil_offset.0, t.y + t.recoil_offset.1);

    // base (drawn at the incoming z)
    emit.region(&format!("{}-base", t.name), t.x, t.y, 0.0);

    // shadow
    let z = emit.z;
    emit.z = t.shadow_layer;
    {
        let saved = emit.color;
        emit.color = SHADOW;
        emit.region(
            &format!("{}-preview", t.name),
            t.x + t.recoil_offset.0 - t.elevation,
            t.y + t.recoil_offset.1 - t.elevation,
            t.drawrot,
        );
        emit.color = saved;
    }
    emit.z = t.turret_layer;

    // turret / liquid / top
    emit.region(t.name, rx, ry, t.drawrot);
    if t.liquid_fraction >= 1.0 / 255.0 {
        let saved = emit.color;
        emit.color = t
            .liquid_color
            .with_alpha(t.liquid_fraction * t.liquid_color.a);
        emit.region(&format!("{}-liquid", t.name), rx, ry, t.drawrot);
        emit.color = saved;
    }
    emit.region(&format!("{}-top", t.name), rx, ry, t.drawrot);

    // heat
    if t.heat > 0.00001 {
        let saved_color = emit.color;
        let saved_blend = emit.blend;
        emit.color = t.heat_color.with_alpha(t.heat);
        emit.blend = Blending::Additive;
        emit.push_at(
            t.heat_layer,
            region_kind(emit, &format!("{}-heat", t.name), rx, ry, t.drawrot),
            Blending::Additive,
        );
        emit.color = saved_color;
        emit.blend = saved_blend;
    }

    // parts
    let has_parts = !t.parts.is_empty() || !t.ammo_parts.is_empty();
    if has_parts {
        if emit.lookup(&format!("{}-outline", t.name)).found {
            emit.push_at(
                t.turret_layer - 0.01,
                region_kind(emit, &format!("{}-outline", t.name), rx, ry, t.drawrot),
                Blending::Normal,
            );
        }
        let params = part_params(t, rx, ry);
        for part in t.parts {
            let mut p = params;
            p.recoil =
                if part.recoil_index >= 0 && (part.recoil_index as usize) < t.cur_recoils.len() {
                    t.cur_recoils[part.recoil_index as usize]
                } else {
                    t.cur_recoil
                };
            emit.draw_part(part, &p, t.name);
        }
        if !t.ammo_parts.is_empty() {
            for part in t.ammo_parts {
                let mut p = params;
                p.recoil = if part.recoil_index >= 0
                    && (part.recoil_index as usize) < t.cur_recoils.len()
                {
                    t.cur_recoils[part.recoil_index as usize]
                } else {
                    t.cur_recoil
                };
                emit.draw_part(part, &p, t.name);
            }
        }
    }

    emit.z = z;
}

/// Ports `DrawTurret.drawPlan` (`DrawTurret.java:64-71`).
pub fn draw_turret_plan(emit: &mut PartEmit, t: &TurretDraw, plan_rotation: i32, rotate: bool) {
    emit.region(&format!("{}-base", t.name), t.x, t.y, 0.0);
    let rot = if rotate {
        plan_rotation as f32 * 90.0 - 90.0
    } else {
        0.0
    };
    emit.region(&format!("{}-preview", t.name), t.x, t.y, rot);
    emit.region(&format!("{}-top", t.name), t.x, t.y, rot);
}

fn part_params(t: &TurretDraw, rx: f32, ry: f32) -> PartParams {
    let mut params = PartParams::default();
    params.set(
        t.warmup,
        1.0 - t.progress,
        1.0 - t.progress,
        t.heat,
        t.cur_recoil,
        t.charge,
        rx,
        ry,
        t.drawrot,
    );
    params
}

fn region_kind(emit: &PartEmit, name: &str, x: f32, y: f32, rot: f32) -> PrimKind {
    let info = emit.lookup(name);
    let _ = rot;
    PrimKind::Region {
        region: info.key,
        x,
        y,
        w: info.w,
        h: info.h,
        rotation_deg: rot,
        origin: (0.5, 0.5),
        color: emit.color,
        mix: None,
        wrap: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::parts::draw::{AllRegions, PartEmit};
    use crate::render::draw::DrawProgram;

    fn names(program: &DrawProgram) -> Vec<(&str, f32)> {
        program
            .prims
            .iter()
            .filter_map(|p| match &p.kind {
                PrimKind::Region { region, .. } => Some((region.0, p.z)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn turret_draw_order() {
        let lookup = AllRegions::new(32.0);
        let mut program = DrawProgram::new();
        {
            let mut emit = PartEmit::new(&mut program, &lookup, 0.0);
            emit.z = Layer::Block.z();
            let t = TurretDraw {
                name: "duo",
                x: 10.0,
                y: 20.0,
                drawrot: 45.0,
                heat: 0.5,
                liquid_fraction: 1.0,
                liquid_color: Rgba::WHITE,
                ..Default::default()
            };
            draw_turret(&mut emit, &t);
        }
        let ns = names(&program);
        let expected = vec![
            ("duo-base", Layer::Block.z()),
            ("duo-preview", 49.5),
            ("duo", 50.0),
            ("duo-liquid", 50.0),
            ("duo-top", 50.0),
            ("duo-heat", 50.1),
        ];
        assert_eq!(ns, expected);
        assert_eq!(program.prims.last().unwrap().blend, Blending::Additive);
    }

    #[test]
    fn ammo_parts_switch_adds_prims() {
        let mut ammo = DrawPartSpec::shape();
        ammo.under = false;
        ammo.color = Some(Rgba::WHITE);
        let lookup = AllRegions::new(32.0);

        let mut program = DrawProgram::new();
        {
            let mut emit = PartEmit::new(&mut program, &lookup, 0.0);
            let t = TurretDraw {
                name: "duo",
                ammo_parts: std::slice::from_ref(&ammo),
                ..Default::default()
            };
            draw_turret(&mut emit, &t);
        }
        assert!(
            program
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Poly { .. }))
        );

        let empty: [DrawPartSpec; 0] = [];
        let mut program2 = DrawProgram::new();
        {
            let mut emit = PartEmit::new(&mut program2, &lookup, 0.0);
            let t = TurretDraw {
                name: "duo",
                ammo_parts: &empty,
                ..Default::default()
            };
            draw_turret(&mut emit, &t);
        }
        assert!(
            !program2
                .prims
                .iter()
                .any(|p| matches!(p.kind, PrimKind::Poly { .. }))
        );
    }

    #[test]
    fn plan_draw_uses_base_preview_top() {
        let lookup = AllRegions::new(32.0);
        let mut program = DrawProgram::new();
        {
            let mut emit = PartEmit::new(&mut program, &lookup, 0.0);
            let t = TurretDraw {
                name: "duo",
                ..Default::default()
            };
            draw_turret_plan(&mut emit, &t, 1, true);
        }
        assert_eq!(program.len(), 3);
    }
}
