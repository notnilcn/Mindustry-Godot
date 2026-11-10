// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Objective and marker class tables plus the objectives JSON.
//! Upstream: core/src/mindustry/editor/MapObjectivesDialog.java

use super::free::{objective_flag_tags, objective_kind_tag};
use super::*;

#[godot_api(secondary)]
impl MindEditor {
    /// `MapObjectivesDialog`'s type table
    /// (`MapObjectives.allObjectiveTypes` + `#[derive(ObjectiveFields)]`):
    /// every objective class with its editor field descriptors. The
    /// `kind`/`flags` tags are the GDScript ABI (`content:item`, `vec2i`,
    /// `seq:int`, flags `second`/`tilepos`/`multiline`/`logiccode`/
    /// `researchable`/`synthetic`/`hidden`).
    #[func]
    pub fn objective_classes_json(&self) -> GString {
        let rows: Vec<serde_json::Value> = mind_core::editor::objectives::all_class_tags()
            .iter()
            .map(|tag| {
                let fields: Vec<serde_json::Value> =
                    mind_core::editor::objectives::class_fields(tag)
                        .iter()
                        .map(|field| {
                            serde_json::json!({
                                "name": field.name,
                                "kind": objective_kind_tag(&field.kind),
                                "flags": objective_flag_tags(field.flags),
                            })
                        })
                        .collect();
                serde_json::json!({ "tag": tag, "fields": fields })
            })
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// A fresh objective of class `tag` with Java's provider defaults
    /// (`MapObjectivesDialog`'s `getProvider(MapObjective.class)`, `:213-240`)
    /// plus the canvas default `editorPos`.
    #[func]
    pub fn objective_default_json(&self, tag: GString) -> GString {
        let tag = tag.to_string();
        let mut map = serde_json::Map::new();
        map.insert("class".to_owned(), serde_json::Value::String(tag.clone()));
        let fields = mind_core::editor::objectives::class_fields(&tag);
        if fields.is_empty() {
            return GString::from("{}");
        }
        for field in fields {
            map.insert(
                field.name.to_owned(),
                mind_core::editor::objectives::provider_default(&field.kind, self.content.as_ref()),
            );
        }
        map.insert(
            "editorPos".to_owned(),
            serde_json::Value::from(mind_core::io::json::objectives::default_editor_pos()),
        );
        GString::from(
            serde_json::to_string(&serde_json::Value::Object(map))
                .unwrap_or_else(|_| "{}".to_owned())
                .as_str(),
        )
    }

    /// `MapObjectivesDialog`'s marker type table (`MapObjectives.allMarkerTypes`)
    /// with the same field-descriptor shape as `objective_classes_json`
    /// (`ObjectiveMarker` subclass reflection in `defaultInterpreter`).
    #[func]
    pub fn marker_classes_json(&self) -> GString {
        let rows: Vec<serde_json::Value> = mind_core::editor::objectives::all_marker_tags()
            .iter()
            .map(|tag| {
                let fields: Vec<serde_json::Value> =
                    mind_core::editor::objectives::marker_fields(tag)
                        .iter()
                        .map(|field| {
                            serde_json::json!({
                                "name": field.name,
                                "kind": objective_kind_tag(&field.kind),
                                "flags": objective_flag_tags(field.flags),
                            })
                        })
                        .collect();
                serde_json::json!({ "tag": tag, "fields": fields })
            })
            .collect();
        GString::from(
            serde_json::to_string(&rows)
                .unwrap_or_else(|_| "[]".to_owned())
                .as_str(),
        )
    }

    /// A fresh marker of class `tag` with the Java field defaults
    /// (`getProvider(ObjectiveMarker.class)`, `MapObjectivesDialog.java:236-256`).
    #[func]
    pub fn marker_default_json(&self, tag: GString) -> GString {
        let value = mind_core::editor::objectives::marker_default_json(&tag.to_string());
        GString::from(
            serde_json::to_string(&value)
                .unwrap_or_else(|_| "{}".to_owned())
                .as_str(),
        )
    }

    /// `MapObjectivesDialog`: the objectives JSON (`MapInfoDialog` route).
    #[func]
    pub fn objectives_json(&self) -> GString {
        match self.editor.tags.get("objectives") {
            Some(json) if !json.trim().is_empty() => GString::from(json.as_str()),
            _ => GString::from("[]"),
        }
    }

    /// Stores the objectives JSON.
    #[func]
    pub fn set_objectives_json(&mut self, json: GString) -> bool {
        let parsed = mind_core::editor::objectives::parse_objectives(&json.to_string());
        match parsed {
            Ok(objectives) => match mind_core::editor::objectives::write_objectives(&objectives) {
                Ok(canonical) => {
                    self.editor.tags.insert("objectives".to_owned(), canonical);
                    self.clear_error();
                    true
                }
                Err(error) => {
                    self.last_error_key = Some("@editor.errorload".to_owned());
                    self.last_error_message = Some(error.to_string());
                    false
                }
            },
            Err(error) => {
                self.last_error_key = Some("@editor.errorload".to_owned());
                self.last_error_message = Some(error.to_string());
                false
            }
        }
    }
}
