// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MapView` view driver (plan 19 §3.5 / `editor/MapView.java`).
//!
//! Pure view math: pan/zoom clamping (`0.2..=20`), the fixed-aspect letterbox
//! `project`/`unproject` formulas (verbatim port, including the even-block
//! `-0.5` offset) and the brush-outline polygon lookup. No sim or editor state
//! lives here; `MindEditor` owns the active [`MapViewDriver`].
//!
//! ## Screen y convention (PAR-0109)
//!
//! Arc's scene is y-up: `MapView.project` measures the pointer from the
//! bottom edge of the view, so tile row 0 sits at the **bottom** of the canvas
//! and `unproject` returns a tile's lower-left corner. Godot `Control`
//! coordinates are y-down. The public functions here take and return Godot
//! (y-down) view pixels and convert at the boundary (`java_y = view_h -
//! godot_y`), so the Java formulas stay verbatim in [`java_project`] /
//! [`java_unproject`]. [`MapViewDriver::offset_y`] is stored in Godot screen
//! orientation (`= -MapView.offsety`), which keeps `map_view.gd`'s camera /
//! canvas-origin math and the pointer-drag pan (`offset += delta / zoom`)
//! unchanged.

use mind_core::world::edges::pixel_polygon;

/// Pan/zoom + projection state of one editor view.
#[derive(Debug, Clone, Copy)]
pub struct MapViewDriver {
    /// Horizontal pan (upstream `offsetx`).
    pub offset_x: f32,
    /// Vertical pan in Godot y-down orientation (`-MapView.offsety`).
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

    /// Gesture pan (`MapView.pan`): `offset += delta / zoom`. The deltas are
    /// Godot (y-down) screen deltas; with `offset_y = -offsety` this is Java's
    /// `offsety += deltaY / zoom` on the y-up delta.
    pub fn pan(&mut self, delta_x: f32, delta_y: f32) {
        self.offset_x += delta_x / self.zoom;
        self.offset_y += delta_y / self.zoom;
    }

    /// `MapView.project(screen) -> tile`, truncated toward zero like Java `(int)`.
    ///
    /// `screen_y` is a Godot (y-down) view coordinate; tile row 0 is the
    /// bottom of the canvas as in Java's y-up scene.
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
        let (x, y) = java_project(
            screen_x,
            view_h - screen_y,
            JavaView {
                width: view_w,
                height: view_h,
                offsetx: self.offset_x,
                offsety: -self.offset_y,
                zoom: self.zoom,
            },
            editor_w,
            editor_h,
        );
        if even_block && !eraser {
            ((x - 0.5) as i32, (y - 0.5) as i32)
        } else {
            (x as i32, y as i32)
        }
    }

    /// `MapView.unproject(tile) -> screen` inside the view `Control`: the
    /// tile's lower-left corner (Java y-up), returned in Godot (y-down) view
    /// pixels, i.e. the tile's bottom-left corner on screen.
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
        let (px, py) = java_unproject(
            tile_x,
            tile_y,
            JavaView {
                width: view_w,
                height: view_h,
                offsetx: self.offset_x,
                offsety: -self.offset_y,
                zoom: self.zoom,
            },
            editor_w,
            editor_h,
        );
        (px, view_h - py)
    }

    /// `MapView.draw` brush outline: `Lines.poly(brushPolygons[index], v.x,
    /// v.y, scaling)` at `v = unproject(tile)`. The polygon offsets are y-up
    /// in Java, so they are mirrored into Godot's y-down view space.
    #[allow(clippy::too_many_arguments)]
    pub fn brush_outline(
        &self,
        tile_x: i32,
        tile_y: i32,
        view_w: f32,
        view_h: f32,
        editor_w: i32,
        editor_h: i32,
        brush_size: f32,
    ) -> Vec<(f32, f32)> {
        let (cx, cy) = self.unproject(tile_x, tile_y, view_w, view_h, editor_w, editor_h);
        let scale = self.tile_scale(view_w, view_h, editor_w);
        brush_polygon(brush_size)
            .iter()
            .map(|point| (cx + point[0] * scale, cy - point[1] * scale))
            .collect()
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

/// The Java `MapView` element state the projection reads (y-up).
#[derive(Clone, Copy)]
struct JavaView {
    width: f32,
    height: f32,
    offsetx: f32,
    offsety: f32,
    zoom: f32,
}

/// Verbatim `MapView.project` (y-up view coordinates, Java `offsety`), before
/// the `(int)` truncation.
fn java_project(x: f32, y: f32, view: JavaView, editor_w: i32, editor_h: i32) -> (f32, f32) {
    let ratio = 1.0 / (editor_w as f32 / editor_h as f32);
    let size = view.width.min(view.height);
    let sclwidth = size * view.zoom;
    let sclheight = size * view.zoom * ratio;
    let x = (x - view.width / 2.0 + sclwidth / 2.0 - view.offsetx * view.zoom) / sclwidth
        * editor_w as f32;
    let y = (y - view.height / 2.0 + sclheight / 2.0 - view.offsety * view.zoom) / sclheight
        * editor_h as f32;
    (x, y)
}

/// Verbatim `MapView.unproject` (y-up view coordinates, Java `offsety`).
fn java_unproject(x: i32, y: i32, view: JavaView, editor_w: i32, editor_h: i32) -> (f32, f32) {
    let ratio = 1.0 / (editor_w as f32 / editor_h as f32);
    let size = view.width.min(view.height);
    let sclwidth = size * view.zoom;
    let sclheight = size * view.zoom * ratio;
    let px = (x as f32 / editor_w as f32) * sclwidth + view.offsetx * view.zoom - sclwidth / 2.0
        + view.width / 2.0;
    let py = (y as f32 / editor_h as f32) * sclheight + view.offsety * view.zoom - sclheight / 2.0
        + view.height / 2.0;
    (px, py)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW_W: f32 = 800.0;
    const VIEW_H: f32 = 600.0;

    fn project(view: &MapViewDriver, x: f32, y: f32, w: i32, h: i32) -> (i32, i32) {
        view.project(x, y, VIEW_W, VIEW_H, w, h, false, false)
    }

    /// PAR-0109: tile row 0 is the bottom of the canvas (Java y-up), not the top.
    #[test]
    fn row_zero_is_the_bottom_of_the_canvas() {
        let view = MapViewDriver::new();
        // 100x100 map in an 800x600 view at zoom 1: a 600x600 canvas centred
        // at (400, 300), 6 px per tile, spanning x 100..700 and y 0..600.
        assert_eq!(project(&view, 103.0, 597.0, 100, 100), (0, 0));
        assert_eq!(project(&view, 103.0, 3.0, 100, 100), (0, 99));
        assert_eq!(project(&view, 697.0, 3.0, 100, 100), (99, 99));
        assert_eq!(project(&view, 697.0, 597.0, 100, 100), (99, 0));
        // `unproject` is the tile's lower-left corner = its bottom edge on screen.
        assert_eq!(
            view.unproject(0, 0, VIEW_W, VIEW_H, 100, 100),
            (100.0, 600.0)
        );
        assert_eq!(
            view.unproject(0, 100, VIEW_W, VIEW_H, 100, 100),
            (100.0, 0.0)
        );
    }

    #[test]
    fn project_inverts_unproject_with_pan_and_zoom() {
        let mut view = MapViewDriver::new();
        view.zoom = 2.5;
        view.pan(-37.0, 52.0);
        let half = view.tile_scale(VIEW_W, VIEW_H, 128) / 2.0;
        for (tx, ty) in [(0, 0), (5, 17), (63, 2), (127, 79)] {
            let (px, py) = view.unproject(tx, ty, VIEW_W, VIEW_H, 128, 80);
            // The tile centre: half a tile right of and above the corner.
            assert_eq!(project(&view, px + half, py - half, 128, 80), (tx, ty));
        }
    }

    /// The Godot boundary is exactly Java's y-up math on `view_h - y` with
    /// `offsety = -offset_y`.
    #[test]
    fn matches_java_on_mirrored_coordinates() {
        let mut view = MapViewDriver::new();
        view.zoom = 1.7;
        view.offset_x = 12.0;
        view.offset_y = -31.0;
        let java = JavaView {
            width: VIEW_W,
            height: VIEW_H,
            offsetx: 12.0,
            offsety: 31.0,
            zoom: 1.7,
        };
        let (jx, jy) = java_project(250.0, VIEW_H - 140.0, java, 90, 60);
        assert_eq!(project(&view, 250.0, 140.0, 90, 60), (jx as i32, jy as i32));
        let (px, py) = java_unproject(7, 9, java, 90, 60);
        assert_eq!(
            view.unproject(7, 9, VIEW_W, VIEW_H, 90, 60),
            (px, VIEW_H - py)
        );
    }

    /// Dragging the pointer moves the canvas (and tile row 0) with it.
    #[test]
    fn pointer_drag_moves_the_canvas_with_the_pointer() {
        let mut view = MapViewDriver::new();
        let before = view.unproject(0, 0, VIEW_W, VIEW_H, 100, 100);
        view.pan(10.0, 20.0);
        let after = view.unproject(0, 0, VIEW_W, VIEW_H, 100, 100);
        assert_eq!((after.0 - before.0, after.1 - before.1), (10.0, 20.0));
    }

    #[test]
    fn brush_outline_mirrors_polygon_y() {
        let view = MapViewDriver::new();
        let (cx, cy) = view.unproject(10, 10, VIEW_W, VIEW_H, 100, 100);
        let scale = view.tile_scale(VIEW_W, VIEW_H, 100);
        let outline = view.brush_outline(10, 10, VIEW_W, VIEW_H, 100, 100, 3.0);
        let polygon = brush_polygon(3.0);
        assert_eq!(outline.len(), polygon.len());
        for (point, source) in outline.iter().zip(polygon) {
            assert_eq!(*point, (cx + source[0] * scale, cy - source[1] * scale));
        }
    }
}
