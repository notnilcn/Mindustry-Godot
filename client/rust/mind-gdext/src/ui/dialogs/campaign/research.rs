// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The research dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/ResearchDialog.java

use super::util::*;
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::widgets::research_items_display::MindResearchItemsDisplay;
use crate::ui::kit::widgets::research_view::MindResearchView;
use crate::ui::kit::{self, MindLabel, styles};
use godot::builtin::{Side, VarDictionary, VariantType};
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, CanvasItem, Control, HBoxContainer, PanelContainer, StyleBox, StyleBoxEmpty,
    StyleBoxTexture, TextureRect, VBoxContainer,
};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "research" => Some(Box::new(ResearchDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// ResearchDialog
// ---------------------------------------------------------------------------

/// `iconMed` (`Scl.scl(32f)`): title icon, root rows and the balancing spacer.
const RESEARCH_ICON_SIZE: f32 = 32.0;

/// Controller state for the reused research dialog node.
#[derive(Default)]
struct ResearchState {
    view: Option<Gd<MindResearchView>>,
    title_button: Option<Gd<Button>>,
    title_icon_holder: Option<Gd<Control>>,
    title_label: Option<Gd<MindLabel>>,
    items_display: Option<Gd<MindResearchItemsDisplay>>,
    popup: Option<Gd<PanelContainer>>,
    hovered: i64,
    tree: VarDictionary,
    last_root: String,
    deferred: Deferred,
}

impl ResearchState {
    fn new() -> Self {
        Self {
            hovered: -1,
            ..Self::default()
        }
    }
}

/// The Java graph view over the `research_tree_json` read model.
pub struct ResearchDialog {
    state: Rc<RefCell<ResearchState>>,
}

impl Default for ResearchDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(ResearchState::new())),
        }
    }
}

impl DialogController for ResearchDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        research_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        research_shown(dialog, &self.state);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        research_hidden(dialog);
    }
}

fn research_ready(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    // Java: `super("")` + `titleTable.remove()`.
    dialog.title_text = GString::new();
    dialog.should_pause = true;
    dialog.refresh_title();
    // Java `BaseDialog`: `setFillParent(true)` + `margin(0f).marginBottom(8)`.
    dialog.fill_panel();
    // The shared panel's `defaultDialog` style would paint an opaque slate over
    // the dimmed planet page, so clear it and keep the 15 px bottom inset
    // (margin + the button cells' pad(3)).
    if let Some(panel_node) = dialog.base().get_node_or_null("Center/Panel")
        && let Ok(mut panel) = panel_node.try_cast::<Control>()
    {
        let mut empty = StyleBoxEmpty::new_gd();
        empty.set_content_margin(Side::BOTTOM, 15.0);
        panel.add_theme_stylebox_override("panel", &empty);
    }
    research_build(dialog, state);
    dialog.add_close_button(210.0);
    // Java: hide research, show the database (`Icon.book`).
    let target = dialog.to_gd();
    let callable = Callable::from_fn("research_database", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        research_open_database(&mut bound);
    });
    let text = kit::t("@database").to_string();
    dialog.add_button(&text, Some(callable), "book", 210.0);
    research_install_tick(dialog, state);
}

fn research_build(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    let Some(mut layout) = dialog
        .base()
        .get_node_or_null("Center/Panel/Layout")
        .and_then(|node| node.try_cast::<VBoxContainer>().ok())
    else {
        return;
    };
    // code-instantiated: the tree canvas replaces the generic scroll body
    // (Java `cont.stack(titleTable, view, itemDisplay)`).
    let mut view = MindResearchView::new_alloc();
    view.set_h_size_flags(SizeFlags::EXPAND_FILL);
    view.set_v_size_flags(SizeFlags::EXPAND_FILL);
    let view_control: Gd<Control> = view.clone().upcast();
    layout.add_child(&view_control);
    let mut view_node: Gd<Node> = view.clone().upcast();
    {
        let target = dialog.to_gd();
        let state_for_press = state.clone();
        let callable = Callable::from_fn("research_node_pressed", move |args: &[&Variant]| {
            let index = args.first().map(|value| (*value).to::<i64>()).unwrap_or(-1);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_on_node_pressed(&mut bound, &state_for_press, index);
        });
        view_node.connect("node_pressed", &callable);
    }
    {
        let target = dialog.to_gd();
        let state_for_hover = state.clone();
        let callable = Callable::from_fn("research_node_hovered", move |args: &[&Variant]| {
            let index = args.first().map(|value| (*value).to::<i64>()).unwrap_or(-1);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_on_node_hovered(&mut bound, &state_for_hover, index);
        });
        view_node.connect("node_hovered", &callable);
    }
    {
        let target = dialog.to_gd();
        let state_for_exit = state.clone();
        let callable = Callable::from_fn("research_node_exited", move |args: &[&Variant]| {
            let index = args.first().map(|value| (*value).to::<i64>()).unwrap_or(-1);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_on_node_exited(&mut bound, &state_for_exit, index);
        });
        view_node.connect("node_exited", &callable);
    }
    // code-instantiated: root-selector title button (Java `titleTable`, a stack
    // layer over the canvas; `titleTable.toFront()` keeps it on top).
    let mut title_button = Button::new_alloc();
    title_button.set_theme_type_variation("defaultt");
    title_button.set_visible(false);
    {
        let target = dialog.to_gd();
        let state_for_selector = state.clone();
        let callable = Callable::from_fn("research_root_selector", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_open_root_selector(&mut bound, &state_for_selector);
        });
        title_button.connect("pressed", &callable);
    }
    view.add_child(&title_button);
    title_button.set_anchors_preset(LayoutPreset::CENTER_TOP);
    title_button.set_offset(Side::LEFT, -150.0);
    title_button.set_offset(Side::RIGHT, 150.0);
    title_button.set_offset(Side::TOP, 4.0);
    title_button.set_offset(Side::BOTTOM, 64.0);
    research_build_title_row(dialog, state, &mut title_button);
    // code-instantiated: the global item pool overlays the canvas top-left.
    let mut items = MindResearchItemsDisplay::new_alloc();
    // The stack sits in `cont` under the Dialog's and cont's `pad(3)` cells:
    // the items box is at (6, 6).
    items.set_position(Vector2::new(6.0, 6.0));
    view.add_child(&items);
    if let Some(mut body) = dialog.body() {
        // The canvas is not a scroll body; keep it out of the ScrollContainer
        // (it must receive drags) and above the hidden Body in the VBox.
        let body_slot = body.get_index();
        layout.move_child(&view_node, body_slot);
        body.set_visible(false);
    }
    {
        let mut st = state.borrow_mut();
        st.view = Some(view);
        st.title_button = Some(title_button);
        st.items_display = Some(items);
    }
}

/// `titleTable.button` content: 32 px root icon, `padRight(8)`, a grow spacer,
/// the accent label, another grow spacer and a 32 px spacer so the label sits
/// centred; built once and retargeted per tree.
fn research_build_title_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    button: &mut Gd<Button>,
) {
    let _ = dialog;
    // code-instantiated: title row content is tree-driven.
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    row.add_theme_constant_override("separation", 0);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);
    {
        // The button's `Tex.button` background pads its cells (Arc
        // `Table.setBackground`), so the icon sits inside the border.
        let mut button_control: Gd<Control> = button.clone().upcast();
        let mut row_control: Gd<Control> = row.clone().upcast();
        factory::inset_to_content(&mut button_control, &mut row_control);
    }
    let button_for_theme = button.clone();
    let row_for_theme = row.clone();
    let callable = Callable::from_fn("research_title_inset", move |_args: &[&Variant]| {
        let mut button_control: Gd<Control> = button_for_theme.clone().upcast();
        let mut row_control: Gd<Control> = row_for_theme.clone().upcast();
        factory::inset_to_content(&mut button_control, &mut row_control);
    });
    button.connect("theme_changed", &callable);
    // code-instantiated: root icon follows the tree data (atlas region or the
    // `terrain` glyph for a sector root).
    let mut holder = Control::new_alloc();
    holder.set_custom_minimum_size(Vector2::new(RESEARCH_ICON_SIZE, RESEARCH_ICON_SIZE));
    holder.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    holder.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&holder);
    row.add_child(&research_title_spacer(8.0));
    row.add_child(&research_title_spacer(-1.0));
    let label = factory::label("");
    rich(&label).add_theme_color_override("default_color", styles::ACCENT);
    rich(&label).set_v_size_flags(SizeFlags::SHRINK_CENTER);
    rich(&label).set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&label);
    row.add_child(&research_title_spacer(-1.0));
    row.add_child(&research_title_spacer(RESEARCH_ICON_SIZE));
    let mut st = state.borrow_mut();
    st.title_icon_holder = Some(holder);
    st.title_label = Some(label);
}

/// Spacer cell for the title button row (`b.add().growX()` = -1).
fn research_title_spacer(size: f32) -> Gd<Control> {
    // code-instantiated: title row is data-driven (Java `b.add()` cells).
    let mut spacer = Control::new_alloc();
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    if size < 0.0 {
        spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    } else {
        spacer.set_custom_minimum_size(Vector2::new(size, 0.0));
    }
    spacer
}

/// Retargets the title button on the displayed tree.
fn research_update_title_button(state: &Rc<RefCell<ResearchState>>) {
    let st = state.borrow();
    let (Some(mut holder), Some(label)) = (st.title_icon_holder.clone(), st.title_label.clone())
    else {
        return;
    };
    for child in holder.get_children().iter_shared() {
        let mut child = child;
        child.queue_free();
    }
    let icon_region = row_str(&st.tree, "root_icon");
    let type_name = row_str(&st.tree, "root_type");
    if let Some(mut icon) = factory::content_icon(&icon_region, &type_name, RESEARCH_ICON_SIZE) {
        icon.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        icon.set_mouse_filter(MouseFilter::IGNORE);
        holder.add_child(&icon);
    }
    rich(&label).set_text(&gs(row_str(&st.tree, "root_title")));
}

/// Java `titleTable.button` click: the `@techtree.select` picker.
fn research_open_root_selector(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    let (roots, selected) = {
        let st = state.borrow();
        (row_arr(&st.tree, "roots"), row_str(&st.tree, "root"))
    };
    if roots.len() <= 1 {
        return;
    }
    let Some(selector) = dialog_node("research_root_select") else {
        return;
    };
    if selector.has_signal("root_selected") {
        let target = dialog.to_gd();
        let state_for_pick = state.clone();
        let callable = Callable::from_fn("research_select_root", move |args: &[&Variant]| {
            let name = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            research_select_root(&mut bound, &state_for_pick, &name);
        });
        connect_once(
            dialog,
            "research_root_selected",
            &selector,
            "root_selected",
            &callable,
        );
    }
    let mut context = VarDictionary::new();
    context.set("roots", &roots.to_variant());
    context.set("selected", &gs(selected));
    open_dialog("research_root_select", &context);
}

/// `rebuildTree(node)`: switch tree, reset the view.
fn research_select_root(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    root_name: &str,
) {
    if root_name.is_empty() {
        return;
    }
    let (last_root, view) = {
        let st = state.borrow();
        (st.last_root.clone(), st.view.clone())
    };
    if root_name == last_root {
        research_update_title_button(state);
        return;
    }
    close_dialog("research_root_select");
    let tree = dialog.campaign_json("research_tree_json", &[gs(root_name).to_variant()]);
    if tree.is_empty() {
        return;
    }
    let Some(mut view) = view else {
        return;
    };
    view.bind_mut().set_tree(tree.clone());
    view.bind_mut().reset_view();
    {
        let mut st = state.borrow_mut();
        st.last_root = row_opt_str(&tree, "root").unwrap_or_else(|| root_name.to_string());
        st.tree = tree;
    }
    research_update_title_button(state);
}

/// `ResearchDialog.java:463-487`: `if(canSpend && locked) spend`.
fn research_on_node_pressed(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    index: i64,
) {
    let view = {
        let st = state.borrow();
        st.view.clone()
    };
    let Some(mut view) = view else {
        return;
    };
    let node = view.bind().node_at(index);
    if node.is_empty() {
        return;
    }
    let content = row_str(&node, "content");
    if content.is_empty() {
        return;
    }
    if !row_bool(&node, "locked") || !row_bool(&node, "can_spend") {
        return;
    }
    let Some(mut campaign) = dialog.campaign_node() else {
        return;
    };
    if !campaign.has_method("research") {
        return;
    }
    if !campaign
        .call("research", &[gs(content.as_str()).to_variant()])
        .try_to::<bool>()
        .unwrap_or(false)
    {
        return;
    }
    // Requirement amounts that changed flash (`ResearchDialog.spend` shine).
    let mut previous_have = VarDictionary::new();
    for req_variant in row_arr(&node, "reqs").iter_shared() {
        let Ok(req) = req_variant.try_to::<VarDictionary>() else {
            continue;
        };
        previous_have.set(
            &gs(row_str(&req, "item")),
            &gs(row_text(&req, "have")).to_variant(),
        );
    }
    let was_locked = row_bool_or(&node, "locked", true);
    let last_root = {
        let st = state.borrow();
        st.last_root.clone()
    };
    // Lock state and layout positions change after a spend (Java `treeLayout`
    // runs from `unlock`); re-fetch the same tree.
    let tree = dialog.campaign_json("research_tree_json", &[gs(last_root).to_variant()]);
    if tree.is_empty() {
        return;
    }
    let mut shine = VarDictionary::new();
    let mut unlocked_now = false;
    for node_variant in row_arr(&tree, "nodes").iter_shared() {
        let Ok(refreshed) = node_variant.try_to::<VarDictionary>() else {
            continue;
        };
        if row_str(&refreshed, "content") != content {
            continue;
        }
        unlocked_now = !row_bool_or(&refreshed, "locked", true);
        for req_variant in row_arr(&refreshed, "reqs").iter_shared() {
            let Ok(req) = req_variant.try_to::<VarDictionary>() else {
                continue;
            };
            let item_name = row_str(&req, "item");
            let before = previous_have
                .get(&gs(item_name.as_str()))
                .map(|value| text_of(&value))
                .unwrap_or_default();
            if previous_have.contains_key(&gs(item_name.as_str()))
                && before != row_text(&req, "have")
            {
                shine.set(&gs(item_name), &true.to_variant());
            }
        }
        break;
    }
    view.bind_mut().set_tree(tree.clone());
    research_raise_overlays(state);
    {
        let st = state.borrow();
        if let Some(mut items) = st.items_display.clone() {
            items
                .bind_mut()
                .rebuild(row_arr(&tree, "items"), shine.clone());
        }
    }
    {
        let mut st = state.borrow_mut();
        st.tree = tree;
    }
    research_update_title_button(state);
    let hovered = {
        let st = state.borrow();
        st.hovered
    };
    if hovered >= 0 {
        research_rebuild_popup(dialog, state, &shine);
    }
    if was_locked && unlocked_now {
        // Java `unlock`: `Sounds.uiUnlock.play()`.
        play_ui_sound("uiUnlock");
    }
}

/// `ResearchDialog.java:490-501`: hover shows the info table for visible nodes.
fn research_on_node_hovered(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    index: i64,
) {
    let view = {
        let st = state.borrow();
        st.view.clone()
    };
    let Some(view) = view else {
        return;
    };
    let node = view.bind().node_at(index);
    if node.is_empty() || !row_bool_or(&node, "visible", true) {
        return;
    }
    state.borrow_mut().hovered = index;
    research_rebuild_popup(dialog, state, &VarDictionary::new());
}

fn research_on_node_exited(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    index: i64,
) {
    let hovered = {
        let st = state.borrow();
        st.hovered
    };
    if hovered != index {
        return;
    }
    let deferred = {
        let st = state.borrow();
        st.deferred.clone()
    };
    let target = dialog.to_gd();
    let state_for_clear = state.clone();
    deferred.push(move || {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        research_maybe_clear_popup(&mut bound, &state_for_clear);
    });
}

/// Keep the popup while the mouse is over it or the node button; else remove.
fn research_maybe_clear_popup(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    let hovered = {
        let st = state.borrow();
        st.hovered
    };
    if hovered < 0 {
        return;
    }
    let mouse = dialog
        .base()
        .get_viewport()
        .map(|viewport| viewport.get_mouse_position())
        .unwrap_or(Vector2::ZERO);
    let popup = {
        let st = state.borrow();
        st.popup.clone()
    };
    if let Some(popup) = popup {
        let popup_control: Gd<Control> = popup.upcast();
        if popup_control.get_global_rect().contains_point(mouse) {
            return;
        }
    }
    let view = {
        let st = state.borrow();
        st.view.clone()
    };
    if let Some(view) = view
        && view.bind().button_rect(hovered).contains_point(mouse)
    {
        return;
    }
    research_clear_popup(state);
}

fn research_clear_popup(state: &Rc<RefCell<ResearchState>>) {
    state.borrow_mut().hovered = -1;
    research_free_popup(state);
}

fn research_free_popup(state: &Rc<RefCell<ResearchState>>) {
    let popup = {
        let mut st = state.borrow_mut();
        st.popup.take()
    };
    if let Some(popup) = popup {
        let mut node: Gd<Node> = popup.upcast();
        node.queue_free();
    }
}

/// `View.rebuild` (`ResearchDialog.java:632-784`): the hover info table.
fn research_rebuild_popup(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ResearchState>>,
    shine: &VarDictionary,
) {
    research_free_popup(state);
    let (hovered, view) = {
        let st = state.borrow();
        (st.hovered, st.view.clone())
    };
    if hovered < 0 {
        return;
    }
    let Some(view) = view else {
        return;
    };
    let node = view.bind().node_at(hovered);
    if node.is_empty() {
        return;
    }
    let selectable = row_bool(&node, "selectable");
    let locked = row_bool_or(&node, "locked", true);
    // code-instantiated: hover info table (Java `infoTable`).
    let mut popup = PanelContainer::new_alloc();
    let mut box_ = StyleBoxTexture::new_gd();
    if let Some(texture) = kit::icon_texture("button") {
        box_.set_texture(&texture);
    }
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        box_.set_texture_margin(side, 8.0);
    }
    let box_gd: Gd<StyleBox> = box_.upcast();
    popup.add_theme_stylebox_override("panel", &box_gd);
    popup.set_mouse_filter(MouseFilter::STOP);
    {
        let target = dialog.to_gd();
        let state_for_clear = state.clone();
        let callable = Callable::from_fn("research_popup_exited", move |_args: &[&Variant]| {
            let deferred = {
                let st = state_for_clear.borrow();
                st.deferred.clone()
            };
            let target_inner = target.clone();
            let state_inner = state_for_clear.clone();
            deferred.push(move || {
                let mut node = target_inner.clone();
                let mut bound = node.bind_mut();
                research_maybe_clear_popup(&mut bound, &state_inner);
            });
        });
        popup.connect("mouse_exited", &callable);
    }
    let mut view_node: Gd<Node> = view.upcast();
    view_node.add_child(&popup);
    state.borrow_mut().popup = Some(popup.clone());

    // code-instantiated: info row + description column (Java `infoTable.table`).
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 4);
    popup.add_child(&column);
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 6);
    column.add_child(&row);
    if selectable {
        // code-instantiated: `Icon.info` button (`ui.content.show`).
        let mut info = factory::glyph_button("info", "i", "flati");
        info.set_custom_minimum_size(Vector2::new(50.0, 0.0));
        let content = row_str(&node, "content");
        let callable = Callable::from_fn("research_info", move |_args: &[&Variant]| {
            crate::ui::dialogs::batch_a::open_content_info(&content);
        });
        info.connect("pressed", &callable);
        row.add_child(&info);
    }
    // code-instantiated: description column (`rebuild(shine)` desc table).
    let mut desc = VBoxContainer::new_alloc();
    desc.add_theme_constant_override("separation", 2);
    row.add_child(&desc);
    let name_text = if selectable {
        row_str(&node, "name")
    } else {
        String::from("???")
    };
    let name_label = factory::label(&name_text);
    if !selectable {
        rich(&name_label).add_theme_color_override("default_color", styles::ACCENT);
    }
    desc.add_child(&name_label);
    if locked {
        if selectable {
            research_build_requirement_rows(&mut desc, &node, shine);
        } else if !row_arr(&node, "objectives").is_empty() {
            // code-instantiated: incomplete objective rows (`@complete` block).
            let complete = factory::label(&kit::tm("@complete").to_string());
            desc.add_child(&complete);
            for objective in row_arr(&node, "objectives").iter_shared() {
                let mut objective_row = HBoxContainer::new_alloc();
                objective_row.add_theme_constant_override("separation", 3);
                let objective_label = factory::label(&format!("> {}", text_of(&objective)));
                rich(&objective_label).add_theme_color_override("default_color", TEXT_GRAY);
                objective_row.add_child(&objective_label);
                let glyph = glyph_control("cancel", 18.0, PAL_REMOVE);
                objective_row.add_child(&glyph);
                desc.add_child(&objective_row);
            }
        }
    } else {
        let completed = factory::label(&kit::tm("@completed").to_string());
        desc.add_child(&completed);
    }
    // `rebuild`: description wrap 270 px when longer than 20 chars.
    let description = row_str(&node, "description");
    if selectable && row_bool_or(&node, "inline_description", false) && !description.is_empty() {
        let description_label = factory::label(&description);
        let mut description_rich = rich(&description_label);
        description_rich.add_theme_color_override("default_color", TEXT_GRAY);
        if description.chars().count() > 20 {
            let mut minimum = description_rich.get_custom_minimum_size();
            minimum.x = 270.0;
            description_rich.set_custom_minimum_size(minimum);
            description_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
        }
        column.add_child(&description_label);
    }
    research_update_popup_position(state);
}

/// `ResearchDialog.java:682-736`: progress line + one row per unfinished stack.
fn research_build_requirement_rows(
    desc: &mut Gd<VBoxContainer>,
    node: &VarDictionary,
    shine: &VarDictionary,
) {
    let progress = row_i64_or(node, "progress", -1);
    if progress >= 0 {
        let text = kit::tm_format("@research.progress", &[progress.to_string().as_str()]);
        let progress_label = factory::label(&text.to_string());
        rich(&progress_label).add_theme_color_override("default_color", TEXT_GRAY);
        desc.add_child(&progress_label);
    }
    for req_variant in row_arr(node, "reqs").iter_shared() {
        let Ok(req) = req_variant.try_to::<VarDictionary>() else {
            continue;
        };
        let item_name = row_str(&req, "item");
        let has = row_bool_or(&req, "has", false);
        let target = if has { TEXT_GRAY } else { PAL_REMOVE };
        // code-instantiated: one requirement row (`list.image` + labels).
        let mut req_row = HBoxContainer::new_alloc();
        req_row.add_theme_constant_override("separation", 3);
        let mut icon = TextureRect::new_alloc();
        if let Some(texture) = kit::icon_texture(&format!("item-{item_name}-ui")) {
            icon.set_texture(&texture);
        }
        icon.set_custom_minimum_size(Vector2::new(24.0, 24.0));
        icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        req_row.add_child(&icon);
        let item_label_text = row_opt_str(&req, "name").unwrap_or_else(|| item_name.clone());
        let item_label = factory::label(&item_label_text);
        rich(&item_label).add_theme_color_override("default_color", TEXT_GRAY);
        req_row.add_child(&item_label);
        let have = row_opt_str(&req, "have").unwrap_or_else(|| String::from("0"));
        let need = row_opt_str(&req, "need").unwrap_or_else(|| String::from("0"));
        let amount_label = factory::label(&format!(" {have} / {need}"));
        rich(&amount_label).add_theme_color_override("default_color", target);
        if shine.contains_key(item_name.as_str()) {
            // `Actions.color(Pal.accent -> targetColor, 0.75s)`.
            let mut amount_canvas: Gd<CanvasItem> = amount_label.clone().upcast();
            amount_canvas.set_modulate(styles::ACCENT);
            let mut tween = amount_canvas.create_tween();
            tween.tween_property(&amount_canvas, "modulate", &Color::WHITE.to_variant(), 0.75);
        }
        req_row.add_child(&amount_label);
        desc.add_child(&req_row);
    }
}

fn research_update_popup_position(state: &Rc<RefCell<ResearchState>>) {
    let (view, popup, hovered) = {
        let st = state.borrow();
        (st.view.clone(), st.popup.clone(), st.hovered)
    };
    let (Some(view), Some(mut popup)) = (view, popup) else {
        return;
    };
    if hovered < 0 {
        return;
    }
    // Java places it at the button's top-right (`Align.topLeft`, y-up): Godot
    // top-right corner = (right, top). Controls are unrotated/unscaled here, so
    // view-local = global - view.global_position.
    let rect = view.bind().button_rect(hovered);
    let corner = Vector2::new(rect.position.x + rect.size.x, rect.position.y);
    let view_control: Gd<Control> = view.upcast();
    popup.set_position(corner - view_control.get_global_position());
}

/// The `@globalitems` pool and the title button draw above the node buttons
/// (`view.move_child(_items_display, -1)` / `titleTable.toFront()`).
fn research_raise_overlays(state: &Rc<RefCell<ResearchState>>) {
    let st = state.borrow();
    let Some(view) = st.view.clone() else {
        return;
    };
    let mut view_node: Gd<Node> = view.upcast();
    if let Some(items) = st.items_display.clone() {
        let items_node: Gd<Node> = items.upcast();
        view_node.move_child(&items_node, -1);
    }
    if let Some(button) = st.title_button.clone() {
        let button_node: Gd<Node> = button.upcast();
        view_node.move_child(&button_node, -1);
    }
}

fn research_install_tick(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    let deferred = {
        let st = state.borrow();
        st.deferred.clone()
    };
    let state_for_tick = state.clone();
    install_frame_tick(dialog, "research", deferred, move || {
        let has_popup = {
            let st = state_for_tick.borrow();
            st.popup.is_some()
        };
        if !has_popup {
            return;
        }
        research_update_popup_position(&state_for_tick);
    });
}

fn research_shown(dialog: &mut MindDialog, state: &Rc<RefCell<ResearchState>>) {
    // Java `shown`: the planet page's planet, else the played sector's planet.
    let planet = dialog.context_string("planet").to_string();
    let tree = dialog.campaign_json("research_tree_json", &[gs(planet).to_variant()]);
    if tree.is_empty() {
        return;
    }
    let view = {
        let st = state.borrow();
        st.view.clone()
    };
    let Some(mut view) = view else {
        return;
    };
    view.bind_mut().set_tree(tree.clone());
    research_raise_overlays(state);
    let view_control: Gd<Control> = view.clone().upcast();
    let view_height = view_control.get_size().y;
    {
        let st = state.borrow();
        if let Some(mut items) = st.items_display.clone() {
            items.bind_mut().max_height = 60.0f32.max(view_height - 120.0);
            items.bind_mut().rebuild_items(row_arr(&tree, "items"));
        }
    }
    let root_name = row_str(&tree, "root");
    let last_root = {
        let st = state.borrow();
        st.last_root.clone()
    };
    if root_name != last_root {
        // Java keeps pan between opens of the same tree, resets on a switch.
        view.bind_mut().reset_view();
        state.borrow_mut().last_root = root_name;
    }
    {
        let mut st = state.borrow_mut();
        st.tree = tree.clone();
    }
    research_update_title_button(state);
    // Java evaluates the button's `visible` lambda against `TechTree.roots`
    // (`ResearchDialog.java:108`), not "a tree is loaded": with one selectable
    // tree the button is dead.
    let selected_locked = row_rows(&tree, "nodes")
        .first()
        .is_some_and(|root| row_bool(root, "locked"));
    let visible = root_select_visible(row_rows(&tree, "roots").len(), selected_locked);
    if let Some(mut button) = state.borrow().title_button.clone() {
        button.set_visible(visible);
    }
}

/// Java `showTechSelect` (`ResearchDialog.java:108`):
/// `TechTree.roots.count(node -> !(node.requiresUnlock && !node.content.unlockedHost())) > 1`.
///
/// `roots` (the `@techtree.select` picker rows) keeps the *preferred* root even
/// when it needs unlocking (`node != getPrefRoot()` in Java's loop) while the
/// visibility count does not, so a locked selected root is subtracted. The
/// tree's own root row supplies that lock state (`unlockedHost()` as
/// `nodes[0].locked`; the root rows use `content_unlocked`, the same predicate
/// in single player).
fn root_select_visible(picker_rows: usize, selected_root_locked: bool) -> bool {
    picker_rows.saturating_sub(usize::from(selected_root_locked)) > 1
}

fn research_open_database(dialog: &mut MindDialog) {
    let _ = dialog;
    // Java: hide research, show the database.
    close_dialog("research");
    open_dialog("database", &VarDictionary::new());
}

fn research_hidden(dialog: &mut MindDialog) {
    let _ = dialog;
    // Java `hidden(ui.planet::setup)`: refresh the planet page underneath.
    if !ui_bool("is_dialog_shown", &[gs("planet").to_variant()]) {
        return;
    }
    let Some(mut planet) = dialog_node("planet") else {
        return;
    };
    if planet.has_method("refresh_page") {
        planet.call_deferred("refresh_page", &[]);
    }
}

/// `row.get(key, "")` coerced to a string (GDScript `str(...)`).
fn row_text(row: &VarDictionary, key: &str) -> String {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .map(|value| text_of(&value))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn root_selector_needs_two_selectable_trees() {
        assert!(!root_select_visible(0, false));
        // Fresh Serpulo profile: the only tree is the preferred root.
        assert!(!root_select_visible(1, false));
        // Erekir campaign before `core-bastion` unlocks: the picker keeps the
        // preferred root, Java's count does not.
        assert!(!root_select_visible(2, true));
        // Both trees selectable.
        assert!(root_select_visible(2, false));
        // The preferred root locked plus two unlocked trees still shows.
        assert!(root_select_visible(3, true));
    }
}
