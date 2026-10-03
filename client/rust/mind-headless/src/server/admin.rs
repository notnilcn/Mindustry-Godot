// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server admin/ban/whitelist JSON mirror (plan 22 §3.7/§6.3, P22-6).
//!
//! Port of the `settings.bin` admin/ban/whitelist persistence split: under D2
//! the **authority** is plan 21's STDB tables (`admin_identity`/`player_ban`/
//! `whitelist_entry`); this module owns the `config/{admins,bans,whitelist}.json`
//! **offline-mode mirror** and the read-only fallback the console scans when no
//! live database is available. Records are identity-keyed (plan 21 deviation 3:
//! no IP/subnet bans exist under D2, so `subnet`/`dos` are recorded only as
//! local name entries and reported as unsupported by the network host).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Offline admins filename (`<config>/admins.json`).
pub const ADMINS_FILE: &str = "admins.json";
/// Offline bans filename (`<config>/bans.json`).
pub const BANS_FILE: &str = "bans.json";
/// Offline whitelist filename (`<config>/whitelist.json`).
pub const WHITELIST_FILE: &str = "whitelist.json";

/// One administrator record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminEntry {
    /// Identity hex (64 lowercase hex chars).
    pub identity: String,
    /// Last-known username (`""` when unknown).
    #[serde(default)]
    pub name: String,
}

/// Ban flavor (`ServerControl` `ban [type-id/name/ip]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BanKind {
    /// Identity ban (the only kind plan 21 can enforce).
    Identity,
    /// Username/regex ban (offline mirror only under D2).
    Name,
    /// Subnet ban (unsupported under D2: no peer IPs reach reducers).
    Subnet,
    /// DoS blacklist (unsupported under D2).
    Dos,
}

impl BanKind {
    /// Parses the `ban` subcommand token (`type-id` → [`BanKind::Identity`]).
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "id" | "type-id" | "identity" => Some(Self::Identity),
            "name" => Some(Self::Name),
            "ip" | "subnet" => Some(Self::Subnet),
            "dos" => Some(Self::Dos),
            _ => None,
        }
    }

    /// Stable lowercase name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Identity => "id",
            Self::Name => "name",
            Self::Subnet => "subnet",
            Self::Dos => "dos",
        }
    }
}

/// One ban record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BanEntry {
    /// Ban flavor.
    pub kind: BanKind,
    /// Banned identity hex (identity bans) or the matched string/regex.
    pub target: String,
    /// Last-known username, when available.
    #[serde(default)]
    pub name: String,
    /// Human-readable reason.
    #[serde(default)]
    pub reason: String,
}

impl BanEntry {
    /// A ban with an empty reason.
    pub fn new(kind: BanKind, target: impl Into<String>) -> Self {
        Self {
            kind,
            target: target.into(),
            name: String::new(),
            reason: String::new(),
        }
    }
}

/// One whitelist record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhitelistEntry {
    /// Identity hex.
    pub identity: String,
    /// Last-known username.
    #[serde(default)]
    pub name: String,
}

/// The three-file admin mirror rooted at `<config>/`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminMirror {
    /// Administrator identities.
    pub admins: Vec<AdminEntry>,
    /// Ban records.
    pub bans: Vec<BanEntry>,
    /// Whitelisted identities.
    pub whitelist: Vec<WhitelistEntry>,
}

impl AdminMirror {
    /// Loads the three files under `root`; a missing file is an empty list and
    /// a malformed file is logged then treated as empty (never fatal).
    pub fn load(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self {
            admins: read_json(&root.join(ADMINS_FILE)),
            bans: read_json(&root.join(BANS_FILE)),
            whitelist: read_json(&root.join(WHITELIST_FILE)),
        }
    }

    /// Writes the three files under `root` (pretty JSON, one trailing newline).
    pub fn save(&self, root: impl AsRef<Path>) -> Result<(), std::io::Error> {
        let root = root.as_ref();
        write_json(&root.join(ADMINS_FILE), &self.admins)?;
        write_json(&root.join(BANS_FILE), &self.bans)?;
        write_json(&root.join(WHITELIST_FILE), &self.whitelist)?;
        Ok(())
    }

    /// Whether `identity` is an administrator.
    pub fn is_admin(&self, identity: &str) -> bool {
        self.admins.iter().any(|entry| entry.identity == identity)
    }

    /// Adds `identity` as an administrator (idempotent; `name` fills a blank).
    pub fn add_admin(&mut self, identity: &str, name: &str) {
        match self
            .admins
            .iter_mut()
            .find(|entry| entry.identity == identity)
        {
            Some(entry) => {
                if entry.name.is_empty() {
                    entry.name = name.to_owned();
                }
            }
            None => self.admins.push(AdminEntry {
                identity: identity.to_owned(),
                name: name.to_owned(),
            }),
        }
    }

    /// Removes `identity` from the administrator list.
    pub fn remove_admin(&mut self, identity: &str) {
        self.admins.retain(|entry| entry.identity != identity);
    }

    /// Adds a ban (replacing a same-kind/same-target record).
    pub fn add_ban(&mut self, ban: BanEntry) {
        self.bans
            .retain(|entry| !(entry.kind == ban.kind && entry.target == ban.target));
        self.bans.push(ban);
    }

    /// Removes every ban whose target equals `target` (identity hex or string).
    pub fn remove_ban(&mut self, target: &str) {
        self.bans
            .retain(|entry| entry.target != target && entry.name != target);
    }

    /// Whether `identity` (or a name-ban pattern substring) matches a ban.
    pub fn is_banned(&self, identity: &str, name: &str) -> bool {
        self.bans.iter().any(|entry| match entry.kind {
            BanKind::Identity => entry.target == identity,
            BanKind::Name => !entry.target.is_empty() && name.contains(&entry.target),
            BanKind::Subnet | BanKind::Dos => false,
        })
    }

    /// Adds `identity` to the whitelist (idempotent).
    pub fn add_whitelist(&mut self, identity: &str, name: &str) {
        if !self
            .whitelist
            .iter()
            .any(|entry| entry.identity == identity)
        {
            self.whitelist.push(WhitelistEntry {
                identity: identity.to_owned(),
                name: name.to_owned(),
            });
        }
    }

    /// Removes `identity` from the whitelist.
    pub fn remove_whitelist(&mut self, identity: &str) {
        self.whitelist.retain(|entry| entry.identity != identity);
    }

    /// Whether `identity` is whitelisted.
    pub fn is_whitelisted(&self, identity: &str) -> bool {
        self.whitelist
            .iter()
            .any(|entry| entry.identity == identity)
    }

    /// The last-known name for `identity`, if recorded.
    pub fn name_for(&self, identity: &str) -> Option<&str> {
        self.admins
            .iter()
            .find(|entry| entry.identity == identity)
            .map(|entry| entry.name.as_str())
            .or_else(|| {
                self.whitelist
                    .iter()
                    .find(|entry| entry.identity == identity)
                    .map(|entry| entry.name.as_str())
            })
            .or_else(|| {
                self.bans
                    .iter()
                    .find(|entry| entry.target == identity)
                    .map(|entry| entry.name.as_str())
            })
    }

    /// Offline `search`/`info`: records whose identity/name contains `query`
    /// (case-insensitive). Empty query returns everything.
    pub fn search(&self, query: &str) -> Vec<AdminMatch> {
        let needle = query.to_ascii_lowercase();
        let mut out = Vec::new();
        for entry in &self.admins {
            if matches(&entry.identity, &entry.name, &needle) {
                out.push(AdminMatch {
                    identity: entry.identity.clone(),
                    name: entry.name.clone(),
                    role: "admin".to_owned(),
                });
            }
        }
        for entry in &self.whitelist {
            if matches(&entry.identity, &entry.name, &needle) {
                out.push(AdminMatch {
                    identity: entry.identity.clone(),
                    name: entry.name.clone(),
                    role: "whitelisted".to_owned(),
                });
            }
        }
        for entry in &self.bans {
            if matches(&entry.target, &entry.name, &needle) {
                out.push(AdminMatch {
                    identity: entry.target.clone(),
                    name: entry.name.clone(),
                    role: format!("banned:{}", entry.kind.name()),
                });
            }
        }
        out
    }
}

/// One offline `search`/`info` result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminMatch {
    /// Identity hex (or the ban target string).
    pub identity: String,
    /// Last-known username.
    pub name: String,
    /// `admin` / `whitelisted` / `banned:<kind>`.
    pub role: String,
}

fn matches(identity: &str, name: &str, needle: &str) -> bool {
    needle.is_empty()
        || identity.to_ascii_lowercase().contains(needle)
        || name.to_ascii_lowercase().contains(needle)
}

fn read_json<T>(path: &PathBuf) -> T
where
    T: serde::de::DeserializeOwned + Default,
{
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(value) => value,
            Err(error) => {
                log::warn!(
                    "admin mirror: ignoring malformed `{}`: {error}",
                    path.display()
                );
                T::default()
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => T::default(),
        Err(error) => {
            log::warn!("admin mirror: cannot read `{}`: {error}", path.display());
            T::default()
        }
    }
}

fn write_json<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), std::io::Error> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    text.push('\n');
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mind-admin-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create");
        dir
    }

    #[test]
    fn roundtrip_three_files() {
        let dir = root("roundtrip");
        let mut mirror = AdminMirror::default();
        mirror.add_admin("aa", "Alice");
        mirror.add_ban(BanEntry::new(BanKind::Identity, "bb"));
        mirror.add_whitelist("cc", "Carol");
        mirror.save(&dir).expect("save");

        let loaded = AdminMirror::load(&dir);
        assert_eq!(loaded, mirror);
        assert!(loaded.is_admin("aa"));
        assert!(loaded.is_banned("bb", "Bob"));
        assert!(loaded.is_whitelisted("cc"));
        assert_eq!(loaded.name_for("aa"), Some("Alice"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn add_remove_is_idempotent() {
        let mut mirror = AdminMirror::default();
        mirror.add_admin("aa", "A");
        mirror.add_admin("aa", "ignored");
        assert_eq!(mirror.admins.len(), 1);
        mirror.remove_admin("aa");
        assert!(mirror.admins.is_empty());

        mirror.add_ban(BanEntry::new(BanKind::Name, "griefer"));
        mirror.add_ban(BanEntry::new(BanKind::Name, "griefer"));
        assert_eq!(mirror.bans.len(), 1);
        assert!(mirror.is_banned("", "the-griefer-x"));
        mirror.remove_ban("griefer");
        assert!(mirror.bans.is_empty());
    }

    #[test]
    fn missing_and_malformed_files_are_empty() {
        let dir = root("missing");
        std::fs::write(dir.join(ADMINS_FILE), "{not json").expect("write");
        let mirror = AdminMirror::load(&dir);
        assert!(mirror.admins.is_empty());
        assert!(mirror.bans.is_empty());
        assert!(mirror.whitelist.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_scans_all_three_lists() {
        let mut mirror = AdminMirror::default();
        mirror.add_admin("aa11", "Alice");
        mirror.add_whitelist("bb22", "Bob");
        mirror.add_ban(BanEntry {
            kind: BanKind::Identity,
            target: "cc33".to_owned(),
            name: "Carol".to_owned(),
            reason: "cheating".to_owned(),
        });
        assert_eq!(mirror.search("alice").len(), 1);
        assert_eq!(mirror.search("").len(), 3);
        assert_eq!(mirror.search("cc33")[0].role, "banned:id");
    }

    #[test]
    fn ban_kind_parse() {
        assert_eq!(BanKind::parse("type-id"), Some(BanKind::Identity));
        assert_eq!(BanKind::parse("ip"), Some(BanKind::Subnet));
        assert_eq!(BanKind::parse("nope"), None);
    }
}
