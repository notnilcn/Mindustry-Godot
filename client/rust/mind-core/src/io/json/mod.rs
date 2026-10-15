// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `JsonIO` equivalent (plan 04 §3.6): serde-based JSON for rules, stats,
//! settings, map tags and objectives.
//!
//! Ported from `core/src/mindustry/io/JsonIO.java`. M6 completes the class-tag
//! registry, content serializers with upstream fallbacks, and the full
//! `Rules`/`GameStats`/`MapLocales`/`SectorInfo` field sets. `writeBytes` is
//! not ported (plan 04 §2.3 deviation 5); the UBJson *reader* ([`ubjson`])
//! decodes Java `Settings.putJson` payloads from imported profiles.

pub mod content_serde;
pub mod objectives;
pub mod rules;
pub mod ubjson;

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
            // Arc `Json` writes non-finite floats as bare literals; strict JSON
            // has no `Infinity`, and the port's JSON model stores Java's
            // `Float.POSITIVE_INFINITY` as `NEVER_F32` (`i32::MAX as f32`, see
            // `io::json::rules::NEVER_F32`) - the legacy import maps it there.
            // (`NaN` has no sentinel; none of the shipped maps use it.)
            if !key && (token == "Infinity" || token == "-Infinity") {
                out.push_str(if token == "Infinity" {
                    "2147483648.0"
                } else {
                    "-2147483648.0"
                });
                continue;
            }
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

/// Arc's `Json.readValue` coercion: a number written as a whole float (`1.0`)
/// reads into an integer field. GDScript's `JSON.stringify` writes every number
/// as a float, so the Godot-built rules blobs need this. Floats with a
/// fractional part, non-finite values and out-of-range wholes are left alone.
pub fn coerce_whole_floats(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                coerce_whole_floats(item);
            }
        }
        Value::Object(map) => {
            for (_, item) in map.iter_mut() {
                coerce_whole_floats(item);
            }
        }
        Value::Number(number) => {
            let Some(float) = number.as_f64() else {
                return;
            };
            if !float.is_finite() || float.fract() != 0.0 {
                return;
            }
            let Ok(whole) = i64::try_from(float as i128) else {
                return;
            };
            *number = serde_json::Number::from(whole);
        }
        _ => {}
    }
}

impl JsonIo {
    /// `JsonIO.write`: compact JSON with upstream field order.
    pub fn write<T: Serialize>(value: &T) -> IoResult<String> {
        Ok(serde_json::to_string(value)?)
    }

    /// `JsonIO.read`: strips the legacy prefix, then parses. Arc's lenient
    /// `Json` text (bare keys/strings, what Java writes into save meta
    /// `rules`) is accepted by retrying through [`quote_bare_keys`]; whole
    /// floats are coerced to integers like Arc's `Json.readValue(int.class)`
    /// (GDScript's `JSON.stringify` writes every number as a float).
    pub fn read<T: DeserializeOwned>(string: &str) -> IoResult<T> {
        let text = strip_legacy_prefix(string);
        let parsed = match serde_json::from_str::<Value>(&text) {
            Ok(value) => Ok(value),
            Err(strict) => serde_json::from_str::<Value>(&quote_bare_keys(&text))
                .map_err(|_| crate::io::IoError::from(strict)),
        };
        match parsed {
            Ok(mut value) => {
                coerce_whole_floats(&mut value);
                Ok(serde_json::from_value(value)?)
            }
            Err(error) => Err(error),
        }
    }

    /// `JsonIO.read(base, string)` (`json.readFields`): overlays the parsed
    /// fields onto `base`, recursing into nested objects.
    ///
    /// Arc's lenient `Json` text (bare keys/strings; the authored maps' `rules`
    /// tag) goes through [`quote_bare_keys`] like [`Self::read`], so a map
    /// overlay never silently falls back to `Rules::default()`; whole floats
    /// are coerced to integers for Arc's numeric coercion.
    pub fn read_into<T: Serialize + DeserializeOwned>(base: T, string: &str) -> IoResult<T> {
        let mut base_value = serde_json::to_value(&base)?;
        let text = strip_legacy_prefix(string);
        let mut overlay: Value = match serde_json::from_str::<Value>(&text) {
            Ok(value) => value,
            Err(strict) => serde_json::from_str::<Value>(&quote_bare_keys(&text))
                .map_err(|_| crate::io::IoError::from(strict))?,
        };
        coerce_whole_floats(&mut overlay);
        coerce_whole_floats(&mut base_value);
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

    /// Arc's `Json.readValue` coercion: a whole float (`1.0`) reads into an
    /// integer field, so a rules blob round-tripped through GDScript (which
    /// stringifies every number as a float) still parses.
    #[test]
    fn read_coerces_whole_floats_for_integer_fields() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug, Default)]
        struct Holder {
            team: u8,
            #[serde(default)]
            index: i32,
            value: f32,
        }
        let holder: Holder = JsonIo::read(r#"{"team": 1.0, "index": -2.0, "value": 1.5}"#).unwrap();
        assert_eq!(holder.team, 1);
        assert_eq!(holder.index, -2);
        assert_eq!(holder.value, 1.5);
    }

    /// The authored maps' `rules` tag is Arc-lenient text (bare keys/strings);
    /// `Map.rules(base)` must overlay it instead of falling back to defaults.
    #[test]
    fn read_into_accepts_arc_bare_keys() {
        use content_serde::JsonItemStack;

        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct MapRules {
            waves: bool,
            spawns: Vec<String>,
            #[serde(default)]
            loadout: Vec<JsonItemStack>,
        }
        fn rules(waves: bool) -> MapRules {
            MapRules {
                waves,
                spawns: Vec::new(),
                loadout: Vec::new(),
            }
        }
        let arc = "{spawns:[dagger,crawler],loadout:[{item:copper,amount:200}],waves:false}";
        let out = JsonIo::read_into(rules(true), arc).unwrap();
        assert!(!out.waves, "bare value overrides the base");
        assert_eq!(out.spawns, vec!["dagger".to_owned(), "crawler".to_owned()]);
        assert_eq!(out.loadout.len(), 1);
        assert_eq!(out.loadout[0].item.as_deref(), Some("copper"));
        assert_eq!(out.loadout[0].amount, 200);
        // Strict JSON keeps working (no regression for modern tags).
        let out =
            JsonIo::read_into(rules(true), r#"{"loadout":[{"item":"lead","amount":5}]}"#).unwrap();
        assert!(out.waves);
        assert_eq!(out.loadout[0].item.as_deref(), Some("lead"));
        assert_eq!(out.loadout[0].amount, 5);
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

    #[test]
    fn arc_json_bare_infinity_maps_to_never_f32() {
        // Arc `Json` writes non-finite floats bare; strict JSON has no
        // `Infinity` (PAR-0155: the authored campaign maps carry
        // `scaling:Infinity` rules).
        let arc = r#"{scaling:Infinity,neg:-Infinity,finite:1.5}"#;
        let strict = super::quote_bare_keys(arc);
        let parsed: Value = serde_json::from_str(&strict).expect("strict json");
        assert_eq!(
            parsed["scaling"].as_f64(),
            Some(crate::io::json::rules::NEVER_F32 as f64)
        );
        assert_eq!(
            parsed["neg"].as_f64(),
            Some(-(crate::io::json::rules::NEVER_F32 as f64))
        );
        assert_eq!(parsed["finite"].as_f64(), Some(1.5));
    }
}
