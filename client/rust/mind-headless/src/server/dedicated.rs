// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Dedicated-server shape + transport/authority contract (plan 21 §3.11/§3.13,
//! plan 22 §3.6).
//!
//! Plan 22 owns the `mind-headless server` process entry and console. This
//! module fixes the **plan-21 match contract** a dedicated server uses: a
//! service identity from a token file (never a player token), a parsed match
//! config (map/mode/visibility/rotation), auto-pause policy and map rotation.
//! Under D2 the authoritative P2P-free game sim is deferred; the dedicated
//! server is a host that never plays (invariant §3.11).

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::ServerOptions;

/// Dedicated `serve` options (plan §3.11 CLI contract).
#[derive(Debug, Clone, Default)]
pub struct ServeOptions {
    /// Server data root (default `./config`).
    pub config_dir: PathBuf,
    /// SpacetimeDB URI (`--stdb-host`).
    pub stdb_host: String,
    /// SpacetimeDB database name (`--db`).
    pub db: String,
    /// Service-token file (`--admin-token-file`); the dedicated identity.
    pub admin_token_file: Option<PathBuf>,
    /// Match config JSON (`--match-config`).
    pub match_config: Option<PathBuf>,
    /// Offline: no STDB connection (invariant 1 test path).
    pub offline: bool,
    /// Startup console commands (tests pass `exit`).
    pub commands: Vec<String>,
    /// Optional boot-timing JSON output path.
    pub boot_timing_json: Option<PathBuf>,
    /// Optional profile JSON output path.
    pub profile_json: Option<PathBuf>,
}

/// Dedicated-server shape failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServeError {
    /// The service-token file is missing/empty.
    MissingToken(PathBuf),
    /// The match config could not be parsed.
    BadConfig(String),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServeError::MissingToken(path) => {
                write!(
                    f,
                    "service-token file `{}` is missing or empty",
                    path.display()
                )
            }
            ServeError::BadConfig(error) => write!(f, "bad match config: {error}"),
        }
    }
}

impl std::error::Error for ServeError {}

/// Parsed `--match-config` (plan §3.11).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct MatchConfig {
    /// Map content id / name.
    pub map_id: String,
    /// Deterministic map seed.
    pub map_seed: u64,
    /// Gamemode tag.
    pub mode: String,
    /// `public` or `unlisted`.
    pub visibility: String,
    /// Player cap.
    pub max_players: u16,
    /// Ordered map rotation (empty = host one map).
    pub map_rotation: Vec<String>,
    /// Pause when no players are connected (plan §3.11).
    pub auto_pause: bool,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            map_id: "synthetic".to_string(),
            map_seed: 0,
            mode: "survival".to_string(),
            visibility: "public".to_string(),
            max_players: 8,
            map_rotation: Vec::new(),
            auto_pause: false,
        }
    }
}

impl MatchConfig {
    /// The first map to host: the config's `map_id`, else the rotation head.
    pub fn initial_map(&self) -> String {
        if !self.map_id.is_empty() {
            self.map_id.clone()
        } else {
            self.map_rotation
                .first()
                .cloned()
                .unwrap_or_else(|| "synthetic".to_string())
        }
    }
}

/// Parses `--match-config` JSON (unknown fields ignored).
pub fn parse_match_config(text: &str) -> Result<MatchConfig, String> {
    serde_json::from_str(text).map_err(|error| error.to_string())
}

/// Reads a dedicated service token (trimmed, non-empty); never a player token.
pub fn read_service_token(path: &Path) -> Result<String, ServeError> {
    let text = std::fs::read_to_string(path).map_err(|_| ServeError::MissingToken(path.into()))?;
    let token = text.trim().to_string();
    if token.is_empty() {
        return Err(ServeError::MissingToken(path.into()));
    }
    Ok(token)
}

/// The map that follows `current` in `rotation` (wrapping); `None` for empty
/// rotations. Unknown `current` restarts at the head.
pub fn next_map(rotation: &[String], current: Option<&str>) -> Option<String> {
    match current {
        Some(name) => {
            let position = rotation.iter().position(|entry| entry == name);
            match position {
                Some(index) => {
                    let next = (index + 1) % rotation.len();
                    Some(rotation[next].clone())
                }
                None => rotation.first().cloned(),
            }
        }
        None => rotation.first().cloned(),
    }
}

/// Auto-pause decision (mirrors the server-side `should_auto_pause`).
pub fn should_pause(auto_pause: bool, player_count: u16, pvp: bool) -> bool {
    if !auto_pause {
        return false;
    }
    if pvp {
        player_count < 2
    } else {
        player_count == 0
    }
}

/// Boots the dedicated server shape: validate the service identity + config,
/// then run the plan-22 local server process with the derived startup commands.
pub fn run(options: ServeOptions) -> i32 {
    // Validate the service token before booting; the token itself is never
    // logged (plan 22 §3.11) and never a player identity.
    if let Some(path) = &options.admin_token_file
        && let Err(error) = read_service_token(path)
    {
        eprintln!("mind-headless serve: {error}");
        return 2;
    }
    let match_config = match &options.match_config {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(text) => match parse_match_config(&text) {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("mind-headless serve: {}: {error}", path.display());
                    return 2;
                }
            },
            Err(error) => {
                eprintln!(
                    "mind-headless serve: cannot read `{}`: {error}",
                    path.display()
                );
                return 2;
            }
        },
        None => MatchConfig::default(),
    };

    let mut commands = vec![format!(
        "host {} {}",
        match_config.initial_map(),
        match_config.mode
    )];
    commands.extend(options.commands.iter().cloned());

    crate::server::run(ServerOptions {
        config_dir: if options.config_dir.as_os_str().is_empty() {
            PathBuf::from("config")
        } else {
            options.config_dir
        },
        commands,
        socket_port: None,
        offline: options.offline,
        boot_timing_json: options.boot_timing_json,
        profile_json: options.profile_json,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mind-serve-{name}-{}", std::process::id()))
    }

    #[test]
    fn match_config_defaults_and_parse() {
        let config = MatchConfig::default();
        assert_eq!(config.initial_map(), "synthetic");
        assert!(!config.auto_pause);

        let parsed = parse_match_config(
            r#"{"map_id":"demo_flat","map_seed":7,"mode":"pvp","visibility":"unlisted","max_players":4,"map_rotation":["a","b"],"auto_pause":true}"#,
        )
        .expect("parse");
        assert_eq!(parsed.map_id, "demo_flat");
        assert_eq!(parsed.map_seed, 7);
        assert_eq!(parsed.max_players, 4);
        assert!(parsed.auto_pause);
        assert_eq!(parsed.initial_map(), "demo_flat");

        // Empty config keeps defaults.
        let empty = parse_match_config("{}").expect("parse empty");
        assert_eq!(empty, MatchConfig::default());
    }

    #[test]
    fn rotation_wraps() {
        let rotation = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert_eq!(next_map(&rotation, None).as_deref(), Some("a"));
        assert_eq!(next_map(&rotation, Some("a")).as_deref(), Some("b"));
        assert_eq!(next_map(&rotation, Some("c")).as_deref(), Some("a"));
        assert_eq!(next_map(&rotation, Some("zzz")).as_deref(), Some("a"));
        assert_eq!(next_map(&[], Some("a")), None);
        // A config with no map_id falls back to the rotation head.
        let config = MatchConfig {
            map_id: String::new(),
            map_rotation: vec!["first".to_string()],
            ..MatchConfig::default()
        };
        assert_eq!(config.initial_map(), "first");
    }

    #[test]
    fn service_token_is_read_and_validated() {
        let path = temp("token");
        std::fs::write(&path, "  abc123  \n").expect("write");
        assert_eq!(read_service_token(&path).expect("token"), "abc123");
        std::fs::write(&path, "   ").expect("write");
        assert_eq!(
            read_service_token(&path).unwrap_err(),
            ServeError::MissingToken(path.clone())
        );
        let _ = std::fs::remove_file(&path);
        assert!(read_service_token(&path).is_err());
    }

    #[test]
    fn auto_pause_matches_server_policy() {
        assert!(should_pause(true, 0, false));
        assert!(!should_pause(true, 1, false));
        assert!(should_pause(true, 1, true));
        assert!(!should_pause(true, 2, true));
        assert!(!should_pause(false, 0, false));
    }

    #[test]
    fn serve_boots_with_exit_command() {
        let root = temp("boot");
        let _ = std::fs::remove_dir_all(&root);
        let config_path = temp("boot-config.json");
        std::fs::write(
            &config_path,
            r#"{"map_id":"synthetic","mode":"survival","auto_pause":true}"#,
        )
        .expect("write config");
        let code = run(ServeOptions {
            config_dir: root.clone(),
            offline: true,
            match_config: Some(config_path.clone()),
            commands: vec!["exit".to_string()],
            ..ServeOptions::default()
        });
        assert_eq!(code, 0);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&config_path);
    }
}
