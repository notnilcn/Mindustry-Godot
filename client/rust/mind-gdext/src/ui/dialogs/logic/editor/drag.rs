// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `drag` support code.
//! Upstream: logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// drag reorder (`StatementElem` header + `DragLayout`)
// ---------------------------------------------------------------------------

/// `StatementElem`'s header `touchDown`/`touchDragged`/`touchUp`
/// (`LCanvas.java:456-494`): left-drag reorders (the drop lands at
/// `insertPosition`), middle-click copies.
pub(super) fn header_input(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    event: Gd<InputEvent>,
    index: i64,
    header: &Gd<PanelContainer>,
) {
    let mut header = header.clone();
    if let Ok(mouse) = event.clone().try_cast::<InputEventMouseButton>() {
        if mouse.get_button_index() == MouseButton::MIDDLE && mouse.is_pressed() {
            header.accept_event();
            let can_copy = state.borrow().statements.len() < MAX_INSTRUCTIONS;
            if can_copy {
                copy_statement(dialog, state, index);
            }
            return;
        }
        if mouse.get_button_index() == MouseButton::LEFT {
            if mouse.is_pressed() {
                {
                    let mut st = state.borrow_mut();
                    st.drag_index = index;
                    st.drag_active = false;
                    st.drag_start = mouse.get_position();
                }
                header.accept_event();
            } else {
                header.accept_event();
                let finish = state.borrow().drag_active;
                state.borrow_mut().drag_index = -1;
                if finish {
                    state.borrow_mut().drag_index = index;
                    finish_drag(dialog, state);
                    state.borrow_mut().drag_index = -1;
                }
                state.borrow_mut().drag_active = false;
                if let Some(mut overlay) = state.borrow().overlay.clone() {
                    overlay.queue_redraw();
                }
            }
            return;
        }
    }
    let drag_index = state.borrow().drag_index;
    if let Ok(motion) = event.try_cast::<InputEventMouseMotion>()
        && drag_index == index
    {
        let transform = header.get_global_transform();
        let global = transform * motion.get_position();
        let start_global = transform * state.borrow().drag_start;
        let drag_started = !state.borrow().drag_active && global.distance_to(start_global) > 4.0;
        if drag_started {
            begin_drag(dialog, state, index, global);
        }
        let drag_active = state.borrow().drag_active;
        if drag_active {
            // `touchDragged`: `translation.add(v - last)` keeps the element under
            // the pointer while `DragLayout` skips it in the normal layout.
            let (drag_row, overlay) = {
                let st = state.borrow();
                (st.drag_row.clone(), st.overlay.clone())
            };
            if let (Some(mut row), Some(overlay)) = (drag_row, overlay)
                && row.is_instance_valid()
            {
                let local = overlay.get_global_transform().affine_inverse() * global;
                let grab = state.borrow().drag_grab;
                row.set_position(local - grab);
            }
            let drop = drop_index_at(state, global);
            state.borrow_mut().drop_index = drop;
            if let Some(mut overlay) = state.borrow().overlay.clone() {
                overlay.queue_redraw();
            }
        }
        header.accept_event();
    }
}

/// `StatementElem.touchDown` + `touchDragged`: remove the element from the
/// `DragLayout` flow and translate it under the pointer.
pub(super) fn begin_drag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    global: Vector2,
) {
    if index < 0 {
        return;
    }
    let (row, overlay, list) = {
        let st = state.borrow();
        if index as usize >= st.row_nodes.len() {
            return;
        }
        (
            st.row_nodes.get(index as usize).cloned(),
            st.overlay.clone(),
            st.list.clone(),
        )
    };
    let (Some(row), Some(overlay), Some(list)) = (row, overlay, list) else {
        return;
    };
    if !row.is_instance_valid() {
        return;
    }
    let overlay_local = overlay.get_global_transform().affine_inverse();
    let row_rect = row.get_global_rect();
    let grab = overlay_local * global - overlay_local * row_rect.position;
    let mut row = row;
    {
        let mut st = state.borrow_mut();
        st.drag_grab = grab;
        st.drag_row = Some(row.clone());
    }
    let mut list = list;
    list.remove_child(&row);
    let mut overlay_parent = overlay;
    overlay_parent.add_child(&row);
    row.set_position(overlay_local * row_rect.position);
    let size = row.get_size();
    row.set_size(Vector2::new(size.x.max(target_width(dialog)), size.y));
    state.borrow_mut().drag_active = true;
}

/// The per-statement drag insertion index (`DragLayout.layout`'s
/// `insertPosition`).
pub(super) fn drop_index_at(state: &Rc<RefCell<LogicState>>, global: Vector2) -> i64 {
    let (rows, drag_row) = {
        let st = state.borrow();
        (st.row_nodes.clone(), st.drag_row.clone())
    };
    let mut insert_index = 0;
    for row in &rows {
        if let Some(drag) = &drag_row
            && drag.instance_id() == row.instance_id()
        {
            continue;
        }
        if !row.is_instance_valid() {
            continue;
        }
        let rect = row.get_global_rect();
        if global.y < rect.position.y + rect.size.y * 0.5 {
            return insert_index;
        }
        insert_index += 1;
    }
    insert_index
}

/// `LCanvas.DragLayout.finishLayout`: drop the dragged element at
/// `insertPosition` and remap every `jump` target like Java's element refs do
/// (`JumpStatement.saveUI` recomputes `indexOf(dest)`).
pub(super) fn finish_drag(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let from = state.borrow().drag_index;
    let count = state.borrow().statements.len() as i64;
    if from < 0 || from >= count {
        return;
    }
    // `_drop_index_at` already excludes the dragged element, so the index is
    // the insertion position after removal (`DragLayout.insertPosition`).
    let to = state.borrow().drop_index.clamp(0, count - 1);
    if to != from {
        {
            let mut st = state.borrow_mut();
            for row in st.statements.iter_mut() {
                if row.name != "jump" {
                    continue;
                }
                let Some(slot) = row.fields.first_mut() else {
                    continue;
                };
                let Ok(mut dest) = slot.parse::<i64>() else {
                    continue;
                };
                if dest == from {
                    dest = to;
                } else if to >= from && from < dest && dest <= to {
                    dest -= 1;
                } else if to < from && to <= dest && dest < from {
                    dest += 1;
                }
                *slot = dest.to_string();
            }
            let moved = st.statements.remove(from as usize);
            st.statements.insert(to as usize, moved);
            st.modified = true;
        }
    }
    state.borrow_mut().drop_index = -1;
    rebuild_list(dialog, state);
}
