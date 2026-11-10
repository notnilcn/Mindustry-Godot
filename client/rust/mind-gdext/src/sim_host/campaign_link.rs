// SPDX-License-Identifier: GPL-3.0-only

//! Campaign/launch/sector seams read by `MindCampaign`.
//! Upstream: core/src/mindustry/core/Control.java (sector play), core/src/mindustry/game/Saves.java

use super::*;

/// `team`'s first core block item counts as a JSON object (empty when the team
/// has no core or content boot failed). `player.team().core()` semantics
/// (`Team.java:83-85`): only the requested team's cores are considered, so an
/// enemy/derelict core earlier in entity order never leaks into the HUD
/// (PAR-0485 / CG-ECON-02). See [`MindSimHost::core_items_json`].
pub(super) fn core_items_json(sim: &Sim, content: &ContentRegistry, team: u8) -> String {
    let world = &sim.ecs.0;
    for (_id, entity, comp) in sim.ecs.entities_by_seq() {
        if comp.team.0 != team {
            continue;
        }
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

impl MindSimHost {
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
            .world()
            .get_resource::<mind_core::game::fog::FogControl>()
            .is_none()
        {
            self.sim.reset_fog();
        }
        let Some(mut fog) = self
            .world_mut()
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
        let Some(content) = self.content_registry() else {
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
        let Some(content) = self.content_registry() else {
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
        self.world_mut().insert_resource(runtime);
    }

    /// Removes the campaign runtime (menu/abort/new world).
    pub fn clear_campaign_runtime(&mut self) {
        self.world_mut().remove_resource::<CampaignRuntime>();
        // The hooks answer `is_campaign: false` again and drop pending deltas.
        mind_core::game::runtime::reset_campaign_core_hooks(self.world_mut());
    }

    /// Takes the runtime out for host-side bookkeeping.
    pub fn take_campaign_runtime(&mut self) -> Option<CampaignRuntime> {
        self.world_mut().remove_resource::<CampaignRuntime>()
    }

    /// Puts a bookkept runtime back.
    pub fn put_campaign_runtime(&mut self, runtime: CampaignRuntime) {
        // The campaign core hooks queue `SectorInfo.handleCoreItem` deltas the
        // campaign tick folds into the played sector (PAR-0082), and the AI
        // queue carries the base-builder placements the pump drains (PAR-0139).
        mind_core::game::runtime::install_campaign_core_hooks(self.world_mut());
        mind_core::game::runtime::install_campaign_ai_queues(self.world_mut());
        self.world_mut().insert_resource(runtime);
    }

    /// The installed runtime's `Vars.service`-equivalent tracker (`GAPS #68`):
    /// the campaign systems count stats/achievements into it, and the facade
    /// mirrors it into `MindCampaign` for persistence and the UI queries.
    /// `None` without a campaign sector installed.
    pub fn campaign_service(&self) -> Option<&mind_core::game::service::ServiceTracker> {
        self.campaign_runtime().map(|runtime| &runtime.service)
    }

    /// The installed runtime's `universe.secondsf()` (read-only; `None` with
    /// no campaign sector installed).
    pub fn campaign_universe_secondsf(&self) -> Option<f32> {
        self.campaign_runtime()
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
            // Field path: the `sector_spawns` read below borrows `self`.
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

    /// `Schematics.placeLaunchLoadout` body shared by the schematic and
    /// single-core loadouts.
    pub(super) fn place_loadout_schematic(
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
        self.mark_world_changed();
        log::info!(
            "place_launch_loadout: `{}` core at ({core_x}, {core_y}) stocked from {} stack(s)",
            schem.name(),
            stacks.len()
        );
        true
    }

    /// The default team's core anchor tile in the live world (`Player.bestCore`
    /// position; `BuildingComp.pos` is the anchor tile).
    pub(super) fn player_core_tile(&self, team: u8) -> Option<(i16, i16)> {
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

    /// The accelerator building at tile `(x, y)` (any tile of its footprint).
    pub(super) fn accelerator_at(&self, x: i32, y: i32) -> Option<bevy_ecs::entity::Entity> {
        let Some((tx, ty)) = crate::gdutil::coords::tile_i16(x, y) else {
            return None;
        };
        let entity = self
            .sim
            .grid
            .entity_at(mind_core::world::TilePos::new(tx, ty))?;
        let block = self
            .world()
            .get::<mind_core::entities::comp::Building>(entity)?
            .block;
        let kind = self.content_snapshot.as_ref()?.block(block)?.kind;
        (kind == mind_core::content::registries::blocks::BlockKind::Accelerator).then_some(entity)
    }

    /// Live core items of `team` as `(content name, amount)` (`Team.items()`:
    /// the first core in entity order). Rust-only seam for the campaign
    /// research spend.
    pub fn player_core_items(&self, team: u8) -> Vec<(String, i32)> {
        let Some(content) = self.content_registry() else {
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
            let Some(module) = self.world().get::<ItemModule>(entity) else {
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
            if !is_core || self.world().get::<ItemModule>(entity).is_none() {
                continue;
            }
            target = Some(entity);
            break;
        }
        let Some(entity) = target else {
            return 0;
        };
        match self.world_mut().get_mut::<ItemModule>(entity) {
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
            if !is_core || self.world().get::<ItemModule>(entity).is_none() {
                continue;
            }
            target = Some(entity);
            break;
        }
        let Some(entity) = target else {
            return false;
        };
        let capacity = self
            .world()
            .get::<mind_core::world::blocks::storage::core_block::CoreBuild>(entity)
            .map(|core| core.storage_capacity)
            .unwrap_or(0);
        // Field path: `content` below borrows `self.content_snapshot`.
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
}

#[godot_api(secondary)]
impl MindSimHost {
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
            .world()
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
                    crate::gdutil::json::value_gstring(&context).to_variant(),
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
        crate::gdutil::json::value_gstring(&serde_json::Value::Array(stacks))
    }

    /// `launching = false` right before the sector switch (Accelerator.java:274).
    #[func]
    pub fn accelerator_end_launch(&mut self, x: i32, y: i32) {
        if let Some(entity) = self.accelerator_at(x, y) {
            self.sim.accelerator_end_launch(entity);
        }
    }

    /// Player-core item counts as JSON (`{item-name: amount}`) for the HUD core
    /// display (`CoreItemsDisplay`). Appended for `MindHud`; read-only. Mirrors
    /// `CoreItemsDisplay.java:33` `player.team().core()` (`Team.java:83-85`):
    /// the **player team's** first core in entity order, never an enemy or
    /// derelict core's inventory (PAR-0485 / CG-ECON-02).
    #[func]
    pub fn core_items_json(&self) -> GString {
        let Some(content) = self.content_registry() else {
            return crate::gdutil::json::empty_object();
        };
        GString::from(core_items_json(&self.sim, content, self.player_team()).as_str())
    }

    /// Launch pods in flight (`LaunchPayloadState`; H04 T6 seam for H03's
    /// `LaunchPayloadComp.draw`: `fin = time/lifetime`, region `<block>-pod`).
    #[func]
    pub fn launch_payloads_json(&mut self) -> GString {
        let rows = mind_core::game::launch_payload::payload_rows(self.world_mut());
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
        let rows = mind_core::game::landing_pad::landing_pad_rows(self.world_mut());
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
}
