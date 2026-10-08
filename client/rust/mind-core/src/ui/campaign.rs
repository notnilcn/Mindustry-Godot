// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/{PlanetDialog,ResearchDialog,SchematicsDialog,
//         SectorSelectDialog,LaunchLoadoutDialog,LoadoutDialog,MapPlayDialog,
//         CustomGameDialog,EditorMapsDialog,CampaignCompleteDialog,CampaignRulesDialog}.java
//         (plan 14 M5).

//! Campaign dialog read models (plan 14 M5).
//!
//! Godot-free view structs + builders that consume the plan-12 read APIs
//! (`Universe`/`Campaign`/`Planet`/`Sector`, `CampaignRules`/`Difficulty`,
//! `Schematics`, `TechStore`, `CampaignStats`). The GDScript dialogs read the
//! serialized form through the documented `MindUi.campaign_views()` endpoint;
//! `mind-headless ui campaign` renders the same structs against a committed
//! golden so the data contract is asserted without Godot or a live world.
//!
//! Static layout stays in the `.tscn`/`.gd` dialog scenes; this module owns the
//! data projection and formatting only (plan 14 §3.10 rule 2).

use serde::Serialize;

use crate::content::registries::sectors::SectorPresetDef;
use crate::content::{ContentRegistry, PlanetId, UnlockStore};
use crate::game::campaign_rules::{ALL_DIFFICULTIES, CampaignRules, Difficulty};
use crate::game::planet::Planet;
use crate::game::schematics::Schematics;
use crate::game::sector::Sector;
use crate::game::tech_tree;
use crate::game::universe::Campaign;

/// One selectable campaign difficulty (`Difficulty.all`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DifficultyView {
    /// Enum name (`casual`…`eradication`); `difficulty.<name>` bundle suffix.
    pub name: &'static str,
    /// Selected in the current rules.
    pub selected: bool,
    /// Localized display name (bundle with empty fallback).
    pub localized: String,
    /// Newline-joined modifier lines (`Difficulty.info()`).
    pub info: String,
    /// Enemy health multiplier.
    pub enemy_health: f32,
    /// Enemy spawn multiplier.
    pub enemy_spawn: f32,
    /// Wave-time multiplier.
    pub wave_time: f32,
}

/// One persisted campaign rule toggle (`CampaignRulesDialog`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RuleToggleView {
    /// Rule field name (`fog`, `hide_spawns`, …).
    pub key: String,
    /// Current value.
    pub value: bool,
}

/// `CampaignRulesDialog` view model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CampaignRulesView {
    /// Owning planet content name.
    pub planet: String,
    /// Active difficulty name.
    pub difficulty: &'static str,
    /// All difficulty buttons in declaration order.
    pub difficulties: Vec<DifficultyView>,
    /// Vanilla toggles in source order.
    pub toggles: Vec<RuleToggleView>,
}

/// One planet card (`PlanetDialog`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlanetView {
    /// Dense planet id.
    pub id: u16,
    /// Content name.
    pub name: String,
    /// Localized name (`planet.<name>.name` fallback: the content name).
    pub localized: String,
    /// Shown in the planet access UI.
    pub visible: bool,
    /// Landable (accessible).
    pub accessible: bool,
    /// Has a landable sector grid.
    pub has_grid: bool,
    /// Number of sectors in the grid.
    pub sector_count: usize,
    /// Default start sector index.
    pub start_sector: u16,
    /// Planet-list icon color `rrggbb`.
    pub color: String,
    /// Tech tree name, when assigned.
    pub tech_tree: Option<String>,
    /// Playtime for this planet in milliseconds (`CampaignStats.playtime`).
    pub playtime_ms: i64,
    /// Captured sectors on this planet.
    pub sectors_captured: i32,
    /// Lost sectors on this planet.
    pub sectors_lost: i32,
}

/// One sector marker (`PlanetDialog`/`SectorSelectDialog`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SectorView {
    /// Sector grid index.
    pub id: u16,
    /// Owning planet content name.
    pub planet: String,
    /// Preset content name, when the sector is authored.
    pub preset: Option<String>,
    /// Display name (preset name or `sector-<planet>-<id>` fallback).
    pub name: String,
    /// Preset difficulty (0-10; `-1` when unassigned).
    pub difficulty: f32,
    /// Aggregated threat (0-1).
    pub threat: f32,
    /// Threat bucket name (`low`…`eradication`).
    pub threat_band: &'static str,
    /// Unlocked for landing.
    pub unlocked: bool,
    /// Locked (inverse of `unlocked`).
    pub locked: bool,
    /// Has a base (save + core).
    pub has_base: bool,
    /// Has any save (base or partial).
    pub has_save: bool,
    /// Captured (`isCaptured(None)`).
    pub captured: bool,
    /// Attacked (`isAttacked(None)`).
    pub attacked: bool,
    /// Frozen (attacked and not being played).
    pub frozen: bool,
    /// Shielded by a nearby capture target.
    pub shielded: bool,
    /// Has an enemy base.
    pub has_enemy_base: bool,
    /// Last sector of its campaign.
    pub is_last: bool,
    /// Capture wave (`0` = never).
    pub capture_wave: i32,
}

/// One tech-tree node (`ResearchDialog`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResearchView {
    /// Node index in `TechStore.nodes`.
    pub index: u32,
    /// Content name shown in the node.
    pub content: String,
    /// Depth in the tree.
    pub depth: u32,
    /// Root tree name, when assigned.
    pub root: Option<String>,
    /// Owning tree id, when assigned.
    pub tree: Option<u32>,
    /// Owning planet content name, when assigned.
    pub planet: Option<String>,
    /// Whether the content is already unlocked.
    pub unlocked: bool,
    /// Parent dependencies satisfied.
    pub dependencies_met: bool,
    /// All item requirements satisfied (`finished` amounts match).
    pub requirements_met: bool,
    /// Item requirements `(name, amount)`.
    pub requirements: Vec<(String, i32)>,
    /// Per-item progress `(name, finished)`.
    pub finished: Vec<(String, i32)>,
    /// Objective description strings.
    pub objectives: Vec<String>,
    /// Direct child count.
    pub children: usize,
}

/// One schematic library entry (`SchematicsDialog`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SchematicView {
    /// Index in `Schematics.all`.
    pub index: usize,
    /// Display name (`Schematic.name()`).
    pub name: String,
    /// Description (`Schematic.description()`).
    pub description: String,
    /// Width in tiles.
    pub width: i32,
    /// Height in tiles.
    pub height: i32,
    /// Tile count.
    pub tiles: usize,
    /// Contains a core block.
    pub has_core: bool,
    /// User labels/tags.
    pub labels: Vec<String>,
    /// On-disk file name, when loaded from disk.
    pub file: Option<String>,
    /// Source mod name, when any.
    pub mod_name: Option<String>,
    /// Total build cost `(item name, amount)`.
    pub requirements: Vec<(String, i32)>,
}

/// One launch loadout for a core (`LoadoutDialog`/`LaunchLoadoutDialog`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadoutView {
    /// Core content name.
    pub core: String,
    /// Schematic index in `Schematics.all`.
    pub index: usize,
    /// Display name.
    pub name: String,
    /// Selected as the core's default loadout.
    pub is_default: bool,
    /// Width in tiles.
    pub width: i32,
    /// Height in tiles.
    pub height: i32,
    /// Tile count.
    pub tiles: usize,
    /// Total requirement cost `(item name, amount)`.
    pub requirements: Vec<(String, i32)>,
}

/// `CampaignCompleteDialog` view model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CampaignCompleteView {
    /// Completed planet content name.
    pub planet: String,
    /// Localized planet name.
    pub localized: String,
    /// Planet icon color `rrggbb`.
    pub color: String,
    /// Campaign playtime in milliseconds.
    pub playtime_ms: i64,
    /// Sectors captured on the planet.
    pub sectors_captured: i32,
    /// Waves lasted on the planet.
    pub waves_lasted: i32,
    /// Next campaign planet to continue to, when any.
    pub next_planet: Option<String>,
}

/// One block entry in the placement palette (`PlacementFragment`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockEntryView {
    /// Content name.
    pub name: String,
    /// Bundle key for the display label (`block.<name>.name`) or the localized
    /// name when base content carries one.
    pub localized: String,
    /// Multiblock size in tiles.
    pub size: i32,
}

/// One placement-palette category (`PlacementFragment` category rail).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockCategoryView {
    /// `Category.name()`.
    pub name: &'static str,
    /// Localized label bundle key (`database-tag.<name>`).
    pub label: String,
    /// Buildable blocks in content order.
    pub blocks: Vec<BlockEntryView>,
}

/// The placement-palette catalog consumed by `placement_fragment.gd`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockCatalogView {
    /// Initially selected block name.
    pub selected: String,
    /// Non-empty categories in `Category.all` order.
    pub categories: Vec<BlockCategoryView>,
}

impl BlockCatalogView {
    /// Empty catalog (content boot failure; never panics in a `#[func]`).
    pub fn empty() -> Self {
        Self {
            selected: String::new(),
            categories: Vec::new(),
        }
    }
}

/// Builds the placement-palette catalog from base content.
///
/// The custom/sandbox inventory: `unlockedNowHost` is true outside a campaign,
/// but `PlacementFragment.getUnlockedByCategory` still hides hidden/debug/
/// editor content, other-planet content and non-placeable blocks. The default
/// `State.getPlanet()` (Serpulo) supplies the environment context.
pub fn block_catalog() -> BlockCatalogView {
    block_catalog_with(None, None)
}

/// Custom/sandbox catalog for one planet (`Block.environmentBuildable`).
pub fn block_catalog_for(planet: &str) -> BlockCatalogView {
    block_catalog_with(None, Some(planet))
}

/// Builds the placement-palette catalog filtered by live unlock state.
///
/// `store` is the campaign unlock store: tech-gated blocks are dropped until
/// their `<name>-unlocked` bit is set (`Block.unlockedNowHost`) and the
/// campaign's non-build `BuildVisibility` values are removed. The generated
/// specs leave `build_visibility` to the resolver, which marks
/// requirement-bearing build-menu blocks `Shown` and requirement-less world
/// content `Hidden` (`Block.requirements(Category, ItemStack...)` defaults to
/// `BuildVisibility.shown`), so only real buildables reach this filter.
/// [`block_catalog`] keeps the no-unlock inventory when no store is bound.
pub fn block_catalog_unlocked(store: &dyn UnlockStore) -> BlockCatalogView {
    block_catalog_with(Some(store), None)
}

/// Unlock-filtered catalog for one campaign planet
/// (`PlacementFragment.getUnlockedByCategory` reads `state.getPlanet()` for
/// `Block.environmentBuildable`).
pub fn block_catalog_unlocked_for(store: &dyn UnlockStore, planet: &str) -> BlockCatalogView {
    block_catalog_with(Some(store), Some(planet))
}

/// Whether a tech node is granted at boot: no effective item requirements and
/// no objectives (`Control.checkAutoUnlocks` roots).
///
/// The generated vanilla trees leave `TechNode.requirements` empty and derive
/// the research cost from the content
/// (`UnlockableContent.researchRequirements()`), so the check must go through
/// [`tech_tree::effective_requirements`] instead of the stored list.
fn tech_node_auto_unlocks(
    registry: &ContentRegistry,
    node_ref: crate::content::tech::TechNodeRef,
) -> bool {
    let Some(node) = registry.tech().node(node_ref) else {
        return false;
    };
    tech_tree::effective_requirements(registry, node_ref).is_empty() && node.objectives.is_empty()
}

/// Shared catalog builder; `store` enables the unlock/visibility filter and
/// `planet` overrides the default Serpulo `environmentBuildable` context.
fn block_catalog_with(store: Option<&dyn UnlockStore>, planet: Option<&str>) -> BlockCatalogView {
    use crate::content::registries::blocks::BuildVisibility;
    use crate::content::{
        Category, MemoryBundle, MemoryUnlockStore, PlanetId, create_base_content,
    };

    let empty = MemoryUnlockStore::new();
    let store_ref: &dyn UnlockStore = match store {
        Some(store) => store,
        None => &empty,
    };
    let Ok(mut registry) = create_base_content(&MemoryBundle::new(), store_ref, true) else {
        return BlockCatalogView::empty();
    };
    // `init`/`post_init` derive per-block fields the build-menu filter reads
    // (`build_visibility`, `build_time`) and auto-assign `shownPlanets`.
    if registry.init().is_err() || registry.post_init().is_err() {
        return BlockCatalogView::empty();
    }
    // `State.getPlanet()` falls back to the rules planet (`serpulo` by default).
    let planet: Option<PlanetId> = planet
        .and_then(|name| registry.planet_id(name))
        .or_else(|| registry.planet_id("serpulo"));
    let mut categories: Vec<BlockCategoryView> = Category::ALL
        .iter()
        .map(|category| BlockCategoryView {
            name: category.name(),
            label: format!("database-tag.{}", category.name()),
            blocks: Vec::new(),
        })
        .collect();
    for block in registry.blocks() {
        // `Block.isVisible()` metadata half: hidden/debug/editor content never
        // reaches the player's placement palette.
        if block.removed
            || matches!(
                block.build_visibility,
                BuildVisibility::Hidden | BuildVisibility::DebugOnly | BuildVisibility::EditorOnly
            )
        {
            continue;
        }
        // `PlacementFragment.unlocked(block)`: player-placeable content only.
        if !block.placeable_player {
            continue;
        }
        // `Block.environmentBuildable()` (`isOnPlanet`): the auto-assigned
        // `shownPlanets` drop other planets' content (e.g. Erekir cores).
        if let Some(planet) = planet
            && !block.unlock.shown_planets.is_empty()
            && !block.unlock.shown_planets.contains(&planet)
        {
            continue;
        }
        if store.is_some() {
            if matches!(
                block.build_visibility,
                BuildVisibility::SandboxOnly
                    | BuildVisibility::CoreZoneOnly
                    | BuildVisibility::WorldProcessorOnly
                    | BuildVisibility::LegacyLaunchPadOnly
                    | BuildVisibility::LightingOnly
                    | BuildVisibility::FogOnly
            ) {
                continue;
            }
            // Tech-gated blocks need their unlock bit (or be a zero-requirement,
            // objective-free root that `check_auto_unlocks` grants at boot);
            // blocks outside the tech tree stay available (the port's specs do
            // not model `alwaysUnlocked` for them).
            if let Some(node_ref) = block.unlock.tech_node
                && !block.unlock.unlocked()
                && !tech_node_auto_unlocks(&registry, node_ref)
            {
                continue;
            }
        }
        let localized = if block.unlock.localized_name.is_empty()
            || block.unlock.localized_name == block.name
        {
            format!("block.{}.name", block.name)
        } else {
            block.unlock.localized_name.clone()
        };
        categories[block.category.ordinal()]
            .blocks
            .push(BlockEntryView {
                name: block.name.clone(),
                localized,
                size: block.size,
            });
    }
    categories.retain(|category| !category.blocks.is_empty());
    BlockCatalogView {
        selected: String::from("conveyor"),
        categories,
    }
}

/// A `MapPlayDialog`/`CustomGameDialog`/`EditorMapsDialog` map row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MapEntryView {
    /// Map file stem (parity name).
    pub name: String,
    /// Author, when known.
    pub author: Option<String>,
    /// Width in tiles.
    pub width: i32,
    /// Height in tiles.
    pub height: i32,
    /// Custom map (not a bundled campaign sector).
    pub custom: bool,
}

/// Complete campaign projection consumed by the M5 dialogs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CampaignViews {
    /// Active planet content name.
    pub planet: String,
    /// Planet cards (`PlanetDialog`).
    pub planets: Vec<PlanetView>,
    /// Sector markers for the active planet (`PlanetDialog`/`SectorSelectDialog`).
    pub sectors: Vec<SectorView>,
    /// Tech nodes for the active planet (`ResearchDialog`).
    pub research: Vec<ResearchView>,
    /// Schematic library (`SchematicsDialog`).
    pub schematics: Vec<SchematicView>,
    /// Loadouts for the active planet's default core (`LoadoutDialog`).
    pub loadouts: Vec<LoadoutView>,
    /// Campaign rules (`CampaignRulesDialog`).
    pub rules: CampaignRulesView,
    /// Campaign completion (`CampaignCompleteDialog`).
    pub complete: CampaignCompleteView,
    /// Map list (`MapPlayDialog`/`CustomGameDialog`/`EditorMapsDialog`).
    pub maps: Vec<MapEntryView>,
}

/// Formats an `Rgba` as `rrggbb`.
pub fn hex(rgba: &crate::content::Rgba) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "{:02x}{:02x}{:02x}",
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b)
    )
}

/// `planet.<name>.name` bundle key (localized planet label).
fn planet_localized(registry: &ContentRegistry, name: &str) -> String {
    registry
        .planet_by_name(name)
        .map(|def| def.unlock.localized_name.clone())
        .filter(|value| !value.is_empty() && value != name)
        .unwrap_or_else(|| name.to_owned())
}

/// Builds one planet card.
fn planet_view(registry: &ContentRegistry, planet: &Planet) -> PlanetView {
    let def = registry.planet(planet.id);
    let (visible, accessible, has_grid, sector_count, start_sector, color, tech_tree) = def
        .map(|def| {
            (
                def.visible,
                def.accessible,
                def.sector_tiles > 0,
                def.sector_count,
                def.start_sector.min(u16::MAX as u32) as u16,
                hex(&def.icon_color),
                def.tech_tree
                    .and_then(|tree| registry.tech().tree(tree).map(|t| t.name.clone())),
            )
        })
        .unwrap_or((
            false,
            false,
            false,
            planet.sector_count(),
            planet.start_sector,
            String::from("ffffff"),
            None,
        ));
    PlanetView {
        id: planet.id.raw(),
        name: planet.name.clone(),
        localized: planet_localized(registry, &planet.name),
        visible,
        accessible,
        has_grid,
        sector_count,
        start_sector,
        color,
        tech_tree,
        playtime_ms: planet.stats.playtime,
        sectors_captured: planet.stats.sectors_captured,
        sectors_lost: planet.stats.sectors_lost,
    }
}

/// Builds one sector marker.
fn sector_view(planet: &Planet, sector: &Sector) -> SectorView {
    let name = sector
        .preset_name
        .clone()
        .unwrap_or_else(|| format!("{}-sector-{}", planet.name, sector.id));
    SectorView {
        id: sector.id,
        planet: planet.name.clone(),
        preset: sector.preset_name.clone(),
        name,
        difficulty: sector.preset_difficulty,
        threat: sector.threat,
        threat_band: sector.threat_bucket(),
        unlocked: sector.unlocked(),
        locked: sector.locked(),
        has_base: sector.has_base(),
        has_save: sector.has_save(),
        captured: sector.is_captured(None),
        attacked: sector.is_attacked(None),
        frozen: sector.is_frozen(None),
        shielded: sector.is_shielded(),
        has_enemy_base: sector.has_enemy_base(),
        is_last: sector_is_last(&planet.name, sector),
        capture_wave: sector.preset_capture_wave,
    }
}

/// `SectorPreset.isLastSector` via the content preset table.
fn sector_is_last(planet_name: &str, sector: &Sector) -> bool {
    // The preset flag is copied onto the content def; the runtime sector only
    // records the preset name, so prefer the per-sector content record below.
    let _ = planet_name;
    sector.preset.map(|_| false).unwrap_or(false)
}

/// Builds the sector markers for a planet.
pub fn sector_views(planet: &Planet, presets: &[SectorPresetDef]) -> Vec<SectorView> {
    planet
        .sectors
        .iter()
        .map(|sector| {
            let mut view = sector_view(planet, sector);
            if let Some(preset) = sector
                .preset
                .and_then(|id| presets.get(id.raw() as usize))
                .filter(|preset| preset.planet == planet.id)
            {
                view.is_last = preset.is_last_sector;
                view.capture_wave = preset.capture_wave;
                view.difficulty = preset.difficulty;
            }
            view
        })
        .collect()
}

/// Builds the tech-tree nodes for a planet (or all nodes when `planet` is None).
pub fn research_views(registry: &ContentRegistry, planet: Option<PlanetId>) -> Vec<ResearchView> {
    let tech = registry.tech();
    tech.nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| planet.is_none_or(|id| node.planet == Some(id)))
        .map(|(index, node)| {
            let node_ref = crate::content::tech::TechNodeRef(index as u32);
            let requirements = node
                .requirements
                .iter()
                .map(|stack| (item_name(registry, stack.item), stack.amount))
                .collect();
            let finished = node
                .finished_requirements
                .iter()
                .map(|stack| (item_name(registry, stack.item), stack.amount))
                .collect();
            let requirements_met = node
                .requirements
                .iter()
                .zip(node.finished_requirements.iter())
                .all(|(need, done)| done.amount >= need.amount);
            let unlocked = node
                .content
                .is_none_or(|content| tech_tree::content_unlocked(registry, content));
            ResearchView {
                index: index as u32,
                content: node.content_name.clone(),
                depth: node.depth,
                root: node.name.clone(),
                tree: node.tree.map(|tree| tree.index() as u32),
                planet: node.planet.map(|id| planet_name(registry, id)),
                unlocked,
                dependencies_met: tech_tree::dependencies_unlocked(registry, node_ref),
                requirements_met,
                requirements,
                finished,
                objectives: node
                    .objectives
                    .iter()
                    .map(|spec| format!("{spec:?}"))
                    .collect(),
                children: node.children.len(),
            }
        })
        .collect()
}

/// Item name by id (fallback `item-<id>`).
fn item_name(registry: &ContentRegistry, id: crate::content::ItemId) -> String {
    registry
        .item(id)
        .map(|item| item.name.clone())
        .unwrap_or_else(|| format!("item-{}", id.raw()))
}

/// Planet name by id (fallback `planet-<id>`).
fn planet_name(registry: &ContentRegistry, id: PlanetId) -> String {
    registry
        .planet(id)
        .map(|planet| planet.name.clone())
        .unwrap_or_else(|| format!("planet-{}", id.raw()))
}

/// Total requirement cost as `(item name, amount)` pairs.
fn requirement_pairs(
    registry: &ContentRegistry,
    seq: &crate::content::stacks::ItemSeq,
) -> Vec<(String, i32)> {
    (0..seq.len())
        .filter_map(|index| {
            let id = crate::content::ItemId::new(index as u16);
            let amount = seq.get(id);
            (amount != 0).then(|| (item_name(registry, id), amount))
        })
        .collect()
}

/// Builds the schematic library rows.
pub fn schematic_views(schematics: &Schematics, registry: &ContentRegistry) -> Vec<SchematicView> {
    schematics
        .all
        .iter()
        .enumerate()
        .map(|(index, schematic)| SchematicView {
            index,
            name: schematic.name(),
            description: schematic.description(),
            width: schematic.width,
            height: schematic.height,
            tiles: schematic.tiles.len(),
            has_core: schematic.has_core(registry),
            labels: schematic.labels.clone(),
            file: schematic.file.clone(),
            mod_name: schematic.mod_name.clone(),
            requirements: requirement_pairs(registry, &schematic.requirements(registry)),
        })
        .collect()
}

/// Builds the loadout rows for a core content name.
pub fn loadout_views(
    schematics: &Schematics,
    registry: &ContentRegistry,
    core_name: &str,
) -> Vec<LoadoutView> {
    let Some(core) = registry.block_id(core_name) else {
        return Vec::new();
    };
    let default = schematics.default_loadouts.get(&core.raw()).copied();
    schematics
        .loadouts
        .get(&core.raw())
        .into_iter()
        .flatten()
        .filter_map(|index| {
            let schematic = schematics.all.get(*index)?;
            Some(LoadoutView {
                core: core_name.to_owned(),
                index: *index,
                name: schematic.name(),
                is_default: default == Some(*index),
                width: schematic.width,
                height: schematic.height,
                tiles: schematic.tiles.len(),
                requirements: requirement_pairs(registry, &schematic.requirements(registry)),
            })
        })
        .collect()
}

/// Builds the campaign rules view.
pub fn rules_view(planet: &str, rules: &CampaignRules) -> CampaignRulesView {
    let difficulties = ALL_DIFFICULTIES
        .iter()
        .map(|difficulty| DifficultyView {
            name: difficulty.name(),
            selected: *difficulty == rules.difficulty,
            localized: difficulty.localized(),
            info: difficulty.info(),
            enemy_health: difficulty.enemy_health_multiplier(),
            enemy_spawn: difficulty.enemy_spawn_multiplier(),
            wave_time: difficulty.wave_time_multiplier(),
        })
        .collect();
    let toggles = [
        ("fog", rules.fog),
        ("hide_spawns", rules.hide_spawns),
        ("sector_invasion", rules.sector_invasion),
        ("random_wave_ai", rules.random_wave_ai),
        ("rts_ai", rules.rts_ai),
    ]
    .into_iter()
    .map(|(key, value)| RuleToggleView {
        key: key.to_owned(),
        value,
    })
    .collect();
    CampaignRulesView {
        planet: planet.to_owned(),
        difficulty: rules.difficulty.name(),
        difficulties,
        toggles,
    }
}

/// Builds the completion view.
pub fn complete_view(
    campaign: &Campaign,
    registry: &ContentRegistry,
    planet: &Planet,
) -> CampaignCompleteView {
    let next = campaign
        .order
        .iter()
        .filter_map(|id| campaign.planets.get(id))
        .find(|candidate| candidate.id != planet.id && candidate.parent == planet.parent)
        .map(|candidate| candidate.name.clone());
    CampaignCompleteView {
        planet: planet.name.clone(),
        localized: planet_localized(registry, &planet.name),
        color: hex(&registry
            .planet(planet.id)
            .map(|def| def.icon_color)
            .unwrap_or(crate::content::Rgba {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            })),
        playtime_ms: planet.stats.playtime,
        sectors_captured: planet.stats.sectors_captured,
        waves_lasted: planet.stats.waves_lasted,
        next_planet: next,
    }
}

/// Map rows for `MapPlayDialog`/`CustomGameDialog`/`EditorMapsDialog`.
///
/// The constructor takes plain metadata so plan 06's `Maps` registry (or a
/// headless fixture) can supply rows without this module depending on the IO
/// layer; the M8 scenario feeds the committed fixture set.
pub fn map_views(entries: &[MapEntryView]) -> Vec<MapEntryView> {
    let mut rows = entries.to_vec();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

/// Built-in map rows from `Maps.defaultMapNames` (`MapListDialog` built-in set).
///
/// Sizes are unknown here because loading the `MSAV` headers needs the IO layer;
/// the map grid renders the name and a placeholder preview until plan 19 supplies
/// the metadata and generated previews. Rows are sorted by name via [`map_views`].
pub fn default_map_entries() -> Vec<MapEntryView> {
    map_views(
        &crate::maps::DEFAULT_MAP_NAMES
            .iter()
            .map(|name| MapEntryView {
                name: (*name).to_owned(),
                author: None,
                width: 0,
                height: 0,
                custom: false,
            })
            .collect::<Vec<_>>(),
    )
}

impl CampaignViews {
    /// Projects the complete campaign state into the M5 dialog read models.
    pub fn from_campaign(
        campaign: &Campaign,
        registry: &ContentRegistry,
        schematics: &Schematics,
        active: PlanetId,
    ) -> Self {
        let planet = campaign
            .planet(active)
            .unwrap_or_else(|| &campaign.planets[0]);
        let presets = registry.sectors();
        let core_name = registry
            .planet(active)
            .and_then(|def| def.default_core.clone())
            .unwrap_or_else(|| "core-shard".to_owned());
        Self {
            planet: planet.name.clone(),
            planets: campaign
                .order
                .iter()
                .filter_map(|id| campaign.planets.get(id))
                .map(|planet| planet_view(registry, planet))
                .collect(),
            sectors: sector_views(planet, presets),
            research: research_views(registry, Some(active)),
            schematics: schematic_views(schematics, registry),
            loadouts: loadout_views(schematics, registry, &core_name),
            rules: rules_view(
                &planet.name,
                &registry
                    .planet(active)
                    .map(|_| planet.campaign_rules.clone())
                    .unwrap_or_default(),
            ),
            complete: complete_view(campaign, registry, planet),
            maps: Vec::new(),
        }
    }

    /// Deterministic vanilla fixture used by `mind-headless ui campaign` and the
    /// `MindUi.campaign_views()` endpoint (M5). Boots base content, builds the
    /// runtime campaign and seeds a couple of sectors so every view branch has
    /// representative data without a live world or server.
    pub fn vanilla_fixture() -> Self {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;

        let registry =
            match create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true) {
                Ok(registry) => registry,
                Err(_) => return Self::empty(),
            };
        let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let active = campaign.planet_id_by_name("serpulo").or_else(|| {
            campaign
                .order
                .first()
                .and_then(|id| campaign.planets.get(id))
                .map(|planet| planet.id)
        });
        let Some(active) = active else {
            return Self::empty();
        };

        // Make one sector owned/played and one captured so flags are exercised.
        if let Some(preset) = registry.sector_by_name("groundZero")
            && let Some(sector) = campaign.sector_mut(active, preset.sector)
        {
            sector.save = Some(format!("sector-serpulo-{}", preset.sector));
            sector.info.info.has_core = true;
            sector.info.info.storage_capacity = 4000;
            sector.info.info.waves = false;
            sector.info.info.attack = false;
            sector.info.info.wave = 1;
            sector.info.info.best_core_type = "core-shard".to_owned();
        }
        if let Some(preset) = registry.sector_by_name("saltFlats")
            && let Some(sector) = campaign.sector_mut(active, preset.sector)
        {
            sector.save = Some(format!("sector-serpulo-{}", preset.sector));
            sector.info.info.has_core = true;
            sector.info.info.waves = false;
            sector.info.info.attack = false;
        }
        if let Some(planet) = campaign.planet_mut(active) {
            planet.stats.playtime = 3_600_000;
            planet.stats.sectors_captured = 2;
            planet.stats.waves_lasted = 12;
            planet.stats.add_enemy_unit_destroyed("dagger", 9);
        }

        let mut schematics = Schematics::new();
        schematics.load_loadouts(&registry);
        Self::from_campaign(&campaign, &registry, &schematics, active)
    }

    /// Empty projection (content boot failure; never panics in a `#[func]`).
    pub fn empty() -> Self {
        Self {
            planet: String::new(),
            planets: Vec::new(),
            sectors: Vec::new(),
            research: Vec::new(),
            schematics: Vec::new(),
            loadouts: Vec::new(),
            rules: rules_view("", &CampaignRules::default()),
            complete: CampaignCompleteView {
                planet: String::new(),
                localized: String::new(),
                color: String::from("ffffff"),
                playtime_ms: 0,
                sectors_captured: 0,
                waves_lasted: 0,
                next_planet: None,
            },
            maps: Vec::new(),
        }
    }
}

/// Renders the empty-bundle difficulty label for a name (parity helper).
pub fn difficulty_label(difficulty: Difficulty) -> String {
    difficulty.localized()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_projects_all_m5_dialogs() {
        let views = CampaignViews::vanilla_fixture();
        assert_eq!(views.planet, "serpulo", "active planet is serpulo");
        assert!(
            views.planets.len() >= 7,
            "all visible planets present: {}",
            views.planets.len()
        );
        assert!(!views.sectors.is_empty(), "serpulo sectors present");
        assert!(!views.research.is_empty(), "tech nodes present");
        assert_eq!(views.schematics.len(), 4, "four vanilla loadout schematics");
        assert!(!views.loadouts.is_empty(), "core-shard loadouts present");
        assert_eq!(views.rules.difficulties.len(), 5, "all five difficulties");
        assert_eq!(views.rules.difficulty, "normal", "default difficulty");
    }

    #[test]
    fn fixture_sector_flags_are_sane() {
        let views = CampaignViews::vanilla_fixture();
        let ground_zero = views
            .sectors
            .iter()
            .find(|sector| sector.preset.as_deref() == Some("groundZero"))
            .expect("groundZero sector");
        assert!(ground_zero.has_base, "seeded base");
        assert!(ground_zero.unlocked, "base implies unlocked");
        assert!(!ground_zero.attacked, "seeded calm");
        let salt = views
            .sectors
            .iter()
            .find(|sector| sector.preset.as_deref() == Some("saltFlats"))
            .expect("saltFlats sector");
        assert!(salt.captured, "seeded captured");
    }

    #[test]
    fn rules_view_marks_selected_difficulty() {
        let rules = CampaignRules {
            difficulty: Difficulty::Eradication,
            fog: true,
            ..CampaignRules::default()
        };
        let view = rules_view("serpulo", &rules);
        assert_eq!(view.difficulty, "eradication");
        assert_eq!(view.difficulties.iter().filter(|d| d.selected).count(), 1);
        assert!(
            view.difficulties
                .iter()
                .any(|d| d.name == "eradication" && d.selected)
        );
        assert!(
            view.toggles
                .iter()
                .any(|toggle| toggle.key == "fog" && toggle.value)
        );
    }

    #[test]
    fn research_view_tracks_requirements() {
        let views = CampaignViews::vanilla_fixture();
        let with_requirements = views
            .research
            .iter()
            .find(|node| !node.requirements.is_empty());
        if let Some(node) = with_requirements {
            assert_eq!(node.requirements.len(), node.finished.len());
            assert!(node.requirements.iter().all(|(_, amount)| *amount > 0));
        }
        assert!(
            views.research.iter().any(|node| node.depth == 0),
            "tree roots present"
        );
    }

    #[test]
    fn schematic_rows_carry_core_and_requirements() {
        let views = CampaignViews::vanilla_fixture();
        assert!(views.schematics.iter().any(|schem| schem.has_core));
        assert!(
            views
                .schematics
                .iter()
                .all(|schem| schem.width > 0 && schem.height > 0)
        );
        assert!(
            views
                .schematics
                .iter()
                .any(|schem| !schem.requirements.is_empty()),
            "loadouts have build costs"
        );
    }

    #[test]
    fn map_views_sort_by_name() {
        let rows = map_views(&[
            MapEntryView {
                name: "zeta".to_owned(),
                author: None,
                width: 10,
                height: 10,
                custom: true,
            },
            MapEntryView {
                name: "alpha".to_owned(),
                author: Some("anuke".to_owned()),
                width: 20,
                height: 20,
                custom: false,
            },
        ]);
        assert_eq!(rows[0].name, "alpha");
        assert_eq!(rows[1].name, "zeta");
    }

    #[test]
    fn block_catalog_covers_build_menu() {
        let catalog = block_catalog();
        assert!(
            catalog.categories.len() >= 4,
            "several non-empty categories: {}",
            catalog.categories.len()
        );
        assert!(
            catalog
                .categories
                .iter()
                .all(|category| !category.blocks.is_empty()),
            "no empty category survives the filter"
        );
        let has = |name: &str| {
            catalog
                .categories
                .iter()
                .any(|category| category.blocks.iter().any(|block| block.name == name))
        };
        assert!(has("conveyor"), "conveyor is in the distribution category");
        // World/editor content carries no build requirements, so the resolver
        // leaves it `BuildVisibility.hidden` (`PlacementFragment.getByCategory`
        // only sees visible, placeable, environment-buildable blocks).
        for world in [
            "air",
            "spawn",
            "remove-wall",
            "deep-water",
            "stone-wall",
            "ore-copper",
            "build2",
            "legacy-mech-pad",
            "command-center",
        ] {
            assert!(!has(world), "world content `{world}` is not build-menu");
        }
        // `Block.environmentBuildable()`: other planets' content stays out of
        // the Serpulo palette (the default `State.getPlanet()`).
        assert!(!has("core-bastion"), "the Erekir core is hidden on Serpulo");
        assert!(
            catalog
                .categories
                .iter()
                .all(|category| { category.label == format!("database-tag.{}", category.name) }),
            "category labels are database-tag bundle keys"
        );
    }

    #[test]
    fn unlocked_catalog_filters_visibility_and_tech_gates() {
        use crate::content::MemoryUnlockStore;

        let full = block_catalog();
        let has = |catalog: &BlockCatalogView, name: &str| {
            catalog
                .categories
                .iter()
                .any(|category| category.blocks.iter().any(|block| block.name == name))
        };
        assert!(
            has(&full, "power-source"),
            "sandbox block in the raw inventory"
        );
        let filtered = block_catalog_unlocked(&MemoryUnlockStore::new());
        assert!(
            filtered.categories.is_empty(),
            "a fresh campaign with no unlock bits has no build-menu content: {:?}",
            filtered
                .categories
                .iter()
                .map(|category| category.name)
                .collect::<Vec<_>>()
        );
        assert!(
            !has(&filtered, "power-source"),
            "sandbox-only block is not buildable in campaign"
        );
        assert!(
            !has(&filtered, "conveyor"),
            "a research-gated block is hidden with an empty unlock store"
        );
        for locked in ["foreshadow", "spectre", "meltdown", "malign"] {
            assert!(
                !has(&filtered, locked),
                "locked turret `{locked}` is not buildable"
            );
        }
        assert!(
            filtered
                .categories
                .iter()
                .all(|category| !category.blocks.is_empty()),
            "empty categories are hidden"
        );

        // The `<name>-unlocked` bit (what research and sector auto-unlocks
        // write) puts the block back into the palette.
        let mut store = MemoryUnlockStore::new();
        store.set_bool("duo-unlocked", true);
        let unlocked = block_catalog_unlocked(&store);
        let turret = unlocked
            .categories
            .iter()
            .find(|category| category.name == "turret")
            .expect("unlocking a turret yields its category");
        assert_eq!(
            turret
                .blocks
                .iter()
                .map(|block| block.name.as_str())
                .collect::<Vec<_>>(),
            vec!["duo"],
            "exactly the researched block appears"
        );
        assert_eq!(
            unlocked.categories.len(),
            1,
            "other categories stay hidden: {:?}",
            unlocked
                .categories
                .iter()
                .map(|category| category.name)
                .collect::<Vec<_>>()
        );
        assert!(
            !has(&unlocked, "foreshadow"),
            "still-locked turret stays hidden"
        );

        // `Block.environmentBuildable()`: the same unlock state yields another
        // planet's palette (Erekir exposes its always-unlocked core).
        let erekir = block_catalog_unlocked_for(&MemoryUnlockStore::new(), "erekir");
        assert!(
            has(&erekir, "core-bastion"),
            "Erekir's always-unlocked core is in the Erekir palette"
        );
        assert!(!has(&erekir, "duo"), "Serpulo content stays off Erekir");

        // The custom/sandbox catalog takes the same planet context.
        let erekir_full = block_catalog_for("erekir");
        assert!(has(&erekir_full, "core-bastion"));
        assert!(!has(&erekir_full, "duo"));
    }

    #[test]
    fn live_projection_tracks_sector_capture() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::{PlaySession, sector_capture};
        use crate::game::rules::Rules;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content boot");
        let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let active = campaign.planet_id_by_name("serpulo").expect("serpulo");
        let sector_id = registry
            .sector_by_name("groundZero")
            .expect("groundZero preset")
            .sector;
        let mut schematics = Schematics::new();
        schematics.load_loadouts(&registry);

        let ground_zero = |views: &CampaignViews| {
            views
                .sectors
                .iter()
                .find(|sector| sector.id == sector_id)
                .cloned()
                .expect("groundZero row")
        };
        let before = ground_zero(&CampaignViews::from_campaign(
            &campaign,
            &registry,
            &schematics,
            active,
        ));
        assert!(
            !before.has_base && !before.captured,
            "fresh campaign is unowned"
        );

        // Launch (host writes the save/core) then capture, as the facade does.
        if let Some(sector) = campaign.sector_mut(active, sector_id) {
            sector.save = Some(format!("sector-serpulo-{sector_id}"));
            sector.info.info.has_core = true;
        }
        let mut session = PlaySession::new(Rules::default());
        session.sector = Some((active, sector_id));
        let events = sector_capture(&mut session, &mut campaign);
        assert!(!events.is_empty(), "capture emits an event");

        let after = ground_zero(&CampaignViews::from_campaign(
            &campaign,
            &registry,
            &schematics,
            active,
        ));
        assert!(after.has_base, "the launched base is projected");
        assert!(after.captured, "the capture is projected");
    }

    #[test]
    fn hex_formats_colors() {
        let color = crate::content::Rgba {
            r: 1.0,
            g: 0.0,
            b: 0.5,
            a: 1.0,
        };
        assert_eq!(hex(&color), "ff0080");
    }
}
