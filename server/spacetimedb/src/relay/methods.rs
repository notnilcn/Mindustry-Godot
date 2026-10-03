// SPDX-License-Identifier: GPL-3.0-only

//! Pure-ish relay helpers (plan 01 §3.9; plan 21 §3.5/§6.3): membership check,
//! rate gate and the cheap command validator. `validate_kind` and
//! [`validate_join`] are deliberately pure functions so a future authoritative
//! sim can re-use them (D2, §3.10) and so their behavior is unit-testable
//! without a database.

use spacetimedb::{ReducerContext, Table};

use crate::admin::admin_identity as _;

use super::tables::{
    AdminSwitchTeam, AdminTileOp, BreakBlock, Bullet, CommandBuilding, CommandKind, CommandRate,
    CompleteObjective, ConfigBlock, DeletePlans, Inventory, LogicClientData, LogicSync, MatchStatus,
    MenuBuilderChoose, MenuChoose, Payload, PlaceBlock, PlayerSpawn, RelayMatch, RelayMember,
    ResearchUnlock, Rotate, RunWave, SetRule, SetRules, TextInputResult, UnitCommand,
    UnitCommandQueue, UnitControl, UnitStance, command_rate, relay_match, relay_member,
};
use crate::main::global::{
    DEFAULT_COMMANDS_PER_SECOND, DEFAULT_COMMAND_RATE_WINDOW_MS, DEFAULT_MAP_HEIGHT_TILES,
    DEFAULT_MAP_WIDTH_TILES, MAX_CONFIG_BYTES, MAX_PLACE_CONFIG_BYTES, MAX_POSITIONS,
    MAX_RULES_JSON, MAX_UNITS,
};
use crate::main::tables::{RelayConfig, relay_config};

/// Map-id cap (plan §6.3 keeps IDs short; plan 21 may relax).
pub const MAX_MAP_ID_LEN: usize = 64;

/// Block content-name cap (plan §6.3).
pub const MAX_BLOCK_NAME_LEN: usize = 100;

/// Generic content-name cap (plan §6.3).
pub const MAX_CONTENT_NAME_LEN: usize = 100;

/// Salt mixed into the password hash so equal passwords differ from bare FNV.
const PASSWORD_SALT: &[u8] = b"mindustry-godot/password/v1";

/// Whether a rate window starting at `window_start_micros` has expired.
pub fn window_expired(now_micros: i64, window_start_micros: i64, window_ms: u32) -> bool {
    now_micros.saturating_sub(window_start_micros) >= i64::from(window_ms) * 1_000
}

/// The next per-sender sequence; `None` on `u64` overflow.
pub fn next_sender_seq(last: u64) -> Option<u64> {
    last.checked_add(1)
}

/// Salted FNV-1a-64 of a password. Never stores the plaintext.
pub fn hash_password(password: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in PASSWORD_SALT.iter().chain(password.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Requires the caller to be a member of `match_id`.
pub fn require_member(ctx: &ReducerContext, match_id: u64) -> Result<RelayMember, String> {
    ctx.db
        .relay_member()
        .by_match_identity()
        .filter((match_id, ctx.sender()))
        .next()
        .ok_or_else(|| "caller is not a member of this match".to_string())
}

/// Loads the singleton relay config (defaults when `init` has not seeded).
pub fn relay_config_or_default(ctx: &ReducerContext) -> RelayConfig {
    ctx.db.relay_config().id().find(0u8).unwrap_or(RelayConfig {
        id: 0,
        commands_per_second: DEFAULT_COMMANDS_PER_SECOND,
        command_rate_window_ms: DEFAULT_COMMAND_RATE_WINDOW_MS,
        max_commit_commands_per_transaction: 256,
        default_map_width_tiles: DEFAULT_MAP_WIDTH_TILES,
        default_map_height_tiles: DEFAULT_MAP_HEIGHT_TILES,
    })
}

/// Inputs to the join gate (plan §3.3/§6.3), kept pure for tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JoinGate {
    /// Target has a non-expired ban row.
    pub banned: bool,
    /// `server_config.whitelist_enabled`.
    pub whitelist_enabled: bool,
    /// Target has a whitelist entry.
    pub whitelisted: bool,
    /// Current `relay_match.player_count`.
    pub player_count: u16,
    /// `relay_match.max_players`.
    pub max_players: u16,
    /// `relay_match.password_hash`, if any.
    pub stored_password_hash: Option<u64>,
    /// Password supplied by the joiner, if any.
    pub supplied_password: Option<String>,
}

/// Cheap join validation (ban → whitelist → cap → password), stable reasons.
pub fn validate_join(gate: &JoinGate) -> Result<(), String> {
    if gate.banned {
        return Err("You are banned from this server.".to_string());
    }
    if gate.whitelist_enabled && !gate.whitelisted {
        return Err("This server is whitelisted.".to_string());
    }
    if gate.whitelisted {
        // Whitelisted players bypass the cap (upstream `Administration`).
    } else if gate.player_count >= gate.max_players {
        return Err("This server is full.".to_string());
    }
    if let Some(stored) = gate.stored_password_hash {
        let supplied = gate
            .supplied_password
            .as_deref()
            .map(hash_password)
            .unwrap_or(0);
        if supplied != stored {
            return Err("Incorrect password.".to_string());
        }
    }
    Ok(())
}

/// Whether `name` is a plausible content identifier (non-empty, charset, cap).
pub fn valid_content_name(name: &str, max_len: usize) -> Result<(), String> {
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

fn in_bounds(x: i32, y: i32, width: i32, height: i32) -> bool {
    x >= 0 && y >= 0 && x < width && y < height
}

/// Cheap coarse validation for the full plan-21 command variant set (§6.3).
///
/// No tile occupancy, resources, unit ownership, rule legality, collision or
/// per-building range — those are sim-side (documented in §6.3).
pub fn validate_kind(
    kind: &CommandKind,
    map_width_tiles: i32,
    map_height_tiles: i32,
) -> Result<(), String> {
    let bounds = |x: i32, y: i32| -> Result<(), String> {
        if in_bounds(x, y, map_width_tiles, map_height_tiles) {
            Ok(())
        } else {
            Err(format!("({x}, {y}) is outside the match bounds"))
        }
    };
    match kind {
        CommandKind::Noop | CommandKind::Ping(_) => Ok(()),
        CommandKind::PlaceBlock(PlaceBlock {
            x,
            y,
            block,
            rotation,
            config,
        }) => {
            bounds(*x, *y)?;
            valid_content_name(block, MAX_BLOCK_NAME_LEN)?;
            if *rotation > 3 {
                return Err(format!("rotation {rotation} must be 0..=3"));
            }
            cap_bytes(config, MAX_PLACE_CONFIG_BYTES, "place config")
        }
        CommandKind::BreakBlock(BreakBlock { x, y }) => bounds(*x, *y),
        CommandKind::ConfigBlock(ConfigBlock { x, y, value }) => {
            bounds(*x, *y)?;
            cap_bytes(value, MAX_CONFIG_BYTES, "config value")
        }
        CommandKind::Rotate(Rotate { x, y, .. }) => bounds(*x, *y),
        CommandKind::DeletePlans(DeletePlans { positions }) => {
            cap_list(positions.len(), MAX_POSITIONS, "positions")
        }
        CommandKind::CommandBuilding(CommandBuilding { positions, .. }) => {
            cap_list(positions.len(), MAX_POSITIONS, "positions")
        }
        CommandKind::Inventory(Inventory {
            x, y, item, amount, ..
        }) => {
            bounds(*x, *y)?;
            if let Some(item) = item {
                valid_content_name(item, MAX_CONTENT_NAME_LEN)?;
            }
            if amount.abs() > 10_000 {
                return Err(format!("inventory amount {amount} exceeds 10000"));
            }
            Ok(())
        }
        CommandKind::Payload(Payload { .. }) => Ok(()),
        CommandKind::UnitControl(UnitControl { .. }) => Ok(()),
        CommandKind::UnitClear => Ok(()),
        CommandKind::BuildingControlSelect(super::tables::BuildingControlSelect { x, y }) => {
            bounds(*x, *y)
        }
        CommandKind::UnitCommand(UnitCommand { units, command, .. }) => {
            cap_list(units.len(), MAX_UNITS, "units")?;
            if *command >= 256 {
                return Err(format!("unit command {command} must be < 256"));
            }
            Ok(())
        }
        CommandKind::UnitCommandQueue(UnitCommandQueue { units, command, .. }) => {
            cap_list(units.len(), MAX_UNITS, "units")?;
            if *command >= 256 {
                return Err(format!("unit command {command} must be < 256"));
            }
            Ok(())
        }
        CommandKind::UnitStance(UnitStance { units, .. }) => {
            cap_list(units.len(), MAX_UNITS, "units")
        }
        CommandKind::PlayerSpawn(PlayerSpawn { unit, .. }) => {
            if let Some(unit) = unit {
                valid_content_name(unit, MAX_CONTENT_NAME_LEN)?;
            }
            Ok(())
        }
        CommandKind::Bullet(Bullet { data, .. }) => cap_bytes(data, 4 * 1024, "bullet data"),
        CommandKind::SetRules(SetRules { rules_json, .. }) => {
            cap_str(rules_json, MAX_RULES_JSON, "rules_json")
        }
        CommandKind::SetRule(SetRule { rule, json }) => {
            cap_str(rule, 64, "rule")?;
            cap_str(json, 16 * 1024, "rule json")
        }
        CommandKind::ResearchUnlock(ResearchUnlock { content }) => {
            valid_content_name(content, MAX_CONTENT_NAME_LEN)
        }
        CommandKind::CompleteObjective(CompleteObjective { .. }) => Ok(()),
        CommandKind::ClearObjectives
        | CommandKind::SectorCapture
        | CommandKind::SaveSector
        | CommandKind::SkipWave => Ok(()),
        CommandKind::RunWave(RunWave { count }) => {
            if *count > 10 {
                return Err(format!("wave count {count} exceeds 10"));
            }
            Ok(())
        }
        CommandKind::AdminSwitchTeam(AdminSwitchTeam { .. }) => Ok(()),
        CommandKind::AdminTileOp(AdminTileOp {
            points, name0, name1, ..
        }) => {
            cap_list(points.len(), 1024, "points")?;
            if let Some(name) = name0 {
                valid_content_name(name, MAX_CONTENT_NAME_LEN)?;
            }
            if let Some(name) = name1 {
                valid_content_name(name, MAX_CONTENT_NAME_LEN)?;
            }
            Ok(())
        }
        CommandKind::LogicSync(LogicSync { var_name, value, .. }) => {
            cap_str(var_name, 64, "var_name")?;
            cap_bytes(value, 1024, "logic value")
        }
        CommandKind::LogicClientData(LogicClientData {
            channel, value, ..
        }) => {
            cap_str(channel, 64, "channel")?;
            cap_bytes(value, 8 * 1024, "logic client value")
        }
        CommandKind::MenuChoose(MenuChoose { .. }) => Ok(()),
        CommandKind::MenuBuilderChoose(MenuBuilderChoose { result, .. }) => {
            cap_bytes(result, 8 * 1024, "menu result")
        }
        CommandKind::TextInputResult(TextInputResult { text, .. }) => {
            if let Some(text) = text {
                cap_str(text, 256, "text input")?;
            }
            Ok(())
        }
        CommandKind::Custom(super::tables::Custom { data, .. }) => {
            cap_bytes(data, 1024, "custom data")
        }
    }
}

/// Role of the caller for a command variant (plan §6.3 “ROLE”).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CommandRole {
    /// Any member (spectators forbidden for sim-affecting variants).
    Member,
    /// Host or admin.
    HostOrAdmin,
    /// Admin only.
    Admin,
    /// Host (server) only.
    Host,
}

/// Classifies the role a variant requires (§6.3).
pub fn command_role(kind: &CommandKind) -> CommandRole {
    match kind {
        CommandKind::PlaceBlock(_)
        | CommandKind::BreakBlock(_)
        | CommandKind::ConfigBlock(_)
        | CommandKind::Rotate(_)
        | CommandKind::DeletePlans(_)
        | CommandKind::CommandBuilding(_)
        | CommandKind::Inventory(_)
        | CommandKind::Payload(_)
        | CommandKind::UnitControl(_)
        | CommandKind::UnitClear
        | CommandKind::BuildingControlSelect(_)
        | CommandKind::UnitCommand(_)
        | CommandKind::UnitCommandQueue(_)
        | CommandKind::UnitStance(_)
        | CommandKind::PlayerSpawn(_)
        | CommandKind::MenuChoose(_)
        | CommandKind::MenuBuilderChoose(_)
        | CommandKind::TextInputResult(_)
        | CommandKind::Custom(_)
        | CommandKind::Noop
        | CommandKind::Ping(_) => CommandRole::Member,
        CommandKind::LogicClientData(_) => CommandRole::Member,
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
        | CommandKind::AdminTileOp(_) => CommandRole::HostOrAdmin,
        CommandKind::Bullet(_) | CommandKind::LogicSync(_) => CommandRole::Host,
    }
}

/// Whether a variant is forbidden for spectators (sim-affecting).
pub fn spectator_forbidden(kind: &CommandKind) -> bool {
    !matches!(
        kind,
        CommandKind::Noop
            | CommandKind::Ping(_)
            | CommandKind::MenuChoose(_)
            | CommandKind::MenuBuilderChoose(_)
            | CommandKind::TextInputResult(_)
            | CommandKind::Custom(_)
    )
}

/// Rate gate + per-sender sequence assignment (plan §3.9 items 2/6).
///
/// Returns the assigned `sender_seq`. Called exactly once per accepted command
/// and rolled back with the reducer on later validation failure.
pub fn rate_allow(ctx: &ReducerContext, config: &RelayConfig) -> Result<u64, String> {
    let identity = ctx.sender();
    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();

    let existing = ctx.db.command_rate().identity().find(identity);
    let had_row = existing.is_some();
    let mut rate = match existing {
        Some(rate) => rate,
        None => CommandRate {
            identity,
            window_start: now,
            count: 0,
            last_sender_seq: 0,
            count_input: 0,
            count_plan: 0,
            count_ui: 0,
            count_logic: 0,
            count_admin: 0,
        },
    };
    if window_expired(
        now_micros,
        rate.window_start.to_micros_since_unix_epoch(),
        config.command_rate_window_ms,
    ) {
        rate.window_start = now;
        rate.count = 0;
        rate.count_input = 0;
        rate.count_plan = 0;
        rate.count_ui = 0;
        rate.count_logic = 0;
        rate.count_admin = 0;
    }
    if rate.count >= config.commands_per_second {
        return Err(format!(
            "rate limit exceeded ({} commands per {} ms)",
            config.commands_per_second, config.command_rate_window_ms
        ));
    }
    let next_seq = next_sender_seq(rate.last_sender_seq)
        .ok_or_else(|| "sender sequence overflow".to_string())?;
    rate.count = rate.count.saturating_add(1);
    rate.last_sender_seq = next_seq;
    if had_row {
        ctx.db.command_rate().identity().update(rate);
    } else {
        ctx.db.command_rate().insert(rate);
    }
    Ok(next_seq)
}

/// Match existence + status gate shared by command/lifecycle reducers.
pub fn require_match(ctx: &ReducerContext, match_id: u64) -> Result<RelayMatch, String> {
    ctx.db
        .relay_match()
        .match_id()
        .find(match_id)
        .ok_or_else(|| format!("unknown match {match_id}"))
}

/// Requires a match in `status`.
pub fn require_status(match_row: &RelayMatch, status: MatchStatus) -> Result<(), String> {
    if match_row.status != status {
        return Err(format!(
            "match {} is {:?}, expected {status:?}",
            match_row.match_id, match_row.status
        ));
    }
    Ok(())
}

/// Whether the caller is a global admin (plan §3.10).
pub fn is_admin(ctx: &ReducerContext) -> bool {
    ctx.db
        .admin_identity()
        .identity()
        .find(ctx.sender())
        .is_some()
}

/// Whether the caller is the match host or a global admin.
pub fn is_host_or_admin(ctx: &ReducerContext, row: &RelayMatch) -> bool {
    row.host == ctx.sender() || is_admin(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay::tables::{
        BuildingControlSelect as Select, Custom as CustomPayload, Payload as PayloadPayload,
        SetRule as SetRulePayload, SetRules as SetRulesPayload,
    };

    fn place(x: i32, y: i32, block: &str, rotation: u8) -> CommandKind {
        CommandKind::PlaceBlock(PlaceBlock {
            x,
            y,
            block: block.to_string(),
            rotation,
            config: Vec::new(),
        })
    }

    #[test]
    fn validate_kind_rejects_out_of_bounds_and_bad_names() {
        assert!(validate_kind(&CommandKind::Ping(1), 100, 100).is_ok());
        assert!(validate_kind(&CommandKind::Noop, 100, 100).is_ok());
        assert!(validate_kind(&place(5, 6, "stone-wall", 3), 100, 100).is_ok());
        assert!(validate_kind(&place(100, 0, "stone-wall", 0), 100, 100).is_err());
        assert!(validate_kind(&place(0, 100, "stone-wall", 0), 100, 100).is_err());
        assert!(validate_kind(&place(1, 1, "", 0), 100, 100).is_err());
        assert!(validate_kind(&place(1, 1, "bad name", 0), 100, 100).is_err());
        assert!(validate_kind(&place(1, 1, "stone-wall", 4), 100, 100).is_err());
        assert!(
            validate_kind(&CommandKind::BreakBlock(BreakBlock { x: -1, y: 0 }), 100, 100).is_err()
        );
        assert!(
            validate_kind(
                &CommandKind::ConfigBlock(ConfigBlock {
                    x: 0,
                    y: 99,
                    value: vec![1, 2, 3]
                }),
                100,
                100
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_kind_caps_lists_and_blobs() {
        let huge = CommandKind::DeletePlans(DeletePlans {
            positions: vec![0; MAX_POSITIONS + 1],
        });
        assert!(validate_kind(&huge, 100, 100).is_err());
        let blob = CommandKind::ConfigBlock(ConfigBlock {
            x: 0,
            y: 0,
            value: vec![0; MAX_CONFIG_BYTES + 1],
        });
        assert!(validate_kind(&blob, 100, 100).is_err());
        assert!(
            validate_kind(
                &CommandKind::SetRules(SetRulesPayload {
                    rules_json: "{}".to_string(),
                    rules_epoch: 1,
                }),
                100,
                100
            )
            .is_ok()
        );
        assert!(
            validate_kind(
                &CommandKind::SetRule(SetRulePayload {
                    rule: "waves".to_string(),
                    json: "true".to_string(),
                }),
                100,
                100
            )
            .is_ok()
        );
        assert!(validate_kind(&CommandKind::RunWave(RunWave { count: 11 }), 100, 100).is_err());
    }

    #[test]
    fn role_and_spectator_classification() {
        assert_eq!(command_role(&CommandKind::PlaceBlock(PlaceBlock {
            x: 0,
            y: 0,
            block: "router".to_string(),
            rotation: 0,
            config: Vec::new(),
        })), CommandRole::Member);
        assert!(!spectator_forbidden(&CommandKind::MenuChoose(MenuChoose {
            menu_id: 1,
            option: 0
        })));
        assert!(spectator_forbidden(&CommandKind::UnitClear));
        assert_eq!(
            command_role(&CommandKind::SetRules(SetRulesPayload {
                rules_json: "{}".to_string(),
                rules_epoch: 1
            })),
            CommandRole::HostOrAdmin
        );
    }

    #[test]
    fn config_custom_and_payload_are_constructible() {
        // Smoke: every payload struct is reachable through the variant set.
        let _ = CommandKind::Payload(PayloadPayload {
            kind: 0,
            x: 1.0,
            y: 2.0,
            target: None,
        });
        let _ = CommandKind::Custom(CustomPayload {
            kind: 1,
            data: vec![1],
        });
        let _ = CommandKind::BuildingControlSelect(Select { x: 0, y: 0 });
        let _ = CommandKind::TextInputResult(TextInputResult {
            id: 1,
            text: Some("hi".to_string()),
        });
    }

    #[test]
    fn rate_window_rolls_after_window() {
        assert!(!window_expired(1_000_000, 0, 1_000));
        assert!(!window_expired(999_999, 0, 1_000));
        assert!(window_expired(1_000_000, 0, 1_000));
        assert!(window_expired(2_000_000, 1_000_000, 1_000));
        // Clock regression must not roll the window.
        assert!(!window_expired(0, 1_000_000, 1_000));
    }

    #[test]
    fn sender_sequence_must_increase() {
        assert_eq!(next_sender_seq(0), Some(1));
        assert_eq!(next_sender_seq(41), Some(42));
        assert_eq!(next_sender_seq(u64::MAX), None);
    }

    #[test]
    fn password_hash_is_salted_and_stable() {
        assert_eq!(hash_password("hunter2"), hash_password("hunter2"));
        assert_ne!(hash_password("hunter2"), hash_password("hunter3"));
        // Salted: bare FNV-1a of "a" is not the stored hash.
        assert_ne!(hash_password("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn join_gate_reasons_are_stable() {
        let full = JoinGate {
            player_count: 4,
            max_players: 4,
            ..JoinGate::default()
        };
        assert_eq!(validate_join(&full).unwrap_err(), "This server is full.");
        let banned = JoinGate {
            banned: true,
            ..JoinGate::default()
        };
        assert_eq!(
            validate_join(&banned).unwrap_err(),
            "You are banned from this server."
        );
        let whitelisted = JoinGate {
            whitelist_enabled: true,
            ..JoinGate::default()
        };
        assert_eq!(
            validate_join(&whitelisted).unwrap_err(),
            "This server is whitelisted."
        );
        let password = JoinGate {
            stored_password_hash: Some(hash_password("pw")),
            supplied_password: Some("nope".to_string()),
            ..JoinGate::default()
        };
        assert_eq!(
            validate_join(&password).unwrap_err(),
            "Incorrect password."
        );
        let ok = JoinGate {
            stored_password_hash: Some(hash_password("pw")),
            supplied_password: Some("pw".to_string()),
            ..JoinGate::default()
        };
        assert!(validate_join(&ok).is_ok());
    }

    #[test]
    fn whitelisted_players_bypass_cap() {
        let gate = JoinGate {
            whitelist_enabled: true,
            whitelisted: true,
            player_count: 100,
            max_players: 4,
            ..JoinGate::default()
        };
        assert!(validate_join(&gate).is_ok());
    }
}
