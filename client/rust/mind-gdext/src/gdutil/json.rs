// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! JSON → `GString` helpers for the `*_json` `#[func]` surfaces.
//!
//! Upstream: none (port-only; the JSON response shapes the GDScript port returned).

#![allow(dead_code)] // adopted in W2 (docs/PLANS); C01 removes this

use godot::prelude::*;

/// `"{}"` as a GString: the empty-object fallback used by every `*_json` #[func].
pub fn empty_object() -> GString {
    GString::from("{}")
}

/// `"[]"` as a GString.
pub fn empty_array() -> GString {
    GString::from("[]")
}

/// `GString::from(value.to_string().as_str())` (compact serde_json output).
pub fn value_gstring(value: &serde_json::Value) -> GString {
    GString::from(value.to_string().as_str())
}

/// `serde_json::to_string(value)` or `fallback` on error, as a GString.
pub fn to_gstring<T: serde::Serialize + ?Sized>(value: &T, fallback: &str) -> GString {
    GString::from(
        serde_json::to_string(value)
            .unwrap_or_else(|_| fallback.to_owned())
            .as_str(),
    )
}
