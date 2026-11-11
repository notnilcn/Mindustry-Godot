// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Sector launch flows: prime/start a sector and adopt the loaded map's rules.
//! Upstream: core/src/mindustry/core/Control.java (playSector, playNewSector); core/src/mindustry/core/World.java (loadSector); game/SectorInfo.java

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Primes the host's sector-generation extras from the campaign record
    /// (`Sector.threat`/`hasEnemyBase`/`hasBase`) so the next `load_sector` of
    /// a numbered sector generates with the live campaign state (H05 T2).
    /// Call before `MindSimHost.load_sector` for the same planet+sector.
    ///
    /// Also hands the host the sector's associated save file
    /// (`Control.playSector` -> `Sector.save`, `Control.java:450-456`): the
    /// resume source is the slot the meta association picked, not a guessed
    /// file name (CG-SAVE-05).
    #[func]
    pub fn prime_sector_load(&mut self, planet: GString, sector: i32) -> bool {
        let planet_name = planet.to_string();
        let Some(sector_id) = sector_arg(i64::from(sector)) else {
            return false;
        };
        let Some(campaign) = self.campaign.as_ref() else {
            return false;
        };
        let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
            return false;
        };
        let Some(record) = campaign.sector(planet_id, sector_id) else {
            return false;
        };
        let extras = mind_core::maps::planet::SectorGenExtras {
            threat: record.threat,
            has_enemy_base: record.has_enemy_base(),
            has_base: record.has_base(),
        };
        let save_file = record
            .save
            .as_deref()
            .map(|stem| self.sector_save_file(&planet_name, sector_id, stem));
        let Some(mut host) = self.host() else {
            return false;
        };
        host.bind_mut()
            .set_sector_gen_extras_rs(&planet_name, sector, extras);
        host.bind_mut().set_sector_resume_file(GString::from(
            save_file
                .map(|file| file.to_string_lossy().into_owned())
                .unwrap_or_default()
                .as_str(),
        ));
        true
    }

    /// Starts a campaign sector with the default (empty) launch loadout,
    /// returning success. Delegates to [`Self::start_sector_with_loadout`] with
    /// an empty payload; use that method to pass an explicit
    /// `[{item, amount}]` loadout.
    #[func]
    pub fn start_sector(&mut self, planet: GString, sector: i32) -> bool {
        self.start_sector_with_loadout(planet, sector, GString::from(""))
    }

    /// Starts a campaign sector with an explicit launch loadout (WS3 wiring):
    /// `loadout_json` is the optional `[{item, amount}]` payload applied on the
    /// fresh-launch path (empty string = default).
    #[func]
    pub fn start_sector_with_loadout(
        &mut self,
        planet: GString,
        sector: i32,
        loadout_json: GString,
    ) -> bool {
        self.current_map_name = None;
        // `logic.reset()` (`state = new GameState()`): not a playtest.
        self.playtesting_map = None;
        let planet_name = planet.to_string();
        let Some(sector_id) = sector_arg(i64::from(sector)) else {
            return false;
        };
        // `Control.playSector` (`Control.java:437-441`): a launch that replaces
        // a live match writes the outgoing slot first (CG-SECTOR-02). The
        // caller must have done this BEFORE the destination world load (the
        // planet dialog does it via `save_outgoing_sector`; the accelerator in
        // `pump_accelerator_launch`), because by now the host already holds the
        // destination world.
        let Some(mut registry) = self.registry.take() else {
            return false;
        };
        // T1: a launch while a sector is live starts from the runtime's state
        // (the old runtime is replaced below by `install_campaign_runtime`).
        self.mirror_runtime_campaign();
        let Some(mut campaign) = self.campaign.take() else {
            self.registry = Some(registry);
            return false;
        };

        // C-9/GAP-12: unknown planet/sector and locked presets are rejected.
        let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
            log::warn!("start_sector: unknown planet `{planet_name}`");
            self.registry = Some(registry);
            self.campaign = Some(campaign);
            return false;
        };
        let Some(record) = campaign.sector(planet_id, sector_id) else {
            log::warn!("start_sector: sector {sector_id} out of range on `{planet_name}`");
            self.registry = Some(registry);
            self.campaign = Some(campaign);
            return false;
        };
        let always_unlocked = record.preset_always_unlocked;
        let require_unlock = record.preset_require_unlock;
        let preset_id = record.preset;
        let mut has_save = record.has_save();
        let spawn_position = record.info.info.spawn_position;
        let had_no_core = !record.info.info.has_core;
        let tech_unlocked = preset_id.is_none_or(|preset| {
            tech_tree::content_unlocked(&registry, ContentRef::of(ContentType::Sector, preset))
        });
        if require_unlock && !always_unlocked && !tech_unlocked {
            log::warn!("start_sector: `{planet_name}:{sector_id}` is locked");
            self.registry = Some(registry);
            self.campaign = Some(campaign);
            return false;
        }
        // `Control.playSector`'s `catch(SaveException)` (`Control.java:555-561`):
        // the host found the associated resume save unreadable between listing
        // and load. Java clears `sector.save`, deletes the slot, reports
        // `@save.corrupted` and replays the sector; it never resumes a
        // regenerated world under the stale record (CG-SAVE-06). The load path
        // only flags it — the record is cleared here, before the resume branch
        // is chosen.
        let corrupt_resume = self
            .host()
            .and_then(|mut host| host.bind_mut().take_corrupt_sector_save());
        if let Some(path) = corrupt_resume {
            let file = std::path::PathBuf::from(&path);
            if let Some(sector) = campaign.sector_mut(planet_id, sector_id) {
                sector.save = None;
            }
            has_save = false;
            // `SaveSlot.delete()`: the file and its backup, then the list entry
            // (`Slot.delete` removes itself from `saves`).
            let mut deleted = false;
            if let Some(slot) = self.saves.slots.iter_mut().find(|slot| slot.file == file) {
                deleted = slot.delete(&NativeFs).is_ok();
            }
            if !deleted {
                delete_save_and_backup(&file);
            }
            self.saves.forget_file(&file);
            // `Time.runTask(10f, () -> ui.showErrorMessage("@save.corrupted"))`.
            self.show_error("@save.corrupted");
            log::warn!(
                "start_sector: `{planet_name}:{sector_id}` resume save `{path}` was corrupt; \
                 deleted the slot and replaying the sector (`@save.corrupted`)"
            );
        }

        // `Control.playSector` (`Control.java:437-448`): quiet-unlock the preset
        // (no unlock event) and record the planet's last sector.
        let preset_name =
            preset_id.and_then(|preset| registry.sector(preset).map(|def| def.name.clone()));
        if let Some(preset) = preset_id
            && let Some(name) = preset_name.as_deref()
        {
            settings_hub::with_settings(|settings| {
                if let Some(fields) = tech_tree::unlock_fields_mut(
                    &mut registry,
                    ContentRef::of(ContentType::Sector, preset),
                ) {
                    fields.quiet_unlock(name, &mut SettingsUnlockStore::new(settings));
                }
            });
        }
        if let Some(planet) = campaign.planet_mut(planet_id) {
            settings_hub::with_settings(|settings| planet.set_last_sector(settings, sector_id));
        }

        // `Control.playSector`'s branch selection (`Control.java:449-470`): the
        // normal load path is a save with a live core; a save whose sector was
        // lost (`no_live_core || had_no_core`) takes the no-core branch, which
        // replays only when `!clearSave && spawnPosition != 0 &&
        // sectorDataMatches` (PAR-0084).
        let default_team = self.session.rules.default_team;
        let no_live_core = self
            .host()
            .map(|host| host.bind().player_core_blocks(default_team) == 0)
            .unwrap_or(false);
        let no_core_branch = no_live_core || had_no_core;
        let clear_save = campaign
            .planet(planet_id)
            .is_some_and(|planet| planet.campaign_rules.clear_sector_on_lose);
        let data_matches = campaign.sector(planet_id, sector_id).is_some_and(|sector| {
            // Deviation: the preset def carries no map dimensions, so the
            // record's own size is passed; the effective test is the preset
            // name (`SectorInfo.sectorDataMatches`, `SectorInfo.java:98-103`).
            sector.sector_data_matches(
                preset_name.as_deref(),
                sector.info.info.last_width,
                sector.info.info.last_height,
            )
        });
        let lost_replay =
            has_save && no_core_branch && !clear_save && spawn_position != 0 && data_matches;
        // Java's load branch: a save that is not in the no-core branch.
        let resume = has_save && !no_core_branch;
        let (cores, core_teams) = self
            .host()
            .map(|host| {
                let guard = host.bind();
                (guard.player_core_blocks(default_team), guard.core_teams())
            })
            .unwrap_or_default();
        log::info!(
            "start_sector branch: sector={planet_name}:{sector_id} has_save={has_save} \
             had_no_core={had_no_core} no_live_core={no_live_core} default_team={default_team} \
             clear_save={clear_save} data_matches={data_matches} lost_replay={lost_replay} \
             resume={resume} cores={cores:?} teams={core_teams:?}"
        );
        let mut restored_survivors = Vec::new();
        if lost_replay {
            // H05 T2: the replay regenerates the world through the host, so
            // prime the generator extras from the live record first.
            if let Some(record) = campaign.sector(planet_id, sector_id)
                && let Some(mut host) = self.host()
            {
                host.bind_mut().set_sector_gen_extras_rs(
                    &planet_name,
                    sector,
                    mind_core::maps::planet::SectorGenExtras {
                        threat: record.threat,
                        has_enemy_base: record.has_enemy_base(),
                        has_base: record.has_base(),
                    },
                );
            }
            let (spawn_x, spawn_y) = mind_core::world::pos::unpack(spawn_position);
            restored_survivors = self
                .host()
                .map(|mut host| {
                    host.bind_mut().replay_lost_sector(
                        planet.clone(),
                        sector,
                        i32::from(spawn_x),
                        i32::from(spawn_y),
                    )
                })
                .unwrap_or_default();
            if let Some(sector_record) = campaign.sector_mut(planet_id, sector_id) {
                sector_record.info.info.has_core = true;
            }
            log::info!(
                "start_sector: lost-sector replay for `{planet_name}:{sector_id}` restored {} survivor(s)",
                restored_survivors.len()
            );
        }

        // Optional launch loadout (`GAP-10`/PAR-0085). A campaign launch without
        // an explicit loadout carries an EMPTY loadout: the `Rules` default
        // (`copper x100`) is the custom-game default and must not stock a
        // campaign core. Java's reference Ground Zero resume save stores
        // `loadout:[]` (runs/20261009-065416-ref-ground-zero/checkpoints/
        // r01-sector-serpulo-170.msav, decompressed meta) and starts with an
        // empty core; `FileMapGenerator`/`Logic.play` then clear the core and
        // add nothing (PAR-0056).
        let loadout_payload = loadout_json.to_string();
        if loadout_payload.trim().is_empty() {
            self.session.rules.loadout.clear();
        } else if let Some(stacks) = parse_loadout_stacks(&loadout_payload) {
            self.session.rules.loadout = stacks;
        }
        let (launch_schematic, launch_origin, launch_stacks_json) =
            parse_launch_payload(&loadout_payload);
        // The accelerator envelope's `launch_core` (`Universe.updateLoadout(
        // CoreBlock)`, L06 T7).
        let launch_core: Option<String> =
            serde_json::from_str::<serde_json::Value>(&loadout_payload)
                .ok()
                .and_then(|value| value.get("launch_core")?.as_str().map(str::to_owned));
        // `Control.playNewSector` (`Control.java:573-578`): the launcher sector
        // id from the dialog becomes `origin`/`destination`.
        let origin = launch_origin.map(|origin| (planet_id, origin));

        // `Sector.isBeingPlayed` (`Sector.java:148-151`): only the sector being
        // played reads true; a launch clears every previous flag first (T1).
        for planet in campaign.planets.values_mut() {
            for sector in &mut planet.sectors {
                sector.being_played = false;
            }
        }

        self.reloader = HostReloader::default();
        let events = if resume {
            play_sector(
                &mut self.session,
                &mut campaign,
                &registry,
                planet_id,
                sector_id,
                origin,
                &mut self.epoch,
                &mut self.reloader,
            )
        } else {
            play_new_sector(
                &mut self.session,
                &mut campaign,
                &registry,
                planet_id,
                sector_id,
                origin,
                &mut self.epoch,
                &mut self.reloader,
            )
        };
        // T2: `readMeta`'s resume fields — `wavetime`/`stats`/markers — from the
        // save `MindSimHost.load_sector` just decoded. Java restores them inside
        // `SaveIO.load` (`SaveVersion.readMeta`) before the `SaveLoadEvent`
        // rule merge; the host also resumed the saved `tick`. The
        // `static-fog-data` chunk is applied after the rules adoption below
        // (it needs the live `FogControl` created by `reset_fog`).
        let mut resume_static_fog: Option<Vec<u8>> = None;
        if resume
            && let Some(meta) = self
                .host()
                .and_then(|mut host| host.bind_mut().take_sector_resume())
        {
            self.session.wavetime = meta.wavetime.unwrap_or(self.session.rules.wave_spacing);
            if let Some(stats) = meta.stats {
                self.session.stats = stats;
            }
            self.session.markers = meta.markers;
            resume_static_fog = meta.static_fog;
        }
        // Adopt the loaded map/save rules wholesale (T5:
        // `SaveVersion.readRules` + `World.setSectorRules`) and re-derive the
        // world-side state (map-area limit rect + border darkness).
        if let Some(map_rules) = self
            .host()
            .and_then(|host| host.bind().last_sector_rules().cloned())
            && let Some(planet) = campaign.planet(planet_id)
        {
            let preset_no_lighting = campaign
                .sector(planet_id, sector_id)
                .is_some_and(|sector| sector.preset_no_lighting);
            adopt_map_rules(
                &mut self.session,
                &map_rules,
                planet,
                &registry,
                preset_id.is_some(),
                preset_no_lighting,
                resume,
            );
            if let Some(mut host) = self.host() {
                host.bind_mut().apply_world_rules(&self.session.rules);
            }
        }
        // `World.loadSector` -> `setSectorRules` -> `planet.generator.addWeather`
        // (World.java:292,303): fresh generations get terrain weather, replacing
        // the preset map's entries; resumed saves keep theirs.
        let fresh = !resume;
        if fresh
            && mind_core::maps::generators::planet::planet_adds_weather(&planet_name)
            && let Some(host) = self.host()
        {
            let host = host.bind();
            if let Some(content) = host.content_registry() {
                self.session.rules.weather = mind_core::maps::generators::planet::add_weather(
                    host.grid(),
                    content,
                    &self.session.rules,
                );
            }
        }
        // `Logic.play` installs the map objective executor from the final rules;
        // the play-flow events ran before the map merge above, so rebuild the
        // runtime here or the tutorial's first objective never qualifies
        // (PAR-0037: resumed Ground Zero showed no objective state).
        self.session.objectives = MapObjectivesRuntime::from_rules(&self.session.rules);

        // `apply_team_plans`: the resume save's build plans (and the replay's
        // restored survivors) land in the live session's team data before the
        // runtime snapshot is taken (PAR-0084).
        if (has_save || lost_replay)
            && let Some(mut host) = self.host()
        {
            let carried = host.bind_mut().take_sector_plans();
            mind_core::io::sim_io::apply_team_plans(&mut self.session.teams, carried);
        }
        if lost_replay && !restored_survivors.is_empty() {
            let data = self.session.teams.get(mind_core::ecs::TeamId(default_team));
            for entry in &restored_survivors {
                data.plans.push_back(mind_core::game::teams::BlockPlan {
                    x: entry.x,
                    y: entry.y,
                    rotation: entry.rot as i8,
                    block: entry.block,
                    config: None,
                    removed: false,
                });
            }
        }

        // Install the runtime and register the live cores/spawns (GAP-9).
        let mut installed = false;
        if let Some(mut host) = self.host() {
            let mut runtime = CampaignRuntime::new(self.session.clone(), campaign.clone())
                // One shared `Vars.service` (GAPS #68): the facade's boot-loaded
                // tracker goes into the live runtime, so the campaign systems
                // keep counting into the same achievement/stat graph.
                .with_service(self.service.clone());
            runtime.set_unlocked_content(collect_unlocked(&registry));
            host.bind_mut().install_campaign_runtime(runtime);
            // `Schematics.placeLaunchLoadout` (PAR-0085 step 5): a launcher
            // launch replaces the map's core with the loadout schematic's core
            // before the session sync registers and stocks it. Fresh launches
            // only (`playNewSector`); a resume keeps the saved world unchanged.
            // H05 T4: a generated sector already placed its loadout during the
            // world load (the generator runs `placeLaunchLoadout`).
            if !has_save
                && self.session.allow_launch_loadout
                && let Some(index) = launch_schematic
                && !host.bind().generated_loadout_placed()
            {
                host.bind_mut()
                    .place_launch_loadout(index, GString::from(launch_stacks_json.as_str()));
            }
            // L06 T7: the accelerator's `updateLoadout(CoreBlock)` single-core
            // loadout on a preset map (`FileMapGenerator` placeLaunchLoadout).
            if !has_save
                && self.session.allow_launch_loadout
                && launch_schematic.is_none()
                && let Some(core) = launch_core.as_deref()
                && !host.bind().generated_loadout_placed()
            {
                host.bind_mut().place_launch_core(
                    GString::from(core),
                    GString::from(launch_stacks_json.as_str()),
                );
            }
            let sync = host.bind_mut().sync_campaign_session(&registry, !has_save);
            if let Some(sync) = sync {
                if sync.player_cores > 0 {
                    let save_name = format!("sector-{planet_name}-{sector_id}");
                    if let Some(sector) = campaign.sector_mut(planet_id, sector_id) {
                        if sector.save.is_none() {
                            sector.save = Some(save_name);
                        }
                        sector.info.info.has_core = true;
                    }
                }
                let core_items = with_host_runtime(self.host(), |runtime| {
                    let mut core_items: Option<indexmap::IndexMap<String, i32>> = None;
                    // Carry the launch ownership back into the runtime copy.
                    runtime.campaign = campaign.clone();
                    if resume {
                        // `Logic` `SaveLoadEvent` (`Logic.java:70-88`):
                        // `info.write()` pushes the stored record into the live
                        // session, then `planet.applyRules` re-derives the
                        // campaign rules over it (H04 T3).
                        let wave_core = runtime.session.wave_core_count() > 0;
                        let allow_waves = campaign
                            .planet(planet_id)
                            .is_some_and(|planet| planet.allow_waves);
                        if let Some(record) = runtime.campaign.sector_mut(planet_id, sector_id) {
                            let capture_wave = record.preset_capture_wave;
                            let capacity = record.info.info.storage_capacity;
                            let items = record.info.write_to_state(
                                wave_core,
                                allow_waves,
                                capture_wave,
                                &mut runtime.session,
                            );
                            record.info.info.has_core = true;
                            settings_hub::with_settings(|settings| {
                                let _ = record.save_info(settings, &planet_name);
                            });
                            // Keep the session team inventory equal to the core
                            // module, like a fresh launch (`sync_session_with_sim`).
                            let item_count = registry.items().len();
                            let inventory = runtime
                                .session
                                .teams
                                .inventory(mind_core::ecs::TeamId(default_team), item_count);
                            inventory.clear();
                            for (name, amount) in &items {
                                if let Some(item) = registry.item_id(name) {
                                    inventory.add(item, (*amount).clamp(0, capacity), i32::MAX);
                                }
                            }
                            core_items = Some(items);
                        }
                        if let Some(planet) = campaign.planet(planet_id) {
                            planet.apply_rules(&registry, &mut runtime.session.rules, false, true);
                        }
                    }
                    // `Trigger.newGame` (`Control.java:212-216`):
                    // `if(info.importRateCache != null) info.refreshImportRates(planet)`
                    // so the played sector's landing pads see its importers (T7).
                    runtime
                        .campaign
                        .refresh_sector_import_rates(&registry, planet_id, sector_id);
                    self.session = runtime.session.clone();
                    core_items
                });
                // H04 T13: `UnitComp.afterRead` (`UnitComp.java:585-593`).
                // The world load spawned the saved units before the session
                // rules existed, so a player-team unit came back as
                // `GroundAI`; re-derive the controllers now that the runtime
                // is installed. A fresh launch has no restored units, so the
                // call is a no-op there.
                if core_items.is_some()
                    && let Some(mut host) = self.host()
                {
                    host.bind_mut().reset_loaded_unit_controllers();
                }
                if let Some(items) = core_items.flatten()
                    && let Some(mut host) = self.host()
                {
                    host.bind_mut()
                        .write_record_items_to_core(default_team, &items);
                }
                installed = true;
            }
        }

        // `static-fog-data`: applied after the campaign runtime is installed
        // (only then can `reset_fog` build the `FogControl` from `rules.fog`).
        if installed
            && let Some(bytes) = resume_static_fog.take()
            && let Some(mut host) = self.host()
        {
            host.bind_mut().apply_static_fog(bytes);
        }

        // `CoreBlock.playerSpawn` (upstream `Trigger.newGame` → `player.set(core)`):
        // spawn the player mech at the best core so `player.unit()` is live after
        // the launch and WASD drives it instead of the camera.
        if installed
            && let Some(mut host) = self.host()
            && !host.bind_mut().spawn_player_unit()
        {
            log::warn!("start_sector: player unit spawn failed");
        }

        // `Trigger.newGame` (`Control.java:198-222`): the core drops in with the
        // 160-tick landing cutscene unless `skipcoreanimation` or a PvP rule set
        // (PAR-0131). The scaled core sprite/fade/announce ride the camera's
        // `get_land_time_in` from the UI; the core block name is unused until
        // that draw lands.
        // Java fires `Trigger.newGame` only from `playNewSector`/`playMap`
        // (Control.java:415,589); a plain resume (`slot.load`) has no landing.
        let new_game = events
            .iter()
            .any(|event| matches!(event, mind_core::game::play::PlayEvent::NewGame));
        if installed
            && new_game
            && !settings_hub::with_settings(|store| store.get_bool("skipcoreanimation", false))
            && !self.session.rules.pvp
            // Read the core position in its own statement: a `host.bind()`
            // temporary in the let-chain would stay borrowed across the camera
            // call below (re-entrant `bind_mut` panic).
            && let Some((core_x, core_y, core_block)) = self.host().and_then(|host| {
                let guard = host.bind();
                guard
                    .best_core_position_px()
                    .map(|(x, y)| (x, y, guard.best_core_block()))
            })
            && let Some(mut camera) = wiring::optional(&self.camera)
        {
            camera.bind_mut().show_landing(core_x, core_y, core_block);
            // G07 T7: `Trigger.newGame` audio — `coreLand.at(camera)` +
            // `Musics.land` (`Control.java:198-222`).
            if let Some(mut audio) = wiring::optional(&self.audio) {
                audio.bind_mut().play_landing();
            }
        }

        self.registry = Some(registry);
        self.campaign = Some(campaign);
        self.runtime_installed = installed;
        if let Some(host) = self.host() {
            self.last_pump_tick = host.bind().get_tick().max(0) as u64;
        }
        self.invalidate_ui_block_catalog();
        if installed
            && let Some(save_name) = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet_id, sector_id))
                .and_then(|sector| sector.save.clone())
        {
            // `Sector.save` is the slot's stem; resolve it through the listed
            // slots so a resumed copy under another name keeps writing back to
            // its own file (`Saves.saveSector` reuses `sector.save`,
            // `Saves.java:257-259`) (CG-SAVE-05).
            let file = self.sector_save_file(&planet_name, sector_id, &save_name);
            if has_save {
                // `World.loadSector` -> `SaveSlot.load`: the resumed slot is
                // current, so the save policy pumps against it.
                settings_hub::with_settings(|settings| {
                    self.saves.set_current_by_file(settings, &file);
                });
            } else {
                // `Control.playNewSector` -> `Saves.saveSector`: create the
                // sector's autosave slot and make it current; the world write is
                // queued at the IO seam.
                settings_hub::with_settings(|settings| {
                    self.saves
                        .prepare_sector_slot(settings, &self.paths, &planet_name, sector_id);
                });
                // `SaveVersion.writeMeta` (`SaveVersion.java:117-118`) runs
                // `state.rules.sector.info.prepare(sector)` + `saveInfo()` on
                // EVERY save, including the launch save. Without it the stored
                // record stays at the default (empty items, `wave: 1`), so a
                // crash in the first session resumes an empty core at wave 1
                // while the tile save has everything (CG-SAVE-01).
                self.persist_sector_record();
                if let Some(mut host) = self.host() {
                    let path = Self::slot_user_path(&save_name);
                    // `Saves.saveSector` -> `SaveSlot.save` -> `savePreview()`.
                    self.write_live_slot_preview(std::path::Path::new(&save_name));
                    host.bind_mut()
                        .request_save(GString::from(path.as_str()), false);
                }
            }
        }
        // Persist attempts/rules and the launch slot bookkeeping (name,
        // autosave flag, `last-sector-save`); the live tile save is queued for
        // the next IO tick (`Control.playNewSector` -> `saves.saveSector`).
        self.persist();
        self.runtime_installed && !events.is_empty()
    }
}

impl MindCampaign {
    /// The file backing a sector's save association (`Sector.save` stores the
    /// slot's **stem**, `Saves.SaveSlot.index`): the listed slot with that
    /// stem, else the canonical sector file when the stem names it (a save
    /// queued earlier this session), else `saves/<stem>.msav`.
    ///
    /// Never re-derives the path from the sector id alone: the association
    /// came from save meta, so a copy under another name resolves to its own
    /// file (CG-SAVE-05).
    fn sector_save_file(&self, planet_name: &str, sector: u16, stem: &str) -> std::path::PathBuf {
        if let Some(slot) = self.saves.slot_for_stem(stem) {
            return slot.file.clone();
        }
        let canonical = self.paths.sector_save(planet_name, sector as u32);
        if canonical
            .file_stem()
            .is_some_and(|name| name.to_string_lossy() == stem)
        {
            canonical
        } else {
            self.slot_file(stem)
        }
    }
}

/// `SaveVersion.readRules` + `World.setSectorRules` (T5): the loaded
/// preset/generated map (or sector save) rules replace the session rules
/// wholesale, then the planet `ruleSetter`, the preset overlays and the
/// campaign load overrides are re-applied.
///
/// `has_preset` restores the preset's capture win wave on a fresh launch
/// (`SectorPreset.rules` = `winWave = captureWave`); `resumed` keeps the wave
/// record `SectorInfo.write` copied in `play_sector` and re-applies the
/// `SaveLoadEvent` overrides (`Logic.java:73-91`).
fn adopt_map_rules(
    session: &mut PlaySession,
    map: &Rules,
    planet: &mind_core::game::planet::Planet,
    registry: &ContentRegistry,
    has_preset: bool,
    preset_no_lighting: bool,
    resumed: bool,
) {
    let kept = session.rules.clone();
    let mut rules = map.clone();
    // Empty spawns fall back to the generated/default waves.
    if rules.spawns.is_empty() {
        rules.spawns = kept.spawns.clone();
    }
    rules.sector = kept.sector.clone();
    rules.loadout = kept.loadout.clone();
    planet.apply_rules(registry, &mut rules, false, true);
    if has_preset && !resumed {
        rules.win_wave = kept.win_wave;
        rules.waves = kept.waves;
        rules.attack_mode = kept.attack_mode;
    }
    if resumed {
        rules.waves = kept.waves;
        rules.wave_spacing = kept.wave_spacing;
        rules.win_wave = kept.win_wave;
        rules.attack_mode = kept.attack_mode;
    }
    // Preset `noLighting` (PAR-0059) survives the wholesale copy.
    if !kept.lighting && preset_no_lighting {
        rules.lighting = false;
    }
    apply_world_load_campaign_rules(&mut rules, planet);
    if resumed {
        apply_save_load_campaign_rules(&mut rules);
    }
    session.rules = rules;
}

/// Parses a launch-loadout payload: the dialog envelope
/// `{"core": ..., "stacks": [...]}` (`PlanetDialog.playSelected`) or the legacy
/// bare `[{item, amount}]` array.
fn parse_loadout_stacks(
    json: &str,
) -> Option<Vec<mind_core::io::json::content_serde::JsonItemStack>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let stacks = value.get("stacks").cloned().unwrap_or(value);
    serde_json::from_value(stacks).ok()
}

/// Parses the launcher launch payload (`PlanetDialog.playSelected`):
/// `(schematic index, origin sector id, stacks JSON)`.
///
/// The schematic index indexes `Schematics.all` (`None` for the items-only
/// fallback); the `stacks` array JSON is forwarded to
/// `MindSimHost.place_launch_loadout`.
fn parse_launch_payload(json: &str) -> (Option<i32>, Option<u16>, String) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return (None, None, String::from("[]"));
    };
    let schematic = value
        .get("schematic")
        .and_then(serde_json::Value::as_i64)
        .map(|index| index as i32);
    // `Control.playNewSector` (`Control.java:573-578`): `info.origin =
    // info.destination = origin`; absent -> no origin (a free launch).
    let origin = value
        .get("origin")
        .and_then(serde_json::Value::as_i64)
        .and_then(|origin| u16::try_from(origin).ok());
    let stacks = value.get("stacks").cloned().unwrap_or(value);
    (schematic, origin, stacks.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::content::MemoryUnlockStore;
    use mind_core::content::create_base_content;

    /// PAR-0037/PAR-0030 + T5: a preset map's rules replace `state.rules`
    /// wholesale in Java (`FileMapGenerator.generate` -> `SaveIO.load(map.file)`
    /// -> `SaveVersion.readRules`), so the adopted rules must carry the
    /// objective graph, its flags and the `waveTimer` gate into the session or
    /// the tutorial launches as a wave sector.
    #[test]
    fn preset_map_rules_carry_objectives_and_wave_timer() {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        let objectives = match mind_core::io::json::objectives::MapObjectives::from_json(
            r#"[{"class":"Item","amount":15,"markers":[{"class":"Text","text":"@gz.mine"}]}]"#,
        ) {
            Ok(objectives) => objectives,
            Err(error) => panic!("fixture objectives must parse: {error}"),
        };
        let map = Rules {
            wave_timer: false,
            wave_sending: false,
            objectives,
            objective_flags: std::iter::once("spawnWave".to_owned()).collect(),
            ..Rules::default()
        };
        let planet_def = match registry.planet_by_name("serpulo") {
            Some(planet) => planet,
            None => panic!("serpulo planet missing"),
        };
        let planet = mind_core::game::planet::Planet::from_def(&registry, planet_def, 0.0);

        let mut session = PlaySession::new(Rules::default());
        adopt_map_rules(&mut session, &map, &planet, &registry, true, false, false);

        assert!(!session.rules.wave_timer, "map waveTimer:false must win");
        assert!(
            !session.rules.wave_sending,
            "map waveSending:false must win (canSkipWave gate - PAR-0030)"
        );
        assert_eq!(session.rules.objectives.len(), 1);
        assert!(session.rules.objective_flags.contains("spawnWave"));
        // The serpulo ruleSetter re-applied after the wholesale copy.
        assert_eq!(session.rules.wave_team, 2);
        assert!(session.rules.core_destroy_clear);
    }
}
