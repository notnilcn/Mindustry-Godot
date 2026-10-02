// SPDX-License-Identifier: GPL-3.0-only

//! `parity/golden_manifest.json` (format 1): committed oracle + scenario goldens
//! with sha256 drift detection. Harness goldens under `client/rust/*/tests/golden/**`
//! are verified by the scenarios themselves and are not listed here.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Upstream provenance for the oracle goldens.
#[derive(Debug, Clone, Deserialize)]
pub struct Upstream {
    /// Upstream repository URL.
    pub repo: String,
    /// 40-hex upstream commit.
    pub commit: String,
}

/// One committed golden artifact.
#[derive(Debug, Clone, Deserialize)]
pub struct Golden {
    /// Stable id.
    pub id: String,
    /// `oracle` (JVM-derived) or `scenario` (Rust self-recorded).
    pub kind: String,
    /// Repo-relative path.
    pub path: String,
    /// Owning plan short number.
    pub owner_plan: String,
    /// Expected sha256 hex.
    pub sha256: String,
    /// Optional regeneration command.
    #[serde(default)]
    pub regen: Option<String>,
}

/// The parsed manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct GoldenManifest {
    /// File format version.
    pub format: u32,
    /// Upstream provenance.
    pub upstream: Upstream,
    /// Golden entries.
    pub goldens: Vec<Golden>,
}

/// Per-golden verification result.
#[derive(Debug, Clone)]
pub struct GoldenResult {
    /// Golden id.
    pub id: String,
    /// Repo-relative path.
    pub path: String,
    /// Expected hash.
    pub expected: String,
    /// Actual hash (empty if the file is missing).
    pub actual: String,
    /// Whether the file exists.
    pub exists: bool,
    /// Whether the file exists and the hash matches.
    pub pass: bool,
}

impl GoldenManifest {
    /// Loads the manifest.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing `{}`", path.display()))
    }

    /// Verifies every golden's sha256.
    pub fn verify(&self, repo: &Path) -> Vec<GoldenResult> {
        self.goldens
            .iter()
            .map(|golden| {
                let path = repo.join(&golden.path);
                let actual = std::fs::read(&path)
                    .map(|bytes| hex(&Sha256::digest(&bytes)))
                    .unwrap_or_default();
                let exists = path.is_file();
                GoldenResult {
                    id: golden.id.clone(),
                    path: golden.path.clone(),
                    expected: golden.sha256.clone(),
                    pass: exists && actual == golden.sha256,
                    actual,
                    exists,
                }
            })
            .collect()
    }

    /// Structural + drift problems (empty means green).
    pub fn check(&self, repo: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            problems.push(format!("golden manifest: format {} != 1", self.format));
        }
        if self.upstream.commit.len() != 40
            || !self.upstream.commit.chars().all(|c| c.is_ascii_hexdigit())
        {
            problems.push(format!(
                "golden manifest: upstream.commit `{}` is not 40-hex",
                self.upstream.commit
            ));
        }
        for result in self.verify(repo) {
            if !result.exists {
                problems.push(format!(
                    "golden `{}`: {} is missing",
                    result.id, result.path
                ));
            } else if !result.pass {
                problems.push(format!(
                    "golden `{}`: sha256 mismatch for {} (expected {}, got {})",
                    result.id, result.path, result.expected, result.actual
                ));
            }
        }
        problems
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_manifest_hashes_match() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let manifest =
            GoldenManifest::load(&repo.join("parity/golden_manifest.json")).expect("manifest");
        let problems = manifest.check(&repo);
        assert!(problems.is_empty(), "golden problems: {problems:?}");
    }

    #[test]
    fn missing_file_is_flagged() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let mut manifest =
            GoldenManifest::load(&repo.join("parity/golden_manifest.json")).expect("manifest");
        manifest.goldens[0].path = String::from("parity/does-not-exist.json");
        assert!(
            manifest
                .check(&repo)
                .iter()
                .any(|p| p.contains("is missing"))
        );
    }
}
