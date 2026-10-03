// SPDX-License-Identifier: GPL-3.0-only

//! Plan 07 headless block/building scenarios (`blocks` subcommand).

use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{Result, bail};
use mind_core::content::BlockId;
use mind_core::world::BuildHarness;
use mind_core::world::config::ConfigValue;
use mind_core::world::modules::{ItemModule, LiquidModule, PowerModule};

use crate::cli::BlocksCommand;

/// Runs a `blocks` subcommand.
pub fn run(command: &BlocksCommand) -> Result<()> {
    match command {
        BlocksCommand::Audit { json } => audit(*json),
        BlocksCommand::Scenario { name, json } => scenario(name, *json),
        BlocksCommand::Bench {
            profile,
            buildings,
            ticks,
            json,
        } => bench(profile, *buildings, *ticks, *json),
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
        "logistics_smoke" => {
            let copper = harness
                .content()
                .item_id("copper")
                .ok_or_else(|| anyhow::anyhow!("copper item missing"))?;
            harness.register_behavior(
                "item-source",
                std::sync::Arc::new(mind_core::world::fixtures::logistics::SourceBehavior {
                    item: copper,
                    per_tick: 1,
                }),
            );
            let source = block(&harness, "item-source")?;
            let conveyor = block(&harness, "conveyor")?;
            // `container` is 2x2; its min corner at (10,4) leaves (9,4) free.
            let sink = block(&harness, "container")?;
            let placed = harness.place(4, 4, source, 0, true);
            for x in 5..=9 {
                let _ = harness.place(x, 4, conveyor, 0, true);
            }
            let sink_placed = harness.place(10, 4, sink, 0, true);
            // ~6 tiles at 0.035 tiles/tick needs ~172 ticks; allow margin.
            for _ in 0..400 {
                harness.tick();
            }
            let sink_total = harness
                .build_at(10, 4)
                .and_then(|e| harness.world.get::<mind_core::world::ItemModule>(e))
                .map(|items| items.total)
                .unwrap_or(0);
            let belt_items: i32 = (5..=9)
                .filter_map(|x| harness.build_at(x, 4))
                .filter_map(|e| {
                    harness
                        .world
                        .get::<mind_core::world::blocks::distribution::conveyor::ConveyorBuild>(e)
                })
                .map(|belt| belt.len as i32)
                .sum();
            let pass = placed && sink_placed && sink_total > 0;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "sink_items": sink_total,
                "belt_items": belt_items,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_conveyor_lane" => {
            // Dedicated 64-wide map so the 32-tile lane fits.
            let mut harness = BuildHarness::new(64, 64, 7);
            let copper = harness
                .content()
                .item_id("copper")
                .ok_or_else(|| anyhow::anyhow!("copper item missing"))?;
            harness.register_behavior(
                "item-source",
                std::sync::Arc::new(mind_core::world::fixtures::logistics::SourceBehavior {
                    item: copper,
                    per_tick: 3,
                }),
            );
            let source = block(&harness, "item-source")?;
            let conveyor = block(&harness, "conveyor")?;
            let sink = block(&harness, "container")?;
            let _ = harness.place(2, 8, source, 0, true);
            for x in 3..=34 {
                let _ = harness.place(x, 8, conveyor, 0, true);
            }
            let _ = harness.place(35, 8, sink, 0, true);
            // 32 tiles at 0.035 tiles/tick needs ~915 ticks; allow margin.
            for _ in 0..1600 {
                harness.tick();
            }
            let delivered = harness
                .build_at(35, 8)
                .and_then(|e| harness.world.get::<mind_core::world::ItemModule>(e))
                .map(|items| items.total)
                .unwrap_or(0);
            let belt_items: i32 = (3..=34)
                .filter_map(|x| harness.build_at(x, 8))
                .filter_map(|e| {
                    harness
                        .world
                        .get::<mind_core::world::blocks::distribution::conveyor::ConveyorBuild>(e)
                })
                .map(|belt| belt.len as i32)
                .sum();
            let pass = delivered > 0;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "delivered": delivered,
                "belt_items": belt_items,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_router_fairness" => {
            let copper = harness
                .content()
                .item_id("copper")
                .ok_or_else(|| anyhow::anyhow!("copper item missing"))?;
            harness.register_behavior(
                "item-source",
                std::sync::Arc::new(mind_core::world::fixtures::logistics::SourceBehavior {
                    item: copper,
                    per_tick: 1,
                }),
            );
            let source = block(&harness, "item-source")?;
            let belt = block(&harness, "conveyor")?;
            let router = block(&harness, "router")?;
            let container = block(&harness, "container")?;
            let _ = harness.place(5, 2, source, 1, true);
            let _ = harness.place(5, 3, belt, 1, true);
            let _ = harness.place(5, 4, router, 0, true);
            let _ = harness.place(3, 3, container, 0, true);
            let _ = harness.place(6, 3, container, 0, true);
            let _ = harness.place(5, 5, container, 0, true);
            for _ in 0..300 {
                harness.tick();
            }
            let counts: Vec<i32> = [(3, 3), (6, 3), (5, 5)]
                .iter()
                .map(|(x, y)| {
                    harness
                        .build_at(*x, *y)
                        .and_then(|e| harness.world.get::<mind_core::world::ItemModule>(e))
                        .map(|items| items.total)
                        .unwrap_or(0)
                })
                .collect();
            let pass = counts.iter().all(|count| *count > 0);
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "outputs": counts,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_bridge_latency" => {
            let copper = harness
                .content()
                .item_id("copper")
                .ok_or_else(|| anyhow::anyhow!("copper item missing"))?;
            harness.register_behavior(
                "item-source",
                std::sync::Arc::new(mind_core::world::fixtures::logistics::SourceBehavior {
                    item: copper,
                    per_tick: 1,
                }),
            );
            let source = block(&harness, "item-source")?;
            let bridge = block(&harness, "bridge-conveyor")?;
            let belt = block(&harness, "conveyor")?;
            let container = block(&harness, "container")?;
            let _ = harness.place(4, 4, source, 0, true);
            let _ = harness.place(5, 4, bridge, 0, true);
            let _ = harness.place(8, 4, bridge, 0, true);
            let _ = harness.place(9, 4, belt, 0, true);
            let _ = harness.place(10, 4, container, 0, true);
            let _ = harness.configure(5, 4, ConfigValue::Point2(3, 0));
            for _ in 0..900 {
                harness.tick();
            }
            let delivered = harness
                .build_at(10, 4)
                .and_then(|e| harness.world.get::<mind_core::world::ItemModule>(e))
                .map(|items| items.total)
                .unwrap_or(0);
            let pass = delivered > 0;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "delivered": delivered,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_mass_driver_roundtrip" => {
            let copper = harness
                .content()
                .item_id("copper")
                .ok_or_else(|| anyhow::anyhow!("copper item missing"))?;
            // `--assume-power`: cheat rules give full consumer efficiency
            // without a power graph (plan 08 M3 verify).
            if let Some(mut rules) = harness
                .world
                .get_resource_mut::<mind_core::world::limits::BuildRules>()
            {
                rules.cheat = true;
            }
            let driver = block(&harness, "mass-driver")?;
            let _ = harness.place(8, 8, driver, 0, true);
            let _ = harness.place(20, 8, driver, 0, true);
            let _ = harness.configure(8, 8, ConfigValue::Point2(12, 0));
            let from = harness.build_at(8, 8);
            if let Some(from) = from
                && let Some(mut items) = harness
                    .world
                    .get_mut::<mind_core::world::modules::ItemModule>(from)
            {
                items.add(copper, 120, 120);
            }
            for _ in 0..1200 {
                harness.tick();
            }
            let received = harness
                .build_at(20, 8)
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::modules::ItemModule>(e)
                })
                .map(|items| items.total)
                .unwrap_or(0);
            let live_bolts = harness
                .world
                .iter_entities()
                .filter(|e| {
                    e.get::<mind_core::world::blocks::distribution::DriverBulletData>()
                        .is_some()
                })
                .count();
            let pass = received == 120 && live_bolts == 0;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "received": received,
                "live_bolts": live_bolts,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_unloader_drain" => {
            let copper = harness
                .content()
                .item_id("copper")
                .ok_or_else(|| anyhow::anyhow!("copper item missing"))?;
            let container = block(&harness, "container")?;
            let unloader = block(&harness, "unloader")?;
            let conveyor = block(&harness, "conveyor")?;
            let vault = block(&harness, "vault")?;
            let _ = harness.place(4, 4, container, 0, true);
            let _ = harness.place(6, 4, unloader, 0, true);
            // `vault` is size 3 centered at (11,4) -> covers (10,4)..(12,6).
            let _ = harness.place(11, 4, vault, 0, true);
            for x in 7..=9 {
                let _ = harness.place(x, 4, conveyor, 0, true);
            }
            let source = harness.build_at(4, 4);
            if let Some(source) = source
                && let Some(mut items) = harness
                    .world
                    .get_mut::<mind_core::world::modules::ItemModule>(source)
            {
                items.add(copper, 100, 300);
            }
            for _ in 0..600 {
                harness.tick();
            }
            let drained = harness
                .build_at(11, 4)
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::modules::ItemModule>(e)
                })
                .map(|items| items.total)
                .unwrap_or(0);
            let pass = drained > 0;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "drained": drained,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_core_inventory" => {
            let core = block(&harness, "core-shard")?;
            let container = block(&harness, "container")?;
            let _ = harness.place(6, 6, core, 0, true);
            let _ = harness.place(8, 6, container, 0, true);
            let _ = harness.place(14, 6, core, 0, true);
            let core_e = harness.build_at(6, 6);
            let capacity = core_e
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::blocks::storage::CoreBuild>(e)
                })
                .map(|c| c.storage_capacity)
                .unwrap_or(0);
            let linked = harness
                .build_at(8, 6)
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::blocks::storage::StorageBuild>(e)
                })
                .and_then(|s| s.linked_core);
            // 2 core-shards (4000 each) + one container (300).
            let pass = capacity == 8300 && linked == core_e;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "storage_capacity": capacity,
                "linked": linked.is_some(),
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_payload_move" => {
            let container = block(&harness, "container")?;
            let conveyor = block(&harness, "payload-conveyor")?;
            let _ = harness.place(9, 6, conveyor, 0, true);
            let _ = harness.place(6, 6, conveyor, 0, true);
            let first = harness.build_at(6, 6);
            let second = harness.build_at(9, 6);
            let entity = mind_core::world::blocks::payloads::create_build_payload(
                &mut harness.world,
                container,
                0,
            );
            if let (Some(first), Some(entity)) = (first, entity) {
                let payload = mind_core::world::behavior::PayloadRef {
                    entity: Some(entity),
                    content: container.raw(),
                    is_block: true,
                };
                mind_core::world::blocks::payloads::handle_payload(
                    &mut harness.world,
                    first,
                    first,
                    payload,
                );
            }
            for _ in 0..200 {
                harness.tick();
            }
            let on_second = second
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::blocks::payloads::PayloadHolder>(e)
                })
                .and_then(|h| h.payload)
                .is_some();
            let on_first = first
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::blocks::payloads::PayloadHolder>(e)
                })
                .and_then(|h| h.payload)
                .is_some();
            let pass = on_second && !on_first;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "on_second": on_second,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_payload_load_unload" => {
            use mind_core::world::blocks::payloads::{
                PayloadHolder, create_build_payload, handle_payload,
            };
            // `--assume-power`: cheat rules give full consumer efficiency
            // without a power graph (plan 08 M7 verify).
            if let Some(mut rules) = harness
                .world
                .get_resource_mut::<mind_core::world::limits::BuildRules>()
            {
                rules.cheat = true;
            }
            let container = block(&harness, "container")?;
            let loader = block(&harness, "payload-loader")?;
            let unloader = block(&harness, "payload-unloader")?;
            let copper = harness.content().item_id("copper");
            let _ = harness.place(6, 6, loader, 0, true);
            let _ = harness.place(6, 18, unloader, 0, true);
            let loader_e = harness.build_at(6, 6);
            let unloader_e = harness.build_at(6, 18);
            let payload_entity = create_build_payload(&mut harness.world, container, 0);
            let mut attached = false;
            if let (Some(load_e), Some(entity), Some(copper)) = (loader_e, payload_entity, copper) {
                if let Some(mut items) = harness
                    .world
                    .get_mut::<mind_core::world::modules::ItemModule>(load_e)
                {
                    items.add(copper, 20, 1000);
                }
                let payload = mind_core::world::behavior::PayloadRef {
                    entity: Some(entity),
                    content: container.raw(),
                    is_block: true,
                };
                handle_payload(&mut harness.world, load_e, load_e, payload);
                attached = true;
            }
            for _ in 0..600 {
                harness.tick();
            }
            let loaded_payload_total = payload_entity
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::modules::ItemModule>(e)
                })
                .map(|m| m.total())
                .unwrap_or(0);
            let loader_total = loader_e
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::modules::ItemModule>(e)
                })
                .map(|m| m.total())
                .unwrap_or(0);

            // Hand the (now item-filled) payload to the unloader and drain it.
            let held = loader_e
                .and_then(|e| harness.world.get::<PayloadHolder>(e))
                .and_then(|h| h.payload);
            if let (Some(load_e), Some(unload_e), Some(payload)) = (loader_e, unloader_e, held) {
                if let Some(mut holder) = harness.world.get_mut::<PayloadHolder>(load_e) {
                    holder.payload = None;
                }
                handle_payload(&mut harness.world, unload_e, unload_e, payload);
            }
            for _ in 0..600 {
                harness.tick();
            }
            let unloader_total = unloader_e
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::modules::ItemModule>(e)
                })
                .map(|m| m.total())
                .unwrap_or(0);
            let payload_after = payload_entity
                .and_then(|e| {
                    harness
                        .world
                        .get::<mind_core::world::modules::ItemModule>(e)
                })
                .map(|m| m.total())
                .unwrap_or(0);
            let loaded = attached && loaded_payload_total == 20 && loader_total == 0;
            let unloaded = unloader_total + payload_after == 20 && payload_after == 0;
            let pass = loaded && unloaded;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "loaded_payload_total": loaded_payload_total,
                "loader_total": loader_total,
                "unloader_total": unloader_total,
                "payload_after": payload_after,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_payload_fluids" => {
            use mind_core::world::blocks::payloads::payload_unloader::PayloadUnloaderBuild;
            use mind_core::world::blocks::payloads::{
                PayloadHolder, create_build_payload, handle_payload,
            };
            use mind_core::world::blocks::power::PowerProduction;
            // Cheat rules give full efficiency without a power graph; the loader
            // battery-charge path additionally needs an explicit `status` because
            // the headless harness installs no power graph (plan 09 owns graphs).
            if let Some(mut rules) = harness
                .world
                .get_resource_mut::<mind_core::world::limits::BuildRules>()
            {
                rules.cheat = true;
            }
            let loader = block(&harness, "payload-loader")?;
            let unloader = block(&harness, "payload-unloader")?;
            let liquid_container = block(&harness, "liquid-container")?;
            let battery = block(&harness, "battery")?;
            let tank = block(&harness, "liquid-tank")?;
            let water = harness
                .content()
                .liquid_id("water")
                .ok_or_else(|| anyhow::anyhow!("water liquid missing"))?;
            let _ = harness.place(6, 6, loader, 0, true);
            let _ = harness.place(6, 14, unloader, 0, true);
            // Neighbour for the unloader's `dumpLiquid` (tank 3x3 at its +x edge).
            let _ = harness.place(9, 14, tank, 0, true);
            let loader_e = harness.build_at(6, 6);
            let unloader_e = harness.build_at(6, 14);
            let neighbour_e = harness.build_at(9, 14);

            // --- Liquid: loader -> payload -> unloader -> neighbour tank. ---
            let liquid_payload = create_build_payload(&mut harness.world, liquid_container, 0);
            let mut liquid_loaded = 0.0f32;
            if let (Some(load_e), Some(entity)) = (loader_e, liquid_payload) {
                if let Some(mut module) = harness.world.get_mut::<LiquidModule>(load_e) {
                    module.add(water, 50.0, 100.0);
                }
                let payload = mind_core::world::behavior::PayloadRef {
                    entity: Some(entity),
                    content: liquid_container.raw(),
                    is_block: true,
                };
                handle_payload(&mut harness.world, load_e, load_e, payload);
                for _ in 0..60 {
                    harness.tick();
                }
                liquid_loaded = harness
                    .world
                    .get::<LiquidModule>(entity)
                    .map(|m| m.current_amount)
                    .unwrap_or(0.0);
                // Hand the loaded payload to the unloader.
                let held = harness
                    .world
                    .get::<PayloadHolder>(load_e)
                    .and_then(|h| h.payload);
                if let Some(payload) = held
                    && let Some(unload_e) = unloader_e
                {
                    if let Some(mut holder) = harness.world.get_mut::<PayloadHolder>(load_e) {
                        holder.payload = None;
                    }
                    handle_payload(&mut harness.world, unload_e, unload_e, payload);
                }
                for _ in 0..60 {
                    harness.tick();
                }
            }
            let neighbour_water = neighbour_e
                .and_then(|e| harness.world.get::<LiquidModule>(e))
                .map(|m| m.current_amount)
                .unwrap_or(0.0);
            let payload_liquid_after = liquid_payload
                .and_then(|e| harness.world.get::<LiquidModule>(e))
                .map(|m| m.current_amount)
                .unwrap_or(0.0);

            // --- Power: loader charges a battery payload; unloader drains it. ---
            let battery_payload = create_build_payload(&mut harness.world, battery, 0);
            let mut battery_loaded = 0.0f32;
            let mut unloader_power = 0.0f32;
            let mut power_production = 0.0f32;
            if let (Some(load_e), Some(unload_e), Some(entity)) =
                (loader_e, unloader_e, battery_payload)
            {
                if let Some(mut module) = harness.world.get_mut::<PowerModule>(load_e) {
                    module.status = 1.0;
                }
                let payload = mind_core::world::behavior::PayloadRef {
                    entity: Some(entity),
                    content: battery.raw(),
                    is_block: true,
                };
                handle_payload(&mut harness.world, load_e, load_e, payload);
                for _ in 0..60 {
                    harness.tick();
                }
                battery_loaded = harness
                    .world
                    .get::<PowerModule>(entity)
                    .map(|m| m.status)
                    .unwrap_or(0.0);
                let held = harness
                    .world
                    .get::<PayloadHolder>(load_e)
                    .and_then(|h| h.payload);
                if let Some(payload) = held {
                    if let Some(mut holder) = harness.world.get_mut::<PayloadHolder>(load_e) {
                        holder.payload = None;
                    }
                    handle_payload(&mut harness.world, unload_e, unload_e, payload);
                }
                for _ in 0..2 {
                    harness.tick();
                }
                unloader_power = harness
                    .world
                    .get::<PayloadUnloaderBuild>(unload_e)
                    .map(|s| s.last_output_power)
                    .unwrap_or(0.0);
                power_production = harness
                    .world
                    .get::<PowerProduction>(unload_e)
                    .map(|p| p.0)
                    .unwrap_or(0.0);
            }

            let liquid_ok =
                liquid_loaded >= 49.0 && payload_liquid_after <= 0.011 && neighbour_water > 0.0;
            let power_ok = battery_loaded > 0.3 && unloader_power > 0.0 && power_production > 0.0;
            let pass = liquid_ok && power_ok;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "liquid_loaded": liquid_loaded,
                "neighbour_water": neighbour_water,
                "payload_liquid_after": payload_liquid_after,
                "battery_loaded": battery_loaded,
                "unloader_power": unloader_power,
                "power_production": power_production,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
            });
            print_json(&report, json, !json);
            if !pass {
                bail!("scenario {name} failed");
            }
        }
        "logistics_payload_driver_throw" => {
            use mind_core::world::blocks::payloads::{
                PayloadHolder, create_build_payload, handle_payload,
            };
            if let Some(mut rules) = harness
                .world
                .get_resource_mut::<mind_core::world::limits::BuildRules>()
            {
                rules.cheat = true;
            }
            let container = block(&harness, "container")?;
            let driver = block(&harness, "payload-mass-driver")?;
            let placed_from = harness.place(6, 6, driver, 0, true);
            let placed_to = harness.place(18, 6, driver, 0, true);
            let linked_from = harness.configure(6, 6, ConfigValue::Point2(12, 0));
            let linked_to = harness.configure(18, 6, ConfigValue::Point2(-12, 0));
            let from = harness.build_at(6, 6);
            let to = harness.build_at(18, 6);
            let payload_entity = create_build_payload(&mut harness.world, container, 0);
            if let (Some(from), Some(entity)) = (from, payload_entity) {
                let payload = mind_core::world::behavior::PayloadRef {
                    entity: Some(entity),
                    content: container.raw(),
                    is_block: true,
                };
                handle_payload(&mut harness.world, from, from, payload);
            }
            for _ in 0..3000 {
                harness.tick();
            }
            let delivered = to
                .and_then(|e| harness.world.get::<PayloadHolder>(e))
                .is_some_and(|h| h.payload.is_some());
            let consumed = from
                .and_then(|e| harness.world.get::<PayloadHolder>(e))
                .is_some_and(|h| h.payload.is_none());
            let pass =
                placed_from && placed_to && linked_from && linked_to && delivered && consumed;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "delivered": delivered,
                "source_empty": consumed,
                "checksum": mind_core::world::fixtures::logistics::logistics_checksum(&harness.world).to_hex(),
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
        "consumer_efficiency" => {
            let smelter = block(&harness, "silicon-smelter")?;
            let placed = harness.place(4, 4, smelter, 0, true);
            let entity = harness.build_at(4, 4);
            if let Some(e) = entity {
                let coal = harness.content().item_id("coal");
                let sand = harness.content().item_id("sand");
                if let (Some(coal), Some(sand)) = (coal, sand)
                    && let Some(mut items) = harness.world.get_mut::<ItemModule>(e)
                {
                    items.add(coal, 100, 100);
                    items.add(sand, 100, 100);
                }
                if let Some(mut power) = harness.world.get_mut::<PowerModule>(e) {
                    power.status = 1.0;
                }
            }
            for _ in 0..90 {
                harness.tick();
            }
            let silicon = harness.content().item_id("silicon");
            let produced = match (entity, silicon) {
                (Some(e), Some(silicon)) => harness
                    .world
                    .get::<ItemModule>(e)
                    .map(|items| items.get(silicon))
                    .unwrap_or(0),
                _ => 0,
            };
            let pass = placed && entity.is_some() && produced >= 2;
            let report = serde_json::json!({
                "scenario": name,
                "pass": pass,
                "silicon": produced,
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

fn bench(profile: &str, buildings: usize, ticks: u64, json: bool) -> Result<()> {
    let mut harness = BuildHarness::new(96, 96, 7);
    let mut samples: Vec<u64> = Vec::new();
    let mut detail = serde_json::json!({});
    let mut placed = 0usize;

    match profile {
        "place" => {
            let large = block(&harness, "copper-wall-large")?;
            for i in 0..buildings {
                let x = (i % 45) as i32 * 2;
                let y = (i / 45) as i32 * 2;
                let start = Instant::now();
                let ok = harness.place(x, y, large, 0, true);
                samples.push(start.elapsed().as_nanos() as u64 / 1000);
                if ok {
                    placed += 1;
                }
            }
            detail = serde_json::json!({ "placed": placed });
        }
        "construct" => {
            let large = block(&harness, "copper-wall-large")?;
            for i in 0..buildings {
                let x = (i % 45) as i32 * 2;
                let y = (i / 45) as i32 * 2;
                if harness.place(x, y, large, 0, false) {
                    placed += 1;
                }
            }
            for _ in 0..ticks.max(1) {
                let start = Instant::now();
                harness.construct_tick(0.05);
                samples.push(start.elapsed().as_nanos() as u64 / 1000);
            }
            detail = serde_json::json!({ "placed": placed });
        }
        "logistics" => {
            let conveyor = block(&harness, "conveyor")?;
            let router = block(&harness, "router")?;
            let copper = harness.content().item_id("copper");
            for i in 0..buildings {
                let x = (i % 45) as i32 * 2;
                let y = (i / 45) as i32 * 2;
                let block_id = if i % 8 == 7 { router } else { conveyor };
                if harness.place(x, y, block_id, 0, true) {
                    placed += 1;
                    if let Some(e) = harness.build_at(x, y)
                        && let Some(copper) = copper
                        && let Some(mut items) = harness.world.get_mut::<ItemModule>(e)
                    {
                        items.add(copper, 3, 1000);
                    }
                }
            }
            time_ticks(&mut harness, ticks, &mut samples);
            detail = serde_json::json!({ "placed": placed, "logistics": true });
        }
        "active" => {
            let smelter = block(&harness, "silicon-smelter")?;
            let coal = harness.content().item_id("coal");
            let sand = harness.content().item_id("sand");
            for i in 0..buildings {
                let x = (i % 30) as i32 * 3;
                let y = (i / 30) as i32 * 3;
                if !harness.place(x, y, smelter, 0, true) {
                    continue;
                }
                placed += 1;
                if let Some(e) = harness.build_at(x, y) {
                    if let (Some(coal), Some(sand)) = (coal, sand)
                        && let Some(mut items) = harness.world.get_mut::<ItemModule>(e)
                    {
                        items.add(coal, 1000, 1000);
                        items.add(sand, 1000, 1000);
                    }
                    if let Some(mut power) = harness.world.get_mut::<PowerModule>(e) {
                        power.status = 1.0;
                    }
                }
            }
            time_ticks(&mut harness, ticks, &mut samples);
            detail = serde_json::json!({ "placed": placed, "active": true });
        }
        _ => {
            let wall = block(&harness, "copper-wall")?;
            for i in 0..buildings {
                let x = (i % 90) as i32;
                let y = (i / 90) as i32;
                if harness.place(x, y, wall, 0, true) {
                    placed += 1;
                }
            }
            time_ticks(&mut harness, ticks, &mut samples);
            detail = serde_json::json!({ "placed": placed });
        }
    }

    let (p50_us, p95_us, p99_us) = percentiles(samples);
    // Plan 23 M4: only present when the bench is built `--features profile-build`.
    #[cfg(feature = "profile-build")]
    if mind_core::world::update::profile::enabled() {
        let (order, consume, dispatch, pticks, entities) =
            mind_core::world::update::profile::take();
        if pticks > 0 {
            eprintln!(
                "PROFILE order_us/tick={:.1} consume_us/tick={:.1} dispatch_us/tick={:.1} entities={} ticks={}",
                order as f64 / pticks as f64 / 1000.0,
                consume as f64 / pticks as f64 / 1000.0,
                dispatch as f64 / pticks as f64 / 1000.0,
                entities,
                pticks
            );
        }
    }
    let report = serde_json::json!({
        "scenario": "blocks_bench",
        "profile": profile,
        "buildings": placed,
        "ticks": ticks,
        "p50_us": p50_us,
        "p95_us": p95_us,
        "p99_us": p99_us,
        "checksum": harness.checksum_hex(),
        "detail": detail,
    });
    print_json(&report, json, !json);
    Ok(())
}

fn time_ticks(harness: &mut BuildHarness, ticks: u64, samples: &mut Vec<u64>) {
    for _ in 0..ticks {
        let start = Instant::now();
        harness.tick();
        samples.push(start.elapsed().as_nanos() as u64 / 1000);
    }
}

fn percentiles(mut samples: Vec<u64>) -> (u64, u64, u64) {
    if samples.is_empty() {
        return (0, 0, 0);
    }
    samples.sort_unstable();
    let pick = |pct: usize| samples[(samples.len() * pct / 100).min(samples.len() - 1)];
    (pick(50), pick(95), pick(99))
}
