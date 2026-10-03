// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `RemoteAction` → `SimCommand` builders + relay batching (plan 15 §3.3/§6.4).

use smallvec::SmallVec;

use crate::determinism::SimCommand;
use crate::world::config::ConfigValue as WorldConfigValue;

use super::action::{CommandTarget, RemoteAction};

/// Unit-id chunk size per relay batch (`commandUnits`, §6.4).
pub const COMMAND_CHUNK: usize = 200;
/// Delete-plan positions per command (cap; larger lists emit consecutively).
pub const DELETE_POSITION_CAP: usize = 256;
/// `Custom` kind used for a unit stance until plan 05 adds a typed variant
/// (documented fallback; 11 §6.5).
pub const CUSTOM_UNIT_STANCE: u16 = 102;

/// One ordered relay batch (`ActionBatch`, §6.4).
#[derive(Debug, Clone, PartialEq)]
pub struct ActionBatch {
    /// Match id.
    pub match_id: u64,
    /// Monotonic client sequence (echo dedup key, plan 21).
    pub client_seq: u32,
    /// Sim tick the commands are stamped with.
    pub tick: u64,
    /// Commands in application order.
    pub commands: SmallVec<[SimCommand; 4]>,
}

/// Error surfaced to plan 14 as a toast for explicit gestures (§3.3.3).
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ActionError {
    /// The action target was invalid.
    #[error("invalid action target")]
    InvalidTarget,
    /// No units were selected.
    #[error("no units selected")]
    NoUnits,
    /// The player is not a builder.
    #[error("not a builder")]
    NotBuilder,
    /// The action was rate-limited.
    #[error("rate limited")]
    RateLimited,
    /// The action has no `SimCommand` mapping yet.
    #[error("unsupported action")]
    Unsupported,
}

/// Builder that assigns client sequence numbers and chunks commands.
#[derive(Debug, Clone)]
pub struct ActionBatcher {
    match_id: u64,
    next_seq: u32,
}

impl ActionBatcher {
    /// Creates a batcher for a match.
    pub fn new(match_id: u64) -> Self {
        Self {
            match_id,
            next_seq: 0,
        }
    }

    /// Current client sequence (the next one to be assigned).
    pub fn next_seq(&self) -> u32 {
        self.next_seq
    }

    /// Turns one action into zero or more ordered [`ActionBatch`]es.
    pub fn emit(
        &mut self,
        tick: u64,
        action: RemoteAction,
    ) -> Result<SmallVec<[ActionBatch; 4]>, ActionError> {
        let mut batches: SmallVec<[ActionBatch; 4]> = SmallVec::new();
        match action {
            RemoteAction::Ping { .. } | RemoteAction::TileTap { .. } => {
                // Local-only / 21 transport events; no SimCommand.
            }
            RemoteAction::Rotate { x, y, direction } => {
                batches.push(self.batch(
                    tick,
                    smallvec::smallvec![SimCommand::Rotate { x, y, direction }],
                ));
            }
            RemoteAction::Configure { x, y, value } => {
                batches.push(self.batch(
                    tick,
                    smallvec::smallvec![SimCommand::Configure {
                        x,
                        y,
                        value: to_sim_config(&value),
                    }],
                ));
            }
            RemoteAction::DeletePlans { positions } => {
                if positions.is_empty() {
                    return Err(ActionError::InvalidTarget);
                }
                let mut commands: SmallVec<[SimCommand; 4]> = SmallVec::new();
                for chunk in positions.chunks(DELETE_POSITION_CAP) {
                    commands.push(SimCommand::DeletePlans {
                        positions: SmallVec::from_slice(chunk),
                    });
                }
                batches.push(self.batch(tick, commands));
            }
            RemoteAction::CommandUnits {
                units,
                target,
                queue,
                final_batch,
            } => {
                if units.is_empty() {
                    return Err(ActionError::NoUnits);
                }
                let (x, y) = match target {
                    CommandTarget::Position { x, y } => (x, y),
                    CommandTarget::Unit(id) => (id as f32, 0.0),
                    CommandTarget::Building(pos) => (pos.x() as f32, pos.y() as f32),
                };
                let _ = queue;
                let _ = final_batch;
                for chunk in units.chunks(COMMAND_CHUNK) {
                    batches.push(self.batch(
                        tick,
                        smallvec::smallvec![SimCommand::UnitCommand {
                            units: SmallVec::from_slice(chunk),
                            command: 0,
                            x,
                            y,
                        }],
                    ));
                }
            }
            RemoteAction::SetUnitCommand { units, command } => {
                if units.is_empty() {
                    return Err(ActionError::NoUnits);
                }
                batches.push(self.batch(
                    tick,
                    smallvec::smallvec![SimCommand::UnitCommand {
                        units,
                        command,
                        x: 0.0,
                        y: 0.0,
                    }],
                ));
            }
            RemoteAction::SetUnitStance {
                units,
                stance,
                enabled,
            } => {
                if units.is_empty() {
                    return Err(ActionError::NoUnits);
                }
                let mut data: SmallVec<[u8; 32]> = SmallVec::new();
                data.extend_from_slice(&stance.to_le_bytes());
                data.push(u8::from(enabled));
                batches.push(self.batch(
                    tick,
                    smallvec::smallvec![SimCommand::Custom {
                        kind: CUSTOM_UNIT_STANCE,
                        data,
                    }],
                ));
            }
            RemoteAction::CommandBuilding { positions, x, y } => {
                if positions.is_empty() {
                    return Err(ActionError::InvalidTarget);
                }
                batches.push(self.batch(
                    tick,
                    smallvec::smallvec![SimCommand::CommandBuilding { positions, x, y }],
                ));
            }
            RemoteAction::Inventory {
                kind,
                x,
                y,
                item,
                amount,
                angle,
            } => {
                batches.push(self.batch(
                    tick,
                    smallvec::smallvec![SimCommand::Inventory {
                        kind: kind.code(),
                        x,
                        y,
                        item,
                        amount,
                        angle,
                    }],
                ));
            }
            RemoteAction::Payload { kind, x, y, target } => {
                batches.push(self.batch(
                    tick,
                    smallvec::smallvec![SimCommand::Payload {
                        kind: kind.code(),
                        x,
                        y,
                        target,
                    }],
                ));
            }
            RemoteAction::UnitControl { unit } => {
                batches
                    .push(self.batch(tick, smallvec::smallvec![SimCommand::UnitControl { unit }]));
            }
            RemoteAction::UnitClear => {
                batches.push(self.batch(tick, smallvec::smallvec![SimCommand::UnitClear]));
            }
            RemoteAction::BuildingControlSelect { x, y } => {
                batches.push(self.batch(
                    tick,
                    smallvec::smallvec![SimCommand::BuildingControlSelect { x, y }],
                ));
            }
        }
        Ok(batches)
    }

    fn batch(&mut self, tick: u64, commands: SmallVec<[SimCommand; 4]>) -> ActionBatch {
        let client_seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        ActionBatch {
            match_id: self.match_id,
            client_seq,
            tick,
            commands,
        }
    }
}

/// Best-effort `world::config::ConfigValue` → `determinism::ConfigValue` bridge.
///
/// The two enums are owned by plans 07 and 05; a typed bridge is the plan-07
/// seam. Network plan configs are `Number|Boolean|Content` only (07 §6.3), which
/// this mapping preserves exactly.
pub fn to_sim_config(value: &WorldConfigValue) -> crate::determinism::ConfigValue {
    use crate::determinism::ConfigValue as SimValue;
    match value {
        WorldConfigValue::None => SimValue::None,
        WorldConfigValue::Bool(value) => SimValue::Bool(*value),
        WorldConfigValue::Number(value) => SimValue::Float(*value as f32),
        WorldConfigValue::String(value) => SimValue::Content(value.clone()),
        WorldConfigValue::Bytes(value) => SimValue::Bytes(value.to_vec()),
        WorldConfigValue::Point2(x, y) => {
            SimValue::Int(crate::world::TilePos::new(*x as i16, *y as i16).pack())
        }
        _ => SimValue::Content("complex".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::action::{InventoryKind, PayloadAction};

    #[test]
    fn chunk_200_final_batch() {
        let mut batcher = ActionBatcher::new(7);
        let units: SmallVec<[i32; 32]> = (0..450).collect();
        let batches = batcher
            .emit(
                10,
                RemoteAction::CommandUnits {
                    units,
                    target: CommandTarget::Position { x: 1.0, y: 2.0 },
                    queue: false,
                    final_batch: true,
                },
            )
            .expect("emit");
        assert_eq!(batches.len(), 3);
        assert_eq!(
            batches[0].commands[0],
            SimCommand::UnitCommand {
                units: (0..200).collect(),
                command: 0,
                x: 1.0,
                y: 2.0,
            }
        );
        assert_eq!(
            batches[2].commands[0],
            SimCommand::UnitCommand {
                units: (400..450).collect(),
                command: 0,
                x: 1.0,
                y: 2.0,
            }
        );
        assert_eq!(batches[0].client_seq, 0);
        assert_eq!(batches[1].client_seq, 1);
        assert_eq!(batches[2].client_seq, 2);
        assert_eq!(batches[0].match_id, 7);
        assert_eq!(batches[0].tick, 10);
    }

    #[test]
    fn delete_plans_positions() {
        let mut batcher = ActionBatcher::new(1);
        let positions: SmallVec<[i32; 32]> = (0..300).collect();
        let batches = batcher
            .emit(0, RemoteAction::DeletePlans { positions })
            .expect("emit");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].commands.len(), 2);
        assert_eq!(
            batches[0].commands[0],
            SimCommand::DeletePlans {
                positions: (0..256).collect(),
            }
        );
        assert_eq!(
            batches[0].commands[1],
            SimCommand::DeletePlans {
                positions: (256..300).collect(),
            }
        );
    }

    #[test]
    fn rotate_block_direction() {
        let mut batcher = ActionBatcher::new(1);
        let batches = batcher
            .emit(
                4,
                RemoteAction::Rotate {
                    x: 5,
                    y: -6,
                    direction: true,
                },
            )
            .expect("emit");
        assert_eq!(
            batches[0].commands[0],
            SimCommand::Rotate {
                x: 5,
                y: -6,
                direction: true,
            }
        );
    }

    #[test]
    fn inventory_kinds() {
        let mut batcher = ActionBatcher::new(1);
        for (kind, code) in [
            (InventoryKind::Withdraw, 0u8),
            (InventoryKind::Deposit, 1),
            (InventoryKind::DropItem, 2),
        ] {
            let batches = batcher
                .emit(
                    0,
                    RemoteAction::Inventory {
                        kind,
                        x: 1,
                        y: 2,
                        item: Some(3),
                        amount: 10,
                        angle: 0.5,
                    },
                )
                .expect("emit");
            assert_eq!(
                batches[0].commands[0],
                SimCommand::Inventory {
                    kind: code,
                    x: 1,
                    y: 2,
                    item: Some(3),
                    amount: 10,
                    angle: 0.5,
                }
            );
        }
    }

    #[test]
    fn payload_kinds() {
        let mut batcher = ActionBatcher::new(1);
        for (kind, code) in [
            (PayloadAction::PickupUnit, 0u8),
            (PayloadAction::PickupBuild, 1),
            (PayloadAction::Drop, 2),
        ] {
            let batches = batcher
                .emit(
                    0,
                    RemoteAction::Payload {
                        kind,
                        x: 1.0,
                        y: 2.0,
                        target: Some(9),
                    },
                )
                .expect("emit");
            assert_eq!(
                batches[0].commands[0],
                SimCommand::Payload {
                    kind: code,
                    x: 1.0,
                    y: 2.0,
                    target: Some(9),
                }
            );
        }
    }

    #[test]
    fn local_only_and_empty_errors() {
        let mut batcher = ActionBatcher::new(1);
        assert!(
            batcher
                .emit(0, RemoteAction::Ping { x: 0.0, y: 0.0 })
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            batcher
                .emit(
                    0,
                    RemoteAction::CommandUnits {
                        units: SmallVec::new(),
                        target: CommandTarget::Position { x: 0.0, y: 0.0 },
                        queue: false,
                        final_batch: true,
                    },
                )
                .unwrap_err(),
            ActionError::NoUnits
        );
    }
}
