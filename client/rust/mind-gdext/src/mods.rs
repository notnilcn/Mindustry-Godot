// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindMods` autoload (plan 20 M6/§3.9): the GDScript-callable mod service.
//!
//! Discovery/metadata/dependencies live in `mind_core::mods`; this node is the
//! thin Godot shell that scans `user://mods`, persists enable flags and exposes
//! the `list/import/set_enabled/remove/requires_reload/errors/mod_strings` API
//! plus `mods_changed`/`mod_error` signals for the plan-14 dialogs.
//!
//! The `ModsDialog`/`ModBrowserDialog` widgets themselves are plan 14 (not
//! landed); the browser's HTTP + `.zip` download stay in GDScript. In-engine
//! MCP verification is deferred to the orchestrator's single-editor mutex.

use std::path::PathBuf;

use godot::classes::notify::NodeNotification;
use godot::classes::{INode, Json, Node as GdNode, ProjectSettings};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::io::{NativeFs, SettingsStore};
use mind_core::mods::{ModListEntry, ModListing, Mods};
use mind_core::version::BuildInfo;

/// Mods directory in Godot user space (`<data>/mods/`).
const MODS_PATH: &str = "user://mods";

/// The mod service autoload.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindMods {
    base: Base<GdNode>,
    mods: Mods,
    settings: SettingsStore,
}

#[godot_api]
impl INode for MindMods {
    fn init(base: Base<GdNode>) -> Self {
        MindMods {
            base,
            mods: Mods::new(false, PathBuf::new()),
            settings: SettingsStore::new(),
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }
}

#[godot_api]
impl MindMods {
    /// Rebuilds the mod registry (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`). The `Mods`
    /// instance is replaced before loading, so a re-run never duplicates.
    fn bootstrap(&mut self) {
        let dir = mods_dir();
        if let Err(error) = std::fs::create_dir_all(&dir) {
            log::warn!("[mods] could not create `{}`: {error}", dir.display());
        }
        self.mods = Mods::new(false, dir.clone());
        match self.mods.load(&NativeFs, &dir, &self.settings) {
            Ok(report) => {
                log::info!("mods loaded: {} mod(s)", report.mods.len());
            }
            Err(error) => log::warn!("[mods] load failed: {error}"),
        }
    }

    /// Emitted whenever the mod set or its enable flags change.
    #[signal]
    fn mods_changed();

    /// Emitted when a mod reports content errors / unsupported code.
    #[signal]
    fn mod_error(name: GString, details: GString);

    /// Emitted around an import (`path`, `ratio` 0.0→1.0). The Rust import is
    /// synchronous (`Mods::import_mod`); the two endpoints are the honest
    /// progress a local import can report. The browser's streaming download
    /// progress lives in `mod_browser_dialog.gd` (`HTTPRequest`).
    #[signal]
    fn mod_import_progress(path: GString, ratio: f32);

    /// Serialized mod rows (`ModListEntry`), sorted like the dialog list.
    #[func]
    pub fn list(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        for entry in &self.mods.report().mods {
            out.push(&entry_to_dict(entry));
        }
        out
    }

    /// One mod row by internal name (`{}` when absent).
    #[func]
    pub fn get(&self, name: GString) -> VarDictionary {
        let target = name.to_string();
        self.mods
            .report()
            .mods
            .iter()
            .find(|entry| entry.name == target)
            .map(entry_to_dict)
            .unwrap_or_default()
    }

    /// Sets `mod-<name>-enabled`; sets `requiresReload` for non-disabled mods.
    #[func]
    pub fn set_enabled(&mut self, name: GString, enabled: bool) {
        let target = name.to_string();
        self.mods.set_enabled(&target, enabled, &mut self.settings);
        let _ = self.base_mut().emit_signal("mods_changed", &[]);
    }

    /// Imports a `.zip`/`.jar`/folder into the mods directory.
    ///
    /// The browser hands in a `user://modcache/...` path; `NativeFs` only speaks
    /// native paths, so localized Godot paths are globalized first (a native
    /// path from the file chooser passes through unchanged).
    #[func]
    pub fn import_mod(&mut self, path: GString) -> VarDictionary {
        let global = ProjectSettings::singleton().globalize_path(path.to_string().as_str());
        let source = PathBuf::from(global.to_string());
        let _ = self.base_mut().emit_signal(
            "mod_import_progress",
            &[path.to_variant(), 0.0f32.to_variant()],
        );
        let result = self
            .mods
            .import_mod(&NativeFs, &source, false, &mut self.settings);
        let _ = self.base_mut().emit_signal(
            "mod_import_progress",
            &[path.to_variant(), 1.0f32.to_variant()],
        );
        let mut out = VarDictionary::new();
        match result {
            Ok(_name) => {
                out.set(&"ok".to_variant(), &true.to_variant());
                out.set(
                    &"requiresReload".to_variant(),
                    &self.mods.requires_reload().to_variant(),
                );
                let _ = self.base_mut().emit_signal("mods_changed", &[]);
            }
            Err(error) => {
                out.set(&"ok".to_variant(), &false.to_variant());
                out.set(&"error".to_variant(), &error.to_string().to_variant());
                let _ = self.base_mut().emit_signal(
                    "mod_error",
                    &[path.to_variant(), error.to_string().to_variant()],
                );
            }
        }
        out
    }

    /// Removes a mod from the mods directory.
    #[func]
    pub fn remove_mod(&mut self, name: GString) -> VarDictionary {
        let target = name.to_string();
        let mut out = VarDictionary::new();
        match self.mods.remove_mod(&NativeFs, &target) {
            Ok(()) => {
                out.set(&"ok".to_variant(), &true.to_variant());
                out.set(
                    &"requiresReload".to_variant(),
                    &self.mods.requires_reload().to_variant(),
                );
                let _ = self.base_mut().emit_signal("mods_changed", &[]);
            }
            Err(error) => {
                out.set(&"ok".to_variant(), &false.to_variant());
                out.set(&"error".to_variant(), &error.to_string().to_variant());
            }
        }
        out
    }

    /// Whether a restart is required (`@mod.reloadrequired`).
    #[func]
    pub fn requires_reload(&self) -> bool {
        self.mods.requires_reload()
    }

    /// Whether any enabled mod ships scripts (OD1: surfaced, not run).
    #[func]
    pub fn has_scripts(&self) -> bool {
        self.mods.list().iter().any(|mod_| mod_.has_scripts)
    }

    /// Content/unsupported errors as `{name, details}` rows.
    #[func]
    pub fn errors(&self) -> Array<VarDictionary> {
        let mut out = Array::<VarDictionary>::new();
        for mod_ in self.mods.list() {
            let mut details = Vec::new();
            if let Some(reason) = &mod_.unsupported_reason {
                details.push(reason.message().to_owned());
            }
            if !mod_.errored_content.is_empty() {
                details.push(format!("{} content error(s)", mod_.errored_content.len()));
            }
            if details.is_empty() {
                continue;
            }
            let mut row = VarDictionary::new();
            row.set(&"name".to_variant(), &mod_.name.to_variant());
            row.set(&"details".to_variant(), &details.join("\n").to_variant());
            out.push(&row);
        }
        out
    }

    /// `name:version` strings for the plan-21 join handshake.
    #[func]
    pub fn mod_strings(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for value in self.mods.get_mod_strings() {
            out.push(&GString::from(value.as_str()));
        }
        out
    }

    /// Rescans the mods directory (still requires a reload for content).
    #[func]
    pub fn reload(&mut self) {
        let dir = mods_dir();
        self.mods.load(&NativeFs, &dir, &self.settings).ok();
        let _ = self.base_mut().emit_signal("mods_changed", &[]);
    }

    /// Globalized mods directory (for the file chooser/import UI).
    #[func]
    pub fn mods_dir(&self) -> GString {
        GString::from(mods_dir().display().to_string().as_str())
    }

    /// Full detail row for the ModsDialog details pane (`showMod`).
    #[func]
    pub fn details(&self, name: GString) -> VarDictionary {
        let target = name.to_string();
        let report = self.mods.report();
        let Some(entry) = report.mods.iter().find(|entry| entry.name == target) else {
            return VarDictionary::new();
        };
        let mut out = entry_to_dict(entry);
        if let Some(mod_) = self.mods.list().iter().find(|mod_| mod_.name == target) {
            out.set(
                &"description".to_variant(),
                &mod_
                    .meta
                    .description
                    .clone()
                    .unwrap_or_default()
                    .to_variant(),
            );
            out.set(
                &"subtitle".to_variant(),
                &mod_.meta.subtitle.clone().unwrap_or_default().to_variant(),
            );
            out.set(
                &"config_folder".to_variant(),
                &mod_
                    .config_folder(self.mods.mod_directory())
                    .display()
                    .to_string()
                    .to_variant(),
            );
            out.set(
                &"file".to_variant(),
                &mod_.file.display().to_string().to_variant(),
            );
            out.set(
                &"repo".to_variant(),
                &mod_.meta.repo.clone().unwrap_or_default().to_variant(),
            );
            out.set(
                &"failed".to_variant(),
                &mod_.failed(&self.settings).to_variant(),
            );
            let reason = mod_
                .unsupported_reason
                .as_ref()
                .map(|reason| reason.message().to_owned())
                .unwrap_or_default();
            out.set(&"reason".to_variant(), &reason.to_variant());
            out.set(
                &"errored_content".to_variant(),
                &(mod_.errored_content.len() as i64).to_variant(),
            );
            out.set(
                &"soft_dependencies".to_variant(),
                &string_array(&mod_.soft_dependencies),
            );
            out.set(
                &"missing_soft_dependencies".to_variant(),
                &string_array(&mod_.missing_soft_dependencies),
            );
        }
        out
    }

    /// Parses a GitHub mod listing JSON (`ModListing`) into a dictionary.
    ///
    /// The browser fetches the JSON with `HTTPRequest`; parsing/version matching
    /// stay in `mind-core` so the dialog does not re-implement `parseVersion`/
    /// `matchesGameVersion`.
    #[func]
    pub fn parse_listing(&self, json_text: GString) -> VarDictionary {
        let mut out = VarDictionary::new();
        match serde_json::from_str::<ModListing>(&json_text.to_string()) {
            Ok(listing) => {
                out.set(&"ok".to_variant(), &true.to_variant());
                let value = serde_json::to_value(&listing).unwrap_or(serde_json::Value::Null);
                out.set(&"listing".to_variant(), &json_to_dict(&value));
            }
            Err(error) => {
                out.set(&"ok".to_variant(), &false.to_variant());
                out.set(&"error".to_variant(), &error.to_string().to_variant());
            }
        }
        out
    }

    /// `ModListing.getMatchingRelease`: the release matching `build`/`revision`.
    ///
    /// Returns `{found, id, version}`; `found = false` means "use `/latest`".
    #[func]
    pub fn matching_release(
        &self,
        listing_json: GString,
        build: i32,
        revision: i32,
    ) -> VarDictionary {
        let mut out = VarDictionary::new();
        let Ok(listing) = serde_json::from_str::<ModListing>(&listing_json.to_string()) else {
            out.set(&"found".to_variant(), &false.to_variant());
            return out;
        };
        match listing.get_matching_release(build, revision) {
            Some(release) => {
                out.set(&"found".to_variant(), &true.to_variant());
                out.set(&"id".to_variant(), &release.id.to_variant());
                out.set(&"version".to_variant(), &release.version.to_variant());
            }
            None => {
                out.set(&"found".to_variant(), &false.to_variant());
            }
        }
        out
    }

    /// The embedded game build (`Version.build`; `-1` for a custom build).
    ///
    /// The browser passes this to `matching_release` instead of guessing.
    #[func]
    pub fn game_build(&self) -> i32 {
        BuildInfo::embedded().build
    }

    /// The embedded game revision (`Version.revision`).
    #[func]
    pub fn game_revision(&self) -> i32 {
        BuildInfo::embedded().revision as i32
    }

    /// `BuildInfo` fields for the dialog footer / version-mismatch checks.
    #[func]
    pub fn version_info(&self) -> VarDictionary {
        let info = BuildInfo::embedded();
        let mut out = VarDictionary::new();
        out.set(&"build".to_variant(), &info.build.to_variant());
        out.set(
            &"revision".to_variant(),
            &(info.revision as i64).to_variant(),
        );
        out.set(
            &"buildString".to_variant(),
            &info.build_string().to_variant(),
        );
        out.set(&"combined".to_variant(), &info.combined().to_variant());
        out
    }

    /// `Mods.getConfigFolder` path for a mod (`user://mods/<internal-name>`).
    #[func]
    pub fn config_folder(&self, name: GString) -> GString {
        let target = name.to_string();
        self.mods
            .list()
            .iter()
            .find(|mod_| mod_.name == target)
            .map(|mod_| {
                GString::from(
                    mod_.config_folder(self.mods.mod_directory())
                        .display()
                        .to_string()
                        .as_str(),
                )
            })
            .unwrap_or_default()
    }
}

/// Packs a `Vec<String>` into a Godot string array.
fn string_array(values: &[String]) -> PackedStringArray {
    let mut out = PackedStringArray::new();
    for value in values {
        out.push(&GString::from(value.as_str()));
    }
    out
}

/// Converts a `serde_json::Value` to a Godot `VarDictionary`.
fn json_to_dict(value: &serde_json::Value) -> VarDictionary {
    let text = serde_json::to_string(value).unwrap_or_else(|_| String::from("{}"));
    Json::parse_string(text.as_str())
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// Converts a `ModListEntry` to a Godot dictionary via its serde JSON form.
fn entry_to_dict(entry: &ModListEntry) -> VarDictionary {
    json_to_dict(&serde_json::to_value(entry).unwrap_or(serde_json::Value::Null))
}

/// Globalizes `user://mods` to a native path.
fn mods_dir() -> PathBuf {
    PathBuf::from(
        ProjectSettings::singleton()
            .globalize_path(MODS_PATH)
            .to_string(),
    )
}
