// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/core/FileTree.java (`addFile`, `get`, `clear`,
// `loadSound`/`loadMusic` name resolution).

//! Mods-first virtual filesystem (plan 03 §3.3/M5).
//!
//! `FileTree` owns the ordered overlay files plan 20 registers (`Mods` files,
//! `sprites-override/`, data patches) plus an optional internal fallback set.
//! Lookups prefer the newest overlay, then internal, then (by the caller) the
//! host's `res://`/`Core.files.internal` resolver. It is pure data: no
//! filesystem access, no Godot types (HLP §2.2).

use indexmap::IndexMap;

/// One file in the tree (logical path + bytes).
///
/// Mod assets are in-memory (already read by plan 20); internal vanilla assets
/// may be registered as bytes or left to the host resolver when absent here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetFile {
    /// Normalized logical path (`/`-separated, no `.png` stripping).
    pub path: String,
    /// File bytes.
    pub bytes: Vec<u8>,
}

impl AssetFile {
    /// Builds a file from raw bytes (`path` is normalized).
    pub fn new(path: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            path: normalize(&path.into()),
            bytes,
        }
    }
}

/// Normalizes a path for lookup: `\` → `/` (upstream `path.replace('\\','/')`).
pub fn normalize(path: &str) -> String {
    path.replace('\\', "/")
}

/// Mods-first virtual filesystem (`Vars.tree` equivalent).
#[derive(Debug, Clone, Default)]
pub struct FileTree {
    /// Overlay files (mods first; later adds win), normalized keys.
    overlays: IndexMap<String, AssetFile>,
    /// Internal/vanilla files, registered by the host for headless/tests.
    internal: IndexMap<String, AssetFile>,
}

impl FileTree {
    /// Empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// `FileTree.addFile` — register/replace an overlay file (plan 20 hook).
    pub fn add_file(&mut self, path: &str, file: AssetFile) {
        let key = normalize(path);
        self.overlays.insert(key, file);
    }

    /// Registers an internal (vanilla) fallback file.
    pub fn add_internal(&mut self, path: &str, file: AssetFile) {
        let key = normalize(path);
        self.internal.insert(key, file);
    }

    /// `FileTree.clear` — drop every overlay file (mod unload); internal stays.
    pub fn clear(&mut self) {
        self.overlays.clear();
    }

    /// Number of overlay files.
    pub fn overlay_len(&self) -> usize {
        self.overlays.len()
    }

    /// `FileTree.get(path)`: mods first, then internal, then the `/<path>`
    /// leading-slash variant (upstream `get`), else `None`.
    pub fn get(&self, path: &str) -> Option<&AssetFile> {
        let key = normalize(path);
        let slashed = format!("/{key}");
        for map in [&self.overlays, &self.internal] {
            if let Some(file) = map.get(&key) {
                return Some(file);
            }
            if let Some(file) = map.get(&slashed) {
                return Some(file);
            }
        }
        None
    }

    /// `FileTree.get(path) != null` (tree-local existence only).
    pub fn has(&self, path: &str) -> bool {
        self.get(path).is_some()
    }

    /// `FileTree.getAudioPath("sounds/" + name)`: tries `.ogg` then `.mp3`.
    ///
    /// Returns the resolved tree path (the caller loads the bytes through
    /// [`FileTree::get`]). A name that already carries an extension is checked
    /// as-is first.
    pub fn resolve_sound(&self, name: &str) -> Option<String> {
        if name.ends_with(".ogg") || name.ends_with(".mp3") {
            return self.has(name).then(|| name.to_owned());
        }
        for extension in [".ogg", ".mp3"] {
            let candidate = format!("{name}{extension}");
            if self.has(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    /// All overlay keys (sorted), for inventory/diagnostics.
    pub fn overlay_paths(&self) -> Vec<&str> {
        let mut paths: Vec<&str> = self.overlays.keys().map(String::as_str).collect();
        paths.sort_unstable();
        paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, byte: u8) -> AssetFile {
        AssetFile::new(path, vec![byte])
    }

    #[test]
    fn mods_first_backslash_normalization() {
        let mut tree = FileTree::new();
        tree.add_internal(
            "bundles/bundle.properties",
            file("bundles/bundle.properties", 1),
        );
        tree.add_file(
            r"bundles\bundle.properties",
            file("bundles/bundle.properties", 2),
        );

        // Mod file (overlay) wins over internal, and `\` is normalized.
        assert_eq!(
            tree.get("bundles/bundle.properties").unwrap().bytes,
            vec![2]
        );
        assert_eq!(
            tree.get(r"bundles\bundle.properties").unwrap().bytes,
            vec![2]
        );
        assert_eq!(tree.overlay_len(), 1);

        // Overlay add is last-wins.
        tree.add_file(
            "bundles/bundle.properties",
            file("bundles/bundle.properties", 3),
        );
        assert_eq!(
            tree.get("bundles/bundle.properties").unwrap().bytes,
            vec![3]
        );

        // clear drops only overlays; internal remains.
        tree.clear();
        assert_eq!(
            tree.get("bundles/bundle.properties").unwrap().bytes,
            vec![1]
        );
    }

    #[test]
    fn leading_slash_variant_and_missing() {
        let mut tree = FileTree::new();
        tree.add_file("/sprites/blank.png", file("/sprites/blank.png", 7));
        // Upstream `get` retries with a leading `/`.
        assert!(tree.has("sprites/blank.png"));
        assert!(tree.has("/sprites/blank.png"));
        assert!(!tree.has("sprites/missing.png"));
        assert!(tree.get("sprites/missing.png").is_none());
    }

    #[test]
    fn resolve_sound_prefers_ogg_then_mp3() {
        let mut tree = FileTree::new();
        tree.add_internal("sounds/ui/uiBack.mp3", file("sounds/ui/uiBack.mp3", 1));
        assert_eq!(
            tree.resolve_sound("sounds/ui/uiBack").as_deref(),
            Some("sounds/ui/uiBack.mp3")
        );
        tree.add_internal("sounds/ui/uiBack.ogg", file("sounds/ui/uiBack.ogg", 2));
        assert_eq!(
            tree.resolve_sound("sounds/ui/uiBack").as_deref(),
            Some("sounds/ui/uiBack.ogg")
        );
        assert!(tree.resolve_sound("sounds/ui/missing").is_none());
        // Explicit extension is honored as-is.
        assert_eq!(
            tree.resolve_sound("sounds/ui/uiBack.ogg").as_deref(),
            Some("sounds/ui/uiBack.ogg")
        );
    }
}
