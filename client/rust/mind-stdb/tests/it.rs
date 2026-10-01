// SPDX-License-Identifier: GPL-3.0-only

//! Env-gated integration tests against a **local** SpacetimeDB (plan 01 §7.1).
//!
//! Every test is `#[ignore]`d and additionally checks `MIND_STDB_IT=1`, so the
//! default `cargo test -p mind-stdb` never touches the network. These tests use
//! database `mindustry-it` (never `mindustry`) and only assert on rows they
//! create. Run with:
//!
//! ```text
//! MIND_STDB_IT=1 cargo test -p mind-stdb -- --ignored local_connect_applies_base_and_lobby_waves
//! ```

use std::time::{Duration, Instant};

use mind_stdb::module_bindings::{LocalPlayerTableAccessor, RelayConfigTableAccessor};
use mind_stdb::{BinderOptions, ConnectionConfig, Connector, ConnectorEvent, StdbMode, WaveName};

/// Integration tests run only with `MIND_STDB_IT=1` (defense in depth on top
/// of `#[ignore]`).
fn it_enabled() -> bool {
    matches!(std::env::var("MIND_STDB_IT"), Ok(value) if value == "1")
}

fn it_config() -> ConnectionConfig {
    ConnectionConfig {
        db_name: "mindustry-it".to_string(),
        token_append: Some(format!("_it{}", std::process::id())),
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

#[test]
#[ignore = "requires a local SpacetimeDB with the mindustry-it db published (MIND_STDB_IT=1)"]
fn local_connect_applies_base_and_lobby_waves() {
    if !it_enabled() {
        return;
    }
    let mut connector = Connector::new(it_config());
    assert!(connector.connect().is_ok(), "initial connect failed");

    // Bind before the wave applies so subscription-applied inserts reach the
    // binder (live callbacks only; views have no primary key to replay).
    let player_binder = connector.bind::<LocalPlayerTableAccessor>("local_player");

    let deadline = Instant::now() + Duration::from_secs(20);
    assert!(
        pump_until(&mut connector, deadline, |conn| conn
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
    let deadline = Instant::now() + Duration::from_secs(20);
    assert!(
        pump_until(&mut connector, deadline, |_| player_binder.pending() > 0),
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
            .any(|change| matches!(change, mind_stdb::RowChange::Insert(_)))
    );

    connector.disconnect();
    assert!(!connector.is_connected());
}
