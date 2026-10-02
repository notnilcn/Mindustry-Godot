// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Save metadata (plan 04 §3.2, keys in §6.2).
//!
//! Ported from `core/src/mindustry/io/SaveMeta.java`. The upstream `rules`
//! field is a parsed `Rules` object; plan 12 owns that type, so the native meta
//! keeps the raw rules JSON string (parsed lazily by consumers, same net
//! effect as upstream's lazy usage in save lists).

use super::super::StringMap;
use super::IoError;

/// Meta tags parsed from a save/map file header.
#[derive(Debug, Clone)]
pub struct SaveMeta {
    /// Content revision written into the tags (`version` key).
    pub version: i32,
    /// Wall-clock save timestamp, millis (`saved`).
    pub timestamp: i64,
    /// Accumulated playtime, millis (`playtime`).
    pub time_played: i64,
    /// Build number (`build`).
    pub build: i32,
    /// Map name (`mapname`).
    pub map_name: String,
    /// Wave number (`wave`).
    pub wave: i32,
    /// Raw rules JSON (`rules`); parsed by consumers via `JsonIo` (plan 04 M6+).
    pub rules_json: String,
    /// All meta tags (including arbitrary `extra_tags` / map tags).
    pub tags: StringMap,
    /// Mod name list parsed from the `mods` JSON array.
    pub mods: Vec<String>,
}

impl SaveMeta {
    /// Builds meta from a tag map (`SaveVersion.getMeta`).
    ///
    /// Missing numeric keys default to 0 (Arc `StringMap.getInt/getLong`).
    pub fn from_tags(version: i32, tags: StringMap) -> Result<Self, IoError> {
        let rules_json = tags
            .get("rules")
            .cloned()
            .unwrap_or_else(|| "{}".to_owned());
        let mods = match tags.get("mods") {
            Some(raw) => serde_json::from_str(raw).unwrap_or_default(),
            None => Vec::new(),
        };
        Ok(Self {
            version,
            timestamp: tag_i64(&tags, "saved"),
            time_played: tag_i64(&tags, "playtime"),
            build: tag_i32(&tags, "build"),
            map_name: tags.get("mapname").cloned().unwrap_or_default(),
            wave: tag_i32(&tags, "wave"),
            rules_json,
            mods,
            tags,
        })
    }

    /// `SaveMeta.isMap`: true when the tags contain a `name` key (map files
    /// carry their display name; saves do not).
    pub fn is_map(&self) -> bool {
        self.tags.contains_key("name")
    }

    /// Integer tag with default (`StringMap.getInt`).
    pub fn tag_i32(&self, key: &str) -> i32 {
        tag_i32(&self.tags, key)
    }

    /// Long tag with default (`StringMap.getLong`).
    pub fn tag_i64(&self, key: &str) -> i64 {
        tag_i64(&self.tags, key)
    }

    /// Map width in tiles (`width` tag; 0 when absent).
    pub fn width(&self) -> i32 {
        self.tag_i32("width")
    }

    /// Map height in tiles (`height` tag; 0 when absent).
    pub fn height(&self) -> i32 {
        self.tag_i32("height")
    }
}

fn tag_i32(tags: &StringMap, key: &str) -> i32 {
    tags.get(key)
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(0)
}

fn tag_i64(tags: &StringMap, key: &str) -> i64 {
    tags.get(key)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_tags_defaults_and_parsing() {
        let mut tags = StringMap::new();
        tags.insert("version".to_owned(), "1".to_owned());
        tags.insert("saved".to_owned(), "1700000000000".to_owned());
        tags.insert("playtime".to_owned(), "60000".to_owned());
        tags.insert("build".to_owned(), "146".to_owned());
        tags.insert("mapname".to_owned(), "groundZero".to_owned());
        tags.insert("wave".to_owned(), "5".to_owned());
        tags.insert("mods".to_owned(), "[\"foo\",\"bar\"]".to_owned());
        tags.insert("width".to_owned(), "256".to_owned());

        let meta = SaveMeta::from_tags(1, tags).unwrap();
        assert_eq!(meta.version, 1);
        assert_eq!(meta.timestamp, 1_700_000_000_000);
        assert_eq!(meta.time_played, 60_000);
        assert_eq!(meta.build, 146);
        assert_eq!(meta.map_name, "groundZero");
        assert_eq!(meta.wave, 5);
        assert_eq!(meta.mods, vec!["foo", "bar"]);
        assert_eq!(meta.width(), 256);
        assert_eq!(meta.height(), 0);
        assert!(!meta.is_map());
    }

    #[test]
    fn is_map_requires_name_tag() {
        let mut tags = StringMap::new();
        tags.insert("name".to_owned(), "My Map".to_owned());
        let meta = SaveMeta::from_tags(1, tags).unwrap();
        assert!(meta.is_map());
        assert_eq!(meta.rules_json, "{}");
    }

    #[test]
    fn junk_numbers_fall_back_to_zero() {
        let mut tags = StringMap::new();
        tags.insert("wave".to_owned(), "not-a-number".to_owned());
        let meta = SaveMeta::from_tags(1, tags).unwrap();
        assert_eq!(meta.wave, 0);
    }
}
