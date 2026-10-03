// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-world [`EditorGrid`] adapter (plan 19 §3.4/§3.6).
//!
//! Wraps `WorldGrid` + `content` + the ECS world (building team/rotation live on
//! [`BuildingComp`]) + the plan-06/16 hooks. Tile mutation routes through
//! `WorldCtx`, keeping the plan-06 §3.13 single-writer invariant.

use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry};
use crate::ecs::BuildingComp;
use crate::world::ops::{WorldCtx, WorldEventLog};
use crate::world::{RenderHooks, WorldGrid, WorldHooks};

use super::EditorGrid;

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
        }
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

    fn linked_tiles(&self, x: i32, y: i32) -> Vec<(i32, i32)> {
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
        let mut out = Vec::with_capacity((size * size) as usize);
        for dx in 0..size {
            for dy in 0..size {
                out.push((cx + offset + dx, cy + offset + dy));
            }
        }
        out
    }

    fn set_floor(&mut self, x: i32, y: i32, floor: BlockId) {
        let mut ctx = WorldCtx {
            grid: self.grid,
            content: self.content,
            ecs: self.ecs,
            hooks: self.hooks,
            render: self.render,
            log: &mut self.log,
        };
        ctx.set_floor(x as i16, y as i16, floor);
    }

    fn set_overlay(&mut self, x: i32, y: i32, overlay: BlockId) {
        let mut ctx = WorldCtx {
            grid: self.grid,
            content: self.content,
            ecs: self.ecs,
            hooks: self.hooks,
            render: self.render,
            log: &mut self.log,
        };
        ctx.set_overlay(x as i16, y as i16, overlay);
    }

    fn set_block(&mut self, x: i32, y: i32, block: BlockId, team: u8, rot: i32) {
        let mut ctx = WorldCtx {
            grid: self.grid,
            content: self.content,
            ecs: self.ecs,
            hooks: self.hooks,
            render: self.render,
            log: &mut self.log,
        };
        ctx.set_block(x as i16, y as i16, block, team, rot as u8);
    }

    fn set_team(&mut self, x: i32, y: i32, team: u8) {
        if let Some(entity) = self.tile_build(x, y)
            && let Some(mut comp) = self.ecs.get_mut::<BuildingComp>(entity)
        {
            comp.team = crate::ecs::TeamId(team);
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
}
