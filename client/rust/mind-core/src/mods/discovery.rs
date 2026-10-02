// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Mod package discovery: folder/zip roots, `resolveRoot`, metadata search.
//!
//! Ported from `core/src/mindustry/mod/Mods.java` (`load` candidate filter,
//! `resolveRoot`, `findMeta`, `loadMod`) and Arc `Fi`/`ZipFi` listing semantics.
//!
//! `mind-core` never assumes `std::fs`; every read goes through plan 04's
//! [`FileSystem`] so tests can use [`MockFs`](crate::io::MockFs).

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use crate::io::FileSystem;

use super::ModError;
use super::meta::{META_FILES, ModMeta};

/// One immediate child of a mod directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModEntry {
    /// File/directory name (no path prefix).
    pub name: String,
    /// Whether this entry is a directory.
    pub is_dir: bool,
}

/// Where a mod came from (`Mods.LoadedMod` + `Publishable` source tags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSource {
    /// Discovered as a directory under `mods/`.
    LocalFolder,
    /// Discovered as a `.zip`/`.jar` under `mods/`.
    LocalZip,
    /// Imported from outside the mod directory.
    ImportedZip,
    /// Downloaded by the mod browser (plan 14).
    BrowserZip,
    /// Steam Workshop (plan 22 stub).
    Workshop,
}

/// A mod's file tree: a folder on disk or an in-memory zip container.
///
/// All paths are relative to the resolved mod root with `/` separators and no
/// leading slash; `resolve_root` has already unwrapped a single top-level
/// directory (GitHub zipball shape).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModRoot {
    /// A directory on the real/mock filesystem.
    Folder(PathBuf),
    /// Decompressed zip entries keyed by root-relative path.
    Zip(BTreeMap<String, Vec<u8>>),
}

impl ModRoot {
    /// Reads a root-relative file.
    pub fn read(&self, fs: &dyn FileSystem, rel: &str) -> Result<Vec<u8>, ModError> {
        match self {
            ModRoot::Folder(base) => Ok(fs.read(&base.join(rel))?),
            ModRoot::Zip(entries) => entries
                .get(rel)
                .cloned()
                .ok_or_else(|| ModError::Invalid(format!("missing mod file `{rel}`"))),
        }
    }

    /// Reads a root file as UTF-8 text.
    pub fn read_to_string(&self, fs: &dyn FileSystem, rel: &str) -> Result<String, ModError> {
        let bytes = self.read(fs, rel)?;
        String::from_utf8(bytes)
            .map_err(|_| ModError::Invalid(format!("mod file `{rel}` is not UTF-8")))
    }

    /// Whether a root-relative path exists.
    pub fn exists(&self, fs: &dyn FileSystem, rel: &str) -> bool {
        match self {
            ModRoot::Folder(base) => fs.exists(&base.join(rel)),
            ModRoot::Zip(entries) => {
                entries.contains_key(rel)
                    || (rel.is_empty() && !entries.is_empty())
                    || entries
                        .keys()
                        .any(|key| key.starts_with(&format!("{rel}/")))
            }
        }
    }

    /// Whether a root-relative path is a directory.
    pub fn is_dir(&self, fs: &dyn FileSystem, rel: &str) -> bool {
        match self {
            ModRoot::Folder(base) => {
                let path = base.join(rel);
                fs.exists(&path) && !fs.is_file(&path)
            }
            ModRoot::Zip(entries) => {
                let prefix = if rel.is_empty() {
                    String::new()
                } else {
                    format!("{rel}/")
                };
                entries
                    .keys()
                    .any(|key| key.starts_with(&prefix) && key.len() > prefix.len())
            }
        }
    }

    /// Immediate children of `rel` (files and directories), name-sorted.
    pub fn list(&self, fs: &dyn FileSystem, rel: &str) -> Result<Vec<ModEntry>, ModError> {
        let mut out: BTreeMap<String, bool> = BTreeMap::new();
        match self {
            ModRoot::Folder(base) => {
                let dir = if rel.is_empty() {
                    base.clone()
                } else {
                    base.join(rel)
                };
                for path in fs.ls(&dir)? {
                    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                        continue;
                    };
                    out.insert(name.to_owned(), !fs.is_file(&path));
                }
            }
            ModRoot::Zip(entries) => {
                let prefix = if rel.is_empty() {
                    String::new()
                } else {
                    format!("{rel}/")
                };
                for key in entries.keys() {
                    let Some(rest) = key.strip_prefix(&prefix) else {
                        continue;
                    };
                    match rest.split_once('/') {
                        Some((head, _)) => {
                            out.insert(head.to_owned(), true);
                        }
                        None => {
                            out.insert(rest.to_owned(), false);
                        }
                    }
                }
            }
        }
        Ok(out
            .into_iter()
            .map(|(name, is_dir)| ModEntry { name, is_dir })
            .collect())
    }

    /// All files below `rel` (recursive), root-relative, sorted.
    pub fn walk(&self, fs: &dyn FileSystem, rel: &str) -> Result<Vec<String>, ModError> {
        match self {
            ModRoot::Folder(base) => {
                let dir = if rel.is_empty() {
                    base.clone()
                } else {
                    base.join(rel)
                };
                let mut out = Vec::new();
                for path in fs.walk(&dir)? {
                    let Ok(rel_path) = path.strip_prefix(base) else {
                        continue;
                    };
                    out.push(normalize_path(rel_path));
                }
                out.sort();
                Ok(out)
            }
            ModRoot::Zip(entries) => {
                let prefix = if rel.is_empty() {
                    String::new()
                } else {
                    format!("{rel}/")
                };
                let mut out: Vec<String> = entries
                    .keys()
                    .filter(|key| key.starts_with(&prefix))
                    .cloned()
                    .collect();
                out.sort();
                Ok(out)
            }
        }
    }
}

/// Normalizes a path to `/`-separated, no leading slash, relative form.
pub fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Rejects zip-slip: `..`/`.` components and absolute paths (`validPath`, R6).
pub fn valid_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') {
        return false;
    }
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return false;
        }
        if component.contains('\\') {
            return false;
        }
    }
    true
}

/// Loads a `.zip`/`.jar` into root-relative entries, applying `resolve_root`.
pub fn read_zip(bytes: &[u8]) -> Result<ModRoot, ModError> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)
        .map_err(|error| ModError::Invalid(format!("invalid mod zip: {error}")))?;
    let mut raw: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|error| ModError::Invalid(format!("invalid mod zip entry: {error}")))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().replace('\\', "/");
        if !valid_path(&name) {
            return Err(ModError::Invalid(format!(
                "mod zip contains an unsafe path `{name}`"
            )));
        }
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|error| ModError::Invalid(format!("reading mod zip entry: {error}")))?;
        raw.insert(name, data);
    }
    Ok(ModRoot::Zip(unwrap_single_dir(raw)))
}

/// `resolveRoot` for zip entries: if every entry lives under one top-level
/// directory, strip that prefix.
fn unwrap_single_dir(raw: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    let mut top: Option<&str> = None;
    let mut single = true;
    for key in raw.keys() {
        let head = key.split('/').next().unwrap_or(key);
        match top {
            None => top = Some(head),
            Some(existing) if existing == head => {}
            Some(_) => {
                single = false;
                break;
            }
        }
    }
    let Some(head) = top else {
        return raw;
    };
    if !single || raw.contains_key(head) {
        return raw;
    }
    let prefix = format!("{head}/");
    if !raw.keys().all(|key| key.starts_with(&prefix)) {
        return raw;
    }
    raw.into_iter()
        .filter_map(|(key, value)| key.strip_prefix(&prefix).map(|k| (k.to_owned(), value)))
        .collect()
}

/// `resolveRoot` for a folder path: unwrap a single top-level directory.
pub fn resolve_folder_root(fs: &dyn FileSystem, dir: &Path) -> PathBuf {
    match fs.ls(dir) {
        Ok(entries) if entries.len() == 1 => {
            let only = &entries[0];
            if fs.exists(only) && !fs.is_file(only) {
                return only.clone();
            }
            dir.to_path_buf()
        }
        _ => dir.to_path_buf(),
    }
}

/// A discovered candidate mod (before metadata resolution).
#[derive(Debug, Clone)]
pub struct ModCandidate {
    /// Source path under the mod directory.
    pub file: PathBuf,
    /// Resolved root (folder path or in-memory zip tree).
    pub root: ModRoot,
    /// Source classification.
    pub source: ModSource,
}

/// Scans `dir` for mod candidates: `.jar`/`.zip` files and directories that
/// contain one of the four meta files (after `resolve_root`).
pub fn scan_mod_directory(fs: &dyn FileSystem, dir: &Path) -> Result<Vec<ModCandidate>, ModError> {
    let mut out = Vec::new();
    for path in fs.ls(dir)? {
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase);
        if matches!(extension.as_deref(), Some("jar") | Some("zip")) {
            let bytes = fs.read(&path)?;
            match read_zip(&bytes) {
                Ok(root) => out.push(ModCandidate {
                    file: path,
                    root,
                    source: ModSource::LocalZip,
                }),
                Err(error) => {
                    log::warn!("[Mods] skipping `{}`: {error}", path.display());
                }
            }
            continue;
        }
        if fs.is_file(&path) {
            continue;
        }
        let root = resolve_folder_root(fs, &path);
        if find_meta(fs, &ModRoot::Folder(root.clone())).is_some() {
            out.push(ModCandidate {
                file: path,
                root: ModRoot::Folder(root),
                source: ModSource::LocalFolder,
            });
        }
    }
    Ok(out)
}

/// `Mods.findMeta`: returns the first meta file found and its parsed metadata.
pub fn find_meta(fs: &dyn FileSystem, root: &ModRoot) -> Option<(String, ModMeta)> {
    for name in META_FILES {
        if root.exists(fs, name) {
            let text = root.read_to_string(fs, name).ok()?;
            return ModMeta::parse(&text)
                .ok()
                .map(|meta| (name.to_owned(), meta));
        }
    }
    None
}

/// `Mods.getConfigFolder` path contract: `<modDirectory>/<internal-name>`.
pub fn config_folder(mod_directory: &Path, internal_name: &str) -> PathBuf {
    mod_directory.join(internal_name)
}

/// `Mods.getConfig` path contract: `<modDirectory>/<internal-name>/config.json`.
pub fn config_file(mod_directory: &Path, internal_name: &str) -> PathBuf {
    config_folder(mod_directory, internal_name).join("config.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;

    /// Plan 20 M0: `discovery::single_dir_root`.
    #[test]
    fn single_dir_root() {
        let fs = MockFs::new();
        fs.write(
            Path::new("/mods/wrapped/inner/mod.json"),
            b"{\"name\":\"Inner\"}",
        )
        .expect("write");
        fs.write(Path::new("/mods/wrapped/inner/content/items/x.json"), b"{}")
            .expect("write");
        let root = resolve_folder_root(&fs, Path::new("/mods/wrapped"));
        assert_eq!(root, PathBuf::from("/mods/wrapped/inner"));
        let candidates = scan_mod_directory(&fs, Path::new("/mods")).expect("scan");
        assert_eq!(candidates.len(), 1);
        let (_, meta) = find_meta(&fs, &candidates[0].root).expect("meta");
        assert_eq!(meta.name, "Inner");
    }

    #[test]
    fn direct_folder_root() {
        let fs = MockFs::new();
        fs.write(Path::new("/mods/direct/mod.json"), b"{\"name\":\"Direct\"}")
            .expect("write");
        fs.write(Path::new("/mods/direct/content/items/x.json"), b"{}")
            .expect("write");
        let candidates = scan_mod_directory(&fs, Path::new("/mods")).expect("scan");
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].root,
            ModRoot::Folder(PathBuf::from("/mods/direct"))
        );
        let entries = candidates[0]
            .root
            .list(&fs, "content")
            .expect("list content");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_dir);
        let files = candidates[0].root.walk(&fs, "content").expect("walk");
        assert_eq!(files, vec![String::from("content/items/x.json")]);
    }

    #[test]
    fn zip_root_and_valid_path() {
        // Build a tiny zip in memory with the `zip` writer is out of scope;
        // validate path rules directly and the single-dir unwrap helper.
        assert!(valid_path("mod.json"));
        assert!(valid_path("content/items/x.json"));
        assert!(!valid_path("../evil"));
        assert!(!valid_path("/abs"));
        assert!(!valid_path("a//b"));

        let mut raw = BTreeMap::new();
        raw.insert(String::from("dir/mod.json"), vec![1]);
        raw.insert(String::from("dir/content/x.json"), vec![2]);
        let unwrapped = unwrap_single_dir(raw);
        assert_eq!(unwrapped.len(), 2);
        assert!(unwrapped.contains_key("mod.json"));
        assert!(unwrapped.contains_key("content/x.json"));

        let mut flat = BTreeMap::new();
        flat.insert(String::from("mod.json"), vec![1]);
        flat.insert(String::from("content/x.json"), vec![2]);
        assert_eq!(unwrap_single_dir(flat).len(), 2);
    }

    #[test]
    fn mixed_root_is_not_unwrapped() {
        let mut raw = BTreeMap::new();
        raw.insert(String::from("a/mod.json"), vec![1]);
        raw.insert(String::from("b/other.json"), vec![2]);
        let unwrapped = unwrap_single_dir(raw);
        assert!(unwrapped.contains_key("a/mod.json"));
    }

    #[test]
    fn config_paths() {
        assert_eq!(
            config_folder(Path::new("/data/mods"), "test-mod"),
            PathBuf::from("/data/mods/test-mod")
        );
        assert_eq!(
            config_file(Path::new("/data/mods"), "test-mod"),
            PathBuf::from("/data/mods/test-mod/config.json")
        );
    }
}
