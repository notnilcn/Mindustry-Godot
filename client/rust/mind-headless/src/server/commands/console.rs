// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server console commands (plan 22 §3.7).
//!
//! Port of the 43-command `ServerControl.registerCommands` table; the
//! `execute`/`dispatch` entry points and the `cmd_*` handlers.
//! Upstream: server/src/mindustry/server/ServerControl.java (`registerCommands`).

use crate::server::admin::BanKind;
use crate::server::host::{Admins, HostControl};
use crate::server::{ServerState, console};

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

/// The one-line notice printed when `config autoUpdate` is enabled (P22-3).
///
/// Mirrors `mind_gdext::platform::update::NOT_AVAILABLE`; the dedicated server
/// cannot depend on `mind-gdext` (which pulls in Godot), so the string is kept
/// here as the server-side disposition record.
pub const UPDATER_NOT_AVAILABLE: &str = "updater not available in this build";

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
    // `CommandHandler("/")`: a leading slash is a player chat command
    // (`NetClient.sendChatMessage`), not a console command. The dedicated
    // host has no local player, so the console acts as a virtual admin
    // (documented deviation: lets the headless host exercise/report the
    // player command surface; a networked ingress uses `handle_chat`).
    if trimmed.starts_with('/') {
        state.handle_chat_console(trimmed);
        return Outcome::Handled;
    }
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let Some((name, args)) = tokens.split_first() else {
        return Outcome::Handled;
    };
    dispatch(state, name, args)
}

pub(super) fn dispatch(state: &mut ServerState, name: &str, args: &[&str]) -> Outcome {
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
            let _ = state.net.kick_all("server closing");
            state.host.stop();
            state.net.stop();
            state.info("Exiting server.");
            state.exit = true;
            return Outcome::Exit;
        }
        "stop" => {
            // `net.closeServer()` (`ServerControl.java:505-510`) disconnects
            // every member before the world goes away.
            let kicked = state.net.kick_all("server closing");
            if kicked > 0 {
                state.info(&format!("Kicked {kicked} player(s)."));
            }
            state.host.stop();
            state.net.stop();
            state.info("Server stopped.");
        }
        "host" => cmd_host(state, args),
        "maps" => cmd_maps(state),
        "reloadmaps" => cmd_reloadmaps(state),
        "reloadassets" => cmd_reloadassets(state),
        "status" => cmd_status(state),
        "mods" => cmd_mods(state, None),
        "mod" => cmd_mods(state, args.first().copied()),
        "js" => state.error("script mods are not available in this build"),
        "say" => cmd_say(state, args),
        "pause" => cmd_pause(state, args),
        "rules" => cmd_rules(state, args),
        "dumpsettings" => cmd_dumpsettings(state),
        "config" => cmd_config(state, args),
        "fillitems" => cmd_fillitems(state, args),
        "playerlimit" => cmd_playerlimit(state, args),
        "subnet-ban" | "dos-ban" => cmd_ip_ban(state, name, args),
        "name-ban" => cmd_name_ban(state, args),
        "whitelist" => cmd_whitelist(state, args),
        "kick" => cmd_kick(state, args),
        "ban" => cmd_ban(state, args),
        "bans" => cmd_bans(state),
        "unban" => cmd_unban(state, args, "Unbanned"),
        "pardon" => cmd_unban(state, args, "Pardoned"),
        "admin" => cmd_admin(state, args),
        "admins" => cmd_admins(state),
        "players" => cmd_players(state),
        "info" | "search" => cmd_search(state, args),
        "shuffle" => cmd_shuffle(state, args),
        "nextmap" => cmd_nextmap(state, args),
        "runwave" => cmd_runwave(state),
        "loadautosave" => cmd_loadautosave(state),
        "load" => cmd_load(state, args),
        "save" => cmd_save(state, args),
        "saves" => cmd_saves(state),
        "gameover" => cmd_gameover(state),
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

/// Maps the dedicated server's config to the module's `server_config` fields
/// (`admin_set_config`). Java has one global `Config` object; under D2 the
/// reducer-side relay (votekick/anti-spam/interaction gates) reads its own
/// `server_config` row, so the host pushes the authoritative values whenever a
/// match opens or `config` changes (GAPS #33 `enableVotekick`/`interactRate`).
pub fn server_config_pushes(
    config: &crate::server::config::ServerConfig,
) -> Vec<(&'static str, String)> {
    vec![
        ("server_name", config.get_string("name")),
        ("description", config.get_string("desc")),
        ("motd", config.get_string("motd")),
        (
            "whitelist_enabled",
            config.get_bool("whitelist").to_string(),
        ),
        (
            "allow_custom_clients",
            config.get_bool("allowCustomClients").to_string(),
        ),
        ("auto_pause", config.get_bool("autoPause").to_string()),
        (
            "enable_votekick",
            config.get_bool("enableVotekick").to_string(),
        ),
        ("anti_spam", config.get_bool("antiSpam").to_string()),
        (
            "interact_rate_window_secs",
            config.get_i32("interactRateWindow").max(0).to_string(),
        ),
        (
            "interact_rate_limit",
            config.get_i32("interactRateLimit").max(0).to_string(),
        ),
        (
            "interact_rate_kick",
            config.get_i32("interactRateKick").max(0).to_string(),
        ),
        (
            "message_rate_limit_secs",
            config.get_i32("messageRateLimit").max(0).to_string(),
        ),
        (
            "message_spam_kick",
            config.get_i32("messageSpamKick").max(0).to_string(),
        ),
        (
            "uuid_change_limit",
            config.get_i32("uuidChangeLimit").max(0).to_string(),
        ),
        (
            "uuid_change_time_period_hours",
            config.get_i32("uuidChangeTimePeriod").max(0).to_string(),
        ),
    ]
}

/// `fillitems [team]` (`ServerControl.java:772-795`): fills the team's first
/// core with every item.
pub(super) fn cmd_fillitems(state: &mut ServerState, args: &[&str]) {
    if !state.host.is_hosting() {
        state.error("Not playing. Host first.");
        return;
    }
    let team = args.first().copied();
    match state.host.fill_items(team) {
        Ok(0) => state.error("That team has no cores."),
        Ok(count) => state.info(&format!("Core filled ({count} core(s)).")),
        Err(error) => state.error(&error.to_string()),
    }
}

/// `shuffle [none/all/custom/builtin]` (`ServerControl.java:966-979`).
pub(super) fn cmd_shuffle(state: &mut ServerState, args: &[&str]) {
    use mind_core::maps::shuffle::ShuffleMode;
    match args.first().copied() {
        None => state.info(&format!(
            "Shuffle mode currently set to '{}'.",
            state.host.rotation().shuffle.name()
        )),
        Some(name) => {
            if !matches!(name, "none" | "all" | "custom" | "builtin") {
                state.error("Invalid shuffle mode.");
                return;
            }
            let mode = ShuffleMode::from_name(name);
            state.host.rotation_mut().set_shuffle(mode);
            state.info(&format!("Shuffle mode set to '{name}'."));
        }
    }
}

/// `nextmap <mapname...>` (`ServerControl.java:981-989`).
pub(super) fn cmd_nextmap(state: &mut ServerState, args: &[&str]) {
    let Some(name) = args.first().copied().filter(|name| !name.is_empty()) else {
        state.warn("Usage: nextmap <mapname...>");
        return;
    };
    state.host.rotation_mut().set_next(name);
    state.info(&format!("Next map set to '{name}'."));
}

/// `gameover` (`ServerControl.java:1234-1243`): forces the game-over path.
pub(super) fn cmd_gameover(state: &mut ServerState) {
    if !state.host.is_hosting() {
        state.error("Not playing a map.");
        return;
    }
    match state.host.game_over() {
        Ok(()) => {
            state.info("Core destroyed.");
            state.drain_host_events();
        }
        Err(error) => state.error(&error.to_string()),
    }
}

/// `mods`/`mod <name>` (`ServerControl.java:641-667`): scans the mod directory
/// and lists the loaded mods.
pub(super) fn cmd_mods(state: &mut ServerState, name: Option<&str>) {
    let directory = state.mods.mod_directory().to_path_buf();
    match name {
        Some(name) => {
            // Java `mods.list().find(p -> p.meta.name.equalsIgnoreCase(arg[0]))`.
            let found = state
                .mods
                .list()
                .iter()
                .find(|entry| {
                    entry.name.eq_ignore_ascii_case(name)
                        || entry
                            .meta
                            .display_name
                            .as_deref()
                            .is_some_and(|display| display.eq_ignore_ascii_case(name))
                })
                .map(|entry| {
                    (
                        entry
                            .meta
                            .display_name
                            .clone()
                            .unwrap_or_else(|| entry.name.clone()),
                        entry.name.clone(),
                        entry.meta.version.clone().unwrap_or_default(),
                        entry.meta.author.clone().unwrap_or_default(),
                        entry.file.display().to_string(),
                        entry.meta.description.clone().unwrap_or_default(),
                    )
                });
            match found {
                Some((display, internal, version, author, path, description)) => {
                    state.info(&format!("Name: {display}"));
                    state.info(&format!("Internal Name: {internal}"));
                    state.info(&format!("Version: {version}"));
                    state.info(&format!("Author: {author}"));
                    state.info(&format!("Path: {path}"));
                    state.info(&format!("Description: {description}"));
                }
                None => state.error(&format!("No mod with name '{name}' found.")),
            }
        }
        None => {
            // Java: `"  @ &fi@ " + (enabled ? "" : " &lr(" + state + ")")`.
            let rows: Vec<String> = state
                .mods
                .list()
                .iter()
                .map(|entry| {
                    let state_note = if entry.enabled() {
                        String::new()
                    } else {
                        format!(" ({:?})", entry.state)
                    };
                    format!(
                        "  {} {} {}{state_note}",
                        entry
                            .meta
                            .display_name
                            .clone()
                            .unwrap_or_else(|| entry.name.clone()),
                        entry.meta.version.clone().unwrap_or_default(),
                        entry.name,
                    )
                })
                .collect();
            if rows.is_empty() {
                state.info("No mods found.");
            } else {
                state.info("Mods:");
                for row in rows {
                    state.info(&row);
                }
            }
            state.info(&format!("Mod directory: {}", directory.display()));
        }
    }
}

/// `reloadassets` (`ServerControl.java:580-587`): re-scan the mod directory and
/// rebuild the content registry from the enabled mods' JSON content + data
/// patches (the plan-20 `createModContent`/`loadModPatches` path). Contrast
/// Java, which reloads `<data>/assets` override files rather than mod content;
/// the Rust port has no separate data-asset directory, so this is the honest
/// equivalent and the command that makes new mod content visible to the host.
pub(super) fn cmd_reloadassets(state: &mut ServerState) {
    match state.reload_content() {
        Ok((0, files)) => {
            state.error("No valid asset files found.");
            if files > 0 {
                state.info(&format!("Reloaded {files} content file(s)."));
            }
        }
        Ok((mod_count, files)) => {
            if files == 0 {
                state.warn("No valid asset files found.");
            } else {
                state.info(&format!(
                    "Reloaded {files} content file(s) from {mod_count} mod(s)."
                ));
            }
            if state.host.is_hosting() {
                state.info(
                    "The running world keeps its loaded content; the next map load uses the new registry.",
                );
            }
        }
        Err(error) => state.error(&format!("Could not reload assets: {error}")),
    }
}

pub(super) fn cmd_help(state: &mut ServerState, args: &[&str]) {
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

pub(super) fn cmd_host(state: &mut ServerState, args: &[&str]) {
    let map = args
        .first()
        .copied()
        .filter(|arg| !arg.is_empty())
        .unwrap_or("synthetic");
    let mode = args.get(1).copied().unwrap_or("survival");
    state.info("Loading map...");
    match state.host.host(map, mode) {
        Ok(()) => {
            state.info(&format!("Map loaded. Hosting '{map}' in mode '{mode}'."));
            state.drain_host_events();
            // `netServer.openServer()` (`ServerControl.java:560`): the
            // plan-21 match row makes the server listed/joinable.
            if state.net.is_online() {
                match state.net.host(map, mode) {
                    Ok(()) => {
                        state.info("Opened a server: networked match created.");
                        // Sync the reducer-side gates with this host's config.
                        let pushes = server_config_pushes(&state.config);
                        let count = state.net.push_server_config(&pushes);
                        log::info!("pushed {count} server_config field(s) to the match");
                    }
                    Err(error) => {
                        state.error(&format!("Could not open a server: {error}"));
                        state.warn("Hosting locally only; clients cannot join.");
                    }
                }
            } else {
                state.info("STDB offline: hosting locally (no networked match).");
            }
        }
        Err(error) => state.error(&format!("Could not host: {error}")),
    }
}

pub(super) fn cmd_maps(state: &mut ServerState) {
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

pub(super) fn cmd_reloadmaps(state: &mut ServerState) {
    let dir = state.paths.maps();
    let count = std::fs::read_dir(&dir)
        .map(|entries| entries.filter_map(Result::ok).count())
        .unwrap_or(0);
    state.info(&format!("Reloaded {count} map(s)."));
}

pub(super) fn cmd_status(state: &mut ServerState) {
    let status = state.host.status();
    let live = state.net.players().len();
    if !status.hosting {
        state.info(&format!("Status: server closed | players={live}"));
        return;
    }
    state.info("Status:");
    state.info(&format!(
        "  Playing on map {} / Wave {}",
        status.map.as_deref().unwrap_or("?"),
        status.wave
    ));
    state.info(&format!(
        "  {} seconds until next wave.",
        status.seconds_until_wave.max(0)
    ));
    if status.auto_paused {
        state.info("  Paused (no players connected).");
    } else if status.paused {
        state.info("  Game paused.");
    }
    if let Some(next) = &status.next_map {
        state.info(&format!(
            "  Next map {} in {} seconds.",
            next,
            status.restart_in.unwrap_or(0) / crate::server::TICKS_PER_SECOND
        ));
    }
    state.info(&format!(
        "  mode={} | tick={} | players={live}",
        status.mode.as_deref().unwrap_or("?"),
        status.tick,
    ));
    // Dedicated-server extensions over the Java status text (GAPS #32/#33):
    // the loaded mod set, the autosave countdown and who is connected.
    let loaded = state
        .mods
        .list()
        .iter()
        .filter(|entry| entry.enabled())
        .count();
    state.info(&format!(
        "  Mods: {} loaded / {} found",
        loaded,
        state.mods.list().len()
    ));
    if state.config.get_bool("autosave") {
        let spacing = state.config.get_i32("autosaveSpacing").max(1) as u64;
        let interval = crate::server::autosave::spacing_ticks(spacing);
        let remaining =
            interval.saturating_sub(state.autosave_clock.saturating_sub(state.autosave_last));
        state.info(&format!(
            "  Autosave in {}s ({} file(s) kept)",
            remaining.div_ceil(crate::server::TICKS_PER_SECOND),
            state.config.get_i32("autosaveAmount").max(1)
        ));
    } else {
        state.info("  Autosave: off");
    }
    let players = state.player_views();
    if players.is_empty() {
        state.info("  No players connected.");
    } else {
        state.info(&format!("  Players ({})", players.len()));
        for player in &players {
            if !player.connected {
                continue;
            }
            state.info(&format!(
                "    {} {} / {}",
                if player.admin { "[A]" } else { "[P]" },
                player.name,
                player.identity
            ));
        }
    }
    // Plan-21 half: the networked match row (`my_match`) once create_match
    // round-trips through the relay.
    let net = state.net.status();
    if state.net.is_online() {
        state.info(&format!(
            "  STDB match: hosting={} map={} mode={} players={}",
            net.hosting,
            net.map.as_deref().unwrap_or("-"),
            net.mode.as_deref().unwrap_or("-"),
            net.players
        ));
    }
}

pub(super) fn cmd_say(state: &mut ServerState, args: &[&str]) {
    let message = args.join(" ");
    if message.trim().is_empty() {
        state.warn("Usage: say <message...>");
        return;
    }
    match state.net.say(&message) {
        Ok(()) => state.info("Server message sent."),
        Err(error) => state.warn(&error.to_string()),
    }
}

pub(super) fn cmd_players(state: &mut ServerState) {
    let players = state.net.players();
    state.info(&format!("Players ({}):", players.len()));
    for player in players {
        let name = if player.name.is_empty() {
            "?"
        } else {
            player.name.as_str()
        };
        let team = player
            .team
            .map(|team| team.to_string())
            .unwrap_or_else(|| "-".to_owned());
        state.info(&format!(
            "  {name} | id={} | team={team} | role={} | admin={} | connected={}",
            player.identity, player.role, player.admin, player.connected
        ));
    }
}

pub(super) fn cmd_kick(state: &mut ServerState, args: &[&str]) {
    let Some(target) = args.first().copied() else {
        state.warn("Usage: kick <username...>");
        return;
    };
    match state.net.kick(target) {
        Ok(()) => state.info(&format!("Kicked {target}.")),
        Err(error) => state.warn(&format!("Could not kick {target}: {error}")),
    }
}

pub(super) fn cmd_ban(state: &mut ServerState, args: &[&str]) {
    if args.is_empty() {
        state.warn("Usage: ban [type-id/name/ip] <username/IP/ID...>");
        return;
    }
    let (kind, target) = match BanKind::parse(args[0]) {
        Some(kind) if args.len() > 1 => (kind, args[1]),
        _ => (BanKind::Identity, args[0]),
    };
    match state.net.ban(kind, target) {
        Ok(()) => state.info(&format!("Banned {target} ({}).", kind.name())),
        Err(error) => state.warn(&format!("Could not ban {target}: {error}")),
    }
}

pub(super) fn cmd_bans(state: &mut ServerState) {
    let bans = state.net.mirror().bans.clone();
    state.info(&format!("Bans ({}):", bans.len()));
    for ban in bans {
        let name = if ban.name.is_empty() {
            String::new()
        } else {
            format!(" ({})", ban.name)
        };
        state.info(&format!("  {} {}{name}", ban.kind.name(), ban.target));
    }
}

pub(super) fn cmd_unban(state: &mut ServerState, args: &[&str], verb: &str) {
    let Some(target) = args.first().copied() else {
        state.warn("Usage: unban <ip/ID>");
        return;
    };
    match state.net.unban(target) {
        Ok(()) => state.info(&format!("{verb} {target}.")),
        Err(error) => state.warn(&error.to_string()),
    }
}

pub(super) fn cmd_admin(state: &mut ServerState, args: &[&str]) {
    match args.first().copied() {
        None => cmd_admins(state),
        Some("add") | Some("remove") => {
            let on = args[0] == "add";
            let Some(target) = args.get(1).copied() else {
                state.warn("Usage: admin add|remove <username/ID...>");
                return;
            };
            match state.net.grant(target, on) {
                Ok(()) => state.info(&format!(
                    "{} admin {target}.",
                    if on { "Added" } else { "Removed" }
                )),
                Err(error) => state.warn(&error.to_string()),
            }
        }
        Some(other) => state.warn(&format!("Unknown admin subcommand: {other}")),
    }
}

pub(super) fn cmd_admins(state: &mut ServerState) {
    let admins = state.net.mirror().admins.clone();
    state.info(&format!("Admins ({}):", admins.len()));
    for admin in admins {
        let name = if admin.name.is_empty() {
            String::new()
        } else {
            format!(" ({})", admin.name)
        };
        state.info(&format!("  {}{name}", admin.identity));
    }
}

pub(super) fn cmd_whitelist(state: &mut ServerState, args: &[&str]) {
    match args.first().copied() {
        Some("add") | Some("remove") => {
            let on = args[0] == "add";
            let Some(target) = args.get(1).copied() else {
                state.warn("Usage: whitelist add|remove <ID>");
                return;
            };
            match state.net.whitelist(target, on) {
                Ok(()) => state.info(&format!(
                    "{} whitelist for {target}.",
                    if on { "Added" } else { "Removed" }
                )),
                Err(error) => state.warn(&error.to_string()),
            }
        }
        Some("on") | Some("off") => {
            let on = args[0] == "on";
            match state.net.set_whitelist_enabled(on) {
                Ok(()) => state.info(&format!(
                    "Whitelist {}.",
                    if on { "enabled" } else { "disabled" }
                )),
                Err(error) => state.warn(&error.to_string()),
            }
        }
        _ => state.warn("Usage: whitelist add|remove <ID> | on|off"),
    }
}

pub(super) fn cmd_playerlimit(state: &mut ServerState, args: &[&str]) {
    match args.first().copied() {
        None => {
            let current = match state.net.configured_player_limit() {
                Some(limit) => limit.to_string(),
                None => "off".to_owned(),
            };
            state.info(&format!("Player limit: {current}"));
        }
        Some("off") | Some("0") => match state.net.player_limit(None) {
            Ok(()) => state.info("Player limit disabled."),
            Err(error) => state.warn(&error.to_string()),
        },
        Some(value) => match value.parse::<u16>() {
            Ok(limit) if limit > 0 => match state.net.player_limit(Some(limit)) {
                Ok(()) => state.info(&format!("Player limit set to {limit}.")),
                Err(error) => state.warn(&error.to_string()),
            },
            _ => state.warn("Usage: playerlimit [off/number]"),
        },
    }
}

pub(super) fn cmd_ip_ban(state: &mut ServerState, name: &str, args: &[&str]) {
    let kind = if name == "dos-ban" {
        BanKind::Dos
    } else {
        BanKind::Subnet
    };
    match args.first().copied() {
        Some("remove") => {
            let target = args.get(1).copied().unwrap_or("");
            match state.net.unban(target) {
                Ok(()) => state.info(&format!("Removed ban for {target}.")),
                Err(error) => state.warn(&error.to_string()),
            }
        }
        Some("add") | None => {
            let target = args.get(1).copied().unwrap_or("");
            if target.is_empty() {
                state.warn(&format!("Usage: {name} add|remove <ip>"));
                return;
            }
            match state.net.ban(kind, target) {
                Ok(()) => state.info(&format!("Banned {target}.")),
                Err(error) => state.warn(&error.to_string()),
            }
        }
        Some(other) => state.warn(&format!("Unknown {name} subcommand: {other}")),
    }
}

pub(super) fn cmd_name_ban(state: &mut ServerState, args: &[&str]) {
    match args.first().copied() {
        Some("add") => {
            let Some(target) = args.get(1).copied() else {
                state.warn("Usage: name-ban add <regex>");
                return;
            };
            match state.net.ban(BanKind::Name, target) {
                Ok(()) => state.info(&format!("Banned name pattern {target}.")),
                Err(error) => state.warn(&error.to_string()),
            }
        }
        Some("remove") => {
            let Some(target) = args.get(1).copied() else {
                state.warn("Usage: name-ban remove <regex>");
                return;
            };
            match state.net.unban(target) {
                Ok(()) => state.info(&format!("Removed name ban {target}.")),
                Err(error) => state.warn(&error.to_string()),
            }
        }
        Some("clear") => {
            state.warn("`name-ban clear` is not supported under D2; remove entries individually")
        }
        _ => state.warn("Usage: name-ban add|remove|clear [regex]"),
    }
}

pub(super) fn cmd_search(state: &mut ServerState, args: &[&str]) {
    let query = args.join(" ");
    if query.trim().is_empty() {
        state.warn("Usage: search <name...>");
        return;
    }
    let results = state.net.search(&query);
    state.info(&format!("Matches ({}):", results.len()));
    for entry in results {
        let name = if entry.name.is_empty() {
            "?"
        } else {
            entry.name.as_str()
        };
        state.info(&format!(
            "  {name} | id={} | {}",
            entry.identity, entry.role
        ));
    }
}

pub(super) fn cmd_pause(state: &mut ServerState, args: &[&str]) {
    if !state.host.is_hosting() {
        state.error("Cannot pause without a game running.");
        return;
    }
    match args.first().copied() {
        Some("on") | Some("true") => {
            state.host.set_paused(true);
            state.paused = true;
            state.info("Game paused.");
        }
        Some("off") | Some("false") => {
            state.host.set_paused(false);
            state.paused = false;
            state.info("Game unpaused.");
        }
        _ => state.warn("Usage: pause <on/off>"),
    }
}

pub(super) fn cmd_rules(state: &mut ServerState, args: &[&str]) {
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

pub(super) fn cmd_dumpsettings(state: &mut ServerState) {
    let mut entries = state.config.entries();
    entries.sort_by_key(|(spec, _)| spec.key);
    state.info("Settings:");
    for (spec, value) in entries {
        state.info(&format!("  {} = {}", spec.key, setting_to_string(&value)));
    }
}

pub(super) fn cmd_config(state: &mut ServerState, args: &[&str]) {
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
                    // Keep the module's enforcement gates in step with the
                    // dedicated server config (`enableVotekick`, `interactRate*`).
                    if state.net.is_online() {
                        let pushes = server_config_pushes(&state.config);
                        state.net.push_server_config(&pushes);
                    }
                    if spec.name == "autoUpdate" && state.config.get_bool("autoUpdate") {
                        // P22-3: the bleeding-edge updater is not ported. Retain
                        // the config key, print the one-line notice, never call
                        // the network at boot.
                        state.warn(UPDATER_NOT_AVAILABLE);
                    }
                }
                Err(error) => state.error(&error.to_string()),
            }
        }
    }
}

pub(super) fn cmd_runwave(state: &mut ServerState) {
    match state.host.run_wave() {
        Ok(()) => {
            state.info("Wave spawned.");
            state.drain_host_events();
            // The host half ran locally; tell the networked match too
            // (`admin_run_wave`).
            if state.net.is_online()
                && let Err(error) = state.net.run_wave()
            {
                state.warn(&format!("Could not relay the wave: {error}"));
            }
        }
        Err(error) => state.error(&error.to_string()),
    }
}

pub(super) fn cmd_loadautosave(state: &mut ServerState) {
    let saves = crate::server::autosave::rotation_order(&state.paths.saves());
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

pub(super) fn cmd_load(state: &mut ServerState, args: &[&str]) {
    let Some(slot) = args.first().and_then(|value| value.parse::<u32>().ok()) else {
        state.warn("Usage: load <slot>");
        return;
    };
    match state.load_slot(slot) {
        Ok(path) => state.info(&format!("Loaded save {}.", path.display())),
        Err(error) => state.error(&error.to_string()),
    }
}

pub(super) fn cmd_save(state: &mut ServerState, args: &[&str]) {
    let Some(slot) = args.first().and_then(|value| value.parse::<u32>().ok()) else {
        state.warn("Usage: save <slot> [embedAssets]");
        return;
    };
    match state.save_slot(slot) {
        Ok(path) => state.info(&format!("Saved to {}.", path.display())),
        Err(error) => state.error(&error.to_string()),
    }
}

pub(super) fn cmd_saves(state: &mut ServerState) {
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

pub(super) fn cmd_gc(state: &mut ServerState) {
    state.info("gc: no-op diagnostic (Rust has no GC)");
    if let Ok(status) = std::fs::read_to_string("/proc/self/status")
        && let Some(line) = status.lines().find(|line| line.starts_with("VmRSS:"))
    {
        state.info(line.trim());
    }
}

pub(super) fn cmd_yes(state: &mut ServerState) {
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

pub(super) fn setting_to_string(value: &mind_core::io::settings::SettingValue) -> String {
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
