// SPDX-License-Identifier: GPL-3.0-only

//! Plan 16 headless render-list scenarios (`render` subcommand).

use std::path::Path;

use anyhow::{Context, Result, bail};
use mind_core::content::{
    BlockId, ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content,
};
use mind_core::render::list::{FORMAT, RenderList, build_entries, sort_entries};
use mind_core::render::{BandPlan, BuildingCacheGrid, FloorChunkGrid, RegionIdTable};
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
        "render_block_change" | "render_layer_order" => {
            world = WorldGrid::new(32, 32);
            world.fill(stone, BlockId::AIR);
            let wall = content
                .block_id("copper-wall")
                .ok_or_else(|| anyhow::anyhow!("missing `copper-wall`"))?;
            world.set_block(TilePos::new(4, 4), wall, 0, 0)?;
            dynamic_blocks = 1;
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
