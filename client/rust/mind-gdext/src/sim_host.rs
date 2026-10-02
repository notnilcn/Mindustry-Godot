// SPDX-License-Identifier: GPL-3.0-only

//! `MindSimHost` — owns the `mind-core::Sim`, pumps it at a fixed 60 Hz and
//! exposes the MCP-visible test API (`00_FOUNDATION_IMPLEMENTATION_PLAN.md`
//! §3.5/§7c). Contains no game rules: every mutation goes through `mind_core`.
//!
//! Ported from `core/src/mindustry/ClientLauncher.java` (boot order) and
//! `core/src/mindustry/core/Logic.java` (fixed step pump); the fixed-step
//! accumulator itself lives in `mind_core::sim::FixedStepRunner` (D8).

use godot::classes::{INode, InputEvent, InputEventMouseButton, Os, ProjectSettings};
use godot::global::MouseButton;
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use crate::camera::MindCamera2D;
use crate::settings;
use mind_core::command::Command;
use mind_core::content::{
    BlockId, ContentRegistry, ContentType, MemoryBundle, MemoryUnlockStore, content_counts,
    create_base_content,
};
use mind_core::scenario::{Scenario, ScenarioPlayer};
use mind_core::sim::{FixedStepRunner, Sim};

/// Maximum ticks accepted by one `step()` call (defensive; UI/MCP only).
const MAX_STEP_TICKS: i64 = 1_000_000;

/// Frames to wait before reading the viewport (the texture is one frame late).
const CAPTURE_WARMUP_FRAMES: u32 = 3;

/// One scripted `-- --capture <path>` request.
struct CaptureRequest {
    path: String,
    frames_waited: u32,
}

/// Sim owner, fixed-step pump and input/API surface of the spine.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindSimHost {
    base: Base<Node>,
    sim: Sim,
    player: Option<ScenarioPlayer>,
    runner: FixedStepRunner,
    capture: Option<CaptureRequest>,
    /// Set whenever the world changed since the last `world_changed` emission.
    world_dirty: bool,
    /// Read-only content registry snapshot for the inspector `Content` tab
    /// (plan 02 §3.7). Plan 05 makes the sim own the `Content` resource; this
    /// view-side copy is dropped when that lands.
    content_snapshot: Option<ContentRegistry>,
}

#[godot_api]
impl INode for MindSimHost {
    fn init(base: Base<Node>) -> Self {
        // Default spine world: 32x32 flat, seed 1, `stone-wall` selected
        // (matches `scenarios/spine_place_break.json` aside from commands).
        let sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
        let runner =
            FixedStepRunner::for_rate(sim.config().fixed_hz, sim.config().max_ticks_per_frame);
        Self {
            base,
            sim,
            player: None,
            runner,
            capture: None,
            world_dirty: false,
            content_snapshot: None,
        }
    }

    fn ready(&mut self) {
        self.content_snapshot =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .and_then(|mut registry| {
                    registry.init()?;
                    registry.post_init()?;
                    Ok(registry)
                }) {
                Ok(registry) => Some(registry),
                Err(error) => {
                    log::warn!("content snapshot unavailable: {error}");
                    None
                }
            };

        if let Some(saved) = settings::read()
            && let Some(name) = saved.selected_block
            && let Ok(id) = self.sim.content().id(&name)
        {
            let _ = self.sim.set_selected_block(id);
        }

        self.capture = parse_capture_args();
        // The STDB connection is owned by the `StdbConnector` autoload (plan 01
        // M5, exactly one pump per process); the sim host no longer connects.
        self.world_dirty = true;
        self.emit_state();
        self.emit_world_changed();

        log::info!(
            "MindSimHost ready ({}x{} seed {} selected `{}`, mind-core {})",
            self.sim.grid.width(),
            self.sim.grid.height(),
            self.sim.seed(),
            self.sim.block_name_of(self.sim.selected_block()),
            mind_core::MIND_VERSION
        );
    }

    fn process(&mut self, delta: f64) {
        if self.capture.is_some() {
            self.process_capture();
            return;
        }
        if self.sim.is_paused() {
            return;
        }
        let steps = self.runner.advance(delta);
        if steps == 0 {
            return;
        }
        self.advance_steps(steps);
        self.emit_state();
        self.emit_world_changed();
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        let Ok(mouse) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !mouse.is_pressed() {
            return;
        }
        let button = mouse.get_button_index();
        if button != MouseButton::LEFT && button != MouseButton::RIGHT {
            return;
        }
        let position = mouse.get_position();

        let Some(camera) = self
            .base()
            .try_get_node_as::<MindCamera2D>("../World/Camera2D")
        else {
            log::warn!("MindSimHost input ignored: no MindCamera2D at ../World/Camera2D");
            return;
        };
        let tile = camera
            .bind()
            .screen_to_tile(position.x as f64, position.y as f64);
        let (Ok(x), Ok(y)) = (i16::try_from(tile.x), i16::try_from(tile.y)) else {
            return;
        };

        let applied = if button == MouseButton::LEFT {
            let block = self.sim.selected_block();
            self.apply_command(Command::Place { x, y, block })
        } else {
            self.apply_command(Command::Break { x, y })
        };
        if applied {
            log::debug!("mouse tile ({x}, {y}) applied ({button:?})");
            self.emit_state();
            self.emit_world_changed();
        }
    }

    fn exit_tree(&mut self) {
        // Minimal P0 client settings (§6.5); plan 04 replaces this with Settings.
        let zoom = self
            .base()
            .try_get_node_as::<MindCamera2D>("../World/Camera2D")
            .map(|camera| camera.bind().zoom_value())
            .unwrap_or(1.0);
        let selected = self.sim.block_name_of(self.sim.selected_block());
        if !settings::write(&selected, zoom) {
            log::warn!("failed to write user://settings.json");
        }
    }
}

#[godot_api]
impl MindSimHost {
    /// Emitted after every tick/API mutation: `(tick, checksum)`.
    #[signal]
    fn state_changed(tick: i64, checksum: GString);

    /// Emitted when place/break/load changed the tile grid (view redraw hint).
    #[signal]
    fn world_changed();

    /// Places a block at `(x, y)` immediately (API path; MCP §7c step 5).
    #[func]
    pub fn place_block(&mut self, x: i32, y: i32, block: GString) -> bool {
        let name = block.to_string();
        let id = match self.sim.content().id(&name) {
            Ok(id) => id,
            Err(_) => {
                log::warn!("place_block: unknown block `{name}`");
                return false;
            }
        };
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            log::warn!("place_block: ({x}, {y}) does not fit in i16");
            return false;
        };
        let applied = self.apply_command(Command::Place { x, y, block: id });
        if applied {
            self.emit_state();
            self.emit_world_changed();
        }
        applied
    }

    /// Breaks the block at `(x, y)` immediately (API path; MCP §7c step 5).
    #[func]
    pub fn break_block(&mut self, x: i32, y: i32) -> bool {
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            log::warn!("break_block: ({x}, {y}) does not fit in i16");
            return false;
        };
        let applied = self.apply_command(Command::Break { x, y });
        if applied {
            self.emit_state();
            self.emit_world_changed();
        }
        applied
    }

    /// Canonical sparse state dump as pretty JSON (same schema as `mind-headless`).
    #[func]
    pub fn get_state_json(&self) -> GString {
        match self.sim.dump_json(false) {
            Ok(json) => GString::from(json.as_str()),
            Err(err) => {
                log::error!("get_state_json: dump failed: {err}");
                GString::from("{}")
            }
        }
    }

    /// Current checksum as 16 lowercase hex digits.
    #[func]
    pub fn get_checksum(&self) -> GString {
        GString::from(self.sim.checksum_hex().as_str())
    }

    /// Per-type content counts for the inspector `Content` tab (plan 02 §3.7).
    /// Read-only; plan 05 moves this to the sim-owned `Content` resource.
    #[func]
    pub fn content_counts(&self) -> Dictionary<GString, i64> {
        let mut out = Dictionary::<GString, i64>::new();
        let Some(registry) = self.content_snapshot.as_ref() else {
            return out;
        };
        for (type_, count) in content_counts(registry) {
            out.set(&GString::from(type_.name()), count as i64);
        }
        out
    }

    /// Ordered content names of `type_name` (`content_list("item")`, plan 02
    /// §3.7). Unknown types return an empty array; read-only.
    #[func]
    pub fn content_list(&self, type_name: GString) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        let Some(registry) = self.content_snapshot.as_ref() else {
            return out;
        };
        let requested = type_name.to_string();
        let Some(type_) = ContentType::ALL
            .iter()
            .copied()
            .find(|type_| type_.name() == requested)
        else {
            return out;
        };
        for entry in registry.entries(type_) {
            if let Some(name) = entry.name {
                out.push(&GString::from(name));
            }
        }
        out
    }

    /// Completed sim ticks.
    #[func]
    pub fn get_tick(&self) -> i64 {
        self.sim.tick_count() as i64
    }

    /// Monotonic update counter (`GameState.updateId`).
    #[func]
    pub fn get_update_id(&self) -> i64 {
        self.sim.update_id() as i64
    }

    /// Phase name (`menu` / `playing` / `paused`).
    #[func]
    pub fn get_state(&self) -> GString {
        GString::from(self.sim.state_name())
    }

    /// Live per-group entity counts for the inspector (plan 05 M9).
    #[func]
    pub fn get_group_counts(&self) -> Dictionary<GString, i64> {
        let mut out = Dictionary::<GString, i64>::new();
        for (name, count) in self.sim.group_counts() {
            out.set(&GString::from(name), count as i64);
        }
        out
    }

    /// One group count by name (`0` when the group is unknown).
    #[func]
    pub fn get_group_count(&self, name: GString) -> i64 {
        let requested = name.to_string();
        self.sim
            .group_counts()
            .get(requested.as_str())
            .copied()
            .unwrap_or(0) as i64
    }

    /// Whether the fixed-step pump is halted.
    #[func]
    pub fn is_paused(&self) -> bool {
        self.sim.is_paused()
    }

    // ---- IoSet seam (plan 05 M9 / plan 04 §3.10; MindIo wiring placeholder) ----
    //
    // The orchestrator wires the plan-04 `MindIo` autoload to these funcs and
    // satisfies requests at the tick boundary. Copy-pasteable MCP evals (do NOT
    // launch the editor from this lane; the single-editor mutex is orchestrator-
    // owned):
    //
    //   godot_exec eval: "var h=Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost'); print('MCP_TICK=',h.get_tick(),' STATE=',h.get_state(),' UP=',h.get_update_id())"
    //   godot_exec eval: "print('MCP_GROUPS=',Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').get_group_counts())"
    //   godot_exec eval: "Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').request_save('user://mcp.msav', false); print('MCP_PENDING=',Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').io_pending())"
    //   godot_exec eval: "print('MCP_REQ=',Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').take_io_requests_json())"

    /// Queues a save at the next `IoSet::Capture` boundary.
    #[func]
    pub fn request_save(&mut self, path: GString, as_map: bool) {
        self.sim.request_save(path.to_string(), as_map);
    }

    /// Queues a load at the next `IoSet::Apply` boundary.
    #[func]
    pub fn request_load(&mut self, path: GString) {
        self.sim.request_load(path.to_string());
    }

    /// Number of queued IO requests.
    #[func]
    pub fn io_pending(&self) -> i64 {
        self.sim.io_pending() as i64
    }

    /// Drains queued IO requests as JSON (`[{kind,path,as_map?}]`) for `MindIo`.
    #[func]
    pub fn take_io_requests_json(&mut self) -> GString {
        let value: Vec<serde_json::Value> = self
            .sim
            .take_io_requests()
            .iter()
            .map(|request| match request {
                mind_core::sim::IoRequest::Save { path, as_map } => serde_json::json!({
                    "kind": "save",
                    "path": path.display().to_string(),
                    "as_map": as_map,
                }),
                mind_core::sim::IoRequest::Load { path } => serde_json::json!({
                    "kind": "load",
                    "path": path.display().to_string(),
                }),
            })
            .collect();
        GString::from(
            serde_json::to_string(&value)
                .unwrap_or_else(|_| String::from("[]"))
                .as_str(),
        )
    }

    /// Delivers a `MindIo` result as JSON (`{kind,path,as_map?,error?}`).
    #[func]
    pub fn deliver_io_response_json(&mut self, json: GString) -> bool {
        let text = json.to_string();
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            log::warn!("deliver_io_response_json: invalid JSON");
            return false;
        };
        let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let path = value.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            log::warn!("deliver_io_response_json: missing path");
            return false;
        }
        let request = match kind {
            "save" => mind_core::sim::IoRequest::Save {
                path: path.into(),
                as_map: value
                    .get("as_map")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            },
            "load" => mind_core::sim::IoRequest::Load { path: path.into() },
            other => {
                log::warn!("deliver_io_response_json: unknown kind `{other}`");
                return false;
            }
        };
        let status = match value.get("error").and_then(|v| v.as_str()) {
            Some(error) => mind_core::sim::IoStatus::Failed(error.to_owned()),
            None => mind_core::sim::IoStatus::Ok,
        };
        self.sim
            .deliver_io_response(mind_core::sim::IoResponse { request, status });
        true
    }

    /// Pauses/resumes the fixed-step pump (resets the accumulator).
    #[func]
    pub fn set_paused(&mut self, paused: bool) {
        self.sim.set_paused(paused);
        self.runner = FixedStepRunner::for_rate(
            self.sim.config().fixed_hz,
            self.sim.config().max_ticks_per_frame,
        );
        self.emit_state();
    }

    /// Runs `ticks` sim steps synchronously and returns the new tick count.
    #[func]
    pub fn step(&mut self, ticks: i64) -> i64 {
        let count = ticks.clamp(0, MAX_STEP_TICKS) as u32;
        if count > 0 {
            self.advance_steps(count);
            self.emit_state();
            self.emit_world_changed();
        }
        self.get_tick()
    }

    /// Loads a scenario (path may be `res://`, `user://` or native) and resets
    /// the sim; returns `false` on read/parse/apply failure.
    #[func]
    pub fn load_scenario(&mut self, path: GString) -> bool {
        let requested = path.to_string();
        let native = ProjectSettings::singleton().globalize_path(&requested);
        let scenario = match Scenario::read(native.to_string()) {
            Ok(scenario) => scenario,
            Err(err) => {
                log::error!("load_scenario `{requested}` failed: {err}");
                return false;
            }
        };
        let sim = match Sim::from_scenario(&scenario) {
            Ok(sim) => sim,
            Err(err) => {
                log::error!("load_scenario `{requested}`: sim build failed: {err}");
                return false;
            }
        };
        let player = match ScenarioPlayer::new(&scenario, sim.content()) {
            Ok(player) => player,
            Err(err) => {
                log::error!("load_scenario `{requested}`: command resolve failed: {err}");
                return false;
            }
        };

        log::info!(
            "loaded scenario `{}` ({} steps, seed {}) from `{requested}`",
            scenario.name,
            player.total_steps(),
            scenario.seed
        );
        self.runner =
            FixedStepRunner::for_rate(sim.config().fixed_hz, sim.config().max_ticks_per_frame);
        self.sim = sim;
        self.player = Some(player);
        self.world_dirty = true;
        self.emit_state();
        self.emit_world_changed();
        true
    }

    /// Name of the currently selected block.
    #[func]
    pub fn selected_block(&self) -> GString {
        GString::from(self.sim.block_name_of(self.sim.selected_block()).as_str())
    }

    /// Selects a block by name; `false` when the name is unknown.
    #[func]
    pub fn select_block(&mut self, name: GString) -> bool {
        let name = name.to_string();
        match self.sim.content().id(&name) {
            Ok(id) => match self.sim.set_selected_block(id) {
                Ok(()) => {
                    self.emit_state();
                    true
                }
                Err(err) => {
                    log::warn!("select_block `{name}` failed: {err}");
                    false
                }
            },
            Err(_) => {
                log::warn!("select_block: unknown block `{name}`");
                false
            }
        }
    }

    /// Writes a PNG of the running viewport to `path` (native or `user://`).
    #[func]
    pub fn capture(&mut self, path: GString) -> bool {
        let requested = path.to_string();
        match self.capture_to(&requested) {
            true => {
                log::info!("capture written to `{requested}`");
                true
            }
            false => {
                log::error!("capture to `{requested}` failed (no viewport image?)");
                false
            }
        }
    }

    /// Tile rows for the view: `(x, y, block_id)` for every non-air tile.
    ///
    /// Plain accessor (not `#[func]`) consumed by `MindTileGrid::draw`.
    pub fn tile_blocks(&self) -> Vec<(i16, i16, u16)> {
        self.sim
            .grid
            .iter_row_major()
            .filter_map(|(pos, index)| {
                let block = self.sim.grid.block_id_at(index);
                (block != BlockId::AIR).then_some((pos.x(), pos.y(), block.get()))
            })
            .collect()
    }

    /// World size in tiles `(width, height)`; used to draw the grid border.
    pub fn world_size(&self) -> (i32, i32) {
        (self.sim.grid.width(), self.sim.grid.height())
    }

    /// Applies one immediate command; `false` when `mind-core` rejects it.
    fn apply_command(&mut self, command: Command) -> bool {
        match self.sim.apply(command) {
            Ok(()) => {
                self.world_dirty = true;
                true
            }
            Err(err) => {
                log::warn!("command rejected: {err}");
                false
            }
        }
    }

    /// Runs `steps` fixed steps, applying scenario commands before each tick.
    fn advance_steps(&mut self, steps: u32) {
        for _ in 0..steps {
            let applied_before = self.sim.commands_applied();
            let result = if let Some(player) = self.player.as_mut() {
                player.step(&mut self.sim)
            } else {
                self.sim.tick().map(|()| true)
            };
            match result {
                // A finished scenario keeps ticking without further commands.
                Ok(false) => {
                    if let Err(err) = self.sim.tick() {
                        log::error!("sim tick failed after scenario end: {err}");
                        break;
                    }
                }
                Ok(true) => {}
                Err(err) => {
                    log::error!("sim tick failed: {err}");
                    break;
                }
            }
            if self.sim.commands_applied() != applied_before {
                self.world_dirty = true;
            }
        }
    }

    fn emit_state(&mut self) {
        let tick = self.sim.tick_count() as i64;
        let checksum = GString::from(self.sim.checksum_hex().as_str());
        let _ = self
            .base_mut()
            .emit_signal("state_changed", &[tick.to_variant(), checksum.to_variant()]);
    }

    fn emit_world_changed(&mut self) {
        if self.world_dirty {
            self.world_dirty = false;
            let _ = self.base_mut().emit_signal("world_changed", &[]);
        }
    }

    /// Captures the viewport texture to a PNG.
    ///
    /// Uses `get_viewport().get_texture().get_image()` — the canonical Godot 4
    /// path (works in windowed and embedded runs; a dummy/headless renderer has
    /// no readable image, which is reported as `false`).
    fn capture_to(&self, path: &str) -> bool {
        let Some(viewport) = self.base().get_viewport() else {
            return false;
        };
        let Some(texture) = viewport.get_texture() else {
            return false;
        };
        let Some(image) = texture.get_image() else {
            return false;
        };
        if image.is_empty() {
            return false;
        }
        let native = ProjectSettings::singleton().globalize_path(path);
        image.save_png(&native) == godot::global::Error::OK
    }

    /// Processes a pending `-- --capture <path>` request (quit code 0 on success).
    fn process_capture(&mut self) {
        let Some(mut request) = self.capture.take() else {
            return;
        };
        request.frames_waited += 1;
        if request.frames_waited < CAPTURE_WARMUP_FRAMES {
            self.capture = Some(request);
            return;
        }

        let ok = self.capture_to(&request.path);
        if ok {
            log::info!("capture written to `{}`", request.path);
        } else {
            log::error!(
                "capture to `{}` failed (headless/dummy renderer has no viewport image)",
                request.path
            );
        }
        if let Some(mut tree) = self.base().get_tree_or_null() {
            tree.quit_ex().exit_code(i32::from(!ok)).done();
        }
    }
}

/// Parses `--capture <path>` / `--capture=<path>` from the user args (after `--`).
fn parse_capture_args() -> Option<CaptureRequest> {
    let args = Os::singleton().get_cmdline_user_args();
    let slice = args.as_slice();
    let mut index = 0;
    while index < slice.len() {
        let arg = slice[index].to_string();
        if arg == "--capture" {
            let path = slice.get(index + 1)?.to_string();
            return Some(CaptureRequest {
                path,
                frames_waited: 0,
            });
        }
        if let Some(path) = arg.strip_prefix("--capture=") {
            return Some(CaptureRequest {
                path: path.to_owned(),
                frames_waited: 0,
            });
        }
        index += 1;
    }
    None
}
