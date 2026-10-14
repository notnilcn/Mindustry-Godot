// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LAccess` and the logic capability trait definitions.
//!
//! Ported from `core/src/mindustry/logic/{LAccess,Senseable,Settable,Controllable,
//! Ranged,LReadable,LWritable,LPrintable,LDrawable}.java`. Definitions live here;
//! the concrete `sense`/`control`/`setProp` implementations land with the owning
//! plans (07/08/10/11/12) behind these traits.

use bevy_ecs::entity::Entity;

use crate::content::ContentRef;
use crate::logic::enums::{FieldKind, MlogField, logic_enum};
use crate::logic::value::LogicObject;

logic_enum!(
    /// `LAccess` — sensor/settable/controllable property.
    ///
    /// **Ordinals are ABI**: append only, never reorder.
    LAccess {
        TotalItems => "totalItems",
        FirstItem => "firstItem",
        TotalLiquids => "totalLiquids",
        TotalPower => "totalPower",
        ItemCapacity => "itemCapacity",
        LiquidCapacity => "liquidCapacity",
        PowerCapacity => "powerCapacity",
        PowerNetStored => "powerNetStored",
        PowerNetCapacity => "powerNetCapacity",
        PowerNetIn => "powerNetIn",
        PowerNetOut => "powerNetOut",
        Ammo => "ammo",
        AmmoCapacity => "ammoCapacity",
        CurrentAmmoType => "currentAmmoType",
        MemoryCapacity => "memoryCapacity",
        Health => "health",
        MaxHealth => "maxHealth",
        Heat => "heat",
        Shield => "shield",
        Armor => "armor",
        Efficiency => "efficiency",
        Progress => "progress",
        Timescale => "timescale",
        Rotation => "rotation",
        X => "x",
        Y => "y",
        VelocityX => "velocityX",
        VelocityY => "velocityY",
        ShootX => "shootX",
        ShootY => "shootY",
        CameraX => "cameraX",
        CameraY => "cameraY",
        CameraWidth => "cameraWidth",
        CameraHeight => "cameraHeight",
        DisplayWidth => "displayWidth",
        DisplayHeight => "displayHeight",
        BufferSize => "bufferSize",
        Operations => "operations",
        Size => "size",
        Solid => "solid",
        Dead => "dead",
        Range => "range",
        Shooting => "shooting",
        Boosting => "boosting",
        MineX => "mineX",
        MineY => "mineY",
        Mining => "mining",
        BuildX => "buildX",
        BuildY => "buildY",
        PingX => "pingX",
        PingY => "pingY",
        PingText => "pingText",
        Building => "building",
        Breaking => "breaking",
        Speed => "speed",
        Team => "team",
        Type => "type",
        Flag => "flag",
        Flying => "flying",
        Controlled => "controlled",
        Controller => "controller",
        Name => "name",
        PayloadCount => "payloadCount",
        PayloadType => "payloadType",
        TotalPayload => "totalPayload",
        PayloadCapacity => "payloadCapacity",
        MaxUnits => "maxUnits",
        Id => "id",
        SelectedBlock => "selectedBlock",
        SelectedRotation => "selectedRotation",
        BulletLifetime => "bulletLifetime",
        BulletTime => "bulletTime",
        Enabled => "enabled",
        Shoot => "shoot",
        Shootp => "shootp",
        Config => "config",
        Color => "color"
    }
);

impl LAccess {
    /// Parameter names (`LAccess.params`).
    pub const fn params(self) -> &'static [&'static str] {
        match self {
            LAccess::Enabled | LAccess::Config | LAccess::Color => &["to"],
            LAccess::Shoot => &["x", "y", "shoot"],
            LAccess::Shootp => &["unit", "shoot"],
            _ => &[],
        }
    }

    /// Object-valued accessor (`LAccess.isObj`).
    pub const fn is_obj(self) -> bool {
        matches!(self, LAccess::Shootp | LAccess::Config)
    }

    /// Privileged accessor (`LAccess.privilegedAccess`).
    pub const fn privileged(self) -> bool {
        matches!(
            self,
            LAccess::CameraX | LAccess::CameraY | LAccess::CameraWidth | LAccess::CameraHeight
        )
    }

    /// `LAccess.senseable`.
    pub fn senseable() -> Vec<LAccess> {
        LAccess::ALL
            .iter()
            .copied()
            .filter(|a| a.params().len() <= 1 && !a.privileged())
            .collect()
    }

    /// `LAccess.senseablePrivileged`.
    pub fn senseable_privileged() -> Vec<LAccess> {
        LAccess::ALL
            .iter()
            .copied()
            .filter(|a| a.params().len() <= 1)
            .collect()
    }

    /// `LAccess.controls`.
    pub fn controls() -> Vec<LAccess> {
        LAccess::ALL
            .iter()
            .copied()
            .filter(|a| !a.params().is_empty())
            .collect()
    }

    /// `LAccess.settable`.
    pub const SETTABLE: &'static [LAccess] = &[
        LAccess::X,
        LAccess::Y,
        LAccess::VelocityX,
        LAccess::VelocityY,
        LAccess::Rotation,
        LAccess::Speed,
        LAccess::Armor,
        LAccess::Health,
        LAccess::Shield,
        LAccess::Team,
        LAccess::Flag,
        LAccess::TotalPower,
        LAccess::PayloadType,
        LAccess::BulletTime,
        LAccess::BulletLifetime,
    ];
}

/// `Senseable`: numeric and object sensors plus content lookup.
pub trait LogicSense {
    /// Numeric `sense` (`Senseable.sense`).
    fn sense(&self, e: Entity, a: LAccess) -> f64;
    /// Object `senseObject` (`Senseable.senseObject`).
    fn sense_object(&self, e: Entity, a: LAccess) -> Option<LogicObject>;
    /// Content `sense` (`Senseable.sense(Content)`).
    fn sense_content(&self, e: Entity, c: ContentRef) -> f64;
}

/// `Settable`: `setProp` dispatch.
pub trait LogicSettle {
    /// Numeric assignment.
    fn set_prop_num(&mut self, e: Entity, a: LAccess, v: f64);
    /// Object assignment.
    fn set_prop_obj(&mut self, e: Entity, a: LAccess, v: LogicObject);
    /// Content-count assignment.
    fn set_prop_content(&mut self, e: Entity, c: ContentRef, v: f64);
}

/// `Controllable`: `control` dispatch.
pub trait LogicControl {
    /// Numeric control.
    fn control_num(&mut self, e: Entity, a: LAccess, p1: f64, p2: f64, p3: f64, p4: f64);
    /// Object control.
    fn control_obj(&mut self, e: Entity, a: LAccess, p1: LogicObject, p2: f64, p3: f64, p4: f64);
    /// Owning team of the target.
    fn logic_team(&self, e: Entity) -> u8;
}

/// `Ranged`: radar/locate range source.
pub trait LogicRanged {
    /// Range in world units (`Ranged.range`).
    fn range(&self, e: Entity) -> f32;
}

/// `LReadable`: a logic-readable target (memory, processor variables).
pub trait LogicRead {
    /// Reads `address` from the target.
    fn logic_read(&mut self, e: Entity, address: &super::value::LVar) -> super::value::LVar;
}

/// `LWritable`: a logic-writable target.
pub trait LogicWrite {
    /// Writes `value` at `address`.
    fn logic_write(&mut self, e: Entity, address: &super::value::LVar, value: &super::value::LVar);
}

/// `LPrintable`: a target that accepts printed text.
pub trait LogicPrint {
    /// Prints `value` to the target.
    fn logic_print(&mut self, e: Entity, value: &str);
}

/// `LDrawable`: a target that accepts packed draw commands.
pub trait LogicDraw {
    /// Appends the packed `DisplayCmd` buffer and advances `operations`.
    fn logic_draw(&mut self, e: Entity, buffer: &[u64]);
}

/// Result of a `sensor` access (`Senseable.sense`/`senseObject`).
#[derive(Clone, Debug, PartialEq)]
pub enum Sensed {
    /// Numeric sense result.
    Num(f64),
    /// Object sense result (`None` == Java `null`; upstream `noSensed` falls
    /// through to [`Sensed::Num`] before reaching this variant).
    Obj(Option<LogicObject>),
}

use bevy_ecs::world::World;

use crate::content::{BlockId, ContentType, ItemId, LiquidId, UnitTypeId};
use crate::entities::comp::{Building, Health, Pos, TeamComp, Unit};
use crate::logic::blocks::{
    LogicBlockState, build_at, display_def_of, logic_def_of, memory_def_of,
};
use crate::logic::value::LVar;
use crate::world::block::BlockTable;
use crate::world::config::{ConfigKind, ConfigValue};
use crate::world::modules::{ItemModule, LiquidModule, PowerModule};

/// Team of an entity (`team.id`).
pub fn entity_team(world: &World, e: Entity) -> Option<u8> {
    world.get::<TeamComp>(e).map(|t| t.team)
}

/// `PrintI.toString` world-resolved names: contents (`LogicContentInfo`),
/// building block names (`BlockTable`) and unit type names.
///
/// Java resolves these in one `instanceof` chain (`LExecutor.PrintI.toString`);
/// the VM calls this for the object kinds it cannot resolve from the value
/// alone. `None` lets the caller fall back (`Str`/`Enum`/`[object]`).
pub fn name_of(world: &World, obj: &LogicObject) -> Option<String> {
    match obj {
        LogicObject::Content(c) => world
            .get_resource::<crate::logic::globals::LogicContentInfo>()
            .and_then(|info| info.name(c.type_, c.id))
            .map(str::to_owned),
        LogicObject::Building(e) => block_def(world, *e).map(|def| def.name.clone()),
        LogicObject::Unit(e) => world
            .get::<crate::entities::comp::unit::UnitTypeComp>(*e)
            .and_then(|t| {
                world
                    .get_resource::<crate::logic::globals::LogicContentInfo>()
                    .and_then(|info| info.name(ContentType::Unit, t.type_id.raw()))
            })
            .map(str::to_owned),
        LogicObject::Team(id) => Some(crate::game::team::Team::get(*id).name.clone()),
        _ => None,
    }
}

/// Block definition for a building entity.
fn block_def(world: &World, e: Entity) -> Option<std::sync::Arc<crate::content::BlockDef>> {
    let block = world.get::<Building>(e)?.block;
    world
        .get_resource::<BlockTable>()?
        .get(block)
        .map(|i| i.def.clone())
}

/// `SensorI` dispatch for a numeric or object accessor.
///
/// Ported from `BuildingComp.sense`/`senseObject` and `UnitComp.sense`/
/// `senseObject` (`entities/comp/{BuildingComp,UnitComp}.java`). Returns
/// [`Sensed::Num`] for unknown accessors, matching upstream `sense`'s `default`
/// branch (objects use [`Sensed::Obj(None)`]).
pub fn sense(world: &World, target: &LogicObject, a: LAccess) -> Sensed {
    // Content values (items/liquids/payload) use the content sense path.
    match target {
        LogicObject::Building(e) => sense_building(world, *e, a),
        LogicObject::Unit(e) => sense_unit(world, *e, a),
        LogicObject::Content(c) => match a {
            // `Block/Item/Liquid/UnitType.sense(@id)` = `getLogicId()`.
            LAccess::Id => Sensed::Num(
                world
                    .get_resource::<crate::logic::globals::GlobalVars>()
                    .map(|globals| globals.lookup_logic_id(*c) as f64)
                    .unwrap_or(-1.0),
            ),
            _ => Sensed::Num(1.0),
        },
        _ => Sensed::Obj(None),
    }
}

/// Content-count sense (`Senseable.sense(Content)`).
pub fn sense_content(world: &World, target: &LogicObject, c: crate::content::ContentRef) -> f64 {
    let LogicObject::Building(e) = target else {
        if let LogicObject::Unit(e) = target {
            return unit_content_count(world, *e, c);
        }
        return 0.0;
    };
    match c.type_ {
        ContentType::Item => world
            .get::<ItemModule>(*e)
            .map(|m| m.get(crate::content::ItemId::new(c.id)) as f64)
            .unwrap_or(0.0),
        ContentType::Liquid => world
            .get::<LiquidModule>(*e)
            .map(|m| m.get(crate::content::LiquidId::new(c.id)) as f64)
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

fn unit_content_count(world: &World, e: Entity, c: crate::content::ContentRef) -> f64 {
    if c.type_ == ContentType::Item {
        world
            .get::<ItemModule>(e)
            .map(|m| m.get(crate::content::ItemId::new(c.id)) as f64)
            .unwrap_or(0.0)
    } else {
        0.0
    }
}

fn sense_building(world: &World, e: Entity, a: LAccess) -> Sensed {
    let building = world.get::<Building>(e);
    let health = world.get::<Health>(e);
    let def = block_def(world, e);
    // Family override first (`Building.sense`/`senseObject` overrides; G06 T7):
    // turrets, crafters, producers, ... implement `logic_sense`.
    if let Some(block) = building.map(|b| b.block)
        && let Some(inst) = world
            .get_resource::<BlockTable>()
            .and_then(|table| table.get(block))
        && let Some(sensed) = inst.behavior.logic_sense(world, e, a)
    {
        return sensed;
    }
    match a {
        LAccess::Dead => Sensed::Num(if health.is_some_and(|h| h.dead || h.health <= 0.0) {
            1.0
        } else {
            0.0
        }),
        LAccess::Health => Sensed::Num(health.map(|h| h.health as f64).unwrap_or(0.0)),
        LAccess::MaxHealth => Sensed::Num(health.map(|h| h.max_health as f64).unwrap_or(0.0)),
        LAccess::Team => Sensed::Num(entity_team(world, e).unwrap_or(0) as f64),
        LAccess::X | LAccess::Y => {
            // Java `Building.x = tile.drawx()` (BuildingComp.java:126), and
            // `World.conv(x) = x / tilesize` (World.java:170).
            let value = world
                .get::<Pos>(e)
                .map(|p| if a == LAccess::X { p.x } else { p.y } as f64 / 8.0);
            Sensed::Num(value.unwrap_or(0.0))
        }
        LAccess::Rotation => Sensed::Num(building.map(|b| b.rotation as f64).unwrap_or(0.0)),
        LAccess::Enabled => Sensed::Num(if building.is_some_and(|b| b.enabled) {
            1.0
        } else {
            0.0
        }),
        LAccess::Efficiency => Sensed::Num(building.map(|b| b.efficiency as f64).unwrap_or(0.0)),
        LAccess::Timescale => Sensed::Num(building.map(|b| b.time_scale as f64).unwrap_or(0.0)),
        LAccess::Size => Sensed::Num(def.as_ref().map(|d| d.size as f64).unwrap_or(1.0)),
        LAccess::Solid => Sensed::Num(if def.as_ref().is_some_and(|d| d.solid) {
            1.0
        } else {
            0.0
        }),
        LAccess::Armor => Sensed::Num(def.as_ref().map(|d| d.armor as f64).unwrap_or(0.0)),
        LAccess::TotalItems => Sensed::Num(
            world
                .get::<ItemModule>(e)
                .map(|m| m.total as f64)
                .unwrap_or(0.0),
        ),
        LAccess::ItemCapacity => {
            Sensed::Num(def.as_ref().map(|d| d.item_capacity as f64).unwrap_or(0.0))
        }
        LAccess::TotalLiquids => Sensed::Num(
            world
                .get::<LiquidModule>(e)
                .map(|m| m.current_amount as f64)
                .unwrap_or(0.0),
        ),
        LAccess::LiquidCapacity => Sensed::Num(
            def.as_ref()
                .map(|d| d.liquid_capacity.max(0.0) as f64)
                .unwrap_or(0.0),
        ),
        LAccess::TotalPower => {
            // `power == null || consPower == null ? 0 : power.status *
            // (consPower.buffered ? consPower.capacity : 1)`.
            let Some(module) = world.get::<PowerModule>(e) else {
                return Sensed::Num(0.0);
            };
            let buffered = def
                .as_ref()
                .and_then(|d| {
                    d.consumes.iter().find_map(|c| match &c.consume {
                        crate::content::registries::blocks::Consume::Power { buffered, .. } => {
                            Some(*buffered)
                        }
                        _ => None,
                    })
                })
                .unwrap_or(0.0);
            Sensed::Num((module.status * if buffered > 0.0 { buffered } else { 1.0 }) as f64)
        }
        LAccess::PowerNetIn
        | LAccess::PowerNetOut
        | LAccess::PowerNetStored
        | LAccess::PowerNetCapacity => {
            // `graph.getLastScaledPowerIn()*60` etc. (0 without power).
            let Some(module) = world.get::<PowerModule>(e) else {
                return Sensed::Num(0.0);
            };
            let graph = world
                .get_resource::<crate::world::blocks::power::graph::PowerGrids>()
                .and_then(|grids| grids.graph(module.graph));
            let Some(graph) = graph else {
                return Sensed::Num(0.0);
            };
            Sensed::Num(match a {
                LAccess::PowerNetIn => graph.last_scaled_power_in() as f64 * 60.0,
                LAccess::PowerNetOut => graph.last_scaled_power_out() as f64 * 60.0,
                LAccess::PowerNetStored => graph.last_power_stored() as f64,
                _ => graph.last_capacity() as f64,
            })
        }
        LAccess::Range => Sensed::Num(
            logic_def_of(world, e)
                .map(|d| (d.range / 8.0) as f64)
                .unwrap_or(0.0),
        ),
        LAccess::Color => {
            let id = entity_team(world, e).unwrap_or(0);
            let color = crate::game::team::Team::get(id).color;
            Sensed::Num(crate::logic::globals::rgba_u8_to_double_bits(
                (color.r * 255.0).round() as i32,
                (color.g * 255.0).round() as i32,
                (color.b * 255.0).round() as i32,
                255,
            ))
        }
        LAccess::DisplayWidth => Sensed::Num(
            display_def_of(world, e)
                .map(|d| d.display_size as f64)
                .unwrap_or(0.0),
        ),
        LAccess::DisplayHeight => Sensed::Num(
            display_def_of(world, e)
                .map(|d| d.display_size as f64)
                .unwrap_or(0.0),
        ),
        LAccess::BufferSize => Sensed::Num(
            world
                .get::<crate::logic::blocks::LogicDisplayState>(e)
                .map(|d| d.commands.len() as f64)
                .unwrap_or(0.0),
        ),
        LAccess::Operations => Sensed::Num(
            world
                .get::<crate::logic::blocks::LogicDisplayState>(e)
                .map(|d| d.operations as f64)
                .unwrap_or(0.0),
        ),
        LAccess::MemoryCapacity => Sensed::Num(
            memory_def_of(world, e)
                .map(|d| d.capacity as f64)
                .unwrap_or(0.0),
        ),
        LAccess::CameraX
        | LAccess::CameraY
        | LAccess::CameraWidth
        | LAccess::CameraHeight
        | LAccess::Shooting
        | LAccess::Boosting
        | LAccess::Mining => Sensed::Num(0.0),
        LAccess::Id => Sensed::Num(e.index().index() as f64),
        LAccess::Type => match world.get::<Building>(e) {
            Some(b) => Sensed::Obj(Some(LogicObject::Content(
                crate::content::ContentRef::block(b.block),
            ))),
            None => Sensed::Obj(None),
        },
        LAccess::FirstItem => Sensed::Obj(
            world
                .get::<ItemModule>(e)
                .and_then(|m| m.first())
                .map(|i| LogicObject::Content(crate::content::ContentRef::item(i))),
        ),
        LAccess::Controlled => Sensed::Num(0.0),
        LAccess::Config => Sensed::Obj(None),
        LAccess::Name => Sensed::Obj(def.as_ref().map(|d| LogicObject::Str(d.name.clone()))),
        // Java `BuildingComp.sense` default: `NaN` → null through `setnum`.
        _ => Sensed::Obj(None),
    }
}

fn sense_unit(world: &World, e: Entity, a: LAccess) -> Sensed {
    let health = world.get::<Health>(e);
    let pos = world.get::<Pos>(e);
    let core = world.get::<crate::entities::comp::unit::UnitCore>(e);
    let phys = world.get::<crate::entities::comp::unit::PhysicsComp>(e);
    let hit = world.get::<crate::entities::comp::unit::HitboxComp>(e);
    let vel = world.get::<crate::entities::comp::Vel>(e);
    match a {
        LAccess::Dead => Sensed::Num(if core.is_some_and(|c| c.dead) {
            1.0
        } else {
            0.0
        }),
        LAccess::Health => Sensed::Num(health.map(|h| h.health as f64).unwrap_or(0.0)),
        LAccess::MaxHealth => Sensed::Num(health.map(|h| h.max_health as f64).unwrap_or(0.0)),
        LAccess::Team => Sensed::Num(entity_team(world, e).unwrap_or(0) as f64),
        LAccess::X => Sensed::Num(pos.map(|p| (p.x / 8.0) as f64).unwrap_or(0.0)),
        LAccess::Y => Sensed::Num(pos.map(|p| (p.y / 8.0) as f64).unwrap_or(0.0)),
        // `vel * 60 / 8`: world px/tick -> tiles/sec.
        LAccess::VelocityX => Sensed::Num(vel.map(|v| (v.x * 60.0 / 8.0) as f64).unwrap_or(0.0)),
        LAccess::VelocityY => Sensed::Num(vel.map(|v| (v.y * 60.0 / 8.0) as f64).unwrap_or(0.0)),
        LAccess::Rotation => Sensed::Num(core.map(|c| c.rotation as f64).unwrap_or(0.0)),
        // `type.speed * 60 / 8 * speedMultiplier`.
        LAccess::Speed => {
            let multiplier = world
                .get::<crate::entities::comp::unit::StatusComp>(e)
                .map(|s| s.speed_multiplier)
                .unwrap_or(1.0);
            Sensed::Num(
                phys.map(|p| (p.speed * 60.0 / 8.0 * multiplier) as f64)
                    .unwrap_or(0.0),
            )
        }
        LAccess::Flying => Sensed::Num(if phys.is_some_and(|p| p.flying) {
            1.0
        } else {
            0.0
        }),
        LAccess::Boosting => Sensed::Num(if core.is_some_and(|c| c.boosting) {
            1.0
        } else {
            0.0
        }),
        LAccess::Size => Sensed::Num(hit.map(|h| (h.hit_size / 8.0) as f64).unwrap_or(0.0)),
        LAccess::TotalItems => Sensed::Num(
            world
                .get::<ItemModule>(e)
                .map(|m| m.total as f64)
                .unwrap_or(0.0),
        ),
        LAccess::ItemCapacity => Sensed::Num(
            world
                .get::<ItemModule>(e)
                .map(|m| m.items.len() as f64)
                .unwrap_or(0.0),
        ),
        LAccess::Shield => Sensed::Num(
            world
                .get::<crate::entities::comp::unit::ShieldComp>(e)
                .map(|s| s.shield as f64)
                .unwrap_or(0.0),
        ),
        LAccess::Armor => {
            // `armorOverride >= 0 ? armorOverride : armor`; the base type armor
            // is not projected onto the unit (remaining T7 row in the summary).
            let value = world
                .get::<crate::entities::comp::unit::StatusComp>(e)
                .filter(|s| s.armor_override >= 0.0)
                .map(|s| s.armor_override as f64)
                .unwrap_or(0.0);
            Sensed::Num(value)
        }
        LAccess::Range | LAccess::Shooting => Sensed::Num(0.0),
        // `controlled`: `ctrlProcessor = 1`, `ctrlPlayer = 2`, `ctrlCommand = 3`
        // (GlobalVars.java:21-24), command only while a command is set.
        LAccess::Controlled => {
            let value = match crate::entities::comp::unit::ai_kind_of(world, e) {
                Some(crate::ai::AiKind::Logic) => crate::logic::globals::CTRL_PROCESSOR as f64,
                Some(crate::ai::AiKind::Player) => crate::logic::globals::CTRL_PLAYER as f64,
                Some(crate::ai::AiKind::Command) => {
                    let has_command = world
                        .get::<crate::ai::controller::ControllerSlot>(e)
                        .and_then(|slot| slot.target)
                        .is_some();
                    if has_command {
                        crate::logic::globals::CTRL_COMMAND as f64
                    } else {
                        0.0
                    }
                }
                _ => 0.0,
            };
            Sensed::Num(value)
        }
        LAccess::Mining => Sensed::Num(
            if world
                .get::<crate::entities::comp::unit::MinerComp>(e)
                .is_some_and(|m| m.mining)
            {
                1.0
            } else {
                0.0
            },
        ),
        LAccess::MineX | LAccess::MineY => {
            let value = world
                .get::<crate::entities::comp::unit::MinerComp>(e)
                .and_then(|m| m.mine_tile)
                .map(|tile| {
                    if a == LAccess::MineX {
                        tile.x()
                    } else {
                        tile.y()
                    }
                })
                .map(|coord| coord as f64)
                .unwrap_or(-1.0);
            Sensed::Num(value)
        }
        LAccess::Id => Sensed::Num(e.index().index() as f64),
        LAccess::Type => match crate::entities::comp::unit::unit_type_of(world, e) {
            Some(t) => Sensed::Obj(Some(LogicObject::Content(crate::content::ContentRef::new(
                ContentType::Unit,
                t.raw(),
            )))),
            None => Sensed::Obj(None),
        },
        // `firstItem -> stack.amount == 0 ? null : item`.
        LAccess::FirstItem => Sensed::Obj(
            world
                .get::<ItemModule>(e)
                .filter(|m| m.total > 0)
                .and_then(|m| m.first())
                .map(|i| LogicObject::Content(crate::content::ContentRef::item(i))),
        ),
        // Java `UnitComp.senseObject` default: null.
        _ => Sensed::Obj(None),
    }
}

/// `Settable.setProp` numeric dispatch; returns whether the accessor was applied.
pub fn set_prop_num(world: &mut World, target: &LogicObject, a: LAccess, v: f64) -> bool {
    match target {
        LogicObject::Building(e) => set_prop_building_num(world, *e, a, v),
        LogicObject::Unit(e) => set_prop_unit_num(world, *e, a, v),
        _ => false,
    }
}

fn set_prop_building_num(world: &mut World, e: Entity, a: LAccess, v: f64) -> bool {
    match a {
        LAccess::Health => {
            // `BuildingComp.setProp(health)` (`BuildingComp.java:2176-2183`):
            // clamp; `<= 0` destroys through `Call.buildDestroyed` (the port
            // marks it dead and `resolve_deaths` runs the destruction).
            if let Some(mut h) = world.get_mut::<Health>(e) {
                h.health = (v as f32).clamp(0.0, h.max_health);
                if h.health <= 0.0 {
                    h.dead = true;
                }
                return true;
            }
            false
        }
        LAccess::Team => {
            if let Some(mut t) = world.get_mut::<TeamComp>(e) {
                t.team = v as u8;
                return true;
            }
            false
        }
        LAccess::TotalPower => {
            // `buffered ? status = clamp(v / capacity) : ignore`.
            let Some(block) = world.get::<Building>(e).map(|b| b.block) else {
                return false;
            };
            let capacity = world
                .get_resource::<BlockTable>()
                .and_then(|table| table.get(block))
                .and_then(|inst| {
                    inst.def.consumes.iter().find_map(|c| match &c.consume {
                        crate::content::registries::blocks::Consume::Power { buffered, .. } => {
                            Some(*buffered)
                        }
                        _ => None,
                    })
                })
                .unwrap_or(0.0);
            if capacity <= 0.0 {
                // Non-buffered consumer: the set is ignored (still "handled").
                return true;
            }
            if let Some(mut p) = world.get_mut::<PowerModule>(e) {
                p.status = (v as f32 / capacity).clamp(0.0, 1.0);
                return true;
            }
            false
        }
        _ => false,
    }
}

fn set_prop_unit_num(world: &mut World, e: Entity, a: LAccess, v: f64) -> bool {
    match a {
        LAccess::Health => {
            // `UnitComp.setProp(health)` (`UnitComp.java:356-362`): clamp and
            // kill at `<= 0` (`resolve_deaths` runs the destroy path).
            if let Some(mut h) = world.get_mut::<Health>(e) {
                h.health = (v as f32).clamp(0.0, h.max_health);
                if h.health <= 0.0 {
                    h.dead = true;
                }
                return true;
            }
            false
        }
        LAccess::Team => {
            if let Some(mut t) = world.get_mut::<TeamComp>(e) {
                t.team = v as u8;
                return true;
            }
            false
        }
        LAccess::X => {
            // `unconv(x)`: tiles -> world px.
            if let Some(mut p) = world.get_mut::<Pos>(e) {
                p.x = (v * 8.0) as f32;
                return true;
            }
            false
        }
        LAccess::Y => {
            if let Some(mut p) = world.get_mut::<Pos>(e) {
                p.y = (v * 8.0) as f32;
                return true;
            }
            false
        }
        LAccess::VelocityX | LAccess::VelocityY => {
            // `v * 8 / 60`: tiles/sec -> world px/tick.
            if let Some(mut vel) = world.get_mut::<crate::entities::comp::Vel>(e) {
                let value = (v * 8.0 / 60.0) as f32;
                if a == LAccess::VelocityX {
                    vel.x = value;
                } else {
                    vel.y = value;
                }
                return true;
            }
            false
        }
        LAccess::Rotation => {
            if let Some(mut c) = world.get_mut::<crate::entities::comp::unit::UnitCore>(e) {
                c.rotation = v as f32;
                return true;
            }
            false
        }
        LAccess::Shield => {
            if let Some(mut shield) = world.get_mut::<crate::entities::comp::unit::ShieldComp>(e) {
                shield.shield = (v as f32).max(0.0);
                return true;
            }
            false
        }
        LAccess::Flag => {
            // `flag = value` (`UnitComp.java:381`).
            world
                .entity_mut(e)
                .insert(crate::entities::comp::unit::comp::UnitFlag(v));
            true
        }
        LAccess::Speed => {
            // `statusSpeed(clamp(v, 0, 1000))` (`UnitComp.java:382`):
            // `speedMultiplier = speed / (type.speed * 60 / tilesize)`.
            let type_speed = unit_content(world).and_then(|content| {
                world
                    .get::<crate::entities::comp::unit::comp::UnitTypeComp>(e)
                    .and_then(|t| content.unit(t.type_id).map(|def| def.speed))
            });
            let Some(type_speed) = type_speed else {
                return true;
            };
            if !apply_dynamic_status(world, e) {
                return true;
            }
            let clamped = (v as f32).clamp(0.0, 1000.0);
            let divisor = type_speed * 60.0 / 8.0;
            if let Some(mut dynamic) =
                world.get_mut::<crate::entities::comp::unit::comp::DynamicStatus>(e)
            {
                dynamic.speed_multiplier = if divisor > 0.0 {
                    clamped / divisor
                } else {
                    1.0
                };
            }
            true
        }
        LAccess::Armor => {
            // `statusArmor(max(v, 0))` (`UnitComp.java:383`).
            if !apply_dynamic_status(world, e) {
                return true;
            }
            if let Some(mut dynamic) =
                world.get_mut::<crate::entities::comp::unit::comp::DynamicStatus>(e)
            {
                dynamic.armor_override = (v as f32).max(0.0);
            }
            true
        }
        _ => false,
    }
}

/// Content registry when the live unit runtime is installed.
fn unit_content(world: &World) -> Option<&crate::content::ContentRegistry> {
    world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .map(|runtime| &runtime.content)
}

/// `StatusComp.applyDynamicStatus()` (`StatusComp.java:136-147`): inserts the
/// [`crate::entities::comp::unit::comp::DynamicStatus`] component (once) and
/// applies the `dynamic` status with infinite time.
fn apply_dynamic_status(world: &mut World, e: Entity) -> bool {
    let Some(dynamic) = unit_content(world).and_then(|content| content.status_id("dynamic")) else {
        return false;
    };
    if world
        .get::<crate::entities::comp::unit::comp::DynamicStatus>(e)
        .is_none()
    {
        world
            .entity_mut(e)
            .insert(crate::entities::comp::unit::comp::DynamicStatus::default());
    }
    if let Some(mut statuses) = world.get_mut::<crate::entities::comp::unit::comp::StatusComp>(e) {
        statuses.ensure_dynamic(dynamic);
        return true;
    }
    false
}

/// `Settable.setProp` object dispatch; returns whether it was applied.
pub fn set_prop_obj(world: &mut World, target: &LogicObject, a: LAccess, v: &LogicObject) -> bool {
    let LogicObject::Team(team) = v else {
        return false;
    };
    match target {
        LogicObject::Building(e) => {
            if matches!(a, LAccess::Team)
                && let Some(mut t) = world.get_mut::<TeamComp>(*e)
            {
                t.team = *team;
                return true;
            }
            false
        }
        LogicObject::Unit(e) => {
            if matches!(a, LAccess::Team)
                && let Some(mut t) = world.get_mut::<TeamComp>(*e)
            {
                t.team = *team;
                return true;
            }
            false
        }
        _ => false,
    }
}

/// `Settable.setProp(Content, amount)` (`BuildingComp.setProp`,
/// `BuildingComp.java:2210-2227`; `UnitComp.setProp`,
/// `UnitComp.java:415-424`).
pub fn set_prop_content(
    world: &mut World,
    target: &LogicObject,
    c: crate::content::ContentRef,
    amount: f64,
) -> bool {
    match target {
        LogicObject::Building(e) => set_prop_content_building(world, *e, c, amount),
        LogicObject::Unit(e) => set_prop_content_unit(world, *e, c, amount),
        _ => false,
    }
}

fn set_prop_content_building(
    world: &mut World,
    e: Entity,
    c: crate::content::ContentRef,
    amount: f64,
) -> bool {
    match c.type_ {
        ContentType::Item => {
            // Java SETS the count (`items.get != amount`): a shortfall runs
            // `handleStack(acceptStack(diff))`, a surplus `removeStack`.
            use crate::world::blocks::distribution::transfer;
            let item = crate::content::ItemId::new(c.id);
            let target = amount as i32;
            let current = {
                let Some(module) = world.get_mut::<ItemModule>(e) else {
                    return false;
                };
                module.get(item)
            };
            if current == target {
                return true;
            }
            if current < target {
                let accepted =
                    transfer::dispatch_accept_stack(world, e, item, target - current, None);
                if accepted > 0 {
                    transfer::dispatch_handle_stack(world, e, item, accepted);
                }
            } else if target >= 0
                && let Some(mut module) = world.get_mut::<ItemModule>(e)
            {
                module.remove(item, current - target);
            }
            true
        }
        ContentType::Liquid => {
            // Java clamps to `block.liquidCapacity`; decreasing is always
            // allowed, increasing needs `acceptLiquid` + the current-liquid
            // gate (`liquids.current() == liquid || currentAmount <= 0.1 ||
            // consumesLiquid(liquid)`).
            let capacity = block_def(world, e)
                .map(|def| def.liquid_capacity)
                .unwrap_or(0.0);
            let liquid = crate::content::LiquidId::new(c.id);
            let value = (amount as f32).clamp(0.0, capacity);
            let (current, is_current, total) = {
                let Some(module) = world.get_mut::<LiquidModule>(e) else {
                    return false;
                };
                (
                    module.get(liquid),
                    crate::world::blocks::liquid::current_liquid(&module) == Some(liquid),
                    module.current_amount,
                )
            };
            let accepted_gate = value < current
                || (crate::world::blocks::liquid::accept_liquid(world, e, e, liquid)
                    && (is_current || total <= 0.1 || block_consumes_liquid(world, e, liquid)));
            if accepted_gate && let Some(mut module) = world.get_mut::<LiquidModule>(e) {
                if value < current {
                    module.remove(liquid, current - value);
                } else if value > current {
                    module.add(liquid, value - current, capacity);
                }
            }
            true
        }
        _ => false,
    }
}

fn set_prop_content_unit(
    world: &mut World,
    e: Entity,
    c: crate::content::ContentRef,
    amount: f64,
) -> bool {
    match c.type_ {
        ContentType::Item => {
            // `stack.item = item; stack.amount = clamp((int)value, 0,
            // type.itemCapacity)`.
            let capacity = unit_content(world).and_then(|content| {
                world
                    .get::<crate::entities::comp::unit::comp::UnitTypeComp>(e)
                    .and_then(|t| content.unit(t.type_id).map(|def| def.item_capacity))
            });
            let item = crate::content::ItemId::new(c.id);
            let amount = (amount as i32).clamp(0, capacity.unwrap_or(0));
            if let Some(mut items) =
                world.get_mut::<crate::entities::comp::unit::comp::ItemsComp>(e)
            {
                items.item = Some((item, amount));
                return true;
            }
            false
        }
        ContentType::Status => {
            // `setDuration(effect, value * 60)`: set an existing entry's time,
            // else `apply(effect, duration)`.
            use crate::entities::comp::unit::comp::{StatusComp, StatusEntry};
            let status = crate::content::StatusId::new(c.id);
            let duration = (amount as f32) * 60.0;
            if let Some(mut statuses) = world.get_mut::<StatusComp>(e) {
                if statuses.has_effect_of(status) {
                    statuses.set_duration(status, duration);
                } else if duration > 0.0 {
                    statuses.apply(StatusEntry::new(status, duration));
                }
                return true;
            }
            false
        }
        _ => false,
    }
}

/// `Block.consumesLiquid(liquid)`: the block has a matching liquid consumer.
fn block_consumes_liquid(world: &World, e: Entity, liquid: crate::content::LiquidId) -> bool {
    block_def(world, e).is_some_and(|def| {
        def.consumes.iter().any(|c| {
            matches!(
                &c.consume,
                crate::content::registries::blocks::Consume::Liquid { liquid: l, .. } if *l == liquid
            )
        })
    })
}

/// `Controllable.control` dispatch; returns whether the accessor was applied.
///
/// `enabled` and object `config` target buildings; unit control is handled by
/// the VM's `ucontrol` path (`logic/executor/unit_control.rs`). `exec_build`/
/// `exec_privileged` implement `ControlI`'s link gate
/// (`LExecutor.java:558-594`).
pub fn control(
    world: &mut World,
    target: &LogicObject,
    a: LAccess,
    params: &[&LVar],
    exec_build: Option<Entity>,
    exec_privileged: bool,
) -> bool {
    let LogicObject::Building(e) = target else {
        return false;
    };
    let e = *e;
    // `ControlI.run`: a building is only controllable by its own processor
    // (valid link) unless the executor is privileged.
    let allowed = exec_privileged
        || exec_build.is_some_and(|build| {
            let build_privileged = crate::logic::blocks::privileged_of(world, build);
            crate::logic::blocks::logic_block::valid_link(world, build, build_privileged, Some(e))
        });
    if !allowed {
        return false;
    }
    match a {
        LAccess::Enabled => {
            let enabled = params.first().map(|p| p.as_bool()).unwrap_or(false);
            if let Some(mut b) = world.get_mut::<Building>(e) {
                if enabled {
                    // `b.noSleep()`; the port has no sleep state to clear.
                    b.enabled = true;
                } else {
                    // `b.lastDisabler = exec.build` (`BuildingComp.control`).
                    b.enabled = false;
                    b.last_disabler = exec_build;
                }
                return true;
            }
            false
        }
        LAccess::Config => {
            // `config(p1)`: `block.logicConfigurable && !(p1 instanceof
            // LogicBuild)` -> `configured(null, p1)`.
            let Some(value) = params.first().and_then(|p| p.value_obj()).cloned() else {
                // Numeric/absent config values are accepted as a controlled
                // no-op (Java only routes objects here).
                return true;
            };
            if is_logic_processor(world, e) {
                return false;
            }
            let Some(block) = world.get::<Building>(e).map(|b| b.block) else {
                return false;
            };
            let Some(inst) = world
                .get_resource::<BlockTable>()
                .and_then(|table| table.get(block))
                .cloned()
            else {
                return false;
            };
            // `Block.logicConfigurable`: any `UnlockableContent`-typed config.
            let configurable = inst.behavior.config_kinds().iter().any(|kind| {
                matches!(
                    kind,
                    ConfigKind::Item
                        | ConfigKind::Liquid
                        | ConfigKind::Block
                        | ConfigKind::Unit
                        | ConfigKind::Content
                )
            });
            if !configurable {
                return false;
            }
            let Some(value) = content_config_value(&value) else {
                return false;
            };
            inst.behavior.configured(world, e, None, value);
            true
        }
        _ => false,
    }
}

/// Maps a content logic object to the `ConfigValue` a build would configure.
fn content_config_value(obj: &LogicObject) -> Option<ConfigValue> {
    let LogicObject::Content(c) = obj else {
        return None;
    };
    Some(match c.type_ {
        ContentType::Item => ConfigValue::Item(ItemId::new(c.id)),
        ContentType::Liquid => ConfigValue::Liquid(LiquidId::new(c.id)),
        ContentType::Block => ConfigValue::Block(BlockId::new(c.id)),
        ContentType::Unit => ConfigValue::Unit(UnitTypeId::new(c.id)),
        other => ConfigValue::Content(other, c.id),
    })
}

/// Reads the building at a logic link/name (`optionalLink`).
pub fn linked_build(world: &World, x: i32, y: i32) -> Option<Entity> {
    build_at(world, x, y)
}

/// Whether a building carries a logic processor (`LogicBlockState`).
pub fn is_logic_processor(world: &World, e: Entity) -> bool {
    world.get::<LogicBlockState>(e).is_some()
}

/// A unit marker target (used by radar candidate collection).
pub fn is_unit(world: &World, e: Entity) -> bool {
    world.get::<Unit>(e).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_access_table_matches_upstream() {
        assert_eq!(LAccess::ALL.len(), 77);
        assert_eq!(LAccess::TotalItems.name(), "totalItems");
        assert_eq!(LAccess::Enabled.params(), &["to"]);
        assert!(LAccess::Config.is_obj());
        assert!(LAccess::CameraX.privileged());
        assert!(!LAccess::senseable().contains(&LAccess::CameraX));
        assert!(LAccess::senseable_privileged().contains(&LAccess::CameraX));
        assert!(LAccess::controls().contains(&LAccess::Shoot));
        assert!(LAccess::SETTABLE.contains(&LAccess::Health));
    }

    #[test]
    fn building_sense_and_control_spot_checks() {
        use crate::world::harness::BuildHarness;
        let mut harness = BuildHarness::new(8, 8, 1);
        let switch = harness.content().block_id("switch").expect("switch");
        assert!(harness.place(3, 3, switch, 0, true));
        let e = harness.build_at(3, 3).expect("entity");
        let target = LogicObject::Building(e);
        assert_eq!(
            sense(&harness.world, &target, LAccess::Enabled),
            Sensed::Num(1.0)
        );
        assert_eq!(
            sense(&harness.world, &target, LAccess::Size),
            Sensed::Num(1.0)
        );
        assert_eq!(
            sense(&harness.world, &target, LAccess::Dead),
            Sensed::Num(0.0)
        );

        // `control enabled switch1 0` clears the enabled flag; the switch has no
        // processor link, so the gate is bypassed with `privileged = true`.
        let mut off = LVar::new("p");
        off.set_num(0.0);
        assert!(control(
            &mut harness.world,
            &target,
            LAccess::Enabled,
            &[&off],
            None,
            true,
        ));
        assert_eq!(
            sense(&harness.world, &target, LAccess::Enabled),
            Sensed::Num(0.0)
        );

        // `setprop @health` clamps to max health.
        let health = harness.world.get::<Health>(e).unwrap().max_health as f64;
        assert!(set_prop_num(
            &mut harness.world,
            &target,
            LAccess::Health,
            health + 100.0
        ));
        assert_eq!(
            sense(&harness.world, &target, LAccess::Health),
            Sensed::Num(health)
        );
    }

    #[test]
    fn control_requires_link_or_privilege() {
        use crate::world::harness::BuildHarness;
        // `ControlI.run`: `exec.privileged || exec.build.validLink(b)`; a
        // processor far outside its own range cannot control the switch.
        let mut harness = BuildHarness::new(16, 16, 1);
        let switch = harness.content().block_id("switch").expect("switch");
        let processor = harness
            .content()
            .block_id("micro-processor")
            .expect("micro-processor");
        assert!(harness.place(0, 0, processor, 0, true));
        assert!(harness.place(15, 15, switch, 0, true));
        let pe = harness.build_at(0, 0).expect("processor");
        let se = harness.build_at(15, 15).expect("switch");
        let target = LogicObject::Building(se);
        let mut off = LVar::new("p");
        off.set_num(0.0);
        assert!(
            !control(
                &mut harness.world,
                &target,
                LAccess::Enabled,
                &[&off],
                Some(pe),
                false,
            ),
            "out-of-range link is refused"
        );
        assert_eq!(
            sense(&harness.world, &target, LAccess::Enabled),
            Sensed::Num(1.0),
            "switch stays enabled"
        );
        // The disabling processor is remembered (`b.lastDisabler = exec.build`)
        // when the gate passes.
        assert!(control(
            &mut harness.world,
            &target,
            LAccess::Enabled,
            &[&off],
            Some(pe),
            true,
        ));
        assert_eq!(
            harness
                .world
                .get::<Building>(se)
                .and_then(|b| b.last_disabler),
            Some(pe)
        );
    }
}
