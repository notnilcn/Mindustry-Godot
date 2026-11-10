// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The objective marker canvas: grid, connectors, drag-to-move and the add-query cursor.
//! Upstream: core/src/mindustry/editor/MapObjectivesCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// MapObjectivesCanvas (`map_objectives_canvas.gd`, `MapObjectivesCanvas.java`)
// ---------------------------------------------------------------------------

/// `MapObjectivesCanvas.{objWidth, objHeight}` in placement units.
pub(super) const OBJ_WIDTH: i32 = 5;
pub(super) const OBJ_HEIGHT: i32 = 2;
/// `MapObjectivesCanvas.bounds`.
pub(super) const BOUNDS: i32 = 100;
/// `MapObjectivesCanvas.unitSize`.
const UNIT: f32 = 48.0;
/// `Pal.accent` marker border (`ffd37f`).
const MARKER_SELECTED: Color = Color::from_rgba8(0xff, 0xd3, 0x7f, 0xff);
/// The invalid-placement ghost (`ff6655`).
const MARKER_INVALID: Color = Color::from_rgba8(0xff, 0x66, 0x55, 0xff);
/// The default objective tile border (`Color(0.85, 0.42, 0.33)`).
const MARKER_BORDER: Color = Color::from_rgba(0.85, 0.42, 0.33, 1.0);

/// `MindMapObjectivesCanvas` — the objectives marker canvas.
///
/// Presentation only: pan, grid, node tiles with their type names, connector
/// lines, drag-to-move, the `@add` query cursor and paste targeting. The
/// objective graph data comes from `MindEditor.objectives_json`.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindMapObjectivesCanvas {
    base: Base<Control>,

    /// Path to the MindEditor facade.
    #[export]
    pub editor_path: NodePath,

    editor: Option<Gd<Node>>,
    pan: Vector2,
    dragging: bool,
    /// Index of the node under an active drag (`ObjectiveTilemap.moving`).
    moving: i64,
    move_offset: Vector2,
    /// While the `@add` query cursor is active (`isQuerying`).
    querying: bool,
    /// Highlighted node (`updateCard`-style selection state).
    selected: i64,
    /// Last mouse position delivered to this control (local space).
    cursor: Vector2,
}

#[godot_api]
impl IControl for MindMapObjectivesCanvas {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            editor_path: NodePath::from("/root/Spine/MindEditor"),
            editor: None,
            pan: Vector2::ZERO,
            dragging: false,
            moving: -1,
            move_offset: Vector2::ZERO,
            querying: false,
            selected: -1,
            cursor: Vector2::ZERO,
        }
    }

    fn ready(&mut self) {
        self.editor = self.editor();
        self.base_mut().set_mouse_filter(MouseFilter::STOP);
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
            self.cursor = button.get_position();
            self.mouse_button(button);
        } else if let Ok(motion) = event.try_cast::<InputEventMouseMotion>() {
            self.cursor = motion.get_position();
            self.mouse_motion(motion);
        }
    }

    fn draw(&mut self) {
        let objectives = self.objectives();
        self.draw_grid();
        let mut positions: Vec<Rect2> = Vec::new();
        for objective in objectives.iter_shared() {
            let value = objective.try_to::<VarDictionary>().unwrap_or_default();
            let editor_pos = dict_i64(&value, "editorPos");
            if !has_pos(editor_pos) {
                positions.push(Rect2::new(Vector2::ZERO, Vector2::ZERO));
                continue;
            }
            positions.push(self.rect_at(editor_pos));
        }
        for (from_index, to_index) in edges(&objectives) {
            let (Some(from_rect), Some(to_rect)) =
                (positions.get(from_index), positions.get(to_index))
            else {
                continue;
            };
            if from_rect.size == Vector2::ZERO || to_rect.size == Vector2::ZERO {
                continue;
            }
            self.base_mut()
                .draw_line_ex(
                    from_rect.center(),
                    to_rect.center(),
                    Color::from_rgba(0.98, 0.78, 0.34, 1.0),
                )
                .width(2.0)
                .done();
        }
        for (index, rect) in positions.iter().enumerate() {
            if rect.size == Vector2::ZERO {
                continue;
            }
            let value = objectives
                .at(index)
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            self.base_mut()
                .draw_rect_ex(*rect, Color::from_rgba(0.2, 0.2, 0.25, 0.9))
                .filled(true)
                .done();
            let selected = index as i64 == self.selected;
            self.base_mut()
                .draw_rect_ex(
                    *rect,
                    if selected {
                        MARKER_SELECTED
                    } else {
                        MARKER_BORDER
                    },
                )
                .filled(false)
                .width(if selected { 3.0 } else { 2.0 })
                .done();
            let type_label = self.type_name(&dict_string(&value, "class"));
            if type_label.is_empty() {
                continue;
            }
            let Some(font) = ThemeDb::singleton().get_fallback_font() else {
                continue;
            };
            let origin = rect.position + Vector2::new(8.0, 24.0);
            self.base()
                .draw_string_ex(&font, origin, &GString::from(type_label.as_str()))
                .alignment(HorizontalAlignment::LEFT)
                .width(rect.size.x - 16.0)
                .font_size(16)
                .modulate(Color::from_rgba(0.9, 0.9, 0.9, 1.0))
                .done();
        }
        if self.querying {
            let cell = self.query_cell(self.cursor);
            let valid = self.valid_place(cell.x, cell.y, -1);
            let ghost = Rect2::new(
                self.origin() + Vector2::new(cell.x as f32, cell.y as f32) * UNIT,
                Vector2::new(OBJ_WIDTH as f32, OBJ_HEIGHT as f32) * UNIT,
            );
            let base_color = if valid {
                MARKER_SELECTED
            } else {
                MARKER_INVALID
            };
            let mut ghost_color = base_color;
            ghost_color.a = 0.4;
            self.base_mut()
                .draw_rect_ex(ghost, ghost_color)
                .filled(true)
                .done();
            self.base_mut()
                .draw_rect_ex(ghost, base_color)
                .filled(false)
                .width(3.0)
                .done();
        }
        if self.moving >= 0 {
            let cell = self.move_cell();
            let valid = self.valid_place(cell.x, cell.y, self.moving);
            let ghost = Rect2::new(
                self.origin() + Vector2::new(cell.x as f32, cell.y as f32) * UNIT,
                Vector2::new(OBJ_WIDTH as f32, OBJ_HEIGHT as f32) * UNIT,
            );
            let mut ghost_color = if valid {
                MARKER_SELECTED
            } else {
                MARKER_INVALID
            };
            ghost_color.a = 0.5;
            self.base_mut()
                .draw_rect_ex(ghost, ghost_color)
                .filled(true)
                .done();
        }
    }
}

#[godot_api]
impl MindMapObjectivesCanvas {
    /// Emitted on a left click over one objective node (`MapObjectivesCanvas`
    /// tile selection -> `MapObjectivesDialog`'s field editors).
    #[signal]
    fn node_selected(index: i64);

    /// A node was dragged to a new grid position (`Mover.touchUp` + `moveTile`).
    #[signal]
    fn node_moved(index: i64, x: i64, y: i64);

    /// The query cursor was clicked (`tap` -> `placeQuery`).
    #[signal]
    fn query_placed(x: i64, y: i64);

    /// `updateCard`-style selection highlight.
    #[func]
    pub fn set_selected(&mut self, index: i64) {
        self.selected = index;
        self.base_mut().queue_redraw();
    }

    /// `canvas.query(obj)` / `stopQuery`.
    #[func]
    pub fn set_querying(&mut self, active: bool) {
        self.querying = active;
        self.base_mut().queue_redraw();
    }

    /// Whether the `@add` query cursor is active.
    #[func]
    pub fn is_querying(&self) -> bool {
        self.querying
    }

    /// `screenToLocalCoordinates(tilemap)` query cursor cell (`tap`/paste math).
    #[func]
    pub fn cursor_cell(&self) -> Vector2i {
        self.query_cell(self.cursor)
    }

    /// The last mouse position in canvas-local space (`Core.input.mouse()`).
    #[func]
    pub fn cursor_position(&self) -> Vector2 {
        self.cursor
    }

    /// The node under the mouse (`doInput`'s copy hit-test).
    #[func]
    pub fn node_index_at(&self, point: Vector2) -> i64 {
        self.node_at(point)
    }

    /// `ObjectiveTilemap.validPlace` for a cell (public for paste/query).
    #[func]
    pub fn cell_valid(&self, x: i64, y: i64) -> bool {
        self.valid_place(x as i32, y as i32, -1)
    }
}

impl MindMapObjectivesCanvas {
    /// The stored `MindEditor` facade, re-resolved through the exported path
    /// when it went invalid.
    fn editor(&self) -> Option<Gd<Node>> {
        cached_editor(
            &self.editor,
            &self.to_gd().upcast::<Node>(),
            &self.editor_path,
        )
    }

    /// One mouse-button branch of `_gui_input`.
    fn mouse_button(&mut self, button: Gd<InputEventMouseButton>) {
        let index = button.get_button_index();
        let pressed = button.is_pressed();
        if pressed && index == MouseButton::LEFT {
            if self.querying {
                // `tap`: place the pending objective at the cursor.
                let cell = self.query_cell(button.get_position());
                if self.valid_place(cell.x, cell.y, -1) {
                    self.emit_deferred(
                        "query_placed",
                        vec![cell.x.to_variant(), cell.y.to_variant()],
                    );
                }
                return;
            }
            // Left click selects a node; a press over one starts a drag, a
            // miss starts a pan (`Mover.touchDown` / pan gesture).
            let hit = self.node_at(button.get_position());
            if hit >= 0 {
                self.moving = hit;
                let node = Vector2::new(self.node_x(hit) as f32, self.node_y(hit) as f32) * UNIT;
                self.move_offset = self.origin() + node - button.get_position();
                self.emit_deferred("node_selected", vec![hit.to_variant()]);
                return;
            }
            self.dragging = true;
        } else if index == MouseButton::MIDDLE {
            self.dragging = pressed;
        } else if index == MouseButton::LEFT {
            if self.moving >= 0 {
                self.finish_move();
                return;
            }
            self.dragging = false;
        }
        self.base_mut().queue_redraw();
    }

    /// One mouse-motion branch of `_gui_input`.
    fn mouse_motion(&mut self, motion: Gd<InputEventMouseMotion>) {
        if self.moving >= 0 {
            self.base_mut().queue_redraw();
        } else if self.dragging {
            // `ElementGestureListener.pan`: clamp to the tilemap's valid range.
            self.pan += motion.get_relative();
            let size = self.base().get_size();
            self.pan.x = clampf(
                self.pan.x,
                -(BOUNDS as f32) * UNIT + size.x,
                BOUNDS as f32 * UNIT,
            );
            self.pan.y = clampf(
                self.pan.y,
                -(BOUNDS as f32) * UNIT + size.y,
                BOUNDS as f32 * UNIT,
            );
            self.base_mut().queue_redraw();
        }
    }

    /// Emits a canvas signal from the idle phase.
    ///
    /// `MapObjectivesDialog`'s handlers call back into this canvas
    /// (`set_querying`) and re-entering a bound GDExtension object panics
    /// (PAR-0066), so the emits leave `_gui_input`'s borrow first.
    fn emit_deferred(&self, name: &'static str, args: Vec<Variant>) {
        let target = self.to_gd();
        let callable =
            Callable::from_fn("objectives_canvas_signal", move |_args: &[&Variant]| {
                if !target.is_instance_valid() {
                    return;
                }
                let mut node = target.clone();
                node.emit_signal(name, &args);
            });
        callable.call_deferred(&[]);
    }

    /// `Mover.touchUp` + `moveTile`: snap to the grid, reject overlaps/bounds.
    fn finish_move(&mut self) {
        let index = self.moving;
        self.moving = -1;
        let objectives = self.objectives();
        if index < 0 || index >= objectives.len() as i64 {
            return;
        }
        let cell = self.move_cell();
        let old_x = self.node_x(index);
        let old_y = self.node_y(index);
        if (cell.x != old_x || cell.y != old_y)
            && self.valid_place_in(&objectives, cell.x, cell.y, index)
        {
            self.emit_deferred(
                "node_moved",
                vec![index.to_variant(), cell.x.to_variant(), cell.y.to_variant()],
            );
        }
        self.base_mut().queue_redraw();
    }

    /// The drag's snapped tile (`Mover.touchUp`'s `round(moving.x / unitSize)`).
    fn move_cell(&self) -> Vector2i {
        let local = self.cursor - self.origin() + self.move_offset;
        Vector2i::new(
            (local.x / UNIT).round() as i32,
            (local.y / UNIT).round() as i32,
        )
    }

    /// `ObjectiveTilemap.validPlace`: in bounds and not overlapping another node.
    fn valid_place(&self, x: i32, y: i32, ignore: i64) -> bool {
        let objectives = self.objectives();
        self.valid_place_in(&objectives, x, y, ignore)
    }

    /// [`Self::valid_place`] against an already parsed objective list.
    fn valid_place_in(&self, objectives: &VarArray, x: i32, y: i32, ignore: i64) -> bool {
        let others: Vec<(i32, i32)> = (0..objectives.len())
            .filter(|index| *index as i64 != ignore)
            .filter_map(|index| {
                let value = objectives
                    .at(index)
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
                let pos = dict_i64(&value, "editorPos");
                has_pos(pos).then(|| (node_x_at(pos), node_y_at(pos)))
            })
            .collect();
        valid_place_rect(x, y, &others)
    }

    /// Index of the node whose rect contains `point`, or `-1`. Nodes paint in
    /// order, so the top-most (last) hit wins.
    fn node_at(&self, point: Vector2) -> i64 {
        let objectives = self.objectives();
        for index in (0..objectives.len() as i64).rev() {
            let value = objectives
                .at(index as usize)
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            let pos = dict_i64(&value, "editorPos");
            if !has_pos(pos) {
                continue;
            }
            if self.rect_at(pos).contains_point(point) {
                return index;
            }
        }
        -1
    }

    /// `size * 0.5 + _pan`.
    fn origin(&self) -> Vector2 {
        self.base().get_size() * 0.5 + self.pan
    }

    /// The tile the mouse is over (`screenToLocalCoordinates(tilemap)` math).
    fn query_cell(&self, point: Vector2) -> Vector2i {
        let local = point - self.origin();
        Vector2i::new(
            (local.x / UNIT - OBJ_WIDTH as f32 * 0.5).round() as i32,
            (local.y / UNIT - 1.0).floor() as i32,
        )
    }

    /// `_node_x(index)`: the `editorPos` x of one objective (`-999` default).
    fn node_x(&self, index: i64) -> i32 {
        let objectives = self.objectives();
        let value = if index >= 0 && index < objectives.len() as i64 {
            objectives
                .at(index as usize)
                .try_to::<VarDictionary>()
                .unwrap_or_default()
        } else {
            VarDictionary::new()
        };
        node_x_at(dict_i64(&value, "editorPos"))
    }

    /// `_node_y(index)`: the `editorPos` y of one objective (`-999` default).
    fn node_y(&self, index: i64) -> i32 {
        let objectives = self.objectives();
        let value = if index >= 0 && index < objectives.len() as i64 {
            objectives
                .at(index as usize)
                .try_to::<VarDictionary>()
                .unwrap_or_default()
        } else {
            VarDictionary::new()
        };
        node_y_at(dict_i64(&value, "editorPos"))
    }

    /// One objective node's canvas rect from its packed `editorPos`.
    fn rect_at(&self, editor_pos: i64) -> Rect2 {
        Rect2::new(
            self.origin()
                + Vector2::new(node_x_at(editor_pos) as f32, node_y_at(editor_pos) as f32) * UNIT,
            Vector2::new(OBJ_WIDTH as f32, OBJ_HEIGHT as f32) * UNIT,
        )
    }

    /// `ObjectiveTilemap.draw`'s grid over the ±`bounds` play area.
    fn draw_grid(&mut self) {
        let origin = self.origin();
        // `Lines.stroke(3f); Draw.color(Pal.darkestGray)`.
        let color = Color::from_rgba(0.1, 0.1, 0.1, 1.0);
        let extent = BOUNDS as f32 * UNIT;
        for i in 0..(2 * BOUNDS + 1) {
            let unit = (-BOUNDS + i) as f32;
            let x = origin.x + unit * UNIT;
            self.base_mut()
                .draw_line_ex(
                    Vector2::new(x, origin.y - extent),
                    Vector2::new(x, origin.y + extent),
                    color,
                )
                .width(3.0)
                .done();
            let y = origin.y + unit * UNIT;
            self.base_mut()
                .draw_line_ex(
                    Vector2::new(origin.x - extent, y),
                    Vector2::new(origin.x + extent, y),
                    color,
                )
                .width(3.0)
                .done();
        }
    }

    /// `obj.typeName()` (`ObjectiveMarker`-style bundle lookup with a fallback).
    fn type_name(&self, tag: &str) -> String {
        if tag.is_empty() {
            return String::new();
        }
        let key = format!("objective.{}.name", tag.to_lowercase());
        if let Some(assets) = kit::assets() {
            let value = assets
                .bind()
                .bundle_get(GString::from(key.as_str()))
                .to_string();
            if value != key {
                return value;
            }
        }
        tag.to_string()
    }

    /// `MindEditor.objectives_json()` parsed as an array (`[]` when absent).
    fn objectives(&self) -> VarArray {
        let Some(mut editor) = self.editor() else {
            return VarArray::new();
        };
        if !editor.has_method("objectives_json") {
            return VarArray::new();
        }
        let text = editor
            .call("objectives_json", &[])
            .try_to::<GString>()
            .unwrap_or_default();
        Json::parse_string(&text)
            .try_to::<VarArray>()
            .unwrap_or_default()
    }
}

/// `ObjectiveTilemap.validPlace` (`MapObjectivesCanvas.java:305-326`): the
/// target is inside `[-bounds, bounds]` and its 5x2 tile rect does not overlap
/// any other placed objective's rect. Edge contact is allowed (strict
/// comparisons in Java).
pub(crate) fn valid_place_rect(x: i32, y: i32, others: &[(i32, i32)]) -> bool {
    if !(-BOUNDS..=BOUNDS).contains(&x) || !(-BOUNDS..=BOUNDS).contains(&y) {
        return false;
    }
    for &(nx, ny) in others {
        if nx < x + OBJ_WIDTH && nx + OBJ_WIDTH > x && ny < y + OBJ_HEIGHT && ny + OBJ_HEIGHT > y {
            return false;
        }
    }
    true
}

/// `Point2.pack`-style `editorPos` used by the canvas (`arc Point2.pack`:
/// **x in the high half, y in the low half**, `Point2.java:40-42`).
pub(super) fn pack_pos(x: i64, y: i64) -> i64 {
    i64::from(((x as i16 as i32) << 16) | ((y as i16 as u16) as i32))
}

/// `_has_pos(editorPos)`: the packed `(-999, -999)` sentinel means "unplaced".
pub(super) fn has_pos(editor_pos: i64) -> bool {
    editor_pos != pack_pos(-999, -999)
}

/// `_node_x_at`: the packed x with a signed 16-bit wrap.
pub(super) fn node_x_at(editor_pos: i64) -> i32 {
    let x = (editor_pos >> 16) & 0xFFFF;
    if x > 32767 {
        (x - 65536) as i32
    } else {
        x as i32
    }
}

/// `_node_y_at`: the packed y with a signed 16-bit wrap.
pub(super) fn node_y_at(editor_pos: i64) -> i32 {
    let y = editor_pos & 0xFFFF;
    if y > 32767 {
        (y - 65536) as i32
    } else {
        y as i32
    }
}

/// `_edges(objectives)`: `[parent, index]` pairs for every parent link.
fn edges(objectives: &VarArray) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for index in 0..objectives.len() {
        let value = objectives
            .at(index)
            .try_to::<VarDictionary>()
            .unwrap_or_default();
        for parent in dict_array(&value, "parents").iter_shared() {
            let parent = variant_i64(&parent);
            if parent >= 0 && (parent as usize) < objectives.len() {
                out.push((parent as usize, index));
            }
        }
    }
    out
}

/// GDScript `clampf` semantics (`min`/`max` are never swapped, so a tiny canvas
/// never panics).
fn clampf(value: f32, min: f32, max: f32) -> f32 {
    if min > max {
        return value;
    }
    value.clamp(min, max)
}
