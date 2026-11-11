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
        let resources = self
            .campaign
            .as_mut()
            .map(|campaign| {
                settings_hub::with_settings(|settings| {
                    campaign.universe.get_launch_resources(settings).to_vec()
                })
            })
            .unwrap_or_default();
        let Some(registry) = self.registry.as_ref() else {
            return empty_object();
        };
        let mut out = serde_json::Map::new();
        for (index, item) in registry.items().iter().enumerate() {
            let Some(amount) = resources.get(index).copied() else {
                break;
            };
            if amount > 0 {
                out.insert(item.name.clone(), serde_json::json!(amount));
            }
        }
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

        // Snapshot the sector reads before the `universe` mutable borrow.
        let (
            sitems,
            destination_allow,
            destination_allow_schematics,
            destination_preset,
            launcher_core_name,
            planet_default_core,
            from_name,
            from_preset,
        ) = {
            let Some(campaign) = self.campaign.as_ref() else {
                return empty_object();
            };
            let Some(planet_record) = campaign.planet(planet_id) else {
                return empty_object();
            };
            let Some(planet_def) = registry.planet(planet_id) else {
                return empty_object();
            };
            let (Some(launcher_sector), Some(destination_sector)) = (
                planet_record.sector(launcher as u16),
                planet_record.sector(destination as u16),
            ) else {
                return empty_object();
            };
            // `Core.bundle.format("launch.from", sector.name())`
            // (`LaunchLoadoutDialog.java:165`): one display-name implementation.
            let from_name = launcher_sector.display_name(planet_record);
            (
                launcher_sector.items().clone(),
                destination_sector.allow_launch_loadout(planet_def.allow_launch_loadout),
                destination_sector.allow_launch_schematics(planet_def.allow_launch_schematics),
                destination_sector.preset,
                launcher_sector.info.info.best_core_type.clone(),
                planet_def.default_core.clone().unwrap_or_default(),
                from_name,
                launcher_sector.preset_name.clone(),
            )
        };

        let schematics = self.load_schematics(registry);
        // `PlanetDialog.java:1405`: the picker's core is the launcher's best
        // core when the *destination* allows launch schematics, and the
        // planet's default core otherwise (`LaunchLoadoutDialog.java:162`
        // then seeds the selection from that core's last-used loadout).
        let core_name = launch_core_name(
            registry,
            &planet_default_core,
            &launcher_core_name,
            destination_allow_schematics,
        );
        let core = registry.block_id(&core_name);
        // `universe.getLoadout(core)` (`LaunchLoadoutDialog.java:162`): the
        // last loadout used for the launcher's core type, persisted as
        // `lastloadout-<core>`; Java falls back to `core-shard`'s first
        // loadout (`:163`).
        let seeded_index = self.campaign.as_ref().and_then(|campaign| {
            settings_hub::with_settings(|settings| {
                campaign
                    .universe
                    .get_loadout_index(settings, &schematics, registry, &core_name)
            })
        });
        let default_index = seeded_index.or_else(|| {
            registry
                .block_id("core-shard")
                .and_then(|shard| schematics.get_loadouts(shard).first().copied())
        });
        let selected_index = usize::try_from(loadout_index)
            .ok()
            .filter(|index| *index < schematics.all.len())
            .or(default_index)
            .unwrap_or(0);
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
        let requirements = launch_requirements(selected, registry);

        // `universe.getLaunchResources()`, with the `show()` on-planet filter
        // (`launch.set(item, 0)` for items not on the launcher's planet).
        let mut resources: indexmap::IndexMap<String, i32> = indexmap::IndexMap::new();
        {
            let Some(campaign) = self.campaign.as_mut() else {
                return empty_object();
            };
            let launch_seq = settings_hub::with_settings(|settings| {
                campaign.universe.get_launch_resources(settings)
            });
            for (index, item) in registry.items().iter().enumerate() {
                if registry
                    .planet(planet_id)
                    .is_some_and(|planet| planet.allow_launch_loadout)
                    && !Self::item_on_planet(registry, &item.name, planet_id)
                {
                    continue;
                }
                let amount = launch_seq.get(index).copied().unwrap_or(0);
                if amount != 0 {
                    resources.insert(item.name.clone(), amount);
                }
            }
        }

        let preset_loadout = destination_preset
            .map(|preset| Self::preset_launch_loadout(registry, planet_id, preset))
            .unwrap_or_default();
        let max = settings_hub::with_settings(|store| store.get_bool("maxresources", true));
        let all_items: Vec<String> = registry
            .items()
            .iter()
            .map(|item| item.name.clone())
            .collect();
        let (planned, valid) = launch_plan(
            &sitems,
            &requirements,
            &resources,
            capacity,
            max,
            destination_allow,
            &preset_loadout,
            &all_items,
        );

        // `universe.updateLaunchResources(resources)`: persist the recomputed
        // sequence so the launch path reads the same staged amounts (T1: on the
        // live campaign). `registry` borrows `self.registry` here, so this
        // cannot route through `with_campaign_mut` (which borrows all of
        // `self`); take/put the runtime directly instead.
        let mut persist_resources = false;
        if destination_allow && max {
            let mut dense = vec![0i32; registry.items().len()];
            for (name, amount) in &planned {
                if let Some(index) = registry.item_id(name)
                    && let Some(slot) = dense.get_mut(index.index())
                {
                    *slot = *amount;
                }
            }
            let updated = with_host_runtime(self.host(), |runtime| {
                settings_hub::with_settings(|settings| {
                    runtime
                        .campaign
                        .universe
                        .update_launch_resources(settings, std::mem::take(&mut dense))
                        .is_ok()
                })
            });
            if let Some(updated) = updated {
                persist_resources = updated;
            }
            if updated.is_none()
                && let Some(campaign) = self.campaign.as_mut()
            {
                persist_resources = settings_hub::with_settings(|settings| {
                    campaign
                        .universe
                        .update_launch_resources(settings, dense)
                        .is_ok()
                });
            }
        }

        // `total = selected.requirements() + universe.getLaunchResources()`
        // (`LaunchLoadoutDialog.java:79-84`): the launcher pays exactly this
        // at `PlanetDialog.playSelected` (`PlanetDialog.java:1409-1410`).
        let total = launch_total(&requirements, &planned);
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
        let mut loadout_rows: Vec<serde_json::Value> = Vec::new();
        if destination_allow_schematics {
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
                if !schematic_placeable_on_planet(loadout, registry, planet_id, env, sun) {
                    continue;
                }
                loadout_rows.push(serde_json::json!({
                    "index": index,
                    "name": loadout.name(),
                    "core": def.name,
                    "is_default": schematics.get_default_loadout(loadout_core) == Some(index),
                    "file": loadout.file,
                }));
            }
        }
        let nonzero = |map: &indexmap::IndexMap<String, i32>| {
            let mut out = serde_json::Map::new();
            for (item, amount) in map {
                if *amount != 0 {
                    out.insert(item.clone(), serde_json::json!(amount));
                }
            }
            out
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
            resolve_preset_description(Some(registry), &self.bundle, destination_preset);
        let value = serde_json::json!({
            "from": from_name,
            "from_preset": from_preset,
            "capacity": capacity,
            "allow_launch_loadout": destination_allow,
            "allow_launch_schematics": destination_allow_schematics,
            "preset_description": preset_description,
            "core": core_name,
            "stored": nonzero(&sitems),
            "requirements": nonzero(&requirements),
            "resources": nonzero(&planned),
            "max": max,
            "valid": valid,
            "total": nonzero(&total),
            "launch_cost": launch_cost,
            "preset_loadout": nonzero(&preset_loadout),
            "loadouts": loadout_rows,
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
        let stacks: Vec<(String, i32)> = stacks
            .iter()
            .filter_map(|stack| stack.item.clone().map(|item| (item, stack.amount)))
            .collect();
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
    /// `item.isOnPlanet(planet)` (`UnlockableContent.shownPlanets`).
    fn item_on_planet(registry: &ContentRegistry, name: &str, planet: PlanetId) -> bool {
        registry
            .item_by_name(name)
            .is_some_and(|item| item.unlock.shown_planets.contains(&planet))
    }

    /// `destination.preset.generator.map.rules().loadout` for the disallowed
    /// branch (`LaunchLoadoutDialog.java:66-76`), filtered to items on the
    /// launcher's planet; the map header is read through `SaveIo.getMeta`.
    pub(super) fn preset_launch_loadout(
        registry: &ContentRegistry,
        planet: PlanetId,
        preset_id: SectorId,
    ) -> indexmap::IndexMap<String, i32> {
        let mut out = indexmap::IndexMap::new();
        let Some(preset) = registry.sector(preset_id) else {
            return out;
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
            return out;
        };
        let Ok(rules) = serde_json::from_str::<Rules>(&quote_bare_keys(&meta.rules_json)) else {
            return out;
        };
        for stack in &rules.loadout {
            let Some(name) = stack.item.as_deref() else {
                continue;
            };
            if Self::item_on_planet(registry, name, planet) {
                *out.entry(name.to_owned()).or_insert(0) += stack.amount;
            }
        }
        out
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

/// `PlanetDialog.java:1405`: the core whose loadout list the launch picker
/// offers. With `destination.allowLaunchSchematics()` the launcher's best core
/// wins (`from.info.bestCoreType instanceof CoreBlock`), otherwise the planet's
/// default core is forced (PAR-0509). A missing/`air` best core or default
/// falls back to `core-shard`.
fn launch_core_name(
    registry: &ContentRegistry,
    planet_default_core: &str,
    launcher_best_core: &str,
    allow_schematics: bool,
) -> String {
    let is_core_block = |name: &str| {
        registry
            .block_id(name)
            .and_then(|id| registry.block(id))
            .is_some_and(|def| def.kind == mind_core::content::BlockKind::CoreBlock)
    };
    let fallback = if is_core_block(planet_default_core) {
        planet_default_core.to_owned()
    } else {
        "core-shard".to_owned()
    };
    if allow_schematics && is_core_block(launcher_best_core) {
        launcher_best_core.to_owned()
    } else {
        fallback
    }
}

/// `Schematic.requirements()` as a non-zero `item name -> amount` map, in item
/// content order (the `ItemSeq` the launch charge is read from).
fn launch_requirements(
    schematic: &mind_core::game::schematic::Schematic,
    registry: &ContentRegistry,
) -> indexmap::IndexMap<String, i32> {
    let requirements = schematic.requirements(registry);
    registry
        .items()
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let amount = requirements.get(ItemId::new(index as u16));
            (amount != 0).then(|| (item.name.clone(), amount))
        })
        .collect()
}

/// `LaunchLoadoutDialog.update`'s `total` (`LaunchLoadoutDialog.java:79-84`):
/// the selected schematic's requirements plus the staged launch resources.
/// `PlanetDialog.playSelected` removes exactly this from the launcher
/// (`PlanetDialog.java:1409-1410`), so it must be priced from the schematic
/// the picker actually selected rather than the planet-default core's loadout
/// list (PAR-0489).
fn launch_total(
    requirements: &indexmap::IndexMap<String, i32>,
    resources: &indexmap::IndexMap<String, i32>,
) -> indexmap::IndexMap<String, i32> {
    let mut total = requirements.clone();
    for (item, amount) in resources {
        if *amount != 0 {
            *total.entry(item.clone()).or_insert(0) += *amount;
        }
    }
    total
}

/// `LaunchLoadoutDialog.java:171-176`: a loadout is offered only when every
/// tile's block supports the launcher planet's default environment and can be
/// built there (`Block.supportsEnv`/`Block.isOnPlanet`).
fn schematic_placeable_on_planet(
    schematic: &mind_core::game::schematic::Schematic,
    registry: &ContentRegistry,
    planet: PlanetId,
    env: i32,
    sun: Option<PlanetId>,
) -> bool {
    schematic.tiles.iter().all(|tile| {
        registry.block(tile.block).is_some_and(|def| {
            mind_core::world::build::supports_env_bits(def, env)
                && mind_core::world::build::environment_buildable(def, Some(planet), sun)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;
    use mind_core::content::MemoryUnlockStore;
    use mind_core::content::create_base_content;

    /// PAR-0489: the launch charge is the *selected* schematic's requirements
    /// plus the staged launch resources (`PlanetDialog.java:1409-1410`), not
    /// the planet-default core's loadout list. The old path looked the chosen
    /// index up in `campaign_section("loadouts")` (built from
    /// `planet.default_core`), which never contains a larger-core schematic,
    /// so `core-nucleus`+ loadouts launched for free.
    #[test]
    fn launch_cost_prices_the_selected_schematic_not_the_default_core_list() {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        let mut schematics = Schematics::new();
        schematics.load_loadouts(&registry);

        // `basicNucleus` is the vanilla `core-nucleus` loadout.
        let nucleus_core = require(registry.block_id("core-nucleus"), "core-nucleus");
        let nucleus_index = require(
            schematics.get_default_loadout(nucleus_core),
            "basicNucleus loadout",
        );
        let nucleus = &schematics.all[nucleus_index];
        let requirements = launch_requirements(nucleus, &registry);
        assert!(
            !requirements.is_empty(),
            "basicNucleus carries its blocks' build costs"
        );

        // The old cost source (the planet-default core's loadout rows) never
        // lists the nucleus schematic, which is why its requirements were lost.
        let default_rows =
            mind_core::ui::campaign::loadout_views(&schematics, &registry, "core-shard");
        assert!(!default_rows.is_empty(), "core-shard has loadouts");
        assert!(
            default_rows.iter().all(|row| row.index != nucleus_index),
            "the default-core list cannot price a nucleus loadout (PAR-0489)"
        );

        // `total = selected.requirements() + universe.getLaunchResources()`.
        let resources = indexmap::IndexMap::from([("copper".to_owned(), 100)]);
        let total = launch_total(&requirements, &resources);
        let copper = requirements.get("copper").copied().unwrap_or(0);
        assert_eq!(
            total.get("copper").copied(),
            Some(copper + 100),
            "schematic copper requirement + staged resources"
        );
        for (item, amount) in &requirements {
            if item != "copper" {
                assert_eq!(
                    total.get(item),
                    Some(amount),
                    "`{item}` requirement charged"
                );
            }
        }
    }

    /// PAR-0509: `PlanetDialog.java:1405` picks the launcher's best core only
    /// when the destination allows launch schematics; otherwise the planet
    /// default core is forced (`LaunchLoadoutDialog` then renders no grid).
    /// Serpulo allows schematics (`Planets.java:138`), Erekir does not.
    #[test]
    fn launch_core_follows_destination_allow_launch_schematics() {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        assert!(
            require(registry.planet_by_name("serpulo"), "serpulo").allow_launch_schematics,
            "Serpulo allows launch schematics"
        );
        assert!(
            !require(registry.planet_by_name("erekir"), "erekir").allow_launch_schematics,
            "Erekir keeps the default `allowLaunchSchematics = false`"
        );
        // Schematics allowed: the launcher's `bestCoreType` wins.
        assert_eq!(
            launch_core_name(&registry, "core-shard", "core-nucleus", true),
            "core-nucleus"
        );
        // Destination forbids schematics: always the planet default core, even
        // when the launcher's best core is larger.
        assert_eq!(
            launch_core_name(&registry, "core-bastion", "core-nucleus", false),
            "core-bastion"
        );
        // `SectorInfo.bestCoreType` defaults to a non-core block ("air");
        // Java's `instanceof CoreBlock` check falls back to the default core.
        assert_eq!(
            launch_core_name(&registry, "core-shard", "air", true),
            "core-shard"
        );
        assert_eq!(
            launch_core_name(&registry, "core-bastion", "", true),
            "core-bastion"
        );
    }

    /// `LaunchLoadoutDialog.java:171-176`: a loadout whose blocks cannot be
    /// built on the launcher's planet is not offered. Uses a Serpulo tech-tree
    /// block (`copper-wall` -> `shownPlanets = [serpulo]`): buildable on
    /// Serpulo, not on Erekir (`Block.isOnPlanet`).
    #[test]
    fn launch_grid_skips_blocks_not_buildable_on_the_planet() {
        let mut registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(error) => panic!("base content: {error}"),
            };
        // `shownPlanets` is assigned by the `init`/`postInit` sweeps
        // (`Planet.init` -> `TechTree.addPlanet`, `Block.postInit`), which the
        // live client boot runs after `create_base_content`.
        require_ok(registry.init(), "content init");
        require_ok(registry.post_init(), "content postInit");
        let copper_wall = require(registry.block_id("copper-wall"), "copper-wall");
        let serpulo_id = require(registry.planet_by_name("serpulo"), "serpulo").id;
        assert!(
            registry
                .block(copper_wall)
                .is_some_and(|def| def.unlock.shown_planets.contains(&serpulo_id)),
            "copper-wall is registered to Serpulo's tech tree"
        );
        let schematic = mind_core::game::schematic::Schematic {
            tiles: vec![mind_core::game::schematic::Stile::new(
                copper_wall,
                0,
                0,
                Default::default(),
                0,
            )],
            labels: Vec::new(),
            tags: Default::default(),
            width: 1,
            height: 1,
            file: None,
            mod_name: None,
        };
        let serpulo = require(registry.planet_by_name("serpulo"), "serpulo");
        let erekir = require(registry.planet_by_name("erekir"), "erekir");
        let sun = registry.planet_by_name("sun").map(|planet| planet.id);
        let serpulo_env = mind_core::game::planet::env_mask(&serpulo.default_env);
        let erekir_env = mind_core::game::planet::env_mask(&erekir.default_env);
        assert!(
            schematic_placeable_on_planet(&schematic, &registry, serpulo.id, serpulo_env, sun),
            "a Serpulo block is offered on Serpulo"
        );
        assert!(
            !schematic_placeable_on_planet(&schematic, &registry, erekir.id, erekir_env, sun),
            "a Serpulo-only block is not offered on Erekir"
        );
    }
}
