// SPDX-License-Identifier: GPL-3.0-only

//! `RelayRuntime` — drains the ordered relay log and maps foreign rows to
//! `mind-core` [`SimCommand`]s (plan 21 §3.5, §6.3).
//!
//! Applied only at fixed-tick boundaries by `MindNet` (via
//! `MindSimHost::enqueue_sim_command`); never mid-frame. The client's own
//! echoed rows are skipped when predicted (`PredictPolicy::Immediate`) and
//! applied when awaited (`AwaitEcho`, I08 T6); both still advance the
//! `applied_command_id` watermark.

use std::collections::VecDeque;

use mind_core::determinism::{ConfigValue, SimCommand};
use mind_core::io::wire::{WireReader, WireWriter};
use mind_stdb::commands::predict_policy;
use mind_stdb::module_bindings::{
    BreakBlock, BuildingControlSelect, CommandBuilding, CommandKind, ConfigBlock, Custom,
    DeletePlans, Inventory, MatchCommand, Payload, PlaceBlock, Rotate, SetRules, UnitCommand,
    UnitCommandQueue, UnitControl, UnitStance,
};
use mind_stdb::{CommandStream, Connector, Identity, PredictPolicy};

/// Config-value wire tags ([`config_to_bytes`]/[`config_from_bytes`]).
mod config_tag {
    pub const NONE: u8 = 0;
    pub const BOOL: u8 = 1;
    pub const INT: u8 = 2;
    pub const FLOAT: u8 = 3;
    pub const CONTENT: u8 = 4;
    pub const BYTES: u8 = 5;
    pub const TEXT: u8 = 6;
    pub const POINT2: u8 = 7;
    pub const POINT2_ARRAY: u8 = 8;
    pub const CONTENT_ID: u8 = 9;
}

/// Content-name resolution for relay commands: the live registry on a client
/// (a stub in tests). Names, never ids, cross the wire.
pub trait RelayContent {
    /// Block id for a content name.
    fn block_id(&self, name: &str) -> Option<u16>;
    /// Block name for a content id.
    fn block_name(&self, id: u16) -> Option<String>;
    /// Item id for a content name.
    fn item_id(&self, name: &str) -> Option<u16>;
    /// Item name for a content id.
    fn item_name(&self, id: u16) -> Option<String>;
}

/// Encodes a [`ConfigValue`] for the relay `ConfigBlock.value` bytes (a
/// compact tag+payload table; the port's own format, I08 T6).
pub fn config_to_bytes(value: &ConfigValue) -> Vec<u8> {
    let mut out = Vec::new();
    let mut writer = WireWriter::new(&mut out);
    match value {
        ConfigValue::None => writer.ub(config_tag::NONE),
        ConfigValue::Bool(flag) => {
            writer.ub(config_tag::BOOL);
            writer.bool(*flag);
        }
        ConfigValue::Int(number) => {
            writer.ub(config_tag::INT);
            writer.i(*number);
        }
        ConfigValue::Float(number) => {
            writer.ub(config_tag::FLOAT);
            writer.f(*number);
        }
        ConfigValue::Content(name) => {
            writer.ub(config_tag::CONTENT);
            let _ = writer.str(name);
        }
        ConfigValue::Bytes(bytes) => {
            writer.ub(config_tag::BYTES);
            writer.us(bytes.len().min(u16::MAX as usize) as u16);
            writer.bytes(bytes);
        }
        ConfigValue::Text(text) => {
            writer.ub(config_tag::TEXT);
            let _ = writer.str(text);
        }
        ConfigValue::Point2(x, y) => {
            writer.ub(config_tag::POINT2);
            writer.i(*x);
            writer.i(*y);
        }
        ConfigValue::Point2Array(points) => {
            writer.ub(config_tag::POINT2_ARRAY);
            writer.us(points.len().min(u16::MAX as usize) as u16);
            for point in points {
                writer.i(*point);
            }
        }
        ConfigValue::ContentId { kind, id } => {
            writer.ub(config_tag::CONTENT_ID);
            writer.ub(*kind);
            writer.us(*id);
        }
    }
    out
}

/// Decodes a relay `ConfigBlock.value` blob (inverse of [`config_to_bytes`]).
pub fn config_from_bytes(bytes: &[u8]) -> Option<ConfigValue> {
    let mut reader = WireReader::new(bytes);
    let tag = reader.ub().ok()?;
    let value = match tag {
        config_tag::NONE => ConfigValue::None,
        config_tag::BOOL => ConfigValue::Bool(reader.bool().ok()?),
        config_tag::INT => ConfigValue::Int(reader.i().ok()?),
        config_tag::FLOAT => ConfigValue::Float(reader.f().ok()?),
        config_tag::CONTENT => ConfigValue::Content(reader.str().ok()?),
        config_tag::BYTES => {
            let len = reader.us().ok()? as usize;
            ConfigValue::Bytes(reader.bytes(len).ok()?.to_vec())
        }
        config_tag::TEXT => ConfigValue::Text(reader.str().ok()?),
        config_tag::POINT2 => ConfigValue::Point2(reader.i().ok()?, reader.i().ok()?),
        config_tag::POINT2_ARRAY => {
            let count = reader.us().ok()? as usize;
            let mut points = Vec::with_capacity(count);
            for _ in 0..count {
                points.push(reader.i().ok()?);
            }
            ConfigValue::Point2Array(points)
        }
        config_tag::CONTENT_ID => ConfigValue::ContentId {
            kind: reader.ub().ok()?,
            id: reader.us().ok()?,
        },
        _ => return None,
    };
    Some(value)
}

/// Maps a local player action to its relay variant (I08 T6). `None` for
/// commands with no relay equivalent ([`SimCommand::ConfigTap`],
/// [`SimCommand::SpawnUnit`]) — the caller applies those locally and logs.
pub fn sim_to_kind(command: &SimCommand, content: &dyn RelayContent) -> Option<CommandKind> {
    match command {
        SimCommand::Place {
            x,
            y,
            block,
            rotation,
            ..
        } => Some(CommandKind::PlaceBlock(PlaceBlock {
            x: i32::from(*x),
            y: i32::from(*y),
            block: content.block_name(*block)?,
            rotation: u8::try_from(*rotation).ok()?,
            config: Vec::new(),
        })),
        SimCommand::Break { x, y, .. } => Some(CommandKind::BreakBlock(BreakBlock {
            x: i32::from(*x),
            y: i32::from(*y),
        })),
        SimCommand::Configure { x, y, value } => Some(CommandKind::ConfigBlock(ConfigBlock {
            x: i32::from(*x),
            y: i32::from(*y),
            value: config_to_bytes(value),
        })),
        SimCommand::Rotate { x, y, direction } => Some(CommandKind::Rotate(Rotate {
            x: i32::from(*x),
            y: i32::from(*y),
            direction: *direction,
        })),
        SimCommand::DeletePlans { positions } => Some(CommandKind::DeletePlans(DeletePlans {
            positions: positions.to_vec(),
        })),
        SimCommand::CommandBuilding { positions, x, y } => {
            Some(CommandKind::CommandBuilding(CommandBuilding {
                positions: positions.to_vec(),
                x: *x,
                y: *y,
            }))
        }
        SimCommand::Inventory {
            kind,
            x,
            y,
            item,
            amount,
            angle,
        } => Some(CommandKind::Inventory(Inventory {
            kind: *kind,
            x: i32::from(*x),
            y: i32::from(*y),
            item: match item {
                Some(id) => Some(content.item_name(*id)?),
                None => None,
            },
            amount: *amount,
            angle: *angle,
        })),
        SimCommand::Payload { kind, x, y, target } => Some(CommandKind::Payload(Payload {
            kind: *kind,
            x: *x,
            y: *y,
            target: *target,
        })),
        SimCommand::UnitControl { unit } => {
            Some(CommandKind::UnitControl(UnitControl { unit: *unit }))
        }
        SimCommand::UnitClear => Some(CommandKind::UnitClear),
        SimCommand::BuildingControlSelect { x, y } => {
            Some(CommandKind::BuildingControlSelect(BuildingControlSelect {
                x: i32::from(*x),
                y: i32::from(*y),
            }))
        }
        SimCommand::UnitCommand {
            units,
            command,
            x,
            y,
            queue,
        } => {
            if *queue {
                Some(CommandKind::UnitCommandQueue(UnitCommandQueue {
                    units: units.to_vec(),
                    command: *command,
                    x: *x,
                    y: *y,
                }))
            } else {
                Some(CommandKind::UnitCommand(UnitCommand {
                    units: units.to_vec(),
                    command: *command,
                    x: *x,
                    y: *y,
                }))
            }
        }
        SimCommand::UnitStance {
            units,
            stance,
            enabled,
        } => Some(CommandKind::UnitStance(UnitStance {
            units: units.to_vec(),
            stance: *stance,
            enabled: *enabled,
        })),
        SimCommand::SetRules { json } => Some(CommandKind::SetRules(SetRules {
            rules_json: json.clone(),
            rules_epoch: 0,
        })),
        SimCommand::Custom { kind, data } => Some(CommandKind::Custom(Custom {
            kind: *kind,
            data: data.to_vec(),
        })),
        SimCommand::SpawnUnit { .. } | SimCommand::ConfigTap { .. } => None,
    }
}

/// Maps a relay variant to a canonical sim command, if it affects the sim.
///
/// Content names resolve through `content` (the live registry, never a
/// hardcoded table); `team` is the sender's team from the member table and
/// `player` the sender's player slot (I08 T6).
pub fn kind_to_sim(
    kind: &CommandKind,
    content: &dyn RelayContent,
    team: u8,
    player: Option<i32>,
) -> Option<SimCommand> {
    match kind {
        CommandKind::PlaceBlock(payload) => Some(SimCommand::Place {
            x: i16::try_from(payload.x).ok()?,
            y: i16::try_from(payload.y).ok()?,
            block: content.block_id(&payload.block)?,
            rotation: payload.rotation as i8,
            team,
            player,
        }),
        CommandKind::BreakBlock(payload) => Some(SimCommand::Break {
            x: i16::try_from(payload.x).ok()?,
            y: i16::try_from(payload.y).ok()?,
            player,
        }),
        CommandKind::ConfigBlock(payload) => Some(SimCommand::Configure {
            x: i16::try_from(payload.x).ok()?,
            y: i16::try_from(payload.y).ok()?,
            value: config_from_bytes(&payload.value)?,
        }),
        CommandKind::Rotate(payload) => Some(SimCommand::Rotate {
            x: i16::try_from(payload.x).ok()?,
            y: i16::try_from(payload.y).ok()?,
            direction: payload.direction,
        }),
        CommandKind::DeletePlans(payload) => Some(SimCommand::DeletePlans {
            positions: payload.positions.iter().copied().collect(),
        }),
        CommandKind::CommandBuilding(payload) => Some(SimCommand::CommandBuilding {
            positions: payload.positions.iter().copied().collect(),
            x: payload.x,
            y: payload.y,
        }),
        CommandKind::Inventory(payload) => Some(SimCommand::Inventory {
            kind: payload.kind,
            x: i16::try_from(payload.x).ok()?,
            y: i16::try_from(payload.y).ok()?,
            item: match &payload.item {
                Some(name) => Some(content.item_id(name)?),
                None => None,
            },
            amount: payload.amount,
            angle: payload.angle,
        }),
        CommandKind::Payload(payload) => Some(SimCommand::Payload {
            kind: payload.kind,
            x: payload.x,
            y: payload.y,
            target: payload.target,
        }),
        CommandKind::UnitControl(payload) => Some(SimCommand::UnitControl { unit: payload.unit }),
        CommandKind::UnitClear => Some(SimCommand::UnitClear),
        CommandKind::BuildingControlSelect(payload) => Some(SimCommand::BuildingControlSelect {
            x: i16::try_from(payload.x).ok()?,
            y: i16::try_from(payload.y).ok()?,
        }),
        CommandKind::UnitCommand(payload) => Some(SimCommand::UnitCommand {
            units: payload.units.iter().copied().collect(),
            command: payload.command,
            x: payload.x,
            y: payload.y,
            queue: false,
        }),
        CommandKind::UnitCommandQueue(payload) => Some(SimCommand::UnitCommand {
            units: payload.units.iter().copied().collect(),
            command: payload.command,
            x: payload.x,
            y: payload.y,
            queue: true,
        }),
        CommandKind::UnitStance(payload) => Some(SimCommand::UnitStance {
            units: payload.units.iter().copied().collect(),
            stance: payload.stance,
            enabled: payload.enabled,
        }),
        CommandKind::SetRules(payload) => Some(SimCommand::SetRules {
            json: payload.rules_json.clone(),
        }),
        CommandKind::Custom(payload) => Some(SimCommand::Custom {
            kind: payload.kind,
            data: payload.data.iter().copied().collect(),
        }),
        _ => None,
    }
}

/// Origin resolution for relayed commands: the sender's team and player slot
/// (I08 T6). Implemented by `MindNet` from the member table.
pub trait RelayOrigin {
    /// Team id to apply for a command sent by `sender`.
    fn team_for(&self, sender: Identity) -> u8;
    /// Player slot for `sender`.
    fn player_for(&self, sender: Identity) -> Option<i32>;
}

/// Per-match relay consumer (echo skip + foreign apply mapping).
pub struct RelayRuntime {
    stream: CommandStream,
    local: Option<Identity>,
    applied_command_id: u64,
    in_flight: VecDeque<u64>,
    applied: u64,
}

impl RelayRuntime {
    /// Subscribes the Game wave and binds `my_match_commands`.
    pub fn subscribe(connector: &mut Connector, match_id: u64) -> Self {
        Self {
            stream: CommandStream::subscribe(connector, match_id),
            local: None,
            applied_command_id: 0,
            in_flight: VecDeque::new(),
            applied: 0,
        }
    }

    /// Records the local identity so own echoes can be skipped.
    pub fn set_local_identity(&mut self, identity: Identity) {
        self.local = Some(identity);
    }

    /// Records a locally predicted command's client sequence.
    pub fn on_prediction(&mut self, sender_seq: u64) {
        self.in_flight.push_back(sender_seq);
    }

    /// Highest applied `command_id` (checkpoint watermark).
    pub fn last_applied_command_id(&self) -> u64 {
        self.applied_command_id
    }

    /// Rows applied from the log (foreign + own echoes advanced).
    pub fn applied_count(&self) -> u64 {
        self.applied
    }

    /// Queued (not yet drained) changes.
    pub fn queue_depth(&self) -> usize {
        self.stream.pending()
    }

    /// First stream order violation, as a diagnostic string.
    pub fn order_error(&self) -> Option<String> {
        self.stream.order_error().map(|error| format!("{error:?}"))
    }

    /// Marks every relayed command up to and including `command_id` as already
    /// applied: a loaded snapshot contains their effects, so the joiner skips
    /// the historical rows the game wave replays (I08 T5).
    pub fn skip_through(&mut self, command_id: u64) {
        self.applied_command_id = self.applied_command_id.max(command_id);
        self.in_flight.clear();
    }

    /// Drains rows and returns foreign commands to apply at the next tick.
    ///
    /// Own echoes with [`PredictPolicy::Immediate`] are skipped (the
    /// prediction already applied) and own [`PredictPolicy::AwaitEcho`]
    /// commands apply on the echo (its confirmation); both advance the
    /// watermark. Each row resolves its sender's team/slot through `origin`.
    pub fn drain(
        &mut self,
        content: &dyn RelayContent,
        origin: &dyn RelayOrigin,
    ) -> Vec<SimCommand> {
        let rows: Vec<MatchCommand> = self.stream.drain();
        let mut out = Vec::new();
        for row in rows {
            if row.command_id <= self.applied_command_id {
                // Already reflected (snapshot watermark or an earlier drain).
                continue;
            }
            self.applied_command_id = self.applied_command_id.max(row.command_id);
            self.applied = self.applied.saturating_add(1);
            if Some(row.sender) == self.local {
                // Own echo: one prediction resolved (best-effort FIFO).
                self.in_flight.pop_front();
                if predict_policy(&row.kind) == PredictPolicy::Immediate {
                    continue;
                }
            }
            let team = origin.team_for(row.sender);
            let player = origin.player_for(row.sender);
            if let Some(command) = kind_to_sim(&row.kind, content, team, player) {
                out.push(command);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    /// Two fixed content names; everything else unknown.
    struct Stub;

    impl RelayContent for Stub {
        fn block_id(&self, name: &str) -> Option<u16> {
            match name {
                "duo" => Some(7),
                "router" => Some(1),
                _ => None,
            }
        }
        fn block_name(&self, id: u16) -> Option<String> {
            match id {
                7 => Some("duo".to_string()),
                1 => Some("router".to_string()),
                _ => None,
            }
        }
        fn item_id(&self, name: &str) -> Option<u16> {
            (name == "copper").then_some(3)
        }
        fn item_name(&self, id: u16) -> Option<String> {
            (id == 3).then(|| "copper".to_string())
        }
    }

    /// Every sim command with a relay equivalent survives a round trip through
    /// the wire form (values compared; `Place.team/player` come from the
    /// receiver).
    #[test]
    fn command_round_trips() {
        let content = Stub;
        let commands = vec![
            SimCommand::Place {
                x: 12,
                y: 34,
                block: 7,
                rotation: 2,
                team: 1,
                player: Some(0),
            },
            SimCommand::Break {
                x: 5,
                y: 6,
                player: Some(0),
            },
            SimCommand::Configure {
                x: 1,
                y: 2,
                value: ConfigValue::Content("copper".to_string()),
            },
            SimCommand::Rotate {
                x: 3,
                y: 4,
                direction: true,
            },
            SimCommand::DeletePlans {
                positions: smallvec::smallvec![16, 32],
            },
            SimCommand::CommandBuilding {
                positions: smallvec::smallvec![16],
                x: 100.0,
                y: 200.0,
            },
            SimCommand::Inventory {
                kind: 0,
                x: 9,
                y: 9,
                item: Some(3),
                amount: 10,
                angle: 1.5,
            },
            SimCommand::Payload {
                kind: 1,
                x: 1.0,
                y: 2.0,
                target: Some(44),
            },
            SimCommand::UnitControl { unit: Some(9) },
            SimCommand::UnitClear,
            SimCommand::BuildingControlSelect { x: 7, y: 8 },
            SimCommand::UnitCommand {
                units: smallvec::smallvec![3, 4],
                command: 2,
                x: 10.0,
                y: 20.0,
                queue: false,
            },
            SimCommand::UnitCommand {
                units: smallvec::smallvec![5],
                command: 2,
                x: 10.0,
                y: 20.0,
                queue: true,
            },
            SimCommand::UnitStance {
                units: smallvec::smallvec![6],
                stance: 1,
                enabled: true,
            },
            SimCommand::SetRules {
                json: "{\"waves\":true}".to_string(),
            },
            SimCommand::Custom {
                kind: 77,
                data: smallvec::smallvec![1, 2, 3],
            },
        ];
        for command in &commands {
            let kind = sim_to_kind(command, &content)
                .unwrap_or_else(|| panic!("{command:?} maps to a relay kind"));
            let back = kind_to_sim(&kind, &content, 1, Some(0))
                .unwrap_or_else(|| panic!("{kind:?} maps back to a sim command"));
            assert_eq!(&back, command, "round trip failed for {command:?}");
        }
        assert!(sim_to_kind(&SimCommand::UnitClear, &content).is_some());
        assert!(
            sim_to_kind(
                &SimCommand::SpawnUnit {
                    unit: 1,
                    x: 0.0,
                    y: 0.0,
                    team: 0
                },
                &content
            )
            .is_none()
        );
        assert!(
            sim_to_kind(
                &SimCommand::ConfigTap {
                    x: 1,
                    y: 1,
                    other_x: 2,
                    other_y: 2
                },
                &content
            )
            .is_none()
        );
        // Unknown content names do not map (never a silent id 0).
        assert!(
            sim_to_kind(
                &SimCommand::Place {
                    x: 0,
                    y: 0,
                    block: 999,
                    rotation: 0,
                    team: 1,
                    player: None
                },
                &content
            )
            .is_none()
        );
    }

    /// The config byte codec is symmetric for every variant.
    #[test]
    fn config_value_codec_round_trips() {
        let values = vec![
            ConfigValue::None,
            ConfigValue::Bool(true),
            ConfigValue::Int(-7),
            ConfigValue::Float(1.25),
            ConfigValue::Content("copper".to_string()),
            ConfigValue::Bytes(vec![9, 8, 7]),
            ConfigValue::Text("hello".to_string()),
            ConfigValue::Point2(-3, 4),
            ConfigValue::Point2Array(vec![1, 2, 3]),
            ConfigValue::ContentId { kind: 1, id: 42 },
        ];
        for value in values {
            let bytes = config_to_bytes(&value);
            assert_eq!(config_from_bytes(&bytes), Some(value));
        }
        assert_eq!(config_from_bytes(&[]), None);
        assert_eq!(config_from_bytes(&[200]), None);
    }
}
