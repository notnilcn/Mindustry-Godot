// SPDX-License-Identifier: GPL-3.0-only

//! The mlog world-processor bridge: globals, event drains, sync, `setblock` and
//! presentation state.
//!
//! Upstream: core/src/mindustry/logic/LExecutor.java,
//! core/src/mindustry/core/NetServer.java.

use crate::sim::*;

impl Sim {
    /// `Logic.update` -> `logicVars.update()` (Logic.java:546), before entity
    /// updates: processors observe the already-incremented tick.
    pub(in crate::sim) fn update_logic_globals(&mut self) {
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
            state.player_unit = self.player_unit;
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
        let (client_locale, client_name) = self
            .ecs
            .0
            .get_resource::<crate::logic::globals::LogicClientInfo>()
            .map(|info| (Some(info.locale.clone()), Some(info.name.clone())))
            .unwrap_or_else(|| (Some("en".to_owned()), Some("player".to_owned())));
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
            // `GlobalVars.java:211-216`: `player.locale()` / `player.name()`
            // from the host-injected [`crate::logic::globals::LogicClientInfo`].
            client_locale,
            client_name,
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
    pub(in crate::sim) fn drain_logic_events(&mut self) {
        // `MapObjectives.completed` -> `LExecutor.runLogicScript`
        // (`MapObjectives.java:213`): a privileged, loop-free script over the
        // LIVE world, up to `SCRIPT_DEFAULT_MAX_INSTRUCTIONS`.
        for code in self.take_pending_logic_scripts() {
            let _ = crate::logic::script::run_logic_script_in(
                &code,
                crate::logic::script::SCRIPT_DEFAULT_MAX_INSTRUCTIONS,
                false,
                &mut self.ecs.0,
            );
        }
        self.drain_logic_world_events();
        // A script may emit world events and complete another objective: one
        // more pass (bounded by the objectives themselves having no cycles).
        for code in self.take_pending_logic_scripts() {
            let _ = crate::logic::script::run_logic_script_in(
                &code,
                crate::logic::script::SCRIPT_DEFAULT_MAX_INSTRUCTIONS,
                false,
                &mut self.ecs.0,
            );
        }
        self.drain_logic_world_events();
    }

    /// Takes the queued objective `completionLogicCode` scripts.
    pub(in crate::sim) fn take_pending_logic_scripts(&mut self) -> Vec<String> {
        self.ecs
            .0
            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
            .map(|mut runtime| {
                std::mem::take(&mut runtime.session.objectives.pending_logic_scripts)
            })
            .unwrap_or_default()
    }

    /// Drains the [`crate::logic::world::LogicWorldState`] event log into the
    /// sim's host handlers.
    pub(in crate::sim) fn drain_logic_world_events(&mut self) {
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
                // `SetBlockI` live apply (`LExecutor.java:1566-1612`).
                LogicWorldEvent::SetBlock {
                    layer,
                    block,
                    x,
                    y,
                    team,
                    rotation,
                } => self.logic_set_block(layer, block, x, y, team, rotation),
                // T5 adds SetBlock handling here.
                e @ LogicWorldEvent::Message { .. } if self.logic_ui_events.len() < 256 => {
                    self.logic_ui_events.push(e);
                }
                // `ucontrol payDrop`/`payTake` (T1): applied through the same
                // helpers the player input uses.
                LogicWorldEvent::Payload {
                    carrier,
                    kind,
                    x,
                    y,
                    units: _,
                } => {
                    let carrier = bevy_ecs::entity::Entity::from_bits(carrier);
                    match kind {
                        0 => {
                            let _ = self.payload_pickup_unit(carrier, x, y, None);
                        }
                        1 => {
                            let _ = self.payload_pickup_build(carrier, x, y);
                        }
                        2 => {
                            let _ = self.payload_drop(carrier, x, y);
                        }
                        _ => {}
                    }
                }
                // `SpawnBulletI` host consumer: the live entity was created
                // synchronously in `run_spawn_bullet` (via
                // [`spawn_logic_bullet`]) so the instruction result is set;
                // here the drained record is kept for the host/debug probe.
                LogicWorldEvent::SpawnBullet {
                    def,
                    entity: Some(entity),
                    ..
                } => {
                    self.logic_bullet_spawns.push((def, entity));
                }
                // `SyncI`: `Call.syncVariable(build, id, value)` is a
                // server-target remote that writes the authoritative executor
                // variable (`LExecutor.java:2047-2060`). The local half does
                // that same write (idempotent in single-player, where the
                // variable already holds the value); the record stays in
                // `logic_sync_events` for the (unported) plan-18/21 relay.
                LogicWorldEvent::Sync(event) => {
                    self.apply_logic_sync(&event);
                    self.logic_sync_events.push(*event);
                }
                // `ClientDataI`: `Call.clientLogicData[Un]reliable` is a
                // client-target remote relayed to `NetServer.logicClientData
                // Handlers` (`NetServer.java:698-705`). The port has no relay,
                // so the local half delivers to the registered handler bus;
                // an empty bus is Java's no-handler no-op.
                LogicWorldEvent::ClientData(event) => {
                    if let Some(mut bus) = self
                        .ecs
                        .0
                        .remove_resource::<crate::logic::world::LogicClientDataBus>()
                    {
                        bus.deliver(&event);
                        self.ecs.0.insert_resource(bus);
                    }
                }
                // `ucontrol payEnter` → `Call.unitBuildingControlSelect`
                // (`LExecutor.java:441-446`): the port's `onControlSelect`
                // implementor is `CoreBlock` (respawn the player at the core).
                LogicWorldEvent::ControlSelect { unit, build }
                    if self.player_unit == Some(unit) =>
                {
                    let _ = self.control_select_core(build);
                }
                // `CutsceneI` state lives in `LogicWorldState.cutscene` (T3).
                _ => {}
            }
        }
    }

    /// Applies `SyncI`'s server-target write locally (`LExecutor.syncVariable`,
    /// `LExecutor.java:2047-2060`): the owning processor's executor variable is
    /// set from the synced payload.
    pub(in crate::sim) fn apply_logic_sync(&mut self, event: &crate::logic::world::LogicSyncEvent) {
        let tile = crate::world::WorldGrid::unpack(event.building_pos);
        let Some(entity) = self.grid.entity_at(tile) else {
            return;
        };
        let Some(mut state) = self
            .ecs
            .0
            .get_mut::<crate::logic::blocks::logic_block::LogicBlockState>(entity)
        else {
            return;
        };
        if event.var_id < 0 {
            return;
        }
        let Some(cell) = state.executor.arena.cells.get_mut(event.var_id as usize) else {
            return;
        };
        if cell.constant {
            return;
        }
        if event.is_obj {
            cell.is_obj = true;
            cell.obj = event.obj.clone();
        } else {
            cell.is_obj = false;
            cell.obj = None;
            cell.num = event.num;
        }
    }

    /// Pops the bullets created by the logic `bullet` instruction since the
    /// last call (`SpawnBulletI` host consumer half).
    pub fn take_logic_bullet_spawns(&mut self) -> Vec<(crate::content::BulletId, Entity)> {
        std::mem::take(&mut self.logic_bullet_spawns)
    }

    /// Pops the `sync` records drained since the last call (`SyncI` network
    /// payload; plan-18/21 relay sends these to the host).
    pub fn take_logic_sync_events(&mut self) -> Vec<crate::logic::world::LogicSyncEvent> {
        std::mem::take(&mut self.logic_sync_events)
    }

    /// Monotonic update counter (`GameState.updateId`).
    pub fn update_id(&self) -> u64 {
        self.state.update_id
    }

    /// `SetBlockI` live apply (`LExecutor.java:1566-1612`): the ore/floor/block
    /// branches against the authoritative grid. `Building` is not allowed.
    pub fn logic_set_block(
        &mut self,
        layer: crate::logic::enums::TileLayer,
        block: Option<crate::logic::value::LogicObject>,
        x: i32,
        y: i32,
        team: u8,
        rotation: i32,
    ) {
        use crate::content::ContentType;
        let Some(crate::logic::value::LogicObject::Content(c)) = block else {
            return;
        };
        if c.type_ != ContentType::Block {
            return;
        }
        let block_id = BlockId::new(c.id);
        let (tx, ty) = (x, y);
        if tx < 0 || ty < 0 || tx >= self.grid.width() || ty >= self.grid.height() {
            return;
        }
        // `Block instanceof Floor` / `OverlayFloor` / `b == Blocks.air`.
        let (is_floor, is_overlay, is_air) = self
            .block_runtime
            .as_ref()
            .and_then(|runtime| runtime.content().block(block_id))
            .map(|def| {
                (
                    crate::maps::filters::block_info::is_floor(def),
                    crate::maps::filters::block_info::is_overlay(def),
                    def.kind == crate::content::registries::blocks::BlockKind::AirBlock,
                )
            })
            .unwrap_or((false, false, block_id == BlockId::AIR));
        let mut changed = false;
        match layer {
            crate::logic::enums::TileLayer::Ore => {
                // `(b instanceof OverlayFloor || b == air) && tile.overlay != b`.
                if (is_overlay || is_air) && self.grid.tile(tx, ty).overlay != block_id {
                    self.grid.tiles.get_mut(tx, ty).overlay = block_id;
                    changed = true;
                }
            }
            crate::logic::enums::TileLayer::Floor => {
                // `b instanceof Floor && tile.floor != f && !f.isOverlay()
                // && !f.isAir()`.
                if is_floor && !is_overlay && !is_air && self.grid.tile(tx, ty).floor != block_id {
                    self.grid.tiles.get_mut(tx, ty).floor = block_id;
                    changed = true;
                }
            }
            crate::logic::enums::TileLayer::Block => {
                // `!b.isFloor() || b == air`.
                if !is_floor || is_air {
                    let pos = TilePos::new(tx as i16, ty as i16);
                    let current_block = self.grid.block_at(pos).unwrap_or(BlockId::AIR);
                    let current_team = self
                        .grid
                        .entity_at(pos)
                        .and_then(|e| self.ecs.0.get::<crate::entities::comp::TeamComp>(e))
                        .map(|t| t.team)
                        .unwrap_or(crate::game::team::DERELICT.0);
                    if current_block != block_id || current_team != team {
                        let rot = rotation.clamp(0, 3) as u8;
                        if block_id == BlockId::AIR {
                            if let Some(runtime) = self.block_runtime.as_mut() {
                                runtime.remove_block_raw(
                                    &mut self.ecs.0,
                                    &mut self.grid,
                                    tx as i16,
                                    ty as i16,
                                );
                            } else {
                                self.grid.tiles.get_mut(tx, ty).block = BlockId::AIR;
                                self.grid.tiles.get_mut(tx, ty).build = None;
                            }
                        } else if let Some(runtime) = self.block_runtime.as_mut() {
                            runtime.set_block_raw(
                                &mut self.ecs.0,
                                &mut self.grid,
                                tx as i16,
                                ty as i16,
                                block_id,
                                rot,
                                team,
                            );
                        } else {
                            self.grid.tiles.get_mut(tx, ty).block = block_id;
                            self.grid.tiles.get_mut(tx, ty).build = None;
                        }
                        changed = true;
                    }
                }
            }
            // `building` case is not allowed.
            crate::logic::enums::TileLayer::Building => {}
        }
        if changed {
            // The unit runtime's grid snapshot feeds `getblock` and spawning.
            self.refresh_unit_runtime();
            self.logic_world_changed = true;
        }
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
}
