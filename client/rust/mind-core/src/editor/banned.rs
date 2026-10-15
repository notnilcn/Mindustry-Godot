// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Banned block/unit editor model (plan 19 M6; `BannedContentDialog.java`).
//!
//! Upstream `BannedContentDialog<T>` hosts an `ObjectSet<T>` directly from
//! `Rules.bannedBlocks`/`bannedUnits` (plan 12). The port exposes the two
//! `IndexSet<String>`s and the dialog operations (add/remove/add-all/clear,
//! search + category filter) as data; GDScript renders the two panes.

use indexmap::IndexSet;

use crate::io::IoError;
use crate::io::IoResult;
use crate::io::json::JsonIo;
use crate::io::json::rules::Rules;

/// Which `Rules` set is being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BanKind {
    /// `Rules.banned_blocks`.
    Block,
    /// `Rules.banned_units`.
    Unit,
}

impl BanKind {
    /// The runtime bundle keys for the pane titles.
    pub fn title_keys(self) -> (&'static str, &'static str) {
        match self {
            BanKind::Block => ("@bannedblocks", "@unbannedblocks"),
            BanKind::Unit => ("@bannedunits", "@unbannedunits"),
        }
    }
}

/// Immutable access to the banned set for `kind`.
pub fn ban_set(rules: &Rules, kind: BanKind) -> &IndexSet<String> {
    match kind {
        BanKind::Block => &rules.banned_blocks,
        BanKind::Unit => &rules.banned_units,
    }
}

/// Mutable access to the banned set for `kind`.
pub fn ban_set_mut(rules: &mut Rules, kind: BanKind) -> &mut IndexSet<String> {
    match kind {
        BanKind::Block => &mut rules.banned_blocks,
        BanKind::Unit => &mut rules.banned_units,
    }
}

/// Adds a name; returns whether the set changed.
pub fn add(set: &mut IndexSet<String>, name: &str) -> bool {
    set.insert(name.to_owned())
}

/// Removes a name; returns whether the set changed.
pub fn remove(set: &mut IndexSet<String>, name: &str) -> bool {
    set.shift_remove(name)
}

/// Adds/removes based on `banned`.
pub fn set_banned(set: &mut IndexSet<String>, name: &str, banned: bool) -> bool {
    if banned {
        add(set, name)
    } else {
        remove(set, name)
    }
}

/// Adds every name (`@addall`); returns the number of newly added names.
pub fn add_all(set: &mut IndexSet<String>, names: &[String]) -> usize {
    names.iter().filter(|name| add(set, name)).count()
}

/// Removes every name; returns the number removed.
pub fn remove_all(set: &mut IndexSet<String>, names: &[String]) -> usize {
    names.iter().filter(|name| remove(set, name)).count()
}

/// Replaces the whole set with `selected` (dialog apply).
pub fn apply_selection(set: &mut IndexSet<String>, selected: &[String]) {
    set.clear();
    for name in selected {
        set.insert(name.clone());
    }
}

/// Filters `all` by pane membership + search (`BannedContentDialog.rebuildTable`).
///
/// `selected == true` returns the banned pane, `false` the available pane.
pub fn filter_pane(
    all: &[String],
    banned: &IndexSet<String>,
    search: &str,
    selected: bool,
) -> Vec<String> {
    let search = search.trim().to_lowercase();
    let mut out: Vec<String> = all
        .iter()
        .filter(|name| banned.contains(*name) == selected)
        .filter(|name| search.is_empty() || name.to_lowercase().contains(&search))
        .cloned()
        .collect();
    out.sort();
    out
}

/// Serializes `Rules` to the map `rules` tag JSON after a banned edit.
pub fn rules_json(rules: &Rules) -> IoResult<String> {
    JsonIo::write(rules)
}

/// The `Rules` JSON key backing `kind` (`Rules.bannedBlocks`/`bannedUnits`).
pub fn tag_key(kind: BanKind) -> &'static str {
    match kind {
        BanKind::Block => "bannedBlocks",
        BanKind::Unit => "bannedUnits",
    }
}

/// Reads the banned names from a raw `rules` tag JSON (`editor.tags["rules"]`).
///
/// Missing, non-object or non-array values yield an empty list (`BannedContentDialog`
/// over a fresh `Rules`). Names keep their stored order (`ObjectSet` iteration).
pub fn names_from_tag_json(tag: &str, kind: BanKind) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(tag) else {
        return Vec::new();
    };
    value
        .get(tag_key(kind))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Replaces the banned set inside a raw `rules` tag JSON, preserving every
/// other key (`BannedContentDialog` mutates the live `state.rules` set; the
/// port keeps the map `rules` tag as the source of truth, `MapEditorDialog.save`).
///
/// An empty tag starts a fresh object (`MapInfoDialog` on a map with no rules).
pub fn apply_to_tag_json(tag: &str, kind: BanKind, names: &[String]) -> IoResult<String> {
    let trimmed = tag.trim();
    let mut value: serde_json::Value = if trimmed.is_empty() {
        serde_json::Value::Object(serde_json::Map::new())
    } else {
        serde_json::from_str(trimmed)?
    };
    let Some(object) = value.as_object_mut() else {
        return Err(IoError::corrupt("rules tag is not a JSON object"));
    };
    let entries = serde_json::Value::Array(
        names
            .iter()
            .map(|name| serde_json::Value::String(name.clone()))
            .collect(),
    );
    object.insert(tag_key(kind).to_owned(), entries);
    Ok(serde_json::to_string(&value)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banned_set_mutates_rules_json() {
        let mut rules = Rules::default();
        {
            let blocks = ban_set_mut(&mut rules, BanKind::Block);
            assert!(add(blocks, "conveyor"));
            assert!(add(blocks, "router"));
            assert!(!add(blocks, "router"), "idempotent");
        }
        {
            let units = ban_set_mut(&mut rules, BanKind::Unit);
            add_all(units, &["dagger".to_owned(), "mace".to_owned()]);
            remove(units, "mace");
        }
        let json = rules_json(&rules).unwrap();
        let parsed: Rules = JsonIo::read(&json).unwrap();
        assert!(parsed.banned_blocks.contains("conveyor"));
        assert!(parsed.banned_blocks.contains("router"));
        assert!(parsed.banned_units.contains("dagger"));
        assert!(!parsed.banned_units.contains("mace"));
    }

    #[test]
    fn pane_filter_and_selection() {
        let all = vec!["alpha".to_owned(), "beta".to_owned(), "gamma".to_owned()];
        let mut banned = IndexSet::new();
        add(&mut banned, "beta");
        assert_eq!(filter_pane(&all, &banned, "", true), vec!["beta"]);
        assert_eq!(
            filter_pane(&all, &banned, "a", false),
            vec!["alpha", "gamma"]
        );
        apply_selection(&mut banned, &["gamma".to_owned()]);
        assert_eq!(banned.len(), 1);
        assert!(banned.contains("gamma"));
    }

    #[test]
    fn tag_json_round_trip_preserves_other_keys() {
        // A map rules tag with unrelated keys + GDScript-style floats.
        let tag = r#"{"waves":true,"winWave":20.0,"mode":"sandbox","bannedBlocks":["router"]}"#;
        assert_eq!(
            names_from_tag_json(tag, BanKind::Block),
            vec!["router".to_owned()]
        );
        assert!(names_from_tag_json(tag, BanKind::Unit).is_empty());

        let updated = apply_to_tag_json(
            tag,
            BanKind::Unit,
            &["dagger".to_owned(), "mace".to_owned()],
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(parsed["bannedUnits"][1], "mace");
        assert_eq!(parsed["bannedBlocks"][0], "router", "other set untouched");
        assert_eq!(parsed["winWave"], 20.0, "unknown/other keys preserved");
        assert_eq!(parsed["mode"], "sandbox");

        // An empty tag starts a fresh object; a non-object tag is rejected.
        let fresh = apply_to_tag_json("", BanKind::Block, &["conveyor".to_owned()]).unwrap();
        let fresh_parsed: serde_json::Value = serde_json::from_str(&fresh).unwrap();
        assert_eq!(fresh_parsed["bannedBlocks"][0], "conveyor");
        assert!(apply_to_tag_json("[]", BanKind::Block, &[]).is_err());
        assert_eq!(
            names_from_tag_json("not json", BanKind::Block),
            Vec::<String>::new()
        );
        assert_eq!(names_from_tag_json("", BanKind::Unit), Vec::<String>::new());
    }
}
