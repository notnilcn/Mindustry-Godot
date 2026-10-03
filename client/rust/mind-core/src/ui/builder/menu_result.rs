// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/MenuResult.java.

//! Server-menu result capture + caps (plan 14 §3.6/§6.3).
//!
//! A `MenuResult` records the clicked result id plus the values of every
//! id-bearing element (slider/field/check/checked-button) at click time. The
//! caps mirror upstream (`maxResultLen` …) and are enforced on construction so a
//! malicious or buggy server cannot blow up the relay payload.

use indexmap::IndexMap;

/// Header byte for the [`MenuResult`] wire encoding (plan 14 §6.3).
pub const MENU_RESULT_FORMAT: u8 = 1;

/// Maximum length of the result string.
pub const MAX_RESULT_LEN: usize = 500;
/// Maximum length of a single value in chars.
pub const MAX_VALUE_LEN: usize = 1000;
/// Maximum total length of all string values in chars.
pub const MAX_TOTAL_STRING_LEN: usize = 6000;
/// Maximum number of values of any type.
pub const MAX_TOTAL_VALUES: usize = 500;

/// A typed result value.
#[derive(Debug, Clone, PartialEq)]
pub enum MenuValue {
    /// String value (text field).
    Str(String),
    /// Float value (slider).
    F32(f32),
    /// Boolean value (check box / checked button).
    Bool(bool),
}

/// Captured menu result (`MenuResult`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MenuResult {
    /// Token passed to `MenuBuilder`.
    pub token: i64,
    /// Clicked button id (`None` when the dialog was cancelled).
    pub result: Option<String>,
    /// Id → element value at click time.
    pub values: IndexMap<String, MenuValue>,
}

impl MenuResult {
    /// Empty (cancelled) result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Result from a clicked button id, clipped to [`MAX_RESULT_LEN`].
    pub fn from_result(result: impl Into<String>) -> Self {
        let mut result = result.into();
        if result.chars().count() > MAX_RESULT_LEN {
            result = result.chars().take(MAX_RESULT_LEN).collect();
        }
        Self {
            token: 0,
            result: Some(result),
            values: IndexMap::new(),
        }
    }

    /// Whether the dialog was closed with no choice (`wasCancelled`).
    pub fn was_cancelled(&self) -> bool {
        self.result.is_none()
    }

    /// Whether the result id equals `text` (`is`).
    pub fn is(&self, text: &str) -> bool {
        self.result.as_deref() == Some(text)
    }

    /// Inserts a value honoring the upstream caps (over-long values clipped,
    /// excess values dropped).
    pub fn insert(&mut self, id: impl Into<String>, value: MenuValue) {
        if self.values.len() >= MAX_TOTAL_VALUES {
            return;
        }
        let id = id.into();
        let value = match value {
            MenuValue::Str(text) => {
                let text = clip(text, MAX_VALUE_LEN);
                let total: usize = self
                    .values
                    .values()
                    .filter_map(|v| match v {
                        MenuValue::Str(s) => Some(s.chars().count()),
                        _ => None,
                    })
                    .sum();
                if total + text.chars().count() > MAX_TOTAL_STRING_LEN {
                    return;
                }
                MenuValue::Str(text)
            }
            other => other,
        };
        self.values.insert(id, value);
    }

    /// `getString(id, default)`.
    pub fn get_str_or<'a>(&'a self, id: &str, default: &'a str) -> &'a str {
        match self.values.get(id) {
            Some(MenuValue::Str(value)) => value.as_str(),
            _ => default,
        }
    }

    /// `getString(id)`.
    pub fn get_str(&self, id: &str) -> Option<&str> {
        match self.values.get(id) {
            Some(MenuValue::Str(value)) => Some(value.as_str()),
            _ => None,
        }
    }

    /// `getFloat(id, default)`.
    pub fn get_f32_or(&self, id: &str, default: f32) -> f32 {
        match self.values.get(id) {
            Some(MenuValue::F32(value)) => *value,
            _ => default,
        }
    }

    /// `getFloat(id)`.
    pub fn get_f32(&self, id: &str) -> f32 {
        self.get_f32_or(id, 0.0)
    }

    /// `getBool(id, default)`.
    pub fn get_bool_or(&self, id: &str, default: bool) -> bool {
        match self.values.get(id) {
            Some(MenuValue::Bool(value)) => *value,
            _ => default,
        }
    }

    /// `getBool(id)`.
    pub fn get_bool(&self, id: &str) -> bool {
        self.get_bool_or(id, false)
    }

    /// Encodes the result for the plan-21 `MenuBuilderChoose.result` bytes
    /// (`format: 1` + token + optional result id + capped value map).
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![MENU_RESULT_FORMAT];
        out.extend_from_slice(&self.token.to_le_bytes());
        match &self.result {
            Some(result) => {
                out.push(1);
                write_str(&mut out, result);
            }
            None => out.push(0),
        }
        out.extend_from_slice(&(self.values.len() as u16).to_le_bytes());
        for (id, value) in &self.values {
            write_str(&mut out, id);
            match value {
                MenuValue::Str(text) => {
                    out.push(0);
                    write_str(&mut out, text);
                }
                MenuValue::F32(value) => {
                    out.push(1);
                    out.extend_from_slice(&value.to_le_bytes());
                }
                MenuValue::Bool(value) => {
                    out.push(2);
                    out.push(u8::from(*value));
                }
            }
        }
        out
    }

    /// Decodes a [`Self::encode`] payload, re-applying the upstream caps.
    pub fn decode(data: &[u8]) -> Result<MenuResult, MenuResultError> {
        let mut reader = Reader { data, pos: 0 };
        let format = reader.u8()?;
        if format != MENU_RESULT_FORMAT {
            return Err(MenuResultError::BadFormat(format));
        }
        let token = reader.i64()?;
        let result = if reader.u8()? != 0 {
            Some(reader.str()?)
        } else {
            None
        };
        let count = reader.u16()?;
        let mut decoded = MenuResult {
            token,
            result: match result {
                Some(result) => {
                    let mut clipped = result;
                    if clipped.chars().count() > MAX_RESULT_LEN {
                        clipped = clipped.chars().take(MAX_RESULT_LEN).collect();
                    }
                    Some(clipped)
                }
                None => None,
            },
            values: IndexMap::new(),
        };
        for _ in 0..count {
            let id = reader.str()?;
            let value = match reader.u8()? {
                0 => MenuValue::Str(reader.str()?),
                1 => MenuValue::F32(reader.f32()?),
                2 => MenuValue::Bool(reader.u8()? != 0),
                other => return Err(MenuResultError::BadTag(other)),
            };
            decoded.insert(id, value);
        }
        if reader.pos != data.len() {
            return Err(MenuResultError::TrailingBytes(data.len() - reader.pos));
        }
        Ok(decoded)
    }
}

/// [`MenuResult`] wire decode errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuResultError {
    /// Unknown format header byte.
    BadFormat(u8),
    /// Unknown value tag byte.
    BadTag(u8),
    /// Unexpected end of input.
    Eof,
    /// Invalid UTF-8 in a string payload.
    Utf8,
    /// Bytes left after decoding.
    TrailingBytes(usize),
}

impl std::fmt::Display for MenuResultError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MenuResultError::BadFormat(value) => write!(f, "unknown menu result format {value}"),
            MenuResultError::BadTag(value) => write!(f, "unknown menu value tag {value}"),
            MenuResultError::Eof => write!(f, "unexpected end of menu result input"),
            MenuResultError::Utf8 => write!(f, "invalid utf-8 in menu result string"),
            MenuResultError::TrailingBytes(count) => write!(f, "{count} trailing bytes"),
        }
    }
}

impl std::error::Error for MenuResultError {}

fn write_str(out: &mut Vec<u8>, value: &str) {
    let bytes = value.as_bytes();
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, len: usize) -> Result<&[u8], MenuResultError> {
        if self.pos + len > self.data.len() {
            return Err(MenuResultError::Eof);
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, MenuResultError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, MenuResultError> {
        let bytes = self.take(2)?;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn i64(&mut self) -> Result<i64, MenuResultError> {
        let bytes = self.take(8)?;
        Ok(i64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    fn f32(&mut self) -> Result<f32, MenuResultError> {
        let bytes = self.take(4)?;
        Ok(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn str(&mut self) -> Result<String, MenuResultError> {
        let len = self.u16()? as usize;
        let bytes = self.take(len)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| MenuResultError::Utf8)
    }
}

/// Truncates a string to `max` chars.
fn clip(text: String, max: usize) -> String {
    if text.chars().count() <= max {
        return text;
    }
    text.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_result_caps() {
        let result = MenuResult::from_result("ok");
        assert!(!result.was_cancelled());
        assert!(result.is("ok"));
        assert!(!result.is("no"));

        let long = "x".repeat(MAX_RESULT_LEN + 50);
        let result = MenuResult::from_result(long);
        assert_eq!(
            result.result.as_deref().unwrap().chars().count(),
            MAX_RESULT_LEN
        );

        let mut result = MenuResult::new();
        result.insert("amount", MenuValue::F32(12.5));
        result.insert("name", MenuValue::Str("s".repeat(MAX_VALUE_LEN + 10)));
        result.insert("enabled", MenuValue::Bool(true));
        assert_eq!(result.get_f32_or("amount", 0.0), 12.5);
        assert_eq!(
            result.get_str("name").unwrap().chars().count(),
            MAX_VALUE_LEN
        );
        assert!(result.get_bool("enabled"));
        assert_eq!(result.get_str_or("missing", "d"), "d");

        // Value count cap.
        for index in 0..(MAX_TOTAL_VALUES + 20) {
            result.insert(format!("k{index}"), MenuValue::Bool(true));
        }
        assert_eq!(result.values.len(), MAX_TOTAL_VALUES);
    }

    #[test]
    fn menu_result_cancelled() {
        let mut result = MenuResult::new();
        assert!(result.was_cancelled());
        result.token = 7;
        assert_eq!(result.token, 7);
    }

    #[test]
    fn menu_result_wire_roundtrip() {
        let mut result = MenuResult::from_result("buy");
        result.token = 42;
        result.insert("amount", MenuValue::F32(3.5));
        result.insert("name", MenuValue::Str("copper".to_owned()));
        result.insert("enabled", MenuValue::Bool(true));
        let bytes = result.encode();
        let decoded = MenuResult::decode(&bytes).unwrap();
        assert_eq!(decoded.token, 42);
        assert!(decoded.is("buy"));
        assert_eq!(decoded.get_f32("amount"), 3.5);
        assert_eq!(decoded.get_str("name"), Some("copper"));
        assert!(decoded.get_bool("enabled"));

        // Cancelled result round-trips with `result == None`.
        let mut cancelled = MenuResult::new();
        cancelled.token = 9;
        let decoded = MenuResult::decode(&cancelled.encode()).unwrap();
        assert!(decoded.was_cancelled());
        assert_eq!(decoded.token, 9);

        assert_eq!(MenuResult::decode(&[2]), Err(MenuResultError::BadFormat(2)));
        assert_eq!(MenuResult::decode(&[]), Err(MenuResultError::Eof));
    }
}
