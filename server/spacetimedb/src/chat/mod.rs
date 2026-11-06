// SPDX-License-Identifier: GPL-3.0-only

//! Match chat (plan 21 §3.6/§6.1/§6.6; PAR-0121/PAR-0125).
//!
//! `send_chat` mirrors the Java server path (`NetClient.sendChatMessage`,
//! `NetClient.java:333-402`): it rate-gates the connection, parses `/` commands
//! *before* the chat filters, dispatches `/t`, `/a`, `/votekick`, `/vote`,
//! `/help` and `/sync`, then sanitizes, filters and appends the message.
//!
//! Scope is per match (PAR-0125): `chat_filter.match_id` (`None` = server-wide)
//! plus the per-(match, identity) `match_chat_state` holding filter infractions
//! and mute expiry, so a mute earned in match A never silences match B. The
//! legacy database-global `chat_rate.muted_until`/`infractions` columns stay for
//! ABI compatibility but are no longer consulted.
//!
//! Every accepted message and every server notice is a `match_chat` row, so the
//! client transport has a real sink: `my_match_chat` converges on the whole
//! visible conversation. `EventKind::PlayerChat` is published on the event bus
//! for the client `EventType` layer (GAPS #37).

pub mod commands;
pub mod vote;

use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp, reducer, table};

use crate::admin::server_config_or_default;
use crate::main::global::{
    CHAT_DUPLICATE_WINDOW_SECS, CHAT_INFRACTION_LIMIT, CHAT_MUTE_SECS, MAX_SYSTEM_TEXT, MAX_TEXT,
    VOTE_MIN_PLAYERS,
};
use crate::relay::events::{EventKind, emit};
use crate::relay::methods::{
    is_admin, is_host_or_admin, require_match, require_member, window_expired,
};
use crate::relay::tables::{MatchStatus, RelayMatch, RelayMember, relay_member};

use commands::{ChatCommand, help_page, parse_chat_command};

/// Chat channel (plan §6.6; variant names are ABI, append-only).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChatKind {
    /// Broadcast to every match member.
    All,
    /// Broadcast to the sender's team.
    Team,
    /// Server/host notice (rendered distinctly).
    System,
    /// Admins only (`/a`, GAPS #33).
    Admin,
}

/// One chat message (plan §6.1).
#[table(accessor = match_chat, public, index(accessor = by_chat_match, btree(columns = [match_id, chat_id])))]
pub struct MatchChat {
    /// Auto-inc message id.
    #[primary_key]
    #[auto_inc]
    pub chat_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Author (`System` messages keep the causing/recipient identity).
    pub sender: Identity,
    /// Channel.
    pub kind: ChatKind,
    /// Team id for `Team` chat.
    pub team: Option<u8>,
    /// Sanitized message text (≤ [`MAX_TEXT`] for player chat, ≤
    /// [`MAX_SYSTEM_TEXT`] for notices).
    pub text: String,
    /// Server receive time.
    pub sent_at: Timestamp,
    /// Private-delivery target (`None` = everyone the channel allows).
    ///
    /// Appended with a default so existing deployments migrate additively.
    #[default(None)]
    pub target: Option<Identity>,
}

/// Server-only per-identity chat rate window (plan §6.1).
///
/// This is the per-connection spam gate (`player.con.chatRate` in Java), which
/// stays server-wide on purpose. Per-match mutes live in [`MatchChatState`].
#[table(accessor = chat_rate)]
pub struct ChatRate {
    /// Caller.
    #[primary_key]
    pub identity: Identity,
    /// Current window start.
    pub window_start: Timestamp,
    /// Posts in the window.
    pub posts: u32,
    /// Lifetime filter infractions (legacy; per-match state is authoritative).
    pub infractions: u32,
    /// Mute expiry (legacy; per-match state is authoritative).
    pub muted_until: Option<Timestamp>,
}

/// Per-(match, identity) filter infractions, mute expiry and spam state
/// (PAR-0125: mutes are scoped to the room they were earned in).
#[table(accessor = match_chat_state, index(accessor = by_chat_state, btree(columns = [match_id, identity])))]
pub struct MatchChatState {
    /// Auto-inc row id (single-column primary keys only).
    #[primary_key]
    #[auto_inc]
    pub state_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Caller.
    pub identity: Identity,
    /// Filter infractions in this match.
    pub infractions: u32,
    /// Mute expiry in this match.
    pub muted_until: Option<Timestamp>,
    /// Last accepted message time (anti-spam `lastMessageTime`).
    pub last_message_at: Option<Timestamp>,
    /// Last accepted message text (duplicate suppression).
    pub last_message: String,
    /// Anti-spam infractions (`PlayerInfo.messageInfractions`).
    pub spam_infractions: u32,
    /// Last touch, for idle pruning.
    pub updated_at: Timestamp,
}

/// Chat filter (plan §6.1; admin-managed).
#[table(accessor = chat_filter, public, index(accessor = by_chat_filter_added, btree(columns = [added_at])))]
pub struct ChatFilter {
    /// Auto-inc filter id.
    #[primary_key]
    #[auto_inc]
    pub filter_id: u64,
    /// Lowercase substring pattern.
    pub pattern: String,
    /// Whether a match mutes (drops) the message.
    pub mute: bool,
    /// Admin who added it.
    pub added_by: Identity,
    /// When it was added.
    pub added_at: Timestamp,
    /// Match scope: `None` = database-global, `Some` = that match only
    /// (PAR-0125; appended with a default so existing rows stay valid).
    #[default(None)]
    pub match_id: Option<u64>,
}

/// Sends one chat message (plan §3.6/§6.7).
#[reducer]
pub fn send_chat(
    ctx: &ReducerContext,
    match_id: u64,
    kind: ChatKind,
    text: String,
) -> Result<(), String> {
    let result = send_chat_impl(ctx, match_id, kind, text);
    if let Err(error) = &result {
        log::warn!("send_chat rejected for match {match_id}: {error}");
    }
    result
}

fn send_chat_impl(
    ctx: &ReducerContext,
    match_id: u64,
    kind: ChatKind,
    text: String,
) -> Result<(), String> {
    let row = require_match(ctx, match_id)?;
    if row.status == MatchStatus::Ended {
        return Err(format!("match {match_id} has ended"));
    }
    let member = require_member(ctx, match_id)?;
    // Java checks `player.con.chatRate.allow(2000, chatSpamLimit)` before any
    // command handling, so commands are rate-gated too.
    if !rate_gate(ctx, match_id)? {
        return Ok(());
    }
    match parse_chat_command(&text) {
        ChatCommand::None => {}
        ChatCommand::Team(message) => {
            return send_channel(ctx, &row, &member, ChatKind::Team, message);
        }
        ChatCommand::Admin(message) => {
            return send_channel(ctx, &row, &member, ChatKind::Admin, message);
        }
        ChatCommand::Votekick { target, reason } => {
            handle_votekick(ctx, &row, target, reason);
            return Ok(());
        }
        ChatCommand::Vote(choice) => {
            let Some(choice) = choice else {
                private_notice(
                    ctx,
                    match_id,
                    ctx.sender(),
                    "Vote either 'y' (yes) or 'n' (no).",
                );
                return Ok(());
            };
            if let Err(message) = vote::cast_vote(ctx, match_id, choice) {
                private_notice(ctx, match_id, ctx.sender(), message);
            }
            return Ok(());
        }
        ChatCommand::Help(page) => {
            let body = help_page(page.unwrap_or(1)).unwrap_or_else(|message| message);
            private_notice(ctx, match_id, ctx.sender(), body);
            return Ok(());
        }
        ChatCommand::Sync => {
            private_notice(
                ctx,
                match_id,
                ctx.sender(),
                "World state re-synchronization is requested from the host.",
            );
            emit(
                ctx,
                match_id,
                EventKind::SyncRequest,
                Some(ctx.sender()),
                None,
                "",
            );
            return Ok(());
        }
        ChatCommand::Invalid(message) => {
            private_notice(ctx, match_id, ctx.sender(), message);
            return Ok(());
        }
    }
    send_channel(ctx, &row, &member, kind, text)
}

/// Appends one message on `kind`, applying the channel/filter/anti-spam rules.
fn send_channel(
    ctx: &ReducerContext,
    row: &RelayMatch,
    member: &RelayMember,
    kind: ChatKind,
    text: String,
) -> Result<(), String> {
    let match_id = row.match_id;
    if kind == ChatKind::System && !is_host_or_admin(ctx, row) {
        return Err("only the host or an admin may send system chat".to_string());
    }
    if kind == ChatKind::Admin && !is_admin(ctx) {
        private_notice(
            ctx,
            match_id,
            ctx.sender(),
            "You must be an admin to use this command.",
        );
        return Ok(());
    }
    let sanitized = sanitize_text(&text);
    if sanitized.is_empty() {
        return Err("chat message is empty".to_string());
    }
    let team = if kind == ChatKind::Team {
        Some(
            member
                .team
                .ok_or_else(|| "team chat requires a team".to_string())?,
        )
    } else {
        None
    };
    // Java's built-in anti-spam filter runs inside `filterMessage`, so `/t`
    // gets it too; `/a`, server notices and internal relays do not.
    if matches!(kind, ChatKind::All | ChatKind::Team) {
        if !message_spam_gate(ctx, match_id, &sanitized)? {
            return Ok(());
        }
        if !filter_gate(ctx, match_id, &sanitized)? {
            return Ok(());
        }
    }
    ctx.db.match_chat().insert(MatchChat {
        chat_id: 0,
        match_id,
        sender: ctx.sender(),
        kind,
        team,
        text: sanitized.clone(),
        sent_at: ctx.timestamp,
        target: None,
    });
    emit(
        ctx,
        match_id,
        EventKind::PlayerChat,
        Some(ctx.sender()),
        None,
        sanitized,
    );
    Ok(())
}

/// Handles `/votekick` eligibility and dispatch (Java `NetServer.java:394-475`).
fn handle_votekick(ctx: &ReducerContext, row: &RelayMatch, target: Option<String>, reason: String) {
    let match_id = row.match_id;
    let config = server_config_or_default(ctx);
    if !config.enable_votekick {
        private_notice(
            ctx,
            match_id,
            ctx.sender(),
            "Vote-kick is disabled on this server.",
        );
        return;
    }
    if vote::member_count(ctx, match_id) < VOTE_MIN_PLAYERS {
        private_notice(
            ctx,
            match_id,
            ctx.sender(),
            format!("At least {VOTE_MIN_PLAYERS} players are needed to start a votekick."),
        );
        return;
    }
    let Some(target_name) = target else {
        let mut list = String::from("Players to kick: \n");
        for member in ctx
            .db
            .relay_member()
            .by_match_identity()
            .filter(match_id)
            .collect::<Vec<_>>()
        {
            if member.identity == ctx.sender()
                || !member.connected
                || crate::relay::methods::is_admin_for(ctx, member.identity)
            {
                continue;
            }
            list.push_str(&format!(
                " {} (#{})\n",
                vote::player_name(ctx, member.identity),
                member.member_id
            ));
        }
        private_notice(ctx, match_id, ctx.sender(), list);
        return;
    };
    if reason.trim().is_empty() {
        private_notice(
            ctx,
            match_id,
            ctx.sender(),
            "You need a valid reason to kick the player. Add a reason after the player name.",
        );
        return;
    }
    let Some(target_member) = resolve_member(ctx, match_id, &target_name) else {
        private_notice(
            ctx,
            match_id,
            ctx.sender(),
            format!("No player '{target_name}' found."),
        );
        return;
    };
    if let Err(message) = vote::start_vote(ctx, row, target_member, &reason) {
        private_notice(ctx, match_id, ctx.sender(), message);
    }
}

/// Resolves `#member_id` (Java `#id`) or a current username among match members.
fn resolve_member(ctx: &ReducerContext, match_id: u64, name: &str) -> Option<RelayMember> {
    if let Some(raw) = name.strip_prefix('#')
        && let Ok(member_id) = raw.parse::<u64>()
        && let Some(member) = ctx.db.relay_member().member_id().find(member_id)
        && member.match_id == match_id
    {
        return Some(member);
    }
    ctx.db
        .relay_member()
        .by_match_identity()
        .filter(match_id)
        .find(|member| vote::player_name(ctx, member.identity).eq_ignore_ascii_case(name))
}

/// Per-match chat state, created on demand.
fn chat_state_for(ctx: &ReducerContext, match_id: u64) -> MatchChatState {
    ctx.db
        .match_chat_state()
        .by_chat_state()
        .filter((match_id, ctx.sender()))
        .next()
        .unwrap_or(MatchChatState {
            state_id: 0,
            match_id,
            identity: ctx.sender(),
            infractions: 0,
            muted_until: None,
            last_message_at: None,
            last_message: String::new(),
            spam_infractions: 0,
            updated_at: ctx.timestamp,
        })
}

/// Upserts the caller's per-match chat state.
fn save_chat_state(ctx: &ReducerContext, state: MatchChatState) {
    let existing = ctx
        .db
        .match_chat_state()
        .by_chat_state()
        .filter((state.match_id, state.identity))
        .next();
    match existing {
        Some(existing) => {
            ctx.db.match_chat_state().state_id().update(MatchChatState {
                state_id: existing.state_id,
                ..state
            });
        }
        None => {
            ctx.db.match_chat_state().insert(MatchChatState {
                state_id: 0,
                ..state
            });
        }
    }
}

fn now_micros(ctx: &ReducerContext) -> i64 {
    ctx.timestamp.to_micros_since_unix_epoch()
}

fn within(timestamp: Option<Timestamp>, now: i64, window_secs: u64) -> bool {
    timestamp
        .map(|at| {
            now.saturating_sub(at.to_micros_since_unix_epoch()) < (window_secs as i64) * 1_000_000
        })
        .unwrap_or(false)
}

/// The generic chat rate window (`player.con.chatRate`); `false` drops the
/// message after a private notice.
fn rate_gate(ctx: &ReducerContext, match_id: u64) -> Result<bool, String> {
    let config = server_config_or_default(ctx);
    let now = ctx.timestamp;
    let now_micros = now.to_micros_since_unix_epoch();
    let existing = ctx.db.chat_rate().identity().find(ctx.sender());
    let mut rate = match &existing {
        Some(existing) => ChatRate {
            identity: existing.identity,
            window_start: existing.window_start,
            posts: existing.posts,
            infractions: existing.infractions,
            muted_until: existing.muted_until,
        },
        None => ChatRate {
            identity: ctx.sender(),
            window_start: now,
            posts: 0,
            infractions: 0,
            muted_until: None,
        },
    };
    if window_expired(
        now_micros,
        rate.window_start.to_micros_since_unix_epoch(),
        config.chat_rate_window_ms,
    ) {
        rate.window_start = now;
        rate.posts = 0;
    }
    if rate.posts >= config.chat_rate_max {
        if existing.is_some() {
            ctx.db.chat_rate().identity().update(rate);
        } else {
            ctx.db.chat_rate().insert(rate);
        }
        private_notice(
            ctx,
            match_id,
            ctx.sender(),
            "You are sending messages too quickly.",
        );
        return Ok(false);
    }
    rate.posts = rate.posts.saturating_add(1);
    if existing.is_some() {
        ctx.db.chat_rate().identity().update(rate);
    } else {
        ctx.db.chat_rate().insert(rate);
    }
    Ok(true)
}

/// Built-in anti-spam filter (`Administration` constructor): message rate,
/// duplicate suppression and `messageSpamKick`. Returns `false` to drop.
fn message_spam_gate(ctx: &ReducerContext, match_id: u64, text: &str) -> Result<bool, String> {
    let config = server_config_or_default(ctx);
    if !config.anti_spam || is_admin(ctx) {
        return Ok(true);
    }
    let state = chat_state_for(ctx, match_id);
    let now = now_micros(ctx);
    if config.message_rate_limit_secs > 0
        && within(
            state.last_message_at,
            now,
            u64::from(config.message_rate_limit_secs),
        )
    {
        let infractions = state.spam_infractions.saturating_add(1);
        save_chat_state(
            ctx,
            MatchChatState {
                spam_infractions: infractions,
                updated_at: ctx.timestamp,
                ..state
            },
        );
        private_notice(
            ctx,
            match_id,
            ctx.sender(),
            format!(
                "You may only send messages every {} seconds.",
                config.message_rate_limit_secs
            ),
        );
        if config.message_spam_kick != 0 && infractions >= config.message_spam_kick {
            private_notice(
                ctx,
                match_id,
                ctx.sender(),
                "You have been kicked for spamming.",
            );
            let _ = crate::admin::history::apply_kick(
                ctx,
                match_id,
                ctx.sender(),
                ctx.sender(),
                "You have been kicked for spamming.",
                crate::main::global::MESSAGE_SPAM_KICK_SECS,
            );
        }
        return Ok(false);
    }
    if state.last_message == text && within(state.last_message_at, now, CHAT_DUPLICATE_WINDOW_SECS)
    {
        save_chat_state(
            ctx,
            MatchChatState {
                spam_infractions: 0,
                updated_at: ctx.timestamp,
                ..state
            },
        );
        private_notice(
            ctx,
            match_id,
            ctx.sender(),
            "You may not send the same message twice.",
        );
        return Ok(false);
    }
    save_chat_state(
        ctx,
        MatchChatState {
            last_message: text.to_string(),
            last_message_at: Some(ctx.timestamp),
            spam_infractions: 0,
            updated_at: ctx.timestamp,
            ..state
        },
    );
    Ok(true)
}

/// Persisted chat-filter pass: silent drop on a mute pattern, escalates to a
/// [`CHAT_MUTE_SECS`] per-match mute after [`CHAT_INFRACTION_LIMIT`] hits.
fn filter_gate(ctx: &ReducerContext, match_id: u64, text: &str) -> Result<bool, String> {
    let patterns = filters_for_match(ctx, match_id);
    let state = chat_state_for(ctx, match_id);
    if filter_mutes(text, &patterns) {
        let infractions = state.infractions.saturating_add(1);
        let muted_until = if infractions >= CHAT_INFRACTION_LIMIT {
            Some(
                ctx.timestamp
                    + spacetimedb::TimeDuration::from_micros(
                        (CHAT_MUTE_SECS.saturating_mul(1_000_000)) as i64,
                    ),
            )
        } else {
            state.muted_until
        };
        save_chat_state(
            ctx,
            MatchChatState {
                infractions,
                muted_until,
                updated_at: ctx.timestamp,
                ..state
            },
        );
        return Ok(false);
    }
    if let Some(until) = state.muted_until
        && until.to_micros_since_unix_epoch() > now_micros(ctx)
    {
        let remaining = (until.to_micros_since_unix_epoch() - now_micros(ctx)) / 1_000_000;
        private_notice(
            ctx,
            match_id,
            ctx.sender(),
            format!("You are muted for another {} second(s).", remaining.max(1)),
        );
        return Ok(false);
    }
    Ok(true)
}

/// Every filter pattern that applies to `match_id` (global + scoped).
pub fn filters_for_match(ctx: &ReducerContext, match_id: u64) -> Vec<(String, bool)> {
    ctx.db
        .chat_filter()
        .iter()
        .filter(|filter| filter.match_id.is_none() || filter.match_id == Some(match_id))
        .map(|filter| (filter.pattern, filter.mute))
        .collect()
}

/// Appends one server notice visible to the whole match.
pub fn broadcast_system(
    ctx: &ReducerContext,
    match_id: u64,
    sender: Identity,
    text: impl Into<String>,
) {
    let text = sanitize_system_text(&text.into());
    if text.is_empty() {
        return;
    }
    ctx.db.match_chat().insert(MatchChat {
        chat_id: 0,
        match_id,
        sender,
        kind: ChatKind::System,
        team: None,
        text,
        sent_at: ctx.timestamp,
        target: None,
    });
}

/// Appends one server notice visible only to `target` (Java `player.sendMessage`).
pub fn private_notice(
    ctx: &ReducerContext,
    match_id: u64,
    target: Identity,
    text: impl Into<String>,
) {
    let text = sanitize_system_text(&text.into());
    if text.is_empty() {
        return;
    }
    ctx.db.match_chat().insert(MatchChat {
        chat_id: 0,
        match_id,
        sender: target,
        kind: ChatKind::System,
        team: None,
        text,
        sent_at: ctx.timestamp,
        target: Some(target),
    });
}

/// Whether `viewer` may see a chat row (pure; mirrors `my_match_chat`).
pub fn chat_visible(
    kind: ChatKind,
    chat_team: Option<u8>,
    viewer_team: Option<u8>,
    viewer_admin: bool,
    target: Option<Identity>,
    viewer: Identity,
) -> bool {
    if let Some(target) = target
        && target != viewer
    {
        return false;
    }
    match kind {
        ChatKind::All | ChatKind::System => true,
        ChatKind::Team => chat_team.is_some() && chat_team == viewer_team,
        ChatKind::Admin => viewer_admin,
    }
}

/// Strips control characters except newlines/tabs, trims and caps notices.
pub fn sanitize_system_text(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect();
    cleaned
        .trim()
        .chars()
        .take(MAX_SYSTEM_TEXT)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Strips newlines/control characters, trims, and caps to [`MAX_TEXT`].
///
/// Upstream `sendChatMessage` strips `\n`; `maxTextLength` is 150. Control
/// characters other than tab are dropped so rendered labels stay single-line.
pub fn sanitize_text(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .filter(|c| *c != '\n' && *c != '\r' && (*c == '\t' || !c.is_control()))
        .collect();
    let trimmed = cleaned.trim();
    trimmed.chars().take(MAX_TEXT).collect()
}

/// Whether `text` matches any mute filter (case-insensitive substring).
pub fn filter_mutes(text: &str, patterns: &[(String, bool)]) -> bool {
    let lowered = text.to_lowercase();
    patterns
        .iter()
        .any(|(pattern, mute)| *mute && !pattern.is_empty() && lowered.contains(pattern))
}

/// Pure chat rate decision (window start/count + max).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateDecision {
    /// The message may be posted; the rolled window/count are returned.
    Allow {
        /// Whether the window rolled.
        rolled: bool,
        /// Posts after this message.
        posts: u32,
    },
    /// The window is full.
    Exceeded,
}

/// Applies the chat rate window purely (no DB), mirroring the reducer.
pub fn rate_limit(
    window_start_micros: i64,
    now_micros: i64,
    posts: u32,
    window_ms: u32,
    max: u32,
) -> RateDecision {
    if window_expired(now_micros, window_start_micros, window_ms) {
        return RateDecision::Allow {
            rolled: true,
            posts: 1,
        };
    }
    if posts >= max {
        RateDecision::Exceeded
    } else {
        RateDecision::Allow {
            rolled: false,
            posts: posts + 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_cap() {
        assert_eq!(sanitize_text("hello"), "hello");
        assert_eq!(sanitize_text(" a\nb\r c "), "ab c");
        assert_eq!(sanitize_text("a\nb").len(), 2);
        let long = "x".repeat(MAX_TEXT + 50);
        assert_eq!(sanitize_text(&long).chars().count(), MAX_TEXT);
        // Tabs are preserved; other control chars are dropped.
        assert_eq!(sanitize_text("a\tb"), "a\tb");
        assert_eq!(sanitize_text("a\u{0}b"), "ab");
        assert!(sanitize_text("   ").is_empty());
    }

    #[test]
    fn system_text_keeps_newlines() {
        assert_eq!(sanitize_system_text("a\nb"), "a\nb");
        assert_eq!(sanitize_system_text("a\u{0}b"), "ab");
        assert_eq!(sanitize_system_text("  x  "), "x");
        let long = "y".repeat(MAX_SYSTEM_TEXT + 10);
        assert_eq!(sanitize_system_text(&long).chars().count(), MAX_SYSTEM_TEXT);
    }

    #[test]
    fn filter_mutes() {
        let filters = vec![("badword".to_string(), true), ("ok".to_string(), false)];
        assert!(super::filter_mutes("this has a BADWORD inside", &filters));
        assert!(!super::filter_mutes("this is fine", &filters));
        // A non-mute filter never drops.
        assert!(!super::filter_mutes("ok then", &filters));
        // Empty patterns never match.
        assert!(!super::filter_mutes("anything", &[("".to_string(), true)]));
    }

    #[test]
    fn rate_limit_window() {
        // First post in an empty window is allowed.
        assert_eq!(
            rate_limit(0, 0, 0, 2_000, 20),
            RateDecision::Allow {
                rolled: false,
                posts: 1
            }
        );
        // Same window at the cap is rejected.
        assert_eq!(rate_limit(0, 1_000, 20, 2_000, 20), RateDecision::Exceeded);
        // Window expiry rolls and resets the count.
        assert_eq!(
            rate_limit(0, 2_000_000, 20, 2_000, 20),
            RateDecision::Allow {
                rolled: true,
                posts: 1
            }
        );
    }

    #[test]
    fn chat_visibility_matches_channels() {
        let viewer = Identity::from_byte_array([1u8; 32]);
        let other = Identity::from_byte_array([2u8; 32]);
        assert!(chat_visible(ChatKind::All, None, None, false, None, viewer));
        assert!(chat_visible(
            ChatKind::System,
            None,
            Some(1),
            false,
            None,
            viewer
        ));
        assert!(chat_visible(
            ChatKind::Team,
            Some(1),
            Some(1),
            false,
            None,
            viewer
        ));
        assert!(!chat_visible(
            ChatKind::Team,
            Some(1),
            Some(2),
            false,
            None,
            viewer
        ));
        assert!(chat_visible(
            ChatKind::Admin,
            None,
            None,
            true,
            None,
            viewer
        ));
        assert!(!chat_visible(
            ChatKind::Admin,
            None,
            None,
            false,
            None,
            viewer
        ));
        // Private rows are only visible to their target.
        assert!(chat_visible(
            ChatKind::System,
            None,
            None,
            false,
            Some(viewer),
            viewer
        ));
        assert!(!chat_visible(
            ChatKind::System,
            None,
            None,
            false,
            Some(other),
            viewer
        ));
    }
}
