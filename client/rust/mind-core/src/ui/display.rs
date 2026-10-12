// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Displayable.java,
//         core/src/mindustry/world/meta/StatValues.java (display rows),
//         core/src/mindustry/ui/Bar.java (`BarModel` bindings).

//! `Displayable` equivalent (plan 14 §3.7/M4).
//!
//! Godot-free view model for hover info / content stats: a [`HoverInfo`] is an
//! ordered list of [`DisplayRow`]s produced by provider traits over the plan
//! 06/07/11 read models. `PlacementFragment`, `ContentInfoDialog` and tooltips
//! render these rows; keeping the model here lets `cargo test -p mind-core`
//! assert formatting without Godot.

use crate::assets::bundle::Bundle;
use crate::assets::icons::Iconc;
use crate::ui::text::{format_amount, format_icons};

/// A single row of a hover-info / stats panel (`Displayable` row kinds).
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayRow {
    /// Plain/colored text (already markup-formatted by the producer).
    Text(String),
    /// Icon + label (`StatValues.number` icon column).
    IconText {
        /// Atlas region name for the icon.
        icon: String,
        /// Text shown next to the icon.
        text: String,
    },
    /// Progress bar row (`BarModel`-backed).
    Bar {
        /// Centered bar label.
        label: String,
        /// RGBA color.
        color: [f32; 4],
        /// Fill fraction in `0..=1`.
        fraction: f32,
    },
    /// Item list with per-item amounts (`StatValues.items`).
    Items(Vec<(String, i32)>),
    /// Status-effect list with per-effect durations in ticks.
    Status(Vec<(String, f32)>),
}

/// The full hover info for one displayed object.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HoverInfo {
    /// Ordered rows.
    pub rows: Vec<DisplayRow>,
}

impl HoverInfo {
    /// Splits the rows into `(left_label, right_value)` strings for the
    /// tooltip table, resolving item/status rows via the plan-03 bundle/iconc.
    pub fn to_text_rows(&self, bundle: &Bundle, iconc: &Iconc) -> Vec<(String, String)> {
        self.rows
            .iter()
            .map(|row| row.to_text(bundle, iconc))
            .collect()
    }
}

impl DisplayRow {
    /// Renders this row as a `(left, right)` string pair (`Displayable` layout).
    pub fn to_text(&self, _bundle: &Bundle, iconc: &Iconc) -> (String, String) {
        match self {
            DisplayRow::Text(text) => (format_icons(text, iconc), String::new()),
            DisplayRow::IconText { icon, text } => (
                format!(":{}: {}", icon, format_icons(text, iconc)),
                String::new(),
            ),
            DisplayRow::Bar {
                label,
                fraction,
                color: _,
            } => (
                label.clone(),
                format!("{}%", (fraction.clamp(0.0, 1.0) * 100.0).round() as i32),
            ),
            DisplayRow::Items(items) => {
                let mut left = String::new();
                let mut right = String::new();
                for (index, (item, amount)) in items.iter().enumerate() {
                    if index > 0 {
                        left.push(' ');
                        right.push(' ');
                    }
                    left.push_str(&format!(":{}:", item));
                    right.push_str(&format_amount(*amount as i64, Default::default()));
                }
                (left, right)
            }
            DisplayRow::Status(statuses) => {
                let mut left = String::new();
                let mut right = String::new();
                for (index, (status, duration)) in statuses.iter().enumerate() {
                    if index > 0 {
                        left.push(' ');
                        right.push(' ');
                    }
                    left.push_str(&format!(":{}:", status));
                    right.push_str(&format!("{:.0}", duration));
                }
                (left, right)
            }
        }
    }
}

/// A provider of hover info (`Displayable` in upstream). Implemented over the
/// 06/07/11 read models; `mind-gdext` collects rows and hands them to GDScript.
pub trait Displayable {
    /// Builds the hover rows for this object.
    fn hover_info(&self) -> HoverInfo;
}

/// `PlacementFragment` requirement-row amount colour: `[scarlet]` below half
/// the need, `[accent]` below the need, else `[white]`
/// (`amount < stackamount / 2f ? "[scarlet]" : amount < stackamount ? "[accent]" : "[white]"`).
pub fn requirement_color(have: i32, need: i32) -> &'static str {
    if (have as f32) < need as f32 / 2.0 {
        "scarlet"
    } else if have < need {
        "accent"
    } else {
        "white"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::bundle::{Bundle, parse_properties};

    fn bundle() -> Bundle {
        Bundle::from_layers(vec![parse_properties(
            "unit.billions=B\nunit.millions=M\nunit.thousands=k\n",
        )])
    }

    fn iconc() -> Iconc {
        Iconc::from_properties("63742=copper|item-copper-ui\n63743=lead|item-lead-ui\n")
    }

    #[test]
    fn text_rows_format_icons() {
        // Unknown tokens stay literal; known `:name:` tokens become the glyph.
        let info = HoverInfo {
            rows: vec![DisplayRow::Text(String::from(":nope: wall"))],
        };
        let rows = info.to_text_rows(&bundle(), &iconc());
        assert_eq!(rows[0].0, ":nope: wall");

        let info = HoverInfo {
            rows: vec![DisplayRow::Text(String::from(":copper: wall"))],
        };
        let rows = info.to_text_rows(&bundle(), &iconc());
        assert_eq!(rows[0].0, "\u{f8fe} wall");
    }

    #[test]
    fn requirement_colour_boundaries() {
        // `have < need / 2f` is a float compare: 1 < 3/2 = 1.5 -> scarlet.
        assert_eq!(requirement_color(0, 1), "scarlet");
        assert_eq!(requirement_color(0, 3), "scarlet");
        assert_eq!(requirement_color(1, 3), "scarlet");
        assert_eq!(requirement_color(2, 3), "accent");
        assert_eq!(requirement_color(2, 2), "white");
        assert_eq!(requirement_color(5, 2), "white");
        // Zero requirement is always satisfied.
        assert_eq!(requirement_color(0, 0), "white");
    }

    #[test]
    fn item_rows_pair_icon_and_amount() {
        let info = HoverInfo {
            rows: vec![DisplayRow::Items(vec![
                (String::from("copper"), 1500),
                (String::from("lead"), 12),
            ])],
        };
        let rows = info.to_text_rows(&bundle(), &iconc());
        assert_eq!(rows[0].0, ":copper: :lead:");
        assert_eq!(rows[0].1, "1.5[gray]k[] 12");
    }
}
