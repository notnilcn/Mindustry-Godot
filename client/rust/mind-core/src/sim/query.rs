// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Determinism-safe read helpers shared by the opt-in sim runtimes: the
//! `(EntitySeq, entity.index())` ordering every entity list must use, the
//! `TeamComp`/`UnitTypeComp` lookups, the tile-to-world-pixel conversion and
//! the plain squared-distance expression.
//! Upstream: none (port-only; internal dedupe of existing sim code, so the
//! bodies copy the original expressions verbatim).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::content::registries::units::UnitTypeDef;
use crate::entities::comp::TeamComp;
use crate::entities::comp::unit::comp::UnitTypeComp;
use crate::world::TilePos;

/// World pixels per tile (`config::TILESIZE`).
const TILE_SIZE: f32 = crate::config::TILESIZE as f32;

/// Sorts the collected `(seq, entity)` pairs by the exact
/// `(seq, entity.index())` key the sim's determinism sorts use and returns the
/// sorted entities.
///
/// The keys are unique in a world, so this stable `sort_by_key` and an
/// `sort_unstable_by_key` with the same key produce the same order. Callers
/// that fold the seq (the runtime checksums) read it back from the entity.
pub(crate) fn sorted_by_seq(it: impl Iterator<Item = (u64, Entity)>) -> Vec<Entity> {
    let mut items: Vec<(u64, Entity)> = it.collect();
    items.sort_by_key(|(seq, entity)| (*seq, entity.index()));
    items.into_iter().map(|(_, entity)| entity).collect()
}

/// The entity's [`TeamComp`] team (`world.get::<TeamComp>(e).map(|t| t.team)`).
pub(crate) fn team_of(world: &World, e: Entity) -> Option<u8> {
    world.get::<TeamComp>(e).map(|t| t.team)
}

/// The entity's unit definition: its [`UnitTypeComp`] id resolved through
/// `content` (`get::<UnitTypeComp>` then `content.unit(id)`).
pub(crate) fn unit_def<'a>(
    world: &World,
    content: &'a ContentRegistry,
    e: Entity,
) -> Option<&'a UnitTypeDef> {
    let type_id = world.get::<UnitTypeComp>(e)?.type_id;
    content.unit(type_id)
}

/// `Tile.worldx()`/`worldy()` (Tile.java:166-171): the tile's corner-based
/// world coordinate, `x * tilesize`. The tile spans `[x*8 - 4, x*8 + 4)` and a
/// building on it sits at `drawx = worldx + block.offset`.
pub(crate) fn tile_world(tile: TilePos) -> (f32, f32) {
    (tile.x() as f32 * TILE_SIZE, tile.y() as f32 * TILE_SIZE)
}

/// Squared distance, verbatim `dx * dx + dy * dy` (no `mul_add`, `hypot`,
/// `f64` widening or reassociation; Rust does not contract to FMA).
pub(crate) fn dist2(dx: f32, dy: f32) -> f32 {
    dx * dx + dy * dy
}
