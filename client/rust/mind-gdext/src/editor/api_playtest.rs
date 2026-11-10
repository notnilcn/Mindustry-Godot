// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The editor playtest lifecycle (`edit_in_game`/`return_to_editor`).
//! Upstream: core/src/mindustry/editor/MapEditorDialog.java

use super::*;

#[godot_api(secondary)]
impl MindEditor {
    // --- playtest lifecycle (plan 19 M7 §3.11) ---

    /// `MapEditorDialog.editInGame()`: snapshot rules, apply the hidden `editor`
    /// gamemode and enter a synthetic play session.
    #[func]
    pub fn edit_in_game(&mut self) -> VarDictionary {
        let events = self
            .play
            .edit_in_game(&mut self.editor, &mut self.play_session);
        self.clear_error();
        dict(vec![
            ("ok", true.to_variant()),
            ("playing", true.to_variant()),
            (
                "phase",
                GString::from(self.play_session.phase.name()).to_variant(),
            ),
            ("rules_editor", self.play_session.rules.editor.to_variant()),
            ("events", (events.len() as i64).to_variant()),
        ])
    }

    /// `MapEditorDialog.playtest()`: save first, then open `MapPlayDialog` in
    /// playtesting mode (`dialog: true`). Java's Shift shortcut (auto-pick a
    /// gamemode and play immediately) is not exposed by the facade.
    #[func]
    pub fn playtest(&mut self) -> VarDictionary {
        let name = self.editor.tags.get("name").cloned().unwrap_or_default();
        if name.trim().is_empty() {
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from("noname").to_variant()),
            ]);
        }
        // `MapEditorDialog.playtest` aborts when `save()` returns null (builtin
        // maps refuse the overwrite); the save error reaches the caller.
        let saved = self.save();
        if !saved
            .get("ok")
            .and_then(|value| value.try_to::<bool>().ok())
            .unwrap_or(false)
        {
            let error = saved
                .get("error")
                .and_then(|value| value.try_to::<GString>().ok())
                .map(|value| value.to_string())
                .unwrap_or_else(|| "save".to_owned());
            return dict(vec![
                ("ok", false.to_variant()),
                ("error", GString::from(error.as_str()).to_variant()),
            ]);
        }
        dict(vec![
            ("ok", true.to_variant()),
            ("dialog", true.to_variant()),
        ])
    }

    /// `MapEditorDialog.resumeEditing()`: return to the menu with the pre-playtest
    /// rules restored.
    #[func]
    pub fn resume_editing(&mut self) -> bool {
        self.mark_tiles_changed();
        self.play
            .resume_editing(&mut self.editor, &mut self.play_session);
        true
    }

    /// `MapEditorDialog.resumeAfterPlaytest(map)` (`MapEditorDialog.java:370-372`):
    /// `beginEditMap(map.file)` — `shownWithMap = true` and the map the
    /// playtest saved is reopened (`:508-519`). Called by
    /// `MindCampaign.check_playtest` when a playtest ends (pause-menu quit or
    /// game-over `@menu`); the caller shows the shell.
    #[func]
    pub fn resume_after_playtest(&mut self, path: GString) -> bool {
        self.play.playtesting = false;
        let opened = self.begin_edit_map(path);
        if opened {
            self.editor.shown_with_map = true;
        }
        opened
    }

    /// `EditorMapsDialog.tryImportMap` (`EditorMapsDialog.java:55-98`): rejects
    /// images, generates `unknown<N>` for nameless maps and reports name
    /// conflicts. Returns `""` on success, `"image"`, `"exists:<name>"` (a
    /// built-in), `"overwrite:<name>"` (a custom map; re-call with `overwrite`)
    /// or `"error"`.
    #[func]
    pub fn import_map(&mut self, path: GString, overwrite: bool) -> GString {
        let native = ProjectSettings::singleton().globalize_path(&path);
        let from = std::path::PathBuf::from(native.to_string());
        if !NativeFs.exists(&from) || MapIo::is_image(&NativeFs, &from) {
            return GString::from("image");
        }
        let Ok(header) = MapIo::create_map(&NativeFs, &from, true) else {
            return GString::from("error");
        };
        let root = crate::assets::loader::user_data_root();
        let paths = mind_core::io::fs::Paths::new(&root);
        let dir = paths.maps();
        let mut maps = crate::assets::loader::load_map_registry(&root);
        // Java's fallback loop returns `unknown<first free index + 1>`
        // (`while(maps.byName(result + number++) != null);`).
        let mut name = header.name.trim().to_owned();
        if name.is_empty() {
            let mut number = 0i32;
            loop {
                let candidate = format!("unknown{number}");
                number += 1;
                if !maps
                    .all()
                    .iter()
                    .any(|map| map.name().eq_ignore_ascii_case(&candidate))
                {
                    break;
                }
            }
            name = format!("unknown{number}");
        }
        if let Some(conflict) = maps
            .all()
            .iter()
            .find(|map| map.name().eq_ignore_ascii_case(&name))
            .cloned()
        {
            if !conflict.custom {
                return GString::from(format!("exists:{name}").as_str());
            }
            if !overwrite {
                return GString::from(format!("overwrite:{name}").as_str());
            }
            let _ = maps.remove_map(&NativeFs, &paths, conflict.name());
        }
        match maps.import_map(&NativeFs, &dir, &from) {
            Ok(_) => {
                self.clear_error();
                GString::new()
            }
            Err(error) => {
                log::warn!("import_map `{}`: {error}", from.display());
                GString::from("error")
            }
        }
    }

    /// `EditorMapsDialog.showMap`'s `@delete`: removes a custom map and its
    /// preview/cache (`Maps.removeMap`). Returns false for built-in/unknown
    /// maps (the button is disabled for those).
    #[func]
    pub fn delete_map(&mut self, name: GString) -> bool {
        let root = crate::assets::loader::user_data_root();
        let paths = mind_core::io::fs::Paths::new(&root);
        let mut maps = crate::assets::loader::load_map_registry(&root);
        let wanted = name.to_string();
        let Some(map) = maps.all().iter().find(|map| map.name() == wanted).cloned() else {
            return false;
        };
        if !map.custom {
            return false;
        }
        maps.remove_map(&NativeFs, &paths, map.name()).is_ok()
    }

    /// `MapEditorDialog.tryExit()`: always requires the unsaved-changes confirm.
    #[func]
    pub fn try_exit(&mut self) -> bool {
        self.play.try_exit()
    }

    /// Playtest lifecycle state for the inspector / MCP oracle.
    #[func]
    pub fn playtest_status(&self) -> VarDictionary {
        dict(vec![
            (
                "phase",
                GString::from(self.play_session.phase.name()).to_variant(),
            ),
            ("editor", self.play_session.rules.editor.to_variant()),
            ("playtesting", self.play.playtesting.to_variant()),
            (
                "has_snapshot",
                self.play.last_saved_rules.is_some().to_variant(),
            ),
            ("saved", self.editor.saved.to_variant()),
        ])
    }
}
