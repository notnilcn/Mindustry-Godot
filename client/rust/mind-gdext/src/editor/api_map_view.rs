// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The editor map-view driver: pan/zoom/paint forwarding to `MapViewDriver`.
//! Upstream: core/src/mindustry/editor/MapView.java

use super::*;

#[godot_api(secondary)]
impl MindEditor {
    // --- map view (view-only, plan 19 §3.5) ---

    /// Pan the editor view by screen-space `(dx, dy)`.
    #[func]
    pub fn map_view_pan(&mut self, dx: f64, dy: f64) {
        self.view.pan(dx as f32, dy as f32);
    }

    /// Scroll-zoom the editor view (`axis` is the wheel axis).
    #[func]
    pub fn map_view_zoom(&mut self, axis: f64) {
        self.view.zoom_by(axis as f32);
    }

    /// Reset the view pan.
    #[func]
    pub fn map_view_center(&mut self) {
        self.view.center();
    }

    /// Current pan/zoom/grid state.
    #[func]
    pub fn map_view_state(&self) -> VarDictionary {
        dict(vec![
            ("offset_x", self.view.offset_x.to_variant()),
            ("offset_y", self.view.offset_y.to_variant()),
            ("zoom", self.view.zoom.to_variant()),
            ("grid", self.view.grid.to_variant()),
        ])
    }

    /// `MapView.project`: screen pixel (Godot y-down) → tile; row 0 is the
    /// bottom of the canvas like Java's y-up scene (PAR-0109).
    #[func]
    pub fn map_view_project(
        &self,
        screen_x: f64,
        screen_y: f64,
        view_w: f64,
        view_h: f64,
    ) -> Vector2 {
        let even_block = self
            .content
            .as_ref()
            .and_then(|content| content.block(self.editor.draw_block))
            .is_some_and(|def| def.size % 2 == 0);
        let (x, y) = self.view.project(
            screen_x as f32,
            screen_y as f32,
            view_w as f32,
            view_h as f32,
            self.grid.width(),
            self.grid.height(),
            even_block,
            self.editor.tool == EditorTool::Eraser,
        );
        Vector2::new(x as f32, y as f32)
    }

    /// `MapView.unproject`: tile → screen pixel inside the view `Control`.
    #[func]
    pub fn map_view_unproject(
        &self,
        tile_x: i32,
        tile_y: i32,
        view_w: f64,
        view_h: f64,
    ) -> Vector2 {
        let (x, y) = self.view.unproject(
            tile_x,
            tile_y,
            view_w as f32,
            view_h as f32,
            self.grid.width(),
            self.grid.height(),
        );
        Vector2::new(x, y)
    }

    /// Brush outline polygon (screen space) for the cursor tile.
    #[func]
    pub fn map_view_brush_outline(
        &self,
        tile_x: i32,
        tile_y: i32,
        view_w: f64,
        view_h: f64,
    ) -> PackedVector2Array {
        let mut out = PackedVector2Array::new();
        for (x, y) in self.view.brush_outline(
            tile_x,
            tile_y,
            view_w as f32,
            view_h as f32,
            self.grid.width(),
            self.grid.height(),
            self.editor.brush_size,
        ) {
            out.push(Vector2::new(x, y));
        }
        out
    }

    /// Routes one cursor tile through the active tool
    /// (`MapView.touchDown` press; `Zoom`/`Line` are handled by the caller).
    #[func]
    pub fn map_view_touch(&mut self, tile_x: i32, tile_y: i32) -> bool {
        self.bump_tiles_rev();
        let tool = self.editor.tool;
        if tool.press_resets_saved() {
            self.editor.saved = false;
        }
        if tool == EditorTool::Line {
            return true;
        }
        let Some(()) = self.with_world(|editor, world, content| {
            mind_core::editor::tool::touched(editor, tool, world, content, tile_x, tile_y);
        }) else {
            return false;
        };
        if !tool.draggable() {
            self.editor.flush_op();
        }
        true
    }

    /// Routes a drag segment through the active draggable tool
    /// (`MapView.touchDragged` + `Bresenham2.line`).
    #[func]
    pub fn map_view_drag(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> bool {
        self.bump_tiles_rev();
        let tool = self.editor.tool;
        if !tool.draggable() {
            return false;
        }
        if tool.drag_resets_saved((x1, y1), (x2, y2)) {
            self.editor.saved = false;
        }
        if self.content().is_none() {
            return false;
        }
        let mut points: Vec<(i32, i32)> = Vec::new();
        mind_core::world::raycast::raycast_each(x1, y1, x2, y2, |x, y| {
            points.push((x, y));
            false
        });
        let _ = self.with_world(|editor, world, content| {
            for (x, y) in points {
                mind_core::editor::tool::touched(editor, tool, world, content, x, y);
            }
        });
        true
    }

    /// `MapView.touchUp`: `editor.flushOp()`.
    #[func]
    pub fn map_view_flush(&mut self) {
        self.bump_tiles_rev();
        self.editor.flush_op();
    }
}
