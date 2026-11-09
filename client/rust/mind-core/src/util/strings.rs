// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Strings.stripColors` / `Strings.sanitizeFilename` ports (`arc.util.Strings`).
//!
//! Needed by `maps/Map.compareTo`/`findFile` (plan 06 §3.1) and the dialog
//! search filters. [`strip_colors`] mirrors `Strings.parseColorMarkup`
//! exactly, including the `Colors` name table Mindustry extends in
//! `UI.loadColors()`.

/// Removes Arc color markup from a string (`Strings.stripColors`).
///
/// Mirrors `Strings.parseColorMarkup`: `[]` pops a color, `[[` escapes a
/// literal `[`, `[#hex]` accepts 1..=8 hex digits, and `[name]` strips only
/// when `Colors.get(name)` is non-null — Arc's canonical names, the lowercased
/// underscore-free aliases `Colors.load` registers, and Mindustry's
/// `UI.loadColors()` additions. Unknown or malformed tags stay literal.
pub fn strip_colors(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'[' {
            let length = parse_color_markup(bytes, i + 1, bytes.len());
            if length >= 0 {
                i += length as usize + 2;
            } else {
                out.push('[');
                i += 1;
            }
        } else {
            // Preserve UTF-8: push the char starting at this byte.
            let ch = input[i..].chars().next().unwrap_or('[');
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// Arc `Strings.parseColorMarkup`: `>= 0` is a tag length, `-1` invalid,
/// `-2` escaped `[[`.
#[allow(clippy::needless_range_loop)] // indexes address byte offsets, not slices
fn parse_color_markup(str: &[u8], start: usize, end: usize) -> i32 {
    if start >= end {
        return -1;
    }
    match str[start] {
        b'#' => {
            for i in start + 1..end {
                match str[i] {
                    b']' => {
                        // `if(i < start + 2 || i > start + 9) break;` — 1..=8
                        // hex digits, the alpha pair optional.
                        let digits = i - start - 1;
                        if !(1..=8).contains(&digits) {
                            return -1;
                        }
                        return (i - start) as i32;
                    }
                    b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => {}
                    _ => return -1,
                }
            }
            -1
        }
        b'[' => -2,
        b']' => 0,
        _ => {
            for i in start + 1..end {
                if str[i] == b']' {
                    // `Color namedColor = Colors.get(...); null => not a tag.`
                    let name = std::str::from_utf8(&str[start..i]).unwrap_or("");
                    return if is_color_name(name) {
                        (i - start) as i32
                    } else {
                        -1
                    };
                }
            }
            -1
        }
    }
}

/// Arc `Colors` canonical names (`arc-core/src/arc/graphics/Colors.java:55-99`).
const ARC_COLOR_NAMES: &[&str] = &[
    "CLEAR",
    "BLACK",
    "WHITE",
    "LIGHT_GRAY",
    "GRAY",
    "DARK_GRAY",
    "LIGHT_GREY",
    "GREY",
    "DARK_GREY",
    "BLUE",
    "NAVY",
    "ROYAL",
    "SLATE",
    "SKY",
    "CYAN",
    "TEAL",
    "GREEN",
    "ACID",
    "LIME",
    "FOREST",
    "OLIVE",
    "YELLOW",
    "GOLD",
    "GOLDENROD",
    "ORANGE",
    "BROWN",
    "TAN",
    "BRICK",
    "RED",
    "SCARLET",
    "CRIMSON",
    "CORAL",
    "SALMON",
    "PINK",
    "MAGENTA",
    "PURPLE",
    "VIOLET",
    "MAROON",
];

/// The lowercase aliases `Colors.load` registers with underscores removed
/// (`Colors.java:103`: `key.toLowerCase().replace("_", "")`).
const ARC_COLOR_NAMES_LOWER: &[&str] = &[
    "clear",
    "black",
    "white",
    "lightgray",
    "gray",
    "darkgray",
    "lightgrey",
    "grey",
    "darkgrey",
    "blue",
    "navy",
    "royal",
    "slate",
    "sky",
    "cyan",
    "teal",
    "green",
    "acid",
    "lime",
    "forest",
    "olive",
    "yellow",
    "gold",
    "goldenrod",
    "orange",
    "brown",
    "tan",
    "brick",
    "red",
    "scarlet",
    "crimson",
    "coral",
    "salmon",
    "pink",
    "magenta",
    "purple",
    "violet",
    "maroon",
];

/// Mindustry's `UI.loadColors()` additions
/// (`core/src/mindustry/core/UI.java:102-106`).
const MIND_COLOR_NAMES: &[&str] = &["accent", "unlaunched", "highlight", "stat", "negstat"];

/// `Colors.get(name) != null`: a named tag is only a color when the exact
/// string is registered.
fn is_color_name(name: &str) -> bool {
    ARC_COLOR_NAMES.contains(&name)
        || ARC_COLOR_NAMES_LOWER.contains(&name)
        || MIND_COLOR_NAMES.contains(&name)
}

/// Sanitizes a filename (`Strings.sanitizeFilename`): reserved DOS names are
/// prefixed with `_` and the unsafe characters `\0 / " < > | : * ? \` become `_`.
pub fn sanitize_filename(input: &str) -> String {
    if input == "." {
        return "_".to_owned();
    }
    if input == ".." {
        return "__".to_owned();
    }
    let mut value = if is_reserved_filename(input) {
        format!("_{input}")
    } else {
        input.to_owned()
    };
    value = value
        .chars()
        .map(|c| {
            if matches!(
                c,
                '\0' | '/' | '"' | '<' | '>' | '|' | ':' | '*' | '?' | '\\'
            ) {
                '_'
            } else {
                c
            }
        })
        .collect();
    value
}

/// Arc `reservedFilenamePattern`: `(CON|AUX|PRN|NUL|COM[0-9]|LPT[0-9])(\..*|$)`.
fn is_reserved_filename(name: &str) -> bool {
    let (stem, _) = match name.split_once('.') {
        Some((stem, ext)) => (stem, Some(ext)),
        None => (name, None),
    };
    let upper = stem.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "AUX" | "PRN" | "NUL")
        || (upper.starts_with("COM") && upper.len() == 4 && upper.as_bytes()[3].is_ascii_digit())
        || (upper.starts_with("LPT") && upper.len() == 4 && upper.as_bytes()[3].is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_colors_matches_arc_parse_color_markup() {
        // `[]` pop, the canonical and lowercase aliases, `[[]` escape.
        assert_eq!(strip_colors("[red]hello[] world"), "hello world");
        assert_eq!(strip_colors("[accent]A[lightgray]B[LIGHT_GRAY]C"), "ABC");
        assert_eq!(strip_colors("[clear]x"), "x");
        assert_eq!(strip_colors("[[]"), "[");
        assert_eq!(strip_colors("[[]x"), "[x");
        assert_eq!(strip_colors("plain"), "plain");
        // 1..=8 hex digits; the alpha pair is optional.
        assert_eq!(strip_colors("[#ff0000]x"), "x");
        assert_eq!(strip_colors("[#f]x"), "x");
        assert_eq!(strip_colors("[#ff0000ff]x"), "x");
        assert_eq!(strip_colors("[#]x"), "[#]x");
        assert_eq!(strip_colors("[#gg]x"), "[#gg]x");
        assert_eq!(strip_colors("[#ff0000000]x"), "[#ff0000000]x");
        // Only registered names strip: `[NotATag]`/`[tags]` stay literal, and
        // the lowercase aliases have their underscores removed.
        assert_eq!(strip_colors("a[NotATag]b"), "a[NotATag]b");
        assert_eq!(strip_colors("a[tags]b"), "a[tags]b");
        assert_eq!(strip_colors("a[light_gray]b"), "a[light_gray]b");
        // Unterminated tag is left as-is.
        assert_eq!(strip_colors("a[red"), "a[red");
    }

    #[test]
    fn sanitize_filename_ports_arc() {
        assert_eq!(sanitize_filename("normal_name"), "normal_name");
        assert_eq!(sanitize_filename("a/b:c*d?e"), "a_b_c_d_e");
        assert_eq!(sanitize_filename("."), "_");
        assert_eq!(sanitize_filename(".."), "__");
        assert_eq!(sanitize_filename("con.msch"), "_con.msch");
        assert_eq!(sanitize_filename("NUL"), "_NUL");
        assert_eq!(sanitize_filename("com7"), "_com7");
        assert_eq!(sanitize_filename("company"), "company");
    }
}
