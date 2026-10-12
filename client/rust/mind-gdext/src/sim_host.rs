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
use godot::classes::{
    DirAccess, FileAccess, INode, InputEvent, InputEventMouseButton, Os, ProjectSettings,
};
use godot::global::MouseButton;
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use crate::camera::MindCamera2D;
use crate::input::MindInput;
use crate::settings;

/// One belt slot as the renderer consumes it: `(item, xs, ys)` (`Conveyor.draw`
/// item loop; `ConveyorBuild.ids/xs/ys`).
pub type BeltItemSlot = (ItemId, f32, f32);

/// One unit's full draw state (`UnitType.draw` composition; plan §3.4).
pub struct UnitDraw {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// Body rotation in degrees (`UnitCore.rotation`).
    pub rotation: f32,
    /// Unit type name (`UnitType.name`).
    pub name: String,
    /// `UnitType.flying`.
    pub flying: bool,
    /// `UnitType.lowAltitude`.
    pub low_altitude: bool,
    /// Owning team id.
    pub team: u8,
    /// Health fraction (`Unit.healthf()`).
    pub healthf: f32,
    /// Flight elevation `0..=1` (`UnitCore.elevation`).
    pub elevation: f32,
    /// Mech walk state (`MechComp`); `None` for non-mechs.
    pub mech: Option<MechDraw>,
    /// Live weapon mounts (`UnitWeapons`).
    pub weapons: Vec<WeaponDraw>,
}

/// `MechComp` walk state resolved for drawing (`drawMech`).
pub struct MechDraw {
    /// Decoupled base rotation in degrees (`baseRotation`).
    pub base_rotation: f32,
    /// Walk-cycle timer (`walkTime`).
    pub walk_time: f32,
    /// `UnitType.mechStride`.
    pub stride: f32,
}

/// One live weapon mount (`Weapon.draw` inputs).
pub struct WeaponDraw {
    /// Weapon region name (`Weapon.name`).
    pub name: String,
    /// Unit-relative mount x (`Weapon.x`).
    pub x: f32,
    /// Unit-relative mount y (`Weapon.y`).
    pub y: f32,
    /// Aiming rotation relative to the unit, degrees (`WeaponMount.rotation`).
    pub mount_rotation: f32,
    /// Rest rotation when `rotate` is false (`Weapon.baseRotation`).
    pub base_rotation: f32,
    /// Whether the mount rotates toward the target (`Weapon.rotate`).
    pub rotate: bool,
    /// Mirrored mount (`Weapon.flipSprite`).
    pub flip: bool,
    /// Visual recoil `0..1`.
    pub recoil: f32,
}
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
    /// Team build plans carried by the last `load_sector` save
    /// (`SaveReadState.team_plans`); `MindCampaign.start_sector` applies them to
    /// the installed runtime (the lost-sector replay restores its survivors'
    /// plans alongside).
    sector_plans: Vec<(i32, Vec<mind_core::io::save::state::TeamPlan>)>,
    /// Sibling `Input`; the player move axis is uploaded to the sim each frame
    /// (`DesktopInput.updateMovement`).
    input: Option<Gd<MindInput>>,
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
            sector_plans: Vec::new(),
            input: None,
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
        // `DesktopInput.updateMovement`: the axis reaches the player unit each
        // tick; the same axis no longer pans the camera while a player exists
        // (`MindCamera2D::process`).
        let (axis_x, axis_y) = self.player_axis();
        self.sim.set_player_input(axis_x, axis_y);
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

        // Sibling `Input` owns the binding state; the player move axis is read
        // here each frame and uploaded to the sim (`DesktopInput.updateMovement`).
        self.input = self.base().try_get_node_as::<MindInput>("../Input");

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
        let coord = |key: &str| {
            value
                .get(key)
                .and_then(|v| v.as_i64())
                .and_then(|v| i16::try_from(v).ok())
        };
        let (x, y) = (coord("x"), coord("y"));
        // Optional issuing player: `Some` routes place/break through the
        // player unit's build queue like live input (PAR-0098).
        let player = value
            .get("player")
            .and_then(|v| v.as_i64())
            .map(|id| id as i32);
        let command = match op {
            "place" => {
                let (Some(x), Some(y)) = (x, y) else {
                    log::warn!("apply_sim_command_json: place needs x,y");
                    return false;
                };
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
                    player,
                }
            }
            "break" => {
                let (Some(x), Some(y)) = (x, y) else {
                    log::warn!("apply_sim_command_json: break needs x,y");
                    return false;
                };
                SimCommand::Break { x, y, player }
            }
            "spawn_unit" => {
                let unit = value.get("unit").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
                let team = value.get("team").and_then(|v| v.as_u64()).unwrap_or(1) as u8;
                let fx = value
                    .get("px")
                    .and_then(|v| v.as_f64())
                    .or_else(|| x.map(f64::from))
                    .unwrap_or(0.0);
                let fy = value
                    .get("py")
                    .and_then(|v| v.as_f64())
                    .or_else(|| y.map(f64::from))
                    .unwrap_or(0.0);
                SimCommand::SpawnUnit {
                    unit,
                    x: fx as f32,
                    y: fy as f32,
                    team,
                }
            }
            "unit_command" => {
                let units: smallvec::SmallVec<[i32; 32]> = value
                    .get("units")
                    .and_then(|v| v.as_array())
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|row| row.as_i64().map(|id| id as i32))
                            .collect()
                    })
                    .unwrap_or_default();
                if units.is_empty() {
                    log::warn!("apply_sim_command_json: unit_command without units");
                    return false;
                }
                let command = value.get("command").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
                // `unit_command` targets are world pixels (floats), not tiles.
                let fx = value.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let fy = value.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                SimCommand::UnitCommand {
                    units,
                    command,
                    x: fx as f32,
                    y: fy as f32,
                }
            }
            other => {
                log::warn!("apply_sim_command_json: unsupported op `{other}`");
                return false;
            }
        };
        self.pending_commands.push_back(command);
        true
    }

    /// `state.rules.defaultTeam` of the live session (campaign rules, then the
    /// build guard resource). The RTS input layer filters selections by it
    /// (`player.team().id`; PAR-0140).
    #[func]
    pub fn default_team(&self) -> i64 {
        self.sim.live_default_team() as i64
    }

    /// Live selectable-unit snapshot (`[{id,type,x,y,team,commandable}]`, the
    /// same rows fed to `MindInput`; PAR-0140 probe).
    #[func]
    pub fn selectable_units_json(&self) -> GString {
        GString::from(self.sim.selectable_units_json().as_str())
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

    /// Stores host playtime (`Saves.getTotalPlaytime`) for the next save's meta
    /// region (`SaveVersion.writeMeta`).
    #[func]
    pub fn set_save_playtime(&mut self, millis: i64) {
        self.sim.set_save_playtime(millis);
    }

    /// Runs queued save/load requests at their `IoSet` boundaries without
    /// advancing the sim (the exit-save flush while the pump is paused).
    pub fn flush_io(&mut self) {
        self.sim.flush_io();
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

    /// Drains queued IO responses as JSON (`[{kind,path,as_map?,error?}]`), so
    /// save/load failures are host-visible (`take_io_requests_json` response
    /// half; the loading overlay only polls `io_pending`).
    #[func]
    pub fn take_io_responses_json(&mut self) -> GString {
        let value: Vec<serde_json::Value> = self
            .sim
            .take_io_responses()
            .iter()
            .map(|response| {
                let mut row = match &response.request {
                    mind_core::sim::IoRequest::Save { path, as_map } => serde_json::json!({
                        "kind": "save",
                        "path": path.display().to_string(),
                        "as_map": as_map,
                    }),
                    mind_core::sim::IoRequest::Load { path } => serde_json::json!({
                        "kind": "load",
                        "path": path.display().to_string(),
                    }),
                };
                if let mind_core::sim::IoStatus::Failed(error) = &response.status {
                    row["error"] = serde_json::Value::String(error.clone());
                }
                row
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
        let planet_name = planet.to_string();
        self.load_sector_world(&planet_name, sector, true)
    }

    /// Shared `World.loadSector` build: `use_save` selects the resume source
    /// (the lost-sector replay reloads fresh with `false`).
    fn load_sector_world(&mut self, planet_name: &str, sector: i32, use_save: bool) -> bool {
        if sector < 0 {
            return false;
        }
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
        let save_path = sector_save_path(planet_name, sector as u16);
        let loaded = if use_save && std::path::Path::new(&save_path).is_file() {
            log::info!("load_sector: resuming `{planet_name}:{sector}` from `{save_path}`");
            load_grid_file(&save_path, &mut registry)
        } else {
            // `World.loadSector`: a preset sector uses its stored designed map
            // (`SectorPreset.generator` / `FileMapGenerator`); planet sectors
            // fall back to the planet generator.
            load_preset_map_grid(planet_name, sector as u16, &mut registry)
        };
        let generated = if loaded.is_none() {
            mind_core::maps::planet::generate_sector(
                planet_name,
                sector as u16,
                1,
                SECTOR_SIZE,
                SECTOR_SIZE,
                &registry,
            )
        } else {
            None
        };
        let (grid, rules, pending_buildings, plans) = match loaded {
            Some(loaded) => (
                loaded.grid,
                loaded.rules,
                loaded.pending_buildings,
                loaded.plans,
            ),
            None => match generated {
                Some(generated) => (
                    generated.grid,
                    Some(generated.rules),
                    Vec::new(),
                    Vec::new(),
                ),
                None => {
                    self.content_snapshot = Some(registry);
                    log::warn!("load_sector: no map or generator for planet `{planet_name}`");
                    return false;
                }
            },
        };
        self.sector_spawns = count_spawns(&grid, &registry);
        self.sector_rules = rules;
        self.sector_plans = plans;
        // `Vars.bases` (PAR-0139): load before the registry moves into the
        // content snapshot; the resource is inserted once the sim exists.
        let base_catalog = load_base_catalog(&registry);
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
        if let Some(catalog) = base_catalog {
            self.sim
                .ecs
                .0
                .insert_resource(mind_core::game::runtime::BaseCatalog(std::sync::Arc::new(
                    catalog,
                )));
        }
        // `World.loadMap`'s building half: the reader decoded the map's tile
        // entities into the pending queue while the grid was built, so spawn
        // the ECS buildings (cores included) the campaign runtime/teams read.
        let materialized = self.sim.materialize_map_buildings(&pending_buildings);
        if materialized > 0 {
            log::info!("load_sector: materialized {materialized} map building(s)");
        }
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

    /// Takes the plans carried by the last `load_sector` save (`apply_team_plans`
    /// input); clears the stash.
    pub fn take_sector_plans(&mut self) -> Vec<(i32, Vec<mind_core::io::save::state::TeamPlan>)> {
        std::mem::take(&mut self.sector_plans)
    }

    /// Default-team core count in the live world (the lost-sector probe; the
    /// session's `Teams.cores` is not installed on a fresh load).
    pub fn player_core_blocks(&self, team: u8) -> usize {
        let Some(content) = self.content_snapshot.as_ref() else {
            return 0;
        };
        self.sim
            .ecs
            .entities_by_seq()
            .into_iter()
            .filter(|(_, _, comp)| {
                comp.team.0 == team
                    && content
                        .block(comp.block)
                        .is_some_and(|def| def.kind == BlockKind::CoreBlock)
            })
            .count()
    }

    /// `Control.playSector`'s lost-sector replay (`Control.java:456-537`): the
    /// loaded (lost) world's survivors are snapshotted after `SectorDamage(1f)`,
    /// the sector reloads fresh with the default core at `(spawn_x, spawn_y)`,
    /// and the survivors come back as derelicts. Returns the restored survivors
    /// (the facade adds their plans alongside the carried save plans).
    pub fn replay_lost_sector(
        &mut self,
        planet: GString,
        sector: i32,
        spawn_x: i32,
        spawn_y: i32,
    ) -> Vec<mind_core::sim::runtime::ReplayBuilding> {
        if sector < 0 {
            return Vec::new();
        }
        let planet_name = planet.to_string();
        let (default_team, wave_team) = self
            .sector_rules
            .as_ref()
            .map(|rules| (rules.default_team, rules.wave_team))
            .unwrap_or((1, 2));
        // `SectorDamage.apply(1f)` + the survivor/derelict snapshot.
        let (survivors, derelicts) = self.sim.capture_lost_sector(default_team, wave_team);
        let plans = self.take_sector_plans();
        // Fresh sector world (`playNewSector`; the save was the snapshot source).
        if !self.load_sector_world(&planet_name, sector, false) {
            return Vec::new();
        }
        // Restore the default core at the record's spawn position
        // (`spawn.setBlock(sector.planet.defaultCore, ...)`).
        if self.player_core_blocks(default_team) == 0
            && let Some(core_name) = self
                .content_snapshot
                .as_ref()
                .and_then(|registry| registry.planet_by_name(&planet_name))
                .and_then(|planet| planet.default_core.clone())
            && let Some(core) = self
                .content_snapshot
                .as_ref()
                .and_then(|registry| registry.block_id(&core_name))
        {
            self.sim.place_replay_building(
                spawn_x as i16,
                spawn_y as i16,
                core,
                0,
                default_team,
                1000.0,
            );
        }
        // Previous derelicts first, then the survivors as derelicts
        // (`changeTeam(Team.derelict)`), each on a free tile.
        let mut restored = Vec::new();
        for entry in derelicts.into_iter().chain(survivors.iter().copied()) {
            if self.sim.place_replay_building(
                entry.x,
                entry.y,
                entry.block,
                entry.rot,
                mind_core::game::team::DERELICT.0,
                entry.health,
            ) {
                restored.push(entry);
            }
        }
        // Hand the carried save plans back to the facade (the runtime applies
        // them with the restored survivors).
        self.sector_plans = plans;
        restored
    }

    /// Installs (replacing any previous) the live campaign runtime resource so
    /// the `TickSet::Campaign`/`Objectives`/`GameStateCheck` systems run.
    pub fn install_campaign_runtime(&mut self, runtime: CampaignRuntime) {
        self.sim.ecs.0.insert_resource(runtime);
    }

    /// Removes the campaign runtime (menu/abort/new world).
    pub fn clear_campaign_runtime(&mut self) {
        self.sim.ecs.0.remove_resource::<CampaignRuntime>();
        // The hooks answer `is_campaign: false` again and drop pending deltas.
        mind_core::game::runtime::reset_campaign_core_hooks(&mut self.sim.ecs.0);
    }

    /// Takes the runtime out for host-side bookkeeping.
    pub fn take_campaign_runtime(&mut self) -> Option<CampaignRuntime> {
        self.sim.ecs.0.remove_resource::<CampaignRuntime>()
    }

    /// Puts a bookkept runtime back.
    pub fn put_campaign_runtime(&mut self, runtime: CampaignRuntime) {
        // The campaign core hooks queue `SectorInfo.handleCoreItem` deltas the
        // campaign tick folds into the played sector (PAR-0082), and the AI
        // queue carries the base-builder placements the pump drains (PAR-0139).
        mind_core::game::runtime::install_campaign_core_hooks(&mut self.sim.ecs.0);
        mind_core::game::runtime::install_campaign_ai_queues(&mut self.sim.ecs.0);
        self.sim.ecs.0.insert_resource(runtime);
    }

    /// Live core snapshot of `team` for `SectorInfo.prepare` (the save-time
    /// sector record: items, capacity, spawn position, map facts). Rust-only
    /// seam for `MindCampaign::persist_sector_record`.
    pub fn campaign_core_snapshot(
        &self,
        team: u8,
    ) -> Option<mind_core::game::sector::CoreSnapshot> {
        let content = self.content_snapshot.as_ref()?;
        Some(mind_core::game::sector::core_snapshot_of(
            &self.sim, content, team,
        ))
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

    /// Binding-driven move axis from the sibling `Input`. `MindInput.pan_axis`
    /// is the shared `move_x`/`move_y` axis; with a player unit it drives the
    /// unit instead of the camera (`DesktopInput.updateMovement`).
    fn player_axis(&self) -> (f32, f32) {
        let Some(input) = self.input.clone() else {
            return (0.0, 0.0);
        };
        let axis = input.bind().pan_axis();
        (axis.x, axis.y)
    }

    /// Player unit world position (`Player` mirror) for the camera follow.
    pub fn player_unit_position(&self) -> Option<(f32, f32)> {
        self.sim.player_unit_position()
    }

    /// Team a local placement carries (`state.rules.defaultTeam`; the live
    /// campaign session wins over the build-guard fallback).
    pub fn player_team(&self) -> u8 {
        self.sim
            .ecs
            .0
            .get_resource::<mind_core::game::runtime::CampaignRuntime>()
            .map(|runtime| runtime.session.rules.default_team)
            .or_else(|| {
                self.sim
                    .ecs
                    .0
                    .get_resource::<mind_core::world::limits::BuildRules>()
                    .map(|rules| rules.default_team)
            })
            .unwrap_or(mind_core::ecs::TeamId::SHARDED.0)
    }

    /// `InputHandler.canMine` for the player unit (world-input seam).
    pub fn player_can_mine(&self, x: i16, y: i16) -> bool {
        self.sim.player_can_mine(x, y)
    }

    /// The player unit's ordered mine tile (world-input seam).
    pub fn player_mine_target(&self) -> Option<(i16, i16)> {
        self.sim.player_mine_tile().map(|tile| (tile.x(), tile.y()))
    }

    /// Sets/clears the player mine order (`tryBeginMine`/`tryStopMine`).
    pub fn set_player_mine(&mut self, tile: Option<(i16, i16)>) {
        self.sim
            .set_player_mine_tile(tile.map(|(x, y)| mind_core::world::TilePos::new(x, y)));
    }

    /// Spawns the campaign player unit at the best core (`CoreBlock.playerSpawn`
    /// → `UnitTypes.alpha`, rotation 90). Idempotent per world load.
    #[func]
    pub fn spawn_player_unit(&mut self) -> bool {
        if self.sim.player_unit_position().is_some() {
            return true;
        }
        let Some((x, y)) = self.best_core_position() else {
            log::warn!("spawn_player_unit: no core in the loaded sector");
            return false;
        };
        // `CoreBlock.playerSpawn` spawns on `rules.defaultTeam`
        // (`Team.sharded` = 1 for the campaign, not `TeamId::SHARDED`).
        let team = self
            .last_sector_rules()
            .map_or(1, |rules| rules.default_team);
        let spawned = self
            .sim
            .spawn_player_unit("alpha", team, x, y, 90.0)
            .is_some();
        if spawned {
            log::info!("spawn_player_unit: alpha team {team} at ({x:.1}, {y:.1})");
        }
        spawned
    }

    /// Whether the player possesses a unit (`Player.unit() != null`).
    #[func]
    pub fn has_player_unit(&self) -> bool {
        self.sim.player_unit().is_some()
    }

    /// HUD status-table read model for the player unit (`player.dead()`,
    /// `unit().healthf()`, `player.icon()` type name): `{dead, healthf, type}`.
    /// `dead` is true (healthf 0) when the player has no unit.
    #[func]
    pub fn player_unit_status(&self) -> VarDictionary {
        use mind_core::entities::comp::Health;
        use mind_core::entities::comp::unit::comp::UnitTypeComp;
        let mut dict = VarDictionary::new();
        let entity = self.sim.player_unit();
        let world = &self.sim.ecs.0;
        let health = entity.and_then(|e| world.get::<Health>(e));
        let type_name = entity
            .and_then(|e| world.get::<UnitTypeComp>(e))
            .and_then(|t| self.content_snapshot.as_ref()?.unit(t.type_id))
            .map(|def| def.name.clone())
            .unwrap_or_default();
        let dead = health.is_none_or(|h| h.dead);
        let healthf = health
            .filter(|h| h.max_health > 0.0)
            .map_or(0.0, |h| h.health / h.max_health);
        dict.set("dead", dead);
        dict.set("healthf", if dead { 0.0 } else { healthf });
        dict.set("type", &GString::from(type_name.as_str()));
        dict
    }

    /// Debug probe: `{name, x, y, team}` rows for every live unit (render
    /// verification; companion to `tile_debug`).
    #[func]
    pub fn unit_debug_positions(&self) -> GString {
        let rows: Vec<serde_json::Value> = self
            .unit_draws_full()
            .iter()
            .map(|unit| {
                serde_json::json!({
                    "name": unit.name,
                    "x": unit.x,
                    "y": unit.y,
                    "team": unit.team,
                })
            })
            .collect();
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// Player unit world position as `[x, y]`, or empty when absent (render
    /// verification probe).
    #[func]
    pub fn player_unit_pos(&self) -> PackedFloat32Array {
        let mut out = PackedFloat32Array::new();
        if let Some(entity) = self.sim.player_unit()
            && let Some(pos) = self.sim.ecs.0.get::<mind_core::entities::comp::Pos>(entity)
        {
            out.push(pos.x);
            out.push(pos.y);
        }
        out
    }

    /// Whether the move axis reached the player unit
    /// (`HintsFragment.desktopMove` trigger).
    #[func]
    pub fn player_unit_moved(&self) -> bool {
        self.sim.player_unit_moved()
    }

    /// The player unit's ordered mine tile as `[x, y]`, or `[]` when idle.
    #[func]
    pub fn player_mine_tile(&self) -> Vec<i32> {
        self.sim
            .player_mine_tile()
            .map(|tile| vec![tile.x() as i32, tile.y() as i32])
            .unwrap_or_default()
    }

    /// Total ore produced by the player unit (tutorial counter evidence).
    #[func]
    pub fn player_mined_count(&self) -> i64 {
        self.sim.player_mined_count()
    }

    /// Debug/playtest seam: adds `amount` of `item` to the player team's first
    /// core (the Java console/sandbox `core.items.add`); returns the amount
    /// accepted (negative removes).
    #[func]
    pub fn debug_add_core_items(&mut self, item: GString, amount: i64) -> i64 {
        let team = self.player_team();
        let amount = i32::try_from(amount).unwrap_or(0);
        if amount < 0 {
            return -i64::from(self.remove_player_core_items(team, &item.to_string(), -amount));
        }
        let Some(item) = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.item_id(&item.to_string()))
        else {
            return 0;
        };
        let target = self
            .sim
            .ecs
            .entities_by_seq()
            .into_iter()
            .find_map(|(_, entity, comp)| {
                (comp.team.0 == team
                    && self.sim.ecs.0.get::<ItemModule>(entity).is_some()
                    && self
                        .sim
                        .ecs
                        .0
                        .get::<mind_core::world::blocks::storage::core_block::CoreBuild>(entity)
                        .is_some())
                .then_some(entity)
            });
        let Some(entity) = target else {
            return 0;
        };
        let capacity = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::blocks::storage::core_block::CoreBuild>(entity)
            .map(|core| core.storage_capacity)
            .unwrap_or(i32::MAX);
        match self.sim.ecs.0.get_mut::<ItemModule>(entity) {
            Some(mut module) => i64::from(module.add(item, amount, capacity)),
            None => 0,
        }
    }

    /// The player unit's build-plan queue as JSON (`BuilderComp.plans`, head
    /// = active): `[{x, y, rotation, block, breaking, progress, stuck}]`.
    #[func]
    pub fn player_build_plans_json(&self) -> GString {
        let rows: Vec<serde_json::Value> = self
            .sim
            .player_build_plans()
            .iter()
            .map(|plan| {
                serde_json::json!({
                    "x": plan.x,
                    "y": plan.y,
                    "rotation": plan.rotation,
                    "block": self.sim.block_name_of(plan.block),
                    "breaking": plan.breaking,
                    "progress": plan.progress,
                    "stuck": plan.stuck,
                })
            })
            .collect();
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// Construct block state at `(x, y)` (`ConstructBuild`): `{block,
    /// progress, deconstructing}`; empty when the tile is not under
    /// construction.
    #[func]
    pub fn construct_progress(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::new();
        if let Some((block, progress, deconstructing)) = self.construct_at(x, y) {
            out.set("block", &self.sim.block_name_of(block).to_variant());
            out.set("progress", &f64::from(progress).to_variant());
            out.set("deconstructing", &deconstructing.to_variant());
        }
        out
    }

    /// Read-only world grid for the plan-16 floor/block bakers.
    pub fn grid(&self) -> &mind_core::world::WorldGrid {
        &self.sim.grid
    }

    /// Live unit draw list — `(x, y, rotation degrees, region name, flying,
    /// low altitude)` — for the plan-16 unit pass. Empty without the runtime.
    pub fn unit_draws(&self) -> Vec<(f32, f32, f32, String, bool, bool)> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        runtime
            .units
            .iter()
            .filter_map(|entity| {
                let pos = self.sim.ecs.0.get::<Pos>(*entity)?;
                let core = self.sim.ecs.0.get::<UnitCore>(*entity)?;
                let type_comp = self.sim.ecs.0.get::<UnitTypeComp>(*entity)?;
                let def = content.unit(type_comp.type_id)?;
                Some((
                    pos.x,
                    pos.y,
                    core.rotation,
                    def.name.clone(),
                    def.flying,
                    def.low_altitude,
                ))
            })
            .collect()
    }

    /// Full per-unit draw state (`UnitType.draw` composition; plan §3.4):
    /// body/team/health plus mech walk state and weapon mounts. Rust-only.
    pub fn unit_draws_full(&self) -> Vec<UnitDraw> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::health::Health;
        use mind_core::entities::comp::unit::comp::{MechComp, UnitCore, UnitTypeComp};
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        runtime
            .units
            .iter()
            .filter_map(|entity| {
                let pos = self.sim.ecs.0.get::<Pos>(*entity)?;
                let core = self.sim.ecs.0.get::<UnitCore>(*entity)?;
                let type_comp = self.sim.ecs.0.get::<UnitTypeComp>(*entity)?;
                let def = content.unit(type_comp.type_id)?;
                let team = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::TeamComp>(*entity)
                    .map(|comp| comp.team)
                    .unwrap_or(0);
                let healthf = self
                    .sim
                    .ecs
                    .0
                    .get::<Health>(*entity)
                    .map(Health::fraction)
                    .unwrap_or(1.0);
                let mech = self
                    .sim
                    .ecs
                    .0
                    .get::<MechComp>(*entity)
                    .map(|mech| MechDraw {
                        base_rotation: mech.base_rotation,
                        walk_time: mech.walk_time,
                        stride: def.mech_stride.max(0.001),
                    });
                let weapons = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::weapons::UnitWeapons>(*entity)
                    .map(|weapons| {
                        weapons
                            .weapons
                            .iter()
                            .enumerate()
                            .map(|(index, weapon)| {
                                let mount = weapons.mounts.get(index);
                                WeaponDraw {
                                    name: weapon.name.clone(),
                                    x: weapon.x,
                                    y: weapon.y,
                                    mount_rotation: mount
                                        .map(|mount| mount.rotation)
                                        .unwrap_or(weapon.base_rotation),
                                    base_rotation: weapon.base_rotation,
                                    rotate: weapon.rotate,
                                    flip: weapon.flip_sprite,
                                    recoil: mount.map(|mount| mount.recoil).unwrap_or(0.0),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Some(UnitDraw {
                    x: pos.x,
                    y: pos.y,
                    rotation: core.rotation,
                    name: def.name.clone(),
                    flying: def.flying,
                    low_altitude: def.low_altitude,
                    team,
                    healthf,
                    elevation: core.elevation,
                    mech,
                    weapons,
                })
            })
            .collect()
    }

    /// Plan-18 sim audio log (`MindAudio` drains it; Rust-only seam).
    pub fn audio_log(&self) -> SharedAudioLog {
        self.audio_log.clone()
    }

    /// Read-only content registry snapshot for block draw metadata.
    pub fn content_registry(&self) -> Option<&ContentRegistry> {
        self.content_snapshot.as_ref()
    }

    /// Config value kinds the block's behavior accepts (`Block.config(...)`,
    /// `Block.configurations` keys) from the live block runtime. Empty when the
    /// runtime is not installed or the behavior declares none.
    pub fn block_config_kinds(&self, block: BlockId) -> &'static [mind_core::world::ConfigKind] {
        self.sim
            .ecs
            .0
            .get_resource::<mind_core::world::block::BlockTable>()
            .and_then(|table| table.get(block).map(|inst| inst.config_kinds()))
            .unwrap_or(&[])
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

    /// Applies queued AI base placements for their owning team at the tick
    /// boundary (`team_ai_system` -> `CampaignAiQueues`; PAR-0139).
    fn drain_ai_placements(&mut self) {
        let Some(queues) = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::runtime::CampaignAiQueues>()
        else {
            return;
        };
        let requests: Vec<mind_core::game::runtime::AiPlaceRequest> = match queues.0.lock() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => return,
        };
        for request in requests {
            let placed = self.sim.place_for_team(
                request.x,
                request.y,
                request.block,
                request.rot,
                request.team,
            );
            if placed {
                self.world_dirty = true;
                log::info!(
                    "campaign ai: placed team {} block at ({}, {})",
                    request.team,
                    request.x,
                    request.y
                );
            } else {
                log::debug!(
                    "campaign ai: rejected team {} placement at ({}, {})",
                    request.team,
                    request.x,
                    request.y
                );
            }
        }
    }

    /// Publishes the live selectable-unit snapshot to the sibling `Input`
    /// (`MindInput.set_selectable_units_json`; PAR-0140): upstream feeds the
    /// selectable list from `Units` each frame, so drag/tap selection and
    /// command mode see live units.
    fn publish_selectable_units(&mut self) {
        let Some(mut input) = self.input.clone() else {
            return;
        };
        let json = self.sim.selectable_units_json();
        input
            .bind_mut()
            .set_selectable_units_json(GString::from(json.as_str()));
    }

    /// Runs `steps` fixed steps, applying scenario commands before each tick.
    fn advance_steps(&mut self, steps: u32) {
        for _ in 0..steps {
            self.drain_pending_commands();
            self.drain_ai_placements();
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
        self.publish_selectable_units();
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

    /// Belt items at `(x, y)`: `(rotation, [(item, xs, ys); len])` for a
    /// `Conveyor`/`ArmoredConveyor` (`Conveyor.draw` item loop). `None` when
    /// the tile holds no belt. Rust-only seam for the block renderer.
    pub fn conveyor_items(&self, x: i32, y: i32) -> Option<(u8, Vec<BeltItemSlot>)> {
        let entity = self.sim.grid.tile(x, y).build?;
        let rot = self
            .sim
            .ecs
            .0
            .get::<mind_core::ecs::BuildingComp>(entity)?
            .rot;
        let belt = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::blocks::distribution::ConveyorBuild>(entity)?;
        let mut items = Vec::with_capacity(belt.len as usize);
        for i in 0..belt.len as usize {
            items.push((belt.ids[i], belt.xs[i], belt.ys[i]));
        }
        Some((rot, items))
    }

    /// Hover target at tile `(x, y)` (`PlacementFragment.hovered`): a unit
    /// within 5 world px of the tile centre, else the tile's building, else the
    /// tile's displayable drop (`Tile.displayable`/`Tile.display`).
    /// `{kind, name, icon, label, team}`; empty when nothing is hovered.
    #[func]
    pub fn hover_info(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::new();
        let Some(content) = self.content_snapshot.as_ref() else {
            return out;
        };
        let size = mind_core::config::TILESIZE as f32;
        let cx = x as f32 * size;
        let cy = y as f32 * size;
        // `Units.closestOverlap(player.team(), mouseWorld, 5f, ...)`: a unit
        // under the cursor wins over the tile.
        let unit = self
            .unit_draws()
            .into_iter()
            .filter(|(ux, uy, ..)| (ux - cx).hypot(*uy - cy) <= 5.0)
            .map(|(_, _, _, name, ..)| name)
            .next();
        if let Some(name) = unit {
            out.set("kind", &"unit".to_variant());
            out.set("name", &GString::from(name.as_str()).to_variant());
            out.set(
                "icon",
                &GString::from(format!("unit-{name}-ui").as_str()).to_variant(),
            );
            out.set(
                "label",
                &GString::from(format!("unit.{name}.name").as_str()).to_variant(),
            );
            return out;
        }
        let tile = self.sim.grid.tile(x, y);
        // `hoverTile.build.displayable()`: any live building.
        if let Some(entity) = tile.build {
            let build = self
                .sim
                .ecs
                .0
                .get::<mind_core::ecs::BuildingComp>(entity)
                .and_then(|comp| content.block(comp.block));
            if let Some(def) = build {
                out.set("kind", &"building".to_variant());
                out.set("name", &GString::from(def.name.as_str()).to_variant());
                out.set(
                    "icon",
                    &GString::from(format!("block-{}-ui", def.name).as_str()).to_variant(),
                );
                out.set(
                    "label",
                    &GString::from(format!("block.{}.name", def.name).as_str()).to_variant(),
                );
                return out;
            }
        }
        // `Tile.displayable()`: `(drop() != null && block == air) ||
        // wallDrop() != null || (floor().liquidDrop != null && block == air)`.
        let block = content.block(tile.block);
        let overlay = content.block(tile.overlay);
        let floor = content.block(tile.floor);
        let air = tile.block == BlockId::AIR;
        let drop = overlay
            .and_then(|def| def.item_drop)
            .or_else(|| floor.and_then(|def| def.item_drop));
        let wall_drop = block
            .filter(|def| def.solid)
            .and_then(|def| def.item_drop)
            .or_else(|| {
                overlay
                    .filter(|def| !air && def.wall_ore)
                    .and_then(|def| def.item_drop)
            });
        let liquid_drop = floor.is_some_and(|def| def.liquid_drop.is_some());
        if !((drop.is_some() && air) || wall_drop.is_some() || (liquid_drop && air)) {
            return out;
        }
        // `Tile.display`: block drop -> overlay drop/wall drop -> floor.
        let displayed = if block.is_some_and(|def| def.item_drop.is_some()) {
            block
        } else if overlay.is_some_and(|def| def.item_drop.is_some()) || wall_drop.is_some() {
            overlay
        } else {
            floor
        };
        let Some(displayed) = displayed else {
            return out;
        };
        out.set("kind", &"tile".to_variant());
        // `OreBlock` carries the ore item's localized name and displays the
        // item icon; plain walls/floors display their own ui icon.
        if displayed.kind == BlockKind::OreBlock
            && let Some(item_def) = displayed.item_drop.and_then(|item| content.item(item))
        {
            out.set("name", &GString::from(item_def.name.as_str()).to_variant());
            out.set(
                "icon",
                &GString::from(format!("item-{}-ui", item_def.name).as_str()).to_variant(),
            );
            out.set(
                "label",
                &GString::from(format!("item.{}.name", item_def.name).as_str()).to_variant(),
            );
            return out;
        }
        out.set("name", &GString::from(displayed.name.as_str()).to_variant());
        out.set(
            "icon",
            &GString::from(format!("block-{}-ui", displayed.name).as_str()).to_variant(),
        );
        out.set(
            "label",
            &GString::from(format!("block.{}.name", displayed.name).as_str()).to_variant(),
        );
        out
    }

    /// Build requirements of block `name` for the placement info table
    /// (`PlacementFragment` requirement rows): JSON
    /// `[{item, amount, have}]` — amount scaled by `Rules.buildCostMultiplier`
    /// (`Math.round(stack.amount * multiplier)`) and `have` from the player's
    /// first core (`player.core().items`).
    #[func]
    pub fn block_requirements_json(&self, name: GString) -> GString {
        let empty = GString::from("[]");
        let Some(content) = self.content_snapshot.as_ref() else {
            return empty;
        };
        let Some(def) = content.block_id(&name.to_string()) else {
            return empty;
        };
        let Some(def) = content.block(def) else {
            return empty;
        };
        let multiplier = self
            .sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| runtime.session.rules.build_cost_multiplier)
            .filter(|value| *value > 0.0)
            .unwrap_or(1.0);
        let team = self
            .sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| runtime.session.rules.default_team)
            .unwrap_or(0);
        let core_items: std::collections::HashMap<String, i32> =
            self.player_core_items(team).into_iter().collect();
        let rows: Vec<serde_json::Value> = def
            .requirements
            .iter()
            .filter_map(|stack| {
                let item = content.item(stack.item)?;
                let amount = (stack.amount as f32 * multiplier + 0.5).floor() as i32;
                let have = core_items.get(&item.name).copied().unwrap_or(0);
                Some(serde_json::json!({
                    "item": item.name,
                    "amount": amount,
                    "have": have,
                }))
            })
            .collect();
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// Building rotation (`Building.rotation`, quarter turns) at `(x, y)`;
    /// `0` without a building. Rust-only seam for rotated block sprites.
    pub fn building_rotation(&self, x: i32, y: i32) -> u8 {
        self.sim
            .grid
            .tile(x, y)
            .build
            .and_then(|entity| {
                self.sim
                    .ecs
                    .0
                    .get::<mind_core::ecs::BuildingComp>(entity)
                    .map(|comp| comp.rot)
            })
            .unwrap_or(0)
    }

    /// `ConstructBuild` draw state at `(x, y)`: `(current block, progress)`.
    /// Rust-only seam for the construct-site sprite (`ConstructBuild.draw`).
    pub fn construct_draw(&self, x: i32, y: i32) -> Option<(mind_core::content::BlockId, f32)> {
        let entity = self.sim.grid.tile(x, y).build?;
        self.sim
            .ecs
            .0
            .get::<mind_core::world::construct::ConstructState>(entity)
            .map(|state| (state.current, state.progress))
    }

    /// Debug probe (PAR-0152): block name/kind at a tile and whether the tile
    /// holds a `ConstructState`. `sim_name` reads the sim's own content table
    /// and `construct` the render seam, so a mismatch between them is visible.
    #[func]
    pub fn tile_debug(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut d = Dictionary::new();
        let tile = self.sim.grid.tile(x, y);
        let def = self
            .content_snapshot
            .as_ref()
            .and_then(|c| c.block(tile.block));
        let name = def
            .map(|def| def.name.clone())
            .unwrap_or_else(|| "?".into());
        let kind = def.map(|def| format!("{:?}", def.kind)).unwrap_or_default();
        let sim_name = self.sim.block_name_of(tile.block);
        let floor = self
            .content_snapshot
            .as_ref()
            .and_then(|c| c.block(tile.floor))
            .map(|def| def.name.clone())
            .unwrap_or_default();
        let overlay = self
            .content_snapshot
            .as_ref()
            .and_then(|c| c.block(tile.overlay))
            .map(|def| def.name.clone())
            .unwrap_or_default();
        d.set("raw", &(tile.block.raw() as i64).to_variant());
        d.set("name", &GString::from(name.as_str()).to_variant());
        d.set("kind", &GString::from(kind.as_str()).to_variant());
        d.set("sim_name", &GString::from(sim_name.as_str()).to_variant());
        d.set("floor", &GString::from(floor.as_str()).to_variant());
        d.set("overlay", &GString::from(overlay.as_str()).to_variant());
        d.set("has_build", &tile.build.is_some().to_variant());
        d.set(
            "construct",
            &self.construct_draw(x, y).is_some().to_variant(),
        );
        d
    }

    /// Construct block state at `(x, y)` as `(current block, progress,
    /// deconstructing)` (`ConstructBuild`; render seam).
    pub fn construct_at(&self, x: i32, y: i32) -> Option<(BlockId, f32, bool)> {
        let entity = self.sim.grid.tile(x, y).build?;
        let state = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::ConstructState>(entity)?;
        Some((state.current, state.progress, state.active_deconstruct))
    }

    /// Owning team of the building at `(x, y)` (`Tile.team()`; `None` when the
    /// tile has no building). Rust-only seam for the block renderer's team
    /// overlays (`Block.drawTeamOverlay`).
    pub fn building_team(&self, x: i32, y: i32) -> Option<u8> {
        let entity = self.sim.grid.tile(x, y).build?;
        self.sim
            .ecs
            .0
            .get::<mind_core::ecs::BuildingComp>(entity)
            .map(|comp| comp.team.0)
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
    /// Team build plans carried by the save (`SaveReadState.team_plans`); the
    /// campaign runtime applies them (`apply_team_plans`).
    plans: Vec<(i32, Vec<mind_core::io::save::state::TeamPlan>)>,
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

/// Loads `Vars.bases`: reads every `baseparts/*.msch` under the resolved
/// assets dir through the host file access and classifies them (PAR-0139).
fn load_base_catalog(registry: &ContentRegistry) -> Option<mind_core::ai::BaseRegistry> {
    let dir = format!("{}/baseparts", crate::assets::loader::resolve_assets_dir());
    let files = DirAccess::get_files_at(&dir);
    if files.is_empty() {
        log::warn!("base catalog: no `.msch` files under `{dir}`");
        return None;
    }
    let mut schematics = Vec::new();
    for name in files.as_slice() {
        let name = name.to_string();
        if !name.ends_with(".msch") {
            continue;
        }
        let path = format!("{dir}/{name}");
        let bytes = FileAccess::get_file_as_bytes(&path);
        match mind_core::game::schematics::read(bytes.as_slice(), registry) {
            Ok(schematic) => schematics.push(schematic),
            Err(error) => log::warn!("base catalog: `{path}` rejected: {error}"),
        }
    }
    if schematics.is_empty() {
        return None;
    }
    let mut bases = mind_core::ai::BaseRegistry::new();
    bases.load(registry, schematics);
    log::info!(
        "base catalog: {} core part(s), {} part(s), {} resource part(s)",
        bases.cores.len(),
        bases.parts.len(),
        bases.req_parts.len()
    );
    Some(bases)
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
    let (result, rules, pending_buildings, plans) = {
        let mut context = EditorContext::new(&mut grid, registry);
        let (result, rules, plans) = {
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(registry),
                ..SaveReadState::default()
            };
            let result = SaveIo::load_bytes(bytes.as_slice(), &mut state);
            (
                result,
                state.rules.clone(),
                std::mem::take(&mut state.team_plans),
            )
        };
        (
            result,
            rules,
            std::mem::take(&mut context.pending_buildings),
            plans,
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
        plans,
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
/// renderer's `anchor_tile * TILESIZE + Block.offset` quad center (Java
/// `Tile.worldx()` convention; the anchor tile is the centre tile for odd
/// multiblocks).
///
/// Upstream `Player.bestCore()` picks the player team's core; the port's tile
/// model carries no team, so this returns the first core tile scanned.
fn first_core_position(grid: &WorldGrid, content: &ContentRegistry) -> Option<(f32, f32)> {
    let unit = mind_core::config::TILESIZE as f32;
    let is_core = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= grid.width() || y >= grid.height() {
            return false;
        }
        content
            .block(grid.tile(x, y).block)
            .is_some_and(|def| def.kind == BlockKind::CoreBlock)
    };
    grid.iter_row_major().find_map(|(pos, index)| {
        let def = content.block(grid.block_id_at(index))?;
        (def.kind == BlockKind::CoreBlock).then(|| {
            // Java `World.setSectorRules` -> `sector.info.spawnPosition =
            // core.pos()` uses the building's ANCHOR tile (the centre for odd
            // multiblocks) and the player spawns at `tile.worldx()/worldy()`
            // (the tile's corner, not its centre). Map-loaded multiblocks fill
            // the whole footprint (the row-major first tile is the min corner,
            // so the anchor is `pos - size_offset`); tool-placed blocks may
            // store only the anchor tile. The candidate `pos - size_offset`
            // being the same core disambiguates the two conventions.
            let candidate_x = pos.x() as i32 - def.size_offset;
            let candidate_y = pos.y() as i32 - def.size_offset;
            let anchor = if is_core(candidate_x, candidate_y) {
                (candidate_x, candidate_y)
            } else {
                (pos.x() as i32, pos.y() as i32)
            };
            (anchor.0 as f32 * unit, anchor.1 as f32 * unit)
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
    fn best_core_position_uses_the_anchor_tile_corner() {
        let registry = base_registry();
        let Some(core) = registry.block_id("core-shard") else {
            panic!("core-shard missing");
        };
        let mut grid = WorldGrid::new(8, 8);
        grid.fill(BlockId::AIR, BlockId::AIR);
        if grid.set_block(TilePos::new(3, 5), core, 0, 0).is_err() {
            panic!("set_block failed");
        }
        let unit = mind_core::config::TILESIZE as f32;
        // Java's spawn is `tile.worldx()/worldy()` of the anchor tile.
        assert_eq!(
            first_core_position(&grid, &registry),
            Some((3.0 * unit, 5.0 * unit))
        );
    }

    #[test]
    fn best_core_position_is_none_without_cores() {
        let registry = base_registry();
        let mut grid = WorldGrid::new(4, 4);
        grid.fill(BlockId::AIR, BlockId::AIR);
        assert_eq!(first_core_position(&grid, &registry), None);
    }

    /// PAR-0037: Java spawns the player at the core's anchor-tile corner
    /// (`core.pos()` -> `tile.worldx/worldy()`), so a 3x3 `core-shard` placed
    /// with its centre at (129, 55) must report `(129*8, 55*8)` - the
    /// reference Ground Zero `viewpos` is `(1032.0, 440.3)`.
    #[test]
    fn core_position_matches_the_java_spawn_anchor() {
        let registry = base_registry();
        let Some(core) = registry.block_id("core-shard") else {
            panic!("core-shard missing");
        };
        let mut sim = Sim::new(1, 256, 256, BlockId::AIR, BlockId::AIR);
        if let Err(error) = sim.apply(Command::Place {
            x: 129,
            y: 55,
            block: core,
        }) {
            panic!("place core failed: {error}");
        }
        assert_eq!(
            first_core_position(&sim.grid, &registry),
            Some((1032.0, 440.0))
        );
    }

    /// The map-loader convention: a 3x3 footprint filled across tiles
    /// 128-130 x 54-56 (min corner first in row-major) must resolve to the
    /// same anchor `(129, 55)` -> `(1032, 440)`.
    #[test]
    fn map_loaded_footprint_resolves_to_the_anchor_corner() {
        let registry = base_registry();
        let Some(core) = registry.block_id("core-shard") else {
            panic!("core-shard missing");
        };
        let mut grid = WorldGrid::new(256, 256);
        grid.fill(BlockId::AIR, BlockId::AIR);
        for y in 54..=56 {
            for x in 128..=130 {
                if grid.set_block(TilePos::new(x, y), core, 0, 0).is_err() {
                    panic!("set_block failed at ({x},{y})");
                }
            }
        }
        assert_eq!(first_core_position(&grid, &registry), Some((1032.0, 440.0)));
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
