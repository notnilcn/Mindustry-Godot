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
pub mod directional_unloader;
pub mod duct;
pub mod duct_bridge;
pub mod duct_junction;
pub mod duct_router;
pub mod item_bridge;
pub mod junction;
pub mod mass_driver;
pub mod overflow_duct;
pub mod overflow_gate;
pub mod router;
pub mod sorter;
pub mod stack_conveyor;
pub mod stack_router;
pub mod transfer;

use std::sync::Arc;

use crate::content::{BlockKind, ContentRegistry};
use crate::world::behavior::{BehaviorRegistry, BuildingBehavior};

pub use buffered_item_bridge::{BufferedItemBridgeBehavior, BufferedItemBridgeBuild};
pub use chained_building::ChainedBuilding;
pub use conveyor::{CAPACITY as CONVEYOR_CAPACITY, ConveyorBehavior, ConveyorBuild};
pub use direction_bridge::{DirectionBridgeBuild, find_link as direction_find_link};
pub use directional_unloader::{DirectionalUnloaderBehavior, DirectionalUnloaderBuild};
pub use duct::{DuctBehavior, DuctBuild};
pub use duct_bridge::{DuctBridgeBehavior, DuctBridgeBuild};
pub use duct_junction::{DuctJunctionBehavior, DuctJunctionBuild};
pub use duct_router::{DuctRouterBehavior, DuctRouterBuild};
pub use item_bridge::{ItemBridgeBehavior, ItemBridgeBuild};
pub use junction::{JunctionBehavior, JunctionBuild};
pub use mass_driver::{
    DriverBulletData, DriverState, MassDriverBehavior, MassDriverBuild, MassDriverCarrier,
    MassDriverPayloadCarrier, TestBoltCarrier,
};
pub use overflow_duct::{OverflowDuctBehavior, OverflowDuctBuild};
pub use overflow_gate::OverflowGateBehavior;
pub use router::{RouterBehavior, RouterBuild};
pub use sorter::{SorterBehavior, SorterBuild};
pub use stack_conveyor::{StackConveyorBehavior, StackConveyorBuild};
pub use stack_router::{StackRouterBehavior, StackRouterBuild};

/// Class-default behavior for a distribution [`BlockKind`] (GAPS #55).
///
/// Upstream dispatch is by Java class, so a mod/JSON block tagged
/// `Conveyor`/`Router`/… runs the same `*Build` code as the vanilla block. The
/// values below are the class presets the port can express without extra
/// `BlockDef` fields (per-block `Conveyor.speed` etc. are the reported #56
/// follow-up); every vanilla block is still registered by name in [`register`].
pub fn class_default(kind: BlockKind) -> Option<Arc<dyn BuildingBehavior>> {
    Some(match kind {
        BlockKind::Conveyor => Arc::new(ConveyorBehavior::BASE),
        BlockKind::ArmoredConveyor => Arc::new(ConveyorBehavior::ARMORED),
        BlockKind::StackConveyor => Arc::new(StackConveyorBehavior::PLASTANIUM),
        BlockKind::Junction => Arc::new(JunctionBehavior::VANILLA),
        BlockKind::Router => Arc::new(RouterBehavior::VANILLA),
        BlockKind::DuctRouter => Arc::new(DuctRouterBehavior::VANILLA),
        BlockKind::StackRouter => Arc::new(StackRouterBehavior::SURGE),
        BlockKind::Sorter => Arc::new(SorterBehavior::NORMAL),
        BlockKind::OverflowGate => Arc::new(OverflowGateBehavior::NORMAL),
        BlockKind::OverflowDuct => Arc::new(OverflowDuctBehavior::NORMAL),
        BlockKind::Duct => Arc::new(DuctBehavior::NORMAL),
        BlockKind::DuctBridge => Arc::new(DuctBridgeBehavior::VANILLA),
        BlockKind::DirectionalUnloader => Arc::new(DirectionalUnloaderBehavior::VANILLA),
        // `ItemBridge` is the non-buffered `transportTime` bridge; the buffered
        // subclass (`BufferedItemBridge`, `bridge-conveyor`) carries the item
        // buffer the factory preset configures.
        BlockKind::ItemBridge => Arc::new(ItemBridgeBehavior {
            range: 4,
            transport_time: 0.0,
            link_same_type: true,
            buffered: false,
            buffer_capacity: 0,
            speed: 0.0,
            displayed_speed: 0.0,
            no_accept_disabled: false,
        }),
        BlockKind::BufferedItemBridge => Arc::new(ItemBridgeBehavior::BRIDGE_CONVEYOR),
        BlockKind::MassDriver => Arc::new(MassDriverBehavior::VANILLA),
        _ => return None,
    })
}

/// Registers every item-distribution behavior available at this milestone.
pub fn register(registry: &mut BehaviorRegistry, _content: &ContentRegistry) {
    // Class-keyed defaults first (GAPS #55); name registrations below stay the
    // more specific override, and vanilla blocks keep their exact presets.
    for kind in [
        BlockKind::Conveyor,
        BlockKind::ArmoredConveyor,
        BlockKind::StackConveyor,
        BlockKind::Junction,
        BlockKind::Router,
        BlockKind::DuctRouter,
        BlockKind::StackRouter,
        BlockKind::Sorter,
        BlockKind::OverflowGate,
        BlockKind::OverflowDuct,
        BlockKind::Duct,
        BlockKind::DuctBridge,
        BlockKind::DirectionalUnloader,
        BlockKind::ItemBridge,
        BlockKind::BufferedItemBridge,
        BlockKind::MassDriver,
    ] {
        if let Some(behavior) = class_default(kind) {
            registry.register_kind(kind, behavior);
        }
    }
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
    registry.register_named("duct-router", Arc::new(DuctRouterBehavior::VANILLA));
    registry.register_named("surge-router", Arc::new(StackRouterBehavior::SURGE));
    registry.register_named("duct-junction", Arc::new(DuctJunctionBehavior::VANILLA));
    registry.register_named("mass-driver", Arc::new(MassDriverBehavior::VANILLA));
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
    registry.register_named(
        "duct-unloader",
        Arc::new(DirectionalUnloaderBehavior::VANILLA),
    );
}
