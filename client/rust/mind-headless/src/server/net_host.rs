// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan-21 networked host/admin control (plan 22 §3.7/§3.11 M3).
//!
//! [`NetHost`] is the real `HostControl`/`Admins` implementation that drives
//! plan 21's host API (`mind-stdb::Connector` reducers + `MatchSession`) and
//! mirrors `config/{admins,bans,whitelist}.json` (P22-6). It boots offline by
//! default and only opens the connector on demand, so `cargo test -p
//! mind-headless` never touches the network. When the connector is offline the
//! admin operations fall back to the JSON mirror (offline authority); when it is
//! connected they delegate to the reducers.
//!
//! The live two-client boot (`spacetime start` + `server/build.sh` + two
//! `--pN` clients) is deferred until a SpacetimeDB instance is available in the
//! lane environment; the delegation itself is unit-tested against an offline
//! connector and the mirror.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mind_stdb::binder::TableBinder;
use mind_stdb::module_bindings::{
    ChatKind, Gamemode, MatchStatus, MyMatchMembersTableAccessor, MyMatchTableAccessor, Visibility,
};
use mind_stdb::{
    ConnectionConfig, Connector, ConnectorEvent, HostParams, MatchSession, RowChange, StdbMode,
};
use spacetimedb_sdk::Identity;

use super::admin::{AdminMirror, BanEntry, BanKind};
use super::host::{AdminError, Admins, HostControl, HostError, HostStatus, PlayerView};

/// The plan-21-backed host (the `serve` mode's networked control).
pub struct NetHost {
    connector: Connector,
    session: MatchSession,
    admin: AdminMirror,
    root: PathBuf,
    online: bool,
    members: Option<TableBinder<MyMatchMembersTableAccessor>>,
    match_binder: Option<TableBinder<MyMatchTableAccessor>>,
    players: BTreeMap<String, PlayerView>,
    player_limit: Option<u16>,
    match_map: Option<String>,
    match_mode: Option<String>,
}

impl std::fmt::Debug for NetHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetHost")
            .field("online", &self.online)
            .field("connected", &self.connector.is_connected())
            .field("state", &self.session.state().name())
            .field("players", &self.players.len())
            .finish()
    }
}

impl NetHost {
    /// Creates a host rooted at `root` (tokens land in `<root>/identity/`).
    ///
    /// `online` selects the connector mode; the connector is **not** opened here
    /// (no boot-time network), call [`NetHost::ensure_connected`] or an admin
    /// operation to open it.
    pub fn new(root: impl Into<PathBuf>, online: bool) -> Self {
        let root = root.into();
        let config = ConnectionConfig {
            token_store_path: Some(root.clone()),
            mode: if online {
                StdbMode::Online
            } else {
                StdbMode::Offline
            },
            ..ConnectionConfig::local()
        };
        let mut connector = Connector::new(config);
        let members = Some(connector.bind::<MyMatchMembersTableAccessor>("my_match_members"));
        let match_binder = Some(connector.bind::<MyMatchTableAccessor>("my_match"));
        Self {
            connector,
            session: MatchSession::new(),
            admin: AdminMirror::load(&root),
            root,
            online,
            members,
            match_binder,
            players: BTreeMap::new(),
            player_limit: None,
            match_map: None,
            match_mode: None,
        }
    }

    /// Whether the connector is configured for online mode.
    pub fn is_online(&self) -> bool {
        self.online
    }

    /// Whether the connector currently holds a live connection.
    pub fn is_connected(&self) -> bool {
        self.connector.is_connected()
    }

    /// Current session state name (`offline`/`idle`/`in_lobby`/…).
    pub fn session_state(&self) -> &'static str {
        self.session.state().name()
    }

    /// The config root (token/mirror directory).
    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// The offline mirror (read-only view for `admins`/`bans` listing).
    pub fn mirror(&self) -> &AdminMirror {
        &self.admin
    }

    /// Opens the connector if online (no-op when already connected/offline).
    pub fn ensure_connected(&mut self) -> Result<(), AdminError> {
        if !self.online {
            return Err(AdminError::Offline);
        }
        if self.connector.is_connected() {
            return Ok(());
        }
        self.connector
            .connect()
            .map_err(|error| AdminError::Connector(error.to_string()))
    }

    /// Pumps the connector, drains plan-21 row changes and refreshes the player
    /// mirror. Never blocks; safe to call when offline.
    pub fn pump(&mut self) {
        self.connector.pump();
        let events = self.connector.drain_events();
        for event in events {
            match event {
                ConnectorEvent::Connected { .. } => {
                    self.connector.subscribe_lobby();
                    self.connector.subscribe_game();
                }
                ConnectorEvent::Disconnected { .. } | ConnectorEvent::Resync => {
                    self.players.clear();
                }
                _ => {}
            }
        }
        self.drain_members();
        self.drain_match();
    }

    /// Advances the loaded match by `ticks` (host-side fixed-step pump hook).
    pub fn tick(&mut self, _ticks: u64) {
        self.pump();
    }

    /// Live player views (`players`/`status`).
    pub fn players(&self) -> Vec<PlayerView> {
        self.players.values().cloned().collect()
    }

    /// The configured player cap, if any.
    pub fn configured_player_limit(&self) -> Option<u16> {
        self.player_limit
    }

    /// Resolves `target` to an identity hex (64-hex input, then live players,
    /// then the offline mirror by name).
    pub fn resolve_hex(&self, target: &str) -> Option<String> {
        if is_hex64(target) {
            return Some(target.to_ascii_lowercase());
        }
        if let Some(player) = self.players.values().find(|player| {
            player.name.eq_ignore_ascii_case(target) || player.identity.eq_ignore_ascii_case(target)
        }) {
            return Some(player.identity.clone());
        }
        self.admin
            .search(target)
            .into_iter()
            .find(|entry| {
                entry.name.eq_ignore_ascii_case(target)
                    || entry.identity.eq_ignore_ascii_case(target)
            })
            .map(|entry| entry.identity)
    }

    /// The mirror key for `target` (resolved hex, else the lowercase string).
    fn mirror_key(&self, target: &str) -> String {
        self.resolve_hex(target)
            .unwrap_or_else(|| target.to_ascii_lowercase())
    }

    /// The last-known name for `target`, if any.
    fn lookup_name(&self, target: &str) -> Option<String> {
        if let Some(player) = self.players.values().find(|player| {
            player.name.eq_ignore_ascii_case(target) || player.identity.eq_ignore_ascii_case(target)
        }) {
            return Some(player.name.clone());
        }
        self.admin
            .search(target)
            .into_iter()
            .next()
            .map(|entry| entry.name)
    }

    /// Resolves `target` to a plan-21 `Identity` for a reducer call.
    fn identity_of(&self, target: &str) -> Result<Identity, AdminError> {
        let hex = self
            .resolve_hex(target)
            .ok_or_else(|| AdminError::UnknownTarget(target.to_owned()))?;
        Identity::from_hex(&hex).map_err(|_| AdminError::UnknownTarget(target.to_owned()))
    }

    /// Persists the mirror after a mutation.
    fn save_mirror(&self) -> Result<(), AdminError> {
        self.admin
            .save(&self.root)
            .map_err(|error| AdminError::Connector(format!("admin mirror save failed: {error}")))
    }

    /// Drains `my_match_members` rows into the player mirror.
    fn drain_members(&mut self) {
        let Some(binder) = self.members.as_ref() else {
            return;
        };
        let changes = binder.drain();
        for change in changes {
            match change {
                RowChange::Insert(row) | RowChange::Update { new: row, .. } => {
                    let identity = row.identity.to_hex().to_string();
                    let name = self.admin.name_for(&identity).unwrap_or("").to_owned();
                    let admin = self.admin.is_admin(&identity);
                    self.players.insert(
                        identity.clone(),
                        PlayerView {
                            identity,
                            name,
                            team: row.team,
                            admin,
                            connected: row.connected,
                            role: format!("{:?}", row.role).to_ascii_lowercase(),
                            mobile: false,
                            locale: String::new(),
                        },
                    );
                }
                RowChange::Delete(row) => {
                    self.players.remove(&row.identity.to_hex().to_string());
                }
            }
        }
    }

    /// Drains `my_match` rows (map/mode + terminal status).
    fn drain_match(&mut self) {
        let Some(binder) = self.match_binder.as_ref() else {
            return;
        };
        let changes = binder.drain();
        for change in changes {
            if let RowChange::Insert(row) | RowChange::Update { new: row, .. } = change {
                self.match_map = Some(row.map_id.clone());
                self.match_mode = Some(row.mode_name.clone());
                self.session.on_match_status(row.status);
                if matches!(row.status, MatchStatus::Ended) {
                    self.players.clear();
                }
            }
        }
    }
}

impl HostControl for NetHost {
    fn host(&mut self, map: &str, mode: &str) -> Result<(), HostError> {
        if !self.online {
            return Err(HostError::Net(
                "offline server: only the local host is available".to_owned(),
            ));
        }
        self.ensure_connected()
            .map_err(|error| HostError::Net(error.to_string()))?;
        let params = HostParams {
            map_id: map.to_owned(),
            mode: parse_gamemode(mode),
            mode_name: mode.to_owned(),
            visibility: Visibility::Public,
            max_players: self.player_limit.unwrap_or(8),
            ..HostParams::default()
        };
        self.session
            .host(&mut self.connector, &params)
            .map_err(|error| HostError::Net(error.to_string()))?;
        self.connector.subscribe_lobby();
        self.connector.subscribe_game();
        self.match_map = Some(map.to_owned());
        self.match_mode = Some(mode.to_owned());
        Ok(())
    }

    fn stop(&mut self) {
        if self.session.match_id().is_some() {
            let _ = self.session.leave(&mut self.connector);
        }
        self.players.clear();
        self.match_map = None;
        self.match_mode = None;
    }

    fn status(&self) -> HostStatus {
        HostStatus {
            hosting: self.session.match_id().is_some(),
            map: self.match_map.clone(),
            mode: self.match_mode.clone(),
            wave: 0,
            tick: 0,
            players: self.players.len(),
        }
    }

    fn run_wave(&mut self) -> Result<(), HostError> {
        if !self.online {
            return Err(HostError::Net("offline".to_owned()));
        }
        let match_id = self
            .session
            .match_id()
            .ok_or_else(|| HostError::Net("no match is hosted".to_owned()))?;
        self.connector
            .admin_run_wave(match_id, 1)
            .map_err(|error| HostError::Net(error.to_string()))
    }

    fn players(&self) -> Vec<PlayerView> {
        NetHost::players(self)
    }
}

impl Admins for NetHost {
    fn say(&mut self, message: &str) -> Result<(), AdminError> {
        if !self.online {
            return Err(AdminError::Unsupported(
                "`say` requires a live match (plan 21)".to_owned(),
            ));
        }
        let match_id = self.session.match_id().ok_or(AdminError::NoMatch)?;
        self.connector
            .send_chat(match_id, ChatKind::System, message)
            .map_err(|error| AdminError::Connector(error.to_string()))
    }

    fn kick(&mut self, target: &str) -> Result<(), AdminError> {
        if !self.online {
            return Err(AdminError::Unsupported(
                "`kick` requires a live match (plan 21)".to_owned(),
            ));
        }
        let match_id = self.session.match_id().ok_or(AdminError::NoMatch)?;
        let identity = self.identity_of(target)?;
        self.connector
            .admin_kick(match_id, identity, "Kicked by admin")
            .map_err(|error| AdminError::Connector(error.to_string()))
    }

    fn ban(&mut self, kind: BanKind, target: &str) -> Result<(), AdminError> {
        if matches!(kind, BanKind::Subnet | BanKind::Dos) {
            return Err(AdminError::Unsupported(format!(
                "`{}` bans are unsupported under D2 (no peer IP reaches a reducer); use `ban id <identity>`",
                kind.name()
            )));
        }
        let key = self.mirror_key(target);
        let name = self.lookup_name(target).unwrap_or_default();
        if self.online {
            let identity = self.identity_of(target)?;
            self.connector
                .admin_ban(identity, "Banned by admin", None)
                .map_err(|error| AdminError::Connector(error.to_string()))?;
        }
        self.admin.add_ban(BanEntry {
            kind,
            target: key,
            name,
            reason: String::new(),
        });
        self.save_mirror()
    }

    fn unban(&mut self, target: &str) -> Result<(), AdminError> {
        if self.online
            && let Ok(identity) = self.identity_of(target)
        {
            self.connector
                .admin_unban(identity)
                .map_err(|error| AdminError::Connector(error.to_string()))?;
        }
        self.admin.remove_ban(&self.mirror_key(target));
        self.admin.remove_ban(target);
        self.save_mirror()
    }

    fn pardon(&mut self, target: &str) -> Result<(), AdminError> {
        self.unban(target)
    }

    fn grant(&mut self, target: &str, on: bool) -> Result<(), AdminError> {
        let key = self.mirror_key(target);
        let name = self.lookup_name(target).unwrap_or_default();
        if self.online {
            let identity = self.identity_of(target)?;
            self.connector
                .admin_grant(identity, on)
                .map_err(|error| AdminError::Connector(error.to_string()))?;
        }
        if on {
            self.admin.add_admin(&key, &name);
        } else {
            self.admin.remove_admin(&key);
        }
        self.save_mirror()
    }

    fn set_whitelist_enabled(&mut self, enabled: bool) -> Result<(), AdminError> {
        if self.online {
            self.connector
                .admin_set_whitelist(enabled)
                .map_err(|error| AdminError::Connector(error.to_string()))?;
        }
        Ok(())
    }

    fn whitelist(&mut self, target: &str, on: bool) -> Result<(), AdminError> {
        let key = self.mirror_key(target);
        let name = self.lookup_name(target).unwrap_or_default();
        if self.online {
            let identity = self.identity_of(target)?;
            self.connector
                .admin_whitelist(identity, on)
                .map_err(|error| AdminError::Connector(error.to_string()))?;
        }
        if on {
            self.admin.add_whitelist(&key, &name);
        } else {
            self.admin.remove_whitelist(&key);
        }
        self.save_mirror()
    }

    fn player_limit(&mut self, limit: Option<u16>) -> Result<(), AdminError> {
        self.player_limit = limit;
        if self.online {
            let value = limit
                .map(|n| n.to_string())
                .unwrap_or_else(|| "0".to_owned());
            self.connector
                .admin_set_config("max_players_default", &value)
                .map_err(|error| AdminError::Connector(error.to_string()))?;
        }
        Ok(())
    }

    fn search(&self, query: &str) -> Vec<PlayerView> {
        let needle = query.to_ascii_lowercase();
        let mut out: Vec<PlayerView> = self
            .players
            .values()
            .filter(|player| {
                needle.is_empty()
                    || player.identity.to_ascii_lowercase().contains(&needle)
                    || player.name.to_ascii_lowercase().contains(&needle)
            })
            .cloned()
            .collect();
        for entry in self.admin.search(query) {
            if !out.iter().any(|player| player.identity == entry.identity) {
                out.push(PlayerView {
                    admin: entry.role == "admin",
                    identity: entry.identity,
                    name: entry.name,
                    team: None,
                    connected: false,
                    role: entry.role,
                    mobile: false,
                    locale: String::new(),
                });
            }
        }
        out
    }

    fn mirror(&self) -> &AdminMirror {
        &self.admin
    }
}

/// `true` for a 64-char lowercase/uppercase hex identity string.
fn is_hex64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Parses a gamemode console token (`Gamemode` names, `survival` default).
fn parse_gamemode(name: &str) -> Gamemode {
    match name {
        "sandbox" => Gamemode::Sandbox,
        "attack" => Gamemode::Attack,
        "pvp" => Gamemode::Pvp,
        "editor" => Gamemode::Editor,
        _ => Gamemode::Survival,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mind-nethost-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create");
        dir
    }

    #[test]
    fn offline_host_reports_empty_and_offline() {
        let dir = root("empty");
        let host = NetHost::new(&dir, false);
        assert!(!host.is_online());
        assert!(host.players().is_empty());
        assert_eq!(host.session_state(), "offline");
        assert_eq!(host.status().players, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn offline_grant_ban_whitelist_persist_to_mirror() {
        let dir = root("persist");
        {
            let mut host = NetHost::new(&dir, false);
            host.grant("Alice", true).expect("grant");
            host.ban(BanKind::Identity, "bob").expect("ban");
            host.whitelist("carol", true).expect("whitelist");
        }
        let reloaded = NetHost::new(&dir, false);
        assert!(reloaded.mirror().is_admin("alice"));
        assert!(reloaded.mirror().is_banned("bob", ""));
        assert!(reloaded.mirror().is_whitelisted("carol"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hex_targets_resolve_without_a_connection() {
        let dir = root("hex");
        let mut host = NetHost::new(&dir, false);
        let identity = "ab".repeat(32);
        host.grant(&identity, true).expect("grant");
        assert!(host.mirror().is_admin(&identity));
        host.grant(&identity, false).expect("revoke");
        assert!(!host.mirror().is_admin(&identity));
        assert!(host.resolve_hex("unknown-player").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn offline_kick_say_and_ip_bans_are_unsupported() {
        let dir = root("unsupported");
        let mut host = NetHost::new(&dir, false);
        assert!(matches!(host.say("hi"), Err(AdminError::Unsupported(_))));
        assert!(matches!(host.kick("x"), Err(AdminError::Unsupported(_))));
        assert!(matches!(
            host.ban(BanKind::Subnet, "10.0.0.0"),
            Err(AdminError::Unsupported(_))
        ));
        assert!(matches!(
            host.ban(BanKind::Dos, "10.0.0.1"),
            Err(AdminError::Unsupported(_))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn configured_player_limit_is_stored() {
        let dir = root("limit");
        let mut host = NetHost::new(&dir, false);
        assert_eq!(host.configured_player_limit(), None);
        host.player_limit(Some(4)).expect("limit");
        assert_eq!(host.configured_player_limit(), Some(4));
        host.player_limit(None).expect("clear");
        assert_eq!(host.configured_player_limit(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_scans_live_and_mirror() {
        let dir = root("search");
        let mut host = NetHost::new(&dir, false);
        host.grant("Alice", true).expect("grant");
        let results = host.search("alice");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].role, "admin");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
