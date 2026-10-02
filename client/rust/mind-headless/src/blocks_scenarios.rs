// SPDX-License-Identifier: GPL-3.0-only

//! Plan 07 headless block/building scenarios (`blocks` subcommand).

use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{Result, bail};
use mind_core::content::BlockId;
use mind_core::world::BuildHarness;
use mind_core::world::config::ConfigValue;

use crate::cli::BlocksCommand;

/// Runs a `blocks` subcommand.
pub fn run(command: &BlocksCommand) -> Result<()> {
    match command {
        BlocksCommand::Audit { json } => audit(*json),
        BlocksCommand::Scenario { name, json } => scenario(name, *json),
        BlocksCommand::Bench {
            buildings,
            ticks,
            json,
        } => bench(*buildings, *ticks, *json),
    }
}

fn print_json(value: &serde_json::Value, json: bool, pretty: bool) {
    if json || pretty {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    }
}

fn audit(json: bool) -> Result<()> {
    let harness = BuildHarness::new(4, 4, 0);
    let mut families: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut rotate = 0usize;
    let mut consumers = 0usize;
    for inst in harness.table().iter() {
        *families.entry(inst.kind_data.family_name()).or_insert(0) += 1;
        if inst.rotate {
            rotate += 1;
        }
        if !inst.consumers.is_empty() {
            consumers += 1;
        }
    }
    let report = serde_json::json!({
        "scenario": "blocks_audit",
        "blocks": harness.table().len(),
        "families": families,
        "rotating": rotate,
        "with_consumers": consumers,
    });
    print_json(&report, json, !json);
    Ok(())
}

fn scenario(name: &str, json: bool) -> Result<()> {
    let mut harness = BuildHarness::new(32, 32, 7);
    match name {
        "place_construct_destroy" => {
            let wall = block(&harness, "copper-wall")?;
            let placed = harness.place(4, 4, wall, 0, false);
            let construct = harness.block_at(4, 4);
            harness.construct_tick(10_000.0);
            let finished = harness.block_at(4, 4);
            let checksum_built = harness.checksum_hex();
            let broken = harness.break_block(4, 4, true);
            let final_block = harness.block_at(4, 4);
            let checksum_final = harness.checksum_hex();
            let pass = placed
                && finished == wall
                && broken
                && final_block == BlockId::AIR
                && harness.events.len() == 4;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "construct_block": name_of(&harness, construct),
                "finished_block": name_of(&harness, finished),
                "final_block": name_of(&harness, final_block),
                "events": harness.events.len(),
                "checksum_built": checksum_built,
                "checksum_final": checksum_final,
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "multiblock_cover_clear" => {
            let large = block(&harness, "copper-wall-large")?;
            let placed = harness.place(5, 5, large, 0, true);
            let center = harness.build_at(5, 5);
            let linked = (5..=6).all(|x| (5..=6).all(|y| harness.build_at(x, y) == center));
            let broken = harness.break_block(6, 6, true);
            let cleared = (5..=6).all(|x| (5..=6).all(|y| harness.block_at(x, y) == BlockId::AIR));
            let pass = placed && center.is_some() && linked && broken && cleared;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "linked": linked,
                "cleared": cleared,
                "checksum": harness.checksum_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "spawn_update" => {
            let wall = block(&harness, "copper-wall")?;
            for i in 0..20 {
                let _ = harness.place(2 + i % 10, 2 + i / 10, wall, 0, true);
            }
            for _ in 0..600 {
                harness.tick();
            }
            let report = serde_json::json!({
                "scenario": name,
                "pass": true,
                "buildings": harness.building_count(),
                "ticks": 600,
                "checksum": harness.checksum_hex(),
            });
            print_json(&report, json, !json);
        }
        "config_roundtrip" => {
            let source = block(&harness, "item-source")?;
            let placed = harness.place(4, 4, source, 0, true);
            let copper = harness.content().item_id("copper");
            let configured = copper
                .map(|item| harness.configure(4, 4, ConfigValue::Item(item)))
                .unwrap_or(false);
            let read_back = harness
                .build_at(4, 4)
                .map(|e| mind_core::world::config::read_config(&harness.world, e));
            let pass = placed && configured && matches!(read_back, Some(ConfigValue::Item(_)));
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "config": format!("{read_back:?}"),
                "checksum": harness.checksum_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "proximity_multiblock" => {
            let wall = block(&harness, "copper-wall")?;
            let _ = harness.place(4, 4, wall, 0, true);
            let _ = harness.place(5, 4, wall, 0, true);
            let _ = harness.place(6, 4, wall, 0, true);
            let middle = harness.build_at(5, 4);
            let linked = middle
                .and_then(|e| harness.world.get::<mind_core::entities::comp::Building>(e))
                .map(|b| b.proximity.len())
                .unwrap_or(0);
            let pass = linked >= 2;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "middle_proximity": linked,
                "checksum": harness.checksum_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "wall_door" => {
            let wall = block(&harness, "copper-wall")?;
            let door = block(&harness, "door")?;
            let wall_placed = harness.place(4, 4, wall, 0, true);
            let door_placed = harness.place(5, 4, door, 0, false);
            harness.construct_tick(10_000.0);
            let door_built = harness.block_at(5, 4) == door;
            let opened = harness.configure(5, 4, ConfigValue::Bool(true));
            let read_back = harness
                .build_at(5, 4)
                .map(|e| mind_core::world::config::read_config(&harness.world, e));
            let closed = harness.configure(5, 4, ConfigValue::Bool(false));
            let pass = wall_placed
                && door_placed
                && door_built
                && opened
                && closed
                && matches!(read_back, Some(ConfigValue::Bool(true)));
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "door_built": door_built,
                "door_open_roundtrip": matches!(read_back, Some(ConfigValue::Bool(true))),
                "checksum": harness.checksum_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        other => bail!("unknown blocks scenario `{other}`"),
    }
    Ok(())
}

fn block(harness: &BuildHarness, name: &str) -> Result<BlockId> {
    harness
        .content()
        .block_id(name)
        .ok_or_else(|| anyhow::anyhow!("unknown block `{name}`"))
}

fn name_of(harness: &BuildHarness, block: BlockId) -> String {
    harness
        .content()
        .block(block)
        .map(|def| def.name.clone())
        .unwrap_or_else(|| format!("?{}", block.raw()))
}

fn bench(buildings: usize, ticks: u64, json: bool) -> Result<()> {
    let mut harness = BuildHarness::new(64, 64, 7);
    let wall = block(&harness, "copper-wall")?;
    let mut placed = 0usize;
    'outer: for y in 0..64 {
        for x in 0..64 {
            if placed >= buildings {
                break 'outer;
            }
            if harness.place(x, y, wall, 0, true) {
                placed += 1;
            }
        }
    }
    let start = Instant::now();
    for _ in 0..ticks {
        harness.tick();
    }
    let elapsed = start.elapsed();
    let per_tick_us = elapsed.as_secs_f64() * 1_000_000.0 / ticks as f64;
    let report = serde_json::json!({
        "scenario": "blocks_bench",
        "buildings": placed,
        "ticks": ticks,
        "total_ms": elapsed.as_secs_f64() * 1000.0,
        "per_tick_us": per_tick_us,
        "checksum": harness.checksum_hex(),
    });
    print_json(&report, json, !json);
    Ok(())
}
