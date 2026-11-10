// SPDX-License-Identifier: GPL-3.0-only

//! Connector bring-up and the short-lived server browser probes.
//! Upstream: core/src/mindustry/net/Net.java

use super::adapters::{parse_server_address, probe_reachable};
use super::*;

impl MindNet {
    /// (Re)builds the connector for `host`/`db_name` in Online mode, attaches
    /// every binder and opens the link. Clears the per-match mirrors. Returns
    /// `false` when the address is unreachable or the socket cannot be opened
    /// now (the connector still schedules its retry); the failure is shown as
    /// `@connectfail` (`Net.java:133`).
    pub(super) fn open_online(&mut self, host: &str, db_name: &str) -> bool {
        // `DbConnectionBuilder::build` runs `block_on` on the calling thread
        // (`spacetimedb-sdk-2.10.1/src/db_connection.rs:979`), so connecting to
        // a black-holed address would freeze the frame loop for the OS connect
        // timeout. Gate on a bounded TCP probe first.
        if !probe_reachable(host) {
            self.show_error("@connectfail", &format!("unreachable `{host}`"));
            return false;
        }
        if let Some(connector) = self.connector.as_mut() {
            connector.disconnect();
        }
        self.runtime = None;
        self.sender = None;
        self.remote_players.clear();
        self.public_matches.clear();
        let config = ConnectionConfig {
            host: host.to_string(),
            db_name: db_name.to_string(),
            token_append: self.token_append.clone(),
            token_store_path: self.token_store_path.clone(),
            mode: StdbMode::Online,
            ..ConnectionConfig::default()
        };
        let mut connector = Connector::new(config);
        self.attach_binders(&mut connector);
        self.connector = Some(connector);
        self.host = GString::from(host);
        self.db_name = GString::from(db_name);
        self.online_mode = true;
        let error = self
            .connector
            .as_mut()
            .and_then(|connector| connector.connect().err().map(|error| error.to_string()));
        if let Some(error) = error {
            self.show_error("@connectfail", &error);
            return false;
        }
        true
    }

    /// Pushes the stored player identity through the identity reducers when
    /// the client is connected. `false` when nothing was stored or the link
    /// is down (the values stay queued for the next connect).
    pub(super) fn push_identity(&mut self) -> bool {
        let Some((name, color)) = self.desired_identity.clone() else {
            return false;
        };
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        if !connector.is_connected() {
            return false;
        }
        let mut ok = true;
        if let Err(error) = connector.set_username(&name) {
            log::warn!("MindNet.set_username failed: {error}");
            ok = false;
        }
        if let Err(error) = connector.set_color(color) {
            log::warn!("MindNet.set_color failed: {error}");
            ok = false;
        }
        ok
    }

    /// Normalizes `address` (`host[:port][/db]`, scheme optional) to the
    /// probe key form `http(s)://host:port/db`.
    pub(super) fn probe_key(&self, address: &str) -> Option<String> {
        let default_db = self.db_name.to_string();
        let (host, db_name) = parse_server_address(address, &default_db)?;
        Some(format!("{host}/{db_name}"))
    }

    /// Ensures a probe exists for `address` (starting one when missing, LRU
    /// eviction at [`PROBE_LIMIT`]) and returns its key.
    pub(super) fn ensure_probe(&mut self, address: &str) -> Option<String> {
        let key = self.probe_key(address)?;
        if let Some(probe) = self.probes.get_mut(&key) {
            probe.last_used = std::time::Instant::now();
            return Some(key);
        }
        while self.probes.len() >= PROBE_LIMIT {
            let Some(oldest) = self
                .probes
                .iter()
                .min_by_key(|(_, probe)| probe.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.probes.remove(&oldest);
        }
        let default_db = self.db_name.to_string();
        let (host, db_name) = parse_server_address(address, &default_db)?;
        // Same blocking-connect gate as `open_online`: an unreachable address
        // becomes a failed probe without ever touching the SDK.
        let reachable = probe_reachable(&host);
        let config = ConnectionConfig {
            host,
            db_name,
            token_append: self.token_append.clone(),
            token_store_path: self.token_store_path.clone(),
            mode: StdbMode::Online,
            ..ConnectionConfig::default()
        };
        let mut connector = Connector::new(config);
        let matches_binder = Some(connector.bind::<AllMatchesTableAccessor>("all_matches"));
        let failed = !reachable || connector.connect().is_err();
        self.probes.insert(
            key.clone(),
            ServerProbe {
                connector,
                matches_binder,
                started: std::time::Instant::now(),
                ping_ms: -1,
                state: if failed {
                    ProbeState::Failed
                } else {
                    ProbeState::Connecting
                },
                matches: std::collections::BTreeMap::new(),
                last_used: std::time::Instant::now(),
            },
        );
        Some(key)
    }

    /// Pumps every browser probe once per frame and drops idle ones.
    pub(super) fn pump_probes(&mut self) {
        for probe in self.probes.values_mut() {
            probe.connector.pump();
            for event in probe.connector.drain_events() {
                if matches!(
                    event,
                    ConnectorEvent::ConnectError { .. } | ConnectorEvent::Disconnected { .. }
                ) && probe.state == ProbeState::Connecting
                {
                    probe.state = ProbeState::Failed;
                }
            }
            if let Some(binder) = probe.matches_binder.as_ref() {
                for change in binder.drain() {
                    match change {
                        RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                            probe.matches.insert(row.match_id, row);
                        }
                        RowChange::Delete(row) => {
                            probe.matches.remove(&row.match_id);
                        }
                    }
                }
            }
            if probe.state == ProbeState::Connecting {
                if probe.connector.is_applied(WaveName::Lobby) {
                    probe.ping_ms =
                        probe.started.elapsed().as_millis().min(i64::MAX as u128) as i64;
                    probe.state = ProbeState::Ready;
                } else if probe.started.elapsed().as_secs() >= PROBE_TIMEOUT_SECS {
                    probe.state = ProbeState::Failed;
                }
            }
        }
        self.probes
            .retain(|_, probe| probe.last_used.elapsed().as_secs() < PROBE_IDLE_SECS);
    }
}
