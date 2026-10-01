// SPDX-License-Identifier: GPL-3.0-only

//! Minimal RGBA value type for content metadata.
//!
//! Ported from `arc.graphics.Color` usage in
//! `core/src/mindustry/type/{Item,Liquid,StatusEffect}.java` and
//! `core/src/mindustry/entities/bullet/BulletType.java`. This is a plain value
//! type; rendering/premultiplied handling is owned by plan 16.

/// Linear RGBA color, one `f32` per channel (`0.0..=1.0` for authored values).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    /// Red channel.
    pub r: f32,
    /// Green channel.
    pub g: f32,
    /// Blue channel.
    pub b: f32,
    /// Alpha channel.
    pub a: f32,
}

impl Rgba {
    /// Opaque white (`Color.white`).
    pub const WHITE: Rgba = Rgba::new(1.0, 1.0, 1.0, 1.0);
    /// Opaque black (`Color.black`).
    pub const BLACK: Rgba = Rgba::new(0.0, 0.0, 0.0, 1.0);
    /// Transparent black (`Color.clear`).
    pub const CLEAR: Rgba = Rgba::new(0.0, 0.0, 0.0, 0.0);
    /// `Color.lightGray`.
    pub const LIGHT_GRAY: Rgba = Rgba::new(0.8, 0.8, 0.8, 1.0);
    /// `Color.gray`.
    pub const GRAY: Rgba = Rgba::new(0.5, 0.5, 0.5, 1.0);
    /// `Color.royal`.
    pub const ROYAL: Rgba = Rgba::new(0.25, 0.25, 0.9, 1.0);

    /// Builds a color from raw channels.
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Returns the same color with a replaced alpha channel (`Color.a`).
    pub const fn with_alpha(mut self, a: f32) -> Self {
        self.a = a;
        self
    }

    /// Builds a color from `0xRRGGBB`/`0xRRGGBBAA` (`Color.rgba8888`).
    pub const fn from_rgba8888(value: u32) -> Self {
        Self {
            r: ((value >> 24) & 0xff) as f32 / 255.0,
            g: ((value >> 16) & 0xff) as f32 / 255.0,
            b: ((value >> 8) & 0xff) as f32 / 255.0,
            a: (value & 0xff) as f32 / 255.0,
        }
    }

    /// Packs to `0xRRGGBBAA` (`Color.rgba()`); channels truncate like the
    /// upstream float→byte casts.
    pub fn to_rgba8888(self) -> u32 {
        let r = (self.r * 255.0) as u32 & 0xff;
        let g = (self.g * 255.0) as u32 & 0xff;
        let b = (self.b * 255.0) as u32 & 0xff;
        let a = (self.a * 255.0) as u32 & 0xff;
        (r << 24) | (g << 16) | (b << 8) | a
    }

    /// Parses a Mindustry hex color string (`"d99d73"` or `"d99d73ff"`).
    ///
    /// Returns `None` for malformed input; call sites use [`Option::unwrap_or`]
    /// with a known-good fallback (no panics on content data).
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix("0x").unwrap_or(hex);
        let value = u32::from_str_radix(hex, 16).ok()?;
        Some(match hex.len() {
            6 => Self::from_rgba8888((value << 8) | 0xff),
            8 => Self::from_rgba8888(value),
            _ => return None,
        })
    }
}

impl Default for Rgba {
    fn default() -> Self {
        Self::WHITE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing_matches_value_of() {
        let copper = Rgba::from_hex("d99d73").unwrap();
        assert_eq!(copper.a, 1.0);
        assert!((copper.r - 0xd9 as f32 / 255.0).abs() < f32::EPSILON);
        assert!((copper.g - 0x9d as f32 / 255.0).abs() < f32::EPSILON);
        assert!((copper.b - 0x73 as f32 / 255.0).abs() < f32::EPSILON);
        assert_eq!(Rgba::from_hex("nope"), None);
    }
}
