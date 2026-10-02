// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/UiDslParser.java.

//! MSUI DSL parser (plan 14 §3.6/§6.2).
//!
//! Exact grammar port of `UiDslParser`: `key: value` properties, `row`,
//! `node { … }`, shorthand `label: "text"` / `image: "region"`, bare childless
//! nodes, `//` comments, `\n` escapes and quoted-vs-bare coercion. Errors carry
//! the 1-based Java line number.

use crate::ui::builder::ui_key::UiKey;
use crate::ui::builder::ui_node::{UiNode, UiValue};

/// A DSL parse error with the upstream line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DslError {
    /// 1-based line number.
    pub line: usize,
    /// Human-readable reason.
    pub message: String,
}

impl std::fmt::Display for DslError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at line {}", self.message, self.line)
    }
}

impl std::error::Error for DslError {}

/// Parses source text into an implicit root table node.
pub fn parse(source: &str) -> Result<UiNode, DslError> {
    let mut parser = Parser {
        chars: source.chars().collect(),
        pos: 0,
        last_quoted: false,
    };
    let mut root = UiNode::new(UiKey::Table);
    parser.parse_statements_into(&mut root)?;
    parser.skip_ws();
    if !parser.at_end() {
        return Err(parser.error(format!(
            "Unexpected character '{}'",
            parser.peek().unwrap_or('\0')
        )));
    }
    Ok(root)
}

struct Parser {
    chars: Vec<char>,
    pos: usize,
    last_quoted: bool,
}

impl Parser {
    fn parse_statements_into(&mut self, node: &mut UiNode) -> Result<(), DslError> {
        self.skip_ws();
        while !self.at_end() && self.peek() != Some('}') {
            self.parse_statement(node)?;
            self.skip_ws();
        }
        Ok(())
    }

    fn parse_statement(&mut self, node: &mut UiNode) -> Result<(), DslError> {
        self.skip_ws();
        if self.at_end() || self.peek() == Some('"') {
            return Err(self.error("Expected identifier"));
        }
        let ident = self.read_bare_token();
        if ident.is_empty() {
            return Err(self.error("Unexpected character"));
        }

        if ident == "row" {
            node.push(UiKey::Row, UiValue::Bool(true));
            return Ok(());
        }

        let key = UiKey::from_name(&ident)
            .ok_or_else(|| self.error(format!("Unknown property: \"{ident}\"")))?;
        let is_node_type = key.is_node_type();
        self.skip_ws();

        // "ident: value" — shorthand node or plain property.
        if self.peek() == Some(':') {
            self.pos += 1;
            let raw = self.parse_value()?;

            if is_node_type {
                let mut child = UiNode::new(key);
                child.push(shorthand_type(key), UiValue::Str(raw));
                self.skip_ws();
                if self.peek() == Some('{') {
                    self.pos += 1;
                    self.parse_statements_into(&mut child)?;
                    self.expect('}')?;
                }
                node.push(key, UiValue::Node(Box::new(child)));
            } else {
                let quoted = self.last_quoted;
                node.push(key, coerce(&raw, quoted));
            }
            return Ok(());
        }

        // "ident { ... }" — a node with a block.
        if self.peek() == Some('{') {
            self.pos += 1;
            if !is_node_type {
                return Err(self.error(format!("Unknown node type '{ident}'")));
            }
            let mut child = UiNode::new(key);
            self.parse_statements_into(&mut child)?;
            self.expect('}')?;
            node.push(key, UiValue::Node(Box::new(child)));
            return Ok(());
        }

        // Bare "ident" — childless node (e.g. "space").
        if !is_node_type {
            return Err(self.error(format!("Unknown node type '{ident}'")));
        }
        node.push(key, UiValue::Node(Box::new(UiNode::new(key))));
        Ok(())
    }

    /// Parses a value token, recording whether it was quoted.
    fn parse_value(&mut self) -> Result<String, DslError> {
        self.skip_ws();
        if self.peek() == Some('"') {
            self.last_quoted = true;
            return self.parse_string();
        }
        self.last_quoted = false;
        let tok = self.read_bare_token();
        if tok.is_empty() {
            return Err(self.error("Expected value"));
        }
        Ok(tok)
    }

    fn parse_string(&mut self) -> Result<String, DslError> {
        self.pos += 1; // opening quote
        let mut out = String::new();
        while !self.at_end() && self.peek() != Some('"') {
            let c = self.current().unwrap_or('\0');
            if c == '\\' && self.pos + 1 < self.chars.len() {
                let next = self.chars[self.pos + 1];
                out.push(if next == 'n' { '\n' } else { next });
                self.pos += 2;
                continue;
            }
            out.push(c);
            self.pos += 1;
        }
        if self.at_end() {
            return Err(self.error("Unterminated string"));
        }
        self.pos += 1; // closing quote
        Ok(out)
    }

    /// Reads until whitespace/brace/colon.
    fn read_bare_token(&mut self) -> String {
        let start = self.pos;
        while !self.at_end() {
            let c = self.current().unwrap_or('\0');
            if c.is_whitespace() || c == '{' || c == '}' || c == ':' {
                break;
            }
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn skip_ws(&mut self) {
        while !self.at_end() {
            let c = self.current().unwrap_or('\0');
            if c.is_whitespace() {
                self.pos += 1;
                continue;
            }
            if c == '/' && self.pos + 1 < self.chars.len() && self.chars[self.pos + 1] == '/' {
                while !self.at_end() && self.current() != Some('\n') {
                    self.pos += 1;
                }
                continue;
            }
            break;
        }
    }

    fn expect(&mut self, c: char) -> Result<(), DslError> {
        if self.at_end() || self.peek() != Some(c) {
            let found = if self.at_end() {
                "<eof>".to_owned()
            } else {
                self.peek().unwrap_or('\0').to_string()
            };
            return Err(self.error(format!("Expected '{c}', found '{found}'")));
        }
        self.pos += 1;
        Ok(())
    }

    fn at_end(&self) -> bool {
        self.pos >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.current()
    }

    fn current(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn line(&self, index: usize) -> usize {
        self.chars[..index.min(self.chars.len())]
            .iter()
            .filter(|c| **c == '\n')
            .count()
            + 1
    }

    fn error(&self, message: impl Into<String>) -> DslError {
        DslError {
            line: self.line(self.pos),
            message: message.into(),
        }
    }
}

/// Shorthand key: `image` maps to `region`, everything else to `text`.
fn shorthand_type(key: UiKey) -> UiKey {
    match key {
        UiKey::Image => UiKey::Region,
        _ => UiKey::Text,
    }
}

/// Converts a bare token into the most specific value type.
fn coerce(raw: &str, quoted: bool) -> UiValue {
    if quoted {
        return UiValue::Str(raw.to_owned());
    }
    match raw {
        "true" => return UiValue::Bool(true),
        "false" => return UiValue::Bool(false),
        _ => {}
    }
    if let Ok(value) = raw.parse::<f32>() {
        UiValue::F32(value)
    } else {
        UiValue::Str(raw.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child(root: &UiNode, index: usize) -> &UiNode {
        match &root.entries[index].value {
            UiValue::Node(node) => node,
            other => panic!("expected node, got {other:?}"),
        }
    }

    #[test]
    fn dsl_parses_shorthand_and_props() {
        let root = parse("label: \"Hello\"\nspace\nbutton: @ok\n").unwrap();
        assert_eq!(root.entries.len(), 3);
        let label = child(&root, 0);
        assert_eq!(label.node_type, UiKey::Label);
        assert_eq!(label.str_value(UiKey::Text), Some("Hello"));
        let space = child(&root, 1);
        assert_eq!(space.node_type, UiKey::Space);
        assert!(space.entries.is_empty());
        let button = child(&root, 2);
        assert_eq!(button.str_value(UiKey::Text), Some("@ok"));
    }

    #[test]
    fn dsl_parses_blocks_rows_and_coercion() {
        let source = "\
table {
  background: grayPanel
  margin: 4
  wrap: true
  row
  label: \"a b\"
  image: \"icon\" { width: 32 }
}
";
        let root = parse(source).unwrap();
        let table = child(&root, 0);
        assert_eq!(table.node_type, UiKey::Table);
        assert_eq!(table.str_value(UiKey::Background), Some("grayPanel"));
        assert_eq!(table.num(UiKey::Margin), Some(4.0));
        assert!(table.bool_or(UiKey::Wrap, false));
        assert!(
            table
                .entries
                .iter()
                .any(|e| e.key == UiKey::Row && e.value == UiValue::Bool(true))
        );
        let image = table
            .entries
            .iter()
            .find_map(|e| match &e.value {
                UiValue::Node(node) if node.node_type == UiKey::Image => Some(node.as_ref()),
                _ => None,
            })
            .unwrap();
        assert_eq!(image.str_value(UiKey::Region), Some("icon"));
        assert_eq!(image.num(UiKey::Width), Some(32.0));
    }

    #[test]
    fn dsl_parse_roundtrip_corpus() {
        let corpus = [
            "label: \"a\"\n",
            "table {\n  row\n  grow: true\n}\n",
            "button: @ok {\n  clicked: \"yes\"\n  width: 100\n}\n",
            "slider: \"vol\" { min: 0\n max: 1\n step: 0.1\n}\n",
            "// comment\nspace\nrow\n",
        ];
        for source in corpus {
            let tree = parse(source).unwrap();
            let written = super::super::dsl_writer::write(&tree);
            let reparsed = parse(&written).unwrap();
            assert_eq!(tree, reparsed, "round-trip failed for {source:?}");
        }
    }

    #[test]
    fn dsl_error_has_line_number() {
        let error = parse("label: \"a\"\nbogus: 1\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.to_string().contains("Unknown property"));
        assert!(error.to_string().contains("line 2"));

        let error = parse("table {\n").unwrap_err();
        assert!(error.to_string().contains("Expected '}'"));
    }

    #[test]
    fn dsl_quotes_and_comments() {
        let root = parse("// a comment\nlabel: \"a\\nb\"\n").unwrap();
        let label = child(&root, 0);
        assert_eq!(label.str_value(UiKey::Text), Some("a\nb"));
        let root = parse("field: \"123\"\n").unwrap();
        let field = child(&root, 0);
        assert_eq!(field.str_value(UiKey::Text), Some("123"));
    }
}
