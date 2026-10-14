// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Cracks for `!drawDynamic` buildings (`BlockRenderer.java:801-821`;
//! H03 T18).
//!
//! Java's `tileExtraCachedView` holds the buildings the dynamic pass skips
//! (`if(!block.drawDynamic){ ... }`, `BlockRenderer.java:582-586`) and the
//! crack loop draws `DrawingComp.drawCracks` for every damaged one each frame
//! at `Layer.blockCracks`. The port bakes those buildings' base sprites into
//! the static cache, so this tiny pass re-draws only their cracks:
//! `cracks-<size>-<i>` (`i = crack_index(healthf)`, rotation
//! `crack_rotation(tile y)`, grey 0.2 with alpha
//! `0.1 + (1 - healthf) * 0.6`; `BuildingComp.java:1236-1244`).

use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D, Texture2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::render::block_sprite::{crack_index, crack_rotation};

use crate::sim_host::MindSimHost;

use super::canvas::{self, RegionCache};

/// `Draw.colorl(0.2f, ..)` grey.
const CRACK_GREY: u8 = 0x33;

/// Cached-building crack overlay (`Node2D` under the `Layer.blockCracks` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct CachedCrackRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    node: Option<Gd<Node2D>>,
    regions: RegionCache,
    /// Diagnostics of the last `draw()` (`light_debug_json` precedent, H03
    /// T6): rows returned, rows inside the view, quads pushed.
    last_rows: usize,
    last_visible_rows: usize,
    last_draws: u64,
    last_missing_regions: usize,
}

#[godot_api]
impl INode2D for CachedCrackRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            node: None,
            regions: RegionCache::new(),
            last_rows: 0,
            last_visible_rows: 0,
            last_draws: 0,
            last_missing_regions: 0,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: CanvasItemNotification) {
        if what == CanvasItemNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, _delta: f64) {
        if self.host.is_some() {
            // Damage changes do not bump the world revision; the crack index
            // follows health, so redraw every frame.
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        // Collect every input first: `base_mut()` borrows `self` for the whole
        // draw, so no `self.*` read may follow it.
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(node) = self.node.clone() else {
            return;
        };
        let Some((min_x, min_y, w_rect, h_rect)) = self.visible_rect() else {
            return;
        };
        let (max_x, max_y) = (min_x + w_rect, min_y + h_rect);
        let mut cmds: Vec<(Gd<Texture2D>, f32, f32, f32, f32, Color)> = Vec::new();
        let rows = host.bind().cached_crack_draws();
        self.last_rows = rows.len();
        self.last_visible_rows = 0;
        self.last_missing_regions = 0;
        for (x, y, tile_y, size, healthf) in rows {
            if x < min_x || y < min_y || x > max_x || y > max_y {
                continue;
            }
            self.last_visible_rows += 1;
            let name = format!("cracks-{}-{}", size, crack_index(healthf));
            let Some(texture) = self.region(&node, &name) else {
                self.last_missing_regions += 1;
                continue;
            };
            let alpha = ((0.1 + (1.0 - healthf) * 0.6).clamp(0.0, 1.0) * 255.0) as u8;
            cmds.push((
                texture,
                x,
                y,
                size as f32 * 8.0,
                crack_rotation(tile_y),
                Color::from_rgba8(CRACK_GREY, CRACK_GREY, CRACK_GREY, alpha),
            ));
        }
        self.last_draws += 1;
        let mut base = self.base_mut();
        for (texture, x, y, edge, rot, color) in cmds {
            canvas::draw_region(&mut base, &texture, x, y, edge, edge, rot, color);
        }
    }
}

/// Playtest probe (kept): crack-pass internals — host bound, rows from
/// `cached_crack_draws`, rows inside the view, region misses, draw calls.
#[godot_api]
impl CachedCrackRenderer {
    #[func]
    pub fn crack_debug_json(&self) -> GString {
        let text = format!(
            "{{\"host\":{},\"node\":{},\"rows\":{},\"visible_rows\":{},\"region_misses\":{},\"draws\":{}}}",
            self.host.is_some(),
            self.node.is_some(),
            self.last_rows,
            self.last_visible_rows,
            self.last_missing_regions,
            self.last_draws,
        );
        GString::from(text.as_str())
    }
}

impl CachedCrackRenderer {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.node = Some(self.to_gd().upcast::<Node2D>());
        self.base_mut().queue_redraw();
    }

    /// Resolves an atlas region through the shared cache.
    fn region(&mut self, node: &Gd<Node2D>, name: &str) -> Option<Gd<Texture2D>> {
        self.regions.get(node, name)
    }

    /// World-space visible rect `(min x, min y, w, h)` from the camera
    /// transform.
    fn visible_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let camera = self
            .base()
            .try_get_node_as::<godot::classes::Camera2D>("/root/Spine/World/Camera2D")?;
        let viewport = self.base().get_viewport()?;
        let rect = viewport.get_visible_rect();
        let inverse = camera.get_canvas_transform().affine_inverse();
        let corners = [
            inverse * rect.position,
            inverse * (rect.position + Vector2::new(rect.size.x, 0.0)),
            inverse * rect.position + rect.size,
            inverse * (rect.position + Vector2::new(0.0, rect.size.y)),
        ];
        let min_x = corners.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
        let min_y = corners.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
        let max_x = corners
            .iter()
            .map(|p| p.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = corners
            .iter()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        Some((min_x, min_y, max_x - min_x, max_y - min_y))
    }
}
