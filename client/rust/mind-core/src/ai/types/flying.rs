// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FlyingAI` (plan 11 §4.2). Air units ignore terrain and steer directly toward
//! their target; flag/core targeting and `randomWaveAI` come with M3/M6. The
//! steering math is shared with [`super::ground::approach`].

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::unit::{HitboxComp, PhysicsComp, UnitCore};
use crate::entities::comp::{Pos, Vel};
use crate::world::TilePos;

use super::super::controller::UnitController;
use super::ground::{DEFAULT_ARRIVE, approach, tile_center};

/// Flying controller state (`FlyingAI`).
#[derive(Debug, Default, Clone, Copy)]
pub struct FlyingAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current move target.
    pub target: Option<TilePos>,
}

impl FlyingAi {
    /// Creates a controller with no target.
    pub fn new() -> Self {
        Self::default()
    }
}

impl UnitController for FlyingAi {
    fn unit(&self) -> Option<Entity> {
        self.unit
    }

    fn set_unit(&mut self, unit: Entity) {
        self.unit = Some(unit);
    }
}

/// Runs one flying-AI tick for `entity`: straight-line steering to `target`.
///
/// Returns `true` when the unit arrived this tick.
pub fn update_flying(world: &mut World, entity: Entity, target: TilePos) -> bool {
    let (pos, speed, hit_size) = {
        let Some(pos) = world.get::<Pos>(entity).copied() else {
            return false;
        };
        let speed = world
            .get::<PhysicsComp>(entity)
            .map(|physics| physics.speed)
            .unwrap_or(1.0)
            * world
                .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
                .map(|status| status.speed_multiplier)
                .unwrap_or(1.0);
        let hit_size = world
            .get::<HitboxComp>(entity)
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        (pos, speed, hit_size)
    };
    let dest = tile_center(target.x() as i32, target.y() as i32);
    let arrive = hit_size.max(DEFAULT_ARRIVE);

    let mut pos = pos;
    let mut vel = world
        .get::<Vel>(entity)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });
    let mut core = world
        .get::<UnitCore>(entity)
        .copied()
        .unwrap_or(UnitCore::new(0.0));
    let arrived = approach(&mut pos, &mut vel, &mut core, speed, dest.0, dest.1, arrive);
    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
    if let Some(mut stored) = world.get_mut::<UnitCore>(entity) {
        *stored = core;
    }
    arrived
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;

    #[test]
    fn flying_unit_crosses_impassable_terrain() {
        let mut harness = UnitHarness::new(64, 16, 3);
        // A solid wall spanning the map would block a ground unit.
        let wall = harness
            .content()
            .block_id("copper-wall")
            .expect("copper-wall");
        for y in 0..16 {
            assert!(harness.build.place(32, y, wall, 0, true));
        }
        let unit = harness.spawn("flare", 0, 40.0, 40.0, 0.0).expect("flare");
        assert!(harness.command_move(unit, 60, 5));
        for _ in 0..1200 {
            harness.tick();
        }
        assert_eq!(harness.unit_count(), 1, "flying unit survives the wall");
    }
}
