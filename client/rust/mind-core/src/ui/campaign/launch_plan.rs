// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The typed launch-loadout payload (`launch_dialog_json`) and its loadout math.
//! Upstream: ui/dialogs/LaunchLoadoutDialog.java; ui/dialogs/PlanetDialog.java (playSelected); game/Universe.java

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::content::{ContentRegistry, PlanetId, SectorId};
use crate::game::launch;
use crate::game::schematics::Schematics;
use crate::game::universe::{Campaign, launch_plan_full};

/// The whole `launch_dialog_json` payload (`LaunchLoadoutDialog.update`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchPlanView {
    /// `Core.bundle.format("launch.from", from.name())`.
    pub from: String,
    /// `from.preset_name` (the invader/preset label).
    pub from_preset: Option<String>,
    /// `max`-clamped launch capacity (`planet multiplier * core capacity`).
    pub capacity: i32,
    /// `destination.allowLaunchLoadout(planet)`.
    pub allow_launch_loadout: bool,
    /// `destination.allowLaunchSchematics(planet)`.
    pub allow_launch_schematics: bool,
    /// The destination preset's description (empty when unresolvable).
    pub preset_description: String,
    /// The picker's core block name.
    pub core: String,
    /// `from.items` (the launcher sector's stored items), nonzero entries.
    pub stored: IndexMap<String, i32>,
    /// The selected loadout's requirements, nonzero entries.
    pub requirements: IndexMap<String, i32>,
    /// The post-`max` staged resources, nonzero entries. This is what the
    /// launcher branch charges and stocks.
    pub resources: IndexMap<String, i32>,
    /// The `maxresources` setting.
    pub max: bool,
    /// `from.items` covers `total`.
    pub valid: bool,
    /// `requirements + resources`, nonzero entries.
    pub total: IndexMap<String, i32>,
    /// `total` as item/amount rows (`PlanetDialog` charges these).
    pub launch_cost: Vec<ItemAmount>,
    /// The destination preset's map loadout (disallowed-loadout branch).
    pub preset_loadout: IndexMap<String, i32>,
    /// Loadouts offered by the schematic grid.
    pub loadouts: Vec<LaunchLoadoutRow>,
    /// Selected `Schematics.all` index.
    pub selected: usize,
    /// Selected loadout's file name (`None` for a built-in default).
    pub selected_file: Option<String>,
}

/// One `{item, amount}` launch cost row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemAmount {
    /// Item content name.
    pub item: String,
    /// Amount charged.
    pub amount: i32,
}

/// One row of the loadout picker (`LaunchLoadoutDialog.java:167-196`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaunchLoadoutRow {
    /// `Schematics.all` index.
    pub index: usize,
    /// `loadout.name()`.
    pub name: String,
    /// The loadout core's block name.
    pub core: String,
    /// `schematics.defaultLoadout(core) == index`.
    pub is_default: bool,
    /// The `.msch` file name (`None` for a built-in default loadout).
    pub file: Option<String>,
}

/// The launch dialog's per-sector snapshot (`LaunchLoadoutDialog.update`'s
/// `from`/`destination` reads), taken before the `universe` mutable borrow.
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchSectorSnapshot {
    /// The launcher sector's stored items (`Sector.items`).
    pub sitems: IndexMap<String, i32>,
    /// `destination.allowLaunchLoadout(planet)`.
    pub destination_allow: bool,
    /// `destination.allowLaunchSchematics(planet)`.
    pub destination_allow_schematics: bool,
    /// `destination.preset`.
    pub destination_preset: Option<SectorId>,
    /// `from.info.bestCoreType` (the launcher core).
    pub launcher_core_name: String,
    /// `planet.defaultCore` (`Option`'s default is `""`).
    pub planet_default_core: String,
    /// `Core.bundle.format("launch.from", sector.name())`
    /// (`LaunchLoadoutDialog.java:165`): one display-name implementation.
    pub from_name: String,
    /// `from.preset_name` (the invader/preset label).
    pub from_preset: Option<String>,
}

/// The inputs `launch_plan_view` assembles the payload from. The facade
/// resolves everything the core cannot read (settings-backed launch resources
/// and loadout selection, the destination preset's `.msav` loadout and the
/// localized description) and passes it in.
pub struct LaunchPlanInputs<'a> {
    /// Content registry.
    pub registry: &'a ContentRegistry,
    /// The planet both sectors live on.
    pub planet_id: PlanetId,
    /// The launcher/destination sector snapshot.
    pub snapshot: &'a LaunchSectorSnapshot,
    /// The schematic library.
    pub schematics: &'a Schematics,
    /// The picker's core block name (`game::launch::launch_core_name`).
    pub core_name: String,
    /// Resolved `Schematics.all` index (`select_loadout`).
    pub selected_index: usize,
    /// `universe.getLaunchResources()` with the on-planet filter.
    pub resources: IndexMap<String, i32>,
    /// The destination preset's map loadout.
    pub preset_loadout: IndexMap<String, i32>,
    /// The `maxresources` setting.
    pub max: bool,
    /// The destination preset's localized description.
    pub preset_description: String,
}

/// Builds the launch dialog payload, or `None` when the selected loadout does
/// not exist (the facade returns `"{}"`).
pub fn launch_plan_view(inputs: LaunchPlanInputs<'_>) -> Option<LaunchPlanView> {
    let registry = inputs.registry;
    let planet_id = inputs.planet_id;
    let snapshot = inputs.snapshot;
    let selected = inputs.schematics.all.get(inputs.selected_index)?;
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
    let requirements = launch::launch_requirements(selected, registry);
    let all_items: Vec<String> = registry
        .items()
        .iter()
        .map(|item| item.name.clone())
        .collect();
    // `LaunchLoadoutDialog.update`: the staged resources, the `total` launch
    // charge and its validity (`universe::launch_plan_full`).
    let plan = launch_plan_full(
        &snapshot.sitems,
        &requirements,
        &inputs.resources,
        capacity,
        inputs.max,
        snapshot.destination_allow,
        &inputs.preset_loadout,
        &all_items,
    );

    // `LaunchLoadoutDialog.java:167-196`: the schematic grid only renders when
    // the destination allows launch schematics, offers every loadout whose core
    // is no larger than the picker's core, and skips loadouts with a block that
    // cannot be built on the launcher's planet.
    let core_size = registry
        .block_id(&inputs.core_name)
        .and_then(|core| registry.block(core))
        .map(|block| block.size)
        .unwrap_or(3);
    let env = registry
        .planet(planet_id)
        .map(|planet| crate::game::planet::env_mask(&planet.default_env))
        .unwrap_or(0);
    let sun = registry.planet_id("sun");
    let loadouts = if snapshot.destination_allow_schematics {
        loadout_rows(inputs.schematics, registry, planet_id, core_size, env, sun)
    } else {
        Vec::new()
    };
    // `from.removeItems(loadout.requirements())` +
    // `from.removeItems(universe.getLaunchResources())` as one stack list
    // (`PlanetDialog.java:1409-1410`); `planet_launch` charges this.
    let launch_cost: Vec<ItemAmount> = plan
        .total
        .iter()
        .filter(|(_, amount)| **amount != 0)
        .map(|(item, amount)| ItemAmount {
            item: item.clone(),
            amount: *amount,
        })
        .collect();
    Some(LaunchPlanView {
        from: snapshot.from_name.clone(),
        from_preset: snapshot.from_preset.clone(),
        capacity,
        allow_launch_loadout: snapshot.destination_allow,
        allow_launch_schematics: snapshot.destination_allow_schematics,
        preset_description: inputs.preset_description,
        core: inputs.core_name,
        stored: nonzero(&snapshot.sitems),
        requirements: nonzero(&requirements),
        resources: nonzero(&plan.resources),
        max: inputs.max,
        valid: plan.valid,
        total: nonzero(&plan.total),
        launch_cost,
        preset_loadout: nonzero(&inputs.preset_loadout),
        loadouts,
        selected: inputs.selected_index,
        selected_file: selected.file.clone(),
    })
}

/// The launch dialog's sector reads: both sectors exist on `planet_id`'s
/// record, else `None` (`LaunchLoadoutDialog` hides the dialog).
pub fn launch_sector_snapshot(
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

/// `LaunchLoadoutDialog`'s selected loadout index: the seeded `universe
/// .getLoadout(core)` (`LaunchLoadoutDialog.java:162`, persisted as
/// `lastloadout-<core>`), else `core-shard`'s first loadout (`:163`), unless
/// the explicit `loadout_index` names one of `Schematics.all`.
pub fn select_loadout(
    seeded: Option<usize>,
    schematics: &Schematics,
    registry: &ContentRegistry,
    loadout_index: i32,
) -> usize {
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
pub fn launch_resources_for_planet(
    registry: &ContentRegistry,
    planet_id: PlanetId,
    launch_seq: &[i32],
) -> IndexMap<String, i32> {
    let mut resources: IndexMap<String, i32> = IndexMap::new();
    for (index, item) in registry.items().iter().enumerate() {
        if registry
            .planet(planet_id)
            .is_some_and(|planet| planet.allow_launch_loadout)
            && !launch::item_on_planet(registry, &item.name, planet_id)
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

/// `LaunchLoadoutDialog.java:167-196`: every `Schematics.all` loadout whose core
/// is no larger than `core_size` and every tile of which can be built on
/// `planet_id` (`game::launch::schematic_placeable_on_planet`).
pub fn loadout_rows(
    schematics: &Schematics,
    registry: &ContentRegistry,
    planet_id: PlanetId,
    core_size: i32,
    env: i32,
    sun: Option<PlanetId>,
) -> Vec<LaunchLoadoutRow> {
    let mut rows: Vec<LaunchLoadoutRow> = Vec::new();
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
        if !launch::schematic_placeable_on_planet(loadout, registry, planet_id, env, sun) {
            continue;
        }
        rows.push(LaunchLoadoutRow {
            name: loadout.name(),
            core: def.name.clone(),
            is_default: schematics.get_default_loadout(loadout_core) == Some(index),
            file: loadout.file.clone(),
            index,
        });
    }
    rows
}

/// `{item: amount}` of the map's nonzero entries, in the map's order.
fn nonzero(map: &IndexMap<String, i32>) -> IndexMap<String, i32> {
    let mut out = IndexMap::new();
    for (item, amount) in map {
        if *amount != 0 {
            out.insert(item.clone(), *amount);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_view() -> LaunchPlanView {
        let mut stored = IndexMap::new();
        stored.insert("copper".to_owned(), 400);
        let mut requirements = IndexMap::new();
        requirements.insert("copper".to_owned(), 1000);
        let mut total = IndexMap::new();
        total.insert("copper".to_owned(), 1000);
        LaunchPlanView {
            from: "Ground Zero".to_owned(),
            from_preset: Some("groundZero".to_owned()),
            capacity: 2000,
            allow_launch_loadout: true,
            allow_launch_schematics: true,
            preset_description: "Frozen".to_owned(),
            core: "core-shard".to_owned(),
            stored,
            requirements,
            resources: IndexMap::new(),
            max: true,
            valid: false,
            total,
            launch_cost: vec![ItemAmount {
                item: "copper".to_owned(),
                amount: 1000,
            }],
            preset_loadout: IndexMap::new(),
            loadouts: vec![LaunchLoadoutRow {
                index: 0,
                name: "basicShard".to_owned(),
                core: "core-shard".to_owned(),
                is_default: true,
                file: None,
            }],
            selected: 0,
            selected_file: None,
        }
    }

    #[test]
    fn launch_plan_view_round_trips_serde_in_field_order() {
        let view = sample_view();
        let text = serde_json::to_string(&view).expect("serialize");
        assert!(text.starts_with(
            "{\"from\":\"Ground Zero\",\"from_preset\":\"groundZero\",\"capacity\":2000"
        ));
        let back: LaunchPlanView = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, view);
        assert_eq!(serde_json::to_string(&back).expect("reserialize"), text);
    }

    #[test]
    fn nonzero_keeps_map_order_and_drops_zero_entries() {
        let mut map = IndexMap::new();
        map.insert("lead".to_owned(), 0);
        map.insert("copper".to_owned(), 5);
        map.insert("graphite".to_owned(), 2);
        let filtered = nonzero(&map);
        assert_eq!(
            filtered.keys().cloned().collect::<Vec<_>>(),
            vec!["copper".to_owned(), "graphite".to_owned()]
        );
        assert_eq!(filtered.values().copied().collect::<Vec<_>>(), vec![5, 2]);
    }

    #[test]
    fn launch_resources_for_planet_reports_only_staged_on_planet_items() {
        let registry = crate::content::test_support::test_registry();
        let Some(planet) = registry.planet_id("serpulo") else {
            panic!("serpulo");
        };
        let Some(copper) = registry.item_id("copper") else {
            panic!("copper");
        };
        let mut launch_seq = vec![0i32; registry.items().len()];
        launch_seq[copper.index()] = 30;
        let resources = launch_resources_for_planet(&registry, planet, &launch_seq);
        assert_eq!(resources.get("copper"), Some(&30));
        assert!(resources.values().all(|amount| *amount != 0));
    }
}
