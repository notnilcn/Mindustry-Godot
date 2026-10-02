// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Content-driven region inventory: the expected region set implied by
// `content/**` metadata + the generator contract (plan 03 §5 M3 verify).

//! Region inventory (plan 03 §7.1b `assets regions --assert-complete`).
//!
//! Derived from the metadata contract after the generate pass: every region
//! the content model requires is listed, so `mind-headless assets regions`
//! can assert the packed atlas resolves all of them (no silent `error`
//! fallbacks).

use std::collections::BTreeSet;

use mind_core::content::load::ContentRegistry;
use serde::{Deserialize, Serialize};

use crate::generate::GenCtx;
use crate::generate::metadata::{self, IconCtx};

/// Serialized inventory (`build/assets/region_inventory.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionInventory {
    /// Format version.
    pub format: u32,
    /// Expected region names (sorted, unique).
    pub regions: Vec<String>,
    /// Expected names absent from the source atlas (should be empty).
    #[serde(rename = "missingInSources")]
    pub missing_in_sources: Vec<String>,
}

impl RegionInventory {
    /// Number of expected regions.
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    /// Whether the inventory is empty (never for vanilla content).
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

/// Builds the expected-region inventory over the completed staging atlas.
pub fn build(ctx: &GenCtx, registry: &ContentRegistry) -> RegionInventory {
    let mut expected: BTreeSet<String> = BTreeSet::new();
    let has = |name: &str| ctx.atlas.has(name);
    let mut add = |name: String| {
        expected.insert(name);
    };

    // --- block-icons ---
    for block in registry.blocks().iter() {
        if matches!(
            block.kind,
            mind_core::content::registries::blocks::BlockKind::AirBlock
                | mind_core::content::registries::blocks::BlockKind::ConstructBlock
                | mind_core::content::registries::blocks::BlockKind::OreBlock
        ) {
            continue;
        }
        let meta = metadata::block_meta(&block.name, block.kind);
        let icons_ctx = IconCtx {
            name: &block.name,
            region: &block.region,
            kind: block.kind,
            size: block.size,
            variants: meta.variants,
            has: &has,
        };
        let icons = metadata::generated_icons(&icons_ctx);

        // Team recolors.
        if has(&format!("{}-team", block.name)) {
            for team in metadata::TEAMS.iter().filter(|team| team.has_palette) {
                add(format!("{}-team-{}", block.name, team.name));
            }
        }
        // Outlines.
        for region in metadata::make_icon_regions(&icons_ctx)
            .into_iter()
            .chain(metadata::regions_to_outline(&icons_ctx))
        {
            add(format!("{region}-outline"));
        }
        if metadata::turret_body_outlined(&icons_ctx) {
            add(format!("{}-outline", block.region));
        }
        // outlineIcon padded replacement + unpadded last.
        if let Some(index) = metadata::outline_icon_index(block.kind, icons.len())
            && has(&icons[index])
        {
            add(icons[index].clone());
        }
        // Composed icon.
        if !icons.is_empty() && has(&icons[0]) {
            for region in &icons {
                if has(region) {
                    add(region.clone());
                }
            }
            let sharded = format!("{}-team-sharded", block.name);
            if !(icons.len() == 1 && icons[0] == block.region && !has(&sharded)) {
                add(format!("block-{}-full", block.name));
            }
            add(format!("block-{}-ui", block.name));
        }
    }

    // --- shallows ---
    for block in registry.blocks().iter() {
        if block.kind != mind_core::content::registries::blocks::BlockKind::ShallowLiquid {
            continue;
        }
        let meta = metadata::block_meta(&block.name, block.kind);
        let floor_variants = meta
            .shallow_floor_base
            .and_then(|floor| registry.block_by_name(floor))
            .map(|def| metadata::block_meta(&def.name, def.kind).variants)
            .unwrap_or(0);
        let count = if floor_variants > 0 {
            floor_variants
        } else {
            1
        };
        for i in 1..=count {
            add(format!("{}{i}", block.name));
        }
    }

    // --- item-icons ---
    for item in registry.items() {
        add(format!("item-{}-ui", item.name));
    }
    for liquid in registry.liquids() {
        add(format!("liquid-{}-ui", liquid.name));
    }
    for status in registry.statuses() {
        if has(&format!("status-{}", status.name)) {
            add(format!("status-{}-ui", status.name));
        }
    }

    // --- sector-icons ---
    for sector in registry.sectors() {
        if has(&format!("sector-{}", sector.name)) {
            add(format!("sector-{}", sector.name));
        }
    }

    // --- team-icons ---
    for team in metadata::TEAMS {
        if has(&format!("team-{}", team.name)) {
            add(format!("team-{}", team.name));
        }
    }

    // --- unit-icons (plan 02 M5 metadata) ---
    for unit in registry.units() {
        if unit.internal && !unit.internal_generate_sprites {
            continue;
        }
        for region in metadata::unit_region_expectations(unit, &has) {
            add(region);
        }
    }

    // --- ore-icons ---
    for block in registry.blocks().iter() {
        if block.kind != mind_core::content::registries::blocks::BlockKind::OreBlock {
            continue;
        }
        let meta = metadata::block_meta(&block.name, block.kind);
        for i in 0..meta.variants.max(1) {
            add(format!("{}{}", block.name, i + 1));
        }
        add(format!("block-{}-full", block.name));
        add(format!("block-{}-ui", block.name));
    }

    // --- autotiles (47-slice outputs + preview) ---
    for (name, variants) in metadata::AUTOTILE_BLOCKS {
        for variant in 0..*variants {
            let base = if *variants > 1 {
                format!("{name}-{}", variant + 1)
            } else {
                (*name).to_owned()
            };
            for slice in 0..47 {
                add(format!("{base}-{slice}"));
            }
        }
        add((*name).to_owned());
    }

    // --- edges ---
    for block in registry.blocks().iter() {
        use mind_core::content::registries::blocks::BlockKind;
        if !metadata::is_floor_kind(block.kind)
            || matches!(
                block.kind,
                BlockKind::OverlayFloor
                    | BlockKind::OreBlock
                    | BlockKind::SpawnBlock
                    | BlockKind::RemoveOre
                    | BlockKind::CharacterOverlay
                    | BlockKind::RuneOverlay
                    | BlockKind::AirBlock
            )
        {
            continue;
        }
        let meta = metadata::block_meta(&block.name, block.kind);
        if !meta.blend_group.is_empty() || !meta.draw_edge_out {
            continue;
        }
        add(format!("{}-edge", block.name));
    }

    // Source-side integrity: everything above must exist in the staging atlas
    // (`block_colors` is a loose extra written beside the pages, checked by
    // `assets regions` against the file, not the manifest).
    let missing_in_sources: Vec<String> =
        expected.iter().filter(|name| !has(name)).cloned().collect();

    let mut regions: Vec<String> = expected.into_iter().collect();
    regions.push(String::from("block_colors"));
    regions.sort();
    regions.dedup();
    RegionInventory {
        format: 1,
        regions,
        missing_in_sources,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inventory_serializes_deterministically() {
        let inventory = RegionInventory {
            format: 1,
            regions: vec!["a".into(), "b".into()],
            missing_in_sources: Vec::new(),
        };
        let json = serde_json::to_string(&inventory).unwrap();
        assert!(json.contains("\"format\":1"));
        assert!(json.contains("missingInSources"));
    }
}
