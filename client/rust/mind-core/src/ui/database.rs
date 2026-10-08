// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Core Database view model (`core/src/mindustry/ui/dialogs/DatabaseDialog.java`).
//!
//! Groups database-visible unlockable content by `databaseCategory` /
//! `databaseTag` and lists the planet tabs `checkTabList()` derives from every
//! record's `databaseTabs` (`Planets.sun` first, then planets by id). Planets
//! and weathers never enter the grid (their `isHidden()` is always true). Pure
//! data: the GDScript dialog filters by the active tab and search text and
//! renders the JSON.

use indexmap::IndexMap;
use serde::Serialize;

use super::super::content::ctype::{Mappable, UnlockFields, Unlockable};
use super::super::content::{ContentRegistry, ContentType, PlanetId};
use super::campaign::hex;

/// `databaseCategory` -> `databaseTag` -> entries, in first-seen content order
/// (`DatabaseDialog.sortContents`).
type CategoryMap = IndexMap<String, IndexMap<String, Vec<DatabaseEntryView>>>;

/// One content entry in the database grid.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DatabaseEntryView {
    /// Content name (icon region base and content-info key).
    pub name: String,
    /// Localized label or bundle key (`<type>.<name>.name`).
    pub localized: String,
    /// Content type name (`item`/`block`/...), for the icon region chain.
    pub content_type: &'static str,
    /// Whether the entry renders unlocked (`DatabaseDialog.unlocked`).
    pub unlocked: bool,
    /// Whether the entry appears in every planet tab (`allDatabaseTabs`).
    pub all_tabs: bool,
    /// Planet content names whose tabs include this entry.
    pub tabs: Vec<String>,
}

/// One `databaseTag` group inside a category.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DatabaseTagView {
    /// `databaseTag` (`default` when unset).
    pub name: String,
    /// Bundle key `database-tag.<name>`.
    pub label: String,
    /// Entries in content order.
    pub entries: Vec<DatabaseEntryView>,
}

/// One `databaseCategory` section.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DatabaseCategoryView {
    /// `databaseCategory` (defaults to the content type name).
    pub name: String,
    /// Bundle key `database-category.<name>`.
    pub label: String,
    /// Tags in first-seen content order.
    pub tags: Vec<DatabaseTagView>,
}

/// One planet tab (`checkTabList`: `Planets.sun` first, then planets by id).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DatabaseTabView {
    /// Tab content name (`sun` is the all-content tab).
    pub name: String,
    /// Localized label or bundle key (`planet.<name>.name`; `all` for the sun).
    pub localized: String,
    /// Planet icon color `rrggbb` (`""` for the all-content tab).
    pub color: String,
    /// Whether this is the `Planets.sun` all-content tab.
    pub is_all: bool,
}

/// The Core Database model consumed by `database_dialog.gd`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DatabaseView {
    /// Planet tabs, all-content first.
    pub tabs: Vec<DatabaseTabView>,
    /// Non-empty categories in content-map order.
    pub categories: Vec<DatabaseCategoryView>,
}

impl DatabaseView {
    /// Empty model (content boot failure; never panics in a `#[func]`).
    pub fn empty() -> Self {
        Self {
            tabs: Vec::new(),
            categories: Vec::new(),
        }
    }
}

/// Builds the database model for `registry`.
///
/// `unlocked_override` mirrors `DatabaseDialog.unlocked`: custom (non-campaign)
/// games show everything unlocked, while the menu and campaign read the
/// persisted per-content unlock state.
pub fn database_view(registry: &ContentRegistry, unlocked_override: bool) -> DatabaseView {
    // Every grid-eligible unlockable, paired with its `isHidden()` flag. The
    // `Unlockable` trait is not dyn-compatible (`Content::TYPE` is an associated
    // const), so the macro records the borrowed fields per concrete type.
    let mut records: Vec<(&str, &UnlockFields, bool)> = Vec::new();
    macro_rules! collect {
        ($($accessor:ident),* $(,)?) => {
            $(
                for record in registry.$accessor() {
                    if record.removed {
                        continue;
                    }
                    let hidden = record.is_hidden();
                    records.push((record.name(), record.unlock(), hidden));
                }
            )*
        };
    }
    collect!(items, blocks, liquids, statuses, units, sectors);

    // `checkTabList`: planets named by any record's `databaseTabs`, sorted by
    // id (`Content.compareTo`), with the sun's all-content tab inserted first.
    let mut tab_ids: Vec<PlanetId> = Vec::new();
    for (_, fields, _) in &records {
        for tab in &fields.database_tabs {
            if tab.type_ == ContentType::Planet {
                let id = PlanetId::new(tab.id);
                if !tab_ids.contains(&id) {
                    tab_ids.push(id);
                }
            }
        }
    }
    tab_ids.sort_by_key(|id| id.raw());

    let mut tabs = vec![DatabaseTabView {
        name: String::from("sun"),
        localized: String::from("all"),
        color: String::new(),
        is_all: true,
    }];
    for id in tab_ids {
        if let Some(planet) = registry.planet(id) {
            tabs.push(DatabaseTabView {
                name: planet.name.clone(),
                localized: label("planet", &planet.name, &planet.unlock.localized_name),
                color: hex(&planet.icon_color),
                is_all: false,
            });
        }
    }

    // `sortContents`: category -> tag -> entries, first-seen insertion order.
    let mut sorted: CategoryMap = CategoryMap::new();
    for (record_name, fields, hidden) in records {
        if hidden || fields.hide_database {
            continue;
        }
        let category = fields
            .database_category
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| fields.content_type.name().to_owned());
        let tag = fields
            .database_tag
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| String::from("default"));
        let entry = DatabaseEntryView {
            name: record_name.to_owned(),
            localized: label(
                fields.content_type.name(),
                record_name,
                &fields.localized_name,
            ),
            content_type: fields.content_type.name(),
            unlocked: unlocked_override || fields.unlocked(),
            all_tabs: fields.all_database_tabs,
            tabs: fields
                .database_tabs
                .iter()
                .filter(|tab| tab.type_ == ContentType::Planet)
                .filter_map(|tab| {
                    registry
                        .planet(PlanetId::new(tab.id))
                        .map(|planet| planet.name.clone())
                })
                .collect(),
        };
        sorted
            .entry(category)
            .or_default()
            .entry(tag)
            .or_default()
            .push(entry);
    }

    let categories = sorted
        .into_iter()
        .map(|(name, tags)| DatabaseCategoryView {
            label: format!("database-category.{name}"),
            name,
            tags: tags
                .into_iter()
                .map(|(tag_name, entries)| DatabaseTagView {
                    label: format!("database-tag.{tag_name}"),
                    name: tag_name,
                    entries,
                })
                .collect(),
        })
        .collect();

    DatabaseView { tabs, categories }
}

/// Localized name when the registry carries one, otherwise its bundle key
/// (`<type>.<name>.name`); the GDScript dialog resolves keys through
/// `MindAssets.bundle_get`.
fn label(type_name: &str, name: &str, localized_name: &str) -> String {
    if localized_name.is_empty() || localized_name == name {
        format!("{type_name}.{name}.name")
    } else {
        localized_name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    #[test]
    fn tabs_and_categories_match_the_reference() {
        let registry = test_registry();
        let view = database_view(&registry, false);
        let tab_names: Vec<&str> = view.tabs.iter().map(|tab| tab.name.as_str()).collect();
        // `checkTabList`: sun inserted first, then the Serpulo/Erekir tech-tree
        // tabs sorted by planet id (erekir = 1, serpulo = 5).
        assert_eq!(tab_names, vec!["sun", "erekir", "serpulo"]);
        assert!(view.tabs[0].is_all);
        assert_eq!(view.tabs[0].localized, "all");
        assert_eq!(view.tabs[1].color, "ff9266");
        let category_names: Vec<&str> = view
            .categories
            .iter()
            .map(|category| category.name.as_str())
            .collect();
        // The registry boots content with an empty bundle, so `isHidden()`
        // checks that depend on bundle-derived localized names (status) or
        // descriptions (sector) keep those categories out of the grid.
        assert_eq!(category_names, vec!["item", "block", "liquid", "unit"]);
        // Planets and weathers are always `isHidden()` and never enter the grid.
        assert!(
            !view
                .categories
                .iter()
                .any(|category| category.name == "planet" || category.name == "weather")
        );
    }

    #[test]
    fn block_tags_follow_first_seen_content_order() {
        let registry = test_registry();
        let view = database_view(&registry, false);
        let blocks = view
            .categories
            .iter()
            .find(|category| category.name == "block")
            .expect("block category");
        let tag_names: Vec<&str> = blocks.tags.iter().map(|tag| tag.name.as_str()).collect();
        // First-seen order over the upstream block-id sequence (graphite-press,
        // copper-wall, mender, conveyor, ...); the Java reference renders the
        // same `databaseTag` groups.
        assert_eq!(
            tag_names,
            vec![
                "crafting",
                "defense",
                "effect",
                "distribution",
                "liquid",
                "power",
                "production",
                "turret",
                "units",
                "logic"
            ]
        );
        assert_eq!(blocks.label, "database-category.block");
        assert_eq!(blocks.tags[0].label, "database-tag.crafting");
        assert!(
            blocks.tags.iter().all(|tag| !tag.entries.is_empty()),
            "empty tags are dropped"
        );
    }

    #[test]
    fn menu_unlock_state_and_custom_game_override() {
        let registry = test_registry();
        let menu = database_view(&registry, false);
        let entry = |view: &DatabaseView, name: &str| {
            view.categories
                .iter()
                .flat_map(|category| &category.tags)
                .flat_map(|tag| &tag.entries)
                .find(|entry| entry.name == name)
                .cloned()
        };
        // `alwaysUnlocked` roots (copper, sand) show unlocked in the menu.
        assert_eq!(entry(&menu, "copper").map(|e| e.unlocked), Some(true));
        assert_eq!(entry(&menu, "metaglass").map(|e| e.unlocked), Some(false));
        // Custom games (`!isCampaign && !isMenu`) force everything unlocked.
        let custom = database_view(&registry, true);
        assert!(
            custom
                .categories
                .iter()
                .flat_map(|category| &category.tags)
                .flat_map(|tag| &tag.entries)
                .all(|entry| entry.unlocked)
        );
    }

    #[test]
    fn planet_tabs_filter_content() {
        let registry = test_registry();
        let view = database_view(&registry, false);
        let entries: Vec<&DatabaseEntryView> = view
            .categories
            .iter()
            .flat_map(|category| &category.tags)
            .flat_map(|tag| &tag.entries)
            .collect();
        // Tech-tree content carries its planet tab.
        assert!(
            entries
                .iter()
                .any(|entry| entry.tabs.contains(&String::from("serpulo")))
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.tabs.contains(&String::from("erekir")))
        );
        assert!(entries.iter().all(|entry| !entry.name.is_empty()));
    }
}
