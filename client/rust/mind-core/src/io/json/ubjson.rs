// Ported from Arc (https://github.com/Anuken/Arc) — Apache-2.0 / Mindustry GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Arc `UBJsonReader` (`arc/util/serialization/UBJsonReader.java`) decoding
//! into a [`serde_json::Value`].
//!
//! Java's `Settings.putJson` (`Arc Settings.java:331`) serializes through a
//! `UBJsonWriter` and stores the bytes as a binary (type 5) value: sector info
//! (`<planet>-s-<id>-info`), campaign rules/stats, `launch-resources-seq`,
//! `servers`, `blocks-built`, ... A Game Data import of a Java profile carries
//! those bytes verbatim, so [`crate::io::settings::SettingsStore::get_json`]
//! decodes them here and feeds the value through the same serde types the
//! native JSON path uses.

use serde_json::{Map, Number, Value};

use crate::io::IoError;

/// Nesting cap: the input is untrusted (imported archives).
const MAX_DEPTH: usize = 128;

/// Decodes one Arc UBJSON value (`UBJsonReader.parse`).
pub fn read_ubjson(bytes: &[u8]) -> Result<Value, IoError> {
    let mut reader = Reader { bytes, pos: 0 };
    let kind = reader.byte()?;
    reader.value(kind, 0)
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, len: usize) -> Result<&[u8], IoError> {
        let end = self.pos.checked_add(len).ok_or(IoError::UnexpectedEof)?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or(IoError::UnexpectedEof)?;
        self.pos = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8, IoError> {
        Ok(self.take(1)?[0])
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], IoError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn available(&self) -> bool {
        self.pos < self.bytes.len()
    }

    fn float(value: f64) -> Value {
        Number::from_f64(value).map_or(Value::Null, Value::Number)
    }

    /// `UBJsonReader.parse(din, type)`.
    fn value(&mut self, kind: u8, depth: usize) -> Result<Value, IoError> {
        if depth > MAX_DEPTH {
            return Err(IoError::corrupt("UBJSON nesting too deep"));
        }
        Ok(match kind {
            b'[' => self.parse_array(depth + 1)?,
            b'{' => self.parse_object(depth + 1)?,
            b'Z' => Value::Null,
            b'T' => Value::Bool(true),
            b'F' => Value::Bool(false),
            b'B' | b'U' => Value::from(self.byte()?),
            b'i' => Value::from(self.byte()? as i8),
            b'I' => Value::from(i16::from_be_bytes(self.array()?)),
            b'l' => Value::from(i32::from_be_bytes(self.array()?)),
            b'L' => Value::from(i64::from_be_bytes(self.array()?)),
            b'd' => Self::float(f64::from(f32::from_be_bytes(self.array()?))),
            b'D' => Self::float(f64::from_be_bytes(self.array()?)),
            b's' | b'S' => Value::String(self.parse_string(false, kind)?),
            b'a' | b'A' => self.parse_data(kind, depth + 1)?,
            b'C' => {
                let unit = u16::from_be_bytes(self.array()?);
                Value::String(
                    char::from_u32(u32::from(unit))
                        .unwrap_or('\u{FFFD}')
                        .to_string(),
                )
            }
            other => {
                return Err(IoError::corrupt(format!(
                    "unrecognized UBJSON data type {other:#04x}"
                )));
            }
        })
    }

    /// Reads the optional `$type` / `#count` container header; returns
    /// `(value_type, count, next_type_byte)`.
    fn container_header(&mut self) -> Result<(u8, i64, u8), IoError> {
        let mut kind = self.byte()?;
        let mut value_type = 0u8;
        if kind == b'$' {
            value_type = self.byte()?;
            kind = self.byte()?;
        }
        let mut size = -1i64;
        if kind == b'#' {
            let size_type = self.byte()?;
            size = self.parse_size(size_type, false, -1)?;
            if size < 0 {
                return Err(IoError::corrupt("UBJSON container size missing"));
            }
            if size > 0 {
                kind = if value_type == 0 {
                    self.byte()?
                } else {
                    value_type
                };
            }
        }
        Ok((value_type, size, kind))
    }

    fn parse_array(&mut self, depth: usize) -> Result<Value, IoError> {
        let (value_type, size, mut kind) = self.container_header()?;
        let mut out = Vec::new();
        if size == 0 {
            return Ok(Value::Array(out));
        }
        let mut count = 0i64;
        while self.available() && kind != b']' {
            out.push(self.value(kind, depth)?);
            count += 1;
            if size > 0 && count >= size {
                break;
            }
            kind = if value_type == 0 {
                self.byte()?
            } else {
                value_type
            };
        }
        Ok(Value::Array(out))
    }

    fn parse_object(&mut self, depth: usize) -> Result<Value, IoError> {
        let (value_type, size, first) = self.container_header()?;
        let mut out = Map::new();
        if size == 0 {
            return Ok(Value::Object(out));
        }
        // With a typed (`$`) header the first key-type byte was consumed as the
        // value type by `container_header`; re-read it like upstream.
        let mut kind = if size > 0 && value_type != 0 {
            self.byte()?
        } else {
            first
        };
        let mut count = 0i64;
        while self.available() && kind != b'}' {
            let key = self.parse_string(true, kind)?;
            let value_kind = if value_type == 0 {
                self.byte()?
            } else {
                value_type
            };
            let value = self.value(value_kind, depth)?;
            out.insert(key, value);
            count += 1;
            if size > 0 && count >= size {
                break;
            }
            kind = self.byte()?;
        }
        Ok(Value::Object(out))
    }

    fn parse_data(&mut self, block: u8, depth: usize) -> Result<Value, IoError> {
        let data_type = self.byte()?;
        let size = if block == b'A' {
            i64::from(i32::from_be_bytes(self.array()?))
        } else {
            i64::from(self.byte()?)
        };
        let mut out = Vec::new();
        for _ in 0..size.max(0) {
            out.push(self.value(data_type, depth)?);
        }
        Ok(Value::Array(out))
    }

    fn parse_string(&mut self, optional_marker: bool, kind: u8) -> Result<String, IoError> {
        let size = match kind {
            b'S' => {
                let size_type = self.byte()?;
                self.parse_size(size_type, true, -1)?
            }
            b's' => i64::from(self.byte()?),
            _ if optional_marker => self.parse_size(kind, false, -1)?,
            _ => -1,
        };
        if size < 0 {
            return Err(IoError::corrupt("UBJSON string expected"));
        }
        let size = usize::try_from(size).map_err(|_| IoError::UnexpectedEof)?;
        if size > self.bytes.len() {
            return Err(IoError::UnexpectedEof);
        }
        Ok(String::from_utf8_lossy(self.take(size)?).into_owned())
    }

    /// `UBJsonReader.parseSize`: `i` is read *unsigned* here.
    fn parse_size(&mut self, kind: u8, int_on_error: bool, default: i64) -> Result<i64, IoError> {
        Ok(match kind {
            b'i' => i64::from(self.byte()?),
            b'I' => i64::from(u16::from_be_bytes(self.array()?)),
            b'l' => i64::from(i32::from_be_bytes(self.array()?)),
            b'L' => i64::from_be_bytes(self.array()?),
            _ if int_on_error => {
                let rest: [u8; 3] = self.array()?;
                i64::from(u32::from_be_bytes([kind, rest[0], rest[1], rest[2]]))
            }
            _ => default,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_arc_sector_info_shape() {
        // {"items":{},"shown":T,"winWave":i10,"origin":S"serpulo-15",
        //  "resources":[s"copper"],"sumHealth":d990.0}
        let mut bytes = vec![b'{'];
        let key = |out: &mut Vec<u8>, name: &str| {
            out.push(b'i');
            out.push(name.len() as u8);
            out.extend_from_slice(name.as_bytes());
        };
        key(&mut bytes, "items");
        bytes.extend_from_slice(b"{}");
        key(&mut bytes, "shown");
        bytes.push(b'T');
        key(&mut bytes, "winWave");
        bytes.extend_from_slice(&[b'i', 10]);
        key(&mut bytes, "origin");
        bytes.extend_from_slice(&[b'S', b'i', 10]);
        bytes.extend_from_slice(b"serpulo-15");
        key(&mut bytes, "resources");
        bytes.extend_from_slice(&[b'[', b'S', b'i', 6]);
        bytes.extend_from_slice(b"copper");
        bytes.push(b']');
        key(&mut bytes, "sumHealth");
        bytes.push(b'd');
        bytes.extend_from_slice(&990.0f32.to_be_bytes());
        bytes.push(b'}');

        let value = read_ubjson(&bytes).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "items": {}, "shown": true, "winWave": 10, "origin": "serpulo-15",
                "resources": ["copper"], "sumHealth": 990.0
            })
        );
        let info: crate::io::json::SectorInfo = serde_json::from_value(value).unwrap();
        assert!(info.shown);
        assert_eq!(info.win_wave, 10);
        assert!(info.has_core, "absent fields keep the prototype default");
    }

    #[test]
    fn truncated_input_is_an_error() {
        assert!(read_ubjson(b"{i\x05ite").is_err());
        assert!(read_ubjson(b"").is_err());
    }
}
