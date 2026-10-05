// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Revisioned building IO (`BuildingComp.writeBase`/`readBase`).
//!
//! Ported byte-for-byte from `core/src/mindustry/entities/comp/BuildingComp.java`
//! `:185-282`, plus the module wire formats of
//! `world/modules/{ItemModule,LiquidModule,PowerModule}.java`. Building is a
//! custom-IO def upstream (`@EntityDef(genio=false, serialize=false)`); this
//! module is the custom codec plan 04's entity registry dispatches to
//! (plan 07 §3.10 R3).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::{Building, Health, TeamComp};
use crate::io::IoError;
use crate::io::entity::{EntityReader, EntityWriter};
use crate::world::modules::{ItemModule, LiquidModule, PowerModule};

/// Module presence bit: item module.
pub const MODULE_ITEM: u8 = 1;
/// Module presence bit: power module.
pub const MODULE_POWER: u8 = 1 << 1;
/// Module presence bit: liquid module.
pub const MODULE_LIQUID: u8 = 1 << 2;
/// Module presence bit: legacy consume module (`1 << 3`, always set).
pub const MODULE_CONSUME: u8 = 1 << 3;
/// Module presence bit: time scale.
pub const MODULE_TIMESCALE: u8 = 1 << 4;
/// Module presence bit: last disabler.
pub const MODULE_DISABLER: u8 = 1 << 5;

/// Decoded base fields (entity-independent, for save loading).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DecodedBase {
    /// Health (capped by block health on read).
    pub health: f32,
    /// Rotation.
    pub rotation: u8,
    /// Team id.
    pub team: u8,
    /// Save version byte.
    pub version: u8,
    /// Enabled flag.
    pub enabled: bool,
    /// Present-module bitmask.
    pub module_bits: u8,
    /// Item stacks `(id, amount)`.
    pub items: Vec<(u16, i32)>,
    /// Liquid amounts `(id, amount)`.
    pub liquids: Vec<(u16, f32)>,
    /// Power links + status.
    pub power: Option<(Vec<i32>, f32)>,
    /// Time scale.
    pub time_scale: f32,
    /// Time-scale duration.
    pub time_scale_duration: f32,
    /// Last disabler position (`i32::MIN` = none).
    pub last_disabler: i32,
    /// Efficiency byte.
    pub efficiency: f32,
    /// Optional efficiency byte.
    pub optional_efficiency: f32,
    /// Visibility flags (version 4).
    pub visible_flags: u64,
}

/// Computes `BuildingComp.moduleBitmask()`.
pub fn module_bitmask(world: &World, entity: Entity) -> u8 {
    let mut bits = MODULE_CONSUME;
    if world.get::<ItemModule>(entity).is_some() {
        bits |= MODULE_ITEM;
    }
    if world.get::<PowerModule>(entity).is_some() {
        bits |= MODULE_POWER;
    }
    if world.get::<LiquidModule>(entity).is_some() {
        bits |= MODULE_LIQUID;
    }
    if let Some(building) = world.get::<Building>(entity) {
        if building.time_scale != 1.0 {
            bits |= MODULE_TIMESCALE;
        }
        if building.last_disabler.is_some() {
            bits |= MODULE_DISABLER;
        }
    }
    bits
}

/// Writes `BuildingComp.writeBase`.
///
/// `fog` mirrors `state.rules.fog`; when set and `visibleFlags != 0` the
/// version-4 layout is used. Plan 12 supplies the real rule (default false).
pub fn write_base(
    world: &World,
    entity: Entity,
    w: &mut EntityWriter,
    fog: bool,
) -> Result<(), IoError> {
    let Some(building) = world.get::<Building>(entity) else {
        return Err(IoError::corrupt("entity has no Building component"));
    };
    let health = world.get::<Health>(entity).map(|h| h.health).unwrap_or(0.0);
    let team = world.get::<TeamComp>(entity).map(|t| t.team).unwrap_or(0);

    let write_visibility = fog && building.visible_flags != 0;

    w.f(health);
    w.b((building.rotation | 0x80) as i8);
    w.b(team as i8);
    // The version byte is emitted once by the plan-04 save chunk writer
    // (`WriteContext::write_building`); `read_base` receives it as an argument.
    w.b(if building.enabled { 1 } else { 0 });
    w.b(module_bitmask(world, entity) as i8);

    if let Some(items) = world.get::<ItemModule>(entity) {
        write_items(items, w);
    }
    if let Some(power) = world.get::<PowerModule>(entity) {
        write_power(power, w);
    }
    if let Some(liquids) = world.get::<LiquidModule>(entity) {
        write_liquids(liquids, w);
    }

    if building.time_scale != 1.0 {
        w.f(building.time_scale);
        w.f(building.time_scale_duration);
    }
    if let Some(disabler) = building.last_disabler
        && let Some(disabled) = world.get::<Building>(disabler)
    {
        w.i(disabled.tile.pack());
    }

    w.b((building.efficiency.clamp(0.0, 1.0) * 255.0) as i8);
    w.b((building.optional_efficiency.clamp(0.0, 1.0) * 255.0) as i8);

    if write_visibility {
        w.l(building.visible_flags as i64);
    }
    Ok(())
}

fn write_items(items: &ItemModule, w: &mut EntityWriter) {
    let count = items.items.iter().filter(|amount| **amount > 0).count();
    w.s(count as i16);
    for (index, amount) in items.items.iter().enumerate() {
        if *amount > 0 {
            w.s(index as i16);
            w.i(*amount);
        }
    }
}

fn write_liquids(liquids: &LiquidModule, w: &mut EntityWriter) {
    let count = liquids
        .liquids
        .iter()
        .filter(|amount| **amount > 0.0)
        .count();
    w.s(count as i16);
    for (index, amount) in liquids.liquids.iter().enumerate() {
        if *amount > 0.0 {
            w.s(index as i16);
            w.f(*amount);
        }
    }
}

fn write_power(power: &PowerModule, w: &mut EntityWriter) {
    w.s(power.links.len() as i16);
    for link in &power.links {
        w.i(*link);
    }
    w.f(power.status);
}

/// Reads `BuildingComp.readBase` into a [`DecodedBase`].
pub fn read_base(
    decoded: &mut DecodedBase,
    r: &mut EntityReader,
    version: u8,
) -> Result<(), IoError> {
    decoded.health = r.f()?;
    let rot = r.b()?;
    decoded.team = r.b()? as u8;
    decoded.rotation = (rot & 0x7f) as u8;

    let mut module_bits = 0u8;
    let mut legacy = true;
    if (rot & 0x80u8 as i8) != 0 {
        decoded.version = version;
        module_bits = 0;
        if version >= 1 {
            decoded.enabled = r.b()? == 1;
        }
        if version >= 2 {
            module_bits = r.ub()?;
        }
        legacy = false;
    }
    decoded.module_bits = module_bits;
    decoded.version = version;

    read_modules(decoded, r, module_bits, legacy, version)
}

/// Pinned upstream `BuildingComp.readBase` layout (legacy `MSAV` maps): the
/// per-building save version byte is embedded after the rotation byte.
pub fn read_base_legacy(decoded: &mut DecodedBase, r: &mut EntityReader) -> Result<(), IoError> {
    decoded.health = r.f()?;
    let rot = r.b()?;
    decoded.team = r.b()? as u8;
    decoded.rotation = (rot & 0x7f) as u8;

    let mut module_bits = 0u8;
    let mut legacy = true;
    let mut version = 0u8;
    if (rot & 0x80u8 as i8) != 0 {
        version = r.b()? as u8;
        if version >= 1 {
            decoded.enabled = r.b()? == 1;
        }
        if version >= 2 {
            module_bits = r.ub()?;
        }
        legacy = false;
    }
    decoded.module_bits = module_bits;
    decoded.version = version;

    read_modules(decoded, r, module_bits, legacy, version)
}

/// Module stacks and trailing version-gated fields shared by both base layouts.
fn read_modules(
    decoded: &mut DecodedBase,
    r: &mut EntityReader,
    module_bits: u8,
    legacy: bool,
    version: u8,
) -> Result<(), IoError> {
    if module_bits & MODULE_ITEM != 0 {
        let count = if legacy {
            r.ub()? as i32
        } else {
            r.s()? as i32
        };
        for _ in 0..count.max(0) {
            let item = if legacy {
                r.ub()? as u16
            } else {
                r.s()? as u16
            };
            let amount = r.i()?;
            decoded.items.push((item, amount));
        }
    }
    if module_bits & MODULE_POWER != 0 {
        let amount = r.s()?;
        let mut links = Vec::new();
        for _ in 0..amount.max(0) {
            links.push(r.i()?);
        }
        let mut status = r.f()?;
        if !status.is_finite() {
            status = 0.0;
        }
        decoded.power = Some((links, status));
    }
    if module_bits & MODULE_LIQUID != 0 {
        let count = if legacy {
            r.ub()? as i32
        } else {
            r.s()? as i32
        };
        for _ in 0..count.max(0) {
            let liquid = if legacy {
                r.ub()? as u16
            } else {
                r.s()? as u16
            };
            let amount = r.f()?;
            decoded.liquids.push((liquid, amount));
        }
    }
    if module_bits & MODULE_TIMESCALE != 0 {
        decoded.time_scale = r.f()?;
        decoded.time_scale_duration = r.f()?;
    }
    if module_bits & MODULE_DISABLER != 0 {
        decoded.last_disabler = r.i()?;
    }
    // consume module bool (version <= 2)
    if version <= 2 {
        let _ = r.bool()?;
    }
    if version >= 3 {
        decoded.efficiency = r.ub()? as f32 / 255.0;
        decoded.optional_efficiency = r.ub()? as f32 / 255.0;
    }
    if version == 4 {
        decoded.visible_flags = r.l()? as u64;
    }
    Ok(())
}

/// Applies a [`DecodedBase`] onto an existing building entity (`readBase`).
pub fn apply_base(world: &mut World, entity: Entity, decoded: &DecodedBase, max_health: f32) {
    if let Some(mut health) = world.get_mut::<Health>(entity) {
        health.health = decoded.health.min(max_health);
        health.max_health = max_health;
    }
    if let Some(mut building) = world.get_mut::<Building>(entity) {
        building.rotation = decoded.rotation;
        building.enabled = decoded.enabled;
        building.time_scale = decoded.time_scale;
        building.time_scale_duration = decoded.time_scale_duration;
        building.efficiency = decoded.efficiency;
        building.optional_efficiency = decoded.optional_efficiency;
        building.potential_efficiency = decoded.efficiency;
        building.visible_flags = decoded.visible_flags;
    }
    if let Some(mut team) = world.get_mut::<TeamComp>(entity) {
        team.team = decoded.team;
    }
    if !decoded.items.is_empty()
        && let Some(mut items) = world.get_mut::<ItemModule>(entity)
    {
        items.items.fill(0);
        items.total = 0;
        for (id, amount) in &decoded.items {
            if let Some(slot) = items.items.get_mut(*id as usize) {
                *slot = *amount;
                items.total += *amount;
            }
        }
    }
    if !decoded.liquids.is_empty()
        && let Some(mut liquids) = world.get_mut::<LiquidModule>(entity)
    {
        liquids.liquids.fill(0.0);
        liquids.current_amount = 0.0;
        for (id, amount) in &decoded.liquids {
            if let Some(slot) = liquids.liquids.get_mut(*id as usize) {
                *slot = *amount;
                liquids.current_amount += *amount;
            }
        }
    }
    if let Some((links, status)) = &decoded.power
        && let Some(mut power) = world.get_mut::<PowerModule>(entity)
    {
        power.links = links.iter().copied().collect();
        power.status = *status;
    }
}

/// Per-`BuildingKind` IO revision (`world/building_io.rs` manifests).
pub fn kind_revision(kind: crate::world::BuildingKind) -> u8 {
    kind.revision()
}

/// Manual entity codec for `Building` (upstream `@EntityDef(genio=false,
/// serialize=false)`; plan 07 §3.10 R3). Registered with plan 04 as the
/// `BuildingComp` class; writes `writeBase` then the per-kind behavior fields.
pub struct BuildingCodec;

impl BuildingCodec {
    /// Def name (`revisions/buildings/` + class-id registry).
    pub const NAME: &'static str = "BuildingComp";
    /// Newest base revision (`fog-visibility` layout).
    pub const REVISION: u8 = 4;

    /// Writes a full building entity (base + per-kind).
    pub fn write(
        world: &World,
        entity: Entity,
        w: &mut EntityWriter,
        fog: bool,
    ) -> Result<(), IoError> {
        write_base(world, entity, w, fog)?;
        if let Some(inst) = behavior_instance(world, entity) {
            inst.behavior.write(world, entity, w);
        }
        Ok(())
    }

    /// Reads a full building entity after the save version byte (base then
    /// per-kind using the kind's revision).
    pub fn read(
        world: &mut World,
        entity: Entity,
        r: &mut EntityReader,
        version: u8,
    ) -> Result<(), IoError> {
        let mut decoded = DecodedBase::default();
        read_base(&mut decoded, r, version)?;
        let max_health = behavior_instance(world, entity)
            .map(|inst| inst.def.health.max(0) as f32)
            .unwrap_or(f32::MAX);
        apply_base(world, entity, &decoded, max_health);
        if let Some(inst) = behavior_instance(world, entity) {
            // The per-kind body uses the behavior's own IO version
            // (`Building.version()`), not the base layout revision: turrets
            // report 1/2/3 (`plan 10 §6.3`) while the base stays at
            // `REVISION`. `BuildingKind::revision()` is the logistics-family
            // fallback only.
            let revision = inst.behavior.version(world, entity);
            inst.behavior.read(world, entity, r, revision);
        }
        Ok(())
    }
}

fn behavior_instance(
    world: &World,
    entity: Entity,
) -> Option<std::sync::Arc<crate::world::block::BlockInstance>> {
    let block = world.get::<Building>(entity)?.block;
    world
        .get_resource::<crate::world::block::BlockTable>()?
        .instance(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::io::wire::{WireReader, WireWriter};
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    fn spawn_smelter() -> (EcsWorld, Entity) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("silicon-smelter")
            .expect("silicon-smelter")
            .clone();
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        let entity = inst.spawn(
            &mut world,
            0,
            TilePos::new(3, 3),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        (world, entity)
    }

    #[test]
    fn module_bitmask_roundtrip() {
        let (mut world, entity) = spawn_smelter();
        {
            let mut items = world.get_mut::<ItemModule>(entity).expect("items");
            items.add(crate::content::ItemId::COPPER, 7, 100);
        }
        let bits = module_bitmask(&world, entity);
        assert_eq!(bits & MODULE_ITEM, MODULE_ITEM);
        assert_eq!(bits & MODULE_CONSUME, MODULE_CONSUME);
        let mut bytes = Vec::new();
        {
            let mut writer = WireWriter::new(&mut bytes);
            write_base(&world, entity, &mut writer, false).expect("write");
        }
        let mut reader = WireReader::new(&bytes);
        let mut decoded = DecodedBase::default();
        read_base(&mut decoded, &mut reader, 3).expect("read");
        assert!(decoded.items.contains(&(0, 7)));
    }

    #[test]
    fn legacy_consume_bool_read() {
        // Version-2 base body: health, rot marker, team, enabled, module bits
        // (0), legacy consume bool, efficiency, optional.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&10.0f32.to_be_bytes());
        bytes.push(0x80u8);
        bytes.push(0);
        bytes.push(1);
        bytes.push(0);
        bytes.push(1);
        bytes.push(255);
        bytes.push(255);
        let mut reader = WireReader::new(&bytes);
        let mut decoded = DecodedBase::default();
        read_base(&mut decoded, &mut reader, 2).expect("read legacy");
        assert_eq!(decoded.health, 10.0);
        assert!(decoded.enabled);
    }

    #[test]
    fn building_codec_base_plus_kind_roundtrip() {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("silicon-smelter")
            .expect("silicon-smelter")
            .clone();
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        let src = inst.spawn(
            &mut world,
            0,
            TilePos::new(3, 3),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        inst.behavior.create_state(&mut world, src);
        world.insert_resource(table);
        {
            let mut state = world
                .get_mut::<crate::entities::comp::CrafterState>(src)
                .expect("crafter state");
            state.progress = 0.5;
            state.warmup = 0.25;
        }
        let mut bytes = Vec::new();
        {
            let mut writer = WireWriter::new(&mut bytes);
            BuildingCodec::write(&world, src, &mut writer, false).expect("write");
        }
        let inst = world
            .get_resource::<BlockTable>()
            .expect("table")
            .get_named("silicon-smelter")
            .expect("smelter")
            .clone();
        let dst = inst.spawn(
            &mut world,
            1,
            TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        inst.behavior.create_state(&mut world, dst);
        let mut reader = WireReader::new(&bytes);
        BuildingCodec::read(&mut world, dst, &mut reader, 3).expect("read");
        let state = world
            .get::<crate::entities::comp::CrafterState>(dst)
            .expect("state");
        assert_eq!(state.progress, 0.5);
        assert_eq!(state.warmup, 0.25);
    }

    #[test]
    fn building_revision_manifests_parse() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("revisions/buildings");
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).expect("revisions/buildings") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read manifest");
            let value: serde_json::Value = serde_json::from_str(&text).expect("parse manifest");
            assert!(value["kind"].is_string(), "{}", path.display());
            assert!(value["revision"].is_u64(), "{}", path.display());
            count += 1;
        }
        assert_eq!(count, 9, "unexpected building revision manifest count");
    }
}
