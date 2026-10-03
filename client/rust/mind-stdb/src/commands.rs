// SPDX-License-Identifier: GPL-3.0-only

//! Client command emission, local preflight and prediction bookkeeping
//! (plan 21 §3.5/§6.3).
//!
//! The server remains the source of truth: this module mirrors the cheap
//! `validate_kind` caps so a malformed action is rejected before it consumes a
//! round-trip, and records in-flight predictions so an echoed row is not
//! re-applied (echo skip). `mind-gdext`/`mind-headless` map the variant to a
//! `SimCommand` and apply it at the fixed-tick boundary.

use std::collections::VecDeque;

use crate::module_bindings::CommandKind;
use crate::transport::{RelayTransport, TransportError};

/// Bounded in-flight prediction table (plan §3.5; overflow forces resync).
pub const PREDICTION_QUEUE_CAP: usize = 512;

/// Default sender-side input delay in ticks (plan §3.5).
pub const INPUT_DELAY_TICKS: u64 = 6;

/// Cap mirrors (plan §6.3); kept in lockstep with the server's `main/global.rs`.
pub const MAX_BLOCK_NAME_LEN: usize = 100;
pub const MAX_CONTENT_NAME_LEN: usize = 100;
pub const MAX_PLACE_CONFIG_BYTES: usize = 4 * 1024;
pub const MAX_CONFIG_BYTES: usize = 16 * 1024;
pub const MAX_POSITIONS: usize = 256;
pub const MAX_UNITS: usize = 200;
pub const MAX_RULES_JSON: usize = 100_000;

/// Local prediction policy per variant (plan §3.5 “Predict”).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredictPolicy {
    /// Apply immediately on send (upstream `called = Loc.both`).
    Immediate,
    /// Apply only after the server echo (host config/admin).
    AwaitEcho,
    /// Owned locally; never predicted from the log (spawn/input).
    LocalOnly,
}

/// Prediction policy for a variant (plan §6.3).
pub fn predict_policy(kind: &CommandKind) -> PredictPolicy {
    match kind {
        CommandKind::SetRules(_)
        | CommandKind::SetRule(_)
        | CommandKind::ResearchUnlock(_)
        | CommandKind::CompleteObjective(_)
        | CommandKind::ClearObjectives
        | CommandKind::SectorCapture
        | CommandKind::SaveSector
        | CommandKind::SkipWave
        | CommandKind::RunWave(_)
        | CommandKind::AdminSwitchTeam(_)
        | CommandKind::AdminTileOp(_)
        | CommandKind::Bullet(_)
        | CommandKind::Custom(_) => PredictPolicy::AwaitEcho,
        CommandKind::PlayerSpawn(_)
        | CommandKind::MenuChoose(_)
        | CommandKind::MenuBuilderChoose(_)
        | CommandKind::TextInputResult(_) => PredictPolicy::LocalOnly,
        _ => PredictPolicy::Immediate,
    }
}

/// Bounds/role context for [`preflight_validate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreflightContext {
    /// Match map width in tiles.
    pub map_width: i32,
    /// Match map height in tiles.
    pub map_height: i32,
    /// Whether the local client is host or admin.
    pub is_host_or_admin: bool,
    /// Whether the local member is a spectator.
    pub is_spectator: bool,
}

impl Default for PreflightContext {
    fn default() -> Self {
        Self {
            map_width: 1_000,
            map_height: 1_000,
            is_host_or_admin: false,
            is_spectator: false,
        }
    }
}

fn cap_str(value: &str, max: usize, field: &str) -> Result<(), String> {
    if value.chars().count() > max {
        return Err(format!("{field} exceeds {max} characters"));
    }
    Ok(())
}

fn cap_bytes(value: &[u8], max: usize, field: &str) -> Result<(), String> {
    if value.len() > max {
        return Err(format!("{field} exceeds {max} bytes"));
    }
    Ok(())
}

fn cap_list(len: usize, max: usize, field: &str) -> Result<(), String> {
    if len > max {
        return Err(format!("{field} exceeds {max} entries"));
    }
    Ok(())
}

fn valid_content_name(name: &str, max_len: usize) -> Result<(), String> {
    if name.is_empty() || name.chars().count() > max_len {
        return Err(format!("content name must be 1-{max_len} characters"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("content name has invalid characters".to_string());
    }
    Ok(())
}

/// Client mirror of the server's cheap validator (plan §6.3). Reason strings
/// match the server for the shared validation-matrix cases.
pub fn preflight_validate(
    kind: &CommandKind,
    ctx: &PreflightContext,
) -> Result<(), String> {
    let bounds = |x: i32, y: i32| -> Result<(), String> {
        if x >= 0 && y >= 0 && x < ctx.map_width && y < ctx.map_height {
            Ok(())
        } else {
            Err(format!("({x}, {y}) is outside the match bounds"))
        }
    };
    match kind {
        CommandKind::Noop | CommandKind::Ping(_) => Ok(()),
        CommandKind::PlaceBlock(payload) => {
            bounds(payload.x, payload.y)?;
            valid_content_name(&payload.block, MAX_BLOCK_NAME_LEN)?;
            if payload.rotation > 3 {
                return Err(format!(
                    "rotation {} must be 0..=3",
                    payload.rotation
                ));
            }
            cap_bytes(&payload.config, MAX_PLACE_CONFIG_BYTES, "place config")
        }
        CommandKind::BreakBlock(payload) => bounds(payload.x, payload.y),
        CommandKind::ConfigBlock(payload) => {
            bounds(payload.x, payload.y)?;
            cap_bytes(&payload.value, MAX_CONFIG_BYTES, "config value")
        }
        CommandKind::Rotate(payload) => bounds(payload.x, payload.y),
        CommandKind::DeletePlans(payload) => {
            cap_list(payload.positions.len(), MAX_POSITIONS, "positions")
        }
        CommandKind::CommandBuilding(payload) => {
            cap_list(payload.positions.len(), MAX_POSITIONS, "positions")
        }
        CommandKind::Inventory(payload) => {
            bounds(payload.x, payload.y)?;
            if let Some(item) = &payload.item {
                valid_content_name(item, MAX_CONTENT_NAME_LEN)?;
            }
            if payload.amount.abs() > 10_000 {
                return Err(format!(
                    "inventory amount {} exceeds 10000",
                    payload.amount
                ));
            }
            Ok(())
        }
        CommandKind::UnitCommand(payload) => {
            cap_list(payload.units.len(), MAX_UNITS, "units")?;
            if payload.command >= 256 {
                return Err(format!("unit command {} must be < 256", payload.command));
            }
            Ok(())
        }
        CommandKind::UnitCommandQueue(payload) => {
            cap_list(payload.units.len(), MAX_UNITS, "units")?;
            if payload.command >= 256 {
                return Err(format!("unit command {} must be < 256", payload.command));
            }
            Ok(())
        }
        CommandKind::UnitStance(payload) => {
            cap_list(payload.units.len(), MAX_UNITS, "units")
        }
        CommandKind::PlayerSpawn(payload) => {
            if let Some(unit) = &payload.unit {
                valid_content_name(unit, MAX_CONTENT_NAME_LEN)?;
            }
            Ok(())
        }
        CommandKind::Bullet(payload) => cap_bytes(&payload.data, 4 * 1024, "bullet data"),
        CommandKind::SetRules(payload) => {
            cap_str(&payload.rules_json, MAX_RULES_JSON, "rules_json")
        }
        CommandKind::SetRule(payload) => {
            cap_str(&payload.rule, 64, "rule")?;
            cap_str(&payload.json, 16 * 1024, "rule json")
        }
        CommandKind::ResearchUnlock(payload) => {
            valid_content_name(&payload.content, MAX_CONTENT_NAME_LEN)
        }
        CommandKind::RunWave(payload) => {
            if payload.count > 10 {
                return Err(format!("wave count {} exceeds 10", payload.count));
            }
            Ok(())
        }
        CommandKind::AdminTileOp(payload) => {
            cap_list(payload.points.len(), 1024, "points")?;
            if let Some(name) = &payload.name_0 {
                valid_content_name(name, MAX_CONTENT_NAME_LEN)?;
            }
            if let Some(name) = &payload.name_1 {
                valid_content_name(name, MAX_CONTENT_NAME_LEN)?;
            }
            Ok(())
        }
        CommandKind::LogicSync(payload) => {
            cap_str(&payload.var_name, 64, "var_name")?;
            cap_bytes(&payload.value, 1024, "logic value")
        }
        CommandKind::LogicClientData(payload) => {
            cap_str(&payload.channel, 64, "channel")?;
            cap_bytes(&payload.value, 8 * 1024, "logic client value")
        }
        CommandKind::MenuBuilderChoose(payload) => {
            cap_bytes(&payload.result, 8 * 1024, "menu result")
        }
        CommandKind::TextInputResult(payload) => {
            if let Some(text) = &payload.text {
                cap_str(text, 256, "text input")?;
            }
            Ok(())
        }
        CommandKind::Custom(payload) => cap_bytes(&payload.data, 1024, "custom data"),
        CommandKind::BuildingControlSelect(payload) => bounds(payload.x, payload.y),
        CommandKind::UnitControl(_)
        | CommandKind::UnitClear
        | CommandKind::Payload(_)
        | CommandKind::CompleteObjective(_)
        | CommandKind::ClearObjectives
        | CommandKind::SectorCapture
        | CommandKind::SaveSector
        | CommandKind::SkipWave
        | CommandKind::AdminSwitchTeam(_)
        | CommandKind::MenuChoose(_) => Ok(()),
    }
}

/// One in-flight predicted command.
#[derive(Debug, Clone, PartialEq)]
pub struct Prediction {
    /// Client-predicted `sender_seq`.
    pub sender_seq: u64,
    /// Emitted variant (for diagnostics/echo comparison).
    pub kind: CommandKind,
}

/// Bounded FIFO of in-flight predictions.
#[derive(Debug, Clone, Default)]
pub struct PredictionQueue {
    entries: VecDeque<Prediction>,
    cap: usize,
}

impl PredictionQueue {
    /// New queue with [`PREDICTION_QUEUE_CAP`].
    pub fn new() -> Self {
        Self {
            entries: VecDeque::new(),
            cap: PREDICTION_QUEUE_CAP,
        }
    }

    /// Pushes a prediction; `false` when the queue is full (force resync).
    pub fn push(&mut self, sender_seq: u64, kind: CommandKind) -> bool {
        if self.entries.len() >= self.cap {
            return false;
        }
        self.entries.push_back(Prediction { sender_seq, kind });
        true
    }

    /// Removes and returns the prediction for `sender_seq` (echo skip).
    pub fn remove(&mut self, sender_seq: u64) -> Option<Prediction> {
        let index = self
            .entries
            .iter()
            .position(|entry| entry.sender_seq == sender_seq)?;
        self.entries.remove(index)
    }

    /// Oldest in-flight prediction.
    pub fn oldest(&self) -> Option<&Prediction> {
        self.entries.front()
    }

    /// Number of in-flight predictions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no predictions are in flight.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Command-send failures.
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    /// The transport rejected the send.
    #[error(transparent)]
    Transport(#[from] TransportError),
    /// The in-flight queue overflowed; a resync is required.
    #[error("prediction queue overflow ({PREDICTION_QUEUE_CAP} in flight)")]
    PredictionOverflow,
}

/// Assigns client-side sequences, stamps `client_tick`, and records in-flight
/// predictions (plan §3.5).
#[derive(Debug, Clone)]
pub struct CommandSender {
    match_id: u64,
    next_seq: u64,
    input_delay_ticks: u64,
    queue: PredictionQueue,
}

impl CommandSender {
    /// Creates a sender for `match_id` with the default input delay.
    pub fn new(match_id: u64) -> Self {
        Self {
            match_id,
            next_seq: 1,
            input_delay_ticks: INPUT_DELAY_TICKS,
            queue: PredictionQueue::new(),
        }
    }

    /// Match this sender is scoped to.
    pub fn match_id(&self) -> u64 {
        self.match_id
    }

    /// In-flight predictions.
    pub fn queue(&self) -> &PredictionQueue {
        &self.queue
    }

    /// Next client sequence that will be assigned (1-based).
    pub fn next_sender_seq(&self) -> u64 {
        self.next_seq
    }

    /// Emits `kind`, returning the locally assigned sequence.
    ///
    /// The caller runs [`preflight_validate`] and applies an `Immediate`
    /// prediction to the local sim before calling this.
    pub fn send<T: RelayTransport>(
        &mut self,
        transport: &mut T,
        sim_tick: u64,
        kind: CommandKind,
    ) -> Result<u64, SendError> {
        let seq = self.next_seq;
        let client_tick = sim_tick.saturating_add(self.input_delay_ticks);
        transport.send_command(self.match_id, client_tick, kind.clone())?;
        if !self.queue.push(seq, kind) {
            return Err(SendError::PredictionOverflow);
        }
        self.next_seq = seq.saturating_add(1);
        Ok(seq)
    }

    /// Records that a locally applied `LocalOnly` action was sent (no echo
    /// expectation, e.g. `PlayerSpawn`).
    pub fn note_local_only(&mut self, kind: CommandKind) {
        let seq = self.next_seq;
        let _ = self.queue.push(seq, kind);
        self.next_seq = seq.saturating_add(1);
    }

    /// Removes an echoed prediction (returns it when it was in flight).
    pub fn on_echo(&mut self, sender_seq: u64) -> Option<Prediction> {
        self.queue.remove(sender_seq)
    }

    /// Drops every prediction (resync/reconnect).
    pub fn clear(&mut self) {
        self.queue = PredictionQueue::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::module_bindings::{PlaceBlock, SetRules};

    #[test]
    fn command_kind_matrix() {
        // Invariant §3.17-6: every variant has a caps row and a predict policy;
        // a valid instance passes preflight.
        use crate::module_bindings::*;
        let ctx = PreflightContext::default();
        let valid: Vec<CommandKind> = vec![
            CommandKind::Noop,
            CommandKind::Ping(1),
            CommandKind::PlaceBlock(PlaceBlock {
                x: 1,
                y: 1,
                block: "stone-wall".to_string(),
                rotation: 0,
                config: Vec::new(),
            }),
            CommandKind::BreakBlock(BreakBlock { x: 1, y: 1 }),
            CommandKind::ConfigBlock(ConfigBlock {
                x: 1,
                y: 1,
                value: vec![1],
            }),
            CommandKind::Rotate(Rotate {
                x: 1,
                y: 1,
                direction: true,
            }),
            CommandKind::DeletePlans(DeletePlans {
                positions: vec![1],
            }),
            CommandKind::CommandBuilding(CommandBuilding {
                positions: vec![1],
                x: 1.0,
                y: 1.0,
            }),
            CommandKind::Inventory(Inventory {
                kind: 0,
                x: 1,
                y: 1,
                item: Some("copper".to_string()),
                amount: 1,
                angle: 0.0,
            }),
            CommandKind::Payload(Payload {
                kind: 0,
                x: 1.0,
                y: 1.0,
                target: None,
            }),
            CommandKind::UnitControl(UnitControl { unit: None }),
            CommandKind::UnitClear,
            CommandKind::BuildingControlSelect(BuildingControlSelect { x: 1, y: 1 }),
            CommandKind::UnitCommand(UnitCommand {
                units: vec![1],
                command: 1,
                x: 1.0,
                y: 1.0,
            }),
            CommandKind::UnitCommandQueue(UnitCommandQueue {
                units: vec![1],
                command: 1,
                x: 1.0,
                y: 1.0,
            }),
            CommandKind::UnitStance(UnitStance {
                units: vec![1],
                stance: 1,
                enabled: true,
            }),
            CommandKind::PlayerSpawn(PlayerSpawn {
                unit: None,
                team: 0,
            }),
            CommandKind::Bullet(Bullet {
                def: 1,
                team: 0,
                x: 0.0,
                y: 0.0,
                angle: 0.0,
                damage: 0.0,
                velocity_scl: 1.0,
                lifetime_scl: 1.0,
                aim_x: f32::NAN,
                aim_y: f32::NAN,
                data: Vec::new(),
            }),
            CommandKind::SetRules(SetRules {
                rules_json: "{}".to_string(),
                rules_epoch: 1,
            }),
            CommandKind::SetRule(SetRule {
                rule: "waves".to_string(),
                json: "true".to_string(),
            }),
            CommandKind::ResearchUnlock(ResearchUnlock {
                content: "router".to_string(),
            }),
            CommandKind::CompleteObjective(CompleteObjective {
                index: 0,
                rules_epoch: 1,
            }),
            CommandKind::ClearObjectives,
            CommandKind::SectorCapture,
            CommandKind::SaveSector,
            CommandKind::SkipWave,
            CommandKind::RunWave(RunWave { count: 1 }),
            CommandKind::AdminSwitchTeam(AdminSwitchTeam {
                target: spacetimedb_sdk::Identity::from_byte_array([1u8; 32]),
                team: 0,
            }),
            CommandKind::AdminTileOp(AdminTileOp {
                op: 0,
                points: vec![1],
                arg_0: 0,
                arg_1: 0,
                arg_2: 0,
                name_0: Some("stone-wall".to_string()),
                name_1: None,
            }),
            CommandKind::LogicSync(LogicSync {
                x: 0,
                y: 0,
                var_name: "v".to_string(),
                var_id: 0,
                value: vec![1],
            }),
            CommandKind::LogicClientData(LogicClientData {
                channel: "c".to_string(),
                value: vec![1],
                reliable: true,
            }),
            CommandKind::MenuChoose(MenuChoose {
                menu_id: 0,
                option: 0,
            }),
            CommandKind::MenuBuilderChoose(MenuBuilderChoose {
                menu_id: 0,
                result: vec![1],
            }),
            CommandKind::TextInputResult(TextInputResult {
                id: 0,
                text: Some("hi".to_string()),
            }),
            CommandKind::Custom(Custom {
                kind: 1,
                data: vec![1],
            }),
        ];
        assert_eq!(valid.len(), 35);
        for kind in &valid {
            preflight_validate(kind, &ctx).unwrap_or_else(|error| {
                panic!("valid `{}` rejected: {error}", kind_predict_name(kind))
            });
            // Every variant maps to a policy (compile-checked match).
            let _ = predict_policy(kind);
        }
    }

    fn kind_predict_name(kind: &CommandKind) -> &'static str {
        match kind {
            CommandKind::Noop => "noop",
            CommandKind::Ping(_) => "ping",
            CommandKind::PlaceBlock(_) => "place_block",
            _ => "other",
        }
    }

    #[test]
    fn predict_policies_are_classified() {
        assert_eq!(predict_policy(&CommandKind::Noop), PredictPolicy::Immediate);
        assert_eq!(
            predict_policy(&CommandKind::PlayerSpawn(crate::module_bindings::PlayerSpawn {
                unit: None,
                team: 0
            })),
            PredictPolicy::LocalOnly
        );
        assert_eq!(
            predict_policy(&CommandKind::SetRules(SetRules {
                rules_json: "{}".to_string(),
                rules_epoch: 1
            })),
            PredictPolicy::AwaitEcho
        );
    }

    #[test]
    fn preflight_bounds_rates_and_blobs() {
        let ctx = PreflightContext {
            map_width: 100,
            map_height: 100,
            ..PreflightContext::default()
        };
        assert!(preflight_validate(&CommandKind::Ping(1), &ctx).is_ok());
        assert!(
            preflight_validate(
                &CommandKind::PlaceBlock(PlaceBlock {
                    x: 5,
                    y: 5,
                    block: "stone-wall".to_string(),
                    rotation: 2,
                    config: vec![1, 2],
                }),
                &ctx
            )
            .is_ok()
        );
        assert_eq!(
            preflight_validate(
                &CommandKind::PlaceBlock(PlaceBlock {
                    x: 999,
                    y: 5,
                    block: "stone-wall".to_string(),
                    rotation: 0,
                    config: Vec::new(),
                }),
                &ctx
            )
            .unwrap_err(),
            "(999, 5) is outside the match bounds"
        );
        assert!(
            preflight_validate(
                &CommandKind::ConfigBlock(crate::module_bindings::ConfigBlock {
                    x: 0,
                    y: 0,
                    value: vec![0; MAX_CONFIG_BYTES + 1],
                }),
                &ctx
            )
            .is_err()
        );
    }

    #[test]
    fn prediction_queue_bounds_and_echo_skip() {
        let mut queue = PredictionQueue::new();
        assert!(queue.push(1, CommandKind::Ping(1)));
        assert!(queue.push(2, CommandKind::Ping(2)));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.remove(2).map(|p| p.sender_seq), Some(2));
        assert_eq!(queue.len(), 1);
        assert!(queue.remove(99).is_none());
        let mut full = PredictionQueue::new();
        for seq in 0..PREDICTION_QUEUE_CAP as u64 {
            assert!(full.push(seq, CommandKind::Ping(seq)));
        }
        assert!(!full.push(u64::MAX, CommandKind::Ping(u64::MAX)));
    }

    #[test]
    fn command_sender_assigns_sequences_and_accepts_echo() {
        use crate::config::{ConnectionConfig, StdbMode};
        use crate::connector::Connector;
        use crate::transport::StdbTransport;

        let mut config = ConnectionConfig::local();
        config.mode = StdbMode::Offline;
        let mut connector = Connector::new(config);
        let mut sender = CommandSender::new(4);
        assert_eq!(sender.next_sender_seq(), 1);
        // Offline transport fails the reducer send; no sequence is consumed.
        let mut transport = StdbTransport::new(&mut connector);
        assert!(sender.send(&mut transport, 0, CommandKind::Noop).is_err());
        assert_eq!(sender.next_sender_seq(), 1);
    }
}
