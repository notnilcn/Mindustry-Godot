// SPDX-License-Identifier: GPL-3.0-only

//! Plain `Sim` accessors: snapshot/dump, phase/pause/selection, content and the
//! relay-runtime views.
//!
//! Upstream: none (port-only accessors).

use crate::sim::*;

impl Sim {
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

    /// The recorded generation seed of the last map load's `FilterContext` step
    /// (`World.loadMap`); `None` until a map load runs filters or after a save
    /// load. Surfaced in the state dump (`map_gen_seed`).
    pub fn map_gen_seed(&self) -> Option<u64> {
        self.map_gen_seed
    }

    /// Records the generation seed of the last map load (host half of
    /// [`crate::io::sim_io::decode_map_bytes`]).
    pub fn set_map_gen_seed(&mut self, seed: u64) {
        self.map_gen_seed = Some(seed);
    }

    /// `state.map.name()` as written into save meta (`SaveVersion.java:133`):
    /// the recorded map name, else `Map.tag`'s `"unknown"` fallback
    /// (`Map.java:151-153`, bundle `unknown` = "Unknown") for `emptyMap`.
    pub fn map_name(&self) -> &str {
        if self.map_name.is_empty() {
            "Unknown"
        } else {
            &self.map_name
        }
    }

    /// Records `state.map` for the world just loaded (host map/sector loads,
    /// save loads); see [`Sim::map_name`].
    pub fn set_map_name(&mut self, name: impl Into<String>) {
        self.map_name = name.into();
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
        self.with_logic_player_view(|view| view.selected_block = Some(block));
        Ok(())
    }

    /// `player.selectedRotation` (`LAccess::SelectedRotation` sense source).
    pub fn set_selected_rotation(&mut self, rotation: u8) {
        self.with_logic_player_view(|view| view.selected_rotation = rotation.rem_euclid(4));
    }

    /// Replaces the `LAccess` player view (camera/ping/selection state;
    /// [`crate::logic::globals::LogicPlayerView`]).
    ///
    /// The gdext host owns the real camera and input selection state, so it
    /// pushes this each frame; `set_selected_block` keeps the placement half
    /// fresh on its own.
    pub fn set_logic_player_view(&mut self, view: crate::logic::globals::LogicPlayerView) {
        self.ecs.0.insert_resource(view);
    }

    /// Mutable access to the `LAccess` player view, inserting a default first.
    pub fn with_logic_player_view<R>(
        &mut self,
        f: impl FnOnce(&mut crate::logic::globals::LogicPlayerView) -> R,
    ) -> R {
        let mut view = self
            .ecs
            .0
            .get_resource_or_insert_with(crate::logic::globals::LogicPlayerView::default);
        f(&mut view)
    }

    /// Wall/block at a tile.
    pub fn block_at(&self, pos: TilePos) -> Option<BlockId> {
        self.grid.block_at(pos)
    }

    /// Content registry.
    pub fn content(&self) -> &Blocks {
        &self.content
    }

    /// Extends the sim's block name/id table with blocks the base table lacks
    /// (mod content, I07 T4). The registry appends mod blocks after the vanilla
    /// ones, so registering in registry order keeps ids identical to
    /// `ContentRegistry` ids (the parity ABI).
    pub fn sync_blocks_from_content(
        &mut self,
        registry: &crate::content::ContentRegistry,
    ) -> Result<(), ContentError> {
        for def in registry.blocks() {
            if !self.content.contains(&def.name) {
                self.content.register(&def.name)?;
            }
        }
        Ok(())
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
    pub(in crate::sim) fn ensure_unit_commands(&mut self) -> &mut command::UnitCommandRuntime {
        self.unit_commands
            .get_or_insert_with(command::UnitCommandRuntime::new)
    }

    /// Lazily builds the relay world-mutation runtime (plan 15 §3.3.1).
    pub(in crate::sim) fn ensure_world_apply(&mut self) -> &mut world_apply::WorldApplyRuntime {
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

    pub(in crate::sim) fn push_event(&mut self, event: SimEvent) {
        self.pending_events.push(event);
    }
}
