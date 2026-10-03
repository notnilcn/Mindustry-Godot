// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Banned block/unit editor model (plan 19 M6; `BannedContentDialog.java`).
//!
//! Upstream `BannedContentDialog<T>` hosts an `ObjectSet<T>` directly from
//! `Rules.bannedBlocks`/`bannedUnits` (plan 12). The port exposes the two
//! `IndexSet<String>`s and the dialog operations (add/remove/add-all/clear,
//! search + category filter) as data; GDScript renders the two panes.

use indexmap::IndexSet;

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
}
