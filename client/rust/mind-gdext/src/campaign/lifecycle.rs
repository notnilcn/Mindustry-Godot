// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign lifecycle: exit saves, playtest/reset handling and the clear-data commands.
//! Upstream: ui/dialogs/PausedDialog.java (runExitSave, checkPlaytest); core/src/mindustry/core/Logic.java (reset); ui/dialogs/SettingsMenuDialog.java (clear data)

use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Rust-only seam for the planet dialog's "save before launch"
    /// (`PlanetDialog.playSelected`, `PlanetDialog.java:1380-1386`): writes the
    /// live sector before `MindSimHost.load_sector` replaces the world
    /// (CG-SECTOR-02). Returns whether a save was queued.
    #[func]
    pub fn save_outgoing_sector(&mut self) -> bool {
        self.save_outgoing_slot()
    }

    /// `PausedDialog.runExitSave` / `Control.dispose`: writes the current
    /// autosave slot through the IO seam, then resets the match to the menu
    /// (`logic.reset`'s saves/session half). Returns whether a write was queued.
    #[func]
    pub fn run_exit_save(&mut self) -> bool {
        let current = self.saves.get_current().cloned();
        let should_save = current
            .as_ref()
            .is_some_and(|slot| settings_hub::with_settings(|store| slot.is_autosave(store)))
            && !self.session.game_over
            && self.session.phase != State::Menu;
        let mut queued = false;
        if should_save
            && let (Some(file), Some(mut host)) = (current.map(|slot| slot.file), self.host())
        {
            // `Saves.SaveSlot.save` -> `savePreview()` (`@save.quit` path).
            self.queue_slot_save(&mut host, &file, None, true, "saved on exit");
            queued = true;
        }
        self.reset_to_menu();
        queued
    }

    /// `PausedDialog.checkPlaytest` (`PausedDialog.java:174-183`), reached from
    /// `runExitSave` (pause-menu quit, `:189-197`) and the game-over `@menu`
    /// button (`GameOverDialog.java:134-139`): when the match is a playtest
    /// (`state.playtestingMap != null`), reset it WITHOUT the exit save and
    /// reopen the editor on the playtested map
    /// (`ui.editor.resumeAfterPlaytest(map)` -> `beginEditMap(map.file)`,
    /// `MapEditorDialog.java:370-372` / `:508-519`).
    ///
    /// Returns `{"playtest": bool, "resumed": bool}`: `playtest` is false (and
    /// nothing happens) for a normal match; `resumed` is false when the map
    /// could not be reopened (`@editor.errorload` is shown, like Java's
    /// `ui.showException`). The caller shows the editor shell on `resumed`.
    #[func]
    pub fn check_playtest(&mut self) -> VarDictionary {
        let status = |playtest: bool, resumed: bool| {
            let mut out = VarDictionary::new();
            out.set(&GString::from("playtest"), &playtest.to_variant());
            out.set(&GString::from("resumed"), &resumed.to_variant());
            out
        };
        let Some(path) = self.playtesting_map.take() else {
            return status(false, false);
        };
        // "no exit save here"; `logic.reset()`.
        self.reset_to_menu();
        let resumed = wiring::optional(&self.editor).is_some_and(|mut editor| {
            editor
                .bind_mut()
                .resume_after_playtest(GString::from(path.as_str()))
        });
        if !resumed {
            log::warn!("check_playtest: could not reopen `{path}` in the editor");
            self.show_error("@editor.errorload");
        }
        status(true, resumed)
    }

    /// `logic.reset()` (`Logic.java:299-311`): THE return-to-menu path (L02
    /// T2). [`Self::run_exit_save`] and [`Self::check_playtest`] end in it,
    /// and the UI calls it for the game-over `@menu` button
    /// (`GameOverDialog.java:134-138`), the editor exit
    /// (`MapEditorDialog.java:789-795`) and the abandon/quit fallbacks
    /// (`UiRoot.return_to_menu`). Saves/session half: persists the sector
    /// record, drops the session runtime and returns to the menu phase
    /// (`state = new GameState()`, so `playtestingMap` is cleared too); world
    /// half: `MindSimHost.reset_to_menu` empties the sim and sets its phase to
    /// menu. Idempotent.
    #[func]
    pub fn reset_to_menu(&mut self) {
        self.playtesting_map = None;
        // `SectorInfo.write()`/`saveInfo` on exit: the live wave/attack bookkeeping
        // lands in the stored record before the runtime is dropped (PAR-0081).
        self.persist_sector_record();
        // T1: `Sector.isBeingPlayed` (`Sector.java:148-151`) is false once the
        // game state leaves `isGame()`; clear it on the live campaign so the
        // sector produces offline from the next turn on.
        // `Campaign::clear_being_played` (H01): every launch clears the whole
        // grid, so the played sector is the only one that can carry the flag.
        if self.session.sector.is_some() {
            self.with_campaign_mut(|campaign, _settings| campaign.clear_being_played());
        }
        // `StateChangeEvent(menu)`: clears current/playtime (`logic.reset`).
        self.saves.on_state_change(State::Menu);
        self.saving_finish_tick = None;
        self.session.reset_world();
        self.runtime_installed = false;
        // T1: mirror the runtime campaign before dropping it, or the turn
        // clock and every other sector's turn production are lost.
        self.mirror_runtime_campaign();
        if let Some(mut host) = self.host() {
            host.bind_mut().clear_campaign_runtime();
            // `Groups.clear(); world.tiles = new Tiles(0, 0); state = new
            // GameState()` (`Logic.java:300-307`).
            host.bind_mut().reset_to_menu();
        }
        self.persist();
    }

    /// `SettingsMenuDialog.planetDataDialog` selectable planets: a generator,
    /// at least one sector and accessibility (content order).
    #[func]
    pub fn selectable_planets(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        let Some(registry) = self.registry.as_ref() else {
            return out;
        };
        for def in registry.planets() {
            if def.generator == GeneratorKind::None || def.sectors.is_empty() || !def.accessible {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set(&GString::from("name"), &def.name.as_str().to_variant());
            let color = def.icon_color;
            row.set(
                &GString::from("iconColor"),
                Color::from_rgba(color.r, color.g, color.b, color.a),
            );
            out.push(&row);
        }
        out
    }

    /// `control.saves.deleteAll()` (`@settings.clearsaves`): removes every
    /// non-sector save slot.
    #[func]
    pub fn clear_saves(&mut self) -> bool {
        let Some(registry) = self.registry.as_ref() else {
            return false;
        };
        settings_hub::with_settings(|settings| {
            self.saves.load(&NativeFs, &self.paths, registry, settings);
        });
        match self.saves.delete_all(&NativeFs) {
            Ok(()) => {
                log::info!(
                    "cleared non-sector saves ({} slot(s) left)",
                    self.saves.slots.len()
                );
                true
            }
            Err(error) => {
                log::warn!("clear saves failed: {error}");
                false
            }
        }
    }

    /// `@settings.clearresearch`: `Universe.clearLoadoutInfo`, every tech node
    /// reset and every content unlock cleared.
    #[func]
    pub fn clear_research(&mut self) -> bool {
        let Some(mut registry) = self.registry.take() else {
            return false;
        };
        self.with_campaign_mut(|campaign, settings| {
            campaign.universe.clear_loadout_info(settings);
        });
        let cleared = settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            registry.clear_unlocks(&mut store, None)
        });
        tech_tree::reset_all(&mut registry);
        settings_hub::with_settings(|store| {
            store.remove("unlocks");
        });
        self.registry = Some(registry);
        self.push_campaign_to_runtime();
        self.persist();
        log::info!("cleared research ({cleared} unlock(s))");
        true
    }

    /// `@settings.clearplanetresearch`: the planet's tech tree, its database
    /// content unlocks and the launch loadout info.
    #[func]
    pub fn clear_planet_research(&mut self, planet: GString) -> bool {
        let name = planet.to_string();
        let Some(mut registry) = self.registry.take() else {
            return false;
        };
        let Some((planet_id, tree)) =
            planet_def(&registry, &name).map(|def| (def.id, def.tech_tree))
        else {
            self.registry = Some(registry);
            log::warn!("clear_planet_research: unknown planet `{name}`");
            return false;
        };
        self.with_campaign_mut(|campaign, settings| {
            campaign.universe.clear_loadout_info(settings);
        });
        let cleared = settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            registry.clear_unlocks(&mut store, Some(planet_id))
        });
        if let Some(tree) = tree {
            tech_tree::reset_tree(&mut registry, tree);
        }
        settings_hub::with_settings(|store| {
            store.remove("unlocks");
        });
        self.registry = Some(registry);
        self.push_campaign_to_runtime();
        self.persist();
        log::info!("cleared `{name}` research ({cleared} unlock(s))");
        true
    }

    /// `@settings.clearcampaignsaves`: every planet's stats and sector info are
    /// cleared and every sector save slot deleted.
    #[func]
    pub fn clear_campaign_saves(&mut self) -> bool {
        self.clear_campaign_saves_for(None)
    }

    /// `@settings.clearplanetcampaignsaves`: planet-scoped campaign-save clear.
    #[func]
    pub fn clear_planet_campaign_saves(&mut self, planet: GString) -> bool {
        let name = planet.to_string();
        let Some(id) = self
            .registry
            .as_ref()
            .and_then(|registry| planet_id_by_name(registry, &name))
        else {
            log::warn!("clear_planet_campaign_saves: unknown planet `{name}`");
            return false;
        };
        self.clear_campaign_saves_for(Some(id))
    }

    /// Content boot error, if any (diagnostics).
    #[func]
    pub fn content_error(&self) -> GString {
        GString::from(self.content_error.as_deref().unwrap_or(""))
    }

    /// Rebuilds settings, content and campaign state from disk
    /// (`importData`'s `settings.load()` + `state = new GameState()` pair).
    #[func]
    pub fn reload_from_disk(&mut self) -> bool {
        self.bootstrap_with(false);
        self.content_error.is_none()
    }

    /// `@settings.cleardata` host half: drops the in-memory campaign, registry,
    /// save and settings state so the exit persist cannot resurrect the data
    /// the UI is about to delete.
    #[func]
    pub fn clear_in_memory(&mut self) {
        settings_hub::with_settings(|store| store.clear());
        self.campaign = None;
        self.registry = None;
        // The memos hold content-derived data (schematic decode) and a
        // filesystem listing under the root the data clear is about to wipe.
        self.invalidate_caches();
        self.saves = Saves::new();
        self.session = PlaySession::default();
        self.runtime_installed = false;
        self.content_error = None;
    }
}

impl MindCampaign {
    /// Clears the filesystem-derived memos (finding #9). A content (re)boot can
    /// change block ids/names the `Schematics` decode resolves, and
    /// `bootstrap_with` re-points `self.paths`; both invalidate every entry.
    pub(super) fn invalidate_caches(&self) {
        self.schematics_cache.borrow_mut().clear();
        self.save_slots_cache.borrow_mut().clear();
    }

    /// `Control.playSector`'s pre-launch save (`Control.java:437-441`): when a
    /// match is live and a slot is current, the outgoing sector is written
    /// before the launch replaces the world, so the player's construction since
    /// the last autosave survives the switch (CG-SECTOR-02). The destination
    /// launch re-points `Saves.current` afterwards (`prepare_sector_slot` on a
    /// fresh launch, `set_current_by_file` on a resume), so Java's
    /// `resetSave()` has no observable equivalent here.
    ///
    /// MUST run while the host still holds the OUTGOING world: the planet
    /// dialog pre-loads the destination (`MindSimHost.load_sector`) before
    /// `start_sector`, so the UI calls this first
    /// ([`Self::save_outgoing_sector`], `PlanetDialog.playSelected`'s
    /// "save before launch"); the accelerator path calls it from
    /// `pump_accelerator_launch` before its own load. Returns whether a save
    /// was queued.
    pub(super) fn save_outgoing_slot(&mut self) -> bool {
        if !self.session.phase.is_game() {
            return false;
        }
        let Some(file) = self.saves.get_current().map(|slot| slot.file.clone()) else {
            return false;
        };
        // `SaveVersion.writeMeta` (`SaveVersion.java:117-118`): prepare/save the
        // sector record as part of the save, like every other save path.
        self.persist_sector_record();
        // `GameService.storeStats` on a sector switch: the live runtime (and
        // its tracker) is about to be replaced by the destination world, so
        // mirror the tracker now and write it through the settings store.
        self.sync_service();
        self.store_service();
        let Some(mut host) = self.host() else {
            return false;
        };
        self.queue_slot_save(
            &mut host,
            &file,
            None,
            true,
            "saved outgoing sector before launch",
        );
        true
    }

    /// `run_exit_save` / capture bookkeeping: writes the live sector's stored
    /// record (`SectorInfo.prepare` + `Sector.saveInfo`) so captures, wave
    /// progress, item pools and the production windows survive a client restart
    /// (PAR-0081/PAR-0082).
    pub(super) fn persist_sector_record(&mut self) {
        // The record writer takes the registry explicitly (finding #1); take the
        // facade copy for the call and restore it afterwards so `&mut self` is
        // free for the campaign/settings half.
        let registry = self.registry.take();
        if with_host_runtime(self.host(), |runtime| {
            self.persist_sector_record_in(Some(runtime), registry.as_ref());
        })
        .is_some()
        {
            self.registry = registry;
            return;
        }
        self.persist_sector_record_in(None, registry.as_ref());
        self.registry = registry;
    }

    /// `persist_sector_record` body. `runtime` is the already-taken live
    /// runtime when the caller holds it (`pump_runtime` events); with `None`
    /// the host runtime (when installed) is taken for the read and put back.
    ///
    /// `registry` is the explicit content registry (finding #1): `pump_runtime`
    /// has `self.registry` `take()`n for its whole event window, so reading the
    /// facade field here silently skipped the capture/game-over record write.
    /// Every caller owns the borrow (the `&mut self` below cannot hold one).
    pub(super) fn persist_sector_record_in(
        &mut self,
        mut runtime: Option<&mut CampaignRuntime>,
        registry: Option<&ContentRegistry>,
    ) {
        let Some((planet, sector_id)) = self.session.sector else {
            return;
        };
        let Some(registry) = registry else {
            return;
        };
        let Some(planet_name) = registry.planet(planet).map(|record| record.name.clone()) else {
            return;
        };
        let default_team = self.session.rules.default_team;
        let snapshot = self
            .host()
            .and_then(|host| host.bind().campaign_core_snapshot(default_team));
        // `play::live_sector_record` (H02) is the record half: prepare from the
        // live core snapshot (PAR-0082) then copy the wave/rules fields. The
        // live runtime owns the authoritative record (the campaign tick folds
        // the core/production deltas into it); the facade clone only mirrors on
        // events, so read the runtime's copy when the caller holds it (or the
        // host does), else fall back to the facade copy.
        let record = match runtime.as_deref_mut() {
            Some(live) => {
                play::live_sector_record(&self.session, &live.campaign, registry, snapshot.as_ref())
            }
            None => {
                let live = self
                    .read_live_campaign(|campaign| {
                        play::live_sector_record(
                            &self.session,
                            campaign,
                            registry,
                            snapshot.as_ref(),
                        )
                    })
                    .flatten();
                live.or_else(|| {
                    play::live_sector_record(
                        &self.session,
                        self.campaign.as_ref()?,
                        registry,
                        snapshot.as_ref(),
                    )
                })
            }
        };
        let Some(record) = record else {
            return;
        };
        // Java has one object graph (`Vars.universe`): the prepared record is
        // THE live record. Without writing it back, the next runtime->facade
        // mirror overwrites the facade with the unprepared copy and the
        // lost-sector replay's `sectorDataMatches` (`SectorInfo.java:98-103`)
        // fails on an in-process re-attack (H04 V1).
        if let Some(live) = runtime
            && let Some(sector) = live.campaign.sector_mut(planet, sector_id)
        {
            sector.info.info = record.clone();
        }
        // Keep the facade copy's record in step for the dialog views.
        if let Some(campaign) = self.campaign.as_mut()
            && let Some(sector) = campaign.sector_mut(planet, sector_id)
        {
            sector.info.info = record.clone();
        }
        let key = mind_core::game::sector::info_key(&planet_name, sector_id);
        if let Err(error) = settings_hub::with_settings(|store| store.put_json(&key, &record)) {
            log::warn!("sector record persist failed: {error}");
        }
        // `put_json` only marks the store dirty; flush so the record is on disk
        // before any restart (the capture path calls this after `pump_runtime`).
        self.flush_settings("sector record flush failed");
    }

    /// Shared `SettingsMenuDialog` clear-campaign-saves body (`only` scopes to
    /// one planet; `None` covers every planet).
    fn clear_campaign_saves_for(&mut self, only: Option<PlanetId>) -> bool {
        let Some(registry) = self.registry.take() else {
            return false;
        };
        // T1: count the planet list on the mirrored (live) campaign; the
        // clearing runs on the live campaign through `with_campaign_mut`.
        self.mirror_runtime_campaign();
        let planets = self
            .campaign
            .as_ref()
            .map(|campaign| {
                campaign
                    .planets
                    .values()
                    .filter(|planet| only.is_none_or(|target| target == planet.id))
                    .count()
            })
            .unwrap_or(0);
        if planets == 0 {
            self.registry = Some(registry);
            return false;
        }
        let paths = self.paths.clone();
        // `Campaign::clear_saves` (H01): the in-memory half (stats + every
        // sector's save/beingPlayed/record) and the `[file, backup]` list in
        // facade order; the filesystem deletion stays here, errors ignored like
        // `Slot.delete`.
        let Some(files) = self
            .with_campaign_mut(|campaign, settings| campaign.clear_saves(settings, &paths, only))
        else {
            self.registry = Some(registry);
            return false;
        };
        for file in &files {
            let _ = NativeFs.delete(file);
        }

        // Re-list saves (a slot may have been written this session) and drop
        // the matching sector slots.
        settings_hub::with_settings(|settings| {
            self.saves.load(&NativeFs, &self.paths, &registry, settings);
        });
        if let Err(error) = self.saves.delete_sectors(&NativeFs, &registry, only) {
            log::warn!("clear campaign saves: {error}");
        }
        self.registry = Some(registry);
        self.push_campaign_to_runtime();
        self.persist();
        log::info!("cleared campaign saves for {planets} planet(s)");
        true
    }
}
