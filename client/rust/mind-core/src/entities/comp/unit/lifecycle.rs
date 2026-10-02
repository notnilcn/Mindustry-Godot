// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit lifecycle (plan 11 §3.4): spawn/remove/kill and the per-kind bundle.
//!
//! Ported from `type/UnitType.java` (`create`/`spawn`/`add`/`remove`) and
//! `entities/comp/UnitComp.java` (`setType`/`add`/`remove`/`kill`/`destroy`).
//! M0 spawns a single unit with the base closure plus its kind marker; segmented
//! spawn, pools and death effects land with M1/M6.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::ai::controller::{AiKind, ControllerSlot, select_ai};
use crate::content::ContentRegistry;
use crate::content::UnitTypeId;
use crate::content::registries::units::{UnitComponent, UnitTypeDef};
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::*;
use crate::entities::comp::{Health, Pos, TeamComp, Unit, Vel};

/// Spawns a unit of content type `name` at world pixels `(x, y)` facing
/// `rotation` degrees. Returns the entity, or `None` for an unknown type.
///
/// Mirrors `UnitType.create(team)` + `spawn`: the bundle is the base closure
/// plus the def's kind components; derived fields come from the content record.
#[allow(clippy::too_many_arguments)] // mirrors the Java `create/spawn` signature
pub fn spawn_unit(
    world: &mut World,
    content: &ContentRegistry,
    seq: u64,
    name: &str,
    team: u8,
    x: f32,
    y: f32,
    rotation: f32,
) -> Option<Entity> {
    let unit = content.unit_by_name(name)?;
    Some(spawn_unit_def(world, seq, unit, team, x, y, rotation))
}

/// Spawns a unit from a resolved content record ([`UnitTypeDef`]).
pub fn spawn_unit_def(
    world: &mut World,
    seq: u64,
    unit: &UnitTypeDef,
    team: u8,
    x: f32,
    y: f32,
    rotation: f32,
) -> Entity {
    let core = UnitCore::new(rotation);
    let physics = PhysicsComp {
        speed: unit.speed,
        boost_multiplier: unit.boost_multiplier,
        drag: unit.drag,
        accel: unit.accel,
        flying: unit.flying,
        can_boost: unit.can_boost,
    };
    let entity = world
        .spawn((
            Unit,
            Pos { x, y },
            Vel { x: 0.0, y: 0.0 },
            TeamComp { team },
            Health::new(unit.health),
            UnitTypeComp { type_id: unit.id },
            HitboxComp {
                hit_size: unit.hit_size,
            },
            physics,
            core,
            ControllerSlot::new(select_ai(unit)),
            EntitySeq(seq),
        ))
        .id();

    insert_kind_components(world, entity, unit, rotation);
    entity
}

/// Inserts the per-kind marker components for a unit's `@EntityDef` group.
pub fn insert_kind_components(
    world: &mut World,
    entity: Entity,
    unit: &UnitTypeDef,
    rotation: f32,
) {
    let components = unit.entity_def.components;
    let has = |component: UnitComponent| components.contains(&component);
    if has(UnitComponent::Mech) {
        world.entity_mut(entity).insert(MechComp::new(rotation));
    }
    if has(UnitComponent::Legs) {
        world.entity_mut(entity).insert(LegsComp { leg_count: 4 });
    }
    if has(UnitComponent::Tank) {
        world
            .entity_mut(entity)
            .insert(TankComp { tread_time: 0.0 });
    }
    if has(UnitComponent::WaterMove) {
        world
            .entity_mut(entity)
            .insert(WaterMoveComp { trail_time: 0.0 });
    }
    if has(UnitComponent::Crawl) {
        world.entity_mut(entity).insert(CrawlComp {
            segment_rot: rotation,
        });
    }
    if has(UnitComponent::ElevationMove) {
        world.entity_mut(entity).insert(ElevationMoveComp {
            flying: unit.flying,
        });
    }
    if has(UnitComponent::Payload) {
        world.entity_mut(entity).insert(PayloadComp {
            capacity: unit.payload_capacity,
        });
    }
    if has(UnitComponent::BlockUnit) {
        world
            .entity_mut(entity)
            .insert(BlockUnitComp { tile: i32::MIN });
    }
    if has(UnitComponent::BuildingTether) {
        world
            .entity_mut(entity)
            .insert(BuildingTetherComp { building: None });
    }
    if has(UnitComponent::TimedKill) {
        world.entity_mut(entity).insert(TimedKillComp {
            time: 0.0,
            lifetime: 0.0,
        });
    }
    if has(UnitComponent::TargetDummy) {
        world.entity_mut(entity).insert(TargetDummyComp);
    }
}

/// Removes a unit from the world (`UnitComp.remove`). Returns whether it existed.
pub fn remove_unit(world: &mut World, entity: Entity) -> bool {
    if world.get_entity(entity).is_err() {
        return false;
    }
    world.despawn(entity);
    true
}

/// Flags a unit dead and removes it (`UnitComp.kill`/`destroy` minimal path).
pub fn kill_unit(world: &mut World, entity: Entity) -> bool {
    if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
        core.dead = true;
    }
    remove_unit(world, entity)
}

/// Reads a unit's content type id.
pub fn unit_type_of(world: &World, entity: Entity) -> Option<UnitTypeId> {
    world.get::<UnitTypeComp>(entity).map(|comp| comp.type_id)
}

/// Reads a unit's controller slot.
pub fn controller_of(world: &World, entity: Entity) -> Option<ControllerSlot> {
    world.get::<ControllerSlot>(entity).copied()
}

/// Sets the unit's move target (`set_target`; M0 test/command hook).
pub fn set_move_target(world: &mut World, entity: Entity, target: crate::world::TilePos) -> bool {
    match world.get_mut::<ControllerSlot>(entity) {
        Some(mut slot) => {
            slot.target = Some(target);
            true
        }
        None => false,
    }
}

/// The active AI kind of a unit.
pub fn ai_kind_of(world: &World, entity: Entity) -> Option<AiKind> {
    world.get::<ControllerSlot>(entity).map(|slot| slot.kind)
}
