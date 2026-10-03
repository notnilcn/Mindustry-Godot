// SPDX-License-Identifier: GPL-3.0-only

//! `parity/screenshots/manifest.json` (format 1): fixed-pose screenshot
//! baselines (plan 23 §3.5/§6.6, M6).
//!
//! In-engine capture needs the Godot editor and is gated on the single-editor
//! mutex. This module scaffolds the directory manifest, the sha256 drift check
//! and the tolerance-based pixel diff (NUD-37: SSIM-equivalent ≤0.5% changed
//! pixels / bounded channel delta), so the diff half is testable headless today.
//! Entries are `deferred` until a capture is committed, at which point a
//! `captured` entry must exist and match its recorded hash.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use mind_atlas::pixmaps::Pixmap;
use mind_atlas::png_io::read_png;
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
                let actual = std::fs::read(&full)
                    .map(|bytes| hex(&Sha256::digest(&bytes)))
                    .ok();
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
                problems.push(format!(
                    "screenshot manifest: duplicate id `{}`",
                    baseline.id
                ));
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
            let Some(entry) = self
                .baselines
                .iter()
                .find(|baseline| baseline.id == result.id)
            else {
                continue;
            };
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

/// Pixel tolerance for a fixed-pose capture diff (plan 23 §3.5, NUD-37).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffTolerance {
    /// Maximum per-channel absolute delta treated as unchanged.
    pub max_channel_delta: u8,
    /// Maximum fraction of pixels allowed to exceed `max_channel_delta`.
    pub max_changed_fraction: f64,
}

impl Default for DiffTolerance {
    fn default() -> Self {
        Self {
            max_channel_delta: 12,
            max_changed_fraction: 0.005,
        }
    }
}

/// The outcome of comparing two same-size RGBA captures.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageDiff {
    /// Capture width.
    pub width: usize,
    /// Capture height.
    pub height: usize,
    /// Total pixels compared.
    pub total_pixels: usize,
    /// Pixels with at least one channel over `max_channel_delta`.
    pub changed_pixels: usize,
    /// `changed_pixels / total_pixels`.
    pub changed_fraction: f64,
    /// Largest per-channel absolute delta observed.
    pub max_channel_delta: u8,
    /// Mean per-channel absolute delta.
    pub mean_channel_delta: f64,
    /// Whether the diff is within tolerance.
    pub pass: bool,
}

impl ImageDiff {
    /// Serializes the diff as a `format: 1` JSON object.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "format": 1,
            "pass": self.pass,
            "width": self.width,
            "height": self.height,
            "total_pixels": self.total_pixels,
            "changed_pixels": self.changed_pixels,
            "changed_fraction": self.changed_fraction,
            "max_channel_delta": self.max_channel_delta,
            "mean_channel_delta": self.mean_channel_delta,
        })
    }
}

/// Compares two same-size RGBA images under a tolerance.
///
/// Geometry is compared exactly (same dimensions required); pixels are compared
/// per channel, matching plan 17's OD-17-A / NUD-37 acceptance rule
/// (≤0.5% changed pixels with a bounded channel delta).
pub fn diff_pixmaps(a: &Pixmap, b: &Pixmap, tolerance: DiffTolerance) -> Result<ImageDiff> {
    if a.width != b.width || a.height != b.height {
        return Err(anyhow!(
            "screenshot diff: dimensions {}x{} != {}x{}",
            a.width,
            a.height,
            b.width,
            b.height
        ));
    }
    let total_pixels = a.width.saturating_mul(a.height);
    let mut changed_pixels = 0usize;
    let mut max_channel_delta = 0u8;
    let mut delta_sum: u64 = 0;
    for (left, right) in a
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.pixels.as_chunks::<4>().0.iter())
    {
        let mut pixel_changed = false;
        for channel in 0..4 {
            let delta = left[channel].abs_diff(right[channel]);
            delta_sum += u64::from(delta);
            max_channel_delta = max_channel_delta.max(delta);
            if delta > tolerance.max_channel_delta {
                pixel_changed = true;
            }
        }
        if pixel_changed {
            changed_pixels += 1;
        }
    }
    let channels = total_pixels.saturating_mul(4).max(1);
    let mean_channel_delta = delta_sum as f64 / channels as f64;
    let changed_fraction = if total_pixels == 0 {
        0.0
    } else {
        changed_pixels as f64 / total_pixels as f64
    };
    let pass = max_channel_delta <= tolerance.max_channel_delta
        && changed_fraction <= tolerance.max_changed_fraction;
    Ok(ImageDiff {
        width: a.width,
        height: a.height,
        total_pixels,
        changed_pixels,
        changed_fraction,
        max_channel_delta,
        mean_channel_delta,
        pass,
    })
}

/// Decodes two PNG captures and diffs them under a tolerance.
///
/// This is the headless half of the in-engine screenshot diff: capture is
/// editor-gated, but a committed baseline (`status = "captured"`) or a fresh
/// `--diff` pair can be compared here.
pub fn diff_png_files(a: &Path, b: &Path, tolerance: DiffTolerance) -> Result<ImageDiff> {
    let left = decode_png(a)?;
    let right = decode_png(b)?;
    diff_pixmaps(&left, &right, tolerance)
}

fn decode_png(path: &Path) -> Result<Pixmap> {
    let bytes = std::fs::read(path).with_context(|| format!("reading `{}`", path.display()))?;
    read_png(&bytes).map_err(|error| anyhow!("decoding `{}`: {error}", path.display()))
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

    fn filled(width: usize, height: usize, rgba: u32) -> Pixmap {
        let mut pixmap = Pixmap::new(width, height);
        pixmap.fill(rgba);
        pixmap
    }

    #[test]
    fn identical_images_are_within_tolerance() {
        let image = filled(8, 8, 0x112233ff);
        let diff = diff_pixmaps(&image, &image.clone(), DiffTolerance::default()).expect("diff");
        assert_eq!(diff.changed_pixels, 0);
        assert_eq!(diff.max_channel_delta, 0);
        assert_eq!(diff.mean_channel_delta, 0.0);
        assert!(diff.pass);
    }

    #[test]
    fn small_perturbation_within_tolerance_passes() {
        let base = filled(10, 10, 0x000000ff);
        let mut tweaked = base.clone();
        // One pixel's red channel moved by 5 (< default channel tolerance 12);
        // no pixel counts as changed, so the diff stays green.
        tweaked.set(0, 0, 0x050000ff);
        let diff = diff_pixmaps(&base, &tweaked, DiffTolerance::default()).expect("diff");
        assert_eq!(diff.max_channel_delta, 5);
        assert_eq!(diff.changed_pixels, 0);
        assert!(diff.pass);
    }

    #[test]
    fn large_change_exceeds_tolerance() {
        let base = filled(10, 10, 0x000000ff);
        let mut changed = base.clone();
        changed.set(0, 0, 0xff0000ff);
        let diff = diff_pixmaps(&base, &changed, DiffTolerance::default()).expect("diff");
        assert_eq!(diff.max_channel_delta, 0xff);
        assert_eq!(diff.changed_pixels, 1);
        // 1/100 = 1% > 0.5% changed-pixel budget.
        assert!(!diff.pass);
    }

    #[test]
    fn size_mismatch_is_an_error() {
        let a = filled(4, 4, 0x000000ff);
        let b = filled(4, 5, 0x000000ff);
        assert!(diff_pixmaps(&a, &b, DiffTolerance::default()).is_err());
    }

    #[test]
    fn png_roundtrip_is_pixel_identical() {
        let image = filled(4, 4, 0x20406080);
        let encoded = mind_atlas::png_io::write_png(&image).expect("encode");
        let decoded = read_png(&encoded).expect("decode");
        let diff = diff_pixmaps(&image, &decoded, DiffTolerance::default()).expect("diff");
        assert_eq!(diff.max_channel_delta, 0, "RGBA PNG is lossless");
        assert!(diff.pass);
    }
}
