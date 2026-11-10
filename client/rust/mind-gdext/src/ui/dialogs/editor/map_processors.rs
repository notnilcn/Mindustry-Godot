// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map processors dialog controller.
//! Upstream: editor/MapProcessorsDialog.java

use crate::gdutil::variant::{dict_i64_or, dict_text_or, to_bool_lenient};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::editor::util::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindScroll};
use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{Button, HBoxContainer, VBoxContainer};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_processors" => Some(Box::new(MapProcessorsDialog::default())),
        _ => None,
    }
}

/// `LogicBlock.maxNameLength` (`LogicBlock.java:39`).
const PROCESSOR_MAX_NAME: i64 = 32;
/// Row height (`float h = 50f`).
const PROCESSOR_ROW_HEIGHT: f32 = 50.0;
/// Name button width (`Vars.mobile ? 390f : 450f`).
const PROCESSOR_NAME_WIDTH: f32 = 450.0;

/// The `map_processors` view state.
#[derive(Default)]
struct ProcessorsState {
    /// `contentSearch` (`_search`).
    search: String,
    /// `MapProcessorsDialog.processors` rows (`{x, y, tag, icon_tag}`).
    entries: Vec<VarDictionary>,
    /// The scan-driven row list (`_list`).
    list: Option<Gd<VBoxContainer>>,
    pending_rename: (i64, i64),
    pending_icon: (i64, i64),
    pending_delete: (i64, i64),
}

/// `@editor.worldprocessors` — the world-processor list editor.
#[derive(Default)]
pub struct MapProcessorsDialog {
    state: Rc<RefCell<ProcessorsState>>,
}

impl DialogController for MapProcessorsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `cont.top()` (`MapProcessorsDialog.java:58`).
        dialog.apply_cont_top(true);
        dialog.set_title_key("@editor.worldprocessors");
        dialog.should_pause = false;
        dialog.clear_buttons();
        // `addCloseButton()` then `buttons.button("@add", Icon.add, ...)`
        // (`MapProcessorsDialog.java:32-33`): Back is the first button.
        dialog.add_default_close_button();
        let target = dialog.to_gd();
        let state = self.state.clone();
        let add = Callable::from_fn("processors_add", move |_args: &[&Variant]| {
            processors_add(&target, &state);
        });
        let text = kit::t("@add").to_string();
        dialog.add_button(&text, Some(add), "add", 210.0);
        self.connect_prompts(dialog);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        {
            let mut state = self.state.borrow_mut();
            state.entries = read_processors();
        }
        self.build(dialog);
    }
}

impl MapProcessorsDialog {
    /// `shown(this::setup)`: rebuild the search row, the pane and its rows.
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut root = dialog.content_table();
        let state = self.state.clone();
        let target = dialog.to_gd();

        // `cont.table(s -> { s.image(Icon.zoom); s.field(...).growX(); }).width(440f)`.
        // code-instantiated: the zoom glyph + search field row.
        let mut search_row = factory::hbox(8);
        search_row.set_custom_minimum_size(Vector2::new(440.0, 0.0));
        search_row.add_child(&factory::glyph_label("zoom", 20, Color::WHITE));
        let mut field = factory::field(&kit::t("@players.search").to_string());
        field.set_text(&GString::from(state.borrow().search.as_str()));
        field.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let state_for_search = state.clone();
        let target_for_search = target.clone();
        let callable = Callable::from_fn("processors_search", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to_string())
                .unwrap_or_default();
            state_for_search.borrow_mut().search = text.to_lowercase();
            processors_rebuild(&target_for_search, &state_for_search);
        });
        field.connect("text_changed", &callable);
        search_row.add_child(&field);
        root.bind_mut().add(&search_row).pad(4.0);
        root.bind_mut().row();

        // `cont.pane(t -> list = t)`.
        // code-instantiated: the processor rows are scan data, not scene-authored.
        let mut scroll = MindScroll::new_alloc();
        scroll.set_h_size_flags(SizeFlags::EXPAND_FILL);
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        scroll.bind_mut().set_scrolling_disabled(true, false);
        scroll.set_custom_minimum_size(Vector2::new(680.0, 300.0));
        root.bind_mut().add(&scroll).grow_x().grow_y();
        // code-instantiated: a ScrollContainer pins a non-filling child to x=0,
        // so the full-width row centres the content-sized list.
        let mut center_row = HBoxContainer::new_alloc();
        center_row.set_alignment(AlignmentMode::CENTER);
        center_row.set_h_size_flags(SizeFlags::EXPAND_FILL);
        scroll.add_child(&center_row);
        // `cont.pane(t -> list = t)` has no grow: the list is content-sized and
        // centred (Arc cell default).
        let mut list = VBoxContainer::new_alloc();
        list.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        center_row.add_child(&list);
        state.borrow_mut().list = Some(list);
        processors_rebuild(&target, &state);
    }

    /// `_connect_prompts`: text-input/confirm answers and the icon picker.
    fn connect_prompts(&mut self, dialog: &mut MindDialog) {
        let target = dialog.to_gd();
        let state = self.state.clone();
        let callable = Callable::from_fn("processors_text_input", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to_string())
                .unwrap_or_default();
            let target_for = target.clone();
            let state_for = state.clone();
            defer_action("processors_rename_result", move || {
                processors_on_rename(&target_for, &state_for, &text);
            });
        });
        connect_prompt_once(
            dialog,
            "processors_text_input",
            "text_input_result",
            &callable,
        );

        let target = dialog.to_gd();
        let state = self.state.clone();
        let callable = Callable::from_fn("processors_confirm", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .map(|value| to_bool_lenient(value))
                .unwrap_or(false);
            let target_for = target.clone();
            let state_for = state.clone();
            defer_action("processors_delete_result", move || {
                processors_on_confirm(&target_for, &state_for, confirmed);
            });
        });
        connect_prompt_once(dialog, "processors_confirm", "confirm_result", &callable);
        let state = self.state.clone();
        connect_icon_select(dialog, &state);
    }
}

/// `ui.showConfirm("@editor.worldprocessors.delete.confirm", ...)`.
fn processors_confirm_delete(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<ProcessorsState>>,
    entry: &VarDictionary,
) {
    let _ = dialog;
    state.borrow_mut().pending_delete = (dict_i64_or(entry, "x", 0), dict_i64_or(entry, "y", 0));
    let _ = ui_call(
        "show_confirm",
        &[kit::t("@editor.worldprocessors.delete.confirm").to_variant()],
    );
}

/// `ui.showTextInput("", "@editor.name", LogicBlock.maxNameLength, ...)`.
fn processors_rename(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<ProcessorsState>>,
    entry: &VarDictionary,
) {
    let _ = dialog;
    let tag = dict_text_or(entry, "tag");
    state.borrow_mut().pending_rename = (dict_i64_or(entry, "x", 0), dict_i64_or(entry, "y", 0));
    let _ = ui_call(
        "show_text_input",
        &[
            GString::new().to_variant(),
            kit::t("@editor.name").to_variant(),
            PROCESSOR_MAX_NAME.to_variant(),
            GString::from(tag.as_str()).to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `iconSelect.show(ic -> log.iconTag = ic; rebuild())`.
fn processors_open_icon_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<ProcessorsState>>,
    entry: &VarDictionary,
) {
    state.borrow_mut().pending_icon = (dict_i64_or(entry, "x", 0), dict_i64_or(entry, "y", 0));
    connect_icon_select(dialog, state);
    let _ = ui_call(
        "open_dialog",
        &[
            GString::from("icon_select").to_variant(),
            GString::from("{}").to_variant(),
        ],
    );
}

/// `_connect_icon_select`: bind `icon_selected` on the `icon_select` dialog.
fn connect_icon_select(dialog: &mut MindDialog, state: &Rc<RefCell<ProcessorsState>>) {
    let meta = "_batch_f_processors_icon";
    if dialog.base().has_meta(meta) {
        return;
    }
    let Some(mut target) = dialog_node("icon_select") else {
        return;
    };
    if !target.has_signal("icon_selected") {
        return;
    }
    let dialog_gd = dialog.to_gd();
    let state = state.clone();
    let callable = Callable::from_fn("processors_icon_selected", move |args: &[&Variant]| {
        let region = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let target_for = dialog_gd.clone();
        let state_for = state.clone();
        defer_action("processors_icon_result", move || {
            processors_on_icon_selected(&target_for, &state_for, &region);
        });
    });
    let _ = target.connect("icon_selected", &callable);
    dialog.base_mut().set_meta(meta, &true.to_variant());
}

/// `log.showEditDialog(true)`: the shared logic editor over the editor tile.
fn processors_edit(dialog: &Gd<MindDialog>, entry: &VarDictionary) {
    let _ = dialog;
    let mut context = VarDictionary::new();
    context.set(&"editor".to_variant(), &true.to_variant());
    context.set(&"x".to_variant(), &dict_i64_or(entry, "x", 0).to_variant());
    context.set(&"y".to_variant(), &dict_i64_or(entry, "y", 0).to_variant());
    let _ = ui_call(
        "open_dialog",
        &[
            GString::from("logic").to_variant(),
            GString::from(stringify(&context.to_variant()).as_str()).to_variant(),
        ],
    );
}

/// `buttons.button("@add", Icon.add, ...)`.
fn processors_add(dialog: &Gd<MindDialog>, state: &Rc<RefCell<ProcessorsState>>) {
    let added = editor_call("add_processor", &[])
        .map(|value| to_bool_lenient(&value))
        .unwrap_or(false);
    if !added {
        notify_key("@editor.worldprocessors.nospace");
        return;
    }
    refresh_processors(state);
    dialog_refresh(dialog);
}

/// `MapProcessorsDialog.setup`: the editor scans its world for processors.
fn read_processors() -> Vec<VarDictionary> {
    let Some(value) = editor_call("processors", &[]) else {
        return Vec::new();
    };
    let Ok(rows) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    rows.iter_shared()
        .filter_map(|row| row.try_to::<VarDictionary>().ok())
        .collect()
}

/// Re-reads the processor rows after a mutation.
fn refresh_processors(state: &Rc<RefCell<ProcessorsState>>) {
    let entries = read_processors();
    state.borrow_mut().entries = entries;
}

/// `rebuild()`: the filtered processor rows (or `@editor.worldprocessors.none`).
fn processors_rebuild(dialog: &Gd<MindDialog>, state: &Rc<RefCell<ProcessorsState>>) {
    let list = state.borrow().list.clone();
    let Some(mut list) = list else {
        return;
    };
    kit::clear_children(&mut list);
    let (entries, search) = {
        let state = state.borrow();
        (state.entries.clone(), state.search.clone())
    };
    if entries.is_empty() {
        // `list.add("@editor.worldprocessors.none")`: the bundle text carries
        // `[lightgray]` markup plus an icon token.
        let label = factory::label(&markup("@editor.worldprocessors.none"));
        list.add_child(&label);
        return;
    }
    for entry in entries {
        let tag = dict_text_or(&entry, "tag");
        // `text.isEmpty() || (log.tag != null && log.tag.toLowerCase().contains(text))`.
        if !search.is_empty() && !tag.to_lowercase().contains(&search) {
            continue;
        }
        let row = processors_row(dialog, state, &entry);
        list.add_child(&row);
    }
}

/// One row: icon select, name (opens the logic editor), rename, delete.
fn processors_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<ProcessorsState>>,
    entry: &VarDictionary,
) -> Gd<HBoxContainer> {
    // code-instantiated: one scan row (icon + name + actions).
    let mut row = factory::hbox(4);
    row.set_custom_minimum_size(Vector2::new(0.0, PROCESSOR_ROW_HEIGHT + 8.0));

    // `t.button(log.iconTag == 0 ? Styles.none : ..., Styles.graySquarei, ...)`.
    // code-instantiated: the icon cell holds a glyph char or nothing.
    let mut icon_button = Button::new_alloc();
    icon_button.set_theme_type_variation("graySquarei");
    icon_button.set_custom_minimum_size(Vector2::new(PROCESSOR_ROW_HEIGHT, PROCESSOR_ROW_HEIGHT));
    let icon_code = dict_i64_or(entry, "icon_tag", 0);
    if let Ok(code) = u32::try_from(icon_code)
        && let Some(character) = char::from_u32(code)
    {
        let mut glyph = factory::glyph_label("", 26, Color::WHITE);
        glyph.set_text(&GString::from(character.to_string().as_str()));
        glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        icon_button.add_child(&glyph);
    }
    let target = dialog.clone();
    let state_for_icon = state.clone();
    let entry_for_icon = entry.clone();
    let callable = Callable::from_fn("processor_pick_icon", move |_args: &[&Variant]| {
        if !target.is_instance_valid() {
            return;
        }
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        processors_open_icon_select(&mut bound, &state_for_icon, &entry_for_icon);
    });
    icon_button.connect("pressed", &callable);
    row.add_child(&icon_button);

    // `t.button((tag == null ? "<no name>\n" : "[accent]" + tag + "\n") + "[lightgray][[x, y]", ...)`.
    // code-instantiated: the wide name cell carries the two-line markup.
    let mut name_button = factory::button("");
    name_button.set_theme_type_variation("grayt");
    name_button.set_custom_minimum_size(Vector2::new(PROCESSOR_NAME_WIDTH, PROCESSOR_ROW_HEIGHT));
    let mut name_label = factory::label(&processors_entry_markup(entry));
    name_label.set_horizontal_alignment(HorizontalAlignment::LEFT);
    name_label.set_vertical_alignment(VerticalAlignment::CENTER);
    name_label.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    name_label.set_offset(Side::LEFT, 10.0);
    name_label.set_mouse_filter(MouseFilter::IGNORE);
    name_button.add_child(&name_label);
    let target = dialog.clone();
    let entry_for_edit = entry.clone();
    let callable = Callable::from_fn("processor_edit", move |_args: &[&Variant]| {
        processors_edit(&target, &entry_for_edit);
    });
    name_button.connect("pressed", &callable);
    row.add_child(&name_button);

    // `t.button(Icon.pencil, Styles.graySquarei, ...)` — rename.
    let target = dialog.clone();
    let state_for_rename = state.clone();
    let entry_for_rename = entry.clone();
    let rename = Callable::from_fn("processor_rename", move |_args: &[&Variant]| {
        processors_rename(&target, &state_for_rename, &entry_for_rename);
    });
    row.add_child(&square_cell(
        "pencil",
        &kit::t("@editor.worldprocessors.editname").to_string(),
        rename,
    ));

    // `t.button(Icon.trash, ...)` — delete with confirm.
    let target = dialog.clone();
    let state_for_delete = state.clone();
    let entry_for_delete = entry.clone();
    let delete = Callable::from_fn("processor_delete", move |_args: &[&Variant]| {
        processors_confirm_delete(&target, &state_for_delete, &entry_for_delete);
    });
    row.add_child(&square_cell(
        "trash",
        &kit::t("@editor.worldprocessors.delete.confirm").to_string(),
        delete,
    ));
    row
}

/// `(log.tag == null ? "<no name>\n" : "[accent]" + log.tag + "\n") + "[lightgray][[x, y]"`.
fn processors_entry_markup(entry: &VarDictionary) -> String {
    let tag = dict_text_or(entry, "tag");
    let head = if tag.is_empty() {
        String::from("<no name>")
    } else {
        format!("[accent]{tag}")
    };
    markup(&format!(
        "{head}\n[lightgray][[{}, {}]",
        dict_i64_or(entry, "x", 0),
        dict_i64_or(entry, "y", 0)
    ))
}

/// `_on_text_input`.
fn processors_on_rename(dialog: &Gd<MindDialog>, state: &Rc<RefCell<ProcessorsState>>, text: &str) {
    let target = std::mem::replace(&mut state.borrow_mut().pending_rename, (-1, -1));
    if target.0 < 0 || text.is_empty() {
        return;
    }
    let updated = editor_call(
        "rename_processor",
        &[
            target.0.to_variant(),
            target.1.to_variant(),
            GString::from(text).to_variant(),
        ],
    )
    .map(|value| to_bool_lenient(&value))
    .unwrap_or(false);
    if updated {
        refresh_processors(state);
        dialog_refresh(dialog);
    }
}

/// `_on_confirm`.
fn processors_on_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<ProcessorsState>>,
    confirmed: bool,
) {
    let target = std::mem::replace(&mut state.borrow_mut().pending_delete, (-1, -1));
    if !confirmed || target.0 < 0 {
        return;
    }
    let updated = editor_call(
        "delete_processor",
        &[target.0.to_variant(), target.1.to_variant()],
    )
    .map(|value| to_bool_lenient(&value))
    .unwrap_or(false);
    if updated {
        refresh_processors(state);
        dialog_refresh(dialog);
    }
}

/// `_on_icon_selected`.
fn processors_on_icon_selected(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<ProcessorsState>>,
    region_name: &str,
) {
    let target = std::mem::replace(&mut state.borrow_mut().pending_icon, (-1, -1));
    if target.0 < 0 {
        return;
    }
    let _ = ui_call("close_dialog", &[GString::from("icon_select").to_variant()]);
    let code = processors_icon_code(region_name);
    let updated = editor_call(
        "set_processor_icon",
        &[
            target.0.to_variant(),
            target.1.to_variant(),
            code.to_variant(),
        ],
    )
    .map(|value| to_bool_lenient(&value))
    .unwrap_or(false);
    if updated {
        refresh_processors(state);
        dialog_refresh(dialog);
    }
}

/// `Iconc.codes.get(key)`: the icon select emits a region name; the stored
/// `iconTag` is its PUA char.
fn processors_icon_code(region_name: &str) -> i64 {
    if let Some(assets) = kit::assets() {
        let text = assets.bind().unicode_str(GString::from(region_name));
        if let Some(character) = text.to_string().chars().next() {
            return character as i64;
        }
    }
    let glyph = kit::glyph(region_name);
    glyph
        .to_string()
        .chars()
        .next()
        .map(|character| character as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::editor::util::tests::engine_available;

    #[test]
    fn processor_markup_matches_java() {
        if !engine_available() {
            return;
        }
        let mut entry = VarDictionary::new();
        entry.set(&"tag".to_variant(), &GString::from("main").to_variant());
        entry.set(&"x".to_variant(), &3i64.to_variant());
        entry.set(&"y".to_variant(), &(-4i64).to_variant());
        let markup = processors_entry_markup(&entry);
        assert!(markup.contains("main"));
        assert!(markup.contains("3, -4"));
    }
}
