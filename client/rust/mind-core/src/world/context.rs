// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `WorldContext` implementations (plan 06 §3.5; HLP §12 C10).
//!
//! Plan 04 defines [`crate::io::save::WorldContext`] because 04 blocks 06; this
//! module implements it over the real [`WorldGrid`] for the default `Context`,
//! the filter context and (later) a sector context. It never forks the trait.

use crate::content::{BlockId, ContentRegistry, SectorId};
use crate::io::IoError;
use crate::io::save::WorldContext;
use crate::io::wire::WireReader;

use super::WorldGrid;

/// Default map/world read context (`WorldContext` over the live grid).
pub struct Context<'a> {
    /// The grid being built.
    pub grid: &'a mut WorldGrid,
    /// Content registry (block metadata).
    pub content: &'a ContentRegistry,
    /// Owning sector, when the load is a campaign sector (plan 12 write-back).
    pub sector: Option<SectorId>,
    /// Decoded tile-entity payloads awaiting spawn by the ECS-owning host.
    ///
    /// Plan 07 §3.10: `read_building` decodes `writeBase` byte-for-byte. Spawning
    /// the `Building` entity (and applying [`super::building_io::DecodedBase`])
    /// needs the ECS world, which plan 06's `Context` deliberately does not own;
    /// the host drains this queue via [`Context::take_pending_buildings`] after
    /// `end()`. This replaces the plan-06 placeholder error.
    pub pending_buildings: Vec<(usize, super::building_io::DecodedBase)>,
}

impl<'a> Context<'a> {
    /// Creates a context over a grid.
    pub fn new(grid: &'a mut WorldGrid, content: &'a ContentRegistry) -> Self {
        Self {
            grid,
            content,
            sector: None,
            pending_buildings: Vec::new(),
        }
    }

    /// Creates a sector context (plan 12).
    pub fn sector(grid: &'a mut WorldGrid, content: &'a ContentRegistry, sector: SectorId) -> Self {
        Self {
            grid,
            content,
            sector: Some(sector),
            pending_buildings: Vec::new(),
        }
    }

    /// Drains decoded building payloads (`(tile index, base)`).
    pub fn take_pending_buildings(&mut self) -> Vec<(usize, super::building_io::DecodedBase)> {
        std::mem::take(&mut self.pending_buildings)
    }

    /// Read-only view of the decoded building payloads.
    pub fn pending_buildings(&self) -> &[(usize, super::building_io::DecodedBase)] {
        &self.pending_buildings
    }
}

impl WorldContext for Context<'_> {
    fn tile_count(&self) -> usize {
        self.grid.len()
    }

    fn resize(&mut self, width: u16, height: u16) {
        self.grid.resize(width as i32, height as i32);
    }

    fn create(&mut self, x: u16, y: u16, floor: u16, overlay: u16, wall: u16) {
        if !self.grid.tiles.in_bounds(x as i32, y as i32) {
            return;
        }
        let index = self.grid.tiles.index(x as i32, y as i32);
        let tile = self.grid.tiles.geti_mut(index);
        tile.floor = BlockId::new(floor);
        tile.overlay = BlockId::new(overlay);
        tile.block = BlockId::new(wall);
        tile.build = None;
    }

    fn is_generating(&self) -> bool {
        self.grid.generating
    }

    fn begin(&mut self) {
        self.grid.generating = true;
    }

    fn end(&mut self) {
        self.grid.generating = false;
    }

    fn set_block(&mut self, index: usize, block: u16) {
        let tile = self.grid.tiles.geti_mut(index);
        tile.block = BlockId::new(block);
        tile.build = None;
    }

    fn has_building(&self, index: usize) -> bool {
        self.grid.tiles.geti(index).build.is_some()
    }

    fn block_has_building_io(&self, index: usize) -> bool {
        let block = self.grid.tiles.geti(index).block;
        super::block_has_building(self.content, block)
    }

    fn set_tile_data(
        &mut self,
        index: usize,
        data: u8,
        floor_data: u8,
        overlay_data: u8,
        extra_data: i32,
    ) {
        let tile = self.grid.tiles.geti_mut(index);
        tile.data = data as i8;
        tile.floor_data = floor_data as i8;
        tile.overlay_data = overlay_data as i8;
        tile.extra_data = extra_data;
    }

    fn read_building(
        &mut self,
        index: usize,
        reader: &mut WireReader,
        version: u8,
    ) -> Result<(), IoError> {
        // Plan 07 decodes `BuildingComp.writeBase`; the ECS-owning host drains
        // `pending_buildings` and spawns/applies the entity after `end()`.
        let mut decoded = super::building_io::DecodedBase::default();
        super::building_io::read_base(&mut decoded, reader, version)?;
        self.pending_buildings.push((index, decoded));
        Ok(())
    }
}

/// Filter context (`WorldContext` whose `end()` applies the map filter stack).
///
/// Ported from `WorldContext.FilterContext`: `end()` runs `Maps.applyFilters`
/// (plan 06 §3.5/§3.8) before the normal end-map-load step. The filter stack is
/// resolved by the caller (`Map::filters`); `seed` feeds the `MapGen` stream for
/// `randomize()` (OD6-A).
pub struct FilterContext<'a> {
    /// Inner default context.
    pub inner: Context<'a>,
    /// The map identity used for filter randomization (`OD6-A`).
    pub map_name: String,
    /// The resolved filter stack.
    pub filters: Vec<Box<dyn crate::maps::filters::GenerateFilter>>,
    /// Deterministic generation seed.
    pub seed: u64,
}

impl<'a> FilterContext<'a> {
    /// Wraps a context with a map name and an empty filter stack.
    pub fn new(inner: Context<'a>, map_name: impl Into<String>) -> Self {
        Self {
            inner,
            map_name: map_name.into(),
            filters: Vec::new(),
            seed: 0,
        }
    }

    /// Wraps a context with a resolved filter stack and seed.
    pub fn with_filters(
        inner: Context<'a>,
        map_name: impl Into<String>,
        filters: Vec<Box<dyn crate::maps::filters::GenerateFilter>>,
        seed: u64,
    ) -> Self {
        Self {
            inner,
            map_name: map_name.into(),
            filters,
            seed,
        }
    }
}

impl WorldContext for FilterContext<'_> {
    fn tile_count(&self) -> usize {
        self.inner.tile_count()
    }

    fn resize(&mut self, width: u16, height: u16) {
        self.inner.resize(width, height);
    }

    fn create(&mut self, x: u16, y: u16, floor: u16, overlay: u16, wall: u16) {
        self.inner.create(x, y, floor, overlay, wall);
    }

    fn is_generating(&self) -> bool {
        self.inner.is_generating()
    }

    fn begin(&mut self) {
        self.inner.begin();
    }

    fn end(&mut self) {
        // Java `FilterContext.end`: `Maps.applyFilters(tiles, map.filters())`
        // then `super.end()`.
        let FilterContext {
            inner,
            filters,
            seed,
            ..
        } = self;
        if !filters.is_empty() {
            let mut rng = crate::determinism::SimRng::new(*seed);
            crate::maps::filters::apply_stack(
                &mut inner.grid.tiles,
                filters,
                inner.content,
                &mut rng,
            );
        }
        inner.end();
    }

    fn set_block(&mut self, index: usize, block: u16) {
        self.inner.set_block(index, block);
    }

    fn has_building(&self, index: usize) -> bool {
        self.inner.has_building(index)
    }

    fn block_has_building_io(&self, index: usize) -> bool {
        self.inner.block_has_building_io(index)
    }

    fn set_tile_data(
        &mut self,
        index: usize,
        data: u8,
        floor_data: u8,
        overlay_data: u8,
        extra_data: i32,
    ) {
        self.inner
            .set_tile_data(index, data, floor_data, overlay_data, extra_data);
    }

    fn read_building(
        &mut self,
        index: usize,
        reader: &mut WireReader,
        version: u8,
    ) -> Result<(), IoError> {
        self.inner.read_building(index, reader, version)
    }

    fn is_map(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_create_and_resize() {
        let content = crate::content::test_support::test_registry();
        let mut grid = WorldGrid::new(0, 0);
        let mut ctx = Context::new(&mut grid, &content);
        assert_eq!(ctx.tile_count(), 0);
        ctx.resize(4, 4);
        ctx.begin();
        assert!(ctx.is_generating());
        ctx.create(1, 2, 80, 0, 0);
        ctx.end();
        assert!(!ctx.is_generating());
        ctx.set_block(0, 80);
        ctx.set_tile_data(ctx.grid.tiles.index(0, 0), 3, 4, 5, 6);
        assert_eq!(ctx.grid.tiles.get(1, 2).floor, BlockId::new(80));
        assert_eq!(ctx.grid.tiles.geti(0).block, BlockId::new(80));
        let tile = ctx.grid.tiles.geti(0);
        assert_eq!(
            (
                tile.data,
                tile.floor_data,
                tile.overlay_data,
                tile.extra_data
            ),
            (3, 4, 5, 6)
        );
    }

    /// Plan 07 M3: `read_building` decodes the base body into the pending queue
    /// instead of erroring.
    #[test]
    fn read_building_queues_decoded_base() {
        let content = crate::content::test_support::test_registry();
        let mut grid = WorldGrid::new(2, 2);
        let mut ctx = Context::new(&mut grid, &content);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&10.0f32.to_be_bytes());
        bytes.push(0x80);
        bytes.push(0);
        bytes.push(1); // enabled
        bytes.push(0); // module bits
        bytes.push(255); // efficiency
        bytes.push(255); // optional efficiency
        let mut reader = crate::io::wire::WireReader::new(&bytes);
        ctx.read_building(1, &mut reader, 3).expect("decode");
        let pending = ctx.pending_buildings();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, 1);
        assert_eq!(pending[0].1.health, 10.0);
        assert_eq!(ctx.take_pending_buildings().len(), 1);
        assert_eq!(ctx.pending_buildings().len(), 0);
    }
}
