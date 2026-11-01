// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/GridImage.java (plan 14 §3.3).
//
// Computed grid lines with a 10 px minimum spacing; `set_image_size` is used by
// Minimap/MapView-adjacent widgets.

//! `MindGridImage` — the computed grid overlay (`GridImage`).
//!
//! The Java widget draws `Pal.gray` lines over a playfield; the Godot port
//! tints the lines with the node's `modulate` (the former GDScript did the
//! same), so a themed or dimmed parent still reaches the strokes.

use godot::classes::{Control, IControl};
use godot::prelude::*;

/// `GridImage`'s minimum spacing between two drawn lines, in pixels.
const MIN_SPACING: f32 = 10.0;

/// Divisor guard for a zero-sized axis (`Math.max(0.001f, space)`).
const MIN_SPACE: f32 = 0.001;

/// `int(max(10, space) / max(0.001, space))`: how many grid steps a drawn line
/// skips when the cells would otherwise be closer than [`MIN_SPACING`].
fn jump_for(space: f32) -> i32 {
    (MIN_SPACING.max(space) / MIN_SPACE.max(space)) as i32
}

/// `MindGridImage` — `Control` drawing `image_width` × `image_height` cells.
#[derive(GodotClass)]
#[class(rename = RustMindGridImage, base=Control, init)]
pub struct MindGridImage {
    base: Base<Control>,
    /// `GridImage.imageWidth` (always at least 1; see [`Self::set_image_size`]).
    #[init(val = 1)]
    pub image_width: i32,
    /// `GridImage.imageHeight`.
    #[init(val = 1)]
    pub image_height: i32,
}

#[godot_api]
impl IControl for MindGridImage {
    fn draw(&mut self) {
        let size = self.base().get_size();
        let image_width = self.image_width.max(1);
        let image_height = self.image_height.max(1);
        let xspace = size.x / image_width as f32;
        let yspace = size.y / image_height as f32;
        let jumpx = jump_for(xspace).max(1);
        let jumpy = jump_for(yspace).max(1);
        let color = self.base().get_modulate();

        let mut x = 0i32;
        while x <= image_width {
            self.base_mut().draw_rect(
                Rect2::new(
                    Vector2::new(xspace * x as f32 - 1.0, -1.0),
                    Vector2::new(2.0, size.y + 1.0),
                ),
                color,
            );
            x += jumpx;
        }
        let mut y = 0i32;
        while y <= image_height {
            self.base_mut().draw_rect(
                Rect2::new(
                    Vector2::new(-1.0, yspace * y as f32 - 1.0),
                    Vector2::new(size.x, 2.0),
                ),
                color,
            );
            y += jumpy;
        }
    }
}

#[godot_api]
impl MindGridImage {
    /// `GridImage.setImageSize` / the GDScript `set_image_size`: clamps the
    /// grid dimensions to at least 1 and repaints.
    #[func]
    pub fn set_image_size(&mut self, w: i32, h: i32) {
        self.image_width = w.max(1);
        self.image_height = h.max(1);
        self.base_mut().queue_redraw();
    }

    /// Rust constructor for the GDScript `MindGridImage.new(w, h)` (`_init`
    /// defaults are 1×1).
    pub fn with_size(w: i32, h: i32) -> Gd<Self> {
        let mut node = Self::new_alloc();
        node.bind_mut().set_image_size(w, h);
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jump_matches_the_grid_image_formula() {
        // A cell narrower than the 10 px minimum skips enough lines to keep
        // them at least 10 px apart; a wider cell draws every line.
        assert_eq!(jump_for(1.0), 10);
        assert_eq!(jump_for(0.5), 20);
        assert_eq!(jump_for(5.0), 2);
        assert_eq!(jump_for(10.0), 1);
        assert_eq!(jump_for(20.0), 1);
        // A zero-sized axis falls through the 0.001 divisor guard.
        assert!(jump_for(0.0) >= 1);
    }
}
