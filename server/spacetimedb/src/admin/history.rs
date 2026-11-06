// SPDX-License-Identifier: GPL-3.0-only

//! Player history, trace info and name-ban enforcement (GAPS #34; Java
//! `Administration.java:200-300` `updatePlayerJoined`/`findByName`/
//! `searchNames`/`isNameBanned`, `Administration.java:364-460` kick bookkeeping
//! and `NetServer.java:906-951` `adminRequest(trace)`).
//!
//! `player_info` is the persistent per-identity history Java keeps in
//! `Administration.playerInfo`: current/last name, every name ever used, join
//! and kick counters, and the kick-cooldown expiry. Under D2 there is no client
//! address source (OD-21-C), so `last_ip`/`ips` stay empty until a trusted
//! address ingest exists; the columns keep the Java shape for a later wire-up.
//!
//! `player_trace` is the module-side answer to `adminRequest(trace)`: an admin
//! reducer writes one row, the requesting admin reads it back through
//! `my_traces` (the Java `TraceInfo` payload).

use regex::RegexBuilder;
use spacetimedb::{Identity, ReducerContext, Table, Timestamp, ViewContext, reducer, table, view};

use std::ops::Bound;

use super::{
    AdminActionKind, ServerConfig, admin_gate, admin_rate_check, record, server_config,
    server_config_or_default, view_is_admin,
};
use crate::identity::tables::{client_settings, player};
use crate::main::global::{MAX_NAME_PATTERN_LEN, MAX_PLAYER_IPS, MAX_PLAYER_NAMES};
use crate::relay::methods::is_admin;
use crate::relay::tables::{MatchKick, match_kick, relay_member};

/// Persistent per-identity player history (Java `Administration.PlayerInfo`,
/// `Administration.java:602-620`).
#[table(accessor = player_info, index(accessor = by_player_info_seen, btree(columns = [last_seen_at])))]
pub struct PlayerInfo {
    /// Player UUID (SpacetimeDB identity).
    #[primary_key]
    pub identity: Identity,
    /// Most recent username (`PlayerInfo.lastName`).
    pub last_name: String,
    /// Every username seen (bounded by [`MAX_PLAYER_NAMES`]).
    pub names: Vec<String>,
    /// Most recent client address; `None` until an address ingest exists (D2).
    pub last_ip: Option<String>,
    /// Every client address seen (bounded by [`MAX_PLAYER_IPS`]).
    pub ips: Vec<String>,
    /// Successful joins (`PlayerInfo.timesJoined`).
    pub times_joined: u32,
    /// Kicks applied (`PlayerInfo.timesKicked`).
    pub times_kicked: u32,
    /// Kick-cooldown expiry (`PlayerInfo.lastKicked`, an absolute time).
    pub last_kicked_at: Option<Timestamp>,
    /// Last join/rename, used for idempotent history reads and pruning.
    pub last_seen_at: Timestamp,
}

/// One `adminRequest(trace)` response (Java `Administration.TraceInfo`,
/// `Administration.java:679-693`), targeted at the requesting admin.
#[table(accessor = player_trace, index(accessor = by_trace_requester, btree(columns = [requested_by])))]
pub struct PlayerTrace {
    /// Auto-inc response id.
    #[primary_key]
    #[auto_inc]
    pub trace_id: u64,
    /// Admin that requested the trace.
    pub requested_by: Identity,
    /// Traced player.
    pub target: Identity,
    /// Match context, when the request came from one.
    pub match_id: Option<u64>,
    /// Client address (always `None` under D2).
    pub ip: Option<String>,
    /// Client locale (`player.locale`, from `client_settings.language`).
    pub locale: Option<String>,
    /// Cumulative joins.
    pub times_joined: u32,
    /// Cumulative kicks.
    pub times_kicked: u32,
    /// Every address seen (empty under D2).
    pub ips: Vec<String>,
    /// Every username seen.
    pub names: Vec<String>,
    /// Request time (retention key).
    pub requested_at: Timestamp,
}

/// Removes `[tag]` colour/format markers, mirroring `arc.util.Strings.stripColors`.
///
/// Upstream strips every bracketed tag (`[red]`, `[#rrggbb]`, `[]`, ...); a
/// malformed trailing `[` is kept verbatim.
pub fn strip_colors(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' {
            let mut tag = String::new();
            let mut closed = false;
            for next in chars.by_ref() {
                if next == ']' {
                    closed = true;
                    break;
                }
                tag.push(next);
            }
            if !closed {
                out.push('[');
                out.push_str(&tag);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Adds `name` to a history's name list (case-insensitive dedupe, bounded).
fn push_name(names: &mut Vec<String>, name: &str) {
    if name.is_empty() {
        return;
    }
    if names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
        return;
    }
    if names.len() >= MAX_PLAYER_NAMES {
        names.remove(0);
    }
    names.push(name.to_string());
}

/// Adds `ip` to a history's address list (bounded); unused under D2 but keeps
/// the Java shape ready for an address ingest.
fn push_ip(ips: &mut Vec<String>, ip: &str) {
    if ip.is_empty() || ips.iter().any(|known| known.eq_ignore_ascii_case(ip)) {
        return;
    }
    if ips.len() >= MAX_PLAYER_IPS {
        ips.remove(0);
    }
    ips.push(ip.to_string());
}

/// Loads a history row, or `None` when the identity was never recorded.
pub fn info_for(ctx: &ReducerContext, identity: Identity) -> Option<PlayerInfo> {
    ctx.db.player_info().identity().find(identity)
}

/// Records a join: creates the history row, bumps `times_joined`, appends the
/// name. Mirrors `Administration.updatePlayerJoined` (`Administration.java:206`).
pub fn record_join(ctx: &ReducerContext, identity: Identity, name: &str) {
    let now = ctx.timestamp;
    match ctx.db.player_info().identity().find(identity) {
        Some(mut info) => {
            push_name(&mut info.names, name);
            if !name.is_empty() {
                info.last_name = name.to_string();
            }
            info.times_joined = info.times_joined.saturating_add(1);
            info.last_seen_at = now;
            ctx.db.player_info().identity().update(info);
        }
        None => {
            let mut names = Vec::new();
            push_name(&mut names, name);
            ctx.db.player_info().insert(PlayerInfo {
                identity,
                last_name: if name.is_empty() {
                    "<unknown>".to_string()
                } else {
                    name.to_string()
                },
                names,
                last_ip: None,
                ips: Vec::new(),
                times_joined: 1,
                times_kicked: 0,
                last_kicked_at: None,
                last_seen_at: now,
            });
        }
    }
}

/// Records a rename (`set_username`): appends the new name and stamps it last.
pub fn record_name(ctx: &ReducerContext, identity: Identity, name: &str) {
    let now = ctx.timestamp;
    match ctx.db.player_info().identity().find(identity) {
        Some(mut info) => {
            push_name(&mut info.names, name);
            info.last_name = name.to_string();
            info.last_seen_at = now;
            ctx.db.player_info().identity().update(info);
        }
        None => {
            let mut names = Vec::new();
            push_name(&mut names, name);
            ctx.db.player_info().insert(PlayerInfo {
                identity,
                last_name: name.to_string(),
                names,
                last_ip: None,
                ips: Vec::new(),
                times_joined: 0,
                times_kicked: 0,
                last_kicked_at: None,
                last_seen_at: now,
            });
        }
    }
}

/// Records a kick: bumps `times_kicked` and pushes the cooldown expiry
/// (Java `Administration.handleKicked`).
pub fn record_kick(ctx: &ReducerContext, identity: Identity, duration_secs: u64) {
    let now = ctx.timestamp;
    let expiry = if duration_secs == 0 {
        None
    } else {
        Some(
            now + spacetimedb::TimeDuration::from_micros(
                (duration_secs.saturating_mul(1_000_000)) as i64,
            ),
        )
    };
    match ctx.db.player_info().identity().find(identity) {
        Some(mut info) => {
            info.times_kicked = info.times_kicked.saturating_add(1);
            if let Some(expiry) = expiry {
                info.last_kicked_at = Some(match info.last_kicked_at {
                    Some(current) if current > expiry => current,
                    _ => expiry,
                });
            }
            info.last_seen_at = now;
            ctx.db.player_info().identity().update(info);
        }
        None => {
            // A kick can precede any tracked join (admin kicked a lobby member).
            let _ = ctx.db.player_info().insert(PlayerInfo {
                identity,
                last_name: "<unknown>".to_string(),
                names: Vec::new(),
                last_ip: None,
                ips: Vec::new(),
                times_joined: 0,
                times_kicked: 1,
                last_kicked_at: expiry,
                last_seen_at: now,
            });
        }
    }
}

/// Kicks `target` out of `match_id`: member row disconnect, ordered kick record,
/// history counters and a `PlayerKick` event. Shared by `admin_kick`, the
/// votekick pass path and the anti-spam/action filters.
pub fn apply_kick(
    ctx: &ReducerContext,
    match_id: u64,
    target: Identity,
    by: Identity,
    reason: &str,
    duration_secs: u64,
) -> Result<(), String> {
    let member = ctx
        .db
        .relay_member()
        .by_match_identity()
        .filter((match_id, target))
        .next()
        .ok_or_else(|| "target is not a member of this match".to_string())?;
    ctx.db
        .relay_member()
        .member_id()
        .update(crate::relay::tables::RelayMember {
            kicked_reason: Some(reason.to_string()),
            connected: false,
            ..member
        });
    ctx.db.match_kick().insert(MatchKick {
        kick_id: 0,
        match_id,
        target,
        reason: reason.to_string(),
        by,
        at: ctx.timestamp,
    });
    record_kick(ctx, target, duration_secs);
    crate::relay::events::emit(
        ctx,
        match_id,
        crate::relay::events::EventKind::PlayerKick,
        Some(by),
        Some(target),
        reason,
    );
    Ok(())
}

/// Seconds left on an identity's kick cooldown, if any.
pub fn kick_cooldown_secs(ctx: &ReducerContext, identity: Identity) -> Option<u64> {
    let info = ctx.db.player_info().identity().find(identity)?;
    let expires = info.last_kicked_at?;
    let left = expires
        .to_micros_since_unix_epoch()
        .saturating_sub(ctx.timestamp.to_micros_since_unix_epoch());
    if left > 0 {
        Some((left / 1_000_000) as u64)
    } else {
        None
    }
}

/// Java `Administration.findByName`: exact current/any name, colour-stripped
/// name, IP (empty under D2) or UUID match.
pub fn find_by_name(infos: &[PlayerInfo], query: &str) -> Vec<Identity> {
    let stripped = strip_colors(query);
    infos
        .iter()
        .filter(|info| {
            info.last_name.eq_ignore_ascii_case(query)
                || info
                    .names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(query))
                || strip_colors(&info.last_name).eq_ignore_ascii_case(&stripped)
                || info.ips.iter().any(|ip| ip.eq_ignore_ascii_case(query))
                || info
                    .identity
                    .to_hex()
                    .to_string()
                    .eq_ignore_ascii_case(query)
        })
        .map(|info| info.identity)
        .collect()
}

/// Java `Administration.searchNames`: case-insensitive substring over every
/// recorded name (colour tags ignored).
pub fn search_names(infos: &[PlayerInfo], query: &str) -> Vec<Identity> {
    let needle = strip_colors(query).to_lowercase();
    infos
        .iter()
        .filter(|info| {
            info.names.iter().any(|name| {
                name.to_lowercase().contains(&needle)
                    || strip_colors(name).to_lowercase().contains(&needle)
            })
        })
        .map(|info| info.identity)
        .collect()
}

/// Validates a name-ban regex (Java `Administration.addNameBan`).
pub fn validate_name_pattern(pattern: &str) -> Result<(), String> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME_PATTERN_LEN {
        return Err(format!(
            "name-ban pattern must be 1-{MAX_NAME_PATTERN_LEN} characters"
        ));
    }
    RegexBuilder::new(trimmed)
        .case_insensitive(true)
        .build()
        .map_err(|error| format!("invalid name-ban regex: {error}"))?;
    Ok(())
}

/// Whether `name` matches any configured name-ban regex
/// (Java `Administration.isNameBanned`, `Administration.java:407`).
///
/// Invalid persisted patterns are skipped rather than failing the join gate;
/// `admin_ban_name` validates before inserting, so this only covers rows edited
/// through a raw config write.
pub fn name_banned(patterns: &[String], name: &str) -> bool {
    patterns.iter().any(|pattern| {
        RegexBuilder::new(pattern)
            .case_insensitive(true)
            .build()
            .map(|regex| regex.is_match(name))
            .unwrap_or(false)
    })
}

/// Adds one name-ban regex (admin only, Java `addNameBan`).
#[reducer]
pub fn admin_ban_name(ctx: &ReducerContext, pattern: String) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::BanName)?;
    validate_name_pattern(&pattern)?;
    admin_rate_check(ctx)?;
    let config = server_config_or_default(ctx);
    let pattern = pattern.trim().to_string();
    if config
        .banned_name_patterns
        .iter()
        .any(|known| known == &pattern)
    {
        return Err("name-ban pattern already exists".to_string());
    }
    let mut patterns = config.banned_name_patterns.clone();
    patterns.push(pattern.clone());
    ctx.db.server_config().id().update(ServerConfig {
        banned_name_patterns: patterns,
        ..config
    });
    record(
        ctx,
        AdminActionKind::BanName,
        None,
        None,
        format!("pattern={pattern}"),
    );
    Ok(())
}

/// Removes one name-ban regex (admin only).
#[reducer]
pub fn admin_unban_name(ctx: &ReducerContext, pattern: String) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::UnbanName)?;
    admin_rate_check(ctx)?;
    let config = server_config_or_default(ctx);
    let before = config.banned_name_patterns.len();
    let patterns: Vec<String> = config
        .banned_name_patterns
        .iter()
        .filter(|known| **known != pattern)
        .cloned()
        .collect();
    if patterns.len() == before {
        return Err(format!("unknown name-ban pattern `{pattern}`"));
    }
    let removed = before - patterns.len();
    ctx.db.server_config().id().update(ServerConfig {
        banned_name_patterns: patterns,
        ..config
    });
    record(
        ctx,
        AdminActionKind::UnbanName,
        None,
        None,
        format!("pattern={pattern} removed={removed}"),
    );
    Ok(())
}

/// Answers `adminRequest(trace)` (Java `NetServer.java:937-945`): an admin
/// requests a target's history, the module writes a targeted `player_trace` row.
#[reducer]
pub fn admin_request_trace(
    ctx: &ReducerContext,
    match_id: Option<u64>,
    target: Identity,
) -> Result<(), String> {
    admin_gate(is_admin(ctx), false, AdminActionKind::RequestTrace)?;
    if ctx.db.player().identity().find(target).is_none()
        && ctx.db.player_info().identity().find(target).is_none()
    {
        return Err("unknown player".to_string());
    }
    admin_rate_check(ctx)?;
    let info = ctx.db.player_info().identity().find(target);
    let locale = ctx
        .db
        .client_settings()
        .identity()
        .find(target)
        .map(|settings| settings.language);
    ctx.db.player_trace().insert(PlayerTrace {
        trace_id: 0,
        requested_by: ctx.sender(),
        target,
        match_id,
        ip: info.as_ref().and_then(|row| row.last_ip.clone()),
        locale,
        times_joined: info.as_ref().map(|row| row.times_joined).unwrap_or(0),
        times_kicked: info.as_ref().map(|row| row.times_kicked).unwrap_or(0),
        ips: info.as_ref().map(|row| row.ips.clone()).unwrap_or_default(),
        names: info
            .as_ref()
            .map(|row| row.names.clone())
            .unwrap_or_default(),
        requested_at: ctx.timestamp,
    });
    record(
        ctx,
        AdminActionKind::RequestTrace,
        Some(target),
        match_id,
        "trace requested",
    );
    Ok(())
}

/// Every recorded player history, visible to admins only.
#[view(accessor = all_player_info, public)]
fn all_player_info(ctx: &ViewContext) -> Vec<PlayerInfo> {
    if !view_is_admin(ctx) {
        return Vec::new();
    }
    ctx.db
        .player_info()
        .by_player_info_seen()
        .filter((Bound::<Timestamp>::Unbounded, Bound::<Timestamp>::Unbounded))
        .collect()
}

/// Trace payloads requested by the caller (admin only; the maintenance sweep
/// deletes responses past `PLAYER_TRACE_RETENTION_SECS`, views have no clock).
#[view(accessor = my_traces, public)]
fn my_traces(ctx: &ViewContext) -> Vec<PlayerTrace> {
    if !view_is_admin(ctx) {
        return Vec::new();
    }
    ctx.db
        .player_trace()
        .by_trace_requester()
        .filter(ctx.sender())
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn info(identity: u8, last: &str, names: &[&str]) -> PlayerInfo {
        let mut bytes = [0u8; 32];
        bytes[0] = identity;
        PlayerInfo {
            identity: Identity::from_byte_array(bytes),
            last_name: last.to_string(),
            names: names.iter().map(|name| name.to_string()).collect(),
            last_ip: None,
            ips: Vec::new(),
            times_joined: 1,
            times_kicked: 0,
            last_kicked_at: None,
            last_seen_at: Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn strip_colours() {
        assert_eq!(strip_colors("[scarlet]hello[white]"), "hello");
        assert_eq!(strip_colors("[]empty"), "empty");
        assert_eq!(strip_colors("no tags"), "no tags");
        assert_eq!(strip_colors("[unclosed"), "[unclosed");
        assert_eq!(strip_colors("[#ff0000]red"), "red");
    }

    #[test]
    fn find_by_name_markers() {
        let rows = [
            info(1, "[red]Builder", &["Builder", "[red]Builder"]),
            info(2, "Alice", &["alice", "Builder"]),
            info(3, "Bob", &["Bob"]),
        ];
        assert_eq!(
            find_by_name(&rows, "Builder"),
            vec![rows[0].identity, rows[1].identity]
        );
        assert_eq!(
            find_by_name(&rows, "builder"),
            vec![rows[0].identity, rows[1].identity]
        );
        assert_eq!(find_by_name(&rows, "alice"), vec![rows[1].identity]);
        assert!(find_by_name(&rows, "nobody").is_empty());
        // UUID lookup.
        let hex = rows[2].identity.to_hex().to_string();
        assert_eq!(find_by_name(&rows, &hex), vec![rows[2].identity]);
    }

    #[test]
    fn search_names_substring() {
        let rows = [
            info(1, "MegaBuilder", &["MegaBuilder", "mb"]),
            info(2, "Alice", &["Alice"]),
        ];
        assert_eq!(search_names(&rows, "builder"), vec![rows[0].identity]);
        assert_eq!(search_names(&rows, "MB"), vec![rows[0].identity]);
        assert!(search_names(&rows, "zzz").is_empty());
    }

    #[test]
    fn name_ban_patterns() {
        assert!(name_banned(&["admin".to_string()], "The ADMIN"));
        assert!(name_banned(&["^kick".to_string()], "kicker"));
        assert!(name_banned(&["\\d{3}".to_string()], "user123"));
        assert!(!name_banned(&["admin".to_string()], "player"));
        // Invalid persisted patterns are skipped, not fatal.
        assert!(!name_banned(&["[".to_string()], "anything"));
        assert!(validate_name_pattern("^[a-z]+$").is_ok());
        assert!(validate_name_pattern("(").is_err());
        assert!(validate_name_pattern("").is_err());
        assert!(validate_name_pattern(&"x".repeat(MAX_NAME_PATTERN_LEN + 1)).is_err());
    }

    #[test]
    fn names_are_deduped_and_bounded() {
        let mut names = Vec::new();
        push_name(&mut names, "Builder");
        push_name(&mut names, "builder");
        assert_eq!(names, vec!["Builder".to_string()]);
        for index in 0..(MAX_PLAYER_NAMES + 5) {
            push_name(&mut names, &format!("name{index}"));
        }
        assert_eq!(names.len(), MAX_PLAYER_NAMES);
    }

    #[test]
    fn ips_are_bounded() {
        let mut ips = Vec::new();
        for index in 0..(MAX_PLAYER_IPS + 2) {
            push_ip(&mut ips, &format!("10.0.0.{index}"));
        }
        assert_eq!(ips.len(), MAX_PLAYER_IPS);
    }
}
