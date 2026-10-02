// SPDX-License-Identifier: GPL-3.0-only

//! Minimal TOML subset reader for plan-23 registry files.
//!
//! The workspace deliberately carries no full TOML dependency; the two TOML
//! registries this plan owns (`parity/matrix.toml`, `parity/soak.toml`) use a
//! tiny, fixed schema. This parser handles:
//!   * comments (`# ...`) outside strings,
//!   * root scalars: `key = "string" | true | false | 123`,
//!   * table headers `[a.b]` and array-of-tables headers `[[row]]`.
//!
//! It intentionally rejects anything else so a malformed registry fails loudly.

use std::collections::BTreeMap;

use anyhow::{Result, anyhow};

/// A parsed TOML scalar.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A quoted string (escapes decoded for double quotes).
    Str(String),
    /// A signed integer.
    Int(i64),
    /// A boolean.
    Bool(bool),
}

impl Value {
    /// Borrows the value as a string, if it is one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(value) => Some(value),
            _ => None,
        }
    }

    /// Borrows the value as a bool, if it is one.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Borrows the value as an integer, if it is one.
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(value) => Some(*value),
            _ => None,
        }
    }
}

/// One `[table]`/`[[array-of-table]]` entry.
#[derive(Debug, Clone, Default)]
pub struct Table {
    /// Header name (e.g. `row`, `profiles.mid`).
    pub name: String,
    /// Key/value pairs in source order.
    pub values: BTreeMap<String, Value>,
}

/// A parsed document: root scalars plus every table in source order.
#[derive(Debug, Clone, Default)]
pub struct Document {
    /// Root-level `key = value` scalars.
    pub root: BTreeMap<String, Value>,
    /// Every table/array-of-table in source order.
    pub tables: Vec<Table>,
}

impl Document {
    /// All tables with the given name.
    pub fn tables_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Table> {
        self.tables.iter().filter(move |table| table.name == name)
    }
}

/// Parses the supported TOML subset, or fails with a line number.
pub fn parse(text: &str) -> Result<Document> {
    let mut document = Document::default();
    let mut current: Option<Table> = None;

    for (index, raw) in text.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("[[") {
            let name = rest
                .strip_suffix("]]")
                .ok_or_else(|| anyhow!("line {line_number}: malformed array-of-tables header"))?
                .trim()
                .to_owned();
            if name.is_empty() {
                return Err(anyhow!("line {line_number}: empty table name"));
            }
            if let Some(table) = current.take() {
                document.tables.push(table);
            }
            current = Some(Table {
                name,
                ..Table::default()
            });
            continue;
        }

        if let Some(rest) = line.strip_prefix('[') {
            let name = rest
                .strip_suffix(']')
                .ok_or_else(|| anyhow!("line {line_number}: malformed table header"))?
                .trim()
                .to_owned();
            if name.is_empty() {
                return Err(anyhow!("line {line_number}: empty table name"));
            }
            if let Some(table) = current.take() {
                document.tables.push(table);
            }
            current = Some(Table {
                name,
                ..Table::default()
            });
            continue;
        }

        let (key, value_text) = line
            .split_once('=')
            .ok_or_else(|| anyhow!("line {line_number}: expected `key = value`"))?;
        let key = key.trim().to_owned();
        if key.is_empty() {
            return Err(anyhow!("line {line_number}: empty key"));
        }
        let value = parse_value(value_text.trim(), line_number)?;
        match current.as_mut() {
            Some(table) => {
                table.values.insert(key, value);
            }
            None => {
                document.root.insert(key, value);
            }
        }
    }

    if let Some(table) = current {
        document.tables.push(table);
    }
    Ok(document)
}

/// Strips a `#` comment that is not inside a quoted string.
fn strip_comment(line: &str) -> &str {
    let mut in_string: Option<char> = None;
    for (offset, ch) in line.char_indices() {
        match in_string {
            Some(quote) => {
                if ch == quote {
                    in_string = None;
                }
            }
            None => match ch {
                '"' | '\'' => in_string = Some(ch),
                '#' => return &line[..offset],
                _ => {}
            },
        }
    }
    line
}

/// Parses one scalar value.
fn parse_value(text: &str, line: usize) -> Result<Value> {
    if text.is_empty() {
        return Err(anyhow!("line {line}: empty value"));
    }
    if let Some(value) = text.strip_prefix('"') {
        let inner = value
            .strip_suffix('"')
            .ok_or_else(|| anyhow!("line {line}: unterminated double-quoted string"))?;
        return Ok(Value::Str(unescape(inner)));
    }
    if let Some(value) = text.strip_prefix('\'') {
        let inner = value
            .strip_suffix('\'')
            .ok_or_else(|| anyhow!("line {line}: unterminated single-quoted string"))?;
        return Ok(Value::Str(inner.to_owned()));
    }
    match text {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    let number = text.replace('_', "");
    number
        .parse::<i64>()
        .map(Value::Int)
        .map_err(|_| anyhow!("line {line}: unsupported value `{text}`"))
}

/// Decodes the escape sequences the registries use in double-quoted strings.
fn unescape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_root_scalars_and_array_of_tables() {
        let text = "# comment\nformat = 1\n\n[[row]]\nname = \"a\"\nflag = true\n\n[[row]]\nname = 'b'\ncount = 3\n";
        let doc = parse(text).expect("parse");
        assert_eq!(doc.root.get("format"), Some(&Value::Int(1)));
        let rows: Vec<_> = doc.tables_named("row").collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].values.get("name"), Some(&Value::Str("a".into())));
        assert_eq!(rows[0].values.get("flag"), Some(&Value::Bool(true)));
        assert_eq!(rows[1].values.get("name"), Some(&Value::Str("b".into())));
        assert_eq!(rows[1].values.get("count"), Some(&Value::Int(3)));
    }

    #[test]
    fn parses_named_tables_and_escapes() {
        let text = "format = 1\n[profiles.mid]\nminutes = 60\nlabel = \"a\\nb\"\n";
        let doc = parse(text).expect("parse");
        let table = doc.tables_named("profiles.mid").next().expect("table");
        assert_eq!(table.values.get("minutes"), Some(&Value::Int(60)));
        assert_eq!(table.values.get("label"), Some(&Value::Str("a\nb".into())));
    }

    #[test]
    fn comments_inside_strings_are_preserved() {
        let doc = parse("k = \"a # b\"\n").expect("parse");
        assert_eq!(doc.root.get("k"), Some(&Value::Str("a # b".into())));
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse("this is not toml\n").is_err());
        assert!(parse("k = \n").is_err());
        assert!(parse("[[row\n").is_err());
    }
}
