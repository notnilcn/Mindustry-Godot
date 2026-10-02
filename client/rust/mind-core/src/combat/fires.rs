// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Fires` tile entities (`core/src/mindustry/entities/Fires.java` +
//! `entities/comp/FireComp.java`).
//!
//! A [`FireState`] component is the sim half of `FireComp`; the tile slot
//! (`Tiles::fires`, plan 06) is a cache synced by [`update_fires`] each tick so
//! bullets that hold a read-only grid borrow can still create fires. View data
//! (animation frames, light) and sound are plan 17.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::determinism::{RngStream, SimRng};
use crate::entities::comp::{Health, Pos, TeamComp};

use super::view::FxSink;

/// `Geometry.d4` neighbor offsets.
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// `Fires.baseLifetime`.
pub const BASE_LIFETIME: f32 = 1000.0;
/// `FireComp.spreadDelay`.
pub const SPREAD_DELAY: f32 = 22.0;
/// `FireComp.damageDelay`.
pub const DAMAGE_DELAY: f32 = 40.0;
/// `FireComp.tileDamage`.
pub const TILE_DAMAGE: f32 = 1.8;
/// `FireComp.unitDamage`.
pub const UNIT_DAMAGE: f32 = 3.0;

/// Fire state (`FireComp` sim fields; draw/animation are plan 16/17).
#[derive(Debug, Clone, Copy, Component)]
pub struct FireState {
    /// Owning tile.
    pub tile: (i16, i16),
    /// Ticks lived.
    pub time: f32,
    /// Ticks to live.
    pub lifetime: f32,
    /// Puddle flammability contribution this tick.
    pub puddle_flammability: f32,
    /// Spread accumulator (`FireComp.spreadTimer`).
    pub spread_timer: f32,
    /// Fireball accumulator (`FireComp.fireballTimer`).
    pub fireball_timer: f32,
    /// Damage accumulator (`FireComp.damageTimer`).
    pub damage_timer: f32,
    /// Warmup (view).
    pub warmup: f32,
    /// Animation frame (view).
    pub animation: f32,
}

impl FireState {
    /// Normalized life fraction (`Timedc.fin`).
    pub fn fin(&self) -> f32 {
        if self.lifetime <= 0.0 {
            1.0
        } else {
            (self.time / self.lifetime).clamp(0.0, 1.0)
        }
    }
}

/// Returns whether `(tx, ty)` has a live fire (`Fires.has`).
pub fn has(world: &World, tx: i16, ty: i16) -> bool {
    find_at(world, tx, ty).is_some()
}

/// Finds the fire entity at a tile (`Fires.get`).
pub fn find_at(world: &World, tx: i16, ty: i16) -> Option<Entity> {
    world
        .iter_entities()
        .find(|entity_ref| {
            entity_ref
                .get::<FireState>()
                .is_some_and(|fire| fire.tile == (tx, ty))
        })
        .map(|entity_ref| entity_ref.id())
}

/// Creates/refreshes a fire (`Fires.create`).
///
/// The `Env.oxygen`/`rules.fire` gate is read from the optional [`CombatEnv`]
/// resource (default: enabled), matching upstream until plan 12 owns `Rules`.
pub fn create(world: &mut World, tx: i16, ty: i16, rng: &mut SimRng) -> Option<Entity> {
    if !env_allows_fire(world) {
        return None;
    }
    if let Some(existing) = find_at(world, tx, ty) {
        if let Some(mut fire) = world.get_mut::<FireState>(existing) {
            fire.lifetime = BASE_LIFETIME;
            fire.time = 0.0;
        }
        return Some(existing);
    }
    let ts = crate::config::TILESIZE as f32;
    let entity = world
        .spawn((
            Pos {
                x: (tx as f32 + 0.5) * ts,
                y: (ty as f32 + 0.5) * ts,
            },
            TeamComp { team: 0 },
            FireState {
                tile: (tx, ty),
                time: 0.0,
                lifetime: BASE_LIFETIME,
                puddle_flammability: 0.0,
                spread_timer: rng.range(RngStream::Sim, 0.0, SPREAD_DELAY),
                fireball_timer: rng.range(RngStream::Sim, 0.0, 40.0),
                damage_timer: rng.range(RngStream::Sim, 0.0, DAMAGE_DELAY),
                warmup: 0.0,
                animation: rng.range(RngStream::Sim, 0.0, 39.0),
            },
        ))
        .id();
    Some(entity)
}

/// Extinguishes a fire by shortening its life (`Fires.extinguish`).
pub fn extinguish(world: &mut World, tx: i16, ty: i16, intensity: f32) -> bool {
    let Some(entity) = find_at(world, tx, ty) else {
        return false;
    };
    if let Some(mut fire) = world.get_mut::<FireState>(entity) {
        fire.time += intensity;
        return fire.time < fire.lifetime;
    }
    false
}

/// Removes a fire (`FireComp.remove`/`Fires.remove`).
pub fn remove(world: &mut World, entity: Entity) {
    let _ = world.despawn(entity);
}

/// Updates every fire one tick (`FireComp.update`).
///
/// `water_attr` is `state.envAttrs.get(Attribute.water)` (plan 12); `tiles` is
/// mutated to keep the `Tiles::fires` cache in sync with live entities.
pub fn update_fires(
    world: &mut World,
    tiles: &mut crate::world::tiles::Tiles,
    content: &crate::content::ContentRegistry,
    rng: &mut SimRng,
    fx: &dyn FxSink,
    water_attr: f32,
) {
    // Reset the slot cache; re-register live fires below.
    tiles.clear_fire_slots();
    let entities: Vec<Entity> = world
        .iter_entities()
        .filter(|entity_ref| entity_ref.get::<FireState>().is_some())
        .map(|entity_ref| entity_ref.id())
        .collect();
    for entity in entities {
        let Some(mut fire) = world.get_mut::<FireState>(entity) else {
            continue;
        };
        fire.animation = (fire.animation + 1.0) % 40.0;
        fire.warmup += 1.0;
        let speed_multiplier = 1.0 + (water_attr * 10.0).max(0.0);
        fire.time = (fire.time + speed_multiplier).min(fire.lifetime);

        let (tx, ty) = fire.tile;
        if fire.time >= fire.lifetime {
            let _ = fire;
            // `remove()` hook: fire-remove FX.
            if let Some(pos) = world.get::<Pos>(entity) {
                fx.effect(
                    &crate::content::registries::fx_meta::EffectRef::Named(
                        crate::content::EffectId::FIRE_REMOVE,
                    ),
                    pos.x,
                    pos.y,
                    0.0,
                    crate::content::Rgba::WHITE,
                );
            }
            remove(world, entity);
            continue;
        }
        let _ = fire;

        let in_bounds = tiles.in_bounds(tx as i32, ty as i32);
        let flammability = if in_bounds {
            let tile = tiles.get(tx as i32, ty as i32);
            let base = tile.get_flammability();
            let puddle = super::puddles::find_at(world, tx, ty)
                .and_then(|p| world.get::<super::puddles::PuddleState>(p).copied())
                .map(|p| {
                    content
                        .liquid(p.liquid)
                        .map(|def| def.flammability * p.amount)
                        .unwrap_or(0.0)
                })
                .unwrap_or(0.0);
            Some((base + puddle, tile.build))
        } else {
            None
        };
        let Some((flammability, build)) = flammability else {
            remove(world, entity);
            continue;
        };

        // Flammable floors damage themselves and spread to `d4` neighbors.
        if let Some(mut fire) = world.get_mut::<FireState>(entity) {
            fire.puddle_flammability = flammability;
            if flammability > 1.0 {
                let rate = (flammability / 5.0).clamp(0.3, 2.0);
                fire.spread_timer += rate;
                if fire.spread_timer >= SPREAD_DELAY {
                    fire.spread_timer = 0.0;
                    let pick = rng.random(RngStream::Sim, 4) as usize;
                    let (dx, dy) = D4[pick % 4];
                    let _ = fire;
                    let (nx, ny) = (tx as i32 + dx, ty as i32 + dy);
                    if tiles.in_bounds(nx, ny) {
                        let _ = create(world, nx as i16, ny as i16, rng);
                    }
                }
            }
        }

        // Damage the building on the tile every `damageDelay`.
        let damage_tick = {
            let Some(mut fire) = world.get_mut::<FireState>(entity) else {
                continue;
            };
            fire.fireball_timer =
                (fire.fireball_timer + (flammability / 10.0).clamp(0.0, 0.5)).min(40.0);
            fire.damage_timer += 1.0;
            if fire.damage_timer >= DAMAGE_DELAY {
                fire.damage_timer = 0.0;
                true
            } else {
                false
            }
        };
        if damage_tick
            && let Some(build) = build
            && let Some(mut health) = world.get_mut::<Health>(build)
        {
            health.health = (health.health - TILE_DAMAGE).max(0.0);
            if health.health <= 0.0 {
                health.dead = true;
            }
        }

        // Keep the tile-slot cache in sync.
        if in_bounds {
            tiles.set_fire(tiles.index(tx as i32, ty as i32), Some(entity));
        }
    }
}

/// Whether `rules.fire` and `Env.oxygen` allow fire (default enabled).
fn env_allows_fire(world: &World) -> bool {
    world
        .get_resource::<CombatEnv>()
        .map(|env| env.fire && env.oxygen)
        .unwrap_or(true)
}

/// Combat environment gates owned by plan 12 (`Rules.fire`, `Env.oxygen`).
#[derive(Debug, Clone, Copy, bevy_ecs::prelude::Resource)]
pub struct CombatEnv {
    /// `Rules.fire`.
    pub fire: bool,
    /// `Rules.hasEnv(Env.oxygen)`.
    pub oxygen: bool,
}

impl Default for CombatEnv {
    fn default() -> Self {
        Self {
            fire: true,
            oxygen: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn create_refresh_and_extinguish() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let mut rng = SimRng::new(3);
        let fire = create(&mut harness.build.world, 4, 4, &mut rng).expect("fire");
        assert!(has(&harness.build.world, 4, 4));
        // Refresh keeps the same entity.
        let again = create(&mut harness.build.world, 4, 4, &mut rng).expect("refresh");
        assert_eq!(fire, again);
        // Extinguish eventually removes.
        loop {
            if !extinguish(&mut harness.build.world, 4, 4, 200.0) {
                break;
            }
        }
        harness.fire_tick(0.0);
        assert!(!has(&harness.build.world, 4, 4));
    }

    #[test]
    fn water_attribute_speeds_up_burn() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let mut rng = SimRng::new(3);
        let e = create(&mut harness.build.world, 4, 4, &mut rng).expect("fire");
        harness.fire_tick(0.2);
        let time = harness
            .build
            .world
            .get::<FireState>(e)
            .map(|f| f.time)
            .unwrap_or(0.0);
        // speedMultiplier = 1 + 0.2*10 = 3.
        assert!((time - 3.0).abs() < 1e-4, "time={time}");
    }

    #[test]
    fn spread_timer_ignites_neighbor_with_flammable_puddle() {
        let mut harness = CombatHarness::new(16, 16, 5);
        let mut rng = SimRng::new(5);
        let oil = harness.content().liquid_id("oil").expect("oil");
        // Ignite the tile and keep a large oil puddle there so `flammability > 1`.
        let _ = create(&mut harness.build.world, 8, 8, &mut rng).expect("fire");
        for _ in 0..400 {
            let _ = super::super::puddles::deposit(
                &mut harness.build.world,
                &harness.build.content,
                8,
                8,
                oil,
                60.0,
                &mut rng,
            );
            harness.fire_tick(0.0);
        }
        let lit_neighbors = [(7, 8), (9, 8), (8, 7), (8, 9)]
            .iter()
            .filter(|(x, y)| has(&harness.build.world, *x, *y))
            .count();
        assert!(has(&harness.build.world, 8, 8));
        assert!(lit_neighbors >= 1, "spread to a d4 neighbor");
    }
}
