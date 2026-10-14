// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/world/meta/StatValues.java (display kinds),
//         core/src/mindustry/world/meta/Stat.java / StatCat.java (labels),
//         arc Strings.autoFixed.

//! Content-stat formatting (plan 14 §3.7/M4).
//!
//! The formatting half of `world/meta/StatValues.java`: `fixValue`, the
//! `stat`/`negstat` colorized ammo/multiplier helpers, and a [`StatDisplay`]
//! view model rendered into [`DisplayRow`]s for `ContentInfoDialog` and block
//! info. Godot-free so `cargo test -p mind-core` covers it.

use crate::assets::bundle::Bundle;
use crate::assets::icons::Iconc;
use crate::ui::display::{DisplayRow, HoverInfo};
use crate::ui::text::format_icons;

/// `StatValues.fixValue` — `Strings.autoFixed(value, 3)`.
pub fn fix_value(value: f32) -> String {
    auto_fixed(value, 3)
}

/// Arc `Strings.autoFixed` (`Strings.java:761-773`): truncates the extra
/// digits past `max` (adds `0.001` before the floor), picks the smallest
/// precision with an exact representation, then formats through [`fixed`].
pub fn auto_fixed(value: f32, max: usize) -> String {
    // `Mathf.pow(10, max)` is `(float)Math.pow(10, max)`.
    let pow10 = 10f64.powi(max as i32) as f32;
    // `(float)Mathf.floor(value * pow10 + 0.001f) / pow10` (f32 division).
    let truncated = floor_int(value * pow10 + 0.001) as f32 / pow10;

    let precision = if (floor_int(truncated) as f32 - truncated).abs() < 0.0001 {
        0
    } else if (floor_int(truncated * 10.0) as f32 - truncated * 10.0).abs() < 0.0001 {
        1
    } else if (floor_int(truncated * 100.0) as f32 - truncated * 100.0).abs() < 0.0001 {
        2
    } else if (floor_int(truncated * 1000.0) as f32 - truncated * 1000.0).abs() < 0.0001 {
        3
    } else {
        4
    };
    fixed(truncated, max.min(precision))
}

/// Arc `Strings.fixed` (`Strings.java:775-777` + `fixedBuilder`): fixed-point
/// formatting with `places` digits (`(int)(float)(|d| * 10^places + 0.0001f)`
/// digits, dot inserted from the right, leading zeroes padded).
pub fn fixed(d: f32, places: usize) -> String {
    let negative = d < 0.0;
    let abs = d.abs();
    let scaled = (abs as f64 * 10f64.powi(places as i32) + 0.0001f32 as f64) as f32 as i32;
    let mut dec = scaled.to_string();
    let len = dec.len() as i32;
    let decimal_position = len - places as i32;
    let mut result = String::new();
    if negative {
        result.push('-');
    }
    if places == 0 {
        if negative {
            dec.insert(0, '-');
        }
        return dec;
    } else if decimal_position > 0 {
        let (head, tail) = dec.split_at(decimal_position as usize);
        result.push_str(head);
        result.push('.');
        result.push_str(tail);
    } else {
        result.push_str("0.");
        let mut position = decimal_position;
        while position < 0 {
            result.push('0');
            position += 1;
        }
        result.push_str(&dec);
    }
    result
}

/// Java `Mathf.floor(float)` returns `int` (`(int)Math.floor`).
fn floor_int(value: f32) -> i32 {
    (value as f64).floor() as i32
}

/// `StatValues.ammoStat`: `[stat]+`/`[negstat]` prefix + 1 decimal.
pub fn ammo_stat(value: f32) -> String {
    format!(
        "{}{}",
        if value > 0.0 { "[stat]+" } else { "[negstat]" },
        auto_fixed(value, 1)
    )
}

/// `StatValues.multStat`: `[stat]` when `>= 1`, else `[negstat]`; 2 decimals.
pub fn mult_stat(value: f32) -> String {
    format!(
        "{}{}",
        if value >= 1.0 { "[stat]" } else { "[negstat]" },
        auto_fixed(value, 2)
    )
}

/// The stat color tags (`Pal.stat`/`Pal.negstat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatColor {
    /// `[stat]` (accent-yellow).
    Stat,
    /// `[negstat]` (red).
    Negstat,
    /// `[accent]`.
    Accent,
    /// `[lightgray]`.
    Lightgray,
}

impl StatColor {
    /// The markup tag for this color.
    pub fn tag(self) -> &'static str {
        match self {
            StatColor::Stat => "stat",
            StatColor::Negstat => "negstat",
            StatColor::Accent => "accent",
            StatColor::Lightgray => "lightgray",
        }
    }
}

/// A stat unit (`StatUnit`) — label key + optional icon + spacing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatUnit {
    /// `StatUnit.power` (`@unit.power`).
    Power,
    /// `StatUnit.powerSecond` (`@unit.powersec`).
    PowerSecond,
    /// `StatUnit.tiles` (`@unit.tiles`).
    Tiles,
    /// `StatUnit.percent` (`@unit.percent`).
    Percent,
    /// `StatUnit.multiplier` (`@unit.multiplier`).
    Multiplier,
    /// `StatUnit.seconds` (`@unit.seconds`).
    Seconds,
    /// No unit.
    None,
}

impl StatUnit {
    /// Bundle key for the unit's localized label.
    pub fn label_key(self) -> &'static str {
        match self {
            StatUnit::Power => "unit.power",
            StatUnit::PowerSecond => "unit.powersec",
            StatUnit::Tiles => "unit.tiles",
            StatUnit::Percent => "unit.percent",
            StatUnit::Multiplier => "unit.multiplier",
            StatUnit::Seconds => "unit.seconds",
            StatUnit::None => "",
        }
    }

    /// Optional unit icon region (`StatUnit.icon`).
    pub fn icon(self) -> Option<&'static str> {
        match self {
            StatUnit::Power => Some("power"),
            StatUnit::PowerSecond => Some("power"),
            _ => None,
        }
    }
}

/// A displayable stat value (`StatValues.*` display kinds used by block info).
#[derive(Debug, Clone, PartialEq)]
pub enum StatDisplay {
    /// `StatValues.string`.
    Text(String),
    /// `StatValues.number`.
    Number { value: f32, unit: StatUnit },
    /// `StatValues.multiplierModifier`.
    Multiplier { value: f32, unit: StatUnit },
    /// `StatValues.percentModifier` (value is a multiplier, shown as `(v-1)*100%`).
    Percent { value: f32, unit: StatUnit },
    /// `StatValues.squared`.
    Squared { value: f32, unit: StatUnit },
    /// Item amounts (`StatValues.items`).
    Items { items: Vec<(String, i32)> },
    /// Status-effect durations (`StatValues.status`).
    Status { statuses: Vec<(String, f32)> },
    /// A progress bar row (`StatValues` bar / `BarModel`).
    Bar {
        label: String,
        color: StatColor,
        fraction: f32,
    },
}

/// Renders a [`StatDisplay`] into hover rows (`StatValue.display`).
pub fn display_rows(stat: &StatDisplay, bundle: &Bundle, iconc: &Iconc) -> HoverInfo {
    let row = match stat {
        StatDisplay::Text(text) => DisplayRow::Text(format_icons(text, iconc)),
        StatDisplay::Number { value, unit } => unit_row(fix_value(*value), *unit, bundle),
        StatDisplay::Multiplier { value, unit } => unit_row(mult_stat(*value), *unit, bundle),
        StatDisplay::Percent { value, unit } => {
            unit_row(ammo_stat((*value - 1.0) * 100.0), *unit, bundle)
        }
        StatDisplay::Squared { value, unit } => {
            let fixed = fix_value(*value);
            unit_row(format!("{fixed}x{fixed}"), *unit, bundle)
        }
        StatDisplay::Items { items } => DisplayRow::Items(items.clone()),
        StatDisplay::Status { statuses } => DisplayRow::Status(statuses.clone()),
        StatDisplay::Bar {
            label,
            color,
            fraction,
        } => {
            let _ = color;
            DisplayRow::Bar {
                label: label.clone(),
                color: [1.0, 0.827, 0.498, 1.0],
                fraction: *fraction,
            }
        }
    };
    HoverInfo { rows: vec![row] }
}

fn unit_row(value: String, unit: StatUnit, bundle: &Bundle) -> DisplayRow {
    let label = if unit == StatUnit::None {
        value
    } else {
        let localized = bundle.get(unit.label_key());
        let separator = if value.is_empty() { "" } else { " " };
        format!("{value}{separator}{localized}")
    };
    match unit.icon() {
        Some(icon) => DisplayRow::IconText {
            icon: icon.to_owned(),
            text: label,
        },
        None => DisplayRow::Text(label),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::bundle::{Bundle, parse_properties};

    fn bundle() -> Bundle {
        Bundle::from_layers(vec![parse_properties(
            "unit.power=Power\nunit.powersec=Power/sec\nunit.tiles=Tiles\nunit.percent=Percent\nunit.multiplier=Multiplier\nunit.seconds=Seconds\n",
        )])
    }

    fn iconc() -> Iconc {
        Iconc::from_properties("63742=copper|item-copper-ui\n")
    }

    #[test]
    fn fix_value_golden() {
        assert_eq!(fix_value(0.0), "0");
        assert_eq!(fix_value(3.0), "3");
        assert_eq!(fix_value(3.5), "3.5");
        // H05 T9: Arc truncates; 1.2345 -> 1.234 (was "1.235" with rounding).
        assert_eq!(fix_value(1.2345), "1.234");
        assert_eq!(auto_fixed(1.20, 3), "1.2");
        assert_eq!(auto_fixed(1000.0, 2), "1000");
    }

    /// Oracle run against the Arc jar (2026-10-14, H05 T9).
    #[test]
    fn auto_fixed_matches_arc() {
        assert_eq!(auto_fixed(0.291666, 3), "0.291");
        assert_eq!(auto_fixed(0.6666667, 3), "0.666");
        assert_eq!(auto_fixed(1.3333334, 3), "1.333");
        assert_eq!(auto_fixed(1.5, 3), "1.5");
        assert_eq!(auto_fixed(2.0, 3), "2");
        assert_eq!(auto_fixed(0.4, 3), "0.4");
        assert_eq!(auto_fixed(-1.25, 1), "-1.3");
        assert_eq!(auto_fixed(60.0 / 7.0, 2), "8.57");
    }

    #[test]
    fn stat_rows_textual() {
        let info = display_rows(
            &StatDisplay::Text(String::from(":nope: holds")),
            &bundle(),
            &iconc(),
        );
        assert_eq!(
            info.rows,
            vec![DisplayRow::Text(String::from(":nope: holds"))]
        );
    }

    #[test]
    fn percent_and_rate_units() {
        assert_eq!(ammo_stat(10.0), "[stat]+10");
        assert_eq!(ammo_stat(0.0), "[negstat]0");
        assert_eq!(mult_stat(1.0), "[stat]1");
        assert_eq!(mult_stat(0.5), "[negstat]0.5");

        let info = display_rows(
            &StatDisplay::Percent {
                value: 1.1,
                unit: StatUnit::Percent,
            },
            &bundle(),
            &iconc(),
        );
        assert_eq!(
            info.rows,
            vec![DisplayRow::Text(String::from("[stat]+10 Percent"))]
        );

        let rate = display_rows(
            &StatDisplay::Number {
                value: 2.5,
                unit: StatUnit::PowerSecond,
            },
            &bundle(),
            &iconc(),
        );
        assert_eq!(
            rate.rows,
            vec![DisplayRow::IconText {
                icon: String::from("power"),
                text: String::from("2.5 Power/sec"),
            }]
        );
    }

    #[test]
    fn stat_negative_color() {
        assert!(ammo_stat(-5.0).starts_with("[negstat]"));
        assert!(mult_stat(0.25).starts_with("[negstat]"));
        assert_eq!(StatColor::Negstat.tag(), "negstat");
        assert_eq!(StatColor::Stat.tag(), "stat");
    }
}
