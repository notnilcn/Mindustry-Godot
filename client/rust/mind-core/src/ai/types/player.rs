// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Player` controller (plan 15 possession): the live input → unit movement
//! bridge. Ports `DesktopInput.updateMovement`
//! (`core/src/mindustry/input/DesktopInput.java:975-1010`) plus
//! `UnitComp.movePref`/`moveAt`/`rotateMove`
//! (`core/src/mindustry/entities/comp/UnitComp.java:101-153`).
//!
//! The host uploads the binding axis every frame through [`PlayerInput`];
//! [`update_player`] runs inside `EntitySet::UpdateUnits` for units carrying
//! `AiKind::Player` and replaces the AI target pass for that tick. Units are in
//! world pixels; `Time.delta` is 1 at the fixed 60 Hz step, so velocity is a
//! per-tick step (`VelComp.move(vel * Time.delta)`).

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::{PhysicsComp, StatusComp, UnitCore};
use crate::entities::comp::{Pos, Vel};

use super::ground::normalize_angle;

/// Host-uploaded player movement axis (`Core.input.axis(Binding.moveX/Y)`).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct PlayerInput {
    /// Horizontal axis in `[-1, 1]` (positive = east).
    pub axis_x: f32,
    /// Vertical axis in `[-1, 1]` (positive = south).
    pub axis_y: f32,
    /// Latched once a non-zero axis reached a player unit (hint trigger).
    pub moved: bool,
}

/// Runs one player-controller tick for `entity`; returns `true` when a non-zero
/// axis was applied this tick (`HintsFragment.desktopMove` trigger).
pub fn update_player(world: &mut World, content: &ContentRegistry, entity: Entity) -> bool {
    let Some(input) = world.get_resource::<PlayerInput>().copied() else {
        return false;
    };
    let Some(mut pos) = world.get::<Pos>(entity).copied() else {
        return false;
    };
    let Some(mut vel) = world.get::<Vel>(entity).copied() else {
        return false;
    };
    let Some(mut core) = world.get::<UnitCore>(entity).copied() else {
        return false;
    };
    let Some(physics) = world.get::<PhysicsComp>(entity).copied() else {
        return false;
    };
    let speed_mult = world
        .get::<StatusComp>(entity)
        .map(|status| status.speed_multiplier)
        .unwrap_or(1.0);
    // `UnitComp.speed()`: boost/floor/strafe multipliers not modeled yet; base
    // speed * status multiplier matches `PhysicsComp` construction.
    let speed = physics.speed * speed_mult;
    let (omni, rotate_speed) = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
        .map(|def| (def.omni_movement, def.rotate_speed))
        .unwrap_or((false, 0.0));

    let (ax, ay) = (input.axis_x, input.axis_y);
    let moving = ax != 0.0 || ay != 0.0;
    let input_len = (ax * ax + ay * ay).sqrt().min(1.0);

    // `DesktopInput.updateMovement`: movement = normalized axis * speed().
    let target_len = input_len * speed;
    if !omni && moving {
        // `UnitComp.rotateMove`: face the input at `rotateSpeed` degrees/tick.
        core.rotation = move_toward(core.rotation, ay.atan2(ax).to_degrees(), rotate_speed);
    }
    let (dir_x, dir_y) = if omni {
        if input_len > 0.0 {
            (ax / input_len, ay / input_len)
        } else {
            (0.0, 0.0)
        }
    } else {
        let rad = core.rotation.to_radians();
        (rad.cos(), rad.sin())
    };
    let target = (dir_x * target_len, dir_y * target_len);

    // `UnitComp.moveAt`: `vel += (target - vel).limit(accel * target.len())`.
    let accel_limit = physics.accel * target_len;
    let dx = target.0 - vel.x;
    let dy = target.1 - vel.y;
    let dist = (dx * dx + dy * dy).sqrt();
    if accel_limit > 0.0 && dist > 0.0 {
        let scale = dist.min(accel_limit) / dist;
        vel.x += dx * scale;
        vel.y += dy * scale;
    } else if accel_limit <= 0.0 {
        vel.x = target.0;
        vel.y = target.1;
    }

    // `VelComp.update`: `move(vel * Time.delta)` with `Time.delta == 1`.
    pos.x += vel.x;
    pos.y += vel.y;

    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
    if let Some(mut stored) = world.get_mut::<UnitCore>(entity) {
        *stored = core;
    }
    if moving && let Some(mut stored) = world.get_resource_mut::<PlayerInput>() {
        stored.moved = true;
    }
    moving
}

/// `Angles.moveToward`: shortest signed angular step toward `to`.
fn move_toward(from: f32, to: f32, max_delta: f32) -> f32 {
    let mut diff = (to - from) % 360.0;
    if diff > 180.0 {
        diff -= 360.0;
    }
    if diff < -180.0 {
        diff += 360.0;
    }
    normalize_angle(from + diff.clamp(-max_delta, max_delta))
}

#[cfg(test)]
mod tests {
    use crate::sim::Sim;

    #[test]
    fn player_axis_moves_an_omni_unit_east() {
        let mut sim = Sim::new(
            1,
            64,
            64,
            crate::content::BlockId::AIR,
            crate::content::BlockId::AIR,
        );
        let Ok(()) = sim.install_unit_runtime() else {
            panic!("unit runtime installs");
        };
        // `flare` is an omni air unit; spawn east-facing with a full axis.
        let Some(_entity) = sim.spawn_player_unit("flare", 0, 100.0, 100.0, 0.0) else {
            panic!("player unit spawns");
        };
        sim.set_player_input(1.0, 0.0);
        for _ in 0..30 {
            let Ok(()) = sim.tick() else {
                panic!("tick");
            };
        }
        let pos = sim.player_unit_position().expect("player position");
        assert!(pos.0 > 100.0, "player moved east: {pos:?}");
        assert!(sim.player_unit_moved(), "moved flag latched");
    }

    #[test]
    fn no_input_keeps_the_player_still() {
        let mut sim = Sim::new(
            1,
            64,
            64,
            crate::content::BlockId::AIR,
            crate::content::BlockId::AIR,
        );
        let Ok(()) = sim.install_unit_runtime() else {
            panic!("unit runtime installs");
        };
        let Some(_entity) = sim.spawn_player_unit("flare", 0, 100.0, 100.0, 0.0) else {
            panic!("player unit spawns");
        };
        let before = sim.player_unit_position().expect("player position");
        sim.set_player_input(0.0, 0.0);
        for _ in 0..30 {
            let Ok(()) = sim.tick() else {
                panic!("tick");
            };
        }
        let after = sim.player_unit_position().expect("player position");
        assert_eq!(before, after, "no axis -> no movement");
        assert!(!sim.player_unit_moved(), "moved flag stays clear");
    }
}
