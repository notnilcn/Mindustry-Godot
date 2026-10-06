// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TickSet` / `EntitySet` — the exact `Logic.update()` + `updateEntities()`
//! schedule (plan 05 §3.4).
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`update` ~`:503`,
//! `updateEntities` ~`:464`). The order is frozen; later plans register systems
//! into these named sets and never re-order them.

use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SingleThreadedExecutor, SystemSet};
use bevy_ecs::world::World;

/// Frame-level `Logic.update()` sets (plan 05 §3.4 table).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TickSet {
    /// `PerfCounter.frame` / tracing span.
    Frame,
    /// `Events.fire(Trigger.update)` — always.
    TriggerUpdate,
    /// `universe.updateGlobal()` (plan 12).
    UniverseGlobal,
    /// Modified-settings flush (platform).
    SettingsFlush,
    /// `state.enemies = Groups.unit.count(...)` — `!client`.
    EnemyCount,
    /// `Events.fire(Trigger.beforeGameUpdate)` — `is_game && !paused`.
    BeforeGameUpdate,
    /// `state.tick += ...`, `state.updateId++`.
    StateClock,
    /// `state.teams.updateTeamStats()` (plan 12).
    TeamStats,
    /// `MapPreviewLoader.checkPreviews()` (plan 16).
    PreviewCheck,
    /// `fogControl.update()` (plan 12).
    Fog,
    /// Sector info / `universe.update()` (plan 12).
    Campaign,
    /// `Time.update()`.
    TimeRuns,
    /// `logicVars.update()` (plan 13).
    LogicVars,
    /// Weather + per-team AI — `!client && !editor`.
    WeatherAndAi,
    /// `objectives.update()` — `!editor`.
    Objectives,
    /// Wave timer — `rules.waves`.
    WaveTimer,
    /// `runWave()` — `rules.waves` and due.
    RunWave,
    /// `state.envAttrs` rebuild.
    EnvAttrs,
    /// `updateEntities()` (nested [`EntitySet`] chain).
    EntityUpdate,
    /// `Events.fire(Trigger.afterGameUpdate)`.
    AfterGameUpdate,
    /// `checkGameState()`.
    GameStateCheck,
    /// Event-bus listener drain.
    DrainEvents,
    /// `Core.app.post` flush.
    PostQueue,
}

/// Every [`TickSet`] in execution order.
pub static TICK_SETS: [TickSet; 23] = [
    TickSet::Frame,
    TickSet::TriggerUpdate,
    TickSet::UniverseGlobal,
    TickSet::SettingsFlush,
    TickSet::EnemyCount,
    TickSet::BeforeGameUpdate,
    TickSet::StateClock,
    TickSet::TeamStats,
    TickSet::PreviewCheck,
    TickSet::Fog,
    TickSet::Campaign,
    TickSet::TimeRuns,
    TickSet::LogicVars,
    TickSet::WeatherAndAi,
    TickSet::Objectives,
    TickSet::WaveTimer,
    TickSet::RunWave,
    TickSet::EnvAttrs,
    TickSet::EntityUpdate,
    TickSet::AfterGameUpdate,
    TickSet::GameStateCheck,
    TickSet::DrainEvents,
    TickSet::PostQueue,
];

impl TickSet {
    /// Parity name used by the trace golden.
    pub const fn name(self) -> &'static str {
        match self {
            TickSet::Frame => "Frame",
            TickSet::TriggerUpdate => "TriggerUpdate",
            TickSet::UniverseGlobal => "UniverseGlobal",
            TickSet::SettingsFlush => "SettingsFlush",
            TickSet::EnemyCount => "EnemyCount",
            TickSet::BeforeGameUpdate => "BeforeGameUpdate",
            TickSet::StateClock => "StateClock",
            TickSet::TeamStats => "TeamStats",
            TickSet::PreviewCheck => "PreviewCheck",
            TickSet::Fog => "Fog",
            TickSet::Campaign => "Campaign",
            TickSet::TimeRuns => "TimeRuns",
            TickSet::LogicVars => "LogicVars",
            TickSet::WeatherAndAi => "WeatherAndAi",
            TickSet::Objectives => "Objectives",
            TickSet::WaveTimer => "WaveTimer",
            TickSet::RunWave => "RunWave",
            TickSet::EnvAttrs => "EnvAttrs",
            TickSet::EntityUpdate => "EntityUpdate",
            TickSet::AfterGameUpdate => "AfterGameUpdate",
            TickSet::GameStateCheck => "GameStateCheck",
            TickSet::DrainEvents => "DrainEvents",
            TickSet::PostQueue => "PostQueue",
        }
    }
}

/// Nested `Logic.updateEntities()` sets (`EntityProcess`/`GroupDefs` order).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntitySet {
    /// `Groups.updatePooling()`.
    PoolCleanup,
    /// `Groups.bullet.updatePhysics()`.
    PhysicsBullets,
    /// `Groups.unit.updatePhysics()`.
    PhysicsUnits,
    /// `Groups.player.update()`.
    UpdatePlayers,
    /// `Groups.effect.update()`.
    UpdateEffects,
    /// `Groups.all.update()` — `!editor`.
    UpdateAll,
    /// `Groups.unit.update()` — `!editor`.
    UpdateUnits,
    /// Editor-filtered `Groups.unit.update()` — `editor`.
    UpdateUnitsEditor,
    /// `Groups.powerGraph.update()` — `!editor`.
    UpdatePowerGraph,
    /// `Groups.build.update()` — `!editor`.
    UpdateBuildings,
    /// `Groups.bullet.update()` — `!editor`.
    UpdateBullets,
    /// `Groups.bullet.collide()` — `!editor`.
    CollideBullets,
}

/// Every [`EntitySet`] in execution order.
pub static ENTITY_SETS: [EntitySet; 12] = [
    EntitySet::PoolCleanup,
    EntitySet::PhysicsBullets,
    EntitySet::PhysicsUnits,
    EntitySet::UpdatePlayers,
    EntitySet::UpdateEffects,
    EntitySet::UpdateAll,
    EntitySet::UpdateUnits,
    EntitySet::UpdateUnitsEditor,
    EntitySet::UpdatePowerGraph,
    EntitySet::UpdateBuildings,
    EntitySet::UpdateBullets,
    EntitySet::CollideBullets,
];

impl EntitySet {
    /// Parity name used by the trace golden.
    pub const fn name(self) -> &'static str {
        match self {
            EntitySet::PoolCleanup => "PoolCleanup",
            EntitySet::PhysicsBullets => "PhysicsBullets",
            EntitySet::PhysicsUnits => "PhysicsUnits",
            EntitySet::UpdatePlayers => "UpdatePlayers",
            EntitySet::UpdateEffects => "UpdateEffects",
            EntitySet::UpdateAll => "UpdateAll",
            EntitySet::UpdateUnits => "UpdateUnits",
            EntitySet::UpdateUnitsEditor => "UpdateUnitsEditor",
            EntitySet::UpdatePowerGraph => "UpdatePowerGraph",
            EntitySet::UpdateBuildings => "UpdateBuildings",
            EntitySet::UpdateBullets => "UpdateBullets",
            EntitySet::CollideBullets => "CollideBullets",
        }
    }
}

/// Run-condition inputs used to decide whether a set executes this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunContext {
    /// A world is loaded (`State.is_game`).
    pub is_game: bool,
    /// The fixed step is paused.
    pub paused: bool,
    /// The editor is active.
    pub editor: bool,
    /// Running as a client (`!client` gates server-only sets).
    pub is_client: bool,
    /// `Rules.waves` is enabled.
    pub waves: bool,
    /// The wave timer is due.
    pub wave_due: bool,
}

impl RunContext {
    /// A headless, playing, non-editor match with waves.
    pub const fn playing_headless() -> Self {
        Self {
            is_game: true,
            paused: false,
            editor: false,
            is_client: false,
            waves: true,
            wave_due: true,
        }
    }

    /// The menu (no world).
    pub const fn menu() -> Self {
        Self {
            is_game: false,
            ..Self::playing_headless()
        }
    }

    /// A paused game.
    pub const fn paused() -> Self {
        Self {
            paused: true,
            ..Self::playing_headless()
        }
    }

    /// The editor.
    pub const fn editor() -> Self {
        Self {
            editor: true,
            ..Self::playing_headless()
        }
    }

    /// A client (server-only sets skipped).
    pub const fn client() -> Self {
        Self {
            is_client: true,
            ..Self::playing_headless()
        }
    }

    fn active(&self) -> bool {
        self.is_game && !self.paused
    }
}

/// One trace row: whether a set ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraceEntry {
    /// Set name.
    pub set: &'static str,
    /// Whether it ran.
    pub ran: bool,
}

/// Computes the deterministic execution trace for a run context (plan 05 §7.2).
///
/// `TickSet::EntityUpdate` expands into the nested [`EntitySet`] rows so the
/// golden records the true `updateEntities()` order.
pub fn schedule_trace(ctx: RunContext) -> Vec<TraceEntry> {
    let mut rows = Vec::new();
    for set in TICK_SETS {
        let ran = match set {
            TickSet::Frame | TickSet::TriggerUpdate => true,
            TickSet::UniverseGlobal => ctx.is_game,
            TickSet::SettingsFlush => true,
            TickSet::EnemyCount => !ctx.is_client,
            TickSet::BeforeGameUpdate => ctx.active(),
            TickSet::StateClock => ctx.active(),
            TickSet::TeamStats => ctx.active(),
            TickSet::PreviewCheck => ctx.active(),
            TickSet::Fog => ctx.active(),
            TickSet::Campaign => ctx.active(),
            TickSet::TimeRuns => ctx.active(),
            TickSet::LogicVars => ctx.active(),
            TickSet::WeatherAndAi => !ctx.is_client && !ctx.editor,
            TickSet::Objectives => !ctx.editor,
            TickSet::WaveTimer => ctx.waves && ctx.active(),
            TickSet::RunWave => ctx.waves && ctx.wave_due && ctx.active(),
            TickSet::EnvAttrs => ctx.is_game,
            TickSet::EntityUpdate => ctx.active(),
            TickSet::AfterGameUpdate => ctx.active(),
            TickSet::GameStateCheck => ctx.is_game,
            TickSet::DrainEvents => true,
            TickSet::PostQueue => true,
        };
        if set == TickSet::EntityUpdate {
            rows.push(TraceEntry {
                set: set.name(),
                ran,
            });
            for entity_set in ENTITY_SETS {
                let entity_ran = ran
                    && match entity_set {
                        EntitySet::UpdateAll
                        | EntitySet::UpdatePowerGraph
                        | EntitySet::UpdateBuildings
                        | EntitySet::UpdateBullets
                        | EntitySet::CollideBullets => !ctx.editor,
                        EntitySet::UpdateUnits => !ctx.editor,
                        EntitySet::UpdateUnitsEditor => ctx.editor,
                        _ => true,
                    };
                rows.push(TraceEntry {
                    set: entity_set.name(),
                    ran: entity_ran,
                });
            }
        } else {
            rows.push(TraceEntry {
                set: set.name(),
                ran,
            });
        }
    }
    rows
}

/// Renders the trace for several contexts as the golden text file.
pub fn render_trace(contexts: &[(&str, RunContext)]) -> String {
    let mut out = String::new();
    for (label, ctx) in contexts {
        out.push_str(&format!("[{label}]\n"));
        for entry in schedule_trace(*ctx) {
            out.push_str(&format!(
                "{} {}\n",
                if entry.ran { "run " } else { "skip" },
                entry.set
            ));
        }
    }
    out
}

/// Builds the full ordered schedule: `TickSet` chain with the nested
/// `EntitySet` chain under `EntityUpdate`, and no-op stub systems for every
/// slot not yet implemented by a later plan.
pub fn build_sim_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    schedule.set_executor(SingleThreadedExecutor::new());
    // Bevy tuple impls top out at 20, so the frame chain is configured in two
    // linked groups; the second follows the first via `.after`.
    schedule.configure_sets(
        (
            TickSet::Frame,
            TickSet::TriggerUpdate,
            TickSet::UniverseGlobal,
            TickSet::SettingsFlush,
            TickSet::EnemyCount,
            TickSet::BeforeGameUpdate,
            TickSet::StateClock,
            TickSet::TeamStats,
            TickSet::PreviewCheck,
            TickSet::Fog,
            TickSet::Campaign,
            TickSet::TimeRuns,
        )
            .chain(),
    );
    schedule.configure_sets(
        (
            TickSet::LogicVars,
            TickSet::WeatherAndAi,
            TickSet::Objectives,
            TickSet::WaveTimer,
            TickSet::RunWave,
            TickSet::EnvAttrs,
            TickSet::EntityUpdate,
            TickSet::AfterGameUpdate,
            TickSet::GameStateCheck,
            TickSet::DrainEvents,
            TickSet::PostQueue,
        )
            .chain()
            .after(TickSet::TimeRuns),
    );
    schedule.configure_sets(
        (
            EntitySet::PoolCleanup,
            EntitySet::PhysicsBullets,
            EntitySet::PhysicsUnits,
            EntitySet::UpdatePlayers,
            EntitySet::UpdateEffects,
            EntitySet::UpdateAll,
            EntitySet::UpdateUnits,
            EntitySet::UpdateUnitsEditor,
            EntitySet::UpdatePowerGraph,
            EntitySet::UpdateBuildings,
            EntitySet::UpdateBullets,
            EntitySet::CollideBullets,
        )
            .chain()
            .in_set(TickSet::EntityUpdate),
    );
    // One no-op placeholder system so the schedule is runnable from M6 onward;
    // later plans register into the named sets (HLP §2.2 boundary).
    schedule.add_systems(stub_system);
    // Plan 07: building runtime. No-op unless a `world::block::BlockTable`
    // resource is present, so the P0 `Sim` checksum/golden is unchanged.
    schedule.add_systems(crate::world::update::update_buildings.in_set(EntitySet::UpdateBuildings));
    // live runtime wiring (WS1: sim-runtime) — additive only; these systems are
    // no-ops unless the opt-in `Sim` runtime installed its resources
    // (`BlockTable`/`PowerGrids`), so P0 scenario sims and goldens stay
    // byte-identical. Never reorder the sets above.
    schedule
        .add_systems(crate::world::update::update_power_graphs.in_set(EntitySet::UpdatePowerGraph));
    // end live runtime wiring
    schedule
}

/// A no-op placeholder (`// plan NN` fills it).
fn stub_system() {}

/// Runs the schedule once against a world (helper for tests/harness).
pub fn run_once(schedule: &mut Schedule, world: &mut World) {
    schedule.run(world);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_schedule_builds_and_runs() {
        let mut schedule = build_sim_schedule();
        let mut world = World::new();
        run_once(&mut schedule, &mut world);
    }

    #[test]
    fn tick_set_order_is_frozen() {
        let names: Vec<&str> = TICK_SETS.iter().map(|set| set.name()).collect();
        assert_eq!(names.first(), Some(&"Frame"));
        assert_eq!(names.last(), Some(&"PostQueue"));
        assert_eq!(TICK_SETS.len(), 23);
        assert_eq!(ENTITY_SETS.len(), 12);
        // EntityUpdate expands to 12 nested rows.
        let trace = schedule_trace(RunContext::playing_headless());
        assert_eq!(trace.len(), 22 + 1 + 12);
    }

    #[test]
    fn run_conditions_skip_menu_and_editor() {
        let menu = schedule_trace(RunContext::menu());
        let before = menu
            .iter()
            .find(|entry| entry.set == "BeforeGameUpdate")
            .copied();
        assert_eq!(before.map(|entry| entry.ran), Some(false));
        let editor = schedule_trace(RunContext::editor());
        let update_all = editor
            .iter()
            .find(|entry| entry.set == "UpdateAll")
            .copied();
        assert_eq!(update_all.map(|entry| entry.ran), Some(false));
        let units_editor = editor
            .iter()
            .find(|entry| entry.set == "UpdateUnitsEditor")
            .copied();
        assert_eq!(units_editor.map(|entry| entry.ran), Some(true));
        let client = schedule_trace(RunContext::client());
        let enemy = client
            .iter()
            .find(|entry| entry.set == "EnemyCount")
            .copied();
        assert_eq!(enemy.map(|entry| entry.ran), Some(false));
    }

    #[test]
    fn render_trace_is_stable() {
        let first = render_trace(&[
            ("playing", RunContext::playing_headless()),
            ("menu", RunContext::menu()),
            ("paused", RunContext::paused()),
            ("editor", RunContext::editor()),
        ]);
        let second = render_trace(&[
            ("playing", RunContext::playing_headless()),
            ("menu", RunContext::menu()),
            ("paused", RunContext::paused()),
            ("editor", RunContext::editor()),
        ]);
        assert_eq!(first, second);
        assert!(first.contains("run  UpdateBuildings"));
        assert!(first.contains("skip UpdateBuildings"));
    }
}
