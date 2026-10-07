// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Data export/import archive (plan 14 §3.4).
//!
//! Ported from `core/src/mindustry/ui/dialogs/SettingsMenuDialog.java`
//! (`exportData`, `importData`) and the `FileChooser.export`/`open("zip")`
//! flow. The archive holds `settings.bin` plus the `saves/`, `maps/`, `mods/`,
//! `schematics/` and `cache/` trees; import requires `settings.bin`
//! (`importData`'s "Not valid save data." check), replaces the save/cache/temp
//! trees and extracts every entry.
//!
//! The port keeps the UI settings store under the data root while the campaign
//! store lives at `<user_root>/config/settings.bin`; both are written to and
//! read from the archive.

use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

use super::error::IoError;
use super::fs::FileSystem;

/// Directory trees copied into the archive from the user data root, in
/// upstream `exportData` order (`customMapDirectory`, `saveDirectory`,
/// `modDirectory`, `schematicDirectory`, `assetCacheDirectory`).
pub const EXPORT_DIRS: [&str; 5] = ["maps", "saves", "mods", "schematics", "cache"];

/// Trees replaced by `importData` before extraction (`saveDirectory`,
/// `assetCacheDirectory`, `tmpDirectory`).
const IMPORT_CLEAR_DIRS: [&str; 3] = ["saves", "cache", "tmp"];

/// Per-entry extraction cap (zip-bomb guard for untrusted archives).
pub const MAX_ENTRY_BYTES: u64 = 128 * 1024 * 1024;

/// One exported archive report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ArchiveReport {
    /// Files written into the archive.
    pub files: usize,
    /// Bytes written (approximate: the uncompressed payload sum).
    pub bytes: usize,
}

impl ArchiveReport {
    /// The archive holds no files (import would key off `settings.bin`).
    pub fn is_empty(&self) -> bool {
        self.files == 0
    }
}

/// `SettingsMenuDialog.exportData`: writes one zip with `settings.bin` plus the
/// game data trees under `user_root` to `out`.
pub fn export_data(
    fs: &dyn FileSystem,
    out: &Path,
    settings_file: &Path,
    user_root: &Path,
) -> Result<ArchiveReport, IoError> {
    let mut report = ArchiveReport::default();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::<u8>::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    if fs.is_file(settings_file) {
        add_file(
            fs,
            &mut writer,
            options,
            settings_file,
            Path::new("settings.bin"),
            &mut report,
        )?;
    }
    let campaign_settings = user_root.join("config").join("settings.bin");
    if fs.is_file(&campaign_settings) {
        add_file(
            fs,
            &mut writer,
            options,
            &campaign_settings,
            Path::new("config").join("settings.bin").as_path(),
            &mut report,
        )?;
    }
    for dir in EXPORT_DIRS {
        let source = user_root.join(dir);
        for file in fs.walk(&source)? {
            let Ok(relative) = file.strip_prefix(user_root) else {
                continue;
            };
            add_file(fs, &mut writer, options, &file, relative, &mut report)?;
        }
    }

    let cursor = writer.finish().map_err(zip_error)?;
    let bytes = cursor.into_inner();
    fs.write(out, &bytes)?;
    report.bytes = bytes.len();
    Ok(report)
}

/// `SettingsMenuDialog.importData`: replaces the save/cache/temp trees and
/// extracts every archive entry. `settings.bin` is required.
pub fn import_data(
    fs: &dyn FileSystem,
    archive: &Path,
    settings_file: &Path,
    user_root: &Path,
) -> Result<ArchiveReport, IoError> {
    let bytes = fs.read(archive)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    if zip.by_name("settings.bin").is_err() {
        return Err(IoError::corrupt("Not valid save data."));
    }

    // Upstream clears old saves, the asset cache and tmp data before merging
    // the archive over the remaining trees.
    for dir in IMPORT_CLEAR_DIRS {
        delete_files(fs, &user_root.join(dir))?;
    }

    let mut report = ArchiveReport::default();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(zip_error)?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(IoError::corrupt("archive entry escapes the data root"));
        };
        if entry.is_dir() {
            continue;
        }
        if entry.size() > MAX_ENTRY_BYTES {
            return Err(IoError::corrupt(format!(
                "archive entry `{}` exceeds the {MAX_ENTRY_BYTES} byte cap",
                relative.display()
            )));
        }
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data).map_err(IoError::Io)?;
        let target: PathBuf = if relative == Path::new("settings.bin") {
            settings_file.to_path_buf()
        } else {
            user_root.join(&relative)
        };
        fs.write(&target, &data)?;
        report.files += 1;
        report.bytes += data.len();
    }
    Ok(report)
}

/// Removes every file below `dir` (the `FileSystem` seam has no directory
/// removal; extraction recreates the tree).
pub fn delete_files(fs: &dyn FileSystem, dir: &Path) -> Result<usize, IoError> {
    let mut removed = 0usize;
    for file in fs.walk(dir)? {
        if fs.delete(&file).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Reads one source file and appends it as an archive entry.
fn add_file<W: Write + std::io::Seek>(
    fs: &dyn FileSystem,
    writer: &mut zip::ZipWriter<W>,
    options: zip::write::SimpleFileOptions,
    source: &Path,
    name: &Path,
    report: &mut ArchiveReport,
) -> Result<(), IoError> {
    let data = fs.read(source)?;
    writer
        .start_file(archive_name(name), options)
        .map_err(zip_error)?;
    writer.write_all(&data).map_err(IoError::Io)?;
    report.files += 1;
    report.bytes += data.len();
    Ok(())
}

/// Archive entry names use forward slashes regardless of the host separator.
fn archive_name(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Wraps a `ZipError` as corrupt untrusted data.
fn zip_error(error: zip::result::ZipError) -> IoError {
    IoError::corrupt(format!("invalid data archive: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;

    fn layout(fs: &MockFs) -> (PathBuf, PathBuf) {
        let root = PathBuf::from("/data");
        let user = PathBuf::from("/user");
        fs.write(&root.join("config").join("settings.bin"), b"ui-settings")
            .unwrap();
        fs.write(
            &user.join("config").join("settings.bin"),
            b"campaign-settings",
        )
        .unwrap();
        fs.write(&user.join("saves").join("sector-serpulo-15.msav"), b"save")
            .unwrap();
        fs.write(&user.join("maps").join("my-map.msav"), b"map")
            .unwrap();
        fs.write(&user.join("mods").join("mod.zip"), b"mod")
            .unwrap();
        (root, user)
    }

    #[test]
    fn export_import_round_trip() {
        let fs = MockFs::new();
        let (root, user) = layout(&fs);
        let archive = Path::new("/out/mindustry-data-export.zip");
        let report = export_data(
            &fs,
            archive,
            &root.join("config").join("settings.bin"),
            &user,
        )
        .unwrap();
        assert!(report.files >= 5, "settings + tree files archived");
        assert!(fs.exists(archive));

        // A fresh target root receives the archive contents.
        let fresh = MockFs::new();
        fresh.write(archive, &fs.read(archive).unwrap()).unwrap();
        let fresh_root = PathBuf::from("/fresh");
        let fresh_user = PathBuf::from("/fresh-user");
        let imported = import_data(
            &fresh,
            archive,
            &fresh_root.join("config").join("settings.bin"),
            &fresh_user,
        )
        .unwrap();
        assert_eq!(imported.files, report.files);
        assert_eq!(
            fresh
                .read(&fresh_root.join("config").join("settings.bin"))
                .unwrap(),
            b"ui-settings"
        );
        assert_eq!(
            fresh
                .read(&fresh_user.join("config").join("settings.bin"))
                .unwrap(),
            b"campaign-settings"
        );
        assert_eq!(
            fresh
                .read(&fresh_user.join("saves").join("sector-serpulo-15.msav"))
                .unwrap(),
            b"save"
        );
        assert_eq!(
            fresh
                .read(&fresh_user.join("maps").join("my-map.msav"))
                .unwrap(),
            b"map"
        );
    }

    #[test]
    fn import_requires_settings_entry() {
        let fs = MockFs::new();
        let bad = Path::new("/bad.zip");
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::<u8>::new()));
        writer
            .start_file("saves/0.msav", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"save").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        fs.write(bad, &bytes).unwrap();

        let error = import_data(
            &fs,
            bad,
            Path::new("/data/settings.bin"),
            Path::new("/user"),
        )
        .expect_err("missing settings.bin is invalid");
        assert!(error.to_string().contains("Not valid save data."));
    }

    #[test]
    fn import_replaces_saves_and_cache_but_merges_maps() {
        let fs = MockFs::new();
        let (root, user) = layout(&fs);
        let archive = Path::new("/out/export.zip");
        export_data(
            &fs,
            archive,
            &root.join("config").join("settings.bin"),
            &user,
        )
        .unwrap();

        // Pre-existing data: saves/cache are cleared, maps are kept.
        fs.write(&user.join("saves").join("0.msav"), b"old-save")
            .unwrap();
        fs.write(&user.join("cache").join("old.ogg"), b"old-cache")
            .unwrap();
        fs.write(&user.join("maps").join("kept.msav"), b"kept-map")
            .unwrap();

        let imported = import_data(
            &fs,
            archive,
            &root.join("config").join("settings.bin"),
            &user,
        )
        .unwrap();
        assert!(imported.files > 0);
        assert!(
            !fs.exists(&user.join("saves").join("0.msav")),
            "old saves are cleared"
        );
        assert!(
            !fs.exists(&user.join("cache").join("old.ogg")),
            "old cache is cleared"
        );
        assert!(
            fs.exists(&user.join("maps").join("kept.msav")),
            "maps not in the archive are kept (upstream merge)"
        );
    }
}
