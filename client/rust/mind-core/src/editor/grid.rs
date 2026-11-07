// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-world [`EditorGrid`] adapter (plan 19 §3.4/§3.6).
//!
//! Wraps `WorldGrid` + `content` + the ECS world (building team/rotation live on
//! [`BuildingComp`]) + the plan-06/16 hooks. Tile mutation routes through
//! `WorldCtx`, keeping the plan-06 §3.13 single-writer invariant.

use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry};
use crate::ecs::{BuildingComp, TeamId};
use crate::world::ops::{WorldCtx, WorldEventLog};
use crate::world::{RenderHooks, TilePos, Tiles, WorldGrid, WorldHooks};

use super::EditorGrid;
use super::context::TileOpSink;

/// An [`EditorGrid`] over the live [`WorldGrid`].
pub struct WorldEditorGrid<'a> {
    /// The live grid.
    pub grid: &'a mut WorldGrid,
    /// Content registry.
    pub content: &'a ContentRegistry,
    /// ECS world owning building entities.
    pub ecs: &'a mut World,
    /// Building-lifecycle hooks.
    pub hooks: &'a dyn WorldHooks,
    /// Render invalidation hooks.
    pub render: &'a dyn RenderHooks,
    log: WorldEventLog,
    sink: Option<Box<dyn TileOpSink>>,
}

impl<'a> WorldEditorGrid<'a> {
    /// Wraps a world for editing.
    pub fn new(
        grid: &'a mut WorldGrid,
        content: &'a ContentRegistry,
        ecs: &'a mut World,
        hooks: &'a dyn WorldHooks,
        render: &'a dyn RenderHooks,
    ) -> Self {
        Self {
            grid,
            content,
            ecs,
            hooks,
            render,
            log: WorldEventLog::default(),
            sink: None,
        }
    }

    /// Installs the tile-op recorder (`EditorTile` replacement, plan 19 §2.3.1).
    ///
    /// The sink observes the *previous* value around each mutation; the editor
    /// installs it while editing and drops it on hide/reset. Gating on
    /// `is_loading`/`is_generating`/playing is the caller's responsibility.
    pub fn install_sink(&mut self, sink: Box<dyn TileOpSink>) {
        self.sink = Some(sink);
    }

    /// Removes and returns the installed recorder, if any.
    pub fn take_sink(&mut self) -> Option<Box<dyn TileOpSink>> {
        self.sink.take()
    }

    /// Drains the accumulated world-event log while retaining its capacity
    /// (`WorldEventLog::clear`). The sim drains world events every frame; the
    /// editor's `editor bench --suite mapview` audit calls this at the frame
    /// boundary so repeated draws do not grow the log (plan 19 §7d).
    pub fn clear_events(&mut self) {
        self.log.clear();
    }

    fn tile_build(&self, x: i32, y: i32) -> Option<bevy_ecs::entity::Entity> {
        if !self.grid.tiles.in_bounds(x, y) {
            return None;
        }
        self.grid.tiles.get(x, y).build
    }
}

impl EditorGrid for WorldEditorGrid<'_> {
    fn width(&self) -> i32 {
        self.grid.width()
    }

    fn height(&self) -> i32 {
        self.grid.height()
    }

    fn in_bounds(&self, x: i32, y: i32) -> bool {
        self.grid.tiles.in_bounds(x, y)
    }

    fn is_loading(&self) -> bool {
        // `MapEditor.load(...)` sets the world generating flag, which suppresses
        // counters/proximity (plan 06 `WorldCtx` honours `grid.generating`).
        self.grid.generating
    }

    fn set_loading(&mut self, loading: bool) {
        self.grid.set_generating(loading);
    }

    fn begin_map_load(&mut self) {
        self.grid.begin_map_load();
    }

    fn end_map_load(&mut self) {
        self.grid.end_map_load(self.content);
    }

    fn resize(&mut self, width: i32, height: i32) {
        self.grid.resize(width, height);
    }

    fn floor_id(&self, x: i32, y: i32) -> BlockId {
        if self.grid.tiles.in_bounds(x, y) {
            self.grid.tiles.get(x, y).floor
        } else {
            BlockId::AIR
        }
    }

    fn overlay_id(&self, x: i32, y: i32) -> BlockId {
        if self.grid.tiles.in_bounds(x, y) {
            self.grid.tiles.get(x, y).overlay
        } else {
            BlockId::AIR
        }
    }

    fn block_id(&self, x: i32, y: i32) -> BlockId {
        if self.grid.tiles.in_bounds(x, y) {
            self.grid.tiles.get(x, y).block
        } else {
            BlockId::AIR
        }
    }

    fn team_id(&self, x: i32, y: i32) -> u8 {
        self.tile_build(x, y)
            .and_then(|entity| self.ecs.get::<BuildingComp>(entity).map(|comp| comp.team.0))
            .unwrap_or(0)
    }

    fn rotation(&self, x: i32, y: i32) -> i32 {
        self.tile_build(x, y)
            .and_then(|entity| {
                self.ecs
                    .get::<BuildingComp>(entity)
                    .map(|comp| comp.rot as i32)
            })
            .unwrap_or(0)
    }

    fn tile_data(&self, x: i32, y: i32) -> (i8, i8, i8, i32) {
        if self.grid.tiles.in_bounds(x, y) {
            let tile = self.grid.tiles.get(x, y);
            (
                tile.data,
                tile.floor_data,
                tile.overlay_data,
                tile.extra_data,
            )
        } else {
            (0, 0, 0, 0)
        }
    }

    fn has_build(&self, x: i32, y: i32) -> bool {
        self.tile_build(x, y).is_some()
    }

    fn is_center(&self, x: i32, y: i32) -> bool {
        match self.tile_build(x, y) {
            None => true,
            Some(entity) => self
                .ecs
                .get::<BuildingComp>(entity)
                .is_none_or(|comp| comp.pos == crate::world::TilePos::new(x as i16, y as i16)),
        }
    }

    fn linked_tiles(&self, x: i32, y: i32) -> smallvec::SmallVec<[(i32, i32); 9]> {
        // Resolve the center (multiblock proxies link to the center entity).
        let center = self
            .tile_build(x, y)
            .and_then(|entity| self.ecs.get::<BuildingComp>(entity))
            .map(|comp| (comp.pos.x() as i32, comp.pos.y() as i32, comp.block))
            .unwrap_or((x, y, self.block_id(x, y)));
        let (cx, cy, block) = center;
        let size = self
            .content
            .block(block)
            .map(|def| def.size.max(1))
            .unwrap_or(1);
        let offset = -(size - 1) / 2;
        let mut out: smallvec::SmallVec<[(i32, i32); 9]> = smallvec::SmallVec::new();
        for dx in 0..size {
            for dy in 0..size {
                out.push((cx + offset + dx, cy + offset + dy));
            }
        }
        out
    }

    fn set_floor(&mut self, x: i32, y: i32, floor: BlockId) {
        let prev = self.floor_id(x, y);
        let mut ctx = WorldCtx {
            grid: self.grid,
            content: self.content,
            ecs: self.ecs,
            hooks: self.hooks,
            render: self.render,
            log: &mut self.log,
        };
        ctx.set_floor(x as i16, y as i16, floor);
        if let Some(sink) = self.sink.as_mut() {
            sink.floor_changed(x, y, prev);
        }
    }

    fn set_overlay(&mut self, x: i32, y: i32, overlay: BlockId) {
        let prev = self.overlay_id(x, y);
        let mut ctx = WorldCtx {
            grid: self.grid,
            content: self.content,
            ecs: self.ecs,
            hooks: self.hooks,
            render: self.render,
            log: &mut self.log,
        };
        ctx.set_overlay(x as i16, y as i16, overlay);
        if let Some(sink) = self.sink.as_mut() {
            sink.overlay_changed(x, y, prev);
        }
    }

    fn set_block(&mut self, x: i32, y: i32, block: BlockId, team: u8, rot: i32) {
        let prev_block = self.block_id(x, y);
        let prev_rot = self.rotation(x, y);
        let prev_team = TeamId(self.team_id(x, y));
        let was_center = self.is_center(x, y);
        let mut ctx = WorldCtx {
            grid: self.grid,
            content: self.content,
            ecs: self.ecs,
            hooks: self.hooks,
            render: self.render,
            log: &mut self.log,
        };
        ctx.set_block(x as i16, y as i16, block, team, rot as u8);
        if let Some(sink) = self.sink.as_mut() {
            sink.block_changed(x, y, prev_block, prev_rot, prev_team, was_center);
        }
    }

    fn set_team(&mut self, x: i32, y: i32, team: u8) {
        let prev = TeamId(self.team_id(x, y));
        if let Some(entity) = self.tile_build(x, y)
            && let Some(mut comp) = self.ecs.get_mut::<BuildingComp>(entity)
        {
            comp.team = TeamId(team);
            if let Some(sink) = self.sink.as_mut() {
                sink.team_changed(x, y, prev);
            }
        }
    }

    fn set_rotation(&mut self, x: i32, y: i32, rot: i32) {
        if let Some(entity) = self.tile_build(x, y)
            && let Some(mut comp) = self.ecs.get_mut::<BuildingComp>(entity)
        {
            comp.rot = rot as u8;
        }
    }

    fn set_data(&mut self, x: i32, y: i32, data: i8, floor_data: i8, overlay_data: i8) {
        if self.grid.tiles.in_bounds(x, y) {
            let tile = self.grid.tiles.get_mut(x, y);
            tile.data = data;
            tile.floor_data = floor_data;
            tile.overlay_data = overlay_data;
        }
    }

    fn set_extra_data(&mut self, x: i32, y: i32, extra: i32) {
        if self.grid.tiles.in_bounds(x, y) {
            self.grid.tiles.get_mut(x, y).extra_data = extra;
        }
    }

    fn update_static(&mut self, x: i32, y: i32) {
        self.render.recache_tile(x as i16, y as i16);
    }

    fn update_block(&mut self, x: i32, y: i32) {
        self.render.recache_tile(x as i16, y as i16);
        self.render.invalidate_tile(x as i16, y as i16);
    }

    fn clear_editor_darkness(&mut self) {
        for index in 0..self.grid.tiles.len() {
            let block = self.grid.tiles.geti(index).block;
            let static_block = self
                .content
                .block(block)
                .is_some_and(|def| crate::world::tile::is_static_kind(def.kind));
            if static_block {
                // `tile.data` is overloaded as edge darkness for static walls
                // (`EditorRenderer.resize` clears it for the editor view).
                self.grid.tiles.geti_mut(index).data = 0;
            }
        }
    }

    fn recache_all(&mut self) {
        let width = self.grid.tiles.width;
        let height = self.grid.tiles.height;
        for y in 0..height {
            for x in 0..width {
                self.render.recache_tile(x as i16, y as i16);
            }
        }
    }

    fn resize_shift(
        &mut self,
        content: &ContentRegistry,
        width: i32,
        height: i32,
        shift_x: i32,
        shift_y: i32,
    ) {
        // Capture every multiblock center build *before* remapping `x/y`
        // (`MapEditor.resize` captures the configs first, `MapEditor.java:334-340`:
        // "configs can be relative to block position (tileX/tileY) before those
        // are reassigned").
        let mut builds: Vec<(TilePos, BlockId, u8, u8, crate::world::config::ConfigValue)> =
            Vec::new();
        for index in 0..self.grid.tiles.len() {
            let tile = self.grid.tiles.geti(index);
            if let Some(entity) = tile.build
                && let Some(comp) = self.ecs.get::<BuildingComp>(entity)
                && comp.pos == TilePos::new(tile.x, tile.y)
            {
                let config = crate::world::config::read_config(&*self.ecs, entity);
                builds.push((comp.pos, comp.block, comp.team.0, comp.rot, config));
            }
        }

        let old = self.grid.tiles.clone();
        let (old_w, old_h) = (old.width, old.height);
        // `MapEditor.resize` (`MapEditor.java:310`): `offsetX = (width() - width) / 2
        // - shiftX` with `width()` still the old size, and the copy reads the old
        // tile at `offsetX + x`. The inverse below maps old `x` to
        // `x - offsetX`, i.e. the old rectangle is re-centred and then shifted.
        let offset_x = (old_w - width) / 2 - shift_x;
        let offset_y = (old_h - height) / 2 - shift_y;
        let stone = content.block_id("stone").map(|b| b.raw()).unwrap_or(0);
        let mut new_tiles = Tiles::new(width, height);
        for tile in new_tiles.array_mut() {
            tile.floor = BlockId::new(stone);
        }
        for y in 0..old_h {
            for x in 0..old_w {
                let nx = x - offset_x;
                let ny = y - offset_y;
                if !new_tiles.in_bounds(nx, ny) {
                    continue;
                }
                let src = old.get(x, y).clone();
                let dst = new_tiles.get_mut(nx, ny);
                dst.floor = src.floor;
                dst.overlay = src.overlay;
                dst.block = src.block;
                dst.data = src.data;
                dst.floor_data = src.floor_data;
                dst.overlay_data = src.overlay_data;
                dst.extra_data = src.extra_data;
            }
        }
        self.grid.tiles = new_tiles;
        self.grid.clear_buildings();

        // Reattach the shifted center builds; the ECS host spawns the entities.
        for (pos, block, team, rot, config) in builds {
            let nx = pos.x() as i32 - offset_x;
            let ny = pos.y() as i32 - offset_y;
            if !self.grid.tiles.in_bounds(nx, ny) {
                continue;
            }
            {
                let mut log = WorldEventLog::default();
                let mut ctx = WorldCtx {
                    grid: self.grid,
                    content: self.content,
                    ecs: self.ecs,
                    hooks: self.hooks,
                    render: self.render,
                    log: &mut log,
                };
                ctx.set_block(nx as i16, ny as i16, block, team, rot);
            }
            // `MapEditor.java:340-352`: `BuildPlan.pointConfig(tile.block(),
            // config, p -> { if(!tile.build.block.ignoreResizeConfig){
            // p.sub(offsetX, offsetY); } })`, re-applied with
            // `tile.build.configureAny(out)`. Blocks with relative point configs
            // (item bridges) recompute their absolute link from the *new* tile
            // position, so the plan moves with the content.
            let ignore = self
                .content
                .block(block)
                .is_some_and(|def| def.ignore_resize_config);
            let Some(shifted) =
                crate::input::plan::resize_config(&config, offset_x, offset_y, ignore)
            else {
                continue;
            };
            let Some(entity) = self.grid.tiles.get(nx, ny).build else {
                continue;
            };
            crate::world::config::configure(self.ecs, entity, None, shifted);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::world::{NoopRenderHooks, NoopWorldHooks};

    #[derive(Default)]
    struct SinkLog {
        floors: Vec<(i32, i32, BlockId)>,
        overlays: Vec<(i32, i32, BlockId)>,
        blocks: Vec<(i32, i32, BlockId, i32, TeamId, bool)>,
        teams: Vec<(i32, i32, TeamId)>,
    }

    struct Recorder(Rc<RefCell<SinkLog>>);

    impl TileOpSink for Recorder {
        fn floor_changed(&mut self, x: i32, y: i32, prev: BlockId) {
            self.0.borrow_mut().floors.push((x, y, prev));
        }
        fn overlay_changed(&mut self, x: i32, y: i32, prev: BlockId) {
            self.0.borrow_mut().overlays.push((x, y, prev));
        }
        fn block_changed(
            &mut self,
            x: i32,
            y: i32,
            prev_block: BlockId,
            prev_rot: i32,
            prev_team: TeamId,
            was_center: bool,
        ) {
            self.0
                .borrow_mut()
                .blocks
                .push((x, y, prev_block, prev_rot, prev_team, was_center));
        }
        fn team_changed(&mut self, x: i32, y: i32, prev_team: TeamId) {
            self.0.borrow_mut().teams.push((x, y, prev_team));
        }
    }

    /// `editor::tests::tile_op_sink_records_previous_values`: the recorder seam
    /// observes the value replaced by each mutation (plan 19 §3.4).
    #[test]
    fn tile_op_sink_records_previous_values() {
        let content = crate::content::test_support::test_registry();
        let mut grid = WorldGrid::new(4, 4);
        let mut ecs = World::new();
        let hooks = NoopWorldHooks;
        let render = NoopRenderHooks;
        let log = Rc::new(RefCell::new(SinkLog::default()));

        let mut editor_grid = WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
        editor_grid.install_sink(Box::new(Recorder(log.clone())));

        let stone = content.block_id("stone").unwrap();
        let ore = content.block_id("ore-copper").unwrap();
        let wall = content.block_id("stone-wall").unwrap();
        editor_grid.set_floor(1, 1, stone);
        editor_grid.set_overlay(2, 2, ore);
        editor_grid.set_block(1, 1, wall, 3, 1);

        let log = log.borrow();
        assert_eq!(log.floors, vec![(1, 1, BlockId::AIR)]);
        assert_eq!(log.overlays, vec![(2, 2, BlockId::AIR)]);
        assert_eq!(log.blocks.len(), 1);
        let (x, y, prev_block, prev_rot, prev_team, was_center) = log.blocks[0];
        assert_eq!((x, y), (1, 1));
        assert_eq!(prev_block, BlockId::AIR);
        assert_eq!(prev_rot, 0);
        assert_eq!(prev_team, TeamId(0));
        assert!(was_center);

        drop(log);
        assert!(editor_grid.take_sink().is_some());
    }

    /// Spawns behavior-backed building entities like the real host
    /// (`WorldHooks.new_building`).
    #[derive(Default)]
    struct SpawnHooks;

    impl WorldHooks for SpawnHooks {
        fn new_building(
            &self,
            world: &mut World,
            request: crate::world::NewBuilding,
        ) -> Option<bevy_ecs::entity::Entity> {
            use std::sync::atomic::{AtomicU64, Ordering};
            static SEQ: AtomicU64 = AtomicU64::new(1);
            let inst = world
                .get_resource::<crate::world::block::BlockTable>()?
                .instance(request.block)?;
            let seq = SEQ.fetch_add(1, Ordering::Relaxed);
            let tile = TilePos::new(request.x, request.y);
            // Full building state (`Block.newBuilding().create(tile)`) plus the
            // editor's `BuildingComp` marker.
            let entity = inst.spawn(world, seq, tile, request.team, request.rot, 0, 0);
            world.entity_mut(entity).insert(crate::ecs::BuildingComp {
                pos: tile,
                block: request.block,
                team: crate::ecs::TeamId(request.team),
                rot: request.rot,
            });
            Some(entity)
        }
    }

    /// `MapEditor.java:340-352` (PAR-0466): a resize captures each center
    /// build's config, moves the tiles by `-(offsetX, offsetY)` and re-applies
    /// `BuildPlan.pointConfig(tile.block(), config, p -> { if
    /// (!tile.build.block.ignoreResizeConfig){ p.sub(offsetX, offsetY); } })`
    /// with `tile.build.configureAny(out)`.
    ///
    /// `mass-driver` (`MassDriver.java:57`: relative `Point2` config, no
    /// `ignoreResizeConfig`) is shifted with its tile, while `phase-conveyor`
    /// (`ItemBridge.java:61,67`: `ignoreResizeConfig = true`, relative
    /// `Point2`) keeps the raw points — the link follows the tile instead.
    #[test]
    fn resize_shifts_relative_point_configs() {
        use crate::world::block::BlockTable;
        use crate::world::config::{ConfigValue, configure, read_config};

        let content = crate::content::test_support::test_registry();
        let mut grid = WorldGrid::new(8, 8);
        let mut ecs = World::new();
        ecs.insert_resource(BlockTable::build_default(&content).expect("block table"));
        let hooks = SpawnHooks;
        let render = NoopRenderHooks;

        let mass_driver = content.block_id("mass-driver").expect("mass-driver");
        let phase_conveyor = content.block_id("phase-conveyor").expect("phase-conveyor");

        {
            let mut editor_grid =
                WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
            editor_grid.set_block(3, 3, mass_driver, 1, 0);
            editor_grid.set_block(5, 3, phase_conveyor, 1, 0);
            let mass = editor_grid.grid.tiles.get(3, 3).build.expect("mass driver");
            configure(&mut *editor_grid.ecs, mass, None, ConfigValue::Point2(3, 3));
            let bridge = editor_grid.grid.tiles.get(5, 3).build.expect("bridge");
            configure(
                &mut *editor_grid.ecs,
                bridge,
                None,
                ConfigValue::Point2(-2, 0),
            );
            assert_eq!(
                read_config(&*editor_grid.ecs, mass),
                ConfigValue::Point2(3, 3)
            );
            assert_eq!(
                read_config(&*editor_grid.ecs, bridge),
                ConfigValue::Point2(-2, 0)
            );
        }

        // 8x8 -> 6x6 with shift(-1,-1): offset = (8-6)/2 - (-1) = 2, so old
        // (3,3) lands on new (1,1) and old (5,3) on new (3,1).
        {
            let mut editor_grid =
                WorldEditorGrid::new(&mut grid, &content, &mut ecs, &hooks, &render);
            editor_grid.resize_shift(&content, 6, 6, -1, -1);
        }

        assert_eq!(grid.tiles.get(1, 1).block, mass_driver, "mass driver moved");
        assert_eq!(
            grid.tiles.get(3, 1).block,
            phase_conveyor,
            "phase conveyor moved"
        );
        let mass = grid.tiles.get(1, 1).build.expect("mass driver reattached");
        assert_eq!(
            read_config(&ecs, mass),
            ConfigValue::Point2(1, 1),
            "the relative point moved with the tile by -(offsetX, offsetY)"
        );
        let bridge = grid.tiles.get(3, 1).build.expect("bridge reattached");
        assert_eq!(
            read_config(&ecs, bridge),
            ConfigValue::Point2(-2, 0),
            "ignoreResizeConfig keeps the raw points"
        );
    }
}
