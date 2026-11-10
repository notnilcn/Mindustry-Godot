// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TickSet::Objectives`: the live `MapObjectives` executor and the input-side
//! command-mode fact it reads.
//! Upstream: core/src/mindustry/game/MapObjectives.java

use super::*;
use crate::content::ItemId;
use crate::ecs::TeamId;
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::entities::comp::{Building, TeamComp};
use crate::game::State;
use crate::game::map_objectives::{ObjectiveEnv, ObjectiveRunParams};
use crate::game::stats::GameStats;
use crate::game::teams::Teams;
use crate::sim::runtime::ContentIndex;
use crate::world::block::BlockTable;

/// `TickSet::Objectives`: `state.objectives.update()` over the live session.
pub fn objectives_tick_system(world: &mut World) {
    let Some(mut runtime) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    if runtime.session.phase != State::Playing || runtime.is_client {
        world.insert_resource(runtime);
        return;
    }
    let params = ObjectiveRunParams {
        default_team: runtime.session.default_team(),
        wave_team: runtime.session.wave_team(),
        timer_multiplier: runtime.session.rules.objective_timer_multiplier,
    };
    let rules_snapshot = runtime.session.rules.clone();
    let command_mode = world
        .get_resource::<ObjectiveInputState>()
        .is_some_and(|state| state.command_mode_satisfied);
    let completed = {
        let env = RuntimeObjectiveEnv {
            rules: &rules_snapshot,
            teams: &runtime.session.teams,
            unlocked: &runtime.unlocked_content,
            stats: &runtime.session.stats,
            world,
            index: world.get_resource::<ContentIndex>(),
            command_mode_satisfied: command_mode,
        };
        // `Time.delta` is one per 60 Hz tick; `TimerObjective.duration` is in
        // ticks (`MapObjectives.java:516-540`).
        runtime.session.objectives.update(&env, &params, 1.0)
    };
    let completed_any = !completed.is_empty();
    for index in completed {
        runtime
            .session
            .objectives
            .complete(index, &mut runtime.session.rules);
    }
    // Persist the transient Timer `countup`/`completed` state back into the
    // rules so saves carry it (Java serializes both). The full clone runs when
    // something completed or once a second; a save between two copies loses
    // <1 s of a timer.
    runtime.session.objectives.sync_timers_into_data();
    if completed_any || runtime.ticks % 60 == 0 {
        runtime.session.rules.objectives = runtime.session.objectives.data().clone();
    }
    world.insert_resource(runtime);
}

/// Input-side facts the objective executor needs (`CommandModeObjective`,
/// `MapObjectives.java:671-687`: `control.input.selectedUnits.contains(u ->
/// u.isCommandable() && u.command().hasCommand())`).
#[derive(bevy_ecs::prelude::Resource, Debug, Clone, Copy, Default)]
pub struct ObjectiveInputState {
    /// Whether the current selection satisfies the command-mode objective.
    pub command_mode_satisfied: bool,
}

impl crate::sim::Sim {
    /// Setter for the input layer (G08): call every frame with the current
    /// selection state; inserts/updates the [`ObjectiveInputState`] resource.
    pub fn set_command_mode_satisfied(&mut self, value: bool) {
        self.ecs.0.insert_resource(ObjectiveInputState {
            command_mode_satisfied: value,
        });
    }
}

/// Live-world objective environment: rules flags/`researched`, the session
/// team registry, the live match stats and the ECS-backed item/block counts.
pub(super) struct RuntimeObjectiveEnv<'a> {
    pub(super) rules: &'a Rules,
    pub(super) teams: &'a Teams,
    pub(super) unlocked: &'a std::collections::HashSet<String>,
    pub(super) stats: &'a GameStats,
    pub(super) world: &'a World,
    pub(super) index: Option<&'a ContentIndex>,
    /// `control.input.selectedUnits` command-mode fact (`ObjectiveInputState`).
    pub(super) command_mode_satisfied: bool,
}

impl ObjectiveEnv for RuntimeObjectiveEnv<'_> {
    fn is_content_unlocked(&self, content: &str) -> bool {
        self.rules.researched.iter().any(|name| name == content) || self.unlocked.contains(content)
    }

    /// `Team.items()`: the team's shared core pool (`ItemObjective`).
    ///
    /// Java's cores alias one `ItemModule`, so the count is the team pool —
    /// the canonical core of the team (`mc/world/blocks/storage/team_items`),
    /// with an unaliased core counting its own module. This is the same
    /// resolution `core_snapshot_of` sums for `SectorInfo.prepare`, so the
    /// sector record and the objective agree.
    fn team_has_item(&self, team: u8, item: &str, amount: i32) -> bool {
        if amount <= 0 {
            return true;
        }
        let Some(&item_id) = self.index.and_then(|index| index.item_ids.get(item)) else {
            return false;
        };
        crate::world::blocks::storage::team_items::team_item_count(
            self.world,
            team,
            ItemId::new(item_id),
        ) >= amount
    }

    fn core_item_count(&self, item: &str) -> i32 {
        self.stats.core_item_count.get(item).copied().unwrap_or(0)
    }

    /// `state.stats.placedBlockCount.get(block, 0)` (`BuildCount`): bumped by
    /// the placement-event listener on [`super::super::sim::Sim`].
    fn placed_block_count(&self, block: &str) -> i32 {
        self.stats
            .placed_block_count
            .get(block)
            .copied()
            .unwrap_or(0)
    }

    /// `state.rules.defaultTeam.data().countType(unit)` (`UnitCountObjective`,
    /// `MapObjectives.java:470`): live units of `team` and `unit` whose
    /// `UnitCore.dead` is false (`Teams.countType` counts live units).
    fn unit_count(&self, team: u8, unit: &str) -> i32 {
        let Some(index) = self.index else {
            return 0;
        };
        let Some(&unit_id) = index.unit_ids.get(unit) else {
            return 0;
        };
        let mut count = 0;
        for entity_ref in self.world.iter_entities() {
            if entity_ref
                .get::<UnitTypeComp>()
                .is_none_or(|comp| comp.type_id.raw() != unit_id)
            {
                continue;
            }
            if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
                continue;
            }
            if entity_ref.get::<UnitCore>().is_some_and(|core| core.dead) {
                continue;
            }
            count += 1;
        }
        count
    }

    /// `state.stats.enemyUnitsDestroyed` (`Logic` `UnitDestroyEvent`,
    /// `Logic.java:218-226`).
    fn enemy_units_destroyed(&self) -> i32 {
        self.stats.enemy_units_destroyed
    }

    fn objective_flag(&self, flag: &str) -> bool {
        self.rules.objective_flags.contains(flag)
    }

    fn core_count(&self, team: u8) -> usize {
        self.teams
            .get_or_null(TeamId(team))
            .map_or(0, |data| data.cores.len())
    }

    /// `world.build(x, y)`: the building whose footprint covers the tile
    /// (`MapObjectives.java:604`; `World.build`).
    fn block_at(&self, x: i32, y: i32) -> Option<(String, u8)> {
        let table = self.world.get_resource::<BlockTable>()?;
        for entity_ref in self.world.iter_entities() {
            let Some(building) = entity_ref.get::<Building>() else {
                continue;
            };
            let Some(team) = entity_ref.get::<TeamComp>() else {
                continue;
            };
            let Some(inst) = table.instance(building.block) else {
                continue;
            };
            let size = inst.def.size.max(1);
            let off = (size - 1) / 2;
            let bx = i32::from(building.tile.x()) - off;
            let by = i32::from(building.tile.y()) - off;
            if x >= bx && x < bx + size && y >= by && y < by + size {
                return Some((inst.name.clone(), team.team));
            }
        }
        None
    }

    fn headless(&self) -> bool {
        false
    }

    /// The `MindInput` command-mode selection fact (`ObjectiveInputState`,
    /// set by G08 through `Sim::set_command_mode_satisfied`).
    fn command_mode_satisfied(&self) -> bool {
        self.command_mode_satisfied
    }
}
