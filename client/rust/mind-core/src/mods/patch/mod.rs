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

    // ---- block ----

    #[allow(clippy::too_many_lines)]
    fn apply_block(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.block_id(name) else {
            self.warn(format!("unknown block `{name}`"));
            return;
        };
        let reference = ContentRef::block(id);
        match field {
            "requirements" => {
                self.edit_item_requirements(registry, reference, id.raw(), mode, value)
            }
            "plans" => self.edit_unit_plans(registry, reference, id.raw(), mode, value),
            field if field == "upgrades" || field.starts_with("upgrades.") => {
                self.edit_reconstructor_upgrades(registry, reference, id.raw(), field, mode, value)
            }
            "consumes" => self.edit_consumes(registry, reference, id, mode, value),
            field if field == "attributes" || field.starts_with("attributes.") => {
                self.edit_attributes(registry, reference, id.raw(), field, value)
            }
            field if field == "drillMultipliers" || field.starts_with("drillMultipliers.") => {
                self.edit_drill_multipliers(registry, reference, id.raw(), field, value)
            }
            "health" => self.set_i32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.health),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.health = v;
                    }
                },
            ),
            "armor" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.armor),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.armor = v;
                    }
                },
            ),
            "itemCapacity" => self.set_i32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.item_capacity),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.item_capacity = v;
                    }
                },
            ),
            "liquidCapacity" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.liquid_capacity),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.liquid_capacity = v;
                    }
                },
            ),
            "solid" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.solid),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.solid = v;
                    }
                },
            ),
            "floating" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.floating),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.floating = v;
                    }
                },
            ),
            "update" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.update),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.update = v;
                    }
                },
            ),
            "destructible" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.block(id).map(|b| b.destructible),
                move |r, v| {
                    if let Some(b) = r.block_mut(BlockId::new(id.raw())) {
                        b.destructible = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `block.{name}.{field}`")),
        }
    }

    /// Edits `block.requirements` (`Seq<ItemStack>`).
    fn edit_item_requirements(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        mode: FieldMode,
        value: &Value,
    ) {
        // Object-index form (`PatcherTests.specificArrayRequirements`):
        // `{"0": "surge-alloy/10"}` assigns by index.
        if mode == FieldMode::Set
            && let Value::Object(map) = value
            && !map.is_empty()
            && map.keys().all(|key| key.parse::<usize>().is_ok())
        {
            let mut edits: Vec<(usize, ItemStack)> = Vec::new();
            for (key, entry) in map {
                let Ok(index) = key.parse::<usize>() else {
                    continue;
                };
                match self.parse_item_stacks(registry, entry) {
                    Ok(stacks) if !stacks.is_empty() => edits.push((index, stacks[0])),
                    Ok(_) => {}
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                }
            }
            let Some(original) = registry
                .block(BlockId::new(raw))
                .map(|b| b.requirements.clone())
            else {
                return;
            };
            if self.mark_used(reference, "requirements") {
                self.resetters
                    .push(Box::new(move |r: &mut ContentRegistry| {
                        if let Some(block) = r.block_mut(BlockId::new(raw)) {
                            block.requirements = original;
                        }
                    }));
            }
            if let Some(block) = registry.block_mut(BlockId::new(raw)) {
                for (index, stack) in edits {
                    if index < block.requirements.len() {
                        block.requirements[index] = stack;
                    }
                }
            }
            return;
        }
        let stacks = match self.parse_item_stacks(registry, value) {
            Ok(stacks) => stacks,
            Err(message) => {
                self.warn(message);
                return;
            }
        };
        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|b| b.requirements.clone())
        else {
            return;
        };
        if self.mark_used(reference, "requirements") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(block) = r.block_mut(BlockId::new(raw)) {
                        block.requirements = original;
                    }
                }));
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            match mode {
                FieldMode::Set => block.requirements = stacks,
                FieldMode::Append => block.requirements.extend(stacks),
                FieldMode::Index(index) => {
                    if index < block.requirements.len() {
                        block.requirements[index] = stacks[0];
                    }
                }
            }
        }
    }

    /// Edits `block.<name>.plans` (`Seq<UnitPlan>`): whole replace, `+` append
    /// (single or array) and numeric index (`PatcherTests.unitFactoryPlans`).
    fn edit_unit_plans(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        mode: FieldMode,
        value: &Value,
    ) {
        use crate::content::registries::blocks::UnitPlanDef;
        let entries: Vec<&Map<String, Value>> = match value {
            Value::Object(map) => vec![map],
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    match item.as_object() {
                        Some(map) => out.push(map),
                        None => {
                            self.warn("plan entries must be objects");
                            return;
                        }
                    }
                }
                out
            }
            _ => {
                self.warn("`plans` must be an object or array");
                return;
            }
        };
        let mut created = Vec::with_capacity(entries.len());
        for map in entries {
            let Some(unit_name) = map.get("unit").and_then(Value::as_str) else {
                self.warn("plan is missing `unit`");
                return;
            };
            let Some(unit) = registry
                .unit_id(unit_name)
                .or_else(|| registry.unit_id(&registry.transform_name(unit_name)))
            else {
                self.warn(format!("unknown unit `{unit_name}`"));
                return;
            };
            let requirements = match map.get("requirements") {
                Some(value) => match self.parse_item_stacks(registry, value) {
                    Ok(stacks) => stacks,
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                },
                None => Vec::new(),
            };
            let time = map
                .get("time")
                .and_then(Value::as_f64)
                .map(|v| v as f32)
                .unwrap_or(0.0);
            created.push(UnitPlanDef {
                unit,
                time,
                requirements,
            });
        }
        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|b| b.unit_plans.clone())
        else {
            return;
        };
        if self.mark_used(reference, "plans") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(block) = r.block_mut(BlockId::new(raw)) {
                        block.unit_plans = original;
                    }
                }));
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            match mode {
                FieldMode::Set => block.unit_plans = created,
                FieldMode::Append => block.unit_plans.extend(created),
                FieldMode::Index(index) => {
                    if index < block.unit_plans.len() && !created.is_empty() {
                        block.unit_plans[index] = created.swap_remove(0);
                    }
                }
            }
        }
    }

    /// Edits `block.<name>.upgrades` (`Seq<UnitType[]>`: reconstructor upgrade
    /// pairs; `PatcherTests.reconstructorPlans`, `reconstructorPlansEditSpecific`,
    /// `reconstructorPlansAdd`, `nestedArrays`, `nestedArrays2`).
    ///
    /// The trailing `.N` / `.+` was peeled into `mode` by [`Self::apply_path`];
    /// `field`'s suffix after `upgrades` is the optional outer index used by
    /// inner-pair edits (`upgrades.0.1`).
    fn edit_reconstructor_upgrades(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let suffix = field.strip_prefix("upgrades").unwrap_or(field);
        let suffix = suffix.strip_prefix('.').unwrap_or(suffix);

        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|block| block.reconstructor_upgrades.clone())
        else {
            return;
        };

        let mut replace: Option<Vec<(UnitTypeId, UnitTypeId)>> = None;
        let mut append: Option<Vec<(UnitTypeId, UnitTypeId)>> = None;
        let mut replace_index: Option<(usize, (UnitTypeId, UnitTypeId))> = None;
        let mut set_component: Option<(usize, usize, UnitTypeId)> = None;

        if suffix.is_empty() {
            match mode {
                FieldMode::Set => match self.parse_upgrade_pairs(registry, value) {
                    Ok(pairs) => replace = Some(pairs),
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                },
                FieldMode::Append => match self.parse_upgrade_pairs(registry, value) {
                    Ok(pairs) => append = Some(pairs),
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                },
                FieldMode::Index(index) => match self.parse_upgrade_pair(registry, value) {
                    Ok(pair) => replace_index = Some((index, pair)),
                    Err(message) => {
                        self.warn(message);
                        return;
                    }
                },
            }
        } else {
            let Ok(outer) = suffix.parse::<usize>() else {
                self.warn(format!("invalid upgrades index `{suffix}`"));
                return;
            };
            match mode {
                FieldMode::Index(inner) => {
                    let Some(name) = value.as_str() else {
                        self.warn("upgrade component must be a unit name");
                        return;
                    };
                    let Some(unit) = self.resolve_unit(registry, name) else {
                        self.warn(format!("unknown unit `{name}`"));
                        return;
                    };
                    set_component = Some((outer, inner, unit));
                }
                _ => {
                    self.warn(format!("invalid upgrades path `{field}`"));
                    return;
                }
            }
        }

        if self.mark_used(reference, "upgrades") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(block) = r.block_mut(BlockId::new(raw)) {
                        block.reconstructor_upgrades = original;
                    }
                }));
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            if let Some(pairs) = replace {
                block.reconstructor_upgrades = pairs;
            }
            if let Some(pairs) = append {
                block.reconstructor_upgrades.extend(pairs);
            }
            if let Some((index, pair)) = replace_index
                && index < block.reconstructor_upgrades.len()
            {
                block.reconstructor_upgrades[index] = pair;
            }
            if let Some((outer, inner, unit)) = set_component
                && let Some(pair) = block.reconstructor_upgrades.get_mut(outer)
            {
                match inner {
                    0 => pair.0 = unit,
                    1 => pair.1 = unit,
                    _ => {}
                }
            }
        }
    }

    /// Resolves a unit name (raw, then `<mod>-`-prefixed).
    fn resolve_unit(&self, registry: &ContentRegistry, name: &str) -> Option<UnitTypeId> {
        registry
            .unit_id(name)
            .or_else(|| registry.unit_id(&registry.transform_name(name)))
    }

    /// Parses one `[from, to]` upgrade pair (array or `{"0":…,"1":…}` object).
    fn parse_upgrade_pair(
        &self,
        registry: &ContentRegistry,
        value: &Value,
    ) -> Result<(UnitTypeId, UnitTypeId), String> {
        let units = match value {
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    let name = item
                        .as_str()
                        .ok_or("upgrade pair entries must be strings")?;
                    out.push(
                        self.resolve_unit(registry, name)
                            .ok_or_else(|| format!("unknown unit `{name}`"))?,
                    );
                }
                out
            }
            Value::Object(map) => {
                let mut out = Vec::with_capacity(2);
                for key in ["0", "1"] {
                    let Some(name) = map.get(key).and_then(Value::as_str) else {
                        return Err(format!("upgrade pair is missing index `{key}`"));
                    };
                    out.push(
                        self.resolve_unit(registry, name)
                            .ok_or_else(|| format!("unknown unit `{name}`"))?,
                    );
                }
                out
            }
            _ => return Err("upgrade pair must be an array or object".to_owned()),
        };
        if units.len() < 2 {
            return Err("upgrade pair needs exactly two units".to_owned());
        }
        Ok((units[0], units[1]))
    }

    /// Parses a `Seq<UnitType[]>` value: a list of pairs, a single flat pair,
    /// or an empty array (clear).
    fn parse_upgrade_pairs(
        &self,
        registry: &ContentRegistry,
        value: &Value,
    ) -> Result<Vec<(UnitTypeId, UnitTypeId)>, String> {
        let Value::Array(items) = value else {
            if value.is_object() {
                return Ok(vec![self.parse_upgrade_pair(registry, value)?]);
            }
            return Err("`upgrades` must be an array of pairs".to_owned());
        };
        if items.is_empty() {
            return Ok(Vec::new());
        }
        if items.iter().all(Value::is_string) {
            return Ok(vec![self.parse_upgrade_pair(registry, value)?]);
        }
        let mut out = Vec::with_capacity(items.len());
        for entry in items {
            out.push(self.parse_upgrade_pair(registry, entry)?);
        }
        Ok(out)
    }

    /// Merges a `consumes` object (typed adds + `remove` semantics; M3 subset).
    fn edit_consumes(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        id: BlockId,
        _mode: FieldMode,
        value: &Value,
    ) {
        let Some(map) = value.as_object() else {
            self.warn("`consumes` must be an object");
            return;
        };
        let Some(original) = registry.block(id).map(|b| {
            (
                b.consumes.clone(),
                b.has_power,
                b.has_items,
                b.accepts_items,
                b.has_liquids,
            )
        }) else {
            return;
        };
        let raw = id.raw();
        if self.mark_used(reference, "consumes") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(block) = r.block_mut(BlockId::new(raw)) {
                        block.consumes = original.0.clone();
                        block.has_power = original.1;
                        block.has_items = original.2;
                        block.accepts_items = original.3;
                        block.has_liquids = original.4;
                    }
                }));
        }
        let remove = map.get("remove").and_then(Value::as_str);
        let power = map.get("power").and_then(Value::as_f64).map(|v| v as f32);
        let buffered = map
            .get("powerBuffered")
            .and_then(Value::as_f64)
            .map(|v| v as f32);
        let coolant = map.get("coolant").and_then(Value::as_f64).map(|v| v as f32);
        // `readBlockConsumers` typed entries (`item`/`items`, `liquid`/`liquids`).
        let items = match map.get("items").or_else(|| map.get("item")) {
            Some(value) => match self.parse_item_stacks(registry, value) {
                Ok(stacks) => Some(stacks),
                Err(message) => {
                    self.warn(message);
                    None
                }
            },
            None => None,
        };
        let liquids = match map.get("liquids").or_else(|| map.get("liquid")) {
            Some(value) => match parse_liquid_stacks(registry, value) {
                Ok(stacks) => Some(stacks),
                Err(message) => {
                    self.warn(message);
                    None
                }
            },
            None => None,
        };
        if let Some(block) = registry.block_mut(id) {
            match remove {
                Some("all") => block.consumes.clear(),
                Some("items" | "item") => block
                    .consumes
                    .retain(|spec| !matches!(spec.consume, Consume::Items(_))),
                Some("liquids" | "liquid") => block.consumes.retain(|spec| {
                    !matches!(spec.consume, Consume::Liquid { .. } | Consume::Liquids(_))
                }),
                Some("power") => block
                    .consumes
                    .retain(|spec| !matches!(spec.consume, Consume::Power { .. })),
                Some("coolant") => block
                    .consumes
                    .retain(|spec| !matches!(spec.consume, Consume::Coolant { .. })),
                _ => {}
            }
            let spec = |consume| ConsumeSpec {
                consume,
                optional: false,
                update: false,
                ignore: false,
            };
            if let Some(items) = items {
                block.has_items = true;
                block.accepts_items = true;
                block.consumes.push(spec(Consume::Items(items)));
            }
            if let Some(liquids) = liquids {
                block.has_liquids = true;
                for stack in liquids {
                    block.consumes.push(spec(Consume::Liquid {
                        liquid: stack.liquid,
                        amount: stack.amount,
                    }));
                }
            }
            if let Some(amount) = coolant {
                block.consumes.push(spec(Consume::Coolant {
                    amount,
                    allow_liquid: true,
                    allow_gas: true,
                }));
            }
            if power.is_some() || buffered.is_some() {
                block.consumes.push(spec(Consume::Power {
                    usage: power.unwrap_or(0.0),
                    buffered: buffered.unwrap_or(0.0),
                }));
                block.has_power = true;
            }
        }
    }

    /// Edits `block.<name>.attributes` (`Attributes` map; plan 20 M3b). Accepts
    /// an object form (`{oil: 99}`) and the dotted field form
    /// (`block.grass.attributes.heat: 77`), including custom names.
    fn edit_attributes(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        field: &str,
        value: &Value,
    ) {
        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|block| block.attributes.clone())
        else {
            return;
        };
        if self.mark_used(reference, "attributes") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(block) = r.block_mut(BlockId::new(raw)) {
                        block.attributes = original;
                    }
                }));
        }
        let mut updates: Vec<(String, f32)> = Vec::new();
        if let Some(key) = field.strip_prefix("attributes.") {
            if let Some(value) = value.as_f64() {
                updates.push((key.to_owned(), value as f32));
            }
        } else if let Some(map) = value.as_object() {
            for (key, entry) in map {
                if let Some(value) = entry.as_f64() {
                    updates.push((key.clone(), value as f32));
                }
            }
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            for (key, value) in updates {
                match block.attributes.iter_mut().find(|(name, _)| *name == key) {
                    Some(entry) => entry.1 = value,
                    None => block.attributes.push((key, value)),
                }
            }
        }
    }

    /// Edits `block.<name>.drillMultipliers` (`ObjectFloatMap<Item>`; plan 20
    /// M3b). Object and dotted forms, `"-"` removes a key.
    fn edit_drill_multipliers(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        field: &str,
        value: &Value,
    ) {
        let Some(original) = registry
            .block(BlockId::new(raw))
            .map(|block| block.drill_multipliers.clone())
        else {
            return;
        };
        if self.mark_used(reference, "drillMultipliers") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(block) = r.block_mut(BlockId::new(raw)) {
                        block.drill_multipliers = original;
                    }
                }));
        }
        let mut updates: Vec<(ItemId, f32)> = Vec::new();
        let mut removals: Vec<ItemId> = Vec::new();
        let mut apply = |name: &str,
                         value: &Value,
                         updates: &mut Vec<(ItemId, f32)>,
                         removals: &mut Vec<ItemId>| {
            let Some(item) = registry
                .item_id(name)
                .or_else(|| registry.item_id(&registry.transform_name(name)))
            else {
                self.warn(format!("unknown item `{name}`"));
                return;
            };
            if value.as_str() == Some("-") {
                removals.push(item);
            } else if let Some(value) = value.as_f64() {
                updates.push((item, value as f32));
            } else {
                self.warn(format!(
                    "drillMultipliers `{name}` must be a number or \"-\""
                ));
            }
        };
        if let Some(key) = field.strip_prefix("drillMultipliers.") {
            apply(key, value, &mut updates, &mut removals);
        } else if let Some(map) = value.as_object() {
            for (key, entry) in map {
                apply(key, entry, &mut updates, &mut removals);
            }
        }
        if let Some(block) = registry.block_mut(BlockId::new(raw)) {
            for item in removals {
                block
                    .drill_multipliers
                    .retain(|(existing, _)| *existing != item);
            }
            for (item, value) in updates {
                match block
                    .drill_multipliers
                    .iter_mut()
                    .find(|(existing, _)| *existing == item)
                {
                    Some(entry) => entry.1 = value,
                    None => block.drill_multipliers.push((item, value)),
                }
            }
        }
    }

    // ---- item ----

    fn apply_item(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.item_id(name) else {
            self.warn(format!("unknown item `{name}`"));
            return;
        };
        let reference = ContentRef::item(id);
        match field {
            "hardness" => self.set_i32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.hardness),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.hardness = v;
                    }
                },
            ),
            "cost" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.cost),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.cost = v;
                    }
                },
            ),
            "explosiveness" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.explosiveness),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.explosiveness = v;
                    }
                },
            ),
            "flammability" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.flammability),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.flammability = v;
                    }
                },
            ),
            "radioactivity" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.radioactivity),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.radioactivity = v;
                    }
                },
            ),
            "charge" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.charge),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.charge = v;
                    }
                },
            ),
            "buildable" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.buildable),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.buildable = v;
                    }
                },
            ),
            "hidden" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.item(id).map(|i| i.hidden),
                move |r, v| {
                    if let Some(i) = r.item_mut(ItemId::new(id.raw())) {
                        i.hidden = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `item.{name}.{field}`")),
        }
    }

    // ---- liquid ----

    fn apply_liquid(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.liquid_id(name) else {
            self.warn(format!("unknown liquid `{name}`"));
            return;
        };
        let reference = ContentRef::liquid(id);
        match field {
            "gas" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.gas),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.gas = v;
                    }
                },
            ),
            "coolant" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.coolant),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.coolant = v;
                    }
                },
            ),
            "hidden" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.hidden),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.hidden = v;
                    }
                },
            ),
            "temperature" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.temperature),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.temperature = v;
                    }
                },
            ),
            "heatCapacity" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.heat_capacity),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.heat_capacity = v;
                    }
                },
            ),
            "viscosity" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.viscosity),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.viscosity = v;
                    }
                },
            ),
            "flammability" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.liquid(id).map(|l| l.flammability),
                move |r, v| {
                    if let Some(l) = r.liquid_mut(crate::content::LiquidId::new(id.raw())) {
                        l.flammability = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `liquid.{name}.{field}`")),
        }
    }

    // ---- status ----

    fn apply_status(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.status_id(name) else {
            self.warn(format!("unknown status `{name}`"));
            return;
        };
        let reference = ContentRef::status(id);
        match field {
            "speedMultiplier" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.status(id).map(|s| s.speed_multiplier),
                move |r, v| {
                    if let Some(s) = r.status_mut(StatusId::new(id.raw())) {
                        s.speed_multiplier = v;
                    }
                },
            ),
            "damageMultiplier" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.status(id).map(|s| s.damage_multiplier),
                move |r, v| {
                    if let Some(s) = r.status_mut(StatusId::new(id.raw())) {
                        s.damage_multiplier = v;
                    }
                },
            ),
            "healthMultiplier" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.status(id).map(|s| s.health_multiplier),
                move |r, v| {
                    if let Some(s) = r.status_mut(StatusId::new(id.raw())) {
                        s.health_multiplier = v;
                    }
                },
            ),
            "disarm" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.status(id).map(|s| s.disarm),
                move |r, v| {
                    if let Some(s) = r.status_mut(StatusId::new(id.raw())) {
                        s.disarm = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `status.{name}.{field}`")),
        }
    }

    // ---- unit ----

    fn apply_unit(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.unit_id(name) else {
            self.warn(format!("unknown unit `{name}`"));
            return;
        };
        let reference = ContentRef::new(ContentType::Unit, id.raw());
        match field {
            "immunities" => self.edit_immunities(registry, reference, id.raw(), mode, value),
            "weapons" => self.edit_weapons(registry, reference, id.raw(), mode, value),
            field if field.starts_with("weapons.") => {
                self.edit_weapon_path(registry, id.raw(), field, value)
            }
            "abilities" => self.edit_abilities(registry, reference, id.raw(), mode, value),
            "targetFlags" => self.edit_target_flags(registry, reference, id.raw(), mode, value),
            "type" => self.edit_unit_entity(registry, reference, id.raw(), value),
            "health" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.unit(id).map(|u| u.health),
                move |r, v| {
                    if let Some(u) = r.unit_mut(UnitTypeId::new(id.raw())) {
                        u.health = v;
                    }
                },
            ),
            "speed" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.unit(id).map(|u| u.speed),
                move |r, v| {
                    if let Some(u) = r.unit_mut(UnitTypeId::new(id.raw())) {
                        u.speed = v;
                    }
                },
            ),
            "armor" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.unit(id).map(|u| u.armor),
                move |r, v| {
                    if let Some(u) = r.unit_mut(UnitTypeId::new(id.raw())) {
                        u.armor = v;
                    }
                },
            ),
            "hitSize" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.unit(id).map(|u| u.hit_size),
                move |r, v| {
                    if let Some(u) = r.unit_mut(UnitTypeId::new(id.raw())) {
                        u.hit_size = v;
                    }
                },
            ),
            "flying" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.unit(id).map(|u| u.flying),
                move |r, v| {
                    if let Some(u) = r.unit_mut(UnitTypeId::new(id.raw())) {
                        u.flying = v;
                    }
                },
            ),
            "hidden" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.unit(id).map(|u| u.hidden),
                move |r, v| {
                    if let Some(u) = r.unit_mut(UnitTypeId::new(id.raw())) {
                        u.hidden = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `unit.{name}.{field}`")),
        }
    }

    /// Edits `unit.immunities` (`ObjectSet` semantics: `+` adds, never duplicates).
    fn edit_immunities(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        mode: FieldMode,
        value: &Value,
    ) {
        let mut targets = Vec::new();
        match value {
            Value::String(name) => {
                if let Some(status) = self.resolve_status_id(registry, name) {
                    targets.push(status);
                }
            }
            Value::Array(entries) => {
                for entry in entries {
                    if let Some(name) = entry.as_str()
                        && let Some(status) = self.resolve_status_id(registry, name)
                    {
                        targets.push(status);
                    }
                }
            }
            _ => {}
        }
        let Some(original) = registry
            .unit(UnitTypeId::new(raw))
            .map(|u| u.immunities.clone())
        else {
            return;
        };
        if self.mark_used(reference, "immunities") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                        unit.immunities = original;
                    }
                }));
        }
        if let Some(unit) = registry.unit_mut(UnitTypeId::new(raw)) {
            match mode {
                FieldMode::Set => unit.immunities = targets,
                FieldMode::Append => {
                    for status in targets {
                        if !unit.immunities.contains(&status) {
                            unit.immunities.push(status);
                        }
                    }
                }
                FieldMode::Index(_) => {}
            }
        }
    }

    /// Edits `unit.weapons` (`Seq<Weapon>`): whole replace, `+` append (single
    /// or array) and numeric index replacement. Created weapons register their
    /// inline bullet and run the created-object `init()` callback
    /// (`PatcherTests.unitWeapons`, `uUnitWeaponReassign`, `addWeapon`).
    fn edit_weapons(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        mode: FieldMode,
        value: &Value,
    ) {
        // `Seq<Weapon>` accepts a whole array, a single object, or an
        // index-keyed object (`{"0": {...}}`; `PatcherTests.arrayMulti`).
        let mut entries: Vec<(Option<usize>, &Map<String, Value>)> = Vec::new();
        match value {
            Value::Object(map) if is_index_map(map) => {
                for (key, val) in map {
                    let Some(weapon) = val.as_object() else {
                        self.warn("weapon entries must be objects");
                        return;
                    };
                    entries.push((key.parse().ok(), weapon));
                }
            }
            Value::Object(map) => entries.push((None, map)),
            Value::Array(items) => {
                for item in items {
                    match item.as_object() {
                        Some(map) => entries.push((None, map)),
                        None => {
                            self.warn("weapon entries must be objects");
                            return;
                        }
                    }
                }
            }
            _ => {
                self.warn("`weapons` must be an object or array");
                return;
            }
        }
        let Some(original) = registry
            .unit(UnitTypeId::new(raw))
            .map(|unit| unit.weapons.clone())
        else {
            return;
        };
        if self.mark_used(reference, "weapons") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                        unit.weapons = original;
                    }
                }));
        }
        let bullets_before = registry.bullets().len();
        let mut parser = crate::mods::json::ContentJsonParser::new();
        let mut created: Vec<(
            Option<usize>,
            crate::content::registries::units::weapon::WeaponDef,
        )> = Vec::with_capacity(entries.len());
        for (index, object) in &entries {
            match parser.parse_weapon_object(registry, "patch", object, index.unwrap_or(0)) {
                Ok(weapon) => created.push((*index, weapon)),
                Err(error) => {
                    self.warn(error.message);
                    return;
                }
            }
        }
        // `DataPatcher.created`: inline bullets registered by these weapons are
        // freshly constructed non-mappable content that must run its lifecycle.
        for (_, weapon) in &created {
            if weapon.bullet.id.raw() as usize >= bullets_before {
                self.mark_created(ContentRef::new(ContentType::Bullet, weapon.bullet.id.raw()));
            }
        }
        let Some(unit) = registry.unit_mut(UnitTypeId::new(raw)) else {
            return;
        };
        match mode {
            FieldMode::Set if created.iter().any(|(index, _)| index.is_some()) => {
                for (index, weapon) in created {
                    if let Some(index) = index
                        && index < unit.weapons.len()
                    {
                        unit.weapons[index] = weapon;
                    }
                }
            }
            FieldMode::Set => {
                unit.weapons = created.into_iter().map(|(_, weapon)| weapon).collect();
            }
            FieldMode::Append => {
                unit.weapons
                    .extend(created.into_iter().map(|(_, weapon)| weapon));
            }
            FieldMode::Index(index) => {
                if index < unit.weapons.len()
                    && let Some((_, weapon)) = created.into_iter().next()
                {
                    unit.weapons[index] = weapon;
                }
            }
        }
    }

    /// Edits `unit.abilities` (`Seq<Ability>`): whole replace and `+` append
    /// (single or array); `PatcherTests.unitAbilities{,Array}`.
    fn edit_abilities(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        mode: FieldMode,
        value: &Value,
    ) {
        let entries: Vec<&Map<String, Value>> = match value {
            Value::Object(map) => vec![map],
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    match item.as_object() {
                        Some(map) => out.push(map),
                        None => {
                            self.warn("ability entries must be objects");
                            return;
                        }
                    }
                }
                out
            }
            _ => {
                self.warn("`abilities` must be an object or array");
                return;
            }
        };
        let Some(original) = registry
            .unit(UnitTypeId::new(raw))
            .map(|unit| unit.abilities.clone())
        else {
            return;
        };
        if self.mark_used(reference, "abilities") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                        unit.abilities = original;
                    }
                }));
        }
        let mut parser = crate::mods::json::ContentJsonParser::new();
        let mut created = Vec::with_capacity(entries.len());
        for object in &entries {
            match parser.parse_ability("patch", registry, object) {
                Ok(ability) => created.push(ability),
                Err(error) => {
                    self.warn(error.message);
                    return;
                }
            }
        }
        if let Some(unit) = registry.unit_mut(UnitTypeId::new(raw)) {
            match mode {
                FieldMode::Set => unit.abilities = created,
                FieldMode::Append => unit.abilities.extend(created),
                FieldMode::Index(_) => {}
            }
        }
    }

    /// Edits a deep weapon path (`weapons.<index>.bullet.<field>` /
    /// `weapons.<index>.<field>`; `PatcherTests.indexAccess`). Bullet fields
    /// resolve through the weapon's registered [`crate::content::BulletId`].
    fn edit_weapon_path(
        &mut self,
        registry: &mut ContentRegistry,
        raw: u16,
        field: &str,
        value: &Value,
    ) {
        let path = field.strip_prefix("weapons.").unwrap_or(field);
        let Some((index, rest)) = path.split_once('.') else {
            self.warn(format!("invalid weapon path `{field}`"));
            return;
        };
        let Ok(index) = index.parse::<usize>() else {
            self.warn(format!("invalid weapon index `{index}`"));
            return;
        };
        let Some(unit) = registry.unit(UnitTypeId::new(raw)) else {
            return;
        };
        let Some(weapon) = unit.weapons.get(index) else {
            self.warn(format!("weapon index {index} out of range"));
            return;
        };
        let bullet_id = weapon.bullet.id;
        if let Some(bullet_field) = rest.strip_prefix("bullet.") {
            self.edit_bullet_field(registry, raw, index, bullet_id, bullet_field, value);
        } else {
            self.warn(format!("unknown weapon field `{rest}`"));
        }
    }

    /// Applies one bullet field edit with a reverse reset (`PatcherTests`).
    fn edit_bullet_field(
        &mut self,
        registry: &mut ContentRegistry,
        unit_raw: u16,
        weapon_index: usize,
        bullet: crate::content::BulletId,
        field: &str,
        value: &Value,
    ) {
        let key = || format!("weapons.{weapon_index}.bullet.{field}");
        macro_rules! set_f32 {
            ($f:ident) => {{
                let Some(original) = registry.bullet(bullet).map(|b| b.$f) else {
                    return;
                };
                let Some(new) = value.as_f64().map(|v| v as f32) else {
                    return;
                };
                if self.mark_used(ContentRef::new(ContentType::Unit, unit_raw), &key()) {
                    self.resetters
                        .push(Box::new(move |r: &mut ContentRegistry| {
                            if let Some(bullet) = r.bullet_mut(bullet) {
                                bullet.$f = original;
                            }
                        }));
                }
                if let Some(bullet) = registry.bullet_mut(bullet) {
                    bullet.$f = new;
                }
            }};
        }
        macro_rules! set_i32 {
            ($f:ident) => {{
                let Some(original) = registry.bullet(bullet).map(|b| b.$f) else {
                    return;
                };
                let Some(new) = value.as_i64().map(|v| v as i32) else {
                    return;
                };
                if self.mark_used(ContentRef::new(ContentType::Unit, unit_raw), &key()) {
                    self.resetters
                        .push(Box::new(move |r: &mut ContentRegistry| {
                            if let Some(bullet) = r.bullet_mut(bullet) {
                                bullet.$f = original;
                            }
                        }));
                }
                if let Some(bullet) = registry.bullet_mut(bullet) {
                    bullet.$f = new;
                }
            }};
        }
        macro_rules! set_bool {
            ($f:ident) => {{
                let Some(original) = registry.bullet(bullet).map(|b| b.$f) else {
                    return;
                };
                let Some(new) = value.as_bool() else {
                    return;
                };
                if self.mark_used(ContentRef::new(ContentType::Unit, unit_raw), &key()) {
                    self.resetters
                        .push(Box::new(move |r: &mut ContentRegistry| {
                            if let Some(bullet) = r.bullet_mut(bullet) {
                                bullet.$f = original;
                            }
                        }));
                }
                if let Some(bullet) = registry.bullet_mut(bullet) {
                    bullet.$f = new;
                }
            }};
        }
        match field {
            "damage" => set_f32!(damage),
            "speed" => set_f32!(speed),
            "lifetime" => set_f32!(lifetime),
            "hitSize" => set_f32!(hit_size),
            "drawSize" => set_f32!(draw_size),
            "splashDamage" => set_f32!(splash_damage),
            "splashDamageRadius" => set_f32!(splash_damage_radius),
            "ammoMultiplier" => set_f32!(ammo_multiplier),
            "reloadMultiplier" => set_f32!(reload_multiplier),
            "lightningLength" => set_i32!(lightning_length),
            "lightningLengthRand" => set_i32!(lightning_length_rand),
            "lightning" => set_i32!(lightning),
            "pierce" => set_bool!(pierce),
            "pierceBuilding" => set_bool!(pierce_building),
            "keepVelocity" => set_bool!(keep_velocity),
            "collides" => set_bool!(collides),
            _ => self.warn(format!("unknown bullet field `{field}`")),
        }
    }

    /// Edits `unit.targetFlags` (`Seq<BlockFlag>`): whole replace, `+` append
    /// (single or array). `PatcherTests.unitFlags{,Array}`.
    fn edit_target_flags(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        mode: FieldMode,
        value: &Value,
    ) {
        let names: Vec<&str> = match value {
            Value::String(name) => vec![name.as_str()],
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            _ => {
                self.warn("`targetFlags` must be a string or array");
                return;
            }
        };
        let mut flags = Vec::with_capacity(names.len());
        for name in names {
            match resolve_block_flag(name) {
                Some(flag) => flags.push(Some(flag)),
                None => {
                    self.warn(format!("unknown block flag `{name}`"));
                    return;
                }
            }
        }
        let Some(original) = registry
            .unit(UnitTypeId::new(raw))
            .map(|unit| unit.target_flags.clone())
        else {
            return;
        };
        if self.mark_used(reference, "targetFlags") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                        unit.target_flags = original;
                    }
                }));
        }
        if let Some(unit) = registry.unit_mut(UnitTypeId::new(raw)) {
            match mode {
                FieldMode::Set => unit.target_flags = flags,
                FieldMode::Append => unit.target_flags.extend(flags),
                FieldMode::Index(_) => {}
            }
        }
    }

    /// Edits `unit.type` (entity/constructor keyword, `PatcherTests.unitType`).
    fn edit_unit_entity(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        raw: u16,
        value: &Value,
    ) {
        let Some(name) = value.as_str() else {
            return;
        };
        let Some(def) = crate::mods::json::resolve_entity_def(name) else {
            self.warn(format!("unknown unit type `{name}`"));
            return;
        };
        let Some(original) = registry
            .unit(UnitTypeId::new(raw))
            .map(|unit| unit.entity_def)
        else {
            return;
        };
        if self.mark_used(reference, "type") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                        unit.entity_def = original;
                    }
                }));
        }
        if let Some(unit) = registry.unit_mut(UnitTypeId::new(raw)) {
            unit.entity_def = def;
        }
    }

    // ---- weather / planet / sector / team (scalar subset) ----

    fn apply_weather(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.weather_by_name(name).map(|w| w.id) else {
            self.warn(format!("unknown weather `{name}`"));
            return;
        };
        let reference = ContentRef::new(ContentType::Weather, id.raw());
        match field {
            "duration" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.weather(id).map(|w| w.duration),
                move |r, v| {
                    if let Some(w) = r.weather_mut(crate::content::WeatherId::new(id.raw())) {
                        w.duration = v;
                    }
                },
            ),
            "hidden" => self.set_bool(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.weather(id).map(|w| w.hidden),
                move |r, v| {
                    if let Some(w) = r.weather_mut(crate::content::WeatherId::new(id.raw())) {
                        w.hidden = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `weather.{name}.{field}`")),
        }
    }

    fn apply_planet(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.planet_id(name) else {
            self.warn(format!("unknown planet `{name}`"));
            return;
        };
        let reference = ContentRef::new(ContentType::Planet, id.raw());
        match field {
            "radius" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.planet(id).map(|p| p.radius),
                move |r, v| {
                    if let Some(p) = r.planet_mut(PlanetId::new(id.raw())) {
                        p.radius = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `planet.{name}.{field}`")),
        }
    }

    fn apply_sector(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.sector_by_name(name).map(|s| s.id) else {
            self.warn(format!("unknown sector `{name}`"));
            return;
        };
        let reference = ContentRef::new(ContentType::Sector, id.raw());
        match field {
            "difficulty" => self.set_f32(
                registry,
                reference,
                field,
                value,
                mode,
                |r| r.sector(id).map(|s| s.difficulty),
                move |r, v| {
                    if let Some(s) = r.sector_mut(crate::content::SectorId::new(id.raw())) {
                        s.difficulty = v;
                    }
                },
            ),
            _ => self.warn(format!("unknown field `sector.{name}.{field}`")),
        }
    }

    fn apply_team(
        &mut self,
        registry: &mut ContentRegistry,
        name: &str,
        field: &str,
        mode: FieldMode,
        value: &Value,
    ) {
        let Some(id) = registry.team_by_name(name).map(|t| t.id) else {
            self.warn(format!("unknown team `{name}`"));
            return;
        };
        let reference = ContentRef::new(ContentType::Team, id.raw());
        match field {
            "team" => {
                if let Some(new) = value.as_str() {
                    if mode != FieldMode::Set {
                        return;
                    }
                    let Some(original) = registry.team(id).map(|t| t.team.clone()) else {
                        return;
                    };
                    if self.mark_used(reference, "team") {
                        let raw = id.raw();
                        self.resetters
                            .push(Box::new(move |r: &mut ContentRegistry| {
                                if let Some(t) = r.team_mut(crate::content::TeamEntryId::new(raw)) {
                                    t.team = original.clone();
                                }
                            }));
                    }
                    if let Some(t) = registry.team_mut(id) {
                        t.team = new.to_owned();
                    }
                }
            }
            _ => self.warn(format!("unknown field `team.{name}.{field}`")),
        }
    }

    // ---- generic scalar helpers ----

    #[allow(clippy::too_many_arguments)]
    fn set_i32(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        field: &str,
        value: &Value,
        mode: FieldMode,
        get: impl FnOnce(&ContentRegistry) -> Option<i32>,
        set: impl Fn(&mut ContentRegistry, i32) + Clone + 'static,
    ) {
        if mode != FieldMode::Set {
            self.warn(format!("array suffix is invalid on `{field}`"));
            return;
        }
        let Some(new) = value.as_i64().map(|v| v as i32) else {
            return;
        };
        if self.mark_used(reference, field) {
            let Some(original) = get(registry) else {
                return;
            };
            let reset = set.clone();
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| reset(r, original)));
        }
        set(registry, new);
    }

    #[allow(clippy::too_many_arguments)]
    fn set_f32(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        field: &str,
        value: &Value,
        mode: FieldMode,
        get: impl FnOnce(&ContentRegistry) -> Option<f32>,
        set: impl Fn(&mut ContentRegistry, f32) + Clone + 'static,
    ) {
        if mode != FieldMode::Set {
            self.warn(format!("array suffix is invalid on `{field}`"));
            return;
        }
        let Some(new) = value.as_f64().map(|v| v as f32) else {
            return;
        };
        if self.mark_used(reference, field) {
            let Some(original) = get(registry) else {
                return;
            };
            let reset = set.clone();
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| reset(r, original)));
        }
        set(registry, new);
    }

    #[allow(clippy::too_many_arguments)]
    fn set_bool(
        &mut self,
        registry: &mut ContentRegistry,
        reference: ContentRef,
        field: &str,
        value: &Value,
        mode: FieldMode,
        get: impl FnOnce(&ContentRegistry) -> Option<bool>,
        set: impl Fn(&mut ContentRegistry, bool) + Clone + 'static,
    ) {
        if mode != FieldMode::Set {
            self.warn(format!("array suffix is invalid on `{field}`"));
            return;
        }
        let Some(new) = value.as_bool() else {
            return;
        };
        if self.mark_used(reference, field) {
            let Some(original) = get(registry) else {
                return;
            };
            let reset = set.clone();
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| reset(r, original)));
        }
        set(registry, new);
    }

    // ---- shared parsing ----

    fn parse_item_stacks(
        &mut self,
        registry: &ContentRegistry,
        value: &Value,
    ) -> Result<Vec<ItemStack>, String> {
        let entries: Vec<&Value> = match value {
            Value::Array(items) => items.iter().collect(),
            Value::String(_) => vec![value],
            Value::Object(map) => {
                // `{ "copper": 10 }` form.
                let mut out = Vec::new();
                for (name, amount) in map {
                    let item = registry
                        .item_id(name)
                        .or_else(|| registry.item_id(&registry.transform_name(name)))
                        .ok_or_else(|| format!("unknown item `{name}`"))?;
                    out.push(ItemStack::new(item, amount.as_i64().unwrap_or(1) as i32));
                }
                return Ok(out);
            }
            _ => return Err("requirements value must be an array/object/string".to_owned()),
        };
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            let (raw, amount) = match entry {
                Value::String(text) => crate::mods::json::parse_stack(text)
                    .ok_or_else(|| format!("invalid stack `{text}`"))?,
                Value::Object(map) => {
                    let item = map
                        .get("item")
                        .and_then(Value::as_str)
                        .ok_or("stack missing item")?;
                    (
                        item.to_owned(),
                        map.get("amount").and_then(Value::as_i64).unwrap_or(1) as i32,
                    )
                }
                _ => return Err("invalid requirement entry".to_owned()),
            };
            let item = registry
                .item_id(&raw)
                .or_else(|| registry.item_id(&registry.transform_name(&raw)))
                .ok_or_else(|| format!("unknown item `{raw}`"))?;
            out.push(ItemStack::new(item, amount));
        }
        Ok(out)
    }

    fn resolve_status_id(&self, registry: &ContentRegistry, raw: &str) -> Option<StatusId> {
        registry
            .status_id(raw)
            .or_else(|| registry.status_id(&registry.transform_name(raw)))
    }

    /// Records `(content, field)` once; returns whether a reset is needed.
    fn mark_used(&mut self, reference: ContentRef, field: &str) -> bool {
        self.used.insert((reference, field.to_owned()))
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }
}

/// `DataPatcher.fixContentArrays`: grows/shrinks the registry-backed dense
/// item tables after patch/embedded content added or removed items
/// (`Block.checkContentArrayCapacity` parity).
///
/// The port stores the block-level item tables as the per-item `BlockDef`
/// arrays `item_costs`/`item_health_scaling` (used by `Block.init_self` for
/// `build_time`/`health`). This hook resizes them to the live item count,
/// matching upstream's `Arrays.copyOf(filter, items)`. The world half — live
/// building `ItemModule`/`LiquidModule` tables and the editor tile remap — is
/// [`fix_world_content_arrays`] (plan 09/19). Upstream's `ItemSeq` usage is
/// campaign-local and sized at construction; the port's transient `ItemSeq`s
/// (`UnitType.getRequirements`) are already built with `registry.items().len()`.
pub fn fix_content_arrays(registry: &mut ContentRegistry) {
    // Upstream guards this with `DataPatcher.needsArrayFix` (set when content is
    // created); the port runs it unconditionally, which is idempotent because
    // the resize is a no-op unless the item count changed.
    let item_count = registry.items().len();
    if item_count == 0 {
        return;
    }
    let costs: Vec<f32> = registry.items().iter().map(|item| item.cost).collect();
    let scaling: Vec<f32> = registry
        .items()
        .iter()
        .map(|item| item.health_scaling)
        .collect();
    for block in registry.blocks_mut() {
        // `!Vars.content.blocks().synthetic()` blocks are still resized upstream;
        // the port has no `synthetic()` flag, so every block is handled.
        if block.item_costs.len() != item_count {
            block.item_costs = costs.clone();
        }
        if block.item_health_scaling.len() != item_count {
            block.item_health_scaling = scaling.clone();
        }
    }
}

/// `DataPatcher.fixContentArrays` world half: grows every live building's
/// [`ItemModule`](crate::world::ItemModule)/[`LiquidModule`](crate::world::LiquidModule)
/// to the dense content counts. Upstream only does this for the editor world
/// (`!Vars.headless && ui.editor.isShown()`); the port exposes it as a hook for
/// plan 09/19 instead of reaching into `Vars.world`.
pub fn fix_world_content_arrays(world: &mut bevy_ecs::world::World, registry: &ContentRegistry) {
    let items = registry.items().len();
    let liquids = registry.liquids().len();
    let mut item_query = world.query::<&mut crate::world::ItemModule>();
    for mut module in item_query.iter_mut(world) {
        module.check_array_capacity(items);
    }
    let mut liquid_query = world.query::<&mut crate::world::LiquidModule>();
    for mut module in liquid_query.iter_mut(world) {
        module.check_array_capacity(liquids);
    }
}

/// `Mods.loadModPatches`: applies each enabled mod's `patches/**.json` with a
/// fresh [`DataPatcher`] (so mods never reset each other), files sorted.
///
/// Returns one warning string per mod whose patches failed to apply.
pub fn load_mod_patches(
    mods: &mut Mods,
    registry: &mut ContentRegistry,
    fs: &dyn FileSystem,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for index in mods.ordered_mods() {
        let Some(mod_) = mods.mod_at(index) else {
            continue;
        };
        if mod_.meta.hidden {
            continue;
        }
        let Ok(entries) = mod_.root.walk(fs, "patches") else {
            continue;
        };
        let mut paths: Vec<String> = entries
            .into_iter()
            .filter(|path| {
                matches!(
                    path.rsplit('.').next().unwrap_or(""),
                    "json" | "hjson" | "json5"
                )
            })
            .collect();
        if paths.is_empty() {
            continue;
        }
        paths.sort();
        let mut assets = Vec::new();
        for path in paths {
            let Ok(json) = mod_.root.read_to_string(fs, &path) else {
                warnings.push(format!("mod `{}`: could not read `{path}`", mod_.name));
                continue;
            };
            assets.push(PatchAsset { name: path, json });
        }
        let mut patcher = DataPatcher::new();
        if let Err(error) = patcher.apply(registry, &assets) {
            warnings.push(format!(
                "Failed to apply patches from mod {}: {error}",
                mod_.name
            ));
        }
    }
    warnings
}

/// Parses a top-level patch type name (`block`/`item`/… all nine kinds).
fn content_type_from_name(name: &str) -> Option<ContentType> {
    match name {
        "item" | "items" => Some(ContentType::Item),
        "block" | "blocks" => Some(ContentType::Block),
        "liquid" | "liquids" => Some(ContentType::Liquid),
        "status" | "statuses" => Some(ContentType::Status),
        "unit" | "units" => Some(ContentType::Unit),
        "weather" | "weathers" => Some(ContentType::Weather),
        "sector" | "sectors" => Some(ContentType::Sector),
        "planet" | "planets" => Some(ContentType::Planet),
        "team" | "teams" => Some(ContentType::Team),
        _ => None,
    }
}

/// Parses typed `liquid`/`liquids` consume entries: `["water/10"]`,
/// `{"water": 10}`, or a single `"water/10"` string.
fn parse_liquid_stacks(
    registry: &ContentRegistry,
    value: &Value,
) -> Result<Vec<LiquidStack>, String> {
    let resolve = |name: &str| {
        registry
            .liquid_id(name)
            .or_else(|| registry.liquid_id(&registry.transform_name(name)))
            .ok_or_else(|| format!("unknown liquid `{name}`"))
    };
    let entries: Vec<&Value> = match value {
        Value::Array(items) => items.iter().collect(),
        Value::String(_) => vec![value],
        Value::Object(map) => {
            let mut out = Vec::with_capacity(map.len());
            for (name, amount) in map {
                out.push(LiquidStack::new(
                    resolve(name)?,
                    amount.as_f64().unwrap_or(0.0) as f32,
                ));
            }
            return Ok(out);
        }
        _ => return Err("`liquids` must be an array/object/string".to_owned()),
    };
    let mut out = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry {
            Value::String(text) => {
                let (name, amount) = text
                    .split_once('/')
                    .map(|(name, amount)| (name, amount.parse::<f32>().unwrap_or(0.0)))
                    .unwrap_or((text.as_str(), 0.0));
                out.push(LiquidStack::new(resolve(name)?, amount));
            }
            Value::Object(map) => {
                let name = map
                    .get("liquid")
                    .and_then(Value::as_str)
                    .ok_or("liquid stack missing `liquid`")?;
                let amount = map.get("amount").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                out.push(LiquidStack::new(resolve(name)?, amount));
            }
            _ => return Err("invalid liquid entry".to_owned()),
        }
    }
    Ok(out)
}

/// Whether a JSON object is an index-keyed container (`{"0": …}`), used to
/// address `Seq` elements in the object form (`PatcherTests.arrayMulti`).
fn is_index_map(map: &Map<String, Value>) -> bool {
    !map.is_empty() && map.keys().all(|key| key.parse::<usize>().is_ok())
}

/// Resolves a `unit.targetFlags` name to a [`BlockFlag`] keyword.
fn resolve_block_flag(name: &str) -> Option<crate::content::registries::blocks::BlockFlag> {
    use crate::content::registries::blocks::BlockFlag;
    Some(match name {
        "core" => BlockFlag::Core,
        "storage" => BlockFlag::Storage,
        "generator" => BlockFlag::Generator,
        "turret" => BlockFlag::Turret,
        "factory" => BlockFlag::Factory,
        "repair" => BlockFlag::Repair,
        "battery" => BlockFlag::Battery,
        "reactor" => BlockFlag::Reactor,
        "extinguisher" => BlockFlag::Extinguisher,
        "drill" => BlockFlag::Drill,
        "shield" => BlockFlag::Shield,
        "launchPad" => BlockFlag::LaunchPad,
        "unitCargoUnloadPoint" => BlockFlag::UnitCargoUnloadPoint,
        "unitAssembler" => BlockFlag::UnitAssembler,
        "hasFogRadius" => BlockFlag::HasFogRadius,
        "steamVent" => BlockFlag::SteamVent,
        "blockRepair" => BlockFlag::BlockRepair,
        "synced" => BlockFlag::Synced,
        _ => return None,
    })
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

    /// PatcherTests.specificArrayRequirements: `{0: …}` + full replace.
    #[test]
    fn array_requirements_index_and_replace() {
        let mut registry = test_registry();
        let id = registry.block_id("scatter").expect("scatter");
        let original = registry.block(id).expect("block").requirements.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"block.scatter.requirements.0": "titanium/99"}"#)],
            )
            .expect("index edit");
        let amount = registry.block(id).expect("block").requirements[0].amount;
        assert_eq!(amount, 99);
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").requirements, original);

        // Whole-array replace.
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"block.scatter.requirements": ["copper/5"]}"#)],
            )
            .expect("replace");
        assert_eq!(registry.block(id).expect("block").requirements.len(), 1);
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").requirements, original);
    }

    /// PatcherTests.reconstructorPlansAdd: `+` append (`requirements.+`).
    #[test]
    fn array_requirements_append() {
        let mut registry = test_registry();
        let id = registry.block_id("scatter").expect("scatter");
        let original = registry.block(id).expect("block").requirements.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"block.scatter.requirements.+": "copper/7"}"#)],
            )
            .expect("append");
        let after = registry.block(id).expect("block").requirements.clone();
        assert_eq!(after.len(), original.len() + 1);
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").requirements, original);
    }

    /// PatcherTests.setMultiAdd: `ObjectSet` `+` (`immunities.+`).
    #[test]
    fn object_set_multi_add() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").immunities.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"unit.dagger.immunities.+": "wet"}"#)],
            )
            .expect("append immunity");
        let after = registry.unit(id).expect("unit").immunities.clone();
        assert_eq!(after.len(), original.len() + 1);
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").immunities, original);
    }

    /// PatcherTests.consumeApply: `hasPower`, consumer count, reset.
    #[test]
    fn consume_apply_power() {
        let mut registry = test_registry();
        let id = registry.block_id("router").expect("router");
        let original = registry.block(id).expect("block").consumes.clone();
        let had_power = registry.block(id).expect("block").has_power;
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"block.router.consumes": {"power": 3.5}}"#)],
            )
            .expect("consumes");
        let block = registry.block(id).expect("block");
        assert!(block.has_power);
        assert_eq!(block.consumes.len(), original.len() + 1);
        patcher.unapply(&mut registry);
        let block = registry.block(id).expect("block");
        assert_eq!(block.consumes, original);
        assert_eq!(block.has_power, had_power);
    }

    /// Plan 20 M3: `patch::load_mod_patches` applies a mod's `patches/` files.
    #[test]
    fn load_mod_patches_applies() {
        use crate::io::{MockFs, SettingsStore};
        use crate::mods::Mods;
        use std::path::Path;

        let fs = MockFs::new();
        fs.write(
            Path::new("/mods/patcher/mod.json"),
            br#"{"name":"Patcher","minGameVersion":"146"}"#,
        )
        .expect("mod.json");
        fs.write(
            Path::new("/mods/patcher/patches/health.json"),
            br#"{"block.router.health": 777}"#,
        )
        .expect("patch");
        let settings = SettingsStore::new();
        let mut mods = Mods::new(true, "/mods");
        mods.load_single(&fs, Path::new("/mods/patcher"), &settings)
            .expect("load_single");
        let mut registry = test_registry();
        let warnings = load_mod_patches(&mut mods, &mut registry, &fs);
        assert!(warnings.is_empty(), "warnings: {warnings:?}");
        assert_eq!(
            registry.block_by_name("router").expect("router").health,
            777
        );
    }

    /// PatcherTests.unitWeapons: append a weapon with an inline
    /// `LightningBulletType`; `unapply` restores the list and drops the bullet.
    #[test]
    fn unit_weapons_append() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").weapons.clone();
        let bullets_before = registry.bullets().len();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"unit.dagger.weapons.+": {"name":"navanax-weapon","bullet":{"type":"LightningBulletType","lightningLength":999}}}"#,
                )],
            )
            .expect("append weapon");
        let unit = registry.unit(id).expect("unit");
        assert_eq!(unit.weapons.len(), original.len() + 1);
        let created = unit.weapons.last().expect("created weapon");
        assert_eq!(created.name, "navanax-weapon");
        assert!(registry.bullets().len() > bullets_before);
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").weapons, original);
        assert_eq!(registry.bullets().len(), bullets_before);
    }

    /// Plan 20 C10: a bullet created by a patch runs the created-object `init()`
    /// lifecycle (`DataPatcher.created`); `BulletType.init` sets `pierce` when
    /// `pierceCap >= 1`, observable proof the callback ran.
    #[test]
    fn created_bullet_runs_init_lifecycle() {
        let mut registry = test_registry();
        let bullets_before = registry.bullets().len();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"unit.dagger.weapons.+": {"name":"lifecycle-weapon","bullet":{"type":"LaserBulletType","pierceCap":2,"damage":5}}}"#,
                )],
            )
            .expect("append weapon");
        let created = &registry.bullets()[bullets_before..];
        assert_eq!(created.len(), 1, "one inline bullet registered");
        assert_eq!(created[0].pierce_cap, 2);
        assert!(
            created[0].pierce,
            "created bullet init() lifecycle did not run"
        );
        patcher.unapply(&mut registry);
        assert_eq!(registry.bullets().len(), bullets_before);
    }

    /// PatcherTests.uUnitWeaponReassign: whole-array replace, reset restores.
    #[test]
    fn unit_weapons_reassign() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").weapons.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"unit.dagger.weapons": [{"name":"megapoop","bullet":{"type":"RailBulletType","lightningLength":999}}]}"#,
                )],
            )
            .expect("reassign weapons");
        let weapons = &registry.unit(id).expect("unit").weapons;
        assert_eq!(weapons.len(), 1);
        assert_eq!(weapons[0].name, "megapoop");
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").weapons, original);
    }

    /// PatcherTests.arrayMulti: index-keyed object replacement + `weapons.+`
    /// append in one patch; unapply restores the authored list.
    #[test]
    fn array_multi_edit() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").weapons.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"name":"Patch0","unit":{"dagger":{
                        "weapons":{"0":{"type":"Weapon","name":"toxopid-cannon"}},
                        "weapons.+":[{"name":"sei-launcher"}]}}}"#,
                )],
            )
            .expect("array multi");
        let weapons = registry.unit(id).expect("unit").weapons.clone();
        assert_eq!(weapons.len(), original.len() + 1);
        assert_eq!(weapons[0].name, "toxopid-cannon");
        assert_eq!(weapons.last().expect("appended").name, "sei-launcher");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").weapons, original);
    }

    /// Plan 20 M3b: `fix_content_arrays` world half grows live building
    /// `ItemModule`/`LiquidModule` arrays to the dense content counts.
    #[test]
    fn fix_world_content_arrays_grows_live_modules() {
        let registry = test_registry();
        let item_count = registry.items().len();
        let liquid_count = registry.liquids().len();

        let mut world = bevy_ecs::world::World::new();
        let items = world.spawn(crate::world::ItemModule::with_items(1)).id();
        let liquids = world
            .spawn(crate::world::LiquidModule::with_liquids(1))
            .id();
        world
            .get_mut::<crate::world::ItemModule>(items)
            .expect("item module")
            .add(ItemId::COPPER, 3, 10);

        fix_world_content_arrays(&mut world, &registry);

        let module = world
            .get::<crate::world::ItemModule>(items)
            .expect("module");
        assert_eq!(module.items.len(), item_count);
        assert_eq!(module.get(ItemId::COPPER), 3, "growth preserves counts");
        assert_eq!(
            world
                .get::<crate::world::LiquidModule>(liquids)
                .expect("liquid module")
                .liquids
                .len(),
            liquid_count
        );
    }

    /// Plan 20 C10: `fix_content_arrays` resizes a block's dense item tables to
    /// the live item count (`Block.checkContentArrayCapacity` grow/shrink).
    #[test]
    fn fix_content_arrays_resizes_block_item_tables() {
        let mut registry = test_registry();
        let item_count = registry.items().len();
        let id = registry.block_id("router").expect("router");
        {
            let block = registry.block_mut(id).expect("block");
            block.item_costs.truncate(1);
            block.item_health_scaling.truncate(1);
        }
        fix_content_arrays(&mut registry);
        {
            let block = registry.block(id).expect("block");
            assert_eq!(block.item_costs.len(), item_count);
            assert_eq!(block.item_health_scaling.len(), item_count);
            assert_eq!(
                block.item_costs[0],
                registry.item(ItemId::new(0)).expect("item").cost
            );
        }
        // Growing past the live count shrinks back (`Arrays.copyOf` parity).
        registry
            .block_mut(id)
            .expect("block")
            .item_costs
            .push(999.0);
        fix_content_arrays(&mut registry);
        assert_eq!(
            registry.block(id).expect("block").item_costs.len(),
            item_count
        );
    }

    /// PatcherTests.unitFlagsArray: `targetFlags.+` array append + reset.
    #[test]
    fn unit_target_flags_append() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").target_flags.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"unit.dagger.targetFlags.+": ["shield", "drill"]}"#,
                )],
            )
            .expect("append flags");
        let flags = registry.unit(id).expect("unit").target_flags.clone();
        assert_eq!(flags.len(), original.len() + 2);
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").target_flags, original);
    }

    /// PatcherTests.unitType: `type` changes the unit entity def; reset restores.
    #[test]
    fn unit_type_controller_change() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").entity_def;
        let mut patcher = DataPatcher::new();
        patcher
            .apply(&mut registry, &[patch(r#"{"unit.dagger.type": "legs"}"#)])
            .expect("change type");
        assert_ne!(registry.unit(id).expect("unit").entity_def, original);
        assert_eq!(
            registry.unit(id).expect("unit").entity_def,
            crate::mods::json::resolve_entity_def("legs").expect("legs")
        );
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").entity_def, original);
    }

    /// PatcherTests.requiredPlanets gating (see plan 20 §3.6).
    #[test]
    fn required_planets_gate() {
        let mut registry = test_registry();
        let id = registry.block_id("router").expect("router");
        let original = registry.block(id).expect("block").health;
        let asset = patch(r#"{"requiredPlanets":["erekir"],"block.router.health": 4242}"#);
        // Active planet is serpulo → skipped.
        let mut patcher = DataPatcher::new();
        patcher
            .apply_with_planet(&mut registry, std::slice::from_ref(&asset), Some("serpulo"))
            .expect("gated");
        assert_eq!(registry.block(id).expect("block").health, original);
        patcher.unapply(&mut registry);
        // Active planet is erekir → applied.
        let mut patcher = DataPatcher::new();
        patcher
            .apply_with_planet(&mut registry, &[asset], Some("erekir"))
            .expect("applied");
        assert_eq!(registry.block(id).expect("block").health, 4242);
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").health, original);
    }

    /// PatcherTests.objectFloatMap: `ObjectFloatMap<Item>` edits (object form,
    /// nested-object form, dotted key) with reset.
    #[test]
    fn object_float_map_edit() {
        let mut registry = test_registry();
        let id = registry.block_id("mechanical-drill").expect("drill");
        let titanium = registry.item_id("titanium").expect("titanium");
        let copper = registry.item_id("copper").expect("copper");
        let surge = registry.item_id("surge-alloy").expect("surge-alloy");
        let original = registry.block(id).expect("block").drill_multipliers.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{
                        "block.mechanical-drill.drillMultipliers": {"titanium": 2.0},
                        "block.mechanical-drill": {"drillMultipliers": {"copper": 3.0}},
                        "block.mechanical-drill.drillMultipliers.surge-alloy": 10
                    }"#,
                )],
            )
            .expect("drill multipliers");
        let block = registry.block(id).expect("block");
        let get = |item| {
            block
                .drill_multipliers
                .iter()
                .find(|(existing, _)| *existing == item)
                .map(|(_, value)| *value)
        };
        assert_eq!(get(titanium), Some(2.0));
        assert_eq!(get(copper), Some(3.0));
        assert_eq!(get(surge), Some(10.0));
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        patcher.unapply(&mut registry);
        assert_eq!(
            registry.block(id).expect("block").drill_multipliers,
            original
        );
    }

    /// PatcherTests.attributes: `Attributes` object + dotted-key edits with reset.
    #[test]
    fn attributes_edit() {
        let mut registry = test_registry();
        let id = registry.block_id("grass").expect("grass");
        let original = registry.block(id).expect("block").attributes.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"block.grass.attributes": {"oil": 99}, "block.grass.attributes.heat": 77}"#,
                )],
            )
            .expect("attributes");
        let block = registry.block(id).expect("block");
        let get = |name: &str| {
            block
                .attributes
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| *value)
        };
        assert_eq!(get("oil"), Some(99.0));
        assert_eq!(get("heat"), Some(77.0));
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").attributes, original);
    }

    /// PatcherTests.customAttribute: a custom attribute name is added and removed
    /// by reset (the dynamic `Attribute` registry itself is plan 02/06).
    #[test]
    fn custom_attribute_add_remove() {
        let mut registry = test_registry();
        let id = registry.block_id("grass").expect("grass");
        let original = registry.block(id).expect("block").attributes.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"block.grass.attributes": {"frogs": 10}}"#)],
            )
            .expect("custom attribute");
        assert!(
            registry
                .block(id)
                .expect("block")
                .attributes
                .iter()
                .any(|(key, value)| key == "frogs" && (*value - 10.0).abs() < 1e-6)
        );
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").attributes, original);
    }

    /// PatcherTests.indexAccess: `weapons.0.bullet.damage` deep field edit + reset.
    #[test]
    fn weapon_index_field_edit() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let weapon_bullet = registry
            .unit(id)
            .expect("unit")
            .weapons
            .first()
            .expect("weapon")
            .bullet
            .id;
        let original = registry.bullet(weapon_bullet).expect("bullet").damage;
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"unit.dagger.weapons.0.bullet.damage": 100}"#)],
            )
            .expect("deep bullet edit");
        assert_eq!(
            registry.bullet(weapon_bullet).expect("bullet").damage,
            100.0
        );
        patcher.unapply(&mut registry);
        assert_eq!(
            registry.bullet(weapon_bullet).expect("bullet").damage,
            original
        );
    }

    /// PatcherTests.unitAbilities: single `+` ability with reset.
    #[test]
    fn unit_abilities_single() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").abilities.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"unit.dagger.abilities.+": {"type":"ShieldArcAbility","max":1000}}"#,
                )],
            )
            .expect("append ability");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        let abilities = registry.unit(id).expect("unit").abilities.clone();
        assert_eq!(abilities.len(), original.len() + 1);
        let added = abilities.last().expect("ability");
        assert_eq!(added.kind.name(), "ShieldArcAbility");
        assert_eq!(added.max, 1000.0);
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").abilities, original);
    }

    /// PatcherTests.unitAbilitiesArray: `+` with an array of abilities.
    #[test]
    fn unit_abilities_array() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").abilities.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"unit.dagger.abilities.+": [
                        {"type":"ShieldArcAbility","max":1000},
                        {"type":"MoveEffectAbility","amount":10}
                    ]}"#,
                )],
            )
            .expect("append abilities");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        let abilities = registry.unit(id).expect("unit").abilities.clone();
        assert_eq!(abilities.len(), original.len() + 2);
        assert_eq!(
            abilities[abilities.len() - 2].kind.name(),
            "ShieldArcAbility"
        );
        assert_eq!(
            abilities[abilities.len() - 1].kind.name(),
            "MoveEffectAbility"
        );
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").abilities, original);
    }

    /// PatcherTests.unitFactoryPlans (flat): `block.ground-factory.plans.+`.
    #[test]
    fn unit_factory_plans_flat() {
        let mut registry = test_registry();
        let id = registry.block_id("ground-factory").expect("ground-factory");
        let flare = registry.unit_id("flare").expect("flare");
        let surge = registry.item_id("surge-alloy").expect("surge-alloy");
        let original = registry.block(id).expect("block").unit_plans.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"block.ground-factory.plans.+":{"unit":"flare","requirements":["surge-alloy/10"],"time":100}}"#,
                )],
            )
            .expect("add plan");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        let plan = registry
            .block(id)
            .expect("block")
            .unit_plans
            .last()
            .expect("plan");
        assert_eq!(plan.unit, flare);
        assert_eq!(plan.time, 100.0);
        assert_eq!(plan.requirements.len(), 1);
        assert_eq!(plan.requirements[0].item, surge);
        assert_eq!(plan.requirements[0].amount, 10);
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").unit_plans, original);
    }

    /// PatcherTests.unitFactoryPlans (nested): `block.{ground-factory:{plans.+}}`.
    #[test]
    fn unit_factory_plans_nested() {
        let mut registry = test_registry();
        let id = registry.block_id("ground-factory").expect("ground-factory");
        let original = registry.block(id).expect("block").unit_plans.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"block":{"ground-factory":{"plans.+":{"unit":"flare","requirements":["surge-alloy/10"],"time":100}}}}"#,
                )],
            )
            .expect("add plan");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        assert_eq!(
            registry.block(id).expect("block").unit_plans.len(),
            original.len() + 1
        );
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").unit_plans, original);
    }

    /// PatcherTests.specificArrayRequirements (object-index form).
    #[test]
    fn array_requirements_object_index() {
        let mut registry = test_registry();
        let id = registry.block_id("scatter").expect("scatter");
        let original = registry.block(id).expect("block").requirements.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"block.scatter.requirements":{"0":"titanium/99"}}"#,
                )],
            )
            .expect("index edit");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        assert_eq!(
            registry.block(id).expect("block").requirements[0].amount,
            99
        );
        assert_eq!(
            registry.block(id).expect("block").requirements[1],
            original[1]
        );
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").requirements, original);
    }

    /// PatcherTests.gibberish: malformed JSON warns, never panics.
    #[test]
    fn malformed_patch_warns() {
        let mut registry = test_registry();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(&mut registry, &[patch("}[35209509()jfkjhadsf,\n,,,,[][]{")])
            .expect("no panic");
        assert_eq!(patcher.warnings().len(), 1);
    }

    /// PatcherTests.unitTypeObject: `{"unit.dagger": {"type": "legs"}}` form.
    #[test]
    fn unit_type_object_syntax() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").entity_def;
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"unit.dagger": {"type": "legs"}}"#)],
            )
            .expect("object syntax");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        assert_eq!(
            registry.unit(id).expect("unit").entity_def,
            crate::mods::json::resolve_entity_def("legs").expect("legs")
        );
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").entity_def, original);
    }

    /// PatcherTests.reconstructorPlans: `Seq<UnitType[]>` replace + typed
    /// `consumes` items merge, both restored on unapply.
    #[test]
    fn reconstructor_plans_and_consumes() {
        let mut registry = test_registry();
        let id = registry
            .block_id("additive-reconstructor")
            .expect("additive-reconstructor");
        let dagger = registry.unit_id("dagger").expect("dagger");
        let flare = registry.unit_id("flare").expect("flare");
        let surge = registry.item_id("surge-alloy").expect("surge-alloy");
        let copper = registry.item_id("copper").expect("copper");
        let original = registry
            .block(id)
            .expect("block")
            .reconstructor_upgrades
            .clone();
        let original_consumes = registry.block(id).expect("block").consumes.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{
                        "block.additive-reconstructor.upgrades": [["dagger", "flare"]],
                        "block.additive-reconstructor.consumes": {
                            "remove": "items",
                            "items": ["surge-alloy/10", "copper/20"]
                        }
                    }"#,
                )],
            )
            .expect("reconstructor patch");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        let block = registry.block(id).expect("block");
        assert_eq!(block.reconstructor_upgrades, vec![(dagger, flare)]);
        let items = block
            .consumes
            .iter()
            .find_map(|spec| match &spec.consume {
                Consume::Items(stacks) => Some(stacks.clone()),
                _ => None,
            })
            .expect("items consumer");
        assert_eq!(
            items,
            vec![ItemStack::new(surge, 10), ItemStack::new(copper, 20)]
        );
        patcher.unapply(&mut registry);
        let block = registry.block(id).expect("block");
        assert_eq!(block.reconstructor_upgrades, original);
        assert_eq!(block.consumes, original_consumes);
    }

    /// PatcherTests.reconstructorPlansEditSpecific: index form `upgrades.1`.
    #[test]
    fn reconstructor_index_edit() {
        let mut registry = test_registry();
        let id = registry
            .block_id("additive-reconstructor")
            .expect("additive-reconstructor");
        let dagger = registry.unit_id("dagger").expect("dagger");
        let flare = registry.unit_id("flare").expect("flare");
        let original = registry
            .block(id)
            .expect("block")
            .reconstructor_upgrades
            .clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"block.additive-reconstructor.upgrades.1": ["dagger", "flare"]}"#,
                )],
            )
            .expect("index edit");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        assert_eq!(
            registry.block(id).expect("block").reconstructor_upgrades[1],
            (dagger, flare)
        );
        patcher.unapply(&mut registry);
        assert_eq!(
            registry.block(id).expect("block").reconstructor_upgrades,
            original
        );
    }

    /// PatcherTests.reconstructorPlansAdd: `+` append form.
    #[test]
    fn reconstructor_append() {
        let mut registry = test_registry();
        let id = registry
            .block_id("additive-reconstructor")
            .expect("additive-reconstructor");
        let dagger = registry.unit_id("dagger").expect("dagger");
        let flare = registry.unit_id("flare").expect("flare");
        let original_len = registry
            .block(id)
            .expect("block")
            .reconstructor_upgrades
            .len();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"block.additive-reconstructor.upgrades.+": [["dagger", "flare"]]}"#,
                )],
            )
            .expect("append upgrade");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        let ups = registry
            .block(id)
            .expect("block")
            .reconstructor_upgrades
            .clone();
        assert_eq!(ups.len(), original_len + 1);
        assert_eq!(*ups.last().expect("upgrade"), (dagger, flare));
        patcher.unapply(&mut registry);
        assert_eq!(
            registry
                .block(id)
                .expect("block")
                .reconstructor_upgrades
                .len(),
            original_len
        );
    }

    /// PatcherTests.nestedArrays / nestedArrays2: object-index and dotted
    /// nested forms both set one `UnitType[]` pair element.
    #[test]
    fn nested_array_edit_forms() {
        for json in [
            r#"{"block.ship-refabricator.upgrades.0": {"0": "dagger", "1": "mace"}}"#,
            r#"{"block": {"ship-refabricator": {"upgrades.0.0": "dagger", "upgrades.0.1": "mace"}}}"#,
        ] {
            let mut registry = test_registry();
            let id = registry
                .block_id("ship-refabricator")
                .expect("ship-refabricator");
            let dagger = registry.unit_id("dagger").expect("dagger");
            let mace = registry.unit_id("mace").expect("mace");
            let original = registry
                .block(id)
                .expect("block")
                .reconstructor_upgrades
                .clone();
            let mut patcher = DataPatcher::new();
            patcher
                .apply(&mut registry, &[patch(json)])
                .expect("nested edit");
            assert!(
                patcher.warnings().is_empty(),
                "{json}: {:?}",
                patcher.warnings()
            );
            assert_eq!(
                registry.block(id).expect("block").reconstructor_upgrades[0],
                (dagger, mace)
            );
            patcher.unapply(&mut registry);
            assert_eq!(
                registry.block(id).expect("block").reconstructor_upgrades,
                original
            );
        }
    }

    /// PatcherTests.unitFlags: `targetFlags.+` single-string append + reset.
    #[test]
    fn unit_target_flags_single() {
        use crate::content::registries::blocks::BlockFlag;
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").target_flags.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"unit.dagger.targetFlags.+": "shield"}"#)],
            )
            .expect("append flag");
        let flags = registry.unit(id).expect("unit").target_flags.clone();
        assert_eq!(flags.len(), original.len() + 1);
        assert_eq!(*flags.last().expect("flag"), Some(BlockFlag::Shield));
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").target_flags, original);
    }

    /// PatcherTests.assignStringToObject: a string assigned to `weapons` warns
    /// and leaves the array unchanged.
    #[test]
    fn string_to_object_warns() {
        let mut registry = test_registry();
        let id = registry.unit_id("dagger").expect("dagger");
        let original = registry.unit(id).expect("unit").weapons.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"unit.dagger.weapons": ["frog"]}"#)],
            )
            .expect("string to object");
        assert_eq!(patcher.warnings().len(), 1);
        assert_eq!(registry.unit(id).expect("unit").weapons, original);
    }

    /// PatcherTests.noIdAssign: `id` is `@NoPatch`.
    #[test]
    fn id_not_patchable() {
        let mut registry = test_registry();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(&mut registry, &[patch(r#"{"block.router.id": 9231}"#)])
            .expect("id patch");
        assert_eq!(patcher.warnings().len(), 1);
    }

    /// PatcherTests.noResolution: an unresolvable class in the patcher parser
    /// warns (the arbitrary-FQCN fallback is disabled).
    #[test]
    fn no_class_resolution_in_patcher() {
        let mut registry = test_registry();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"block.conveyor.lastConfig": {"class": "mindustry.ai.Pathfinder"}}"#,
                )],
            )
            .expect("no resolution");
        assert_eq!(patcher.warnings().len(), 1);
    }

    /// PatcherTests.singleValue: nested `{"block": {"<name>.<field>": v}}`
    /// scalar form.
    #[test]
    fn nested_type_single_value() {
        let mut registry = test_registry();
        let id = registry.block_id("router").expect("router");
        let original = registry.block(id).expect("block").health;
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(r#"{"block": {"router.health": 9}}"#)],
            )
            .expect("single value");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        assert_eq!(registry.block(id).expect("block").health, 9);
        patcher.unapply(&mut registry);
        assert_eq!(registry.block(id).expect("block").health, original);
    }

    /// PatcherTests.addWeapon: append a fully-formed weapon to `flare`.
    #[test]
    fn weapon_append_object() {
        let mut registry = test_registry();
        let id = registry.unit_id("flare").expect("flare");
        let original = registry.unit(id).expect("unit").weapons.clone();
        let mut patcher = DataPatcher::new();
        patcher
            .apply(
                &mut registry,
                &[patch(
                    r#"{"unit.flare.weapons.+": {
                        "x": 0, "y": 0, "reload": 10,
                        "bullet": {"type": "LaserBulletType", "damage": 100}
                    }}"#,
                )],
            )
            .expect("append weapon");
        assert!(patcher.warnings().is_empty(), "{:?}", patcher.warnings());
        let weapons = registry.unit(id).expect("unit").weapons.clone();
        assert_eq!(weapons.len(), original.len() + 1);
        let added = weapons.last().expect("weapon");
        assert_eq!(
            registry.bullet(added.bullet.id).expect("bullet").damage,
            100.0
        );
        patcher.unapply(&mut registry);
        assert_eq!(registry.unit(id).expect("unit").weapons, original);
    }
}
