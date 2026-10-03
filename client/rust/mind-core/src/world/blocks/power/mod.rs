// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Power blocks, graphs and modules (`core/src/mindustry/world/blocks/power/`).
//!
//! Plan 09 owns the power network. Plan 07 owns the `Building`/`PowerModule`
//! data layout and placement lifecycle; this module implements the
//! [`graph::PowerGrids`] arena, the connection/lifecycle functions in
//! [`module`], and the block-family behaviors in the sibling files.
//!
//! Deviations (plan 09 §2.3): Java's identity-referenced graphs become
//! generation-checked arena handles; static BFS scratch becomes the shared
//! [`graph::PowerScratch`]; `update` takes an explicit `delta` (D8).

pub mod behavior;
pub mod explosion;
pub mod generator;
pub mod graph;
pub mod module;
pub mod nodes;
pub mod reactors;
pub mod sandbox;

pub use behavior::register;
pub use explosion::{
    ExplosionFired, ExplosionSink, ExplosionSinkRes, ReactorExplosion, fire_explosion,
};

use bevy_ecs::component::Component;

pub use graph::{PowerGraph, PowerGrids, PowerScratch};
pub use module::{
    PowerParams, get_power_connections, power_graph_removed, read_power_info, update_power_graph,
};

/// Back-compat alias matching the plan's `PowerGraphs` resource name.
pub type PowerGraphs = PowerGrids;

/// Static power classification override for fixtures/synthetic content.
///
/// Real content derives these from `BlockDef` metadata; fixtures attach this to
/// keep the network algorithms content-free (plan 09 §7a).
#[derive(Debug, Clone, PartialEq, Component)]
pub struct PowerNodeInfo {
    /// `Block.outputsPower`.
    pub outputs: bool,
    /// `Block.consumesPower`.
    pub consumes: bool,
    /// `ConsumePower.buffered`.
    pub buffered: bool,
    /// `ConsumePower.usage`.
    pub usage: f32,
    /// `ConsumePower.capacity`.
    pub capacity: f32,
    /// `Block.conductivePower`.
    pub conductive: bool,
    /// `Block.insulated`.
    pub insulated: bool,
}

impl Default for PowerNodeInfo {
    fn default() -> Self {
        Self {
            outputs: false,
            consumes: false,
            buffered: false,
            usage: 0.0,
            capacity: 0.0,
            conductive: false,
            insulated: false,
        }
    }
}

impl PowerNodeInfo {
    /// A pure producer (`outputsPower` only).
    pub fn producer() -> Self {
        Self {
            outputs: true,
            ..Self::default()
        }
    }

    /// An unbuffered consumer (`consumesPower` only).
    pub fn consumer(usage: f32) -> Self {
        Self {
            consumes: true,
            usage,
            ..Self::default()
        }
    }

    /// A buffered battery (`outputsPower && consumesPower`).
    pub fn battery(capacity: f32) -> Self {
        Self {
            outputs: true,
            consumes: true,
            buffered: true,
            capacity,
            ..Self::default()
        }
    }
}

/// Current power production (`getPowerProduction()`).
///
/// Behavior code (generators/sandbox source) writes this each tick; the graph
/// reads it via [`read_power_info`]. Defaults to `0.0`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct PowerProduction(pub f32);

/// `PowerNode` configuration (`maxNodes`/`laserRange`/`autolink`).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct PowerNodeConfig {
    /// `PowerNode.maxNodes`.
    pub max_nodes: u8,
    /// `PowerNode.laserRange` (tiles).
    pub laser_range: f32,
    /// `PowerNode.autolink`.
    pub autolink: bool,
    /// `PowerNode.sameBlockConnection`.
    pub same_block_connection: bool,
}

impl Default for PowerNodeConfig {
    fn default() -> Self {
        Self {
            max_nodes: 3,
            laser_range: 6.0,
            autolink: true,
            same_block_connection: false,
        }
    }
}

/// Pure power bar data (`Block.setBars` power factories; plan 14 renders).
#[derive(Debug, Clone, PartialEq)]
pub struct PowerBarData {
    /// Bundle label key (`bar.powerbalance`, `bar.powerstored`, `bar.powerlines`).
    pub label_key: String,
    /// Fill fraction `0..1`.
    pub fill: f32,
    /// Bar color.
    pub color: [f32; 4],
}

/// `makePowerBalance`: graph balance relative to the last 60-tick window.
pub fn make_power_balance(graph: &PowerGraph) -> PowerBarData {
    let balance = graph.power_balance();
    let max = graph
        .last_power_needed()
        .max(graph.last_power_produced())
        .max(0.0001);
    PowerBarData {
        label_key: String::from("bar.powerbalance"),
        fill: (balance / max).clamp(-1.0, 1.0),
        color: [1.0, 0.85, 0.3, 1.0],
    }
}

/// `makeBatteryBalance`: stored fraction of total capacity.
pub fn make_battery_balance(graph: &PowerGraph, world: &bevy_ecs::world::World) -> PowerBarData {
    let capacity = graph.get_total_battery_capacity(world);
    let stored = graph.get_battery_stored(world);
    let fill = if capacity <= 0.0 {
        0.0
    } else {
        stored / capacity
    };
    PowerBarData {
        label_key: String::from("bar.powerstored"),
        fill: fill.clamp(0.0, 1.0),
        color: [1.0, 0.6, 0.4, 1.0],
    }
}

#[cfg(test)]
mod generator_tests;
#[cfg(test)]
mod tests;
