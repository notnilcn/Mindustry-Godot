// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Save slots and listing (plan 04 §3.8).
//!
//! Ported from `core/src/mindustry/game/Saves.java` (`SaveSlot`: file + meta +
//! preview path + name/autosave via settings keys) and
//! `io/SavePreviewLoader.java` (delete-and-requeue resilience at the path
//! layer; pixel generation is plan 19, deviation 7). Autosave cadence, sector
//! remap and `Universe` playtime are plan 12 policy — this module is the
//! file/meta layer only.

use std::path::{Path, PathBuf};

use super::super::IoError;
use super::super::fs::{FileSystem, Paths, SAVE_EXTENSION};
use super::super::settings::{SettingsStore, slot_autosave_key, slot_name_key};
use super::{SaveIo, SaveMeta, SaveOptions, SaveReadState, WriteContext};

/// One save slot (`Saves.SaveSlot`).
#[derive(Debug, Clone)]
pub struct SaveSlot {
    /// The slot file (`saves/<n>.msav` or `saves/sector-<planet>-<id>.msav`).
    pub file: PathBuf,
    /// Meta read at load/list time (`None` until refreshed).
    pub meta: Option<SaveMeta>,
    /// Whether a preview re-render was requested (plan 19 renders).
    pub requested_preview: bool,
}

impl SaveSlot {
    /// A slot for one file.
    pub fn new(file: PathBuf) -> Self {
        Self {
            file,
            meta: None,
            requested_preview: false,
        }
    }

    /// The settings index (`SaveSlot.index`): the file stem.
    pub fn index(&self) -> String {
        self.file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Refreshes `meta` from disk (`SaveIO.getMeta` with backup fallback).
    pub fn load_meta(&mut self, fs: &dyn FileSystem) -> Result<(), IoError> {
        self.meta = Some(SaveIo::get_meta(fs, &self.file)?);
        Ok(())
    }

    /// Preview image path (`SaveSlot.previewFile`):
    /// `previews/save_slot_<index>.png`.
    pub fn preview_path(&self, paths: &Paths) -> PathBuf {
        paths.preview_for_save(&self.index())
    }

    /// Slot display name (`save-<index>-name`, default `untitled`).
    pub fn name(&self, settings: &SettingsStore) -> String {
        settings.get_string(&slot_name_key(&self.index()), "untitled")
    }

    /// Sets the display name.
    pub fn set_name(&self, settings: &mut SettingsStore, name: &str) {
        settings.put_string(&slot_name_key(&self.index()), name);
    }

    /// Whether the slot autosaves (`save-<index>-autosave`, default `true`).
    pub fn is_autosave(&self, settings: &SettingsStore) -> bool {
        settings.get_bool(&slot_autosave_key(&self.index()), true)
    }

    /// Sets the autosave flag.
    pub fn set_autosave(&self, settings: &mut SettingsStore, autosave: bool) {
        settings.put_bool(&slot_autosave_key(&self.index()), autosave);
    }

    /// Whether the slot is a sector save (`SaveSlot.isSector`): the loaded
    /// rules reference a sector. Uses the typed `Rules.sector` field, with a
    /// raw-JSON fallback for malformed/unknown shapes.
    pub fn is_sector(&self) -> bool {
        self.meta
            .as_ref()
            .map(|meta| rules_json_has_sector(&meta.rules_json))
            .unwrap_or(false)
    }

    /// `SaveSlot.isHidden` (sector saves are hidden from the save dialog).
    pub fn is_hidden(&self) -> bool {
        self.is_sector()
    }

    /// Wave number from meta.
    pub fn get_wave(&self) -> i32 {
        self.meta.as_ref().map(|meta| meta.wave).unwrap_or(0)
    }

    /// Build number from meta.
    pub fn get_build(&self) -> i32 {
        self.meta.as_ref().map(|meta| meta.build).unwrap_or(0)
    }

    /// `SaveSlot.hasExternalAssets` (`hasExternalAssets` tag).
    pub fn has_external_assets(&self) -> bool {
        self.meta
            .as_ref()
            .map(|meta| {
                meta.tags
                    .get("hasExternalAssets")
                    .is_some_and(|v| v == "true")
            })
            .unwrap_or(false)
    }

    /// Mod name list from meta.
    pub fn get_mods(&self) -> &[String] {
        self.meta
            .as_ref()
            .map(|meta| meta.mods.as_slice())
            .unwrap_or(&[])
    }

    /// `SaveSlot.save`: writes the save, refreshes meta and queues a preview
    /// re-render (plan 19 renders the pixels).
    pub fn save(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        ctx: &WriteContext,
        options: &SaveOptions,
    ) -> Result<(), IoError> {
        SaveIo::save(fs, &self.file, ctx, options)?;
        self.load_meta(fs)?;
        self.queue_preview_rerender(fs, paths);
        Ok(())
    }

    /// `SaveSlot.load`: loads the save through `SaveIo.load`.
    pub fn load(&mut self, fs: &dyn FileSystem, state: &mut SaveReadState) -> Result<(), IoError> {
        SaveIo::load(fs, &self.file, state)?;
        self.load_meta(fs)?;
        Ok(())
    }

    /// `SaveSlot.delete`: removes the file and its backup.
    pub fn delete(&mut self, fs: &dyn FileSystem) -> Result<(), IoError> {
        let backup = SaveIo::backup_file_for(&self.file);
        if fs.exists(&backup) {
            fs.delete(&backup)?;
        }
        fs.delete(&self.file)?;
        self.meta = None;
        Ok(())
    }

    /// `SaveSlot.importFile`: copies a save in, discarding any stale preview.
    pub fn import_file(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        from: &Path,
    ) -> Result<(), IoError> {
        fs.copy(from, &self.file)?;
        let preview = self.preview_path(paths);
        if fs.exists(&preview) {
            fs.delete(&preview)?;
            self.requested_preview = false;
        }
        self.load_meta(fs)?;
        Ok(())
    }

    /// `SaveSlot.exportFile`: copies the file out, or re-writes it with
    /// embedded assets when it is the live slot with external assets (the
    /// caller provides the write context in that case).
    pub fn export_file(
        &mut self,
        fs: &dyn FileSystem,
        to: &Path,
        embed_ctx: Option<(&WriteContext, SaveOptions)>,
    ) -> Result<(), IoError> {
        match embed_ctx {
            Some((ctx, mut options)) if self.has_external_assets() => {
                options.embed_assets = true;
                SaveIo::save(fs, to, ctx, &options)?;
            }
            _ => {
                fs.copy(&self.file, to)?;
            }
        }
        Ok(())
    }

    /// Preview resilience (`SavePreviewLoader`, path layer): a missing/stale
    /// preview is deleted and re-queued, never fatal.
    pub fn queue_preview_rerender(&mut self, fs: &dyn FileSystem, paths: &Paths) {
        let preview = self.preview_path(paths);
        if fs.exists(&preview) {
            // Stale previews are replaced; plan 19 writes the new pixels.
            let _ = fs.delete(&preview);
        }
        self.requested_preview = true;
    }
}

/// Whether the raw rules JSON references a sector (non-null `sector` field).
fn rules_json_has_sector(rules_json: &str) -> bool {
    // Typed parse first (`Rules.sector`), raw-JSON fallback for unknown shapes.
    if let Ok(rules) = crate::io::json::JsonIo::read::<crate::io::json::Rules>(rules_json) {
        return rules.sector.is_some();
    }
    serde_json::from_str::<serde_json::Value>(rules_json)
        .ok()
        .and_then(|value| value.get("sector").cloned())
        .is_some_and(|sector| !sector.is_null())
}

/// Meta for every readable save in one directory, read in parallel
/// (`Saves.load`: walk, skip `*backup*`, meta-read on the main executor).
///
/// Corrupt entries are skipped with a warning (both primary and backup
/// unreadable). Results are sorted by file name for deterministic output.
pub fn list_files_meta(fs: &dyn FileSystem, dir: &Path) -> Vec<(PathBuf, SaveMeta)> {
    let mut files: Vec<PathBuf> = fs
        .ls(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|path| {
            path.extension().and_then(|ext| ext.to_str()) == Some(SAVE_EXTENSION)
                && !path
                    .file_name()
                    .map(|name| name.to_string_lossy().contains("backup"))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    if files.is_empty() {
        // `chunks` panics on a zero chunk size; an empty saves dir has no slots.
        return Vec::new();
    }

    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(files.len().max(1));
    let chunk_size = files.len().div_ceil(threads);
    let mut results: Vec<(PathBuf, SaveMeta)> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = files
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .filter_map(|path| match SaveIo::get_meta(fs, path) {
                            Ok(meta) => Some((path.clone(), meta)),
                            Err(error) => {
                                log::warn!("skipping corrupt save `{}`: {error}", path.display());
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            results.extend(handle.join().unwrap_or_default());
        }
    });
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

/// Lists all valid save slots in `paths.saves()` with meta loaded
/// (`Saves.load` shape, plan 04 M5 §7a `slot_meta_listing_parallel`).
pub fn list_save_slots(fs: &dyn FileSystem, paths: &Paths) -> Vec<SaveSlot> {
    list_files_meta(fs, &paths.saves())
        .into_iter()
        .map(|(file, meta)| SaveSlot {
            file,
            meta: Some(meta),
            requested_preview: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::fs::MockFs;
    use crate::io::save::versions::v1::base_meta_tags;

    fn save_with_wave(fs: &MockFs, file: &Path, wave: i32, map: &str) {
        let mut tags = base_meta_tags(8, 8, wave, map);
        tags.insert("wave".to_owned(), wave.to_string());
        let ctx = WriteContext::meta_only(tags);
        SaveIo::save(fs, file, &ctx, &SaveOptions::new()).unwrap();
    }

    /// `io::save::slot::tests::slot_meta_listing_parallel` (plan 04 §7a):
    /// 100-slot listing, sorted, corrupt entries skipped.
    #[test]
    fn slot_meta_listing_parallel() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        for slot in 0..100 {
            save_with_wave(&fs, &paths.save_slot(slot), slot as i32, "map");
        }
        // One corrupt file (no backup) and a stray non-save file.
        fs.write(&paths.saves().join("broken.msav"), b"garbage")
            .unwrap();
        fs.write(&paths.saves().join("notes.txt"), b"hi").unwrap();

        let slots = list_save_slots(&fs, &paths);
        assert_eq!(slots.len(), 100);
        // Every slot's meta is readable; waves cover the full 0..100 set.
        let mut waves: Vec<i32> = slots.iter().map(SaveSlot::get_wave).collect();
        waves.sort_unstable();
        assert_eq!(waves, (0..100).collect::<Vec<_>>());
        let names: Vec<String> = slots.iter().map(|slot| slot.index()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    /// `io::save::slot::tests::empty_dir_lists_no_slots`: a saves directory with
    /// no `.msav` files (fresh install) lists zero slots instead of panicking.
    #[test]
    fn empty_dir_lists_no_slots() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        assert!(list_save_slots(&fs, &paths).is_empty());
        // A stray non-save file is not a slot either.
        fs.write(&paths.saves().join("notes.txt"), b"hi").unwrap();
        assert!(list_save_slots(&fs, &paths).is_empty());
    }

    /// `io::save::slot::tests::backup_fallback` (plan 04 §7a): a corrupt
    /// primary falls back to the backup for meta and full loads.
    #[test]
    fn backup_fallback() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let file = paths.save_slot(0);
        save_with_wave(&fs, &file, 1, "first");
        save_with_wave(&fs, &file, 2, "second");
        assert!(fs.exists(&SaveIo::backup_file_for(&file)));

        // Corrupt the primary; slot load_meta must fall back to the backup.
        fs.write(&file, b"garbage").unwrap();
        let mut slot = SaveSlot::new(file);
        slot.load_meta(&fs).unwrap();
        assert_eq!(slot.get_wave(), 1);
    }

    /// `io::save::slot::tests::preview_paths` (plan 04 §7a) + name/autosave
    /// settings keys + import/export/delete.
    #[test]
    fn preview_paths_and_slot_kv() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let mut settings = SettingsStore::new();

        let file = paths.save_slot(3);
        save_with_wave(&fs, &file, 5, "map");
        let mut slot = SaveSlot::new(file.clone());
        slot.load_meta(&fs).unwrap();

        assert_eq!(
            slot.preview_path(&paths),
            PathBuf::from("/data/previews/save_slot_3.png")
        );
        assert_eq!(slot.name(&settings), "untitled");
        slot.set_name(&mut settings, "base");
        assert_eq!(slot.name(&settings), "base");
        assert!(slot.is_autosave(&settings));
        slot.set_autosave(&mut settings, false);
        assert!(!slot.is_autosave(&settings));
        // The upstream key names are the parity ABI.
        assert!(settings.has("save-3-name"));
        assert!(settings.has("save-3-autosave"));

        // Preview re-render queue deletes stale previews.
        fs.write(&slot.preview_path(&paths), b"png").unwrap();
        slot.queue_preview_rerender(&fs, &paths);
        assert!(slot.requested_preview);
        assert!(!fs.exists(&slot.preview_path(&paths)));

        // Export copies; import replaces + refreshes meta.
        let export = PathBuf::from("/tmp/export.msav");
        slot.export_file(&fs, &export, None).unwrap();
        let other = paths.save_slot(4);
        save_with_wave(&fs, &other, 9, "other");
        let mut imported = SaveSlot::new(other.clone());
        imported.import_file(&fs, &paths, &export).unwrap();
        assert_eq!(imported.get_wave(), 5);

        // Delete removes file + backup.
        save_with_wave(&fs, &file, 6, "third");
        assert!(fs.exists(&SaveIo::backup_file_for(&file)));
        slot.delete(&fs).unwrap();
        assert!(!fs.exists(&file));
        assert!(!fs.exists(&SaveIo::backup_file_for(&file)));
    }

    #[test]
    fn sector_detection_from_rules_json() {
        assert!(!rules_json_has_sector("{}"));
        assert!(rules_json_has_sector("{\"sector\":\"serpulo-12\"}"));
        assert!(!rules_json_has_sector("{\"sector\":null}"));
        assert!(!rules_json_has_sector("not json"));
    }
}
