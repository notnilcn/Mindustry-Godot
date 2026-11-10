// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `overlay` support code.
//! Upstream: logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// overlay (`LCanvas.DragLayout.draw` + `JumpCurve.draw`)
// ---------------------------------------------------------------------------

/// `MindLogicOverlay` — the per-frame jump-curve / drop-line canvas above the
/// statement column (`LCanvas.DragLayout.draw`, `JumpCurve.act`).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindLogicOverlay {
    pub(super) base: Base<Control>,
    pub(super) state: Rc<RefCell<LogicState>>,
    pub(super) dialog: Option<Gd<MindDialog>>,
}

#[godot_api]
impl IControl for MindLogicOverlay {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            state: Rc::new(RefCell::new(LogicState::default())),
            dialog: None,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_process(true);
    }

    fn process(&mut self, delta: f64) {
        if !self.base().is_visible_in_tree() {
            return;
        }
        let state = self.state.clone();
        update_curve_heights(&state, delta, portrait_of(self));
        self.base_mut().queue_redraw();
        refresh_variables(&state);
    }

    fn draw(&mut self) {
        let state = self.state.clone();
        draw_overlay(self, &state);
    }

    fn input(&mut self, event: Gd<InputEvent>) {
        if !self.base().is_visible_in_tree() {
            return;
        }
        let Some(dialog) = self.dialog.clone() else {
            return;
        };
        if !dialog.is_instance_valid() {
            return;
        }
        let Ok(key) = event.try_cast::<godot::classes::InputEventKey>() else {
            return;
        };
        if !key.is_pressed() || key.is_echo() {
            return;
        }
        let sub_open = self.state.borrow().sub_dialog.is_some();
        if key.get_keycode() == godot::global::Key::ESCAPE && sub_open {
            close_sub_dialog(&self.state, true);
            if let Some(mut viewport) = self.base().get_viewport() {
                viewport.set_input_as_handled();
            }
            return;
        }
        let enter = key.get_keycode() == godot::global::Key::ENTER
            || key.get_keycode() == godot::global::Key::KP_ENTER;
        let no_sub = self.state.borrow().sub_dialog.is_none();
        if enter && key.is_shift_pressed() && no_sub {
            let state = self.state.clone();
            let mut handle = dialog.clone();
            let mut bound = handle.bind_mut();
            if bound.is_top() {
                open_add_dialog(&mut bound, &state, -1);
                if let Some(mut viewport) = self.base().get_viewport() {
                    viewport.set_input_as_handled();
                }
            }
        }
    }
}

/// `Core.graphics.isPortrait()` (`get_viewport_rect().size.y > size.x`).
pub(super) fn portrait_of(overlay: &MindLogicOverlay) -> bool {
    let size = overlay.base().get_viewport_rect().size;
    size.y > size.x
}

/// `JumpCurve.drawCurve`'s height smoothing:
/// `uiHeight = lerp(40 + 10 * predHeight, uiHeight, 0.9^delta)` (portrait:
/// `20 + 8 * predHeight`).
pub(super) fn update_curve_heights(state: &Rc<RefCell<LogicState>>, delta: f64, portrait: bool) {
    let base = if portrait { 20.0 } else { 40.0 };
    let step = if portrait { 8.0 } else { 10.0 };
    let keep = 0.9_f32.powf((delta * 60.0) as f32);
    let jumps: Vec<i64> = state.borrow().jump_buttons.keys().copied().collect();
    let mut st = state.borrow_mut();
    for jump in jumps {
        let target = base + step * *st.pred_heights.get(&jump).unwrap_or(&0) as f32;
        let current = *st.curve_heights.get(&jump).unwrap_or(&60.0);
        st.curve_heights
            .insert(jump, target + (current - target) * keep);
    }
}

/// The translated overlay position of a global point.
pub(super) fn to_overlay(overlay: &MindLogicOverlay, global: Vector2) -> Vector2 {
    overlay.base().get_global_transform().affine_inverse() * global
}

/// `LCanvas.DragLayout.draw` + `JumpCurve.draw`: the drop line while dragging
/// and one trapezoid per `jump` from its node to the target element's right
/// edge (`JumpCurve.act`: `t.set(hover.getWidth(), hover.getHeight() / 2f)`).
pub(super) fn draw_overlay(overlay: &mut MindLogicOverlay, state: &Rc<RefCell<LogicState>>) {
    let (
        rows,
        drag_active,
        drop_index,
        jump_buttons,
        jump_over,
        statements,
        row_nodes,
        curve_heights,
        jump_selecting,
        jump_hover,
        jump_pointer,
    ) = {
        let st = state.borrow();
        (
            visible_rows(&st),
            st.drag_active,
            st.drop_index,
            st.jump_buttons.clone(),
            st.jump_over,
            st.statements.clone(),
            st.row_nodes.clone(),
            st.curve_heights.clone(),
            st.jump_selecting,
            st.jump_hover,
            st.jump_pointer,
        )
    };
    if drag_active && drop_index >= 0 && !rows.is_empty() {
        let first = rows[0].get_global_rect();
        let y = if drop_index <= 0 {
            first.position.y - 3.0
        } else if drop_index as usize >= rows.len() {
            let last = rows[rows.len() - 1].get_global_rect();
            last.end().y + 3.0
        } else {
            let above = rows[drop_index as usize - 1].get_global_rect();
            let below = rows[drop_index as usize].get_global_rect();
            (above.end().y + below.position.y) * 0.5
        };
        let left = first.position.x;
        let right = left + first.size.x;
        let from = to_overlay(overlay, Vector2::new(left, y));
        let to = to_overlay(overlay, Vector2::new(right, y));
        overlay
            .base_mut()
            .draw_line_ex(from, to, Color::from_rgba(1.0, 1.0, 1.0, 0.7))
            .width(3.0)
            .done();
    }
    let mut hovered_last: Option<i64> = None;
    for jump in jump_buttons.keys().copied() {
        // `if(button.listener.isOver()) toFront()`: the hovered curve draws last.
        if jump == jump_over {
            hovered_last = Some(jump);
            continue;
        }
        draw_jump(
            overlay,
            state,
            jump,
            &jump_buttons,
            &row_nodes,
            &statements,
            &curve_heights,
            jump_selecting,
            jump_hover,
            jump_pointer,
        );
    }
    if let Some(jump) = hovered_last {
        draw_jump(
            overlay,
            state,
            jump,
            &jump_buttons,
            &row_nodes,
            &statements,
            &curve_heights,
            jump_selecting,
            jump_hover,
            jump_pointer,
        );
    }
}

/// The rows currently in the normal layout (the dragged element is under the
/// overlay, not in `_list`).
pub(super) fn visible_rows(st: &LogicState) -> Vec<Gd<Control>> {
    let mut rows = Vec::new();
    for row in &st.row_nodes {
        if let Some(drag) = &st.drag_row
            && drag.instance_id() == row.instance_id()
        {
            continue;
        }
        if row.is_instance_valid() {
            rows.push(row.clone());
        }
    }
    rows
}

/// One `JumpCurve.draw` (`LCanvas.java:678-723`).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_jump(
    overlay: &mut MindLogicOverlay,
    state: &Rc<RefCell<LogicState>>,
    jump: i64,
    jump_buttons: &BTreeMap<i64, Gd<Button>>,
    row_nodes: &[Gd<Control>],
    statements: &[StatementRow],
    curve_heights: &BTreeMap<i64, f32>,
    jump_selecting: i64,
    jump_hover: i64,
    jump_pointer: Vector2,
) {
    let Some(button) = jump_buttons.get(&jump) else {
        return;
    };
    if !button.is_instance_valid() {
        return;
    }
    let src = to_overlay(overlay, button.get_global_rect().center());
    let mut dst = Vector2::ZERO;
    let target: i64;
    if jump == jump_selecting {
        target = jump_hover;
        if target < 0 {
            dst = to_overlay(overlay, jump_pointer);
        }
    } else if (jump as usize) < statements.len() {
        target = statements[jump as usize].jump_dest(statements.len());
        if target < 0 {
            return;
        }
    } else {
        return;
    }
    if target >= 0 {
        if target as usize >= row_nodes.len() {
            return;
        }
        let row = &row_nodes[target as usize];
        if !row.is_instance_valid() {
            return;
        }
        let rect = row.get_global_rect();
        dst = to_overlay(
            overlay,
            Vector2::new(rect.end().x, rect.position.y + rect.size.y * 0.5),
        );
    }
    if src.distance_squared_to(dst) < 0.0001 {
        return;
    }
    let color = if jump == state.borrow().jump_over {
        PLACE
    } else {
        Color::WHITE
    };
    let ui_height = *curve_heights.get(&jump).unwrap_or(&60.0);
    // Trapezoidal jumps (`drawCurve`): out by `uiHeight`, across, back in.
    let mut dy = 0.0_f32;
    if dst.y != src.y {
        dy = if dst.y > src.y { 1.0 } else { -1.0 } * ui_height * 0.5;
    }
    let corner_a = Vector2::new(src.x + ui_height, src.y + dy);
    let corner_b = Vector2::new(src.x + ui_height, dst.y - dy);
    let mut points = PackedVector2Array::new();
    if let Some(crossing) = segment_intersection(src, corner_a, dst, corner_b) {
        points.push(src);
        points.push(crossing);
        points.push(dst);
    } else {
        points.push(src);
        points.push(corner_a);
        points.push(corner_b);
        points.push(dst);
    }
    overlay
        .base_mut()
        .draw_polyline_ex(&points, color)
        .width(4.0)
        .done();
    // `Tex.logicNode.draw(t.x + s * 0.75f, t.y - s / 2f, -s, s)`: the mirrored
    // node at the target.
    let texture = state.borrow().node_texture.clone();
    let size = button.get_size();
    let s = if size.x > 0.0 {
        size.x
    } else {
        JUMP_BUTTON_SIZE
    };
    if let Some(texture) = texture {
        overlay
            .base_mut()
            .draw_set_transform_ex(Vector2::new(dst.x + s * 0.75, dst.y - s * 0.5))
            .rotation(0.0)
            .scale(Vector2::new(-1.0, 1.0))
            .done();
        overlay
            .base_mut()
            .draw_texture_rect_ex(
                &texture,
                Rect2::new(Vector2::ZERO, Vector2::new(s, s)),
                false,
            )
            .modulate(color)
            .done();
        overlay.base_mut().draw_set_transform(Vector2::ZERO);
    }
}

/// `Geometry2D.segment_intersects_segment`: the intersection point of two
/// segments, or `None` when they are parallel / do not cross.
pub(super) fn segment_intersection(
    a1: Vector2,
    a2: Vector2,
    b1: Vector2,
    b2: Vector2,
) -> Option<Vector2> {
    let denominator = (b2.y - b1.y) * (a2.x - a1.x) - (b2.x - b1.x) * (a2.y - a1.y);
    if denominator.abs() < f32::EPSILON {
        return None;
    }
    let ua = ((b2.x - b1.x) * (a1.y - b1.y) - (b2.y - b1.y) * (a1.x - b1.x)) / denominator;
    let ub = ((a2.x - a1.x) * (a1.y - b1.y) - (a2.y - a1.y) * (a1.x - b1.x)) / denominator;
    if !(0.0..=1.0).contains(&ua) || !(0.0..=1.0).contains(&ub) {
        return None;
    }
    Some(Vector2::new(
        a1.x + ua * (a2.x - a1.x),
        a1.y + ua * (a2.y - a1.y),
    ))
}
