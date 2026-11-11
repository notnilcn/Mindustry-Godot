// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Snapshot tests for the typed campaign view producers (T01).
//!
//! `research_tree_serpulo.json` and `launch_plan_frozen_forest.json` are the
//! pre-T01 `serde_json::json!` layouts (key order and number formatting
//! included), generated from a deterministic fixture registry/campaign with
//! the builders' legacy assembly; the typed views must serialize byte-for-byte
//! to them. The sector-state test pins the `VarDictionary` key order and
//! `Variant` types the dialogs read.
//! Upstream: none (port-only; the campaign view-model contract)

use indexmap::IndexMap;
use mind_core::content::create_base_content;
use mind_core::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, PlanetId};
use mind_core::game::planet::EmptyNeighborhood;
use mind_core::game::rules::Rules;
use mind_core::game::schematics::Schematics;
use mind_core::game::universe::Campaign;
use mind_core::ui::campaign::{
    CampaignText, LaunchPlanInputs, LaunchPlanView, ResearchTreeView, SectorStateInputs,
    SectorStateView, launch_plan_view, launch_resources_for_planet, launch_sector_snapshot,
    research_tree_view, sector_state_view, select_loadout,
};

#[cfg(test)]
mod tests {
    use super::*;
    use godot::builtin::{GString, VariantType};

    /// Deterministic bundle/assets stub: every localized string the builders can
    /// ask for resolves to a recognizable fixture value.
    struct FixtureText;

    impl CampaignText for FixtureText {
        fn bundle(&self, key: &str) -> Option<String> {
            Some(format!("bundle:{key}"))
        }

        fn format(&self, key: &str, args: &[&str]) -> String {
            format!("fmt({key}:{})", args.join(","))
        }

        fn content_name(&self, kind: &str, name: &str) -> String {
            format!("{kind}/{name}")
        }

        fn content_description(&self, kind: &str, name: &str) -> String {
            format!("{kind}/{name}/desc")
        }

        fn content_emoji(&self, name: &str) -> String {
            format!("[{name}]")
        }
    }

    struct Fixture {
        registry: ContentRegistry,
        campaign: Campaign,
        schematics: Schematics,
        planet: PlanetId,
        launcher: u16,
        destination: u16,
        rules: Rules,
    }

    fn fixture() -> Option<Fixture> {
        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(_) => return None,
            };
        let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let planet = registry.planet_id("serpulo")?;
        let ground_zero = registry
            .sectors()
            .iter()
            .find(|preset| preset.name == "groundZero")?
            .sector;
        let frozen_forest = registry
            .sectors()
            .iter()
            .find(|preset| preset.name == "frozenForest")?
            .sector;
        // Seed the launcher sector so the launch dialog has a stored inventory.
        if let Some(sector) = campaign.sector_mut(planet, ground_zero) {
            sector.save = Some(format!("sector-serpulo-{ground_zero}"));
            sector.info.info.has_core = true;
            sector.info.info.best_core_type = "core-shard".to_owned();
            sector.info.info.items.insert("copper".to_owned(), 400);
            sector.info.info.items.insert("lead".to_owned(), 120);
        }
        // Seed the destination so it carries a preset and capture state.
        if let Some(sector) = campaign.sector_mut(planet, frozen_forest) {
            sector.info.info.was_captured = false;
            sector.info.info.has_core = false;
        }
        let mut schematics = Schematics::new();
        schematics.load_loadouts(&registry);
        Some(Fixture {
            registry,
            campaign,
            schematics,
            planet,
            launcher: ground_zero,
            destination: frozen_forest,
            rules: Rules::default(),
        })
    }

    fn fixture_unwrap() -> Fixture {
        match fixture() {
            Some(fixture) => fixture,
            None => panic!("fixture content boot"),
        }
    }

    fn research_view(fixture: &Fixture, root: &str) -> Option<ResearchTreeView> {
        let played_items = vec![("copper".to_owned(), 900), ("lead".to_owned(), 40)];
        research_tree_view(
            &fixture.registry,
            &fixture.campaign,
            &FixtureText,
            &fixture.rules,
            Some((fixture.planet, fixture.launcher)),
            &played_items,
            root,
        )
    }

    fn launch_view(fixture: &Fixture) -> Option<LaunchPlanView> {
        let registry = &fixture.registry;
        let snapshot = launch_sector_snapshot(
            &fixture.campaign,
            registry,
            fixture.planet,
            i32::from(fixture.launcher),
            i32::from(fixture.destination),
        )?;
        let core_name = mind_core::game::launch::launch_core_name(
            registry,
            &snapshot.planet_default_core,
            &snapshot.launcher_core_name,
            snapshot.destination_allow_schematics,
        );
        let selected_index = select_loadout(None, &fixture.schematics, registry, -1);
        // A sparse launch sequence: a few nonzero items, the rest absent.
        let mut launch_seq = vec![0i32; registry.items().len()];
        if let Some(index) = registry.item_id("copper") {
            launch_seq[index.index()] = 30;
        }
        if let Some(index) = registry.item_id("graphite") {
            launch_seq[index.index()] = 25;
        }
        let resources = launch_resources_for_planet(registry, fixture.planet, &launch_seq);
        let mut preset_loadout = IndexMap::new();
        preset_loadout.insert("copper".to_owned(), 100);
        preset_loadout.insert("lead".to_owned(), 0);
        preset_loadout.insert("graphite".to_owned(), 20);
        launch_plan_view(LaunchPlanInputs {
            registry,
            planet_id: fixture.planet,
            snapshot: &snapshot,
            schematics: &fixture.schematics,
            core_name,
            selected_index,
            resources,
            preset_loadout,
            max: true,
            preset_description: "fixture destination".to_owned(),
        })
    }

    fn sector_state(fixture: &Fixture) -> SectorStateView {
        sector_state_view(&SectorStateInputs {
            campaign: Some(&fixture.campaign),
            registry: Some(&fixture.registry),
            sector: Some((fixture.planet, fixture.launcher)),
            is_campaign: true,
            editor: false,
            wave: 3,
            waves: false,
            attack: true,
            has_core: true,
            map_name: "fixture-map".to_owned(),
            campaign_complete: false,
            preset_description: "fixture preset description".to_owned(),
        })
    }

    #[test]
    fn research_tree_serializes_to_the_committed_snapshot() {
        let fixture = fixture_unwrap();
        let view = match research_view(&fixture, "serpulo") {
            Some(view) => view,
            None => panic!("serpulo research tree"),
        };
        let text = match serde_json::to_string(&view) {
            Ok(text) => text,
            Err(error) => panic!("research serialize: {error}"),
        };
        assert_eq!(text, include_str!("testdata/research_tree_serpulo.json"));
    }

    #[test]
    fn launch_plan_serializes_to_the_committed_snapshot() {
        let fixture = fixture_unwrap();
        let view = match launch_view(&fixture) {
            Some(view) => view,
            None => panic!("launch plan"),
        };
        let text = match serde_json::to_string(&view) {
            Ok(text) => text,
            Err(error) => panic!("launch serialize: {error}"),
        };
        assert_eq!(
            text,
            include_str!("testdata/launch_plan_frozen_forest.json")
        );
    }

    #[test]
    fn research_tree_builder_covers_every_node_and_root() {
        let fixture = fixture_unwrap();
        let view = match research_view(&fixture, "serpulo") {
            Some(view) => view,
            None => panic!("serpulo research tree"),
        };
        assert_eq!(view.root, "serpulo");
        assert!(view.nodes.len() > 1);
        assert!(!view.roots.is_empty());
        assert!(
            view.nodes
                .iter()
                .enumerate()
                .all(|(index, row)| row.index == index)
        );
        assert_eq!(view.nodes[0].parent, -1);
        // Empty root selects the played sector's tree.
        let selected = match research_view(&fixture, "") {
            Some(view) => view,
            None => panic!("played-sector research tree"),
        };
        assert_eq!(selected.root, "serpulo");
        // Unknown root is a miss (the #[func] returns "{}").
        assert!(research_view(&fixture, "no-such-tree").is_none());
    }

    #[test]
    fn launch_plan_builder_omits_zero_amounts_and_costs_the_selected_loadout() {
        let fixture = fixture_unwrap();
        let view = match launch_view(&fixture) {
            Some(view) => view,
            None => panic!("launch plan"),
        };
        for map in [
            &view.stored,
            &view.requirements,
            &view.resources,
            &view.total,
            &view.preset_loadout,
        ] {
            assert!(
                map.values().all(|amount| *amount != 0),
                "zero entry: {map:?}"
            );
        }
        assert!(view.launch_cost.iter().all(|row| row.amount != 0));
        assert_eq!(view.preset_loadout.get("lead"), None);
        assert_eq!(view.stored.get("copper"), Some(&400));
        assert!(view.selected < fixture.schematics.all.len());
    }

    #[test]
    fn sector_state_reports_the_played_sector_rows() {
        let fixture = fixture_unwrap();
        let view = sector_state(&fixture);
        assert_eq!(view.planet, "serpulo");
        assert_eq!(view.sector, i64::from(fixture.launcher));
        assert_eq!(view.preset, "groundZero");
        assert!(!view.was_captured);
        assert!(view.has_save);
        assert_eq!(view.wave, 3);
        assert!(!view.waves);
        assert!(view.attack);
        assert!(view.has_core);
        assert_eq!(view.map_name, "fixture-map");
        assert_eq!(view.preset_description, "fixture preset description");
        assert!(!view.campaign_complete);
        // No played sector: the record rows stay empty and `sector` is -1.
        let none = sector_state_view(&SectorStateInputs {
            campaign: Some(&fixture.campaign),
            registry: Some(&fixture.registry),
            sector: None,
            is_campaign: false,
            editor: true,
            wave: 0,
            waves: true,
            attack: false,
            has_core: false,
            map_name: String::new(),
            campaign_complete: true,
            preset_description: String::new(),
        });
        assert_eq!(none.sector, -1);
        assert!(none.planet.is_empty());
        assert!(none.preset.is_empty());
        assert!(!none.show_land_info);
        assert!(none.editor);
        assert!(none.campaign_complete);
    }

    #[test]
    fn sector_state_dict_keeps_key_order_and_variant_types() {
        // `VarDictionary` is engine-backed; the Godot gate runs this test with
        // an initialized engine (PAR-0586).
        if !godot::sys::is_godot_initialized() {
            return;
        }
        let view = SectorStateView {
            campaign: true,
            editor: false,
            wave: 7,
            waves: true,
            attack: false,
            has_core: true,
            map_name: "map".to_owned(),
            planet: "serpulo".to_owned(),
            sector: 12,
            was_captured: true,
            has_save: false,
            is_last: true,
            preset: "groundZero".to_owned(),
            show_land_info: false,
            captured: true,
            preset_description: "desc".to_owned(),
            campaign_complete: true,
        };
        let dict = crate::campaign::views::sector_state_dict(&view);
        let keys: Vec<String> = dict
            .keys_array()
            .iter_shared()
            .map(|key| key.to_string())
            .collect();
        assert_eq!(
            keys,
            [
                "campaign",
                "editor",
                "wave",
                "waves",
                "attack",
                "hasCore",
                "mapName",
                "planet",
                "sector",
                "wasCaptured",
                "hasSave",
                "isLast",
                "preset",
                "showLandInfo",
                "captured",
                "presetDescription",
                "campaignComplete",
            ]
        );
        let kind = |key: &str| dict.get(&GString::from(key)).map(|value| value.get_type());
        assert_eq!(kind("campaign"), Some(VariantType::BOOL));
        assert_eq!(kind("wave"), Some(VariantType::INT));
        assert_eq!(kind("waves"), Some(VariantType::BOOL));
        assert_eq!(kind("mapName"), Some(VariantType::STRING));
        assert_eq!(kind("sector"), Some(VariantType::INT));
        assert_eq!(kind("preset"), Some(VariantType::STRING));
        assert_eq!(kind("campaignComplete"), Some(VariantType::BOOL));
        // Without a played sector the record rows are absent, like the replaced
        // `sector_state_rows` (empty `Vec`).
        let mut none = view;
        none.sector = -1;
        let dict = crate::campaign::views::sector_state_dict(&none);
        let keys: Vec<String> = dict
            .keys_array()
            .iter_shared()
            .map(|key| key.to_string())
            .collect();
        assert_eq!(
            keys,
            [
                "campaign",
                "editor",
                "wave",
                "waves",
                "attack",
                "hasCore",
                "mapName",
                "campaignComplete",
            ]
        );
    }
}
