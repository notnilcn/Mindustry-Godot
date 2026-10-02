// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Item distribution family (`world/blocks/distribution/`) — plan 08.
//!
//! `LogisticsPlugin` equivalent: [`register`] installs one [`BuildingBehavior`]
//! per vanilla transport block into plan 07's [`BehaviorRegistry`]. The shared
//! transfer helpers live in [`transfer`]; the autotiler is
//! [`crate::world::blocks::autotiler`] and the 47-slice table is
//! [`crate::world::blocks::tile_bitmask`] (both consumed by plan 09).

pub mod chained_building;
pub mod conveyor;
pub mod duct;
pub mod stack_conveyor;
pub mod transfer;

use std::sync::Arc;

use crate::content::ContentRegistry;
use crate::world::behavior::BehaviorRegistry;

pub use chained_building::ChainedBuilding;
pub use conveyor::{CAPACITY as CONVEYOR_CAPACITY, ConveyorBehavior, ConveyorBuild};
pub use duct::{DuctBehavior, DuctBuild};
pub use stack_conveyor::{StackConveyorBehavior, StackConveyorBuild};

/// Registers every item-distribution behavior available at this milestone.
pub fn register(registry: &mut BehaviorRegistry, _content: &ContentRegistry) {
    registry.register_named("conveyor", Arc::new(ConveyorBehavior::BASE));
    registry.register_named("titanium-conveyor", Arc::new(ConveyorBehavior::TITANIUM));
    registry.register_named("armored-conveyor", Arc::new(ConveyorBehavior::ARMORED));
    registry.register_named("duct", Arc::new(DuctBehavior::NORMAL));
    registry.register_named("armored-duct", Arc::new(DuctBehavior::ARMORED));
    registry.register_named(
        "plastanium-conveyor",
        Arc::new(StackConveyorBehavior::PLASTANIUM),
    );
    registry.register_named("surge-conveyor", Arc::new(StackConveyorBehavior::SURGE));
}
