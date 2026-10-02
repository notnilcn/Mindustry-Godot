// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `rules.hjson` bootstrap and rule edits (plan 22 §3.6/§6.4).
//!
//! Port of `ServerControl`'s `defaultRuleString` bootstrap and the
//! `rules [remove/add]` command. The typed `Rules` deserializer is plan 12 (not
//! built yet), so this module keeps a tolerant flat `key: value` representation
//! (HJSON-ish) and applies the three upstream default rule keys to a small
//! [`RulesState`]; unknown keys round-trip untouched.

use std::collections::BTreeMap;
use std::path::Path;

use mind_core::io::fs::FileSystem;

/// Upstream `ServerControl.defaultRuleString`.
pub const DEFAULT_RULES: &str =
    "reactorExplosions: false\nlogicUnitBuild: false\nlogicUnitDeconstruct: false\n";

/// Parse failure for a rules document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesError {
    /// The offending line (1-based) and reason.
    pub line: usize,
    /// Human reason.
    pub reason: String,
}

impl std::fmt::Display for RulesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rules line {}: {}", self.line, self.reason)
    }
}

impl std::error::Error for RulesError {}

/// A tolerant flat rules document (`key: value`) with parse warnings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RulesFile {
    /// Entries in file order.
    pub entries: BTreeMap<String, serde_json::Value>,
    /// Lines that were skipped (logged, not fatal).
    pub warnings: Vec<String>,
}

impl RulesFile {
    /// Parses a tolerant `key: value` / `key = value` document.
    ///
    /// A line without a separator, or an empty key, is a hard error (the
    /// upstream default file is always valid); a malformed value degrades to a
    /// string with a warning.
    pub fn parse(text: &str) -> Result<Self, RulesError> {
        let mut file = RulesFile::default();
        for (index, raw) in text.lines().enumerate() {
            let line_number = index + 1;
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
                continue;
            }
            let separator =
                line.find(':')
                    .or_else(|| line.find('='))
                    .ok_or_else(|| RulesError {
                        line: line_number,
                        reason: format!("missing ':' separator in {line:?}"),
                    })?;
            let key = line[..separator].trim().to_owned();
            if key.is_empty() {
                return Err(RulesError {
                    line: line_number,
                    reason: "empty key".to_owned(),
                });
            }
            let value_text = line[separator + 1..].trim().trim_end_matches(',');
            let value = parse_value(value_text).unwrap_or_else(|| {
                file.warnings
                    .push(format!("line {line_number}: unparsed value {value_text:?}"));
                serde_json::Value::String(value_text.to_owned())
            });
            file.entries.insert(key, value);
        }
        Ok(file)
    }

    /// Writes the upstream default file when `path` is absent.
    pub fn ensure_default(fs: &dyn FileSystem, path: &Path) -> Result<bool, std::io::Error> {
        if fs.exists(path) {
            return Ok(false);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, DEFAULT_RULES)?;
        Ok(true)
    }

    /// Loads `path`; a missing or malformed file falls back to the defaults
    /// (with a logged warning), matching upstream's "parse errors log and
    /// continue".
    pub fn load(fs: &dyn FileSystem, path: &Path) -> Self {
        if !fs.exists(path) {
            return Self::parse(DEFAULT_RULES).unwrap_or_default();
        }
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => return Self::parse(DEFAULT_RULES).unwrap_or_default(),
        };
        match Self::parse(&text) {
            Ok(file) => file,
            Err(error) => {
                log::warn!("failed to parse rules file {}: {error}", path.display());
                Self::parse(DEFAULT_RULES).unwrap_or_default()
            }
        }
    }

    /// Serializes back to `key: value` lines.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.entries {
            out.push_str(key);
            out.push_str(": ");
            out.push_str(&value.to_string());
            out.push('\n');
        }
        out
    }

    /// Sets `key` from a console string (`rules add`).
    pub fn set(&mut self, key: &str, value: &str) {
        let parsed =
            parse_value(value).unwrap_or_else(|| serde_json::Value::String(value.to_owned()));
        self.entries.insert(key.to_owned(), parsed);
    }

    /// Removes `key` (`rules remove`).
    pub fn remove(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }
}

fn parse_value(text: &str) -> Option<serde_json::Value> {
    match text.trim() {
        "true" => Some(serde_json::Value::Bool(true)),
        "false" => Some(serde_json::Value::Bool(false)),
        other => {
            if let Ok(value) = other.parse::<i64>() {
                return Some(serde_json::Value::Number(value.into()));
            }
            if let Ok(value) = other.parse::<f64>()
                && let Some(number) = serde_json::Number::from_f64(value)
            {
                return Some(serde_json::Value::Number(number));
            }
            if other.len() >= 2 && other.starts_with('"') && other.ends_with('"') {
                return Some(serde_json::Value::String(
                    other[1..other.len() - 1].to_owned(),
                ));
            }
            None
        }
    }
}

/// The three upstream default rules as typed state (plan-12 `Rules` seam).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RulesState {
    /// `reactorExplosions`.
    pub reactor_explosions: bool,
    /// `logicUnitBuild`.
    pub logic_unit_build: bool,
    /// `logicUnitDeconstruct`.
    pub logic_unit_deconstruct: bool,
}

impl RulesState {
    /// Applies the known keys from a parsed file (unknown keys are ignored).
    pub fn apply(&mut self, file: &RulesFile) {
        if let Some(value) = file
            .entries
            .get("reactorExplosions")
            .and_then(|v| v.as_bool())
        {
            self.reactor_explosions = value;
        }
        if let Some(value) = file.entries.get("logicUnitBuild").and_then(|v| v.as_bool()) {
            self.logic_unit_build = value;
        }
        if let Some(value) = file
            .entries
            .get("logicUnitDeconstruct")
            .and_then(|v| v.as_bool())
        {
            self.logic_unit_deconstruct = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::io::fs::NativeFs;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mind-srv-rules-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create");
        dir
    }

    #[test]
    fn default_written() {
        let dir = temp_dir("default");
        let path = dir.join("rules.hjson");
        assert!(RulesFile::ensure_default(&NativeFs, &path).expect("write"));
        assert_eq!(std::fs::read_to_string(&path).expect("read"), DEFAULT_RULES);
        // Second call is a no-op.
        assert!(!RulesFile::ensure_default(&NativeFs, &path).expect("exists"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_and_apply() {
        let file = RulesFile::parse(DEFAULT_RULES).expect("parse");
        assert_eq!(file.entries.len(), 3);
        assert!(file.warnings.is_empty());
        let mut state = RulesState::default();
        state.apply(&file);
        assert!(!state.reactor_explosions);
        assert!(!state.logic_unit_build);
        assert!(!state.logic_unit_deconstruct);

        let mut edited = file.clone();
        edited.set("reactorExplosions", "true");
        state = RulesState::default();
        state.apply(&edited);
        assert!(state.reactor_explosions);
        assert!(edited.remove("logicUnitBuild"));
        assert!(!edited.entries.contains_key("logicUnitBuild"));
    }

    #[test]
    fn bad_json_logged_and_ignored() {
        // A hard parse error.
        assert!(RulesFile::parse("{").is_err());
        // `load` falls back to the upstream defaults instead of failing.
        let dir = temp_dir("bad");
        let path = dir.join("rules.hjson");
        std::fs::write(&path, "{ not valid").expect("write");
        let file = RulesFile::load(&NativeFs, &path);
        assert_eq!(file.entries.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
