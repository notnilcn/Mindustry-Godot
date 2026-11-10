// SPDX-License-Identifier: GPL-3.0-only

//! `Sim` construction: `new`, `from_scenario` and the building-sequence allocator.
//!
//! Upstream: core/src/mindustry/core/Logic.java.

use crate::sim::*;

impl Sim {
    /// Creates a flat world in the `Playing` phase.
    pub fn new(seed: u64, width: i32, height: i32, floor: BlockId, wall: BlockId) -> Self {
        let mut grid = WorldGrid::new(width, height);
        grid.fill(floor, wall);
        let mut sim = Self {
            ecs: MindWorld::new(),
            schedule: crate::sim::schedule::build_sim_schedule(),
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
            loaded_unit_entities: Vec::new(),
            hint_events: HintEvents::default(),
            seed,
            map_gen_seed: None,
            map_name: String::new(),
            // Upstream `player.block == null` at boot: no placement block is
            // selected until the player picks one (the command-mode gate
            // requires it; PAR-0140).
            selected_block: BlockId::AIR,
            entity_seq: EntitySequencer::default(),
            commands_applied: 0,
            logic_ui_events: Vec::new(),
            logic_event_log: std::collections::VecDeque::new(),
            logic_bullet_spawns: Vec::new(),
            logic_sync_events: Vec::new(),
            logic_world_changed: false,
            pending_events: Vec::new(),
            event_log: Vec::new(),
            last_config_tap: None,
        };
        // GAPS #61: consume the ability queues in the same entity pass that
        // fills them. `build_sim_schedule` is shared with the P0 trace golden and
        // only names the sets, so the extra system is registered per instance
        // (it no-ops without a live `UnitRuntime`/queues and never reorders a
        // named set).
        sim.schedule.add_systems(
            drain_ability_queues_system
                .in_set(crate::sim::schedule::EntitySet::UpdateUnits)
                .after(unit_runtime::unit_update_system),
        );
        sim
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

    /// Raises the P0 building sequence allocator so its next value is at least
    /// `next` (`EntityGroup.checkNextId`; I01 T10 restores saved unit ids and
    /// the whole `EntitySeq` space must stay collision-free).
    pub(crate) fn bump_entity_seq(&mut self, next: u64) {
        self.entity_seq.bump_to(next);
    }
}
