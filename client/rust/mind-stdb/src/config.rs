// SPDX-License-Identifier: GPL-3.0-only

//! Connection configuration (plan 01 §3.3), extending the legacy
//! `DatabaseConnector` exports (`Host`/`DbName`/`TokenAppend`) with an explicit
//! offline mode and a reconnect policy.

use std::path::PathBuf;

/// Local development server used when nothing overrides it.
pub const DEFAULT_HOST: &str = "http://127.0.0.1:3000";

/// Local database name. The crate/module keeps the snake_case name `mindustry_godot`;
/// the local database is `mindustry` and integration tests use `mindustry-it`.
pub const DEFAULT_DB_NAME: &str = "mindustry";

/// Whether the connector is allowed to open a network connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StdbMode {
    /// Single-player / headless default: no `DbConnection` is ever built and
    /// [`crate::connector::Connector::pump`] is a no-op.
    #[default]
    Offline,
    /// Connect on [`crate::connector::Connector::connect`].
    Online,
}

impl StdbMode {
    /// Stable debug/log name (`"offline"`/`"online"`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Online => "online",
        }
    }
}

/// Exponential reconnect backoff: `initial_ms * 2^attempt`, capped at `max_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    /// Delay before the first retry.
    pub initial_ms: u64,
    /// Upper bound for a single retry delay.
    pub max_ms: u64,
}

impl Backoff {
    /// Creates a backoff profile; `max_ms` is clamped to at least `initial_ms`.
    pub const fn new(initial_ms: u64, max_ms: u64) -> Self {
        Self {
            initial_ms,
            max_ms: if max_ms < initial_ms {
                initial_ms
            } else {
                max_ms
            },
        }
    }

    /// Delay before retry `attempt` (0 = first retry), capped at [`Backoff::max_ms`].
    pub fn delay_ms(&self, attempt: u32) -> u64 {
        let factor = 1u64.checked_shl(attempt.min(16)).unwrap_or(u64::MAX);
        self.initial_ms.saturating_mul(factor).min(self.max_ms)
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(500, 30_000)
    }
}

/// Reconnect policy applied after an unexpected disconnect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectPolicy {
    /// Re-`Connect()` after [`ConnectPolicy::backoff`] when the link drops.
    pub auto_reconnect: bool,
    /// Retry delay profile.
    pub backoff: Backoff,
}

impl Default for ConnectPolicy {
    fn default() -> Self {
        Self {
            auto_reconnect: true,
            backoff: Backoff::default(),
        }
    }
}

/// Everything needed to open (or describe) a SpacetimeDB connection.
///
/// `token_append` carries the `--pN` suffix in its on-disk form (e.g. `_p1`),
/// matching the legacy C# `TokenAppend` semantics: the token key is `host`
/// sanitized plus this suffix, so parallel instances get distinct identities
/// per host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionConfig {
    pub host: String,
    pub db_name: String,
    pub token_append: Option<String>,
    /// Injected identity directory (plan §6.6). `None` resolves to the
    /// platform data dir via [`crate::token::default_token_dir`].
    pub token_store_path: Option<PathBuf>,
    pub mode: StdbMode,
    pub policy: ConnectPolicy,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            db_name: DEFAULT_DB_NAME.to_string(),
            token_append: None,
            token_store_path: None,
            mode: StdbMode::Offline,
            policy: ConnectPolicy::default(),
        }
    }
}

impl ConnectionConfig {
    /// The default local-dev configuration: `127.0.0.1:3000`, `mindustry`,
    /// offline (single-player default; callers opt in with
    /// [`StdbMode::Online`]).
    pub fn local() -> Self {
        Self::default()
    }

    /// Token key for this config (see [`crate::token::token_key`]).
    pub fn token_key(&self) -> String {
        crate::token::token_key(&self.host, self.token_append.as_deref())
    }
}
