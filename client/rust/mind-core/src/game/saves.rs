// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Save policy on top of plan 04's `SaveSlot` (plan 12 M4).
//!
//! Ported from `core/src/mindustry/game/Saves.java`. Plan 04 owns the file
//! format, meta and per-slot primitives ([`crate::io::save::slot`]); this module
//! owns the *policy*: load/listing, autosave cadence, playtime, sector saves,
//! legacy megabase cleanup, the two-phase sector remap and mod checks.
//!
//! Deviation 11 (HLP §2.4): the autosave/playtime clock is tick-driven
//! (`delta_ticks`), not `Time.delta`/`Time.millis`; the same wall-clock interval
//! results in practice but stays deterministic and replayable.

use std::path::{Path, PathBuf};

use super::State;
use crate::content::{ContentRegistry, PlanetId};
use crate::io::IoError;
use crate::io::fs::{FileSystem, Paths, SAVE_EXTENSION};
use crate::io::json::content_serde::SectorKey;
use crate::io::save::slot::{SaveSlot, list_save_slots};
use crate::io::save::version::WriteContext;
use crate::io::save::{SaveOptions, SaveReadState};
use crate::io::settings::{KEY_LAST_SECTOR_SAVE, KEY_SAVE_INTERVAL, SettingsStore};

/// `Vars.saveInterval` default (minutes) when the setting is absent.
pub const DEFAULT_SAVE_INTERVAL_MINUTES: i32 = 10;

/// Legacy beta megabase Serpulo sectors removed by `Saves.clearOldMegabaseSectors`.
pub const OLD_MEGABASE_SECTORS: [u16; 13] = [
    27, 245, 244, 243, 242, 247, 246, 237, 150, 157, 138, 251, 103,
];

/// Inputs for [`Saves::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavesUpdate {
    /// Fixed ticks elapsed since the last update.
    pub delta_ticks: u64,
    /// `state.isGame()`.
    pub is_game: bool,
    /// A dialog is open while paused (suspends playtime).
    pub paused_with_dialog: bool,
    /// `state.gameOver`.
    pub game_over: bool,
    /// `state.isCampaign()` (adds to the planet's playtime).
    pub is_campaign: bool,
}

/// Outcome of [`Saves::update`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SavesUpdateResult {
    /// An autosave was written this update.
    pub autosaved: bool,
    /// Milliseconds of playtime added this update.
    pub playtime_added: i64,
}

/// Missing-mod report from `cautious_load` (`Saves.load` mod validation).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MissingModsReport {
    /// Mods required by the save, in meta order.
    pub required: Vec<String>,
    /// Required mods that are not currently loaded.
    pub missing: Vec<String>,
}

impl MissingModsReport {
    /// Whether every required mod is loaded.
    pub fn is_empty(&self) -> bool {
        self.missing.is_empty()
    }
}

/// Save-slot policy resource (`Saves`).
#[derive(Debug, Clone, Default)]
pub struct Saves {
    /// All known save slots, sorted by file path after [`Saves::load`].
    pub slots: Vec<SaveSlot>,
    /// Index of the current slot in [`Saves::slots`].
    pub current: Option<usize>,
    /// File of the most recently saved sector (`last-sector-save`).
    pub last_sector_save: Option<PathBuf>,
    /// A save is in flight (upstream clears it 3 ticks later).
    pub saving: bool,
    /// Autosave timer in seconds.
    pub time: f32,
    /// Accumulated playtime in milliseconds.
    pub total_playtime: i64,
    /// Tick timestamp of the last playtime accrual; `None` when unset.
    pub last_timestamp: Option<u64>,
    /// A Serpulo sector was remapped during [`Saves::load`] (`Vars.hadSerpuloRemaps`).
    pub had_serpulo_remaps: bool,
}

impl Saves {
    /// Empty policy (`Saves()` without `load`).
    pub fn new() -> Self {
        Self::default()
    }

    /// `Saves.load`: list slots (parallel meta reads), clean legacy megabase
    /// saves, then assign/remap sector slots.
    pub fn load(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        registry: &ContentRegistry,
        settings: &mut SettingsStore,
    ) {
        self.slots = list_save_slots(fs, paths);
        self.clear_old_megabase_sectors(fs, registry);

        let last_name = settings.get_string(KEY_LAST_SECTOR_SAVE, "<none>");
        self.last_sector_save = self
            .slots
            .iter()
            .find(|slot| sector_key_of(slot).is_some() && slot.name(settings) == last_name)
            .map(|slot| slot.file.clone());

        self.assign_sector_saves(fs, paths, registry, settings);
    }

    /// `Saves.clearOldMegabaseSectors`.
    fn clear_old_megabase_sectors(&mut self, fs: &dyn FileSystem, registry: &ContentRegistry) {
        let serpulo = registry.planet_id("serpulo");
        self.slots.retain(|slot| {
            let Some((planet, sector)) = sector_of_slot(registry, slot) else {
                return true;
            };
            if Some(planet) != serpulo || !OLD_MEGABASE_SECTORS.contains(&sector) {
                return true;
            }
            let build = slot.meta.as_ref().map(|meta| meta.build).unwrap_or(0);
            if (147..157).contains(&build) {
                let _ = fs.delete(&slot.file);
                false
            } else {
                true
            }
        });
    }

    /// `Saves.load` sector assignment + two-phase remap.
    fn assign_sector_saves(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        registry: &ContentRegistry,
        settings: &mut SettingsStore,
    ) {
        // Phase 1: decide remaps and move source files to a temp sibling.
        let mut pending: Vec<(usize, u16, PathBuf)> = Vec::new();
        for index in 0..self.slots.len() {
            let Some((planet, sector)) = sector_of_slot(registry, &self.slots[index]) else {
                continue;
            };
            let preset_tag = self.slots[index]
                .meta
                .as_ref()
                .and_then(|meta| meta.tags.get("sectorPreset").cloned());
            let Some(target) = plan_remap(registry, planet, sector, preset_tag.as_deref()) else {
                continue;
            };
            let planet_name = registry
                .planet(planet)
                .map(|record| record.name.clone())
                .unwrap_or_default();
            let dest_file = paths.sector_save(&planet_name, target as u32);
            if self.slots[index].file == dest_file {
                continue;
            }
            let tmp = paths
                .saves()
                .join(format!("remap_{planet_name}_{sector}.{SAVE_EXTENSION}"));
            if fs.rename(&self.slots[index].file, &tmp).is_ok() {
                self.slots[index].file = tmp.clone();
                pending.push((index, target, dest_file));
            }
        }

        // Phase 2: move the files into their destinations and migrate info.
        for (index, target, dest_file) in pending {
            let Some((planet, source_sector)) = sector_of_slot(registry, &self.slots[index]) else {
                continue;
            };
            let planet_name = registry
                .planet(planet)
                .map(|record| record.name.clone())
                .unwrap_or_default();
            if planet_name == "serpulo" {
                self.had_serpulo_remaps = true;
            }
            let source_key = format!("{planet_name}-s-{source_sector}-info");
            let dest_key = format!("{planet_name}-s-{target}-info");
            let info =
                settings.get_json_or(&source_key, crate::io::json::rules::SectorInfo::default);
            let _ = settings.put_json(&dest_key, &info);
            let tmp = self.slots[index].file.clone();
            if fs.rename(&tmp, &dest_file).is_ok() {
                self.slots[index].file = dest_file;
            }
        }
    }

    /// `Events.on(StateChangeEvent)`: entering the menu clears current/playtime.
    pub fn on_state_change(&mut self, to: State) {
        if to == State::Menu {
            self.total_playtime = 0;
            self.last_timestamp = None;
            self.current = None;
        }
    }

    /// `Saves.update`. Returns the outcome; `save_current` must be a live-game
    /// write context (node-06 world; `mind-core` never touches the world here).
    pub fn update(
        &mut self,
        update: SavesUpdate,
        settings: &SettingsStore,
        fs: &dyn FileSystem,
        paths: &Paths,
        ctx: &WriteContext,
        options: &SaveOptions,
    ) -> SavesUpdateResult {
        let mut result = SavesUpdateResult::default();
        let playing = update.is_game && !(update.paused_with_dialog);
        if playing && self.current.is_some() {
            if let Some(last) = self.last_timestamp {
                result.playtime_added = update.delta_ticks as i64 * 1000 / 60;
                self.total_playtime += result.playtime_added;
                let _ = last;
            }
            self.last_timestamp = Some(update.delta_ticks);
        }

        let autosave = update.is_game
            && !update.game_over
            && self.current.is_some()
            && self
                .current_slot()
                .is_some_and(|slot| slot.is_autosave(settings));
        if autosave {
            self.time += update.delta_ticks as f32 / 60.0;
            let interval = settings
                .get_i32(KEY_SAVE_INTERVAL, DEFAULT_SAVE_INTERVAL_MINUTES)
                .max(0) as f32
                * 60.0;
            if self.time > interval {
                if let Some(index) = self.current {
                    self.saving = true;
                    if let Some(slot) = self.slots.get_mut(index)
                        && slot.save(fs, paths, ctx, options).is_ok()
                    {
                        result.autosaved = true;
                    }
                }
                self.time = 0.0;
            }
        } else {
            self.time = 0.0;
        }
        result
    }

    /// Clears the in-flight saving flag (upstream clears it after 3 ticks).
    pub fn finish_saving(&mut self) {
        self.saving = false;
    }

    /// `Saves.getTotalPlaytime`.
    pub fn get_total_playtime(&self) -> i64 {
        self.total_playtime
    }

    /// `Saves.resetSave`.
    pub fn reset_save(&mut self) {
        self.current = None;
    }

    /// `Saves.isSaving`.
    pub fn is_saving(&self) -> bool {
        self.saving
    }

    /// Current slot, if any.
    pub fn current_slot(&self) -> Option<&SaveSlot> {
        self.current.and_then(|index| self.slots.get(index))
    }

    /// `Saves.getCurrent`.
    pub fn get_current(&self) -> Option<&SaveSlot> {
        self.current_slot()
    }

    /// `Saves.getLastSector`.
    pub fn get_last_sector(&self) -> Option<&SaveSlot> {
        self.last_sector_save
            .as_ref()
            .and_then(|file| self.slots.iter().find(|slot| &slot.file == file))
    }

    /// `Saves.getSectorFile`.
    pub fn get_sector_file(&self, paths: &Paths, planet: &str, id: u16) -> PathBuf {
        paths.sector_save(planet, id as u32)
    }

    /// `Saves.saveSector`: creates/updates the sector slot and writes it.
    #[allow(clippy::too_many_arguments)]
    pub fn save_sector(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        settings: &mut SettingsStore,
        planet: &str,
        id: u16,
        ctx: &WriteContext,
        options: &SaveOptions,
    ) -> Result<usize, IoError> {
        let file = self.get_sector_file(paths, planet, id);
        let index = match self.slots.iter().position(|slot| slot.file == file) {
            Some(index) => index,
            None => {
                let slot = SaveSlot::new(file.clone());
                slot.set_name(settings, &slot.index());
                let index = self.slots.len();
                self.slots.push(slot);
                index
            }
        };
        self.slots[index].set_autosave(settings, true);
        self.slots[index].save(fs, paths, ctx, options)?;
        self.last_sector_save = Some(file);
        settings.put_string(KEY_LAST_SECTOR_SAVE, &self.slots[index].name(settings));
        Ok(index)
    }

    /// `Saves.addSave`.
    pub fn add_save(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        settings: &mut SettingsStore,
        name: &str,
        ctx: &WriteContext,
        options: &SaveOptions,
    ) -> Result<usize, IoError> {
        let file = get_next_slot_file(fs, paths);
        let slot = SaveSlot::new(file);
        slot.set_name(settings, name);
        let index = self.slots.len();
        self.slots.push(slot);
        self.slots[index].save(fs, paths, ctx, options)?;
        Ok(index)
    }

    /// `Saves.importSave`.
    pub fn import_save(
        &mut self,
        fs: &dyn FileSystem,
        paths: &Paths,
        settings: &mut SettingsStore,
        from: &Path,
    ) -> Result<usize, IoError> {
        let file = get_next_slot_file(fs, paths);
        let mut slot = SaveSlot::new(file);
        slot.import_file(fs, paths, from)?;
        let name = from
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| slot.index());
        slot.set_name(settings, &name);
        let index = self.slots.len();
        self.slots.push(slot);
        self.current = Some(index);
        Ok(index)
    }

    /// `Saves.getSaveSlots`.
    pub fn get_save_slots(&self) -> &[SaveSlot] {
        &self.slots
    }

    /// `Saves.deleteAll`: removes every non-sector save.
    pub fn delete_all(&mut self, fs: &dyn FileSystem) -> Result<(), IoError> {
        self.slots.retain_mut(|slot| {
            if slot.is_sector() {
                true
            } else {
                let _ = slot.delete(fs);
                false
            }
        });
        Ok(())
    }

    /// Mod validation for one slot (`cautious_load` on the slot meta): the
    /// caller passes the currently loaded mod names.
    pub fn cautious_load(slot: &SaveSlot, loaded_mods: &[String]) -> MissingModsReport {
        let required = slot.get_mods().to_vec();
        let missing = required
            .iter()
            .filter(|name| !loaded_mods.contains(name))
            .cloned()
            .collect();
        MissingModsReport { required, missing }
    }
}

/// `Saves.getNextSlotFile`: `saves/<n>.msav` for the first free `n`.
pub fn get_next_slot_file(fs: &dyn FileSystem, paths: &Paths) -> PathBuf {
    let mut index = 0u32;
    loop {
        let file = paths.save_slot(index);
        if !fs.exists(&file) {
            return file;
        }
        index += 1;
    }
}

/// Parses `(planet, sector)` from a slot's rules JSON (`SaveSlot.getSector`).
pub fn sector_of_slot(registry: &ContentRegistry, slot: &SaveSlot) -> Option<(PlanetId, u16)> {
    let meta = slot.meta.as_ref()?;
    let rules: crate::io::json::Rules = serde_json::from_str(&meta.rules_json).ok()?;
    rules.sector?.resolve(registry)
}

/// Whether a slot's rules reference a sector at all.
fn sector_key_of(slot: &SaveSlot) -> Option<SectorKey> {
    let meta = slot.meta.as_ref()?;
    let rules: crate::io::json::Rules = serde_json::from_str(&meta.rules_json).ok()?;
    rules.sector
}

/// `Saves.load` remap decision (pure): returns the target sector index when the
/// save must be moved, `None` when it is already in place.
///
/// Ported from the two branches in `Saves.load`: an explicit `sectorPreset`
/// tag relocates to `preset.sector`; a legacy save with no tag relocates to the
/// preset whose `originalPosition` matches the saved index.
pub fn plan_remap(
    registry: &ContentRegistry,
    planet: PlanetId,
    saved_sector: u16,
    preset_tag: Option<&str>,
) -> Option<u16> {
    match preset_tag {
        Some(name) if !name.is_empty() => {
            let preset = registry.sector_by_name(name)?;
            if preset.planet == planet && preset.sector != saved_sector && preset.require_unlock {
                Some(preset.sector)
            } else {
                None
            }
        }
        Some(_) => None,
        None => {
            let preset = registry.sectors().iter().find(|preset| {
                preset.planet == planet && preset.original_position == saved_sector as i32
            })?;
            if preset.sector != saved_sector && preset.require_unlock {
                Some(preset.sector)
            } else {
                None
            }
        }
    }
}

/// `Saves.load` uses a `WorldContext` load hook; kept here so callers can load
/// a slot's world through the same policy object.
pub fn load_slot(
    slot: &mut SaveSlot,
    fs: &dyn FileSystem,
    state: &mut SaveReadState,
) -> Result<(), IoError> {
    slot.load(fs, state)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::field_reassign_with_default)]

    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::io::fs::MockFs;
    use crate::io::save::versions::v1::base_meta_tags;
    use crate::io::{StringMap, WireReader, WireWriter};

    fn registry() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true).unwrap()
    }

    fn meta_ctx(rules_json: &str, extra: &[(&str, &str)]) -> WriteContext<'static> {
        let mut tags: StringMap = base_meta_tags(8, 8, 2, "sector-map");
        tags.insert("rules".to_owned(), rules_json.to_owned());
        for (key, value) in extra {
            tags.insert((*key).to_owned(), (*value).to_owned());
        }
        WriteContext::meta_only(tags)
    }

    #[test]
    fn get_next_slot_file_skips_used() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        assert_eq!(get_next_slot_file(&fs, &paths), paths.save_slot(0));
        fs.write(&paths.save_slot(0), b"x").unwrap();
        fs.write(&paths.save_slot(1), b"x").unwrap();
        assert_eq!(get_next_slot_file(&fs, &paths), paths.save_slot(2));
    }

    #[test]
    fn save_sector_names_and_records_settings() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();
        let ctx = meta_ctx(r#"{"sector":"serpulo-15"}"#, &[]);
        let index = saves
            .save_sector(
                &fs,
                &paths,
                &mut settings,
                "serpulo",
                15,
                &ctx,
                &SaveOptions::new(),
            )
            .unwrap();
        let file = paths.sector_save("serpulo", 15);
        assert_eq!(saves.slots[index].file, file);
        assert!(fs.exists(&file));
        assert_eq!(
            settings.get_string(KEY_LAST_SECTOR_SAVE, "<none>"),
            "sector-serpulo-15"
        );
        assert_eq!(saves.get_last_sector().map(|slot| &slot.file), Some(&file));
    }

    #[test]
    fn load_lists_and_detects_sector_saves() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let registry = registry();
        let mut settings = SettingsStore::new();
        // One numbered slot, one sector slot.
        let ctx = WriteContext::meta_only(base_meta_tags(8, 8, 1, "normal"));
        SaveSlot::new(paths.save_slot(0))
            .save(&fs, &paths, &ctx, &SaveOptions::new())
            .unwrap();
        let sector_ctx = meta_ctx(r#"{"sector":"serpulo-15"}"#, &[]);
        SaveSlot::new(paths.sector_save("serpulo", 15))
            .save(&fs, &paths, &sector_ctx, &SaveOptions::new())
            .unwrap();

        let mut saves = Saves::new();
        saves.load(&fs, &paths, &registry, &mut settings);
        assert_eq!(saves.slots.len(), 2);
        let sectors = saves.slots.iter().filter(|slot| slot.is_sector()).count();
        assert_eq!(sectors, 1);
        assert_eq!(
            sector_of_slot(
                &registry,
                saves.slots.iter().find(|s| s.is_sector()).unwrap()
            )
            .map(|(_, id)| id),
            Some(15)
        );
    }

    #[test]
    fn autosave_policy_fires_after_interval() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let mut settings = SettingsStore::new();
        settings.put_i32(KEY_SAVE_INTERVAL, 1);
        let mut saves = Saves::new();
        let ctx = WriteContext::meta_only(base_meta_tags(8, 8, 1, "auto"));
        let index = saves
            .add_save(
                &fs,
                &paths,
                &mut settings,
                "auto",
                &ctx,
                &SaveOptions::new(),
            )
            .unwrap();
        saves.current = Some(index);
        saves.slots[index].set_autosave(&mut settings, true);

        let update = SavesUpdate {
            delta_ticks: 30 * 60,
            is_game: true,
            paused_with_dialog: false,
            game_over: false,
            is_campaign: false,
        };
        // 1-minute interval; 30 s + 30 s + 1 s crosses it on the third update.
        let first = saves.update(update, &settings, &fs, &paths, &ctx, &SaveOptions::new());
        assert!(!first.autosaved);
        let second = saves.update(update, &settings, &fs, &paths, &ctx, &SaveOptions::new());
        assert!(!second.autosaved);
        let third = saves.update(
            SavesUpdate {
                delta_ticks: 61,
                ..update
            },
            &settings,
            &fs,
            &paths,
            &ctx,
            &SaveOptions::new(),
        );
        assert!(third.autosaved);
        assert!(saves.is_saving());
        saves.finish_saving();
        assert!(!saves.is_saving());
    }

    #[test]
    fn state_change_to_menu_clears_progress() {
        let mut saves = Saves::new();
        saves.current = Some(0);
        saves.total_playtime = 999;
        saves.on_state_change(State::Menu);
        assert_eq!(saves.current, None);
        assert_eq!(saves.get_total_playtime(), 0);
    }

    #[test]
    fn cautious_load_reports_missing_mods() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        // A slot with a mods tag.
        let mut tags = base_meta_tags(8, 8, 1, "moddy");
        tags.insert("mods".to_owned(), "[\"foo\",\"bar\"]".to_owned());
        let ctx = WriteContext::meta_only(tags);
        let file = paths.save_slot(0);
        SaveSlot::new(file.clone())
            .save(&fs, &paths, &ctx, &SaveOptions::new())
            .unwrap();
        let mut slot = SaveSlot::new(file);
        slot.load_meta(&fs).unwrap();
        let report = Saves::cautious_load(&slot, &["foo".to_owned()]);
        assert_eq!(report.required, vec!["foo", "bar"]);
        assert_eq!(report.missing, vec!["bar"]);
        assert!(!report.is_empty());
    }

    #[test]
    fn plan_remap_explicit_preset_and_legacy() {
        let registry = registry();
        let serpulo = registry.planet_id("serpulo").unwrap();
        // Explicit preset tag: a save at sector 16 that belongs to groundZero
        // (resolved sector 15) is remapped to 15.
        assert_eq!(
            plan_remap(&registry, serpulo, 16, Some("groundZero")),
            Some(15)
        );
        // Already at the destination -> no remap.
        assert_eq!(plan_remap(&registry, serpulo, 15, Some("groundZero")), None);
        // Unknown preset tag -> no remap.
        assert_eq!(plan_remap(&registry, serpulo, 15, Some("nope")), None);
        // Legacy (no tag) with no preset authored at the saved index -> none.
        assert_eq!(plan_remap(&registry, serpulo, 9999, None), None);
    }

    #[test]
    fn import_and_delete_all() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();

        // Create a numbered save to import and a sector save to keep.
        let ctx = WriteContext::meta_only(base_meta_tags(8, 8, 1, "importable"));
        let external = PathBuf::from("/tmp/importable.msav");
        SaveSlot::new(external.clone())
            .save(&fs, &paths, &ctx, &SaveOptions::new())
            .unwrap();
        let sector_ctx = meta_ctx(r#"{"sector":"serpulo-15"}"#, &[]);
        saves
            .save_sector(
                &fs,
                &paths,
                &mut settings,
                "serpulo",
                15,
                &sector_ctx,
                &SaveOptions::new(),
            )
            .unwrap();

        let index = saves
            .import_save(&fs, &paths, &mut settings, &external)
            .unwrap();
        assert_eq!(saves.current, Some(index));
        assert_eq!(saves.slots.len(), 2);

        saves.delete_all(&fs).unwrap();
        assert_eq!(saves.slots.len(), 1, "sector saves are kept");
        assert!(saves.slots[0].is_sector());
    }

    #[test]
    fn typeio_roundtrip_keeps_writer_unused() {
        // Guard against accidental signature drift in the io boundary.
        let mut buf = Vec::new();
        {
            let mut writer = WireWriter::new(&mut buf);
            writer.bool(true);
        }
        let mut reader = WireReader::new(&buf);
        assert!(reader.bool().unwrap());
    }
}
