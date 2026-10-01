// SPDX-License-Identifier: GPL-3.0-only

//! Token persistence, ported from the legacy `sstdbsdk` per-host `AuthToken`
//! behavior. One small schema-versioned JSON file **per host+suffix** under the
//! injected identity directory (`<data-dir>/identity/<key>.token.json`), as
//! reconciled with the plan-00 M5 findings (plan 01 legacy notes / §6.6).
//!
//! Tokens are never logged.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::ConnectionConfig;

/// Current on-disk token schema.
pub const TOKEN_SCHEMA: u32 = 1;

/// Directory name under the injected data dir that holds identity/token files.
pub const IDENTITY_DIR: &str = "identity";

/// Token key for a host + optional `--pN` suffix.
///
/// Exact port of the C# `_authTokenKey` pipeline:
/// `host.Replace("://", "_").Replace(":", "_").Replace("/", "_")` followed by the
/// verbatim suffix (`_p1`, ...). Example:
/// `http://127.0.0.1:3000` + `_p1` -> `http_127.0.0.1_3000_p1` (the `://`
/// replacement is a single underscore).
pub fn token_key(host: &str, token_append: Option<&str>) -> String {
    let mut key = host.replace("://", "_").replace([':', '/'], "_");
    if let Some(append) = token_append {
        key.push_str(append);
    }
    key
}

/// Platform fallback identity directory when no path is injected.
///
/// `$XDG_DATA_HOME/mindustry-godot`, else `$HOME/.local/share/mindustry-godot`,
/// else the system temp dir.
pub fn default_token_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("mindustry-godot");
    }
    if let Ok(home) = std::env::var("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".local/share/mindustry-godot");
    }
    std::env::temp_dir().join("mindustry-godot")
}

/// Injected token persistence (Godot passes `user://`, headless passes a temp
/// or XDG path, tests pass a scratch dir). Failures are logged, never panic.
pub trait TokenStore {
    /// Loads the token for `key`; `None` when absent or unreadable.
    fn load(&self, key: &str) -> Option<String>;

    /// Saves (overwrites) the token for `key`.
    fn save(&self, key: &str, token: &str);
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredToken {
    schema: u32,
    token: String,
}

/// File-backed [`TokenStore`] rooted at `<data-dir>/identity/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTokenStore {
    dir: PathBuf,
}

impl FileTokenStore {
    /// Creates a store rooted at `<data_dir>/identity/`.
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            dir: data_dir.as_ref().join(IDENTITY_DIR),
        }
    }

    /// Resolves the store root from `config.token_store_path`, falling back to
    /// [`default_token_dir`].
    pub fn from_config(config: &ConnectionConfig) -> Self {
        match &config.token_store_path {
            Some(path) => Self::new(path),
            None => Self::new(default_token_dir()),
        }
    }

    /// The identity directory this store manages.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path of the token file for `key`.
    pub fn path_for(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.token.json"))
    }
}

impl TokenStore for FileTokenStore {
    fn load(&self, key: &str) -> Option<String> {
        let text = match fs::read_to_string(self.path_for(key)) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
            Err(err) => {
                log::warn!("token load failed for `{key}`: {err}");
                return None;
            }
        };
        match serde_json::from_str::<StoredToken>(&text) {
            Ok(stored) if stored.schema == TOKEN_SCHEMA => Some(stored.token),
            Ok(stored) => {
                log::warn!(
                    "ignoring token for `{key}` with unsupported schema {}",
                    stored.schema
                );
                None
            }
            Err(err) => {
                log::warn!("ignoring unreadable token for `{key}`: {err}");
                None
            }
        }
    }

    fn save(&self, key: &str, token: &str) {
        if let Err(err) = fs::create_dir_all(&self.dir) {
            log::warn!("token dir create failed: {err}");
            return;
        }
        let stored = StoredToken {
            schema: TOKEN_SCHEMA,
            token: token.to_string(),
        };
        match serde_json::to_string(&stored) {
            Ok(text) => {
                if let Err(err) = fs::write(self.path_for(key), text) {
                    log::warn!("token save failed for `{key}`: {err}");
                }
            }
            Err(err) => log::warn!("token serialize failed: {err}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_scopes_host_and_appends_suffix() {
        assert_eq!(
            token_key("http://127.0.0.1:3000", None),
            "http_127.0.0.1_3000"
        );
        assert_eq!(
            token_key("http://127.0.0.1:3000", Some("_p1")),
            "http_127.0.0.1_3000_p1"
        );
        assert_eq!(
            token_key("https://example.com/game/", Some("_p2")),
            "https_example.com_game__p2"
        );
    }

    fn scratch_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mind-stdb-{name}-{}", std::process::id()))
    }

    #[test]
    fn file_store_roundtrip_is_schema_versioned() {
        let dir = scratch_dir("roundtrip");
        let _ = fs::remove_dir_all(&dir);
        let store = FileTokenStore::new(&dir);
        let key = token_key("http://127.0.0.1:3000", Some("_p1"));

        assert_eq!(store.load(&key), None);
        store.save(&key, "jwt-token");
        assert_eq!(store.load(&key), Some("jwt-token".to_string()));

        // Different suffix -> different file.
        let other = token_key("http://127.0.0.1:3000", Some("_p2"));
        assert_eq!(store.load(&other), None);

        // Schema versions other than 1 are ignored, not misread.
        let written = fs::write(store.path_for(&other), r#"{"schema":99,"token":"old"}"#);
        assert!(written.is_ok());
        assert_eq!(store.load(&other), None);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn from_config_uses_injected_path() {
        let dir = scratch_dir("config");
        let config = ConnectionConfig {
            token_store_path: Some(dir.clone()),
            ..ConnectionConfig::local()
        };
        let store = FileTokenStore::from_config(&config);
        assert_eq!(store.dir(), dir.join(IDENTITY_DIR).as_path());
    }
}
