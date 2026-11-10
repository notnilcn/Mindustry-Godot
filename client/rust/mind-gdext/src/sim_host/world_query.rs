// SPDX-License-Identifier: GPL-3.0-only

//! World queries shared by the host probes.
//! Upstream: none (port-only; world query helpers)

use super::*;

/// Rendered center of the first core in row-major order, matching the block
/// renderer's `anchor_tile * TILESIZE + Block.offset` quad center (Java
/// `Tile.worldx()` convention; the anchor tile is the centre tile for odd
/// multiblocks).
///
/// Upstream `Player.bestCore()` picks the player team's core; the port's tile
/// model carries no team, so this returns the first core tile scanned.
pub(super) fn first_core_position(
    grid: &WorldGrid,
    content: &ContentRegistry,
) -> Option<(f32, f32)> {
    let unit = mind_core::config::TILESIZE as f32;
    let is_core = |x: i32, y: i32| -> bool {
        if x < 0 || y < 0 || x >= grid.width() || y >= grid.height() {
            return false;
        }
        content
            .block(grid.tile(x, y).block)
            .is_some_and(|def| def.kind == BlockKind::CoreBlock)
    };
    grid.iter_row_major().find_map(|(pos, index)| {
        let def = content.block(grid.block_id_at(index))?;
        (def.kind == BlockKind::CoreBlock).then(|| {
            // Java `World.setSectorRules` -> `sector.info.spawnPosition =
            // core.pos()` uses the building's ANCHOR tile (the centre for odd
            // multiblocks) and the player spawns at `tile.worldx()/worldy()`
            // (the tile's corner, not its centre). Map-loaded multiblocks fill
            // the whole footprint (the row-major first tile is the min corner,
            // so the anchor is `pos - size_offset`); tool-placed blocks may
            // store only the anchor tile. The candidate `pos - size_offset`
            // being the same core disambiguates the two conventions.
            let candidate_x = pos.x() as i32 - def.size_offset;
            let candidate_y = pos.y() as i32 - def.size_offset;
            let anchor = if is_core(candidate_x, candidate_y) {
                (candidate_x, candidate_y)
            } else {
                (pos.x() as i32, pos.y() as i32)
            };
            (anchor.0 as f32 * unit, anchor.1 as f32 * unit)
        })
    })
}

impl MindSimHost {
    /// Tile rows for the view: `(x, y, block_id)` for every non-air tile.
    ///
    /// Plain accessor (not `#[func]`) consumed by `MindTileGrid::draw`.
    pub fn tile_blocks(&self) -> Vec<(i16, i16, u16)> {
        self.sim
            .grid
            .iter_row_major()
            .filter_map(|(pos, index)| {
                let block = self.sim.grid.block_id_at(index);
                (block != BlockId::AIR).then_some((pos.x(), pos.y(), block.get()))
            })
            .collect()
    }

    /// World size in tiles `(width, height)`; used to draw the grid border.
    pub fn world_size(&self) -> (i32, i32) {
        (self.sim.grid.width(), self.sim.grid.height())
    }

    /// Number of completed world loads; the camera rig snaps on a change.
    pub fn world_loads(&self) -> u64 {
        self.world_loads
    }

    /// Player best-core camera target for a world load (`Control.WorldLoadEvent`
    /// → `camera.position.set(player.bestCore())`), in world pixels. `None`
    /// before content boot or on a core-less map.
    pub fn best_core_position_px(&self) -> Option<(f32, f32)> {
        first_core_position(&self.sim.grid, self.content_snapshot.as_ref()?)
    }

    /// Block name of the player's best core (`Player.bestCore().block.name`,
    /// the `block-<name>-full` region the landing cutscene draws); empty before
    /// content boot or on a core-less map.
    pub fn best_core_block(&self) -> GString {
        let Some(content) = self.content_snapshot.as_ref() else {
            return GString::new();
        };
        self.sim
            .grid
            .iter_row_major()
            .find_map(|(_, index)| {
                let def = content.block(self.sim.grid.block_id_at(index))?;
                (def.kind == BlockKind::CoreBlock).then(|| def.name.clone())
            })
            .map(|name| GString::from(name.as_str()))
            .unwrap_or_default()
    }

    /// Read-only world grid for the plan-16 floor/block bakers.
    pub fn grid(&self) -> &mind_core::world::WorldGrid {
        &self.sim.grid
    }

    /// Tile `(x, y)`; `None` off the map (`world.build`
    /// returns null out of bounds). Every coordinate-taking render/HUD seam
    /// goes through this: `WorldGrid::tile` panics (debug_assert) off-map.
    pub(super) fn tile_at(&self, x: i32, y: i32) -> Option<&mind_core::world::Tile> {
        if !self.sim.grid.tiles.in_bounds(x, y) {
            return None;
        }
        Some(self.sim.grid.tile(x, y))
    }

    /// Building rotation (`Building.rotation`, quarter turns) at `(x, y)`;
    /// `0` without a building. Rust-only seam for rotated block sprites.
    pub fn building_rotation(&self, x: i32, y: i32) -> u8 {
        self.tile_at(x, y)
            .and_then(|tile| tile.build)
            .and_then(|entity| {
                self.sim
                    .ecs
                    .0
                    .get::<mind_core::ecs::BuildingComp>(entity)
                    .map(|comp| comp.rot)
            })
            .unwrap_or(0)
    }

    /// `ConstructBuild` draw state at `(x, y)`: `(current block, progress)`.
    /// Rust-only seam for the construct-site sprite (`ConstructBuild.draw`).
    pub fn construct_draw(&self, x: i32, y: i32) -> Option<(mind_core::content::BlockId, f32)> {
        let entity = self.tile_at(x, y)?.build?;
        self.sim
            .ecs
            .0
            .get::<mind_core::world::construct::ConstructState>(entity)
            .map(|state| (state.current, state.progress))
    }

    /// Construct block state at `(x, y)` as `(current block, progress,
    /// deconstructing)` (`ConstructBuild`; render seam).
    pub fn construct_at(&self, x: i32, y: i32) -> Option<(BlockId, f32, bool)> {
        let entity = self.tile_at(x, y)?.build?;
        let state = self
            .sim
            .ecs
            .0
            .get::<mind_core::world::ConstructState>(entity)?;
        Some((state.current, state.progress, state.active_deconstruct))
    }

    /// Owning team of the building at `(x, y)` (`Tile.team()`; `None` when the
    /// tile has no building). Rust-only seam for the block renderer's team
    /// overlays (`Block.drawTeamOverlay`).
    pub fn building_team(&self, x: i32, y: i32) -> Option<u8> {
        let entity = self.tile_at(x, y)?.build?;
        self.sim
            .ecs
            .0
            .get::<mind_core::ecs::BuildingComp>(entity)
            .map(|comp| comp.team.0)
    }

    /// Rust-only: `Build.validPlace` for the player team without mutating the
    /// sim ([`Sim::valid_place_player`], the exact check
    /// `queue_player_place_config` runs; the placement preview's
    /// `HostWorld.valid_place`).
    pub fn can_place_player(&self, x: i32, y: i32, block: BlockId, rotation: u8) -> bool {
        let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
            return false;
        };
        self.sim.valid_place_player(x, y, block, rotation)
    }
}

#[godot_api(secondary)]
impl MindSimHost {
    /// Name of the currently selected block.
    #[func]
    pub fn selected_block(&self) -> GString {
        GString::from(self.sim.block_name_of(self.sim.selected_block()).as_str())
    }

    /// Selects a block by name; `false` when the name is unknown.
    #[func]
    pub fn select_block(&mut self, name: GString) -> bool {
        let name = name.to_string();
        match self.sim.content().id(&name) {
            Ok(id) => match self.sim.set_selected_block(id) {
                Ok(()) => {
                    self.emit_state();
                    true
                }
                Err(err) => {
                    log::warn!("select_block `{name}` failed: {err}");
                    false
                }
            },
            Err(_) => {
                log::warn!("select_block: unknown block `{name}`");
                false
            }
        }
    }

    /// [`best_core_position_px`](Self::best_core_position_px) for GDScript;
    /// `(-1, -1)` before content boot or on a core-less map.
    #[func]
    pub fn best_core_position(&self) -> Vector2 {
        self.best_core_position_px()
            .map(|(x, y)| Vector2::new(x, y))
            .unwrap_or(Vector2::new(-1.0, -1.0))
    }

    /// [`building_team`](Self::building_team) for GDScript (`-1` = no building;
    /// G05 T4b `derelictmouse` probe).
    #[func]
    pub fn building_team_at(&self, x: i32, y: i32) -> i64 {
        self.building_team(x, y)
            .map(|team| team as i64)
            .unwrap_or(-1)
    }

    /// Building footprint size at `(x, y)` (`Block.size`; G03 T13 inventory
    /// anchoring). Read-only MCP/HUD probe; `1` without a building.
    #[func]
    pub fn block_size(&self, x: i32, y: i32) -> i32 {
        self.tile_at(x, y)
            .and_then(|tile| self.content_snapshot.as_ref()?.block(tile.block))
            .map(|def| def.size.max(1))
            .unwrap_or(1)
    }

    /// World-pixel centre `(Building.x, Building.y)` of the building covering
    /// tile `(x, y)` — a multiblock proxy tile resolves to its centre tile plus
    /// `Block.offset` — or `null` without a building. The block-config fragment
    /// anchors to it every frame (`Building.updateTableAlign`,
    /// `BuildingComp.java:1682-1685`) and hides once it is gone
    /// (`BlockConfigFragment.showConfig` update: `!selected.isValid()`).
    #[func]
    pub fn building_center_world(&self, x: i32, y: i32) -> Variant {
        let Some(entity) = self.tile_at(x, y).and_then(|tile| tile.build) else {
            return Variant::nil();
        };
        let Some(building) = self
            .sim
            .ecs
            .0
            .get::<mind_core::entities::comp::building::Building>(entity)
        else {
            return Variant::nil();
        };
        let offset = self
            .content_snapshot
            .as_ref()
            .and_then(|content| content.block(building.block))
            .map(|def| def.offset)
            .unwrap_or(0.0);
        Vector2::new(
            f32::from(building.tile.0) * TILESIZE_F + offset,
            f32::from(building.tile.1) * TILESIZE_F + offset,
        )
        .to_variant()
    }
}
