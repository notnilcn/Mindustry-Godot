// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map metadata (`mindustry.maps.Map`, plan 06 §3.9).
//!
//! Ported from `core/src/mindustry/maps/Map.java`: tags, `rules()` overlay with
//! the planet/env fallbacks, `filters()` (M5), preview/cache paths and
//! `compareTo` sort order. Pixel/texture state (`Map.texture`) is plan 19.

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::path::PathBuf;

use indexmap::IndexMap;

use crate::content::ContentRegistry;
use crate::io::json::{JsonIo, Rules};
use crate::util::strings::strip_colors;
use crate::world::MapGenHooks;

use super::error::MapError;
use super::preview;

/// `Env.scorching` bit (`mindustry.world.meta.Env`).
pub const ENV_SCORCHING: i32 = 1 << 4;
/// The default bundle "unknown" string (`Map.tag`).
pub const UNKNOWN: &str = "unknown";

/// One map/SaveMeta record (`mindustry.maps.Map`).
#[derive(Debug, Clone)]
pub struct Map {
    /// Whether this is a custom (user) map.
    pub custom: bool,
    /// Metadata tags (`name`/`author`/`description`/`rules`/`genfilters`/`build`).
    pub tags: IndexMap<String, String>,
    /// Base file of this map (path can be anything).
    pub file: PathBuf,
    /// Save format version of the file.
    pub version: i32,
    /// Build that created this map (`-1` = unknown/custom).
    pub build: i32,
    /// Whether this map is Steam-workshop managed.
    pub workshop: bool,
    /// Owning mod id (`Map.mod`; plan 20 supplies).
    pub mod_id: Option<String>,
    /// Map width in tiles.
    pub width: i32,
    /// Map height in tiles.
    pub height: i32,
    /// Teams present on the map (filled by preview generation).
    pub teams: BTreeSet<u8>,
    /// Enemy spawn-overlay count.
    pub spawns: u32,
}

impl Map {
    /// Builds a map (`Map(Fi,width,height,tags,custom,version,build)`).
    pub fn new(
        file: PathBuf,
        width: i32,
        height: i32,
        tags: IndexMap<String, String>,
        custom: bool,
        version: i32,
        build: i32,
    ) -> Self {
        Self {
            custom,
            tags,
            file,
            version,
            build,
            workshop: false,
            mod_id: None,
            width,
            height,
            teams: BTreeSet::new(),
            spawns: 0,
        }
    }

    /// File stem (`Fi.nameWithoutExtension`).
    pub fn file_stem(&self) -> Option<String> {
        self.file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
    }

    /// `Map.name`.
    pub fn name(&self) -> &str {
        self.tag("name")
    }

    /// `Map.author`.
    pub fn author(&self) -> &str {
        self.tag("author")
    }

    /// `Map.description`.
    pub fn description(&self) -> &str {
        self.tag("description")
    }

    /// `Map.plainName` (color-stripped).
    pub fn plain_name(&self) -> String {
        strip_colors(self.name())
    }

    /// `Map.plainAuthor`.
    pub fn plain_author(&self) -> String {
        strip_colors(self.author())
    }

    /// `Map.plainDescription`.
    pub fn plain_description(&self) -> String {
        strip_colors(self.description())
    }

    /// `Map.tag(name)`: the value or `"unknown"`.
    pub fn tag(&self, name: &str) -> &str {
        if self.has_tag(name) {
            self.tags.get(name).map(String::as_str).unwrap_or(UNKNOWN)
        } else {
            UNKNOWN
        }
    }

    /// `Map.hasTag`: present and non-empty after trimming.
    pub fn has_tag(&self, name: &str) -> bool {
        self.tags
            .get(name)
            .is_some_and(|value| !value.trim().is_empty())
    }

    /// `Map.getSteamID`.
    pub fn steam_id(&self) -> Option<&str> {
        self.tags.get("steamid").map(String::as_str)
    }

    /// Whether the map's `build` tag predates filter support (`build < 83`).
    pub fn build_lt_83(&self) -> bool {
        let build = self.tag_i32("build", -1);
        build != -1 && build < 83
    }

    /// Reads an integer tag with a default (`tags.getInt`).
    pub fn tag_i32(&self, name: &str, default: i32) -> i32 {
        self.tags
            .get(name)
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(default)
    }

    /// The raw `genfilters` JSON, empty → `None`.
    pub fn genfilters_json(&self) -> Option<&str> {
        self.tags
            .get("genfilters")
            .map(String::as_str)
            .filter(|value| !value.is_empty())
    }

    /// `Map.rules(base)`: JSON overlay + planet/env/spawn fallbacks.
    pub fn rules(&self, base: Rules, hooks: &dyn MapGenHooks) -> Rules {
        let json = self.tags.get("rules").map(String::as_str).unwrap_or("{}");
        let mut result = match JsonIo::read_into(base, json) {
            Ok(rules) => rules,
            Err(error) => {
                log::error!("failed to read rules for map `{}`: {error}", self.name());
                return Rules::default();
            }
        };

        // Replace the default serpulo env with erekir when the map is scorching.
        if result.planet == "serpulo" && (result.env & ENV_SCORCHING) != 0 {
            result.planet = "erekir".to_owned();
        }
        if result.planet.is_empty() {
            result.planet = "serpulo".to_owned();
        }
        if result.spawns.is_empty() {
            result.spawns = hooks.wave_groups();
        }
        result
    }

    /// `Map.filters()`: the generation filters used on load.
    ///
    /// Maps with a `build` tag in `1..83` and no `genfilters` use an empty
    /// stack (upstream legacy rule); otherwise the `genfilters` JSON (or the
    /// default stack when absent) is parsed.
    pub fn filters(
        &self,
        content: &ContentRegistry,
    ) -> Vec<Box<dyn super::filters::GenerateFilter>> {
        if self.build_lt_83() && self.genfilters_json().is_none() {
            Vec::new()
        } else {
            super::filters::read_filters(content, self.genfilters_json().unwrap_or(""))
        }
    }

    /// `Map.compareTo`: workshop, then custom, then PvP, then plain name.
    pub fn compare_to(&self, other: &Map) -> Ordering {
        // `-Boolean.compare(a, b)` == `b.cmp(a)`.
        let work = other.workshop.cmp(&self.workshop);
        if work != Ordering::Equal {
            return work;
        }
        let kind = other.custom.cmp(&self.custom);
        if kind != Ordering::Equal {
            return kind;
        }
        let modes = super::shuffle::is_pvp(self).cmp(&super::shuffle::is_pvp(other));
        if modes != Ordering::Equal {
            return modes;
        }
        self.plain_name().cmp(&other.plain_name())
    }

    /// Preview image path (`Map.previewFile`).
    pub fn preview_file(&self, paths: &crate::io::fs::Paths) -> PathBuf {
        preview::preview_file(paths, self)
    }

    /// Cache file path (`Map.cacheFile`).
    pub fn cache_file(&self, paths: &crate::io::fs::Paths) -> PathBuf {
        preview::cache_file(paths, self)
    }

    /// Builds a [`Map`] from an IO header (`MapIO.createMap`).
    pub fn from_header(header: &crate::io::map::MapHeader, custom: bool) -> Self {
        Self {
            custom,
            tags: header.tags.clone(),
            file: header.file.clone().unwrap_or_default(),
            version: header.version,
            build: header.build,
            workshop: false,
            mod_id: None,
            width: header.width,
            height: header.height,
            teams: header.teams.clone(),
            spawns: header.spawns,
        }
    }

    /// Validates a loaded map (`Maps.loadMap` empty-name check).
    pub fn validate_name(&self) -> Result<(), MapError> {
        if self
            .tags
            .get("name")
            .is_none_or(|name| name.trim().is_empty())
        {
            return Err(MapError::EmptyName(self.file.clone()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::NoopMapGenHooks;

    fn tags(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        let mut tags = IndexMap::new();
        for (key, value) in pairs {
            tags.insert((*key).to_owned(), (*value).to_owned());
        }
        tags
    }

    /// `maps::tests::map_tags_rules_fallback`.
    #[test]
    fn map_tags_and_rules_fallback() {
        let map = Map::new(
            PathBuf::from("/maps/fork.msav"),
            16,
            16,
            tags(&[("name", "[red]Fork"), ("author", "me")]),
            false,
            1,
            100,
        );
        assert_eq!(map.name(), "[red]Fork");
        assert_eq!(map.plain_name(), "Fork");
        assert_eq!(map.author(), "me");
        assert_eq!(map.description(), UNKNOWN);
        assert!(map.has_tag("name"));
        assert!(!map.has_tag("description"));

        // Default planet fallback is serpulo.
        let hooks = NoopMapGenHooks;
        let rules = map.rules(Rules::default(), &hooks);
        assert_eq!(rules.planet, "serpulo");

        // Scorching serpulo flips to erekir.
        let scorching = Rules {
            planet: "serpulo".to_owned(),
            env: ENV_SCORCHING,
            ..Rules::default()
        };
        let mut map2 = map.clone();
        map2.tags.insert("rules".to_owned(), "{}".to_owned());
        let rules2 = map2.rules(scorching, &hooks);
        assert_eq!(rules2.planet, "erekir");
    }

    #[test]
    fn compare_to_orders_custom_workshop_pvp_name() {
        let builtin = Map::new(
            PathBuf::from("/maps/fork.msav"),
            1,
            1,
            tags(&[("name", "Fork")]),
            false,
            1,
            -1,
        );
        let custom = Map::new(
            PathBuf::from("/maps/mine.msav"),
            1,
            1,
            tags(&[("name", "Mine")]),
            true,
            1,
            -1,
        );
        // Custom sorts before built-in (negated Boolean.compare).
        assert_eq!(custom.compare_to(&builtin), Ordering::Less);
        assert_eq!(builtin.compare_to(&custom), Ordering::Greater);
        // PvP built-in sorts after non-PvP built-in.
        let veins = Map::new(
            PathBuf::from("/maps/veins.msav"),
            1,
            1,
            tags(&[("name", "Veins")]),
            false,
            1,
            -1,
        );
        assert_eq!(builtin.compare_to(&veins), Ordering::Less);
    }

    /// `maps::tests::build_lt_83_filters_empty`.
    #[test]
    fn build_lt_83_filters_empty() {
        let content = crate::content::test_support::test_registry();
        let legacy = Map::new(
            PathBuf::from("/maps/old.msav"),
            1,
            1,
            tags(&[("name", "Old"), ("build", "82")]),
            true,
            1,
            82,
        );
        assert!(legacy.filters(&content).is_empty());
        // A modern map with no genfilters gets the default stack.
        let modern = Map::new(
            PathBuf::from("/maps/new.msav"),
            1,
            1,
            tags(&[("name", "New"), ("build", "100")]),
            true,
            1,
            100,
        );
        assert!(!modern.filters(&content).is_empty());
    }

    #[test]
    fn build_lt_83_and_genfilters_json() {
        let mut map = Map::new(
            PathBuf::from("/maps/x.msav"),
            1,
            1,
            tags(&[("name", "X"), ("build", "82")]),
            true,
            1,
            82,
        );
        assert!(map.build_lt_83());
        assert!(map.genfilters_json().is_none());
        map.tags.insert("build".to_owned(), "84".to_owned());
        assert!(!map.build_lt_83());
        map.tags.insert("genfilters".to_owned(), "[]".to_owned());
        assert_eq!(map.genfilters_json(), Some("[]"));
    }
}
