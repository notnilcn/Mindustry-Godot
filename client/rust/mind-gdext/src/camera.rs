// SPDX-License-Identifier: GPL-3.0-only

//! `MindCamera2D` — the RTS camera rig node (plan 15 §3.11).
//!
//! All camera math lives in the Godot-free [`mind_core::input::CameraState`];
//! this node only polls the sibling `MindInput` bindings, applies the transform
//! and exposes the MCP probe API. Ported from
//! `core/src/mindustry/core/Renderer.java` (scale/shake),
//! `input/DesktopInput.java` (pan/zoom/follow/detach) and
//! `input/MobileInput.java` (edge pan). `Vars.tilesize = 8` comes from
//! `mind_core::config`.

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{Camera2D, ICamera2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::fx::FxEvent;
use mind_core::input::{CameraState, MinimapRegion};
use mind_core::render::cutscene::{
    AcceleratorLaunchAnimator, CoreLandAnimator, Cutscene, LandDustTimer,
};

use crate::input::MindInput;
use crate::sim_host::MindSimHost;

/// Default `screenshake` setting (`Renderer` reads it each frame).
const DEFAULT_SCREENSHAKE: i32 = 4;
/// `sliderPref("screenshake", 4, 0, 8, ...)` upper bound (2x shake).
const MAX_SCREENSHAKE: i32 = 8;
/// Distance from a viewport edge that starts edge panning (px, upstream base).
const EDGE_MARGIN: f32 = 60.0;

/// 2D camera over the tile grid.
#[derive(GodotClass)]
#[class(base=Camera2D)]
pub struct MindCamera2D {
    base: Base<Camera2D>,
    /// Pure camera rig state.
    camera: CameraState,
    /// Edge pan only when the OS cursor is actually inside the game window.
    mouse_inside: bool,
    /// Monotonic render tick (deterministic shake direction seed).
    view_tick: u64,
    /// `screenshake` setting `0..=8`.
    screenshake: i32,
    /// Last `(x, y)` position exchanged with the Godot node (echo guard).
    shadow_position: (f32, f32),
    /// Last zoom exchanged with the Godot node (echo guard).
    shadow_scale: f32,
    /// Scene sim host; a world-size change recenters the camera on it.
    host: Option<Gd<MindSimHost>>,
    /// Sibling `MindInput`; pan/boost come from the binding state.
    input: Option<Gd<MindInput>>,
    /// World size the rig last adopted (`Control` world-load camera snap).
    world_dims: (i32, i32),
    /// World-load counter the rig last adopted (recenters on a same-size load).
    world_loads: u64,
    /// Smooth-follow target (`None` = detached; manual pan clears it).
    follow_target: Option<(f32, f32)>,
    /// `Renderer` launch/land cutscene (`landTime`/`camerascale`); drives the
    /// camera scale while `is_cutscene()` (PAR-0131).
    cutscene: Cutscene,
    /// World position of the dropping core (`show_landing`), for the
    /// `landing_draw_json` sprite seam.
    landing_core: Option<(f32, f32)>,
    /// Block name of the dropping core (`block-<name>-full` region).
    landing_block: String,
    /// `CoreBuild.landParticleTimer` (`updateLaunch` dust bursts).
    land_dust: LandDustTimer,
    /// View-side `Mathf.rand` for the dust rolls (`chance`/`range`).
    land_dust_rand: mind_core::math::ArcRand,
    /// `Block.mapColor` per block id (`sprites/block_colors.png`), loaded on
    /// the first landing.
    block_colors: Option<Vec<u32>>,
    /// Interplanetary Accelerator launch centre (`show_accelerator_launch`).
    accelerator_launch: Option<(f32, f32)>,
    /// Whether the `MindInput` cutscene lock is currently raised.
    cutscene_locked: bool,
    /// Whether the `MindInput` logic-cutscene lock is currently raised (H06 T3).
    logic_locked: bool,
    /// `DesktopInput.panning`: set by the pan key, cleared by manual movement.
    panning: bool,
    /// `smoothcamera` setting (`DesktopInput.java:289` follow lerp 0.08 vs 1).
    smooth_camera: bool,
}

#[godot_api]
impl ICamera2D for MindCamera2D {
    fn init(base: Base<Camera2D>) -> Self {
        Self {
            base,
            camera: CameraState::new((1920.0, 1080.0)),
            mouse_inside: false,
            view_tick: 0,
            screenshake: DEFAULT_SCREENSHAKE,
            shadow_position: (0.0, 0.0),
            shadow_scale: 1.0,
            host: None,
            input: None,
            world_dims: (0, 0),
            world_loads: 0,
            cutscene: Cutscene::new(1.0),
            landing_core: None,
            landing_block: String::new(),
            land_dust: LandDustTimer::default(),
            land_dust_rand: mind_core::math::ArcRand::new(0x1a4d),
            block_colors: None,
            accelerator_launch: None,
            cutscene_locked: false,
            logic_locked: false,
            follow_target: None,
            panning: false,
            smooth_camera: true,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn process(&mut self, delta: f64) {
        // Shadow guard (godot-bevy `TransformSyncMetadata::shadow` pattern):
        // adopt any position Godot authored since our last write before stepping
        // the rig, so editor/direct-node moves are not immediately clobbered.
        let node_position = self.base().get_position();
        if node_position.x != self.shadow_position.0 || node_position.y != self.shadow_position.1 {
            self.camera.position = (node_position.x, node_position.y);
            self.shadow_position = (node_position.x, node_position.y);
            self.follow_target = None;
        }

        self.view_tick = self.view_tick.wrapping_add(1);
        self.read_view_settings();
        if let Some(viewport) = self.base().get_viewport() {
            let size = viewport.get_visible_rect().size;
            self.camera.viewport = (size.x, size.y);
        }
        self.follow_world();

        // H06 T3: adopt the world-processor cutscene state (`InputHandler`
        // reads `logicCutscene`/`logicCamPan`/`logicCamSpeed`/
        // `logicCutsceneZoom`; while `Renderer.isCutscene()` runs the zoom is
        // forced to -1 and the pan is suppressed, `InputHandler.java:946-950`).
        let logic = self
            .host
            .as_ref()
            .map(|host| host.bind().logic_cutscene_state())
            .unwrap_or_default();
        let landing = self.cutscene.is_cutscene();
        if landing {
            self.camera.logic_cutscene_zoom = -1.0;
        } else {
            self.camera.logic_cutscene = logic.active;
            self.camera.logic_cam_pan = logic.pan;
            self.camera.logic_cam_speed = logic.speed;
            self.camera.logic_cutscene_zoom = if logic.active { logic.zoom } else { -1.0 };
        }

        // Java clamps its global `Time.delta` to `[0.0001, maxDeltaClient]`
        // (`ClientLauncher.java:84-87`, `Vars.java:181`); the port applies the
        // same clamp to the camera rig's frame delta (T3 step 1).
        let delta_frames = ((delta * 60.0) as f32).clamp(0.0001, 4.0);
        // `InputHandler.inputLocks` includes `logicCutscene`: while a logic
        // cutscene runs the player follow and every pan path are skipped
        // (`DesktopInput.java:276` `if(!locked)`).
        if !self.camera.logic_cutscene {
            // Possessed unit: the move axis drives the unit (`MindSimHost` uploads
            // it each tick) and the camera follows the unit instead of panning
            // (`DesktopInput.updateMovement` / `:283-289`).
            let player = self
                .host
                .as_ref()
                .and_then(|host| host.bind().player_unit_position());
            let paused = self
                .host
                .as_ref()
                .map(|host| host.bind().is_paused())
                .unwrap_or(false);
            let (axis_x, axis_y) = self.pan_axis();
            let boost = self.boost_pressed();
            let pan_key = self.pan_pressed();
            let mouse_move = self.mouse_move_pressed();
            let active = self.gameplay_active();
            // `DesktopInput.java:270-273`: the pan latch drops on any manual
            // movement input (axis or `mouseMove`).
            if active {
                if pan_key {
                    self.panning = true;
                }
                if axis_x != 0.0 || axis_y != 0.0 || mouse_move {
                    self.panning = false;
                }
            }
            let mut pan_cam = pan_key && active;
            if !self.locked() {
                if (player.is_none() || paused) && active {
                    // `DesktopInput.java:277-283`: desktop pans with WASD while
                    // the player is dead or the game is paused.
                    if mouse_move {
                        pan_cam = true;
                    }
                    self.camera.pan_axis(axis_x, axis_y, delta_frames, boost);
                    self.follow_target = None;
                } else if let Some((player_x, player_y)) = player
                    && !self.panning
                {
                    self.camera
                        .follow(player_x, player_y, self.smooth_camera, delta_frames);
                    self.follow_target = Some((player_x, player_y));
                }
                if pan_cam {
                    // Arc `mouseY` is bottom-up; the tracked cursor is in
                    // Godot viewport coords (top-down) - flip. Reading
                    // `MindInput`'s tracked position (not the viewport getter)
                    // keeps synthetic bridge events in sync.
                    let (mouse_x, mouse_y) = self.mouse_position();
                    self.camera.pan_mouse(
                        mouse_x,
                        self.camera.viewport.1 - mouse_y,
                        delta_frames,
                        boost,
                    );
                }
            }

            // Edge pan is `MobileInput`-only; the desktop port used to run it
            // whenever no player existed (T10).
            if godot::classes::Os::singleton().has_feature("mobile")
                && player.is_none()
                && self.mouse_inside
                && active
                && let Some(viewport) = self.base().get_viewport()
            {
                let rect = viewport.get_visible_rect();
                let mouse = viewport.get_mouse_position();
                let margin = EDGE_MARGIN;
                if mouse.x < rect.position.x + margin
                    || mouse.x > rect.position.x + rect.size.x - margin
                    || mouse.y < rect.position.y + margin
                    || mouse.y > rect.position.y + rect.size.y - margin
                {
                    self.camera.auto_pan(mouse.x, mouse.y);
                    self.follow_target = None;
                }
            }
        }
        // `MobileInput.pan` clamp for the manual/edge pans: without it the port
        // edge-panned off the map indefinitely (PAR-0031; the pointer resting
        // near a window edge walked the camera to y=32870 on a 256x256 map).
        self.camera.clamp_position();

        if !landing {
            self.camera.step_logic_cutscene(delta_frames);
        }
        self.camera
            .update(delta_frames, self.screenshake, self.view_tick);

        // `CutsceneI shake` (`Renderer.shake(p1, p2 * 60)`): the pending request
        // is consumed once (`InputHandler` forwards it the frame it arrives).
        if let Some(mut host) = self.host.clone()
            && let Some((intensity, duration)) = host.bind_mut().take_logic_shake()
        {
            self.camera.on_fx_event(&FxEvent::Shake {
                intensity,
                duration,
            });
        }
        // `InputHandler.inputLocks` `logicCutscene` mirror (H06 T3).
        let logic_active = self.camera.logic_cutscene;
        let was_logic_locked = std::mem::replace(&mut self.logic_locked, logic_active);
        if logic_active != was_logic_locked
            && let Some(mut input) = self.input.clone()
        {
            input.bind_mut().set_logic_cutscene_lock(logic_active);
        }

        // `Renderer.update` (PAR-0131): a landing cutscene replaces the display
        // scale with the animator's `zoomLaunch()` (0.02 -> 4, ending at the
        // normal scale, `Renderer.java:186,198`); otherwise the rig's own scale.
        let sim_paused = self
            .host
            .as_ref()
            .map(|host| host.bind().is_paused())
            .unwrap_or(false);
        // `Renderer.update`: `if(!state.isPaused()) launchAnimator.updateLaunch()`
        // runs before `landTime -= delta` (Renderer.java:183-188).
        if !sim_paused && self.cutscene.is_cutscene() && !self.cutscene.is_launching() {
            self.update_land_dust(delta_frames);
        }
        self.cutscene.update(delta_frames, sim_paused);
        let in_cutscene = self.cutscene.is_cutscene();
        if !in_cutscene {
            self.accelerator_launch = None;
        }
        // `AcceleratorBuild.zoomLaunch` (Accelerator.java:394-397):
        // `camera.position.set(x, y).add(randomDirection * shake * 2)`.
        if let Some((ax, ay)) = self.accelerator_launch {
            let mut probe = AcceleratorLaunchAnimator::new(120.0, 220.0);
            mind_core::render::cutscene::LaunchAnimator::set_land_time_in(
                &mut probe,
                self.cutscene.get_land_time_in(),
            );
            let shake = probe.shake() * 2.0;
            // View-only jitter (Java `Tmp.v1.setToRandomDirection()`).
            let angle = (self.view_tick as f32 * 2.399_963).rem_euclid(std::f32::consts::TAU);
            self.camera.position = (ax + angle.cos() * shake, ay + angle.sin() * shake);
            self.follow_target = Some(self.camera.position);
        }
        let was_cutscene = std::mem::replace(&mut self.cutscene_locked, in_cutscene);
        if in_cutscene {
            // `Renderer.update` writes `camerascale = zoomLaunch()` into the
            // renderer's own scale, so when the landing ends the normal
            // `lerpDelta(camerascale, targetscale, 0.1)` eases from the last
            // landing zoom instead of snapping (Renderer.java:160-199).
            self.camera.camerascale = self.cutscene.camerascale();
        }
        if in_cutscene != was_cutscene
            && let Some(mut input) = self.input.clone()
        {
            // `InputHandler.inputLocks` includes `renderer::isCutscene`.
            input.bind_mut().set_cutscene_lock(in_cutscene);
        }
        let zoom = self.camera.camerascale.max(0.01);
        if zoom != self.shadow_scale {
            // Java/Arc projects world `+y` to screen-up (`Camera.unproject`
            // flips the y axis); Godot 2D maps `+y` to screen-down. The world
            // coordinates already match Java numerically (core at file rows
            // 54-56, camera ~= Java `viewpos`), so negate the rendered y axis
            // here: the canvas transform flips and `tile_to_screen`/
            // `screen_to_tile` (both built from it) follow.
            self.base_mut().set_zoom(Vector2::new(zoom, -zoom));
            self.shadow_scale = zoom;
        }
        let (x, y) = self.camera.render_position();
        if x != self.shadow_position.0 || y != self.shadow_position.1 {
            self.base_mut().set_position(Vector2::new(x, y));
            self.shadow_position = (x, y);
        }
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        match what {
            CanvasItemNotification::WM_MOUSE_ENTER => self.mouse_inside = true,
            CanvasItemNotification::WM_MOUSE_EXIT => self.mouse_inside = false,
            CanvasItemNotification::EXTENSION_RELOADED => self.bootstrap(),
            _ => {}
        }
    }
}

#[godot_api]
impl MindCamera2D {
    /// `Renderer.showLanding(core)` + `Trigger.newGame` (`Renderer.java:567-574`,
    /// `Control.java:198-222`; PAR-0131): center the camera on the dropped core
    /// and run the 160-tick `CoreLandAnimator` zoom. `core_x`/`core_y` are world
    /// coordinates (`MindSimHost.best_core_position`); `core_block` names the
    /// dropped core (used by the renderer's scaled-sprite draw).
    #[func]
    pub fn show_landing(&mut self, core_x: f32, core_y: f32, core_block: GString) -> bool {
        self.camera.position = (core_x, core_y);
        self.follow_target = Some((core_x, core_y));
        self.landing_core = Some((core_x, core_y));
        self.landing_block = core_block.to_string();
        self.land_dust = LandDustTimer::default();
        // A landing replaces any accelerator launch cutscene (the sector
        // switch at `launchDuration() - 6`): drop its camera pin.
        self.accelerator_launch = None;
        self.cutscene = Cutscene::new(self.camera.camerascale.max(0.01));
        self.cutscene
            .show_landing(Box::new(CoreLandAnimator::new()));
        self.camera.camerascale = self.cutscene.camerascale();
        // The input lock follows in `process` (no sim-host borrow from here:
        // callers may hold one).
        true
    }

    /// `renderer.showLaunch(accelerator)` (Accelerator.java:270,
    /// `Renderer.java:576-586`): centre on the accelerator at world `(x, y)`
    /// and run the 340-tick `AcceleratorLaunchAnimator` (charge zoom + shake,
    /// then the launch zoom out).
    #[func]
    pub fn show_accelerator_launch(&mut self, x: f32, y: f32) -> bool {
        self.camera.position = (x, y);
        self.follow_target = Some((x, y));
        self.landing_core = None;
        self.accelerator_launch = Some((x, y));
        self.cutscene = Cutscene::new(self.camera.camerascale.max(0.01));
        self.cutscene
            .show_launch(Box::new(AcceleratorLaunchAnimator::new(120.0, 220.0)));
        true
    }

    /// The accelerator launch's draw/sequence inputs for the overlay
    /// (`accelerator_launch.gd`): `{"active", "x", "y", "fin", "raw_time",
    /// "duration", "charge_duration"}` (`getLandTimeIn`, `launchDuration() -
    /// getLandTime()`).
    #[func]
    pub fn accelerator_launch_json(&self) -> GString {
        let (x, y) = self.accelerator_launch.unwrap_or((0.0, 0.0));
        let fin = self.cutscene.get_land_time_in();
        let value = serde_json::json!({
            "active": self.accelerator_launch.is_some() && self.cutscene.is_cutscene(),
            "x": x,
            "y": y,
            "fin": fin,
            "raw_time": 340.0 - fin * 340.0,
            "duration": 340.0,
            "charge_duration": 220.0,
        });
        GString::from(value.to_string().as_str())
    }

    /// `CoreBlock.drawLanding` inputs for the world-space core sprite
    /// (`landing_core.gd`): `{"active", "x", "y", "block", "fin"}`. The white
    /// fade-in and the sector announce are driven from the same state by the
    /// fragment.
    #[func]
    pub fn landing_draw_json(&self) -> GString {
        let (x, y) = self.landing_core.unwrap_or((0.0, 0.0));
        // `CoreBuild.team` name for the `<block>-team-<team>` region
        // (`teamRegions[team.id]`, CoreBlock.java:523-525).
        let team = self
            .host
            .as_ref()
            .map(|host| host.bind().player_team())
            .map(|id| mind_core::game::team::Team::get(id).name.clone())
            .unwrap_or_default();
        let value = serde_json::json!({
            "active": self.cutscene.is_cutscene(),
            "team": team,
            "x": x,
            "y": y,
            "block": self.landing_block,
            "fin": self.cutscene.get_land_time_in(),
            // `renderer.getDisplayScale()` (the cutscene camera scale): the
            // landing draw scales by `Scl.scl(4) / getDisplayScale()`.
            "scale": self.cutscene.camerascale(),
        });
        GString::from(value.to_string().as_str())
    }

    /// `CoreBlock.drawLaunch` streak rays (G03 T18): `Angles.randLenVectors(1,
    /// fin, 100, length)` rows `[ax, ay, angleDeg, ffin]`; the fragment strokes
    /// each ray `ffin * pf * 3` wide and `(ffin * 20 + 1)` long from
    /// `(x + ax, y + ay)` toward `angleDeg`.
    #[func]
    pub fn landing_streaks_json(&self, fin: f64, length: f64) -> GString {
        let mut rows: Vec<serde_json::Value> = Vec::new();
        mind_core::fx::angles::rand_len_vectors_fin(
            1,
            fin as f32,
            100,
            length as f32,
            |ax, ay, ffin, _ffout| {
                let angle = ay.atan2(ax).to_degrees();
                rows.push(serde_json::json!([ax, ay, angle, ffin]));
            },
        );
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// `Renderer.isCutscene()` (landing/launch animation running).
    #[func]
    pub fn is_cutscene(&self) -> bool {
        self.cutscene.is_cutscene()
    }

    /// `Renderer.getLandTimeIn()` (`0..1` through the landing).
    #[func]
    pub fn get_land_time_in(&self) -> f32 {
        self.cutscene.get_land_time_in()
    }

    /// `CoreBuild.updateLaunch` (`CoreBlock.java:660-673`): when the
    /// `landParticleTimer` fills, every linked core tile spawns
    /// `Fx.coreLandDust` with chance 0.4, aimed away from the core
    /// (`angleTo(tile) + range(30)`) and tinted `floor.mapColor * (1.5 +
    /// range(0.15))`. View-only effect spawn (`MindFx`).
    fn update_land_dust(&mut self, delta_frames: f32) {
        use mind_core::render::cutscene::LaunchAnimator;
        let Some((cx, cy)) = self.landing_core else {
            return;
        };
        let fin = self.cutscene.get_land_time_in();
        if !self
            .land_dust
            .step(fin, CoreLandAnimator::new().launch_duration(), delta_frames)
        {
            return;
        }
        let Some(host) = self.host.as_ref() else {
            return;
        };
        let tiles = host.bind().linked_tile_floors(cx, cy);
        let Some(effect) = mind_core::content::effect_by_name("coreLandDust") else {
            return;
        };
        let Some(mut fx) = self
            .base()
            .try_get_node_as::<crate::fx::MindFx>("/root/Spine/MindFx")
        else {
            return;
        };
        let mut fx = fx.bind_mut();
        let colors = self
            .block_colors
            .get_or_insert_with(crate::render::minimap::load_block_colors);
        for (tx, ty, floor, registry_color) in tiles {
            // `floor().mapColor`: the registry colour `ContentLoader.loadColors`
            // filled from `block_colors.png` (alpha forced to 1); the raw row
            // is only a fallback for a registry without colours.
            let color = registry_color
                .or_else(|| {
                    colors
                        .get(floor)
                        .map(|packed| mind_core::content::Rgba::from_rgba8888(*packed | 0xff))
                })
                .unwrap_or(mind_core::content::Rgba::WHITE);
            if !self.land_dust_rand.chance(0.4) {
                continue;
            }
            // `Angles.angle(x, y, tx, ty)` in degrees `0..360`.
            let angle = (ty - cy).atan2(tx - cx).to_degrees().rem_euclid(360.0)
                + self.land_dust_rand.range_float(30.0);
            // `Tmp.c1.set(mapColor).mul(1.5 + range(0.15))`: rgb only, clamped.
            let mul = 1.5 + self.land_dust_rand.range_float(0.15);
            let tint = mind_core::content::Rgba {
                r: (color.r * mul).clamp(0.0, 1.0),
                g: (color.g * mul).clamp(0.0, 1.0),
                b: (color.b * mul).clamp(0.0, 1.0),
                a: color.a.clamp(0.0, 1.0),
            };
            fx.spawn_view_effect(effect, tx, ty, angle, tint);
        }
    }

    /// Rebuilds the rig's Godot-derived state (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`).
    fn bootstrap(&mut self) {
        self.base_mut().make_current();
        // Scene-wired sim host (tscn-first): the spine declares SimHost as a
        // sibling of World, so the camera follows world loads.
        self.host = self.base().try_get_node_as::<MindSimHost>("../../SimHost");
        // Sibling `Input` owns the binding state (`BindingState`).
        self.input = self.base().try_get_node_as::<MindInput>("../../Input");
        // `smoothcamera` (T10): the WorldLoadEvent re-reads it too.
        self.smooth_camera = self.read_smooth_camera();
        let position = self.base().get_position();
        self.camera.position = (position.x, position.y);
        // Seed the echo-guard shadows from the node so no write is issued for
        // state Godot already owns.
        self.shadow_position = (position.x, position.y);
        self.shadow_scale = self.base().get_zoom().x;
        log::debug!(
            "MindCamera2D ready at {:?} zoom {:?}",
            self.base().get_position(),
            self.base().get_zoom()
        );
    }

    /// `Control` `WorldLoadEvent`: on a world load, set the pan bounds and snap
    /// the camera to the player's best core (`camera.position.set(player.bestCore())`),
    /// falling back to the world middle when the map has no core.
    fn follow_world(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let (width, height) = host.bind().world_size();
        let loads = host.bind().world_loads();
        if width <= 0
            || height <= 0
            || ((width, height) == self.world_dims && loads == self.world_loads)
        {
            return;
        }
        self.world_dims = (width, height);
        self.world_loads = loads;
        // A world load re-reads `smoothcamera` (`Control` WorldLoadEvent).
        self.smooth_camera = self.read_smooth_camera();
        let unit = TILESIZE as f32;
        self.camera.world_size = Some((width as f32 * unit, height as f32 * unit));
        match host.bind().best_core_position_px() {
            Some((x, y)) => {
                self.camera.pan_camera(x, y);
                self.follow_target = Some((x, y));
            }
            None => {
                self.camera.center_on_tile(width / 2, height / 2);
                self.follow_target = None;
            }
        }
        let (x, y) = self.camera.render_position();
        self.base_mut().set_position(Vector2::new(x, y));
        self.shadow_position = (x, y);
    }

    /// Binding-driven pan axis from the sibling `MindInput`.
    fn pan_axis(&self) -> (f32, f32) {
        let Some(input) = self.input.clone() else {
            return (0.0, 0.0);
        };
        let guard = input.bind();
        let axis = guard.pan_axis();
        (axis.x, axis.y)
    }

    /// Binding-driven `boost` hold.
    fn boost_pressed(&self) -> bool {
        let Some(input) = self.input.clone() else {
            return false;
        };
        input.bind().boost_pressed()
    }

    /// Binding-driven `pan` hold (middle mouse by default).
    fn pan_pressed(&self) -> bool {
        let Some(input) = self.input.clone() else {
            return false;
        };
        input.bind().pan_pressed()
    }

    /// `mouseMove` binding hold (`DesktopInput.java:994-996`, T9/T10).
    fn mouse_move_pressed(&self) -> bool {
        let Some(input) = self.input.clone() else {
            return false;
        };
        input.bind().mouse_move_pressed()
    }

    /// Tracked cursor position (`Core.input.mouseX/Y`; viewport coordinates).
    fn mouse_position(&self) -> (f32, f32) {
        let Some(input) = self.input.clone() else {
            return (0.0, 0.0);
        };
        input.bind().mouse_position()
    }

    /// `InputHandler.locked()` (T10): any input lock skips the pan/follow.
    fn locked(&self) -> bool {
        let Some(input) = self.input.clone() else {
            return false;
        };
        input.bind().locked()
    }

    /// `Renderer.update` per-frame settings reads: `screenshake`
    /// (`Renderer.java:210`, slider `0..8`, `/ 4f`) and the magnification
    /// multipliers behind `maxZoomInGame`/`minZoomInGame`
    /// (`Renderer.java:173-174`; written by the Graphics sliders).
    fn read_view_settings(&mut self) {
        let shake = crate::settings_hub::row_i32("screenshake");
        let (max_mult, min_mult) = crate::settings_hub::with_settings(|store| {
            (
                store.get_f32("maxzoomingamemultiplier", 1.0),
                store.get_f32("minzoomingamemultiplier", 1.0),
            )
        });
        self.screenshake = shake.clamp(0, MAX_SCREENSHAKE);
        self.camera.max_zoom_multiplier = max_mult;
        self.camera.min_zoom_multiplier = min_mult;
    }

    /// `smoothcamera` setting (`/root/MindUi` owns the store; default true).
    fn read_smooth_camera(&self) -> bool {
        let Some(mut ui) = self
            .base()
            .try_get_node_as::<godot::classes::Node>("/root/MindUi")
        else {
            return true;
        };
        ui.call(
            "settings_get_bool",
            &[GString::from("smoothcamera").to_variant()],
        )
        .try_to::<bool>()
        .unwrap_or(true)
    }

    /// Whether the world accepts gameplay input (no dialog/text field open).
    fn gameplay_active(&self) -> bool {
        let Some(input) = self.input.clone() else {
            return true;
        };
        input.bind().gameplay_input_active()
    }

    /// Viewport position → tile `(x, y)`.
    ///
    /// `(x, y)` is a viewport-space point (e.g. `InputEventMouse.position`);
    /// the tile centre is `(x * 8, y * 8)` (Java `Tile.worldx()`), so the tile
    /// index is `floor((world + 4) / 8)`.
    #[func]
    pub fn screen_to_tile(&self, x: f64, y: f64) -> Vector2i {
        let canvas = self.base().get_canvas_transform();
        let world = canvas.affine_inverse() * Vector2::new(x as f32, y as f32);
        let size = TILESIZE as f32;
        Vector2i::new(
            ((world.x + size / 2.0) / size).floor() as i32,
            ((world.y + size / 2.0) / size).floor() as i32,
        )
    }

    /// Viewport position → world pixels (`Camera.unproject`; the exact cursor
    /// point `InputHandler.mouseWorldX/Y` hands to `Units.closestOverlap`).
    #[func]
    pub fn screen_to_world(&self, x: f64, y: f64) -> Vector2 {
        let canvas = self.base().get_canvas_transform();
        canvas.affine_inverse() * Vector2::new(x as f32, y as f32)
    }

    /// Tile `(x, y)` → its **center** in viewport coordinates.
    ///
    /// This is the exact point to feed `godot_input mouse_button` with
    /// `coords: "viewport"` (see `§7c` step 6): `(x, y) * TILESIZE` (Java
    /// `Tile.worldx()`) transformed by the camera's canvas transform.
    #[func]
    pub fn tile_to_screen(&self, x: i32, y: i32) -> Vector2 {
        let world = Vector2::new(x as f32 * TILESIZE as f32, y as f32 * TILESIZE as f32);
        self.base().get_canvas_transform() * world
    }

    /// World pixels → viewport position (`tile_to_screen` without the `*8`
    /// tile-centre convention; used by the block-inventory popup, G03 T13).
    #[func]
    pub fn world_to_screen(&self, wx: f64, wy: f64) -> Vector2 {
        self.base().get_canvas_transform() * Vector2::new(wx as f32, wy as f32)
    }

    /// Centers the camera on tile `(x, y)` (`x * TILESIZE` center, Java
    /// `Tile.worldx()`).
    #[func]
    pub fn center_on_tile(&mut self, x: i32, y: i32) {
        self.camera.center_on_tile(x, y);
        self.follow_target = None;
        let (px, py) = self.camera.position;
        self.base_mut().set_position(Vector2::new(px, py));
        self.shadow_position = (px, py);
    }

    /// `InputHandler.panCamera`: force the camera center (world pixels).
    #[func]
    pub fn pan_to(&mut self, x: f64, y: f64) {
        self.camera.pan_camera(x as f32, y as f32);
        self.follow_target = None;
        self.base_mut()
            .set_position(Vector2::new(x as f32, y as f32));
        self.shadow_position = (x as f32, y as f32);
    }

    /// `Binding.respawn`/detach toggle: snap back to and follow the best core
    /// (`DesktopInput.update` player/core follow).
    #[func]
    pub fn recenter_player(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some((x, y)) = host.bind().best_core_position_px() else {
            return;
        };
        self.camera.pan_camera(x, y);
        self.follow_target = Some((x, y));
        let (px, py) = self.camera.render_position();
        self.base_mut().set_position(Vector2::new(px, py));
        self.shadow_position = (px, py);
    }

    /// Enables/disables the smooth-follow lock (`settings` `detach-camera`).
    #[func]
    pub fn set_follow(&mut self, follow: bool) {
        if follow {
            self.recenter_player();
        } else {
            self.follow_target = None;
        }
    }

    /// Whether the camera is currently locked to a follow target.
    #[func]
    pub fn is_following(&self) -> bool {
        self.follow_target.is_some()
    }

    /// Viewport size in pixels (selection-rect math / probes).
    #[func]
    pub fn viewport_size(&self) -> Vector2 {
        Vector2::new(self.camera.viewport.0, self.camera.viewport.1)
    }

    /// `Renderer.scaleCamera(amount)` (relative zoom-by).
    #[func]
    pub fn zoom_by(&mut self, amount: f64) {
        self.camera.scale_camera(amount as f32);
    }

    /// `MobileInput.pan`: viewport-pixel drag delta (touch gesture).
    #[func]
    pub fn mobile_pan(&mut self, delta_x: f64, delta_y: f64) {
        self.camera.mobile_pan(delta_x as f32, delta_y as f32);
        self.follow_target = None;
    }

    /// `Renderer.setScale` (absolute target scale).
    #[func]
    pub fn set_camera_scale(&mut self, scale: f64) {
        self.camera.set_scale(scale as f32);
    }

    /// `Minimap` right-click: center on the mapped minimap fraction.
    #[func]
    pub fn minimap_pan(&mut self, fraction_x: f64, fraction_y: f64) {
        self.camera
            .minimap_pan(fraction_x as f32, fraction_y as f32, MinimapRegion::FULL);
        self.follow_target = None;
        let (px, py) = self.camera.position;
        self.base_mut().set_position(Vector2::new(px, py));
    }

    /// `MinimapRenderer.zoomBy(amounty)`.
    #[func]
    pub fn minimap_zoom(&mut self, amount: f64) {
        self.camera.scale_camera(amount as f32);
    }

    /// Enables world-edge position clamping (`world.unitWidth/Height` px).
    #[func]
    pub fn set_world_size(&mut self, width: f64, height: f64) {
        self.camera.world_size = Some((width as f32, height as f32));
    }

    /// Pushes the `screenshake` setting (`0..=8`, `sliderPref("screenshake",
    /// 4, 0, 8)`). The next frame re-reads the persisted setting.
    #[func]
    pub fn set_screenshake(&mut self, value: i64) {
        self.screenshake = (value as i32).clamp(0, MAX_SCREENSHAKE);
    }

    /// Consumes a plan-17 `ShakeEvent { intensity, duration }`.
    #[func]
    pub fn apply_shake_event(&mut self, intensity: f64, duration: f64) {
        self.camera.on_fx_event(&FxEvent::Shake {
            intensity: intensity as f32,
            duration: duration as f32,
        });
    }

    /// Locks the camera to a unit/position (`InputHandler.spectate`).
    #[func]
    pub fn spectate(&mut self, x: f64, y: f64) {
        self.camera.spectate(x as f32, y as f32);
        self.follow_target = None;
        self.base_mut()
            .set_position(Vector2::new(x as f32, y as f32));
        self.shadow_position = (x as f32, y as f32);
    }

    /// `logicCutscene` pan+zoom (`zoom` `0..=1`, `-1` keeps the gameplay target).
    #[func]
    pub fn set_logic_cutscene(&mut self, pan_x: f64, pan_y: f64, zoom: f64, speed: f64) {
        self.camera
            .set_logic_cutscene((pan_x as f32, pan_y as f32), zoom as f32, speed as f32);
    }

    /// Ends `logicCutscene`.
    #[func]
    pub fn clear_logic_cutscene(&mut self) {
        self.camera.clear_logic_cutscene();
    }

    /// The camera rig state as JSON (MCP `§7c` camera probes).
    #[func]
    pub fn camera_state_json(&self) -> GString {
        let value = serde_json::json!({
            "format": 1,
            "position": [self.camera.position.0, self.camera.position.1],
            "render_position": [
                self.camera.render_position().0,
                self.camera.render_position().1,
            ],
            "target_scale": self.camera.target_scale,
            "camerascale": self.camera.camerascale,
            "width": self.camera.camera_width(),
            "height": self.camera.camera_height(),
            "shake_offset": [self.camera.shake.offset.0, self.camera.shake.offset.1],
            "shake_intensity": self.camera.shake.intensity,
            "logic_cutscene": self.camera.logic_cutscene,
            "min_scale": self.camera.min_scale(),
            "max_scale": self.camera.max_scale(),
        });
        GString::from(&value.to_string())
    }
}
