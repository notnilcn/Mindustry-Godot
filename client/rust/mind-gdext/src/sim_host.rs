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
    /// Unit hit radius (`UnitType.hitSize`); `Units.closestOverlap` adds half.
    pub hit_size: f32,
    /// Mech walk state (`MechComp`); `None` for non-mechs.
    pub mech: Option<MechDraw>,
    /// Live weapon mounts (`UnitWeapons`).
    pub weapons: Vec<WeaponDraw>,
    /// `EntitySeq.0` (view-state key; entities are reused).
    pub id: u64,
    /// `MinerComp.mine_tile` when `MinerComp.mining` (world tile).
    pub mine_tile: Option<(i16, i16)>,
    /// `ItemsComp.item` as `(item name, amount)`.
    pub item: Option<(String, i32)>,
    /// Whether this is the local player's unit.
    pub is_player: bool,
    /// Absolute health (`Health.health`).
    pub health: f32,
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

/// One command-overlay row: `(x, y, hit_size, flying, target_pos)`
/// (`InputHandler.drawCommanded` inputs).
pub type UnitCommandRow = (f32, f32, f32, bool, Option<(f32, f32)>);

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
    /// Outline drawn over the body (`Weapon.top`); non-top weapons draw their
    /// `<name>-outline` sprite under the body (`drawWeaponOutlines`).
    pub top: bool,
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
use mind_core::ecs::BuildingComp;
use mind_core::game::rules::Rules;
use mind_core::game::runtime::{CampaignRuntime, SessionSync, sync_session_with_sim};
use mind_core::render::scan::{CameraView, visible_blocks};
use mind_core::scenario::{Scenario, ScenarioPlayer};
use mind_core::sim::{FixedStepRunner, Sim};
use mind_core::world::TilePos;
use mind_core::world::WorldGrid;
use mind_core::world::blocks::heat::HeatState;
use mind_core::world::blocks::power::PowerGrids;
use mind_core::world::modules::{ItemModule, LiquidModule, PowerModule};
use mind_core::world::pos::unpack;

/// Maximum ticks accepted by one `step()` call (defensive; UI/MCP only).
const MAX_STEP_TICKS: i64 = 1_000_000;

/// Frames to wait before reading the viewport (the texture is one frame late).
const CAPTURE_WARMUP_FRAMES: u32 = 3;

/// One scripted `-- --capture <path>` request.
struct CaptureRequest {
    path: String,
    frames_waited: u32,
}

/// `Block.drawPlace` range-circle kinds (G03 T2): which Arc helper the
/// placement preview uses and which colour applies. The range value comes
/// from `MindSimHost::block_place_range`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceRangeKind {
    /// `BaseTurret.drawPlace` (`BaseTurret.java:84-90`): `Drawf.dashCircle`
    /// in `Pal.placing`.
    Turret,
    /// `PowerNode.drawPlace` (`PowerNode.java:159-173`): `Drawf.circles` in
    /// `Pal.placing` at `laserRange * 8`.
    PowerNode,
    /// `MendProjector`/`OverdriveProjector.drawPlace`: `Drawf.dashCircle` in
    /// the projector's `baseColor`.
    Projector,
}

/// Player-builder draw state (`BuilderComp.drawBuilding`, G03 T3).
pub struct BuilderView {
    /// Player unit position (world px).
    pub x: f32,
    /// Player unit position (world px).
    pub y: f32,
    /// Player unit rotation (degrees).
    pub rotation: f32,
    /// `UnitType.buildBeamOffset`.
    pub beam_offset: f32,
    /// `UnitType.buildRange`.
    pub build_range: f32,
    /// `UnitType.drawBuildBeam`.
    pub draw_beam: bool,
    /// `BuilderComp.updateBuilding`.
    pub update_building: bool,
    /// Live plan queue copy (head first).
    pub plans: Vec<mind_core::sim::builder::UnitBuildPlan>,
}

/// One live bullet for the draw pass (`Groups.bullet`; G03 T4).
#[derive(Clone)]
pub struct BulletDraw {
    /// `EntitySeq` (`Bullet.id`; bevy entities are reused).
    pub id: u64,
    /// World position.
    pub x: f32,
    /// World position.
    pub y: f32,
    /// Rotation in degrees.
    pub rotation: f32,
    /// Ticks lived.
    pub time: f32,
    /// Lifetime in ticks.
    pub lifetime: f32,
    /// Effect parameter (`Bullet.fdata`).
    pub fdata: f32,
    /// Bullet content id.
    pub def: mind_core::content::BulletId,
    /// Owning team (bullets are teamless in the port; kept for parity).
    pub team: u8,
}

/// One power-node laser link (`PowerNode.drawLaser`, G03 T7).
pub struct LaserLink {
    /// Source building centre (world px).
    pub x1: f32,
    /// Source building centre (world px).
    pub y1: f32,
    /// Source `Block.size`.
    pub size1: i32,
    /// Target building centre (world px).
    pub x2: f32,
    /// Target building centre (world px).
    pub y2: f32,
    /// Target `Block.size`.
    pub size2: i32,
    /// Owning graph satisfaction (`setupColor`).
    pub satisfaction: f32,
    /// Source block id (region fallbacks).
    pub block: BlockId,
}

/// One item-bridge link (`ItemBridge.draw`, G03 T7).
pub struct BridgeLink {
    /// Source tile.
    pub x: i32,
    /// Source tile.
    pub y: i32,
    /// Linked tile.
    pub ox: i32,
    /// Linked tile.
    pub oy: i32,
    /// `ItemBridgeBuild.warmup`.
    pub warmup: f32,
    /// `ItemBridgeBuild.time`.
    pub time: f32,
    /// `Block.hasPower` (fade-in only for phase conveyors).
    pub has_power: bool,
    /// Source block id (region names).
    pub block: BlockId,
}

/// Laser/bridge visible-rect growth: the longest link is a surge tower's
/// 40 tiles = 320 px (`Blocks.java:2502`; G03 T7 pitfall).
const POWER_GROW: f32 = 320.0;
/// Bridge links reach `range 4` tiles (`Blocks.java`); 64 px covers it.
const BRIDGE_GROW: f32 = 64.0;
/// Light sources are stamped from outside the view too (`LightRenderer`
/// quads), so grow the collection rect by the largest radius (G03 T12).
const LIGHT_GROW: f32 = 200.0;

/// One `LightRenderer.add` source (G03 T12): world centre, radius, RGBA
/// colour and the per-source opacity the pass multiplies into the alpha.
pub struct LightSource {
    /// World x.
    pub x: f32,
    /// World y.
    pub y: f32,
    /// World-pixel radius.
    pub radius: f32,
    /// RGBA colour `[r, g, b, a]`.
    pub color: [f32; 4],
    /// Accumulated-alpha multiplier (`LightRenderer.add` opacity).
    pub opacity: f32,
}

/// Unpacks a packed `rgba8888` into float RGBA.
fn rgba_f32(packed: u32) -> [f32; 4] {
    [
        ((packed >> 24) & 0xff) as f32 / 255.0,
        ((packed >> 16) & 0xff) as f32 / 255.0,
        ((packed >> 8) & 0xff) as f32 / 255.0,
        (packed & 0xff) as f32 / 255.0,
    ]
}

/// Building sprite centre (`anchor * 8 + Block.offset`; Java `Building.x`).
fn building_center(
    x: i32,
    y: i32,
    def: &mind_core::content::registries::blocks::BlockDef,
) -> (f32, f32) {
    let anchor_x = (x + (def.size - 1) / 2) as f32 * 8.0 + def.offset;
    let anchor_y = (y + (def.size - 1) / 2) as f32 * 8.0 + def.offset;
    (anchor_x, anchor_y)
}

/// Per-building draw state (`Door`/`SwitchBlock`/`Sorter`/`ItemSource`/
/// `Unloader` + cracks/team marks; G03 T8).
pub struct BlockStateDraw {
    /// `DoorState.open`.
    pub door_open: bool,
    /// `Building.enabled`.
    pub enabled: bool,
    /// `SorterBuild.sort_item`.
    pub sort_item: Option<mind_core::content::ItemId>,
    /// `UnloaderBuild.sort_item`.
    pub unloader_item: Option<mind_core::content::ItemId>,
    /// `SandboxState.item` (item source).
    pub source_item: Option<mind_core::content::ItemId>,
    /// `Health.fraction`.
    pub healthf: f32,
    /// `damaged()` = `health < maxHealth - 0.001`.
    pub damaged: bool,
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
    /// Last `logic_set_code` failure (`""` when none; H06 T1) — `"toolong:<len>"`
    /// mirrors `LogicBuild.showEditDialog`'s error message.
    logic_last_error: String,
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
            fx_host: None,
            pending_commands: VecDeque::new(),
            audio_log: SharedAudioLog::new(),
            sector_rules: None,
            sector_plans: Vec::new(),
            input: None,
            sector_spawns: 0,
            place_range_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            sector_resume: None,
            last_limit_area: None,
            logic_last_error: String::new(),
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
        let (axis_x, axis_y, shooting, aim_x, aim_y, boosting) = self.player_control();
        self.sim
            .set_player_control(axis_x, axis_y, shooting, aim_x, aim_y, boosting);
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
        self.input = self.base().try_get_node_as::<MindInput>("../Input");

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

    /// Installs the content-initialized logic globals (`GlobalVars`) on the sim.
    ///
    /// `LAssembler.var` resolves constants from the global `Vars.logicVars`
    /// (`J/logic/LAssembler.java:59-61`), created once in `Vars.init`
    /// (`J/core/Vars.java:369`) and filled with the content by
    /// `ContentLoader.java:107` — long before any map or save is read. Map/save
    /// buildings are decoded by `materialize_map_buildings` (`LogicBuild.read`
    /// -> `read_compressed` -> `update_code`), so this must run BEFORE them or
    /// `@sharded`/`@duo`/`@scatter` become plain null variables (H06 T20).
    /// Idempotent.
    fn install_logic_globals(&mut self) {
        if self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::logic::globals::GlobalVars>()
            .is_some()
        {
            return;
        }
        if let Some(registry) = self.content_snapshot.as_ref() {
            mind_core::logic::globals::GlobalVars::install_world(&mut self.sim.ecs.0, registry);
        }
    }

    /// Wires the host seams that live on the `Sim` value: the content-initialized
    /// logic globals (plan-13 M7), the live-sim IO executor (plan 04 §3.10) and
    /// the audio sink (plan-18).
    ///
    /// `load_scenario`/`load_sector` install a brand-new `Sim`, so this must run
    /// again after every replacement; otherwise `request_save`/`request_load`
    /// queue into an `IoQueue` with no handler and never drain.
    fn install_sim_seams(&mut self) {
        self.install_logic_globals();
        if let Some(registry) = self.content_snapshot.take() {
            // Live world-processor sinks (G06 T4): rules/markers/wave live in
            // the campaign session; the event log + message timers are sim
            // resources (headless=false so messages/cutscenes reach the host).
            match self
                .sim
                .ecs
                .0
                .get_resource_mut::<mind_core::logic::world::LogicWorldState>()
            {
                Some(mut state) => {
                    state.headless = false;
                    state.is_host = true;
                }
                None => {
                    self.sim
                        .ecs
                        .0
                        .insert_resource(mind_core::logic::world::LogicWorldState {
                            headless: false,
                            ..mind_core::logic::world::LogicWorldState::new()
                        });
                }
            }
            if self
                .sim
                .ecs
                .0
                .get_resource::<mind_core::logic::world::MessageState>()
                .is_none()
            {
                self.sim
                    .ecs
                    .0
                    .insert_resource(mind_core::logic::world::MessageState::default());
            }
            if self
                .sim
                .ecs
                .0
                .get_resource::<mind_core::logic::world::LogicContentIndex>()
                .is_none()
            {
                self.sim.ecs.0.insert_resource(
                    mind_core::logic::world::LogicContentIndex::from_content(&registry),
                );
            }
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

        // Combat FX (`Effect.at`/`Effect.shake`/decals) go to `MindFx`'s bus;
        // a fresh `Sim` carries a fresh noop-sink unit runtime, so this runs
        // after every replacement.
        self.install_fx_sink();
    }

    /// Installs the live `MindFx` bus on the sim's unit runtime so every
    /// `FxSink` call reaches the view instead of the noop sink (`MindFx` sits
    /// at `/root/Spine/MindFx`; `FxBus` is `Send + Sync`). No-op when the FX
    /// host or unit runtime is absent.
    fn install_fx_sink(&mut self) {
        let Some(bus) = self
            .base()
            .try_get_node_as::<crate::fx::MindFx>("/root/Spine/MindFx")
            .map(|fx| fx.bind().bus_handle())
        else {
            return;
        };
        if let Some(mut runtime) = self.sim.unit_runtime_mut() {
            runtime.fx = bus;
        }
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
        // PAR-0165 (T7): `apply_command` returns `Ok` even when the live
        // runtime rejected the placement, so read the tile back and only
        // report success when the block really sits anchored at `(x, y)`.
        let placed = applied && self.tile_holds_block(x, y, id);
        if placed {
            self.emit_state();
            self.emit_world_changed();
        }
        placed
    }

    /// Whether `(x, y)` now holds `block` anchored there: the tile's block id
    /// matches and the building entity's center tile is `(x, y)` (`T7`, the
    /// `place_block` readback; PAR-0165).
    fn tile_holds_block(&self, x: i16, y: i16, block: BlockId) -> bool {
        let pos = mind_core::world::TilePos::new(x, y);
        self.sim.grid.block_at(pos) == Some(block)
            && self
                .sim
                .grid
                .entity_at(pos)
                .and_then(|entity| {
                    self.sim
                        .ecs
                        .0
                        .get::<mind_core::entities::comp::Building>(entity)
                })
                .is_some_and(|building| building.tile == pos)
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

    /// Places `block` at `(x, y)` with an explicit rotation (`0..3`) through the
    /// same live-runtime path as [`Self::place_block`]. `Command::Place` always
    /// carries rotation 0, so rotated belts/conduits need this test seam
    /// (MCP §7c has no rotation field).
    #[func]
    pub fn place_block_rot(&mut self, x: i32, y: i32, block: GString, rot: i32) -> bool {
        let name = block.to_string();
        let id = match self.sim.content().id(&name) {
            Ok(id) => id,
            Err(_) => {
                log::warn!("place_block_rot: unknown block `{name}`");
                return false;
            }
        };
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            log::warn!("place_block_rot: ({x}, {y}) does not fit in i16");
            return false;
        };
        let team = self.sim.live_default_team();
        let applied = self
            .sim
            .place_for_team(x, y, id, rot.clamp(0, 3) as u8, team);
        let placed = applied && self.tile_holds_block(x, y, id);
        if placed {
            self.emit_state();
            self.emit_world_changed();
        }
        placed
    }

    /// Placement ghost validity (`Build.validPlace(type, player.team(), x, y,
    /// rot)`, `InputHandler.java:1455`); H08 consumes it. Out-of-range ints and
    /// unknown block names return `false`.
    #[func]
    pub fn valid_place_preview(&self, x: i32, y: i32, block: GString, rotation: i32) -> bool {
        let name = block.to_string();
        let Ok(id) = self.sim.content().id(&name) else {
            log::debug!("valid_place_preview: unknown block `{name}`");
            return false;
        };
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        self.sim
            .valid_place_player(x, y, id, rotation.clamp(0, 3) as u8)
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
                // The staged team (when given) drives the pre-check so enemy
                // staging works; otherwise the live default team is used.
                let team = value.get("team").and_then(|v| v.as_u64()).unwrap_or(0) as u8;
                let check_team = if value.get("team").is_some() {
                    team
                } else {
                    self.sim.live_default_team()
                };
                // PAR-0165 (T7): reject invalid placements at enqueue time so
                // callers get an honest `false` instead of a silently dropped
                // command (`Sim::command` logs rejections at debug level).
                if !self.sim.can_place_for_team(
                    x,
                    y,
                    BlockId::new(block),
                    rotation as u8,
                    check_team,
                ) {
                    log::warn!("apply_sim_command_json: place rejected at ({x}, {y})");
                    return false;
                }
                SimCommand::Place {
                    x,
                    y,
                    block,
                    rotation,
                    team,
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
            // `InputHandler.setUnitStance` (G01 T11): `units` + `stance` id in
            // `content_list("unitStance")` order, `enabled`.
            "unit_stance" => {
                let units: smallvec::SmallVec<[i32; 32]> = value
                    .get("units")
                    .and_then(|v| v.as_array())
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|row| row.as_i64().map(|id| id as i32))
                            .collect()
                    })
                    .unwrap_or_default();
                let stance = value.get("stance").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
                let enabled = value
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                SimCommand::UnitStance {
                    units,
                    stance,
                    enabled,
                }
            }
            // `InputHandler.unitControl`/`unitClear` (G01 T9).
            "unit_control" => SimCommand::UnitControl {
                unit: value.get("unit").and_then(|v| v.as_i64()).map(|v| v as i32),
            },
            "unit_clear" => SimCommand::UnitClear,
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

    /// Debug-only live unit dump: id, type, team, controller kind, position,
    /// rotation, health, `can_shoot` and per-mount fire state. Read-only MCP
    /// probe (`PAR-0088`/`§7`); never saved or synced.
    #[func]
    pub fn unit_debug_json(&self) -> GString {
        GString::from(self.sim.unit_debug_json().as_str())
    }

    /// G01 T16: live command targets for the HUD reticle
    /// (`[{"id":seq,"x":wx,"y":wy,"attack":bool}, ...]`).
    #[func]
    fn command_targets_json(&self) -> GString {
        let rows: Vec<serde_json::Value> = self
            .sim
            .command_targets()
            .into_iter()
            .map(|(id, x, y, attack)| {
                serde_json::json!({
                    "id": id,
                    "x": x,
                    "y": y,
                    "attack": attack,
                })
            })
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| String::from("[]"))
                .as_str(),
        )
    }

    /// G01 T15: drains the per-tick building-damage log as
    /// `[[tile_x, tile_y, team], ...]`; G03 filters by the player team and
    /// forwards to `MindMinimap.add_indicator`.
    #[func]
    fn drain_build_damage_json(&mut self) -> GString {
        let events = self.sim.drain_build_damage_events();
        let rows: Vec<[f64; 3]> = events
            .iter()
            .map(|event| [event.x as f64, event.y as f64, event.team as f64])
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| String::from("[]"))
                .as_str(),
        )
    }

    /// `Teams.bosses`/`GameState.boss()` seam (G01 T19): G03's boss bar reads
    /// count/names/value, G05's guardian hint reads `first`.
    #[func]
    pub fn boss_json(&self) -> GString {
        GString::from(self.sim.boss_json().as_str())
    }

    /// Debug seam (G01 T19): apply a status by name to a unit by `EntitySeq`
    /// (`apply_sim_command_json` has no status op).
    #[func]
    pub fn debug_apply_status(&mut self, seq: i64, status: GString, ticks: f64) -> bool {
        self.sim
            .debug_apply_status(seq, &status.to_string(), ticks as f32)
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

    /// Building state at `(x, y)` (debug seam: `{block, enabled, efficiency,
    /// potential_efficiency, optional_efficiency, time_scale}` plus
    /// `{item, liquid, accumulator, emitted}` for sandbox sources/voids).
    #[func]
    pub fn building_debug(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::<GString, Variant>::new();
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
        let Some(building) = world.get::<mind_core::entities::comp::Building>(entity) else {
            return out;
        };
        {
            let mut put = |key: &str, value: Variant| {
                out.set(&GString::from(key), &value);
            };
            put("block", Variant::from(i64::from(building.block.raw())));
            put("rotation", Variant::from(i64::from(building.rotation)));
            put("enabled", Variant::from(building.enabled));
            if let Some(team) = world.get::<mind_core::entities::comp::TeamComp>(entity) {
                put("team", Variant::from(i64::from(team.team)));
            }
            put("efficiency", Variant::from(f64::from(building.efficiency)));
            put(
                "potential_efficiency",
                Variant::from(f64::from(building.potential_efficiency)),
            );
            put(
                "optional_efficiency",
                Variant::from(f64::from(building.optional_efficiency)),
            );
            put("time_scale", Variant::from(f64::from(building.time_scale)));
            // Per-kind body readback after a load (PAR-0161): drill progress,
            // crafter progress and turret ammo.
            if let Some(state) = world.get::<mind_core::entities::comp::DrillState>(entity) {
                put("drill_progress", Variant::from(f64::from(state.progress)));
                put("drill_warmup", Variant::from(f64::from(state.warmup)));
            }
            if let Some(state) = world.get::<mind_core::entities::comp::CrafterState>(entity) {
                put("crafter_progress", Variant::from(f64::from(state.progress)));
                put("crafter_warmup", Variant::from(f64::from(state.warmup)));
            }
            if let Some(state) =
                world.get::<mind_core::world::blocks::defense::turrets::TurretState>(entity)
            {
                put("turret_total_ammo", Variant::from(state.total_ammo));
            }
            // Fog discovery progress (`Radar.progress`; H09 T3 readback).
            if let Some(state) = world.get::<mind_core::entities::comp::RadarState>(entity) {
                put("radarProgress", Variant::from(f64::from(state.progress)));
            }
            // In-progress construction (`ConstructBuild`; PAR-0098 §2d).
            if let Some(state) = world.get::<mind_core::world::construct::ConstructState>(entity) {
                put(
                    "construct_current",
                    Variant::from(i64::from(state.current.raw())),
                );
                put(
                    "construct_previous",
                    Variant::from(i64::from(state.previous.raw())),
                );
                put(
                    "construct_progress",
                    Variant::from(f64::from(state.progress)),
                );
                put(
                    "construct_deconstruct",
                    Variant::from(state.active_deconstruct),
                );
            }
            // Item module contents for `has_items` blocks (`{item: amount}`).
            if let Some(content) = self.content_snapshot.as_ref()
                && let Some(json) = block_items_json(&self.sim, content, x, y)
            {
                put("items_json", Variant::from(json.as_str()));
            }
            if let Some(state) = world.get::<mind_core::entities::comp::SandboxState>(entity) {
                put(
                    "sandbox_item",
                    state
                        .item
                        .map(|item| Variant::from(i64::from(item.raw())))
                        .unwrap_or(Variant::nil()),
                );
                put(
                    "sandbox_liquid",
                    state
                        .liquid
                        .map(|liquid| Variant::from(i64::from(liquid.raw())))
                        .unwrap_or(Variant::nil()),
                );
                put("accumulator", Variant::from(f64::from(state.accumulator)));
                put("emitted", Variant::from(i64::from(state.emitted)));
            }
        }
        out
    }

    /// Building config readback (`Building.config()`) as JSON, for config-UI
    /// verification (`{"type":"string","value":...}`, `{"type":"item","id":..}`,
    /// `null` for no config).
    #[func]
    pub fn building_config_json(&self, x: i32, y: i32) -> GString {
        let entity = self
            .sim
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.sim.grid.tile(x, y).build)
            .flatten();
        let Some(entity) = entity else {
            return GString::from("null");
        };
        let config = mind_core::world::config::read_config(&self.sim.ecs.0, entity);
        let value = config_to_json(&config);
        GString::from(value.to_string().as_str())
    }

    /// Building config readback as a plain string (`nil` for non-string configs).
    #[func]
    pub fn building_config_string(&self, x: i32, y: i32) -> Variant {
        let entity = self
            .sim
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.sim.grid.tile(x, y).build)
            .flatten();
        let Some(entity) = entity else {
            return Variant::nil();
        };
        match mind_core::world::config::read_config(&self.sim.ecs.0, entity) {
            mind_core::world::config::ConfigValue::String(text) => Variant::from(text.as_str()),
            _ => Variant::nil(),
        }
    }

    /// Packed RGBA color of a `LightBlock` at `(x, y)` (`nil` when absent).
    #[func]
    pub fn light_color(&self, x: i32, y: i32) -> Variant {
        let entity = self
            .sim
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.sim.grid.tile(x, y).build)
            .flatten();
        let Some(entity) = entity else {
            return Variant::nil();
        };
        match self
            .sim
            .ecs
            .0
            .get::<mind_core::world::behavior::light::LightState>(entity)
        {
            Some(state) => Variant::from(i64::from(state.color)),
            None => Variant::nil(),
        }
    }

    /// `Building.enabled` at `(x, y)` (`nil` when the tile has no building).
    #[func]
    pub fn building_enabled(&self, x: i32, y: i32) -> Variant {
        let entity = self
            .sim
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.sim.grid.tile(x, y).build)
            .flatten();
        let Some(entity) = entity else {
            return Variant::nil();
        };
        match self
            .sim
            .ecs
            .0
            .get::<mind_core::entities::comp::Building>(entity)
        {
            Some(building) => Variant::from(building.enabled),
            None => Variant::nil(),
        }
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
        true
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

    /// Normalized processor program text (`LogicDialog.show`:
    /// `LAssembler.read` → `LAssembler.write`; empty string when no processor).
    #[func]
    pub fn logic_get_code(&self, x: i64, y: i64) -> GString {
        let world = &self.sim.ecs.0;
        let Some(e) =
            mind_core::logic::blocks::logic_block::processor_at(world, x as i32, y as i32)
        else {
            return GString::from("");
        };
        let Some(state) = world.get::<mind_core::logic::blocks::LogicBlockState>(e) else {
            return GString::from("");
        };
        let privileged = mind_core::logic::blocks::privileged_of(world, e);
        let text = mind_core::logic::Assembler::read(&state.code, privileged)
            .map(|statements| mind_core::logic::Assembler::write(&statements))
            .unwrap_or_else(|_| state.code.clone());
        GString::from(text.as_str())
    }

    /// Debug seam: writes `code` into the processor at `(x, y)` directly,
    /// bypassing `LogicBuild.accessible()` like the editor (world processors
    /// are not accessible in campaign; every later in-engine check uses them).
    /// Returns false with `logic_last_error() == "toolong:<len>"` past
    /// `LogicBlock.maxCompressedLen` (`showEditDialog`'s size gate).
    #[func]
    pub fn logic_set_code(&mut self, x: i64, y: i64, code: GString) -> bool {
        self.logic_last_error = String::new();
        let code = code.to_string();
        let world = &mut self.sim.ecs.0;
        let Some(e) =
            mind_core::logic::blocks::logic_block::processor_at(world, x as i32, y as i32)
        else {
            self.logic_last_error = "noprocessor".to_owned();
            return false;
        };
        let links = world
            .get::<mind_core::logic::blocks::LogicBlockState>(e)
            .map(|state| state.relative_connections(world, e));
        let Some(links) = links else {
            self.logic_last_error = "noprocessor".to_owned();
            return false;
        };
        let bytes = match mind_core::logic::blocks::logic_block::encode_for_config(&code, &links) {
            Ok(bytes) => bytes,
            Err(len) => {
                self.logic_last_error = format!("toolong:{len}");
                return false;
            }
        };
        let Some(mut state) = world
            .entity_mut(e)
            .take::<mind_core::logic::blocks::LogicBlockState>()
        else {
            return false;
        };
        let result = state.read_compressed(world, e, &bytes, true);
        world.entity_mut(e).insert(state);
        result.is_ok()
    }

    /// Last `logic_set_code` failure (`""` when none).
    #[func]
    pub fn logic_last_error(&self) -> GString {
        GString::from(self.logic_last_error.as_str())
    }

    /// Processor readback (`privileged`/`accessible`/`ipt`/`tag`/`code`/`links`/
    /// `vars`); empty `Dictionary` when no processor is at `(x, y)`.
    #[func]
    pub fn logic_get_state(&self, x: i64, y: i64) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::<GString, Variant>::new();
        let world = &self.sim.ecs.0;
        let Some(e) =
            mind_core::logic::blocks::logic_block::processor_at(world, x as i32, y as i32)
        else {
            return out;
        };
        let Some(snapshot) = mind_core::logic::blocks::logic_block::state_snapshot(world, e) else {
            return out;
        };
        out.set(
            &GString::from("privileged"),
            &snapshot.privileged.to_variant(),
        );
        out.set(
            &GString::from("accessible"),
            &snapshot.accessible.to_variant(),
        );
        out.set(&GString::from("ipt"), &(snapshot.ipt as i64).to_variant());
        out.set(
            &GString::from("tag"),
            &match &snapshot.tag {
                Some(tag) => GString::from(tag.as_str()).to_variant(),
                None => Variant::nil(),
            },
        );
        out.set(
            &GString::from("code"),
            &GString::from(snapshot.code.as_str()).to_variant(),
        );
        let mut links = Array::<Variant>::new();
        for (name, lx, ly, valid) in &snapshot.links {
            let mut row = Dictionary::<GString, Variant>::new();
            row.set(
                &GString::from("name"),
                &GString::from(name.as_str()).to_variant(),
            );
            row.set(&GString::from("x"), &(*lx as i64).to_variant());
            row.set(&GString::from("y"), &(*ly as i64).to_variant());
            row.set(&GString::from("valid"), &valid.to_variant());
            links.push(&row.to_variant());
        }
        out.set(&GString::from("links"), &links.to_variant());
        let mut vars = Array::<Variant>::new();
        for (name, value, type_) in &snapshot.vars {
            let mut row = Dictionary::<GString, Variant>::new();
            row.set(
                &GString::from("name"),
                &GString::from(name.as_str()).to_variant(),
            );
            row.set(
                &GString::from("value"),
                &GString::from(value.as_str()).to_variant(),
            );
            row.set(&GString::from("type"), &GString::from(*type_).to_variant());
            vars.push(&row.to_variant());
        }
        out.set(&GString::from("vars"), &vars.to_variant());
        out
    }

    /// Last 64 drained logic-event kind tags (`LogicWorldEvent::kind`) as a
    /// JSON array (H06 T1 debug probe).
    #[func]
    pub fn logic_event_log_json(&self) -> GString {
        let value: Vec<&str> = self
            .sim
            .logic_event_log
            .iter()
            .map(String::as_str)
            .collect();
        GString::from(
            serde_json::to_string(&value)
                .unwrap_or_else(|_| String::from("[]"))
                .as_str(),
        )
    }

    /// `InputHandler.logicCutscene` state for the camera rig (plain Rust
    /// forwarder; H06 T3).
    pub fn logic_cutscene_state(&self) -> mind_core::logic::world::LogicCutscene {
        self.sim.logic_cutscene()
    }

    /// Takes the pending `CutsceneI` shake request (camera rig poller).
    pub fn take_logic_shake(&mut self) -> Option<(f32, f32)> {
        self.sim.take_logic_shake()
    }

    /// Logic cutscene/HUD probe: `{"active","x","y","speed","zoom","hideHud"}`
    /// (`x`/`y` in world pixels; H06 T3).
    #[func]
    pub fn logic_cutscene_json(&self) -> GString {
        let cutscene = self.sim.logic_cutscene();
        let value = serde_json::json!({
            "active": cutscene.active,
            "x": cutscene.pan.0,
            "y": cutscene.pan.1,
            "speed": cutscene.speed,
            "zoom": cutscene.zoom,
            "hideHud": cutscene.hide_hud,
        });
        GString::from(value.to_string().as_str())
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

    /// G06 T4 verification seam: live campaign rules relevant to world
    /// processors (`waveSending`/`waveTimer`/`mapArea`/flags/wave).
    #[func]
    pub fn logic_rules_probe(&self) -> GString {
        let Some(runtime) = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::runtime::CampaignRuntime>()
        else {
            return GString::from("{}");
        };
        let rules = &runtime.session.rules;
        let value = serde_json::json!({
            "waveSending": rules.wave_sending,
            "waveTimer": rules.wave_timer,
            "waves": rules.waves,
            "wave": runtime.session.wave,
            "wavetime": runtime.session.wavetime,
            "limitMapArea": rules.limit_map_area,
            "limitX": rules.limit_x,
            "limitY": rules.limit_y,
            "limitWidth": rules.limit_width,
            "limitHeight": rules.limit_height,
            "unitCap": rules.unit_cap,
            "flags": rules.objective_flags.iter().cloned().collect::<Vec<_>>(),
        });
        GString::from(value.to_string().as_str())
    }

    /// G06 T4 verification-only seam: set/clear an objective flag (what the
    /// Ground Zero tutorial objectives raise).
    #[func]
    pub fn logic_set_flag(&mut self, name: GString, on: bool) -> bool {
        let Some(mut runtime) = self
            .sim
            .ecs
            .0
            .get_resource_mut::<mind_core::game::runtime::CampaignRuntime>()
        else {
            return false;
        };
        let name = name.to_string();
        if on {
            runtime.session.rules.objective_flags.insert(name);
        } else {
            runtime.session.rules.objective_flags.shift_remove(&name);
        }
        true
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

    /// Whether a *save* is queued (`control.saves.isSaving()`; G03 T9 shows
    /// the small `@saving` label instead of the full loading overlay).
    #[func]
    pub fn io_saving(&self) -> bool {
        self.sim.io_saving()
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

    /// Input-side command-mode fact for `CommandModeObjective` (G08 calls this
    /// from `MindInput` whenever the command-mode selection changes; T4).
    #[func]
    pub fn set_command_mode_satisfied(&mut self, value: bool) {
        self.sim.set_command_mode_satisfied(value);
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
        self.sim.reset_logic_presentation();
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
        // Resume metadata is per load; clear before this load decides.
        self.sector_resume = None;
        // Take the registry out so the generator can borrow it while the sim is
        // rebuilt, then hand it back (content boot is expensive; never drop it).
        let Some(mut registry) = self.content_snapshot.take() else {
            log::warn!("load_sector: content registry unavailable");
            return false;
        };
        const SECTOR_SIZE: i32 = 128;
        // `Control.playSector`: an existing sector save is the resume source.
        let save_path = sector_save_path(planet_name, sector as u16);
        let resume_file = use_save && std::path::Path::new(&save_path).is_file();
        let loaded = if resume_file {
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
        let (grid, rules, pending_buildings, plans, save) = match loaded {
            Some(loaded) => (
                loaded.grid,
                loaded.rules,
                loaded.pending_buildings,
                loaded.plans,
                loaded.save,
            ),
            None => match generated {
                Some(generated) => (
                    generated.grid,
                    Some(generated.rules),
                    Vec::new(),
                    Vec::new(),
                    None,
                ),
                None => {
                    self.content_snapshot = Some(registry);
                    log::warn!("load_sector: no map or generator for planet `{planet_name}`");
                    return false;
                }
            },
        };
        // A resume save only counts when the save file supplied the world (a
        // failed decode falls back to the generator; a preset map is not a
        // resume).
        let resume_from_save = resume_file && save.is_some();
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
        // H06 T20: resolve `@content` constants from the point the globals
        // exist — before the map/save processors are decoded below — exactly
        // like `Vars.logicVars` at content init (`Vars.java:369`).
        self.install_logic_globals();
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
        // T2: the save's world entities (units/fires/puddles) restore into the
        // live runtime; the resume metadata (wavetime/tick/stats/markers) is
        // handed to `MindCampaign` after `play_sector`.
        if let Some(save) = save.as_ref() {
            self.sim.apply_decoded_entities(save);
        }
        if resume_from_save && let Some(save) = save.as_ref() {
            if let Some(tick) = mind_core::io::sim_io::meta_tick(&save.tags) {
                self.sim.state.tick = tick;
            }
            self.sector_resume = Some(mind_core::io::sim_io::SectorResumeMeta {
                wavetime: save
                    .tags
                    .get("wavetime")
                    .and_then(|v| v.parse::<f32>().ok()),
                tick: mind_core::io::sim_io::meta_tick(&save.tags),
                stats: save.stats.clone(),
                markers: save.markers.clone(),
            });
        }
        // The fresh `Sim` carries no IO executor; re-wire the seams so a queued
        // `request_save`/`request_load` drains at the next `IoSet` boundary.
        self.install_sim_seams();
        self.sim.reset_logic_presentation();
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

    /// Takes the last `load_sector`'s resume metadata (`wavetime`/`tick`/
    /// `stats`/markers) when it resumed from a save; `None` for fresh loads,
    /// generated worlds and failed decodes (T2).
    pub fn take_sector_resume(&mut self) -> Option<mind_core::io::sim_io::SectorResumeMeta> {
        self.sector_resume.take()
    }

    /// Re-derives the world-side state from `rules` (map-area limit rect +
    /// border darkness, `World.isInMapArea`/`getDarkness`) and marks the world
    /// dirty (T5; G06's `setrule` path goes through
    /// `CampaignRuntime.rules_world_dirty` instead).
    pub fn apply_world_rules(&mut self, rules: &Rules) {
        self.sim.apply_rules_to_world(rules);
        self.world_dirty = true;
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
        // The live runtime carries rotation/team and routes through the real
        // tile ops (`SimCommand::Place`); the P0 command has no rotation and is
        // kept for runtime-less worlds (T18).
        if self.sim.has_block_runtime() {
            let rotation = rot.rem_euclid(4);
            self.pending_commands.push_back(SimCommand::Place {
                x,
                y,
                block: block.raw(),
                rotation,
                team: self.player_team(),
                player: None,
            });
            self.world_dirty = true;
            return true;
        }
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

    /// Whether player construction plans are accepted (`Sim::player_can_build`;
    /// T18 schematic placement).
    pub fn player_can_build(&self) -> bool {
        self.sim.player_can_build()
    }

    /// Queues a player construction plan (`Sim::queue_player_place`; T18).
    pub fn queue_player_place_at(&mut self, x: i16, y: i16, block: BlockId, rot: i8) -> bool {
        let rotation = u8::try_from(rot).unwrap_or(0).rem_euclid(4);
        self.sim.queue_player_place(x, y, block, rotation)
    }

    /// Queues a player construction plan carrying a config (G08 T5: schematic
    /// copies, rebuild ghosts, line plan configs, `nextConfig`).
    pub fn queue_player_place_config(
        &mut self,
        x: i16,
        y: i16,
        block: BlockId,
        rot: u8,
        config: mind_core::world::config::ConfigValue,
    ) -> bool {
        self.sim.queue_player_place_config(x, y, block, rot, config)
    }

    /// `Block.rotate` for a block id (G08 T6b plan rotation math).
    pub fn block_rotates(&self, id: BlockId) -> bool {
        mind_core::world::blocks::payloads::payload_block::block_rotates(&self.sim.ecs.0, id)
    }

    /// `Schematics.create` over the live world (F-drag / break-cut copy): the
    /// selection rect plus the paste cursor resolve to select plans
    /// (`Schematics.toPlans`, priority-sorted).
    pub fn copy_schematic_plans(
        &self,
        start: (i32, i32),
        end: (i32, i32),
        cursor: (i32, i32),
    ) -> Vec<mind_core::input::ClientPlan> {
        let (x1, y1) = start;
        let (x2, y2) = end;
        let (cx, cy) = cursor;
        let Some(content) = self.content_registry() else {
            return Vec::new();
        };
        let world = mind_core::game::schematics::EcsSchematicWorld {
            grid: &self.sim.grid,
            world: &self.sim.ecs.0,
            content,
            team: Some(self.player_team()),
        };
        let schem = mind_core::game::schematics::Schematics::new().create(
            &world,
            content,
            x1,
            y1,
            x2,
            y2,
            mind_core::game::rules::MAX_SCHEMATIC_SIZE,
        );
        mind_core::game::schematics::plans_for(&schem, cx, cy, true, content)
            .iter()
            .map(mind_core::input::ClientPlan::from_build_plan)
            .collect()
    }

    /// `InputHandler.rebuildArea` (B drag): re-queues team ghost plans in the
    /// rect. Returns the queued count.
    pub fn queue_team_rebuild(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> usize {
        self.sim.queue_team_rebuild(x1, y1, x2, y2)
    }

    /// `InputHandler.removeSelection` plan half (right drag): drops queued
    /// player plans and team ghosts overlapping the clamped rect.
    pub fn remove_plans_in(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, max_length: i32) {
        self.sim.remove_plans_in(x1, y1, x2, y2, max_length);
    }

    /// Playtest seam: the player team's ghost rebuild plans
    /// (`[{x,y,block,rotation}]`).
    #[func]
    pub fn team_ghost_plans_json(&self) -> GString {
        let team = self.player_team();
        let plans: Vec<serde_json::Value> = self
            .sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .and_then(|campaign| {
                campaign
                    .session
                    .teams
                    .get_or_null(mind_core::ecs::TeamId(team))
            })
            .map(|data| {
                data.plans
                    .iter()
                    .map(|plan| {
                        serde_json::json!({
                            "x": plan.x,
                            "y": plan.y,
                            "block": self.sim.block_name_of(plan.block),
                            "rotation": plan.rotation,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        GString::from(serde_json::Value::Array(plans).to_string().as_str())
    }

    /// Playtest seam (H01 T3): kills the unit with `EntitySeq == seq`; the
    /// next tick's `resolve_deaths` runs the normal death path (respawn timer,
    /// wreck for flyers, kill stats).
    #[func]
    pub fn debug_kill_unit(&mut self, seq: i64) -> bool {
        self.sim.debug_kill_unit(seq)
    }

    /// Playtest seam: kills a building (`Health.health = 0`); the next tick's
    /// `resolve_deaths` destroys it and pushes the ghost plan.
    #[func]
    pub fn debug_kill_building(&mut self, x: i32, y: i32) -> bool {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        let Some(entity) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return false;
        };
        let Some(mut health) = self
            .sim
            .ecs
            .0
            .get_mut::<mind_core::entities::comp::Health>(entity)
        else {
            return false;
        };
        health.health = 0.0;
        true
    }

    /// Playtest seam: force-spawns the player unit without the sector env gate
    /// (`CoreBlock.requestSpawn` refuses alpha when `rules.env` wrongly carries
    /// scorching — G01's recorded divergence). Lets input-path probes run on
    /// Ground Zero; the real spawn stays `spawn_player_unit()`.
    #[func]
    pub fn debug_spawn_player(&mut self, name: GString, team: i64, x: f32, y: f32) -> bool {
        self.sim
            .spawn_player_unit(&name.to_string(), team.max(0) as u8, x, y, 90.0)
            .is_some()
    }

    /// `MapObjectives.CommandModeObjective` input read (G08 T15): whether any
    /// selected unit holds a command order (`CommandAI.hasCommand()`).
    pub fn any_unit_has_command(&self, ids: &[i32]) -> bool {
        self.sim.any_unit_has_command(ids)
    }

    /// `ObjectiveInputState.command_mode_satisfied` (G04 T4/G08 T15).
    pub fn command_mode_satisfied(&self) -> bool {
        self.sim
            .ecs
            .0
            .get_resource::<mind_core::game::runtime::ObjectiveInputState>()
            .is_some_and(|state| state.command_mode_satisfied)
    }

    /// Verify seam: the `ObjectiveInputState.command_mode_satisfied` flag.
    #[func]
    pub fn command_mode_objective(&self) -> bool {
        self.command_mode_satisfied()
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
    pub fn best_core_position_px(&self) -> Option<(f32, f32)> {
        first_core_position(&self.sim.grid, self.content_snapshot.as_ref()?)
    }

    /// [`best_core_position_px`](Self::best_core_position_px) for GDScript;
    /// `(-1, -1)` before content boot or on a core-less map.
    #[func]
    pub fn best_core_position(&self) -> Vector2 {
        self.best_core_position_px()
            .map(|(x, y)| Vector2::new(x, y))
            .unwrap_or(Vector2::new(-1.0, -1.0))
    }

    /// Block name of the player's best core (`Player.bestCore().block.name`,
    /// the `block-<name>-full` region the landing cutscene draws); empty before
    /// content boot or on a core-less map.
    pub fn best_core_block(&self) -> GString {
        let Some(content) = self.content_snapshot.as_ref() else {
            return GString::new();
        };
        self.sim
            .grid
            .iter_row_major()
            .find_map(|(_, index)| {
                let def = content.block(self.sim.grid.block_id_at(index))?;
                (def.kind == BlockKind::CoreBlock).then(|| def.name.clone())
            })
            .map(|name| GString::from(name.as_str()))
            .unwrap_or_default()
    }

    /// Binding-driven control state from the sibling `Input`:
    /// `(axis_x, axis_y, shooting, aim_x, aim_y, boosting)`.
    /// `MindInput.player_control` shares the `move_x`/`move_y` axis; with a
    /// player unit it drives the unit instead of the camera
    /// (`DesktopInput.updateMovement`).
    fn player_control(&self) -> (f32, f32, bool, f32, f32, bool) {
        if self.input_locked {
            // `InputHandler.locked()`: movement and firing are gated off
            // (PAR-0100).
            return (0.0, 0.0, false, 0.0, 0.0, false);
        }
        let Some(input) = self.input.clone() else {
            return (0.0, 0.0, false, 0.0, 0.0, false);
        };
        input.bind().player_control()
    }

    /// `InputHandler.locked()` push from `MindInput` (PAR-0100): while set, the
    /// player axis is forced to zero.
    #[func]
    pub fn set_input_locked(&mut self, locked: bool) {
        self.input_locked = locked;
        if locked {
            self.sim
                .set_player_control(0.0, 0.0, false, 0.0, 0.0, false);
        }
    }

    /// Player unit world position (`Player` mirror) for the camera follow.
    pub fn player_unit_position(&self) -> Option<(f32, f32)> {
        self.sim.player_unit_position()
    }

    /// `BuilderComp.activelyBuilding` for the player unit: the `canShoot` gate
    /// the world input checks before latching `player.shooting`.
    pub fn player_actively_building(&self) -> bool {
        self.sim.player_actively_building()
    }

    /// `tryDropItems` deposit check (`InputHandler.java:2286-2308`):
    /// `Some(true)` when the building at the tile accepts the held stack,
    /// `None` when Java takes the `Call.dropItem` branch.
    pub fn player_can_deposit_at(&self, x: i32, y: i32) -> Option<bool> {
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        (self
            .sim
            .player_deposit_accepted(mind_core::world::TilePos::new(x, y))
            > 0)
        .then_some(true)
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
        // `CoreBlock.playerSpawn` spawns on `rules.defaultTeam`
        // (`Team.sharded` = 1 for the campaign, not `TeamId::SHARDED`) with
        // the best core's `unitType` (`CoreBlock.java:95-125`).
        let team = self
            .last_sector_rules()
            .map_or(1, |rules| rules.default_team);
        // `PlayerComp.team` is set before the spawn attempt so a failed first
        // spawn (no supported core yet) is retried by
        // `update_player_respawn` every 60 ticks (PlayerComp.java:44-48).
        self.sim.set_player_team(team);
        let spawned = self.sim.spawn_player_at_best_core(team).is_some();
        if spawned {
            log::info!("spawn_player_unit: core unit team {team} spawned");
        } else {
            log::warn!("spawn_player_unit: no supported core in the loaded sector");
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
        use mind_core::entities::comp::unit::comp::StatusComp;
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
        // `HudFragment.addInfoTable` (`:1205-1231`): applied status effects
        // with their remaining duration (view-only entries, G03 T9).
        let statuses: Vec<serde_json::Value> = entity
            .and_then(|e| world.get::<StatusComp>(e))
            .map(|comp| {
                comp.statuses
                    .iter()
                    .map(|entry| {
                        let name = self
                            .content_snapshot
                            .as_ref()
                            .and_then(|content| content.status(entry.effect))
                            .map(|def| def.name.clone())
                            .unwrap_or_default();
                        serde_json::json!({
                            "name": name,
                            "duration": entry.duration,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        dict.set("dead", dead);
        dict.set("healthf", if dead { 0.0 } else { healthf });
        dict.set("type", &GString::from(type_name.as_str()));
        dict.set(
            "statuses",
            &GString::from(serde_json::Value::Array(statuses).to_string().as_str()),
        );
        dict
    }

    // ---- G03 T9 HUD seams ----

    /// Player-team cores `[{x, y, health, max}]` in row-major order (G03 T9):    /// the core-attack banner sums the healths and pans to the core whose
    /// health dropped last. Java fires `Trigger.teamCoreDamage` on
    /// `CoreBuild.damage` (`CoreBlock.java:842-848`); the port diffs these
    /// values between 5 Hz refreshes (documented deviation).
    #[func]
    pub fn player_core_healths_json(&self) -> GString {
        use mind_core::entities::comp::health::Health;
        let team = self.player_team();
        let Some(content) = self.content_snapshot.as_ref() else {
            return GString::from("[]");
        };
        let mut rows: Vec<serde_json::Value> = Vec::new();
        let mut seen = Vec::new();
        let unit = mind_core::config::TILESIZE as f32;
        for (pos, index) in self.sim.grid.iter_row_major() {
            let Some(def) = content.block(self.sim.grid.block_id_at(index)) else {
                continue;
            };
            if def.kind != BlockKind::CoreBlock {
                continue;
            }
            let Some(entity) = self.sim.grid.tile(pos.x() as i32, pos.y() as i32).build else {
                continue;
            };
            if seen.contains(&entity) {
                continue;
            }
            if self.building_team(pos.x() as i32, pos.y() as i32) != Some(team) {
                continue;
            }
            seen.push(entity);
            let anchor_x = (pos.x() as i32 - def.size_offset) as f32 * unit + def.offset;
            let anchor_y = (pos.y() as i32 - def.size_offset) as f32 * unit + def.offset;
            let (health, max) = self
                .sim
                .ecs
                .0
                .get::<Health>(entity)
                .map(|health| (health.health, health.max_health))
                .unwrap_or((0.0, 0.0));
            rows.push(serde_json::json!({
                "x": anchor_x,
                "y": anchor_y,
                "health": health,
                "max": max,
            }));
        }
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// `WaveSpawner.playerNear()` (`WaveSpawner.java:58-60`, G03 T9): spawns
    /// exist, the player unit is alive, a spawn tile is within
    /// `rules.dropZoneRadius` of the player and the player team is not the
    /// wave team.
    #[func]
    pub fn player_near_drop_zone(&self) -> bool {
        let spawns: Vec<(i32, i32)> = self
            .sim
            .unit_runtime()
            .map(|runtime| {
                runtime
                    .wave_spawner
                    .spawns
                    .iter()
                    .map(|pos| (pos.x() as i32, pos.y() as i32))
                    .collect()
            })
            .unwrap_or_default();
        let rules = self.render_rules_or_default();
        player_near_drop_zone_predicate(
            &spawns,
            self.sim.player_unit_position(),
            self.player_team(),
            rules.wave_team,
            rules.drop_zone_radius,
        )
    }

    /// Live sector rules when available (campaign session first, then the last
    /// `load_sector` rules), else defaults.
    fn render_rules_or_default(&self) -> Rules {
        self.live_rules().unwrap_or_default()
    }

    /// Live sector rules for the renderer's per-frame feed (G03 T12):
    /// campaign session first, else the last `load_sector` rules.
    pub fn live_rules(&self) -> Option<Rules> {
        self.sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| runtime.session.rules.clone())
            .or_else(|| self.sector_rules.clone())
    }

    /// `LightRenderer` sources in the (grown) camera rect (G03 T12): cores
    /// (`CoreBlock.java:158,739-741`), illuminators (`LightBlock.java:40,105,
    /// 126-129`), static `emitLight` blocks (`Block.lightRadius` default 60),
    /// units when `rules.unitLight` (`UnitType.java:954,1800-1804`) and bullets
    /// (`BulletType.drawLight` defaults `max(18, hitSize*5)`,
    /// `Pal.powerLight`, 0.3 — `BulletDef` has no light fields yet).
    pub fn light_sources(&self, rect: (f32, f32, f32, f32)) -> Vec<LightSource> {
        use mind_core::combat::bullet::Bullet;
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::UnitTypeComp;
        use mind_core::render::draw_meta::BlockDrawMeta;
        use mind_core::render::drawf::pal;
        use mind_core::world::behavior::light::LightState;
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let rules = self.render_rules_or_default();
        let view = CameraView {
            x: rect.0 + rect.2 / 2.0,
            y: rect.1 + rect.3 / 2.0,
            w: rect.2 + 2.0 * LIGHT_GROW,
            h: rect.3 + 2.0 * LIGHT_GROW,
            zoom: 1.0,
            team: self.player_team(),
        };
        let mut out = Vec::new();
        for tile in visible_blocks(&self.sim.grid, content, &view) {
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            let (cx, cy) = building_center(tile.x, tile.y, def);
            match def.kind {
                BlockKind::CoreBlock => {
                    // `CoreBlock.java:739-741`: `0.65 + absin(time, 20, 0.1)`.
                    let ticks = godot::classes::Time::singleton().get_ticks_msec() as f32 * 0.06;
                    out.push(LightSource {
                        x: cx,
                        y: cy,
                        radius: 30.0 + 20.0 * def.size as f32,
                        color: pal::ACCENT,
                        opacity: 0.65 + mind_core::render::drawf::absin(ticks, 20.0, 0.1),
                    });
                }
                BlockKind::LightBlock => {
                    let Some(entity) = self.sim.grid.tile(tile.x, tile.y).build else {
                        continue;
                    };
                    let state = self.sim.ecs.0.get::<LightState>(entity);
                    let color = state.map_or(0xffd3_7fff_u32, |s| s.color);
                    let smooth = state.map_or(1.0, |s| s.smooth_time).min(2.0);
                    out.push(LightSource {
                        x: cx,
                        y: cy,
                        // `illuminator` radius 140 * 2.5 (per-block radius is
                        // not in `BlockDef` yet; G03 T12 note).
                        radius: 350.0 * smooth,
                        color: rgba_f32(color),
                        opacity: 0.75,
                    });
                }
                _ if BlockDrawMeta::from_def(def).emit_light => out.push(LightSource {
                    x: cx,
                    y: cy,
                    radius: 60.0,
                    color: [1.0, 1.0, 1.0, 1.0],
                    opacity: 1.0,
                }),
                _ => {}
            }
        }
        if rules.unit_light
            && let Some(runtime) = self.sim.unit_runtime()
        {
            for &entity in &runtime.units {
                let Some(pos) = self.sim.ecs.0.get::<Pos>(entity) else {
                    continue;
                };
                let Some(type_comp) = self.sim.ecs.0.get::<UnitTypeComp>(entity) else {
                    continue;
                };
                let Some(def) = content.unit(type_comp.type_id) else {
                    continue;
                };
                let radius = if def.light_radius > 0.0 {
                    def.light_radius
                } else {
                    60.0f32.max(def.hit_size * 2.3)
                };
                let opacity = if def.light_opacity > 0.0 {
                    def.light_opacity
                } else {
                    0.6
                };
                let c = def.light_color;
                out.push(LightSource {
                    x: pos.x,
                    y: pos.y,
                    radius,
                    color: [c.r, c.g, c.b, c.a],
                    opacity,
                });
            }
        }
        if let Some(runtime) = self.sim.unit_runtime() {
            for &entity in &runtime.bullets {
                let Some(pos) = self.sim.ecs.0.get::<Pos>(entity) else {
                    continue;
                };
                let Some(bullet) = self.sim.ecs.0.get::<Bullet>(entity) else {
                    continue;
                };
                if bullet.finished() {
                    continue;
                }
                out.push(LightSource {
                    x: pos.x,
                    y: pos.y,
                    radius: 18.0f32.max(bullet.hit_size * 5.0),
                    color: pal::POWER_LIGHT,
                    opacity: 0.3,
                });
            }
        }
        out
    }

    /// `rules.borderDarkness` for the darkness/minimap passes (G03 T11):
    /// live campaign rules, else the last `load_sector` rules, else `true`.
    pub fn border_darkness_enabled(&self) -> bool {
        self.sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| runtime.session.rules.border_darkness)
            .or_else(|| {
                self.sector_rules
                    .as_ref()
                    .map(|rules| rules.border_darkness)
            })
            .unwrap_or(true)
    }

    /// Player-team colour as packed `rgba8888` (landing flame circles,
    /// `CoreBlock.java:505-512`; G03 T18).
    #[func]
    pub fn player_team_color(&self) -> i64 {
        i64::from(crate::render::overlays::team_color(self.player_team()))
    }

    // ---- G03 T7 power/bridge link seams (Rust-only) ----

    /// Power-node laser links in the (grown) visible rect (`PowerNode.draw`,
    /// `PowerNode.java:496-512`): each node pair once (the other node's entity
    /// index must be below ours), derelicts skipped.
    pub fn power_lasers(&self, rect: (f32, f32, f32, f32)) -> Vec<LaserLink> {
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let view = CameraView {
            x: rect.0 + rect.2 / 2.0,
            y: rect.1 + rect.3 / 2.0,
            w: rect.2 + 2.0 * POWER_GROW,
            h: rect.3 + 2.0 * POWER_GROW,
            zoom: 1.0,
            team: self.player_team(),
        };
        let mut out = Vec::new();
        for tile in visible_blocks(&self.sim.grid, content, &view) {
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            if def.kind != BlockKind::PowerNode {
                continue;
            }
            let Some(entity) = self.sim.grid.tile(tile.x, tile.y).build else {
                continue;
            };
            let Some(module) = self.sim.ecs.0.get::<PowerModule>(entity) else {
                continue;
            };
            let satisfaction = self
                .sim
                .ecs
                .0
                .get_resource::<PowerGrids>()
                .and_then(|grids| grids.graph(module.graph))
                .map(|graph| graph.get_satisfaction())
                .unwrap_or(1.0);
            let (x1, y1) = building_center(tile.x, tile.y, def);
            for packed in &module.links {
                let (lx, ly) = unpack(*packed);
                if !self.sim.grid.tiles.in_bounds(lx as i32, ly as i32) {
                    continue;
                }
                let Some(other) = self.sim.grid.entity_at(TilePos::new(lx, ly)) else {
                    continue;
                };
                if other == entity {
                    continue;
                }
                if self.building_team(lx as i32, ly as i32) == Some(0) {
                    continue;
                }
                let Some(other_def) = self
                    .sim
                    .ecs
                    .0
                    .get::<BuildingComp>(other)
                    .and_then(|comp| content.block(comp.block))
                else {
                    continue;
                };
                // Each node pair once (`link.id >= id` skip on the other node).
                if other_def.kind == BlockKind::PowerNode && other.index() >= entity.index() {
                    continue;
                }
                let (x2, y2) = building_center(lx as i32, ly as i32, other_def);
                out.push(LaserLink {
                    x1,
                    y1,
                    size1: def.size,
                    x2,
                    y2,
                    size2: other_def.size,
                    satisfaction,
                    block: tile.block,
                });
            }
        }
        out
    }

    /// Item-bridge links in the (grown) visible rect (`ItemBridge.draw`,
    /// `ItemBridge.java:384-432`).
    pub fn bridge_links(&self, rect: (f32, f32, f32, f32)) -> Vec<BridgeLink> {
        use mind_core::world::blocks::distribution::item_bridge::ItemBridgeBuild;
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let view = CameraView {
            x: rect.0 + rect.2 / 2.0,
            y: rect.1 + rect.3 / 2.0,
            w: rect.2 + 2.0 * BRIDGE_GROW,
            h: rect.3 + 2.0 * BRIDGE_GROW,
            zoom: 1.0,
            team: self.player_team(),
        };
        let mut out = Vec::new();
        for tile in visible_blocks(&self.sim.grid, content, &view) {
            let Some(def) = content.block(tile.block) else {
                continue;
            };
            if !matches!(
                def.kind,
                BlockKind::ItemBridge | BlockKind::BufferedItemBridge
            ) {
                continue;
            }
            let Some(entity) = self.sim.grid.tile(tile.x, tile.y).build else {
                continue;
            };
            let Some(bridge) = self.sim.ecs.0.get::<ItemBridgeBuild>(entity) else {
                continue;
            };
            if bridge.link < 0 {
                continue;
            }
            let (lx, ly) = unpack(bridge.link);
            if !self.sim.grid.tiles.in_bounds(lx as i32, ly as i32) {
                continue;
            }
            let Some(other) = self.sim.grid.entity_at(TilePos::new(lx, ly)) else {
                continue;
            };
            let linked_same = self
                .sim
                .ecs
                .0
                .get::<BuildingComp>(other)
                .is_some_and(|comp| comp.block == tile.block);
            if !linked_same {
                continue;
            }
            out.push(BridgeLink {
                x: tile.x,
                y: tile.y,
                ox: lx as i32,
                oy: ly as i32,
                warmup: bridge.warmup,
                time: bridge.time,
                has_power: def.has_power,
                block: tile.block,
            });
        }
        out
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

    /// The player unit's carried stack as `(item name, amount)` (`unit.stack`).
    pub fn player_unit_stack(&self) -> Option<(String, i32)> {
        let (item, amount) = self.sim.player_unit_stack()?;
        let name = self
            .sim
            .unit_runtime()
            .and_then(|runtime| runtime.content.item(item))
            .map(|def| def.name.clone())?;
        Some((name, amount))
    }

    /// Playtest seam: the player unit's stack as `{"item":..,"amount":..}` or
    /// `{}` when empty.
    #[func]
    pub fn player_stack_json(&self) -> GString {
        match self.player_unit_stack() {
            Some((item, amount)) => GString::from(
                serde_json::json!({
                    "item": item,
                    "amount": amount,
                })
                .to_string()
                .as_str(),
            ),
            None => GString::from("{}"),
        }
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

    /// Debug/playtest seam: adds `amount` of `item` to the building at (x, y)
    /// (clamped to `Block.itemCapacity`); returns the amount accepted.
    #[func]
    pub fn debug_add_building_items(&mut self, x: i32, y: i32, item: GString, amount: i64) -> i64 {
        let Ok(amount) = i32::try_from(amount) else {
            return 0;
        };
        let Some(entity) = self.sim.grid.tiles.getn(x, y).and_then(|tile| tile.build) else {
            return 0;
        };
        let Some(item) = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.item_id(&item.to_string()))
        else {
            return 0;
        };
        let block = self
            .sim
            .ecs
            .0
            .get::<mind_core::entities::comp::Building>(entity)
            .map(|building| building.block);
        let capacity = block
            .and_then(|block| {
                self.sim
                    .ecs
                    .0
                    .get_resource::<mind_core::world::block::BlockTable>()
                    .and_then(|table| table.get(block).map(|inst| inst.def.item_capacity))
            })
            .unwrap_or(0);
        match self.sim.ecs.0.get_mut::<ItemModule>(entity) {
            Some(mut module) => i64::from(module.add(item, amount, capacity)),
            None => 0,
        }
    }

    /// `Schematics.placeLaunchLoadout` (`game/Schematics.java:465-469`) for the
    /// launcher branch: replaces the default team's core at its tile with the
    /// chosen loadout schematic (core placed first, non-core footprints cleared
    /// - `placeLoadout(..., check = true)`), then stocks the new core with
    /// `stacks_json` (`[{item, amount}]`, the launch resources).
    #[func]
    pub fn place_launch_loadout(&mut self, schematic_index: i32, stacks_json: GString) -> bool {
        use mind_core::game::schematics::Schematics;
        if schematic_index < 0 {
            return false;
        }
        let Some(registry) = self.content_snapshot.as_ref() else {
            return false;
        };
        // Java's `schematicDirectory` is `dataDirectory/schematics`; `user://`
        // globalizes to the same data root the campaign facade loads from.
        let paths = mind_core::io::fs::Paths::new(
            ProjectSettings::singleton()
                .globalize_path("user://")
                .to_string(),
        );
        let mut schematics = Schematics::new();
        if let Err(error) = schematics.load(
            &mind_core::io::fs::NativeFs,
            &paths.root().join("schematics"),
            registry,
        ) {
            log::warn!("place_launch_loadout: schematic load failed: {error}");
        }
        let Some(schem) = schematics.all.get(schematic_index as usize).cloned() else {
            log::warn!("place_launch_loadout: no schematic at index {schematic_index}");
            return false;
        };
        let team = self.player_team();
        let Some((core_x, core_y)) = self.player_core_tile(team) else {
            log::warn!("place_launch_loadout: no default-team core to replace");
            return false;
        };
        let cores = self.sim.place_loadout(&schem, core_x, core_y, team, true);
        let Some(entity) = cores.first().copied() else {
            log::warn!("place_launch_loadout: schematic placement produced no core");
            return false;
        };
        // `placeLaunchLoadout`: `tile.build.items.add(universe.getLaunchResources())`
        // on the freshly placed core (empty inventory).
        let stacks: Vec<mind_core::io::json::content_serde::JsonItemStack> =
            serde_json::from_str(&stacks_json.to_string()).unwrap_or_default();
        let capacity = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::blocks::storage::core_block::CoreBuild>(entity)
            .map(|core| core.storage_capacity)
            .unwrap_or(i32::MAX);
        if let Some(mut module) = self.sim.ecs.0.get_mut::<ItemModule>(entity) {
            module.clear();
            for stack in &stacks {
                let Some(name) = stack.item.as_deref() else {
                    continue;
                };
                if let Some(item) = registry.item_id(name) {
                    module.add(item, stack.amount, capacity);
                }
            }
        }
        self.world_dirty = true;
        self.emit_state();
        self.emit_world_changed();
        log::info!(
            "place_launch_loadout: `{}` core at ({core_x}, {core_y}) stocked from {} stack(s)",
            schem.name(),
            stacks.len()
        );
        true
    }

    /// The default team's core anchor tile in the live world (`Player.bestCore`
    /// position; `BuildingComp.pos` is the anchor tile).
    fn player_core_tile(&self, team: u8) -> Option<(i16, i16)> {
        let content = self.content_snapshot.as_ref()?;
        self.sim
            .ecs
            .entities_by_seq()
            .into_iter()
            .find_map(|(_, _, comp)| {
                (comp.team.0 == team
                    && content
                        .block(comp.block)
                        .is_some_and(|def| def.kind == BlockKind::CoreBlock))
                .then_some((comp.pos.0, comp.pos.1))
            })
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

    /// Clears the player unit's queued build plans (`Binding.clearBuilding`;
    /// PAR-0098 §2a).
    #[func]
    pub fn clear_player_build_plans(&mut self) {
        self.sim.clear_player_build_plans();
    }

    /// `HintsFragment` poll (`HintsFragment.java:74-102`): event sets + the
    /// per-frame predicate inputs for the hint driver (T4a/T4b).
    #[func]
    pub fn hint_state_json(&self) -> GString {
        GString::from(
            hint_state_value(
                &self.sim,
                self.content_snapshot.as_ref(),
                self.player_team(),
            )
            .to_string()
            .as_str(),
        )
    }

    /// Inserts a hint event by name (G05 T4b). `derelictmouse` is host-detected
    /// (`HintsFragment.java:100-102`, `Trigger.derelictMouse`); unknown names
    /// are ignored.
    #[func]
    pub fn hint_event(&mut self, name: GString) {
        if name == "derelictmouse" {
            self.sim.hint_events.events.insert("derelictmouse");
        }
    }

    /// `BuilderComp.updateBuilding` (`InputHandler.update`; PAR-0098 §2b).
    #[func]
    pub fn set_player_building_paused(&mut self, paused: bool) {
        self.sim.set_player_building_paused(paused);
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
                                    top: weapon.top,
                                    recoil: mount.map(|mount| mount.recoil).unwrap_or(0.0),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let id = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::ecs::EntitySeq>(*entity)
                    .map(|seq| seq.0)
                    .unwrap_or(0);
                let mine_tile = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::MinerComp>(*entity)
                    .filter(|miner| miner.mining)
                    .and_then(|miner| miner.mine_tile.map(|pos| (pos.x(), pos.y())));
                let item = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::ItemsComp>(*entity)
                    .and_then(|items| items.item)
                    .and_then(|(item, amount)| {
                        self.content_snapshot
                            .as_ref()
                            .and_then(|content| content.item(item))
                            .map(|item| (item.name.clone(), amount))
                    });
                let is_player = self.sim.player_unit() == Some(*entity);
                let health = self
                    .sim
                    .ecs
                    .0
                    .get::<Health>(*entity)
                    .map(|health| health.health)
                    .unwrap_or(0.0);
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
                    hit_size: def.hit_size,
                    mech,
                    weapons,
                    id,
                    mine_tile,
                    item,
                    is_player,
                    health,
                })
            })
            .collect()
    }

    /// Plan-18 sim audio log (`MindAudio` drains it; Rust-only seam).
    pub fn audio_log(&self) -> SharedAudioLog {
        self.audio_log.clone()
    }

    // ---- G07 audio/weather seams ----

    /// T5: live ambient sources for `MindAudio`'s 20 Hz poll (Rust-only).
    pub fn ambient_snapshot(&self, out: &mut mind_core::audio::AmbientSnapshot) {
        let Some(content) = self.sim.block_runtime().map(|runtime| runtime.content()) else {
            out.entries.clear();
            return;
        };
        mind_core::audio::ambient::collect_ambient(&self.sim.ecs.0, content, out);
    }

    /// T6: per-frame `control.sound.loop` sources (Rust-only).
    pub fn audio_frame_loops(&self, out: &mut Vec<mind_core::audio::frame::LoopSource>) {
        let Some(content) = self.sim.block_runtime().map(|runtime| runtime.content()) else {
            out.clear();
            return;
        };
        let infinite = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::runtime::CampaignRuntime>()
            .is_some_and(|campaign| campaign.session.rules.infinite_resources);
        mind_core::audio::frame::collect_frame_loops(
            &self.sim.ecs.0,
            content,
            self.sim.tick_count() as f32,
            infinite,
            out,
        );
    }

    /// T6: turret `SoundLoop` inputs (Rust-only).
    pub fn audio_turret_loops(&self, out: &mut Vec<mind_core::audio::frame::TurretLoopSource>) {
        let Some(content) = self.sim.block_runtime().map(|runtime| runtime.content()) else {
            out.clear();
            return;
        };
        mind_core::audio::frame::collect_turret_loops(&self.sim.ecs.0, content, out);
    }

    /// T7: in-game music context state (rules/wave/boss/core hp; Rust-only).
    pub fn audio_game_state(&self) -> mind_core::audio::frame::AudioGameState {
        let Some(content) = self.sim.block_runtime().map(|runtime| runtime.content()) else {
            return Default::default();
        };
        mind_core::audio::frame::read_game_state(&self.sim.ecs.0, content)
    }

    /// T9: weather rules entries + active runtime state (playtest readback).
    #[func]
    pub fn weather_json(&self) -> GString {
        let content = self.sim.block_runtime().map(|runtime| runtime.content());
        let entries: Vec<serde_json::Value> = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::runtime::CampaignRuntime>()
            .map(|campaign| {
                campaign
                    .session
                    .rules
                    .weather
                    .iter()
                    .map(|entry| {
                        serde_json::json!({
                            "weather": entry.weather,
                            "cooldown": entry.cooldown,
                            "minFrequency": entry.min_frequency,
                            "maxFrequency": entry.max_frequency,
                            "minDuration": entry.min_duration,
                            "maxDuration": entry.max_duration,
                            "always": entry.always,
                            "intensity": entry.intensity,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut active: Vec<(u64, serde_json::Value)> = self
            .sim
            .ecs
            .0
            .iter_entities()
            .filter_map(|entity| {
                let state = entity.get::<mind_core::entities::comp::weather::WeatherRuntime>()?;
                let name = content
                    .and_then(|content| content.weather(state.weather))
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                let seq = entity
                    .get::<mind_core::ecs::EntitySeq>()
                    .map_or(u64::MAX, |seq| seq.0);
                Some((
                    seq,
                    serde_json::json!({
                        "weather": name,
                        "intensity": state.intensity,
                        "opacity": state.opacity,
                        "life": state.life,
                        "windX": state.wind_x,
                        "windY": state.wind_y,
                    }),
                ))
            })
            .collect();
        active.sort_by_key(|(seq, _)| *seq);
        let active: Vec<serde_json::Value> = active.into_iter().map(|(_, value)| value).collect();
        let value = serde_json::json!({
            "entries": entries,
            "active": active,
        });
        GString::from(value.to_string().as_str())
    }

    /// T9 test seam: force a weather (`Call.createWeather`), same shape as
    /// `weather_system`'s spawn. `false` when the name is unknown or a weather
    /// of that type is already active.
    #[func]
    pub fn debug_create_weather(&mut self, name: GString, intensity: f32, duration: f32) -> bool {
        let Some(weather) = self
            .sim
            .block_runtime()
            .map(|runtime| runtime.content())
            .and_then(|content| content.weather_by_name(&name.to_string()))
            .map(|def| def.id)
        else {
            log::warn!("debug_create_weather: unknown weather `{name}`");
            return false;
        };
        if self.sim.ecs.0.iter_entities().any(|entity| {
            entity
                .get::<mind_core::entities::comp::weather::WeatherRuntime>()
                .is_some_and(|state| state.weather == weather)
        }) {
            return false;
        }
        let Some(mut runtime) = self
            .sim
            .ecs
            .0
            .remove_non_send::<mind_core::sim::unit_runtime::UnitRuntime>()
        else {
            return false;
        };
        let angle = runtime
            .rng
            .range(mind_core::determinism::RngStream::Fx, 0.0, 360.0)
            .to_radians();
        let seq = runtime.seq;
        runtime.seq = seq.wrapping_add(1);
        let mut state =
            mind_core::entities::comp::weather::WeatherRuntime::new(weather, intensity, duration);
        state.wind_x = angle.cos();
        state.wind_y = angle.sin();
        self.sim.ecs.0.spawn((
            state,
            mind_core::entities::comp::WeatherState,
            mind_core::ecs::EntitySeq(seq),
        ));
        self.sim.ecs.0.insert_non_send(runtime);
        true
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

    /// Item counts of the building at `(x, y)` as JSON (`{item-name: amount}`),
    /// `{}` when the tile holds no item-capable building. MCP-facing wrapper
    /// over [`Self::block_items_json`] (T0 inventory playtest seam).
    #[func]
    pub fn building_items_json(&self, x: i32, y: i32) -> GString {
        GString::from(
            self.block_items_json(x, y)
                .unwrap_or_else(|| "{}".to_string())
                .as_str(),
        )
    }

    /// `Units.anyEntities(tile)` for the door tap gate (`Door.java:148-154`;
    /// T13): whether a ground unit overlaps the building at `(x, y)`.
    #[func]
    pub fn building_has_unit_overlap(&self, x: i32, y: i32) -> bool {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        if !self.sim.grid.tiles.in_bounds(x, y) {
            return false;
        }
        let pos = mind_core::world::TilePos::new(tx, ty);
        let Some(block) = self.sim.grid.block_at(pos) else {
            return false;
        };
        let Some(content) = self.content_snapshot.as_ref() else {
            return false;
        };
        mind_core::sim::builder::unit_overlap_at(&self.sim.ecs.0, content, pos, block)
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
        // A rules adoption or mid-match `setrule` changed the map-area rect
        // (T5): mark the world dirty so the renderer rebuilds.
        if self.sim.grid.limit_area != self.last_limit_area {
            self.last_limit_area = self.sim.grid.limit_area;
            self.world_dirty = true;
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

    /// Belt items at `(x, y)`: `(rotation, [(item, xs, ys); len])` for a
    /// `Conveyor`/`ArmoredConveyor` (`Conveyor.draw` item loop). `None` when
    /// the tile holds no belt. Rust-only seam for the block renderer.
    pub fn conveyor_items(&self, x: i32, y: i32) -> Option<(u8, Vec<BeltItemSlot>)> {
        let entity = self.tile_at(x, y)?.build?;
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

    /// Belt sprite state at `(x, y)`: `(blendbits, sclx, scly, frame)` for a
    /// `Conveyor`/`ArmoredConveyor` (`ConveyorBuild.draw` / `Autotiler`
    /// blending cached by the sim on proximity updates). `None` when the tile
    /// holds no belt. Rust-only seam for the block renderer.
    pub fn belt_draw(&self, x: i32, y: i32) -> Option<(u8, f32, f32, i32)> {
        let entity = self.tile_at(x, y)?.build?;
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(self.sim.grid.tile(x, y).block)?;
        let speed =
            mind_core::world::blocks::distribution::ConveyorBehavior::belt_speed(&def.name)?;
        let world = &self.sim.ecs.0;
        let (blend_bits, sclx, scly, clog_heat) = {
            let belt =
                world.get::<mind_core::world::blocks::distribution::ConveyorBuild>(entity)?;
            (
                belt.blend_bits,
                f32::from(belt.blend_sclx),
                f32::from(belt.blend_scly),
                belt.clog_heat,
            )
        };
        let building = world.get::<mind_core::entities::comp::Building>(entity)?;
        let frame = mind_core::render::autotile::conveyor_frame(
            self.sim.tick_count() as f32,
            speed,
            building.efficiency,
            building.enabled,
            clog_heat,
        );
        Some((blend_bits, sclx, scly, frame))
    }

    /// Drill draw state at `(x, y)`: `timeDrilled` (`Drill.DrillBuild.draw`
    /// rotator angle = `timeDrilled * rotateSpeed`; `Drill.rotateSpeed = 2`).
    /// `None` when the tile holds no drill. Rust-only seam for the block
    /// renderer.
    pub fn drill_draw(&self, x: i32, y: i32) -> Option<f32> {
        let entity = self.tile_at(x, y)?.build?;
        self.sim
            .ecs
            .0
            .get::<mind_core::entities::comp::DrillState>(entity)
            .map(|state| state.time_drilled)
    }

    /// Turret draw state at `(x, y)`: `(rotation degrees, recoil offset x,
    /// recoil offset y, heat)` (`DrawTurret.draw`: `recoilOffset =
    /// trns(rotation, -pow(curRecoil, recoilPow) * recoil)`, barrel rotation
    /// `drawrot() = rotation - 90`). `None` without a turret.
    pub fn turret_draw(&self, x: i32, y: i32) -> Option<(f32, f32, f32, f32)> {
        let entity = self.tile_at(x, y)?.build?;
        let state = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::blocks::defense::turrets::TurretState>(entity)?;
        let len = -state.cur_recoil.max(0.0).powf(state.config.recoil_pow) * state.config.recoil;
        let (dx, dy) = mind_core::render::conveyor_items::trns(state.rotation, len, 0.0);
        Some((state.rotation, dx, dy, state.heat))
    }

    /// Command-overlay rows for the selected unit ids
    /// (`InputHandler.drawCommanded`; T16/PAR-0157):
    /// `(x, y, hit_size, flying, target_pos)` where `target_pos` is
    /// `CommandAiState.target_pos` (`None` without a move command). Rust-only
    /// seam for the overlay pass.
    pub fn unit_command_view(&self, ids: &[i32]) -> Vec<UnitCommandRow> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::UnitTypeComp;
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let wanted: std::collections::HashSet<i32> = ids.iter().copied().collect();
        let mut out = Vec::new();
        for &entity in &runtime.units {
            let Ok(entity_ref) = self.sim.ecs.0.get_entity(entity) else {
                continue;
            };
            let Some(seq) = entity_ref.get::<mind_core::ecs::EntitySeq>() else {
                continue;
            };
            if !wanted.contains(&(seq.0 as i32)) {
                continue;
            }
            let (Some(pos), Some(type_comp)) =
                (entity_ref.get::<Pos>(), entity_ref.get::<UnitTypeComp>())
            else {
                continue;
            };
            let Some(def) = content.unit(type_comp.type_id) else {
                continue;
            };
            let target = self
                .sim
                .unit_command_state(seq.0 as i32)
                .and_then(|state| state.target_pos);
            out.push((pos.x, pos.y, def.hit_size, def.flying, target));
        }
        out
    }

    /// Force-projector domes: `(x, y, real radius, sides, rotation degrees,
    /// team, hit)` for every unbroken projector (`ForceProjector.drawShield`,
    /// `ForceProjector.java:308-330`). Rust-only seam for the shield pass
    /// (T13/PAR-0129).
    pub fn force_shields(&self) -> Vec<(f32, f32, f32, u8, f32, u8, f32)> {
        use mind_core::world::blocks::defense::shields::ForceProjectorState;
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(state) = entity.get::<ForceProjectorState>() else {
                continue;
            };
            if state.broken {
                continue;
            }
            let Some(building) = entity.get::<mind_core::entities::comp::Building>() else {
                continue;
            };
            let Some(comp) = entity.get::<mind_core::ecs::BuildingComp>() else {
                continue;
            };
            let Some(def) = content.block(building.block) else {
                continue;
            };
            // `realRadius()` (the state method mirrors the Java formula).
            let radius = state.real_radius();
            if radius <= 0.001 {
                continue;
            }
            out.push((
                f32::from(building.tile.x()) * 8.0 + def.offset,
                f32::from(building.tile.y()) * 8.0 + def.offset,
                radius,
                state.sides.clamp(3, 300) as u8,
                state.shield_rotation,
                comp.team.0,
                state.hit,
            ));
        }
        out
    }

    /// Unit shield bubbles: `(x, y, radius, alpha, team, hit)` for every unit
    /// with a `ShieldComp` (`UnitType.drawShield`: radius `hitSize * 1.3`,
    /// colour `shieldColor.lerp(white, clamp(hitTime/2)).a(0.7 * shieldAlpha)`;
    /// the port has no `hitTime`, so hit is 0). Rust-only seam (T13).
    pub fn unit_shields(&self) -> Vec<(f32, f32, f32, f32, u8, f32)> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::{ShieldComp, UnitTypeComp};
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(_shield) = entity.get::<ShieldComp>() else {
                continue;
            };
            let Some(pos) = entity.get::<Pos>() else {
                continue;
            };
            let Some(type_comp) = entity.get::<UnitTypeComp>() else {
                continue;
            };
            let Some(def) = content.unit(type_comp.type_id) else {
                continue;
            };
            let team = entity
                .get::<mind_core::entities::comp::TeamComp>()
                .map(|comp| comp.team)
                .unwrap_or(0);
            out.push((
                pos.x,
                pos.y,
                def.hit_size * 1.3,
                0.55, // `shieldAlpha` default (`UnitType.shieldAlpha`)
                team,
                0.0,
            ));
        }
        out
    }

    /// F6 status markers (`BuildingComp.drawStatus`/`status()`): one row per
    /// player-team building with a non-empty `consumes` —
    /// `(center tile x, center tile y, world x, world y, size, java_color)`.
    /// Rust-only seam for the status pass (T14/PAR-0097).
    pub fn building_statuses(&self) -> Vec<(i32, i32, f32, f32, i32, u32)> {
        use mind_core::world::status::BlockStatus;
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let team = self.player_team();
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(building) = entity.get::<mind_core::entities::comp::Building>() else {
                continue;
            };
            let Some(comp) = entity.get::<mind_core::ecs::BuildingComp>() else {
                continue;
            };
            if comp.team.0 != team {
                continue;
            }
            let Some(def) = content.block(building.block) else {
                continue;
            };
            if def.consumes.is_empty() {
                continue;
            }
            // `BlockStatus.forBlock`: `!enabled` -> logicDisable, efficiency
            // `<= 0` -> noInput, else active. Java's pulse condition
            // `((state.tick / 30) % 1) < efficiency` is always true (integer
            // modulo 1), so the marker is simply `active`
            // (`BuildingComp.java:638-652`).
            let status = if !building.enabled {
                BlockStatus::LogicDisable
            } else if building.efficiency <= 0.0 {
                BlockStatus::NoInput
            } else {
                BlockStatus::Active
            };
            let size = def.size.max(1);
            // `Building.tile` is the center tile; `Block.offset` recentres the
            // even-size sprites (same convention as `blocks.rs`).
            let cx = f32::from(building.tile.x()) * 8.0 + def.offset;
            let cy = f32::from(building.tile.y()) * 8.0 + def.offset;
            out.push((
                i32::from(building.tile.x()),
                i32::from(building.tile.y()),
                cx,
                cy,
                size,
                status.java_color(),
            ));
        }
        out
    }

    /// Cursor hover at world pixels (`PlacementFragment.hovered`):
    /// `Units.closestOverlap(player.team(), Core.input.mouseWorldX/Y, 5f,
    /// u -> !u.isLocal() && u.displayable())` — the distance test adds half
    /// the hit size, and the cursor position (not the tile centre) is used.
    #[func]
    pub fn hover_info_world(&self, wx: f32, wy: f32) -> Dictionary<GString, Variant> {
        let size = mind_core::config::TILESIZE as f32;
        let team = self.player_team();
        let own = self.sim.player_unit_position();
        let unit = self
            .unit_draws_full()
            .into_iter()
            .filter(|unit| unit.team == team)
            .filter(|unit| own != Some((unit.x, unit.y)))
            .find(|unit| (unit.x - wx).hypot(unit.y - wy) <= 5.0 + unit.hit_size / 2.0)
            .map(|unit| unit.name);
        if let Some(name) = unit {
            let mut out = Dictionary::new();
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
        // `world.tileWorld`/`screenToTile` fallback (Java recomputes the tile
        // from the mouse world position).
        self.hover_tile(
            ((wx + size / 2.0) / size).floor() as i32,
            ((wy + size / 2.0) / size).floor() as i32,
        )
    }

    /// Hover target at tile `(x, y)` (`PlacementFragment.hovered`): a unit
    /// within 5 world px of the tile centre, else the tile's building, else the
    /// tile's displayable drop (`Tile.displayable`/`Tile.display`).
    /// `{kind, name, icon, label, team}`; empty when nothing is hovered.
    #[func]
    pub fn hover_info(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::new();
        let size = mind_core::config::TILESIZE as f32;
        let cx = x as f32 * size;
        let cy = y as f32 * size;
        // `Units.closestOverlap(player.team(), mouseWorld, 5f, ...)`: a unit
        // under the cursor wins over the tile.
        // Only the player's team, never the player's own unit
        // (`u -> !u.isLocal() && u.displayable()`).
        let team = self.player_team();
        let own = self.sim.player_unit_position();
        let unit = self
            .unit_draws_full()
            .into_iter()
            .filter(|unit| unit.team == team)
            .filter(|unit| own != Some((unit.x, unit.y)))
            .find(|unit| (unit.x - cx).hypot(unit.y - cy) <= 5.0)
            .map(|unit| unit.name);
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
        // `world.tileWorld` is null off the map handled by `hover_tile`.
        self.hover_tile(x, y)
    }

    /// Tile hover rows only (`hover_info` minus the unit test).
    fn hover_tile(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::new();
        let Some(content) = self.content_snapshot.as_ref() else {
            return out;
        };
        // `world.tileWorld` is null off the map: no hover info.
        if !self.sim.grid.tiles.in_bounds(x, y) {
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
        // `OreBlock.getDisplayName` is the drop item's name, but the icon stays
        // the ore block's own `uiIcon` (`Block.getDisplayIcon`).
        if displayed.kind == BlockKind::OreBlock
            && let Some(item_def) = displayed.item_drop.and_then(|item| content.item(item))
        {
            out.set("name", &GString::from(item_def.name.as_str()).to_variant());
            out.set(
                "icon",
                &GString::from(format!("block-{}-ui", displayed.name).as_str()).to_variant(),
            );
            out.set(
                "label",
                &GString::from(format!("item.{}.name", item_def.name).as_str()).to_variant(),
            );
            return out;
        }
        // `Floor.getDisplayIcon/Name`: a liquid floor shows its liquid.
        if let Some(liquid) = displayed.liquid_drop.and_then(|id| content.liquid(id)) {
            out.set("name", &GString::from(liquid.name.as_str()).to_variant());
            out.set(
                "icon",
                &GString::from(format!("liquid-{}-ui", liquid.name).as_str()).to_variant(),
            );
            out.set(
                "label",
                &GString::from(format!("liquid.{}.name", liquid.name).as_str()).to_variant(),
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

    /// Tile `(x, y)`; `None` off the map (`world.build`
    /// returns null out of bounds). Every coordinate-taking render/HUD seam
    /// goes through this: `WorldGrid::tile` panics (debug_assert) off-map.
    fn tile_at(&self, x: i32, y: i32) -> Option<&mind_core::world::Tile> {
        if !self.sim.grid.tiles.in_bounds(x, y) {
            return None;
        }
        Some(self.sim.grid.tile(x, y))
    }

    /// Building rotation (`Building.rotation`, quarter turns) at `(x, y)`;
    /// `0` without a building. Rust-only seam for rotated block sprites.
    pub fn building_rotation(&self, x: i32, y: i32) -> u8 {
        self.tile_at(x, y)
            .and_then(|tile| tile.build)
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
        let entity = self.tile_at(x, y)?.build?;
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
        // Off-map coordinates from MCP/input must not panic (`Tiles::get`
        // indexes unchecked).
        let Some(tile) = self.sim.grid.tiles.getn(x, y) else {
            return d;
        };
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
        // `Rules.limitMapArea` play area (`World.getDarkness` border half);
        // `null` when unrestricted.
        match self.sim.grid.limit_area {
            Some((lx, ly, lw, lh)) => {
                let mut area = PackedInt32Array::new();
                area.push(lx);
                area.push(ly);
                area.push(lw);
                area.push(lh);
                d.set("limit_area", &area.to_variant());
            }
            None => d.set("limit_area", &Variant::nil()),
        }
        d
    }

    /// Construct block state at `(x, y)` as `(current block, progress,
    /// deconstructing)` (`ConstructBuild`; render seam).
    pub fn construct_at(&self, x: i32, y: i32) -> Option<(BlockId, f32, bool)> {
        let entity = self.tile_at(x, y)?.build?;
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
        let entity = self.tile_at(x, y)?.build?;
        self.sim
            .ecs
            .0
            .get::<mind_core::ecs::BuildingComp>(entity)
            .map(|comp| comp.team.0)
    }

    /// [`building_team`](Self::building_team) for GDScript (`-1` = no building;
    /// G05 T4b `derelictmouse` probe).
    #[func]
    pub fn building_team_at(&self, x: i32, y: i32) -> i64 {
        self.building_team(x, y)
            .map(|team| team as i64)
            .unwrap_or(-1)
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

    /// `SectorInfo.write()`'s core half (`SectorInfo.java:165-196`): clears the
    /// default team's first core module and sets each record stack, clamped to
    /// `CoreBuild.storage_capacity`. Rust-only seam for
    /// `MindCampaign::start_sector_with_loadout`.
    pub fn write_record_items_to_core(
        &mut self,
        team: u8,
        items: &indexmap::IndexMap<String, i32>,
    ) -> bool {
        let Some(content) = self.content_snapshot.as_ref() else {
            return false;
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
            return false;
        };
        let capacity = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::blocks::storage::core_block::CoreBuild>(entity)
            .map(|core| core.storage_capacity)
            .unwrap_or(0);
        let Some(mut module) = self.sim.ecs.0.get_mut::<ItemModule>(entity) else {
            return false;
        };
        module.clear();
        for (name, amount) in items {
            if let Some(item) = content.item_id(name) {
                module.set(item, (*amount).clamp(0, capacity));
            }
        }
        true
    }

    /// Launch pods in flight (`LaunchPayloadState`; H04 T6 seam for H03's
    /// `LaunchPayloadComp.draw`: `fin = time/lifetime`, region `<block>-pod`).
    #[func]
    pub fn launch_payloads_json(&mut self) -> GString {
        let rows = mind_core::game::launch_payload::payload_rows(&mut self.sim.ecs.0);
        let mapped: Vec<String> = rows
            .into_iter()
            .map(|(id, x, y, time, lifetime, block, team)| {
                let block = self
                    .content_snapshot
                    .as_ref()
                    .and_then(|content| content.block(block))
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                serde_json::json!({
                    "id": id,
                    "x": x,
                    "y": y,
                    "time": time,
                    "lifetime": lifetime,
                    "block": block,
                    "team": team,
                })
                .to_string()
            })
            .collect();
        GString::from(format!("[{}]", mapped.join(",")).as_str())
    }

    /// Landing pads (`LandingPadState`; H04 T7 seam for H03's pod + cooldown
    /// draw). `id` is the entity index; `config`/`arriving` are item names.
    #[func]
    pub fn landing_pads_json(&mut self) -> GString {
        let rows = mind_core::game::landing_pad::landing_pad_rows(&mut self.sim.ecs.0);
        let item_name = |id: Option<u16>| -> Option<String> {
            id.and_then(|id| {
                self.content_snapshot
                    .as_ref()
                    .and_then(|content| content.item(mind_core::content::ItemId::new(id)))
                    .map(|item| item.name.clone())
            })
        };
        let mapped: Vec<String> = rows
            .into_iter()
            .map(
                |(id, x, y, block, config, arriving, arriving_timer, cooldown)| {
                    let block = self
                        .content_snapshot
                        .as_ref()
                        .and_then(|content| content.block(block))
                        .map(|def| def.name.clone())
                        .unwrap_or_default();
                    serde_json::json!({
                        "id": id,
                        "x": x,
                        "y": y,
                        "block": block,
                        "config": item_name(config),
                        "arriving": item_name(arriving),
                        "arrivingTimer": arriving_timer,
                        "cooldown": cooldown,
                    })
                    .to_string()
                },
            )
            .collect();
        GString::from(format!("[{}]", mapped.join(",")).as_str())
    }

    // ---- G03 T2 placement preview seams (Rust-only) ----

    /// `InputHandler.validPlace` for the placement-preview ghost. Rust-only;
    /// preview only, no side effects (G03 T2).
    pub fn preview_can_place(&self, x: i32, y: i32, block: BlockId, rot: u8) -> bool {
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        self.sim.preview_can_place(x, y, block, rot)
    }

    /// `Block.drawPlace` range circle for `block` (G03 T2): `(radius world px,
    /// kind, rgba8888 colour)`. `None` = the block draws no range circle.
    ///
    /// Turret ranges come from the ported `TurretConfig`
    /// (`BaseTurret.java:84-90`); power-node `laserRange` (6/15/40) and the
    /// projector table are transcribed from `Blocks.java:2483-2504,1921-1958`.
    pub fn block_place_range(&self, block: BlockId) -> Option<(f32, PlaceRangeKind, u32)> {
        use mind_core::render::drawf::{self, pal};
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(block)?;
        if matches!(
            def.kind,
            BlockKind::MendProjector | BlockKind::OverdriveProjector
        ) {
            const PROJECTOR_RANGES: [(&str, f32, u32); 4] = [
                ("mender", 40.0, 0x84f4_91ff),
                ("mend-projector", 85.0, 0x84f4_91ff),
                ("overdrive-projector", 80.0, 0xfeb3_80ff),
                ("overdrive-dome", 200.0, 0xfeb3_80ff),
            ];
            return PROJECTOR_RANGES
                .iter()
                .find(|(name, ..)| *name == def.name)
                .map(|(_, range, color)| (*range, PlaceRangeKind::Projector, *color));
        }
        let accent = drawf::to_bits(pal::ACCENT);
        let laser_range = match def.name.as_str() {
            "power-node" => 6.0,
            "power-node-large" => 15.0,
            "surge-tower" => 40.0,
            _ => 0.0,
        };
        if laser_range > 0.0 {
            return Some((laser_range * 8.0, PlaceRangeKind::PowerNode, accent));
        }
        let cached = self.place_range_cache.borrow().get(&block).copied();
        let range = match cached {
            Some(range) => range,
            None => {
                // Turret ranges resolve against the *sim runtime* content: it is
                // the registry the live `TurretBehavior` was built from and
                // carries the named turret-ammo fixtures (`install_turret_ammo`),
                // unlike the view snapshot (`create_base_content` only).
                let range = self
                    .sim
                    .block_runtime()
                    .and_then(|runtime| {
                        let runtime_content = runtime.content();
                        let runtime_def = runtime_content.block(block)?;
                        mind_core::world::blocks::defense::turrets::config_for(
                            runtime_content,
                            &runtime_def.name,
                            runtime_content.bullet_names(),
                        )
                    })
                    .map(|config| config.range)
                    .or_else(|| {
                        mind_core::world::blocks::defense::turrets::config_for(
                            content,
                            &def.name,
                            content.bullet_names(),
                        )
                        .map(|config| config.range)
                    });
                self.place_range_cache.borrow_mut().insert(block, range);
                range
            }
        };
        range.map(|range| (range, PlaceRangeKind::Turret, accent))
    }

    /// `InputHandler.validBreak(x, y)`: whether the local team may deconstruct
    /// the building at `(x, y)`. Builds `BuildRules` from the world resource
    /// like `SimRuntime::can_place` does. Rust-only seam for the break-rect
    /// preview (G03 T2).
    pub fn preview_can_break(&self, x: i32, y: i32) -> bool {
        let team = self.sim.live_default_team();
        let Some(content) = self.content_snapshot.as_ref() else {
            return false;
        };
        let rules = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::world::limits::BuildRules>()
            .cloned()
            .unwrap_or_default();
        mind_core::world::build::valid_break(
            content,
            &self.sim.grid,
            Some(&self.sim.ecs.0),
            &rules,
            team,
            x,
            y,
        )
    }

    // ---- G03 T3 build-plan / beam seams (Rust-only) ----

    /// One player-builder draw state (`BuilderComp.drawBuilding`, G03 T3).
    pub fn player_builder_view(&self) -> Option<BuilderView> {
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
        use mind_core::sim::builder::BuilderPlans;
        let entity = self.sim.player_unit()?;
        let pos = self.sim.ecs.0.get::<Pos>(entity)?;
        let type_comp = self.sim.ecs.0.get::<UnitTypeComp>(entity)?;
        let def = self.content_snapshot.as_ref()?.unit(type_comp.type_id)?;
        let update_building = self
            .sim
            .ecs
            .0
            .get::<BuilderPlans>(entity)
            .map(|plans| plans.update_building)
            .unwrap_or(false);
        let rotation = self
            .sim
            .ecs
            .0
            .get::<UnitCore>(entity)
            .map(|core| core.rotation)
            .unwrap_or(0.0);
        Some(BuilderView {
            x: pos.x,
            y: pos.y,
            rotation,
            beam_offset: def.build_beam_offset,
            build_range: def.build_range,
            draw_beam: def.draw_build_beam,
            update_building,
            plans: self.sim.player_build_plans(),
        })
    }

    /// Whether `(x, y)` holds a construct site building `block`
    /// (`InputHandler.planMatches`, `InputHandler.java:1515-1518`).
    pub fn plan_matches(&self, x: i32, y: i32, block: BlockId) -> bool {
        self.construct_at(x, y)
            .is_some_and(|(current, _, _)| current == block)
    }

    /// Building footprint size at `(x, y)` (`Block.size`; G03 T13 inventory
    /// anchoring). Read-only MCP/HUD probe; `1` without a building.
    #[func]
    pub fn block_size(&self, x: i32, y: i32) -> i32 {
        self.tile_at(x, y)
            .and_then(|tile| self.content_snapshot.as_ref()?.block(tile.block))
            .map(|def| def.size.max(1))
            .unwrap_or(1)
    }

    /// Per-building state for the block pass (G03 T8): door open, switch
    /// enabled, sorter/unloader/source items, damage and team mark inputs.
    pub fn block_state_draw(&self, x: i32, y: i32) -> Option<BlockStateDraw> {
        use mind_core::entities::comp::building::{DoorState, SandboxState};
        use mind_core::entities::comp::health::Health;
        use mind_core::world::blocks::distribution::sorter::SorterBuild;
        use mind_core::world::blocks::storage::unloader::UnloaderBuild;
        let entity = self.tile_at(x, y)?.build?;
        let healthf = self
            .sim
            .ecs
            .0
            .get::<Health>(entity)
            .map(Health::fraction)
            .unwrap_or(1.0);
        Some(BlockStateDraw {
            door_open: self
                .sim
                .ecs
                .0
                .get::<DoorState>(entity)
                .map(|door| door.open)
                .unwrap_or(false),
            enabled: self
                .sim
                .ecs
                .0
                .get::<mind_core::entities::comp::building::Building>(entity)
                .map(|building| building.enabled)
                .unwrap_or(true),
            sort_item: self
                .sim
                .ecs
                .0
                .get::<SorterBuild>(entity)
                .and_then(|sorter| sorter.sort_item),
            unloader_item: self
                .sim
                .ecs
                .0
                .get::<UnloaderBuild>(entity)
                .and_then(|unloader| unloader.sort_item),
            source_item: self
                .sim
                .ecs
                .0
                .get::<SandboxState>(entity)
                .and_then(|state| state.item),
            healthf,
            damaged: healthf < 0.999,
        })
    }

    /// The player team's rebuild ghosts (`TeamData.plans`, pushed by
    /// `Sim::resolve_deaths`): `(x, y, rotation, block)` in insertion order.
    pub fn team_block_plans(&self) -> Vec<(i16, i16, i8, BlockId)> {
        let team = mind_core::ecs::TeamId(self.player_team());
        self.sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .and_then(|runtime| runtime.session.teams.data(team))
            .map(|data| {
                data.plans
                    .iter()
                    .filter(|plan| !plan.removed)
                    .map(|plan| (plan.x, plan.y, plan.rotation, plan.block))
                    .collect()
            })
            .unwrap_or_default()
    }

    // ---- G03 T4 bullet draw seam (Rust-only) ----

    /// Live bullets in spawn order (`Groups.bullet`, G03 T4). Skips bullets
    /// already marked finished (removed this tick).
    pub fn bullet_draws(&self) -> Vec<BulletDraw> {
        use mind_core::combat::bullet::Bullet;
        use mind_core::ecs::EntitySeq;
        use mind_core::entities::comp::Pos;
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &entity in &runtime.bullets {
            let Some(pos) = self.sim.ecs.0.get::<Pos>(entity) else {
                continue;
            };
            let Some(bullet) = self.sim.ecs.0.get::<Bullet>(entity) else {
                continue;
            };
            if bullet.finished() {
                continue;
            }
            let id = self
                .sim
                .ecs
                .0
                .get::<EntitySeq>(entity)
                .map(|seq| seq.0)
                .unwrap_or(0);
            out.push(BulletDraw {
                id,
                x: pos.x,
                y: pos.y,
                rotation: bullet.rotation,
                time: bullet.time,
                lifetime: bullet.lifetime,
                fdata: bullet.fdata,
                def: bullet.def,
                team: 0,
            });
        }
        out
    }

    // ---- G03 T6 hovered-building bars (MCP/API seam) ----

    /// `Block.setBars` rows for the player-team building at `(x, y)` (G03 T6),
    /// in Java order: `[{"key","label_key","label_arg","color","value"}]`.
    /// `[]` without a player building. Label text is formatted in GDScript.
    #[func]
    pub fn building_bars_json(&self, x: i32, y: i32) -> GString {
        let Some(tile) = self.tile_at(x, y) else {
            return GString::from("[]");
        };
        let Some(entity) = tile.build else {
            return GString::from("[]");
        };
        if self.building_team(x, y) != Some(self.player_team()) {
            return GString::from("[]");
        }
        let Some(content) = self.content_snapshot.as_ref() else {
            return GString::from("[]");
        };
        let Some(def) = content.block(tile.block) else {
            return GString::from("[]");
        };
        let healthf = self
            .sim
            .ecs
            .0
            .get::<mind_core::entities::comp::health::Health>(entity)
            .map(mind_core::entities::comp::health::Health::fraction)
            .unwrap_or(1.0);
        let power_status = self
            .sim
            .ecs
            .0
            .get::<PowerModule>(entity)
            .map(|module| module.status);
        let items_total = self
            .sim
            .ecs
            .0
            .get::<ItemModule>(entity)
            .map(|module| module.total);
        let liquids = self
            .sim
            .ecs
            .0
            .get::<LiquidModule>(entity)
            .map(|module| module.liquids.clone());
        let liquid_info: Vec<(String, u32)> = content
            .liquids()
            .iter()
            .map(|liquid| {
                (
                    liquid.name.clone(),
                    mind_core::render::drawf::to_bits([
                        liquid.color.r,
                        liquid.color.g,
                        liquid.color.b,
                        liquid.color.a,
                    ]),
                )
            })
            .collect();
        let rows = building_bar_rows(
            def,
            healthf,
            power_status,
            items_total,
            liquids.as_deref(),
            &liquid_info,
        );
        let json: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "key": row.key,
                    "label_key": row.label_key,
                    "label_arg": row.label_arg,
                    "color": format!("{:08x}", row.color),
                    "value": row.value,
                })
            })
            .collect();
        GString::from(serde_json::Value::Array(json).to_string().as_str())
    }

    /// Discovered (static) fog bits for `team`, one byte per tile (`0`/`255`,
    /// row-major, y up), or `None` when fog is off (H03 T12 render seam).
    pub fn fog_static_bytes(&self, team: u8) -> Option<(i32, i32, Vec<u8>)> {
        self.sim.fog_static_bytes(team)
    }

    /// Dynamic vision sources for `team`: `(world x, world y, radius px)`
    /// (H03 T12 render seam; radius = fog radius in tiles * 8).
    pub fn fog_sources(&self, team: u8) -> Vec<(f32, f32, f32)> {
        self.sim.fog_sources(team)
    }

    /// Fog probe (`FogControl.isVisibleTile`/`isDiscovered`) for playtests and
    /// the H03 fog renderer: `{fog, staticFog, visible, discovered, teams:[id]}`.
    /// Reports `{"fog":false}` without a `CampaignRuntime`.
    #[func]
    pub fn fog_debug_json(&self, team: i32, x: i32, y: i32) -> GString {
        let Some(rules) = mind_core::game::runtime::rules_of(&self.sim.ecs.0) else {
            return GString::from("{\"fog\":false}");
        };
        let team = (team & 0xFF) as u8;
        let mut teams: Vec<u8> = Vec::new();
        if let Some(fog) = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::fog::FogControl>()
        {
            for (id, data) in fog.fog.iter().enumerate() {
                if data.is_some() {
                    teams.push(id as u8);
                }
            }
        }
        let value = serde_json::json!({
            "fog": rules.fog,
            "staticFog": rules.static_fog,
            "visible": self.sim.fog_visible_tile(team, x, y),
            "discovered": self.sim.fog_discovered(team, x, y),
            "teams": teams,
        });
        GString::from(value.to_string().as_str())
    }

    /// Debug: forces `rules.fog`/`rules.staticFog` on the live session and
    /// re-initialises fog. `false` without a `CampaignRuntime`.
    #[func]
    pub fn debug_set_fog(&mut self, fog: bool, static_fog: bool) -> bool {
        {
            let Some(mut runtime) = self
                .sim
                .ecs
                .0
                .get_resource_mut::<mind_core::game::runtime::CampaignRuntime>()
            else {
                return false;
            };
            runtime.session.rules.fog = fog;
            runtime.session.rules.static_fog = static_fog;
        }
        self.sim.reset_fog();
        true
    }

    /// Debug: `Tile.setTeam` for the building at `(x, y)` (`BuildingComp.changeTeam`
    /// -> `checkAllowUpdate`); `false` when the tile has no building.
    #[func]
    pub fn debug_set_building_team(&mut self, x: i32, y: i32, team: i32) -> bool {
        self.sim.debug_set_building_team(x, y, (team & 0xFF) as u8)
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

/// `HintsFragment` poll data (`HintsFragment.java:74-102`; T4a/T4b inputs).
/// Read-only and cheap: one building-entity pass plus one grid pass.
fn hint_state_value(sim: &Sim, content: Option<&ContentRegistry>, team: u8) -> serde_json::Value {
    use mind_core::content::BlockFlag;
    use mind_core::entities::comp::TeamComp;
    use mind_core::entities::comp::unit::comp::{ItemsComp, UnitCore, UnitPayloads, UnitTypeComp};

    let block_name = |id: BlockId| {
        content
            .and_then(|content| content.block(id))
            .map(|def| def.name.clone())
            .unwrap_or_default()
    };

    // `Trigger.placedBlocks` / `events` sets.
    let placed: Vec<String> = sim
        .hint_events
        .placed_by_player
        .iter()
        .map(|id| block_name(*id))
        .collect();
    let events: Vec<&str> = sim.hint_events.events.iter().copied().collect();

    // Player unit inputs (`UnitCore`/`ItemsComp`/`UnitPayloads`).
    let (player_dead, spawned_by_core, player_has_item, player_payload, player_can_boost) =
        match sim.player_unit() {
            Some(entity) => {
                let core = sim.ecs.0.get::<UnitCore>(entity);
                let items = sim.ecs.0.get::<ItemsComp>(entity);
                let payloads = sim.ecs.0.get::<UnitPayloads>(entity);
                let can_boost = sim
                    .ecs
                    .0
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| content.and_then(|content| content.unit(comp.type_id)))
                    .is_some_and(|def| def.can_boost);
                (
                    core.map(|core| core.dead).unwrap_or(false),
                    core.map(|core| core.spawned_by_core).unwrap_or(false),
                    items.and_then(|items| items.item).is_some(),
                    payloads
                        .map(|payloads| payloads.stack.len() as i64)
                        .unwrap_or(-1),
                    can_boost,
                )
            }
            None => (true, false, false, -1, false),
        };

    // Live session counters (`HintsFragment` reads `state.wave`/wave team AI).
    let runtime = sim
        .ecs
        .0
        .get_resource::<mind_core::game::runtime::CampaignRuntime>();
    let (enemies, wave) = runtime
        .map(|runtime| (runtime.session.enemies as i64, runtime.session.wave as i64))
        .unwrap_or((0, 0));

    // Team unit count (`TeamData.units`).
    let team_units = sim
        .ecs
        .0
        .get_non_send::<mind_core::sim::unit_runtime::UnitRuntime>()
        .map(|runtime| {
            runtime
                .units
                .iter()
                .filter(|entity| {
                    sim.ecs
                        .0
                        .get::<TeamComp>(**entity)
                        .is_some_and(|comp| comp.team == team)
                })
                .count() as i64
        })
        .unwrap_or(0);

    // Building counters + the first core's identity (`state.teams.get(team)`).
    let mut conveyors = 0i64;
    let mut cores = 0i64;
    let mut fabricators = 0i64;
    let mut extinguishers = 0i64;
    let mut core_block = String::new();
    let mut core_capacity = 0i64;
    let mut core_copper = 0i64;
    let mut core_has_foundation_cost = false;
    let copper_id = content.and_then(|content| content.item_id("copper"));
    let foundation = content.and_then(|content| content.block_by_name("core-foundation"));
    for (_seq, entity, comp) in sim.ecs.entities_by_seq() {
        let Some(def) = content.and_then(|content| content.block(comp.block)) else {
            continue;
        };
        if comp.team.0 == team
            && matches!(
                def.kind,
                BlockKind::Conveyor | BlockKind::ArmoredConveyor | BlockKind::StackConveyor
            )
        {
            conveyors += 1;
        }
        if def.name == "tank-fabricator" || def.name == "ground-factory" {
            fabricators += 1;
        }
        if def.flags.contains(&BlockFlag::Extinguisher) {
            extinguishers += 1;
        }
        if def.kind != BlockKind::CoreBlock || comp.team.0 != team {
            continue;
        }
        cores += 1;
        if !core_block.is_empty() {
            continue;
        }
        core_block = def.name.clone();
        core_capacity = def.item_capacity as i64;
        if let Some(module) = sim.ecs.0.get::<ItemModule>(entity) {
            if let Some(copper) = copper_id {
                core_copper = module.get(copper) as i64;
            }
            core_has_foundation_cost = foundation.is_some_and(|def| {
                def.requirements
                    .iter()
                    .all(|stack| module.get(stack.item) >= stack.amount)
            });
        }
    }

    // `Trigger.blockInfo`-style live plan count (the player's build queue).
    let plans = sim.player_build_plans().len() as i64;

    // `Indexer.isBlockPresent(Blocks.coreZone)`; checked across the tile's
    // block/floor/overlay layers.
    let core_zone_present = content
        .and_then(|content| content.block_id("core-zone"))
        .is_some_and(|zone| {
            sim.grid.iter_row_major().any(|(pos, _)| {
                sim.grid
                    .tiles
                    .getn(pos.x() as i32, pos.y() as i32)
                    .is_some_and(|tile| {
                        tile.block == zone || tile.floor == zone || tile.overlay == zone
                    })
            })
        });

    // Campaign facts for the T4b predicates (live runtime campaign + rules).
    let rules = mind_core::game::runtime::rules_of(&sim.ecs.0);
    let attack_mode = rules.is_some_and(|rules| rules.attack_mode);
    // `!rules.hasEnv(Env.scorching)` (`world/meta/Env.java`; the one bit table
    // is `game::planet::env_bit`).
    let serpulo = rules.is_none_or(|rules| {
        rules.env & mind_core::game::planet::env_bit(mind_core::content::EnvFlag::Scorching) == 0
    });
    let team_rebuild_plans = runtime
        .and_then(|runtime| {
            runtime
                .session
                .teams
                .get_or_null(mind_core::ecs::TeamId(team))
        })
        .map(|data| data.plans.len() as i64)
        .unwrap_or(0);
    let sector_fact = |name: &str| -> (bool, bool) {
        let (Some(registry), Some(campaign)) = (content, runtime.map(|r| &r.campaign)) else {
            return (false, false);
        };
        let Some(preset) = registry.sector_by_name(name) else {
            return (false, false);
        };
        let Some(record) = campaign.sector(preset.planet, preset.sector) else {
            return (false, false);
        };
        (record.has_save(), record.is_captured(None))
    };
    let (fungal_pass_saved, _) = sector_fact("fungalPass");
    let (_, tar_fields_captured) = sector_fact("tarFields");
    let sector_threat = runtime
        .and_then(|runtime| runtime.session.sector)
        .and_then(|(planet, sector)| {
            runtime.and_then(|runtime| runtime.campaign.sector(planet, sector))
        })
        .map(|record| record.threat as f64)
        .unwrap_or(0.0);

    // `state.stats.destroyed` cores: not tracked per core block yet (T4b).
    let cores_destroyed = 0i64;
    let boss: serde_json::Value =
        serde_json::from_str(&sim.boss_json()).unwrap_or(serde_json::Value::Null);

    serde_json::json!({
        "placed": placed,
        "events": events,
        "player_dead": player_dead,
        "player_has_item": player_has_item,
        "player_can_boost": player_can_boost,
        "player_spawned_by_core": spawned_by_core,
        "player_payload": player_payload,
        "team_units": team_units,
        "enemies": enemies,
        "wave": wave,
        "conveyors": conveyors,
        "plans": plans,
        "core_block": core_block,
        "core_copper": core_copper,
        "core_capacity": core_capacity,
        "core_has_foundation_cost": core_has_foundation_cost,
        "cores": cores,
        "fabricators": fabricators,
        "extinguishers": extinguishers,
        "core_zone_present": core_zone_present,
        "cores_destroyed": cores_destroyed,
        "boss": boss,
        "team_rebuild_plans": team_rebuild_plans,
        "serpulo": serpulo,
        "fungal_pass_saved": fungal_pass_saved,
        "tar_fields_captured": tar_fields_captured,
        "sector_threat": sector_threat,
        "attack_mode": attack_mode,
    })
}

/// Item counts for the building at `(x, y)` as JSON; `None` when the tile has
/// no item-capable building or its items total zero. See
/// [`MindSimHost::block_items_json`].
/// JSON view of a building's `config()` for the config-UI seams.
fn config_to_json(value: &mind_core::world::config::ConfigValue) -> serde_json::Value {
    use mind_core::world::config::ConfigValue as V;
    use serde_json::json;
    match value {
        V::None => serde_json::Value::Null,
        V::Item(id) => json!({"type": "item", "id": id.raw()}),
        V::Liquid(id) => json!({"type": "liquid", "id": id.raw()}),
        V::Block(id) => json!({"type": "block", "id": id.raw()}),
        V::Unit(id) => json!({"type": "unit", "id": id.raw()}),
        V::Content(type_, id) => json!({"type": "content", "content": type_.name(), "id": id}),
        V::Point2(x, y) => json!({"type": "point", "x": x, "y": y}),
        V::Point2Array(points) => json!({
            "type": "point_array",
            "points": points.iter().collect::<Vec<_>>()
        }),
        V::Number(number) => serde_json::Number::from_f64(*number)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        V::Bool(flag) => serde_json::Value::Bool(*flag),
        V::Building(_) => json!({"type": "building"}),
        V::Bytes(bytes) => json!({"type": "bytes", "len": bytes.len()}),
        V::String(text) => json!({"type": "string", "value": text}),
    }
}

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
    /// Resume half of the decoded save (units/fires/puddles, markers, stats,
    /// meta tags); `None` for generated worlds. The world half above was moved
    /// out (T2).
    save: Option<mind_core::io::sim_io::DecodedSave>,
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
/// reader, preserving the container's rules and the resume payload (units,
/// fires, puddles, markers, stats, meta tags).
fn load_grid_file(path: &str, registry: &mut ContentRegistry) -> Option<LoadedGrid> {
    let bytes = FileAccess::get_file_as_bytes(path);
    let mut save = match mind_core::io::sim_io::decode_save_bytes(bytes.as_slice(), registry) {
        Ok(save) => save,
        Err(error) => {
            log::warn!("load_sector: `{path}` failed: {error}");
            return None;
        }
    };
    // Move the world half out (callers keep their current shape); the resume
    // half stays in `save` for the sector-resume path.
    let grid = std::mem::take(&mut save.grid);
    let pending_buildings = std::mem::take(&mut save.pending);
    let plans = std::mem::take(&mut save.plans);
    let rules = save.rules.take();
    Some(LoadedGrid {
        grid,
        rules,
        pending_buildings,
        plans,
        save: Some(save),
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

/// `WaveSpawner.playerNear()` (`WaveSpawner.java:58-60`): spawns exist, the
/// player is alive, a spawn tile is within `radius` of the player and the
/// player team is not the wave team. `spawns` are tiles; the player position
/// is world px.
pub fn player_near_drop_zone_predicate(
    spawns: &[(i32, i32)],
    player: Option<(f32, f32)>,
    player_team: u8,
    wave_team: u8,
    radius: f32,
) -> bool {
    if spawns.is_empty() || player_team == wave_team {
        return false;
    }
    let Some((px, py)) = player else {
        return false;
    };
    let unit = mind_core::config::TILESIZE as f32;
    spawns.iter().any(|(sx, sy)| {
        let dx = *sx as f32 * unit - px;
        let dy = *sy as f32 * unit - py;
        dx.hypot(dy) < radius
    })
}

/// One hovered-building bar row (`Block.setBars`, G03 T6).
pub struct BarRow {
    /// `"health" | "power" | "items" | "liquid"`.
    pub key: &'static str,
    /// Bundle key (no `@`).
    pub label_key: String,
    /// `{0}` argument for the bundle format (`None` = no argument).
    pub label_arg: Option<String>,
    /// `Bar` colour RGBA8888.
    pub color: u32,
    /// Fraction `0..=1`.
    pub value: f32,
}

/// `Block.setBars` base rows (`Block.java:709-760`) in Java order: health,
/// power (when consumed), items (when `hasItems && configurable`) and liquids
/// (per consumed liquid, else the dynamic current=max entry bar).
///
/// `health` is the health **fraction** (`Health.fraction`); `liquid_info` is
/// `(name, rgba8888)` per liquid in id order (colour for the bar; `name` for
/// the `liquid.<name>.name` label key).
pub fn building_bar_rows(
    def: &mind_core::content::registries::blocks::BlockDef,
    health: f32,
    power_status: Option<f32>,
    items_total: Option<i32>,
    liquids: Option<&[f32]>,
    liquid_info: &[(String, u32)],
) -> Vec<BarRow> {
    use mind_core::content::registries::blocks::Consume;
    let mut rows = Vec::new();
    rows.push(BarRow {
        key: "health",
        label_key: String::from("stat.health"),
        label_arg: None,
        color: 0xff34_1cff,
        value: health.clamp(0.0, 1.0),
    });
    if let Some(status) = power_status {
        let buffered = def.consumes.iter().find_map(|spec| match &spec.consume {
            Consume::Power { buffered, .. } if *buffered > 0.0 => Some(*buffered),
            _ => None,
        });
        let (label_key, label_arg) = match buffered {
            Some(capacity) => (
                String::from("bar.poweramount"),
                Some(((status * capacity) as i32).to_string()),
            ),
            None => (String::from("bar.power"), None),
        };
        rows.push(BarRow {
            key: "power",
            label_key,
            label_arg,
            color: 0xec7b_4cff,
            value: status.clamp(0.0, 1.0),
        });
    }
    if def.has_items
        && def.configurable
        && def.item_capacity > 0
        && let Some(total) = items_total
    {
        rows.push(BarRow {
            key: "items",
            label_key: String::from("bar.items"),
            label_arg: Some(total.to_string()),
            color: 0x2ea7_56ff,
            value: (total as f32 / def.item_capacity as f32).clamp(0.0, 1.0),
        });
    }
    if def.has_liquids && def.liquid_capacity > 0.0 {
        let amounts = liquids.unwrap_or(&[]);
        let mut added = false;
        for spec in &def.consumes {
            let consumed: Vec<mind_core::content::LiquidId> = match &spec.consume {
                Consume::Liquid { liquid, .. } => vec![*liquid],
                Consume::Liquids(stacks) => stacks.iter().map(|stack| stack.liquid).collect(),
                _ => Vec::new(),
            };
            for liquid in consumed {
                let Some((name, color)) = liquid_info.get(liquid.index()) else {
                    continue;
                };
                let amount = amounts.get(liquid.index()).copied().unwrap_or(0.0);
                rows.push(BarRow {
                    key: "liquid",
                    label_key: format!("liquid.{name}.name"),
                    label_arg: None,
                    color: *color,
                    value: (amount / def.liquid_capacity).clamp(0.0, 1.0),
                });
                added = true;
            }
        }
        if !added {
            // Dynamic current-liquid bar (`bar.liquid` when empty).
            let (index, amount) = amounts.iter().enumerate().fold(
                (usize::MAX, 0.0f32),
                |(best, max), (i, amount)| {
                    if *amount > max {
                        (i, *amount)
                    } else {
                        (best, max)
                    }
                },
            );
            let (label_key, color) = if amount > 0.0 {
                liquid_info
                    .get(index)
                    .map(|(name, color)| (format!("liquid.{name}.name"), *color))
                    .unwrap_or((String::from("bar.liquid"), 0x596a_b8ff))
            } else {
                (String::from("bar.liquid"), 0x596a_b8ff)
            };
            rows.push(BarRow {
                key: "liquid",
                label_key,
                label_arg: None,
                color,
                value: (amount / def.liquid_capacity).clamp(0.0, 1.0),
            });
        }
    }
    rows
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

    #[test]
    fn building_bars_items_row() {
        // Java gates `bar.items` on `hasItems && configurable`
        // (`Block.java:724`); the launch pad satisfies both.
        let registry = base_registry();
        let Some(block) = registry.block_id("launch-pad") else {
            panic!("launch-pad missing");
        };
        let Some(def) = registry.block(block) else {
            panic!("launch-pad def missing");
        };
        assert!(def.has_items && def.configurable && def.item_capacity > 0);
        let rows = building_bar_rows(def, 1.0, None, Some(5), None, &[]);
        assert_eq!(rows.len(), 2, "rows: {:?}", rows.len());
        assert_eq!(rows[0].key, "health");
        assert_eq!(rows[1].key, "items");
        assert_eq!(rows[1].label_key, "bar.items");
        assert_eq!(rows[1].label_arg.as_deref(), Some("5"));
        let expected = 5.0 / def.item_capacity as f32;
        assert!((rows[1].value - expected).abs() < 1e-4);
    }

    #[test]
    fn building_bars_conduit_dynamic_liquid() {
        // Conduit: `has_liquids` with no liquid consumer -> dynamic max bar.
        let registry = base_registry();
        let Some(conduit) = registry.block_id("conduit") else {
            panic!("conduit missing");
        };
        let Some(def) = registry.block(conduit) else {
            panic!("conduit def missing");
        };
        assert!(def.has_liquids && def.liquid_capacity > 0.0);
        let liquid_info: Vec<(String, u32)> = registry
            .liquids()
            .iter()
            .map(|liquid| (liquid.name.clone(), 0))
            .collect();
        let capacity = def.liquid_capacity;
        let liquids = vec![0.0, capacity * 0.4];
        let rows = building_bar_rows(def, 1.0, None, None, Some(&liquids), &liquid_info);
        let row = rows
            .iter()
            .find(|row| row.key == "liquid")
            .unwrap_or_else(|| panic!("liquid row"));
        assert!((row.value - 0.4).abs() < 1e-3, "value: {}", row.value);
        assert_eq!(row.label_key, format!("liquid.{}.name", liquid_info[1].0));
    }

    #[test]
    fn drop_zone_predicate_uses_the_radius() {
        // Spawn tile (10,10) -> world (80,80); radius 300 (G03 T9).
        let spawns = [(10, 10)];
        assert!(player_near_drop_zone_predicate(
            &spawns,
            Some((80.0 + 299.0, 80.0)),
            1,
            2,
            300.0
        ));
        assert!(!player_near_drop_zone_predicate(
            &spawns,
            Some((80.0 + 301.0, 80.0)),
            1,
            2,
            300.0
        ));
        // No spawns / same team / dead player.
        assert!(!player_near_drop_zone_predicate(
            &[],
            Some((80.0, 80.0)),
            1,
            2,
            300.0
        ));
        assert!(!player_near_drop_zone_predicate(
            &spawns,
            Some((80.0, 80.0)),
            2,
            2,
            300.0
        ));
        assert!(!player_near_drop_zone_predicate(&spawns, None, 1, 2, 300.0));
    }
}
