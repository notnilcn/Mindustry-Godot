// SPDX-License-Identifier: GPL-3.0-only

//! Client chat command parsing (GAPS #33; Java `NetServer.registerCommands`,
//! `NetServer.java:347-521`).
//!
//! Upstream parses chat text on the server: `NetClient.sendChatMessage` hands
//! the message to `netServer.clientCommands.handleMessage` *before* the chat
//! filter pass, so `/t`, `/a`, `/votekick`, `/vote`, `/help` and `/sync` are
//! commands rather than chat. The module keeps that shape: [`send_chat`] parses
//! the text first and dispatches. Pure parser so the command grammar is
//! unit-testable without a database.

use crate::main::global::VOTE_MIN_PLAYERS;

/// Commands listed per `/help` page (Java `registerCommands`: 6).
pub const HELP_COMMANDS_PER_PAGE: usize = 6;

/// One vote choice (`/vote <y/n/c>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteChoice {
    /// Vote yes.
    Yes,
    /// Vote no.
    No,
    /// Admin cancellation.
    Cancel,
}

/// Parsed `/...` chat command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatCommand {
    /// Plain chat (no command prefix).
    None,
    /// `/t <message...>` team channel.
    Team(String),
    /// `/a <message...>` admin channel.
    Admin(String),
    /// `/votekick [player] [reason...]`.
    Votekick {
        /// Target name/`#member`; `None` lists eligible players.
        target: Option<String>,
        /// Reason text (empty when the client omitted it).
        reason: String,
    },
    /// `/vote <y/n/c>`; `None` when the argument is not a valid choice.
    Vote(Option<VoteChoice>),
    /// `/help [page]`; `None` = page 1.
    Help(Option<u32>),
    /// `/sync` world-state resynchronization request.
    Sync,
    /// A malformed/unknown command; carries the private error the caller sees.
    Invalid(String),
}

/// Command metadata for `/help` (Java `Command` text/paramText/description).
pub struct HelpEntry {
    /// Command name without the prefix.
    pub name: &'static str,
    /// Parameter hint (`<message...>`, ...).
    pub params: &'static str,
    /// One-line description.
    pub description: &'static str,
}

/// Every registered chat command, in `/help` order (Java registration order:
/// `help`, `t`, `a`, `votekick`, `vote`, `sync`).
pub const COMMANDS: &[HelpEntry] = &[
    HelpEntry {
        name: "help",
        params: "[page]",
        description: "Lists all commands.",
    },
    HelpEntry {
        name: "t",
        params: "<message...>",
        description: "Send a message only to your teammates.",
    },
    HelpEntry {
        name: "a",
        params: "<message...>",
        description: "Send a message only to admins.",
    },
    HelpEntry {
        name: "votekick",
        params: "[player] [reason...]",
        description: "Vote to kick a player with a valid reason.",
    },
    HelpEntry {
        name: "vote",
        params: "<y/n/c>",
        description: "Vote to kick the current player. Admins can cancel the voting with 'c'.",
    },
    HelpEntry {
        name: "sync",
        params: "",
        description: "Re-synchronize world state.",
    },
];

/// `VOTE_MIN_PLAYERS` is re-exported through the parser docs for callers that
/// render the upstream "At least 3 players" message.
pub const MIN_VOTE_PLAYERS: usize = VOTE_MIN_PLAYERS;

/// Parses a chat message into a command.
///
/// Only a leading `/` starts a command (Java's default command prefix). The
/// message body of `/t`/`/a` keeps its inner spacing (outer whitespace is
/// trimmed, like Java's `[message...]` join).
pub fn parse_chat_command(text: &str) -> ChatCommand {
    // Java checks the prefix on the raw message (`CommandHandler.handleMessage`),
    // so a leading space means ordinary chat rather than a command.
    let Some(rest) = text.strip_prefix('/') else {
        return ChatCommand::None;
    };
    let (name, args) = match rest.find(char::is_whitespace) {
        Some(index) => (&rest[..index], rest[index..].trim()),
        None => (rest, ""),
    };
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "" => {
            ChatCommand::Invalid("Unknown command. Type /help for a list of commands.".to_string())
        }
        "t" | "team" => {
            if args.is_empty() {
                ChatCommand::Invalid("Usage: /t <message...>".to_string())
            } else {
                ChatCommand::Team(args.to_string())
            }
        }
        "a" | "admin" => {
            if args.is_empty() {
                ChatCommand::Invalid("Usage: /a <message...>".to_string())
            } else {
                ChatCommand::Admin(args.to_string())
            }
        }
        "votekick" => {
            let mut tokens = args.split_whitespace();
            let target = tokens.next().map(str::to_string);
            let reason = tokens.collect::<Vec<_>>().join(" ");
            ChatCommand::Votekick { target, reason }
        }
        "vote" => {
            let mut tokens = args.split_whitespace();
            let choice = match tokens.next().map(str::to_ascii_lowercase).as_deref() {
                Some("y") | Some("yes") => Some(VoteChoice::Yes),
                Some("n") | Some("no") => Some(VoteChoice::No),
                Some("c") | Some("cancel") if tokens.next().is_none() => Some(VoteChoice::Cancel),
                _ => None,
            };
            ChatCommand::Vote(choice)
        }
        "help" => {
            let mut tokens = args.split_whitespace();
            match tokens.next() {
                None => ChatCommand::Help(None),
                Some(raw) => match raw.parse::<u32>() {
                    Ok(page) => ChatCommand::Help(Some(page)),
                    Err(_) => ChatCommand::Invalid("'page' must be a number.".to_string()),
                },
            }
        }
        "sync" => ChatCommand::Sync,
        _ => ChatCommand::Invalid(format!(
            "Unknown command '/{name}'. Type /help for a list of commands."
        )),
    }
}

/// Renders one `/help` page body, mirroring Java's
/// `[orange]-- Commands Page[lightgray] @[gray]/[lightgray]@[orange] --`.
///
/// `page` is 1-based; out-of-range pages return the upstream error text.
pub fn help_page(page: u32) -> Result<String, String> {
    let pages = COMMANDS.len().div_ceil(HELP_COMMANDS_PER_PAGE).max(1) as u32;
    if page == 0 || page > pages {
        return Err(format!("'page' must be a number between 1 and {pages}."));
    }
    let start = ((page - 1) as usize) * HELP_COMMANDS_PER_PAGE;
    let end = (start + HELP_COMMANDS_PER_PAGE).min(COMMANDS.len());
    let mut out = format!("-- Commands Page {page}/{pages} --\n");
    for entry in &COMMANDS[start..end] {
        out.push_str(&format!(
            " /{} {} - {}\n",
            entry.name, entry.params, entry.description
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_chat_is_not_a_command() {
        assert_eq!(parse_chat_command("hello there"), ChatCommand::None);
        assert_eq!(
            parse_chat_command(" /not-a-command-leading-space"),
            ChatCommand::None
        );
    }

    #[test]
    fn team_and_admin_channels() {
        assert_eq!(
            parse_chat_command("/t push mid"),
            ChatCommand::Team("push mid".to_string())
        );
        assert_eq!(
            parse_chat_command("/A admin only"),
            ChatCommand::Admin("admin only".to_string())
        );
        assert!(matches!(parse_chat_command("/t"), ChatCommand::Invalid(_)));
    }

    #[test]
    fn votekick_arguments() {
        assert_eq!(
            parse_chat_command("/votekick Builder griefing the core"),
            ChatCommand::Votekick {
                target: Some("Builder".to_string()),
                reason: "griefing the core".to_string(),
            }
        );
        assert_eq!(
            parse_chat_command("/votekick"),
            ChatCommand::Votekick {
                target: None,
                reason: String::new(),
            }
        );
        assert_eq!(
            parse_chat_command("/votekick Builder"),
            ChatCommand::Votekick {
                target: Some("Builder".to_string()),
                reason: String::new(),
            }
        );
    }

    #[test]
    fn vote_choices() {
        assert_eq!(
            parse_chat_command("/vote y"),
            ChatCommand::Vote(Some(VoteChoice::Yes))
        );
        assert_eq!(
            parse_chat_command("/vote NO"),
            ChatCommand::Vote(Some(VoteChoice::No))
        );
        assert_eq!(
            parse_chat_command("/vote c"),
            ChatCommand::Vote(Some(VoteChoice::Cancel))
        );
        assert_eq!(parse_chat_command("/vote maybe"), ChatCommand::Vote(None));
        assert_eq!(parse_chat_command("/vote"), ChatCommand::Vote(None));
    }

    #[test]
    fn help_pages() {
        assert_eq!(parse_chat_command("/help"), ChatCommand::Help(None));
        assert_eq!(parse_chat_command("/help 2"), ChatCommand::Help(Some(2)));
        assert!(matches!(
            parse_chat_command("/help nope"),
            ChatCommand::Invalid(_)
        ));
        let page = help_page(1).expect("page 1");
        assert!(page.contains("-- Commands Page 1/1 --"));
        assert!(page.contains("/t <message...>"));
        assert!(page.contains("/sync  - "));
        assert!(help_page(0).is_err());
        assert!(help_page(2).is_err());
    }

    #[test]
    fn unknown_commands_are_reported() {
        match parse_chat_command("/nope x") {
            ChatCommand::Invalid(message) => assert!(message.contains("Unknown command '/nope'")),
            other => panic!("expected invalid, got {other:?}"),
        }
        assert!(matches!(parse_chat_command("/"), ChatCommand::Invalid(_)));
        assert_eq!(parse_chat_command("/sync"), ChatCommand::Sync);
    }
}
