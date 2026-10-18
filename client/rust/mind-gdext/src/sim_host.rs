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
use mind_core::content::registries::units::parts::DrawPartSpec;
use mind_core::content::registries::units::weapon::WeaponDef;
use mind_core::weapons::WeaponMount;

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
    /// Legged-unit state (`LegsComp` + `UnitType` leg knobs); `None` for
    /// non-legged units (J03 T4).
    pub legs: Option<LegsDraw>,
    /// Tank tread state (`TankComp` + `UnitType` tread knobs); `None` for
    /// non-tanks (J03 T5).
    pub treads: Option<TreadsDraw>,
    /// Live weapon mounts (`UnitWeapons`).
    pub weapons: Vec<WeaponDraw>,
    /// `UnitType.parts` (drawn after body/weapons/items, `UnitType.java:1561-1578`).
    pub parts: Vec<DrawPartSpec>,
    /// `Scaled.fin()` (`TimedKillComp.fin`, `time / lifetime`); `0` when the
    /// unit is not timed.
    pub fin: f32,
    /// `UnitType.drawBody` (gates the body quad).
    pub draw_body: bool,
    /// `UnitType.drawCell` (gates the team cell quad).
    pub draw_cell: bool,
    /// `UnitType.groundLayer` (draw z base; `Layer.groundUnit` default).
    pub ground_layer: f32,
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
    /// Active status effects with view parameters `(effect, chance, color)`
    /// (`StatusEffect.update` particles; H03 T10).
    pub statuses: Vec<(mind_core::content::EffectId, f32, mind_core::content::Rgba)>,
    /// `isGrounded()` = `elevation < 0.001` (`UnitComp.java:80-82`; H03 T10).
    pub grounded: bool,
    /// `UnitType.hovering` (walk-effect gate; H03 T10).
    pub hovering: bool,
    /// `UnitType.emitWalkEffect` (H03 T10).
    pub emit_walk_effect: bool,
    /// `floorOn().isLiquid` (walk effect is `Fx.ripple` on liquid floors;
    /// H03 T10).
    pub floor_is_liquid: bool,
    /// `tile.getFloorColor()` (`Floor.mapColor`; H03 T10).
    pub floor_color: mind_core::content::Rgba,
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

/// `TankComp` tread animation state (`drawTank`) + `UnitType` tread knobs.
pub struct TreadsDraw {
    /// `TankComp.treadTime` (`unit.treadTime()`).
    pub tread_time: f32,
    /// `UnitType.treadRects`.
    pub rects: Vec<mind_core::content::registries::units::TreadRect>,
    /// `UnitType.treadFrames`.
    pub frames: i32,
}

/// `LegsComp` state + `UnitType` leg knobs resolved for `drawLegs` (J03 T4).
pub struct LegsDraw {
    /// `LegsComp.baseRotation` (`unit.baseRotation()`).
    pub base_rotation: f32,
    /// Live legs in index order (foot/joint from `LegsComp.Leg`).
    pub legs: Vec<mind_core::render::unit_draw::LegDrawLeg>,
    /// `1 - drownTime`; the port has no drowning yet, so 1.
    pub inv_drown: f32,
    /// `UnitType.legBaseOffset`.
    pub leg_base_offset: f32,
    /// `UnitType.legStraightness`.
    pub leg_straightness: f32,
    /// `UnitType.baseLegStraightness`.
    pub base_leg_straightness: f32,
    /// `UnitType.legStraightLength`.
    pub leg_straight_length: f32,
    /// `UnitType.legExtension`.
    pub leg_extension: f32,
    /// `UnitType.legBaseUnder`.
    pub leg_base_under: bool,
    /// `UnitType.shadowElevation` (`0` = off).
    pub shadow_elevation: f32,
}

/// One command-overlay row: `(x, y, hit_size, flying, target_pos)`
/// (`InputHandler.drawCommanded` inputs).
pub type UnitCommandRow = (f32, f32, f32, bool, Option<(f32, f32)>);

/// One ability's live draw state (`Ability.draw(Unit)`; L04 T2).
pub struct UnitAbilityDraw {
    /// Ability class tag.
    pub kind: mind_core::content::registries::units::ability::AbilityKind,
    /// Unit center x.
    pub x: f32,
    /// Unit center y.
    pub y: f32,
    /// Unit rotation (degrees).
    pub rotation: f32,
    /// Unit team id.
    pub team: u8,
    /// Ability metadata.
    pub spec: mind_core::content::registries::units::ability::AbilitySpec,
    /// Index-aligned runtime state (shield pools, view lerps).
    pub state: mind_core::combat::abilities::AbilityState,
    /// `Unit.shield` (`ShieldComp.shield`).
    pub shield: f32,
    /// `EntitySeq.0` (deterministic suppression particle seed).
    pub id: u64,
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
    /// Outline drawn over the body (`Weapon.top`); non-top weapons draw their
    /// `<name>-outline` sprite under the body (`drawWeaponOutlines`).
    pub top: bool,
    /// Visual recoil `0..1`.
    pub recoil: f32,
    /// The resolved weapon definition (`Weapon.draw` layer offset, parts,
    /// heat colour, flip sprite).
    pub def: WeaponDef,
    /// The live mount state (`DrawPart.params` warmup/reload/heat/recoil).
    pub mount: WeaponMount,
}
use mind_core::ai::StanceBits;
use mind_core::audio::{AudioSinkRes, SharedAudioLog};
use mind_core::command::Command;
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

/// One beam-node laser (`BeamNode.BeamNodeBuild.draw`, I02 T5). The endpoints
/// are already inset (`BeamNode.java:225-231`).
pub struct BeamLaserLink {
    /// Beam start x (source centre + `size * 4 * d4`).
    pub x1: f32,
    /// Beam start y.
    pub y1: f32,
    /// Beam end x (destination centre - `4 * d4`).
    pub x2: f32,
    /// Beam end y.
    pub y2: f32,
    /// Source graph satisfaction (`setupColor`).
    pub satisfaction: f32,
    /// Source block id (`<name>-beam` region fallback).
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

/// Fire draw row (`FireComp.draw`; H03 T7).
pub struct FireDraw {
    /// World centre x (tile centre + `randomSeedRange(y, 2)` jitter).
    pub x: f32,
    /// World centre y (tile centre + `randomSeedRange(x, 2)` jitter).
    pub y: f32,
    /// `min((int)animation, frames - 1)`.
    pub frame: i32,
    /// `clamp(warmup / warmupDuration)`.
    pub alpha: f32,
}

/// Puddle draw row (`PuddleComp.draw` / `Liquid.drawPuddle`; H03 T7).
pub struct PuddleDraw {
    /// World centre (`Puddle.x/y`).
    pub x: f32,
    /// World centre (`Puddle.x/y`).
    pub y: f32,
    /// Entity id (`Puddle.id`; seeds the sine phases and satellites).
    pub id: i32,
    /// `clamp(amount / (maxLiquid / 1.5))`.
    pub f: f32,
    /// `liquid.color.shiftValue(-0.05)`.
    pub color: [f32; 4],
    /// `tile.floor().isLiquid`.
    pub on_liquid: bool,
    /// `(lightColor, opacity)` when `lightColor.a > 0.001 && f > 0`.
    pub light: Option<([f32; 4], f32)>,
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
    /// Campaign extras for the next generated numbered sector (H05 T2):
    /// `MindCampaign.prime_sector_load` primes this before `load_sector`;
    /// `load_sector_world` falls back to `game::planet::static_gen_extras`.
    pending_gen_extras: Option<(String, u16, mind_core::maps::planet::SectorGenExtras)>,
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
            fx_host: None,
            pending_commands: VecDeque::new(),
            relay_outbox: None,
            audio_log: SharedAudioLog::new(),
            sector_rules: None,
            sector_plans: Vec::new(),
            input: None,
            sector_spawns: 0,
            place_range_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            sector_resume: None,
            last_limit_area: None,
            pending_gen_extras: None,
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

    /// Emitted once per world load (`WorldLoadEvent`; `World.loadMap` end).
    /// The placement palette resets its category/selection and re-reads the
    /// catalogue (`J/ui/fragments/PlacementFragment.java:78-84`).
    #[signal]
    fn world_loaded();

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
        // `@clientLocale`/`@clientName` from the shared settings store
        // (`GlobalVars.java:211-216` reads `player.locale()`/`player.name()`).
        let info = mind_core::logic::globals::LogicClientInfo {
            locale: crate::settings_hub::with_settings(|store| {
                store.get_string(mind_core::io::settings::KEY_LOCALE, "en")
            }),
            name: crate::settings_hub::with_settings(|store| {
                store.get_string(mind_core::io::settings::KEY_PLAYER_NAME, "")
            }),
        };
        self.sim.ecs.0.insert_resource(info);
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
            // World-processor text sources (H06 T4): `Core.bundle`, `Iconc`
            // and `player.locale()` for `message`/`localeprint`/`printchar`.
            // The map-scoped locales are replaced per sector load.
            let assets_dir = crate::assets::loader::resolve_assets_dir();
            let locale = crate::assets::loader::resolve_locale();
            let map_locales = self
                .sim
                .ecs
                .0
                .get_resource::<mind_core::logic::world::LogicText>()
                .map(|text| text.map_locales.clone())
                .unwrap_or_default();
            self.sim
                .ecs
                .0
                .insert_resource(mind_core::logic::world::LogicText {
                    bundle: Some(crate::assets::bundle::load_bundle(&assets_dir, &locale)),
                    iconc: Some(crate::assets::bundle::load_iconc(&assets_dir)),
                    map_locales,
                    locale,
                });
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
        // Reactor/generator explosions (`ExplosionSink`) share the same bus so
        // their effect/sound/shake/scorch reach the view (PAR-0399).
        self.sim.ecs.0.insert_resource(
            mind_core::world::blocks::power::explosion::ExplosionSinkRes(Some(Box::new(
                crate::fx::explosion_sink::HostExplosionSink { bus: bus.clone() },
            ))),
        );
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
    /// (plan 21 §3.5). Rust-only seam used by the `MindNet` relay runtime for
    /// foreign rows and predictions.
    pub fn enqueue_sim_command(&mut self, command: SimCommand) {
        self.pending_commands.push_back(command);
    }

    /// Single outbound seam for player actions (I08 T6): into the relay
    /// mailbox when a match installed one, else the local command queue.
    pub fn submit_player_command(&mut self, command: SimCommand) {
        if let Some(outbox) = self.relay_outbox.as_ref()
            && let Ok(mut queue) = outbox.lock()
        {
            queue.push(command);
            return;
        }
        self.enqueue_sim_command(command);
    }

    /// Installs the relay mailbox (called by `MindNet` while a match is
    /// active).
    pub fn install_relay_outbox(
        &mut self,
        outbox: std::sync::Arc<std::sync::Mutex<Vec<SimCommand>>>,
    ) {
        self.relay_outbox = Some(outbox);
    }

    /// Removes the relay mailbox (single-player again).
    pub fn clear_relay_outbox(&mut self) {
        self.relay_outbox = None;
    }

    /// Runs `f` against the live content registry (relay name resolution and
    /// the content hash; I08 T4/T6). `None` before a registry is installed.
    pub fn with_registry<R>(
        &self,
        f: impl FnOnce(&mind_core::content::ContentRegistry) -> R,
    ) -> Option<R> {
        self.content_snapshot.as_ref().map(f)
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
                // `unit_command` targets: `x`/`y` are the raw wire pair (world
                // pixels for a move, or the lossless `pack_wire_target` pair);
                // the optional `target_unit` key packs an EntitySeq attack
                // target for recipes (H03 blocker #2: ids do not fit an f32).
                let fx = value.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let fy = value.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let (x, y) = value
                    .get("target_unit")
                    .and_then(|v| v.as_i64())
                    .map(|id| {
                        mind_core::sim::pack_wire_target(
                            &mind_core::input::action::CommandTarget::Unit(id as i32),
                        )
                    })
                    .unwrap_or((fx as f32, fy as f32));
                SimCommand::UnitCommand {
                    units,
                    command,
                    x,
                    y,
                    // `queue` (I01 T13): append instead of replace.
                    queue: value
                        .get("queue")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
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

    /// MCP/test helper: parse a small JSON command and route it through the
    /// single outbound seam [`Self::submit_player_command`] (I08 T6), so the
    /// relay path can be driven in-engine.
    ///
    /// Shape: `{"op":"place","x":1,"y":2,"block":<u16>,"rotation":0}`,
    /// `{"op":"break","x":1,"y":2}`,
    /// `{"op":"configure","x":1,"y":2,"content":<name>}` (or `"number"`/
    /// `"bool"`/`"text"`),
    /// `{"op":"rotate","x":1,"y":2,"direction":false}`,
    /// `{"op":"delete_plans","positions":[...]}`.
    #[func]
    pub fn submit_player_command_json(&mut self, cmd_json: GString) -> bool {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&cmd_json.to_string()) else {
            log::warn!("submit_player_command_json: invalid JSON");
            return false;
        };
        let op = value.get("op").and_then(|v| v.as_str()).unwrap_or("");
        let coord = |key: &str| {
            value
                .get(key)
                .and_then(|v| v.as_i64())
                .and_then(|v| i16::try_from(v).ok())
        };
        let command = match op {
            "place" => {
                let (Some(x), Some(y)) = (coord("x"), coord("y")) else {
                    return false;
                };
                SimCommand::Place {
                    x,
                    y,
                    block: value.get("block").and_then(|v| v.as_u64()).unwrap_or(0) as u16,
                    rotation: value
                        .get("rotation")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                        .clamp(0, 3) as i8,
                    team: value.get("team").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
                    player: Some(0),
                }
            }
            "break" => {
                let (Some(x), Some(y)) = (coord("x"), coord("y")) else {
                    return false;
                };
                SimCommand::Break {
                    x,
                    y,
                    player: Some(0),
                }
            }
            "configure" => {
                use mind_core::determinism::ConfigValue;
                let (Some(x), Some(y)) = (coord("x"), coord("y")) else {
                    return false;
                };
                let config = if let Some(name) = value.get("content").and_then(|v| v.as_str()) {
                    ConfigValue::Content(name.to_string())
                } else if let Some(number) = value.get("number").and_then(|v| v.as_f64()) {
                    // `to_sim_config` (`command_emit.rs:268-277`): integral in
                    // i32 range -> Int, otherwise Float.
                    if number.fract() == 0.0
                        && number >= f64::from(i32::MIN)
                        && number <= f64::from(i32::MAX)
                    {
                        ConfigValue::Int(number as i32)
                    } else {
                        ConfigValue::Float(number as f32)
                    }
                } else if let Some(flag) = value.get("bool").and_then(|v| v.as_bool()) {
                    ConfigValue::Bool(flag)
                } else if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                    ConfigValue::Text(text.to_string())
                } else {
                    log::warn!("submit_player_command_json: configure needs a value");
                    return false;
                };
                SimCommand::Configure {
                    x,
                    y,
                    value: config,
                }
            }
            "rotate" => {
                let (Some(x), Some(y)) = (coord("x"), coord("y")) else {
                    return false;
                };
                SimCommand::Rotate {
                    x,
                    y,
                    direction: value
                        .get("direction")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                }
            }
            "delete_plans" => {
                let positions: smallvec::SmallVec<[i32; 32]> = value
                    .get("positions")
                    .and_then(|v| v.as_array())
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|row| row.as_i64().map(|id| id as i32))
                            .collect()
                    })
                    .unwrap_or_default();
                SimCommand::DeletePlans { positions }
            }
            other => {
                log::warn!("submit_player_command_json: unsupported op `{other}`");
                return false;
            }
        };
        self.submit_player_command(command);
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

    /// I02 T4: drains the per-tick `flashHit` wall-collision log as
    /// `[(tile_x, tile_y)]`. Rust-only seam for `render/blocks.rs`, which owns
    /// the `hit` decay (`Wall.WallBuild.draw`, `Wall.java:146-161`).
    pub fn drain_wall_flashes(&mut self) -> Vec<(i16, i16)> {
        self.sim.drain_wall_flashes()
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

    /// FNV-1a-64 over the registered content names
    /// (`ContentRegistry::content_hash`); the relay build gate compares it
    /// between host and joiner (I08 T4). 0 when no registry is loaded.
    #[func]
    pub fn content_hash(&self) -> i64 {
        self.content_snapshot
            .as_ref()
            .map(|registry| registry.content_hash() as i64)
            .unwrap_or(0)
    }

    /// Sets the sim tick counter (snapshot restore / late join, I08 T5; the
    /// save-load path sets the same field at `sim_io.rs:1325`).
    #[func]
    pub fn set_sim_tick(&mut self, tick: i64) {
        self.sim.state.tick = tick.max(0) as u64;
    }

    /// Saves the live world through the plan-05 IoSet handler and returns the
    /// `.msav` bytes (I08 T4/T5 host snapshot encoder). `flush_io` runs the
    /// save boundary without advancing the tick (`io_set.rs:227`), so the
    /// bytes are ready when this returns; empty on failure.
    #[func]
    pub fn encode_snapshot(&mut self) -> PackedByteArray {
        let relative = GString::from("user://mp/host-snapshot.msav");
        let path = globalize(&relative);
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            log::warn!("encode_snapshot: cannot create {}", parent.display());
            return PackedByteArray::new();
        }
        self.sim.request_save(path.clone(), false);
        self.sim.flush_io();
        match std::fs::read(&path) {
            Ok(bytes) => PackedByteArray::from(bytes.as_slice()),
            Err(error) => {
                log::warn!("encode_snapshot: read {} failed: {error}", path.display());
                PackedByteArray::new()
            }
        }
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

    /// `state.rules.pauseDisabled` of the live session (`false` without
    /// rules); the UI's pause paths gate on it (`Control.java:620,629`).
    #[func]
    pub fn rules_pause_disabled(&self) -> bool {
        self.live_rules().is_some_and(|rules| rules.pause_disabled)
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

    /// `InputHandler.selectedUnit()`'s ControlBlock branch
    /// (`InputHandler.java:2159-2161`, I01 T12): the detached `block` unit of an
    /// own controllable turret at `(x, y)` as its sim `EntitySeq`. `None` when
    /// the tile holds no turret, the team differs, or the turret is the
    /// player's own unit already. Rust-only seam (`#[func]`-free, like
    /// `building_team`); the desktop input calls it through `bind()`.
    pub fn control_block_unit_id(&mut self, x: i32, y: i32) -> Option<i32> {
        if !self.sim.grid.tiles.in_bounds(x, y) {
            return None;
        }
        self.sim
            .control_block_unit_id(TilePos::new(x as i16, y as i16))
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
            // Force projector state (`ForceProjector.java:166-167`; H02 T7).
            if let Some(state) =
                world.get::<mind_core::world::blocks::defense::shields::ForceProjectorState>(entity)
            {
                put("force_radscl", Variant::from(f64::from(state.radscl)));
                put("force_buildup", Variant::from(f64::from(state.buildup)));
                put("force_broken", Variant::from(state.broken));
                put(
                    "force_phase_heat",
                    Variant::from(f64::from(state.phase_heat)),
                );
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

    /// `StatusEffect.permanent` for a `@status-<name>` content reference
    /// (`LStatements.java:1465-1474` hides `seconds` for permanent statuses).
    #[func]
    pub fn status_permanent(&self, effect: GString) -> bool {
        let name = effect.to_string();
        let name = name.trim_start_matches("@status-");
        self.content_snapshot
            .as_ref()
            .and_then(|content| content.status_by_name(name))
            .is_some_and(|status| status.permanent)
    }

    /// Logic-editor content picker rows (`LStatements.java` `showSelectTable`
    /// content grids): `unit`/`item`/`liquid`/`status`/`weather`/`block`.
    ///
    /// Rows are `{name, hidden, unlocked, always_unlocked, unlocked_now}`
    /// (`unlocked_now` is `UnlockableContent.unlockedNow`: unlocked outside a
    /// campaign), plus `logic_controllable` for units and `permanent`/`show`
    /// for statuses. Unknown kinds return `[]`.
    #[func]
    pub fn logic_picker_json(&self, kind: GString) -> GString {
        let Some(registry) = self.content_snapshot.as_ref() else {
            return GString::from("[]");
        };
        let campaign = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::runtime::CampaignRuntime>()
            .is_some();
        let unlocked_now = |unlock: &mind_core::content::UnlockFields| {
            unlock.unlocked || unlock.always_unlocked || !campaign
        };
        let base_row = |name: &str, unlock: &mind_core::content::UnlockFields, hidden: bool| {
            serde_json::json!({
                "name": name,
                "hidden": hidden,
                "unlocked": unlock.unlocked,
                "always_unlocked": unlock.always_unlocked,
                "unlocked_now": unlocked_now(unlock),
            })
        };
        let rows: Vec<serde_json::Value> = match kind.to_string().as_str() {
            "unit" => registry
                .units()
                .iter()
                .filter(|unit| !unit.removed)
                .map(|unit| {
                    let mut row = base_row(&unit.name, &unit.unlock, unit.hidden);
                    row["logic_controllable"] = unit.logic_controllable.into();
                    row
                })
                .collect(),
            "item" => registry
                .items()
                .iter()
                .filter(|item| !item.removed)
                .map(|item| base_row(&item.name, &item.unlock, item.hidden))
                .collect(),
            "liquid" => registry
                .liquids()
                .iter()
                .filter(|liquid| !liquid.removed)
                .map(|liquid| base_row(&liquid.name, &liquid.unlock, liquid.hidden))
                .collect(),
            "status" => registry
                .statuses()
                .iter()
                .filter(|status| !status.removed)
                .map(|status| {
                    let mut row = base_row(&status.name, &status.unlock, !status.show);
                    row["permanent"] = status.permanent.into();
                    row["show"] = status.show.into();
                    row
                })
                .collect(),
            "weather" => registry
                .weathers()
                .iter()
                .filter(|weather| !weather.removed)
                .map(|weather| base_row(&weather.name, &weather.unlock, weather.hidden))
                .collect(),
            "block" => registry
                .blocks()
                .iter()
                .filter(|block| !block.removed)
                .map(|block| base_row(&block.name, &block.unlock, block.is_hidden()))
                .collect(),
            _ => Vec::new(),
        };
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| String::from("[]"))
                .as_str(),
        )
    }

    /// Read-only `Building.onConfigureBuildTapped` preview for the tap handler
    /// (H02 T6, PAR-0220): `0` = Java `return false` (panel stays), `1` =
    /// `deselect(); return false` (panel closes), `2` = `return true` (switch
    /// the panel to the tapped building). `-1` when either tile has no build.
    #[func]
    pub fn preview_config_tap(&self, selected_x: i32, selected_y: i32, x: i32, y: i32) -> i32 {
        let (Ok(sx), Ok(sy), Ok(tx), Ok(ty)) = (
            i16::try_from(selected_x),
            i16::try_from(selected_y),
            i16::try_from(x),
            i16::try_from(y),
        ) else {
            return -1;
        };
        let Some(selected) = self.sim.grid.entity_at(TilePos::new(sx, sy)) else {
            return -1;
        };
        let Some(other) = self.sim.grid.entity_at(TilePos::new(tx, ty)) else {
            return -1;
        };
        match mind_core::world::config_tap::config_tap(
            &self.sim.ecs.0,
            &self.sim.grid,
            selected,
            other,
        ) {
            mind_core::world::behavior::ConfigTapResult::Consume(_) => 0,
            mind_core::world::behavior::ConfigTapResult::ConsumeAndDeselect(_) => 1,
            mind_core::world::behavior::ConfigTapResult::SwitchAfter(_) => 2,
        }
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

    /// Drains queued world-processor messages (`announce`/`notify`/`toast`) as
    /// an Array of `{type, text, duration}` dictionaries (H06 T4).
    #[func]
    pub fn drain_logic_messages(&mut self) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        for (type_, text, duration) in self.sim.take_logic_messages() {
            let mut row = Dictionary::<GString, Variant>::new();
            row.set(
                &GString::from("type"),
                &GString::from(type_.name()).to_variant(),
            );
            row.set(
                &GString::from("text"),
                &GString::from(text.as_str()).to_variant(),
            );
            row.set(
                &GString::from("duration"),
                &f64::from(duration).to_variant(),
            );
            out.push(&row.to_variant());
        }
        out
    }

    /// The queued draw commands of the display at `(x, y)`'s ROOT (T13).
    ///
    /// Returns the packed `DisplayCmd` queue the view-side rasterizer drains
    /// (`LogicDisplay.build.commands`; a tileable member forwards to its root).
    #[func]
    pub fn logic_display_commands(&self, x: i64, y: i64) -> PackedInt64Array {
        let mut out = PackedInt64Array::new();
        let Ok(tx) = i16::try_from(x) else {
            return out;
        };
        let Ok(ty) = i16::try_from(y) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let root = self
            .sim
            .ecs
            .0
            .get::<mind_core::logic::blocks::LogicDisplayState>(e)
            .and_then(|state| state.root_display)
            .unwrap_or(e);
        let Some(state) = self
            .sim
            .ecs
            .0
            .get::<mind_core::logic::blocks::LogicDisplayState>(root)
        else {
            return out;
        };
        for command in &state.commands {
            out.push(*command as i64);
        }
        out
    }

    /// Display geometry/queue readback (`logic_display_state`; T13).
    #[func]
    pub fn logic_display_state(&self, x: i64, y: i64) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::<GString, Variant>::new();
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let world = &self.sim.ecs.0;
        let Some(state) = world.get::<mind_core::logic::blocks::LogicDisplayState>(e) else {
            return out;
        };
        let root = state.root_display.unwrap_or(e);
        let root_state = world
            .get::<mind_core::logic::blocks::LogicDisplayState>(root)
            .unwrap_or(state);
        let (root_x, root_y) = world
            .get::<mind_core::entities::comp::Building>(root)
            .map(|building| (building.tile.x(), building.tile.y()))
            .unwrap_or((tx, ty));
        let tileable = mind_core::logic::blocks::block_kind_of(world, e)
            == Some(mind_core::content::BlockKind::TileableLogicDisplay);
        let frame = mind_core::logic::blocks::tileable_def_of(world, e)
            .map(|def| def.frame_size)
            .unwrap_or(6);
        let (width, height) = if tileable {
            (
                root_state.tiles_width * 32 - frame * 2,
                root_state.tiles_height * 32 - frame * 2,
            )
        } else {
            (root_state.display_size, root_state.display_size)
        };
        out.set(&GString::from("root_x"), &i64::from(root_x).to_variant());
        out.set(&GString::from("root_y"), &i64::from(root_y).to_variant());
        out.set(&GString::from("width"), &i64::from(width).to_variant());
        out.set(&GString::from("height"), &i64::from(height).to_variant());
        out.set(
            &GString::from("queued"),
            &(root_state.commands.len() as i64).to_variant(),
        );
        out.set(
            &GString::from("operations"),
            &(root_state.operations as i64).to_variant(),
        );
        out.set(
            &GString::from("rectangular"),
            &root_state.rectangular.to_variant(),
        );
        out.set(&GString::from("tileable"), &tileable.to_variant());
        out
    }

    /// RGBA of one raster pixel (`logic_display_pixel`; command space,
    /// bottom-left origin). `-1` when the display or pixel is absent.
    #[func]
    pub fn logic_display_pixel(&self, x: i64, y: i64, px: i64, py: i64) -> i64 {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return -1;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return -1;
        };
        let root = self
            .sim
            .ecs
            .0
            .get::<mind_core::logic::blocks::LogicDisplayState>(e)
            .and_then(|state| state.root_display)
            .unwrap_or(e);
        let rasters = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::logic::display_raster::LogicDisplayRasters>()
            .and_then(|rasters| rasters.rasters.get(&root));
        rasters
            .and_then(|raster| raster.pixel(px as i32, py as i32))
            .map(i64::from)
            .unwrap_or(-1)
    }

    /// Processes the visible display roots and returns one row per visible
    /// display for the render pass (T13).
    ///
    /// Rows: `{draw_buffer, x, y, w, h, root_x, root_y, frame, frame_x,
    /// frame_y, bits, sprite}`. Processing drains each visible root's queue
    /// into its CPU raster (`LogicDisplay.processCommands`).
    #[func]
    pub fn logic_display_sync(
        &mut self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        if self.display_icon_names.is_none() {
            self.display_icon_names = self
                .sim
                .block_runtime()
                .map(|runtime| crate::display_context::IconNames::build(runtime.content()));
        }
        let assets = self
            .base()
            .try_get_node_as::<crate::assets::MindAssets>("/root/MindAssets");
        let Some(names) = self.display_icon_names.as_ref() else {
            return out;
        };
        let ctx = crate::display_context::AtlasIcons::new(assets, names);
        let rows = mind_core::logic::display_raster::display_rows(
            &mut self.sim.ecs.0,
            &ctx,
            min_x,
            min_y,
            max_x,
            max_y,
        );
        for row in rows {
            let mut dict = Dictionary::<GString, Variant>::new();
            dict.set(
                &GString::from("root_x"),
                &i64::from(row.root_x).to_variant(),
            );
            dict.set(
                &GString::from("root_y"),
                &i64::from(row.root_y).to_variant(),
            );
            dict.set(&GString::from("draw_buffer"), &row.draw_buffer.to_variant());
            dict.set(&GString::from("x"), &row.x.to_variant());
            dict.set(&GString::from("y"), &row.y.to_variant());
            dict.set(&GString::from("w"), &row.w.to_variant());
            dict.set(&GString::from("h"), &row.h.to_variant());
            dict.set(&GString::from("frame"), &row.frame.to_variant());
            dict.set(&GString::from("frame_x"), &row.frame_x.to_variant());
            dict.set(&GString::from("frame_y"), &row.frame_y.to_variant());
            dict.set(&GString::from("bits"), &i64::from(row.bits).to_variant());
            dict.set(
                &GString::from("sprite"),
                &i64::from(row.sprite).to_variant(),
            );
            out.push(&dict.to_variant());
        }
        out
    }

    /// Canvas draw rows for the render pass (T14).
    ///
    /// Rows: `{x, y, cx, cy, size, blending, invalidated}`.
    #[func]
    pub fn logic_canvas_sync(
        &self,
        min_x: f32,
        min_y: f32,
        max_x: f32,
        max_y: f32,
    ) -> Array<Variant> {
        let mut out = Array::<Variant>::new();
        for row in mind_core::logic::blocks::canvas::canvas_rows(
            &self.sim.ecs.0,
            min_x,
            min_y,
            max_x,
            max_y,
        ) {
            let mut dict = Dictionary::<GString, Variant>::new();
            dict.set(&GString::from("x"), &i64::from(row.x).to_variant());
            dict.set(&GString::from("y"), &i64::from(row.y).to_variant());
            dict.set(&GString::from("cx"), &row.cx.to_variant());
            dict.set(&GString::from("cy"), &row.cy.to_variant());
            dict.set(&GString::from("size"), &row.size.to_variant());
            dict.set(
                &GString::from("blending"),
                &i64::from(row.blending).to_variant(),
            );
            dict.set(
                &GString::from("pixels"),
                &i64::from(row.pixels).to_variant(),
            );
            dict.set(
                &GString::from("bpp"),
                &i64::from(row.bits_per_pixel).to_variant(),
            );
            let mut palette = PackedInt64Array::new();
            for color in &row.palette {
                palette.push(i64::from(*color));
            }
            dict.set(&GString::from("palette"), &palette.to_variant());
            dict.set(&GString::from("invalidated"), &row.invalidated.to_variant());
            out.push(&dict.to_variant());
        }
        out
    }

    /// RGBA8 canvas pixels (bottom-row-first) for the canvas at `(x, y)`.
    ///
    /// Clears the canvas's `invalidated` flag (the view uploaded it).
    #[func]
    pub fn logic_canvas_image(&mut self, x: i64, y: i64) -> PackedByteArray {
        let mut out = PackedByteArray::new();
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let palette = mind_core::logic::blocks::canvas_def_of(&self.sim.ecs.0, e)
            .map(|def| def.palette)
            .unwrap_or_default();
        let Some(mut state) = self
            .sim
            .ecs
            .0
            .get_mut::<mind_core::logic::blocks::CanvasBlockState>(e)
        else {
            return out;
        };
        let size = state.canvas_size.max(0);
        let pixels = size * size;
        let mut bytes: Vec<u8> = Vec::with_capacity((pixels * 4) as usize);
        for row in (0..size).rev() {
            for col in 0..size {
                let pixel = state.pixel_rgba(row * size + col, &palette);
                bytes.push(((pixel >> 24) & 0xff) as u8);
                bytes.push(((pixel >> 16) & 0xff) as u8);
                bytes.push(((pixel >> 8) & 0xff) as u8);
                bytes.push((pixel & 0xff) as u8);
            }
        }
        state.invalidated = false;
        out.extend(bytes.iter().copied());
        out
    }

    /// Palette indices of the canvas at `(x, y)`, one byte per pixel
    /// (linear `pos = y * size + x`, `CanvasBuild.getPixel`).
    #[func]
    pub fn logic_canvas_pixels(&self, x: i64, y: i64) -> PackedByteArray {
        let mut out = PackedByteArray::new();
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let Some(state) = self
            .sim
            .ecs
            .0
            .get::<mind_core::logic::blocks::CanvasBlockState>(e)
        else {
            return out;
        };
        let pixels = state.canvas_size.max(0) * state.canvas_size.max(0);
        for pos in 0..pixels {
            let index = state.get_byte(pos * i32::from(state.bits_per_pixel));
            out.push((index.clamp(0, 255)) as u8);
        }
        out
    }

    /// RGBA8 bytes (row 0 = top) of the root raster at `(root_x, root_y)`.
    #[func]
    pub fn logic_display_image(&self, root_x: i64, root_y: i64) -> PackedByteArray {
        let mut out = PackedByteArray::new();
        let (Ok(tx), Ok(ty)) = (i16::try_from(root_x), i16::try_from(root_y)) else {
            return out;
        };
        let Some(e) = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))
        else {
            return out;
        };
        let Some(raster) = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::logic::display_raster::LogicDisplayRasters>()
            .and_then(|rasters| rasters.rasters.get(&e))
        else {
            return out;
        };
        // The raster is stored top-row-first, but `canvas::draw_region` flips
        // every region (negative height), so the upload is bottom-row-first:
        // display y = 0 (the Java buffer's origin) ends up at the bottom.
        let mut bytes: Vec<u8> = Vec::with_capacity(raster.pixels.len() * 4);
        let width = raster.width as usize;
        let height = raster.height as usize;
        for row in (0..height).rev() {
            for col in 0..width {
                let Some(pixel) = raster.pixels.get(row * width + col) else {
                    continue;
                };
                bytes.push(((pixel >> 24) & 0xff) as u8);
                bytes.push(((pixel >> 16) & 0xff) as u8);
                bytes.push(((pixel >> 8) & 0xff) as u8);
                bytes.push((pixel & 0xff) as u8);
            }
        }
        out.extend(bytes.iter().copied());
        out
    }

    /// `LogicBuild.showEditDialog` -> `configure(bytes)` (I03 T7).
    ///
    /// Compresses the editor text with `relativeConnections`, returns
    /// `"toolong:<len>"` past `maxCompressedLen`, and otherwise queues a
    /// `Bytes` configure as the player (the campaign configure gate applies,
    /// unlike the `logic_set_code` test seam).
    #[func]
    pub fn logic_configure_code(&mut self, x: i64, y: i64, code: GString) -> GString {
        self.logic_last_error = String::new();
        let code = code.to_string();
        let world = &self.sim.ecs.0;
        let Some(e) =
            mind_core::logic::blocks::logic_block::processor_at(world, x as i32, y as i32)
        else {
            self.logic_last_error = "noprocessor".to_owned();
            return GString::from("noprocessor");
        };
        let Some(links) = world
            .get::<mind_core::logic::blocks::LogicBlockState>(e)
            .map(|state| state.relative_connections(world, e))
        else {
            self.logic_last_error = "noprocessor".to_owned();
            return GString::from("noprocessor");
        };
        match mind_core::logic::blocks::logic_block::encode_for_config(&code, &links) {
            Err(len) => {
                self.logic_last_error = format!("toolong:{len}");
                GString::from(format!("toolong:{len}").as_str())
            }
            Ok(bytes) => {
                let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
                    return GString::from("noprocessor");
                };
                self.pending_commands.push_back(SimCommand::Configure {
                    x: tx,
                    y: ty,
                    value: mind_core::determinism::ConfigValue::Bytes(bytes),
                });
                GString::new()
            }
        }
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
        let logic_rules = mind_core::logic::blocks::rules_ref(&self.sim.ecs.0);
        let value = serde_json::json!({
            // I03 T9: the add dialog hides the unit category when the rule is off
            // (`LogicDialog.showAddDialog`).
            "logicUnitControl": logic_rules.logic_unit_control(),
            "logicUnitBuild": logic_rules.logic_unit_build(),
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

    /// `Saves.SaveSlot.savePreview`: writes `<user>/previews/save_slot_<name>.png`
    /// from the live grid (`Saves.java:343-375`). Returns whether the PNG landed.
    #[func]
    pub fn write_save_preview(&mut self, name: GString) -> bool {
        use mind_core::io::fs::{FileSystem, NativeFs, Paths};
        use mind_core::io::map::{BlockPalette, encode_png, generate_preview_from_tiles};
        use mind_core::world::map_source::EcsMapSource;

        let paths = Paths::new(crate::assets::loader::user_data_root());
        let Some(registry) = self.content_snapshot.as_ref() else {
            log::warn!("write_save_preview: no content registry");
            return false;
        };
        // `Saves.savePreview` writes `renderer.minimap.getPixmap()`: the real
        // per-block colours (`ContentLoader.loadColors`).
        let mut palette = BlockPalette::of(registry);
        palette.apply_block_colors(&crate::render::minimap::load_block_colors());
        let source = EcsMapSource::new(&self.sim.grid, registry, &self.sim.ecs.0);
        let image = generate_preview_from_tiles(&palette, &source);
        let Ok(bytes) = encode_png(&image) else {
            log::warn!("write_save_preview: PNG encode failed");
            return false;
        };
        match NativeFs.write(&paths.preview_for_save(name.to_string().as_str()), &bytes) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("write_save_preview: {error}");
                false
            }
        }
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

    /// `Logic.reset()` world half (`Logic.java:299-311`): `Groups.clear()`,
    /// `Time.clear()`, `world.tiles` emptied and `state = new GameState()`
    /// (menu), through [`Sim::reset`]. The host's per-match leftovers (sector
    /// rules/plans/resume, pending launch/generation, the scenario player, the
    /// cutscene input lock) go with it, so `get_state_json().phase` reads
    /// `menu` and `live_rules()` is empty once a game ends (L02 T2). Called
    /// only from `MindCampaign.reset_to_menu`, the single return-to-menu path.
    #[func]
    pub fn reset_to_menu(&mut self) {
        self.sim.reset();
        self.sim.reset_logic_presentation();
        self.player = None;
        self.sector_rules = None;
        self.sector_plans.clear();
        self.sector_resume = None;
        self.pending_launch = None;
        self.pending_launch_core = None;
        self.pending_gen_extras = None;
        self.pending_map_gen_seed = None;
        self.generated_loadout_placed = false;
        self.input_locked = false;
        self.pending_commands.clear();
        self.runner = FixedStepRunner::for_rate(
            self.sim.config().fixed_hz,
            self.sim.config().max_ticks_per_frame,
        );
        self.world_dirty = true;
        self.emit_state();
        self.emit_world_changed();
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
        self.emit_world_loaded();
        true
    }

    /// Primes the campaign extras (`SectorGenExtras`) the next generated
    /// numbered sector reads at load time (H05 T2;
    /// `MindCampaign.prime_sector_load`). Only for `planet`/`sector` matching
    /// the next `load_sector`; otherwise the static fallback applies.
    #[func]
    pub fn set_sector_gen_extras(
        &mut self,
        planet: GString,
        sector: i32,
        threat: f32,
        has_enemy_base: bool,
        has_base: bool,
    ) {
        self.set_sector_gen_extras_rs(
            &planet.to_string(),
            sector,
            mind_core::maps::planet::SectorGenExtras {
                threat,
                has_enemy_base,
                has_base,
            },
        );
    }

    /// Rust-only variant (called by `MindCampaign` through a sibling bind, and
    /// by the lost-sector replay before its `load_sector`).
    pub fn set_sector_gen_extras_rs(
        &mut self,
        planet: &str,
        sector: i32,
        extras: mind_core::maps::planet::SectorGenExtras,
    ) {
        if sector < 0 {
            return;
        }
        self.pending_gen_extras = Some((planet.to_owned(), sector as u16, extras));
    }

    /// Sets the launch loadout the next generated sector places
    /// (`Control.playNewSector`/`Universe.getLastLoadout`; H05 T4).
    /// `schematic_index = -1` = planet default (free launch); `stacks_json` is
    /// the launch-resources array (`[{item, amount}]`).
    #[func]
    pub fn set_pending_launch(&mut self, schematic_index: i32, stacks_json: GString) {
        self.pending_launch = Some((schematic_index, stacks_json.to_string()));
    }

    /// `Universe.updateLoadout((CoreBlock)launchBlock)` +
    /// `updateLaunchResources(resources)` (Accelerator.java:275-277) for the
    /// next sector: the launch loadout becomes a single `core` block
    /// (`Universe.java:118-121`) stocked with `stacks_json`.
    #[func]
    pub fn set_pending_launch_core(&mut self, core: GString, stacks_json: GString) {
        self.pending_launch_core = Some(core.to_string());
        self.pending_launch = Some((-1, stacks_json.to_string()));
    }

    /// The single-core loadout schematic of `core` (`Schematic::single_core`).
    fn single_core_schematic(&self, core: &str) -> Option<mind_core::game::schematic::Schematic> {
        let registry = self.content_snapshot.as_ref()?;
        let id = registry.block_id(core)?;
        let size = registry.block(id).map(|def| def.size).unwrap_or(1);
        Some(mind_core::game::schematic::Schematic::single_core(id, size))
    }

    /// Whether the last `load_sector` placed the generated sector's launch
    /// loadout during the world load (H05 T4).
    pub fn generated_loadout_placed(&self) -> bool {
        self.generated_loadout_placed
    }

    /// Loads the user schematic directory (`dataDirectory/schematics`) into a
    /// fresh registry (`Schematics.get()`; used by `place_launch_loadout` and
    /// the generated-sector launch resolution).
    fn load_user_schematics(&self) -> mind_core::game::schematics::Schematics {
        use mind_core::game::schematics::Schematics;
        let mut schematics = Schematics::new();
        let Some(registry) = self.content_snapshot.as_ref() else {
            return schematics;
        };
        // Java's `schematicDirectory` is `dataDirectory/schematics`; `user://`
        // globalizes to the same data root the campaign facade loads from.
        let paths = mind_core::io::fs::Paths::new(
            ProjectSettings::singleton()
                .globalize_path("user://")
                .to_string(),
        );
        if let Err(error) = schematics.load(
            &mind_core::io::fs::NativeFs,
            &paths.root().join("schematics"),
            registry,
        ) {
            log::warn!("load_user_schematics: schematic load failed: {error}");
        }
        schematics
    }

    /// Resolves the launch loadout for a generated sector (H05 T4): the
    /// pending schematic index or the planet default (`getDefaultLoadout` on
    /// the planet's `defaultCore`), with the pending launch-resource stacks.
    fn resolve_generated_loadout(
        &mut self,
        planet_name: &str,
    ) -> Option<mind_core::maps::generators::planet::LaunchLoadout> {
        let (index, stacks_json) = self.pending_launch.take().unwrap_or((-1, String::new()));
        let core_override = self
            .pending_launch_core
            .take()
            .and_then(|core| self.single_core_schematic(&core));
        let schematics = self.load_user_schematics();
        let registry = self.content_snapshot.as_ref()?;
        let index = if core_override.is_some() {
            None
        } else if index >= 0 {
            Some(index as usize)
        } else {
            registry
                .planet_by_name(planet_name)
                .and_then(|def| def.default_core.clone())
                .and_then(|core| registry.block_id(&core))
                .and_then(|core| schematics.get_default_loadout(core))
        };
        let Some(schematic) =
            core_override.or_else(|| index.and_then(|index| schematics.all.get(index).cloned()))
        else {
            log::warn!("load_sector: no launch loadout for `{planet_name}`");
            return None;
        };
        let stacks: Vec<mind_core::io::json::content_serde::JsonItemStack> =
            serde_json::from_str(&stacks_json).unwrap_or_default();
        let stacks = stacks
            .iter()
            .filter_map(|stack| {
                let name = stack.item.as_deref()?;
                Some((registry.item_id(name)?, stack.amount))
            })
            .collect();
        Some(mind_core::maps::generators::planet::LaunchLoadout { schematic, stacks })
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

    /// Loads a custom map file into the sim (`Control.playMap`'s world half,
    /// `Control.java:400-426`): the grid, buildings and cores of `path`, with
    /// no sector preset/generator fallback.
    ///
    /// `path` is an absolute path, a `res://`/`user://` path, or a bare map
    /// name registered with `MindPreview`. Returns `false` (and sets
    /// `last_map_error` to `map.invalid`) when the file is missing or fails to
    /// decode, exactly like `World.loadMap`'s error branch
    /// (`World.java:355-364`).
    #[func]
    pub fn load_map_world(&mut self, path: GString) -> bool {
        self.last_map_error = None;
        let Some(resolved) = self.resolve_map_path(&path) else {
            log::warn!("load_map_world: unknown map `{path}`");
            self.last_map_error = Some(String::from("map.invalid"));
            return false;
        };
        // A new world invalidates the previous campaign runtime.
        self.sim.ecs.0.remove_resource::<CampaignRuntime>();
        // Resume metadata is per load; clear before this load decides.
        self.sector_resume = None;
        // The loadout placement flag is per load (H05 T4).
        self.generated_loadout_placed = false;
        let Some(mut registry) = self.content_snapshot.take() else {
            log::warn!("load_map_world: content registry unavailable");
            self.last_map_error = Some(String::from("map.invalid"));
            return false;
        };
        // `World.loadMap` loads through `new FilterContext(map)`: the map's
        // `genfilters` stack (or the default one) is applied to the decoded
        // tiles (`World.java:341-357,661-695`). Java's `filter.randomize()`
        // draws from the global `Mathf.rand`, so every load re-rolls; the port
        // draws a fresh seed per load and records it (`map_gen_seed`) so the
        // world is reproducible from seed + map (`GenerateFilter.java:91-92`).
        let seed = self.next_map_gen_seed();
        let Some(loaded) = load_map_grid_file(&resolved, &mut registry, seed) else {
            self.content_snapshot = Some(registry);
            self.last_map_error = Some(String::from("map.invalid"));
            return false;
        };
        // `state.map = map` (`World.java:369`); `Map.name()` = the `name` tag
        // (`Map.java:127-129`).
        let map_name = loaded
            .save
            .as_ref()
            .and_then(|save| save.tags.get("name").cloned())
            .unwrap_or_default();
        let loaded_ok = self.finish_world_load(registry, loaded, WorldLoadExtras::default());
        if loaded_ok {
            self.sim.set_map_name(map_name);
        }
        loaded_ok
    }

    /// A fresh generation seed for the next `load_map_world` (`Mathf.rand`
    /// re-rolls every map load; the port keeps the draw recorded instead).
    ///
    /// The value is mixed with the monotonic world-load counter so two loads
    /// requested within the same clock tick still re-roll; in-engine replays
    /// can pin it through `MindSimHost::set_next_map_gen_seed`.
    fn next_map_gen_seed(&mut self) -> u64 {
        if let Some(seed) = self.pending_map_gen_seed.take() {
            return seed;
        }
        let ticks = godot::classes::Time::singleton().get_ticks_usec();
        ticks ^ self.world_loads.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    }

    /// Pins the generation seed the next `load_map_world` will use and report
    /// (`World.FilterContext`; the value recorded in the state dump as
    /// `map_gen_seed`). Rust-only replay/debug seam; the default draw is a
    /// fresh seed per load.
    pub fn set_next_map_gen_seed(&mut self, seed: u64) {
        self.pending_map_gen_seed = Some(seed);
    }

    /// Resolves a `load_map_world` path argument: absolute paths as-is,
    /// `res://`/`user://` through `ProjectSettings.globalize_path`, and a bare
    /// map name through the `MindPreview` registry rows (`path` key).
    fn resolve_map_path(&self, path: &GString) -> Option<String> {
        let raw = path.to_string();
        if raw.starts_with("res://") || raw.starts_with("user://") {
            return Some(
                ProjectSettings::singleton()
                    .globalize_path(&raw)
                    .to_string(),
            );
        }
        if std::path::Path::new(&raw).is_absolute() {
            return Some(raw);
        }
        if raw.is_empty() {
            return None;
        }
        let mut preview = self.base().get_node_or_null("/root/Spine/MindPreview")?;
        let resolved = preview
            .call("map_path", &[GString::from(raw.as_str()).to_variant()])
            .try_to::<GString>()
            .ok()?;
        (!resolved.is_empty()).then(|| resolved.to_string())
    }

    /// `map.invalid` (or `None`) from the last failed `load_map_world`
    /// (`World.loadMap`'s `ui.showErrorMessage("@map.invalid")` branch).
    #[func]
    pub fn last_map_error(&self) -> GString {
        GString::from(self.last_map_error.as_deref().unwrap_or(""))
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
        // The loadout placement flag is per load (H05 T4).
        self.generated_loadout_placed = false;
        // Take the registry out so the generator can borrow it while the sim is
        // rebuilt, then hand it back (content boot is expensive; never drop it).
        let Some(mut registry) = self.content_snapshot.take() else {
            log::warn!("load_sector: content registry unavailable");
            return false;
        };
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
        let generated_sector = if loaded.is_none() {
            // H05 T2: the campaign primes the live threat/enemy-base flags;
            // the static fallback reproduces `updateBaseCoverage` for an
            // unsaved cell (`SectorGenExtras`).
            let extras = match self.pending_gen_extras.take() {
                Some((planet, id, extras)) if planet == planet_name && id == sector as u16 => {
                    extras
                }
                other => {
                    self.pending_gen_extras = other;
                    mind_core::game::planet::static_gen_extras(
                        &registry,
                        planet_name,
                        sector as u16,
                    )
                }
            };
            mind_core::maps::planet::generate_sector(planet_name, sector as u16, &registry, extras)
        } else {
            None
        };
        // The generator survives the grid/rules move so T4-T6 can run its
        // structure passes on the live sim.
        let mut generated_ctx: Option<(
            Box<dyn mind_core::maps::generators::WorldGenerator>,
            mind_core::maps::generators::planet::SectorRect,
            usize,
        )> = None;
        let loaded = match loaded {
            Some(loaded) => loaded,
            None => match generated_sector {
                Some(generated) => {
                    let mind_core::maps::planet::GeneratedSector {
                        grid,
                        rules,
                        generator,
                        rect,
                        sides,
                    } = generated;
                    generated_ctx = Some((generator, rect, sides));
                    LoadedGrid {
                        grid,
                        rules: Some(rules),
                        pending_buildings: Vec::new(),
                        plans: Vec::new(),
                        save: None,
                    }
                }
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
        let resume_from_save = resume_file && loaded.save.is_some();
        // H05 T3: a generated sector is bounded by its projected polygon
        // (`World.getDarkness` sector term, `World.java:505-529`); presets and
        // saves of preset sectors keep the square map. A resumed generated
        // sector needs the same polygon as a fresh generation. Only
        // `(rotation, sides)` are decided here: the size is the loaded grid's.
        let polygon = if let Some((_, rect, sides)) = &generated_ctx {
            Some((rect.rotation, *sides as i32))
        } else if resume_from_save
            && let Some(def) = registry.planet_by_name(planet_name)
            && def
                .sectors
                .get(sector as usize)
                .is_some_and(|cell| cell.preset.is_none())
        {
            let planet_grid =
                mind_core::render::g3d::grid::PlanetGrid::create(def.sector_tiles as usize);
            let rect = mind_core::maps::generators::planet::make_rect(
                &planet_grid,
                sector as usize,
                def.radius,
            );
            let sides = planet_grid
                .tiles
                .get(sector as usize)
                .map(|tile| tile.corners.len())
                .unwrap_or(0);
            Some((rect.rotation, sides as i32))
        } else {
            None
        };
        // H05 T4: a generated sector places its launch loadout during the world
        // load (`Schematics.placeLaunchLoadout` runs inside `generate()`;
        // T5/T6 add the enemy base and ruins to the same pass). The closure
        // runs from the shared tail once the fresh sim is live.
        let generated_loadout: Option<GeneratedLoadout> =
            generated_ctx.take().map(|(generator, _, _)| {
                let planet = planet_name.to_owned();
                Box::new(move |host: &mut MindSimHost| {
                    let mut generator = generator;
                    if let Some(loadout) = host.resolve_generated_loadout(&planet) {
                        let bases = host
                            .sim
                            .ecs
                            .0
                            .get_resource::<mind_core::game::runtime::BaseCatalog>()
                            .map(|catalog| catalog.0.clone());
                        let mut rules = host.sector_rules.take().unwrap_or_default();
                        // `state.rules.defaultTeam` (`Schematics.placeLaunchLoadout`):
                        // the fresh sim has no `CampaignRuntime`/`BuildRules` yet, so
                        // the load's own rules carry the team.
                        let player_team = rules.default_team;
                        let wave_team = rules.wave_team;
                        if let Some(content) = host.content_snapshot.as_ref() {
                            let mut ctx = mind_core::maps::generators::planet::StructureCtx {
                                sim: &mut host.sim,
                                content,
                                bases: bases.as_deref(),
                                loadout: Some(&loadout),
                                rules: &mut rules,
                                player_team,
                                wave_team,
                            };
                            generator.place_structures(&mut ctx);
                            host.generated_loadout_placed = true;
                        }
                        host.sector_rules = Some(rules);
                    }
                    host.pending_launch = None;
                    host.pending_launch_core = None;
                    host.sim.refresh_unit_runtime();
                }) as GeneratedLoadout
            });
        let map_name = sector_map_name(&registry, planet_name, sector);
        let loaded_ok = self.finish_world_load(
            registry,
            loaded,
            WorldLoadExtras {
                polygon,
                generated_loadout,
                resume: resume_from_save,
            },
        );
        if loaded_ok {
            // `World.setSectorRules` runs after a resume too (`World.java:266,292,300`).
            self.sim.set_map_name(map_name);
            log::info!("loaded campaign sector `{planet_name}:{sector}`");
        }
        loaded_ok
    }

    /// Shared world-load tail of `load_sector_world`/`load_map_world`
    /// (`World.loadMap`'s building half): swaps the grid into a fresh `Sim`,
    /// installs the live runtimes and materializes the map's buildings. The
    /// sector-only steps (polygon, launch loadout, resume metadata) arrive via
    /// [`WorldLoadExtras`] so the two callers cannot drift.
    fn finish_world_load(
        &mut self,
        registry: ContentRegistry,
        loaded: LoadedGrid,
        extras: WorldLoadExtras,
    ) -> bool {
        let resume_from_save = extras.resume && loaded.save.is_some();
        let LoadedGrid {
            grid,
            rules,
            pending_buildings,
            plans,
            save,
        } = loaded;
        self.sector_spawns = count_spawns(&grid, &registry);
        self.sector_rules = rules;
        self.sector_plans = plans;
        // `Vars.bases` (PAR-0139): load before the registry moves into the
        // content snapshot; the resource is inserted once the sim exists.
        let base_catalog = load_base_catalog(&registry);
        self.content_snapshot = Some(registry);

        // Reuse a fresh sim and swap in the loaded/generated grid; a new
        // iteration keeps no ECS entities from the previous world.
        let mut sim = Sim::new(1, grid.width(), grid.height(), BlockId::AIR, BlockId::AIR);
        sim.grid = grid;
        // `World.loadMap`'s `FilterContext` seed (recorded from
        // `decode_map_bytes`; surfaced in the state dump as `map_gen_seed`).
        if let Some(seed) = save.as_ref().and_then(|save| save.map_gen_seed) {
            sim.set_map_gen_seed(seed);
        }
        if let Some((rotation, sides)) = extras.polygon {
            sim.grid.sector_polygon = Some(mind_core::world::SectorPolygon {
                rotation,
                sides,
                size: sim.grid.tiles.width,
            });
        }
        let _ = sim.set_phase(mind_core::game::State::Playing);
        self.runner =
            FixedStepRunner::for_rate(sim.config().fixed_hz, sim.config().max_ticks_per_frame);
        self.sim = sim;
        self.player = None;
        // I07 T4: give the fresh sim the mod-extended block table before any
        // entity/placement work (`Blocks::new` is vanilla-only).
        if let Some(registry) = self.content_snapshot.as_ref()
            && let Err(error) = self.sim.sync_blocks_from_content(registry)
        {
            log::warn!("load_sector: mod block table sync failed: {error}");
        }
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
        // H05 T4: the generated-sector launch loadout (`load_sector_world`).
        if let Some(generated_loadout) = extras.generated_loadout {
            generated_loadout(self);
        }
        // `World.loadMap`'s building half: the reader decoded the map's tile
        // entities into the pending queue while the grid was built, so spawn
        // the ECS buildings (cores included) the campaign runtime/teams read.
        let materialized = self.sim.materialize_map_buildings(&pending_buildings);
        if materialized > 0 {
            log::info!("world load: materialized {materialized} map building(s)");
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
                static_fog: save.static_fog.clone(),
            });
        }
        // The fresh `Sim` carries no IO executor; re-wire the seams so a queued
        // `request_save`/`request_load` drains at the next `IoSet` boundary.
        self.install_sim_seams();
        self.sim.reset_logic_presentation();
        // Map locales (`state.mapLocales`, from the save/map `locales` meta
        // tag; H06 T4). Preset maps without the tag fall back to the global
        // bundle in `run_locale_print`.
        let map_locales = save
            .as_ref()
            .and_then(|save| save.tags.get("locales"))
            .and_then(|text| {
                mind_core::io::json::JsonIo::read::<mind_core::io::json::MapLocales>(text).ok()
            })
            .unwrap_or_default();
        if let Some(mut text) = self
            .sim
            .ecs
            .0
            .get_resource_mut::<mind_core::logic::world::LogicText>()
        {
            text.map_locales = map_locales;
        }
        self.world_dirty = true;
        self.world_loads += 1;
        self.emit_state();
        self.emit_world_changed();
        self.emit_world_loaded();
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

    /// Applies a decoded `static-fog-data` chunk to the live `FogControl`
    /// (campaign resume path; `FogControl.read`). Builds the controller from the
    /// live rules first when the world load has not run yet. `false` when the
    /// rules keep fog off.
    pub fn apply_static_fog(&mut self, bytes: Vec<u8>) -> bool {
        if self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::fog::FogControl>()
            .is_none()
        {
            self.sim.reset_fog();
        }
        let Some(mut fog) = self
            .sim
            .ecs
            .0
            .get_resource_mut::<mind_core::game::fog::FogControl>()
        else {
            return false;
        };
        fog.read(&bytes);
        true
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

    /// Distinct teams holding a core in the live world
    /// (`state.teams.getActive().count(TeamData::hasCore)`, `World.java:369-373`).
    pub fn core_teams(&self) -> Vec<u8> {
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let mut teams: Vec<u8> = self
            .sim
            .ecs
            .entities_by_seq()
            .into_iter()
            .filter(|(_, _, comp)| {
                content
                    .block(comp.block)
                    .is_some_and(|def| def.kind == BlockKind::CoreBlock)
            })
            .map(|(_, _, comp)| comp.team.0)
            .collect();
        teams.sort_unstable();
        teams.dedup();
        teams
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

    /// The installed runtime's `universe.secondsf()` (read-only; `None` with
    /// no campaign sector installed).
    pub fn campaign_universe_secondsf(&self) -> Option<f32> {
        self.sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| runtime.campaign.universe.secondsf(false))
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

    /// `UnitComp.afterRead` (`UnitComp.java:585-593`): re-derives the
    /// controllers of the units the last world load restored, now that the
    /// session rules are installed (H04 T13). Rust-only seam for
    /// `MindCampaign.start_sector`'s resume path.
    pub fn reset_loaded_unit_controllers(&mut self) {
        self.sim.reset_loaded_controllers();
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

    /// Playtest seam: starts a fire at `(x, y)` (`Fires.create`) through the
    /// live unit runtime rng. `false` when fire is disabled or there is no
    /// runtime.
    #[func]
    pub fn debug_create_fire(&mut self, x: i32, y: i32) -> bool {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        let Some(mut runtime) = self
            .sim
            .ecs
            .0
            .remove_non_send::<mind_core::sim::unit_runtime::UnitRuntime>()
        else {
            return false;
        };
        let created =
            mind_core::combat::fires::create(&mut self.sim.ecs.0, tx, ty, &mut runtime.rng)
                .is_some();
        self.sim.ecs.0.insert_non_send(runtime);
        created
    }

    /// Playtest seam: deposits `amount` of `liquid` at `(x, y)`
    /// (`Puddles.deposit`) through the live unit runtime rng. `false` when the
    /// liquid is unknown, the deposit reacts/vaporizes, or there is no runtime.
    #[func]
    pub fn debug_deposit_puddle(&mut self, x: i32, y: i32, liquid: GString, amount: f64) -> bool {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        let Some(id) = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.liquid_id(liquid.to_string().as_str()))
        else {
            return false;
        };
        let Some(mut runtime) = self
            .sim
            .ecs
            .0
            .remove_non_send::<mind_core::sim::unit_runtime::UnitRuntime>()
        else {
            return false;
        };
        let Some(content) = self.content_snapshot.as_ref() else {
            self.sim.ecs.0.insert_non_send(runtime);
            return false;
        };
        let created = mind_core::combat::puddles::deposit(
            &mut self.sim.ecs.0,
            &self.sim.grid.tiles,
            content,
            tx,
            ty,
            id,
            amount as f32,
            &mut runtime.rng,
        )
        .is_some();
        self.sim.ecs.0.insert_non_send(runtime);
        created
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
    /// `(axis_x, axis_y, shooting, aim_x, aim_y, boosting, mouse_move)`.
    /// `MindInput.player_control` shares the `move_x`/`move_y` axis; with a
    /// player unit it drives the unit instead of the camera
    /// (`DesktopInput.updateMovement`).
    fn player_control(&self) -> (f32, f32, bool, f32, f32, bool, bool) {
        if self.input_locked {
            // `InputHandler.locked()`: movement and firing are gated off
            // (PAR-0100).
            return (0.0, 0.0, false, 0.0, 0.0, false, false);
        }
        let Some(input) = self.input.clone() else {
            return (0.0, 0.0, false, 0.0, 0.0, false, false);
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
                .set_player_control(0.0, 0.0, false, 0.0, 0.0, false, false);
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

    /// `player.unit().plans.size` (`BuilderComp.plans`; the `buildautopause`
    /// auto-pause/resume and the deselect-mining gate).
    pub fn player_plan_count(&self) -> usize {
        self.sim.player_plan_count()
    }

    /// `player.unit().type.canBoost` and the unit is alive
    /// (`DesktopInput.java:300-311`): the command-mode gate when `commandMode`
    /// and `boost` share one key.
    pub fn player_unit_can_boost(&self) -> bool {
        use mind_core::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
        let Some(entity) = self.sim.player_unit() else {
            return false;
        };
        if self
            .sim
            .ecs
            .0
            .get::<UnitCore>(entity)
            .is_some_and(|core| core.dead)
        {
            return false;
        }
        self.sim
            .ecs
            .0
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| {
                self.content_registry()
                    .and_then(|content| content.unit(comp.type_id))
            })
            .is_some_and(|def| def.can_boost)
    }

    /// `build.isCommandable()` own commandable buildings (`DI:346-357`).
    pub fn commandable_buildings(&self) -> Vec<(i16, i16)> {
        self.sim.commandable_buildings(self.sim.live_default_team())
    }

    /// `InputHandler.tryRepairDerelict` (H08 T11): queue a team-0 building in
    /// the rect/cursor for repair (the click and rebuild-select paths).
    pub fn try_repair_derelict(&mut self, x: i32, y: i32) -> bool {
        self.sim.try_repair_derelict(x, y)
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

    /// `state.isCampaign()` (`GameState.java:60-62`: `rules.sector != null`)
    /// for the config panels' `unlockedNow()` short-circuit
    /// (`UnlockableContent.java:281-283`): outside a campaign every content
    /// option is unlocked.
    pub fn in_campaign(&self) -> bool {
        self.sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .is_some_and(|runtime| runtime.session.sector.is_some())
    }

    /// `state.getPlanet()` (`GameState.java:85-87`) for the config panels'
    /// `isOnPlanet` filter: the campaign session's sector planet, else
    /// `rules.planet`'s default (`Rules.java:249`, `serpulo`).
    pub fn config_planet(&self) -> Option<mind_core::content::PlanetId> {
        if let Some(runtime) = self.sim.ecs.0.get_resource::<CampaignRuntime>()
            && let Some((planet, _)) = runtime.session.sector
        {
            return Some(planet);
        }
        self.content_snapshot
            .as_ref()
            .and_then(|content| content.planet_id("serpulo"))
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
        // Fires and puddles light the map (`FireComp.draw` -> `Drawf.light`,
        // `Liquid.drawPuddle`; H03 T7).
        let ticks = godot::classes::Time::singleton().get_ticks_msec() as f32 * 0.06;
        for fire in self.fire_draws(rect) {
            out.push(LightSource {
                x: fire.x,
                y: fire.y,
                radius: 50.0 + mind_core::render::drawf::absin(ticks, 5.0, 5.0),
                color: pal::LIGHT_FLAME,
                opacity: 0.6 * fire.alpha,
            });
        }
        for puddle in self.puddle_draws(rect) {
            if let Some((color, opacity)) = puddle.light {
                out.push(LightSource {
                    x: puddle.x,
                    y: puddle.y,
                    radius: 30.0 * puddle.f,
                    color,
                    opacity,
                });
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

    /// `BeamNode.BeamNodeBuild.draw` links (`BeamNode.java:207-233`; I02 T5):
    /// one row per drawn beam, already inset by `tilesize / 2` on the source
    /// (by `size * d4`) and destination (by `d4`) sides, with the source
    /// graph's satisfaction for `setupColor`.
    pub fn beam_node_lasers(&self, rect: (f32, f32, f32, f32)) -> Vec<BeamLaserLink> {
        use mind_core::entities::comp::Building;
        use mind_core::world::blocks::power::nodes::BeamNodeState;
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
            if def.kind != BlockKind::BeamNode {
                continue;
            }
            let Some(entity) = self.sim.grid.tile(tile.x, tile.y).build else {
                continue;
            };
            // `team == Team.derelict` never draws (`BeamNode.java:213`).
            if self.building_team(tile.x, tile.y) == Some(0) {
                continue;
            }
            let Some(state) = self.sim.ecs.0.get::<BeamNodeState>(entity).copied() else {
                continue;
            };
            let satisfaction = self
                .sim
                .ecs
                .0
                .get::<PowerModule>(entity)
                .and_then(|module| {
                    self.sim
                        .ecs
                        .0
                        .get_resource::<PowerGrids>()
                        .and_then(|grids| grids.graph(module.graph))
                        .map(|graph| graph.get_satisfaction())
                })
                .unwrap_or(1.0);
            let (x1, y1) = building_center(tile.x, tile.y, def);
            // `Geometry.d4`: right, up, left, down.
            const D4: [(f32, f32); 4] = [(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)];
            for (i, link) in state.links.iter().enumerate() {
                let Some(other) = *link else {
                    continue;
                };
                let Some(other_build) = self.sim.ecs.0.get::<Building>(other) else {
                    continue;
                };
                let Some(other_def) = content.block(other_build.block) else {
                    continue;
                };
                let (ox, oy) = building_center(
                    i32::from(other_build.tile.x()),
                    i32::from(other_build.tile.y()),
                    other_def,
                );
                // Don't draw lasers for adjacent blocks (`dst > 1 + size/2`).
                let dst = (i32::from(other_build.tile.x()) - tile.x)
                    .abs()
                    .max((i32::from(other_build.tile.y()) - tile.y).abs());
                if dst <= 1 + def.size / 2 {
                    continue;
                }
                // Beam-to-beam dedupe (`BeamNode.java:219-221`): the larger
                // range draws; equal ranges break the tie by entity index.
                if other_def.kind == BlockKind::BeamNode {
                    let other_range = self
                        .sim
                        .ecs
                        .0
                        .get::<BeamNodeState>(other)
                        .map(|state| state.range)
                        .unwrap_or(0);
                    if state.range < other_range
                        || (state.range == other_range && entity.index() < other.index())
                    {
                        continue;
                    }
                }
                let (dx, dy) = D4[i % 4];
                let poff = 4.0;
                out.push(BeamLaserLink {
                    x1: x1 + poff * def.size as f32 * dx,
                    y1: y1 + poff * def.size as f32 * dy,
                    x2: ox - poff * dx,
                    y2: oy - poff * dy,
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

    /// Debug probe: `{name, x, y, team, hp_fract}` rows for every live unit
    /// (render verification; companion to `tile_debug`).
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
                    "hp_fract": unit.healthf,
                })
            })
            .collect();
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// Debug probe (J04 T1): the turret's resolved targeting state at `(x, y)`
    /// — `{sort, min_range, tracking_range, target_interval,
    /// new_target_interval, target}` where `target` is `{seq, name, x, y}` or
    /// null. Companion to `tile_debug`.
    #[func]
    pub fn turret_debug(&self, x: i32, y: i32) -> Dictionary<GString, Variant> {
        use mind_core::combat::targeting::UnitSort;
        use mind_core::world::blocks::defense::turrets::TurretState;

        let mut d = Dictionary::new();
        let Some(tile) = self.sim.grid.tiles.getn(x, y) else {
            return d;
        };
        let Some(entity) = tile.build else {
            return d;
        };
        let Some(state) = self.sim.ecs.0.get::<TurretState>(entity) else {
            return d;
        };
        let config = &state.config;
        d.set(
            "sort",
            &GString::from(match config.unit_sort {
                UnitSort::Closest => "closest",
                UnitSort::Farthest => "farthest",
                UnitSort::Strongest => "strongest",
                UnitSort::Weakest => "weakest",
                UnitSort::Grouped(_) => "grouped",
            })
            .to_variant(),
        );
        d.set("min_range", &config.min_range.to_variant());
        d.set("tracking_range", &config.tracking_range.to_variant());
        d.set("target_interval", &config.target_interval.to_variant());
        d.set(
            "new_target_interval",
            &config.new_target_interval.to_variant(),
        );
        d.set("total_ammo", &state.total_ammo.to_variant());
        d.set("ammo_entries", &(state.ammo.len() as i64).to_variant());
        d.set("max_ammo", &config.max_ammo.to_variant());
        let accepts_copper = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.item_id("copper"))
            .is_some_and(|item| {
                mind_core::world::blocks::defense::turrets::accept_item(
                    &self.sim.ecs.0,
                    entity,
                    item,
                )
            });
        d.set("accepts_copper", &accepts_copper.to_variant());
        match state.target {
            Some(target) => {
                let mut row = Dictionary::<GString, Variant>::new();
                row.set(
                    "seq",
                    &(self.sim.ecs.seq_of(target).unwrap_or(u64::MAX) as i64).to_variant(),
                );
                let name = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::UnitTypeComp>(target)
                    .and_then(|comp| {
                        self.content_snapshot
                            .as_ref()
                            .and_then(|content| content.unit(comp.type_id))
                    })
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                row.set("name", &GString::from(name.as_str()).to_variant());
                if let Some(pos) = self.sim.ecs.0.get::<mind_core::entities::comp::Pos>(target) {
                    row.set("x", &pos.x.to_variant());
                    row.set("y", &pos.y.to_variant());
                }
                d.set("target", &row.to_variant());
            }
            None => d.set("target", &Variant::nil()),
        }
        d
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

    /// Debug/playtest seam: adds `amount` of `liquid` to the building at (x, y)
    /// (clamped to `Block.liquidCapacity`); returns the amount accepted.
    #[func]
    pub fn debug_add_building_liquid(
        &mut self,
        x: i32,
        y: i32,
        liquid: GString,
        amount: f64,
    ) -> f64 {
        let Some(entity) = self.sim.grid.tiles.getn(x, y).and_then(|tile| tile.build) else {
            return 0.0;
        };
        let Some(liquid) = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.liquid_id(&liquid.to_string()))
        else {
            return 0.0;
        };
        let capacity = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::world::block::BlockTable>()
            .and_then(|table| {
                self.sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::Building>(entity)
                    .map(|building| building.block)
                    .and_then(|block| table.get(block).map(|inst| inst.def.liquid_capacity))
            })
            .unwrap_or(0.0);
        match self
            .sim
            .ecs
            .0
            .get_mut::<mind_core::world::modules::LiquidModule>(entity)
        {
            Some(mut module) => f64::from(module.add(liquid, amount as f32, capacity)),
            None => 0.0,
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
        if schematic_index < 0 || self.content_snapshot.is_none() {
            return false;
        }
        let schematics = self.load_user_schematics();
        let Some(schem) = schematics.all.get(schematic_index as usize).cloned() else {
            log::warn!("place_launch_loadout: no schematic at index {schematic_index}");
            return false;
        };
        self.place_loadout_schematic(&schem, stacks_json)
    }

    /// `place_launch_loadout` for the accelerator's single-core loadout
    /// (`Universe.updateLoadout(CoreBlock)`, Universe.java:118-121) on a
    /// preset map's fresh launch.
    #[func]
    pub fn place_launch_core(&mut self, core: GString, stacks_json: GString) -> bool {
        let Some(schem) = self.single_core_schematic(&core.to_string()) else {
            log::warn!("place_launch_core: unknown core `{core}`");
            return false;
        };
        self.place_loadout_schematic(&schem, stacks_json)
    }

    /// `Schematics.placeLaunchLoadout` body shared by the schematic and
    /// single-core loadouts.
    fn place_loadout_schematic(
        &mut self,
        schem: &mind_core::game::schematic::Schematic,
        stacks_json: GString,
    ) -> bool {
        let Some(registry) = self.content_snapshot.as_ref() else {
            return false;
        };
        let team = self.player_team();
        let Some((core_x, core_y)) = self.player_core_tile(team) else {
            log::warn!("place_launch_loadout: no default-team core to replace");
            return false;
        };
        let cores = self.sim.place_loadout(schem, core_x, core_y, team, true);
        let Some(entity) = cores.first().copied() else {
            log::warn!("place_launch_loadout: schematic placement produced no core");
            return false;
        };
        // `placeLaunchLoadout`: `tile.build.items.add(universe.getLaunchResources())`
        // on the freshly placed core (empty inventory).
        let stacks: Vec<mind_core::io::json::content_serde::JsonItemStack> =
            serde_json::from_str(&stacks_json.to_string()).unwrap_or_default();
        let resolved: Vec<(mind_core::content::ItemId, i32)> = stacks
            .iter()
            .filter_map(|stack| {
                let name = stack.item.as_deref()?;
                Some((registry.item_id(name)?, stack.amount))
            })
            .collect();
        mind_core::sim::Sim::stock_core(&mut self.sim, entity, &resolved);
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

    /// `updatePick`'s `tryBlock`/`tryConfig` (`PlacementFragment.java:141-143`):
    /// the hovered building's block (a `ConstructBuild` resolves to its
    /// `current`) and its config, copied only for `copyConfig` blocks
    /// (`ItemBridge.copyConfig = false`, `ItemBridge.java:60`). Rust-only seam
    /// for the middle-click pick.
    pub fn pick_target_at(
        &self,
        x: i32,
        y: i32,
    ) -> Option<(BlockId, Option<mind_core::world::config::ConfigValue>)> {
        let entity = self.tile_at(x, y)?.build?;
        let block = self
            .sim
            .ecs
            .0
            .get::<mind_core::entities::comp::Building>(entity)
            .map(|building| building.block)?;
        let effective = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::ConstructState>(entity)
            .map_or(block, |state| state.current);
        let def = self.content_registry()?.block(block)?;
        let copy = !matches!(
            def.kind,
            BlockKind::ItemBridge | BlockKind::BufferedItemBridge
        );
        let config = if copy {
            let value = mind_core::world::config::read_config(&self.sim.ecs.0, entity);
            (!matches!(value, mind_core::world::config::ConfigValue::None)).then_some(value)
        } else {
            None
        };
        Some((effective, config))
    }

    /// `updatePick`'s plan override (`PlacementFragment.java:145-154`): the
    /// player unit's queued non-breaking plan whose `Block.bounds(plan.x,
    /// plan.y)` contains the world point, with its carried config.
    pub fn player_plan_at_world(
        &self,
        wx: f32,
        wy: f32,
    ) -> Option<(BlockId, mind_core::world::config::ConfigValue)> {
        let registry = self.content_registry()?;
        for plan in self.sim.player_build_plans() {
            if plan.breaking || plan.block == BlockId::AIR {
                continue;
            }
            let Some(def) = registry.block(plan.block) else {
                continue;
            };
            // `Block.bounds(x, y)`: centre `x*tilesize + offset`, edge
            // `size*tilesize` (`Block.java` `bounds`; tilesize 8).
            let offset = ((def.size + 1) % 2) as f32 * 4.0;
            let half = def.size as f32 * 4.0;
            let cx = f32::from(plan.x) * 8.0 + offset;
            let cy = f32::from(plan.y) * 8.0 + offset;
            if wx >= cx - half && wx < cx + half && wy >= cy - half && wy < cy + half {
                return Some((plan.block, plan.config.clone()));
            }
        }
        None
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
                let treads = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::TankComp>(*entity)
                    .map(|tank| TreadsDraw {
                        tread_time: tank.tread_time,
                        rects: def.tread_rects.clone(),
                        frames: def.tread_frames.max(1),
                    });
                let legs = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::LegsComp>(*entity)
                    .map(|comp| LegsDraw {
                        base_rotation: comp.base_rotation,
                        legs: comp
                            .legs
                            .iter()
                            .map(|leg| mind_core::render::unit_draw::LegDrawLeg {
                                foot: (leg.foot_x, leg.foot_y),
                                joint: (leg.joint_x, leg.joint_y),
                                moving: leg.moving,
                                stage: leg.step,
                            })
                            .collect(),
                        // `UnitType.drawLegs` reads `1 - unit.drownTime`; the
                        // port has no drowning yet (`UnitCore.drown_time`).
                        inv_drown: 1.0,
                        leg_base_offset: def.leg_base_offset,
                        leg_straightness: def.leg_straightness,
                        base_leg_straightness: def.base_leg_straightness,
                        leg_straight_length: def.leg_straight_length,
                        leg_extension: def.leg_extension,
                        leg_base_under: def.leg_base_under,
                        shadow_elevation: def.shadow_elevation,
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
                                    def: weapon.clone(),
                                    mount: mount
                                        .cloned()
                                        .unwrap_or_else(|| WeaponMount::new(weapon)),
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
                // H03 T10: active status effects with their view parameters
                // (`StatusEffect.effect/effectChance/color`).
                let statuses = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::StatusComp>(*entity)
                    .map(|comp| {
                        comp.statuses
                            .iter()
                            .filter_map(|entry| content.status(entry.effect))
                            .filter(|def| {
                                def.effect != mind_core::content::EffectId::NONE
                                    && def.effect_chance > 0.0
                            })
                            .map(|def| (def.effect, def.effect_chance, def.color))
                            .collect()
                    })
                    .unwrap_or_default();
                // H03 T10: floor under the unit (`floorOn()`).
                let (floor_is_liquid, floor_color) = {
                    let tile = self
                        .sim
                        .grid
                        .tiles
                        .getn(WorldGrid::to_tile(pos.x), WorldGrid::to_tile(pos.y));
                    let floor = tile.and_then(|tile| content.block(tile.floor));
                    (
                        floor.map(|def| def.is_liquid).unwrap_or(false),
                        floor
                            .and_then(|def| def.map_color)
                            .unwrap_or(mind_core::content::Rgba::WHITE),
                    )
                };
                let fin = self
                    .sim
                    .ecs
                    .0
                    .get::<mind_core::entities::comp::unit::comp::TimedKillComp>(*entity)
                    .map(|timed| {
                        if timed.lifetime > 0.0 {
                            (timed.time / timed.lifetime).clamp(0.0, 1.0)
                        } else {
                            0.0
                        }
                    })
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
                    legs,
                    treads,
                    weapons,
                    parts: def.parts.clone(),
                    fin,
                    draw_body: def.draw_body,
                    draw_cell: def.draw_cell,
                    ground_layer: def.ground_layer,
                    id,
                    mine_tile,
                    item,
                    is_player,
                    health,
                    statuses,
                    grounded: core.elevation < 0.001,
                    hovering: def.hovering,
                    emit_walk_effect: def.emit_walk_effect,
                    floor_is_liquid,
                    floor_color,
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
        // H07 T11: continuous-weapon `SoundLoop`s (vela/navanax) append to the
        // same per-key fade table.
        mind_core::audio::frame::collect_weapon_loops(&self.sim.ecs.0, out);
    }

    /// T7: in-game music context state (rules/wave/boss/core hp; Rust-only).
    pub fn audio_game_state(&self) -> mind_core::audio::frame::AudioGameState {
        let Some(content) = self.sim.block_runtime().map(|runtime| runtime.content()) else {
            return Default::default();
        };
        mind_core::audio::frame::read_game_state(&self.sim.ecs.0, content)
    }

    /// H07 T6: one view per live `WeatherRuntime`, sorted by `EntitySeq`
    /// (renderer input; Rust-only).
    pub fn weather_views(&self, out: &mut Vec<mind_core::fx::WeatherStateView>) {
        out.clear();
        let mut rows: Vec<(u64, mind_core::fx::WeatherStateView)> = self
            .sim
            .ecs
            .0
            .iter_entities()
            .filter_map(|entity| {
                let state = entity.get::<mind_core::entities::comp::weather::WeatherRuntime>()?;
                let seq = entity
                    .get::<mind_core::ecs::EntitySeq>()
                    .map_or(u64::MAX, |seq| seq.0);
                Some((
                    seq,
                    mind_core::fx::WeatherStateView {
                        weather: state.weather,
                        intensity: state.intensity,
                        opacity: state.opacity,
                        wind_vector: (state.wind_x, state.wind_y),
                        life: state.life,
                    },
                ))
            })
            .collect();
        rows.sort_by_key(|(seq, _)| *seq);
        out.extend(rows.into_iter().map(|(_, view)| view));
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
        mind_core::entities::comp::weather::create_weather(
            &mut self.sim.ecs.0,
            seq,
            weather,
            intensity,
            duration,
            (angle.cos(), angle.sin()),
        );
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

    /// [`Self::unit_factory_config`] as `"[plan, command]"` (`command` = `-1`
    /// unset); `"null"` without a factory build. Probe/UI seam.
    #[func]
    pub fn factory_command_json(&self, x: i32, y: i32) -> GString {
        match self.unit_factory_config(x, y) {
            Some((plan, command)) => {
                GString::from(format!("[{plan},{}]", command.map_or(-1, i64::from)).as_str())
            }
            None => GString::from("null"),
        }
    }

    /// `MessageBlock.maxTextLength`/`maxNewlines` for the block's live
    /// instance (`MessageBlock.java:29-30`; the port's `MessageDef`); `None`
    /// for other kinds.
    pub fn message_limits(&self, block: BlockId) -> Option<(i32, i32)> {
        let table = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::world::block::BlockTable>()?;
        match &table.get(block)?.kind_data {
            mind_core::world::block_kind_data::BlockKindData::Message(message) => {
                Some((message.max_text, message.max_newlines))
            }
            _ => None,
        }
    }

    /// `UnitFactoryBuild` config state for the plan grid + command row (J05
    /// T2): the selected plan index (`-1` = none) and the queued command
    /// override (`UnitFactory.java:227`).
    pub fn unit_factory_config(&self, x: i32, y: i32) -> Option<(i32, Option<u16>)> {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let entity = self.sim.grid.entity_at(TilePos::new(tx, ty))?;
        let state = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::blocks::units::UnitFactoryBuild>(entity)?;
        Some((
            state.current_plan,
            state.command.map(mind_core::content::UnitCommandId::raw),
        ))
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

    /// `Door.DoorBuild.tapped`'s 60-tick timer (`Door.java:149`):
    /// `origin().timer(timerToggle, 60f)`. Returns whether the tap may toggle;
    /// consumes the origin door's slot when it fires. The caller checks
    /// `building_has_unit_overlap` first (Java short-circuit order).
    #[func]
    pub fn door_tap_allowed(&mut self, x: i32, y: i32) -> bool {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        let pos = mind_core::world::TilePos::new(tx, ty);
        let Some(entity) = self.sim.grid.entity_at(pos) else {
            return false;
        };
        mind_core::world::behavior::defense::tap_allowed(&mut self.sim.ecs.0, entity)
    }

    /// The accelerator building at tile `(x, y)` (any tile of its footprint).
    fn accelerator_at(&self, x: i32, y: i32) -> Option<bevy_ecs::entity::Entity> {
        let (Ok(tx), Ok(ty)) = (i16::try_from(x), i16::try_from(y)) else {
            return None;
        };
        let entity = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))?;
        let block = self
            .sim
            .ecs
            .0
            .get::<mind_core::entities::comp::Building>(entity)?
            .block;
        let kind = self.content_snapshot.as_ref()?.block(block)?.kind;
        (kind == mind_core::content::registries::blocks::BlockKind::Accelerator).then_some(entity)
    }

    /// `AcceleratorBuild.buildConfiguration` (Accelerator.java:258-291) for the
    /// tap path: `""` for a non-accelerator; `{"accelerator": true, "open":
    /// false}` = `deselect()` with no panel (`!canLaunch()`); `"open": true`
    /// = open the planet-launch picker (`ui.planet.showPlanetLaunch`) for the
    /// current sector - `Trigger.acceleratorUse` has fired. `InputHandler`'s
    /// `config_spec` should route accelerators here instead of the item panel.
    #[func]
    pub fn accelerator_config_spec(&mut self, x: i32, y: i32) -> GString {
        let Some(entity) = self.accelerator_at(x, y) else {
            return GString::new();
        };
        let open = self.sim.accelerator_config_tapped(entity);
        let (anchor_x, anchor_y) = self
            .sim
            .ecs
            .0
            .get::<mind_core::entities::comp::Building>(entity)
            .map(|building| (i32::from(building.tile.x()), i32::from(building.tile.y())))
            .unwrap_or((x, y));
        GString::from(
            serde_json::json!({
                "accelerator": true,
                "open": open,
                "x": anchor_x,
                "y": anchor_y,
            })
            .to_string()
            .as_str(),
        )
    }

    /// The whole accelerator tap (`buildConfiguration`, Accelerator.java:
    /// 258-291) for `InputHandler`: returns false for a non-accelerator;
    /// otherwise the tap is consumed with no config panel (`deselect()`), and
    /// when `canLaunch()` holds `Trigger.acceleratorUse` fires and the planet
    /// page opens in `planetLaunch` mode (`ui.planet.showPlanetLaunch`),
    /// deferred so no host borrow is held across the dialog build.
    #[func]
    pub fn accelerator_tapped(&mut self, x: i32, y: i32) -> bool {
        let spec = self.accelerator_config_spec(x, y).to_string();
        if spec.is_empty() {
            return false;
        }
        let open = serde_json::from_str::<serde_json::Value>(&spec)
            .ok()
            .and_then(|value| value.get("open")?.as_bool())
            .unwrap_or(false);
        if open && let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") {
            let value: serde_json::Value = serde_json::from_str(&spec).unwrap_or_default();
            let context = serde_json::json!({
                "mode": "planetLaunch",
                "x": value.get("x"),
                "y": value.get("y"),
            });
            ui.call_deferred(
                "open_dialog",
                &[
                    GString::from("planet").to_variant(),
                    GString::from(context.to_string().as_str()).to_variant(),
                ],
            );
        }
        true
    }

    /// `AcceleratorBuild.canLaunch()` at `(x, y)` (Accelerator.java:239-241).
    #[func]
    pub fn accelerator_can_launch(&self, x: i32, y: i32) -> bool {
        self.accelerator_at(x, y)
            .is_some_and(|entity| self.sim.accelerator_can_launch(entity))
    }

    /// The planet-launch picker's confirm on the accelerator at `(x, y)`
    /// (Accelerator.java:262-272): consumes the launch core, clears the stored
    /// items, drains the power buffer, resets `progress` and starts
    /// `launching`. Returns the carried resources `[{item, amount}]`, or `""`
    /// when `canLaunch()` no longer holds.
    #[func]
    pub fn accelerator_launch(&mut self, x: i32, y: i32) -> GString {
        let Some(entity) = self.accelerator_at(x, y) else {
            return GString::new();
        };
        let Some(resources) = self.sim.accelerator_launch(entity) else {
            return GString::new();
        };
        let Some(registry) = self.content_snapshot.as_ref() else {
            return GString::new();
        };
        let stacks: Vec<serde_json::Value> = resources
            .iter()
            .filter_map(|(item, amount)| {
                let name = registry.item(*item)?.name.clone();
                Some(serde_json::json!({"item": name, "amount": amount}))
            })
            .collect();
        self.world_dirty = true;
        self.emit_state();
        GString::from(serde_json::Value::Array(stacks).to_string().as_str())
    }

    /// `launching = false` right before the sector switch (Accelerator.java:274).
    #[func]
    pub fn accelerator_end_launch(&mut self, x: i32, y: i32) {
        if let Some(entity) = self.accelerator_at(x, y) {
            self.sim.accelerator_end_launch(entity);
        }
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
        // I07 T4: install over the host's content snapshot (base + mods) so
        // runtime placement/config lookups address the same registry the
        // command entry points do. Without a snapshot, fall back to vanilla.
        if !self.sim.has_block_runtime() {
            let result = match self.content_snapshot.as_ref() {
                Some(content) => self.sim.install_block_runtime_from(
                    mind_core::sim::runtime::SimRuntimeConfig::default(),
                    content.clone(),
                ),
                None => self.sim.install_block_runtime(),
            };
            if let Err(error) = result {
                log::warn!("live block runtime install failed: {error}");
                return false;
            }
        }
        // EV-0048: the live unit/combat runtime rides on the same content/world;
        // a failed install keeps buildings working but leaves units inert.
        if !self.sim.has_unit_runtime() {
            match self.content_snapshot.as_ref() {
                Some(content) => self.sim.install_unit_runtime_from(content.clone()),
                None => {
                    if let Err(error) = self.sim.install_unit_runtime() {
                        log::warn!("live unit runtime install failed: {error}");
                        return false;
                    }
                }
            }
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
            // A world-processor `setblock` mutated tiles (H06 T8).
            if self.sim.logic_world_changed {
                self.sim.logic_world_changed = false;
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

    /// Emits the `world_loaded` signal (`WorldLoadEvent`; `World.loadMap`).
    fn emit_world_loaded(&mut self) {
        let _ = self.base_mut().emit_signal("world_loaded", &[]);
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

    /// Full `DrawTurret.draw` inputs at `(x, y)` (J03 T1): the live
    /// `TurretState` counters (`DrawTurret.java:74-119`) plus the placed
    /// block's draw metadata (parts, heat color, liquid, elevation). `None`
    /// without a turret. Rust-only seam for the turret render pass.
    pub fn turret_draw_input(
        &self,
        x: i32,
        y: i32,
    ) -> Option<crate::fx::draw_turret::TurretDrawInput> {
        use mind_core::content::registries::blocks::turret_parts;
        use mind_core::world::blocks::defense::turrets::TurretState;

        let entity = self.tile_at(x, y)?.build?;
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(self.sim.grid.tile(x, y).block)?;
        let state = self.sim.ecs.0.get::<TurretState>(entity)?;
        let name = def.name.as_str();

        let len = -state.cur_recoil.max(0.0).powf(state.config.recoil_pow) * state.config.recoil;
        let (dx, dy) = mind_core::render::conveyor_items::trns(state.rotation, len, 0.0);

        // `TurretBuild.progress()` = `clamp(reloadCounter / reload)`.
        let progress = if state.config.reload > 0.0 {
            (state.reload_counter / state.config.reload).clamp(0.0, 1.0)
        } else {
            0.0
        };

        // `drawTurret`: `Drawf.liquid(liquid, ..., liquids.get(toDraw) /
        // block.liquidCapacity, toDraw.color, drawrot)`.
        let (liquid_fraction, liquid_color) = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::modules::LiquidModule>(entity)
            .map(|liquids| {
                let liquid = liquids.current_liquid();
                let capacity = def.liquid_capacity.max(1e-6);
                let color = content
                    .liquid(liquid)
                    .map(|def| def.color)
                    .unwrap_or(mind_core::content::Rgba::WHITE);
                (liquids.get(liquid) / capacity, color)
            })
            .unwrap_or((0.0, mind_core::content::Rgba::WHITE));

        // `TurretBuild.getAmmoContent()` for `ammoParts`: the last item entry
        // (`ItemTurretBuild.getAmmoContent`, `ItemTurret.java:172-174`).
        let ammo_item = state
            .ammo
            .last()
            .and_then(|entry| content.item(entry.item))
            .map(|item| item.name.as_str())
            .unwrap_or("");
        let ammo_parts = if ammo_item.is_empty() {
            Vec::new()
        } else {
            turret_parts::ammo_parts_for(name, ammo_item)
        };

        let center = ((def.size - 1) / 2) as f32;
        let size = mind_core::config::TILESIZE as f32;

        Some(crate::fx::draw_turret::TurretDrawInput {
            handle: 0,
            name: name.to_owned(),
            // `Building.x/y` = the footprint center (`Tile.drawx + Block.offset`);
            // the caller passes the first row-major tile, exactly like the block
            // pass's anchor.
            x: (x as f32 + center) * size + def.offset,
            y: (y as f32 + center) * size + def.offset,
            // `BaseTurretBuild.drawrot()` = `rotation - 90`.
            drawrot: state.rotation - 90.0,
            // `DrawPart.params` takes the raw rotation (`DrawTurret.java:101`).
            rotation: state.rotation,
            recoil_offset: (dx, dy),
            // `Turret.init`: `elevation = size / 2f` (no vanilla override).
            elevation: def.size as f32 / 2.0,
            heat: state.heat,
            warmup: state.shoot_warmup,
            cur_recoil: state.cur_recoil,
            cur_recoils: state.cur_recoils.to_vec(),
            charge: state.charge,
            progress,
            liquid_fraction,
            liquid_color,
            heat_color: turret_parts::heat_color(name),
            parts: turret_parts::parts_for(name),
            ammo_parts,
            turret_layer: mind_core::render::layer::Layer::Turret.z(),
            shadow_layer: mind_core::render::layer::Layer::Turret.z() - 0.5,
            heat_layer: mind_core::render::layer::Layer::TurretHeat.z(),
        })
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
            let Some(shield) = entity.get::<ShieldComp>() else {
                continue;
            };
            // `UnitType.draw`: `if(unit.shieldAlpha > 0) drawShield(unit)`
            // (`UnitType.java:1555-1556`; H03 T15). Transient per-hit flash.
            if shield.shield_alpha <= 0.0 {
                continue;
            }
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
                shield.shield_alpha,
                team,
                // `UnitType.drawShield` lerps the rim toward white by
                // `clamp(hitTime / 2)`: `hitTime` is view-only state owned by
                // the unit renderer, so the port passes 0 (deviation).
                0.0,
            ));
        }
        out
    }

    /// Live unit abilities with a `draw(Unit)` override (L04 T2): ForceField,
    /// EnergyField, SuppressionField and ShieldArc. Mod-only ArmorPlate/
    /// UnitSpawn/MoveLightning kinds are not modelled in the content registry.
    pub fn unit_ability_draws(&self) -> Vec<UnitAbilityDraw> {
        use mind_core::combat::abilities::AbilityComp;
        use mind_core::content::registries::units::ability::AbilityKind;
        use mind_core::entities::comp::Pos;
        use mind_core::entities::comp::unit::comp::{ShieldComp, UnitCore, UnitTypeComp};
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(comp) = entity.get::<AbilityComp>() else {
                continue;
            };
            let Some(pos) = entity.get::<Pos>() else {
                continue;
            };
            let Some(rotation) = entity.get::<UnitCore>().map(|core| core.rotation) else {
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
                .map(|team| team.team)
                .unwrap_or(0);
            let shield = entity
                .get::<ShieldComp>()
                .map(|shield| shield.shield)
                .unwrap_or(0.0);
            let id = entity
                .get::<mind_core::ecs::EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(0);
            for (spec, state) in def.abilities.iter().zip(&comp.states) {
                if matches!(
                    spec.kind,
                    AbilityKind::ForceField
                        | AbilityKind::EnergyField
                        | AbilityKind::SuppressionField
                        | AbilityKind::ShieldArc
                ) {
                    out.push(UnitAbilityDraw {
                        kind: spec.kind,
                        x: pos.x,
                        y: pos.y,
                        rotation,
                        team,
                        spec: spec.clone(),
                        state: *state,
                        shield,
                        id,
                    });
                }
            }
        }
        out
    }

    /// F6 status markers (`BuildingComp.drawStatus`/`status()`): one row per
    /// player-team building with a non-empty `consumes` —
    /// `(center tile x, center tile y, world x, world y, size, java_color)`.
    /// Rust-only seam for the status pass (T14/PAR-0097).
    pub fn building_statuses(&self) -> Vec<(i32, i32, f32, f32, i32, u32)> {
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let team = self.player_team();
        // `BuildingComp.status()` reads `state.tick` (Vars.state): the sim tick
        // drives the efficiency pulse (`BuildingComp.java:651`; I02 T3).
        let tick = self.sim.tick_count();
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
            // `BuildingComp.status()` (`BuildingComp.java:638-652`): logic
            // disable, `noOutput` when `shouldConsume()` is false, `noInput`
            // with no efficiency/production, else the 30-tick pulse
            // `((tick / 30) % 1) < efficiency` (I02 T3; H01 T8 transcribed the
            // per-family `shouldConsume()` overrides in
            // `mind_core::world::status::java_block_status`).
            let status =
                mind_core::world::status::java_block_status(&self.sim.ecs.0, entity.id(), tick);
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
    /// `[{item, amount, have, infinite, label}]` — amount scaled by
    /// `Rules.buildCostMultiplier` (`Math.round(stack.amount * multiplier)`),
    /// `have` from the player's first core (`player.core().items`), and
    /// `label` the Java-markup amount text (`*/90` under infinite resources
    /// or without a core; `PlacementFragment.java:407-416`).
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
        // `core == null || state.rules.isInfiniteResources(player.team())`
        // (`PlacementFragment.java:410`): Java's team check is
        // `infiniteResources || teams.get(team).infiniteResources`.
        let infinite = self
            .sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .is_some_and(|runtime| {
                let rules = &runtime.session.rules;
                rules.infinite_resources
                    || rules
                        .teams
                        .get(team)
                        .is_some_and(|rule| rule.infinite_resources)
            });
        let has_core = self.player_core_blocks(team) > 0;
        let rows: Vec<serde_json::Value> = def
            .requirements
            .iter()
            .filter_map(|stack| {
                let item = content.item(stack.item)?;
                let amount = (stack.amount as f32 * multiplier + 0.5).floor() as i32;
                let have = core_items.get(&item.name).copied().unwrap_or(0);
                let label = mind_core::ui::display::requirement_label(
                    has_core.then_some(have),
                    amount,
                    infinite,
                );
                Some(serde_json::json!({
                    "item": item.name,
                    "amount": amount,
                    "have": have,
                    "infinite": infinite || !has_core,
                    "label": label,
                }))
            })
            .collect();
        GString::from(serde_json::Value::Array(rows).to_string().as_str())
    }

    /// `PlacementFragment.java:313-323`: the palette cell tint for every content
    /// block, keyed by block name — `0` white (placeable, affordable and the
    /// player unit is a builder), `1` gray (unaffordable or not a builder),
    /// `2` darkGray (`Block.isPlaceable()` false: the block does not support
    /// the current env). One call per palette refresh instead of a per-cell
    /// seam, mirroring Java's per-frame `button.update`.
    #[func]
    pub fn palette_tints_json(&self) -> GString {
        let Some(content) = self.content_snapshot.as_ref() else {
            return GString::from("{}");
        };
        let rules = self
            .sim
            .ecs
            .0
            .get_resource::<CampaignRuntime>()
            .map(|runtime| runtime.session.rules.clone());
        let team = rules.as_ref().map_or(0, |rules| rules.default_team);
        let core_items: std::collections::HashMap<String, i32> =
            self.player_core_items(team).into_iter().collect();
        let multiplier = rules
            .as_ref()
            .map(|rules| rules.build_cost_multiplier)
            .filter(|value| *value > 0.0)
            .unwrap_or(1.0);
        // `state.rules.isInfiniteResources(player.team())`
        // (`PlacementFragment.java:316`): the rule field or the team rule.
        let infinite = rules.as_ref().is_some_and(|rules| {
            rules.infinite_resources
                || rules
                    .teams
                    .get(team)
                    .is_some_and(|rule| rule.infinite_resources)
        });
        let builder = self.sim.player_can_build();
        let env = rules.as_ref().map_or(0, |rules| rules.env);
        let mut out = serde_json::Map::new();
        for def in content.blocks() {
            let placeable = mind_core::world::build::supports_env_bits(def, env);
            let affordable = infinite
                || def.requirements.iter().all(|stack| {
                    let amount = (stack.amount as f32 * multiplier + 0.5).floor() as i32;
                    content.item(stack.item).is_some_and(|item| {
                        core_items.get(&item.name).copied().unwrap_or(0) >= amount
                    })
                });
            let tint = mind_core::world::build::palette_tint(placeable, affordable, builder);
            out.insert(def.name.clone(), serde_json::Value::from(tint));
        }
        GString::from(serde_json::Value::Object(out).to_string().as_str())
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
        // Live building health (J04 T2 verification; debug probe only).
        match tile.build.and_then(|entity| {
            self.sim
                .ecs
                .0
                .get::<mind_core::entities::comp::health::Health>(entity)
        }) {
            Some(health) => {
                d.set("health", &health.health.to_variant());
                d.set("max_health", &health.max_health.to_variant());
            }
            None => {
                d.set("health", &Variant::nil());
                d.set("max_health", &Variant::nil());
            }
        }
        // Stored liquids `[{"liquid": name, "amount": f}]` (J04 T3 verification;
        // debug probe only).
        let mut liquid_rows: Vec<serde_json::Value> = Vec::new();
        if let Some(entity) = tile.build
            && let Some(module) = self
                .sim
                .ecs
                .0
                .get::<mind_core::world::modules::LiquidModule>(entity)
        {
            for (index, amount) in module.liquids.iter().enumerate() {
                if *amount <= 0.0 {
                    continue;
                }
                let name = self
                    .content_snapshot
                    .as_ref()
                    .and_then(|content| {
                        content.liquid(mind_core::content::LiquidId::new(index as u16))
                    })
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                liquid_rows.push(serde_json::json!({"liquid": name, "amount": amount}));
            }
        }
        d.set(
            "liquids",
            &GString::from(serde_json::Value::Array(liquid_rows).to_string().as_str()).to_variant(),
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

    /// Debug probe (J01 T1): the loaded grid's generation facts — the recorded
    /// `World.FilterContext` seed (`-1` when the load ran no filters) plus ore
    /// overlay and boulder block counts and a few ore tile coordinates for
    /// `tile_debug` spot checks. Companion to `tile_debug`.
    #[func]
    pub fn world_gen_debug(&self) -> Dictionary<GString, Variant> {
        let mut d = Dictionary::new();
        let seed = self
            .sim
            .map_gen_seed()
            .map(|seed| seed as i64)
            .unwrap_or(-1);
        d.set("seed", &seed.to_variant());
        let (ore, boulders, ore_sample, boulder_sample) = match self.content_snapshot.as_ref() {
            Some(content) => count_world_gen(&self.sim.grid, content),
            None => (0, 0, Vec::new(), Vec::new()),
        };
        d.set("ore", &ore.to_variant());
        d.set("boulders", &boulders.to_variant());
        d.set("ore_tiles", &pack_coords(ore_sample).to_variant());
        d.set("boulder_tiles", &pack_coords(boulder_sample).to_variant());
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

    /// Rust-only: `Build.validPlace` for the player team without mutating the
    /// sim ([`Sim::valid_place_player`], the exact check
    /// `queue_player_place_config` runs; the placement preview's
    /// `HostWorld.valid_place`).
    pub fn can_place_player(&self, x: i32, y: i32, block: BlockId, rotation: u8) -> bool {
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        self.sim.valid_place_player(x, y, block, rotation)
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

    /// `LogicBlock.drawPlace` (`LogicBlock.java:185-189`): the processor link
    /// range drawn while placing (`Drawf.circles(.., range)` in `Pal.accent`);
    /// `None` for privileged/world processors and non-logic blocks. Kept apart
    /// from [`Self::block_place_range`] because `LogicBuild.drawSelect` draws
    /// no hover range.
    pub fn logic_place_range(&self, block: BlockId) -> Option<f32> {
        mind_core::logic::blocks::logic_place_range(&self.sim.ecs.0, block)
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

    /// World-pixel centre `(Building.x, Building.y)` of the building covering
    /// tile `(x, y)` — a multiblock proxy tile resolves to its centre tile plus
    /// `Block.offset` — or `null` without a building. The block-config fragment
    /// anchors to it every frame (`Building.updateTableAlign`,
    /// `BuildingComp.java:1682-1685`) and hides once it is gone
    /// (`BlockConfigFragment.showConfig` update: `!selected.isValid()`).
    #[func]
    pub fn building_center_world(&self, x: i32, y: i32) -> Variant {
        let Some(entity) = self.tile_at(x, y).and_then(|tile| tile.build) else {
            return Variant::nil();
        };
        let Some(building) = self
            .sim
            .ecs
            .0
            .get::<mind_core::entities::comp::building::Building>(entity)
        else {
            return Variant::nil();
        };
        let offset = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.block(building.block))
            .map(|def| def.offset)
            .unwrap_or(0.0);
        Vector2::new(
            f32::from(building.tile.0) * TILESIZE_F + offset,
            f32::from(building.tile.1) * TILESIZE_F + offset,
        )
        .to_variant()
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

    /// Cheap per-building hash of the state that changes a `drawCached` bake
    /// (`BuildingCacheRenderer` chunk state sums; H03 T18). `0` without a
    /// building. Mirrors `BuildingComp.recache()` being called on config
    /// changes that never bump the world revision.
    pub fn cached_state_key(&self, x: i32, y: i32) -> u64 {
        use mind_core::render::block_sprite::{CachedBlockState, cached_state_hash, crack_index};
        let Some(state) = self.block_state_draw(x, y) else {
            return 0;
        };
        cached_state_hash(&CachedBlockState {
            door_open: state.door_open,
            enabled: state.enabled,
            item: state
                .sort_item
                .or(state.unloader_item)
                .or(state.source_item),
            damaged: state.damaged,
            crack: crack_index(state.healthf),
        })
    }

    /// `BlockRenderer` crack-overlay rows (`BlockRenderer.java:801-821`):
    /// every damaged building whose block is `!draw_dynamic` (exactly the
    /// buildings the dynamic pass skips and the static cache bakes; the port
    /// counterpart of Java's `tileExtraCachedView`). `(world x, world y,
    /// tile y, size, healthf)`.
    pub fn cached_crack_draws(&self) -> Vec<(f32, f32, i32, i32, f32)> {
        use mind_core::entities::comp::health::Health;
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entity in self.sim.ecs.0.iter_entities() {
            let Some(building) = entity.get::<mind_core::entities::comp::Building>() else {
                continue;
            };
            let Some(def) = content.block(building.block) else {
                continue;
            };
            // `drawCracks` guards (`BuildingComp.java:1234`): `drawDynamic`
            // blocks draw their own cracks in the dynamic pass, and
            // `BlockRenderer.maxCrackSize` is 7.
            if def.draw_dynamic || def.size > 7 {
                continue;
            }
            let Some(health) = entity.get::<Health>() else {
                continue;
            };
            if !health.damaged() {
                continue;
            }
            let size = def.size.max(1);
            // `Building.x = tile.drawx() + Block.offset` (same anchor as the
            // dynamic pass and the cache bake).
            out.push((
                f32::from(building.tile.x()) * 8.0 + def.offset,
                f32::from(building.tile.y()) * 8.0 + def.offset,
                i32::from(building.tile.y()),
                size,
                health.fraction(),
            ));
        }
        out
    }

    /// Fires in `rect` (`FireComp.draw` view data; H03 T7), stable by entity
    /// index. The tile centre gets Java's `randomSeedRange` jitter; `frame`
    /// and `alpha` come from the sim's animation/warmup.
    pub fn fire_draws(&self, rect: (f32, f32, f32, f32)) -> Vec<FireDraw> {
        use mind_core::combat::fires::{FRAMES, FireState, WARMUP_DURATION};
        use mind_core::render::drawf::random_seed_range;
        let (x0, y0, w, h) = rect;
        let (x1, y1) = (x0 + w, y0 + h);
        let mut rows: Vec<(u32, FireDraw)> = Vec::new();
        for entity_ref in self.sim.ecs.0.iter_entities() {
            let Some(fire) = entity_ref.get::<FireState>() else {
                continue;
            };
            let (tx, ty) = fire.tile;
            let cx = f32::from(tx) * 8.0;
            let cy = f32::from(ty) * 8.0;
            // `Draw.rect(regions[...], x + randomSeedRange((int)y, 2), y + randomSeedRange((int)x, 2))`
            // (`FireComp.java:104-106`): the fire's world coordinates seed the
            // two axis offsets.
            let x = cx + random_seed_range(i64::from(ty) * 8, 2.0);
            let y = cy + random_seed_range(i64::from(tx) * 8, 2.0);
            if x < x0 || y < y0 || x > x1 || y > y1 {
                continue;
            }
            rows.push((
                entity_ref.id().index_u32(),
                FireDraw {
                    x,
                    y,
                    frame: (fire.animation as i32).min(FRAMES as i32 - 1),
                    alpha: (fire.warmup / WARMUP_DURATION).clamp(0.0, 1.0),
                },
            ));
        }
        rows.sort_by_key(|(index, _)| *index);
        rows.into_iter().map(|(_, draw)| draw).collect()
    }

    /// Puddles in `rect` (`PuddleComp.draw`/`Liquid.drawPuddle` view data;
    /// H03 T7), stable by entity index.
    pub fn puddle_draws(&self, rect: (f32, f32, f32, f32)) -> Vec<PuddleDraw> {
        use mind_core::combat::puddles::{MAX_LIQUID, PuddleState};
        use mind_core::render::drawf::shift_value;
        let Some(content) = self.content_snapshot.as_ref() else {
            return Vec::new();
        };
        let (x0, y0, w, h) = rect;
        let (x1, y1) = (x0 + w, y0 + h);
        let mut rows: Vec<(u32, PuddleDraw)> = Vec::new();
        for entity_ref in self.sim.ecs.0.iter_entities() {
            let Some(puddle) = entity_ref.get::<PuddleState>() else {
                continue;
            };
            let (tx, ty) = puddle.tile;
            let x = f32::from(tx) * 8.0;
            let y = f32::from(ty) * 8.0;
            if x < x0 || y < y0 || x > x1 || y > y1 {
                continue;
            }
            let Some(def) = content.liquid(puddle.liquid) else {
                continue;
            };
            // `float f = Mathf.clamp(amount / (maxLiquid / 1.5f))`.
            let f = (puddle.amount / (MAX_LIQUID / 1.5)).clamp(0.0, 1.0);
            // `Tmp.c1.set(color).shiftValue(-0.05f)`.
            let color = shift_value([def.color.r, def.color.g, def.color.b, def.color.a], -0.05);
            let on_liquid = content
                .block(self.sim.grid.tile(tx as i32, ty as i32).floor)
                .is_some_and(mind_core::maps::filters::block_info::is_liquid);
            let light = (def.light_color.a > 0.001 && f > 0.0).then(|| {
                (
                    [
                        def.light_color.r,
                        def.light_color.g,
                        def.light_color.b,
                        def.light_color.a,
                    ],
                    color[3] * f * 0.8,
                )
            });
            rows.push((
                entity_ref.id().index_u32(),
                PuddleDraw {
                    x,
                    y,
                    id: entity_ref.id().index_u32() as i32,
                    f,
                    color,
                    on_liquid,
                    light,
                },
            ));
        }
        rows.sort_by_key(|(index, _)| *index);
        rows.into_iter().map(|(_, draw)| draw).collect()
    }

    /// `DrawState` for a block's `DrawBlock` chain (`world/draw/*`; H03 T8).
    /// `None` without a building, a content def, or the content snapshot.
    pub fn block_draw_state(
        &self,
        x: i32,
        y: i32,
    ) -> Option<mind_core::render::draw_desc::DrawState> {
        use mind_core::content::LiquidId;
        use mind_core::entities::comp::building::CrafterState;
        use mind_core::world::blocks::heat::HeatState;
        use mind_core::world::blocks::power::generator::GeneratorState;
        use mind_core::world::modules::{LiquidModule, PowerModule};
        let tile = self.tile_at(x, y)?;
        let entity = tile.build?;
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(tile.block)?;
        let ecs = &self.sim.ecs.0;
        let size = def.size.max(1);
        let rotation = self.building_rotation(x, y);
        let cx = (x + (size - 1) / 2) as f32 * 8.0 + def.offset;
        let cy = (y + (size - 1) / 2) as f32 * 8.0 + def.offset;
        let crafter = ecs.get::<CrafterState>(entity);
        // `GenericCrafter` carries `warmup` on `CrafterState`; generators drive
        // their drawer from `productionEfficiency` (`ConsumeGenerator` /
        // reactors), which the port keeps on `GeneratorState`.
        let warmup = ecs
            .get::<GeneratorState>(entity)
            .map(|state| state.production_efficiency)
            .or_else(|| crafter.map(|state| state.warmup))
            .unwrap_or(0.0);
        // Dominant liquid entry (for `liquidColor`/`liquidAmount` consumers).
        let (liquid_amount, liquid_color) = ecs
            .get::<LiquidModule>(entity)
            .map(|module| {
                let (id, amount) = module
                    .liquids
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(id, amount)| (id as u16, *amount))
                    .unwrap_or((0, 0.0));
                let color = content
                    .liquid(LiquidId::new(id))
                    .map(|liquid| liquid.color.to_rgba8888())
                    .unwrap_or(0xffff_ffff);
                (amount, color)
            })
            .unwrap_or((0.0, 0xffff_ffff));
        Some(mind_core::render::draw_desc::DrawState {
            warmup,
            progress: crafter.map(|state| state.progress).unwrap_or(0.0),
            total_progress: crafter.map(|state| state.total_progress).unwrap_or(0.0),
            // `Time.time` view clock (the same 60 Hz ticks the other passes use).
            time: godot::classes::Time::singleton().get_ticks_msec() as f32 * 0.06,
            id: entity.index_u32() as i32,
            power_status: ecs
                .get::<PowerModule>(entity)
                .map(|module| module.status)
                .unwrap_or(0.0),
            liquid_capacity: def.liquid_capacity,
            liquid_amount,
            liquid_color,
            heat: ecs
                .get::<HeatState>(entity)
                .map(|state| state.heat)
                .unwrap_or(0.0),
            ..mind_core::render::draw_desc::DrawState::new(cx, cy, i32::from(rotation), size)
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
        // `PlacementFragment.hovered()` calls `build.display(table)` for the
        // hovered building whatever its team (Java shows enemy/derelict bars
        // too); only the *unit* test is player-team-scoped
        // (`Units.closestOverlap(player.team(), ...)`).
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
        // `Block.setBars` adds the base `power` bar only when `consPower != null`
        // (a power *consumer*); producers like generators show only their own
        // `poweroutput` row.
        let power_status = (def.consumes.iter().any(|spec| {
            matches!(
                &spec.consume,
                mind_core::content::registries::blocks::Consume::Power { .. }
            )
        }))
        .then(|| {
            self.sim
                .ecs
                .0
                .get::<PowerModule>(entity)
                .map(|module| module.status)
        })
        .flatten();
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
        // `Block.setBars` family overrides read live component state (J05
        // T4): heat, drill speed/warmup, `storageCapacity` + the unlocked item
        // count for a core, and the factory's progress.
        let world = &self.sim.ecs.0;
        let table = world.get_resource::<mind_core::world::block::BlockTable>();
        let instance = table.and_then(|table| table.instance(tile.block));
        let power_module = world.get::<PowerModule>(entity);
        let turret = world
            .get::<mind_core::world::blocks::defense::turrets::TurretState>(entity)
            .map(|state| TurretBars {
                total_ammo: state.total_ammo,
                max_ammo: state.config.max_ammo,
                heat_req: state.heat_req,
                heat_requirement: state.config.heat_requirement,
                max_heat_efficiency: state.config.max_heat_efficiency,
                activation_timer: state.activation_timer,
                activation_time: state.config.activation_time,
            });
        let power = power_module.and_then(|module| {
            let graph = world
                .get_resource::<PowerGrids>()
                .and_then(|grids| grids.graph(module.graph))?;
            let diode_fraction = |dx: i32, dy: i32| {
                let target = self.tile_at(tile.x as i32 + dx, tile.y as i32 + dy)?;
                let target_entity = target.build?;
                let target_module = world.get::<PowerModule>(target_entity)?;
                let target_graph = world
                    .get_resource::<PowerGrids>()
                    .and_then(|grids| grids.graph(target_module.graph))?;
                let capacity = target_graph.get_total_battery_capacity(world);
                (capacity > 0.0).then(|| target_graph.get_battery_stored(world) / capacity)
            };
            let (back, front) = {
                let rotation = world
                    .get::<mind_core::entities::comp::Building>(entity)
                    .map(|building| building.rotation)
                    .unwrap_or(0);
                let (dx, dy) = match rotation & 3 {
                    0 => (0, 1),
                    1 => (1, 0),
                    2 => (0, -1),
                    _ => (-1, 0),
                };
                (diode_fraction(-dx, -dy), diode_fraction(dx, dy))
            };
            Some(PowerBars {
                balance: graph.power_balance(),
                produced: graph.last_power_produced(),
                needed: graph.last_power_needed(),
                stored: graph.get_battery_stored(world),
                capacity: graph.get_total_battery_capacity(world),
                links: module.links.len(),
                max_nodes: world
                    .get::<mind_core::world::blocks::power::PowerNodeConfig>(entity)
                    .map_or(0, |config| config.max_nodes as usize),
                back,
                front,
            })
        });
        let generator = world
            .get::<mind_core::world::blocks::power::generator::GeneratorConfig>(entity)
            .map(|config| GeneratorBars {
                production: config.power_production
                    * world
                        .get::<mind_core::world::blocks::power::generator::GeneratorState>(entity)
                        .map_or(0.0, |state| state.production_efficiency),
                usage: def
                    .consumes
                    .iter()
                    .find_map(|spec| match &spec.consume {
                        mind_core::content::registries::blocks::Consume::Power {
                            usage, ..
                        } => Some(*usage),
                        _ => None,
                    })
                    .unwrap_or(0.0),
                efficiency: world
                    .get::<mind_core::world::blocks::power::generator::GeneratorState>(entity)
                    .map_or(0.0, |state| state.production_efficiency),
            });
        let team = world
            .get::<mind_core::entities::comp::TeamComp>(entity)
            .map_or(0, |team| team.team);
        let unit_bars = |unit: mind_core::content::UnitTypeId| {
            let def = content.units().get(unit.index())?;
            Some(UnitCapBars {
                name: def.name.clone(),
                count: mind_core::entities::comp::unit::queries::live_type_count(world, team, unit),
                cap: mind_core::entities::comp::unit::queries::unit_cap(world, team)
                    .map(|cap| cap as i32),
                use_cap: def.use_unit_cap,
                has_unit: true,
            })
        };
        let factory_units = world
            .get::<mind_core::world::blocks::units::UnitFactoryBuild>(entity)
            .and_then(|state| {
                usize::try_from(state.current_plan)
                    .ok()
                    .and_then(|index| def.unit_plans.get(index))
                    .and_then(|plan| unit_bars(plan.unit))
            });
        let assembler = world
            .get::<mind_core::world::blocks::units::UnitAssembler>(entity)
            .map(|state| {
                (
                    state.progress,
                    state.plan().map_or(0.0, |plan| plan.time),
                    state.plan().and_then(|plan| unit_bars(plan.unit)),
                )
            });
        let cargo_units = world
            .get::<mind_core::world::blocks::units::CargoLoaderState>(entity)
            .and_then(|state| {
                let unit = state.unit?;
                let type_id = world
                    .get::<mind_core::entities::comp::unit::comp::UnitTypeComp>(unit)?
                    .type_id;
                unit_bars(type_id)
            });
        let campaign_state = world.get::<mind_core::entities::comp::CampaignState>(entity);
        let launch = campaign_state.and_then(|state| {
            instance
                .as_ref()
                .and_then(|instance| match &instance.kind_data {
                    mind_core::world::BlockKindData::LaunchPad(def) => {
                        Some((state.launch_time, def.launch_time))
                    }
                    _ => None,
                })
        });
        let variable_reactor = world
            .get::<mind_core::world::blocks::power::behavior::VariableReactorState>(entity)
            .map(|state| (state.heat, 150.0, state.instability));
        let overdrive = world
            .get::<mind_core::world::blocks::defense::projectors::OverdriveProjectorState>(entity)
            .map(|state| {
                let boost = state.real_boost(
                    world
                        .get::<mind_core::entities::comp::Building>(entity)
                        .map_or(0.0, |building| building.efficiency),
                );
                (boost, state.speed_boost + state.speed_boost_phase)
            });
        let shield = world
            .get::<mind_core::world::blocks::defense::shields::ForceProjectorState>(entity)
            .map(|state| {
                if state.broken {
                    0.0
                } else {
                    1.0 - state.buildup
                        / (state.shield_health + state.phase_shield_boost * state.phase_heat)
                }
            });
        let pump = world
            .get::<mind_core::entities::comp::PumpState>(entity)
            .map(|state| {
                (
                    state.amount,
                    state.warmup,
                    world
                        .get::<mind_core::entities::comp::Building>(entity)
                        .map_or(0.0, |building| building.efficiency),
                )
            });
        let target_dummy = world
            .get::<mind_core::world::blocks::defense::shields::TargetDummyState>(entity)
            .map(|state| (state.reset, state.reset_time));
        let output_liquids = instance
            .as_ref()
            .and_then(|instance| match &instance.kind_data {
                mind_core::world::BlockKindData::Crafter(crafter) => Some(&crafter.output_liquids),
                _ => None,
            })
            .map(|outputs| {
                outputs
                    .iter()
                    .filter_map(|(liquid, _amount)| {
                        let liquid = mind_core::content::LiquidId::new(*liquid);
                        let (name, color) = liquid_info.get(liquid.index())?;
                        Some((
                            name.clone(),
                            *color,
                            liquids
                                .as_deref()
                                .and_then(|amounts| amounts.get(liquid.index()))
                                .copied()
                                .unwrap_or(0.0),
                            def.liquid_capacity,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let extras = BarExtras {
            heat: world
                .get::<mind_core::world::blocks::heat::HeatState>(entity)
                .map(|state| (state.heat, state.heat_output)),
            drill: world
                .get::<mind_core::entities::comp::DrillState>(entity)
                .map(|state| (state.last_drill_speed, state.warmup)),
            time_scale: world
                .get::<mind_core::entities::comp::Building>(entity)
                .map(|building| building.time_scale)
                .unwrap_or(1.0),
            core_capacity: (def.kind == mind_core::content::BlockKind::CoreBlock).then(|| {
                let unlocked = self
                    .content_snapshot
                    .as_ref()
                    .map(|content| {
                        content
                            .items()
                            .iter()
                            .filter(|item| item.unlock.unlocked() || !self.in_campaign())
                            .count() as f32
                    })
                    .unwrap_or(0.0);
                let capacity = world
                    .get::<mind_core::world::blocks::storage::CoreBuild>(entity)
                    .map(|core| core.storage_capacity as f32)
                    .unwrap_or(0.0);
                (capacity, unlocked)
            }),
            factory: world
                .get::<mind_core::world::blocks::units::UnitFactoryBuild>(entity)
                .map(|state| (state.progress, state.current_plan)),
            turret,
            power: power.clone(),
            generator,
            reactor_heat: world
                .get::<mind_core::world::blocks::power::behavior::NuclearReactorState>(entity)
                .map(|state| state.heat),
            variable_reactor,
            overdrive,
            shield,
            pump,
            payload_loader: None,
            assembler,
            factory_units,
            cargo_units,
            launch,
            landing_cooldown: campaign_state.map(|state| state.cooldown),
            accelerator: power.map(|power| {
                (
                    power.stored,
                    instance
                        .as_ref()
                        .and_then(|instance| match &instance.kind_data {
                            mind_core::world::BlockKindData::Accelerator(def) => {
                                Some(def.power_buffer_requirement)
                            }
                            _ => None,
                        })
                        .unwrap_or(0.0),
                )
            }),
            target_dummy,
            output_liquids,
        };
        let rows = building_bar_rows(
            def,
            healthf,
            power_status,
            items_total,
            liquids.as_deref(),
            &liquid_info,
            &extras,
        );
        let json: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                serde_json::json!({
                    "key": row.key,
                    "label_key": row.label_key,
                    "label_args": row.label_args,
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

    /// Fog controller state + counts, **no per-tile data** (I01 T15): the
    /// callable-with-no-arguments readback. `{fog, staticFog, teams:[id],
    /// sources:<live source count>, discovered:<set static bits>}`. Reports
    /// `{"fog":false}` without a `CampaignRuntime`.
    ///
    /// The T15 stall was a script error from calling the old 3-argument probe
    /// with no arguments (break-on-error wedges the frame loop); keep this
    /// entry cheap so a playtest can always probe the fog state.
    #[func]
    pub fn fog_debug_json(&self) -> GString {
        let Some(rules) = mind_core::game::runtime::rules_of(&self.sim.ecs.0) else {
            return GString::from("{\"fog\":false}");
        };
        let mut teams: Vec<u8> = Vec::new();
        let mut discovered = 0usize;
        if let Some(fog) = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::fog::FogControl>()
        {
            for (id, data) in fog.fog.iter().enumerate() {
                if let Some(data) = data {
                    teams.push(id as u8);
                    discovered += data.static_data.count_set_bits();
                }
            }
        }
        let sources = self.sim.fog_sources(0).len().max(
            self.sim
                .ecs
                .0
                .get_resource::<mind_core::game::fog::FogControl>()
                .map_or(0, |fog| fog.fog.iter().flatten().count()),
        );
        let value = serde_json::json!({
            "fog": rules.fog,
            "staticFog": rules.static_fog,
            "teams": teams,
            "discoveredTiles": discovered,
            "sources": sources,
        });
        GString::from(value.to_string().as_str())
    }

    /// Single-tile fog probe (`FogControl.isVisibleTile`/`isDiscovered`):
    /// `{fog, staticFog, visible, discovered, teams:[id]}` (the pre-T15 3-arg
    /// readback, renamed).
    #[func]
    pub fn fog_debug_tile_json(&self, team: i32, x: i32, y: i32) -> GString {
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

    /// Bounded per-tile fog rect probe (I01 T15): visibility + discovery for
    /// `w x h` tiles from `(x, y)`, clamped to 64x64 so a probe can never
    /// walk a whole large map.
    #[func]
    pub fn fog_debug_json_rect(&self, team: i32, x: i32, y: i32, w: i32, h: i32) -> GString {
        let team = (team & 0xFF) as u8;
        let w = w.clamp(0, 64);
        let h = h.clamp(0, 64);
        let mut rows: Vec<serde_json::Value> = Vec::with_capacity(h as usize);
        for ty in y..y.saturating_add(h) {
            let mut row: Vec<u8> = Vec::with_capacity(w as usize);
            for tx in x..x.saturating_add(w) {
                let visible = self.sim.fog_visible_tile(team, tx, ty);
                let discovered = self.sim.fog_discovered(team, tx, ty);
                row.push(match (visible, discovered) {
                    (true, _) => 1,
                    (false, true) => 2,
                    (false, false) => 0,
                });
            }
            rows.push(serde_json::Value::Array(
                row.into_iter().map(serde_json::Value::from).collect(),
            ));
        }
        let value = serde_json::json!({ "x": x, "y": y, "w": w, "h": h, "rows": rows });
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

    // ---- H03 T3 command panel + reticle seams ----

    /// `PlacementFragment` command table probe (`PlacementFragment.java:443-676`).
    /// `ids_json` is the selection id array (`MindInput.selection_json()`);
    /// returns the same JSON [`command_panel`] builds for the GDScript panel.
    #[func]
    pub fn command_panel_json(&self, ids_json: GString) -> GString {
        let ids: Vec<i32> = serde_json::from_str(ids_json.to_string().as_str()).unwrap_or_default();
        let Some(content) = self.content_snapshot.as_ref() else {
            return GString::from("{\"types\":[],\"commands\":[],\"stances\":[]}");
        };
        let units = self.command_panel_units(&ids);
        GString::from(command_panel(content, &units).to_string().as_str())
    }

    /// Per-unit command-panel rows: `(type id, live command, stance bits)` for
    /// the live units whose `EntitySeq` is in `ids`. The live command is the
    /// `CommandAI` command component (`UnitCommandComp`, set by
    /// `Sim::apply_live_unit_command`) falling back to `ControllerSlot.command`;
    /// `None` = the type's `defaultCommand` (`CommandAI.currentCommand`).
    pub(crate) fn command_panel_units(
        &self,
        ids: &[i32],
    ) -> Vec<(i32, Option<UnitCommandId>, StanceBits)> {
        use mind_core::ai::controller::{ControllerSlot, UnitCommandComp};
        use mind_core::ai::unit_stance_runtime::{StanceBits, UnitStancesComp};
        use mind_core::content::id::UnitCommandId;
        use mind_core::ecs::EntitySeq;
        use mind_core::entities::comp::unit::comp::UnitTypeComp;
        let Some(runtime) = self.sim.unit_runtime() else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(ids.len());
        for &id in ids {
            let Some(&entity) = runtime.units.iter().find(|&&entity| {
                self.sim
                    .ecs
                    .0
                    .get::<EntitySeq>(entity)
                    .is_some_and(|seq| seq.0 as i32 == id)
            }) else {
                continue;
            };
            let Ok(entity_ref) = self.sim.ecs.0.get_entity(entity) else {
                continue;
            };
            let Some(type_comp) = entity_ref.get::<UnitTypeComp>() else {
                continue;
            };
            let command = entity_ref
                .get::<UnitCommandComp>()
                .map(|comp| UnitCommandId::new(comp.0))
                .or_else(|| {
                    entity_ref
                        .get::<ControllerSlot>()
                        .and_then(|slot| slot.command)
                });
            let stances: StanceBits = entity_ref
                .get::<UnitStancesComp>()
                .map(|comp| comp.0)
                .unwrap_or_default();
            out.push((type_comp.type_id.get() as i32, command, stances));
        }
        out
    }

    /// Whether the live command of unit `id` draws its target
    /// (`UnitCommand.drawTarget`; `InputHandler.java:1231`). Rust-only reticle
    /// filter (the command panel probe above reports the command but not the
    /// icon behaviour).
    pub fn command_draws_target(&self, id: i32) -> bool {
        let Some(content) = self.content_snapshot.as_ref() else {
            return false;
        };
        self.command_panel_units(&[id])
            .first()
            .is_some_and(|(type_id, command, _)| {
                let Some(unit) = content.unit(UnitTypeId::new(*type_id as u16)) else {
                    return false;
                };
                let command = command.or_else(|| {
                    mind_core::ai::unit_command_runtime::default_command(unit, content)
                        .map(|def| def.id)
                });
                command
                    .and_then(|id| content.unit_command(id))
                    .is_some_and(|def| def.draw_target)
            })
    }

    /// Live command targets (`Sim::command_targets`; G01 T16) as
    /// `(id, world x, world y, is_attack)`. Rust-only feed for the T3 attack
    /// reticle (`InputHandler.drawCommandedTargets`).
    pub fn command_targets(&self) -> Vec<(i32, f32, f32, bool)> {
        self.sim.command_targets()
    }

    /// Drains the per-tick `BuildDamageEvent` log as `(tile x, tile y, team)`
    /// (`Control.java:68-72`; H03 T4). Rust-only: the minimap is the SINGLE
    /// drain consumer (the T3-style `#[func] drain_build_damage_json` probe is
    /// for debug reads only — two consumers would eat each other's rows).
    pub fn drain_build_damage(&mut self) -> Vec<(i32, i32, u8)> {
        self.sim
            .drain_build_damage_events()
            .into_iter()
            .map(|event| (event.x as i32, event.y as i32, event.team))
            .collect()
    }

    /// `Block.offset` of the building at `(x, y)` in world px
    /// (`((size + 1) % 2) * tilesize / 2`); `0` without a building or off-map.
    /// Minimap indicator centring (`MinimapRenderer.java:266-271`).
    pub fn block_offset(&self, x: i32, y: i32) -> f32 {
        let Some(tile) = self.tile_at(x, y) else {
            return 0.0;
        };
        if tile.build.is_none() {
            return 0.0;
        }
        self.content_snapshot
            .as_ref()
            .and_then(|content| content.block(tile.block))
            .map(|def| def.offset)
            .unwrap_or(0.0)
    }

    // ---- H03 T5 overlay-top seams ----

    /// One frame of `OverlayRenderer.drawTop` inputs
    /// (`OverlayRenderer.java:91-226`): player position/team, non-player-team
    /// units, enemy cores and the ground spawn tiles.
    pub fn overlay_view(&self) -> OverlayView {
        use mind_core::entities::comp::{Pos, TeamComp};
        let player_team = self.player_team();
        let player = self.player_unit_position();
        let mut enemy_units = Vec::new();
        if let Some(runtime) = self.sim.unit_runtime() {
            for &entity in &runtime.units {
                let Some(pos) = self.sim.ecs.0.get::<Pos>(entity) else {
                    continue;
                };
                let team = self
                    .sim
                    .ecs
                    .0
                    .get::<TeamComp>(entity)
                    .map(|comp| comp.team)
                    .unwrap_or(0);
                if team != player_team {
                    enemy_units.push((pos.x, pos.y, team));
                }
            }
        }
        let mut enemy_cores = Vec::new();
        if let Some(runtime) = self
            .sim
            .ecs
            .0
            .get_resource::<mind_core::game::runtime::CampaignRuntime>()
        {
            for team in runtime.session.teams.active.clone() {
                let Some(data) = runtime.session.teams.data(team) else {
                    continue;
                };
                if team.0 == player_team {
                    continue;
                }
                for &core in &data.cores {
                    if let Some(pos) = self.sim.ecs.0.get::<Pos>(core) {
                        enemy_cores.push((pos.x, pos.y, team.0));
                    }
                }
            }
        }
        let spawns = self
            .sim
            .unit_runtime()
            .map(|runtime| {
                runtime
                    .wave_spawner
                    .spawns
                    .iter()
                    .map(|pos| {
                        (
                            (pos.x() as f32 + 0.5) * TILESIZE_F,
                            (pos.y() as f32 + 0.5) * TILESIZE_F,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        OverlayView {
            player,
            player_team,
            enemy_units,
            enemy_cores,
            spawns,
        }
    }

    /// `input.config.getSelected().drawConfigure()` dispatch
    /// (`BuildingComp.java:1732-1737`; `PowerNode.java:458-480`,
    /// `ItemBridge.java:282-296`). `Logic` is filled by H06 T12; everything
    /// else falls back to the generic accent square.
    pub(crate) fn config_overlay(&self, x: i32, y: i32) -> Option<ConfigOverlay> {
        use mind_core::world::modules::PowerModule;
        let content = self.content_snapshot.as_ref()?;
        let tile = self.tile_at(x, y)?;
        let entity = tile.build?;
        let def = content.block(tile.block)?;
        let cx = x as f32 * TILESIZE_F + def.offset;
        let cy = y as f32 * TILESIZE_F + def.offset;
        if matches!(def.kind, BlockKind::PowerNode) {
            let laser_range = power_laser_range(&def.name) * TILESIZE_F;
            let mut links = Vec::new();
            if let Some(module) = self.sim.ecs.0.get::<PowerModule>(entity) {
                for packed in &module.links {
                    let (lx, ly) = mind_core::world::pos::unpack(*packed);
                    let Some(other) = self.tile_at(lx as i32, ly as i32) else {
                        continue;
                    };
                    let size = self
                        .content_snapshot
                        .as_ref()
                        .and_then(|content| content.block(other.block))
                        .map(|def| def.size)
                        .unwrap_or(1);
                    links.push((
                        lx as f32 * TILESIZE_F + size as f32 * 4.0,
                        ly as f32 * TILESIZE_F + size as f32 * 4.0,
                        size,
                    ));
                }
            }
            return Some(ConfigOverlay::PowerNodeLinks {
                x: cx,
                y: cy,
                size: def.size,
                laser_range,
                links,
            });
        }
        let bridge_range = bridge_link_range(def);
        if let Some(range) = bridge_range {
            use mind_core::world::blocks::distribution::item_bridge::ItemBridgeBuild;
            let linked = self
                .sim
                .ecs
                .0
                .get::<ItemBridgeBuild>(entity)
                .map(|bridge| bridge.link)
                .unwrap_or(-1);
            let team = self
                .sim
                .ecs
                .0
                .get::<mind_core::entities::comp::TeamComp>(entity)
                .map(|comp| comp.team)
                .unwrap_or(0);
            let mut targets = Vec::new();
            let dirs = [(1, 0), (-1, 0), (0, 1), (0, -1)];
            for (dx, dy) in dirs {
                for step in 1..=range {
                    let (tx, ty) = (x + dx * step, y + dy * step);
                    let Some(other) = self.tile_at(tx, ty) else {
                        continue;
                    };
                    if other.build.is_none() || other.block != tile.block {
                        continue;
                    }
                    let other_team = other
                        .build
                        .and_then(|entity| {
                            self.sim
                                .ecs
                                .0
                                .get::<mind_core::entities::comp::TeamComp>(entity)
                        })
                        .map(|comp| comp.team)
                        .unwrap_or(0);
                    if other_team != team {
                        continue;
                    }
                    targets.push((
                        tx as f32 * TILESIZE_F + 4.0,
                        ty as f32 * TILESIZE_F + 4.0,
                        linked == (tx << 16) | ty,
                    ));
                }
            }
            return Some(ConfigOverlay::BridgeLink {
                x: cx,
                y: cy,
                size: def.size,
                range,
                targets,
            });
        }
        if def.kind == BlockKind::LogicBlock {
            // `LogicBlock.drawConfigure` (`LogicBlock.java:661-683`).
            let privileged = def.privileged;
            // `LogicBlock.range` lives on the family knobs (`LogicBlockDef`).
            let range = (!privileged)
                .then(|| {
                    mind_core::logic::blocks::logic_def_of(&self.sim.ecs.0, entity)
                        .map(|def| def.range)
                        .unwrap_or(0.0)
                })
                .filter(|range| *range > 0.0);
            let links =
                mind_core::logic::blocks::logic_block::overlay_links(&self.sim.ecs.0, entity);
            return Some(ConfigOverlay::Logic {
                x: cx,
                y: cy,
                size: def.size,
                range,
                links,
            });
        }
        Some(ConfigOverlay::Generic {
            x: cx,
            y: cy,
            size: def.size,
        })
    }

    /// `Building.drawSelect` dispatch for the hovered player-team building
    /// (`OverlayRenderer.java:213-229`): turret/projector range circles,
    /// power-node `laserRange` circles (`PowerNode.java:445-453`).
    pub(crate) fn select_overlay(&self, x: i32, y: i32) -> SelectOverlay {
        let Some(tile) = self.tile_at(x, y) else {
            return SelectOverlay::None;
        };
        let Some(entity) = tile.build else {
            return SelectOverlay::None;
        };
        let team = self
            .sim
            .ecs
            .0
            .get::<mind_core::entities::comp::TeamComp>(entity)
            .map(|comp| comp.team)
            .unwrap_or(0);
        let content = self.content_snapshot.as_ref();
        let def = content.and_then(|content| content.block(tile.block));
        let (cx, cy) = def
            .map(|def| {
                (
                    x as f32 * TILESIZE_F + def.offset,
                    y as f32 * TILESIZE_F + def.offset,
                )
            })
            .unwrap_or((x as f32 * TILESIZE_F, y as f32 * TILESIZE_F));
        if team != self.player_team() {
            return SelectOverlay::None;
        }
        if def.is_some_and(|def| def.kind == BlockKind::MessageBlock) {
            // `MessageBlock.drawSelect`: the stored message above the block.
            let text = self
                .sim
                .ecs
                .0
                .get::<mind_core::logic::blocks::MessageBlockState>(entity)
                .map(|state| state.message.clone())
                .unwrap_or_default();
            if !text.is_empty() {
                return SelectOverlay::MessageText { x: cx, y: cy, text };
            }
        }
        if def.is_some_and(|def| def.kind == BlockKind::LogicBlock) {
            // `LogicBuild.drawSelect` (`LogicBlock.java:685-725`).
            let icon_assets = self
                .base()
                .try_get_node_as::<crate::assets::MindAssets>("/root/MindAssets");
            let (tag, icon) = self
                .sim
                .ecs
                .0
                .get::<mind_core::logic::blocks::LogicBlockState>(entity)
                .map(|state| {
                    let icon = (state.icon_tag != '\0')
                        .then(|| {
                            icon_assets.as_ref().map(|assets| {
                                let name = assets.bind().icon_name_for_code(state.icon_tag as i32);
                                let region = assets.bind().icon_region_for_name(name.clone());
                                (name.to_string(), region.to_string())
                            })
                        })
                        .flatten()
                        .filter(|(_, region)| !region.is_empty());
                    (state.tag.clone(), icon)
                })
                .unwrap_or((None, None));
            let mut units = Vec::new();
            for unit in self.sim.ecs.0.iter_entities() {
                let Some(ai) = unit.get::<mind_core::ai::types::logic::LogicAi>() else {
                    continue;
                };
                if ai.controller != Some(entity) {
                    continue;
                }
                let Some(pos) = unit.get::<mind_core::entities::comp::Pos>() else {
                    continue;
                };
                let hit = unit
                    .get::<mind_core::entities::comp::unit::comp::HitboxComp>()
                    .map(|hitbox| hitbox.hit_size)
                    .unwrap_or(0.0);
                let rotation = unit
                    .get::<mind_core::entities::comp::unit::comp::UnitCore>()
                    .map(|core| core.rotation)
                    .unwrap_or(0.0);
                units.push((pos.x, pos.y, hit, rotation));
            }
            if tag.is_some() || icon.is_some() || !units.is_empty() {
                return SelectOverlay::LogicTag {
                    x: cx,
                    y: cy,
                    tag,
                    icon,
                    units,
                };
            }
        }
        if let Some((range, kind, color)) = self.block_place_range(tile.block) {
            return SelectOverlay::Range {
                x: cx,
                y: cy,
                range,
                color,
                circles: matches!(kind, PlaceRangeKind::PowerNode),
            };
        }
        SelectOverlay::None
    }

    /// Debug probe for the logic overlays (I03 T11): the config overlay and
    /// the hover select overlay of the building at `(x, y)` as JSON.
    #[func]
    pub fn overlay_probe(&self, x: i32, y: i32) -> GString {
        let config = match self.config_overlay(x, y) {
            Some(ConfigOverlay::Logic {
                x, y, range, links, ..
            }) => serde_json::json!({
                "kind": "logic",
                "x": x,
                "y": y,
                "range": range,
                "links": links
                    .iter()
                    .map(|(lx, ly, size, name)| serde_json::json!({
                        "x": lx, "y": ly, "size": size, "name": name,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Some(ConfigOverlay::Generic { x, y, size }) => serde_json::json!({
                "kind": "generic", "x": x, "y": y, "size": size,
            }),
            Some(ConfigOverlay::PowerNodeLinks { x, y, .. }) => serde_json::json!({
                "kind": "power-node", "x": x, "y": y,
            }),
            Some(ConfigOverlay::BridgeLink { x, y, .. }) => serde_json::json!({
                "kind": "bridge", "x": x, "y": y,
            }),
            None => serde_json::Value::Null,
        };
        let select = match self.select_overlay(x, y) {
            SelectOverlay::LogicTag {
                x,
                y,
                tag,
                icon,
                units,
            } => serde_json::json!({
                "kind": "logic-tag",
                "x": x,
                "y": y,
                "tag": tag,
                "icon": icon.as_ref().map(|(name, _)| name.as_str()),
                "units": units
                    .iter()
                    .map(|(ux, uy, hit, rotation)| serde_json::json!({
                        "x": ux, "y": uy, "hit": hit, "rotation": rotation,
                    }))
                    .collect::<Vec<_>>(),
            }),
            SelectOverlay::MessageText { x, y, text } => serde_json::json!({
                "kind": "message", "x": x, "y": y, "text": text,
            }),
            SelectOverlay::Range { x, y, range, .. } => serde_json::json!({
                "kind": "range", "x": x, "y": y, "range": range,
            }),
            SelectOverlay::None => serde_json::Value::Null,
        };
        GString::from(
            serde_json::json!({"config": config, "select": select})
                .to_string()
                .as_str(),
        )
    }

    /// Debug probe for the placement range lookup (`Block.drawPlace` /
    /// `drawSelect`): `{"range","kind","color"}` or `"null"` for a block name.
    /// H03 T5 step 6 diagnosis seam.
    #[func]
    pub fn place_range_json(&self, name: GString) -> GString {
        let Some(content) = self.content_snapshot.as_ref() else {
            return GString::from("null");
        };
        let Some(id) = content.block_id(&name.to_string()) else {
            return GString::from("null");
        };
        match self.block_place_range(id) {
            Some((range, kind, color)) => {
                let kind = match kind {
                    PlaceRangeKind::Turret => "turret",
                    PlaceRangeKind::Projector => "projector",
                    PlaceRangeKind::PowerNode => "power-node",
                };
                GString::from(
                    serde_json::json!({"range": range, "kind": kind, "color": color})
                        .to_string()
                        .as_str(),
                )
            }
            None => GString::from("null"),
        }
    }

    // ---- H03 T9: unit factory / reconstructor build preview seam ----

    /// `UnitFactory.draw` / `Reconstructor.draw` construct-preview state at
    /// `(x, y)` (`Drawf.construct`, `UnitFactory.java:380-397`,
    /// `Reconstructor.java:287-317`; H03 T9). `None` for any other block kind.
    pub fn unit_build_draw(&self, x: i32, y: i32) -> Option<UnitBuildDraw> {
        use mind_core::content::BlockKind;
        use mind_core::entities::comp::Building;
        use mind_core::world::blocks::units::{ReconstructorBuild, UnitFactoryBuild};
        let tile = self.tile_at(x, y)?;
        let entity = tile.build?;
        let content = self.content_snapshot.as_ref()?;
        let def = content.block(tile.block)?;
        let size = def.size.max(1);
        // `Building.x/y` = the footprint centre (`Tile.drawx + Block.offset`).
        let cx = (x + (size - 1) / 2) as f32 * 8.0 + def.offset;
        let cy = (y + (size - 1) / 2) as f32 * 8.0 + def.offset;
        let rotation = self.building_rotation(x, y);
        let efficiency = self
            .sim
            .ecs
            .0
            .get::<Building>(entity)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        match def.kind {
            BlockKind::UnitFactory => {
                let state = self.sim.ecs.0.get::<UnitFactoryBuild>(entity)?;
                let (unit, fraction) =
                    factory_preview(&def.unit_plans, state.current_plan, state.progress);
                Some(UnitBuildDraw {
                    reconstructor: false,
                    cx,
                    cy,
                    size,
                    rotation,
                    unit,
                    payload: None,
                    fraction,
                    // `UnitFactory.updateTile`: `speedScl` targets 1 while
                    // `efficiency > 0 && currentPlan != -1`.
                    working: efficiency > 0.0 && unit.is_some(),
                })
            }
            BlockKind::Reconstructor => {
                let state = self.sim.ecs.0.get::<ReconstructorBuild>(entity)?;
                let (unit, fraction, arrived) = reconstructor_preview(
                    &def.reconstructor_upgrades,
                    state.payload,
                    state.progress,
                    def.construct_time,
                    state.move_in,
                );
                Some(UnitBuildDraw {
                    reconstructor: true,
                    cx,
                    cy,
                    size,
                    rotation,
                    unit,
                    payload: state.payload,
                    fraction,
                    // `Reconstructor.updateTile`: `valid` = the payload arrived
                    // (`moveInPayload()`) with a valid upgrade and
                    // `efficiency > 0`.
                    working: efficiency > 0.0 && arrived && unit.is_some(),
                })
            }
            _ => None,
        }
    }

    /// Pod and pad draw rows (`LaunchPayloadComp.draw` `LaunchPad.java:227-280`,
    /// `LaunchPadBuild.draw` `:105-135`, `LandingPadBuild.draw`
    /// `LandingPad.java:205-265`; H03 T13). Rust-only seam for `render/pods.rs`.
    pub fn pad_draws(&mut self) -> Vec<PadDraw> {
        let mut out = Vec::new();
        let block_size = |snapshot: &Option<mind_core::content::ContentRegistry>,
                          block: mind_core::content::BlockId| {
            snapshot
                .as_ref()
                .and_then(|content| content.block(block))
                .map(|def| def.size.max(1))
                .unwrap_or(1)
        };
        for (id, x, y, time, lifetime, block, _team) in
            mind_core::game::launch_payload::payload_rows(&mut self.sim.ecs.0)
        {
            out.push(PadDraw::Launch {
                id,
                x,
                y,
                time,
                lifetime,
                block,
            });
        }
        for (id, x, y, block, _config, arriving, arriving_timer, cooldown) in
            mind_core::game::landing_pad::landing_pad_rows(&mut self.sim.ecs.0)
        {
            out.push(PadDraw::Landing {
                id,
                x,
                y,
                size: block_size(&self.content_snapshot, block),
                arriving: arriving.map(mind_core::content::ItemId::new),
                arriving_timer,
                cooldown,
                block,
            });
        }
        for (id, x, y, block, progress) in
            mind_core::world::behavior::campaign::launch_pad_rows(&mut self.sim.ecs.0)
        {
            out.push(PadDraw::Pad {
                id,
                x,
                y,
                size: block_size(&self.content_snapshot, block),
                progress,
                block,
            });
        }
        out
    }
}

/// `UnitFactory.draw` preview numbers: `(unit, progress / plan.time)` for the
/// selected plan (`UnitFactory.java:380-397`; H03 T9). Pure so it is testable
/// without a Godot host.
pub(crate) fn factory_preview(
    plans: &[mind_core::content::registries::blocks::UnitPlanDef],
    current_plan: i32,
    progress: f32,
) -> (Option<mind_core::content::UnitTypeId>, f32) {
    let plan = usize::try_from(current_plan)
        .ok()
        .and_then(|index| plans.get(index));
    let unit = plan.map(|plan| plan.unit);
    let fraction = plan
        .map(|plan| (progress / plan.time.max(f32::EPSILON)).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    (unit, fraction)
}

/// `Reconstructor.draw` preview numbers: `(upgrade target`,
/// `progress / constructTime`, arrived)` (`Reconstructor.java:287-317`;
/// H03 T9). Pure so it is testable without a Godot host.
pub(crate) fn reconstructor_preview(
    upgrades: &[(
        mind_core::content::UnitTypeId,
        mind_core::content::UnitTypeId,
    )],
    payload: Option<mind_core::content::UnitTypeId>,
    progress: f32,
    construct_time: f32,
    move_in: f32,
) -> (Option<mind_core::content::UnitTypeId>, f32, bool) {
    let target = payload.and_then(|from| {
        upgrades
            .iter()
            .find(|(source, _)| *source == from)
            .map(|(_, target)| *target)
    });
    let fraction = (progress / construct_time.max(f32::EPSILON)).clamp(0.0, 1.0);
    (target, fraction, move_in >= 1.0)
}

/// One pod or pad draw row (`LaunchPad.java`/`LandingPad.java`; H03 T13).
pub enum PadDraw {
    /// Flying launch payload (`LaunchPayloadComp.draw`): `time`/`lifetime`
    /// drive the `fin`/`fout` path.
    Launch {
        /// `LaunchPayload.id()` (view draw seed).
        id: u32,
        /// Pod centre x (`x`).
        x: f32,
        /// Pod centre y (`y`).
        y: f32,
        /// Ticks in flight.
        time: f32,
        /// Total flight time (120).
        lifetime: f32,
        /// Source pad block (`<block>-pod` region).
        block: mind_core::content::BlockId,
    },
    /// Landing pad: an arriving pod or the cooldown pod (`LandingPadBuild.draw`).
    Landing {
        /// Entity index (view state key).
        id: u32,
        /// Pad centre x.
        x: f32,
        /// Pad centre y.
        y: f32,
        /// Pad size in tiles.
        size: i32,
        /// `arriving` item (`None` = nothing inbound).
        arriving: Option<mind_core::content::ItemId>,
        /// `arrivingTimer` (`0..=1`).
        arriving_timer: f32,
        /// `cooldown` (`0..=1`).
        cooldown: f32,
        /// Pad block (`<block>-pod` region).
        block: mind_core::content::BlockId,
    },
    /// Idle launch pad: pod + corner progress lights (`LaunchPadBuild.draw`).
    Pad {
        /// Entity index (view state key).
        id: u32,
        /// Pad centre x.
        x: f32,
        /// Pad centre y.
        y: f32,
        /// Pad size in tiles.
        size: i32,
        /// `min(items.total/itemCapacity, launchCounter/launchTime)`.
        progress: f32,
        /// Pad block (`<block>-pod` / `<block>-light` regions).
        block: mind_core::content::BlockId,
    },
}

/// Unit factory / reconstructor build preview (`Drawf.construct`,
/// `Drawf.java:586-603`; H03 T9). Rust-only seam for `render/unit_build.rs`.
pub struct UnitBuildDraw {
    /// `true`: reconstructor (payload upgrade); `false`: unit factory.
    pub reconstructor: bool,
    /// Block centre world x (`Building.x`).
    pub cx: f32,
    /// Block centre world y (`Building.y`).
    pub cy: f32,
    /// `Block.size` in tiles (`Drawf.construct` line size = `size*8 - 4`).
    pub size: i32,
    /// `Building.rotation` (`0..=3`; drawn as `rotation * 90`).
    pub rotation: u8,
    /// Unit being constructed: the factory plan's unit or the reconstructor
    /// upgrade target (`None` = nothing to preview).
    pub unit: Option<mind_core::content::UnitTypeId>,
    /// Reconstructor payload unit (`Reconstructor.draw` icon at
    /// `1 - progress/constructTime`; the port tracks no payload rotation).
    pub payload: Option<mind_core::content::UnitTypeId>,
    /// `progress / plan.time` (factory) or `progress / constructTime`.
    pub fraction: f32,
    /// `efficiency > 0` with a valid plan/payload (Java `speedScl` target).
    pub working: bool,
}

/// `OverlayRenderer.drawTop` inputs (H03 T5).
pub struct OverlayView {
    /// Player (alpha) centre in world px.
    pub player: Option<(f32, f32)>,
    /// Player team id.
    pub player_team: u8,
    /// Non-player-team units `(x, y, team)`.
    pub enemy_units: Vec<(f32, f32, u8)>,
    /// Enemy core centres `(x, y, team)`.
    pub enemy_cores: Vec<(f32, f32, u8)>,
    /// Ground spawn tiles in world px.
    pub spawns: Vec<(f32, f32)>,
}

/// `Building.drawConfigure` dispatch (`BuildingComp.java:1732-1737`).
/// H06 T12 constructs `Logic`; `BridgeLink::range` stays for its use.
#[allow(dead_code)]
pub(crate) enum ConfigOverlay {
    /// Generic accent square (half-size `size * 4 + 1`).
    Generic { x: f32, y: f32, size: i32 },
    /// `PowerNode.drawConfigure` (`PowerNode.java:458-480`): pulsing accent
    /// rings at the node and at `laserRange`, one `Pal.place` square per link.
    PowerNodeLinks {
        x: f32,
        y: f32,
        size: i32,
        laser_range: f32,
        links: Vec<(f32, f32, i32)>,
    },
    /// `ItemBridge.drawConfigure` (`ItemBridge.java:282-296`): accent/place/
    /// breakInvalid squares on the candidate targets.
    BridgeLink {
        x: f32,
        y: f32,
        size: i32,
        range: i32,
        targets: Vec<(f32, f32, bool)>,
    },
    /// `LogicBuild.drawConfigure` (`LogicBlock.java:661-683`): the non-privileged
    /// link-range circles, one `Pal.place` square per valid link and the link
    /// names above them.
    Logic {
        /// Building centre x/y (world px).
        x: f32,
        y: f32,
        /// `Block.size` (the `super.drawConfigure()` outline).
        size: i32,
        /// `LogicBlock.range` circles (non-privileged only; `None` = none).
        range: Option<f32>,
        /// Valid links: `(centre x, centre y, block size, link name)`.
        links: Vec<(f32, f32, i32, String)>,
    },
}

/// `Building.drawSelect` dispatch (`OverlayRenderer.java:213-229`).
/// H06 T12 constructs `MessageText`/`LogicTag`.
#[allow(dead_code)]
pub(crate) enum SelectOverlay {
    /// Range circle(s) (`Drawf.dashCircle` / `Drawf.circles`).
    Range {
        x: f32,
        y: f32,
        range: f32,
        color: u32,
        circles: bool,
    },
    /// `MessageBlock.drawSelect` (`MessageBlock.java`): the stored message
    /// above the block.
    MessageText {
        /// Building centre x/y (world px).
        x: f32,
        y: f32,
        /// The message text.
        text: String,
    },
    /// `LogicBuild.drawSelect` (`LogicBlock.java:685-725`): the world-processor
    /// tag/icon and one square per unit this processor drives.
    LogicTag {
        /// Building centre x/y (world px).
        x: f32,
        y: f32,
        /// World-processor tag text (drawn above the block).
        tag: Option<String>,
        /// World-processor icon tag: `(Fonts.unicodeToName, UI region name)`.
        icon: Option<(String, String)>,
        /// Driven units: `(x, y, hit size, rotation)` (`LogicAI.controller == e`).
        units: Vec<(f32, f32, f32, f32)>,
    },
    /// No overlay.
    None,
}

/// `Block.offset` world-pixel centre of a tile (H01 T1 convention).
const TILESIZE_F: f32 = mind_core::config::TILESIZE as f32;

/// `PowerNode.laserRange` per name (`Blocks.java:2483-2504`), in tiles.
fn power_laser_range(name: &str) -> f32 {
    match name {
        "power-node" => 6.0,
        "power-node-large" => 15.0,
        "surge-tower" => 40.0,
        _ => 0.0,
    }
}

/// `ItemBridge.range` for bridge kinds (`ItemBridge.java`), in tiles.
fn bridge_link_range(def: &mind_core::content::BlockDef) -> Option<i32> {
    match def.name.as_str() {
        "bridge-conveyor" => Some(4),
        "phase-conveyor" => Some(12),
        _ => None,
    }
}

/// `PlacementFragment` unit table + command/stance rows (`PlacementFragment.java:443-676`).
///
/// `units` holds `(type id, live command, stance bits)` per selected unit.
/// Types are counted and iterated by ascending type id; `commands` is the
/// union of each type's `UnitType.commands` in that order, deduped
/// (`PlacementFragment.java:564-570`), `stances` the union of
/// `UnitType.getUnitStances`. A row is reported only when its union has more
/// than one entry (`:574`, `:590`). `active` = any unit's live command
/// (explicit, else `UnitType.defaultCommand`) equals it, / any stance bit is
/// set (`:619-630`).
pub(crate) fn command_panel(
    content: &ContentRegistry,
    units: &[(i32, Option<UnitCommandId>, StanceBits)],
) -> serde_json::Value {
    use mind_core::ai::unit_command_runtime::{default_command, get_unit_stances};
    let mut counts: std::collections::BTreeMap<i32, i32> = std::collections::BTreeMap::new();
    let mut ordered: Vec<&(i32, Option<UnitCommandId>, StanceBits)> = units.iter().collect();
    ordered.sort_by_key(|(type_id, _, _)| *type_id);
    let mut commands: Vec<UnitCommandId> = Vec::new();
    let mut stances: Vec<UnitStanceId> = Vec::new();
    let mut active_commands: Vec<UnitCommandId> = Vec::new();
    let mut active_stances: Vec<UnitStanceId> = Vec::new();
    for (type_id, _, _) in units {
        *counts.entry(*type_id).or_default() += 1;
    }
    for (type_id, current, bits) in ordered {
        let Some(def) = content.unit(UnitTypeId::new(*type_id as u16)) else {
            continue;
        };
        for command in &def.commands {
            if !commands.contains(command) {
                commands.push(*command);
            }
        }
        let current = current.or_else(|| default_command(def, content).map(|def| def.id));
        if let Some(command) = current
            && !active_commands.contains(&command)
        {
            active_commands.push(command);
        }
        let mut unit_stances = Vec::new();
        get_unit_stances(def, content, &mut unit_stances);
        for stance in unit_stances {
            if !stances.contains(&stance) {
                stances.push(stance);
            }
            if bits.get(stance) && !active_stances.contains(&stance) {
                active_stances.push(stance);
            }
        }
    }
    let type_rows: Vec<serde_json::Value> = counts
        .iter()
        .map(|(type_id, count)| {
            let name = content
                .unit(UnitTypeId::new(*type_id as u16))
                .map(|def| def.name.clone())
                .unwrap_or_default();
            serde_json::json!({"type": type_id, "name": name, "count": count})
        })
        .collect();
    let command_rows: Vec<serde_json::Value> = if commands.len() > 1 {
        commands
            .iter()
            .filter_map(|id| {
                content.unit_command(*id).map(|def| {
                    serde_json::json!({
                        "name": def.name,
                        "icon": def.icon,
                        "key": def.localized_key,
                        "active": active_commands.contains(id),
                    })
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    let stance_rows: Vec<serde_json::Value> = if stances.len() > 1 {
        stances
            .iter()
            .filter_map(|id| {
                content.unit_stance(*id).map(|def| {
                    serde_json::json!({
                        "name": def.name,
                        "icon": def.icon,
                        "key": def.localized_key,
                        "active": active_stances.contains(id),
                    })
                })
            })
            .collect()
    } else {
        Vec::new()
    };
    serde_json::json!({
        "types": type_rows,
        "commands": command_rows,
        "stances": stance_rows,
    })
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

    // `state.stats.getDestroyed(...)` sum (`HintsFragment.java:285`,
    // `serpuloCoreZone`): the session's counters are name-keyed since H09 T10.
    let cores_destroyed = runtime
        .map(|runtime| {
            let stats = &runtime.session.stats;
            i64::from(stats.get_destroyed("core-shard"))
                + i64::from(stats.get_destroyed("core-foundation"))
                + i64::from(stats.get_destroyed("core-nucleus"))
        })
        .unwrap_or(0);
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
        V::Bytes(bytes) => json!({
            "type": "bytes",
            "len": bytes.len(),
            // The canvas editor reads the packed payload (`CanvasEditDialog`).
            "data": bytes.iter().copied().collect::<Vec<u8>>()
        }),
        V::String(text) => json!({"type": "string", "value": text}),
        V::IntArray(values) => json!({"type": "int_array", "values": values}),
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

/// Sector-only inputs of the shared [`MindSimHost::finish_world_load`] tail;
/// the map path leaves every one unset.
#[derive(Default)]
struct WorldLoadExtras {
    /// Generated-sector polygon `(rotation, sides)` (`World.getDarkness`,
    /// `World.java:505-529`); the size is the loaded grid's width.
    polygon: Option<(f32, i32)>,
    /// `Schematics.placeLaunchLoadout` for a generated sector, run once the
    /// fresh sim + runtimes are live (H05 T4).
    generated_loadout: Option<GeneratedLoadout>,
    /// The save supplied the world: stash the resume metadata + saved tick.
    resume: bool,
}

/// The deferred generated-sector loadout step (`H05 T4`).
type GeneratedLoadout = Box<dyn FnOnce(&mut MindSimHost)>;

/// `World.setSectorRules` (`World.java:300`): `state.map` is named after the
/// preset (`preset.localizedName`) or `"<planet>; Sector <id>"` for a
/// generated sector (L02 T4).
fn sector_map_name(registry: &ContentRegistry, planet_name: &str, sector: i32) -> String {
    let Some(planet) = registry.planet_by_name(planet_name) else {
        return String::new();
    };
    let preset = planet
        .sectors
        .get(sector.max(0) as usize)
        .and_then(|cell| cell.preset)
        .and_then(|preset| registry.sector(preset));
    match preset {
        Some(preset) => preset.unlock.localized_name.clone(),
        None => format!("{}; Sector {sector}", planet.unlock.localized_name),
    }
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
/// fires, puddles, markers, stats, meta tags). Plain loads (Load Game, sector
/// resume) never re-run generation filters.
fn load_grid_file(path: &str, registry: &mut ContentRegistry) -> Option<LoadedGrid> {
    let bytes = FileAccess::get_file_as_bytes(path);
    let save = match mind_core::io::sim_io::decode_save_bytes(bytes.as_slice(), registry) {
        Ok(save) => save,
        Err(error) => {
            log::warn!("load_map_file: `{path}` failed: {error}");
            return None;
        }
    };
    let loaded = take_loaded_grid(save);
    Some(loaded)
}

/// [`load_grid_file`] for map loads: runs the `World.FilterContext` step with
/// `seed` (`World.loadMap`, `World.java:341-357,661-695`), so the map's
/// `genfilters`/default stack lands ore and boulders before `endMapLoad`.
fn load_map_grid_file(path: &str, registry: &mut ContentRegistry, seed: u64) -> Option<LoadedGrid> {
    let bytes = FileAccess::get_file_as_bytes(path);
    let save = match mind_core::io::sim_io::decode_map_bytes(bytes.as_slice(), registry, seed) {
        Ok(save) => save,
        Err(error) => {
            log::warn!("load_map_file: `{path}` failed: {error}");
            return None;
        }
    };
    if let Some(seed) = save.map_gen_seed {
        log::info!("load_map_file: `{path}` gen seed {seed}");
    }
    Some(take_loaded_grid(save))
}

/// Moves the world half out of a decoded save (callers keep their current
/// shape); the resume half stays in `save` for the sector-resume path.
fn take_loaded_grid(mut save: mind_core::io::sim_io::DecodedSave) -> LoadedGrid {
    let grid = std::mem::take(&mut save.grid);
    let pending_buildings = std::mem::take(&mut save.pending);
    let plans = std::mem::take(&mut save.plans);
    let rules = save.rules.take();
    LoadedGrid {
        grid,
        rules,
        pending_buildings,
        plans,
        save: Some(save),
    }
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

/// `(x, y)` tile-coordinate list for the `world_gen_debug` probe.
type GenCoords = Vec<(i32, i32)>;

/// Counts generated ore overlays (`OreBlock`) and boulder props (the
/// `StaticProp` blocks named `*-boulder`) on a loaded grid, returning the
/// counts plus a few ore and boulder tile coordinates (J01 T1 debug probe; the
/// generation filters' observable output).
fn count_world_gen(
    grid: &WorldGrid,
    content: &ContentRegistry,
) -> (i64, i64, GenCoords, GenCoords) {
    let mut ore = 0i64;
    let mut boulders = 0i64;
    let mut ore_sample: GenCoords = Vec::new();
    let mut boulder_sample: GenCoords = Vec::new();
    for (pos, index) in grid.iter_row_major() {
        let tile = grid.tile_ref(index);
        if content
            .block(tile.overlay)
            .is_some_and(|def| def.kind == BlockKind::OreBlock)
        {
            ore += 1;
            if ore_sample.len() < 8 {
                ore_sample.push((i32::from(pos.x()), i32::from(pos.y())));
            }
        }
        if content
            .block(tile.block)
            .is_some_and(|def| def.kind == BlockKind::StaticProp && def.name.contains("boulder"))
        {
            boulders += 1;
            if boulder_sample.len() < 8 {
                boulder_sample.push((i32::from(pos.x()), i32::from(pos.y())));
            }
        }
    }
    (ore, boulders, ore_sample, boulder_sample)
}

/// Packs `(x, y)` pairs into a flat `PackedInt32Array` (debug probes).
fn pack_coords(coords: GenCoords) -> PackedInt32Array {
    let mut packed = PackedInt32Array::new();
    for (x, y) in coords {
        packed.push(x);
        packed.push(y);
    }
    packed
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
    /// Bar key (`"health"`, `"power"`, `"liquid"`, `"progress"`, ...).
    pub key: &'static str,
    /// Bundle key (no `@`).
    pub label_key: String,
    /// Arguments for the bundle format (`{0}`, `{1}`, `{2}`).
    pub label_args: Vec<String>,
    /// `Bar` colour RGBA8888.
    pub color: u32,
    /// Fraction `0..=1`.
    pub value: f32,
}

/// `Turret` family bar state (`BaseTurret.java:104-110`,
/// `Turret.java:201-207`, `ItemTurret.java:61-67`).
#[derive(Debug, Clone, Default)]
pub struct TurretBars {
    pub total_ammo: i32,
    pub max_ammo: i32,
    pub heat_req: f32,
    pub heat_requirement: f32,
    pub max_heat_efficiency: f32,
    pub activation_timer: f32,
    pub activation_time: f32,
}

/// Power-graph bar state (`PowerNode.java:112-141`, `PowerDiode.java:33-52`).
#[derive(Debug, Clone, Default)]
pub struct PowerBars {
    pub balance: f32,
    pub produced: f32,
    pub needed: f32,
    pub stored: f32,
    pub capacity: f32,
    pub links: usize,
    pub max_nodes: usize,
    /// Diode back/front graph stored fractions (`PowerDiode.bar`).
    pub back: Option<f32>,
    pub front: Option<f32>,
}

/// `PowerGenerator`/`ImpactReactor` bar state
/// (`PowerGenerator.java:88-96`, `ImpactReactor.java:47-54`).
#[derive(Debug, Clone, Default)]
pub struct GeneratorBars {
    /// `getPowerProduction()` (per tick, efficiency included).
    pub production: f32,
    /// `ConsPower.usage` (ImpactReactor net output).
    pub usage: f32,
    /// `productionEfficiency`.
    pub efficiency: f32,
}

/// A factory/reconstructor/cargo-loader `units` bar (three arguments).
#[derive(Debug, Clone, Default)]
pub struct UnitCapBars {
    pub name: String,
    pub count: i32,
    /// `None` = uncapped (`∞`).
    pub cap: Option<i32>,
    pub use_cap: bool,
    /// `false` when the factory has no current plan (`[lightgray]✖`).
    pub has_unit: bool,
}

/// `PayloadLoader` bar state (`PayloadLoader.java:60-66`).
#[derive(Debug, Clone, Default)]
pub struct PayloadLoaderBars {
    pub progress: f32,
    pub payload_has_items: bool,
    pub payload_items: i32,
}

/// Live values the `Block.setBars` family overrides read (J05 T4, L05 T1).
#[derive(Debug, Clone, Default)]
pub struct BarExtras {
    /// `HeatBlock.heat()` and `heatOutput`/`heatRequirement`
    /// (`HeatProducer.java:40`, `HeatConductor.java:34`, `HeatCrafter.java:28`).
    pub heat: Option<(f32, f32)>,
    /// `DrillBuild.lastDrillSpeed`/`warmup` (`Drill.java:117-119`).
    pub drill: Option<(f32, f32)>,
    /// `Building.timeScale()`.
    pub time_scale: f32,
    /// `CoreBuild.storageCapacity` plus the unlocked item count
    /// (`CoreBlock.java:148-152`).
    pub core_capacity: Option<(f32, f32)>,
    /// `UnitFactoryBuild`: `(progress, currentPlan)`
    /// (`UnitFactory.java:121-125`).
    pub factory: Option<(f32, i32)>,
    pub turret: Option<TurretBars>,
    pub power: Option<PowerBars>,
    pub generator: Option<GeneratorBars>,
    /// `NuclearReactorBuild.heat` (`NuclearReactor.java:103`).
    pub reactor_heat: Option<f32>,
    /// `VariableReactorBuild`: `(heat, maxHeat, instability)`
    /// (`VariableReactor.java:57-63`).
    pub variable_reactor: Option<(f32, f32, f32)>,
    /// `OverdriveBuild`: `(realBoost, maxBoost)` (`OverdriveProjector.java:80`).
    pub overdrive: Option<(f32, f32)>,
    /// `ForceBuild` shield fill (`ForceProjector.java:99`).
    pub shield: Option<f32>,
    /// `SolidPumpBuild`: `(lastPump, warmup, efficiency)`
    /// (`SolidPump.java:51-55`).
    pub pump: Option<(f32, f32, f32)>,
    pub payload_loader: Option<PayloadLoaderBars>,
    /// `UnitAssemblerBuild`: `(progress, time, unit bars)`
    /// (`UnitAssembler.java:128-143`).
    pub assembler: Option<(f32, f32, Option<UnitCapBars>)>,
    /// `Reconstructor`/`UnitFactory` `units` bar (`UnitFactory.java:127-136`).
    pub factory_units: Option<UnitCapBars>,
    /// `UnitCargoLoader` `units` bar (`UnitCargoLoader.java:51-61`).
    pub cargo_units: Option<UnitCapBars>,
    /// `LaunchPadBuild`: `(launchCounter, launchTime)`
    /// (`LaunchPad.java:69-70`).
    pub launch: Option<(f32, f32)>,
    /// `LandingPadBuild.cooldown` (`LandingPad.java:113-115`).
    pub landing_cooldown: Option<f32>,
    /// `Accelerator` graph battery stored and `powerBufferRequirement`.
    pub accelerator: Option<(f32, f32)>,
    /// `TargetDummyBuild`: `(reset, resetTime)` (`TargetDummy.java:64-73`).
    pub target_dummy: Option<(f32, f32)>,
    /// `GenericCrafter.outputLiquids` output bars: `(name, rgba8888, amount,
    /// capacity)` (`GenericCrafter.java:81-98`).
    pub output_liquids: Vec<(String, u32, f32, f32)>,
}

impl BarExtras {
    fn turret_heat_efficiency(&self) -> f32 {
        self.turret
            .as_ref()
            .map_or(1.0, |turret| turret.max_heat_efficiency.max(1.0))
    }
}

fn amount(value: f32) -> String {
    mind_core::ui::text::format_amount_default(value as i64)
        .replace("[gray]", "")
        .replace("[]", "")
}

fn cap_text(cap: Option<i32>) -> String {
    cap.map_or_else(|| String::from("∞"), |cap| cap.to_string())
}

fn push_unit_cap_rows(rows: &mut Vec<BarRow>, bars: &UnitCapBars) {
    // `bar.unitcap` = `{0} {1}/{2}` (`UnitFactory.java:127-136`).
    let name = format!(":{}:", bars.name);
    if !bars.has_unit {
        rows.push(BarRow {
            key: "units",
            label_key: String::from("bar.unitcap"),
            label_args: vec![name, String::from("0"), cap_text(bars.cap)],
            color: 0xec7b_4cff,
            value: 0.0,
        });
        return;
    }
    let cap = if bars.use_cap { bars.cap } else { None };
    let value = match (bars.use_cap, cap) {
        (true, Some(cap)) if cap > 0 => (bars.count as f32 / cap as f32).clamp(0.0, 1.0),
        _ => 1.0,
    };
    rows.push(BarRow {
        key: "units",
        label_key: String::from("bar.unitcap"),
        label_args: vec![name, bars.count.to_string(), cap_text(cap)],
        color: 0xec7b_4cff,
        value,
    });
}

pub fn building_bar_rows(
    def: &mind_core::content::registries::blocks::BlockDef,
    health: f32,
    power_status: Option<f32>,
    items_total: Option<i32>,
    liquids: Option<&[f32]>,
    liquid_info: &[(String, u32)],
    extras: &BarExtras,
) -> Vec<BarRow> {
    use mind_core::content::BlockKind;
    use mind_core::content::registries::blocks::Consume;
    let mut rows = Vec::new();
    // `TargetDummy.java:64-73`: `removeBar("health")` and a `dps` bar.
    if def.kind == BlockKind::TargetDummy {
        if let Some((reset, reset_time)) = extras.target_dummy {
            let fraction = if reset_time > 0.0 {
                (1.0 - reset / reset_time).clamp(0.0, 1.0)
            } else {
                0.0
            };
            rows.push(BarRow {
                key: "dps",
                label_key: String::from("bar.dps"),
                label_args: Vec::new(),
                color: 0xff89_47ff,
                value: fraction,
            });
        }
    } else {
        rows.push(BarRow {
            key: "health",
            label_key: String::from("stat.health"),
            label_args: Vec::new(),
            color: 0xff34_1cff,
            value: health.clamp(0.0, 1.0),
        });
    }
    if let Some(status) = power_status {
        let buffered = def.consumes.iter().find_map(|spec| match &spec.consume {
            Consume::Power { buffered, .. } if *buffered > 0.0 => Some(*buffered),
            _ => None,
        });
        let (label_key, label_args) = match buffered {
            Some(capacity) => (
                String::from("bar.poweramount"),
                vec![((status * capacity) as i32).to_string()],
            ),
            None => (String::from("bar.power"), Vec::new()),
        };
        rows.push(BarRow {
            key: "power",
            label_key,
            label_args,
            color: 0xec7b_4cff,
            value: status.clamp(0.0, 1.0),
        });
    }
    // `ItemSource.java:41`, `Unloader.java:74`, `DirectionalUnloader.java:79`:
    // the item bar is removed for these families.
    let removes_items = matches!(
        def.kind,
        BlockKind::ItemSource | BlockKind::Unloader | BlockKind::DirectionalUnloader
    );
    // `LiquidSource.java:45`, `LiquidVoid.java:23`, `LiquidJunction.java:29`:
    // the liquid bar is removed for these families.
    let removes_liquid = matches!(
        def.kind,
        BlockKind::LiquidSource | BlockKind::LiquidVoid | BlockKind::LiquidJunction
    );
    // `LaunchPad.java:69-70` appends its own `items` bar after the base rows.
    let launch_items = def.kind == BlockKind::LaunchPad;
    if def.has_items
        && def.configurable
        && def.item_capacity > 0
        && !removes_items
        && !launch_items
        && let Some(total) = items_total
    {
        rows.push(BarRow {
            key: "items",
            label_key: String::from("bar.items"),
            label_args: vec![total.to_string()],
            color: 0x2ea7_56ff,
            value: (total as f32 / def.item_capacity as f32).clamp(0.0, 1.0),
        });
    }
    // `GenericCrafter.java:81-98`: output liquids replace the dynamic bar.
    if !extras.output_liquids.is_empty() {
        for (name, color, amount, capacity) in &extras.output_liquids {
            rows.push(BarRow {
                key: "liquid",
                label_key: format!("liquid.{name}.name"),
                label_args: Vec::new(),
                color: *color,
                value: if *capacity > 0.0 {
                    (amount / capacity).clamp(0.0, 1.0)
                } else {
                    0.0
                },
            });
        }
    } else if def.has_liquids && def.liquid_capacity > 0.0 && !removes_liquid {
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
                    label_args: Vec::new(),
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
                label_args: Vec::new(),
                color,
                value: (amount / def.liquid_capacity).clamp(0.0, 1.0),
            });
        }
    }
    // Family `addBar` calls append after the defaults, in `setBars` order.
    match def.kind {
        BlockKind::CoreBlock => {
            if let Some((capacity, unlocked)) = extras.core_capacity
                && capacity > 0.0
                && unlocked > 0.0
            {
                rows.push(BarRow {
                    key: "capacity",
                    label_key: String::from("bar.capacity"),
                    label_args: vec![amount(capacity)],
                    color: 0x2ea7_56ff,
                    value: (items_total.unwrap_or(0) as f32 / (capacity * unlocked))
                        .clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::Drill => {
            if let Some((speed, warmup)) = extras.drill {
                rows.push(BarRow {
                    key: "drillspeed",
                    label_key: String::from("bar.drillspeed"),
                    label_args: vec![format!("{:.2}", speed * 60.0 * extras.time_scale)],
                    color: 0xff89_47ff,
                    value: warmup.clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::HeatProducer | BlockKind::HeatConductor => {
            if let Some((heat, output)) = extras.heat
                && output > 0.0
            {
                rows.push(BarRow {
                    key: "heat",
                    label_key: String::from("bar.heat"),
                    label_args: Vec::new(),
                    color: 0xf680_21ff,
                    value: (heat / output).clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::HeatCrafter => {
            if let Some((heat, requirement)) = extras.heat
                && requirement > 0.0
            {
                // `HeatCrafter.java:28-34`: `bar.heatpercent` = `{0} ({1}%)`.
                rows.push(BarRow {
                    key: "heat",
                    label_key: String::from("bar.heatpercent"),
                    label_args: vec![
                        format!("{}", heat as i32),
                        format!(
                            "{}",
                            (heat / requirement).min(extras.turret_heat_efficiency()) * 100.0
                        ),
                    ],
                    color: 0xf680_21ff,
                    value: (heat / requirement).clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::UnitFactory | BlockKind::Reconstructor => {
            if let Some((progress, plan)) = extras.factory {
                let time = usize::try_from(plan)
                    .ok()
                    .and_then(|index| def.unit_plans.get(index))
                    .map(|unit| unit.time)
                    .unwrap_or(0.0);
                let fraction = if time > 0.0 {
                    (progress / time).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                rows.push(BarRow {
                    key: "progress",
                    label_key: String::from("bar.progress"),
                    label_args: vec![format!("{}", (fraction * 100.0) as i32)],
                    color: 0xff89_47ff,
                    value: fraction,
                });
            }
            if let Some(units) = &extras.factory_units {
                push_unit_cap_rows(&mut rows, units);
            }
        }
        BlockKind::UnitAssembler => {
            if let Some((progress, _time, units)) = &extras.assembler {
                rows.push(BarRow {
                    key: "progress",
                    label_key: String::from("bar.progress"),
                    label_args: vec![format!("{}", (progress * 100.0) as i32)],
                    color: 0xff89_47ff,
                    value: progress.clamp(0.0, 1.0),
                });
                if let Some(units) = units {
                    push_unit_cap_rows(&mut rows, units);
                }
            }
        }
        BlockKind::UnitCargoLoader => {
            if let Some(units) = &extras.cargo_units {
                push_unit_cap_rows(&mut rows, units);
            }
        }
        BlockKind::ItemTurret
        | BlockKind::LiquidTurret
        | BlockKind::PowerTurret
        | BlockKind::LaserTurret
        | BlockKind::ContinuousLiquidTurret
        | BlockKind::ContinuousTurret
        | BlockKind::PointDefenseTurret
        | BlockKind::TractorBeamTurret
        | BlockKind::RepairTurret
        | BlockKind::BuildTurret => {
            if let Some(turret) = &extras.turret {
                if turret.activation_time > 0.0 {
                    // `BaseTurret.java:104-110`.
                    let active = turret.activation_timer > 0.0;
                    rows.push(BarRow {
                        key: "activationtimer",
                        label_key: String::from(if active {
                            "bar.activationtimer"
                        } else {
                            "bar.activated"
                        }),
                        label_args: if active {
                            vec![format!(
                                "{}",
                                (turret.activation_timer / 60.0).ceil() as i32
                            )]
                        } else {
                            Vec::new()
                        },
                        color: if active { 0xf680_21ff } else { 0x8ca9_e8ff },
                        value: (1.0 - turret.activation_timer / turret.activation_time)
                            .clamp(0.0, 1.0),
                    });
                }
                if turret.heat_requirement > 0.0 {
                    // `Turret.java:201-207`.
                    let percent = (turret.heat_req / turret.heat_requirement)
                        .min(turret.max_heat_efficiency)
                        * 100.0;
                    rows.push(BarRow {
                        key: "heat",
                        label_key: String::from("bar.heatpercent"),
                        label_args: vec![
                            format!("{}", turret.heat_req as i32),
                            format!("{}", percent as i32),
                        ],
                        color: 0xf680_21ff,
                        value: (turret.heat_req / turret.heat_requirement).clamp(0.0, 1.0),
                    });
                }
                if turret.max_ammo > 0 {
                    // `ItemTurret.java:61-67`.
                    rows.push(BarRow {
                        key: "ammo",
                        label_key: String::from("stat.ammo"),
                        label_args: Vec::new(),
                        color: 0xff89_47ff,
                        value: (turret.total_ammo as f32 / turret.max_ammo as f32).clamp(0.0, 1.0),
                    });
                }
            }
        }
        BlockKind::PowerNode
        | BlockKind::LongPowerNode
        | BlockKind::BeamNode
        | BlockKind::PowerDiode
        | BlockKind::Accelerator => {
            if let Some(power) = &extras.power {
                if def.kind != BlockKind::PowerDiode {
                    // `PowerNode.java:115-122`, `BeamNode.java:53-54`.
                    rows.push(BarRow {
                        key: "power",
                        label_key: String::from("bar.powerbalance"),
                        label_args: vec![format!(
                            "{}{}",
                            if power.balance >= 0.0 { "+" } else { "" },
                            amount(power.balance * 60.0)
                        )],
                        color: 0xec7b_4cff,
                        value: if power.needed > 0.0 {
                            (power.produced / power.needed).clamp(0.0, 1.0)
                        } else {
                            0.0
                        },
                    });
                    rows.push(BarRow {
                        key: "batteries",
                        label_key: String::from("bar.powerstored"),
                        label_args: vec![amount(power.stored), amount(power.capacity)],
                        color: 0xec7b_4cff,
                        value: if power.capacity > 0.0 {
                            (power.stored / power.capacity).clamp(0.0, 1.0)
                        } else {
                            0.0
                        },
                    });
                    if def.kind != BlockKind::BeamNode && power.max_nodes > 0 {
                        rows.push(BarRow {
                            key: "connections",
                            label_key: String::from("bar.powerlines"),
                            label_args: vec![power.links.to_string(), power.max_nodes.to_string()],
                            color: 0x2ea7_56ff,
                            value: (power.links as f32 / power.max_nodes as f32).clamp(0.0, 1.0),
                        });
                    }
                }
                if def.kind == BlockKind::PowerDiode {
                    for (key, label, value) in [
                        ("back", "bar.input", power.back),
                        ("front", "bar.output", power.front),
                    ] {
                        rows.push(BarRow {
                            key,
                            label_key: String::from(label),
                            label_args: Vec::new(),
                            color: 0xec7b_4cff,
                            value: value.unwrap_or(0.0).clamp(0.0, 1.0),
                        });
                    }
                }
            }
            if def.kind == BlockKind::Accelerator
                && let Some((stored, requirement)) = extras.accelerator
                && requirement > 0.0
            {
                rows.push(BarRow {
                    key: "powerBufferRequirement",
                    label_key: String::from("bar.powerbuffer"),
                    label_args: vec![amount(stored), amount(requirement)],
                    color: 0xec7b_4cff,
                    value: (stored / requirement).clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::NuclearReactor => {
            if let Some(heat) = extras.reactor_heat {
                rows.push(BarRow {
                    key: "heat",
                    label_key: String::from("bar.heat"),
                    label_args: Vec::new(),
                    color: 0xf680_21ff,
                    value: heat.clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::VariableReactor => {
            if let Some((heat, max_heat, instability)) = extras.variable_reactor {
                rows.push(BarRow {
                    key: "instability",
                    label_key: String::from("bar.instability"),
                    label_args: Vec::new(),
                    color: 0x665c_9fff,
                    value: instability.clamp(0.0, 1.0),
                });
                if max_heat > 0.0 {
                    rows.push(BarRow {
                        key: "heat",
                        label_key: String::from("bar.heatpercent"),
                        label_args: vec![
                            format!("{}", heat as i32),
                            format!("{}", (heat / max_heat).clamp(0.0, 1.0) * 100.0),
                        ],
                        color: 0xf680_21ff,
                        value: (heat / max_heat).clamp(0.0, 1.0),
                    });
                }
            }
        }
        BlockKind::ConsumeGenerator
        | BlockKind::ThermalGenerator
        | BlockKind::SolarGenerator
        | BlockKind::HeaterGenerator
        | BlockKind::ImpactReactor => {
            if let Some(generator) = &extras.generator {
                // `PowerGenerator.java:88-96`; `ImpactReactor.java:44-54` nets
                // the consumed power.
                let production = (generator.production - generator.usage).max(0.0);
                rows.push(BarRow {
                    key: "power",
                    label_key: String::from("bar.poweroutput"),
                    label_args: vec![format!("{:.1}", production * 60.0 * extras.time_scale)],
                    color: 0xec7b_4cff,
                    value: generator.efficiency.clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::OverdriveProjector => {
            if let Some((boost, max)) = extras.overdrive {
                rows.push(BarRow {
                    key: "boost",
                    label_key: String::from("bar.boost"),
                    label_args: vec![format!(
                        "{}",
                        ((boost * 100.0 - 100.0).max(0.0)).round() as i32
                    )],
                    color: 0xffd3_7fff,
                    value: if max > 0.0 {
                        (boost / max).clamp(0.0, 1.0)
                    } else {
                        0.0
                    },
                });
            }
        }
        BlockKind::ForceProjector => {
            if let Some(fill) = extras.shield {
                rows.push(BarRow {
                    key: "shield",
                    label_key: String::from("stat.shieldhealth"),
                    label_args: Vec::new(),
                    color: 0xffd3_7fff,
                    value: fill.clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::SolidPump | BlockKind::Fracker => {
            if let Some((last_pump, warmup, efficiency)) = extras.pump {
                rows.push(BarRow {
                    key: "efficiency",
                    label_key: String::from("bar.pumpspeed"),
                    label_args: vec![format!("{:.1}", last_pump * 60.0)],
                    color: 0xff89_47ff,
                    value: (warmup * efficiency).clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::PayloadLoader | BlockKind::PayloadUnloader => {
            if let Some(loader) = &extras.payload_loader {
                let has_items = loader.payload_has_items;
                rows.push(BarRow {
                    key: "progress",
                    label_key: String::from(if has_items {
                        "bar.items"
                    } else {
                        "bar.loadprogress"
                    }),
                    label_args: if has_items {
                        vec![loader.payload_items.to_string()]
                    } else {
                        Vec::new()
                    },
                    color: 0x2ea7_56ff,
                    value: loader.progress.clamp(0.0, 1.0),
                });
            }
        }
        BlockKind::LaunchPad => {
            if let Some(total) = items_total {
                rows.push(BarRow {
                    key: "items",
                    label_key: String::from("bar.items"),
                    label_args: vec![total.to_string()],
                    color: 0x2ea7_56ff,
                    value: (total as f32 / def.item_capacity.max(1) as f32).clamp(0.0, 1.0),
                });
            }
            if let Some((counter, time)) = extras.launch {
                rows.push(BarRow {
                    key: "progress",
                    label_key: String::from("bar.launchcooldown"),
                    label_args: Vec::new(),
                    color: 0xff89_47ff,
                    value: if time > 0.0 {
                        (counter / time).clamp(0.0, 1.0)
                    } else {
                        0.0
                    },
                });
            }
        }
        BlockKind::LandingPad => {
            if let Some(cooldown) = extras.landing_cooldown {
                rows.push(BarRow {
                    key: "cooldown",
                    label_key: String::from("bar.cooldown"),
                    label_args: Vec::new(),
                    color: 0xf680_21ff,
                    value: cooldown.clamp(0.0, 1.0),
                });
            }
        }
        _ => {}
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::content::create_base_content;
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

    fn bar_rows(def_name: &str, extras: &BarExtras, content: &ContentRegistry) -> Vec<BarRow> {
        let Some(def) = content.block_by_name(def_name) else {
            panic!("block {def_name}");
        };
        building_bar_rows(def, 1.0, Some(0.5), Some(0), Some(&[]), &[], extras)
    }

    #[test]
    fn bar_rows_follow_the_java_overrides() {
        // J05 T4: `Block.setBars` + the `addBar`/`removeBar` calls in `world/**`.
        let content = base_registry();
        let extras = BarExtras {
            core_capacity: Some((4000.0, 16.0)),
            drill: Some((12.5, 0.5)),
            heat: Some((5.0, 10.0)),
            time_scale: 1.0,
            factory: Some((450.0, 0)),
            ..BarExtras::default()
        };
        // `CoreBlock.java:148-152`: `bar.capacity` appended after the defaults.
        let core = bar_rows("core-shard", &extras, &content);
        let Some(capacity) = core.iter().find(|row| row.key == "capacity") else {
            panic!("core capacity bar missing");
        };
        assert_eq!(capacity.label_key, "bar.capacity");
        assert_eq!(capacity.color, 0x2ea7_56ff);
        assert_eq!(capacity.value, 0.0, "0 items over 4000 * 16");
        assert_eq!(
            capacity.label_args.first().map(String::as_str),
            Some(
                mind_core::ui::text::format_amount_default(4000)
                    .replace("[gray]", "")
                    .replace("[]", "")
                    .as_str()
            ),
            "`UI.formatAmount(storageCapacity)` with the markup stripped"
        );
        // `ItemSource.java:41` / `Unloader.java:74` / `DirectionalUnloader.java:79`.
        for name in ["item-source", "unloader", "duct-unloader"] {
            assert!(
                !bar_rows(name, &extras, &content)
                    .iter()
                    .any(|row| row.key == "items"),
                "{name} removes the item bar"
            );
        }
        // `LiquidSource.java:45` / `LiquidVoid.java:23` / `LiquidJunction.java:29`.
        for name in ["liquid-source", "liquid-void", "liquid-junction"] {
            assert!(
                !bar_rows(name, &extras, &content)
                    .iter()
                    .any(|row| row.key == "liquid"),
                "{name} removes the liquid bar"
            );
        }
        // `Drill.java:117-119`: `bar.drillspeed` = speed * 60 * timeScale.
        let drill = bar_rows("mechanical-drill", &extras, &content);
        let Some(speed) = drill.iter().find(|row| row.key == "drillspeed") else {
            panic!("drill speed bar missing");
        };
        assert_eq!(speed.label_key, "bar.drillspeed");
        assert_eq!(speed.label_args.first().map(String::as_str), Some("750.00"));
        assert_eq!(speed.value, 0.5, "warmup");
        assert_eq!(speed.color, 0xff89_47ff);
        // `HeatProducer.java:40` / `HeatConductor.java:34`: `bar.heat`, heat/output.
        let heat_block = bar_rows("heat-source", &extras, &content);
        assert!(
            heat_block.iter().any(|row| row.key == "heat"),
            "heat-source has a heat bar"
        );
        // `UnitFactory.java:121-125`: `bar.progress` = `fraction()`.
        let factory = bar_rows("ground-factory", &extras, &content);
        let Some(progress) = factory.iter().find(|row| row.key == "progress") else {
            panic!("factory progress bar missing");
        };
        assert_eq!(progress.value, 0.5, "450 / dagger's 900 ticks");
        assert_eq!(progress.label_key, "bar.progress");
        // A plain block keeps exactly the default rows (`Block.java:709-760`):
        // health only (the helper's synthetic power status is not fed here).
        let Some(router_def) = content.block_by_name("router") else {
            panic!("router");
        };
        let router = building_bar_rows(router_def, 1.0, None, None, None, &[], &extras);
        assert_eq!(
            router.iter().map(|row| row.key).collect::<Vec<_>>(),
            ["health"]
        );
        let duo = bar_rows("duo", &extras, &content);
        assert_eq!(
            duo.iter().map(|row| row.key).collect::<Vec<_>>(),
            ["health", "power", "liquid"]
        );
    }

    /// L05 T1: the remaining `Block.setBars` families (turrets, power,
    /// generators/reactors, projectors, pumps, assemblers, pads, accelerator).
    #[test]
    fn bar_rows_for_the_remaining_families() {
        use mind_core::content::BlockKind;
        let content = base_registry();
        let def = |name: &str| {
            content
                .block_by_name(name)
                .unwrap_or_else(|| panic!("{name}"))
        };
        let rows_for = |name: &str, extras: &BarExtras| {
            building_bar_rows(def(name), 1.0, None, Some(0), None, &[], extras)
        };
        let keys = |rows: &[BarRow]| rows.iter().map(|row| row.key).collect::<Vec<_>>();

        // `ItemTurret.java:61-67`, `Turret.java:201-207`,
        // `BaseTurret.java:104-110`.
        let turret = BarExtras {
            turret: Some(TurretBars {
                total_ammo: 15,
                max_ammo: 30,
                heat_req: 2.0,
                heat_requirement: 4.0,
                max_heat_efficiency: 1.0,
                activation_timer: 0.0,
                activation_time: 0.0,
            }),
            ..BarExtras::default()
        };
        let duo = rows_for("duo", &turret);
        let Some(ammo) = duo.iter().find(|row| row.key == "ammo") else {
            panic!("ammo bar");
        };
        assert_eq!(ammo.label_key, "stat.ammo");
        assert_eq!(ammo.value, 0.5);
        let Some(heat) = duo.iter().find(|row| row.key == "heat") else {
            panic!("heat bar");
        };
        assert_eq!(heat.label_key, "bar.heatpercent");
        assert_eq!(heat.label_args, ["2", "50"]);

        // `PowerNode.java:115-122`: power, batteries, connections.
        let power = BarExtras {
            power: Some(PowerBars {
                balance: 10.0,
                produced: 30.0,
                needed: 60.0,
                stored: 500.0,
                capacity: 1000.0,
                links: 2,
                max_nodes: 3,
                back: None,
                front: None,
            }),
            ..BarExtras::default()
        };
        let node = rows_for("power-node", &power);
        assert_eq!(keys(&node), ["health", "power", "batteries", "connections"]);
        assert_eq!(node[1].label_args, ["+600"]);
        assert_eq!(node[1].value, 0.5);
        assert_eq!(
            node[2].label_args,
            [
                mind_core::ui::text::format_amount_default(500)
                    .replace("[gray]", "")
                    .replace("[]", ""),
                mind_core::ui::text::format_amount_default(1000)
                    .replace("[gray]", "")
                    .replace("[]", "")
            ]
        );
        assert_eq!(node[3].label_args, ["2", "3"]);

        // `PowerDiode.java:33-48`: `bar.input`/`bar.output`, no balance rows.
        let diode = rows_for(
            "diode",
            &BarExtras {
                power: Some(PowerBars {
                    back: Some(0.25),
                    front: Some(0.75),
                    ..PowerBars::default()
                }),
                ..BarExtras::default()
            },
        );
        assert_eq!(keys(&diode), ["health", "back", "front"]);
        assert_eq!(diode[1].label_key, "bar.input");
        assert_eq!(diode[2].label_key, "bar.output");
        assert_eq!((diode[1].value, diode[2].value), (0.25, 0.75));

        // `PowerGenerator.java:88-96`.
        let generator = rows_for(
            "combustion-generator",
            &BarExtras {
                generator: Some(GeneratorBars {
                    production: 1.0,
                    usage: 0.0,
                    efficiency: 0.5,
                }),
                time_scale: 1.0,
                ..BarExtras::default()
            },
        );
        let Some(output) = generator.iter().find(|row| row.key == "power") else {
            panic!("power output bar");
        };
        assert_eq!(output.label_key, "bar.poweroutput");
        assert_eq!(output.label_args, ["60.0"]);
        assert_eq!(output.value, 0.5);

        // `NuclearReactor.java:103`, `VariableReactor.java:57-63`.
        let reactor = rows_for(
            "thorium-reactor",
            &BarExtras {
                reactor_heat: Some(0.25),
                ..BarExtras::default()
            },
        );
        assert_eq!(
            reactor
                .iter()
                .find(|row| row.key == "heat")
                .map(|row| row.value),
            Some(0.25)
        );
        let flux = rows_for(
            "flux-reactor",
            &BarExtras {
                variable_reactor: Some((30.0, 150.0, 0.4)),
                ..BarExtras::default()
            },
        );
        let Some(instability) = flux.iter().find(|row| row.key == "instability") else {
            panic!("instability bar");
        };
        assert_eq!(instability.value, 0.4);
        assert_eq!(instability.color, 0x665c_9fff);

        // `OverdriveProjector.java:80`, `ForceProjector.java:99`.
        let overdrive = rows_for(
            "overdrive-projector",
            &BarExtras {
                overdrive: Some((2.25, 2.25)),
                ..BarExtras::default()
            },
        );
        let Some(boost) = overdrive.iter().find(|row| row.key == "boost") else {
            panic!("boost bar");
        };
        assert_eq!(boost.label_args, ["125"]);
        assert_eq!(boost.value, 1.0);
        let shield = rows_for(
            "force-projector",
            &BarExtras {
                shield: Some(0.5),
                ..BarExtras::default()
            },
        );
        assert_eq!(
            shield
                .iter()
                .find(|row| row.key == "shield")
                .map(|row| row.value),
            Some(0.5)
        );

        // `SolidPump.java:51-55`.
        let pump = rows_for(
            "water-extractor",
            &BarExtras {
                pump: Some((0.25, 0.5, 0.5)),
                ..BarExtras::default()
            },
        );
        let Some(efficiency) = pump.iter().find(|row| row.key == "efficiency") else {
            panic!("pump speed bar");
        };
        assert_eq!(efficiency.label_args, ["15.0"]);
        assert_eq!(efficiency.value, 0.25);

        // `UnitAssembler.java:128-143` `units` bar is the 3-argument form.
        let assembler = rows_for(
            "tank-assembler",
            &BarExtras {
                assembler: Some((
                    0.5,
                    100.0,
                    Some(UnitCapBars {
                        name: String::from("vanquish"),
                        count: 3,
                        cap: Some(8),
                        use_cap: true,
                        has_unit: true,
                    }),
                )),
                ..BarExtras::default()
            },
        );
        let Some(units) = assembler.iter().find(|row| row.key == "units") else {
            panic!("assembler units bar");
        };
        assert_eq!(units.label_key, "bar.unitcap");
        assert_eq!(units.label_args, [":vanquish:", "3", "8"]);
        assert_eq!(units.value, 3.0 / 8.0);
        assert_eq!(units.color, 0xec7b_4cff);

        // `LaunchPad.java:69-70`, `LandingPad.java:113-115`.
        let launch = rows_for(
            "launch-pad",
            &BarExtras {
                launch: Some((50.0, 100.0)),
                ..BarExtras::default()
            },
        );
        assert_eq!(keys(&launch), ["health", "items", "progress"]);
        assert_eq!(launch[2].label_key, "bar.launchcooldown");
        assert_eq!(launch[2].value, 0.5);
        let landing = rows_for(
            "landing-pad",
            &BarExtras {
                landing_cooldown: Some(0.25),
                ..BarExtras::default()
            },
        );
        assert_eq!(
            landing
                .iter()
                .find(|row| row.key == "cooldown")
                .map(|row| row.value),
            Some(0.25)
        );

        // `Accelerator.java:106-110`.
        let accelerator = rows_for(
            "interplanetary-accelerator",
            &BarExtras {
                accelerator: Some((250_000.0, 1_000_000.0)),
                ..BarExtras::default()
            },
        );
        let Some(buffer) = accelerator
            .iter()
            .find(|row| row.key == "powerBufferRequirement")
        else {
            panic!("power buffer bar");
        };
        assert_eq!(buffer.label_key, "bar.powerbuffer");
        assert_eq!(
            buffer.label_args,
            [
                mind_core::ui::text::format_amount_default(250_000)
                    .replace("[gray]", "")
                    .replace("[]", ""),
                mind_core::ui::text::format_amount_default(1_000_000)
                    .replace("[gray]", "")
                    .replace("[]", "")
            ]
        );
        assert_eq!(buffer.value, 0.25);

        // `TargetDummy.java:64-73`: health removed, `dps` over the reset timer.
        let dummy = rows_for(
            "target-dummy",
            &BarExtras {
                target_dummy: Some((30.0, 120.0)),
                ..BarExtras::default()
            },
        );
        assert_eq!(keys(&dummy), ["dps"]);
        assert_eq!(dummy[0].value, 0.75);

        // `GenericCrafter.java:81-98`: output liquids replace the dynamic bar.
        let electrolyzer = def("electrolyzer");
        assert_eq!(electrolyzer.kind, BlockKind::GenericCrafter);
        let crafter = building_bar_rows(
            electrolyzer,
            1.0,
            None,
            Some(0),
            Some(&[]),
            &[],
            &BarExtras {
                output_liquids: vec![
                    (String::from("ozone"), 0xaabb_ccff, 10.0, 20.0),
                    (String::from("hydrogen"), 0xddee_ffff, 5.0, 20.0),
                ],
                ..BarExtras::default()
            },
        );
        let liquids: Vec<_> = crafter.iter().filter(|row| row.key == "liquid").collect();
        assert_eq!(liquids.len(), 2);
        assert_eq!(liquids[0].label_key, "liquid.ozone.name");
        assert_eq!((liquids[0].value, liquids[1].value), (0.5, 0.25));
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
        let rows = building_bar_rows(def, 1.0, None, Some(5), None, &[], &BarExtras::default());
        assert_eq!(rows.len(), 2, "rows: {:?}", rows.len());
        assert_eq!(rows[0].key, "health");
        assert_eq!(rows[1].key, "items");
        assert_eq!(rows[1].label_key, "bar.items");
        assert_eq!(rows[1].label_args.first().map(String::as_str), Some("5"));
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
        let rows = building_bar_rows(
            def,
            1.0,
            None,
            None,
            Some(&liquids),
            &liquid_info,
            &BarExtras::default(),
        );
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

    /// H03 T3: types sort by id, command rows are the first-seen union without
    /// duplicates, and each type's default command is reported active.
    #[test]
    fn command_panel_orders_types_and_unions_commands() {
        use mind_core::ai::default_command;
        let content = base_registry();
        let (Some(dagger), Some(nova)) =
            (content.unit_by_name("dagger"), content.unit_by_name("nova"))
        else {
            panic!("dagger/nova missing from content");
        };
        let (first, second) = if dagger.id < nova.id {
            (dagger, nova)
        } else {
            (nova, dagger)
        };
        let units = vec![
            (second.id.get() as i32, None, StanceBits::new()),
            (first.id.get() as i32, None, StanceBits::new()),
            (first.id.get() as i32, None, StanceBits::new()),
        ];
        let panel = command_panel(&content, &units);
        let Some(types) = panel["types"].as_array() else {
            panic!("types not an array: {panel}");
        };
        assert_eq!(types.len(), 2);
        assert_eq!(types[0]["name"], serde_json::json!(first.name));
        assert_eq!(types[0]["count"], serde_json::json!(2));
        assert_eq!(types[1]["name"], serde_json::json!(second.name));
        assert_eq!(types[1]["count"], serde_json::json!(1));
        // Expected union: the lower id's list, then the other type's extras.
        let mut expected: Vec<String> = Vec::new();
        for command in first.commands.iter().chain(second.commands.iter()) {
            let Some(def) = content.unit_command(*command) else {
                panic!("command {command:?} missing");
            };
            let name = def.name.clone();
            if !expected.contains(&name) {
                expected.push(name);
            }
        }
        let Some(commands) = panel["commands"].as_array() else {
            panic!("commands not an array: {panel}");
        };
        if expected.len() > 1 {
            let names: Vec<String> = commands
                .iter()
                .map(|row| row["name"].as_str().unwrap_or_default().to_owned())
                .collect();
            assert_eq!(names, expected);
            for unit in [first, second] {
                let Some(default) = default_command(unit, &content) else {
                    panic!("{} has no default command", unit.name);
                };
                let default = default.name.clone();
                let row = commands
                    .iter()
                    .find(|row| row["name"] == serde_json::json!(default));
                assert!(
                    row.is_some_and(|row| row["active"] == serde_json::json!(true)),
                    "default command {default} not active"
                );
            }
        } else {
            assert!(commands.is_empty());
        }
    }

    /// H03 T9: the factory preview fraction is `progress / plan.time` on a
    /// live placed `ground-factory` (half progress -> 0.5), and the
    /// reconstructor preview resolves the upgrade target with
    /// `progress / constructTime`.
    #[test]
    fn unit_build_preview_fractions() {
        use mind_core::content::BlockId;
        use mind_core::determinism::SimCommand;
        use mind_core::world::TilePos;
        use mind_core::world::blocks::units::UnitFactoryBuild;

        fn some<T>(value: Option<T>, what: &str) -> T {
            match value {
                Some(value) => value,
                None => panic!("{what} missing"),
            }
        }

        let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
        let stone = BlockId::new(some(sim.world_block_id("stone"), "stone"));
        sim.grid.fill(stone, BlockId::AIR);
        if let Err(error) = sim.install_block_runtime() {
            panic!("runtime: {error}");
        }
        let factory = some(sim.world_block_id("ground-factory"), "ground-factory");
        if let Err(error) = sim.command(SimCommand::Place {
            x: 10,
            y: 10,
            block: factory,
            rotation: 0,
            team: 1,
            player: None,
        }) {
            panic!("place: {error}");
        }
        let entity = some(sim.grid.entity_at(TilePos::new(10, 10)), "entity");
        let def = some(
            sim.block_runtime().map(|runtime| runtime.content()),
            "runtime",
        );
        let def = some(def.block(BlockId::new(factory)), "def");
        let plans = def.unit_plans.clone();
        let plan_time = some(plans.first(), "plan").time;
        {
            let mut state = some(sim.ecs.0.get_mut::<UnitFactoryBuild>(entity), "state");
            state.current_plan = 0;
            state.progress = plan_time / 2.0;
        }
        let state = some(sim.ecs.0.get::<UnitFactoryBuild>(entity), "state");
        let (unit, fraction) = factory_preview(&plans, state.current_plan, state.progress);
        assert_eq!(unit, plans.first().map(|plan| plan.unit));
        assert!((fraction - 0.5).abs() < 1e-6, "fraction {fraction}");
        let Some(unit) = unit else {
            panic!("plan unit missing");
        };

        // Reconstructor: `(from -> to)` upgrades, payload arrived at half
        // progress.
        let nova = some(
            sim.block_runtime()
                .and_then(|runtime| runtime.content().unit_id("nova")),
            "nova",
        );
        let (target, fraction, arrived) =
            reconstructor_preview(&[(unit, nova)], Some(unit), 60.0, 120.0, 1.0);
        assert_eq!(target, Some(nova));
        assert!((fraction - 0.5).abs() < 1e-6, "fraction {fraction}");
        assert!(arrived);
    }
}
