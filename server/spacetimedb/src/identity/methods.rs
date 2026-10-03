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
    fn keybinds_size_cap() {
        assert!(validate_keybinds_json("{}").is_ok());
        assert!(validate_keybinds_json(&"x".repeat(MAX_KEYBINDS_JSON_LEN + 1)).is_err());
    }
}
