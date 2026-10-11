// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Construction state machine
//! (`core/src/mindustry/world/blocks/ConstructBlock.java` + `ConstructBuild`).
//!
//! Plan 07 §3.7: one `ConstructState` component drives both the accumulating
//! build and the deconstruction path; `ConstructBlock` size singletons are
//! registered by plan 02 as `build1`..`build16`. The harness owns the tile/ECS
//! mutation; this module owns the progress math and component shape.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use smallvec::SmallVec;

use crate::content::{BlockId, ItemStack};
use crate::world::config::ConfigValue;

/// Per-construction state (`ConstructBuild` fields; plan 07 §3.7).
#[derive(Debug, Clone, Component)]
pub struct ConstructState {
    /// Block being constructed.
    pub current: BlockId,
    /// Block that was there before construction.
    pub previous: BlockId,
    /// Buildings overwritten by this construction (`prevBuild`).
    pub prev_build: SmallVec<[Entity; 9]>,
    /// Construction progress `0..1`.
    pub progress: f32,
    /// Total build cost in "build units".
    pub build_cost: f32,
    /// Last config applied to the construction.
    pub last_config: ConfigValue,
    /// Last builder to work on this construction.
    pub last_builder: Option<Entity>,
    /// Whether this was built (not deconstructed).
    pub was_constructing: bool,
    /// Whether this is a deconstruction.
    pub active_deconstruct: bool,
    /// Construct color accumulator (`constructColor`).
    pub construct_color: f32,
    /// Per-requirement accumulator (`accumulator`).
    pub accumulator: SmallVec<[f32; 4]>,
    /// Total per-requirement accumulator (`totalAccumulator`).
    pub total_accumulator: SmallVec<[f32; 4]>,
    /// Remaining required items (`itemsLeft`).
    pub items_left: SmallVec<[i32; 4]>,
}

impl ConstructState {
    /// Creates construction state for `previous -> current`.
    pub fn construct(previous: BlockId, current: BlockId, build_cost: f32) -> Self {
        Self {
            current,
            previous,
            prev_build: SmallVec::new(),
            progress: 0.0,
            build_cost,
            last_config: ConfigValue::None,
            last_builder: None,
            was_constructing: true,
            active_deconstruct: false,
            construct_color: 0.0,
            accumulator: SmallVec::new(),
            total_accumulator: SmallVec::new(),
            items_left: SmallVec::new(),
        }
    }

    /// Creates deconstruction state for `previous`.
    pub fn deconstruct(previous: BlockId, build_cost: f32) -> Self {
        Self {
            current: previous,
            previous,
            prev_build: SmallVec::new(),
            progress: 1.0,
            build_cost,
            last_config: ConfigValue::None,
            last_builder: None,
            was_constructing: false,
            active_deconstruct: true,
            construct_color: 0.0,
            accumulator: SmallVec::new(),
            total_accumulator: SmallVec::new(),
            items_left: SmallVec::new(),
        }
    }

    /// Advances construction by `amount`; returns `true` when it completes.
    pub fn advance_construct(&mut self, amount: f32) -> bool {
        if self.build_cost <= 0.0 {
            self.progress = 1.0;
            return true;
        }
        self.progress += amount / self.build_cost;
        self.progress >= 1.0
    }

    /// Advances deconstruction by `amount`; returns `(finished, refund_fraction)`.
    pub fn advance_deconstruct(&mut self, amount: f32, refund_multiplier: f32) -> (bool, f32) {
        if self.build_cost <= 0.0 {
            self.progress = 0.0;
            return (true, refund_multiplier);
        }
        self.progress -= amount / self.build_cost;
        (self.progress <= 0.0, refund_multiplier)
    }
}

/// Rule inputs to [`ConstructState::construct`]/[`ConstructState::deconstruct`]
/// (`state.rules` / `team.rules()` reads in `ConstructBuild`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuildEconomy {
    /// `Rules.buildCostMultiplier`.
    pub cost_multiplier: f32,
    /// `Rules.deconstructRefundMultiplier`.
    pub refund_multiplier: f32,
    /// `state.rules.infiniteResources || team.rules().infiniteResources`.
    pub infinite: bool,
}

impl Default for BuildEconomy {
    fn default() -> Self {
        Self {
            cost_multiplier: 1.0,
            refund_multiplier: 0.5,
            infinite: false,
        }
    }
}

/// The builder's team core inventory (`core.items` + `core.storageCapacity`).
pub struct CoreInventory<'a> {
    /// Core item module.
    pub items: &'a mut crate::world::modules::ItemModule,
    /// `CoreBuild.storageCapacity`.
    pub capacity: i32,
}

/// `Math.round` (half-up) for the requirement scaling.
fn java_round(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

impl ConstructState {
    /// `ConstructBuild.setConstruct(previous, block)`.
    pub fn set_construct(
        &mut self,
        previous: BlockId,
        current: BlockId,
        build_time: f32,
        requirements: &[ItemStack],
        cost_multiplier: f32,
    ) {
        self.construct_color = 0.0;
        self.was_constructing = true;
        self.current = current;
        self.previous = previous;
        self.build_cost = build_time * cost_multiplier;
        self.items_left = requirements
            .iter()
            .map(|stack| java_round(stack.amount as f32 * cost_multiplier))
            .collect();
        self.accumulator = smallvec::smallvec![0.0; requirements.len()];
        self.total_accumulator = smallvec::smallvec![0.0; requirements.len()];
    }

    /// `ConstructBuild.setDeconstruct(previous)`.
    pub fn set_deconstruct(
        &mut self,
        previous: BlockId,
        build_time: f32,
        requirements: &[ItemStack],
        cost_multiplier: f32,
    ) {
        self.construct_color = 1.0;
        self.was_constructing = false;
        self.previous = previous;
        self.progress = 1.0;
        self.current = previous;
        self.build_cost = build_time * cost_multiplier;
        self.items_left = smallvec::smallvec![0; requirements.len()];
        self.accumulator = smallvec::smallvec![0.0; requirements.len()];
        self.total_accumulator = smallvec::smallvec![0.0; requirements.len()];
    }

    /// `ConstructBuild.checkRequired`: the progress the core inventory can fund
    /// this step; consumes the funded items when `remove`.
    fn check_required(
        &mut self,
        requirements: &[ItemStack],
        inventory: &mut crate::world::modules::ItemModule,
        amount: f32,
        remove: bool,
        eco: &BuildEconomy,
    ) -> f32 {
        let mut max_progress = amount;
        for (i, stack) in requirements.iter().enumerate() {
            if self.items_left.get(i).copied().unwrap_or(0) == 0 {
                continue;
            }
            let scaled = java_round(eco.cost_multiplier * stack.amount as f32);
            let required = self.accumulator[i] as i32;
            let have = inventory.get(stack.item);
            if have == 0 && scaled != 0 {
                max_progress = 0.0;
            } else if required > 0 {
                let max_use = required.min(have);
                let fraction = max_use as f32 / required as f32;
                max_progress = max_progress.min(max_progress * fraction);
                self.accumulator[i] -= max_use as f32;
                if remove && !eco.infinite {
                    inventory.remove(stack.item, max_use);
                    self.items_left[i] -= max_use;
                }
            }
        }
        max_progress
    }

    /// `ConstructBuild.construct(builder, core, amount, config)`: advances the
    /// build by `amount` (already divided by `buildCost`), drawing the
    /// requirements from `core` as progress accrues. Returns `true` when the
    /// block may be finished (`constructed`).
    pub fn construct_step(
        &mut self,
        requirements: &[ItemStack],
        mut core: Option<CoreInventory<'_>>,
        amount: f32,
        eco: &BuildEconomy,
        state_infinite: bool,
    ) -> bool {
        self.was_constructing = true;
        self.active_deconstruct = false;
        if self.accumulator.len() != requirements.len()
            || self.total_accumulator.len() != requirements.len()
        {
            return false;
        }
        let mut max_progress = match core.as_mut() {
            Some(core) if !eco.infinite => {
                self.check_required(requirements, core.items, amount, false, eco)
            }
            _ => amount,
        };
        for (i, stack) in requirements.iter().enumerate() {
            let req = java_round(eco.cost_multiplier * stack.amount as f32) as f32;
            self.accumulator[i] += (req * max_progress).min(req - self.total_accumulator[i]);
            self.total_accumulator[i] = (self.total_accumulator[i] + req * max_progress).min(req);
        }
        if let Some(core) = core.as_mut()
            && !eco.infinite
        {
            max_progress = self.check_required(requirements, core.items, max_progress, true, eco);
        }
        self.progress = (self.progress + max_progress).clamp(0.0, 1.0);

        if self.progress >= 1.0 || state_infinite {
            if !eco.infinite {
                for (i, stack) in requirements.iter().enumerate() {
                    let left = self.items_left[i];
                    if left > 0 {
                        match core.as_mut() {
                            Some(core) if core.items.get(stack.item) >= left => {
                                core.items.remove(stack.item, left);
                                self.items_left[i] = 0;
                            }
                            _ => return false,
                        }
                    }
                }
            }
            return true;
        }
        false
    }

    /// `ConstructBuild.deconstruct(builder, core, amount)`: refunds the scaled
    /// requirements into `core` as progress drains. Returns `true` when the
    /// tile should be removed (`deconstructFinish`).
    pub fn deconstruct_step(
        &mut self,
        requirements: &[ItemStack],
        mut core: Option<CoreInventory<'_>>,
        amount: f32,
        eco: &BuildEconomy,
        state_infinite: bool,
    ) -> bool {
        if self.was_constructing {
            self.accumulator.iter_mut().for_each(|value| *value = 0.0);
            self.total_accumulator
                .iter_mut()
                .for_each(|value| *value = 0.0);
        }
        self.was_constructing = false;
        self.active_deconstruct = true;
        if self.accumulator.len() != requirements.len()
            || self.total_accumulator.len() != requirements.len()
            || self.items_left.len() != requirements.len()
        {
            return false;
        }
        let refund = eco.refund_multiplier;
        let clamped = amount.min(self.progress);
        for (i, stack) in requirements.iter().enumerate() {
            let req = java_round(eco.cost_multiplier * stack.amount as f32) as f32;
            self.accumulator[i] +=
                (clamped * refund * req).min(refund * req - self.total_accumulator[i]);
            self.total_accumulator[i] =
                (self.total_accumulator[i] + req * clamped * refund).min(req);
            let accumulated = self.accumulator[i] as i32;
            if clamped > 0.0 && accumulated > 0 {
                match core.as_mut() {
                    Some(core) => {
                        let accepting = accumulated
                            .min(core.capacity - core.items.get(stack.item))
                            .max(0);
                        if !state_infinite {
                            core.items.add(stack.item, accepting, core.capacity);
                        }
                        self.items_left[i] += accepting;
                        self.accumulator[i] -= accepting as f32;
                    }
                    None => self.accumulator[i] -= accumulated as f32,
                }
            }
        }
        self.progress = (self.progress - amount).clamp(0.0, 1.0);
        if self.progress <= 0.0 || state_infinite {
            if let Some(core) = core.as_mut()
                && !state_infinite
            {
                for (i, stack) in requirements.iter().enumerate() {
                    let target = java_round(
                        stack.amount as f32 * eco.cost_multiplier * eco.refund_multiplier,
                    );
                    let remaining = target - self.items_left[i];
                    let room = core.capacity - core.items.get(stack.item);
                    core.items
                        .add(stack.item, remaining.clamp(0, room.max(0)), core.capacity);
                    self.items_left[i] = target;
                }
            }
            return true;
        }
        false
    }
}

/// Deterministic pseudo-pitch for construction sounds (`ConstructBlock.calcPitch`
/// uses `Time.millis`; plan 07 §2.4.8 shifts it to a sim-time counter so the view
/// is reproducible and cannot affect checksums).
pub fn calc_pitch(seed: u32) -> f32 {
    1.0 - 0.1 + (seed % 100) as f32 / 100.0 * 0.2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_progress_math() {
        let mut state = ConstructState::construct(BlockId::AIR, BlockId::STONE_WALL, 100.0);
        assert!(!state.advance_construct(25.0));
        assert!((state.progress - 0.25).abs() < f32::EPSILON);
        assert!(!state.advance_construct(50.0));
        assert!(state.advance_construct(25.0));
    }

    #[test]
    fn deconstruction_progress_math() {
        let mut state = ConstructState::deconstruct(BlockId::STONE_WALL, 100.0);
        let (finished, _refund) = state.advance_deconstruct(40.0, 0.5);
        assert!(!finished);
        assert!((state.progress - 0.6).abs() < f32::EPSILON);
        let (finished, refund) = state.advance_deconstruct(60.0, 0.5);
        assert!(finished);
        assert_eq!(refund, 0.5);
    }
}
