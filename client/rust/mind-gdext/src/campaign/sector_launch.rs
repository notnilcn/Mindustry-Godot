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
        // `Campaign::sector_by_name` (H01): planet name + sector index in one.
        let Some((_planet_id, record)) = campaign.sector_by_name(&planet_name, sector_id) else {
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

        // C-9/GAP-12 + `Control.playSector`'s branch selection
        // (`Control.java:449-470`): the unknown planet/sector and locked-preset
        // rejects, then the resume/lost-replay/fresh decision
        // (`play::select_sector_launch`, H02). The host's live-core read is the
        // `no_live_core` input.
        let default_team = self.session.rules.default_team;
        let no_live_core = self
            .host()
            .map(|host| host.bind().player_core_blocks(default_team) == 0)
            .unwrap_or(false);
        let Some((planet_id, launch)) =
            self.select_launch(&campaign, &registry, &planet_name, sector_id, no_live_core)
        else {
            self.registry = Some(registry);
            self.campaign = Some(campaign);
            return false;
        };
        // `Control.playSector`'s `catch(SaveException)`: an unreadable resume
        // save is discarded before the play flow runs (it can only clear the
        // save, so the branch flags are corrected here).
        let launch =
            self.discard_corrupt_resume(&mut campaign, planet_id, sector_id, &planet_name, launch);

        // `Control.playSector` (`Control.java:437-448`): quiet-unlock the preset
        // (no unlock event) and record the planet's last sector.
        settings_hub::with_settings(|settings| {
            play::begin_sector_launch(&mut campaign, &mut registry, settings, planet_id, sector_id);
        });
        self.log_launch_branch(
            &campaign,
            planet_id,
            sector_id,
            &planet_name,
            &launch,
            no_live_core,
            default_team,
        );

        // H05 T2: a lost sector's replay regenerates the world through the host.
        let restored_survivors = if launch.lost_replay {
            self.replay_lost_sector(
                &mut campaign,
                planet_id,
                &planet,
                sector,
                &planet_name,
                sector_id,
                launch.spawn_position,
            )
        } else {
            Vec::new()
        };

        // Optional launch loadout (`GAP-10`/PAR-0085). A campaign launch without
        // an explicit loadout carries an EMPTY loadout: the `Rules` default
        // (`copper x100`) is the custom-game default and must not stock a
        // campaign core. Java's reference Ground Zero resume save stores
        // `loadout:[]` (runs/20261009-065416-ref-ground-zero/checkpoints/
        // r01-sector-serpulo-170.msav, decompressed meta) and starts with an
        // empty core; `FileMapGenerator`/`Logic.play` then clear the core and
        // add nothing (PAR-0056). The payload also carries the dialog's
        // schematic index, origin sector id, accelerator `launch_core` and the
        // stacks JSON (`LaunchPayload::parse`, H02).
        let payload = self.apply_launch_payload(&loadout_json.to_string());
        // `Control.playNewSector` (`Control.java:573-578`): the launcher sector
        // id from the dialog becomes `origin`/`destination`.
        let origin = payload.origin.map(|origin| (planet_id, origin));

        // `Sector.isBeingPlayed` (`Sector.java:148-151`): only the sector being
        // played reads true; a launch clears every previous flag first (T1).
        // `Campaign::clear_being_played` (H01).
        campaign.clear_being_played();

        self.reloader = HostReloader::default();
        let events = if launch.resume {
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
        // T2: `readMeta`'s resume fields from the save the host just decoded.
        let resume_static_fog = self.take_resume_meta(&launch);
        // Adopt the loaded map/save rules wholesale (T5).
        self.adopt_loaded_map_rules(&campaign, &registry, planet_id, sector_id, &launch);
        // Fresh generations get terrain weather; resumed saves keep theirs.
        self.adopt_fresh_weather(&planet_name, &launch);
        // `Logic.play` installs the map objective executor from the final rules;
        // the play-flow events ran before the map merge above, so rebuild the
        // runtime here or the tutorial's first objective never qualifies
        // (PAR-0037: resumed Ground Zero showed no objective state).
        self.session.objectives = MapObjectivesRuntime::from_rules(&self.session.rules);
        // `apply_team_plans`: the resume save's build plans (and the replay's
        // restored survivors) land in the live session's team data before the
        // runtime snapshot is taken (PAR-0084).
        self.carry_team_plans(&launch, &restored_survivors, default_team);

        // Install the runtime and register the live cores/spawns (GAP-9).
        let installed = self.install_sector_runtime(
            &registry,
            &mut campaign,
            planet_id,
            sector_id,
            &planet_name,
            &launch,
            &payload,
            default_team,
        );

        // `static-fog-data`: applied after the campaign runtime is installed
        // (only then can `reset_fog` build the `FogControl` from `rules.fog`).
        if installed
            && let Some(bytes) = resume_static_fog
            && let Some(mut host) = self.host()
        {
            host.bind_mut().apply_static_fog(bytes);
        }

        // `Trigger.newGame` (`Control.java:198-222`): spawn the player mech at
        // the best core so `player.unit()` is live after the launch and WASD
        // drives it instead of the camera; the core then drops in with the
        // 160-tick landing cutscene unless `skipcoreanimation` or a PvP rule set
        // (PAR-0131). Shared with `start_loaded_world`.
        self.finish_launch(installed, &events, "start_sector");

        self.registry = Some(registry);
        self.campaign = Some(campaign);
        self.invalidate_ui_block_catalog();
        if installed {
            self.bind_sector_slot(planet_id, &planet_name, sector_id, launch.has_save);
        }
        // Persist attempts/rules and the launch slot bookkeeping (name,
        // autosave flag, `last-sector-save`); the live tile save is queued for
        // the next IO tick (`Control.playNewSector` -> `saves.saveSector`).
        self.persist();
        self.runtime_installed && !events.is_empty()
    }
}

impl MindCampaign {
    /// `Control.playSector`'s validation + branch decision: the unknown-planet
    /// warn (the planet name resolves first so the two warns stay distinct),
    /// then H02's `play::select_sector_launch` (locked preset, resume, lost
    /// replay, fresh). `None` after logging the same line the replaced code did.
    fn select_launch(
        &self,
        campaign: &Campaign,
        registry: &ContentRegistry,
        planet_name: &str,
        sector_id: u16,
        no_live_core: bool,
    ) -> Option<(PlanetId, play::SectorLaunch)> {
        let Some(planet_id) = campaign.planet_id_by_name(planet_name) else {
            log::warn!("start_sector: unknown planet `{planet_name}`");
            return None;
        };
        match play::select_sector_launch(campaign, registry, planet_id, sector_id, no_live_core) {
            Ok(launch) => Some((planet_id, launch)),
            Err(reject) => {
                match reject {
                    play::LaunchReject::SectorOutOfRange => log::warn!(
                        "start_sector: sector {sector_id} out of range on `{planet_name}`"
                    ),
                    play::LaunchReject::LockedPreset => {
                        log::warn!("start_sector: `{planet_name}:{sector_id}` is locked");
                    }
                }
                None
            }
        }
    }

    /// `Control.playSector`'s `catch(SaveException)` (`Control.java:555-561`):
    /// the host found the associated resume save unreadable between listing and
    /// load. Java clears `sector.save`, deletes the slot, reports
    /// `@save.corrupted` and replays the sector; it never resumes a regenerated
    /// world under the stale record (CG-SAVE-06). The load path only flags it —
    /// the record is cleared here, before the resume branch runs, so the
    /// pre-corrupt branch flags are corrected to the no-save branch.
    fn discard_corrupt_resume(
        &mut self,
        campaign: &mut Campaign,
        planet_id: PlanetId,
        sector_id: u16,
        planet_name: &str,
        mut launch: play::SectorLaunch,
    ) -> play::SectorLaunch {
        let corrupt_resume = self
            .host()
            .and_then(|mut host| host.bind_mut().take_corrupt_sector_save());
        let Some(path) = corrupt_resume else {
            return launch;
        };
        let file = std::path::PathBuf::from(&path);
        if let Some(sector) = campaign.sector_mut(planet_id, sector_id) {
            sector.save = None;
        }
        launch.has_save = false;
        launch.resume = false;
        launch.lost_replay = false;
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
            "start_sector: `{planet_name}:{sector_id}` resume save `{path}` was corrupt;              deleted the slot and replaying the sector (`@save.corrupted`)"
        );
        launch
    }

    /// The `start_sector` branch diagnostics: one `log::info!` per launch with
    /// the per-branch inputs the decision read (the replaced block computed them
    /// inline). Values are re-derived from the same sources
    /// [`play::select_sector_launch`] reads so the line stays byte-identical.
    #[allow(clippy::too_many_arguments)]
    fn log_launch_branch(
        &self,
        campaign: &Campaign,
        planet_id: PlanetId,
        sector_id: u16,
        planet_name: &str,
        launch: &play::SectorLaunch,
        no_live_core: bool,
        default_team: u8,
    ) {
        let record = campaign.sector(planet_id, sector_id);
        let had_no_core = record.is_some_and(|sector| !sector.info.info.has_core);
        let clear_save = campaign
            .planet(planet_id)
            .is_some_and(|planet| planet.campaign_rules.clear_sector_on_lose);
        let data_matches = record.is_some_and(|sector| {
            // Deviation: the preset def carries no map dimensions, so the
            // record's own size is passed; the effective test is the preset
            // name (`SectorInfo.sectorDataMatches`, `SectorInfo.java:98-103`).
            sector.sector_data_matches(
                launch.preset_name.as_deref(),
                sector.info.info.last_width,
                sector.info.info.last_height,
            )
        });
        let (cores, core_teams) = self
            .host()
            .map(|host| {
                let guard = host.bind();
                (guard.player_core_blocks(default_team), guard.core_teams())
            })
            .unwrap_or_default();
        log::info!(
            "start_sector branch: sector={planet_name}:{sector_id} has_save={} \
             had_no_core={had_no_core} no_live_core={no_live_core} default_team={default_team} \
             clear_save={clear_save} data_matches={data_matches} lost_replay={} \
             resume={} cores={cores:?} teams={core_teams:?}",
            launch.has_save,
            launch.lost_replay,
            launch.resume
        );
    }

    /// H05 T2/PAR-0084: the lost-sector replay — prime the generator extras from
    /// the live record, replay through the host and mark the record as having a
    /// core again. Returns the restored building survivors.
    #[allow(clippy::too_many_arguments)]
    fn replay_lost_sector(
        &mut self,
        campaign: &mut Campaign,
        planet_id: PlanetId,
        planet: &GString,
        sector: i32,
        planet_name: &str,
        sector_id: u16,
        spawn_position: i32,
    ) -> Vec<mind_core::sim::runtime::ReplayBuilding> {
        if let Some(record) = campaign.sector(planet_id, sector_id)
            && let Some(mut host) = self.host()
        {
            host.bind_mut().set_sector_gen_extras_rs(
                planet_name,
                sector,
                mind_core::maps::planet::SectorGenExtras {
                    threat: record.threat,
                    has_enemy_base: record.has_enemy_base(),
                    has_base: record.has_base(),
                },
            );
        }
        let (spawn_x, spawn_y) = mind_core::world::pos::unpack(spawn_position);
        let restored = self
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
            restored.len()
        );
        restored
    }

    /// The launch payload (`LaunchPayload::parse`, H02) and the session
    /// loadout half (`GAP-10`/PAR-0085): an empty payload clears the loadout, a
    /// valid stacks list replaces it, an invalid one leaves it alone.
    fn apply_launch_payload(&mut self, json: &str) -> LaunchPayload {
        let payload = LaunchPayload::parse(json);
        if json.trim().is_empty() {
            self.session.rules.loadout.clear();
        } else if let Some(stacks) = &payload.stacks {
            self.session.rules.loadout = stacks.clone();
        }
        payload
    }

    /// T2: `readMeta`'s resume fields — `wavetime`/`stats`/markers — from the
    /// save `MindSimHost.load_sector` just decoded. Java restores them inside
    /// `SaveIO.load` (`SaveVersion.readMeta`) before the `SaveLoadEvent` rule
    /// merge; the host also resumed the saved `tick`. The `static-fog-data`
    /// chunk is returned for the caller to apply after the runtime install (it
    /// needs the live `FogControl` created by `reset_fog`).
    fn take_resume_meta(&mut self, launch: &play::SectorLaunch) -> Option<Vec<u8>> {
        if !launch.resume {
            return None;
        }
        let meta = self
            .host()
            .and_then(|mut host| host.bind_mut().take_sector_resume())?;
        self.session.wavetime = meta.wavetime.unwrap_or(self.session.rules.wave_spacing);
        if let Some(stats) = meta.stats {
            self.session.stats = stats;
        }
        self.session.markers = meta.markers;
        meta.static_fog
    }

    /// `SaveVersion.readRules` + `World.setSectorRules` (T5): the loaded
    /// preset/generated map (or sector save) rules replace the session rules
    /// wholesale and the host re-derives the world-side state (map-area limit
    /// rect + border darkness) — `play::adopt_map_rules` (H02).
    fn adopt_loaded_map_rules(
        &mut self,
        campaign: &Campaign,
        registry: &ContentRegistry,
        planet_id: PlanetId,
        sector_id: u16,
        launch: &play::SectorLaunch,
    ) {
        let Some(map_rules) = self
            .host()
            .and_then(|host| host.bind().last_sector_rules().cloned())
        else {
            return;
        };
        let Some(planet) = campaign.planet(planet_id) else {
            return;
        };
        let preset_no_lighting = campaign
            .sector(planet_id, sector_id)
            .is_some_and(|sector| sector.preset_no_lighting);
        play::adopt_map_rules(
            &mut self.session,
            &map_rules,
            planet,
            registry,
            launch.preset.is_some(),
            preset_no_lighting,
            launch.resume,
        );
        if let Some(mut host) = self.host() {
            host.bind_mut().apply_world_rules(&self.session.rules);
        }
    }

    /// `World.loadSector` -> `setSectorRules` -> `planet.generator.addWeather`
    /// (World.java:292,303): fresh generations get terrain weather, replacing
    /// the preset map's entries; resumed saves keep theirs.
    fn adopt_fresh_weather(&mut self, planet_name: &str, launch: &play::SectorLaunch) {
        let fresh = !launch.resume;
        if fresh
            && mind_core::maps::generators::planet::planet_adds_weather(planet_name)
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
    }

    /// `apply_team_plans`: the resume save's build plans (and the replay's
    /// restored survivors) land in the live session's team data before the
    /// runtime snapshot is taken (PAR-0084).
    fn carry_team_plans(
        &mut self,
        launch: &play::SectorLaunch,
        restored_survivors: &[mind_core::sim::runtime::ReplayBuilding],
        default_team: u8,
    ) {
        if (launch.has_save || launch.lost_replay)
            && let Some(mut host) = self.host()
        {
            let carried = host.bind_mut().take_sector_plans();
            mind_core::io::sim_io::apply_team_plans(&mut self.session.teams, carried);
        }
        if launch.lost_replay && !restored_survivors.is_empty() {
            let data = self.session.teams.get(mind_core::ecs::TeamId(default_team));
            for entry in restored_survivors {
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
    }

    /// Installs the sector's `CampaignRuntime` (`CampaignRuntime::install`, H02)
    /// and runs the launch's post-install host steps: launch loadout/core
    /// placement (PAR-0085/L06 T7), the session sync, the record's save
    /// association and — on a resume — the `SaveLoadEvent` record write
    /// (`runtime::apply_resume_record`, H02) plus the core refill. Returns
    /// whether the runtime was installed.
    #[allow(clippy::too_many_arguments)]
    fn install_sector_runtime(
        &mut self,
        registry: &ContentRegistry,
        campaign: &mut Campaign,
        planet_id: PlanetId,
        sector_id: u16,
        planet_name: &str,
        launch: &play::SectorLaunch,
        payload: &LaunchPayload,
        default_team: u8,
    ) -> bool {
        let has_save = launch.has_save;
        let mut installed = false;
        if let Some(mut host) = self.host() {
            // One shared `Vars.service` (GAPS #68): the facade's boot-loaded
            // tracker goes into the live runtime, so the campaign systems keep
            // counting into the same achievement/stat graph.
            let runtime = CampaignRuntime::install(
                self.session.clone(),
                campaign.clone(),
                self.service.clone(),
                registry,
            );
            host.bind_mut().install_campaign_runtime(runtime);
            // `Schematics.placeLaunchLoadout` (PAR-0085 step 5): a launcher
            // launch replaces the map's core with the loadout schematic's core
            // before the session sync registers and stocks it. Fresh launches
            // only (`playNewSector`); a resume keeps the saved world unchanged.
            // H05 T4: a generated sector already placed its loadout during the
            // world load (the generator runs `placeLaunchLoadout`).
            if !has_save
                && self.session.allow_launch_loadout
                && let Some(index) = payload.schematic
                && !host.bind().generated_loadout_placed()
            {
                host.bind_mut()
                    .place_launch_loadout(index, GString::from(payload.stacks_json.as_str()));
            }
            // L06 T7: the accelerator's `updateLoadout(CoreBlock)` single-core
            // loadout on a preset map (`FileMapGenerator` placeLaunchLoadout).
            if !has_save
                && self.session.allow_launch_loadout
                && payload.schematic.is_none()
                && let Some(core) = payload.launch_core.as_deref()
                && !host.bind().generated_loadout_placed()
            {
                host.bind_mut().place_launch_core(
                    GString::from(core),
                    GString::from(payload.stacks_json.as_str()),
                );
            }
            let sync = host.bind_mut().sync_campaign_session(registry, !has_save);
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
                    // Carry the launch ownership back into the runtime copy.
                    runtime.campaign = campaign.clone();
                    let core_items = if launch.resume {
                        // `Logic` `SaveLoadEvent` (`Logic.java:70-88`):
                        // `info.write()` pushes the stored record into the live
                        // session, then `planet.applyRules` re-derives the
                        // campaign rules over it (H04 T3).
                        settings_hub::with_settings(|settings| {
                            apply_resume_record(runtime, registry, settings, planet_id, sector_id)
                        })
                    } else {
                        None
                    };
                    // `Trigger.newGame` (`Control.java:212-216`):
                    // `if(info.importRateCache != null) info.refreshImportRates(planet)`
                    // so the played sector's landing pads see its importers (T7).
                    runtime
                        .campaign
                        .refresh_sector_import_rates(registry, planet_id, sector_id);
                    self.session = runtime.session.clone();
                    core_items
                });
                // H04 T13: `UnitComp.afterRead` (`UnitComp.java:585-593`).
                // The world load spawned the saved units before the session
                // rules existed, so a player-team unit came back as
                // `GroundAI`; re-derive the controllers now that the runtime is
                // installed. A fresh launch has no restored units, so the call
                // is a no-op there.
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
        installed
    }

    /// `Trigger.newGame` (`Control.java:198-222`) + the shared post-install tail
    /// of both play flows: spawn the player at the best core, play the landing
    /// cutscene unless `skipcoreanimation` or a PvP rule set (PAR-0131), then
    /// record the runtime state and reset the pump tick. `ctx` is the caller's
    /// log prefix (`start_sector`/`start_loaded_world`).
    pub(super) fn finish_launch(
        &mut self,
        installed: bool,
        events: &[mind_core::game::play::PlayEvent],
        ctx: &str,
    ) {
        // `CoreBlock.playerSpawn` (upstream `Trigger.newGame` -> `player.set(core)`):
        // spawn the player mech at the best core so `player.unit()` is live after
        // the launch and WASD drives it instead of the camera.
        if installed
            && let Some(mut host) = self.host()
            && !host.bind_mut().spawn_player_unit()
        {
            log::warn!("{ctx}: player unit spawn failed");
        }
        // The core drops in with the 160-tick landing cutscene unless
        // `skipcoreanimation` or a PvP rule set (PAR-0131). The scaled core
        // sprite/fade/announce ride the camera's `get_land_time_in` from the UI;
        // the core block name is unused until that draw lands.
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
        self.runtime_installed = installed;
        if let Some(host) = self.host() {
            self.last_pump_tick = host.bind().get_tick().max(0) as u64;
        }
    }

    /// The launch's save-slot bookkeeping (`Control.playNewSector` ->
    /// `Saves.saveSector` / `World.loadSector` -> `SaveSlot.load`): resolve the
    /// record's slot stem to its file (`CG-SAVE-05`), make a resumed slot
    /// current, or create the fresh sector's autosave slot and queue its tile
    /// save with the launch record (`CG-SAVE-01`).
    fn bind_sector_slot(
        &mut self,
        planet_id: PlanetId,
        planet_name: &str,
        sector_id: u16,
        has_save: bool,
    ) {
        let Some(save_name) = self
            .campaign
            .as_ref()
            .and_then(|campaign| campaign.sector(planet_id, sector_id))
            .and_then(|sector| sector.save.clone())
        else {
            return;
        };
        // `Sector.save` is the slot's stem; resolve it through the listed
        // slots so a resumed copy under another name keeps writing back to its
        // own file (`Saves.saveSector` reuses `sector.save`,
        // `Saves.java:257-259`) (CG-SAVE-05).
        let file = self.sector_save_file(planet_name, sector_id, &save_name);
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
                    .prepare_sector_slot(settings, &self.paths, planet_name, sector_id);
            });
            // `SaveVersion.writeMeta` (`SaveVersion.java:117-118`) runs
            // `state.rules.sector.info.prepare(sector)` + `saveInfo()` on
            // EVERY save, including the launch save. Without it the stored
            // record stays at the default (empty items, `wave: 1`), so a crash
            // in the first session resumes an empty core at wave 1 while the
            // tile save has everything (CG-SAVE-01).
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
