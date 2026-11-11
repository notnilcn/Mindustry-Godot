// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The schematics tag/edit sub-dialog helpers. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/SchematicsDialog.java (tag/edit sub-dialogs)

use super::files::*;
use super::schematics_dialog::*;
use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::campaign::util::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindScroll, MindTable};
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_edit::LineWrappingMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, CenterContainer, ColorRect, Control, HBoxContainer, Label, PanelContainer, TextEdit,
    Texture2D, VBoxContainer,
};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

// ---------------------------------------------------------------------------
// SchematicsDialog: transient sub-dialogs
// ---------------------------------------------------------------------------

/// Opens a code-instantiated sub-dialog (`BaseDialog` in Java; the port's
/// dialogs are scene-rooted, so sub-dialogs are transient overlays).
pub(super) fn schematic_overlay_open(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    kind: SchematicOverlay,
    title: &str,
    width: f32,
) -> Gd<MindTable> {
    schematic_overlay_close(state);
    // code-instantiated: per-action sub-dialogs have no static scene.
    let mut root = ColorRect::new_alloc();
    // Deterministic handle for the headless sweep and the twin walk.
    root.set_name("schematic_overlay");
    root.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.8));
    root.set_anchors_preset(LayoutPreset::FULL_RECT);
    root.set_mouse_filter(MouseFilter::STOP);
    let mut center = CenterContainer::new_alloc();
    center.set_anchors_preset(LayoutPreset::FULL_RECT);
    center.set_mouse_filter(MouseFilter::IGNORE);
    let mut panel = PanelContainer::new_alloc();
    panel.set_theme_type_variation("defaultDialog");
    panel.set_custom_minimum_size(Vector2::new(width, 0.0));
    let mut layout = VBoxContainer::new_alloc();
    layout.add_theme_constant_override("separation", 6);
    let title_label = factory::styled_label(
        &kit::markup_text(&kit::t(title).to_string()).to_string(),
        "techLabel",
    );
    layout.add_child(&title_label);
    let mut body = factory::table();
    body.set_h_size_flags(SizeFlags::EXPAND_FILL);
    layout.add_child(&body);
    // `BaseDialog.addCloseButton`/`addCloseListener` on every sub-dialog.
    let mut close = factory::button(&kit::t("@back").to_string());
    close.set_custom_minimum_size(Vector2::new(200.0, 50.0));
    {
        let state_for_close = state.clone();
        let callable = Callable::from_fn("schematic_overlay_close", move |_args: &[&Variant]| {
            schematic_overlay_close(&state_for_close);
        });
        close.connect("pressed", &callable);
    }
    layout.add_child(&close);
    panel.add_child(&layout);
    center.add_child(&panel);
    root.add_child(&center);
    let root_control: Gd<Control> = root.upcast();
    dialog.base_mut().add_child(&root_control);
    {
        let mut st = state.borrow_mut();
        st.overlay_root = Some(root_control);
        st.overlay_body = Some(body.clone());
        st.overlay_kind = kind;
    }
    body
}

/// Frees the transient sub-dialog and clears its widget handles.
pub(super) fn schematic_overlay_close(state: &Rc<RefCell<SchematicState>>) {
    let root = {
        let mut st = state.borrow_mut();
        st.overlay_body = None;
        st.overlay_kind = SchematicOverlay::None;
        st.edit_name_field = None;
        st.edit_description_field = None;
        st.edit_tags_row = None;
        st.edit_pending_return = false;
        st.overlay_root.take()
    };
    if let Some(mut root) = root
        && root.is_instance_valid()
    {
        root.queue_free();
    }
}

/// Places a freshly built overlay's cells immediately (a new table is not
/// sorted until the next layout pass).
pub(super) fn schematic_overlay_finish(panel: &mut Gd<MindTable>) {
    panel.bind_mut().sort_now();
    if let Some(parent) = panel.get_parent()
        && let Ok(mut container) = parent.try_cast::<godot::classes::Container>()
    {
        container.queue_sort();
    }
}

/// A scrollable body for the overlay's tall lists (Java `cont.pane`).
fn schematic_overlay_scroll(width: f32, height: f32) -> (Gd<MindScroll>, Gd<MindTable>) {
    let mut pane = MindScroll::new_alloc();
    pane.bind_mut().set_scrolling_disabled(true, false);
    let mut control: Gd<Control> = pane.clone().upcast();
    control.set_custom_minimum_size(Vector2::new(width, height));
    control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let content = pane.bind_mut().content();
    (pane, content)
}

/// A tag's display control: icon tags render as an atlas texture (content
/// icons) or an `icon.ttf` glyph (the fontello `Icon` set); text tags as a
/// label with the tag's Arc color applied.
pub(super) fn schematic_tag_visual(tag: &str, extent: f32) -> Gd<Control> {
    if let Some(character) = schematic_icon_tag_char(tag)
        && let Some(control) = schematic_icon_visual(character, extent)
    {
        return control;
    }
    let label = factory::label(&schematics_tag_plain(tag));
    if let Some(tint) = schematics_tag_color(tag) {
        rich(&label).add_theme_color_override("default_color", tint);
    }
    let mut rich_label = rich(&label);
    rich_label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    rich_label.set_custom_minimum_size(Vector2::new(0.0, extent));
    label.upcast()
}

/// The visual for one PUA icon code: a content icon texture when
/// `icons.properties` has the code, else the `icon.ttf` glyph.
fn schematic_icon_visual(character: char, extent: f32) -> Option<Gd<Control>> {
    if let Some(assets) = kit::assets() {
        let name = assets
            .bind()
            .icon_name_for_code(character as i32)
            .to_string();
        if !name.is_empty() {
            let region = assets.bind().icon_texture(gs(&name)).to_string();
            if !region.is_empty()
                && let Some(texture) = kit::icon_texture(&region)
            {
                let mut icon = icon_rect(&texture, extent);
                icon.set_h_size_flags(SizeFlags::SHRINK_CENTER);
                return Some(icon.upcast());
            }
        }
    }
    let glyph = char::from_u32(character as u32)?;
    let mut label = Label::new_alloc();
    let mut buffer = [0u8; 4];
    label.set_text(&gs(glyph.encode_utf8(&mut buffer)));
    let mut control: Gd<Control> = label.clone().upcast();
    kit::icons::apply_icon_font(&mut control);
    label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
    label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    label.set_custom_minimum_size(Vector2::new(extent, extent));
    label.add_theme_font_size_override("font_size", extent as i32);
    Some(control)
}

/// The content icons of `icons.properties` (`Iconc`): name, char and atlas
/// texture. Used by the `@schematic.icontag` grid.
fn schematic_content_icons() -> Vec<(String, char, Option<Gd<Texture2D>>)> {
    let Some(assets) = kit::assets() else {
        return Vec::new();
    };
    let listed: Vec<(String, char, String)> = {
        let bound = assets.bind();
        let mut out = Vec::new();
        for name in bound.icon_names().as_slice() {
            let text = bound.unicode_str(name.clone()).to_string();
            let mut characters = text.chars();
            let Some(character) = characters.next() else {
                continue;
            };
            if characters.next().is_some() {
                continue;
            }
            out.push((
                name.to_string(),
                character,
                bound.icon_texture(name.clone()).to_string(),
            ));
        }
        out
    };
    listed
        .into_iter()
        .map(|(name, character, region)| {
            let texture = if region.is_empty() {
                None
            } else {
                kit::icon_texture(&region)
            };
            (name, character, texture)
        })
        .collect()
}

/// `showAllTags`: the global tag editor (reorder/rename/delete/count/select).
pub(super) fn schematics_show_all_tags(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
) {
    let mut body = schematic_overlay_open(
        dialog,
        state,
        SchematicOverlay::Tags,
        "@schematic.edittags",
        680.0,
    );
    let tags = state.borrow().tags.clone();
    let labels = schematic_label_lists(dialog);
    let (pane, mut list) = schematic_overlay_scroll(640.0, 360.0);
    for tag in &tags {
        // code-instantiated: one tag row per entry of `schematic-tags`.
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 3);
        // move up/down (`Icon.upOpen`/`Icon.downOpen`).
        let mut up = factory::glyph_button("upOpen", "@editor.moveup", "emptyi");
        up.set_custom_minimum_size(Vector2::new(38.0, TAG_HEIGHT));
        up.set_tooltip_text(&kit::t("@editor.moveup"));
        {
            let target = dialog.to_gd();
            let state_for_move = state.clone();
            let tag_name = tag.clone();
            let callable = Callable::from_fn("schematic_tag_up", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                schematics_move_tag(&mut bound, &state_for_move, &tag_name, -1);
            });
            up.connect("pressed", &callable);
        }
        row.add_child(&up);
        let mut down = factory::glyph_button("downOpen", "@editor.movedown", "emptyi");
        down.set_custom_minimum_size(Vector2::new(38.0, TAG_HEIGHT));
        down.set_tooltip_text(&kit::t("@editor.movedown"));
        {
            let target = dialog.to_gd();
            let state_for_move = state.clone();
            let tag_name = tag.clone();
            let callable = Callable::from_fn("schematic_tag_down", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                schematics_move_tag(&mut bound, &state_for_move, &tag_name, 1);
            });
            down.connect("pressed", &callable);
        }
        row.add_child(&down);
        row.add_child(&schematic_tag_visual(tag, 32.0));
        // `Core.bundle.format("schematic.tagged", count)`: clicking selects the
        // tag in the main filter (`selectedTags.clear().add(tag)`).
        let count = schematics_tag_count(&labels, tag);
        let count_text = kit::tm_format("@schematic.tagged", &[count.to_string().as_str()]);
        let mut count_button = factory::button(&count_text.to_string());
        count_button.set_theme_type_variation("flatt");
        {
            let target = dialog.to_gd();
            let state_for_select = state.clone();
            let tag_name = tag.clone();
            let callable =
                Callable::from_fn("schematic_tag_select", move |_args: &[&Variant]| {
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    schematics_select_tag(&mut bound, &state_for_select, &tag_name);
                });
            count_button.connect("pressed", &callable);
        }
        row.add_child(&count_button);
        row.add_child(&factory::space(4.0, 0.0));
        // rename tag (`Icon.pencil` -> `@schematic.renametag`).
        let mut rename = factory::glyph_button("pencil", "@schematic.renametag", "emptyi");
        rename.set_custom_minimum_size(Vector2::new(38.0, TAG_HEIGHT));
        rename.set_tooltip_text(&kit::t("@schematic.renametag"));
        {
            let target = dialog.to_gd();
            let state_for_rename = state.clone();
            let tag_name = tag.clone();
            let callable =
                Callable::from_fn("schematic_tag_rename", move |_args: &[&Variant]| {
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    schematics_prompt_rename_tag(&mut bound, &state_for_rename, &tag_name);
                });
            rename.connect("pressed", &callable);
        }
        row.add_child(&rename);
        // delete tag (`Icon.trash` -> `@schematic.tagdelconfirm`).
        let mut delete = factory::glyph_button("trash", "@save.delete", "emptyi");
        delete.set_custom_minimum_size(Vector2::new(38.0, TAG_HEIGHT));
        delete.set_tooltip_text(&kit::t("@save.delete"));
        {
            let target = dialog.to_gd();
            let state_for_delete = state.clone();
            let tag_name = tag.clone();
            let callable =
                Callable::from_fn("schematic_tag_delete", move |_args: &[&Variant]| {
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    schematics_prompt_delete_tag(&mut bound, &state_for_delete, &tag_name);
                });
            delete.connect("pressed", &callable);
        }
        row.add_child(&delete);
        list.bind_mut().add(&row).grow_x().pad(3.0);
        list.bind_mut().row();
    }
    body.bind_mut().add(&pane).grow_x();
    body.bind_mut().row();
    // `t.button("@schematic.texttag", Icon.add, () -> showNewTag(...))` and the
    // icon variant.
    let mut text_tag = factory::icon_button(
        "add",
        &kit::t("@schematic.texttag").to_string(),
        "flatt",
        AlignmentMode::BEGIN,
    );
    text_tag.set_custom_minimum_size(Vector2::new(300.0, TAG_HEIGHT));
    {
        let target = dialog.to_gd();
        let state_for_new = state.clone();
        let callable = Callable::from_fn("schematic_new_text_tag", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_prompt_new_tag(&mut bound, &state_for_new, -1);
        });
        text_tag.connect("pressed", &callable);
    }
    body.bind_mut().add(&text_tag).left().pad(2.0);
    body.bind_mut().row();
    let mut icon_tag = factory::icon_button(
        "add",
        &kit::t("@schematic.icontag").to_string(),
        "flatt",
        AlignmentMode::BEGIN,
    );
    icon_tag.set_custom_minimum_size(Vector2::new(300.0, TAG_HEIGHT));
    {
        let target = dialog.to_gd();
        let state_for_icon = state.clone();
        let callable = Callable::from_fn("schematic_new_icon_tag", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            state_for_icon.borrow_mut().tag_target = -1;
            schematics_show_icon_tags(&mut bound, &state_for_icon);
        });
        icon_tag.connect("pressed", &callable);
    }
    body.bind_mut().add(&icon_tag).left().pad(2.0);
    body.bind_mut().row();
    schematic_overlay_finish(&mut body);
}

/// `@editor.moveup`/`@editor.movedown` (`tags.swap` + `tagsChanged`).
fn schematics_move_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    tag: &str,
    delta: isize,
) {
    let moved = {
        let mut st = state.borrow_mut();
        st.tags
            .iter()
            .position(|value| value == tag)
            .map(|index| schematics_tag_swap(&mut st.tags, index, delta))
            .unwrap_or(false)
    };
    if !moved {
        return;
    }
    schematics_tags_changed(dialog, state);
    schematics_show_all_tags(dialog, state);
}

/// The `schematic.tagged` count button: `selectedTags.clear().add(tag)`, then
/// the filter and the pane rebuild (`showAllTags` click handler).
fn schematics_select_tag(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>, tag: &str) {
    {
        let mut st = state.borrow_mut();
        st.selected_tags.clear();
        st.selected_tags.push(tag.to_owned());
    }
    schematic_overlay_close(state);
    schematics_rebuild_tags(dialog, state);
    schematics_rebuild_list(dialog, state);
}

/// `showNewTag` (`@schematic.addtag` prompt): a new text tag.
fn schematics_prompt_new_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    target_index: i64,
) {
    let _ = dialog;
    state.borrow_mut().tag_target = target_index;
    state.borrow_mut().pending_text = SchematicTextPrompt::NewTag;
    let _ = ui_call(
        "show_text_input",
        &[
            kit::t("@schematic.addtag").to_variant(),
            GString::new().to_variant(),
            64i64.to_variant(),
            GString::new().to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `showTextInput("@schematic.renametag", "@name", tag, ...)`.
fn schematics_prompt_rename_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    tag: &str,
) {
    let _ = dialog;
    state.borrow_mut().pending_text = SchematicTextPrompt::RenameTag(tag.to_owned());
    let _ = ui_call(
        "show_text_input",
        &[
            kit::t("@schematic.renametag").to_variant(),
            kit::t("@name").to_variant(),
            64i64.to_variant(),
            gs(tag).to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `ui.showConfirm("@schematic.tagdelconfirm", ...)`.
fn schematics_prompt_delete_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    tag: &str,
) {
    let _ = dialog;
    state.borrow_mut().pending_confirm = SchematicConfirm::DeleteTag(tag.to_owned());
    let _ = ui_call(
        "show_confirm",
        &[kit::t("@schematic.tagdelconfirm").to_variant()],
    );
}

/// `showNewIconTag`: the accessible + content icon grid.
fn schematics_show_icon_tags(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let mut body = schematic_overlay_open(
        dialog,
        state,
        SchematicOverlay::IconTag,
        "@schematic.icontag",
        700.0,
    );
    let existing = state.borrow().tags.clone();
    let (pane, mut grid) = schematic_overlay_scroll(660.0, 420.0);
    // `cols = min(20, width / 52)`.
    let cols = 12usize;
    let mut count = 0usize;
    for name in SCHEMATIC_ACCESSIBLE_ICONS {
        let glyph = kit::icons::glyph(name);
        if glyph.is_empty() {
            continue;
        }
        let tag = glyph.to_string();
        if existing.iter().any(|value| value == &tag) {
            continue;
        }
        let mut button = factory::button("");
        button.set_theme_type_variation("emptyi");
        button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
        button.set_tooltip_text(&gs(name));
        let mut icon = glyph_control(name, 30.0, Color::WHITE);
        icon.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        icon.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&icon);
        schematics_wire_icon_tag(dialog, state, &mut button, &tag);
        grid.bind_mut().add(&button).pad(2.0);
        count += 1;
        if count.is_multiple_of(cols) {
            grid.bind_mut().row();
        }
    }
    for (name, character, texture) in schematic_content_icons() {
        let tag = character.to_string();
        if existing.iter().any(|value| value == &tag) {
            continue;
        }
        let mut button = factory::button("");
        button.set_theme_type_variation("emptyi");
        button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
        button.set_tooltip_text(&gs(&name));
        match texture {
            Some(texture) => {
                let mut icon = icon_rect(&texture, 40.0);
                icon.set_anchors_and_offsets_preset(LayoutPreset::CENTER);
                icon.set_mouse_filter(MouseFilter::IGNORE);
                icon.set_custom_minimum_size(Vector2::new(40.0, 40.0));
                button.add_child(&icon);
            }
            None => button.set_text(&kit::icons::glyph("star")),
        }
        schematics_wire_icon_tag(dialog, state, &mut button, &tag);
        grid.bind_mut().add(&button).pad(2.0);
        count += 1;
        if count.is_multiple_of(cols) {
            grid.bind_mut().row();
        }
    }
    if count.is_multiple_of(cols) && count > 0 {
        grid.bind_mut().row();
    }
    if count == 0 {
        grid.bind_mut()
            .add(&factory::label(&kit::t("@none").to_string()));
        grid.bind_mut().row();
    }
    body.bind_mut().add(&pane).grow();
    body.bind_mut().row();
    schematic_overlay_finish(&mut body);
}

/// Wires an icon-grid cell: add the char to `schematic-tags`, then assign it to
/// the pending schematic (if any) and return to the right overlay.
fn schematics_wire_icon_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    button: &mut Gd<Button>,
    tag: &str,
) {
    let target = dialog.to_gd();
    let state_for_pick = state.clone();
    let tag_name = tag.to_owned();
    let callable = Callable::from_fn("schematic_icon_pick", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        schematics_pick_icon_tag(&mut bound, &state_for_pick, &tag_name);
    });
    button.connect("pressed", &callable);
}

/// `showNewIconTag` result: `tags.add(out); tagsChanged(); cons.get(out)`.
fn schematics_pick_icon_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    tag: &str,
) {
    let target = {
        let st = state.borrow();
        st.tag_target
    };
    if !schematics_add_global_tag(dialog, state, tag) {
        return;
    }
    if target >= 0 {
        schematic_set_tag(dialog, state, target, tag, true);
        schematics_show_edit(dialog, state, target);
    } else {
        schematics_show_all_tags(dialog, state);
    }
}

/// `showEdit`: name, description and the per-schematic tag chips.
pub(super) fn schematics_show_edit(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
) {
    let Some(row) = schematic_row(dialog, index) else {
        return;
    };
    if schematic_is_mod(&row) {
        // Mod schematics live under the mod's data dir (`s.mod != null`); the
        // Java edit writes only when `s.file != null`, a mod file is read-only.
        let text = kit::tm_format("@mod.item.remove", &[row.mod_name.as_deref().unwrap_or("")]);
        dialog.show_toast(&text.to_string());
        return;
    }
    // Coming back from the add-tag/icon-tag flows: keep the uncommitted text.
    let returning = {
        let st = state.borrow();
        st.edit_pending_return && st.edit_index == index
    };
    let (name, description) = {
        let st = state.borrow();
        if returning {
            (st.edit_name.clone(), st.edit_description.clone())
        } else {
            (row.name.clone(), row.description.clone())
        }
    };
    {
        let mut st = state.borrow_mut();
        st.edit_index = index;
        st.edit_name = name.clone();
        st.edit_description = description.clone();
        st.tag_target = index;
        st.edit_pending_return = false;
    }
    let mut body = schematic_overlay_open(
        dialog,
        state,
        SchematicOverlay::Edit,
        "@schematic.edit",
        460.0,
    );
    // `cont.add("@schematic.tags").padRight(6f)` + `buildTags(s, tags, false)`.
    // code-instantiated: one chip row per label of the edited schematic.
    let mut tags_row = HBoxContainer::new_alloc();
    tags_row.add_theme_constant_override("separation", 4);
    body.bind_mut().add(&tags_row).grow_x().pad(2.0);
    body.bind_mut().row();
    {
        let mut st = state.borrow_mut();
        st.edit_tags_row = Some(tags_row);
    }
    schematics_edit_rebuild_tags(dialog, state, index);
    // `cont.add("@name").padRight(6f); cont.field(s.name(), null).size(400, 55)`.
    let mut name_row = HBoxContainer::new_alloc();
    name_row.add_theme_constant_override("separation", 6);
    name_row.add_child(&factory::label(&kit::t("@name").to_string()));
    let mut name_field = factory::field("");
    name_field.set_text(&gs(name.as_str()));
    name_field.set_custom_minimum_size(Vector2::new(400.0, 55.0));
    {
        let target = dialog.to_gd();
        let state_for_name = state.clone();
        let callable = Callable::from_fn(
            "schematic_edit_name_changed",
            move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                schematics_edit_name_changed(&mut bound, &state_for_name);
            },
        );
        name_field.connect("text_changed", &callable);
        // `keyDown(KeyCode.enter, accept)` for the name field (`accept` is gated
        // on a non-empty name and on the description not having focus).
        let target = dialog.to_gd();
        let state_for_submit = state.clone();
        let callable = Callable::from_fn("schematic_edit_accept", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_edit_accept(&mut bound, &state_for_submit);
        });
        name_field.connect("text_submitted", &callable);
    }
    name_row.add_child(&name_field);
    body.bind_mut().add(&name_row).grow_x().pad(2.0);
    body.bind_mut().row();
    {
        let mut st = state.borrow_mut();
        st.edit_name_field = Some(name_field.clone());
    }
    // `cont.add("@editor.description").padRight(6f)` + `cont.area(...)`.
    let mut description_row = HBoxContainer::new_alloc();
    description_row.add_theme_constant_override("separation", 6);
    description_row.add_child(&factory::label(&kit::t("@editor.description").to_string()));
    let mut area = TextEdit::new_alloc();
    area.set_text(&gs(description.as_str()));
    area.set_custom_minimum_size(Vector2::new(400.0, 140.0));
    area.set_line_wrapping_mode(LineWrappingMode::BOUNDARY);
    {
        let target = dialog.to_gd();
        let state_for_text = state.clone();
        let callable = Callable::from_fn("schematic_edit_desc", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            let mut st = state_for_text.borrow_mut();
            if let Some(area) = st.edit_description_field.clone() {
                st.edit_description = area.get_text().to_string();
            }
            let _ = &mut bound;
        });
        area.connect("text_changed", &callable);
    }
    description_row.add_child(&area);
    body.bind_mut().add(&description_row).grow_x().pad(2.0);
    body.bind_mut().row();
    {
        let mut st = state.borrow_mut();
        st.edit_description_field = Some(area);
    }
    // `buttons.button("@ok", Icon.ok, accept).disabled(nameField empty)` /
    // `@cancel` -> hide.
    let mut buttons = HBoxContainer::new_alloc();
    buttons.set_alignment(AlignmentMode::CENTER);
    buttons.add_theme_constant_override("separation", 6);
    buttons.set_meta("schematic_edit_buttons", &true.to_variant());
    let name_now = state
        .borrow()
        .edit_name_field
        .clone()
        .map(|field| field.get_text().to_string())
        .unwrap_or_default();
    let mut ok = factory::icon_button(
        "ok",
        &kit::t("@ok").to_string(),
        "defaultt",
        AlignmentMode::CENTER,
    );
    ok.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    ok.set_disabled(!schematic_edit_name_valid(&name_now));
    ok.set_meta("schematic_edit_ok", &true.to_variant());
    {
        let target = dialog.to_gd();
        let state_for_ok = state.clone();
        let callable = Callable::from_fn("schematic_edit_accept", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_edit_accept(&mut bound, &state_for_ok);
        });
        ok.connect("pressed", &callable);
    }
    buttons.add_child(&ok);
    let mut cancel = factory::icon_button(
        "cancel",
        &kit::t("@cancel").to_string(),
        "defaultt",
        AlignmentMode::CENTER,
    );
    cancel.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    {
        let state_for_cancel = state.clone();
        let callable = Callable::from_fn("schematic_edit_cancel", move |_args: &[&Variant]| {
            schematic_overlay_close(&state_for_cancel);
        });
        cancel.connect("pressed", &callable);
    }
    buttons.add_child(&cancel);
    body.bind_mut().add(&buttons).grow_x().pad(4.0);
    body.bind_mut().row();
    schematic_overlay_finish(&mut body);
}

/// `buildTags(schem, t, false)`: the chip row of the schematic's labels plus
/// the add button (`Icon.addSmall` -> `@schematic.addtag`).
fn schematics_edit_rebuild_tags(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
) {
    let Some(row) = state.borrow().edit_tags_row.clone() else {
        return;
    };
    let mut row = row;
    for child in row.get_children().iter_shared() {
        let mut child = child;
        row.remove_child(&child);
        child.queue_free();
    }
    // `schem.labels.sort(s -> tags.indexOf(s))`: the chip order follows the
    // global tag list.
    let (mut labels, tags) = {
        let st = state.borrow();
        let labels = schematic_row(dialog, index)
            .map(|schematic| schematic.labels.clone())
            .unwrap_or_default();
        (labels, st.tags.clone())
    };
    labels.sort_by_key(|label| {
        tags.iter()
            .position(|tag| tag == label)
            .unwrap_or(usize::MAX)
    });
    for label in &labels {
        // code-instantiated: one chip per label (`s.table(Tex.button, ...)`).
        let mut chip = HBoxContainer::new_alloc();
        chip.add_theme_constant_override("separation", 2);
        chip.add_child(&schematic_tag_visual(label, TAG_HEIGHT));
        let mut remove = factory::glyph_button("cancel", "@save.delete", "emptyi");
        remove.set_custom_minimum_size(Vector2::new(TAG_HEIGHT, TAG_HEIGHT));
        remove.set_tooltip_text(&kit::t("@save.delete"));
        {
            let target = dialog.to_gd();
            let state_for_remove = state.clone();
            let tag_name = label.clone();
            let callable =
                Callable::from_fn("schematic_chip_remove", move |_args: &[&Variant]| {
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    schematics_remove_schematic_tag(
                        &mut bound,
                        &state_for_remove,
                        index,
                        &tag_name,
                    );
                });
            remove.connect("pressed", &callable);
        }
        chip.add_child(&remove);
        row.add_child(&chip);
    }
    let mut add = factory::glyph_button("add", "@schematic.addtag", "emptyi");
    add.set_custom_minimum_size(Vector2::new(TAG_HEIGHT, TAG_HEIGHT));
    add.set_tooltip_text(&kit::t("@schematic.addtag"));
    {
        let target = dialog.to_gd();
        let state_for_add = state.clone();
        let callable = Callable::from_fn("schematic_chip_add", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_show_add_tag(&mut bound, &state_for_add, index);
        });
        add.connect("pressed", &callable);
    }
    row.add_child(&add);
}

/// `removeTag(s, tag)` from a chip's `Icon.cancelSmall`.
fn schematics_remove_schematic_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
    tag: &str,
) {
    if !schematic_set_tag(dialog, state, index, tag, false) {
        return;
    }
    schematics_edit_rebuild_tags(dialog, state, index);
}

/// `@name` `text_changed`: keep the draft and gate `@ok` on a non-empty name.
fn schematics_edit_name_changed(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let _ = dialog;
    let name = {
        let st = state.borrow();
        st.edit_name_field
            .clone()
            .map(|field| field.get_text().to_string())
            .unwrap_or_default()
    };
    state.borrow_mut().edit_name = name.clone();
    // `disabled(b -> nameField.getText().isEmpty())`: find the `@ok` button in
    // the overlay body (`schematic_ok_buttons` tag it at build time).
    if let Some(body) = state.borrow().overlay_body.clone() {
        for child in body.get_children().iter_shared() {
            let Ok(container) = child.try_cast::<HBoxContainer>() else {
                continue;
            };
            if !container.has_meta("schematic_edit_buttons") {
                continue;
            }
            for button in container.get_children().iter_shared() {
                if let Ok(mut button) = button.try_cast::<Button>()
                    && button.has_meta("schematic_edit_ok")
                {
                    button.set_disabled(!schematic_edit_name_valid(&name));
                }
            }
        }
    }
}

/// `accept`: `s.tags.put("name"/"description", ...); s.save(); hide; rebuild`.
fn schematics_edit_accept(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let (index, name, description) = {
        let st = state.borrow();
        let name = st
            .edit_name_field
            .clone()
            .map(|field| field.get_text().to_string())
            .unwrap_or_else(|| st.edit_name.clone());
        let description = st
            .edit_description_field
            .clone()
            .map(|area| area.get_text().to_string())
            .unwrap_or_else(|| st.edit_description.clone());
        (st.edit_index, name, description)
    };
    if !schematic_edit_name_valid(&name) {
        return;
    }
    let Some(path) = schematic_file_of(dialog, index) else {
        return;
    };
    let saved = schematic_save_file(&path, |schematic| {
        schematic.tags.insert("name".to_owned(), name.clone());
        schematic
            .tags
            .insert("description".to_owned(), description.clone());
    });
    if !saved {
        let text = kit::t("@error.title").to_string();
        dialog.show_toast(&text);
        return;
    }
    schematic_overlay_close(state);
    schematics_rebuild_list(dialog, state);
}

/// `buildTags` add button: the `@schematic.addtag` panel with the tags the
/// schematic does not carry, plus the text/icon tag creators.
fn schematics_show_add_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
) {
    // Stash the edit fields so the round trip back to `showEdit` keeps them.
    schematics_edit_stash(state);
    state.borrow_mut().edit_pending_return = true;
    let labels = schematic_row(dialog, index)
        .map(|schematic| schematic.labels.clone())
        .unwrap_or_default();
    let tags = state.borrow().tags.clone();
    let available = schematics_available_tags(&tags, &labels);
    {
        let mut st = state.borrow_mut();
        st.tag_target = index;
    }
    let mut body = schematic_overlay_open(
        dialog,
        state,
        SchematicOverlay::AddTag,
        "@schematic.addtag",
        620.0,
    );
    let (pane, mut list) = schematic_overlay_scroll(580.0, 300.0);
    if available.is_empty() {
        list.bind_mut()
            .add(&factory::label(&kit::t("@none").to_string()));
        list.bind_mut().row();
    }
    for tag in &available {
        // code-instantiated: one available-tag button per global tag.
        let mut button = factory::button("");
        button.set_theme_type_variation("defaultt");
        button.set_custom_minimum_size(Vector2::new(180.0, TAG_HEIGHT));
        button.set_autowrap_mode(AutowrapMode::OFF);
        button.add_child(&{
            let mut visual = schematic_tag_visual(tag, TAG_HEIGHT - 10.0);
            visual.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            visual.set_mouse_filter(MouseFilter::IGNORE);
            visual
        });
        {
            let target = dialog.to_gd();
            let state_for_add = state.clone();
            let tag_name = tag.clone();
            let callable = Callable::from_fn("schematic_add_tag", move |_args: &[&Variant]| {
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                schematics_add_schematic_tag(&mut bound, &state_for_add, index, &tag_name);
            });
            button.connect("pressed", &callable);
        }
        list.bind_mut().add(&button).pad(2.0);
    }
    if !available.is_empty() {
        list.bind_mut().row();
    }
    body.bind_mut().add(&pane).grow_x();
    body.bind_mut().row();
    let mut text_tag = factory::icon_button(
        "add",
        &kit::t("@schematic.texttag").to_string(),
        "flatt",
        AlignmentMode::BEGIN,
    );
    text_tag.set_custom_minimum_size(Vector2::new(300.0, TAG_HEIGHT));
    {
        let target = dialog.to_gd();
        let state_for_new = state.clone();
        let callable = Callable::from_fn("schematic_add_text_tag", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_prompt_new_tag(&mut bound, &state_for_new, index);
        });
        text_tag.connect("pressed", &callable);
    }
    body.bind_mut().add(&text_tag).left().pad(2.0);
    body.bind_mut().row();
    let mut icon_tag = factory::icon_button(
        "add",
        &kit::t("@schematic.icontag").to_string(),
        "flatt",
        AlignmentMode::BEGIN,
    );
    icon_tag.set_custom_minimum_size(Vector2::new(300.0, TAG_HEIGHT));
    {
        let target = dialog.to_gd();
        let state_for_icon = state.clone();
        let callable = Callable::from_fn("schematic_add_icon_tag", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            state_for_icon.borrow_mut().tag_target = index;
            schematics_show_icon_tags(&mut bound, &state_for_icon);
            // The overlay swap cleared the flag; keep the draft for the
            // `showEdit` round trip after the pick.
            state_for_icon.borrow_mut().edit_pending_return = true;
        });
        icon_tag.connect("pressed", &callable);
    }
    body.bind_mut().add(&icon_tag).left().pad(2.0);
    body.bind_mut().row();
    schematic_overlay_finish(&mut body);
}

/// Copies the live edit fields into the draft (`schematics_show_add_tag`).
fn schematics_edit_stash(state: &Rc<RefCell<SchematicState>>) {
    let mut st = state.borrow_mut();
    if let Some(field) = st.edit_name_field.clone() {
        st.edit_name = field.get_text().to_string();
    }
    if let Some(area) = st.edit_description_field.clone() {
        st.edit_description = area.get_text().to_string();
    }
}

/// `addTag(s, tag)` from the add panel: append, save, return to `showEdit`.
fn schematics_add_schematic_tag(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
    tag: &str,
) {
    if !schematic_set_tag(dialog, state, index, tag, true) {
        return;
    }
    schematics_show_edit(dialog, state, index);
}
