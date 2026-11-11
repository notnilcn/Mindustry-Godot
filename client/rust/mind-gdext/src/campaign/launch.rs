// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Launch loadout: launch resources, the launch dialog payload and the loadout math.
//! Upstream: ui/dialogs/LaunchLoadoutDialog.java; ui/dialogs/PlanetDialog.java (playSelected); game/Universe.java

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
        let stacks: indexmap::IndexMap<String, i32> =
            self.launch_resource_stacks().into_iter().collect();
        to_gstring(&stacks, "{}")
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
    /// default loadout). Output: T01's typed `LaunchPlanView` (`launch_plan`).
    #[func]
    pub fn launch_dialog_json(
        &mut self,
        planet: GString,
        launcher: i32,
        destination: i32,
        loadout_index: i32,
    ) -> GString {
        match self.launch_plan(&planet.to_string(), launcher, destination, loadout_index) {
            Some(view) => to_gstring(&view, "{}"),
            None => empty_object(),
        }
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
