// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadMassDriver` build behavior
//! (`world/blocks/payloads/PayloadMassDriver.java`) — plan 08 M7.
//!
//! Charge machine + `waitingShooters` queue that throws a carried payload to a
//! linked driver. Effects/audio are plan 17; the physical bolt is plan 10's
//! (`MassDriverBolt`). This module also installs plan 10's `MassDriverSink`
//! adapter so item bolts reach [`crate::world::blocks::distribution::mass_driver::handle_payload`].

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::combat::bullet::kinds::mass_driver::{MassDriverPayload, MassDriverSink};
use crate::combat::{Bullet, BulletData};
use crate::entities::comp::{Building, Pos};
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::config::ConfigValue;
use crate::world::update::{delta, edelta};
use crate::world::{TileBuilds, TilePos};

use super::payload_block::{
    accept_payload_base, move_in_payload, move_out_payload, payload_block_update, rot_deg,
};
use super::{PayloadHolder, dispatch_handle_payload, payload_fits};

/// `PayloadMassDriver.PayloadDriverState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Component, Default)]
pub enum PayloadDriverState {
    /// `payloadDriverState.idle`.
    #[default]
    Idle,
    /// `accepting`.
    Accepting,
    /// `shooting`.
    Shooting,
}

/// `PayloadMassDriver.PayloadDriverBuild` state.
#[derive(Debug, Clone, Component, Default)]
pub struct PayloadDriverBuild {
    /// Packed destination tile or `-1` (`link`).
    pub link: i32,
    /// Turret rotation in degrees (`turretRotation`).
    pub turret_rotation: f32,
    /// Reload countdown `0..1` (`reloadCounter`).
    pub reload_counter: f32,
    /// Charge accumulator (`charge`).
    pub charge: f32,
    /// Loaded travel length (`payLength`).
    pub pay_length: f32,
    /// Whether the payload is loaded (`loaded`).
    pub loaded: bool,
    /// Whether currently charging (`charging`).
    pub charging: bool,
    /// State machine.
    pub state: PayloadDriverState,
    /// Ordered shooters (`waitingShooters`).
    pub waiting_shooters: SmallVec<[Entity; 4]>,
    /// Receive-effect delay (`effectDelayTimer`).
    pub effect_delay_timer: f32,
}

/// `PayloadMassDriver` behavior (`payload-mass-driver`, `large-payload-mass-driver`).
#[derive(Debug, Clone, Copy)]
pub struct PayloadMassDriverBehavior {
    /// `range` (world pixels).
    pub range: f32,
    /// `rotateSpeed`.
    pub rotate_speed: f32,
    /// `length`.
    pub length: f32,
    /// `knockback`.
    pub knockback: f32,
    /// `reload` (ticks).
    pub reload: f32,
    /// `chargeTime` (ticks).
    pub charge_time: f32,
    /// `maxPayloadSize`.
    pub max_payload_size: f32,
    /// `payloadSpeed`.
    pub payload_speed: f32,
}

impl PayloadMassDriverBehavior {
    /// Vanilla `payload-mass-driver`.
    pub const VANILLA: PayloadMassDriverBehavior = PayloadMassDriverBehavior {
        range: 700.0,
        rotate_speed: 5.0,
        length: 89.0 / 8.0,
        knockback: 5.0,
        reload: 45.0,
        charge_time: 70.0,
        max_payload_size: 2.5,
        payload_speed: 0.7,
    };
    /// `large-payload-mass-driver`.
    pub const LARGE: PayloadMassDriverBehavior = PayloadMassDriverBehavior {
        range: 2100.0,
        rotate_speed: 5.0,
        length: 89.0 / 8.0,
        knockback: 5.0,
        reload: 130.0,
        charge_time: 100.0,
        max_payload_size: 4.0,
        payload_speed: 0.7,
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

impl PayloadMassDriverBehavior {
    /// `PayloadDriverBuild.linkValid()`.
    pub fn link_valid(&self, world: &World, e: Entity) -> bool {
        let link = world
            .get::<PayloadDriverBuild>(e)
            .map(|b| b.link)
            .unwrap_or(-1);
        if link == -1 {
            return false;
        }
        let Some(other) = tile_entity(world, link) else {
            return false;
        };
        let same_block = world
            .get::<Building>(e)
            .zip(world.get::<Building>(other))
            .is_some_and(|(a, b)| a.block == b.block);
        let same_team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .zip(world.get::<crate::entities::comp::TeamComp>(other))
            .is_some_and(|(x, y)| x.team == y.team);
        world.get::<PayloadDriverBuild>(other).is_some()
            && same_block
            && same_team
            && world_dst(world, e, other) <= self.range
    }

    /// `PayloadDriverBuild.shooterValid(other)`.
    fn shooter_valid(&self, world: &World, e: Entity, other: Entity) -> bool {
        let Some(shooter) = world.get::<PayloadDriverBuild>(other) else {
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
            .get::<PayloadDriverBuild>(e)
            .and_then(|b| b.waiting_shooters.first().copied())
    }
}

impl BuildingBehavior for PayloadMassDriverBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<PayloadDriverBuild>(e).is_none() {
            let rotation = rot_deg(world, e);
            world.entity_mut(e).insert(PayloadDriverBuild {
                turret_rotation: rotation,
                ..PayloadDriverBuild::default()
            });
        }
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
        if !world.contains_resource::<MassDriverSink>() {
            world.insert_resource(MassDriverSink(Box::new(DriverSink)));
        }
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        _source: Entity,
        payload: PayloadRef,
    ) -> bool {
        accept_payload_base(world, e) && payload_fits(world, payload, self.max_payload_size)
    }

    fn handle_payload(&self, world: &mut World, e: Entity, source: Entity, payload: PayloadRef) {
        super::handle_payload(world, e, source, payload);
        if let Some(mut driver) = world.get_mut::<PayloadDriverBuild>(e) {
            driver.state = PayloadDriverState::Idle;
        }
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        world.get::<PayloadHolder>(e).and_then(|h| h.payload)
    }

    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        super::take_payload(world, e)
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        payload_block_update(world, e);
        let Some(mut build) = world.get::<PayloadDriverBuild>(e).cloned() else {
            return;
        };
        let has_link = self.link_valid(world, e);
        let dt = delta(world, e);
        let edt = edelta(world, e);
        let efficiency = world
            .get::<Building>(e)
            .map(|b| b.efficiency)
            .unwrap_or(0.0);

        if !build.charging {
            build.charge = (build.charge - dt * 10.0).max(0.0);
        }
        build.charging = false;
        build.effect_delay_timer -= dt;
        build.reload_counter = (build.reload_counter - edt / self.reload).clamp(0.0, 1.0);

        if let Some(current) = build.waiting_shooters.first().copied()
            && !self.shooter_valid(world, e, current)
        {
            build.waiting_shooters.retain(|s| *s != current);
        }

        let has_payload = world
            .get::<PayloadHolder>(e)
            .is_some_and(|h| h.payload.is_some());

        if build.state == PayloadDriverState::Idle {
            if !build.waiting_shooters.is_empty() && !has_payload {
                build.state = PayloadDriverState::Accepting;
            } else if has_link {
                build.state = PayloadDriverState::Shooting;
            }
        }

        if matches!(
            build.state,
            PayloadDriverState::Idle | PayloadDriverState::Accepting
        ) && has_payload
        {
            if build.loaded {
                build.pay_length -= self.payload_speed * dt;
                if build.pay_length <= 0.0 {
                    build.loaded = false;
                    if let Some(mut holder) = world.get_mut::<PayloadHolder>(e) {
                        holder.pay_vector = (0.0, 0.0);
                    }
                }
            } else {
                move_out_payload(world, e);
            }
        }

        if efficiency <= 0.0 {
            world.entity_mut(e).insert(build);
            return;
        }

        if build.state == PayloadDriverState::Accepting {
            let Some(current) = self.current_shooter(world, e) else {
                build.state = PayloadDriverState::Idle;
                world.entity_mut(e).insert(build);
                return;
            };
            if has_payload {
                build.state = PayloadDriverState::Idle;
                world.entity_mut(e).insert(build);
                return;
            }
            build.turret_rotation = move_toward(
                build.turret_rotation,
                angle_to(world, e, current),
                self.rotate_speed * efficiency,
            );
            world.entity_mut(e).insert(build);
            return;
        }

        if build.state != PayloadDriverState::Shooting {
            world.entity_mut(e).insert(build);
            return;
        }

        // Shooting state.
        if !has_link || (!build.waiting_shooters.is_empty() && !has_payload) {
            build.state = PayloadDriverState::Idle;
            world.entity_mut(e).insert(build);
            return;
        }
        let Some(target) = tile_entity(world, build.link) else {
            build.state = PayloadDriverState::Idle;
            world.entity_mut(e).insert(build);
            return;
        };
        let target_rotation = angle_to(world, e, target);
        let mut moved_out = false;
        if build.loaded {
            let load_length = self.length - build.reload_counter * self.knockback;
            build.pay_length += self.payload_speed * dt;
            if build.pay_length >= load_length {
                build.pay_length = load_length;
                moved_out = true;
            }
        } else if move_in_payload(world, e, false) {
            build.pay_length = 0.0;
            build.loaded = true;
        }

        if moved_out && let Some(payload) = world.get::<PayloadHolder>(e).and_then(|h| h.payload) {
            let target_has_payload = world
                .get::<PayloadHolder>(target)
                .is_some_and(|h| h.payload.is_some());
            let current = self.current_shooter(world, e);
            if !target_has_payload && current.is_none() {
                if let Some(mut other) = world.get_mut::<PayloadDriverBuild>(target)
                    && !other.waiting_shooters.contains(&e)
                {
                    other.waiting_shooters.push(e);
                }
            }
            if build.reload_counter <= 0.0 {
                build.turret_rotation = move_toward(
                    build.turret_rotation,
                    target_rotation,
                    self.rotate_speed * efficiency,
                );
                let fire = {
                    let other = world.get::<PayloadDriverBuild>(target);
                    other.is_some_and(|other| {
                        other.waiting_shooters.first() == Some(&e)
                            && other.state == PayloadDriverState::Accepting
                            && other.reload_counter <= 0.0
                            && near(build.turret_rotation, target_rotation, 1.0)
                            && near(other.turret_rotation, target_rotation + 180.0, 1.0)
                    })
                };
                if fire {
                    build.charge += edt;
                    build.charging = true;
                    if build.charge >= self.charge_time {
                        let cx = build.turret_rotation.to_radians().cos() * self.length;
                        let cy = build.turret_rotation.to_radians().sin() * self.length;
                        dispatch_handle_payload(world, target, e, payload);
                        if let Some(mut holder) = world.get_mut::<PayloadHolder>(e) {
                            holder.payload = None;
                        }
                        if let Some(mut other) = world.get_mut::<PayloadDriverBuild>(target) {
                            other.waiting_shooters.retain(|s| *s != e);
                            other.state = PayloadDriverState::Idle;
                            other.pay_length = self.length;
                            other.loaded = true;
                            other.turret_rotation = build.turret_rotation;
                            other.effect_delay_timer = 11.0;
                        }
                        if let Some(mut holder) = world.get_mut::<PayloadHolder>(target) {
                            holder.pay_vector = (-cx, -cy);
                            holder.pay_rotation = build.turret_rotation;
                        }
                        build.payload_reset();
                    }
                }
            }
        }
        world.entity_mut(e).insert(build);
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        let tile = world.get::<Building>(e).map(|b| b.tile);
        let link = world
            .get::<PayloadDriverBuild>(e)
            .map(|b| b.link)
            .unwrap_or(-1);
        match tile {
            Some(tile) if link != -1 => {
                let (x, y) = crate::world::pos::unpack(link);
                ConfigValue::Point2(x as i32 - tile.x() as i32, y as i32 - tile.y() as i32)
            }
            _ => ConfigValue::Point2(-1, -1),
        }
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        let link = match value {
            ConfigValue::Point2(x, y) => {
                let tile = world.get::<Building>(e).map(|b| b.tile);
                match tile {
                    Some(tile) if x != -1 || y != -1 => {
                        TilePos::new((tile.x() as i32 + x) as i16, (tile.y() as i32 + y) as i16)
                            .pack()
                    }
                    _ => -1,
                }
            }
            ConfigValue::Number(number) => number as i32,
            ConfigValue::None => -1,
            _ => return,
        };
        if let Some(mut build) = world.get_mut::<PayloadDriverBuild>(e) {
            build.link = link;
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        super::payload_block::write_base(world, e, w);
        let build = world
            .get::<PayloadDriverBuild>(e)
            .cloned()
            .unwrap_or_default();
        w.i(build.link);
        w.f(build.turret_rotation);
        w.b(match build.state {
            PayloadDriverState::Idle => 0i8,
            PayloadDriverState::Accepting => 1i8,
            PayloadDriverState::Shooting => 2i8,
        });
        w.f(build.reload_counter);
        w.f(build.charge);
        w.bool(build.loaded);
        w.bool(build.charging);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        super::payload_block::read_base(world, e, r);
        let link = r.i().unwrap_or(-1);
        let turret_rotation = r.f().unwrap_or(90.0);
        let state = match r.b().unwrap_or(0) {
            1 => PayloadDriverState::Accepting,
            2 => PayloadDriverState::Shooting,
            _ => PayloadDriverState::Idle,
        };
        let mut build = world
            .get::<PayloadDriverBuild>(e)
            .cloned()
            .unwrap_or_default();
        build.link = link;
        build.turret_rotation = turret_rotation;
        build.state = state;
        if revision >= 1 {
            build.reload_counter = r.f().unwrap_or(0.0);
            build.charge = r.f().unwrap_or(0.0);
            build.loaded = r.bool().unwrap_or(false);
            build.charging = r.bool().unwrap_or(false);
        }
        world.entity_mut(e).insert(build);
    }
}

impl PayloadDriverBuild {
    fn payload_reset(&mut self) {
        self.pay_length = 0.0;
        self.loaded = false;
        self.state = PayloadDriverState::Idle;
        self.reload_counter = 1.0;
    }
}

/// Plan 10 `MassDriverSink` adapter: routes a bolt's `DriverBulletData` into the
/// destination `MassDriverBuild`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DriverSink;

impl MassDriverPayload for DriverSink {
    fn handle_payload(&mut self, world: &mut World, bullet: Entity, _items: &[i32]) {
        let data = world.get::<Bullet>(bullet).map(|b| b.data);
        let Some(BulletData::MassDriver(data_entity)) = data else {
            return;
        };
        let Some(to) = world
            .get::<crate::world::blocks::distribution::mass_driver::DriverBulletData>(data_entity)
            .map(|d| d.to)
        else {
            return;
        };
        crate::world::blocks::distribution::mass_driver::handle_payload(world, to, data_entity);
    }

    fn dead(&self, world: &World, entity: Entity) -> bool {
        world.get_entity(entity).is_err()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn payload_driver_throws_to_linked_driver() {
        let mut harness = BuildHarness::new(40, 40, 7);
        let container = harness.content().block_id("container").expect("container");
        let driver = harness
            .content()
            .block_id("payload-mass-driver")
            .expect("payload-mass-driver");
        if let Some(mut rules) = harness
            .world
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.cheat = true;
        }
        assert!(harness.place(6, 6, driver, 0, true));
        assert!(harness.place(18, 6, driver, 0, true));
        assert!(harness.configure(6, 6, ConfigValue::Point2(12, 0)));
        assert!(harness.configure(18, 6, ConfigValue::Point2(-12, 0)));
        let from = harness.build_at(6, 6).expect("from");
        let to = harness.build_at(18, 6).expect("to");
        let payload_entity =
            super::super::create_build_payload(&mut harness.world, container, 0).expect("payload");
        let payload = PayloadRef {
            entity: Some(payload_entity),
            content: container.raw(),
            is_block: true,
        };
        super::super::handle_payload(&mut harness.world, from, from, payload);
        for _ in 0..3000 {
            harness.tick();
        }
        assert!(
            harness
                .world
                .get::<PayloadHolder>(to)
                .is_some_and(|h| h.payload.is_some()),
            "payload should have been thrown to the linked driver"
        );
    }
}
