// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Static placement/break validity (`core/src/mindustry/world/Build.java`).
//!
//! Ported from `Build.validPlace*`, `validBreak`, `contactsGround`,
//! `contactsShallows`, `getEnemyOverlap` and `Block.canReplace`/`canPlaceOn`.
//! Bans/limits/darkness/core-radius use the plan-07 [`BuildRules`]/
//! [`BlockCounter`] providers (plan 12 overrides).

use crate::content::{BlockDef, BlockId, BlockKind, ContentRegistry, EnvFlag};
use crate::ecs::BuildingComp;
use crate::entities::comp::TeamComp;
use crate::world::darkness::get_wall_darkness;

use super::WorldGrid;
use super::block::{BlockTable, BlockView};
use super::limits::{BlockCounter, BuildRules};

/// Block-relevant environment being placed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlacementEnv {
    /// Whether the target tile is covered by liquid (`placeableLiquid` gate).
    pub on_liquid: bool,
    /// Whether the target floor is deep liquid (`isDeep`).
    pub deep_liquid: bool,
    /// Whether the tile is water for `requiresWater` (plan 09).
    pub on_water: bool,
}

/// `Block.supportsEnv(env)` for one flag.
pub fn supports_env(def: &BlockDef, env: EnvFlag) -> bool {
    if !def.env_disabled.is_empty() && def.env_disabled.contains(env) {
        return false;
    }
    if !def.env_required.is_empty() && !def.env_required.contains(env) {
        return false;
    }
    def.env_enabled.contains(env) || def.env_enabled.is_empty()
}

fn mask_bits(mask: &crate::content::EnvMask, empty: i32) -> i32 {
    if mask.any {
        return -1;
    }
    if mask.flags.is_empty() {
        return empty;
    }
    mask.flags
        .iter()
        .fold(0, |bits, flag| bits | crate::game::planet::env_bit(*flag))
}

/// `Block.supportsEnv(env)` against a Java `Env` bitmask (`Rules.env`):
/// `(envEnabled & env) != 0 && (envDisabled & env) == 0 && (envRequired == 0
/// || (envRequired & env) == envRequired)`. An unset `envEnabled` is the
/// upstream default `Env.terrestrial`.
pub fn supports_env_bits(def: &BlockDef, env: i32) -> bool {
    let enabled = mask_bits(
        &def.env_enabled,
        crate::game::planet::env_bit(EnvFlag::Terrestrial),
    );
    let disabled = mask_bits(&def.env_disabled, 0);
    let required = mask_bits(&def.env_required, 0);
    (enabled & env) != 0 && (disabled & env) == 0 && (required == 0 || (required & env) == required)
}

/// `Block.canPlaceOn`.
pub fn can_place_on(_def: &BlockDef, _team: u8, _rot: u8) -> bool {
    true
}

/// `Block.canBreak` (`CoreBlock.java:173-177` for cores; `Block.canBreak` is
/// unconditionally true otherwise).
pub fn can_break(def: &BlockDef, rules: &BuildRules, cores_of_team: usize) -> bool {
    if def.kind == BlockKind::CoreBlock {
        //always keep at least 1 core to not lose the save
        return rules.editor || (rules.core_build_and_config && cores_of_team > 1);
    }
    true
}

/// `Block.canReplace(other)` (`Block.java:796`).
///
/// `def` is `this` (the block being placed); `other` is the existing block. The
/// `subclass` equality check uses plan 07's `BlockKind` proxy (equal `BlockKind`
/// implies the same Java base class for every ported vanilla block).
pub fn can_replace(def: &BlockDef, other: &BlockDef) -> bool {
    if other.kind == BlockKind::ConstructBlock || other.always_replace {
        return true;
    }
    // `CoreBlock.canReplace` (`CoreBlock.java:179-183`): cores upgrade smaller
    // cores.
    if def.kind == BlockKind::CoreBlock
        && other.kind == BlockKind::CoreBlock
        && def.size >= other.size
        && other.id != def.id
    {
        return true;
    }
    if other.privileged {
        return false;
    }
    let self_replace =
        other.id != def.id || (super::block::kind_rotates(def.kind) && def.quick_rotate);
    let grouped = (def.group != crate::content::BlockGroup::None && other.group == def.group)
        || other.id == def.id;
    let sized = def.size == other.size
        || (def.size >= other.size && (def.subclass == other.subclass || def.group.any_replace()));
    other.replaceable && self_replace && grouped && sized
}

/// Whether a tile holds a breakable, interactable block for `team`
/// (`Build.validBreak`, `Build.java:317-320`).
pub fn valid_break(
    content: &ContentRegistry,
    grid: &WorldGrid,
    world: Option<&bevy_ecs::world::World>,
    rules: &BuildRules,
    team: u8,
    x: i32,
    y: i32,
) -> bool {
    if !grid.tiles.in_bounds(x, y) {
        return false;
    }
    let tile = grid.tile(x, y);
    if tile.block == BlockId::AIR {
        return false;
    }
    let Some(def) = content.block(tile.block) else {
        return false;
    };
    let breakable = tile.breakable(content) || rules.allow_environment_deconstruct;
    if !breakable {
        return false;
    }
    match tile.build {
        Some(entity) => {
            // `Tile.interactable(team)` -> `state.teams.canInteract(team, team())`
            // (`Tile.java:576-578`, `Teams.java:125-127`): own team or derelict.
            // Without a world (harness/scenarios) the team cannot be read; keep
            // the pre-world behavior (any team).
            let tile_team = world
                .and_then(|world| world.get::<crate::entities::comp::TeamComp>(entity))
                .map(|comp| comp.team)
                .unwrap_or(team);
            if tile_team != team && tile_team != 0 {
                return false;
            }
            // `CoreBlock.canBreak`: always keep at least 1 core to not lose the
            // save.
            let cores_of_team = world.map_or(usize::MAX, |world| {
                world
                    .iter_entities()
                    .filter(|entity| {
                        entity.contains::<crate::world::blocks::storage::CoreBuild>()
                            && entity
                                .get::<crate::entities::comp::TeamComp>()
                                .is_some_and(|comp| comp.team == tile_team)
                    })
                    .count()
            });
            can_break(def, rules, cores_of_team)
        }
        None => true,
    }
}

/// Flags of `Build.validPlace(type, team, x, y, rot, checkVisible,
/// checkCoreRadius)` (`Build.java:163-175`); `check_units` is the port's
/// [`crate::world::build::valid_place`] overlap toggle.
#[derive(Debug, Clone, Copy)]
pub struct PlaceFlags {
    /// `checkVisible`: block-visibility/bans/env gate.
    pub check_visible: bool,
    /// `checkCoreRadius`: enemy-core no-build radius.
    pub check_core_radius: bool,
    /// Whether unit overlap is checked (`checkNoUnitOverlap`).
    pub check_units: bool,
}

impl PlaceFlags {
    /// `Build.validPlace(type, team, x, y, rot)` (player, AI, schematics).
    pub const PLAYER: Self = Self {
        check_visible: true,
        check_core_radius: true,
        check_units: true,
    };
    /// `validPlace(.., false, false)` (`Control.java:505,521`): dev/API
    /// placement.
    pub const API: Self = Self {
        check_visible: false,
        check_core_radius: false,
        check_units: false,
    };
}

/// `Build.validPlace(type, team, x, y, rot)` (5-arg form; no world).
#[allow(clippy::too_many_arguments)]
pub fn valid_place(
    content: &ContentRegistry,
    table: &BlockTable,
    rules: &BuildRules,
    counter: &BlockCounter,
    grid: &WorldGrid,
    block: BlockId,
    team: u8,
    rot: u8,
    x: i32,
    y: i32,
) -> bool {
    valid_place_at(
        content, table, rules, counter, grid, block, team, rot, x, y, true,
    )
}

/// `Build.validPlace` with explicit coordinates and the unit-overlap toggle
/// (no world; visibility/core-radius checks off, the pre-world semantics).
#[allow(clippy::too_many_arguments)]
pub fn valid_place_at(
    content: &ContentRegistry,
    table: &BlockTable,
    rules: &BuildRules,
    counter: &BlockCounter,
    grid: &WorldGrid,
    block: BlockId,
    team: u8,
    rot: u8,
    x: i32,
    y: i32,
    check_units: bool,
) -> bool {
    valid_place_query(
        content,
        table,
        rules,
        counter,
        grid,
        None,
        block,
        team,
        rot,
        x,
        y,
        PlaceFlags {
            check_visible: false,
            check_core_radius: false,
            check_units,
        },
    )
}

/// `Build.validPlaceIgnoreUnits` (`Build.java:183-273`), full port.
///
/// `world` supplies building teams/cores and enemy-overlap data; `None` keeps
/// the pure-grid behavior (harness/headless scenarios) with `check_team = team`
/// and no core/overlap reads.
#[allow(clippy::too_many_arguments)]
pub fn valid_place_query(
    content: &ContentRegistry,
    table: &BlockTable,
    rules: &BuildRules,
    counter: &BlockCounter,
    grid: &WorldGrid,
    world: Option<&bevy_ecs::world::World>,
    block: BlockId,
    team: u8,
    rot: u8,
    x: i32,
    y: i32,
    flags: PlaceFlags,
) -> bool {
    if block == BlockId::AIR || !grid.tiles.in_bounds(x, y) {
        return false;
    }
    let Some(def) = content.block(block) else {
        return false;
    };
    if !def.placeable_player && !def.in_editor {
        return false;
    }
    // (a) `Block.isVisible`/`isPlaceable` (`Block.java:1031-1037`); the env
    // check lives inside `isPlaceable`, so only the visible path checks it.
    if !rules.editor && flags.check_visible {
        let visible_base = build_visible(def, rules, content, grid);
        let visible =
            visible_base && (rules.editor || !rules.hide_banned_blocks || !rules.is_banned(block));
        let placeable = visible_base
            && (!rules.is_banned(block) || rules.editor)
            && supports_env_bits(def, rules.env);
        if !placeable && !(rules.waves && team == rules.wave_team && visible) {
            return false;
        }
    }
    if counter.is_over_limit(rules, team, block) {
        return false;
    }
    // (b) Enemy-core no-build radius (`Build.java:195-208`).
    let offset = def.offset;
    if !rules.editor
        && flags.check_core_radius
        && !(!flags.check_visible && team == 0)
        && let Some(world) = world
    {
        let wx = x as f32 * 8.0 + offset;
        let wy = y as f32 * 8.0 + offset;
        if rules.polygon_core_protection {
            // Nearest protecting core wins; tie-break by ascending entity
            // index (`EntitySeq` order). Only an ENEMY closest core blocks.
            if let Some(closest) = closest_protecting_core(world, rules, wx, wy)
                && closest != team
            {
                return false;
            }
        } else if enemy_core_within_build_radius(world, rules, team, wx, wy) {
            return false;
        }
    }
    // (c)/(d) Darkness (`Build.java:229`) + shallows (`Build.java:230`).
    let dark = (get_wall_darkness(grid, content, x, y) as f32)
        .max(crate::world::darkness::get_darkness(grid, content, x, y));
    if dark >= 3.0 {
        return false;
    }
    // `requiresWater` is not a `BlockDef` field; no Serpulo block sets it
    // (`grep "requiresWater = true" J/content/Blocks.java` -> Erekir pumps), so
    // treat it as false and require shallows contact unless the block is
    // `placeableLiquid` (`Build.java:230`; `floating` does NOT exempt this).
    if !def.placeable_liquid && !contacts_shallows(content, grid, def, x, y) {
        return false;
    }

    // (e) Footprint loop (`Build.java:233-266`).
    let size = def.size.max(1);
    let offset_t = -(size - 1) / 2;
    let blocks_rotate = table.get(block).map(|inst| inst.rotate).unwrap_or(false);
    for dx in 0..size {
        for dy in 0..size {
            let tx = x + offset_t + dx;
            let ty = y + offset_t + dy;
            if !grid.tiles.in_bounds(tx, ty) {
                return false;
            }
            let tile = grid.tile(tx, ty);
            if tile.block == BlockId::AIR {
                continue;
            }
            let Some(current) = content.block(tile.block) else {
                return false;
            };
            let Some(floor) = content.block(tile.floor) else {
                return false;
            };
            // `size == 2 && world.getDarkness(wx, wy) >= 3`.
            if size == 2 {
                let check_dark = (get_wall_darkness(grid, content, tx, ty) as f32)
                    .max(crate::world::darkness::get_darkness(grid, content, tx, ty));
                if check_dark >= 3.0 {
                    return false;
                }
            }
            // Deep floor rejects non-floating, non-liquid blocks.
            if floor.is_deep && !def.floating && !def.placeable_liquid {
                return false;
            }
            let check_team = world
                .and_then(|world| tile.build.and_then(|e| world.get::<TeamComp>(e)))
                .map(|comp| comp.team)
                .unwrap_or(if world.is_some() { 0 } else { team });
            // `!derelictRepair && check.team() == derelict && check.build != null`.
            if !rules.allow_derelict_repair && check_team == 0 && tile.build.is_some() {
                return false;
            }
            // Same block, same rotation, rotating: cannot re-place (loses
            // contents); derelict repair over a same-block is allowed.
            if tile.block == block
                && tile.build.is_some()
                && blocks_rotate
                && tile
                    .build
                    .and_then(|e| world.and_then(|world| world.get::<BuildingComp>(e)))
                    .map(|building| building.rot)
                    .unwrap_or(rot)
                    == rot
                && !(team != 0 && check_team == 0)
            {
                return false;
            }
            // `check.interactable(team)`: own team or derelict.
            if check_team != team && check_team != 0 {
                return false;
            }
            if !floor.placeable_on {
                return false;
            }
            // `!checkVisible && checkCoreRadius && !check.block().alwaysReplace`.
            if !flags.check_visible && flags.check_core_radius && !current.always_replace {
                return false;
            }
            // Replaceability + bounds containment (`Build.java:250-262`).
            let construct_site = tile.block == block
                && tile.build.is_some_and(|e| {
                    world
                        .and_then(|world| world.get::<crate::world::ConstructState>(e))
                        .is_some_and(|state| state.current == block)
                });
            let center_ok = tile.build.is_some_and(|e| {
                world
                    .and_then(|world| world.get::<BuildingComp>(e))
                    .map(|building| building.pos)
                    .map(|anchor| (i32::from(anchor.x()), i32::from(anchor.y())))
                    .map(|anchor| anchor == (x, y))
                    .unwrap_or(false)
            });
            let replaceable = can_replace(def, current)
                || (tile.block == block && team != 0 && check_team == 0)
                || (construct_site && center_ok);
            if !replaceable {
                return false;
            }
            let new_bounds = block_bounds(size, x, y);
            let old_def_center = tile
                .build
                .and_then(|e| world.and_then(|world| world.get::<BuildingComp>(e)))
                .map(|building| {
                    let anchor = building.pos;
                    (
                        i32::from(anchor.x()),
                        i32::from(anchor.y()),
                        current.size.max(1),
                    )
                })
                .unwrap_or((tx, ty, current.size.max(1)));
            let old_bounds = block_bounds(old_def_center.2, old_def_center.0, old_def_center.1);
            if !rect_contains_grown(new_bounds, old_bounds, 0.01) {
                return false;
            }
        }
    }

    // (f) Enemy overlap (`Build.java:268`, `getEnemyOverlap` `:275-277`).
    if rules.place_range_check
        && flags.check_core_radius
        && !rules.editor
        && let Some(world) = world
        && get_enemy_overlap(world, table, rules, def, team, x, y).is_some()
    {
        return false;
    }

    // Kind-specific `canPlaceOn` (pump/beam drill/wall crafter/solid pump).
    if def.kind == crate::content::BlockKind::Pump && !pump_can_place_on(content, grid, size, x, y)
    {
        return false;
    }
    // `BeamDrill.canPlaceOn` / `WallCrafter.canPlaceOn` (PAR-0079): at least one
    // facing edge must find wall ore in range / a sand-attribute wall in front.
    match kind_data_for(table, block) {
        Some(crate::world::BlockKindData::BeamDrill(beam)) => {
            let hardness = |item: crate::content::ItemId| {
                content.item(item).map(|item| item.hardness).unwrap_or(0)
            };
            let (_, amount, _) = crate::world::behavior::production::beam_facing_scan(
                table,
                grid,
                crate::world::TilePos::new(x as i16, y as i16),
                size,
                rot,
                beam.range,
                beam.tier,
                &beam.blocked_items,
                hardness,
            );
            if amount == 0 {
                return false;
            }
        }
        Some(crate::world::BlockKindData::WallCrafter(crafter)) => {
            let eff = crate::world::behavior::production::wall_crafter_efficiency(
                table,
                grid,
                crate::world::TilePos::new(x as i16, y as i16),
                size,
                rot,
                &crafter.attribute,
            );
            if eff <= 0.0 {
                return false;
            }
        }
        // `SolidPump.canPlaceOn` (`SolidPump.java:68-75`; T7): the footprint sum
        // `(canPump ? baseEfficiency + attribute : 0)` must exceed `0.00001`.
        Some(crate::world::BlockKindData::SolidPump(pump))
            if !solid_pump_can_place_on(
                content,
                grid,
                size,
                x,
                y,
                &pump.attribute,
                pump.base_efficiency,
            ) =>
        {
            return false;
        }
        Some(crate::world::BlockKindData::Fracker(fracker))
            if !solid_pump_can_place_on(
                content,
                grid,
                size,
                x,
                y,
                &fracker.attribute,
                fracker.base_efficiency,
            ) =>
        {
            return false;
        }
        _ => {}
    }
    // (c) `Block.canPlaceOn` (`CoreBlock.canPlaceOn`, `CoreBlock.java:185-203`).
    // Deviations: `requiresCoreZone` is treated false (no Erekir cores in the
    // port), and the API/dev form (`PlaceFlags::API`, `check_core_radius` off)
    // skips the check like it skips the other world gates, so map/test staging
    // of cores keeps working; the player form runs it exactly as Java.
    if def.kind == BlockKind::CoreBlock
        && flags.check_core_radius
        && !core_can_place_on(content, grid, world, rules, def, team, x, y)
    {
        return false;
    }
    true
}

/// `CoreBlock.canPlaceOn` (`CoreBlock.java:185-203`): a `core-zone`-free
/// footprint of allowed floors is always placeable; otherwise the team's core
/// must hold the requirements and the centre tile must hold a smaller core.
/// `requiresCoreZone` is true only for the Erekir cores; treated as false (the
/// port's Serpulo content never sets it).
#[allow(clippy::too_many_arguments)]
fn core_can_place_on(
    content: &ContentRegistry,
    grid: &WorldGrid,
    world: Option<&bevy_ecs::world::World>,
    rules: &BuildRules,
    def: &BlockDef,
    team: u8,
    x: i32,
    y: i32,
) -> bool {
    if rules.editor || rules.core_build_and_config {
        return true;
    }
    let size = def.size.max(1);
    let offset = -(size - 1) / 2;
    let mut any_core_or_disallowed = false;
    for dx in 0..size {
        for dy in 0..size {
            let tx = x + offset + dx;
            let ty = y + offset + dy;
            if !grid.tiles.in_bounds(tx, ty) {
                return false;
            }
            let tile = grid.tile(tx, ty);
            let floor_allows = content
                .block(tile.floor)
                .is_some_and(|floor| floor.allow_core_placement);
            let holds_core = content
                .block(tile.block)
                .is_some_and(|block| block.kind == BlockKind::CoreBlock);
            if !floor_allows || holds_core {
                any_core_or_disallowed = true;
            }
        }
    }
    if !any_core_or_disallowed {
        return true;
    }
    // Without a world the team core/items cannot be read (harness paths).
    let Some(world) = world else {
        return true;
    };
    let Some(core) = first_team_core(world, team) else {
        return false;
    };
    if !rules.infinite_resources {
        let items = world.get::<crate::world::modules::ItemModule>(core);
        for stack in &def.requirements {
            let amount = ((stack.amount as f32) * rules.build_cost_multiplier).round() as i32;
            if items.map(|module| module.get(stack.item)).unwrap_or(0) < amount {
                return false;
            }
        }
    }
    let center = &grid.tile(x, y);
    content
        .block(center.block)
        .is_some_and(|block| block.kind == BlockKind::CoreBlock && def.size > block.size)
}

/// The team's core (`Team.core()`): first `CoreBuild` of `team` by entity
/// index.
fn first_team_core(world: &bevy_ecs::world::World, team: u8) -> Option<bevy_ecs::entity::Entity> {
    let mut cores: Vec<(u32, bevy_ecs::entity::Entity)> = world
        .iter_entities()
        .filter(|entity| entity.contains::<crate::world::blocks::storage::CoreBuild>())
        .filter_map(|entity| {
            let core_team = entity.get::<TeamComp>()?.team;
            if core_team != team {
                return None;
            }
            Some((entity.id().index().index(), entity.id()))
        })
        .collect();
    cores.sort_unstable_by_key(|(index, _)| *index);
    cores.first().map(|(_, entity)| *entity)
}

/// `Block.bounds(x, y)` (`Block.java:1023-1025`) in world pixels: size in
/// pixels centered on `tile*8 + offset`.
fn block_bounds(size: i32, x: i32, y: i32) -> (f32, f32, f32, f32) {
    let offset = ((size + 1) % 2) as f32 * 4.0;
    let cx = x as f32 * 8.0 + offset;
    let cy = y as f32 * 8.0 + offset;
    let half = size as f32 * 8.0 / 2.0;
    (cx - half, cy - half, cx + half, cy + half)
}

/// `Tmp.r1.grow(grow).contains(Tmp.r2)`.
fn rect_contains_grown(
    outer: (f32, f32, f32, f32),
    inner: (f32, f32, f32, f32),
    grow: f32,
) -> bool {
    outer.0 - grow <= inner.0
        && outer.1 - grow <= inner.1
        && outer.2 + grow >= inner.2
        && outer.3 + grow >= inner.3
}

/// `Block.isVisible()` escape half: `BuildVisibility` + `revealedBlocks`
/// (`Block.java:1031-1037,1553-1555`, `BuildVisibility.java:10-21`).
pub fn build_visible(
    def: &BlockDef,
    rules: &BuildRules,
    content: &ContentRegistry,
    grid: &WorldGrid,
) -> bool {
    visibility_visible(def, rules, content, grid) || rules.revealed_blocks.contains(&def.id)
}

fn visibility_visible(
    def: &BlockDef,
    rules: &BuildRules,
    content: &ContentRegistry,
    grid: &WorldGrid,
) -> bool {
    use crate::content::registries::blocks::BuildVisibility as V;
    match def.build_visibility {
        V::Hidden => false,
        V::Shown => true,
        V::DebugOnly => false,
        V::EditorOnly => rules.editor,
        // `indexer.isBlockPresent(Blocks.coreZone) || !state.isGame()`: the
        // core-zone floor scan is the placement-relevant half (a scan is only
        // paid when this visibility is requested).
        V::CoreZoneOnly => core_zone_present(content, grid),
        V::WorldProcessorOnly => rules.editor || rules.allow_edit_world_processors,
        V::SandboxOnly => rules.infinite_resources,
        // `state.isCampaign() || !state.isGame()`.
        V::CampaignOnly => rules.is_campaign,
        // H04 owns launch pads; treated as visible/not-campaign.
        V::LegacyLaunchPadOnly => true,
        V::NotLegacyLaunchPadOnly => !rules.is_campaign,
        // `rules.lighting || isCampaign || rules.infiniteResources`.
        V::LightingOnly => rules.lighting || rules.is_campaign || rules.infinite_resources,
        V::FogOnly => rules.fog || rules.editor,
    }
}

/// The nearest core of a team whose rule `protectCores` is set
/// (`Build.java:197-206`); tie-break by ascending entity index.
fn closest_protecting_core(
    world: &bevy_ecs::world::World,
    rules: &BuildRules,
    wx: f32,
    wy: f32,
) -> Option<u8> {
    let mut cores: Vec<(u32, u8, f32, f32)> = world
        .iter_entities()
        .filter(|entity| entity.contains::<crate::world::blocks::storage::CoreBuild>())
        .filter_map(|entity| {
            let team = entity.get::<TeamComp>()?.team;
            let pos = entity.get::<crate::entities::comp::Pos>()?;
            Some((entity.id().index().index(), team, pos.x, pos.y))
        })
        .collect();
    cores.sort_unstable_by_key(|(index, ..)| *index);
    let mut best: Option<(f32, u8)> = None;
    for (_, core_team, px, py) in cores {
        if !rules.team_rule(core_team).protect_cores {
            continue;
        }
        let dx = px - wx;
        let dy = py - wy;
        let dst2 = dx * dx + dy * dy;
        if best.is_none_or(|(current, _)| dst2 < current) {
            best = Some((dst2, core_team));
        }
    }
    best.map(|(_, team)| team)
}

/// `Teams.anyEnemyCoresWithinBuildRadius(team, x, y)`
/// (`Teams.java:60-71`; `Rules.java:294-296`).
fn enemy_core_within_build_radius(
    world: &bevy_ecs::world::World,
    rules: &BuildRules,
    team: u8,
    wx: f32,
    wy: f32,
) -> bool {
    let mut cores: Vec<(u32, u8, f32, f32)> = world
        .iter_entities()
        .filter(|entity| entity.contains::<crate::world::blocks::storage::CoreBuild>())
        .filter_map(|entity| {
            let core_team = entity.get::<TeamComp>()?.team;
            let pos = entity.get::<crate::entities::comp::Pos>()?;
            Some((entity.id().index().index(), core_team, pos.x, pos.y))
        })
        .collect();
    cores.sort_unstable_by_key(|(index, ..)| *index);
    for (_, core_team, px, py) in cores {
        if core_team == team || !rules.team_rule(core_team).protect_cores {
            continue;
        }
        let radius = rules.build_radius(core_team) + 8.0;
        let dx = px - wx;
        let dy = py - wy;
        if dx * dx + dy * dy <= radius * radius {
            return true;
        }
    }
    false
}

/// `Build.getEnemyOverlap` (`Build.java:275-277`; `BlockIndexer.findEnemyTile`
/// `:456-483`): the closest enemy building whose team checks placement, within
/// `placeOverlapRange + 4 + hitSize/2` of `(x*8 + size, y*8 + size)`.
///
/// `Block.placeOverlapRange` (default 50, `Block.java`) is not a `BlockDef`
/// field yet; the default is used.
pub fn get_enemy_overlap(
    world: &bevy_ecs::world::World,
    table: &BlockTable,
    rules: &BuildRules,
    def: &BlockDef,
    team: u8,
    x: i32,
    y: i32,
) -> Option<bevy_ecs::entity::Entity> {
    const PLACE_OVERLAP_RANGE: f32 = 50.0;
    let range = PLACE_OVERLAP_RANGE + 4.0;
    let cx = x as f32 * 8.0 + def.size.max(1) as f32;
    let cy = y as f32 * 8.0 + def.size.max(1) as f32;
    let mut buildings: Vec<(u32, bevy_ecs::entity::Entity, f32, f32, f32)> = world
        .iter_entities()
        .filter(|entity| entity.contains::<BuildingComp>())
        .filter_map(|entity| {
            let comp = entity.get::<BuildingComp>()?;
            if comp.team.0 == team {
                return None;
            }
            if !rules.team_rule(comp.team.0).check_placement {
                return None;
            }
            let pos = entity.get::<crate::entities::comp::Pos>()?;
            let hit = table
                .get(comp.block)
                .map(|inst| inst.def.size.max(1) as f32 * 8.0)
                .unwrap_or(8.0);
            Some((entity.id().index().index(), entity.id(), pos.x, pos.y, hit))
        })
        .collect();
    buildings.sort_unstable_by_key(|(index, ..)| *index);
    for (_, entity, px, py, hit) in buildings {
        let dx = px - cx;
        let dy = py - cy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < range + hit / 2.0 {
            return Some(entity);
        }
    }
    None
}

/// Resolved family knobs for placement checks (`None` without a table entry).
fn kind_data_for(table: &BlockTable, block: BlockId) -> Option<crate::world::BlockKindData> {
    table.get(block).map(|instance| instance.kind_data.clone())
}

/// `Pump.canPlaceOn`: every pumpable tile of the footprint drops the same
/// liquid and at least one exists (PAR-0078).
fn pump_can_place_on(
    content: &ContentRegistry,
    grid: &WorldGrid,
    size: i32,
    x: i32,
    y: i32,
) -> bool {
    let offset = -(size - 1) / 2;
    let mut last = None;
    for dx in 0..size {
        for dy in 0..size {
            let (tx, ty) = (x + offset + dx, y + offset + dy);
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            let Some(drop) = content
                .block(grid.tile(tx, ty).floor)
                .and_then(|floor| floor.liquid_drop)
            else {
                continue;
            };
            if last.is_some_and(|last| last != drop) {
                return false;
            }
            last = Some(drop);
        }
    }
    last.is_some()
}

/// `SolidPump.canPlaceOn` (`SolidPump.java:68-75`; T7): the footprint sum
/// `Σ (canPump(t) ? baseEfficiency + attribute(t) : 0)` must exceed `0.00001`.
fn solid_pump_can_place_on(
    content: &ContentRegistry,
    grid: &WorldGrid,
    size: i32,
    x: i32,
    y: i32,
    attribute: &str,
    base_efficiency: f32,
) -> bool {
    let offset = -(size - 1) / 2;
    let mut sum = 0.0f32;
    for dx in 0..size {
        for dy in 0..size {
            let (tx, ty) = (x + offset + dx, y + offset + dy);
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            let Some(floor) = content.block(grid.tile(tx, ty).floor) else {
                continue;
            };
            // `canPump(tile) = !tile.floor().isLiquid`.
            if floor.is_liquid {
                continue;
            }
            let affinity = if attribute.is_empty() {
                0.0
            } else {
                floor
                    .attributes
                    .iter()
                    .find(|(name, _)| name == attribute)
                    .map(|(_, value)| *value)
                    .unwrap_or(0.0)
            };
            sum += base_efficiency + affinity;
        }
    }
    sum > 0.00001
}

/// `Build.contactsGround` — whether a block footprint touches solid ground.
pub fn contacts_ground(
    content: &ContentRegistry,
    grid: &WorldGrid,
    block: BlockId,
    x: i32,
    y: i32,
) -> bool {
    let Some(def) = content.block(block) else {
        return false;
    };
    let size = def.size.max(1);
    let offset = -(size - 1) / 2;
    for dx in 0..size {
        for dy in 0..size {
            let tx = x + offset + dx;
            let ty = y + offset + dy;
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            // A floor that is not a liquid/air counts as ground.
            let floor = grid.tile(tx, ty).floor;
            if floor != BlockId::AIR
                && content
                    .block(floor)
                    .is_some_and(|def| def.solid || !def.placeable_liquid)
            {
                return true;
            }
            // Adjacent solid blocks also count.
            for (nx, ny) in [(tx - 1, ty), (tx + 1, ty), (tx, ty - 1), (tx, ty + 1)] {
                if !grid.tiles.in_bounds(nx, ny) {
                    continue;
                }
                let neighbor = grid.tile(nx, ny).block;
                if neighbor != BlockId::AIR && content.block(neighbor).is_some_and(|def| def.solid)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// `Build.contactsShallows` (`Build.java:294-314`): any inside-edge/edge floor
/// of a multiblock (or any d4 neighbour plus the tile itself for a 1x1) that is
/// not deep. `Floor.isDeep()` = `drownTime > 0`.
pub fn contacts_shallows(
    content: &ContentRegistry,
    grid: &WorldGrid,
    def: &BlockDef,
    x: i32,
    y: i32,
) -> bool {
    let size = def.size.max(1);
    let deep = |tx: i32, ty: i32| -> bool {
        if !grid.tiles.in_bounds(tx, ty) {
            // `tile == null` does not count as "not deep".
            return true;
        }
        content
            .block(grid.tile(tx, ty).floor)
            .is_some_and(|floor| floor.is_deep)
    };
    if size > 1 {
        for point in crate::world::edges::inside_edges(size) {
            if !deep(x + i32::from(point.x()), y + i32::from(point.y())) {
                return true;
            }
        }
        for point in crate::world::edges::edges(size) {
            if !deep(x + i32::from(point.x()), y + i32::from(point.y())) {
                return true;
            }
        }
        false
    } else {
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            if !deep(x + dx, y + dy) {
                return true;
            }
        }
        !deep(x, y)
    }
}

/// `BlockIndexer.isBlockPresent(Blocks.coreZone)`: any tile of the grid carries
/// the `core-zone` floor (`Blocks.java:587`).
fn core_zone_present(content: &ContentRegistry, grid: &WorldGrid) -> bool {
    let Some(zone) = content.block_id("core-zone") else {
        return false;
    };
    grid.tiles
        .iter()
        .any(|tile| tile.floor == zone || tile.overlay == zone || tile.block == zone)
}

/// Convenience: [`BlockView`] for a placed block.
pub fn view_of<'a>(
    content: &'a ContentRegistry,
    table: &'a BlockTable,
    id: BlockId,
) -> Option<BlockView<'a>> {
    table.view(id).filter(|_| content.block(id).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_replace_same_group_and_construct() {
        let content = crate::content::test_support::test_registry();
        let wall = content.block_by_name("copper-wall").expect("wall");
        let wall2 = content.block_by_name("titanium-wall").expect("wall2");
        assert!(can_replace(wall, wall2));
        let build2 = content.block_by_name("build2").expect("build2");
        assert!(can_replace(wall, build2));
    }

    #[test]
    fn can_replace_honors_always_replace_and_privileged() {
        let content = crate::content::test_support::test_registry();
        let stone = content.block_by_name("stone").expect("stone");
        // `Prop` subclasses set `alwaysReplace = true`.
        let boulder = content.block_by_name("sand-boulder").expect("sand-boulder");
        assert!(can_replace(stone, boulder));
        // `privileged` blocks can never be replaced.
        let processor = content
            .block_by_name("world-processor")
            .expect("world-processor");
        assert!(!can_replace(stone, processor));
    }

    #[test]
    fn valid_place_rejects_occupied_dissimilar() {
        let content = crate::content::test_support::test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let rules = BuildRules::default();
        let counter = BlockCounter::new();
        let mut grid = WorldGrid::new(8, 8);
        grid.fill(BlockId::STONE_WALL, BlockId::AIR);
        let core = content.block_id("core-shard").expect("core-shard");
        let wall = content.block_id("copper-wall").expect("copper-wall");
        assert!(valid_place_at(
            &content, &table, &rules, &counter, &grid, wall, 0, 0, 4, 4, true
        ));
        // Place a core, then a wall over it: same-group? no -> rejected.
        grid.tiles.get_mut(4, 4).block = core;
        assert!(!valid_place_at(
            &content, &table, &rules, &counter, &grid, wall, 0, 0, 4, 4, true
        ));
    }

    /// T5: the map-area limit rect + border darkness block placement
    /// (`World.isInMapArea`/`getDarkness`, `World.java:483-503`;
    /// `Build.validPlace` `Build.java:229`). Derivation: `dark = (2 - edgeDst)
    /// * (4 / 2)`; edgeDst 0 → 4 (blocked), 1 → 2 (allowed).
    #[test]
    fn limit_area_blocks_placement_outside() {
        let content = crate::content::test_support::test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let rules = BuildRules::default();
        let counter = BlockCounter::new();
        let mut grid = WorldGrid::new(64, 64);
        grid.fill(BlockId::AIR, BlockId::AIR);
        grid.limit_area = Some((10, 10, 20, 20));
        let wall = content.block_id("copper-wall").expect("copper-wall");
        assert!(valid_place_at(
            &content, &table, &rules, &counter, &grid, wall, 0, 0, 15, 15, true
        ));
        assert!(!valid_place_at(
            &content, &table, &rules, &counter, &grid, wall, 0, 0, 5, 15, true
        ));
        assert!(
            !valid_place_at(
                &content, &table, &rules, &counter, &grid, wall, 0, 0, 10, 15, true
            ),
            "edge tile: edgeDst 0 -> dark 4"
        );
        assert!(
            valid_place_at(
                &content, &table, &rules, &counter, &grid, wall, 0, 0, 11, 15, true
            ),
            "one inside: edgeDst 1 -> dark 2"
        );
    }
}
