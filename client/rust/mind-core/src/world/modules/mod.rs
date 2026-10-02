// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Building modules (item/liquid/power) and flow windows
//! (`core/src/mindustry/world/modules/{BlockModule,ItemModule,LiquidModule,PowerModule}.java`).
//!
//! Java's static `ItemModule.cacheFlow`/`cacheSums`/`displayFlow` are replaced by
//! a per-module [`FlowWindow`] only updated for the selected building (plan 07
//! §2.4.7). `PowerGraphId` is defined here and consumed by plan 09's graph arena.

use bevy_ecs::component::Component;
use mind_macros::SimComponent;
use smallvec::SmallVec;

use crate::content::{ItemId, LiquidId};

/// Stable handle into plan 09's power-graph arena (`{slot, generation}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PowerGraphId {
    /// Arena slot.
    pub slot: u32,
    /// Generation counter (stale handles detect reuse).
    pub generation: u32,
}

impl PowerGraphId {
    /// The null/invalid handle (`-1` in Java).
    pub const NONE: PowerGraphId = PowerGraphId {
        slot: u32::MAX,
        generation: u32::MAX,
    };

    /// Whether the handle refers to a live graph.
    pub fn is_valid(self) -> bool {
        self.slot != u32::MAX
    }
}

/// Per-module flow window (`ItemModule.cacheFlow` etc.), UI-only.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FlowWindow {
    /// Display flow cache (`displayFlow`).
    pub display_flow: f32,
    /// Current computed flow (`current`).
    pub current: f32,
    /// Smoothing (`smooth`).
    pub smooth: f32,
    /// Ring buffer of recent flow samples.
    pub samples: SmallVec<[f32; 8]>,
}

impl FlowWindow {
    /// Pushes a new sample and recomputes the smoothed flow.
    pub fn push(&mut self, amount: f32) {
        self.samples.push(amount);
        if self.samples.len() > 8 {
            self.samples.remove(0);
        }
        let sum: f32 = self.samples.iter().sum();
        self.current = sum;
        self.smooth += (sum - self.smooth) * 0.3;
        self.display_flow = self.smooth;
    }
}

/// Item storage module (`ItemModule`).
#[derive(Debug, Clone, PartialEq, Component, SimComponent)]
#[sim(component, base)]
pub struct ItemModule {
    /// Per-item amounts indexed by item id.
    pub items: Vec<i32>,
    /// Total item count (`total`).
    pub total: i32,
    /// Take rotation cursor (`takeRotation`).
    pub take_rotation: u8,
    /// UI flow window (`None` until observed).
    pub flow: Option<FlowWindow>,
}

impl ItemModule {
    /// Creates a module with `capacity_items` item slots.
    pub fn with_items(items: usize) -> Self {
        Self {
            items: vec![0; items],
            total: 0,
            take_rotation: 0,
            flow: None,
        }
    }

    /// Amount of `item` currently stored.
    pub fn get(&self, item: ItemId) -> i32 {
        self.items.get(item.index()).copied().unwrap_or(0)
    }

    /// `ItemModule.add(item, amount)`; returns the amount accepted.
    pub fn add(&mut self, item: ItemId, amount: i32, capacity: i32) -> i32 {
        let Some(slot) = self.items.get_mut(item.index()) else {
            return 0;
        };
        let before = *slot;
        *slot = (*slot + amount).clamp(0, capacity);
        let accepted = *slot - before;
        self.total += accepted;
        accepted
    }

    /// `ItemModule.remove(item, amount)`; returns the amount removed.
    pub fn remove(&mut self, item: ItemId, amount: i32) -> i32 {
        let Some(slot) = self.items.get_mut(item.index()) else {
            return 0;
        };
        let removed = amount.min(*slot).max(0);
        *slot -= removed;
        self.total -= removed;
        removed
    }

    /// `ItemModule.total`.
    pub fn total(&self) -> i32 {
        self.total
    }

    /// `ItemModule.first()`: the first non-zero item in id order.
    pub fn first(&self) -> Option<ItemId> {
        self.items
            .iter()
            .position(|amount| *amount > 0)
            .map(|index| ItemId::new(index as u16))
    }

    /// Whether any amount of `item` is stored (`ItemModule.has`).
    pub fn has(&self, item: ItemId) -> bool {
        self.get(item) > 0
    }

    /// Whether the module holds at least one item (`any`).
    pub fn any(&self) -> bool {
        self.total > 0
    }

    /// `ItemModule.take()`: removes one of the first non-zero item (id order).
    pub fn take(&mut self) -> Option<ItemId> {
        let item = self.first()?;
        self.remove(item, 1);
        Some(item)
    }

    /// `ItemModule.clear()`.
    pub fn clear(&mut self) {
        self.items.iter_mut().for_each(|amount| *amount = 0);
        self.total = 0;
    }

    /// All non-zero stacks in item-id order.
    pub fn stacks(&self) -> impl Iterator<Item = (ItemId, i32)> + '_ {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, amount)| **amount > 0)
            .map(|(index, amount)| (ItemId::new(index as u16), *amount))
    }
}

/// Liquid storage module (`LiquidModule`).
#[derive(Debug, Clone, PartialEq, Component, SimComponent)]
#[sim(component, base)]
pub struct LiquidModule {
    /// Per-liquid amounts indexed by liquid id.
    pub liquids: Vec<f32>,
    /// Current total (`current`).
    pub current_amount: f32,
    /// UI flow window.
    pub flow: Option<FlowWindow>,
}

impl LiquidModule {
    /// Creates a module with `liquids` liquid slots.
    pub fn with_liquids(liquids: usize) -> Self {
        Self {
            liquids: vec![0.0; liquids],
            current_amount: 0.0,
            flow: None,
        }
    }

    /// Amount of `liquid` currently stored.
    pub fn get(&self, liquid: LiquidId) -> f32 {
        self.liquids.get(liquid.index()).copied().unwrap_or(0.0)
    }

    /// `LiquidModule.add(liquid, amount)`; returns the amount accepted.
    pub fn add(&mut self, liquid: LiquidId, amount: f32, capacity: f32) -> f32 {
        let Some(slot) = self.liquids.get_mut(liquid.index()) else {
            return 0.0;
        };
        let before = *slot;
        *slot = (*slot + amount).clamp(0.0, capacity);
        let accepted = *slot - before;
        self.current_amount += accepted;
        accepted
    }

    /// `LiquidModule.remove(liquid, amount)`; returns the amount removed.
    pub fn remove(&mut self, liquid: LiquidId, amount: f32) -> f32 {
        let Some(slot) = self.liquids.get_mut(liquid.index()) else {
            return 0.0;
        };
        let removed = amount.min(*slot).max(0.0);
        *slot -= removed;
        self.current_amount -= removed;
        removed
    }

    /// Total current liquid (`current()`).
    pub fn current(&self) -> f32 {
        self.current_amount
    }

    /// Whether any liquid is stored.
    pub fn has_any(&self) -> bool {
        self.current_amount > 0.0
    }
}

/// Power module (`PowerModule`).
#[derive(Debug, Clone, PartialEq, Component, SimComponent)]
#[sim(component, base)]
pub struct PowerModule {
    /// Satisfaction status `0..1` (`status`).
    pub status: f32,
    /// Whether the graph was initialized (`init`).
    pub init: bool,
    /// Owning graph handle (plan 09).
    pub graph: PowerGraphId,
    /// Link positions (packed tile indices, plan 09).
    pub links: SmallVec<[i32; 4]>,
    /// Buffered power stored (`stored`).
    pub stored: f32,
}

impl PowerModule {
    /// A module with no graph assigned.
    pub fn new() -> Self {
        Self {
            status: 0.0,
            init: false,
            graph: PowerGraphId::NONE,
            links: SmallVec::new(),
            stored: 0.0,
        }
    }
}

impl Default for PowerModule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_module_add_remove_clamps_at_zero() {
        let mut module = ItemModule::with_items(4);
        let copper = ItemId::COPPER;
        assert_eq!(module.add(copper, 5, 10), 5);
        assert_eq!(module.total(), 5);
        assert_eq!(module.add(copper, 100, 10), 5);
        assert_eq!(module.total(), 10);
        assert_eq!(module.remove(copper, 3), 3);
        assert_eq!(module.total(), 7);
        assert_eq!(module.remove(copper, 100), 7);
        assert_eq!(module.total(), 0);
        assert_eq!(module.remove(copper, 1), 0);
    }

    #[test]
    fn liquid_module_total_tracks_current() {
        let mut module = LiquidModule::with_liquids(2);
        assert_eq!(module.add(LiquidId::WATER, 5.0, 10.0), 5.0);
        assert_eq!(module.current(), 5.0);
        assert_eq!(module.remove(LiquidId::WATER, 2.0), 2.0);
        assert_eq!(module.current(), 3.0);
    }

    #[test]
    fn power_graph_id_validity() {
        assert!(!PowerGraphId::NONE.is_valid());
        assert!(
            PowerGraphId {
                slot: 1,
                generation: 0
            }
            .is_valid()
        );
    }
}
