// SPDX-License-Identifier: GPL-3.0-only

//! Plan 09 headless network scenarios and benches
//! (`power`/`liquid`/`heat` subcommands, plan 09 §7b/§7d).
//!
//! The scenarios drive the plan-09 core algorithms directly (graph arena,
//! liquid transfer primitives, heat pull) so the oracle is independent of the
//! plan-07 behavior merge. Dumps are stable [`NetworkState`] JSON committed
//! under `tests/golden/network/`; benches report P50/P99 per tick against the
//! §7d budgets.

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use mind_core::content::{BlockId, LiquidId};
use mind_core::entities::comp::{Building, PowerGraphUpdater, TeamComp};
use mind_core::fixtures::power::PowerHarness;
use mind_core::world::blocks::heat::{
    HeatConductor, HeatCrafter, HeatState, calculate_heat, crafter_efficiency_scale, crafter_heat,
    heat_producer_step,
};
use mind_core::world::blocks::liquid::{LiquidFlowCache, LiquidNode, update_conduit};
use mind_core::world::blocks::power::reactors::nuclear_coolant_removal;
use mind_core::world::blocks::power::{
    PowerGrids, PowerNodeInfo, PowerProduction, power_graph_removed, update_power_graph,
};
use mind_core::world::modules::{LiquidModule, PowerModule};
use mind_core::world::{BuildRules, BuildingState, NetworkState, TilePos, WorldGrid};

use crate::cli::NetworkCommand;

/// Exit code: success.
const EXIT_PASS: i32 = 0;
/// Exit code: golden mismatch.
const EXIT_FAIL: i32 = 1;

/// Which resource network a subcommand targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkKind {
    /// Power graphs/generators.
    Power,
    /// Liquid conduits/routers.
    Liquid,
    /// Heat producers/conductors/crafters.
    Heat,
}

/// Scenario names registered for a network (append-only).
pub fn names(kind: NetworkKind) -> &'static [&'static str] {
    match kind {
        NetworkKind::Power => &[
            "power_battery_cycle",
            "power_graph_split_merge",
            "power_network_determinism",
        ],
        NetworkKind::Liquid => &["liquid_conduit_transfer"],
        NetworkKind::Heat => &["heat_network_equilibrium"],
    }
}

/// One scenario result: canonical dump text plus a human/JSON report.
struct ScenarioOutput {
    dump: String,
    report: serde_json::Value,
}

/// Runs a `power`/`liquid`/`heat` subcommand.
pub fn run(kind: NetworkKind, command: &NetworkCommand) -> Result<i32> {
    match command {
        NetworkCommand::List => {
            for name in names(kind) {
                println!("{name}");
            }
            Ok(EXIT_PASS)
        }
        NetworkCommand::Scenario {
            name,
            dump,
            check,
            json,
        } => scenario(kind, name, dump.as_deref(), check.as_deref(), *json),
        NetworkCommand::Bench {
            buildings,
            ticks,
            warmup,
            json,
        } => bench(kind, *buildings, *ticks, *warmup, *json),
    }
}

fn run_scenario(kind: NetworkKind, name: &str) -> Result<ScenarioOutput> {
    match (kind, name) {
        (NetworkKind::Power, "power_battery_cycle") => power_battery_cycle(),
        (NetworkKind::Power, "power_graph_split_merge") => {
            let run = split_merge_run()?;
            let dump = run
                .states
                .last()
                .map(|state| state.to_json())
                .ok_or_else(|| anyhow::anyhow!("split/merge produced no final state"))?;
            Ok(ScenarioOutput {
                dump,
                report: run.report,
            })
        }
        (NetworkKind::Power, "power_network_determinism") => power_determinism(),
        (NetworkKind::Liquid, "liquid_conduit_transfer") => liquid_conduit_transfer(),
        (NetworkKind::Heat, "heat_network_equilibrium") => heat_network_equilibrium(),
        _ => bail!("unknown {} scenario `{name}`", kind_label(kind)),
    }
}

fn kind_label(kind: NetworkKind) -> &'static str {
    match kind {
        NetworkKind::Power => "power",
        NetworkKind::Liquid => "liquid",
        NetworkKind::Heat => "heat",
    }
}

fn scenario(
    kind: NetworkKind,
    name: &str,
    dump: Option<&Path>,
    check: Option<&Path>,
    json: bool,
) -> Result<i32> {
    let out = run_scenario(kind, name)?;
    if let Some(path) = check {
        let golden = std::fs::read_to_string(path)
            .with_context(|| format!("reading golden `{}`", path.display()))?;
        if golden != out.dump {
            log::error!("{} scenario `{name}`: golden mismatch", kind_label(kind));
            return Ok(EXIT_FAIL);
        }
        if json {
            println!("{}", serde_json::to_string_pretty(&out.report)?);
        } else {
            println!("{} scenario `{name}`: OK", kind_label(kind));
        }
        return Ok(EXIT_PASS);
    }
    if let Some(path) = dump {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(path, &out.dump).with_context(|| format!("writing `{}`", path.display()))?;
    } else if !json {
        print!("{}", out.dump);
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&out.report)?);
    }
    Ok(EXIT_PASS)
}

// ---------------------------------------------------------------------------
// Power
// ---------------------------------------------------------------------------

fn spawn_power(
    world: &mut World,
    grid: &mut WorldGrid,
    x: i16,
    y: i16,
    info: PowerNodeInfo,
    production: f32,
) -> Entity {
    let entity = world
        .spawn((
            Building::new(TilePos::new(x, y), BlockId::AIR, 0),
            TeamComp { team: 0 },
            PowerModule::new(),
            info,
            PowerProduction(production),
        ))
        .id();
    grid.tiles.get_mut(x as i32, y as i32).build = Some(entity);
    entity
}

fn link(world: &mut World, a: Entity, b: Entity) {
    if let Some(mut building) = world.get_mut::<Building>(a)
        && !building.proximity.contains(&b)
    {
        building.proximity.push(b);
    }
    if let Some(mut building) = world.get_mut::<Building>(b)
        && !building.proximity.contains(&a)
    {
        building.proximity.push(a);
    }
}

fn unlink(world: &mut World, a: Entity, b: Entity) {
    if let Some(mut building) = world.get_mut::<Building>(a) {
        building.proximity.retain(|entity| *entity != b);
    }
    if let Some(mut building) = world.get_mut::<Building>(b) {
        building.proximity.retain(|entity| *entity != a);
    }
}

fn updater_count(world: &World) -> usize {
    world
        .iter_entities()
        .filter(|entity| entity.get::<PowerGraphUpdater>().is_some())
        .count()
}

fn conductive_node() -> PowerNodeInfo {
    PowerNodeInfo {
        conductive: true,
        ..PowerNodeInfo::default()
    }
}

fn graph_snapshot(graphs: &PowerGrids, world: &World, tick: u64) -> NetworkState {
    let mut state = NetworkState::new(tick);
    for graph in graphs.iter_graphs() {
        state.push_graph(graph, world, 1.0);
    }
    state
}

/// `power_battery_cycle` (§7b.#1): charge a battery, clear the producer, drain.
fn power_battery_cycle() -> Result<ScenarioOutput> {
    let mut harness = PowerHarness::new();
    let producer = harness.producer(10.0);
    let battery = harness.battery(100.0);
    let mut samples: Vec<serde_json::Value> = Vec::new();
    for tick in 0..30u64 {
        harness.update(1.0);
        if matches!(tick, 0 | 1 | 4 | 9 | 19 | 29) {
            samples.push(serde_json::json!({"tick": tick, "phase": "charge", "status": harness.status(battery)}));
        }
    }
    let charged = harness.status(battery);

    harness.remove_list(producer);
    let consumer = harness.direct_consumer(5.0);
    for tick in 30..230u64 {
        harness.update(1.0);
        if matches!(tick, 30 | 34 | 39 | 59 | 99 | 199 | 229) {
            samples.push(serde_json::json!({"tick": tick, "phase": "drain", "status": harness.status(battery), "consumer": harness.status(consumer)}));
        }
    }
    let drained = harness.status(battery);
    let consumer_status = harness.status(consumer);
    let pass = charged > 0.0 && drained < charged;

    let mut state = NetworkState::new(230);
    if let Some(graph) = harness.graphs.graph(harness.graph) {
        state.push_graph(graph, &harness.world, 1.0);
    }
    let report = serde_json::json!({
        "scenario": "power_battery_cycle",
        "pass": pass,
        "charged_status": charged,
        "drained_status": drained,
        "consumer_status": consumer_status,
        "samples": samples,
    });
    if !pass {
        bail!("power_battery_cycle failed");
    }
    Ok(ScenarioOutput {
        dump: state.to_json(),
        report,
    })
}

struct SplitMergeRun {
    states: Vec<NetworkState>,
    report: serde_json::Value,
}

/// Builds a 5-node chain with a generator on the left and a consumer on the
/// right; breaks the middle node, then re-adds it. Returns the three states
/// (built, split, merged) and the assertion report.
fn split_merge_run() -> Result<SplitMergeRun> {
    let mut world = World::new();
    world.insert_resource(BuildRules::default());
    let mut grid = WorldGrid::new(16, 16);
    let mut graphs = PowerGrids::new();

    let producer = spawn_power(&mut world, &mut grid, 0, 1, PowerNodeInfo::producer(), 10.0);
    let n0 = spawn_power(&mut world, &mut grid, 0, 0, conductive_node(), 0.0);
    let n1 = spawn_power(&mut world, &mut grid, 1, 0, conductive_node(), 0.0);
    let n2 = spawn_power(&mut world, &mut grid, 2, 0, conductive_node(), 0.0);
    let n3 = spawn_power(&mut world, &mut grid, 3, 0, conductive_node(), 0.0);
    let n4 = spawn_power(&mut world, &mut grid, 4, 0, conductive_node(), 0.0);
    let consumer = spawn_power(
        &mut world,
        &mut grid,
        4,
        1,
        PowerNodeInfo::consumer(5.0),
        0.0,
    );

    link(&mut world, producer, n0);
    link(&mut world, n0, n1);
    link(&mut world, n1, n2);
    link(&mut world, n2, n3);
    link(&mut world, n3, n4);
    link(&mut world, n4, consumer);

    let order = [producer, n0, n1, n2, n3, n4, consumer];
    for entity in order {
        update_power_graph(&mut graphs, &mut world, &grid, entity);
    }
    graphs.update_all(&mut world, 1.0);
    let built = graph_snapshot(&graphs, &world, 0);
    let built_graphs = graphs.graph_count();
    let built_updaters = updater_count(&world);
    let built_satisfaction = graphs
        .iter_graphs()
        .next()
        .map(|graph| graph.get_satisfaction())
        .unwrap_or(0.0);

    power_graph_removed(&mut graphs, &mut world, &grid, n2);
    graphs.update_all(&mut world, 1.0);
    let split = graph_snapshot(&graphs, &world, 1);
    let split_graphs = graphs.graph_count();
    let split_updaters = updater_count(&world);
    let mut producer_side = 0.0f32;
    let mut consumer_side = 0.0f32;
    for graph in graphs.iter_graphs() {
        if graph.producers.is_empty() {
            consumer_side = graph.get_satisfaction();
        } else {
            producer_side = graph.get_satisfaction();
        }
    }

    update_power_graph(&mut graphs, &mut world, &grid, n2);
    graphs.update_all(&mut world, 1.0);
    let merged = graph_snapshot(&graphs, &world, 2);
    let merged_graphs = graphs.graph_count();
    let merged_updaters = updater_count(&world);
    let merged_satisfaction = graphs
        .iter_graphs()
        .next()
        .map(|graph| graph.get_satisfaction())
        .unwrap_or(0.0);

    let pass = built_graphs == 1
        && built_updaters == 1
        && split_graphs == 2
        && split_updaters == 2
        && producer_side == 1.0
        && consumer_side == 0.0
        && merged_graphs == 1
        && merged_updaters == 1
        && merged_satisfaction == 1.0;

    let report = serde_json::json!({
        "scenario": "power_graph_split_merge",
        "pass": pass,
        "built": {"graphs": built_graphs, "updaters": built_updaters, "satisfaction": built_satisfaction},
        "split": {"graphs": split_graphs, "updaters": split_updaters, "producer_satisfaction": producer_side, "consumer_satisfaction": consumer_side},
        "merged": {"graphs": merged_graphs, "updaters": merged_updaters, "satisfaction": merged_satisfaction},
    });
    if !pass {
        bail!("power_graph_split_merge failed: {report}");
    }
    Ok(SplitMergeRun {
        states: vec![built, split, merged],
        report,
    })
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// `power_network_determinism`: the split/merge state sequence is identical
/// across runs; emits one `step checksum` line per checkpoint.
fn power_determinism() -> Result<ScenarioOutput> {
    let first = split_merge_run()?;
    let second = split_merge_run()?;
    let identical = first.states == second.states;
    let checksums: Vec<u64> = first
        .states
        .iter()
        .map(|state| fnv1a(state.to_json().as_bytes()))
        .collect();
    let other: Vec<u64> = second
        .states
        .iter()
        .map(|state| fnv1a(state.to_json().as_bytes()))
        .collect();
    let dump = format!(
        "0 {:016x}\n1 {:016x}\n2 {:016x}\n",
        checksums[0], checksums[1], checksums[2]
    );
    let report = serde_json::json!({
        "scenario": "power_network_determinism",
        "pass": identical && checksums == other,
        "checkpoints": checksums.len(),
        "checksums": checksums.iter().map(|value| format!("{value:016x}")).collect::<Vec<_>>(),
    });
    if !(identical && checksums == other) {
        bail!("power_network_determinism: runs diverged");
    }
    Ok(ScenarioOutput { dump, report })
}

// ---------------------------------------------------------------------------
// Liquid
// ---------------------------------------------------------------------------

fn spawn_liquid(
    world: &mut World,
    grid: &mut WorldGrid,
    x: i16,
    y: i16,
    rotation: u8,
    node: LiquidNode,
    slots: usize,
) -> Entity {
    let entity = world
        .spawn((
            Building::new(TilePos::new(x, y), BlockId::AIR, rotation),
            TeamComp { team: 0 },
            LiquidModule::with_liquids(slots),
            node,
        ))
        .id();
    grid.tiles.get_mut(x as i32, y as i32).build = Some(entity);
    entity
}

fn liquid_node(capacity: f32, accepts: bool) -> LiquidNode {
    LiquidNode {
        capacity,
        accepts,
        ..LiquidNode::default()
    }
}

/// `liquid_conduit_transfer` (§7b.#3): an infinite source pushes through a
/// 10-conduit chain into a tank; breaking the chain mid-way stops delivery and
/// the flow window reports the steady-state rate.
fn liquid_conduit_transfer() -> Result<ScenarioOutput> {
    let water = LiquidId::WATER;
    let slots = (water.index() + 1).max(2);
    let mut world = World::new();
    world.insert_resource(BuildRules::default());
    let mut grid = WorldGrid::new(16, 16);

    let source = spawn_liquid(
        &mut world,
        &mut grid,
        0,
        0,
        0,
        liquid_node(100.0, false),
        slots,
    );
    let mut conduits = Vec::new();
    for x in 1..=10i16 {
        let node = LiquidNode {
            leakable: true,
            ..liquid_node(10.0, true)
        };
        conduits.push(spawn_liquid(&mut world, &mut grid, x, 0, 0, node, slots));
    }
    let tank = spawn_liquid(
        &mut world,
        &mut grid,
        11,
        0,
        0,
        liquid_node(5_000.0, true),
        slots,
    );
    let first = conduits[0];
    let middle = conduits[4];

    let mut cache = LiquidFlowCache::new(slots);
    cache.active = true;

    let mut tank_samples: Vec<serde_json::Value> = Vec::new();
    let mut previous_tank = 0.0f32;
    let mut monotone = true;
    let mut flow_rate = -1.0f32;

    for tick in 0..300u64 {
        // Infinite source: refill to capacity before moving.
        if let Some(mut module) = world.get_mut::<LiquidModule>(source) {
            module.liquids[water.index()] = 100.0;
            module.current_amount = 100.0;
        }
        if tick < 150 {
            // Flow the whole chain.
            mind_core::world::blocks::liquid::move_liquid(&mut world, &grid, source, first, water);
            for &conduit in &conduits {
                update_conduit(&mut world, &grid, conduit);
            }
        } else if tick == 150 {
            if let Some(mut node) = world.get_mut::<LiquidNode>(middle) {
                node.accepts = false;
            }
        } else {
            // Only the broken segment can still drain downstream.
            for &conduit in conduits.iter().skip(5) {
                update_conduit(&mut world, &grid, conduit);
            }
        }

        let tank_amount = world
            .get::<LiquidModule>(tank)
            .map(|module| module.get(water))
            .unwrap_or(0.0);
        let delta = tank_amount - previous_tank;
        if delta < -1e-4 {
            monotone = false;
        }
        cache.record(water, delta.max(0.0));
        cache.tick();
        previous_tank = tank_amount;

        if matches!(tick, 0 | 10 | 50 | 99 | 149 | 199 | 299) {
            tank_samples.push(serde_json::json!({"tick": tick, "tank": tank_amount}));
        }
        if tick == 99 {
            // Read while the chain is still flowing (the window decays to zero
            // once delivery stops after the break).
            flow_rate = cache.get_flow_rate(water);
        }
    }

    let pass = monotone && previous_tank > 100.0 && flow_rate > 0.0;

    let mut state = NetworkState::new(300);
    for entity in std::iter::once(source)
        .chain(conduits.iter().copied())
        .chain(std::iter::once(tank))
    {
        if let (Some(building), Some(module), Some(node)) = (
            world.get::<Building>(entity),
            world.get::<LiquidModule>(entity),
            world.get::<LiquidNode>(entity),
        ) {
            state.push_liquid(BuildingState::new(
                building.tile.x(),
                building.tile.y(),
                "liquid",
                module.get(water),
                node.capacity,
            ));
        }
    }
    let report = serde_json::json!({
        "scenario": "liquid_conduit_transfer",
        "pass": pass,
        "monotone": monotone,
        "final_tank": previous_tank,
        "flow_rate": flow_rate,
        "samples": tank_samples,
    });
    if !pass {
        bail!("liquid_conduit_transfer failed: {report}");
    }
    Ok(ScenarioOutput {
        dump: state.to_json(),
        report,
    })
}

// ---------------------------------------------------------------------------
// Heat
// ---------------------------------------------------------------------------

fn spawn_heat(world: &mut World, grid: &mut WorldGrid, x: i16, y: i16, state: HeatState) -> Entity {
    let entity = world
        .spawn((
            Building::new(TilePos::new(x, y), BlockId::AIR, 0),
            TeamComp { team: 0 },
            state,
        ))
        .id();
    grid.tiles.get_mut(x as i32, y as i32).build = Some(entity);
    entity
}

/// `heat_network_equilibrium` (§7b.#4): producer → conductor → HeatCrafter;
/// the conductor carries the producer's heat and the crafter overheat scale
/// follows the formula; removing the producer decays heat to zero.
fn heat_network_equilibrium() -> Result<ScenarioOutput> {
    let mut world = World::new();
    world.insert_resource(BuildRules::default());
    let mut grid = WorldGrid::new(16, 16);

    let producer = spawn_heat(
        &mut world,
        &mut grid,
        0,
        0,
        HeatState {
            heat: 0.0,
            heat_progress: 0.0,
            heat_output: 10.0,
            rotate: false,
        },
    );
    let conductor = spawn_heat(
        &mut world,
        &mut grid,
        1,
        0,
        HeatState {
            heat: 0.0,
            heat_progress: 0.0,
            heat_output: 0.0,
            rotate: false,
        },
    );
    world.entity_mut(conductor).insert(HeatConductor::default());
    let crafter = spawn_heat(
        &mut world,
        &mut grid,
        2,
        0,
        HeatState {
            heat: 0.0,
            heat_progress: 0.0,
            heat_output: 0.0,
            rotate: false,
        },
    );
    world.entity_mut(crafter).insert(HeatCrafter {
        requirement: 5.0,
        overheat_scale: 1.0,
        max_efficiency: 3.0,
    });

    link(&mut world, producer, conductor);
    link(&mut world, conductor, crafter);

    let mut checkpoint = serde_json::json!({});
    let mut final_scale = 0.0f32;
    for tick in 0..600u64 {
        if let Some(mut state) = world.get_mut::<HeatState>(producer) {
            state.heat = heat_producer_step(state.heat, state.heat_output, 1.0, 0.05, 1.0);
        }
        let heat = crafter_heat(&mut world, crafter, tick);
        if let Some(mut state) = world.get_mut::<HeatState>(crafter) {
            state.heat = heat;
        }
        if tick == 299 {
            let producer_heat = world
                .get::<HeatState>(producer)
                .map(|state| state.heat)
                .unwrap_or(0.0);
            let conductor_heat = world
                .get::<HeatState>(conductor)
                .map(|state| state.heat)
                .unwrap_or(0.0);
            final_scale = crafter_efficiency_scale(heat, 5.0, 1.0, 3.0);
            checkpoint = serde_json::json!({
                "producer_heat": producer_heat,
                "conductor_heat": conductor_heat,
                "crafter_heat": heat,
                "crafter_scale": final_scale,
            });
        }
        if tick == 349 {
            unlink(&mut world, producer, conductor);
        }
    }

    let final_conductor_heat = world
        .get::<HeatState>(conductor)
        .map(|state| state.heat)
        .unwrap_or(0.0);
    let final_crafter_heat = world
        .get::<HeatState>(crafter)
        .map(|state| state.heat)
        .unwrap_or(0.0);
    let conductor_at_rest = checkpoint["conductor_heat"].as_f64().unwrap_or(0.0) as f32;
    let pass = final_conductor_heat < 0.01
        && final_crafter_heat < 0.01
        && conductor_at_rest > 9.9
        && (final_scale - 2.0).abs() < 0.01;

    // Nuclear coolant formula as the second fixture (coolantPower 0.5, 1 unit).
    let (nuclear_heat, nuclear_used) = nuclear_coolant_removal(1.0, 0.5, 1.0);

    let mut state = NetworkState::new(600);
    for (entity, name) in [
        (producer, "heat-producer"),
        (conductor, "heat-conductor"),
        (crafter, "heat-crafter"),
    ] {
        if let Some(building) = world.get::<Building>(entity)
            && let Some(heat) = world.get::<HeatState>(entity)
        {
            state.push_heat(BuildingState::new(
                building.tile.x(),
                building.tile.y(),
                name,
                heat.heat,
                heat.heat_output,
            ));
        }
    }
    let report = serde_json::json!({
        "scenario": "heat_network_equilibrium",
        "pass": pass,
        "checkpoint": checkpoint,
        "final_conductor_heat": final_conductor_heat,
        "final_crafter_heat": final_crafter_heat,
        "nuclear": {"heat": nuclear_heat, "used": nuclear_used},
    });
    if !pass {
        bail!("heat_network_equilibrium failed: {report}");
    }
    Ok(ScenarioOutput {
        dump: state.to_json(),
        report,
    })
}

// ---------------------------------------------------------------------------
// Bench
// ---------------------------------------------------------------------------

fn percentiles(mut samples: Vec<u64>) -> (u64, u64) {
    if samples.is_empty() {
        return (0, 0);
    }
    samples.sort_unstable();
    let p50 = samples[samples.len() * 50 / 100];
    let p99 = samples[(samples.len() * 99 / 100).min(samples.len() - 1)];
    (p50, p99)
}

fn bench(kind: NetworkKind, buildings: usize, ticks: u64, warmup: u64, json: bool) -> Result<i32> {
    if ticks == 0 {
        bail!("bench --ticks must be greater than 0");
    }
    let (mut samples, detail) = match kind {
        NetworkKind::Power => bench_power(buildings.max(1), ticks, warmup),
        NetworkKind::Liquid => bench_liquid(buildings.max(1), ticks, warmup),
        NetworkKind::Heat => bench_heat(buildings.max(1), ticks, warmup),
    };
    let (p50_us, p99_us) = percentiles(std::mem::take(&mut samples));
    let budget_ms = match kind {
        NetworkKind::Power => 0.30,
        NetworkKind::Liquid => 0.20,
        NetworkKind::Heat => 0.50,
    };
    let p99_ms = p99_us as f64 / 1000.0;
    let report = serde_json::json!({
        "scenario": format!("bench_{}", kind_label(kind)),
        "network": kind_label(kind),
        "buildings": buildings,
        "ticks": ticks,
        "warmup": warmup,
        "p50_us": p50_us,
        "p99_us": p99_us,
        "p99_ms": p99_ms,
        "budget_ms": budget_ms,
        "within_budget": p99_ms <= budget_ms,
        "detail": detail,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "bench {}: {} buildings, p50={}us p99={}us (budget {}ms) {}",
            kind_label(kind),
            buildings,
            p50_us,
            p99_us,
            budget_ms,
            if p99_ms <= budget_ms { "OK" } else { "OVER" }
        );
    }
    Ok(EXIT_PASS)
}

fn bench_power(buildings: usize, ticks: u64, warmup: u64) -> (Vec<u64>, serde_json::Value) {
    let mut world = World::new();
    world.insert_resource(BuildRules::default());
    let mut grid = WorldGrid::new(128, 64);
    let mut graphs = PowerGrids::new();
    let groups = (buildings / 10).max(1);
    let mut placed = 0usize;
    for group in 0..groups {
        let id = graphs.alloc();
        for slot in 0..5 {
            let x = ((group % 40) * 2) as i16;
            let y = (group / 40) as i16 * 2 + slot as i16;
            let entity = spawn_power(&mut world, &mut grid, x, y, PowerNodeInfo::producer(), 10.0);
            graphs.add(&mut world, id, entity);
            placed += 1;
        }
        for slot in 0..5 {
            let x = ((group % 40) * 2 + 1) as i16;
            let y = (group / 40) as i16 * 2 + slot as i16;
            let entity = spawn_power(
                &mut world,
                &mut grid,
                x,
                y,
                PowerNodeInfo::consumer(5.0),
                0.0,
            );
            graphs.add(&mut world, id, entity);
            placed += 1;
        }
    }
    for _ in 0..warmup {
        graphs.update_all(&mut world, 1.0);
    }
    let mut samples = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        let start = Instant::now();
        graphs.update_all(&mut world, 1.0);
        samples.push(start.elapsed().as_nanos() as u64 / 1000);
    }
    (
        samples,
        serde_json::json!({"placed": placed, "graphs": graphs.graph_count(), "method": "PowerGrids::update_all"}),
    )
}

fn bench_liquid(buildings: usize, ticks: u64, warmup: u64) -> (Vec<u64>, serde_json::Value) {
    const COLS: usize = 50;
    let rows = buildings.div_ceil(COLS).max(1);
    let mut grid = WorldGrid::new((COLS * 2) as i32, rows as i32);
    let mut world = World::new();
    world.insert_resource(BuildRules::default());
    let water = LiquidId::WATER;
    let slots = (water.index() + 1).max(2);
    let mut conduits: Vec<Entity> = Vec::with_capacity(buildings);
    for index in 0..buildings {
        let col = index % COLS;
        let row = index / COLS;
        let x = (col * 2) as i16;
        let y = row as i16;
        conduits.push(spawn_liquid(
            &mut world,
            &mut grid,
            x,
            y,
            0,
            liquid_node(10.0, true),
            slots,
        ));
        let _ = spawn_liquid(
            &mut world,
            &mut grid,
            x + 1,
            y,
            0,
            liquid_node(1.0e9, true),
            slots,
        );
    }
    let refill = |world: &mut World| {
        for &conduit in &conduits {
            if let Some(mut module) = world.get_mut::<LiquidModule>(conduit) {
                module.liquids[water.index()] = 10.0;
                module.current_amount = 10.0;
            }
        }
    };
    for _ in 0..warmup {
        refill(&mut world);
        for &conduit in &conduits {
            update_conduit(&mut world, &grid, conduit);
        }
    }
    let mut samples = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        refill(&mut world);
        let start = Instant::now();
        for &conduit in &conduits {
            update_conduit(&mut world, &grid, conduit);
        }
        samples.push(start.elapsed().as_nanos() as u64 / 1000);
    }
    (
        samples,
        serde_json::json!({"placed": buildings * 2, "method": "update_conduit (conduit -> large tank, refilled pre-timing)"}),
    )
}

fn bench_heat(buildings: usize, ticks: u64, warmup: u64) -> (Vec<u64>, serde_json::Value) {
    const COLS: usize = 50;
    let rows = buildings.div_ceil(COLS).max(1);
    let mut grid = WorldGrid::new((COLS * 2) as i32, rows as i32);
    let mut world = World::new();
    world.insert_resource(BuildRules::default());
    let mut pairs: Vec<(Entity, Entity)> = Vec::with_capacity(buildings);
    for index in 0..buildings {
        let col = index % COLS;
        let row = index / COLS;
        let x = (col * 2) as i16;
        let y = row as i16;
        let producer = spawn_heat(
            &mut world,
            &mut grid,
            x,
            y,
            HeatState {
                heat: 10.0,
                heat_progress: 0.0,
                heat_output: 10.0,
                rotate: false,
            },
        );
        let conductor = spawn_heat(
            &mut world,
            &mut grid,
            x + 1,
            y,
            HeatState {
                heat: 0.0,
                heat_progress: 0.0,
                heat_output: 0.0,
                rotate: false,
            },
        );
        world.entity_mut(conductor).insert(HeatConductor::default());
        link(&mut world, producer, conductor);
        pairs.push((producer, conductor));
    }
    // Reused scratch mirrors `HeatScratch` (capacity retained across ticks);
    // using `crafter_heat` per call would allocate an `IdSet` every tick.
    let mut side = [0.0f32; 4];
    let mut scratch = mind_core::util::IdSet::new();
    for tick in 0..warmup {
        for &(_, conductor) in &pairs {
            calculate_heat(&mut world, conductor, &mut side, &mut scratch, tick, 0);
        }
    }
    let mut samples = Vec::with_capacity(ticks as usize);
    for tick in 0..ticks {
        let start = Instant::now();
        for &(_, conductor) in &pairs {
            calculate_heat(
                &mut world,
                conductor,
                &mut side,
                &mut scratch,
                warmup + tick,
                0,
            );
        }
        samples.push(start.elapsed().as_nanos() as u64 / 1000);
    }
    (
        samples,
        serde_json::json!({"placed": buildings * 2, "method": "calculate_heat (producer -> conductor, reused scratch)"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_merge_invariants_hold() {
        let run = split_merge_run().expect("split/merge");
        assert_eq!(run.report["pass"], true);
        assert_eq!(run.states.len(), 3);
    }

    #[test]
    fn battery_cycle_runs() {
        let out = power_battery_cycle().expect("battery cycle");
        assert!(out.report["pass"].as_bool().unwrap_or(false));
    }

    #[test]
    fn conduit_transfer_runs() {
        let out = liquid_conduit_transfer().expect("conduit transfer");
        assert!(out.report["pass"].as_bool().unwrap_or(false));
    }

    #[test]
    fn heat_equilibrium_runs() {
        let out = heat_network_equilibrium().expect("heat equilibrium");
        assert!(out.report["pass"].as_bool().unwrap_or(false));
    }

    #[test]
    fn determinism_is_stable() {
        let out = power_determinism().expect("determinism");
        assert!(out.report["pass"].as_bool().unwrap_or(false));
    }

    #[test]
    fn committed_goldens_match() {
        let base = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/network");
        let cases = [
            (
                NetworkKind::Power,
                "power_battery_cycle",
                "power_battery_cycle.json",
            ),
            (
                NetworkKind::Power,
                "power_graph_split_merge",
                "power_graph_split_merge.json",
            ),
            (
                NetworkKind::Power,
                "power_network_determinism",
                "power_network_determinism.checksums",
            ),
            (
                NetworkKind::Liquid,
                "liquid_conduit_transfer",
                "liquid_conduit_transfer.json",
            ),
            (
                NetworkKind::Heat,
                "heat_network_equilibrium",
                "heat_network_equilibrium.json",
            ),
        ];
        for (kind, name, file) in cases {
            let out = run_scenario(kind, name).unwrap_or_else(|error| panic!("{name}: {error}"));
            let golden = std::fs::read_to_string(format!("{base}/{file}"))
                .unwrap_or_else(|error| panic!("{file}: {error}"));
            assert_eq!(out.dump, golden, "golden mismatch for {name}");
        }
    }
}
