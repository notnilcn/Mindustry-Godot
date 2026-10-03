// SPDX-License-Identifier: GPL-3.0-only

//! Relay reducers (plan 01 §3.8/§3.9; plan 21 §3.3–§3.6). Every reducer is
//! deterministic, returns no data (`Result<(), String>` at most), uses
//! `ctx.sender()` as the only principal and performs cheap validation only (D2).

use spacetimedb::{ReducerContext, Table, reducer};

use super::methods::{
    CommandRole, JoinGate, MAX_MAP_ID_LEN, command_role, hash_password, is_host_or_admin,
    rate_allow, relay_config_or_default, require_match, require_member, require_status,
    spectator_forbidden, validate_join, validate_kind,
};
use super::tables::{
    AuthorityMode, CommandKind, Gamemode, MatchCommand, MatchState, MatchStatus, MemberRole,
    RelayMatch, RelayMember, SenderCommandState, SetRule, SetRules, Visibility, match_command,
    match_state, relay_match, relay_member, sender_command_state,
};
use crate::admin::{player_ban, server_config_or_default, whitelist_entry};
use crate::main::audit::audit;
use crate::main::global::{DEFAULT_MAX_PLAYERS, MAX_RULES_JSON, PROTOCOL_VERSION};
use crate::main::tables::AuditKind;
use crate::mods::{MatchMod, check_mods, match_mod};

/// Creates a lobby match and auto-joins the creator as host + player.
#[reducer]
#[allow(clippy::too_many_arguments)]
pub fn create_match(
    ctx: &ReducerContext,
    map_id: String,
    map_seed: u64,
    mode: Gamemode,
    mode_name: String,
    visibility: Visibility,
    password: Option<String>,
    max_players: u16,
    rules_json: String,
    build_id: String,
    content_hash: u64,
    mods: Vec<String>,
    campaign_id: Option<u64>,
    sector_planet: Option<String>,
    sector_id: Option<u32>,
) -> Result<(), String> {
    let map_id = map_id.trim().to_string();
    if map_id.is_empty() || map_id.chars().count() > MAX_MAP_ID_LEN {
        return Err(format!("map_id must be 1-{MAX_MAP_ID_LEN} characters"));
    }
    if mode_name.chars().count() > 64 {
        return Err("mode_name exceeds 64 characters".to_string());
    }
    if rules_json.chars().count() > MAX_RULES_JSON {
        return Err(format!("rules_json exceeds {MAX_RULES_JSON} characters"));
    }
    let max_players = if max_players == 0 {
        DEFAULT_MAX_PLAYERS
    } else {
        max_players
    };
    if max_players > 64 {
        return Err("max_players exceeds 64".to_string());
    }
    let config = relay_config_or_default(ctx);
    let now = ctx.timestamp;
    let password_hash = password
        .as_deref()
        .filter(|pw| !pw.is_empty())
        .map(hash_password);
    let row = ctx.db.relay_match().insert(RelayMatch {
        match_id: 0,
        map_id,
        map_seed,
        map_hash: 0,
        map_width_tiles: config.default_map_width_tiles,
        map_height_tiles: config.default_map_height_tiles,
        status: MatchStatus::Lobby,
        authority: AuthorityMode::Relay,
        protocol_version: PROTOCOL_VERSION,
        created_by: ctx.sender(),
        created_at: now,
        started_at: None,
        ended_at: None,
        host: ctx.sender(),
        mode,
        mode_name,
        visibility,
        password_hash,
        rules_json,
        rules_epoch: 1,
        build_id,
        content_hash,
        is_dedicated: false,
        player_count: 1,
        max_players,
        campaign_id,
        sector_planet,
        sector_id,
        last_command_id: 0,
        last_snapshot_id: None,
        closed_at: None,
    });
    ctx.db.relay_member().insert(RelayMember {
        member_id: 0,
        match_id: row.match_id,
        identity: ctx.sender(),
        joined_at: now,
        role: MemberRole::Player,
        team: None,
        connected: true,
        ready: true,
        last_seen_at: now,
        kicked_reason: None,
    });
    for name in mods {
        if name.is_empty() || name.chars().count() > 100 {
            return Err(format!("bad mod name `{name}`"));
        }
        ctx.db.match_mod().insert(MatchMod {
            match_mod_id: 0,
            match_id: row.match_id,
            name,
            version: String::new(),
            content_hash: 0,
        });
    }
    audit(
        ctx,
        Some(ctx.sender()),
        AuditKind::MatchCreate,
        format!("match {} `{}`", row.match_id, row.map_id),
    );
    Ok(())
}

/// Joins an existing non-ended match (idempotency is an error, not a no-op).
#[reducer]
pub fn join_match(
    ctx: &ReducerContext,
    match_id: u64,
    password: Option<String>,
    build_id: String,
    content_hash: u64,
    role: MemberRole,
    mods: Vec<String>,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    if require_member(ctx, match_id).is_ok() {
        return Err("caller is already a member of this match".to_string());
    }
    if row.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "match protocol {} != module protocol {PROTOCOL_VERSION}",
            row.protocol_version
        ));
    }
    // Build/content compatibility (skipped when the host left them unset).
    if !row.build_id.is_empty() && !build_id.is_empty() && row.build_id != build_id {
        return Err("client build does not match the server".to_string());
    }
    if row.content_hash != 0 && content_hash != 0 && row.content_hash != content_hash {
        return Err("content hash does not match the server".to_string());
    }
    // Mod set compatibility.
    let host_mods: Vec<String> = ctx
        .db
        .match_mod()
        .by_match_mod()
        .filter(match_id)
        .map(|entry| entry.name)
        .collect();
    check_mods(&host_mods, &mods)?;

    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();
    let banned = ctx
        .db
        .player_ban()
        .by_ban_identity()
        .filter(ctx.sender())
        .any(|ban| {
            ban.expires_at
                .map(|expiry| expiry.to_micros_since_unix_epoch() > now_micros)
                .unwrap_or(true)
        });
    let config = server_config_or_default(ctx);
    let whitelisted = ctx
        .db
        .whitelist_entry()
        .identity()
        .find(ctx.sender())
        .is_some();
    let gate = JoinGate {
        banned,
        whitelist_enabled: config.whitelist_enabled,
        whitelisted,
        player_count: row.player_count,
        max_players: row.max_players,
        stored_password_hash: row.password_hash,
        supplied_password: password,
    };
    validate_join(&gate)?;

    ctx.db.relay_member().insert(RelayMember {
        member_id: 0,
        match_id,
        identity: ctx.sender(),
        joined_at: now,
        role,
        team: None,
        connected: true,
        ready: false,
        last_seen_at: now,
        kicked_reason: None,
    });
    ctx.db.relay_match().match_id().update(RelayMatch {
        player_count: row.player_count.saturating_add(1),
        ..row
    });
    audit(
        ctx,
        Some(ctx.sender()),
        AuditKind::MatchJoin,
        format!("joined match {match_id}"),
    );
    Ok(())
}

/// Leaves a match. The host leaving ends the match for everyone (no migration).
#[reducer]
pub fn leave_match(ctx: &ReducerContext, match_id: u64) -> Result<(), String> {
    let member = require_member(ctx, match_id)?;
    let row = require_match(ctx, match_id)?;
    ctx.db.relay_member().member_id().delete(member.member_id);
    if row.host == ctx.sender() {
        // Host leaves: terminal. Drop all members and close the match.
        for other in ctx
            .db
            .relay_member()
            .by_match_identity()
            .filter(match_id)
            .collect::<Vec<_>>()
        {
            ctx.db.relay_member().member_id().delete(other.member_id);
        }
        ctx.db.relay_match().match_id().update(RelayMatch {
            status: MatchStatus::Ended,
            ended_at: Some(ctx.timestamp),
            closed_at: Some(ctx.timestamp),
            player_count: 0,
            ..row
        });
        audit(
            ctx,
            Some(ctx.sender()),
            AuditKind::MatchEnd,
            format!("host left match {match_id}; ended"),
        );
    } else {
        let count = ctx
            .db
            .relay_member()
            .by_match_identity()
            .filter(match_id)
            .count() as u16;
        ctx.db.relay_match().match_id().update(RelayMatch {
            player_count: count,
            ..row
        });
        audit(
            ctx,
            Some(ctx.sender()),
            AuditKind::MatchLeave,
            format!("left match {match_id}"),
        );
    }
    Ok(())
}

/// Marks the caller ready/unready in a lobby (plan §3.3).
#[reducer]
pub fn set_ready(ctx: &ReducerContext, match_id: u64, ready: bool) -> Result<(), String> {
    let member = require_member(ctx, match_id)?;
    ctx.db.relay_member().member_id().update(RelayMember {
        ready,
        connected: true,
        last_seen_at: ctx.timestamp,
        ..member
    });
    audit(
        ctx,
        Some(ctx.sender()),
        AuditKind::MatchReady,
        format!("match {match_id} ready={ready}"),
    );
    Ok(())
}

/// Starts a lobby match (host only). `force` bypasses the all-ready gate.
#[reducer]
pub fn start_match(ctx: &ReducerContext, match_id: u64, force: bool) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    require_status(&row, MatchStatus::Lobby)?;
    if !is_host_or_admin(ctx, &row) {
        return Err("only the host may start the match".to_string());
    }
    if !force {
        let not_ready = ctx
            .db
            .relay_member()
            .by_match_identity()
            .filter(match_id)
            .filter(|member| member.connected && !member.ready)
            .count();
        if not_ready > 0 {
            return Err(format!("{not_ready} member(s) are not ready"));
        }
    }
    ctx.db.relay_match().match_id().update(RelayMatch {
        status: MatchStatus::Running,
        started_at: Some(ctx.timestamp),
        ..row
    });
    audit(
        ctx,
        Some(ctx.sender()),
        AuditKind::MatchStart,
        format!("started match {match_id}"),
    );
    Ok(())
}

/// Host-published state snapshot equivalent (plan §3.6/§6.1).
#[reducer]
#[allow(clippy::too_many_arguments)]
pub fn publish_match_state(
    ctx: &ReducerContext,
    match_id: u64,
    wave: i32,
    wavetime: f32,
    enemies: i32,
    paused: bool,
    game_over: bool,
    sim_tick: u64,
    last_command_id: u64,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if !is_host_or_admin(ctx, &row) {
        return Err("only the host may publish match state".to_string());
    }
    let now = ctx.timestamp;
    let existing = ctx
        .db
        .match_state()
        .by_match_state()
        .filter(match_id)
        .next();
    let state = MatchState {
        state_id: existing.as_ref().map(|state| state.state_id).unwrap_or(0),
        match_id,
        rules_json: row.rules_json.clone(),
        rules_epoch: row.rules_epoch,
        wave,
        wavetime,
        enemies,
        paused,
        game_over,
        sim_tick,
        last_command_id,
        rng_sim: Vec::new(),
        next_entity_id: 0,
        snapshot_id: row.last_snapshot_id,
        updated_by: ctx.sender(),
        updated_at: now,
    };
    match existing {
        Some(_) => {
            ctx.db.match_state().state_id().update(state);
        }
        None => {
            ctx.db.match_state().insert(state);
        }
    }
    Ok(())
}

/// Appends one validated command to the private relay log (plan §3.6/§3.9).
///
/// `sender_seq` is assigned by the server from the per-identity
/// `CommandRate.last_sender_seq` (monotonic across window rolls), so clients
/// cannot replay old sequences; `command_id` is the global commit order.
#[reducer]
pub fn send_match_command(
    ctx: &ReducerContext,
    match_id: u64,
    client_tick: u64,
    kind: CommandKind,
) -> Result<(), String> {
    let result = send_match_command_impl(ctx, match_id, client_tick, kind);
    if let Err(error) = &result {
        // Failed reducers roll back, so rejections log instead of auditing (R7).
        log::warn!("send_match_command rejected for match {match_id}: {error}");
    }
    result
}

pub(crate) fn send_match_command_impl(
    ctx: &ReducerContext,
    match_id: u64,
    client_tick: u64,
    kind: CommandKind,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    // `PlayerSpawn` is allowed before the match starts (join/loading handoff).
    let spawn = matches!(kind, CommandKind::PlayerSpawn(_));
    if !spawn {
        require_status(&row, MatchStatus::Running)?;
    } else if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    let member = require_member(ctx, match_id)?;
    if row.protocol_version != PROTOCOL_VERSION {
        return Err(format!(
            "match protocol {} != module protocol {PROTOCOL_VERSION}",
            row.protocol_version
        ));
    }
    // Role gates (§6.3): spectators cannot emit sim-effecting variants; host or
    // admin required for the HA/H variants.
    if member.role == MemberRole::Spectator && spectator_forbidden(&kind) {
        return Err("spectators cannot send simulation commands".to_string());
    }
    match command_role(&kind) {
        CommandRole::Member => {}
        CommandRole::HostOrAdmin | CommandRole::Admin | CommandRole::Host => {
            if !is_host_or_admin(ctx, &row) {
                return Err("command requires host or admin privileges".to_string());
            }
        }
    }
    validate_kind(&kind, row.map_width_tiles, row.map_height_tiles)?;
    // Plan §3.12.1: campaign rows are written on the same ordered command.
    crate::campaign::persist_from_command(ctx, &row, &kind)?;
    let rules_update = rules_after_command(&kind, &row)?;
    let config = relay_config_or_default(ctx);
    let sender_seq = rate_allow(ctx, &config)?;
    let command = ctx.db.match_command().insert(MatchCommand {
        command_id: 0,
        match_id,
        sender: ctx.sender(),
        sender_seq,
        client_tick,
        kind,
        sent_at: ctx.timestamp,
    });
    let (rules_json, rules_epoch) = match &rules_update {
        Some((json, epoch)) => (json.clone(), *epoch),
        None => (row.rules_json.clone(), row.rules_epoch),
    };
    ctx.db.relay_match().match_id().update(RelayMatch {
        last_command_id: command.command_id,
        rules_json,
        rules_epoch,
        ..row
    });
    if let Some((rules_json, rules_epoch)) = rules_update {
        upsert_match_state_rules(ctx, match_id, &rules_json, rules_epoch);
    }
    // Per-sender acceptance bookkeeping (plan §3.5).
    let identity = ctx.sender();
    let existing = ctx.db.sender_command_state().identity().find(identity);
    let state = SenderCommandState {
        identity,
        last_accepted_seq: sender_seq,
        last_accepted_command_id: command.command_id,
        reject_count: existing.as_ref().map(|s| s.reject_count).unwrap_or(0),
        last_gap_at: existing.as_ref().and_then(|s| s.last_gap_at),
        updated_at: ctx.timestamp,
    };
    match existing {
        Some(_) => {
            ctx.db.sender_command_state().identity().update(state);
        }
        None => {
            ctx.db.sender_command_state().insert(state);
        }
    }
    Ok(())
}

/// Rules changes a command carries, validated against the current epoch.
///
/// `SetRules` replaces the whole blob (host edit); `SetRule` is a single-field
/// edit whose merge is plan 12/13's job — until that lands the stub only
/// materializes the first field when `rules_json` is empty, otherwise it keeps
/// the blob and just advances the epoch (documented M3 stub).
pub fn rules_after_command(
    kind: &CommandKind,
    row: &RelayMatch,
) -> Result<Option<(String, u32)>, String> {
    match kind {
        CommandKind::SetRules(SetRules {
            rules_json,
            rules_epoch,
        }) => {
            if *rules_epoch != row.rules_epoch {
                return Err(format!(
                    "rules epoch {rules_epoch} != current {}",
                    row.rules_epoch
                ));
            }
            Ok(Some((
                rules_json.clone(),
                row.rules_epoch.saturating_add(1),
            )))
        }
        CommandKind::SetRule(SetRule { rule, json }) => Ok(Some((
            merge_rule_stub(&row.rules_json, rule, json),
            row.rules_epoch.saturating_add(1),
        ))),
        _ => Ok(None),
    }
}

/// Stubbed top-level rule merge (see [`rules_after_command`]).
pub fn merge_rule_stub(rules_json: &str, rule: &str, json: &str) -> String {
    let trimmed = rules_json.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        format!("{{\"{rule}\":{json}}}")
    } else {
        rules_json.to_string()
    }
}

/// Mirrors a rules change into `match_state` so late joiners read the latest
/// (plan §3.6); inserts a zeroed state row when the host has not published yet.
fn upsert_match_state_rules(
    ctx: &ReducerContext,
    match_id: u64,
    rules_json: &str,
    rules_epoch: u32,
) {
    let existing = ctx
        .db
        .match_state()
        .by_match_state()
        .filter(match_id)
        .next();
    match existing {
        Some(state) => {
            ctx.db.match_state().state_id().update(MatchState {
                rules_json: rules_json.to_string(),
                rules_epoch,
                updated_by: ctx.sender(),
                updated_at: ctx.timestamp,
                ..state
            });
        }
        None => {
            ctx.db.match_state().insert(MatchState {
                state_id: 0,
                match_id,
                rules_json: rules_json.to_string(),
                rules_epoch,
                wave: 0,
                wavetime: 0.0,
                enemies: 0,
                paused: false,
                game_over: false,
                sim_tick: 0,
                last_command_id: 0,
                rng_sim: Vec::new(),
                next_entity_id: 0,
                snapshot_id: None,
                updated_by: ctx.sender(),
                updated_at: ctx.timestamp,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn relay_row(rules_json: &str, rules_epoch: u32) -> RelayMatch {
        RelayMatch {
            match_id: 1,
            map_id: "demo".to_string(),
            map_seed: 1,
            map_hash: 0,
            map_width_tiles: 100,
            map_height_tiles: 100,
            status: MatchStatus::Running,
            authority: AuthorityMode::Relay,
            protocol_version: PROTOCOL_VERSION,
            created_by: spacetimedb::Identity::from_byte_array([1u8; 32]),
            created_at: spacetimedb::Timestamp::UNIX_EPOCH,
            started_at: None,
            ended_at: None,
            host: spacetimedb::Identity::from_byte_array([1u8; 32]),
            mode: Gamemode::Survival,
            mode_name: "survival".to_string(),
            visibility: Visibility::Public,
            password_hash: None,
            rules_json: rules_json.to_string(),
            rules_epoch,
            build_id: String::new(),
            content_hash: 0,
            is_dedicated: false,
            player_count: 1,
            max_players: 8,
            campaign_id: None,
            sector_planet: None,
            sector_id: None,
            last_command_id: 0,
            last_snapshot_id: None,
            closed_at: None,
        }
    }

    #[test]
    fn set_rules_updates_match_state() {
        let row = relay_row("{}", 1);
        let update = rules_after_command(
            &CommandKind::SetRules(SetRules {
                rules_json: "{\"waves\":true}".to_string(),
                rules_epoch: 1,
            }),
            &row,
        )
        .unwrap()
        .expect("rules update");
        assert_eq!(update, ("{\"waves\":true}".to_string(), 2));
    }

    #[test]
    fn set_rules_epoch_mismatch() {
        let row = relay_row("{}", 5);
        let error = rules_after_command(
            &CommandKind::SetRules(SetRules {
                rules_json: "{}".to_string(),
                rules_epoch: 1,
            }),
            &row,
        )
        .unwrap_err();
        assert!(error.contains("rules epoch 1 != current 5"));
    }

    #[test]
    fn set_rule_stub_materializes_first_field() {
        assert_eq!(merge_rule_stub("{}", "waves", "true"), "{\"waves\":true}");
        assert_eq!(merge_rule_stub("", "pvp", "false"), "{\"pvp\":false}");
        // Non-empty blobs are left untouched until plan 12/13's merge lands.
        assert_eq!(
            merge_rule_stub("{\"waves\":true}", "pvp", "false"),
            "{\"waves\":true}"
        );
    }

    #[test]
    fn non_rules_commands_do_not_change_rules() {
        let row = relay_row("{}", 1);
        assert!(
            rules_after_command(&CommandKind::Noop, &row)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn set_rules_host_only_is_enforced_by_role() {
        assert_eq!(
            command_role(&CommandKind::SetRules(SetRules {
                rules_json: "{}".to_string(),
                rules_epoch: 1,
            })),
            CommandRole::HostOrAdmin
        );
    }
}
