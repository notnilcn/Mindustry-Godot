// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Accelerator.AcceleratorBuild.draw` pass
//! (world/blocks/campaign/Accelerator.java:140-236,413-540).
//!
//! Two nodes share the work: `Body` at `Layer.block` runs the resting arrow
//! ring, the launch-core construction (progress reveal) and the heat square/
//! arrow burst; `Effects` at `Layer.effect` runs the four `drawLaunchGlobalZ`
//! charge rings while the launch cutscene is live (fin from
//! `MindCamera2D.accelerator_launch_json`). The Godot-free math lives in
//! `mind_core::render::accelerator`.
//!
//! Not reproduced here (recorded residuals): the `blockbuild` scan shader on
//! the constructing core (the port fades/tints the core icon instead), the
//! `drawLaunch`/`drawLanding` ascent with core thrusters and clouds, the white
//! fade image, and the `Lightning`/`coreLandDust` world effects (sim-side
//! effects, wired by the host pump).

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{Camera2D, INode2D, Node2D, Texture2D};
use godot::obj::{Base, WithBaseField};
use godot::prelude::*;

use mind_core::render::accelerator as math;
use mind_core::render::drawf::pal;

use crate::render::canvas::{self, RegionCache};
use crate::sim_host::MindSimHost;

/// `Pal.accent` (`ffd37f`).
const ACCENT: u32 = 0xffd3_7fff;

/// Which slice of the accelerator draw this node runs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AcceleratorMode {
    /// Resting arrows, launch-core build and heat burst at `Layer.block`.
    Body,
    /// `drawLaunchGlobalZ` charge rings at `Layer.effect`.
    Effects,
}

/// One deferred draw command (textures resolved before the canvas borrow).
enum Cmd {
    Region {
        texture: Option<Gd<Texture2D>>,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rotation: f32,
        color: Color,
    },
    Lines {
        points: Vec<(f32, f32)>,
        stroke: f32,
        color: Color,
    },
    Light {
        x: f32,
        y: f32,
        radius: f32,
        center: Color,
        edge: Color,
    },
}

/// Accelerator draw pass (`Node2D` in its band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct AcceleratorRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    camera: Option<Gd<Camera2D>>,
    regions: RegionCache,
    /// `build_floor` assigns the mode before the node enters the tree.
    mode: AcceleratorMode,
}

#[godot_api]
impl INode2D for AcceleratorRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            camera: None,
            regions: RegionCache::new(),
            mode: AcceleratorMode::Body,
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
            // The arrow pulse, heat rotation and charge rings animate every
            // frame; progress changes without a world-revision bump.
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        // Collect every input first: `base_mut()` borrows `self` for the whole
        // draw, so no `self.*` read may follow it.
        let Some(host) = self.host.clone() else {
            return;
        };
        let rows = {
            let mut bound = host.clone();
            bound.bind_mut().accelerator_draws()
        };
        if rows.is_empty() {
            return;
        }
        let mode = self.mode;
        let node = self.to_gd().upcast::<Node2D>();
        let launch = self.launch_state();
        let mut cmds: Vec<Cmd> = Vec::new();
        match mode {
            AcceleratorMode::Body => {
                let launch_fin = launch.map(|(fin, _, _)| fin);
                for row in &rows {
                    push_body(self, &node, row, launch_fin, &mut cmds);
                }
            }
            AcceleratorMode::Effects => {
                let Some((fin, launch_duration, charge_duration)) = launch else {
                    return;
                };
                let Some(row) = rows.iter().find(|row| row.launching) else {
                    return;
                };
                push_charge_rings(
                    self,
                    &node,
                    row,
                    fin,
                    launch_duration,
                    charge_duration,
                    &mut cmds,
                );
            }
        }
        let mut base = self.base_mut();
        for cmd in cmds {
            match cmd {
                Cmd::Region {
                    texture,
                    x,
                    y,
                    w,
                    h,
                    rotation,
                    color,
                } => {
                    if let Some(texture) = texture {
                        canvas::draw_region(&mut base, &texture, x, y, w, h, rotation, color);
                    }
                }
                Cmd::Lines {
                    points,
                    stroke,
                    color,
                } => {
                    if points.len() >= 2 {
                        canvas::poly_line(&mut base, &points, stroke, color);
                    }
                }
                Cmd::Light {
                    x,
                    y,
                    radius,
                    center,
                    edge,
                } => {
                    if radius > 0.0 {
                        canvas::fill_light(&mut base, x, y, 15, radius, center, edge);
                    }
                }
            }
        }
    }
}

impl AcceleratorRenderer {
    /// Installs the draw mode (called from `MindWorldRenderer::build_floor`).
    pub(crate) fn set_mode(&mut self, mode: AcceleratorMode) {
        self.mode = mode;
    }

    /// Scene-wired camera (`MindWorldRenderer::camera`), used only by the
    /// `Effects` half for `accelerator_launch_json`.
    pub(crate) fn set_camera(&mut self, camera: Gd<Camera2D>) {
        self.camera = Some(camera);
    }

    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.base_mut().queue_redraw();
    }

    /// `renderer.getLandTimeIn()` for a live accelerator cutscene:
    /// `(fin, launchDuration, chargeDuration)` from
    /// `MindCamera2D.accelerator_launch_json`. `None` when no launch plays.
    fn launch_state(&self) -> Option<(f32, f32, f32)> {
        let mut camera = self.camera.clone()?;
        if !camera.has_method("accelerator_launch_json") {
            return None;
        }
        let json = camera.call("accelerator_launch_json", &[]).to::<GString>();
        let value: serde_json::Value = serde_json::from_str(&json.to_string()).ok()?;
        if !value.get("active")?.as_bool()? {
            return None;
        }
        let fin = value.get("fin")?.as_f64()? as f32;
        let duration = value.get("duration")?.as_f64()? as f32;
        let charge = value.get("charge_duration")?.as_f64()? as f32;
        // `duration` is `launchDuration()` = launch + charge (:322-324).
        Some((fin, (duration - charge).max(0.0), charge))
    }
}

/// `AcceleratorBuild.draw`'s non-launching/launching body
/// (`Accelerator.java:140-236`).
fn push_body(
    pass: &mut AcceleratorRenderer,
    node: &Gd<Node2D>,
    row: &math::AcceleratorDraw,
    launch_fin: Option<f32>,
    cmds: &mut Vec<Cmd>,
) {
    let accent = canvas::color_u32(ACCENT);
    let team = canvas::color_u32(mindustry_team_color(row.team));
    // Resting arrows: `<name>-launch-arrow` (`@-launch-arrow`), fallback
    // `launch-arrow` (`Accelerator.java:31`).
    let arrow = pass
        .regions
        .get(node, &format!("{}-launch-arrow", block_name()))
        .or_else(|| pass.regions.get(node, "launch-arrow"));
    let (arrow_w, arrow_h) = natural(&arrow);
    for a in math::rest_arrows(row) {
        cmds.push(Cmd::Region {
            texture: arrow.clone(),
            x: a.x,
            y: a.y,
            w: arrow_w,
            h: arrow_h,
            rotation: a.rotation,
            color: canvas::color_f4(a.color),
        });
    }

    if row.launching {
        // Launching core (`Accelerator.java:169-204`): additive light, the
        // launch block icon, then an accent/white double pass scaled by the
        // cutscene progress.
        let tint = launching_tint(row, launch_fin).unwrap_or(1.0);
        let core = pass.regions.get(node, "block-core-nucleus-full");
        let (core_w, core_h) = natural(&core);
        cmds.push(Cmd::Light {
            x: row.x,
            y: row.y,
            radius: 40.0,
            center: Color::from_rgba(accent.r, accent.g, accent.b, tint),
            edge: Color::from_rgba(accent.r, accent.g, accent.b, 0.0),
        });
        cmds.push(Cmd::Region {
            texture: core.clone(),
            x: row.x,
            y: row.y,
            w: core_w,
            h: core_h,
            rotation: 0.0,
            color: Color::WHITE,
        });
        let glow = mind_core::math::Interp::Pow2In.apply((tint * 0.7).clamp(0.0, 1.0));
        cmds.push(Cmd::Region {
            texture: core,
            x: row.x,
            y: row.y,
            w: core_w,
            h: core_h,
            rotation: 0.0,
            color: Color::from_rgba(accent.r, accent.g, accent.b, glow),
        });
    } else if row.progress > 0.0 {
        // `Drawf.shadow(x, y, launchBlock.size * tilesize * 2, progress)`.
        if let Some(shadow) = pass.regions.get(node, "circle-shadow") {
            // `Drawf.shadow(x, y, launchBlock.size * tilesize * 2, progress)`:
            // the third argument is the quad size, the fourth the alpha scale.
            let shadow_size = 5.0 * 8.0 * 2.0;
            cmds.push(Cmd::Region {
                texture: Some(shadow),
                x: row.x,
                y: row.y,
                w: shadow_size,
                h: shadow_size,
                rotation: 0.0,
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.4 * core_alpha(row.progress)),
            });
        }
        // `Shaders.blockbuild` reveal approximated by a progress alpha +
        // `Pal.accent` tint (the shader's scan line is host-only).
        let core = pass
            .regions
            .get(node, "block-core-nucleus-full")
            .or_else(|| pass.regions.get(node, "core-nucleus"));
        let alpha = core_alpha(row.progress);
        let (core_w, core_h) = natural(&core);
        cmds.push(Cmd::Region {
            texture: core.clone(),
            x: row.x,
            y: row.y,
            w: core_w,
            h: core_h,
            rotation: 0.0,
            color: Color::from_rgba(accent.r, accent.g, accent.b, alpha),
        });
        // The launch block's team overlay (`teamRegions[team.id]`,
        // `Accelerator.java:190-192`).
        let team_name = mind_core::game::team::Team::get(row.team).name.clone();
        let overlay = pass
            .regions
            .get(node, &format!("core-nucleus-team-{team_name}"))
            .or_else(|| pass.regions.get(node, "core-nucleus-team"));
        if overlay.is_some() {
            cmds.push(Cmd::Region {
                texture: overlay,
                x: row.x,
                y: row.y,
                w: core_w,
                h: core_h,
                rotation: 0.0,
                color: Color::from_rgba(team.r, team.g, team.b, alpha),
            });
        }
    }

    // `Accelerator.java:207-236`: heat squares + rotating team arrows.
    if let Some(heat) = math::heat_geometry(row) {
        for square in heat.squares {
            cmds.push(Cmd::Lines {
                points: square.points.to_vec(),
                stroke: square.stroke,
                color: accent,
            });
        }
        let arrow = pass.regions.get(node, "launch-arrow");
        let (heat_w, heat_h) = natural(&arrow);
        for a in heat.arrows {
            cmds.push(Cmd::Region {
                texture: arrow.clone(),
                x: a.x,
                y: a.y,
                w: heat_w,
                h: heat_h,
                rotation: a.rotation,
                color: Color::from_rgba(team.r, team.g, team.b, a.alpha),
            });
        }
    }
}

/// `AcceleratorBuild.drawLaunchGlobalZ` (`Accelerator.java:462-490`).
fn push_charge_rings(
    pass: &mut AcceleratorRenderer,
    node: &Gd<Node2D>,
    row: &math::AcceleratorDraw,
    fin: f32,
    launch_duration: f32,
    charge_duration: f32,
    cmds: &mut Vec<Cmd>,
) {
    let phase = math::LaunchPhase::from_land_time_in(fin, launch_duration, charge_duration);
    let command = canvas::color_f4(pal::COMMAND);
    let accent = canvas::color_u32(ACCENT);
    let arrow = pass.regions.get(node, "select-arrow-small");
    let (arrow_w, arrow_h) = natural(&arrow);
    for ring in math::charge_rings(&phase) {
        let geometry = math::ring_geometry(row.x, row.y, ring.radius, ring.fin);
        let color = Color::from_rgba(
            command.r + (accent.r - command.r) * geometry.color_mix,
            command.g + (accent.g - command.g) * geometry.color_mix,
            command.b + (accent.b - command.b) * geometry.color_mix,
            command.a + (accent.a - command.a) * geometry.color_mix,
        );
        for handle in geometry.handles {
            cmds.push(Cmd::Lines {
                points: handle.to_vec(),
                stroke: geometry.stroke,
                color,
            });
        }
        if geometry.arrow_scale <= 0.001 {
            continue;
        }
        for (x, y, rotation) in geometry.arrows {
            cmds.push(Cmd::Region {
                texture: arrow.clone(),
                x,
                y,
                w: arrow_w * geometry.arrow_scale,
                h: arrow_h * geometry.arrow_scale,
                rotation,
                color,
            });
        }
    }
}

/// The launch-cutscene tint of the launching-core pass
/// (`Accelerator.java:175-179`); `None` when no cutscene is live.
fn launching_tint(row: &math::AcceleratorDraw, fin: Option<f32>) -> Option<f32> {
    let fin = fin?;
    let phase = math::LaunchPhase::from_land_time_in(fin, row.launch_duration, row.charge_duration);
    Some(phase.launch_tint(row.charge_duration))
}

/// Core-build alpha (`Shaders.blockbuild` progress reveal approximation).
fn core_alpha(progress: f32) -> f32 {
    progress.clamp(0.0, 1.0)
}

/// Natural region size in world units (`0` when unresolved).
fn natural(texture: &Option<Gd<Texture2D>>) -> (f32, f32) {
    texture
        .as_ref()
        .map(RegionCache::natural_size)
        .unwrap_or((0.0, 0.0))
}

/// `Team.get(id).color` as RGBA8888.
fn mindustry_team_color(team: u8) -> u32 {
    mind_core::game::team::Team::get(team).color.to_rgba8888()
}

/// The accelerator's content name for its `<name>-launch-arrow` override.
fn block_name() -> &'static str {
    "interplanetary-accelerator"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_row(launching: bool) -> math::AcceleratorDraw {
        math::AcceleratorDraw {
            id: 0,
            x: 32.0,
            y: 32.0,
            size: 7,
            team: 1,
            progress: 1.0,
            heat: 1.0,
            status_lerp: 1.0,
            time: 0.0,
            launch_heat: 1.0,
            launching,
            efficiency: 1.0,
            launch_duration: 120.0,
            charge_duration: 220.0,
            build_duration: 120.0,
        }
    }

    #[test]
    fn core_build_alpha_clamps_progress() {
        assert_eq!(core_alpha(0.0), 0.0);
        assert_eq!(core_alpha(0.25), 0.25);
        assert_eq!(core_alpha(2.0), 1.0);
        assert_eq!(core_alpha(-1.0), 0.0);
    }

    /// `@Load(value = "@-launch-arrow", fallback = "launch-arrow")`
    /// (`Accelerator.java:31`): the port tries the accelerator-specific
    /// region, then the vanilla fallback.
    #[test]
    fn arrow_region_fallback_matches_the_annotation() {
        assert_eq!(block_name(), "interplanetary-accelerator");
        assert_eq!(
            format!("{}-launch-arrow", block_name()),
            "interplanetary-accelerator-launch-arrow"
        );
        assert_eq!(mindustry_team_color(1), 0xffd37fff);
    }

    /// `AcceleratorBuild.draw`'s launching-core tint is driven by the camera
    /// cutscene (`getLandTimeIn`), never by a stale `launching` flag alone.
    #[test]
    fn launching_core_tint_follows_the_cutscene_phase() {
        let row = test_row(true);
        assert_eq!(launching_tint(&row, None), None, "no camera -> no tint");
        assert_eq!(
            launching_tint(&row, Some(1.0)),
            Some(0.0),
            "cutscene start: launchTime 0 / chargeDuration"
        );
        assert_eq!(
            launching_tint(&row, Some(0.0)),
            Some(1.0),
            "cutscene end clamps to 1"
        );
    }

    /// The `Effects` half draws only while the cutscene reports `active`; the
    /// `drawLaunchGlobalZ` gates also need the launch phase to have started.
    #[test]
    fn launch_phase_requires_a_launching_row() {
        let row = test_row(true);
        assert!(row.launching);
        let start =
            math::LaunchPhase::from_land_time_in(1.0, row.launch_duration, row.charge_duration);
        assert!(math::charge_rings(&start).is_empty(), "no ring at tick 0");
        let after_charge =
            math::LaunchPhase::from_land_time_in((340.0 - 220.0) / 340.0, 120.0, 220.0);
        assert_eq!(math::charge_rings(&after_charge).len(), 4);
    }
}
