// SPDX-License-Identifier: GPL-3.0-only

//! The P0 simulation spine: fixed-step tick, command application, checksum and dump.
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`update`, `updateEntities`)
//! and `GameState`; the reserved slot order lives in [`crate::schedule`].

pub mod boot;
pub mod clock;
pub mod command;
pub mod config;
pub mod dump;
pub mod events;
pub mod fixed;
pub mod io_set;
pub mod logic;
pub mod reset;
pub mod schedule;

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
pub use clock::SimClock;
pub use command::{MOVE_COMMAND, UnitCommandRuntime, resolve_wire_target};
pub use config::SimConfig;
pub use dump::StateDump;
pub use events::{ALL_TRIGGERS, Trigger, TriggerRegistry};
pub use fixed::{FixedStepRunner, SIM_STEP};
pub use io_set::{DeferringIoHandler, IoHandler, IoQueue, IoRequest, IoResponse, IoSet, IoStatus};

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

    /// Relay-side unit-command apply state (plan 15 M4). Built lazily on the
    /// first unit command so P0 worlds never pay for content boot.
    unit_commands: Option<command::UnitCommandRuntime>,
    /// Currently possessed unit (`unitControl`); `None` means the player unit.
    controlled_unit: Option<i32>,
    /// Currently selected controllable building (`buildingControlSelect`).
    control_building: Option<TilePos>,

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
            controlled_unit: None,
            control_building: None,
            seed,
            selected_block: BlockId::STONE_WALL,
            entity_seq: EntitySequencer::default(),
            commands_applied: 0,
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
                    self.push_event(SimEvent::BlockPlacedEvent(BlockPlacedEvent { x, y, block }));
                } else {
                    log::debug!(
                        "place command rejected at ({x}, {y}): tile already holds block {}",
                        current.raw()
                    );
                }
            }
            Command::Break { x, y } => {
                let pos = TilePos::new(x, y);
                self.grid.index(pos)?;
                let current = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                if current != BlockId::AIR {
                    if let Some(entity) = self.grid.entity_at(pos) {
                        self.ecs.despawn(entity);
                    }
                    self.grid.clear(pos)?;
                    self.push_event(SimEvent::BlockBrokenEvent(BlockBrokenEvent {
                        x,
                        y,
                        block: current,
                    }));
                } else {
                    log::debug!("break command rejected at ({x}, {y}): tile is empty");
                }
            }
        }
        self.commands_applied = self.commands_applied.wrapping_add(1);
        Ok(())
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
        self.schedule.run(&mut self.ecs.0);
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

    /// Monotonic update counter (`GameState.updateId`).
    pub fn update_id(&self) -> u64 {
        self.state.update_id
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
        let mut counts = std::collections::BTreeMap::new();
        counts.insert("all", all);
        counts.insert("unit", 0);
        counts.insert("build", build);
        counts.insert("bullet", 0);
        counts.insert("player", 0);
        counts.insert("effect", 0);
        counts.insert("weather", 0);
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
                self.ensure_unit_commands()
                    .apply_unit_command(units, *unit_command, *x, *y);
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::UnitControl { unit } => {
                self.controlled_unit = *unit;
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::UnitClear => {
                self.controlled_unit = None;
                self.commands_applied = self.commands_applied.wrapping_add(1);
                return Ok(());
            }
            SimCommand::BuildingControlSelect { x, y } => {
                self.control_building = Some(TilePos::new(*x, *y));
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
            let record = dump::block_event_record(self, &event);
            if let Some(record) = record {
                self.event_log.push(record);
            }
            self.events.dispatch(event);
        }
    }
}

/// Entity kind byte used by the checksum stream (`0 = building`).
pub const KIND_BUILDING: u8 = 0;

#[cfg(test)]
mod tests;
