// SPDX-License-Identifier: GPL-3.0-only

//! The P0 simulation spine: fixed-step tick, command application, checksum and dump.
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`update`, `updateEntities`)
//! and `GameState`; the reserved slot order lives in [`crate::schedule`].

pub mod accelerator;
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

mod query;
mod spine;
#[cfg(test)]
mod spine_tests;

use bevy_ecs::entity::Entity;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
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

pub use spine::KIND_BUILDING;
pub(crate) use spine::{drain_ability_queues_system, spawn_logic_bullet};

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

/// Result kind of an applied `SimCommand::ConfigTap`
/// (`Building.onConfigureBuildTapped`; H02 T6, PAR-0220).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigTapKind {
    /// Java `return false` without `deselect()`: the panel stays on the
    /// selected building.
    Consume,
    /// Java `deselect(); return false`: the panel closes.
    Deselect,
    /// Java `return true`: the panel switches to the tapped building.
    Switch,
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

    /// Bullets created by the world-processor `bullet` instruction, drained by
    /// the sim each tick (`SpawnBulletI` consumer half; the live spawn itself
    /// runs synchronously in [`crate::logic::world::run_spawn_bullet`] so the
    /// instruction result is set). Hosts/tests pop it with
    /// [`Sim::take_logic_bullet_spawns`].
    pub logic_bullet_spawns: Vec<(crate::content::BulletId, Entity)>,

    /// `SyncI` records drained this tick (`Call.syncVariable` network payload;
    /// the local half applies the value to the owning executor's variable, the
    /// server-side `LExecutor.syncVariable` write). Hosts/tests pop it with
    /// [`Sim::take_logic_sync_events`].
    pub logic_sync_events: Vec<crate::logic::world::LogicSyncEvent>,

    /// Set when a world-processor `setblock` mutated tiles (H06 T8): the host
    /// frame pump marks the renderer dirty and clears it.
    pub logic_world_changed: bool,

    /// Outcome of the last applied [`crate::determinism::SimCommand::ConfigTap`]
    /// (H02 T6, PAR-0220): `None` until one is applied. Bytes 0-2 of the
    /// `Building.onConfigureBuildTapped` return contract.
    pub last_config_tap: Option<ConfigTapKind>,

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
    /// Unit entities the last `apply_decoded_entities` restored (H04 T13).
    /// `reset_loaded_controllers` re-derives their controllers with Java's
    /// `UnitComp.afterRead` semantics once the session rules are installed.
    pub(crate) loaded_unit_entities: Vec<Entity>,
    /// `HintsFragment` tick inputs (placed blocks + fired events).
    pub hint_events: HintEvents,

    seed: u64,
    /// Generation seed of the last map load's `World.FilterContext` step
    /// ([`Sim::map_gen_seed`]).
    map_gen_seed: Option<u64>,
    /// `state.map.name()` (`GameState.map`, `Map.name()` = the `name` tag):
    /// the loaded map, the sector (`World.setSectorRules`) or the save's
    /// `mapname` (`SaveVersion.readMeta`). Empty = `Vars.emptyMap` (L02 T4).
    map_name: String,
    selected_block: BlockId,
    entity_seq: EntitySequencer,
    commands_applied: u64,
    pending_events: Vec<SimEvent>,
    event_log: Vec<dump::DumpEvent>,
}
#[cfg(test)]
mod combat_tests;
#[cfg(test)]
mod logic_campaign_tests;
#[cfg(test)]
mod production_tests;
#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
