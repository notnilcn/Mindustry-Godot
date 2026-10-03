// SPDX-License-Identifier: GPL-3.0-only

//! `parity/screenshots/manifest.json` (format 1): fixed-pose screenshot
//! baselines (plan 23 §3.5/§6.6, M6).
//!
//! In-engine capture needs the Godot editor and is gated on the single-editor
//! mutex. This module scaffolds the directory manifest and the sha256 diff
//! logic: entries are `deferred` until a capture is committed, at which point a
//! `captured` entry must exist and match its recorded hash.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// One expected baseline capture.
#[derive(Debug, Clone, Deserialize)]
pub struct Baseline {
    /// Stable capture id.
    pub id: String,
    /// Owning plan short number.
    pub plan: String,
    /// Backing headless scenario (if any).
    #[serde(default)]
    pub scenario: Option<String>,
    /// Fixed camera pose (`tx,ty,zoom`).
    #[serde(default)]
    pub pose: Option<String>,
    /// Repo-relative PNG path under `parity/screenshots/`.
    pub path: String,
    /// `deferred | captured`.
    pub status: String,
    /// Recorded sha256 (required when `status = "captured"`).
    #[serde(default)]
    pub sha256: Option<String>,
}

/// The parsed baseline manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct ScreenshotManifest {
    /// File format version.
    pub format: u32,
    /// Baselines.
    pub baselines: Vec<Baseline>,
}

/// Per-baseline verification result.
#[derive(Debug, Clone)]
pub struct BaselineResult {
    /// Capture id.
    pub id: String,
    /// Whether the PNG exists.
    pub exists: bool,
    /// Whether the hash matches (or `None` when deferred).
    pub sha256_match: Option<bool>,
    /// Whether the entry is satisfied (deferred, or captured + matching).
    pub pass: bool,
}

impl ScreenshotManifest {
    /// Loads the manifest.
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading `{}`", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing `{}`", path.display()))
    }

    /// Verifies every baseline against the repo.
    pub fn verify(&self, repo: &Path) -> Vec<BaselineResult> {
        self.baselines
            .iter()
            .map(|baseline| {
                let full = repo.join(&baseline.path);
                let exists = full.is_file();
                let actual = std::fs::read(&full).map(|bytes| hex(&Sha256::digest(&bytes)));
                let sha256_match = match (&baseline.sha256, &actual) {
                    (Some(expected), Some(actual)) => Some(actual == expected),
                    (Some(_), None) => Some(false),
                    (None, _) => None,
                };
                let pass = match baseline.status.as_str() {
                    "deferred" => true,
                    "captured" => exists && sha256_match == Some(true),
                    _ => false,
                };
                BaselineResult {
                    id: baseline.id.clone(),
                    exists,
                    sha256_match,
                    pass,
                }
            })
            .collect()
    }

    /// Structural + drift problems (empty means green).
    pub fn check(&self, repo: &Path) -> Vec<String> {
        let mut problems = Vec::new();
        if self.format != 1 {
            problems.push(format!("screenshot manifest: format {} != 1", self.format));
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for baseline in &self.baselines {
            if !ids.insert(baseline.id.as_str()) {
                problems.push(format!("screenshot manifest: duplicate id `{}`", baseline.id));
            }
            if !paths.insert(baseline.path.as_str()) {
                problems.push(format!(
                    "screenshot manifest: duplicate path `{}`",
                    baseline.path
                ));
            }
            if !baseline.path.starts_with("parity/screenshots/") {
                problems.push(format!(
                    "screenshot manifest: `{}` path is not under parity/screenshots/",
                    baseline.id
                ));
            }
            if !matches!(baseline.status.as_str(), "deferred" | "captured") {
                problems.push(format!(
                    "screenshot manifest: `{}` unknown status `{}`",
                    baseline.id, baseline.status
                ));
            }
            if baseline.status == "captured" && baseline.sha256.is_none() {
                problems.push(format!(
                    "screenshot manifest: captured `{}` has no sha256",
                    baseline.id
                ));
            }
        }
        for result in self.verify(repo) {
            let entry = self
                .baselines
                .iter()
                .find(|baseline| baseline.id == result.id)
                .expect("verified entry exists");
            if !result.pass {
                problems.push(format!(
                    "screenshot manifest: `{}` captured but {}",
                    result.id,
                    if !result.exists {
                        String::from("the PNG is missing")
                    } else {
                        String::from("the sha256 mismatches")
                    }
                ));
            } else if entry.status == "deferred" && result.exists {
                // A committed capture for a deferred entry is a promotion signal,
                // not an error: report it so the manifest can be flipped.
                problems.push(format!(
                    "screenshot manifest: `{}` has a PNG but is still marked deferred",
                    result.id
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

    fn manifest_path() -> std::path::PathBuf {
        crate::paths::find_repo_root(None)
            .expect("repo root")
            .join("parity/screenshots/manifest.json")
    }

    #[test]
    fn committed_screenshot_manifest_is_consistent() {
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        let manifest = ScreenshotManifest::load(&manifest_path()).expect("manifest");
        let problems = manifest.check(&repo);
        assert!(problems.is_empty(), "screenshot problems: {problems:?}");
    }

    #[test]
    fn captured_without_hash_is_flagged() {
        let mut manifest = ScreenshotManifest::load(&manifest_path()).expect("manifest");
        manifest.baselines[0].status = String::from("captured");
        manifest.baselines[0].sha256 = None;
        let repo = crate::paths::find_repo_root(None).expect("repo root");
        assert!(
            manifest
                .check(&repo)
                .iter()
                .any(|p| p.contains("has no sha256"))
        );
    }
}
