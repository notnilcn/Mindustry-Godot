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
    ///
    /// `mods.json` writes this as a JSON number; `arc`'s `Jval` coerces it to
    /// the upstream `String` field, so deserialize accepts either form.
    #[serde(default, deserialize_with = "deserialize_id")]
    pub id: String,
    /// Mod version string in the release's `mod.json`.
    #[serde(default)]
    pub version: String,
}

/// Accepts a release id written as a string or a number (upstream `Jval.asString`).
fn deserialize_id<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum IdValue {
        Str(String),
        Int(i64),
        Uint(u64),
        Float(f64),
    }

    Ok(match Option::<IdValue>::deserialize(deserializer)? {
        None => String::new(),
        Some(IdValue::Str(value)) => value,
        Some(IdValue::Int(value)) => value.to_string(),
        Some(IdValue::Uint(value)) => value.to_string(),
        Some(IdValue::Float(value)) => {
            if value.fract() == 0.0 {
                format!("{}", value as i64)
            } else {
                value.to_string()
            }
        }
    })
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
    /// `getMatchingRelease`: the release whose key matches `build`/`revision`,
    /// or `None` (use `/latest`).
    ///
    /// `releases` is keyed by the game build string (`"146"` / `"146.1"`) and
    /// upstream parses each key with `parseVersion` (not `parseVersionTag`).
    pub fn get_matching_release(&self, build: i32, revision: i32) -> Option<&ModRelease> {
        for (tag, release) in &self.releases {
            if matches_game_version(parse_version(tag), build, revision) {
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
        // Only a tag anchored at the start or end counts; the first one found
        // decides (upstream returns null for an anchored major below 15 rather
        // than scanning on).
        if start == 0 || cursor + 1 == bytes.len() {
            // Below 15 is likely a major-version tag like `[v7]`.
            if major < 15 {
                return None;
            }
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
        // Signed forms parse like Java `Strings.parseInt` (sign is preserved).
        assert_eq!(super::parse_version("+146"), Some((146, 0)));
        assert_eq!(super::parse_version("-1"), Some((-1, 0)));
        // Non-numeric tails are refused.
        assert_eq!(super::parse_version("146a"), None);
        assert_eq!(super::parse_version("146.1a"), None);
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
        // Custom builds (`-1`) never match a specified tag.
        assert!(!matches_game_version(Some((146, 0)), -1, 0));
        assert!(matches_game_version(None, -1, 0));
    }

    /// Plan 20 M6: `matchesGameVersion(releaseTitle)` (title -> tag -> match).
    #[test]
    fn matches_game_version_title() {
        assert!(matches_game_version(parse_version_tag("[v146]"), 146, 0));
        assert!(matches_game_version(
            parse_version_tag("Mod [b146.2]"),
            146,
            2
        ));
        assert!(!matches_game_version(parse_version_tag("[v146]"), 145, 0));
        // Untagged (or unanchored) titles match every build.
        assert!(matches_game_version(parse_version_tag("Nightly"), 999, 9));
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
    ///
    /// `releases` is keyed by the game build string and parsed with
    /// `parseVersion` (upstream), so `"[v146]"` keys would never match.
    #[test]
    fn matching_release() {
        let mut releases = IndexMap::new();
        releases.insert(
            String::from("147"),
            ModRelease {
                id: String::from("2"),
                version: String::from("2.0"),
            },
        );
        releases.insert(
            String::from("146.1"),
            ModRelease {
                id: String::from("11"),
                version: String::from("1.1"),
            },
        );
        releases.insert(
            String::from("146"),
            ModRelease {
                id: String::from("1"),
                version: String::from("1.0"),
            },
        );
        let listing = ModListing {
            releases,
            ..ModListing::default()
        };
        // Insertion order decides: `146.1` precedes `146`, so an exact revision
        // match returns it, while a non-matching revision falls through to the
        // unspecified-minor entry.
        let release = listing.get_matching_release(146, 1).expect("match 146.1");
        assert_eq!(release.id, "11");
        let release = listing.get_matching_release(146, 3).expect("match 146");
        assert_eq!(release.id, "1", "first matching entry wins, like ArrayMap");
        assert!(listing.get_matching_release(140, 0).is_none());
        // A tag-form key is not a version string and must not match.
        assert!(listing.get_matching_release(999, 0).is_none());
        // `release_matches` uses the anchored tag parser (`[vN]`/`[bN.M]`).
        assert!(listing.release_matches("Something [v146]", 146, 0));
        assert!(!listing.release_matches("Something [v146]", 147, 0));
        // An unanchored tag parses to `None`, which matches every build.
        assert!(listing.release_matches("inline [v146] tag", 146, 0));
    }

    /// An anchored major below 15 stops the tag scan (upstream returns null).
    #[test]
    fn anchored_low_major_stops_scan() {
        assert_eq!(parse_version_tag("[v7]"), None);
        assert_eq!(parse_version_tag("[v7] then [v160] at end"), None);
        assert_eq!(parse_version_tag("start [v7]"), None);
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
            "releases": {"146": {"id": 1, "version": "1.0"}}
        }"#;
        let listing: ModListing = serde_json::from_str(json).expect("parse listing");
        assert_eq!(listing.repo, "owner/repo");
        assert_eq!(listing.internal_name, "bingus");
        assert_eq!(listing.stars, 12);
        assert_eq!(listing.tags, vec!["utility"]);
        assert_eq!(listing.releases.len(), 1);
        // Numeric `id` from `mods.json` becomes the upstream string field.
        assert_eq!(listing.releases["146"].id, "1");
    }

    /// The real `mods.json` shape: numeric release ids, absent optional keys,
    /// plain-version release keys. `getMatchingRelease` must pick a release.
    #[test]
    fn real_mods_json_shape_matches_release() {
        let json = r#"{
            "repo": "cardillan/mlogassertions",
            "internalName": "mlog-assertions-8",
            "name": "Mlog Dev Tools",
            "author": "cardillan",
            "lastUpdated": "2026-10-03T18:50:14Z",
            "stars": 4,
            "version": "0.11.2",
            "minGameVersion": "154.2",
            "hasIcon": true,
            "hasScripts": false,
            "hasJava": true,
            "description": "tools",
            "releases": {
                "154.2": {"id": 389859853, "version": "0.9.0"},
                "155": {"id": 396067194, "version": "0.10.21"},
                "160": {"id": 402587569, "version": "0.11.2"}
            }
        }"#;
        let listing: ModListing = serde_json::from_str(json).expect("parse listing");
        assert_eq!(listing.icon_hash, "");
        assert!(listing.tags.is_empty());
        assert!(!listing.legacy_compatible);
        assert_eq!(listing.releases["160"].id, "402587569");
        let release = listing.get_matching_release(160, 0).expect("match 160");
        assert_eq!(release.version, "0.11.2");
        let release = listing.get_matching_release(155, 0).expect("match 155");
        assert_eq!(release.version, "0.10.21");
        // A build with no version-specific release falls back to `/latest`.
        assert!(listing.get_matching_release(146, 0).is_none());
    }
}
