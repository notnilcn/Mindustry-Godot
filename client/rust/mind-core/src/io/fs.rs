// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Filesystem abstraction and the data-directory layout (plan 04 §3.8).
//!
//! Ported from Arc `Fi`/`Files` (the surface `mindustry.io` uses) and
//! `core/src/mindustry/Vars.java` directory constants (`saveDirectory`,
//! `mapPreviewDirectory`, custom maps, settings file). Sim/IO code never touches
//! `std::fs` directly; tests use [`MockFs`] (parity with Arc `MockFiles`).
//!
//! Data-root resolution is owned by plan 22 §6.1 (HLP §12 C5): app-data default
//! with a portable `./data` override. This module only resolves the *override or
//! default* through [`crate::config::default_data_dir`] and defines the layout
//! below the root.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::IoError;

/// Save/map file extension (`Vars.saveExtension`); maps share it (`Vars.mapExtension`).
pub const SAVE_EXTENSION: &str = "msav";

/// Filesystem trait (`Fi`-like handles by path).
///
/// All paths are absolute or relative-to-process; the trait is object-safe so
/// hosts can swap [`NativeFs`] for [`MockFs`] without generic plumbing.
pub trait FileSystem: Send + Sync {
    /// Reads an entire file.
    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError>;
    /// Writes (create/truncate) a file, creating parent directories.
    fn write(&self, path: &Path, data: &[u8]) -> Result<(), IoError>;
    /// Writes a file and flushes it to stable storage before returning.
    fn write_sync(&self, path: &Path, data: &[u8]) -> Result<(), IoError>;
    /// Appends to a file, creating parent directories.
    fn append(&self, path: &Path, data: &[u8]) -> Result<(), IoError>;
    /// Whether the path exists (file or directory).
    fn exists(&self, path: &Path) -> bool;
    /// Whether the path is an existing file.
    fn is_file(&self, path: &Path) -> bool;
    /// File length in bytes.
    fn len(&self, path: &Path) -> Result<u64, IoError>;
    /// Deletes a file; a missing file is not an error (`Fi.delete` semantics).
    fn delete(&self, path: &Path) -> Result<(), IoError>;
    /// Moves/renames a file, replacing an existing target.
    ///
    /// `std::fs::rename` refuses to replace an existing target on Windows, so
    /// implementations remove the target first (same best-effort atomicity as
    /// Arc `Fi.moveTo`).
    fn rename(&self, from: &Path, to: &Path) -> Result<(), IoError>;
    /// Copies a file, creating parent directories of the target.
    fn copy(&self, from: &Path, to: &Path) -> Result<(), IoError>;
    /// Lists the direct children of a directory (files and directories), sorted.
    /// A missing directory yields an empty list.
    fn ls(&self, dir: &Path) -> Result<Vec<PathBuf>, IoError>;
    /// Lists all files below a directory recursively, sorted (`Fi.walk`).
    fn walk(&self, dir: &Path) -> Result<Vec<PathBuf>, IoError>;
    /// Creates a directory and all missing parents.
    fn mkdirs(&self, dir: &Path) -> Result<(), IoError>;

    /// Atomic write: write `<path>.tmp`, sync, rename over `path`
    /// (settings flush + save writes, plan 04 §3.2/§3.7).
    fn write_atomic(&self, path: &Path, data: &[u8]) -> Result<(), IoError> {
        let tmp = tmp_path_for(path);
        let result = self
            .write_sync(&tmp, data)
            .and_then(|()| self.rename(&tmp, path));
        if result.is_err() {
            let _ = self.delete(&tmp);
        }
        result
    }
}

/// The path of the temporary sibling used by atomic writes (`<file>.tmp`).
pub fn tmp_path_for(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!("{name}.tmp"))
}

/// Real filesystem backed by `std::fs`.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeFs;

impl FileSystem for NativeFs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        Ok(std::fs::read(path)?)
    }

    fn write(&self, path: &Path, data: &[u8]) -> Result<(), IoError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(std::fs::write(path, data)?)
    }

    fn write_sync(&self, path: &Path, data: &[u8]) -> Result<(), IoError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(path)?;
        file.write_all(data)?;
        file.sync_all()?;
        Ok(())
    }

    fn append(&self, path: &Path, data: &[u8]) -> Result<(), IoError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?;
        file.write_all(data)?;
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn len(&self, path: &Path) -> Result<u64, IoError> {
        Ok(std::fs::metadata(path)?.len())
    }

    fn delete(&self, path: &Path) -> Result<(), IoError> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), IoError> {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if to.exists() {
            // Windows cannot rename over an existing file; remove it first.
            std::fs::remove_file(to)?;
        }
        Ok(std::fs::rename(from, to)?)
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), IoError> {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to)?;
        Ok(())
    }

    fn ls(&self, dir: &Path) -> Result<Vec<PathBuf>, IoError> {
        let mut out = Vec::new();
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries {
                    out.push(entry?.path());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(error) => return Err(error.into()),
        }
        out.sort();
        Ok(out)
    }

    fn walk(&self, dir: &Path) -> Result<Vec<PathBuf>, IoError> {
        let mut out = Vec::new();
        walk_native(dir, &mut out)?;
        out.sort();
        Ok(out)
    }

    fn mkdirs(&self, dir: &Path) -> Result<(), IoError> {
        Ok(std::fs::create_dir_all(dir)?)
    }
}

fn walk_native(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), IoError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            walk_native(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// In-memory filesystem for tests and headless fixtures (Arc `MockFiles` parity).
///
/// Deterministic: entries live in `BTreeMap`s so `ls`/`walk` are sorted. Also
/// supports failure injection (`set_read_only`) for backup-restore tests.
#[derive(Debug, Default)]
pub struct MockFs {
    files: Mutex<BTreeMap<PathBuf, Vec<u8>>>,
    dirs: Mutex<BTreeSet<PathBuf>>,
    read_only: AtomicBool,
}

impl MockFs {
    /// Empty filesystem.
    pub fn new() -> Self {
        Self::default()
    }

    /// When set, all mutating operations fail (backup-restore fault injection).
    pub fn set_read_only(&self, read_only: bool) {
        self.read_only.store(read_only, Ordering::Relaxed);
    }

    fn check_writable(&self) -> Result<(), IoError> {
        if self.read_only.load(Ordering::Relaxed) {
            return Err(IoError::Io(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "MockFs is read-only",
            )));
        }
        Ok(())
    }

    /// Number of stored files.
    pub fn file_count(&self) -> usize {
        self.files.lock().map(|files| files.len()).unwrap_or(0)
    }
}

impl FileSystem for MockFs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, IoError> {
        let files = self
            .files
            .lock()
            .map_err(|_| IoError::corrupt("MockFs poisoned"))?;
        files.get(path).cloned().ok_or_else(|| {
            IoError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no such file: {}", path.display()),
            ))
        })
    }

    fn write(&self, path: &Path, data: &[u8]) -> Result<(), IoError> {
        self.check_writable()?;
        self.mkdirs(path.parent().unwrap_or_else(|| Path::new(".")))?;
        self.files
            .lock()
            .map_err(|_| IoError::corrupt("MockFs poisoned"))?
            .insert(path.to_path_buf(), data.to_vec());
        Ok(())
    }

    fn write_sync(&self, path: &Path, data: &[u8]) -> Result<(), IoError> {
        self.write(path, data)
    }

    fn append(&self, path: &Path, data: &[u8]) -> Result<(), IoError> {
        self.check_writable()?;
        self.mkdirs(path.parent().unwrap_or_else(|| Path::new(".")))?;
        let mut files = self
            .files
            .lock()
            .map_err(|_| IoError::corrupt("MockFs poisoned"))?;
        files
            .entry(path.to_path_buf())
            .or_default()
            .extend_from_slice(data);
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let in_files = self
            .files
            .lock()
            .map(|f| f.contains_key(path))
            .unwrap_or(false);
        let in_dirs = self.dirs.lock().map(|d| d.contains(path)).unwrap_or(false);
        in_files || in_dirs
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files
            .lock()
            .map(|f| f.contains_key(path))
            .unwrap_or(false)
    }

    fn len(&self, path: &Path) -> Result<u64, IoError> {
        Ok(self.read(path)?.len() as u64)
    }

    fn delete(&self, path: &Path) -> Result<(), IoError> {
        self.check_writable()?;
        self.files
            .lock()
            .map_err(|_| IoError::corrupt("MockFs poisoned"))?
            .remove(path);
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), IoError> {
        self.check_writable()?;
        self.mkdirs(to.parent().unwrap_or_else(|| Path::new(".")))?;
        let mut files = self
            .files
            .lock()
            .map_err(|_| IoError::corrupt("MockFs poisoned"))?;
        let data = files.remove(from).ok_or_else(|| {
            IoError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no such file: {}", from.display()),
            ))
        })?;
        files.insert(to.to_path_buf(), data);
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> Result<(), IoError> {
        self.check_writable()?;
        let data = self.read(from)?;
        self.write(to, &data)
    }

    fn ls(&self, dir: &Path) -> Result<Vec<PathBuf>, IoError> {
        let mut out = BTreeSet::new();
        let files = self
            .files
            .lock()
            .map_err(|_| IoError::corrupt("MockFs poisoned"))?;
        for path in files.keys() {
            if path.parent() == Some(dir) {
                out.insert(path.clone());
            }
        }
        let dirs = self
            .dirs
            .lock()
            .map_err(|_| IoError::corrupt("MockFs poisoned"))?;
        for path in dirs.iter() {
            if path.parent() == Some(dir) {
                out.insert(path.clone());
            }
        }
        Ok(out.into_iter().collect())
    }

    fn walk(&self, dir: &Path) -> Result<Vec<PathBuf>, IoError> {
        let files = self
            .files
            .lock()
            .map_err(|_| IoError::corrupt("MockFs poisoned"))?;
        let prefix = dir.to_path_buf();
        Ok(files
            .keys()
            .filter(|path| path.starts_with(&prefix))
            .cloned()
            .collect())
    }

    fn mkdirs(&self, dir: &Path) -> Result<(), IoError> {
        self.check_writable()?;
        let mut dirs = self
            .dirs
            .lock()
            .map_err(|_| IoError::corrupt("MockFs poisoned"))?;
        let mut current = PathBuf::new();
        for part in dir.components() {
            current.push(part);
            dirs.insert(current.clone());
        }
        Ok(())
    }
}

/// The data-directory layout below one root (`Vars.*Directory` constants,
/// plan 04 §6.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    /// Uses an explicit root.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolves the root: explicit override, else the platform default
    /// (`MINDUSTRY_GODOT_DATA_DIR` → XDG data dir; plan 22 §6.1 owns the full
    /// portable-`./data` resolution, HLP §12 C5).
    pub fn resolve(override_dir: Option<&Path>) -> Self {
        match override_dir {
            Some(dir) => Self::new(dir),
            None => Self::new(crate::config::default_data_dir()),
        }
    }

    /// The data root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `saves/` — slot files `<n>.msav`, sector saves, backups.
    pub fn saves(&self) -> PathBuf {
        self.root.join("saves")
    }

    /// `maps/` — custom maps (`data/maps/` upstream).
    pub fn maps(&self) -> PathBuf {
        self.root.join("maps")
    }

    /// `previews/` — save-slot and map preview caches (`mapPreviewDirectory`).
    pub fn previews(&self) -> PathBuf {
        self.root.join("previews")
    }

    /// `config/` — settings and keybinds.
    pub fn config(&self) -> PathBuf {
        self.root.join("config")
    }

    /// `config/settings.bin`.
    pub fn settings_file(&self) -> PathBuf {
        self.config().join("settings.bin")
    }

    /// Save file for a numbered slot (`SaveIO.fileFor`): `saves/<n>.msav`.
    pub fn save_slot(&self, slot: u32) -> PathBuf {
        self.saves().join(format!("{slot}.{SAVE_EXTENSION}"))
    }

    /// Sector save file (`Saves.getSectorFile`): `saves/sector-<planet>-<id>.msav`.
    pub fn sector_save(&self, planet: &str, id: u32) -> PathBuf {
        self.saves()
            .join(format!("sector-{planet}-{id}.{SAVE_EXTENSION}"))
    }

    /// Preview image for a save file stem (`SaveSlot.previewFile`):
    /// `previews/save_slot_<stem>.png`.
    pub fn preview_for_save(&self, file_stem: &str) -> PathBuf {
        self.previews().join(format!("save_slot_{file_stem}.png"))
    }

    /// Preview image for a map name (`Vars.mapPreviewDirectory`):
    /// `previews/<name>_v2.png`.
    pub fn preview_for_map(&self, name: &str) -> PathBuf {
        self.previews().join(format!("{name}_v2.png"))
    }

    /// Minimap pixel cache for a map: `previews/<name>-cache_v2.dat`.
    pub fn map_cache(&self, name: &str) -> PathBuf {
        self.previews().join(format!("{name}-cache_v2.dat"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_fs_roundtrip_and_walk() {
        let fs = MockFs::new();
        fs.write(Path::new("/data/saves/0.msav"), b"one").unwrap();
        fs.write(Path::new("/data/saves/1.msav"), b"two").unwrap();
        fs.write(Path::new("/data/maps/m.msav"), b"map").unwrap();

        assert_eq!(fs.read(Path::new("/data/saves/0.msav")).unwrap(), b"one");
        assert!(fs.exists(Path::new("/data/saves")));
        assert!(fs.is_file(Path::new("/data/saves/1.msav")));
        assert_eq!(fs.len(Path::new("/data/saves/1.msav")).unwrap(), 3);

        let saves = fs.ls(Path::new("/data/saves")).unwrap();
        assert_eq!(
            saves,
            vec![
                PathBuf::from("/data/saves/0.msav"),
                PathBuf::from("/data/saves/1.msav")
            ]
        );
        let all = fs.walk(Path::new("/data")).unwrap();
        assert_eq!(all.len(), 3);

        fs.rename(
            Path::new("/data/saves/0.msav"),
            Path::new("/data/saves/9.msav"),
        )
        .unwrap();
        assert!(!fs.exists(Path::new("/data/saves/0.msav")));
        assert_eq!(fs.read(Path::new("/data/saves/9.msav")).unwrap(), b"one");

        fs.append(Path::new("/data/saves/9.msav"), b"!").unwrap();
        assert_eq!(fs.read(Path::new("/data/saves/9.msav")).unwrap(), b"one!");

        fs.delete(Path::new("/data/saves/9.msav")).unwrap();
        fs.delete(Path::new("/data/saves/9.msav")).unwrap();
        assert!(!fs.exists(Path::new("/data/saves/9.msav")));
    }

    #[test]
    fn mock_fs_read_only_injection() {
        let fs = MockFs::new();
        fs.write(Path::new("/a.bin"), b"a").unwrap();
        fs.set_read_only(true);
        assert!(fs.write(Path::new("/b.bin"), b"b").is_err());
        assert!(fs.delete(Path::new("/a.bin")).is_err());
        assert!(fs.rename(Path::new("/a.bin"), Path::new("/c.bin")).is_err());
        assert_eq!(fs.read(Path::new("/a.bin")).unwrap(), b"a");
        fs.set_read_only(false);
        fs.write(Path::new("/b.bin"), b"b").unwrap();
    }

    #[test]
    fn mock_fs_atomic_write_replaces() {
        let fs = MockFs::new();
        let path = Path::new("/data/config/settings.bin");
        fs.write_atomic(path, b"v1").unwrap();
        fs.write_atomic(path, b"v2-longer").unwrap();
        assert_eq!(fs.read(path).unwrap(), b"v2-longer");
        assert!(!fs.exists(Path::new("/data/config/settings.bin.tmp")));
    }

    #[test]
    fn paths_layout() {
        let paths = Paths::new("/data");
        assert_eq!(paths.saves(), PathBuf::from("/data/saves"));
        assert_eq!(paths.save_slot(3), PathBuf::from("/data/saves/3.msav"));
        assert_eq!(
            paths.sector_save("serpulo", 12),
            PathBuf::from("/data/saves/sector-serpulo-12.msav")
        );
        assert_eq!(
            paths.preview_for_save("0"),
            PathBuf::from("/data/previews/save_slot_0.png")
        );
        assert_eq!(
            paths.preview_for_map("fork"),
            PathBuf::from("/data/previews/fork_v2.png")
        );
        assert_eq!(
            paths.map_cache("fork"),
            PathBuf::from("/data/previews/fork-cache_v2.dat")
        );
        assert_eq!(
            paths.settings_file(),
            PathBuf::from("/data/config/settings.bin")
        );
    }
}
