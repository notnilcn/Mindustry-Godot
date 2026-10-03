// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapEditor` lifecycle (`editor/MapEditor.java`, plan 19 §3.2/§3.6).
//!
//! `begin_edit_map`/`begin_edit_image`/`adopt_world`/`resize`/`create_map`, the
//! `load(Runnable)` wrapper (`runtime_load`), the image import/export bridge and
//! the `tryCatchMapError` UI mapping. Darkeness clear + recache go through the
//! [`RenderHooks`](crate::world::RenderHooks) no-ops on the editor grid.

use std::path::PathBuf;

use crate::content::{BlockId, ContentRegistry};
use crate::io::fs::FileSystem;
use crate::io::map::{BlockPalette, PreviewImage};
use crate::io::save::SaveReadState;
use crate::io::save::state::MapSource;
use crate::maps::{Map, MapError};
use crate::world::WorldGrid;

use super::context::EditorContext;
use super::{EditorGrid, MapEditor};

impl MapEditor {
    /// `MapEditor.createMap(Fi)`: a custom map carrying the editor's tags.
    ///
    /// Width/height are supplied by the caller (the live grid size upstream).
    pub fn create_map(&self, file: PathBuf, width: i32, height: i32) -> Map {
        Map::new(file, width, height, self.tags.clone(), true, 1, -1)
    }

    /// `MapEditor.load(Runnable)`: suppresses recording while `f` runs.
    pub fn runtime_load<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let previous = self.loading;
        self.loading = true;
        let result = f(self);
        self.loading = previous;
        result
    }

    /// `beginEdit(Map)`: copy tags and load the map into the live world.
    ///
    /// `content` is taken mutably because the save reader needs a
    /// `&mut ContentRegistry` for the optional temporary mapper while the
    /// editor context owns the grid.
    pub fn begin_edit_map(
        &mut self,
        grid: &mut WorldGrid,
        content: &mut ContentRegistry,
        fs: &dyn FileSystem,
        map: &Map,
    ) -> Result<(), MapError> {
        self.tags = map.tags.clone();
        self.loading = true;
        let result = {
            let mut context = EditorContext::new(grid, content);
            let mut state = SaveReadState {
                context: Some(&mut context),
                content: Some(content),
                ..SaveReadState::default()
            };
            crate::io::map::MapIo::load_map(fs, &map.file, &mut state)
        };
        self.loading = false;
        result.map_err(MapError::from)
    }

    /// `beginEdit(Pixmap)`: resize to the image then import its color-mapped tiles.
    ///
    /// No name check upstream; the caller enforces the 800×800 limit.
    pub fn begin_edit_image(
        &mut self,
        grid: &mut WorldGrid,
        content: &ContentRegistry,
        image: &PreviewImage,
    ) -> Result<(), MapError> {
        let width = image.width as i32;
        let height = image.height as i32;
        self.reset(content);
        self.loading = true;
        grid.begin_map_load();
        grid.resize(width, height);
        let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
        for tile in grid.tiles.array_mut() {
            tile.floor = stone;
        }
        grid.end_map_load(content);

        let result = super::maps_glue::import_image_into_grid(content, image, grid);
        self.loading = false;
        result.map_err(MapError::from)
    }

    /// `updateRenderer()`: clears editor darkness and recaches the world.
    ///
    /// The port has no tile subclass, so the only state to reset is the static
    /// wall edge-darkness byte (`EditorRenderer.resize`).
    pub fn adopt_world(&mut self, grid: &mut dyn EditorGrid) {
        grid.clear_editor_darkness();
        grid.recache_all();
        self.shown_with_map = true;
    }

    /// `MapEditor.resize`: clear the op stack, then rebuild/shift the tiles.
    ///
    /// Plan 07 owns live building configs (`BuildPlan::point_config`); M1 shifts
    /// the center builds (block/team/rotation) and all tile data bytes.
    pub fn resize(
        &mut self,
        grid: &mut dyn EditorGrid,
        content: &ContentRegistry,
        width: i32,
        height: i32,
        shift_x: i32,
        shift_y: i32,
    ) {
        self.clear_op();
        grid.resize_shift(content, width, height, shift_x, shift_y);
        grid.recache_all();
    }

    /// `MapIO.writeImage`: export the environment as a color-mapped image.
    pub fn export_image(&self, content: &ContentRegistry, map: &dyn MapSource) -> PreviewImage {
        crate::io::map::write_image(&BlockPalette::of(content), map)
    }
}

/// `tryCatchMapError`: map a [`MapError`] to a `MindUi` bundle key + message.
///
/// `Outdated legacy map format` → `@editor.errornot`; `Incorrect header!` →
/// `@editor.errorheader`; everything else → `@editor.errorload` (plan 19 §3.8).
pub fn try_catch_map_error(error: &MapError) -> (&'static str, String) {
    let message = error.to_string();
    let key = if message.contains("Outdated legacy map format") {
        "@editor.errornot"
    } else if message.contains("Incorrect header!") {
        "@editor.errorheader"
    } else {
        "@editor.errorload"
    };
    (key, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::test_grid::TestGrid;

    /// `editor::tests::recording_suppressed` (plan 19 §7a) — the `is_game`/loading
    /// /generating gate for the recorder.
    #[test]
    fn skip_gate_suppresses_recording() {
        let mut editor = MapEditor::new();
        assert!(editor.should_record_ops(false, false));
        editor.loading = true;
        assert!(!editor.should_record_ops(false, false));
        editor.loading = false;
        assert!(!editor.should_record_ops(true, false), "in game");
        assert!(!editor.should_record_ops(false, true), "generating");
    }

    #[test]
    fn runtime_load_suppresses_then_restores() {
        let mut editor = MapEditor::new();
        let observed = editor.runtime_load(|editor| editor.loading);
        assert!(observed, "loading while inside runtime_load");
        assert!(!editor.loading, "restored after");
    }

    #[test]
    fn create_map_carries_tags() {
        let mut editor = MapEditor::new();
        editor.tags.insert("name".to_owned(), "My Map".to_owned());
        let map = editor.create_map(PathBuf::from("/maps/my.msav"), 16, 12);
        assert!(map.custom);
        assert_eq!(map.name(), "My Map");
        assert_eq!((map.width, map.height), (16, 12));
    }

    /// `editor::tests::resize_shift_preserves_tiles` (plan 19 §5): the in-bounds
    /// rectangle (with data bytes) shifts by `(shift_x, shift_y)`; the border
    /// becomes the default air floor.
    #[test]
    fn resize_shift_preserves_tiles_and_data() {
        let content = crate::content::test_support::test_registry();
        let mut editor = MapEditor::new();
        let mut grid = TestGrid::new(8, 8);
        let wall = content.block_id("copper-wall").unwrap();
        let ice = content.block_id("ice").unwrap();
        grid.set_floor(3, 3, ice);
        grid.set_block(3, 3, wall, 2, 1);
        grid.set_extra_data(3, 3, 0x1234);
        grid.set_data(3, 3, 7, 8, 9);

        editor.resize(&mut grid, &content, 6, 6, -1, -1);
        assert_eq!((grid.width(), grid.height()), (6, 6));
        assert_eq!(grid.block_id(2, 2), wall);
        assert_eq!(grid.floor_id(2, 2), ice);
        assert_eq!(grid.team_id(2, 2), 2);
        assert_eq!(grid.rotation(2, 2), 1);
        assert_eq!(grid.tile_data(2, 2).3, 0x1234);
        assert_eq!(grid.tile_data(2, 2).0, 7);
        // Out-of-bounds source tile (7,7) is dropped; the earlier (0,0) air
        // floor shifts to a valid slot only when it was non-default.
        assert!(!editor.can_undo(), "resize clears the op stack");
    }

    #[test]
    fn adopt_world_clears_darkness() {
        let mut editor = MapEditor::new();
        let mut grid = TestGrid::new(4, 4);
        grid.set_data(1, 1, 42, 0, 0);
        editor.adopt_world(&mut grid);
        assert_eq!(grid.tile_data(1, 1).0, 0);
        assert!(editor.shown_with_map);
    }
}
