// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/game/EventType.java (event classes),
//         core/src/mindustry/ui/fragments/HudFragment.java (`showToast`/`showUnlock`),
//         core/src/mindustry/core/NetServer.java (`sendMessage` join/leave lines),
//         core/src/mindustry/core/Control.java (`UnlockEvent` toast).

//! `EventType` client toasts (GAPS #37 client remainder, PAR-0548).
//!
//! Java fires `EventType` classes in-process; the relay publishes
//! `match_event` rows and `mind-stdb`'s `EventBus` names each row with the
//! upstream `EventType` class it stands for (`mind_stdb::events::event_type_name`).
//! This module maps those rows onto the HUD toast queue (`MindUiRoot.on_toast`,
//! `HudFragment.showToast` slot).
//!
//! Java parity notes:
//! - `UnlockEvent`/`ResearchEvent` toasts are already produced client-side by
//!   `MindCampaign` (`Control.java:161-164` → `HudFragment.showUnlock`), so the
//!   relay rows never carry them; this table therefore starts at the
//!   server-observable classes.
//! - The relay publishes a typed `EventType` envelope in `match_event.detail`
//!   for `MatchStart`/`MatchEnd`/`PlayerJoin`/`ConfigChange` (and, since
//!   PAR-0577's residual, `MatchCreate`/`PlayerLeave`). Those rows render the
//!   decoded payload fields (`EventPayload`) instead of the raw JSON envelope;
//!   legacy/plain-text rows keep the verbatim detail path.
//! - Upstream surfaces join/leave/ban/`Config` changes only as chat/system
//!   lines (`NetServer.java:1012,623`, `Call.sendMessage`); the port raises the
//!   same events as HUD toasts because the relay's `my_match_chat` view may be
//!   scrolled away. Every mapping is documented below — this is a port
//!   addition, not a claim that Java toasts these classes.
//! - `PlayerChatEvent` never toasts (the chat overlay renders it), and
//!   `SyncRequest`/`ClientCommand` are relay plumbing with no user-facing line.
//! - `MatchCreate`→`WorldLoadEvent`/`MatchStart`→`PlayEvent`/`MatchEnd`→
//!   `GameOverEvent` are the module's documented lifecycle stand-ins (GAPS
//!   #37); the toast text follows the world/game lifecycle, not the lobby row.

use std::collections::HashMap;

use mind_stdb::EventType;
use mind_stdb::events::{ALL_EVENT_KINDS, event_type_name};
use mind_stdb::module_bindings::EventKind;

use crate::ui::kit::roster::{self, RosterName};

/// The toast glyph tag understood by `MindUiRoot.on_toast`
/// (`"warning"`/`"error"` → `Icon.warn`, else `Icon.ok`).
pub const ICON_OK: &str = "ok";
/// `Icon.warn` (`HudFragment.showToast(Icon.warning, ...)`).
pub const ICON_WARN: &str = "warning";

/// Typed `EventType` payload of one row.
///
/// The relay writes the `{"type": "<Class>", ...}` envelope into
/// `MatchEvent.detail` (`server/spacetimedb/src/relay/events.rs::typed_detail`),
/// so a row's `detail` may be a JSON payload rather than display text; the
/// `EventType` codec (`mind_stdb::EventType`) decodes it, and this projection
/// keeps only the fields the toast text needs (the codec drops the relay's
/// ignored `name`/`reason`/`map` side channel, which is read from the raw JSON).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventPayload {
    /// Upstream `EventType` class name of the decoded payload.
    pub class: &'static str,
    /// Port player id carried by `PlayerJoin`/`PlayerLeave`.
    pub player: Option<i32>,
    /// Winning team id carried by `GameOverEvent` (`0` = no winner).
    pub winner: Option<u8>,
    /// `ConfigEvent.value` rendered as display text.
    pub value: Option<String>,
    /// Relay plain-text side channel (`name`/`reason`/`map`).
    pub text: Option<String>,
}

impl EventPayload {
    /// Decodes one envelope; `None` when `envelope` is plain text (legacy rows,
    /// module-only kinds) or names a class the codec does not know.
    pub fn decode(envelope: &str) -> Option<Self> {
        let envelope = envelope.trim();
        if !envelope.starts_with('{') {
            return None;
        }
        let raw: serde_json::Value = serde_json::from_str(envelope).ok()?;
        let event = EventType::decode(envelope).ok()?;
        let text = ["name", "reason", "map"]
            .iter()
            .find_map(|key| raw.get(key).and_then(serde_json::Value::as_str))
            .filter(|text| !text.is_empty())
            .map(str::to_owned);
        let value = raw
            .get("value")
            .map(|value| match value {
                serde_json::Value::String(text) => text.clone(),
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            })
            .filter(|value| !value.is_empty());
        let (player, winner) = match &event {
            EventType::PlayerJoin { player } | EventType::PlayerLeave { player } => (*player, None),
            EventType::GameOver { winner } => (None, Some(*winner)),
            _ => (None, None),
        };
        Some(Self {
            class: event.class_name(),
            player,
            winner,
            value,
            text,
        })
    }
}

/// One `my_match_events` row projected for the toast mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRow {
    /// Monotonic event id (the replay watermark).
    pub id: u64,
    /// ABI event kind (`match_event.kind`).
    pub kind: EventKind,
    /// Causer identity hex, when the event has one.
    pub actor: Option<String>,
    /// Subject identity hex, when the event has one.
    pub target: Option<String>,
    /// Short plain-text detail from the module (or the typed JSON envelope for
    /// classes with a `typed_detail` payload).
    pub detail: String,
    /// Decoded typed payload, when `detail`/`payload` carries one.
    pub payload: Option<EventPayload>,
}

/// One queued toast (`MindUiRoot.toast_label` input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Java-markup text.
    pub text: String,
    /// `"ok"` / `"warning"`.
    pub icon: &'static str,
}

impl EventRow {
    /// The ABI variant name of the row (`{"kind": "<name>"}` JSON value).
    pub fn kind_name(&self) -> String {
        format!("{:?}", self.kind)
    }
}

impl Toast {
    fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            icon: ICON_OK,
        }
    }

    fn warn(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            icon: ICON_WARN,
        }
    }
}

/// `EventKind` by its ABI variant name (`format!("{kind:?}")`), for JSON rows.
pub fn kind_from_name(name: &str) -> Option<EventKind> {
    ALL_EVENT_KINDS
        .iter()
        .copied()
        .find(|kind| format!("{kind:?}") == name)
}

/// Parses one `mind_stdb::ServerEvent::to_json` row (`None` when malformed).
pub fn parse_event_row(value: &serde_json::Value) -> Option<EventRow> {
    let detail = value
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    // `ServerEvent::to_json` exposes the decoded class payload as a JSON string
    // (null for untyped rows); older/legacy projections carry the envelope in
    // `detail` only, so both are accepted.
    let payload = value
        .get("payload")
        .and_then(serde_json::Value::as_str)
        .and_then(EventPayload::decode)
        .or_else(|| EventPayload::decode(&detail));
    Some(EventRow {
        id: value.get("id")?.as_u64()?,
        kind: kind_from_name(value.get("kind")?.as_str()?)?,
        actor: value
            .get("actor")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        target: value
            .get("target")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        detail,
        payload,
    })
}

/// Parses a `my_match_events` payload: an array of rows or one row object.
pub fn parse_rows(json: &str) -> Vec<EventRow> {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Array(rows)) => rows.iter().filter_map(parse_event_row).collect(),
        Ok(value @ serde_json::Value::Object(_)) => parse_event_row(&value).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// The upstream `EventType` class name of `row`: the decoded payload's class
/// when it carries one (`ServerEvent::class_name`), else the kind mapping.
pub fn event_type(row: &EventRow) -> &'static str {
    row.payload
        .as_ref()
        .map_or_else(|| event_type_name(row.kind), |payload| payload.class)
}

/// Plain-text detail of a row, or `""` when the module wrote a typed JSON
/// envelope there (that envelope must never render as toast text).
fn plain_detail(row: &EventRow) -> &str {
    let detail = row.detail.trim();
    if detail.starts_with('{') { "" } else { detail }
}

/// Relay side-channel text of the typed payload (`name`/`reason`/`map`).
fn payload_text(payload: Option<&EventPayload>) -> &str {
    payload
        .and_then(|payload| payload.text.as_deref())
        .unwrap_or_default()
}

/// Display name for a row's subject: the resolved roster name, else the typed
/// payload's `name` side channel, else the payload's player id, else the
/// identity prefix.
fn subject(row: &EventRow, names: &HashMap<String, RosterName>) -> String {
    if let Some(identity) = row.target.as_deref().or(row.actor.as_deref()) {
        return roster::display_name(identity, names);
    }
    let typed = payload_text(row.payload.as_ref());
    if !typed.is_empty() {
        return typed.to_owned();
    }
    if let Some(player) = row.payload.as_ref().and_then(|payload| payload.player) {
        return format!("player {player}");
    }
    plain_detail(row).to_owned()
}

/// The toast for one event row (`None` = no user-facing notification).
///
/// Mapping (see the module docs for the parity justification). Rows with a
/// typed `EventType` payload (`MatchStart`/`MatchEnd`/`PlayerJoin`/
/// `ConfigChange`/`MatchCreate`/`PlayerLeave`) render the payload's fields —
/// never the raw `detail` envelope.
///
/// | kind | `EventType` | toast |
/// |---|---|---|
/// | `MatchCreate` | `WorldLoadEvent` | `World loaded: <map>` |
/// | `MatchStart` | `PlayEvent` | `Game started` |
/// | `MatchEnd` | `GameOverEvent` | warn `Game over: team <winner> won` / `Game over (<reason>)` |
/// | `PlayerJoin` | `PlayerJoin` | `<name> joined` |
/// | `PlayerLeave` | `PlayerLeave` | `<name> left (<reason>)` |
/// | `PlayerKick` | `PlayerLeave` | warn `<name> kicked` |
/// | `PlayerBan` | `PlayerBanEvent` | warn `<name> banned` |
/// | `PlayerUnban` | `PlayerUnbanEvent` | `<name> unbanned` |
/// | `VotePassed` | `VotePassed` | warn `<name> vote-kicked` |
/// | `ConfigChange` | `ConfigEvent` | `Config changed: <value>` |
/// | `NameBan` | `NameBan` | warn `Name ban added` |
/// | `NameUnban` | `NameUnban` | `Name ban removed` |
/// | `PlayerChat`/`VoteStarted`/`VoteFailed`/`AdminAction`/`SyncRequest`/`ClientCommand` | — | none |
pub fn toast_for(row: &EventRow, names: &HashMap<String, RosterName>) -> Option<Toast> {
    let name = subject(row, names);
    log::debug!(
        "[ui] event toast for {} ({})",
        event_type(row),
        row.kind_name()
    );
    let detail = plain_detail(row);
    let payload = row.payload.as_ref();
    Some(match row.kind {
        EventKind::MatchCreate => {
            // `WorldLoadEvent`: the map name rides the ignored `name` side
            // channel of the typed envelope.
            let map = payload_text(payload);
            let map = if map.is_empty() { detail } else { map };
            if map.is_empty() {
                Toast::ok("[accent]World loaded[]")
            } else {
                Toast::ok(format!("[accent]World loaded:[] [white]{map}[]"))
            }
        }
        EventKind::MatchStart => Toast::ok("[accent]Game started[]"),
        EventKind::MatchEnd => {
            // `GameOverEvent.winner` (0 = the module's "no winner", e.g. the
            // host left); the module's end reason rides the `reason` channel.
            match payload.and_then(|payload| payload.winner) {
                Some(winner) if winner != 0 => {
                    Toast::warn(format!("[scarlet]Game over:[] [white]team {winner} won[]"))
                }
                _ => {
                    let reason = payload_text(payload);
                    let reason = if reason.is_empty() { detail } else { reason };
                    if reason.is_empty() {
                        Toast::warn("[scarlet]Game over[]")
                    } else {
                        Toast::warn(format!("[scarlet]Game over[] ([lightgray]{reason}[])"))
                    }
                }
            }
        }
        EventKind::PlayerJoin => Toast::ok(format!("[lightgray]{name} joined[]")),
        EventKind::PlayerLeave => {
            let reason = payload_text(payload);
            let reason = if reason.is_empty() { detail } else { reason };
            let reason = if reason.is_empty() {
                String::new()
            } else {
                format!(" ({reason})")
            };
            Toast::ok(format!("[lightgray]{name} left{reason}[]"))
        }
        EventKind::PlayerKick => Toast::warn(format!("[scarlet]{name} kicked[]")),
        EventKind::PlayerBan => Toast::warn(format!("[scarlet]{name} banned[]")),
        EventKind::PlayerUnban => Toast::ok(format!("[lightgray]{name} unbanned[]")),
        EventKind::VotePassed => Toast::warn(format!("[scarlet]{name} vote-kicked[]")),
        EventKind::ConfigChange => {
            // `ConfigEvent.value` (the relay's opaque config projection).
            let value = payload
                .and_then(|payload| payload.value.as_deref())
                .filter(|value| !value.is_empty())
                .unwrap_or(detail);
            if value.is_empty() {
                Toast::ok("[accent]Config changed[]")
            } else {
                Toast::ok(format!("[accent]Config changed:[] [white]{value}[]"))
            }
        }
        EventKind::NameBan => Toast::warn("[scarlet]Name ban added[]"),
        EventKind::NameUnban => Toast::ok("[lightgray]Name ban removed[]"),
        // The chat overlay renders chat; votes/admin/sync rows have no toast.
        EventKind::PlayerChat
        | EventKind::VoteStarted
        | EventKind::VoteFailed
        | EventKind::AdminAction
        | EventKind::SyncRequest
        | EventKind::ClientCommand => return None,
    })
}

/// Bounded event replay that turns new `my_match_events` rows into toasts.
#[derive(Debug, Clone, Default)]
pub struct EventToasts {
    watermark: Option<u64>,
    pending: Vec<Toast>,
}

impl EventToasts {
    /// Empty pump (nothing seen, nothing queued).
    pub fn new() -> Self {
        Self::default()
    }

    /// Highest consumed `event_id` (the replay watermark).
    pub fn watermark(&self) -> Option<u64> {
        self.watermark
    }

    /// Applies a `my_match_events` JSON payload; returns the number of toasts
    /// queued. Rows at or below the watermark replay without re-toasting (the
    /// relay cache re-sends them on a resync).
    pub fn apply_json(&mut self, json: &str, names: &HashMap<String, RosterName>) -> usize {
        let mut rows = parse_rows(json);
        rows.sort_by_key(|row| row.id);
        let mut queued = 0;
        for row in rows {
            if self.watermark.is_some_and(|watermark| row.id <= watermark) {
                continue;
            }
            self.watermark = Some(row.id);
            if let Some(toast) = toast_for(&row, names)
                && !toast.text.is_empty()
            {
                self.pending.push(toast);
                queued += 1;
            }
        }
        queued
    }

    /// Takes every queued toast (the HUD consumes them through its slot queue).
    pub fn take(&mut self) -> Vec<Toast> {
        std::mem::take(&mut self.pending)
    }

    /// Queued toast count (tests).
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Drops the watermark and the queue (disconnect).
    pub fn clear(&mut self) {
        self.watermark = None;
        self.pending.clear();
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]
    #![allow(clippy::unwrap_used)]

    use super::*;
    use serde_json::json;

    fn names() -> HashMap<String, RosterName> {
        roster::roster_index(&roster::roster_names(
            r#"[{"identity":"aa11","name":"Alice","team":1,"admin":false,"connected":true},
                {"identity":"bb22","name":"Bob","team":2,"admin":false,"connected":true}]"#,
        ))
    }

    fn row(id: u64, kind: EventKind, target: Option<&str>, detail: &str) -> serde_json::Value {
        json!({
            "id": id,
            "matchId": 7,
            "kind": format!("{kind:?}"),
            "type": event_type_name(kind),
            "actor": "aa11",
            "target": target,
            "detail": detail,
            "at": 0,
        })
    }

    #[test]
    fn kind_names_round_trip_the_abi() {
        assert_eq!(ALL_EVENT_KINDS.len(), 18);
        for kind in ALL_EVENT_KINDS {
            let name = format!("{kind:?}");
            assert_eq!(kind_from_name(&name), Some(kind), "{name}");
        }
        assert_eq!(kind_from_name("Nope"), None);
        // The parser accepts the module's JSON spelling.
        let parsed = parse_rows(&format!(
            "[{}]",
            row(1, EventKind::PlayerJoin, None, "Alice")
        ));
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].kind, EventKind::PlayerJoin);
        assert!(parse_rows("nope").is_empty());
    }

    #[test]
    fn every_toasted_kind_names_its_upstream_event_type() {
        let checks = [
            (EventKind::MatchCreate, "WorldLoadEvent"),
            (EventKind::MatchStart, "PlayEvent"),
            (EventKind::MatchEnd, "GameOverEvent"),
            (EventKind::PlayerJoin, "PlayerJoin"),
            (EventKind::PlayerLeave, "PlayerLeave"),
            (EventKind::PlayerKick, "PlayerLeave"),
            (EventKind::PlayerBan, "PlayerBanEvent"),
            (EventKind::PlayerUnban, "PlayerUnbanEvent"),
            (EventKind::PlayerChat, "PlayerChatEvent"),
            (EventKind::ConfigChange, "ConfigEvent"),
            (EventKind::AdminAction, "AdminRequestEvent"),
        ];
        for (kind, expected) in checks {
            let value = row(1, kind, None, "");
            let parsed = parse_event_row(&value).expect("row");
            assert_eq!(event_type(&parsed), expected);
        }
    }

    #[test]
    fn maps_player_events_to_names_and_warning_icons() {
        let names = names();
        let join = parse_event_row(&row(1, EventKind::PlayerJoin, Some("aa11"), "Alice")).unwrap();
        let toast = toast_for(&join, &names).expect("join toast");
        assert_eq!(toast.text, "[lightgray]Alice joined[]");
        assert_eq!(toast.icon, ICON_OK);

        let ban = parse_event_row(&row(2, EventKind::PlayerBan, Some("bb22"), "")).unwrap();
        let toast = toast_for(&ban, &names).expect("ban toast");
        assert_eq!(toast.text, "[scarlet]Bob banned[]");
        assert_eq!(toast.icon, ICON_WARN);

        // An unresolved identity still names the subject by prefix.
        let leave = parse_event_row(&row(3, EventKind::PlayerLeave, Some("cc33"), "")).unwrap();
        assert_eq!(
            toast_for(&leave, &names).expect("leave toast").text,
            "[lightgray]cc33 left[]"
        );
        // The module's host-leave detail rides along.
        let host =
            parse_event_row(&row(4, EventKind::PlayerLeave, Some("aa11"), "host left")).unwrap();
        assert_eq!(
            toast_for(&host, &names).expect("leave toast").text,
            "[lightgray]Alice left (host left)[]"
        );
    }

    #[test]
    fn lifecycle_and_config_toasts_carry_the_detail() {
        let names = names();
        let world = parse_event_row(&row(1, EventKind::MatchCreate, None, "groundZero")).unwrap();
        assert_eq!(
            toast_for(&world, &names).expect("world toast").text,
            "[accent]World loaded:[] [white]groundZero[]"
        );
        let start = parse_event_row(&row(2, EventKind::MatchStart, None, "")).unwrap();
        assert_eq!(
            toast_for(&start, &names).expect("start toast").text,
            "[accent]Game started[]"
        );
        let over = parse_event_row(&row(3, EventKind::MatchEnd, None, "")).unwrap();
        assert_eq!(
            toast_for(&over, &names).expect("over toast").icon,
            ICON_WARN
        );
        let config =
            parse_event_row(&row(4, EventKind::ConfigChange, None, "waveSpacing")).unwrap();
        assert_eq!(
            toast_for(&config, &names).expect("config toast").text,
            "[accent]Config changed:[] [white]waveSpacing[]"
        );
    }

    #[test]
    fn chat_and_plumbing_kinds_never_toast() {
        let names = names();
        for kind in [
            EventKind::PlayerChat,
            EventKind::VoteStarted,
            EventKind::VoteFailed,
            EventKind::AdminAction,
            EventKind::SyncRequest,
            EventKind::ClientCommand,
        ] {
            let parsed = parse_event_row(&row(1, kind, Some("aa11"), "x")).expect("row");
            assert!(toast_for(&parsed, &names).is_none(), "{kind:?}");
        }
        let passed = parse_event_row(&row(1, EventKind::VotePassed, Some("bb22"), "")).unwrap();
        assert_eq!(
            toast_for(&passed, &names).expect("vote toast").icon,
            ICON_WARN
        );
    }

    /// Row with the relay's typed `EventType` envelope in `detail` (the module's
    /// `typed_detail` output) — no `payload` field, like a live `to_json` row.
    fn typed_row(
        id: u64,
        kind: EventKind,
        target: Option<&str>,
        envelope: &str,
    ) -> serde_json::Value {
        // A row with a subject identity also carries the causer, like the
        // module's join/leave rows; a row with neither resolves its name from
        // the typed payload.
        let actor = target.map(|_| "aa11");
        json!({
            "id": id,
            "matchId": 7,
            "kind": format!("{kind:?}"),
            "type": event_type_name(kind),
            "actor": actor,
            "target": target,
            "detail": envelope,
            "payload": null,
            "at": 0,
        })
    }

    /// PAR-0577 residual: rows with a typed payload render its fields, never the
    /// raw `{"type": ...}` JSON envelope (`toasts.rs` used to print `detail`
    /// verbatim, which is exactly the envelope now).
    #[test]
    fn typed_payloads_render_typed_fields_not_raw_json() {
        let names = names();

        // `MatchStart` → `PlayEvent`.
        let start = parse_event_row(&typed_row(
            1,
            EventKind::MatchStart,
            None,
            r#"{"type":"PlayEvent"}"#,
        ))
        .expect("row");
        assert_eq!(event_type(&start), "PlayEvent");
        assert_eq!(
            toast_for(&start, &names).expect("start toast").text,
            "[accent]Game started[]"
        );

        // `MatchEnd` → `GameOverEvent` (typed winner, typed reason).
        let over = parse_event_row(&typed_row(
            2,
            EventKind::MatchEnd,
            None,
            r#"{"type":"GameOverEvent","winner":2}"#,
        ))
        .expect("row");
        assert_eq!(event_type(&over), "GameOverEvent");
        assert_eq!(
            toast_for(&over, &names).expect("over toast").text,
            "[scarlet]Game over:[] [white]team 2 won[]"
        );
        assert_eq!(
            toast_for(&over, &names).expect("over toast").icon,
            ICON_WARN
        );
        let host_left = parse_event_row(&typed_row(
            3,
            EventKind::MatchEnd,
            None,
            r#"{"type":"GameOverEvent","winner":0,"reason":"host left"}"#,
        ))
        .expect("row");
        assert_eq!(
            toast_for(&host_left, &names).expect("over toast").text,
            "[scarlet]Game over[] ([lightgray]host left[])"
        );

        // `PlayerJoin` → `PlayerJoin` (typed name side channel).
        let join = parse_event_row(&typed_row(
            4,
            EventKind::PlayerJoin,
            None,
            r#"{"type":"PlayerJoin","player":null,"name":"alice"}"#,
        ))
        .expect("row");
        assert_eq!(event_type(&join), "PlayerJoin");
        assert_eq!(
            toast_for(&join, &names).expect("join toast").text,
            "[lightgray]alice joined[]"
        );
        // Without a name the typed player id stands in.
        let join_id = parse_event_row(&typed_row(
            5,
            EventKind::PlayerJoin,
            None,
            r#"{"type":"PlayerJoin","player":7}"#,
        ))
        .expect("row");
        assert_eq!(
            toast_for(&join_id, &names).expect("join toast").text,
            "[lightgray]player 7 joined[]"
        );

        // `ConfigChange` → `ConfigEvent` (typed value).
        let config = parse_event_row(&typed_row(
            6,
            EventKind::ConfigChange,
            None,
            r#"{"type":"ConfigEvent","tile":null,"player":null,"value":"waveSpacing=30"}"#,
        ))
        .expect("row");
        assert_eq!(event_type(&config), "ConfigEvent");
        assert_eq!(
            toast_for(&config, &names).expect("config toast").text,
            "[accent]Config changed:[] [white]waveSpacing=30[]"
        );

        // `MatchCreate` → `WorldLoadEvent` with the map in the ignored channel.
        let world = parse_event_row(&typed_row(
            7,
            EventKind::MatchCreate,
            None,
            r#"{"type":"WorldLoadEvent","name":"groundZero"}"#,
        ))
        .expect("row");
        assert_eq!(event_type(&world), "WorldLoadEvent");
        assert_eq!(
            toast_for(&world, &names).expect("world toast").text,
            "[accent]World loaded:[] [white]groundZero[]"
        );

        // `PlayerLeave` → `PlayerLeave` with the reason side channel.
        let leave = parse_event_row(&typed_row(
            8,
            EventKind::PlayerLeave,
            Some("aa11"),
            r#"{"type":"PlayerLeave","player":null,"reason":"host left"}"#,
        ))
        .expect("row");
        assert_eq!(
            toast_for(&leave, &names).expect("leave toast").text,
            "[lightgray]Alice left (host left)[]"
        );

        // No rendered toast ever contains the envelope.
        for value in [
            typed_row(1, EventKind::MatchStart, None, r#"{"type":"PlayEvent"}"#),
            typed_row(
                2,
                EventKind::MatchEnd,
                None,
                r#"{"type":"GameOverEvent","winner":2}"#,
            ),
            typed_row(
                3,
                EventKind::PlayerJoin,
                None,
                r#"{"type":"PlayerJoin","player":null}"#,
            ),
            typed_row(
                4,
                EventKind::ConfigChange,
                None,
                r#"{"type":"ConfigEvent","tile":null,"player":null,"value":null}"#,
            ),
        ] {
            let parsed = parse_event_row(&value).expect("row");
            let text = toast_for(&parsed, &names).expect("toast").text;
            assert!(!text.contains('{') && !text.contains("type"), "{text}");
        }
    }

    /// `ServerEvent::to_json` also exposes the decoded payload as its own
    /// string; the parser prefers it over `detail` and still renders text.
    #[test]
    fn payload_field_is_decoded_when_detail_is_plain() {
        let names = names();
        let value = json!({
            "id": 9,
            "matchId": 7,
            "kind": "ConfigChange",
            "type": "ConfigEvent",
            "class": "ConfigEvent",
            "payload": r#"{"type":"ConfigEvent","tile":null,"player":null,"value":"waveSpacing=30"}"#,
            "actor": "aa11",
            "target": null,
            "detail": "plain fallback",
            "at": 0,
        });
        let parsed = parse_event_row(&value).expect("row");
        assert_eq!(
            toast_for(&parsed, &names).expect("config toast").text,
            "[accent]Config changed:[] [white]waveSpacing=30[]"
        );
        assert_eq!(parsed.payload.expect("payload").class, "ConfigEvent");
    }

    #[test]
    fn pump_replays_only_new_events() {
        let names = names();
        let mut pump = EventToasts::new();
        let payload = format!(
            "[{},{}]",
            row(2, EventKind::PlayerJoin, Some("aa11"), "Alice"),
            row(1, EventKind::MatchStart, None, "")
        );
        assert_eq!(pump.apply_json(&payload, &names), 2);
        assert_eq!(pump.watermark(), Some(2));
        let toasts = pump.take();
        assert_eq!(toasts.len(), 2);
        // Sorted by id: the lifecycle line first, then the join.
        assert_eq!(toasts[0].text, "[accent]Game started[]");
        assert_eq!(toasts[1].text, "[lightgray]Alice joined[]");
        assert!(pump.take().is_empty());

        // A replay of the same rows produces nothing.
        assert_eq!(pump.apply_json(&payload, &names), 0);
        // A new row does.
        let next = format!("[{}]", row(3, EventKind::PlayerChat, Some("aa11"), "hi"));
        assert_eq!(pump.apply_json(&next, &names), 0, "chat never toasts");
        assert_eq!(pump.watermark(), Some(3));
        let next = format!("[{}]", row(4, EventKind::PlayerBan, Some("bb22"), ""));
        assert_eq!(pump.apply_json(&next, &names), 1);
        assert_eq!(pump.pending(), 1);
        pump.clear();
        assert_eq!(pump.watermark(), None);
        assert_eq!(pump.pending(), 0);
    }
}
