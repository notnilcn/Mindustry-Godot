// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: arc.scene.ui.layout.Cell / CellDefaults (scene2d cell property surface).

//! Per-cell layout flags for [`super::table::MindTable`].
//!
//! The GDScript port kept a `MindCell` object with a back-reference to its
//! table so every setter could invalidate layout; the Rust port keeps the flags
//! in plain data inside the table and hands out a [`Cell`] guard whose `Drop`
//! invalidates once per chained statement.

use godot::builtin::Color;

/// Arc-style cell alignment (`Align` bit values; `0` = centre).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Align {
    #[default]
    Center = 0,
    #[allow(dead_code)]
    // ported surface; not reachable from the native call graph (kept for parity)
    Top = 1,
    Bottom = 2,
    Left = 3,
    Right = 4,
    TopLeft = 5,
    TopRight = 6,
    BottomLeft = 7,
    BottomRight = 8,
}

impl Align {
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn from_bits(bits: i32) -> Self {
        match bits {
            1 => Self::Top,
            2 => Self::Bottom,
            3 => Self::Left,
            4 => Self::Right,
            5 => Self::TopLeft,
            6 => Self::TopRight,
            7 => Self::BottomLeft,
            8 => Self::BottomRight,
            _ => Self::Center,
        }
    }
}

/// The layout properties of one table cell (`arc Cell`), without the child.
#[derive(Clone, Debug)]
pub struct CellData {
    pub grow_x: bool,
    pub grow_y: bool,
    pub fill_x: bool,
    pub fill_y: bool,
    pub expand_x: bool,
    pub expand_y: bool,
    #[allow(dead_code)]
    // ported surface; not reachable from the native call graph (kept for parity)
    pub uniform: bool,
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub uniform_x: bool,
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub uniform_y: bool,
    /// `-1` = unset (Arc uses negative as "no fixed value").
    pub width: f32,
    pub height: f32,
    pub size: f32,
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub pad_left: f32,
    pub pad_right: f32,
    pub pad_top: f32,
    pub pad_bottom: f32,
    pub colspan: i32,
    pub align: Align,
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub label_align: i32,
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub color: Color,
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub disabled: bool,
}

impl Default for CellData {
    fn default() -> Self {
        Self {
            grow_x: false,
            grow_y: false,
            fill_x: false,
            fill_y: false,
            expand_x: false,
            expand_y: false,
            uniform: false,
            uniform_x: false,
            uniform_y: false,
            width: -1.0,
            height: -1.0,
            size: -1.0,
            min_width: -1.0,
            max_width: -1.0,
            min_height: -1.0,
            max_height: -1.0,
            pad_left: 0.0,
            pad_right: 0.0,
            pad_top: 0.0,
            pad_bottom: 0.0,
            colspan: 1,
            align: Align::Center,
            label_align: 0,
            color: Color::WHITE,
            disabled: false,
        }
    }
}

impl CellData {
    /// Copies every layout property from another cell (`Cell.copyFrom`).
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn copy_from(&mut self, other: &CellData) {
        *self = other.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_gdscript_sentinels() {
        let cell = CellData::default();
        assert_eq!(cell.width, -1.0);
        assert_eq!(cell.colspan, 1);
        assert_eq!(cell.align, Align::Center);
    }

    #[test]
    fn copy_from_replaces_all_fields() {
        let mut target = CellData::default();
        let source = CellData {
            grow_x: true,
            pad_left: 4.0,
            align: Align::TopLeft,
            ..CellData::default()
        };
        target.copy_from(&source);
        assert!(target.grow_x);
        assert_eq!(target.pad_left, 4.0);
        assert_eq!(target.align, Align::TopLeft);
    }

    #[test]
    fn align_bits_round_trip() {
        for bits in 0..=8 {
            let align = Align::from_bits(bits);
            assert_eq!(align as i32, bits);
        }
        assert_eq!(Align::from_bits(99), Align::Center);
    }
}
