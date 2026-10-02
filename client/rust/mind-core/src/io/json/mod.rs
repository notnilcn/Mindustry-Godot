// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `JsonIO` equivalent (plan 04 §3.6): serde-based JSON for rules, stats,
//! settings, map tags and objectives.
//!
//! Ported from `core/src/mindustry/io/JsonIO.java`. M6 completes the class-tag
//! registry, content serializers with upstream fallbacks, and the full
//! `Rules`/`GameStats`/`MapLocales`/`SectorInfo` field sets. `writeBytes`/
//! `readBytes` (UBJson) are deliberately not ported (plan 04 §2.3 deviation 5:
//! no production callers).

pub mod content_serde;
pub mod objectives;
pub mod rules;

pub use content_serde::{
    ColorHex, JsonItemStack, MusicContainer, SectorKey, attribute_from_name, attribute_name,
    block_from_name, content_name, is_unlockable, item_from_name, liquid_from_name,
    planet_from_name, sector_preset_from_name, status_from_name, team_from_id, team_id,
    unit_from_name, unlockable_from_name, weather_from_name,
};
pub use objectives::{
    BuildCountObjective, ClassTagRegistry, CommandModeObjective, CoreItemObjective,
    DestroyBlockObjective, DestroyBlocksObjective, DestroyCoreObjective, DestroyUnitsObjective,
    FlagObjective, ItemObjective, LightMarker, LineMarker, MapObjective, MapObjectives,
    MarkerCommon, ObjectiveCommon, ObjectiveMarker, Point2, PointMarker, ProduceObjective,
    QuadMarker, ResearchObjective, ShapeMarker, ShapeTextMarker, TextMarker, TextureHolder,
    TextureMarker, TimerObjective, UnitCountObjective, Vec2,
};
pub use rules::{
    Attributes, DEFAULT_ENV, ExportStat, GameStats, MapLocales, NEVER, PlanetParams, Rules,
    SectorInfo, SpawnGroup, TIME_TO_MINUTES, TeamRule, TeamRules, WeatherEntry,
};

use std::borrow::Cow;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::IoResult;

/// Legacy class-name prefix stripped on read (`JsonIO.read`).
pub const LEGACY_PREFIX: &str = "io.anuke.";

/// Strips the legacy `io.anuke.` class-name prefix.
pub fn strip_legacy_prefix(input: &str) -> Cow<'_, str> {
    if input.contains(LEGACY_PREFIX) {
        Cow::Owned(input.replace(LEGACY_PREFIX, ""))
    } else {
        Cow::Borrowed(input)
    }
}

/// `JsonIO` static API (`write`, `read`, `read_into`, `copy`, `print`).
pub struct JsonIo;

impl JsonIo {
    /// `JsonIO.write`: compact JSON with upstream field order.
    pub fn write<T: Serialize>(value: &T) -> IoResult<String> {
        Ok(serde_json::to_string(value)?)
    }

    /// `JsonIO.read`: strips the legacy prefix, then parses.
    pub fn read<T: DeserializeOwned>(string: &str) -> IoResult<T> {
        Ok(serde_json::from_str(&strip_legacy_prefix(string))?)
    }

    /// `JsonIO.read(base, string)` (`json.readFields`): overlays the parsed
    /// fields onto `base`, recursing into nested objects.
    pub fn read_into<T: Serialize + DeserializeOwned>(base: T, string: &str) -> IoResult<T> {
        let mut base_value = serde_json::to_value(&base)?;
        let overlay: Value = serde_json::from_str(&strip_legacy_prefix(string))?;
        merge_values(&mut base_value, overlay);
        Ok(serde_json::from_value(base_value)?)
    }

    /// `JsonIO.copy` (`json.copyFields` semantics via a JSON round-trip).
    ///
    /// Note: `Rules::copy` uses `Clone` instead (plan 04 deviation 8); this
    /// helper exists for parity and tests.
    pub fn copy<T: Serialize + DeserializeOwned>(value: &T) -> IoResult<T> {
        Self::read(&Self::write(value)?)
    }

    /// `JsonIO.print` (`json.prettyPrint`).
    pub fn pretty(string: &str) -> IoResult<String> {
        let value: Value = serde_json::from_str(&strip_legacy_prefix(string))?;
        Ok(serde_json::to_string_pretty(&value)?)
    }
}

/// Recursive object overlay used by [`JsonIo::read_into`]. Non-object values
/// (arrays, scalars, null) replace the base value, matching Arc's field
/// reads where nested object fields are read into the existing instance.
fn merge_values(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, value) in overlay_map {
                match base_map.get_mut(&key) {
                    Some(base_value) => merge_values(base_value, value),
                    None => {
                        base_map.insert(key, value);
                    }
                }
            }
        }
        (base_value, overlay_value) => *base_value = overlay_value,
    }
}

/// Ordered string map re-export for JSON shapes (`Arc StringMap`).
pub use super::StringMap as JsonStringMap;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_legacy_prefix_only_when_present() {
        assert_eq!(strip_legacy_prefix("io.anuke.Rules"), "Rules");
        assert_eq!(
            strip_legacy_prefix("mindustry.game.Rules"),
            "mindustry.game.Rules"
        );
    }

    #[test]
    fn read_strips_legacy_class_names() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct Holder {
            #[serde(rename = "class")]
            class: String,
        }
        let holder: Holder = JsonIo::read(r#"{"class":"io.anuke.Rules"}"#).unwrap();
        assert_eq!(holder.class, "Rules");
    }

    #[test]
    fn read_into_overlays_fields_recursively() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Default)]
        struct Inner {
            a: i32,
            b: i32,
        }
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Default)]
        struct Outer {
            name: String,
            inner: Inner,
        }
        let base = Outer {
            name: "base".to_owned(),
            inner: Inner { a: 1, b: 2 },
        };
        let out = JsonIo::read_into(base, r#"{"inner":{"a":9},"future":true}"#).unwrap();
        assert_eq!(out.name, "base");
        assert_eq!(out.inner, Inner { a: 9, b: 2 });
    }

    #[test]
    fn write_is_compact_and_pretty_roundtrips() {
        let map: super::super::StringMap = [("a".to_owned(), "b".to_owned())].into_iter().collect();
        let json = JsonIo::write(&map).unwrap();
        assert_eq!(json, r#"{"a":"b"}"#);
        assert!(JsonIo::pretty(&json).unwrap().contains('\n'));
        assert_eq!(JsonIo::copy(&map).unwrap(), map);
    }
}
