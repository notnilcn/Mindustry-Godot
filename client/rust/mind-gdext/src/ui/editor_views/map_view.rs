// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The editor's pan/zoom/paint surface, its brush-outline/grid overlay and the camera mirror.
//! Upstream: core/src/mindustry/editor/MapView.java

use super::*;

// ---------------------------------------------------------------------------
// MapView (`map_view.gd`, `MapView.java`)
// ---------------------------------------------------------------------------

/// `MindMapView` — the editor's pan/zoom/paint surface.
///
/// Pointer events are forwarded to `/root/Spine/MindEditor`; all projection and
/// drawing primitives come back from Rust. The `ViewportContainer/SubViewport`
/// camera mirrors the editor projection (`_drive_view`) and the brush outline /
/// grid overlay is drawn on top (`_draw`).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindMapView {
    base: Base<Control>,

    /// Path to the MindEditor facade.
    #[export]
    pub editor_path: NodePath,

    editor: Option<Gd<Node>>,
    cursor: Vector2i,
    drawing: bool,
    last: Vector2i,
    start: Vector2i,
    last_tool: String,
    world_view: Option<Gd<Node>>,
    camera: Option<Gd<Camera2D>>,
    /// Middle-drag pan (`MapView.pan` while the temporary `zoom` tool is active).
    panning: bool,
    pan_position: Vector2,
}

#[godot_api]
impl IControl for MindMapView {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            editor_path: NodePath::from("/root/Spine/MindEditor"),
            editor: None,
            cursor: Vector2i::new(-1, -1),
            drawing: false,
            last: Vector2i::new(-1, -1),
            start: Vector2i::new(-1, -1),
            last_tool: String::new(),
            world_view: None,
            camera: None,
            panning: false,
            pan_position: Vector2::ZERO,
        }
    }

    fn ready(&mut self) {
        self.editor = self.resolve_editor();
        self.base_mut().set_mouse_filter(MouseFilter::STOP);
        self.camera = self.node_as::<Camera2D>("ViewportContainer/SubViewport/Camera2D");
        self.world_view = self.node_as::<Node>("ViewportContainer/SubViewport/EditorWorldView");
    }

    fn process(&mut self, delta: f64) {
        // Hidden with the shell (playtest / main menu): WASD belongs to the game.
        if !self.base().is_visible_in_tree() {
            return;
        }
        if self.editor().is_some() {
            self.pan_keys(delta);
            self.drive_view();
            self.base_mut().queue_redraw();
        }
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if self.editor().is_none() {
            return;
        }
        if let Ok(motion) = event.clone().try_cast::<InputEventMouseMotion>() {
            self.cursor = self.tile_at(motion.get_position());
            // `MapView.pan` while the middle-drag `zoom` tool is active.
            if self.panning {
                let delta = motion.get_position() - self.pan_position;
                self.pan_position = motion.get_position();
                self.call_editor(
                    "map_view_pan",
                    &[delta.x.to_variant(), delta.y.to_variant()],
                );
            } else if self.drawing {
                self.drag_to(self.cursor);
            }
            self.base_mut().queue_redraw();
            self.base_mut().accept_event();
        } else if let Ok(button) = event.try_cast::<InputEventMouseButton>() {
            let index = button.get_button_index();
            let pressed = button.is_pressed();
            if index == MouseButton::WHEEL_UP && pressed {
                self.call_editor("map_view_zoom", &[1.0f64.to_variant()]);
            } else if index == MouseButton::WHEEL_DOWN && pressed {
                self.call_editor("map_view_zoom", &[(-1.0f64).to_variant()]);
            } else if index == MouseButton::LEFT {
                if pressed {
                    let tile = self.tile_at(button.get_position());
                    self.begin(tile, false);
                } else {
                    self.end();
                }
            } else if index == MouseButton::RIGHT {
                if pressed {
                    let tile = self.tile_at(button.get_position());
                    self.begin(tile, true);
                } else {
                    self.end();
                }
            } else if index == MouseButton::MIDDLE {
                if pressed {
                    self.begin_pan(button.get_position());
                } else {
                    self.end_pan();
                }
            }
            self.base_mut().accept_event();
        }
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if self.editor().is_none() || !self.base().is_visible_in_tree() {
            return;
        }
        let Ok(key) = event.try_cast::<InputEventKey>() else {
            return;
        };
        if key.is_echo() {
            return;
        }
        let code = key.get_keycode();
        if code != Key::SHIFT && code != Key::ALT {
            return;
        }
        if key.is_pressed() {
            if self.modal() {
                return;
            }
            // `MapView.act`: Shift or Alt swaps to the temporary `pick` tool.
            if self.last_tool.is_empty() {
                self.last_tool = self.tool();
                self.set_tool("pick");
            }
        } else if !self.last_tool.is_empty() {
            let tool = self.last_tool.clone();
            self.set_tool(&tool);
            self.last_tool.clear();
        }
    }

    fn draw(&mut self) {
        let Some(editor) = self.editor() else {
            return;
        };
        let status = editor_call(&editor, "status", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let map_w = dict_i64(&status, "width");
        let map_h = dict_i64(&status, "height");
        if map_w <= 0 || map_h <= 0 {
            return;
        }
        if dict_bool(&status, "grid") {
            self.draw_grid(map_w as i32, map_h as i32);
        }
        if self.cursor.x >= 0 && self.cursor.y >= 0 {
            let size = self.base().get_size();
            let points = editor_call(
                &editor,
                "map_view_brush_outline",
                &[
                    self.cursor.x.to_variant(),
                    self.cursor.y.to_variant(),
                    size.x.to_variant(),
                    size.y.to_variant(),
                ],
            )
            .and_then(|value| value.try_to::<PackedVector2Array>().ok())
            .unwrap_or_default();
            if points.as_slice().len() >= 2 {
                let mut closed = points.clone();
                if let Some(first) = points.as_slice().first() {
                    closed.push(*first);
                }
                // `Draw.color(Pal.accent); Lines.stroke(Scl.scl(2f))`.
                self.base_mut()
                    .draw_polyline_ex(&closed, styles::ACCENT)
                    .width(2.0)
                    .done();
            }
        }
        // The view frame, drawn last: `Draw.color(Pal.accent);
        // Lines.stroke(Scl.scl(3f)); Lines.rect(x, y, width, height)`. Inset so
        // the 3 px stroke stays inside the clipped view.
        let size = self.base().get_size();
        self.base_mut()
            .draw_rect_ex(
                Rect2::new(Vector2::new(1.5, 1.5), size - Vector2::new(3.0, 3.0)),
                styles::ACCENT,
            )
            .filled(false)
            .width(3.0)
            .done();
    }
}

#[godot_api]
impl MindMapView {
    /// Drops an in-progress stroke/pan and restores a temporary tool (the shell
    /// is hiding; the view would otherwise keep the eraser/zoom/pick swap).
    #[func]
    pub fn cancel_input(&mut self) {
        self.drawing = false;
        self.panning = false;
        self.cursor = Vector2i::new(-1, -1);
        if !self.last_tool.is_empty() && self.editor().is_some() {
            let tool = self.last_tool.clone();
            self.set_tool(&tool);
        }
        self.last_tool.clear();
        self.base_mut().queue_redraw();
    }
}

impl MindMapView {
    /// One `@onready`-style node lookup with the GDScript cast semantics.
    fn node_as<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: Inherits<Node>,
    {
        self.base().get_node_or_null(path)?.try_cast::<T>().ok()
    }

    /// The live `MindEditor` facade from the exported path.
    fn resolve_editor(&self) -> Option<Gd<Node>> {
        let path = self.editor_path.clone();
        self.base().get_node_or_null(&path)
    }

    /// The stored facade, re-resolved when the exported path changed.
    fn editor(&self) -> Option<Gd<Node>> {
        if let Some(editor) = &self.editor
            && editor.is_instance_valid()
        {
            return Some(editor.clone());
        }
        self.resolve_editor()
    }

    /// `_call(method, ...)` with the facade's graceful degradation.
    fn call_editor(&self, method: &str, args: &[Variant]) {
        let Some(editor) = self.editor() else {
            return;
        };
        let _ = editor_call(&editor, method, args);
    }

    /// `str(_editor.call("tool"))`.
    fn tool(&self) -> String {
        let Some(editor) = self.editor() else {
            return String::new();
        };
        editor_call(&editor, "tool", &[])
            .and_then(|value| value.try_to::<GString>().ok())
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    /// `_editor.call("set_tool", tool)`.
    fn set_tool(&self, tool: &str) {
        self.call_editor("set_tool", &[GString::from(tool).to_variant()]);
    }

    /// A dialog or prompt is up over the editor shell (`MapEditorDialog.has_modal`).
    fn modal(&self) -> bool {
        let Some(mut shell) = self.base().get_parent() else {
            return false;
        };
        if !shell.has_method("has_modal") {
            return false;
        }
        shell
            .call("has_modal", &[])
            .try_to::<bool>()
            .unwrap_or(false)
    }

    /// `MapView.act` (`editor/MapView.java:180-185`): pan by the
    /// `move_x`/`move_y` binding axes, `offset -= axis * 15 * delta / zoom`,
    /// unless a text field has focus or Ctrl is held.
    fn pan_keys(&mut self, delta: f64) {
        if self.modal() {
            return;
        }
        if let Some(viewport) = self.base().get_viewport()
            && let Some(focus) = viewport.gui_get_focus_owner()
            && (focus.clone().try_cast::<LineEdit>().is_ok()
                || focus.clone().try_cast::<godot::classes::TextEdit>().is_ok())
        {
            return;
        }
        if Input::singleton().is_key_pressed(Key::CTRL) {
            return;
        }
        let Some(mut input) = kit::node("/root/Spine/Input") else {
            return;
        };
        if !input.has_method("binding_axis") {
            return;
        }
        let ax = variant_f64(&input.call("binding_axis", &[GString::from("move_x").to_variant()]));
        let ay = variant_f64(&input.call("binding_axis", &[GString::from("move_y").to_variant()]));
        if ax != 0.0 || ay != 0.0 {
            // `Time.delta` is in 60 Hz ticks, so one second of `delta` is 60.
            let step = 15.0 * delta * 60.0;
            // `offset_y` is Godot y-down (`-MapView.offsety`): Java's
            // `offsety -= ay * step` is `offset_y += ay * step`.
            self.call_editor(
                "map_view_pan",
                &[(-ax * step).to_variant(), (ay * step).to_variant()],
            );
        }
    }

    /// `MapView.draw`: the `EditorRenderer` is drawn into the view rect
    /// `(centerx - sclwidth/2, centery - sclheight/2, sclwidth, sclheight)`; the
    /// `SubViewport` camera mirrors that projection.
    fn drive_view(&mut self) {
        let (Some(mut camera), Some(mut world_view)) =
            (self.camera.clone(), self.world_view.clone())
        else {
            return;
        };
        let Some(editor) = self.editor() else {
            return;
        };
        let status = editor_call(&editor, "status", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let map_w = dict_i64(&status, "width");
        let map_h = dict_i64(&status, "height");
        if map_w <= 0 || map_h <= 0 {
            return;
        }
        let state = editor_call(&editor, "map_view_state", &[])
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .unwrap_or_default();
        let zoom = dict_f64(&state, "zoom");
        let zoom = if state.get("zoom").is_some() {
            zoom
        } else {
            1.0
        };
        let offset_x = dict_f64(&state, "offset_x");
        let offset_y = dict_f64(&state, "offset_y");
        let size = self.base().get_size();
        let view = size.x.min(size.y) as f64;
        let scl_w = view * zoom;
        let scl_h = scl_w * map_h as f64 / map_w as f64;
        // View-pixel position of the world origin (tile 0,0 = world 0,0).
        let origin_x = offset_x * zoom - scl_w / 2.0 + size.x as f64 / 2.0;
        let origin_y = offset_y * zoom - scl_h / 2.0 + size.y as f64 / 2.0;
        let scale_px = scl_w / map_w as f64 / 8.0;
        if scale_px == 0.0 {
            return;
        }
        camera.set_position(Vector2::new(
            ((size.x as f64 / 2.0 - origin_x) / scale_px) as f32,
            ((size.y as f64 / 2.0 - origin_y) / scale_px) as f32,
        ));
        camera.set_zoom(Vector2::new(scale_px as f32, scale_px as f32));
        // Visible world-pixel rect for the chunked editor renderer.
        let _ = world_view.call(
            "set_view_rect",
            &[
                ((-origin_x / scale_px) as f32).to_variant(),
                ((-origin_y / scale_px) as f32).to_variant(),
                ((size.x as f64 / scale_px) as f32).to_variant(),
                ((size.y as f64 / scale_px) as f32).to_variant(),
            ],
        );
    }

    /// `MapView.touchDown`: middle swaps to the `zoom` tool until release.
    fn begin_pan(&mut self, pointer_pos: Vector2) {
        self.panning = true;
        self.pan_position = pointer_pos;
        if self.last_tool.is_empty() {
            self.last_tool = self.tool();
            self.set_tool("zoom");
        }
    }

    /// `MapView.touchUp`: the temporary `zoom` tool is restored.
    fn end_pan(&mut self) {
        self.panning = false;
        if !self.last_tool.is_empty() {
            let tool = self.last_tool.clone();
            self.set_tool(&tool);
            self.last_tool.clear();
        }
    }

    /// `screenToLocalCoordinates`-style cursor tile (`MapView.project`).
    fn tile_at(&self, pointer: Vector2) -> Vector2i {
        let Some(editor) = self.editor() else {
            return Vector2i::new(-1, -1);
        };
        let size = self.base().get_size();
        let tile = editor_call(
            &editor,
            "map_view_project",
            &[
                pointer.x.to_variant(),
                pointer.y.to_variant(),
                size.x.to_variant(),
                size.y.to_variant(),
            ],
        )
        .and_then(|value| value.try_to::<Vector2>().ok());
        match tile {
            Some(tile) => Vector2i::new(tile.x as i32, tile.y as i32),
            None => Vector2i::new(-1, -1),
        }
    }

    /// `MapView.touchDown` on the paint surface.
    fn begin(&mut self, tile: Vector2i, temporary_eraser: bool) {
        self.drawing = true;
        self.last = tile;
        self.start = tile;
        if temporary_eraser {
            self.last_tool = self.tool();
            self.set_tool("eraser");
        }
        self.call_editor(
            "map_view_touch",
            &[tile.x.to_variant(), tile.y.to_variant()],
        );
        self.base_mut().queue_redraw();
    }

    /// `MapView.touchDragged` (`touchDragged` + `Bresenham2.line`).
    fn drag_to(&mut self, tile: Vector2i) {
        if !self.drawing || tile == self.last {
            return;
        }
        if self.tool() == "line" {
            // The line endpoint is committed on release (`touchedLine`).
            self.last = tile;
            return;
        }
        self.call_editor(
            "map_view_drag",
            &[
                self.last.x.to_variant(),
                self.last.y.to_variant(),
                tile.x.to_variant(),
                tile.y.to_variant(),
            ],
        );
        self.last = tile;
    }

    /// `MapView.touchUp`: commit the stroke and restore a temporary tool.
    fn end(&mut self) {
        if !self.drawing {
            return;
        }
        self.drawing = false;
        if self.tool() == "line" {
            self.call_editor(
                "dev_draw_line",
                &[
                    self.start.x.to_variant(),
                    self.start.y.to_variant(),
                    self.last.x.to_variant(),
                    self.last.y.to_variant(),
                ],
            );
        }
        self.call_editor("map_view_flush", &[]);
        if !self.last_tool.is_empty() {
            let tool = self.last_tool.clone();
            self.set_tool(&tool);
            self.last_tool.clear();
        }
        self.base_mut().queue_redraw();
    }

    /// Tile → view pixel (`MapView.unproject`).
    fn unproject(&self, tile_x: i32, tile_y: i32) -> Option<Vector2> {
        let editor = self.editor()?;
        let size = self.base().get_size();
        editor_call(
            &editor,
            "map_view_unproject",
            &[
                tile_x.to_variant(),
                tile_y.to_variant(),
                size.x.to_variant(),
                size.y.to_variant(),
            ],
        )
        .and_then(|value| value.try_to::<Vector2>().ok())
    }

    /// The quarter/centre grid lines (`MapView.java` grid overlay).
    fn draw_grid(&mut self, map_w: i32, map_h: i32) {
        let quarter = Color::from_rgba(0.98, 0.78, 0.34, 0.65);
        let center = Color::from_rgba(1.0, 0.83, 0.5, 1.0);
        for (fraction, is_center) in [(0.25f64, false), (0.5, true), (0.75, false)] {
            let color = if is_center { center } else { quarter };
            let width = if is_center { 3.0 } else { 2.0 };
            let vx = (map_w as f64 * fraction) as i32;
            let vy = (map_h as f64 * fraction) as i32;
            if let (Some(top), Some(bottom)) = (self.unproject(vx, 0), self.unproject(vx, map_h)) {
                self.base_mut()
                    .draw_line_ex(top, bottom, color)
                    .width(width)
                    .done();
            }
            if let (Some(left), Some(right)) = (self.unproject(0, vy), self.unproject(map_w, vy)) {
                self.base_mut()
                    .draw_line_ex(left, right, color)
                    .width(width)
                    .done();
            }
        }
    }
}
