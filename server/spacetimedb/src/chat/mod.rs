// SPDX-License-Identifier: GPL-3.0-only

//! Match chat (plan 21 §3.6/§6.1/§6.6).
//!
//! `send_chat` sanitizes text, applies the global substring filter and the
//! server-side rate window, then inserts a `match_chat` row. Team chat requires
//! a team; `System` is host/admin only. Filters and rate state are global
//! (documented limitation R-21-10), not per-match.

use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp, reducer, table};

use crate::admin::server_config_or_default;
use crate::main::global::MAX_TEXT;
use crate::relay::methods::{is_host_or_admin, require_match, require_member, window_expired};
use crate::relay::tables::MatchStatus;

/// Chat channel (plan §6.6; variant names are ABI).
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChatKind {
    /// Broadcast to every match member.
    All,
    /// Broadcast to the sender's team.
    Team,
    /// Host/admin broadcast (rendered distinctly).
    System,
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
    /// Author (`System` messages keep the host/admin sender).
    pub sender: Identity,
    /// Channel.
    pub kind: ChatKind,
    /// Team id for `Team` chat.
    pub team: Option<u8>,
    /// Sanitized message text (≤ [`MAX_TEXT`]).
    pub text: String,
    /// Server receive time.
    pub sent_at: Timestamp,
}

/// Server-only per-identity chat rate/mute state (plan §6.1).
#[table(accessor = chat_rate)]
pub struct ChatRate {
    /// Caller.
    #[primary_key]
    pub identity: Identity,
    /// Current window start.
    pub window_start: Timestamp,
    /// Posts in the window.
    pub posts: u32,
    /// Lifetime filter infractions.
    pub infractions: u32,
    /// Mute expiry, if muted.
    pub muted_until: Option<Timestamp>,
}

/// Global lowercase-substring chat filter (plan §6.1; admin-managed in M6).
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
    let sanitized = sanitize_text(&text);
    if sanitized.is_empty() {
        return Err("chat message is empty".to_string());
    }
    if kind == ChatKind::System && !is_host_or_admin(ctx, &row) {
        return Err("only the host or an admin may send system chat".to_string());
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

    // Filter pass before insert. A muted match drops the message and counts an
    // infraction; upstream auto-kicks after three spam infractions (§6.6).
    let patterns: Vec<(String, bool)> = ctx
        .db
        .chat_filter()
        .iter()
        .map(|filter| (filter.pattern, filter.mute))
        .collect();
    let mut rate = chat_rate_for(ctx);
    if filter_mutes(&sanitized, &patterns) {
        rate.infractions = rate.infractions.saturating_add(1);
        if rate.infractions >= 3 {
            rate.muted_until =
                Some(ctx.timestamp + spacetimedb::TimeDuration::from_micros(60_000_000));
        }
        ctx.db.chat_rate().identity().update(rate);
        return Ok(());
    }

    let config = server_config_or_default(ctx);
    if rate
        .muted_until
        .map(|until| {
            until.to_micros_since_unix_epoch() > ctx.timestamp.to_micros_since_unix_epoch()
        })
        .unwrap_or(false)
    {
        return Err("you are muted".to_string());
    }
    let now_micros = ctx.timestamp.to_micros_since_unix_epoch();
    if window_expired(
        now_micros,
        rate.window_start.to_micros_since_unix_epoch(),
        config.chat_rate_window_ms,
    ) {
        rate.window_start = ctx.timestamp;
        rate.posts = 0;
    }
    if rate.posts >= config.chat_rate_max {
        return Err("chat rate limit exceeded".to_string());
    }
    rate.posts = rate.posts.saturating_add(1);
    ctx.db.chat_rate().identity().update(rate);

    ctx.db.match_chat().insert(MatchChat {
        chat_id: 0,
        match_id,
        sender: ctx.sender(),
        kind,
        team,
        text: sanitized,
        sent_at: ctx.timestamp,
    });
    Ok(())
}

fn chat_rate_for(ctx: &ReducerContext) -> ChatRate {
    ctx.db
        .chat_rate()
        .identity()
        .find(ctx.sender())
        .unwrap_or(ChatRate {
            identity: ctx.sender(),
            window_start: ctx.timestamp,
            posts: 0,
            infractions: 0,
            muted_until: None,
        })
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
}
