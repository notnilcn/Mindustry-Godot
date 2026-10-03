// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/UiHotReload.java.

//! MSUI hot-reload state (plan 14 §3.6).
//!
//! `UiHotReload` watches an `.msui` file's mtime with a 100 ms debounce and
//! renders parse errors with the source line. The Godot file picker/dialog is
//! `client/ui/dialogs/ui_hot_reload.gd`; this module owns the Godot-free timing
//! and error-line extraction so it is testable headless.

use crate::ui::builder::dsl;
use crate::ui::builder::ui_node::UiNode;

/// Debounce window (`UiHotReload` uses `> 100` ms).
pub const DEBOUNCE_MS: i64 = 100;

/// A parse error annotated with the offending source line (upstream renders
/// `Line: [red]<text>` under the editor).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotReloadError {
    /// 1-based line number from the parser.
    pub line: usize,
    /// Parser message (without the line suffix).
    pub message: String,
    /// The source line text, when in range.
    pub source_line: Option<String>,
}

/// Parses hot-reload source, tagging failures with their source line.
pub fn parse_source(source: &str) -> Result<UiNode, HotReloadError> {
    dsl::parse(source).map_err(|error| {
        let message = error.to_string();
        HotReloadError {
            line: error.line,
            message: error.message,
            source_line: error_source_line(source, &message),
        }
    })
}

/// Mtime/debounce tracker for one file.
#[derive(Debug, Clone)]
pub struct HotReload {
    last_modified: i64,
    debounce_at: i64,
    modified: bool,
}

impl HotReload {
    /// Starts tracking `modified` (a file mtime in ms).
    pub fn new(modified: i64) -> Self {
        Self {
            last_modified: modified,
            debounce_at: 0,
            modified: false,
        }
    }

    /// The last mtime seen.
    pub fn last_modified(&self) -> i64 {
        self.last_modified
    }

    /// Feeds the current mtime and time. Returns `true` once the file changed and
    /// the debounce window has elapsed, and resets the pending flag.
    pub fn tick(&mut self, modified: i64, now_ms: i64) -> bool {
        if modified != self.last_modified {
            self.last_modified = modified;
            self.modified = true;
            self.debounce_at = now_ms;
        }
        if self.modified && now_ms.saturating_sub(self.debounce_at) > DEBOUNCE_MS {
            self.modified = false;
            return true;
        }
        false
    }
}

/// Extracts the 1-based line number from an error message containing `line N`
/// (`UiHotReload.parseException`).
pub fn error_line(message: &str) -> Option<usize> {
    let index = message.find("line ")?;
    let rest = &message[index + 5..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let line: usize = digits.parse().ok()?;
    (line > 0).then_some(line)
}

/// Returns the trimmed source line named by an error message (upstream renders
/// `Line: [red]<text>`), or `None` when the line is out of range.
pub fn error_source_line(source: &str, message: &str) -> Option<String> {
    let line = error_line(message)?;
    let lines: Vec<&str> = source.split('\n').collect();
    // Upstream indexes `lines[actualLine - 1]` only when `actualLine < lines.length`.
    if line >= lines.len() {
        return None;
    }
    Some(lines[line - 1].trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debounce_waits_for_the_window() {
        let mut hot = HotReload::new(1_000);
        // Unchanged mtime: never reloads.
        assert!(!hot.tick(1_000, 1_050));
        // Changed mtime marks pending but waits out the debounce.
        assert!(!hot.tick(1_100, 1_200));
        assert!(!hot.tick(1_100, 1_250));
        assert!(hot.tick(1_100, 1_301));
        // Flag reset: no more reloads without another change.
        assert!(!hot.tick(1_100, 2_000));
        // A second edit re-arms.
        assert!(!hot.tick(1_200, 3_000));
        assert!(hot.tick(1_200, 3_101));
        assert_eq!(hot.last_modified(), 1_200);
    }

    #[test]
    fn error_line_and_source_extraction() {
        assert_eq!(error_line("Unknown property at line 3"), Some(3));
        assert_eq!(error_line("no line here"), None);
        assert_eq!(error_line("line 0"), None);

        let source = "label: \"a\"\nbogus: 1\n";
        assert_eq!(
            error_source_line(source, "Unknown property \"bogus\" at line 2"),
            Some("bogus: 1".to_owned())
        );
        // Out-of-range line returns `None`.
        assert_eq!(error_source_line(source, "line 5"), None);
    }

    #[test]
    fn parse_source_reports_line_and_source_text() {
        let tree = parse_source("label: \"ok\"\n").unwrap();
        assert_eq!(tree.entries.len(), 1);

        let error = parse_source("label: \"a\"\nbogus: 1\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert_eq!(error.source_line.as_deref(), Some("bogus: 1"));
        assert!(error.message.contains("Unknown property"));
    }
}
