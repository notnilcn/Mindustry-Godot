// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PowerTestFixture` port (`tests/src/test/java/power/PowerTestFixture.java`).
//!
//! `FakeProducer` (a `PowerGenerator` with `powerProduction`), `FakeBattery`
//! (`consumePowerBuffered(capacity)`), `FakeDirectConsumer` (`consumePower`) and
//! `PowerHarness` (the graph + world). Names use a deterministic counter instead
//! of `System.nanoTime()`; graph functions take an explicit `delta` (D8), so the
//! Java `2/1/0.5` cases remain expressible.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockId;
use crate::entities::comp::{Building, TeamComp};
use crate::world::TilePos;
use crate::world::blocks::power::graph::PowerGrids;
use crate::world::blocks::power::{PowerNodeInfo, PowerProduction};
use crate::world::modules::{PowerGraphId, PowerModule};

/// A fake power producer (`createFakeProducerBlock`).
pub struct FakeProducer;

/// A fake buffered power store (`createFakeBattery`).
pub struct FakeBattery;

/// A fake unbuffered consumer (`createFakeDirectConsumer`).
pub struct FakeDirectConsumer;

/// Deterministic fixture harness over a single power graph.
pub struct PowerHarness {
    /// The ECS world owning the fake buildings.
    pub world: World,
    /// The power-graph arena.
    pub graphs: PowerGrids,
    /// The graph every fake building is added to.
    pub graph: PowerGraphId,
    next_x: i16,
}

impl Default for PowerHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerHarness {
    /// Creates an empty harness with one graph.
    pub fn new() -> Self {
        let mut graphs = PowerGrids::new();
        let graph = graphs.alloc();
        Self {
            world: World::new(),
            graphs,
            graph,
            next_x: 0,
        }
    }

    fn next_tile(&mut self) -> TilePos {
        let x = self.next_x;
        self.next_x = self.next_x.saturating_add(1);
        TilePos::new(x, 0)
    }

    fn spawn(&mut self, info: PowerNodeInfo, production: f32, status: f32) -> Entity {
        let tile = self.next_tile();
        let entity = self
            .world
            .spawn((
                Building::new(tile, BlockId::AIR, 0),
                TeamComp { team: 0 },
                PowerModule::new(),
                info,
                PowerProduction(production),
            ))
            .id();
        if let Some(mut module) = self.world.get_mut::<PowerModule>(entity) {
            module.status = status;
        }
        self.graphs.add(&mut self.world, self.graph, entity);
        entity
    }

    /// `createFakeProducerBlock(producedPower)`: `productionEfficiency = 1`.
    pub fn producer(&mut self, produced_power: f32) -> Entity {
        self.spawn(PowerNodeInfo::producer(), produced_power, 0.0)
    }

    /// `createFakeBattery(capacity)`: starts empty.
    pub fn battery(&mut self, capacity: f32) -> Entity {
        self.spawn(PowerNodeInfo::battery(capacity), 0.0, 0.0)
    }

    /// `createFakeBattery` with an initial stored amount.
    pub fn battery_with_stored(&mut self, capacity: f32, stored: f32) -> Entity {
        let status = if capacity > 0.0 {
            stored / capacity
        } else {
            0.0
        };
        self.spawn(PowerNodeInfo::battery(capacity), 0.0, status)
    }

    /// `createFakeDirectConsumer(powerPerTick)`.
    pub fn direct_consumer(&mut self, power_per_tick: f32) -> Entity {
        self.spawn(PowerNodeInfo::consumer(power_per_tick), 0.0, 0.0)
    }

    /// Adds an already-spawned fake with a custom classification.
    pub fn add_with(&mut self, info: PowerNodeInfo, production: f32, status: f32) -> Entity {
        self.spawn(info, production, status)
    }

    /// Runs `PowerGraph.update` with the explicit fixture `delta`.
    pub fn update(&mut self, delta: f32) {
        self.graphs.update(&mut self.world, self.graph, delta);
    }

    /// Reads a fake's power status.
    pub fn status(&self, entity: Entity) -> f32 {
        self.world
            .get::<PowerModule>(entity)
            .map(|module| module.status)
            .unwrap_or(0.0)
    }

    /// The graph's produced power for `delta`.
    pub fn produced(&self, delta: f32) -> f32 {
        self.graphs
            .graph(self.graph)
            .map(|graph| graph.get_power_produced(&self.world, delta))
            .unwrap_or(0.0)
    }

    /// The graph's needed power for `delta`.
    pub fn needed(&self, delta: f32) -> f32 {
        self.graphs
            .graph(self.graph)
            .map(|graph| graph.get_power_needed(&self.world, delta))
            .unwrap_or(0.0)
    }

    /// `removeList` for a fake (fixture-only).
    pub fn remove_list(&mut self, entity: Entity) {
        self.graphs.remove_list(self.graph, entity);
    }

    /// Re-adds a fake to the graph (fixture-only).
    pub fn add(&mut self, entity: Entity) {
        self.graphs.add(&mut self.world, self.graph, entity);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn producer_consumer_satisfaction() {
        let mut harness = PowerHarness::new();
        let _producer = harness.producer(10.0);
        let consumer = harness.direct_consumer(5.0);
        harness.update(1.0);
        assert_eq!(harness.produced(1.0), 10.0);
        assert_eq!(harness.needed(1.0), 5.0);
        assert_eq!(harness.status(consumer), 1.0);
    }

    #[test]
    fn battery_stores_and_discharges() {
        let mut harness = PowerHarness::new();
        harness.producer(10.0);
        let battery = harness.battery(100.0);
        harness.update(0.5);
        assert_eq!(harness.status(battery), 0.05);
    }

    #[test]
    fn fake_marker_types_exist() {
        let _ = (FakeProducer, FakeBattery, FakeDirectConsumer);
    }
}
