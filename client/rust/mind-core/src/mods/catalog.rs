// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Per-mod content catalog (plan 20, `HIGH_LEVEL_PLAN.md` §12 plan-21
//! reconciliation).
//!
//! Plan 21 freezes the STDB `content_catalog { content_type, name, source_mod }`
//! shape and generates the vanilla seed with `mind-headless content seed`
//! (`server/gen_content_seed.sh`). That seed emits **667 deduped
//! `(content_type, name)` pairs** using the compact live-type order from
//! `mind-headless/src/registry.rs::LIVE_CONTENT_TYPES`
//! (`item, block, bullet, liquid, status, unit, weather, sector, planet, team,
//! unitCommand, unitStance`) — which is intentionally *not* the same as
//! `ContentType::ordinal()` (that enum keeps historical `_UNUSED` slots).
//!
//! This module lets plan 20 emit the same pair list from a live registry, with
//! the mod provenance (`Content.ModContentInfo.mod`) folded into `source_mod`
//! (`""` = vanilla). Names are globally unique because every mod content name is
//! `<mod>-` prefixed (plan 02 `transform_name`), so deduping by name is
//! equivalent to the seed's `BTreeMap<String, u8>` `or_insert`.
//!
//! The catalog walks only **mappable** records; bullets are non-mappable and
//! have no name, matching the seed (they contribute no rows but still occupy
//! compact index `2`).

use std::collections::BTreeMap;

use crate::content::{Content, ContentRegistry, ContentType, Mappable, ModId};

/// Content types recorded by the shipped `content_catalog` seed, in compact
/// order. Bullets are present for index stability but produce no rows.
pub const CATALOG_CONTENT_TYPES: [ContentType; 12] = [
    ContentType::Item,
    ContentType::Block,
    ContentType::Bullet,
    ContentType::Liquid,
    ContentType::Status,
    ContentType::Unit,
    ContentType::Weather,
    ContentType::Sector,
    ContentType::Planet,
    ContentType::Team,
    ContentType::UnitCommand,
    ContentType::UnitStance,
];

/// One `content_catalog` row: compact content-type index, name and owning mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentCatalogEntry {
    /// Compact content-type index (`mind-headless content seed` order).
    pub content_type: u8,
    /// Globally unique content name (already `<mod>-` prefixed for mod content).
    pub name: String,
    /// Owning mod internal name (`""` for vanilla).
    pub source_mod: String,
}

impl ContentCatalogEntry {
    /// Whether this row is vanilla content.
    pub fn is_vanilla(&self) -> bool {
        self.source_mod.is_empty()
    }
}

/// Compact catalog index for a live content type (`None` for `_UNUSED`/`error`).
pub fn catalog_type_index(type_: ContentType) -> Option<u8> {
    CATALOG_CONTENT_TYPES
        .iter()
        .position(|candidate| *candidate == type_)
        .map(|index| index as u8)
}

/// Builds the catalog for every content type in the registry, deduped by name
/// and sorted by name (the seed's byte order).
pub fn content_catalog(registry: &ContentRegistry) -> Vec<ContentCatalogEntry> {
    let mut entries: BTreeMap<String, ContentCatalogEntry> = BTreeMap::new();
    macro_rules! collect {
        ($accessor:ident, $ty:expr) => {
            for record in registry.$accessor() {
                let content_type = catalog_type_index($ty).expect("live catalog content type");
                add_entry(
                    &mut entries,
                    record.name(),
                    record.minfo().mod_id.as_ref().map(ModId::name),
                    content_type,
                );
            }
        };
    }
    collect!(items, ContentType::Item);
    collect!(blocks, ContentType::Block);
    // Bullets (compact index 2) are non-mappable and have no name.
    collect!(liquids, ContentType::Liquid);
    collect!(statuses, ContentType::Status);
    collect!(units, ContentType::Unit);
    collect!(weathers, ContentType::Weather);
    collect!(sectors, ContentType::Sector);
    collect!(planets, ContentType::Planet);
    collect!(teams, ContentType::Team);
    collect!(unit_commands, ContentType::UnitCommand);
    collect!(unit_stances, ContentType::UnitStance);
    entries.into_values().collect()
}

/// The vanilla subset of [`content_catalog`] (`source_mod == ""`).
pub fn vanilla_content_catalog(registry: &ContentRegistry) -> Vec<ContentCatalogEntry> {
    content_catalog(registry)
        .into_iter()
        .filter(ContentCatalogEntry::is_vanilla)
        .collect()
}

/// The catalog rows owned by one mod internal name.
pub fn content_catalog_for_mod(
    registry: &ContentRegistry,
    internal_name: &str,
) -> Vec<ContentCatalogEntry> {
    content_catalog(registry)
        .into_iter()
        .filter(|entry| entry.source_mod == internal_name)
        .collect()
}

/// Groups every modded catalog row by owning mod internal name (vanilla
/// excluded). Each list keeps the global name ordering.
pub fn catalogs_by_mod(registry: &ContentRegistry) -> BTreeMap<String, Vec<ContentCatalogEntry>> {
    let mut grouped: BTreeMap<String, Vec<ContentCatalogEntry>> = BTreeMap::new();
    for entry in content_catalog(registry) {
        if entry.is_vanilla() {
            continue;
        }
        grouped
            .entry(entry.source_mod.clone())
            .or_default()
            .push(entry);
    }
    grouped
}

/// First-writer-wins insert, matching the seed's `or_insert` dedup.
fn add_entry(
    entries: &mut BTreeMap<String, ContentCatalogEntry>,
    name: &str,
    source_mod: Option<&str>,
    content_type: u8,
) {
    entries
        .entry(name.to_owned())
        .or_insert_with(|| ContentCatalogEntry {
            content_type,
            name: name.to_owned(),
            source_mod: source_mod.unwrap_or_default().to_owned(),
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::color::Rgba;
    use crate::content::test_support::test_registry;
    use crate::content::{Item, MemoryBundle};

    #[test]
    fn catalog_is_sorted_deduped_and_uses_compact_types() {
        let registry = test_registry();
        let catalog = content_catalog(&registry);
        assert!(!catalog.is_empty());

        // Byte-for-byte parity with the committed plan-21 vanilla seed
        // (`server/spacetimedb/src/main/content_seed.rs`, 667 `VANILLA_CONTENT`
        // pairs for the same `create_base_content` boot).
        assert_eq!(
            catalog.len(),
            667,
            "vanilla catalog must match the plan-21 content seed"
        );

        // Names strictly increasing (deduped + sorted like the seed's BTreeMap).
        for pair in catalog.windows(2) {
            assert!(
                pair[0].name < pair[1].name,
                "{} !< {}",
                pair[0].name,
                pair[1].name
            );
        }
        // `test_registry` is entirely vanilla.
        assert!(catalog.iter().all(ContentCatalogEntry::is_vanilla));

        let entry = |name: &str| {
            catalog
                .iter()
                .find(|entry| entry.name == name)
                .unwrap_or_else(|| panic!("missing catalog entry `{name}`"))
        };
        // Compact indices, not `ContentType::ordinal()`: block 1, liquid 3,
        // unit 5 (bullet 2 is reserved/non-mappable).
        assert_eq!(entry("copper").content_type, 0);
        assert_eq!(entry("router").content_type, 1);
        assert_eq!(entry("water").content_type, 3);
        assert_eq!(entry("dagger").content_type, 5);
        assert_eq!(CATALOG_CONTENT_TYPES[2], ContentType::Bullet);
        assert_eq!(catalog_type_index(ContentType::Bullet), Some(2));
        assert_eq!(catalog_type_index(ContentType::MechUnused), None);
    }

    #[test]
    fn per_mod_catalog_tags_source_mod() {
        let mut registry = test_registry();
        let bundle = MemoryBundle::new();
        let store = crate::content::MemoryUnlockStore::new();
        // Mappable content is registered with the already-prefixed name
        // (`ContentJsonParser` applies `transform_name`); `minfo.mod` is taken
        // from the registry's current mod.
        registry.set_current_mod(Some(ModId("testmod".to_owned())));
        registry
            .add_item(Item::new("testmod-test", Rgba::WHITE, &bundle, &store))
            .expect("add mod item");
        registry.set_current_mod(None);

        let mod_rows = content_catalog_for_mod(&registry, "testmod");
        assert_eq!(mod_rows.len(), 1);
        assert_eq!(mod_rows[0].name, "testmod-test");
        assert_eq!(mod_rows[0].content_type, 0);
        assert_eq!(mod_rows[0].source_mod, "testmod");

        let vanilla = vanilla_content_catalog(&registry);
        assert!(vanilla.iter().all(|entry| entry.name != "testmod-test"));
        assert_eq!(content_catalog(&registry).len(), vanilla.len() + 1);

        let grouped = catalogs_by_mod(&registry);
        assert_eq!(grouped.get("testmod").map(Vec::len), Some(1));
        assert!(!grouped.contains_key(""), "vanilla is not a mod bucket");
    }
}
