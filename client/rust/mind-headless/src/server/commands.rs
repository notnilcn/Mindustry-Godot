// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server console commands (plan 22 §3.7).
//!
//! Port of the 43-command `ServerControl.registerCommands` table. Commands that
//! are local (config/help/version/rules/maps/save/load/status/…) are implemented
//! against `mind-core`/the local host; multiplayer authority commands delegate
//! to the plan-21 seam and return a clear "requires plan 21/STDB" line until that
//! plan lands (R2).

use super::host::HostControl;
use super::{ServerState, console};

/// One entry of the `help` table.
#[derive(Debug, Clone, Copy)]
pub struct CommandSpec {
    /// Command name.
    pub name: &'static str,
    /// Parameter text (`<required> [optional] ...`).
    pub param: &'static str,
    /// Description.
    pub description: &'static str,
}

/// The upstream 43-command table (plan 22 §3.7).
pub static COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "help",
        param: "[command]",
        description: "List all commands, or show help for a command.",
    },
    CommandSpec {
        name: "version",
        param: "",
        description: "Display the server version.",
    },
    CommandSpec {
        name: "exit",
        param: "",
        description: "Stop hosting and exit the server.",
    },
    CommandSpec {
        name: "stop",
        param: "",
        description: "Stop hosting the current game.",
    },
    CommandSpec {
        name: "host",
        param: "[mapname] [mode]",
        description: "Open a server on the specified map.",
    },
    CommandSpec {
        name: "maps",
        param: "[all/custom/default]",
        description: "List all available maps.",
    },
    CommandSpec {
        name: "reloadassets",
        param: "",
        description: "Reload all content assets.",
    },
    CommandSpec {
        name: "reloadmaps",
        param: "",
        description: "Reload all maps.",
    },
    CommandSpec {
        name: "status",
        param: "",
        description: "Display the server status.",
    },
    CommandSpec {
        name: "mods",
        param: "",
        description: "List all loaded mods.",
    },
    CommandSpec {
        name: "mod",
        param: "<name...>",
        description: "Display information about a mod.",
    },
    CommandSpec {
        name: "js",
        param: "<script...>",
        description: "Run a script. Only available with script mods.",
    },
    CommandSpec {
        name: "say",
        param: "<message...>",
        description: "Send a message to all players.",
    },
    CommandSpec {
        name: "pause",
        param: "<on/off>",
        description: "Pause or unpause the game.",
    },
    CommandSpec {
        name: "rules",
        param: "[remove/add] [name] [value...]",
        description: "View or modify the current rules.",
    },
    CommandSpec {
        name: "dumpsettings",
        param: "",
        description: "Dump all settings.",
    },
    CommandSpec {
        name: "fillitems",
        param: "[team]",
        description: "Fill all cores with all items.",
    },
    CommandSpec {
        name: "playerlimit",
        param: "[off/number]",
        description: "Set the player limit.",
    },
    CommandSpec {
        name: "config",
        param: "[name] [value...]",
        description: "List or set a configuration value.",
    },
    CommandSpec {
        name: "subnet-ban",
        param: "[add/remove] [address]",
        description: "Ban or unban a subnet.",
    },
    CommandSpec {
        name: "name-ban",
        param: "[add/remove/clear] [regex]",
        description: "Ban names matching a regex.",
    },
    CommandSpec {
        name: "whitelist",
        param: "[add/remove] [ID]",
        description: "Add or remove a player from the whitelist.",
    },
    CommandSpec {
        name: "shuffle",
        param: "[none/all/custom/builtin]",
        description: "Set the map shuffle mode.",
    },
    CommandSpec {
        name: "nextmap",
        param: "<mapname...>",
        description: "Set the next map.",
    },
    CommandSpec {
        name: "kick",
        param: "<username...>",
        description: "Kick a player.",
    },
    CommandSpec {
        name: "ban",
        param: "[type-id/name/ip] <username/IP/ID...>",
        description: "Ban a player.",
    },
    CommandSpec {
        name: "bans",
        param: "",
        description: "List all bans.",
    },
    CommandSpec {
        name: "unban",
        param: "<ip/ID>",
        description: "Unban a player by IP or ID.",
    },
    CommandSpec {
        name: "pardon",
        param: "<ID>",
        description: "Pardon a player by ID.",
    },
    CommandSpec {
        name: "admin",
        param: "[add/remove] <username/ID...>",
        description: "Add or remove an admin.",
    },
    CommandSpec {
        name: "admins",
        param: "",
        description: "List all admins.",
    },
    CommandSpec {
        name: "players",
        param: "",
        description: "List all connected players.",
    },
    CommandSpec {
        name: "runwave",
        param: "",
        description: "Force the next wave to start.",
    },
    CommandSpec {
        name: "loadautosave",
        param: "",
        description: "Load the last auto-save.",
    },
    CommandSpec {
        name: "load",
        param: "<slot>",
        description: "Load a save slot.",
    },
    CommandSpec {
        name: "save",
        param: "<slot> [embedAssets]",
        description: "Save the current game to a slot.",
    },
    CommandSpec {
        name: "saves",
        param: "",
        description: "List all saves.",
    },
    CommandSpec {
        name: "gameover",
        param: "",
        description: "Force a game-over.",
    },
    CommandSpec {
        name: "info",
        param: "<IP/UUID/name...>",
        description: "Search for player info.",
    },
    CommandSpec {
        name: "search",
        param: "<name...>",
        description: "Search for player info by name.",
    },
    CommandSpec {
        name: "gc",
        param: "",
        description: "Print memory usage (no-op diagnostic).",
    },
    CommandSpec {
        name: "yes",
        param: "",
        description: "Re-run the last suggested command.",
    },
    CommandSpec {
        name: "dos-ban",
        param: "[add/remove] [ip]",
        description: "Blacklist an IP for DoS.",
    },
];

/// Result of a console line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The line was handled.
    Handled,
    /// The `exit` command was issued.
    Exit,
}

/// Executes one console line (stdin or socket).
pub fn execute(state: &mut ServerState, line: &str) -> Outcome {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Outcome::Handled;
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let Some((name, args)) = tokens.split_first() else {
        return Outcome::Handled;
    };
    dispatch(state, name, args)
}

fn dispatch(state: &mut ServerState, name: &str, args: &[&str]) -> Outcome {
    match name {
        "help" => cmd_help(state, args),
        "version" => {
            let build = mind_core::version::BuildInfo::embedded();
            state.info(&format!(
                "Mindustry-Godot {} — headless server",
                build.combined()
            ));
        }
        "exit" => {
            state.info("Exiting server.");
            state.exit = true;
            return Outcome::Exit;
        }
        "stop" => {
            state.host.stop();
            state.info("Server stopped.");
        }
        "host" => cmd_host(state, args),
        "maps" => cmd_maps(state),
        "reloadmaps" => cmd_reloadmaps(state),
        "reloadassets" => state.warn("reloadassets requires the plan-20 server asset loader"),
        "status" => cmd_status(state),
        "mods" | "mod" => state.info("0 mods loaded (plan-20 server loaders not built)"),
        "js" => state.error("script mods are not available in this build"),
        "say" => delegate(state, "say", "plan 21"),
        "pause" => cmd_pause(state, args),
        "rules" => cmd_rules(state, args),
        "dumpsettings" => cmd_dumpsettings(state),
        "config" => cmd_config(state, args),
        "fillitems" => delegate(state, "fillitems", "plan 12"),
        "playerlimit" | "subnet-ban" | "name-ban" | "whitelist" | "kick" | "ban" | "bans"
        | "unban" | "pardon" | "admin" | "admins" | "players" | "info" | "search" | "dos-ban" => {
            delegate(state, name, "plan 21 (STDB admin/host)")
        }
        "shuffle" | "nextmap" => state.warn(&format!("`{name}` needs the plan-19 map registry")),
        "runwave" => cmd_runwave(state),
        "loadautosave" => cmd_loadautosave(state),
        "load" => cmd_load(state, args),
        "save" => cmd_save(state, args),
        "saves" => cmd_saves(state),
        "gameover" => state.warn("gameover needs the plan-12 campaign state"),
        "gc" => cmd_gc(state),
        "yes" => cmd_yes(state),
        other => {
            let suggestion = console::suggest(other, COMMANDS.iter().map(|spec| spec.name));
            match &suggestion {
                Some(found) => state.warn(&format!(
                    "Unknown command: {other}. Did you mean \"{found}\"? To confirm, use the 'yes' command."
                )),
                None => state.warn(&format!("Unknown command: {other}. Enter 'help' to list commands.")),
            }
            state.console.record_unknown(other, suggestion);
        }
    }
    Outcome::Handled
}

fn delegate(state: &mut ServerState, name: &str, owner: &str) {
    state.warn(&format!(
        "`{name}` requires {owner}; not available in this build"
    ));
}

fn cmd_help(state: &mut ServerState, args: &[&str]) {
    if let Some(name) = args.first() {
        match COMMANDS.iter().find(|spec| spec.name == *name) {
            Some(spec) => {
                let param = if spec.param.is_empty() {
                    String::new()
                } else {
                    format!(" {}", spec.param)
                };
                state.info(&format!("{}{param} — {}", spec.name, spec.description));
            }
            None => state.warn(&format!("Unknown command: {name}")),
        }
        return;
    }
    state.info("Commands:");
    for spec in COMMANDS {
        let param = if spec.param.is_empty() {
            String::new()
        } else {
            format!(" {}", spec.param)
        };
        state.info(&format!(
            "  &lk{}{param}&fr — {}",
            spec.name, spec.description
        ));
    }
}

fn cmd_host(state: &mut ServerState, args: &[&str]) {
    let map = args.first().copied().unwrap_or("synthetic");
    let mode = args.get(1).copied().unwrap_or("survival");
    match state.host.host(map, mode) {
        Ok(()) => state.info(&format!("Hosting map '{map}' in mode '{mode}'.")),
        Err(error) => state.error(&format!("Could not host: {error}")),
    }
}

fn cmd_maps(state: &mut ServerState) {
    let dir = state.paths.maps();
    match std::fs::read_dir(&dir) {
        Ok(entries) => {
            let mut names: Vec<String> = entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            state.info(&format!("Maps ({}):", names.len()));
            for name in names {
                state.info(&format!("  {name}"));
            }
        }
        Err(_) => state.info("Maps (0):"),
    }
}

fn cmd_reloadmaps(state: &mut ServerState) {
    let dir = state.paths.maps();
    let count = std::fs::read_dir(&dir)
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0);
    state.info(&format!("Reloaded {count} map(s)."));
}

fn cmd_status(state: &mut ServerState) {
    let status = state.host.status();
    if !status.hosting {
        state.info("Hosting: no");
        return;
    }
    state.info(&format!(
        "Hosting: {} | mode={} | wave={} | tick={} | players={}",
        status.map.as_deref().unwrap_or("?"),
        status.mode.as_deref().unwrap_or("?"),
        status.wave,
        status.tick,
        status.players
    ));
}

fn cmd_pause(state: &mut ServerState, args: &[&str]) {
    match args.first().copied() {
        Some("on") | Some("true") => {
            state.paused = true;
            state.info("Game paused.");
        }
        Some("off") | Some("false") => {
            state.paused = false;
            state.info("Game resumed.");
        }
        _ => state.warn("Usage: pause <on/off>"),
    }
}

fn cmd_rules(state: &mut ServerState, args: &[&str]) {
    match args.first().copied() {
        None => {
            state.info("Rules:");
            let entries: Vec<(String, String)> = state
                .rules
                .entries
                .iter()
                .map(|(key, value)| (key.clone(), value.to_string()))
                .collect();
            for (key, value) in entries {
                state.info(&format!("  {key}: {value}"));
            }
        }
        Some("add") => {
            let Some(key) = args.get(1) else {
                state.warn("Usage: rules add <name> <value...>");
                return;
            };
            let value = args[2..].join(" ");
            state.rules.set(key, &value);
            state.rules_state.apply(&state.rules);
            let text = state.rules.to_text();
            let _ = std::fs::write(state.config_dir.join("rules.hjson"), text);
            state.info(&format!("Set rule {key} = {value}"));
        }
        Some("remove") => {
            let Some(key) = args.get(1) else {
                state.warn("Usage: rules remove <name>");
                return;
            };
            state.rules.remove(key);
            state.rules_state.apply(&state.rules);
            let text = state.rules.to_text();
            let _ = std::fs::write(state.config_dir.join("rules.hjson"), text);
            state.info(&format!("Removed rule {key}"));
        }
        Some(other) => state.warn(&format!("Unknown rules subcommand: {other}")),
    }
}

fn cmd_dumpsettings(state: &mut ServerState) {
    let mut entries = state.config.entries();
    entries.sort_by_key(|(spec, _)| spec.key);
    state.info("Settings:");
    for (spec, value) in entries {
        state.info(&format!("  {} = {}", spec.key, setting_to_string(&value)));
    }
}

fn cmd_config(state: &mut ServerState, args: &[&str]) {
    match args.len() {
        0 => {
            state.info("Config:");
            for (spec, value) in state.config.entries() {
                state.info(&format!("  {} = {}", spec.name, setting_to_string(&value)));
            }
        }
        1 => match state.config.get(args[0]) {
            Some(value) => state.info(&format!("{} = {}", args[0], setting_to_string(&value))),
            None => state.warn(&format!("Unknown config key: {}", args[0])),
        },
        _ => {
            let name = args[0];
            let value = args[1..].join(" ");
            match state.config.set(name, &value) {
                Ok(spec) => {
                    state.info(&format!("Set {} to {}", spec.name, value));
                    if spec.name == "debug" {
                        // Log level changes are handled by the plan-00 logger.
                    }
                }
                Err(error) => state.error(&error.to_string()),
            }
        }
    }
}

fn cmd_runwave(state: &mut ServerState) {
    match state.host.run_wave() {
        Ok(()) => state.info("Wave forced."),
        Err(error) => state.error(&error.to_string()),
    }
}

fn cmd_loadautosave(state: &mut ServerState) {
    let saves = super::autosave::rotation_order(&state.paths.saves());
    match saves.last() {
        Some(path) => {
            let path = path.clone();
            let fs = state.fs();
            match state.host.load(&fs, &path) {
                Ok(()) => state.info(&format!("Loaded autosave {}.", path.display())),
                Err(error) => state.error(&error.to_string()),
            }
        }
        None => {
            state.warn("No auto-saves found! Type `config autosave true` to enable auto-saves.")
        }
    }
}

fn cmd_load(state: &mut ServerState, args: &[&str]) {
    let Some(slot) = args.first().and_then(|value| value.parse::<u32>().ok()) else {
        state.warn("Usage: load <slot>");
        return;
    };
    match state.load_slot(slot) {
        Ok(path) => state.info(&format!("Loaded save {}.", path.display())),
        Err(error) => state.error(&error.to_string()),
    }
}

fn cmd_save(state: &mut ServerState, args: &[&str]) {
    let Some(slot) = args.first().and_then(|value| value.parse::<u32>().ok()) else {
        state.warn("Usage: save <slot> [embedAssets]");
        return;
    };
    match state.save_slot(slot) {
        Ok(path) => state.info(&format!("Saved to {}.", path.display())),
        Err(error) => state.error(&error.to_string()),
    }
}

fn cmd_saves(state: &mut ServerState) {
    let dir = state.paths.saves();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    state.info(&format!("Saves ({}):", names.len()));
    for name in names {
        state.info(&format!("  {name}"));
    }
}

fn cmd_gc(state: &mut ServerState) {
    state.info("gc: no-op diagnostic (Rust has no GC)");
    if let Ok(status) = std::fs::read_to_string("/proc/self/status")
        && let Some(line) = status.lines().find(|line| line.starts_with("VmRSS:"))
    {
        state.info(line.trim());
    }
}

fn cmd_yes(state: &mut ServerState) {
    match state.console.resolve_yes() {
        Some(command) => {
            state.console.clear();
            if command.split_whitespace().next() == Some("yes") {
                state.warn("No command to confirm.");
                return;
            }
            state.info(&format!("Running '{command}'."));
            execute(state, &command);
        }
        None => state.warn("No command to confirm."),
    }
}

fn setting_to_string(value: &mind_core::io::settings::SettingValue) -> String {
    use mind_core::io::settings::SettingValue;
    match value {
        SettingValue::Str(value) => value.clone(),
        SettingValue::Int(value) => value.to_string(),
        SettingValue::Long(value) => value.to_string(),
        SettingValue::Float(value) => value.to_string(),
        SettingValue::Bool(value) => value.to_string(),
        SettingValue::Json(value) => value.clone(),
        SettingValue::Bytes(value) => format!("<{} bytes>", value.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{ServerOptions, run};

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mind-srv-cmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create");
        dir
    }

    #[test]
    fn command_table_has_43_entries() {
        assert_eq!(COMMANDS.len(), 43);
    }

    #[test]
    fn rules_and_config_commands_run() {
        let dir = temp_dir("run");
        let options = ServerOptions {
            config_dir: dir.clone(),
            commands: vec![
                "config name Demo".to_owned(),
                "rules add reactorExplosions true".to_owned(),
                "host synthetic survival".to_owned(),
                "runwave".to_owned(),
                "status".to_owned(),
                "save 0".to_owned(),
                "exit".to_owned(),
            ],
            ..ServerOptions::default()
        };
        assert_eq!(run(options), 0);
        assert!(dir.join("saves").join("0.msav").exists());
        let rules = std::fs::read_to_string(dir.join("rules.hjson")).expect("rules");
        assert!(rules.contains("reactorExplosions: true"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_map_is_non_fatal() {
        let dir = temp_dir("badmap");
        let options = ServerOptions {
            config_dir: dir.clone(),
            commands: vec!["host unknown-map-name".to_owned(), "exit".to_owned()],
            ..ServerOptions::default()
        };
        assert_eq!(run(options), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
