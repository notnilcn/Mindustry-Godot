// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! ECS-aware [`MapSource`] over a live [`WorldGrid`] (plan 06 M4).
//!
//! Upstream `Maps.saveMap` scans `tile.block() instanceof CoreBlock` and reads
//! `tile.getTeamID()` from the tile. The Rust `Tile` carries no team (teams are
//! a plan-07 ECS component), so this view looks the building entity up in the
//! ECS world and projects its `TeamComp`/`Building` back onto the tile. Tile-only
//! sources keep the pre-plan-07 behavior ([`MapSource::core_team`] defaults to
//! `None`).

use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry};
use crate::entities::comp::{Building, TeamComp};
use crate::io::save::state::MapSource;
use crate::world::WorldGrid;

/// [`MapSource`] view that resolves core teams and multiblock centers from the
/// plan-07 ECS building entities.
pub struct EcsMapSource<'a> {
    /// The grid being written.
    pub grid: &'a WorldGrid,
    /// Content registry (`shouldSaveData`/core classification).
    pub content: &'a ContentRegistry,
    /// ECS world holding the `Building`/`TeamComp` entities.
    pub ecs: &'a World,
}

impl<'a> EcsMapSource<'a> {
    /// Wraps a live grid, content registry and ECS world.
    pub fn new(grid: &'a WorldGrid, content: &'a ContentRegistry, ecs: &'a World) -> Self {
        Self { grid, content, ecs }
    }
}

impl MapSource for EcsMapSource<'_> {
    fn width(&self) -> u16 {
        self.grid.tiles.width as u16
    }

    fn height(&self) -> u16 {
        self.grid.tiles.height as u16
    }

    fn floor_id(&self, index: usize) -> u16 {
        self.grid.tiles.geti(index).floor.raw()
    }

    fn overlay_id(&self, index: usize) -> u16 {
        self.grid.tiles.geti(index).overlay.raw()
    }

    fn block_id(&self, index: usize) -> u16 {
        self.grid.tiles.geti(index).block.raw()
    }

    fn has_building(&self, index: usize) -> bool {
        self.grid.tiles.geti(index).build.is_some()
    }

    fn is_center(&self, index: usize) -> bool {
        let Some(entity) = self.grid.tiles.geti(index).build else {
            return true;
        };
        self.ecs.get::<Building>(entity).is_some_and(|building| {
            building.tile.x() as usize == index % self.width() as usize
                && building.tile.y() as usize == index / self.width() as usize
        })
    }

    fn should_save_data(&self, index: usize) -> bool {
        self.grid.tiles.geti(index).should_save_data(self.content)
    }

    fn tile_data(&self, index: usize) -> (u8, u8, u8, i32) {
        let tile = self.grid.tiles.geti(index);
        (
            tile.data as u8,
            tile.floor_data as u8,
            tile.overlay_data as u8,
            tile.extra_data,
        )
    }

    fn write_building(
        &self,
        _index: usize,
        _chunk: &mut crate::io::wire::WireWriter,
    ) -> crate::io::IoResult<()> {
        // Building tile-entity chunks are a plan-07/04 seam; the ECS-aware
        // source currently projects presence/team/center only.
        Ok(())
    }

    fn core_team(&self, index: usize) -> Option<u8> {
        let entity = self.grid.tiles.geti(index).build?;
        let building = self.ecs.get::<Building>(entity)?;
        if !is_core(self.content, building.block) {
            return None;
        }
        self.ecs.get::<TeamComp>(entity).map(|team| team.team)
    }
}

/// Whether a block is a core (`CoreBlock`), mirroring `Maps.saveMap`'s
/// `instanceof CoreBlock` check.
fn is_core(content: &ContentRegistry, block: BlockId) -> bool {
    content
        .block(block)
        .is_some_and(|def| def.kind == crate::content::BlockKind::CoreBlock)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn ecs_source_collects_core_teams() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let core = harness
            .content()
            .block_id("core-shard")
            .expect("core-shard");
        harness.rules.default_team = 1;
        assert!(harness.place(4, 4, core, 0, true));
        // A second core on the wave team.
        harness.rules.default_team = 2;
        assert!(harness.place(10, 10, core, 0, true));

        let source = EcsMapSource::new(&harness.grid, &harness.content, &harness.world);
        let width = source.width() as usize;
        let first = 4 + 4 * width;
        let second = 10 + 10 * width;
        assert_eq!(source.core_team(first), Some(1));
        assert_eq!(source.core_team(second), Some(2));
        assert!(source.is_center(first));
        assert_eq!(source.core_team(0), None);
    }
}
