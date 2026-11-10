// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret math helpers (`Angles`/`Mathf` subset, delta == 1).
//! Upstream: core/src/mindustry/world/blocks/defense/turrets/Turret.java, arc/math/Mathf.java

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::{Pos, TeamComp};

// ---- math helpers (`Angles`/`Mathf` subset, delta == 1) ----

pub(super) fn trnsx(angle_deg: f32, x: f32, y: f32) -> f32 {
    let r = angle_deg.to_radians();
    x * r.cos() + y * r.sin()
}

pub(super) fn trnsy(angle_deg: f32, x: f32, y: f32) -> f32 {
    let r = angle_deg.to_radians();
    x * r.sin() - y * r.cos()
}

pub(super) fn angle_to(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    (y2 - y1).atan2(x2 - x1).to_degrees()
}

pub(super) fn angle_dist(a: f32, b: f32) -> f32 {
    let d = (b - a).rem_euclid(360.0);
    if d > 180.0 { 360.0 - d } else { d }
}

pub(super) fn move_toward(from: f32, to: f32, speed: f32) -> f32 {
    if from < to {
        (from + speed).min(to)
    } else {
        (from - speed).max(to)
    }
}

pub(super) fn approach_delta(from: f32, to: f32, speed: f32) -> f32 {
    move_toward(from, to, speed)
}

pub(super) fn lerp_delta(from: f32, to: f32, alpha: f32) -> f32 {
    from + (to - from) * alpha.clamp(0.0, 1.0)
}

pub(super) fn efficiency(world: &World, e: Entity) -> f32 {
    world
        .get::<crate::entities::comp::Building>(e)
        .map(|b| b.efficiency)
        .unwrap_or(1.0)
}

/// `TeamComp.cheating()` (`TeamComp.java:17-19`): `team.rules().cheat`.
///
/// A cheating team never consumes turret ammo and ignores the heat efficiency
/// floor (`Turret.java:679`, `:685`, `:715`, `ItemTurret.java:109`).
pub fn cheating(world: &World, e: Entity) -> bool {
    let team = world.get::<TeamComp>(e).map(|team| team.team).unwrap_or(0);
    crate::game::runtime::rules_of(world).is_some_and(|rules| rules.team_rule(team).cheat)
}

pub(super) fn position(world: &World, e: Entity) -> Option<(f32, f32)> {
    world.get::<Pos>(e).map(|p| (p.x, p.y))
}
