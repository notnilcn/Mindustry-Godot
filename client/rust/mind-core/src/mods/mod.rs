// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Mod manager (`Vars.mods`): discovery, states, ordering, import/remove.
//!
//! Ported from `core/src/mindustry/mod/Mods.java`. Plan 20 keeps the parser and
//! data-manager halves in sibling modules (`json`, `patch`, `assets`); this file
//! owns the lifecycle surface the content loader and dialogs drive.
//!
//! `mind-core::mods` is Godot-free and tokio-free; file access goes through
//! plan 04's [`FileSystem`](crate::io::FileSystem).

pub mod assets;
pub mod deps;
pub mod discovery;
pub mod json;
pub mod loaded;
pub mod meta;
pub mod overlay;
pub mod patch;
pub mod provider;
pub mod script;
pub mod server;

use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use serde::Serialize;

use crate::io::{FileSystem, SettingsStore};

pub use deps::{ModDependency, resolve_dependencies};
pub use discovery::{
    ModCandidate, ModEntry, ModRoot, ModSource, config_file, config_folder, find_meta, read_zip,
    scan_mod_directory,
};
pub use loaded::{LoadedMod, UnsupportedReason};
pub use meta::{
    BLACKLISTED_MODS, MAX_MOD_SUBTITLE_LENGTH, META_FILES, MIN_JAVA_MOD_GAME_VERSION,
    MIN_MOD_GAME_VERSION, ModMeta,
};
pub use script::{NoScriptHost, ScriptError, ScriptHost};

/// Mod errors (`Mods.ModLoadException` + IO/zip wrapping).
#[derive(Debug, thiserror::Error)]
pub enum ModError {
    /// Underlying filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] crate::io::IoError),
    /// Invalid mod metadata/package.
    #[error("{0}")]
    Invalid(String),
}

/// Alias matching the plan's `ModLoadError` name.
pub type ModLoadError = ModError;

/// `Mods.ModState`. Ordinal order is load/display order (do not reorder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum ModState {
    /// Loaded and enabled.
    Enabled = 0,
    /// Loaded, enabled, but some content failed (isolated).
    ContentErrors = 1,
    /// A required dependency is absent.
    MissingDependencies = 2,
    /// A required dependency failed/(was disabled).
    IncompleteDependencies = 3,
    /// The dependency graph has a cycle.
    CircularDependencies = 4,
    /// Unsupported on this build/platform.
    Unsupported = 5,
    /// Disabled via settings.
    Disabled = 6,
}

impl ModState {
    /// Ordinal (`ModState.ordinal()`).
    pub const fn ordinal(self) -> u8 {
        self as u8
    }
}

/// Per-mod JSON row for the harness/dialog list.
#[derive(Debug, Clone, Serialize)]
pub struct ModListEntry {
    /// Internal name.
    pub name: String,
    /// Display name (colors stripped).
    pub display_name: String,
    /// Author, if any.
    pub author: Option<String>,
    /// Version string.
    pub version: String,
    /// Resolved state.
    pub state: ModState,
    /// State ordinal (stable sort key).
    pub state_ordinal: u8,
    /// Whether the state counts as enabled.
    pub enabled: bool,
    /// Sprite scale.
    pub texturescale: f32,
    /// `minGameVersion`.
    pub min_game_version: String,
    /// Declared required dependencies.
    pub dependencies: Vec<String>,
    /// Declared soft dependencies.
    pub soft_dependencies: Vec<String>,
    /// Unresolved required dependencies.
    pub missing_dependencies: Vec<String>,
    /// Whether the mod is hidden.
    pub hidden: bool,
    /// Whether the mod ships Java code.
    pub java: bool,
    /// Whether the mod ships scripts.
    pub has_scripts: bool,
    /// Discovery source tag.
    pub source: &'static str,
    /// Short description (dialog subtitle).
    pub short_description: String,
}

/// Discovery/load report (`mind-headless mods list`).
#[derive(Debug, Clone, Serialize)]
pub struct ModLoadReport {
    /// Scanned mod directory.
    pub mod_directory: String,
    /// Mods in sorted order.
    pub mods: Vec<ModListEntry>,
    /// Whether a restart is required.
    pub requires_reload: bool,
}

impl ModLoadReport {
    /// Plan-00-favored `{ "mods": [...] }` wrapper.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// `Vars.mods`: the mod manager.
pub struct Mods {
    headless: bool,
    mod_directory: PathBuf,
    mods: Vec<LoadedMod>,
    /// Ordered enabled-mod indices cache (invalidated by state changes).
    last_ordered: Option<Vec<usize>>,
    requires_reload: bool,
    failed_to_launch: bool,
    script_host: Box<dyn ScriptHost>,
}

impl std::fmt::Debug for Mods {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mods")
            .field("headless", &self.headless)
            .field("mod_directory", &self.mod_directory)
            .field("mods", &self.mods.len())
            .field("requires_reload", &self.requires_reload)
            .finish()
    }
}

impl Mods {
    /// Empty manager rooted at `mod_directory`.
    pub fn new(headless: bool, mod_directory: impl Into<PathBuf>) -> Self {
        Self {
            headless,
            mod_directory: mod_directory.into(),
            mods: Vec::new(),
            last_ordered: None,
            requires_reload: false,
            failed_to_launch: false,
            script_host: Box::new(NoScriptHost),
        }
    }

    /// Replaces the script host (OD1 M8 decision point).
    pub fn set_script_host(&mut self, host: Box<dyn ScriptHost>) {
        self.script_host = host;
    }

    /// The configured mod directory.
    pub fn mod_directory(&self) -> &Path {
        &self.mod_directory
    }

    /// All mods, including disabled ones.
    pub fn list(&self) -> &[LoadedMod] {
        &self.mods
    }

    /// Mutable mod list (harness/parser only).
    pub fn mods_mut(&mut self) -> &mut Vec<LoadedMod> {
        &mut self.mods
    }

    /// `Mods.getMod(name)` → index.
    pub fn get_mod(&self, name: &str) -> Option<usize> {
        self.mods.iter().position(|mod_| mod_.name == name)
    }

    /// Mod record by index.
    pub fn mod_at(&self, index: usize) -> Option<&LoadedMod> {
        self.mods.get(index)
    }

    /// Mutable mod record by index.
    pub fn mod_at_mut(&mut self, index: usize) -> Option<&mut LoadedMod> {
        self.mods.get_mut(index)
    }

    /// `Mods.requiresReload()`.
    pub fn requires_reload(&self) -> bool {
        self.requires_reload
    }

    /// Stores the `requiresReload` flag (import/enable/delete set it).
    pub fn set_requires_reload(&mut self, value: bool) {
        self.requires_reload = value;
    }

    /// `Mods.skipModLoading()`.
    pub fn skip_mod_loading(&self, settings: &SettingsStore) -> bool {
        self.failed_to_launch && settings.get_bool("modcrashdisable", true)
    }

    /// Marks the previous launch as failed (`modcrashdisable` boot behavior).
    pub fn set_failed_to_launch(&mut self, value: bool) {
        self.failed_to_launch = value;
    }

    /// Scans `dir`, parses metadata and resolves dependency states.
    pub fn load(
        &mut self,
        fs: &dyn FileSystem,
        dir: &Path,
        settings: &SettingsStore,
    ) -> Result<ModLoadReport, ModError> {
        self.mod_directory = dir.to_path_buf();
        self.mods.clear();
        self.last_ordered = None;

        let candidates = scan_mod_directory(fs, dir)?;
        let mut parsed: Vec<(ModCandidate, ModMeta, String)> = Vec::new();
        for candidate in candidates {
            let Some((_meta_file, meta)) = find_meta(fs, &candidate.root) else {
                continue;
            };
            let internal = meta.internal_name.clone();
            parsed.push((candidate, meta, internal));
        }

        let metas: Vec<ModMeta> = parsed.iter().map(|(_, meta, _)| meta.clone()).collect();
        let is_enabled = |name: &str| settings.get_bool(&format!("mod-{name}-enabled"), true);
        let resolved = resolve_dependencies(&metas, &is_enabled);

        let mut by_name: IndexMap<String, ModCandidate> = IndexMap::new();
        for (candidate, _meta, internal) in parsed {
            // Last candidate with a given internal name wins (upstream
            // `mapping.put`), but a duplicate surfaces as a load warning.
            if by_name.contains_key(&internal) {
                log::warn!("[Mods] duplicate mod internal name `{internal}`; keeping the last");
            }
            by_name.insert(internal, candidate);
        }

        for (internal, state) in &resolved {
            let Some(candidate) = by_name.get(internal).cloned() else {
                continue;
            };
            let Some((_, mut meta)) = find_meta(fs, &candidate.root) else {
                continue;
            };
            // `loadMod`: version keeps only its first line.
            if let Some(version) = meta.version.as_mut()
                && let Some(line) = version.find('\n')
            {
                version.truncate(line);
            }
            let mut loaded = LoadedMod::new(
                self.mods.len(),
                candidate.file.clone(),
                candidate.root.clone(),
                candidate.source,
                meta,
            );
            loaded.has_scripts = candidate.root.exists(fs, "scripts");
            loaded.state = *state;
            self.mods.push(loaded);
        }

        self.update_all_dependencies();
        let mut resolved_reasons: Vec<(usize, UnsupportedReason)> = Vec::new();
        let mut disabled: Vec<usize> = Vec::new();
        for (index, mod_) in self.mods.iter().enumerate() {
            if mod_.state != ModState::Enabled {
                continue;
            }
            if let Some(reason) = self.unsupported_reason(mod_) {
                resolved_reasons.push((index, reason));
            } else if !mod_.should_be_enabled(settings) {
                disabled.push(index);
            }
        }
        for (index, reason) in resolved_reasons {
            let mod_ = &mut self.mods[index];
            mod_.state = ModState::Unsupported;
            mod_.unsupported_reason = Some(reason);
        }
        for index in disabled {
            self.mods[index].state = ModState::Disabled;
        }
        // Java-class code hooks are unavailable (OD1): surface the reason but
        // keep the mod enabled so its JSON content/assets still load.
        for mod_ in &mut self.mods {
            if mod_.state == ModState::Enabled
                && mod_.is_java()
                && mod_.unsupported_reason.is_none()
            {
                mod_.unsupported_reason = Some(UnsupportedReason::JavaModUnsupported);
            }
        }

        self.sort_mods();
        self.reindex();
        self.requires_reload = false;
        Ok(self.report())
    }

    /// `Mods.updateDependencies`.
    fn update_all_dependencies(&mut self) {
        let enabled_names: Vec<String> = self
            .mods
            .iter()
            .filter(|mod_| mod_.enabled())
            .map(|mod_| mod_.name.clone())
            .collect();
        for mod_ in &mut self.mods {
            mod_.dependencies = mod_.meta.dependencies.clone();
            mod_.soft_dependencies = mod_.meta.soft_dependencies.clone();
            mod_.missing_dependencies = mod_
                .meta
                .dependencies
                .iter()
                .filter(|name| !enabled_names.contains(name))
                .cloned()
                .collect();
            mod_.missing_soft_dependencies = mod_
                .meta
                .soft_dependencies
                .iter()
                .filter(|name| !enabled_names.contains(name))
                .cloned()
                .collect();
        }
    }

    /// Support check used during `load` (`LoadedMod.isSupported`).
    fn unsupported_reason(&self, mod_: &LoadedMod) -> Option<UnsupportedReason> {
        // `LoadedMod.isSupported`: servers support everything.
        if self.headless {
            return None;
        }
        if mod_.is_blacklisted() {
            return Some(UnsupportedReason::Blacklisted);
        }
        if mod_.is_outdated() {
            return Some(UnsupportedReason::Outdated);
        }
        None
    }

    /// `Mods.sortMods`: state ordinal, then name.
    fn sort_mods(&mut self) {
        self.mods.sort_by(|a, b| {
            a.state
                .ordinal()
                .cmp(&b.state.ordinal())
                .then_with(|| a.name.cmp(&b.name))
        });
    }

    /// Reassigns stable per-run indices after a sort/insert/remove.
    fn reindex(&mut self) {
        for (index, mod_) in self.mods.iter_mut().enumerate() {
            mod_.index = index;
        }
        self.last_ordered = None;
    }

    /// `Mods.orderedMods`: enabled mods in dependency order (indices).
    pub fn ordered_mods(&mut self) -> Vec<usize> {
        if let Some(cached) = &self.last_ordered {
            return cached.clone();
        }
        let enabled_metas: Vec<ModMeta> = self
            .mods
            .iter()
            .filter(|mod_| mod_.enabled())
            .map(|mod_| mod_.meta.clone())
            .collect();
        let resolved = resolve_dependencies(&enabled_metas, &|_| true);
        let mut ordered = Vec::new();
        for (internal, state) in resolved {
            if state != ModState::Enabled {
                continue;
            }
            if let Some(index) = self
                .mods
                .iter()
                .position(|mod_| mod_.name == internal && mod_.enabled())
            {
                ordered.push(index);
            }
        }
        self.last_ordered = Some(ordered.clone());
        ordered
    }

    /// Iterates enabled mods in dependency order.
    pub fn each_enabled(&mut self, mut f: impl FnMut(&LoadedMod)) {
        for index in self.ordered_mods() {
            if let Some(mod_) = self.mods.get(index) {
                f(mod_);
            }
        }
    }

    /// Enabled mods that ship scripts (OD1 surface).
    pub fn script_mods(&self) -> Vec<&LoadedMod> {
        self.mods
            .iter()
            .filter(|mod_| mod_.enabled() && mod_.has_scripts)
            .collect()
    }

    /// `Mods.loadScripts` with the active host; marks script-only mods as
    /// unsupported when the host refuses (`NoScriptHost`, OD1).
    pub fn load_scripts(&mut self, fs: &dyn FileSystem) {
        let engine = self.script_host.engine_name();
        let indices: Vec<usize> = self
            .mods
            .iter()
            .enumerate()
            .filter(|(_, mod_)| mod_.enabled() && mod_.has_scripts)
            .map(|(index, _)| index)
            .collect();
        for index in indices {
            let (name, script_path) = {
                let mod_ = &self.mods[index];
                let script = mod_
                    .root
                    .walk(fs, "scripts")
                    .ok()
                    .and_then(|files| files.into_iter().next());
                (mod_.name.clone(), script)
            };
            let Some(script_path) = script_path else {
                continue;
            };
            if engine == "none" {
                log::warn!(
                    "[Mods] script mods are not supported in this build ({name}/{script_path})"
                );
                let mod_ = &mut self.mods[index];
                mod_.unsupported_reason = Some(UnsupportedReason::ScriptsUnsupported);
                if mod_.state == ModState::Enabled {
                    mod_.state = ModState::Unsupported;
                }
                continue;
            }
            match self
                .mods
                .get(index)
                .map(|mod_| mod_.root.read_to_string(fs, &script_path))
            {
                Some(Ok(source)) => {
                    if let Err(error) =
                        self.script_host
                            .run_mod_script(&name, &script_path, &source)
                    {
                        log::warn!("[Mods] {name}: {error}");
                        let mod_ = &mut self.mods[index];
                        mod_.unsupported_reason = Some(UnsupportedReason::ScriptsUnsupported);
                    }
                }
                Some(Err(error)) => log::warn!("[Mods] {name}: {error}"),
                None => {}
            }
        }
        self.sort_mods();
        self.reindex();
    }

    /// `Mods.hasContentErrors()`.
    pub fn has_content_errors(&self) -> bool {
        self.mods.iter().any(LoadedMod::has_content_errors)
    }

    /// `Mods.getModStrings()` — enabled, non-hidden `name:version`, sorted.
    pub fn get_mod_strings(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .mods
            .iter()
            .filter(|mod_| mod_.enabled() && !mod_.meta.hidden)
            .map(LoadedMod::mod_string)
            .collect();
        out.sort();
        out
    }

    /// `Mods.getIncompatibility`: client mods missing from `server` are
    /// returned; `server` is rewritten to contain server-only mods.
    pub fn get_incompatibility(&self, server: &mut Vec<String>) -> Vec<String> {
        let mods = self.get_mod_strings();
        let mut result = mods.clone();
        for mod_ in &mods {
            if let Some(position) = server.iter().position(|entry| entry == mod_) {
                server.remove(position);
                if let Some(position) = result.iter().position(|entry| entry == mod_) {
                    result.remove(position);
                }
            }
        }
        result
    }

    /// `Mods.setEnabled`.
    pub fn set_enabled(&mut self, name: &str, enabled: bool, settings: &mut SettingsStore) {
        let Some(index) = self.get_mod(name) else {
            return;
        };
        let currently = self.mods[index].enabled();
        if currently == enabled {
            return;
        }
        settings.put_bool(&format!("mod-{name}-enabled"), enabled);
        settings.put_bool(&format!("mod-{name}-failed"), false);
        self.requires_reload = true;
        self.mods[index].state = if enabled {
            ModState::Enabled
        } else {
            ModState::Disabled
        };
        self.update_all_dependencies();
        self.sort_mods();
        self.reindex();
    }

    /// `Mods.importMod`: copies a zip into the mod directory, loads it, enables
    /// it and marks a reload required. Folders are not importable.
    pub fn import_mod(
        &mut self,
        fs: &dyn FileSystem,
        source: &Path,
        force_enable: bool,
        settings: &mut SettingsStore,
    ) -> Result<String, ModError> {
        let base = source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("mod")
            .replace([':', ' '], "_");
        let mut final_name = base.clone();
        let mut count = 1;
        while fs.exists(&self.mod_directory.join(format!("{final_name}.zip"))) {
            final_name = format!("{base}{count}");
            count += 1;
        }
        let dest = self.mod_directory.join(format!("{final_name}.zip"));
        let bytes = fs.read(source)?;
        fs.write(&dest, &bytes)?;

        let root = read_zip(&bytes)?;
        let Some((_, mut meta)) = find_meta(fs, &root) else {
            let _ = fs.delete(&dest);
            return Err(ModError::Invalid(String::from(
                "Invalid file: No mod.json found.",
            )));
        };
        if let Some(version) = meta.version.as_mut()
            && let Some(line) = version.find('\n')
        {
            version.truncate(line);
        }
        if self.get_mod(&meta.internal_name).is_some() {
            let _ = fs.delete(&dest);
            return Err(ModError::Invalid(format!(
                "A mod with the name '{}' is already imported.",
                meta.internal_name
            )));
        }
        let mut loaded = LoadedMod::new(
            self.mods.len(),
            dest,
            root.clone(),
            ModSource::ImportedZip,
            meta,
        );
        loaded.has_scripts = root.exists(fs, "scripts");
        let name = loaded.name.clone();
        settings.put_bool(&format!("mod-{name}-failed"), false);
        if force_enable {
            settings.put_bool(&format!("mod-{name}-enabled"), true);
        }
        self.mods.push(loaded);
        self.requires_reload = true;
        self.update_all_dependencies();
        self.sort_mods();
        self.reindex();
        Ok(name)
    }

    /// `Mods.removeMod`: deletes the file/folder and drops the record.
    pub fn remove_mod(&mut self, fs: &dyn FileSystem, name: &str) -> Result<(), ModError> {
        let Some(index) = self.get_mod(name) else {
            return Err(ModError::Invalid(format!("mod `{name}` is not loaded")));
        };
        let file = self.mods[index].file.clone();
        let is_dir = fs.exists(&file) && !fs.is_file(&file);
        if is_dir {
            let mut stack = vec![file.clone()];
            let mut files = Vec::new();
            while let Some(dir) = stack.pop() {
                for child in fs.ls(&dir)? {
                    if fs.is_file(&child) {
                        files.push(child);
                    } else {
                        stack.push(child);
                    }
                }
            }
            for path in files {
                let _ = fs.delete(&path);
            }
        } else {
            let _ = fs.delete(&file);
        }
        let state = self.mods[index].state;
        self.mods.remove(index);
        if state != ModState::Disabled {
            self.requires_reload = true;
        }
        self.update_all_dependencies();
        self.sort_mods();
        self.reindex();
        Ok(())
    }

    /// Builds the harness report.
    pub fn report(&self) -> ModLoadReport {
        ModLoadReport {
            mod_directory: self.mod_directory.display().to_string(),
            mods: self.mods.iter().map(LoadedMod::list_entry).collect(),
            requires_reload: self.requires_reload,
        }
    }
}

impl LoadedMod {
    /// Converts to the serializable list row.
    pub fn list_entry(&self) -> ModListEntry {
        ModListEntry {
            name: self.name.clone(),
            display_name: self.meta.display_name().to_owned(),
            author: self.meta.author.clone(),
            version: self.meta.version().to_owned(),
            state: self.state,
            state_ordinal: self.state.ordinal(),
            enabled: self.enabled(),
            texturescale: self.meta.texturescale,
            min_game_version: self.meta.min_game_version.clone(),
            dependencies: self.dependencies.clone(),
            soft_dependencies: self.soft_dependencies.clone(),
            missing_dependencies: self.missing_dependencies.clone(),
            hidden: self.meta.hidden,
            java: self.java,
            has_scripts: self.has_scripts,
            source: match self.source {
                ModSource::LocalFolder => "folder",
                ModSource::LocalZip => "zip",
                ModSource::ImportedZip => "imported",
                ModSource::BrowserZip => "browser",
                ModSource::Workshop => "workshop",
            },
            short_description: self.meta.short_description(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MockFs;

    fn write_mod(fs: &MockFs, name: &str, meta: &str) {
        fs.write(
            &Path::new("/mods").join(name).join("mod.json"),
            meta.as_bytes(),
        )
        .expect("write mod.json");
    }

    /// Plan 20 M0: `state::disabled_by_settings`.
    #[test]
    fn disabled_by_settings() {
        let fs = MockFs::new();
        write_mod(&fs, "a", r#"{"name":"Alpha"}"#);
        write_mod(&fs, "b", r#"{"name":"Beta"}"#);
        let mut settings = SettingsStore::new();
        settings.put_bool("mod-beta-enabled", false);

        let mut mods = Mods::new(true, "/mods");
        let report = mods.load(&fs, Path::new("/mods"), &settings).expect("load");
        let alpha = report
            .mods
            .iter()
            .find(|m| m.name == "alpha")
            .expect("alpha");
        let beta = report.mods.iter().find(|m| m.name == "beta").expect("beta");
        assert_eq!(alpha.state, ModState::Enabled, "default is enabled");
        assert_eq!(beta.state, ModState::Disabled);
        assert!(alpha.enabled);
        assert!(!beta.enabled);
    }

    #[test]
    fn load_states_and_sort() {
        let fs = MockFs::new();
        write_mod(&fs, "app", r#"{"name":"App","dependencies":["lib"]}"#);
        write_mod(&fs, "lib", r#"{"name":"Lib"}"#);
        write_mod(
            &fs,
            "broken",
            r#"{"name":"Broken","dependencies":["nope"]}"#,
        );
        write_mod(&fs, "cycle", r#"{"name":"Cycle","dependencies":["cycle"]}"#);
        let settings = SettingsStore::new();

        let mut mods = Mods::new(true, "/mods");
        let report = mods.load(&fs, Path::new("/mods"), &settings).expect("load");
        let state = |name: &str| {
            report
                .mods
                .iter()
                .find(|m| m.name == name)
                .map(|m| m.state)
                .expect("exists")
        };
        assert_eq!(state("lib"), ModState::Enabled);
        assert_eq!(state("app"), ModState::Enabled);
        assert_eq!(state("broken"), ModState::MissingDependencies);
        assert_eq!(state("cycle"), ModState::CircularDependencies);
        // Sorted by state ordinal then name: enabled (app, lib) before
        // missing (broken) before circular (cycle).
        assert_eq!(report.mods[0].name, "app");
        assert_eq!(report.mods[1].name, "lib");
        assert_eq!(report.mods[2].name, "broken");
        assert_eq!(report.mods[3].name, "cycle");

        let ordered = mods.ordered_mods();
        let names: Vec<String> = ordered
            .iter()
            .map(|index| mods.mod_at(*index).expect("mod").name.clone())
            .collect();
        assert_eq!(names, vec!["lib".to_owned(), "app".to_owned()]);
    }

    #[test]
    fn config_folder_path() {
        let fs = MockFs::new();
        write_mod(&fs, "a", r#"{"name":"Alpha"}"#);
        let settings = SettingsStore::new();
        let mut mods = Mods::new(true, "/data/mods");
        mods.load(&fs, Path::new("/mods"), &settings).expect("load");
        let index = mods.get_mod("alpha").expect("alpha");
        let mod_ = mods.mod_at(index).expect("mod");
        assert_eq!(
            mod_.config_folder(Path::new("/data/mods")),
            PathBuf::from("/data/mods/alpha")
        );
        assert_eq!(
            mod_.config_file(Path::new("/data/mods")),
            PathBuf::from("/data/mods/alpha/config.json")
        );
    }

    #[test]
    fn incompatibility_and_fingerprints() {
        let fs = MockFs::new();
        write_mod(&fs, "a", r#"{"name":"Alpha","version":"1.0"}"#);
        write_mod(
            &fs,
            "hidden",
            r#"{"name":"Hidden","version":"2.0","hidden":true}"#,
        );
        let settings = SettingsStore::new();
        let mut mods = Mods::new(true, "/mods");
        mods.load(&fs, Path::new("/mods"), &settings).expect("load");
        assert_eq!(mods.get_mod_strings(), vec!["alpha:1.0".to_owned()]);

        let mut server = vec!["alpha:1.0".to_owned(), "server-only:3.0".to_owned()];
        let missing = mods.get_incompatibility(&mut server);
        assert!(missing.is_empty(), "client has everything the server has");
        assert_eq!(server, vec!["server-only:3.0".to_owned()]);

        let mut server = vec!["alpha:1.0".to_owned()];
        let missing = mods.get_incompatibility(&mut server);
        assert!(missing.is_empty());
    }

    #[test]
    fn set_enabled_sets_requires_reload() {
        let fs = MockFs::new();
        write_mod(&fs, "a", r#"{"name":"Alpha"}"#);
        let mut settings = SettingsStore::new();
        let mut mods = Mods::new(true, "/mods");
        mods.load(&fs, Path::new("/mods"), &settings).expect("load");
        assert!(!mods.requires_reload());
        mods.set_enabled("alpha", false, &mut settings);
        assert!(mods.requires_reload());
        assert_eq!(
            mods.mod_at(mods.get_mod("alpha").expect("a"))
                .expect("m")
                .state,
            ModState::Disabled
        );
    }

    #[test]
    fn scriptless_mod_unaffected_by_load_scripts() {
        let fs = MockFs::new();
        write_mod(&fs, "a", r#"{"name":"Alpha"}"#);
        let settings = SettingsStore::new();
        let mut mods = Mods::new(true, "/mods");
        mods.load(&fs, Path::new("/mods"), &settings).expect("load");
        mods.load_scripts(&fs);
        let index = mods.get_mod("alpha").expect("alpha");
        assert_eq!(mods.mod_at(index).expect("m").state, ModState::Enabled);
    }

    /// Plan 20 M8: `script::json_assets_still_load` — a script mod is marked
    /// unsupported but its record still loads (JSON/assets remain usable).
    #[test]
    fn script_mod_marked_unsupported() {
        let fs = MockFs::new();
        write_mod(&fs, "s", r#"{"name":"Scripted","minGameVersion":"146"}"#);
        fs.write(Path::new("/mods/s/scripts/main.js"), b"console.log('hi')")
            .expect("write script");
        let settings = SettingsStore::new();
        let mut mods = Mods::new(false, "/mods");
        mods.load(&fs, Path::new("/mods"), &settings).expect("load");
        let index = mods.get_mod("scripted").expect("scripted");
        assert!(mods.mod_at(index).expect("m").has_scripts);
        mods.load_scripts(&fs);
        let mod_ = mods
            .mod_at(mods.get_mod("scripted").expect("s"))
            .expect("m");
        assert_eq!(mod_.state, ModState::Unsupported);
        assert_eq!(
            mod_.unsupported_reason,
            Some(UnsupportedReason::ScriptsUnsupported)
        );
    }
}
