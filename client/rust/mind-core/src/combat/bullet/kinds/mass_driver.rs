// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MassDriverBolt` behavior (`entities/bullet/MassDriverBolt.java`) plus the
//! physical-bolt carrier that plan 08's `MassDriver` fires through.
//!
//! Plan 10 R-10-5: the payload type (`DriverBulletData`) is owned by plan 08's
//! `MassDriver`. To avoid a `08 -> 10` cycle this module defines the
//! [`MassDriverPayload`] trait that 08 implements on its build; the bolt only
//! carries the destination entity in [`BulletData::MassDriver`].
//!
//! The bolt is created by [`MassDriverBoltCarrier`], a plan-10 implementation of
//! plan 08's [`MassDriverPayloadCarrier`] seam. Because `MassDriverBuild.fire`
//! runs in the plan-07 building update pass (outside the plan-10 bullet runtime),
//! the carrier only records a [`QueuedMassDriverBolt`]; the host
//! (`CombatHarness`/`MindSimHost`) drains [`MassDriverBoltQueue`] and spawns the
//! real [`crate::combat::bullet::Bullet`] through the normal `create` path. This
//! keeps deterministic ordering and lets the bolt participate in the bullet
//! group, collision and checksum passes.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::content::registries::fx_meta::EffectRef;
use crate::content::{BulletId, EffectId, Rgba};
use crate::determinism::RngStream;
use crate::entities::comp::Pos;

use super::super::behavior::BulletBehavior;
use super::super::{Bullet, BulletData, BulletSpawn, CombatCtx, HIT, hit_bullet};
use super::motion;

use crate::world::blocks::distribution::mass_driver::{DriverBulletData, MassDriverPayloadCarrier};

/// Interface plan 08's `MassDriverBuild` implements to receive a bolt payload.
pub trait MassDriverPayload: Send + Sync + 'static {
    /// Called when a `MassDriverBolt` reaches this destination.
    fn handle_payload(&mut self, world: &mut World, bullet: Entity, items: &[i32]);

    /// Whether the destination is dead/removed (the bolt keeps flying).
    fn dead(&self, world: &World, entity: Entity) -> bool;
}

/// World resource holding the installed [`MassDriverPayload`] handler.
#[derive(Resource)]
pub struct MassDriverSink(pub Box<dyn MassDriverPayload>);

/// A physical `MassDriverBolt` queued by [`MassDriverBoltCarrier`] for the host
/// to spawn (`MassDriverBuild.fire` cannot reach the plan-10 bullet runtime).
#[derive(Debug, Clone, Copy)]
pub struct QueuedMassDriverBolt {
    /// `DriverBulletData` entity (`from`/`to`/`items`).
    pub data: Entity,
    /// Content id of the registered `MassDriverBolt` def.
    pub bolt: BulletId,
    /// Spawn x (source driver muzzle).
    pub x: f32,
    /// Spawn y.
    pub y: f32,
    /// Travel angle in degrees (source -> destination).
    pub angle: f32,
    /// Requested speed (world units/tick) so the bolt arrives in `lifetime`.
    pub speed: f32,
    /// Requested lifetime in ticks.
    pub lifetime: f32,
    /// Source team.
    pub team: u8,
}

/// Queue of physical bolts awaiting spawn (drained by the combat host).
#[derive(Debug, Default, Resource)]
pub struct MassDriverBoltQueue {
    /// Pending bolts in fire order.
    pub bolts: Vec<QueuedMassDriverBolt>,
}

/// Plan-10 [`MassDriverPayloadCarrier`]: converts plan 08's fire request into a
/// queued physical bolt (see module docs).
#[derive(Debug, Clone, Copy)]
pub struct MassDriverBoltCarrier {
    /// Content id of the `MassDriverBolt` def this carrier fires.
    pub bolt: BulletId,
}

impl MassDriverPayloadCarrier for MassDriverBoltCarrier {
    fn fire(&self, world: &mut World, data: Entity, travel_ticks: f32) {
        let Some((from, to)) = world.get::<DriverBulletData>(data).map(|d| (d.from, d.to)) else {
            return;
        };
        let (Some(fp), Some(tp)) = (
            world.get::<Pos>(from).copied(),
            world.get::<Pos>(to).copied(),
        ) else {
            return;
        };
        let (dx, dy) = (tp.x - fp.x, tp.y - fp.y);
        let dist = (dx * dx + dy * dy).sqrt();
        let angle = dy.atan2(dx).to_degrees();
        let lifetime = travel_ticks.max(1.0 / 60.0);
        let speed = if dist > 0.0 { dist / lifetime } else { 0.0 };
        let team = world
            .get::<crate::entities::comp::TeamComp>(from)
            .map(|t| t.team)
            .unwrap_or(0);
        let entry = QueuedMassDriverBolt {
            data,
            bolt: self.bolt,
            x: fp.x,
            y: fp.y,
            angle,
            speed,
            lifetime,
            team,
        };
        if !world.contains_resource::<MassDriverBoltQueue>() {
            world.insert_resource(MassDriverBoltQueue::default());
        }
        world
            .resource_mut::<MassDriverBoltQueue>()
            .bolts
            .push(entry);
    }
}

/// Converts a queued bolt into a [`BulletSpawn`] (`create` needs multipliers).
pub fn queued_spawn(
    content: &crate::content::ContentRegistry,
    queued: &QueuedMassDriverBolt,
) -> Option<BulletSpawn> {
    let def = content.bullet(queued.bolt)?;
    let velocity_scl = if def.speed.abs() > 1e-6 {
        queued.speed / def.speed
    } else {
        0.0
    };
    let lifetime_scl = if def.lifetime.abs() > 1e-6 {
        queued.lifetime / def.lifetime
    } else {
        1.0
    };
    Some(BulletSpawn {
        def: queued.bolt,
        team: queued.team,
        x: queued.x,
        y: queued.y,
        angle: queued.angle,
        velocity_scl,
        lifetime_scl,
        data: BulletData::MassDriver(queued.data),
        ..BulletSpawn::default()
    })
}

/// `MassDriverBolt` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct MassDriverBehavior;

/// `Angles.near(a, b, eps)`.
fn angle_near(a: f32, b: f32, eps: f32) -> bool {
    ((b - a + 180.0).rem_euclid(360.0) - 180.0).abs() < eps
}

impl MassDriverBehavior {
    /// Delivers the payload through the installed sink, or directly through
    /// plan 08's `handle_payload` when no sink is registered.
    fn deliver(ctx: &mut CombatCtx<'_>, bullet: Entity, to: Entity, data: Entity) {
        if ctx.world.contains_resource::<MassDriverSink>() {
            ctx.world
                .resource_scope::<MassDriverSink, _>(|world, mut sink| {
                    sink.0.handle_payload(world, bullet, &[]);
                });
        } else {
            crate::world::blocks::distribution::mass_driver::handle_payload(ctx.world, to, data);
        }
    }
}

impl BulletBehavior for MassDriverBehavior {
    fn update(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(data) = ctx.bullet(b).map(|bullet| bullet.data) else {
            return;
        };
        let BulletData::MassDriver(data) = data else {
            // Data MUST be a `DriverBulletData`; otherwise hit immediately.
            let (x, y) = ctx.pos(b).unwrap_or((0.0, 0.0));
            hit_bullet(ctx, b, x, y, false);
            if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(b) {
                bullet.set(HIT);
            }
            return;
        };
        // The destination (or source) being gone means the bolt flies on.
        let Some((from, to)) = ctx
            .world
            .get::<DriverBulletData>(data)
            .map(|d| (d.from, d.to))
        else {
            return;
        };
        if ctx.world.get_entity(from).is_err() || ctx.world.get_entity(to).is_err() {
            return;
        }
        let Some(((bx, by), _bv, _br)) = motion(ctx, b) else {
            return;
        };
        let (Some((fx, fy)), Some((tx, ty))) = (ctx.pos(from), ctx.pos(to)) else {
            return;
        };
        let hit_dst = 7.0f32;
        let base_dst = (tx - fx).hypot(ty - fy);
        let dst1 = (bx - fx).hypot(by - fy);
        let dst2 = (bx - tx).hypot(by - ty);

        let mut intersect = false;
        // Bullet has gone past the destination: did it intersect the line?
        if dst1 > base_dst {
            let angle_to = (ty - by).atan2(tx - bx).to_degrees();
            let base_angle = (fy - ty).atan2(fx - tx).to_degrees();
            if angle_near(angle_to, base_angle, 2.0) {
                intersect = true;
                // Snap back (low-FPS compensation).
                if let Some(mut pos) = ctx.world.get_mut::<Pos>(b) {
                    let rad = base_angle.to_radians();
                    pos.x = tx + rad.cos() * hit_dst;
                    pos.y = ty + rad.sin() * hit_dst;
                }
            }
        }
        // On course and within range of the target.
        if (dst1 + dst2 - base_dst).abs() < 4.0 && dst2 <= hit_dst {
            intersect = true;
        }
        if !intersect {
            return;
        }
        Self::deliver(ctx, b, to, data);
        let (x, y) = ctx.pos(b).unwrap_or((bx, by));
        hit_bullet(ctx, b, x, y, false);
        if let Some(mut bullet) = ctx.world.get_mut::<Bullet>(b) {
            bullet.set(HIT);
        }
    }

    fn despawned(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        // `MassDriverBolt.despawned`: drop each in-flight item stack as a
        // `Fx.dropItem`. Plan 17 owns the item-bearing FX body, so the sim only
        // emits the `DROP_ITEM` marker at the bolt position, one per surviving
        // stack (deterministic 1..=amount, matching `Mathf.random(0, items[i])`).
        let Some(BulletData::MassDriver(data)) = ctx.bullet(b).map(|bullet| bullet.data) else {
            return;
        };
        let Some(items) = ctx
            .world
            .get::<DriverBulletData>(data)
            .map(|d| d.items.clone())
        else {
            return;
        };
        let (x, y) = ctx.pos(b).unwrap_or((0.0, 0.0));
        for amount in items.iter() {
            if *amount <= 0 {
                continue;
            }
            let dropped = ctx.rng.random(RngStream::Sim, *amount + 1);
            if dropped > 0 {
                ctx.fx.effect(
                    &EffectRef::Named(EffectId::DROP_ITEM),
                    x,
                    y,
                    0.0,
                    Rgba::WHITE,
                );
            }
        }
    }

    fn hit(&self, ctx: &mut CombatCtx<'_>, b: Entity, x: f32, y: f32, create_frags: bool) {
        hit_bullet(ctx, b, x, y, create_frags);
        // `MassDriverBolt.hit`: explode from the payload's item stats.
        let Some(BulletData::MassDriver(data)) = ctx.bullet(b).map(|bullet| bullet.data) else {
            return;
        };
        let Some(items) = ctx
            .world
            .get::<DriverBulletData>(data)
            .map(|d| d.items.clone())
        else {
            return;
        };
        if items.iter().all(|amount| *amount <= 0) {
            return;
        }
        let mut explosiveness = 0.0f32;
        let mut flammability = 0.0f32;
        let mut power = 0.0f32;
        for (index, amount) in items.iter().enumerate() {
            if *amount <= 0 {
                continue;
            }
            let Some(item) = ctx.content.items().get(index) else {
                continue;
            };
            let amount = *amount as f32;
            explosiveness += item.explosiveness * amount;
            flammability += item.flammability * amount;
            power += item.charge * amount.powf(1.1) * 25.0;
        }
        let _ = flammability;
        let team = ctx.team(b);
        crate::combat::damage::explosion::dynamic_explosion(
            ctx.world,
            ctx.content,
            Some(team),
            x,
            y,
            1.0,
            explosiveness / 10.0,
            power,
            false,
        );
    }
}

/// Static mass-driver-behavior instance.
pub static MASS_DRIVER: MassDriverBehavior = MassDriverBehavior;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;
    use crate::world::config::ConfigValue;
    use crate::world::limits::BuildRules;
    use crate::world::modules::ItemModule;

    #[test]
    fn mass_driver_fires_physical_bolt_and_delivers_items() {
        let mut harness = CombatHarness::new(64, 64, 7);
        if let Some(mut rules) = harness.build.world.get_resource_mut::<BuildRules>() {
            rules.cheat = true;
        }
        let driver = harness
            .content()
            .block_id("mass-driver")
            .expect("mass-driver");
        assert!(harness.place(8, 8, driver, 0, true));
        assert!(harness.place(20, 8, driver, 0, true));
        assert!(harness.build.configure(8, 8, ConfigValue::Point2(12, 0)));
        let copper = harness.content().item_id("copper").expect("copper");
        let from = harness.build_at(8, 8).expect("from");
        harness
            .build
            .world
            .get_mut::<ItemModule>(from)
            .expect("items")
            .add(copper, 120, 120);

        let bolt_id = harness.bullet_id("mass_driver").expect("bolt def");
        let live_bolt = |h: &CombatHarness| {
            h.bullets
                .iter()
                .any(|e| h.build.world.get::<Bullet>(*e).map(|b| b.def) == Some(bolt_id))
        };

        let mut saw_bolt = false;
        for _ in 0..400 {
            harness.tick();
            if live_bolt(&harness) {
                saw_bolt = true;
                break;
            }
        }
        assert!(
            saw_bolt,
            "plan-10 carrier spawned a physical MassDriverBolt"
        );

        for _ in 0..600 {
            harness.tick();
        }
        let to = harness.build_at(20, 8).expect("to");
        let received = harness
            .build
            .world
            .get::<ItemModule>(to)
            .map(|m| m.total)
            .unwrap_or(0);
        assert_eq!(received, 120, "receiver total");
        assert!(!live_bolt(&harness), "bolt consumed on delivery");
    }
}
