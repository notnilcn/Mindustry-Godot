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
/// Vaporizes when the liquid boils (`Liquid.willBoil`: global heat ≥
/// `boilPoint`; gases always boil), reacts with a liquid-drop floor it cannot
/// stay on (`tar`/`pooled-cryofluid`/`molten-slag`/…), and reacts with a
/// different existing puddle. A solid floor rejects the puddle.
#[allow(clippy::too_many_arguments)]
pub fn deposit(
    world: &mut World,
    tiles: &crate::world::tiles::Tiles,
    content: &ContentRegistry,
    tx: i16,
    ty: i16,
    liquid: LiquidId,
    amount: f32,
    rng: &mut SimRng,
) -> Option<Entity> {
    let def = content.liquid(liquid)?;
    // `Liquid.willBoil()` (`Attribute.heat.env() >= boilPoint`); gases set
    // `boilPoint = -1`, so they always boil. The heat value is the global
    // `state.envAttrs` (plan 06/12), surfaced through [`super::fires::CombatEnv`].
    let heat = world
        .get_resource::<super::fires::CombatEnv>()
        .map(|env| env.heat)
        .unwrap_or(0.0);
    if def.gas || heat >= def.boil_point {
        return None;
    }

    let ts = crate::config::TILESIZE as f32;
    let ax = (tx as f32 + 0.5) * ts;
    let ay = (ty as f32 + 0.5) * ts;

    // Floor liquid-drop reaction (`Puddles.deposit` step 4): an `isLiquid` floor
    // carries a `liquidDrop`; if the incoming liquid cannot stay on it, react
    // (fire/steam/`Liquid.react`) and do not pool.
    let floor_def = tiles
        .in_bounds(tx as i32, ty as i32)
        .then(|| content.block(tiles.get(tx as i32, ty as i32).floor))
        .flatten();
    if let Some(drop) = floor_def.and_then(|floor| floor.liquid_drop)
        && !def.can_stay_on.contains(&drop)
    {
        let _ = react_puddle(world, content, drop, liquid, amount, tx, ty, rng);
        return None;
    }
    // `if(tile.floor().solid) return;`
    if floor_def.is_some_and(|floor| floor.solid) {
        return None;
    }

    if let Some(entity) = find_at(world, tx, ty)
        && let Some(puddle) = world.get::<PuddleState>(entity).copied()
    {
        if puddle.liquid == liquid {
            if let Some(mut puddle) = world.get_mut::<PuddleState>(entity) {
                puddle.accepting += amount;
                puddle.amount = (puddle.amount + puddle.accepting).min(MAX_LIQUID);
                puddle.accepting = 0.0;
            }
            return Some(entity);
        }
        // Different existing liquid: `reactPuddle(p.liquid, liquid, ...)`.
        let added = react_puddle(world, content, puddle.liquid, liquid, amount, tx, ty, rng);
        if let Some(mut puddle) = world.get_mut::<PuddleState>(entity) {
            puddle.amount = (puddle.amount + added).clamp(0.0, MAX_LIQUID);
        }
        return Some(entity);
    }
    let entity = world
        .spawn((
            Pos { x: ax, y: ay },
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

/// `Puddles.reactPuddle`: reacts two liquids at a tile, returning the amount of
/// `liquid` that should be added to the destination puddle.
///
/// Flammable + hot ignites a fire; a cold/hot mismatch removes liquid (steam);
/// otherwise defers to [`liquid_react`]. The `Bullets.fireball.createNet`
/// chance and `Fx.steam` are view/net seams (plan 17/21).
#[allow(clippy::too_many_arguments)]
fn react_puddle(
    world: &mut World,
    content: &ContentRegistry,
    dest: LiquidId,
    liquid: LiquidId,
    amount: f32,
    tx: i16,
    ty: i16,
    rng: &mut SimRng,
) -> f32 {
    let Some(dest_def) = content.liquid(dest) else {
        return 0.0;
    };
    let Some(other) = content.liquid(liquid) else {
        return 0.0;
    };
    let flammable_hot = (dest_def.flammability > 0.3 && other.temperature > 0.7)
        || (other.flammability > 0.3 && dest_def.temperature > 0.7);
    if flammable_hot {
        let _ = super::fires::create(world, tx, ty, rng);
    } else if dest_def.temperature > 0.7 && other.temperature < 0.55 {
        // Cold liquid poured onto a hot puddle.
        return -0.1 * amount;
    } else if other.temperature > 0.7 && dest_def.temperature < 0.55 {
        // Hot liquid poured onto a cold puddle.
        return -0.7 * amount;
    }
    // `Liquid.react` (base `0`; `CellLiquid` returns `amount` for `spreadTarget`).
    liquid_react(dest_def, liquid, amount)
}

/// `Liquid.react` behavior half (`Liquid.java`; `CellLiquid.java` override).
fn liquid_react(
    dest: &crate::content::registries::liquids::Liquid,
    other: LiquidId,
    amount: f32,
) -> f32 {
    if dest.cell.as_ref().and_then(|cell| cell.spread_target) == Some(other) {
        amount
    } else {
        0.0
    }
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
                        let _ = deposit(
                            world, tiles, content, nx as i16, ny as i16, liquid, deposited, rng,
                        );
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
            &harness.build.grid.tiles,
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
                &harness.build.grid.tiles,
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
            &harness.build.grid.tiles,
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
            &harness.build.grid.tiles,
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

    #[test]
    fn will_boil_heat_attribute_vaporizes_fitting_liquid() {
        let mut harness = CombatHarness::new(16, 16, 3);
        // `Attribute.heat.env() >= boilPoint` (water 0.5, oil 0.65).
        harness
            .build
            .world
            .insert_resource(crate::combat::fires::CombatEnv {
                heat: 0.6,
                ..Default::default()
            });
        let water = harness.content().liquid_id("water").expect("water");
        let oil = harness.content().liquid_id("oil").expect("oil");
        let mut rng = SimRng::new(3);
        // water boils (0.6 >= 0.5); oil does not (0.6 < 0.65).
        assert!(
            deposit(
                &mut harness.build.world,
                &harness.build.grid.tiles,
                &harness.build.content,
                4,
                4,
                water,
                10.0,
                &mut rng
            )
            .is_none()
        );
        assert!(
            deposit(
                &mut harness.build.world,
                &harness.build.grid.tiles,
                &harness.build.content,
                5,
                5,
                oil,
                10.0,
                &mut rng
            )
            .is_some()
        );
    }

    #[test]
    fn tar_floor_flammability_and_floor_drop_reaction() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let tar = harness.content().block_id("tar").expect("tar");
        let slag = harness.content().liquid_id("slag").expect("slag");
        harness.build.grid.tiles.get_mut(4, 4).floor = tar;
        // `Tile.getFlammability` reads the floor's `liquidDrop` (tar → oil 1.2).
        let flammability = harness
            .build
            .grid
            .tiles
            .get(4, 4)
            .get_flammability(&harness.build.content);
        assert!(
            (flammability - 1.2).abs() < 1e-5,
            "flammability={flammability}"
        );
        // Hot slag + flammable oil floor → fire, and no puddle is created.
        let mut rng = SimRng::new(3);
        let result = deposit(
            &mut harness.build.world,
            &harness.build.grid.tiles,
            &harness.build.content,
            4,
            4,
            slag,
            10.0,
            &mut rng,
        );
        assert!(result.is_none(), "floor drop consumes the deposit");
        assert!(crate::combat::fires::has(&harness.build.world, 4, 4));
    }

    #[test]
    fn solid_floor_rejects_puddle() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let space = harness.content().block_id("space").expect("space");
        harness.build.grid.tiles.get_mut(6, 6).floor = space;
        let water = harness.content().liquid_id("water").expect("water");
        let mut rng = SimRng::new(3);
        assert!(
            deposit(
                &mut harness.build.world,
                &harness.build.grid.tiles,
                &harness.build.content,
                6,
                6,
                water,
                10.0,
                &mut rng
            )
            .is_none()
        );
    }

    #[test]
    fn cell_liquid_reacts_with_spread_target() {
        let mut harness = CombatHarness::new(16, 16, 3);
        let neph = harness.content().liquid_id("neoplasm").expect("neoplasm");
        let water = harness.content().liquid_id("water").expect("water");
        let mut rng = SimRng::new(3);
        let e = deposit(
            &mut harness.build.world,
            &harness.build.grid.tiles,
            &harness.build.content,
            8,
            8,
            neph,
            10.0,
            &mut rng,
        )
        .expect("neoplasm puddle");
        let before = harness
            .build
            .world
            .get::<PuddleState>(e)
            .map(|p| p.amount)
            .unwrap_or(0.0);
        // `CellLiquid.react(water)` returns the full amount for its spread target.
        let _ = deposit(
            &mut harness.build.world,
            &harness.build.grid.tiles,
            &harness.build.content,
            8,
            8,
            water,
            5.0,
            &mut rng,
        );
        let after = harness
            .build
            .world
            .get::<PuddleState>(e)
            .map(|p| p.amount)
            .unwrap_or(0.0);
        assert!((after - (before + 5.0)).abs() < 1e-5, "{before} -> {after}");
    }
}
