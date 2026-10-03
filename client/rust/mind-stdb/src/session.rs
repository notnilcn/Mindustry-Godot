// SPDX-License-Identifier: GPL-3.0-only

//! `MatchSession` — the multiplayer lifecycle state machine (plan 21 §3.1/§3.3).
//!
//! The session is a Godot-free, network-free state machine layered over
//! [`Connector`] reducer calls. It never runs the simulation and never reads
//! STDB rows directly: hosts/joiners drive reducers, then feed observed rows
//! back in (`observe_match`, `on_match_status`). Single-player keeps the
//! session in [`SessionState::Offline`] and never calls a reducer.

use spacetimedb_sdk::Identity;

use crate::connector::{Connector, ConnectorError};
use crate::module_bindings::{Gamemode, MatchStatus, MemberRole, RelayMatch, Visibility};

/// High-level lifecycle state (plan §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Single-player / no match; reducers are no-ops.
    Offline,
    /// Reading `all_matches` for the server browser.
    Browsing,
    /// Created or joined a lobby; waiting for the host to start.
    InLobby,
    /// Joined; loading/regenerating the world before the match starts.
    Loading,
    /// Match running and commands relayed.
    InGame,
    /// Full-resync snapshot download in progress.
    SnapshotSync,
    /// Connection lost; reconnect policy is retrying.
    Reconnecting,
}

impl SessionState {
    /// Stable debug/log name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Browsing => "browsing",
            Self::InLobby => "in_lobby",
            Self::Loading => "loading",
            Self::InGame => "in_game",
            Self::SnapshotSync => "snapshot_sync",
            Self::Reconnecting => "reconnecting",
        }
    }
}

/// Parameters for [`MatchSession::host`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostParams {
    /// Map content id.
    pub map_id: String,
    /// Deterministic map seed.
    pub map_seed: u64,
    /// Gamemode tag.
    pub mode: Gamemode,
    /// Human-readable mode name.
    pub mode_name: String,
    /// Browser visibility.
    pub visibility: Visibility,
    /// Optional lobby password.
    pub password: Option<String>,
    /// Player cap.
    pub max_players: u16,
    /// Initial rules JSON.
    pub rules_json: String,
    /// Client build id.
    pub build_id: String,
    /// Content manifest hash.
    pub content_hash: u64,
    /// Required mod names.
    pub mods: Vec<String>,
}

impl Default for HostParams {
    fn default() -> Self {
        Self {
            map_id: "demo_flat".to_string(),
            map_seed: 1,
            mode: Gamemode::Survival,
            mode_name: "survival".to_string(),
            visibility: Visibility::Public,
            password: None,
            max_players: 8,
            rules_json: "{}".to_string(),
            build_id: String::new(),
            content_hash: 0,
            mods: Vec::new(),
        }
    }
}

/// Session errors (invalid transitions or reducer send failures).
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    /// The connector is offline (`StdbMode::Offline`).
    #[error(transparent)]
    Connector(#[from] ConnectorError),
    /// The requested transition is not valid from the current state.
    #[error("session is {state}, cannot {action}")]
    InvalidTransition {
        /// Current state name.
        state: &'static str,
        /// Attempted action.
        action: &'static str,
    },
    /// No match is active.
    #[error("no active match")]
    NoMatch,
}

/// Multiplayer lifecycle state machine.
#[derive(Debug, Clone)]
pub struct MatchSession {
    state: SessionState,
    match_id: Option<u64>,
    local_identity: Option<Identity>,
    host_identity: Option<Identity>,
    is_host: bool,
    ready: bool,
    role: MemberRole,
    resync_count: u32,
}

impl Default for MatchSession {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchSession {
    /// A fresh session in [`SessionState::Offline`].
    pub fn new() -> Self {
        Self {
            state: SessionState::Offline,
            match_id: None,
            local_identity: None,
            host_identity: None,
            is_host: false,
            ready: false,
            role: MemberRole::Player,
            resync_count: 0,
        }
    }

    /// Current lifecycle state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Active match id, if any.
    pub fn match_id(&self) -> Option<u64> {
        self.match_id
    }

    /// Whether this client is the match host.
    pub fn is_host(&self) -> bool {
        self.is_host
    }

    /// Host identity once observed from `my_match`.
    pub fn host_identity(&self) -> Option<Identity> {
        self.host_identity
    }

    /// Whether this client marked itself ready.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Membership role.
    pub fn role(&self) -> MemberRole {
        self.role
    }

    /// Number of full-resyncs entered (diagnostic).
    pub fn resync_count(&self) -> u32 {
        self.resync_count
    }

    /// Records the local identity (after `ConnectorEvent::Connected`).
    pub fn set_local_identity(&mut self, identity: Identity) {
        self.local_identity = Some(identity);
    }

    /// Enters [`SessionState::Browsing`] (server browser).
    pub fn open_browser(&mut self) {
        if self.state == SessionState::Offline {
            self.state = SessionState::Browsing;
        }
    }

    /// Creates a match and enters [`SessionState::InLobby`] as host.
    pub fn host(
        &mut self,
        connector: &mut Connector,
        params: &HostParams,
    ) -> Result<(), SessionError> {
        connector.create_match(
            &params.map_id,
            params.map_seed,
            params.mode,
            &params.mode_name,
            params.visibility,
            params.password.clone(),
            params.max_players,
            &params.rules_json,
            &params.build_id,
            params.content_hash,
            params.mods.clone(),
        )?;
        self.is_host = true;
        self.ready = true;
        self.role = MemberRole::Player;
        self.state = SessionState::InLobby;
        Ok(())
    }

    /// Joins `match_id`; enters [`SessionState::Loading`] then lobby/game.
    #[allow(clippy::too_many_arguments)]
    pub fn join(
        &mut self,
        connector: &mut Connector,
        match_id: u64,
        password: Option<String>,
        build_id: &str,
        content_hash: u64,
        role: MemberRole,
        mods: Vec<String>,
    ) -> Result<(), SessionError> {
        connector.join_match(match_id, password, build_id, content_hash, role, mods)?;
        self.match_id = Some(match_id);
        self.role = role;
        self.ready = false;
        self.is_host = false;
        self.state = SessionState::Loading;
        Ok(())
    }

    /// Marks readiness and enters the lobby.
    pub fn set_ready(
        &mut self,
        connector: &mut Connector,
        ready: bool,
    ) -> Result<(), SessionError> {
        let match_id = self.match_id.ok_or(SessionError::NoMatch)?;
        connector.set_ready(match_id, ready)?;
        self.ready = ready;
        if self.state == SessionState::Loading {
            self.state = SessionState::InLobby;
        }
        Ok(())
    }

    /// Starts the match (host only); the status observation moves everyone.
    pub fn start(&mut self, connector: &mut Connector, force: bool) -> Result<(), SessionError> {
        let match_id = self.match_id.ok_or(SessionError::NoMatch)?;
        if !self.is_host {
            return Err(SessionError::InvalidTransition {
                state: self.state.name(),
                action: "start (not host)",
            });
        }
        connector.start_match(match_id, force)?;
        Ok(())
    }

    /// Leaves the active match and returns to [`SessionState::Offline`].
    pub fn leave(&mut self, connector: &mut Connector) -> Result<(), SessionError> {
        let match_id = self.match_id.ok_or(SessionError::NoMatch)?;
        connector.leave_match(match_id)?;
        self.reset();
        Ok(())
    }

    /// Feed a `RelayMatch` row observed from `my_match`.
    pub fn observe_match(&mut self, row: &RelayMatch, local: &Identity) {
        self.match_id = Some(row.match_id);
        self.host_identity = Some(row.host);
        self.is_host = row.host == *local;
        if row.host == *local && self.state == SessionState::Browsing {
            self.state = SessionState::InLobby;
        }
        self.on_match_status(row.status);
    }

    /// Apply a match status transition observed from rows.
    pub fn on_match_status(&mut self, status: MatchStatus) {
        match status {
            MatchStatus::Lobby => {
                if matches!(self.state, SessionState::Loading) {
                    self.state = SessionState::InLobby;
                }
            }
            MatchStatus::Running => self.state = SessionState::InGame,
            MatchStatus::Ended => self.reset(),
        }
    }

    /// Connection lost: pause into [`SessionState::Reconnecting`] when a match
    /// is active (dedicated servers never pause a client).
    pub fn on_disconnected(&mut self) {
        if !matches!(self.state, SessionState::Offline | SessionState::Browsing) {
            self.state = SessionState::Reconnecting;
        }
    }

    /// A `Resync` event re-delivers rows; the state is preserved.
    pub fn on_resync(&mut self) {}

    /// Enter snapshot full-resync (plan §3.7).
    pub fn enter_snapshot_sync(&mut self) {
        self.resync_count = self.resync_count.saturating_add(1);
        self.state = SessionState::SnapshotSync;
    }

    /// Leave snapshot sync after a successful restore.
    pub fn exit_snapshot_sync(&mut self) {
        if self.state == SessionState::SnapshotSync {
            self.state = SessionState::InGame;
        }
    }

    fn reset(&mut self) {
        self.state = SessionState::Offline;
        self.match_id = None;
        self.host_identity = None;
        self.is_host = false;
        self.ready = false;
        self.role = MemberRole::Player;
    }
}

/// Whether a member with `role` may emit a command whose variant is
/// `spectator_forbidden` (plan §6.3 “ROLE”).
pub fn can_emit(role: MemberRole, spectator_forbidden: bool) -> bool {
    !(role == MemberRole::Spectator && spectator_forbidden)
}

/// Whether the host may start given per-member readiness (plan §3.3).
pub fn can_start(force: bool, ready: impl IntoIterator<Item = bool>) -> bool {
    force || ready.into_iter().all(|flag| flag)
}

/// Whether the caller is allowed to host/join as a spectator (always yes).
pub fn can_spectate() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConnectionConfig, StdbMode};

    fn offline_connector() -> Connector {
        let mut config = ConnectionConfig::local();
        config.mode = StdbMode::Offline;
        Connector::new(config)
    }

    #[test]
    fn join_states() {
        let mut session = MatchSession::new();
        assert_eq!(session.state(), SessionState::Offline);
        session.open_browser();
        assert_eq!(session.state(), SessionState::Browsing);

        // Offline connector still exercises the local transition on send error?
        // No: an offline join is rejected and the state is unchanged.
        let mut connector = offline_connector();
        let result = session.join(
            &mut connector,
            7,
            None,
            "",
            0,
            MemberRole::Player,
            Vec::new(),
        );
        assert!(result.is_err());
        assert_eq!(session.state(), SessionState::Browsing);
    }

    #[test]
    fn ready_gating() {
        assert!(can_start(false, [true, true, true]));
        assert!(!can_start(false, [true, false]));
        assert!(can_start(true, [false, false]));
        assert!(can_start(false, std::iter::empty()));
    }

    #[test]
    fn host_leave_ends_match() {
        // Pure transition: the server reducer ends the match; the session reset
        // is the client mirror of that.
        let mut session = MatchSession::new();
        session.state = SessionState::InLobby;
        session.is_host = true;
        session.match_id = Some(3);
        session.on_match_status(MatchStatus::Ended);
        assert_eq!(session.state(), SessionState::Offline);
        assert_eq!(session.match_id(), None);
        assert!(!session.is_host());
    }

    #[test]
    fn spectator_cannot_emit() {
        assert!(!can_emit(MemberRole::Spectator, true));
        assert!(can_emit(MemberRole::Spectator, false));
        assert!(can_emit(MemberRole::Player, true));
    }

    #[test]
    fn disconnected_and_resync_transitions() {
        let mut session = MatchSession::new();
        session.state = SessionState::InGame;
        session.match_id = Some(1);
        session.on_disconnected();
        assert_eq!(session.state(), SessionState::Reconnecting);
        session.on_resync();
        assert_eq!(session.state(), SessionState::Reconnecting);
        session.enter_snapshot_sync();
        assert_eq!(session.state(), SessionState::SnapshotSync);
        assert_eq!(session.resync_count(), 1);
        session.exit_snapshot_sync();
        assert_eq!(session.state(), SessionState::InGame);
    }

    #[test]
    fn on_match_status_moves_between_states() {
        let mut session = MatchSession::new();
        session.state = SessionState::Loading;
        session.match_id = Some(9);
        session.on_match_status(MatchStatus::Lobby);
        assert_eq!(session.state(), SessionState::InLobby);
        session.on_match_status(MatchStatus::Running);
        assert_eq!(session.state(), SessionState::InGame);
    }
}
