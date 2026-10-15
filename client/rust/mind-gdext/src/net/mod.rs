// SPDX-License-Identifier: GPL-3.0-only

//! `MindNet` — the plan-21 multiplayer autoload (plan §3.15).
//!
//! Owns the `MatchSession`, the STDB `Connector`, the `RelayRuntime` mapping and
//! the `CommandSender`. It contains no sim rules: foreign commands are pushed
//! into `MindSimHost.pending_commands` and applied at the next tick boundary.
//!
//! Orchestration note: plan 00's `StdbConnector` already owns a connector for
//! the MCP/dev path. To avoid double-pumping, **either** add `MindNet` to
//! `client/scenes/game.tscn`/`project.godot` and drop `StdbConnector`, **or**
//! keep `StdbConnector` for dev and instantiate `MindNet` lazily from the UI
//! (plan 14). This lane implements the Rust node; the scene wiring is left to
//! the orchestrator because `game.tscn` is shared.

pub mod relay;

use std::net::ToSocketAddrs;

use godot::classes::notify::NodeNotification;
use godot::classes::{INode, Node, Os, ProjectSettings};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_stdb::binder::TableBinder;
use mind_stdb::checksum::{ChecksumMonitor, ChecksumReport};
use mind_stdb::commands::CommandSender;
use mind_stdb::module_bindings::{
    AllMatchesTableAccessor, BreakBlock, ChatKind, CommandKind, Custom, Gamemode, MatchPlayerState,
    MatchStatus, MemberRole, MyMatchChecksumsTableAccessor, MyMatchMembersTableAccessor,
    MyMatchPlayerStatesTableAccessor, MyMatchSnapshotChunksTableAccessor,
    MyMatchSnapshotRequestsTableAccessor, MyMatchSnapshotsTableAccessor, MyMatchStateTableAccessor,
    MyMatchTableAccessor, MyMatchesTableAccessor, PlaceBlock, RelayMatch, SnapshotKind, Visibility,
};
use mind_stdb::rows::RowView;
use mind_stdb::snapshot::{DynamicSnapshot, SNAPSHOT_FORMAT, SnapshotHeader};
use mind_stdb::transport::StdbTransport;
use mind_stdb::{
    ConnectionConfig, Connector, ConnectorEvent, HostParams, MatchSession, RowChange, SessionState,
    StdbMode, WaveName, parse_player_suffix_from,
};

use crate::settings_hub;
use crate::sim_host::MindSimHost;

use self::relay::RelayRuntime;

/// `stdb-server` setting default: the local harness (`tools/mp-local.sh up`)
/// publishes `mindustry` to a single-node server on 3000.
const DEFAULT_SERVER_ADDRESS: &str = "127.0.0.1:3000/mindustry";
/// Settings key for the multiplayer server address (`host[:port][/db]`).
const SERVER_SETTING_KEY: &str = "stdb-server";
/// Per-server browser probes kept alive at once (`JoinDialog` server rows).
const PROBE_LIMIT: usize = 6;
/// A probe that has not reached the lobby wave within this window fails.
const PROBE_TIMEOUT_SECS: u64 = 5;
/// Idle probes are dropped after this window (the dialog polls each frame).
const PROBE_IDLE_SECS: u64 = 30;

/// State of one short-lived browser probe (`ping_server`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeState {
    /// WebSocket handshake / subscription in flight.
    Connecting,
    /// Lobby wave applied; `ping_ms` and `matches` are live.
    Ready,
    /// Connect failed or the lobby wave timed out.
    Failed,
}

impl ProbeState {
    /// Stable debug name for the dialog JSON.
    fn name(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

/// The live world a host captured for [`MindNet::host_current_world`]: the
/// match identity plus the snapshot header inputs (I08 T4).
#[derive(Debug, Clone)]
struct HostedWorld {
    map_id: String,
    map_seed: u64,
    build_id: String,
    content_hash: u64,
    rules_json: String,
}

/// An in-flight joiner snapshot load (I08 T5): the newest host snapshot's
/// chunks are reassembled and applied to the live world.
struct JoinLoad {
    target_snapshot_id: u64,
    expected_chunks: u16,
    chunks: std::collections::BTreeMap<u16, Vec<u8>>,
}

/// A secondary `Connector` used to browse one server address without touching
/// the active match connection (I08 T3): the lobby wave (`all_matches`) only,
/// pumped once per frame by `pump_probes`.
struct ServerProbe {
    connector: Connector,
    matches_binder: Option<TableBinder<AllMatchesTableAccessor>>,
    started: std::time::Instant,
    /// Lobby-subscription round trip once [`ProbeState::Ready`]; -1 before.
    ping_ms: i64,
    state: ProbeState,
    matches: std::collections::BTreeMap<u64, RelayMatch>,
    last_used: std::time::Instant,
}

/// Multiplayer autoload: session + relay pump ownership (plan §3.15).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindNet {
    base: Base<Node>,
    /// SpacetimeDB host URL.
    #[export]
    pub host: GString,
    /// Database name.
    #[export]
    pub db_name: GString,
    /// Stay offline unless `--db`/`--pN` opt in (single-player invariant).
    #[export]
    pub offline: bool,
    connector: Option<Connector>,
    session: MatchSession,
    sim_host: Option<Gd<MindSimHost>>,
    runtime: Option<RelayRuntime>,
    sender: Option<CommandSender>,
    match_binder: Option<TableBinder<MyMatchTableAccessor>>,
    members_binder: Option<TableBinder<MyMatchMembersTableAccessor>>,
    state_binder: Option<TableBinder<MyMatchStateTableAccessor>>,
    players_binder: Option<TableBinder<MyMatchPlayerStatesTableAccessor>>,
    checksums_binder: Option<TableBinder<MyMatchChecksumsTableAccessor>>,
    snapshots_binder: Option<TableBinder<MyMatchSnapshotsTableAccessor>>,
    public_matches_binder: Option<TableBinder<AllMatchesTableAccessor>>,
    /// Lobby-wave `my_matches` rows: the host's match id arrives here before
    /// the game wave exists (I08 T4).
    my_matches_binder: Option<TableBinder<MyMatchesTableAccessor>>,
    /// Snapshot-wave chunk rows for the joiner's download (I08 T5).
    snapshot_chunks_binder: Option<TableBinder<MyMatchSnapshotChunksTableAccessor>>,
    /// Host-only pending snapshot requests (I08 T5).
    snapshot_requests_binder: Option<TableBinder<MyMatchSnapshotRequestsTableAccessor>>,
    public_matches: std::collections::BTreeMap<u64, RelayMatch>,
    token_append: Option<String>,
    token_store_path: Option<std::path::PathBuf>,
    last_match_json: String,
    last_members_json: String,
    last_state_json: String,
    last_checksum: String,
    authority: String,
    queue_depth: i64,
    remote_players: std::collections::BTreeMap<String, MatchPlayerState>,
    checksum_monitor: ChecksumMonitor,
    snapshot_progress: f32,
    last_checksum_tick: u64,
    /// Connector was built for online play (boot `--db`/`--pN` or
    /// [`Self::ensure_online`]); single-player boots stay offline.
    online_mode: bool,
    /// Last `session_changed` payload, so transitions emit exactly once.
    last_session_state: SessionState,
    /// Java `player.name` + `player.color` (`0xRRGGBBAA`), pushed by
    /// [`Self::set_player_identity`] and re-sent on every connect.
    desired_identity: Option<(String, u32)>,
    /// Per-address short-lived browser probes (`ping_server`), keyed by the
    /// normalized `host/db` address.
    probes: std::collections::BTreeMap<String, ServerProbe>,
    /// Live world captured by [`Self::host_current_world`] (snapshot header
    /// inputs); cleared when the host flow fails.
    hosted: Option<HostedWorld>,
    /// `host_current_world` asked for a start; the `my_matches` row drives
    /// `start_match(true)` once the match id is known (I08 T4).
    pending_host_start: bool,
    /// A join is loading the host snapshot (I08 T5): drives the request and
    /// the apply once every chunk arrived.
    pending_join: bool,
    /// In-flight joiner snapshot download state.
    join_load: Option<JoinLoad>,
    /// Match id the current `runtime`/`sender` were built for; a new match
    /// rebuilds both (leave → re-host/join must not keep the old id).
    relay_match_id: u64,
}

#[godot_api]
impl INode for MindNet {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            host: GString::from("http://127.0.0.1:3000"),
            db_name: GString::from("mindustry"),
            offline: true,
            connector: None,
            session: MatchSession::new(),
            sim_host: None,
            runtime: None,
            sender: None,
            match_binder: None,
            members_binder: None,
            state_binder: None,
            players_binder: None,
            checksums_binder: None,
            snapshots_binder: None,
            public_matches_binder: None,
            my_matches_binder: None,
            snapshot_chunks_binder: None,
            snapshot_requests_binder: None,
            public_matches: std::collections::BTreeMap::new(),
            token_append: None,
            token_store_path: None,
            last_match_json: String::new(),
            last_members_json: String::new(),
            last_state_json: String::new(),
            last_checksum: String::new(),
            authority: "relay".to_string(),
            queue_depth: 0,
            remote_players: std::collections::BTreeMap::new(),
            checksum_monitor: ChecksumMonitor::new(),
            snapshot_progress: 0.0,
            last_checksum_tick: 0,
            online_mode: false,
            last_session_state: SessionState::Offline,
            desired_identity: None,
            probes: std::collections::BTreeMap::new(),
            hosted: None,
            pending_host_start: false,
            pending_join: false,
            join_load: None,
            relay_match_id: 0,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        let mut connected_now = false;
        if let Some(connector) = self.connector.as_mut() {
            connector.pump();
            for event in connector.drain_events() {
                match event {
                    ConnectorEvent::Connected { identity } => {
                        connected_now = true;
                        self.session.set_local_identity(identity.identity);
                        self.checksum_monitor.set_local(identity.identity);
                        if let Some(runtime) = self.runtime.as_mut() {
                            runtime.set_local_identity(identity.identity);
                        }
                    }
                    ConnectorEvent::Disconnected { .. } => self.session.on_disconnected(),
                    ConnectorEvent::Resync => self.session.on_resync(),
                    _ => {}
                }
            }
            self.drain_match_rows();
            self.drain_my_matches();
            self.drain_members_rows();
            self.drain_state_rows();
            self.drain_public_matches();
            self.drain_player_states();
            self.drain_checksums();
            self.drain_snapshots();
            self.drain_snapshot_chunks();
            self.drain_snapshot_requests();
            self.drain_relay();
            self.maybe_publish_checksum();
        }
        if connected_now && self.push_identity() {
            log::info!("MindNet: player identity pushed after connect");
        }
        self.pump_probes();
        self.notify_session_state();
    }

    fn exit_tree(&mut self) {
        if let Some(connector) = self.connector.as_mut() {
            connector.disconnect();
        }
    }
}

#[godot_api]
impl MindNet {
    /// Rebuilds the session/connector (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`). Any previous
    /// connector is dropped and the binder handles are overwritten, so a re-run
    /// never leaks a link.
    fn bootstrap(&mut self) {
        if let Some(connector) = self.connector.as_mut() {
            connector.disconnect();
        }

        let engine = collect_args(Os::singleton().get_cmdline_args().as_slice());
        let user = collect_args(Os::singleton().get_cmdline_user_args().as_slice());
        let suffix = parse_player_suffix_from(&engine, &user);
        let db_arg = engine.iter().any(|arg| arg == "--db") || user.iter().any(|arg| arg == "--db");
        let online = !self.offline || db_arg || suffix.is_some();

        let token_dir = ProjectSettings::singleton().globalize_path("user://");
        self.token_append = suffix;
        self.token_store_path = Some(std::path::PathBuf::from(token_dir.to_string()));
        let config = ConnectionConfig {
            host: self.host.to_string(),
            db_name: self.db_name.to_string(),
            token_append: self.token_append.clone(),
            token_store_path: self.token_store_path.clone(),
            mode: if online {
                StdbMode::Online
            } else {
                StdbMode::Offline
            },
            ..ConnectionConfig::default()
        };
        let mut connector = Connector::new(config);
        self.attach_binders(&mut connector);
        if online {
            if let Err(error) = connector.connect() {
                log::warn!("MindNet connect failed: {error}");
            }
        } else {
            log::info!("MindNet offline (single-player default)");
        }
        self.connector = Some(connector);
        self.online_mode = online;
        self.sim_host = self
            .base()
            .try_get_node_as::<MindSimHost>("../SimHost")
            .or_else(|| {
                self.base()
                    .try_get_node_as::<MindSimHost>("/root/Spine/SimHost")
            });
    }

    /// Registers the table binders on a freshly built connector. Shared by
    /// `bootstrap()` and `connect_to()`; every binder is overwritten, so a
    /// rebuilt connector never leaves a stale link.
    fn attach_binders(&mut self, connector: &mut Connector) {
        self.match_binder = Some(connector.bind::<MyMatchTableAccessor>("my_match"));
        self.members_binder =
            Some(connector.bind::<MyMatchMembersTableAccessor>("my_match_members"));
        self.state_binder = Some(connector.bind::<MyMatchStateTableAccessor>("my_match_state"));
        self.players_binder =
            Some(connector.bind::<MyMatchPlayerStatesTableAccessor>("my_match_player_states"));
        self.checksums_binder =
            Some(connector.bind::<MyMatchChecksumsTableAccessor>("my_match_checksums"));
        self.snapshots_binder =
            Some(connector.bind::<MyMatchSnapshotsTableAccessor>("my_match_snapshots"));
        self.public_matches_binder = Some(connector.bind::<AllMatchesTableAccessor>("all_matches"));
        self.my_matches_binder = Some(connector.bind::<MyMatchesTableAccessor>("my_matches"));
        self.snapshot_chunks_binder =
            Some(connector.bind::<MyMatchSnapshotChunksTableAccessor>("my_match_snapshot_chunks"));
        self.snapshot_requests_binder = Some(
            connector.bind::<MyMatchSnapshotRequestsTableAccessor>("my_match_snapshot_requests"),
        );
    }

    /// Session state changed (`offline`/`browsing`/`in_lobby`/`in_game`/...).
    /// Emitted from [`Self::notify_session_state`] whenever the state differs
    /// from the last emitted one.
    #[signal]
    fn session_changed(state: GString);

    /// Goes online on demand (I08 T1): with the boot connector offline, builds
    /// the Online connector from the `stdb-server` setting
    /// (`host[:port][/db]`, default [`DEFAULT_SERVER_ADDRESS`] — the local
    /// `tools/mp-local.sh` harness) and opens the browser session. Idempotent
    /// once online; `false` when the address is invalid or the connection
    /// cannot be opened (the failure is shown as Java's `@connectfail`,
    /// `Net.java:133`).
    #[func]
    pub fn ensure_online(&mut self) -> bool {
        if self.online_mode {
            return true;
        }
        let address = settings_hub::with_settings(|settings| {
            settings.get_string(SERVER_SETTING_KEY, DEFAULT_SERVER_ADDRESS)
        });
        let default_db = self.db_name.to_string();
        let Some((host, db_name)) = parse_server_address(&address, &default_db) else {
            self.show_error("@connectfail", &format!("invalid stdb-server `{address}`"));
            return false;
        };
        if !self.open_online(&host, &db_name) {
            return false;
        }
        self.session.open_browser();
        self.notify_session_state();
        log::info!("MindNet.ensure_online: browsing `{host}/{db_name}`");
        true
    }

    /// Creates a match and enters the lobby as host (returns 0; id arrives via
    /// the `my_match` view). Goes online on demand first (I08 T1).
    #[func]
    pub fn create_match(
        &mut self,
        map_id: GString,
        seed: i64,
        mode: GString,
        visibility: GString,
        max_players: i64,
        rules_json: GString,
    ) -> i64 {
        if !self.ensure_online() {
            return 0;
        }
        let params = HostParams {
            map_id: map_id.to_string(),
            map_seed: seed.max(0) as u64,
            mode: parse_gamemode(&mode.to_string()),
            mode_name: mode.to_string(),
            visibility: parse_visibility(&visibility.to_string()),
            password: None,
            max_players: max_players.clamp(1, 64) as u16,
            rules_json: rules_json.to_string(),
            ..HostParams::default()
        };
        let Some(connector) = self.connector.as_mut() else {
            return 0;
        };
        match self.session.host(connector, &params) {
            Ok(()) => {
                self.ensure_relay();
                self.notify_session_state();
                self.session.match_id().unwrap_or(0) as i64
            }
            Err(error) => {
                // Java `HostDialog.runHost` (`HostDialog.java:80-99`): failures
                // show `@server.error`.
                self.show_error("@server.error", &error.to_string());
                0
            }
        }
    }

    /// Hosts the CURRENT world on demand (I08 T4, Java `HostDialog.runHost`
    /// `HostDialog.java:80-99`): creates a public running match from the live
    /// map/rules, starts it immediately (Java has no lobby) and publishes a
    /// snapshot so joiners can load. `map_id` is the live map/`planet-sector`
    /// id; the player name/colour travel through `set_player_identity` (T2).
    /// Returns 0 when the server is unreachable or hosting fails.
    #[func]
    pub fn host_current_world(&mut self, name: GString, map_id: GString, seed: i64) -> i64 {
        let _ = name;
        if !self.ensure_online() {
            return 0;
        }
        let requested = map_id.to_string();
        let map_id = if requested.trim().is_empty() {
            "spine".to_string()
        } else {
            requested
        };
        let rules_json = self
            .campaign_call("get_rules_json")
            .unwrap_or_else(|| "{}".to_string());
        let mode_name = serde_json::from_str::<serde_json::Value>(&rules_json)
            .ok()
            .and_then(|value| {
                value
                    .get("modeName")
                    .and_then(|name| name.as_str())
                    .map(str::to_string)
            })
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "survival".to_string());
        // Java `SettingsMenuDialog.java:419`: `sliderPref("playerlimit", 16, 2, 32)`.
        let max_players = settings_hub::with_settings(|settings| {
            settings.get_i32("playerlimit", 16)
        })
        .clamp(1, 64) as u16;
        let map_seed = seed.max(0) as u64;
        let build_id = mind_stdb::MIND_VERSION.to_string();
        let content_hash = self.content_hash();
        let params = HostParams {
            map_id: map_id.clone(),
            map_seed,
            mode: parse_gamemode(&mode_name),
            mode_name,
            visibility: Visibility::Public,
            password: None,
            max_players,
            rules_json: rules_json.clone(),
            build_id: build_id.clone(),
            content_hash,
            mods: self.mod_strings(),
            ..HostParams::default()
        };
        let Some(connector) = self.connector.as_mut() else {
            return 0;
        };
        match self.session.host(connector, &params) {
            Ok(()) => {
                self.hosted = Some(HostedWorld {
                    map_id,
                    map_seed,
                    build_id,
                    content_hash,
                    rules_json,
                });
                self.pending_host_start = true;
                self.ensure_relay();
                self.notify_session_state();
                log::info!("MindNet.host_current_world: match created, start pending");
                self.session.match_id().unwrap_or(0) as i64
            }
            Err(error) => {
                self.hosted = None;
                // Java `HostDialog.runHost` failure: `@server.error`.
                self.show_error("@server.error", &error.to_string());
                0
            }
        }
    }

    /// `player.admin = true` after hosting (`HostDialog.runHost`): the match
    /// host; the server's admin gate accepts the host for in-match actions
    /// (`admin/mod.rs:194-220`).
    #[func]
    pub fn local_admin(&self) -> bool {
        self.session.is_host()
    }

    /// Direct-connect (`JoinDialog.connect`): rebuilds the connector against
    /// `address` (`host[:port][/db]`, scheme optional) and enters the public
    /// match browser. Goes online on demand (I08 T1); returns `false` for an
    /// invalid address or an active match.
    #[func]
    pub fn connect_to(&mut self, address: GString) -> bool {
        let requested = address.to_string();
        let default_db = self.db_name.to_string();
        let Some((host, db_name)) = parse_server_address(&requested, &default_db) else {
            self.show_error("@connectfail", &format!("invalid address `{requested}`"));
            return false;
        };
        if !matches!(
            self.session.state(),
            mind_stdb::session::SessionState::Offline | mind_stdb::session::SessionState::Browsing
        ) {
            log::warn!("MindNet.connect_to: a match is already active");
            return false;
        }
        if !self.open_online(&host, &db_name) {
            return false;
        }
        self.session.open_browser();
        self.notify_session_state();
        log::info!("MindNet.connect_to: browsing `{host}/{db_name}`");
        true
    }

    /// Public match rows from the `all_matches` view (join-dialog browser).
    #[func]
    pub fn get_public_matches_json(&self) -> GString {
        let entries: Vec<String> = self
            .public_matches
            .values()
            .map(RowView::debug_json)
            .collect();
        GString::from(format!("[{}]", entries.join(",")).as_str())
    }

    /// Pings `address` through a short-lived browser connector and returns the
    /// lobby-subscription round trip in ms (Java `net.pingHost`,
    /// `JoinDialog.java:245-252`). The STDB connect is asynchronous, so the
    /// first call starts the probe and returns -1; poll
    /// [`Self::server_probe_json`] until `state` leaves `connecting`
    /// (`failed` keeps -1). The relay has no init-ping packet, so the measured
    /// round trip is connect + lobby subscription.
    #[func]
    pub fn ping_server(&mut self, address: GString) -> i64 {
        let Some(key) = self.ensure_probe(&address.to_string()) else {
            return -1;
        };
        self.probes
            .get(&key)
            .map(|probe| probe.ping_ms)
            .unwrap_or(-1)
    }

    /// One join-dialog server row's probe state:
    /// `{"address","state","ping_ms","matches":[RelayMatch...]}`; `state` is
    /// `none` for an invalid address.
    #[func]
    pub fn server_probe_json(&mut self, address: GString) -> GString {
        let Some(key) = self.ensure_probe(&address.to_string()) else {
            return GString::from("{\"state\":\"none\"}");
        };
        let Some(probe) = self.probes.get(&key) else {
            return GString::from("{\"state\":\"none\"}");
        };
        let matches: Vec<serde_json::Value> = probe
            .matches
            .values()
            .filter_map(|row| serde_json::from_str(&row.debug_json()).ok())
            .collect();
        let json = serde_json::json!({
            "address": key,
            "state": probe.state.name(),
            "ping_ms": probe.ping_ms,
            "matches": matches,
        });
        GString::from(json.to_string().as_str())
    }

    /// The active connector's address in normalized `host[:port]/db` form
    /// (no scheme); the join dialog compares it before reconnecting.
    #[func]
    pub fn current_server_address(&self) -> GString {
        let host = self.host.to_string();
        let host = host
            .trim_start_matches("http://")
            .trim_start_matches("https://");
        GString::from(format!("{host}/{}", self.db_name).as_str())
    }

    /// Drops `address`'s probe so the next poll opens a fresh connector (Java
    /// `Server.refresh` re-pings, `JoinDialog.java:280-292`).
    #[func]
    pub fn refresh_server(&mut self, address: GString) -> bool {
        let Some(key) = self.probe_key(&address.to_string()) else {
            return false;
        };
        self.probes.remove(&key);
        true
    }

    /// Joins `match_id`; readiness/loading follows the view. Goes online on
    /// demand first (I08 T1); the real build/content/mods identity travels
    /// with the join so the server's build gates work (I08 T5).
    #[func]
    pub fn join_match(&mut self, match_id: i64, password: GString) -> bool {
        if match_id <= 0 {
            return false;
        }
        if !self.ensure_online() {
            return false;
        }
        let password = {
            let value = password.to_string();
            (!value.is_empty()).then_some(value)
        };
        let build_id = mind_stdb::MIND_VERSION.to_string();
        let content_hash = self.content_hash();
        let mods = self.mod_strings();
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match self.session.join(
            connector,
            match_id as u64,
            password,
            &build_id,
            content_hash,
            MemberRole::Player,
            mods,
        ) {
            Ok(()) => {
                self.pending_join = true;
                self.join_load = None;
                self.snapshot_progress = 0.0;
                // Ask the host for a fresh snapshot immediately: the game-wave
                // subscription can race the join commit and miss metadata rows
                // that already exist (I08 T5). `request_snapshot` subscribes
                // the snapshot wave and enters `snapshot_sync`.
                self.request_snapshot();
                self.ensure_relay();
                self.notify_session_state();
                true
            }
            Err(error) => {
                // Java shows the join rejection through the kick/dialog path;
                // until T9 maps the reasons, the message is shown verbatim.
                self.show_error("@connectfail", &error.to_string());
                false
            }
        }
    }

    /// Leaves the active match.
    #[func]
    pub fn leave_match(&mut self) -> bool {
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        let result = self.session.leave(connector).is_ok();
        if result {
            // The relay runtime/sender are scoped to the old match id.
            self.runtime = None;
            self.sender = None;
            self.relay_match_id = 0;
            self.pending_join = false;
            self.join_load = None;
        }
        self.notify_session_state();
        result
    }

    /// Starts the lobby (host only).
    #[func]
    pub fn start_match(&mut self, force: bool) -> bool {
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        self.session.start(connector, force).is_ok()
    }

    /// Marks readiness.
    #[func]
    pub fn set_ready(&mut self, ready: bool) -> bool {
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        let result = self.session.set_ready(connector, ready).is_ok();
        self.notify_session_state();
        result
    }

    /// Java `HostDialog`/`JoinDialog` identity: `player.name`/`player.color`
    /// follow the `name`/`color-0` settings (`HostDialog.java:22-34`,
    /// `JoinDialog.java:342-354`). Stores the desired values and pushes the
    /// `set_username`/`set_color` reducers now (when connected) and again on
    /// every future connect. `color` is `0xRRGGBBAA` as a signed 32-bit int.
    #[func]
    pub fn set_player_identity(&mut self, name: GString, color: i64) -> bool {
        self.desired_identity = Some((name.to_string(), color as i32 as u32));
        self.push_identity()
    }

    /// Session state name (`offline`/`browsing`/`in_lobby`/`in_game`/...).
    #[func]
    pub fn session_state(&self) -> GString {
        GString::from(self.session.state().name())
    }

    /// Active match row as JSON (empty when none).
    #[func]
    pub fn get_match_state_json(&self) -> GString {
        if self.last_match_json.is_empty() {
            GString::from("{}")
        } else {
            GString::from(self.last_match_json.as_str())
        }
    }

    /// Match members as a JSON array.
    #[func]
    pub fn get_members_json(&self) -> GString {
        GString::from(self.last_members_json.as_str())
    }

    /// Greatest applied `command_id` (checkpoint watermark).
    #[func]
    pub fn last_applied_command_id(&self) -> i64 {
        self.runtime
            .as_ref()
            .map(|runtime| runtime.last_applied_command_id() as i64)
            .unwrap_or(0)
    }

    /// Queued relay changes (`RelayRuntime::queue_depth`).
    #[func]
    pub fn relay_queue_depth(&self) -> i64 {
        self.queue_depth
    }

    /// First relay stream order error, or empty.
    #[func]
    pub fn relay_order_error(&self) -> GString {
        self.runtime
            .as_ref()
            .and_then(RelayRuntime::order_error)
            .map(|error| GString::from(error.as_str()))
            .unwrap_or_default()
    }

    /// Sends a command from a small JSON shape (`{op:"place"|"break",...}`).
    #[func]
    pub fn send_command_json(&mut self, kind_json: GString) -> bool {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&kind_json.to_string()) else {
            return false;
        };
        let op = value.get("op").and_then(|v| v.as_str()).unwrap_or("");
        let x = value.get("x").and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
        let y = value.get("y").and_then(|v| v.as_i64()).unwrap_or(i64::MIN);
        let kind = match op {
            "place" => CommandKind::PlaceBlock(PlaceBlock {
                x: x as i32,
                y: y as i32,
                block: value
                    .get("block")
                    .and_then(|v| v.as_str())
                    .unwrap_or("stone-wall")
                    .to_string(),
                rotation: value.get("rotation").and_then(|v| v.as_u64()).unwrap_or(0) as u8,
                config: Vec::new(),
            }),
            "break" => CommandKind::BreakBlock(BreakBlock {
                x: x as i32,
                y: y as i32,
            }),
            other => {
                log::warn!("MindNet.send_command_json: unsupported op `{other}`");
                return false;
            }
        };
        self.emit_command(kind)
    }

    /// Last checksum reported by the local sim / relay.
    #[func]
    pub fn checksum_report(&self) -> Dictionary<GString, Variant> {
        let mut out = Dictionary::<GString, Variant>::new();
        out.set(
            &GString::from("last_checksum"),
            &self.last_checksum.to_variant(),
        );
        out.set(
            &GString::from("applied_command_id"),
            &self.last_applied_command_id().to_variant(),
        );
        out.set(
            &GString::from("applied_rows"),
            &self
                .runtime
                .as_ref()
                .map(|runtime| runtime.applied_count() as i64)
                .unwrap_or(0)
                .to_variant(),
        );
        out.set(
            &GString::from("checksum_reports"),
            &(self.checksum_monitor.len() as i64).to_variant(),
        );
        out.set(
            &GString::from("desync"),
            &self
                .checksum_monitor
                .detect()
                .map(|detected| detected.command_id as i64)
                .unwrap_or(-1)
                .to_variant(),
        );
        out.set(
            &GString::from("resync_count"),
            &(self.session.resync_count() as i64).to_variant(),
        );
        out
    }

    /// Authority mode name (`relay` today; `authoritative` deferred).
    #[func]
    pub fn authority(&self) -> GString {
        GString::from(self.authority.as_str())
    }

    /// Remote player state mirror (plan M3): the last LWW row for `identity`.
    #[func]
    pub fn get_remote_player_state(&self, identity_hex: GString) -> Dictionary<GString, Variant> {
        let key = identity_hex.to_string();
        let Some(state) = self.remote_players.get(&key) else {
            return Dictionary::new();
        };
        let mut out = Dictionary::<GString, Variant>::new();
        out.set(&GString::from("identity"), &key.to_variant());
        out.set(&GString::from("seq"), &(state.seq as i64).to_variant());
        out.set(
            &GString::from("unit_id"),
            &(state.unit_id as i64).to_variant(),
        );
        out.set(&GString::from("dead"), &state.dead.to_variant());
        out.set(&GString::from("x"), &state.x.to_variant());
        out.set(&GString::from("y"), &state.y.to_variant());
        out.set(&GString::from("vx"), &state.vx.to_variant());
        out.set(&GString::from("vy"), &state.vy.to_variant());
        out.set(&GString::from("rotation"), &state.rotation.to_variant());
        out.set(&GString::from("health"), &state.health.to_variant());
        out.set(&GString::from("shield"), &state.shield.to_variant());
        out.set(&GString::from("team"), &(state.team as i64).to_variant());
        out
    }

    /// Remote player states as a JSON array (renderer/debug; plan §3.15).
    #[func]
    pub fn get_remote_players_json(&self) -> GString {
        let entries: Vec<String> = self
            .remote_players
            .values()
            .map(|state| state.debug_json())
            .collect();
        let json = format!("[{}]", entries.join(","));
        GString::from(json.as_str())
    }

    /// Requests a fresh host snapshot and enters snapshot sync (plan §3.7).
    #[func]
    pub fn request_snapshot(&mut self) -> bool {
        let Some(match_id) = self.session.match_id() else {
            return false;
        };
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        connector.subscribe_snapshot();
        match connector.request_snapshot(match_id) {
            Ok(()) => {
                self.session.enter_snapshot_sync();
                self.snapshot_progress = 0.0;
                self.notify_session_state();
                true
            }
            Err(error) => {
                log::warn!("MindNet.request_snapshot failed: {error}");
                false
            }
        }
    }

    /// Snapshot download progress in `0.0..=1.0` (plan M4/M5).
    #[func]
    pub fn snapshot_progress(&self) -> f32 {
        self.snapshot_progress
    }

    /// Sends a chat message on the `All` channel (plan M3).
    #[func]
    pub fn send_chat(&mut self, text: GString) -> bool {
        let Some(match_id) = self.session.match_id() else {
            return false;
        };
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match connector.send_chat(match_id, ChatKind::All, &text.to_string()) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("MindNet.send_chat failed: {error}");
                false
            }
        }
    }

    /// Sends a UI result as an opaque `Custom` relay command (plan M3/14).
    #[func]
    pub fn send_ui_result(&mut self, _kind: GString, payload: GString) -> bool {
        let data = payload.to_string().into_bytes();
        if data.len() > 1024 {
            log::warn!("MindNet.send_ui_result: payload exceeds 1 KiB");
            return false;
        }
        let kind = CommandKind::Custom(Custom { kind: 1, data });
        self.emit_command(kind)
    }

    /// Dev: request an authority mode change (authoritative is not implemented).
    #[func]
    pub fn set_authority_mode(&mut self, mode: GString) -> bool {
        if mode == "relay" {
            self.authority = "relay".to_string();
            true
        } else {
            log::warn!("MindNet.set_authority_mode: authoritative mode is deferred (D2)");
            false
        }
    }

    /// Test-only divergence hook (debug builds; no-op otherwise).
    #[func]
    pub fn dev_inject_divergence(&self) -> bool {
        false
    }
}

impl MindNet {
    /// (Re)builds the connector for `host`/`db_name` in Online mode, attaches
    /// every binder and opens the link. Clears the per-match mirrors. Returns
    /// `false` when the address is unreachable or the socket cannot be opened
    /// now (the connector still schedules its retry); the failure is shown as
    /// `@connectfail` (`Net.java:133`).
    fn open_online(&mut self, host: &str, db_name: &str) -> bool {
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
    fn push_identity(&mut self) -> bool {
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
    fn probe_key(&self, address: &str) -> Option<String> {
        let default_db = self.db_name.to_string();
        let (host, db_name) = parse_server_address(address, &default_db)?;
        Some(format!("{host}/{db_name}"))
    }

    /// Ensures a probe exists for `address` (starting one when missing, LRU
    /// eviction at [`PROBE_LIMIT`]) and returns its key.
    fn ensure_probe(&mut self, address: &str) -> Option<String> {
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
    fn pump_probes(&mut self) {
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

    /// Feeds the lobby-wave `my_matches` view into the session: the host's
    /// match id is known from here (the game wave that carries `my_match` does
    /// not exist yet), and a pending [`Self::host_current_world`] start runs
    /// once the row reaches `Lobby` (I08 T4).
    fn drain_my_matches(&mut self) {
        let Some(binder) = self.my_matches_binder.as_ref() else {
            return;
        };
        let mut newest: Option<RelayMatch> = None;
        for change in binder.drain() {
            match change {
                RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                    if row.status != MatchStatus::Ended
                        && newest
                            .as_ref()
                            .map(|current| row.match_id > current.match_id)
                            .unwrap_or(true)
                    {
                        newest = Some(row);
                    }
                }
                RowChange::Delete(_) => {}
            }
        }
        let Some(row) = newest else {
            return;
        };
        let local = self
            .connector
            .as_ref()
            .and_then(Connector::local_identity)
            .map(|identity| identity.identity);
        let Some(local) = local else {
            return;
        };
        let is_new = self.session.match_id() != Some(row.match_id);
        self.session.observe_match(&row, &local);
        if is_new {
            self.ensure_relay();
        }
        if self.pending_host_start
            && self.session.is_host()
            && self.session.match_id() == Some(row.match_id)
            && row.status == MatchStatus::Lobby
        {
            self.pending_host_start = false;
            if self.start_match(true) {
                log::info!(
                    "MindNet: hosting match {} started immediately",
                    row.match_id
                );
            }
            self.publish_world_snapshot();
        }
    }

    /// Publishes the live world as a dynamic snapshot so late joiners can load
    /// it (`NetClient`'s `WorldStream`; I08 T4/T5). Wraps
    /// `MindSimHost.encode_snapshot`'s save bytes in the `MGSN` header
    /// (`snapshot.rs`) and sends them through `publish_snapshot`, which the
    /// server splits into chunks.
    fn publish_world_snapshot(&mut self) -> bool {
        let Some(hosted) = self.hosted.clone() else {
            return false;
        };
        let Some(match_id) = self.session.match_id() else {
            return false;
        };
        let (save_bytes, sim_tick, checksum) = {
            let Some(host) = self.sim_host.as_mut() else {
                return false;
            };
            let bytes = host.bind_mut().encode_snapshot().to_vec();
            let tick = host.bind().get_tick().max(0) as u64;
            let checksum = host.bind().checksum_scoped_value();
            (bytes, tick, checksum)
        };
        if save_bytes.is_empty() {
            log::warn!("MindNet.publish_world_snapshot: save bytes empty; no snapshot");
            return false;
        }
        let command_id = self
            .runtime
            .as_ref()
            .map(|runtime| runtime.last_applied_command_id())
            .unwrap_or(0);
        let mut payload = SnapshotHeader {
            format: SNAPSHOT_FORMAT,
            checksum_version: mind_core::constants::CHECKSUM_VERSION,
            command_id,
            sim_tick,
            map_id: hosted.map_id.clone(),
            map_seed: hosted.map_seed,
            map_hash: 0,
            build_id: hosted.build_id.clone(),
            content_hash: hosted.content_hash,
            next_entity_id: 0,
            rules_json: hosted.rules_json.clone(),
            wave: 0,
            wavetime: 0.0,
            rng_sim: Vec::new(),
        }
        .to_bytes();
        payload.extend_from_slice(&save_bytes);
        let payload_len = payload.len();
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match connector.publish_snapshot(
            match_id,
            SnapshotKind::Dynamic,
            SNAPSHOT_FORMAT,
            mind_core::constants::CHECKSUM_VERSION,
            command_id,
            sim_tick,
            checksum,
            &hosted.map_id,
            hosted.map_seed,
            0,
            &hosted.build_id,
            hosted.content_hash,
            payload,
        ) {
            Ok(()) => {
                log::info!(
                    "MindNet: snapshot published for match {match_id} ({payload_len} bytes)"
                );
                true
            }
            Err(error) => {
                log::warn!("MindNet.publish_snapshot failed: {error}");
                false
            }
        }
    }

    /// Reads a `GString`-returning method on `/root/Spine/MindCampaign`.
    fn campaign_call(&self, method: &str) -> Option<String> {
        let mut campaign = self
            .base()
            .try_get_node_as::<crate::MindCampaign>("/root/Spine/MindCampaign")?;
        if !campaign.has_method(method) {
            return None;
        }
        campaign
            .call(method, &[])
            .try_to::<GString>()
            .ok()
            .map(|value| value.to_string())
    }

    /// `ContentRegistry::content_hash` from the live sim host (0 when absent).
    fn content_hash(&self) -> u64 {
        self.sim_host
            .as_ref()
            .map(|host| host.bind().content_hash() as u64)
            .unwrap_or(0)
    }

    /// Enabled mod strings from `/root/MindMods` (`MindMods.mod_strings`).
    fn mod_strings(&self) -> Vec<String> {
        let Some(mut mods) = self
            .base()
            .try_get_node_as::<crate::MindMods>("/root/MindMods")
        else {
            return Vec::new();
        };
        mods.bind_mut()
            .mod_strings()
            .as_slice()
            .iter()
            .map(|value| value.to_string())
            .collect()
    }

    /// Emits `session_changed` when the session state differs from the last
    /// emitted one. Every state mutation funnels through here, and `process()`
    /// calls it once per frame so row-driven transitions (match status,
    /// disconnect) are covered too.
    fn notify_session_state(&mut self) {
        let state = self.session.state();
        if state != self.last_session_state {
            self.last_session_state = state;
            self.base_mut().emit_signal(
                "session_changed",
                &[GString::from(state.name()).to_variant()],
            );
        }
    }

    /// Shows a bundle-keyed error through `MindUi.show_error_key`
    /// (`Net.java:133` connection failures; `HostDialog.java:80-99` host
    /// failures). Falls back to the log when `/root/MindUi` is absent.
    fn show_error(&self, key: &str, detail: &str) {
        log::warn!("MindNet: {key} {detail}");
        let Some(mut ui) = self.base().try_get_node_as::<crate::MindUi>("/root/MindUi") else {
            return;
        };
        ui.bind_mut()
            .show_error_key(GString::from(key), GString::from(detail));
    }

    /// Creates the relay runtime/sender once a match id is known. A different
    /// match id rebuilds both (leave → re-host/join), so commands never carry
    /// a stale match (I08 T5).
    fn ensure_relay(&mut self) {
        let Some(match_id) = self.session.match_id() else {
            return;
        };
        if self.relay_match_id != match_id {
            self.relay_match_id = match_id;
            self.runtime = None;
            self.sender = None;
        }
        if self.runtime.is_none()
            && let Some(connector) = self.connector.as_mut()
        {
            self.runtime = Some(RelayRuntime::subscribe(connector, match_id));
        }
        if self.sender.is_none() {
            self.sender = Some(CommandSender::new(match_id));
        }
    }

    fn sim_tick(&self) -> u64 {
        self.sim_host
            .as_ref()
            .map(|host| host.bind().get_tick().max(0) as u64)
            .unwrap_or(0)
    }

    fn drain_match_rows(&mut self) {
        let Some(binder) = self.match_binder.as_ref() else {
            return;
        };
        let mut latest = None;
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                latest = Some(row);
            }
        }
        if let Some(row) = latest {
            self.last_match_json = row.debug_json();
            let local = self
                .connector
                .as_ref()
                .and_then(Connector::local_identity)
                .map(|identity| identity.identity);
            if let Some(local) = local {
                self.session.observe_match(&row, &local);
            }
            self.ensure_relay();
        }
    }

    fn drain_members_rows(&mut self) {
        let Some(binder) = self.members_binder.as_ref() else {
            return;
        };
        let mut members = Vec::new();
        for change in binder.drain() {
            match change {
                RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                    members.push(row.debug_json());
                }
                RowChange::Delete(_) => {}
            }
        }
        if !members.is_empty() {
            self.last_members_json = format!("[{}]", members.join(","));
        }
    }

    fn drain_state_rows(&mut self) {
        let Some(binder) = self.state_binder.as_ref() else {
            return;
        };
        let mut latest = None;
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                latest = Some(row);
            }
        }
        if let Some(row) = latest {
            self.last_state_json = row.debug_json();
            self.last_checksum = format!("{:016x}", row.last_command_id);
        }
    }

    fn drain_relay(&mut self) {
        let Some(runtime) = self.runtime.as_mut() else {
            return;
        };
        let commands = runtime.drain();
        self.queue_depth = runtime.queue_depth() as i64;
        self.last_checksum = self
            .sim_host
            .as_ref()
            .map(|host| host.bind().get_checksum().to_string())
            .unwrap_or_default();
        if let Some(host) = self.sim_host.as_mut() {
            let mut host = host.bind_mut();
            for command in commands {
                host.enqueue_sim_command(command);
            }
        }
    }

    /// Emits one relay command with prediction bookkeeping (plan §3.5).
    fn emit_command(&mut self, kind: CommandKind) -> bool {
        let tick = self.sim_tick();
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        let Some(sender) = self.sender.as_mut() else {
            return false;
        };
        let mut transport = StdbTransport::new(connector);
        match sender.send(&mut transport, tick, kind) {
            Ok(seq) => {
                if let Some(runtime) = self.runtime.as_mut() {
                    runtime.on_prediction(seq);
                }
                true
            }
            Err(error) => {
                log::warn!("MindNet.emit_command failed: {error}");
                false
            }
        }
    }

    /// Mirrors `my_match_player_states` rows for the renderer/tests (plan M3).
    fn drain_player_states(&mut self) {
        let Some(binder) = self.players_binder.as_ref() else {
            return;
        };
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                self.remote_players
                    .insert(row.identity.to_hex().to_string(), row);
            }
        }
    }

    /// Feeds `my_match_checksums` rows into the desync monitor (plan M4).
    fn drain_checksums(&mut self) {
        let Some(binder) = self.checksums_binder.as_ref() else {
            return;
        };
        let host = self.session.host_identity();
        if let Some(host) = host {
            self.checksum_monitor.set_host(host);
        }
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                self.checksum_monitor.record(ChecksumReport {
                    sender: row.sender,
                    command_id: row.command_id,
                    sim_tick: row.sim_tick,
                    checksum: row.checksum,
                    checksum_version: row.checksum_version,
                    scope: row.scope,
                });
                self.last_checksum = format!("{:016x}", row.checksum);
            }
        }
        if self.checksum_monitor.detect().is_some() && self.session.resync_count() == 0 {
            log::warn!("MindNet: desync detected; requesting a host snapshot");
            let _ = self.request_snapshot();
        }
    }

    /// Tracks the newest snapshot metadata and progress (plan M4/M5). A
    /// pending join requests a fresh host snapshot and targets the newest
    /// available one (I08 T5).
    fn drain_snapshots(&mut self) {
        let Some(binder) = self.snapshots_binder.as_ref() else {
            return;
        };
        let mut newest: Option<(u64, u16)> = None;
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change
                && newest.map(|(id, _)| row.snapshot_id > id).unwrap_or(true)
            {
                newest = Some((row.snapshot_id, row.chunk_count));
            }
        }
        let Some((id, chunks)) = newest else {
            return;
        };
        log::debug!("MindNet: snapshot {id} available ({chunks} chunks)");
        if !self.pending_join || self.session.is_host() {
            return;
        }
        // The join requested a fresh snapshot; this row is its metadata (or a
        // fresher republication). Track the newest one and wait for chunks.
        let newer_target = self
            .join_load
            .as_ref()
            .map(|load| load.target_snapshot_id < id)
            .unwrap_or(true);
        if !newer_target {
            return;
        }
        self.join_load = Some(JoinLoad {
            target_snapshot_id: id,
            expected_chunks: chunks,
            chunks: std::collections::BTreeMap::new(),
        });
        self.snapshot_progress = 0.0;
        log::info!("MindNet: loading snapshot {id} ({chunks} chunks)");
    }

    /// Collects the joiner's snapshot chunks and applies the reassembled blob
    /// once every chunk arrived (`snapshot_progress` tracks the fraction;
    /// I08 T5).
    fn drain_snapshot_chunks(&mut self) {
        let Some(binder) = self.snapshot_chunks_binder.as_ref() else {
            return;
        };
        let mut rows = Vec::new();
        for change in binder.drain() {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                rows.push((row.snapshot_id, row.chunk_index, row.data));
            }
        }
        if rows.is_empty() {
            return;
        }
        let blob = {
            let Some(load) = self.join_load.as_mut() else {
                return;
            };
            for (snapshot_id, index, data) in rows {
                if snapshot_id == load.target_snapshot_id {
                    load.chunks.entry(index).or_insert(data);
                }
            }
            let received = load.chunks.len() as u16;
            self.snapshot_progress = if load.expected_chunks == 0 {
                1.0
            } else {
                f32::from(received) / f32::from(load.expected_chunks)
            };
            if load.expected_chunks == 0 || received < load.expected_chunks {
                return;
            }
            let indexed: Vec<(u16, Vec<u8>)> = load
                .chunks
                .iter()
                .map(|(index, data)| (*index, data.clone()))
                .collect();
            mind_stdb::snapshot::reassemble(&indexed, load.expected_chunks as usize)
        };
        let Some(blob) = blob else {
            log::warn!("MindNet: snapshot chunks did not reassemble");
            return;
        };
        self.apply_join_snapshot(&blob);
    }

    /// The host answers every pending snapshot request with a fresh snapshot
    /// (`snapshot.rs` `my_match_snapshot_requests`; I08 T5).
    fn drain_snapshot_requests(&mut self) {
        let Some(binder) = self.snapshot_requests_binder.as_ref() else {
            return;
        };
        let mut requested = false;
        for change in binder.drain() {
            if matches!(change, RowChange::Insert(_) | RowChange::Update { .. }) {
                requested = true;
            }
        }
        if requested && self.session.is_host() && self.session.match_id().is_some() {
            log::info!("MindNet: snapshot request received; publishing a fresh snapshot");
            self.publish_world_snapshot();
        }
    }

    /// Applies a reassembled host snapshot (I08 T5): writes the save under
    /// `user://mp/`, loads it (I06 T1 `load_map_world`), starts the session
    /// with the header rules, sets the snapshot tick, skips relayed commands
    /// the blob already includes and shows the world.
    fn apply_join_snapshot(&mut self, blob: &[u8]) -> bool {
        let Ok(snapshot) = DynamicSnapshot::decode(blob) else {
            log::warn!("MindNet: snapshot decode failed");
            return false;
        };
        let header = snapshot.header;
        let Some(match_id) = self.session.match_id() else {
            return false;
        };
        let relative = format!("user://mp/join-{match_id}.msav");
        let path = std::path::PathBuf::from(
            ProjectSettings::singleton()
                .globalize_path(&relative)
                .to_string(),
        );
        if let Some(parent) = path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            log::warn!("MindNet: cannot create {}", parent.display());
            return false;
        }
        if let Err(error) = std::fs::write(&path, &snapshot.body) {
            log::warn!("MindNet: snapshot save write failed: {error}");
            return false;
        }
        let loaded = self
            .sim_host
            .as_mut()
            .map(|host| {
                host.bind_mut()
                    .load_map_world(GString::from(relative.as_str()))
            })
            .unwrap_or(false);
        if !loaded {
            log::warn!("MindNet: snapshot world load failed");
            return false;
        }
        let started = self
            .base()
            .try_get_node_as::<crate::MindCampaign>("/root/Spine/MindCampaign")
            .map(|mut campaign| {
                campaign
                    .bind_mut()
                    .start_joined_world(GString::from(header.rules_json.as_str()))
            })
            .unwrap_or(false);
        if !started {
            log::warn!("MindNet: joined session start failed");
            return false;
        }
        if let Some(host) = self.sim_host.as_mut() {
            host.bind_mut().set_sim_tick(header.sim_tick as i64);
        }
        if let Some(runtime) = self.runtime.as_mut() {
            runtime.skip_through(header.command_id);
        }
        // Java: the state leaves the menu when the world is live.
        if let Some(mut ui_root) = self.base().get_node_or_null("/root/Spine/Ui/UiRoot") {
            ui_root.call("set_menu_visible", &[false.to_variant()]);
        }
        self.pending_join = false;
        self.join_load = None;
        self.snapshot_progress = 1.0;
        self.session.exit_snapshot_sync();
        self.notify_session_state();
        log::info!(
            "MindNet: snapshot applied (tick {}, command {})",
            header.sim_tick,
            header.command_id
        );
        true
    }

    /// Mirrors public `all_matches` rows for the join-dialog browser.
    fn drain_public_matches(&mut self) {
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
    fn maybe_publish_checksum(&mut self) {
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

fn collect_args(args: &[GString]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

fn parse_gamemode(name: &str) -> Gamemode {
    match name {
        "sandbox" => Gamemode::Sandbox,
        "attack" => Gamemode::Attack,
        "pvp" => Gamemode::Pvp,
        "editor" => Gamemode::Editor,
        _ => Gamemode::Survival,
    }
}

fn parse_visibility(name: &str) -> Visibility {
    if name.eq_ignore_ascii_case("unlisted") {
        Visibility::Unlisted
    } else {
        Visibility::Public
    }
}

/// Parses a direct-connect address (`host[:port][/db]`, `http://` optional)
/// into a SpacetimeDB host URL and database name. A missing port defaults to
/// the local dev port (3000); a missing database keeps `default_db`.
fn parse_server_address(address: &str, default_db: &str) -> Option<(String, String)> {
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
fn probe_reachable(host: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::{DEFAULT_SERVER_ADDRESS, parse_server_address};

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
}
