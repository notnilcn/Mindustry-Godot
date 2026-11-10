// SPDX-License-Identifier: GPL-3.0-only

//! Team/placement predicates and the live-runtime place/break entry points.
//!
//! Upstream: core/src/mindustry/world/Build.java.

use crate::sim::*;

impl Sim {
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

    /// `PlayerComp.team(team)` for a relay-assigned team (PAR-0600).
    ///
    /// Upstream `PlayerComp.team(team)` (`PlayerComp.java:266-271`) sets the
    /// player's team and then `if(unit != null) unit.team(team)`. The editor
    /// path (`HudFragment.setPlayerTeamEditor`, `HudFragment.java:714-718`) is
    /// gated on `state.isEditor()`; a relay-assigned team must land in a live
    /// match as well (module-side PvP balancer, GAPS #33 / PAR-0545), so this
    /// setter skips the editor gate. The rules `defaultTeam` mirror that
    /// [`Self::live_default_team`] reads for placement is the host's job
    /// (`gdext/sim_host.rs::set_player_team_from_relay`).
    pub fn set_relay_player_team(&mut self, team: u8) {
        // `if(unit != null) unit.team(team)` (`PlayerComp.java:269`).
        if let Some(unit) = self.player_unit()
            && let Some(mut comp) = self.ecs.0.get_mut::<crate::entities::comp::TeamComp>(unit)
        {
            comp.team = team;
        }
        // The `PlayerComp.team` field: respawn/retry-spawn team and the
        // `Player.team()` read model (`unit_runtime.rs::set_player_team`).
        self.set_player_team(team);
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

    pub(in crate::sim) fn runtime_place(
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

    /// Whether `entity` is a core of the player's team — the
    /// `CoreBlock.canControlSelect` team gate (I01 T12).
    pub(in crate::sim) fn is_own_core(&self, entity: Entity) -> bool {
        let Some(team) = self.player_team else {
            return false;
        };
        if self
            .ecs
            .0
            .get::<crate::entities::comp::TeamComp>(entity)
            .map(|comp| comp.team)
            != Some(team)
        {
            return false;
        }
        self.ecs
            .0
            .get::<crate::entities::comp::Building>(entity)
            .map(|building| building.block)
            .is_some_and(|block| {
                self.block_runtime
                    .as_ref()
                    .and_then(|runtime| runtime.content().block(block))
                    .is_some_and(|def| def.kind == crate::content::BlockKind::CoreBlock)
            })
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
    pub(in crate::sim) fn runtime_break(&mut self, x: i16, y: i16) -> bool {
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
}
