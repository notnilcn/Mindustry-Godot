// SPDX-License-Identifier: GPL-3.0-only

//! [`Connector`] — the Rust replacement for the C# `DatabaseConnector` +
//! `TableSubscriber` lifecycle (plan 01 §3.3/§3.4/§3.6).
//!
//! The pump contract: [`Connector::pump`] calls `DbConnection::frame_tick`
//! exactly once per frame, then drains the callback queue and emits typed
//! [`ConnectorEvent`]s. SDK callbacks only push into a shared queue, so the
//! same queue contract works for the `run_threaded` fallback (§3.4). Offline
//! mode never builds a `DbConnection` and `pump()` is a no-op.
//!
//! Reconnect is connector-owned: on every successful connect the connector
//! re-issues the desired subscription waves and re-registers every live binder,
//! then emits [`ConnectorEvent::Resync`] so consumers drop stale mirrors.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use spacetimedb_sdk::{
    DbContext as _, Identity, SubscriptionHandle as _, Table as SdkTable, TableAccessor,
    TableWithPrimaryKey,
};

use crate::binder::{self, BinderCore, BinderOptions, TableBinder};
use crate::config::{ConnectionConfig, StdbMode};
use crate::identity::LocalIdentity;
use crate::module_bindings::{
    CommandKind, DbConnection, Gamemode, MemberRole, RemoteTables, SubscriptionHandle, Visibility,
    all_matchesQueryTableAccess, all_playersQueryTableAccess, create_match as _,
    join_match as _, leave_match as _, local_client_settingsQueryTableAccess,
    local_player_profileQueryTableAccess, local_playerQueryTableAccess,
    my_kickQueryTableAccess, my_match_commandsQueryTableAccess, my_match_membersQueryTableAccess,
    my_match_stateQueryTableAccess, my_matchQueryTableAccess, my_matchesQueryTableAccess,
    my_sender_command_stateQueryTableAccess, protocol_infoQueryTableAccess,
    publish_match_state as _, relay_configQueryTableAccess, send_match_command as _,
    server_configQueryTableAccess, set_ready as _, set_username as _, start_match as _,
};
use crate::token::{FileTokenStore, TokenStore};
use crate::waves::{SubscriptionWaves, WaveName};

/// Connection state of a [`Connector`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorState {
    /// No connection desired (single-player default); `pump()` is a no-op.
    Offline,
    /// Online mode, not connected, not trying (initial online state).
    Idle,
    /// A connect attempt is in flight.
    Connecting,
    /// Connected; waves/binders are re-registered on every entry.
    Connected,
    /// Unexpected disconnect; retry scheduled per the backoff policy.
    Retrying,
    /// Terminal disconnect (policy disabled it, or a deliberate `disconnect()`).
    Disconnected,
}

impl ConnectorState {
    /// Whether the state expects (or has) a live connection.
    pub fn is_online(self) -> bool {
        matches!(
            self,
            Self::Idle | Self::Connecting | Self::Connected | Self::Retrying
        )
    }

    /// Stable debug/log name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Idle => "idle",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Retrying => "retrying",
            Self::Disconnected => "disconnected",
        }
    }
}

/// Events drained by consumers after [`Connector::pump`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorEvent {
    /// Connection established; `identity` is this process's server identity.
    Connected {
        /// Server-assigned identity + `--pN` suffix.
        identity: LocalIdentity,
    },
    /// Connection lost (never emitted for a deliberate [`Connector::disconnect`]).
    Disconnected {
        /// Human-readable reason.
        reason: String,
    },
    /// A connect attempt failed before establishing a session.
    ConnectError {
        /// Human-readable reason.
        message: String,
    },
    /// A subscription wave applied.
    WaveApplied(WaveName),
    /// A subscription wave failed.
    WaveError {
        /// Failed wave.
        wave: WaveName,
        /// Human-readable reason.
        message: String,
    },
    /// State may be stale: consumers drop mirrors and replay from the cache.
    Resync,
}

/// Errors from [`Connector`] operations.
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    /// The connector is in [`StdbMode::Offline`].
    #[error("connector is offline (StdbMode::Offline)")]
    Offline,
    /// The initial connection could not be built.
    #[error("SpacetimeDB connect failed: {0}")]
    Connect(String),
}

/// Events pushed by SDK callbacks and drained by `pump()`.
#[derive(Debug)]
enum InternalEvent {
    Connected { identity: Identity, token: String },
    ConnectError { message: String },
    Disconnected { reason: String },
    WaveApplied(WaveName),
    WaveError { wave: WaveName, message: String },
}

/// Shared callback queue (SDK callbacks require `Send + 'static`).
#[derive(Debug, Default)]
struct SharedQueue {
    events: Mutex<VecDeque<InternalEvent>>,
}

impl SharedQueue {
    fn push(&self, event: InternalEvent) {
        let mut guard = match self.events.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.push_back(event);
    }

    fn drain_into(&self, out: &mut Vec<InternalEvent>) {
        let mut guard = match self.events.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        out.extend(guard.drain(..));
    }
}

/// Type-erased binder registration, re-invoked on every reconnect.
struct ErasedBinder {
    table: &'static str,
    register: Box<dyn Fn(&RemoteTables)>,
}

/// A typed callback-registration step for one row type, boxed for erasure.
type TypedRegister<Row> = Box<dyn Fn(&Arc<BinderCore<Row>>, &RemoteTables)>;

/// The one connection facade `mind-gdext`/`mind-headless` talk to.
pub struct Connector {
    config: ConnectionConfig,
    state: ConnectorState,
    queue: Arc<SharedQueue>,
    conn: Option<DbConnection>,
    identity: Option<LocalIdentity>,
    token_store: Box<dyn TokenStore>,
    /// Set by [`Connector::disconnect`] so the resulting SDK event is suppressed.
    intentional_disconnect: bool,
    retry_attempts: u32,
    retry_at: Option<Instant>,
    events: Vec<ConnectorEvent>,
    frames: u64,
    waves: SubscriptionWaves,
    wave_handles: [Option<SubscriptionHandle>; 3],
    binders: Vec<ErasedBinder>,
}

impl Connector {
    /// Creates a connector for `config`; offline mode stays [`ConnectorState::Offline`].
    pub fn new(config: ConnectionConfig) -> Self {
        let token_store = Box::new(FileTokenStore::from_config(&config));
        Self::with_token_store(config, token_store)
    }

    /// Creates a connector with an injected token store (tests, custom paths).
    pub fn with_token_store(config: ConnectionConfig, token_store: Box<dyn TokenStore>) -> Self {
        let state = match config.mode {
            StdbMode::Offline => ConnectorState::Offline,
            StdbMode::Online => ConnectorState::Idle,
        };
        Self {
            config,
            state,
            queue: Arc::new(SharedQueue::default()),
            conn: None,
            identity: None,
            token_store,
            intentional_disconnect: false,
            retry_attempts: 0,
            retry_at: None,
            events: Vec::new(),
            frames: 0,
            waves: SubscriptionWaves::new(),
            wave_handles: [None, None, None],
            binders: Vec::new(),
        }
    }

    /// Creates an offline connector with the default local configuration.
    pub fn local() -> Self {
        Self::new(ConnectionConfig::local())
    }

    /// Configuration this connector was built with.
    pub fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    /// Current connection state.
    pub fn state(&self) -> ConnectorState {
        self.state
    }

    /// Whether the connector is currently connected.
    pub fn is_connected(&self) -> bool {
        self.state == ConnectorState::Connected
    }

    /// This process's identity once connected.
    pub fn local_identity(&self) -> Option<&LocalIdentity> {
        self.identity.as_ref()
    }

    /// Number of [`Connector::pump`] calls processed.
    pub fn frame_count(&self) -> u64 {
        self.frames
    }

    /// Number of live binder registrations.
    pub fn binder_count(&self) -> usize {
        self.binders.len()
    }

    /// Table names of every live binder registration (diagnostics).
    pub fn binder_tables(&self) -> Vec<&'static str> {
        self.binders.iter().map(|binder| binder.table).collect()
    }

    /// Whether `wave` has applied on the current connection.
    pub fn is_applied(&self, wave: WaveName) -> bool {
        self.waves.is_applied(wave)
    }

    /// Opens the connection (offline: a no-op that keeps [`ConnectorState::Offline`]).
    ///
    /// The SDK builds the WebSocket synchronously; on failure the connector
    /// schedules a retry (`auto_reconnect`) and returns [`ConnectorError`].
    pub fn connect(&mut self) -> Result<(), ConnectorError> {
        if self.config.mode == StdbMode::Offline {
            self.state = ConnectorState::Offline;
            return Ok(());
        }
        if self.conn.is_some() {
            return Ok(());
        }
        self.intentional_disconnect = false;
        self.state = ConnectorState::Connecting;
        if let Err(message) = self.open() {
            self.events.push(ConnectorEvent::ConnectError {
                message: message.clone(),
            });
            self.schedule_retry();
            return Err(ConnectorError::Connect(message));
        }
        Ok(())
    }

    /// Re-connects from scratch: drops the current link and opens a new one.
    pub fn reconnect(&mut self) -> Result<(), ConnectorError> {
        self.disconnect();
        self.retry_attempts = 0;
        self.retry_at = None;
        if self.config.mode == StdbMode::Offline {
            return Ok(());
        }
        self.connect()
    }

    /// Closes the connection. Deliberate disconnects never emit
    /// [`ConnectorEvent::Disconnected`] (C# `_shuttingDown` parity).
    pub fn disconnect(&mut self) {
        if self.conn.is_some() {
            self.intentional_disconnect = true;
        }
        if let Some(conn) = self.conn.take() {
            let _ = conn.disconnect();
        }
        self.identity = None;
        self.retry_at = None;
        self.clear_waves();
        self.state = match self.config.mode {
            StdbMode::Offline => ConnectorState::Offline,
            StdbMode::Online => ConnectorState::Disconnected,
        };
    }

    /// Advances the connection without blocking; call exactly once per frame
    /// (plan §3.12 invariant 7). Never panics.
    pub fn pump(&mut self) {
        self.frames = self.frames.saturating_add(1);
        if self.state == ConnectorState::Offline {
            return;
        }
        let tick_error = self.conn.as_ref().and_then(|conn| conn.frame_tick().err());
        if let Some(error) = tick_error {
            self.conn = None;
            self.on_disconnected_internal(error.to_string());
        }
        self.drain_callbacks();
        self.maybe_retry();
    }

    /// Takes every pending [`ConnectorEvent`] in emission order.
    pub fn drain_events(&mut self) -> Vec<ConnectorEvent> {
        std::mem::take(&mut self.events)
    }

    /// Takes the oldest pending [`ConnectorEvent`].
    pub fn poll_event(&mut self) -> Option<ConnectorEvent> {
        if self.events.is_empty() {
            None
        } else {
            Some(self.events.remove(0))
        }
    }

    /// Re-issues the Base wave (it is auto-desired; manual re-issue is a repair).
    pub fn subscribe_base(&mut self) {
        self.subscribe_wave(WaveName::Base);
    }

    /// Ensures the Lobby wave is desired and issued.
    pub fn subscribe_lobby(&mut self) {
        self.subscribe_wave(WaveName::Lobby);
    }

    /// Desires and issues the Game wave (plan §3.6; populated in M4).
    pub fn subscribe_game(&mut self) {
        self.subscribe_wave(WaveName::Game);
    }

    /// Drops the Lobby wave subscription.
    pub fn unsubscribe_lobby(&mut self) {
        self.unsubscribe_wave(WaveName::Lobby);
    }

    /// Drops the Game wave subscription.
    pub fn unsubscribe_game(&mut self) {
        self.unsubscribe_wave(WaveName::Game);
    }

    /// Binds a consumer to one table (insert/delete changes only).
    ///
    /// `table_name` is the generated accessor name and is checked against the
    /// static wave lists (plan §3.12 invariant 5).
    pub fn bind<A>(&mut self, table_name: &'static str) -> TableBinder<A>
    where
        A: TableAccessor<RemoteTables>,
        for<'db> A::Handle<'db>: SdkTable<Row = A::Row>,
        A::Row: Clone + Send,
    {
        let typed = Box::new(|core: &Arc<BinderCore<A::Row>>, db: &RemoteTables| {
            binder::bind_live::<A>(core, db);
        });
        self.bind_erased::<A>(table_name, BinderOptions::default(), typed)
    }

    /// Binds a consumer to one primary-key table: insert/delete/update changes,
    /// plus a client-cache replay when `options.replay_existing` is set.
    pub fn bind_with_replay<A>(
        &mut self,
        table_name: &'static str,
        options: BinderOptions,
    ) -> TableBinder<A>
    where
        A: TableAccessor<RemoteTables>,
        for<'db> A::Handle<'db>: SdkTable<Row = A::Row> + TableWithPrimaryKey,
        A::Row: Clone + Send,
    {
        let replay = options.replay_existing;
        let typed = Box::new(move |core: &Arc<BinderCore<A::Row>>, db: &RemoteTables| {
            binder::bind_live_with_updates::<A>(core, db, replay);
        });
        self.bind_erased::<A>(table_name, options, typed)
    }

    /// Calls the `set_username` reducer (identity smoke path / MCP helper).
    pub fn set_username(&mut self, username: &str) -> Result<(), ConnectorError> {
        self.reducer_conn()?
            .reducers
            .set_username(username.to_string())
            .map_err(send_error)
    }

    /// Calls the `create_match` reducer (creator auto-joins as host/player).
    #[allow(clippy::too_many_arguments)]
    pub fn create_match(
        &mut self,
        map_id: &str,
        map_seed: u64,
        mode: Gamemode,
        mode_name: &str,
        visibility: Visibility,
        password: Option<String>,
        max_players: u16,
        rules_json: &str,
        build_id: &str,
        content_hash: u64,
        mods: Vec<String>,
    ) -> Result<(), ConnectorError> {
        self.reducer_conn()?
            .reducers
            .create_match(
                map_id.to_string(),
                map_seed,
                mode,
                mode_name.to_string(),
                visibility,
                password,
                max_players,
                rules_json.to_string(),
                build_id.to_string(),
                content_hash,
                mods,
            )
            .map_err(send_error)
    }

    /// Calls the `join_match` reducer.
    pub fn join_match(
        &mut self,
        match_id: u64,
        password: Option<String>,
        build_id: &str,
        content_hash: u64,
        role: MemberRole,
        mods: Vec<String>,
    ) -> Result<(), ConnectorError> {
        self.reducer_conn()?
            .reducers
            .join_match(
                match_id,
                password,
                build_id.to_string(),
                content_hash,
                role,
                mods,
            )
            .map_err(send_error)
    }

    /// Calls the `leave_match` reducer.
    pub fn leave_match(&mut self, match_id: u64) -> Result<(), ConnectorError> {
        self.reducer_conn()?
            .reducers
            .leave_match(match_id)
            .map_err(send_error)
    }

    /// Calls the `set_ready` reducer (lobby readiness gate).
    pub fn set_ready(&mut self, match_id: u64, ready: bool) -> Result<(), ConnectorError> {
        self.reducer_conn()?
            .reducers
            .set_ready(match_id, ready)
            .map_err(send_error)
    }

    /// Calls the `start_match` reducer (lobby → running). `force` skips the
    /// all-ready gate (host only).
    pub fn start_match(&mut self, match_id: u64, force: bool) -> Result<(), ConnectorError> {
        self.reducer_conn()?
            .reducers
            .start_match(match_id, force)
            .map_err(send_error)
    }

    /// Calls the `publish_match_state` reducer (host only).
    #[allow(clippy::too_many_arguments)]
    pub fn publish_match_state(
        &mut self,
        match_id: u64,
        wave: i32,
        wavetime: f32,
        enemies: i32,
        paused: bool,
        game_over: bool,
        sim_tick: u64,
        last_command_id: u64,
    ) -> Result<(), ConnectorError> {
        self.reducer_conn()?
            .reducers
            .publish_match_state(
                match_id,
                wave,
                wavetime,
                enemies,
                paused,
                game_over,
                sim_tick,
                last_command_id,
            )
            .map_err(send_error)
    }

    /// Calls the `send_match_command` reducer (validated + relayed by the server).
    pub fn send_match_command(
        &mut self,
        match_id: u64,
        client_tick: u64,
        kind: CommandKind,
    ) -> Result<(), ConnectorError> {
        self.reducer_conn()?
            .reducers
            .send_match_command(match_id, client_tick, kind)
            .map_err(send_error)
    }

    /// Sends a `Ping { nonce }` command (round-trip probe).
    pub fn send_ping(
        &mut self,
        match_id: u64,
        client_tick: u64,
        nonce: u64,
    ) -> Result<(), ConnectorError> {
        self.send_match_command(match_id, client_tick, CommandKind::Ping(nonce))
    }

    fn reducer_conn(&self) -> Result<&DbConnection, ConnectorError> {
        self.conn.as_ref().ok_or(ConnectorError::Offline)
    }

    /// Test hook: simulate the SDK `on_connect` callback without a network.
    #[cfg(test)]
    pub(crate) fn inject_connected_for_test(&self, identity: Identity, token: &str) {
        self.queue.push(InternalEvent::Connected {
            identity,
            token: token.to_string(),
        });
    }

    /// Test hook: simulate the SDK `on_disconnect` callback without a network.
    #[cfg(test)]
    pub(crate) fn inject_disconnected_for_test(&self, reason: &str) {
        self.queue.push(InternalEvent::Disconnected {
            reason: reason.to_string(),
        });
    }

    /// Test hook: simulate a `WaveApplied` subscription callback.
    #[cfg(test)]
    pub(crate) fn inject_wave_applied_for_test(&self, wave: WaveName) {
        self.queue.push(InternalEvent::WaveApplied(wave));
    }

    /// Builds the SDK connection and registers the lifecycle callbacks.
    fn open(&mut self) -> Result<(), String> {
        let token = self.token_store.load(&self.config.token_key());
        let queue = self.queue.clone();
        let builder = DbConnection::builder()
            .with_uri(self.config.host.as_str())
            .with_database_name(self.config.db_name.as_str())
            .with_token(token.as_deref())
            .on_connect({
                let queue = queue.clone();
                move |_conn, identity, token| {
                    queue.push(InternalEvent::Connected {
                        identity,
                        token: token.to_string(),
                    });
                }
            })
            .on_connect_error({
                let queue = queue.clone();
                move |_ctx, error| {
                    queue.push(InternalEvent::ConnectError {
                        message: error.to_string(),
                    });
                }
            })
            .on_disconnect(move |_ctx, error| {
                let reason = match error {
                    Some(error) => error.to_string(),
                    None => "connection closed".to_string(),
                };
                queue.push(InternalEvent::Disconnected { reason });
            });
        match builder.build() {
            Ok(conn) => {
                self.conn = Some(conn);
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    /// Processes every callback queued during the last `frame_tick`.
    fn drain_callbacks(&mut self) {
        let mut internal = Vec::new();
        self.queue.drain_into(&mut internal);
        for event in internal {
            match event {
                InternalEvent::Connected { identity, token } => {
                    self.on_connected(identity, &token);
                }
                InternalEvent::ConnectError { message } => {
                    self.events.push(ConnectorEvent::ConnectError {
                        message: message.clone(),
                    });
                    self.conn = None;
                    self.schedule_retry();
                }
                InternalEvent::Disconnected { reason } => {
                    self.on_disconnected_internal(reason);
                }
                InternalEvent::WaveApplied(wave) => {
                    self.waves.mark_applied(wave);
                    self.events.push(ConnectorEvent::WaveApplied(wave));
                }
                InternalEvent::WaveError { wave, message } => {
                    self.waves.mark_dropped(wave);
                    self.events
                        .push(ConnectorEvent::WaveError { wave, message });
                }
            }
        }
    }

    /// Session established: save the token, adopt the identity, re-issue every
    /// desired wave and re-register every live binder, then emit events.
    fn on_connected(&mut self, identity: Identity, token: &str) {
        self.state = ConnectorState::Connected;
        self.retry_attempts = 0;
        self.retry_at = None;
        self.token_store.save(&self.config.token_key(), token);
        let local = LocalIdentity::new(identity, self.config.token_append.clone());
        self.identity = Some(local.clone());
        self.reissue_waves();
        self.rebind_all();
        self.events
            .push(ConnectorEvent::Connected { identity: local });
        self.events.push(ConnectorEvent::Resync);
    }

    /// Link lost: update state, schedule a retry, emit at most one event.
    fn on_disconnected_internal(&mut self, reason: String) {
        if self.state == ConnectorState::Offline {
            return;
        }
        self.conn = None;
        self.identity = None;
        self.clear_waves();
        if self.intentional_disconnect {
            self.intentional_disconnect = false;
            self.state = ConnectorState::Disconnected;
            return;
        }
        let already_ended = matches!(
            self.state,
            ConnectorState::Disconnected | ConnectorState::Retrying
        );
        if self.config.policy.auto_reconnect {
            self.state = ConnectorState::Retrying;
            self.arm_retry();
        } else {
            self.state = ConnectorState::Disconnected;
        }
        if !already_ended {
            self.events.push(ConnectorEvent::Disconnected { reason });
        }
    }

    /// Arms the next retry timestamp using the current attempt count.
    fn arm_retry(&mut self) {
        let delay = self.config.policy.backoff.delay_ms(self.retry_attempts);
        self.retry_attempts = self.retry_attempts.saturating_add(1);
        self.retry_at = Some(Instant::now() + Duration::from_millis(delay));
    }

    /// Moves to `Retrying` (or `Disconnected`) after a failed connect attempt.
    fn schedule_retry(&mut self) {
        self.conn = None;
        if self.config.mode == StdbMode::Offline {
            self.state = ConnectorState::Offline;
            return;
        }
        if self.config.policy.auto_reconnect {
            self.state = ConnectorState::Retrying;
            self.arm_retry();
        } else {
            self.state = ConnectorState::Disconnected;
        }
    }

    /// Re-opens the connection when the retry deadline elapsed.
    fn maybe_retry(&mut self) {
        if self.state != ConnectorState::Retrying {
            return;
        }
        let Some(deadline) = self.retry_at else {
            return;
        };
        if Instant::now() < deadline {
            return;
        }
        self.retry_at = None;
        self.state = ConnectorState::Connecting;
        if let Err(message) = self.open() {
            self.events.push(ConnectorEvent::ConnectError { message });
            self.schedule_retry();
        }
    }

    /// Sets the desired flag and, when connected, issues the wave.
    fn subscribe_wave(&mut self, wave: WaveName) {
        self.waves.set_desired(wave, true);
        if self.conn.is_none() || self.waves.is_applied(wave) {
            return;
        }
        let index = wave_index(wave);
        if self.wave_handles[index].is_some() {
            return;
        }
        if let Some(handle) = self.issue_wave(wave) {
            self.wave_handles[index] = Some(handle);
        }
    }

    /// Clears the desired flag and drops the handle (if any).
    fn unsubscribe_wave(&mut self, wave: WaveName) {
        self.waves.set_desired(wave, false);
        let index = wave_index(wave);
        if let Some(handle) = self.wave_handles[index].take()
            && let Err(error) = handle.unsubscribe()
        {
            log::warn!("unsubscribe {} failed: {error}", wave.name());
        }
        self.waves.mark_dropped(wave);
    }

    /// Issues every desired, non-empty wave after a connect.
    fn reissue_waves(&mut self) {
        self.wave_handles = [None, None, None];
        for wave in WaveName::ALL {
            if !self.waves.is_desired(wave) || wave.tables().is_empty() {
                continue;
            }
            if let Some(handle) = self.issue_wave(wave) {
                self.wave_handles[wave_index(wave)] = Some(handle);
            }
        }
    }

    /// Builds and sends one subscription with its typed queries (plan §3.6).
    fn issue_wave(&mut self, wave: WaveName) -> Option<SubscriptionHandle> {
        let conn = self.conn.as_ref()?;
        if wave.tables().is_empty() {
            log::warn!("{} wave has no tables yet; not subscribing", wave.name());
            return None;
        }
        let queue = self.queue.clone();
        let builder = conn.subscription_builder();
        let handle = match wave {
            // Base has no applied callback by design: late binders replay the cache.
            WaveName::Base => builder
                .add_query(|q| q.from.protocol_info())
                .add_query(|q| q.from.relay_config())
                .add_query(|q| q.from.local_client_settings())
                .add_query(|q| q.from.server_config())
                .subscribe(),
            WaveName::Lobby => {
                let applied_queue = queue.clone();
                let error_queue = queue.clone();
                builder
                    .on_applied(move |_ctx| {
                        applied_queue.push(InternalEvent::WaveApplied(WaveName::Lobby));
                    })
                    .on_error(move |_ctx, error| {
                        error_queue.push(InternalEvent::WaveError {
                            wave: WaveName::Lobby,
                            message: error.to_string(),
                        });
                    })
                    .add_query(|q| q.from.local_player())
                    .add_query(|q| q.from.local_player_profile())
                    .add_query(|q| q.from.all_players())
                    .add_query(|q| q.from.my_matches())
                    .add_query(|q| q.from.all_matches())
                    .subscribe()
            }
            WaveName::Game => {
                let applied_queue = queue.clone();
                let error_queue = queue.clone();
                builder
                    .on_applied(move |_ctx| {
                        applied_queue.push(InternalEvent::WaveApplied(WaveName::Game));
                    })
                    .on_error(move |_ctx, error| {
                        error_queue.push(InternalEvent::WaveError {
                            wave: WaveName::Game,
                            message: error.to_string(),
                        });
                    })
                    .add_query(|q| q.from.my_match())
                    .add_query(|q| q.from.my_match_commands())
                    .add_query(|q| q.from.my_match_members())
                    .add_query(|q| q.from.my_match_state())
                    .add_query(|q| q.from.my_kick())
                    .add_query(|q| q.from.my_sender_command_state())
                    .subscribe()
            }
        };
        log::debug!("subscribed {} wave", wave.name());
        Some(handle)
    }

    /// Re-registers every live binder against the current connection.
    fn rebind_all(&mut self) {
        if let Some(conn) = &self.conn {
            for erased in &self.binders {
                (erased.register)(&conn.db);
            }
        }
    }

    /// Drops wave handles and applied flags (connection gone).
    fn clear_waves(&mut self) {
        self.wave_handles = [None, None, None];
        self.waves.clear();
    }

    fn bind_erased<A>(
        &mut self,
        table_name: &'static str,
        options: BinderOptions,
        typed_register: TypedRegister<A::Row>,
    ) -> TableBinder<A>
    where
        A: TableAccessor<RemoteTables>,
        A::Row: Clone + Send,
    {
        if WaveName::for_table(table_name).is_none() {
            log::warn!("binder for `{table_name}` is not listed in any subscription wave");
        }
        let core = Arc::new(BinderCore::new(table_name, options));
        let weak = Arc::downgrade(&core);
        let register = Box::new(move |db: &RemoteTables| {
            if let Some(core) = weak.upgrade() {
                typed_register(&core, db);
            }
        });
        self.binders.push(ErasedBinder {
            table: table_name,
            register,
        });
        if let Some(conn) = &self.conn
            && let Some(erased) = self.binders.last()
        {
            (erased.register)(&conn.db);
        }
        TableBinder::new(core)
    }
}

/// Maps the SDK's send failure into a connector error.
fn send_error(error: spacetimedb_sdk::Error) -> ConnectorError {
    ConnectorError::Connect(error.to_string())
}

/// Wave state helper kept on the connector: index into `wave_handles`.
fn wave_index(wave: WaveName) -> usize {
    match wave {
        WaveName::Base => 0,
        WaveName::Lobby => 1,
        WaveName::Game => 2,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mind-stdb-conn-{name}-{}", std::process::id()))
    }

    fn offline_config(name: &str) -> ConnectionConfig {
        ConnectionConfig {
            token_store_path: Some(scratch_dir(name)),
            ..ConnectionConfig::local()
        }
    }

    fn online_config(name: &str) -> ConnectionConfig {
        ConnectionConfig {
            mode: StdbMode::Online,
            token_store_path: Some(scratch_dir(name)),
            ..ConnectionConfig::local()
        }
    }

    #[test]
    fn offline_never_builds_and_reports_offline() {
        let config = offline_config("offline");
        let mut connector = Connector::new(config);
        assert_eq!(connector.state(), ConnectorState::Offline);
        assert!(connector.connect().is_ok());
        assert_eq!(connector.state(), ConnectorState::Offline);
        assert!(!connector.is_connected());
        assert!(connector.local_identity().is_none());
        for _ in 0..16 {
            connector.pump();
        }
        assert_eq!(connector.state(), ConnectorState::Offline);
        assert_eq!(connector.frame_count(), 16);
        assert!(connector.drain_events().is_empty());
    }

    #[test]
    fn pump_without_connection_is_noop() {
        let config = online_config("noop");
        let mut connector = Connector::new(config);
        assert_eq!(connector.state(), ConnectorState::Idle);
        connector.pump();
        connector.pump();
        assert_eq!(connector.state(), ConnectorState::Idle);
        assert!(connector.drain_events().is_empty());
        assert!(!connector.is_connected());
    }

    #[test]
    fn disconnect_then_reconnect_rebinds() {
        let config = online_config("reconnect");
        let mut connector = Connector::new(config);

        connector.inject_connected_for_test(Identity::from_byte_array([1u8; 32]), "token-1");
        connector.pump();
        let events = connector.drain_events();
        assert!(matches!(
            events.first(),
            Some(ConnectorEvent::Connected { .. })
        ));
        assert_eq!(events.last(), Some(&ConnectorEvent::Resync));
        assert!(connector.is_connected());

        connector.inject_disconnected_for_test("server closed");
        connector.pump();
        let events = connector.drain_events();
        assert!(matches!(
            events.first(),
            Some(ConnectorEvent::Disconnected { .. })
        ));
        assert_eq!(connector.state(), ConnectorState::Retrying);
        assert!(!connector.is_connected());

        connector.inject_connected_for_test(Identity::from_byte_array([2u8; 32]), "token-2");
        connector.pump();
        let events = connector.drain_events();
        assert!(matches!(
            events.first(),
            Some(ConnectorEvent::Connected { .. })
        ));
        assert_eq!(events.last(), Some(&ConnectorEvent::Resync));
        assert_eq!(connector.state(), ConnectorState::Connected);
        let identity = connector.local_identity();
        assert_eq!(
            identity.map(LocalIdentity::hex),
            Some(Identity::from_byte_array([2u8; 32]).to_hex().to_string())
        );
    }

    #[test]
    fn deliberate_disconnect_emits_no_event() {
        let config = online_config("deliberate");
        let mut connector = Connector::new(config);
        connector.inject_connected_for_test(Identity::from_byte_array([3u8; 32]), "token");
        connector.pump();
        let _ = connector.drain_events();

        connector.disconnect();
        assert_eq!(connector.state(), ConnectorState::Disconnected);
        assert!(connector.drain_events().is_empty());

        // A stray SDK disconnect queued around the deliberate close stays silent.
        connector.inject_disconnected_for_test("socket closed");
        connector.pump();
        assert!(connector.drain_events().is_empty());
    }

    #[test]
    fn no_auto_reconnect_terminal_disconnect() {
        let mut config = online_config("terminal");
        config.policy.auto_reconnect = false;
        let mut connector = Connector::new(config);
        connector.inject_connected_for_test(Identity::from_byte_array([4u8; 32]), "token");
        connector.pump();
        let _ = connector.drain_events();

        connector.inject_disconnected_for_test("boom");
        connector.pump();
        assert_eq!(connector.state(), ConnectorState::Disconnected);
        let events = connector.drain_events();
        assert!(matches!(
            events.first(),
            Some(ConnectorEvent::Disconnected { .. })
        ));
    }

    #[test]
    fn wave_events_update_wave_state() {
        let config = online_config("waves");
        let mut connector = Connector::new(config);
        assert!(!connector.is_applied(WaveName::Lobby));

        connector.inject_connected_for_test(Identity::from_byte_array([5u8; 32]), "token");
        connector.inject_wave_applied_for_test(WaveName::Lobby);
        connector.pump();
        let events = connector.drain_events();
        assert!(connector.is_applied(WaveName::Lobby));
        assert!(events.contains(&ConnectorEvent::WaveApplied(WaveName::Lobby)));

        connector.inject_disconnected_for_test("drop");
        connector.pump();
        assert!(!connector.is_applied(WaveName::Lobby));
    }
}
