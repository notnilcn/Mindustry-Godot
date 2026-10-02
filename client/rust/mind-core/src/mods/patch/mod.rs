// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DataPatcher` (plan 20 §3.6): replace-only JSON data patches.
//!
//! M3: flat dotted keys and nested `type → name → fields` forms for the nine
//! content kinds, `Seq`/array (`+`, numeric index, whole replace),
//! `ObjectSet` (`immunities.+`), `consumes` power merge, `@NoPatch` refusal,
//! `requiredPlanets` gating, created-`afterPatch` on touched content, and
//! `unapply` restoring via recorded `ResetAction`s + the registry index
//! snapshot. `ObjectMap`/`ObjectFloatMap`/`Attributes`, created-object
//! `init/postInit/load`, `fix_content_arrays` growth and per-mod
//! `load_mod_patches` file wiring are tracked in the plan Changelog (M3b).

use indexmap::IndexSet;
use serde_json::{Map, Value};

use crate::content::parser_hooks::{PatchAsset, ResetAction};
use crate::content::snapshot::RegistryIndexSnapshot;
use crate::content::stacks::ItemStack;
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
        self.after_patch_calls = 0;

        for patch in patches {
            let value: Value = serde_json::from_str(&patch.json)
                .map_err(|error| ContentError::Parse(format!("{}: {error}", patch.name)))?;
            let Some(object) = value.as_object() else {
                return Err(ContentError::Parse(format!(
                    "{}: patch must be a JSON object",
                    patch.name
                )));
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
        // Created content runs `afterPatch()` once after the traversal.
        registry
            .after_patch()
            .map_err(|error| ContentError::Parse(error.to_string()))?;
        self.after_patch_calls += 1;
        fix_content_arrays(registry);
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
            "consumes" => self.edit_consumes(registry, reference, id, mode, value),
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

    /// Merges a `consumes` object (power + `remove` semantics; M3 subset).
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
        let Some(original) = registry
            .block(id)
            .map(|b| (b.consumes.clone(), b.has_power))
        else {
            return;
        };
        let raw = id.raw();
        if self.mark_used(reference, "consumes") {
            self.resetters
                .push(Box::new(move |r: &mut ContentRegistry| {
                    if let Some(block) = r.block_mut(BlockId::new(raw)) {
                        block.consumes = original.0.clone();
                        block.has_power = original.1;
                    }
                }));
        }
        let remove_all = map.get("remove").and_then(Value::as_str) == Some("all");
        let power = map.get("power").and_then(Value::as_f64).map(|v| v as f32);
        let buffered = map
            .get("powerBuffered")
            .and_then(Value::as_f64)
            .map(|v| v as f32);
        if let Some(block) = registry.block_mut(id) {
            if remove_all {
                block.consumes.clear();
            }
            if power.is_some() || buffered.is_some() {
                block.consumes.push(ConsumeSpec {
                    consume: Consume::Power {
                        usage: power.unwrap_or(0.0),
                        buffered: buffered.unwrap_or(0.0),
                    },
                    optional: false,
                    update: false,
                    ignore: false,
                });
                block.has_power = true;
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
        let entries: Vec<&Map<String, Value>> = match value {
            Value::Object(map) => vec![map],
            Value::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    match item.as_object() {
                        Some(map) => out.push(map),
                        None => {
                            self.warn("weapon entries must be objects");
                            return;
                        }
                    }
                }
                out
            }
            _ => {
                self.warn("`weapons` must be an object or array");
                return;
            }
        };
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
        let mut parser = crate::mods::json::ContentJsonParser::new();
        let mut created = Vec::with_capacity(entries.len());
        for (index, object) in entries.iter().enumerate() {
            match parser.parse_weapon_object(registry, "patch", object, index) {
                Ok(weapon) => created.push(weapon),
                Err(error) => {
                    self.warn(error.message);
                    return;
                }
            }
        }
        if let Some(unit) = registry.unit_mut(UnitTypeId::new(raw)) {
            match mode {
                FieldMode::Set => unit.weapons = created,
                FieldMode::Append => unit.weapons.extend(created),
                FieldMode::Index(index) => {
                    if index < unit.weapons.len() && !created.is_empty() {
                        unit.weapons[index] = created.swap_remove(0);
                    }
                }
            }
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

/// `DataPatcher.fixContentArrays`: grows item/liquid/block-backed arrays and
/// editor tile references after patch content added new content (plan 09/19).
///
/// The registry rebuilds its arrays from `arr_epoch` consumers in plans 07/09;
/// this hook exists so M3 callers keep the upstream call site. Growth of the
/// concrete `ItemSeq`/`ItemModule` tables lands with plans 07/09 when those
/// arrays exist in the port.
pub fn fix_content_arrays(_registry: &mut ContentRegistry) {}

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
}
