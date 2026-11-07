// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/PlayerListFragment.java (row names),
//         core/src/mindustry/entities/comp/PlayerComp.java (`coloredName`),
//         core/src/mindustry/ui/fragments/ChatFragment.java (`addMessage` text).

//! Roster name/team resolution shared by the chat overlay and the event toasts.
//!
//! The relay rows carry identities (64 hex chars), not display names:
//! `mind-stdb`'s `Roster` resolves them from `relay_member`/`player` and
//! `MindUi.player_list_json()` exposes the same aggregation the player list
//! renders. This module turns that JSON into the small lookup the chat fragment
//! (`ChatFormatter.java`-style `[coral][[name][coral]]:[white]`) and the event
//! toasts need — no game state, no net calls (pure, unit-tested).
//!
//! Upstream `Player.coloredName()` uses the player's chosen `color`; the relay
//! schema carries no colour column (roster deviation D2), so the sender's
//! *team* colour stands in (`Team.coloredName`), falling back to
//! `Pal.accent` when the roster does not know the identity.

use std::collections::HashMap;

use mind_core::game::team::Team;
use serde_json::Value;

/// Characters of an identity hex used when the roster has no name for it
/// (`Player.plainName` fallback the player list already uses).
const SHORT_IDENTITY: usize = 8;

/// One roster row resolved for display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RosterName {
    /// Identity hex (64 lowercase hex chars).
    pub identity: String,
    /// Last-known username (`""` when unresolved).
    pub name: String,
    /// Assigned team index, when known.
    pub team: Option<u8>,
    /// Whether the identity holds an admin record.
    pub admin: bool,
    /// Whether the member is currently connected.
    pub connected: bool,
}

/// Short identity form (`PlayerListFragment`'s 8-char prefix).
pub fn short_identity(identity: &str) -> String {
    identity.chars().take(SHORT_IDENTITY).collect()
}

/// Parses the `MindUi.player_list_json()` / `MindNet.players_json()` rows.
///
/// Rows without an identity are skipped (they are not addressable); a missing
/// `name` stays empty so [`display_name`] falls back to the identity prefix.
pub fn roster_names(json: &str) -> Vec<RosterName> {
    let Ok(Value::Array(rows)) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let identity = row.get("identity").and_then(Value::as_str)?;
            if identity.is_empty() {
                return None;
            }
            Some(RosterName {
                identity: identity.to_owned(),
                name: row
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                team: row
                    .get("team")
                    .and_then(Value::as_u64)
                    .and_then(|team| u8::try_from(team).ok()),
                admin: row.get("admin").and_then(Value::as_bool).unwrap_or(false),
                connected: row
                    .get("connected")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect()
}

/// Identity-keyed index (`identity` stays the lookup key; names are not unique).
pub fn roster_index(rows: &[RosterName]) -> HashMap<String, RosterName> {
    rows.iter()
        .map(|row| (row.identity.to_ascii_lowercase(), row.clone()))
        .collect()
}

/// One roster row by identity hex (case-insensitive).
pub fn find<'a>(index: &'a HashMap<String, RosterName>, identity: &str) -> Option<&'a RosterName> {
    index.get(&identity.to_ascii_lowercase())
}

/// Display name for an identity (`coloredName`'s text half).
///
/// Prefers the roster username, else the 8-char identity prefix — the same
/// fallback the console and player list use, so an unresolved row stays
/// addressable instead of rendering empty.
pub fn display_name(identity: &str, index: &HashMap<String, RosterName>) -> String {
    match find(index, identity) {
        Some(row) if !row.name.is_empty() => row.name.clone(),
        _ => short_identity(identity),
    }
}

/// `[#rrggbb]` colour for an identity's team name (`Team.coloredName`).
///
/// `Pal.accent` (`ffd37f`) when the roster does not know the identity: the
/// default player team's colour, so an unresolved sender still reads as a
/// player rather than a system line.
pub fn name_color(identity: &str, index: &HashMap<String, RosterName>) -> String {
    let team = find(index, identity).and_then(|row| row.team).unwrap_or(1);
    format!("#{}", Team::get(team).hex())
}

/// One chat author prefix in Java markup (`"<coloredName>"`).
pub fn colored_name(identity: &str, index: &HashMap<String, RosterName>) -> String {
    format!(
        "[{}]{}",
        name_color(identity, index),
        display_name(identity, index)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: &str = r#"[
        {"identity":"aa11","name":"Alice","team":1,"admin":true,"connected":true},
        {"identity":"bb22","name":"","team":null,"admin":false,"connected":false},
        {"identity":"cc33","name":"Carol","team":5,"admin":false,"connected":true}
    ]"#;

    #[test]
    fn parses_roster_rows_and_skips_the_identityless() {
        let rows = roster_names(ROWS);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "Alice");
        assert!(rows[0].admin);
        assert_eq!(rows[0].team, Some(1));
        assert_eq!(rows[1].name, "");
        assert_eq!(rows[1].team, None);
        assert!(roster_names("[]").is_empty());
        assert!(roster_names("{}").is_empty());
        assert!(roster_names("not json").is_empty());
    }

    #[test]
    fn display_name_prefers_the_username_and_falls_back_to_the_prefix() {
        let index = roster_index(&roster_names(ROWS));
        assert_eq!(display_name("aa11", &index), "Alice");
        // An unresolved username keeps the row addressable.
        assert_eq!(display_name("bb22", &index), "bb22");
        // An identity the roster does not know at all.
        assert_eq!(
            display_name("deadbeef0123456789", &index),
            "deadbeef",
            "8-char identity prefix"
        );
    }

    #[test]
    fn identity_lookup_is_case_insensitive() {
        let index = roster_index(&roster_names(ROWS));
        assert_eq!(display_name("AA11", &index), "Alice");
        assert!(find(&index, "Aa11").is_some());
    }

    #[test]
    fn name_colour_uses_the_team_colour_then_pal_accent() {
        let index = roster_index(&roster_names(ROWS));
        // Team 1 (sharded) is Pal.accent `ffd37f`; team 5 (blue) `6c87fd`.
        assert_eq!(name_color("aa11", &index), "#ffd37f");
        assert_eq!(name_color("cc33", &index), "#6c87fd");
        // Unknown identities read as the default player team.
        assert_eq!(name_color("ffff", &index), "#ffd37f");
        assert_eq!(colored_name("aa11", &index), "[#ffd37f]Alice");
    }

    #[test]
    fn short_identity_takes_eight_chars() {
        assert_eq!(short_identity("0123456789"), "01234567");
        assert_eq!(short_identity("ab"), "ab");
        assert_eq!(short_identity(""), "");
    }
}
