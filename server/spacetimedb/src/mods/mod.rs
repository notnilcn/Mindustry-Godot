// SPDX-License-Identifier: GPL-3.0-only

//! Mod-compatibility handshake tables/helpers (plan 21 §3.12.4/§6.1).
//!
//! Plan 20 (not on disk at authoring) owns mod discovery/manifest generation;
//! this module freezes the STDB shape and the set-comparison semantics derived
//! from upstream `NetServer` (`Missing mods:` / `Unnecessary mods:`).

use spacetimedb::{table};

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

#[cfg(test)]
mod tests {
    use super::check_mods;

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
}
