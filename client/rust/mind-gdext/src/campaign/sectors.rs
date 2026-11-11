// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Sector records and the planet dialog's sector commands (launch destination,
//! capture state, abandon, rename/icon).
//! Upstream: ui/dialogs/PlanetDialog.java; type/Sector.java; world/blocks/campaign/LaunchPad.java

use super::text::resolve_preset_description;
use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// The live record of one sector as JSON (T1 seam): the `SectorInfo` serde
    /// (camelCase) plus `"beingPlayed"` and `"hasSave"`; `"{}"` when unknown.
    /// Reads the runtime copy while a sector is installed, else the facade.
    #[func]
    pub fn sector_record_json(&self, planet: GString, sector: i64) -> GString {
        if !(0..=i64::from(u16::MAX)).contains(&sector) {
            return empty_object();
        }
        let planet_name = planet.to_string();
        let sector_id = sector as u16;
        // The runtime copy while a sector is installed, else the facade copy.
        // `Sector::record_json` (H01) is the record projection.
        let value = self
            .read_live_campaign(|campaign| {
                campaign
                    .sector_by_name(&planet_name, sector_id)
                    .map(|(_, sector)| sector.record_json())
            })
            .flatten();
        match value {
            Some(value) => value_gstring(&value),
            None => empty_object(),
        }
    }

    /// `LaunchPad.buildConfiguration` -> `ui.planet.showSelect` result
    /// (`LaunchPad.java:185-203`): sets the played sector's export destination
    /// to `planet:sector`; `-1` clears it. H05 wires the pad config button.
    #[func]
    pub fn set_launch_destination(&mut self, planet: GString, sector: i64) -> bool {
        let planet_name = planet.to_string();
        let destination = if sector < 0 {
            None
        } else if sector <= i64::from(u16::MAX) {
            Some(mind_core::io::json::content_serde::SectorKey::format(
                &planet_name,
                sector as u16,
            ))
        } else {
            return false;
        };
        let Some((planet_id, sector_id)) = self.session.sector else {
            return false;
        };
        let Some(registry) = self.registry.take() else {
            return false;
        };
        let updated = self.with_campaign_mut(|campaign, settings| {
            let previous = campaign
                .sector(planet_id, sector_id)
                .and_then(|sector| sector.info.info.destination.clone());
            let Some(record) = campaign.sector_mut(planet_id, sector_id) else {
                return false;
            };
            record.info.info.destination = destination.clone();
            let _ = record.save_info(settings, &planet_name);
            // `if(prev != null) prev.info.refreshImportRates(planet)`.
            if let Some(previous) = previous
                && let Some(prev_planet) = previous.planet()
                && let Some(prev_sector) = previous.sector()
                && let Some(prev_planet_id) = campaign.planet_id_by_name(prev_planet)
            {
                campaign.refresh_sector_import_rates(&registry, prev_planet_id, prev_sector);
            }
            true
        });
        self.registry = Some(registry);
        if updated != Some(true) {
            return false;
        }
        self.persist();
        true
    }

    /// M5 campaign dialog read models as live JSON (`CampaignViews`).
    ///
    /// The planet-14 dialogs prefer this endpoint over
    /// `MindUi.campaign_views()`'s deterministic fixture: sector
    /// owned/captured/locked flags, stats and planet rows are projected from
    /// the running `Campaign` every call, so a capture (`capture_sector`) is
    /// visible on the next refresh. `planet` is a content name; empty selects
    /// the planet being played, else the default starting planet.
    #[func]
    pub fn campaign_views_json(&self, planet: GString) -> GString {
        let Some(campaign) = self.campaign.as_ref() else {
            return empty_object();
        };
        let Some(registry) = self.registry.as_ref() else {
            return empty_object();
        };
        let name = planet.to_string();
        let active = if name.is_empty() {
            self.session
                .sector
                .map(|(planet, _)| planet)
                .or_else(|| campaign.planet_id_by_name("serpulo"))
                .or_else(|| {
                    campaign
                        .order
                        .first()
                        .and_then(|id| campaign.planets.get(id))
                        .map(|planet| planet.id)
                })
        } else {
            campaign.planet_id_by_name(&name)
        };
        let Some(active) = active else {
            return empty_object();
        };
        let schematics = self.load_schematics(registry);
        // The `findLauncher` actual launch sector: the played sector, cleared
        // after game over (`PlanetDialog`: `gameOver ? null : state.getSector()`).
        let played = if self.session.game_over {
            None
        } else {
            self.session.sector
        };
        // `Sector.isAttacked()`/`isCaptured()` read the live rules while the
        // sector is being played (`Sector.java:129-131,181-184`), so the panel
        // shows `Under attack!` and the stats dialog its `Threat:` row
        // (PAR-0503). `gameOver` clears the "being played" state in Java too.
        let playing_rules = if self.session.game_over {
            None
        } else {
            self.session
                .sector
                .map(|_| (self.session.rules.waves, self.session.rules.attack_mode))
        };
        // `loadout_of` reads the preset map header for the disallowed-loadout
        // branch, filtered to the destination planet's items.
        let loadout_of = |sector_id: u16| -> Vec<(String, i32)> {
            let Some(sector) = campaign.sector(active, sector_id) else {
                return Vec::new();
            };
            let Some(preset_id) = sector.preset else {
                return Vec::new();
            };
            Self::preset_launch_loadout(registry, active, preset_id)
                .into_iter()
                .collect()
        };
        let mut views = mind_core::ui::campaign::CampaignViews::from_campaign(
            campaign,
            registry,
            &schematics,
            active,
            played,
            // No cheap core-size seam for the being-played `allowNumberedLaunch`
            // clause (G05 T3 deviation; `0` = unknown).
            0,
            playing_rules,
            &loadout_of,
        );
        // T10: `SaveSlot.getPlayTime` (`Saves.java:385-386`) — the stats dialog
        // reads `meta.timePlayed` from the sector's save slot (one scan; the
        // live slot's in-memory accumulation is not added, deviation).
        let stems: std::collections::HashMap<u16, String> = campaign
            .planet(active)
            .map(|planet| {
                planet
                    .sectors
                    .iter()
                    .filter_map(|sector| sector.save.clone().map(|save| (sector.id, save)))
                    .collect()
            })
            .unwrap_or_default();
        if !stems.is_empty() {
            // Memoized on the saves-directory mtime (finding #9): this listing
            // reads every slot header on each dialog rebuild otherwise.
            let saved = self.cached_save_slots();
            for view in &mut views.sectors {
                if let Some(stem) = stems.get(&view.id)
                    && let Some(slot) = saved.iter().find(|slot| slot.index() == *stem)
                    && let Some(meta) = slot.meta.as_ref()
                {
                    view.stats.playtime_ms = meta.time_played;
                }
            }
        }
        // The live registry boots with the empty `MemoryBundle`, so the
        // builder's `SectorPreset.isHidden()` proxy (`unlock.description`) is
        // always true here. Java reads the description from `Core.bundle`
        // (`UnlockableContent.java:91`); resolve the stats description key the
        // same way `localized_content_description` does so a bundle-less
        // preset keeps no row.
        for view in &mut views.sectors {
            if view.stats.preset_description_key.is_some() {
                continue;
            }
            let Some(preset) = view.preset.as_deref() else {
                continue;
            };
            let key = format!("sector.{preset}.description");
            if self.bundle_lookup(&key).is_some() {
                view.stats.preset_description_key = Some(key);
            }
        }
        // The map-registry dialogs read `maps` from the same endpoint; keep the
        // built-in rows the fixture path feeds them.
        views.maps = mind_core::ui::campaign::default_map_entries();
        to_gstring(&views, "{}")
    }

    /// Current sector/rules summary: the session rules scalars, the played
    /// sector's record rows ([`Self::sector_state_rows`]) and the campaign
    /// flag. `set` is the `GString`-key shorthand every row uses.
    #[func]
    pub fn get_sector_state(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let set = |dict: &mut VarDictionary, key: &str, value: Variant| {
            dict.set(&GString::from(key), &value);
        };
        set(
            &mut dict,
            "campaign",
            self.session.is_campaign().to_variant(),
        );
        // `GameState.isEditor` (`rules.editor`): the pause dialog's desktop
        // `!state.isEditor()` gate for the `@savegame`/`@loadgame` entries
        // (`PausedDialog.java:89-93`).
        set(&mut dict, "editor", self.session.rules.editor.to_variant());
        set(&mut dict, "wave", self.session.wave.to_variant());
        set(&mut dict, "waves", self.session.rules.waves.to_variant());
        set(
            &mut dict,
            "attack",
            self.session.rules.attack_mode.to_variant(),
        );
        set(
            &mut dict,
            "hasCore",
            (self.session.player_core_count() > 0).to_variant(),
        );
        // I08 T4: the live custom map's name (the host match id for
        // non-campaign worlds); empty for campaign sectors and the menu.
        set(
            &mut dict,
            "mapName",
            self.current_map_name
                .clone()
                .unwrap_or_default()
                .to_variant(),
        );
        for (key, value) in self.sector_state_rows() {
            set(&mut dict, key, value);
        }
        set(
            &mut dict,
            "campaignComplete",
            self.campaign_complete.to_variant(),
        );
        dict
    }

    /// `Vars.hadSerpuloRemaps && settings.getBoolOnce("serpulo-remaps-notice")`:
    /// Java shows the one-time "Campaign Changes Notice" dialog after a Serpulo
    /// remap (`PlanetDialog.java:216-222`). `had_serpulo_remaps` is only true in
    /// the boot that performed the remap (same as Java's static flag), so this
    /// consumes the notice flag once and reports whether to show it.
    #[func]
    pub fn take_serpulo_remaps_notice(&mut self) -> bool {
        let noticed =
            settings_hub::with_settings(|store| store.get_bool("serpulo-remaps-notice", false));
        if !self.saves.had_serpulo_remaps || noticed {
            return false;
        }
        settings_hub::with_settings(|store| store.put_bool("serpulo-remaps-notice", true));
        self.flush_settings("remap notice flush failed");
        true
    }

    /// `PlanetDialog.abandonSectorConfirm` for the played sector
    /// (PlanetDialog.java:1153-1161): the caller hides the planet page and
    /// 7 ticks later every player-team core dies, so the campaign game-over
    /// path (`Logic.checkGameState` -> `GameOverEvent` -> sector loss) plays.
    /// Without a live sim runtime (no sector running) it falls back to the
    /// off-play record clear.
    #[func]
    pub fn abandon_sector(&mut self) -> bool {
        let played = with_host_runtime(self.host(), |runtime| {
            let played = runtime.session.is_campaign();
            runtime.abandon_played_sector();
            played
        });
        if played == Some(true) {
            log::info!("abandon: player cores die in 7 ticks (game-over path)");
            return true;
        }
        self.clear_played_sector_record()
    }

    /// `PlanetDialog.abandonSectorConfirm`: an off-play sector loses its items,
    /// base and production in the record (`saveInfo` + persist); the played
    /// sector routes through the existing played-sector clear.
    #[func]
    pub fn abandon_sector_at(&mut self, planet: GString, sector: i32) -> bool {
        let Some(sector_id) = sector_arg(i64::from(sector)) else {
            return false;
        };
        let planet_name = planet.to_string();
        let Some(planet_id) = self
            .registry
            .as_ref()
            .and_then(|registry| planet_id_by_name(registry, &planet_name))
        else {
            return false;
        };
        if self.session.sector == Some((planet_id, sector_id)) {
            return self.abandon_sector();
        }
        let Some(true) = self.with_campaign_mut(|campaign, settings| {
            // `Sector::abandon` (H01): clear items/core/production + saveInfo.
            let Some((_, record)) = campaign.sector_by_name_mut(&planet_name, sector_id) else {
                return false;
            };
            record.abandon(settings, &planet_name);
            true
        }) else {
            return false;
        };
        self.persist();
        self.planet_mesh_cache = None;
        true
    }

    /// The planet-globe reveal list (`PlanetDialog.java:270-288`): collects
    /// this planet's unlocked, still-hidden presets, marks `info.shown` and
    /// saves each sector record (`Sector.saveInfo`); the caller appends the
    /// last sector, reverses and replays the reveal.
    #[func]
    pub fn take_new_presets(&mut self, planet: GString) -> PackedInt32Array {
        let planet_name = planet.to_string();
        let mut out = PackedInt32Array::new();
        let Some(registry) = self.registry.take() else {
            return out;
        };
        let Some(ids) = self.with_campaign_mut(|campaign, settings| {
            let Some(planet_id) = campaign.planet_id_by_name(&planet_name) else {
                return Vec::new();
            };
            let ids = campaign
                .planet_mut(planet_id)
                .map(|planet| planet.new_presets(&registry))
                .unwrap_or_default();
            // `Sector.saveInfo` per revealed preset (`Campaign::save_sector_info`).
            for id in &ids {
                let _ = campaign.save_sector_info(settings, planet_id, *id);
            }
            ids
        }) else {
            self.registry = Some(registry);
            return out;
        };
        self.registry = Some(registry);
        for id in ids {
            out.push(id as i32);
        }
        if !out.is_empty() {
            self.persist();
        }
        out
    }

    /// `Sector.setName` (`Sector.java:164-167`): stores the display name in the
    /// sector's info record and persists (`saveInfo`); an empty name clears it.
    #[func]
    pub fn set_sector_name(&mut self, planet: GString, sector: i32, name: GString) -> bool {
        let Some(sector_id) = sector_arg(i64::from(sector)) else {
            return false;
        };
        let planet_name = planet.to_string();
        let display = name.to_string();
        let display = (!display.is_empty()).then_some(display);
        // `Sector.setName` + `Sector.saveInfo` (`Campaign::edit_sector`).
        let Some(true) = self.with_campaign_mut(|campaign, settings| {
            campaign
                .edit_sector(settings, &planet_name, sector_id, |record| {
                    record.info.info.name = display.clone();
                })
                .is_some()
        }) else {
            return false;
        };
        self.persist();
        true
    }

    /// Sector marker icon (`PlanetDialog.java:1223-1283`): an accessible
    /// `info.icon` (`Iconc` field name) or an `info.contentIcon` content name;
    /// the unused half is stored as `None` (both empty = `Icon.none`).
    #[func]
    pub fn set_sector_icon(
        &mut self,
        planet: GString,
        sector: i32,
        icon: GString,
        content_icon: GString,
    ) -> bool {
        let Some(sector_id) = sector_arg(i64::from(sector)) else {
            return false;
        };
        let planet_name = planet.to_string();
        let icon = icon.to_string();
        let content_icon = content_icon.to_string();
        let Some(true) = self.with_campaign_mut(|campaign, settings| {
            campaign
                .edit_sector(settings, &planet_name, sector_id, |record| {
                    record.info.info.icon = (!icon.is_empty()).then_some(icon.clone());
                    record.info.info.content_icon =
                        (!content_icon.is_empty()).then_some(content_icon.clone());
                })
                .is_some()
        }) else {
            return false;
        };
        self.persist();
        true
    }
}

impl MindCampaign {
    /// The played sector's record rows: planet/sector identity, the
    /// captured/save/last flags, the preset name, the landing announce gate,
    /// the captured predicate and the localized preset description. Empty
    /// without a played sector.
    fn sector_state_rows(&self) -> Vec<(&'static str, Variant)> {
        let Some((planet, sector)) = self.session.sector else {
            return Vec::new();
        };
        let name = self
            .campaign
            .as_ref()
            .and_then(|campaign| campaign.planet_name(planet))
            .unwrap_or_default()
            .to_owned();
        let record = self
            .campaign
            .as_ref()
            .and_then(|campaign| campaign.sector(planet, sector));
        // `state.getSector().preset` name (hint predicates: `isTutorial`,
        // `presetLaunch`), empty for a generated sector.
        let preset_name = record
            .and_then(|sector| sector.preset)
            .and_then(|preset| {
                self.registry
                    .as_ref()
                    .and_then(|registry| registry.sector(preset))
                    .map(|def| def.name.clone())
            })
            .unwrap_or_default();
        // The landing announce gate (CoreBlock.java:414): campaign +
        // `Vars.showSectorLandInfo` + `preset.showSectorLandInfo`.
        let preset_def = record.and_then(|sector| sector.preset).and_then(|preset| {
            self.registry
                .as_ref()
                .and_then(|registry| registry.sector(preset))
        });
        // `PausedDialog.rebuild` `showObjective`: the preset description feeds
        // the `@objective` full-text dialog (`UnlockableContent` resolves it
        // from `Core.bundle`).
        let preset_description = resolve_preset_description(
            self.registry.as_ref(),
            &self.bundle,
            record.and_then(|sector| sector.preset),
        );
        vec![
            ("planet", name.as_str().to_variant()),
            ("sector", (sector as i64).to_variant()),
            (
                "wasCaptured",
                record
                    .map(|sector| sector.info.info.was_captured)
                    .unwrap_or(false)
                    .to_variant(),
            ),
            (
                "hasSave",
                record
                    .map(|sector| sector.has_save())
                    .unwrap_or(false)
                    .to_variant(),
            ),
            (
                "isLast",
                record
                    .map(|sector| sector.is_last_sector())
                    .unwrap_or(false)
                    .to_variant(),
            ),
            ("preset", preset_name.as_str().to_variant()),
            (
                "showLandInfo",
                mind_core::content::registries::sectors::shows_sector_land_info(
                    self.session.is_campaign(),
                    preset_def,
                )
                .to_variant(),
            ),
            (
                "captured",
                record
                    .map(|sector| sector.is_captured(None))
                    .unwrap_or(false)
                    .to_variant(),
            ),
            (
                "presetDescription",
                preset_description.as_str().to_variant(),
            ),
        ]
    }

    /// The persisted half of an abandon without a running sector: clears the
    /// session sector's items/base/production, drops its save and removes the
    /// file.
    fn clear_played_sector_record(&mut self) -> bool {
        let Some((planet, sector_id)) = self.session.sector else {
            return false;
        };
        let Some(planet_name) = self
            .registry
            .as_ref()
            .and_then(|registry| registry.planet(planet))
            .map(|record| record.name.clone())
        else {
            return false;
        };

        // T1: clear the record on the single live campaign (runtime while a
        // sector runs), then mirror into the facade via `with_campaign_mut`.
        // `Sector::clear_base` (H01) is the record-clear half.
        let cleared = self
            .with_campaign_mut(|campaign, settings| {
                if let Some(sector) = campaign.sector_mut(planet, sector_id) {
                    sector.clear_base(settings, &planet_name);
                }
            })
            .is_some();
        if !cleared {
            return false;
        }

        let file = self.paths.sector_save(&planet_name, sector_id as u32);
        if let Err(error) = NativeFs.delete(&file) {
            log::warn!(
                "abandon_sector: failed to delete `{}`: {error}",
                file.display()
            );
        }
        let backup = mind_core::io::SaveIo::backup_file_for(&file);
        let _ = NativeFs.delete(&backup);
        self.persist();
        log::info!("abandoned campaign sector `{planet_name}:{sector_id}`");
        self.planet_mesh_cache = None;
        true
    }
}
