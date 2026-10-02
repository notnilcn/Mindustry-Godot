// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Schematic`/`Stile` data and value operations (plan 12 M5).
//!
//! Ported from `core/src/mindustry/game/Schematic.java`. The `.msch` byte
//! codec lives in [`super::schematics`]; rendering (`getBuffer`/`getPreview`)
//! is plan 16/19 and replaces the Java `FrameBuffer` with a
//! [`SchematicPreviewSpec`].

use std::cmp::Ordering;

use indexmap::IndexMap;

use crate::content::Consume;
use crate::content::registries::blocks::BlockKind;
use crate::content::stacks::ItemSeq;
use crate::content::{BlockId, ContentRegistry};
use crate::world::config::ConfigValue;
use crate::world::plan::BuildPlan;

/// One placed tile in a schematic (`Schematic.Stile`).
#[derive(Debug, Clone, PartialEq)]
pub struct Stile {
    /// Block to place.
    pub block: BlockId,
    /// Local X (relative to the schematic origin).
    pub x: i16,
    /// Local Y.
    pub y: i16,
    /// Block config (`TypeIO` object).
    pub config: ConfigValue,
    /// Rotation (0-3).
    pub rotation: i8,
}

impl Stile {
    /// Creates a tile.
    pub fn new(block: BlockId, x: i16, y: i16, config: ConfigValue, rotation: i8) -> Self {
        Self {
            block,
            x,
            y,
            config,
            rotation,
        }
    }

    /// `Stile.copy`.
    pub fn copy(&self) -> Self {
        self.clone()
    }
}

/// A saved base (`Schematic`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Schematic {
    /// Tiles in write order.
    pub tiles: Vec<Stile>,
    /// User-facing tags/labels (`Schematic.labels`).
    pub labels: Vec<String>,
    /// Internal meta tags (name/description/contentMap/labels JSON).
    pub tags: IndexMap<String, String>,
    /// Width in tiles.
    pub width: i32,
    /// Height in tiles.
    pub height: i32,
    /// On-disk file name (not persisted in the payload).
    pub file: Option<String>,
    /// Associated mod name, if any.
    pub mod_name: Option<String>,
}

/// Preview description consumed by plan 19 (replaces the Java `FrameBuffer`).
#[derive(Debug, Clone, PartialEq)]
pub struct SchematicPreviewSpec {
    /// Centered build plans.
    pub plans: Vec<BuildPlan>,
    /// Width in tiles (with padding).
    pub width: i32,
    /// Height in tiles (with padding).
    pub height: i32,
}

impl Schematic {
    /// Empty schematic.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a schematic from tiles/tags/dimensions.
    pub fn from_tiles(
        tiles: Vec<Stile>,
        tags: IndexMap<String, String>,
        width: i32,
        height: i32,
    ) -> Self {
        Self {
            tiles,
            labels: Vec::new(),
            tags,
            width,
            height,
            file: None,
            mod_name: None,
        }
    }

    /// `Schematic.powerProduction` (needs `PowerGenerator` metadata; plan 09).
    pub fn power_production(&self, registry: &ContentRegistry) -> f32 {
        self.tiles
            .iter()
            .filter_map(|tile| registry.block(tile.block))
            .filter(|block| {
                matches!(
                    block.kind,
                    BlockKind::ConsumeGenerator
                        | BlockKind::SolarGenerator
                        | BlockKind::ThermalGenerator
                        | BlockKind::ImpactReactor
                        | BlockKind::NuclearReactor
                        | BlockKind::VariableReactor
                        | BlockKind::HeaterGenerator
                )
            })
            .map(generator_displayed_power)
            .sum()
    }

    /// `Schematic.powerConsumption`.
    pub fn power_consumption(&self, registry: &ContentRegistry) -> f32 {
        self.tiles
            .iter()
            .filter_map(|tile| registry.block(tile.block))
            .flat_map(|block| block.consumes.iter())
            .filter_map(|spec| match spec.consume {
                Consume::Power { usage, .. } => Some(usage),
                _ => None,
            })
            .sum()
    }

    /// `Schematic.requirements` (sum of every tile's build cost).
    pub fn requirements(&self, registry: &ContentRegistry) -> ItemSeq {
        let mut requirements = ItemSeq::with_len(registry.items().len());
        for tile in &self.tiles {
            if let Some(block) = registry.block(tile.block) {
                for stack in &block.requirements {
                    requirements.add(stack.item, stack.amount);
                }
            }
        }
        requirements
    }

    /// `Schematic.hasCore`.
    pub fn has_core(&self, registry: &ContentRegistry) -> bool {
        self.tiles.iter().any(|tile| {
            registry
                .block(tile.block)
                .is_some_and(|block| block.kind == BlockKind::CoreBlock)
        })
    }

    /// `Schematic.findCore` (the first core block, if any).
    pub fn find_core(&self, registry: &ContentRegistry) -> Option<BlockId> {
        self.tiles
            .iter()
            .find(|tile| {
                registry
                    .block(tile.block)
                    .is_some_and(|block| block.kind == BlockKind::CoreBlock)
            })
            .map(|tile| tile.block)
    }

    /// `Schematic.name` (`tags.name`, default `unknown`).
    pub fn name(&self) -> String {
        self.tags
            .get("name")
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned())
    }

    /// `Schematic.description` (`tags.description`, default empty).
    pub fn description(&self) -> String {
        self.tags.get("description").cloned().unwrap_or_default()
    }

    /// `Schematic.save` (file write is plan 12's `Schematics::save_changes`).
    pub fn steam_id(&self) -> Option<&str> {
        self.tags.get("steamid").map(String::as_str)
    }

    /// `Schematic.addSteamID`.
    pub fn add_steam_id(&mut self, id: &str) {
        self.tags.insert("steamid".to_owned(), id.to_owned());
    }

    /// `Schematic.removeSteamID`.
    pub fn remove_steam_id(&mut self) {
        self.tags.shift_remove("steamid");
    }

    /// `Schematic.compareTo` (by name).
    pub fn compare_to(&self, other: &Schematic) -> Ordering {
        self.name().cmp(&other.name())
    }

    /// Builds a preview spec centered at `(0, 0)` (plan 19 consumes it).
    pub fn preview_spec(&self) -> SchematicPreviewSpec {
        let half_w = self.width / 2;
        let half_h = self.height / 2;
        let plans = self
            .tiles
            .iter()
            .map(|tile| BuildPlan {
                x: tile.x as i32 - half_w,
                y: tile.y as i32 - half_h,
                rotation: tile.rotation as u8,
                block: tile.block,
                config: tile.config.clone(),
                breaking: false,
            })
            .collect();
        SchematicPreviewSpec {
            plans,
            width: self.width,
            height: self.height,
        }
    }

    /// `Schematic.getSteamID` used by plan 22 (`tags.steamid` presence).
    pub fn has_steam_id(&self) -> bool {
        self.tags.contains_key("steamid")
    }
}

/// `PowerGenerator.getDisplayedPowerProduction()` approximation (plan 09 owns
/// the exact generator metadata; this reads the block's power production spec
/// when available and otherwise reports zero).
fn generator_displayed_power(block: &crate::content::BlockDef) -> f32 {
    block
        .consumes
        .iter()
        .find_map(|spec| match spec.consume {
            // Generators store their output via the `Power` consume/`buffered`
            // slot in the interim plan-07 overlay; plan 09 replaces this.
            Consume::Power { buffered, .. } => Some(buffered),
            _ => None,
        })
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

    fn registry() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap()
    }

    #[test]
    fn requirements_sum_and_core_detection() {
        let registry = registry();
        let core = registry.block_id("core-shard").unwrap();
        let wall = registry.block_id("copper-wall").unwrap();
        let schem = Schematic::from_tiles(
            vec![
                Stile::new(core, 0, 0, ConfigValue::None, 0),
                Stile::new(wall, 2, 0, ConfigValue::None, 0),
            ],
            IndexMap::new(),
            3,
            1,
        );
        assert!(schem.has_core(&registry));
        assert_eq!(schem.find_core(&registry), Some(core));
        let requirements = schem.requirements(&registry);
        let core_cost = registry
            .block(core)
            .unwrap()
            .requirements
            .iter()
            .fold(0, |sum, stack| sum + stack.amount);
        let wall_cost = registry
            .block(wall)
            .unwrap()
            .requirements
            .iter()
            .fold(0, |sum, stack| sum + stack.amount);
        assert_eq!(requirements.total() as i32, core_cost + wall_cost);
    }

    #[test]
    fn name_description_and_steam_id() {
        let mut schem = Schematic::new();
        assert_eq!(schem.name(), "unknown");
        assert_eq!(schem.description(), "");
        schem.tags.insert("name".to_owned(), "base".to_owned());
        schem
            .tags
            .insert("description".to_owned(), "desc".to_owned());
        assert_eq!(schem.name(), "base");
        assert_eq!(schem.description(), "desc");
        schem.add_steam_id("123");
        assert_eq!(schem.steam_id(), Some("123"));
        schem.remove_steam_id();
        assert!(!schem.has_steam_id());
    }

    #[test]
    fn preview_spec_centers_tiles() {
        let schem = Schematic::from_tiles(
            vec![
                Stile::new(BlockId::new(1), 0, 0, ConfigValue::None, 0),
                Stile::new(BlockId::new(2), 3, 1, ConfigValue::None, 2),
            ],
            IndexMap::new(),
            4,
            2,
        );
        let spec = schem.preview_spec();
        assert_eq!(spec.width, 4);
        assert_eq!(spec.height, 2);
        assert_eq!(spec.plans[0].x, -2);
        assert_eq!(spec.plans[1].x, 1);
        assert_eq!(spec.plans[1].rotation, 2);
    }
}
