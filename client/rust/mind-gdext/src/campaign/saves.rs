// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Save-slot facade: save/load/import/export slots, previews and the playtime clock.
//! Upstream: game/Saves.java; ui/dialogs/LoadDialog.java; ui/dialogs/SaveDialog.java

use super::cache::dir_mtime_key;
use super::*;

#[godot_api(secondary)]
impl MindCampaign {
    /// Writes the live sim to a named `user://saves/<name>.msav` slot
    /// (queued at the next IO tick boundary). Returns whether the request was
    /// queued.
    #[func]
    pub fn save_slot(&mut self, name: GString) -> bool {
        let Some(mut host) = self.host() else {
            return false;
        };
        let slot = sanitize_slot_name(&name.to_string());
        if slot.is_empty() {
            return false;
        }
        let total_playtime = self.saves.total_playtime;
        host.bind_mut().set_save_playtime(total_playtime);
        // `Saves.addSave` (`Saves.java:265-271`) -> `SaveSlot.save`
        // (`:343-358`): the slot's preview PNG is written from the live grid
        // before the queued save lands at the IO boundary, the slot is
        // registered and it becomes current while in game (`slot.save()`'s
        // `if(state.isGame()) current = this`).
        host.bind_mut()
            .write_save_preview(GString::from(slot.as_str()));
        let file = self.slot_file(&slot);
        if self.session.phase.is_game() {
            settings_hub::with_settings(|store| {
                self.saves.set_current_by_file(store, &file);
            });
        }
        host.bind_mut()
            .request_save(GString::from(Self::slot_user_path(&slot).as_str()), false);
        // `control.saves.addSave(text)` writes at once (`SaveDialog.java:26-31`
        // inside `ui.loadAnd`), and the pause menu that offers it has the sim
        // paused: with the pump halted no IO boundary would ever run the
        // queued save (PAR-0438). Flush it now, like the exit save.
        if host.bind().is_paused() {
            host.bind_mut().flush_io();
        }
        true
    }

    /// `Saves.SaveSlot.previewTexture()`: decodes
    /// `<user>/previews/save_slot_<name>.png` (`Saves.java:359-370`); null when
    /// the file is missing (the dialog keeps its placeholder).
    #[func]
    pub fn save_preview_texture(&mut self, name: GString) -> Option<Gd<Texture2D>> {
        use mind_core::io::map::decode_png;

        let paths = Paths::new(crate::assets::loader::user_data_root());
        let file = paths.preview_for_save(name.to_string().as_str());
        let bytes = NativeFs.read(&file).ok()?;
        let image = decode_png(&bytes).ok()?;
        crate::editor::preview::bind_texture(&image)
    }

    /// Loads a named `user://saves/<name>.msav` slot at the next IO tick
    /// boundary. Returns whether the request was queued.
    #[func]
    pub fn load_slot(&mut self, name: GString) -> bool {
        // `logic.reset()` (`state = new GameState()`): not a playtest.
        self.playtesting_map = None;
        let Some(mut host) = self.host() else {
            return false;
        };
        let slot = sanitize_slot_name(&name.to_string());
        if slot.is_empty() {
            return false;
        }
        let path = Self::slot_user_path(&slot);
        if !FileAccess::file_exists(&path) {
            log::warn!("load_slot: `{path}` does not exist");
            return false;
        }
        // `SaveSlot.load` (`Saves.java:314-328`): apply the saved meta to the
        // live session (rules/wave/timer/stats) and resume its playtime before
        // the world swap lands at the IO boundary.
        let native = ProjectSettings::singleton().globalize_path(&path);
        if let Ok(bytes) = NativeFs.read(std::path::Path::new(native.to_string().as_str()))
            && let Ok(meta) = SaveIo::get_meta_bytes(&bytes)
        {
            apply_loaded_meta(&mut self.session, &meta);
            self.saves.total_playtime = meta.time_played;
            let _ = with_host_runtime(self.host(), |runtime| {
                apply_loaded_meta(&mut runtime.session, &meta);
            });
        } else {
            log::warn!("load_slot: `{path}` meta unavailable; session rules not applied");
        }
        // `SaveSlot.load` (`Saves.java:319-324`): `current = this`. Without it
        // the loaded save is never the current slot, so the autosave cadence,
        // the playtime accrual and the exit save all stay inert and the pause
        // menu keeps `@quit` (CG-SAVE-02).
        let file = self.slot_file(&slot);
        let time_played = self.saves.total_playtime;
        settings_hub::with_settings(|settings| {
            adopt_loaded_slot(&mut self.saves, settings, &file, time_played);
        });
        host.bind_mut().request_load(GString::from(path.as_str()));
        true
    }

    /// `Saves.getTotalPlaytime` (session playtime in milliseconds).
    #[func]
    pub fn get_total_playtime(&self) -> i64 {
        self.saves.get_total_playtime()
    }

    /// Whether the current save slot is an autosave
    /// (`control.saves.getCurrent().isAutosave()`, used by the pause menu's
    /// `@save.quit` label, `PausedDialog.java:116`).
    #[func]
    pub fn current_save_is_autosave(&self) -> bool {
        self.saves
            .get_current()
            .is_some_and(|slot| settings_hub::with_settings(|store| slot.is_autosave(store)))
    }

    /// Lists the known save slots (`name`, `file`, `sector`, `autosave`).
    #[func]
    pub fn list_save_slots(&mut self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        // `request_save` writes through Godot's `user://`, so scan that same
        // directory instead of the bootstrap-time `Saves` snapshot: a slot saved
        // during this session must show up without a restart. The slot listing
        // is memoized on the directory mtime (finding #9); the rows are rebuilt
        // on every call.
        let paths = self.save_paths();
        let slots = self.cached_save_slots();
        for slot in slots.iter() {
            // `Saves.SaveSlot.previewTexture()` shows the saved preview PNG; a
            // slot that has none (imported saves, pre-fix saves) gets one from
            // its own tiles here (`Saves.java:343-358`).
            let preview = slot.preview_path(&paths);
            if !NativeFs.exists(&preview) {
                let file = slot.file.clone();
                self.render_slot_preview_from_file(&file);
            }
            let mut row = VarDictionary::new();
            row.set(
                &GString::from("name"),
                &settings_hub::with_settings(|store| slot.name(store))
                    .as_str()
                    .to_variant(),
            );
            row.set(
                &GString::from("file"),
                &slot.file.to_string_lossy().as_ref().to_variant(),
            );
            row.set(&GString::from("sector"), &slot.is_sector().to_variant());
            row.set(
                &GString::from("autosave"),
                &settings_hub::with_settings(|store| slot.is_autosave(store)).to_variant(),
            );
            // `SaveSlot.hasExternalAssets` (`@save.export.needsload` guard).
            row.set(
                &GString::from("external_assets"),
                &slot.has_external_assets().to_variant(),
            );
            // `SaveSlot.getMap/mode/getWave/getPlayTime/getDate` meta reads;
            // the dialog resolves `map_name` against the map registry
            // (`SaveMeta`: `maps.all().find(m -> m.name().equals(map))`).
            if let Some(meta) = slot.meta.as_ref() {
                row.set(
                    &GString::from("map_name"),
                    &meta.map_name.as_str().to_variant(),
                );
                row.set(&GString::from("wave"), &i64::from(meta.wave).to_variant());
                row.set(
                    &GString::from("playtime_ms"),
                    &meta.time_played.to_variant(),
                );
                row.set(&GString::from("timestamp_ms"), &meta.timestamp.to_variant());
                let mode = serde_json::from_str::<Rules>(&quote_bare_keys(&meta.rules_json))
                    .map(|rules| rules.mode().name())
                    .unwrap_or("survival");
                row.set(&GString::from("mode"), &mode.to_variant());
            }
            out.push(&row);
        }
        out
    }

    /// `LoadDialog.addSetup`: imports one `.msav` into `user://saves`
    /// (`SaveIO.isSaveValid` -> `SaveIO.getMeta` sector check ->
    /// `Saves.importSave`, `LoadDialog.java:198-216`). Returns `""` on success
    /// or `"invalid"` / `"nocampaign"` / `"fail:<message>"` for the dialog.
    #[func]
    pub fn import_save(&mut self, path: GString) -> GString {
        let native = ProjectSettings::singleton().globalize_path(&path);
        let from = std::path::PathBuf::from(native.to_string());
        if !SaveIo::is_save_valid(&NativeFs, &from) {
            return GString::from("invalid");
        }
        // `meta.rules.sector != null` -> `@save.nocampaign`: campaign saves are
        // not importable.
        if let Ok(meta) = SaveIo::get_meta(&NativeFs, &from) {
            let has_sector = serde_json::from_str::<serde_json::Value>(&meta.rules_json)
                .ok()
                .and_then(|value| value.get("sector").cloned())
                .is_some_and(|sector| !sector.is_null());
            if has_sector {
                return GString::from("nocampaign");
            }
        }
        let paths = self.save_paths();
        let result = settings_hub::with_settings(|store| {
            self.saves.import_save(&NativeFs, &paths, store, &from)
        });
        match result {
            Ok(index) => {
                // `importData` ships no `previews/` entries (`SettingsMenuDialog.java:575-608`),
                // so the imported slot renders its preview from its own tiles
                // (`Saves.SaveSlot.savePreview`, `Saves.java:343-358`).
                let slot = self
                    .saves
                    .get_save_slots()
                    .get(index)
                    .map(|slot| (slot.index(), slot.file.clone(), slot.preview_path(&paths)));
                if let Some((_stem, file, preview)) = slot
                    && !NativeFs.exists(&preview)
                {
                    self.render_slot_preview_from_file(&file);
                }
                GString::new()
            }
            Err(error) => GString::from(format!("fail:{error}").as_str()),
        }
    }

    /// `LoadDialog.modifyButton` `Icon.trash`: `slot.delete()` plus the preview
    /// PNG and the `save-<n>-name`/`save-<n>-autosave` settings keys
    /// (`Saves.java:245-255`). Returns whether the slot file is gone.
    #[func]
    pub fn delete_save_slot(&mut self, name: GString) -> bool {
        let stem = name.to_string();
        let Some(mut slot) = self.save_slot_by_name(&stem) else {
            return false;
        };
        let paths = self.save_paths();
        let preview = slot.preview_path(&paths);
        if NativeFs.exists(&preview) {
            let _ = NativeFs.delete(&preview);
        }
        settings_hub::with_settings(|store| {
            store.remove(&mind_core::io::settings::slot_name_key(&stem));
            store.remove(&mind_core::io::settings::slot_autosave_key(&stem));
        });
        if let Err(error) = slot.delete(&NativeFs) {
            log::warn!("delete_save_slot `{stem}`: {error}");
            return false;
        }
        self.flush_settings("delete_save_slot: settings save failed");
        true
    }

    /// `LoadDialog.modifyButton` `Icon.pencil`: `slot.setName(text)`.
    #[func]
    pub fn rename_save_slot(&mut self, name: GString, new_name: GString) -> bool {
        let stem = name.to_string();
        let renamed = new_name.to_string();
        if renamed.trim().is_empty() {
            return false;
        }
        let Some(slot) = self.save_slot_by_name(&stem) else {
            return false;
        };
        settings_hub::with_settings(|store| slot.set_name(store, &renamed));
        self.flush_settings("rename_save_slot: settings save failed");
        true
    }

    /// `Saves.getCurrent().isAutosave()` (`PausedDialog.java:116,154-155`):
    /// whether the live current save slot autosaves (`save-<index>-autosave`,
    /// default true). `false` when no slot is current.
    #[func]
    pub fn current_save_autosave(&self) -> bool {
        self.saves
            .current_slot()
            .is_some_and(|slot| settings_hub::with_settings(|store| slot.is_autosave(store)))
    }

    /// `LoadDialog.modifyButton` `Icon.save`: `slot.setAutosave(on)`.
    #[func]
    pub fn set_slot_autosave(&mut self, name: GString, on: bool) -> bool {
        let stem = name.to_string();
        let Some(slot) = self.save_slot_by_name(&stem) else {
            return false;
        };
        settings_hub::with_settings(|store| slot.set_autosave(store, on));
        self.flush_settings("set_slot_autosave: settings save failed");
        true
    }

    /// `LoadDialog.modifyButton` `Icon.export`: `slot.exportFile(to)`
    /// (`FileChooser.export`).
    #[func]
    pub fn export_save_slot(&mut self, name: GString, path: GString) -> bool {
        let stem = name.to_string();
        let Some(mut slot) = self.save_slot_by_name(&stem) else {
            return false;
        };
        let native = ProjectSettings::singleton().globalize_path(&path);
        let to = std::path::PathBuf::from(native.to_string());
        match slot.export_file(&NativeFs, &to, None) {
            Ok(()) => true,
            Err(error) => {
                log::warn!("export_save_slot `{stem}`: {error}");
                false
            }
        }
    }

    /// `SaveSlot.getPlayTime()` for the current slot (`GameOverDialog`'s
    /// "Time Played" row, `GameOverDialog.java:91-97`): the live accumulated
    /// playtime, else the slot's saved meta. `-1` when no slot is current
    /// (Java hides the row then).
    #[func]
    pub fn current_save_playtime_ms(&self) -> i64 {
        let Some(slot) = self.saves.get_current() else {
            return -1;
        };
        if self.saves.total_playtime > 0 {
            return self.saves.total_playtime;
        }
        slot.meta
            .as_ref()
            .map(|meta| meta.time_played)
            .unwrap_or_default()
    }
}

impl MindCampaign {
    /// `Saves.SaveSlot.savePreview` on the live world (`Saves.java:343-358`):
    /// renders `<user>/previews/save_slot_<stem>.png` from the current grid
    /// before the queued save lands at the IO boundary. Java runs it for every
    /// slot save (manual, autosave, sector, capture, game over).
    pub(super) fn write_live_slot_preview(&self, file: &std::path::Path) {
        let Some(stem) = file.file_stem().and_then(|stem| stem.to_str()) else {
            return;
        };
        if let Some(mut host) = self.host() {
            host.bind_mut().write_save_preview(GString::from(stem));
        }
    }

    /// Queues a save of `file` through `host` (`SaveSlot.save`): the preview
    /// PNG first, then the IO-boundary request, the optional `Saves.saving`
    /// finish tick, the optional immediate flush and the caller's log line
    /// (`label` is the whole message prefix, so each call site's log text is
    /// kept verbatim).
    pub(super) fn queue_slot_save(
        &mut self,
        host: &mut Gd<MindSimHost>,
        file: &std::path::Path,
        finish_tick: Option<u64>,
        flush: bool,
        label: &str,
    ) {
        self.write_live_slot_preview(file);
        host.bind_mut()
            .request_save(GString::from(file.to_string_lossy().as_ref()), false);
        if let Some(tick) = finish_tick {
            self.saving_finish_tick = Some(tick);
        }
        if flush {
            host.bind_mut().flush_io();
        }
        log::info!("{label} (`{}`)", file.display());
    }

    /// `<user://saves>/<stem>.<SAVE_EXTENSION>` on the native filesystem.
    pub(super) fn slot_file(&self, stem: &str) -> std::path::PathBuf {
        self.save_paths()
            .saves()
            .join(format!("{stem}.{SAVE_EXTENSION}"))
    }

    /// `user://saves/<stem>.<SAVE_EXTENSION>`: the Godot-facing path the
    /// `request_save`/`request_load` seams take.
    pub(super) fn slot_user_path(stem: &str) -> String {
        format!("user://saves/{stem}.{SAVE_EXTENSION}")
    }

    /// `SaveSlot.savePreview` for a slot with no live world: renders the preview
    /// PNG from the slot file's own tiles. Used on import and when listing a
    /// slot that has no preview (`Saves.java:343-358`).
    fn render_slot_preview_from_file(&mut self, file: &std::path::Path) -> bool {
        let paths = self.save_paths();
        let Some(mut registry) = self.registry.take() else {
            log::warn!("save preview: content registry unavailable");
            return false;
        };
        let mut palette = mind_core::io::map::BlockPalette::of(&registry);
        palette.apply_block_colors(&crate::render::minimap::load_block_colors());
        let mut slot = mind_core::io::save::slot::SaveSlot::new(file.to_path_buf());
        let result = slot.write_preview_from_file(&NativeFs, &paths, &mut registry, palette);
        self.registry = Some(registry);
        match result {
            Ok(_) => true,
            Err(error) => {
                log::warn!("save preview `{}` render failed: {error}", file.display());
                false
            }
        }
    }

    /// The `user://` `Paths` the save slots live in (same root as
    /// `list_save_slots`).
    fn save_paths(&self) -> Paths {
        let root = ProjectSettings::singleton()
            .globalize_path("user://")
            .to_string();
        Paths::new(root)
    }

    /// The `user://saves` slot listing, memoized on the saves directory's mtime
    /// (finding #9): [`list_slot_files`] reads and parses every slot header, and
    /// the campaign dialogs ask for the listing on every rebuild.
    ///
    /// Safe against in-place rewrites: every save path in the port goes through
    /// `SaveIo::save`, which renames `<file> -> <file>-backup` and then
    /// `<file>.tmp -> <file>` inside this directory, so even a logical
    /// overwrite of the same slot bumps the directory mtime; imports and
    /// deletes change the directory too. An unreadable directory bypasses the
    /// memo rather than pinning an empty listing.
    pub(super) fn cached_save_slots(&self) -> Rc<Vec<SaveSlot>> {
        let paths = self.save_paths();
        let key = dir_mtime_key(&paths.saves());
        if let Some(cached) = self.save_slots_cache.borrow().get(key) {
            return Rc::clone(cached);
        }
        let slots = Rc::new(list_slot_files(&NativeFs, &paths));
        self.save_slots_cache
            .borrow_mut()
            .store(key, Rc::clone(&slots));
        slots
    }

    /// One `user://saves/<stem>.msav` slot, re-read from disk.
    fn save_slot_by_name(&self, stem: &str) -> Option<mind_core::io::save::slot::SaveSlot> {
        let paths = self.save_paths();
        list_slot_files(&NativeFs, &paths)
            .into_iter()
            .find(|slot| slot.index() == stem)
    }
}

/// `SaveSlot.load` meta half (`Saves.java:314-328`): the saved rules/wave,
/// wave timer and stats replace the live session's, and the loaded match is
/// playing (`readMeta`/`readRules`).
fn apply_loaded_meta(session: &mut PlaySession, meta: &SaveMeta) {
    if let Ok(rules) = serde_json::from_str::<Rules>(&quote_bare_keys(&meta.rules_json)) {
        session.rules = rules;
    } else {
        log::warn!("load: rules JSON did not parse; session rules kept");
    }
    session.wave = meta.wave;
    if let Some(wavetime) = meta
        .tags
        .get("wavetime")
        .and_then(|value| value.parse::<f32>().ok())
    {
        session.wavetime = wavetime;
    }
    if let Some(stats) = meta
        .tags
        .get("stats")
        .and_then(|value| serde_json::from_str::<GameStats>(value).ok())
    {
        session.stats = stats;
    }
    session.phase = State::Playing;
}

/// Sanitizes a save-slot name into a file stem (alphanumerics, `-`, `_`).
///
/// **Deviation (J01 T6, kept deliberately).** Java slots are numeric files:
/// `Saves.getNextSlotFile` builds `saves/<index>.msav` and the display name
/// lives in the `save-<index>-name` setting / the meta (`Saves.java:265-271,
/// 283-290`), previews as `previews/save_slot_<index>.png`. The port keys the
/// file, the `save-<stem>-name`/`-autosave` settings and the preview PNG by
/// this sanitized name — the round-I I06 flow (`save_slot("t5-preview")` ->
/// `save_slot_t5-preview.png`) built the load/save/import/rename/export
/// surface on stems, and `Saves::import_save` keeps imported Java saves on
/// their numeric stems. A Java data export/import therefore round-trips by
/// slot identity, not by file name; switching to numeric indices would churn
/// the save/preview/settings ABI for no behavioural gain (the slot name is
/// user data in both designs).
pub(super) fn sanitize_slot_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect()
}

/// `SaveSlot.load`'s policy half (`Saves.java:319-324`): the loaded slot
/// becomes `Saves.current` and its saved playtime seeds the accumulator, so the
/// autosave cadence, the playtime accrual and the exit save all key on the slot
/// the player loaded (`MindCampaign.load_slot`, CG-SAVE-02). Returns the slot
/// index — the entry is created when the file is not listed yet.
fn adopt_loaded_slot(
    saves: &mut Saves,
    settings: &mut SettingsStore,
    file: &std::path::Path,
    time_played: i64,
) -> usize {
    saves.total_playtime = time_played;
    saves.set_current_by_file(settings, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CG-SAVE-02: `SaveSlot.load` (`Saves.java:319-324`) makes the loaded slot
    /// `Saves.current` and seeds the playtime from its meta, so the autosave
    /// cadence, the exit save and the pause label key on the slot the player
    /// loaded. Before the fix `load_slot` left `current` at `None`, so this
    /// policy was inert for the whole session.
    #[test]
    fn loaded_slot_becomes_current_and_autosaves() {
        let mut settings = SettingsStore::new();
        let mut saves = Saves::new();
        let file = std::path::PathBuf::from("/data/saves/t5-load.msav");
        assert!(saves.get_current().is_none(), "no slot is current at boot");

        let index = adopt_loaded_slot(&mut saves, &mut settings, &file, 12_345);
        assert_eq!(saves.current, Some(index), "the loaded slot is current");
        assert_eq!(saves.get_total_playtime(), 12_345);
        assert_eq!(
            saves.get_current().map(|slot| slot.file.clone()),
            Some(file)
        );
        // `save-<stem>-autosave` defaults to true (`SaveSlot.isAutosave`), so a
        // loaded slot takes part in the autosave cadence like Java's.
        assert!(
            saves
                .get_current()
                .is_some_and(|slot| slot.is_autosave(&settings))
        );
        let update = SavesUpdate {
            delta_ticks: 61 * 60,
            is_game: true,
            paused_with_dialog: false,
            game_over: false,
            is_campaign: false,
        };
        // 61 s crosses the default 60 s interval; the same call seeds the
        // playtime timestamp, so the next one accrues on the loaded slot.
        assert!(
            saves
                .update_policy(update, &settings, DEFAULT_SAVE_INTERVAL_SECONDS)
                .autosaved
        );
        let accrued = saves.update_policy(update, &settings, DEFAULT_SAVE_INTERVAL_SECONDS);
        assert_eq!(accrued.playtime_added, 61_000);
        assert!(saves.get_total_playtime() > 12_345);
    }
}
