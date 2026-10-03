// SPDX-License-Identifier: GPL-3.0-only

//! Env-gated integration tests against a **local** SpacetimeDB (plan 01 §7.1).
//!
//! Every test is `#[ignore]`d and additionally checks `MIND_STDB_IT=1`, so the
//! default `cargo test -p mind-stdb` never touches the network. These tests use
//! database `mindustry-it` (never `mindustry`) and only assert on rows they
//! create. Run with:
//!
//! ```text
//! MIND_STDB_IT=1 cargo test -p mind-stdb -- --ignored
//! ```

// Test code may unwrap/expect failures; the library lints stay denied.
#![allow(clippy::expect_used)]

use std::time::{Duration, Instant};

use mind_stdb::module_bindings::{
    CommandKind, Gamemode, LocalPlayerTableAccessor, MemberRole, MyMatchesTableAccessor,
    RelayConfigTableAccessor, Visibility,
};
use mind_stdb::{
    BinderOptions, CommandStream, ConnectionConfig, Connector, ConnectorEvent, RowChange, StdbMode,
    WaveName,
};

/// Integration tests run only with `MIND_STDB_IT=1` (defense in depth on top
/// of `#[ignore]`).
fn it_enabled() -> bool {
    matches!(std::env::var("MIND_STDB_IT"), Ok(value) if value == "1")
}

/// Per-client config: distinct `--pN`-style token suffix and store path.
fn it_config(suffix: &str) -> ConnectionConfig {
    ConnectionConfig {
        db_name: "mindustry-it".to_string(),
        token_append: Some(format!("_it{}_{suffix}", std::process::id())),
        token_store_path: Some(std::env::temp_dir().join("mind-stdb-it")),
        mode: StdbMode::Online,
        ..ConnectionConfig::local()
    }
}

fn pump_until(
    connector: &mut Connector,
    deadline: Instant,
    mut done: impl FnMut(&Connector) -> bool,
) -> bool {
    while Instant::now() < deadline {
        connector.pump();
        if done(connector) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn deadline(seconds: u64) -> Instant {
    Instant::now() + Duration::from_secs(seconds)
}

/// Pumps two connections until `done` or the deadline; reducer sends only flush
/// while their own connection is pumped.
fn pump_pair_until(
    first: &mut Connector,
    second: &mut Connector,
    deadline: Instant,
    mut done: impl FnMut() -> bool,
) -> bool {
    while Instant::now() < deadline {
        first.pump();
        second.pump();
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    false
}

fn first_match_id(changes: &[RowChange<mind_stdb::module_bindings::RelayMatch>]) -> Option<u64> {
    changes.iter().find_map(|change| match change {
        RowChange::Insert(row) | RowChange::Update { new: row, .. } => Some(row.match_id),
        RowChange::Delete(_) => None,
    })
}

#[test]
#[ignore = "requires a local SpacetimeDB with the mindustry-it db published (MIND_STDB_IT=1)"]
fn local_connect_applies_base_and_lobby_waves() {
    if !it_enabled() {
        return;
    }
    let mut connector = Connector::new(it_config("waves"));
    assert!(connector.connect().is_ok(), "initial connect failed");

    // Bind before the wave applies so subscription-applied inserts reach the
    // binder (live callbacks only; views have no primary key to replay).
    let player_binder = connector.bind::<LocalPlayerTableAccessor>("local_player");

    let timeout = deadline(20);
    assert!(
        pump_until(&mut connector, timeout, |conn| conn
            .is_applied(WaveName::Lobby)),
        "lobby wave did not apply; state {:?}",
        connector.state()
    );
    let events = connector.drain_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ConnectorEvent::WaveApplied(WaveName::Lobby))),
        "no WaveApplied(Lobby) event in {events:?}"
    );
    assert!(connector.local_identity().is_some());
    assert!(
        pump_until(&mut connector, timeout, |_| player_binder.pending() > 0),
        "no local_player row arrived on the lobby wave"
    );
    assert!(!player_binder.drain().is_empty());

    // Primary-key replay: the seeded `relay_config` singleton is already in the
    // client cache, so `bind_with_replay` must enqueue it without a transaction.
    let config_binder = connector.bind_with_replay::<RelayConfigTableAccessor>(
        "relay_config",
        BinderOptions {
            replay_existing: true,
            verbose: false,
        },
    );
    assert!(config_binder.pending() > 0, "relay_config was not replayed");
    assert!(
        config_binder
            .drain()
            .iter()
            .any(|change| matches!(change, RowChange::Insert(_)))
    );

    connector.disconnect();
    assert!(!connector.is_connected());
}

#[test]
#[ignore = "requires a local SpacetimeDB with the mindustry-it db published (MIND_STDB_IT=1)"]
fn two_clients_relay_ping_round_trip() {
    if !it_enabled() {
        return;
    }
    let mut host = Connector::new(it_config("host"));
    let mut guest = Connector::new(it_config("guest"));
    assert!(host.connect().is_ok(), "host connect failed");
    assert!(guest.connect().is_ok(), "guest connect failed");

    // Bind `my_matches` before the Lobby wave applies (view, live-only).
    let matches_host = host.bind::<MyMatchesTableAccessor>("my_matches");
    let timeout = deadline(20);
    assert!(pump_until(&mut host, timeout, |conn| conn.is_applied(WaveName::Lobby)));
    assert!(pump_until(&mut guest, timeout, |conn| conn.is_applied(WaveName::Lobby)));

    assert!(
        host.create_match(
            "it-ping-map",
            42,
            Gamemode::Survival,
            "survival",
            Visibility::Public,
            None,
            8,
            "{}",
            "",
            0,
            Vec::new(),
        )
        .is_ok(),
        "create_match failed"
    );
    assert!(
        pump_until(&mut host, timeout, |_| matches_host.pending() > 0),
        "creator never saw the match in my_matches"
    );
    let match_id = first_match_id(&matches_host.drain()).expect("match row");

    assert!(
        guest
            .join_match(match_id, None, "", 0, MemberRole::Player, Vec::new())
            .is_ok(),
        "join_match failed"
    );
    let mut stream_host = CommandStream::subscribe(&mut host, match_id);
    let mut stream_guest = CommandStream::subscribe(&mut guest, match_id);

    assert!(host.start_match(match_id, true).is_ok(), "start_match failed");
    assert!(pump_until(&mut host, timeout, |conn| conn.is_applied(WaveName::Game)));
    assert!(pump_until(&mut guest, timeout, |conn| conn.is_applied(WaveName::Game)));

    assert!(host.send_ping(match_id, 1, 42).is_ok(), "send_ping failed");
    assert!(
        pump_pair_until(&mut host, &mut guest, timeout, || {
            stream_host.pending() > 0 && stream_guest.pending() > 0
        }),
        "ping never reached both peers (host pending {}, guest pending {})",
        stream_host.pending(),
        stream_guest.pending()
    );
    let ping = stream_guest.drain().into_iter().next().expect("guest row");
    assert_eq!(ping.kind, CommandKind::Ping(42));
    assert_eq!(ping.match_id, match_id);
    assert_eq!(stream_guest.order_error(), None);
    assert_eq!(stream_guest.applied_count(), 1);
    assert_eq!(stream_host.drain().len(), 1);
    assert_eq!(stream_host.order_error(), None);

    host.disconnect();
    guest.disconnect();
}

#[test]
#[ignore = "requires a local SpacetimeDB with the mindustry-it db published (MIND_STDB_IT=1)"]
fn relay_rejects_non_member_and_rate_limit() {
    if !it_enabled() {
        return;
    }
    let mut host = Connector::new(it_config("rlhost"));
    let mut guest = Connector::new(it_config("rlguest"));
    assert!(host.connect().is_ok());
    assert!(guest.connect().is_ok());

    let matches_host = host.bind::<MyMatchesTableAccessor>("my_matches");
    let timeout = deadline(20);
    assert!(pump_until(&mut host, timeout, |conn| conn.is_applied(WaveName::Lobby)));
    assert!(pump_until(&mut guest, timeout, |conn| conn.is_applied(WaveName::Lobby)));
    assert!(
        host.create_match(
            "it-rate-map",
            7,
            Gamemode::Survival,
            "survival",
            Visibility::Public,
            None,
            8,
            "{}",
            "",
            0,
            Vec::new(),
        )
        .is_ok()
    );
    assert!(pump_until(&mut host, timeout, |_| matches_host.pending() > 0));
    let match_id = first_match_id(&matches_host.drain()).expect("match row");

    // Non-member probe before start: reducer must reject it on the server.
    assert!(
        guest.send_ping(match_id, 0, 1).is_ok(),
        "send itself should not fail"
    );
    let mut stream_host = CommandStream::subscribe(&mut host, match_id);
    assert!(host.start_match(match_id, true).is_ok());
    assert!(pump_until(&mut host, timeout, |conn| conn.is_applied(WaveName::Game)));
    // Give the rejected probe time to (not) show up.
    let probe_window = deadline(2);
    while Instant::now() < probe_window {
        host.pump();
        guest.pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    let observed: Vec<_> = stream_host
        .drain()
        .into_iter()
        .filter(|row| row.kind == CommandKind::Ping(1))
        .collect();
    assert!(
        observed.is_empty(),
        "non-member command was relayed: {observed:?}"
    );

    // Member rate limit: send beyond the default cap in one burst; some must be
    // rejected. `commands_per_second` is seeded to 600 by `init`.
    const SENT: u64 = 700;
    assert!(
        guest
            .join_match(match_id, None, "", 0, MemberRole::Player, Vec::new())
            .is_ok()
    );
    let mut stream_guest = CommandStream::subscribe(&mut guest, match_id);
    assert!(pump_until(&mut guest, timeout, |conn| conn.is_applied(WaveName::Game)));
    for nonce in 0..SENT {
        guest.pump();
        assert!(guest.send_ping(match_id, nonce, nonce).is_ok());
    }
    let settle = deadline(30);
    while Instant::now() < settle {
        host.pump();
        guest.pump();
        if stream_guest.pending() > 0 {
            stream_guest.drain();
        }
        if stream_guest.applied_count() >= SENT {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let applied = stream_guest.applied_count();
    assert!(applied > 0, "no command was relayed");
    assert!(
        applied < SENT,
        "rate limit accepted all {SENT} commands in a burst"
    );
    assert_eq!(stream_guest.order_error(), None);

    host.disconnect();
    guest.disconnect();
}
