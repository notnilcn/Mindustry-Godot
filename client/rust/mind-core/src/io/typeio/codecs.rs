// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Direct (non-tagged) `TypeIO` codecs (plan 04 §3.4).
//!
//! Ported from `core/src/mindustry/io/TypeIO.java` (`writeString`, `writeInts`,
//! `writeItemStacks`, `writeStatus`, `writeRules`, `writePlans`,
//! `writePlansQueueNet`, `writeClientPlans`, `writeColor`, ...). Every function
//! mirrors the upstream wire layout exactly; reads are bounds-checked and
//! capped (safe-read semantics).
//!
//! Deferred by owner plan (recorded in plan 04 §8/Changelog):
//! - `writeObjectives`/`writeObjectiveMarker` → M6 (needs `MapObjectives`).
//! - `writeUiBuilder`/`writeMenuResult` → plan 14 payload types.
//! - `writePayload`/`writeMounts`/`writeAbilities`/`writeController` → plans 11/21.

use super::super::wire::{WireReader, WireWriter};
use super::super::{IoError, IoResult};
use super::{
    MAX_PLAYER_PREVIEW_PLANS, MAX_RULES_BYTES, MAX_SYNCED_PLANS, TypeValue, pack_point2,
    read_object_safe, tags, write_object,
};
use crate::content::stacks::{ItemStack, LiquidStack};
use crate::content::{
    BlockId, BulletId, ContentRef, ContentRegistry, ContentType, EffectId, ItemId, LiquidId, Rgba,
    StatusId, UnitCommandId, UnitStanceId, UnitTypeId, WeatherId,
};
use crate::io::json::Rules;

/// `TypeIO.writeString`: existence byte + UTF-8 string.
pub fn write_string(w: &mut WireWriter, value: Option<&str>) -> IoResult<()> {
    match value {
        Some(text) => {
            w.ub(1);
            w.str(text)?;
        }
        None => w.ub(0),
    }
    Ok(())
}

/// `TypeIO.readString`.
pub fn read_string(r: &mut WireReader) -> IoResult<Option<String>> {
    Ok(if r.ub()? != 0 { Some(r.str()?) } else { None })
}

/// `TypeIO.writeString(ByteBuffer, String)`: `i16` byte length, `-1` = null
/// (schematic/logic text framing).
pub fn write_string_data(w: &mut WireWriter, value: Option<&str>) -> IoResult<()> {
    match value {
        Some(text) => {
            let bytes = text.as_bytes();
            if bytes.len() > i16::MAX as usize {
                return Err(IoError::TooLarge {
                    limit: i16::MAX as usize,
                    actual: bytes.len(),
                });
            }
            w.s(bytes.len() as i16);
            w.bytes(bytes);
        }
        None => w.s(-1),
    }
    Ok(())
}

/// `TypeIO.readString(ByteBuffer)`.
pub fn read_string_data(r: &mut WireReader) -> IoResult<Option<String>> {
    let len = r.s()?;
    if len == -1 {
        return Ok(None);
    }
    if len < 0 {
        return Err(IoError::corrupt(format!("invalid string length: {len}")));
    }
    Ok(Some(
        std::str::from_utf8(r.bytes(len as usize)?)?.to_owned(),
    ))
}

/// `TypeIO.writeBytes`: `i16` count + bytes.
pub fn write_bytes(w: &mut WireWriter, bytes: &[u8]) -> IoResult<()> {
    if bytes.len() > i16::MAX as usize {
        return Err(IoError::TooLarge {
            limit: i16::MAX as usize,
            actual: bytes.len(),
        });
    }
    w.s(bytes.len() as i16);
    w.bytes(bytes);
    Ok(())
}

/// `TypeIO.readBytes`.
pub fn read_bytes(r: &mut WireReader) -> IoResult<Vec<u8>> {
    let len = r.s()?;
    if len < 0 {
        return Err(IoError::corrupt(format!(
            "invalid byte array length: {len}"
        )));
    }
    Ok(r.bytes(len as usize)?.to_vec())
}

/// `TypeIO.writeInts`: `i16` count + `i32` values.
pub fn write_ints(w: &mut WireWriter, values: &[i32]) -> IoResult<()> {
    if values.len() > i16::MAX as usize {
        return Err(IoError::TooLarge {
            limit: i16::MAX as usize,
            actual: values.len(),
        });
    }
    w.s(values.len() as i16);
    for value in values {
        w.i(*value);
    }
    Ok(())
}

/// `TypeIO.readInts`.
pub fn read_ints(r: &mut WireReader) -> IoResult<Vec<i32>> {
    let len = r.s()?;
    if len < 0 {
        return Err(IoError::corrupt(format!("invalid int array length: {len}")));
    }
    let mut out = Vec::with_capacity(len as usize);
    for _ in 0..len {
        out.push(r.i()?);
    }
    Ok(out)
}

/// `TypeIO.writeShorts`: `i16` count + `i16` values.
pub fn write_shorts(w: &mut WireWriter, values: &[i16]) -> IoResult<()> {
    if values.len() > i16::MAX as usize {
        return Err(IoError::TooLarge {
            limit: i16::MAX as usize,
            actual: values.len(),
        });
    }
    w.s(values.len() as i16);
    for value in values {
        w.s(*value);
    }
    Ok(())
}

/// `TypeIO.readShorts`.
pub fn read_shorts(r: &mut WireReader) -> IoResult<Vec<i16>> {
    let len = r.s()?;
    if len < 0 {
        return Err(IoError::corrupt(format!(
            "invalid short array length: {len}"
        )));
    }
    let mut out = Vec::with_capacity(len as usize);
    for _ in 0..len {
        out.push(r.s()?);
    }
    Ok(out)
}

/// `TypeIO.writeItem` (`i16` id, `-1` = null).
pub fn write_item(w: &mut WireWriter, item: Option<ItemId>) {
    w.s(item.map_or(-1, |id| id.raw() as i16));
}

/// `TypeIO.readItem`.
pub fn read_item(r: &mut WireReader) -> IoResult<Option<ItemId>> {
    let id = r.s()?;
    Ok((id >= 0).then(|| ItemId::new(id as u16)))
}

/// `TypeIO.writeLiquid` (`i16` id, `-1` = null).
pub fn write_liquid(w: &mut WireWriter, liquid: Option<LiquidId>) {
    w.s(liquid.map_or(-1, |id| id.raw() as i16));
}

/// `TypeIO.readLiquid`.
pub fn read_liquid(r: &mut WireReader) -> IoResult<Option<LiquidId>> {
    let id = r.s()?;
    Ok((id >= 0).then(|| LiquidId::new(id as u16)))
}

/// `TypeIO.writeBlock` (`i16` id, `-1` = null).
pub fn write_block(w: &mut WireWriter, block: Option<BlockId>) {
    w.s(block.map_or(-1, |id| id.raw() as i16));
}

/// `TypeIO.readBlock`.
pub fn read_block(r: &mut WireReader) -> IoResult<Option<BlockId>> {
    let id = r.s()?;
    Ok((id >= 0).then(|| BlockId::new(id as u16)))
}

/// `TypeIO.writeWeather` (`i16` id, `-1` = null).
pub fn write_weather(w: &mut WireWriter, weather: Option<WeatherId>) {
    w.s(weather.map_or(-1, |id| id.raw() as i16));
}

/// `TypeIO.readWeather`.
pub fn read_weather(r: &mut WireReader) -> IoResult<Option<WeatherId>> {
    let id = r.s()?;
    Ok((id >= 0).then(|| WeatherId::new(id as u16)))
}

/// `TypeIO.writeContent`: `u8` type ordinal + `i16` id.
pub fn write_content(w: &mut WireWriter, content: ContentRef) {
    w.ub(content.type_.ordinal() as u8);
    w.s(content.id as i16);
}

/// `TypeIO.readContent`.
pub fn read_content(r: &mut WireReader) -> IoResult<ContentRef> {
    let ordinal = r.ub()? as usize;
    let type_ = ContentType::ALL
        .get(ordinal)
        .copied()
        .ok_or_else(|| IoError::corrupt(format!("invalid content type ordinal: {ordinal}")))?;
    let id = r.s()?;
    if id < 0 {
        return Err(IoError::corrupt(format!("invalid content id: {id}")));
    }
    Ok(ContentRef::new(type_, id as u16))
}

/// `TypeIO.writeItemStacks`: `i16` count + (item, `i32` amount).
pub fn write_item_stacks(w: &mut WireWriter, stacks: &[ItemStack]) -> IoResult<()> {
    if stacks.len() > i16::MAX as usize {
        return Err(IoError::TooLarge {
            limit: i16::MAX as usize,
            actual: stacks.len(),
        });
    }
    w.s(stacks.len() as i16);
    for stack in stacks {
        write_item(w, Some(stack.item));
        w.i(stack.amount);
    }
    Ok(())
}

/// `TypeIO.readItemStacks`.
pub fn read_item_stacks(r: &mut WireReader) -> IoResult<Vec<ItemStack>> {
    let count = r.s()?;
    if count < 0 {
        return Err(IoError::corrupt(format!(
            "invalid item stack count: {count}"
        )));
    }
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        // Missing items default to copper (`JsonIO` Item fallback parity).
        let item = read_item(r)?.unwrap_or(ItemId::COPPER);
        out.push(ItemStack::new(item, r.i()?));
    }
    Ok(out)
}

/// `TypeIO.writeLiquidStacks`: `i16` count + (liquid, `f32` amount).
pub fn write_liquid_stacks(w: &mut WireWriter, stacks: &[LiquidStack]) -> IoResult<()> {
    if stacks.len() > i16::MAX as usize {
        return Err(IoError::TooLarge {
            limit: i16::MAX as usize,
            actual: stacks.len(),
        });
    }
    w.s(stacks.len() as i16);
    for stack in stacks {
        write_liquid(w, Some(stack.liquid));
        w.f(stack.amount);
    }
    Ok(())
}

/// `TypeIO.readLiquidStacks`.
pub fn read_liquid_stacks(r: &mut WireReader) -> IoResult<Vec<LiquidStack>> {
    let count = r.s()?;
    if count < 0 {
        return Err(IoError::corrupt(format!(
            "invalid liquid stack count: {count}"
        )));
    }
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        // Missing liquids default to water (`JsonIO` Liquid fallback parity).
        let liquid = read_liquid(r)?.unwrap_or(LiquidId::WATER);
        out.push(LiquidStack::new(liquid, r.f()?));
    }
    Ok(out)
}

/// `TypeIO.writeTeam` (`u8` id; null → 0).
pub fn write_team(w: &mut WireWriter, team: Option<u8>) {
    w.ub(team.unwrap_or(0));
}

/// `TypeIO.readTeam`.
pub fn read_team(r: &mut WireReader) -> IoResult<u8> {
    r.ub()
}

/// `TypeIO.writeColor`: `i32` RGBA8888.
pub fn write_color(w: &mut WireWriter, color: Rgba) {
    w.i(color.to_rgba8888() as i32);
}

/// `TypeIO.readColor`.
pub fn read_color(r: &mut WireReader) -> IoResult<Rgba> {
    Ok(Rgba::from_rgba8888(r.u()?))
}

/// `TypeIO.writeCommand` (`u8` id; null → 255).
pub fn write_command(w: &mut WireWriter, command: Option<UnitCommandId>) {
    w.ub(command.map_or(255, |id| id.raw() as u8));
}

/// `TypeIO.readCommand`.
pub fn read_command(r: &mut WireReader) -> IoResult<Option<UnitCommandId>> {
    let id = r.ub()?;
    Ok((id != 255).then(|| UnitCommandId::new(u16::from(id))))
}

/// `TypeIO.writeStance` (`u8` id; null → 255).
pub fn write_stance(w: &mut WireWriter, stance: Option<UnitStanceId>) {
    w.ub(stance.map_or(255, |id| id.raw() as u8));
}

/// `TypeIO.readStance`: 255/out-of-range → `stop` (id 0 in the vanilla
/// registry; never null).
pub fn read_stance(r: &mut WireReader, registry: &ContentRegistry) -> IoResult<UnitStanceId> {
    let id = r.ub()?;
    let fallback = UnitStanceId::new(0);
    if id == 255 {
        return Ok(fallback);
    }
    let stance = UnitStanceId::new(u16::from(id));
    Ok(if registry.unit_stance(stance).is_some() {
        stance
    } else {
        fallback
    })
}

/// `TypeIO.writeEffect` (`i16` fx id).
pub fn write_effect(w: &mut WireWriter, effect: EffectId) {
    w.s(effect.raw() as i16);
}

/// `TypeIO.readEffect`.
pub fn read_effect(r: &mut WireReader) -> IoResult<EffectId> {
    Ok(EffectId(r.us()?))
}

/// `TypeIO.writeUnitType` (`i16` content id).
pub fn write_unit_type(w: &mut WireWriter, unit_type: UnitTypeId) {
    w.s(unit_type.raw() as i16);
}

/// `TypeIO.readUnitType`.
pub fn read_unit_type(r: &mut WireReader) -> IoResult<UnitTypeId> {
    Ok(UnitTypeId::new(r.us()?))
}

/// `TypeIO.writeBulletType` (`i16` content id).
pub fn write_bullet_type(w: &mut WireWriter, bullet: BulletId) {
    w.s(bullet.raw() as i16);
}

/// `TypeIO.readBulletType`.
pub fn read_bullet_type(r: &mut WireReader) -> IoResult<BulletId> {
    Ok(BulletId::new(r.us()?))
}

/// `TypeIO.writeVec2` (null → `0,0`).
pub fn write_vec2(w: &mut WireWriter, vec: Option<(f32, f32)>) {
    let (x, y) = vec.unwrap_or((0.0, 0.0));
    w.f(x);
    w.f(y);
}

/// `TypeIO.readVec2`.
pub fn read_vec2(r: &mut WireReader) -> IoResult<(f32, f32)> {
    Ok((r.f()?, r.f()?))
}

/// `TypeIO.writeVecNullable` (null → `NaN,NaN`).
pub fn write_vec_nullable(w: &mut WireWriter, vec: Option<(f32, f32)>) {
    let (x, y) = vec.unwrap_or((f32::NAN, f32::NAN));
    w.f(x);
    w.f(y);
}

/// `TypeIO.readVecNullable`.
pub fn read_vec_nullable(r: &mut WireReader) -> IoResult<Option<(f32, f32)>> {
    let (x, y) = (r.f()?, r.f()?);
    Ok(if x.is_nan() || y.is_nan() {
        None
    } else {
        Some((x, y))
    })
}

/// One status application (`mindustry.entities.units.StatusEntry`).
///
/// This is the wire shape; plans 10/11 own the runtime component. Only
/// `dynamic` effects serialize their multiplier fields (upstream bitmask).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusEntry {
    /// Status effect content id.
    pub effect: StatusId,
    /// Remaining time in ticks.
    pub time: f32,
    /// Interval-damage accumulator (not serialized).
    pub damage_time: f32,
    /// Dynamic multiplier (serialized only when set ≠ 1).
    pub damage_multiplier: f32,
    /// Dynamic multiplier.
    pub health_multiplier: f32,
    /// Dynamic multiplier.
    pub speed_multiplier: f32,
    /// Dynamic multiplier.
    pub reload_multiplier: f32,
    /// Dynamic multiplier.
    pub build_speed_multiplier: f32,
    /// Dynamic multiplier.
    pub drag_multiplier: f32,
    /// Dynamic armor override (serialized when `>= 0`).
    pub armor_override: f32,
}

impl StatusEntry {
    /// `StatusEntry.set(effect, time)` with upstream dynamic-field defaults.
    pub fn new(effect: StatusId, time: f32) -> Self {
        Self {
            effect,
            time,
            damage_time: 0.0,
            damage_multiplier: 1.0,
            health_multiplier: 1.0,
            speed_multiplier: 1.0,
            reload_multiplier: 1.0,
            build_speed_multiplier: 1.0,
            drag_multiplier: 1.0,
            armor_override: -1.0,
        }
    }
}

/// `TypeIO.writeStatus`: effect id, time, then the dynamic-field bitmask when
/// the effect is `dynamic`.
pub fn write_status(
    w: &mut WireWriter,
    registry: &ContentRegistry,
    entry: &StatusEntry,
) -> IoResult<()> {
    w.s(entry.effect.raw() as i16);
    w.f(entry.time);
    let dynamic = registry
        .status(entry.effect)
        .map(|effect| effect.dynamic)
        .unwrap_or(false);
    if dynamic {
        let mut mask = 0u8;
        if entry.damage_multiplier != 1.0 {
            mask |= 1 << 0;
        }
        if entry.health_multiplier != 1.0 {
            mask |= 1 << 1;
        }
        if entry.speed_multiplier != 1.0 {
            mask |= 1 << 2;
        }
        if entry.reload_multiplier != 1.0 {
            mask |= 1 << 3;
        }
        if entry.build_speed_multiplier != 1.0 {
            mask |= 1 << 4;
        }
        if entry.drag_multiplier != 1.0 {
            mask |= 1 << 5;
        }
        if entry.armor_override >= 0.0 {
            mask |= 1 << 6;
        }
        w.ub(mask);
        if mask & (1 << 0) != 0 {
            w.f(entry.damage_multiplier);
        }
        if mask & (1 << 1) != 0 {
            w.f(entry.health_multiplier);
        }
        if mask & (1 << 2) != 0 {
            w.f(entry.speed_multiplier);
        }
        if mask & (1 << 3) != 0 {
            w.f(entry.reload_multiplier);
        }
        if mask & (1 << 4) != 0 {
            w.f(entry.build_speed_multiplier);
        }
        if mask & (1 << 5) != 0 {
            w.f(entry.drag_multiplier);
        }
        if mask & (1 << 6) != 0 {
            w.f(entry.armor_override);
        }
    }
    Ok(())
}

/// `TypeIO.readStatus`.
pub fn read_status(r: &mut WireReader, registry: &ContentRegistry) -> IoResult<StatusEntry> {
    let id = r.s()?;
    if id < 0 {
        return Err(IoError::corrupt(format!("invalid status id: {id}")));
    }
    let time = r.f()?;
    let mut entry = StatusEntry::new(StatusId::new(id as u16), time);
    let dynamic = registry
        .status(entry.effect)
        .map(|effect| effect.dynamic)
        .unwrap_or(false);
    if dynamic {
        let flags = r.ub()?;
        if flags & (1 << 0) != 0 {
            entry.damage_multiplier = r.f()?;
        }
        if flags & (1 << 1) != 0 {
            entry.health_multiplier = r.f()?;
        }
        if flags & (1 << 2) != 0 {
            entry.speed_multiplier = r.f()?;
        }
        if flags & (1 << 3) != 0 {
            entry.reload_multiplier = r.f()?;
        }
        if flags & (1 << 4) != 0 {
            entry.build_speed_multiplier = r.f()?;
        }
        if flags & (1 << 5) != 0 {
            entry.drag_multiplier = r.f()?;
        }
        if flags & (1 << 6) != 0 {
            entry.armor_override = r.f()?;
        }
    }
    Ok(entry)
}

/// `TypeIO.writeRules`: length-prefixed JSON bytes.
pub fn write_rules(w: &mut WireWriter, rules: &Rules) -> IoResult<()> {
    let bytes = serde_json::to_vec(rules)?;
    w.i(bytes.len() as i32);
    w.bytes(&bytes);
    Ok(())
}

/// `TypeIO.readRules` ([`MAX_RULES_BYTES`] cap; `Rules too long`).
pub fn read_rules(r: &mut WireReader) -> IoResult<Rules> {
    let len = r.i()?;
    if len < 0 || len as usize > MAX_RULES_BYTES {
        return Err(IoError::corrupt("Rules too long"));
    }
    Ok(serde_json::from_slice(r.bytes(len as usize)?)?)
}

/// One build plan (`mindustry.entities.units.BuildPlan`).
///
/// Wire shape only; plan 07 owns the runtime type (reconstruction/removal
/// queues, progress). Configs are [`TypeValue`]s (schematic parity ABI).
#[derive(Debug, Clone, PartialEq)]
pub struct BuildPlan {
    /// Tile x.
    pub x: i32,
    /// Tile y.
    pub y: i32,
    /// Build rotation.
    pub rotation: i32,
    /// Block to place (meaningless when `breaking`).
    pub block: BlockId,
    /// Configuration object (`null` when absent).
    pub config: TypeValue,
    /// Whether this plan removes the block.
    pub breaking: bool,
}

impl BuildPlan {
    /// A place plan (`BuildPlan(x, y, rotation, block, config)`).
    pub fn place(x: i32, y: i32, rotation: i32, block: BlockId, config: TypeValue) -> Self {
        Self {
            x,
            y,
            rotation,
            block,
            config,
            breaking: false,
        }
    }

    /// A break plan (`BuildPlan(x, y)`).
    pub fn breaking(x: i32, y: i32) -> Self {
        Self {
            x,
            y,
            rotation: 0,
            block: BlockId::AIR,
            config: TypeValue::Null,
            breaking: true,
        }
    }
}

/// `TypeIO.writePlan`: breaking flag, packed pos, block/rotation/config for
/// place plans.
pub fn write_plan(w: &mut WireWriter, plan: &BuildPlan) -> IoResult<()> {
    w.ub(u8::from(plan.breaking));
    w.i(pack_point2(plan.x, plan.y));
    if !plan.breaking {
        w.s(plan.block.raw() as i16);
        w.ub(plan.rotation as u8);
        w.ub(1); // always has config
        write_object(w, &plan.config)?;
    }
    Ok(())
}

/// `TypeIO.readPlan`. Upstream also drops plans on missing tiles
/// (`world.tile(position) == null`); the world check is the consumer's job
/// (plan 07), the codec is pure.
pub fn read_plan(r: &mut WireReader) -> IoResult<BuildPlan> {
    let breaking = r.ub()? == 1;
    let position = r.i()?;
    let x = super::unpack_point2_x(position);
    let y = super::unpack_point2_y(position);
    if breaking {
        return Ok(BuildPlan::breaking(x, y));
    }
    let block = BlockId::new(r.us()?);
    let rotation = i32::from(r.ub()?);
    let has_config = r.ub()? == 1;
    let config = read_object_safe(r)?;
    Ok(BuildPlan {
        x,
        y,
        rotation,
        block,
        config: if has_config { config } else { TypeValue::Null },
        breaking,
    })
}

/// `TypeIO.writePlans`: `i16` count, `-1` = null.
pub fn write_plans(w: &mut WireWriter, plans: Option<&[BuildPlan]>) -> IoResult<()> {
    let Some(plans) = plans else {
        w.s(-1);
        return Ok(());
    };
    if plans.len() > i16::MAX as usize {
        return Err(IoError::TooLarge {
            limit: i16::MAX as usize,
            actual: plans.len(),
        });
    }
    w.s(plans.len() as i16);
    for plan in plans {
        write_plan(w, plan)?;
    }
    Ok(())
}

/// `TypeIO.readPlans`.
pub fn read_plans(r: &mut WireReader) -> IoResult<Option<Vec<BuildPlan>>> {
    let count = r.s()?;
    if count == -1 {
        return Ok(None);
    }
    if count < 0 {
        return Err(IoError::corrupt(format!("invalid plan count: {count}")));
    }
    let mut out = Vec::with_capacity(count as usize);
    for _ in 0..count {
        out.push(read_plan(r)?);
    }
    Ok(Some(out))
}

/// `TypeIO.getMaxPlans`: caps a network queue at [`MAX_SYNCED_PLANS`] and a
/// 500-byte total config budget.
pub fn get_max_plans(plans: &[BuildPlan]) -> usize {
    let mut used = plans.len().min(MAX_SYNCED_PLANS);
    let mut total_length = 0usize;
    for (i, plan) in plans.iter().enumerate().take(used) {
        match &plan.config {
            TypeValue::ByteArray(bytes) => total_length += bytes.len(),
            TypeValue::Str(Some(text)) => total_length += text.len(),
            _ => {}
        }
        if total_length > 500 {
            used = i + 1;
            break;
        }
    }
    used
}

/// `TypeIO.writePlansQueueNet`: `i32` count, `-1` = null.
pub fn write_plans_queue_net(w: &mut WireWriter, plans: Option<&[BuildPlan]>) -> IoResult<()> {
    let Some(plans) = plans else {
        w.i(-1);
        return Ok(());
    };
    let used = get_max_plans(plans);
    w.i(used as i32);
    for plan in &plans[..used] {
        write_plan(w, plan)?;
    }
    Ok(())
}

/// `TypeIO.readPlansQueueNet` ([`MAX_SYNCED_PLANS`] cap; `Queue too long: N`).
pub fn read_plans_queue_net(r: &mut WireReader) -> IoResult<Option<Vec<BuildPlan>>> {
    let used = r.i()?;
    if used == -1 {
        return Ok(None);
    }
    if used < 0 || used as usize > MAX_SYNCED_PLANS {
        return Err(IoError::corrupt(format!("Queue too long: {used}")));
    }
    let mut out = Vec::with_capacity(used as usize);
    for _ in 0..used {
        out.push(read_plan(r)?);
    }
    Ok(Some(out))
}

/// `TypeIO.readPlansQueue` (save-side: no size cap).
pub fn read_plans_queue(r: &mut WireReader) -> IoResult<Option<Vec<BuildPlan>>> {
    let used = r.i()?;
    if used == -1 {
        return Ok(None);
    }
    if used < 0 {
        return Err(IoError::corrupt(format!("invalid plan queue size: {used}")));
    }
    let mut out = Vec::with_capacity(used as usize);
    for _ in 0..used {
        out.push(read_plan(r)?);
    }
    Ok(Some(out))
}

/// `TypeIO.writeClientPlans`: preview-only plan list; configs are whitelisted
/// (`validClientPlanConfig`: null/Number/Boolean/Content) or written as null.
///
/// `rotate` reports whether a block uses rotation (`Block.rotate`); plan 07
/// owns that field, so the predicate is injected until it lands (plan 04 §8).
pub fn write_client_plans(
    w: &mut WireWriter,
    plans: Option<&[BuildPlan]>,
    rotate: &dyn Fn(BlockId) -> bool,
) -> IoResult<()> {
    let Some(plans) = plans else {
        w.s(0);
        return Ok(());
    };
    if plans.len() > i16::MAX as usize {
        return Err(IoError::TooLarge {
            limit: i16::MAX as usize,
            actual: plans.len(),
        });
    }
    w.s(plans.len() as i16);
    for plan in plans {
        if plan.breaking {
            return Err(IoError::corrupt("Breaking plans should not be sent."));
        }
        w.i(pack_point2(plan.x, plan.y));
        w.s(plan.block.raw() as i16);
        if rotate(plan.block) {
            w.ub(plan.rotation as u8);
        }
        let config = if valid_client_plan_config(&plan.config) {
            &plan.config
        } else {
            &TypeValue::Null
        };
        write_object(w, config)?;
    }
    Ok(())
}

/// `TypeIO.validClientPlanConfig`: links/complex configs are not sent.
pub fn valid_client_plan_config(config: &TypeValue) -> bool {
    matches!(
        config,
        TypeValue::Null
            | TypeValue::Int(_)
            | TypeValue::Long(_)
            | TypeValue::Float(_)
            | TypeValue::Double(_)
            | TypeValue::Bool(_)
            | TypeValue::Content(..)
    )
}

/// `TypeIO.readClientPlans` ([`MAX_PLAYER_PREVIEW_PLANS`] cap).
///
/// Wire quirk kept: positions are written as one packed `i32` and read back as
/// two unsigned shorts (valid for non-negative coords).
pub fn read_client_plans(
    r: &mut WireReader,
    rotate: &dyn Fn(BlockId) -> bool,
) -> IoResult<Option<Vec<BuildPlan>>> {
    let amount = r.s()?;
    if amount == 0 {
        return Ok(None);
    }
    if amount < 0 || amount as usize > MAX_PLAYER_PREVIEW_PLANS {
        return Err(IoError::corrupt(format!("Too many plans: {amount}")));
    }
    let mut out = Vec::with_capacity(amount as usize);
    for _ in 0..amount {
        let x = i32::from(r.us()?);
        let y = i32::from(r.us()?);
        let block = BlockId::new(r.us()?);
        let rotation = if rotate(block) { i32::from(r.ub()?) } else { 0 };
        let config = read_client_plan_config(r)?;
        out.push(BuildPlan::place(x, y, rotation, block, config));
    }
    Ok(Some(out))
}

/// `TypeIO.readClientPlanConfig`: whitelisted tag subset.
pub fn read_client_plan_config(r: &mut WireReader) -> IoResult<TypeValue> {
    let tag = r.ub()?;
    match tag {
        tags::NULL => Ok(TypeValue::Null),
        tags::INTEGER => Ok(TypeValue::Int(r.i()?)),
        tags::LONG => Ok(TypeValue::Long(r.l()?)),
        tags::FLOAT => Ok(TypeValue::Float(r.f()?)),
        tags::CONTENT => {
            let ordinal = r.ub()? as usize;
            let type_ = ContentType::ALL.get(ordinal).copied().ok_or_else(|| {
                IoError::corrupt(format!("invalid content type ordinal: {ordinal}"))
            })?;
            let id = r.s()?;
            if id < 0 {
                return Err(IoError::corrupt(format!("invalid content id: {id}")));
            }
            Ok(TypeValue::Content(type_, id as u16))
        }
        tags::BOOLEAN => Ok(TypeValue::Bool(r.bool()?)),
        tags::DOUBLE => Ok(TypeValue::Double(r.d()?)),
        other => Err(IoError::corrupt(format!(
            "Unknown plan config object type: {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    fn buf_with(f: impl FnOnce(&mut WireWriter)) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut w = WireWriter::new(&mut buf);
        f(&mut w);
        buf
    }

    /// Ported from `ApplicationTests.writeStringTest` (null, ASCII, CJK, emoji).
    #[test]
    fn write_string_roundtrip() {
        let cases: Vec<Option<String>> = vec![
            None,
            Some(String::new()),
            Some("hello world".to_owned()),
            Some("简体中文测试".to_owned()),
            Some("🚀🗺️✨ emoji string".to_owned()),
            Some("a".repeat(3000)),
        ];
        for case in cases {
            let buf = buf_with(|w| write_string(w, case.as_deref()).unwrap());
            let mut r = WireReader::new(&buf);
            assert_eq!(read_string(&mut r).unwrap(), case);
            assert_eq!(r.remaining(), 0);
        }

        // `writeStringData` framing (-1 = null) round-trips too.
        let buf = buf_with(|w| {
            write_string_data(w, None).unwrap();
            write_string_data(w, Some("abc")).unwrap();
        });
        let mut r = WireReader::new(&buf);
        assert_eq!(read_string_data(&mut r).unwrap(), None);
        assert_eq!(read_string_data(&mut r).unwrap(), Some("abc".to_owned()));
    }

    /// Ported from `ApplicationTests.writeRules` (TypeIO binary rules).
    #[test]
    fn write_rules_binary() {
        let rules = Rules {
            attack_mode: true,
            build_speed_multiplier: 22.2,
            ..Rules::default()
        };
        let buf = buf_with(|w| write_rules(w, &rules).unwrap());
        let mut r = WireReader::new(&buf);
        let out = read_rules(&mut r).unwrap();
        assert_eq!(out.attack_mode, rules.attack_mode);
        assert_eq!(out.build_speed_multiplier, rules.build_speed_multiplier);

        // Oversized rules blob is rejected ("Rules too long").
        let buf = buf_with(|w| w.i(MAX_RULES_BYTES as i32 + 1));
        let mut r = WireReader::new(&buf);
        assert!(
            read_rules(&mut r)
                .unwrap_err()
                .to_string()
                .contains("Rules too long")
        );
    }

    #[test]
    fn primitive_codec_roundtrips() {
        let registry = test_registry();
        let buf = buf_with(|w| {
            write_ints(w, &[3, -1, 7]).unwrap();
            write_shorts(w, &[-2, 5]).unwrap();
            write_bytes(w, &[9, 9]).unwrap();
            write_item(w, Some(ItemId::COPPER));
            write_liquid(w, None);
            write_block(w, Some(BlockId::STONE_WALL));
            write_content(w, ContentRef::new(ContentType::Block, 5));
            write_item_stacks(w, &[ItemStack::new(ItemId::COPPER, 50)]).unwrap();
            write_liquid_stacks(w, &[LiquidStack::new(LiquidId::WATER, 1.5)]).unwrap();
            write_team(w, Some(2));
            write_color(w, Rgba::new(1.0, 0.5, 0.25, 1.0));
            write_command(w, Some(UnitCommandId::new(1)));
            write_stance(w, Some(UnitStanceId::new(1)));
            write_vec2(w, Some((1.5, -2.5)));
            write_vec_nullable(w, None);
            write_effect(w, EffectId(42));
            write_unit_type(w, UnitTypeId::new(9));
            write_bullet_type(w, BulletId::new(3));
        });
        let mut r = WireReader::new(&buf);
        assert_eq!(read_ints(&mut r).unwrap(), vec![3, -1, 7]);
        assert_eq!(read_shorts(&mut r).unwrap(), vec![-2, 5]);
        assert_eq!(read_bytes(&mut r).unwrap(), vec![9, 9]);
        assert_eq!(read_item(&mut r).unwrap(), Some(ItemId::COPPER));
        assert_eq!(read_liquid(&mut r).unwrap(), None);
        assert_eq!(read_block(&mut r).unwrap(), Some(BlockId::STONE_WALL));
        assert_eq!(
            read_content(&mut r).unwrap(),
            ContentRef::new(ContentType::Block, 5)
        );
        assert_eq!(
            read_item_stacks(&mut r).unwrap(),
            vec![ItemStack::new(ItemId::COPPER, 50)]
        );
        assert_eq!(
            read_liquid_stacks(&mut r).unwrap(),
            vec![LiquidStack::new(LiquidId::WATER, 1.5)]
        );
        assert_eq!(read_team(&mut r).unwrap(), 2);
        let color = read_color(&mut r).unwrap();
        assert!((color.g - 0.5).abs() < 0.01);
        assert_eq!(read_command(&mut r).unwrap(), Some(UnitCommandId::new(1)));
        assert_eq!(
            read_stance(&mut r, &registry).unwrap(),
            UnitStanceId::new(1)
        );
        assert_eq!(read_vec2(&mut r).unwrap(), (1.5, -2.5));
        assert_eq!(read_vec_nullable(&mut r).unwrap(), None);
        assert_eq!(read_effect(&mut r).unwrap(), EffectId(42));
        assert_eq!(read_unit_type(&mut r).unwrap(), UnitTypeId::new(9));
        assert_eq!(read_bullet_type(&mut r).unwrap(), BulletId::new(3));
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn status_dynamic_bitmask_roundtrip() {
        let registry = test_registry();
        // Find the vanilla `dynamic` effect (id by name `dynamic`).
        let dynamic_id = registry.status_id("dynamic").unwrap();
        let mut entry = StatusEntry::new(dynamic_id, 60.0);
        entry.speed_multiplier = 2.0;
        entry.armor_override = 3.0;

        let buf = buf_with(|w| write_status(w, &registry, &entry).unwrap());
        let mut r = WireReader::new(&buf);
        let out = read_status(&mut r, &registry).unwrap();
        assert_eq!(out.effect, dynamic_id);
        assert_eq!(out.time, 60.0);
        assert_eq!(out.speed_multiplier, 2.0);
        assert_eq!(out.armor_override, 3.0);
        // Untouched dynamic fields stay at defaults.
        assert_eq!(out.damage_multiplier, 1.0);

        // Non-dynamic effect: only id+time on the wire.
        let burning = registry.status_id("burning").unwrap();
        let plain = StatusEntry::new(burning, 10.0);
        let buf = buf_with(|w| write_status(w, &registry, &plain).unwrap());
        assert_eq!(buf.len(), 2 + 4);
        let mut r = WireReader::new(&buf);
        let out = read_status(&mut r, &registry).unwrap();
        assert_eq!(out.effect, burning);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn plan_codecs_roundtrip_and_caps() {
        let registry = test_registry();
        let conveyor = registry.block_id("conveyor").unwrap();
        let plans = vec![
            BuildPlan::place(3, 4, 1, conveyor, TypeValue::Int(7)),
            BuildPlan::breaking(9, 10),
        ];
        let buf = buf_with(|w| write_plans(w, Some(&plans)).unwrap());
        let mut r = WireReader::new(&buf);
        assert_eq!(read_plans(&mut r).unwrap(), Some(plans.clone()));

        // Null plans.
        let buf = buf_with(|w| write_plans(w, None).unwrap());
        let mut r = WireReader::new(&buf);
        assert_eq!(read_plans(&mut r).unwrap(), None);

        // Queue net cap: 25 plans → 20 written; >20 on read is an error.
        let many: Vec<BuildPlan> = (0..25)
            .map(|i| BuildPlan::place(i, 0, 0, conveyor, TypeValue::Null))
            .collect();
        let buf = buf_with(|w| write_plans_queue_net(w, Some(&many)).unwrap());
        let mut r = WireReader::new(&buf);
        let read = read_plans_queue_net(&mut r).unwrap().unwrap();
        assert_eq!(read.len(), 20);

        let buf = buf_with(|w| w.i(21));
        let mut r = WireReader::new(&buf);
        assert!(
            read_plans_queue_net(&mut r)
                .unwrap_err()
                .to_string()
                .contains("Queue too long: 21")
        );

        // Client plans: configs are whitelisted; a string config is nulled.
        let mut with_config =
            BuildPlan::place(1, 2, 0, conveyor, TypeValue::Str(Some("x".repeat(600))));
        let buf =
            buf_with(|w| write_client_plans(w, Some(&[with_config.clone()]), &|_| true).unwrap());
        let mut r = WireReader::new(&buf);
        let read = read_client_plans(&mut r, &|_| true).unwrap().unwrap();
        assert_eq!(read[0].config, TypeValue::Null);
        with_config.config = TypeValue::Null;
        assert_eq!(read[0], with_config);

        // Client plan config tag whitelist rejects e.g. int_seq.
        let buf = buf_with(|w| {
            w.ub(tags::INT_SEQ);
            w.s(0);
        });
        let mut r = WireReader::new(&buf);
        assert!(
            read_client_plan_config(&mut r)
                .unwrap_err()
                .to_string()
                .contains("Unknown plan config object type: 6")
        );
    }
}
