// SPDX-License-Identifier: GPL-3.0-only

//! Pure validation helpers for the identity reducers (plan 01 §3.9). Cheap,
//! deterministic checks only (D2): no content lookups, no cross-table work.
//! `#[cfg(test)]` tests typecheck with
//! `cargo check --manifest-path server/spacetimedb/Cargo.toml --tests` (a
//! cdylib cannot be linked in place — same constraint as `main/`).

/// Mirror of `Vars.maxNameLength` (`core/src/mindustry/Vars.java:113`).
pub const MAX_USERNAME_LEN: usize = 40;

/// Profile-name cap (same budget as usernames; plan 12 may tighten).
pub const MAX_PROFILE_NAME_LEN: usize = 40;

/// Language tag cap (`en`, `pt-BR`, ...; BCP-47 is short).
pub const MAX_LANGUAGE_LEN: usize = 32;

/// Cap on the opaque keybinds JSON blob (defensive; plan 14 owns the schema).
pub const MAX_KEYBINDS_JSON_LEN: usize = 16 * 1024;

/// UI scale bounds mirroring Godot's sane content-scale range.
pub const MIN_UI_SCALE: f32 = 0.5;
pub const MAX_UI_SCALE: f32 = 4.0;

/// Validates a username: 1..=[`MAX_USERNAME_LEN`] non-whitespace characters.
pub fn validate_username(username: &str) -> Result<(), String> {
    let len = username.chars().count();
    if username.trim().is_empty() || len > MAX_USERNAME_LEN {
        return Err(format!(
            "username must be 1-{MAX_USERNAME_LEN} non-whitespace characters"
        ));
    }
    Ok(())
}

/// Java `Colors`-aware name sanitizer (`NetServer.fixName`,
/// `NetServer.java:976-999`, v146).
///
/// Trims, removes tabs/newlines, strips transparent (`alpha < 1`) colour tags
/// and truncates at [`MAX_USERNAME_LEN`] UTF-8 bytes. GAPS #33/PAR-0545: the
/// host sanitizes every join name, so a SpacetimeDB module (which owns the
/// authoritative `player.username` every peer reads) must apply the same rule.
pub fn fix_name(name: &str) -> String {
    let mut name: Vec<char> = name
        .trim()
        .chars()
        .filter(|c| *c != '\n' && *c != '\t')
        .collect();
    if name.len() == 1 && (name[0] == '[' || name[0] == ']') {
        return String::new();
    }
    let mut index = 0;
    while index < name.len() {
        if name[index] == '['
            && index + 1 < name.len()
            && name[index + 1] != '['
            && (index == 0 || name[index - 1] != '[')
        {
            let tail: String = name[index..].iter().collect();
            let prefix: String = name[..index].iter().collect();
            let checked = check_color(&tail);
            name = format!("{prefix}{checked}").chars().collect();
        }
        index += 1;
    }
    let mut out = String::new();
    for c in name {
        if out.len() >= MAX_USERNAME_LEN {
            break;
        }
        out.push(c);
    }
    out
}

/// `NetServer.checkColor` (`NetServer.java:1000-1029`): if `str` starts a
/// colour tag whose colour is transparent (`alpha < 1`), return the text after
/// the tag; otherwise return `str` unchanged.
pub fn check_color(str: &str) -> &str {
    let bytes = str.as_bytes();
    let mut index = 1;
    while index < bytes.len() {
        if bytes[index] == b']' {
            let color = &str[1..index];
            if let Some(alpha) = color_alpha(color) {
                if alpha < 255 {
                    return &str[index + 1..];
                }
            } else {
                // `Color.valueOf` threw for a non-hex name: keep the tag.
                return str;
            }
        }
        index += 1;
    }
    str
}

/// Alpha of a colour tag body (`Colors.get(...)` names or `Color.valueOf` hex).
///
/// `None` means `Color.valueOf` would throw (unknown non-hex token). Every
/// named Arc colour except `clear` is opaque, so only the transparent cases
/// (`clear`, 8-digit hex with alpha < 0xff) change the returned name.
fn color_alpha(color: &str) -> Option<u8> {
    if color.eq_ignore_ascii_case("clear") {
        return Some(0);
    }
    let hex = color.strip_prefix('#').unwrap_or(color);
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    if hex.len() == 8 {
        u8::from_str_radix(&hex[6..8], 16).ok()
    } else {
        Some(255)
    }
}

/// Validates a profile name (same rules as usernames).
pub fn validate_profile_name(name: &str) -> Result<(), String> {
    let len = name.chars().count();
    if name.trim().is_empty() || len > MAX_PROFILE_NAME_LEN {
        return Err(format!(
            "profile name must be 1-{MAX_PROFILE_NAME_LEN} non-whitespace characters"
        ));
    }
    Ok(())
}

/// Validates a language tag: non-empty ASCII alphanumerics/`-`/`_`.
pub fn validate_language(language: &str) -> Result<(), String> {
    if language.is_empty() || language.len() > MAX_LANGUAGE_LEN {
        return Err(format!("language must be 1-{MAX_LANGUAGE_LEN} characters"));
    }
    if !language
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("language must be ASCII alphanumerics, '-' or '_'".to_string());
    }
    Ok(())
}

/// Validates UI scale: finite and within [`MIN_UI_SCALE`]..=[`MAX_UI_SCALE`].
pub fn validate_ui_scale(scale: f32) -> Result<(), String> {
    if !scale.is_finite() || !(MIN_UI_SCALE..=MAX_UI_SCALE).contains(&scale) {
        return Err(format!(
            "ui_scale must be finite and between {MIN_UI_SCALE} and {MAX_UI_SCALE}"
        ));
    }
    Ok(())
}

/// Validates a volume: finite and within `0.0..=1.0`.
pub fn validate_volume(volume: f32) -> Result<(), String> {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err("volume must be finite and between 0.0 and 1.0".to_string());
    }
    Ok(())
}

/// Validates the opaque keybinds blob: size cap only (plan 14 owns the schema).
pub fn validate_keybinds_json(json: &str) -> Result<(), String> {
    if json.len() > MAX_KEYBINDS_JSON_LEN {
        return Err(format!(
            "keybinds_json exceeds {MAX_KEYBINDS_JSON_LEN} bytes"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_rules() {
        assert!(validate_username("Builder").is_ok());
        assert!(validate_username("").is_err());
        assert!(validate_username("   ").is_err());
        assert!(validate_username(&"a".repeat(MAX_USERNAME_LEN)).is_ok());
        assert!(validate_username(&"a".repeat(MAX_USERNAME_LEN + 1)).is_err());
    }

    #[test]
    fn profile_name_rules() {
        assert!(validate_profile_name("Co-op run").is_ok());
        assert!(validate_profile_name("\t").is_err());
        assert!(validate_profile_name(&"p".repeat(MAX_PROFILE_NAME_LEN + 1)).is_err());
    }

    #[test]
    fn language_and_volume_rules() {
        assert!(validate_language("pt-BR").is_ok());
        assert!(validate_language("en_US").is_ok());
        assert!(validate_language("").is_err());
        assert!(validate_language("bad tag!").is_err());
        assert!(validate_volume(0.0).is_ok());
        assert!(validate_volume(1.0).is_ok());
        assert!(validate_volume(1.5).is_err());
        assert!(validate_volume(f32::NAN).is_err());
        assert!(validate_ui_scale(1.0).is_ok());
        assert!(validate_ui_scale(0.1).is_err());
        assert!(validate_ui_scale(f32::INFINITY).is_err());
    }

    #[test]
    fn fix_name_and_check_color_follow_net_server() {
        assert_eq!(fix_name("  Bob  "), "Bob");
        assert_eq!(fix_name("A\nB\tC"), "ABC");
        assert_eq!(fix_name("["), "");
        assert_eq!(fix_name("]"), "");
        assert_eq!(fix_name("A[clear]B"), "AB");
        assert_eq!(fix_name("A[#00000000]B"), "AB");
        assert_eq!(fix_name("A[CLEAR]B"), "AB");
        assert_eq!(fix_name("[red]Bob"), "[red]Bob");
        assert_eq!(fix_name("[#ff0000]Bob"), "[#ff0000]Bob");
        assert_eq!(fix_name("[[red]Bob"), "[[red]Bob");
        assert_eq!(check_color("[clear]tail"), "tail");
        assert_eq!(check_color("[#000000ff]tail"), "[#000000ff]tail");
        assert_eq!(check_color("[notacolor]tail"), "[notacolor]tail");
        // Java truncates at `maxNameLength` UTF-8 bytes (Vars.java:113).
        assert_eq!(
            fix_name(&"a".repeat(MAX_USERNAME_LEN + 5)).len(),
            MAX_USERNAME_LEN
        );
        assert_eq!(fix_name("[clear]"), "");
        assert!(validate_username(&fix_name("[clear]")).is_err());
    }

    #[test]
    fn keybinds_size_cap() {
        assert!(validate_keybinds_json("{}").is_ok());
        assert!(validate_keybinds_json(&"x".repeat(MAX_KEYBINDS_JSON_LEN + 1)).is_err());
    }
}
