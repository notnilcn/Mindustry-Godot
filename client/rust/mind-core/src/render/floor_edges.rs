// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Floor edge blending (`Floor.drawEdges` / `drawNonLayer` / `drawBlended`),
//! Godot-free (J02 T1).
//!
//! Ported from `world/blocks/environment/Floor.java:343-440` and
//! `graphics/FloorRenderer.java:318-337`. The functions here return the
//! **ordered list of edge sprites** for one tile; the Godot side
//! (`mind-gdext::render::floor`) resolves each sprite to the 3×3 split cell of
//! `<blendGroup>-edge` and emits the quad.
//!
//! Java semantics, in draw order:
//! 1. `drawEdges` (`:363-386`): for every d8 neighbour whose *floor or overlay*
//!    is a different blend group with a higher `realBlendId`, gather it as a
//!    "blender"; sort blenders by id with the tile's own floor last
//!    (`drawBlended`, `:388-407`); draw each blender's edge cell for every
//!    direction pointing at it.
//! 2. `drawNonLayer` (`:343-361`): for cache layers the tile's own floor does
//!    not belong to, draw every neighbouring floor on that layer (no `doEdge`
//!    check, `checkId = false`).
//!
//! `realBlendId` (`:409-414`) packs the (overlay, floor) pair for liquid floors
//! with non-ore overlays so no edges bleed between different liquid/overlay
//! combinations.

use smallvec::SmallVec;

use crate::content::{BlockId, BlockKind, ContentRegistry};
use crate::render::layer::CacheLayerId;
use crate::world::{Tile, WorldGrid};

/// `Geometry.d8` order (arc `Geometry.java:25-34`): E, NE, N, NW, W, SW, S, SE.
pub const D8: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// One ordered edge sprite: a cell of the `<blendGroup>-edge` 3×3 split
/// (`Floor.edge(x, y, rx, ry)` → `edges[x][y][rx][2 - ry]`, `Floor.java:421-423`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeQuad {
    /// Blend group whose `<name>-edge` region is drawn (`Floor.edges`, `:416-419`).
    pub blend_group: BlockId,
    /// Cell column in the 3×3 split (`1 - point.x`).
    pub col: u8,
    /// Cell row counted from the image top (`2 - ry` where `ry = 1 - point.y`).
    pub row: u8,
}

impl EdgeQuad {
    /// The cell for a neighbour at d8 `point`: `edge(x, y, 1 - dx, 1 - dy)`.
    fn cell(blend_group: BlockId, point: (i32, i32)) -> Self {
        Self {
            blend_group,
            col: (1 - point.0) as u8,
            row: (1 + point.1) as u8,
        }
    }
}

/// `world.tile(x, y)` with the nullable `Tiles.getn` bounds check.
pub fn tile_at(world: &WorldGrid, x: i32, y: i32) -> Option<&Tile> {
    world.tiles.getn(x, y)
}

/// Whether a floor's `<blendGroup>-edge` region exists (`Floor.load:170`;
/// `<name>-edge` is generated for non-overlay floors with `blendGroup == this`
/// and `drawEdgeOut`, `Generators.java:827-847`).
pub fn edges_available(content: &ContentRegistry, floor: BlockId) -> bool {
    content
        .block(floor)
        .map(|def| {
            content
                .block(def.blend_group_id())
                .is_some_and(|group| group.has_edge)
        })
        .unwrap_or(false)
}

/// `Floor.realBlendId(tile)` (`Floor.java:409-414`) for the floor instance
/// `this_floor`: the packed negative id when `tile` is a liquid with a
/// non-ore, non-air overlay, else `this_floor`'s `blendId`.
pub fn real_blend_id(content: &ContentRegistry, this_floor: BlockId, tile: &Tile) -> i32 {
    let floor = content.block(tile.floor);
    let overlay = content.block(tile.overlay);
    if let (Some(floor), Some(overlay)) = (floor, overlay)
        && floor.is_liquid
        && tile.overlay != BlockId::AIR
        && overlay.kind != BlockKind::OreBlock
    {
        // `-((overlay.blendId) | (floor.blendId << 15))`.
        let overlay_id = overlay.blend_group_id().raw() as i32;
        let floor_id = floor.blend_group_id().raw() as i32;
        return -(overlay_id | (floor_id << 15));
    }
    content
        .block(this_floor)
        .map(|def| def.blend_group_id().raw() as i32)
        .unwrap_or(0)
}

/// `Floor.doEdge(tile, otherTile, other)` (`Floor.java:426-428`):
/// `other.realBlendId(otherTile) > realBlendId(tile) || edges(tile) == null`.
pub fn do_edge(
    content: &ContentRegistry,
    tile: &Tile,
    other_tile: &Tile,
    other: BlockId,
    this_floor: BlockId,
) -> bool {
    real_blend_id(content, other, other_tile) > real_blend_id(content, this_floor, tile)
        || !edges_available(content, this_floor)
}

/// The neighbour floor considered by `drawEdges`/`drawBlended`
/// (`Floor.java:375,399`): `this == tile.floor() || other.overlay() == air ?
/// other.floor() : other.overlay()`.
fn neighbour_floor(tile: &Tile, other: &Tile, this_floor: BlockId) -> BlockId {
    if this_floor == tile.floor || other.overlay == BlockId::AIR {
        other.floor
    } else {
        other.overlay
    }
}

/// `Floor.drawBlended(tile, checkId)` (`Floor.java:388-407`) over the gathered
/// `blenders`/`dirs`.
fn draw_blended(
    content: &ContentRegistry,
    world: &WorldGrid,
    tile: &Tile,
    this_floor: BlockId,
    mut blenders: SmallVec<[BlockId; 8]>,
    dirs: [u16; 8],
    check_id: bool,
) -> SmallVec<[EdgeQuad; 8]> {
    // "prioritize the 'actual' floor over other floors to fix weird square
    // corners": the tile's own floor sorts last when drawing another floor's
    // edges (the overlay pass), key `id + (tile.floor() != this && a ==
    // tile.floor() ? 99999 : 0)`.
    let own = tile.floor;
    blenders.sort_by_key(|block| {
        let bump = if this_floor != own && *block == own {
            99999
        } else {
            0
        };
        block.raw() as i32 + bump
    });

    let mut out = SmallVec::new();
    for block in blenders {
        // `edges(x, y)` on `block` returns its blend group's edges.
        let Some(group) = content.block(block).map(|def| def.blend_group_id()) else {
            continue;
        };
        for point in D8 {
            let Some(other) = tile_at(world, tile.x as i32 + point.0, tile.y as i32 + point.1)
            else {
                continue;
            };
            let ob = neighbour_floor(tile, other, this_floor);
            if ob == block && (!check_id || dirs[index_of(point)] == block.raw()) {
                out.push(EdgeQuad::cell(group, point));
            }
        }
    }
    out
}

/// The d8 loop index of `point` (for the `dirs` table).
fn index_of(point: (i32, i32)) -> usize {
    D8.iter().position(|p| *p == point).unwrap_or(0)
}

/// `Floor.drawEdges(tile)` (`Floor.java:363-386`) for the floor instance
/// `this_floor` (the tile's floor, or its overlay floor during `drawOverlay`).
pub fn draw_edges(
    content: &ContentRegistry,
    world: &WorldGrid,
    tile: &Tile,
    this_floor: BlockId,
) -> SmallVec<[EdgeQuad; 8]> {
    let real_cache = content
        .block(tile.floor)
        .map(|def| def.cache_layer)
        .unwrap_or(CacheLayerId::Normal);
    let mut blenders: SmallVec<[BlockId; 8]> = SmallVec::new();
    let mut dirs = [0u16; 8];

    for (i, point) in D8.iter().enumerate() {
        let Some(other) = tile_at(world, tile.x as i32 + point.0, tile.y as i32 + point.1) else {
            continue;
        };
        let ob = neighbour_floor(tile, other, this_floor);
        let Some(ob_def) = content.block(ob) else {
            continue;
        };
        // `ob.drawEdgeOut && doEdge(...) && other.floor().cacheLayer ==
        // realCache && ob.edges(...) != null`.
        let other_floor_layer = content
            .block(other.floor)
            .map(|def| def.cache_layer)
            .unwrap_or(CacheLayerId::Normal);
        if ob_def.draw_edge_out
            && do_edge(content, tile, other, ob, this_floor)
            && other_floor_layer == real_cache
            && edges_available(content, ob)
        {
            if !blenders.contains(&ob) {
                blenders.push(ob);
            }
            dirs[i] = ob.raw();
        }
    }

    draw_blended(content, world, tile, this_floor, blenders, dirs, true)
}

/// `Floor.drawNonLayer(tile, layer)` (`Floor.java:343-361`): draws the edges of
/// neighbouring floors that live on `layer` when the tile's own floor does not.
pub fn draw_non_layer(
    content: &ContentRegistry,
    world: &WorldGrid,
    tile: &Tile,
    layer: CacheLayerId,
) -> SmallVec<[EdgeQuad; 8]> {
    let this_floor = tile.floor;
    let mut blenders: SmallVec<[BlockId; 8]> = SmallVec::new();
    let mut dirs = [0u16; 8];

    for (i, point) in D8.iter().enumerate() {
        let Some(other) = tile_at(world, tile.x as i32 + point.0, tile.y as i32 + point.1) else {
            continue;
        };
        let floor = other.floor;
        let Some(def) = content.block(floor) else {
            continue;
        };
        // `other.floor().drawEdgeOut && other.floor().cacheLayer == layer &&
        // other.floor().edges(tile) != null`.
        if def.draw_edge_out && def.cache_layer == layer && edges_available(content, floor) {
            if !blenders.contains(&floor) {
                blenders.push(floor);
            }
            dirs[i] = floor.raw();
        }
    }

    // `drawBlended(tile, false)`: `checkId = false`, and `this ==
    // tile.floor()` so no sort bump.
    draw_blended(content, world, tile, this_floor, blenders, dirs, false)
}

/// `Floor.drawOverlay` (`Floor.java:304-309`) resolves the overlay floor for
/// the overlay pass: `tile.overlay()` when it is neither air nor the drawing
/// floor, else nothing.
pub fn overlay_floor(tile: &Tile, this_floor: BlockId) -> Option<BlockId> {
    if tile.overlay != BlockId::AIR && tile.overlay != this_floor {
        Some(tile.overlay)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::world::WorldGrid;

    fn world_with(
        registry: &ContentRegistry,
        width: i32,
        height: i32,
        entries: &[(i32, i32, &str)],
    ) -> WorldGrid {
        let mut grid = WorldGrid::new(width, height);
        for (x, y, name) in entries {
            let id = registry.block_id(name).expect("block exists");
            grid.tiles.get_mut(*x, *y).floor = id;
        }
        grid
    }

    /// A stone tile with a sand neighbour to the east draws the sand edge cell
    /// `[1 - dx][1 + dy] = [0][1]` (`Floor.drawEdges`, `Floor.java:401-403`).
    #[test]
    fn sand_east_of_stone_draws_sand_edge() {
        let registry = test_registry();
        let sand = registry.block_id("sand-floor").unwrap();
        let stone = registry.block_id("stone").unwrap();
        // Edge order follows blend ids: sand (created after stone) draws onto
        // stone, never the reverse.
        assert!(sand.raw() > stone.raw());
        let world = world_with(&registry, 3, 1, &[(0, 0, "stone"), (1, 0, "sand-floor")]);
        let tile = world.tile(0, 0);
        let edges = draw_edges(&registry, &world, tile, tile.floor);
        assert_eq!(
            edges.as_slice(),
            &[EdgeQuad {
                blend_group: sand,
                col: 0,
                row: 1
            }]
        );
        // The sand tile itself draws nothing: stone's blend id is lower.
        let sand_tile = world.tile(1, 0);
        assert!(draw_edges(&registry, &world, sand_tile, sand_tile.floor).is_empty());
    }

    /// Diagonal (NE) sand neighbour: cell `[0][2]`.
    #[test]
    fn sand_north_east_of_stone_draws_diagonal_cell() {
        let registry = test_registry();
        let sand = registry.block_id("sand-floor").unwrap();
        let world = world_with(&registry, 2, 2, &[(0, 0, "stone"), (1, 1, "sand-floor")]);
        let tile = world.tile(0, 0);
        let edges = draw_edges(&registry, &world, tile, tile.floor);
        assert_eq!(
            edges.as_slice(),
            &[EdgeQuad {
                blend_group: sand,
                col: 0,
                row: 2
            }]
        );
    }

    /// `core-zone` (late id) stripes draw onto the surrounding `stone`, one
    /// cell per neighbour direction; `stone` never draws onto `core-zone`.
    #[test]
    fn core_zone_stripes_draw_onto_stone() {
        let registry = test_registry();
        let core_zone = registry.block_id("core-zone").unwrap();
        let stone = registry.block_id("stone").unwrap();
        assert!(core_zone.raw() > stone.raw());
        // Stone tile at (1,1) with core-zone to E and S: cells [0][1] and
        // [1][0], sorted by blend id (same blender, so d8 order).
        let world = world_with(
            &registry,
            3,
            3,
            &[(1, 1, "stone"), (2, 1, "core-zone"), (1, 0, "core-zone")],
        );
        let tile = world.tile(1, 1);
        let edges = draw_edges(&registry, &world, tile, tile.floor);
        assert_eq!(
            edges.as_slice(),
            &[
                EdgeQuad {
                    blend_group: core_zone,
                    col: 0,
                    row: 1
                },
                EdgeQuad {
                    blend_group: core_zone,
                    col: 1,
                    row: 0
                },
            ]
        );
        // The core-zone tile draws no stone edges on itself.
        let zone = world.tile(2, 1);
        assert!(draw_edges(&registry, &world, zone, zone.floor).is_empty());
    }

    /// Water is a liquid on its own cache layer: `drawEdges` only considers
    /// neighbours on the same layer, so a normal-layer `sand-floor` draws onto
    /// the water tile through the tile's *normal-layer* `drawNonLayer` bake
    /// (`FloorRenderer.java:325`).
    #[test]
    fn sand_draws_onto_water_via_normal_layer() {
        let registry = test_registry();
        let sand = registry.block_id("sand-floor").unwrap();
        let water = registry.block_id("shallow-water").unwrap();
        assert!(sand.raw() > water.raw());
        assert_eq!(
            registry.block(water).map(|def| def.cache_layer),
            Some(CacheLayerId::Water)
        );
        let world = world_with(
            &registry,
            2,
            1,
            &[(0, 0, "shallow-water"), (1, 0, "sand-floor")],
        );
        let tile = world.tile(0, 0);
        // Same-layer `drawEdges`: the sand neighbour is on another cache layer.
        assert!(draw_edges(&registry, &world, tile, tile.floor).is_empty());
        // The normal-layer bake of the water tile pulls the sand edge in.
        let edges = draw_non_layer(&registry, &world, tile, CacheLayerId::Normal);
        assert_eq!(
            edges.as_slice(),
            &[EdgeQuad {
                blend_group: sand,
                col: 0,
                row: 1
            }]
        );
    }

    /// A liquid with a non-ore overlay takes the packed negative `realBlendId`
    /// (`Floor.java:409-414`): a plain-water neighbour's edge draws onto it,
    /// while it never draws onto the plain-water tile.
    #[test]
    fn liquid_overlay_packs_real_blend_id() {
        let registry = test_registry();
        let water = registry.block_id("shallow-water").unwrap();
        let overlay = registry.block_id("spawn").unwrap();
        let mut grid = world_with(
            &registry,
            2,
            1,
            &[(0, 0, "shallow-water"), (1, 0, "shallow-water")],
        );
        grid.tiles.get_mut(0, 0).overlay = overlay;
        let tile = grid.tile(0, 0);
        let packed = real_blend_id(&registry, water, tile);
        assert!(packed < 0);
        let spawn_def = registry.block(overlay);
        assert_eq!(spawn_def.map(|def| def.kind), Some(BlockKind::SpawnBlock));
        // The packed id is lower than the plain water blend id, so the plain
        // water neighbour's edge draws onto the water+spawn tile.
        let edges = draw_edges(&registry, &grid, tile, tile.floor);
        assert_eq!(
            edges.as_slice(),
            &[EdgeQuad {
                blend_group: water,
                col: 0,
                row: 1
            }]
        );
        // The plain water tile takes the positive id: no edge back.
        let plain = grid.tile(1, 0);
        assert!(draw_edges(&registry, &grid, plain, plain.floor).is_empty());
    }

    /// `drawNonLayer` draws neighbour floors from another cache layer with
    /// `checkId = false` (every direction, sorted by id).
    #[test]
    fn non_layer_draws_neighbour_floor_edges() {
        let registry = test_registry();
        let water = registry.block_id("shallow-water").unwrap();
        // A stone tile next to shallow water: the water-layer bake pulls the
        // water neighbour's edge in with `drawNonLayer` (`Floor.java:343-361`;
        // `FloorRenderer.java:325`).
        let world = world_with(&registry, 2, 1, &[(0, 0, "stone"), (1, 0, "shallow-water")]);
        let tile = world.tile(0, 0);
        let edges = draw_non_layer(&registry, &world, tile, CacheLayerId::Water);
        assert_eq!(
            edges.as_slice(),
            &[EdgeQuad {
                blend_group: water,
                col: 0,
                row: 1
            }]
        );
        // `stone`'s own layer is normal, not water.
        assert!(draw_non_layer(&registry, &world, tile, CacheLayerId::Normal).is_empty());
    }

    /// `edges_available` follows the blend group and the generated region set.
    #[test]
    fn edge_availability_follows_blend_group() {
        let registry = test_registry();
        let stone = registry.block_id("stone").unwrap();
        assert!(edges_available(&registry, stone));
        let crater = registry.block_id("crater-stone").unwrap();
        // `crater-stone` blends with `stone` (`Blocks.java:352`).
        assert_eq!(
            registry.block(crater).map(|def| def.blend_group_id()),
            Some(stone)
        );
        assert!(edges_available(&registry, crater));
        // `space`/`metal-tiles-1` disable `drawEdgeOut`: no generated region.
        let space = registry.block_id("space").unwrap();
        assert!(!edges_available(&registry, space));
        let metal = registry.block_id("metal-tiles-1").unwrap();
        assert!(!edges_available(&registry, metal));
        // Ores are overlay floors: no `<ore>-edge` region upstream.
        let ore = registry.block_id("ore-copper").unwrap();
        assert!(!edges_available(&registry, ore));
    }
}
