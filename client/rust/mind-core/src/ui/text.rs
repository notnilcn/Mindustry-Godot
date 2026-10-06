// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/core/UI.java (`formatIcons`, `formatTime`,
//         `formatAmount`, `roundAmount`, `loadColors`),
//         arc.graphics.Colors / mindustry.graphics.Pal (color tag table),
//         arc.scene.ui.Label / MarkupParser (markup semantics).

//! Godot-free UI text rendering (plan 14 §3.7).
//!
//! This is the pure translation layer between Mindustry bundle text and Godot
//! `RichTextLabel` BBCode. It owns:
//!
//! * `format_icons` — exact `UI.formatIcons` port (`:name:` → `Iconc` char).
//! * `render_markup` — Mindustry `[color]` markup → BBCode `[color=#rrggbbaa]`
//!   plus `[icon name="…"]` tokens (OD-UI3: no runtime font-glyph injection).
//! * `format_time` / `format_amount` / `round_amount` — exact numeric ports.
//!
//! Nothing here reads Godot, the filesystem or the clock; callers feed the
//! `Iconc` table and bundle words.

use crate::assets::icons::Iconc;

/// Bundle words used by [`format_amount`] (`unit.billions`/`millions`/`thousands`).
#[derive(Debug, Clone, Copy)]
pub struct AmountWords<'a> {
    /// Word appended after a billions magnitude (`UI.billions`).
    pub billions: &'a str,
    /// Word appended after a millions magnitude (`UI.millions`).
    pub millions: &'a str,
    /// Word appended after a thousands magnitude (`UI.thousands`).
    pub thousands: &'a str,
}

impl Default for AmountWords<'_> {
    fn default() -> Self {
        Self {
            billions: "B",
            millions: "M",
            thousands: "k",
        }
    }
}

/// A parsed `#rrggbb` / `#rrggbbaa` color tag.
pub fn parse_color_tag(tag: &str) -> Option<u32> {
    let hex = tag.strip_prefix('#')?;
    let value = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        6 => Some((value << 8) | 0xff),
        8 => Some(value),
        _ => None,
    }
}

/// Resolves a named color tag to `0xRRGGBBAA`.
///
/// The table combines Mindustry's `UI.loadColors()` entries with the Arc
/// `Colors` names used in vanilla bundles. Unknown names return `None` so the
/// markup renderer can keep them literal.
pub fn color_tag(name: &str) -> Option<u32> {
    // `UI.loadColors`: accent/unlaunched/highlight/stat/negstat.
    // `highlight = accent.lerp(white, 0.3)`, precomputed.
    let table: &[(&str, u32)] = &[
        ("accent", 0xffd37fff),
        ("unlaunched", 0x8982edff),
        ("highlight", 0xffe0a6ff),
        ("stat", 0xffd37fff),
        ("negstat", 0xe55454ff),
        // Arc `Colors`.
        ("white", 0xffffffff),
        ("lightgray", 0xbfbfbfff),
        ("gray", 0x7f7f7fff),
        ("darkgray", 0x3f3f3fff),
        ("black", 0x000000ff),
        ("pink", 0xffaaccff),
        ("magenta", 0xff00ffff),
        ("purple", 0xaa44ffff),
        ("violet", 0x8844ffff),
        ("blue", 0x5267ffff),
        ("navy", 0x001080ff),
        ("royal", 0x4040ffff),
        ("sky", 0x87ceebff),
        ("cyan", 0x00ffffff),
        ("teal", 0x0088a0ff),
        ("green", 0x00ff00ff),
        ("lime", 0x88ff00ff),
        ("olive", 0x888800ff),
        ("yellow", 0xffff00ff),
        ("gold", 0xffd700ff),
        ("orange", 0xffa500ff),
        ("brown", 0x8b4513ff),
        ("scarlet", 0xff3100ff),
        ("red", 0xff0000ff),
        ("crimson", 0xdc143cff),
        ("maroon", 0x800000ff),
        ("coral", 0xff7f50ff),
        ("salmon", 0xfa8072ff),
        ("peach", 0xffcc99ff),
        ("tan", 0xd2b48cff),
        ("chocolate", 0x5d2f0cff),
        ("none", 0xffffffff),
    ];
    let lower = name.to_ascii_lowercase();
    table
        .iter()
        .find(|(key, _)| *key == lower)
        .map(|(_, value)| *value)
}

/// `UI.formatIcons`: replaces `:name:` tokens with their `Iconc`/font glyph.
///
/// The `changed` flag preserves the original string when no substitution
/// happened (Arc returns `s` itself, matching identity semantics).
pub fn format_icons(s: &str, iconc: &Iconc) -> String {
    if !s.contains(':') {
        return s.to_owned();
    }
    let mut buffer = String::with_capacity(s.len());
    let mut changed = false;
    let mut check_icon = false;
    for token in s.split(':') {
        if check_icon {
            if let Some(ch) = iconc.get(token) {
                buffer.push(ch);
                changed = true;
                check_icon = false;
            } else {
                buffer.push(':');
                buffer.push_str(token);
            }
        } else {
            buffer.push_str(token);
            check_icon = true;
        }
    }
    if changed { buffer } else { s.to_owned() }
}

/// `UI.formatTime`: `0:ss`, `m:ss` or `h:mm:ss` from a tick count.
pub fn format_time(ticks: f32) -> String {
    let seconds = (ticks / 60.0) as i64;
    if seconds < 60 {
        return format!("0:{:02}", seconds);
    }
    let minutes = seconds / 60;
    let mod_sec = seconds % 60;
    if minutes < 60 {
        return format!("{}:{:02}", minutes, mod_sec);
    }
    let hours = minutes / 60;
    let mod_minute = minutes % 60;
    format!("{}:{:02}:{:02}", hours, mod_minute, mod_sec)
}

/// `UI.formatAmount` with explicit bundle words.
pub fn format_amount(number: i64, words: AmountWords<'_>) -> String {
    if number == i64::MAX {
        return "∞".to_owned();
    }
    if number == i64::MIN {
        return "-∞".to_owned();
    }
    let mag = number.unsigned_abs();
    let sign = if number < 0 { "-" } else { "" };
    if mag >= 1_000_000_000 {
        format!(
            "{}{}[gray]{}[]",
            sign,
            fixed(mag as f64 / 1_000_000_000.0, 1),
            words.billions
        )
    } else if mag >= 1_000_000 {
        format!(
            "{}{}[gray]{}[]",
            sign,
            fixed(mag as f64 / 1_000_000.0, 1),
            words.millions
        )
    } else if mag >= 10_000 {
        format!("{}[gray]{}[]", number / 1000, words.thousands)
    } else if mag >= 1000 {
        format!(
            "{}{}[gray]{}[]",
            sign,
            fixed(mag as f64 / 1000.0, 1),
            words.thousands
        )
    } else {
        number.to_string()
    }
}

/// `UI.formatAmount` with the default `B`/`M`/`k` words.
pub fn format_amount_default(number: i64) -> String {
    format_amount(number, AmountWords::default())
}

/// `Strings.fixed(value, decimals)` — fixed-point formatting.
fn fixed(value: f64, decimals: usize) -> String {
    format!("{value:.decimals$}")
}

/// `UI.roundAmount`: rounds to the nearest magnitude step (Arc `Mathf.round`).
pub fn round_amount(number: i32) -> i32 {
    let step = if number >= 1_000_000_000 {
        100_000_000.0
    } else if number >= 1_000_000 {
        100_000.0
    } else if number >= 10_000 {
        1_000.0
    } else if number >= 100 {
        100.0
    } else if number >= 10 {
        10.0
    } else {
        return number;
    };
    ((number as f64 / step).round() * step) as i32
}

/// Renders Mindustry markup into Godot BBCode (plan 14 §3.7).
///
/// Handles:
/// * `:name:` icon tokens → `[icon name="name"]` (PUA codepoints are
///   normalized back to names first so `unit.emoji()` output round-trips).
/// * `[tag]` / `[]` color tags against [`color_tag`] and `#rrggbb(aa)`.
/// * Unknown tags, unknown `:tokens:` and lone `[` stay literal (`[lb]`).
pub fn render_markup(s: &str, iconc: &Iconc) -> String {
    let normalized = normalize_pua(s, iconc);
    let mut out = String::with_capacity(normalized.len() + 16);
    let chars: Vec<char> = normalized.chars().collect();
    // Color stack: index 0 is the implicit default (no color).
    let mut stack: Vec<Option<u32>> = vec![None];
    let mut open = false;

    let set_color = |out: &mut String, open: &mut bool, color: Option<u32>| {
        if *open {
            out.push_str("[/color]");
            *open = false;
        }
        if let Some(c) = color {
            out.push_str(&format!("[color=#{c:08x}]"));
            *open = true;
        }
    };

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '[' {
            if let Some(end) = chars[i + 1..].iter().position(|&ch| ch == ']') {
                let tag: String = chars[i + 1..i + 1 + end].iter().collect();
                if tag.is_empty() {
                    // `[]` closes to the previous color.
                    stack.pop();
                    if stack.is_empty() {
                        stack.push(None);
                    }
                    set_color(&mut out, &mut open, *stack.last().unwrap_or(&None));
                    i += end + 2;
                    continue;
                }
                if tag == "/" {
                    set_color(&mut out, &mut open, None);
                    i += end + 2;
                    continue;
                }
                if let Some(color) = color_tag(&tag).or_else(|| parse_color_tag(&tag)) {
                    stack.push(Some(color));
                    set_color(&mut out, &mut open, Some(color));
                    i += end + 2;
                    continue;
                }
                // Not a known tag: emit literal.
                out.push_str("[lb]");
                i += 1;
                continue;
            }
            out.push_str("[lb]");
            i += 1;
            continue;
        }
        if c == ':' {
            // Try to consume a `:name:` icon token.
            if let Some(rel) = chars[i + 1..].iter().position(|&ch| ch == ':') {
                let name: String = chars[i + 1..i + 1 + rel].iter().collect();
                if !name.is_empty() && iconc.get(&name).is_some() {
                    out.push_str(&format!("[icon name=\"{name}\"]"));
                    i += rel + 2;
                    continue;
                }
            }
            out.push(':');
            i += 1;
            continue;
        }
        if c == '\\' && i + 1 < chars.len() && chars[i + 1] == 'n' {
            out.push('\n');
            i += 2;
            continue;
        }
        out.push(c);
        i += 1;
    }

    if open {
        out.push_str("[/color]");
    }
    out
}

/// Maps `Iconc` PUA codepoints embedded in a string back to `:name:` tokens so
/// [`render_markup`] can emit `[icon name]` for them.
pub fn normalize_pua(s: &str, iconc: &Iconc) -> String {
    if s.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match iconc.name(ch as u32) {
            Some(name) => {
                out.push(':');
                out.push_str(name);
                out.push(':');
            }
            None => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iconc() -> Iconc {
        Iconc::from_properties("63743=spawn|block-spawn-ui\n63742=copper|item-copper-ui\n")
    }

    #[test]
    fn format_icons_matches_java_golden() {
        let table = iconc();
        assert_eq!(format_icons("no icons here", &table), "no icons here");
        assert_eq!(format_icons(":spawn:", &table), "\u{f8ff}");
        assert_eq!(format_icons("use :spawn: now", &table), "use \u{f8ff} now");
        // Unknown token stays verbatim.
        assert_eq!(format_icons("a :nope: b", &table), "a :nope: b");
        // Non-icon colon usage (a URL) survives.
        assert_eq!(format_icons("http://x", &table), "http://x");
    }

    #[test]
    fn markup_color_stack_reset() {
        assert_eq!(
            render_markup("[accent]hello[]", &iconc()),
            "[color=#ffd37fff]hello[/color]"
        );
        assert_eq!(
            render_markup("[red]a[gray]b[]c", &iconc()),
            "[color=#ff0000ff]a[/color][color=#7f7f7fff]b[/color][color=#ff0000ff]c[/color]"
        );
        assert_eq!(render_markup("#ff0000aa", &iconc()), "#ff0000aa");
    }

    #[test]
    fn markup_sky_tag_renders_credits_blurb() {
        // `credits.text` uses `[royal]` and `[sky]`; arc registers `SKY` from
        // `Color.sky` (0x87ceebff) (`arc.graphics.Colors.reset`).
        assert_eq!(
            render_markup("[royal]Anuken[] - [sky]anukendev@gmail.com[]", &iconc()),
            "[color=#4040ffff]Anuken[/color] - [color=#87ceebff]anukendev@gmail.com[/color]"
        );
    }

    #[test]
    fn pua_icon_normalization() {
        let table = iconc();
        let pua = table.unicode_str("spawn").unwrap();
        assert_eq!(normalize_pua(&pua, &table), ":spawn:");
        assert_eq!(render_markup(&pua, &table), "[icon name=\"spawn\"]");
        assert_eq!(
            render_markup(":copper: x", &table),
            "[icon name=\"copper\"] x"
        );
    }

    #[test]
    fn unknown_tokens_verbatim() {
        assert_eq!(render_markup("[bogus]x", &iconc()), "[lb]bogus]x");
        assert_eq!(render_markup(":bogus:", &iconc()), ":bogus:");
        assert_eq!(render_markup("[", &iconc()), "[lb]");
    }

    #[test]
    fn color_tag_unknown_is_literal() {
        assert_eq!(color_tag("definitely-not-a-color"), None);
        assert_eq!(parse_color_tag("#zz0000"), None);
    }

    #[test]
    fn format_time_zero() {
        assert_eq!(format_time(0.0), "0:00");
        assert_eq!(format_time(59.0), "0:00");
        assert_eq!(format_time(60.0), "0:01");
        assert_eq!(format_time(60.0 * 59.0), "0:59");
        assert_eq!(format_time(60.0 * 60.0), "1:00");
        assert_eq!(format_time(60.0 * 3661.0), "1:01:01");
    }

    #[test]
    fn format_time_golden() {
        assert_eq!(format_time(60.0 * 61.0), "1:01");
        assert_eq!(format_time(-1.0), "0:00");
    }

    #[test]
    fn format_amount_golden() {
        let words = AmountWords {
            billions: "B",
            millions: "M",
            thousands: "k",
        };
        assert_eq!(format_amount(0, words), "0");
        assert_eq!(format_amount(999, words), "999");
        assert_eq!(format_amount(1000, words), "1.0[gray]k[]");
        assert_eq!(format_amount(12_345, words), "12[gray]k[]");
        assert_eq!(format_amount(1_500_000, words), "1.5[gray]M[]");
        assert_eq!(format_amount(2_000_000_000, words), "2.0[gray]B[]");
    }

    #[test]
    fn amount_infinities_for_long_extremes() {
        assert_eq!(format_amount_default(i64::MAX), "∞");
        assert_eq!(format_amount_default(i64::MIN), "-∞");
        assert_eq!(
            format_amount(-1500, AmountWords::default()),
            "-1.5[gray]k[]"
        );
    }

    #[test]
    fn round_amount_golden() {
        assert_eq!(round_amount(0), 0);
        assert_eq!(round_amount(9), 9);
        assert_eq!(round_amount(12), 10);
        assert_eq!(round_amount(150), 200);
        assert_eq!(round_amount(1234), 1200);
        assert_eq!(round_amount(12_345), 12_000);
        assert_eq!(round_amount(1_234_567), 1_200_000);
        assert_eq!(round_amount(1_234_567_890), 1_200_000_000);
    }
}
