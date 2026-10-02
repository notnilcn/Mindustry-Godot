// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Mod metadata (`Mods.ModMeta`) and meta-file discovery.
//!
//! Ported from `core/src/mindustry/mod/Mods.java` (`ModMeta`, `metaFiles`,
//! `findMeta`, `cleanup`, `getMinMajor`, `shortDescription`, `isBlacklisted`)
//! and `core/src/mindustry/Vars.java` (`maxModSubtitleLength`,
//! `minModGameVersion`, `minJavaModGameVersion`).

use serde::{Deserialize, Deserializer, Serialize};

use super::ModError;

/// `Mods.metaFiles` — checked in order; the first existing file wins.
pub const META_FILES: [&str; 4] = ["mod.json", "mod.hjson", "plugin.json", "plugin.hjson"];

/// `Vars.maxModSubtitleLength`.
pub const MAX_MOD_SUBTITLE_LENGTH: usize = 40;

/// `Vars.minModGameVersion` — minimum major game version for data mods.
pub const MIN_MOD_GAME_VERSION: i32 = 136;

/// `Vars.minJavaModGameVersion` — minimum major game version for Java mods.
pub const MIN_JAVA_MOD_GAME_VERSION: i32 = 154;

/// `Mods.blacklistedMods` (name and `name:version` forms).
pub const BLACKLISTED_MODS: [&str; 15] = [
    "ui-lib",
    "braindustry",
    "schema",
    "scheme-size:1.0.5",
    "scheme-size:1.0.4",
    "scheme-size:1.0.3",
    "scheme-size:1.0.1",
    "scheme-size:1.0.0",
    "scheme-size:1.1.0",
    "scheme-size:1.0.4.1",
    "patch-editor:1.10.1",
    "patch-editor:1.10.0",
    "patch-editor:1.9.5",
    "patch-editor:1.9.4",
    "patch-editor:1.9.3",
];

/// `Mods.ModMeta`: parsed `mod.json`/`mod.hjson` metadata.
///
/// Field names are the upstream `camelCase` ABI; the Rust fields are
/// `snake_case` with serde renames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModMeta {
    /// Name as defined in mod.json (colors stripped, may contain spaces).
    pub name: String,
    /// Name without spaces, all lowercase (`cleanup()`).
    #[serde(skip)]
    pub internal_name: String,
    /// Display name (defaults to `name`).
    #[serde(default, deserialize_with = "opt_string_lenient")]
    pub display_name: Option<String>,
    /// Author (colors stripped).
    #[serde(default, deserialize_with = "opt_string_lenient")]
    pub author: Option<String>,
    /// Description (colors stripped).
    #[serde(default, deserialize_with = "opt_string_lenient")]
    pub description: Option<String>,
    /// Subtitle (colors stripped, newlines removed).
    #[serde(default, deserialize_with = "opt_string_lenient")]
    pub subtitle: Option<String>,
    /// Version (first line only).
    #[serde(default, deserialize_with = "opt_string_lenient")]
    pub version: Option<String>,
    /// Minimum game version (e.g. `"140.1"`), defaults to `"0"`.
    #[serde(default = "default_min_game_version")]
    pub min_game_version: String,
    /// Main Java class, if any.
    #[serde(default, deserialize_with = "opt_string_lenient")]
    pub main: Option<String>,
    /// Explicit Java mod flag.
    #[serde(default)]
    pub java: bool,
    /// Hidden mods are server/client-only and never add content.
    #[serde(default)]
    pub hidden: bool,
    /// Required dependencies (internal names).
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Soft dependencies (internal names).
    #[serde(default)]
    pub soft_dependencies: Vec<String>,
    /// Explicit content load order (content names).
    #[serde(default)]
    pub content_order: Vec<String>,
    /// Sprite scale (size in pixels of a 1x1 block).
    #[serde(default = "default_texture_scale", deserialize_with = "f32_lenient")]
    pub texturescale: f32,
    /// Skip bleeding and content icon generation.
    #[serde(default)]
    pub pregenerated: bool,
    /// iOS-compatible scripts/Java mod flag.
    #[serde(default)]
    pub ios_compatible: bool,
    /// Older-major mod that is still compatible.
    #[serde(default)]
    pub legacy_compatible: bool,
    /// Repository (`owner/repo`).
    #[serde(default, deserialize_with = "opt_string_lenient")]
    pub repo: Option<String>,
}

fn default_min_game_version() -> String {
    String::from("0")
}

fn default_texture_scale() -> f32 {
    1.0
}

impl Default for ModMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            internal_name: String::new(),
            display_name: None,
            author: None,
            description: None,
            subtitle: None,
            version: None,
            min_game_version: default_min_game_version(),
            main: None,
            java: false,
            hidden: false,
            dependencies: Vec::new(),
            soft_dependencies: Vec::new(),
            content_order: Vec::new(),
            texturescale: default_texture_scale(),
            pregenerated: false,
            ios_compatible: false,
            legacy_compatible: false,
            repo: None,
        }
    }
}

impl ModMeta {
    /// `Mods.findMeta` + `ModMeta.cleanup`: parse raw text and normalize.
    pub fn parse(text: &str) -> Result<Self, ModError> {
        let json = normalize_dialect(text);
        let mut meta: ModMeta = serde_json::from_str(&json)
            .map_err(|error| ModError::Invalid(format!("invalid mod metadata: {error}")))?;
        meta.cleanup()?;
        Ok(meta)
    }

    /// `ModMeta.cleanup()`: strip colors, apply defaults, derive internal name.
    pub fn cleanup(&mut self) -> Result<(), ModError> {
        if self.name.is_empty() {
            return Err(ModError::Invalid(String::from(
                "mod metadata is missing a name",
            )));
        }
        self.name = strip_colors(&self.name);
        self.display_name = self.display_name.as_deref().map(strip_colors);
        if self.display_name.is_none() {
            self.display_name = Some(self.name.clone());
        }
        if self.version.is_none() {
            self.version = Some(String::from("0"));
        }
        self.author = self.author.as_deref().map(strip_colors);
        self.description = self.description.as_deref().map(strip_colors);
        self.subtitle = self
            .subtitle
            .as_deref()
            .map(|subtitle| strip_colors(subtitle).replace('\n', ""));
        self.internal_name = internal_name(&self.name);
        Ok(())
    }

    /// `ModMeta.displayName()` equivalent.
    pub fn display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }

    /// `ModMeta.version()` equivalent (defaults to `"0"`).
    pub fn version(&self) -> &str {
        self.version.as_deref().unwrap_or("0")
    }

    /// `ModMeta.getMinMajor()`: parses the major component before the first dot.
    pub fn min_major(&self) -> i32 {
        let version = if self.min_game_version.is_empty() {
            "0"
        } else {
            &self.min_game_version
        };
        let major = version.split('.').next().unwrap_or("0");
        major.trim().parse::<i32>().unwrap_or(0)
    }

    /// `ModMeta.isBlacklisted()`.
    pub fn is_blacklisted(&self) -> bool {
        if BLACKLISTED_MODS.contains(&self.name.as_str()) {
            return true;
        }
        let with_version = format!("{}:{}", self.name, self.version());
        BLACKLISTED_MODS.contains(&with_version.as_str())
    }

    /// `ModMeta.shortDescription()`.
    pub fn short_description(&self) -> String {
        let base = match &self.subtitle {
            Some(subtitle) => subtitle.clone(),
            None => match &self.description {
                Some(description) if description.len() <= MAX_MOD_SUBTITLE_LENGTH => {
                    description.clone()
                }
                _ => String::new(),
            },
        };
        truncate(&base, MAX_MOD_SUBTITLE_LENGTH, "...")
    }

    /// Render the metadata back to canonical JSON (harness output).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// `Mods.LoadedMod.name` derivation: lowercase, spaces → hyphens.
pub fn internal_name(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

/// `Strings.stripColors`: removes `[...]` color/markup tags.
pub fn strip_colors(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '[' {
            out.push(c);
            continue;
        }
        let mut tag = String::new();
        let mut closed = false;
        while let Some(&next) = chars.peek() {
            if next == ']' {
                chars.next();
                closed = true;
                break;
            }
            if next == '[' {
                break;
            }
            tag.push(next);
            chars.next();
        }
        if !closed {
            out.push('[');
            out.push_str(&tag);
        }
    }
    out
}

/// `Strings.truncate(s, len, ellipsis)`.
pub fn truncate(input: &str, len: usize, ellipsis: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    if chars.len() >= len {
        let keep = len.saturating_sub(ellipsis.chars().count());
        let head: String = chars.into_iter().take(keep).collect();
        format!("{head}{ellipsis}")
    } else {
        input.to_owned()
    }
}

/// Lenient string field: accepts JSON string, number, bool, or null.
fn opt_string_lenient<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(text)) => Some(text),
        Some(serde_json::Value::Number(number)) => Some(number.to_string()),
        Some(serde_json::Value::Bool(flag)) => Some(flag.to_string()),
        Some(other) => Some(other.to_string()),
    })
}

/// Lenient float field: accepts JSON number or numeric string.
fn f32_lenient<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Repr {
        Float(f32),
        Int(i64),
        Text(String),
    }
    match Repr::deserialize(deserializer)? {
        Repr::Float(value) => Ok(value),
        Repr::Int(value) => Ok(value as f32),
        Repr::Text(text) => text.parse::<f32>().map_err(serde::de::Error::custom),
    }
}

/// Tolerant dialect normalizer: strips `//` and `/* */` comments outside
/// strings. `.hjson`/`.json5` unquoted keys remain unsupported and surface as a
/// clear parse error (plan 20 R2).
fn normalize_dialect(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' => match chars.peek() {
                Some('/') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\n' {
                            out.push('\n');
                            break;
                        }
                    }
                }
                Some('*') => {
                    chars.next();
                    let mut prev = '\0';
                    for next in chars.by_ref() {
                        if prev == '*' && next == '/' {
                            break;
                        }
                        prev = next;
                    }
                }
                _ => out.push('/'),
            },
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan 20 M0: `meta::cleanup_and_internal_name`.
    #[test]
    fn cleanup_and_internal_name() {
        let text = r#"{
            "name": "[accent]Test Mod[white]",
            "displayName": "[red]Display",
            "author": "[blue]tester",
            "description": "[gray]Fixture mod",
            "subtitle": "fixture\nline2",
            "version": "1.0\n2.0",
            "texturescale": 2
        }"#;
        let meta = ModMeta::parse(text).expect("parse");
        assert_eq!(meta.name, "Test Mod");
        assert_eq!(meta.internal_name, "test-mod");
        assert_eq!(meta.display_name.as_deref(), Some("Display"));
        assert_eq!(meta.author.as_deref(), Some("tester"));
        assert_eq!(meta.description.as_deref(), Some("Fixture mod"));
        assert_eq!(meta.subtitle.as_deref(), Some("fixtureline2"));
        assert_eq!(meta.texturescale, 2.0);
        // Version is not truncated by `cleanup` (upstream does it in loadMod).
        assert_eq!(meta.version(), "1.0\n2.0");
    }

    /// Plan 20 M0: `meta::min_major`.
    #[test]
    fn min_major() {
        let mut meta = ModMeta {
            name: String::from("x"),
            ..ModMeta::default()
        };
        meta.cleanup().expect("cleanup");
        assert_eq!(meta.min_major(), 0);

        meta.min_game_version = String::from("146");
        assert_eq!(meta.min_major(), 146);
        meta.min_game_version = String::from("146.2");
        assert_eq!(meta.min_major(), 146);
        meta.min_game_version = String::from("bogus");
        assert_eq!(meta.min_major(), 0);
    }

    #[test]
    fn defaults_and_blacklist() {
        let meta = ModMeta::parse(r#"{"name":"Plain"}"#).expect("parse");
        assert_eq!(meta.version(), "0");
        assert_eq!(meta.display_name(), "Plain");
        assert_eq!(meta.texturescale, 1.0);
        assert!(!meta.is_blacklisted());

        let mut banned = ModMeta {
            name: String::from("ui-lib"),
            ..ModMeta::default()
        };
        banned.cleanup().expect("cleanup");
        assert!(banned.is_blacklisted());
    }

    #[test]
    fn color_strip_and_truncate() {
        assert_eq!(strip_colors("[red]hello[white] world"), "hello world");
        assert_eq!(strip_colors("no tags"), "no tags");
        assert_eq!(strip_colors("unclosed [tag"), "unclosed [tag");
        assert_eq!(truncate("abcdefghij", 8, "..."), "abcde...");
        assert_eq!(truncate("short", 8, "..."), "short");
    }

    #[test]
    fn comments_are_stripped() {
        let text = "{\n// a comment\n\"name\": \"Commented\", /* block */ \"version\": \"2\"\n}";
        let meta = ModMeta::parse(text).expect("parse");
        assert_eq!(meta.name, "Commented");
        assert_eq!(meta.version(), "2");
    }
}
