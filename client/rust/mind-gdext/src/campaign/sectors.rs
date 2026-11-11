// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Sector records and the planet dialog's sector commands (launch destination,
//! capture state, abandon, rename/icon).
//! Upstream: ui/dialogs/PlanetDialog.java; type/Sector.java; world/blocks/campaign/LaunchPad.java

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
    /// the planet being played, else the default starting planet. The typed
    /// producer is [`MindCampaign::campaign_views`] (T01).
    #[func]
    pub fn campaign_views_json(&self, planet: GString) -> GString {
        match self.campaign_views(&planet.to_string()) {
            Some(views) => to_gstring(&views, "{}"),
            None => empty_object(),
        }
    }

    /// Current sector/rules summary: the session rules scalars, the played
    /// sector's record rows ([`SectorStateView`]) and the campaign flag. The
    /// typed producer is [`MindCampaign::sector_state`] (T01); the dictionary
    /// keeps the same keys, insertion order and `Variant` types.
    #[func]
    pub fn get_sector_state(&self) -> VarDictionary {
        super::views::sector_state_dict(&self.sector_state())
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
