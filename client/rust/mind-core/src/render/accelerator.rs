// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Accelerator.AcceleratorBuild` draw math (world/blocks/campaign/
//! `Accelerator.java:140-236,413-560`).
//!
//! Godot-free inputs for `mind-gdext::render::accelerator`: the per-building
//! state snapshot ([`accelerator_rows`]), the resting arrow ring, the heat
//! square/arrow burst, the launch-phase mapping shared by `drawLaunch` /
//! `drawLaunchGlobalZ`, and the charge-ring geometry. The blockbuilder scan
//! shader and the `drawLanding` thruster pass are host-side (see the render
//! module); everything here is pure and unit-tested.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::{Building, CampaignState};
use crate::render::drawf;
use crate::world::block::BlockTable;
use crate::world::block_kind_data::BlockKindData;

/// `Accelerator.chargeRings` (`Accelerator.java:59`).
pub const CHARGE_RINGS: usize = 4;
/// `Accelerator.ringRadBase` (`Accelerator.java:60`).
pub const RING_RAD_BASE: f32 = 60.0;
/// `Accelerator.ringRadSpacing` (`Accelerator.java:60`).
pub const RING_RAD_SPACING: f32 = 25.0;
/// `Accelerator.ringRadPow` (`Accelerator.java:60`).
pub const RING_RAD_POW: f32 = 1.6;
/// `Accelerator.ringStroke` (`Accelerator.java:61`).
pub const RING_STROKE: f32 = 3.0;
/// `Accelerator.ringSpeedup` (`Accelerator.java:61`).
pub const RING_SPEEDUP: f32 = 1.4;
/// `Accelerator.chargeRingMerge` (`Accelerator.java:61`).
pub const RING_MERGE: f32 = 2.0;
/// `Accelerator.ringHandleTilt` (`Accelerator.java:62`).
pub const RING_HANDLE_TILT: f32 = 0.8;
/// `Accelerator.ringHandleLen` (`Accelerator.java:62`).
pub const RING_HANDLE_LEN: f32 = 30.0;

/// `Accelerator.launchLightning` (`Accelerator.java:69`).
pub const LAUNCH_LIGHTNING: usize = 20;
/// `Accelerator.lightningOffset` (`Accelerator.java:72`).
pub const LIGHTNING_OFFSET: f32 = 24.0;
/// `Accelerator.lightningLengthMin` / `Max` (`Accelerator.java:73`).
pub const LIGHTNING_LENGTH_MIN: i32 = 5;
/// `Accelerator.lightningLengthMax` (`Accelerator.java:73`).
pub const LIGHTNING_LENGTH_MAX: i32 = 25;

/// One placed accelerator's draw snapshot (`AcceleratorBuild.draw` inputs).
#[derive(Clone, Debug, PartialEq)]
pub struct AcceleratorDraw {
    /// Entity index (stable order/tie-break, like the pad rows).
    pub id: u32,
    /// World centre x (`Building.x`).
    pub x: f32,
    /// World centre y (`Building.y`).
    pub y: f32,
    /// Block footprint size (`Block.size`).
    pub size: i32,
    /// Team index (`Building.team`).
    pub team: u8,
    /// `AcceleratorBuild.progress` (launch-core construction, `0..1`).
    pub progress: f32,
    /// `AcceleratorBuild.heat`.
    pub heat: f32,
    /// `AcceleratorBuild.statusLerp`.
    pub status_lerp: f32,
    /// `AcceleratorBuild.time`.
    pub time: f32,
    /// `AcceleratorBuild.launchHeat`.
    pub launch_heat: f32,
    /// `AcceleratorBuild.launching`.
    pub launching: bool,
    /// `AcceleratorBuild.efficiency` (`Building.efficiency`).
    pub efficiency: f32,
    /// `Accelerator.launchDuration` (:51).
    pub launch_duration: f32,
    /// `Accelerator.chargeDuration` (:52).
    pub charge_duration: f32,
    /// `Accelerator.buildDuration` (:53).
    pub build_duration: f32,
}

impl AcceleratorDraw {
    /// `AcceleratorBuild.isCoreBuilt()` (`Accelerator.java:143`):
    /// `progress >= 1`.
    pub fn core_built(&self) -> bool {
        self.progress >= 1.0
    }

    /// `launchDuration()` (`Accelerator.java:322-324`): the animator runs
    /// `launchDuration + chargeDuration` ticks.
    pub fn launch_total(&self) -> f32 {
        self.launch_duration + self.charge_duration
    }
}

/// Collects the visible accelerator draw rows. Rust-only seam for
/// `MindSimHost::accelerator_draws` (mirrors `launch_pad_rows`).
pub fn accelerator_rows(world: &mut World) -> Vec<AcceleratorDraw> {
    let mut out = Vec::new();
    let mut query = world.query::<(Entity, &Building, &CampaignState)>();
    for (entity, building, state) in query.iter(world) {
        let Some(table) = world.get_resource::<BlockTable>() else {
            return out;
        };
        let Some(inst) = table.instance(building.block) else {
            continue;
        };
        let BlockKindData::Accelerator(def) = &inst.kind_data else {
            continue;
        };
        let team = world
            .get::<crate::entities::comp::TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);
        // `Building.x = tile.drawx() + Block.offset` (Blocks.rs anchor rule:
        // odd sizes sit on the centre tile, even sizes shift by `offset`).
        let size = inst.def.size.max(1);
        let tile_size = crate::config::TILESIZE as f32;
        let x =
            (f32::from(building.tile.x()) + ((size - 1) / 2) as f32) * tile_size + inst.def.offset;
        let y =
            (f32::from(building.tile.y()) + ((size - 1) / 2) as f32) * tile_size + inst.def.offset;
        out.push(AcceleratorDraw {
            id: entity.index().index(),
            x,
            y,
            size,
            team,
            progress: state.progress,
            heat: state.heat,
            status_lerp: state.status_lerp,
            time: state.time,
            launch_heat: state.launch_heat,
            launching: state.launching,
            efficiency: building.efficiency,
            launch_duration: def.launch_duration,
            charge_duration: def.charge_duration,
            build_duration: def.build_duration,
        });
    }
    out.sort_by_key(|row| row.id);
    out
}

/// One resting arrow of `AcceleratorBuild.draw`'s `for(l < 4)` ring
/// (`Accelerator.java:146-156`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RestArrow {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Sprite rotation (`rot + 180`).
    pub rotation: f32,
    /// `color(Tmp.c1.set(darkMetal).lerp(team.color, statusLerp), darkMetal,
    /// absin(Time.time + l*50, 10, 1))`.
    pub color: [f32; 4],
}

/// `AcceleratorBuild.draw`'s resting arrow ring (`Accelerator.java:146-156`):
/// four radii × four diagonals, tinted by the power status and a per-ring
/// `absin` pulse.
pub fn rest_arrows(row: &AcceleratorDraw) -> Vec<RestArrow> {
    let team = rgba_u32(team_color(row.team));
    let mut out = Vec::with_capacity(16);
    for l in 0..4 {
        let length = 7.0 + l as f32 * 5.0;
        let base = drawf::lerp(drawf::pal::DARK_METAL, team, row.status_lerp);
        let pulse = drawf::absin(row.time + l as f32 * 50.0, 10.0, 1.0);
        let color = drawf::lerp(base, drawf::pal::DARK_METAL, pulse);
        for i in 0..4 {
            let rotation = i as f32 * 90.0 + 45.0;
            let (dx, dy) = drawf::trns(rotation, length);
            out.push(RestArrow {
                x: row.x + dx,
                y: row.y + dy,
                rotation: rotation + 180.0,
                color,
            });
        }
    }
    out
}

/// One outlined square of the heat burst (`Lines.square`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatSquare {
    /// Four closed-ring points (world).
    pub points: [(f32, f32); 4],
    /// `Lines.stroke` width.
    pub stroke: f32,
}

/// One team-tinted heat arrow (`Accelerator.java:231-235`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeatArrow {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Sprite rotation (`rot + 180`).
    pub rotation: f32,
    /// `Draw.alpha(clamp(heat * 3))`.
    pub alpha: f32,
}

/// `AcceleratorBuild.draw`'s heat pass (`Accelerator.java:207-236`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeatGeometry {
    /// The two `Lines.square` rings (the first is a single 1.75-stroke ring,
    /// the other two share the 3-stroke colour).
    pub squares: Vec<HeatSquare>,
    /// The four rotating team arrows.
    pub arrows: Vec<HeatArrow>,
}

/// `AcceleratorBuild.draw` heat geometry. `None` when `heat < 0.0001`
/// (`Accelerator.java:207`: `if(heat < 0.0001f) return;`).
pub fn heat_geometry(row: &AcceleratorDraw) -> Option<HeatGeometry> {
    if row.heat < 0.0001 {
        return None;
    }
    let rad = row.size as f32 * 8.0 / 2.0 * 0.74 * lerp(1.0, 1.3, row.launch_heat);
    let squares = vec![
        HeatSquare {
            points: square_points(row.x, row.y, rad * 1.22, lerp(45.0, 0.0, row.launch_heat)),
            stroke: 1.75 * row.heat,
        },
        HeatSquare {
            points: square_points(
                row.x,
                row.y,
                rad * lerp(1.0, 1.3, row.launch_heat),
                45.0 + row.time / 2.0,
            ),
            stroke: 3.0 * row.heat,
        },
        HeatSquare {
            points: square_points(
                row.x,
                row.y,
                rad * lerp(1.0, 1.8, row.launch_heat),
                lerp(45.0, 0.0, row.launch_heat) - row.time / 2.0,
            ),
            stroke: 3.0 * row.heat,
        },
    ];
    let mut arrows = Vec::with_capacity(4);
    for i in 0..4 {
        let rotation = i as f32 * 90.0 + 45.0 + (-row.time / 3.0) % 360.0;
        let length = 26.0 * row.heat * lerp(1.0, 1.5, row.launch_heat);
        let (dx, dy) = drawf::trns(rotation, length);
        arrows.push(HeatArrow {
            x: row.x + dx,
            y: row.y + dy,
            rotation: rotation + 180.0,
            alpha: (row.heat * 3.0).clamp(0.0, 1.0),
        });
    }
    Some(HeatGeometry { squares, arrows })
}

/// `Renderer.getLandTimeIn()` mapped onto the accelerator launch animator's
/// two phases (`drawLaunch`/`updateLaunch`, `Accelerator.java:413-436`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LaunchPhase {
    /// `fin` of the launch half (`0..1` after the charge).
    pub fin: f32,
    /// `1 - fin`.
    pub fout: f32,
    /// `chargeFin` (`Accelerator.java:420`).
    pub charge_fin: f32,
    /// `1 - chargeFin`.
    pub charge_fout: f32,
    /// `rawTime = launchDuration() - renderer.getLandTime()` (elapsed ticks).
    pub raw_time: f32,
    /// `AcceleratorBuild.launchTime = launchDuration() - in`
    /// (`Accelerator.java:415-416`; equal to `raw_time`).
    pub launch_time: f32,
}

impl LaunchPhase {
    /// Maps `renderer.getLandTimeIn()` (`1` at the start, falling to `0`).
    pub fn from_land_time_in(fin_in: f32, launch_duration: f32, charge_duration: f32) -> Self {
        let total = (launch_duration + charge_duration).max(1e-6);
        let charge = (charge_duration / total).clamp(0.0, 1.0);
        let fin_in = fin_in.clamp(0.0, 1.0);
        let remaining = 1.0 - fin_in;
        let raw_time = remaining * total;
        // `fin = 1 - clamp((1 - rawFin) - chargeFraction) / (1 -
        // chargeFraction)` (`Accelerator.java:417-418`).
        let fin = 1.0 - (remaining - charge).clamp(0.0, 1.0) / (1.0 - charge).max(1e-6);
        let charge_fin = 1.0 - (remaining / charge.max(1e-6)).clamp(0.0, 1.0);
        Self {
            fin,
            fout: 1.0 - fin,
            charge_fin,
            charge_fout: 1.0 - charge_fin,
            raw_time,
            launch_time: raw_time,
        }
    }

    /// `Mathf.clamp(launchTime / chargeDuration)` used by the launching-core
    /// tint (`Accelerator.java:175-179`).
    pub fn launch_tint(&self, charge_duration: f32) -> f32 {
        (self.launch_time / charge_duration.max(1e-6)).clamp(0.0, 1.0)
    }
}

/// One charge ring of `drawLaunchGlobalZ` (`Accelerator.java:472-489`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChargeRing {
    /// `ringRadBase + ringRadSpacing * pow(i, ringRadPow)`.
    pub radius: f32,
    /// `cfin` (ring reveal `0..1`).
    pub fin: f32,
}

/// `AcceleratorBuild.drawLaunchGlobalZ` (`Accelerator.java:462-490`): the four
/// charge rings, hidden until the charge phase and faded out during launch.
pub fn charge_rings(phase: &LaunchPhase) -> Vec<ChargeRing> {
    let spacing = 1.0 / (CHARGE_RINGS as f32 + RING_MERGE);
    // `chargeFout = clamp(chargeFout - fout * 2)` (Accelerator.java:475).
    let charge_fout = (phase.charge_fout - phase.fout * 2.0).clamp(0.0, 1.0);
    let mut out = Vec::with_capacity(CHARGE_RINGS);
    for i in 0..CHARGE_RINGS {
        let cfin = ((charge_fout * RING_SPEEDUP - spacing * i as f32)
            / (spacing * (1.0 + RING_MERGE)))
            .clamp(0.0, 1.0);
        if cfin > 0.0 {
            out.push(ChargeRing {
                radius: RING_RAD_BASE + RING_RAD_SPACING * (i as f32).powf(RING_RAD_POW),
                fin: cfin,
            });
        }
    }
    out
}

/// `AcceleratorBuild.drawRing` geometry (`Accelerator.java:492-540`).
#[derive(Clone, Debug, PartialEq)]
pub struct RingGeometry {
    /// `rad = radius + 20 * pow4In(fout)`.
    pub radius: f32,
    /// `Lines.stroke(ringStroke * fin)`.
    pub stroke: f32,
    /// `Draw.color(Pal.command, ringColor, fin)` mix.
    pub color_mix: f32,
    /// The four handle polylines (three points each).
    pub handles: Vec<[(f32, f32); 3]>,
    /// The four selection triangles: `(x, y, rotation)`.
    pub arrows: Vec<(f32, f32, f32)>,
    /// `Draw.scl(fin)` for the selection triangles.
    pub arrow_scale: f32,
}

/// `drawRing` (`Accelerator.java:492-540`) for a ring centred at `(x, y)`.
pub fn ring_geometry(x: f32, y: f32, radius: f32, fin: f32) -> RingGeometry {
    let fin = fin.clamp(0.0, 1.0);
    let fout = 1.0 - fin;
    let rotate = drawf::pow4_in(fout) * 90.0;
    let rad = radius + 20.0 * drawf::pow4_in(fout);
    let mut handles = Vec::with_capacity(4);
    let mut arrows = Vec::with_capacity(4);
    for i in 0..4 {
        let angle = i as f32 * 90.0 + 45.0 + rotate;
        let (ax, ay) = drawf::trns(angle - RING_HANDLE_LEN, rad * RING_HANDLE_TILT);
        let (bx, by) = drawf::trns(angle, rad);
        let (cx, cy) = drawf::trns(angle + RING_HANDLE_LEN, rad * RING_HANDLE_TILT);
        handles.push([(x + ax, y + ay), (x + bx, y + by), (x + cx, y + cy)]);
        let (px, py) = drawf::trns(angle, rad);
        arrows.push((x + px, y + py, angle + 180.0 + 45.0));
    }
    RingGeometry {
        radius: rad,
        stroke: RING_STROKE * fin,
        color_mix: fin,
        handles,
        arrows,
        arrow_scale: fin,
    }
}

/// One `Lightning.create` bolt of `beginLaunch` (`Accelerator.java:364-367`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightningBolt {
    /// World x (`x + trnsx(a, lightningOffset)`).
    pub x: f32,
    /// World y (`y + trnsy(a, lightningOffset)`).
    pub y: f32,
    /// Bolt direction degrees (`a`).
    pub angle: f32,
    /// Bolt length (`Mathf.random(lightningLengthMin, lightningLengthMax)`).
    pub length: i32,
}

/// `beginLaunch`'s 20-bolt lightning burst (`Accelerator.java:364-367`).
/// Mirrors Java's per-bolt `Mathf.random(360f)` angle and integer length.
pub fn launch_lightning(x: f32, y: f32, rand: &mut crate::math::ArcRand) -> Vec<LightningBolt> {
    let mut out = Vec::with_capacity(LAUNCH_LIGHTNING);
    for _ in 0..LAUNCH_LIGHTNING {
        let angle = rand.next_float() * 360.0;
        let (dx, dy) = drawf::trns(angle, LIGHTNING_OFFSET);
        out.push(LightningBolt {
            x: x + dx,
            y: y + dy,
            angle,
            length: rand.random_range_int(LIGHTNING_LENGTH_MIN, LIGHTNING_LENGTH_MAX),
        });
    }
    out
}

/// `Mathf.lerp` (Arc).
fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

/// Closed 4-point ring for `Lines.square(x, y, radius, rotation)`:
/// `Lines.poly(x, y, 4, radius, rotation - 45)` (`Lines.java:483-486`).
fn square_points(x: f32, y: f32, radius: f32, rotation: f32) -> [(f32, f32); 4] {
    let ring = drawf::poly_ring(x, y, 4, radius, rotation - 45.0);
    [
        (ring[0].0, ring[0].1),
        (ring[1].0, ring[1].1),
        (ring[2].0, ring[2].1),
        (ring[3].0, ring[3].1),
    ]
}

/// `Team.get(id).color.toRgba8888()`.
fn team_color(team: u8) -> u32 {
    crate::game::team::Team::get(team).color.to_rgba8888()
}

/// RGBA8888 → `[f32; 4]` (`Color.toFloatBits`-free render conversion).
fn rgba_u32(rgba: u32) -> [f32; 4] {
    [
        ((rgba >> 24) & 0xff) as f32 / 255.0,
        ((rgba >> 16) & 0xff) as f32 / 255.0,
        ((rgba >> 8) & 0xff) as f32 / 255.0,
        (rgba & 0xff) as f32 / 255.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BlockId;
    use crate::determinism::SimCommand;
    use crate::sim::Sim;
    use crate::world::TilePos;

    fn row() -> AcceleratorDraw {
        AcceleratorDraw {
            id: 1,
            x: 100.0,
            y: 60.0,
            size: 7,
            team: 1,
            progress: 0.5,
            heat: 0.5,
            status_lerp: 1.0,
            time: 30.0,
            launch_heat: 0.0,
            launching: false,
            efficiency: 1.0,
            launch_duration: 120.0,
            charge_duration: 220.0,
            build_duration: 120.0,
        }
    }

    #[test]
    fn rest_arrows_match_the_java_ring() {
        let arrows = rest_arrows(&row());
        assert_eq!(arrows.len(), 16, "4 radii x 4 diagonals");
        // Radius lengths 7/12/17/22 on the 45deg diagonal.
        let first = arrows[0];
        assert!((first.x - (100.0 + 7.0 * 45f32.to_radians().cos())).abs() < 1e-4);
        assert!((first.y - (60.0 + 7.0 * 45f32.to_radians().sin())).abs() < 1e-4);
        assert_eq!(first.rotation, 225.0);
        // Ring 3 (l = 3) sits 22 units out.
        let ring3 = arrows[12];
        let (dx, dy) = drawf::trns(45.0, 22.0);
        assert!((ring3.x - (100.0 + dx)).abs() < 1e-4);
        assert!((ring3.y - (60.0 + dy)).abs() < 1e-4);
        // A fully status-lit arrow lerps team -> darkMetal by the absin pulse.
        let pulse = drawf::absin(30.0, 10.0, 1.0);
        let team = rgba_u32(team_color(1));
        let expected = drawf::lerp(
            drawf::lerp(drawf::pal::DARK_METAL, team, 1.0),
            drawf::pal::DARK_METAL,
            pulse,
        );
        assert_eq!(first.color, expected);
    }

    #[test]
    fn heat_geometry_scales_with_heat_and_launch_heat() {
        let mut hot = row();
        hot.heat = 1.0;
        let geo = heat_geometry(&hot).expect("heat");
        assert_eq!(geo.squares.len(), 3);
        assert_eq!(geo.arrows.len(), 4);
        assert!((geo.squares[0].stroke - 1.75).abs() < 1e-6);
        assert!((geo.squares[1].stroke - 3.0).abs() < 1e-6);
        assert_eq!(geo.arrows[0].alpha, 1.0);

        let cold = AcceleratorDraw { heat: 0.0, ..row() };
        assert!(
            heat_geometry(&cold).is_none(),
            "heat < 0.0001 returns early"
        );

        let flying = AcceleratorDraw {
            heat: 0.5,
            launch_heat: 1.0,
            ..row()
        };
        let geo = heat_geometry(&flying).unwrap();
        // `rad = size*8/2*0.74*lerp(1,1.3,1)` = 26.936; the first square is
        // 1.22 rad and the heat arrows 26 * heat * 1.5 = 19.5 out.
        let rad = 7.0 * 8.0 / 2.0 * 0.74 * 1.3;
        let (dx, dy) = drawf::trns(45.0 - flying.time / 3.0, 19.5);
        assert!((geo.arrows[0].x - (100.0 + dx)).abs() < 1e-3);
        assert!((geo.arrows[0].y - (60.0 + dy)).abs() < 1e-3);
        let (sx, sy) = drawf::trns(-45.0, rad * 1.22);
        assert!((geo.squares[0].points[0].0 - (100.0 + sx)).abs() < 1e-3);
        assert!((geo.squares[0].points[0].1 - (60.0 + sy)).abs() < 1e-3);
    }

    #[test]
    fn launch_phase_matches_the_340_tick_split() {
        let start = LaunchPhase::from_land_time_in(1.0, 120.0, 220.0);
        assert_eq!(start.raw_time, 0.0);
        assert_eq!(start.fin, 1.0);
        assert_eq!(start.fout, 0.0);
        assert_eq!(start.charge_fin, 1.0);
        assert_eq!(start.charge_fout, 0.0);
        assert_eq!(start.launch_time, 0.0);

        // Charge ends after 220 of the 340 ticks: fin_in = (340-220)/340.
        let boundary = LaunchPhase::from_land_time_in((340.0 - 220.0) / 340.0, 120.0, 220.0);
        assert!((boundary.raw_time - 220.0).abs() < 1e-3);
        assert!((boundary.charge_fin - 0.0).abs() < 1e-4);
        assert!((boundary.charge_fout - 1.0).abs() < 1e-4);
        assert!((boundary.fin - 1.0).abs() < 1e-4, "launch half starts");
        assert!((boundary.launch_tint(220.0) - 1.0).abs() < 1e-4);

        let end = LaunchPhase::from_land_time_in(0.0, 120.0, 220.0);
        assert_eq!(end.fin, 0.0);
        assert_eq!(end.fout, 1.0);
        assert_eq!(end.charge_fout, 1.0);
    }

    #[test]
    fn charge_rings_appear_during_the_charge_and_fade_on_launch() {
        let start = LaunchPhase::from_land_time_in(1.0, 120.0, 220.0);
        assert!(charge_rings(&start).is_empty(), "no ring at tick 0");
        let boundary = LaunchPhase::from_land_time_in((340.0 - 220.0) / 340.0, 120.0, 220.0);
        let rings = charge_rings(&boundary);
        assert_eq!(rings.len(), CHARGE_RINGS, "all four at charge end");
        for (i, ring) in rings.iter().enumerate() {
            let expected = RING_RAD_BASE + RING_RAD_SPACING * (i as f32).powf(RING_RAD_POW);
            assert!((ring.radius - expected).abs() < 1e-4);
            assert_eq!(ring.fin, 1.0);
        }
        // Just after the charge starts fading the rings out.
        let fade = LaunchPhase::from_land_time_in((340.0 - 250.0) / 340.0, 120.0, 220.0);
        let rings = charge_rings(&fade);
        assert_eq!(rings.len(), CHARGE_RINGS);
        assert!(rings.iter().any(|ring| ring.fin < 1.0), "outer rings fade");
        // After the intro fraction `charge_fout - fout * 2` is zero.
        let done = LaunchPhase::from_land_time_in(0.1, 120.0, 220.0);
        assert!(charge_rings(&done).is_empty());
    }

    #[test]
    fn ring_geometry_rotates_and_scales_like_draw_ring() {
        let full = ring_geometry(100.0, 60.0, 60.0, 1.0);
        assert_eq!(full.radius, 60.0);
        assert_eq!(full.stroke, 3.0);
        assert_eq!(full.color_mix, 1.0);
        assert_eq!(full.handles.len(), 4);
        assert_eq!(full.arrows.len(), 4);
        assert_eq!(full.arrow_scale, 1.0);
        let (bx, by) = drawf::trns(45.0, 60.0);
        assert!((full.handles[0][1].0 - (100.0 + bx)).abs() < 1e-4);
        assert!((full.handles[0][1].1 - (60.0 + by)).abs() < 1e-4);
        assert_eq!(full.arrows[0].2, 45.0 + 225.0);

        let fout = ring_geometry(100.0, 60.0, 60.0, 0.0);
        assert_eq!(fout.radius, 80.0, "radius + 20 * pow4In(1)");
        assert_eq!(fout.arrow_scale, 0.0);
        assert_eq!(fout.stroke, 0.0);
        // Handles rotate by pow4In(fout) * 90 = 90 degrees.
        let (bx, by) = drawf::trns(45.0 + 90.0, 80.0);
        assert!((fout.handles[0][1].0 - (100.0 + bx)).abs() < 1e-4);
        assert!((fout.handles[0][1].1 - (60.0 + by)).abs() < 1e-4);
    }

    #[test]
    fn launch_lightning_matches_the_java_burst() {
        let mut rand = crate::math::ArcRand::new(7);
        let bolts = launch_lightning(100.0, 60.0, &mut rand);
        assert_eq!(bolts.len(), LAUNCH_LIGHTNING);
        for bolt in &bolts {
            let (dx, dy) = drawf::trns(bolt.angle, LIGHTNING_OFFSET);
            assert!((bolt.x - (100.0 + dx)).abs() < 1e-3);
            assert!((bolt.y - (60.0 + dy)).abs() < 1e-3);
            assert!((0.0..360.0).contains(&bolt.angle));
            assert!((LIGHTNING_LENGTH_MIN..=LIGHTNING_LENGTH_MAX).contains(&bolt.length));
        }
    }

    /// `AcceleratorBuild.draw` reads the live ECS `CampaignState`; the row
    /// snapshot carries it to the Godot renderer.
    #[test]
    fn accelerator_rows_reads_live_campaign_state() {
        let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
        let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
        sim.grid.fill(stone, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        let block = sim
            .world_block_id("interplanetary-accelerator")
            .expect("accelerator");
        sim.command(SimCommand::Place {
            x: 10,
            y: 10,
            block,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("placed");
        let entity = sim.grid.entity_at(TilePos::new(10, 10)).expect("entity");
        if let Some(mut state) = sim.ecs.0.get_mut::<CampaignState>(entity) {
            state.progress = 0.75;
            state.heat = 0.25;
            state.status_lerp = 1.0;
            state.time = 12.0;
            state.launch_heat = 0.5;
            state.launching = true;
        }
        let rows = accelerator_rows(&mut sim.ecs.0);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.size, 7);
        assert_eq!(row.progress, 0.75);
        assert_eq!(row.heat, 0.25);
        assert_eq!(row.time, 12.0);
        assert!(row.launching);
        assert!(!row.core_built());
        assert_eq!(row.launch_total(), 340.0);
        assert_eq!(row.x, (10.0 + 3.0) * 8.0, "7x7 centre tile, offset 0");
        assert_eq!(row.y, (10.0 + 3.0) * 8.0);
    }
}
