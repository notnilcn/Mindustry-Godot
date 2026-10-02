// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Stable network-state dump for the headless oracle and the MCP bridge
//! (plan 09 §3.11).
//!
//! `NetworkState` is a plain, sorted JSON projection of the three resource
//! networks. It is derived state only: graphs are rebuilt from `links` on load
//! and are never part of the sim checksum (plan 09 §3.11). Float fields are
//! rounded to four decimals so goldens are byte-stable across runs.

use bevy_ecs::world::World;
use serde::Serialize;

use crate::world::blocks::power::PowerGraph;

/// Rounds to four decimals and normalizes `-0.0` to `0.0`.
fn round4(value: f32) -> f32 {
    let rounded = (value * 10_000.0).round() / 10_000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// One drained power network.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct GraphState {
    /// Monotonic debug id (`PowerGraph.debug_id`).
    pub id: u32,
    /// Producer count.
    pub producers: u32,
    /// Unbuffered consumer count.
    pub consumers: u32,
    /// Battery count.
    pub batteries: u32,
    /// `getSatisfaction()`.
    pub satisfaction: f32,
    /// `lastPowerProduced`.
    pub produced: f32,
    /// `lastPowerNeeded`.
    pub needed: f32,
    /// `lastPowerStored`.
    pub stored: f32,
    /// `lastCapacity` (total, not missing).
    pub capacity: f32,
    /// `getPowerBalance()` (raw mean of the 60-tick window).
    pub balance: f32,
    /// `energyDelta` (diode workaround).
    pub energy_delta: f32,
}

impl GraphState {
    /// Captures a graph's observable state for `delta`.
    pub fn capture(graph: &PowerGraph, world: &World, delta: f32) -> Self {
        let capacity: f32 = graph
            .batteries
            .iter()
            .filter_map(|battery| {
                crate::world::blocks::power::read_power_info(world, *battery)
                    .map(|params| params.capacity)
            })
            .sum();
        Self {
            id: graph.debug_id,
            producers: graph.producers.len() as u32,
            consumers: graph.consumers.len() as u32,
            batteries: graph.batteries.len() as u32,
            satisfaction: round4(graph.get_satisfaction()),
            produced: round4(graph.get_power_produced(world, delta)),
            needed: round4(graph.get_power_needed(world, delta)),
            stored: round4(graph.get_battery_stored(world)),
            capacity: round4(capacity),
            balance: round4(graph.power_balance()),
            energy_delta: round4(graph.energy_delta()),
        }
    }
}

/// One building's contributed network value.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct BuildingState {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Content name.
    pub name: String,
    /// Primary amount (liquid stored / heat / power status).
    pub amount: f32,
    /// Secondary amount (capacity / heat requirement).
    pub extra: f32,
}

impl BuildingState {
    /// Creates a building state row.
    pub fn new(x: i16, y: i16, name: impl Into<String>, amount: f32, extra: f32) -> Self {
        Self {
            x,
            y,
            name: name.into(),
            amount: round4(amount),
            extra: round4(extra),
        }
    }
}

/// The full sorted three-network projection.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NetworkState {
    /// Tick the dump was taken at.
    pub tick: u64,
    /// Power graphs, sorted by debug id.
    pub graphs: Vec<GraphState>,
    /// Liquid building rows, sorted by `(y, x)`.
    pub liquids: Vec<BuildingState>,
    /// Heat building rows, sorted by `(y, x)`.
    pub heat: Vec<BuildingState>,
}

impl NetworkState {
    /// An empty dump at `tick`.
    pub fn new(tick: u64) -> Self {
        Self {
            tick,
            graphs: Vec::new(),
            liquids: Vec::new(),
            heat: Vec::new(),
        }
    }

    /// Appends a captured graph.
    pub fn push_graph(&mut self, graph: &PowerGraph, world: &World, delta: f32) {
        self.graphs.push(GraphState::capture(graph, world, delta));
    }

    /// Appends a liquid building row.
    pub fn push_liquid(&mut self, row: BuildingState) {
        self.liquids.push(row);
    }

    /// Appends a heat building row.
    pub fn push_heat(&mut self, row: BuildingState) {
        self.heat.push(row);
    }

    /// Stable JSON (graphs by id, building rows by `(y, x)`).
    pub fn to_json(&self) -> String {
        #[derive(Serialize)]
        struct Out<'a> {
            tick: u64,
            graphs: &'a [GraphState],
            liquids: &'a [BuildingState],
            heat: &'a [BuildingState],
        }

        let mut graphs = self.graphs.clone();
        graphs.sort_by_key(|graph| graph.id);
        let mut liquids = self.liquids.clone();
        liquids.sort_by_key(|row| (row.y, row.x));
        let mut heat = self.heat.clone();
        heat.sort_by_key(|row| (row.y, row.x));

        let out = Out {
            tick: self.tick,
            graphs: &graphs,
            liquids: &liquids,
            heat: &heat,
        };
        serde_json::to_string_pretty(&out).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round4_normalizes_negative_zero() {
        assert_eq!(round4(-0.0), 0.0);
        assert_eq!(round4(0.123_456), 0.1235);
    }

    #[test]
    fn to_json_sorts_graphs_and_rows() {
        let mut state = NetworkState::new(3);
        state.graphs.push(GraphState {
            id: 2,
            ..Default::default()
        });
        state.graphs.push(GraphState {
            id: 1,
            ..Default::default()
        });
        state.liquids.push(BuildingState::new(5, 1, "a", 1.0, 2.0));
        state.liquids.push(BuildingState::new(1, 0, "b", 3.0, 0.0));
        let text = state.to_json();
        let json: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(json["graphs"][0]["id"], 1);
        assert_eq!(json["graphs"][1]["id"], 2);
        assert_eq!(json["liquids"][0]["name"], "b");
        assert_eq!(json["liquids"][1]["name"], "a");
    }
}
