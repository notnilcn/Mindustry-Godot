// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Canonical deterministic checksum (plan 05 §6.5; HLP §12 C2).
//!
//! The P0 xxh3 dump hash is replaced by this FNV-1a-64 stream. The hashing order
//! is versioned (`CHECKSUM_VERSION`); any change to the stream bumps the version
//! and all peers compare versions before desync judgment (plan 21).

use crate::constants::CHECKSUM_VERSION;

/// FNV-1a 64-bit offset basis.
pub const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A finalized checksum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Checksum(pub u64);

impl Checksum {
    /// Raw value.
    pub const fn value(self) -> u64 {
        self.0
    }

    /// 16 lowercase hex digits (golden/report format).
    pub fn to_hex(self) -> String {
        format!("{:016x}", self.0)
    }
}

/// Incremental FNV-1a-64 hasher (locally implemented; no dependency).
#[derive(Debug, Clone)]
pub struct Hasher {
    state: u64,
    len: u64,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    /// Starts a new stream.
    pub const fn new() -> Self {
        Self {
            state: FNV_OFFSET_BASIS,
            len: 0,
        }
    }

    /// Number of bytes written.
    pub const fn len(&self) -> u64 {
        self.len
    }

    /// Whether nothing has been written.
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Folds a byte slice.
    pub fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
        self.len = self.len.wrapping_add(bytes.len() as u64);
    }

    /// Folds a `u8`.
    pub fn write_u8(&mut self, value: u8) {
        self.write(&[value]);
    }

    /// Folds a `bool` (`0`/`1`).
    pub fn write_bool(&mut self, value: bool) {
        self.write_u8(u8::from(value));
    }

    /// Folds a `u16` (LE).
    pub fn write_u16(&mut self, value: u16) {
        self.write(&value.to_le_bytes());
    }

    /// Folds a `u32` (LE).
    pub fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }

    /// Folds a `u64` (LE).
    pub fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    /// Folds an `i32` (LE).
    pub fn write_i32(&mut self, value: i32) {
        self.write(&value.to_le_bytes());
    }

    /// Folds an `f32` by bit pattern (LE).
    pub fn write_f32(&mut self, value: f32) {
        self.write(&value.to_bits().to_le_bytes());
    }

    /// Folds an `f64` by bit pattern (LE).
    pub fn write_f64(&mut self, value: f64) {
        self.write(&value.to_bits().to_le_bytes());
    }

    /// Finalizes the stream.
    pub const fn finish(&self) -> Checksum {
        Checksum(self.state)
    }
}

/// A type that contributes to the canonical checksum stream.
pub trait ChecksumPart {
    /// Writes this value into `hasher`.
    fn checksum_into(&self, hasher: &mut Hasher);
}

impl ChecksumPart for u8 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_u8(*self);
    }
}

impl ChecksumPart for u16 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_u16(*self);
    }
}

impl ChecksumPart for u32 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_u32(*self);
    }
}

impl ChecksumPart for u64 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_u64(*self);
    }
}

impl ChecksumPart for i8 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_u8(*self as u8);
    }
}

impl ChecksumPart for i16 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write(&self.to_le_bytes());
    }
}

impl ChecksumPart for i32 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_i32(*self);
    }
}

impl ChecksumPart for i64 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write(&self.to_le_bytes());
    }
}

impl ChecksumPart for f64 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_f64(*self);
    }
}

impl ChecksumPart for f32 {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_f32(*self);
    }
}

impl ChecksumPart for bool {
    fn checksum_into(&self, hasher: &mut Hasher) {
        hasher.write_bool(*self);
    }
}

/// A checksummer that prefixes the stream with [`CHECKSUM_VERSION`].
pub struct Checksummer {
    hasher: Hasher,
}

impl Default for Checksummer {
    fn default() -> Self {
        Self::new()
    }
}

impl Checksummer {
    /// Starts a versioned stream.
    pub fn new() -> Self {
        let mut hasher = Hasher::new();
        hasher.write_u32(CHECKSUM_VERSION);
        Self { hasher }
    }

    /// Writes a checksum part.
    pub fn part<T: ChecksumPart + ?Sized>(&mut self, value: &T) {
        value.checksum_into(&mut self.hasher);
    }

    /// Access to the raw hasher for complex parts.
    pub fn hasher(&mut self) -> &mut Hasher {
        &mut self.hasher
    }

    /// Finalizes.
    pub const fn finish(&self) -> Checksum {
        self.hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_known_vector() {
        // FNV-1a-64 of the empty string is the offset basis.
        assert_eq!(Hasher::new().finish().value(), FNV_OFFSET_BASIS);
        // FNV-1a-64 of "a".
        let mut hasher = Hasher::new();
        hasher.write(b"a");
        assert_eq!(hasher.finish().value(), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn versioned_stream_changes_with_version() {
        let checksum = Checksummer::new().finish();
        assert_ne!(checksum.value(), FNV_OFFSET_BASIS);
        assert_eq!(checksum.to_hex().len(), 16);
    }

    #[test]
    fn parts_are_order_sensitive() {
        let mut a = Checksummer::new();
        a.part(&1u32);
        a.part(&2u32);
        let mut b = Checksummer::new();
        b.part(&2u32);
        b.part(&1u32);
        assert_ne!(a.finish(), b.finish());
    }
}
