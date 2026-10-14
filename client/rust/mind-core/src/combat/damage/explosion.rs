// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Explosions (`core/src/mindustry/entities/Damage.java`
//! `dynamicExplosion`/`tileDamage`).
//!
//! [`dynamic_explosion`] ports all of `Damage.dynamicExplosion`'s simulation
//! half: `clamp(power / 700, 0, 8)` lightning bolts, `clamp(flammability / 4,
//! 0, 30)` fireballs and the `Time.run(i * 2f)` staggered damage waves. The
//! delays are carried by [`DelayedExplosionActions`] and drained by
//! `Sim::update_fires_and_puddles`; zero-delay actions run inline. The FX half
//! (`blockExplosionSmoke`, shockwaves, shake, `explosionFx`) is view-only and
//! stays with the host renderer.

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::content::Rgba;
use crate::content::registries::bullets::FIREBALL;
use crate::determinism::RngStream;
use crate::entities::comp::{Pos, TeamComp};
use crate::world::blocks::defense::projectors::{BaseShieldState, base_shield_radius};
use crate::world::blocks::defense::shields::ForceProjectorState;
use crate::world::blocks::defense::shields::is_in_regular_polygon;

use super::area::{DamageOptions, complete_damage, damage_area};
use crate::combat::bullet::{BulletSpawn, CombatCtx};

/// One delayed `Damage.dynamicExplosion` action (`Time.run(delay, action)`,
/// `Damage.java:97-125`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DelayedAction {
    /// `Lightning.create(Team.derelict, Pal.power, damage, x, y, angle, length)`.
    Lightning {
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Angle in degrees (`Mathf.random(360f)`).
        angle: f32,
        /// Branch length in tiles.
        length: i32,
        /// Bolt damage.
        damage: f32,
    },
    /// `Call.createBullet(Bullets.fireball, Team.derelict, x, y, angle, ...)`.
    Fireball {
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Angle in degrees.
        angle: f32,
    },
    /// One staggered damage wave (`Time.run(i * 2f, ...)`).
    Wave {
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Wave radius in world px.
        radius: f32,
        /// Wave damage.
        damage: f32,
        /// Shield ignore team (`null` skips the shield check).
        ignore_team: Option<u8>,
    },
}

/// Pending delayed explosion actions as `(remaining_ticks, action)` in
/// insertion order. The port has no `Time.run` queue for world actions, so the
/// entries count down one per tick in `Sim::update_fires_and_puddles`.
#[derive(Debug, Default, Resource)]
pub struct DelayedExplosionActions(pub Vec<(f32, DelayedAction)>);

/// Queues `action` after `delay` ticks (`Time.run`); a zero delay runs inline
/// (`Time.run(0)` lands in the same tick's `Time.update`).
fn schedule_or_run(ctx: &mut CombatCtx<'_>, delay: f32, action: DelayedAction) {
    if delay <= 0.0 {
        run_delayed_action(ctx, &action);
        return;
    }
    let mut actions = ctx
        .world
        .remove_resource::<DelayedExplosionActions>()
        .unwrap_or_default();
    actions.0.push((delay, action));
    ctx.world.insert_resource(actions);
}

/// Executes one due [`DelayedAction`].
pub fn run_delayed_action(ctx: &mut CombatCtx<'_>, action: &DelayedAction) {
    match *action {
        DelayedAction::Lightning {
            x,
            y,
            angle,
            length,
            damage,
        } => {
            // `Pal.power` (`Pal.java:92`); the port's lightning ignores the
            // color, but keep the value faithful.
            let _ = crate::combat::lightning::create(
                ctx,
                0,
                Rgba::from_rgba8888(0xFBAD67FF),
                damage,
                x,
                y,
                angle,
                length,
            );
        }
        DelayedAction::Fireball { x, y, angle } => {
            // `Bullets.fireball.damage` = 4 is the `damage -1` sentinel path.
            let _ = ctx.spawn(&BulletSpawn {
                def: FIREBALL,
                team: 0,
                x,
                y,
                angle,
                damage: -1.0,
                ..BulletSpawn::default()
            });
        }
        DelayedAction::Wave {
            x,
            y,
            radius,
            damage,
            ignore_team,
        } => {
            // `var shields = ignoreTeam == null ? null : indexer.getEnemy(...)`
            // (`Damage.java:120-127`): only an explicit ignore team checks
            // shields (H01 owns `explosion_absorbed` semantics).
            if ignore_team.is_some() && explosion_absorbed(ctx.world, x, y, damage, ignore_team) {
                return;
            }
            let _ = damage_area(
                ctx.world,
                ctx.content,
                ignore_team,
                x,
                y,
                radius,
                damage,
                DamageOptions::default(),
            );
        }
    }
}

/// `Damage.dynamicExplosion` (`Damage.java:93-130`): lightning, fireballs and
/// staggered radial damage from a destroyed block/unit.
///
/// `damage == false` skips all of it (`rules.damageExplosions`).
#[allow(clippy::too_many_arguments)]
pub fn dynamic_explosion(
    ctx: &mut CombatCtx<'_>,
    x: f32,
    y: f32,
    flammability: f32,
    explosiveness: f32,
    power: f32,
    radius: f32,
    damage: bool,
    fire: bool,
    ignore_team: Option<u8>,
) {
    if !damage {
        return;
    }
    // Lightning: `for(int i = 0; i < clamp(power / 700, 0, 8); i++)` — a float
    // bound, so the loop runs `ceil` times for fractional values.
    let bolts = (power / 700.0).clamp(0.0, 8.0);
    let mut i = 0u32;
    while (i as f32) < bolts {
        // `int length = 5 + clamp((int)(pow(power, 0.98f) / 500), 1, 18)` then
        // `length + Mathf.range(2)` (Arc's int range is inclusive).
        let length = 5
            + ((power.powf(0.98) / 500.0) as i32).clamp(1, 18)
            + ctx.rng.random(RngStream::Sim, 5)
            - 2;
        let bolt_damage = 3.0 + power.powf(0.35);
        let angle = ctx.rng.range(RngStream::Sim, 0.0, 360.0);
        let delay = i as f32 * 0.8 + ctx.rng.range(RngStream::Sim, 0.0, 4.0);
        schedule_or_run(
            ctx,
            delay,
            DelayedAction::Lightning {
                x,
                y,
                angle,
                length,
                damage: bolt_damage,
            },
        );
        i += 1;
    }
    if fire {
        // `for(int i = 0; i < clamp(flammability / 4, 0, 30); i++)`.
        let fireballs = (flammability / 4.0).clamp(0.0, 30.0);
        let mut i = 0u32;
        while (i as f32) < fireballs {
            let angle = ctx.rng.range(RngStream::Sim, 0.0, 360.0);
            schedule_or_run(ctx, i as f32 / 2.0, DelayedAction::Fireball { x, y, angle });
            i += 1;
        }
    }
    // `int waves = explosiveness <= 2 ? 0 : clamp((int)(explosiveness / 11), 1, 25)`.
    let waves = if explosiveness <= 2.0 {
        0
    } else {
        ((explosiveness / 11.0) as i32).clamp(1, 25)
    };
    let damage_per_wave = explosiveness / 2.0;
    for i in 0..waves {
        // `clamp(radius + explosiveness, 0, 50f) * ((i + 1f) / waves)`.
        let wave_radius =
            (radius + explosiveness).clamp(0.0, 50.0) * ((i as f32 + 1.0) / waves as f32);
        schedule_or_run(
            ctx,
            i as f32 * 2.0,
            DelayedAction::Wave {
                x,
                y,
                radius: wave_radius,
                damage: damage_per_wave,
                ignore_team,
            },
        );
    }
}

/// `ForceBuild.absorbExplosion(ex, ey, damage)` (ForceProjector.java:270-278)
/// over the live shields: `ForceProjectorState` uses its polygon and buildup
/// (`absorb_explosion`), `BaseShieldState` the simplified latch. Any absorbing
/// enemy shield skips the whole wave.
pub fn explosion_absorbed(
    world: &mut World,
    x: f32,
    y: f32,
    damage: f32,
    ignore_team: Option<u8>,
) -> bool {
    let mut projectors: Vec<Entity> = Vec::new();
    let mut base_shields: Vec<Entity> = Vec::new();
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<TeamComp>().map(|team| team.team) == ignore_team {
            continue;
        }
        if entity_ref.get::<ForceProjectorState>().is_some() {
            projectors.push(entity_ref.id());
        } else if entity_ref.get::<BaseShieldState>().is_some() {
            base_shields.push(entity_ref.id());
        }
    }
    let mut absorbed = false;
    for entity in projectors {
        let pos = world.get::<Pos>(entity).copied();
        if let (Some(pos), Some(mut state)) = (pos, world.get_mut::<ForceProjectorState>(entity))
            && state.absorb_explosion(pos.x, pos.y, x, y, damage)
        {
            absorbed = true;
        }
    }
    let mut hit: Vec<Entity> = Vec::new();
    for entity in base_shields {
        let Some(state) = world.get::<BaseShieldState>(entity) else {
            continue;
        };
        if state.broken {
            continue;
        }
        let Some(pos) = world.get::<Pos>(entity) else {
            continue;
        };
        let radius = base_shield_radius(state);
        if radius > 0.0 && is_in_regular_polygon(state.sides, pos.x, pos.y, radius, 0.0, x, y) {
            hit.push(entity);
        }
    }
    for entity in hit {
        if let Some(mut state) = world.get_mut::<BaseShieldState>(entity) {
            state.hit = 1.0;
        }
        absorbed = true;
    }
    absorbed
}

/// `Damage.tileDamage`: square-radius building damage from an explosion.
pub fn tile_damage(
    world: &mut World,
    content: &ContentRegistry,
    x: f32,
    y: f32,
    radius: f32,
    damage: f32,
) -> f32 {
    complete_damage(world, content, x, y, radius, damage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn dynamic_explosion_damages_radius() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, wall, 0, true));
        let (x, y) = CombatHarness::tile_center(10, 10);
        {
            let mut ctx = harness.combat_ctx();
            dynamic_explosion(&mut ctx, x, y, 0.0, 40.0, 0.0, 32.0, true, false, Some(1));
        }
        // Wave 0 runs inline; the rest stagger via `DelayedExplosionActions`.
        assert!(harness.building_health_at(10, 10) < 320.0);
        assert!(
            harness
                .build
                .world
                .get_resource::<DelayedExplosionActions>()
                .is_some_and(|actions| actions.0.len() >= 2),
            "later waves are queued"
        );
    }

    #[test]
    fn tile_damage_is_square() {
        let mut harness = CombatHarness::new(48, 32, 5);
        let wall = harness.content().block_id("copper-wall").expect("wall");
        assert!(harness.place(10, 10, wall, 0, true));
        let (x, y) = CombatHarness::tile_center(10, 10);
        assert!(
            tile_damage(
                &mut harness.build.world,
                &harness.build.content,
                x,
                y,
                12.0,
                40.0
            ) > 0.0
        );
    }
}
