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

/// Rewrites Arc's lenient `Json` text into strict JSON: bare object keys
/// (`{teams:{0:{}}}`) and bare string values (`type:dagger`, `text:@gz.conveyors`)
/// are quoted, while literals (`true`/`false`/`null`) and numbers are left
/// alone. String literals are copied verbatim. Used by the legacy `MSAV`
/// import for `Rules`/game data; strict JSON is unchanged.
pub fn quote_bare_keys(text: &str) -> String {
    fn is_bare(ch: char) -> bool {
        ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '@' | '#')
    }
    fn is_literal(token: &str) -> bool {
        matches!(token, "true" | "false" | "null") || token.parse::<f64>().is_ok()
    }

    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 64);
    let mut index = 0;
    while index < chars.len() {
        let current = chars[index];
        if current == '"' {
            out.push(current);
            index += 1;
            while index < chars.len() {
                out.push(chars[index]);
                if chars[index] == '\\' && index + 1 < chars.len() {
                    index += 1;
                    out.push(chars[index]);
                } else if chars[index] == '"' {
                    index += 1;
                    break;
                }
                index += 1;
            }
            continue;
        }
        if is_bare(current) {
            let start = index;
            while index < chars.len() && is_bare(chars[index]) {
                index += 1;
            }
            let token: String = chars[start..index].iter().collect();
            let mut next = index;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            let key = next < chars.len() && chars[next] == ':';
            if key || !is_literal(&token) {
                out.push('"');
                out.push_str(&token);
                out.push('"');
            } else {
                out.push_str(&token);
            }
            continue;
        }
        out.push(current);
        index += 1;
    }
    out
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

    #[test]
    fn arc_json_quoting_produces_strict_json() {
        let arc = r#"{teams:{0:{},1:{}},type:dagger,text:@gz.conveyors,class:CoreItem,
            x:10.24,neg:-2,arr:[1,true,false,null,{a:b}],quoted:"a:b"}"#;
        let strict = super::quote_bare_keys(arc);
        let parsed: Value = serde_json::from_str(&strict).expect("strict json");
        assert_eq!(parsed["teams"]["0"], Value::Object(Default::default()));
        assert_eq!(parsed["type"], Value::String("dagger".to_owned()));
        assert_eq!(parsed["text"], Value::String("@gz.conveyors".to_owned()));
        assert_eq!(parsed["neg"], serde_json::json!(-2));
        assert_eq!(parsed["arr"][4]["a"], Value::String("b".to_owned()));
        assert_eq!(parsed["quoted"], Value::String("a:b".to_owned()));
    }
}
