// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DataPatcher` (plan 20 §3.6): replace-only JSON data patches.
//!
//! M3: flat dotted keys and nested `type → name → fields` forms for the nine
//! content kinds, `Seq`/array (`+`, numeric index, whole replace),
//! `ObjectSet` (`immunities.+`), `consumes` power merge, `@NoPatch` refusal,
//! `requiredPlanets` gating, created-`afterPatch` on touched content, and
//! `unapply` restoring via recorded `ResetAction`s + the registry index
//! snapshot, created-object `init/postInit/load` on patch-created content, and
//! `fix_content_arrays` dense-table growth (registry + world halves). `ObjectMap`
//! /`ObjectFloatMap`/`Attributes` and per-mod `load_mod_patches` file wiring
//! landed in M3b (see the plan Changelog).

use indexmap::IndexSet;
use serde_json::{Map, Value};

use crate::content::parser_hooks::{PatchAsset, ResetAction};
use crate::content::snapshot::RegistryIndexSnapshot;
use crate::content::stacks::{ItemStack, LiquidStack};
use crate::content::{
    BlockId, Consume, ConsumeSpec, ContentError, ContentRef, ContentRegistry, ContentType, ItemId,
    PlanetId, StatusId, UnitTypeId,
};
use crate::io::FileSystem;

use super::Mods;

mod block;
mod content;
mod load;
mod scalar;
#[cfg(test)]
mod tests;
mod unit;

use load::content_type_from_name;

/// Fields refused by `@NoPatch` (plan 02 `PATCH_DENIED`).
pub const PATCH_DENIED: [&str; 3] = ["id", "size", "name"];

/// `DataPatcher.patchFormatVersion`.
pub const PATCH_FORMAT_VERSION: u32 = 2;

/// How a target field is addressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldMode {
    /// Replace the field.
    Set,
    /// Append to an array/`Seq`.
    Append,
    /// Assign array index `n`.
    Index(usize),
}

/// `mod/DataPatcher.java` equivalent.
pub struct DataPatcher {
    applied: bool,
    snapshot: Option<RegistryIndexSnapshot>,
    resetters: Vec<ResetAction>,
    used: IndexSet<(ContentRef, String)>,
    warnings: Vec<String>,
    after_patch_calls: usize,
    /// Content records created during this apply (`DataPatcher.created`); their
    /// `init`/`postInit`/client `load` run once after traversal.
    created: Vec<ContentRef>,
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
            after_patch_calls: 0,
            created: Vec::new(),
        }
    }

    /// Records a content reference constructed by this apply (`created`).
    pub(crate) fn mark_created(&mut self, reference: ContentRef) {
        if !self.created.contains(&reference) {
            self.created.push(reference);
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

    /// Number of `afterPatch` sweeps run (test/audit helper).
    pub fn after_patch_calls(&self) -> usize {
        self.after_patch_calls
    }

    /// Content references touched by the last apply (upstream `DataManager`
    /// copies `DataPatcher.used` into its `patched` set for `isPatched`).
    pub fn touched_contents(&self) -> Vec<ContentRef> {
        self.used.iter().map(|(reference, _)| *reference).collect()
    }

    /// Applies patches with no active planet (`requiredPlanets` never gates).
    pub fn apply(
        &mut self,
        registry: &mut ContentRegistry,
        patches: &[PatchAsset],
    ) -> Result<(), ContentError> {
        self.apply_with_planet(registry, patches, None)
    }

    /// Applies patches, replacing any previous application (never stacks).
    ///
    /// `active_planet` is the current rules planet (plan 12); a patch whose
    /// `requiredPlanets` excludes it is skipped exactly like upstream's
    /// `state.rules.planet` gate.
    pub fn apply_with_planet(
        &mut self,
        registry: &mut ContentRegistry,
        patches: &[PatchAsset],
        active_planet: Option<&str>,
    ) -> Result<(), ContentError> {
        if self.applied {
            self.unapply(registry);
        }
        self.snapshot = Some(registry.snapshot_index());
        self.warnings.clear();
        self.used.clear();
        self.resetters.clear();
        self.created.clear();
        self.after_patch_calls = 0;

        for patch in patches {
            // `PatcherTests.gibberish`: malformed patch JSON is a per-asset
            // warning, never fatal to the whole apply.
            let value: Value = match serde_json::from_str(&patch.json) {
                Ok(value) => value,
                Err(error) => {
                    self.warn(format!("{}: {error}", patch.name));
                    continue;
                }
            };
            let Some(object) = value.as_object() else {
                self.warn(format!("{}: patch must be a JSON object", patch.name));
                continue;
            };
            if Self::planet_gated(object, registry, active_planet) {
                continue;
            }
            for (key, value) in object {
                if matches!(key.as_str(), "name" | "requiredPlanets") {
                    continue;
                }
                self.apply_key(registry, key, value);
            }
        }
        // `DataPatcher.created`: init then postInit on each new record, and the
        // client `loadIcon`/`load` half (headless no-op).
        let created = std::mem::take(&mut self.created);
        for reference in &created {
            registry
                .init_created(*reference)
                .map_err(|error| ContentError::Parse(error.to_string()))?;
        }
        for reference in &created {
            registry
                .post_init_created(*reference)
                .map_err(|error| ContentError::Parse(error.to_string()))?;
        }
        for reference in &created {
            registry
                .load_created(*reference)
                .map_err(|error| ContentError::Parse(error.to_string()))?;
        }
        // `DataPatcher.fixContentArrays` runs before the `afterPatch` sweep so
        // the derived `build_time`/`health` re-derivation sees the grown item
        // tables (upstream order: content apply + `fixContentArrays`, then
        // `afterCallbacks`).
        fix_content_arrays(registry);
        // Created content runs `afterPatch()` once after the traversal.
        registry
            .after_patch()
            .map_err(|error| ContentError::Parse(error.to_string()))?;
        self.after_patch_calls += 1;
        self.applied = true;
        Ok(())
    }

    /// `requiredPlanets`: skip when the active planet is not listed.
    fn planet_gated(
        object: &Map<String, Value>,
        registry: &ContentRegistry,
        active_planet: Option<&str>,
    ) -> bool {
        let Some(active) = active_planet else {
            return false;
        };
        let Some(Value::Array(planets)) = object.get("requiredPlanets") else {
            return false;
        };
        let active_id = registry.planet_id(active);
        !planets.iter().any(|entry| {
            entry
                .as_str()
                .is_some_and(|name| registry.planet_id(name) == active_id && active_id.is_some())
        })
    }

    /// Restores the registry index and all touched fields in reverse order.
    pub fn unapply(&mut self, registry: &mut ContentRegistry) {
        if let Some(snapshot) = self.snapshot.take() {
            registry.restore_index(snapshot);
        }
        for resetter in self.resetters.drain(..).rev() {
            resetter(registry);
        }
        self.created.clear();
        self.used.clear();
        self.applied = false;
    }

    /// Handles one top-level patch key (flat dotted or nested type form).
    fn apply_key(&mut self, registry: &mut ContentRegistry, key: &str, value: &Value) {
        let Some((type_name, rest)) = key.split_once('.') else {
            // Nested form: `{ "block": { "<name>": { "<field>": v } } }` or the
            // scalar sugar `{ "block": { "<name>.<field>": v } }`.
            if let Some(map) = value.as_object() {
                for (name, fields) in map {
                    if let Some(fields) = fields.as_object() {
                        self.apply_fields(registry, key, name, fields);
                    } else if let Some((content, field)) = name.split_once('.') {
                        self.apply_path(registry, key, content, field, fields);
                    } else {
                        self.warn(format!("patch `{key}.{name}` must be an object"));
                    }
                }
            }
            return;
        };
        if let Some((name, path)) = rest.split_once('.') {
            self.apply_path(registry, type_name, name, path, value);
        } else if let Some(fields) = value.as_object() {
            self.apply_fields(registry, type_name, rest, fields);
        } else {
            self.warn(format!(
                "patch `{type_name}.{rest}` must be an object or a dotted field"
            ));
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
            self.apply_path(registry, content_type.name(), name, field, value);
        }
    }

    /// Splits an array suffix (`+`/index) off a dotted path.
    fn apply_path(
        &mut self,
        registry: &mut ContentRegistry,
        type_name: &str,
        name: &str,
        path: &str,
        value: &Value,
    ) {
        let Some(content_type) = content_type_from_name(type_name) else {
            self.warn(format!("unknown patch type `{type_name}`"));
            return;
        };
        let (field, mode) = if let Some(field) = path.strip_suffix(".+") {
            (field, FieldMode::Append)
        } else if let Some((field, index)) = path.rsplit_once('.') {
            match index.parse::<usize>() {
                Ok(index) => (field, FieldMode::Index(index)),
                Err(_) => (path, FieldMode::Set),
            }
        } else {
            (path, FieldMode::Set)
        };

        let name = if registry.get_by_name(content_type, name).is_none() {
            // Locate already-prefixed `dp-` content when the patch targets it.
            let prefixed = registry.transform_name(name);
            if registry.get_by_name(content_type, &prefixed).is_some() {
                prefixed
            } else {
                self.warn(format!("unknown {content_type} `{name}`"));
                return;
            }
        } else {
            name.to_owned()
        };

        self.apply_field(registry, content_type, &name, field, mode, value);
    }

    /// Dispatches one field assignment to the per-kind setter.
    fn apply_field(
        &mut self,
        registry: &mut ContentRegistry,
        content_type: ContentType,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        if mode == FieldMode::Set && PATCH_DENIED.contains(&field) {
            self.warn(format!("Field '{field}' cannot be edited."));
            return;
        }
        match content_type {
            ContentType::Block => self.apply_block(registry, name, field, mode, value),
            ContentType::Item => self.apply_item(registry, name, field, mode, value),
            ContentType::Liquid => self.apply_liquid(registry, name, field, mode, value),
            ContentType::Status => self.apply_status(registry, name, field, mode, value),
            ContentType::Unit => self.apply_unit(registry, name, field, mode, value),
            ContentType::Weather => self.apply_weather(registry, name, field, mode, value),
            ContentType::Planet => self.apply_planet(registry, name, field, mode, value),
            ContentType::Sector => self.apply_sector(registry, name, field, mode, value),
            ContentType::Team => self.apply_team(registry, name, field, mode, value),
            other => self.warn(format!("patching `{other}` is not supported")),
        }
    }
}

pub use load::*;
