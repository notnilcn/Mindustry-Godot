// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Dedicated-server config (plan 22 §3.6/§6.3).
//!
//! Port of `core/src/mindustry/net/Administration.java` `Config` (the 34 keys,
//! exact names/defaults plus the `servername`/`socket`/`allow-custom` storage
//! aliases). Persistence is the plan-04 `SettingsStore` at
//! `<config>/settings.json`; enforcement is plan 21 and not built here.

use std::path::{Path, PathBuf};

use mind_core::io::fs::{FileSystem, Paths};
use mind_core::io::settings::{SettingValue, SettingsStore};

/// Value kind of a config entry (`Config.isNum/isBool/isString`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigKind {
    /// String value.
    Str,
    /// 32-bit integer value.
    Int,
    /// Boolean value.
    Bool,
}

/// Compile-time default for a config entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultValue {
    /// String default.
    Str(&'static str),
    /// Integer default.
    Int(i32),
    /// Boolean default.
    Bool(bool),
}

impl DefaultValue {
    /// The runtime [`SettingValue`] for this default.
    pub fn to_setting(self) -> SettingValue {
        match self {
            DefaultValue::Str(value) => SettingValue::Str(value.to_owned()),
            DefaultValue::Int(value) => SettingValue::Int(value),
            DefaultValue::Bool(value) => SettingValue::Bool(value),
        }
    }

    /// The value kind (`Config.isNum/isBool/isString`).
    pub fn kind(self) -> ConfigKind {
        match self {
            DefaultValue::Str(_) => ConfigKind::Str,
            DefaultValue::Int(_) => ConfigKind::Int,
            DefaultValue::Bool(_) => ConfigKind::Bool,
        }
    }
}

/// One `Config` entry (name/key/description/default).
#[derive(Debug, Clone, Copy)]
pub struct ConfigSpec {
    /// Console/display name (`Config.name`).
    pub name: &'static str,
    /// Storage key (`Config.key`; usually the name, sometimes an alias).
    pub key: &'static str,
    /// Human description (`Config.description`).
    pub description: &'static str,
    /// Default value.
    pub default: DefaultValue,
}

/// The upstream 34 `Config` entries, in declaration order. Headless defaults
/// are used for the two platform-dependent entries (`antiSpam=true`,
/// `allowCustomClients=false`), matching `ServerLauncher` (`headless=true`).
pub static CONFIGS: &[ConfigSpec] = &[
    ConfigSpec {
        name: "name",
        key: "servername",
        description: "The server name as displayed on clients.",
        default: DefaultValue::Str("Server"),
    },
    ConfigSpec {
        name: "desc",
        key: "desc",
        description: "The server description, displayed under the name. Max 100 characters.",
        default: DefaultValue::Str("off"),
    },
    ConfigSpec {
        name: "port",
        key: "port",
        description: "The port to host on.",
        default: DefaultValue::Int(6567),
    },
    ConfigSpec {
        name: "autoUpdate",
        key: "autoUpdate",
        description: "Whether to auto-update and exit when a new bleeding-edge update arrives.",
        default: DefaultValue::Bool(false),
    },
    ConfigSpec {
        name: "showConnectMessages",
        key: "showConnectMessages",
        description: "Whether to display connect/disconnect messages.",
        default: DefaultValue::Bool(true),
    },
    ConfigSpec {
        name: "enableVotekick",
        key: "enableVotekick",
        description: "Whether votekick is enabled.",
        default: DefaultValue::Bool(true),
    },
    ConfigSpec {
        name: "startCommands",
        key: "startCommands",
        description: "Commands run at startup. This should be a comma-separated list.",
        default: DefaultValue::Str(""),
    },
    ConfigSpec {
        name: "logging",
        key: "logging",
        description: "Whether to log everything to files.",
        default: DefaultValue::Bool(true),
    },
    ConfigSpec {
        name: "strict",
        key: "strict",
        description: "Whether strict mode is on - corrects positions and prevents duplicate UUIDs.",
        default: DefaultValue::Bool(true),
    },
    ConfigSpec {
        name: "antiSpam",
        key: "antiSpam",
        description: "Whether spammers are automatically kicked and rate-limited.",
        default: DefaultValue::Bool(true),
    },
    ConfigSpec {
        name: "interactRateWindow",
        key: "interactRateWindow",
        description: "Block interaction rate limit window, in seconds.",
        default: DefaultValue::Int(6),
    },
    ConfigSpec {
        name: "interactRateLimit",
        key: "interactRateLimit",
        description: "Block interaction rate limit.",
        default: DefaultValue::Int(25),
    },
    ConfigSpec {
        name: "interactRateKick",
        key: "interactRateKick",
        description: "How many times a player must interact inside the window to get kicked.",
        default: DefaultValue::Int(60),
    },
    ConfigSpec {
        name: "messageRateLimit",
        key: "messageRateLimit",
        description: "Message rate limit in seconds. 0 to disable.",
        default: DefaultValue::Int(0),
    },
    ConfigSpec {
        name: "messageSpamKick",
        key: "messageSpamKick",
        description: "How many times a player must send a message before the cooldown to get kicked. 0 to disable.",
        default: DefaultValue::Int(3),
    },
    ConfigSpec {
        name: "packetSpamLimit",
        key: "packetSpamLimit",
        description: "Limit for packet count sent within 3sec that will lead to a blacklist + kick.",
        default: DefaultValue::Int(300),
    },
    ConfigSpec {
        name: "uuidChangeLimit",
        key: "uuidChangeLimit",
        description: "Limit for how many UUID changes an IP can send in the time frame specified by uuidChangeTimePeriod before it gets banned.",
        default: DefaultValue::Int(10),
    },
    ConfigSpec {
        name: "uuidChangeTimePeriod",
        key: "uuidChangeTimePeriod",
        description: "Time window for the uuidChangeLimit config, in hours.",
        default: DefaultValue::Int(3),
    },
    ConfigSpec {
        name: "chatSpamLimit",
        key: "chatSpamLimit",
        description: "Limit for chat packet count sent within 2sec that will lead to a blacklist + kick. Not the same as a rate limit.",
        default: DefaultValue::Int(20),
    },
    ConfigSpec {
        name: "socketInput",
        key: "socket",
        description: "Allows a local application to control this server through a local TCP socket.",
        default: DefaultValue::Bool(false),
    },
    ConfigSpec {
        name: "socketInputPort",
        key: "socketInputPort",
        description: "The port for socket input.",
        default: DefaultValue::Int(6859),
    },
    ConfigSpec {
        name: "socketInputAddress",
        key: "socketInputAddress",
        description: "The bind address for socket input.",
        default: DefaultValue::Str("localhost"),
    },
    ConfigSpec {
        name: "allowCustomClients",
        key: "allow-custom",
        description: "Whether custom clients are allowed to connect.",
        default: DefaultValue::Bool(false),
    },
    ConfigSpec {
        name: "whitelist",
        key: "whitelist",
        description: "Whether the whitelist is used.",
        default: DefaultValue::Bool(false),
    },
    ConfigSpec {
        name: "motd",
        key: "motd",
        description: "The message displayed to people on connection.",
        default: DefaultValue::Str("off"),
    },
    ConfigSpec {
        name: "autosave",
        key: "autosave",
        description: "Whether the periodically save the map when playing.",
        default: DefaultValue::Bool(false),
    },
    ConfigSpec {
        name: "autosaveAmount",
        key: "autosaveAmount",
        description: "The maximum amount of autosaves. Older ones get replaced.",
        default: DefaultValue::Int(10),
    },
    ConfigSpec {
        name: "autosaveSpacing",
        key: "autosaveSpacing",
        description: "Spacing between autosaves in seconds.",
        default: DefaultValue::Int(60 * 5),
    },
    ConfigSpec {
        name: "debug",
        key: "debug",
        description: "Enable debug logging.",
        default: DefaultValue::Bool(false),
    },
    ConfigSpec {
        name: "snapshotInterval",
        key: "snapshotInterval",
        description: "Client entity snapshot interval in ms.",
        default: DefaultValue::Int(200),
    },
    ConfigSpec {
        name: "autoPause",
        key: "autoPause",
        description: "Whether the game should pause when nobody is online.",
        default: DefaultValue::Bool(false),
    },
    ConfigSpec {
        name: "roundExtraTime",
        key: "roundExtraTime",
        description: "Time before loading a new map after the gameover, in seconds.",
        default: DefaultValue::Int(12),
    },
    ConfigSpec {
        name: "maxLogLength",
        key: "maxLogLength",
        description: "The Maximum log file size, in bytes.",
        default: DefaultValue::Int(1024 * 1024 * 5),
    },
    ConfigSpec {
        name: "logCommands",
        key: "logCommands",
        description: "Whether player commands should be logged.",
        default: DefaultValue::Bool(true),
    },
];

/// Config lookup/parse failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// The key does not name a known `Config` entry.
    UnknownKey(String),
    /// The value could not be coerced to the entry's kind.
    BadValue {
        /// Config name.
        name: String,
        /// Rejected value.
        value: String,
    },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::UnknownKey(name) => write!(f, "unknown config key: {name}"),
            ConfigError::BadValue { name, value } => {
                write!(f, "invalid value for `{name}`: {value:?}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// The loaded server configuration over a plan-04 `SettingsStore`.
#[derive(Debug)]
pub struct ServerConfig {
    store: SettingsStore,
    root: PathBuf,
    paths: Paths,
}

impl ServerConfig {
    /// Opens `<root>/settings.json` and registers the 34 upstream defaults.
    pub fn load(root: impl Into<PathBuf>, fs: &dyn FileSystem) -> Self {
        let root = root.into();
        let paths = Paths::new(root.clone());
        let mut store = SettingsStore::load(fs, &paths);
        let defaults: Vec<(&str, SettingValue)> = CONFIGS
            .iter()
            .map(|spec| (spec.key, spec.default.to_setting()))
            .collect();
        store.defaults(&defaults);
        Self { store, root, paths }
    }

    /// The config root directory (`<config>/`).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The underlying settings store.
    pub fn store(&self) -> &SettingsStore {
        &self.store
    }

    /// The underlying settings store, mutably.
    pub fn store_mut(&mut self) -> &mut SettingsStore {
        &mut self.store
    }

    /// The `Paths` rooted at [`Self::root`].
    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    /// Finds a config entry by display name or storage key.
    pub fn spec(name: &str) -> Option<&'static ConfigSpec> {
        CONFIGS
            .iter()
            .find(|spec| spec.name == name || spec.key == name)
    }

    /// Reads the raw stored value, falling back to the default.
    pub fn get(&self, name: &str) -> Option<SettingValue> {
        let spec = Self::spec(name)?;
        Some(
            self.store
                .get(spec.key)
                .cloned()
                .unwrap_or_else(|| spec.default.to_setting()),
        )
    }

    /// Reads a string config (`Config.string`).
    pub fn get_string(&self, name: &str) -> String {
        match Self::spec(name) {
            Some(spec) => {
                let default = match spec.default {
                    DefaultValue::Str(value) => value,
                    _ => "",
                };
                self.store.get_string(spec.key, default)
            }
            None => String::new(),
        }
    }

    /// Reads an `i32` config (`Config.num`).
    pub fn get_i32(&self, name: &str) -> i32 {
        Self::spec(name)
            .map(|spec| match spec.default {
                DefaultValue::Int(value) => self.store.get_i32(spec.key, value),
                _ => self.store.get_i32(spec.key, 0),
            })
            .unwrap_or(0)
    }

    /// Reads a boolean config (`Config.bool`).
    pub fn get_bool(&self, name: &str) -> bool {
        Self::spec(name)
            .map(|spec| match spec.default {
                DefaultValue::Bool(value) => self.store.get_bool(spec.key, value),
                _ => self.store.get_bool(spec.key, false),
            })
            .unwrap_or(false)
    }

    /// Sets a config from a console string value, coercing to the entry kind
    /// (`ServerControl` `config` command).
    pub fn set(&mut self, name: &str, value: &str) -> Result<&'static ConfigSpec, ConfigError> {
        let spec = Self::spec(name).ok_or_else(|| ConfigError::UnknownKey(name.to_owned()))?;
        let coerced = match spec.default.kind() {
            ConfigKind::Str => SettingValue::Str(value.to_owned()),
            ConfigKind::Bool => {
                SettingValue::Bool(parse_bool(value).ok_or_else(|| ConfigError::BadValue {
                    name: name.to_owned(),
                    value: value.to_owned(),
                })?)
            }
            ConfigKind::Int => SettingValue::Int(value.trim().parse::<i32>().map_err(|_| {
                ConfigError::BadValue {
                    name: name.to_owned(),
                    value: value.to_owned(),
                }
            })?),
        };
        self.store.put(spec.key, coerced);
        Ok(spec)
    }

    /// Every config entry with its current value, in declaration order.
    pub fn entries(&self) -> Vec<(&'static ConfigSpec, SettingValue)> {
        CONFIGS
            .iter()
            .map(|spec| {
                let value = self
                    .store
                    .get(spec.key)
                    .cloned()
                    .unwrap_or_else(|| spec.default.to_setting());
                (spec, value)
            })
            .collect()
    }

    /// Flushes to `<root>/settings.json`.
    pub fn force_save(&mut self, fs: &dyn FileSystem) -> Result<(), mind_core::io::IoError> {
        self.store.force_save(fs, &self.paths)
    }
}

/// `Strings.canParseBoolean`-style boolean coercion.
pub fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "on" | "yes" | "1" => Some(true),
        "false" | "off" | "no" | "0" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::io::fs::NativeFs;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("mind-srv-cfg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn defaults_match_upstream() {
        let root = temp_root("defaults");
        let config = ServerConfig::load(&root, &NativeFs);
        assert_eq!(config.get_string("name"), "Server");
        assert_eq!(config.get_i32("port"), 6567);
        assert!(!config.get_bool("autosave"));
        assert_eq!(config.get_i32("maxLogLength"), 5 * 1024 * 1024);
        assert!(config.get_bool("antiSpam"));
        assert!(!config.get_bool("allowCustomClients"));
        assert_eq!(config.get_i32("socketInputPort"), 6859);
        assert_eq!(config.get_string("socketInputAddress"), "localhost");
        assert_eq!(config.get_i32("roundExtraTime"), 12);
        assert_eq!(CONFIGS.len(), 34);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn set_coerce_bool_int_string() {
        let root = temp_root("coerce");
        let mut config = ServerConfig::load(&root, &NativeFs);
        config.set("autosave", "true").expect("bool");
        config.set("port", "7000").expect("int");
        config.set("name", "Demo").expect("string");
        assert!(config.get_bool("autosave"));
        assert_eq!(config.get_i32("port"), 7000);
        assert_eq!(config.get_string("name"), "Demo");
        assert!(config.set("port", "abc").is_err());
        assert!(config.set("autosave", "maybe").is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_key_rejected() {
        let root = temp_root("unknown");
        let mut config = ServerConfig::load(&root, &NativeFs);
        assert_eq!(
            config.set("nope", "1").unwrap_err(),
            ConfigError::UnknownKey("nope".to_owned())
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn socket_alias_key() {
        let root = temp_root("alias");
        let mut config = ServerConfig::load(&root, &NativeFs);
        // `socket` is `socketInput`'s storage alias.
        config.set("socket", "true").expect("set alias");
        assert!(config.get_bool("socketInput"));
        assert_eq!(
            ServerConfig::spec("socket").map(|s| s.name),
            Some("socketInput")
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn roundtrip_json() {
        let root = temp_root("roundtrip");
        {
            let mut config = ServerConfig::load(&root, &NativeFs);
            config.set("name", "RoundTrip").expect("set");
            config.set("autosave", "true").expect("set");
            config.force_save(&NativeFs).expect("save");
        }
        let reloaded = ServerConfig::load(&root, &NativeFs);
        assert_eq!(reloaded.get_string("name"), "RoundTrip");
        assert!(reloaded.get_bool("autosave"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
