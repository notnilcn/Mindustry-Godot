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
use crate::game::universe::Campaign;
use crate::io::IoError;
use crate::io::fs::{FileSystem, Paths, SAVE_EXTENSION};
use crate::io::json::content_serde::SectorKey;
use crate::io::save::slot::{SaveSlot, list_save_slots};
use crate::io::save::version::WriteContext;
use crate::io::save::{SaveOptions, SaveReadState};
use crate::io::settings::{KEY_LAST_SECTOR_SAVE, KEY_SAVE_INTERVAL, SettingsStore};

/// `saveinterval` default when the setting is absent
/// (`SettingsMenuDialog`: `sliderPref("saveinterval", 60, 10, 600, 10, seconds)`).
/// The value is a duration in **seconds**: `Saves.update` accumulates
/// `Time.delta` (ticks, `arc Time.java:26`) and compares it against
/// `saveinterval * 60` ticks (`Saves.java:219`), i.e. `saveinterval` seconds —
/// 60 s with the default.
pub const DEFAULT_SAVE_INTERVAL_SECONDS: i32 = 60;

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

/// One sector↔save-slot association (`Saves.load`'s "automatically assign
/// sector save slots" pass, `Saves.java:120-186`).
///
/// The association is derived from the save's **meta** (`meta.rules.sector`),
/// never from the file name: a readable copy under any name still backs its
/// sector, and an unreadable file is not listed as a slot at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectorSaveAssignment {
    /// The planet whose sector the slot backs.
    pub planet: PlanetId,
    /// The sector index the slot backs (the remap target for a save that
    /// still carries its pre-remap sector).
    pub sector: u16,
    /// The slot file.
    pub file: PathBuf,
    /// The slot's stem (`SaveSlot.index`), the value `Sector.save` stores.
    pub stem: String,
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
        let remapped: Vec<(PlanetId, u16)> = pending
            .iter()
            .filter_map(|(index, target, _)| {
                sector_of_slot(registry, &self.slots[*index]).map(|(planet, _)| (planet, *target))
            })
            .collect();
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
            // `remap.sourceSector.clearInfo()` unless another save moved into it.
            if !remapped.contains(&(planet, source_sector)) {
                settings.remove(&source_key);
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

    /// `Saves.update` policy half: playtime accrual and the autosave cadence
    /// decision. `autosaved` reports that the `saveinterval` threshold was
    /// crossed and the current slot must be written; the caller performs the
    /// write ([`Saves::update`] inlines a `WriteContext` write, the live host
    /// queues it at the `IoSet` seam). `save_interval_seconds` is the raw
    /// `saveinterval` setting value, a duration in **seconds**.
    pub fn update_policy(
        &mut self,
        update: SavesUpdate,
        settings: &SettingsStore,
        save_interval_seconds: i32,
    ) -> SavesUpdateResult {
        let mut result = SavesUpdateResult::default();
        let playing = update.is_game && !update.paused_with_dialog;
        if playing && self.current.is_some() {
            if self.last_timestamp.is_some() {
                result.playtime_added = update.delta_ticks as i64 * 1000 / 60;
                self.total_playtime += result.playtime_added;
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
            // `Saves.java:217`: `time += Time.delta`. Arc's `Time.delta` is in
            // ticks (`arc Time.java:26`, `deltaTime * 60`), so keeping the
            // accumulator in seconds crosses the same wall-clock threshold.
            self.time += update.delta_ticks as f32 / 60.0;
            // `Saves.java:219`: `time > saveinterval * 60` with `time` in ticks
            // ⇒ the setting is **seconds** (the settings slider is labelled
            // `setting.seconds`, `SettingsMenuDialog.java:373`). The old
            // `* 60` here made the default a 3600 s autosave (CG-SECTOR-01).
            let interval = save_interval_seconds.max(0) as f32;
            if self.time > interval {
                self.saving = true;
                self.time = 0.0;
                result.autosaved = true;
            }
        } else {
            self.time = 0.0;
        }
        result
    }

    /// `Saves.update`. Returns the outcome; the current slot is written inline
    /// through `ctx` (node-06 world; `mind-core` never touches the world here).
    pub fn update(
        &mut self,
        update: SavesUpdate,
        settings: &SettingsStore,
        fs: &dyn FileSystem,
        paths: &Paths,
        ctx: &WriteContext,
        options: &SaveOptions,
    ) -> SavesUpdateResult {
        let interval = settings.get_i32(KEY_SAVE_INTERVAL, DEFAULT_SAVE_INTERVAL_SECONDS);
        let mut result = self.update_policy(update, settings, interval);
        if result.autosaved {
            let mut saved = false;
            if let Some(index) = self.current
                && let Some(slot) = self.slots.get_mut(index)
            {
                saved = slot.save(fs, paths, ctx, options).is_ok();
            }
            result.autosaved = saved;
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

    /// The listed slot whose stem is `stem` (`Sector.save`'s reference).
    ///
    /// `Sector.save` stores a slot *name*, not a path: resolving it through
    /// the listed slots keeps a save that was copied under another name
    /// attached to its sector (CG-SAVE-05).
    pub fn slot_for_stem(&self, stem: &str) -> Option<&SaveSlot> {
        self.slots.iter().find(|slot| slot.index() == stem)
    }

    /// `Saves.load`'s "automatically assign sector save slots" pass: every
    /// listed slot whose **meta** names a sector backs that sector.
    ///
    /// Ported from `Saves.java:120-186`. Java walks the slots and sets
    /// `sector.save = slot` (or `remapTarget.save = slot` for a slot that
    /// still names its pre-remap sector); the file name plays no part, so a
    /// copy under another name still resumes. [`Saves::slots`] only holds
    /// slots whose meta decoded (main or backup, `SaveIO.isSaveValid`), so an
    /// undecodable file can never manufacture a base (CG-SAVE-06).
    pub fn sector_save_assignments(&self, registry: &ContentRegistry) -> Vec<SectorSaveAssignment> {
        let mut out = Vec::new();
        for slot in &self.slots {
            let Some((planet, sector)) = sector_of_slot(registry, slot) else {
                continue;
            };
            let preset_tag = slot
                .meta
                .as_ref()
                .and_then(|meta| meta.tags.get("sectorPreset").cloned());
            let target =
                plan_remap(registry, planet, sector, preset_tag.as_deref()).unwrap_or(sector);
            out.push(SectorSaveAssignment {
                planet,
                sector: target,
                file: slot.file.clone(),
                stem: slot.index(),
            });
        }
        out
    }

    /// Removes the slot backed by `file` from the list, fixing `current`
    /// (`SaveSlot.delete`'s `saves.remove(this, true)` and its `current`
    /// handling, `Saves.java:400-415`).
    pub fn forget_file(&mut self, file: &Path) -> bool {
        let Some(index) = self.slots.iter().position(|slot| slot.file == file) else {
            return false;
        };
        self.slots.remove(index);
        self.current = match self.current {
            Some(current) if current == index => None,
            Some(current) if current > index => Some(current - 1),
            other => other,
        };
        if self.last_sector_save.as_deref() == Some(file) {
            self.last_sector_save = None;
        }
        true
    }

    /// `Saves.saveSector` slot bookkeeping without the world write: creates or
    /// reuses the sector's autosave slot, records `last-sector-save` and makes
    /// it `current` (`SaveSlot.save`'s `state.isGame()` branch; the live host
    /// queues the world write at the `IoSet` seam afterwards).
    pub fn prepare_sector_slot(
        &mut self,
        settings: &mut SettingsStore,
        paths: &Paths,
        planet: &str,
        id: u16,
    ) -> usize {
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
        self.last_sector_save = Some(file);
        settings.put_string(KEY_LAST_SECTOR_SAVE, &self.slots[index].name(settings));
        self.current = Some(index);
        index
    }

    /// Makes the slot backed by `file` current (`SaveSlot.load`/`save` set
    /// `current` for the live slot), creating the entry when the file is not
    /// yet listed (a sector save queued earlier this session).
    pub fn set_current_by_file(&mut self, settings: &mut SettingsStore, file: &Path) -> usize {
        let index = match self.slots.iter().position(|slot| slot.file == file) {
            Some(index) => index,
            None => {
                let slot = SaveSlot::new(file.to_path_buf());
                slot.set_name(settings, &slot.index());
                let index = self.slots.len();
                self.slots.push(slot);
                index
            }
        };
        self.current = Some(index);
        index
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
        let index = self.prepare_sector_slot(settings, paths, planet, id);
        self.slots[index].save(fs, paths, ctx, options)?;
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

    /// `SettingsMenuDialog` clear/clear-planet campaign saves: removes every
    /// sector slot, or only the slots of `planet`.
    pub fn delete_sectors(
        &mut self,
        fs: &dyn FileSystem,
        registry: &ContentRegistry,
        planet: Option<PlanetId>,
    ) -> Result<(), IoError> {
        self.slots.retain_mut(|slot| {
            let Some((slot_planet, _)) = sector_of_slot(registry, slot) else {
                return true;
            };
            if planet.is_some_and(|target| target != slot_planet) {
                return true;
            }
            let _ = slot.delete(fs);
            false
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

/// Applies [`Saves::sector_save_assignments`] to a campaign's sectors
/// (`Saves.load`'s `sector.save = slot`): a readable slot's stem backs its
/// sector and nothing else does.
///
/// Every sector's association is cleared first, so a slot that vanished (or
/// no longer decodes) cannot leave a stale base behind. Returns the number of
/// sectors that ended up with a base. A second slot for one sector wins, with
/// Java's "two corresponding saves" warning (`Saves.java:161-163`).
pub fn apply_sector_saves(campaign: &mut Campaign, assignments: &[SectorSaveAssignment]) -> usize {
    for planet in campaign.planets.values_mut() {
        for sector in &mut planet.sectors {
            sector.save = None;
        }
    }
    let mut applied = 0;
    for assignment in assignments {
        let planet_name = campaign
            .planet(assignment.planet)
            .map(|planet| planet.name.clone())
            .unwrap_or_default();
        let Some(sector) = campaign.sector_mut(assignment.planet, assignment.sector) else {
            continue;
        };
        if let Some(existing) = sector.save.as_deref() {
            log::warn!(
                "Sector {planet_name}:{} has two corresponding saves: `{existing}` and `{}`",
                assignment.sector,
                assignment.stem
            );
        }
        sector.save = Some(assignment.stem.clone());
        applied += 1;
    }
    applied
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
    let rules: crate::io::json::Rules = crate::io::json::JsonIo::read(&meta.rules_json).ok()?;
    rules.sector?.resolve(registry)
}

/// Whether a slot's rules reference a sector at all.
fn sector_key_of(slot: &SaveSlot) -> Option<SectorKey> {
    let meta = slot.meta.as_ref()?;
    let rules: crate::io::json::Rules = crate::io::json::JsonIo::read(&meta.rules_json).ok()?;
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
    use crate::io::save::SaveIo;
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
        settings.put_i32(KEY_SAVE_INTERVAL, 60);
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
            delta_ticks: 59 * 60,
            is_game: true,
            paused_with_dialog: false,
            game_over: false,
            is_campaign: false,
        };
        // The default `saveinterval` is 60 s (`Saves.java:219` compares the
        // tick clock against `saveinterval * 60`), so 59 s of play must not
        // cross the threshold and a further 2 s (61 s total) must.
        let first = saves.update(update, &settings, &fs, &paths, &ctx, &SaveOptions::new());
        assert!(!first.autosaved);
        let second = saves.update(
            SavesUpdate {
                delta_ticks: 2 * 60,
                ..update
            },
            &settings,
            &fs,
            &paths,
            &ctx,
            &SaveOptions::new(),
        );
        assert!(second.autosaved);
        assert!(saves.is_saving());
        saves.finish_saving();
        assert!(!saves.is_saving());
    }

    #[test]
    fn update_policy_interval_is_seconds() {
        // `saveinterval` is SECONDS (`Saves.java:219` compares the tick clock
        // against `saveinterval * 60` ticks, and the slider is labelled
        // `setting.seconds` at `SettingsMenuDialog.java:373`). With
        // saveinterval=10 the threshold is 10 s: 9 s must not cross it and a
        // further 2 s (11 s total) must. The pre-fix `* 60` on the interval
        // made the threshold 600 s, so this fails on it.
        let mut settings = SettingsStore::new();
        settings.put_i32(KEY_SAVE_INTERVAL, 10);
        let mut saves = Saves::new();
        saves
            .slots
            .push(SaveSlot::new(PathBuf::from("/data/saves/0.msav")));
        saves.current = Some(0);
        saves.slots[0].set_autosave(&mut settings, true);
        let update = SavesUpdate {
            delta_ticks: 9 * 60,
            is_game: true,
            paused_with_dialog: false,
            game_over: false,
            is_campaign: true,
        };
        let interval = settings.get_i32(KEY_SAVE_INTERVAL, DEFAULT_SAVE_INTERVAL_SECONDS);
        assert_eq!(interval, 10);
        assert!(!saves.update_policy(update, &settings, interval).autosaved);
        let crossed = saves.update_policy(
            SavesUpdate {
                delta_ticks: 2 * 60,
                ..update
            },
            &settings,
            interval,
        );
        assert!(crossed.autosaved);
        assert!(saves.is_saving());
        saves.finish_saving();
        assert!(!saves.is_saving());
    }

    #[test]
    fn update_policy_accrues_playtime_and_suspends_while_paused() {
        let mut settings = SettingsStore::new();
        settings.put_i32(KEY_SAVE_INTERVAL, 1);
        let mut saves = Saves::new();
        saves
            .slots
            .push(SaveSlot::new(PathBuf::from("/data/saves/0.msav")));
        saves.current = Some(0);
        saves.slots[0].set_autosave(&mut settings, true);
        let update = SavesUpdate {
            delta_ticks: 60,
            is_game: true,
            paused_with_dialog: false,
            game_over: false,
            is_campaign: true,
        };
        // The first update seeds the timestamp; later updates accrue 1 s each.
        assert_eq!(saves.update_policy(update, &settings, 1).playtime_added, 0);
        assert_eq!(
            saves.update_policy(update, &settings, 1).playtime_added,
            1000
        );
        assert_eq!(saves.get_total_playtime(), 1000);
        // A paused-with-dialog update suspends playtime (Saves.java:205).
        let paused = SavesUpdate {
            paused_with_dialog: true,
            ..update
        };
        assert_eq!(saves.update_policy(paused, &settings, 1).playtime_added, 0);
        assert_eq!(saves.get_total_playtime(), 1000);
    }

    #[test]
    fn prepare_sector_slot_makes_it_current_and_autosaved() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();
        let ctx = meta_ctx(r#"{"sector":"serpulo-15"}"#, &[]);
        let file = paths.sector_save("serpulo", 15);

        let index = saves.prepare_sector_slot(&mut settings, &paths, "serpulo", 15);
        assert_eq!(saves.current, Some(index), "fresh sector slot is current");
        assert!(saves.slots[index].is_autosave(&settings));
        assert_eq!(saves.get_last_sector().map(|slot| &slot.file), Some(&file));
        assert_eq!(
            settings.get_string(KEY_LAST_SECTOR_SAVE, "<none>"),
            "sector-serpulo-15"
        );
        // The second call reuses the slot instead of duplicating it.
        assert_eq!(
            saves.prepare_sector_slot(&mut settings, &paths, "serpulo", 15),
            index
        );
        // The writing half still lands the file and keeps `current`.
        saves
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
        assert!(fs.exists(&file));
        assert_eq!(saves.current, Some(index));
    }

    #[test]
    fn set_current_by_file_creates_a_missing_slot() {
        let paths = Paths::new("/data");
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();
        let file = paths.sector_save("serpulo", 15);
        let index = saves.set_current_by_file(&mut settings, &file);
        assert_eq!(saves.current, Some(index));
        assert_eq!(saves.slots[index].file, file);
        // Reuses the existing entry.
        assert_eq!(saves.set_current_by_file(&mut settings, &file), index);
        assert_eq!(saves.slots.len(), 1);
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
        // (resolved sector 170 after the planet-data remap) is remapped to 170.
        assert_eq!(
            plan_remap(&registry, serpulo, 16, Some("groundZero")),
            Some(170)
        );
        // Already at the destination -> no remap.
        assert_eq!(
            plan_remap(&registry, serpulo, 170, Some("groundZero")),
            None
        );
        // Unknown preset tag -> no remap.
        assert_eq!(plan_remap(&registry, serpulo, 170, Some("nope")), None);
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
    fn delete_sectors_scoped_by_planet() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let registry = registry();
        let serpulo = registry.planet_id("serpulo").unwrap();
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();

        // One numbered save, one Serpulo sector save, one Erekir sector save.
        let ctx = WriteContext::meta_only(base_meta_tags(8, 8, 1, "normal"));
        SaveSlot::new(paths.save_slot(0))
            .save(&fs, &paths, &ctx, &SaveOptions::new())
            .unwrap();
        let serpulo_ctx = meta_ctx(r#"{"sector":"serpulo-15"}"#, &[]);
        saves
            .save_sector(
                &fs,
                &paths,
                &mut settings,
                "serpulo",
                15,
                &serpulo_ctx,
                &SaveOptions::new(),
            )
            .unwrap();
        let erekir_ctx = meta_ctx(r#"{"sector":"erekir-0"}"#, &[]);
        saves
            .save_sector(
                &fs,
                &paths,
                &mut settings,
                "erekir",
                0,
                &erekir_ctx,
                &SaveOptions::new(),
            )
            .unwrap();
        saves.load(&fs, &paths, &registry, &mut settings);
        assert_eq!(saves.slots.len(), 3);

        saves.delete_sectors(&fs, &registry, Some(serpulo)).unwrap();
        assert_eq!(
            saves.slots.len(),
            2,
            "only Serpulo's sector save is removed"
        );
        assert!(!fs.exists(&paths.sector_save("serpulo", 15)));
        assert!(fs.exists(&paths.sector_save("erekir", 0)));

        saves.delete_sectors(&fs, &registry, None).unwrap();
        assert_eq!(saves.slots.len(), 1, "numbered saves are kept");
        assert!(!fs.exists(&paths.sector_save("erekir", 0)));
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

    /// CG-SAVE-05 (`Saves.java:120-186`): the sector a slot backs comes from
    /// the save's **meta**, so a copy under a user name still resumes. The
    /// canonical file is gone; only `mybase.msav` exists.
    #[test]
    fn renamed_sector_save_attaches_by_meta() {
        use crate::game::planet::EmptyNeighborhood;

        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let registry = registry();
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();
        let ctx = meta_ctx(
            r#"{"sector":"serpulo-170"}"#,
            &[("sectorPreset", "groundZero")],
        );
        let canonical = paths.sector_save("serpulo", 170);
        SaveSlot::new(canonical.clone())
            .save(&fs, &paths, &ctx, &SaveOptions::new())
            .unwrap();
        let renamed = paths.saves().join("mybase.msav");
        fs.copy(&canonical, &renamed).unwrap();
        fs.delete(&canonical).unwrap();

        saves.load(&fs, &paths, &registry, &mut settings);
        assert_eq!(saves.slots.len(), 1, "the renamed copy is a readable slot");
        let assignments = saves.sector_save_assignments(&registry);
        assert_eq!(assignments.len(), 1);
        assert_eq!(
            assignments[0].planet,
            registry.planet_id("serpulo").unwrap()
        );
        assert_eq!(assignments[0].sector, 170);
        assert_eq!(assignments[0].stem, "mybase");
        assert_eq!(assignments[0].file, renamed);
        assert_eq!(
            saves.slot_for_stem("mybase").map(|slot| slot.file.clone()),
            Some(renamed.clone()),
            "`Sector.save`'s stem resolves back to the renamed file"
        );

        let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        assert_eq!(apply_sector_saves(&mut campaign, &assignments), 1);
        let sector = campaign
            .sector(registry.planet_id("serpulo").unwrap(), 170)
            .unwrap();
        assert_eq!(sector.save.as_deref(), Some("mybase"));
        assert!(sector.has_save());
    }

    /// CG-SAVE-06: a sector file whose meta no longer decodes is not a slot
    /// (`SaveIO.isSaveValid`), so it cannot manufacture a base — even when the
    /// stored record still says one existed. The association pass clears the
    /// stale value instead of stocking the record into a fresh core.
    #[test]
    fn corrupt_sector_save_never_attaches_as_a_base() {
        use crate::game::planet::EmptyNeighborhood;

        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let registry = registry();
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();
        let ctx = meta_ctx(
            r#"{"sector":"serpulo-170"}"#,
            &[("sectorPreset", "groundZero")],
        );
        let canonical = paths.sector_save("serpulo", 170);
        SaveSlot::new(canonical.clone())
            .save(&fs, &paths, &ctx, &SaveOptions::new())
            .unwrap();
        let backup = SaveIo::backup_file_for(&canonical);
        let _ = fs.delete(&backup);
        // A torn write: 1 KB of zero bytes and no backup to fall back to.
        fs.write(&canonical, &[0u8; 1024]).unwrap();
        assert!(!SaveIo::is_save_valid(&fs, &canonical));

        saves.load(&fs, &paths, &registry, &mut settings);
        assert!(saves.slots.is_empty(), "unreadable files are not slots");
        assert!(saves.sector_save_assignments(&registry).is_empty());

        let mut campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
        let serpulo = registry.planet_id("serpulo").unwrap();
        {
            // The settings record survived the corruption (Java keeps it too);
            // only the slot is gone.
            let record = campaign.sector_mut(serpulo, 170).unwrap();
            record.save = Some("sector-serpulo-170".to_owned());
            record.info.info.has_core = true;
        }
        assert_eq!(apply_sector_saves(&mut campaign, &[]), 0);
        let sector = campaign.sector(serpulo, 170).unwrap();
        assert!(sector.save.is_none(), "the stale association is cleared");
        assert!(!sector.has_save());
        assert!(!sector.has_base(), "no base without a readable save");
    }

    /// `SaveSlot.delete` removes the list entry and clears `current` when it is
    /// the deleted slot (`Saves.java:400-415`); the corrupt-resume path uses
    /// the same bookkeeping.
    #[test]
    fn forget_file_removes_slot_and_fixes_current() {
        let fs = MockFs::new();
        let paths = Paths::new("/data");
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();
        let ctx = WriteContext::meta_only(base_meta_tags(8, 8, 1, "a"));
        let mut indexes = Vec::new();
        for slot in 0..3 {
            indexes.push(
                saves
                    .add_save(
                        &fs,
                        &paths,
                        &mut settings,
                        &format!("s{slot}"),
                        &ctx,
                        &SaveOptions::new(),
                    )
                    .unwrap(),
            );
        }
        saves.current = Some(indexes[1]);
        assert!(saves.forget_file(&paths.save_slot(0)));
        assert_eq!(saves.slots.len(), 2);
        assert_eq!(saves.current, Some(0), "the current index shifts down");
        saves.current = Some(0);
        assert!(saves.forget_file(&paths.save_slot(1)));
        assert_eq!(saves.current, None, "deleting the current slot clears it");
        assert!(!saves.forget_file(&PathBuf::from("/data/saves/nope.msav")));
    }
}
