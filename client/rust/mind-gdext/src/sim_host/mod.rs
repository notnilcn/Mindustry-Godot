// SPDX-License-Identifier: GPL-3.0-only

//! `MindSimHost` — owns the `mind-core::Sim`, pumps it at a fixed 60 Hz and
//! exposes the MCP-visible test API (`00_FOUNDATION_IMPLEMENTATION_PLAN.md`
//! §3.5/§7c). Contains no game rules: every mutation goes through `mind_core`.
//!
//! Ported from `core/src/mindustry/ClientLauncher.java` (boot order) and
//! `core/src/mindustry/core/Logic.java` (fixed step pump); the fixed-step
//! accumulator itself lives in `mind_core::sim::FixedStepRunner` (D8).

mod bar_rows;
mod bars;
mod boot;
mod campaign_link;
mod command_panel;
mod commands;
mod config;
mod hover;
mod inspect;
mod io;
mod logic;
mod overlay;
mod plans;
mod player;
mod pump;
mod render_blocks;
mod render_light;
mod render_units;
mod state;
mod types;
mod world_load;
mod world_query;

#[cfg(test)]
mod tests;

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
use crate::wiring;
use mind_core::ai::StanceBits;
use mind_core::audio::{AudioSinkRes, SharedAudioLog};
use mind_core::command::Command;
use mind_core::content::registries::units::parts::DrawPartSpec;
use mind_core::content::registries::units::weapon::WeaponDef;
use mind_core::content::{
    BlockId, BlockKind, ContentRegistry, ContentType, ItemId, MemoryBundle, MemoryUnlockStore,
    UnitCommandId, UnitStanceId, UnitTypeId, content_counts,
};
use mind_core::determinism::SimCommand;
use mind_core::ecs::BuildingComp;
use mind_core::game::rules::Rules;
use mind_core::game::runtime::{CampaignRuntime, SessionSync, sync_session_with_sim};
use mind_core::render::scan::{CameraView, visible_blocks};
use mind_core::scenario::{Scenario, ScenarioPlayer};
use mind_core::sim::{FixedStepRunner, Sim};
use mind_core::weapons::WeaponMount;
use mind_core::world::TilePos;
use mind_core::world::WorldGrid;
use mind_core::world::blocks::heat::HeatState;
use mind_core::world::blocks::power::PowerGrids;
use mind_core::world::modules::{ItemModule, LiquidModule, PowerModule};
use mind_core::world::pos::unpack;

pub use types::{
    BeamLaserLink, BeltItemSlot, BlockStateDraw, BridgeLink, BuilderView, BulletDraw, CrawlDraw,
    FireDraw, LaserLink, LegsDraw, MechDraw, OverlayView, PadDraw, PayloadDraw, PlaceRangeKind,
    PuddleDraw, TreadsDraw, UnitAbilityDraw, UnitBuildDraw, UnitCommandRow, UnitDraw, WeaponDraw,
};
pub(crate) use types::{ConfigOverlay, SelectOverlay};

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
    /// `InputHandler.locked()` mirror (PAR-0100): while set, the player axis is
    /// forced to zero.
    input_locked: bool,
    /// Plan-17 `MindFx` sibling (`../MindFx`); the one-way sim→view tick hook.
    /// Optional: absent in headless/no-FX scenes.
    #[export]
    pub fx: Option<Gd<crate::fx::MindFx>>,
    /// Plan-21 relay queue: commands drained and applied at the next tick
    /// start (never mid-frame). Populated by `MindNet`/MCP via
    /// `enqueue_sim_command`/`apply_sim_command_json`.
    pending_commands: VecDeque<SimCommand>,
    /// Relay mailbox installed by `MindNet` while a match is active (I08 T6):
    /// `submit_player_command` pushes here; the relay drains it each frame.
    relay_outbox: Option<std::sync::Arc<std::sync::Mutex<Vec<SimCommand>>>>,
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
    /// Sibling `World/Camera2D` (`../World/Camera2D`); clicks the GUI did not
    /// claim map to a tile through its `screen_to_tile`.
    #[export]
    pub camera: Option<Gd<MindCamera2D>>,
    /// Sibling `Input`; the player move axis is uploaded to the sim each frame
    /// (`DesktopInput.updateMovement`).
    #[export]
    pub input: Option<Gd<MindInput>>,
    /// Sibling `MindPreview` (`../MindPreview`); bare `load_map_world` names
    /// resolve through its registry rows (`map_path`). Optional: absent in
    /// headless scenes.
    #[export]
    pub preview: Option<Gd<crate::editor::MindPreview>>,
    /// Enemy spawn-overlay count of the last loaded sector grid.
    sector_spawns: i32,
    /// Turret `BaseTurret.range` cache for `block_place_range` (`config_for`
    /// rebuilds the whole config each call; the ghost asks per frame).
    place_range_cache: std::cell::RefCell<std::collections::HashMap<BlockId, Option<f32>>>,
    /// Resume metadata (`wavetime`/`tick`/`stats`/markers) of the last
    /// `load_sector` when it resumed from a save; `MindCampaign.start_sector`
    /// takes it right after `play_sector` (T2).
    sector_resume: Option<mind_core::io::sim_io::SectorResumeMeta>,
    /// Last `grid.limit_area` seen by the pump; a change (rules adoption,
    /// G06 `setrule`) sets `world_dirty` so the renderer rebuilds (T5).
    last_limit_area: Option<(i32, i32, i32, i32)>,
    /// Campaign extras for the next generated numbered sector (H05 T2):
    /// `MindCampaign.prime_sector_load` primes this before `load_sector`;
    /// `load_sector_world` falls back to `game::planet::static_gen_extras`.
    pending_gen_extras: Option<(String, u16, mind_core::maps::planet::SectorGenExtras)>,
    /// Resume source for the next `load_sector` (`Control.playSector`'s
    /// `sector.save` slot): the file `MindCampaign.prime_sector_load` resolved
    /// from the meta association, or an empty path for "no base here".
    /// `None` (never primed) keeps the canonical
    /// `saves/sector-<planet>-<id>.msav` fallback for direct host calls
    /// (CG-SAVE-05).
    pending_sector_save: Option<String>,
    /// The resume file the last `load_sector` was asked to load but could not
    /// decode (`Control.playSector`'s `SaveException` catch): the campaign
    /// clears the association, deletes the slot and reports
    /// `@save.corrupted` instead of resuming a regenerated world (CG-SAVE-06).
    corrupt_sector_save: Option<String>,
    /// Launch payload for the next generated sector (H05 T4): schematic index
    /// (`-1` = planet default) and launch-resource stacks JSON.
    pending_launch: Option<(i32, String)>,
    /// `Universe.updateLoadout(CoreBlock)` override for the next sector
    /// (the Interplanetary Accelerator's launch core, Accelerator.java:277).
    pending_launch_core: Option<String>,
    /// Whether the last `load_sector` placed a generated sector's launch
    /// loadout during the world load (`MindCampaign.start_sector_with_loadout`
    /// skips its own `place_launch_loadout` then; H05 T4).
    generated_loadout_placed: bool,
    /// Last `logic_set_code` failure (`""` when none; H06 T1) — `"toolong:<len>"`
    /// mirrors `LogicBuild.showEditDialog`'s error message.
    logic_last_error: String,
    /// `World.loadMap`'s error key from the last failed `load_map_world`
    /// (`map.invalid`; `None` = the last load succeeded or none ran).
    last_map_error: Option<String>,
    /// Pinned generation seed for the next `load_map_world`
    /// ([`Self::set_next_map_gen_seed`]); `None` draws a fresh one.
    pending_map_gen_seed: Option<u64>,
    /// Content-name snapshot for the display rasterizer's icon lookup (T13);
    /// built on the first `logic_display_sync`.
    display_icon_names: Option<crate::display_context::IconNames>,
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
            input_locked: false,
            fx: None,
            pending_commands: VecDeque::new(),
            relay_outbox: None,
            audio_log: SharedAudioLog::new(),
            sector_rules: None,
            sector_plans: Vec::new(),
            camera: None,
            input: None,
            preview: None,
            sector_spawns: 0,
            place_range_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            sector_resume: None,
            last_limit_area: None,
            pending_gen_extras: None,
            pending_sector_save: None,
            corrupt_sector_save: None,
            pending_launch: None,
            pending_launch_core: None,
            generated_loadout_placed: false,
            logic_last_error: String::new(),
            last_map_error: None,
            pending_map_gen_seed: None,
            display_icon_names: None,
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
        // `DesktopInput.updateMovement`: the full control state (axis, shooting,
        // cursor aim, boost) reaches the player unit each tick; the same axis no
        // longer pans the camera while a player exists
        // (`MindCamera2D::process`).
        let (axis_x, axis_y, shooting, aim_x, aim_y, boosting, mouse_move) = self.player_control();
        self.sim
            .set_player_control(axis_x, axis_y, shooting, aim_x, aim_y, boosting, mouse_move);
        // GAPS #26 host half: `NetServer.clientSnapshot`'s per-frame player
        // view upload (camera/selection/ping) before the logic tick reads it.
        self.push_logic_player_view();
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

        let Some(camera) = self.camera.as_ref() else {
            return;
        };
        let tile = camera
            .bind()
            .screen_to_tile(position.x as f64, position.y as f64);
        let Some((x, y)) = crate::gdutil::coords::tile_i16(tile.x, tile.y) else {
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
            self.mark_world_changed();
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
        // I07 T4: the one content boot path (base content + enabled mods'
        // content/bundles/patches); Java `ClientLauncher.java:166-189`.
        self.content_snapshot = match crate::content_boot::build_registry(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
        ) {
            Ok(registry) => Some(registry),
            Err(error) => {
                log::warn!("content snapshot unavailable: {error}");
                None
            }
        };
        // I07 T4: the mods' asset overlay (bundles now; sprites/sounds T5).
        crate::content_boot::push_asset_overlay();
        // I07 T4: the sim's block name/id table starts from vanilla
        // (`Blocks::new`); mod blocks extend it with identical registry ids so
        // the API/command entry points can address them.
        if let Some(registry) = self.content_snapshot.as_ref()
            && let Err(error) = self.sim.sync_blocks_from_content(registry)
        {
            log::warn!("mod block table sync failed: {error}");
        }

        // Plan-13 M7 + plan 04 §3.10 + plan-18: wire the per-sim host seams
        // (logic globals, IO executor, audio sink). Must be re-run whenever a
        // fresh `Sim` replaces `self.sim`, or queued IO never drains.
        self.install_sim_seams();
        // `InputHandler` world reset: clear the logic cutscene/HUD flags and
        // message timers (`InputHandler.java:921-922`; H06 T3).
        self.sim.reset_logic_presentation();

        // Live gameplay (the default spine world, custom maps and campaign
        // sectors) runs the plan-07 block stack so placed buildings execute their
        // behaviors. Scenario loads keep the frozen P0 path on purpose: the
        // committed checksum goldens are recorded against `Sim::from_scenario`.
        self.install_live_block_runtime();

        // Sibling `Input` owns the binding state; the player move axis is read
        // here each frame and uploaded to the sim (`DesktopInput.updateMovement`).
        // `player.block` is not persisted upstream; a restored block would close
        // the command-mode gate (`command_mode_allowed`), so ordered RTS units
        // could never be selected after a restart (PAR-0140). The selected block
        // starts at AIR and the settings file is still written on exit.
        self.input = wiring::required("MindSimHost", "input", &self.input);
        // `World/Camera2D` maps unclaimed GUI clicks to a tile (required: the
        // scene spine always has one).
        self.camera = wiring::required("MindSimHost", "camera", &self.camera);

        self.capture = pump::parse_capture_args();
        // The STDB connection is owned by the `StdbConnector` autoload (plan 01
        // M5, exactly one pump per process); the sim host no longer connects.
        self.mark_world_changed();

        // Plan-17 sim→view hook: the FX host advances one fixed view tick per
        // sim tick (one-way; the sim never reads the view). Optional: absent in
        // headless/no-FX scenes.
        self.fx = wiring::optional(&self.fx);
        // Bare `load_map_world` names resolve through the map registry; optional
        // (headless/no-preview scenes fall through to the file paths above).
        self.preview = wiring::optional(&self.preview);

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

    /// Emitted once per world load (`WorldLoadEvent`; `World.loadMap` end).
    /// The placement palette resets its category/selection and re-reads the
    /// catalogue (`J/ui/fragments/PlacementFragment.java:78-84`).
    #[signal]
    fn world_loaded();
}

/// Accessors and small shared helpers for the `sim_host` children (H06).
impl MindSimHost {
    /// The sim's ECS world (`self.sim.ecs.0`; `MindWorld`'s only field).
    pub(super) fn world(&self) -> &bevy_ecs::world::World {
        &self.sim.ecs.0
    }

    /// Mutable form of [`Self::world`].
    pub(super) fn world_mut(&mut self) -> &mut bevy_ecs::world::World {
        &mut self.sim.ecs.0
    }

    /// Building entity at `(x, y)`; `None` off the map or on an empty tile.
    pub(super) fn build_at(&self, x: i32, y: i32) -> Option<bevy_ecs::entity::Entity> {
        self.sim
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.sim.grid.tile(x, y).build)
            .flatten()
    }

    /// Live campaign runtime (`None` outside a campaign session).
    pub(super) fn campaign_runtime(&self) -> Option<&CampaignRuntime> {
        self.world().get_resource::<CampaignRuntime>()
    }

    /// Mutable form of [`Self::campaign_runtime`] (bevy's change-detection
    /// guard, which derefs to `&mut CampaignRuntime`).
    pub(super) fn campaign_runtime_mut(
        &mut self,
    ) -> Option<bevy_ecs::change_detection::Mut<'_, CampaignRuntime>> {
        self.sim.ecs.0.get_resource_mut::<CampaignRuntime>()
    }

    /// Camera view covering `rect` grown by `grow` world pixels per side.
    pub(super) fn grown_view(&self, rect: (f32, f32, f32, f32), grow: f32) -> CameraView {
        CameraView {
            x: rect.0 + rect.2 / 2.0,
            y: rect.1 + rect.3 / 2.0,
            w: rect.2 + 2.0 * grow,
            h: rect.3 + 2.0 * grow,
            zoom: 1.0,
            team: self.player_team(),
        }
    }

    /// Marks the world dirty, then emits `state_changed` and `world_changed`.
    ///
    /// `emit_world_changed` only signals while `world_dirty` is set, so the
    /// assignment must stay first.
    pub(super) fn mark_world_changed(&mut self) {
        self.world_dirty = true;
        self.emit_state();
        self.emit_world_changed();
    }
}

/// `Block.offset` world-pixel centre of tile `(x, y)`: `x * TILESIZE + offset`.
///
/// The multiblock-aware variant is [`render_light::building_center`]; this one
/// is the plain per-tile form (no `(size - 1) / 2` term).
pub(super) fn tile_center(x: i32, y: i32, offset: f32) -> (f32, f32) {
    let cx = x as f32 * TILESIZE_F + offset;
    let cy = y as f32 * TILESIZE_F + offset;
    (cx, cy)
}

/// `Block.offset` world-pixel centre of a tile (H01 T1 convention).
const TILESIZE_F: f32 = mind_core::config::TILESIZE as f32;

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
