// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `canvas` support code.
//! Upstream: logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// statement column (`LCanvas`/`DragLayout`)
// ---------------------------------------------------------------------------

/// Rebuilds the statement column from [`LogicState::statements`].
pub(super) fn rebuild_list(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let (list, drag_row, keep_scroll) = {
        let st = state.borrow();
        (
            st.list.clone(),
            st.drag_row.clone(),
            st.scroll
                .as_ref()
                .map(|scroll| scroll.get_v_scroll())
                .unwrap_or(0),
        )
    };
    // A dragged element lives under the overlay while it follows the pointer;
    // the rebuild always owns the final element list (`finishLayout`).
    if let Some(mut row) = drag_row
        && row.is_instance_valid()
    {
        if let Some(mut parent) = row.get_parent() {
            parent.remove_child(&row);
        }
        row.queue_free();
    }
    {
        let mut st = state.borrow_mut();
        st.drag_row = None;
        st.drag_active = false;
    }
    let Some(mut list) = list else {
        return;
    };
    kit::clear_children(&mut list);
    {
        let mut st = state.borrow_mut();
        st.row_nodes.clear();
        st.jump_buttons.clear();
        st.jump_over = -1;
    }
    let rows = state.borrow().statements.clone();
    for (index, row) in rows.iter().enumerate() {
        let panel = statement_row(dialog, state, index as i64, row);
        list.add_child(&panel);
        state.borrow_mut().row_nodes.push(panel);
    }
    compute_jump_heights(state);
    if let Some(mut scroll) = state.borrow().scroll.clone() {
        scroll.set_deferred("scroll_vertical", &keep_scroll.to_variant());
    }
    update_buttons(state);
    if let Some(mut overlay) = state.borrow().overlay.clone() {
        overlay.queue_redraw();
    }
}

/// One `LCanvas.StatementElem` (`LCanvas.java:417-510`): a `Tex.whitePane`
/// frame tinted with the category colour over a drop shadow and a 30% black
/// fill (`draw`), the 38 px `Tex.whiteui` header (name, growX spacer, address,
/// `+`/copy/delete `logici` buttons) and the statement's field row.
pub(super) fn statement_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    index: i64,
    row: &StatementRow,
) -> Gd<Control> {
    let statement_name = row.name.clone();
    let category = row.category.clone();
    let color = category_color(&category);
    let full = state.borrow().statements.len() >= MAX_INSTRUCTIONS;
    let dialog_gd = dialog.to_gd();
    // code-instantiated: one element per statement; the count is the program
    // length, not scene-authorable.
    let mut elem = PanelContainer::new_alloc();
    elem.set_custom_minimum_size(Vector2::new(target_width(dialog), 0.0));
    elem.set_mouse_filter(MouseFilter::PASS);
    elem.add_theme_stylebox_override("panel", &shadow_box());
    let mut frame = PanelContainer::new_alloc();
    frame.set_mouse_filter(MouseFilter::PASS);
    frame.add_theme_stylebox_override("panel", &white_pane_box(state, color));
    elem.add_child(&frame);
    let mut column = factory::vbox(0);
    column.set_mouse_filter(MouseFilter::PASS);
    frame.add_child(&column);

    let mut header = PanelContainer::new_alloc();
    header.set_custom_minimum_size(Vector2::new(0.0, 38.0));
    header.set_mouse_filter(MouseFilter::STOP);
    header.set_default_cursor_shape(CursorShape::POINTING_HAND);
    let mut header_box = StyleBoxFlat::new_gd();
    header_box.set_bg_color(color);
    for side in [Side::LEFT, Side::RIGHT, Side::TOP, Side::BOTTOM] {
        header_box.set_content_margin(side, 6.0);
    }
    header.add_theme_stylebox_override("panel", &header_box);
    column.add_child(&header);
    let mut box_ = factory::hbox(0);
    box_.set_mouse_filter(MouseFilter::IGNORE);
    header.add_child(&box_);
    let mut hbox: Gd<Container> = box_.clone().upcast();
    add_any(&hbox, &outline_label(&header_text(state, row), color));
    add_any(&hbox, &gap(8.0));
    let mut spacer = Control::new_alloc();
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    hbox.add_child(&spacer);
    add_any(&hbox, &outline_label(&index.to_string(), color));
    add_any(&hbox, &gap(8.0));
    // "taken from foo's client": `showAddDialog(index + 1)`.
    let mut add = logici_button("add");
    add.set_disabled(full);
    add.set_tooltip_text(&gs(bundle("@add")));
    on_pressed(&add, &dialog_gd, state, move |d, s| {
        open_add_dialog(d, s, index + 1);
    });
    hbox.add_child(&add);
    add_any(&hbox, &gap(6.0));
    let mut copy = logici_button("copy");
    copy.set_disabled(full);
    copy.set_tooltip_text(&gs(bundle("@copy")));
    on_pressed(&copy, &dialog_gd, state, move |d, s| {
        copy_statement(d, s, index);
    });
    hbox.add_child(&copy);
    add_any(&hbox, &gap(6.0));
    let mut delete = logici_button("cancel");
    delete.set_tooltip_text(&gs(bundle("@delete")));
    on_pressed(&delete, &dialog_gd, state, move |d, s| {
        delete_statement(d, s, index);
    });
    hbox.add_child(&delete);
    // `StatementElem`'s header drag: left-drag reorders, middle-click copies.
    let header_node: Gd<Node> = header.clone().upcast();
    {
        let target = dialog_gd.clone();
        let state_for_input = state.clone();
        let header_for_input = header.clone();
        let callable = Callable::from_fn("logic_header_input", move |args: &[&Variant]| {
            let Some(event) = args
                .first()
                .and_then(|value| value.try_to::<Gd<InputEvent>>().ok())
            else {
                return Variant::nil();
            };
            if !target.is_instance_valid() {
                return Variant::nil();
            }
            let mut handle = target.clone();
            let mut bound = handle.bind_mut();
            header_input(
                &mut bound,
                &state_for_input,
                event,
                index,
                &header_for_input,
            );
            Variant::nil()
        });
        let mut header_node = header_node;
        header_node.connect("gui_input", &callable);
    }

    // `add(t).pad(4).padTop(2).left().grow()` with `marginLeft(4)` (+
    // `marginRight(4)` when wrapping); `marginBottom(7)` sits on the frame.
    let mut body_margin = MarginContainer::new_alloc();
    body_margin.set_mouse_filter(MouseFilter::PASS);
    let wraps = dict_bool(&meta(state, &statement_name), "use_wrapping", true);
    body_margin.add_theme_constant_override("margin_left", 8);
    body_margin.add_theme_constant_override("margin_right", if wraps { 8 } else { 4 });
    body_margin.add_theme_constant_override("margin_top", 2);
    body_margin.add_theme_constant_override("margin_bottom", 4);
    column.add_child(&body_margin);
    let fields = field_row(dialog, state, index, row, color, wraps);
    body_margin.add_child(&fields);
    elem.upcast()
}
