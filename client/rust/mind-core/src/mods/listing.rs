// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! GitHub mod listings and version matching (plan 20 M6/§3.9).
//!
//! Ports the data half of `mod/ModListing.java` plus `ModsDialog.parseVersion`,
//! `parseVersionTag` and `matchesGameVersion`. HTTP/download is owned by the
//! Godot layer (`mod_browser_dialog.gd`, plan 14); `mind-core` only parses the
//! listing JSON and decides which release matches the build.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// One GitHub release pointer (`ModListing.ModRelease`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModRelease {
    /// GitHub release id (`$API/releases/$id`).
    #[serde(default)]
    pub id: String,
    /// Mod version string in the release's `mod.json`.
    #[serde(default)]
    pub version: String,
}

/// A parsed mod listing (`ModListing`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModListing {
    /// `owner/repo`.
    pub repo: String,
    /// Display name.
    pub name: String,
    /// Internal name.
    pub internal_name: String,
    /// Author.
    pub author: String,
    /// Last-updated timestamp.
    pub last_updated: String,
    /// Description text.
    pub description: String,
    /// Minimum game version.
    pub min_game_version: String,
    /// Latest version.
    pub version: String,
    /// Icon hash (`iconHash`).
    pub icon_hash: String,
    /// Ships scripts.
    pub has_scripts: bool,
    /// Ships Java classes.
    pub has_java: bool,
    /// iOS compatible.
    pub ios_compatible: bool,
    /// Legacy compatible.
    pub legacy_compatible: bool,
    /// Has an icon.
    pub has_icon: bool,
    /// `game build -> release`.
    pub releases: IndexMap<String, ModRelease>,
    /// Tags.
    pub tags: Vec<String>,
    /// Star count.
    pub stars: i32,
}

impl ModListing {
    /// `getMatchingRelease`: the release whose tag matches `build`/`revision`,
    /// or `None` (use `/latest`).
    pub fn get_matching_release(&self, build: i32, revision: i32) -> Option<&ModRelease> {
        for (tag, release) in &self.releases {
            if matches_game_version(parse_version_tag(tag), build, revision) {
                return Some(release);
            }
        }
        None
    }

    /// `ModsDialog.matchesGameVersion` on a release title.
    pub fn release_matches(&self, release_title: &str, build: i32, revision: i32) -> bool {
        matches_game_version(parse_version_tag(release_title), build, revision)
    }
}

/// `ModsDialog.parseVersion`: parses `"146"` or `"146.1"` (exactly one dot).
///
/// Returns `(major, minor)`; `None` for empty/malformed input (major-only
/// versions default `minor = 0`).
pub fn parse_version(text: &str) -> Option<(i32, i32)> {
    if text.is_empty() {
        return None;
    }
    match text.split_once('.') {
        None => Some((text.parse().ok()?, 0)),
        Some((major, minor)) => {
            if major.is_empty() || minor.is_empty() || minor.contains('.') {
                return None;
            }
            Some((major.parse().ok()?, minor.parse().ok()?))
        }
    }
}

/// `ModsDialog.parseVersionTag`: extracts `[v160]`/`[b160.1]` from a title.
///
/// Only a tag anchored at the start or end counts, and a major below 15 is
/// ignored (likely a major-version-style tag). Returns `(major, minor)` with
/// `minor = 0` when unspecified.
pub fn parse_version_tag(text: &str) -> Option<(i32, i32)> {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'[' {
            index += 1;
            continue;
        }
        let start = index;
        let mut cursor = index + 1;
        if cursor < bytes.len() && (bytes[cursor] == b'v' || bytes[cursor] == b'b') {
            cursor += 1;
        }
        let major_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == major_start {
            index += 1;
            continue;
        }
        let major: i32 = text[major_start..cursor].parse().ok()?;
        let mut minor = 0;
        if cursor < bytes.len() && bytes[cursor] == b'.' {
            cursor += 1;
            let minor_start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
            if cursor == minor_start {
                index += 1;
                continue;
            }
            minor = text[minor_start..cursor].parse().ok()?;
        }
        if cursor >= bytes.len() || bytes[cursor] != b']' {
            index += 1;
            continue;
        }
        // Anchored at start or end, and major >= 15.
        if (start == 0 || cursor + 1 == bytes.len()) && major >= 15 {
            return Some((major, minor));
        }
        index = cursor + 1;
    }
    None
}

/// `ModsDialog.matchesGameVersion`: an unspecified tag matches anything; a
/// specified tag must match the build exactly, and a specified revision
/// (`minor != 0`) must match the revision too.
pub fn matches_game_version(tag: Option<(i32, i32)>, build: i32, revision: i32) -> bool {
    match tag {
        None => true,
        Some((major, minor)) => build == major && (minor == 0 || revision == minor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan 20 M6: `listing::parse_version`.
    #[test]
    fn parse_version() {
        assert_eq!(super::parse_version("146"), Some((146, 0)));
        assert_eq!(super::parse_version("146.1"), Some((146, 1)));
        assert_eq!(super::parse_version(""), None);
        assert_eq!(super::parse_version("abc"), None);
        assert_eq!(super::parse_version("146."), None);
        assert_eq!(super::parse_version(".1"), None);
        assert_eq!(super::parse_version("146.1.2"), None);
    }

    /// Plan 20 M6: `listing::matches_game_version`.
    #[test]
    fn matches_game_version_semantics() {
        // No tag matches anything.
        assert!(matches_game_version(None, 146, 1));
        // Major-only matches all revisions.
        assert!(matches_game_version(Some((146, 0)), 146, 7));
        assert!(!matches_game_version(Some((146, 0)), 147, 0));
        // Specified revision must match exactly.
        assert!(matches_game_version(Some((146, 2)), 146, 2));
        assert!(!matches_game_version(Some((146, 2)), 146, 3));
    }

    /// Plan 20 M6: version tags in release titles.
    #[test]
    fn parse_version_tags() {
        assert_eq!(parse_version_tag("Bingus Mod [v160]"), Some((160, 0)));
        assert_eq!(parse_version_tag("[b160.4] Bingus"), Some((160, 4)));
        // Unanchored tags are ignored.
        assert_eq!(parse_version_tag("x [v160] y"), None);
        // Major below 15 is ignored.
        assert_eq!(parse_version_tag("[v7]"), None);
    }

    /// Plan 20 M6: `getMatchingRelease` picks the version-matching entry.
    #[test]
    fn matching_release() {
        let mut releases = IndexMap::new();
        releases.insert(
            String::from("[v147]"),
            ModRelease {
                id: String::from("2"),
                version: String::from("2.0"),
            },
        );
        releases.insert(
            String::from("[v146]"),
            ModRelease {
                id: String::from("1"),
                version: String::from("1.0"),
            },
        );
        let listing = ModListing {
            releases,
            ..ModListing::default()
        };
        let release = listing.get_matching_release(146, 0).expect("match 146");
        assert_eq!(release.id, "1");
        assert!(listing.get_matching_release(140, 0).is_none());
    }

    /// GitHub listing JSON deserializes with camelCase keys and defaults.
    #[test]
    fn listing_json_roundtrip() {
        let json = r#"{
            "repo": "owner/repo",
            "name": "Bingus",
            "internalName": "bingus",
            "author": "tester",
            "stars": 12,
            "tags": ["utility"],
            "releases": {"[v146]": {"id": "1", "version": "1.0"}}
        }"#;
        let listing: ModListing = serde_json::from_str(json).expect("parse listing");
        assert_eq!(listing.repo, "owner/repo");
        assert_eq!(listing.internal_name, "bingus");
        assert_eq!(listing.stars, 12);
        assert_eq!(listing.tags, vec!["utility"]);
        assert_eq!(listing.releases.len(), 1);
    }
}
