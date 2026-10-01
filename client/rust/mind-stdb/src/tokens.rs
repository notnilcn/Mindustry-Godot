// SPDX-License-Identifier: GPL-3.0-only

//! Token persistence, ported from the legacy `sstdbsdk` per-host `AuthToken` behavior.
//!
//! P0 stores one small JSON file per host+suffix under `<data-dir>/identity/` (foundation
//! plan §3.7). Plan `01` may fold this into the schema-versioned `FileTokenStore`
//! (`{"schema":1,"hosts":{...}}`, `01_PLATFORM_STDB_IMPLEMENTATION_PLAN.md` §6.6); keep
//! the public functions stable so callers do not change. Tokens are never logged.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Current on-disk token schema.
pub const TOKEN_SCHEMA: u32 = 1;

/// Directory name under the injected data dir that holds identity/token files.
pub const IDENTITY_DIR: &str = "identity";

/// Token key for a host + optional `--pN` suffix.
///
/// Exact port of the C# `_authTokenKey` pipeline:
/// `host.Replace("://", "_").Replace(":", "_").Replace("/", "_")` followed by the
/// verbatim suffix (`_p1`, ...). Example:
/// `http://127.0.0.1:3000` + `_p1` -> `http___127.0.0.1_3000_p1`.
pub fn token_key(host: &str, token_append: Option<&str>) -> String {
    let mut key = host.replace("://", "_").replace([':', '/'], "_");
    if let Some(append) = token_append {
        key.push_str(append);
    }
    key
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredToken {
    schema: u32,
    token: String,
}

/// File-backed token store rooted at `<data-dir>/identity/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenStore {
    dir: PathBuf,
}

impl TokenStore {
    /// Creates a store rooted at `<data_dir>/identity/`; the directory is created on save.
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            dir: data_dir.as_ref().join(IDENTITY_DIR),
        }
    }

    /// The identity directory this store manages.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path of the token file for `host` + `token_append`.
    pub fn path_for(&self, host: &str, token_append: Option<&str>) -> PathBuf {
        self.dir
            .join(format!("{}.token.json", token_key(host, token_append)))
    }

    /// Loads a stored token; `Ok(None)` when no file exists yet.
    pub fn load(&self, host: &str, token_append: Option<&str>) -> io::Result<Option<String>> {
        match fs::read_to_string(self.path_for(host, token_append)) {
            Ok(text) => match serde_json::from_str::<StoredToken>(&text) {
                Ok(stored) if stored.schema == TOKEN_SCHEMA => Ok(Some(stored.token)),
                Ok(stored) => Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "unsupported token schema {} (expected {TOKEN_SCHEMA})",
                        stored.schema
                    ),
                )),
                Err(err) => Err(io::Error::new(io::ErrorKind::InvalidData, err)),
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Saves (or overwrites) the token file for `host` + `token_append`.
    pub fn save(&self, host: &str, token_append: Option<&str>, token: &str) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let stored = StoredToken {
            schema: TOKEN_SCHEMA,
            token: token.to_string(),
        };
        let text = serde_json::to_string(&stored)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        fs::write(self.path_for(host, token_append), text)
    }

    /// Removes the token file if present (`Ok(())` when it never existed).
    pub fn clear(&self, host: &str, token_append: Option<&str>) -> io::Result<()> {
        match fs::remove_file(self.path_for(host, token_append)) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }
}
