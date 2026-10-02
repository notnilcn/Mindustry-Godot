// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Ported power-graph tests (`tests/src/test/java/power/PowerTests.java`,
//! `DirectConsumerTests.java`) plus topology invariants.
//!
//! Deviation §2.3.1: the Java fixture fixes `Time.delta = 0.5`; the port passes
//! an explicit `delta` to `update`, and the expected values are recomputed
//! Rust-vs-Rust. Every dynamic case is retained.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockId;
use crate::entities::comp::{Building, PowerGraphUpdater, TeamComp};
use crate::world::modules::PowerModule;
use crate::world::{TilePos, WorldGrid};

use super::graph::PowerGrids;
use super::{PowerNodeInfo, PowerProduction};

/// Fixture delta matching `PowerTestFixture`'s `Time.setDeltaProvider(() -> 0.5f)`.
const DELTA: f32 = 0.5;

struct Fixture {
    world: World,
    grid: WorldGrid,
    graphs: PowerGrids,
}

impl Fixture {
    fn new() -> Self {
        Self {
            world: World::new(),
            grid: WorldGrid::new(16, 16),
            graphs: PowerGrids::new(),
        }
    }

    fn spawn(
        &mut self,
        x: i16,
        y: i16,
        info: PowerNodeInfo,
        production: f32,
        status: f32,
    ) -> Entity {
        let entity = self
            .world
            .spawn((
                Building::new(TilePos::new(x, y), BlockId::AIR, 0),
                TeamComp { team: 0 },
                PowerModule::new(),
                info,
                PowerProduction(production),
            ))
            .id();
        {
            let mut module = self
                .world
                .get_mut::<PowerModule>(entity)
                .expect("power module");
            module.status = status;
        }
        entity
    }

    fn link(&mut self, a: Entity, b: Entity) {
        let mut building = self.world.get_mut::<Building>(a).expect("a");
        if !building.proximity.contains(&b) {
            building.proximity.push(b);
        }
        let mut other = self.world.get_mut::<Building>(b).expect("b");
        if !other.proximity.contains(&a) {
            other.proximity.push(a);
        }
    }

    fn status(&self, entity: Entity) -> f32 {
        self.world
            .get::<PowerModule>(entity)
            .map(|module| module.status)
            .unwrap_or(0.0)
    }

    fn updater_count(&self) -> usize {
        self.world
            .iter_entities()
            .filter(|entity| entity.get::<PowerGraphUpdater>().is_some())
            .count()
    }
}

fn assert_close(actual: f32, expected: f32, context: &str) {
    assert!(
        (actual - expected).abs() < 0.000_01,
        "{context}: expected {expected}, got {actual}"
    );
}

#[test]
fn node_info_constructors_are_consistent() {
    let producer = PowerNodeInfo::producer();
    assert!(producer.outputs && !producer.consumes);
    let consumer = PowerNodeInfo::consumer(0.5);
    assert!(!consumer.outputs && consumer.consumes && consumer.usage == 0.5);
    let battery = PowerNodeInfo::battery(4000.0);
    assert!(battery.outputs && battery.consumes && battery.buffered);
    assert_eq!(battery.capacity, 4000.0);
}

/// `PowerTests.directConsumerSatisfactionIsAsExpected` (7 dynamic cases).
#[test]
fn direct_consumer_satisfaction_is_as_expected() {
    let cases = [
        (0.0f32, 1.0f32, 0.0f32),
        (0.0, 0.0, 0.0),
        (1.0, 0.0, 1.0),
        (1.0, 1.0, 1.0),
        (0.5, 1.0, 0.5),
        (1.0, 0.5, 1.0),
        (0.09, 0.09 - f32::EPSILON / 10.0, 1.0),
    ];
    for (produced_power, required_power, expected_satisfaction) in cases {
        let mut fixture = Fixture::new();
        let producer = fixture.spawn(0, 0, PowerNodeInfo::producer(), produced_power, 0.0);
        let consumer = fixture.spawn(0, 1, PowerNodeInfo::consumer(required_power), 0.0, 0.0);
        let graph = fixture.graphs.alloc();
        fixture.graphs.add(&mut fixture.world, graph, producer);
        fixture.graphs.add(&mut fixture.world, graph, consumer);

        let produced = fixture
            .graphs
            .graph(graph)
            .expect("graph")
            .get_power_produced(&fixture.world, DELTA);
        let needed = fixture
            .graphs
            .graph(graph)
            .expect("graph")
            .get_power_needed(&fixture.world, DELTA);
        assert_close(produced, produced_power * DELTA, "produced power");
        assert_close(needed, required_power * DELTA, "needed power");

        fixture.graphs.update(&mut fixture.world, graph, DELTA);
        assert_close(
            fixture.status(consumer),
            expected_satisfaction,
            "consumer satisfaction",
        );
    }
}

/// `PowerTests.batteryCapacityIsAsExpected` (9 dynamic cases).
#[test]
fn battery_capacity_is_as_expected() {
    let cases = [
        (10.0f32, 0.0f32, 0.0f32, 5.0f32, 0.0f32),
        (10.0, 0.0, 94.999, 99.999, 0.0),
        (10.0, 0.0, 100.0, 100.0, 0.0),
        (0.0, 0.0, 0.0, 0.0, 0.0),
        (0.0, 0.0, 100.0, 100.0, 0.0),
        (0.0, 10.0, 0.0, 0.0, 0.0),
        (0.0, 10.0, 100.0, 95.0, 1.0),
        (0.0, 10.0, 2.5, 0.0, 0.5),
        (5.0, 10.0, 5.0, 2.5, 1.0),
    ];
    let max_capacity = 100.0f32;
    for (produced_power, requested_power, initial, expected_capacity, expected_satisfaction) in
        cases
    {
        let mut fixture = Fixture::new();
        let graph = fixture.graphs.alloc();
        if produced_power > 0.0 {
            let producer = fixture.spawn(0, 0, PowerNodeInfo::producer(), produced_power, 0.0);
            fixture.graphs.add(&mut fixture.world, graph, producer);
        }
        let mut consumer = None;
        if requested_power > 0.0 {
            let entity = fixture.spawn(0, 1, PowerNodeInfo::consumer(requested_power), 0.0, 0.0);
            fixture.graphs.add(&mut fixture.world, graph, entity);
            consumer = Some(entity);
        }
        let battery = fixture.spawn(
            0,
            2,
            PowerNodeInfo::battery(max_capacity),
            0.0,
            initial / max_capacity,
        );
        fixture.graphs.add(&mut fixture.world, graph, battery);

        fixture.graphs.update(&mut fixture.world, graph, DELTA);

        assert_close(
            fixture.status(battery) * max_capacity,
            expected_capacity,
            "battery capacity",
        );
        if let Some(consumer) = consumer {
            assert_close(
                fixture.status(consumer),
                expected_satisfaction,
                "consumer satisfaction",
            );
        }
    }
}

/// `PowerTests.directConsumptionStopsWithNoPower`.
#[test]
fn direct_consumption_stops_without_power() {
    let mut fixture = Fixture::new();
    let producer = fixture.spawn(0, 0, PowerNodeInfo::producer(), 10.0, 0.0);
    let consumer = fixture.spawn(0, 1, PowerNodeInfo::consumer(5.0), 0.0, 0.0);
    let graph = fixture.graphs.alloc();
    fixture.graphs.add(&mut fixture.world, graph, producer);
    fixture.graphs.add(&mut fixture.world, graph, consumer);

    fixture.graphs.update(&mut fixture.world, graph, 1.0);
    assert_close(fixture.status(consumer), 1.0, "initial status");

    fixture.graphs.remove_list(graph, producer);
    fixture.graphs.add(&mut fixture.world, graph, consumer);
    fixture.graphs.update(&mut fixture.world, graph, 1.0);
    assert_close(fixture.status(consumer), 0.0, "status after removal");
}

/// New topology invariant: an absorbed graph's larger side wins.
#[test]
fn merge_prefers_larger() {
    let mut fixture = Fixture::new();
    let a = fixture.spawn(0, 0, PowerNodeInfo::producer(), 1.0, 0.0);
    let b = fixture.spawn(1, 0, PowerNodeInfo::producer(), 1.0, 0.0);
    let c = fixture.spawn(2, 0, PowerNodeInfo::producer(), 1.0, 0.0);
    fixture.link(a, b);
    fixture.link(b, c);

    let small = fixture.graphs.alloc();
    fixture.graphs.add(&mut fixture.world, small, a);
    let large = fixture.graphs.alloc();
    fixture.graphs.add(&mut fixture.world, large, b);
    fixture.graphs.add(&mut fixture.world, large, c);

    fixture.graphs.add_graph(&mut fixture.world, small, large);

    assert_eq!(fixture.graphs.graph_count(), 1);
    let surviving = fixture
        .world
        .get::<PowerModule>(a)
        .map(|module| module.graph)
        .expect("a graph");
    assert!(fixture.graphs.contains(surviving));
    assert_eq!(
        fixture.graphs.graph(surviving).map(|graph| graph.all.len()),
        Some(3)
    );
    assert_eq!(fixture.updater_count(), 1);
}

/// New topology invariant: breaking the middle node splits the line 1-1.
#[test]
fn split_reassigns_all_members() {
    let mut fixture = Fixture::new();
    let a = fixture.spawn(0, 0, PowerNodeInfo::producer(), 1.0, 0.0);
    let b = fixture.spawn(1, 0, PowerNodeInfo::producer(), 1.0, 0.0);
    let c = fixture.spawn(2, 0, PowerNodeInfo::producer(), 1.0, 0.0);
    fixture.link(a, b);
    fixture.link(b, c);

    let graph = fixture.graphs.alloc();
    fixture.graphs.add(&mut fixture.world, graph, a);
    fixture.graphs.add(&mut fixture.world, graph, b);
    fixture.graphs.add(&mut fixture.world, graph, c);

    fixture.graphs.remove(&mut fixture.world, &fixture.grid, b);

    let graph_a = fixture
        .world
        .get::<PowerModule>(a)
        .map(|module| module.graph)
        .expect("a");
    let graph_c = fixture
        .world
        .get::<PowerModule>(c)
        .map(|module| module.graph)
        .expect("c");
    assert_ne!(graph_a, graph_c);
    assert!(graph_a != graph);
    assert!(graph_c != graph);
    assert_eq!(fixture.graphs.graph_count(), 2);
    assert_eq!(fixture.updater_count(), 2);
}

/// New topology invariant: reflow handles a triangle without looping.
#[test]
fn reflow_terminates_on_cycle() {
    let mut fixture = Fixture::new();
    let a = fixture.spawn(0, 0, PowerNodeInfo::producer(), 1.0, 0.0);
    let b = fixture.spawn(1, 0, PowerNodeInfo::producer(), 1.0, 0.0);
    let c = fixture.spawn(0, 1, PowerNodeInfo::producer(), 1.0, 0.0);
    fixture.link(a, b);
    fixture.link(b, c);
    fixture.link(c, a);

    fixture.graphs.reflow(&mut fixture.world, &fixture.grid, a);
    assert_eq!(fixture.graphs.graph_count(), 1);
    let graph = fixture
        .world
        .get::<PowerModule>(a)
        .map(|module| module.graph)
        .expect("a graph");
    let all = fixture
        .graphs
        .graph(graph)
        .map(|graph| graph.all.len())
        .unwrap_or(0);
    assert_eq!(all, 3);
    assert_eq!(fixture.updater_count(), 1);
}

/// New topology invariant: the graph updater entity count equals graph count.
#[test]
fn graph_updater_entity_lifecycle() {
    let mut fixture = Fixture::new();
    let a = fixture.spawn(0, 0, PowerNodeInfo::producer(), 1.0, 0.0);
    let graph = fixture.graphs.alloc();
    fixture.graphs.add(&mut fixture.world, graph, a);
    fixture.graphs.check_add(&mut fixture.world, graph);
    assert_eq!(fixture.updater_count(), 1);

    fixture.graphs.clear(&mut fixture.world, graph);
    assert_eq!(fixture.updater_count(), 0);
    assert_eq!(fixture.graphs.graph(graph).map(|g| g.all.len()), Some(0));
}

/// `PowerTests` graph ordering: the per-tick system updates members in list order.
#[test]
fn update_uses_list_order() {
    let mut fixture = Fixture::new();
    let producer = fixture.spawn(0, 0, PowerNodeInfo::producer(), 4.0, 0.0);
    let consumer_a = fixture.spawn(1, 0, PowerNodeInfo::consumer(1.0), 0.0, 0.0);
    let consumer_b = fixture.spawn(2, 0, PowerNodeInfo::consumer(1.0), 0.0, 0.0);
    let graph = fixture.graphs.alloc();
    fixture.graphs.add(&mut fixture.world, graph, producer);
    fixture.graphs.add(&mut fixture.world, graph, consumer_a);
    fixture.graphs.add(&mut fixture.world, graph, consumer_b);
    fixture.graphs.update_all(&mut fixture.world, 1.0);
    assert_eq!(
        fixture
            .graphs
            .graph(graph)
            .map(|graph| graph.consumers.clone()),
        Some(smallvec::smallvec![consumer_a, consumer_b])
    );
    assert_close(fixture.status(consumer_a), 1.0, "a");
    assert_close(fixture.status(consumer_b), 1.0, "b");
}
