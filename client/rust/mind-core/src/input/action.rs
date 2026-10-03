// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RemoteAction` — every client mutation expressed once (plan 15 §3.3).
//!
//! `command_emit` turns one action into one or more `SimCommand`s and applies
//! them locally in the same order they are appended to the relay batch (D2/21).

use smallvec::SmallVec;

use crate::world::TilePos;
use crate::world::config::ConfigValue;

/// Inventory mutation kind (`@Remote withdraw/deposit/drop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryKind {
    /// `requestItem`.
    Withdraw,
    /// `transferInventory`.
    Deposit,
    /// `dropItem`.
    DropItem,
}

impl InventoryKind {
    /// Wire/`SimCommand` discriminant.
    pub const fn code(self) -> u8 {
        match self {
            InventoryKind::Withdraw => 0,
            InventoryKind::Deposit => 1,
            InventoryKind::DropItem => 2,
        }
    }
}

/// Payload action kind (`@Remote request*Payload`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadAction {
    /// `requestUnitPayload`.
    PickupUnit,
    /// `requestBuildPayload`.
    PickupBuild,
    /// `requestDropPayload`.
    Drop,
}

impl PayloadAction {
    /// Wire/`SimCommand` discriminant.
    pub const fn code(self) -> u8 {
        match self {
            PayloadAction::PickupUnit => 0,
            PayloadAction::PickupBuild => 1,
            PayloadAction::Drop => 2,
        }
    }
}

/// Resolved command target for `commandUnits`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandTarget {
    /// Ground position (world pixels).
    Position { x: f32, y: f32 },
    /// Enemy unit entity id.
    Unit(i32),
    /// Enemy building tile.
    Building(TilePos),
}

/// One client mutation (plan §3.3 table).
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteAction {
    /// `rotateBlock` on a placed building.
    Rotate {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// `false` clockwise, `true` counter-clockwise.
        direction: bool,
    },
    /// `tileConfig`.
    Configure {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// Config value.
        value: ConfigValue,
    },
    /// `deletePlans`/`removeQueueBlock`.
    DeletePlans {
        /// Packed positions.
        positions: SmallVec<[i32; 32]>,
    },
    /// `commandUnits`.
    CommandUnits {
        /// Unit entity ids.
        units: SmallVec<[i32; 32]>,
        /// Resolved target.
        target: CommandTarget,
        /// Queue mode.
        queue: bool,
        /// Last chunk of a formation batch.
        final_batch: bool,
    },
    /// `setUnitCommand`.
    SetUnitCommand {
        /// Unit entity ids.
        units: SmallVec<[i32; 32]>,
        /// Command content id.
        command: u16,
    },
    /// `setUnitStance`.
    SetUnitStance {
        /// Unit entity ids.
        units: SmallVec<[i32; 32]>,
        /// Stance content id.
        stance: u16,
        /// Enabled flag.
        enabled: bool,
    },
    /// `commandBuilding`.
    CommandBuilding {
        /// Packed building positions.
        positions: SmallVec<[i32; 32]>,
        /// Target x.
        x: f32,
        /// Target y.
        y: f32,
    },
    /// `requestItem`/`transferInventory`/`dropItem`.
    Inventory {
        /// Kind.
        kind: InventoryKind,
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
        /// Item id.
        item: Option<u16>,
        /// Amount.
        amount: i32,
        /// Angle.
        angle: f32,
    },
    /// Payload pickup/drop.
    Payload {
        /// Kind.
        kind: PayloadAction,
        /// X.
        x: f32,
        /// Y.
        y: f32,
        /// Target id.
        target: Option<i32>,
    },
    /// `unitControl`.
    UnitControl {
        /// Unit entity id, or `None` to clear.
        unit: Option<i32>,
    },
    /// `unitClear`.
    UnitClear,
    /// `buildingControlSelect`.
    BuildingControlSelect {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
    },
    /// `pingLocation` (21 event, no `SimCommand`).
    Ping {
        /// X.
        x: f32,
        /// Y.
        y: f32,
    },
    /// `tileTap` (21 event, no `SimCommand`).
    TileTap {
        /// Tile x.
        x: i16,
        /// Tile y.
        y: i16,
    },
}

impl RemoteAction {
    /// Parity action name (state dump / relay debugging).
    pub const fn name(&self) -> &'static str {
        match self {
            RemoteAction::Rotate { .. } => "rotate",
            RemoteAction::Configure { .. } => "configure",
            RemoteAction::DeletePlans { .. } => "delete_plans",
            RemoteAction::CommandUnits { .. } => "command_units",
            RemoteAction::SetUnitCommand { .. } => "set_unit_command",
            RemoteAction::SetUnitStance { .. } => "set_unit_stance",
            RemoteAction::CommandBuilding { .. } => "command_building",
            RemoteAction::Inventory { .. } => "inventory",
            RemoteAction::Payload { .. } => "payload",
            RemoteAction::UnitControl { .. } => "unit_control",
            RemoteAction::UnitClear => "unit_clear",
            RemoteAction::BuildingControlSelect { .. } => "building_control_select",
            RemoteAction::Ping { .. } => "ping",
            RemoteAction::TileTap { .. } => "tile_tap",
        }
    }

    /// Whether the action is relayed (vs local-only/21 event).
    pub const fn relayed(&self) -> bool {
        !matches!(
            self,
            RemoteAction::Ping { .. } | RemoteAction::TileTap { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_names_and_relay_flags() {
        assert_eq!(RemoteAction::UnitClear.name(), "unit_clear");
        assert!(RemoteAction::UnitClear.relayed());
        assert!(!RemoteAction::Ping { x: 0.0, y: 0.0 }.relayed());
        assert!(!RemoteAction::TileTap { x: 0, y: 0 }.relayed());
        assert_eq!(InventoryKind::Withdraw.code(), 0);
        assert_eq!(InventoryKind::Deposit.code(), 1);
        assert_eq!(InventoryKind::DropItem.code(), 2);
        assert_eq!(PayloadAction::Drop.code(), 2);
    }
}
