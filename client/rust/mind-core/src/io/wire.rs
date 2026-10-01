// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Big-endian wire primitives (plan 04 §3.3, deviation 2).
//!
//! Ported from Arc `Writes`/`Reads` (`arc.util.io`) and Java `DataOutput`/
//! `DataInput` semantics: big-endian integers/floats, `bool` as `u8`, strings as
//! `u16` byte-length + UTF-8 (plain UTF-8, not Java modified-UTF — plan 04 R10).
//! Method names mirror Arc (`b/ub/s/us/i/l/f/d/bool/str`) so ported codecs read
//! like the upstream call sites.

use super::{IoError, StringMap};

/// Maximum byte length of one length-prefixed wire string (`u16` field).
pub const MAX_STRING_BYTES: usize = u16::MAX as usize;

/// Vec-backed writer of big-endian wire primitives (`Writes` equivalent).
///
/// Writing to a `Vec` is infallible; only length-validated operations return
/// `Result`.
pub struct WireWriter<'a> {
    out: &'a mut Vec<u8>,
}

impl<'a> WireWriter<'a> {
    /// Wraps an output buffer.
    pub fn new(out: &'a mut Vec<u8>) -> Self {
        Self { out }
    }

    /// Current position (bytes written so far).
    pub fn pos(&self) -> usize {
        self.out.len()
    }

    /// `i8`/`u8` byte (`Writes.b`).
    pub fn b(&mut self, value: i8) {
        self.out.push(value as u8);
    }

    /// Unsigned byte (`Writes.ub`-style sites use `b`; kept distinct for reads).
    pub fn ub(&mut self, value: u8) {
        self.out.push(value);
    }

    /// Boolean as one byte (`Writes.bool`).
    pub fn bool(&mut self, value: bool) {
        self.out.push(u8::from(value));
    }

    /// Big-endian `i16` (`Writes.s`).
    pub fn s(&mut self, value: i16) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// Big-endian `u16` (`Writes.us`).
    pub fn us(&mut self, value: u16) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// Big-endian `i32` (`Writes.i`).
    pub fn i(&mut self, value: i32) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// Big-endian `u32`.
    pub fn u(&mut self, value: u32) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// Big-endian `i64` (`Writes.l`).
    pub fn l(&mut self, value: i64) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// Big-endian `f32` (`Writes.f`).
    pub fn f(&mut self, value: f32) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// Big-endian `f64` (`Writes.d`).
    pub fn d(&mut self, value: f64) {
        self.out.extend_from_slice(&value.to_be_bytes());
    }

    /// Raw bytes (`Writes.b(byte[])`).
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.out.extend_from_slice(bytes);
    }

    /// Length-prefixed UTF-8 string: `u16` byte length + bytes
    /// (`DataOutput.writeUTF` framing, plain UTF-8 payload).
    pub fn str(&mut self, value: &str) -> Result<(), IoError> {
        let bytes = value.as_bytes();
        if bytes.len() > MAX_STRING_BYTES {
            return Err(IoError::TooLarge {
                limit: MAX_STRING_BYTES,
                actual: bytes.len(),
            });
        }
        self.us(bytes.len() as u16);
        self.bytes(bytes);
        Ok(())
    }

    /// String map: `i16` count + (key, value) pairs (`SaveFileReader.writeStringMap`).
    pub fn string_map(&mut self, map: &StringMap) -> Result<(), IoError> {
        if map.len() > i16::MAX as usize {
            return Err(IoError::InvalidStringMapSize(map.len() as i32));
        }
        self.s(map.len() as i16);
        for (key, value) in map {
            self.str(key)?;
            self.str(value)?;
        }
        Ok(())
    }
}

impl std::io::Write for WireWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.out.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Slice-backed reader of big-endian wire primitives (`Reads` equivalent).
///
/// Every read is bounds-checked; running past the end yields
/// [`IoError::UnexpectedEof`] (safe-read semantics, plan 04 §3.12.5).
pub struct WireReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> WireReader<'a> {
    /// Wraps an input slice.
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Total length of the underlying slice.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether the slice is empty.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Current read position.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Bytes left to read.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Skips `n` bytes (`Reads.skip`).
    pub fn skip(&mut self, n: usize) -> Result<(), IoError> {
        self.take(n).map(|_| ())
    }

    /// Skips everything left (used to tolerate unknown trailing region data).
    pub fn skip_to_end(&mut self) {
        self.pos = self.buf.len();
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], IoError> {
        if self.remaining() < n {
            return Err(IoError::UnexpectedEof);
        }
        let start = self.pos;
        self.pos += n;
        Ok(&self.buf[start..self.pos])
    }

    /// `i8` (`Reads.b`).
    pub fn b(&mut self) -> Result<i8, IoError> {
        Ok(self.take(1)?[0] as i8)
    }

    /// `u8` (`Reads.ub`).
    pub fn ub(&mut self) -> Result<u8, IoError> {
        Ok(self.take(1)?[0])
    }

    /// Boolean (`Reads.bool`): any nonzero byte is `true`.
    pub fn bool(&mut self) -> Result<bool, IoError> {
        Ok(self.ub()? != 0)
    }

    /// Big-endian `i16` (`Reads.s`).
    pub fn s(&mut self) -> Result<i16, IoError> {
        let bytes = self.take(2)?;
        Ok(i16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Big-endian `u16` (`Reads.us`).
    pub fn us(&mut self) -> Result<u16, IoError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Big-endian `i32` (`Reads.i`).
    pub fn i(&mut self) -> Result<i32, IoError> {
        let bytes = self.take(4)?;
        Ok(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Big-endian `u32`.
    pub fn u(&mut self) -> Result<u32, IoError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Big-endian `i64` (`Reads.l`).
    pub fn l(&mut self) -> Result<i64, IoError> {
        let bytes = self.take(8)?;
        let mut raw = [0u8; 8];
        raw.copy_from_slice(bytes);
        Ok(i64::from_be_bytes(raw))
    }

    /// Big-endian `f32` (`Reads.f`).
    pub fn f(&mut self) -> Result<f32, IoError> {
        let bytes = self.take(4)?;
        Ok(f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Big-endian `f64` (`Reads.d`).
    pub fn d(&mut self) -> Result<f64, IoError> {
        let bytes = self.take(8)?;
        let mut raw = [0u8; 8];
        raw.copy_from_slice(bytes);
        Ok(f64::from_be_bytes(raw))
    }

    /// `n` raw bytes (`Reads.b(byte[])`).
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], IoError> {
        self.take(n)
    }

    /// Length-prefixed UTF-8 string (`DataInput.readUTF` framing).
    pub fn str(&mut self) -> Result<String, IoError> {
        let len = self.us()? as usize;
        let bytes = self.take(len)?;
        Ok(std::str::from_utf8(bytes)?.to_owned())
    }

    /// Length-prefixed string with a byte cap (`Reads.str(max)` safe reads).
    pub fn str_capped(&mut self, cap: usize) -> Result<String, IoError> {
        let len = self.us()? as usize;
        if len > cap {
            return Err(IoError::TooLarge {
                limit: cap,
                actual: len,
            });
        }
        let bytes = self.take(len)?;
        Ok(std::str::from_utf8(bytes)?.to_owned())
    }

    /// String map (`SaveFileReader.readStringMap`).
    pub fn string_map(&mut self) -> Result<StringMap, IoError> {
        let count = self.s()?;
        if count < 0 {
            return Err(IoError::InvalidStringMapSize(count as i32));
        }
        let mut map = StringMap::with_capacity(count as usize);
        for _ in 0..count {
            let key = self.str()?;
            let value = self.str()?;
            map.insert(key, value);
        }
        Ok(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_roundtrip_big_endian() {
        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            w.b(-5);
            w.ub(200);
            w.bool(true);
            w.s(-1234);
            w.us(60_000);
            w.i(-2_000_000_000);
            w.u(4_000_000_000);
            w.l(-9_000_000_000_000);
            w.f(1.5);
            w.d(-2.25);
        }
        // Spot-check big-endian framing of the u16 60000 (0xEA60 at bytes 5..7).
        assert_eq!(&buf[5..7], &[0xEA, 0x60]);
        let mut r = WireReader::new(&buf);
        assert_eq!(r.b().unwrap(), -5);
        assert_eq!(r.ub().unwrap(), 200);
        assert!(r.bool().unwrap());
        assert_eq!(r.s().unwrap(), -1234);
        assert_eq!(r.us().unwrap(), 60_000);
        assert_eq!(r.i().unwrap(), -2_000_000_000);
        assert_eq!(r.u().unwrap(), 4_000_000_000);
        assert_eq!(r.l().unwrap(), -9_000_000_000_000);
        assert_eq!(r.f().unwrap(), 1.5);
        assert_eq!(r.d().unwrap(), -2.25);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn string_and_map_roundtrip() {
        let mut map = StringMap::new();
        map.insert("wave".to_owned(), "12".to_owned());
        map.insert("日本語".to_owned(), "🗺️ emoji".to_owned());

        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            w.string_map(&map).unwrap();
        }
        let mut r = WireReader::new(&buf);
        let back = r.string_map().unwrap();
        assert_eq!(back, map);
    }

    #[test]
    fn reads_are_bounds_checked() {
        let buf = [0u8, 1];
        let mut r = WireReader::new(&buf);
        assert!(matches!(r.i(), Err(IoError::UnexpectedEof)));
        let mut r = WireReader::new(&buf);
        assert!(matches!(r.bytes(3), Err(IoError::UnexpectedEof)));
        let mut r = WireReader::new(&buf);
        assert!(matches!(r.str(), Err(IoError::UnexpectedEof)));
    }

    #[test]
    fn negative_string_map_size_rejected() {
        let mut buf = Vec::new();
        {
            let mut w = WireWriter::new(&mut buf);
            w.s(-1);
        }
        let mut r = WireReader::new(&buf);
        assert!(matches!(
            r.string_map(),
            Err(IoError::InvalidStringMapSize(-1))
        ));
    }
}
