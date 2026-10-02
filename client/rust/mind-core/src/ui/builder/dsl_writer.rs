// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/UiDslWriter.java.

//! MSUI DSL writer (plan 14 §3.6).
//!
//! Inverse of [`super::dsl`]: emits canonical source text (two-space indent,
//! shorthand detection, quoting rules) matching `UiDslWriter`.

use crate::ui::builder::ui_key::UiKey;
use crate::ui::builder::ui_node::{UiNode, UiValue};

/// Writes a root node's entries (the implicit root table's body).
pub fn write(root: &UiNode) -> String {
    let mut out = String::new();
    write_entries(&mut out, root, 0);
    out
}

fn write_entries(out: &mut String, node: &UiNode, depth: usize) {
    for entry in &node.entries {
        if entry.key == UiKey::Row && entry.value == UiValue::Bool(true) {
            indent(out, depth);
            out.push_str("row\n");
            continue;
        }
        if let UiValue::Node(child) = &entry.value {
            write_child_node(out, child, depth);
            continue;
        }
        indent(out, depth);
        out.push_str(entry.key.name());
        out.push_str(": ");
        out.push_str(&format_value(&entry.value));
        out.push('\n');
    }
}

fn write_child_node(out: &mut String, child: &UiNode, depth: usize) {
    indent(out, depth);
    out.push_str(child.node_type.name());

    let shorthand_key = shorthand_type(child.node_type);
    let mut shorthand: Option<usize> = None;
    for (index, entry) in child.entries.iter().enumerate() {
        if entry.key == shorthand_key && matches!(entry.value, UiValue::Str(_)) {
            shorthand = Some(index);
            break;
        }
    }

    if let Some(index) = shorthand {
        out.push_str(": ");
        out.push_str(&format_value(&child.entries[index].value));
    }

    let has_rest = child
        .entries
        .iter()
        .enumerate()
        .any(|(index, _)| Some(index) != shorthand);
    if !has_rest {
        out.push('\n');
    } else {
        if shorthand.is_some() {
            out.push(' ');
        }
        out.push_str("{\n");
        for (index, entry) in child.entries.iter().enumerate() {
            if Some(index) == shorthand {
                continue;
            }
            if entry.key == UiKey::Row && entry.value == UiValue::Bool(true) {
                indent(out, depth + 1);
                out.push_str("row\n");
                continue;
            }
            if let UiValue::Node(nested) = &entry.value {
                write_child_node(out, nested, depth + 1);
                continue;
            }
            indent(out, depth + 1);
            out.push_str(entry.key.name());
            out.push_str(": ");
            out.push_str(&format_value(&entry.value));
            out.push('\n');
        }
        indent(out, depth);
        out.push_str("}\n");
    }
}

/// `image` uses `region`, everything else uses `text`.
fn shorthand_type(key: UiKey) -> UiKey {
    match key {
        UiKey::Image => UiKey::Region,
        _ => UiKey::Text,
    }
}

fn format_value(value: &UiValue) -> String {
    match value {
        UiValue::Bool(value) => {
            if *value {
                "true".to_owned()
            } else {
                "false".to_owned()
            }
        }
        UiValue::F32(value) => format_float(*value),
        UiValue::Str(value) => {
            if needs_quotes(value) {
                quote(value)
            } else {
                value.clone()
            }
        }
        UiValue::Node(_) => String::new(),
    }
}

fn format_float(value: f32) -> String {
    if value == (value as i64) as f32 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

/// A bare token round-trips only if the parser would not reinterpret it.
fn needs_quotes(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    if value == "true" || value == "false" {
        return true;
    }
    if value.parse::<f32>().is_ok() {
        return true;
    }
    for c in value.chars() {
        if c.is_whitespace() || c == '{' || c == '}' || c == ':' || c == '"' || c == '\\' {
            return true;
        }
    }
    value.starts_with("//")
}

fn quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_normalizes_shorthand() {
        let tree = crate::ui::builder::dsl::parse(
            "table {\n  background: grayPanel\n  label: \"a b\"\n  image: \"icon\"\n}\n",
        )
        .unwrap();
        let written = write(&tree);
        assert!(written.contains("label: \"a b\""));
        // `icon` is a valid bare token, so the writer leaves it unquoted.
        assert!(written.contains("image: icon"));
        assert!(written.contains("background: grayPanel"));
    }

    #[test]
    fn writer_quotes_ambiguous_values() {
        let mut root = UiNode::new(UiKey::Table);
        root.push(
            UiKey::Label,
            UiValue::Node(Box::new(UiNode::new(UiKey::Label).str(UiKey::Text, "true"))),
        );
        let written = write(&root);
        assert_eq!(written, "label: \"true\"\n");
    }
}
