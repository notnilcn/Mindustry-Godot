// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MassDriverBolt` behavior (`entities/bullet/MassDriverBolt.java`).
//!
//! Plan 10 R-10-5: the payload type (`DriverBulletData`) is owned by plan 08's
//! `MassDriver`. To avoid a `08 -> 10` cycle this module defines the
//! [`MassDriverPayload`] trait that 08 implements on its build; the bolt only
//! carries the destination entity in [`super::super::BulletData::MassDriver`].

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use super::super::behavior::BulletBehavior;
use super::super::{BulletData, CombatCtx, HIT, hit_bullet};
use super::motion;

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

/// `MassDriverBolt` behavior.
#[derive(Debug, Default, Clone, Copy)]
pub struct MassDriverBehavior;

impl BulletBehavior for MassDriverBehavior {
    fn update(&self, ctx: &mut CombatCtx<'_>, b: Entity) {
        let Some(data) = ctx.bullet(b).map(|bullet| bullet.data) else {
            return;
        };
        let BulletData::MassDriver(dest) = data else {
            return;
        };
        if ctx.world.get_entity(dest).is_err() {
            return;
        }
        let Some(((bx, by), _bv, _br)) = motion(ctx, b) else {
            return;
        };
        let Some((dx, dy)) = ctx.pos(dest) else {
            return;
        };
        let hit_dst = 7.0f32;
        let ndx = dx - bx;
        let ndy = dy - by;
        if ndx * ndx + ndy * ndy > hit_dst * hit_dst {
            return;
        }
        // Deliver through plan 08's installed handler (no-op until it lands).
        ctx.world
            .resource_scope::<MassDriverSink, _>(|world, mut sink| {
                sink.0.handle_payload(world, b, &[]);
            });
        hit_bullet(ctx, b, bx, by, false);
        if let Some(mut bullet) = ctx.world.get_mut::<super::super::Bullet>(b) {
            bullet.set(HIT);
        }
    }
}

/// Static mass-driver-behavior instance.
pub static MASS_DRIVER: MassDriverBehavior = MassDriverBehavior;
