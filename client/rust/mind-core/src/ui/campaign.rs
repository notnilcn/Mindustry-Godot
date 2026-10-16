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
use crate::content::tech::ObjectiveSpec;
use crate::content::{ContentRegistry, PlanetId, UnlockStore};
use crate::game::campaign_rules::{ALL_DIFFICULTIES, CampaignRules, Difficulty};
use crate::game::objectives::{CampaignObjectiveContext, Objective, ObjectiveContext};
use crate::game::planet::Planet;
use crate::game::schematics::Schematics;
use crate::game::sector::Sector;
use crate::game::tech_tree;
use crate::game::universe::Campaign;
use crate::io::json::rules::ExportStat;

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
    /// `Planet.allowSectorInvasion` (`CampaignRulesDialog` gates the
    /// `@rules.invasions` toggle on it).
    pub allow_sector_invasion: bool,
    /// `Planet.showRtsAIRule` (gates `@rules.rtsai.campaign`).
    pub show_rts_ai_rule: bool,
    /// `Planet.clearSectorOnLose` (hides `@rules.clearsectoronloss` when true).
    pub clear_sector_on_lose: bool,
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
    /// Display name (`Sector.name()`: `info.name`, preset, single-sector
    /// planet name, else the grid index).
    pub name: String,
    /// The player renamed the sector (`info.name` set): the dialog shows
    /// `name` verbatim instead of the localized preset name.
    pub named: bool,
    /// `PlanetDialog.canSelect` (look mode): the globe draws the cell's status
    /// ring instead of the shadow fill.
    pub selectable: bool,
    /// `preset.requireUnlock`.
    pub require_unlock: bool,
    /// `preset.unlocked()`.
    pub preset_unlocked: bool,
    /// `renderProjections` padlock: a locked, tech-gated preset whose parent
    /// node is unlocked.
    pub lock_icon: bool,
    /// `sector.info.icon` / `contentIcon` (player-chosen marker icon).
    pub icon: Option<String>,
    /// `sector.info.contentIcon` (content uiIcon; drawn white).
    pub content_icon: Option<String>,
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
    /// `PlanetDialog.showStats` data block (production/export/import/stored).
    pub stats: SectorStatsView,
    /// `PlanetDialog.findLauncher` result (look mode), sector id.
    pub launch_from: Option<u16>,
    /// `PlanetDialog.hasNoCandidate`: a non-start, baseless sector with no
    /// launch candidate.
    pub no_candidate: bool,
    /// `updateSelected` action-button gate: `(hasBase && look) || canSelect ||
    /// preset.alwaysUnlocked`.
    pub show_action: bool,
    /// `isLocked(sector)`: preset locked, no base and the preset has a tech
    /// node.
    pub objectives_locked: bool,
    /// Incomplete objectives of the preset's tech node: `(kind, argument)`.
    pub objectives: Vec<(String, String)>,
    /// `sectors.vulnerable` rule (sectorInvasion + enemy-base neighbour).
    pub vulnerable: bool,
    /// `displayThreat` colour, 8 hex digits (`ff998dff`).
    pub threat_color: String,
}

/// One `@sectors.import` source row (`SectorInfo.eachImport`): the sector that
/// exports into the shown one (`PlanetDialog.java:1109-1113`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SectorStatRef {
    /// Planet content name.
    pub planet: String,
    /// Sector grid index.
    pub sector: u16,
    /// Display name (`Sector.name()`).
    pub name: String,
    /// `info.icon` (`Iconc` field name; export/import row prefix).
    pub icon: Option<String>,
    /// `info.contentIcon` (content name; export/import row prefix).
    pub content_icon: Option<String>,
}

/// `PlanetDialog.showStats` data: per-minute rate rows for `production`,
/// `export` and `import` (entries with `mean * 60 > 1`) plus attempts/wave and
/// the stored item pool.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SectorStatsView {
    /// `sector.info.attempts`.
    pub attempts: i32,
    /// `sector.info.wave` when waves are live on a base sector.
    pub wave: i32,
    /// `(item, per-minute)` production rows.
    pub production: Vec<(String, i32)>,
    /// `(item, per-minute)` export rows.
    pub export: Vec<(String, i32)>,
    /// `(item, per-minute)` import rows.
    pub import: Vec<(String, i32)>,
    /// `(item, amount)` stored rows (`sector.items()` off-play: the record pool).
    pub stored: Vec<(String, i32)>,
    /// `sector.info.resources` content names (`@sectors.resources` icon row;
    /// `Sector.loadInfo` folds the legacy `water` block into the liquid).
    pub resources: Vec<String>,
    /// `SaveSlot.getPlayTime()` in ms (`meta.timePlayed`); filled by the host
    /// from the save meta (`PlanetDialog.java:1053`).
    pub playtime_ms: i64,
    /// `sector.info.destination` when the target has a base (the export row's
    /// `Iconc.rightOpen` line).
    pub destination: Option<SectorStatRef>,
    /// `SectorInfo.eachImport` sources (the import row's lines).
    pub import_sources: Vec<SectorStatRef>,
    /// `sector.isAttacked() && !sector.isBeingPlayed()` (`@sector.lockdown`).
    pub lockdown: bool,
    /// `sector.preset.displayDescription` bundle key
    /// (`sector.<preset>.description`), when the preset has a description.
    pub preset_description_key: Option<String>,
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

/// One placed tile of a schematic (`Schematic.Stile`) for the preview widget.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SchematicTileView {
    /// Block content name (`block-<name>-full` region).
    pub block: String,
    /// Local tile x (relative to the schematic origin).
    pub x: i32,
    /// Local tile y.
    pub y: i32,
    /// Rotation 0-3.
    pub rotation: u8,
    /// Block footprint size (1/2/3/4).
    pub size: i32,
    /// `kind_rotates(def.kind)`: draw the plan region rotated.
    pub rotate: bool,
    /// Power-node laser targets as indices into the view's `tiles`
    /// (`PowerNode.drawPlanConfigTop`, drawn by `Schematics.getBuffer`).
    /// Omitted when empty (non-node tiles), which keeps the row shape stable.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lasers: Vec<usize>,
}

/// `PowerNode.drawPlanConfigTop` (`PowerNode.java:328-351`): for each
/// `Point2[]` link offset, the first other plan whose footprint covers
/// `(x + dx, y + dy)` and whose block `hasPower` (`planFinder`).
fn schematic_lasers(
    schematic: &crate::game::schematic::Schematic,
    tiles: &[(usize, &crate::content::registries::blocks::BlockDef)],
    own: usize,
) -> Vec<usize> {
    use crate::content::registries::blocks::BlockKind;
    use crate::io::typeio::{unpack_point2_x, unpack_point2_y};
    use crate::world::config::ConfigValue;
    let Some(&(stile_index, def)) = tiles.get(own) else {
        return Vec::new();
    };
    if !matches!(def.kind, BlockKind::PowerNode | BlockKind::LongPowerNode) {
        return Vec::new();
    }
    let Some(plan) = schematic.tiles.get(stile_index) else {
        return Vec::new();
    };
    let ConfigValue::Point2Array(points) = &plan.config else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for &point in points.iter() {
        let find_x = plan.x as i32 + unpack_point2_x(point);
        let find_y = plan.y as i32 + unpack_point2_y(point);
        let found = tiles
            .iter()
            .enumerate()
            .find(|(index, (other_index, other))| {
                let Some(other_plan) = schematic.tiles.get(*other_index) else {
                    return false;
                };
                let (ox, oy) = (other_plan.x as i32, other_plan.y as i32);
                *index != own
                    && other.has_power
                    && find_x >= ox - (other.size - 1) / 2
                    && find_y >= oy - (other.size - 1) / 2
                    && find_x <= ox + other.size / 2
                    && find_y <= oy + other.size / 2
            });
        if let Some((index, _)) = found {
            out.push(index);
        }
    }
    out
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
    /// Placed tiles (`SchematicImage` preview inputs; `Schematic.tiles`).
    pub tiles: Vec<SchematicTileView>,
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
    /// `Schematic.powerConsumption()` (per tick).
    pub power_consumption: f32,
    /// `Schematic.powerProduction()` (per tick).
    pub power_production: f32,
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
    /// On-disk file name when loaded from disk
    /// (`Universe.updateLoadout`'s `schem.file`; `None` for vanilla loadouts).
    pub file: Option<String>,
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
    /// `Category.ordinal()` — the category hotkey digit derives from it
    /// (`blockSelect[ordinal]` in `PlacementFragment`). Appended field; the
    /// JSON contract stays append-only.
    pub ordinal: usize,
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
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

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
    block_catalog_from(&registry, store, planet)
}

/// Catalog over a caller-supplied registry: the live session registry in the
/// client, so mod content is in the palette exactly like Java's
/// `PlacementFragment.getUnlockedByCategory` reads the global
/// `Vars.content.blocks()` (`PlacementFragment.java:771-781`). `store` enables
/// the unlock/visibility filter and `planet` overrides the default Serpulo
/// `environmentBuildable` context.
pub fn block_catalog_from(
    registry: &ContentRegistry,
    store: Option<&dyn UnlockStore>,
    planet: Option<&str>,
) -> BlockCatalogView {
    use crate::content::registries::blocks::BuildVisibility;
    use crate::content::{Category, PlanetId};

    // `State.getPlanet()` falls back to the rules planet (`serpulo` by default).
    let planet: Option<PlanetId> = planet
        .and_then(|name| registry.planet_id(name))
        .or_else(|| registry.planet_id("serpulo"));
    let mut categories: Vec<BlockCategoryView> = Category::ALL
        .iter()
        .enumerate()
        .map(|(ordinal, category)| BlockCategoryView {
            name: category.name(),
            ordinal,
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
                && !tech_node_auto_unlocks(registry, node_ref)
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
    let (visible, accessible, has_grid, sector_count, start_sector, color, tech_tree, flags) = def
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
                (
                    def.allow_sector_invasion,
                    def.show_rts_ai_rule,
                    def.clear_sector_on_lose,
                ),
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
            (false, false, false),
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
        allow_sector_invasion: flags.0,
        show_rts_ai_rule: flags.1,
        clear_sector_on_lose: flags.2,
    }
}

/// `Sector.name()`: a player-set `info.name` wins, then the preset name
/// (localized by the dialog), then the planet name for single-sector planets,
/// then the bare grid index (`id + ""`).
fn sector_display_name(planet: &Planet, sector: &Sector) -> String {
    sector
        .info
        .info
        .name
        .clone()
        .filter(|name| !name.is_empty())
        .or_else(|| sector.preset_name.clone())
        .unwrap_or_else(|| {
            if planet.sectors.len() == 1 {
                planet.name.clone()
            } else {
                sector.id.to_string()
            }
        })
}

/// Builds one sector marker.
fn sector_view(planet: &Planet, sector: &Sector, registry: &ContentRegistry) -> SectorView {
    let named = sector
        .info
        .info
        .name
        .as_ref()
        .is_some_and(|name| !name.is_empty());
    let name = sector_display_name(planet, sector);
    SectorView {
        named,
        selectable: false,
        require_unlock: sector.preset_require_unlock,
        preset_unlocked: false,
        lock_icon: false,
        icon: sector.info.info.icon.clone(),
        content_icon: sector.info.info.content_icon.clone(),
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
        stats: sector_stats_view(sector, planet, registry),
        launch_from: None,
        no_candidate: false,
        show_action: false,
        objectives_locked: false,
        objectives: Vec::new(),
        vulnerable: false,
        threat_color: threat_color(sector.threat),
    }
}

/// `Sector.displayThreat` colour (Sector.java:80-85): white lerps to scarlet by
/// `Mathf.round(threat, 0.25)` (a truncating round), emitted like Arc's
/// `Color.toString` (8 hex digits of `(int)(255*c)`).
pub fn threat_color(threat: f32) -> String {
    const STEP: f32 = 0.25;
    // Arc `Color.scarlet` (0xff341cff).
    const SCARLET_G: f32 = 52.0 / 255.0;
    const SCARLET_B: f32 = 28.0 / 255.0;
    // `Mathf.round(v, step) = (int)(v / step) * step` (truncation).
    let t = ((threat / STEP) as i32) as f32 * STEP;
    let lerp = |scarlet: f32| 1.0 + (scarlet - 1.0) * t;
    let byte = |channel: f32| (channel * 255.0) as u32;
    format!(
        "{:02x}{:02x}{:02x}ff",
        byte(lerp(1.0)),
        byte(lerp(SCARLET_G)),
        byte(lerp(SCARLET_B)),
    )
}

/// `PlanetDialog.showStats` rows from the stored sector record: rate rows pass
/// the `(int)(mean * 60) > 1` filter (per-minute totals), stored items only
/// positive amounts. Rows are item-id ordered (`content.items()` order in
/// Java; `PlanetDialog.java:1040-1046`). `destination`/`import_sources` come
/// from `SectorInfo.destination` + `SectorInfo.eachImport`; `playtime_ms` is
/// left at 0 for the host to fill from the save meta.
fn sector_stats_view(
    sector: &Sector,
    planet: &Planet,
    registry: &ContentRegistry,
) -> SectorStatsView {
    let info = &sector.info.info;
    let item_order = |name: &str| -> u16 {
        registry
            .item_id(name)
            .map(|id| id.raw())
            .unwrap_or(u16::MAX)
    };
    let rates = |map: &indexmap::IndexMap<String, ExportStat>| -> Vec<(String, i32)> {
        let mut rows: Vec<(String, i32)> = map
            .iter()
            .filter_map(|(name, stat)| {
                let per_minute = (stat.mean * 60.0) as i32;
                (per_minute > 1).then(|| (name.clone(), per_minute))
            })
            .collect();
        rows.sort_by_key(|(name, _)| item_order(name));
        rows
    };
    let this_key = crate::io::json::content_serde::SectorKey::format(&planet.name, sector.id);
    let stat_ref = |other: &Sector| -> SectorStatRef {
        SectorStatRef {
            planet: planet.name.clone(),
            sector: other.id,
            name: sector_display_name(planet, other),
            icon: other.info.info.icon.clone(),
            content_icon: other.info.info.content_icon.clone(),
        }
    };
    let destination = info.destination.as_ref().and_then(|key| {
        let target = planet.sectors.iter().find(|other| {
            crate::io::json::content_serde::SectorKey::format(&planet.name, other.id) == *key
        });
        target.filter(|other| other.has_base()).map(stat_ref)
    });
    let import_sources: Vec<SectorStatRef> = planet
        .sectors
        .iter()
        .filter(|other| {
            other.id != sector.id
                && other.has_base()
                && other.info.info.destination.as_ref() == Some(&this_key)
                && other.info.any_exports()
        })
        .map(stat_ref)
        .collect();
    let preset_description_key = sector
        .preset
        .and_then(|id| registry.sector(id))
        .and_then(|def| (!def.is_hidden()).then(|| format!("sector.{}.description", def.name)));
    let mut stored: Vec<(String, i32)> = info
        .items
        .iter()
        .filter(|(_, amount)| **amount > 0)
        .map(|(name, amount)| (name.clone(), *amount))
        .collect();
    stored.sort_by_key(|(name, _)| item_order(name));
    SectorStatsView {
        attempts: info.attempts,
        wave: if info.waves && sector.has_base() {
            info.wave
        } else {
            0
        },
        production: rates(&info.production),
        export: rates(&info.export),
        import: rates(&info.imports),
        stored,
        resources: {
            let mut resources: Vec<String> = Vec::new();
            for name in &info.resources {
                if !resources.contains(name) {
                    resources.push(name.clone());
                }
            }
            resources
        },
        playtime_ms: 0,
        destination,
        import_sources,
        lockdown: sector.is_attacked(None) && !sector.is_being_played(),
        preset_description_key,
    }
}

/// `SectorPreset.isLastSector` via the content preset table.
fn sector_is_last(planet_name: &str, sector: &Sector) -> bool {
    // The preset flag is copied onto the content def; the runtime sector only
    // records the preset name, so prefer the per-sector content record below.
    let _ = planet_name;
    sector.preset.map(|_| false).unwrap_or(false)
}

/// `PlanetDialog.renderSectors/renderProjections` inputs that need the
/// registry plus the look-mode `updateSelected` rules: `canSelect`, preset
/// unlock, padlock, locked objectives, the vulnerable flag, the threat colour,
/// `findLauncher` and the action-button gates.
///
/// Neighbours come from `planet.neighbors` (`Ptile.tiles` order, same as Java
/// `Sector.near()`); the `PlanetGrid` is kept only for `tile.v` distances.
/// Deterministic: sectors in id order, neighbours in grid order.
#[allow(clippy::too_many_arguments)]
pub fn annotate_globe(
    mut views: Vec<SectorView>,
    registry: &ContentRegistry,
    planet: &Planet,
    ctx: &dyn ObjectiveContext,
    launch_sector: Option<u16>,
    played_core_size: i32,
    loadout_of: &dyn Fn(u16) -> Vec<(String, i32)>,
) -> Vec<SectorView> {
    use crate::content::registries::planets::GeneratorKind;
    use crate::content::{ContentRef, ContentType, SectorId};

    let Some(def) = registry.planet(planet.id) else {
        return views;
    };
    let tech = registry.tech();
    let node_of =
        |content: ContentRef| tech.nodes.iter().find(|node| node.content == Some(content));
    let grid = (def.sector_tiles > 0)
        .then(|| crate::render::g3d::grid::PlanetGrid::create(def.sector_tiles as usize));
    let tile_v = |id: u16| -> Option<[f32; 3]> {
        grid.as_ref()
            .and_then(|grid| grid.tiles.get(id as usize))
            .map(|tile| tile.v)
    };
    let near = |id: u16| -> Vec<u16> {
        planet
            .neighbors
            .get(id as usize)
            .cloned()
            .unwrap_or_default()
    };
    let is_based = |id: u16| {
        planet
            .sectors
            .get(id as usize)
            .is_some_and(Sector::has_base)
    };
    let is_calm = |id: u16| {
        planet
            .sectors
            .get(id as usize)
            .is_some_and(|s| !s.is_attacked(None))
    };
    let has_items = |id: u16, stacks: &[(String, i32)]| -> bool {
        planet.sectors.get(id as usize).is_some_and(|sector| {
            stacks.iter().all(|(name, amount)| {
                sector.info.info.items.get(name).copied().unwrap_or(0) >= *amount
            })
        })
    };
    let core_size = |name: &str| {
        registry
            .block_by_name(name)
            .map(|block| block.size)
            .unwrap_or(0)
    };
    // `SerpuloPlanetGenerator.allowNumberedLaunch` (Serpulo only): based,
    // unattacked and a size >= 4 core; the being-played clause uses
    // `played_core_size` (`0` when the caller cannot supply it).
    let allow_numbered_launch = |id: u16| -> bool {
        let Some(sector) = planet.sectors.get(id as usize) else {
            return false;
        };
        if !sector.has_base() || sector.is_attacked(None) {
            return false;
        }
        core_size(&sector.info.info.best_core_type) >= 4
            || (sector.is_being_played() && played_core_size >= 4)
    };
    let content_name =
        |content: ContentRef| tech_tree::content_name(registry, content).unwrap_or_default();
    let sector_name = |id: SectorId| {
        registry
            .sector(id)
            .map(|preset| preset.name.clone())
            .unwrap_or_default()
    };
    let planet_name = |id: PlanetId| {
        registry
            .planet(id)
            .map(|planet| planet.name.clone())
            .unwrap_or_default()
    };
    // `Seq.min(pred, f)`: first minimum in id order (strict `<`).
    let min_sector = |dest: u16, pred: &dyn Fn(u16) -> bool, squared: bool| -> Option<u16> {
        let dest_v = tile_v(dest)?;
        let mut best: Option<u16> = None;
        let mut best_distance = f32::MAX;
        for id in 0..planet.sectors.len() as u16 {
            if !pred(id) {
                continue;
            }
            let Some(v) = tile_v(id) else {
                continue;
            };
            let dx = v[0] - dest_v[0];
            let dy = v[1] - dest_v[1];
            let dz = v[2] - dest_v[2];
            let distance = dx * dx + dy * dy + dz * dz;
            let distance = if squared { distance } else { distance.sqrt() };
            if distance < best_distance {
                best_distance = distance;
                best = Some(id);
            }
        }
        best
    };
    // `PlanetGenerator.findLaunchCandidate` (base generator,
    // PlanetGenerator.java:59-97).
    let base_candidate = |dest: u16, sel: Option<u16>| -> Option<u16> {
        let dest_sector = planet.sectors.get(dest as usize)?;
        let dest_preset = dest_sector.preset.and_then(|id| registry.sector(id));
        let allow_loadout = dest_preset
            .map(|preset| {
                if preset.override_launch_defaults {
                    preset.allow_launch_loadout
                } else {
                    def.allow_launch_loadout
                }
            })
            .unwrap_or(def.allow_launch_loadout);
        if !allow_loadout && dest_preset.is_some() {
            let tmp = loadout_of(dest);
            if let Some(selected) =
                sel.filter(|id| is_based(*id) && is_calm(*id) && has_items(*id, &tmp))
            {
                return Some(selected);
            }
            return min_sector(
                dest,
                &|id| is_based(id) && is_calm(id) && has_items(id, &tmp),
                false,
            );
        }
        let launch_sel = sel.filter(|id| is_based(*id) && is_calm(*id));
        let near_dest = near(dest);
        if launch_sel.is_some_and(|selected| near_dest.contains(&selected)) {
            return launch_sel;
        }
        let mut launch_from = launch_sel;
        if launch_from.is_none() || dest_preset.is_none() {
            launch_from = near_dest
                .iter()
                .copied()
                .find(|id| is_based(*id) && is_calm(*id));
            if launch_from.is_none() && dest_preset.is_some() {
                if launch_sel.is_some() {
                    return launch_sel;
                }
                launch_from = min_sector(dest, &|id| is_based(id) && is_calm(id), true);
            }
        }
        launch_from
    };
    // `PlanetDialog.findLauncher` (PlanetDialog.java:437-444): the generator's
    // candidate, else the actual launch sector when it is unattacked.
    let actual = launch_sector.filter(|id| (*id as usize) < planet.sectors.len());
    let find_launcher = |dest: u16| -> Option<u16> {
        let dest_sector = planet.sectors.get(dest as usize)?;
        let dest_require_unlock = dest_sector
            .preset
            .and_then(|id| registry.sector(id))
            .is_some_and(|preset| preset.require_unlock);
        let candidate = if def.generator == GeneratorKind::Serpulo
            && (dest_sector.preset.is_none() || !dest_require_unlock)
        {
            // `SerpuloPlanetGenerator.findLaunchCandidate` (:90-112).
            let near_dest = near(dest);
            actual
                .filter(|id| near_dest.contains(id) && allow_numbered_launch(*id))
                .or_else(|| near_dest.into_iter().find(|id| allow_numbered_launch(*id)))
        } else {
            base_candidate(dest, actual)
        };
        match candidate {
            None => actual.filter(|id| is_calm(*id)),
            Some(found) => Some(found),
        }
    };

    for view in &mut views {
        let Some(sector) = planet.sectors.get(view.id as usize) else {
            continue;
        };
        let preset = sector.preset.and_then(|id| registry.sector(id));
        let node =
            preset.and_then(|preset| node_of(ContentRef::of(ContentType::Sector, preset.id)));
        let preset_unlocked = preset.is_some_and(|preset| preset.unlock.unlocked());
        view.preset_unlocked = preset_unlocked;
        let parent_unlocked = node.and_then(|node| node.parent).map(|parent| {
            tech.nodes
                .get(parent.0 as usize)
                .and_then(|parent| parent.content)
                .is_none_or(|content| tech_tree::content_unlocked(registry, content))
        });
        view.lock_icon = preset.is_some_and(|preset| preset.require_unlock)
            && !preset_unlocked
            && node.is_some()
            && parent_unlocked.unwrap_or(true);
        view.selectable = if def.generator == GeneratorKind::None || sector.is_shielded() {
            false
        } else if sector.has_base() || sector.id == def.start_sector as u16 {
            true
        } else if preset.is_some_and(|preset| preset.require_unlock) {
            preset_unlocked
                || node.is_none()
                || match node.and_then(|node| node.parent) {
                    None => true,
                    Some(parent) => tech.nodes.get(parent.0 as usize).is_some_and(|parent| {
                        let unlocked = parent
                            .content
                            .is_none_or(|content| tech_tree::content_unlocked(registry, content));
                        let preset_parent_based = match parent.content {
                            Some(content) if content.type_ == ContentType::Sector => planet
                                .sectors
                                .iter()
                                .find(|other| other.preset.map(|id| id.raw()) == Some(content.id))
                                .is_some_and(Sector::has_base),
                            _ => true,
                        };
                        unlocked && preset_parent_based
                    }),
                }
        } else {
            def.allow_launch_to_numbered
                && near(sector.id)
                    .iter()
                    .any(|&other| allow_numbered_launch(other))
        };
        // `updateSelected`: `isLocked(sector)` = preset locked, no base, node.
        view.objectives_locked =
            preset.is_some() && !preset_unlocked && !sector.has_base() && node.is_some();
        view.objectives.clear();
        if view.objectives_locked
            && let Some(node) = node
        {
            for spec in &node.objectives {
                if Objective::from_spec(*spec).complete(ctx) {
                    continue;
                }
                let (kind, argument) = match *spec {
                    ObjectiveSpec::SectorComplete(id) => ("capture", sector_name(id)),
                    ObjectiveSpec::OnSector(id) => ("onsector", sector_name(id)),
                    ObjectiveSpec::OnPlanet(id) => ("onplanet", planet_name(id)),
                    ObjectiveSpec::Research(content) => ("research", content_name(content)),
                    ObjectiveSpec::Produce(content) => ("produce", content_name(content)),
                };
                view.objectives.push((kind.to_owned(), argument));
            }
        }
        // `sectors.vulnerable`: sectorInvasion + a neighbour with an enemy
        // base whose preset is unlocked (or unassigned).
        view.vulnerable = sector.has_base()
            && planet.campaign_rules.sector_invasion
            && near(sector.id).iter().any(|&other| {
                planet.sectors.get(other as usize).is_some_and(|other| {
                    other.has_enemy_base()
                        && other
                            .preset
                            .and_then(|id| registry.sector(id))
                            .is_none_or(|preset| !preset.require_unlock)
                })
            });
        view.threat_color = threat_color(sector.threat);
        // `updateSelected` (PlanetDialog.java:1286-1354).
        view.launch_from = find_launcher(sector.id);
        view.no_candidate = sector.id != def.start_sector as u16
            && !sector.has_base()
            && view.launch_from.is_none();
        view.show_action = sector.has_base()
            || view.selectable
            || preset.is_some_and(|preset| preset.unlock.always_unlocked);
    }
    views
}

/// Builds the sector markers for a planet.
pub fn sector_views(
    planet: &Planet,
    presets: &[SectorPresetDef],
    registry: &ContentRegistry,
) -> Vec<SectorView> {
    planet
        .sectors
        .iter()
        .map(|sector| {
            let mut view = sector_view(planet, sector, registry);
            if let Some(preset) = sector
                .preset
                .and_then(|id| presets.get(id.raw() as usize))
                .filter(|preset| preset.planet == planet.id)
            {
                view.is_last = preset.is_last_sector;
                view.capture_wave = preset.capture_wave;
                view.difficulty = preset.difficulty;
                // `PlanetDialog.isLocked`: only a locked, tech-gated preset on a
                // base-less cell blocks selection; `requireUnlock = false`
                // presets (map submissions) stay selectable even before their
                // unlock flag is set.
                view.locked =
                    preset.require_unlock && !preset.unlock.unlocked() && !sector.has_base();
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
                root: node
                    .tree
                    .and_then(|tree| tech.tree(tree))
                    .map(|tree| tree.name.clone()),
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
///
/// The vanilla launch loadouts (`Schematics.load_loadouts`, `file == None`) are
/// hidden: Java's `Schematics.all()` yields them but `SchematicsDialog` never
/// shows them (they are launch-loadout scaffolding, addressed through
/// `loadout_views`). `SchematicView.index` keeps the real `Schematics.all`
/// position so every action (`use_schematic`, export, delete) addresses the
/// right entry.
pub fn schematic_views(schematics: &Schematics, registry: &ContentRegistry) -> Vec<SchematicView> {
    schematics
        .all
        .iter()
        .enumerate()
        .filter(|(_, schematic)| schematic.file.is_some())
        .map(|(index, schematic)| SchematicView {
            index,
            name: schematic.name(),
            description: schematic.description(),
            width: schematic.width,
            height: schematic.height,
            tiles: schematic_tile_views(schematic, registry),
            has_core: schematic.has_core(registry),
            labels: schematic.labels.clone(),
            file: schematic.file.clone(),
            mod_name: schematic.mod_name.clone(),
            requirements: requirement_pairs(registry, &schematic.requirements(registry)),
            power_consumption: schematic.power_consumption(registry),
            power_production: schematic.power_production(registry),
        })
        .collect()
}

/// The preview tiles of one schematic (`Schematics.getBuffer` plan list):
/// tiles whose block is unknown are dropped, and power nodes carry their
/// resolved laser targets.
fn schematic_tile_views(
    schematic: &crate::game::schematic::Schematic,
    registry: &ContentRegistry,
) -> Vec<SchematicTileView> {
    let known: Vec<(usize, &crate::content::registries::blocks::BlockDef)> = schematic
        .tiles
        .iter()
        .enumerate()
        .filter_map(|(index, tile)| registry.block(tile.block).map(|def| (index, def)))
        .collect();
    known
        .iter()
        .enumerate()
        .map(|(own, &(stile_index, def))| {
            let tile = &schematic.tiles[stile_index];
            SchematicTileView {
                block: def.name.clone(),
                x: tile.x as i32,
                y: tile.y as i32,
                rotation: tile.rotation.max(0) as u8,
                size: def.size,
                rotate: crate::world::block::kind_rotates(def.kind),
                lasers: schematic_lasers(schematic, &known, own),
            }
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
                file: schematic.file.clone(),
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
    ///
    /// `played` is the sector currently being played (filtered by `game_over`
    /// by the caller; the `findLauncher` "actual launch sector"), `played_core_size`
    /// its live core block size (`0` when unknown — the being-played
    /// `allowNumberedLaunch` clause) and `loadout_of` the preset map loadout for
    /// a destination (items already filtered to the planet; empty for the
    /// headless fixture).
    pub fn from_campaign(
        campaign: &Campaign,
        registry: &ContentRegistry,
        schematics: &Schematics,
        active: PlanetId,
        played: Option<(PlanetId, u16)>,
        played_core_size: i32,
        loadout_of: &dyn Fn(u16) -> Vec<(String, i32)>,
    ) -> Self {
        let planet = campaign
            .planet(active)
            .unwrap_or_else(|| &campaign.planets[0]);
        let presets = registry.sectors();
        let core_name = registry
            .planet(active)
            .and_then(|def| def.default_core.clone())
            .unwrap_or_else(|| "core-shard".to_owned());
        let ctx = CampaignObjectiveContext::new(registry, campaign);
        let launch_sector = played
            .filter(|(planet, _)| *planet == active)
            .map(|(_, sector)| sector);
        Self {
            planet: planet.name.clone(),
            planets: campaign
                .order
                .iter()
                .filter_map(|id| campaign.planets.get(id))
                .map(|planet| planet_view(registry, planet))
                .collect(),
            sectors: annotate_globe(
                sector_views(planet, presets, registry),
                registry,
                planet,
                &ctx,
                launch_sector,
                played_core_size,
                loadout_of,
            ),
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
        // `schematic_views` hides the vanilla loadouts (`file == None`); keep
        // one file-backed copy so the M5 fixtures still carry a dialog row
        // with a core and requirements.
        if let Some(first) = schematics.all.first_mut() {
            first.file = Some("fixture-loadout.msch".to_owned());
        }
        Self::from_campaign(&campaign, &registry, &schematics, active, None, 0, &|_| {
            Vec::new()
        })
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
        assert_eq!(
            views.schematics.len(),
            1,
            "only the file-backed fixture schematic is dialog-visible \
             (vanilla loadouts hidden)"
        );
        assert_eq!(
            views.schematics[0].index, 0,
            "the view keeps the real Schematics.all index"
        );
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
    fn threat_color_matches_java() {
        // `Sector.displayThreat`: truncating `Mathf.round(threat, 0.25)`.
        assert_eq!(threat_color(0.3), "ffccc6ff");
        assert_eq!(threat_color(0.5), "ff998dff");
        assert_eq!(threat_color(0.74), "ff998dff");
        assert_eq!(threat_color(0.0), "ffffffff");
    }

    /// T10: `PlanetDialog.showStats` rows follow `content.items()` order (item
    /// id), not the insertion order of the persisted maps, for both the stored
    /// pool and the per-minute rate lists (`PlanetDialog.java:1040-1046`).
    #[test]
    fn sector_stats_rows_in_item_order() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;
        use crate::io::json::rules::ExportStat;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("base content");
        let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let serpulo = campaign.planet_id_by_name("serpulo").expect("serpulo");
        let ground_zero = registry
            .sector_by_name("groundZero")
            .expect("groundZero")
            .sector;
        {
            let planet = campaign.planet_mut(serpulo).expect("serpulo");
            let sector = planet
                .sectors
                .get_mut(ground_zero as usize)
                .expect("groundZero sector");
            sector.save = Some(format!("sector-serpulo-{ground_zero}"));
            sector.info.info.has_core = true;
            sector.info.info.waves = false;
            // Inserted in non-item-id order (graphite 3, lead 1, copper 0).
            sector.info.info.items.insert("graphite".to_owned(), 21);
            sector.info.info.items.insert("lead".to_owned(), 12);
            sector.info.info.items.insert("copper".to_owned(), 33);
            for (name, mean) in [("graphite", 0.5f32), ("copper", 2.0)] {
                sector.info.info.production.insert(
                    name.to_owned(),
                    ExportStat {
                        mean,
                        ..Default::default()
                    },
                );
            }
        }
        let planet = campaign.planet(serpulo).expect("serpulo");
        let sector = planet.sectors.get(ground_zero as usize).expect("sector");
        let view = sector_stats_view(sector, planet, &registry);
        assert_eq!(
            view.stored,
            vec![
                ("copper".to_owned(), 33),
                ("lead".to_owned(), 12),
                ("graphite".to_owned(), 21)
            ],
            "stored pool in item-id order"
        );
        assert_eq!(
            view.production,
            vec![("copper".to_owned(), 120), ("graphite".to_owned(), 30)],
            "rate rows in item-id order (`content.items()`)"
        );
    }

    /// Serpulo's numbered launch needs a based, calm neighbour with a size >= 4
    /// core (`SerpuloPlanetGenerator.allowNumberedLaunch`).
    #[test]
    fn serpulo_numbered_launch_needs_size4_neighbour() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("base content");
        let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let serpulo = campaign.planet_id_by_name("serpulo").expect("serpulo");
        // A non-preset destination next to a based sector.
        let (based, dest) = {
            let planet = campaign.planet(serpulo).expect("serpulo");
            (0..planet.sectors.len() as u16)
                .find_map(|id| {
                    let sector = planet.sectors.get(id as usize)?;
                    if sector.preset.is_some() {
                        return None;
                    }
                    let neighbour = *planet.neighbors.get(id as usize)?.first()?;
                    Some((neighbour, id))
                })
                .expect("a non-preset sector with a neighbour")
        };
        {
            let planet = campaign.planet_mut(serpulo).expect("serpulo");
            let sector = planet
                .sectors
                .get_mut(based as usize)
                .expect("based sector");
            sector.save = Some(format!("sector-serpulo-{based}"));
            sector.info.info.has_core = true;
            // A calm base (`waves`/`attack` off; `SectorInfo.waves` defaults on).
            sector.info.info.waves = false;
            sector.info.info.attack = false;
            sector.info.info.best_core_type = "core-shard".to_owned();
        }
        let ctx = CampaignObjectiveContext::new(&registry, &campaign);
        let planet = campaign.planet(serpulo).expect("serpulo");
        let views = annotate_globe(
            sector_views(planet, registry.sectors(), &registry),
            &registry,
            planet,
            &ctx,
            None,
            0,
            &|_| Vec::new(),
        );
        let dest_view = views
            .iter()
            .find(|view| view.id == dest)
            .expect("dest view");
        assert_eq!(
            dest_view.launch_from, None,
            "core-shard (size 3) is no launcher"
        );
        assert!(dest_view.no_candidate);

        // Upgrade the neighbour to a size 4 core (`core-foundation`).
        campaign.planet_mut(serpulo).expect("serpulo").sectors[based as usize]
            .info
            .info
            .best_core_type = "core-foundation".to_owned();
        let ctx = CampaignObjectiveContext::new(&registry, &campaign);
        let planet = campaign.planet(serpulo).expect("serpulo");
        let views = annotate_globe(
            sector_views(planet, registry.sectors(), &registry),
            &registry,
            planet,
            &ctx,
            None,
            0,
            &|_| Vec::new(),
        );
        let dest_view = views
            .iter()
            .find(|view| view.id == dest)
            .expect("dest view");
        assert_eq!(dest_view.launch_from, Some(based));
    }

    /// A require-unlock preset prefers the played (actual) launch sector even
    /// when it is not adjacent (`PlanetGenerator.findLaunchCandidate`).
    #[test]
    fn preset_launch_prefers_played_sector() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::planet::EmptyNeighborhood;

        let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("base content");
        let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let serpulo = campaign.planet_id_by_name("serpulo").expect("serpulo");
        let dest = {
            let planet = campaign.planet(serpulo).expect("serpulo");
            (0..planet.sectors.len() as u16)
                .find(|id| {
                    planet
                        .sectors
                        .get(*id as usize)
                        .and_then(|sector| sector.preset)
                        .and_then(|preset| registry.sector(preset))
                        .is_some_and(|preset| preset.require_unlock)
                })
                .expect("a require-unlock preset sector")
        };
        let planet = campaign.planet(serpulo).expect("serpulo");
        let near_dest = planet
            .neighbors
            .get(dest as usize)
            .cloned()
            .unwrap_or_default();
        let actual = (0..planet.sectors.len() as u16)
            .find(|id| *id != dest && !near_dest.contains(id))
            .expect("a non-adjacent sector");
        {
            let planet = campaign.planet_mut(serpulo).expect("serpulo");
            let sector = planet
                .sectors
                .get_mut(actual as usize)
                .expect("actual sector");
            sector.save = Some(format!("sector-serpulo-{actual}"));
            sector.info.info.has_core = true;
            // A calm base (`waves`/`attack` off; `SectorInfo.waves` defaults on).
            sector.info.info.waves = false;
            sector.info.info.attack = false;
            sector.info.info.best_core_type = "core-foundation".to_owned();
        }
        let ctx = CampaignObjectiveContext::new(&registry, &campaign);
        let planet = campaign.planet(serpulo).expect("serpulo");
        let views = annotate_globe(
            sector_views(planet, registry.sectors(), &registry),
            &registry,
            planet,
            &ctx,
            Some(actual),
            0,
            &|_| Vec::new(),
        );
        let dest_view = views
            .iter()
            .find(|view| view.id == dest)
            .expect("dest view");
        assert_eq!(dest_view.launch_from, Some(actual));
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

    /// Every row carries its tree root name (`ResearchDialog` groups the node
    /// list by it); only the root node is named in `TechTree` itself.
    #[test]
    fn research_rows_carry_tree_root() {
        let views = CampaignViews::vanilla_fixture();
        assert!(
            views
                .research
                .iter()
                .any(|node| node.depth > 0 && node.root.as_deref() == Some("serpulo")),
            "non-root nodes carry their tree name"
        );
        assert!(
            views
                .research
                .iter()
                .filter(|node| node.planet.as_deref() == Some("serpulo"))
                .all(|node| node.root.as_deref() == Some("serpulo")),
            "every serpulo row groups under the serpulo root"
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
    fn schematic_preview_resolves_power_lasers() {
        use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
        use crate::game::schematic::{Schematic, Stile};
        use crate::io::typeio::pack_point2;
        use crate::world::config::ConfigValue;
        let registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap();
        let block = |name: &str| registry.block_id(name).unwrap();
        // Node at (0,0) linked to a battery at (3,0) and a 2x2 battery whose
        // footprint covers (0,3) from its origin (0,2); the conveyor link at
        // (5,0) has no power and draws nothing (`planFinder.hasPower`).
        let links = ConfigValue::Point2Array(
            [pack_point2(3, 0), pack_point2(0, 3), pack_point2(5, 0)]
                .into_iter()
                .collect(),
        );
        let schematic = Schematic::from_tiles(
            vec![
                Stile::new(block("power-node"), 0, 0, links, 0),
                Stile::new(block("battery"), 3, 0, ConfigValue::None, 0),
                Stile::new(block("battery-large"), 0, 2, ConfigValue::None, 0),
                Stile::new(block("conveyor"), 5, 0, ConfigValue::None, 0),
            ],
            Default::default(),
            6,
            5,
        );
        let tiles = schematic_tile_views(&schematic, &registry);
        assert_eq!(tiles[0].lasers, vec![1, 2]);
        assert!(tiles[1].lasers.is_empty());
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

    /// I07 T6 (PAR-0115): the client builds the palette over the live session
    /// registry (`Vars.content.blocks()`, `PlacementFragment.java:771-781`), so
    /// an enabled mod's block reaches the sandbox placement palette.
    #[test]
    fn block_catalog_from_lists_live_mod_content() {
        use crate::content::boot::{build_registry_with_mods, collect_boot_content};
        use crate::content::{MemoryBundle, MemoryUnlockStore};
        use crate::io::SettingsStore;
        use crate::io::fs::NativeFs;
        use crate::mods::Mods;
        use std::path::Path;

        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../parity/mod_fixtures/allkinds");
        let fs = NativeFs;
        let mut mods = Mods::new(true, &dir);
        mods.load_single(&fs, &dir, &SettingsStore::new())
            .expect("load allkinds");
        let boot = collect_boot_content(&mut mods, &fs);
        let (registry, report) =
            build_registry_with_mods(&MemoryBundle::new(), &MemoryUnlockStore::new(), true, &boot)
                .expect("boot with mods");
        assert!(report.failures.is_empty(), "{:?}", report.failures);

        let catalog = block_catalog_from(&registry, None, Some("serpulo"));
        let mod_blocks = catalog
            .categories
            .iter()
            .flat_map(|category| category.blocks.iter())
            .filter(|block| block.name.starts_with("all-kinds-"))
            .map(|block| block.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            mod_blocks,
            vec!["all-kinds-all-wall"],
            "the modded block is in the live-registry palette"
        );

        // The base-content builder stays mod-free (the no-session fallback).
        let base = block_catalog_for("serpulo");
        assert!(
            !base.categories.iter().any(|category| category
                .blocks
                .iter()
                .any(|block| block.name == "all-kinds-all-wall")),
            "base content has no mod blocks"
        );
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
            None,
            0,
            &|_| Vec::new(),
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
            None,
            0,
            &|_| Vec::new(),
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
