// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Basic defense block behavior (`world/blocks/defense/{Wall,Door,AutoDoor,
//! Radar,Thruster,TargetDummy}.java`).
//!
//! Plan 07 §3.12 defense family. Bullet-level hooks (lightning/deflect on
//! [`WallBehavior`]) belong to plan 10 and are documented call sites; fog
//! discovery on `Radar` is plan 11. The behavior here is the non-combat state:
//! wall autotiling, door open/close + chaining, and auto-door proximity checks.
//!
//! Autotiling uses the already-computed [`Building::proximity`] list (8 `Edges`
//! neighbors, same-team) and the tile deltas rather than the tile grid, so the
//! hook needs no `WorldGrid` borrow and stays a pure ECS operation.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::BlockId;
use crate::entities::comp::{Building, DoorState, RadarState, TeamComp, WallState};
use crate::world::block::BlockTable;
use crate::world::config::ConfigValue;

use super::{BuildingBehavior, BuildingReader, BuildingWriter};

/// `Geometry.d8` order `(dx, dy)`: E, NE, N, NW, W, SW, S, SE.
const D8: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// `Mathf.lerp(from, to, progress)` with Arc's clamped progress.
fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress.clamp(0.0, 1.0)
}

fn building_identity(
    world: &World,
    e: Entity,
) -> Option<(crate::world::TilePos, BlockId, u8, i32)> {
    let building = world.get::<Building>(e)?;
    let team = world.get::<TeamComp>(e)?.team;
    let size = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(building.block))
        .map(|inst| inst.def.size.max(1))
        .unwrap_or(1);
    Some((building.tile, building.block, team, size))
}

fn is_wall_state(world: &World, e: Entity) -> bool {
    world.get::<WallState>(e).is_some()
}

/// `Wall`/`ShieldWall` autotiling behavior (`Wall.WallBuild`).
#[derive(Debug, Default, Clone, Copy)]
pub struct WallBehavior;

impl BuildingBehavior for WallBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<WallState>(e).is_none() {
            world.entity_mut(e).insert(WallState::default());
        }
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        if autotile_wall(world, e) {
            update_autotile_bits(world, e);
        }
    }

    fn on_proximity_removed(&self, world: &mut World, e: Entity) {
        if autotile_wall(world, e) {
            update_other_bits(world, e);
        }
    }

    fn on_proximity_added(&self, world: &mut World, e: Entity) {
        if autotile_wall(world, e) {
            update_other_bits(world, e);
        }
    }
}

/// Whether a block's `WallDef.autotile` is set.
fn autotile_wall(world: &World, e: Entity) -> bool {
    let Some(block) = world.get::<Building>(e).map(|b| b.block) else {
        return false;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
        .is_some_and(|inst| match &inst.kind_data {
            crate::world::BlockKindData::Wall(def) => def.autotile,
            _ => false,
        })
}

/// 8-direction autotile bits from same-block/same-team proximity neighbors
/// (`Wall.WallBuild.updateAutotileBits`).
pub fn update_autotile_bits(world: &mut World, e: Entity) {
    let Some((tile, block, team, size)) = building_identity(world, e) else {
        return;
    };
    let proximity: SmallVec<[Entity; 6]> = world
        .get::<Building>(e)
        .map(|building| building.proximity.clone())
        .unwrap_or_default();
    let mut bits = 0u8;
    for other in proximity {
        let same = world
            .get::<Building>(other)
            .is_some_and(|b| b.block == block)
            && world.get::<TeamComp>(other).is_some_and(|t| t.team == team);
        if !same {
            continue;
        }
        let Some(other_tile) = world.get::<Building>(other).map(|b| b.tile) else {
            continue;
        };
        let dx = (other_tile.x() as i32 - tile.x() as i32) / size;
        let dy = (other_tile.y() as i32 - tile.y() as i32) / size;
        if let Some(index) = D8.iter().position(|(x, y)| *x == dx && *y == dy) {
            bits |= 1 << index;
        }
    }
    if let Some(mut state) = world.get_mut::<WallState>(e)
        && state.autotile_bits != bits
    {
        state.autotile_bits = bits;
    }
}

/// Recomputes the bits of every same-block neighbor (`updateOtherBits`).
pub fn update_other_bits(world: &mut World, e: Entity) {
    let Some((_tile, block, team, _size)) = building_identity(world, e) else {
        return;
    };
    let proximity: SmallVec<[Entity; 6]> = world
        .get::<Building>(e)
        .map(|building| building.proximity.clone())
        .unwrap_or_default();
    let mut targets: SmallVec<[Entity; 6]> = SmallVec::new();
    for other in proximity {
        let same = world
            .get::<Building>(other)
            .is_some_and(|b| b.block == block)
            && world.get::<TeamComp>(other).is_some_and(|t| t.team == team)
            && is_wall_state(world, other);
        if same {
            targets.push(other);
        }
    }
    for target in targets {
        update_autotile_bits(world, target);
    }
}

/// `Door` behavior (`Door.DoorBuild`).
#[derive(Debug, Default, Clone, Copy)]
pub struct DoorBehavior;

impl BuildingBehavior for DoorBehavior {
    fn logic_sense(
        &self,
        world: &World,
        e: Entity,
        a: crate::logic::LAccess,
    ) -> Option<crate::logic::access::Sensed> {
        use crate::logic::LAccess;
        use crate::logic::access::Sensed;
        Some(match a {
            // `Door.sense(enabled)` (`Door.java:88-91`).
            LAccess::Enabled => Sensed::Num(if world.get::<DoorState>(e)?.open {
                1.0
            } else {
                0.0
            }),
            _ => return None,
        })
    }
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<DoorState>(e).is_none() {
            world.entity_mut(e).insert(DoorState::default());
        }
    }

    /// `DoorBuild.checkSolid()` (`Door.java:142-145`): `!open`.
    fn check_solid(&self, world: &World, e: Entity) -> bool {
        world.get::<DoorState>(e).is_some_and(|state| !state.open)
    }

    fn on_proximity_added(&self, world: &mut World, e: Entity) {
        update_chained(world, e);
    }

    fn on_proximity_removed(&self, world: &mut World, e: Entity) {
        let neighbors: SmallVec<[Entity; 6]> = world
            .get::<Building>(e)
            .map(|building| building.proximity.clone())
            .unwrap_or_default();
        for other in neighbors {
            if world.get::<DoorState>(other).is_some() {
                update_chained(world, other);
            }
        }
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        world
            .get::<DoorState>(e)
            .map(|state| ConfigValue::Bool(state.open))
            .unwrap_or(ConfigValue::None)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        let open = match value {
            ConfigValue::Bool(open) => open,
            ConfigValue::None => false,
            _ => return,
        };
        // `Door`'s config body (`Door.java:38-58`) has no throttle; the 60-tick
        // `timerToggle` gate belongs to `DoorBuild.tapped` only (see
        // [`tap_allowed`]). The body emits the sound + ring effect keyed on the
        // state BEFORE the change, then applies it.
        let was_open = world.get::<DoorState>(e).is_some_and(|state| state.open);
        set_open(world, e, open);
        // `Door.java:39-42`: `doorSound.at(base)` + `base.effect()` on a live
        // config application. Java gates on `!world.isGenerating()`; the port
        // never reaches this path from map/save reads (they write `DoorState`
        // directly).
        if let Some((x, y)) = crate::audio::sim::building_centre_of(world, e) {
            crate::audio::sim::emit_world_sound(
                world,
                crate::audio::SoundId::DOOR,
                x,
                y,
                1.0,
                1.0,
                0.0,
            );
            emit_door_fx(world, e, was_open, x, y);
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        if let Some(state) = world.get::<DoorState>(e) {
            w.b(if state.open { 1 } else { 0 });
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, _revision: u8) {
        let open = r.ub().map(|byte| byte != 0).unwrap_or(false);
        if let Some(mut state) = world.get_mut::<DoorState>(e) {
            state.open = open;
        }
    }
}

/// Sets `open` on a door and every door chained to it (`DoorBuild` config body).
///
/// `Door.java:49-53`: a door with units standing in its footprint is skipped
/// while closing (`Units.anyEntities(entity.tile) && !open`), and doors already
/// in the requested state are skipped.
pub fn set_open(world: &mut World, e: Entity, open: bool) {
    for door in chain_of(world, e) {
        let already = world
            .get::<DoorState>(door)
            .is_some_and(|state| state.open == open);
        if already || (!open && unit_on_footprint(world, door)) {
            continue;
        }
        if let Some(mut state) = world.get_mut::<DoorState>(door) {
            state.open = open;
        }
        // `pathfinder.updateTile(entity.tile)` (Door.java:56) is the sim's
        // `world::sync_dynamic_solid` + unit-runtime refresh (`Sim::configure`).
    }
}

/// `Units.anyEntities(tile)` (`Units.java:170-178`, `ground = true`): whether a
/// grounded non-leg-step unit's hitbox overlaps the building's footprint
/// square.
fn unit_on_footprint(world: &World, entity: Entity) -> bool {
    let Some((cx, cy)) = crate::audio::sim::building_centre_of(world, entity) else {
        return false;
    };
    let Some(size) = world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| {
            world
                .get::<crate::entities::comp::Building>(entity)
                .and_then(|building| table.get(building.block))
        })
        .map(|instance| instance.def.size)
    else {
        return false;
    };
    let ts = crate::config::TILESIZE as f32;
    let half = size as f32 * ts / 2.0;
    let (left, top) = (cx - half, cy - half);
    world
        .iter_entities()
        .filter_map(|entity_ref| {
            entity_ref.get::<crate::entities::comp::Unit>()?;
            if entity_ref
                .get::<crate::entities::comp::unit::comp::BlockUnitComp>()
                .is_some()
            {
                return None;
            }
            let pos = entity_ref.get::<crate::entities::comp::Pos>()?;
            Some((entity_ref.id(), *pos))
        })
        .any(|(unit, pos)| {
            // `(unit.isGrounded() && !unit.type.allowLegStep) == ground`.
            if crate::combat::bullet::unit_is_flying(world, unit) {
                return false;
            }
            let leg_step = world
                .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
                .and_then(|runtime| {
                    world
                        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(unit)
                        .and_then(|comp| runtime.content.unit(comp.type_id))
                        .map(|def| def.allow_leg_step)
                })
                .unwrap_or(false);
            if leg_step {
                return false;
            }
            let hit = world
                .get::<crate::entities::comp::unit::comp::HitboxComp>(unit)
                .map(|hitbox| hitbox.hit_size / 2.0)
                .unwrap_or(0.0);
            pos.x - hit < left + size as f32 * ts
                && pos.x + hit > left
                && pos.y - hit < top + size as f32 * ts
                && pos.y + hit > top
        })
}

fn chain_of(world: &World, e: Entity) -> SmallVec<[Entity; 6]> {
    world
        .get::<DoorState>(e)
        .map(|state| state.chained.clone())
        .filter(|chain| !chain.is_empty())
        .unwrap_or_else(|| SmallVec::from_iter([e]))
}

/// `DoorBuild.origin()` (`Door.java:106-108`): the first door of the chain.
fn origin_of(world: &World, e: Entity) -> Entity {
    world
        .get::<DoorState>(e)
        .and_then(|state| state.chained.first().copied())
        .unwrap_or(e)
}

/// `DoorBuild.tapped`'s timer gate (`Door.java:148-154`):
/// `origin().timer(timerToggle, 60f)`. Arc's `timer` consumes the slot when it
/// fires, so a tap is rejected for 60 ticks after the last accepted one. The
/// caller checks the `Units.anyEntities(tile) && open` clause first (Java
/// short-circuit order: a unit in an open door must not consume the slot).
pub fn tap_allowed(world: &mut World, e: Entity) -> bool {
    let origin = origin_of(world, e);
    let now = crate::world::update::build_time(world);
    let Some(mut state) = world.get_mut::<DoorState>(origin) else {
        return false;
    };
    if state.last_toggle.is_some_and(|last| now - last < 60.0) {
        return false;
    }
    state.last_toggle = Some(now);
    true
}

/// `DoorBuild.effect()` (`Door.java:110-112`): `(open ? closefx : openfx)
/// .at(this, size)` through the live unit runtime's FX sink. `door-large`
/// swaps in the `*large` pair (`Blocks.java:1794-1796`; the vanilla `Door`s are
/// the only configured doors, so `size > 1` covers both).
fn emit_door_fx(world: &World, e: Entity, was_open: bool, x: f32, y: f32) {
    use crate::content::registries::fx_meta::EffectRef;
    let Some(fx) = world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .map(|runtime| runtime.fx.clone())
    else {
        return;
    };
    let Some(size) = world
        .get::<Building>(e)
        .and_then(|building| world.get_resource::<BlockTable>()?.get(building.block))
        .map(|instance| instance.def.size)
    else {
        return;
    };
    let id = match (was_open, size > 1) {
        (true, false) => crate::content::EffectId::DOORCLOSE,
        (false, false) => crate::content::EffectId::DOOROPEN,
        (true, true) => crate::content::EffectId::DOORCLOSELARGE,
        (false, true) => crate::content::EffectId::DOOROPENLARGE,
    };
    fx.effect(
        &EffectRef::Named(id),
        x,
        y,
        size as f32,
        crate::content::Rgba::WHITE,
    );
}

/// Rebuilds a door's connectivity group (`DoorBuild.updateChained`).
pub fn update_chained(world: &mut World, e: Entity) {
    let mut chain: SmallVec<[Entity; 6]> = SmallVec::new();
    let mut stack: SmallVec<[Entity; 6]> = SmallVec::from_iter([e]);
    while let Some(next) = stack.pop() {
        if chain.contains(&next) {
            continue;
        }
        chain.push(next);
        if let Some(building) = world.get::<Building>(next) {
            for neighbor in building.proximity.clone() {
                if !chain.contains(&neighbor) && world.get::<DoorState>(neighbor).is_some() {
                    stack.push(neighbor);
                }
            }
        }
    }
    for door in &chain {
        if let Some(mut state) = world.get_mut::<DoorState>(*door) {
            state.chained = chain.clone();
        }
    }
}

/// `AutoDoor` behavior (`AutoDoor.AutoDoorBuild`).
#[derive(Debug, Default, Clone, Copy)]
pub struct AutoDoorBehavior;

/// `AutoDoor.checkInterval`.
pub const AUTO_DOOR_CHECK_INTERVAL: f32 = 20.0;

impl BuildingBehavior for AutoDoorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<DoorState>(e).is_none() {
            world.entity_mut(e).insert(DoorState::default());
        }
    }

    /// `AutoDoor extends Door`: inherits `DoorBuild.checkSolid()` (`!open`).
    fn check_solid(&self, world: &World, e: Entity) -> bool {
        world.get::<DoorState>(e).is_some_and(|state| !state.open)
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        // `timer(timerToggle, checkInterval)`: with no unit tree yet the
        // `shouldOpen` probe is always false, so the door stays closed. The
        // timer still runs so plan 11 can hook the unit query.
        let _ = crate::world::update::run_timer(world, e, 0, AUTO_DOOR_CHECK_INTERVAL);
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        world
            .get::<DoorState>(e)
            .map(|state| ConfigValue::Bool(state.open))
            .unwrap_or(ConfigValue::None)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        let open = match value {
            ConfigValue::Bool(open) => open,
            ConfigValue::None => false,
            _ => return,
        };
        if let Some(mut state) = world.get_mut::<DoorState>(e) {
            state.open = open;
        }
        // `AutoDoor.java:82` (`setOpen`): `doorSound.at(this)` on a toggle.
        if let Some((x, y)) = crate::audio::sim::building_centre_of(world, e) {
            crate::audio::sim::emit_world_sound(
                world,
                crate::audio::SoundId::DOOR,
                x,
                y,
                1.0,
                1.0,
                0.0,
            );
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        if let Some(state) = world.get::<DoorState>(e) {
            w.b(if state.open { 1 } else { 0 });
        }
    }

    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, _revision: u8) {
        let open = r.ub().map(|byte| byte != 0).unwrap_or(false);
        if let Some(mut state) = world.get_mut::<DoorState>(e) {
            state.open = open;
        }
    }
}

/// `Radar` behavior (`Radar.RadarBuild`): discovery progress is plan 11.
#[derive(Debug, Default, Clone, Copy)]
pub struct RadarBehavior;

impl BuildingBehavior for RadarBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<RadarState>(e).is_none() {
            world.entity_mut(e).insert(RadarState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        // `Radar.updateTile` (`Radar.java:59-72`): ramp the efficiency, force a
        // fog update when the radius moved by >= 0.5, grow the discovery
        // progress and the rotation accumulator.
        let (def_fog_radius, discovery_time, efficiency, edelta) = {
            let Some(building) = world.get::<Building>(e) else {
                return;
            };
            let Some(table) = world.get_resource::<BlockTable>() else {
                return;
            };
            let Some(inst) = table.get(building.block) else {
                return;
            };
            let discovery_time = match &inst.kind_data {
                crate::world::block_kind_data::BlockKindData::Radar(radar) => radar.discovery_time,
                _ => 600.0,
            };
            (
                inst.def.fog_radius as f32,
                discovery_time,
                building.efficiency,
                crate::world::update::edelta(world, e),
            )
        };
        let force = {
            let Some(mut state) = world.get_mut::<RadarState>(e) else {
                return;
            };
            // `Mathf.lerpDelta(smoothEfficiency, efficiency, 0.05f)`: Arc clamps
            // `progress * Time.delta` with `Time.delta = 1` at 60 Hz.
            state.smooth_efficiency = lerp(state.smooth_efficiency, efficiency, 0.05);
            let fog_radius = def_fog_radius * state.progress * state.smooth_efficiency;
            let force = (fog_radius - state.last_radius).abs() >= 0.5;
            if force {
                state.last_radius = fog_radius;
            }
            state.progress =
                (state.progress + edelta / discovery_time.max(f32::EPSILON)).clamp(0.0, 1.0);
            state.total_progress += efficiency * edelta;
            force
        };
        if force {
            crate::sim::fog::fog_force_update(world, e);
        }
    }
    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        w.f(world
            .get::<RadarState>(e)
            .map(|s| s.progress)
            .unwrap_or_default());
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        let progress = r.f().unwrap_or(0.0);
        if let Some(mut state) = world.get_mut::<RadarState>(e) {
            state.progress = progress;
        }
    }
}

/// `Thruster` behavior (`Thruster.ThrusterBuild`): entity force is plan 11.
#[derive(Debug, Default, Clone, Copy)]
pub struct ThrusterBehavior;

impl BuildingBehavior for ThrusterBehavior {
    fn always_update_when_disabled(&self) -> bool {
        true
    }
}

/// `TargetDummy` behavior (`TargetDummy.TargetDummyBuild`): tethering is plan 11.
#[derive(Debug, Default, Clone, Copy)]
pub struct TargetDummyBehavior;

impl BuildingBehavior for TargetDummyBehavior {}

// --- G04 T3a: per-kind save IO -------------------------------------------------
// Java body: Radar.java:98-109 (`f progress`).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::io::wire::{WireReader, WireWriter};
    use crate::world::TilePos;
    use crate::world::block::BlockTable;
    use crate::world::limits::BuildRules;
    use bevy_ecs::world::World as EcsWorld;

    fn spawn_named(
        world: &mut EcsWorld,
        table: &BlockTable,
        content: &crate::content::ContentRegistry,
        name: &str,
        seq: u64,
        x: i16,
        y: i16,
    ) -> Entity {
        let inst = table.get_named(name).expect(name).clone();
        let entity = inst.spawn(
            world,
            seq,
            TilePos::new(x, y),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        inst.behavior.create_state(world, entity);
        entity
    }

    fn setup() -> (EcsWorld, BlockTable, crate::content::ContentRegistry) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let mut world = EcsWorld::new();
        world.insert_resource(BuildRules::default());
        (world, table, content)
    }

    #[test]
    fn door_config_roundtrips_open_state() {
        let (mut world, table, content) = setup();
        let entity = spawn_named(&mut world, &table, &content, "door", 0, 4, 4);
        world.insert_resource(table);
        assert_eq!(
            DoorBehavior.config(&world, entity),
            ConfigValue::Bool(false)
        );
        DoorBehavior.configured(&mut world, entity, None, ConfigValue::Bool(true));
        assert_eq!(DoorBehavior.config(&world, entity), ConfigValue::Bool(true));
        let mut bytes = Vec::new();
        {
            let mut w = WireWriter::new(&mut bytes);
            DoorBehavior.write(&world, entity, &mut w);
        }
        assert_eq!(bytes, vec![1u8]);
        let mut r = WireReader::new(&[0u8]);
        DoorBehavior.read(&mut world, entity, &mut r, 0);
        assert_eq!(
            DoorBehavior.config(&world, entity),
            ConfigValue::Bool(false)
        );
    }

    #[test]
    fn door_chain_propagates_open_state() {
        let (mut world, table, content) = setup();
        let first = spawn_named(&mut world, &table, &content, "door", 0, 4, 4);
        let second = spawn_named(&mut world, &table, &content, "door", 1, 5, 4);
        world.insert_resource(table);
        world
            .get_mut::<Building>(first)
            .unwrap()
            .proximity
            .push(second);
        world
            .get_mut::<Building>(second)
            .unwrap()
            .proximity
            .push(first);
        update_chained(&mut world, first);
        update_chained(&mut world, second);
        set_open(&mut world, first, true);
        assert_eq!(DoorBehavior.config(&world, first), ConfigValue::Bool(true));
        assert_eq!(DoorBehavior.config(&world, second), ConfigValue::Bool(true));
    }

    #[test]
    fn wall_autotile_bits_track_same_block_neighbors() {
        let (mut world, table, content) = setup();
        let first = spawn_named(&mut world, &table, &content, "copper-wall", 0, 4, 4);
        let second = spawn_named(&mut world, &table, &content, "copper-wall", 1, 5, 4);
        world.insert_resource(table);
        world
            .get_mut::<Building>(first)
            .unwrap()
            .proximity
            .push(second);
        update_autotile_bits(&mut world, first);
        // East neighbor => bit 0.
        assert_eq!(world.get::<WallState>(first).unwrap().autotile_bits, 1);
        assert_eq!(D8.len(), 8);
    }

    #[test]
    fn door_config_applies_without_tap_gate() {
        // `Door`'s config body (`Door.java:38-58`) has no `timerToggle` gate;
        // only `DoorBuild.tapped` throttles (`Door.java:149`).
        let (mut world, table, content) = setup();
        let entity = spawn_named(&mut world, &table, &content, "door", 0, 4, 4);
        world.insert_resource(table);
        DoorBehavior.configured(&mut world, entity, None, ConfigValue::Bool(true));
        DoorBehavior.configured(&mut world, entity, None, ConfigValue::Bool(false));
        assert_eq!(
            DoorBehavior.config(&world, entity),
            ConfigValue::Bool(false)
        );
    }

    #[test]
    fn door_tap_timer_gates_and_records() {
        // `origin().timer(timerToggle, 60f)` (`Door.java:149`): fires at most
        // once per 60 ticks and consumes the slot.
        let (mut world, table, content) = setup();
        let entity = spawn_named(&mut world, &table, &content, "door", 0, 4, 4);
        world.insert_resource(table);
        world.insert_resource(crate::world::update::BuildClock { time: 100.0 });
        assert!(tap_allowed(&mut world, entity));
        assert!(!tap_allowed(&mut world, entity));
        world
            .get_resource_mut::<crate::world::update::BuildClock>()
            .expect("clock")
            .time = 159.0;
        assert!(!tap_allowed(&mut world, entity));
        world
            .get_resource_mut::<crate::world::update::BuildClock>()
            .expect("clock")
            .time = 160.0;
        assert!(tap_allowed(&mut world, entity));
    }
}
