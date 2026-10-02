// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Item distribution family (`world/blocks/distribution/`) — plan 08.
//!
//! `LogisticsPlugin` equivalent: [`register`] installs one [`BuildingBehavior`]
//! per vanilla transport block into plan 07's [`BehaviorRegistry`]. The shared
//! transfer helpers live in [`transfer`]; the autotiler is
//! [`crate::world::blocks::autotiler`] and the 47-slice table is
//! [`crate::world::blocks::tile_bitmask`] (both consumed by plan 09).

pub mod buffered_item_bridge;
pub mod chained_building;
pub mod conveyor;
pub mod direction_bridge;
pub mod duct;
pub mod duct_bridge;
pub mod item_bridge;
pub mod junction;
pub mod overflow_duct;
pub mod overflow_gate;
pub mod router;
pub mod sorter;
pub mod stack_conveyor;
pub mod transfer;

use std::sync::Arc;

use crate::content::ContentRegistry;
use crate::world::behavior::BehaviorRegistry;

pub use buffered_item_bridge::{BufferedItemBridgeBehavior, BufferedItemBridgeBuild};
pub use chained_building::ChainedBuilding;
pub use conveyor::{CAPACITY as CONVEYOR_CAPACITY, ConveyorBehavior, ConveyorBuild};
pub use direction_bridge::{DirectionBridgeBuild, find_link as direction_find_link};
pub use duct::{DuctBehavior, DuctBuild};
pub use duct_bridge::{DuctBridgeBehavior, DuctBridgeBuild};
pub use item_bridge::{ItemBridgeBehavior, ItemBridgeBuild};
pub use junction::{JunctionBehavior, JunctionBuild};
pub use overflow_duct::{OverflowDuctBehavior, OverflowDuctBuild};
pub use overflow_gate::OverflowGateBehavior;
pub use router::{RouterBehavior, RouterBuild};
pub use sorter::{SorterBehavior, SorterBuild};
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
    registry.register_named("junction", Arc::new(JunctionBehavior::VANILLA));
    registry.register_named("router", Arc::new(RouterBehavior::VANILLA));
    registry.register_named("distributor", Arc::new(RouterBehavior::VANILLA));
    registry.register_named("sorter", Arc::new(SorterBehavior::NORMAL));
    registry.register_named("inverted-sorter", Arc::new(SorterBehavior::INVERTED));
    registry.register_named("overflow-gate", Arc::new(OverflowGateBehavior::NORMAL));
    registry.register_named("underflow-gate", Arc::new(OverflowGateBehavior::UNDERFLOW));
    registry.register_named("overflow-duct", Arc::new(OverflowDuctBehavior::NORMAL));
    registry.register_named("underflow-duct", Arc::new(OverflowDuctBehavior::UNDERFLOW));
    registry.register_named(
        "phase-conveyor",
        Arc::new(ItemBridgeBehavior::PHASE_CONVEYOR),
    );
    registry.register_named(
        "bridge-conveyor",
        Arc::new(ItemBridgeBehavior::BRIDGE_CONVEYOR),
    );
    registry.register_named("duct-bridge", Arc::new(DuctBridgeBehavior::VANILLA));
}
