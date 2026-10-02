// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SpawnGroup` — wave spawn descriptor (plan 11 §3.10).
//!
//! Ported from `core/src/mindustry/game/SpawnGroup.java`: the [JSON
//! serialization][`SpawnGroup::to_json`] (exact upstream keys, defaults omitted),
//! `getSpawned`/`getShield` scaling math and `createUnit` payload/effect/items
//! handoff. Unit payload entities come from plan 08's payload API; the actual
//! carrier pickup is asserted in `createUnit`'s tests.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use serde_json::{Map, Value};

use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::{ItemsComp, ShieldComp, StatusComp, StatusEntry};
use crate::entities::comp::unit::lifecycle::spawn_unit;
use crate::world::blocks::payloads::{
    PayloadHolder, PayloadKind, UnitPayloadSize, handle_payload, payload_ref,
};

/// `SpawnGroup.never` (`Integer.MAX_VALUE`).
pub const NEVER: i32 = i32::MAX;

/// An item stack spawned with a unit (`arc.struct.ItemStack`, JSON form).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemStack {
    /// Item content name.
    pub item: String,
    /// Amount.
    pub amount: i32,
}

impl ItemStack {
    /// Creates a stack.
    pub fn new(item: impl Into<String>, amount: i32) -> Self {
        Self {
            item: item.into(),
            amount,
        }
    }
}

/// A wave spawn group (`SpawnGroup`).
#[derive(Debug, Clone, PartialEq)]
pub struct SpawnGroup {
    /// Unit type name (`type`; resolved through content on creation).
    pub unit: String,
    /// First wave (`begin`).
    pub begin: i32,
    /// Last wave (`end`; `NEVER` = unbounded).
    pub end: i32,
    /// Spawn spacing, in waves (`spacing`).
    pub spacing: i32,
    /// Maximum units (`max`).
    pub max: i32,
    /// Waves per +1 unit (`unitScaling`).
    pub unit_scaling: f32,
    /// Base shields (`shields`).
    pub shields: f32,
    /// Shields gained per wave (`shieldScaling`).
    pub shield_scaling: f32,
    /// Initial amount (`unitAmount`).
    pub unit_amount: i32,
    /// Restrict to one packed spawn position (`spawn`; `-1` = any).
    pub spawn: i32,
    /// Payload unit-type names (`payloads`).
    pub payloads: Option<Vec<String>>,
    /// Status effect name (`effect`).
    pub effect: Option<String>,
    /// Items spawned with the unit (`items`).
    pub items: Option<ItemStack>,
    /// Team override (`team`; `None` = wave team).
    pub team: Option<u8>,
}

impl Default for SpawnGroup {
    fn default() -> Self {
        Self {
            unit: String::from("dagger"),
            begin: 0,
            end: NEVER,
            spacing: 1,
            max: 40,
            unit_scaling: NEVER as f32,
            shields: 0.0,
            shield_scaling: 0.0,
            unit_amount: 1,
            spawn: -1,
            payloads: None,
            effect: None,
            items: None,
            team: None,
        }
    }
}

impl SpawnGroup {
    /// Creates a group for `unit` with upstream defaults.
    pub fn new(unit: impl Into<String>) -> Self {
        Self {
            unit: unit.into(),
            ..Self::default()
        }
    }

    /// `SpawnGroup.canSpawn(position)`.
    pub fn can_spawn(&self, position: i32) -> bool {
        self.spawn == -1 || self.spawn == position
    }

    /// `SpawnGroup.getSpawned(wave)`.
    pub fn get_spawned(&self, wave: i32) -> i32 {
        // Upstream mutates `spacing = 1` when 0; observable behavior is the same.
        let spacing = self.spacing.max(1);
        if wave < self.begin || wave > self.end || (wave - self.begin) % spacing != 0 {
            return 0;
        }
        let steps = (wave - self.begin) / spacing;
        let scaled = steps as f32 / self.unit_scaling;
        self.unit_amount.saturating_add(scaled as i32).min(self.max)
    }

    /// `SpawnGroup.getShield(wave)`.
    pub fn get_shield(&self, wave: i32) -> f32 {
        (self.shields + self.shield_scaling * (wave - self.begin) as f32).max(0.0)
    }

    /// Creates the group's unit and applies effect/items/shield/payloads.
    ///
    /// Mirrors `SpawnGroup.createUnit(team, x, y, rotation, wave, cons)`; the
    /// `cons` callback of upstream is a plan-11 host hook and is intentionally
    /// omitted (no caller needs it yet).
    #[allow(clippy::too_many_arguments)]
    pub fn create_unit(
        &self,
        world: &mut World,
        content: &ContentRegistry,
        seq: u64,
        team: u8,
        x: f32,
        y: f32,
        rotation: f32,
        wave: i32,
    ) -> Option<Entity> {
        let unit = spawn_unit(world, content, seq, &self.unit, team, x, y, rotation)?;

        if let Some(effect) = &self.effect
            && let Some(status) = content.status_by_name(effect)
            && let Some(mut state) = world.get_mut::<StatusComp>(unit)
        {
            state.apply(StatusEntry {
                effect: status.id,
                duration: 999_999.0,
            });
        }

        if let Some(items) = &self.items
            && let Some(item) = content.item_by_name(&items.item)
            && let Some(mut comp) = world.get_mut::<ItemsComp>(unit)
        {
            comp.add_item(item.id, items.amount);
        }

        if let Some(mut shield) = world.get_mut::<ShieldComp>(unit) {
            shield.shield = self.get_shield(wave);
        }

        if let Some(payloads) = &self.payloads {
            // Pickups require the payload carrier state (plan 08). Insert it
            // lazily; payload-capable unit defs get it from lifecycle already.
            if world.get::<PayloadHolder>(unit).is_none() {
                world.entity_mut(unit).insert(PayloadHolder::default());
            }
            let mut payload_seq = seq.wrapping_add(1);
            for name in payloads {
                let Some(payload_type) = content.unit_by_name(name) else {
                    continue;
                };
                let Some(payload) =
                    spawn_unit(world, content, payload_seq, name, team, x, y, rotation)
                else {
                    continue;
                };
                payload_seq = payload_seq.wrapping_add(1);
                world
                    .entity_mut(payload)
                    .insert(UnitPayloadSize(payload_type.hit_size));
                let handle = payload_ref(PayloadKind::Unit, payload, payload_type.id.raw());
                handle_payload(world, unit, unit, handle);
            }
        }

        Some(unit)
    }

    /// Serializes to upstream `write(Json)` shape (defaults omitted).
    pub fn to_json(&self) -> Value {
        let mut map = Map::new();
        map.insert("type".into(), Value::String(self.unit.clone()));
        if self.begin != 0 {
            map.insert("begin".into(), Value::from(self.begin));
        }
        if self.end != NEVER {
            map.insert("end".into(), Value::from(self.end));
        }
        if self.spacing != 1 {
            map.insert("spacing".into(), Value::from(self.spacing));
        }
        if self.max != 40 {
            map.insert("max".into(), Value::from(self.max));
        }
        if self.unit_scaling != NEVER as f32 {
            map.insert("scaling".into(), Value::from(self.unit_scaling));
        }
        if self.shields != 0.0 {
            map.insert("shields".into(), Value::from(self.shields));
        }
        if self.shield_scaling != 0.0 {
            map.insert("shieldScaling".into(), Value::from(self.shield_scaling));
        }
        if self.unit_amount != 1 {
            map.insert("amount".into(), Value::from(self.unit_amount));
        }
        if self.spawn != -1 {
            map.insert("spawn".into(), Value::from(self.spawn));
        }
        if let Some(payloads) = &self.payloads
            && !payloads.is_empty()
        {
            map.insert(
                "payloads".into(),
                Value::Array(payloads.iter().cloned().map(Value::String).collect()),
            );
        }
        if let Some(effect) = &self.effect {
            map.insert("effect".into(), Value::String(effect.clone()));
        }
        if let Some(items) = &self.items
            && items.amount > 0
        {
            let mut item_map = Map::new();
            item_map.insert("item".into(), Value::String(items.item.clone()));
            item_map.insert("amount".into(), Value::from(items.amount));
            map.insert("items".into(), Value::Object(item_map));
        }
        if let Some(team) = self.team {
            map.insert("team".into(), Value::from(team));
        }
        Value::Object(map)
    }

    /// Parses the upstream `read(Json)` shape.
    ///
    /// `unitMap` legacy rewriting and the numeric boss effect (`8`) are applied
    /// by the caller (plan 04/12 own `LegacyIO`); a numeric effect `8` is left as
    /// the string `"boss"` here.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let obj = value
            .as_object()
            .ok_or_else(|| String::from("spawn group must be a JSON object"))?;
        let mut group = SpawnGroup::default();
        if let Some(name) = obj.get("type").and_then(Value::as_str) {
            group.unit = name.to_owned();
        }
        if let Some(v) = obj.get("begin").and_then(Value::as_i64) {
            group.begin = v as i32;
        }
        if let Some(v) = obj.get("end").and_then(Value::as_i64) {
            group.end = v as i32;
        }
        if let Some(v) = obj.get("spacing").and_then(Value::as_i64) {
            group.spacing = v as i32;
        }
        if let Some(v) = obj.get("max").and_then(Value::as_i64) {
            group.max = v as i32;
        }
        if let Some(v) = obj.get("scaling").and_then(Value::as_f64) {
            group.unit_scaling = v as f32;
        }
        if let Some(v) = obj.get("shields").and_then(Value::as_f64) {
            group.shields = v as f32;
        }
        if let Some(v) = obj.get("shieldScaling").and_then(Value::as_f64) {
            group.shield_scaling = v as f32;
        }
        if let Some(v) = obj.get("amount").and_then(Value::as_i64) {
            group.unit_amount = v as i32;
        }
        if let Some(v) = obj.get("spawn").and_then(Value::as_i64) {
            group.spawn = v as i32;
        }
        if let Some(v) = obj.get("payloads").and_then(Value::as_array) {
            group.payloads = Some(
                v.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
            );
        }
        match obj.get("effect") {
            Some(Value::String(effect)) => group.effect = Some(effect.clone()),
            // Legacy numeric boss effect (`8` => `StatusEffects.boss`).
            Some(Value::Number(number)) if number.as_i64() == Some(8) => {
                group.effect = Some(String::from("boss"));
            }
            _ => {}
        }
        if let Some(items) = obj.get("items").and_then(Value::as_object) {
            let item = items
                .get("item")
                .and_then(Value::as_str)
                .ok_or_else(|| String::from("spawn group items.item missing"))?;
            let amount = items.get("amount").and_then(Value::as_i64).unwrap_or(0) as i32;
            group.items = Some(ItemStack::new(item, amount));
        }
        if let Some(v) = obj.get("team").and_then(Value::as_i64) {
            group.team = Some(v as u8);
        }
        Ok(group)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn get_spawned_bounds() {
        let mut group = SpawnGroup::new("dagger");
        group.unit_scaling = 2.0;
        group.unit_amount = 1;
        group.spacing = 2;
        group.max = 10;
        // Out of window.
        assert_eq!(group.get_spawned(-1), 0);
        assert_eq!(group.get_spawned(1), 0, "odd wave with spacing 2");
        // begin 0: `unitAmount + trunc(steps/unitScaling)`.
        assert_eq!(group.get_spawned(0), 1);
        assert_eq!(group.get_spawned(2), 1, "0.5 truncates to 0");
        assert_eq!(group.get_spawned(4), 2);
        assert_eq!(group.get_spawned(8), 3);
        // Clamped at max.
        assert_eq!(group.get_spawned(100), 10);
        // Past end.
        group.end = 6;
        assert_eq!(group.get_spawned(8), 0);
    }

    #[test]
    fn spacing_zero_is_treated_as_one() {
        let mut group = SpawnGroup::new("dagger");
        group.spacing = 0;
        group.unit_scaling = 1.0;
        assert_eq!(group.get_spawned(0), 1);
        assert_eq!(group.get_spawned(3), 4);
    }

    #[test]
    fn shield_scaling_is_linear_and_never_negative() {
        let mut group = SpawnGroup::new("dagger");
        group.shields = 50.0;
        group.shield_scaling = 10.0;
        group.begin = 5;
        assert_eq!(group.get_shield(5), 50.0);
        assert_eq!(group.get_shield(8), 80.0);
        // Negative scaling clamps at 0.
        group.shields = 20.0;
        group.shield_scaling = -10.0;
        assert_eq!(group.get_shield(5), 20.0);
        assert_eq!(group.get_shield(10), 0.0);
    }

    #[test]
    fn spawn_group_json_round_trips_and_omits_defaults() {
        let minimal = SpawnGroup::new("dagger");
        let json = minimal.to_json();
        assert_eq!(
            json,
            serde_json::json!({ "type": "dagger" }),
            "defaults omitted"
        );
        assert_eq!(SpawnGroup::from_json(&json).unwrap(), minimal);

        let mut full = SpawnGroup::new("mace");
        full.begin = 3;
        full.end = 20;
        full.spacing = 2;
        full.max = 15;
        full.unit_scaling = 1.5;
        full.shields = 100.0;
        full.shield_scaling = 5.0;
        full.unit_amount = 4;
        full.spawn = 42;
        full.payloads = Some(vec![String::from("dagger")]);
        full.effect = Some(String::from("boss"));
        full.items = Some(ItemStack::new("copper", 25));
        full.team = Some(2);
        let json = full.to_json();
        assert_eq!(SpawnGroup::from_json(&json).unwrap(), full);
    }

    #[test]
    fn legacy_numeric_boss_effect_maps() {
        let group = SpawnGroup::from_json(&serde_json::json!({
            "type": "dagger",
            "effect": 8
        }))
        .unwrap();
        assert_eq!(group.effect.as_deref(), Some("boss"));
    }

    #[test]
    fn create_unit_applies_effect_items_shield_and_payloads() {
        let mut harness = BuildHarness::new(32, 32, 1);
        let mut group = SpawnGroup::new("mega"); // payload carrier
        group.shields = 25.0;
        group.items = Some(ItemStack::new("copper", 5));
        group.effect = Some(String::from("boss"));
        group.payloads = Some(vec![String::from("dagger")]);
        let unit = group
            .create_unit(
                &mut harness.world,
                &harness.content,
                123,
                0,
                64.0,
                64.0,
                0.0,
                0,
            )
            .expect("unit");
        assert_eq!(harness.world.get::<ShieldComp>(unit).unwrap().shield, 25.0);
        assert_eq!(
            harness
                .world
                .get::<ItemsComp>(unit)
                .unwrap()
                .item
                .map(|s| s.1),
            Some(5)
        );
        assert!(harness.world.get::<StatusComp>(unit).unwrap().has_effect());
        assert!(
            harness
                .world
                .get::<PayloadHolder>(unit)
                .unwrap()
                .payload
                .is_some()
        );
    }
}
