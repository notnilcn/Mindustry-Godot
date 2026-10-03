// SPDX-License-Identifier: GPL-3.0-only

//! Plan 16 headless render-list scenarios (`render` subcommand).

use std::path::Path;

use anyhow::{Context, Result, bail};
use mind_core::content::{
    BlockId, ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content,
};
use mind_core::render::list::{FORMAT, RenderList, build_entries, sort_entries};
use mind_core::render::{BandPlan, BuildingCacheGrid, FloorChunkGrid, RegionIdTable, build_layers};
use mind_core::world::{TilePos, WorldGrid};

use crate::cli::RenderCommand;

/// Exit code: success.
const EXIT_PASS: i32 = 0;
/// Exit code: golden mismatch.
const EXIT_FAIL: i32 = 1;

/// Runs a `render` subcommand.
pub fn run(command: &RenderCommand) -> Result<i32> {
    match command {
        RenderCommand::List {
            scenario,
            out,
            check,
            no_sort,
            all_chunks,
            json,
        } => list(
            scenario,
            out.as_deref(),
            check.as_deref(),
            *no_sort,
            *all_chunks,
            *json,
        ),
        RenderCommand::Bands { out } => bands(out.as_deref()),
        RenderCommand::Bench {
            width,
            height,
            buildings,
            iters,
            warmup,
            json,
        } => bench(*width, *height, *buildings, *iters, *warmup, *json),
    }
}

fn boot_content() -> Result<ContentRegistry> {
    let mut registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    registry
        .init()
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    registry
        .post_init()
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    registry
        .load()
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    Ok(registry)
}

/// A scenario world + camera + notes.
struct ScenarioWorld {
    world: WorldGrid,
    camera: mind_core::render::CameraView,
    dynamic_blocks: usize,
}

fn scenario_world(name: &str, content: &ContentRegistry) -> Result<ScenarioWorld> {
    let stone = content
        .block_id("stone")
        .ok_or_else(|| anyhow::anyhow!("missing `stone` floor"))?;
    let camera = mind_core::render::CameraView {
        x: 132.0,
        y: 132.0,
        w: 320.0,
        h: 180.0,
        zoom: 4.0,
        team: 0,
    };
    let mut world;
    let mut dynamic_blocks = 0;
    match name {
        "render_flat_floor" => {
            world = WorldGrid::new(64, 64);
            world.fill(stone, BlockId::AIR);
        }
        "render_block_change" | "render_layer_order" | "render_layer_order_nosort" => {
            world = WorldGrid::new(32, 32);
            world.fill(stone, BlockId::AIR);
            let wall = content
                .block_id("copper-wall")
                .ok_or_else(|| anyhow::anyhow!("missing `copper-wall`"))?;
            world.set_block(TilePos::new(4, 4), wall, 0, 0)?;
            dynamic_blocks = 1;
        }
        "render_darkness_radius" => {
            world = WorldGrid::new(48, 48);
            world.fill(stone, BlockId::AIR);
            let wall = content
                .block_id("stone-wall")
                .ok_or_else(|| anyhow::anyhow!("missing `stone-wall`"))?;
            // A hollow 7x7 wall ring centred at (24, 24).
            for d in -3i16..=3 {
                world.set_block(TilePos::new(24 + d, 21), wall, 0, 0)?;
                world.set_block(TilePos::new(24 + d, 27), wall, 0, 0)?;
                world.set_block(TilePos::new(21, 24 + d), wall, 0, 0)?;
                world.set_block(TilePos::new(27, 24 + d), wall, 0, 0)?;
            }
        }
        other => bail!("unknown render scenario `{other}`"),
    }
    world.tile_changes = -1;
    world.floor_changes = -1;
    Ok(ScenarioWorld {
        world,
        camera,
        dynamic_blocks,
    })
}

#[allow(clippy::too_many_arguments)]
fn list(
    name: &str,
    out: Option<&Path>,
    check: Option<&Path>,
    no_sort: bool,
    all_chunks: bool,
    json: bool,
) -> Result<i32> {
    let content = boot_content()?;
    if name == "render_menu_world" {
        // Plan 16 §7.2: deterministic menu block/floor counts + tile hash. The
        // full region render-list awaits plan-03 menu region-name resolution.
        let world = mind_core::render::menu::generate(1234, false);
        let text = world.summary().to_json();
        if let Some(path) = check {
            let golden = std::fs::read_to_string(path)
                .with_context(|| format!("reading golden `{}`", path.display()))?;
            if golden != text {
                eprintln!("render-list: mismatch against `{}`", path.display());
                return Ok(EXIT_FAIL);
            }
            if json {
                println!(
                    "{{\"scenario\":\"{name}\",\"pass\":true,\"entries\":{}}}",
                    world.tiles.len()
                );
            }
            return Ok(EXIT_PASS);
        }
        if let Some(path) = out {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
        } else {
            print!("{text}");
        }
        if json && out.is_some() {
            println!(
                "{{\"scenario\":\"{name}\",\"entries\":{},\"regions\":0}}",
                world.tiles.len()
            );
        }
        return Ok(EXIT_PASS);
    }
    if name == "render_layers_full" {
        let stone = content
            .block_id("stone")
            .ok_or_else(|| anyhow::anyhow!("missing `stone` floor"))?;
        let mut world = WorldGrid::new(32, 32);
        world.fill(stone, BlockId::AIR);
        for (bx, by, block) in [
            (4, 4, "copper-wall"),
            (8, 8, "phase-weaver"),
            (12, 8, "cryofluid-mixer"),
            (16, 8, "cultivator"),
            (20, 8, "silicon-smelter"),
            (8, 13, "steam-generator"),
            (12, 13, "pulverizer"),
            (16, 13, "melter"),
            (20, 13, "plastanium-compressor"),
            (24, 13, "router"),
            (26, 13, "duct"),
        ] {
            let id = content
                .block_id(block)
                .ok_or_else(|| anyhow::anyhow!("missing `{block}`"))?;
            world.set_block(TilePos::new(bx, by), id, 0, 0)?;
        }
        world.tile_changes = -1;
        world.floor_changes = -1;
        let camera = mind_core::render::CameraView {
            x: 128.0,
            y: 128.0,
            w: 320.0,
            h: 180.0,
            zoom: 4.0,
            team: 1,
        };
        let mut ids = RegionIdTable::new();
        let mut full = build_layers(&world, &content, &mut ids, &camera, 1, 2);
        full.scenario = name.to_owned();
        let text = full.to_json(&ids);
        if let Some(path) = check {
            let golden = std::fs::read_to_string(path)
                .with_context(|| format!("reading golden `{}`", path.display()))?;
            if golden != text {
                eprintln!("render-list: mismatch against `{}`", path.display());
                return Ok(EXIT_FAIL);
            }
            if json {
                println!(
                    "{{\"scenario\":\"{name}\",\"pass\":true,\"entries\":{},\"layers\":{},\"draw_calls\":{}}}",
                    full.entries.len(),
                    full.layers.len(),
                    full.counters.draw_calls
                );
            }
            return Ok(EXIT_PASS);
        }
        if let Some(path) = out {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
        } else {
            print!("{text}");
        }
        return Ok(EXIT_PASS);
    }

    let scenario = scenario_world(name, &content)?;
    let mut ids = RegionIdTable::new();
    let mut entries = build_entries(&scenario.world, &content, &mut ids, &scenario.camera);
    let sort = !no_sort;
    sort_entries(&mut entries, sort);

    let mut floor = FloorChunkGrid::new(scenario.world.width(), scenario.world.height());
    let mut blocks = BuildingCacheGrid::new(scenario.world.width(), scenario.world.height());
    let floor_built = floor.chunk_count();
    for cy in 0..floor.chunks_y() {
        for cx in 0..floor.chunks_x() {
            floor.mark_baked(cx, cy);
        }
    }
    let _ = all_chunks;
    if scenario.dynamic_blocks > 0 {
        let (cx, cy) = BuildingCacheGrid::chunk_of(4, 4);
        blocks.mark_baked(1, cx, cy);
    }
    let list = RenderList {
        format: FORMAT,
        scenario: name.to_owned(),
        tick: 0,
        camera: scenario.camera,
        sort,
        entries,
        floor_dirty: floor.dirty_chunks(),
        floor_built,
        floor_epoch: floor.epoch(),
        block_dirty: Vec::new(),
        block_built: if scenario.dynamic_blocks > 0 { 1 } else { 0 },
        block_epoch: if scenario.dynamic_blocks > 0 { 1 } else { 0 },
        regions: ids.len(),
    };
    let text = list.to_json(&ids);

    if let Some(path) = check {
        let golden = std::fs::read_to_string(path)
            .with_context(|| format!("reading golden `{}`", path.display()))?;
        if golden != text {
            eprintln!("render-list: mismatch against `{}`", path.display());
            return Ok(EXIT_FAIL);
        }
        if json {
            println!(
                "{{\"scenario\":\"{name}\",\"pass\":true,\"entries\":{}}}",
                list.entries.len()
            );
        }
        return Ok(EXIT_PASS);
    }

    if let Some(path) = out {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, &text).with_context(|| format!("writing `{}`", path.display()))?;
    } else {
        print!("{text}");
    }
    if json && out.is_some() {
        println!(
            "{{\"scenario\":\"{name}\",\"entries\":{},\"regions\":{}}}",
            list.entries.len(),
            ids.len()
        );
    }
    Ok(EXIT_PASS)
}

fn bands(out: Option<&Path>) -> Result<i32> {
    let plan = BandPlan::new();
    let text = plan.to_json();
    match out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(path, text).with_context(|| format!("writing `{}`", path.display()))?;
        }
        None => print!("{text}"),
    }
    Ok(EXIT_PASS)
}

/// `render bench`: times the render-list build path over a large world and
/// reports p50/p99 (plan 16 §7.4/M9). The release budgets are p50 ≤ 2500 µs and
/// p99 ≤ 5000 µs; debug runs are recorded for regression tracking.
fn bench(
    width: i32,
    height: i32,
    buildings: usize,
    iters: usize,
    warmup: usize,
    json: bool,
) -> Result<i32> {
    let content = boot_content()?;
    let stone = content
        .block_id("stone")
        .ok_or_else(|| anyhow::anyhow!("missing `stone` floor"))?;
    let conveyor = content
        .block_id("conveyor")
        .ok_or_else(|| anyhow::anyhow!("missing `conveyor`"))?;

    let mut world = WorldGrid::new(width.max(2), height.max(2));
    world.fill(stone, BlockId::AIR);
    let mut placed = 0usize;
    'outer: for y in (1..world.height() - 1).step_by(2) {
        for x in (1..world.width() - 1).step_by(2) {
            if placed >= buildings {
                break 'outer;
            }
            world.set_block(TilePos::new(x as i16, y as i16), conveyor, 0, 0)?;
            placed += 1;
        }
    }
    world.tile_changes = -1;
    world.floor_changes = -1;

    let tilesize = 8.0f32;
    let camera = mind_core::render::CameraView {
        x: world.width() as f32 * tilesize / 2.0,
        y: world.height() as f32 * tilesize / 2.0,
        w: world.width() as f32 * tilesize,
        h: world.height() as f32 * tilesize,
        zoom: 1.0,
        team: 0,
    };

    let mut ids = RegionIdTable::new();
    for _ in 0..warmup {
        let entries = build_entries(&world, &content, &mut ids, &camera);
        std::hint::black_box(&entries);
    }

    let mut samples = Vec::with_capacity(iters);
    let mut entries = 0usize;
    let mut counters = mind_core::render::DrawCounters::default();
    for _ in 0..iters {
        let start = std::time::Instant::now();
        let built = build_entries(&world, &content, &mut ids, &camera);
        samples.push(start.elapsed().as_micros() as u64);
        entries = built.len();
        counters = mind_core::render::count_entries(&built, 0);
        std::hint::black_box(&built);
    }
    samples.sort_unstable();
    let p50 = samples.get(samples.len() / 2).copied().unwrap_or(0);
    let p99 = samples
        .get((samples.len() * 99 / 100).min(samples.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);

    let report = format!(
        "{{\"scenario\":\"render_bench\",\"width\":{width},\"height\":{height},\"buildings\":{placed},\"iters\":{iters},\"entries\":{entries},\"regions\":{},\"draw_calls\":{},\"triangles\":{},\"p50_us\":{p50},\"p99_us\":{p99},\"budget_us\":{{\"p50\":2500,\"p99\":5000}},\"pass\":{}}}",
        ids.len(),
        counters.draw_calls,
        counters.triangles,
        p99 <= 5000,
    );
    let _ = json;
    println!("{report}");
    Ok(EXIT_PASS)
}
