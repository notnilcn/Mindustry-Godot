// SPDX-License-Identifier: GPL-3.0-only

//! Plan 23 M1 — joint ownership of the canonical sim checksum.
//!
//! `parity/checksum_registry.json` mirrors `CHECKSUM_VERSION` and the contributor
//! order (plan 05 §6.5/§6.4). This module owns the cross-registry invariants:
//! the registry matches the compiled constant, every committed golden is recorded
//! at the same canonical width, and the contributor order is stable.

use std::path::Path;

use anyhow::Result;

use super::golden::GoldenManifest;
use super::registry::{CANONICAL_ALGORITHM, ChecksumRegistry};
use super::scenario::ScenarioCatalog;

/// Whether the committed registry mirrors the compiled checksum constant and
/// algorithm (plan 23 §6.4).
pub fn registry_matches_core(registry: &ChecksumRegistry) -> bool {
    registry.checksum_version == mind_core::constants::CHECKSUM_VERSION
        && registry.algorithm == CANONICAL_ALGORITHM
}

/// Whether every committed golden is recorded as a canonical 16-hex checksum
/// (the FNV-1a-64 width at [`mind_core::constants::CHECKSUM_VERSION`]).
///
/// The hex value cannot encode the version, so this also requires the manifest's
/// `upstream.commit` to be a single 40-hex value shared by every golden.
pub fn golden_versions_equal(repo: &Path) -> Result<bool> {
    let catalog = ScenarioCatalog::load(&repo.join("parity/scenario_catalog.json"))?;
    for entry in &catalog.entries {
        if let Some(checksum) = &entry.expect_checksum
            && !is_canonical_checksum(checksum)
        {
            return Ok(false);
        }
    }

    let manifest = GoldenManifest::load(&repo.join("parity/golden_manifest.json"))?;
    if manifest.upstream.commit.len() != 40
        || !manifest
            .upstream
            .commit
            .chars()
            .all(|c| c.is_ascii_hexdigit())
    {
        return Ok(false);
    }
    for result in manifest.verify(repo) {
        // Only checksum-typed harness goldens are checked; JSON goldens are
        // sha256-pinned independently by the manifest.
        if !result.path.ends_with(".checksums") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(repo.join(&result.path)) else {
            return Ok(false);
        };
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // Some checkpoint files are `<tick> <checksum>`; validate the value.
            let Some(checksum) = line.split_whitespace().last() else {
                continue;
            };
            if !is_canonical_checksum(checksum) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// A canonical checksum is exactly 16 lowercase hex digits (FNV-1a-64).
fn is_canonical_checksum(value: &str) -> bool {
    value.len() == 16
        && value
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> std::path::PathBuf {
        crate::paths::find_repo_root(None).expect("repo root")
    }

    #[test]
    fn registry_matches_core() {
        let registry = ChecksumRegistry::load(&repo().join("parity/checksum_registry.json"))
            .expect("registry");
        assert!(
            super::registry_matches_core(&registry),
            "registry must mirror CHECKSUM_VERSION={} / {CANONICAL_ALGORITHM}",
            mind_core::constants::CHECKSUM_VERSION
        );
    }

    #[test]
    fn golden_versions_equal() {
        assert!(
            super::golden_versions_equal(&repo()).expect("golden versions"),
            "every committed golden must use the canonical 16-hex checksum"
        );
    }

    #[test]
    fn contributor_order_stable() {
        let registry = ChecksumRegistry::load(&repo().join("parity/checksum_registry.json"))
            .expect("registry");
        let active = registry.active_ids();
        assert_eq!(active.first().copied(), Some("game_state"));
        // The content hash is a header field and is listed last (plan §6.4).
        assert_eq!(
            registry.contributors.last().map(|c| c.id.as_str()),
            Some("content_hash")
        );
    }
}
