// SPDX-License-Identifier: GPL-3.0-only

//! `StdbConnector` autoload + `StdbBinder` node (plan 01 §3.11).
//!
//! Thin Godot shell around `mind-stdb`: one `pump()` per frame, signals for the
//! GDScript UI, plus the MCP/diagnostic helpers. **No game rules live here**
//! (HIGH_LEVEL_PLAN §2.2): gameplay code consumes `mind-stdb` types directly;
//! this node exists for autoload wiring, the inspector `net` page and MCP.

use std::collections::HashMap;
use std::path::PathBuf;

use godot::classes::{INode, Node, Os, ProjectSettings};
use godot::obj::Singleton;
use godot::prelude::*;

use mind_stdb::binder::{BinderOptions, TableBinder};
use mind_stdb::module_bindings::{
    CommandKind, Gamemode, MemberRole, MyMatchesTableAccessor, PlayerTableAccessor,
    ProtocolInfoTableAccessor, RelayConfigTableAccessor, Visibility,
};
use mind_stdb::rows::RowView;
use mind_stdb::{
    CommandStream, ConnectionConfig, Connector, ConnectorEvent, LocalIdentity, OrderError,
    RowChange, StdbMode, WaveName, parse_player_suffix_from,
};

/// One drained binder queue in debug-JSON form.
#[derive(Debug, Default)]
struct BinderDrain {
    inserts: u64,
    updates: u64,
    deletes: u64,
    last_row: Option<String>,
    last_deleted_row: Option<String>,
}

/// Type-erased binder owned by the autoload, keyed by the `StdbBinder` node path.
enum AnyBinder {
    ProtocolInfo(TableBinder<ProtocolInfoTableAccessor>),
    RelayConfig(TableBinder<RelayConfigTableAccessor>),
    Player(TableBinder<PlayerTableAccessor>),
    MyMatches(TableBinder<MyMatchesTableAccessor>),
}

impl AnyBinder {
    /// Creates a binder for a known accessor name; `None` for unsupported tables.
    fn attach(connector: &mut Connector, table: &str, options: BinderOptions) -> Option<Self> {
        match table {
            "protocol_info" => Some(Self::ProtocolInfo(
                connector.bind_with_replay::<ProtocolInfoTableAccessor>("protocol_info", options),
            )),
            "relay_config" => Some(Self::RelayConfig(
                connector.bind_with_replay::<RelayConfigTableAccessor>("relay_config", options),
            )),
            // Views have no primary key: live-only.
            "local_player" => Some(Self::Player(
                connector.bind::<PlayerTableAccessor>("local_player"),
            )),
            "my_matches" => Some(Self::MyMatches(
                connector.bind::<MyMatchesTableAccessor>("my_matches"),
            )),
            _ => None,
        }
    }

    fn drain(&self) -> BinderDrain {
        let mut drain = BinderDrain::default();
        match self {
            Self::ProtocolInfo(binder) => drain_into(binder.drain(), &mut drain),
            Self::RelayConfig(binder) => drain_into(binder.drain(), &mut drain),
            Self::Player(binder) => drain_into(binder.drain(), &mut drain),
            Self::MyMatches(binder) => drain_into(binder.drain(), &mut drain),
        }
        drain
    }
}

fn drain_into<Row: RowView>(changes: Vec<RowChange<Row>>, drain: &mut BinderDrain) {
    for change in changes {
        match change {
            RowChange::Insert(row) => {
                drain.inserts = drain.inserts.saturating_add(1);
                drain.last_row = Some(row.debug_json());
            }
            RowChange::Update { new, .. } => {
                drain.updates = drain.updates.saturating_add(1);
                drain.last_row = Some(new.debug_json());
            }
            RowChange::Delete(row) => {
                drain.deletes = drain.deletes.saturating_add(1);
                drain.last_deleted_row = Some(row.debug_json());
            }
        }
    }
}

/// Autoload node owning the one `mind-stdb` connector (plan §3.11).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct StdbConnector {
    base: Base<Node>,
    /// SpacetimeDB host URL.
    #[export]
    pub host: GString,
    /// Database name.
    #[export]
    pub db_name: GString,
    /// Scene default: stay offline unless `--db`/`--pN` opt in.
    #[export]
    pub offline: bool,
    /// `--pN` suffix parsed at `_ready` (token scoping).
    #[export]
    pub token_suffix: GString,
    connector: Option<Connector>,
    matches: Option<TableBinder<MyMatchesTableAccessor>>,
    stream: Option<CommandStream>,
    match_id: u64,
    next_tick: u64,
    binder_nodes: HashMap<String, AnyBinder>,
    last_match_id: i64,
    applied_count: u64,
    last_command_id: u64,
    order_error: GString,
}

#[godot_api]
impl INode for StdbConnector {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            host: GString::from("http://127.0.0.1:3000"),
            db_name: GString::from("mindustry"),
            offline: true,
            token_suffix: GString::new(),
            connector: None,
            matches: None,
            stream: None,
            match_id: 0,
            next_tick: 0,
            binder_nodes: HashMap::new(),
            last_match_id: -1,
            applied_count: 0,
            last_command_id: 0,
            order_error: GString::new(),
        }
    }

    fn ready(&mut self) {
        let engine_args = Os::singleton().get_cmdline_args();
        let user_args = Os::singleton().get_cmdline_user_args();
        let engine = collect_args(engine_args.as_slice());
        let user = collect_args(user_args.as_slice());
        let suffix = parse_player_suffix_from(&engine, &user);
        let db_arg = user.iter().any(|arg| arg == "--db") || engine.iter().any(|arg| arg == "--db");
        if let Some(parsed) = &suffix {
            self.token_suffix = GString::from(parsed.as_str());
        }
        // `--pN`/`--db` opt into online; the scene default is offline.
        let online = !self.offline || db_arg || suffix.is_some();

        let token_dir = ProjectSettings::singleton().globalize_path("user://");
        let config = ConnectionConfig {
            host: self.host.to_string(),
            db_name: self.db_name.to_string(),
            token_append: suffix,
            token_store_path: Some(PathBuf::from(token_dir.to_string())),
            mode: if online {
                StdbMode::Online
            } else {
                StdbMode::Offline
            },
            ..ConnectionConfig::default()
        };
        let mut connector = Connector::new(config);
        // Bound before connect so the Lobby wave's snapshot reaches it.
        let matches = connector.bind::<MyMatchesTableAccessor>("my_matches");
        if !online {
            log::info!("StdbConnector offline (single-player default)");
        } else if let Err(error) = connector.connect() {
            log::warn!("StdbConnector connect failed: {error}");
        }
        self.connector = Some(connector);
        self.matches = Some(matches);
    }

    fn process(&mut self, _delta: f64) {
        let Some(connector) = self.connector.as_mut() else {
            return;
        };
        connector.pump();
        let events = connector.drain_events();
        for event in events {
            match event {
                ConnectorEvent::Connected { identity } => {
                    self.emit_connected(&identity);
                }
                ConnectorEvent::Disconnected { reason } => {
                    self.base_mut().emit_signal(
                        "disconnected",
                        &[GString::from(reason.as_str()).to_variant()],
                    );
                }
                ConnectorEvent::ConnectError { message } => {
                    self.base_mut().emit_signal(
                        "connect_error",
                        &[GString::from(message.as_str()).to_variant()],
                    );
                }
                ConnectorEvent::WaveApplied(wave) => {
                    self.base_mut()
                        .emit_signal("wave_applied", &[GString::from(wave.name()).to_variant()]);
                }
                ConnectorEvent::WaveError { wave, message } => {
                    self.base_mut().emit_signal(
                        "wave_error",
                        &[
                            GString::from(wave.name()).to_variant(),
                            GString::from(message.as_str()).to_variant(),
                        ],
                    );
                }
                ConnectorEvent::Resync => {
                    self.base_mut().emit_signal("resync", &[]);
                }
            }
        }
        // Cache relay/MCP diagnostics once per frame.
        if let Some(matches) = &self.matches {
            for change in matches.drain() {
                if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                    self.last_match_id = row.match_id as i64;
                }
            }
        }
        if let Some(stream) = self.stream.as_mut() {
            let rows = stream.drain();
            let _ = rows.len();
            self.applied_count = stream.applied_count();
            self.last_command_id = stream.last_command_id();
            self.order_error = match stream.order_error() {
                Some(OrderError::Gap { expected, got, .. }) => {
                    GString::from(format!("gap: expected {expected}, got {got}").as_str())
                }
                Some(OrderError::Regression { previous, incoming }) => {
                    GString::from(format!("regression: {previous} -> {incoming}").as_str())
                }
                None => GString::new(),
            };
        }
    }

    fn exit_tree(&mut self) {
        if let Some(connector) = self.connector.as_mut() {
            connector.disconnect();
        }
    }
}

#[godot_api]
impl StdbConnector {
    /// Connection established (call `local_identity_hex()` for the identity).
    #[signal]
    fn connected();

    /// Connection lost; `reason` is diagnostic. Never fires on deliberate
    /// `disconnect()`.
    #[signal]
    fn disconnected(reason: GString);

    /// Async connect error before a session was established.
    #[signal]
    fn connect_error(message: GString);

    /// A subscription wave applied (`wave` is `"base"`/`"lobby"`/`"game"`).
    #[signal]
    fn wave_applied(wave: GString);

    /// A subscription wave failed.
    #[signal]
    fn wave_error(wave: GString, message: GString);

    /// State may be stale after a reconnect; consumers drop mirrors.
    #[signal]
    fn resync();

    /// Current connector state name (`"offline"`, `"connected"`, ...).
    #[func]
    pub fn state(&self) -> GString {
        match &self.connector {
            Some(connector) => GString::from(connector.state().name()),
            None => GString::from("offline"),
        }
    }

    /// 64-hex identity, or empty when not connected.
    #[func]
    pub fn local_identity_hex(&self) -> GString {
        match self.connector.as_ref().and_then(Connector::local_identity) {
            Some(identity) => GString::from(identity.hex().as_str()),
            None => GString::new(),
        }
    }

    /// `--pN` suffix parsed at boot (`_p1`, ...), or empty.
    #[func]
    pub fn player_suffix(&self) -> GString {
        self.token_suffix.clone()
    }

    /// Opens the connection (offline mode keeps it offline).
    ///
    /// Rust name is `connect_db`: `Object::connect` exists on `Node` and the
    /// Godot-visible name is pinned by plan 01 §3.11 (`connect`).
    #[func(rename = "connect")]
    pub fn connect_db(&mut self) {
        if let Some(connector) = self.connector.as_mut()
            && let Err(error) = connector.connect()
        {
            log::warn!("StdbConnector connect failed: {error}");
        }
    }

    /// Drops the link and reopens it.
    #[func]
    pub fn reconnect(&mut self) {
        if let Some(connector) = self.connector.as_mut()
            && let Err(error) = connector.reconnect()
        {
            log::warn!("StdbConnector reconnect failed: {error}");
        }
    }

    /// Deliberate close (no `disconnected` signal).
    ///
    /// Rust name is `disconnect_db`: `Object::disconnect` exists on `Node` and
    /// the Godot-visible name is pinned by plan 01 §3.11 (`disconnect`).
    #[func(rename = "disconnect")]
    pub fn disconnect_db(&mut self) {
        if let Some(connector) = self.connector.as_mut() {
            connector.disconnect();
        }
    }

    /// Issues the Game wave.
    #[func]
    pub fn subscribe_game(&mut self) {
        if let Some(connector) = self.connector.as_mut() {
            connector.subscribe_game();
        }
    }

    /// Drops the Game wave.
    #[func]
    pub fn unsubscribe_game(&mut self) {
        if let Some(connector) = self.connector.as_mut() {
            connector.unsubscribe_game();
        }
    }

    /// Whether `wave` ("base"/"lobby"/"game") applied on this connection.
    #[func]
    pub fn wave_applied_state(&self, wave: GString) -> bool {
        let name = wave.to_string();
        let Some(target) = WaveName::ALL
            .into_iter()
            .find(|candidate| candidate.name() == name)
        else {
            return false;
        };
        match &self.connector {
            Some(connector) => connector.is_applied(target),
            None => false,
        }
    }

    /// `StdbBinder` attach hook: binds `table_name` for `node_path`.
    #[func]
    pub fn attach_binder(
        &mut self,
        node_path: GString,
        table_name: GString,
        replay_existing: bool,
        verbose: bool,
    ) -> bool {
        let options = BinderOptions {
            replay_existing,
            verbose,
        };
        let table = table_name.to_string();
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match AnyBinder::attach(connector, &table, options) {
            Some(binder) => {
                self.binder_nodes.insert(node_path.to_string(), binder);
                true
            }
            None => {
                log::warn!("StdbBinder: unsupported table `{table}`");
                false
            }
        }
    }

    /// `StdbBinder` drain hook: JSON `{inserts,updates,deletes,last,last_deleted}`.
    #[func]
    pub fn drain_binder_json(&mut self, node_path: GString) -> GString {
        let Some(binder) = self.binder_nodes.get(&node_path.to_string()) else {
            return GString::from("{}");
        };
        let drain = binder.drain();
        let json = serde_json::json!({
            "inserts": drain.inserts,
            "updates": drain.updates,
            "deletes": drain.deletes,
            "last": drain.last_row,
            "last_deleted": drain.last_deleted_row,
        });
        GString::from(json.to_string().as_str())
    }

    /// Dev helper (MCP): creates a match; returns whether the call was sent.
    #[func]
    pub fn dev_create_match(&mut self, map_id: GString, map_seed: i64) -> bool {
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match connector.create_match(
            &map_id.to_string(),
            map_seed.max(0) as u64,
            Gamemode::Survival,
            "survival",
            Visibility::Public,
            None,
            8,
            "{}",
            "",
            0,
            Vec::new(),
        ) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("dev_create_match failed: {error}");
                false
            }
        }
    }

    /// Dev helper (MCP): newest match ID seen in `my_matches` (`-1` = none).
    #[func]
    pub fn dev_match_id(&self) -> i64 {
        self.last_match_id
    }

    /// Dev helper (MCP): joins `match_id` and starts the ordered stream.
    #[func]
    pub fn dev_join_match(&mut self, match_id: i64) -> bool {
        if match_id < 0 {
            return false;
        }
        let match_id = match_id as u64;
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        if let Err(error) =
            connector.join_match(match_id, None, "", 0, MemberRole::Player, Vec::new())
        {
            log::warn!("dev_join_match failed: {error}");
            return false;
        }
        self.match_id = match_id;
        self.stream = Some(CommandStream::subscribe(connector, match_id));
        true
    }

    /// Dev helper (MCP): starts a lobby match.
    #[func]
    pub fn dev_start_match(&mut self, match_id: i64) -> bool {
        if match_id < 0 {
            return false;
        }
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match connector.start_match(match_id as u64, true) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("dev_start_match failed: {error}");
                false
            }
        }
    }

    /// Dev helper (MCP): sends a `Ping` on the current match.
    #[func]
    pub fn dev_send_ping(&mut self, nonce: i64) -> bool {
        let tick = self.next_tick;
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match connector.send_match_command(
            self.match_id,
            tick,
            CommandKind::Ping(nonce.max(0) as u64),
        ) {
            Ok(()) => {
                self.next_tick = tick.saturating_add(1);
                true
            }
            Err(error) => {
                log::warn!("dev_send_ping failed: {error}");
                false
            }
        }
    }

    /// Dev helper (MCP): commands applied by the ordered stream.
    #[func]
    pub fn dev_relay_applied_count(&self) -> i64 {
        self.applied_count as i64
    }

    /// Dev helper (MCP): highest applied `command_id` (0 = none).
    #[func]
    pub fn dev_last_command_id(&self) -> i64 {
        self.last_command_id as i64
    }

    /// Dev helper (MCP): first order error, or empty.
    #[func]
    pub fn relay_order_error(&self) -> GString {
        self.order_error.clone()
    }
}

impl StdbConnector {
    fn emit_connected(&mut self, identity: &LocalIdentity) {
        let hex = GString::from(identity.hex().as_str());
        log::info!("StdbConnector connected as {hex}");
        self.base_mut().emit_signal("connected", &[]);
    }
}

/// Copies a Godot `PackedStringArray` slice into owned `String`s.
fn collect_args(args: &[GString]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

/// `StdbBinder` — layout/debug node for one table (plan §3.11).
///
/// Typed gameplay/UI code consumes `mind-stdb` binders directly; this node
/// exists so scenes can wire `row_inserted`/`row_updated`/`row_deleted`
/// handlers and inspect `last_row_json` without Rust.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct StdbBinder {
    base: Base<Node>,
    /// Generated accessor name (`local_player`, `relay_config`, ...).
    #[export]
    pub table_name: GString,
    /// Enqueue cached rows on attach (primary-key tables only).
    #[export]
    pub replay_existing: bool,
    /// Verbose drain logging.
    #[export]
    pub verbose: bool,
    node_key: String,
    last_row_json: GString,
    last_deleted_row_json: GString,
}

#[godot_api]
impl INode for StdbBinder {
    fn init(base: Base<Node>) -> Self {
        Self {
            base,
            table_name: GString::new(),
            replay_existing: false,
            verbose: false,
            node_key: String::new(),
            last_row_json: GString::new(),
            last_deleted_row_json: GString::new(),
        }
    }

    fn ready(&mut self) {
        self.node_key = self.base().get_path().to_string();
        let Some(mut connector) = self
            .base()
            .try_get_node_as::<StdbConnector>("/root/StdbConnector")
        else {
            log::warn!(
                "StdbBinder `{}`: no /root/StdbConnector autoload",
                self.node_key
            );
            return;
        };
        let table = self.table_name.clone();
        let key = GString::from(self.node_key.as_str());
        let replay = self.replay_existing;
        let verbose = self.verbose;
        if !connector
            .bind_mut()
            .attach_binder(key, table, replay, verbose)
        {
            return;
        }
        log::info!(
            "StdbBinder `{}` attached to `{}` (replay {replay})",
            self.node_key,
            self.table_name
        );
    }

    fn process(&mut self, _delta: f64) {
        if self.node_key.is_empty() {
            return;
        }
        let Some(mut connector) = self
            .base()
            .try_get_node_as::<StdbConnector>("/root/StdbConnector")
        else {
            return;
        };
        let json = connector
            .bind_mut()
            .drain_binder_json(GString::from(self.node_key.as_str()));
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json.to_string()) else {
            return;
        };
        let inserts = parsed.get("inserts").and_then(|v| v.as_u64()).unwrap_or(0);
        let updates = parsed.get("updates").and_then(|v| v.as_u64()).unwrap_or(0);
        let deletes = parsed.get("deletes").and_then(|v| v.as_u64()).unwrap_or(0);
        if let Some(last) = parsed.get("last").and_then(|v| v.as_str()) {
            self.last_row_json = GString::from(last);
        }
        if let Some(last) = parsed.get("last_deleted").and_then(|v| v.as_str()) {
            self.last_deleted_row_json = GString::from(last);
        }
        if inserts > 0 {
            self.base_mut().emit_signal("row_inserted", &[]);
        }
        if updates > 0 {
            self.base_mut().emit_signal("row_updated", &[]);
        }
        if deletes > 0 {
            self.base_mut().emit_signal("row_deleted", &[]);
        }
        if self.verbose && (inserts + updates + deletes) > 0 {
            log::debug!(
                "StdbBinder `{}`: +{inserts} ~{updates} -{deletes}",
                self.node_key
            );
        }
    }
}

#[godot_api]
impl StdbBinder {
    /// A row entered the bound set (inspect `last_row_json`).
    #[signal]
    fn row_inserted();

    /// A row changed (inspect `last_row_json`).
    #[signal]
    fn row_updated();

    /// A row left the bound set (inspect `last_deleted_row_json`).
    #[signal]
    fn row_deleted();

    /// JSON of the most recently inserted/updated row (empty before any).
    #[func]
    pub fn last_row_json(&self) -> GString {
        self.last_row_json.clone()
    }

    /// JSON of the most recently deleted row (empty before any).
    #[func]
    pub fn last_deleted_row_json(&self) -> GString {
        self.last_deleted_row_json.clone()
    }
}
