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

/// Rewrites Arc's lenient `Json` text into strict JSON (what `JsonReader`
/// accepts and `JsonWriter.OutputType.minimal` emits).
///
/// Arc's minimal writer leaves names and values unquoted whenever
/// `minimalNamePattern`/`minimalValuePattern` match (`JsonWriter.java`,
/// `OutputType.minimal`); `JsonReader.parse` then lexes unquoted names up to
/// `:`/CR/LF and unquoted values up to `}`, `]`, `,`, CR or LF, classifies the
/// token as `true`/`false`/`null`, a number (only `0-9`, `-`, `+`, `.`, `e`,
/// `E` characters, parsed as long/double) or otherwise a string. This port
/// re-emits that token stream as strict JSON: bare names and string values are
/// quoted (escape sequences are decoded with Arc's `unescape` then re-encoded),
/// keywords stay bare, and numeric tokens stay bare only when they are valid
/// strict JSON numbers - Arc's own `JsonValue` keeps the raw text for numbers
/// (`asString()`), and older saves such as `ambientLight:00000000` rely on
/// that (the port's `ColorHex` reads the hex string).
///
/// Arc writes non-finite floats bare (`Infinity`/`-Infinity`); strict JSON has
/// none, and the port's JSON model stores Java's `Float.POSITIVE_INFINITY` as
/// `NEVER_F32` (`i32::MAX as f32`), so those two tokens map to its numeric
/// sentinel. Strict JSON input is copied through unchanged.
pub fn quote_bare_keys(text: &str) -> String {
    ArcMinimal::new(text).convert()
}

/// Token-level converter behind [`quote_bare_keys`].
struct ArcMinimal {
    chars: Vec<char>,
    pos: usize,
    out: String,
}

impl ArcMinimal {
    fn new(text: &str) -> Self {
        Self {
            chars: text.chars().collect(),
            pos: 0,
            out: String::with_capacity(text.len() + 64),
        }
    }

    fn convert(mut self) -> String {
        self.skip_ws();
        if self.pos < self.chars.len() {
            self.value();
        }
        self.out
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.pos += 1;
        }
    }

    fn value(&mut self) {
        self.skip_ws();
        match self.peek() {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => self.quoted(),
            Some(_) => self.unquoted_value(),
            None => {}
        }
    }

    fn object(&mut self) {
        self.out.push('{');
        self.pos += 1;
        let mut first = true;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('}') => {
                    self.pos += 1;
                    self.out.push('}');
                    return;
                }
                None => return,
                Some(_) => {}
            }
            if !first {
                self.out.push(',');
            }
            first = false;
            self.name();
            self.skip_ws();
            if self.peek() == Some(':') {
                self.pos += 1;
                self.out.push(':');
            }
            self.value();
            self.skip_ws();
            if self.peek() == Some(',') {
                self.pos += 1;
            }
        }
    }

    fn array(&mut self) {
        self.out.push('[');
        self.pos += 1;
        let mut first = true;
        loop {
            self.skip_ws();
            match self.peek() {
                Some(']') => {
                    self.pos += 1;
                    self.out.push(']');
                    return;
                }
                None => return,
                Some(_) => {}
            }
            if !first {
                self.out.push(',');
            }
            first = false;
            self.value();
            self.skip_ws();
            if self.peek() == Some(',') {
                self.pos += 1;
            }
        }
    }

    /// Arc unquoted name: up to `:`, CR, LF or a comment (`JsonReader`
    /// `unquotedChars` name branch); trailing whitespace is trimmed.
    fn name(&mut self) {
        if self.peek() == Some('"') {
            self.quoted();
            return;
        }
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch == ':' || ch == '\r' || ch == '\n' {
                break;
            }
            if ch == '/'
                && self
                    .chars
                    .get(self.pos + 1)
                    .is_some_and(|c| *c == '/' || *c == '*')
            {
                break;
            }
            self.pos += 1;
        }
        let raw: String = self.chars[start..self.pos].iter().collect();
        let trimmed = raw.trim_end();
        write_json_string(&mut self.out, trimmed);
    }

    /// Quoted string: copied verbatim (already strict-JSON escapes).
    fn quoted(&mut self) {
        self.out.push('"');
        self.pos += 1;
        while let Some(ch) = self.peek() {
            self.out.push(ch);
            self.pos += 1;
            if ch == '\\' {
                if let Some(next) = self.peek() {
                    self.out.push(next);
                    self.pos += 1;
                }
                continue;
            }
            if ch == '"' {
                break;
            }
        }
    }

    /// Arc unquoted value: up to `}`, `]`, `,`, CR or LF (`JsonReader`
    /// `unquotedChars` value branch), with `\` escape awareness; classified by
    /// the reader's keyword/number/string rules.
    fn unquoted_value(&mut self) {
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch == '\\' {
                self.pos += 1;
                if self.peek().is_some() {
                    self.pos += 1;
                }
                continue;
            }
            if ch == '/'
                && self
                    .chars
                    .get(self.pos + 1)
                    .is_some_and(|c| *c == '/' || *c == '*')
            {
                break;
            }
            if matches!(ch, '}' | ']' | ',' | '\r' | '\n') {
                break;
            }
            self.pos += 1;
        }
        let raw: String = self.chars[start..self.pos].iter().collect();
        let token = raw.trim_end();
        if matches!(token, "true" | "false" | "null") {
            self.out.push_str(token);
            return;
        }
        if token == "Infinity" || token == "-Infinity" {
            self.out.push_str(if token == "Infinity" {
                "2147483648.0"
            } else {
                "-2147483648.0"
            });
            return;
        }
        if is_strict_json_number(token) {
            self.out.push_str(token);
            return;
        }
        // Arc classifies digit/dot/exponent tokens as numbers, but keeps the
        // raw text (`JsonValue.asString`); the port preserves the token as a
        // string so field-level readers (e.g. `ColorHex`) can consume it.
        let unescaped = arc_unescape(token);
        write_json_string(&mut self.out, &unescaped);
    }
}

/// Whether `token` is a valid strict JSON number (Arc's numeric classification
/// also accepts forms like `00000000`, which strict JSON rejects).
fn is_strict_json_number(token: &str) -> bool {
    let bytes = token.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut index = 0;
    if bytes[index] == b'-' {
        index += 1;
    }
    match bytes.get(index) {
        Some(b'0') => index += 1,
        Some(ch) if ch.is_ascii_digit() => {
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
        _ => return false,
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let frac_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == frac_start {
            return false;
        }
    }
    if matches!(bytes.get(index), Some(b'e' | b'E')) {
        index += 1;
        if matches!(bytes.get(index), Some(b'+' | b'-')) {
            index += 1;
        }
        let exp_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if index == exp_start {
            return false;
        }
    }
    index == bytes.len()
}

/// Arc `JsonReader.unescape` for unquoted tokens: `\n`/`\r`/`\t`/`\b`/`\f`/
/// `\"`/`\\`/`\/` and `\uXXXX`. Unknown escapes keep both characters.
fn arc_unescape(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    let mut out = String::with_capacity(token.len());
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        index += 1;
        if ch != '\\' || index >= chars.len() {
            out.push(ch);
            continue;
        }
        let escaped = chars[index];
        index += 1;
        match escaped {
            '"' | '\\' | '/' => out.push(escaped),
            'b' => out.push('\u{0008}'),
            'f' => out.push('\u{000C}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                let hex: String = chars
                    .get(index..index + 4)
                    .map(|s| s.iter().collect())
                    .unwrap_or_default();
                if let Ok(code) = u32::from_str_radix(&hex, 16)
                    && let Some(decoded) = char::from_u32(code)
                {
                    out.push(decoded);
                    index += 4;
                } else {
                    out.push('u');
                }
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

/// Writes `value` as a strict JSON string literal.
fn write_json_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
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

    /// T2: Arc's minimal writer leaves multi-word string values unquoted with
    /// embedded quotes and `\n` escapes (the authored maps' logic-code
    /// strings); `JsonReader` lexes them as one string token. The converter
    /// must emit one escaped JSON string, not fragments.
    #[test]
    fn arc_json_unquoted_string_values_are_single_tokens() {
        let arc = "{objectives:[{class:Flag,completionLogicCode:print \"@announce.nuclearstrike\"\\nmessage announce 7 @wait\\nstop\\n,flag:nuc1}],hidden:false}";
        let strict = super::quote_bare_keys(arc);
        let parsed: Value = serde_json::from_str(&strict).expect("strict json");
        assert_eq!(
            parsed["objectives"][0]["completionLogicCode"],
            Value::String(
                "print \"@announce.nuclearstrike\"\nmessage announce 7 @wait\nstop\n".to_owned()
            )
        );
        assert_eq!(
            parsed["objectives"][0]["flag"],
            Value::String("nuc1".to_owned())
        );
    }

    /// T2: `ambientLight:00000000` is Arc's `Color.toString()` written bare;
    /// the converter keeps the token text so `ColorHex` can read it.
    #[test]
    fn arc_json_leading_zero_color_tokens_stay_strings() {
        let arc = r#"{ambientLight:00000000,cloudColor:7457ce7f}"#;
        let strict = super::quote_bare_keys(arc);
        let parsed: Value = serde_json::from_str(&strict).expect("strict json");
        assert_eq!(parsed["ambientLight"], Value::String("00000000".to_owned()));
        assert_eq!(parsed["cloudColor"], Value::String("7457ce7f".to_owned()));
    }

    /// Strict JSON round-trips unchanged through the converter.
    #[test]
    fn arc_json_strict_input_is_preserved() {
        let strict =
            r#"{"a":"b:c","n":-1.5e3,"flag":true,"nil":null,"arr":[1,2],"nested":{"k":"v"}}"#;
        let converted = super::quote_bare_keys(strict);
        assert_eq!(converted, strict);
    }
}
