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

/// Typed `EventType` detail envelope for one published kind.
///
/// The client `mind-stdb` codec (`event_type.rs`, serde internally tagged on
/// `"type"` with camelCase fields) requires every struct field to be present, so
/// the envelopes spell out `player: null` where the module has no port player id
/// and carry the plain text in ignored extra fields.
///
/// Kinds deliberately left as plain text: `MatchCreate` (the map id is rendered
/// verbatim by the client toast) and `PlayerLeave` (the leave reason is rendered
/// verbatim), plus the module-only kinds. Those need the client toast to prefer
/// the typed payload first (`mind-gdext/src/ui/fragments/toasts.rs`, outside this
/// crate's ownership).
pub fn typed_detail(kind: EventKind, detail: &str) -> Option<String> {
    let text = json_escape(detail);
    Some(match kind {
        // `Logic.play` fires `PlayEvent`; the lobby start row stands in.
        EventKind::MatchStart => r#"{"type":"PlayEvent"}"#.to_owned(),
        // `Logic.gameOver` fires `GameOverEvent`; the module has no winning team
        // for a host-left end, so 0 (derelict) means "no winner".
        EventKind::MatchEnd if text.is_empty() => {
            r#"{"type":"GameOverEvent","winner":0}"#.to_owned()
        }
        EventKind::MatchEnd => {
            format!(r#"{{"type":"GameOverEvent","winner":0,"reason":"{text}"}}"#)
        }
        // `NetServer` join path fires `PlayerJoin`.
        EventKind::PlayerJoin if text.is_empty() => {
            r#"{"type":"PlayerJoin","player":null}"#.to_owned()
        }
        EventKind::PlayerJoin => {
            format!(r#"{{"type":"PlayerJoin","player":null,"name":"{text}"}}"#)
        }
        // `Building.configure` fires `ConfigEvent`; the config field/value text
        // goes through as the opaque `value` projection.
        EventKind::ConfigChange if text.is_empty() => {
            r#"{"type":"ConfigEvent","tile":null,"player":null,"value":null}"#.to_owned()
        }
        EventKind::ConfigChange => {
            format!(r#"{{"type":"ConfigEvent","tile":null,"player":null,"value":"{text}"}}"#)
        }
        _ => return None,
    })
}

/// Minimal JSON string escaping (the module crate has no `serde_json`).
fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            character if (character as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => out.push(character),
        }
    }
    out
}

/// Publishes one event row. Values are capped so a client cannot overflow the
/// table through a long reject/detail string.
///
/// Kinds with a typed upstream class publish the `{"type":"<Class>", ...}`
/// JSON envelope the client `mind-stdb` `EventType` codec decodes
/// (`client/rust/mind-stdb/src/event_type.rs`); the plain text rides along in
/// an ignored extra field (`name`/`reason`/`value`) so nothing is lost.
/// Kinds without one keep the plain detail.
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
    let detail = typed_detail(kind, &detail).unwrap_or(detail);
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

    /// The lifecycle rows carry the typed `EventType` envelope the client
    /// `EventBus` decodes (`mind-stdb/src/event_type.rs`: internally tagged on
    /// `type`, camelCase fields, every struct field present); the plain text
    /// rides in an ignored `reason`/`name` field so nothing is lost.
    #[test]
    fn lifecycle_rows_publish_typed_event_type_envelopes() {
        assert_eq!(
            typed_detail(EventKind::MatchStart, "").as_deref(),
            Some(r#"{"type":"PlayEvent"}"#)
        );
        assert_eq!(
            typed_detail(EventKind::MatchEnd, "").as_deref(),
            Some(r#"{"type":"GameOverEvent","winner":0}"#)
        );
        assert_eq!(
            typed_detail(EventKind::MatchEnd, "host left").as_deref(),
            Some(r#"{"type":"GameOverEvent","winner":0,"reason":"host left"}"#)
        );
        assert_eq!(
            typed_detail(EventKind::PlayerJoin, "").as_deref(),
            Some(r#"{"type":"PlayerJoin","player":null}"#)
        );
        assert_eq!(
            typed_detail(EventKind::PlayerJoin, "alice").as_deref(),
            Some(r#"{"type":"PlayerJoin","player":null,"name":"alice"}"#)
        );
        assert_eq!(
            typed_detail(EventKind::ConfigChange, "").as_deref(),
            Some(r#"{"type":"ConfigEvent","tile":null,"player":null,"value":null}"#)
        );
        assert_eq!(
            typed_detail(EventKind::ConfigChange, "waveSpacing=30").as_deref(),
            Some(r#"{"type":"ConfigEvent","tile":null,"player":null,"value":"waveSpacing=30"}"#)
        );
        // Kinds whose detail is rendered verbatim stay plain text.
        for kind in [
            EventKind::MatchCreate,
            EventKind::PlayerLeave,
            EventKind::PlayerChat,
            EventKind::VoteStarted,
            EventKind::NameBan,
            EventKind::SyncRequest,
            EventKind::ClientCommand,
        ] {
            assert_eq!(typed_detail(kind, "text"), None, "{kind:?} stays plain");
        }
    }

    /// Detail text is JSON-escaped: a name/reason cannot break the envelope.
    #[test]
    fn typed_detail_escapes_json_text() {
        let envelope = typed_detail(EventKind::PlayerJoin, "a\"b\\c\td").expect("typed join");
        assert!(
            envelope.contains(r#""name":"a\"b\\c\td""#),
            "escaped name: {envelope}"
        );
        // One JSON object: balanced braces, no stray control characters.
        assert!(envelope.starts_with('{') && envelope.ends_with('}'));
        assert_eq!(envelope.matches('{').count(), 1);
        assert_eq!(envelope.matches('}').count(), 1);
        assert!(!envelope.contains('\t'), "tab escaped: {envelope}");
    }
}
