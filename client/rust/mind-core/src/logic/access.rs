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

use crate::content::ContentType;
use crate::entities::comp::{Building, Health, Pos, TeamComp, Unit};
use crate::logic::blocks::{LogicBlockState, build_at, display_def_of, memory_def_of};
use crate::logic::value::{LVar, conv};
use crate::world::block::BlockTable;
use crate::world::modules::{ItemModule, LiquidModule, PowerModule};

/// Team of an entity (`team.id`).
pub fn entity_team(world: &World, e: Entity) -> Option<u8> {
    world.get::<TeamComp>(e).map(|t| t.team)
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
            LAccess::Id => Sensed::Num(c.id as f64),
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
            let Some(building) = building else {
                return Sensed::Num(0.0);
            };
            let size = def.as_ref().map(|d| d.size).unwrap_or(1);
            // `Building.x` is the multiblock center in tile coordinates.
            let tile = building.tile;
            let coord = if a == LAccess::X { tile.x() } else { tile.y() };
            let half = (size - 1) as f32 / 2.0;
            Sensed::Num(conv(coord as f32 + half) as f64)
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
        LAccess::Solid => Sensed::Num(0.0),
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
        LAccess::TotalPower | LAccess::PowerNetStored => Sensed::Num(
            world
                .get::<PowerModule>(e)
                .map(|m| m.stored as f64)
                .unwrap_or(0.0),
        ),
        LAccess::PowerNetCapacity => Sensed::Num(0.0),
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
        LAccess::Range
        | LAccess::CameraX
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
        _ => Sensed::Num(0.0),
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
        LAccess::X => Sensed::Num(pos.map(|p| p.x as f64).unwrap_or(0.0)),
        LAccess::Y => Sensed::Num(pos.map(|p| p.y as f64).unwrap_or(0.0)),
        LAccess::VelocityX => Sensed::Num(vel.map(|v| v.x as f64).unwrap_or(0.0)),
        LAccess::VelocityY => Sensed::Num(vel.map(|v| v.y as f64).unwrap_or(0.0)),
        LAccess::Rotation => Sensed::Num(core.map(|c| c.rotation as f64).unwrap_or(0.0)),
        LAccess::Speed => Sensed::Num(phys.map(|p| p.speed as f64).unwrap_or(0.0)),
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
        LAccess::Shield | LAccess::Armor | LAccess::Range | LAccess::Shooting => Sensed::Num(0.0),
        LAccess::Controlled => {
            Sensed::Num(match crate::entities::comp::unit::ai_kind_of(world, e) {
                Some(crate::ai::AiKind::Logic) => 2.0,
                Some(crate::ai::AiKind::Command) => 3.0,
                Some(crate::ai::AiKind::Player) => 1.0,
                _ => 0.0,
            })
        }
        LAccess::Id => Sensed::Num(e.index().index() as f64),
        LAccess::Type => match crate::entities::comp::unit::unit_type_of(world, e) {
            Some(t) => Sensed::Obj(Some(LogicObject::Content(crate::content::ContentRef::new(
                ContentType::Unit,
                t.raw(),
            )))),
            None => Sensed::Obj(None),
        },
        _ => Sensed::Num(0.0),
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
            if let Some(mut h) = world.get_mut::<Health>(e) {
                h.health = (v as f32).clamp(0.0, h.max_health);
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
            if let Some(mut p) = world.get_mut::<PowerModule>(e) {
                p.stored = v as f32;
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
            if let Some(mut h) = world.get_mut::<Health>(e) {
                h.health = (v as f32).clamp(0.0, h.max_health);
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
            if let Some(mut p) = world.get_mut::<Pos>(e) {
                p.x = v as f32;
                return true;
            }
            false
        }
        LAccess::Y => {
            if let Some(mut p) = world.get_mut::<Pos>(e) {
                p.y = v as f32;
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
        LAccess::VelocityX | LAccess::VelocityY | LAccess::Speed | LAccess::Armor => true,
        _ => false,
    }
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

/// `Settable.setProp(Content, amount)`.
pub fn set_prop_content(
    world: &mut World,
    target: &LogicObject,
    c: crate::content::ContentRef,
    amount: f64,
) -> bool {
    let LogicObject::Building(e) = target else {
        return false;
    };
    match c.type_ {
        ContentType::Item => {
            let cap = block_def(world, *e).map(|d| d.item_capacity).unwrap_or(0);
            if let Some(mut module) = world.get_mut::<ItemModule>(*e) {
                let item = crate::content::ItemId::new(c.id);
                if amount >= 0.0 {
                    module.add(item, amount as i32, cap);
                } else {
                    module.remove(item, (-amount) as i32);
                }
                return true;
            }
            false
        }
        ContentType::Liquid => {
            if let Some(mut module) = world.get_mut::<LiquidModule>(*e) {
                let liquid = crate::content::LiquidId::new(c.id);
                if amount >= 0.0 {
                    module.add(liquid, amount as f32, f32::MAX);
                } else {
                    module.remove(liquid, (-amount) as f32);
                }
                return true;
            }
            false
        }
        _ => false,
    }
}

/// `Controllable.control` dispatch; returns whether the accessor was applied.
///
/// `enabled` and object `config` target buildings; unit control is handled by
/// the VM's `ucontrol` path (`logic/executor/unit_control.rs`).
pub fn control(world: &mut World, target: &LogicObject, a: LAccess, params: &[&LVar]) -> bool {
    let LogicObject::Building(e) = target else {
        return false;
    };
    let e = *e;
    match a {
        LAccess::Enabled => {
            let enabled = params.first().map(|p| p.as_bool()).unwrap_or(false);
            if enabled {
                if let Some(mut b) = world.get_mut::<Building>(e) {
                    b.enabled = true;
                    b.last_disabler = None;
                    return true;
                }
            } else if let Some(mut b) = world.get_mut::<Building>(e) {
                b.enabled = false;
                b.last_disabler = None;
                return true;
            }
            false
        }
        LAccess::Config => true,
        _ => false,
    }
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

        // `control enabled switch1 0` clears the enabled flag.
        let mut off = LVar::new("p");
        off.set_num(0.0);
        assert!(control(
            &mut harness.world,
            &target,
            LAccess::Enabled,
            &[&off]
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
}
