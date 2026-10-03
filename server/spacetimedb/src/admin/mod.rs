// SPDX-License-Identifier: GPL-3.0-only

//! Admin/moderation tables, reducers and views (plan 21 §3.10/§4/§6.1/§6.8).
//!
//! Plan 21 M1 shipped the singleton config, the global admin identity set, the
//! identity ban list and the whitelist so join validation had a data source.
//! M6 adds the full reducer surface (`admin_kick`/`admin_ban`/…), the admin
//! rate window, the committed-action log and the admin-gated views.
//!
//! Under D2 there is no IP/subnet ban data source (OD-21-C): only STDB identity
//! bans + whitelist. Wave/team/tile actions are emitted as ordered
//! `match_command` rows so every peer applies them deterministically.

use spacetimedb::{
    Identity, ReducerContext, SpacetimeType, Table, TimeDuration, Timestamp, ViewContext, reducer,
    table, view,
};

use std::ops::Bound;

use crate::chat::{ChatFilter, chat_filter, chat_filter__view};
use crate::main::audit::audit;
use crate::main::global::{
    ADMIN_RATE_MAX, ADMIN_RATE_WINDOW_MS, MAX_BAN_SECS, MAX_CHAT_FILTER_LEN,
};
use crate::main::tables::AuditKind;
use crate::relay::methods::{is_admin, require_match, valid_content_name, window_expired};
use crate::relay::reducers::send_match_command_impl;
use crate::relay::tables::{
    AdminSwitchTeam, AdminTileOp, CommandKind, MatchKick, RunWave, match_kick, match_state,
    relay_member,
};

/// Module-wide server configuration, singleton `id = 0` (plan §6.1).
#[table(accessor = server_config, public)]
pub struct ServerConfig {
    #[primary_key]
    pub id: u8,
    pub server_name: String,
    pub description: String,
    pub motd: String,
    pub whitelist_enabled: bool,
    pub allow_custom_clients: bool,
    pub max_players_default: u16,
    pub auto_pause: bool,
    pub chat_rate_window_ms: u32,
    pub chat_rate_max: u32,
    pub snapshot_interval_ticks: u32,
    pub digest_interval_ticks: u32,
    pub checksum_interval_ticks: u32,
    pub command_retention_commands: u64,
    pub chat_retention_rows: u32,
    pub match_ttl_hours: u32,
    pub banned_name_patterns: Vec<String>,
}

/// Global admin grant (plan §6.1); bootstrap seed in [`super::main::seeds`].
#[table(accessor = admin_identity, public, index(accessor = by_admin_granted, btree(columns = [granted_at])))]
pub struct AdminIdentity {
    #[primary_key]
    pub identity: Identity,
    pub granted_by: Identity,
    pub granted_at: Timestamp,
}

/// Identity ban (plan §3.10; no IP/subnet bans under D2).
#[table(accessor = player_ban, public, index(accessor = by_ban_identity, btree(columns = [identity])))]
pub struct PlayerBan {
    #[primary_key]
    #[auto_inc]
    pub ban_id: u64,
    pub identity: Identity,
    pub reason: String,
    pub banned_by: Identity,
    pub created_at: Timestamp,
    pub expires_at: Option<Timestamp>,
}

/// Whitelist entry (plan §3.10/§6.1).
#[table(accessor = whitelist_entry, public, index(accessor = by_whitelist_added, btree(columns = [added_at])))]
pub struct WhitelistEntry {
    #[primary_key]
    pub identity: Identity,
    pub added_by: Identity,
    pub added_at: Timestamp,
}

/// Committed admin action kind (plan §3.10; variant names are ABI).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AdminActionKind {
    /// Removed a member from a match.
    Kick,
    /// Wrote an identity ban.
    Ban,
    /// Deleted identity bans.
    Unban,
    /// Granted a global admin.
    GrantAdmin,
    /// Revoked a global admin.
    RevokeAdmin,
    /// Added a whitelist entry.
    WhitelistAdd,
    /// Removed a whitelist entry.
    WhitelistRemove,
    /// Toggled `server_config.whitelist_enabled`.
    SetWhitelist,
    /// Set a typed server-config field.
    SetConfig,
    /// Forced the next wave.
    RunWave,
    /// Skipped the current wave.
    SkipWave,
    /// Switched a player's team.
    SwitchTeam,
    /// Applied a remote tile operation.
    TileOp,
    /// Added a chat filter.
    AddChatFilter,
    /// Removed a chat filter.
    RemoveChatFilter,
}

impl AdminActionKind {
    /// Whether the action is emitted as an ordered `match_command` row.
    pub fn emits_command(self) -> bool {
        matches!(
            self,
            Self::RunWave | Self::SkipWave | Self::SwitchTeam | Self::TileOp
        )
    }
}

/// Server-only per-identity admin rate window (plan §6.1).
#[table(accessor = admin_rate)]
pub struct AdminRate {
    #[primary_key]
    pub identity: Identity,
    pub window_start: Timestamp,
    pub actions: u32,
}

/// Append-only committed admin action log (plan §6.1).
#[table(accessor = admin_action_log, public, index(accessor = by_admin_action, btree(columns = [at])))]
pub struct AdminActionLog {
    #[primary_key]
    #[auto_inc]
    pub action_id: u64,
    pub actor: Identity,
    pub kind: AdminActionKind,
    pub target: Option<Identity>,
    pub match_id: Option<u64>,
    pub detail: String,
    pub at: Timestamp,
}

/// Defaults for the [`ServerConfig`] singleton (plan §6.8).
pub fn default_server_config() -> ServerConfig {
    ServerConfig {
        id: 0,
        server_name: "Mindustry-Godot".to_string(),
        description: String::new(),
        motd: String::new(),
        whitelist_enabled: false,
        allow_custom_clients: true,
        max_players_default: crate::main::global::DEFAULT_MAX_PLAYERS,
        auto_pause: false,
        chat_rate_window_ms: 2_000,
        chat_rate_max: 20,
        snapshot_interval_ticks: crate::main::global::SNAPSHOT_INTERVAL_TICKS as u32,
        digest_interval_ticks: crate::main::global::DIGEST_INTERVAL_TICKS as u32,
        checksum_interval_ticks: crate::main::global::CHECKSUM_INTERVAL_TICKS as u32,
        command_retention_commands: crate::main::global::REPLAY_TAIL,
        chat_retention_rows: 500,
        match_ttl_hours: 24,
        banned_name_patterns: Vec::new(),
    }
}

/// Loads the singleton config (defaults when `init` has not seeded yet).
pub fn server_config_or_default(ctx: &ReducerContext) -> ServerConfig {
    ctx.db
        .server_config()
        .id()
        .find(0u8)
        .unwrap_or_else(default_server_config)
}

// ---- pure helpers (unit-testable without a database) ------------------------

/// Whether `kind` is allowed for an actor with the given privileges.
///
/// `admin` is the global `admin_identity` grant; `host` is
/// `relay_match.host == ctx.sender()` (plan §3.10 admin table).
pub fn admin_gate(admin: bool, host: bool, kind: AdminActionKind) -> Result<(), String> {
    let allowed = match kind {
        AdminActionKind::Kick
        | AdminActionKind::RunWave
        | AdminActionKind::SkipWave
        | AdminActionKind::SwitchTeam
        | AdminActionKind::TileOp => admin || host,
        // Global/identity/config moderation is admin-only.
        AdminActionKind::Ban
        | AdminActionKind::Unban
        | AdminActionKind::GrantAdmin
        | AdminActionKind::RevokeAdmin
        | AdminActionKind::WhitelistAdd
        | AdminActionKind::WhitelistRemove
        | AdminActionKind::SetWhitelist
        | AdminActionKind::SetConfig
        | AdminActionKind::AddChatFilter
        | AdminActionKind::RemoveChatFilter => admin,
    };
    if allowed {
        Ok(())
    } else {
        Err("admin privileges required".to_string())
    }
}

/// Clamps a requested ban duration to `[0, MAX_BAN_SECS]`; `0` means permanent.
pub fn clamp_ban_secs(requested: Option<u64>) -> u64 {
    requested.unwrap_or(0).min(MAX_BAN_SECS)
}

/// Whether the admin rate window allows one more action.
pub fn admin_rate_allow(
    window_start_micros: i64,
    now_micros: i64,
    actions: u32,
    window_ms: u32,
    max: u32,
) -> bool {
    if window_expired(now_micros, window_start_micros, window_ms) {
        return true;
    }
    actions < max
}

/// Whether `field` is a writable `server_config` field (`admin_set_config`).
pub fn known_server_config_field(field: &str) -> bool {
    matches!(
        field,
        "server_name"
            | "description"
            | "motd"
            | "whitelist_enabled"
            | "allow_custom_clients"
            | "max_players_default"
            | "auto_pause"
            | "chat_rate_window_ms"
            | "chat_rate_max"
            | "snapshot_interval_ticks"
            | "digest_interval_ticks"
            | "checksum_interval_ticks"
            | "command_retention_commands"
            | "chat_retention_rows"
            | "match_ttl_hours"
    )
}

fn parse_bool(value: &str) -> Result<bool, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Ok(true),
        "false" | "off" | "no" | "0" => Ok(false),
        _ => Err(format!("invalid boolean `{value}`")),
    }
}

fn parse_u32(value: &str) -> Result<u32, String> {
    value
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("invalid integer `{value}`"))
}

fn parse_u16(value: &str) -> Result<u16, String> {
    value
        .trim()
        .parse::<u16>()
        .map_err(|_| format!("invalid integer `{value}`"))
}

fn parse_u64(value: &str) -> Result<u64, String> {
    value
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("invalid integer `{value}`"))
}

// ---- reducer plumbing -------------------------------------------------------

/// Applies the per-identity admin rate window; `Err` rolls back the reducer.
fn admin_rate_check(ctx: &ReducerContext) -> Result<(), String> {
    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();
    let identity = ctx.sender();
    let existing = ctx.db.admin_rate().identity().find(identity);
    let mut rate = existing.unwrap_or(AdminRate {
        identity,
        window_start: now,
        actions: 0,
    });
    if window_expired(
        now_micros,
        rate.window_start.to_micros_since_unix_epoch(),
        ADMIN_RATE_WINDOW_MS,
    ) {
        rate.window_start = now;
        rate.actions = 0;
    }
    if !admin_rate_allow(
        now_micros,
        now_micros,
        rate.actions,
        ADMIN_RATE_WINDOW_MS,
        ADMIN_RATE_MAX,
    ) {
        return Err("admin rate limit exceeded".to_string());
    }
    rate.actions = rate.actions.saturating_add(1);
    if ctx.db.admin_rate().identity().find(identity).is_some() {
        ctx.db.admin_rate().identity().update(rate);
    } else {
        ctx.db.admin_rate().insert(rate);
    }
    Ok(())
}

/// Records one committed admin action + audit line.
fn record(
    ctx: &ReducerContext,
    kind: AdminActionKind,
    target: Option<Identity>,
    match_id: Option<u64>,
    detail: impl Into<String>,
) {
    let detail = detail.into();
    ctx.db.admin_action_log().insert(AdminActionLog {
        action_id: 0,
        actor: ctx.sender(),
        kind,
        target,
        match_id,
        detail: detail.clone(),
        at: ctx.timestamp,
    });
    audit(ctx, Some(ctx.sender()), AuditKind::ConfigChange, detail);
}

// ---- reducers ---------------------------------------------------------------

/// Kicks a member from a match (admin or match host; plan §3.10).
#[reducer]
pub fn admin_kick(
    ctx: &ReducerContext,
    match_id: u64,
    target: Identity,
    reason: String,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    admin_gate(
        is_admin(ctx),
        row.host == ctx.sender(),
        AdminActionKind::Kick,
    )?;
    if target == ctx.sender() {
        return Err("cannot kick yourself".to_string());
    }
    let member = ctx
        .db
        .relay_member()
        .by_match_identity()
        .filter((match_id, target))
        .next()
        .ok_or_else(|| "target is not a member of this match".to_string())?;
    admin_rate_check(ctx)?;
    ctx.db.relay_member().member_id().update(RelayMemberRow {
        kicked_reason: Some(reason.clone()),
        connected: false,
        ..member
    });
    ctx.db.match_kick().insert(MatchKick {
        kick_id: 0,
        match_id,
        target,
        reason: reason.clone(),
        by: ctx.sender(),
        at: ctx.timestamp,
    });
    record(
        ctx,
        AdminActionKind::Kick,
        Some(target),
        Some(match_id),
        reason,
    );
    Ok(())
}

/// Bans an identity (admin only; duration capped at 30 days; `None` permanent).
#[reducer]
pub fn admin_ban(
    ctx: &ReducerContext,
    target: Identity,
    reason: String,
    duration_secs: Option<u64>,
) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::Ban)?;
    if ctx.db.admin_identity().identity().find(target).is_some() {
        return Err("cannot ban an admin".to_string());
    }
    admin_rate_check(ctx)?;
    let secs = clamp_ban_secs(duration_secs);
    let expires_at = if secs == 0 {
        None
    } else {
        Some(ctx.timestamp + TimeDuration::from_micros((secs * 1_000_000) as i64))
    };
    ctx.db.player_ban().insert(PlayerBan {
        ban_id: 0,
        identity: target,
        reason: reason.clone(),
        banned_by: ctx.sender(),
        created_at: ctx.timestamp,
        expires_at,
    });
    // Kick the identity from every match it currently belongs to.
    for member in ctx
        .db
        .relay_member()
        .by_identity()
        .filter(target)
        .collect::<Vec<_>>()
    {
        ctx.db.relay_member().member_id().update(RelayMemberRow {
            kicked_reason: Some(reason.clone()),
            connected: false,
            ..member
        });
        ctx.db.match_kick().insert(MatchKick {
            kick_id: 0,
            match_id: member.match_id,
            target,
            reason: reason.clone(),
            by: ctx.sender(),
            at: ctx.timestamp,
        });
    }
    record(ctx, AdminActionKind::Ban, Some(target), None, reason);
    Ok(())
}

/// Removes every ban row for an identity (admin only).
#[reducer]
pub fn admin_unban(ctx: &ReducerContext, target: Identity) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::Unban)?;
    admin_rate_check(ctx)?;
    let bans: Vec<PlayerBan> = ctx
        .db
        .player_ban()
        .by_ban_identity()
        .filter(target)
        .collect();
    let count = bans.len();
    for ban in bans {
        ctx.db.player_ban().ban_id().delete(ban.ban_id);
    }
    record(
        ctx,
        AdminActionKind::Unban,
        Some(target),
        None,
        format!("removed {count} ban row(s)"),
    );
    Ok(())
}

/// Toggles the global whitelist gate (admin only).
#[reducer]
pub fn admin_set_whitelist(ctx: &ReducerContext, enabled: bool) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::SetWhitelist)?;
    admin_rate_check(ctx)?;
    let config = server_config_or_default(ctx);
    ctx.db.server_config().id().update(ServerConfig {
        whitelist_enabled: enabled,
        ..config
    });
    record(
        ctx,
        AdminActionKind::SetWhitelist,
        None,
        None,
        format!("whitelist={enabled}"),
    );
    Ok(())
}

/// Adds or removes one whitelist entry (admin only).
#[reducer]
pub fn admin_whitelist(ctx: &ReducerContext, target: Identity, on: bool) -> Result<(), String> {
    let kind = if on {
        AdminActionKind::WhitelistAdd
    } else {
        AdminActionKind::WhitelistRemove
    };
    admin_gate(is_admin(ctx), false, kind)?;
    admin_rate_check(ctx)?;
    if on {
        if ctx.db.whitelist_entry().identity().find(target).is_none() {
            ctx.db.whitelist_entry().insert(WhitelistEntry {
                identity: target,
                added_by: ctx.sender(),
                added_at: ctx.timestamp,
            });
        }
    } else {
        ctx.db.whitelist_entry().identity().delete(target);
    }
    record(ctx, kind, Some(target), None, format!("whitelisted={on}"));
    Ok(())
}

/// Grants or revokes a global admin (admin only).
#[reducer]
pub fn admin_grant(ctx: &ReducerContext, target: Identity, on: bool) -> Result<(), String> {
    let kind = if on {
        AdminActionKind::GrantAdmin
    } else {
        AdminActionKind::RevokeAdmin
    };
    admin_gate(is_admin(ctx), false, kind)?;
    admin_rate_check(ctx)?;
    if on {
        if ctx.db.admin_identity().identity().find(target).is_none() {
            ctx.db.admin_identity().insert(AdminIdentity {
                identity: target,
                granted_by: ctx.sender(),
                granted_at: ctx.timestamp,
            });
        }
    } else if target == ctx.sender() {
        return Err("cannot revoke your own admin".to_string());
    } else {
        ctx.db.admin_identity().identity().delete(target);
    }
    record(ctx, kind, Some(target), None, format!("admin={on}"));
    Ok(())
}

/// Sets a typed `server_config` field (admin only; no free-form keys).
#[reducer]
pub fn admin_set_config(ctx: &ReducerContext, field: String, value: String) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::SetConfig)?;
    if !known_server_config_field(&field) {
        return Err(format!("unknown server config field `{field}`"));
    }
    admin_rate_check(ctx)?;
    let config = server_config_or_default(ctx);
    let updated = match field.as_str() {
        "server_name" => ServerConfig {
            server_name: value.clone(),
            ..config
        },
        "description" => ServerConfig {
            description: value.clone(),
            ..config
        },
        "motd" => ServerConfig {
            motd: value.clone(),
            ..config
        },
        "whitelist_enabled" => ServerConfig {
            whitelist_enabled: parse_bool(&value)?,
            ..config
        },
        "allow_custom_clients" => ServerConfig {
            allow_custom_clients: parse_bool(&value)?,
            ..config
        },
        "max_players_default" => ServerConfig {
            max_players_default: parse_u16(&value)?.min(64),
            ..config
        },
        "auto_pause" => ServerConfig {
            auto_pause: parse_bool(&value)?,
            ..config
        },
        "chat_rate_window_ms" => ServerConfig {
            chat_rate_window_ms: parse_u32(&value)?,
            ..config
        },
        "chat_rate_max" => ServerConfig {
            chat_rate_max: parse_u32(&value)?,
            ..config
        },
        "snapshot_interval_ticks" => ServerConfig {
            snapshot_interval_ticks: parse_u32(&value)?,
            ..config
        },
        "digest_interval_ticks" => ServerConfig {
            digest_interval_ticks: parse_u32(&value)?,
            ..config
        },
        "checksum_interval_ticks" => ServerConfig {
            checksum_interval_ticks: parse_u32(&value)?,
            ..config
        },
        "command_retention_commands" => ServerConfig {
            command_retention_commands: parse_u64(&value)?,
            ..config
        },
        "chat_retention_rows" => ServerConfig {
            chat_retention_rows: parse_u32(&value)?,
            ..config
        },
        "match_ttl_hours" => ServerConfig {
            match_ttl_hours: parse_u32(&value)?,
            ..config
        },
        _ => return Err(format!("unknown server config field `{field}`")),
    };
    ctx.db.server_config().id().update(updated);
    record(
        ctx,
        AdminActionKind::SetConfig,
        None,
        None,
        format!("{field}={value}"),
    );
    Ok(())
}

/// Adds a global chat filter (admin only).
#[reducer]
pub fn admin_add_chat_filter(
    ctx: &ReducerContext,
    pattern: String,
    mute: bool,
) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::AddChatFilter)?;
    let pattern = pattern.trim().to_lowercase();
    if pattern.is_empty() || pattern.chars().count() > MAX_CHAT_FILTER_LEN {
        return Err(format!("filter must be 1-{MAX_CHAT_FILTER_LEN} characters"));
    }
    admin_rate_check(ctx)?;
    ctx.db.chat_filter().insert(ChatFilter {
        filter_id: 0,
        pattern: pattern.clone(),
        mute,
        added_by: ctx.sender(),
        added_at: ctx.timestamp,
    });
    record(
        ctx,
        AdminActionKind::AddChatFilter,
        None,
        None,
        format!("{pattern} mute={mute}"),
    );
    Ok(())
}

/// Removes a chat filter by id (admin only).
#[reducer]
pub fn admin_remove_chat_filter(ctx: &ReducerContext, filter_id: u64) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::RemoveChatFilter)?;
    admin_rate_check(ctx)?;
    if ctx.db.chat_filter().filter_id().find(filter_id).is_none() {
        return Err(format!("unknown chat filter {filter_id}"));
    }
    ctx.db.chat_filter().filter_id().delete(filter_id);
    record(
        ctx,
        AdminActionKind::RemoveChatFilter,
        None,
        None,
        format!("filter {filter_id}"),
    );
    Ok(())
}

/// Forces the next wave as an ordered `SkipWave` command (admin or host).
#[reducer]
pub fn admin_skip_wave(ctx: &ReducerContext, match_id: u64) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    admin_gate(
        is_admin(ctx),
        row.host == ctx.sender(),
        AdminActionKind::SkipWave,
    )?;
    admin_rate_check(ctx)?;
    let client_tick = ctx
        .db
        .match_state()
        .by_match_state()
        .filter(match_id)
        .next()
        .map(|state| state.sim_tick)
        .unwrap_or(0);
    send_match_command_impl(ctx, match_id, client_tick, CommandKind::SkipWave)?;
    record(
        ctx,
        AdminActionKind::SkipWave,
        None,
        Some(match_id),
        "skip wave",
    );
    Ok(())
}

/// Runs `count` waves as an ordered `RunWave` command (admin or host).
#[reducer]
pub fn admin_run_wave(ctx: &ReducerContext, match_id: u64, count: u8) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    admin_gate(
        is_admin(ctx),
        row.host == ctx.sender(),
        AdminActionKind::RunWave,
    )?;
    if count == 0 || count > 10 {
        return Err("wave count must be 1..=10".to_string());
    }
    admin_rate_check(ctx)?;
    let client_tick = ctx
        .db
        .match_state()
        .by_match_state()
        .filter(match_id)
        .next()
        .map(|state| state.sim_tick)
        .unwrap_or(0);
    send_match_command_impl(
        ctx,
        match_id,
        client_tick,
        CommandKind::RunWave(RunWave { count }),
    )?;
    record(
        ctx,
        AdminActionKind::RunWave,
        None,
        Some(match_id),
        format!("run {count} wave(s)"),
    );
    Ok(())
}

/// Switches a player's team: updates the member row and emits the ordered
/// `AdminSwitchTeam` command (admin or host).
#[reducer]
pub fn admin_switch_team(
    ctx: &ReducerContext,
    match_id: u64,
    target: Identity,
    team: u8,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    admin_gate(
        is_admin(ctx),
        row.host == ctx.sender(),
        AdminActionKind::SwitchTeam,
    )?;
    let member = require_member_of(ctx, match_id, target)?;
    admin_rate_check(ctx)?;
    let client_tick = ctx
        .db
        .match_state()
        .by_match_state()
        .filter(match_id)
        .next()
        .map(|state| state.sim_tick)
        .unwrap_or(0);
    send_match_command_impl(
        ctx,
        match_id,
        client_tick,
        CommandKind::AdminSwitchTeam(AdminSwitchTeam { target, team }),
    )?;
    ctx.db.relay_member().member_id().update(RelayMemberRow {
        team: Some(team),
        ..member
    });
    record(
        ctx,
        AdminActionKind::SwitchTeam,
        Some(target),
        Some(match_id),
        format!("team={team}"),
    );
    Ok(())
}

/// Applies a host/admin remote tile operation as an ordered `AdminTileOp`.
#[reducer]
#[allow(clippy::too_many_arguments)]
pub fn admin_tile_op(
    ctx: &ReducerContext,
    match_id: u64,
    op: u8,
    points: Vec<i32>,
    arg0: i32,
    arg1: i32,
    arg2: i32,
    name0: Option<String>,
    name1: Option<String>,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    admin_gate(
        is_admin(ctx),
        row.host == ctx.sender(),
        AdminActionKind::TileOp,
    )?;
    admin_rate_check(ctx)?;
    if let Some(name) = &name0 {
        valid_content_name(name, crate::relay::methods::MAX_CONTENT_NAME_LEN)?;
    }
    if let Some(name) = &name1 {
        valid_content_name(name, crate::relay::methods::MAX_CONTENT_NAME_LEN)?;
    }
    let client_tick = ctx
        .db
        .match_state()
        .by_match_state()
        .filter(match_id)
        .next()
        .map(|state| state.sim_tick)
        .unwrap_or(0);
    send_match_command_impl(
        ctx,
        match_id,
        client_tick,
        CommandKind::AdminTileOp(AdminTileOp {
            op,
            points,
            arg0,
            arg1,
            arg2,
            name0,
            name1,
        }),
    )?;
    record(
        ctx,
        AdminActionKind::TileOp,
        None,
        Some(match_id),
        format!("op={op}"),
    );
    Ok(())
}

/// Local alias so `..member` spread updates read naturally.
type RelayMemberRow = crate::relay::tables::RelayMember;

/// Requires `target` to be a member of `match_id`.
fn require_member_of(
    ctx: &ReducerContext,
    match_id: u64,
    target: Identity,
) -> Result<RelayMemberRow, String> {
    ctx.db
        .relay_member()
        .by_match_identity()
        .filter((match_id, target))
        .next()
        .ok_or_else(|| "target is not a member of this match".to_string())
}

// ---- views ------------------------------------------------------------------

/// Whether the caller is a global admin (drives the 14 admin menu).
#[view(accessor = am_i_admin, public)]
fn am_i_admin(ctx: &ViewContext) -> Option<AdminIdentity> {
    ctx.db.admin_identity().identity().find(ctx.sender())
}

/// Every ban row, visible to admins only (plan §6.2).
///
/// Full-table scans in read-only views must go through an index (view handles
/// expose no `iter()`), so this ranges-unbounded over `by_ban_identity`.
#[view(accessor = all_bans, public)]
fn all_bans(ctx: &ViewContext) -> Vec<PlayerBan> {
    if !view_is_admin(ctx) {
        return Vec::new();
    }
    ctx.db
        .player_ban()
        .by_ban_identity()
        .filter((Bound::<Identity>::Unbounded, Bound::<Identity>::Unbounded))
        .collect()
}

/// The whitelist, visible to admins only (plan §6.2).
#[view(accessor = all_whitelist, public)]
fn all_whitelist(ctx: &ViewContext) -> Vec<WhitelistEntry> {
    if !view_is_admin(ctx) {
        return Vec::new();
    }
    ctx.db
        .whitelist_entry()
        .by_whitelist_added()
        .filter((Bound::<Timestamp>::Unbounded, Bound::<Timestamp>::Unbounded))
        .collect()
}

/// The global admin list, visible to admins only (plan §6.2).
#[view(accessor = all_admins, public)]
fn all_admins(ctx: &ViewContext) -> Vec<AdminIdentity> {
    if !view_is_admin(ctx) {
        return Vec::new();
    }
    ctx.db
        .admin_identity()
        .by_admin_granted()
        .filter((Bound::<Timestamp>::Unbounded, Bound::<Timestamp>::Unbounded))
        .collect()
}

/// Global chat filters, visible to admins only (plan §6.2).
#[view(accessor = all_chat_filters, public)]
fn all_chat_filters(ctx: &ViewContext) -> Vec<ChatFilter> {
    if !view_is_admin(ctx) {
        return Vec::new();
    }
    ctx.db
        .chat_filter()
        .by_chat_filter_added()
        .filter((Bound::<Timestamp>::Unbounded, Bound::<Timestamp>::Unbounded))
        .collect()
}

/// The caller's own committed admin actions (plan §6.2).
#[view(accessor = my_admin_actions, public)]
fn my_admin_actions(ctx: &ViewContext) -> Vec<AdminActionLog> {
    ctx.db
        .admin_action_log()
        .by_admin_action()
        .filter((Bound::<Timestamp>::Unbounded, Bound::<Timestamp>::Unbounded))
        .filter(|entry| entry.actor == ctx.sender())
        .collect()
}

/// View-safe admin check (mirrors [`is_admin`]).
fn view_is_admin(ctx: &ViewContext) -> bool {
    ctx.db
        .admin_identity()
        .identity()
        .find(ctx.sender())
        .is_some()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::relay::methods::{JoinGate, hash_password, validate_join};

    #[test]
    fn ban_blocks_join() {
        let gate = JoinGate {
            banned: true,
            ..JoinGate::default()
        };
        assert_eq!(
            validate_join(&gate).unwrap_err(),
            "You are banned from this server."
        );
    }

    #[test]
    fn whitelist_blocks_join() {
        let gate = JoinGate {
            whitelist_enabled: true,
            whitelisted: false,
            ..JoinGate::default()
        };
        assert_eq!(
            validate_join(&gate).unwrap_err(),
            "This server is whitelisted."
        );
        let allowed = JoinGate {
            whitelist_enabled: true,
            whitelisted: true,
            ..JoinGate::default()
        };
        assert!(validate_join(&allowed).is_ok());
    }

    #[test]
    fn password_gate_uses_salted_hash() {
        let gate = JoinGate {
            stored_password_hash: Some(hash_password("secret")),
            supplied_password: Some("wrong".to_string()),
            max_players: 8,
            ..JoinGate::default()
        };
        assert_eq!(validate_join(&gate).unwrap_err(), "Incorrect password.");
    }

    #[test]
    fn admin_gate_denies_non_admin() {
        for kind in [
            AdminActionKind::Ban,
            AdminActionKind::Unban,
            AdminActionKind::SetWhitelist,
            AdminActionKind::SetConfig,
        ] {
            assert!(admin_gate(false, false, kind).is_err());
            assert!(admin_gate(false, true, kind).is_err());
            assert!(admin_gate(true, false, kind).is_ok());
        }
    }

    #[test]
    fn host_can_kick() {
        assert!(admin_gate(false, true, AdminActionKind::Kick).is_ok());
        assert!(admin_gate(true, false, AdminActionKind::Kick).is_ok());
        assert!(admin_gate(false, false, AdminActionKind::Kick).is_err());
    }

    #[test]
    fn trace_requires_admin() {
        // The trace surface is the admin-only logs/lists.
        assert!(admin_gate(false, true, AdminActionKind::SetConfig).is_err());
        assert!(admin_gate(true, false, AdminActionKind::SetConfig).is_ok());
        assert!(known_server_config_field("auto_pause"));
        assert!(!known_server_config_field("mystery"));
    }

    #[test]
    fn switch_team_emits_command() {
        assert!(AdminActionKind::SwitchTeam.emits_command());
        assert!(AdminActionKind::RunWave.emits_command());
        assert!(!AdminActionKind::Kick.emits_command());
        assert!(!AdminActionKind::SetConfig.emits_command());
    }

    #[test]
    fn ban_duration_is_capped() {
        assert_eq!(clamp_ban_secs(None), 0);
        assert_eq!(clamp_ban_secs(Some(60)), 60);
        assert_eq!(clamp_ban_secs(Some(u64::MAX)), MAX_BAN_SECS);
    }

    #[test]
    fn admin_rate_window() {
        assert!(admin_rate_allow(0, 0, 0, 10_000, 5));
        assert!(admin_rate_allow(0, 1_000, 4, 10_000, 5));
        assert!(!admin_rate_allow(0, 1_000, 5, 10_000, 5));
        // Window roll resets.
        assert!(admin_rate_allow(0, 11_000_000, 5, 10_000, 5));
    }

    #[test]
    fn chat_filter_cap() {
        assert!(admin_add_chat_filter_validation("badword", true).is_ok());
        assert!(admin_add_chat_filter_validation("", true).is_err());
        assert!(
            admin_add_chat_filter_validation(&"x".repeat(MAX_CHAT_FILTER_LEN + 1), true).is_err()
        );
    }

    /// Mirror of the `admin_add_chat_filter` validation (pure).
    fn admin_add_chat_filter_validation(pattern: &str, _mute: bool) -> Result<(), String> {
        let pattern = pattern.trim().to_lowercase();
        if pattern.is_empty() || pattern.chars().count() > MAX_CHAT_FILTER_LEN {
            return Err("bad filter".to_string());
        }
        Ok(())
    }
}
