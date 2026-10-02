// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Version and format constants (plan 00) plus the `Version.java` build report
//! (plan 22 M0 §3.2).
//!
//! Ported from `core/src/mindustry/core/Version.java`: the upstream
//! `init`/`isAtLeast`/`buildString`/`combined` semantics over a
//! `version.properties` document with the upstream key set (`type`, `number`,
//! `modifier`, `commitHash`, `buildDate`, `build`). The Godot-free half; the
//! client and server both consume [`BuildInfo`].

use std::sync::OnceLock;

use crate::assets::bundle::parse_properties;

/// Version of the `mind-core` crate, taken from `Cargo.toml`.
pub const MIND_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Application name used by dumps and the data directory.
pub const APP_NAME: &str = "Mindustry-Godot";

/// Format version of the canonical JSON state dump (§6.3).
pub const DUMP_FORMAT: u32 = 1;

/// Format version of scenario files (§6.1).
pub const SCENARIO_FORMAT: u32 = 1;

/// Format version of command-log JSONL files (§6.2).
pub const COMMAND_LOG_FORMAT: u32 = 1;

/// Build number written into save meta tags (`Version.build`; 0 = dev build,
/// matching the upstream default when no build is stamped).
pub const BUILD: i32 = 0;

/// Generated at build time from `client/assets/version.properties` (defaults
/// when the file is absent). Never hand-edited.
const EMBEDDED_VERSION_PROPERTIES: &str =
    include_str!(concat!(env!("OUT_DIR"), "/version.properties"));

/// Failure parsing a `version.properties` document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// The `number` key is present but not a non-negative integer.
    InvalidNumber(String),
}

impl std::fmt::Display for VersionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VersionError::InvalidNumber(value) => {
                write!(f, "invalid `number` in version.properties: {value:?}")
            }
        }
    }
}

impl std::error::Error for VersionError {}

/// Parsed build report (port of `Version.java` static fields).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildInfo {
    /// Build type. `"official"` for official releases; `"unknown"` default.
    pub r#type: String,
    /// Build modifier, e.g. `"alpha"` or `"release"`.
    pub modifier: String,
    /// Git commit hash (short).
    pub commit_hash: String,
    /// Date the build was produced.
    pub build_date: String,
    /// Major version number (`Version.number`, default 4).
    pub number: u32,
    /// Build number; `-1` for custom builds.
    pub build: i32,
    /// Revision number (hotfixes; does not affect server compatibility).
    pub revision: u32,
    /// Whether the Steam build was requested (`modifier.contains("steam")`).
    pub is_steam: bool,
    /// Whether version loading is enabled (`Version.enabled`).
    pub enabled: bool,
}

impl Default for BuildInfo {
    fn default() -> Self {
        Self {
            r#type: "unknown".to_owned(),
            modifier: "unknown".to_owned(),
            commit_hash: "unknown".to_owned(),
            build_date: "unknown".to_owned(),
            number: 4,
            build: -1,
            revision: 0,
            is_steam: false,
            enabled: true,
        }
    }
}

impl BuildInfo {
    /// Parses a `version.properties` document (port of `Version.init` over the
    /// Arc `PropertiesUtils.load` map).
    ///
    /// Missing keys keep the upstream defaults (`type`/`modifier`/`commitHash`/
    /// `buildDate` = `"unknown"`, `number` = 4, `build` = -1). A non-numeric
    /// `build` becomes -1; a `build` of the form `N.R` also sets `revision`.
    pub fn init_from(src: &str) -> Result<Self, VersionError> {
        let map = parse_properties(src);
        let mut info = Self::default();

        if let Some(value) = map.get("type") {
            info.r#type = value.clone();
        }
        if let Some(value) = map.get("modifier") {
            info.modifier = value.clone();
        }
        if let Some(value) = map.get("commitHash") {
            info.commit_hash = value.clone();
        }
        if let Some(value) = map.get("buildDate") {
            info.build_date = value.clone();
        }
        if let Some(value) = map.get("number") {
            info.number = value
                .trim()
                .parse::<u32>()
                .map_err(|_| VersionError::InvalidNumber(value.clone()))?;
        }
        if let Some(value) = map.get("build") {
            if let Some((major, minor)) = value.split_once('.') {
                match (major.trim().parse::<i32>(), minor.trim().parse::<u32>()) {
                    (Ok(build), Ok(revision)) => {
                        info.build = build;
                        info.revision = revision;
                    }
                    _ => {
                        info.build = -1;
                    }
                }
            } else {
                info.build = value.trim().parse::<i32>().unwrap_or(-1);
            }
        }
        info.is_steam = info.modifier.contains("steam");
        Ok(info)
    }

    /// The build embedded in the binary at compile time.
    ///
    /// `client/assets/version.properties` is generated by `tools/version.sh`
    /// (gitignored); `mind-core/build.rs` copies it to `$OUT_DIR` or emits the
    /// upstream defaults when it is absent.
    pub fn embedded() -> &'static BuildInfo {
        static INFO: OnceLock<BuildInfo> = OnceLock::new();
        INFO.get_or_init(|| BuildInfo::init_from(EMBEDDED_VERSION_PROPERTIES).unwrap_or_default())
    }

    /// Whether this build is at least the version string, e.g. `"120.1"`
    /// (`Version.isAtLeast`).
    pub fn is_at_least(&self, version: &str) -> bool {
        Self::is_at_least_nums(self.build, self.revision, version)
    }

    /// Whether `build`/`revision` are at least the version string
    /// (`Version.isAtLeast(int, int, String)`).
    pub fn is_at_least_nums(build: i32, revision: u32, version: &str) -> bool {
        if build <= 0 || version.is_empty() {
            return true;
        }
        if let Some(dot) = version.find('.') {
            let major = version[..dot].trim().parse::<i32>().unwrap_or(0);
            let minor = version[dot + 1..].trim().parse::<u32>().unwrap_or(0);
            build > major || (build == major && revision >= minor)
        } else {
            build >= version.trim().parse::<i32>().unwrap_or(0)
        }
    }

    /// `"custom"` or `"<build>[.<revision>]"` (`Version.buildString`).
    pub fn build_string(&self) -> String {
        if self.build < 0 {
            "custom".to_owned()
        } else if self.revision == 0 {
            self.build.to_string()
        } else {
            format!("{}.{}", self.build, self.revision)
        }
    }

    /// Menu version string without colors (`Version.combined`).
    pub fn combined(&self) -> String {
        if self.build == -1 {
            return "custom build".to_owned();
        }
        let prefix = if self.r#type == "official" {
            &self.modifier
        } else {
            &self.r#type
        };
        let revision = if self.revision == 0 {
            String::new()
        } else {
            format!(".{}", self.revision)
        };
        let commit = if self.commit_hash == "unknown" {
            String::new()
        } else {
            format!(" ({})", self.commit_hash)
        };
        format!("{prefix} build {}{revision}{commit}", self.build)
    }

    /// A JSON object with the fields `mind-headless version --json` prints.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "type": self.r#type,
            "modifier": self.modifier,
            "commitHash": self.commit_hash,
            "buildDate": self.build_date,
            "number": self.number,
            "build": self.build,
            "revision": self.revision,
            "isSteam": self.is_steam,
            "enabled": self.enabled,
            "buildString": self.build_string(),
            "combined": self.combined(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_upstream_keys() {
        let info = BuildInfo::init_from(
            "type=official\nnumber=4\nmodifier=release\ncommitHash=abc1234\nbuildDate=2026-10-01\nbuild=43\n",
        )
        .expect("parse");
        assert_eq!(info.r#type, "official");
        assert_eq!(info.modifier, "release");
        assert_eq!(info.commit_hash, "abc1234");
        assert_eq!(info.build_date, "2026-10-01");
        assert_eq!(info.number, 4);
        assert_eq!(info.build, 43);
        assert_eq!(info.revision, 0);
        assert!(!info.is_steam);
        assert_eq!(info.build_string(), "43");
        assert_eq!(info.combined(), "release build 43 (abc1234)");
    }

    #[test]
    fn parse_build_revision() {
        let info =
            BuildInfo::init_from("type=official\nmodifier=release\nbuild=43.7\n").expect("parse");
        assert_eq!(info.build, 43);
        assert_eq!(info.revision, 7);
        assert_eq!(info.build_string(), "43.7");
        assert_eq!(info.combined(), "release build 43.7");
    }

    #[test]
    fn missing_keys_default_custom() {
        let info = BuildInfo::init_from("").expect("parse");
        assert_eq!(info.r#type, "unknown");
        assert_eq!(info.modifier, "unknown");
        assert_eq!(info.number, 4);
        assert_eq!(info.build, -1);
        assert_eq!(info.build_string(), "custom");
        assert_eq!(info.combined(), "custom build");
    }

    #[test]
    fn crlf_and_comments() {
        let info = BuildInfo::init_from(
            "# comment\r\ntype=bleeding-edge\r\nmodifier=steam\r\nbuild=12\r\n",
        )
        .expect("parse");
        assert_eq!(info.r#type, "bleeding-edge");
        assert!(info.is_steam);
        assert_eq!(info.build, 12);
    }

    #[test]
    fn is_at_least_build() {
        assert!(BuildInfo::is_at_least_nums(-1, 0, "120"));
        assert!(BuildInfo::is_at_least_nums(0, 0, "120"));
        assert!(BuildInfo::is_at_least_nums(120, 0, "120"));
        assert!(!BuildInfo::is_at_least_nums(119, 9, "120"));
        assert!(BuildInfo::is_at_least_nums(121, 0, "120"));
        assert!(BuildInfo::is_at_least_nums(43, 0, ""));
    }

    #[test]
    fn is_at_least_build_revision() {
        assert!(BuildInfo::is_at_least_nums(120, 1, "120.1"));
        assert!(BuildInfo::is_at_least_nums(120, 2, "120.1"));
        assert!(!BuildInfo::is_at_least_nums(120, 0, "120.1"));
        assert!(BuildInfo::is_at_least_nums(121, 0, "120.9"));
    }

    #[test]
    fn build_string_revision() {
        let mut info = BuildInfo {
            build: 5,
            revision: 0,
            ..BuildInfo::default()
        };
        assert_eq!(info.build_string(), "5");
        info.revision = 3;
        assert_eq!(info.build_string(), "5.3");
        info.build = -1;
        assert_eq!(info.build_string(), "custom");
    }

    #[test]
    fn combined_official_steam() {
        let mut info = BuildInfo {
            modifier: "steam".to_owned(),
            commit_hash: "unknown".to_owned(),
            build: 10,
            ..BuildInfo::default()
        };
        // Default `type` is not `official`, so the type is printed.
        assert_eq!(info.combined(), "unknown build 10");
        info.r#type = "official".to_owned();
        assert_eq!(info.combined(), "steam build 10");
        info.commit_hash = "deadbee".to_owned();
        assert_eq!(info.combined(), "steam build 10 (deadbee)");
    }

    #[test]
    fn embedded_parses_without_panic() {
        let info = BuildInfo::embedded();
        // Any of the default/generated values is acceptable; parsing must not
        // panic and the derived flags must be self-consistent.
        assert_eq!(info.is_steam, info.modifier.contains("steam"));
        assert_eq!(info.build_string() == "custom", info.build < 0);
    }

    #[test]
    fn invalid_number_is_an_error() {
        let error = BuildInfo::init_from("number=four\n").expect_err("must fail");
        assert_eq!(error, VersionError::InvalidNumber("four".to_owned()));
    }
}
