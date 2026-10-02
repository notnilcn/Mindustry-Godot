// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Puddles` tile entities (`core/src/mindustry/entities/Puddles.java` +
//! `entities/comp/PuddleComp.java`).
//!
//! [`PuddleState`] is the sim half of `PuddleComp`; liquid behavior (`viscosity`,
//! `willBoil`, floor reactions, `update`) is plan 02 data + plan 09 execution.
//! The `Tiles::puddles` slot is a cache synced by [`update_puddles`] (plan 10
//! M3), so bullets holding a read-only grid borrow can still deposit.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ContentRegistry, LiquidId};
use crate::determinism::{RngStream, SimRng};
use crate::entities::comp::Pos;

/// `Puddles.maxLiquid`.
pub const MAX_LIQUID: f32 = 70.0;

/// `Geometry.d4` neighbor offsets.
const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

/// Puddle state (`PuddleComp` sim fields; draw is plan 16).
#[derive(Debug, Clone, Copy, Component)]
pub struct PuddleState {
    /// Owning tile.
    pub tile: (i16, i16),
    /// Liquid content id.
    pub liquid: LiquidId,
    /// Current amount (`Puddle.amount`).
    pub amount: f32,
    /// Pending deposit this tick (`Puddle.accepting`).
    pub accepting: f32,
    /// Effect update cooldown (`Puddle.updateTime`).
    pub update_time: f32,
    /// Last ripple tick (`Puddle.lastRipple`).
    pub last_ripple: f32,
}

/// Finds the puddle entity at a tile (`Puddles.get`).
pub fn find_at(world: &World, tx: i16, ty: i16) -> Option<Entity> {
    world
        .iter_entities()
        .find(|entity_ref| {
            entity_ref
                .get::<PuddleState>()
                .is_some_and(|puddle| puddle.tile == (tx, ty))
        })
        .map(|entity_ref| entity_ref.id())
}

/// Whether a tile has a puddle of `liquid` (`Puddles.hasLiquid`).
pub fn has_liquid(world: &World, tx: i16, ty: i16, liquid: LiquidId) -> bool {
    find_at(world, tx, ty)
        .and_then(|entity| world.get::<PuddleState>(entity).copied())
        .is_some_and(|puddle| puddle.liquid == liquid && puddle.amount > 0.0)
}

/// Deposits `amount` of `liquid` at `(tx, ty)` (`Puddles.deposit`).
///
/// Gaseous liquids vaporize immediately (no puddle); a different existing liquid
/// is replaced.
pub fn deposit(
    world: &mut World,
    content: &ContentRegistry,
    tx: i16,
    ty: i16,
    liquid: LiquidId,
    amount: f32,
    rng: &mut SimRng,
) -> Option<Entity> {
    let def = content.liquid(liquid)?;
    // Gaseous liquids vaporize instead of pooling (`Liquid.willBoil` needs the
    // global heat attribute owned by plan 12; gases are always treated as boiling).
    if def.gas {
        return None;
    }
    let ts = crate::config::TILESIZE as f32;
    if let Some(entity) = find_at(world, tx, ty)
        && let Some(mut puddle) = world.get_mut::<PuddleState>(entity)
    {
        if puddle.liquid == liquid {
            puddle.accepting += amount;
            puddle.amount = (puddle.amount + puddle.accepting).min(MAX_LIQUID);
            puddle.accepting = 0.0;
            return Some(entity);
        }
        // Different liquid: reset to the new one.
        puddle.liquid = liquid;
        puddle.amount = amount.min(MAX_LIQUID);
        return Some(entity);
    }
    let entity = world
        .spawn((
            Pos {
                x: (tx as f32 + 0.5) * ts,
                y: (ty as f32 + 0.5) * ts,
            },
            PuddleState {
                tile: (tx, ty),
                liquid,
                amount: amount.clamp(0.0, MAX_LIQUID),
                accepting: 0.0,
                update_time: 0.0,
                last_ripple: rng.range(RngStream::Sim, 0.0, 40.0),
            },
        ))
        .id();
    Some(entity)
}

/// Removes a puddle (`PuddleComp.remove`/`Puddles.remove`).
pub fn remove(world: &mut World, entity: Entity) {
    let _ = world.despawn(entity);
}

/// Updates every puddle one tick (`PuddleComp.update`).
///
/// Spreads to `d4` neighbors above `maxLiquid / 1.5`, evaporates at zero, and
/// re-registers the tile-slot cache.
pub fn update_puddles(
    world: &mut World,
    tiles: &mut crate::world::tiles::Tiles,
    content: &ContentRegistry,
    rng: &mut SimRng,
    _fx: &dyn super::view::FxSink,
) {
    tiles.clear_puddle_slots();
    let entities: Vec<Entity> = world
        .iter_entities()
        .filter(|entity_ref| entity_ref.get::<PuddleState>().is_some())
        .map(|entity_ref| entity_ref.id())
        .collect();
    for entity in entities {
        let Some(mut puddle) = world.get_mut::<PuddleState>(entity) else {
            continue;
        };
        let (tx, ty) = puddle.tile;
        let Some(def) = content.liquid(puddle.liquid) else {
            let _ = puddle;
            remove(world, entity);
            continue;
        };
        let viscosity = def.viscosity;
        let cap_puddles = def.cap_puddles;
        let add_speed = if puddle.accepting > 0.0 { 3.0 } else { 0.0 };
        puddle.amount -= (1.0 - viscosity) / (5.0 + add_speed);
        puddle.amount += puddle.accepting;
        puddle.amount = puddle.amount.min(MAX_LIQUID);
        puddle.accepting = 0.0;
        puddle.update_time -= 1.0;

        if puddle.amount >= MAX_LIQUID / 1.5 {
            let deposited = ((puddle.amount - MAX_LIQUID / 1.5) / 4.0).min(0.3);
            let liquid = puddle.liquid;
            let mut targets = 0;
            for (dx, dy) in D4 {
                let (nx, ny) = (tx as i32 + dx, ty as i32 + dy);
                if tiles.in_bounds(nx, ny) {
                    let other = tiles.get(nx, ny);
                    if other.build.is_none() || def.move_through_blocks {
                        targets += 1;
                    }
                }
            }
            if targets > 0 {
                puddle.amount -= deposited * targets as f32;
            }
            let _ = puddle;
            for (dx, dy) in D4 {
                let (nx, ny) = (tx as i32 + dx, ty as i32 + dy);
                if tiles.in_bounds(nx, ny) {
                    let empty = tiles.get(nx, ny).build.is_none() || def.move_through_blocks;
                    if empty {
                        let _ =
                            deposit(world, content, nx as i16, ny as i16, liquid, deposited, rng);
                    }
                }
            }
            let Some(mut puddle) = world.get_mut::<PuddleState>(entity) else {
                continue;
            };
            if cap_puddles {
                puddle.amount = puddle.amount.clamp(0.0, MAX_LIQUID);
            }
            if puddle.amount <= 0.0 {
                let _ = puddle;
                remove(world, entity);
                continue;
            }
            if tiles.in_bounds(tx as i32, ty as i32) {
                tiles.set_puddle(tiles.index(tx as i32, ty as i32), Some(entity));
            }
            continue;
        }

        if cap_puddles {
            puddle.amount = puddle.amount.clamp(0.0, MAX_LIQUID);
        }

        if puddle.amount <= 0.0 {
            let _ = puddle;
            remove(world, entity);
            continue;
        }
        if tiles.in_bounds(tx as i32, ty as i32) {
            tiles.set_puddle(tiles.index(tx as i32, ty as i32), Some(entity));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::harness::CombatHarness;

    #[test]
    fn deposit_caps_at_max_liquid() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let water = harness.content().liquid_id("water").expect("water");
        let mut rng = SimRng::new(3);
        let e = deposit(
            &mut harness.build.world,
            &harness.build.content,
            4,
            4,
            water,
            500.0,
            &mut rng,
        )
        .expect("puddle");
        let amount = harness
            .build
            .world
            .get::<PuddleState>(e)
            .map(|p| p.amount)
            .unwrap_or(0.0);
        assert_eq!(amount, MAX_LIQUID);
    }

    #[test]
    fn gaseous_liquid_does_not_pool() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let mut rng = SimRng::new(3);
        let gas = harness.content().liquid_id("hydrogen").expect("hydrogen");
        assert!(
            deposit(
                &mut harness.build.world,
                &harness.build.content,
                4,
                4,
                gas,
                10.0,
                &mut rng
            )
            .is_none()
        );
    }

    #[test]
    fn spread_neighbors_above_threshold() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let water = harness.content().liquid_id("water").expect("water");
        let mut rng = SimRng::new(3);
        let e = deposit(
            &mut harness.build.world,
            &harness.build.content,
            8,
            8,
            water,
            MAX_LIQUID,
            &mut rng,
        )
        .expect("puddle");
        for _ in 0..5 {
            harness.puddle_tick();
        }
        let neighbors = [(7, 8), (9, 8), (8, 7), (8, 9)]
            .iter()
            .filter(|(x, y)| find_at(&harness.build.world, *x, *y).is_some())
            .count();
        assert!(neighbors >= 1, "spread to d4 neighbors");
        assert!(harness.build.world.get_entity(e).is_ok());
    }

    #[test]
    fn zero_amount_evaporates() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let water = harness.content().liquid_id("water").expect("water");
        let mut rng = SimRng::new(3);
        let e = deposit(
            &mut harness.build.world,
            &harness.build.content,
            4,
            4,
            water,
            1.0,
            &mut rng,
        )
        .expect("puddle");
        for _ in 0..40 {
            harness.puddle_tick();
        }
        assert!(harness.build.world.get_entity(e).is_err());
    }
}
