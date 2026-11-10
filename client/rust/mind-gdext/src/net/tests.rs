// SPDX-License-Identifier: GPL-3.0-only

//! Unit tests for the net free helpers and the roster/chat projections.
//! Upstream: none (port-only; unit tests for the net helpers)

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::DEFAULT_SERVER_ADDRESS;
use super::adapters::{
    chat_facade_json, events_facade_json, is_identity_hex, parse_server_address, relay_trace_info,
    resolve_action_target,
};
use mind_stdb::module_bindings::{
    ChatKind, EventKind, MatchChat, MatchEvent, MemberRole, Player, RelayMember,
};
use mind_stdb::{ChatLog, EventBus, Identity, Roster, RowChange, Timestamp};

fn identity(byte: u8) -> Identity {
    Identity::from_byte_array([byte; 32])
}

fn member(byte: u8) -> RelayMember {
    let now = Timestamp::from_micros_since_unix_epoch(1_000_000);
    RelayMember {
        member_id: u64::from(byte),
        match_id: 7,
        identity: identity(byte),
        joined_at: now,
        role: MemberRole::Player,
        team: Some(1),
        connected: true,
        ready: true,
        last_seen_at: now,
        kicked_reason: None,
    }
}

fn roster_player(byte: u8, username: &str) -> Player {
    let now = Timestamp::from_micros_since_unix_epoch(2_000_000);
    Player {
        identity: identity(byte),
        username: username.to_owned(),
        created_at: now,
        last_seen_at: now,
        protocol_version: 1,
        color: 0xffff_ffff,
    }
}

/// The admin roster dialogs dispatch `unban`/`unadmin` on a `PlayerInfo.id`
/// (`BansDialog.java:37-42` / `AdminsDialog.java:37-46`); that identity is
/// normally offline, so it must resolve without a `relay_member` row.
#[test]
fn player_actions_resolve_offline_identity_hex() {
    let mut roster = Roster::new();
    roster.apply_member(RowChange::Insert(member(2)));
    roster.apply_player(RowChange::Insert(roster_player(2, "Alice")));
    let alice = identity(2).to_hex().to_string();
    // Roster name/identity-prefix lookups still work for live rows.
    assert_eq!(resolve_action_target(&roster, "Alice"), Some(alice.clone()));
    assert_eq!(
        resolve_action_target(&roster, "alice "),
        Some(alice.clone())
    );
    assert_eq!(resolve_action_target(&roster, "ali"), None);
    assert_eq!(resolve_action_target(&roster, ""), None);
    // An identity with no member/player row resolves verbatim (the ban row
    // is the only record the dialogs have).
    let offline = identity(9).to_hex().to_string();
    assert_eq!(
        resolve_action_target(&roster, &offline),
        Some(offline.clone())
    );
    assert_eq!(
        resolve_action_target(&roster, &offline.to_ascii_uppercase()),
        Some(offline.clone()),
        "identities normalize to lowercase hex"
    );
    // Only a full 64-char hex takes the direct path: a short prefix is a
    // roster lookup, never a truncated identity.
    assert!(!is_identity_hex(&"ab".repeat(31)));
    assert!(!is_identity_hex(&format!("{}z", "ab".repeat(31))));
    assert!(is_identity_hex(&"AB".repeat(32)));
    assert_eq!(
        resolve_action_target(&roster, &"cd".repeat(31)),
        None,
        "63 chars is not an identity"
    );
}

fn chat_row(chat_id: u64, kind: ChatKind, text: &str) -> MatchChat {
    MatchChat {
        chat_id,
        match_id: 7,
        sender: identity(2),
        kind,
        team: Some(1),
        text: text.to_string(),
        target: None,
        sent_at: Timestamp::from_micros_since_unix_epoch(chat_id as i64 * 1_000_000),
    }
}

fn event_row(event_id: u64, kind: EventKind, detail: &str) -> MatchEvent {
    MatchEvent {
        event_id,
        match_id: 7,
        kind,
        actor: Some(identity(2)),
        target: None,
        detail: detail.to_string(),
        at: Timestamp::from_micros_since_unix_epoch(event_id as i64 * 1_000_000),
    }
}

/// The chat facade is a JSON array of `ChatLine::to_json` rows built from
/// `my_match_chat` bindings (PAR-0121/0125); rows the viewer may not see
/// never reach the array.
#[test]
fn chat_facade_json_projects_bound_rows() {
    let mut chat = ChatLog::new();
    chat.set_viewer(Some(identity(2)), Some(1), false);
    chat.apply(RowChange::Insert(chat_row(2, ChatKind::All, "second")));
    chat.apply(RowChange::Insert(chat_row(1, ChatKind::All, "first")));
    // Team-2 chat is invisible to a team-1 viewer.
    chat.apply(RowChange::Insert(MatchChat {
        team: Some(2),
        ..chat_row(3, ChatKind::Team, "other team")
    }));

    let json = chat_facade_json(&chat);
    let rows = json.as_array().expect("array");
    assert_eq!(rows.len(), 2, "only the visible rows");
    assert_eq!(rows[0]["id"], 1);
    assert_eq!(rows[0]["text"], "first");
    assert_eq!(rows[0]["kind"], "all");
    assert_eq!(rows[1]["id"], 2);
    assert_eq!(rows[0]["sender"], identity(2).to_hex().to_string());
    assert_eq!(
        chat.to_json(),
        json,
        "same projection as `ChatLog::to_json`"
    );
}

/// The toast facade is a JSON array of `ServerEvent::to_json` rows built
/// from `my_match_events` bindings (GAPS #37).
#[test]
fn events_facade_json_projects_bound_rows() {
    let mut events = EventBus::new();
    events.apply(RowChange::Insert(event_row(
        2,
        EventKind::PlayerJoin,
        "player joined",
    )));
    events.apply(RowChange::Insert(event_row(
        1,
        EventKind::MatchCreate,
        "match created",
    )));

    let json = events_facade_json(&events);
    let rows = json.as_array().expect("array");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], 1, "commit order, not insertion order");
    assert_eq!(rows[0]["type"], "WorldLoadEvent");
    assert_eq!(rows[0]["kind"], "MatchCreate");
    assert_eq!(rows[1]["type"], "PlayerJoin");
    assert_eq!(
        events.to_json(),
        json,
        "same projection as `EventBus::to_json`"
    );
}

/// PAR-0601: the `Call.traceInfo` payload projects the roster trace with
/// the relay's IP-less shape.
#[test]
fn relay_trace_info_projects_the_roster_trace() {
    let identity = "ab".repeat(32);
    let trace = mind_stdb::roster::TraceInfo {
        identity: identity.clone(),
        names: vec!["alice".to_string(), "alice2".to_string()],
        sessions: 3,
        connected: true,
        last_join_micros: Some(1),
        kicks: vec![("vote".to_string(), "cd".repeat(32), 2)],
        actions: Vec::new(),
        banned: false,
        whitelisted: false,
    };
    let wire = relay_trace_info(&trace);
    assert_eq!(wire.uuid.as_deref(), Some(identity.as_str()));
    assert_eq!(wire.times_joined, 3);
    assert_eq!(wire.times_kicked, 1);
    assert_eq!(
        wire.names,
        vec![Some("alice".to_string()), Some("alice2".to_string())]
    );
    assert_eq!(wire.ip, None, "no peer IP reaches a reducer");
    assert!(wire.ips.is_empty());
    assert!(!wire.modded && !wire.mobile);
}

#[test]
fn server_address_parsing() {
    assert_eq!(
        parse_server_address("127.0.0.1", "mindustry"),
        Some(("http://127.0.0.1:3000".to_string(), "mindustry".to_string()))
    );
    assert_eq!(
        parse_server_address("example.com:4000/other", "mindustry"),
        Some(("http://example.com:4000".to_string(), "other".to_string()))
    );
    assert_eq!(
        parse_server_address("https://db.example.com/prod", "mindustry"),
        Some((
            "https://db.example.com:3000".to_string(),
            "prod".to_string()
        ))
    );
    assert_eq!(parse_server_address("   ", "mindustry"), None);
    assert_eq!(parse_server_address("bad host", "mindustry"), None);
    // `stdb-server` setting shapes (I08 T1): host:port/db and host/db.
    assert_eq!(
        parse_server_address("127.0.0.1:3000/mindustry", "mindustry"),
        Some(("http://127.0.0.1:3000".to_string(), "mindustry".to_string()))
    );
    assert_eq!(
        parse_server_address("db.example.com/prod", "mindustry"),
        Some(("http://db.example.com:3000".to_string(), "prod".to_string()))
    );
}

#[test]
fn default_server_address_parses() {
    assert_eq!(
        parse_server_address(DEFAULT_SERVER_ADDRESS, "other"),
        Some(("http://127.0.0.1:3000".to_string(), "mindustry".to_string()))
    );
}
