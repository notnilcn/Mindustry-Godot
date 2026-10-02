// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! World & terrain (plan 06).
//!
//! Ported from `core/src/mindustry/core/World.java`, `world/Tile.java`,
//! `world/Tiles.java` and the surrounding world-support types. Plan 06 replaces
//! the P0 placeholder grid: [`WorldGrid`] owns a real [`Tiles`] grid of [`Tile`]s
//! and the tile-change counters; tile mutation lives in [`ops`].
//!
//! Deviation §2.3.1: the building link is an ECS `Entity` handle. This plan's
//! core keeps `WorldGrid` owned by the caller (`Sim`) and the tile ops take an
//! explicit [`WorldCtx`] bundle (`grid` + `content` + ECS world + hooks) rather
//! than `resource_scope::<WorldGrid>`; this is observationally identical and
//! avoids the aliasing constraints of re-entrant hook spawning.

pub mod attributes;
pub mod cached;
pub mod checksum;
pub mod color_mapper;
pub mod context;
pub mod darkness;
pub mod edges;
pub mod events;
pub mod hooks;
pub mod ops;
pub mod params;
pub mod pos;
pub mod raycast;
pub mod tile;
pub mod tiles;

pub use attributes::Attributes;
pub use cached::{CachedBuild, CachedTile, CachedTiles, TileGen};
pub use color_mapper::ColorMapper;
pub use context::{Context, FilterContext};
pub use edges::Edges;
pub use events::{
    TileChangeEvent, TileFloorChangeEvent, TileOverlayChangeEvent, TilePreChangeEvent,
    WorldLoadBeginEvent, WorldLoadEndEvent, WorldLoadEvent,
};
pub use hooks::{NewBuilding, NoopRenderHooks, NoopWorldHooks, RenderHooks, WorldHooks};
pub use ops::{WorldCtx, WorldEventLog};
pub use params::WorldParams;
pub use pos::TilePos;
pub use tile::{PACK_DATA_LAYOUT, Tile};
pub use tiles::Tiles;

use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ContentRegistry};

/// Errors raised by world operations.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum WorldError {
    /// The requested tile lies outside the grid.
    #[error("tile ({x}, {y}) is outside a {width}x{height} world")]
    OutOfBounds {
        /// Requested x.
        x: i16,
        /// Requested y.
        y: i16,
        /// World width.
        width: i32,
        /// World height.
        height: i32,
    },
}

/// The world resource (`core.World` / plan 06 §3.2).
///
/// Deviation §2.3.1: rather than a Java singleton, `WorldGrid` is a plain
/// `Resource`-marked struct owned by the sim/host. Counters start at `1` and are
/// reset to `-1` when a `WorldLoadEvent` completes (upstream `World`).
#[derive(Debug, Clone, Resource)]
pub struct WorldGrid {
    /// Tile grid.
    pub tiles: Tiles,
    /// Whether generation is in progress (suppresses events/proximity).
    pub generating: bool,
    /// Whether the loaded map was invalid (`World.invalidMap`).
    pub invalid_map: bool,
    /// Tile-change counter (Java `int`, starts at 1).
    pub tile_changes: i32,
    /// Floor-change counter (Java `int`, starts at 1).
    pub floor_changes: i32,
    /// Seed used by generation filters (`OD6-A`).
    pub generator_seed: u64,
    /// Optional map-area limit `(x, y, w, h)` (`Rules.limitMapArea`, plan 12).
    pub limit_area: Option<(i32, i32, i32, i32)>,
}

impl Default for WorldGrid {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl WorldGrid {
    /// An all-`air` grid of the given size. Matches `new World()` counters.
    pub fn new(width: i32, height: i32) -> Self {
        Self {
            tiles: Tiles::new(width, height),
            generating: false,
            invalid_map: false,
            tile_changes: 1,
            floor_changes: 1,
            generator_seed: 0,
            limit_area: None,
        }
    }

    /// Wraps an existing [`Tiles`] grid.
    pub fn from_tiles(tiles: Tiles) -> Self {
        Self {
            tiles,
            ..Self::default()
        }
    }

    /// Grid width in tiles.
    pub fn width(&self) -> i32 {
        self.tiles.width
    }

    /// Grid height in tiles.
    pub fn height(&self) -> i32 {
        self.tiles.height
    }

    /// Number of tiles.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }

    /// Whether the grid is empty.
    pub fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    /// Whether the world is generating (`World.isGenerating`).
    pub fn is_generating(&self) -> bool {
        self.generating
    }

    /// Replaces the grid with a fresh all-`air` grid (`World.resize`); the old
    /// tiles/buildings are discarded (upstream clears buildings first).
    pub fn resize(&mut self, width: i32, height: i32) {
        self.tiles = Tiles::new(width, height);
    }

    /// Clears every building entity link (`World.clearBuildings`); the caller
    /// despawns the ECS entities (plan 07).
    pub fn clear_buildings(&mut self) {
        for tile in self.tiles.array_mut() {
            tile.build = None;
        }
    }

    /// Packs a tile position (`World.packArray`-adjacent helper).
    pub fn pack_array(x: i32, y: i32) -> i32 {
        Tiles::pos_of(x, y)
    }

    /// Flat `x + y*width` index (`World.packArray`).
    pub fn flat_index(&self, x: i32, y: i32) -> i32 {
        x + y * self.tiles.width
    }

    /// Unpacks an Arc `Point2` position (`World.unpack`).
    pub fn unpack(packed: i32) -> TilePos {
        TilePos::from_pack(packed)
    }

    /// World pixels → logic tile coordinates (`World.conv`; unrounded).
    pub fn conv(coord: f32) -> f32 {
        coord / crate::config::TILESIZE as f32
    }

    /// Logic tile coordinates → world pixels (`World.unconv`).
    pub fn unconv(coord: f32) -> f32 {
        coord * crate::config::TILESIZE as f32
    }

    /// World pixel → nearest tile coordinate (`World.toTile`; rounded).
    pub fn to_tile(coord: f32) -> i32 {
        (coord / crate::config::TILESIZE as f32).round() as i32
    }

    /// Whether `(x, y)` lies inside the defined map limit rect
    /// (`World.isInMapArea`); `limit_area` is plan 12's `Rules.limitMapArea`.
    pub fn is_in_map_area(&self, x: i32, y: i32) -> bool {
        if !self.tiles.in_bounds(x, y) {
            return false;
        }
        match self.limit_area {
            Some((lx, ly, w, h)) => x >= lx && y >= ly && x < lx + w && y < ly + h,
            None => true,
        }
    }

    /// Begins a map load; tile events are suppressed until
    /// [`end_map_load`](Self::end_map_load) (`World.beginMapLoad`).
    pub fn begin_map_load(&mut self) {
        self.generating = true;
    }

    /// Ends a map load: darkness BFS, proximity (07 hook), counters reset to
    /// `-1` (`World.endMapLoad` + `WorldLoadEvent` listener). Legacy-block
    /// removal and building proximity are plan 07 hooks.
    pub fn end_map_load(&mut self, content: &ContentRegistry) {
        // Building proximity + legacy-block removal are plan 07 hooks; the
        // darkness BFS and counter reset are this plan's.
        darkness::add_darkness(self, content);
        self.generating = false;
        // `WorldLoadEvent` listener resets the counters.
        self.tile_changes = -1;
        self.floor_changes = -1;
    }

    /// Explicitly toggles the generating flag (`World.setGenerating`).
    pub fn set_generating(&mut self, generating: bool) {
        self.generating = generating;
    }

    /// World/scene quad bounds `[x, y, w, h]` in pixels (`World.getQuadBounds`).
    pub fn get_quad_bounds(&self) -> [f32; 4] {
        let b = crate::constants::FINAL_WORLD_BOUNDS;
        [
            -b,
            -b,
            self.tiles.width as f32 * crate::config::TILESIZE as f32 + b * 2.0,
            self.tiles.height as f32 * crate::config::TILESIZE as f32 + b * 2.0,
        ]
    }

    /// Tile at `(x, y)` (`World.tile`).
    pub fn tile(&self, x: i32, y: i32) -> &Tile {
        self.tiles.get(x, y)
    }

    /// Tile at a position.
    pub fn tile_at_pos(&self, pos: TilePos) -> &Tile {
        self.tiles.get(pos.x() as i32, pos.y() as i32)
    }

    /// Tile by flat index.
    pub fn tile_ref(&self, index: usize) -> &Tile {
        self.tiles.geti(index)
    }

    /// Block id by flat index.
    pub fn block_id_at(&self, index: usize) -> BlockId {
        self.tiles.geti(index).block
    }

    /// Building entity by flat index.
    pub fn build_entity_at(&self, index: usize) -> Option<Entity> {
        self.tiles.geti(index).build
    }

    /// Row-major index of a position, or an out-of-bounds error.
    pub fn index(&self, pos: TilePos) -> Result<usize, WorldError> {
        if !self.in_bounds(pos) {
            return Err(self.out_of_bounds(pos));
        }
        Ok(self.tiles.index(pos.x() as i32, pos.y() as i32))
    }

    /// Whether a position is inside the grid.
    pub fn in_bounds(&self, pos: TilePos) -> bool {
        self.tiles.in_bounds(pos.x() as i32, pos.y() as i32)
    }

    /// Fills every tile with a floor and wall (`Tiles.fill`, plan-00 compat).
    pub fn fill(&mut self, floor: BlockId, wall: BlockId) {
        self.tiles.fill(floor, wall);
    }

    /// Sets a tile block without building handling (plan-00 `Sim` path).
    pub fn set_block(
        &mut self,
        pos: TilePos,
        block: BlockId,
        _team: u8,
        _rot: u8,
    ) -> Result<(), WorldError> {
        let index = self.index(pos)?;
        self.tiles.geti_mut(index).block = block;
        Ok(())
    }

    /// Sets the building entity of a tile (plan-00 `Sim` path).
    pub fn set_entity(&mut self, pos: TilePos, entity: Option<Entity>) -> Result<(), WorldError> {
        let index = self.index(pos)?;
        self.tiles.geti_mut(index).build = entity;
        Ok(())
    }

    /// Clears a tile to `air` with no entity.
    pub fn clear(&mut self, pos: TilePos) -> Result<(), WorldError> {
        let index = self.index(pos)?;
        let tile = self.tiles.geti_mut(index);
        tile.block = BlockId::AIR;
        tile.build = None;
        Ok(())
    }

    /// Wall/block at a position (`None` when out of bounds).
    pub fn block_at(&self, pos: TilePos) -> Option<BlockId> {
        self.in_bounds(pos)
            .then(|| self.tile(pos.x() as i32, pos.y() as i32).block)
    }

    /// Floor at a position (`None` when out of bounds).
    pub fn floor_at(&self, pos: TilePos) -> Option<BlockId> {
        self.in_bounds(pos)
            .then(|| self.tile(pos.x() as i32, pos.y() as i32).floor)
    }

    /// Building entity at a position (`None` when out of bounds or empty).
    pub fn entity_at(&self, pos: TilePos) -> Option<Entity> {
        self.in_bounds(pos)
            .then(|| self.tile(pos.x() as i32, pos.y() as i32).build)
            .flatten()
    }

    /// Iterates positions row-major by `(y, x)` — the canonical checksum/dump order.
    pub fn iter_row_major(&self) -> impl Iterator<Item = (TilePos, usize)> + '_ {
        let width = self.tiles.width;
        let height = self.tiles.height;
        (0..height).flat_map(move |y| {
            (0..width).map(move |x| {
                let index = x as usize + y as usize * width as usize;
                (TilePos(x as i16, y as i16), index)
            })
        })
    }

    fn out_of_bounds(&self, pos: TilePos) -> WorldError {
        WorldError::OutOfBounds {
            x: pos.0,
            y: pos.1,
            width: self.tiles.width,
            height: self.tiles.height,
        }
    }
}

/// Whether a block kind creates a building entity upstream
/// (`!environment`); plan 07 supplies the exact `hasBuilding` flag.
pub fn has_building_kind(kind: BlockKind) -> bool {
    !matches!(
        kind,
        BlockKind::AirBlock
            | BlockKind::SpawnBlock
            | BlockKind::RemoveWall
            | BlockKind::RemoveOre
            | BlockKind::Cliff
            | BlockKind::Floor
            | BlockKind::EmptyFloor
            | BlockKind::OverlayFloor
            | BlockKind::OreBlock
            | BlockKind::StaticWall
            | BlockKind::StaticProp
            | BlockKind::StaticTree
            | BlockKind::Prop
            | BlockKind::TreeBlock
            | BlockKind::TallBlock
            | BlockKind::SeaBush
            | BlockKind::Seaweed
            | BlockKind::ShallowLiquid
            | BlockKind::CharacterOverlay
            | BlockKind::RuneOverlay
            | BlockKind::ColoredFloor
            | BlockKind::SteamVent
    )
}

/// Whether a block id creates a building entity (`Block.hasBuilding()`).
pub fn block_has_building(content: &ContentRegistry, block: BlockId) -> bool {
    content
        .block(block)
        .is_some_and(|def| has_building_kind(def.kind))
}

/// World hooks resource (plan 07/16 inject real implementations).
#[derive(Resource)]
pub struct WorldHooksRes(pub Box<dyn WorldHooks>);

impl Default for WorldHooksRes {
    fn default() -> Self {
        Self(Box::new(NoopWorldHooks))
    }
}

/// Render hooks resource (plan 16 injects the real implementation).
#[derive(Resource)]
pub struct RenderHooksRes(pub Box<dyn RenderHooks>);

impl Default for RenderHooksRes {
    fn default() -> Self {
        Self(Box::new(NoopRenderHooks))
    }
}

/// Registers the world resource and default hook seams.
///
/// `mind-core` depends on `bevy_ecs` only (no `bevy_app`), so this is a helper
/// for hosts that build a `World` by hand rather than a `Plugin` impl.
#[derive(Debug, Default)]
pub struct WorldPlugin;

impl WorldPlugin {
    /// Inserts the default world/hook resources into a `World`.
    pub fn register(&self, world: &mut World) {
        world.init_resource::<WorldGrid>();
        world.init_resource::<WorldHooksRes>();
        world.init_resource::<RenderHooksRes>();
    }
}

/// Spawns a bare building entity for tests/plan-07 fallback (no modules).
#[cfg(test)]
pub fn spawn_bare_building(world: &mut World, request: NewBuilding, seq: u64) -> Option<Entity> {
    use crate::ecs::{BuildingComp, EntitySeq, TeamId};
    Some(
        world
            .spawn((
                EntitySeq(seq),
                BuildingComp {
                    pos: TilePos::new(request.x, request.y),
                    block: request.block,
                    team: TeamId(request.team),
                    rot: request.rot,
                },
            ))
            .id(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ported from `tests/src/test/java/ApplicationTests.java` `createMap()`:
    /// `world.resize(8, 8)` followed by `tiles.fill()`.
    #[test]
    fn create_map_resize_fill() {
        let mut grid = WorldGrid::new(0, 0);
        grid.resize(8, 8);
        grid.fill(BlockId::STONE_WALL, BlockId::AIR);

        assert_eq!(grid.width(), 8);
        assert_eq!(grid.height(), 8);
        assert_eq!(grid.len(), 64);
        assert_eq!(grid.tile_changes, 1);

        for (pos, index) in grid.iter_row_major() {
            assert_eq!(grid.tiles.geti(index).floor, BlockId::STONE_WALL);
            assert_eq!(grid.tiles.geti(index).block, BlockId::AIR);
            assert_eq!(grid.entity_at(pos), None);
        }

        let pos = TilePos::new(3, 4);
        grid.set_block(pos, BlockId::STONE_WALL, 0, 0).unwrap();
        assert_eq!(grid.block_at(pos), Some(BlockId::STONE_WALL));
        assert_eq!(grid.block_at(TilePos::new(-1, 0)), None);
        assert!(matches!(
            grid.set_block(TilePos::new(8, 0), BlockId::STONE_WALL, 0, 0),
            Err(WorldError::OutOfBounds { .. })
        ));
        grid.clear(pos).unwrap();
        assert_eq!(grid.block_at(pos), Some(BlockId::AIR));
    }

    #[test]
    fn tile_get_getn_getc_geti_in() {
        let grid = WorldGrid::new(4, 3);
        assert!(grid.in_bounds(TilePos::new(3, 2)));
        assert!(!grid.in_bounds(TilePos::new(4, 0)));
        assert_eq!(grid.tiles.geti(0).x, 0);
        assert_eq!(grid.tiles.getn(9, 9), None);
        assert_eq!(grid.tiles.getc(-1, -1).pos(), TilePos::new(0, 0));
        assert_eq!(grid.tiles.get(2, 1).pos(), TilePos::new(2, 1));
    }

    #[test]
    fn fill_resets_grid() {
        let mut grid = WorldGrid::new(2, 2);
        grid.tiles.get_mut(1, 1).block = BlockId::STONE_WALL;
        grid.fill(BlockId::STONE_WALL, BlockId::AIR);
        assert!(grid.tiles.iter().all(|tile| tile.block == BlockId::AIR));
        assert!(
            grid.tiles
                .iter()
                .all(|tile| tile.floor == BlockId::STONE_WALL)
        );
    }

    #[test]
    fn begin_end_map_load_events() {
        let content = crate::content::test_support::test_registry();
        let mut grid = WorldGrid::new(4, 4);
        grid.begin_map_load();
        assert!(grid.is_generating());
        // A floor change while generating fires no events and bumps no counter.
        grid.generating = true;
        let before = grid.floor_changes;
        grid.tiles.get_mut(1, 1).floor = BlockId::STONE_WALL;
        assert_eq!(grid.floor_changes, before);
        grid.end_map_load(&content);
        assert!(!grid.is_generating());
        // `WorldLoadEvent` listener resets the counters to -1.
        assert_eq!(grid.tile_changes, -1);
        assert_eq!(grid.floor_changes, -1);
    }

    #[test]
    fn raycast_dda_hits_first_block() {
        let mut grid = WorldGrid::new(8, 8);
        grid.tiles.get_mut(4, 0).block = BlockId::STONE_WALL;
        let hit = raycast::raycast_first(&grid, 0, 0, 7, 0);
        assert_eq!(hit, Some(TilePos::new(4, 0)));
        assert_eq!(raycast::raycast_first(&grid, 0, 5, 7, 5), None);
    }

    #[test]
    fn is_in_map_area_with_limit() {
        let mut grid = WorldGrid::new(8, 8);
        assert!(grid.is_in_map_area(7, 7));
        assert!(!grid.is_in_map_area(8, 0));
        grid.limit_area = Some((2, 2, 3, 3));
        assert!(grid.is_in_map_area(2, 2));
        assert!(grid.is_in_map_area(4, 4));
        assert!(!grid.is_in_map_area(5, 5));
        assert!(!grid.is_in_map_area(1, 1));
    }

    /// Test hooks that spawn a bare building entity (plan-07 fallback).
    struct TestHooks(std::sync::atomic::AtomicU64);

    impl WorldHooks for TestHooks {
        fn new_building(&self, world: &mut World, request: NewBuilding) -> Option<Entity> {
            let seq = self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            spawn_bare_building(world, request, seq)
        }
    }

    fn multiblock_ctx<'a>(
        grid: &'a mut WorldGrid,
        content: &'a ContentRegistry,
        ecs: &'a mut World,
        hooks: &'a TestHooks,
        render: &'a NoopRenderHooks,
        log: &'a mut crate::world::ops::WorldEventLog,
    ) -> crate::world::ops::WorldCtx<'a> {
        crate::world::ops::WorldCtx {
            grid,
            content,
            ecs,
            hooks,
            render,
            log,
        }
    }

    /// Ported from `ApplicationTests.multiblock`.
    #[test]
    fn multiblock_linkage() {
        let content = crate::content::test_support::test_registry();
        let Some(core) = content.block_id("core-shard") else {
            return;
        };
        let mut grid = WorldGrid::new(8, 8);
        let mut ecs = World::new();
        let hooks = TestHooks(std::sync::atomic::AtomicU64::new(0));
        let render = NoopRenderHooks;
        let mut log = crate::world::ops::WorldEventLog::default();
        let mut ctx = multiblock_ctx(&mut grid, &content, &mut ecs, &hooks, &render, &mut log);

        ctx.set_block(4, 4, core, 0, 0);
        let entity = ctx.grid.tiles.get(4, 4).build;
        assert!(entity.is_some());
        for x in 3..=5 {
            for y in 3..=5 {
                let tile = ctx.grid.tiles.get(x, y);
                assert_eq!(tile.block, core);
                assert_eq!(tile.build, entity);
            }
        }
    }

    /// Ported from `ApplicationTests.blockOverlapRemoved`.
    #[test]
    fn multiblock_overlap_removed() {
        let content = crate::content::test_support::test_registry();
        let Some(core) = content.block_id("core-shard") else {
            return;
        };
        let mut grid = WorldGrid::new(8, 8);
        let mut ecs = World::new();
        let hooks = TestHooks(std::sync::atomic::AtomicU64::new(0));
        let render = NoopRenderHooks;
        let mut log = crate::world::ops::WorldEventLog::default();
        let mut ctx = multiblock_ctx(&mut grid, &content, &mut ecs, &hooks, &render, &mut log);

        // Edge block covers (0,0)..(2,2).
        ctx.set_block(1, 1, core, 0, 0);
        assert_eq!(ctx.grid.tiles.get(0, 0).block, core);
        // Overlapping placement at (2,2) must clear the first block's footprint.
        ctx.set_block(2, 2, core, 0, 0);
        assert_eq!(ctx.grid.tiles.get(0, 0).block, BlockId::AIR);
    }
}
