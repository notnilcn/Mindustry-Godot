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

use crate::content::BlockId;
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
