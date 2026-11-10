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

pub mod call;
pub mod relay;

mod actions;
mod adapters;
mod drain;
mod probe;
mod roster;
mod snapshot;

#[cfg(test)]
mod tests;

use std::net::ToSocketAddrs;

use godot::classes::notify::NodeNotification;
use godot::classes::{INode, Node, Os, ProjectSettings};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_stdb::binder::TableBinder;
use mind_stdb::call_packet::CallPacket;
use mind_stdb::calls::CallStream;
use mind_stdb::checksum::{ChecksumMonitor, ChecksumReport};
use mind_stdb::commands::CommandSender;
use mind_stdb::module_bindings::{
    AllAdminsTableAccessor, AllBansTableAccessor, AllMatchesTableAccessor, AllPlayersTableAccessor,
    AllWhitelistTableAccessor, BreakBlock, CallPacketKind, ChatKind, CommandKind, Custom, Gamemode,
    MatchPlayerState, MatchStatus, MemberRole, MyAdminActionsTableAccessor, MyKickTableAccessor,
    MyMatchCallsTableAccessor, MyMatchChatTableAccessor, MyMatchChecksumsTableAccessor,
    MyMatchEventsTableAccessor, MyMatchMembersTableAccessor, MyMatchPlayerStatesTableAccessor,
    MyMatchSnapshotChunksTableAccessor, MyMatchSnapshotRequestsTableAccessor,
    MyMatchSnapshotsTableAccessor, MyMatchStateTableAccessor, MyMatchTableAccessor,
    MyMatchesTableAccessor, MyTracesTableAccessor, PlaceBlock, RelayMatch, SnapshotKind,
    Visibility,
};
use mind_stdb::rows::RowView;
use mind_stdb::snapshot::{DynamicSnapshot, SNAPSHOT_FORMAT, SnapshotHeader};
use mind_stdb::transport::StdbTransport;
use mind_stdb::{
    ChatLog, ConnectionConfig, Connector, ConnectorEvent, EventBus, HostParams, MatchSession,
    PlayerAction, Roster, RowChange, SessionState, StdbMode, WaveName, parse_player_suffix_from,
};

use crate::settings_hub;
use crate::sim_host::MindSimHost;
use crate::wiring;

use self::adapters::{
    chat_facade_json, collect_args, events_facade_json, parse_gamemode, parse_server_address,
    parse_visibility,
};
use self::call::{CallBridge, CallEffect, effect_json};
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
    /// Sibling `SimHost` (`../SimHost`); relayed commands queue into its pump.
    #[export]
    pub sim_host: Option<Gd<MindSimHost>>,
    /// Sibling `MindCampaign` (`../MindCampaign`); rules/join-world facade.
    /// Optional: silent when a scene has no campaign node.
    #[export]
    pub campaign: Option<Gd<crate::campaign::MindCampaign>>,
    /// `Ui/UiRoot` (`../Ui/UiRoot`); hides the menu once a joined world is live.
    /// Optional.
    #[export]
    pub ui_root: Option<Gd<crate::ui::root::MindUiRoot>>,
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
    /// Player-command mailbox installed into `MindSimHost` while a match is
    /// active (I08 T6); drained every frame into `send_player_command`.
    relay_outbox: Option<std::sync::Arc<std::sync::Mutex<Vec<mind_core::determinism::SimCommand>>>>,
    /// Member rows by identity hex (sender team/slot resolution, I08 T6).
    members: std::collections::BTreeMap<String, mind_stdb::module_bindings::RelayMember>,
    /// Relay team last applied to the local sim (PAR-0600); `None` while
    /// unassigned/offline so a repeated assignment still lands.
    applied_relay_team: Option<u8>,
    /// `my_match_chat` rows mirrored for the chat overlay (PAR-0121/0125):
    /// the net facade's `chat_json()` projection.
    chat: ChatLog,
    /// `my_match_chat` binder (Game wave).
    chat_binder: Option<TableBinder<MyMatchChatTableAccessor>>,
    /// Relay kinds already reported as "no sim equivalent" (log once).
    unmapped_kinds: std::collections::BTreeSet<String>,
    /// Player-list/trace aggregation over the roster tables (PAR-0122).
    roster: Roster,
    /// `all_players` rows (usernames for the roster).
    all_players_binder: Option<TableBinder<AllPlayersTableAccessor>>,
    /// `all_admins` rows (admin flag).
    all_admins_binder: Option<TableBinder<AllAdminsTableAccessor>>,
    /// `all_bans` rows (trace ban flag).
    all_bans_binder: Option<TableBinder<AllBansTableAccessor>>,
    /// `all_whitelist` rows (trace whitelist flag).
    all_whitelist_binder: Option<TableBinder<AllWhitelistTableAccessor>>,
    /// `my_kick` rows (kick history/trace).
    my_kick_binder: Option<TableBinder<MyKickTableAccessor>>,
    /// `my_admin_actions` rows (admin action history/trace).
    admin_actions_binder: Option<TableBinder<MyAdminActionsTableAccessor>>,
    /// `my_traces` rows (server answers to `admin_request_trace`).
    traces_binder: Option<TableBinder<MyTracesTableAccessor>>,
    /// `my_match_events` rows projected onto the client `EventType` bus
    /// (GAPS #37); typed payloads decode from the row detail envelope.
    events: EventBus,
    /// `my_match_events` binder (Game wave).
    events_binder: Option<TableBinder<MyMatchEventsTableAccessor>>,
    /// `my_match_calls` rows → frames (GAPS #35 / PAR-0576); the binder half of
    /// the call channel. Every new row is pushed into [`Self::calls`].
    call_rows: CallStream,
    /// `my_match_calls` binder (Game wave).
    call_rows_binder: Option<TableBinder<MyMatchCallsTableAccessor>>,
    /// Server→client `@Remote` call channel (GAPS #35).
    calls: CallBridge,
    /// Applied calls not yet read by the UI/view owners (`server_calls_json`).
    pending_calls: Vec<serde_json::Value>,
    /// Last kick reason applied from a call (`None` until kicked).
    kicked: Option<String>,
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
            campaign: None,
            ui_root: None,
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
            relay_outbox: None,
            members: std::collections::BTreeMap::new(),
            applied_relay_team: None,
            chat: ChatLog::new(),
            chat_binder: None,
            unmapped_kinds: std::collections::BTreeSet::new(),
            roster: Roster::new(),
            all_players_binder: None,
            all_admins_binder: None,
            all_bans_binder: None,
            all_whitelist_binder: None,
            my_kick_binder: None,
            admin_actions_binder: None,
            traces_binder: None,
            events: EventBus::new(),
            events_binder: None,
            call_rows: CallStream::new(),
            call_rows_binder: None,
            calls: CallBridge::new(),
            pending_calls: Vec::new(),
            kicked: None,
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
                    ConnectorEvent::Disconnected { .. } => {
                        self.session.on_disconnected();
                        self.roster.clear();
                        self.events.clear();
                        self.call_rows.clear();
                        self.calls.clear();
                        self.chat.clear();
                        self.applied_relay_team = None;
                    }
                    ConnectorEvent::Resync => {
                        self.session.on_resync();
                        self.roster.clear();
                        self.events.clear();
                        self.call_rows.clear();
                        self.calls.clear();
                        self.chat.clear();
                        self.applied_relay_team = None;
                    }
                    _ => {}
                }
            }
            self.drain_match_rows();
            self.drain_my_matches();
            self.drain_members_rows();
            self.drain_roster();
            // After the roster/team drains, so the admin/team filter is current
            // before any chat row is applied (a filtered-out row is not
            // resurrected by a later viewer change).
            self.refresh_chat_viewer();
            self.drain_server_chat();
            self.drain_server_events();
            self.drain_server_calls();
            self.drain_state_rows();
            self.drain_public_matches();
            self.drain_player_states();
            self.drain_checksums();
            self.drain_snapshots();
            self.drain_snapshot_chunks();
            self.drain_snapshot_requests();
            self.drain_outbox();
            self.drain_relay();
            self.maybe_publish_checksum();
        }
        if connected_now && self.push_identity() {
            log::info!("MindNet: player identity pushed after connect");
        }
        self.pump_server_calls();
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
        // Scene-wired spine references (`game.tscn` wires the `Spine`
        // siblings); re-adopted on every bootstrap so a reload keeps them.
        self.sim_host = wiring::required("MindNet", "sim_host", &self.sim_host);
        self.campaign = wiring::optional(&self.campaign);
        self.ui_root = wiring::optional(&self.ui_root);
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
        // Player roster/trace tables (PAR-0122).
        self.all_players_binder = Some(connector.bind::<AllPlayersTableAccessor>("all_players"));
        self.all_admins_binder = Some(connector.bind::<AllAdminsTableAccessor>("all_admins"));
        self.all_bans_binder = Some(connector.bind::<AllBansTableAccessor>("all_bans"));
        self.all_whitelist_binder =
            Some(connector.bind::<AllWhitelistTableAccessor>("all_whitelist"));
        self.my_kick_binder = Some(connector.bind::<MyKickTableAccessor>("my_kick"));
        self.admin_actions_binder =
            Some(connector.bind::<MyAdminActionsTableAccessor>("my_admin_actions"));
        self.traces_binder = Some(connector.bind::<MyTracesTableAccessor>("my_traces"));
        // `EventType` bus rows (GAPS #37): typed payloads arrive in `detail`.
        self.events_binder = Some(connector.bind::<MyMatchEventsTableAccessor>("my_match_events"));
        // Server→client `@Remote` calls (GAPS #35 / PAR-0576).
        self.call_rows_binder = Some(connector.bind::<MyMatchCallsTableAccessor>("my_match_calls"));
        // Chat rows for the overlay (`chat_json`, PAR-0121/0125).
        self.chat_binder = Some(connector.bind::<MyMatchChatTableAccessor>("my_match_chat"));
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
            // Back to single-player command routing (I08 T6).
            if let Some(host) = self.sim_host.as_mut() {
                host.bind_mut().clear_relay_outbox();
            }
            self.relay_outbox = None;
            self.members.clear();
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

    /// Decodes and queues one server→client `@Remote` call frame
    /// (`[kind: u8][body]`, GAPS #35). Returns `false` when the frame is
    /// unknown or malformed; the rejection stays visible in
    /// [`Self::call_channel_json`] and later frames keep working.
    #[func]
    pub fn push_call_frame(&mut self, frame: PackedByteArray) -> bool {
        let accepted = self.calls.push_frame(frame.as_slice());
        if !accepted {
            log::warn!(
                "MindNet: rejected server call frame: {}",
                self.calls.rejects_json()
            );
        }
        accepted
    }

    /// Drains the applied server calls as a JSON array
    /// (`[{call: {...}, effect: {...}|null}]`, oldest first) and clears the
    /// pending list.
    ///
    /// Calls are applied every frame ([`Self::pump_server_calls`]): a kick
    /// leaves the match and shows the localized reason immediately; camera,
    /// music and info-message effects are also reported per call so the
    /// camera/audio/UI owners can consume them; every other call is reported
    /// verbatim for its owner. The pending list is capped at
    /// `CALL_QUEUE_LIMIT`, so a consumer that never polls cannot grow memory.
    #[func]
    pub fn server_calls_json(&mut self) -> GString {
        let calls = std::mem::take(&mut self.pending_calls);
        GString::from(serde_json::Value::Array(calls).to_string().as_str())
    }

    /// Call-channel diagnostics (queue depth, accepted/rejected/dropped, the
    /// rejection tail, and the last applied kick reason).
    #[func]
    pub fn call_channel_json(&self) -> GString {
        let mut stats = self.calls.stats_json();
        stats["kicked"] = serde_json::Value::String(self.kicked.clone().unwrap_or_default());
        GString::from(stats.to_string().as_str())
    }

    /// `my_match_events` rows as a JSON array (the `EventType` bus, GAPS #37).
    #[func]
    pub fn match_events_json(&self) -> GString {
        GString::from(self.events.to_json().to_string().as_str())
    }

    /// `my_match_events` rows with their typed [`mind_stdb::EventType`]
    /// payload: `[{id, kind, class, payload, detail}]`, in commit order.
    #[func]
    pub fn match_event_types_json(&self) -> GString {
        let mut out = Vec::new();
        for event in self.events.events() {
            let typed = event.typed();
            out.push(serde_json::json!({
                "id": event.event_id,
                "kind": format!("{:?}", event.kind),
                "class": event.class_name(),
                "payload": typed.as_ref().map(mind_stdb::EventType::encode).transpose().ok().flatten(),
                "detail": event.detail,
            }));
        }
        GString::from(serde_json::Value::Array(out).to_string().as_str())
    }

    /// `my_match_chat` rows as a JSON array of `mind_stdb::ChatLine`
    /// projections (PAR-0121/0125), chronological order.
    ///
    /// This is the net half of the `chatui` contract: `MindUi.chat_poll`
    /// pulls it through `net_call("chat_json")` and re-filters nothing itself
    /// (the log already drops rows the local viewer may not see).
    #[func]
    pub fn chat_json(&self) -> GString {
        GString::from(chat_facade_json(&self.chat).to_string().as_str())
    }

    /// `my_match_events` rows as a JSON array of `mind_stdb::ServerEvent`
    /// projections (GAPS #37), commit order.
    ///
    /// `MindUi.take_event_toasts` pulls it through `net_call("events_json")`
    /// when the facade exposes it; [`Self::match_events_json`] stays the
    /// debug/UI-inspection variant.
    #[func]
    pub fn events_json(&self) -> GString {
        GString::from(events_facade_json(&self.events).to_string().as_str())
    }

    /// Local SpacetimeDB identity hex (64 lowercase hex chars), or `""` while
    /// offline/before the handshake.
    ///
    /// The chat overlay and the admin/trace views read it through
    /// `net_call("local_identity_hex")` (guarded; the offline `StdbConnector`
    /// never carries a live identity).
    #[func]
    pub fn local_identity_hex(&self) -> GString {
        let identity = self
            .session
            .local_identity()
            .map(|identity| identity.to_hex().to_string())
            .unwrap_or_default();
        GString::from(identity.as_str())
    }

    /// Player-list rows (name, team, admin, connected, role) built from the
    /// relay roster tables (PAR-0122). Replaces the always-empty
    /// `MindUi.player_list_json()`; the UI owns the rendering.
    #[func]
    pub fn players_json(&self) -> GString {
        GString::from(self.roster.players_json().to_string().as_str())
    }

    /// Ban roster rows (`[{id, lastIP, lastName, reason, ...}]`) built from the
    /// `all_bans` subscription rows (`player_ban`), for the player list's
    /// `open_dialog("bans")` context (`BansDialog`).
    ///
    /// The rows are identity-direct: an offline banned identity has no
    /// `relay_member` row, so the dialog cannot go through
    /// [`Roster::find`]/[`Roster::players`]. `lastIP` stays empty (deviation
    /// D2: the relay schema carries no client addresses).
    #[func]
    pub fn bans_json(&self) -> GString {
        GString::from(self.roster.bans_json().to_string().as_str())
    }

    /// Admin roster rows (`[{id, lastIP, lastName, grantedBy, grantedAt}]`)
    /// built from the `all_admins` subscription rows (`admin_identity`), for
    /// the player list's `open_dialog("admins")` context (`AdminsDialog`).
    #[func]
    pub fn admins_json(&self) -> GString {
        GString::from(self.roster.admins_json().to_string().as_str())
    }

    /// Whitelist roster rows (`[{id, lastIP, lastName, addedBy, addedAt}]`)
    /// built from the `all_whitelist` subscription rows (`whitelist_entry`).
    #[func]
    pub fn whitelist_json(&self) -> GString {
        GString::from(self.roster.whitelist_json().to_string().as_str())
    }

    /// Relay-assigned team for the local player (`relay_member.team`,
    /// GAPS #33 / PAR-0545), or `-1` when unassigned/offline.
    ///
    /// This is the client-apply seam for the module-side PvP balancer: the
    /// host publishes team liveness through [`Self::publish_team_data`], the
    /// module assigns a team on join/start and every peer reads it here (and
    /// through [`Self::players_json`]). Applying the value to the local sim is
    /// a residual: `MindSimHost::set_player_team` is editor-gated today.
    #[func]
    pub fn my_team(&self) -> i64 {
        let Some(identity) = self.session.local_identity() else {
            return -1;
        };
        self.members
            .get(&identity.to_hex().to_string())
            .and_then(|member| member.team)
            .map(i64::from)
            .unwrap_or(-1)
    }

    /// Host/admin: publishes the match's team liveness for the module-side PvP
    /// balancer (GAPS #33). `teams_json` is
    /// `[{"team":1,"active":true,"protect_cores":true,"wave_team":false}]`;
    /// the host derives it from its world (`state.teams`) and republishes when
    /// a core appears/disappears.
    #[func]
    pub fn publish_team_data(&mut self, teams_json: GString) -> bool {
        let Some(match_id) = self.session.match_id() else {
            log::warn!("publish_team_data: no match joined");
            return false;
        };
        let parsed: Vec<serde_json::Value> = match serde_json::from_str(&teams_json.to_string()) {
            Ok(parsed) => parsed,
            Err(error) => {
                log::warn!("publish_team_data: bad JSON ({error})");
                return false;
            }
        };
        let mut teams = Vec::with_capacity(parsed.len());
        for value in parsed {
            let Some(team) = value.get("team").and_then(serde_json::Value::as_u64) else {
                log::warn!("publish_team_data: entry without `team`: {value}");
                return false;
            };
            let Some(team) = u8::try_from(team).ok().filter(|team| *team < 8) else {
                log::warn!("publish_team_data: team id {team} out of range");
                return false;
            };
            teams.push(mind_stdb::module_bindings::TeamAssignment {
                team,
                active: value
                    .get("active")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                protect_cores: value
                    .get("protect_cores")
                    .or_else(|| value.get("protectCores"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true),
                wave_team: value
                    .get("wave_team")
                    .or_else(|| value.get("waveTeam"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            });
        }
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match connector.publish_match_teams(match_id, teams) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("publish_team_data failed: {error}");
                false
            }
        }
    }

    /// Connected player count (player-list header).
    #[func]
    pub fn player_count(&self) -> i32 {
        self.roster.connected().len() as i32
    }

    /// Dispatches a player-list action
    /// (`kick`/`ban`/`unban`/`admin`/`unadmin`/`whitelist`/`unwhitelist`/`team`/
    /// `trace`) for `target` (name or identity). Returns `true` when the reducer
    /// was sent. The UI player-list menu calls this per button; `trace` asks the
    /// module for a `my_traces` answer (`admin_request_trace`, GAPS #34).
    #[func]
    pub fn player_action(&mut self, action: GString, target: GString) -> bool {
        self.dispatch_player_action(action, target, 0)
    }

    /// [`Self::player_action`] with the team id for the `team` action.
    #[func]
    pub fn player_action_with(&mut self, action: GString, target: GString, value: i64) -> bool {
        self.dispatch_player_action(action, target, value)
    }

    /// Admin-action/session history for `target` (trace dialog JSON).
    #[func]
    pub fn player_trace_json(&self, target: GString) -> GString {
        let query = target.to_string();
        let identity = self
            .roster
            .find(query.as_str())
            .map(|entry| entry.identity)
            .unwrap_or(query);
        GString::from(
            self.roster
                .trace_json(identity.as_str())
                .to_string()
                .as_str(),
        )
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
