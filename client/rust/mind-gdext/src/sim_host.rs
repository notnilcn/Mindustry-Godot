// SPDX-License-Identifier: GPL-3.0-only

//! `MindSimHost` — owns the `mind-core::Sim`, pumps it at a fixed 60 Hz and
//! exposes the MCP-visible test API (`00_FOUNDATION_IMPLEMENTATION_PLAN.md`
//! §3.5/§7c). Contains no game rules: every mutation goes through `mind_core`.
//!
//! Ported from `core/src/mindustry/ClientLauncher.java` (boot order) and
//! `core/src/mindustry/core/Logic.java` (fixed step pump); the fixed-step
//! accumulator itself lives in `mind_core::sim::FixedStepRunner` (D8).

use std::collections::VecDeque;

use godot::classes::notify::NodeNotification;
use godot::classes::{FileAccess, INode, InputEvent, InputEventMouseButton, Os, ProjectSettings};
use godot::global::MouseButton;
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use crate::camera::MindCamera2D;
use crate::settings;
use mind_core::audio::{AudioSinkRes, SharedAudioLog};
use mind_core::command::Command;
use mind_core::content::{
    BlockId, BlockKind, ContentRegistry, ContentType, ItemId, MemoryBundle, MemoryUnlockStore,
    content_counts, create_base_content,
};
use mind_core::determinism::SimCommand;
use mind_core::editor::context::EditorContext;
use mind_core::game::rules::Rules;
use mind_core::game::runtime::{CampaignRuntime, SessionSync, sync_session_with_sim};
use mind_core::io::save::{SaveIo, SaveReadState};
use mind_core::scenario::{Scenario, ScenarioPlayer};
use mind_core::sim::{FixedStepRunner, Sim};
use mind_core::world::WorldGrid;
use mind_core::world::blocks::heat::HeatState;
use mind_core::world::blocks::power::PowerGrids;
use mind_core::world::modules::{ItemModule, LiquidModule, PowerModule};

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
    /// Monotonic world-mutation counter (incremented by `emit_world_changed`).
    /// The plan-16 renderer uses it to invalidate chunk meshes without wiring
    /// raw tile coords across the view boundary (plan 16 §3.5).
    revision: u64,
    /// Monotonic world-load counter (`load_scenario`/`load_sector`), used by the
    /// camera rig to snap on every world load, including same-size reloads.
    world_loads: u64,
    /// Plan-17 `MindFx` sibling (`../MindFx`); the one-way sim→view tick hook.
    fx_host: Option<Gd<Node>>,
    /// Plan-21 relay queue: commands drained and applied at the next tick
    /// start (never mid-frame). Populated by `MindNet`/MCP via
    /// `enqueue_sim_command`/`apply_sim_command_json`.
    pending_commands: VecDeque<SimCommand>,
    /// Plan-18 sim→client audio log; installed as an `AudioSinkRes` resource
    /// and drained by `/root/Spine/MindAudio` (plan 18 §3.3/§3.10).
    audio_log: SharedAudioLog,
    /// Rules resolved by the last `load_sector` (preset map/generator/save).
    sector_rules: Option<Rules>,
    /// Enemy spawn-overlay count of the last loaded sector grid.
    sector_spawns: i32,
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
            revision: 0,
            world_loads: 0,
            fx_host: None,
            pending_commands: VecDeque::new(),
            audio_log: SharedAudioLog::new(),
            sector_rules: None,
            sector_spawns: 0,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
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

    /// Applies world place/break only for clicks the GUI did not consume.
    ///
    /// Godot dispatches `Control` input (`_gui_input`/`accept_event`, and every
    /// `mouse_filter` STOP hit) before `_unhandled_input`, so a click on a
    /// placement tab, block button, HUD or dialog can never reach the world.
    /// Upstream parity: `DesktopInput.update` gates world clicks on
    /// `!Core.scene.hasMouse()` (`core/src/mindustry/input/DesktopInput.java`).
    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
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
        let selected = self.sim.block_name_of(self.sim.selected_block());
        if !settings::write(&selected) {
            log::warn!("failed to write user://settings.json");
        }
    }
}

#[godot_api]
impl MindSimHost {
    /// Rebuilds Godot-derived boot state from the (possibly reloaded) node.
    ///
    /// Runs from `ready()` and again on `EXTENSION_RELOADED`; every step is
    /// idempotent (resource inserts overwrite, `set_io_handler` replaces) and
    /// the sim `init` already provided a fresh world.
    fn bootstrap(&mut self) {
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

        // Plan-13 M7 + plan 04 §3.10 + plan-18: wire the per-sim host seams
        // (logic globals, IO executor, audio sink). Must be re-run whenever a
        // fresh `Sim` replaces `self.sim`, or queued IO never drains.
        self.install_sim_seams();

        // Live gameplay (the default spine world, custom maps and campaign
        // sectors) runs the plan-07 block stack so placed buildings execute their
        // behaviors. Scenario loads keep the frozen P0 path on purpose: the
        // committed checksum goldens are recorded against `Sim::from_scenario`.
        self.install_live_block_runtime();

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

        // Plan-17 sim→view hook: the FX host advances one fixed view tick per
        // sim tick (one-way; the sim never reads the view). Optional: absent in
        // headless/no-FX scenes.
        self.fx_host = self.base().try_get_node_as::<Node>("../MindFx");

        log::info!(
            "MindSimHost ready ({}x{} seed {} selected `{}`, mind-core {})",
            self.sim.grid.width(),
            self.sim.grid.height(),
            self.sim.seed(),
            self.sim.block_name_of(self.sim.selected_block()),
            mind_core::MIND_VERSION
        );
    }

    /// Emitted after every tick/API mutation: `(tick, checksum)`.
    #[signal]
    fn state_changed(tick: i64, checksum: GString);

    /// Emitted when place/break/load changed the tile grid (view redraw hint).
    #[signal]
    fn world_changed();

    /// Wires the host seams that live on the `Sim` value: the content-initialized
    /// logic globals (plan-13 M7), the live-sim IO executor (plan 04 §3.10) and
    /// the audio sink (plan-18).
    ///
    /// `load_scenario`/`load_sector` install a brand-new `Sim`, so this must run
    /// again after every replacement; otherwise `request_save`/`request_load`
    /// queue into an `IoQueue` with no handler and never drain.
    fn install_sim_seams(&mut self) {
        if let Some(registry) = self.content_snapshot.take() {
            mind_core::logic::globals::GlobalVars::install_world(&mut self.sim.ecs.0, &registry);
            self.content_snapshot = Some(registry);
        }

        // The handler boots its content lazily on the first IO call, so a
        // session that never saves pays nothing.
        self.sim
            .set_io_handler(Box::new(mind_core::io::SimIoHandler::new()));

        // Sim systems emit `AudioEvent`s unconditionally; `MindAudio` drains this
        // log once per frame.
        self.sim
            .ecs
            .0
            .insert_resource(AudioSinkRes::new(self.audio_log.clone()));
    }

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

    /// Plan-21 checksum-scope hook (relay scope; the mask is a no-op until plan
    /// 05 adds possessed-unit fields).
    #[func]
    pub fn get_checksum_scoped(&self) -> GString {
        GString::from(format!("{:016x}", self.checksum_scoped_value()).as_str())
    }

    /// Plan-21 scoped checksum value (Rust-only seam for relay publication).
    pub fn checksum_scoped_value(&self) -> u64 {
        self.sim
            .checksum_scoped(&mind_core::sim::ChecksumScope::relay())
    }

    /// Number of relay commands queued for the next tick boundary.
    #[func]
    pub fn pending_command_count(&self) -> i64 {
        self.pending_commands.len() as i64
    }

    /// Enqueues a canonical [`SimCommand`] to apply at the next tick start
    /// (plan 21 §3.5). Rust-only seam used by the `MindNet` relay runtime.
    pub fn enqueue_sim_command(&mut self, command: SimCommand) {
        self.pending_commands.push_back(command);
    }

    /// MCP/dev helper: parse a small JSON command and enqueue it.
    ///
    /// Shape: `{"op":"place","x":1,"y":2,"block":<u16>,"rotation":0}` or
    /// `{"op":"break","x":1,"y":2}`.
    #[func]
    pub fn apply_sim_command_json(&mut self, cmd_json: GString) -> bool {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&cmd_json.to_string()) else {
            log::warn!("apply_sim_command_json: invalid JSON");
            return false;
        };
        let op = value.get("op").and_then(|v| v.as_str()).unwrap_or("");
        let x = value.get("x").and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
        let y = value.get("y").and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            log::warn!("apply_sim_command_json: missing/oversized x,y");
            return false;
        };
        let command = match op {
            "place" => {
                let block = value.get("block").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
                let rotation = value
                    .get("rotation")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0)
                    .clamp(0, 3) as i8;
                SimCommand::Place {
                    x,
                    y,
                    block,
                    rotation,
                    team: 0,
                    player: None,
                }
            }
            "break" => SimCommand::Break { x, y, player: None },
            other => {
                log::warn!("apply_sim_command_json: unsupported op `{other}`");
                return false;
            }
        };
        self.pending_commands.push_back(command);
        true
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

    /// Monotonic world-mutation counter (renderer chunk invalidation).
    #[func]
    pub fn world_revision(&self) -> i64 {
        self.revision as i64
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

    // ---- Plan 09 networks debug API (data side; §7c MCP run deferred) ----

    /// Whole-network projection as pretty JSON (plan 09 §3.11).
    #[func]
    pub fn network_state(&self) -> GString {
        let tick = self.sim.tick_count();
        let state = mind_core::world::NetworkState::capture(&self.sim.ecs.0, tick);
        GString::from(state.to_json().as_str())
    }

    /// Live network counts `{graphs, liquids, heat_buildings}` (plan 09 §3.12).
    #[func]
    pub fn network_counts(&self) -> Dictionary<GString, i64> {
        let world = &self.sim.ecs.0;
        let graphs = world
            .get_resource::<PowerGrids>()
            .map(|grids| grids.graph_count())
            .unwrap_or(0);
        let mut liquids = 0i64;
        let mut heat = 0i64;
        for entity in world.iter_entities() {
            if entity.get::<LiquidModule>().is_some() {
                liquids += 1;
            }
            if entity.get::<HeatState>().is_some() {
                heat += 1;
            }
        }
        let mut out = Dictionary::<GString, i64>::new();
        out.set(&GString::from("graphs"), graphs as i64);
        out.set(&GString::from("liquids"), liquids);
        out.set(&GString::from("heat_buildings"), heat);
        out
    }

    /// Power network values at `(x, y)` (`{graph_id, status, stored, init}`).
    #[func]
    pub fn power_debug(&self, x: i32, y: i32) -> Dictionary<GString, f64> {
        let mut out = Dictionary::<GString, f64>::new();
        let entity = self
            .sim
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.sim.grid.tile(x, y).build)
            .flatten();
        let Some(entity) = entity else {
            return out;
        };
        let world = &self.sim.ecs.0;
        if let Some(module) = world.get::<PowerModule>(entity) {
            out.set(&GString::from("status"), module.status as f64);
            out.set(&GString::from("stored"), module.stored as f64);
            out.set(&GString::from("init"), if module.init { 1.0 } else { 0.0 });
            if let Some(graph) = world
                .get_resource::<PowerGrids>()
                .and_then(|grids| grids.graph(module.graph))
            {
                out.set(&GString::from("graph_id"), graph.debug_id as f64);
                out.set(
                    &GString::from("satisfaction"),
                    graph.get_satisfaction() as f64,
                );
            }
        }
        out
    }

    /// Liquid values at `(x, y)` (`{current, has_any}`).
    #[func]
    pub fn liquid_debug(&self, x: i32, y: i32) -> Dictionary<GString, f64> {
        let mut out = Dictionary::<GString, f64>::new();
        let entity = self
            .sim
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.sim.grid.tile(x, y).build)
            .flatten();
        let Some(entity) = entity else {
            return out;
        };
        if let Some(module) = self.sim.ecs.0.get::<LiquidModule>(entity) {
            out.set(&GString::from("current"), module.current_amount as f64);
            out.set(
                &GString::from("has_any"),
                if module.has_any() { 1.0 } else { 0.0 },
            );
        }
        out
    }

    /// Heat values at `(x, y)` (`{heat, heat_output, frac}`).
    #[func]
    pub fn heat_debug(&self, x: i32, y: i32) -> Dictionary<GString, f64> {
        let mut out = Dictionary::<GString, f64>::new();
        let entity = self
            .sim
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.sim.grid.tile(x, y).build)
            .flatten();
        let Some(entity) = entity else {
            return out;
        };
        if let Some(heat) = self.sim.ecs.0.get::<HeatState>(entity) {
            out.set(&GString::from("heat"), heat.heat as f64);
            out.set(&GString::from("heat_output"), heat.heat_output as f64);
            out.set(
                &GString::from("frac"),
                mind_core::world::blocks::heat::heat_frac(heat.heat, heat.heat_output) as f64,
            );
        }
        out
    }

    // ---- Plan 13 M8 logic host surface (§3.12) ----
    //
    // The pure probes (`logic_run`, `logic_statement_names`, `logic_compile`)
    // work everywhere. The sim-integrated probes (`logic_place`,
    // `logic_set_code`, `logic_get_state`, `logic_display_commands`) return
    // their empty/false form until plan 05/07 expose live logic-building state
    // in `Sim` (the P0 `Sim` uses `BuildingComp`, not plan-07 behaviors).
    // MCP recipes (plan 13 §7c; editor run is orchestrator-owned via the
    // single-editor mutex):
    //
    //   godot_exec call: $"/root/Spine/MindLogic" statement_names()   # (after adding MindLogic to game.tscn)
    //   godot_exec call: $"/root/Spine/SimHost" logic_run("op add a a 1\njump 0 always", 10, 2, true)
    //   godot_exec call: $"/root/Spine/SimHost" logic_place("logic-processor", 40, 40)  # false until 05/07
    //   godot_exec call: $"/root/Spine/SimHost" logic_get_state(40, 40)

    /// Whether the sim-integrated logic probes can mutate a live processor.
    #[func]
    pub fn logic_available(&self) -> bool {
        false
    }

    /// Registered statement names (plan-14 editor handoff).
    #[func]
    pub fn logic_statement_names(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for meta in mind_core::logic::statement::all_statements() {
            out.push(&GString::from(meta.registered_name));
        }
        out
    }

    /// Runs `code` on a temporary executor for `ticks` at `ipt`; dumps vars.
    #[func]
    pub fn logic_run(
        &self,
        code: GString,
        ticks: i64,
        ipt: i64,
        privileged: bool,
    ) -> Dictionary<GString, Variant> {
        let ticks = ticks.clamp(0, 1_000_000) as u64;
        let ipt = ipt.clamp(1, 1000) as i32;
        let mut out = Dictionary::<GString, Variant>::new();
        let Some(exec) =
            mind_core::logic::script::run_standalone(&code.to_string(), privileged, ticks, ipt)
        else {
            out.set(&GString::from("ok"), &false.to_variant());
            out.set(
                &GString::from("error"),
                &GString::from("assemble failed").to_variant(),
            );
            return out;
        };
        let mut vars = Dictionary::<GString, Variant>::new();
        for cell in &exec.arena.cells {
            if cell.constant || cell.name.starts_with('@') {
                continue;
            }
            let value: Variant = if cell.is_obj {
                match &cell.obj {
                    None => Variant::nil(),
                    Some(value) => GString::from(value.display().as_str()).to_variant(),
                }
            } else {
                cell.num.to_variant()
            };
            vars.set(&GString::from(cell.name.as_str()), &value);
        }
        out.set(&GString::from("ok"), &true.to_variant());
        out.set(
            &GString::from("text"),
            &GString::from(exec.text_buffer.as_str()).to_variant(),
        );
        out.set(&GString::from("vars"), &vars.to_variant());
        out
    }

    /// Placeholder for placing a logic block (awaits plan 05/07 integration).
    #[func]
    pub fn logic_place(&self, kind: GString, _x: i64, _y: i64) -> bool {
        log::warn!("logic_place(`{kind}`): sim-side logic building state is a plan-05/07 hook");
        false
    }

    /// Placeholder for applying compressed logic code (awaits plan 05/07).
    #[func]
    pub fn logic_set_code(&self, _x: i64, _y: i64, _code: GString) -> bool {
        log::warn!("logic_set_code: sim-side logic building state is a plan-05/07 hook");
        false
    }

    /// Placeholder for reading a processor state (awaits plan 05/07).
    #[func]
    pub fn logic_get_state(&self, _x: i64, _y: i64) -> Dictionary<GString, Variant> {
        Dictionary::new()
    }

    /// Placeholder for a display command queue (awaits plan 16 rendering/05).
    #[func]
    pub fn logic_display_commands(&self, _x: i64, _y: i64) -> PackedInt64Array {
        PackedInt64Array::new()
    }

    /// Runs a registered logic bench case (plan 13 §7d; not yet registered).
    #[func]
    pub fn logic_bench(&self, name: GString) -> Dictionary<GString, Variant> {
        log::warn!("logic_bench(`{name}`): no bench registered yet");
        Dictionary::new()
    }

    /// Dev `setrule` probe (awaits the plan-12 `LogicWorldState` resource).
    #[func]
    pub fn logic_set_rule(&self, name: GString, _value: Variant) -> bool {
        log::warn!("logic_set_rule(`{name}`): timeline/sim rule state is a plan-12 hook");
        false
    }

    // ---- IoSet seam (plan 05 M9 / plan 04 §3.10; SimIoHandler wired in ready) ----
    //
    // `ready()` registers `mind_core::io::SimIoHandler`, so a queued save/load is
    // fulfilled at the tick boundary against the live grid (Godot `user://` /
    // `res://` paths are globalized to native paths for plan-04 `NativeFs`). The
    // plan-04 `MindIo` gdext autoload is a thin client-side wrapper; the in-engine
    // MCP round-trip itself remains orchestrator-owned (single-editor mutex).
    // Copy-pasteable MCP evals (do NOT launch the editor from this lane):
    //
    //   godot_exec eval: "var h=Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost'); print('MCP_TICK=',h.get_tick(),' STATE=',h.get_state(),' UP=',h.get_update_id())"
    //   godot_exec eval: "print('MCP_GROUPS=',Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').get_group_counts())"
    //   godot_exec eval: "Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').request_save('user://mcp.msav', false); print('MCP_PENDING=',Engine.get_main_loop().current_scene.get_node('/root/Spine/SimHost').io_pending())"

    /// Queues a save at the next `IoSet::Capture` boundary.
    #[func]
    pub fn request_save(&mut self, path: GString, as_map: bool) {
        self.sim.request_save(globalize(&path), as_map);
    }

    /// Queues a load at the next `IoSet::Apply` boundary.
    #[func]
    pub fn request_load(&mut self, path: GString) {
        self.sim.request_load(globalize(&path));
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
        self.install_sim_seams();
        self.world_dirty = true;
        self.world_loads += 1;
        self.emit_state();
        self.emit_world_changed();
        true
    }

    /// Generates and installs a campaign sector's world into the sim
    /// (`Control.playNewSector` world half), preferring an existing sector save
    /// (`user://saves/sector-<planet>-<id>.msav`, resume) over the preset map
    /// and the planet generator. The campaign play-flow state itself lives in
    /// `MindCampaign`; this only owns the tile grid the renderer draws.
    /// Returns `false` when the planet generator, map and save are all missing.
    #[func]
    pub fn load_sector(&mut self, planet: GString, sector: i32) -> bool {
        if sector < 0 {
            return false;
        }
        let planet_name = planet.to_string();
        // A new world invalidates the previous campaign runtime.
        self.sim.ecs.0.remove_resource::<CampaignRuntime>();
        // Take the registry out so the generator can borrow it while the sim is
        // rebuilt, then hand it back (content boot is expensive; never drop it).
        let Some(mut registry) = self.content_snapshot.take() else {
            log::warn!("load_sector: content registry unavailable");
            return false;
        };
        const SECTOR_SIZE: i32 = 128;
        // `Control.playSector`: an existing sector save is the resume source.
        let save_path = sector_save_path(&planet_name, sector as u16);
        let loaded = if std::path::Path::new(&save_path).is_file() {
            log::info!("load_sector: resuming `{planet_name}:{sector}` from `{save_path}`");
            load_grid_file(&save_path, &mut registry)
        } else {
            // `World.loadSector`: a preset sector uses its stored designed map
            // (`SectorPreset.generator` / `FileMapGenerator`); planet sectors
            // fall back to the planet generator.
            load_preset_map_grid(&planet_name, sector as u16, &mut registry)
        };
        let generated = if loaded.is_none() {
            mind_core::maps::planet::generate_sector(
                &planet_name,
                sector as u16,
                1,
                SECTOR_SIZE,
                SECTOR_SIZE,
                &registry,
            )
        } else {
            None
        };
        let (grid, rules, pending_buildings) = match loaded {
            Some(loaded) => (loaded.grid, loaded.rules, loaded.pending_buildings),
            None => match generated {
                Some(generated) => (generated.grid, Some(generated.rules), Vec::new()),
                None => {
                    self.content_snapshot = Some(registry);
                    log::warn!("load_sector: no map or generator for planet `{planet_name}`");
                    return false;
                }
            },
        };
        self.sector_spawns = count_spawns(&grid, &registry);
        self.sector_rules = rules;
        self.content_snapshot = Some(registry);

        // Reuse a fresh sim and swap in the loaded/generated grid; a new
        // iteration keeps no ECS entities from the previous world.
        let mut sim = Sim::new(1, SECTOR_SIZE, SECTOR_SIZE, BlockId::AIR, BlockId::AIR);
        sim.grid = grid;
        let _ = sim.set_phase(mind_core::game::State::Playing);
        self.runner =
            FixedStepRunner::for_rate(sim.config().fixed_hz, sim.config().max_ticks_per_frame);
        self.sim = sim;
        self.player = None;
        // A campaign sector is live play: its placed blocks must execute their
        // plan-07 behaviors (mining, transport, crafting, power). The runtime
        // resources are installed before any entity exists so ECS indices stay
        // stable.
        self.install_live_block_runtime();
        // `World.loadMap`'s building half: the reader decoded the map's tile
        // entities into the pending queue while the grid was built, so spawn
        // the ECS buildings (cores included) the campaign runtime/teams read.
        let materialized = self.sim.materialize_map_buildings(&pending_buildings);
        if materialized > 0 {
            log::info!("load_sector: materialized {materialized} map building(s)");
        }
        // The unit runtime snapshotted the grid before materialization; keep its
        // terrain/path tiles in sync with the live map.
        self.sim.refresh_unit_runtime();
        // The fresh `Sim` carries no IO executor; re-wire the seams so a queued
        // `request_save`/`request_load` drains at the next `IoSet` boundary.
        self.install_sim_seams();
        self.world_dirty = true;
        self.world_loads += 1;
        self.emit_state();
        self.emit_world_changed();
        log::info!("loaded campaign sector `{planet_name}:{sector}`");
        true
    }

    /// Rules resolved by the last `load_sector` (preset map, save or generator).
    /// Rust-only seam for `MindCampaign.start_sector`.
    pub fn last_sector_rules(&self) -> Option<&Rules> {
        self.sector_rules.as_ref()
    }

    /// Enemy spawn-overlay count of the last loaded sector grid.
    pub fn sector_spawn_count(&self) -> i32 {
        self.sector_spawns
    }

    /// Installs (replacing any previous) the live campaign runtime resource so
    /// the `TickSet::Campaign`/`Objectives`/`GameStateCheck` systems run.
    pub fn install_campaign_runtime(&mut self, runtime: CampaignRuntime) {
        self.sim.ecs.0.insert_resource(runtime);
    }

    /// Removes the campaign runtime (menu/abort/new world).
    pub fn clear_campaign_runtime(&mut self) {
        self.sim.ecs.0.remove_resource::<CampaignRuntime>();
    }

    /// Takes the runtime out for host-side bookkeeping.
    pub fn take_campaign_runtime(&mut self) -> Option<CampaignRuntime> {
        self.sim.ecs.0.remove_resource::<CampaignRuntime>()
    }

    /// Puts a bookkept runtime back.
    pub fn put_campaign_runtime(&mut self, runtime: CampaignRuntime) {
        self.sim.ecs.0.insert_resource(runtime);
    }

    /// Registers the live world's cores into the installed session and reports
    /// the spawn accounting (GAP-9). `apply_loadout` adds the launch loadout to
    /// the default team's core inventory (fresh launches only).
    pub fn sync_campaign_session(
        &mut self,
        content: &ContentRegistry,
        apply_loadout: bool,
    ) -> Option<SessionSync> {
        let mut runtime = self.take_campaign_runtime()?;
        let sync = sync_session_with_sim(
            &mut runtime.session,
            &mut self.sim.ecs.0,
            content,
            self.sector_spawns,
            apply_loadout,
        );
        // The wave spawner spawns/rebuilds path tiles for the session's wave
        // team (`rules.waveTeam`).
        self.sim.set_unit_wave_team(runtime.session.wave_team());
        self.put_campaign_runtime(runtime);
        Some(sync)
    }

    /// Queues the spawn of `wave` (0-based) on the live unit runtime; the
    /// `TickSet::RunWave` system emits it on the next tick. No-op without the
    /// runtime. Rust-only seam for `MindCampaign.run_wave`.
    pub fn request_wave_spawn(&mut self, wave: i32) {
        self.sim.request_wave_spawn(wave);
    }

    /// Applies `Planet.sectorCaptureReplacements` to the loaded tile floors
    /// (`Logic`'s `SectorCaptureEvent` listener). Returns the replaced count.
    pub fn apply_capture_replacements(&mut self, pairs: &[(String, String)]) -> i64 {
        let Some(registry) = self.content_snapshot.as_ref() else {
            return 0;
        };
        let map: Vec<(u16, u16)> = pairs
            .iter()
            .filter_map(|(from, to)| {
                Some((registry.block_id(from)?.get(), registry.block_id(to)?.get()))
            })
            .collect();
        if map.is_empty() {
            return 0;
        }
        let mut replaced = 0i64;
        for tile in self.sim.grid.tiles.array_mut() {
            for (from, to) in &map {
                if tile.floor.get() == *from {
                    tile.floor = BlockId::new(*to);
                    replaced += 1;
                }
            }
        }
        if replaced > 0 {
            self.world_dirty = true;
            log::info!("capture replacements: {replaced} floor tile(s) rethemed");
        }
        replaced
    }

    /// Linearly searches the grid for `(x, y, block-id)` pairs of a rectangle;
    /// Rust-only seam for `MindCampaign.write_schematic_selection`.
    pub fn tile_region(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<(i16, i16, u16)> {
        let (min_x, max_x) = (x0.min(x1), x0.max(x1));
        let (min_y, max_y) = (y0.min(y1), y0.max(y1));
        let mut out = Vec::new();
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let Ok(x16) = i16::try_from(x) else { continue };
                let Ok(y16) = i16::try_from(y) else { continue };
                if let Some(block) = self
                    .sim
                    .grid
                    .block_at(mind_core::world::TilePos::new(x16, y16))
                {
                    out.push((x16, y16, block.raw()));
                }
            }
        }
        out
    }

    /// Immediate block placement at a tile (schematic placement seam). The P0
    /// `Sim::apply` path carries rotation `0` only; rotated schematics degrade
    /// to unrotated until the rotate relay command lands (I-2).
    pub fn place_block_at(&mut self, x: i16, y: i16, block: BlockId, rot: i8) -> bool {
        let _ = rot;
        match self.sim.apply(Command::Place { x, y, block }) {
            Ok(()) => {
                self.world_dirty = true;
                true
            }
            Err(error) => {
                log::warn!("schematic place at ({x}, {y}) rejected: {error}");
                false
            }
        }
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

    /// Number of completed world loads; the camera rig snaps on a change.
    pub fn world_loads(&self) -> u64 {
        self.world_loads
    }

    /// Player best-core camera target for a world load (`Control.WorldLoadEvent`
    /// → `camera.position.set(player.bestCore())`), in world pixels. `None`
    /// before content boot or on a core-less map.
    pub fn best_core_position(&self) -> Option<(f32, f32)> {
        first_core_position(&self.sim.grid, self.content_snapshot.as_ref()?)
    }

    /// Read-only world grid for the plan-16 floor/block bakers.
    pub fn grid(&self) -> &mind_core::world::WorldGrid {
        &self.sim.grid
    }

    /// Plan-18 sim audio log (`MindAudio` drains it; Rust-only seam).
    pub fn audio_log(&self) -> SharedAudioLog {
        self.audio_log.clone()
    }

    /// Read-only content registry snapshot for block draw metadata.
    pub fn content_registry(&self) -> Option<&ContentRegistry> {
        self.content_snapshot.as_ref()
    }

    /// Item counts of the building at `(x, y)` as JSON (`{item-name: amount}`).
    ///
    /// `None` when the tile holds no item-capable building or its item module
    /// is empty (`Block.hasItems && items.total() > 0`,
    /// `InputHandler.tileTapped`). Rust-only input-bridge seam; not MCP API.
    pub fn block_items_json(&self, x: i32, y: i32) -> Option<String> {
        let content = self.content_snapshot.as_ref()?;
        block_items_json(&self.sim, content, x, y)
    }

    /// Installs the opt-in live block runtime (plan-07 `BlockTable` + behaviors)
    /// over the host's content snapshot.
    ///
    /// The default `Sim` spine stores a bare `BuildingComp` per placed block, so
    /// buildings are inert (`update_buildings` is a no-op without the runtime
    /// resources). Installing the runtime makes every subsequent place/break go
    /// through the real tile ops and lets the scheduled building/power systems
    /// run. Idempotent; returns `false` when the content snapshot is unavailable
    /// or the table fails to build.
    fn install_live_block_runtime(&mut self) -> bool {
        if !self.sim.has_block_runtime()
            && let Err(error) = self.sim.install_block_runtime()
        {
            log::warn!("live block runtime install failed: {error}");
            return false;
        }
        // EV-0048: the live unit/combat runtime rides on the same content/world;
        // a failed install keeps buildings working but leaves units inert.
        if !self.sim.has_unit_runtime()
            && let Err(error) = self.sim.install_unit_runtime()
        {
            log::warn!("live unit runtime install failed: {error}");
            return false;
        }
        true
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

    /// Drains queued relay commands and applies them at a tick boundary
    /// (plan 21 §3.5: apply only at fixed-tick starts, never mid-frame).
    fn drain_pending_commands(&mut self) {
        while let Some(command) = self.pending_commands.pop_front() {
            match self.sim.command(command) {
                Ok(()) => self.world_dirty = true,
                Err(error) => log::warn!("relay command rejected by sim: {error}"),
            }
        }
    }

    /// Runs `steps` fixed steps, applying scenario commands before each tick.
    fn advance_steps(&mut self, steps: u32) {
        for _ in 0..steps {
            self.drain_pending_commands();
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
            self.tick_fx_view();
        }
    }

    /// Advances the plan-17 `MindFx` view by one fixed tick (sim→view hook).
    fn tick_fx_view(&mut self) {
        if let Some(node) = self.fx_host.as_mut()
            && node.has_method("tick_view")
        {
            let _ = node.call("tick_view", &[]);
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
            self.revision = self.revision.wrapping_add(1);
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

    /// Player-core item counts as JSON (`{item-name: amount}`) for the HUD core
    /// display (`CoreItemsDisplay`). Appended for `MindHud`; read-only. Mirrors
    /// `player.team().core()`: the first core block in entity order.
    #[func]
    pub fn core_items_json(&self) -> GString {
        let Some(content) = self.content_snapshot.as_ref() else {
            return GString::from("{}");
        };
        GString::from(core_items_json(&self.sim, content).as_str())
    }

    /// Live core items of `team` as `(content name, amount)` (`Team.items()`:
    /// the first core in entity order). Rust-only seam for the campaign
    /// research spend.
    pub fn player_core_items(&self, team: u8) -> Vec<(String, i32)> {
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        for (_seq, entity, comp) in self.sim.ecs.entities_by_seq() {
            if comp.team.0 != team {
                continue;
            }
            if !content
                .block(comp.block)
                .is_some_and(|def| def.kind == BlockKind::CoreBlock)
            {
                continue;
            }
            let Some(module) = self.sim.ecs.0.get::<ItemModule>(entity) else {
                continue;
            };
            let mut out = Vec::new();
            for (index, amount) in module.items.iter().enumerate() {
                if *amount <= 0 {
                    continue;
                }
                if let Some(item) = content.item(ItemId::new(index as u16)) {
                    out.push((item.name.clone(), *amount));
                }
            }
            return out;
        }
        Vec::new()
    }

    /// `Team.items().remove(item, amount)` on the first core of `team`; returns
    /// the removed amount (`0` without a matching core/item). Rust-only seam for
    /// the campaign research spend.
    pub fn remove_player_core_items(&mut self, team: u8, item: &str, amount: i32) -> i32 {
        if amount <= 0 {
            return 0;
        }
        let Some(content) = self.content_snapshot.as_ref() else {
            return 0;
        };
        let Some(item) = content.item_id(item) else {
            return 0;
        };
        let mut target = None;
        for (_seq, entity, comp) in self.sim.ecs.entities_by_seq() {
            if comp.team.0 != team {
                continue;
            }
            let is_core = content
                .block(comp.block)
                .is_some_and(|def| def.kind == BlockKind::CoreBlock);
            if !is_core || self.sim.ecs.0.get::<ItemModule>(entity).is_none() {
                continue;
            }
            target = Some(entity);
            break;
        }
        let Some(entity) = target else {
            return 0;
        };
        match self.sim.ecs.0.get_mut::<ItemModule>(entity) {
            Some(mut module) => module.remove(item, amount),
            None => 0,
        }
    }
}

/// First core block's item counts as a JSON object (empty when there is no core
/// or content boot failed). See [`MindSimHost::core_items_json`].
fn core_items_json(sim: &Sim, content: &ContentRegistry) -> String {
    let world = &sim.ecs.0;
    for (_id, entity, comp) in sim.ecs.entities_by_seq() {
        let Some(def) = content.block(comp.block) else {
            continue;
        };
        if def.kind != BlockKind::CoreBlock {
            continue;
        }
        let Some(module) = world.get::<ItemModule>(entity) else {
            continue;
        };
        let mut items = serde_json::Map::new();
        for (index, amount) in module.items.iter().enumerate() {
            if *amount <= 0 {
                continue;
            }
            let Some(item) = content.item(ItemId::new(index as u16)) else {
                continue;
            };
            items.insert(item.name.clone(), serde_json::Value::from(*amount));
        }
        return serde_json::to_string(&serde_json::Value::Object(items))
            .unwrap_or_else(|_| String::from("{}"));
    }
    String::from("{}")
}

/// Item counts for the building at `(x, y)` as JSON; `None` when the tile has
/// no item-capable building or its items total zero. See
/// [`MindSimHost::block_items_json`].
fn block_items_json(sim: &Sim, content: &ContentRegistry, x: i32, y: i32) -> Option<String> {
    let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
        return None;
    };
    let pos = mind_core::world::TilePos::new(tx, ty);
    let def = content.block(sim.grid.block_at(pos)?)?;
    if !def.has_items {
        return None;
    }
    let entity = sim.grid.entity_at(pos)?;
    let module = sim.ecs.0.get::<ItemModule>(entity)?;
    if module.total() <= 0 {
        return None;
    }
    let mut items = serde_json::Map::new();
    for (index, amount) in module.items.iter().enumerate() {
        if *amount <= 0 {
            continue;
        }
        let Some(item) = content.item(ItemId::new(index as u16)) else {
            continue;
        };
        items.insert(item.name.clone(), serde_json::Value::from(*amount));
    }
    serde_json::to_string(&serde_json::Value::Object(items)).ok()
}

/// Resolves a Godot `user://` / `res://` path to a native path for plan-04
/// `NativeFs` (the `SimIoHandler` runs in Godot-free `mind-core`).
fn globalize(path: &GString) -> std::path::PathBuf {
    let requested = path.to_string();
    std::path::PathBuf::from(
        ProjectSettings::singleton()
            .globalize_path(&requested)
            .to_string(),
    )
}

/// A grid loaded from a save/preset-map file plus the rules the container
/// carried (MSAV map rules / MGRS save rules), when any.
struct LoadedGrid {
    grid: WorldGrid,
    rules: Option<Rules>,
    /// Decoded building payloads `(tile index, base)` awaiting ECS spawn.
    pending_buildings: Vec<(usize, mind_core::world::building_io::DecodedBase)>,
}

/// Native path of a campaign sector save (`user://saves/sector-<planet>-<id>.msav`).
fn sector_save_path(planet_name: &str, sector: u16) -> String {
    let root = ProjectSettings::singleton()
        .globalize_path("user://")
        .to_string();
    std::path::Path::new(&root)
        .join("saves")
        .join(format!("sector-{planet_name}-{sector}.msav"))
        .to_string_lossy()
        .into_owned()
}

/// `World.loadSector` preset branch: loads the preset's `maps/<planet>/<map>.msav`
/// through the save reader into a fresh [`WorldGrid`]. Returns `None` when the
/// sector cell has no preset, the file is absent, or the load fails (the caller
/// then uses the planet generator).
fn load_preset_map_grid(
    planet_name: &str,
    sector: u16,
    registry: &mut ContentRegistry,
) -> Option<LoadedGrid> {
    // Resolve `sector.preset` and its map name (`SectorPreset.initialize`).
    let (owner_name, map_name) = {
        let planet = registry.planet_by_name(planet_name)?;
        let cell = planet.sectors.get(sector as usize)?;
        let preset = registry.sector(cell.preset?)?;
        let owner = registry
            .planet(preset.planet)
            .map(|planet| planet.name.clone())
            .unwrap_or_else(|| planet_name.to_owned());
        let map = preset
            .file_name
            .clone()
            .unwrap_or_else(|| preset.name.clone());
        (owner, map)
    };
    let path = format!(
        "{}/maps/{owner_name}/{map_name}.msav",
        crate::assets::loader::resolve_assets_dir()
    );
    if !FileAccess::file_exists(&path) {
        log::warn!("load_sector: preset map missing `{path}`");
        return None;
    }
    let loaded = load_grid_file(&path, registry)?;
    log::info!(
        "load_sector: loaded preset map `{owner_name}/{map_name}` ({}x{})",
        loaded.grid.tiles.width,
        loaded.grid.tiles.height
    );
    Some(loaded)
}

/// Reads an `.msav`/`MGRS` file into a fresh [`WorldGrid`] through the save
/// reader, preserving the container's rules.
fn load_grid_file(path: &str, registry: &mut ContentRegistry) -> Option<LoadedGrid> {
    let bytes = FileAccess::get_file_as_bytes(path);
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let (result, rules, pending_buildings) = {
        let mut context = EditorContext::new(&mut grid, registry);
        let (result, rules) = {
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(registry),
                ..SaveReadState::default()
            };
            let result = SaveIo::load_bytes(bytes.as_slice(), &mut state);
            (result, state.rules.clone())
        };
        (
            result,
            rules,
            std::mem::take(&mut context.pending_buildings),
        )
    };
    if let Err(error) = result {
        log::warn!("load_sector: `{path}` failed: {error}");
        return None;
    }
    grid.end_map_load(registry);
    Some(LoadedGrid {
        grid,
        rules,
        pending_buildings,
    })
}

/// Counts enemy spawn overlays (`BlockPalette.is_spawn`) on a loaded grid.
fn count_spawns(grid: &WorldGrid, content: &ContentRegistry) -> i32 {
    grid.tiles
        .array()
        .iter()
        .filter(|tile| {
            content
                .block(tile.overlay)
                .is_some_and(|def| def.kind == BlockKind::SpawnBlock)
        })
        .count() as i32
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

/// Rendered center of the first core in row-major order, matching the block
/// renderer's `(tile + 0.5) * TILESIZE + Block.offset` quad center.
///
/// Upstream `Player.bestCore()` picks the player team's core; the port's tile
/// model carries no team, so this returns the first core tile scanned.
fn first_core_position(grid: &WorldGrid, content: &ContentRegistry) -> Option<(f32, f32)> {
    let unit = mind_core::config::TILESIZE as f32;
    grid.iter_row_major().find_map(|(pos, index)| {
        let def = content.block(grid.block_id_at(index))?;
        (def.kind == BlockKind::CoreBlock).then(|| {
            (
                (pos.x() as f32 + 0.5) * unit + def.offset,
                (pos.y() as f32 + 0.5) * unit + def.offset,
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::world::TilePos;

    fn base_registry() -> ContentRegistry {
        let bundle = MemoryBundle::new();
        let store = MemoryUnlockStore::new();
        match create_base_content(&bundle, &store, true) {
            Ok(mut registry) => {
                if let Err(error) = registry.init() {
                    panic!("registry init failed: {error}");
                }
                if let Err(error) = registry.post_init() {
                    panic!("registry post_init failed: {error}");
                }
                registry
            }
            Err(error) => panic!("content boot failed: {error}"),
        }
    }

    #[test]
    fn best_core_position_uses_the_core_tile_center() {
        let registry = base_registry();
        let Some(core) = registry.block_id("core-shard") else {
            panic!("core-shard missing");
        };
        let Some(def) = registry.block(core) else {
            panic!("core-shard def missing");
        };
        let offset = def.offset;
        let mut grid = WorldGrid::new(8, 8);
        grid.fill(BlockId::AIR, BlockId::AIR);
        if grid.set_block(TilePos::new(3, 5), core, 0, 0).is_err() {
            panic!("set_block failed");
        }
        let unit = mind_core::config::TILESIZE as f32;
        assert_eq!(
            first_core_position(&grid, &registry),
            Some(((3.0 + 0.5) * unit + offset, (5.0 + 0.5) * unit + offset))
        );
    }

    #[test]
    fn best_core_position_is_none_without_cores() {
        let registry = base_registry();
        let mut grid = WorldGrid::new(4, 4);
        grid.fill(BlockId::AIR, BlockId::AIR);
        assert_eq!(first_core_position(&grid, &registry), None);
    }

    #[test]
    fn core_items_json_reports_the_core_inventory() {
        let registry = base_registry();
        let Some(core) = registry.block_id("core-shard") else {
            panic!("core-shard missing");
        };
        let Some(copper) = registry.item_id("copper") else {
            panic!("copper missing");
        };
        let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
        if let Err(error) = sim.apply(Command::Place {
            x: 4,
            y: 4,
            block: core,
        }) {
            panic!("place core failed: {error}");
        }
        let Some(entity) = sim.grid.tile(4, 4).build else {
            panic!("core building entity missing");
        };
        {
            // `Sim::apply` spawns the base building only; the plan-08 building
            // path attaches the item module, so attach the fixture module here.
            let mut module = ItemModule::with_items(registry.items().len());
            module.add(copper, 25, 1000);
            sim.ecs.0.entity_mut(entity).insert(module);
        }
        let json = core_items_json(&sim, &registry);
        assert!(json.contains("\"copper\":25"), "unexpected JSON: {json}");
    }

    #[test]
    fn block_items_json_gates_on_the_item_total() {
        let registry = base_registry();
        let Some(conveyor) = registry.block_id("conveyor") else {
            panic!("conveyor missing");
        };
        let Some(copper) = registry.item_id("copper") else {
            panic!("copper missing");
        };
        let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
        if let Err(error) = sim.apply(Command::Place {
            x: 2,
            y: 2,
            block: conveyor,
        }) {
            panic!("place conveyor failed: {error}");
        }
        let Some(entity) = sim.grid.tile(2, 2).build else {
            panic!("conveyor building entity missing");
        };
        // `Sim::apply` spawns the base building only; the plan-08 building path
        // attaches the item module, so attach the fixture module here.
        sim.ecs
            .0
            .entity_mut(entity)
            .insert(ItemModule::with_items(registry.items().len()));
        // Zero stored items opens no inventory (`InputHandler.java:2017`).
        assert_eq!(block_items_json(&sim, &registry, 2, 2), None);
        if let Some(mut module) = sim.ecs.0.get_mut::<ItemModule>(entity) {
            module.add(copper, 3, 1000);
        }
        let json = match block_items_json(&sim, &registry, 2, 2) {
            Some(json) => json,
            None => panic!("items JSON missing"),
        };
        assert!(json.contains("\"copper\":3"), "unexpected JSON: {json}");
    }
}
