// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic nested JSON merge for the relay rules path (plan 21 M3,
//! §3.6/§6.3; plan 12/13 `Rules` shape).
//!
//! `SetRules` replaces the whole `rules_json` blob; `SetRule { rule, json }`
//! edits one field *inside* it (`NetClient.setRule`). The module cannot depend
//! on `mind-core` (HLP §2 boundary), so this is a small, dependency-free JSON
//! reader/writer plus a path setter that mirrors plan 12's serde JSON shape
//! (camelCase object keys, arrays for `IndexSet`/`Vec`, numeric-string keys for
//! `TeamRules`).
//!
//! The merge is a pure function of its inputs and keeps object insertion order,
//! so a given `(rules_json, rule, json)` always yields the same bytes. Number
//! tokens are preserved verbatim rather than re-formatted, which avoids
//! float-formatting drift.
//!
//! # Path grammar
//!
//! Segments are separated by `.`; a segment is `key`, `key[i]`, `key[]`,
//! `[i]` or `[]`:
//!
//! - `key` navigates an object key. When the current value is an array, a bare
//!   `key` must be a non-negative integer and is used as an index
//!   (`spawns.0.type`).
//! - `key[i]` navigates `key`, then array index `i` (`spawns[0].type`).
//! - `key[]` / `key[-]` / `key[+]` append to the array at `key`.
//! - `[i]` / `[]` index or append into the current array.
//!
//! Missing intermediate objects are created on demand; a segment with a
//! bracket suffix creates an array for its key, otherwise it creates an object.
//! A missing index instead of a replace is only allowed at `len` (append).

use std::fmt::Write as _;

/// A parsed JSON value. Object entries keep insertion order for deterministic
/// output; numbers keep their original token.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// A number, stored exactly as written in the source.
    Number(String),
    /// A string with escapes already decoded.
    String(String),
    /// An array.
    Array(Vec<Json>),
    /// An object; keys are unique and ordered.
    Object(Vec<(String, Json)>),
}

/// Parses a complete JSON document, rejecting trailing content.
pub fn parse_json(input: &str) -> Result<Json, String> {
    let mut parser = Parser::new(input);
    let value = parser.parse_value()?;
    parser.skip_ws();
    if parser.pos != parser.s.len() {
        return Err("trailing characters after JSON value".to_string());
    }
    Ok(value)
}

/// Serializes a JSON value in compact form.
pub fn serialize(value: &Json) -> String {
    let mut out = String::new();
    write_value(value, &mut out);
    out
}

/// Merges `value_json` at `path` into `existing`, returning the new blob.
///
/// An empty/whitespace `existing` is treated as an empty object (the same
/// tolerance `merge_rule_stub` had). The root of a non-empty blob must be a
/// JSON object.
pub fn merge_rule(existing: &str, path: &str, value_json: &str) -> Result<String, String> {
    let mut root = if existing.trim().is_empty() {
        Json::Object(Vec::new())
    } else {
        parse_json(existing)?
    };
    if !matches!(root, Json::Object(_)) {
        return Err("rules_json root is not a JSON object".to_string());
    }
    let value = parse_json(value_json)?;
    set_path(&mut root, path, value)?;
    Ok(serialize(&root))
}

/// Cheap validation for a whole `SetRules` blob: it must parse as an object.
pub fn validate_rules_blob(rules_json: &str) -> Result<(), String> {
    let root = parse_json(rules_json)?;
    if !matches!(root, Json::Object(_)) {
        return Err("rules_json root is not a JSON object".to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Path handling
// ---------------------------------------------------------------------------

/// One `SetRule` path segment.
#[derive(Debug, Clone, PartialEq)]
struct Segment {
    /// Object key (may be empty for a pure `[i]` segment).
    key: Option<String>,
    /// Bracket suffix, when present.
    index: Option<Index>,
}

/// A bracket suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Index {
    /// `[i]`: index `i`.
    At(usize),
    /// `[]`, `[-]`, `[+]`: append to the end.
    Append,
}

fn parse_path(path: &str) -> Result<Vec<Segment>, String> {
    if path.is_empty() {
        return Err("rule path is empty".to_string());
    }
    path.split('.').map(parse_segment).collect()
}

fn parse_segment(segment: &str) -> Result<Segment, String> {
    match segment.find('[') {
        None => {
            if segment.is_empty() {
                return Err("rule path has an empty segment".to_string());
            }
            if segment.contains(']') {
                return Err("rule path has an unmatched `]`".to_string());
            }
            Ok(Segment {
                key: Some(segment.to_string()),
                index: None,
            })
        }
        Some(open) => {
            let key = &segment[..open];
            if !segment.ends_with(']') {
                return Err(format!("rule path segment `{segment}` is missing `]`"));
            }
            let inner = &segment[open + 1..segment.len() - 1];
            if inner.contains('[') || inner.contains(']') {
                return Err(format!("rule path segment `{segment}` has nested brackets"));
            }
            let index =
                match inner {
                    "" | "-" | "+" => Index::Append,
                    digits => Index::At(digits.parse::<usize>().map_err(|_| {
                        format!("rule index `{digits}` is not a non-negative integer")
                    })?),
                };
            Ok(Segment {
                key: if key.is_empty() {
                    None
                } else {
                    Some(key.to_string())
                },
                index: Some(index),
            })
        }
    }
}

fn set_path(root: &mut Json, path: &str, value: Json) -> Result<(), String> {
    let segments = parse_path(path)?;
    set_at(root, &segments, value)
}

fn set_at(container: &mut Json, segments: &[Segment], value: Json) -> Result<(), String> {
    let (first, rest) = segments
        .split_first()
        .ok_or_else(|| "rule path is empty".to_string())?;
    match container {
        Json::Object(entries) => {
            let key = first
                .key
                .as_deref()
                .ok_or_else(|| "array index applied to an object".to_string())?;
            if key.is_empty() {
                return Err("rule path has an empty object key".to_string());
            }
            let position = match entries.iter().position(|(name, _)| name == key) {
                Some(position) => position,
                None => {
                    let placeholder = if first.index.is_some() {
                        Json::Array(Vec::new())
                    } else {
                        Json::Object(Vec::new())
                    };
                    entries.push((key.to_string(), placeholder));
                    entries.len() - 1
                }
            };
            if let Some(index) = first.index {
                set_in_array(&mut entries[position].1, key, index, rest, value)
            } else if rest.is_empty() {
                entries[position].1 = value;
                Ok(())
            } else {
                set_at(&mut entries[position].1, rest, value)
            }
        }
        Json::Array(_) => {
            let index = match (first.key.as_deref(), first.index) {
                (Some(token), None) => Index::At(parse_index_token(token)?),
                (None, Some(index)) => index,
                _ => return Err("invalid array rule path".to_string()),
            };
            set_in_array(container, "[]", index, rest, value)
        }
        other => Err(format!(
            "rule path crosses a non-container value ({})",
            describe(other)
        )),
    }
}

fn set_in_array(
    container: &mut Json,
    label: &str,
    index: Index,
    rest: &[Segment],
    value: Json,
) -> Result<(), String> {
    let Json::Array(items) = container else {
        return Err(format!("`{label}` is not an array"));
    };
    match index {
        Index::At(position) => {
            if position < items.len() {
                if rest.is_empty() {
                    items[position] = value;
                    Ok(())
                } else {
                    set_at(&mut items[position], rest, value)
                }
            } else if position == items.len() {
                if rest.is_empty() {
                    items.push(value);
                    Ok(())
                } else {
                    items.push(array_element_container(&rest[0]));
                    let position = items.len() - 1;
                    set_at(&mut items[position], rest, value)
                }
            } else {
                Err(format!(
                    "rule array index {position} is out of range (len {})",
                    items.len()
                ))
            }
        }
        Index::Append => {
            if rest.is_empty() {
                items.push(value);
                return Ok(());
            }
            items.push(array_element_container(&rest[0]));
            let position = items.len() - 1;
            set_at(&mut items[position], rest, value)
        }
    }
}

/// The container to create for a new array element addressed by `segment`
/// (`key...` addresses an object, a bare `[i]` addresses an array).
fn array_element_container(segment: &Segment) -> Json {
    if segment.key.is_some() {
        Json::Object(Vec::new())
    } else {
        Json::Array(Vec::new())
    }
}

fn parse_index_token(token: &str) -> Result<usize, String> {
    token
        .parse::<usize>()
        .map_err(|_| format!("array key `{token}` is not a non-negative integer"))
}

fn describe(value: &Json) -> &'static str {
    match value {
        Json::Null => "null",
        Json::Bool(_) => "a boolean",
        Json::Number(_) => "a number",
        Json::String(_) => "a string",
        Json::Array(_) => "an array",
        Json::Object(_) => "an object",
    }
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            s: input.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek();
        if byte.is_some() {
            self.pos += 1;
        }
        byte
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.bump() == Some(byte) {
            Ok(())
        } else {
            Err(format!("expected `{}` in JSON", char::from(byte)))
        }
    }

    fn parse_value(&mut self) -> Result<Json, String> {
        self.skip_ws();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => Ok(Json::String(self.parse_string()?)),
            Some(b't') => {
                self.expect_literal("true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.expect_literal("false")?;
                Ok(Json::Bool(false))
            }
            Some(b'n') => {
                self.expect_literal("null")?;
                Ok(Json::Null)
            }
            Some(_) => self.parse_number(),
            None => Err("unexpected end of JSON input".to_string()),
        }
    }

    fn expect_literal(&mut self, literal: &str) -> Result<(), String> {
        if self.s[self.pos..].starts_with(literal.as_bytes()) {
            self.pos += literal.len();
            Ok(())
        } else {
            Err(format!("expected `{literal}` in JSON"))
        }
    }

    fn parse_object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut entries: Vec<(String, Json)> = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Object(entries));
        }
        loop {
            self.skip_ws();
            let key = self.parse_string()?;
            self.skip_ws();
            self.expect(b':')?;
            let value = self.parse_value()?;
            match entries.iter_mut().find(|(name, _)| name == &key) {
                Some(existing) => existing.1 = value,
                None => entries.push((key, value)),
            }
            self.skip_ws();
            match self.bump() {
                Some(b',') => {}
                Some(b'}') => break,
                _ => return Err("expected `,` or `}` in JSON object".to_string()),
            }
        }
        Ok(Json::Object(entries))
    }

    fn parse_array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Array(items));
        }
        loop {
            items.push(self.parse_value()?);
            self.skip_ws();
            match self.bump() {
                Some(b',') => {}
                Some(b']') => break,
                _ => return Err("expected `,` or `]` in JSON array".to_string()),
            }
        }
        Ok(Json::Array(items))
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let byte = self
                .bump()
                .ok_or_else(|| "unterminated JSON string".to_string())?;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escape = self
                        .bump()
                        .ok_or_else(|| "unterminated JSON escape".to_string())?;
                    match escape {
                        b'"' => out.push(b'"'),
                        b'\\' => out.push(b'\\'),
                        b'/' => out.push(b'/'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0c),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let code = self.parse_hex4()?;
                            let scalar = if (0xD800..=0xDBFF).contains(&code) {
                                if self.bump() != Some(b'\\') || self.bump() != Some(b'u') {
                                    return Err(
                                        "unpaired high surrogate in JSON string".to_string()
                                    );
                                }
                                let low = self.parse_hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err("invalid low surrogate in JSON string".to_string());
                                }
                                0x1_0000 + ((code - 0xD800) << 10) + (low - 0xDC00)
                            } else if (0xDC00..=0xDFFF).contains(&code) {
                                return Err("unpaired low surrogate in JSON string".to_string());
                            } else {
                                code
                            };
                            let ch = char::from_u32(scalar).ok_or_else(|| {
                                "invalid Unicode scalar in JSON string".to_string()
                            })?;
                            let mut buffer = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
                        }
                        _ => return Err("invalid escape in JSON string".to_string()),
                    }
                }
                byte if byte < 0x20 => {
                    return Err("unescaped control character in JSON string".to_string());
                }
                other => out.push(other),
            }
        }
        String::from_utf8(out).map_err(|_| "invalid UTF-8 in JSON string".to_string())
    }

    fn parse_hex4(&mut self) -> Result<u32, String> {
        let mut value = 0u32;
        for _ in 0..4 {
            let byte = self
                .bump()
                .ok_or_else(|| "truncated `\\u` escape in JSON string".to_string())?;
            let digit = char::from(byte)
                .to_digit(16)
                .ok_or_else(|| "invalid hex digit in JSON string".to_string())?;
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
                if matches!(self.peek(), Some(b'0'..=b'9')) {
                    return Err("leading zero in JSON number".to_string());
                }
            }
            Some(b'1'..=b'9') => {
                self.consume_digits();
            }
            _ => return Err("invalid number in JSON".to_string()),
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            if self.consume_digits() == 0 {
                return Err("invalid fraction in JSON number".to_string());
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.consume_digits() == 0 {
                return Err("invalid exponent in JSON number".to_string());
            }
        }
        let token = std::str::from_utf8(&self.s[start..self.pos])
            .map_err(|_| "invalid UTF-8 in JSON number".to_string())?;
        Ok(Json::Number(token.to_string()))
    }

    fn consume_digits(&mut self) -> usize {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos - start
    }
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

fn write_value(value: &Json, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Number(token) => out.push_str(token),
        Json::String(text) => write_string(text, out),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Json::Object(entries) => {
            out.push('{');
            for (index, (key, item)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                write_value(item, out);
            }
            out.push('}');
        }
    }
}

fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            ch if (ch as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn round_trips_structured_json() {
        let source =
            r#"{"a":1,"b":[true,false,null,-2.5e3],"c":{"d":"x\ny","e":"\u00e9\uD83D\uDE00"}}"#;
        let value = parse_json(source).unwrap();
        // Escapes decode to the same value; re-serialization is byte-stable and
        // emits non-ASCII as UTF-8 (still valid JSON).
        assert_eq!(
            serialize(&value),
            "{\"a\":1,\"b\":[true,false,null,-2.5e3],\"c\":{\"d\":\"x\\ny\",\"e\":\"é😀\"}}"
        );
        assert_eq!(parse_json(&serialize(&value)).unwrap(), value);
    }

    #[test]
    fn serializes_control_characters() {
        let value = Json::String("a\u{1}b\tc".to_string());
        assert_eq!(serialize(&value), "\"a\\u0001b\\tc\"");
    }

    #[test]
    fn rejects_malformed_json() {
        for bad in [
            "",
            "{",
            "{\"a\":}",
            "[1,]",
            "01",
            "1.",
            "\"unterminated",
            "tru",
            "{} trailing",
        ] {
            assert!(parse_json(bad).is_err(), "accepted `{bad}`");
        }
    }

    #[test]
    fn merge_materializes_top_level_field() {
        assert_eq!(
            merge_rule("{}", "waves", "true").unwrap(),
            "{\"waves\":true}"
        );
        assert_eq!(merge_rule("", "pvp", "false").unwrap(), "{\"pvp\":false}");
    }

    #[test]
    fn merge_preserves_existing_fields() {
        assert_eq!(
            merge_rule("{\"waves\":true}", "pvp", "false").unwrap(),
            "{\"waves\":true,\"pvp\":false}"
        );
        // Overwriting keeps the original key position.
        assert_eq!(
            merge_rule("{\"waves\":true,\"pvp\":false}", "waves", "false").unwrap(),
            "{\"waves\":false,\"pvp\":false}"
        );
    }

    #[test]
    fn merge_nested_object_path() {
        let existing = "{\"teams\":{\"1\":{\"unitDamageMultiplier\":1.0}}}";
        let merged = merge_rule(existing, "teams.2.unitDamageMultiplier", "2.5").unwrap();
        assert_eq!(
            merged,
            "{\"teams\":{\"1\":{\"unitDamageMultiplier\":1.0},\"2\":{\"unitDamageMultiplier\":2.5}}}"
        );
    }

    #[test]
    fn merge_array_index_and_append() {
        let existing = "{\"spawns\":[{\"type\":\"dagger\"},{\"type\":\"crawler\"}]}";
        let indexed = merge_rule(existing, "spawns.1.type", "\"flare\"").unwrap();
        assert_eq!(
            indexed,
            "{\"spawns\":[{\"type\":\"dagger\"},{\"type\":\"flare\"}]}"
        );
        let appended = merge_rule(existing, "spawns[]", "{\"type\":\"mace\"}").unwrap();
        assert_eq!(
            appended,
            "{\"spawns\":[{\"type\":\"dagger\"},{\"type\":\"crawler\"},{\"type\":\"mace\"}]}"
        );
        // `[i]` form and the `-` append synonym.
        assert_eq!(
            merge_rule(existing, "spawns[1].type", "\"nova\"").unwrap(),
            "{\"spawns\":[{\"type\":\"dagger\"},{\"type\":\"nova\"}]}"
        );
        assert_eq!(
            merge_rule(
                "{\"bannedBlocks\":[\"router\"]}",
                "bannedBlocks[-]",
                "\"conveyor\""
            )
            .unwrap(),
            "{\"bannedBlocks\":[\"router\",\"conveyor\"]}"
        );
    }

    #[test]
    fn merge_creates_missing_containers() {
        // Bracket suffix means the key holds an array.
        assert_eq!(
            merge_rule("{}", "spawns[0].type", "\"dagger\"").unwrap(),
            "{\"spawns\":[{\"type\":\"dagger\"}]}"
        );
        // No suffix means the key holds an object (TeamRules numeric-string key).
        assert_eq!(
            merge_rule("{}", "teams.2.protectCores", "false").unwrap(),
            "{\"teams\":{\"2\":{\"protectCores\":false}}}"
        );
        // Append when absent.
        assert_eq!(
            merge_rule("{}", "bannedUnits[]", "\"dagger\"").unwrap(),
            "{\"bannedUnits\":[\"dagger\"]}"
        );
    }

    #[test]
    fn merge_rejects_bad_paths_and_types() {
        assert!(merge_rule("{}", "", "true").is_err());
        assert!(merge_rule("{}", "a..b", "true").is_err());
        assert!(merge_rule("{}", "a[", "true").is_err());
        assert!(merge_rule("{}", "a[x]", "true").is_err());
        assert!(merge_rule("{}", "a[5]", "true").is_err());
        assert!(merge_rule("{\"a\":1}", "a.b", "true").is_err());
        assert!(merge_rule("{\"a\":{}}", "a[0]", "true").is_err());
    }

    #[test]
    fn merge_rejects_invalid_inputs() {
        assert!(merge_rule("{", "waves", "true").is_err());
        assert!(merge_rule("{}", "waves", "not json").is_err());
        assert!(merge_rule("[1,2]", "waves", "true").is_err());
    }

    #[test]
    fn validate_rules_blob_requires_object() {
        assert!(validate_rules_blob("{}").is_ok());
        assert!(validate_rules_blob("{\"waves\":true}").is_ok());
        assert!(validate_rules_blob("[]").is_err());
        assert!(validate_rules_blob("oops").is_err());
    }
}
