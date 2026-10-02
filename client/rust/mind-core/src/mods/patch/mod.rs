// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DataPatcher` (plan 20 §3.6): replace-only JSON data patches.
//!
//! M3 vertical slice: flat dotted keys and nested `type → name → fields`
//! forms for `block`/`item` scalar fields, `@NoPatch` refusal, unknown-field
//! warnings, and `unapply` restoring via recorded `ResetAction`s + the registry
//! index snapshot. `Seq`/`ObjectMap`/`consumes`/created-object semantics are the
//! M3 follow-up (see the plan Changelog).

use indexmap::IndexSet;
use serde_json::{Map, Value};

use crate::content::parser_hooks::{PatchAsset, ResetAction};
use crate::content::snapshot::RegistryIndexSnapshot;
use crate::content::{BlockId, ContentError, ContentRef, ContentRegistry, ContentType, ItemId};

/// Fields refused by `@NoPatch` (plan 02 `PATCH_DENIED`).
pub const PATCH_DENIED: [&str; 4] = ["id", "size", "name", "requirements"];

/// `DataPatcher.patchFormatVersion`.
pub const PATCH_FORMAT_VERSION: u32 = 2;

/// `mod/DataPatcher.java` equivalent.
pub struct DataPatcher {
    applied: bool,
    snapshot: Option<RegistryIndexSnapshot>,
    resetters: Vec<ResetAction>,
    used: IndexSet<(ContentRef, String)>,
    warnings: Vec<String>,
}

impl Default for DataPatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl DataPatcher {
    /// Empty patcher.
    pub fn new() -> Self {
        Self {
            applied: false,
            snapshot: None,
            resetters: Vec::new(),
            used: IndexSet::new(),
            warnings: Vec::new(),
        }
    }

    /// Whether patches are currently applied.
    pub fn is_applied(&self) -> bool {
        self.applied
    }

    /// Patch warnings from the last apply.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Applies patches, replacing any previous application (never stacks).
    pub fn apply(
        &mut self,
        registry: &mut ContentRegistry,
        patches: &[PatchAsset],
    ) -> Result<(), ContentError> {
        if self.applied {
            self.unapply(registry);
        }
        self.snapshot = Some(registry.snapshot_index());
        self.warnings.clear();
        self.used.clear();
        self.resetters.clear();

        for patch in patches {
            let value: Value = serde_json::from_str(&patch.json)
                .map_err(|error| ContentError::Parse(format!("{}: {error}", patch.name)))?;
            let Some(object) = value.as_object() else {
                return Err(ContentError::Parse(format!(
                    "{}: patch must be a JSON object",
                    patch.name
                )));
            };
            for (key, value) in object {
                if matches!(key.as_str(), "name" | "requiredPlanets") {
                    continue;
                }
                self.apply_key(registry, key, value);
            }
        }
        self.applied = true;
        Ok(())
    }

    /// Restores the registry index and all touched fields in reverse order.
    pub fn unapply(&mut self, registry: &mut ContentRegistry) {
        if let Some(snapshot) = self.snapshot.take() {
            registry.restore_index(snapshot);
        }
        for resetter in self.resetters.drain(..).rev() {
            resetter(registry);
        }
        self.used.clear();
        self.applied = false;
    }

    /// Handles one top-level patch key (flat dotted or nested type form).
    fn apply_key(&mut self, registry: &mut ContentRegistry, key: &str, value: &Value) {
        let Some((type_name, rest)) = key.split_once('.') else {
            // Nested form: `{ "block": { "<name>": { "<field>": v } } }`.
            if let Some(map) = value.as_object() {
                for (name, fields) in map {
                    if let Some(fields) = fields.as_object() {
                        self.apply_fields(registry, key, name, fields);
                    }
                }
            }
            return;
        };
        let Some(content_type) = content_type_from_name(type_name) else {
            self.warn(format!("unknown patch type `{type_name}`"));
            return;
        };
        if let Some((name, path)) = rest.split_once('.') {
            self.apply_scalar(registry, content_type, name, path, value);
        } else if let Some(fields) = value.as_object() {
            self.apply_fields(registry, type_name, rest, fields);
        }
    }

    fn apply_fields(
        &mut self,
        registry: &mut ContentRegistry,
        type_name: &str,
        name: &str,
        fields: &Map<String, Value>,
    ) {
        let Some(content_type) = content_type_from_name(type_name) else {
            self.warn(format!("unknown patch type `{type_name}`"));
            return;
        };
        for (field, value) in fields {
            self.apply_scalar(registry, content_type, name, field, value);
        }
    }

    fn apply_scalar(
        &mut self,
        registry: &mut ContentRegistry,
        content_type: ContentType,
        name: &str,
        field: &str,
        value: &Value,
    ) {
        if PATCH_DENIED.contains(&field) {
            self.warn(format!("Field '{field}' cannot be edited."));
            return;
        }
        match content_type {
            ContentType::Block => self.apply_block(registry, name, field, value),
            ContentType::Item => self.apply_item(registry, name, field, value),
            other => self.warn(format!(
                "patching `{other}` is not supported yet (M3 follow-up)"
            )),
        }
    }

    fn apply_block(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        value: &Value,
    ) {
        let Some(id) = registry.block_id(name) else {
            self.warn(format!("unknown block `{name}`"));
            return;
        };
        let reference = ContentRef::block(id);
        let raw = id.raw();
        match field {
            "health" => {
                if let Some(new) = value.as_i64() {
                    let new = new as i32;
                    let Some(original) = registry.block(id).map(|block| block.health) else {
                        return;
                    };
                    if self.mark_used(reference, field) {
                        self.resetters
                            .push(Box::new(move |r: &mut ContentRegistry| {
                                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                                    block.health = original;
                                }
                            }));
                    }
                    if let Some(block) = registry.block_mut(id) {
                        block.health = new;
                    }
                }
            }
            "armor" => {
                if let Some(new) = value.as_f64() {
                    let new = new as f32;
                    let Some(original) = registry.block(id).map(|block| block.armor) else {
                        return;
                    };
                    if self.mark_used(reference, field) {
                        self.resetters
                            .push(Box::new(move |r: &mut ContentRegistry| {
                                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                                    block.armor = original;
                                }
                            }));
                    }
                    if let Some(block) = registry.block_mut(id) {
                        block.armor = new;
                    }
                }
            }
            "itemCapacity" => {
                if let Some(new) = value.as_i64() {
                    let new = new as i32;
                    let Some(original) = registry.block(id).map(|block| block.item_capacity) else {
                        return;
                    };
                    if self.mark_used(reference, field) {
                        self.resetters
                            .push(Box::new(move |r: &mut ContentRegistry| {
                                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                                    block.item_capacity = original;
                                }
                            }));
                    }
                    if let Some(block) = registry.block_mut(id) {
                        block.item_capacity = new;
                    }
                }
            }
            "solid" => {
                if let Some(new) = value.as_bool() {
                    let Some(original) = registry.block(id).map(|block| block.solid) else {
                        return;
                    };
                    if self.mark_used(reference, field) {
                        self.resetters
                            .push(Box::new(move |r: &mut ContentRegistry| {
                                if let Some(block) = r.block_mut(BlockId::new(raw)) {
                                    block.solid = original;
                                }
                            }));
                    }
                    if let Some(block) = registry.block_mut(id) {
                        block.solid = new;
                    }
                }
            }
            _ => self.warn(format!("unknown field `block.{name}.{field}`")),
        }
    }

    fn apply_item(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        value: &Value,
    ) {
        let Some(id) = registry.item_id(name) else {
            self.warn(format!("unknown item `{name}`"));
            return;
        };
        let reference = ContentRef::item(id);
        match field {
            "hardness" => {
                if let Some(new) = value.as_i64() {
                    if self.mark_used(reference, field) {
                        let Some(original) = registry.item(id).map(|item| item.hardness) else {
                            return;
                        };
                        let raw = id.raw();
                        self.resetters
                            .push(Box::new(move |r: &mut ContentRegistry| {
                                if let Some(item) = r.item_mut(ItemId::new(raw)) {
                                    item.hardness = original;
                                }
                            }));
                    }
                    if let Some(item) = registry.item_mut(id) {
                        item.hardness = new as i32;
                    }
                }
            }
            "cost" => {
                if let Some(new) = value.as_f64() {
                    if self.mark_used(reference, field) {
                        let Some(original) = registry.item(id).map(|item| item.cost) else {
                            return;
                        };
                        let raw = id.raw();
                        self.resetters
                            .push(Box::new(move |r: &mut ContentRegistry| {
                                if let Some(item) = r.item_mut(ItemId::new(raw)) {
                                    item.cost = original;
                                }
                            }));
                    }
                    if let Some(item) = registry.item_mut(id) {
                        item.cost = new as f32;
                    }
                }
            }
            _ => self.warn(format!("unknown field `item.{name}.{field}`")),
        }
    }

    /// Records `(content, field)` once; returns whether a reset is needed.
    fn mark_used(&mut self, reference: ContentRef, field: &str) -> bool {
        self.used.insert((reference, field.to_owned()))
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}

/// Parses a top-level patch type name (`block`/`item`; M2 adds the rest).
fn content_type_from_name(name: &str) -> Option<ContentType> {
    match name {
        "block" | "blocks" => Some(ContentType::Block),
        "item" | "items" => Some(ContentType::Item),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    fn patch(json: &str) -> PatchAsset {
        PatchAsset {
            name: String::from("test"),
            json: json.to_owned(),
        }
    }

    /// Plan 20 M3: `patch::block_health_edit_and_unapply`.
    #[test]
    fn block_health_edit_and_unapply() {
        let mut registry = test_registry();
        let id = registry.block_id("router").expect("router");
        let original = registry.block(id).expect("block").health;
        let mut patcher = DataPatcher::new();
        patcher
            .apply(&mut registry, &[patch(r#"{"block.router.health": 999}"#)])
            .expect("apply");
        assert_eq!(registry.block(id).expect("block").health, 999);
        assert!(patcher.is_applied());
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").health, original);
        assert!(!patcher.is_applied());
    }

    /// Plan 20 M3: `patch::nested_form_edits`.
    #[test]
    fn nested_form_edits() {
        let mut registry = test_registry();
        let id = registry.item_id("copper").expect("copper");
        let original = registry.item(id).expect("item").hardness;
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"item": {"copper": {"hardness": 7}}}"#)],
            )
            .expect("apply");
        assert_eq!(registry.item(id).expect("item").hardness, 7);
        patcher.unapply(&mut registry);
        assert_eq!(registry.item(id).expect("item").hardness, original);
    }

    /// Plan 20 M3: `patch::no_patch_field_refused`.
    #[test]
    fn no_patch_field_refused() {
        let mut registry = test_registry();
        let id = registry.block_id("router").expect("router");
        let original = registry.block(id).expect("block").size;
        let mut patcher = DataPatcher::new();
        patcher
            .apply(&mut registry, &[patch(r#"{"block.router.size": 5}"#)])
            .expect("apply");
        assert_eq!(registry.block(id).expect("block").size, original);
        assert_eq!(patcher.warnings().len(), 1);
        assert!(patcher.warnings()[0].contains("cannot be edited"));
    }

    /// Plan 20 M3: `patch::unknown_fields_warn`.
    #[test]
    fn unknown_fields_warn() {
        let mut registry = test_registry();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"block.router.bogus": 1, "block.nope.health": 2}"#,
                )],
            )
            .expect("apply");
        assert_eq!(patcher.warnings().len(), 2);
    }

    /// Plan 20 M3: `patch::replace_only_never_stacks`.
    #[test]
    fn replace_only_never_stacks() {
        let mut registry = test_registry();
        let id = registry.block_id("router").expect("router");
        let original = registry.block(id).expect("block").health;
        let mut patcher = DataPatcher::new();
        patcher
            .apply(&mut registry, &[patch(r#"{"block.router.health": 100}"#)])
            .expect("apply");
        patcher
            .apply(&mut registry, &[patch(r#"{"block.router.health": 200}"#)])
            .expect("apply again");
        assert_eq!(registry.block(id).expect("block").health, 200);
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").health, original);
    }
}
