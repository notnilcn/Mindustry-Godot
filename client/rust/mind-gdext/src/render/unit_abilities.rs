// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live unit ability fields (L04 T2).
//!
//! `EnergyFieldAbility.draw` (`EnergyFieldAbility.java:83-115`) lands at
//! `Layer.bullet - 0.001`; `SuppressionFieldAbility.draw`
//! (`SuppressionFieldAbility.java:67-101`) lands at `Layer.effect`. Both are
//! shape-only, so one node per layer draws them from the host's
//! `unit_ability_draws` rows. ForceField/ShieldArc are drawn by the shield
//! pass because they share `Layer.shields` and its group material.

use godot::builtin::Color;
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::content::Rgba;
use mind_core::content::registries::pal;
use mind_core::content::registries::units::ability::AbilityKind;
use mind_core::math::interp::Interp;

use crate::sim_host::MindSimHost;

use super::canvas;

/// One stroked polyline `(points, width, color)`.
type Stroke = (Vec<(f32, f32)>, f32, Color);

/// Which ability family this node draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbilityFieldMode {
    /// `EnergyFieldAbility` (bullet layer, below bullets).
    EnergyField,
    /// `SuppressionFieldAbility` (effect layer).
    SuppressionField,
}

/// Live ability-field pass (`Node2D` under one band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct AbilityFieldRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    /// Ability family.
    pub(crate) mode: AbilityFieldMode,
}

#[godot_api]
impl INode2D for AbilityFieldRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            mode: AbilityFieldMode::EnergyField,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.host.is_some() {
            // All fields animate with `Time.time`.
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let draws = host.bind().unit_ability_draws();
        if draws.is_empty() {
            return;
        }
        let time = host.bind().get_tick() as f32 / 60.0;
        let mode = self.mode;
        let mut lower: Vec<(f32, f32, f32, Color)> = Vec::new();
        let mut fills: Vec<(f32, f32, f32, Color)> = Vec::new();
        let mut strokes: Vec<Stroke> = Vec::new();
        for ability in &draws {
            if ability.kind != mode_kind(mode) {
                continue;
            }
            match mode {
                AbilityFieldMode::EnergyField => {
                    let (dx, dy) = (
                        mind_core::fx::angles::trnsx_vec(
                            ability.rotation - 90.0,
                            ability.spec.x,
                            ability.spec.y,
                        ),
                        mind_core::fx::angles::trnsy_vec(
                            ability.rotation - 90.0,
                            ability.spec.x,
                            ability.spec.y,
                        ),
                    );
                    let rx = ability.x + dx;
                    let ry = ability.y + dy;
                    let orb_radius = ability.spec.effect_radius
                        * (1.0
                            + mind_core::render::drawf::absin(
                                time,
                                ability.spec.blink_scl,
                                ability.spec.blink_size,
                            ));
                    let color = color_of(ability.spec.color.unwrap_or(pal::HEAL));
                    fills.push((rx, ry, orb_radius, color));
                    fills.push((rx, ry, orb_radius / 2.0, Color::WHITE));
                    let stroke =
                        0.7 + mind_core::render::drawf::absin(time, ability.spec.blink_scl, 0.7);
                    let sectors = ability.spec.sectors.max(1);
                    let sector_frac = ability.spec.sector_rad;
                    let mut colored = Vec::new();
                    for i in 0..sectors {
                        let rot = ability.rotation + i as f32 * 360.0 / sectors as f32
                            - time * ability.spec.rotate_speed;
                        colored.push(arc_points(rx, ry, orb_radius + 3.0, sector_frac, rot));
                    }
                    for points in colored {
                        strokes.push((points, stroke, color));
                    }
                    if ability.state.cur_stroke > 0.0 {
                        for i in 0..sectors {
                            let rot = ability.rotation
                                + i as f32 * 360.0 / sectors as f32
                                + time * ability.spec.rotate_speed;
                            strokes.push((
                                arc_points(rx, ry, ability.spec.range, sector_frac, rot),
                                stroke * ability.state.cur_stroke,
                                color,
                            ));
                        }
                    }
                }
                AbilityFieldMode::SuppressionField => {
                    let (dx, dy) = (
                        mind_core::fx::angles::trnsx_vec(
                            ability.rotation - 90.0,
                            ability.spec.x,
                            ability.spec.y,
                        ),
                        mind_core::fx::angles::trnsy_vec(
                            ability.rotation - 90.0,
                            ability.spec.x,
                            ability.spec.y,
                        ),
                    );
                    let rx = ability.x + dx;
                    let ry = ability.y + dy;
                    let rad = ability.spec.orb_radius
                        + mind_core::render::drawf::absin(
                            time,
                            ability.spec.orb_sin_scl,
                            ability.spec.orb_sin_mag,
                        );
                    let color = color_of(ability.spec.color.unwrap_or(pal::SUPPRESS));
                    let particle_color = color_of(ability.spec.particle_color.unwrap_or(pal::SAP));
                    // `rand.setSeed(unit.id + hashCode())` is per-frame and
                    // deterministic in Java; a splitmix seed reproduces the
                    // stable placement without a global RNG.
                    let mut seed = ability.id ^ (rad.to_bits() as u64).rotate_left(17);
                    let base = time / ability.spec.particle_life.max(0.001);
                    for _ in 0..ability.spec.particles.max(0) {
                        let fin = (rand_unit(&mut seed) + base).rem_euclid(1.0);
                        let fout = 1.0 - fin;
                        let angle = rand_unit(&mut seed) * 360.0
                            + (time / ability.spec.rotate_scl.max(0.001) + ability.rotation)
                                .rem_euclid(360.0);
                        let len = ability.spec.particle_len
                            * Interp::CircleOut.apply(Interp::Slope.apply(fout));
                        let (sin, cos) = angle.to_radians().sin_cos();
                        let size = ability.spec.particle_size * Interp::Slope.apply(fin);
                        fills.push((rx + cos * len, ry + sin * len, size, particle_color));
                    }
                    lower.push((rx, ry, rad, color));
                    fills.push((rx, ry, rad * ability.spec.orb_mid_scl, color));
                }
            }
        }
        let mut base = self.base_mut();
        for (x, y, r, color) in fills {
            if r > 0.0 {
                canvas::fill_circle(&mut base, x, y, r, color);
            }
        }
        for (x, y, r, color) in lower {
            canvas::circle_stroke(&mut base, x, y, r, 2.0, color);
        }
        for (points, stroke, color) in strokes {
            canvas::poly_line(&mut base, &points, stroke, color);
        }
    }
}

impl AbilityFieldRenderer {
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.base_mut().queue_redraw();
    }
}

fn mode_kind(mode: AbilityFieldMode) -> AbilityKind {
    match mode {
        AbilityFieldMode::EnergyField => AbilityKind::EnergyField,
        AbilityFieldMode::SuppressionField => AbilityKind::SuppressionField,
    }
}

fn color_of(rgba: Rgba) -> Color {
    Color::from_rgba(rgba.r, rgba.g, rgba.b, rgba.a)
}

/// `Lines.arc(x, y, radius, fraction, rotation)`: an arc sweep in degrees.
fn arc_points(x: f32, y: f32, radius: f32, fraction: f32, rotation: f32) -> Vec<(f32, f32)> {
    let sweep = fraction * 360.0;
    let steps = ((sweep.abs() / 5.0).ceil() as usize).max(2);
    let mut points = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let angle = rotation + sweep * i as f32 / steps as f32;
        let (sin, cos) = angle.to_radians().sin_cos();
        points.push((x + cos * radius, y + sin * radius));
    }
    points
}

/// Splitmix64 mapped to `[0, 1)`.
fn rand_unit(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) >> 40) as f32 / (1u32 << 24) as f32
}
