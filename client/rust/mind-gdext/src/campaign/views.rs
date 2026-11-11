// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Typed campaign view producers (T01): the Rust-only methods the dialogs will
//! consume in T02. Each `*_json` `#[func]` in the sibling modules is a
//! serialization wrapper over one of these.
//! Upstream: none (port-only; the typed `MindCampaign` view seam)

use super::*;
use mind_core::ui::campaign::{
    CampaignText, CampaignViews, LaunchPlanInputs, LaunchPlanView, LaunchSectorSnapshot,
    ResearchTreeView, SectorStateInputs, SectorStateView, launch_plan_view,
    launch_resources_for_planet, launch_sector_snapshot, research_tree_view, sector_state_view,
    select_loadout,
};

impl MindCampaign {
    /// The planet dialog's typed read model (`campaign_views_json`): sectors,
    /// stats, research, schematics, loadouts, rules, completion and the map
    /// rows, with the playtime and preset-description patch-ups.
    ///
    /// `planet` is a content name; empty selects the planet being played, else
    /// the default starting planet. `None` when the campaign is not booted or
    /// the planet is unknown (the `#[func]` returns `"{}"`).
    pub fn campaign_views(&self, planet: &str) -> Option<CampaignViews> {
        let campaign = self.campaign.as_ref()?;
        let registry = self.registry.as_ref()?;
        let active = if planet.is_empty() {
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
            campaign.planet_id_by_name(planet)
        }?;
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
        let mut views = CampaignViews::from_campaign(
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
        Some(views)
    }

    /// The research dialog's typed tree payload (`research_tree_json`).
    pub fn research_tree(&self, root: &str) -> Option<ResearchTreeView> {
        let registry = self.registry.as_ref()?;
        let campaign = self.campaign.as_ref()?;
        let played_items = self.played_core_items(self.session.default_team());
        let text = FacadeText { campaign: self };
        research_tree_view(
            registry,
            campaign,
            &text,
            &self.session.rules,
            self.session.sector,
            &played_items,
            root,
        )
    }

    /// The launch dialog's typed payload (`launch_dialog_json`).
    ///
    /// `&mut` because it persists the recomputed launch-resource sequence on
    /// the live campaign, like today (`persist_planned_resources`).
    pub fn launch_plan(
        &mut self,
        planet: &str,
        launcher: i32,
        destination: i32,
        loadout_index: i32,
    ) -> Option<LaunchPlanView> {
        if launcher < 0 || destination < 0 {
            return None;
        }
        let registry = self.registry.as_ref()?;
        let planet_id = self
            .campaign
            .as_ref()
            .and_then(|campaign| campaign.planet_id_by_name(planet))?;
        // Snapshot the sector reads before the `universe` mutable borrow
        // (`launch_sector_snapshot`).
        let snapshot: LaunchSectorSnapshot = self.campaign.as_ref().and_then(|campaign| {
            launch_sector_snapshot(campaign, registry, planet_id, launcher, destination)
        })?;

        let schematics = self.load_schematics(registry);
        // `PlanetDialog.java:1405`: the picker's core is the launcher's best
        // core when the *destination* allows launch schematics, and the
        // planet's default core otherwise (`LaunchLoadoutDialog.java:162`
        // then seeds the selection from that core's last-used loadout).
        let core_name = mind_core::game::launch::launch_core_name(
            registry,
            &snapshot.planet_default_core,
            &snapshot.launcher_core_name,
            snapshot.destination_allow_schematics,
        );
        let seeded = self.campaign.as_ref().and_then(|campaign| {
            settings_hub::with_settings(|settings| {
                campaign
                    .universe
                    .get_loadout_index(settings, &schematics, registry, &core_name)
            })
        });
        let selected_index = select_loadout(seeded, &schematics, registry, loadout_index);

        // `universe.getLaunchResources()`, with the `show()` on-planet filter
        // (`launch.set(item, 0)` for items not on the launcher's planet).
        let launch_seq = self.campaign.as_mut().map(|campaign| {
            settings_hub::with_settings(|settings| {
                campaign.universe.get_launch_resources(settings).to_vec()
            })
        })?;
        let resources = launch_resources_for_planet(registry, planet_id, &launch_seq);
        let preset_loadout = snapshot
            .destination_preset
            .map(|preset| Self::preset_launch_loadout(registry, planet_id, preset))
            .unwrap_or_default();
        let max = settings_hub::with_settings(|store| store.get_bool("maxresources", true));
        // `LaunchLoadoutDialog.java:204-210`: a destination that forbids launch
        // schematics shows the preset description pane instead of the grid.
        let preset_description = super::text::resolve_preset_description(
            Some(registry),
            &self.bundle,
            snapshot.destination_preset,
        );
        let view = launch_plan_view(LaunchPlanInputs {
            registry,
            planet_id,
            snapshot: &snapshot,
            schematics: &schematics,
            core_name,
            selected_index,
            resources,
            preset_loadout,
            max,
            preset_description,
        })?;

        // `universe.updateLaunchResources(resources)`: persist the recomputed
        // sequence so the launch path reads the same staged amounts (T1: on the
        // live campaign). `registry` borrows `self.registry` here, so this
        // cannot route through `with_campaign_mut` (which borrows all of
        // `self`); take/put the runtime directly instead.
        let mut persist_resources = false;
        if snapshot.destination_allow && max {
            // `sector::dense_from_names` (H01): item-id-indexed staged sequence.
            // The staged map's names are unique, so add == assign here.
            let planned_pairs: Vec<(String, i32)> = view
                .resources
                .iter()
                .map(|(name, amount)| (name.clone(), *amount))
                .collect();
            let dense = mind_core::game::sector::dense_from_names(registry, &planned_pairs);
            persist_resources =
                persist_planned_resources(self.host(), self.campaign.as_mut(), dense);
        }
        if persist_resources {
            self.persist();
        }
        Some(view)
    }

    /// `Universe.getLaunchResources` as typed `(item name, amount)` rows for the
    /// launch dialogs (`get_launch_resource_stacks`).
    pub fn launch_resource_stacks(&mut self) -> Vec<(String, i32)> {
        let Some(registry) = self.registry.as_ref() else {
            return Vec::new();
        };
        // `Universe.getLaunchResources` refreshes the in-memory sequence from
        // settings; `Universe::launch_resource_stacks` (H01) projects it to the
        // nonzero `{item: amount}` rows (item order, stop at the sequence end).
        self.campaign
            .as_mut()
            .map(|campaign| {
                settings_hub::with_settings(|settings| {
                    campaign.universe.get_launch_resources(settings);
                    campaign
                        .universe
                        .launch_resource_stacks(registry)
                        .into_iter()
                        .collect()
                })
            })
            .unwrap_or_default()
    }

    /// The session/rules scalars plus the played sector's record rows
    /// (`get_sector_state`).
    pub fn sector_state(&self) -> SectorStateView {
        let record = self.session.sector.and_then(|(planet, sector)| {
            self.campaign
                .as_ref()
                .and_then(|campaign| campaign.sector(planet, sector))
        });
        // `PausedDialog.rebuild` `showObjective`: the preset description feeds
        // the `@objective` full-text dialog (`UnlockableContent` resolves it
        // from `Core.bundle`).
        let preset_description = super::text::resolve_preset_description(
            self.registry.as_ref(),
            &self.bundle,
            record.and_then(|sector| sector.preset),
        );
        sector_state_view(&SectorStateInputs {
            campaign: self.campaign.as_ref(),
            registry: self.registry.as_ref(),
            sector: self.session.sector,
            is_campaign: self.session.is_campaign(),
            editor: self.session.rules.editor,
            wave: self.session.wave,
            waves: self.session.rules.waves,
            attack: self.session.rules.attack_mode,
            has_core: self.session.player_core_count() > 0,
            map_name: self.current_map_name.clone().unwrap_or_default(),
            campaign_complete: self.campaign_complete,
            preset_description,
        })
    }
}

/// [`MindCampaign`] as the core builders' [`CampaignText`] seam: the live
/// `Core.bundle` plus the `MindAssets` `Iconc` unicode table.
struct FacadeText<'a> {
    campaign: &'a MindCampaign,
}

impl CampaignText for FacadeText<'_> {
    fn bundle(&self, key: &str) -> Option<String> {
        self.campaign.bundle_lookup(key).map(str::to_owned)
    }

    fn format(&self, key: &str, args: &[&str]) -> String {
        self.campaign.bundle.format(key, args)
    }

    fn content_name(&self, kind: &str, name: &str) -> String {
        self.campaign.localized_content_name(kind, name)
    }

    fn content_description(&self, kind: &str, name: &str) -> String {
        self.campaign.localized_content_description(kind, name)
    }

    fn content_emoji(&self, name: &str) -> String {
        self.campaign
            .assets_string("unicode_str", &[GString::from(name).to_variant()])
            .unwrap_or_default()
    }
}

/// Builds the `get_sector_state` `VarDictionary` from the typed view: the same
/// keys, insertion order and `Variant` types as the hand-made dictionary.
pub(super) fn sector_state_dict(view: &SectorStateView) -> VarDictionary {
    let mut dict = VarDictionary::new();
    let set = |dict: &mut VarDictionary, key: &str, value: Variant| {
        dict.set(&GString::from(key), &value);
    };
    set(&mut dict, "campaign", view.campaign.to_variant());
    // `GameState.isEditor` (`rules.editor`): the pause dialog's desktop
    // `!state.isEditor()` gate for the `@savegame`/`@loadgame` entries
    // (`PausedDialog.java:89-93`).
    set(&mut dict, "editor", view.editor.to_variant());
    set(&mut dict, "wave", view.wave.to_variant());
    set(&mut dict, "waves", view.waves.to_variant());
    set(&mut dict, "attack", view.attack.to_variant());
    set(&mut dict, "hasCore", view.has_core.to_variant());
    // I08 T4: the live custom map's name (the host match id for
    // non-campaign worlds); empty for campaign sectors and the menu.
    set(&mut dict, "mapName", view.map_name.as_str().to_variant());
    // The replaced `sector_state_rows` only emitted the record rows when a
    // sector was being played; `-1` is the no-sector sentinel.
    if view.sector >= 0 {
        set(&mut dict, "planet", view.planet.as_str().to_variant());
        set(&mut dict, "sector", view.sector.to_variant());
        set(&mut dict, "wasCaptured", view.was_captured.to_variant());
        set(&mut dict, "hasSave", view.has_save.to_variant());
        set(&mut dict, "isLast", view.is_last.to_variant());
        set(&mut dict, "preset", view.preset.as_str().to_variant());
        set(&mut dict, "showLandInfo", view.show_land_info.to_variant());
        set(&mut dict, "captured", view.captured.to_variant());
        set(
            &mut dict,
            "presetDescription",
            view.preset_description.as_str().to_variant(),
        );
    }
    set(
        &mut dict,
        "campaignComplete",
        view.campaign_complete.to_variant(),
    );
    dict
}

/// `universe.updateLaunchResources(resources)` on the live runtime, else on the
/// mirrored campaign. Returns whether the sequence was written.
fn persist_planned_resources(
    host: Option<Gd<MindSimHost>>,
    campaign: Option<&mut Campaign>,
    stacks: Vec<i32>,
) -> bool {
    let updated = with_host_runtime(host, |runtime| {
        settings_hub::with_settings(|settings| {
            runtime
                .campaign
                .universe
                .update_launch_resources(settings, stacks.clone())
                .is_ok()
        })
    });
    if let Some(updated) = updated {
        return updated;
    }
    let Some(campaign) = campaign else {
        return false;
    };
    settings_hub::with_settings(|settings| {
        campaign
            .universe
            .update_launch_resources(settings, stacks)
            .is_ok()
    })
}
