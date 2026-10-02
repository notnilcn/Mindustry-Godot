// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MassDriver`/`PayloadMassDriver` core driver behavior
//! (`world/blocks/distribution/MassDriver.java`) — plan 08 M3.
//!
//! Ports the `DriverState` machine, the ordered `waitingShooters` queue, the
//! reload reel, link/config handling and the `DriverBulletData` item transfer.
//! The physical bolt is plan 10 (`MassDriverBolt`); 08 fires through the frozen
//! [`MassDriverPayloadCarrier`] seam (plan 08 L7). The default [`TestBoltCarrier`]
//! hands the data to plan 10's `BulletData::MassDriver` slot when a bullet
//! spawner is registered; otherwise it uses the deterministic in-engine
//! [`MassDriverBolts`] queue (arrival timed by plan 08's `BuildClock`).

use std::sync::Arc;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::ItemId;
use crate::entities::comp::{Building, Pos};
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::config::ConfigValue;
use crate::world::modules::ItemModule;
use crate::world::update::{build_time, edelta};
use crate::world::{TileBuilds, TilePos};

/// `MassDriver.DriverState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Default)]
pub enum DriverState {
    /// Nothing is shooting at this driver and it has no target (`idle`).
    #[default]
    Idle,
    /// Currently being shot at; unload items (`accepting`).
    Accepting,
    /// Has a valid link and items to fire (`shooting`).
    Shooting,
}

/// `MassDriver.DriverBulletData` (component so plan 10's `BulletData::MassDriver`
/// can carry it; the `items` array is indexed by item id).
#[derive(Debug, Clone, Component)]
pub struct DriverBulletData {
    /// Source driver (`from`).
    pub from: Entity,
    /// Destination driver (`to`).
    pub to: Entity,
    /// Per-item amounts in flight (`items`).
    pub items: Vec<i32>,
}

/// `MassDriver.MassDriverBuild` state.
#[derive(Debug, Clone, Component)]
pub struct MassDriverBuild {
    /// Packed destination tile pos or `-1` (`link`).
    pub link: i32,
    /// Turret rotation in degrees (`rotation`).
    pub rotation: f32,
    /// Reload countdown `0..1` (`reloadCounter`).
    pub reload_counter: f32,
    /// Driver state.
    pub state: DriverState,
    /// Shooters queued to fire at this driver, insertion-ordered
    /// (`waitingShooters`).
    pub waiting_shooters: SmallVec<[Entity; 4]>,
}

impl Default for MassDriverBuild {
    fn default() -> Self {
        Self {
            link: -1,
            rotation: 90.0,
            reload_counter: 0.0,
            state: DriverState::Idle,
            waiting_shooters: SmallVec::new(),
        }
    }
}

/// Fires a `DriverBulletData` entity at its target after `travel_ticks`.
pub trait MassDriverPayloadCarrier: Send + Sync {
    /// Schedules delivery of `data` (whose `to` field is the receiver).
    fn fire(&self, world: &mut World, data: Entity, travel_ticks: f32);
}

/// Registered carrier (plan 10 swaps in the real `MassDriverBolt`).
#[derive(Resource)]
pub struct MassDriverCarrier(pub Arc<dyn MassDriverPayloadCarrier>);

impl Default for MassDriverCarrier {
    fn default() -> Self {
        Self(Arc::new(TestBoltCarrier))
    }
}

/// In-engine fallback bolt queue (`MassDriverBolts`).
#[derive(Debug, Default, Resource)]
pub struct MassDriverBolts {
    /// `(data entity, arrival BuildClock time)` pairs.
    pub bolts: Vec<(Entity, f32)>,
}

/// Headless/`TestBolt` carrier: stores the bolt until its arrival tick.
#[derive(Debug, Clone, Copy, Default)]
pub struct TestBoltCarrier;

impl MassDriverPayloadCarrier for TestBoltCarrier {
    fn fire(&self, world: &mut World, data: Entity, travel_ticks: f32) {
        let arrive = build_time(world) + travel_ticks.max(0.0);
        if !world.contains_resource::<MassDriverBolts>() {
            world.insert_resource(MassDriverBolts::default());
        }
        world
            .resource_mut::<MassDriverBolts>()
            .bolts
            .push((data, arrive));
    }
}

/// `MassDriver` behavior (`mass-driver`).
#[derive(Debug, Clone, Copy)]
pub struct MassDriverBehavior {
    /// `MassDriver.range` (world pixels).
    pub range: f32,
    /// `MassDriver.rotateSpeed` (degrees/tick at full efficiency).
    pub rotate_speed: f32,
    /// `MassDriver.minDistribute`.
    pub min_distribute: i32,
    /// `MassDriver.reload` (ticks).
    pub reload: f32,
    /// `MassDriver.bulletSpeed` (pixels/tick).
    pub bullet_speed: f32,
    /// `MassDriver.bulletLifetime` (ticks).
    pub bullet_lifetime: f32,
}

impl MassDriverBehavior {
    /// Vanilla `mass-driver`.
    pub const VANILLA: MassDriverBehavior = MassDriverBehavior {
        range: 440.0,
        rotate_speed: 5.0,
        min_distribute: 10,
        reload: 200.0,
        bullet_speed: 5.5,
        bullet_lifetime: 200.0,
    };
}

fn tile_entity(world: &World, packed: i32) -> Option<Entity> {
    let (x, y) = crate::world::pos::unpack(packed);
    world.get_resource::<TileBuilds>()?.get(x as i32, y as i32)
}

fn tile_pack(world: &World, e: Entity) -> i32 {
    world
        .get::<Building>(e)
        .map(|b| b.tile.pack())
        .unwrap_or(-1)
}

fn world_dst(world: &World, a: Entity, b: Entity) -> f32 {
    let (Some(pa), Some(pb)) = (world.get::<Pos>(a), world.get::<Pos>(b)) else {
        return f32::MAX;
    };
    let dx = pb.x - pa.x;
    let dy = pb.y - pa.y;
    (dx * dx + dy * dy).sqrt()
}

fn angle_to(world: &World, a: Entity, b: Entity) -> f32 {
    let (Some(pa), Some(pb)) = (world.get::<Pos>(a), world.get::<Pos>(b)) else {
        return 0.0;
    };
    (pb.y - pa.y).atan2(pb.x - pa.x).to_degrees()
}

fn near(a: f32, b: f32, eps: f32) -> bool {
    ((b - a + 180.0).rem_euclid(360.0) - 180.0).abs() < eps
}
fn move_toward(current: f32, target: f32, speed: f32) -> f32 {
    let delta = ((target - current + 180.0).rem_euclid(360.0)) - 180.0;
    current + delta.clamp(-speed, speed)
}

impl MassDriverBehavior {
    /// `MassDriverBuild.linkValid()`.
    fn link_valid(&self, world: &World, e: Entity) -> bool {
        let link = world
            .get::<MassDriverBuild>(e)
            .map(|b| b.link)
            .unwrap_or(-1);
        if link == -1 {
            return false;
        }
        let Some(other) = tile_entity(world, link) else {
            return false;
        };
        let (Some(a), Some(b)) = (world.get::<Building>(e), world.get::<Building>(other)) else {
            return false;
        };
        let same_block = a.block == b.block;
        let same_team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .zip(world.get::<crate::entities::comp::TeamComp>(other))
            .is_some_and(|(x, y)| x.team == y.team);
        same_block && same_team && world_dst(world, e, other) <= self.range
    }

    /// `MassDriverBuild.shooterValid(other)`.
    fn shooter_valid(&self, world: &World, e: Entity, other: Entity) -> bool {
        let Some(shooter) = world.get::<MassDriverBuild>(other) else {
            return false;
        };
        let same_block = world
            .get::<Building>(e)
            .zip(world.get::<Building>(other))
            .is_some_and(|(a, b)| a.block == b.block);
        let efficiency = world
            .get::<Building>(other)
            .map(|b| b.efficiency)
            .unwrap_or(0.0);
        shooter.link == tile_pack(world, e)
            && same_block
            && efficiency > 0.0
            && world_dst(world, e, other) <= self.range
    }

    fn current_shooter(&self, world: &World, e: Entity) -> Option<Entity> {
        world
            .get::<MassDriverBuild>(e)
            .and_then(|b| b.waiting_shooters.first().copied())
    }

    /// Delivers every in-engine bolt that has arrived at `e`.
    fn deliver_due(&self, world: &mut World, e: Entity) {
        if !world.contains_resource::<MassDriverBolts>() {
            return;
        }
        let now = build_time(world);
        let bolts = std::mem::take(&mut world.resource_mut::<MassDriverBolts>().bolts);
        let mut kept = Vec::with_capacity(bolts.len());
        for (data, arrive) in bolts {
            let belongs = world
                .get::<DriverBulletData>(data)
                .is_some_and(|d| d.to == e);
            if belongs && arrive <= now {
                deliver_bolt(world, data);
            } else {
                kept.push((data, arrive));
            }
        }
        world.resource_mut::<MassDriverBolts>().bolts = kept;
    }
}

/// `handlePayload(bullet, data)`: add the in-flight items to `to` (up to
/// `itemCapacity * 2`), reset its reload and clear the shooter queue.
pub fn handle_payload(world: &mut World, to: Entity, data_entity: Entity) {
    let Some(data) = world.get::<DriverBulletData>(data_entity).cloned() else {
        return;
    };
    let cap = world
        .get_resource::<BlockTable>()
        .and_then(|table| world.get::<Building>(to).map(|b| (table, b.block)))
        .and_then(|(table, block)| table.get(block).map(|inst| inst.def.item_capacity))
        .unwrap_or(0);
    let mut total = world.get::<ItemModule>(to).map(|m| m.total).unwrap_or(0);
    for (index, amount) in data.items.iter().enumerate() {
        if *amount <= 0 {
            continue;
        }
        let max_add = (*amount).min(cap * 2 - total).max(0);
        if max_add <= 0 {
            continue;
        }
        if let Some(mut items) = world.get_mut::<ItemModule>(to) {
            items.add(ItemId::new(index as u16), max_add, cap * 2);
        }
        total += max_add;
        if total >= cap * 2 {
            break;
        }
    }
    // `other.waitingShooters.remove(this); other.state = idle;`.
    if let Some(mut driver) = world.get_mut::<MassDriverBuild>(to) {
        driver.waiting_shooters.retain(|s| *s != data.from);
        driver.state = DriverState::Idle;
        driver.reload_counter = 1.0;
    }
    let _ = world.despawn(data_entity);
}

fn deliver_bolt(world: &mut World, data_entity: Entity) {
    let Some(to) = world.get::<DriverBulletData>(data_entity).map(|d| d.to) else {
        let _ = world.despawn(data_entity);
        return;
    };
    handle_payload(world, to, data_entity);
}

impl BuildingBehavior for MassDriverBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<MassDriverBuild>(e).is_none() {
            world.entity_mut(e).insert(MassDriverBuild::default());
        }
        if !world.contains_resource::<MassDriverCarrier>() {
            world.insert_resource(MassDriverCarrier::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        self.deliver_due(world, e);
        let Some(mut build) = world.get::<MassDriverBuild>(e).cloned() else {
            return;
        };
        let has_link = self.link_valid(world, e);

        // Reload regardless of state.
        if build.reload_counter > 0.0 {
            build.reload_counter =
                (build.reload_counter - edelta(world, e) / self.reload).clamp(0.0, 1.0);
        }

        // Clean up invalid shooters.
        if let Some(current) = build.waiting_shooters.first().copied()
            && !self.shooter_valid(world, e, current)
        {
            build.waiting_shooters.retain(|s| *s != current);
        }

        let item_capacity = world
            .get_resource::<BlockTable>()
            .and_then(|table| world.get::<Building>(e).map(|b| (table, b.block)))
            .and_then(|(table, block)| table.get(block).map(|inst| inst.def.item_capacity))
            .unwrap_or(0);
        let total = world.get::<ItemModule>(e).map(|m| m.total).unwrap_or(0);
        let efficiency = world
            .get::<Building>(e)
            .map(|b| b.efficiency)
            .unwrap_or(0.0);

        // State switch.
        if build.state == DriverState::Idle {
            if !build.waiting_shooters.is_empty() && item_capacity - total >= self.min_distribute {
                build.state = DriverState::Accepting;
            } else if has_link {
                build.state = DriverState::Shooting;
            }
        }

        // Dump when idle or accepting.
        if matches!(build.state, DriverState::Idle | DriverState::Accepting) {
            crate::world::blocks::distribution::transfer::dump_accumulate(world, e, None);
        }

        if efficiency <= 0.0 {
            world.entity_mut(e).insert(build);
            return;
        }

        if build.state == DriverState::Accepting {
            let current = self.current_shooter(world, e);
            let Some(current) = current else {
                build.state = DriverState::Idle;
                world.entity_mut(e).insert(build);
                return;
            };
            if item_capacity - total < self.min_distribute {
                build.state = DriverState::Idle;
                world.entity_mut(e).insert(build);
                return;
            }
            build.rotation = move_toward(
                build.rotation,
                angle_to(world, e, current),
                self.rotate_speed * efficiency,
            );
        } else if build.state == DriverState::Shooting {
            let link = build.link;
            let target = tile_entity(world, link);
            let target_ok = has_link && target.is_some();
            if !target_ok
                || (!build.waiting_shooters.is_empty()
                    && item_capacity - total >= self.min_distribute)
            {
                build.state = DriverState::Idle;
                world.entity_mut(e).insert(build);
                return;
            }
            let Some(target) = target else {
                build.state = DriverState::Idle;
                world.entity_mut(e).insert(build);
                return;
            };
            let link_cap = world
                .get_resource::<BlockTable>()
                .and_then(|table| world.get::<Building>(target).map(|b| (table, b.block)))
                .and_then(|(table, block)| table.get(block).map(|inst| inst.def.item_capacity))
                .unwrap_or(0);
            let link_total = world
                .get::<ItemModule>(target)
                .map(|m| m.total)
                .unwrap_or(0);
            let target_rotation = angle_to(world, e, target);
            if total >= self.min_distribute && link_cap - link_total >= self.min_distribute {
                // Add this driver to the target's ordered shooter queue.
                if let Some(mut other) = world.get_mut::<MassDriverBuild>(target)
                    && !other.waiting_shooters.contains(&e)
                {
                    other.waiting_shooters.push(e);
                }
                if build.reload_counter <= 0.0001 {
                    build.rotation = move_toward(
                        build.rotation,
                        target_rotation,
                        self.rotate_speed * efficiency,
                    );
                    let first = world
                        .get::<MassDriverBuild>(target)
                        .and_then(|b| b.waiting_shooters.first().copied());
                    let other_state = world.get::<MassDriverBuild>(target).map(|b| b.state);
                    let other_rotation = world.get::<MassDriverBuild>(target).map(|b| b.rotation);
                    if first == Some(e)
                        && other_state == Some(DriverState::Accepting)
                        && near(build.rotation, target_rotation, 2.0)
                        && other_rotation.is_some_and(|r| near(r, target_rotation + 180.0, 2.0))
                    {
                        self.fire(world, e, target, item_capacity);
                        build.state = DriverState::Idle;
                    }
                }
            }
        }

        world.entity_mut(e).insert(build);
    }

    fn accept_item(&self, world: &World, e: Entity, _source: Entity, _item: ItemId) -> bool {
        let capacity = world
            .get_resource::<BlockTable>()
            .and_then(|table| world.get::<Building>(e).map(|b| (table, b.block)))
            .and_then(|(table, block)| table.get(block).map(|inst| inst.def.item_capacity))
            .unwrap_or(0);
        let total = world.get::<ItemModule>(e).map(|m| m.total).unwrap_or(0);
        total < capacity && self.link_valid(world, e)
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        let (link, tile) = match (world.get::<MassDriverBuild>(e), world.get::<Building>(e)) {
            (Some(driver), Some(building)) => (driver.link, building.tile),
            _ => return ConfigValue::None,
        };
        let (lx, ly) = crate::world::pos::unpack(link);
        ConfigValue::Point2(lx as i32 - tile.x() as i32, ly as i32 - tile.y() as i32)
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        let tile = world.get::<Building>(e).map(|b| b.tile);
        let link = match (value, tile) {
            (ConfigValue::Point2(x, y), Some(tile)) => {
                TilePos::new((tile.x() as i32 + x) as i16, (tile.y() as i32 + y) as i16).pack()
            }
            (ConfigValue::Number(n), _) => n as i32,
            _ => -1,
        };
        if let Some(mut driver) = world.get_mut::<MassDriverBuild>(e) {
            driver.link = link;
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        0
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let driver = world.get::<MassDriverBuild>(e).cloned().unwrap_or_default();
        w.i(driver.link);
        w.f(driver.rotation);
        w.b(driver.state as u8 as i8);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        let mut driver = world.get::<MassDriverBuild>(e).cloned().unwrap_or_default();
        if let Ok(link) = r.i() {
            driver.link = link;
        }
        if let Ok(rotation) = r.f() {
            driver.rotation = rotation;
        }
        if let Ok(state) = r.b() {
            driver.state = match state {
                1 => DriverState::Accepting,
                2 => DriverState::Shooting,
                _ => DriverState::Idle,
            };
        }
        world.entity_mut(e).insert(driver);
    }
}

impl MassDriverBehavior {
    /// `MassDriverBuild.fire(target)`: move the items into a `DriverBulletData`
    /// entity and hand it to the registered carrier.
    fn fire(&self, world: &mut World, e: Entity, target: Entity, item_capacity: i32) {
        if let Some(mut driver) = world.get_mut::<MassDriverBuild>(e) {
            driver.reload_counter = 1.0;
        }
        let item_count = world
            .get::<ItemModule>(e)
            .map(|m| m.items.len())
            .unwrap_or(0);
        let mut items = vec![0i32; item_count];
        let mut total_used = 0;
        for (index, slot) in items.iter_mut().enumerate() {
            let amount = world
                .get::<ItemModule>(e)
                .map(|m| m.items.get(index).copied().unwrap_or(0))
                .unwrap_or(0);
            let max_transfer = amount.min(item_capacity - total_used).max(0);
            *slot = max_transfer;
            total_used += max_transfer;
        }
        if let Some(mut module) = world.get_mut::<ItemModule>(e) {
            for (index, amount) in items.iter().enumerate() {
                if *amount > 0 {
                    module.remove(ItemId::new(index as u16), *amount);
                }
            }
        }
        let data = world
            .spawn(DriverBulletData {
                from: e,
                to: target,
                items,
            })
            .id();
        let distance = world_dst(world, e, target);
        let time_scale = world
            .get::<Building>(e)
            .map(|b| b.time_scale)
            .unwrap_or(1.0);
        let travel =
            (self.bullet_lifetime / time_scale).min(distance / (self.bullet_speed * time_scale));
        let carrier = world
            .get_resource::<MassDriverCarrier>()
            .map(|c| c.0.clone())
            .unwrap_or_else(|| Arc::new(TestBoltCarrier));
        carrier.fire(world, data, travel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::clock::SimClock;
    use crate::world::BuildHarness;
    use crate::world::limits::BuildRules;

    fn assume_power(harness: &mut BuildHarness) {
        if let Some(mut rules) = harness.world.get_resource_mut::<BuildRules>() {
            rules.cheat = true;
        }
    }

    #[test]
    fn state_machine_and_waiting_queue() {
        let mut harness = BuildHarness::new(64, 64, 7);
        assume_power(&mut harness);
        let driver = harness
            .content()
            .block_id("mass-driver")
            .expect("mass-driver");
        assert!(harness.place(8, 8, driver, 0, true));
        assert!(harness.place(20, 8, driver, 0, true));
        assert!(harness.configure(8, 8, ConfigValue::Point2(12, 0)));
        // Inject enough items to fire.
        let copper = harness.content().item_id("copper").expect("copper");
        let from = harness.build_at(8, 8).expect("from");
        harness
            .world
            .get_mut::<ItemModule>(from)
            .expect("items")
            .add(copper, 120, 120);
        for _ in 0..600 {
            harness.tick();
        }
        let to = harness.build_at(20, 8).expect("to");
        let received = harness
            .world
            .get::<ItemModule>(to)
            .map(|m| m.total)
            .unwrap_or(0);
        assert_eq!(received, 120, "receiver total");
        let to_state = harness.world.get::<MassDriverBuild>(to).map(|b| b.state);
        assert_eq!(to_state, Some(DriverState::Idle));
        // All bolt entities delivered and despawned.
        let live = harness
            .world
            .iter_entities()
            .filter(|e| e.get::<DriverBulletData>().is_some())
            .count();
        assert_eq!(live, 0, "pool must have no live bolts");
    }

    #[test]
    fn sim_clock_run_callback_is_deterministic() {
        let mut clock = SimClock::new();
        let mut world = World::new();
        let seen = Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
        for i in 0..3u32 {
            let seen = seen.clone();
            clock.run(i as f32 + 1.0, move |_| seen.lock().unwrap().push(i));
        }
        clock.update(&mut world);
        clock.update(&mut world);
        clock.update(&mut world);
        clock.update(&mut world);
        assert_eq!(*seen.lock().unwrap(), vec![0, 1, 2]);
    }
}
