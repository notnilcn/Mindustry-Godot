// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BaseGenerator` — enemy/ally base decoration (plan 06 §3.10, risk R3).
//!
//! Ported from `maps/generators/BaseGenerator.java`. Operates on the live
//! world through [`StructureCtx`] (H05 T5); `try_place`/`is_taken`/the
//! resource `set` are shared with the Serpulo ruins pass (H05 T6).

use crate::ai::base_registry::{BasePart, BaseResource};
use crate::content::{BlockId, ContentRegistry, ItemId};
use crate::determinism::RngStream;
use crate::determinism::SimRng;
use crate::maps::generators::planet::StructureCtx;
use crate::math::rand_arc::ArcRand;

/// `Seq.getFrac` (`Arc Seq.java`): `items[clamp((int)(f * size), 0, size - 1)]`,
/// `None` when empty.
pub fn get_frac<T>(items: &[T], f: f32) -> Option<&T> {
    if items.is_empty() {
        return None;
    }
    let index = ((f * items.len() as f32) as i32).clamp(0, items.len() as i32 - 1);
    items.get(index as usize)
}

/// The `Rand` calls `BaseGenerator.tryPlace`/ruins make. Implemented for
/// `ArcRand` (enemy bases, Java `Mathf.rand`) and the generator `SimRng`
/// (ruins, `RngStream::MapGen`).
pub trait BaseRand {
    /// Arc `Rand.range(int amount) -> nextInt(amount * 2 + 1) - amount`.
    fn range_int(&mut self, amount: i32) -> i32;

    /// Arc `Rand.chance(double)`.
    fn chance(&mut self, chance: f64) -> bool;

    /// Arc `Rand.random(float max) -> nextFloat() * max`.
    fn random_float(&mut self, max: f32) -> f32;
}

impl BaseRand for ArcRand {
    fn range_int(&mut self, amount: i32) -> i32 {
        ArcRand::range_int(self, amount)
    }

    fn chance(&mut self, chance: f64) -> bool {
        ArcRand::chance(self, chance)
    }

    fn random_float(&mut self, max: f32) -> f32 {
        ArcRand::random_float(self, max)
    }
}

impl BaseRand for SimRng {
    fn range_int(&mut self, amount: i32) -> i32 {
        self.random(RngStream::MapGen, amount * 2 + 1) - amount
    }

    fn chance(&mut self, chance: f64) -> bool {
        SimRng::chance(self, RngStream::MapGen, chance)
    }

    fn random_float(&mut self, max: f32) -> f32 {
        self.next_float(RngStream::MapGen) * max
    }
}

/// The per-tile callback `tryPlace` runs after validation; Java's closure
/// captures `ctx`/`rand` implicitly, the Rust one receives them.
pub type PosCallback<'a> = dyn FnMut(&mut StructureCtx, &mut dyn BaseRand, i32, i32) + 'a;

/// `BaseGenerator.tryPlace` (`BaseGenerator.java:196-262`): tries to place a
/// base part at `(x, y)`; the optional `posc` runs per placed tile after
/// validation (the ruins ore-scrap callback).
///
/// Deviation: Java's callback closes over `ctx` and `rand` implicitly; the
/// Rust callback receives them explicitly (`PosCallback`) so the borrow
/// checker accepts it.
pub fn try_place(
    ctx: &mut StructureCtx,
    part: &BasePart,
    x: i32,
    y: i32,
    team: u8,
    rand: &mut dyn BaseRand,
    mut posc: Option<&mut PosCallback<'_>>,
) -> bool {
    let rotation = rand.range_int(2);
    let axis = (
        (part.schematic.width as f32 / 2.0) as i32,
        (part.schematic.height as f32 / 2.0) as i32,
    );
    let result =
        crate::game::schematics::Schematics::rotate(&part.schematic, rotation, ctx.content);

    let (rx, ry) = rotate_around(
        part.center_x as f32,
        part.center_y as f32,
        axis,
        rotation as f32 * 90.0,
    );
    // Bottom-left schematic corner.
    let cx = x - rx as i32;
    let cy = y - ry as i32;

    for tile in &result.tiles {
        let real_x = tile.x as i32 + cx;
        let real_y = tile.y as i32 + cy;
        if is_taken(ctx, tile.block, real_x, real_y) {
            return false;
        }
        // `tile.block == Blocks.oilExtractor && sumAttribute(Attribute.oil)
        // <= 0.001`: the port's `Attribute` enum has no `oil` yet
        // (`content/registries/weathers.rs`), so the clause is recorded as a
        // deviation; oil extractors only exist in sandbox-only parts.
    }

    // Only do the callback after validation.
    if let Some(posc) = posc.as_mut() {
        for tile in &result.tiles {
            posc(ctx, rand, tile.x as i32 + cx, tile.y as i32 + cy);
        }
    }

    if let Some(BaseResource::Item(item)) = part.required {
        for tile in &result.tiles {
            if !is_drill(ctx.content, tile.block) {
                continue;
            }
            for_each_taken(
                ctx.content,
                tile.block,
                tile.x as i32 + cx,
                tile.y as i32 + cy,
                |ex, ey| {
                    let Some(placed) = ctx.sim.grid.tiles.getn(ex, ey) else {
                        return;
                    };
                    if has_surface(ctx.content, placed.floor) {
                        set(ctx, ex, ey, item);
                    }
                    let rand_x = ex + rand.range_int(1);
                    let rand_y = ey + rand.range_int(1);
                    let clamped = ctx.sim.grid.tiles.getc(rand_x, rand_y);
                    if has_surface(ctx.content, clamped.floor) {
                        set(ctx, rand_x, rand_y, item);
                    }
                },
            );
        }
    }

    let placed = ctx.sim.with_world_ctx(|wctx| {
        crate::game::schematics::place(
            wctx,
            &result,
            cx + result.width / 2,
            cy + result.height / 2,
            team,
            false,
        )
    });
    if placed.is_none() {
        return false;
    }

    // Fill drills with items after placing.
    if let Some(BaseResource::Item(item)) = part.required {
        for tile in &result.tiles {
            if !is_drill(ctx.content, tile.block) {
                continue;
            }
            let Some(entity) = ctx
                .sim
                .grid
                .tiles
                .getn(tile.x as i32 + cx, tile.y as i32 + cy)
                .and_then(|t| t.build)
            else {
                continue;
            };
            let Some(block) = ctx
                .sim
                .ecs
                .0
                .get::<crate::ecs::BuildingComp>(entity)
                .map(|comp| comp.block)
            else {
                continue;
            };
            if block != tile.block {
                continue;
            }
            let capacity = ctx
                .content
                .block(block)
                .map(|def| def.item_capacity)
                .unwrap_or(0);
            if let Some(mut module) = ctx
                .sim
                .ecs
                .0
                .get_mut::<crate::world::modules::ItemModule>(entity)
            {
                module.add(item, capacity.max(0), capacity);
            }
        }
    }

    true
}

/// `Vec2.rotateAround(origin, degrees)` (`Vec2.java:463-468`).
fn rotate_around(x: f32, y: f32, origin: (i32, i32), degrees: f32) -> (f32, f32) {
    let (ox, oy) = (origin.0 as f32, origin.1 as f32);
    let dx = x - ox;
    let dy = y - oy;
    let rad = degrees.to_radians();
    let (sin, cos) = rad.sin_cos();
    (ox + dx * cos - dy * sin, oy + dx * sin + dy * cos)
}

/// `BaseGenerator.set(tile, item)`: `bases.ores` -> overlay, else
/// `bases.oreFloors` -> floor.
fn set(ctx: &mut StructureCtx, x: i32, y: i32, item: ItemId) {
    let overlay = ctx.bases.and_then(|bases| bases.ores.get(&item).copied());
    let floor = ctx
        .bases
        .and_then(|bases| bases.ore_floors.get(&item).copied());
    if overlay.is_none() && floor.is_none() {
        return;
    }
    let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
        return;
    };
    ctx.sim.with_world_ctx(|wctx| {
        if let Some(overlay) = overlay {
            wctx.set_overlay(x, y, overlay);
        } else if let Some(floor) = floor {
            wctx.set_floor(x, y, floor);
        }
    });
}

/// `BaseGenerator.isTaken` (`BaseGenerator.java:278-299`).
fn is_taken(ctx: &StructureCtx, block: BlockId, x: i32, y: i32) -> bool {
    let Some(def) = ctx.content.block(block) else {
        return true;
    };
    let wx = x as f32 * 8.0 + def.offset;
    let wy = y as f32 * 8.0 + def.offset;
    if any_enemy_core_within(ctx, wx, wy, ctx.rules.enemy_core_build_radius + 8.0) {
        return true;
    }

    let offset = -(def.size - 1) / 2;
    for dx in -1..def.size + 1 {
        for dy in -1..def.size + 1 {
            if overlaps(ctx, dx + offset + x, dy + offset + y) {
                return true;
            }
        }
    }
    false
}

/// `TeamData.anyEnemyCoresWithin(team, x, y, range)` for the wave team: a core
/// of any team whose rules `protectCores`, within `range` world units.
fn any_enemy_core_within(ctx: &StructureCtx, x: f32, y: f32, range: f32) -> bool {
    let mut cores: Vec<(u32, u8, f32, f32)> = ctx
        .sim
        .ecs
        .0
        .iter_entities()
        .filter(|entity| entity.contains::<crate::world::blocks::storage::CoreBuild>())
        .filter_map(|entity| {
            let team = entity.get::<crate::entities::comp::TeamComp>()?.team;
            let pos = entity.get::<crate::entities::comp::Pos>()?;
            Some((entity.id().index().index(), team, pos.x, pos.y))
        })
        .collect();
    cores.sort_unstable_by_key(|(index, ..)| *index);
    for (_, core_team, px, py) in cores {
        if core_team == ctx.wave_team || !ctx.rules.team_rule(core_team).protect_cores {
            continue;
        }
        let dx = px - x;
        let dy = py - y;
        if dx * dx + dy * dy <= range * range {
            return true;
        }
    }
    false
}

/// `BaseGenerator.overlaps` (`BaseGenerator.java:300-305`): out of bounds,
/// `!alwaysReplace` or any darkness (`World.getDarkness`).
fn overlaps(ctx: &StructureCtx, x: i32, y: i32) -> bool {
    let grid = &ctx.sim.grid;
    if !grid.tiles.in_bounds(x, y) {
        return true;
    }
    let tile = grid.tile(x, y);
    let always_replace = ctx
        .content
        .block(tile.block)
        .map(|def| def.always_replace)
        .unwrap_or(false);
    if !always_replace {
        return true;
    }
    crate::world::darkness::get_darkness(grid, ctx.content, x, y) > 0.0
}

/// The port's drill kinds (`Drill`/`BeamDrill`/`BurstDrill`/`WallCrafter`).
fn is_drill(content: &ContentRegistry, block: BlockId) -> bool {
    content.block(block).is_some_and(|def| {
        matches!(
            def.kind,
            crate::content::registries::blocks::BlockKind::Drill
                | crate::content::registries::blocks::BlockKind::BeamDrill
                | crate::content::registries::blocks::BlockKind::BurstDrill
                | crate::content::registries::blocks::BlockKind::WallCrafter
        )
    })
}

/// `Block.iterateTaken(x, y, cons)` (`Block.java:968-983`): every grid position
/// in the block footprint.
fn for_each_taken(
    content: &ContentRegistry,
    block: BlockId,
    x: i32,
    y: i32,
    mut f: impl FnMut(i32, i32),
) {
    let size = content.block(block).map(|def| def.size.max(1)).unwrap_or(1);
    let offset = -(size - 1) / 2;
    for dx in 0..size {
        for dy in 0..size {
            f(dx + offset + x, dy + offset + y);
        }
    }
}

fn has_surface(content: &ContentRegistry, floor: BlockId) -> bool {
    content
        .block(floor)
        .is_some_and(crate::maps::filters::block_info::has_surface)
}
