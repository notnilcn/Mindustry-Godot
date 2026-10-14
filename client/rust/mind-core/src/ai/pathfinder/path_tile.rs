// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PathTile` bit packing (plan 11 §3.7, frozen layout).
//!
//! Ported from `core/src/mindustry/ai/Pathfinder.java` (`@Struct PathTileStruct`).
//! The 32-bit layout is a save/hash ABI; do not reorder bits.

use crate::content::{BlockId, ContentRegistry};
use crate::world::WorldGrid;

/// Health bits `0..8` (block hp / 40, capped at 80).
pub const HEALTH_MASK: u32 = 0xff;
/// Team bits `8..16` (`255` = derelict under core capture).
pub const TEAM_SHIFT: u32 = 8;
/// Solid block bit 16.
pub const BIT_SOLID: u32 = 1 << 16;
/// Liquid floor bit 17.
pub const BIT_LIQUID: u32 = 1 << 17;
/// Leg-solid bit 18.
pub const BIT_LEG_SOLID: u32 = 1 << 18;
/// Near-liquid bit 19.
pub const BIT_NEAR_LIQUID: u32 = 1 << 19;
/// Near-ground bit 20.
pub const BIT_NEAR_GROUND: u32 = 1 << 20;
/// Near-solid bit 21.
pub const BIT_NEAR_SOLID: u32 = 1 << 21;
/// Near-leg-solid bit 22.
pub const BIT_NEAR_LEG_SOLID: u32 = 1 << 22;
/// Deep-liquid bit 23.
pub const BIT_DEEP: u32 = 1 << 23;
/// Damaging-floor bit 24.
pub const BIT_DAMAGES: u32 = 1 << 24;
/// All-deep bit 25.
pub const BIT_ALL_DEEP: u32 = 1 << 25;
/// Near-deep bit 26.
pub const BIT_NEAR_DEEP: u32 = 1 << 26;
/// Team-passable (no enemy block) bit 27.
pub const BIT_TEAM_PASSABLE: u32 = 1 << 27;

/// Packed pathfinding tile (frozen `@Struct`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PathTile(pub u32);

impl PathTile {
    /// Health bits (`block.hp / 40`).
    pub const fn health(self) -> u8 {
        (self.0 & HEALTH_MASK) as u8
    }

    /// Team bits.
    pub const fn team(self) -> u8 {
        ((self.0 >> TEAM_SHIFT) & 0xff) as u8
    }

    /// Whether the tile holds a solid block.
    pub const fn solid(self) -> bool {
        self.0 & BIT_SOLID != 0
    }

    /// Whether the floor is deep liquid.
    pub const fn deep(self) -> bool {
        self.0 & BIT_DEEP != 0
    }

    /// Whether the tile is a damaging floor.
    pub const fn damages(self) -> bool {
        self.0 & BIT_DAMAGES != 0
    }

    /// Whether the floor is liquid (`PathTile.liquid`).
    pub const fn liquid(self) -> bool {
        self.0 & BIT_LIQUID != 0
    }

    /// Whether legs cannot pass (`PathTile.legSolid`).
    pub const fn leg_solid(self) -> bool {
        self.0 & BIT_LEG_SOLID != 0
    }

    /// Whether a neighbour is a deep liquid (`PathTile.nearLiquid`).
    pub const fn near_liquid(self) -> bool {
        self.0 & BIT_NEAR_LIQUID != 0
    }

    /// Whether a neighbour is not liquid (`PathTile.nearGround`).
    pub const fn near_ground(self) -> bool {
        self.0 & BIT_NEAR_GROUND != 0
    }

    /// Whether a neighbour is solid (`PathTile.nearSolid`).
    pub const fn near_solid(self) -> bool {
        self.0 & BIT_NEAR_SOLID != 0
    }

    /// Whether a neighbour is leg-solid (`PathTile.nearLegSolid`).
    pub const fn near_leg_solid(self) -> bool {
        self.0 & BIT_NEAR_LEG_SOLID != 0
    }

    /// Whether the tile and its diagonal neighbours are all deep
    /// (`PathTile.allDeep`).
    pub const fn all_deep(self) -> bool {
        self.0 & BIT_ALL_DEEP != 0
    }

    /// Whether a neighbour is deep (`PathTile.nearDeep`).
    pub const fn near_deep(self) -> bool {
        self.0 & BIT_NEAR_DEEP != 0
    }

    /// Whether the block is team-passable (`Block.teamPassable`; `AutoDoor`).
    pub const fn team_passable(self) -> bool {
        self.0 & BIT_TEAM_PASSABLE != 0
    }

    /// Raw packed value.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// `PathTile.get(...)` (Pathfinder.java:267-282) with the upstream argument
    /// order. Prefer [`pack_tile`] for world data.
    #[allow(clippy::too_many_arguments)]
    pub fn from_bits(
        health: u8,
        team: u8,
        solid: bool,
        liquid: bool,
        leg_solid: bool,
        near_liquid: bool,
        near_ground: bool,
        near_solid: bool,
        near_leg_solid: bool,
        deep: bool,
        damages: bool,
        all_deep: bool,
        near_deep: bool,
        team_passable: bool,
    ) -> Self {
        let mut bits = (health as u32) & HEALTH_MASK;
        bits |= ((team as u32) << TEAM_SHIFT) & (0xff << TEAM_SHIFT);
        let flag = |on: bool, bit: u32| if on { bit } else { 0 };
        bits |= flag(solid, BIT_SOLID);
        bits |= flag(liquid, BIT_LIQUID);
        bits |= flag(leg_solid, BIT_LEG_SOLID);
        bits |= flag(near_liquid, BIT_NEAR_LIQUID);
        bits |= flag(near_ground, BIT_NEAR_GROUND);
        bits |= flag(near_solid, BIT_NEAR_SOLID);
        bits |= flag(near_leg_solid, BIT_NEAR_LEG_SOLID);
        bits |= flag(deep, BIT_DEEP);
        bits |= flag(damages, BIT_DAMAGES);
        bits |= flag(all_deep, BIT_ALL_DEEP);
        bits |= flag(near_deep, BIT_NEAR_DEEP);
        bits |= flag(team_passable, BIT_TEAM_PASSABLE);
        Self(bits)
    }

    /// Builds a tile from explicit parts (test/port helper).
    pub fn from_parts(health: u8, team: u8, solid: bool, deep: bool, damages: bool) -> Self {
        let mut bits = (health as u32) & HEALTH_MASK;
        bits |= ((team as u32) << TEAM_SHIFT) & (0xff << TEAM_SHIFT);
        bits |= if solid { BIT_SOLID } else { 0 };
        bits |= if deep { BIT_DEEP } else { 0 };
        bits |= if damages { BIT_DAMAGES } else { 0 };
        Self(bits)
    }
}

/// Per-tile building facts `packTile` needs that the grid does not carry: the
/// port keeps a building's team/health/kind on the ECS entity
/// (`BuildingComp.team`/`.health`, `tile.block() instanceof CoreBlock`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileBuilding {
    /// `Tile.getTeamID()`: the building's team.
    pub team: u8,
    /// `BuildingComp.health`.
    pub health: f32,
    /// `tile.block() instanceof CoreBlock`.
    pub core: bool,
}

/// Neighbour order (`Geometry.d4`).
pub const D4: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
/// Diagonal edge order (`Geometry.d8edge`).
const D8_EDGE: [(i32, i32); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

/// Reads the live building on tile `(x, y)` (None when the tile is empty).
pub fn building_at(
    world: &bevy_ecs::world::World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    x: i32,
    y: i32,
) -> Option<TileBuilding> {
    let tile = grid.tiles.get(x, y);
    let entity = tile.build?;
    let team = world
        .get::<crate::entities::comp::TeamComp>(entity)
        .map(|comp| comp.team)
        .unwrap_or(0);
    let health = world
        .get::<crate::entities::comp::Health>(entity)
        .map(|health| health.health)
        .unwrap_or(0.0);
    Some(TileBuilding {
        team,
        health,
        core: content
            .block(tile.block)
            .is_some_and(|def| def.kind == crate::content::BlockKind::CoreBlock),
    })
}

/// `Pathfinder.packTile(Tile)` (Pathfinder.java:220-284).
///
/// `tiles` is the array being built: like upstream, a solid tile ORs
/// `BIT_NEAR_SOLID` into its four neighbours.
pub fn pack_tile(
    grid: &WorldGrid,
    content: &ContentRegistry,
    core_capture: bool,
    x: i32,
    y: i32,
    building: Option<TileBuilding>,
    tiles: &mut [PathTile],
) -> PathTile {
    let tile = grid.tiles.get(x, y);
    let solid = tile.solid_full(content);
    let deep = tile.is_deep(content);
    let team_passable = content
        .block(tile.block)
        .is_some_and(|def| def.team_passable);
    let mut near_liquid = false;
    let mut near_solid = false;
    let mut near_leg_solid = false;
    let mut near_ground = false;
    let mut all_deep = deep;
    let mut near_deep = deep;

    for (dx, dy) in D4 {
        let (ox, oy) = (x + dx, y + dy);
        if !grid.tiles.in_bounds(ox, oy) {
            continue;
        }
        let other = grid.tiles.get(ox, oy);
        let other_floor = content.block(other.floor);
        let other_solid = other.solid_full(content);
        let other_deep = other.is_deep(content);
        if other_floor.is_some_and(|def| def.is_liquid) && other_deep {
            near_liquid = true;
        }
        if other_solid
            && !content
                .block(other.block)
                .is_some_and(|def| def.team_passable)
        {
            near_solid = true;
        }
        if !other_floor.is_some_and(|def| def.is_liquid) {
            near_ground = true;
        }
        if other_deep {
            near_deep = true;
        } else {
            all_deep = false;
        }
        if other.leg_solid(content) {
            near_leg_solid = true;
        }
        // other tile is now near solid
        if solid && !team_passable {
            let index = grid.tiles.index(ox, oy);
            if index < tiles.len() {
                tiles[index].0 |= BIT_NEAR_SOLID;
            }
        }
    }

    // check diagonals for allDeep
    if all_deep {
        for (dx, dy) in D8_EDGE {
            let (ox, oy) = (x + dx, y + dy);
            if !grid.tiles.in_bounds(ox, oy) {
                continue;
            }
            if !grid.tiles.get(ox, oy).is_deep(content) {
                all_deep = false;
                break;
            }
        }
    }

    // `tile.build == null || !solid || tile.block() instanceof CoreBlock ? 0
    //  : Math.min((int)(tile.build.health / 40), 80)`
    let (mut team, health) = match building {
        Some(build) if solid && !build.core => {
            let scaled = (build.health / 40.0) as i32;
            (build.team, scaled.clamp(0, 80) as u8)
        }
        Some(build) => (build.team, 0),
        None => (0, 0),
    };
    // `tid == 0 && tile.build != null && state.rules.coreCapture ? 255 : tid`
    if team == 0 && building.is_some() && core_capture {
        team = 255;
    }

    PathTile::from_bits(
        health,
        team,
        solid,
        content.block(tile.floor).is_some_and(|def| def.is_liquid) && !tile.solid(content),
        tile.leg_solid(content),
        near_liquid,
        near_ground,
        near_solid,
        near_leg_solid,
        deep,
        content
            .block(tile.floor)
            .is_some_and(|def| crate::world::tile::floor_damages(def, content)),
        all_deep,
        near_deep,
        team_passable,
    )
}

/// Builds the world's packed pathfinding tiles (`WorldLoadEvent` loop,
/// Pathfinder.java:124-128) from the live grid + ECS world.
pub fn build_tiles(
    grid: &WorldGrid,
    content: &ContentRegistry,
    world: &bevy_ecs::world::World,
    core_capture: bool,
) -> Vec<PathTile> {
    let mut tiles = vec![PathTile::default(); grid.len()];
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            let building = building_at(world, grid, content, x, y);
            let packed = pack_tile(grid, content, core_capture, x, y, building, &mut tiles);
            let index = grid.tiles.index(x, y);
            tiles[index] = packed;
        }
    }
    tiles
}

/// Whether `block` is a solid obstacle for ground pathing.
pub fn is_solid(content: &ContentRegistry, block: BlockId) -> bool {
    content.block(block).map(|def| def.solid).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_layout_is_frozen() {
        let tile = PathTile::from_parts(80, 3, true, true, true);
        assert_eq!(tile.health(), 80);
        assert_eq!(tile.team(), 3);
        assert!(tile.solid());
        assert!(tile.deep());
        assert!(tile.damages());
        // Exact bits: health 80 (0x50), team 3 << 8, solid 1<<16, deep 1<<23,
        // damages 1<<24.
        let expected = 80 | (3 << 8) | (1 << 16) | (1 << 23) | (1 << 24);
        assert_eq!(tile.raw(), expected);
    }

    /// T15: a solid tile ORs `BIT_NEAR_SOLID` into its four `d4` neighbours
    /// (`Pathfinder.packTile`, Pathfinder.java:243-246).
    #[test]
    fn pack_tile_marks_neighbours_near_solid() {
        let harness = crate::world::BuildHarness::new(16, 16, 1);
        let wall = harness
            .content
            .block_id("copper-wall")
            .expect("copper-wall");
        let mut grid = harness.grid;
        grid.tiles.get_mut(5, 5).block = wall;
        let mut tiles = vec![PathTile::default(); grid.len()];
        let packed = pack_tile(&grid, &harness.content, false, 5, 5, None, &mut tiles);
        assert!(packed.solid(), "copper-wall packs the solid bit");
        let index = |x: i32, y: i32| grid.tiles.index(x, y);
        assert!(tiles[index(4, 5)].near_solid(), "west neighbour");
        assert!(tiles[index(6, 5)].near_solid(), "east neighbour");
        assert!(tiles[index(5, 4)].near_solid(), "north neighbour");
        assert!(tiles[index(5, 6)].near_solid(), "south neighbour");
        assert!(
            !tiles[index(4, 4)].near_solid(),
            "diagonals are not marked (d4 only)"
        );
    }

    /// T15: the team bits are the building's team (`Tile.getTeamID`), `0` on
    /// empty tiles, and `255` for derelict buildings under `coreCapture`.
    #[test]
    fn pack_tile_team_is_building_team() {
        let mut harness = crate::world::BuildHarness::new(16, 16, 1);
        harness.rules.default_team = 2;
        let wall = harness
            .content
            .block_id("copper-wall")
            .expect("copper-wall");
        assert!(harness.place(5, 5, wall, 0, true));
        let index = harness.grid.tiles.index(5, 5);
        let tiles = build_tiles(&harness.grid, &harness.content, &harness.world, false);
        assert_eq!(
            tiles[index].team(),
            2,
            "the wall tile carries the building's team"
        );
        assert_eq!(
            tiles[harness.grid.tiles.index(0, 0)].team(),
            0,
            "empty tiles are team 0"
        );
        // `rules.coreCapture`: a team-0 (derelict) building packs 255.
        harness.rules.default_team = 0;
        assert!(harness.break_block(5, 5, true));
        assert!(harness.place(5, 6, wall, 0, true));
        let captured = build_tiles(&harness.grid, &harness.content, &harness.world, true);
        assert_eq!(captured[harness.grid.tiles.index(5, 6)].team(), 255);
    }

    #[test]
    fn empty_tile_is_all_zero() {
        let tile = PathTile::default();
        assert!(!tile.solid());
        assert_eq!(tile.health(), 0);
        assert_eq!(tile.team(), 0);
    }
}
