// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Liquid blocks and modules (`core/src/mindustry/world/blocks/liquid/`).
//!
//! Plan 07 owns `LiquidModule` storage; plan 09 owns the transfer primitives
//! ([`movement`]) and the single-watched flow window ([`LiquidFlowCache`], plan 09
//! §2.3.4/§6.2 — Java's static `cacheFlow`/`cacheSums`/`displayFlow`/`cacheBits`
//! become one Resource). Conduit/router/junction behaviors are the M3 files.
//! (`movement.rs` is the plan's `move.rs`; `move` is a Rust keyword.)

pub mod movement;

use bevy_ecs::component::Component;
use bevy_ecs::prelude::Resource;
use smallvec::SmallVec;

use crate::content::LiquidId;
use crate::math::WindowedMean;

pub use movement::{
    accept_liquid, can_dump_liquid, dump_liquid, get_liquid_destination, handle_liquid,
    move_liquid, move_liquid_forward, transfer_liquid,
};

/// Per-block liquid routing/transfer knobs (`LiquidBlock`/`Conduit`/`Router`).
#[derive(Debug, Clone, PartialEq, Component)]
pub struct LiquidNode {
    /// `Block.liquidCapacity`.
    pub capacity: f32,
    /// Whether the building accepts liquid (`hasLiquids`/router rules).
    pub accepts: bool,
    /// Whether the block leaks when the output is blocked (`Conduit.leaks`).
    pub leakable: bool,
    /// `LiquidJunction` pass-through routing.
    pub junction: bool,
    /// `LiquidRouter` dumps its current liquid each tick.
    pub router: bool,
    /// `liquidPressure` multiplier (default `1.0`).
    pub pressure: f32,
    /// Accepted liquid filter (`Block.liquidFilter`); empty = accept all.
    pub filter: SmallVec<[LiquidId; 2]>,
}

impl Default for LiquidNode {
    fn default() -> Self {
        Self {
            capacity: 0.0,
            accepts: false,
            leakable: false,
            junction: false,
            router: false,
            pressure: 1.0,
            filter: SmallVec::new(),
        }
    }
}

/// Flow window across all liquids (`LiquidModule`'s static cache).
///
/// `LiquidModule.flowWindowSize = 6`, `flowPollInterval = 10`,
/// `flowVisualRefreshInterval = 15`; `displayFlow` starts at `-1` (not ready).
#[derive(Debug, Resource)]
pub struct LiquidFlowCache {
    /// Per-liquid windowed means.
    pub flow: Vec<WindowedMean>,
    /// Per-liquid accumulated sample sums.
    pub cache_sums: Vec<f32>,
    /// Per-liquid display flow (`-1` = not ready).
    pub display_flow: Vec<f32>,
    /// Per-liquid "has flow" bits.
    pub cache_bits: Vec<bool>,
    /// Tick counter driving the poll/refresh intervals.
    pub ticks: u32,
    /// Whether a building is currently watched.
    pub active: bool,
    /// Poll interval in ticks (`flowPollInterval`).
    pub poll_interval: u32,
    /// Visual refresh interval in ticks (`flowVisualRefreshInterval`).
    pub refresh_interval: u32,
}

impl Default for LiquidFlowCache {
    fn default() -> Self {
        Self::new(0)
    }
}

impl LiquidFlowCache {
    /// Creates a cache sized for `liquid_count` liquids.
    pub fn new(liquid_count: usize) -> Self {
        Self {
            flow: (0..liquid_count).map(|_| WindowedMean::new(6)).collect(),
            cache_sums: vec![0.0; liquid_count],
            display_flow: vec![-1.0; liquid_count],
            cache_bits: vec![false; liquid_count],
            ticks: 0,
            active: false,
            poll_interval: 10,
            refresh_interval: 15,
        }
    }

    /// Sizes the cache for `liquid_count` (resets the window).
    pub fn check_array_capacity(&mut self, liquid_count: usize) {
        if self.flow.len() != liquid_count {
            *self = Self::new(liquid_count);
        }
    }

    /// `LiquidModule.add`/`handleFlow` accounting for the watched building.
    pub fn record(&mut self, liquid: LiquidId, amount: f32) {
        let index = liquid.index();
        if index < self.cache_sums.len() {
            self.cache_sums[index] += amount.max(0.0);
        }
    }

    /// `LiquidModule.stopFlow`.
    pub fn stop_flow(&mut self) {
        self.active = false;
        self.display_flow.iter_mut().for_each(|value| *value = -1.0);
    }

    /// Advances the flow timer by one tick (`LiquidModule.updateFlow`).
    pub fn tick(&mut self) {
        if !self.active {
            return;
        }
        self.ticks = self.ticks.wrapping_add(1);
        if !self.ticks.is_multiple_of(self.poll_interval.max(1)) {
            return;
        }
        let refresh = self.ticks.is_multiple_of(self.refresh_interval.max(1));
        for index in 0..self.flow.len() {
            let sum = self.cache_sums[index];
            self.flow[index].add(sum);
            if sum > 0.0 {
                self.cache_bits[index] = true;
            }
            self.cache_sums[index] = 0.0;
            if refresh {
                self.display_flow[index] = if self.flow[index].has_enough_data() {
                    self.flow[index].mean() / self.poll_interval as f32
                } else {
                    -1.0
                };
            }
        }
    }

    /// `LiquidModule.getFlowRate`: `displayFlow * 60` u/s; `< 0` = not ready.
    pub fn get_flow_rate(&self, liquid: LiquidId) -> f32 {
        if !self.active {
            return -1.0;
        }
        self.display_flow
            .get(liquid.index())
            .map(|value| value * 60.0)
            .unwrap_or(-1.0)
    }

    /// `LiquidModule.hasFlowLiquid`.
    pub fn has_flow_liquid(&self, liquid: LiquidId) -> bool {
        self.active
            && self
                .cache_bits
                .get(liquid.index())
                .copied()
                .unwrap_or(false)
    }
}

/// First stored liquid in ascending id order (`LiquidModule.current` fallback;
/// plan 07's module does not yet track the last-received liquid).
pub fn current_liquid(module: &crate::world::modules::LiquidModule) -> Option<LiquidId> {
    module
        .liquids
        .iter()
        .enumerate()
        .find(|(_, amount)| **amount > 0.0)
        .map(|(index, _)| LiquidId::new(index as u16))
}

/// `LiquidRouterBuild.updateTile`: dump the current liquid.
pub fn update_router(
    world: &mut bevy_ecs::world::World,
    grid: &crate::world::WorldGrid,
    entity: bevy_ecs::entity::Entity,
) {
    let Some(liquid) = world
        .get::<crate::world::modules::LiquidModule>(entity)
        .and_then(current_liquid)
    else {
        return;
    };
    movement::dump_liquid(world, grid, entity, liquid, 2.0, -1);
}

/// `ConduitBuild.updateTile`: move the current liquid forward (or leak).
pub fn update_conduit(
    world: &mut bevy_ecs::world::World,
    grid: &crate::world::WorldGrid,
    entity: bevy_ecs::entity::Entity,
) {
    let leakable = world
        .get::<LiquidNode>(entity)
        .map(|node| node.leakable)
        .unwrap_or(false);
    let Some(liquid) = world
        .get::<crate::world::modules::LiquidModule>(entity)
        .and_then(current_liquid)
    else {
        return;
    };
    let next = world
        .get::<crate::entities::comp::Building>(entity)
        .and_then(|building| {
            let (dx, dy) = match building.rotation % 4 {
                0 => (1, 0),
                1 => (0, 1),
                2 => (-1, 0),
                _ => (0, -1),
            };
            let x = building.tile.x() as i32 + dx;
            let y = building.tile.y() as i32 + dy;
            grid.tiles
                .in_bounds(x, y)
                .then(|| grid.tile(x, y).build)
                .flatten()
        });
    movement::move_liquid_forward(world, grid, entity, next, leakable, liquid);
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn node_defaults_match_java() {
        let node = LiquidNode::default();
        assert_eq!(node.capacity, 0.0);
        assert!(!node.accepts && !node.junction && !node.router);
        assert_eq!(node.pressure, 1.0);
    }

    #[test]
    fn flow_cache_display_starts_not_ready() {
        let cache = LiquidFlowCache::new(2);
        assert_eq!(cache.get_flow_rate(LiquidId::WATER), -1.0);
        assert!(!cache.has_flow_liquid(LiquidId::WATER));
    }
}

#[cfg(test)]
mod tests;
