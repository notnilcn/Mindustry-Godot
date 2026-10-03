// SPDX-License-Identifier: GPL-3.0-only

//! Mod-compatibility handshake tables/helpers (plan 21 §3.12.4/§6.1).
//!
//! Plan 20 (not on disk at authoring) owns mod discovery/manifest generation;
//! this module freezes the STDB shape and the set-comparison semantics derived
//! from upstream `NetServer` (`Missing mods:` / `Unnecessary mods:`).

use spacetimedb::table;

/// One mod the host requires for a match (plan §6.1); vanilla is implicit.
#[table(accessor = match_mod, public, index(accessor = by_match_mod, btree(columns = [match_id, name])))]
pub struct MatchMod {
    #[primary_key]
    #[auto_inc]
    pub match_mod_id: u64,
    #[index(btree)]
    pub match_id: u64,
    pub name: String,
    pub version: String,
    pub content_hash: u64,
}

/// Vanilla content manifest row (plan §3.12.4/§6.1), seeded from the build.
///
/// `content_type` mirrors plan 02's content kind discriminator; `source_mod`
/// is `""` for vanilla (mods do not replicate per-mod catalogs under D2).
#[table(accessor = content_catalog, public)]
pub struct ContentCatalog {
    #[primary_key]
    #[auto_inc]
    pub content_id: u64,
    #[unique]
    pub name: String,
    pub content_type: u8,
    pub source_mod: String,
}

/// Compares the host's required mod set against a joiner's loaded set.
///
/// Both inputs are mod names (order-independent). Mirrors upstream
/// `NetServer` handshake semantics: missing and extra mods are both rejected
/// with a stable, human-readable reason.
pub fn check_mods(host_mods: &[String], client_mods: &[String]) -> Result<(), String> {
    let missing: Vec<&str> = host_mods
        .iter()
        .filter(|name| !client_mods.contains(name))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        return Err(format!("Missing mods: {}", missing.join(", ")));
    }
    let extra: Vec<&str> = client_mods
        .iter()
        .filter(|name| !host_mods.contains(name))
        .map(String::as_str)
        .collect();
    if !extra.is_empty() {
        return Err(format!("Unnecessary mods: {}", extra.join(", ")));
    }
    Ok(())
}

/// Whether a content name may be referenced by a command (plan §3.12.4).
///
/// Vanilla matches use the seeded `content_catalog`; modded matches skip the
/// existence check because per-mod catalogs are not replicated under D2 (the
/// name still passes charset/length validation).
pub fn content_name_allowed(catalog: &[String], has_mods: bool, name: &str) -> bool {
    has_mods || catalog.iter().any(|entry| entry == name)
}

#[cfg(test)]
mod tests {
    use super::{check_mods, content_name_allowed};

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn identical_sets_pass() {
        assert!(check_mods(&names(&["a", "b"]), &names(&["b", "a"])).is_ok());
        assert!(check_mods(&[], &[]).is_ok());
    }

    #[test]
    fn missing_and_extra_are_rejected() {
        let error = check_mods(&names(&["a", "b"]), &names(&["a"])).unwrap_err();
        assert_eq!(error, "Missing mods: b");
        let error = check_mods(&names(&["a"]), &names(&["a", "c"])).unwrap_err();
        assert_eq!(error, "Unnecessary mods: c");
    }

    #[test]
    fn content_catalog_gates_vanilla_only() {
        let catalog = names(&["router", "stone-wall"]);
        assert!(content_name_allowed(&catalog, false, "router"));
        assert!(!content_name_allowed(&catalog, false, "modded-block"));
        // Modded matches skip existence checks (names still shape-validated).
        assert!(content_name_allowed(&catalog, true, "modded-block"));
    }
}
