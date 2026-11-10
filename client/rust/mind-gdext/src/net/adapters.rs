// SPDX-License-Identifier: GPL-3.0-only

//! Relay content/origin adapters over the live sim host, plus the free parsing/JSON helpers.
//! Upstream: core/src/mindustry/net/Net.java

use super::*;

impl MindNet {
    /// Mirrors public `all_matches` rows for the join-dialog browser.
    pub(super) fn drain_public_matches(&mut self) {
        let Some(binder) = self.public_matches_binder.as_ref() else {
            return;
        };
        for change in binder.drain() {
            match change {
                RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                    self.public_matches.insert(row.match_id, row);
                }
                RowChange::Delete(row) => {
                    self.public_matches.remove(&row.match_id);
                }
            }
        }
    }

    /// Publishes a scoped checksum every `CHECKSUM_INTERVAL_TICKS` while running.
    pub(super) fn maybe_publish_checksum(&mut self) {
        if self.session.state() != mind_stdb::session::SessionState::InGame {
            return;
        }
        let tick = self.sim_tick();
        const CHECKSUM_INTERVAL_TICKS: u64 = 120;
        if tick
            < self
                .last_checksum_tick
                .saturating_add(CHECKSUM_INTERVAL_TICKS)
        {
            return;
        }
        self.last_checksum_tick = tick;
        let Some(match_id) = self.session.match_id() else {
            return;
        };
        let checksum = self
            .sim_host
            .as_ref()
            .map(|host| host.bind().checksum_scoped_value())
            .unwrap_or(0);
        let command_id = self
            .runtime
            .as_ref()
            .map(|runtime| runtime.last_applied_command_id())
            .unwrap_or(0);
        if let Some(connector) = self.connector.as_mut()
            && connector.is_connected()
        {
            let scope = mind_stdb::scope_bits(true, false, false);
            if let Err(error) =
                connector.publish_checksum(match_id, command_id, tick, checksum, 1, scope)
            {
                log::warn!("MindNet: publish_checksum failed: {error}");
            }
        }
    }
}

/// Content-name resolution adapter over the live `MindSimHost` registry
/// (I08 T6).
pub(super) struct SimHostContent<'a>(pub(super) &'a MindSimHost);

impl relay::RelayContent for SimHostContent<'_> {
    fn block_id(&self, name: &str) -> Option<u16> {
        self.0
            .with_registry(|registry| registry.block_id(name).map(|id| id.raw()))
            .flatten()
    }

    fn block_name(&self, id: u16) -> Option<String> {
        self.0
            .with_registry(|registry| {
                registry
                    .entries(mind_core::content::ContentType::Block)
                    .into_iter()
                    .find(|entry| entry.id == id)
                    .and_then(|entry| entry.name.map(str::to_string))
            })
            .flatten()
    }

    fn item_id(&self, name: &str) -> Option<u16> {
        self.0
            .with_registry(|registry| registry.item_id(name).map(|id| id.raw()))
            .flatten()
    }

    fn item_name(&self, id: u16) -> Option<String> {
        self.0
            .with_registry(|registry| {
                registry
                    .entries(mind_core::content::ContentType::Item)
                    .into_iter()
                    .find(|entry| entry.id == id)
                    .and_then(|entry| entry.name.map(str::to_string))
            })
            .flatten()
    }
}

/// Sender team/slot resolution from the match member table (I08 T6).
pub(super) struct MemberOrigin<'a> {
    pub(super) members:
        &'a std::collections::BTreeMap<String, mind_stdb::module_bindings::RelayMember>,
    pub(super) default_team: u8,
}

impl relay::RelayOrigin for MemberOrigin<'_> {
    fn team_for(&self, sender: mind_stdb::Identity) -> u8 {
        self.members
            .get(&sender.to_hex().to_string())
            .and_then(|member| member.team)
            .unwrap_or(self.default_team)
    }

    fn player_for(&self, _sender: mind_stdb::Identity) -> Option<i32> {
        // The member table has no player slot; the port uses slot 0 for the
        // acting player like the local path (`desktop.rs:2955`).
        Some(0)
    }
}

pub(super) fn collect_args(args: &[GString]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

pub(super) fn parse_gamemode(name: &str) -> Gamemode {
    match name {
        "sandbox" => Gamemode::Sandbox,
        "attack" => Gamemode::Attack,
        "pvp" => Gamemode::Pvp,
        "editor" => Gamemode::Editor,
        _ => Gamemode::Survival,
    }
}

pub(super) fn parse_visibility(name: &str) -> Visibility {
    if name.eq_ignore_ascii_case("unlisted") {
        Visibility::Unlisted
    } else {
        Visibility::Public
    }
}

/// Parses a direct-connect address (`host[:port][/db]`, `http://` optional)
/// into a SpacetimeDB host URL and database name. A missing port defaults to
/// the local dev port (3000); a missing database keeps `default_db`.
pub(super) fn parse_server_address(address: &str, default_db: &str) -> Option<(String, String)> {
    let address = address.trim();
    if address.is_empty() {
        return None;
    }
    let (scheme, rest) = match address.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() => (format!("{scheme}://"), rest),
        _ => ("http://".to_string(), address),
    };
    let (authority, db) = match rest.split_once('/') {
        Some((authority, tail)) => {
            let db = tail.split('/').next().unwrap_or("").trim();
            (
                authority,
                if db.is_empty() {
                    default_db.to_string()
                } else {
                    db.to_string()
                },
            )
        }
        None => (rest, default_db.to_string()),
    };
    let authority = authority.trim();
    if authority.is_empty() || authority.contains(char::is_whitespace) {
        return None;
    }
    let host = if authority.contains(':') {
        format!("{scheme}{authority}")
    } else {
        format!("{scheme}{authority}:3000")
    };
    Some((host, db))
}

/// Bounded reachability gate for the SDK's blocking WebSocket connect: parses
/// the authority out of a host URL and tries each resolved address with a
/// 400 ms TCP connect. Unreachable addresses never reach
/// `DbConnectionBuilder::build` (which `block_on`s the caller).
pub(super) fn probe_reachable(host: &str) -> bool {
    let authority = host
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let authority = authority.split('/').next().unwrap_or(authority);
    let Some((name, port)) = authority.rsplit_once(':') else {
        return false;
    };
    let Ok(port) = port.parse::<u16>() else {
        return false;
    };
    let Ok(addresses) = (name, port).to_socket_addrs() else {
        return false;
    };
    addresses.into_iter().any(|address| {
        std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(400))
            .is_ok()
    })
}

/// JSON array of the retained `ChatLine` projections (`MindNet.chat_json`).
///
/// Kept as a free function so the `chatui` facade shape is unit-testable from
/// rows without a live `MindNet` node (no connector/server needed).
pub(super) fn chat_facade_json(chat: &ChatLog) -> serde_json::Value {
    serde_json::Value::Array(chat.lines().iter().map(|line| line.to_json()).collect())
}

/// JSON array of the retained `ServerEvent` projections
/// (`MindNet.events_json`).
pub(super) fn events_facade_json(events: &EventBus) -> serde_json::Value {
    serde_json::Value::Array(
        events
            .events()
            .iter()
            .map(|event| event.to_json())
            .collect(),
    )
}

/// Port player id for a relay member row (`PlayerJoin`/`PlayerLeave` payload).
///
/// Java's `Player.id` is a small per-connection counter (`NetServer` join
/// path); the relay's auto-inc `relay_member.member_id` is the closest stable
/// identity the client sees, so it is truncated to the wire `u32`.
pub(super) fn roster_player_id(member_id: u64) -> u32 {
    u32::try_from(member_id).unwrap_or(u32::MAX)
}

/// Resolves a player-action target the way the console and the roster dialogs
/// address one.
///
/// A full identity hex wins verbatim: Java's `BansDialog`/`AdminsDialog` call
/// `unbanPlayerID`/`unAdminPlayer` with `PlayerInfo.id`
/// (`BansDialog.java:37-42`, `AdminsDialog.java:37-46`), and an offline
/// banned/unadmin'd identity has no `relay_member` row for [`Roster::find`] to
/// match. Every other target keeps the name/identity-prefix roster lookup.
pub(super) fn resolve_action_target(roster: &Roster, target: &str) -> Option<String> {
    let target = target.trim();
    if is_identity_hex(target) {
        return Some(target.to_ascii_lowercase());
    }
    roster.find(target).map(|entry| entry.identity)
}

/// Whether `text` is a full 64-char identity hex (`PlayerInfo.id`).
pub(super) fn is_identity_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// `Administration.TraceInfo` wire payload for `Call.traceInfo` (PAR-0601),
/// projected from the relay roster trace.
///
/// The relay has no peer IP (`D2`: reducers never see a socket), so `ip`/`ips`
/// stay empty; `modded`/`mobile` have no relay source and default to `false`.
/// The uuid/counters/names come from the roster aggregation
/// (`Roster::trace`), which mirrors `Administration.PlayerInfo`.
pub(super) fn relay_trace_info(trace: &mind_stdb::TraceInfo) -> mind_stdb::typeio::TraceInfo {
    mind_stdb::typeio::TraceInfo {
        ip: None,
        uuid: Some(trace.identity.clone()),
        locale: None,
        modded: false,
        mobile: false,
        times_joined: i32::try_from(trace.sessions).unwrap_or(i32::MAX),
        times_kicked: i32::try_from(trace.kicks.len()).unwrap_or(i32::MAX),
        ips: Vec::new(),
        names: trace.names.iter().cloned().map(Some).collect(),
    }
}
