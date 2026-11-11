// SPDX-License-Identifier: GPL-3.0-only

//! `world` command bodies: tile ops, generation, filters and multiblock placement (plan 06).
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::content::boot_content;
use super::*;

/// Deterministic hooks that spawn a bare building entity (plan 06 M3 oracle
/// until plan 07 provides the real building runtime).
pub(super) struct BareBuildingHooks;

impl mind_core::world::WorldHooks for BareBuildingHooks {
    fn new_building(
        &self,
        world: &mut bevy_ecs::world::World,
        request: mind_core::world::NewBuilding,
    ) -> Option<bevy_ecs::entity::Entity> {
        use mind_core::ecs::{BuildingComp, EntitySeq, TeamId};
        Some(
            world
                .spawn((
                    EntitySeq(0),
                    BuildingComp {
                        pos: TilePos::new(request.x, request.y),
                        block: request.block,
                        team: TeamId(request.team),
                        rot: request.rot,
                    },
                ))
                .id(),
        )
    }
}

pub(super) fn cmd_world_tile_ops(
    seed: u64,
    width: i32,
    height: i32,
    ops: u64,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::determinism::{RngStream, SimRng};
    use mind_core::world::ops::{WorldCtx, WorldEventLog};
    use mind_core::world::{NoopRenderHooks, NoopWorldHooks, WorldGrid};

    if width <= 0 || height <= 0 {
        return Err(anyhow!("--width and --height must be positive"));
    }
    let content = boot_content()?;
    let mut grid = WorldGrid::new(width, height);
    let mut ecs = bevy_ecs::world::World::new();
    let hooks = NoopWorldHooks;
    let render = NoopRenderHooks;
    let mut log = WorldEventLog::default();
    let mut rng = SimRng::new(seed);

    let pick = |names: &[&str]| -> Vec<mind_core::content::BlockId> {
        names
            .iter()
            .filter_map(|name| content.block_id(name))
            .collect()
    };
    let floors = pick(&["stone", "sand-floor", "grass", "dirt", "ice", "moss"]);
    let overlays = pick(&["ore-copper", "ore-lead", "air"]);
    let walls = pick(&["stone-wall", "sand-wall", "air"]);

    {
        let mut ctx = WorldCtx {
            grid: &mut grid,
            content: &content,
            ecs: &mut ecs,
            hooks: &hooks,
            render: &render,
            log: &mut log,
        };
        for _ in 0..ops {
            let x = rng.random(RngStream::MapGen, width) as i16;
            let y = rng.random(RngStream::MapGen, height) as i16;
            match rng.random(RngStream::MapGen, 4) {
                0 => {
                    if !floors.is_empty() {
                        let floor =
                            floors[rng.random(RngStream::MapGen, floors.len() as i32) as usize];
                        ctx.set_floor(x, y, floor);
                    }
                }
                1 => {
                    if !overlays.is_empty() {
                        let overlay =
                            overlays[rng.random(RngStream::MapGen, overlays.len() as i32) as usize];
                        ctx.set_overlay(x, y, overlay);
                    }
                }
                2 => {
                    if !walls.is_empty() {
                        let wall =
                            walls[rng.random(RngStream::MapGen, walls.len() as i32) as usize];
                        ctx.set_block(x, y, wall, 0, 0);
                    }
                }
                _ => ctx.set_air(x, y),
            }
        }
    }

    // Counter == event-count invariant (M0 §7b).
    let tile_events = log.tile_changes.len() as i32;
    let floor_events = log.floor_changes.len() as i32;
    let counters_ok =
        grid.tile_changes == 1 + tile_events && grid.floor_changes == 1 + floor_events;

    // Histograms (BTreeMap → deterministic).
    let mut blocks: BTreeMap<String, u64> = BTreeMap::new();
    let mut floors_hist: BTreeMap<String, u64> = BTreeMap::new();
    for tile in grid.tiles.iter() {
        *blocks
            .entry(tile.block_name(&content).to_owned())
            .or_default() += 1;
        *floors_hist
            .entry(
                content
                    .block(tile.floor)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "air".to_owned()),
            )
            .or_default() += 1;
    }

    let report = serde_json::json!({
        "format": 1,
        "generator": "tile_ops",
        "seed": seed,
        "width": width,
        "height": height,
        "ops": ops,
        "tile_changes": grid.tile_changes,
        "floor_changes": grid.floor_changes,
        "tile_events": tile_events,
        "floor_events": floor_events,
        "counters_ok": counters_ok,
        "counts": { "blocks": blocks, "floors": floors_hist },
    });
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = dump {
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        println!("{text}");
    }
    if counters_ok {
        Ok(EXIT_PASS)
    } else {
        log::error!(
            "counter/event mismatch: tile_changes={} tile_events={} floor_changes={} floor_events={}",
            grid.tile_changes,
            tile_events,
            grid.floor_changes,
            floor_events
        );
        Ok(EXIT_FAIL)
    }
}

pub(super) fn cmd_world_bench_gen(
    generator: &str,
    seed: u64,
    width: i32,
    height: i32,
    iters: u64,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::maps::generators::{SimplexGenerator, WorldGenerator};
    use mind_core::world::{WorldGrid, WorldParams};

    if width <= 0 || height <= 0 || iters == 0 {
        return Err(anyhow!("--width/--height/--iters must be positive"));
    }
    let content = boot_content()?;
    let params = WorldParams {
        seed_offset: seed,
        width,
        height,
        ..WorldParams::default()
    };
    let mut grid = WorldGrid::new(width, height);
    let mut samples = time_runs(iters as usize, || {
        match generator {
            "simplex" => {
                let mut g = SimplexGenerator::new(seed);
                g.generate(&mut grid.tiles, &params, &content);
            }
            "tantros" | "blank" | "serpulo" | "erekir" | "asteroid" => {
                let mut g = make_planet_generator(generator, seed)?;
                g.generate(&mut grid.tiles, &params, &content);
            }
            other => return Err(anyhow!("unknown generator `{other}`")),
        }
        Ok(())
    })?;
    samples.sort_unstable();
    let p = |q: f64| -> f64 {
        let index = ((samples.len() as f64 - 1.0) * q).round() as usize;
        samples[index] as f64 / 1_000_000.0
    };
    let report = serde_json::json!({
        "format": 1,
        "generator": generator,
        "seed": seed,
        "width": width,
        "height": height,
        "iters": iters,
        "p50_ms": p(0.50),
        "p95_ms": p(0.95),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "world gen bench: generator={generator} {width}x{height} iters={iters} p50={:.2}ms p95={:.2}ms",
            p(0.50),
            p(0.95)
        );
    }
    Ok(EXIT_PASS)
}

/// Constructs a vanilla planet generator for the harness `world gen` command.
pub(super) fn make_planet_generator(
    planet: &str,
    seed: u64,
) -> anyhow::Result<Box<dyn mind_core::maps::generators::WorldGenerator>> {
    use mind_core::maps::generators::{BlankPlanetGenerator, WorldGenerator};
    let generator: Box<dyn WorldGenerator> = match planet {
        "blank" => Box::new(BlankPlanetGenerator::new(0)),
        "tantros" => Box::new(mind_core::maps::planet::TantrosPlanetGenerator::new()),
        "asteroid" => Box::new(mind_core::maps::planet::AsteroidGenerator::new(seed as i32)),
        "erekir" => Box::new(mind_core::maps::planet::ErekirPlanetGenerator::new()),
        "serpulo" => Box::new(mind_core::maps::planet::SerpuloPlanetGenerator::new()),
        other => return Err(anyhow!("unknown planet generator `{other}`")),
    };
    Ok(generator)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn cmd_world_gen(
    generator: &str,
    planet: Option<&str>,
    sector: u32,
    seed: u64,
    width: i32,
    height: i32,
    iters: u64,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::content::BlockId;
    use mind_core::determinism::{Checksum, Hasher};
    use mind_core::maps::generators::{SimplexGenerator, WorldGenerator};
    use mind_core::world::{WorldGrid, WorldParams};

    if width <= 0 || height <= 0 {
        return Err(anyhow!("--width and --height must be positive"));
    }
    if iters == 0 {
        return Err(anyhow!("--iters must be positive"));
    }
    let content = boot_content()?;
    let params = WorldParams {
        seed_offset: seed,
        width,
        height,
        ..WorldParams::default()
    };
    let mut grid = WorldGrid::new(width, height);

    // `--planet` selects a vanilla planet generator; otherwise `--generator`
    // picks the flat/simplex helper (`--generator planet --planet X` also works).
    let planet = planet.or_else(|| (generator == "planet").then_some("serpulo"));
    for _ in 0..iters {
        if let Some(planet) = planet {
            let mut planet_gen: Box<dyn WorldGenerator> = make_planet_generator(planet, seed)?;
            planet_gen.generate(&mut grid.tiles, &params, &content);
        } else {
            match generator {
                "simplex" => {
                    let mut generator_impl = SimplexGenerator::new(seed);
                    generator_impl.generate(&mut grid.tiles, &params, &content);
                }
                "flat" | "blank" => {
                    let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
                    for index in 0..grid.tiles.len() {
                        grid.tiles.geti_mut(index).floor = stone;
                    }
                }
                other => return Err(anyhow!("unknown generator `{other}`")),
            }
        }

        // `iters > 1` is a determinism check (same output every pass); the
        // generators overwrite tile data, so no reset is required.
    }

    let mut hasher = Hasher::new();
    hasher.write_u32(width as u32);
    hasher.write_u32(height as u32);
    let mut floors: BTreeMap<String, u64> = BTreeMap::new();
    let mut blocks: BTreeMap<String, u64> = BTreeMap::new();
    let mut overlays: BTreeMap<String, u64> = BTreeMap::new();
    for index in 0..grid.tiles.len() {
        let tile = grid.tiles.geti(index);
        hasher.write_u16(tile.block.raw());
        hasher.write_u16(tile.floor.raw());
        hasher.write_u16(tile.overlay.raw());
        for (map, id) in [
            (&mut floors, tile.floor),
            (&mut blocks, tile.block),
            (&mut overlays, tile.overlay),
        ] {
            *map.entry(
                content
                    .block(id)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "air".to_owned()),
            )
            .or_default() += 1;
        }
    }
    let checksum = Checksum(hasher.finish().value()).to_hex();
    let generator_label = if planet.is_some() {
        "planet"
    } else {
        generator
    };

    let report = serde_json::json!({
        "format": 1,
        "generator": generator_label,
        "planet": planet,
        "sector": sector,
        "seed": seed,
        "width": width,
        "height": height,
        "checksum": checksum,
        "counts": { "floors": floors, "blocks": blocks, "overlays": overlays },
    });
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = dump {
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        println!("{text}");
    }
    Ok(EXIT_PASS)
}

pub(super) fn cmd_world_filters(
    seed: u64,
    width: i32,
    height: i32,
    stack: &str,
    order: &str,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::determinism::{Checksum, Hasher, SimRng};
    use mind_core::maps::filters::{FilterRegistry, apply_stack};
    use mind_core::world::WorldGrid;

    if width <= 0 || height <= 0 {
        return Err(anyhow!("--width and --height must be positive"));
    }
    let content = boot_content()?;
    let registry = FilterRegistry::vanilla();

    let mut tags: Vec<String> = stack
        .split(',')
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty())
        .collect();
    if order == "reverse" {
        tags.reverse();
    } else if order != "forward" {
        return Err(anyhow!("--order must be `forward` or `reverse`"));
    }

    let mut filters = Vec::with_capacity(tags.len());
    for tag in &tags {
        let payload = serde_json::json!({ "class": tag });
        let filter = registry
            .from_json(&content, &payload)
            .with_context(|| format!("building filter `{tag}`"))?;
        filters.push(filter);
    }

    // Deterministic base grid: stone floor, scattered walls.
    let mut grid = WorldGrid::new(width, height);
    let stone = content
        .block_id("stone")
        .unwrap_or(mind_core::content::BlockId::AIR);
    let sand = content
        .block_id("sand-floor")
        .unwrap_or(mind_core::content::BlockId::AIR);
    let wall = content
        .block_id("stone-wall")
        .unwrap_or(mind_core::content::BlockId::AIR);
    let mut base_rng = SimRng::new(seed);
    let n = (width * height) as usize;
    for index in 0..n {
        let noise_wall = base_rng.chance(mind_core::determinism::RngStream::MapGen, 0.05);
        let noise_sand = base_rng.chance(mind_core::determinism::RngStream::MapGen, 0.3);
        let tile = grid.tiles.geti_mut(index);
        tile.floor = if noise_sand { sand } else { stone };
        if noise_wall {
            tile.block = wall;
        }
    }

    let mut filter_rng = SimRng::new(seed ^ 0x5EED);
    apply_stack(&mut grid.tiles, &mut filters, &content, &mut filter_rng);

    // FNV over the resulting tiles (order contract: flat `x + y*width`).
    let mut hasher = Hasher::new();
    hasher.write_u32(width as u32);
    hasher.write_u32(height as u32);
    let mut floors: BTreeMap<String, u64> = BTreeMap::new();
    let mut blocks: BTreeMap<String, u64> = BTreeMap::new();
    for index in 0..n {
        let tile = grid.tiles.geti(index);
        hasher.write_u16(tile.block.raw());
        hasher.write_u16(tile.floor.raw());
        hasher.write_u16(tile.overlay.raw());
        *floors
            .entry(
                content
                    .block(tile.floor)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "air".to_owned()),
            )
            .or_default() += 1;
        *blocks
            .entry(
                content
                    .block(tile.block)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "air".to_owned()),
            )
            .or_default() += 1;
    }
    let checksum = Checksum(hasher.finish().value()).to_hex();

    let report = serde_json::json!({
        "format": 1,
        "generator": "filters",
        "seed": seed,
        "width": width,
        "height": height,
        "stack": tags,
        "order": order,
        "checksum": checksum,
        "counts": { "floors": floors, "blocks": blocks },
    });
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = dump {
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        println!("{text}");
    }
    Ok(EXIT_PASS)
}

pub(super) fn cmd_world_multiblock(
    size: i32,
    block_name: &str,
    dump: Option<&Path>,
    json: bool,
) -> anyhow::Result<i32> {
    use mind_core::world::ops::{WorldCtx, WorldEventLog};
    use mind_core::world::{NoopRenderHooks, WorldGrid};

    if size < 1 {
        return Err(anyhow!("--size must be positive"));
    }
    let content = boot_content()?;
    let block = content
        .block_id(block_name)
        .ok_or_else(|| anyhow!("unknown block `{block_name}`"))?;
    let actual_size = content.block(block).map(|def| def.size).unwrap_or(1);
    if actual_size != size {
        return Err(anyhow!(
            "block `{block_name}` has size {actual_size}, expected {size}"
        ));
    }

    let mut grid = WorldGrid::new(8, 8);
    let mut ecs = bevy_ecs::world::World::new();
    let hooks = BareBuildingHooks;
    let render = NoopRenderHooks;
    let mut log = WorldEventLog::default();

    let mut linked = true;
    let mut first_overlap_cleared = true;
    let mut broke_air = true;

    {
        let mut ctx = WorldCtx {
            grid: &mut grid,
            content: &content,
            ecs: &mut ecs,
            hooks: &hooks,
            render: &render,
            log: &mut log,
        };
        // First multiblock at (1,1).
        ctx.set_block(1, 1, block, 0, 0);
        let first = ctx.grid.tiles.get(1, 1).build;
        let offset = -(size - 1) / 2;
        for dx in 0..size {
            for dy in 0..size {
                let tile = ctx.grid.tiles.get(1 + offset + dx, 1 + offset + dy);
                if tile.build != first || tile.block != block {
                    linked = false;
                }
            }
        }
        // Overlapping second multiblock at (2,2) must clear the first.
        ctx.set_block(2, 2, block, 0, 0);
        if ctx.grid.tiles.get(0, 0).block != mind_core::content::BlockId::AIR {
            first_overlap_cleared = false;
        }
        // Break the (center) block: its whole footprint returns to air.
        ctx.remove_block(2, 2);
        for dx in 0..size {
            for dy in 0..size {
                if ctx.grid.tiles.get(2 + offset + dx, 2 + offset + dy).block
                    != mind_core::content::BlockId::AIR
                {
                    broke_air = false;
                }
            }
        }
    }

    let pass = linked && first_overlap_cleared && broke_air;
    let report = serde_json::json!({
        "format": 1,
        "generator": "multiblock",
        "block": block_name,
        "size": size,
        "linked": linked,
        "first_overlap_cleared": first_overlap_cleared,
        "break_clears_footprint": broke_air,
        "pass": pass,
    });
    let text = serde_json::to_string_pretty(&report)?;
    if let Some(path) = dump {
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    }
    if json || dump.is_none() {
        println!("{text}");
    }
    Ok(exit_code(pass))
}
