// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Launch loadout: launch resources, the launch dialog payload and the loadout math.
//! Upstream: ui/dialogs/LaunchLoadoutDialog.java; ui/dialogs/PlanetDialog.java (playSelected); game/Universe.java

use super::text::resolve_preset_description;
use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// `Universe.getLaunchResources` (item-id-indexed last-launch sequence).
    #[func]
    pub fn get_launch_resources(&mut self) -> PackedInt32Array {
        let mut out = PackedInt32Array::new();
        for value in self
            .campaign
            .as_mut()
            .map(|campaign| {
                settings_hub::with_settings(|settings| {
                    campaign.universe.get_launch_resources(settings)
                })
            })
            .unwrap_or_default()
        {
            out.push(*value);
        }
        out
    }

    /// `Universe.getLaunchResources` as a `{item name: amount}` JSON object for
    /// the launch dialogs (`LaunchLoadoutDialog` merges it with the schematic
    /// requirements; PAR-0085). Only nonzero entries are reported.
    #[func]
    pub fn get_launch_resource_stacks(&mut self) -> GString {
        let Some(registry) = self.registry.as_ref() else {
            return empty_object();
        };
        // `Universe.getLaunchResources` refreshes the in-memory sequence from
        // settings; `Universe::launch_resource_stacks` (H01) projects it to the
        // nonzero `{item: amount}` object (item order, stop at the sequence end).
        let stacks = self.campaign.as_mut().map(|campaign| {
            settings_hub::with_settings(|settings| {
                campaign.universe.get_launch_resources(settings);
                campaign.universe.launch_resource_stacks(registry)
            })
        });
        let Some(stacks) = stacks else {
            return empty_object();
        };
        let out: serde_json::Map<String, serde_json::Value> = stacks
            .into_iter()
            .map(|(name, amount)| (name, serde_json::json!(amount)))
            .collect();
        value_gstring(&serde_json::Value::Object(out))
    }

    /// `Universe.updateLaunchResources` from the loadout editor's
    /// `{item: amount}` map: the `LoadoutDialog` hider callback
    /// (`LaunchLoadoutDialog.java:138-141`) writes the edited `ItemSeq` back
    /// before the parent rows refresh. Items are placed in the dense
    /// item-id-indexed sequence the launch flow charges and stocks.
    #[func]
    pub fn set_launch_resources(&mut self, amounts: VarDictionary) -> bool {
        let stacks = {
            let Some(registry) = self.registry.as_ref() else {
                return false;
            };
            let mut amounts_by_name: Vec<(String, i64)> = Vec::new();
            for key in amounts.keys_array().iter_shared() {
                let Ok(name) = key.try_to::<GString>() else {
                    continue;
                };
                // The editor stores amounts as GDScript `int` variants
                // (`amount.to_variant()`); a numeric read must accept INT as
                // well as FLOAT, because `try_to::<f64>` rejects INT and
                // silently zeroed every edited amount (PAR-0488).
                let amount = amounts
                    .get(&key)
                    .map(|value| variant_amount(&value))
                    .unwrap_or(0);
                amounts_by_name.push((name.to_string(), amount));
            }
            mind_core::ui::campaign::launch_resource_sequence(registry, &amounts_by_name)
        };
        let Some(result) = self.with_campaign_mut(|campaign, settings| {
            campaign.universe.update_launch_resources(settings, stacks)
        }) else {
            return false;
        };
        if result.is_err() {
            return false;
        }
        self.persist();
        true
    }

    /// `Universe.updateLaunchResources` from a JSON `[n, ...]` sequence.
    #[func]
    pub fn update_launch_resources_json(&mut self, json: GString) -> bool {
        let Ok(stacks) = serde_json::from_str::<Vec<i32>>(&json.to_string()) else {
            return false;
        };
        let Some(result) = self.with_campaign_mut(|campaign, settings| {
            campaign.universe.update_launch_resources(settings, stacks)
        }) else {
            return false;
        };
        if result.is_err() {
            return false;
        }
        self.persist();
        true
    }

    /// `LaunchLoadoutDialog.update` (`LaunchLoadoutDialog.java:58-90`): the
    /// Configure Loadout dialog's inputs and outputs as JSON.
    ///
    /// `launcher`/`destination` are sector ids on `planet`; `loadout_index` is a
    /// `Schematics.all` index (`-1`/out of range selects the launcher core's
    /// default loadout). Output: `{"from", "from_preset", "capacity",
    /// "allow_launch_loadout", "stored", "requirements", "resources", "max",
    /// "valid", "total", "preset_loadout"}`. `resources` is the post-`max`
    /// staged sequence (persisted like Java while `max` is on), so the launcher
    /// branch of `PlanetDialog.playSelected` charges/stocks exactly it.
    #[func]
    pub fn launch_dialog_json(
        &mut self,
        planet: GString,
        launcher: i32,
        destination: i32,
        loadout_index: i32,
    ) -> GString {
        if launcher < 0 || destination < 0 {
            return empty_object();
        }
        let planet_name = planet.to_string();
        let Some(registry) = self.registry.as_ref() else {
            return empty_object();
        };
        let Some(planet_id) = self
            .campaign
            .as_ref()
            .and_then(|campaign| campaign.planet_id_by_name(&planet_name))
        else {
            return empty_object();
        };
        // Snapshot the sector reads before the `universe` mutable borrow
        // (`launch_sector_snapshot`).
        let Some(snapshot) = self.campaign.as_ref().and_then(|campaign| {
            launch_sector_snapshot(campaign, registry, planet_id, launcher, destination)
        }) else {
            return empty_object();
        };

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
        let core = registry.block_id(&core_name);
        let selected_index = select_loadout(
            self.campaign.as_ref(),
            &schematics,
            registry,
            &core_name,
            loadout_index,
        );
        let Some(selected) = schematics.all.get(selected_index) else {
            return empty_object();
        };

        // `cap = (int)(sector.planet.launchCapacityMultiplier *
        // selected.findCore().itemCapacity)`.
        let capacity = selected
            .find_core(registry)
            .and_then(|core| registry.block(core))
            .map(|block| {
                (registry
                    .planet(planet_id)
                    .map(|planet| planet.launch_capacity_multiplier)
                    .unwrap_or(0.0)
                    * block.item_capacity as f32) as i32
            })
            .unwrap_or(0);
        // `ItemSeq schems = selected.requirements()`.
        let requirements = mind_core::game::launch::launch_requirements(selected, registry);
        // `universe.getLaunchResources()`, with the `show()` on-planet filter
        // (`launch.set(item, 0)` for items not on the launcher's planet).
        let Some(resources) = self
            .campaign
            .as_mut()
            .map(|campaign| launch_resources_on_planet(registry, planet_id, campaign))
        else {
            return empty_object();
        };
        let preset_loadout = snapshot
            .destination_preset
            .map(|preset| Self::preset_launch_loadout(registry, planet_id, preset))
            .unwrap_or_default();
        let max = settings_hub::with_settings(|store| store.get_bool("maxresources", true));
        let all_items: Vec<String> = registry
            .items()
            .iter()
            .map(|item| item.name.clone())
            .collect();
        // `LaunchLoadoutDialog.update`: the staged resources, the `total` launch
        // charge and its validity (`universe::launch_plan_full`, H01; the
        // replaced `launch_plan` + `launch_total` pair).
        let plan = mind_core::game::universe::launch_plan_full(
            &snapshot.sitems,
            &requirements,
            &resources,
            capacity,
            max,
            snapshot.destination_allow,
            &preset_loadout,
            &all_items,
        );
        let planned = plan.resources;
        let total = plan.total;
        let valid = plan.valid;

        // `universe.updateLaunchResources(resources)`: persist the recomputed
        // sequence so the launch path reads the same staged amounts (T1: on the
        // live campaign). `registry` borrows `self.registry` here, so this
        // cannot route through `with_campaign_mut` (which borrows all of
        // `self`); take/put the runtime directly instead.
        let mut persist_resources = false;
        if snapshot.destination_allow && max {
            // `sector::dense_from_names` (H01): item-id-indexed staged sequence.
            // The staged map's names are unique, so add == assign here.
            let planned_pairs: Vec<(String, i32)> = planned
                .iter()
                .map(|(name, amount)| (name.clone(), *amount))
                .collect();
            let dense = mind_core::game::sector::dense_from_names(registry, &planned_pairs);
            persist_resources =
                persist_planned_resources(self.host(), self.campaign.as_mut(), dense);
        }

        // `LaunchLoadoutDialog.java:167-196`: the schematic grid only renders
        // when the destination allows launch schematics, offers every loadout
        // whose core is no larger than the picker's core, and skips loadouts
        // with a block that cannot be built on the launcher's planet.
        let core_size = core
            .and_then(|core| registry.block(core))
            .map(|block| block.size)
            .unwrap_or(3);
        let env = registry
            .planet(planet_id)
            .map(|planet| mind_core::game::planet::env_mask(&planet.default_env))
            .unwrap_or(0);
        let sun = planet_def(registry, "sun").map(|planet| planet.id);
        let loadout_grid = if snapshot.destination_allow_schematics {
            loadout_rows(&schematics, registry, planet_id, core_size, env, sun)
        } else {
            Vec::new()
        };
        // `from.removeItems(loadout.requirements())` +
        // `from.removeItems(universe.getLaunchResources())` as one stack list
        // (`PlanetDialog.java:1409-1410`); `planet_launch` charges this.
        let launch_cost: Vec<serde_json::Value> = total
            .iter()
            .filter(|(_, amount)| **amount != 0)
            .map(|(item, amount)| serde_json::json!({"item": item, "amount": amount}))
            .collect();
        // `LaunchLoadoutDialog.java:204-210`: a destination that forbids launch
        // schematics shows the preset description pane instead of the grid.
        let preset_description =
            resolve_preset_description(Some(registry), &self.bundle, snapshot.destination_preset);
        let value = serde_json::json!({
            "from": snapshot.from_name,
            "from_preset": snapshot.from_preset,
            "capacity": capacity,
            "allow_launch_loadout": snapshot.destination_allow,
            "allow_launch_schematics": snapshot.destination_allow_schematics,
            "preset_description": preset_description,
            "core": core_name,
            "stored": nonzero(&snapshot.sitems),
            "requirements": nonzero(&requirements),
            "resources": nonzero(&planned),
            "max": max,
            "valid": valid,
            "total": nonzero(&total),
            "launch_cost": launch_cost,
            "preset_loadout": nonzero(&preset_loadout),
            "loadouts": loadout_grid,
            "selected": selected_index,
            "selected_file": selected.file,
        });
        if persist_resources {
            self.persist();
        }
        value_gstring(&value)
    }

    /// `Universe.getLastLoadout` (selected schematic file name).
    #[func]
    pub fn get_last_loadout(&self) -> GString {
        GString::from(
            self.campaign
                .as_ref()
                .and_then(|campaign| campaign.universe.get_last_loadout())
                .unwrap_or(""),
        )
    }

    /// `Universe.updateLoadout` for a core type (empty file clears it).
    #[func]
    pub fn update_loadout(&mut self, core: GString, file: GString) -> bool {
        let core_name = core.to_string();
        let file_name = file.to_string();
        let file_name = (!file_name.is_empty()).then_some(file_name.as_str());
        let Some(()) = self.with_campaign_mut(|campaign, settings| {
            campaign
                .universe
                .update_loadout(settings, &core_name, file_name);
        }) else {
            return false;
        };
        self.persist();
        true
    }

    /// `Universe.clearLoadoutInfo`: a free launch (no launcher sector) drops
    /// the stored loadout + launch resources (`PlanetDialog.playSelected`).
    #[func]
    pub fn clear_loadout_info(&mut self) -> bool {
        let Some(()) = self.with_campaign_mut(|campaign, settings| {
            campaign.universe.clear_loadout_info(settings);
        }) else {
            return false;
        };
        self.persist();
        true
    }

    /// `Sector.removeItems`: subtracts `[{item, amount}]` stacks from a
    /// sector's stored items (clamped at 0) and persists (PAR-0085 launch
    /// cost, applied to the launcher sector before the launch).
    #[func]
    pub fn remove_sector_items(
        &mut self,
        planet: GString,
        sector: i32,
        stacks_json: GString,
    ) -> bool {
        let Some(sector_id) = sector_arg(i64::from(sector)) else {
            return false;
        };
        let Ok(stacks) = serde_json::from_str::<
            Vec<mind_core::io::json::content_serde::JsonItemStack>,
        >(&stacks_json.to_string()) else {
            return false;
        };
        let planet_name = planet.to_string();
        // `sector::stacks_to_pairs` (H01): the nameless-stack filter.
        let stacks = mind_core::game::sector::stacks_to_pairs(&stacks);
        // T1: mutate the single live campaign; `Sector.removeItems` ->
        // `addItems` -> `saveInfo()` (`Sector.java:207-231`) persists the
        // record right away (the PAR-0085 launch cost must stick even while
        // a sector runs).
        let Some(true) = self.with_campaign_mut(|campaign, settings| {
            campaign
                .edit_sector(settings, &planet_name, sector_id, |record| {
                    record.remove_items(stacks.iter().cloned());
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
    /// `destination.preset.generator.map.rules().loadout` for the disallowed
    /// branch (`LaunchLoadoutDialog.java:66-76`), filtered to items on the
    /// launcher's planet; the map header is read through `SaveIo.getMeta` and
    /// the filter is `game::launch::preset_launch_loadout_filter` (H02).
    pub(super) fn preset_launch_loadout(
        registry: &ContentRegistry,
        planet: PlanetId,
        preset_id: SectorId,
    ) -> indexmap::IndexMap<String, i32> {
        let Some(preset) = registry.sector(preset_id) else {
            return indexmap::IndexMap::new();
        };
        let owner = registry
            .planet(preset.planet)
            .map(|planet| planet.name.clone())
            .unwrap_or_default();
        let map = preset
            .file_name
            .clone()
            .unwrap_or_else(|| preset.name.clone());
        let path = std::path::PathBuf::from(crate::assets::loader::resolve_assets_dir())
            .join("maps")
            .join(owner)
            .join(format!("{map}.msav"));
        let Ok(meta) = SaveIo::get_meta(&NativeFs, &path) else {
            return indexmap::IndexMap::new();
        };
        let Ok(rules) = serde_json::from_str::<Rules>(&quote_bare_keys(&meta.rules_json)) else {
            return indexmap::IndexMap::new();
        };
        mind_core::game::launch::preset_launch_loadout_filter(registry, planet, &rules)
    }
}

/// Reads a numeric `Variant` as an integer (`LoadoutDialog`'s amounts are
/// GDScript `int`s; JSON-derived values are `float`s).
fn variant_amount(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.try_to::<i64>().unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number as i64)
            .unwrap_or_default(),
        _ => 0,
    }
}

/// The launch dialog's per-sector snapshot (`LaunchLoadoutDialog.update`'s
/// `from`/`destination` reads), taken before the `universe` mutable borrow.
struct LaunchSectorSnapshot {
    /// The launcher sector's stored items (`Sector.items`).
    sitems: indexmap::IndexMap<String, i32>,
    /// `destination.allowLaunchLoadout(planet)`.
    destination_allow: bool,
    /// `destination.allowLaunchSchematics(planet)`.
    destination_allow_schematics: bool,
    /// `destination.preset`.
    destination_preset: Option<SectorId>,
    /// `from.info.bestCoreType` (the launcher core).
    launcher_core_name: String,
    /// `planet.defaultCore` (`Option`'s default is `""`).
    planet_default_core: String,
    /// `Core.bundle.format("launch.from", sector.name())`
    /// (`LaunchLoadoutDialog.java:165`): one display-name implementation.
    from_name: String,
    /// `from.preset_name` (the invader/preset label).
    from_preset: Option<String>,
}

/// The launch dialog's sector reads: both sectors exist on `planet_id`'s
/// record, else `None` (`LaunchLoadoutDialog` hides the dialog).
fn launch_sector_snapshot(
    campaign: &Campaign,
    registry: &ContentRegistry,
    planet_id: PlanetId,
    launcher: i32,
    destination: i32,
) -> Option<LaunchSectorSnapshot> {
    let planet_record = campaign.planet(planet_id)?;
    let planet_def = registry.planet(planet_id)?;
    let launcher_sector = planet_record.sector(launcher as u16)?;
    let destination_sector = planet_record.sector(destination as u16)?;
    Some(LaunchSectorSnapshot {
        sitems: launcher_sector.items().clone(),
        destination_allow: destination_sector.allow_launch_loadout(planet_def.allow_launch_loadout),
        destination_allow_schematics: destination_sector
            .allow_launch_schematics(planet_def.allow_launch_schematics),
        destination_preset: destination_sector.preset,
        launcher_core_name: launcher_sector.info.info.best_core_type.clone(),
        planet_default_core: planet_def.default_core.clone().unwrap_or_default(),
        from_name: launcher_sector.display_name(planet_record),
        from_preset: launcher_sector.preset_name.clone(),
    })
}

/// `LaunchLoadoutDialog`'s selected loadout index: the seeded
/// `universe.getLoadout(core)` (`LaunchLoadoutDialog.java:162`, persisted as
/// `lastloadout-<core>`), else `core-shard`'s first loadout (`:163`), unless
/// the explicit `loadout_index` names one of `Schematics.all`.
fn select_loadout(
    campaign: Option<&Campaign>,
    schematics: &Schematics,
    registry: &ContentRegistry,
    core_name: &str,
    loadout_index: i32,
) -> usize {
    let seeded = campaign.and_then(|campaign| {
        settings_hub::with_settings(|settings| {
            campaign
                .universe
                .get_loadout_index(settings, schematics, registry, core_name)
        })
    });
    let default_index = seeded.or_else(|| {
        registry
            .block_id("core-shard")
            .and_then(|shard| schematics.get_loadouts(shard).first().copied())
    });
    usize::try_from(loadout_index)
        .ok()
        .filter(|index| *index < schematics.all.len())
        .or(default_index)
        .unwrap_or(0)
}

/// `universe.getLaunchResources()` with the `show()` on-planet filter
/// (`launch.set(item, 0)` for items not on the launcher's planet).
fn launch_resources_on_planet(
    registry: &ContentRegistry,
    planet_id: PlanetId,
    campaign: &mut Campaign,
) -> indexmap::IndexMap<String, i32> {
    let launch_seq =
        settings_hub::with_settings(|settings| campaign.universe.get_launch_resources(settings));
    let mut resources: indexmap::IndexMap<String, i32> = indexmap::IndexMap::new();
    for (index, item) in registry.items().iter().enumerate() {
        if registry
            .planet(planet_id)
            .is_some_and(|planet| planet.allow_launch_loadout)
            && !mind_core::game::launch::item_on_planet(registry, &item.name, planet_id)
        {
            continue;
        }
        let amount = launch_seq.get(index).copied().unwrap_or(0);
        if amount != 0 {
            resources.insert(item.name.clone(), amount);
        }
    }
    resources
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

/// `LaunchLoadoutDialog.java:167-196`: every `Schematics.all` loadout whose core
/// is no larger than `core_size` and every tile of which can be built on
/// `planet_id` (`game::launch::schematic_placeable_on_planet`, H02).
fn loadout_rows(
    schematics: &Schematics,
    registry: &ContentRegistry,
    planet_id: PlanetId,
    core_size: i32,
    env: i32,
    sun: Option<PlanetId>,
) -> Vec<serde_json::Value> {
    let mut rows: Vec<serde_json::Value> = Vec::new();
    for index in 0..schematics.all.len() {
        let Some(loadout) = schematics.all.get(index) else {
            continue;
        };
        let Some(loadout_core) = loadout.find_core(registry) else {
            continue;
        };
        let Some(def) = registry.block(loadout_core) else {
            continue;
        };
        if def.size > core_size {
            continue;
        }
        if !mind_core::game::launch::schematic_placeable_on_planet(
            loadout, registry, planet_id, env, sun,
        ) {
            continue;
        }
        rows.push(serde_json::json!({
            "index": index,
            "name": loadout.name(),
            "core": def.name,
            "is_default": schematics.get_default_loadout(loadout_core) == Some(index),
            "file": loadout.file,
        }));
    }
    rows
}

/// `{item: amount}` of the map's nonzero entries.
fn nonzero(map: &indexmap::IndexMap<String, i32>) -> serde_json::Map<String, serde_json::Value> {
    let mut out = serde_json::Map::new();
    for (item, amount) in map {
        if *amount != 0 {
            out.insert(item.clone(), serde_json::json!(amount));
        }
    }
    out
}
