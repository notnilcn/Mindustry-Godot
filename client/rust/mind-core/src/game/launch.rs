// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Launch loadout: the payload envelope, the launch-picker core/requirements
//! math and the planet filters.
//! Upstream: core/src/mindustry/ui/dialogs/LaunchLoadoutDialog.java;
//! core/src/mindustry/ui/dialogs/PlanetDialog.java (`playSelected`);
//! core/src/mindustry/game/Universe.java; core/src/mindustry/world/blocks/campaign/LaunchPad.java

use crate::content::{ContentRegistry, ItemId, PlanetId};
use crate::game::rules::Rules;
use crate::game::schematic::Schematic;
use crate::io::json::content_serde::JsonItemStack;
use indexmap::IndexMap;

/// `item.isOnPlanet(planet)` (`UnlockableContent.shownPlanets`): whether the
/// item appears in the launch dialog on `planet`.
///
/// **Divergence (kept verbatim from the facade):** Java also accepts an empty
/// `shownPlanets` and the `null`/`sun` planet (`UnlockableContent.java:132-134`);
/// this checks `contains` only, so items with no planet assigned are filtered
/// out. Behaviour-preserving copy — see the H02 report follow-up; fix separately.
///
/// Extracted from `MindCampaign::item_on_planet` (`mind-gdext`
/// `campaign.rs:817` at `291e30a8`; adopted by X01).
pub fn item_on_planet(registry: &ContentRegistry, name: &str, planet: PlanetId) -> bool {
    registry
        .item_by_name(name)
        .is_some_and(|item| item.unlock.shown_planets.contains(&planet))
}

/// `destination.preset.generator.map.rules().loadout` (`LaunchLoadoutDialog.java:66-76`),
/// filtered to items on the launcher's planet.
///
/// The facade keeps the header read (`SaveIo::get_meta`) and the `Rules`
/// parse; this is the filter half.
///
/// Extracted from `MindCampaign::preset_launch_loadout` (`mind-gdext`
/// `campaign.rs:852-860` at `291e30a8`; adopted by X01).
pub fn preset_launch_loadout_filter(
    registry: &ContentRegistry,
    planet: PlanetId,
    rules: &Rules,
) -> IndexMap<String, i32> {
    let mut out = IndexMap::new();
    for stack in &rules.loadout {
        let Some(name) = stack.item.as_deref() else {
            continue;
        };
        if item_on_planet(registry, name, planet) {
            *out.entry(name.to_owned()).or_insert(0) += stack.amount;
        }
    }
    out
}

/// `PlanetDialog.playSelected`'s launcher payload: the schematic index, the
/// origin sector id, the stacks array and the accelerator's `launch_core`.
///
/// Field types are exactly what the facade code produces:
/// - `stacks`: the dialog envelope's `stacks` array (or the legacy bare
///   `[{item, amount}]` array), `None` when the payload is not a stack list;
/// - `schematic`: `LaunchLoadoutDialog`'s selected `Schematics.all` index;
/// - `origin`: `Control.playNewSector`'s launcher sector id;
/// - `launch_core`: `Universe.updateLoadout(CoreBlock)`'s core block name;
/// - `stacks_json`: the stacks array JSON forwarded to
///   `MindSimHost.place_launch_loadout` (`"[]"` for invalid JSON).
#[derive(Debug, Clone, PartialEq)]
pub struct LaunchPayload {
    /// Parsed `[{item, amount}]` stacks (`None` when absent/invalid).
    pub stacks: Option<Vec<JsonItemStack>>,
    /// `Schematics.all` index of the selected loadout schematic.
    pub schematic: Option<i32>,
    /// Launch origin sector id (`Control.playNewSector`, `Control.java:573-578`).
    pub origin: Option<u16>,
    /// Accelerator envelope's `launch_core` (`Universe.updateLoadout(CoreBlock)`).
    pub launch_core: Option<String>,
    /// The `stacks` array JSON (`"[]"` when the payload carries none).
    pub stacks_json: String,
}

impl LaunchPayload {
    /// Parses the dialog envelope
    /// `{"core": ..., "stacks": [...], "schematic": N, "origin": N, "launch_core": "..."}`
    /// or the legacy bare `[{item, amount}]` array.
    ///
    /// Extracted from the free `parse_loadout_stacks` (7549-7560), the free
    /// `parse_launch_payload` (7562-7578) and the `launch_core` read at
    /// 1610-1613 (`mind-gdext` `campaign.rs` at `291e30a8`; adopted by X01).
    pub fn parse(json: &str) -> LaunchPayload {
        let (schematic, origin, stacks_json) = parse_launch_payload(json);
        LaunchPayload {
            stacks: parse_loadout_stacks(json),
            schematic,
            origin,
            launch_core: serde_json::from_str::<serde_json::Value>(json)
                .ok()
                .and_then(|value| value.get("launch_core")?.as_str().map(str::to_owned)),
            stacks_json,
        }
    }
}

/// Parses a launch-loadout payload: the dialog envelope
/// `{"core": ..., "stacks": [...]}` (`PlanetDialog.playSelected`) or the legacy
/// bare `[{item, amount}]` array.
fn parse_loadout_stacks(json: &str) -> Option<Vec<JsonItemStack>> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let stacks = value.get("stacks").cloned().unwrap_or(value);
    serde_json::from_value(stacks).ok()
}

/// Parses the launcher launch payload (`PlanetDialog.playSelected`):
/// `(schematic index, origin sector id, stacks JSON)`.
///
/// The schematic index indexes `Schematics.all` (`None` for the items-only
/// fallback); the `stacks` array JSON is forwarded to
/// `MindSimHost.place_launch_loadout`.
fn parse_launch_payload(json: &str) -> (Option<i32>, Option<u16>, String) {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return (None, None, String::from("[]"));
    };
    let schematic = value
        .get("schematic")
        .and_then(serde_json::Value::as_i64)
        .map(|index| index as i32);
    // `Control.playNewSector` (`Control.java:573-578`): `info.origin =
    // info.destination = origin`; absent -> no origin (a free launch).
    let origin = value
        .get("origin")
        .and_then(serde_json::Value::as_i64)
        .and_then(|origin| u16::try_from(origin).ok());
    let stacks = value.get("stacks").cloned().unwrap_or(value);
    (schematic, origin, stacks.to_string())
}

/// `PlanetDialog.java:1405`: the core whose loadout list the launch picker
/// offers. With `destination.allowLaunchSchematics()` the launcher's best core
/// wins (`from.info.bestCoreType instanceof CoreBlock`), otherwise the planet's
/// default core is forced (PAR-0509). A missing/`air` best core or default
/// falls back to `core-shard`.
///
/// Extracted from the free `launch_core_name` (`mind-gdext`
/// `campaign.rs:7467` at `291e30a8`; adopted by X01).
pub fn launch_core_name(
    registry: &ContentRegistry,
    planet_default_core: &str,
    launcher_best_core: &str,
    allow_schematics: bool,
) -> String {
    let is_core_block = |name: &str| {
        registry
            .block_id(name)
            .and_then(|id| registry.block(id))
            .is_some_and(|def| def.kind == crate::content::BlockKind::CoreBlock)
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
///
/// Extracted from the free `launch_requirements` (`mind-gdext`
/// `campaign.rs:7493` at `291e30a8`; adopted by X01).
pub fn launch_requirements(
    schematic: &Schematic,
    registry: &ContentRegistry,
) -> IndexMap<String, i32> {
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

/// `LaunchLoadoutDialog.java:171-176`: a loadout is offered only when every
/// tile's block supports the launcher planet's default environment and can be
/// built there (`Block.supportsEnv`/`Block.isOnPlanet`).
///
/// Extracted from the free `schematic_placeable_on_planet` (`mind-gdext`
/// `campaign.rs:7531` at `291e30a8`; adopted by X01).
pub fn schematic_placeable_on_planet(
    schematic: &Schematic,
    registry: &ContentRegistry,
    planet: PlanetId,
    env: i32,
    sun: Option<PlanetId>,
) -> bool {
    schematic.tiles.iter().all(|tile| {
        registry.block(tile.block).is_some_and(|def| {
            crate::world::build::supports_env_bits(def, env)
                && crate::world::build::environment_buildable(def, Some(planet), sun)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::schematic::Stile;
    use crate::game::schematics::Schematics;

    fn registry() -> ContentRegistry {
        crate::game::test_support::raw_registry()
    }

    /// The launch charge is read from the *selected* schematic's requirements
    /// (PAR-0489): the vanilla `basicNucleus` loadout carries its blocks' build
    /// costs, in item content order.
    #[test]
    fn launch_requirements_price_the_selected_loadout() {
        let registry = registry();
        let mut schematics = Schematics::new();
        schematics.load_loadouts(&registry);
        let nucleus_core = registry.block_id("core-nucleus").expect("core-nucleus");
        let nucleus_index = schematics
            .get_default_loadout(nucleus_core)
            .expect("basicNucleus loadout");
        let requirements = launch_requirements(&schematics.all[nucleus_index], &registry);
        assert!(
            !requirements.is_empty(),
            "basicNucleus carries its blocks' build costs"
        );
        // Item content order, no zero entries.
        let order: Vec<usize> = requirements
            .keys()
            .filter_map(|name| registry.item_id(name).map(|id| id.index()))
            .collect();
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(order, sorted, "item content order");
        assert!(requirements.values().all(|amount| *amount != 0));
    }

    /// PAR-0509: `PlanetDialog.java:1405` picks the launcher's best core only
    /// when the destination allows launch schematics; otherwise the planet
    /// default core is forced (`LaunchLoadoutDialog` then renders no grid).
    #[test]
    fn launch_core_follows_destination_allow_launch_schematics() {
        let registry = registry();
        assert!(
            registry
                .planet_by_name("serpulo")
                .expect("serpulo")
                .allow_launch_schematics,
            "Serpulo allows launch schematics"
        );
        assert!(
            !registry
                .planet_by_name("erekir")
                .expect("erekir")
                .allow_launch_schematics,
            "Erekir keeps the default `allowLaunchSchematics = false`"
        );
        assert_eq!(
            launch_core_name(&registry, "core-shard", "core-nucleus", true),
            "core-nucleus"
        );
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
        assert_eq!(
            launch_core_name(&registry, "air", "air", true),
            "core-shard"
        );
    }

    /// `LaunchLoadoutDialog.java:171-176`: a loadout whose blocks cannot be
    /// built on the launcher's planet is not offered. Uses a Serpulo tech-tree
    /// block (`copper-wall` -> `shownPlanets = [serpulo]`): buildable on
    /// Serpulo, not on Erekir (`Block.isOnPlanet`).
    #[test]
    fn launch_grid_skips_blocks_not_buildable_on_the_planet() {
        let mut registry = registry();
        // `shownPlanets` is assigned by the `init`/`postInit` sweeps
        // (`Planet.init` -> `TechTree.addPlanet`, `Block.postInit`).
        registry.init().expect("init");
        registry.post_init().expect("post_init");
        let copper_wall = registry.block_id("copper-wall").expect("copper-wall");
        let serpulo = registry.planet_by_name("serpulo").expect("serpulo");
        assert!(
            registry
                .block(copper_wall)
                .is_some_and(|def| def.unlock.shown_planets.contains(&serpulo.id)),
            "copper-wall is registered to Serpulo's tech tree"
        );
        let schematic = Schematic {
            tiles: vec![Stile::new(copper_wall, 0, 0, Default::default(), 0)],
            labels: Vec::new(),
            tags: Default::default(),
            width: 1,
            height: 1,
            file: None,
            mod_name: None,
        };
        let erekir = registry.planet_by_name("erekir").expect("erekir");
        let sun = registry.planet_by_name("sun").map(|planet| planet.id);
        let serpulo_env = crate::game::planet::env_mask(&serpulo.default_env);
        let erekir_env = crate::game::planet::env_mask(&erekir.default_env);
        assert!(
            schematic_placeable_on_planet(&schematic, &registry, serpulo.id, serpulo_env, sun),
            "a Serpulo block is offered on Serpulo"
        );
        assert!(
            !schematic_placeable_on_planet(&schematic, &registry, erekir.id, erekir_env, sun),
            "a Serpulo-only block is not offered on Erekir"
        );
    }

    #[test]
    fn item_on_planet_uses_shown_planets() {
        let mut registry = registry();
        let serpulo = registry.planet_id("serpulo").expect("serpulo");
        let erekir = registry.planet_id("erekir").expect("erekir");
        // Vanilla items may carry no `shownPlanets` at all (the block loader
        // treats an empty list as "all planets"), so pin the filter directly.
        let copper = registry.item_id("copper").expect("copper");
        registry
            .item_mut(copper)
            .expect("copper")
            .unlock
            .shown_planets = vec![serpulo];
        assert!(item_on_planet(&registry, "copper", serpulo));
        assert!(!item_on_planet(&registry, "copper", erekir));
        assert!(!item_on_planet(&registry, "no-such-item", serpulo));
    }

    #[test]
    fn preset_launch_loadout_filter_keeps_on_planet_items() {
        let mut registry = registry();
        let serpulo = registry.planet_id("serpulo").expect("serpulo");
        let copper = registry.item_id("copper").expect("copper");
        registry
            .item_mut(copper)
            .expect("copper")
            .unlock
            .shown_planets = vec![serpulo];
        let rules = Rules {
            loadout: vec![
                JsonItemStack {
                    item: Some("copper".to_owned()),
                    amount: 100,
                },
                JsonItemStack {
                    item: Some("copper".to_owned()),
                    amount: 25,
                },
                JsonItemStack {
                    item: None,
                    amount: 7,
                },
            ],
            ..Rules::default()
        };
        let out = preset_launch_loadout_filter(&registry, serpulo, &rules);
        assert_eq!(out.get("copper"), Some(&125), "duplicate rows accumulate");
        assert_eq!(out.len(), 1, "item-less rows are skipped");
    }

    #[test]
    fn launch_payload_parses_the_envelope_and_the_legacy_array() {
        let payload = LaunchPayload::parse(
            r#"{"core":"core-shard","stacks":[{"item":"copper","amount":10}],"schematic":2,"origin":17,"launch_core":"core-nucleus"}"#,
        );
        assert_eq!(
            payload.stacks,
            Some(vec![JsonItemStack {
                item: Some("copper".to_owned()),
                amount: 10
            }])
        );
        assert_eq!(payload.schematic, Some(2));
        assert_eq!(payload.origin, Some(17));
        assert_eq!(payload.launch_core.as_deref(), Some("core-nucleus"));
        assert_eq!(payload.stacks_json, r#"[{"item":"copper","amount":10}]"#);

        let legacy = LaunchPayload::parse(r#"[{"item":"lead","amount":3}]"#);
        assert_eq!(
            legacy.stacks,
            Some(vec![JsonItemStack {
                item: Some("lead".to_owned()),
                amount: 3
            }])
        );
        assert_eq!(legacy.schematic, None);
        assert_eq!(legacy.origin, None);
        assert_eq!(legacy.launch_core, None);
        assert_eq!(legacy.stacks_json, r#"[{"item":"lead","amount":3}]"#);

        // Invalid JSON: empty payload, the host gets the canonical `[]`.
        let invalid = LaunchPayload::parse("not json");
        assert_eq!(invalid.stacks, None);
        assert_eq!(invalid.schematic, None);
        assert_eq!(invalid.origin, None);
        assert_eq!(invalid.launch_core, None);
        assert_eq!(invalid.stacks_json, "[]");
        // Out-of-range origin ids are dropped (u16 conversion fails).
        assert_eq!(
            LaunchPayload::parse(r#"{"origin":70000,"stacks":[]}"#).origin,
            None
        );
    }
}
