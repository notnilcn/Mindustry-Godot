// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit patching and the unit weapon/ability/bullet/flag/entity edits.
//!
//! Upstream: core/src/mindustry/mod/DataPatcher.java

use super::load::is_index_map;
use super::*;

impl DataPatcher {
    // ---- unit ----

    pub(super) fn apply_unit(
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
    pub(super) fn edit_immunities(
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
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                    unit.immunities = original;
                }
            });
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
    pub(super) fn edit_weapons(
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
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                    unit.weapons = original;
                }
            });
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
    pub(super) fn edit_abilities(
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
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                    unit.abilities = original;
                }
            });
        }
        let mut parser = crate::mods::json::ContentJsonParser::new();
        let mut created = Vec::with_capacity(entries.len());
        for object in &entries {
            match parser.parse_ability("patch", registry, object) {
                Ok(Some(ability)) => created.push(ability),
                Ok(None) => {
                    for warning in &parser.warnings {
                        self.warn(warning.message.clone());
                    }
                }
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
    pub(super) fn edit_weapon_path(
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
    pub(super) fn edit_bullet_field(
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
                    self.push_reset(move |r: &mut ContentRegistry| {
                        if let Some(bullet) = r.bullet_mut(bullet) {
                            bullet.$f = original;
                        }
                    });
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
                    self.push_reset(move |r: &mut ContentRegistry| {
                        if let Some(bullet) = r.bullet_mut(bullet) {
                            bullet.$f = original;
                        }
                    });
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
                    self.push_reset(move |r: &mut ContentRegistry| {
                        if let Some(bullet) = r.bullet_mut(bullet) {
                            bullet.$f = original;
                        }
                    });
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
    pub(super) fn edit_target_flags(
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
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                    unit.target_flags = original;
                }
            });
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
    pub(super) fn edit_unit_entity(
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
            self.push_reset(move |r: &mut ContentRegistry| {
                if let Some(unit) = r.unit_mut(UnitTypeId::new(raw)) {
                    unit.entity_def = original;
                }
            });
        }
        if let Some(unit) = registry.unit_mut(UnitTypeId::new(raw)) {
            unit.entity_def = def;
        }
    }
}
