// SPDX-License-Identifier: GPL-3.0-only

//! `MindTileGrid` — view driver: grid lines plus one colored quad per non-air
//! block, pulled from the owning `MindSimHost` and redrawn on `world_changed`
//! (`00_FOUNDATION_IMPLEMENTATION_PLAN.md` §3.5). View only; no game rules.
//!
//! Placeholder for the plan-16 `FloorRenderer`/`BlockRenderer`; at P0 every
//! block is a flat quad (`Vars.tilesize = 8` from `mind_core::config`).

use godot::classes::{INode2D, Node2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::config::TILESIZE;
use mind_core::content::BlockId;

use crate::sim_host::MindSimHost;

/// Grid line color (white, low alpha).
const GRID_COLOR: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.08);
/// World border color (white, brighter).
const BORDER_COLOR: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.30);
/// `stone-wall` placeholder color (plan 03 swaps quads for atlas regions).
const STONE_WALL_COLOR: Color = Color::from_rgb(0.55, 0.56, 0.62);

/// Tile grid view.
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct MindTileGrid {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
}

#[godot_api]
impl INode2D for MindTileGrid {
    fn init(base: Base<Node2D>) -> Self {
        Self { base, host: None }
    }

    fn ready(&mut self) {
        // Scene-wired host (tscn-first): the spine declares SimHost as a sibling.
        if let Some(host) = self.base().try_get_node_as::<MindSimHost>("../../SimHost") {
            self.host = Some(host);
        } else {
            log::warn!("MindTileGrid: no MindSimHost at ../../SimHost (call set_host)");
        }
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let (width, height, tiles) = {
            let bound = host.bind();
            let (width, height) = bound.world_size();
            (width, height, bound.tile_blocks())
        };

        let size = TILESIZE as f32;
        let world = Vector2::new(width as f32 * size, height as f32 * size);

        for x in 0..=width {
            let px = x as f32 * size;
            self.base_mut()
                .draw_line_ex(Vector2::new(px, 0.0), Vector2::new(px, world.y), GRID_COLOR)
                .width(1.0)
                .done();
        }
        for y in 0..=height {
            let py = y as f32 * size;
            self.base_mut()
                .draw_line_ex(Vector2::new(0.0, py), Vector2::new(world.x, py), GRID_COLOR)
                .width(1.0)
                .done();
        }

        for (x, y, block) in tiles {
            let color = block_color(block);
            let rect = Rect2::new(
                Vector2::new(x as f32 * size, y as f32 * size),
                Vector2::new(size, size),
            );
            self.base_mut()
                .draw_rect_ex(rect, color)
                .filled(true)
                .done();
        }

        self.base_mut()
            .draw_rect_ex(Rect2::new(Vector2::ZERO, world), BORDER_COLOR)
            .filled(false)
            .width(2.0)
            .done();
    }
}

#[godot_api]
impl MindTileGrid {
    /// Binds the sim host the grid pulls `tile_blocks()` from.
    #[func]
    pub fn set_host(&mut self, host: Gd<MindSimHost>) {
        self.host = Some(host);
        self.base_mut().queue_redraw();
    }

    /// `world_changed` receiver (wired in `spine.tscn`): schedules a `_draw`.
    #[func]
    pub fn redraw_requested(&mut self) {
        self.base_mut().queue_redraw();
    }
}

/// Flat P0 color per block id (plan 03 replaces this with atlas regions).
fn block_color(block: u16) -> Color {
    if block == BlockId::STONE_WALL.get() {
        STONE_WALL_COLOR
    } else {
        Color::from_rgb(0.35, 0.35, 0.35)
    }
}
