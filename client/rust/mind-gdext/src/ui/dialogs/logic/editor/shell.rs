// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `shell` support code.
//! Upstream: logic/LogicDialog.java

use super::*;

// ---------------------------------------------------------------------------
// shell (`LogicDialog.setup` / `_build`)
// ---------------------------------------------------------------------------

/// `_build`: the clipped canvas holder (scroll + centred statement column +
/// jump/drop overlay) and the 160x64 `@back`/`@edit`/`@variables`/`@add` row.
pub(super) fn build(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    if let Some(mut content) = dialog.content() {
        content.set_v_size_flags(SizeFlags::EXPAND_FILL);
    }
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
    }
    let dialog_gd = dialog.to_gd();
    // code-instantiated: the canvas holder clips the jump overlay to the pane
    // (`JumpCurve` draws inside `canvas.pane`).
    let mut holder = Control::new_alloc();
    holder.set_clip_contents(true);
    holder.set_h_size_flags(SizeFlags::EXPAND_FILL);
    holder.set_v_size_flags(SizeFlags::EXPAND_FILL);
    holder.set_custom_minimum_size(Vector2::new(0.0, 320.0));
    if let Some(mut content) = dialog.content() {
        content.add_child(&holder);
    } else {
        dialog.base_mut().add_child(&holder);
    }
    // code-instantiated: the canvas pane owns the runtime-sized statement list.
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    holder.add_child(&scroll);
    // code-instantiated: `t.center()` inside the pane centres the column.
    let mut center = CenterContainer::new_alloc();
    center.set_h_size_flags(SizeFlags::EXPAND_FILL);
    center.set_v_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&center);
    // code-instantiated: one element per statement; the count is the program
    // length (`DragLayout`, `space = 10`).
    let mut list = factory::vbox(ELEMENT_SPACE);
    list.set_custom_minimum_size(Vector2::new(target_width(dialog), 0.0));
    center.add_child(&list);
    {
        let mut st = state.borrow_mut();
        st.list = Some(list);
        st.scroll = Some(scroll);
    }
    // code-instantiated: the jump-arrow and drop-line overlay sits above the
    // statement list (`LCanvas.DragLayout.draw` + `JumpCurve`); it never takes
    // mouse events and redraws every frame.
    let mut overlay = MindLogicOverlay::new_alloc();
    {
        let mut bound = overlay.bind_mut();
        bound.state = state.clone();
        bound.dialog = Some(dialog_gd.clone());
    }
    let mut overlay_control: Gd<Control> = overlay.upcast();
    overlay_control.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    overlay_control.set_mouse_filter(MouseFilter::IGNORE);
    holder.add_child(&overlay_control);
    state.borrow_mut().overlay = Some(overlay_control);

    // `LogicDialog.setup`: `buttons.defaults().size(160f, 64f)`.
    bottom_button(dialog, state, "left", "@back", |d, _s| d.close());
    bottom_button(dialog, state, "edit", "@edit", |d, s| {
        open_edit_menu(d, s);
    });
    let variables = bottom_button(dialog, state, "menu", "@variables", |d, s| {
        open_variables_panel(d, s);
    });
    let add = bottom_button(dialog, state, "add", "@add", |d, s| {
        open_add_dialog(d, s, -1);
    });
    {
        let mut st = state.borrow_mut();
        st.variables_button = Some(variables);
        st.add_button = Some(add);
    }
}

/// One `buttons.button(text, icon, listener)` cell at 160x64.
pub(super) fn bottom_button<F>(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<LogicState>>,
    icon: &str,
    key: &str,
    action: F,
) -> Gd<Button>
where
    F: Fn(&mut MindDialog, &Rc<RefCell<LogicState>>) + 'static,
{
    // code-instantiated: the editor's button row is built by `setup()`.
    let mut button = factory::icon_button(icon, &bundle(key), "defaultt", AlignmentMode::BEGIN);
    button.set_custom_minimum_size(Vector2::new(160.0, 64.0));
    let dialog_gd = dialog.to_gd();
    on_pressed(&button, &dialog_gd, state, action);
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&button);
    }
    button
}

/// `LogicDialog.onResize`: the column follows `getTargetWidth`.
pub(super) fn on_resize(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let width = target_width(dialog);
    let (list, rows) = {
        let st = state.borrow();
        (st.list.clone(), st.row_nodes.clone())
    };
    if let Some(mut list) = list
        && list.is_instance_valid()
    {
        let mut min = list.get_custom_minimum_size();
        min.x = width;
        list.set_custom_minimum_size(min);
    }
    for mut row in rows {
        if !row.is_instance_valid() {
            continue;
        }
        let mut min = row.get_custom_minimum_size();
        min.x = width;
        row.set_custom_minimum_size(min);
    }
}

/// Connects the viewport's `size_changed` -> [`on_resize`] once per node.
pub(super) fn connect_resize(dialog: &mut MindDialog, state: &Rc<RefCell<LogicState>>) {
    let dialog_gd = dialog.to_gd();
    if dialog_gd.has_meta(RESIZE_META) {
        return;
    }
    let Some(mut viewport) = dialog.base().get_viewport() else {
        return;
    };
    let target = dialog_gd.clone();
    let state_for_resize = state.clone();
    let callable = Callable::from_fn("logic_resize", move |_args: &[&Variant]| {
        if !target.is_instance_valid() {
            return Variant::nil();
        }
        let mut handle = target.clone();
        let mut bound = handle.bind_mut();
        on_resize(&mut bound, &state_for_resize);
        Variant::nil()
    });
    viewport.connect("size_changed", &callable);
    let mut node = dialog_gd;
    node.set_meta(RESIZE_META, &true.to_variant());
}
