// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The schematics dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/SchematicsDialog.java

use super::files::*;
use super::import_export::*;
use super::subdialogs::*;
use crate::gdutil::variant::{parse_array, to_text_godot};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::campaign::util::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::widgets::schematic_image::MindSchematicImage;
use crate::ui::kit::{self, MindScroll, MindTable, styles};
use godot::builtin::{Side, VarArray, VarDictionary};
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::{AutowrapMode, OverrunBehavior};
use godot::classes::{
    Button, CanvasItem, Control, FileDialog, HBoxContainer, Label, LineEdit, PanelContainer,
    StyleBoxFlat, TextEdit, VBoxContainer,
};
use godot::prelude::*;
use mind_core::io::{FileSystem, NativeFs};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "schematics" => Some(Box::new(SchematicsDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// SchematicsDialog
// ---------------------------------------------------------------------------

/// `SchematicsDialog.ignoreSymbols` (the regex symbol class).
const IGNORE_SYMBOLS: &str = "`~!@#$%^&*()-_=+{}|;:'\",<.>/?";
/// `SchematicsDialog.tagh` (tag button height).
pub(super) const TAG_HEIGHT: f32 = 42.0;
/// `rebuildPane`: `cols = max((int)(screenW / 230), 1)`.
pub(in crate::ui::dialogs) const SCHEMATIC_COLUMN_WIDTH: f32 = 230.0;
/// The fixed card action row (`SchematicsDialog.java:150-205`).
const SCHEMATIC_ACTIONS: [(&str, &str, &str); 4] = [
    ("info", "@info.title", "info"),
    ("upload", "@editor.export", "export"),
    ("pencil", "@schematic.edit", "edit"),
    ("trash", "@save.delete", "delete"),
];

/// The `.msch` base64 prefix (`Vars.schematicBaseStart`; every
/// `Schematics.writeBase64` result starts with it).
pub(super) const SCHEMATIC_BASE_START: &str = "bXNjaAB";
/// `Vars.accessibleIcons`: the icon-font names the tag picker offers
/// (`Iconc` field names, resolved through `kit::icons`).
pub(super) const SCHEMATIC_ACCESSIBLE_ICONS: [&str; 31] = [
    "effect",
    "power",
    "logic",
    "units",
    "liquid",
    "production",
    "defense",
    "turret",
    "distribution",
    "crafting",
    "settings",
    "cancel",
    "zoom",
    "ok",
    "star",
    "home",
    "pencil",
    "up",
    "down",
    "left",
    "right",
    "hammer",
    "warning",
    "tree",
    "admin",
    "map",
    "modePvp",
    "terrain",
    "modeSurvival",
    "commandRally",
    "commandAttack",
];

/// The transient sub-dialog a Java `BaseDialog` opens over `SchematicsDialog`
/// (`showAllTags`, `showEdit`, `showImport`, `showExport`, the icon picker and
/// the per-schematic add-tag panel).
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum SchematicOverlay {
    #[default]
    None,
    /// `showAllTags` (`@schematic.edittags`).
    Tags,
    /// `showEdit` (`@schematic.edit`): name, description and the tag chips.
    Edit,
    /// `buildTags` add button: the tags the schematic does not carry yet.
    AddTag,
    /// `showNewIconTag`: the accessible + content icon grid.
    IconTag,
    /// `showImport` (`@editor.import`).
    Import,
    /// `showExport` (`@editor.export`).
    Export,
}

/// The pending `ui.showConfirm` action (`SchematicsDialog.java:166-174,566-580`).
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) enum SchematicConfirm {
    #[default]
    None,
    /// `@schematic.delete.confirm` for the card action.
    DeleteSchematic(i64),
    /// `@schematic.tagdelconfirm` for the tag editor.
    DeleteTag(String),
}

/// The pending `ui.showTextInput` action (`showNewTag`/tag rename).
#[derive(Clone, Default, PartialEq, Eq)]
pub(super) enum SchematicTextPrompt {
    #[default]
    None,
    /// `showNewTag`: a new text tag for the global list.
    NewTag,
    /// `@schematic.renametag`: rename `old` in the list and every schematic.
    RenameTag(String),
}

/// Controller state for the reused schematics dialog node.
#[derive(Default)]
pub(super) struct SchematicState {
    field: Option<Gd<LineEdit>>,
    list: Option<Gd<MindTable>>,
    tags_row: Option<Gd<HBoxContainer>>,
    pub(super) tags: Vec<String>,
    pub(super) selected_tags: Vec<String>,
    /// First visible schematic's view index (`firstSchematic`; Enter uses it).
    first_index: i64,
    /// The open sub-dialog (Java `BaseDialog`s are transient, code-built).
    pub(super) overlay_root: Option<Gd<Control>>,
    pub(super) overlay_body: Option<Gd<MindTable>>,
    pub(super) overlay_kind: SchematicOverlay,
    /// `showEdit` target and its uncommitted fields. Kept across the add-tag
    /// and icon-tag sub-flows so a round trip never drops typed text.
    pub(super) edit_index: i64,
    pub(super) edit_name: String,
    pub(super) edit_description: String,
    pub(super) edit_name_field: Option<Gd<LineEdit>>,
    pub(super) edit_description_field: Option<Gd<TextEdit>>,
    pub(super) edit_tags_row: Option<Gd<HBoxContainer>>,
    /// True while the add-tag/icon-tag sub-flow will return to `showEdit`
    /// (`schematics_show_edit` then keeps the uncommitted fields).
    pub(super) edit_pending_return: bool,
    /// The schematic a new tag is assigned to (`-1` = global list only).
    pub(super) tag_target: i64,
    /// The live `.msch` file dialog (`FileChooser`; a transient window).
    pub(super) file_chooser: Option<Gd<FileDialog>>,
    pub(super) pending_confirm: SchematicConfirm,
    pub(super) pending_text: SchematicTextPrompt,
}

impl SchematicState {
    fn new() -> Self {
        Self {
            first_index: -1,
            edit_index: -1,
            tag_target: -1,
            ..Self::default()
        }
    }
}

/// The schematic library (`SchematicsDialog`).
pub struct SchematicsDialog {
    state: Rc<RefCell<SchematicState>>,
}

impl Default for SchematicsDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(SchematicState::new())),
        }
    }
}

impl DialogController for SchematicsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        schematics_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        schematics_shown(dialog, &self.state);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        // `BaseDialog.hide` closes the transient sub-dialogs; the Java ones are
        // separate windows that outlive the parent, the port's overlay does not.
        let _ = dialog;
        schematic_overlay_close(&self.state);
        state_reset_prompts(&self.state);
    }
}

/// Drops any pending confirm/text-prompt latch (a cancelled or superseded
/// prompt must never fire on the next unrelated signal).
fn state_reset_prompts(state: &Rc<RefCell<SchematicState>>) {
    schematic_file_chooser_close(state);
    let mut st = state.borrow_mut();
    st.pending_confirm = SchematicConfirm::None;
    st.pending_text = SchematicTextPrompt::None;
    st.tag_target = -1;
}

/// Frees the live `.msch` file dialog (it is a `Window`, not a `Control` child,
/// so hiding the schematics dialog does not dismiss it).
pub(super) fn schematic_file_chooser_close(state: &Rc<RefCell<SchematicState>>) {
    let chooser = state.borrow_mut().file_chooser.take();
    if let Some(mut chooser) = chooser
        && chooser.is_instance_valid()
    {
        chooser.queue_free();
    }
}

fn schematics_ready(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    // `cont.top()` (`SchematicsDialog.java:78`): the body sticks to the top.
    dialog.apply_cont_top(true);
    dialog.set_title_key("@schematics");
    dialog.should_pause = true;
    dialog.enable_full_dialog(true);
    // `makeButtonOverlay()` (`SchematicsDialog.java:64`).
    dialog.apply_button_overlay(true);
    schematics_build(dialog, state);
    dialog.add_default_close_button();
    // `buttons.button("@schematic.import", Icon.download, this::showImport)`:
    // the import button follows Back in the bottom row.
    let mut import = factory::button("");
    import.set_custom_minimum_size(Vector2::new(200.0, 54.0));
    dialog.apply_pane_button(&mut import, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control("download", 20.0, Color::WHITE).upcast();
    let label = kit::t("@schematic.import").to_string();
    dialog.build_button_content(&mut import, Some(glyph), &label);
    let target = dialog.to_gd();
    let state_for_import = state.clone();
    let callable = Callable::from_fn("schematic_import", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        schematics_import(&mut bound, &state_for_import);
    });
    import.connect("pressed", &callable);
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&import);
    }
    if let Some(ui) = kit::ui() {
        let target = dialog.to_gd();
        let state_for_confirm = state.clone();
        let callable = Callable::from_fn("schematic_confirm", move |args: &[&Variant]| {
            let confirmed = args
                .first()
                .map(|value| (*value).to::<bool>())
                .unwrap_or(false);
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_on_confirm(&mut bound, &state_for_confirm, confirmed);
        });
        connect_once(
            dialog,
            "schematic_confirm",
            &ui,
            "confirm_result",
            &callable,
        );
        let target = dialog.to_gd();
        let state_for_input = state.clone();
        let callable = Callable::from_fn("schematic_text_input", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| (*value).to::<GString>().to_string())
                .unwrap_or_default();
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_on_text_input(&mut bound, &state_for_input, &text);
        });
        connect_once(
            dialog,
            "schematic_text_input",
            &ui,
            "text_input_result",
            &callable,
        );
    }
}

fn schematics_build(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let mut root = dialog.content_table();
    // `cont.table(s -> { s.image(Icon.zoom); s.add(searchField).growX() })`.
    // code-instantiated: the search row is a fixed composite (zoom glyph +
    // field) that filters the runtime schematic rows.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 4);
    let zoom = glyph_control("zoom", 22.0, Color::WHITE);
    search_row.add_child(&zoom);
    let mut field = factory::field(&kit::t("@schematic.search").to_string());
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        // `searchField.changed(() -> { search = searchField.getText(); rebuildPane.run(); })`.
        let target = dialog.to_gd();
        let state_for_filter = state.clone();
        let callable = Callable::from_fn("schematic_search", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_rebuild_list(&mut bound, &state_for_filter);
        });
        field.connect("text_changed", &callable);
        // `Binding.chat` with the field focused -> use/inspect the first match.
        let target = dialog.to_gd();
        let state_for_submit = state.clone();
        let callable = Callable::from_fn("schematic_search_submit", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_search_submit(&mut bound, &state_for_submit);
        });
        field.connect("text_submitted", &callable);
    }
    search_row.add_child(&field);
    root.bind_mut().add(&search_row).grow_x().pad(4.0);
    root.bind_mut().row();
    // `cont.table(in -> { in.left(); in.add("@schematic.tags"); pane(toggles);
    // button(Icon.pencilSmall, showAllTags) })`.
    let mut tags_row = HBoxContainer::new_alloc();
    tags_row.add_theme_constant_override("separation", 2);
    root.bind_mut().add(&tags_row).grow_x().pad(2.0);
    root.bind_mut().row();
    // `cont.pane(t -> ...)` with `cols = max(screenW / 230, 1)`.
    let mut list = MindTable::new_alloc();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    root.bind_mut().add(&list).grow_x().grow_y();
    root.bind_mut().row();
    {
        let mut st = state.borrow_mut();
        st.field = Some(field);
        st.tags_row = Some(tags_row);
        st.list = Some(list);
    }
    schematics_rebuild_tags(dialog, state);
    schematics_rebuild_list(dialog, state);
}

fn schematics_shown(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    // A re-show starts from the library top level (`shown` in Java rebuilds
    // `setup`, which has no sub-dialog state).
    schematic_overlay_close(state);
    state_reset_prompts(state);
    schematics_load_tags(dialog, state);
    // `shown(() -> { searchField.setText(search = ""); setup(); })`.
    if let Some(mut field) = state.borrow().field.clone() {
        field.set_text("");
    }
    schematics_rebuild_tags(dialog, state);
    schematics_rebuild_list(dialog, state);
    // `SchematicsDialog.show`: desktop focuses the search field.
    if let Some(mut field) = state.borrow().field.clone() {
        field.call_deferred("grab_focus", &[]);
    }
}

/// `SchematicsDialog` tags (`Core.settings` JSON `schematic-tags`).
fn schematics_load_tags(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let _ = dialog;
    let mut tags: Vec<String> = Vec::new();
    let text = setting_text("schematic-tags", "[]");
    for tag in parse_array(&text).iter_shared() {
        let value = to_text_godot(&tag);
        if !value.is_empty() && !tags.contains(&value) {
            tags.push(value);
        }
    }
    state.borrow_mut().tags = tags;
    schematics_check_tags(dialog, state);
}

/// `checkTags()` (`SchematicsDialog.java:367-379`): every label of every
/// schematic joins the global tag list in first-seen order.
fn schematics_check_tags(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let schematics = dialog.campaign_section("schematics");
    let mut st = state.borrow_mut();
    for row_variant in schematics.iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        for label in row_arr(&row, "labels").iter_shared() {
            let tag = to_text_godot(&label);
            if !tag.is_empty() && !st.tags.contains(&tag) {
                st.tags.push(tag);
            }
        }
    }
}

pub(super) fn schematics_save_tags(state: &Rc<RefCell<SchematicState>>) {
    let tags = {
        let st = state.borrow();
        st.tags.clone()
    };
    let mut array = VarArray::new();
    for tag in &tags {
        array.push(&gs(tag.as_str()).to_variant());
    }
    setting_put_text("schematic-tags", &json_stringify_array(&array));
}

/// `rebuildTags` (`SchematicsDialog.java:86-116`): `@schematic.tags` label, one
/// toggle per saved tag (checked while selected) and the edit pencil.
pub(super) fn schematics_rebuild_tags(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
) {
    let Some(mut row) = state.borrow().tags_row.clone() else {
        return;
    };
    for child in row.get_children().iter_shared() {
        let mut child = child;
        child.queue_free();
    }
    let (tags, selected) = {
        let st = state.borrow();
        (st.tags.clone(), st.selected_tags.clone())
    };
    // code-instantiated: the tag label/toggles follow the settings tag list.
    let mut tags_label = factory::label(&kit::t("@schematic.tags").to_string());
    tags_label.set_custom_minimum_size(Vector2::new(0.0, TAG_HEIGHT));
    rich(&tags_label).set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    row.add_child(&tags_label);
    // `in.add("@schematic.tags").padRight(4)`.
    row.add_child(&factory::space(2.0, 0.0));
    // `in.pane(Styles.noBarPane, t -> ...).fillX().height(tagh).scrollY(false)`:
    // the toggles scroll horizontally without a visible bar.
    // code-instantiated: rebuilt with the tag list (`rebuildTags`).
    let mut pane = MindScroll::new_alloc();
    pane.set_h_size_flags(SizeFlags::EXPAND_FILL);
    pane.set_custom_minimum_size(Vector2::new(0.0, TAG_HEIGHT + 4.0));
    pane.set_horizontal_scroll_mode(ScrollMode::SHOW_NEVER);
    pane.set_vertical_scroll_mode(ScrollMode::DISABLED);
    let mut toggles = HBoxContainer::new_alloc();
    toggles.add_theme_constant_override("separation", 4);
    pane.add_child(&toggles);
    row.add_child(&pane);
    for tag in &tags {
        // Icon tags store a single PUA character (`showNewIconTag`); a plain
        // `Button` label would render tofu, so the toggle carries the icon
        // texture/glyph instead (Java registers the glyph on `Fonts.def`).
        let icon_tag = schematic_icon_tag_char(tag).is_some();
        let mut button = if icon_tag {
            factory::button("")
        } else {
            factory::button(&schematics_tag_plain(tag))
        };
        if icon_tag {
            let mut visual = schematic_tag_visual(tag, TAG_HEIGHT - 14.0);
            visual.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            visual.set_mouse_filter(MouseFilter::IGNORE);
            button.add_child(&visual);
        }
        // Tag labels carry Arc color markup (`[magenta]advanced-logic`): tint
        // the button text with that color.
        if let Some(tint) = schematics_tag_color(tag) {
            for key in [
                "font_color",
                "font_hover_color",
                "font_pressed_color",
                "font_focus_color",
            ] {
                button.add_theme_color_override(key, tint);
            }
        }
        button.set_theme_type_variation("togglet");
        button.set_toggle_mode(true);
        button.set_pressed(selected.contains(tag));
        button.set_custom_minimum_size(Vector2::new(0.0, TAG_HEIGHT));
        // `.with(c -> c.getLabel().setWrap(false))`.
        button.set_autowrap_mode(AutowrapMode::OFF);
        let target = dialog.to_gd();
        let state_for_tag = state.clone();
        let tag_name = tag.clone();
        let callable = Callable::from_fn("schematic_toggle_tag", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_toggle_tag(&mut bound, &state_for_tag, &tag_name);
        });
        button.connect("pressed", &callable);
        toggles.add_child(&button);
    }
    let mut pencil = factory::glyph_button(
        "pencil",
        &kit::t("@schematic.edittags").to_string(),
        "emptyi",
    );
    pencil.set_custom_minimum_size(Vector2::new(TAG_HEIGHT, TAG_HEIGHT));
    let target = dialog.to_gd();
    let state_for_edit_tags = state.clone();
    let callable = Callable::from_fn("schematic_edit_tags", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        schematics_edit_tags(&mut bound, &state_for_edit_tags);
    });
    pencil.connect("pressed", &callable);
    row.add_child(&pencil);
}

/// A tag with its Arc color markup (`[magenta]`, `[#ff00ff]`, `[]`) removed.
pub(super) fn schematics_tag_plain(tag: &str) -> String {
    let characters: Vec<char> = tag.chars().collect();
    let mut out = String::with_capacity(tag.len());
    let mut index = 0usize;
    while index < characters.len() {
        if characters[index] == '['
            && let Some(close) = characters[index..].iter().position(|value| *value == ']')
        {
            index += close + 1;
            continue;
        }
        out.push(characters[index]);
        index += 1;
    }
    out
}

/// The first color the tag's markup selects (`None` for plain tags).
pub(super) fn schematics_tag_color(tag: &str) -> Option<Color> {
    let rest = tag.strip_prefix('[')?;
    let close = rest.find(']')?;
    let inner = &rest[..close];
    if inner.is_empty() {
        return None;
    }
    let parsed = Color::from_string(&gs(inner))?;
    // `Color.from_string(s, Color(0, 0, 0, 0))` + sentinel compare.
    if parsed == Color::from_rgba(0.0, 0.0, 0.0, 0.0) {
        return None;
    }
    Some(parsed)
}

fn schematics_toggle_tag(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>, tag: &str) {
    {
        let mut st = state.borrow_mut();
        if let Some(position) = st.selected_tags.iter().position(|value| value == tag) {
            st.selected_tags.remove(position);
        } else {
            st.selected_tags.push(tag.to_string());
        }
    }
    schematics_rebuild_list(dialog, state);
}

/// `showAllTags` pencil: the global tag editor (`@schematic.edittags`).
fn schematics_edit_tags(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    schematics_show_all_tags(dialog, state);
}

/// `rebuildPane` (`SchematicsDialog.java:118-224`).
pub(super) fn schematics_rebuild_list(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
) {
    let Some(mut list) = state.borrow().list.clone() else {
        return;
    };
    list.bind_mut().clear();
    let search = state
        .borrow()
        .field
        .clone()
        .map(|field| field.get_text().to_string())
        .unwrap_or_default();
    let needle = schematics_strip_symbols(&search.to_lowercase());
    let schematics = dialog.campaign_section("schematics");
    // `cols = max((int)(screenW / 230), 1)`.
    let cols = ((dialog.base().get_viewport_rect().size.x / SCHEMATIC_COLUMN_WIDTH) as i64).max(1);
    let mut shown_count = 0i64;
    let mut first_index = -1i64;
    let selected_tags = {
        let st = state.borrow();
        st.selected_tags.clone()
    };
    for row_variant in schematics.iter_shared() {
        let Ok(schematic) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        // `selectedTags.any() && !s.labels.containsAll(selectedTags)`.
        let labels = row_arr(&schematic, "labels");
        let mut tagged = true;
        for tag in &selected_tags {
            if !labels
                .iter_shared()
                .any(|value| to_text_godot(&value) == *tag)
            {
                tagged = false;
                break;
            }
        }
        if !tagged {
            continue;
        }
        // `!search.isEmpty() && !ignoreSymbols(name).contains(searchString)`.
        let name = row_str(&schematic, "name");
        if !needle.is_empty() && !schematics_strip_symbols(&name.to_lowercase()).contains(&needle) {
            continue;
        }
        if first_index < 0 {
            first_index = row_i64_or(&schematic, "index", -1);
        }
        let card = schematics_build_card(dialog, state, &schematic);
        list.bind_mut().add(&card).pad(4.0);
        shown_count += 1;
        if shown_count % cols == 0 {
            list.bind_mut().row();
        }
    }
    state.borrow_mut().first_index = first_index;
    if shown_count == 0 {
        // `@none.found` when filtering, plain `@none` (light gray) otherwise.
        if !needle.is_empty() || !selected_tags.is_empty() {
            let label = factory::label(&kit::t("@none.found").to_string());
            list.bind_mut().add(&label);
        } else {
            let label = factory::label(&kit::t("@none").to_string());
            let mut canvas: Gd<CanvasItem> = label.clone().upcast();
            canvas.set_modulate(styles::ARC_LIGHT_GRAY);
            list.bind_mut().add(&label);
        }
        list.bind_mut().row();
    }
    list.bind_mut().sort_now();
}

/// One card (`SchematicsDialog.java:150-205`): a flat button with the `pane`
/// background holding the 50 px action row and the 200 px image stack.
fn schematics_build_card(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    schematic: &VarDictionary,
) -> Gd<Button> {
    let index = row_i64_or(schematic, "index", -1);
    // code-instantiated: one card per filtered schematic row.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("flati");
    button.set_custom_minimum_size(Vector2::new(200.0, 250.0));
    let pane = MindDialog::pane_box(Color::BLACK, OUTLINE_GRAY, 4.0, 2.0, -1.0);
    for style_name in ["normal", "hover", "pressed"] {
        button.add_theme_stylebox_override(style_name, &pane);
    }
    {
        let target = dialog.to_gd();
        let state_for_press = state.clone();
        let callable = Callable::from_fn("schematic_card_pressed", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_on_card_pressed(&mut bound, &state_for_press, index);
        });
        button.connect("pressed", &callable);
    }
    let mut column = VBoxContainer::new_alloc();
    column.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    column.add_theme_constant_override("separation", 0);
    column.set_mouse_filter(MouseFilter::IGNORE);
    // `b.table(buttons -> { buttons.left(); buttons.defaults().size(50f); ... })
    // .growX().height(50f)`.
    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 0);
    actions.set_mouse_filter(MouseFilter::IGNORE);
    for (icon, tooltip, action) in SCHEMATIC_ACTIONS {
        // code-instantiated: fixed upstream action row (four card buttons).
        let mut action_button = factory::glyph_button(icon, &kit::t(tooltip).to_string(), "emptyi");
        action_button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
        let target = dialog.to_gd();
        let state_for_action = state.clone();
        let action_name = action.to_string();
        let callable = Callable::from_fn("schematic_card_action", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            schematics_on_card_action(&mut bound, &state_for_action, index, &action_name);
        });
        action_button.connect("pressed", &callable);
        actions.add_child(&action_button);
    }
    column.add_child(&actions);
    // `b.stack(new SchematicImage(s), new Table(n -> { n.top(); black3 badge }))
    // .size(200f)`.
    let mut stack = Control::new_alloc();
    stack.set_custom_minimum_size(Vector2::new(200.0, 200.0));
    stack.set_mouse_filter(MouseFilter::IGNORE);
    let mut image = MindSchematicImage::new_alloc();
    {
        let mut image_control: Gd<Control> = image.clone().upcast();
        image_control.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    }
    image.bind_mut().setup(schematic.clone());
    stack.add_child(&image);
    let mut badge = PanelContainer::new_alloc();
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(styles::BLACK_3);
    box_.set_content_margin(Side::LEFT, 4.0);
    box_.set_content_margin(Side::RIGHT, 4.0);
    badge.add_theme_stylebox_override("panel", &box_);
    badge.set_anchors_preset(LayoutPreset::TOP_WIDE);
    badge.set_mouse_filter(MouseFilter::IGNORE);
    // `label.setEllipsis(true)`, `maxWidth(200f - 8f)`, centred, top.
    let mut name_label = Label::new_alloc();
    name_label.set_text(&gs(row_str(schematic, "name")));
    name_label.set_custom_minimum_size(Vector2::new(192.0, 0.0));
    name_label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
    name_label.set_text_overrun_behavior(OverrunBehavior::TRIM_ELLIPSIS);
    name_label.set_clip_text(true);
    name_label.set_mouse_filter(MouseFilter::IGNORE);
    badge.add_child(&name_label);
    stack.add_child(&badge);
    column.add_child(&stack);
    button.add_child(&column);
    {
        let image_for_enter = image.clone();
        let callable = Callable::from_fn("schematic_card_enter", move |_args: &[&Variant]| {
            let mut image = image_for_enter.clone();
            image.bind_mut().set_hovered(true);
        });
        button.connect("mouse_entered", &callable);
        let image_for_exit = image.clone();
        let callable = Callable::from_fn("schematic_card_exit", move |_args: &[&Variant]| {
            let mut image = image_for_exit.clone();
            image.bind_mut().set_hovered(false);
        });
        button.connect("mouse_exited", &callable);
    }
    button
}

/// Card press (`SchematicsDialog.java:190-205`): menu -> info; in game ->
/// `control.input.useSchematic(s)` + hide (or `@schematic.disabled`).
fn schematics_on_card_pressed(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
) {
    let _ = state;
    if index < 0 {
        return;
    }
    if !in_game() {
        schematics_open_info(index);
        return;
    }
    if !use_schematic(index) {
        let text = kit::t("@schematic.disabled").to_string();
        dialog.show_toast(&text);
        return;
    }
    close_dialog("schematics");
}

/// Card action row press: never falls through as a card press (the buttons stop
/// the event; Java `sel[0].childrenPressed()`).
fn schematics_on_card_action(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    index: i64,
    action: &str,
) {
    match action {
        "info" => schematics_open_info(index),
        "export" => schematics_export(dialog, state, index),
        "edit" => schematics_edit(dialog, state, index),
        "delete" => schematics_delete(dialog, state, index),
        _ => {}
    }
    dialog.base_mut().emit_signal(
        "schematic_action",
        &[index.to_variant(), gs(action).to_variant()],
    );
}

/// Enter in the search field (`Binding.chat`).
fn schematics_search_submit(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    let first_index = {
        let st = state.borrow();
        st.first_index
    };
    if first_index < 0 {
        return;
    }
    if !in_game() {
        schematics_open_info(first_index);
        return;
    }
    if !use_schematic(first_index) {
        let text = kit::t("@schematic.disabled").to_string();
        dialog.show_toast(&text);
        return;
    }
    close_dialog("schematics");
}

/// Runs a schematic mutation through the campaign facade when it exposes one
/// (Java mutates `schematics`, owned by `MindCampaign`). The port's facade has
/// no schematic mutators yet, so the caller falls back to rewriting the `.msch`
/// file itself (`Schematic.save`).
pub(super) fn campaign_schematic_mutation(
    dialog: &mut MindDialog,
    method: &str,
    args: &[Variant],
) -> Option<Variant> {
    dialog.campaign_call(method, args)
}

/// `showInfo(schematic)` -> the `schematic_info` dialog.
pub(super) fn schematics_open_info(index: i64) {
    let mut context = VarDictionary::new();
    context.set("index", &index.to_variant());
    open_dialog("schematic_info", &context);
}

/// `SchematicsDialog.showExport`: the `@editor.export` panel.
fn schematics_export(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>, index: i64) {
    schematics_show_export(dialog, state, index);
}

/// `SchematicsDialog.showImport`: the `@editor.import` panel.
fn schematics_import(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>) {
    schematics_show_import(dialog, state);
}

/// `@schematic.delete.confirm`; mod-provided schematics show the removal note
/// (`mod.item.remove`) instead of the confirm.
fn schematics_delete(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>, index: i64) {
    let Some(row) = schematic_row(dialog, index) else {
        return;
    };
    if schematic_is_mod(&row) {
        let text = kit::tm_format("@mod.item.remove", &[row_str(&row, "mod_name").as_str()]);
        dialog.show_toast(&text.to_string());
        return;
    }
    state.borrow_mut().pending_confirm = SchematicConfirm::DeleteSchematic(index);
    let _ = ui_call(
        "show_confirm",
        &[kit::t("@schematic.delete.confirm").to_variant()],
    );
}

/// `showEdit(s)`: the edit sub-dialog (name, description, tag chips).
fn schematics_edit(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>, index: i64) {
    schematics_show_edit(dialog, state, index);
}

fn schematics_on_confirm(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    confirmed: bool,
) {
    let pending = std::mem::take(&mut state.borrow_mut().pending_confirm);
    if !confirmed {
        return;
    }
    match pending {
        // `schematics.remove(s)`: drop the `.msch` (the read model reloads).
        SchematicConfirm::DeleteSchematic(index) => {
            if campaign_schematic_mutation(dialog, "delete_schematic", &[index.to_variant()])
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(false)
            {
                schematics_rebuild_list(dialog, state);
                return;
            }
            let Some(path) = schematic_file_of(dialog, index) else {
                return;
            };
            if NativeFs.delete(&path).is_ok() {
                schematics_rebuild_list(dialog, state);
            }
        }
        SchematicConfirm::DeleteTag(tag) => schematics_delete_tag(dialog, state, &tag),
        SchematicConfirm::None => {}
    }
}

/// `@schematic.tagdelconfirm`: strip the tag from every schematic, the filter
/// and the global list, then rebuild (`SchematicsDialog.java:566-580`).
fn schematics_delete_tag(dialog: &mut MindDialog, state: &Rc<RefCell<SchematicState>>, tag: &str) {
    for row in schematic_rows(dialog) {
        if schematic_is_mod(&row) {
            continue;
        }
        let carries = row_arr(&row, "labels")
            .iter_shared()
            .any(|label| to_text_godot(&label) == tag);
        if !carries {
            continue;
        }
        let file = row_str(&row, "file");
        if file.is_empty() {
            continue;
        }
        let path = PathBuf::from(file);
        let _ = schematic_save_file(&path, |schematic| {
            schematic_labels_remove(&mut schematic.labels, tag);
        });
    }
    {
        let mut st = state.borrow_mut();
        st.selected_tags.retain(|value| value != tag);
        st.tags.retain(|value| value != tag);
    }
    schematics_tags_changed(dialog, state);
    schematics_rebuild_list(dialog, state);
    if state.borrow().overlay_kind == SchematicOverlay::Tags {
        schematics_show_all_tags(dialog, state);
    }
}

fn schematics_on_text_input(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    text: &str,
) {
    let pending = std::mem::take(&mut state.borrow_mut().pending_text);
    match pending {
        SchematicTextPrompt::NewTag => schematics_new_tag_result(dialog, state, text),
        SchematicTextPrompt::RenameTag(old) => {
            schematics_rename_tag_result(dialog, state, &old, text)
        }
        SchematicTextPrompt::None => {}
    }
}

/// `showNewTag` result: `tags.add(out); tagsChanged(); result.get(out)`.
fn schematics_new_tag_result(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    text: &str,
) {
    // An empty result is a cancelled prompt (the port's `showTextInput`).
    if text.is_empty() {
        return;
    }
    let target = state.borrow().tag_target;
    if !schematics_add_global_tag(dialog, state, text) {
        let exists = kit::t("@schematic.tagexists").to_string();
        dialog.show_toast(&exists);
        return;
    }
    if target >= 0 {
        schematic_set_tag(dialog, state, target, text, true);
        schematics_show_edit(dialog, state, target);
    } else if state.borrow().overlay_kind == SchematicOverlay::Tags {
        schematics_show_all_tags(dialog, state);
    }
}

/// Tag rename result: `s.labels.replace(tag, result)` on every schematic,
/// `selectedTags.replace`, `tags.replace` (`SchematicsDialog.java:544-564`).
fn schematics_rename_tag_result(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SchematicState>>,
    old: &str,
    new: &str,
) {
    if new.is_empty() || new == old {
        return;
    }
    if state.borrow().tags.iter().any(|tag| tag == new) {
        let exists = kit::t("@schematic.tagexists").to_string();
        dialog.show_toast(&exists);
        return;
    }
    for row in schematic_rows(dialog) {
        if schematic_is_mod(&row) {
            continue;
        }
        let carries = row_arr(&row, "labels")
            .iter_shared()
            .any(|label| to_text_godot(&label) == old);
        if !carries {
            continue;
        }
        let file = row_str(&row, "file");
        if file.is_empty() {
            continue;
        }
        let path = PathBuf::from(file);
        let _ = schematic_save_file(&path, |schematic| {
            schematic_labels_replace(&mut schematic.labels, old, new);
        });
    }
    {
        let mut st = state.borrow_mut();
        if let Some(position) = st.tags.iter().position(|tag| tag == old) {
            st.tags[position] = new.to_owned();
        }
        if let Some(value) = st.selected_tags.iter_mut().find(|tag| *tag == old) {
            *value = new.to_owned();
        }
    }
    schematics_tags_changed(dialog, state);
    schematics_rebuild_list(dialog, state);
    if state.borrow().overlay_kind == SchematicOverlay::Tags {
        schematics_show_all_tags(dialog, state);
    }
}

/// `SchematicsDialog.ignoreSymbols`: strip the symbol class from a name.
fn schematics_strip_symbols(value: &str) -> String {
    value
        .chars()
        .filter(|character| !IGNORE_SYMBOLS.contains(*character))
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn symbol_strip_matches_ignore_symbols_class() {
        assert_eq!(schematics_strip_symbols("Copper Wall!"), "Copper Wall");
        assert_eq!(schematics_strip_symbols("a-b_c"), "abc");
        assert_eq!(schematics_strip_symbols("keep spaces"), "keep spaces");
    }

    #[test]
    fn tag_markup_is_split_into_plain_text_and_color() {
        // Godot-FFI-backed lookups/types need the engine; plain `cargo test`
        // runs without it (the headless oracles cover the engine path).
        if !godot::sys::is_godot_initialized() {
            return;
        }
        assert_eq!(
            schematics_tag_plain("[magenta]advanced-logic"),
            "advanced-logic"
        );
        assert_eq!(schematics_tag_plain("plain"), "plain");
        assert_eq!(
            schematics_tag_color("[#ff00ff]x"),
            Some(Color::from_rgba(1.0, 0.0, 1.0, 1.0))
        );
        assert_eq!(schematics_tag_color("plain"), None);
    }
}
