// SPDX-License-Identifier: GPL-3.0-only

//! Server-side event publication for the client `EventType` bus (GAPS #37).
//!
//! Java fires its `EventType` classes inside the server process so plugins and
//! the UI observe them directly. A SpacetimeDB module has no in-process bus, so
//! the module publishes one row per server-observable event into `match_event`;
//! `my_match_events` exposes the caller's active match. The client transport
//! binds that view and re-emits each row onto its `EventType` bus.
//!
//! Scope is deliberately "server-observable" events: match lifecycle, player
//! join/leave/kick/ban, chat, votes, admin/config changes and the relay's sync
//! request. Simulation events (`UnitCreate`, `BulletDestroy`, ...) are produced
//! by the deterministic client sim, not the relay, so they are not published
//! here; see the note on the twin item for the mapping.
//!
//! Variant names are ABI and append-only, like every other `SpacetimeType` enum
//! in this crate. Rows are pruned by [`crate::main::global::MATCH_EVENT_RETENTION_SECS`].

use spacetimedb::{Identity, ReducerContext, SpacetimeType, Table, Timestamp, table};

use crate::main::global::MAX_TEXT;

/// Server-observable event kind, named after the upstream `EventType` class it
/// maps to (GAPS #37). Append-only ABI.
#[derive(SpacetimeType, Clone, Copy, PartialEq, Eq, Debug)]
pub enum EventKind {
    /// A match was created (`EventType.WorldLoadEvent` stands in on the client).
    MatchCreate,
    /// A lobby match started.
    MatchStart,
    /// A match reached its terminal state.
    MatchEnd,
    /// A player joined a match (`EventType.PlayerJoin`).
    PlayerJoin,
    /// A player left a match (`EventType.PlayerLeave`).
    PlayerLeave,
    /// A player was kicked (`EventType.PlayerBan`/`KickReason` path).
    PlayerKick,
    /// An identity was banned (`EventType.PlayerBanEvent`).
    PlayerBan,
    /// An identity was unbanned (`EventType.PlayerUnbanEvent`).
    PlayerUnban,
    /// A chat message was accepted (`EventType.PlayerChatEvent`).
    PlayerChat,
    /// A votekick session started.
    VoteStarted,
    /// A votekick passed and the target was kicked.
    VotePassed,
    /// A votekick expired without enough votes.
    VoteFailed,
    /// A `server_config` field changed (`Config` change listeners).
    ConfigChange,
    /// A committed admin action outside the specific kinds above.
    AdminAction,
    /// A name-ban pattern was added (`Administration.addNameBan`).
    NameBan,
    /// A name-ban pattern was removed.
    NameUnban,
    /// A client asked to re-synchronize world state (`/sync`).
    SyncRequest,
    /// A client requested a plugin sync by command (reserved).
    ClientCommand,
}

/// One published server event (`my_match_events` exposes the caller's match).
#[table(accessor = match_event, index(accessor = by_match_event, btree(columns = [match_id, event_id])))]
pub struct MatchEvent {
    /// Auto-inc event id; monotonically increasing commit order.
    #[primary_key]
    #[auto_inc]
    pub event_id: u64,
    /// Owning match.
    #[index(btree)]
    pub match_id: u64,
    /// Upstream event class equivalent.
    pub kind: EventKind,
    /// Causer, when the event has one.
    pub actor: Option<Identity>,
    /// Subject (kicked/banned/traced player), when the event has one.
    pub target: Option<Identity>,
    /// Short plain-text detail (empty when the structured columns suffice).
    pub detail: String,
    /// Server commit time.
    pub at: Timestamp,
}

/// Publishes one event row. Values are capped so a client cannot overflow the
/// table through a long reject/detail string.
pub fn emit(
    ctx: &ReducerContext,
    match_id: u64,
    kind: EventKind,
    actor: Option<Identity>,
    target: Option<Identity>,
    detail: impl Into<String>,
) {
    let mut detail = detail.into();
    detail.retain(|c| c == '\t' || !c.is_control());
    let detail: String = detail.chars().take(MAX_TEXT).collect();
    ctx.db.match_event().insert(MatchEvent {
        event_id: 0,
        match_id,
        kind,
        actor,
        target,
        detail,
        at: ctx.timestamp,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_kinds_are_distinct() {
        // Guard against accidental duplicate variants in the ABI enum.
        let kinds = [
            EventKind::MatchCreate,
            EventKind::MatchStart,
            EventKind::MatchEnd,
            EventKind::PlayerJoin,
            EventKind::PlayerLeave,
            EventKind::PlayerKick,
            EventKind::PlayerBan,
            EventKind::PlayerUnban,
            EventKind::PlayerChat,
            EventKind::VoteStarted,
            EventKind::VotePassed,
            EventKind::VoteFailed,
            EventKind::ConfigChange,
            EventKind::AdminAction,
            EventKind::NameBan,
            EventKind::NameUnban,
            EventKind::SyncRequest,
            EventKind::ClientCommand,
        ];
        for (index, kind) in kinds.iter().enumerate() {
            assert!(!kinds[..index].contains(kind));
        }
    }
}
