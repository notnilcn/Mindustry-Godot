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
#[derive(Debug, Clone, Default)]
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
}
