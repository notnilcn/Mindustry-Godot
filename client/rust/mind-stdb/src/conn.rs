// SPDX-License-Identifier: GPL-3.0-only

//! The `MindDb` facade: connection state, frame pump and reducer calls.
//!
//! P0 is deliberately inert: [`MindDb::new`] is offline and never touches the network,
//! [`MindDb::frame_tick`] is a cheap pump that is safe to call once per Godot frame, and
//! every live/network operation returns [`MindDbError::NotImplemented`]. Plan `01`
//! replaces the innards (worker thread + channels, OD-R4) without changing this boundary.

use std::any::Any;

use spacetimedb_sdk::DbContext as _;

use crate::binder::{BinderHandle, TableBinder};
use crate::config::ConnectionConfig;
use crate::module_bindings::DbConnection;
use crate::waves::SubscriptionWave;

/// Connection state of [`MindDb`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MindDbState {
    /// No connection desired (single-player default, and the P0 state).
    Offline,
    /// A connect attempt is in flight.
    Connecting,
    /// Connected and subscribed to the base wave.
    Connected,
    /// Connection dropped; reconnect policy is plan `01`'s.
    Disconnected,
}

impl MindDbState {
    /// Whether the state expects a live connection (`Connecting` or `Connected`).
    pub fn is_online(self) -> bool {
        matches!(self, Self::Connecting | Self::Connected)
    }

    /// Stable debug/log name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Disconnected => "disconnected",
        }
    }
}

/// Errors from [`MindDb`] operations.
#[derive(Debug, thiserror::Error)]
pub enum MindDbError {
    /// Live SpacetimeDB behavior is deferred to plan `01`; the P0 facade only exists to
    /// keep the gdext wiring boundary stable.
    #[error("mind-stdb live operations are not implemented at P0 (plan 01 owns them)")]
    NotImplemented,
    /// Transport/frame-tick failure.
    #[error("SpacetimeDB connection error: {0}")]
    Connection(String),
    /// Token-store/filesystem failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The one connection facade `mind-gdext`/`mind-headless` talk to.
///
/// The generated [`DbConnection`] is held opaquely so plan `01` can start the real pump
/// without changing this boundary.
pub struct MindDb {
    config: ConnectionConfig,
    state: MindDbState,
    conn: Option<DbConnection>,
    binders: Vec<Box<dyn TableBinder>>,
    frame_count: u64,
}

impl MindDb {
    /// Creates an offline facade for `config`.
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            state: MindDbState::Offline,
            conn: None,
            binders: Vec::new(),
            frame_count: 0,
        }
    }

    /// Creates an offline facade with the default local configuration.
    pub fn local() -> Self {
        Self::new(ConnectionConfig::local())
    }

    /// Configuration this facade was built with.
    pub fn config(&self) -> &ConnectionConfig {
        &self.config
    }

    /// Current connection state.
    pub fn state(&self) -> MindDbState {
        self.state
    }

    /// Whether a live connection is established (never at P0).
    pub fn is_connected(&self) -> bool {
        self.state == MindDbState::Connected
    }

    /// Number of [`MindDb::frame_tick`] calls processed.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Number of registered table binders.
    pub fn binder_count(&self) -> usize {
        self.binders.len()
    }

    /// Opens the connection and applies the base wave.
    ///
    /// P0: always [`MindDbError::NotImplemented`]; state stays [`MindDbState::Offline`].
    pub fn connect(&mut self) -> Result<(), MindDbError> {
        Err(MindDbError::NotImplemented)
    }

    /// Closes the connection (offline-safe; also used on `_exit_tree`).
    pub fn disconnect(&mut self) {
        if let Some(conn) = self.conn.take() {
            let _ = conn.disconnect();
        }
        self.state = MindDbState::Offline;
    }

    /// Advances the connection without blocking; call exactly once per frame.
    ///
    /// Offline (P0 default) this only bumps [`MindDb::frame_count`].
    pub fn frame_tick(&mut self) -> Result<(), MindDbError> {
        self.frame_count = self.frame_count.saturating_add(1);
        if let Some(conn) = self.conn.as_ref() {
            conn.frame_tick()
                .map_err(|err| MindDbError::Connection(err.to_string()))?;
        }
        Ok(())
    }

    /// Subscribes (or re-issues) a wave. P0: [`MindDbError::NotImplemented`].
    pub fn subscribe(&mut self, _wave: SubscriptionWave) -> Result<(), MindDbError> {
        Err(MindDbError::NotImplemented)
    }

    /// Unsubscribes a wave. P0: [`MindDbError::NotImplemented`].
    pub fn unsubscribe(&mut self, _wave: SubscriptionWave) -> Result<(), MindDbError> {
        Err(MindDbError::NotImplemented)
    }

    /// Remote `set_username` reducer call. P0: [`MindDbError::NotImplemented`].
    pub fn set_username(&mut self, _username: &str) -> Result<(), MindDbError> {
        Err(MindDbError::NotImplemented)
    }

    /// Binds a consumer to one table; the handle identifies it for diagnostics.
    pub fn register_binder(&mut self, binder: impl TableBinder + 'static) -> BinderHandle {
        let table = binder.table_name();
        let index = self.binders.len();
        self.binders.push(Box::new(binder));
        BinderHandle::new(index, table)
    }

    /// Delivers an insert to every binder on `table` (used by the future SDK callbacks
    /// and by the P0 binder tests).
    pub fn dispatch_insert(&mut self, table: &str, row: &dyn Any) {
        for binder in self.binders.iter_mut() {
            if binder.table_name() == table {
                binder.on_insert(row);
            }
        }
    }

    /// Delivers an update to every binder on `table`.
    pub fn dispatch_update(&mut self, table: &str, old: &dyn Any, new: &dyn Any) {
        for binder in self.binders.iter_mut() {
            if binder.table_name() == table {
                binder.on_update(old, new);
            }
        }
    }

    /// Delivers a delete to every binder on `table`.
    pub fn dispatch_delete(&mut self, table: &str, row: &dyn Any) {
        for binder in self.binders.iter_mut() {
            if binder.table_name() == table {
                binder.on_delete(row);
            }
        }
    }
}
