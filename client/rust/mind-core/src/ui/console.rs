// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/ConsoleFragment.java (plan 14 M7 §3.5).

//! Console command registry + line interpreter (plan 14 M7, deviation OD1).
//!
//! Upstream injects Rhino scripts and runs `mods.getScripts().runConsole(...)`.
//! Scripting is OD1 (plan 20); here the console is a line-based command
//! interpreter whose commands are registered in Rust (plan 22's server console
//! and plan 20's script hook). The historical/scroll/mobile behaviour stays in
//! the GDScript fragment; this module owns parsing and dispatch only.

use indexmap::IndexMap;

/// `ConsoleFragment.messagesShown`.
pub const MESSAGES_SHOWN: usize = 30;

/// A registered console command handler.
pub type ConsoleHandler = Box<dyn Fn(&[String]) -> String + Send + Sync>;

/// One registered command.
pub struct ConsoleCommand {
    /// Command name (first token).
    pub name: String,
    /// Usage hint (`name <arg> …`).
    pub usage: String,
    /// One-line description for `help`.
    pub description: String,
    handler: ConsoleHandler,
}

impl std::fmt::Debug for ConsoleCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsoleCommand")
            .field("name", &self.name)
            .field("usage", &self.usage)
            .field("description", &self.description)
            .finish()
    }
}

/// Result of one console line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsoleEntry {
    /// The echoed input (`[lightgray]> ` is added by the fragment).
    pub input: String,
    /// Command output (empty for `clear`).
    pub output: String,
}

/// Registry of console commands.
#[derive(Default)]
pub struct ConsoleRegistry {
    commands: IndexMap<String, ConsoleCommand>,
}

impl std::fmt::Debug for ConsoleRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsoleRegistry")
            .field("commands", &self.commands.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ConsoleRegistry {
    /// Empty registry (no implicit commands).
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry with the ported `help` command.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register("help", "help", "list available commands", |_args| {
            String::from("Use the command palette; see the console docs for the full list.")
        });
        registry
    }

    /// Registers/replaces a command.
    pub fn register(
        &mut self,
        name: &str,
        usage: &str,
        description: &str,
        handler: impl Fn(&[String]) -> String + Send + Sync + 'static,
    ) {
        self.commands.insert(
            name.to_owned(),
            ConsoleCommand {
                name: name.to_owned(),
                usage: usage.to_owned(),
                description: description.to_owned(),
                handler: Box::new(handler),
            },
        );
    }

    /// Whether a command is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }

    /// Registered names in insertion order.
    pub fn names(&self) -> Vec<&str> {
        self.commands.keys().map(String::as_str).collect()
    }

    /// Number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// `help` rendering: one `usage — description` line per command.
    pub fn help_text(&self) -> String {
        self.commands
            .values()
            .map(|command| format!("{} — {}", command.usage, command.description))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// `sendMessage`: splits on whitespace and dispatches. The special `clear`
    /// command is handled by the fragment (it clears the buffer) — here it
    /// returns an empty output.
    pub fn execute(&self, line: &str) -> ConsoleEntry {
        let input = line.to_owned();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return ConsoleEntry {
                input,
                output: String::new(),
            };
        }
        if trimmed == "clear" {
            return ConsoleEntry {
                input,
                output: String::new(),
            };
        }
        let mut tokens = trimmed.split_whitespace();
        let name = tokens.next().unwrap_or("");
        let args: Vec<String> = tokens.map(str::to_owned).collect();
        let output = match self.commands.get(name) {
            Some(command) => (command.handler)(&args),
            None => format!("Unknown command: {name}. Type 'help' for a list."),
        };
        ConsoleEntry { input, output }
    }

    /// `ConsoleFragment.injectConsoleVariables` — JS injection is dropped (OD1);
    /// the Rust commands bind their own cursor/unit context instead.
    pub const fn inject_console_variables() -> &'static str {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatches_registered_command() {
        let mut registry = ConsoleRegistry::new();
        registry.register("wave", "wave <n>", "set wave", |args| {
            format!("wave={}", args.first().map(String::as_str).unwrap_or("?"))
        });
        let entry = registry.execute("wave 12");
        assert_eq!(entry.output, "wave=12");
        assert_eq!(entry.input, "wave 12");
    }

    #[test]
    fn unknown_command_reports_name() {
        let registry = ConsoleRegistry::with_defaults();
        let entry = registry.execute("nope");
        assert!(entry.output.contains("Unknown command: nope"));
    }

    #[test]
    fn blank_and_clear_are_silent() {
        let registry = ConsoleRegistry::with_defaults();
        assert_eq!(registry.execute("   ").output, "");
        assert_eq!(registry.execute("clear").output, "");
    }

    #[test]
    fn help_lists_registered_commands() {
        let registry = ConsoleRegistry::with_defaults();
        assert!(!registry.help_text().is_empty());
        assert_eq!(registry.names(), vec!["help"]);
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
        assert!(registry.contains("help"));
    }
}
