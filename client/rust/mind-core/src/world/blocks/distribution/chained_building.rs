// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ChainedBuilding` (`world/blocks/distribution/ChainedBuilding.java`) — plan 08.
//!
//! The single-method marker consumed by plan 16's renderer to walk a belt/chain
//! of linked buildings.

use bevy_ecs::entity::Entity;

/// `ChainedBuilding` (`next()` returns the next building in the chain).
pub trait ChainedBuilding {
    /// Next building in the chain, if any.
    fn next(&self) -> Option<Entity>;
}
