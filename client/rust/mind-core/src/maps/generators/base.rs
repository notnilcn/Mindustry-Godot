// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BaseGenerator` — enemy/ally base decoration (plan 06 §3.10, risk R3).
//!
//! Ported from `maps/generators/BaseGenerator.java`. Operates on the live
//! world through [`StructureCtx`] (H05 T5) or a raw [`WorldCtx`] (I04 T17);
//! `try_place`/`is_taken`/the resource `set` are shared with the Serpulo ruins
//! pass (H05 T6). [`generate`]/[`post_generate`] (I04 T17) are the
//! `BaseGenerator.generate`/`postGenerate` enemy-base halves.

use crate::ai::base_registry::{BasePart, BaseRegistry, BaseResource};
use crate::content::registries::blocks::{BlockKind, BuildVisibility};
use crate::content::{BlockDef, BlockId, ContentRegistry, ItemId, PlanetId};
use crate::determinism::RngStream;
use crate::determinism::SimRng;
use crate::game::rules::Rules;
use crate::maps::generators::planet::StructureCtx;
use crate::math::rand_arc::ArcRand;
use crate::world::Tile;
use crate::world::ops::WorldCtx;
use crate::world::tiles::Tiles;

/// `BaseGenerator.range` (`BaseGenerator.java:27`): the pass radius (tiles)
/// around the first core.
const RANGE: i32 = 180;

/// `Geometry.d4`, Java order: `(1,0),(0,1),(-1,0),(0,-1)`.
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// `Seq.getFrac` (`Arc Seq.java`): `items[clamp((int)(f * size), 0, size - 1)]`,
/// `None` when empty.
pub fn get_frac<T>(items: &[T], f: f32) -> Option<&T> {
    if items.is_empty() {
        return None;
    }
    let index = ((f * items.len() as f32) as i32).clamp(0, items.len() as i32 - 1);
    items.get(index as usize)
}

/// The `Rand` calls `BaseGenerator.tryPlace`/ruins make. Implemented for
/// `ArcRand` (enemy bases, Java `Mathf.rand`) and the generator `SimRng`
/// (ruins, `RngStream::MapGen`).
pub trait BaseRand {
    /// Arc `Rand.range(int amount) -> nextInt(amount * 2 + 1) - amount`.
    fn range_int(&mut self, amount: i32) -> i32;

    /// Arc `Rand.chance(double)`.
    fn chance(&mut self, chance: f64) -> bool;

    /// Arc `Rand.random(float max) -> nextFloat() * max`.
    fn random_float(&mut self, max: f32) -> f32;
}

impl BaseRand for ArcRand {
    fn range_int(&mut self, amount: i32) -> i32 {
        ArcRand::range_int(self, amount)
    }

    fn chance(&mut self, chance: f64) -> bool {
        ArcRand::chance(self, chance)
    }

    fn random_float(&mut self, max: f32) -> f32 {
        ArcRand::random_float(self, max)
    }
}

impl BaseRand for SimRng {
    fn range_int(&mut self, amount: i32) -> i32 {
        self.random(RngStream::MapGen, amount * 2 + 1) - amount
    }

    fn chance(&mut self, chance: f64) -> bool {
        SimRng::chance(self, RngStream::MapGen, chance)
    }

    fn random_float(&mut self, max: f32) -> f32 {
        self.next_float(RngStream::MapGen) * max
    }
}

/// The per-tile callback `tryPlace` runs after validation; Java's closure
/// captures the world implicitly, the Rust one receives the live [`WorldCtx`].
pub type PosCallback<'a> = dyn FnMut(&mut WorldCtx<'_>, &mut dyn BaseRand, i32, i32) + 'a;

/// `BaseGenerator.tryPlace` (`BaseGenerator.java:196-262`) for callers that
/// hold a [`StructureCtx`] (the Serpulo ruins pass): runs one
/// [`Sim::with_world_ctx`](crate::sim::Sim::with_world_ctx) around
/// `try_place_world`.
pub fn try_place(
    ctx: &mut StructureCtx,
    part: &BasePart,
    x: i32,
    y: i32,
    team: u8,
    rand: &mut dyn BaseRand,
    posc: Option<&mut PosCallback<'_>>,
) -> bool {
    let StructureCtx {
        sim,
        content: _,
        bases,
        rules,
        wave_team,
        ..
    } = ctx;
    let bases = *bases;
    let wave_team = *wave_team;
    let enemy_core_radius = rules.enemy_core_build_radius;
    let rules_ref: &Rules = rules;
    sim.with_world_ctx(|wctx| {
        try_place_world(
            wctx,
            bases,
            rules_ref,
            wave_team,
            enemy_core_radius,
            part,
            x,
            y,
            team,
            rand,
            posc,
        )
    })
    .unwrap_or(false)
}

/// `BaseGenerator.tryPlace` core (`BaseGenerator.java:196-262`): tries to place
/// a base part at `(x, y)`; the optional `posc` runs per placed tile after
/// validation (the ruins ore-scrap callback).
#[allow(clippy::too_many_arguments)]
fn try_place_world(
    wctx: &mut WorldCtx<'_>,
    bases: Option<&BaseRegistry>,
    rules: &Rules,
    wave_team: u8,
    enemy_core_radius: f32,
    part: &BasePart,
    x: i32,
    y: i32,
    team: u8,
    rand: &mut dyn BaseRand,
    mut posc: Option<&mut PosCallback<'_>>,
) -> bool {
    let rotation = rand.range_int(2);
    let axis = (
        (part.schematic.width as f32 / 2.0) as i32,
        (part.schematic.height as f32 / 2.0) as i32,
    );
    let result =
        crate::game::schematics::Schematics::rotate(&part.schematic, rotation, wctx.content);

    let (rx, ry) = rotate_around(
        part.center_x as f32,
        part.center_y as f32,
        axis,
        rotation as f32 * 90.0,
    );
    // Bottom-left schematic corner.
    let cx = x - rx as i32;
    let cy = y - ry as i32;

    for tile in &result.tiles {
        let real_x = tile.x as i32 + cx;
        let real_y = tile.y as i32 + cy;
        if is_taken(
            wctx,
            rules,
            wave_team,
            enemy_core_radius,
            tile.block,
            real_x,
            real_y,
        ) {
            return false;
        }
        // `tile.block == Blocks.oilExtractor && sumAttribute(Attribute.oil)
        // <= 0.001`: the port's `Attribute` enum has no `oil` yet
        // (`content/registries/weathers.rs`), so the clause is recorded as a
        // deviation; oil extractors only exist in sandbox-only parts.
    }

    // Only do the callback after validation.
    if let Some(posc) = posc.as_mut() {
        for tile in &result.tiles {
            posc(wctx, rand, tile.x as i32 + cx, tile.y as i32 + cy);
        }
    }

    if let Some(BaseResource::Item(item)) = part.required {
        let content = wctx.content;
        for tile in &result.tiles {
            if !is_drill(content, tile.block) {
                continue;
            }
            for_each_taken(
                content,
                tile.block,
                tile.x as i32 + cx,
                tile.y as i32 + cy,
                |ex, ey| {
                    let Some(placed) = wctx.grid.tiles.getn(ex, ey) else {
                        return;
                    };
                    if has_surface(content, placed.floor) {
                        set(wctx, bases, ex, ey, item);
                    }
                    let rand_x = ex + rand.range_int(1);
                    let rand_y = ey + rand.range_int(1);
                    let clamped = wctx.grid.tiles.getc(rand_x, rand_y);
                    if has_surface(content, clamped.floor) {
                        set(wctx, bases, rand_x, rand_y, item);
                    }
                },
            );
        }
    }

    let _ = crate::game::schematics::place(
        wctx,
        &result,
        cx + result.width / 2,
        cy + result.height / 2,
        team,
        false,
    );

    // Fill drills with items after placing.
    if let Some(BaseResource::Item(item)) = part.required {
        for tile in &result.tiles {
            if !is_drill(wctx.content, tile.block) {
                continue;
            }
            let Some(entity) = wctx
                .grid
                .tiles
                .getn(tile.x as i32 + cx, tile.y as i32 + cy)
                .and_then(|t| t.build)
            else {
                continue;
            };
            let Some(block) = wctx
                .ecs
                .get::<crate::ecs::BuildingComp>(entity)
                .map(|comp| comp.block)
            else {
                continue;
            };
            if block != tile.block {
                continue;
            }
            let capacity = wctx
                .content
                .block(block)
                .map(|def| def.item_capacity)
                .unwrap_or(0);
            if let Some(mut module) = wctx
                .ecs
                .get_mut::<crate::world::modules::ItemModule>(entity)
            {
                module.add(item, capacity.max(0), capacity);
            }
        }
    }

    true
}

/// `BaseGenerator.getDifficultyWall` (`BaseGenerator.java:33-40`): the vanilla,
/// shown, non-insulated, non-`Door` wall of `size` for `planet`, sorted by
/// `buildTime`; `get_frac(difficulty * 0.91f)`.
///
/// Deviation: Arc's unstable float sort is replaced by a stable
/// `build_time` sort; vanilla wall build times are distinct, so the order is
/// identical unless two candidates tie exactly.
pub fn get_difficulty_wall(
    content: &ContentRegistry,
    size: i32,
    difficulty: f32,
    planet: Option<PlanetId>,
) -> Option<BlockId> {
    let walls = difficulty_walls(content, size, planet);
    get_frac(&walls, difficulty * 0.91).copied()
}

/// The sorted `getDifficultyWall` candidate list (`wallsSmall`).
pub fn difficulty_walls(
    content: &ContentRegistry,
    size: i32,
    planet: Option<PlanetId>,
) -> Vec<BlockId> {
    let mut walls: Vec<&BlockDef> = content
        .blocks()
        .iter()
        .filter(|def| {
            is_wall_instance(def.kind)
                && def.kind != BlockKind::Door
                && def.minfo.is_vanilla()
                && def.size == size
                && !def.insulated
                && def.build_visibility == BuildVisibility::Shown
                && is_on_planet(content, def, planet)
        })
        .collect();
    walls.sort_by(|a, b| a.build_time.total_cmp(&b.build_time));
    walls.iter().map(|def| def.id).collect()
}

/// `Block instanceof Wall` (`J/world/blocks/defense/Wall.java`); in Java
/// `ShieldWall`, `Door`, `AutoDoor` and `Thruster` extend `Wall`.
fn is_wall_instance(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Wall
            | BlockKind::ShieldWall
            | BlockKind::Door
            | BlockKind::AutoDoor
            | BlockKind::Thruster
    )
}

/// `UnlockableContent.isOnPlanet` (`UnlockableContent.java:132-134`):
/// `planet == null || planet == Planets.sun || shownPlanets.isEmpty() ||
/// shownPlanets.contains(planet)`.
fn is_on_planet(content: &ContentRegistry, def: &BlockDef, planet: Option<PlanetId>) -> bool {
    let Some(planet) = planet else {
        return true;
    };
    if content.planet_id("sun") == Some(planet) {
        return true;
    }
    def.unlock.shown_planets.is_empty() || def.unlock.shown_planets.contains(&planet)
}

/// `o.block() instanceof PayloadConveyor || o.block() instanceof PayloadBlock`
/// (`BaseGenerator.java:109`); `PayloadRouter` extends `PayloadConveyor`.
fn blocks_payloads(kind: BlockKind) -> bool {
    kind.is_payload_block() || matches!(kind, BlockKind::PayloadConveyor | BlockKind::PayloadRouter)
}

/// The generator state kept between [`generate`] and [`post_generate`]
/// (`BaseGenerator.tiles`/`cores` fields).
#[derive(Debug, Clone, Default)]
pub struct BaseGenState {
    /// Whether [`generate`] ran (`BaseGenerator.tiles != null`).
    pub ran: bool,
    /// Enemy room centres passed to [`generate`] (`BaseGenerator.cores`).
    pub cores: Vec<(i32, i32)>,
}

/// `BaseGenerator.generate` (`BaseGenerator.java:42-172`): places the enemy
/// room `cores` (core loadouts + item fill), the random parts, the wall ring
/// and the ground-unit path clearing through one live [`WorldCtx`]. Returns the
/// state [`post_generate`] reads (`BaseGenerator.tiles`).
pub fn generate(
    ctx: &mut StructureCtx,
    cores: &[(i32, i32)],
    spawn: (i32, i32),
    team: u8,
    sector_id: u32,
    difficulty: f32,
) -> BaseGenState {
    let state = BaseGenState {
        ran: true,
        cores: cores.to_vec(),
    };
    let StructureCtx {
        sim,
        content,
        bases,
        rules,
        wave_team,
        ..
    } = ctx;
    let Some(bases) = *bases else {
        return state;
    };
    // "don't generate bases when there are no loaded schematics"
    if bases.cores.is_empty() {
        return state;
    }
    let env = GenEnv {
        bases,
        rules,
        team,
        wave_team: *wave_team,
        enemy_core_radius: rules.enemy_core_build_radius,
        planet: content.planet_id(&rules.planet),
        sector_id,
        difficulty,
        cores,
        spawn,
    };
    sim.with_world_ctx(|wctx| generate_world(wctx, &env));
    state
}

/// Inputs [`generate_world`] needs (bundled so the pass stays readable).
struct GenEnv<'a> {
    bases: &'a BaseRegistry,
    rules: &'a Rules,
    team: u8,
    wave_team: u8,
    enemy_core_radius: f32,
    planet: Option<PlanetId>,
    sector_id: u32,
    difficulty: f32,
    cores: &'a [(i32, i32)],
    spawn: (i32, i32),
}

/// `BaseGenerator.generate` (`BaseGenerator.java:42-172`) against a live
/// [`WorldCtx`]: core loadouts, the item-filled cores, the `passes` random
/// parts, the wall ring and the ground-unit path clearing.
fn generate_world(wctx: &mut WorldCtx<'_>, env: &GenEnv<'_>) {
    let content = wctx.content;

    // `Mathf.rand.setSeed(sector.id); Mathf.rand.nextDouble();` — a local
    // `ArcRand` reproduces the global `Mathf.rand` because nothing else draws
    // during generation.
    let mut rand = ArcRand::new(u64::from(env.sector_id));
    let _ = rand.next_double();

    let bracket_range = 0.17f32;
    let base_chance = 0.7 + (2.1 - 0.7) * env.difficulty;
    let wall_angle = 70f32;
    let resource_chance = 0.5 * f64::from(base_chance);
    let non_resource_chance = 0.002 * f64::from(base_chance);
    let passes = if env.difficulty < 0.4 {
        1
    } else if env.difficulty < 0.8 {
        3
    } else {
        5
    };

    let (Some(wall), Some(wall_large)) = (
        get_difficulty_wall(content, 1, env.difficulty, env.planet),
        get_difficulty_wall(content, 2, env.difficulty, env.planet),
    ) else {
        log::warn!(
            "base generator: no difficulty wall for planet `{}`",
            env.rules.planet
        );
        return;
    };

    for &(cx, cy) in env.cores {
        // `tile.clearOverlay()`.
        if let (Ok(x), Ok(y)) = (i16::try_from(cx), i16::try_from(cy)) {
            wctx.set_overlay(x, y, BlockId::AIR);
        }
        let Some(core_part) = get_frac(
            &env.bases.cores,
            (env.difficulty + rand.range_float(0.4)) / 1.4,
        ) else {
            continue; // `bases.cores` is non-empty; keeps the port panic-free.
        };
        let _ = crate::game::schematics::place_loadout(
            wctx,
            &core_part.schematic,
            cx,
            cy,
            env.team,
            false,
        );

        // Fill the core with every type of item (even non-material).
        let Some(entity) = wctx.grid.tiles.getn(cx, cy).and_then(|tile| tile.build) else {
            continue;
        };
        let capacity = wctx
            .ecs
            .get::<crate::ecs::BuildingComp>(entity)
            .and_then(|comp| content.block(comp.block))
            .map(|def| def.item_capacity)
            .unwrap_or(0);
        if let Some(mut module) = wctx
            .ecs
            .get_mut::<crate::world::modules::ItemModule>(entity)
        {
            for item in content.items() {
                module.add(item.id, capacity, capacity);
            }
        }
    }

    // `pass`: every tile in `RANGE` of the first core (`Tile.circle` ->
    // `Geometry.circle(x, y, width, height, radius)`, strict `Mathf.within`).
    let Some(&first_core) = env.cores.first() else {
        return;
    };
    let pass_tiles = circle_tiles(&wctx.grid.tiles, first_core.0, first_core.1, RANGE);

    for _ in 0..passes {
        // Random schematics (`:72-87`).
        for &(x, y) in &pass_tiles {
            let (always_replace, overlay_drop, floor_liquid_drop, tile_drop) = {
                let tile = wctx.grid.tiles.get(x, y);
                let always_replace = content
                    .block(tile.block)
                    .map(|def| def.always_replace)
                    .unwrap_or(false);
                // `tile.overlay().asFloor().itemDrop` / `tile.floor().itemDrop`
                // (`Tile.overlay`/`Tile.floor` are `Floor`-typed upstream).
                let overlay_drop = content.block(tile.overlay).and_then(|def| def.item_drop);
                let floor_drop = content.block(tile.floor).and_then(|def| def.item_drop);
                // `Tile.drop()` (`Tile.java:580-582`).
                let tile_drop = if tile.overlay == BlockId::AIR || overlay_drop.is_none() {
                    floor_drop
                } else {
                    overlay_drop
                };
                let floor_liquid_drop = content.block(tile.floor).and_then(|def| def.liquid_drop);
                (always_replace, overlay_drop, floor_liquid_drop, tile_drop)
            };
            if !always_replace {
                continue;
            }

            let mut left = overlay_drop.is_some();
            if !left {
                if tile_drop.is_some() && rand.chance(non_resource_chance) {
                    left = true;
                }
                if !left && floor_liquid_drop.is_some() && rand.chance(non_resource_chance * 2.0) {
                    left = true;
                }
            }

            if left && rand.chance(resource_chance) {
                let resource = match tile_drop {
                    Some(item) => Some(BaseResource::Item(item)),
                    None => floor_liquid_drop.map(BaseResource::Liquid),
                };
                if let Some(resource) = resource
                    && let Some(part) = get_frac(
                        env.bases.for_resource(resource),
                        env.difficulty + rand.range_float(bracket_range),
                    )
                {
                    let _ = try_place_world(
                        wctx,
                        Some(env.bases),
                        env.rules,
                        env.wave_team,
                        env.enemy_core_radius,
                        part,
                        x,
                        y,
                        env.team,
                        &mut rand,
                        None,
                    );
                }
            } else if rand.chance(non_resource_chance)
                && let Some(part) = get_frac(&env.bases.parts, rand.random_float(1.0))
            {
                let _ = try_place_world(
                    wctx,
                    Some(env.bases),
                    env.rules,
                    env.wave_team,
                    env.enemy_core_radius,
                    part,
                    x,
                    y,
                    env.team,
                    &mut rand,
                    None,
                );
            }
        }
    }

    // Replace walls with the correct type (`:89-95`; disabled upstream with
    // `if(false)`).
    //
    //     pass(tile -> {
    //         if(tile.block() instanceof Wall && tile.team() == team && ...)
    //             tile.setBlock(tile.block().size == 2 ? wallLarge : wall, team);
    //     });

    if wall_angle > 0.0 {
        // Small walls (`:99-130`).
        for &(x, y) in &pass_tiles {
            let always_replace = {
                let tile = wctx.grid.tiles.get(x, y);
                content
                    .block(tile.block)
                    .map(|def| def.always_replace)
                    .unwrap_or(false)
            };
            if !always_replace {
                continue;
            }

            // Do not block payloads.
            let mut payload = false;
            for (dx, dy) in D4 {
                if let Some(o) = wctx.grid.tiles.getn(x + dx, y + dy)
                    && let Some(def) = content.block(o.block)
                    && blocks_payloads(def.kind)
                {
                    payload = true;
                    break;
                }
            }
            if payload {
                continue;
            }

            let spawn_angle = angle_to(env.spawn.0 as f32, env.spawn.1 as f32, x as f32, y as f32);
            let mut any = false;
            for (dx, dy) in crate::maps::generators::basic::D8 {
                if angle_dist(angle(dx as f32, dy as f32), spawn_angle) > wall_angle {
                    continue;
                }
                let Some(o) = wctx.grid.tiles.getn(x + dx, y + dy) else {
                    continue;
                };
                let o_team = tile_team(wctx.ecs, o);
                let o_kind = content.block(o.block).map(|def| def.kind);
                if o_team == env.team
                    && !o_kind.is_some_and(is_wall_instance)
                    && o_kind != Some(BlockKind::ShockMine)
                {
                    any = true;
                    break;
                }
            }
            if any && let (Ok(x16), Ok(y16)) = (i16::try_from(x), i16::try_from(y)) {
                wctx.set_block(x16, y16, wall, env.team, 0);
            }
        }

        // Large walls (`:132-149`).
        for &(x, y) in &pass_tiles {
            let mut walls = 0;
            let mut ok = true;
            'cell: for cx in 0..2 {
                for cy in 0..2 {
                    let Some(tile) = wctx.grid.tiles.getn(x + cx, y + cy) else {
                        ok = false;
                        break 'cell;
                    };
                    let size = content.block(tile.block).map(|def| def.size).unwrap_or(1);
                    let is_wall = tile.block == wall;
                    let always_replace = content
                        .block(tile.block)
                        .map(|def| def.always_replace)
                        .unwrap_or(false);
                    if size != 1 || (!is_wall && !always_replace) {
                        ok = false;
                        break 'cell;
                    }
                    if is_wall {
                        walls += 1;
                    }
                }
            }
            if ok
                && walls >= 3
                && let (Ok(x16), Ok(y16)) = (i16::try_from(x), i16::try_from(y))
            {
                wctx.set_block(x16, y16, wall_large, env.team, 0);
            }
        }
    }

    let core_dst = 10.0 * 8.0;

    // Clear path for ground units (`:153-171`).
    for &(core_x, core_y) in env.cores {
        let path = {
            let tiles = &wctx.grid.tiles;
            let ecs = &*wctx.ecs;
            let content = wctx.content;
            crate::maps::generators::astar::pathfind(
                tiles,
                (core_x, core_y),
                env.spawn,
                |_from, to| {
                    if tile_team(ecs, to) == env.wave_team
                        && !tile_within(to.x as i32, to.y as i32, core_x, core_y, core_dst)
                    {
                        100000.0
                    } else if has_surface(content, to.floor) {
                        1.0
                    } else {
                        10.0
                    }
                },
                |tile| {
                    !content
                        .block(tile.block)
                        .map(crate::maps::filters::block_info::is_static)
                        .unwrap_or(false)
                },
            )
        };

        let mut to_clear: Vec<(i32, i32)> = Vec::new();
        for (tx, ty) in path {
            if tile_within(tx, ty, core_x, core_y, core_dst) {
                continue;
            }
            let team = wctx
                .grid
                .tiles
                .getn(tx, ty)
                .map(|tile| tile_team(wctx.ecs, tile))
                .unwrap_or(0);
            if team == env.wave_team {
                to_clear.push((tx, ty));
            }
            for (dx, dy) in crate::maps::generators::basic::D8 {
                if let Some(other) = wctx.grid.tiles.getn(tx + dx, ty + dy)
                    && tile_team(wctx.ecs, other) == env.wave_team
                {
                    to_clear.push((tx + dx, ty + dy));
                }
            }
        }
        for (tx, ty) in to_clear {
            if let (Ok(x16), Ok(y16)) = (i16::try_from(tx), i16::try_from(ty)) {
                wctx.remove_block(x16, y16);
            }
        }
    }
}

/// `BaseGenerator.postGenerate` (`BaseGenerator.java:174-187`): every wave-team
/// center tile re-configures its `PowerNode`s (clear links, then `placed()`
/// re-links) and charges `Battery`s to full.
pub fn post_generate(ctx: &mut StructureCtx, state: &BaseGenState) {
    if !state.ran {
        return;
    }
    // Port compensation: Java's `PowerModule` construction gives every power
    // building a fresh `PowerGraph`, so `getPotentialLinks` links nodes right
    // after placement. The port only merges a graph on the runtime placement
    // path (`SimRuntime::set_block_raw`), while `base::generate` places through
    // `WorldCtx`; rebuild once so the `placed()` pass below sees distinct
    // graphs (`World.endMapLoad` rebuild; `Sim::install_block_runtime_with`).
    ctx.sim.rebuild_power_graphs();
    let wave_team = ctx.wave_team;
    let mut power_nodes: Vec<bevy_ecs::entity::Entity> = Vec::new();
    let mut batteries: Vec<bevy_ecs::entity::Entity> = Vec::new();
    {
        let tiles = &ctx.sim.grid.tiles;
        let ecs = &ctx.sim.ecs.0;
        // `for(Tile tile : tiles)` — array order (row-major).
        for index in 0..tiles.len() {
            let tile = tiles.geti(index);
            let Some(entity) = tile.build else {
                continue;
            };
            // `tile.isCenter()`.
            let Some(building) = ecs.get::<crate::entities::comp::Building>(entity) else {
                continue;
            };
            if building.tile != tile.pos() {
                continue;
            }
            let team = ecs
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|comp| comp.team)
                .unwrap_or(0);
            if team != wave_team {
                continue;
            }
            let Some(def) = ctx.content.block(tile.block) else {
                continue;
            };
            match def.kind {
                BlockKind::PowerNode | BlockKind::LongPowerNode => power_nodes.push(entity),
                BlockKind::Battery => batteries.push(entity),
                _ => {}
            }
        }
    }

    for entity in power_nodes {
        // `tile.build.configureAny(new Point2[0])` — clear the links
        // (`PowerNode.java` `config(Point2[].class)`).
        ctx.sim.apply_world_config(
            entity,
            crate::world::config::ConfigValue::Point2Array(Default::default()),
        );
        // `tile.build.placed()` — `PowerNodeBuild.placed` re-links
        // (`PowerNode.java:400-411`).
        ctx.sim.run_placed_hooks(entity);
    }
    for entity in batteries {
        crate::world::blocks::power::graph::set_status(&mut ctx.sim.ecs.0, entity, 1.0);
    }
}

/// `Vec2.rotateAround(origin, degrees)` (`Vec2.java:463-468`).
fn rotate_around(x: f32, y: f32, origin: (i32, i32), degrees: f32) -> (f32, f32) {
    let (ox, oy) = (origin.0 as f32, origin.1 as f32);
    let dx = x - ox;
    let dy = y - oy;
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    (ox + dx * cos - dy * sin, oy + dx * sin + dy * cos)
}

/// `BaseGenerator.set(tile, item)`: `bases.ores` -> overlay, else
/// `bases.oreFloors` -> floor.
fn set(wctx: &mut WorldCtx<'_>, bases: Option<&BaseRegistry>, x: i32, y: i32, item: ItemId) {
    let overlay = bases.and_then(|bases| bases.ores.get(&item).copied());
    let floor = bases.and_then(|bases| bases.ore_floors.get(&item).copied());
    if overlay.is_none() && floor.is_none() {
        return;
    }
    let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
        return;
    };
    if let Some(overlay) = overlay {
        wctx.set_overlay(x, y, overlay);
    } else if let Some(floor) = floor {
        wctx.set_floor(x, y, floor);
    }
}

/// `BaseGenerator.isTaken` (`BaseGenerator.java:278-299`).
#[allow(clippy::too_many_arguments)]
fn is_taken(
    wctx: &WorldCtx<'_>,
    rules: &Rules,
    wave_team: u8,
    enemy_core_radius: f32,
    block: BlockId,
    x: i32,
    y: i32,
) -> bool {
    let Some(def) = wctx.content.block(block) else {
        return true;
    };
    let wx = x as f32 * 8.0 + def.offset;
    let wy = y as f32 * 8.0 + def.offset;
    if any_enemy_core_within(wctx.ecs, rules, wave_team, wx, wy, enemy_core_radius + 8.0) {
        return true;
    }

    let offset = -(def.size - 1) / 2;
    for dx in -1..def.size + 1 {
        for dy in -1..def.size + 1 {
            if overlaps(wctx, dx + offset + x, dy + offset + y) {
                return true;
            }
        }
    }
    false
}

/// `Teams.anyEnemyCoresWithin(team, x, y, range)` (`Teams.java:73-82`) for the
/// wave team: a core of another team whose rules `protectCores`, within
/// `range` world units (`Building.within` = strict `Mathf.within`).
fn any_enemy_core_within(
    ecs: &bevy_ecs::world::World,
    rules: &Rules,
    wave_team: u8,
    x: f32,
    y: f32,
    range: f32,
) -> bool {
    let mut cores: Vec<(u32, u8, f32, f32)> = ecs
        .iter_entities()
        .filter(|entity| entity.contains::<crate::world::blocks::storage::CoreBuild>())
        .filter_map(|entity| {
            let team = entity.get::<crate::entities::comp::TeamComp>()?.team;
            let pos = entity.get::<crate::entities::comp::Pos>()?;
            Some((entity.id().index().index(), team, pos.x, pos.y))
        })
        .collect();
    cores.sort_unstable_by_key(|(index, ..)| *index);
    for (_, core_team, px, py) in cores {
        if core_team == wave_team || !rules.team_rule(core_team).protect_cores {
            continue;
        }
        let dx = px - x;
        let dy = py - y;
        if dx * dx + dy * dy < range * range {
            return true;
        }
    }
    false
}

/// `BaseGenerator.overlaps` (`BaseGenerator.java:300-305`): out of bounds,
/// `!alwaysReplace` or any darkness (`World.getDarkness`).
fn overlaps(wctx: &WorldCtx<'_>, x: i32, y: i32) -> bool {
    let grid = &*wctx.grid;
    if !grid.tiles.in_bounds(x, y) {
        return true;
    }
    let tile = grid.tile(x, y);
    let always_replace = wctx
        .content
        .block(tile.block)
        .map(|def| def.always_replace)
        .unwrap_or(false);
    if !always_replace {
        return true;
    }
    crate::world::darkness::get_darkness(grid, wctx.content, x, y) > 0.0
}

/// The port's drill kinds (`Drill`/`BeamDrill`/`BurstDrill`/`WallCrafter`).
fn is_drill(content: &ContentRegistry, block: BlockId) -> bool {
    content.block(block).is_some_and(|def| {
        matches!(
            def.kind,
            crate::content::registries::blocks::BlockKind::Drill
                | crate::content::registries::blocks::BlockKind::BeamDrill
                | crate::content::registries::blocks::BlockKind::BurstDrill
                | crate::content::registries::blocks::BlockKind::WallCrafter
        )
    })
}

/// `Block.iterateTaken(x, y, cons)` (`Block.java:968-983`): every grid position
/// in the block footprint.
fn for_each_taken(
    content: &ContentRegistry,
    block: BlockId,
    x: i32,
    y: i32,
    mut f: impl FnMut(i32, i32),
) {
    let size = content.block(block).map(|def| def.size.max(1)).unwrap_or(1);
    let offset = -(size - 1) / 2;
    for dx in 0..size {
        for dy in 0..size {
            f(dx + offset + x, dy + offset + y);
        }
    }
}

fn has_surface(content: &ContentRegistry, floor: BlockId) -> bool {
    content
        .block(floor)
        .is_some_and(crate::maps::filters::block_info::has_surface)
}

/// `Tile.circle(radius)` -> `Geometry.circle(x, y, width, height, radius)`
/// (`Geometry.java:73-82`): clamped loop bounds, strict `Mathf.within`
/// (`dst2 < radius * radius`), dx-outer/dy-inner order.
fn circle_tiles(tiles: &Tiles, cx: i32, cy: i32, radius: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let x0 = (cx - radius).max(0);
    let x1 = (cx + radius).min(tiles.width - 1);
    let y0 = (cy - radius).max(0);
    let y1 = (cy + radius).min(tiles.height - 1);
    let r2 = (radius as f32) * (radius as f32);
    for x in x0..=x1 {
        for y in y0..=y1 {
            let dx = (x - cx) as f32;
            let dy = (y - cy) as f32;
            if dx * dx + dy * dy < r2 {
                out.push((x, y));
            }
        }
    }
    out
}

/// `Tile.team()`: the building team, or derelict (`0`) for an empty tile.
fn tile_team(ecs: &bevy_ecs::world::World, tile: &Tile) -> u8 {
    tile.build
        .and_then(|entity| ecs.get::<crate::entities::comp::TeamComp>(entity))
        .map(|comp| comp.team)
        .unwrap_or(0)
}

/// `Mathf.within(t.x*8, t.y*8, cx*8, cy*8, radius)` (strict `<`).
fn tile_within(x: i32, y: i32, cx: i32, cy: i32, radius: f32) -> bool {
    let dx = (x - cx) as f32 * 8.0;
    let dy = (y - cy) as f32 * 8.0;
    dx * dx + dy * dy < radius * radius
}

/// `Angles.angle(x, y)` in degrees `[0, 360)` (`Angles.java:52-62`).
///
/// Deviation: Arc's `Mathf.atan2` fast approximation is replaced by the std
/// `atan2` (the port-wide convention, e.g. `logic::ops::angle`); the two
/// differ by less than 0.1 degree.
fn angle(x: f32, y: f32) -> f32 {
    let mut a = y.atan2(x) * 180.0 / std::f32::consts::PI;
    if a < 0.0 {
        a += 360.0;
    }
    a
}

/// `Angles.angleDist(a, b)` (`Angles.java:23-27`), the port-wide form
/// (`weapons::angle_dist`).
fn angle_dist(a: f32, b: f32) -> f32 {
    let d = (b - a).rem_euclid(360.0);
    if d > 180.0 { 360.0 - d } else { d }
}

/// `Position.angleTo` (`Angles.angle(x, y, x2, y2)`).
fn angle_to(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    angle(x2 - x1, y2 - y1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};

    fn content() -> ContentRegistry {
        let mut registry =
            create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
                .expect("content");
        // Full boot lifecycle: `post_init` assigns the requirement-derived
        // `shownPlanets` (`Block.postInit`, `Blocks.java:1335-1345`), which
        // `get_difficulty_wall` reads.
        registry.init().expect("init");
        registry.post_init().expect("post_init");
        registry
    }

    #[test]
    fn get_frac_clamps() {
        let items = [10, 20, 30];
        assert_eq!(get_frac(&items, 0.0), Some(&10));
        assert_eq!(get_frac(&items, 0.5), Some(&20));
        assert_eq!(get_frac(&items, 0.999), Some(&30));
        assert_eq!(get_frac(&items, 1.0), Some(&30), "index clamps");
        assert_eq!(get_frac(&items, -1.0), Some(&10), "index clamps");
        assert_eq!(get_frac::<i32>(&[], 0.5), None);
    }

    #[test]
    fn difficulty_wall_order() {
        let registry = content();
        let planet = registry.planet_id("serpulo");

        let walls = difficulty_walls(&registry, 1, planet);
        assert!(walls.len() >= 2, "serpulo has 1x1 walls");
        let mut last = f32::MIN;
        for wall in &walls {
            let def = registry.block(*wall).expect("wall");
            assert!(
                def.build_time >= last,
                "{} ({}) out of buildTime order after {last}",
                def.name,
                def.build_time
            );
            last = def.build_time;
            assert_eq!(def.size, 1);
            assert!(def.minfo.is_vanilla());
            assert!(!def.insulated, "{} is insulated", def.name);
            assert_eq!(def.build_visibility, BuildVisibility::Shown);
            assert!(is_wall_instance(def.kind) && def.kind != BlockKind::Door);
        }
        assert_eq!(
            registry.block(walls[0]).expect("wall").name,
            "scrap-wall",
            "cheapest serpulo wall first (scrap-wall 12s < copper-wall 18s; \
             `Wall.buildCostMultiplier = 6`, scrap overrides to 4)"
        );
        assert_eq!(
            registry
                .block(*walls.last().expect("wall"))
                .expect("wall")
                .name,
            "phase-wall",
            "priciest serpulo 1x1 wall last"
        );

        // `getFrac(difficulty * 0.91f)`.
        assert_eq!(
            get_difficulty_wall(&registry, 1, 0.0, planet),
            Some(walls[0])
        );
        let top = ((0.91 * walls.len() as f32) as i32).clamp(0, walls.len() as i32 - 1) as usize;
        assert_eq!(
            get_difficulty_wall(&registry, 1, 1.0, planet),
            Some(walls[top])
        );

        let large = difficulty_walls(&registry, 2, planet);
        assert!(!large.is_empty(), "serpulo has 2x2 walls");
        assert_eq!(
            registry.block(large[0]).expect("wall").name,
            "scrap-wall-large"
        );
        assert_eq!(
            registry
                .block(*large.last().expect("wall"))
                .expect("wall")
                .name,
            "phase-wall-large"
        );
    }

    /// The real `assets/baseparts` catalog; panics (never skips) when it is
    /// missing or does not decode, so the test cannot pass vacuously.
    fn real_bases(registry: &ContentRegistry) -> BaseRegistry {
        crate::ai::base_registry::real_catalog_for_tests(registry)
    }

    #[test]
    fn enemy_base_places_cores_and_parts() {
        use crate::game::schematics::Schematics;
        use crate::maps::generators::planet::LaunchLoadout;
        use crate::maps::planet::{SectorGenExtras, generate_sector};
        use crate::world::blocks::storage::CoreBuild;
        use crate::world::modules::ItemModule;

        let registry = content();
        let bases = real_bases(&registry);

        let extras = SectorGenExtras {
            threat: 0.5,
            has_enemy_base: true,
            has_base: false,
        };
        let generated = generate_sector("serpulo", 1, &registry, extras).expect("serpulo");
        let Some((spawn_x, spawn_y)) = generated.generator.player_spawn() else {
            panic!("serpulo spawn");
        };
        let crate::maps::planet::GeneratedSector {
            grid,
            rules,
            mut generator,
            ..
        } = generated;
        let (width, height) = (grid.tiles.width, grid.tiles.height);
        let mut sim = crate::sim::Sim::new(1, width, height, BlockId::AIR, BlockId::AIR);
        sim.grid = grid;
        sim.install_block_runtime().expect("live runtime");

        // A player core at the spawn (the launch loadout), so `is_taken`'s
        // enemy-core-radius check is exercised.
        let mut schematics = Schematics::new();
        schematics.load_loadouts(&registry);
        let shard = registry.block_id("core-shard").expect("core-shard");
        let schematic =
            schematics.all[schematics.get_default_loadout(shard).expect("basicShard")].clone();
        let copper = registry.item_id("copper").expect("copper");
        let loadout = LaunchLoadout {
            schematic,
            stacks: vec![(copper, 100)],
        };

        let mut rules = rules;
        // The enemy room centres carry the `spawn` overlay until
        // `base::generate` clears it (`tile.clearOverlay()`), so capture them
        // first.
        let enemy_rooms: Vec<(i32, i32)> = {
            let spawn_block = registry.block_id("spawn").expect("spawn");
            sim.grid
                .tiles
                .iter()
                .filter(|tile| tile.overlay == spawn_block)
                .map(|tile| (tile.x as i32, tile.y as i32))
                .collect()
        };
        let mut ctx = StructureCtx {
            sim: &mut sim,
            content: &registry,
            bases: Some(&bases),
            loadout: Some(&loadout),
            rules: &mut rules,
            player_team: 1,
            wave_team: 2,
        };
        generator.place_structures(&mut ctx);

        assert!(rules.attack_mode, "attack mode set");
        let player_core = sim
            .grid
            .tile(spawn_x, spawn_y)
            .build
            .expect("player core at the spawn");

        // Every enemy room centre (a `spawn` overlay tile) holds a team-2 core.
        assert!(!enemy_rooms.is_empty(), "enemy rooms exist");

        let item_capacity = |block: BlockId| {
            registry
                .block(block)
                .map(|def| def.item_capacity)
                .unwrap_or(0)
        };
        let mut enemy_buildings = 0usize;
        let mut enemy_cores = 0usize;
        for index in 0..sim.grid.tiles.len() {
            let tile = sim.grid.tiles.geti(index);
            let Some(entity) = tile.build else {
                continue;
            };
            let team = sim
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|comp| comp.team)
                .unwrap_or(0);
            if team != 2 {
                continue;
            }
            enemy_buildings += 1;
            let block = sim
                .ecs
                .0
                .get::<crate::ecs::BuildingComp>(entity)
                .map(|comp| comp.block)
                .expect("building");
            if sim.ecs.0.get::<CoreBuild>(entity).is_some() {
                enemy_cores += 1;
                // Core items are filled to `itemCapacity` for every item.
                let module = sim.ecs.0.get::<ItemModule>(entity).expect("core items");
                let capacity = item_capacity(block);
                for item in registry.items() {
                    assert_eq!(
                        module.get(item.id),
                        capacity,
                        "{} full at ({}, {})",
                        item.name,
                        tile.x,
                        tile.y
                    );
                }
            }
        }
        assert!(
            enemy_cores >= enemy_rooms.len(),
            "one team-2 core per enemy room: {enemy_cores} cores / {} rooms",
            enemy_rooms.len()
        );
        assert!(
            enemy_buildings > enemy_cores,
            "base parts placed outside the cores ({enemy_buildings} buildings, {enemy_cores} cores)"
        );

        // No enemy building inside the player core's build radius
        // (`is_taken` -> `anyEnemyCoresWithin`), and some outside it.
        let radius = rules.enemy_core_build_radius + 8.0;
        let player_pos = sim
            .ecs
            .0
            .get::<crate::entities::comp::Pos>(player_core)
            .map(|pos| (pos.x, pos.y))
            .expect("player core pos");
        // At least one non-core team-2 building sits outside the player core
        // radius. (`tryPlace` never places a part inside
        // `anyEnemyCoresWithin`; the wall ring is not gated by it, matching
        // Java's `wall` pass.)
        let mut outside = 0usize;
        for index in 0..sim.grid.tiles.len() {
            let tile = sim.grid.tiles.geti(index);
            let Some(entity) = tile.build else {
                continue;
            };
            let team = sim
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|comp| comp.team)
                .unwrap_or(0);
            if team != 2 || sim.ecs.0.get::<CoreBuild>(entity).is_some() {
                continue;
            }
            let pos = sim
                .ecs
                .0
                .get::<crate::entities::comp::Pos>(entity)
                .map(|pos| (pos.x, pos.y))
                .expect("enemy building pos");
            let dx = pos.0 - player_pos.0;
            let dy = pos.1 - player_pos.1;
            if dx * dx + dy * dy >= radius * radius {
                outside += 1;
            }
        }
        assert!(
            outside > 0,
            "non-core team-2 buildings outside the player core radius"
        );
    }

    #[test]
    fn post_generate_charges_batteries() {
        let registry = content();
        let battery = registry.block_id("battery").expect("battery");
        let node = registry.block_id("power-node").expect("power-node");
        let mut sim = crate::sim::Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("live runtime");
        let (battery_entity, node_a, node_b) = sim
            .with_world_ctx(|wctx| {
                wctx.set_block(3, 3, battery, 2, 0);
                wctx.set_block(8, 3, node, 2, 0);
                wctx.set_block(12, 3, node, 2, 0);
                (
                    wctx.grid.tile(3, 3).build,
                    wctx.grid.tile(8, 3).build,
                    wctx.grid.tile(12, 3).build,
                )
            })
            .expect("runtime");
        let battery_entity = battery_entity.expect("battery placed");
        let node_a = node_a.expect("node a placed");
        let node_b = node_b.expect("node b placed");

        let mut rules = Rules {
            wave_team: 2,
            ..Rules::default()
        };
        let mut ctx = StructureCtx {
            sim: &mut sim,
            content: &registry,
            bases: None,
            loadout: None,
            rules: &mut rules,
            player_team: 1,
            wave_team: 2,
        };
        post_generate(
            &mut ctx,
            &BaseGenState {
                ran: true,
                cores: Vec::new(),
            },
        );

        let status = sim
            .ecs
            .0
            .get::<crate::world::modules::PowerModule>(battery_entity)
            .map(|module| module.status)
            .expect("battery power module");
        assert_eq!(status, 1.0, "battery charged to full");

        // `PowerNode.placed` re-links the cleared nodes (`maxNodes`).
        let links = sim
            .ecs
            .0
            .get::<crate::world::modules::PowerModule>(node_a)
            .map(|module| module.links.len())
            .expect("node power module");
        let links_b = sim
            .ecs
            .0
            .get::<crate::world::modules::PowerModule>(node_b)
            .map(|module| module.links.len())
            .expect("node power module");
        assert!(
            links > 0 || links_b > 0,
            "postGenerate re-links the wave-team power nodes"
        );
    }
}
