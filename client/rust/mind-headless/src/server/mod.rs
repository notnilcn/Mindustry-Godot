// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Dedicated server (`mind-headless server`) — plan 22 M2 §3.6.
//!
//! Port of `server/src/mindustry/server/ServerLauncher.java` (`init` order) and
//! `ServerControl.java` (config, console, commands, rules, autosave, logs,
//! socket). Pure Rust and Godot-free; multiplayer host/admin delegation is
//! plan 21 and returns "requires plan 21/STDB" until that lands (R2).

pub mod autosave;
pub mod commands;
pub mod config;
pub mod console;
pub mod host;
pub mod logs;
pub mod rules_file;
pub mod socket;

use std::path::{Path, PathBuf};
use std::time::Instant;

use mind_core::content::{ContentRegistry, MemoryBundle, MemoryUnlockStore, create_base_content};
use mind_core::io::fs::{NativeFs, Paths};
use mind_core::io::settings::SettingValue;

use config::ServerConfig;
use console::ConsoleState;
use host::LocalHost;
use logs::LogRotator;
use rules_file::{RulesFile, RulesState};
use socket::CommandSocket;

/// Fixed simulation rate (matches `mind_core::constants::TICKS_PER_SECOND`).
pub const TICKS_PER_SECOND: u64 = 60;

/// Server boot options (`ServerControl(args)` + `ServerLauncher`).
#[derive(Debug, Clone, Default)]
pub struct ServerOptions {
    /// Server data root (`--config-dir`, upstream `config/`).
    pub config_dir: PathBuf,
    /// Startup command lines (each may itself be comma-separated).
    pub commands: Vec<String>,
    /// Socket port override (`--socket-port`; `0` = ephemeral test hook).
    pub socket_port: Option<u16>,
    /// Offline mode (`--stdb offline`): no STDB connection.
    pub offline: bool,
    /// Optional boot-timing JSON output path.
    pub boot_timing_json: Option<PathBuf>,
    /// Optional tick-profile JSON output path.
    pub profile_json: Option<PathBuf>,
}

impl ServerOptions {
    /// Options rooted at `config_dir`.
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
            ..Self::default()
        }
    }
}

/// Live server state shared by the boot loop and [`commands`].
pub struct ServerState {
    /// Parsed server configuration.
    pub config: ServerConfig,
    /// Server data root.
    pub config_dir: PathBuf,
    /// Data `Paths` rooted at `config_dir`.
    pub paths: Paths,
    /// Local host (plan-21 host seam).
    pub host: LocalHost,
    /// Parsed `rules.hjson`.
    pub rules: RulesFile,
    /// Typed default rule state.
    pub rules_state: RulesState,
    /// Console `yes` state.
    pub console: ConsoleState,
    /// Console command socket.
    pub socket: CommandSocket,
    /// Rotating log file.
    pub logger: LogRotator,
    /// Whether the game is paused.
    pub paused: bool,
    /// Set by the `exit` command.
    pub exit: bool,
    /// Boot duration (ms), for `--boot-timing-json`.
    pub boot_ms: u64,
    /// Server start instant.
    pub started: Instant,
}

impl ServerState {
    /// The file system implementation (unit struct).
    pub fn fs(&self) -> NativeFs {
        NativeFs
    }

    /// Formats and emits a log line to stdout, the rotating file and the socket.
    pub fn emit(&mut self, level: &str, message: &str) {
        let line = format!("[{level}] {message}");
        println!("{line}");
        if let Err(error) = self.logger.write_line(&line) {
            eprintln!("mind-headless server: log write failed: {error}");
        }
        if let Err(error) = self.socket.send_log(&line) {
            log::warn!("[socket] send failed: {error}");
        }
    }

    /// Emits an `[I]` line.
    pub fn info(&mut self, message: &str) {
        self.emit("I", message);
    }

    /// Emits a `[W]` line.
    pub fn warn(&mut self, message: &str) {
        self.emit("W", message);
    }

    /// Emits an `[E]` line.
    pub fn error(&mut self, message: &str) {
        self.emit("E", message);
    }

    /// Saves a server save slot (`save <slot>`).
    pub fn save_slot(&mut self, slot: u32) -> Result<PathBuf, String> {
        let path = self.paths.save_slot(slot);
        self.host
            .save(&NativeFs, &path)
            .map_err(|error| error.to_string())?;
        Ok(path)
    }

    /// Loads a server save slot (`load <slot>`).
    pub fn load_slot(&mut self, slot: u32) -> Result<PathBuf, String> {
        let path = self.paths.save_slot(slot);
        self.host
            .load(&NativeFs, &path)
            .map_err(|error| error.to_string())?;
        Ok(path)
    }
}

/// Boots the dedicated server and runs `options.commands` (then stdin unless
/// `exit` was issued). Returns the process exit code.
pub fn run(options: ServerOptions) -> i32 {
    let started = Instant::now();
    let fs = NativeFs;
    let config_dir = if options.config_dir.as_os_str().is_empty() {
        PathBuf::from("config")
    } else {
        options.config_dir.clone()
    };
    for dir in [
        config_dir.clone(),
        Paths::new(&config_dir).saves(),
        Paths::new(&config_dir).maps(),
    ] {
        let _ = std::fs::create_dir_all(&dir);
    }

    // 1. Config + rules bootstrap.
    let config = ServerConfig::load(&config_dir, &fs);
    let rules_path = config_dir.join("rules.hjson");
    if let Err(error) = RulesFile::ensure_default(&fs, &rules_path) {
        eprintln!("mind-headless server: could not write rules.hjson: {error}");
    }
    let rules = RulesFile::load(&fs, &rules_path);
    let mut rules_state = RulesState::default();
    rules_state.apply(&rules);

    // 2. Logging (rotation from config).
    let max_log_length = config.get_i32("maxLogLength").max(1) as u64;
    let logger = match LogRotator::new(config_dir.join("logs"), max_log_length) {
        Ok(logger) => logger,
        Err(error) => {
            eprintln!("mind-headless server: could not open log file: {error}");
            return 2;
        }
    };

    // 3. Content (plan 02) — Godot-free base content.
    let registry = match boot_content() {
        Ok(registry) => Box::leak(Box::new(registry)),
        Err(error) => {
            eprintln!("| &ly[@] {error}");
            return 1;
        }
    };

    // 4. Socket.
    let mut socket = CommandSocket::disabled();
    if config.get_bool("socketInput") || options.socket_port.is_some() {
        let port = options
            .socket_port
            .unwrap_or_else(|| config.get_i32("socketInputPort").max(0) as u16);
        let address = config.get_string("socketInputAddress");
        let bind = format!("{address}:{port}");
        match CommandSocket::bind(&bind) {
            Ok(bound) => socket = bound,
            Err(error) => eprintln!("mind-headless server: socket bind {bind} failed: {error}"),
        }
    }

    let paths = Paths::new(&config_dir);
    let maps_dir = paths.maps();
    let mut state = ServerState {
        config,
        config_dir: config_dir.clone(),
        paths,
        host: LocalHost::new(registry, maps_dir),
        rules,
        rules_state,
        console: ConsoleState::default(),
        socket,
        logger,
        paused: false,
        exit: false,
        boot_ms: started.elapsed().as_millis() as u64,
        started,
    };

    state.info(&format!(
        "Mindustry-Godot server {} — Server loaded. Type 'help' for help.",
        mind_core::version::BuildInfo::embedded().combined()
    ));

    // 5. Startup commands: argv, then `config startCommands`.
    let mut startup: Vec<String> = Vec::new();
    for group in &options.commands {
        startup.extend(group.split(',').map(|part| part.trim().to_owned()));
    }
    let start_commands = state.config.get_string("startCommands");
    if !start_commands.trim().is_empty() {
        startup.extend(start_commands.split(',').map(|part| part.trim().to_owned()));
    }
    for line in startup {
        if line.is_empty() {
            continue;
        }
        commands::execute(&mut state, &line);
        if state.exit {
            break;
        }
    }

    // 6. Socket loop, else interactive stdin loop (skipped when startup
    //    commands were given and no socket is enabled, so scripted boots
    //    terminate).
    if !state.exit && state.socket.is_enabled() {
        socket_loop(&mut state);
    } else if !state.exit && options.commands.is_empty() {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            match line {
                Ok(line) => {
                    commands::execute(&mut state, line.trim());
                    if state.exit {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }

    // 7. Shutdown: persist settings, force rules, run the socket down.
    if let Err(error) = state.config.force_save(&fs) {
        state.error(&format!("could not save settings.json: {error}"));
    }
    write_settings_json(&state.config, &config_dir.join("settings.json"));
    let _ = std::fs::write(state.config_dir.join("rules.hjson"), state.rules.to_text());
    state.info("Server closed.");
    state.exit = true;

    if let Some(path) = &options.boot_timing_json {
        let json = serde_json::json!({
            "bootMs": state.boot_ms,
            "commandCount": state.config.entries().len(),
        });
        let _ = std::fs::write(
            path,
            serde_json::to_string_pretty(&json).unwrap_or_default(),
        );
    }
    if options.profile_json.is_some() {
        // Tick histogram profile is plan-21/23 work; the flag is accepted.
        state.warn("--profile-json is not implemented until the plan-21 load harness");
    }

    0
}

/// Serves the console socket until `exit` (plan 22 §6.6): one active client,
/// each line handled identically to stdin, every log line echoed back.
fn socket_loop(state: &mut ServerState) {
    if let Some(addr) = state.socket.local_addr() {
        state.info(&format!("Socket listening on {addr}"));
    }
    while !state.exit {
        if let Err(error) = state.socket.accept() {
            state.warn(&format!("socket accept failed: {error}"));
        }
        match state.socket.read_line() {
            Ok(Some(line)) => {
                if !line.is_empty() {
                    commands::execute(state, &line);
                }
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(error) => {
                state.warn(&format!("socket read failed: {error}"));
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }
}

/// Boots base content (plan 02), the headless server path.
pub fn boot_content() -> Result<ContentRegistry, String> {
    let bundle = MemoryBundle::new();
    let store = MemoryUnlockStore::new();
    let mut registry =
        create_base_content(&bundle, &store, true).map_err(|error| error.to_string())?;
    registry.init().map_err(|error| error.to_string())?;
    registry.post_init().map_err(|error| error.to_string())?;
    registry.load().map_err(|error| error.to_string())?;
    Ok(registry)
}

/// Resolves the server data root (`--config-dir`, else `./config`).
pub fn resolve_config_dir(explicit: Option<&Path>) -> PathBuf {
    explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("config"))
}

/// Writes a human-readable JSON mirror of the 34 config keys
/// (`<config>/settings.json`); the durable copy is the 04 `SettingsStore` file.
fn write_settings_json(config: &ServerConfig, path: &Path) {
    let mut map = serde_json::Map::new();
    for (spec, value) in config.entries() {
        map.insert(spec.name.to_owned(), setting_json(&value));
    }
    let json = serde_json::Value::Object(map);
    let _ = std::fs::write(
        path,
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    );
}

fn setting_json(value: &SettingValue) -> serde_json::Value {
    match value {
        SettingValue::Str(value) => serde_json::Value::String(value.clone()),
        SettingValue::Int(value) => serde_json::Value::Number((*value).into()),
        SettingValue::Long(value) => serde_json::Value::Number((*value).into()),
        SettingValue::Float(value) => serde_json::Number::from_f64(f64::from(*value))
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        SettingValue::Bool(value) => serde_json::Value::Bool(*value),
        SettingValue::Json(value) => serde_json::from_str(value).unwrap_or(serde_json::Value::Null),
        SettingValue::Bytes(value) => serde_json::Value::String(format!("<{} bytes>", value.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boot_content_and_config_roundtrip() {
        let root = std::env::temp_dir().join(format!("mind-srv-boot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create");
        let options = ServerOptions {
            config_dir: root.clone(),
            commands: vec!["config name Demo".to_owned(), "exit".to_owned()],
            ..ServerOptions::default()
        };
        assert_eq!(run(options), 0);
        assert!(root.join("settings.json").exists());
        assert!(root.join("rules.hjson").exists());
        assert!(root.join("logs").join("log-0.txt").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
