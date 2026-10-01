// SPDX-License-Identifier: GPL-3.0-only

//! Deterministic file-tree hashing and manifest IO (plan 03 §3.5 stages 1/8).
//!
//! Used by `mind-tools migrate` (provenance manifest), `mind-tools pack`
//! (`asset_manifest.json`, `inputsHash`) and the `mind-headless assets`
//! verification scenarios. All walks are sorted by normalized relative path
//! and all hashes are sha256 — two runs over identical inputs produce
//! identical manifests (§2.2 definition of done).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{AtlasError, Result};

/// sha256 of one file, lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path)?;
    Ok(sha256_hex(&bytes))
}

/// sha256 of a byte slice, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// One hashed file entry (relative `/`-normalized path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHash {
    /// Relative path from the walked root, `/` separators.
    pub path: String,
    /// Lowercase hex sha256 of the file bytes.
    pub sha256: String,
    /// File length in bytes.
    pub len: u64,
}

/// A deterministic manifest over a tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeManifest {
    /// Manifest format version.
    pub format: u32,
    /// Root label (informational; e.g. `assets/`).
    pub root: String,
    /// Optional upstream commit hash the tree was migrated from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_commit: Option<String>,
    /// Sorted file entries.
    pub files: Vec<FileHash>,
    /// sha256 over the sorted `(path, sha256)` pairs.
    pub tree_hash: String,
}

impl TreeManifest {
    /// Builds a manifest over `root` (labeled `label`), excluding paths for
    /// which `exclude(relative_path, is_dir)` returns true.
    pub fn build(
        root: &Path,
        label: &str,
        upstream_commit: Option<String>,
        exclude: impl Fn(&str, bool) -> bool,
    ) -> Result<TreeManifest> {
        let files = hash_tree(root, &exclude)?;
        let tree_hash = combined_hash(&files);
        Ok(TreeManifest {
            format: 1,
            root: label.to_owned(),
            upstream_commit,
            files,
            tree_hash,
        })
    }

    /// Writes the manifest as pretty JSON (deterministic key order).
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = format!("{}\n", serde_json::to_string_pretty(self)?);
        fs::write(path, text)?;
        Ok(())
    }

    /// Reads a manifest written by [`TreeManifest::write`].
    pub fn read(path: &Path) -> Result<TreeManifest> {
        let text = fs::read_to_string(path)?;
        let manifest: TreeManifest = serde_json::from_str(&text)?;
        Ok(manifest)
    }

    /// Recomputes hashes over `root` and reports mismatches: missing files,
    /// extra files, and content drift. Deterministic ordering.
    pub fn verify(&self, root: &Path, exclude: impl Fn(&str, bool) -> bool) -> Result<Vec<String>> {
        let current = hash_tree(root, &exclude)?;
        let mut problems = Vec::new();
        let current_map: std::collections::BTreeMap<&str, &FileHash> =
            current.iter().map(|f| (f.path.as_str(), f)).collect();
        let expected_map: std::collections::BTreeMap<&str, &FileHash> =
            self.files.iter().map(|f| (f.path.as_str(), f)).collect();
        for (path, expected) in &expected_map {
            match current_map.get(path) {
                None => problems.push(format!("missing: {path}")),
                Some(actual) if actual.sha256 != expected.sha256 => {
                    problems.push(format!("drift: {path}"))
                }
                Some(_) => {}
            }
        }
        for path in current_map.keys() {
            if !expected_map.contains_key(path) {
                problems.push(format!("extra: {path}"));
            }
        }
        Ok(problems)
    }
}

/// Combined hash of a sorted entry list (§6.3 `inputsHash` semantics).
pub fn combined_hash(files: &[FileHash]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.path.as_bytes());
        hasher.update([0u8]);
        hasher.update(file.sha256.as_bytes());
        hasher.update(b"\n");
    }
    let digest = hasher.finalize();
    let mut out = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Deterministic sorted hash walk over `root`.
pub fn hash_tree(root: &Path, exclude: &impl Fn(&str, bool) -> bool) -> Result<Vec<FileHash>> {
    let mut paths = Vec::new();
    walk(root, root, exclude, &mut paths)?;
    paths.sort();
    let mut out = Vec::with_capacity(paths.len());
    for rel in paths {
        let full = root.join(&rel);
        let meta = fs::metadata(&full)?;
        let normalized = rel.to_string_lossy().replace('\\', "/");
        out.push(FileHash {
            path: normalized,
            sha256: sha256_file(&full)?,
            len: meta.len(),
        });
    }
    Ok(out)
}

fn walk(
    root: &Path,
    dir: &Path,
    exclude: &impl Fn(&str, bool) -> bool,
    out: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let rel = path
            .strip_prefix(root)
            .map_err(|error| AtlasError::Invalid(error.to_string()))?
            .to_path_buf();
        let normalized = rel.to_string_lossy().replace('\\', "/");
        let is_dir = path.is_dir();
        if exclude(&normalized, is_dir) {
            continue;
        }
        if is_dir {
            walk(root, &path, exclude, out)?;
        } else if path.is_file() {
            out.push(rel);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_hash_is_deterministic_and_order_independent() {
        let dir = std::env::temp_dir().join(format!("mind-atlas-test-{}", std::process::id()));
        let sub = dir.join("sub");
        fs::create_dir_all(&sub).unwrap();
        fs::write(dir.join("b.png"), b"two").unwrap();
        fs::write(sub.join("a.png"), b"one").unwrap();

        let first = TreeManifest::build(&dir, "test", None, |_, _| false).unwrap();
        let second = TreeManifest::build(&dir, "test", None, |_, _| false).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.files.len(), 2);
        assert_eq!(first.files[0].path, "b.png");
        assert_eq!(first.files[1].path, "sub/a.png");
        assert!(first.verify(&dir, |_, _| false).unwrap().is_empty());

        fs::write(dir.join("b.png"), b"drifted").unwrap();
        let problems = first.verify(&dir, |_, _| false).unwrap();
        assert_eq!(problems, vec!["drift: b.png".to_owned()]);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn excludes_apply_to_dirs_and_files() {
        let dir = std::env::temp_dir().join(format!("mind-atlas-test-ex-{}", std::process::id()));
        fs::create_dir_all(dir.join("keep")).unwrap();
        fs::create_dir_all(dir.join("skip")).unwrap();
        fs::write(dir.join("keep/a.png"), b"a").unwrap();
        fs::write(dir.join("skip/b.png"), b"b").unwrap();
        fs::write(dir.join("skip.dat"), b"c").unwrap();

        let manifest =
            TreeManifest::build(&dir, "test", None, |path, _| path.starts_with("skip")).unwrap();
        assert_eq!(manifest.files.len(), 1);
        assert_eq!(manifest.files[0].path, "keep/a.png");

        fs::remove_dir_all(&dir).unwrap();
    }
}
