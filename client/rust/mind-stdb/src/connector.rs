// SPDX-License-Identifier: GPL-3.0-only

//! [`Connector`] — the Rust replacement for the C# `DatabaseConnector` +
//! `TableSubscriber` lifecycle (plan 01 §3.3/§3.4).
//!
//! The pump contract: [`Connector::pump`] calls `DbConnection::frame_tick`
//! exactly once per frame, then drains the callback queue and emits typed
//! [`ConnectorEvent`]s. SDK callbacks only push into a shared queue, so the
//! same queue contract works for the `run_threaded` fallback (§3.4). Offline
//! mode never builds a `DbConnection` and `pump()` is a no-op.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use spacetimedb_sdk::{DbContext as _, Identity};

use crate::config::{ConnectionConfig, StdbMode};
use crate::identity::LocalIdentity;
use crate::module_bindings::DbConnection;
use crate::token::{FileTokenStore, TokenStore};
use crate::waves::WaveName;

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
            }
        }
    }

    /// Session established: save the token, adopt the identity, emit events.
    fn on_connected(&mut self, identity: Identity, token: &str) {
        self.state = ConnectorState::Connected;
        self.retry_attempts = 0;
        self.retry_at = None;
        self.token_store.save(&self.config.token_key(), token);
        let local = LocalIdentity::new(identity, self.config.token_append.clone());
        self.identity = Some(local.clone());
        // M3 re-subscribes active waves and re-registers binders here, before
        // consumers see `Resync`.
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
}
