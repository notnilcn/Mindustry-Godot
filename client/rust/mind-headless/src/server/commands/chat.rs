// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server chat commands (plan 22 §3.7).
//!
//! Name/color sanitising, the chat-command parser, the vote session and the
//! `ChatEngine` that dispatches `/t`, `/a`, `/votekick`, `/vote`, `/help` and
//! `/sync`.
//! Upstream: core/src/mindustry/core/NetServer.java (`registerCommands`).

use crate::server::host::Admins;
use crate::server::{ServerState, console};

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
pub(super) fn color_alpha(color: &str) -> Option<u8> {
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
pub(super) fn split_args(argstr: &str) -> Vec<&str> {
    if argstr.is_empty() {
        Vec::new()
    } else {
        argstr.split(' ').collect()
    }
}

/// `CommandHandler` usage error (`invalidHandler`).
pub(super) fn usage_error(name: &str, too_many: bool) -> String {
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
    pub fn from_server(config: &crate::server::config::ServerConfig) -> Self {
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
            .saturating_add(ticks.saturating_mul(1_000) / crate::server::TICKS_PER_SECOND);
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
