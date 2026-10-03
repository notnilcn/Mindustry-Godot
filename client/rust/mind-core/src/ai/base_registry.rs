// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BaseRegistry` — the vanilla/AI base-part catalogue (plan 11 M5).
//!
//! Ported from `core/src/mindustry/ai/BaseRegistry.java`. Upstream reads the
//! `basepartnames` file and decodes each `baseparts/<name>` `.msch` through
//! plan 12's `Schematics`; this port takes the already-decoded [`Schematic`]s
//! (plan 02/03/12 own the asset pipeline and the `.msch` codec) and reproduces
//! the classification/derivation bodies exactly:
//!
//! * ore/floor item-drop maps for resource matching,
//! * sandbox-only tile removal,
//! * `required` resource from `ItemSource`/`LiquidSource` configs,
//! * `core` block detection,
//! * drill/pump center averaging,
//! * `tier = Σ (buildTime / buildCostMultiplier)^1.4`,
//! * `cores` / `parts` / `reqParts` buckets with tier ascending sort.

use std::collections::BTreeMap;

use crate::content::registries::blocks::{BlockKind, BuildVisibility};
use crate::content::{BlockId, ContentRegistry, ItemId, LiquidId};
use crate::game::schematic::Schematic;
use crate::world::config::ConfigValue;

/// A resource a base part is configured to extract (item or liquid source).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BaseResource {
    /// An `ItemSource` item config.
    Item(ItemId),
    /// A `LiquidSource` liquid config.
    Liquid(LiquidId),
}

/// One classified base part (`BaseRegistry.BasePart`).
#[derive(Debug, Clone, PartialEq)]
pub struct BasePart {
    /// The decoded schematic.
    pub schematic: Schematic,
    /// Drill/pump-averaged center X, or `width / 2`.
    pub center_x: i32,
    /// Drill/pump-averaged center Y, or `height / 2`.
    pub center_y: i32,
    /// Required output resource, if any.
    pub required: Option<BaseResource>,
    /// Core block, if this part contains one.
    pub core: Option<BlockId>,
    /// Total build cost (`Σ (buildTime/buildCostMultiplier)^1.4`).
    pub tier: f32,
}

/// `Vars.bases`: the loaded base-part catalogue.
#[derive(Debug, Clone, Default)]
pub struct BaseRegistry {
    /// Cores, sorted by tier.
    pub cores: Vec<BasePart>,
    /// Parts with no required resource, sorted by tier.
    pub parts: Vec<BasePart>,
    /// Parts keyed by required resource, each list sorted by tier.
    pub req_parts: BTreeMap<BaseResource, Vec<BasePart>>,
    /// `OreBlock.itemDrop -> block` (non-wall ores, first seen).
    pub ores: BTreeMap<ItemId, BlockId>,
    /// `Floor.itemDrop -> floor` (first seen).
    pub ore_floors: BTreeMap<ItemId, BlockId>,
}

impl BaseRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// `forResource(item/liquid)`.
    pub fn for_resource(&self, resource: BaseResource) -> &[BasePart] {
        self.req_parts
            .get(&resource)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Loads the ore maps and classifies every decoded schematic (`load()`).
    ///
    /// Upstream's file reads (`basepartnames`, `Schematics.read`) are plan
    /// 03/12's; this method is the pure classification half.
    pub fn load(
        &mut self,
        registry: &ContentRegistry,
        schematics: impl IntoIterator<Item = Schematic>,
    ) {
        self.cores.clear();
        self.parts.clear();
        self.req_parts.clear();
        self.ores.clear();
        self.ore_floors.clear();

        for block in registry.blocks() {
            if let Some(item) = block.item_drop {
                if block.kind == BlockKind::OreBlock && !block.wall_ore {
                    self.ores.entry(item).or_insert(block.id);
                } else if block.kind == BlockKind::Floor {
                    self.ore_floors.entry(item).or_insert(block.id);
                }
            }
        }

        for schematic in schematics {
            let part = classify(registry, schematic);
            if part.core.is_some() {
                self.cores.push(part);
            } else if part.required.is_none() {
                self.parts.push(part);
            } else if let Some(required) = part.required {
                self.req_parts.entry(required).or_default().push(part);
            }
        }

        self.cores.sort_by(|a, b| {
            a.tier
                .partial_cmp(&b.tier)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        self.parts.sort_by(|a, b| {
            a.tier
                .partial_cmp(&b.tier)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for parts in self.req_parts.values_mut() {
            parts.sort_by(|a, b| {
                a.tier
                    .partial_cmp(&b.tier)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }
}

/// Classifies one decoded schematic (`BaseRegistry.load()` inner body).
fn classify(registry: &ContentRegistry, mut schematic: Schematic) -> BasePart {
    let tilesize = crate::config::TILESIZE as f32;
    let mut center_sum = (0.0f32, 0.0f32);
    let mut drills = 0i32;
    let mut required = None;
    let mut core = None;

    for tile in &schematic.tiles {
        let Some(def) = registry.block(tile.block) else {
            continue;
        };
        if def.kind == BlockKind::CoreBlock {
            core = Some(tile.block);
        }
        if def.kind == BlockKind::ItemSource
            && let ConfigValue::Item(item) = &tile.config
        {
            required = Some(BaseResource::Item(*item));
        }
        if def.kind == BlockKind::LiquidSource
            && let ConfigValue::Liquid(liquid) = &tile.config
        {
            required = Some(BaseResource::Liquid(*liquid));
        }
        if matches!(
            def.kind,
            BlockKind::Drill
                | BlockKind::BeamDrill
                | BlockKind::BurstDrill
                | BlockKind::WallCrafter
                | BlockKind::Pump
                | BlockKind::SolidPump
                | BlockKind::Fracker
        ) {
            center_sum.0 += tile.x as f32 * tilesize + def.offset;
            center_sum.1 += tile.y as f32 * tilesize + def.offset;
            drills += 1;
        }
    }

    schematic.tiles.retain(|tile| {
        registry
            .block(tile.block)
            .map(|def| def.build_visibility != BuildVisibility::SandboxOnly)
            .unwrap_or(true)
    });

    let tier = schematic
        .tiles
        .iter()
        .filter_map(|tile| registry.block(tile.block))
        .map(|def| {
            let multiplier = if def.build_cost_multiplier == 0.0 {
                f32::INFINITY
            } else {
                def.build_cost_multiplier
            };
            (def.build_time / multiplier).powf(1.4)
        })
        .sum();

    let (center_x, center_y) = if drills > 0 {
        (
            (center_sum.0 / drills as f32 / tilesize) as i32,
            (center_sum.1 / drills as f32 / tilesize) as i32,
        )
    } else {
        (schematic.width / 2, schematic.height / 2)
    };

    BasePart {
        schematic,
        center_x,
        center_y,
        required,
        core,
        tier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::schematic::Stile;
    use indexmap::IndexMap;

    fn content() -> ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        registry.init().expect("init");
        registry
    }

    fn schematic(registry: &ContentRegistry, blocks: &[(&str, i16, i16)]) -> Schematic {
        let tiles = blocks
            .iter()
            .map(|(name, x, y)| {
                let block = registry
                    .block_id(name)
                    .unwrap_or_else(|| panic!("missing block {name}"));
                Stile::new(block, *x, *y, ConfigValue::None, 0)
            })
            .collect::<Vec<_>>();
        let width = blocks.iter().map(|(_, x, _)| *x).max().unwrap_or(0) + 1;
        let height = blocks.iter().map(|(_, _, y)| *y).max().unwrap_or(0) + 1;
        Schematic::from_tiles(tiles, IndexMap::new(), width as i32, height as i32)
    }

    #[test]
    fn registry_classifies_cores_parts_and_tiers() {
        let registry = content();
        let core = schematic(&registry, &[("core-shard", 0, 0)]);
        let wall = schematic(&registry, &[("copper-wall", 0, 0), ("copper-wall", 1, 0)]);
        let wall2 = schematic(&registry, &[("copper-wall", 0, 0)]);

        let mut bases = BaseRegistry::new();
        bases.load(&registry, vec![wall, core, wall2]);

        assert_eq!(bases.cores.len(), 1, "only the core part is a core");
        assert_eq!(bases.parts.len(), 2, "two non-core, non-required parts");
        assert!(
            bases.cores[0].tier > 0.0,
            "core tier is derived from build time"
        );
        // Parts sorted ascending by tier: the 1-wall schematic is cheaper.
        assert!(bases.parts[0].tier <= bases.parts[1].tier);
        // No ore floors use items in the synthetic base content under test.
        assert!(bases.req_parts.is_empty());
    }

    #[test]
    fn item_source_sets_required_and_buckets() {
        let registry = content();
        let source = registry.block_id("item-source").expect("item-source");
        let copper = registry.item_by_name("copper").expect("copper").id;
        let mut schem = Schematic::from_tiles(
            vec![Stile::new(source, 0, 0, ConfigValue::Item(copper), 0)],
            IndexMap::new(),
            1,
            1,
        );
        schem.width = 1;
        schem.height = 1;

        let mut bases = BaseRegistry::new();
        bases.load(&registry, vec![schem]);
        assert_eq!(bases.parts.len(), 0);
        let parts = bases.for_resource(BaseResource::Item(copper));
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].required, Some(BaseResource::Item(copper)));
        assert!(
            bases
                .for_resource(BaseResource::Liquid(crate::content::LiquidId::new(0)))
                .is_empty()
        );
    }

    #[test]
    fn ore_maps_only_include_non_wall_ore_floors() {
        let registry = content();
        let mut bases = BaseRegistry::new();
        let schems: Vec<Schematic> = Vec::new();
        bases.load(&registry, schems);
        // Vanilla Serpulo has ore blocks; every mapped value must be an ore block.
        for (item, block) in &bases.ores {
            let def = registry.block(*block).expect("ore block");
            assert_eq!(def.kind, BlockKind::OreBlock);
            assert!(!def.wall_ore);
            assert_eq!(def.item_drop, Some(*item));
        }
        for (item, block) in &bases.ore_floors {
            let def = registry.block(*block).expect("floor");
            assert_eq!(def.kind, BlockKind::Floor);
            assert_eq!(def.item_drop, Some(*item));
        }
    }
}
