// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Lenient `Variant`/`VarDictionary` coercion for the JSON read models.
//!
//! Every helper here is a verbatim copy of one duplicated dialog/widget helper;
//! the doc comment names the original copy. The names state the fallback order,
//! because the copies are not interchangeable: an `f64` branch, a bool branch or
//! a string parse is exactly what distinguishes one copy from another. W2 adopts
//! a helper only where the replaced copy's body matches.
//!
//! Upstream: none (port-only; GDScript's implicit `str`/`int`/`float`/`bool`
//! coercions that the native UI reproduces).


use godot::builtin::{GString, StringName, VarArray, VarDictionary, Variant, VariantType};
use godot::classes::Json;

// ---------------------------------------------------------------------------
// scalar coercion
// ---------------------------------------------------------------------------

/// `int(value)`: `i64`, then `f64` truncated, then `bool` as 0/1, then a trimmed
/// `GString` parse, then 0.
///
/// Body from `ui/dialogs/batch_b.rs:271` (also `batch_f.rs:185`, `batch_h.rs:267`).
pub fn to_i64_lenient(value: &Variant) -> i64 {
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return i64::from(flag);
    }
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string().trim().parse().unwrap_or(0);
    }
    0
}

/// `int(value)` for JSON numbers: `i64`, then `f64` truncated, then 0.
///
/// Body from `ui/dialogs/misc.rs:62` (same body at `batch_c.rs:123`; the
/// `match get_type()` copies at `editor_views.rs:99`, `schematic_image.rs:476`
/// and `research_view.rs:550` convert the same int/float cases).
pub fn to_i64_int_or_float(value: &Variant) -> i64 {
    if let Ok(integer) = value.try_to::<i64>() {
        return integer;
    }
    value
        .try_to::<f64>()
        .map(|number| number as i64)
        .unwrap_or(0)
}

/// `int(value)` by `VariantType`: INT, FLOAT truncated, BOOL 0/1, STRING or
/// STRING_NAME trimmed parse, otherwise 0.
///
/// Body from `ui/kit/widgets/items_display.rs:120` (also
/// `core_items_display.rs:211`, `research_items_display.rs:312`;
/// `batch_i.rs:122` is the same type match with infallible `to` conversions).
pub fn to_i64_typed(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.try_to::<i64>().unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number as i64)
            .unwrap_or_default(),
        VariantType::BOOL => {
            if value.try_to::<bool>().unwrap_or(false) {
                1
            } else {
                0
            }
        }
        VariantType::STRING | VariantType::STRING_NAME => {
            to_text_display(value).trim().parse().unwrap_or_default()
        }
        _ => 0,
    }
}

/// `int(value)` for numbers and numeric strings, **without** a bool branch:
/// `i64`, then `f64` truncated, then a trimmed `GString` parse, then 0.
///
/// Body from `ui/dialogs/batch_g.rs:178`.
pub fn to_i64_number(value: &Variant) -> i64 {
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string().trim().parse().unwrap_or(0);
    }
    0
}

/// `int(value)` without a string parse: `i64`, then `f64` truncated, then
/// `bool` as 0/1, then 0 (`None` -> 0).
///
/// Body from `ui/dialogs/batch_a.rs:131` (identical at `batch_e.rs:118`).
pub fn to_i64_no_text(value: Option<Variant>) -> i64 {
    let Some(value) = value else {
        return 0;
    };
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return i64::from(flag);
    }
    0
}

/// `float(value)`: `f64`, then `i64` as `f64`, then a trimmed `GString` parse,
/// then 0.0.
///
/// Body from `ui/dialogs/batch_f.rs:202` (also `batch_b.rs:288`).
pub fn to_f64_lenient(value: &Variant) -> f64 {
    if let Ok(number) = value.try_to::<f64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number as f64;
    }
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string().trim().parse().unwrap_or(0.0);
    }
    0.0
}

/// `float(value)` by `VariantType`: FLOAT, INT, BOOL 0.0/1.0, STRING or
/// STRING_NAME trimmed parse, otherwise 0.0.
///
/// Body from `ui/dialogs/batch_i.rs:100`.
pub fn to_f64_typed(value: &Variant) -> f64 {
    match value.get_type() {
        VariantType::FLOAT => value.to::<f64>(),
        VariantType::INT => value.to::<i64>() as f64,
        VariantType::BOOL => {
            if value.to::<bool>() {
                1.0
            } else {
                0.0
            }
        }
        VariantType::STRING | VariantType::STRING_NAME => value
            .to::<GString>()
            .to_string()
            .trim()
            .parse()
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

/// `float(value)` without a string parse: `f64`, then `i64` as `f64`, then
/// `bool` as 0.0/1.0, then 0.0 (`None` -> 0.0).
///
/// Body from `ui/dialogs/batch_a.rs:148` (identical at `batch_e.rs:135`).
pub fn to_f64_boolish(value: Option<Variant>) -> f64 {
    let Some(value) = value else {
        return 0.0;
    };
    if let Ok(number) = value.try_to::<f64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number as f64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return if flag { 1.0 } else { 0.0 };
    }
    0.0
}

/// `bool(value)`: `bool`, then `i64` != 0, then `"true"`/`"1"`, then false.
///
/// Body from `ui/dialogs/batch_b.rs:302` (also `batch_f.rs:216`, `batch_h.rs:284`).
pub fn to_bool_lenient(value: &Variant) -> bool {
    if let Ok(flag) = value.try_to::<bool>() {
        return flag;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number != 0;
    }
    if let Ok(text) = value.try_to::<GString>() {
        let text = text.to_string();
        return text == "true" || text == "1";
    }
    false
}

/// `bool(value)` with no string branch: `bool`, then `i64` != 0, then false.
///
/// Body from `ui/dialogs/batch_g.rs:192`.
pub fn to_bool_int_only(value: &Variant) -> bool {
    if let Ok(flag) = value.try_to::<bool>() {
        return flag;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number != 0;
    }
    false
}

/// `bool(value)` by `VariantType`: BOOL, INT != 0, FLOAT != 0.0, non-empty
/// STRING or STRING_NAME, otherwise false.
///
/// Body from `ui/dialogs/batch_i.rs:89`.
pub fn to_bool_typed(value: &Variant) -> bool {
    match value.get_type() {
        VariantType::BOOL => value.to::<bool>(),
        VariantType::INT => value.to::<i64>() != 0,
        VariantType::FLOAT => value.to::<f64>() != 0.0,
        VariantType::STRING | VariantType::STRING_NAME => !value.to::<GString>().is_empty(),
        _ => false,
    }
}

/// `bool(value)` as GDScript's `try_to::<bool>`: false for every other type
/// and for `None`.
///
/// Body from `ui/dialogs/batch_a.rs:165` (identical at `batch_e.rs:152`).
pub fn to_bool_strict(value: Option<Variant>) -> bool {
    let Some(value) = value else {
        return false;
    };
    value.try_to::<bool>().unwrap_or(false)
}

// ---------------------------------------------------------------------------
// text coercion
// ---------------------------------------------------------------------------

/// `str(value)`: `GString`, then `bool`, `i64` and `f64` as their `to_string`,
/// then `""`.
///
/// Body from `ui/dialogs/batch_h.rs:250` (same fallback order with different
/// local names at `batch_f.rs:168`, `batch_b.rs:254`, `batch_g.rs:161` and
/// `ui/dialogs/misc.rs:31`).
pub fn to_text_lenient(value: &Variant) -> String {
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string();
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return flag.to_string();
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number.to_string();
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number.to_string();
    }
    String::new()
}

/// GDScript `str(value)` for a variant (`<null>` for null, like Godot):
/// `GString`, otherwise `Variant::stringify`.
///
/// Body from `ui/dialogs/batch_a.rs:123` (identical at `batch_e.rs:110`).
pub fn to_text_stringify(value: &Variant) -> String {
    value
        .try_to::<GString>()
        .map(|text| text.to_string())
        .unwrap_or_else(|_| value.stringify().to_string())
}

/// `str(value)` by `VariantType`: STRING, STRING_NAME, INT, FLOAT and BOOL;
/// any other type uses `format!("{value}")`.
///
/// Body from `ui/kit/widgets/items_display.rs:93`.
pub fn to_text_display(value: &Variant) -> String {
    match value.get_type() {
        VariantType::INT => value
            .try_to::<i64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        VariantType::BOOL => value
            .try_to::<bool>()
            .map(|flag| flag.to_string())
            .unwrap_or_default(),
        VariantType::STRING => value
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::STRING_NAME => value
            .try_to::<StringName>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        _ => format!("{value}"),
    }
}

/// `str(value)` by `VariantType`: STRING/STRING_NAME, INT, FLOAT, BOOL;
/// otherwise `""` (no stringify fallback).
///
/// Body from `ui/dialogs/batch_i.rs:78`.
pub fn to_text_typed(value: &Variant) -> String {
    match value.get_type() {
        VariantType::STRING | VariantType::STRING_NAME => value.to::<GString>().to_string(),
        VariantType::INT => value.to::<i64>().to_string(),
        VariantType::FLOAT => value.to::<f64>().to_string(),
        VariantType::BOOL => value.to::<bool>().to_string(),
        _ => String::new(),
    }
}

/// GDScript `str(value)`: NIL -> `""`, BOOL -> `"true"`/`"false"`, STRING,
/// STRING_NAME, INT, FLOAT through [`format_godot_float`]; other types use
/// `format!("{value}")`.
///
/// Body from `ui/dialogs/batch_campaign2.rs:194` (`text_of`).
pub fn to_text_godot(value: &Variant) -> String {
    match value.get_type() {
        VariantType::NIL => String::new(),
        VariantType::BOOL => {
            if value.to::<bool>() {
                String::from("true")
            } else {
                String::from("false")
            }
        }
        VariantType::STRING => value
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::STRING_NAME => value
            .try_to::<StringName>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::INT => value
            .try_to::<i64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(format_godot_float)
            .unwrap_or_default(),
        _ => format!("{value}"),
    }
}

/// GDScript `str(float)`: whole numbers keep the `.0` suffix.
///
/// Body from `ui/dialogs/batch_campaign2.rs:222` (`format_godot_float`).
pub fn format_godot_float(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

// ---------------------------------------------------------------------------
// dictionary readers
// ---------------------------------------------------------------------------

/// A dictionary value coerced with [`to_i64_lenient`], or `default`.
///
/// Body from `ui/dialogs/batch_b.rs:240` (also `batch_f.rs:238`,
/// `batch_g.rs:217`, `batch_h.rs:236`, `batch_i.rs:145`).
pub fn dict_i64_or(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .map(|value| to_i64_lenient(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced with `try_to_relaxed::<i64>`, or `default`.
///
/// Body from `ui/fragments/block_config.rs:1061` (also `fragments/hud.rs:1938`
/// with a `0` default).
pub fn dict_i64_relaxed_or(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<i64>().ok())
        .unwrap_or(default)
}

/// A dictionary value coerced with `try_to::<f64>`, or `default`.
///
/// Body from `ui/dialogs/batch_j.rs:250`.
pub fn dict_f64_try_or(dict: &VarDictionary, key: &str, default: f64) -> f64 {
    dict.get(key)
        .and_then(|value| value.try_to::<f64>().ok())
        .unwrap_or(default)
}

/// A dictionary value coerced with `try_to_relaxed::<f64>`, or `default`.
///
/// Body from `ui/fragments/hud.rs:1945` (with a `0.0` default).
pub fn dict_f64_relaxed_or(dict: &VarDictionary, key: &str, default: f64) -> f64 {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<f64>().ok())
        .unwrap_or(default)
}

/// A dictionary value coerced with [`to_bool_lenient`], or `default`.
///
/// Body from `ui/dialogs/batch_b.rs:247` (also `batch_f.rs:252`,
/// `batch_g.rs:210`, `batch_h.rs:243`).
pub fn dict_bool_or(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .map(|value| to_bool_lenient(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced with `try_to::<bool>`, or `default`.
///
/// Body from `ui/dialogs/batch_j.rs:244` (also `batch_a.rs:189`,
/// `batch_e.rs:176` through `as_bool(Option<Variant>)`).
pub fn dict_bool_try_or(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(default)
}

/// A dictionary value coerced with [`to_text_lenient`], or `""`.
///
/// Body from `ui/dialogs/batch_b.rs:233` (also `batch_f.rs:231`,
/// `batch_g.rs:203`, `batch_h.rs:222`).
pub fn dict_text_or(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| to_text_lenient(&value))
        .unwrap_or_default()
}

/// A dictionary value coerced with `try_to::<GString>`, or `""`.
///
/// Body from `ui/dialogs/batch_j.rs:237`.
pub fn dict_text_try_or(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// `JSON.parse_string(text)` as a dictionary (`{}` on any other shape); an
/// empty string short-circuits before touching the engine.
///
/// Body from `ui/dialogs/batch_c.rs:174` (`batch_campaign2.rs:159` is the same
/// body with a `gs()` GString constructor wrapper).
pub fn parse_object(text: &str) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(&GString::from(text))
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `JSON.parse_string(text)` as an object (`{}` on any other shape).
///
/// Body from `ui/dialogs/batch_b.rs:317` (identical at `batch_f.rs:269`).
pub fn parse_json_object(text: &str) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(text)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `JSON.parse_string(text)` as an array (`[]` on any other shape).
///
/// Body from `ui/dialogs/batch_campaign2.rs:169` (`parse_json_array` at
/// `batch_b.rs:307`/`batch_f.rs:259` is the same body under another name).
pub fn parse_array(text: &str) -> VarArray {
    if text.is_empty() {
        return VarArray::new();
    }
    Json::parse_string(&GString::from(text))
        .try_to::<VarArray>()
        .unwrap_or_default()
}
