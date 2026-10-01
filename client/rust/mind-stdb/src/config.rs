// SPDX-License-Identifier: GPL-3.0-only

//! Connection configuration, mirroring the legacy `DatabaseConnector` exports
//! (`Host`/`DbName`/`TokenAppend`) without the Godot `[Export]` surface.

/// Local development server used when nothing overrides it.
pub const DEFAULT_HOST: &str = "http://127.0.0.1:3000";

/// Local database name. The crate/module keeps the snake_case name `mindustry_godot`;
/// the local database is `mindustry` and integration tests use `mindustry-it`
/// (final naming recorded in plan 01's legacy-notes section and
/// `23_PARITY_VERIFICATION_IMPLEMENTATION_PLAN.md` §8.1.1).
pub const DEFAULT_DB_NAME: &str = "mindustry";

/// Everything needed to open (or describe) a SpacetimeDB connection.
///
/// `token_append` carries the `--pN` suffix in its on-disk form (e.g. `_p1`), matching
/// the legacy C# `TokenAppend` semantics: the token key is `host` sanitized plus this
/// suffix, so parallel instances get distinct identities per host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionConfig {
    pub host: String,
    pub db_name: String,
    pub token_append: Option<String>,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            db_name: DEFAULT_DB_NAME.to_string(),
            token_append: None,
        }
    }
}

impl ConnectionConfig {
    /// The default local-dev configuration (`127.0.0.1:3000`, `mindustry`, no suffix).
    pub fn local() -> Self {
        Self::default()
    }

    /// Token key for this config (see [`crate::tokens::token_key`]).
    pub fn token_key(&self) -> String {
        crate::tokens::token_key(&self.host, self.token_append.as_deref())
    }
}
