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

use godot::classes::{INode, Json, Node as GdNode, ProjectSettings};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::io::{NativeFs, SettingsStore};
use mind_core::mods::{ModListEntry, Mods};

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
}

#[godot_api]
impl MindMods {
    /// Emitted whenever the mod set or its enable flags change.
    #[signal]
    fn mods_changed();

    /// Emitted when a mod reports content errors / unsupported code.
    #[signal]
    fn mod_error(name: GString, details: GString);

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
    #[func]
    pub fn import_mod(&mut self, path: GString) -> VarDictionary {
        let source = PathBuf::from(path.to_string());
        let result = self
            .mods
            .import_mod(&NativeFs, &source, false, &mut self.settings);
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
}

/// Converts a `ModListEntry` to a Godot dictionary via its serde JSON form.
fn entry_to_dict(entry: &ModListEntry) -> VarDictionary {
    let value = serde_json::to_value(entry).unwrap_or(serde_json::Value::Null);
    let text = serde_json::to_string(&value).unwrap_or_else(|_| String::from("{}"));
    Json::parse_string(text.as_str())
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// Globalizes `user://mods` to a native path.
fn mods_dir() -> PathBuf {
    PathBuf::from(
        ProjectSettings::singleton()
            .globalize_path(MODS_PATH)
            .to_string(),
    )
}
