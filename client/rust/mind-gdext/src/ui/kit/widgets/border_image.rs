// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/BorderImage.java (plan 14 §3.3).
//          arc/scene/ui/Image.java (`layout`: `Scaling` + `Align.center`)
//
// Texture with a colored stroke border; `forceNearest` sets the texture filter
// (Godot edition: `texture_filter` on the canvas item).
//
// Like an Arc `Image`, the widget sizes to its cell (not the texture): the
// texture is laid out inside it by `stretch_mode` (`STRETCH_KEEP_ASPECT_*` =
// `Scaling.fit`, `STRETCH_SCALE` = `Scaling.stretch`), and the border is drawn
// around the laid-out image rect (`imageX/imageY/imageWidth/imageHeight`), so
// a fitted preview is never cropped and its stroke hugs the picture.

//! `MindBorderImage` — a `TextureRect` with an Arc `BorderImage` stroke.
//!
//! The base `TextureRect` draws the texture; this class adds the optional
//! alpha backdrop and the four-edge border on top, exactly like the former
//! GDScript widget.

use godot::classes::canvas_item::TextureFilter;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{ITextureRect, TextureRect};
use godot::prelude::*;

use super::super::icon_texture;

/// `BorderImage` — `TextureRect` plus `Pal.gray` stroke.
#[derive(GodotClass)]
#[class(base=TextureRect, init)]
pub struct MindBorderImage {
    base: Base<TextureRect>,
    /// `BorderImage.thickness` stroke width, in pixels.
    #[export]
    #[init(val = 4.0)]
    pub thickness: f32,
    /// Extra distance between the image rect and the stroke (`BorderImage.pad`).
    #[export]
    pub pad: f32,
    /// `Pal.gray` (`BorderImage.borderColor` default).
    #[export]
    #[init(val = Color::from_rgba(0.271, 0.271, 0.271, 1.0))]
    pub border_color: Color,
    /// `BorderImage.forceNearest`: keeps map previews crisp.
    #[export]
    pub force_nearest: bool,
    /// `BorderImage.drawAlpha`: paint `alphaColor` behind the image.
    #[export]
    pub draw_alpha: bool,
    /// Backdrop colour used while [`Self::draw_alpha`] is set.
    #[export]
    #[init(val = Color::GRAY)]
    pub alpha_color: Color,
}

#[godot_api]
impl ITextureRect for MindBorderImage {
    /// An Arc `Image` takes its cell's size; the texture size must not become
    /// the minimum size (that pushed large map previews past their cards).
    ///
    /// The GDScript set this in `_init`, but gdext forbids touching the base
    /// object from `init` (the extension class is not bound yet: `base_mut()`
    /// fails its downcast). `enter_tree` is the earliest safe hook and still
    /// runs before the engine's `POST_ENTER_TREE` size clamp, so the cell size
    /// a container assigns later is honoured. The only residual difference is
    /// that assigning an explicit `size` *before* the node is added still
    /// clamps to the texture size; every call site sizes the widget through a
    /// container cell (`custom_minimum_size`/table width), which is laid out
    /// after `enter_tree` and therefore matches the original.
    fn enter_tree(&mut self) {
        self.base_mut().set_expand_mode(ExpandMode::IGNORE_SIZE);
    }

    fn ready(&mut self) {
        // `_ready` resets the filter from the exported flag so a scene can set
        // `force_nearest` after instantiating the node.
        let filter = if self.force_nearest {
            TextureFilter::NEAREST
        } else {
            TextureFilter::LINEAR
        };
        self.base_mut().set_texture_filter(filter);
        self.base_mut().queue_redraw();
    }

    /// The `TextureRect` base already draws the texture laid out by
    /// `stretch_mode`; this pass adds the alpha backdrop and the border stroke.
    fn draw(&mut self) {
        // Read the exported fields up front: the `base_mut()` guard borrows
        // `self` for the whole drawing statement.
        let rect = self.image_rect();
        let alpha_color = self.alpha_color;
        let border_color = self.border_color;
        let thickness = self.thickness;
        let pad = Vector2::new(self.pad, self.pad);
        if self.draw_alpha {
            self.base_mut().draw_rect(rect, alpha_color);
            if let Some(texture) = self.base().get_texture() {
                // The backdrop covers the base draw; repaint the image over it.
                self.base_mut().draw_texture_rect(&texture, rect, false);
            }
        }
        // `Lines.rect(x + imageX - pad, ...)` (`BorderImage.java:69-70`) fills
        // the four `stroke`-wide edges *inside* the rect (arc
        // `Lines.java:511-522`); Godot's unfilled `draw_rect` centres the
        // stroke on the edge, so inset by half the stroke (a 300 px preview
        // stays 300 px, java/17-mapinfo.png).
        let outer = Rect2::new(rect.position - pad, rect.size + pad * 2.0);
        let half = Vector2::new(thickness, thickness) * 0.5;
        self.base_mut()
            .draw_rect_ex(
                Rect2::new(outer.position + half, outer.size - half * 2.0),
                border_color,
            )
            .filled(false)
            .width(thickness)
            .done();
    }
}

#[godot_api]
impl MindBorderImage {
    /// Rust constructor for the GDScript `MindWidgets.border_image(region)`: the
    /// `BorderImage` with the atlas region's texture already assigned.
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn with_region(region_name: &str) -> Gd<Self> {
        let mut node = Self::new_alloc();
        if let Some(texture) = icon_texture(region_name) {
            node.bind_mut().base_mut().set_texture(&texture);
        }
        node
    }

    /// `BorderImage.border(color)`: recolours the stroke (GDScript chainable
    /// form returns the node).
    #[func]
    pub fn border(&mut self, color: Color) -> Gd<MindBorderImage> {
        self.border_color = color;
        self.base_mut().queue_redraw();
        self.to_gd()
    }

    /// `Image.layout`: the image rect inside the widget for the current
    /// `stretch_mode` (`Scaling.apply` + centered alignment).
    #[func]
    pub fn image_rect(&self) -> Rect2 {
        let size = self.base().get_size();
        let Some(texture) = self.base().get_texture() else {
            return Rect2::new(Vector2::ZERO, size);
        };
        let texture_size = texture.get_size();
        if texture_size.x <= 0.0 || texture_size.y <= 0.0 {
            return Rect2::new(Vector2::ZERO, size);
        }
        let mode = self.base().get_stretch_mode();
        if mode == StretchMode::KEEP_ASPECT || mode == StretchMode::KEEP_ASPECT_CENTERED {
            let factor = (size.x / texture_size.x).min(size.y / texture_size.y);
            let fitted = texture_size * factor;
            let origin = if mode == StretchMode::KEEP_ASPECT_CENTERED {
                (size - fitted) * 0.5
            } else {
                Vector2::ZERO
            };
            return Rect2::new(origin, fitted);
        }
        if mode == StretchMode::KEEP {
            return Rect2::new(Vector2::ZERO, texture_size);
        }
        if mode == StretchMode::KEEP_CENTERED {
            return Rect2::new((size - texture_size) * 0.5, texture_size);
        }
        // `STRETCH_SCALE`, `STRETCH_TILE`, `STRETCH_KEEP_ASPECT_COVERED`.
        Rect2::new(Vector2::ZERO, size)
    }
}
