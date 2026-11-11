// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Frame pump: runtime tick, capture bookkeeping, toasts and the turn/wave entry points.
//! Upstream: core/src/mindustry/core/Control.java (update); core/src/mindustry/core/Logic.java (sectorCapture, runWave); game/Universe.java (runTurn); ui/fragments/HudFragment.java (toasts)

use super::events::dispatch_event_rows;
use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Forces sector capture (`Logic.sectorCapture`), routing through the live
    /// runtime when one is installed so events and bookkeeping stay in sync.
    #[func]
    pub fn capture_sector(&mut self) -> bool {
        let captured = with_host_runtime(self.host(), |runtime| {
            // `SectorCaptureEvent` listener inputs: Java fires the event
            // inside `Logic.sectorCapture` before `rules.attackMode =
            // false`, so snapshot the attacking flag and the pre-capture
            // loss counter first (finding #2).
            let was_attack_mode = runtime.session.rules.attack_mode;
            let buildings_destroyed = runtime.session.stats.buildings_destroyed;
            let events = sector_capture(&mut runtime.session, &mut runtime.campaign);
            let ok = !events.is_empty();
            for event in events {
                // Shared with `game_state_tick_system` (GAPS #68 /
                // finding #2): the `GameService` capture hooks
                // (`on_sector_capture`) and the `Logic.java:117-121`
                // derelict-wave-team queue now run on this path too, so a
                // capture here counts for achievements.
                if let mind_core::game::play::PlayEvent::SectorCapture { planet, .. } = &event {
                    on_captured_sector(runtime, *planet, was_attack_mode, buildings_destroyed);
                }
                runtime.events.push(event);
            }
            ok
        })
        .unwrap_or(false);
        if captured {
            self.pump_runtime();
            // `Logic.sectorCapture`'s "save, just in case": the capture
            // bookkeeping must be on disk before a restart (PAR-0081).
            self.persist_sector_record();
            // `onSectorCaptured` -> `reloadMeshAsync` (T14).
            self.planet_mesh_cache = None;
            return true;
        }
        // Menu fallback on the facade copy.
        let Some(mut campaign) = self.campaign.take() else {
            return false;
        };
        let events = sector_capture(&mut self.session, &mut campaign);
        self.campaign = Some(campaign);
        self.planet_mesh_cache = None;
        !events.is_empty()
    }

    /// Runs one production turn; returns success. Runs on the live runtime
    /// campaign when one is installed (T1: one authoritative campaign).
    #[func]
    pub fn run_turn(&mut self) -> bool {
        let Some(registry) = self.registry.take() else {
            return false;
        };
        // `CampaignRuntime::run_turn_now` (H02): the manual turn path that never
        // checks `turn_due`; it seeds `JavaRandom::new(turn + 1)` with the live
        // `TurnContext` and applies the played-sector invasions to its session.
        let live = with_host_runtime(self.host(), |runtime| {
            settings_hub::with_settings(|settings| runtime.run_turn_now(&registry, settings))
        });
        let report = match live {
            Some(report) => report,
            None => {
                // Menu/harness facade without a live runtime: `with_host_runtime`
                // yielded no context, so the turn runs with
                // `TurnContext::default()` (the replaced `unwrap_or_default`).
                let Some(report) = self.with_campaign_mut(|campaign, settings| {
                    let turn = campaign.universe.turn;
                    campaign.run_turn(
                        &registry,
                        settings,
                        &TurnContext::default(),
                        &mut JavaRandom::new(turn as u64 + 1),
                    )
                }) else {
                    self.registry = Some(registry);
                    return false;
                };
                report
            }
        };
        // Played-sector invasions hit the live session rules (T4:
        // `Universe.java:550-555`); `run_turn_now` already applied them, so the
        // facade only mirrors the resulting rules.
        if report.events.iter().any(|event| {
            matches!(
                event,
                mind_core::game::universe::CampaignEvent::SectorInvasion { played: true, .. }
            )
        }) {
            let _ = with_host_runtime(self.host(), |runtime| {
                self.session.rules = runtime.session.rules.clone();
            });
        }
        // T7: toasts for off-play sectors (`HudFragment.java:261-275`).
        self.toast_campaign_events(&report.events);
        self.registry = Some(registry);
        self.persist();
        report.turn > 0
    }

    /// Advances the wave counter (`Logic.runWave`), mirroring into the live
    /// runtime when one is installed. The live unit runtime spawns the wave on
    /// the next `TickSet::RunWave` tick (`WaveSpawner.spawnEnemies`).
    #[func]
    pub fn run_wave(&mut self) -> bool {
        let _ = run_wave_campaign(&mut self.session);
        if let Some((_, spawn_index)) = with_host_runtime(self.host(), |runtime| {
            run_wave_campaign(&mut runtime.session)
        }) && let Some(mut host) = self.host()
        {
            host.bind_mut().request_wave_spawn(spawn_index);
        }
        true
    }
}

impl MindCampaign {
    /// Pumps the live campaign runtime: drains tick events, applies capture
    /// bookkeeping (stats, auto-unlocks, capture replacements), advances the
    /// `Saves` policy (playtime + autosave cadence), runs due campaign turns
    /// and saves. No-op without a host/runtime.
    pub(super) fn pump_runtime(&mut self) {
        let Some(mut host) = self.host() else {
            // Menu/harness facade: the `capture_sector` fallback queues rows on
            // the facade session, so drain them here too (PAR-0577 residual).
            self.drain_event_outbox(None);
            return;
        };
        let Some(mut runtime) = host.bind_mut().take_campaign_runtime() else {
            self.drain_event_outbox(None);
            return;
        };
        self.drain_event_outbox(Some(&mut runtime));
        // The runtime's tracker is authoritative while a sector is live; mirror
        // it every pump so the persist path and the UI queries see the same
        // `Vars.service` state the campaign systems just updated (GAPS #68).
        self.service = runtime.service.clone();
        let events = runtime.take_events();
        let mut dirty = false;

        // `Control.update` -> `saves.update()`: playtime accrual and the
        // autosave cadence on the fixed-tick delta (deviation 11).
        let (saves_update, saves_result, tick, autosaved) =
            self.pump_saves_policy(&mut host, &mut runtime);
        dirty |= autosaved;

        let mut registry = self.registry.take();
        // `SectorCaptureEvent`'s capture replacements (H02
        // `play::capture_replacements`); the event gate stays here.
        let replacements: Vec<(String, String)> = if events.iter().any(|event| {
            matches!(
                event,
                mind_core::game::play::PlayEvent::SectorCapture { .. }
            )
        }) {
            registry
                .as_ref()
                .map(|registry| play::capture_replacements(registry, &runtime.session))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        dirty |= self.handle_play_events(&mut host, &mut runtime, registry.as_mut(), &events, tick);

        // T2: produced/dumped content unlocks (`UnlockableContent.unlock()` ->
        // `UnlockEvent` -> `Control.java:160-174`).
        let (unlocked_content, unlocked) = match registry.as_mut() {
            Some(registry) => self.resolve_pending_unlocks(&mut runtime, registry),
            None => (Vec::new(), false),
        };
        dirty |= unlocked;
        // H04 T6: legacy launch pads queue destination records for the host to
        // flush (`Sector.saveInfo` after `addItems`).
        dirty |= Self::flush_pending_info_saves(&mut runtime);
        self.registry = registry;

        let unlocked_block = unlocked_content
            .iter()
            .any(|content| content.type_ == ContentType::Block);
        if unlocked_block {
            // `UnlockEvent` with a block reached the facade: the placement
            // palette rebuilds (`PlacementFragment.java:92-96`).
            let _ = self.base_mut().emit_signal("block_unlocked", &[]);
        }
        if let Some(registry) = self.registry.as_ref() {
            // `Control.java:160-165`: each drained unlock shows the toast when
            // `showUnlock()` is true.
            self.push_unlock_toasts(registry, &unlocked_content);
        }

        if !replacements.is_empty() {
            let replaced = host.bind_mut().apply_capture_replacements(&replacements);
            if replaced > 0 {
                dirty = true;
            }
        }

        // Auto campaign turns (`Universe.update` turnCounter).
        dirty |= self.run_due_turn(&mut runtime);

        // Mirror the authoritative runtime state into the facade for getters.
        let mirrored = !events.is_empty();
        self.mirror_session_from(&runtime, mirrored, dirty);
        host.bind_mut().put_campaign_runtime(runtime);
        // The facade campaign copy feeds the views/persist; keep its playtime in
        // step with the runtime accrual (a mirrored clone already carries it).
        if !mirrored
            && saves_result.playtime_added > 0
            && saves_update.is_campaign
            && let Some((planet, _)) = self.session.sector
            && let Some(stats) = self
                .campaign
                .as_mut()
                .and_then(|campaign| campaign.stats_for_mut(planet))
        {
            stats.playtime += saves_result.playtime_added;
        }
        if dirty {
            self.persist();
        }
    }

    /// PAR-0577 residual: drain the play-flow `EventRow` outbox into the
    /// `EventType` bus. The runtime session is authoritative while a sector is
    /// live, so its outbox is the one dispatched; the facade session's rows are
    /// a clone of it (the `start_*` flows install the runtime from
    /// `self.session.clone()`), and dropping them keeps a later fallback drain
    /// from replaying the same rows. Without a runtime the facade session is
    /// the only outbox (`capture_sector`'s menu fallback).
    fn drain_event_outbox(&mut self, runtime: Option<&mut CampaignRuntime>) {
        let rows = match runtime {
            Some(runtime) => {
                let rows = runtime.session.take_event_rows();
                let _ = self.session.take_event_rows();
                rows
            }
            None => self.session.take_event_rows(),
        };
        dispatch_event_rows(&mut self.events, rows);
    }

    /// `Control.update` -> `saves.update()`: the playtime accrual and the
    /// autosave cadence on the fixed-tick delta (deviation 11), the
    /// `SaveVersion.writeMeta` sector-record + service write at the autosave
    /// boundary, and the `Saves.saving` finish tick. Returns
    /// `(SavesUpdate, SavesUpdateResult, tick, queued_an_autosave)`.
    fn pump_saves_policy(
        &mut self,
        host: &mut Gd<MindSimHost>,
        runtime: &mut CampaignRuntime,
    ) -> (SavesUpdate, SavesUpdateResult, u64, bool) {
        let tick = host.bind().get_tick().max(0) as u64;
        let delta_ticks = tick.saturating_sub(self.last_pump_tick);
        self.last_pump_tick = tick;
        let saves_update = SavesUpdate {
            delta_ticks,
            is_game: runtime.session.phase.is_game(),
            paused_with_dialog: host.bind().is_paused() && self.ui_has_dialog(),
            game_over: runtime.session.game_over,
            is_campaign: runtime.session.is_campaign(),
        };
        let saves_result = if delta_ticks > 0 {
            settings_hub::with_settings(|store| {
                let interval = store.get_i32(KEY_SAVE_INTERVAL, DEFAULT_SAVE_INTERVAL_SECONDS);
                self.saves.update_policy(saves_update, store, interval)
            })
        } else {
            SavesUpdateResult::default()
        };
        // `SaveVersion.writeMeta` reads `control.saves.getTotalPlaytime()`; pass
        // the host-accrued value into the sim before any save boundary.
        let total_playtime = self.saves.total_playtime;
        host.bind_mut().set_save_playtime(total_playtime);
        // `Saves.java:210-212`: campaign playtime also lands in the planet stats
        // (the runtime copy is authoritative while the sector is live).
        if saves_result.playtime_added > 0
            && saves_update.is_campaign
            && let Some((planet, _)) = runtime.session.sector
            && let Some(stats) = runtime.campaign.stats_for_mut(planet)
        {
            stats.playtime += saves_result.playtime_added;
        }
        let mut autosaved = false;
        if saves_result.autosaved
            && let Some(file) = self.saves.current_slot().map(|slot| slot.file.clone())
        {
            // `SaveVersion.writeMeta` -> `state.rules.sector.saveInfo()`: the
            // live sector record (core snapshot, items, production, wave) lands
            // in settings next to the autosave (PAR-0153). The writer takes the
            // registry explicitly (finding #1): no facade field is borrowed
            // across the `&mut self` call.
            let registry = self.registry.take();
            self.persist_sector_record_in(Some(runtime), registry.as_ref());
            self.registry = registry;
            // `GameService.storeStats` at the autosave boundary: the tracker's
            // stats/achievements land in the settings store alongside the
            // sector record (the exit persist flushes the file).
            self.store_service();
            // `Saves.SaveSlot.save` -> `savePreview()` (autosave path).
            self.queue_slot_save(
                host,
                &file,
                Some(tick.saturating_add(3)),
                false,
                "autosave queued",
            );
            autosaved = true;
        }
        if self.saving_finish_tick.is_some_and(|finish| tick >= finish) {
            self.saves.finish_saving();
            self.saving_finish_tick = None;
        }
        (saves_update, saves_result, tick, autosaved)
    }

    /// The `PlayEvent` listener half: `SectorCapture` (auto-unlocks, immediate
    /// sector save, record write), `CampaignComplete` (signal) and
    /// `GameOver`/`SectorLose` (immediate game-over save + record write).
    /// Returns whether anything needs persisting.
    fn handle_play_events(
        &mut self,
        host: &mut Gd<MindSimHost>,
        runtime: &mut CampaignRuntime,
        mut registry: Option<&mut ContentRegistry>,
        events: &[mind_core::game::play::PlayEvent],
        tick: u64,
    ) -> bool {
        let mut dirty = false;
        for event in events {
            match event {
                mind_core::game::play::PlayEvent::SectorCapture { .. } => {
                    self.apply_capture_bookkeeping(runtime, registry.as_deref_mut());
                    dirty = true;
                    // `Logic.sectorCapture` (`Logic.java:398-401`):
                    // `control.saves.saveSector(state.rules.sector)` — the
                    // captured world is saved immediately, like the GameOver arm.
                    if let Some(file) = self.saves.current_slot().map(|slot| slot.file.clone()) {
                        self.queue_slot_save(
                            host,
                            &file,
                            Some(tick.saturating_add(3)),
                            false,
                            "capture save queued",
                        );
                    }
                    self.persist_sector_record_in(Some(runtime), registry.as_deref());
                }
                mind_core::game::play::PlayEvent::CampaignComplete { .. } => {
                    self.campaign_complete = true;
                    let _ = self.base_mut().emit_signal("campaign_complete", &[]);
                }
                mind_core::game::play::PlayEvent::GameOver { .. }
                | mind_core::game::play::PlayEvent::SectorLose { .. } => {
                    // `Sector.isBeingPlayed` (`Sector.java:148-151`) is already
                    // false here: the campaign `GameOverEvent` arm of
                    // `game_state_tick_system` runs `play::apply_sector_loss`,
                    // which clears the flag on this same `CampaignRuntime` (the
                    // pump took that instance out of the host), and the loss
                    // event is only ever queued by that path. No local clear.
                    // `Control` GameOverEvent (`Control.java:188-195`): save the
                    // game-over state immediately, so a later re-attack loads
                    // the destroyed base (PAR-0084).
                    if let Some(file) = self.saves.current_slot().map(|slot| slot.file.clone()) {
                        self.queue_slot_save(
                            host,
                            &file,
                            Some(tick.saturating_add(3)),
                            false,
                            "game-over save queued",
                        );
                    }
                    // `SaveVersion.writeMeta` persists the sector record next to
                    // the save (spawn position + prepared core state), which the
                    // lost-sector replay reads on the re-attack.
                    self.persist_sector_record_in(Some(runtime), registry.as_deref());
                    dirty = true;
                }
                _ => {}
            }
        }
        dirty
    }

    /// `tech_tree::apply_pending_unlocks` (H02) for the runtime's queued
    /// production/liquid/unit unlocks: resolve + persist each, run
    /// `checkAutoUnlocks()`, log, buffer the toast names and refresh the
    /// runtime's unlocked-content cache. Returns `(contents, dirty)`.
    fn resolve_pending_unlocks(
        &mut self,
        runtime: &mut CampaignRuntime,
        registry: &mut ContentRegistry,
    ) -> (Vec<ContentRef>, bool) {
        if runtime.pending_unlocks.is_empty() {
            return (Vec::new(), false);
        }
        let unlocks = std::mem::take(&mut runtime.pending_unlocks);
        let outcome = settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            tech_tree::apply_pending_unlocks(registry, &mut store, &runtime.campaign, unlocks)
        });
        if outcome.names.is_empty() {
            return (Vec::new(), false);
        }
        log::info!(
            "campaign unlock: {} produced, {} auto-unlocked",
            outcome.produced,
            outcome.auto
        );
        self.unlocked_since_last_take.extend(outcome.names);
        let unlocked = tech_tree::unlocked_content_names(registry);
        runtime.set_unlocked_content(unlocked);
        (outcome.contents, true)
    }

    /// H04 T6: legacy launch pads queue destination records for the host to
    /// flush (`Sector.saveInfo` after `addItems`). Returns whether any landed.
    fn flush_pending_info_saves(runtime: &mut CampaignRuntime) -> bool {
        if runtime.pending_info_saves.is_empty() {
            return false;
        }
        for (planet, sector_id) in std::mem::take(&mut runtime.pending_info_saves) {
            settings_hub::with_settings(|settings| {
                let _ = runtime
                    .campaign
                    .save_sector_info(settings, planet, sector_id);
            });
        }
        true
    }

    /// Auto campaign turns (`Universe.update` turnCounter):
    /// `CampaignRuntime::run_due_turn` (H02) clears `turn_due`, seeds
    /// `JavaRandom::new(turn + 1)`, runs the turn and applies the played-sector
    /// invasions to the live session (T4). Returns whether a turn ran.
    fn run_due_turn(&mut self, runtime: &mut CampaignRuntime) -> bool {
        if !runtime.turn_due {
            return false;
        }
        let report = self.registry.as_ref().and_then(|registry| {
            settings_hub::with_settings(|settings| runtime.run_due_turn(registry, settings))
        });
        let Some(report) = report else {
            // No registry (content boot failed / data cleared): the turn is
            // skipped but the flag still clears, like the replaced block.
            runtime.turn_due = false;
            return false;
        };
        log::info!(
            "campaign turn {} ({} event(s))",
            report.turn,
            report.events.len()
        );
        // T7: toasts for off-play sectors (`HudFragment.java:261-275`).
        if !report.events.is_empty() {
            self.toast_campaign_events(&report.events);
        }
        true
    }

    /// Mirrors the authoritative runtime state into the facade for getters.
    /// Scalars/objectives stay fresh every pump; the full session (teams) clones
    /// only when an event landed (`mirrored`), and the campaign object only when
    /// an event landed or the campaign changed (`mirrored || dirty`) — the
    /// facade must carry the runtime's campaign before `persist()` runs (T1).
    fn mirror_session_from(&mut self, runtime: &CampaignRuntime, mirrored: bool, dirty: bool) {
        self.session.wave = runtime.session.wave;
        self.session.wavetime = runtime.session.wavetime;
        self.session.game_over = runtime.session.game_over;
        self.session.won = runtime.session.won;
        self.session.enemies = runtime.session.enemies;
        self.session.spawn_count = runtime.session.spawn_count;
        self.session.is_spawning = runtime.session.is_spawning;
        self.session.sector = runtime.session.sector;
        self.session.attack_after_waves = runtime.session.attack_after_waves;
        self.session.rules = runtime.session.rules.clone();
        self.session.objectives = runtime.session.objectives.clone();
        // H09 T10: `game_stats_json` reads the live counters (small clone).
        self.session.stats = runtime.session.stats.clone();
        if mirrored {
            self.session = runtime.session.clone();
        }
        if mirrored || dirty {
            self.campaign = Some(runtime.campaign.clone());
        }
    }

    /// `SectorCaptureEvent` listener half: tech auto-unlocks (including
    /// `SectorComplete` objectives) and campaign-complete detection.
    fn apply_capture_bookkeeping(
        &mut self,
        runtime: &mut CampaignRuntime,
        registry: Option<&mut ContentRegistry>,
    ) {
        let Some(registry) = registry else {
            return;
        };
        let unlocked = settings_hub::with_settings(|settings| {
            let mut store = SettingsUnlockStore::new(settings);
            // `tech_tree::check_campaign_auto_unlocks` (H02): the shared
            // `CampaignObjectiveContext` + `check_auto_unlocks` block.
            tech_tree::check_campaign_auto_unlocks(registry, &mut store, &runtime.campaign)
        });
        if !unlocked.is_empty() {
            log::info!("campaign capture auto-unlocked {} content", unlocked.len());
        }
        // `Control.java:160-165` (`UnlockEvent`) half.
        self.push_unlock_toasts(registry, &unlocked);
        // `UnlockEvent` with a block: the palette rebuilds on capture unlocks
        // (`PlacementFragment.java:92-96`).
        self.emit_block_unlocked_if_any(&unlocked);
    }

    /// `HudFragment.showToast` for `CampaignEvent`s (`HudFragment.java:261-275`):
    /// `sector.attacked` with the warning icon on a `SectorInvasion`. The
    /// off-play `sector.capture`/`sector.lost` toasts land when the
    /// `CampaignEvent` variants exist (seam §6). The label itself is suppressed
    /// in the menu by `ui_root.gd _on_toast` (`HudFragment.java:753-754`).
    fn toast_campaign_events(&self, events: &[mind_core::game::universe::CampaignEvent]) {
        use mind_core::game::universe::CampaignEvent;
        for event in events {
            let CampaignEvent::SectorInvasion { planet, sector, .. } = event else {
                continue;
            };
            let planet_record = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.planet(*planet));
            let planet_name = self
                .campaign
                .as_ref()
                .and_then(|campaign| campaign.planet_name(*planet))
                .unwrap_or_default()
                .to_owned();
            // `Sector.name()` (`Sector.java:153-164`) through the one shared
            // implementation; the record can be missing only if the event's
            // sector is out of range (the port's defensive fallback is the bare
            // grid index, never Java's `<planet>-<id>`).
            let name = planet_record
                .zip(
                    self.campaign
                        .as_ref()
                        .and_then(|campaign| campaign.sector(*planet, *sector)),
                )
                .map(|(planet, record)| record.display_name(planet))
                .unwrap_or_else(|| sector.to_string());
            log::info!("campaign event: `{planet_name}:{sector}` invaded (toast sector.attacked)");
            let Some(mut hud) = self.autoload("MindHud") else {
                continue;
            };
            let text = self
                .assets_string(
                    "bundle_format",
                    &[
                        GString::from("sector.attacked").to_variant(),
                        PackedStringArray::from(vec![GString::from(name.as_str())]).to_variant(),
                    ],
                )
                .unwrap_or_else(|| format!("Sector {name} under attack!"));
            let _ = hud.call(
                "push_toast",
                &[text.to_variant(), GString::from("warning").to_variant()],
            );
        }
    }
}
