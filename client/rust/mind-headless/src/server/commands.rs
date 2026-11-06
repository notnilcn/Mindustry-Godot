// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server console commands (plan 22 §3.7).
//!
//! Port of the 43-command `ServerControl.registerCommands` table. Commands that
//! are local (config/help/version/rules/maps/save/load/status/…) are implemented
//! against `mind-core`/the local host; multiplayer authority commands delegate
//! to the plan-21 seam and return a clear "requires plan 21/STDB" line until that
//! plan lands (R2).

use super::admin::BanKind;
use super::host::{Admins, HostControl};
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
pub fn server_config_pushes(config: &super::config::ServerConfig) -> Vec<(&'static str, String)> {
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
fn cmd_fillitems(state: &mut ServerState, args: &[&str]) {
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
fn cmd_shuffle(state: &mut ServerState, args: &[&str]) {
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
fn cmd_nextmap(state: &mut ServerState, args: &[&str]) {
    let Some(name) = args.first().copied().filter(|name| !name.is_empty()) else {
        state.warn("Usage: nextmap <mapname...>");
        return;
    };
    state.host.rotation_mut().set_next(name);
    state.info(&format!("Next map set to '{name}'."));
}

/// `gameover` (`ServerControl.java:1234-1243`): forces the game-over path.
fn cmd_gameover(state: &mut ServerState) {
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
fn cmd_mods(state: &mut ServerState, name: Option<&str>) {
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
fn cmd_reloadassets(state: &mut ServerState) {
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
            status.restart_in.unwrap_or(0) / super::TICKS_PER_SECOND
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
        let interval = super::autosave::spacing_ticks(spacing);
        let remaining =
            interval.saturating_sub(state.autosave_clock.saturating_sub(state.autosave_last));
        state.info(&format!(
            "  Autosave in {}s ({} file(s) kept)",
            remaining.div_ceil(super::TICKS_PER_SECOND),
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

fn cmd_say(state: &mut ServerState, args: &[&str]) {
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

fn cmd_players(state: &mut ServerState) {
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

fn cmd_kick(state: &mut ServerState, args: &[&str]) {
    let Some(target) = args.first().copied() else {
        state.warn("Usage: kick <username...>");
        return;
    };
    match state.net.kick(target) {
        Ok(()) => state.info(&format!("Kicked {target}.")),
        Err(error) => state.warn(&format!("Could not kick {target}: {error}")),
    }
}

fn cmd_ban(state: &mut ServerState, args: &[&str]) {
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

fn cmd_bans(state: &mut ServerState) {
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

fn cmd_unban(state: &mut ServerState, args: &[&str], verb: &str) {
    let Some(target) = args.first().copied() else {
        state.warn("Usage: unban <ip/ID>");
        return;
    };
    match state.net.unban(target) {
        Ok(()) => state.info(&format!("{verb} {target}.")),
        Err(error) => state.warn(&error.to_string()),
    }
}

fn cmd_admin(state: &mut ServerState, args: &[&str]) {
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

fn cmd_admins(state: &mut ServerState) {
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

fn cmd_whitelist(state: &mut ServerState, args: &[&str]) {
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

fn cmd_playerlimit(state: &mut ServerState, args: &[&str]) {
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

fn cmd_ip_ban(state: &mut ServerState, name: &str, args: &[&str]) {
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

fn cmd_name_ban(state: &mut ServerState, args: &[&str]) {
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

fn cmd_search(state: &mut ServerState, args: &[&str]) {
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

fn cmd_pause(state: &mut ServerState, args: &[&str]) {
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

fn cmd_runwave(state: &mut ServerState) {
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

// ---------------------------------------------------------------------------
// Player chat commands (`NetServer.registerCommands`, `NetServer.java:347-521`)
// ---------------------------------------------------------------------------
//
// Java's server owns chat-command dispatch: `NetClient.sendChatMessage` hands
// the text to `netServer.clientCommands.handleMessage` *before* the chat-filter
// pass, so `/t`, `/a`, `/votekick`, `/vote`, `/help` and `/sync` are commands
// rather than chat. The dedicated host needs the same surface for its local
// console (`/`-prefixed lines, a documented headless extension) and as the
// reference dispatch the relay tests compare against.

/// `Vars.maxNameLength` (`Vars.java:113`).
pub const MAX_NAME_LENGTH: usize = 40;

/// Java `registerCommands`: `int commandsPerPage = 6`.
pub const HELP_COMMANDS_PER_PAGE: usize = 6;

/// Java `NetServer.voteDuration = 0.5f * 60` seconds.
pub const VOTE_DURATION_SECS: u64 = 30;

/// Java `NetServer.voteCooldown = 60 * 5` seconds.
pub const VOTE_COOLDOWN_SECS: u64 = 300;

/// Java `NetServer.kickDuration = 60 * 60` seconds.
pub const KICK_DURATION_SECS: u32 = 3_600;

/// Java `/sync`: `Time.timeSinceMillis(lastSyncTime) < 1000 * 5`.
pub const SYNC_COOLDOWN_MS: u64 = 5_000;

/// One registered chat command (`Command` in `NetServer.registerCommands`).
#[derive(Debug, Clone, Copy)]
pub struct ChatSpec {
    /// Command name without the `/` prefix.
    pub name: &'static str,
    /// Parameter hint.
    pub params: &'static str,
    /// One-line description.
    pub description: &'static str,
}

/// Registration order (`help`, `t`, `a`, `votekick`, `vote`, `sync`).
pub static CHAT_COMMANDS: &[ChatSpec] = &[
    ChatSpec {
        name: "help",
        params: "[page]",
        description: "Lists all commands.",
    },
    ChatSpec {
        name: "t",
        params: "<message...>",
        description: "Send a message only to your teammates.",
    },
    ChatSpec {
        name: "a",
        params: "<message...>",
        description: "Send a message only to admins.",
    },
    ChatSpec {
        name: "votekick",
        params: "[player] [reason...]",
        description: "Vote to kick a player with a valid reason.",
    },
    ChatSpec {
        name: "vote",
        params: "<y/n/c>",
        description: "Vote to kick the current player. Admins can cancel the voting with 'c'.",
    },
    ChatSpec {
        name: "sync",
        params: "",
        description: "Re-synchronize world state.",
    },
];

/// `/vote` choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteChoice {
    /// `y`/`yes`.
    Yes,
    /// `n`/`no`.
    No,
    /// `c` (admin cancellation).
    Cancel,
}

/// Parsed `/...` chat message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatCommand {
    /// Plain chat (no command prefix).
    None,
    /// `/help [page]`.
    Help(Option<u32>),
    /// `/t <message...>`.
    Team(String),
    /// `/a <message...>`.
    Admin(String),
    /// `/votekick [player] [reason...]`.
    Votekick {
        /// Target name/`#id`; `None` lists eligible players.
        target: Option<String>,
        /// Reason text.
        reason: String,
    },
    /// `/vote <y/n/c>`; `None` when the argument is not a valid choice.
    Vote(Option<VoteChoice>),
    /// `/sync`.
    Sync,
    /// Invalid command (`CommandHandler` error or a bad page number).
    Invalid(String),
}

/// Java `Colors`-aware name sanitizer (`NetServer.fixName`,
/// `NetServer.java:1261-1283`).
///
/// Trims, removes tabs/newlines, strips transparent (`alpha < 1`) colour tags
/// and truncates at [`MAX_NAME_LENGTH`] UTF-8 bytes.
pub fn fix_name(name: &str) -> String {
    let mut name: Vec<char> = name
        .trim()
        .chars()
        .filter(|c| *c != '\n' && *c != '\t')
        .collect();
    if name.len() == 1 && (name[0] == '[' || name[0] == ']') {
        return String::new();
    }
    let mut index = 0;
    while index < name.len() {
        if name[index] == '['
            && index + 1 < name.len()
            && name[index + 1] != '['
            && (index == 0 || name[index - 1] != '[')
        {
            let tail: String = name[index..].iter().collect();
            let prefix: String = name[..index].iter().collect();
            let checked = check_color(&tail);
            name = format!("{prefix}{checked}").chars().collect();
        }
        index += 1;
    }
    let mut out = String::new();
    for c in name {
        if out.len() >= MAX_NAME_LENGTH {
            break;
        }
        out.push(c);
    }
    out
}

/// `NetServer.checkColor`: if `str` starts a colour tag whose colour is
/// transparent (`alpha < 1`), return the text after the tag; otherwise return
/// `str` unchanged (`NetServer.java:1285-1309`).
pub fn check_color(str: &str) -> &str {
    let bytes = str.as_bytes();
    let mut index = 1;
    while index < bytes.len() {
        if bytes[index] == b']' {
            let color = &str[1..index];
            if let Some(alpha) = color_alpha(color) {
                if alpha < 255 {
                    return &str[index + 1..];
                }
            } else {
                // `Color.valueOf` threw for a non-hex name: keep the tag.
                return str;
            }
        }
        index += 1;
    }
    str
}

/// Alpha of a colour tag body (`Colors.get(...)` names or `Color.valueOf` hex).
///
/// `None` means `Color.valueOf` would throw (unknown non-hex token). Every
/// named Arc colour except `clear` is opaque, so only the transparent cases
/// (`clear`, 8-digit hex with alpha < 0xff) change the returned name.
fn color_alpha(color: &str) -> Option<u8> {
    if color.eq_ignore_ascii_case("clear") {
        return Some(0);
    }
    let hex = color.strip_prefix('#').unwrap_or(color);
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    if hex.len() == 8 {
        u8::from_str_radix(&hex[6..8], 16).ok()
    } else {
        Some(255)
    }
}

/// Number of votes needed (`NetServer.votesRequired`): 2, or 3 above 4 players.
pub fn votes_required(players: usize) -> i32 {
    2 + i32::from(players > 4)
}

/// Splits `argstr` exactly like Arc's `CommandHandler`: single spaces delimit
/// arguments; a variadic tail keeps its raw remainder.
fn split_args(argstr: &str) -> Vec<&str> {
    if argstr.is_empty() {
        Vec::new()
    } else {
        argstr.split(' ').collect()
    }
}

/// `CommandHandler` usage error (`invalidHandler`).
fn usage_error(name: &str, too_many: bool) -> String {
    let spec = CHAT_COMMANDS.iter().find(|spec| spec.name == name).copied();
    let params = spec.map(|spec| spec.params).unwrap_or("");
    format!(
        "[scarlet]{} arguments. Usage:[lightgray] {name}[gray] {params}",
        if too_many { "Too many" } else { "Too few" }
    )
}

/// Parses one chat message (`NetClient.sendChatMessage` ->
/// `CommandHandler.handleMessage`).
pub fn parse_chat_command(text: &str) -> ChatCommand {
    // Java checks the raw prefix: a leading space means ordinary chat.
    let Some(rest) = text.strip_prefix('/') else {
        return ChatCommand::None;
    };
    let (name, argstr) = match rest.find(' ') {
        Some(index) => (&rest[..index], &rest[index + 1..]),
        None => (rest, ""),
    };
    let name = name.to_ascii_lowercase();
    let args = split_args(argstr);
    match name.as_str() {
        "help" => {
            if args.len() > 1 {
                return ChatCommand::Invalid(usage_error("help", true));
            }
            match args.first() {
                None => ChatCommand::Help(None),
                Some(raw) => match raw.parse::<i32>() {
                    Ok(page) => ChatCommand::Help(Some(page.max(0) as u32)),
                    Err(_) => ChatCommand::Invalid("[scarlet]'page' must be a number.".to_owned()),
                },
            }
        }
        "t" => {
            if argstr.is_empty() {
                ChatCommand::Invalid(usage_error("t", false))
            } else {
                ChatCommand::Team(argstr.to_owned())
            }
        }
        "a" => {
            if argstr.is_empty() {
                ChatCommand::Invalid(usage_error("a", false))
            } else {
                ChatCommand::Admin(argstr.to_owned())
            }
        }
        "votekick" => {
            if argstr.is_empty() {
                ChatCommand::Votekick {
                    target: None,
                    reason: String::new(),
                }
            } else {
                let mut split = argstr.splitn(2, ' ');
                let target = split.next().unwrap_or_default().to_owned();
                let reason = split.next().unwrap_or_default().to_owned();
                ChatCommand::Votekick {
                    target: Some(target),
                    reason,
                }
            }
        }
        "vote" => {
            if args.is_empty() {
                ChatCommand::Invalid(usage_error("vote", false))
            } else if args.len() > 1 {
                ChatCommand::Invalid(usage_error("vote", true))
            } else {
                let choice = match args[0].to_ascii_lowercase().as_str() {
                    "y" | "yes" => Some(VoteChoice::Yes),
                    "n" | "no" => Some(VoteChoice::No),
                    "c" => Some(VoteChoice::Cancel),
                    _ => None,
                };
                ChatCommand::Vote(choice)
            }
        }
        "sync" => {
            if args.is_empty() {
                ChatCommand::Sync
            } else {
                ChatCommand::Invalid(usage_error("sync", true))
            }
        }
        _ => {
            let suggestion = console::suggest(&name, CHAT_COMMANDS.iter().map(|spec| spec.name));
            match suggestion {
                Some(found) => ChatCommand::Invalid(format!(
                    "[scarlet]Unknown command. Did you mean \"[lightgray]{found}[]\"?"
                )),
                None => ChatCommand::Invalid(
                    "[scarlet]Unknown command. Check [lightgray]/help[scarlet].".to_owned(),
                ),
            }
        }
    }
}

/// Renders one `/help` page (Java's `help` command body).
///
/// `page` is 1-based; Java's out-of-range message is returned as `Err`.
pub fn help_page(page: u32) -> Result<String, String> {
    let pages = CHAT_COMMANDS.len().div_ceil(HELP_COMMANDS_PER_PAGE).max(1) as u32;
    if page == 0 || page > pages {
        return Err(format!(
            "[scarlet]'page' must be a number between[orange] 1[] and[orange] {pages}[scarlet]."
        ));
    }
    let start = ((page - 1) as usize) * HELP_COMMANDS_PER_PAGE;
    let end = (start + HELP_COMMANDS_PER_PAGE).min(CHAT_COMMANDS.len());
    let mut out = format!(
        "[orange]-- Commands Page[lightgray] {page}[gray]/[lightgray]{pages}[orange] --\n\n"
    );
    for spec in &CHAT_COMMANDS[start..end] {
        out.push_str(&format!(
            "[orange] /{}[white] {}[lightgray] - {}\n",
            spec.name, spec.params, spec.description
        ));
    }
    Ok(out)
}

/// The dedicated server config gates the chat dispatcher enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatConfig {
    /// `Config.enableVotekick` (`/votekick` gate).
    pub enable_votekick: bool,
    /// `Config.interactRateWindow`, seconds.
    pub interact_rate_window_secs: u32,
    /// `Config.interactRateLimit`.
    pub interact_rate_limit: u32,
    /// `Config.interactRateKick`.
    pub interact_rate_kick: u32,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            enable_votekick: true,
            interact_rate_window_secs: 6,
            interact_rate_limit: 25,
            interact_rate_kick: 60,
        }
    }
}

impl ChatConfig {
    /// Reads the Java config gates out of the server settings.
    pub fn from_server(config: &super::config::ServerConfig) -> Self {
        Self {
            enable_votekick: config.get_bool("enableVotekick"),
            interact_rate_window_secs: config.get_i32("interactRateWindow").max(0) as u32,
            interact_rate_limit: config.get_i32("interactRateLimit").max(0) as u32,
            interact_rate_kick: config.get_i32("interactRateKick").max(0) as u32,
        }
    }
}

/// One server-side player row for chat dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatPlayer {
    /// Server player id (`Player.id`; 1-based within the roster).
    pub id: u32,
    /// Identity hex.
    pub identity: String,
    /// Sanitized display name.
    pub name: String,
    /// Team id.
    pub team: Option<u8>,
    /// Admin flag.
    pub admin: bool,
    /// Host-local player (`Player.isLocal`).
    pub local: bool,
    /// Connection state (`p.con != null`).
    pub connected: bool,
}

/// One action the caller delivers to the match/console.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatAction {
    /// Private message to the sender (`player.sendMessage`).
    Reply(String),
    /// Everyone (`Call.sendMessage`).
    Broadcast {
        /// Server-formatted line.
        text: String,
        /// Raw message (filters/echo).
        message: String,
    },
    /// The sender's team.
    Team {
        /// Team id.
        team: u8,
        /// Server-formatted line.
        text: String,
        /// Raw message.
        message: String,
    },
    /// Admins only.
    Admin {
        /// Server-formatted line.
        text: String,
        /// Raw message.
        message: String,
    },
    /// Kick an identity (`Player.kick`).
    Kick {
        /// Identity hex.
        identity: String,
        /// Kick reason.
        reason: String,
    },
    /// World resync request (`netServer.sendWorldData`).
    Sync {
        /// Requester identity hex.
        identity: String,
    },
}

/// Result of the interaction-rate gate (`interactRate*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractVerdict {
    /// Within the limit.
    Allow,
    /// Over the limit: drop the action (`Java` filters the packet).
    Limited,
    /// Over the kick threshold: kick (`KickReason.kick`).
    Kick,
}

/// An active votekick (`NetServer.VoteSession`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoteSession {
    /// Target identity hex.
    pub target_identity: String,
    /// Target display name.
    pub target_name: String,
    /// Target team.
    pub target_team: Option<u8>,
    /// Reason text.
    pub reason: String,
    /// Votes by identity.
    pub votes: std::collections::BTreeMap<String, i32>,
    /// Running total.
    pub count: i32,
    /// Expiry in engine milliseconds.
    pub ends_at_ms: u64,
}

/// Server-side chat command state: votekick session, cooldowns, `/sync` gate
/// and the interaction-rate window.
#[derive(Debug, Default)]
pub struct ChatEngine {
    config: ChatConfig,
    vote: Option<VoteSession>,
    cooldowns: std::collections::BTreeMap<String, u64>,
    last_sync: std::collections::BTreeMap<String, u64>,
    interact: std::collections::BTreeMap<String, (u64, u32)>,
    pending: Vec<ChatAction>,
    now_ms: u64,
}

impl ChatEngine {
    /// Engine with the default config gates.
    pub fn new() -> Self {
        Self {
            config: ChatConfig::default(),
            ..Self::default()
        }
    }

    /// Replaces the config gates.
    pub fn set_config(&mut self, config: ChatConfig) {
        self.config = config;
    }

    /// The active config.
    pub fn config(&self) -> ChatConfig {
        self.config
    }

    /// Engine clock in milliseconds.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// Advances the engine clock by `ticks` fixed steps and expires the vote
    /// session (`Timer.schedule(..., voteDuration)`).
    pub fn tick(&mut self, ticks: u64) {
        self.now_ms = self
            .now_ms
            .saturating_add(ticks.saturating_mul(1_000) / super::TICKS_PER_SECOND);
        let now = self.now_ms;
        if let Some(vote) = &self.vote
            && vote.ends_at_ms <= now
        {
            let target = vote.target_name.clone();
            self.vote = None;
            self.pending.push(ChatAction::Broadcast {
                text: format!(
                    "[lightgray]Vote failed. Not enough votes to kick[orange] {target}[lightgray]."
                ),
                message: String::new(),
            });
        }
    }

    /// Takes the actions produced by [`Self::tick`].
    pub fn take_pending(&mut self) -> Vec<ChatAction> {
        std::mem::take(&mut self.pending)
    }

    /// The running vote, if any (`currentlyKicking`).
    pub fn vote(&self) -> Option<&VoteSession> {
        self.vote.as_ref()
    }

    /// `interactRateWindow`/`interactRateLimit`/`interactRateKick` gate.
    pub fn interact_check(&mut self, identity: &str, now_ms: u64) -> InteractVerdict {
        let limit = self.config.interact_rate_limit;
        let kick = self.config.interact_rate_kick;
        if limit == 0 && kick == 0 {
            return InteractVerdict::Allow;
        }
        let window_ms = u64::from(self.config.interact_rate_window_secs.max(1)) * 1_000;
        let entry = self
            .interact
            .entry(identity.to_owned())
            .or_insert((now_ms, 0));
        if now_ms.saturating_sub(entry.0) >= window_ms {
            *entry = (now_ms, 0);
        }
        entry.1 = entry.1.saturating_add(1);
        if kick > 0 && entry.1 >= kick {
            InteractVerdict::Kick
        } else if limit > 0 && entry.1 > limit {
            InteractVerdict::Limited
        } else {
            InteractVerdict::Allow
        }
    }

    /// Dispatches one player chat line (`clientCommands.handleMessage`).
    ///
    /// Returns the actions the caller must deliver; `[]` for plain chat or a
    /// rate-limited command.
    pub fn dispatch(
        &mut self,
        players: &[ChatPlayer],
        player: &ChatPlayer,
        text: &str,
    ) -> Vec<ChatAction> {
        let command = parse_chat_command(text);
        if matches!(command, ChatCommand::None) {
            return Vec::new();
        }
        let now = self.now_ms;
        if !player.local {
            match self.interact_check(&player.identity, now) {
                InteractVerdict::Kick => {
                    return vec![ChatAction::Kick {
                        identity: player.identity.clone(),
                        reason: "interaction rate limit".to_owned(),
                    }];
                }
                InteractVerdict::Limited => return Vec::new(),
                InteractVerdict::Allow => {}
            }
        }
        let reply = |text: String| vec![ChatAction::Reply(text)];
        match command {
            ChatCommand::None => Vec::new(),
            ChatCommand::Invalid(message) => reply(message),
            ChatCommand::Help(page) => {
                let page = page.unwrap_or(1);
                match help_page(page) {
                    Ok(body) => reply(body),
                    Err(message) => reply(message),
                }
            }
            ChatCommand::Team(message) => {
                let Some(team) = player.team else {
                    // Java players always have a team; the virtual console
                    // player does not (headless-only path).
                    return reply("[scarlet]You are not on a team.".to_owned());
                };
                let text = format!("<T> {}: {message}", player.name);
                vec![ChatAction::Team {
                    team,
                    text,
                    message,
                }]
            }
            ChatCommand::Admin(message) => {
                if !player.admin {
                    return reply("[scarlet]You must be an admin to use this command.".to_owned());
                }
                let text = format!("<A> {}: {message}", player.name);
                vec![ChatAction::Admin { text, message }]
            }
            ChatCommand::Votekick { target, reason } => {
                self.dispatch_votekick(players, player, target, reason, now)
            }
            ChatCommand::Vote(choice) => self.dispatch_vote(players, player, choice, text, now),
            ChatCommand::Sync => self.dispatch_sync(player, now),
        }
    }

    fn dispatch_votekick(
        &mut self,
        players: &[ChatPlayer],
        player: &ChatPlayer,
        target: Option<String>,
        reason: String,
        now: u64,
    ) -> Vec<ChatAction> {
        let reply = |text: &str| vec![ChatAction::Reply(text.to_owned())];
        if !self.config.enable_votekick {
            return reply("[scarlet]Vote-kick is disabled on this server.");
        }
        if players.len() < 3 {
            return reply("[scarlet]At least 3 players are needed to start a votekick.");
        }
        if player.local {
            return reply("[scarlet]Just kick them yourself if you're the host.");
        }
        if self.vote.is_some() {
            return reply("[scarlet]A vote is already in progress.");
        }
        let Some(target) = target else {
            let mut out = String::from("[orange]Players to kick: \n");
            for entry in players.iter().filter(|entry| {
                !entry.admin && entry.connected && !entry.local && entry.identity != player.identity
            }) {
                out.push_str(&format!(
                    "[lightgray] {}[accent] (#{})\n",
                    entry.name, entry.id
                ));
            }
            return vec![ChatAction::Reply(out)];
        };
        if reason.is_empty() {
            return reply(
                "[orange]You need a valid reason to kick the player. Add a reason after the player name.",
            );
        }
        let found = players.iter().find(|entry| {
            if let Some(id) = target
                .strip_prefix('#')
                .and_then(|id| id.parse::<u32>().ok())
            {
                entry.id == id
            } else {
                entry.name.eq_ignore_ascii_case(&target)
            }
        });
        let Some(found) = found else {
            return reply(&format!(
                "[scarlet]No player [orange]'{target}'[scarlet] found."
            ));
        };
        if found.identity == player.identity {
            return reply("[scarlet]You can't vote to kick yourself.");
        }
        if found.admin {
            return reply("[scarlet]Did you really expect to be able to kick an admin?");
        }
        if found.local {
            return reply("[scarlet]Local players cannot be kicked.");
        }
        if found.team != player.team {
            return reply("[scarlet]Only players on your team can be kicked.");
        }
        if let Some(ready) = self.cooldowns.get(&player.identity)
            && now < *ready
        {
            return reply(&format!(
                "[scarlet]You must wait {} minutes between votekicks.",
                VOTE_COOLDOWN_SECS / 60
            ));
        }
        self.cooldowns
            .insert(player.identity.clone(), now + VOTE_COOLDOWN_SECS * 1_000);
        let mut actions = vec![ChatAction::Broadcast {
            text: format!("[lightgray]Reason:[orange] {reason}[lightgray]."),
            message: reason.clone(),
        }];
        self.vote = Some(VoteSession {
            target_identity: found.identity.clone(),
            target_name: found.name.clone(),
            target_team: found.team,
            reason,
            votes: std::collections::BTreeMap::new(),
            count: 0,
            ends_at_ms: now + VOTE_DURATION_SECS * 1_000,
        });
        actions.extend(self.cast_vote_raw(players.len(), player, 1));
        actions
    }

    fn dispatch_vote(
        &mut self,
        players: &[ChatPlayer],
        player: &ChatPlayer,
        choice: Option<VoteChoice>,
        raw: &str,
        now: u64,
    ) -> Vec<ChatAction> {
        let reply = |text: &str| vec![ChatAction::Reply(text.to_owned())];
        if self.vote.is_none() {
            return reply("[scarlet]Nobody is being voted on.");
        }
        if player.admin && choice == Some(VoteChoice::Cancel) {
            self.vote = None;
            return vec![ChatAction::Broadcast {
                text: format!(
                    "[lightgray]Vote canceled by admin[orange] {}[lightgray].",
                    player.name
                ),
                message: String::new(),
            }];
        }
        if player.local {
            return reply("[scarlet]Local players can't vote. Kick the player yourself instead.");
        }
        let sign = match choice {
            Some(VoteChoice::Yes) => 1,
            Some(VoteChoice::No) => -1,
            _ => 0,
        };
        let (target_identity, target_team, already) = match self.vote.as_ref() {
            Some(vote) => (
                vote.target_identity.clone(),
                vote.target_team,
                vote.votes.get(&player.identity).copied(),
            ),
            None => return reply("[scarlet]Nobody is being voted on."),
        };
        let arg = raw
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .to_ascii_lowercase();
        if already == Some(sign) {
            return reply(&format!("[scarlet]You've already voted {arg}. Sit down."));
        }
        if target_identity == player.identity {
            return reply("[scarlet]You can't vote on your own trial.");
        }
        if target_team != player.team {
            return reply("[scarlet]You can't vote for other teams.");
        }
        if sign == 0 {
            return reply("[scarlet]Vote either 'y' (yes) or 'n' (no).");
        }
        let _ = now;
        self.cast_vote_raw(players.len(), player, sign)
    }

    fn cast_vote_raw(
        &mut self,
        players_len: usize,
        player: &ChatPlayer,
        sign: i32,
    ) -> Vec<ChatAction> {
        let required = votes_required(players_len);
        let Some(vote) = self.vote.as_mut() else {
            return Vec::new();
        };
        let last = vote.votes.get(&player.identity).copied().unwrap_or(0);
        vote.count -= last;
        vote.count += sign;
        vote.votes.insert(player.identity.clone(), sign);
        let target_name = vote.target_name.clone();
        let target_identity = vote.target_identity.clone();
        let count = vote.count;
        let mut actions = vec![ChatAction::Broadcast {
            text: format!(
                "[lightgray]{}[lightgray] has voted on kicking[orange] {target_name}[lightgray].[accent] ({count}/{required})\n[lightgray]Type[orange] /vote <y/n>[] to agree.",
                player.name
            ),
            message: String::new(),
        }];
        if count >= required {
            actions.push(ChatAction::Broadcast {
                text: format!(
                    "[orange]Vote passed.[scarlet] {target_name}[orange] will be banned from the server for {} minutes.",
                    KICK_DURATION_SECS / 60
                ),
                message: String::new(),
            });
            actions.push(ChatAction::Kick {
                identity: target_identity,
                reason: "votekick".to_owned(),
            });
            self.vote = None;
        }
        actions
    }

    fn dispatch_sync(&mut self, player: &ChatPlayer, now: u64) -> Vec<ChatAction> {
        if player.local {
            return vec![ChatAction::Reply(
                "[scarlet]Re-synchronizing as the host is pointless.".to_owned(),
            )];
        }
        if let Some(last) = self.last_sync.get(&player.identity)
            && now.saturating_sub(*last) < SYNC_COOLDOWN_MS
        {
            return vec![ChatAction::Reply(
                "[scarlet]You may only /sync every 5 seconds.".to_owned(),
            )];
        }
        self.last_sync.insert(player.identity.clone(), now);
        vec![ChatAction::Sync {
            identity: player.identity.clone(),
        }]
    }
}

/// Applies one batch of chat actions to the dedicated host: console replies
/// are logged, broadcasts go through the match's System channel, team/admin
/// messages use their channel and kicks go through the host admin seam.
///
/// A private reply to a *networked* player would need a targeted reducer row
/// (`MatchChat.target`); the module currently emits only match-wide rows, so a
/// future `NetHost::reply` hook is the remaining piece (reported, not edited).
pub fn apply_chat_actions(state: &mut ServerState, actions: Vec<ChatAction>) {
    use mind_stdb::module_bindings::ChatKind;
    for action in actions {
        match action {
            ChatAction::Reply(text) => state.info(&text),
            ChatAction::Broadcast { text, .. } => state.broadcast(&text),
            ChatAction::Team { text, message, .. } => {
                if state.net.is_online()
                    && let Err(error) = state.net.say_kind(ChatKind::Team, &message)
                {
                    state.warn(&format!("could not send team message: {error}"));
                }
                state.info(&text);
            }
            ChatAction::Admin { text, message } => {
                if state.net.is_online()
                    && let Err(error) = state.net.say_kind(ChatKind::Admin, &message)
                {
                    state.warn(&format!("could not send admin message: {error}"));
                }
                state.info(&text);
            }
            ChatAction::Kick { identity, reason } => match state.net.kick(&identity) {
                Ok(()) => state.info(&format!("Kicked {identity} ({reason}).")),
                Err(error) => state.warn(&format!("Could not kick {identity}: {error}")),
            },
            ChatAction::Sync { identity } => {
                state.warn(&format!(
                    "World resync requested by {identity}: the headless host has no resend path yet (module hook needed)."
                ));
            }
        }
    }
}

/// Advances the chat engine by `ticks` and delivers any timeout notices.
pub fn tick_chat(state: &mut ServerState, ticks: u64) {
    state.chat.tick(ticks);
    let pending = state.chat.take_pending();
    apply_chat_actions(state, pending);
}

impl ServerState {
    /// Builds the chat roster from the live match (`Groups.player`).
    pub fn chat_players(&self) -> Vec<ChatPlayer> {
        self.net
            .players()
            .into_iter()
            .enumerate()
            .map(|(index, player)| ChatPlayer {
                id: (index + 1) as u32,
                identity: player.identity,
                name: fix_name(&player.name),
                team: player.team,
                admin: player.admin,
                local: false,
                connected: player.connected,
            })
            .collect()
    }

    /// Dispatches one `/...` line from the dedicated server console as a
    /// virtual admin player (`local = true`, like the host player upstream).
    ///
    /// This is the headless ingress that keeps the player command surface
    /// reachable without a client: Java's console has no player identity, so a
    /// networked ingress would call `ChatEngine::dispatch` with the real sender
    /// row instead (reported hook).
    pub fn handle_chat_console(&mut self, line: &str) {
        self.chat.set_config(ChatConfig::from_server(&self.config));
        let players = self.chat_players();
        let name = fix_name(&self.config.get_string("name"));
        let console = ChatPlayer {
            id: 0,
            identity: "console".to_owned(),
            name: if name.is_empty() {
                "server".to_owned()
            } else {
                name
            },
            team: None,
            admin: true,
            local: true,
            connected: true,
        };
        let actions = self.chat.dispatch(&players, &console, line);
        apply_chat_actions(self, actions);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::{ServerOptions, TICKS_PER_SECOND, run};

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mind-srv-cmd-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create");
        dir
    }

    fn player(id: u32, name: &str, team: Option<u8>) -> ChatPlayer {
        ChatPlayer {
            id,
            identity: format!("{id:064x}"),
            name: name.to_owned(),
            team,
            admin: false,
            local: false,
            connected: true,
        }
    }

    fn roster(count: usize, team: Option<u8>) -> Vec<ChatPlayer> {
        (1..=count)
            .map(|id| player(id as u32, &format!("p{id}"), team))
            .collect()
    }

    fn engine_with(enable_votekick: bool) -> ChatEngine {
        let mut engine = ChatEngine::new();
        engine.set_config(ChatConfig {
            enable_votekick,
            ..ChatConfig::default()
        });
        engine
    }

    fn replies(actions: &[ChatAction]) -> Vec<&str> {
        actions
            .iter()
            .filter_map(|action| match action {
                ChatAction::Reply(text) | ChatAction::Broadcast { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn chat_parser_matches_the_java_grammar() {
        assert_eq!(parse_chat_command("hello"), ChatCommand::None);
        assert_eq!(parse_chat_command(" /hello"), ChatCommand::None);
        assert_eq!(
            parse_chat_command("/t push mid"),
            ChatCommand::Team("push mid".to_owned())
        );
        assert!(matches!(
            parse_chat_command("/t"),
            ChatCommand::Invalid(ref message) if message.contains("Too few arguments. Usage:[lightgray] t[gray] <message...>")
        ));
        assert_eq!(
            parse_chat_command("/A admins only"),
            ChatCommand::Admin("admins only".to_owned())
        );
        assert_eq!(
            parse_chat_command("/votekick"),
            ChatCommand::Votekick {
                target: None,
                reason: String::new()
            }
        );
        assert_eq!(
            parse_chat_command("/votekick Bob griefing the core"),
            ChatCommand::Votekick {
                target: Some("Bob".to_owned()),
                reason: "griefing the core".to_owned()
            }
        );
        assert_eq!(
            parse_chat_command("/votekick #2 lag"),
            ChatCommand::Votekick {
                target: Some("#2".to_owned()),
                reason: "lag".to_owned()
            }
        );
        assert_eq!(
            parse_chat_command("/vote YES"),
            ChatCommand::Vote(Some(VoteChoice::Yes))
        );
        assert_eq!(
            parse_chat_command("/vote n"),
            ChatCommand::Vote(Some(VoteChoice::No))
        );
        assert_eq!(
            parse_chat_command("/vote c"),
            ChatCommand::Vote(Some(VoteChoice::Cancel))
        );
        assert_eq!(parse_chat_command("/vote maybe"), ChatCommand::Vote(None));
        assert!(matches!(
            parse_chat_command("/vote"),
            ChatCommand::Invalid(ref message) if message.contains("Too few arguments")
        ));
        assert!(matches!(
            parse_chat_command("/vote y n"),
            ChatCommand::Invalid(ref message) if message.contains("Too many arguments")
        ));
        assert_eq!(parse_chat_command("/help"), ChatCommand::Help(None));
        assert_eq!(parse_chat_command("/help 1"), ChatCommand::Help(Some(1)));
        assert!(matches!(
            parse_chat_command("/help abc"),
            ChatCommand::Invalid(ref message) if message == "[scarlet]'page' must be a number."
        ));
        assert!(matches!(
            parse_chat_command("/help 1 2"),
            ChatCommand::Invalid(ref message) if message.contains("Too many arguments")
        ));
        assert_eq!(parse_chat_command("/sync"), ChatCommand::Sync);
        assert!(matches!(
            parse_chat_command("/sync now"),
            ChatCommand::Invalid(ref message) if message.contains("Too many arguments")
        ));
        assert!(matches!(
            parse_chat_command("/hel"),
            ChatCommand::Invalid(ref message) if message.contains("Did you mean \"[lightgray]help[]\"?")
        ));
        assert!(matches!(
            parse_chat_command("/zzzzzz"),
            ChatCommand::Invalid(ref message) if message.contains("Check [lightgray]/help")
        ));
    }

    #[test]
    fn fix_name_and_check_color_follow_net_server() {
        assert_eq!(fix_name("  Bob  "), "Bob");
        assert_eq!(fix_name("A\nB\tC"), "ABC");
        assert_eq!(fix_name("["), "");
        assert_eq!(fix_name("]"), "");
        assert_eq!(fix_name("A[clear]B"), "AB");
        assert_eq!(fix_name("A[#00000000]B"), "AB");
        assert_eq!(fix_name("A[CLEAR]B"), "AB");
        assert_eq!(fix_name("[red]Bob"), "[red]Bob");
        assert_eq!(fix_name("[#ff0000]Bob"), "[#ff0000]Bob");
        assert_eq!(fix_name("[[red]Bob"), "[[red]Bob");
        // Java caps at 40 UTF-8 bytes (`Vars.maxNameLength`).
        assert_eq!(fix_name(&"a".repeat(45)), "a".repeat(40));
        assert_eq!(check_color("[clear]tail"), "tail");
        assert_eq!(check_color("[#000000ff]tail"), "[#000000ff]tail");
        assert_eq!(check_color("[notacolor]tail"), "[notacolor]tail");
    }

    #[test]
    fn help_pages_match_java() {
        let page = help_page(1).expect("page 1");
        assert!(page.contains("Commands Page[lightgray] 1[gray]/[lightgray]1"));
        for spec in CHAT_COMMANDS {
            assert!(
                page.contains(spec.name),
                "help page missing {}: {page}",
                spec.name
            );
        }
        let error = help_page(2).expect_err("page 2");
        assert!(error.contains("between[orange] 1[] and[orange] 1[scarlet]"));
        assert!(help_page(0).is_err());
    }

    #[test]
    fn votekick_respects_the_enable_gate_and_player_count() {
        let players = roster(4, Some(0));
        let voter = players[0].clone();
        let mut engine = engine_with(false);
        let actions = engine.dispatch(&players, &voter, "/votekick p2 griefing");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]Vote-kick is disabled on this server."]
        );
        assert!(engine.vote().is_none());

        let mut engine = engine_with(true);
        let two = roster(2, Some(0));
        let actions = engine.dispatch(&two, &two[0], "/votekick p2 griefing");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]At least 3 players are needed to start a votekick."]
        );

        let actions = engine.dispatch(&two, &two[0], "/votekick");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]At least 3 players are needed to start a votekick."]
        );
    }

    #[test]
    fn votekick_lists_players_and_validates_the_target() {
        let mut players = roster(4, Some(0));
        let voter = players[0].clone();
        let mut engine = engine_with(true);
        let actions = engine.dispatch(&players, &voter, "/votekick");
        let list = replies(&actions);
        assert_eq!(list.len(), 1);
        assert!(list[0].contains("Players to kick:"));
        assert!(list[0].contains("p2"), "list: {}", list[0]);
        assert!(list[0].contains("p4"), "list: {}", list[0]);

        let actions = engine.dispatch(&players, &voter, "/votekick p2");
        assert_eq!(
            replies(&actions),
            vec![
                "[orange]You need a valid reason to kick the player. Add a reason after the player name."
            ]
        );
        let actions = engine.dispatch(&players, &voter, "/votekick nobody reason");
        assert!(replies(&actions)[0].contains("No player [orange]'nobody'[scarlet] found."));
        let actions = engine.dispatch(&players, &voter, "/votekick p1 reason");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]You can't vote to kick yourself."]
        );
        let mut admin = players[1].clone();
        admin.admin = true;
        players[1] = admin;
        let actions = engine.dispatch(&players, &voter, "/votekick p2 reason");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]Did you really expect to be able to kick an admin?"]
        );
        let mut other = players[2].clone();
        other.team = Some(1);
        players[2] = other;
        let actions = engine.dispatch(&players, &voter, "/votekick p3 reason");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]Only players on your team can be kicked."]
        );
    }

    #[test]
    fn votekick_session_counts_votes_and_passes_at_threshold() {
        let players = roster(5, Some(0));
        let voter = players[0].clone();
        let mut engine = engine_with(true);
        let actions = engine.dispatch(&players, &voter, "/votekick p2 griefing");
        let texts = replies(&actions);
        assert!(texts[0].contains("Reason:[orange] griefing"));
        assert!(texts[1].contains("(1/3)"), "tally: {}", texts[1]);
        assert!(engine.vote().is_some());
        // Same-direction repeat is rejected.
        let actions = engine.dispatch(&players, &voter, "/vote y");
        assert!(replies(&actions)[0].contains("already voted y"));
        // Self/other-team/invalid choices.
        let target = &players[1];
        let actions = engine.dispatch(&players, target, "/vote y");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]You can't vote on your own trial."]
        );
        let actions = engine.dispatch(&players, &players[2], "/vote maybe");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]Vote either 'y' (yes) or 'n' (no)."]
        );
        // Two more yes votes reach the 3-vote threshold for 5 players.
        let actions = engine.dispatch(&players, &players[2], "/vote y");
        assert!(replies(&actions)[0].contains("(2/3)"));
        assert!(engine.vote().is_some());
        let actions = engine.dispatch(&players, &players[3], "/vote y");
        let texts = replies(&actions);
        assert!(texts[0].contains("(3/3)"), "tally: {}", texts[0]);
        assert!(texts[1].contains("Vote passed"));
        assert_eq!(
            actions.iter().find_map(|action| match action {
                ChatAction::Kick { identity, .. } => Some(identity.clone()),
                _ => None,
            }),
            Some(players[1].identity.clone())
        );
        assert!(engine.vote().is_none());
    }

    #[test]
    fn vote_admin_cancel_and_expiry() {
        let players = roster(4, Some(0));
        let mut engine = engine_with(true);
        engine.dispatch(&players, &players[0], "/votekick p2 reason");
        let mut admin = players[3].clone();
        admin.admin = true;
        let actions = engine.dispatch(&players, &admin, "/vote c");
        assert!(replies(&actions)[0].contains("Vote canceled by admin[orange] p4"));
        assert!(engine.vote().is_none());
        let actions = engine.dispatch(&players, &players[0], "/vote y");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]Nobody is being voted on."]
        );

        // A fresh session expires after `VOTE_DURATION_SECS` (past the
        // five-minute cooldown the cancelled vote started).
        engine.tick(VOTE_COOLDOWN_SECS * TICKS_PER_SECOND);
        engine.dispatch(&players, &players[0], "/votekick p2 reason");
        assert!(engine.vote().is_some());
        engine.tick(VOTE_DURATION_SECS * TICKS_PER_SECOND);
        let pending = engine.take_pending();
        assert_eq!(
            replies(&pending),
            vec!["[lightgray]Vote failed. Not enough votes to kick[orange] p2[lightgray]."]
        );
        assert!(engine.vote().is_none());
    }

    #[test]
    fn votekick_cooldown_is_five_minutes() {
        let players = roster(4, Some(0));
        let mut engine = engine_with(true);
        engine.dispatch(&players, &players[0], "/votekick p2 first");
        engine.dispatch(&players, &players[1], "/vote n");
        engine.tick(VOTE_DURATION_SECS * TICKS_PER_SECOND);
        let actions = engine.dispatch(&players, &players[0], "/votekick p3 second");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]You must wait 5 minutes between votekicks."]
        );
    }

    #[test]
    fn local_players_cannot_vote_or_sync() {
        let players = roster(4, Some(0));
        let mut local = players[0].clone();
        local.local = true;
        let mut engine = engine_with(true);
        let actions = engine.dispatch(&players, &local, "/votekick p2 reason");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]Just kick them yourself if you're the host."]
        );
        let actions = engine.dispatch(&players, &local, "/sync");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]Re-synchronizing as the host is pointless."]
        );
        // A local player cannot vote either (session started by p1).
        engine.dispatch(&players, &players[0], "/votekick p2 reason");
        let actions = engine.dispatch(&players, &local, "/vote y");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]Local players can't vote. Kick the player yourself instead."]
        );
    }

    #[test]
    fn sync_gate_allows_once_per_five_seconds() {
        let players = roster(3, Some(0));
        let mut engine = engine_with(true);
        let actions = engine.dispatch(&players, &players[0], "/sync");
        assert!(matches!(actions.as_slice(), [ChatAction::Sync { .. }]));
        let actions = engine.dispatch(&players, &players[0], "/sync");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]You may only /sync every 5 seconds."]
        );
        engine.tick(5 * TICKS_PER_SECOND);
        let actions = engine.dispatch(&players, &players[0], "/sync");
        assert!(matches!(actions.as_slice(), [ChatAction::Sync { .. }]));
    }

    #[test]
    fn interact_rate_gate_honours_window_limit_and_kick() {
        let mut engine = ChatEngine::new();
        engine.set_config(ChatConfig {
            interact_rate_window_secs: 1,
            interact_rate_limit: 2,
            interact_rate_kick: 4,
            ..ChatConfig::default()
        });
        assert_eq!(engine.interact_check("id", 0), InteractVerdict::Allow);
        assert_eq!(engine.interact_check("id", 100), InteractVerdict::Allow);
        assert_eq!(engine.interact_check("id", 200), InteractVerdict::Limited);
        assert_eq!(engine.interact_check("id", 300), InteractVerdict::Kick);
        // A new window resets both counters.
        assert_eq!(engine.interact_check("id", 1_300), InteractVerdict::Allow);
        // Zero limits disable the gate.
        engine.set_config(ChatConfig {
            interact_rate_limit: 0,
            interact_rate_kick: 0,
            ..ChatConfig::default()
        });
        for _ in 0..100 {
            assert_eq!(engine.interact_check("id", 1_400), InteractVerdict::Allow);
        }
    }

    #[test]
    fn team_and_admin_channels_validate_the_sender() {
        let players = roster(3, Some(2));
        let mut engine = engine_with(true);
        let mut admin = players[0].clone();
        admin.admin = true;
        let actions = engine.dispatch(&players, &admin, "/a incident at core");
        assert_eq!(
            actions,
            vec![ChatAction::Admin {
                text: "<A> p1: incident at core".to_owned(),
                message: "incident at core".to_owned()
            }]
        );
        let actions = engine.dispatch(&players, &players[1], "/a nope");
        assert_eq!(
            replies(&actions),
            vec!["[scarlet]You must be an admin to use this command."]
        );
        let actions = engine.dispatch(&players, &players[1], "/t push left");
        assert_eq!(
            actions,
            vec![ChatAction::Team {
                team: 2,
                text: "<T> p2: push left".to_owned(),
                message: "push left".to_owned()
            }]
        );
        let mut no_team = players[1].clone();
        no_team.team = None;
        let actions = engine.dispatch(&players, &no_team, "/t hello");
        assert_eq!(replies(&actions), vec!["[scarlet]You are not on a team."]);
    }

    #[test]
    fn server_config_pushes_cover_the_module_gates() {
        let dir = temp_dir("pushes");
        let config = super::super::config::ServerConfig::load(&dir, &mind_core::io::fs::NativeFs);
        let pushes = server_config_pushes(&config);
        let field = |name: &str| {
            pushes
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.clone())
        };
        assert_eq!(field("enable_votekick").as_deref(), Some("true"));
        assert_eq!(field("interact_rate_window_secs").as_deref(), Some("6"));
        assert_eq!(field("interact_rate_limit").as_deref(), Some("25"));
        assert_eq!(field("interact_rate_kick").as_deref(), Some("60"));
        assert_eq!(field("whitelist_enabled").as_deref(), Some("false"));
        assert_eq!(field("auto_pause").as_deref(), Some("false"));
        assert_eq!(pushes.len(), 15);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn console_slash_lines_dispatch_through_the_server() {
        let dir = temp_dir("consolechat");
        let options = crate::server::ServerOptions {
            config_dir: dir.clone(),
            offline: true,
            ..crate::server::ServerOptions::default()
        };
        let mut state =
            crate::server::boot_state(&options, std::time::Instant::now()).expect("boot state");
        execute(&mut state, "/help");
        execute(&mut state, "/sync");
        execute(&mut state, "/t team message");
        let log = std::fs::read_to_string(dir.join("logs").join("log-0.txt")).expect("log");
        assert!(log.contains("Commands Page"), "help output missing: {log}");
        assert!(
            log.contains("Re-synchronizing as the host is pointless."),
            "sync reply missing: {log}"
        );
        assert!(
            log.contains("[scarlet]You are not on a team."),
            "team reply missing: {log}"
        );
        let _ = std::fs::remove_dir_all(&dir);
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
    fn js_gc_and_autoupdate_commands_run_without_panicking() {
        let dir = temp_dir("jsgc");
        let options = ServerOptions {
            config_dir: dir.clone(),
            commands: vec![
                "js console.log(1)".to_owned(),
                "gc".to_owned(),
                "config autoUpdate true".to_owned(),
                "exit".to_owned(),
            ],
            ..ServerOptions::default()
        };
        assert_eq!(run(options), 0);
        let settings = std::fs::read_to_string(dir.join("settings.json")).expect("settings");
        assert!(
            settings.contains("\"autoUpdate\": true"),
            "autoUpdate not persisted: {settings}"
        );
        assert_eq!(UPDATER_NOT_AVAILABLE, "updater not available in this build");
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
