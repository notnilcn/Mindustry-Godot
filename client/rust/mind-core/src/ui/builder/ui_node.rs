// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/builder/UiBuilder.java (`NodeBuilder`/`Entry`).

//! Typed UI tree (`UiNode`) and the `ui_node` wire codec (plan 14 §6.1).
//!
//! Mirrors Java `NodeBuilder`/`Entry`: a node has a type plus an ordered entry
//! list; entries hold a string, `f32`, `bool` or a nested node. The wire format
//! is our own versioned `format: 1` (header byte + node), used inside plan-21
//! relay rows.

use crate::ui::builder::ui_key::UiKey;

/// Header byte for the `ui_node` wire format.
pub const UI_NODE_FORMAT: u8 = 1;

/// One entry value.
#[derive(Debug, Clone, PartialEq)]
pub enum UiValue {
    /// String payload (tag 0).
    Str(String),
    /// Float payload (tag 1).
    F32(f32),
    /// Boolean payload (tag 2).
    Bool(bool),
    /// Nested node (tag 3).
    Node(Box<UiNode>),
}

impl UiValue {
    /// Wire tag byte.
    pub fn tag(&self) -> u8 {
        match self {
            UiValue::Str(_) => 0,
            UiValue::F32(_) => 1,
            UiValue::Bool(_) => 2,
            UiValue::Node(_) => 3,
        }
    }
}

/// One node entry.
#[derive(Debug, Clone, PartialEq)]
pub struct UiEntry {
    /// Entry key.
    pub key: UiKey,
    /// Entry value.
    pub value: UiValue,
}

impl UiEntry {
    /// Builds an entry.
    pub fn new(key: UiKey, value: UiValue) -> Self {
        Self { key, value }
    }
}

/// A typed UI node (Java `NodeBuilder`).
#[derive(Debug, Clone, PartialEq)]
pub struct UiNode {
    /// Node type (one of the container/element keys).
    pub node_type: UiKey,
    /// Ordered entries.
    pub entries: Vec<UiEntry>,
}

impl UiNode {
    /// Creates an empty node of `node_type`.
    pub fn new(node_type: UiKey) -> Self {
        Self {
            node_type,
            entries: Vec::new(),
        }
    }

    /// Pushes a string property.
    pub fn str(mut self, key: UiKey, value: impl Into<String>) -> Self {
        self.entries
            .push(UiEntry::new(key, UiValue::Str(value.into())));
        self
    }

    /// Pushes a float property.
    pub fn f32(mut self, key: UiKey, value: f32) -> Self {
        self.entries.push(UiEntry::new(key, UiValue::F32(value)));
        self
    }

    /// Pushes a boolean property.
    pub fn bool(mut self, key: UiKey, value: bool) -> Self {
        self.entries.push(UiEntry::new(key, UiValue::Bool(value)));
        self
    }

    /// Pushes a nested child node.
    pub fn child(mut self, node: UiNode) -> Self {
        self.entries
            .push(UiEntry::new(node.node_type, UiValue::Node(Box::new(node))));
        self
    }

    /// Adds a raw entry (used by the DSL parser for exact key ordering).
    pub fn push(&mut self, key: UiKey, value: UiValue) {
        self.entries.push(UiEntry::new(key, value));
    }

    /// First string value for `key`.
    pub fn str_value(&self, key: UiKey) -> Option<&str> {
        self.entries.iter().find_map(|entry| match &entry.value {
            UiValue::Str(value) if entry.key == key => Some(value.as_str()),
            _ => None,
        })
    }

    /// String value for `key`, falling back to `default`.
    pub fn str_or<'a>(&'a self, key: UiKey, default: &'a str) -> &'a str {
        self.str_value(key).unwrap_or(default)
    }

    /// First float value for `key`.
    pub fn num(&self, key: UiKey) -> Option<f32> {
        self.entries.iter().find_map(|entry| match entry.value {
            UiValue::F32(value) if entry.key == key => Some(value),
            _ => None,
        })
    }

    /// Float value for `key`, falling back to `default`.
    pub fn num_or(&self, key: UiKey, default: f32) -> f32 {
        self.num(key).unwrap_or(default)
    }

    /// First boolean value for `key`, falling back to `default`.
    pub fn bool_or(&self, key: UiKey, default: bool) -> bool {
        self.entries
            .iter()
            .find_map(|entry| match entry.value {
                UiValue::Bool(value) if entry.key == key => Some(value),
                _ => None,
            })
            .unwrap_or(default)
    }

    /// Encodes `format: 1` header + this node.
    pub fn encode_message(&self) -> Vec<u8> {
        let mut out = vec![UI_NODE_FORMAT];
        self.encode_into(&mut out);
        out
    }

    /// Encodes this node (without the message header).
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        out.push(self.node_type.ordinal());
        out.extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        for entry in &self.entries {
            out.extend_from_slice(&(entry.key.ordinal() as u16).to_le_bytes());
            out.push(entry.value.tag());
            match &entry.value {
                UiValue::Str(value) => {
                    let bytes = value.as_bytes();
                    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
                    out.extend_from_slice(bytes);
                }
                UiValue::F32(value) => out.extend_from_slice(&value.to_le_bytes()),
                UiValue::Bool(value) => out.push(u8::from(*value)),
                UiValue::Node(node) => node.encode_into(out),
            }
        }
    }

    /// Decodes a `format: 1` message.
    pub fn decode_message(data: &[u8]) -> Result<UiNode, WireError> {
        let mut cursor = Cursor { data, pos: 0 };
        let format = cursor.u8()?;
        if format != UI_NODE_FORMAT {
            return Err(WireError::BadFormat(format));
        }
        let node = UiNode::decode_from(&mut cursor)?;
        if cursor.pos != data.len() {
            return Err(WireError::TrailingBytes(data.len() - cursor.pos));
        }
        Ok(node)
    }

    /// Decodes a node (no message header).
    pub fn decode_from(cursor: &mut Cursor<'_>) -> Result<UiNode, WireError> {
        let type_ord = cursor.u8()?;
        let node_type = UiKey::from_ordinal(type_ord).ok_or(WireError::BadNodeType(type_ord))?;
        let count = cursor.u16()?;
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let key_ord = cursor.u16()?;
            let key = u8::try_from(key_ord)
                .ok()
                .and_then(UiKey::from_ordinal)
                .ok_or(WireError::BadKey(key_ord))?;
            let tag = cursor.u8()?;
            let value = match tag {
                0 => UiValue::Str(cursor.str()?),
                1 => UiValue::F32(cursor.f32()?),
                2 => UiValue::Bool(cursor.u8()? != 0),
                3 => UiValue::Node(Box::new(UiNode::decode_from(cursor)?)),
                other => return Err(WireError::BadTag(other)),
            };
            entries.push(UiEntry::new(key, value));
        }
        Ok(UiNode { node_type, entries })
    }
}

/// `ui_node` codec errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    /// Unknown format header byte.
    BadFormat(u8),
    /// Unknown node-type ordinal.
    BadNodeType(u8),
    /// Unknown key ordinal.
    BadKey(u16),
    /// Unknown value tag.
    BadTag(u8),
    /// Unexpected end of input.
    Eof,
    /// Bytes left over after decoding.
    TrailingBytes(usize),
    /// Invalid UTF-8 in a string payload.
    Utf8,
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WireError::BadFormat(value) => write!(f, "unknown ui_node format {value}"),
            WireError::BadNodeType(value) => write!(f, "unknown node type ordinal {value}"),
            WireError::BadKey(value) => write!(f, "unknown key ordinal {value}"),
            WireError::BadTag(value) => write!(f, "unknown entry tag {value}"),
            WireError::Eof => write!(f, "unexpected end of ui_node input"),
            WireError::TrailingBytes(count) => write!(f, "{count} trailing bytes"),
            WireError::Utf8 => write!(f, "invalid utf-8 in string payload"),
        }
    }
}

impl std::error::Error for WireError {}

/// Byte cursor over a `ui_node` payload.
pub struct Cursor<'a> {
    /// Source bytes.
    pub data: &'a [u8],
    /// Read position.
    pub pos: usize,
}

impl<'a> Cursor<'a> {
    /// Creates a cursor at the start of `data`.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u8(&mut self) -> Result<u8, WireError> {
        let value = *self.data.get(self.pos).ok_or(WireError::Eof)?;
        self.pos += 1;
        Ok(value)
    }

    fn u16(&mut self) -> Result<u16, WireError> {
        if self.pos + 2 > self.data.len() {
            return Err(WireError::Eof);
        }
        let value = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(value)
    }

    fn f32(&mut self) -> Result<f32, WireError> {
        if self.pos + 4 > self.data.len() {
            return Err(WireError::Eof);
        }
        let bytes = [
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ];
        self.pos += 4;
        Ok(f32::from_le_bytes(bytes))
    }

    fn str(&mut self) -> Result<String, WireError> {
        let len = self.u16()? as usize;
        if self.pos + len > self.data.len() {
            return Err(WireError::Eof);
        }
        let bytes = &self.data[self.pos..self.pos + len];
        self.pos += len;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| WireError::Utf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> UiNode {
        UiNode::new(UiKey::Table)
            .str(UiKey::Background, "grayPanel")
            .f32(UiKey::Margin, 4.0)
            .bool(UiKey::Wrap, true)
            .child(
                UiNode::new(UiKey::Label)
                    .str(UiKey::Text, "hello")
                    .str(UiKey::Style, "default"),
            )
            .child(UiNode::new(UiKey::Space))
    }

    #[test]
    fn node_wire_roundtrip() {
        let node = sample();
        let bytes = node.encode_message();
        assert_eq!(bytes[0], UI_NODE_FORMAT);
        let decoded = UiNode::decode_message(&bytes).unwrap();
        assert_eq!(decoded, node);
    }

    #[test]
    fn node_wire_tag_errors() {
        assert_eq!(UiNode::decode_message(&[2]), Err(WireError::BadFormat(2)));
        // header + unknown node type 200
        assert_eq!(
            UiNode::decode_message(&[1, 200]),
            Err(WireError::BadNodeType(200))
        );
        // header + table node + 1 entry + key 0 (table) tag 9
        assert_eq!(
            UiNode::decode_message(&[1, 0, 1, 0, 0, 0, 9]),
            Err(WireError::BadTag(9))
        );
        assert_eq!(UiNode::decode_message(&[]), Err(WireError::Eof));
    }
}
