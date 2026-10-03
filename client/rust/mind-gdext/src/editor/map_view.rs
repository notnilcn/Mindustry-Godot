// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapView` view driver (plan 19 §3.5 / `editor/MapView.java`).
//!
//! Pure view math: pan/zoom clamping (`0.2..=20`), the fixed-aspect letterbox
//! `project`/`unproject` formulas (verbatim port, including the even-block
//! `-0.5` offset) and the brush-outline polygon lookup. No sim or editor state
//! lives here; `MindEditor` owns the active [`MapViewDriver`].

use mind_core::world::edges::pixel_polygon;

/// Pan/zoom + projection state of one editor view.
#[derive(Debug, Clone, Copy)]
pub struct MapViewDriver {
    /// Horizontal pan in tiles (upstream `offsetx`).
    pub offset_x: f32,
    /// Vertical pan in tiles (upstream `offsety`).
    pub offset_y: f32,
    /// Zoom factor (upstream `zoom`, clamped `0.2..=20`).
    pub zoom: f32,
    /// Grid overlay (`ctrl+g`).
    pub grid: bool,
}

impl Default for MapViewDriver {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            zoom: 1.0,
            grid: false,
        }
    }
}

impl MapViewDriver {
    /// A fresh view (`MapView()`).
    pub fn new() -> Self {
        Self::default()
    }

    /// `MapView.center()`: resets the pan offset.
    pub fn center(&mut self) {
        self.offset_x = 0.0;
        self.offset_y = 0.0;
    }

    /// `clampZoom()`: `Mathf.clamp(zoom, 0.2, 20)`.
    pub fn clamp_zoom(&mut self) {
        self.zoom = self.zoom.clamp(0.2, 20.0);
    }

    /// Scroll zoom (`MapView.act`): `zoom += axis / 10 * zoom`.
    pub fn zoom_by(&mut self, axis: f32) {
        self.zoom += axis / 10.0 * self.zoom;
        self.clamp_zoom();
    }

    /// Gesture pan (`MapView.pan`): `offset += delta / zoom`.
    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        self.offset_x += delta_x / self.zoom;
        self.offset_y += delta_y / self.zoom;
    }

    /// `MapView.project(screen) -> tile`, truncated toward zero like Java `(int)`.
    #[allow(clippy::too_many_arguments)]
    pub fn project(
        &self,
        screen_x: f32,
        screen_y: f32,
        view_w: f32,
        view_h: f32,
        editor_w: i32,
        editor_h: i32,
        even_block: bool,
        eraser: bool,
    ) -> (i32, i32) {
        if editor_w <= 0 || editor_h <= 0 || view_w <= 0.0 || view_h <= 0.0 {
            return (0, 0);
        }
        let ratio = 1.0 / (editor_w as f32 / editor_h as f32);
        let size = view_w.min(view_h);
        let scl_width = size * self.zoom;
        let scl_height = size * self.zoom * ratio;
        let x = (screen_x - view_w / 2.0 + scl_width / 2.0 - self.offset_x * self.zoom) / scl_width
            * editor_w as f32;
        let y = (screen_y - view_h / 2.0 + scl_height / 2.0 - self.offset_y * self.zoom)
            / scl_height
            * editor_h as f32;
        if even_block && !eraser {
            ((x - 0.5) as i32, (y - 0.5) as i32)
        } else {
            (x as i32, y as i32)
        }
    }

    /// `MapView.unproject(tile) -> screen` inside the view `Control`.
    pub fn unproject(
        &self,
        tile_x: i32,
        tile_y: i32,
        view_w: f32,
        view_h: f32,
        editor_w: i32,
        editor_h: i32,
    ) -> (f32, f32) {
        if editor_w <= 0 || editor_h <= 0 {
            return (0.0, 0.0);
        }
        let ratio = 1.0 / (editor_w as f32 / editor_h as f32);
        let size = view_w.min(view_h);
        let scl_width = size * self.zoom;
        let scl_height = size * self.zoom * ratio;
        let px = (tile_x as f32 / editor_w as f32) * scl_width + self.offset_x * self.zoom
            - scl_width / 2.0
            + view_w / 2.0;
        let py = (tile_y as f32 / editor_h as f32) * scl_height + self.offset_y * self.zoom
            - scl_height / 2.0
            + view_h / 2.0;
        (px, py)
    }

    /// The scale factor of one tile in screen pixels
    /// (`MapView.draw`: `zoom * min(width, height) / editor.width()`).
    pub fn tile_scale(&self, view_w: f32, view_h: f32, editor_w: i32) -> f32 {
        if editor_w <= 0 {
            return 1.0;
        }
        self.zoom * view_w.min(view_h) / editor_w as f32
    }
}

/// `Edges.pixelPolygon(radius)` clamped to the supported `1..=12` table.
pub fn brush_polygon(brush_size: f32) -> &'static [[f32; 2]] {
    pixel_polygon(brush_size.clamp(1.0, 12.0))
}
