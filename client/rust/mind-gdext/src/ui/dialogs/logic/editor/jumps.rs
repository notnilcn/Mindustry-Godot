// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `jumps` support code.
//! Upstream: logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// jump nodes (`LCanvas.JumpButton`)
// ---------------------------------------------------------------------------

/// `LCanvas.JumpButton` (`LCanvas.java:551-624`): a `Tex.logicNode` image, white
/// at rest and `Pal.place` while hovered.
pub(super) fn jump_button(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
) -> Gd<Button> {
    // code-instantiated: one jump node per jump statement element.
    let mut button = Button::new_alloc();
    button.set_custom_minimum_size(Vector2::new(JUMP_BUTTON_SIZE, JUMP_BUTTON_SIZE));
    button.set_focus_mode(FocusMode::NONE);
    button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    for style_name in [
        "normal",
        "hover",
        "pressed",
        "disabled",
        "focus",
        "hover_pressed",
    ] {
        button.add_theme_stylebox_override(style_name, &StyleBoxEmpty::new_gd());
    }
    let needs_node_texture = state.borrow().node_texture.is_none();
    if needs_node_texture {
        let texture = kit::icon_texture("logic-node");
        state.borrow_mut().node_texture = texture;
    }
    let texture = state.borrow().node_texture.clone();
    if let Some(texture) = texture {
        button.set_button_icon(&texture);
        button.set_expand_icon(true);
        button.set_icon_alignment(HorizontalAlignment::CENTER);
    } else {
        button.set_text("o");
    }
    button.add_theme_color_override("icon_normal_color", Color::WHITE);
    button.add_theme_color_override("icon_hover_color", PLACE);
    button.add_theme_color_override("icon_pressed_color", PLACE);
    button.add_theme_color_override("icon_hover_pressed_color", PLACE);
    {
        let state_for_enter = state.clone();
        let callable = Callable::from_fn("logic_jump_over", move |_args: &[&Variant]| {
            state_for_enter.borrow_mut().jump_over = index;
            Variant::nil()
        });
        let mut node: Gd<Node> = button.clone().upcast();
        node.connect("mouse_entered", &callable);
    }
    {
        let state_for_exit = state.clone();
        let callable = Callable::from_fn("logic_jump_out", move |_args: &[&Variant]| {
            let mut st = state_for_exit.borrow_mut();
            if st.jump_over == index {
                st.jump_over = -1;
            }
            Variant::nil()
        });
        let mut node: Gd<Node> = button.clone().upcast();
        node.connect("mouse_exited", &callable);
    }
    state
        .borrow_mut()
        .jump_buttons
        .insert(index, button.clone());
    // `JumpButton`: press-drag-release onto a target element.
    {
        let dialog_for_input = dialog.clone();
        let state_for_input = state.clone();
        let button_for_input = button.clone();
        let callable = Callable::from_fn("logic_jump_input", move |args: &[&Variant]| {
            let Some(event) = args
                .first()
                .and_then(|value| value.try_to::<Gd<InputEvent>>().ok())
            else {
                return Variant::nil();
            };
            if !dialog_for_input.is_instance_valid() {
                return Variant::nil();
            }
            let mut handle = dialog_for_input.clone();
            let mut bound = handle.bind_mut();
            jump_button_input(
                &mut bound,
                &state_for_input,
                event,
                index,
                &button_for_input,
            );
            Variant::nil()
        });
        let mut node: Gd<Node> = button.clone().upcast();
        node.connect("gui_input", &callable);
    }
    button
}

/// `JumpButton`'s touch handlers (`LCanvas.java:570-600`): press clears the
/// target and starts selecting (the curve follows the pointer / hovered
/// element), release sets the hovered element (not the jump's own) or leaves the
/// target cleared.
pub(super) fn jump_button_input(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    event: Gd<InputEvent>,
    index: i64,
    button: &Gd<Button>,
) {
    let mut button = button.clone();
    if let Ok(mouse) = event.clone().try_cast::<InputEventMouseButton>()
        && mouse.get_button_index() == MouseButton::LEFT
    {
        button.accept_event();
        let global = button.get_global_transform() * mouse.get_position();
        if mouse.is_pressed() {
            state.borrow_mut().jump_selecting = index;
            set_jump_target(state, index, -1);
            let hover = row_at(state, global);
            {
                let mut st = state.borrow_mut();
                st.jump_hover = hover;
                st.jump_pointer = global;
            }
            compute_jump_heights(state);
        } else {
            let hovered = row_at(state, global);
            if hovered >= 0 && hovered != index {
                set_jump_target(state, index, hovered);
            }
            {
                let mut st = state.borrow_mut();
                st.jump_selecting = -1;
                st.jump_hover = -1;
            }
            rebuild_list(dialog, state);
        }
        if let Some(mut overlay) = state.borrow().overlay.clone() {
            overlay.queue_redraw();
        }
        return;
    }
    let selecting = state.borrow().jump_selecting == index;
    if let Ok(motion) = event.try_cast::<InputEventMouseMotion>()
        && selecting
    {
        let pointer = button.get_global_transform() * motion.get_position();
        let hover = row_at(state, pointer);
        {
            let mut st = state.borrow_mut();
            st.jump_pointer = pointer;
            st.jump_hover = hover;
        }
        if let Some(mut overlay) = state.borrow().overlay.clone() {
            overlay.queue_redraw();
        }
        button.accept_event();
    }
}

/// The statement element under a global pointer position (`canvas.hovered`).
pub(super) fn row_at(state: &Rc<RefCell<LogicState>>, global: Vector2) -> i64 {
    let (rows, drag_row) = {
        let st = state.borrow();
        (st.row_nodes.clone(), st.drag_row.clone())
    };
    for (index, row) in rows.iter().enumerate() {
        if let Some(drag) = &drag_row
            && drag.instance_id() == row.instance_id()
        {
            continue;
        }
        if row.is_instance_valid() && row.get_global_rect().contains_point(global) {
            return index as i64;
        }
    }
    -1
}
