// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `model` support code.
//! Upstream: logic/LStatement.java

use super::*;

// ---------------------------------------------------------------------------
// statement model (`MindLogic.parse_statements` rows)
// ---------------------------------------------------------------------------

/// One editor row: `{name, category, fields: [String]}`.
#[derive(Clone, Debug, Default)]
pub(super) struct StatementRow {
    pub(super) name: String,
    pub(super) category: String,
    pub(super) fields: Vec<String>,
}

impl StatementRow {
    /// The `jump` row's `dest_index` (`-1` when unset/out of range).
    pub(super) fn jump_dest(&self, count: usize) -> i64 {
        if self.name != "jump" {
            return -1;
        }
        let Some(first) = self.fields.first() else {
            return -1;
        };
        let Ok(dest) = first.parse::<i64>() else {
            return -1;
        };
        if dest >= 0 && (dest as usize) < count {
            dest
        } else {
            -1
        }
    }
}

/// `{name, category, fields}` from a `MindLogic` row dictionary.
pub(super) fn row_from_dict(dict: &VarDictionary) -> StatementRow {
    let name = dict
        .get("name")
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default();
    let category = dict
        .get("category")
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let fields = dict
        .get("fields")
        .and_then(|value| value.try_to::<VarArray>().ok())
        .map(|array| {
            array
                .iter_shared()
                .map(|value| value.stringify().to_string())
                .collect()
        })
        .unwrap_or_default();
    StatementRow {
        name,
        category,
        fields,
    }
}

/// `Array<Dictionary>` -> rows (`[]` for anything else).
pub(super) fn rows_from_variant(value: Variant) -> Vec<StatementRow> {
    let Ok(array) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    array
        .iter_shared()
        .filter_map(|entry| entry.try_to::<VarDictionary>().ok())
        .map(|dict| row_from_dict(&dict))
        .collect()
}

/// Rows -> the `Array<Dictionary>` `MindLogic.write_statements` consumes.
pub(super) fn rows_to_variant(rows: &[StatementRow]) -> VarArray {
    let mut array = VarArray::new();
    for row in rows {
        let mut dict = VarDictionary::new();
        dict.set("name", &gs(row.name.as_str()).to_variant());
        let mut fields = VarArray::new();
        for field in &row.fields {
            fields.push(&gs(field.as_str()).to_variant());
        }
        dict.set("fields", &fields.to_variant());
        array.push(&dict.to_variant());
    }
    array
}

/// `PackedStringArray(values)` for the `MindLogic` field calls.
pub(super) fn packed_strings(values: &[String]) -> PackedStringArray {
    let mut out = PackedStringArray::new();
    for value in values {
        out.push(&gs(value.as_str()));
    }
    out
}

/// A `PackedStringArray` return as `Vec<String>`.
pub(super) fn packed_to_vec(value: &Variant) -> Option<Vec<String>> {
    value.try_to::<PackedStringArray>().ok().map(|packed| {
        packed
            .as_slice()
            .iter()
            .map(|item| item.to_string())
            .collect()
    })
}
