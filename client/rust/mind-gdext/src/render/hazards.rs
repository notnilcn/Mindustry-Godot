// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Fires and puddles (`FireComp.draw`, `PuddleComp.draw` /
//! `Liquid.drawPuddle`; H03 T7).
//!
//! * `FireComp.draw` (`FireComp.java:100-111`): `Draw.rect(regions[min(
//!   (int)animation, frames - 1)], x + randomSeedRange(y, 2), y +
//!   randomSeedRange(x, 2))` modulated by `clamp(warmup / warmupDuration)` at
//!   `Layer.effect`.
//! * `PuddleComp.draw` (`PuddleComp.java:126-130`): `Draw.z(Layer.debris - 1)`
//!   then `liquid.drawPuddle(self)` — one `Fill.circle` of radius `f * 8`, a
//!   seeded `f * 6` offset for 3 satellites of radius `f * 5`, each with the
//!   `sin(Time.time + i*532 / 25, smag)` wobble (`Liquid.java:124-149`). The
//!   port draws in the `Layer::Debris` band (the band table has no slot at
//!   `debris - 1`; one z step above Java, still below blocks).
//!
//! The sim halves are `MindSimHost::fire_draws` / `puddle_draws`; the puddle
//! satellite stream is a tiny per-draw seeded LCG (Java `Rand(id)`; positions
//! need not be bit-identical, but must be stable frame to frame).

use godot::classes::Time;
use godot::classes::notify::CanvasItemNotification;
use godot::classes::{INode2D, Node2D, Texture2D};
use godot::obj::Base;
use godot::prelude::*;

use mind_core::render::drawf;

use crate::sim_host::MindSimHost;

use super::canvas::{self, RegionCache};

/// Arc `Rand` approximation for the puddle satellites: a 64-bit LCG re-seeded
/// per draw from `Puddle.id` (Java `rand.setSeed(id); rand.random(max)`).
struct ViewRng {
    state: u64,
}

impl ViewRng {
    fn new(seed: i64) -> Self {
        Self {
            state: (seed as u64)
                .wrapping_mul(0x5851_f42d_4c95_7f2d)
                .wrapping_add(0x1405_7b7e_f767_814f),
        }
    }

    /// Next value in `[0, 1)`.
    fn unit(&mut self) -> f32 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.state >> 33) as f32) / (1u64 << 31) as f32
    }
}

/// Puddle pass (`Node2D` under the `Layer.debris` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct PuddleRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
}

#[godot_api]
impl INode2D for PuddleRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self { base, host: None }
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
            // Puddle amounts and the sine wobble change every frame.
            self.base_mut().queue_redraw();
        }
    }

    fn draw(&mut self) {
        let Some(host) = self.host.clone() else {
            return;
        };
        let Some(rect) = self.visible_rect() else {
            return;
        };
        let rows = host.bind().puddle_draws(rect);
        let t = Time::singleton().get_ticks_msec() as f32 * 0.06;
        let mut base = self.base_mut();
        for row in rows {
            // `smag = puddle.tile.floor().isLiquid ? 0.8 : 0`, `sscl = 25`.
            let smag = if row.on_liquid { 0.8 } else { 0.0 };
            let sscl = 25.0;
            let color = canvas::color_f4(row.color);
            let id = row.id as f32;
            canvas::fill_circle(
                &mut base,
                row.x + drawf::sin_scl(t + id * 532.0, sscl, smag),
                row.y + drawf::sin_scl(t + id * 53.0, sscl, smag),
                row.f * 8.0,
                color,
            );
            // `length = f * 6`; `rand.setSeed(id)`; 3 satellites.
            let length = row.f * 6.0;
            let mut rng = ViewRng::new(row.id as i64);
            for i in 0..3 {
                let (vx, vy) = drawf::trns(rng.unit() * 360.0, rng.unit() * length);
                canvas::fill_circle(
                    &mut base,
                    row.x + vx + drawf::sin_scl(t + i as f32 * 532.0, sscl, smag),
                    row.y + vy + drawf::sin_scl(t + i as f32 * 53.0, sscl, smag),
                    row.f * 5.0,
                    color,
                );
            }
        }
    }
}

impl PuddleRenderer {
    /// Rebuilds Godot-derived refs (`ready` + hot reload).
    fn bootstrap(&mut self) {
        self.host = self
            .base()
            .try_get_node_as::<MindSimHost>("/root/Spine/SimHost");
        self.base_mut().queue_redraw();
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

/// Fire pass (`Node2D` under the `Layer.effect` band).
#[derive(GodotClass)]
#[class(base=Node2D)]
pub struct FireRenderer {
    base: Base<Node2D>,
    host: Option<Gd<MindSimHost>>,
    node: Option<Gd<Node2D>>,
    regions: RegionCache,
}

#[godot_api]
impl INode2D for FireRenderer {
    fn init(base: Base<Node2D>) -> Self {
        Self {
            base,
            host: None,
            node: None,
            regions: RegionCache::new(),
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
            // The flame frame/warmup alpha change every tick.
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
        let Some(rect) = self.visible_rect() else {
            return;
        };
        let mut cmds: Vec<(Gd<Texture2D>, f32, f32, f32, f32, f32)> = Vec::new();
        for row in host.bind().fire_draws(rect) {
            let name = format!("fire{}", row.frame);
            let Some(texture) = self.region(&node, &name) else {
                continue;
            };
            let (w, h) = RegionCache::natural_size(&texture);
            cmds.push((texture, row.x, row.y, w, h, row.alpha));
        }
        let mut base = self.base_mut();
        for (texture, x, y, w, h, alpha) in cmds {
            canvas::draw_region(
                &mut base,
                &texture,
                x,
                y,
                w,
                h,
                0.0,
                Color::from_rgba(1.0, 1.0, 1.0, alpha),
            );
        }
    }
}

impl FireRenderer {
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

#[cfg(test)]
mod tests {
    use super::ViewRng;

    #[test]
    fn view_rng_is_stable_for_a_seed() {
        let mut a = ViewRng::new(7);
        let mut b = ViewRng::new(7);
        let first = a.unit();
        assert_eq!(first, b.unit());
        assert!((0.0..1.0).contains(&first));
        assert_ne!(first, a.unit());
    }
}
