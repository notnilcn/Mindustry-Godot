// SPDX-License-Identifier: GPL-3.0-only

//! `parity/checksum_registry.json` (format 1): joint ownership of the canonical
//! sim checksum. `checksum_version` must mirror `mind_core::constants::CHECKSUM_VERSION`.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// The FNV-1a-64 canonical algorithm name.
pub const CANONICAL_ALGORITHM: &str = "fnv1a64";

/// One checksum contributor.
#[derive(Debug, Clone, Deserialize)]
pub struct Contributor {
    /// Stable component id.
    pub id: String,
    /// Owning plan short number (e.g. `05`).
    pub owner: String,
    /// `active | planned | header`.
    pub status: String,
    /// Optional note.
    #[serde(default)]
    pub note: Option<String>,
}

/// An excluded (view-only) component.
#[derive(Debug, Clone, Deserialize)]
pub struct Excluded {
    /// Component id.
    pub id: String,
    /// Owning plan short number.
    pub owner: String,
    /// Why it is excluded.
    pub reason: String,
}

/// The parsed registry.
#[derive(Debug, Clone, Deserialize)]
pub struct ChecksumRegistry {
    /// File format version.
    pub format: u32,
    /// Mirror of `CHECKSUM_VERSION`.
    pub checksum_version: u32,
    /// Canonical algorithm.
    pub algorithm: String,
    /// Owning plan file.
    pub owner: String,
    /// Active/planned/header contributors.
    pub contributors: Vec<Contributor>,
    /// Reviewed order in which the **active** contributors fold into the
    /// canonical stream. Must be a permutation of the active contributor ids.
    #[serde(default)]
    pub fold_order: Vec<String>,
    /// View-only exclusions.
    #[serde(default)]
    pub excluded: Vec<Excluded>,
}

impl ChecksumRegistry {
    /// Loads the registry from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing `{}`", path.display()))
    }

    /// Validates the registry against the crate constant and repo layout.
    pub fn check(&self, repo: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            problems.push(format!("checksum registry: format {} != 1", self.format));
        }
        if self.checksum_version != mind_core::constants::CHECKSUM_VERSION {
            problems.push(format!(
                "checksum registry: checksum_version {} != mind_core::CHECKSUM_VERSION {}",
                self.checksum_version,
                mind_core::constants::CHECKSUM_VERSION
            ));
        }
        if self.algorithm != CANONICAL_ALGORITHM {
            problems.push(format!(
                "checksum registry: algorithm `{}` != `{CANONICAL_ALGORITHM}`",
                self.algorithm
            ));
        }
        if !repo.join(&self.owner).is_file() {
            problems.push(format!(
                "checksum registry: owner plan `{}` does not exist",
                self.owner
            ));
        }
        let mut seen = BTreeSet::new();
        for contributor in &self.contributors {
            if !seen.insert(contributor.id.as_str()) {
                problems.push(format!(
                    "checksum registry: duplicate contributor `{}`",
                    contributor.id
                ));
            }
            if !matches!(contributor.status.as_str(), "active" | "planned" | "header") {
                problems.push(format!(
                    "checksum registry: contributor `{}` has unknown status `{}`",
                    contributor.id, contributor.status
                ));
            }
        }
        // Reviewed fold order: must be exactly the active contributor set.
        let active: BTreeSet<&str> = self
            .contributors
            .iter()
            .filter(|c| c.status == "active")
            .map(|c| c.id.as_str())
            .collect();
        let mut ordered = BTreeSet::new();
        for id in &self.fold_order {
            if !ordered.insert(id.as_str()) {
                problems.push(format!("checksum registry: duplicate fold_order id `{id}`"));
            }
            if !active.contains(id.as_str()) {
                problems.push(format!(
                    "checksum registry: fold_order id `{id}` is not an active contributor"
                ));
            }
        }
        for id in &active {
            if !ordered.contains(id) {
                problems.push(format!(
                    "checksum registry: active contributor `{id}` is missing from fold_order"
                ));
            }
        }
        problems
    }

    /// Convenience accessor for the active contributor ids.
    pub fn active_ids(&self) -> Vec<&str> {
        self.contributors
            .iter()
            .filter(|c| c.status == "active")
            .map(|c| c.id.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_path() -> std::path::PathBuf {
        crate::paths::find_repo_root(None)
            .expect("repo root")
            .join("parity/checksum_registry.json")
    }

    #[test]
    fn committed_registry_matches_core_constant() {
        let registry = ChecksumRegistry::load(&registry_path()).expect("registry");
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        assert!(
            registry.check(&repo).is_empty(),
            "registry problems: {:?}",
            registry.check(&repo)
        );
        assert!(registry.active_ids().contains(&"game_state"));
    }

    #[test]
    fn version_mismatch_is_flagged() {
        let mut registry = ChecksumRegistry::load(&registry_path()).expect("registry");
        registry.checksum_version = registry.checksum_version.wrapping_add(1);
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        assert!(
            registry
                .check(&repo)
                .iter()
                .any(|p| p.contains("checksum_version"))
        );
    }

    #[test]
    fn fold_order_must_match_active_contributors() {
        let registry = ChecksumRegistry::load(&registry_path()).expect("registry");
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        assert!(
            registry.check(&repo).is_empty(),
            "committed fold_order: {:?}",
            registry.check(&repo)
        );

        let mut missing = registry.clone();
        missing.fold_order.retain(|id| id != "groups");
        assert!(
            missing
                .check(&repo)
                .iter()
                .any(|p| p.contains("missing from fold_order")),
            "a dropped active contributor must be flagged"
        );

        let mut unknown = registry;
        unknown.fold_order.push("rules".to_owned());
        assert!(
            unknown
                .check(&repo)
                .iter()
                .any(|p| p.contains("not an active contributor")),
            "a non-active fold_order id must be flagged"
        );
    }
}
