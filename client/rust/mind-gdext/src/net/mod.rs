// SPDX-License-Identifier: GPL-3.0-only

//! `MindNet` — the plan-21 multiplayer autoload (plan §3.15).
//!
//! Owns the `MatchSession`, the STDB `Connector`, the `RelayRuntime` mapping and
//! the `CommandSender`. It contains no sim rules: foreign commands are pushed
//! into `MindSimHost.pending_commands` and applied at the next tick boundary.
//!
//! Orchestration note: plan 00's `StdbConnector` already owns a connector for
//! the MCP/dev path. To avoid double-pumping, **either** add `MindNet` to
//! `client/scenes/spine.tscn`/`project.godot` and drop `StdbConnector`, **or**
//! keep `StdbConnector` for dev and instantiate `MindNet` lazily from the UI
//! (plan 14). This lane implements the Rust node; the scene wiring is left to
//! the orchestrator because `spine.tscn` is shared.

pub mod relay;

use godot::classes::{INode, Node, Os, ProjectSettings};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_stdb::binder::TableBinder;
use mind_stdb::commands::CommandSender;
use mind_stdb::module_bindings::{
    BreakBlock, CommandKind, Gamemode, MemberRole, MyMatchMembersTableAccessor,
    MyMatchStateTableAccessor, MyMatchTableAccessor, PlaceBlock, Visibility,
};
use mind_stdb::rows::RowView;
use mind_stdb::transport::StdbTransport;
use mind_stdb::{
    ConnectionConfig, Connector, ConnectorEvent, HostParams, MatchSession, RowChange, StdbMode,
    parse_player_suffix_from,
};

use crate::sim_host::MindSimHost;

use self::relay::RelayRuntime;

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
    last_match_json: String,
    last_members_json: String,
    last_state_json: String,
    last_checksum: String,
    authority: String,
    queue_depth: i64,
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
            last_match_json: String::new(),
            last_members_json: String::new(),
            last_state_json: String::new(),
            last_checksum: String::new(),
            authority: "relay".to_string(),
            queue_depth: 0,
        }
    }

    fn ready(&mut self) {
        let engine = collect_args(Os::singleton().get_cmdline_args().as_slice());
        let user = collect_args(Os::singleton().get_cmdline_user_args().as_slice());
        let suffix = parse_player_suffix_from(&engine, &user);
        let db_arg = engine.iter().any(|arg| arg == "--db") || user.iter().any(|arg| arg == "--db");
        let online = !self.offline || db_arg || suffix.is_some();

        let token_dir = ProjectSettings::singleton().globalize_path("user://");
        let config = ConnectionConfig {
            host: self.host.to_string(),
            db_name: self.db_name.to_string(),
            token_append: suffix,
            token_store_path: Some(std::path::PathBuf::from(token_dir.to_string())),
            mode: if online {
                StdbMode::Online
            } else {
                StdbMode::Offline
            },
            ..ConnectionConfig::default()
        };
        let mut connector = Connector::new(config);
        self.match_binder = Some(connector.bind::<MyMatchTableAccessor>("my_match"));
        self.members_binder =
            Some(connector.bind::<MyMatchMembersTableAccessor>("my_match_members"));
        self.state_binder = Some(connector.bind::<MyMatchStateTableAccessor>("my_match_state"));
        if online {
            if let Err(error) = connector.connect() {
                log::warn!("MindNet connect failed: {error}");
            }
        } else {
            log::info!("MindNet offline (single-player default)");
        }
        self.connector = Some(connector);
        self.sim_host = self
            .base()
            .try_get_node_as::<MindSimHost>("../SimHost")
            .or_else(|| {
                self.base()
                    .try_get_node_as::<MindSimHost>("/root/Spine/SimHost")
            });
    }

    fn process(&mut self, _delta: f64) {
        let Some(connector) = self.connector.as_mut() else {
            return;
        };
        connector.pump();
        for event in connector.drain_events() {
            match event {
                ConnectorEvent::Connected { identity } => {
                    self.session.set_local_identity(identity.identity);
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
        self.drain_members_rows();
        self.drain_state_rows();
        self.drain_relay();
    }

    fn exit_tree(&mut self) {
        if let Some(connector) = self.connector.as_mut() {
            connector.disconnect();
        }
    }
}

#[godot_api]
impl MindNet {
    /// Session state changed (`offline`/`in_lobby`/`in_game`/...).
    #[signal]
    fn session_changed(state: GString);

    /// Creates a match and enters the lobby as host (returns 0; id arrives via
    /// the `my_match` view).
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
                self.session.match_id().unwrap_or(0) as i64
            }
            Err(error) => {
                log::warn!("MindNet.create_match failed: {error}");
                0
            }
        }
    }

    /// Joins `match_id`; readiness/loading follows the view.
    #[func]
    pub fn join_match(&mut self, match_id: i64, password: GString) -> bool {
        if match_id <= 0 {
            return false;
        }
        let password = {
            let value = password.to_string();
            (!value.is_empty()).then_some(value)
        };
        let Some(connector) = self.connector.as_mut() else {
            return false;
        };
        match self.session.join(
            connector,
            match_id as u64,
            password,
            "",
            0,
            MemberRole::Player,
            Vec::new(),
        ) {
            Ok(()) => {
                self.ensure_relay();
                true
            }
            Err(error) => {
                log::warn!("MindNet.join_match failed: {error}");
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
        self.session.leave(connector).is_ok()
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
        self.session.set_ready(connector, ready).is_ok()
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
                log::warn!("MindNet.send_command_json failed: {error}");
                false
            }
        }
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
        out
    }

    /// Authority mode name (`relay` today; `authoritative` deferred).
    #[func]
    pub fn authority(&self) -> GString {
        GString::from(self.authority.as_str())
    }

    /// Remote player state mirror (plan M3; empty until then).
    #[func]
    pub fn get_remote_player_state(&self, _identity_hex: GString) -> Dictionary<GString, Variant> {
        Dictionary::new()
    }

    /// Requests a fresh host snapshot (plan M4/M5; not yet implemented).
    #[func]
    pub fn request_snapshot(&self) -> bool {
        log::warn!("MindNet.request_snapshot: snapshot path lands in plan 21 M4/M5");
        false
    }

    /// Snapshot download progress (plan M4/M5; always 0.0 until then).
    #[func]
    pub fn snapshot_progress(&self) -> f32 {
        0.0
    }

    /// Sends a chat message (plan M3; not yet implemented).
    #[func]
    pub fn send_chat(&self, _text: GString) -> bool {
        log::warn!("MindNet.send_chat: chat transport lands in plan 21 M3");
        false
    }

    /// Sends a UI result command (plan M3; not yet implemented).
    #[func]
    pub fn send_ui_result(&self, _kind: GString, _payload: GString) -> bool {
        log::warn!("MindNet.send_ui_result: plan 14/21 M3");
        false
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
    /// Creates the relay runtime/sender once a match id is known.
    fn ensure_relay(&mut self) {
        let Some(match_id) = self.session.match_id() else {
            return;
        };
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
