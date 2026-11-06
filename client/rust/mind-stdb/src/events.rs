// SPDX-License-Identifier: GPL-3.0-only

//! Server event rows (`my_match_events`) on the client `EventType` bus
//! (GAPS #37).
//!
//! Ported from Mindustry `core/src/mindustry/game/EventType.java` (the event
//! classes this bus names) plus `core/src/mindustry/core/NetServer.java` and
//! `core/src/mindustry/net/Administration.java` (the fire sites). Java fires
//! `EventType` classes in-process on the server; a SpacetimeDB module has no
//! such bus, so `server/spacetimedb/src/relay/events.rs` publishes one
//! `match_event` row per server-observable event and `my_match_events` exposes
//! the caller's match. This module turns those rows into an ordered, bounded
//! client bus.
//!
//! Mapping notes (upstream parity, GAPS #37):
//! - match lifecycle maps to `WorldLoadEvent` / `PlayEvent` / `GameOverEvent`;
//! - a kick surfaces as `PlayerLeave` (Java `NetServer.onDisconnect`); Java has
//!   no `EventType` class for `KickReason` (GAPS #35), so the module kind stays
//!   on [`ServerEvent::kind`];
//! - votekick, name-ban, `/sync` and client-command rows have no upstream
//!   `EventType` class; their module kind name is used verbatim.

use std::collections::BTreeMap;

use crate::binder::RowChange;
use crate::module_bindings::{EventKind, MatchEvent};

/// Maximum number of events retained client-side (the server prunes the table
/// by `MATCH_EVENT_RETENTION_SECS`; the mirror keeps a bounded tail).
pub const EVENT_HISTORY_LIMIT: usize = 512;

/// Every published [`EventKind`], in ABI order (append new variants here and
/// the mapping test fails until [`event_type_name`] covers them).
pub const ALL_EVENT_KINDS: [EventKind; 18] = [
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

/// Upstream `EventType` class name for one published kind.
///
/// Kinds with no upstream class return the module's own `EventKind` name; the
/// doc comment on [`ServerEvent::event_type`] lists them.
pub fn event_type_name(kind: EventKind) -> &'static str {
    match kind {
        // Lobby match created; Java's `WorldLoadEvent` stands in for the
        // world a client loads on join (module ABI note).
        EventKind::MatchCreate => "WorldLoadEvent",
        // Lobby match started; `Logic.play` fires `PlayEvent`.
        EventKind::MatchStart => "PlayEvent",
        // Terminal match state.
        EventKind::MatchEnd => "GameOverEvent",
        EventKind::PlayerJoin => "PlayerJoin",
        EventKind::PlayerLeave => "PlayerLeave",
        // A kick is observed as a leave (`NetServer.onDisconnect`); the
        // `KickReason` itself is not an `EventType` class (GAPS #35).
        EventKind::PlayerKick => "PlayerLeave",
        EventKind::PlayerBan => "PlayerBanEvent",
        EventKind::PlayerUnban => "PlayerUnbanEvent",
        EventKind::PlayerChat => "PlayerChatEvent",
        // No upstream `EventType` class for votekick sessions.
        EventKind::VoteStarted => "VoteStarted",
        EventKind::VotePassed => "VotePassed",
        EventKind::VoteFailed => "VoteFailed",
        EventKind::ConfigChange => "ConfigEvent",
        EventKind::AdminAction => "AdminRequestEvent",
        // `Administration.addNameBan`/`removeNameBan` fire nothing upstream.
        EventKind::NameBan => "NameBan",
        EventKind::NameUnban => "NameUnban",
        // Java `/sync` asks for a world resend directly; no event class.
        EventKind::SyncRequest => "SyncRequest",
        EventKind::ClientCommand => "ClientCommand",
    }
}

/// One event row projected for the client bus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerEvent {
    /// Auto-inc event id (monotonic commit order).
    pub event_id: u64,
    /// Owning match.
    pub match_id: u64,
    /// Published kind (ABI enum).
    pub kind: EventKind,
    /// Causer identity hex, when the event has one.
    pub actor: Option<String>,
    /// Subject identity hex, when the event has one.
    pub target: Option<String>,
    /// Short plain-text detail (`""` when the columns suffice).
    pub detail: String,
    /// Server commit time (micros since the Unix epoch).
    pub at_micros: i64,
}

impl ServerEvent {
    /// Projects one generated row.
    pub fn from_row(row: &MatchEvent) -> Self {
        Self {
            event_id: row.event_id,
            match_id: row.match_id,
            kind: row.kind,
            actor: row.actor.map(|actor| actor.to_hex().to_string()),
            target: row.target.map(|target| target.to_hex().to_string()),
            detail: row.detail.clone(),
            at_micros: row.at.to_micros_since_unix_epoch(),
        }
    }

    /// Upstream `EventType` class name (see [`event_type_name`]).
    pub fn event_type(&self) -> &'static str {
        event_type_name(self.kind)
    }

    /// Stable JSON line for the event bus consumer.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.event_id,
            "matchId": self.match_id,
            "kind": format!("{:?}", self.kind),
            "type": self.event_type(),
            "actor": self.actor,
            "target": self.target,
            "detail": self.detail,
            "at": self.at_micros,
        })
    }
}

/// Ordered client mirror of the `my_match_events` view.
///
/// Rows are deduplicated by `event_id` (a replayed cache re-inserts the same
/// rows) and retained newest-`limit`. [`Self::after`] replays only the events a
/// consumer has not processed yet, the same watermark idiom as
/// [`crate::CommandStream`].
#[derive(Debug, Clone)]
pub struct EventBus {
    limit: usize,
    events: BTreeMap<u64, ServerEvent>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self {
            limit: EVENT_HISTORY_LIMIT,
            events: BTreeMap::new(),
        }
    }
}

impl EventBus {
    /// Empty bus with the default history limit.
    pub fn new() -> Self {
        Self::default()
    }

    /// Empty bus retaining at most `limit` events (clamped to at least one).
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            ..Self::default()
        }
    }

    /// Applies one `my_match_events` change.
    pub fn apply(&mut self, change: RowChange<MatchEvent>) {
        match change {
            RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                let event = ServerEvent::from_row(&row);
                self.events.insert(event.event_id, event);
                self.trim();
            }
            RowChange::Delete(row) => {
                self.events.remove(&row.event_id);
            }
        }
    }

    /// One retained event by id.
    pub fn event(&self, event_id: u64) -> Option<&ServerEvent> {
        self.events.get(&event_id)
    }

    /// Retained events in commit (`event_id`) order.
    pub fn events(&self) -> Vec<&ServerEvent> {
        self.events.values().collect()
    }

    /// Retained events with `event_id > after_id`, in commit order.
    pub fn after(&self, after_id: u64) -> Vec<&ServerEvent> {
        self.events
            .range((
                std::ops::Bound::Excluded(after_id),
                std::ops::Bound::Unbounded,
            ))
            .map(|(_, event)| event)
            .collect()
    }

    /// Greatest retained `event_id`, if any (consumer watermark).
    pub fn last_event_id(&self) -> Option<u64> {
        self.events.keys().next_back().copied()
    }

    /// Retained event count.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether no event is retained.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// History limit.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Drops every retained event (disconnect/resync).
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// The bus as a JSON array (UI/log consumer input).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::Value::Array(self.events.values().map(ServerEvent::to_json).collect())
    }

    fn trim(&mut self) {
        while self.events.len() > self.limit {
            let Some(oldest) = self.events.keys().next().copied() else {
                break;
            };
            self.events.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spacetimedb_sdk::{Identity, Timestamp};

    fn identity(byte: u8) -> Identity {
        Identity::from_byte_array([byte; 32])
    }

    fn row(event_id: u64, kind: EventKind) -> MatchEvent {
        MatchEvent {
            event_id,
            match_id: 7,
            kind,
            actor: Some(identity(2)),
            target: None,
            detail: format!("event {event_id}"),
            at: Timestamp::from_micros_since_unix_epoch(event_id as i64 * 1_000_000),
        }
    }

    /// Every ABI kind has a non-empty name; the player-facing ones match the
    /// upstream `EventType` classes (`EventType.java:248-846`).
    #[test]
    fn event_type_names_cover_every_kind() {
        assert_eq!(ALL_EVENT_KINDS.len(), 18);
        for kind in ALL_EVENT_KINDS {
            assert!(!event_type_name(kind).is_empty(), "{kind:?} has no name");
        }
        assert_eq!(event_type_name(EventKind::MatchCreate), "WorldLoadEvent");
        assert_eq!(event_type_name(EventKind::MatchStart), "PlayEvent");
        assert_eq!(event_type_name(EventKind::MatchEnd), "GameOverEvent");
        assert_eq!(event_type_name(EventKind::PlayerJoin), "PlayerJoin");
        assert_eq!(event_type_name(EventKind::PlayerLeave), "PlayerLeave");
        assert_eq!(event_type_name(EventKind::PlayerKick), "PlayerLeave");
        assert_eq!(event_type_name(EventKind::PlayerBan), "PlayerBanEvent");
        assert_eq!(event_type_name(EventKind::PlayerUnban), "PlayerUnbanEvent");
        assert_eq!(event_type_name(EventKind::PlayerChat), "PlayerChatEvent");
        assert_eq!(event_type_name(EventKind::ConfigChange), "ConfigEvent");
        assert_eq!(event_type_name(EventKind::AdminAction), "AdminRequestEvent");
        // Module-only kinds keep their own names.
        assert_eq!(event_type_name(EventKind::VotePassed), "VotePassed");
        assert_eq!(event_type_name(EventKind::NameBan), "NameBan");
        assert_eq!(event_type_name(EventKind::SyncRequest), "SyncRequest");
    }

    #[test]
    fn bus_orders_dedupes_and_replays_after_watermark() {
        let mut bus = EventBus::new();
        bus.apply(RowChange::Insert(row(3, EventKind::PlayerChat)));
        bus.apply(RowChange::Insert(row(1, EventKind::MatchCreate)));
        bus.apply(RowChange::Insert(row(2, EventKind::PlayerJoin)));
        assert_eq!(
            bus.events()
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(bus.last_event_id(), Some(3));

        // A replayed cache re-inserts the same id: no duplicate.
        bus.apply(RowChange::Insert(row(2, EventKind::PlayerJoin)));
        assert_eq!(bus.len(), 3);

        // A consumer watermark sees only newer events.
        assert_eq!(
            bus.after(1)
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert!(bus.after(3).is_empty());

        // Update replaces in place; delete removes.
        bus.apply(RowChange::Update {
            old: row(2, EventKind::PlayerJoin),
            new: MatchEvent {
                detail: "joined again".to_owned(),
                ..row(2, EventKind::PlayerJoin)
            },
        });
        assert_eq!(
            bus.event(2).map(|event| event.detail.as_str()),
            Some("joined again")
        );
        bus.apply(RowChange::Delete(row(2, EventKind::PlayerJoin)));
        assert_eq!(bus.len(), 2);
        assert_eq!(bus.events()[1].event_id, 3);

        // The JSON projection names the upstream class and keeps the ABI kind.
        let json = bus.to_json();
        assert_eq!(json[0]["id"], 1);
        assert_eq!(json[0]["type"], "WorldLoadEvent");
        assert_eq!(json[0]["kind"], "MatchCreate");
        assert_eq!(json[0]["actor"], identity(2).to_hex().to_string());
        assert_eq!(json[0]["matchId"], 7);
        assert_eq!(json[1]["type"], "PlayerChatEvent");
        bus.clear();
        assert!(bus.is_empty());
    }

    #[test]
    fn bus_bounds_history_to_the_newest_events() {
        let mut bus = EventBus::with_limit(2);
        bus.apply(RowChange::Insert(row(1, EventKind::MatchCreate)));
        bus.apply(RowChange::Insert(row(2, EventKind::MatchStart)));
        bus.apply(RowChange::Insert(row(3, EventKind::MatchEnd)));
        assert_eq!(
            bus.events()
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(bus.limit(), 2);
        assert!(EventBus::with_limit(0).limit() >= 1);
    }
}
