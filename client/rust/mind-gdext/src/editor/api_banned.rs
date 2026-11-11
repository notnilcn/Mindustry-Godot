// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Banned-content dialog data: ban lists, kinds and the filter rows.
//! Upstream: core/src/mindustry/editor/BannedContentDialog.java

use super::*;

#[godot_api(secondary)]
impl MindEditor {
    // --- banned content (`BannedContentDialog`) ---

    /// Parses `"block"`/`"unit"` (everything else means blocks).
    pub(super) fn ban_kind(kind: &str) -> banned::BanKind {
        if kind == "unit" {
            banned::BanKind::Unit
        } else {
            banned::BanKind::Block
        }
    }

    /// `BannedContentDialog` picker payload `{categories, rows}`.
    ///
    /// Rows are every content type entry passing Java's `pred` (`Block::canBeBuilt`,
    /// `BannedContentDialog.java:36`; `!UnitType.isHidden()`, `:37`) in content-id
    /// order, `{name, id, category, ui_icon}`. `categories` is the block filter
    /// row (`Category.values()`, `:73`) and empty for units.
    #[func]
    pub fn bannable_content_json(&self, kind: GString) -> GString {
        let empty = "{\"categories\":[],\"rows\":[]}";
        let Some(content) = self.content() else {
            return GString::from(empty);
        };
        let kind = Self::ban_kind(&kind.to_string());
        let rows: Vec<serde_json::Value> = match kind {
            banned::BanKind::Block => content
                .blocks()
                .iter()
                .filter(|def| {
                    !matches!(
                        def.build_visibility,
                        BuildVisibility::Hidden | BuildVisibility::DebugOnly
                    )
                })
                .map(|def| {
                    serde_json::json!({
                        "name": def.name,
                        "id": def.id.raw(),
                        "category": def.category.name(),
                        "ui_icon": format!("block-{}-ui", def.name),
                    })
                })
                .collect(),
            banned::BanKind::Unit => content
                .units()
                .iter()
                .filter(|unit| !unit.is_hidden())
                .map(|unit| {
                    serde_json::json!({
                        "name": unit.name,
                        "id": unit.id.raw(),
                        "category": "",
                        "ui_icon": format!("unit-{}-ui", unit.name),
                    })
                })
                .collect(),
        };
        let categories: Vec<&str> = match kind {
            banned::BanKind::Block => Category::ALL
                .iter()
                .map(|category| category.name())
                .collect(),
            banned::BanKind::Unit => Vec::new(),
        };
        let payload = serde_json::json!({ "categories": categories, "rows": rows });
        GString::from(
            serde_json::to_string(&payload)
                .unwrap_or_else(|_| empty.to_owned())
                .as_str(),
        )
    }

    /// The map's banned set for `kind` from the `rules` tag
    /// (`state.rules.bannedBlocks`/`bannedUnits`), as a JSON array.
    #[func]
    pub fn banned_names_json(&self, kind: GString) -> GString {
        let kind = Self::ban_kind(&kind.to_string());
        let tag = self.editor.tags.get("rules").cloned().unwrap_or_default();
        let names = banned::names_from_tag_json(&tag, kind);
        GString::from(
            serde_json::to_string(&names)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// Replaces the banned set in the `rules` tag, preserving every other key
    /// (`BannedContentDialog` mutates the live `ObjectSet`; `CustomRulesDialog.java:189,210`).
    #[func]
    pub fn set_banned_names_json(&mut self, kind: GString, names_json: GString) -> bool {
        let kind = Self::ban_kind(&kind.to_string());
        let names: Vec<String> = match serde_json::from_str(&names_json.to_string()) {
            Ok(names) => names,
            Err(_) => return false,
        };
        let tag = self.editor.tags.get("rules").cloned().unwrap_or_default();
        match banned::apply_to_tag_json(&tag, kind, &names) {
            Ok(updated) => {
                self.editor.tags.insert("rules".to_owned(), updated);
                self.clear_error();
                true
            }
            Err(error) => {
                self.set_load_error(error);
                false
            }
        }
    }

    /// Monotonic tile-edit counter (`MindEditorRenderer` invalidation).
    #[func]
    pub fn tiles_revision(&self) -> i64 {
        self.tiles_rev as i64
    }

    /// Concise editor status for the inspector / MCP oracle.
    #[func]
    pub fn status(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        out.set(&key("tool"), &self.tool().to_variant());
        out.set(&key("tool_mode"), &self.tool_mode().to_variant());
        out.set(&key("brush_size"), &self.brush_size().to_variant());
        out.set(&key("draw_block"), &self.draw_block().to_variant());
        out.set(&key("draw_team"), &self.draw_team().to_variant());
        out.set(&key("rotation"), &self.rotation().to_variant());
        out.set(&key("show_terrain"), &self.show_terrain().to_variant());
        out.set(&key("show_floor"), &self.show_floor().to_variant());
        out.set(&key("show_buildings"), &self.show_buildings().to_variant());
        out.set(&key("grid"), &self.grid().to_variant());
        out.set(&key("width"), &(self.grid.width() as i64).to_variant());
        out.set(&key("height"), &(self.grid.height() as i64).to_variant());
        out.set(&key("can_undo"), &self.can_undo().to_variant());
        out.set(&key("can_redo"), &self.can_redo().to_variant());
        out.set(
            &key("retained_ops"),
            &(self.editor.retained_ops() as i64).to_variant(),
        );
        out.set(
            &key("current_ops"),
            &(self.editor.ops() as i64).to_variant(),
        );
        out.set(&key("saved"), &self.editor.saved.to_variant());
        out.set(&key("loading"), &self.editor.is_loading().to_variant());
        out.set(
            &key("file"),
            &GString::from(
                self.file
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_default()
                    .as_str(),
            )
            .to_variant(),
        );
        out.set(
            &key("last_error"),
            &GString::from(self.last_error_key.as_deref().unwrap_or("")).to_variant(),
        );
        out
    }

    /// Last map error as `{Key, message}` (`{}` when none).
    #[func]
    pub fn last_error(&self) -> VarDictionary {
        match (&self.last_error_key, &self.last_error_message) {
            (Some(key), Some(message)) => dict(vec![
                ("Key", GString::from(key.as_str()).to_variant()),
                ("message", GString::from(message.as_str()).to_variant()),
            ]),
            _ => VarDictionary::new(),
        }
    }
}
