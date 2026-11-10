// SPDX-License-Identifier: GPL-3.0-only

//! Dedicated-server console and chat command tests.
//! Upstream: none (port-only CLI; the oracle has no Java counterpart).

use super::chat::*;
use super::console::*;

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
