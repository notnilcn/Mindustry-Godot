// SPDX-License-Identifier: GPL-3.0-only

//! The P0 simulation spine: fixed-step tick, command application, checksum and dump.
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`update`, `updateEntities`)
//! and `GameState`; the reserved slot order lives in [`crate::schedule`].

pub mod boot;
pub mod builder;
pub mod clock;
pub mod command;
pub mod config;
pub mod dump;
pub mod events;
pub mod fixed;
pub mod fog;
pub mod io_set;
pub mod logic;
pub mod player_items;
pub mod reset;
pub mod runtime;
pub mod schedule;
pub mod unit_runtime;
pub mod world_apply;

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::Schedule;
use serde_json::Error as JsonError;

use crate::command::Command;
use crate::content::{BlockId, Blocks, ContentError};
use crate::determinism::{Checksum, Checksummer, SimRng};
use crate::ecs::{BuildingComp, EntitySequencer, MindWorld, TeamId};
use crate::event::{BlockBrokenEvent, BlockPlacedEvent, Events, SimEvent, StateChangeEvent};
use crate::game::{GameState, State};
use crate::platform::{HeadlessPlatform, Platform};
use crate::random::JavaRandom;
use crate::scenario::{Scenario, ScenarioError, resolve_block};
use crate::time::Time;
use crate::world::{TilePos, WorldError, WorldGrid};

pub use boot::{BootError, SimBuilder, TickReport};

/// `HintsFragment` tick inputs (`HintsFragment.java:38-102`): the player's
/// placed block set and the fired one-shot event names.
///
/// `placed_by_player` mirrors `HintsFragment.placedBlocks` (`BlockBuildEndEvent`
/// with `!breaking` and `unit == player.unit`); `events` mirrors its
/// `events` set (`"break"`, `"cannotupgrade"`, `"fire"`, `"factorycontrol"`,
/// `"derelictbreak"`, `"derelictmouse"`).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HintEvents {
    /// Blocks the player has completed placing (`placedBlocks`).
    pub placed_by_player: std::collections::BTreeSet<BlockId>,
    /// One-shot hint events, by name.
    pub events: std::collections::BTreeSet<&'static str>,
}

impl HintEvents {
    /// `ResetEvent`: clears both sets.
    pub fn clear(&mut self) {
        self.placed_by_player.clear();
        self.events.clear();
    }
}
pub use clock::SimClock;
pub use command::{MOVE_COMMAND, UnitCommandRuntime, pack_wire_target, resolve_wire_target};
pub use config::SimConfig;
pub use dump::StateDump;
pub use events::{ALL_TRIGGERS, Trigger, TriggerRegistry};
pub use fixed::{FixedStepRunner, SIM_STEP};
pub use io_set::{DeferringIoHandler, IoHandler, IoQueue, IoRequest, IoResponse, IoSet, IoStatus};
pub use runtime::{RuntimeError, SimRuntime, SimRuntimeConfig};
pub use world_apply::WorldApplyRuntime;

/// Errors raised by simulation operations.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum SimError {
    /// A tile operation was out of bounds.
    #[error("{0}")]
    World(#[from] WorldError),
    /// A block id is not in the content registry.
    #[error("block id {0} is not registered")]
    UnknownBlock(u16),
    /// Placing `air` is a no-op, not a command.
    #[error("cannot place the air block")]
    CannotPlaceAir,
}

impl From<ContentError> for SimError {
    fn from(error: ContentError) -> Self {
        match error {
            ContentError::UnknownId(id) => SimError::UnknownBlock(id),
            _ => SimError::UnknownBlock(u16::MAX),
        }
    }
}

/// Minimal deterministic snapshot header (plan 21 `Sim::snapshot` hook).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimSnapshot {
    /// Completed ticks.
    pub tick: u64,
    /// Update counter.
    pub update_id: u64,
    /// Current phase.
    pub phase: State,
    /// Canonical checksum.
    pub checksum: u64,
}

/// Comparison-time checksum projection (plan 21 §3.2/§6.5).
///
/// The authority mask is a *projection*, never a change to plan 05's hasher;
/// masked fields are replaced by their declared defaults while the field walk
/// order is unchanged. Bits map directly to `match_checksum.scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumScope {
    /// Include the shared world cohort (always true under D2).
    pub world_cohort: bool,
    /// Include possessed-unit kinematics/health/input (false under
    /// `AuthorityMode::Relay`; true under the authoritative sim and headless
    /// replay).
    pub include_possessed: bool,
    /// Include view-only/FX state (always false).
    pub include_view: bool,
}

impl Default for ChecksumScope {
    fn default() -> Self {
        Self::relay()
    }
}

impl ChecksumScope {
    /// Masked scope used while clients own possessed units (D2 relay).
    pub const fn relay() -> Self {
        Self {
            world_cohort: true,
            include_possessed: false,
            include_view: false,
        }
    }

    /// Unmasked scope for the deferred authoritative sim / exact replay.
    pub const fn authoritative() -> Self {
        Self {
            world_cohort: true,
            include_possessed: true,
            include_view: false,
        }
    }

    /// `ChecksumScope` bitset sent in `match_checksum.scope`.
    pub const fn bits(&self) -> u8 {
        (self.world_cohort as u8)
            | ((self.include_possessed as u8) << 1)
            | ((self.include_view as u8) << 2)
    }
}

/// The P0 simulation container.
pub struct Sim {
    /// ECS world (placed blocks at P0).
    pub ecs: MindWorld,
    /// Reserved schedule slots ([`crate::schedule::SimSet`]).
    pub schedule: Schedule,
    /// Tile grid.
    pub grid: WorldGrid,
    /// Phase + tick counter.
    pub state: GameState,
    /// Event bus (listeners; dispatch is registration-ordered).
    pub events: Events,
    /// Deterministic RNG (unused by P0 systems; consumed by later plans).
    pub rng: JavaRandom,
    /// Content registry.
    pub content: Blocks,
    /// Arc-style clock/delayed-run queue (P0).
    pub time: Time,
    /// Deterministic fixed-step clock (plan 05 M1; owns `Time.run` semantics).
    pub clock: SimClock,
    /// Trigger registry (plan 05 M2).
    pub triggers: TriggerRegistry,
    /// Deterministic RNG streams (plan 05 M1).
    pub rng_streams: SimRng,
    /// Fixed-step/determinism configuration.
    pub config: SimConfig,
    /// Host capability seam (headless by default).
    pub platform: Box<dyn Platform>,
    /// Save/load boundary queue (plan 05 M9; plan 04 §3.10).
    pub io: IoQueue,

    /// World-processor UI events drained from `LogicWorldState` each tick
    /// (announce/toast/cutscene/spawnwave/setblock); the host pops them
    /// (G06 T4; T5/T6 extend the handling).
    pub logic_ui_events: Vec<crate::logic::world::LogicWorldEvent>,

    /// Last 64 drained logic-event kind tags (host debug probe; H06 T1).
    pub logic_event_log: std::collections::VecDeque<String>,

    /// Relay-side unit-command apply state (plan 15 M4). Built lazily on the
    /// first unit command so P0 worlds never pay for content boot.
    unit_commands: Option<command::UnitCommandRuntime>,
    /// Relay-side world-mutation apply state (plan 15 §3.3.1: inventory, build
    /// plans, building commands). Built lazily on the first such command.
    world_apply: Option<world_apply::WorldApplyRuntime>,
    /// Opt-in live building runtime (plan-07 `BlockTable` + behaviors). `None`
    /// keeps the P0 path byte-identical; see [`runtime`].
    block_runtime: Option<runtime::SimRuntime>,
    /// Currently possessed unit (`unitControl`); `None` means the player unit.
    controlled_unit: Option<i32>,
    /// The campaign player unit entity (`Player.unit()`); runtime-only.
    player_unit: Option<Entity>,
    /// Player team used by respawn (`Player.team`); `None` before the first spawn.
    player_team: Option<u8>,
    /// Ticks since the player unit died (`PlayerComp.deathTimer`).
    player_death_timer: f32,
    /// Last known player position (`Player.x/y`); `bestCore` distance key.
    player_last_pos: Option<(f32, f32)>,
    /// Last `(rules.fog, rules.staticFog)` applied to the world
    /// (`FogControl` reset gate; `None` before the first world load).
    last_fog_flags: Option<(bool, bool)>,
    /// Currently selected controllable building (`buildingControlSelect`).
    control_building: Option<TilePos>,
    /// `HintsFragment` tick inputs (placed blocks + fired events).
    pub hint_events: HintEvents,

    seed: u64,
    selected_block: BlockId,
    entity_seq: EntitySequencer,
    commands_applied: u64,
    pending_events: Vec<SimEvent>,
    event_log: Vec<dump::DumpEvent>,
}

impl Sim {
    /// Creates a flat world in the `Playing` phase.
    pub fn new(seed: u64, width: i32, height: i32, floor: BlockId, wall: BlockId) -> Self {
        let mut grid = WorldGrid::new(width, height);
        grid.fill(floor, wall);
        Self {
            ecs: MindWorld::new(),
            schedule: self::schedule::build_sim_schedule(),
            grid,
            state: GameState::new(State::Playing),
            events: Events::new(),
            rng: JavaRandom::new(seed),
            content: Blocks::new(),
            time: Time::new(),
            clock: SimClock::new(),
            triggers: TriggerRegistry::new(),
            rng_streams: SimRng::new(seed),
            config: SimConfig::new().with_seed(seed),
            platform: Box::new(HeadlessPlatform::with_default_dir()),
            io: IoQueue::new(),
            unit_commands: None,
            world_apply: None,
            block_runtime: None,
            controlled_unit: None,
            player_unit: None,
            player_team: None,
            player_death_timer: 0.0,
            player_last_pos: None,
            last_fog_flags: None,
            control_building: None,
            hint_events: HintEvents::default(),
            seed,
            // Upstream `player.block == null` at boot: no placement block is
            // selected until the player picks one (the command-mode gate
            // requires it; PAR-0140).
            selected_block: BlockId::AIR,
            entity_seq: EntitySequencer::default(),
            commands_applied: 0,
            logic_ui_events: Vec::new(),
            logic_event_log: std::collections::VecDeque::new(),
            pending_events: Vec::new(),
            event_log: Vec::new(),
        }
    }

    /// Builds a sim from a parsed scenario (`flat` generator only at P0).
    pub fn from_scenario(scenario: &Scenario) -> Result<Self, ScenarioError> {
        scenario.validate()?;
        if scenario.world.generator != "flat" {
            return Err(ScenarioError::UnknownGenerator(
                scenario.world.generator.clone(),
            ));
        }
        // `world.floor` has no content entry at P0 (no floor content until plan 06);
        // floors stay `air` and the name is retained by the scenario.
        let mut sim = Sim::new(
            scenario.seed,
            scenario.world.width,
            scenario.world.height,
            BlockId::AIR,
            BlockId::AIR,
        );
        let wall = resolve_block(&sim.content, &scenario.world.wall)?;
        sim.grid.fill(BlockId::AIR, wall);
        sim.state = GameState::new(State::Playing);
        Ok(sim)
    }

    /// Applies one command to the simulation. Commands are applied *before* the
    /// tick they are stamped with executes (`ScenarioPlayer` enforces the stamping).
    ///
    /// Invalid placements/breaks (occupied tile / empty tile) are deterministic
    /// no-ops, logged at debug level; out-of-bounds and unknown blocks are errors.
    pub fn apply(&mut self, command: Command) -> Result<(), SimError> {
        match command {
            Command::SelectBlock { block } => {
                self.content.name(block).map_err(SimError::from)?;
                self.selected_block = block;
            }
            Command::Place { x, y, block } => {
                self.content.name(block).map_err(SimError::from)?;
                if block == BlockId::AIR {
                    return Err(SimError::CannotPlaceAir);
                }
                let pos = TilePos::new(x, y);
                self.grid.index(pos)?;
                if self.block_runtime.is_some() {
                    // Java: a placement carries the player's team
                    // (`state.rules.defaultTeam`); prefer the live campaign
                    // session, then the build guard resource.
                    let team = self
                        .ecs
                        .0
                        .get_resource::<crate::game::runtime::CampaignRuntime>()
                        .map(|runtime| runtime.session.rules.default_team)
                        .or_else(|| {
                            self.ecs
                                .0
                                .get_resource::<crate::world::limits::BuildRules>()
                                .map(|rules| rules.default_team)
                        })
                        .unwrap_or(TeamId::SHARDED.0);
                    // `Control.input` dev placement uses the API form
                    // (`Control.java:505,521`).
                    if self.runtime_place(
                        x,
                        y,
                        block,
                        0,
                        team,
                        crate::world::build::PlaceFlags::API,
                    ) {
                        self.push_event(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                            x,
                            y,
                            block,
                            team,
                        }));
                    } else {
                        log::debug!(
                            "place command rejected at ({x}, {y}) by the live block runtime"
                        );
                    }
                } else {
                    let current = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                    if current == BlockId::AIR {
                        let seq = self.entity_seq.alloc();
                        let comp = BuildingComp {
                            pos,
                            block,
                            team: TeamId::SHARDED,
                            rot: 0,
                        };
                        let entity = self.ecs.spawn_building(seq, comp);
                        self.grid.set_block(pos, block, TeamId::SHARDED.0, 0)?;
                        self.grid.set_entity(pos, Some(entity))?;
                        self.push_event(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                            x,
                            y,
                            block,
                            team: TeamId::SHARDED.0,
                        }));
                    } else {
                        log::debug!(
                            "place command rejected at ({x}, {y}): tile already holds block {}",
                            current.raw()
                        );
                    }
                }
            }
            Command::Break { x, y } => {
                let pos = TilePos::new(x, y);
                self.grid.index(pos)?;
                let current = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                if current == BlockId::AIR {
                    log::debug!("break command rejected at ({x}, {y}): tile is empty");
                } else if self.block_runtime.is_some() {
                    let team = self
                        .grid
                        .entity_at(pos)
                        .and_then(|entity| {
                            self.ecs.0.get::<crate::entities::comp::TeamComp>(entity)
                        })
                        .map(|team| team.team)
                        .unwrap_or(0);
                    if self.runtime_break(x, y) {
                        self.push_event(SimEvent::BlockBrokenEvent(BlockBrokenEvent {
                            x,
                            y,
                            block: current,
                            // `Command::Break` is a deconstruct/API break.
                            destroyed: false,
                            team,
                        }));
                    }
                } else {
                    let team = self
                        .grid
                        .entity_at(pos)
                        .and_then(|entity| {
                            self.ecs.0.get::<crate::entities::comp::TeamComp>(entity)
                        })
                        .map(|team| team.team)
                        .unwrap_or(0);
                    if let Some(entity) = self.grid.entity_at(pos) {
                        self.ecs.despawn(entity);
                    }
                    self.grid.clear(pos)?;
                    self.push_event(SimEvent::BlockBrokenEvent(BlockBrokenEvent {
                        x,
                        y,
                        block: current,
                        // `Command::Break` is a deconstruct/API break.
                        destroyed: false,
                        team,
                    }));
                }
            }
        }
        self.commands_applied = self.commands_applied.wrapping_add(1);
        Ok(())
    }

    /// Places through the opt-in live runtime; `false` when absent or rejected.
    /// `state.rules.defaultTeam`: the live campaign session, then the build
    /// guard resource, then the sharded default.
    pub fn live_default_team(&self) -> u8 {
        self.ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .map(|runtime| runtime.session.rules.default_team)
            .or_else(|| {
                self.ecs
                    .0
                    .get_resource::<crate::world::limits::BuildRules>()
                    .map(|rules| rules.default_team)
            })
            .unwrap_or(TeamId::SHARDED.0)
    }

    /// `Build.validPlace` for an explicit team against the live block runtime
    /// (`SimRuntime::can_place`).
    ///
    /// `false` when the runtime is absent or the placement is invalid. The API
    /// placement seams (`place_block`, `apply_sim_command_json`) use it to
    /// report rejections honestly instead of the old `Ok`-means-placed
    /// behavior (T7, PAR-0165).
    pub fn can_place_for_team(&self, x: i16, y: i16, block: BlockId, rot: u8, team: u8) -> bool {
        self.block_runtime.as_ref().is_some_and(|runtime| {
            runtime.can_place(
                &self.ecs.0,
                &self.grid,
                x,
                y,
                block,
                rot,
                team,
                crate::world::build::PlaceFlags::API,
            )
        })
    }

    /// `Build.validPlace(type, player.team(), x, y, rot)` for the placement
    /// ghost (`InputHandler.java:1455`); H08 consumes it.
    pub fn valid_place_player(&self, x: i16, y: i16, block: BlockId, rot: u8) -> bool {
        let team = self.live_default_team();
        self.block_runtime.as_ref().is_some_and(|runtime| {
            runtime.can_place(
                &self.ecs.0,
                &self.grid,
                x,
                y,
                block,
                rot,
                team,
                crate::world::build::PlaceFlags::PLAYER,
            )
        })
    }

    /// T2: applies a `PowerNode` link config (`PowerNode.java:419-441`,
    /// `config(Integer.class)` = toggle link, `config(Point2[].class)` = replace
    /// all links) through the `PowerGrids` arena.
    ///
    /// Returns `false` when `entity` is not a power node or the value is not a
    /// link config, so the caller falls through to the normal behavior config
    /// path.
    fn configure_power_node(
        &mut self,
        entity: Entity,
        value: &crate::world::config::ConfigValue,
    ) -> bool {
        use crate::world::blocks::power::nodes;
        use crate::world::config::ConfigValue as V;
        if self
            .ecs
            .0
            .get::<crate::world::blocks::power::PowerNodeConfig>(entity)
            .is_none()
        {
            return false;
        }
        match value {
            V::Number(number) => {
                let packed = *number as i32;
                self.with_power_graphs(|graphs, world, grid| {
                    nodes::configure_link(graphs, world, grid, entity, packed);
                });
                true
            }
            V::Point2(dx, dy) => {
                let Some(building) = self.ecs.0.get::<crate::entities::comp::Building>(entity)
                else {
                    return false;
                };
                let packed = TilePos::new(
                    building.tile.x() + *dx as i16,
                    building.tile.y() + *dy as i16,
                )
                .pack();
                self.with_power_graphs(|graphs, world, grid| {
                    nodes::configure_link(graphs, world, grid, entity, packed);
                });
                true
            }
            V::Point2Array(points) => {
                let packed: Vec<i32> = points.iter().copied().collect();
                self.with_power_graphs(|graphs, world, grid| {
                    nodes::configure_links(graphs, world, grid, entity, &packed);
                });
                true
            }
            _ => false,
        }
    }

    /// Takes `PowerGrids` out of the ECS for `f` and always re-inserts it (the
    /// `SimRuntime::merge_power_graph` pattern).
    fn with_power_graphs(
        &mut self,
        f: impl FnOnce(
            &mut crate::world::blocks::power::PowerGrids,
            &mut bevy_ecs::world::World,
            &WorldGrid,
        ),
    ) {
        let Some(mut graphs) = self
            .ecs
            .0
            .remove_resource::<crate::world::blocks::power::PowerGrids>()
        else {
            return;
        };
        f(&mut graphs, &mut self.ecs.0, &self.grid);
        self.ecs.0.insert_resource(graphs);
    }

    /// Applies a wire `UnitCommand` to **live** units (`id` = `EntitySeq`).
    ///
    /// Returns the number of units ordered; `0` routes the command to the
    /// relay runtime (the net/test id space). `command == 0` is the emitter's
    /// `commandUnits` marker: the wire target resolves through
    /// [`resolve_wire_target`] (a packed unit id in `x`, else a world-pixel
    /// position) and lands on the member's move target
    /// (`ControllerSlot.target`). Only default-team units whose controller is
    /// `CommandAI` accept (`isCommandable`) and only commands the unit type
    /// allows (`CommandAI.command`, CommandAI.java:57-64).
    ///
    /// Every registered command id is accepted and stored on the unit
    /// ([`crate::ai::controller::UnitCommandComp`]); non-move commands clear
    /// the move target and wait for their controller dispatch
    /// (`CommandAI.updateUnit`'s `commandController`, ported with the AI kinds).
    fn apply_live_unit_command(&mut self, ids: &[i32], command: u16, x: f32, y: f32) -> usize {
        let command_id = crate::content::UnitCommandId::new(command);
        let Some(runtime) = self
            .ecs
            .0
            .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        else {
            return 0;
        };
        if runtime.units.is_empty() {
            return 0;
        }
        // `CommandAI.command` ignores ids that are not registered.
        if runtime.content.unit_command(command_id).is_none() {
            return 0;
        }
        let is_move = command == MOVE_COMMAND.raw();
        // `setUnitCommand` carries the `(0, 0)` wire pair (`command_emit.rs`):
        // a command switch must not create a move target. A real move to world
        // (0, 0) is indistinguishable on this wire — accepted deviation.
        let set_only = is_move && x == 0.0 && y == 0.0;
        let default_team = self.live_default_team();
        let target = resolve_wire_target(x, y);
        // `CommandAI.commandTarget` (G01 T16): Unit/Building wire targets keep
        // the entity so the order follows moving targets.
        let attack_entity = match &target {
            crate::input::action::CommandTarget::Unit(target_id) => runtime
                .units
                .iter()
                .find(|&&entity| {
                    self.ecs
                        .0
                        .get::<crate::ecs::EntitySeq>(entity)
                        .is_some_and(|seq| seq.0 as i32 == *target_id)
                })
                .copied(),
            crate::input::action::CommandTarget::Building(pos) => self
                .grid
                .tiles
                .getn(pos.x() as i32, pos.y() as i32)
                .and_then(|tile| tile.build),
            crate::input::action::CommandTarget::Position { .. } => None,
        };
        let tile = match target {
            // `World.toTile` (rounded): tile `t` spans `t*8 ± 4` world px.
            crate::input::action::CommandTarget::Position { x, y } => Some(TilePos::new(
                crate::world::WorldGrid::to_tile(x) as i16,
                crate::world::WorldGrid::to_tile(y) as i16,
            )),
            crate::input::action::CommandTarget::Unit(target_id) => runtime
                .units
                .iter()
                .find(|&&entity| {
                    self.ecs
                        .0
                        .get::<crate::ecs::EntitySeq>(entity)
                        .is_some_and(|seq| seq.0 as i32 == target_id)
                })
                .and_then(|&entity| self.ecs.0.get::<crate::entities::comp::Pos>(entity))
                .map(|pos| {
                    TilePos::new(
                        crate::world::WorldGrid::to_tile(pos.x) as i16,
                        crate::world::WorldGrid::to_tile(pos.y) as i16,
                    )
                }),
            crate::input::action::CommandTarget::Building(pos) => Some(pos),
        };
        let mut orders: Vec<(Entity, Option<TilePos>, Option<Entity>)> = Vec::new();
        for &id in ids {
            let Some(&entity) = runtime.units.iter().find(|&&entity| {
                self.ecs
                    .0
                    .get::<crate::ecs::EntitySeq>(entity)
                    .is_some_and(|seq| seq.0 as i32 == id)
            }) else {
                continue;
            };
            let team = self
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|team| team.team)
                .unwrap_or(0);
            // `unit.isCommandable()` (`controller instanceof CommandAI`): the
            // player's own unit and AI-controlled units ignore orders.
            let commandable = self
                .ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(entity)
                .is_some_and(|slot| slot.kind == crate::ai::controller::AiKind::Command);
            if team != default_team || !commandable {
                continue;
            }
            // `CommandAI.command`: only unit-typed commands are accepted.
            let allowed = self
                .ecs
                .0
                .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .is_some_and(|def| {
                    crate::ai::unit_command_runtime::allows_command(def, command_id)
                });
            if !allowed {
                continue;
            }
            // Move orders need a resolvable target; other commands only switch
            // the active command. A `(0, 0)` wire pair is a pure command
            // switch (`setUnitCommand`), not a move to world (0, 0).
            let order = if is_move && !set_only {
                let Some(tile) = tile else {
                    continue;
                };
                Some(tile)
            } else {
                None
            };
            orders.push((entity, order, attack_entity));
        }
        let mut ordered = 0usize;
        for (entity, order, attack_entity) in orders {
            // `CommandAI.command`: `this.command = command` (CommandAI.java:62);
            // the move target is cleared when switching away from move, and
            // `unit.mineTile`/build plans are dropped (CommandAI.java:59-63).
            self.ecs
                .0
                .entity_mut(entity)
                .insert(crate::ai::controller::UnitCommandComp(command));
            crate::entities::comp::unit::miner::clear_mine_tile(&mut self.ecs.0, entity);
            self.ecs
                .0
                .entity_mut(entity)
                .insert(crate::ai::types::command::AttackTargetComp(attack_entity));
            if let Some(mut slot) = self
                .ecs
                .0
                .get_mut::<crate::ai::controller::ControllerSlot>(entity)
            {
                slot.command = Some(command_id);
                slot.target = order;
                ordered += 1;
            }
        }
        ordered
    }

    /// Applies a wire `UnitStance` to **live** units (`id` = `EntitySeq`),
    /// mirroring [`Self::apply_live_unit_command`]'s filters: default team,
    /// `CommandAI` controller and a stance the unit type lists
    /// (`UnitType.getUnitStances`). Returns the number of units whose live
    /// stance mask was touched (`0` routes the command to the relay).
    fn apply_live_unit_stance(&mut self, ids: &[i32], stance: u16, enabled: bool) -> usize {
        let stance_id = crate::content::id::UnitStanceId::new(stance);
        let Some(runtime) = self
            .ecs
            .0
            .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        else {
            return 0;
        };
        if runtime.units.is_empty() || runtime.content.unit_stance(stance_id).is_none() {
            return 0;
        }
        let default_team = self.live_default_team();
        let mut accepted: Vec<Entity> = Vec::new();
        for &id in ids {
            let Some(&entity) = runtime.units.iter().find(|&&entity| {
                self.ecs
                    .0
                    .get::<crate::ecs::EntitySeq>(entity)
                    .is_some_and(|seq| seq.0 as i32 == id)
            }) else {
                continue;
            };
            let team = self
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|team| team.team)
                .unwrap_or(0);
            let commandable = self
                .ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(entity)
                .is_some_and(|slot| slot.kind == crate::ai::controller::AiKind::Command);
            if team != default_team || !commandable {
                continue;
            }
            let allowed = self
                .ecs
                .0
                .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
                .and_then(|comp| runtime.content.unit(comp.type_id))
                .is_some_and(|def| {
                    let mut stances = Vec::new();
                    crate::ai::unit_command_runtime::get_unit_stances(
                        def,
                        &runtime.content,
                        &mut stances,
                    );
                    stances.contains(&stance_id)
                });
            if allowed {
                accepted.push(entity);
            }
        }
        let stance_def = runtime.content.unit_stance(stance_id).cloned();
        let Some(stance_def) = stance_def else {
            return 0;
        };
        let mut applied = 0usize;
        for entity in accepted {
            let mut comp = self
                .ecs
                .0
                .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(entity)
                .copied()
                .unwrap_or_default();
            if enabled {
                crate::ai::unit_stance_runtime::apply_stance(&mut comp.0, &stance_def);
            } else {
                crate::ai::unit_stance_runtime::disable_stance(&mut comp.0, stance_id);
            }
            self.ecs.0.entity_mut(entity).insert(comp);
            applied += 1;
        }
        applied
    }

    /// Live command targets for the HUD reticle (G01 T16):
    /// `(seq, world_x, world_y, is_attack)`. Move orders report their tile
    /// centre; attack orders report the tracked entity's position.
    pub fn command_targets(&self) -> Vec<(i32, f32, f32, bool)> {
        let Some(runtime) = self
            .ecs
            .0
            .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &entity in &runtime.units {
            let Some(seq) = self.ecs.0.get::<crate::ecs::EntitySeq>(entity) else {
                continue;
            };
            let attack = self
                .ecs
                .0
                .get::<crate::ai::types::command::AttackTargetComp>(entity)
                .and_then(|comp| comp.0);
            let move_tile = self
                .ecs
                .0
                .get::<crate::ai::controller::ControllerSlot>(entity)
                .and_then(|slot| slot.target);
            if attack.is_none() && move_tile.is_none() {
                continue;
            }
            let pos = attack
                .and_then(|target| self.ecs.0.get::<crate::entities::comp::Pos>(target))
                .map(|pos| (pos.x, pos.y))
                .or_else(|| {
                    move_tile.map(|tile| {
                        (
                            // `Tile.worldx()/worldy()` (Tile.java:166-171).
                            tile.x() as f32 * crate::config::TILESIZE as f32,
                            tile.y() as f32 * crate::config::TILESIZE as f32,
                        )
                    })
                })
                .unwrap_or((0.0, 0.0));
            out.push((seq.0 as i32, pos.0, pos.1, attack.is_some()));
        }
        out
    }

    /// Drains the per-tick `BuildDamageEvent` log (G01 T15): one entry per
    /// building hit by a bullet (direct or bullet-sourced splash).
    pub fn drain_build_damage_events(&mut self) -> Vec<crate::world::events::BuildDamageEvent> {
        self.ecs
            .0
            .get_resource_mut::<crate::world::events::BuildDamageLog>()
            .map(|mut log| std::mem::take(&mut log.0))
            .unwrap_or_default()
    }

    fn runtime_place(
        &mut self,
        x: i16,
        y: i16,
        block: BlockId,
        rot: u8,
        team: u8,
        flags: crate::world::build::PlaceFlags,
    ) -> bool {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return false;
        };
        let placed = runtime.place(
            &mut self.ecs.0,
            &mut self.grid,
            x,
            y,
            block,
            rot,
            team,
            flags,
        );
        if placed {
            self.refresh_unit_runtime();
        }
        placed
    }

    /// The live per-team block counter resource (`Build.validPlace` limits).
    pub fn block_counter(&self) -> Option<&crate::world::limits::BlockCounter> {
        self.ecs
            .0
            .get_resource::<crate::world::limits::BlockCounter>()
    }

    /// Applies a live block placement for an explicit team.
    ///
    /// `Command::Place` carries only the default team; the campaign AI queue
    /// drain (`CampaignAiQueues` -> `Sim::place_for_team`) uses this entry
    /// point so `BaseBuilderAI` plans land for their owning team (PAR-0139).
    pub fn place_for_team(&mut self, x: i16, y: i16, block: BlockId, rot: u8, team: u8) -> bool {
        if self.block_runtime.is_none() || self.content.name(block).is_err() {
            return false;
        }
        // AI/base placements use the 5-arg player form (`BaseBuilderAI.java:211`).
        if self.runtime_place(
            x,
            y,
            block,
            rot,
            team,
            crate::world::build::PlaceFlags::PLAYER,
        ) {
            self.push_event(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                x,
                y,
                block,
                team,
            }));
            true
        } else {
            log::debug!("team placement rejected at ({x}, {y}) for team {team}");
            false
        }
    }

    /// `Schematics.placeLoadout` for the live runtime (launcher loadouts,
    /// `game/Schematics.java:465-500`): places `schem` centered on its core
    /// tile at `(x, y)` for `team`, replacing whatever occupies the footprints
    /// (`check` clears non-core blockers first). Returns the placed core
    /// entities; empty when the runtime is not installed or the schematic has
    /// no core.
    pub fn place_loadout(
        &mut self,
        schem: &crate::game::schematic::Schematic,
        x: i16,
        y: i16,
        team: u8,
        check: bool,
    ) -> Vec<Entity> {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return Vec::new();
        };
        let cores =
            runtime.place_schematic(&mut self.ecs.0, &mut self.grid, schem, x, y, team, check);
        if !cores.is_empty() {
            self.refresh_unit_runtime();
        }
        cores
    }

    /// Runs `f` with the live [`crate::world::ops::WorldCtx`] (generator
    /// structure passes; returns `None` without a block runtime) and refreshes
    /// the unit runtime afterwards (H05 T5/T6).
    pub fn with_world_ctx<R>(
        &mut self,
        f: impl FnOnce(&mut crate::world::ops::WorldCtx<'_>) -> R,
    ) -> Option<R> {
        let outcome = {
            let grid = &mut self.grid;
            let world = &mut self.ecs.0;
            let runtime = self.block_runtime.as_mut()?;
            runtime.with_world_ctx(world, grid, f)
        };
        self.refresh_unit_runtime();
        Some(outcome)
    }

    /// `Schematics.placeLaunchLoadout`'s core stocking
    /// (`Schematics.java:465-469`): `tile.build.items.add(universe.
    /// getLaunchResources())` on the just-placed core, capped at its
    /// `storageCapacity` (H05 T4).
    pub fn stock_core(sim: &mut Sim, entity: Entity, stacks: &[(crate::content::ItemId, i32)]) {
        let capacity = sim
            .ecs
            .0
            .get::<crate::world::blocks::storage::CoreBuild>(entity)
            .map(|core| core.storage_capacity)
            .unwrap_or(i32::MAX);
        if let Some(mut module) = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(entity)
        {
            module.clear();
            for (item, amount) in stacks {
                module.add(*item, *amount, capacity);
            }
        }
    }

    /// Breaks through the opt-in live runtime; `false` when absent or rejected.
    fn runtime_break(&mut self, x: i16, y: i16) -> bool {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return false;
        };
        let broken = runtime
            .break_block(&mut self.ecs.0, &mut self.grid, x, y)
            .is_some();
        if broken {
            self.refresh_unit_runtime();
        }
        broken
    }

    /// Advances the simulation by one fixed step and flushes queued events.
    ///
    /// Schedule order follows `Logic.update()` (plan 05 §3.4): `Trigger.update`
    /// → set chain → delayed runs (`Time.update`) → `Trigger.afterGameUpdate`.
    pub fn tick(&mut self) -> Result<(), SimError> {
        // `IoSet::Apply` is the pre-tick boundary (plan 04 §3.10): no load may
        // mutate the world mid-tick.
        self.pump_io(IoSet::Apply);
        self.triggers.fire(Trigger::Update, &mut self.ecs.0);
        // `Logic.update` -> `logicVars.update()` (Logic.java:546) runs before
        // the entity update pass so processors observe the incremented tick.
        self.update_logic_globals();
        // `Logic.java:533-535`: `fogControl.update()` when `rules.fog`, after
        // `updateTeamStats()` and before the entity pass.
        self.update_fog();
        self.schedule.run(&mut self.ecs.0);
        // `BeamDrill.updateFacing`/`WallCrafter.getEfficiency` (T5): the facing
        // rescan needs the grid, so it runs outside the scheduled behaviors.
        self.refresh_facing_drills();
        // `Groups.fires`/`Groups.puddles.update()` (`Logic.updateEntities`):
        // fires burn/spread and puddles evaporate/apply statuses every tick
        // (PAR-0091). Damage they deal is picked up by `resolve_deaths` below.
        self.update_fires_and_puddles();
        // `BuilderComp.update` (PAR-0098): player build plans drive the
        // construct blocks after the entity update pass.
        self.update_builders();
        // `Trigger.fireCreate` (G05 T4b): any fire born this tick (bullets,
        // puddles, explosions) arms the `waveFire` hint.
        if self
            .ecs
            .0
            .remove_resource::<crate::combat::fires::FireCreated>()
            .is_some()
        {
            self.hint_events.events.insert("fire");
        }
        // World-processor UI events (announce/toast/cutscene) are drained here so
        // the log never grows unbounded (T4; T5/T6 extend the match).
        self.drain_logic_events();
        // Zero-health units/buildings die (`HealthComp.damage` -> `kill`).
        let _ = self.resolve_deaths();
        // `PlayerComp.update`: respawn at the best core after `deathDelay`.
        self.update_player_respawn();
        // Capture-derived world requests (T7) apply after the death pass and
        // before the events flush, mirroring Java's next-frame `Core.app.post`.
        self.apply_campaign_world_requests();
        // A mid-match rules change (G06 `setrule`, relay `setRules`, invasion)
        // re-derives the world from the session rules (T5).
        self.apply_dirty_rules_to_world();
        self.flush_events();
        self.time.update();
        self.clock.update(&mut self.ecs.0);
        self.state.advance();
        self.triggers
            .fire(Trigger::AfterGameUpdate, &mut self.ecs.0);
        // `IoSet::Capture` is the post-update boundary.
        self.pump_io(IoSet::Capture);
        Ok(())
    }

    /// `Logic.update` -> `logicVars.update()` (Logic.java:546), before entity
    /// updates: processors observe the already-incremented tick.
    fn update_logic_globals(&mut self) {
        // Keep the standalone logic clock/map size current even without a
        // `GlobalVars` resource (message timers, `run_sync`).
        if let Some(mut state) = self
            .ecs
            .0
            .get_resource_mut::<crate::logic::world::LogicWorldState>()
        {
            state.tick = self.state.tick + 1;
            state.map_width = self.grid.width();
            state.map_height = self.grid.height();
        }
        if self
            .ecs
            .0
            .get_resource::<crate::logic::globals::GlobalVars>()
            .is_none()
        {
            return;
        }
        let (wave, wavetime) = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .map(|rt| (rt.session.wave, rt.session.wavetime))
            .unwrap_or((1, 0.0));
        // Same chain as `gdext/sim_host.rs` `player_team`.
        let client_team = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .map(|rt| rt.session.rules.default_team)
            .or_else(|| {
                self.ecs
                    .0
                    .get_resource::<crate::world::limits::BuildRules>()
                    .map(|rules| rules.default_team)
            })
            .unwrap_or(crate::ecs::TeamId::SHARDED.0);
        let update = crate::logic::globals::GlobalUpdate {
            // Java increments `state.tick` before `logicVars.update()`
            // (Logic.java:526-546), so processors see tick N during tick N.
            tick: self.state.tick + 1,
            wave,
            wavetime,
            map_width: self.grid.width(),
            map_height: self.grid.height(),
            server: true,
            client: false,
            // The port has no player-name/locale settings in core; Java reads
            // `player.locale()` / `player.name()` (GlobalVars.java:211-216).
            client_locale: Some("en".to_owned()),
            client_name: Some("player".to_owned()),
            client_team,
            client_mobile: false,
            client_music_playing: false,
            client_current_music: None,
            client_unit: self.player_unit(),
        };
        crate::logic::globals::GlobalVars::update_world(&mut self.ecs.0, &update);
    }

    /// Drains the world-processor side-effect log once per tick
    /// (`LogicWorldState.events`; the T5/T6 half of `LExecutor`'s host sinks).
    fn drain_logic_events(&mut self) {
        let events = match self
            .ecs
            .0
            .get_resource_mut::<crate::logic::world::LogicWorldState>()
        {
            Some(mut state) => std::mem::take(&mut state.events),
            None => return,
        };
        for event in events {
            if self.logic_event_log.len() == 64 {
                self.logic_event_log.pop_front();
            }
            self.logic_event_log.push_back(event.kind().to_owned());
            use crate::logic::world::LogicWorldEvent;
            match event {
                // `SpawnWaveI` natural: `logic.skipWave()` (`LExecutor.java:2173-2175`).
                LogicWorldEvent::SpawnWave { natural: true, .. } => self.logic_skip_wave(),
                // Positional spawn (`LExecutor.java:2177-2198`).
                LogicWorldEvent::SpawnWave {
                    natural: false,
                    x,
                    y,
                } => self.logic_spawn_wave_at(x, y),
                // T5 adds SetBlock handling here.
                e @ LogicWorldEvent::Message { .. } if self.logic_ui_events.len() < 256 => {
                    self.logic_ui_events.push(e);
                }
                // `CutsceneI` state lives in `LogicWorldState.cutscene` (T3).
                _ => {}
            }
        }
    }

    /// Monotonic update counter (`GameState.updateId`).
    pub fn update_id(&self) -> u64 {
        self.state.update_id
    }

    /// `InputHandler.logicCutscene` presentation state (defaults when no live
    /// `LogicWorldState`).
    pub fn logic_cutscene(&self) -> crate::logic::world::LogicCutscene {
        self.ecs
            .0
            .get_resource::<crate::logic::world::LogicWorldState>()
            .map(|state| state.cutscene)
            .unwrap_or_default()
    }

    /// Takes the pending `CutsceneI shake` request (`Renderer.shake(p1, p2 * 60)`).
    pub fn take_logic_shake(&mut self) -> Option<(f32, f32)> {
        self.ecs
            .0
            .get_resource_mut::<crate::logic::world::LogicWorldState>()?
            .cutscene
            .shake
            .take()
    }

    /// Drains queued world-processor messages (`announce`/`notify`/`toast`;
    /// `mission` is written to the rules directly). Only `Message` events
    /// remain in `logic_ui_events` after T3.
    pub fn take_logic_messages(&mut self) -> Vec<(crate::logic::enums::MessageType, String, f32)> {
        let mut out = Vec::new();
        self.logic_ui_events.retain(|event| {
            if let crate::logic::world::LogicWorldEvent::Message {
                type_,
                text,
                duration,
            } = event
            {
                out.push((*type_, text.clone(), *duration));
                false
            } else {
                true
            }
        });
        out
    }

    /// `InputHandler` world-reset half (`InputHandler.java:921-922`): clears the
    /// logic cutscene/HUD flags and the message timers on a world load.
    pub fn reset_logic_presentation(&mut self) {
        if let Some(mut state) = self
            .ecs
            .0
            .get_resource_mut::<crate::logic::world::LogicWorldState>()
        {
            state.cutscene = crate::logic::world::LogicCutscene::default();
        }
        if let Some(mut messages) = self
            .ecs
            .0
            .get_resource_mut::<crate::logic::world::MessageState>()
        {
            messages.has_announcement = false;
            messages.has_toast = false;
            messages.mission = None;
            messages.announce_until = 0;
            messages.toast_until = 0;
        }
        self.logic_ui_events.clear();
    }

    /// Current phase name (inspector/parity).
    pub fn state_name(&self) -> &'static str {
        self.state.phase.name()
    }

    /// Per-group live entity counts (plan 05 M9 inspector surface).
    ///
    /// At the P0/M9 stage only the `build` group carries entities; the remaining
    /// groups report `0` until their plans (08–11) register membership. The map
    /// is a `BTreeMap` so the inspector dictionary is deterministic.
    pub fn group_counts(&self) -> std::collections::BTreeMap<&'static str, usize> {
        // `all` counts real sim entities (`EntitySeq`); Bevy's raw entity slots
        // can include internal/reserved ids that are not sim entities.
        let all = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.get::<crate::ecs::EntitySeq>().is_some())
            .count();
        let build = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.get::<crate::ecs::BuildingComp>().is_some())
            .count();
        // Live unit/bullet halves of `Groups` (present only once the opt-in
        // unit runtime is installed).
        let (unit, bullet) = match self.ecs.0.get_non_send::<unit_runtime::UnitRuntime>() {
            Some(runtime) => (
                runtime.unit_count(&self.ecs.0),
                runtime.bullet_count(&self.ecs.0),
            ),
            None => (0, 0),
        };
        let weather = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| {
                entity
                    .get::<crate::entities::comp::WeatherState>()
                    .is_some()
            })
            .count();
        let mut counts = std::collections::BTreeMap::new();
        counts.insert("all", all);
        counts.insert("unit", unit);
        counts.insert("build", build);
        counts.insert("bullet", bullet);
        counts.insert("player", 0);
        counts.insert("effect", 0);
        counts.insert("weather", weather);
        counts.insert("power_graph", 0);
        counts.insert("sync", 0);
        counts.insert("draw", 0);
        counts
    }

    /// The deterministic fixed-step clock.
    pub fn clock(&self) -> &SimClock {
        &self.clock
    }

    /// The simulation configuration.
    pub fn config(&self) -> &SimConfig {
        &self.config
    }

    /// The host platform seam.
    pub fn platform(&self) -> &dyn Platform {
        self.platform.as_ref()
    }

    /// Canonical deterministic checksum (§6.5; HLP C2): FNV-1a-64 over the
    /// versioned stream — `GameState` scalars → clock time/runs → `Sim` RNG
    /// state → grid row-major → entities in stable seq order.
    pub fn checksum(&self) -> u64 {
        self.checksum_value().value()
    }

    /// The strongly-typed [`Checksum`].
    pub fn checksum_value(&self) -> Checksum {
        let mut c = Checksummer::new();
        c.part(&self.state.phase.as_u8());
        c.part(&self.state.tick);
        c.part(&self.state.update_id);
        c.part(&self.clock.time_units());
        c.part(&(self.clock.runs_len() as u64));
        c.part(&self.rng_streams.sim_state());
        c.part(&self.seed);

        for (_pos, index) in self.grid.iter_row_major() {
            let tile = self.grid.tile_ref(index);
            let block = tile.block;
            let build = tile.build;
            c.part(&block.raw());
            // Team/rotation live on the building entity; the P0 path always uses
            // team `sharded` (0) / rotation 0, preserved here to keep the
            // canonical stream byte-identical.
            c.part(&0u8);
            c.part(&0u8);
            match build.and_then(|entity| self.ecs.seq_of(entity)) {
                Some(build_id) => {
                    c.part(&1u8);
                    c.part(&build_id);
                }
                None => {
                    c.part(&0u8);
                    c.part(&0u64);
                }
            }
        }

        for (seq, _entity, comp) in self.ecs.entities_by_seq() {
            c.part(&seq);
            c.part(&comp.block.raw());
            c.part(&comp.team.0);
            c.part(&comp.rot);
            c.part(&comp.pos.x());
            c.part(&comp.pos.y());
        }

        c.finish()
    }

    /// Checksum as 16 lowercase hex digits (golden format).
    pub fn checksum_hex(&self) -> String {
        self.checksum_value().to_hex()
    }

    /// Scoped checksum (plan 21 §6.5): walks the identical plan-05 hasher order
    /// but replaces fields excluded by `scope` with their declared defaults.
    ///
    /// At the P0/M9 stage there are no `SyncLocal`/player-possessed components,
    /// so no field is masked and the value equals [`Sim::checksum`]. The seam is
    /// additive: plans 05/11/16 hook masked fields here without changing the
    /// relay API. `scope.bits()` is the value peers publish in
    /// `match_checksum.scope`.
    pub fn checksum_scoped(&self, scope: &ChecksumScope) -> u64 {
        let _ = scope;
        self.checksum()
    }

    /// Applies a canonical [`SimCommand`] through the same path the relay uses
    /// (plan 05 §6.4). Unsupported/unknown commands are structured errors.
    pub fn command(
        &mut self,
        command: crate::determinism::SimCommand,
    ) -> Result<(), crate::determinism::CommandError> {
        use crate::determinism::{CommandError, SimCommand};
        match &command {
            SimCommand::UnitCommand {
                units,
                command: unit_command,
                x,
                y,
            } => {
                // Live units first (ids are `EntitySeq`; PAR-0140); the relay
                // runtime keeps the wire-only/net bookkeeping path.
                if self.apply_live_unit_command(units, *unit_command, *x, *y) == 0 {
                    self.ensure_unit_commands()
                        .apply_unit_command(units, *unit_command, *x, *y);
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::UnitControl { unit } => {
                // `InputHandler.unitControl` (`InputHandler.java:775-822`):
                // possess an own unit, or clear the possession.
                match unit {
                    Some(seq) => {
                        self.possess_unit(*seq);
                    }
                    None => {
                        self.clear_unit();
                    }
                }
                self.controlled_unit = *unit;
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::UnitClear => {
                self.clear_unit();
                self.controlled_unit = None;
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::SpawnUnit { unit, x, y, team } => {
                // Host/harness spawn: only the live unit runtime can carry it
                // (`spawn_unit_def` inserts the full unit component closure).
                if self.has_unit_runtime() {
                    let ecs = &mut self.ecs.0;
                    let Some(mut runtime) = ecs.remove_non_send::<unit_runtime::UnitRuntime>()
                    else {
                        return Err(CommandError::Unsupported("spawn_unit"));
                    };
                    let id = crate::content::UnitTypeId::new(*unit);
                    let entity = runtime.spawn_by_id(ecs, id, *team, *x, *y, 0.0);
                    ecs.insert_non_send(runtime);
                    if entity.is_none() {
                        return Err(CommandError::UnknownContent(*unit));
                    }
                    self.commands_applied = self.commands_applied.wrapping_add(1);
                    return Ok(());
                }
            }
            SimCommand::BuildingControlSelect { x, y } => {
                self.control_building = Some(TilePos::new(*x, *y));
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::UnitStance {
                units,
                stance,
                enabled,
            } => {
                // Live units first; `0` routes to the relay runtime (net id
                // space), mirroring `UnitCommand` (G01 T11).
                if self.apply_live_unit_stance(units, *stance, *enabled) == 0 {
                    self.ensure_unit_commands()
                        .set_unit_stance(units, *stance, *enabled);
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Rotate { x, y, direction } => {
                let pos = TilePos::new(*x, *y);
                if self.grid.index(pos).is_err() {
                    return Err(CommandError::InvalidTarget);
                }
                let Some(entity) = self.grid.entity_at(pos) else {
                    return Err(CommandError::InvalidTarget);
                };
                // Upstream `InputHandler.rotateBlock`: `rotation = mod(rotation +
                // sign(direction), 4)`; `direction == true` is counter-clockwise.
                let step = if *direction { 1 } else { -1 };
                let Some(new_rot) =
                    self.ecs
                        .0
                        .get_mut::<BuildingComp>(entity)
                        .map(|mut building| {
                            building.rot = (building.rot as i32 + step).rem_euclid(4) as u8;
                            building.rot
                        })
                else {
                    return Err(CommandError::InvalidTarget);
                };
                // The live runtime reads `Building.rotation`; keep both in sync.
                if let Some(mut building) = self
                    .ecs
                    .0
                    .get_mut::<crate::entities::comp::Building>(entity)
                {
                    building.rotation = new_rot;
                }
                crate::world::update::no_sleep(&mut self.ecs.0, entity);
                // Rotation matters for the facing scans (`BeamDrill.updateFacing`
                // / `WallCrafter.getEfficiency`; PAR-0079 §5).
                if let Some(runtime) = self.block_runtime.as_ref() {
                    let content = runtime.content();
                    crate::world::behavior::production::refresh_drill_ore(
                        &mut self.ecs.0,
                        &self.grid,
                        content,
                        entity,
                    );
                }
                // `Build.beginPlace` (same-block rotate) FX (`Build.java:88,112`):
                // `Fx.rotateBlock.at(build.x, build.y, build.block.size)`.
                let placed_block = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                if let Some(def) = self
                    .block_runtime
                    .as_ref()
                    .and_then(|runtime| runtime.content().block(placed_block))
                {
                    let (offset, size) = (def.offset, def.size);
                    let cx = *x as f32 * 8.0 + offset;
                    let cy = *y as f32 * 8.0 + offset;
                    if let Some(runtime) = self.unit_runtime() {
                        use crate::content::registries::fx_meta::EffectRef;
                        runtime.fx.effect(
                            &EffectRef::Named(crate::content::EffectId::ROTATE_BLOCK),
                            cx,
                            cy,
                            size as f32,
                            crate::content::Rgba::WHITE,
                        );
                    }
                    // `Build.java:90`/`InputHandler.java:704`:
                    // `Sounds.blockRotate.at(build, 1f + Mathf.range(0.1f), 1f)`.
                    if let Some(audio) = self
                        .ecs
                        .0
                        .get_resource::<crate::audio::AudioSinkRes>()
                        .cloned()
                    {
                        let fx = self
                            .rng_streams
                            .next_float(crate::determinism::RngStream::Fx);
                        audio.emit(crate::audio::AudioEvent::at(
                            crate::audio::SoundId::BLOCK_ROTATE,
                            cx,
                            cy,
                            1.0 + (fx * 0.2 - 0.1),
                            1.0,
                            true,
                        ));
                    }
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Payload { kind, x, y, target } => {
                // `Payloadc` unit carrier (PAR-0138): pick up a unit/building or
                // drop the last carried payload. No-op without a player unit.
                let _ = self.apply_payload_command(*kind, *x, *y, *target);
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Inventory {
                kind,
                x,
                y,
                item,
                amount,
                angle,
            } => {
                // `InputHandler.requestItem`/`transferInventory`/`dropItem`.
                let _ = angle;
                let pos = TilePos::new(*x, *y);
                if self.grid.index(pos).is_err() {
                    return Err(CommandError::InvalidTarget);
                }
                if self.player_unit.is_some() {
                    // Live player unit: the transfer operates on the unit's own
                    // stack with Java's range/`interactable`/`acceptStack`
                    // checks (G08 T2a). The relay `player_item` stays for
                    // headless/relay callers.
                    self.apply_player_inventory(*kind, pos, *item, *amount);
                } else if self.block_runtime.is_some()
                    && let Some(entity) = self.grid.entity_at(pos)
                {
                    // Relay runtime: the real `ItemModule` is the storage the
                    // block behaviors read, so mutate that instead of the relay
                    // mirror.
                    let world_apply = self
                        .world_apply
                        .get_or_insert_with(world_apply::WorldApplyRuntime::new);
                    world_apply.apply_inventory_entity(
                        &mut self.ecs.0,
                        entity,
                        *kind,
                        *item,
                        *amount,
                    )?;
                } else {
                    let block = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                    self.ensure_world_apply()
                        .apply_inventory(pos, block, *kind, *item, *amount)?;
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::DeletePlans { positions } => {
                // `InputHandler.deletePlans`: the relay `BuildQueue` plus the
                // player unit's own `BuilderPlans` queue (PAR-0098 §2a).
                self.ensure_world_apply().apply_delete_plans(positions)?;
                self.delete_player_plans(positions);
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::CommandBuilding { positions, x, y } => {
                // `InputHandler.commandBuilding`: `Building.onCommand(target)`
                // for every live, `Block.commandable` building (upstream also
                // checks the player team; the client selection filters team and
                // `Sim` has no relay-side team).
                for &packed in positions.iter() {
                    let pos = TilePos::from_pack(packed);
                    if self.grid.index(pos).is_err() || self.grid.entity_at(pos).is_none() {
                        continue;
                    }
                    let Some(block) = self.grid.block_at(pos) else {
                        continue;
                    };
                    let commandable = self
                        .ensure_world_apply()
                        .content()
                        .block(block)
                        .is_some_and(|def| def.commandable);
                    if !commandable {
                        continue;
                    }
                    self.ensure_world_apply()
                        .apply_command_building(pos, *x, *y);
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Place {
                x,
                y,
                block,
                rotation,
                team,
                player,
            } => {
                // The live runtime honors rotation/team end-to-end; without it the
                // P0 path below still drops rotation (no rotation-aware `Command`).
                if self.block_runtime.is_some() {
                    let id = BlockId::new(*block);
                    if self.content.name(id).is_err() {
                        return Err(CommandError::UnknownContent(*block));
                    }
                    if id == BlockId::AIR {
                        return Err(CommandError::InvalidTarget);
                    }
                    let pos = TilePos::new(*x, *y);
                    if self.grid.index(pos).is_err() {
                        return Err(CommandError::InvalidTarget);
                    }
                    let rot = u8::try_from((*rotation).rem_euclid(4)).unwrap_or(0);
                    // Player input queues a build plan on the player unit
                    // (`InputHandler.flushPlans` -> `BuilderComp.addBuild`);
                    // the builder pass constructs it over time (PAR-0098).
                    if player.is_some() && self.player_can_build() {
                        if !self.queue_player_place(*x, *y, id, rot) {
                            log::debug!("player plan rejected at ({x}, {y})");
                        }
                        self.commands_applied = self.commands_applied.wrapping_add(1);
                        return Ok(());
                    }
                    if player.is_some() && self.team_has_core(*team) {
                        // No builder: Java's `player.unit()` is `Nulls.unit`,
                        // `addBuild` returns at `if(!canBuild()) return;`
                        // (`BuilderComp.java`).
                        log::debug!(
                            "player plan at ({x}, {y}) dropped: no build-capable player unit"
                        );
                        self.commands_applied = self.commands_applied.wrapping_add(1);
                        return Ok(());
                    }
                    if self.runtime_place(
                        *x,
                        *y,
                        id,
                        rot,
                        *team,
                        crate::world::build::PlaceFlags::API,
                    ) {
                        self.push_event(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                            x: *x,
                            y: *y,
                            block: id,
                            team: *team,
                        }));
                    } else {
                        log::debug!(
                            "place command rejected at ({x}, {y}) by the live block runtime"
                        );
                    }
                    self.commands_applied = self.commands_applied.wrapping_add(1);
                    return Ok(());
                }
            }
            SimCommand::Break {
                x,
                y,
                player: Some(_),
            } if self.player_can_build() => {
                // `InputHandler.removeSelection` -> a deconstruction plan.
                let pos = TilePos::new(*x, *y);
                if self.grid.index(pos).is_err() {
                    return Err(CommandError::InvalidTarget);
                }
                if !self.queue_player_break(*x, *y) {
                    log::debug!("player break plan rejected at ({x}, {y})");
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Break {
                x,
                y,
                player: Some(_),
            } if self.team_has_core(self.player_command_team()) => {
                // No builder: `player.unit()` is `Nulls.unit`, whose
                // `addBuild` (break plan) does nothing (`BuilderComp.java`).
                log::debug!("player break at ({x}, {y}) dropped: no build-capable player unit");
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::Configure { x, y, value } => {
                // `InputHandler` configure: apply the value to the target
                // building's behavior. Unsupported until a block runtime exists
                // (P0 has no behavior table).
                if self.block_runtime.is_none() {
                    return Err(CommandError::Unsupported("configure"));
                }
                let pos = TilePos::new(*x, *y);
                if self.grid.index(pos).is_err() {
                    return Err(CommandError::InvalidTarget);
                }
                let Some(entity) = self.grid.entity_at(pos) else {
                    return Err(CommandError::InvalidTarget);
                };
                let world_value = match self.block_runtime.as_ref() {
                    Some(runtime) => runtime.resolve_config(value)?,
                    None => return Err(CommandError::Unsupported("configure")),
                };
                // T2: power nodes take link configs (`PowerNode.java`
                // `config(Integer.class)` / `config(Point2[].class)`) through the
                // `PowerGrids` arena, not the behavior config path.
                if self.configure_power_node(entity, &world_value) {
                    self.commands_applied = self.commands_applied.wrapping_add(1);
                    return Ok(());
                }
                if !crate::world::config::configure(&mut self.ecs.0, entity, None, world_value) {
                    return Err(CommandError::InvalidTarget);
                }
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            _ => {}
        }
        let op = command.op_name();
        match command.to_p0() {
            Some(p0) => self.apply(p0).map_err(|error| match error {
                SimError::UnknownBlock(id) => CommandError::UnknownContent(id),
                SimError::World(_) | SimError::CannotPlaceAir => CommandError::InvalidTarget,
            }),
            None => Err(CommandError::Unsupported(op)),
        }
    }

    /// Captures the minimal deterministic snapshot header (plan 21 restore hook).
    pub fn snapshot(&self) -> SimSnapshot {
        SimSnapshot {
            tick: self.state.tick,
            update_id: self.state.update_id,
            phase: self.state.phase,
            checksum: self.checksum(),
        }
    }

    /// Canonical sparse state dump (§6.3).
    pub fn dump(&self) -> StateDump {
        dump::build(self, false)
    }

    /// Canonical dump that includes every tile (`--all-tiles`).
    pub fn dump_all_tiles(&self) -> StateDump {
        dump::build(self, true)
    }

    /// Dump serialized as pretty JSON (byte-stable; structs only, no maps).
    pub fn dump_json(&self, all_tiles: bool) -> Result<String, JsonError> {
        serde_json::to_string_pretty(&if all_tiles {
            self.dump_all_tiles()
        } else {
            self.dump()
        })
    }

    /// Current tick.
    pub fn tick_count(&self) -> u64 {
        self.state.tick
    }

    /// Scenario seed.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Re-derives the world state that mirrors `Rules`: the map-area limit rect
    /// and border darkness (`World.isInMapArea`/`getDarkness`,
    /// `World.java:337-339, 483-503`), plus the build guard (G02 T8's
    /// `sync_build_rules`, called from here once it exists). Call after any
    /// rules change: sector start (`MindCampaign`), or mid-match through
    /// `CampaignRuntime.rules_world_dirty` (G06 `setrule`) / the gdext
    /// `apply_world_rules` wrapper.
    pub fn apply_rules_to_world(&mut self, rules: &crate::game::rules::Rules) {
        let new_limit = rules.limit_map_area.then_some((
            rules.limit_x,
            rules.limit_y,
            rules.limit_width,
            rules.limit_height,
        ));
        // `World.checkMapArea(prev)` (`World.java:469-481`): a shrunken area
        // re-enables disabled `autoResetEnabled` buildings outside the previous
        // rect before the new one is checked.
        let prev = self.grid.limit_area;
        if let Some(prev) = prev
            && Some(prev) != new_limit
        {
            self.reset_map_area(prev);
        }
        self.grid.limit_area = new_limit;
        self.grid.border_darkness = rules.border_darkness;
        // `World.setRules` + every `Build.validPlace` read of `state.rules`
        // (H02 T2 `sync_build_rules`); `is_campaign` comes from the live
        // campaign runtime when present.
        let is_campaign = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .map(|runtime| runtime.session.is_campaign())
            .unwrap_or(false);
        crate::world::limits::sync_build_rules(&mut self.ecs.0, rules, is_campaign);
        self.refresh_unit_runtime();
        // `WorldLoadEvent` half (`World.java:44-52`): every building re-checks
        // `allowUpdate` against the new rules.
        self.check_all_allow_update();
        // `WorldLoadEvent` fog half (`FogControl.java:46-62`): (re)size the fog
        // whenever the flags change or the resource is missing. Serpulo
        // (`!rules.fog`) removes the resource and pays nothing per tick.
        let flags = (rules.fog, rules.static_fog);
        let missing = flags.0
            && self
                .ecs
                .0
                .get_resource::<crate::game::fog::FogControl>()
                .is_none();
        if self.last_fog_flags != Some(flags) || missing {
            self.last_fog_flags = Some(flags);
            self.reset_fog();
        }
    }

    /// `World.checkAllowUpdate` for every live building (`World.java:44-52`).
    /// Editor worlds skip it (`EditorTile` is always allowed).
    pub fn check_all_allow_update(&mut self) {
        let (env, limit_area, disable_outside_area, editor) =
            match crate::game::runtime::rules_of(&self.ecs.0) {
                Some(rules) => (
                    rules.env,
                    rules.limit_map_area.then_some((
                        rules.limit_x,
                        rules.limit_y,
                        rules.limit_width,
                        rules.limit_height,
                    )),
                    rules.disable_outside_area,
                    rules.editor,
                ),
                None => (crate::game::rules::DEFAULT_ENV, None, true, false),
            };
        if editor {
            return;
        }
        let mut buildings: Vec<bevy_ecs::entity::Entity> = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.contains::<crate::entities::comp::Building>())
            .map(|entity| entity.id())
            .collect();
        buildings.sort_by_key(|entity| entity.index());
        for entity in buildings {
            crate::world::allow_update::check_allow_update(
                &mut self.ecs.0,
                entity,
                env,
                limit_area,
                disable_outside_area,
            );
        }
    }

    /// `Tile.setTeam` for the building at `(x, y)` (`BuildingComp.changeTeam`
    /// -> `checkAllowUpdate`, `BuildingComp.java:1842`).
    pub fn debug_set_building_team(&mut self, x: i32, y: i32, team: u8) -> bool {
        let Some(entity) = self
            .grid
            .tiles
            .in_bounds(x, y)
            .then(|| self.grid.tile(x, y).build)
            .flatten()
        else {
            return false;
        };
        let Some(mut comp) = self
            .ecs
            .0
            .get_mut::<crate::entities::comp::TeamComp>(entity)
        else {
            return false;
        };
        comp.team = team;
        let (env, limit_area, disable_outside_area) =
            match crate::game::runtime::rules_of(&self.ecs.0) {
                Some(rules) => (
                    rules.env,
                    rules.limit_map_area.then_some((
                        rules.limit_x,
                        rules.limit_y,
                        rules.limit_width,
                        rules.limit_height,
                    )),
                    rules.disable_outside_area,
                ),
                None => (crate::game::rules::DEFAULT_ENV, None, true),
            };
        crate::world::allow_update::check_allow_update(
            &mut self.ecs.0,
            entity,
            env,
            limit_area,
            disable_outside_area,
        );
        true
    }

    /// `World.checkMapArea(prevX, prevY, prevW, prevH)`'s re-enable half
    /// (`World.java:472-476`).
    fn reset_map_area(&mut self, prev: (i32, i32, i32, i32)) {
        let mut buildings: Vec<bevy_ecs::entity::Entity> = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| entity.contains::<crate::entities::comp::Building>())
            .map(|entity| entity.id())
            .collect();
        buildings.sort_by_key(|entity| entity.index());
        let (x, y, w, h) = prev;
        for entity in buildings {
            let (tile, auto_reset, enabled) = {
                let Some(building) = self.ecs.0.get::<crate::entities::comp::Building>(entity)
                else {
                    continue;
                };
                let Some(auto_reset) = self
                    .ecs
                    .0
                    .get_resource::<crate::world::BlockTable>()
                    .and_then(|table| table.instance(building.block))
                    .map(|instance| instance.def.auto_reset_enabled)
                else {
                    continue;
                };
                (building.tile, auto_reset, building.enabled)
            };
            let (tx, ty) = (tile.x() as i32, tile.y() as i32);
            if !enabled
                && auto_reset
                && !(tx >= x && ty >= y && tx < x + w && ty < y + h)
                && let Some(mut building) = self
                    .ecs
                    .0
                    .get_mut::<crate::entities::comp::Building>(entity)
            {
                building.enabled = true;
            }
        }
    }

    /// Current phase.
    pub fn phase(&self) -> State {
        self.state.phase
    }

    /// Changes the phase, queueing a `StateChangeEvent` when it changes.
    pub fn set_phase(&mut self, phase: State) -> bool {
        let from = self.state.phase;
        if self.state.set_phase(phase) {
            self.push_event(SimEvent::StateChangeEvent(StateChangeEvent {
                from,
                to: phase,
            }));
            true
        } else {
            false
        }
    }

    /// Pauses/resumes by phase (convenience for hosts).
    pub fn set_paused(&mut self, paused: bool) {
        self.set_phase(if paused {
            State::Paused
        } else {
            State::Playing
        });
    }

    /// Whether the phase is `Paused`.
    pub fn is_paused(&self) -> bool {
        self.state.phase == State::Paused
    }

    /// Currently selected block (UI intent).
    pub fn selected_block(&self) -> BlockId {
        self.selected_block
    }

    /// Sets the selected block, validating the registry.
    pub fn set_selected_block(&mut self, block: BlockId) -> Result<(), SimError> {
        self.content.name(block).map_err(SimError::from)?;
        self.selected_block = block;
        Ok(())
    }

    /// Wall/block at a tile.
    pub fn block_at(&self, pos: TilePos) -> Option<BlockId> {
        self.grid.block_at(pos)
    }

    /// Content registry.
    pub fn content(&self) -> &Blocks {
        &self.content
    }

    /// Block name for a resolved id (fallback `?<id>` for unknown ids).
    pub fn block_name_of(&self, block: BlockId) -> String {
        match self.content.name(block) {
            Ok(name) => name.to_owned(),
            Err(_) => format!("?{}", block.raw()),
        }
    }

    /// Number of commands applied so far (valid or rejected no-ops).
    pub fn commands_applied(&self) -> u64 {
        self.commands_applied
    }

    /// Lazily builds the unit-command runtime (plan 15 M4).
    fn ensure_unit_commands(&mut self) -> &mut command::UnitCommandRuntime {
        self.unit_commands
            .get_or_insert_with(command::UnitCommandRuntime::new)
    }

    /// Lazily builds the relay world-mutation runtime (plan 15 §3.3.1).
    fn ensure_world_apply(&mut self) -> &mut world_apply::WorldApplyRuntime {
        self.world_apply
            .get_or_insert_with(world_apply::WorldApplyRuntime::new)
    }

    /// Read-only relay world-mutation runtime, if any such command has applied.
    pub fn world_apply(&self) -> Option<&world_apply::WorldApplyRuntime> {
        self.world_apply.as_ref()
    }

    /// Seeds the controlled player's carried stack (`Unit.stack`; host/test).
    pub fn set_player_item(
        &mut self,
        item: Option<(u16, i32)>,
    ) -> Result<(), crate::determinism::CommandError> {
        self.ensure_world_apply().set_player_item(item)
    }

    /// Resolves an item content name to its raw id via the relay world runtime
    /// (boots it on first use). `None` when the name is unknown.
    pub fn world_item_id(&mut self, name: &str) -> Option<u16> {
        self.ensure_world_apply()
            .content()
            .item_by_name(name)
            .map(|item| item.id.raw())
    }

    /// Resolves a block content name to its raw id via the relay world runtime
    /// (boots it on first use). `None` when the name is unknown.
    pub fn world_block_id(&mut self, name: &str) -> Option<u16> {
        self.ensure_world_apply()
            .content()
            .block_by_name(name)
            .map(|block| block.id.raw())
    }

    /// The controlled player's carried stack as `(item id, amount)`, if any.
    pub fn player_item(&self) -> Option<(u16, i32)> {
        self.world_apply()?
            .player_item()
            .map(|(item, amount)| (item.raw(), amount))
    }

    /// Queues a build plan (host/test seeding of `BuilderComp.plans`).
    pub fn enqueue_build_plan(&mut self, plan: crate::input::plan::ClientPlan) {
        self.ensure_world_apply().plans_mut().add_build(plan, false);
    }

    /// The queued build plan at `(x, y)`, if any.
    pub fn build_plan_at(&self, x: i32, y: i32) -> Option<&crate::input::plan::ClientPlan> {
        self.world_apply()?.plans().get(x, y)
    }

    /// The `command_pos` recorded for a commanded building, if any.
    pub fn building_command_target(&self, pos: TilePos) -> Option<(f32, f32)> {
        self.world_apply()?.commanded_target(pos)
    }

    /// Deterministic digest of the relay world-mutation state (ascending order).
    pub fn world_apply_checksum(&self) -> Checksum {
        match &self.world_apply {
            Some(runtime) => runtime.checksum(),
            None => Checksummer::new().finish(),
        }
    }

    /// Read-only unit-command runtime, if any unit command has been applied.
    pub fn unit_commands(&self) -> Option<&command::UnitCommandRuntime> {
        self.unit_commands.as_ref()
    }

    /// Registers a commandable unit for the relay-side apply path, returning its
    /// relay id.
    pub fn spawn_command_unit(&mut self, name: &str, team: u8) -> Option<i32> {
        self.ensure_unit_commands().spawn(name, team)
    }

    /// The `CommandAI` state of a relay unit id, if tracked.
    pub fn unit_command_state(&self, id: i32) -> Option<&crate::ai::CommandAiState> {
        self.unit_commands.as_ref()?.state(id)
    }

    /// Deterministic digest of the relay unit-command state (ascending id order).
    pub fn unit_command_checksum(&self) -> Checksum {
        match &self.unit_commands {
            Some(runtime) => runtime.checksum(),
            None => Checksummer::new().finish(),
        }
    }

    /// Currently possessed unit id (`unitControl`), if any.
    pub fn controlled_unit(&self) -> Option<i32> {
        self.controlled_unit
    }

    /// Currently selected controllable building (`buildingControlSelect`).
    pub fn control_building(&self) -> Option<TilePos> {
        self.control_building
    }

    /// Recorded block events, oldest first.
    pub fn event_log(&self) -> &[dump::DumpEvent] {
        &self.event_log
    }

    fn push_event(&mut self, event: SimEvent) {
        self.pending_events.push(event);
    }

    /// P0 flush: dispatch queued events synchronously at tick end.
    fn flush_events(&mut self) {
        if self.pending_events.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending_events);
        for event in pending {
            // H09 T1 fog listeners (`FogControl.java:64-95`): placed/broken
            // fog-radius buildings mark the dynamic flush pending.
            self.handle_fog_block_event(&event);
            // `Logic`'s `BlockBuildEndEvent`/`BlockDestroyEvent` listeners
            // (`Logic.java:186-216`; H09 T10): placed/deconstructed/destroyed
            // counts plus the campaign planet maps, keyed by block name.
            match &event {
                SimEvent::BlockPlacedEvent(placed) => {
                    if let Ok(name) = self.content.name(placed.block)
                        && let Some(mut runtime) = self
                            .ecs
                            .0
                            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
                        && runtime.session.rules.default_team == placed.team
                    {
                        runtime.session.stats.add_placed(name, 1);
                        runtime.session.stats.buildings_built += 1;
                        if runtime.session.is_campaign()
                            && let Some((planet, _)) = runtime.session.sector
                            && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                        {
                            stats.add_building_built(name, 1);
                        }
                    }
                }
                SimEvent::BlockBrokenEvent(broken) => {
                    if let Ok(name) = self.content.name(broken.block)
                        && let Some(mut runtime) = self
                            .ecs
                            .0
                            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
                    {
                        let default_team = runtime.session.rules.default_team;
                        if broken.destroyed {
                            if broken.team == default_team {
                                runtime.session.stats.buildings_destroyed += 1;
                                if runtime.session.is_campaign()
                                    && let Some((planet, _)) = runtime.session.sector
                                    && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                                {
                                    stats.add_building_destroyed(name, 1);
                                }
                            } else {
                                runtime.session.stats.add_destroyed(name, 1);
                                if runtime.session.is_campaign()
                                    && let Some((planet, _)) = runtime.session.sector
                                    && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                                {
                                    stats.add_enemy_building_destroyed(name, 1);
                                }
                            }
                        } else if broken.team == default_team {
                            runtime.session.stats.buildings_deconstructed += 1;
                            if runtime.session.is_campaign()
                                && let Some((planet, _)) = runtime.session.sector
                                && let Some(stats) = runtime.campaign.stats_for_mut(planet)
                            {
                                stats.add_building_deconstructed(name, 1);
                            }
                        }
                    }
                }
                _ => {}
            }
            let record = dump::block_event_record(self, &event);
            if let Some(record) = record {
                self.event_log.push(record);
            }
            // G07 T3: live place/break sounds
            // (`ConstructBlock.constructFinish`/`deconstructFinish`,
            // `ConstructBlock.java:58-120`; `PAR-0073`). Fog gating
            // (`fogControl.isVisibleTile`) is skipped (fog is G03).
            if let Some(audio) = self
                .ecs
                .0
                .get_resource::<crate::audio::AudioSinkRes>()
                .cloned()
                && let Some(content) = self.block_runtime.as_ref().map(|runtime| runtime.content())
            {
                match &event {
                    SimEvent::BlockPlacedEvent(placed) => {
                        if let Some(def) = content.block(placed.block) {
                            let (x, y) =
                                crate::audio::blocks::world_center(def, placed.x, placed.y);
                            audio.emit(crate::audio::AudioEvent::Construct {
                                sound: crate::audio::blocks::place_sound(def),
                                x,
                                y,
                                place: true,
                                pitch_change: true,
                            });
                        }
                    }
                    SimEvent::BlockBrokenEvent(broken) if !broken.destroyed => {
                        if let Some(def) = content.block(broken.block) {
                            let (x, y) =
                                crate::audio::blocks::world_center(def, broken.x, broken.y);
                            audio.emit(crate::audio::AudioEvent::Construct {
                                sound: crate::audio::blocks::break_sound(def),
                                x,
                                y,
                                place: false,
                                pitch_change: true,
                            });
                        }
                    }
                    _ => {}
                }
            }
            self.events.dispatch(event);
        }
    }
}

/// Entity kind byte used by the checksum stream (`0 = building`).
pub const KIND_BUILDING: u8 = 0;

#[cfg(test)]
mod combat_tests;
#[cfg(test)]
mod production_tests;
#[cfg(test)]
mod tests;
