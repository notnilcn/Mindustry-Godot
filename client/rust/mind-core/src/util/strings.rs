// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Strings.stripColors` / `Strings.sanitizeFilename` ports (`arc.util.Strings`).
//!
//! Needed by `maps/Map.compareTo`/`findFile` (plan 06 §3.1). Deviation: named
//! color tags are accepted structurally (letters/digits/`#` hex) rather than via
//! Arc's full `Colors` table, which is a view concern.

/// Removes Arc color markup from a string (`Strings.stripColors`).
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
                        let digits = i - start - 1;
                        if !(2..=8).contains(&digits) {
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
                    // Structurally valid named color tag.
                    let ok = str[start..i]
                        .iter()
                        .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_');
                    return if ok { (i - start) as i32 } else { -1 };
                }
            }
            -1
        }
    }
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
    fn strip_colors_removes_tags() {
        assert_eq!(strip_colors("[red]hello[] world"), "hello world");
        assert_eq!(strip_colors("plain"), "plain");
        assert_eq!(strip_colors("[#ff0000]x"), "x");
        assert_eq!(strip_colors("a[unknown-not-color]b"), "ab");
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
