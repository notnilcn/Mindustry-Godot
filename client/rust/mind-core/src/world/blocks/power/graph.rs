// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PowerGraph` + `PowerGraphs` arena
//! (`core/src/mindustry/world/blocks/power/PowerGraph.java`).
//!
//! Java's identity-referenced `PowerGraph` objects become
//! [`PowerGraphId`] handles into a [`PowerGrids`] arena (plan 09 §2.3.2): the
//! arena never hands out a stale id (generation counters + a free list), and the
//! per-graph static BFS scratch becomes the shared [`PowerScratch`] resource
//! (plan 09 §2.3.3). Every order/merge/split rule is ported exactly; the only
//! deviation is the explicit `delta` parameter (D8).
//!
//! `update()` keeps Java's re-entrant `graph.update()` during `remove()`
//! (risk R8): callers must hold exclusive world access, which plan 07's
//! placement/break path guarantees.

use std::collections::VecDeque;

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::entities::comp::Building;
use crate::math::WindowedMean;
use crate::util::IdSet;
use crate::world::WorldGrid;
use crate::world::modules::{PowerGraphId, PowerModule};

use super::module::{get_power_connections, pack, read_power_info};

/// Java `Mathf.zero` epsilon (`Math.abs(f) < 0.000001f`).
fn zero(value: f32) -> bool {
    value.abs() < 0.000_001
}

/// Java `Mathf.equal(a, b)`.
fn equal(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.000_001
}

/// Reusable BFS/topology scratch (`PowerGraph.queue/outArray1/outArray2/closedSet`).
#[derive(Debug, Default)]
pub struct PowerScratch {
    /// FIFO BFS queue.
    pub queue: VecDeque<Entity>,
    /// First output buffer (`outArray1`).
    pub out1: Vec<Entity>,
    /// Second output buffer (`outArray2`).
    pub out2: Vec<Entity>,
    /// Temporary buffer (Rust adaptation for disjoint borrows).
    pub temp: Vec<Entity>,
    /// Closed set keyed by packed tile position (`closedSet`).
    pub closed: IdSet,
}

/// One live power network (`PowerGraph`).
#[derive(Debug)]
pub struct PowerGraph {
    /// Monotonic debug id (mirrors `PowerGraph.lastGraphID`).
    pub debug_id: u32,
    /// Producers (`outputsPower` only).
    pub producers: SmallVec<[Entity; 16]>,
    /// Unbuffered consumers (`consumesPower` only).
    pub consumers: SmallVec<[Entity; 16]>,
    /// Buffered consumers/batteries.
    pub batteries: SmallVec<[Entity; 16]>,
    /// Every member.
    pub all: SmallVec<[Entity; 16]>,
    /// Spawned `PowerGraphUpdater` entity, if any.
    pub updater: Option<Entity>,
    power_balance: WindowedMean,
    last_power_produced: f32,
    last_power_needed: f32,
    last_power_stored: f32,
    last_scaled_power_in: f32,
    last_scaled_power_out: f32,
    last_capacity: f32,
    energy_delta: f32,
}

impl PowerGraph {
    /// Creates an empty graph with the given debug id.
    pub fn new(debug_id: u32) -> Self {
        Self {
            debug_id,
            producers: SmallVec::new(),
            consumers: SmallVec::new(),
            batteries: SmallVec::new(),
            all: SmallVec::new(),
            updater: None,
            power_balance: WindowedMean::new(60),
            last_power_produced: 0.0,
            last_power_needed: 0.0,
            last_power_stored: 0.0,
            last_scaled_power_in: 0.0,
            last_scaled_power_out: 0.0,
            last_capacity: 0.0,
            energy_delta: 0.0,
        }
    }

    /// `getLastScaledPowerIn`.
    pub fn last_scaled_power_in(&self) -> f32 {
        self.last_scaled_power_in
    }

    /// `getLastScaledPowerOut`.
    pub fn last_scaled_power_out(&self) -> f32 {
        self.last_scaled_power_out
    }

    /// `getLastCapacity`.
    pub fn last_capacity(&self) -> f32 {
        self.last_capacity
    }

    /// `getPowerBalance` (`rawMean`).
    pub fn power_balance(&self) -> f32 {
        self.power_balance.raw_mean()
    }

    /// `hasPowerBalanceSamples`.
    pub fn has_power_balance_samples(&self) -> bool {
        self.power_balance.has_enough_data()
    }

    /// `getLastPowerNeeded`.
    pub fn last_power_needed(&self) -> f32 {
        self.last_power_needed
    }

    /// `getLastPowerProduced`.
    pub fn last_power_produced(&self) -> f32 {
        self.last_power_produced
    }

    /// `getLastPowerStored`.
    pub fn last_power_stored(&self) -> f32 {
        self.last_power_stored
    }

    /// `energyDelta` (diode workaround).
    pub fn energy_delta(&self) -> f32 {
        self.energy_delta
    }

    /// `getSatisfaction`.
    pub fn get_satisfaction(&self) -> f32 {
        if zero(self.last_power_produced) {
            0.0
        } else if zero(self.last_power_needed) {
            1.0
        } else {
            (self.last_power_produced / self.last_power_needed).clamp(0.0, 1.0)
        }
    }

    /// `getPowerProduced`: Σ `getPowerProduction() * build.delta()`.
    pub fn get_power_produced(&self, world: &World, delta: f32) -> f32 {
        let mut produced = 0.0;
        for &producer in &self.producers {
            if let Some(params) = read_power_info(world, producer) {
                produced += params.production * (delta * params.time_scale);
            }
        }
        produced
    }

    /// `getPowerNeeded`: Σ `requestedPower * build.delta()` for active consumers.
    pub fn get_power_needed(&self, world: &World, delta: f32) -> f32 {
        let mut needed = 0.0;
        for &consumer in &self.consumers {
            let Some(params) = read_power_info(world, consumer) else {
                continue;
            };
            if !params.should_consume_power {
                continue;
            }
            let status = world
                .get::<PowerModule>(consumer)
                .map(|module| module.status)
                .unwrap_or(0.0);
            let requested = if params.buffered {
                (1.0 - status) * params.capacity
            } else {
                params.usage * if params.enabled { 1.0 } else { 0.0 }
            };
            needed += requested * (delta * params.time_scale);
        }
        needed
    }

    /// `getBatteryStored`.
    pub fn get_battery_stored(&self, world: &World) -> f32 {
        let mut total = 0.0;
        for &battery in &self.batteries {
            let enabled = world
                .get::<Building>(battery)
                .map(|building| building.enabled)
                .unwrap_or(false);
            if enabled
                && let (Some(params), Some(module)) = (
                    read_power_info(world, battery),
                    world.get::<PowerModule>(battery),
                )
            {
                total += module.status * params.capacity;
            }
        }
        total
    }

    /// `getBatteryCapacity` (missing capacity only).
    pub fn get_battery_capacity(&self, world: &World) -> f32 {
        let mut total = 0.0;
        for &battery in &self.batteries {
            let enabled = world
                .get::<Building>(battery)
                .map(|building| building.enabled)
                .unwrap_or(false);
            if enabled
                && let (Some(params), Some(module)) = (
                    read_power_info(world, battery),
                    world.get::<PowerModule>(battery),
                )
            {
                total += (1.0 - module.status) * params.capacity;
            }
        }
        total
    }

    /// `getTotalBatteryCapacity`.
    pub fn get_total_battery_capacity(&self, world: &World) -> f32 {
        let mut total = 0.0;
        for &battery in &self.batteries {
            let enabled = world
                .get::<Building>(battery)
                .map(|building| building.enabled)
                .unwrap_or(false);
            if enabled && let Some(params) = read_power_info(world, battery) {
                total += params.capacity;
            }
        }
        total
    }

    /// `useBatteries`.
    pub fn use_batteries(&self, world: &mut World, needed: f32) -> f32 {
        let stored = self.get_battery_stored(world);
        if zero(stored) {
            return 0.0;
        }
        let used = stored.min(needed);
        let consumed_percentage = (needed / stored).min(1.0);
        for &battery in &self.batteries {
            let enabled = world
                .get::<Building>(battery)
                .map(|building| building.enabled)
                .unwrap_or(false);
            if enabled && let Some(mut module) = world.get_mut::<PowerModule>(battery) {
                module.status *= 1.0 - consumed_percentage;
            }
        }
        used
    }

    /// `chargeBatteries` (note: the `chargedPercent` division happens *before*
    /// the zero-capacity guard, exactly as upstream).
    pub fn charge_batteries(&self, world: &mut World, excess: f32) -> f32 {
        let capacity = self.get_battery_capacity(world);
        let charged_percent = (excess / capacity).min(1.0);
        if zero(capacity) {
            return 0.0;
        }
        for &battery in &self.batteries {
            let enabled = world
                .get::<Building>(battery)
                .map(|building| building.enabled)
                .unwrap_or(false);
            let battery_capacity = read_power_info(world, battery)
                .map(|params| params.capacity)
                .unwrap_or(0.0);
            if enabled
                && battery_capacity > 0.0
                && let Some(mut module) = world.get_mut::<PowerModule>(battery)
            {
                module.status += (1.0 - module.status) * charged_percent;
            }
        }
        excess.min(capacity)
    }

    /// `transferPower`.
    pub fn transfer_power(&mut self, world: &mut World, amount: f32) {
        if amount > 0.0 {
            self.charge_batteries(world, amount);
        } else {
            self.use_batteries(world, -amount);
        }
        self.energy_delta += amount;
    }

    /// `distributePower`.
    pub fn distribute_power(
        &mut self,
        world: &mut World,
        needed: f32,
        produced: f32,
        charged: bool,
        delta: f32,
    ) {
        let coverage = if zero(needed) && zero(produced) && !charged && zero(self.last_power_stored)
        {
            0.0
        } else if zero(needed) {
            1.0
        } else {
            (produced / needed).min(1.0)
        };

        for &consumer in &self.consumers {
            let Some(params) = read_power_info(world, consumer) else {
                continue;
            };
            let Some(mut module) = world.get_mut::<PowerModule>(consumer) else {
                continue;
            };
            if params.buffered {
                if !zero(params.capacity) {
                    let requested = (1.0 - module.status) * params.capacity;
                    let maximum_rate = requested * coverage * (delta * params.time_scale);
                    module.status =
                        (module.status + maximum_rate / params.capacity).clamp(0.0, 1.0);
                }
            } else if params.should_consume_power {
                module.status = coverage;
            } else {
                let denominator = needed + params.usage * (delta * params.time_scale);
                let status = if zero(denominator) {
                    0.0
                } else {
                    (produced / denominator).min(1.0)
                };
                module.status = if status.is_nan() { 0.0 } else { status };
            }
        }
    }

    /// `update()` with the explicit fixed-step `delta` (D8).
    pub fn update(&mut self, world: &mut World, delta: f32) {
        // When cheating, just set status to 1.
        if let Some(&first) = self.consumers.first()
            && read_cheating(world, first)
        {
            for &consumer in &self.consumers {
                set_status(world, consumer, 1.0);
            }
            self.last_power_needed = 1.0;
            self.last_power_produced = 1.0;
            return;
        }

        let power_needed = self.get_power_needed(world, delta);
        let power_produced = self.get_power_produced(world, delta);

        self.last_power_needed = power_needed;
        self.last_power_produced = power_produced;

        let step = if delta == 0.0 { 1.0 } else { delta };
        self.last_scaled_power_in = (power_produced + self.energy_delta) / step;
        self.last_scaled_power_out = power_needed / step;
        self.last_capacity = self.get_total_battery_capacity(world);
        self.last_power_stored = self.get_battery_stored(world);

        self.power_balance
            .add((self.last_power_produced - self.last_power_needed + self.energy_delta) / step);
        self.energy_delta = 0.0;

        if !(self.consumers.is_empty() && self.producers.is_empty() && self.batteries.is_empty()) {
            let mut charged = false;
            let mut produced = power_produced;

            if !equal(power_needed, power_produced) {
                if power_needed > power_produced {
                    let used = self.use_batteries(world, power_needed - power_produced);
                    produced += used;
                    self.last_power_produced += used;
                } else if power_produced > power_needed {
                    charged = true;
                    produced -= self.charge_batteries(world, power_produced - power_needed);
                }
            }

            self.distribute_power(world, power_needed, produced, charged, delta);
        }
    }
}

/// Sets a building's power status if the module exists.
pub fn set_status(world: &mut World, entity: Entity, status: f32) {
    if let Some(mut module) = world.get_mut::<PowerModule>(entity) {
        module.status = status;
    }
}

/// `TeamComp.cheating()` for a building (plan 12 owns the full predicate).
pub fn read_cheating(world: &World, entity: Entity) -> bool {
    let _ = entity;
    world
        .get_resource::<crate::world::limits::BuildRules>()
        .is_some_and(|rules| rules.cheat)
}

/// The power-graph arena resource (`PowerGraphs`).
#[derive(Debug, Resource)]
pub struct PowerGrids {
    graphs: Vec<Option<(u32, PowerGraph)>>,
    free_slots: Vec<u32>,
    /// Reusable BFS scratch.
    pub scratch: PowerScratch,
    next_graph_id: u32,
}

impl Default for PowerGrids {
    fn default() -> Self {
        Self::new()
    }
}

impl PowerGrids {
    /// Creates an empty arena.
    pub fn new() -> Self {
        Self {
            graphs: Vec::new(),
            free_slots: Vec::new(),
            scratch: PowerScratch::default(),
            next_graph_id: 0,
        }
    }

    /// Back-compat alias matching the plan's `PowerGraphs` name.
    pub fn graphs(&self) -> usize {
        self.graphs.iter().filter(|slot| slot.is_some()).count()
    }

    /// Number of live graphs.
    pub fn graph_count(&self) -> usize {
        self.graphs()
    }

    /// Whether there are no live graphs.
    pub fn is_empty(&self) -> bool {
        self.graph_count() == 0
    }

    /// Resolves a graph handle, rejecting stale generations.
    pub fn graph(&self, id: PowerGraphId) -> Option<&PowerGraph> {
        let (generation, graph) = self.graphs.get(id.slot as usize)?.as_ref()?;
        (*generation == id.generation).then_some(graph)
    }

    /// Mutable graph resolution, rejecting stale generations.
    pub fn graph_mut(&mut self, id: PowerGraphId) -> Option<&mut PowerGraph> {
        let (generation, graph) = self.graphs.get_mut(id.slot as usize)?.as_mut()?;
        (*generation == id.generation).then_some(graph)
    }

    /// Whether a handle is live.
    pub fn contains(&self, id: PowerGraphId) -> bool {
        self.graph(id).is_some()
    }

    /// Allocates a fresh empty graph (no updater).
    pub fn alloc(&mut self) -> PowerGraphId {
        let debug_id = self.next_graph_id;
        self.next_graph_id = self.next_graph_id.wrapping_add(1);
        let graph = PowerGraph::new(debug_id);
        let slot = match self.free_slots.pop() {
            Some(slot) => {
                let generation = self.graphs[slot as usize]
                    .as_ref()
                    .map(|(generation, _)| generation.wrapping_add(1))
                    .unwrap_or(0);
                self.graphs[slot as usize] = Some((generation, graph));
                slot
            }
            None => {
                let slot = self.graphs.len() as u32;
                self.graphs.push(Some((0, graph)));
                slot
            }
        };
        let generation = self.graphs[slot as usize]
            .as_ref()
            .map(|(generation, _)| *generation)
            .unwrap_or(0);
        PowerGraphId { slot, generation }
    }

    /// Releases a graph slot, invalidating all handles to it.
    pub fn free(&mut self, id: PowerGraphId) {
        if let Some(slot) = self.graphs.get_mut(id.slot as usize)
            && slot
                .as_ref()
                .is_some_and(|(generation, _)| *generation == id.generation)
        {
            *slot = None;
            self.free_slots.push(id.slot);
        }
    }

    /// `PowerGraph.add(build)` for `target`.
    pub fn add(&mut self, world: &mut World, target: PowerGraphId, build: Entity) {
        let Some(params) = read_power_info(world, build) else {
            return;
        };
        if !params.has_power {
            return;
        }
        let module_graph = params.graph;
        let init = params.init;
        if module_graph == target && init {
            return;
        }

        // Any old graph that is added here must be invalid; remove its updater.
        if module_graph != PowerGraphId::NONE && module_graph != target {
            if let Some(updater) = self.graph(module_graph).and_then(|graph| graph.updater) {
                world.despawn(updater);
            }
            if let Some(graph) = self.graph_mut(module_graph) {
                graph.updater = None;
            }
        }

        {
            let Some(mut module) = world.get_mut::<PowerModule>(build) else {
                return;
            };
            module.graph = target;
            module.init = true;
        }
        let Some(graph) = self.graph_mut(target) else {
            return;
        };
        graph.all.push(build);

        if params.outputs && params.consumes && !params.buffered {
            graph.producers.push(build);
            graph.consumers.push(build);
        } else if params.outputs && params.consumes {
            graph.batteries.push(build);
        } else if params.outputs {
            graph.producers.push(build);
        } else if params.consumes {
            graph.consumers.push(build);
        }
    }

    /// `PowerGraph.addGraph(graph)`: merge the smaller graph into the larger.
    pub fn add_graph(&mut self, world: &mut World, a: PowerGraphId, b: PowerGraphId) {
        if a == b {
            return;
        }
        let size_a = self.graph(a).map(|graph| graph.all.len());
        let size_b = self.graph(b).map(|graph| graph.all.len());
        if let (Some(size_a), Some(size_b)) = (size_a, size_b)
            && size_b > size_a
        {
            self.add_graph(world, b, a);
            return;
        }

        // Remove the absorbed graph's updater.
        if let Some(updater) = self.graph(b).and_then(|graph| graph.updater) {
            world.despawn(updater);
        }
        if let Some(graph) = self.graph_mut(b) {
            graph.updater = None;
        }

        let members: SmallVec<[Entity; 16]> = self
            .graph(b)
            .map(|graph| graph.all.clone())
            .unwrap_or_default();
        for member in members {
            self.add(world, a, member);
        }
        self.check_add(world, a);
        self.free(b);
    }

    /// `PowerGraph.checkAdd()`: ensure the graph has an updater entity.
    pub fn check_add(&mut self, world: &mut World, id: PowerGraphId) {
        let needs_updater = self.graph(id).is_some_and(|graph| graph.updater.is_none());
        if !needs_updater {
            return;
        }
        let entity = world
            .spawn((
                crate::entities::comp::PowerGraphUpdater,
                crate::ecs::EntitySeq(u64::MAX),
            ))
            .id();
        if let Some(graph) = self.graph_mut(id) {
            graph.updater = Some(entity);
        }
    }

    /// `PowerGraph.reflow(tile)`: BFS every connected building into a new graph.
    pub fn reflow(&mut self, world: &mut World, grid: &WorldGrid, tile: Entity) {
        let graph = self.alloc();
        self.scratch.queue.clear();
        self.scratch.closed.clear();
        self.scratch.queue.push_back(tile);
        while let Some(child) = self.scratch.queue.pop_front() {
            self.add(world, graph, child);
            self.check_add(world, graph);
            let mut out = std::mem::take(&mut self.scratch.out2);
            get_power_connections(world, grid, child, &mut out);
            for next in out.iter().copied() {
                let pack = pack(world, next);
                if self.scratch.closed.add(pack as u32) {
                    self.scratch.queue.push_back(next);
                }
            }
            self.scratch.out2 = out;
        }
    }

    /// `PowerGraph.remove(tile)`: reassign each branch to a new graph, then
    /// invalidate this one. Calls `update(1.0)` on each new branch so direct
    /// consumers without producers lose power immediately.
    pub fn remove(&mut self, world: &mut World, grid: &WorldGrid, tile: Entity) {
        let Some(current) = world.get::<PowerModule>(tile).map(|module| module.graph) else {
            return;
        };
        if !self.contains(current) {
            return;
        }

        let mut connections = std::mem::take(&mut self.scratch.out1);
        get_power_connections(world, grid, tile, &mut connections);
        for other in connections.iter().copied() {
            let other_graph = world
                .get::<PowerModule>(other)
                .map(|module| module.graph)
                .unwrap_or(PowerGraphId::NONE);
            if other_graph != current {
                continue;
            }

            let graph = self.alloc();
            self.check_add(world, graph);
            self.add(world, graph, other);
            self.scratch.queue.clear();
            self.scratch.queue.push_back(other);
            while let Some(child) = self.scratch.queue.pop_front() {
                self.add(world, graph, child);
                let mut out = std::mem::take(&mut self.scratch.out2);
                get_power_connections(world, grid, child, &mut out);
                for next in out.iter().copied() {
                    let next_graph = world
                        .get::<PowerModule>(next)
                        .map(|module| module.graph)
                        .unwrap_or(PowerGraphId::NONE);
                    if next != tile && next_graph != graph {
                        self.add(world, graph, next);
                        self.scratch.queue.push_back(next);
                    }
                }
                self.scratch.out2 = out;
            }
            if let Some(branch) = self.graph_mut(graph) {
                branch.update(world, 1.0);
            }
        }
        self.scratch.out1 = connections;

        // Implied empty graph here.
        if let Some(updater) = self.graph(current).and_then(|graph| graph.updater) {
            world.despawn(updater);
        }
        self.free(current);
    }

    /// `PowerGraph.removeList(build)` (unit tests only).
    pub fn remove_list(&mut self, id: PowerGraphId, build: Entity) {
        if let Some(graph) = self.graph_mut(id) {
            graph.all.retain(|entity| *entity != build);
            graph.producers.retain(|entity| *entity != build);
            graph.consumers.retain(|entity| *entity != build);
            graph.batteries.retain(|entity| *entity != build);
        }
    }

    /// `PowerGraph.clear()`: empty the graph and remove its updater.
    pub fn clear(&mut self, world: &mut World, id: PowerGraphId) {
        if let Some(updater) = self.graph(id).and_then(|graph| graph.updater) {
            world.despawn(updater);
        }
        if let Some(graph) = self.graph_mut(id) {
            graph.all.clear();
            graph.producers.clear();
            graph.consumers.clear();
            graph.batteries.clear();
            graph.updater = None;
        }
    }

    /// Updates one graph with the explicit `delta`.
    pub fn update(&mut self, world: &mut World, id: PowerGraphId, delta: f32) {
        if let Some(graph) = self.graph_mut(id) {
            graph.update(world, delta);
        }
    }

    /// `EntitySet::UpdatePowerGraph`: update every live graph in slot order.
    pub fn update_all(&mut self, world: &mut World, delta: f32) {
        for index in 0..self.graphs.len() {
            let Some((generation, _)) = self.graphs[index].as_ref() else {
                continue;
            };
            let id = PowerGraphId {
                slot: index as u32,
                generation: *generation,
            };
            if let Some(graph) = self.graph_mut(id) {
                graph.update(world, delta);
            }
        }
    }

    /// Rebuilds every graph from proximity (`World.endMapLoad` path).
    ///
    /// Each connected power component is assigned one fresh graph, matching
    /// upstream's re-add-every-building merge.
    pub fn rebuild_all(&mut self, world: &mut World, grid: &WorldGrid) {
        // Drop every existing graph and clear module handles.
        let ids: Vec<PowerGraphId> = self
            .graphs
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                slot.as_ref().map(|(generation, _)| PowerGraphId {
                    slot: index as u32,
                    generation: *generation,
                })
            })
            .collect();
        for id in ids {
            if let Some(members) = self.graph(id).map(|graph| graph.all.clone()) {
                for member in members {
                    if let Some(mut module) = world.get_mut::<PowerModule>(member) {
                        module.graph = PowerGraphId::NONE;
                        module.init = false;
                    }
                }
            }
            if let Some(updater) = self.graph(id).and_then(|graph| graph.updater) {
                world.despawn(updater);
            }
            self.free(id);
        }

        // Rebuild in deterministic (entity index) order; `reflow` assigns every
        // still-uninitialized component member.
        let mut buildings: Vec<Entity> = world
            .iter_entities()
            .filter_map(|entity_ref| {
                entity_ref.get::<Building>()?;
                entity_ref.get::<PowerModule>()?;
                Some(entity_ref.id())
            })
            .collect();
        buildings.sort_by_key(|entity| entity.index());
        for entity in buildings {
            let assigned = world
                .get::<PowerModule>(entity)
                .map(|module| module.graph != PowerGraphId::NONE && module.init)
                .unwrap_or(false);
            if !assigned {
                self.reflow(world, grid, entity);
            }
        }
    }
}
