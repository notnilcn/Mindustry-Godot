// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Server console commands (plan 22 §3.7).
//!
//! Port of the 43-command `ServerControl.registerCommands` table. Commands that
//! are local (config/help/version/rules/maps/save/load/status/…) are implemented
//! against `mind-core`/the local host; multiplayer authority commands delegate
//! to the plan-21 seam and return a clear "requires plan 21/STDB" line until that
//! plan lands (R2).
//! Upstream: server/src/mindustry/server/ServerControl.java (`registerCommands`).

mod chat;
mod console;
#[cfg(test)]
mod tests;

pub use chat::{
    CHAT_COMMANDS, ChatAction, ChatCommand, ChatConfig, ChatEngine, ChatPlayer, ChatSpec,
    HELP_COMMANDS_PER_PAGE, InteractVerdict, KICK_DURATION_SECS, MAX_NAME_LENGTH, SYNC_COOLDOWN_MS,
    VOTE_COOLDOWN_SECS, VOTE_DURATION_SECS, VoteChoice, VoteSession, apply_chat_actions,
    check_color, fix_name, help_page, parse_chat_command, tick_chat, votes_required,
};
pub use console::{
    COMMANDS, CommandSpec, Outcome, UPDATER_NOT_AVAILABLE, execute, server_config_pushes,
};
