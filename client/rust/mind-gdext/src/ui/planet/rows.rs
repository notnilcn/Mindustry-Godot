// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Read-model field accessors for `planet_view`/`campaign_views` dictionaries.
//! Upstream: none (port-only; read-model field accessors)

use godot::prelude::*;

// ---------------------------------------------------------------------------
// Read-model field accessors (`planet_view`/`campaign_views` dictionaries)
// ---------------------------------------------------------------------------

/// A non-null field of a read-model row.
pub(super) fn value(row: &VarDictionary, key: &str) -> Option<Variant> {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
}

pub(super) fn variant_string(value: &Variant) -> Option<String> {
    if let Ok(text) = value.try_to::<GString>() {
        return Some(text.to_string());
    }
    if let Ok(name) = value.try_to::<StringName>() {
        return Some(name.to_string());
    }
    None
}

pub(super) fn variant_f32(value: &Variant) -> Option<f32> {
    if let Ok(number) = value.try_to::<f32>() {
        return Some(number);
    }
    if let Ok(number) = value.try_to::<f64>() {
        return Some(number as f32);
    }
    value.try_to::<i64>().ok().map(|number| number as f32)
}

pub(super) fn variant_i64(value: &Variant) -> Option<i64> {
    if let Ok(number) = value.try_to::<i64>() {
        return Some(number);
    }
    variant_f32(value).map(|number| number as i64)
}

/// Optional string field of a read-model row (`null` -> "").
pub(super) fn row_str(row: &VarDictionary, key: &str) -> String {
    value(row, key)
        .and_then(|value| variant_string(&value))
        .unwrap_or_default()
}

pub(super) fn row_bool(row: &VarDictionary, key: &str) -> bool {
    value(row, key)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(false)
}

pub(super) fn row_i64(row: &VarDictionary, key: &str, default: i64) -> i64 {
    value(row, key)
        .and_then(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// Optional id field of a read-model row (`null`/missing -> `None`).
pub(super) fn row_opt_i64(row: &VarDictionary, key: &str) -> Option<i64> {
    value(row, key).and_then(|value| variant_i64(&value))
}

/// Integer-array field of a read-model row (`attack_sources`).
pub(super) fn row_i64_array(row: &VarDictionary, key: &str) -> Vec<i64> {
    row_array(row, key)
        .iter_shared()
        .filter_map(|value| variant_i64(&value))
        .collect()
}

/// `color` with its alpha replaced (`Color.write().a(alpha)`).
pub(super) fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::from_rgba(color.r, color.g, color.b, alpha.clamp(0.0, 1.0))
}

pub(super) fn row_f32(row: &VarDictionary, key: &str, default: f32) -> f32 {
    value(row, key)
        .and_then(|value| variant_f32(&value))
        .unwrap_or(default)
}

pub(super) fn row_vec3(row: &VarDictionary, key: &str, default: Vector3) -> Vector3 {
    value(row, key)
        .and_then(|value| value.try_to::<Vector3>().ok())
        .unwrap_or(default)
}

pub(super) fn row_color(row: &VarDictionary, key: &str, default: Color) -> Color {
    value(row, key)
        .and_then(|value| value.try_to::<Color>().ok())
        .unwrap_or(default)
}

pub(super) fn row_dict(row: &VarDictionary, key: &str) -> VarDictionary {
    value(row, key)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// A non-null array field of a read-model row.
///
/// The campaign read-model producers hand GDScript-shaped **typed** arrays
/// (`Array[PackedInt32Array]` for `tile_corners`, `Array[Dictionary]` for
/// `orbits`), while JSON-born rows are untyped. gdext 0.5.5 refuses to convert
/// a typed array to `Array<Variant>` (`ElementType::is_compatible_with`), which
/// silently left `PlanetView.tile_corners` empty and dead-ended every overlay
/// build (`rebuild_highlight`/`rebuild_hover_fill`/`rebuild_markers`) on the
/// bounds check — PAR-0018. Read the runtime array type through `AnyArray` and
/// rebuild an untyped array element by element; this keeps both producer shapes
/// working without changing the UI ABI.
pub(super) fn row_array(row: &VarDictionary, key: &str) -> VarArray {
    let Some(value) = value(row, key) else {
        return VarArray::new();
    };
    if let Ok(array) = value.try_to::<VarArray>() {
        return array;
    }
    let Ok(array) = value.try_to::<AnyArray>() else {
        return VarArray::new();
    };
    let mut out = VarArray::new();
    for element in array.iter_shared() {
        out.push(&element);
    }
    out
}

pub(super) fn row_packed_vector3(row: &VarDictionary, key: &str) -> PackedVector3Array {
    value(row, key)
        .and_then(|value| value.try_to::<PackedVector3Array>().ok())
        .unwrap_or_default()
}

pub(super) fn row_packed_color(row: &VarDictionary, key: &str) -> PackedColorArray {
    value(row, key)
        .and_then(|value| value.try_to::<PackedColorArray>().ok())
        .unwrap_or_default()
}

pub(super) fn row_packed_i32(row: &VarDictionary, key: &str) -> PackedInt32Array {
    value(row, key)
        .and_then(|value| value.try_to::<PackedInt32Array>().ok())
        .unwrap_or_default()
}

/// `Color.a * state.uiAlpha` for the sector overlays the dialog fades while
/// zooming (`PlanetDialog.renderSectors`: `hoverColor.mulA(uiAlpha)` at
/// `PlanetDialog.java:489`, `drawSelection(selected, uiAlpha)` at `:495`,
/// `drawBorders(..., uiAlpha)` at `:490/:496`). Clamped like a colour channel.
pub(super) fn faded_alpha(base: f32, ui_alpha: f32) -> f32 {
    (base * ui_alpha).clamp(0.0, 1.0)
}
