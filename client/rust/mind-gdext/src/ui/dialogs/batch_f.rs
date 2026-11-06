// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/editor/MapProcessorsDialog.java,
//         core/src/mindustry/editor/MapLoadDialog.java,
//         core/src/mindustry/editor/SectorGenerateDialog.java,
//         core/src/mindustry/editor/BannedContentDialog.java,
//         core/src/mindustry/editor/data/MapAssetsDialog.java (+ `MapPatchesView`,
//         `MapContentView`, `MapBundlesView`, `MapImagesView`, `MapAudioView`,
//         `AssetView`), core/src/mindustry/ui/dialogs/SchematicsDialog.java
//         (`SchematicInfoDialog`, :774-834).

//! Editor data dialogs and the schematic info page.
//!
//! Each controller mirrors one `client/scenes/editor/**/*.gd` (or
//! `client/ui/dialogs/schematic_info_dialog.gd`) script: the map-processor list,
//! the map loader, the sector generator, the banned-content picker, the data
//! asset manager and the schematic info page. Behaviour (editor map state, the
//! data-asset owner, the banned-set model) stays in `MindEditor` /
//! `MindCampaign`; the controllers only read their JSON/array projections and
//! call the same dynamic method names the GDScript used.
//!
//! Prompts (`MindUi.show_text_input`/`show_confirm`) and the `icon_select`
//! signal are answered asynchronously; those handlers queue their work through
//! [`defer_action`] because a prompt is emitted from inside a bound `MindUi`
//! method and re-entering a bound GDExtension object panics (`PAR-0066`).

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::file_dialog::{Access, FileMode};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    AcceptDialog, BoxContainer, Button, ButtonGroup, CenterContainer, ColorRect, Control,
    DisplayServer, FileAccess, FileDialog, GridContainer, HBoxContainer, Json, LineEdit,
    MarginContainer, Os, PanelContainer, RichTextLabel, StyleBoxFlat, Texture2D, TextureRect,
    VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindScroll, MindTable, styles};

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_processors" => Some(Box::new(MapProcessorsDialog::default())),
        "map_load" => Some(Box::new(MapLoadDialog::default())),
        "sector_generate" => Some(Box::new(SectorGenerateDialog::default())),
        "banned_content" => Some(Box::new(BannedContentDialog::default())),
        "map_assets" => Some(Box::new(MapAssetsDialog::default())),
        "schematic_info" => Some(Box::new(SchematicInfoDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// A queued one-shot action (`Callable.call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase (see the module docs).
fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
    let slot: DeferredAction = Rc::new(RefCell::new(Some(Box::new(action))));
    let callable = Callable::from_fn(name, move |_args: &[&Variant]| {
        let queued = slot.borrow_mut().take();
        if let Some(action) = queued {
            action();
        }
        Variant::nil()
    });
    callable.call_deferred(&[]);
}

/// Calls a `MindUi` facade method (`None` when the facade/method is absent).
fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// Calls a `MindEditor` facade method (`None` when the facade/method is absent).
fn editor_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut editor = kit::editor()?;
    if !editor.has_method(method) {
        return None;
    }
    Some(editor.call(method, args))
}

/// `Vars.mobile`-independent portrait test (`Core.graphics.isPortrait()`).
fn is_portrait(dialog: &MindDialog) -> bool {
    let size = dialog.base().get_viewport_rect().size;
    size.y > size.x
}

/// The viewport width (`get_viewport_rect().size.x`).
fn viewport_width(dialog: &MindDialog) -> f32 {
    dialog.base().get_viewport_rect().size.x
}

/// A rich label's engine handle (text/color/alignment/autowrap).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// `MindWidgets.markup(text)` for a raw markup string.
fn markup(raw: &str) -> String {
    kit::markup_text(raw).to_string()
}

/// `UiRoot.dialog(name)` on the `UiRoot` registry (falls back to the tree).
fn dialog_node(name: &str) -> Option<Gd<Node>> {
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("dialog")
        && let Ok(node) = root
            .call("dialog", &[GString::from(name).to_variant()])
            .try_to::<Gd<Node>>()
    {
        return Some(node);
    }
    kit::tree_root()?.find_child(name)
}

/// Connects a `MindUi` prompt signal once.
fn connect_prompt_once(dialog: &mut MindDialog, key: &str, signal: &str, callable: &Callable) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    let meta = format!("_batch_f_{key}");
    if dialog.base().has_meta(&meta) || !ui.has_signal(signal) {
        return;
    }
    let _ = ui.connect(signal, callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// Re-runs the active controller's `shown` hook on a live dialog handle.
fn dialog_refresh(dialog: &Gd<MindDialog>) {
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    node.bind_mut().refresh_controller();
}

/// Removes and frees every child of `node`.
fn clear_children<T: Inherits<Node>>(node: &mut Gd<T>) {
    let mut parent: Gd<Node> = node.clone().upcast();
    for child in parent.get_children().iter_shared() {
        let mut child = child.clone();
        parent.remove_child(&child);
        child.queue_free();
    }
}

/// `str(value)` for one variant (empty when unconvertible).
fn variant_string(value: &Variant) -> String {
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string();
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return flag.to_string();
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number.to_string();
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number.to_string();
    }
    String::new()
}

/// `int(value)` for one variant (JSON integers and floats both convert).
fn variant_i64(value: &Variant) -> i64 {
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return i64::from(flag);
    }
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string().trim().parse().unwrap_or(0);
    }
    0
}

/// `float(value)` for one variant.
fn variant_f64(value: &Variant) -> f64 {
    if let Ok(number) = value.try_to::<f64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number as f64;
    }
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string().trim().parse().unwrap_or(0.0);
    }
    0.0
}

/// `bool(value)` for one variant.
fn variant_bool(value: &Variant) -> bool {
    if let Ok(flag) = value.try_to::<bool>() {
        return flag;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number != 0;
    }
    if let Ok(text) = value.try_to::<GString>() {
        let text = text.to_string();
        return text == "true" || text == "1";
    }
    false
}

/// A dictionary value coerced to text (`str(dict.get(key, ""))`).
fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_string(&value))
        .unwrap_or_default()
}

/// A dictionary value coerced to an integer.
fn dict_i64(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced to a float.
fn dict_f64(dict: &VarDictionary, key: &str, default: f64) -> f64 {
    dict.get(key)
        .map(|value| variant_f64(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced to a boolean.
fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .map(|value| variant_bool(&value))
        .unwrap_or(default)
}

/// `JSON.parse_string(text)` as an array (`[]` on any other shape).
fn parse_json_array(text: &str) -> VarArray {
    if text.is_empty() {
        return VarArray::new();
    }
    Json::parse_string(text)
        .try_to::<VarArray>()
        .unwrap_or_default()
}

/// `JSON.parse_string(text)` as a dictionary (`{}` on any other shape).
fn parse_json_object(text: &str) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(text)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `JSON.stringify(value)` for one variant.
fn stringify(value: &Variant) -> String {
    Json::stringify(value).to_string()
}

/// An array of strings as a Godot array (`Array[String]` writes).
fn string_array_variant(values: &[String]) -> Variant {
    let mut array = VarArray::new();
    for value in values {
        array.push(&GString::from(value.as_str()).to_variant());
    }
    array.to_variant()
}

/// `String.is_valid_int()`: an optional sign followed by digits only.
fn is_valid_int(text: &str) -> bool {
    let body = text.strip_prefix('-').unwrap_or(text);
    !body.is_empty() && body.chars().all(|character| character.is_ascii_digit())
}

/// A flat `StyleBoxFlat` with a solid fill (`MindThemeBuilder.flat`).
fn flat_box(fill: Color) -> Gd<StyleBoxFlat> {
    // code-instantiated: the theme builder's `flat` helper is private; the
    // gray data cards need the same look at runtime.
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(fill);
    box_.set_corner_radius_all(3);
    box_.set_content_margin(Side::LEFT, 8.0);
    box_.set_content_margin(Side::RIGHT, 8.0);
    box_.set_content_margin(Side::TOP, 4.0);
    box_.set_content_margin(Side::BOTTOM, 4.0);
    box_
}

/// A flat 50x50 `graySquarei` action cell (`_square`).
fn square_cell(icon_name: &str, tooltip: &str, handler: Callable) -> Gd<Button> {
    // code-instantiated: per-row action cells are data-driven.
    let mut button = factory::icon_button(
        icon_name,
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    button.set_tooltip_text(tooltip);
    button.connect("pressed", &handler);
    button
}

/// Shows a transient error toast (`ui.toast(text, "error")`).
fn toast_error(text: &str) {
    let _ = ui_call(
        "toast",
        &[
            GString::from(text).to_variant(),
            GString::from("error").to_variant(),
        ],
    );
}

/// `_notify(key)`: the bundle lookup as an error toast.
fn notify_key(key: &str) {
    toast_error(&kit::t(key).to_string());
}

/// `_show_text(title, body)`: the shared text prompt with a toast fallback.
fn show_text(title: &str, body: &str) {
    if ui_call(
        "show_text",
        &[
            GString::from(title).to_variant(),
            GString::from(body).to_variant(),
        ],
    )
    .is_some()
    {
        return;
    }
    show_toast(body);
}

/// `MindDialog.show_toast`: a transient info prompt through `MindUi`.
fn show_toast(text: &str) {
    if text.is_empty() {
        return;
    }
    let _ = ui_call("show_info", &[GString::from(text).to_variant()]);
}

// ---------------------------------------------------------------------------
// MapProcessorsDialog (`map_processors`)
// ---------------------------------------------------------------------------

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
        dialog.add_close_button(210.0);
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
        let mut search_row = HBoxContainer::new_alloc();
        search_row.add_theme_constant_override("separation", 8);
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
                .map(|value| variant_bool(value))
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
    state.borrow_mut().pending_delete = (dict_i64(entry, "x", 0), dict_i64(entry, "y", 0));
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
    let tag = dict_string(entry, "tag");
    state.borrow_mut().pending_rename = (dict_i64(entry, "x", 0), dict_i64(entry, "y", 0));
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
    state.borrow_mut().pending_icon = (dict_i64(entry, "x", 0), dict_i64(entry, "y", 0));
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
    context.set(&"x".to_variant(), &dict_i64(entry, "x", 0).to_variant());
    context.set(&"y".to_variant(), &dict_i64(entry, "y", 0).to_variant());
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
        .map(|value| variant_bool(&value))
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
    clear_children(&mut list);
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
        let tag = dict_string(&entry, "tag");
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
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 4);
    row.set_custom_minimum_size(Vector2::new(0.0, PROCESSOR_ROW_HEIGHT + 8.0));

    // `t.button(log.iconTag == 0 ? Styles.none : ..., Styles.graySquarei, ...)`.
    // code-instantiated: the icon cell holds a glyph char or nothing.
    let mut icon_button = Button::new_alloc();
    icon_button.set_theme_type_variation("graySquarei");
    icon_button.set_custom_minimum_size(Vector2::new(PROCESSOR_ROW_HEIGHT, PROCESSOR_ROW_HEIGHT));
    let icon_code = dict_i64(entry, "icon_tag", 0);
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
    let tag = dict_string(entry, "tag");
    let head = if tag.is_empty() {
        String::from("<no name>")
    } else {
        format!("[accent]{tag}")
    };
    markup(&format!(
        "{head}\n[lightgray][[{}, {}]",
        dict_i64(entry, "x", 0),
        dict_i64(entry, "y", 0)
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
    .map(|value| variant_bool(&value))
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
    .map(|value| variant_bool(&value))
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
    .map(|value| variant_bool(&value))
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

// ---------------------------------------------------------------------------
// MapLoadDialog (`map_load`)
// ---------------------------------------------------------------------------

/// `table.defaults().size(250f, 90f).pad(4f)`.
const MAP_BUTTON_WIDTH: f32 = 250.0;
const MAP_BUTTON_HEIGHT: f32 = 90.0;
/// `new BorderImage(map.safeTexture(), 2f)` at `size(16 * 4f)`.
const MAP_PREVIEW_SIZE: f32 = 64.0;

/// The `map_load` view state.
#[derive(Default)]
struct MapLoadState {
    /// The picked map path (`_selected`).
    selected: String,
    /// The `@load` button, disabled until a map is picked.
    load_button: Option<Gd<Button>>,
    /// The `squareTogglet` radio group.
    group: Option<Gd<ButtonGroup>>,
}

/// `@editor.loadmap` — the map picker grid.
#[derive(Default)]
pub struct MapLoadDialog {
    state: Rc<RefCell<MapLoadState>>,
}

impl DialogController for MapLoadDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        map_load_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        map_load_shown(dialog, &self.state);
    }
}

fn map_load_ready(dialog: &mut MindDialog, state: &Rc<RefCell<MapLoadState>>) {
    dialog.set_title_key("@editor.loadmap");
    // `makeButtonOverlay()` (`MapLoadDialog.java:34`): the padded title cell.
    dialog.apply_button_overlay(true);
    dialog.clear_buttons();
    state.borrow_mut().group = Some(ButtonGroup::new_gd());

    // `buttons.defaults().size(210f, 64f)`: Cancel (Icon.cancel), Load (Icon.ok).
    let hide = dialog.to_gd().callable("hide_dialog");
    let cancel_text = kit::t("@cancel").to_string();
    let mut cancel = dialog.add_button(&cancel_text, Some(hide), "cancel", 210.0);
    let mut size = cancel.get_custom_minimum_size();
    size.y = 64.0;
    cancel.set_custom_minimum_size(size);

    let dialog_gd = dialog.to_gd();
    let state_for_load = state.clone();
    let load = Callable::from_fn("map_load_selected", move |_args: &[&Variant]| {
        map_load_selected(&dialog_gd, &state_for_load);
    });
    let load_text = kit::t("@load").to_string();
    let mut load_button = dialog.add_button(&load_text, Some(load), "ok", 210.0);
    let mut size = load_button.get_custom_minimum_size();
    size.y = 64.0;
    load_button.set_custom_minimum_size(size);
    load_button.set_disabled(true);
    state.borrow_mut().load_button = Some(load_button);
}

fn map_load_shown(dialog: &mut MindDialog, state: &Rc<RefCell<MapLoadState>>) {
    {
        let mut state = state.borrow_mut();
        state.selected.clear();
        if let Some(mut load) = state.load_button.clone() {
            load.set_disabled(true);
        }
    }
    map_load_build(dialog, state);
}

/// `rebuild()` (`MapLoadDialog.java:35-71`).
fn map_load_build(dialog: &mut MindDialog, state: &Rc<RefCell<MapLoadState>>) {
    dialog.clear_content();
    let mut root = dialog.content_table();
    let maps = map_rows();
    if maps.is_empty() {
        let label = factory::label(&kit::t("@maps.none").to_string());
        root.bind_mut().add(&label);
        return;
    }
    let label = factory::label(&kit::t("@editor.selectmap").to_string());
    root.bind_mut().add(&label);
    root.bind_mut().row();

    // `ScrollPane(table)`, X scrolling off; `table.margin(10f)`.
    // code-instantiated: the map grid is data-driven (the map registry).
    let mut scroll = MindScroll::new_alloc();
    scroll.bind_mut().set_scrolling_disabled(true, false);
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    root.bind_mut().add(&scroll).grow_y();
    let mut margin = MarginContainer::new_alloc();
    for side in ["left", "top", "right", "bottom"] {
        margin.add_theme_constant_override(&format!("margin_{side}"), 10);
    }
    scroll.add_child(&margin);
    let mut grid = GridContainer::new_alloc();
    // `cols = max(width / 300, 1)`.
    let columns = ((viewport_width(dialog) / 300.0) as i32).max(1);
    grid.set_columns(columns);
    grid.add_theme_constant_override("h_separation", 8);
    grid.add_theme_constant_override("v_separation", 8);
    margin.add_child(&grid);
    for map in &maps {
        let button = map_load_button(state, map);
        grid.add_child(&button);
    }
    // The grid's full width (columns x 258 + margins) fixes the pane width.
    scroll.set_custom_minimum_size(Vector2::new(
        columns as f32 * (MAP_BUTTON_WIDTH + 8.0) + 12.0,
        0.0,
    ));
    root.bind_mut().row();
}

/// `maps.all()`: the registry rows, custom maps first.
fn map_rows() -> Vec<VarDictionary> {
    let Some(preview) = kit::node("/root/Spine/MindPreview") else {
        return Vec::new();
    };
    let mut preview = preview;
    if !preview.has_method("maps_list") {
        return Vec::new();
    }
    let value = preview.call("maps_list", &[]);
    let Ok(rows) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    let mut custom: Vec<VarDictionary> = Vec::new();
    let mut builtin: Vec<VarDictionary> = Vec::new();
    for row in rows.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        if dict_string(&row, "name").is_empty() {
            continue;
        }
        if dict_bool(&row, "custom", false) {
            custom.push(row);
        } else {
            builtin.push(row);
        }
    }
    custom.append(&mut builtin);
    custom
}

/// `_preview_texture(map_name)`.
fn map_preview_texture(map_name: &str) -> Option<Gd<Texture2D>> {
    let mut preview = kit::node("/root/Spine/MindPreview")?;
    if !preview.has_method("texture_for_name") {
        return None;
    }
    preview
        .call("texture_for_name", &[GString::from(map_name).to_variant()])
        .try_to::<Gd<Texture2D>>()
        .ok()
}

/// One `squareTogglet` map button: preview (padLeft 5, 64 px) + name.
fn map_load_button(state: &Rc<RefCell<MapLoadState>>, map: &VarDictionary) -> Gd<Button> {
    // code-instantiated: one map card per registry row.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("squareTogglet");
    button.set_toggle_mode(true);
    if let Some(group) = state.borrow().group.clone() {
        button.set_button_group(&group);
    }
    button.set_custom_minimum_size(Vector2::new(MAP_BUTTON_WIDTH, MAP_BUTTON_HEIGHT));
    let path = dict_string(map, "path");
    let state_for_pick = state.clone();
    let path_for_press = path.clone();
    let callable = Callable::from_fn("map_pick", move |_args: &[&Variant]| {
        state_for_pick.borrow_mut().selected = path_for_press.clone();
        let disabled = path_for_press.is_empty();
        if let Some(mut load) = state_for_pick.borrow().load_button.clone() {
            load.set_disabled(disabled);
        }
    });
    button.connect("pressed", &callable);

    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 8.0 + 5.0);
    row.set_offset(Side::RIGHT, -8.0);
    row.set_offset(Side::TOP, 8.0);
    row.set_offset(Side::BOTTOM, -8.0);
    row.add_theme_constant_override("separation", 5);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);

    // `BorderImage(texture, 2f)`: a 2 px `Pal.gray` frame around the preview.
    let mut frame = PanelContainer::new_alloc();
    // code-instantiated: the 2 px preview frame (a per-cell drawable).
    let mut border = StyleBoxFlat::new_gd();
    border.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    border.set_border_color(Color::from_rgba8(0x45, 0x45, 0x45, 0xff));
    border.set_border_width_all(2);
    border.set_content_margin_all(2.0);
    frame.add_theme_stylebox_override("panel", &border);
    frame.set_custom_minimum_size(Vector2::new(MAP_PREVIEW_SIZE, MAP_PREVIEW_SIZE));
    frame.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    frame.set_mouse_filter(MouseFilter::IGNORE);
    let mut image = TextureRect::new_alloc();
    if let Some(texture) = map_preview_texture(&dict_string(map, "name")) {
        image.set_texture(&texture);
    }
    // `map.safeTexture()` keeps Arc's nearest filtering: crisp pixels.
    image.set_texture_filter(godot::classes::canvas_item::TextureFilter::NEAREST);
    image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    image.set_mouse_filter(MouseFilter::IGNORE);
    frame.add_child(&image);
    row.add_child(&frame);

    let display = {
        let display = dict_string(map, "display_name");
        let name = dict_string(map, "name");
        if display.is_empty() { name } else { display }
    };
    let mut map_label = factory::label(&display);
    map_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
    map_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    map_label.set_vertical_alignment(VerticalAlignment::CENTER);
    map_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    map_label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    map_label.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&map_label);
    button
}

/// `_load_selected`: `editor.begin_edit_map(path)` then hide.
fn map_load_selected(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapLoadState>>) {
    let selected = state.borrow().selected.clone();
    if selected.is_empty() {
        return;
    }
    let _ = editor_call(
        "begin_edit_map",
        &[GString::from(selected.as_str()).to_variant()],
    );
    if dialog.is_instance_valid() {
        let mut node = dialog.clone();
        node.bind_mut().close();
    }
}

// ---------------------------------------------------------------------------
// SectorGenerateDialog (`sector_generate`)
// ---------------------------------------------------------------------------

/// The `sector_generate` view state.
struct SectorGenerateState {
    planet: String,
    sector: i64,
    seed: i64,
    planet_button: Option<Gd<Button>>,
    sector_field: Option<Gd<LineEdit>>,
    seed_field: Option<Gd<LineEdit>>,
    size_label: Option<Gd<MindLabel>>,
}

impl Default for SectorGenerateState {
    fn default() -> Self {
        Self {
            planet: String::from("erekir"),
            sector: 0,
            seed: 0,
            planet_button: None,
            sector_field: None,
            seed_field: None,
            size_label: None,
        }
    }
}

/// `@editor.sectorgenerate` — the sector/seed generator form.
#[derive(Default)]
pub struct SectorGenerateDialog {
    state: Rc<RefCell<SectorGenerateState>>,
}

impl DialogController for SectorGenerateDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.sectorgenerate");
        dialog.should_pause = false;
        dialog.clear_buttons();
        // `addCloseButton()` + `buttons.button("@editor.apply", Icon.ok, ...)`.
        dialog.add_close_button(210.0);
        let dialog_gd = dialog.to_gd();
        let state = self.state.clone();
        let apply = Callable::from_fn("sector_generate_apply", move |_args: &[&Variant]| {
            sector_generate_apply(&dialog_gd, &state);
        });
        let text = kit::t("@editor.apply").to_string();
        dialog.add_button(&text, Some(apply), "ok", 210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        sector_generate_build(dialog, &self.state);
    }
}

/// `setup()` (`:23-85`).
fn sector_generate_build(dialog: &mut MindDialog, state: &Rc<RefCell<SectorGenerateState>>) {
    dialog.clear_content();
    let mut root = dialog.content_table();

    // `cont.add("@editor.planet"); cont.button(planet.localizedName, ...)`.
    let planet_button = sector_generate_planet_picker(dialog, state);
    let planet_control: Gd<Control> = planet_button.clone().upcast();
    let row = sector_generate_row(&kit::t("@editor.planet").to_string(), &planet_control);
    root.bind_mut().add(&row).pad(4.0);
    root.bind_mut().row();

    // `cont.field(sector + "", ...).valid(...)`.
    let mut sector_field = factory::field("");
    sector_field.set_text(&GString::from(state.borrow().sector.to_string().as_str()));
    sector_field.set_custom_minimum_size(Vector2::new(200.0, 0.0));
    let dialog_gd = dialog.to_gd();
    let state_for_sector = state.clone();
    let callable = Callable::from_fn("sector_field_changed", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        sector_generate_on_sector_changed(&dialog_gd, &state_for_sector, &text);
    });
    sector_field.connect("text_changed", &callable);
    let sector_control: Gd<Control> = sector_field.clone().upcast();
    let row = sector_generate_row(&kit::t("@editor.sector").to_string(), &sector_control);
    root.bind_mut().add(&row).pad(4.0);
    root.bind_mut().row();
    state.borrow_mut().sector_field = Some(sector_field);

    // `cont.field(seed + "", ...).valid(Strings::canParseInt)`.
    let mut seed_field = factory::field("");
    seed_field.set_text(&GString::from(state.borrow().seed.to_string().as_str()));
    seed_field.set_custom_minimum_size(Vector2::new(200.0, 0.0));
    let state_for_seed = state.clone();
    let callable = Callable::from_fn("seed_field_changed", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        if !text.is_empty() && text != "-" && is_valid_int(&text) {
            state_for_seed.borrow_mut().seed = text.parse().unwrap_or(0);
        }
    });
    seed_field.connect("text_changed", &callable);
    let seed_control: Gd<Control> = seed_field.clone().upcast();
    let row = sector_generate_row(&kit::t("@editor.seed").to_string(), &seed_control);
    root.bind_mut().add(&row).pad(4.0);
    root.bind_mut().row();
    state.borrow_mut().seed_field = Some(seed_field);

    // `cont.label(() -> "[ " + size + "x" + size + " ]").color(Pal.accent)`.
    let mut size_label = factory::label("");
    size_label.add_theme_color_override("font_color", styles::ACCENT);
    size_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    root.bind_mut().add(&size_label).pad(5.0);
    state.borrow_mut().size_label = Some(size_label);
    sector_generate_refresh_size(state);
}

/// One dialog row (`planet`/`sector`/`seed`), data-driven.
fn sector_generate_row(label_text: &str, control: &Gd<Control>) -> Gd<HBoxContainer> {
    // code-instantiated: one labelled form row (planet/sector/seed).
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 10);
    let label = factory::label(label_text);
    row.add_child(&label);
    row.add_child(control);
    row
}

/// `cont.button(planet.localizedName, ...)`: the planet-picker entry button.
fn sector_generate_planet_picker(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SectorGenerateState>>,
) -> Gd<Button> {
    let mut button = factory::button(&sector_generate_planet_name(state));
    button.set_custom_minimum_size(Vector2::new(200.0, 40.0));
    let dialog_gd = dialog.to_gd();
    let state_for = state.clone();
    let callable = Callable::from_fn("sector_planet_picker", move |_args: &[&Variant]| {
        if !dialog_gd.is_instance_valid() {
            return;
        }
        let mut node = dialog_gd.clone();
        let mut bound = node.bind_mut();
        sector_generate_open_picker(&mut bound, &state_for);
    });
    button.connect("pressed", &callable);
    state.borrow_mut().planet_button = Some(button.clone());
    button
}

/// `_planet_label(_planet)`: `planet.<name>.name` from the bundle.
fn sector_generate_planet_name(state: &Rc<RefCell<SectorGenerateState>>) -> String {
    let planet = state.borrow().planet.clone();
    planet_label(&planet)
}

/// `_planet_label(planet)`: `MindAssets.bundle_get("planet.<name>.name")`.
fn planet_label(planet: &str) -> String {
    let Some(assets) = kit::assets() else {
        return String::from(planet);
    };
    let key = format!("planet.{planet}.name");
    let value = assets.bind().bundle_get(GString::from(key.as_str()));
    let value = value.to_string();
    if value.is_empty() || value == key {
        return String::from(planet);
    }
    value
}

/// `_open_planet_picker`: the `BaseDialog` planet list (:33-57) as a local
/// overlay with 4 toggle columns.
fn sector_generate_open_picker(dialog: &mut MindDialog, state: &Rc<RefCell<SectorGenerateState>>) {
    let Some(value) = editor_call("sector_planets_json", &[]) else {
        return;
    };
    let planets = parse_json_array(&value.to::<GString>().to_string());
    if planets.is_empty() {
        return;
    }
    let current = state.borrow().planet.clone();
    // code-instantiated: per-action picker panels have no static scene.
    let mut overlay = ColorRect::new_alloc();
    overlay.set_name("PlanetPicker");
    overlay.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.8));
    overlay.set_anchors_preset(LayoutPreset::FULL_RECT);
    overlay.set_mouse_filter(MouseFilter::STOP);
    let overlay_control: Gd<Control> = overlay.clone().upcast();
    dialog.base_mut().add_child(&overlay_control);

    let mut center = CenterContainer::new_alloc();
    center.set_anchors_preset(LayoutPreset::FULL_RECT);
    center.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&center);
    let mut panel = PanelContainer::new_alloc();
    panel.set_theme_type_variation("defaultDialog");
    center.add_child(&panel);
    let mut box_ = VBoxContainer::new_alloc();
    panel.add_child(&box_);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(4);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    box_.add_child(&grid);

    for value in planets.iter_shared() {
        let Ok(row) = value.try_to::<VarDictionary>() else {
            continue;
        };
        let planet_name = dict_string(&row, "name");
        let mut button = factory::button(&planet_label(&planet_name));
        button.set_theme_type_variation("flatTogglet");
        button.set_custom_minimum_size(Vector2::new(110.0, 45.0));
        button.set_toggle_mode(true);
        button.set_pressed(planet_name == current);
        let dialog_gd = dialog.to_gd();
        let state_for = state.clone();
        let overlay_for_pick: Gd<Control> = overlay_control.clone();
        let name_for_pick = planet_name.clone();
        let callable = Callable::from_fn("sector_pick_planet", move |_args: &[&Variant]| {
            let mut overlay = overlay_for_pick.clone();
            overlay.queue_free();
            if !dialog_gd.is_instance_valid() {
                return;
            }
            let mut node = dialog_gd.clone();
            let mut bound = node.bind_mut();
            sector_generate_select_planet(&mut bound, &state_for, &name_for_pick);
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
    let mut close = factory::button(&kit::t("@back").to_string());
    let overlay_for_close: Gd<Control> = overlay_control.clone();
    let callable = Callable::from_fn("sector_picker_close", move |_args: &[&Variant]| {
        let mut overlay = overlay_for_close.clone();
        overlay.queue_free();
    });
    close.connect("pressed", &callable);
    box_.add_child(&close);
}

/// `planet = plan; sector = min(sector, sectors.size - 1); seed = 0`.
fn sector_generate_select_planet(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<SectorGenerateState>>,
    planet_name: &str,
) {
    {
        let mut state = state.borrow_mut();
        state.planet = String::from(planet_name);
        let sectors = sector_planet_sector_count(&state.planet);
        if sectors > 0 {
            state.sector = state.sector.min(sectors - 1);
        }
        state.seed = 0;
    }
    if let Some(mut button) = state.borrow().planet_button.clone() {
        button.set_text(&sector_generate_planet_name(state));
    }
    sector_generate_build(dialog, state);
}

/// `valid(text -> planet.sectors.size > parseInt(text, 99999) && ...)`.
fn sector_generate_on_sector_changed(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<SectorGenerateState>>,
    text: &str,
) {
    if !is_valid_int(text) {
        return;
    }
    let value: i64 = text.parse().unwrap_or(0);
    let sectors = sector_planet_sector_count(&state.borrow().planet);
    if value < 0 || (sectors > 0 && value >= sectors) {
        return;
    }
    state.borrow_mut().sector = value;
    sector_generate_refresh_size(state);
    let _ = dialog;
}

/// `_refresh_size`: `editor.sector_size(planet, sector)` into `[ NxN ]`.
fn sector_generate_refresh_size(state: &Rc<RefCell<SectorGenerateState>>) {
    let Some(mut label) = state.borrow().size_label.clone() else {
        return;
    };
    let (planet, sector) = {
        let state = state.borrow();
        (state.planet.clone(), state.sector)
    };
    let size = editor_call(
        "sector_size",
        &[
            GString::from(planet.as_str()).to_variant(),
            sector.to_variant(),
        ],
    )
    .map(|value| variant_i64(&value))
    .unwrap_or(0);
    label.set_text(&GString::from(format!("[ {size}x{size} ]").as_str()));
}

/// `_apply`: `ui.loadAnd(() -> { apply(); hide(); })`.
fn sector_generate_apply(dialog: &Gd<MindDialog>, state: &Rc<RefCell<SectorGenerateState>>) {
    let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot") else {
        sector_generate_apply_now(dialog, state);
        return;
    };
    if !root.has_method("load_and") {
        sector_generate_apply_now(dialog, state);
        return;
    }
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("sector_generate_now", move |_args: &[&Variant]| {
        sector_generate_apply_now(&dialog_for, &state_for);
        Variant::nil()
    });
    let _ = root.call("load_and", &[callable.to_variant()]);
}

/// `_apply_now`: `editor.sector_generate(planet, sector, seed)`, then close.
fn sector_generate_apply_now(dialog: &Gd<MindDialog>, state: &Rc<RefCell<SectorGenerateState>>) {
    let (planet, sector, seed) = {
        let state = state.borrow();
        (state.planet.clone(), state.sector, state.seed)
    };
    let generated = editor_call(
        "sector_generate",
        &[
            GString::from(planet.as_str()).to_variant(),
            sector.to_variant(),
            seed.to_variant(),
        ],
    )
    .map(|value| variant_bool(&value))
    .unwrap_or(false);
    if !generated {
        notify_key("@editor.sectorgenerate");
        return;
    }
    if dialog.is_instance_valid() {
        let mut node = dialog.clone();
        node.bind_mut().close();
    }
}

/// `_planet_sector_count`: `{name, sectors}` from `sector_planets_json`.
fn sector_planet_sector_count(planet: &str) -> i64 {
    let Some(value) = editor_call("sector_planets_json", &[]) else {
        return 0;
    };
    let planets = parse_json_array(&value.to::<GString>().to_string());
    for row in planets.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        if dict_string(&row, "name") == planet {
            return dict_i64(&row, "sectors", 0);
        }
    }
    0
}

// ---------------------------------------------------------------------------
// BannedContentDialog (`banned_content`)
// ---------------------------------------------------------------------------

/// `BannedContentDialog.rebuildTable`: 50px cells, 32px icons.
const BANNED_CELL: f32 = 50.0;
const BANNED_ICON: i32 = 32;
/// `TabHoverColor` / Java `Color.valueOf("f25555")` banned accent.
const BANNED_ACCENT: Color = Color::from_rgba8(0xf2, 0x55, 0x55, 0xff);

/// The `banned_content` picker state.
#[derive(Default)]
struct BannedState {
    /// `_kind` (`content_type` in the GDScript, `"block"` by default).
    kind: String,
    /// Working rules dictionary: caller-supplied or the editor `rules` tag.
    rules: VarDictionary,
    /// True when `rules` mirrors the editor `rules` tag.
    tag_mode: bool,
    /// `contentSearch`: trimmed, collapsed, lowercased.
    search: String,
    /// `selectedCategory`, `""` for none.
    category: String,
    /// Bannable rows from `bannable_content_json`.
    rows: Vec<VarDictionary>,
    /// Category names for the filter row (`Category.values()`).
    categories: Vec<String>,
    /// Current banned names (`contentSet` order).
    banned: Vec<String>,
    search_field: Option<Gd<LineEdit>>,
    selected_grid: Option<Gd<GridContainer>>,
    deselected_grid: Option<Gd<GridContainer>>,
    selected_box: Option<Gd<VBoxContainer>>,
    deselected_box: Option<Gd<VBoxContainer>>,
    add_all_selected: Option<Gd<Button>>,
    add_all_deselected: Option<Gd<Button>>,
    /// `rebuildTable` column count (`max(4, ...)`).
    columns: i32,
}

/// `@bannedblocks`/`@bannedunits` — the two-pane ban picker.
#[derive(Default)]
pub struct BannedContentDialog {
    state: Rc<RefCell<BannedState>>,
}

impl DialogController for BannedContentDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        let mut state = self.state.borrow_mut();
        if state.kind.is_empty() {
            state.kind = String::from("block");
        }
        let kind = state.kind.clone();
        drop(state);
        dialog.set_title_key(banned_title_key(&kind));
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        banned_context_changed(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        banned_shown(dialog, &self.state);
    }
}

/// `@bannedblocks` for blocks, `@bannedunits` for units.
fn banned_title_key(kind: &str) -> &'static str {
    if kind == "block" {
        "@bannedblocks"
    } else {
        "@bannedunits"
    }
}

/// `set_context_json`: `{kind, rules}`; without `rules` the editor tag is edited.
fn banned_context_changed(dialog: &mut MindDialog, state: &Rc<RefCell<BannedState>>) {
    let context_kind = dialog.context_string("kind").to_string();
    let rules_value = dialog.context_value("rules");
    let mut state = state.borrow_mut();
    if !context_kind.is_empty() {
        state.kind = context_kind;
    }
    state.search.clear();
    state.category.clear();
    match rules_value.map(|value| value.try_to::<VarDictionary>()) {
        Some(Ok(rules)) => {
            state.rules = rules.duplicate_deep();
            state.tag_mode = false;
        }
        _ => {
            state.rules = banned_tag_rules();
            state.tag_mode = true;
        }
    }
    let kind = state.kind.clone();
    drop(state);
    dialog.set_title_key(banned_title_key(&kind));
}

/// `shown(this::build)`: load the bannable content + the live set, then build.
fn banned_shown(dialog: &mut MindDialog, state: &Rc<RefCell<BannedState>>) {
    banned_load_content(state);
    let banned = banned_read(state);
    state.borrow_mut().banned = banned;
    banned_build(dialog, state);
}

/// `content.<T>getBy(type)` rows + the category names.
fn banned_load_content(state: &Rc<RefCell<BannedState>>) {
    let kind = state.borrow().kind.clone();
    let mut rows: Vec<VarDictionary> = Vec::new();
    let mut categories: Vec<String> = Vec::new();
    if let Some(value) = editor_call(
        "bannable_content_json",
        &[GString::from(kind.as_str()).to_variant()],
    ) {
        let payload = parse_json_object(&value.to::<GString>().to_string());
        if let Some(value) = payload.get("rows")
            && let Ok(array) = value.try_to::<VarArray>()
        {
            rows = array
                .iter_shared()
                .filter_map(|row| row.try_to::<VarDictionary>().ok())
                .collect();
        }
        if let Some(value) = payload.get("categories")
            && let Ok(array) = value.try_to::<VarArray>()
        {
            categories = array
                .iter_shared()
                .map(|entry| variant_string(&entry))
                .collect();
        }
    }
    let mut state = state.borrow_mut();
    state.rows = rows;
    state.categories = categories;
}

/// `contentSet` contents (`Rules.bannedBlocks`/`bannedUnits`).
fn banned_read(state: &Rc<RefCell<BannedState>>) -> Vec<String> {
    let key = banned_key(&state.borrow().kind);
    let raw = state.borrow().rules.get(key);
    let mut out: Vec<String> = Vec::new();
    if let Some(value) = raw
        && let Ok(array) = value.try_to::<VarArray>()
    {
        for entry in array.iter_shared() {
            if let Ok(name) = entry.try_to::<GString>() {
                let name = name.to_string();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
            }
        }
    }
    out
}

/// `bannedBlocks` for blocks, `bannedUnits` for units.
fn banned_key(kind: &str) -> &'static str {
    if kind == "block" {
        "bannedBlocks"
    } else {
        "bannedUnits"
    }
}

/// `editor.tags["rules"]` parsed.
fn banned_tag_rules() -> VarDictionary {
    let Some(value) = editor_call("tags", &[]) else {
        return VarDictionary::new();
    };
    let Ok(tags) = value.try_to::<VarDictionary>() else {
        return VarDictionary::new();
    };
    let Some(rules) = tags.get("rules") else {
        return VarDictionary::new();
    };
    parse_json_object(&variant_string(&rules))
}

/// `_build`: search row, category row, the two panes.
fn banned_build(dialog: &mut MindDialog, state: &Rc<RefCell<BannedState>>) {
    dialog.clear_content();
    let mut root = dialog.content_table();
    let kind = state.borrow().kind.clone();
    let categories = state.borrow().categories.clone();

    // `t.table(s -> s.label("@search") + field + cancel button)`: content-sized
    // and centred (`BannedContentDialog.java:55-86`).
    let mut top = VBoxContainer::new_alloc();
    root.bind_mut().add(&top).pad(4.0);
    root.bind_mut().row();
    // code-instantiated: the search row mirrors Java's label + field + clear.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 10);
    search_row.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    search_row.set_alignment(AlignmentMode::CENTER);
    let mut search_label = factory::label(&kit::t("@search").to_string());
    search_label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    search_row.add_child(&search_label);
    let mut field = factory::field("");
    field.set_text(&GString::from(state.borrow().search.as_str()));
    // `s.field(...)` has no growX: Arc `TextField` prefWidth is 150.
    field.set_custom_minimum_size(Vector2::new(150.0, 0.0));
    let dialog_gd = dialog.to_gd();
    let state_for_search = state.clone();
    let callable = Callable::from_fn("banned_search", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        banned_on_search(&dialog_gd, &state_for_search, &text);
    });
    field.connect("text_changed", &callable);
    search_row.add_child(&field);
    let dialog_gd = dialog.to_gd();
    let state_for_clear = state.clone();
    let field_for_clear = field.clone();
    let clear = Callable::from_fn("banned_clear", move |_args: &[&Variant]| {
        state_for_clear.borrow_mut().search.clear();
        let mut field = field_for_clear.clone();
        field.set_text("");
        banned_rebuild(&dialog_gd, &state_for_clear);
    });
    let mut clear_button = factory::icon_button(
        "cancel",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    clear_button.set_custom_minimum_size(Vector2::new(35.0, 35.0));
    clear_button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    clear_button.connect("pressed", &clear);
    search_row.add_child(&clear_button);
    top.add_child(&search_row);
    state.borrow_mut().search_field = Some(field);

    // `if(type == ContentType.block)`: category toggles (`Category.values()`).
    if kind == "block" && !categories.is_empty() {
        // code-instantiated: one toggle per block category (data-driven row).
        let mut category_row = HBoxContainer::new_alloc();
        category_row.set_alignment(AlignmentMode::CENTER);
        category_row.add_theme_constant_override("separation", 4);
        for category in &categories {
            let category_name = category.clone();
            let mut button = factory::glyph_button(&category_name, &category_name, "squareTogglei");
            button.set_custom_minimum_size(Vector2::new(45.0, 45.0));
            button.set_toggle_mode(true);
            button.set_pressed(state.borrow().category == category_name);
            let dialog_gd = dialog.to_gd();
            let state_for_category = state.clone();
            let name_for_toggle = category_name.clone();
            let callable = Callable::from_fn("banned_category", move |_args: &[&Variant]| {
                {
                    let mut state = state_for_category.borrow_mut();
                    if state.category == name_for_toggle {
                        state.category.clear();
                    } else {
                        state.category = name_for_toggle.clone();
                    }
                }
                banned_rebuild(&dialog_gd, &state_for_category);
            });
            button.connect("pressed", &callable);
            category_row.add_child(&button);
        }
        top.add_child(&category_row);
    }

    // `cont.table(...)` / `cont.table(...)`: the banned + unbanned panes.
    let portrait = is_portrait(dialog);
    // Portrait stacks the panes (`if(Core.graphics.isPortrait()) cont.row()`).
    let mut panes: Gd<BoxContainer> = if portrait {
        VBoxContainer::new_alloc().upcast()
    } else {
        HBoxContainer::new_alloc().upcast()
    };
    panes.set_v_size_flags(SizeFlags::EXPAND_FILL);
    panes.add_theme_constant_override("separation", 10);
    root.bind_mut().add(&panes).grow_y().pad(4.0);
    let dialog_gd = dialog.to_gd();
    state.borrow_mut().columns = banned_columns(dialog);
    let banned_widget = banned_pane(&dialog_gd, state, true);
    panes.add_child(&banned_widget);
    let available_widget = banned_pane(&dialog_gd, state, false);
    panes.add_child(&available_widget);
    banned_rebuild(&dialog_gd, state);
}

/// One pane (`table.image().color(...)` + `pane(...)` + `@addall`).
fn banned_pane(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<BannedState>>,
    selected: bool,
) -> Gd<VBoxContainer> {
    let kind = state.borrow().kind.clone();
    // code-instantiated: one ban pane (title, rule, scrolling icon grid).
    let mut box_ = VBoxContainer::new_alloc();
    box_.set_h_size_flags(SizeFlags::FILL);
    box_.set_v_size_flags(SizeFlags::EXPAND_FILL);
    let color = if selected {
        BANNED_ACCENT
    } else {
        styles::ACCENT
    };
    let prefix = if selected { "banned" } else { "unbanned" };
    let suffix = if kind == "block" { "blocks" } else { "units" };
    let mut title = factory::label(&kit::t(&format!("@{prefix}{suffix}")).to_string());
    title.add_theme_color_override("font_color", color);
    // `table.add(...).padBottom(-1).top()`: the title centres over the rule.
    title.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    box_.add_child(&title);
    // code-instantiated: the 3px rule under the pane header.
    let mut line = ColorRect::new_alloc();
    line.set_color(color);
    line.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    line.set_h_size_flags(SizeFlags::EXPAND_FILL);
    box_.add_child(&line);
    let mut scroll = MindScroll::new_alloc();
    scroll.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    scroll.bind_mut().set_scrolling_disabled(true, false);
    scroll.set_custom_minimum_size(Vector2::new(0.0, 110.0));
    box_.add_child(&scroll);
    // code-instantiated: the icon grid is data-driven (content rows).
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(state.borrow().columns.max(1));
    // code-instantiated: the centring wrapper of the data-driven grid.
    let mut center = CenterContainer::new_alloc();
    center.set_h_size_flags(SizeFlags::EXPAND_FILL);
    center.set_v_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&center);
    center.add_child(&grid);
    let add_text = kit::t("@addall").to_string();
    let mut add_all = factory::icon_button(
        "add",
        &add_text,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    add_all.set_custom_minimum_size(Vector2::new(0.0, 48.0));
    let dialog_gd = dialog.clone();
    let state_for_add = state.clone();
    let callable = Callable::from_fn("banned_add_all", move |_args: &[&Variant]| {
        banned_add_all(&dialog_gd, &state_for_add, selected);
    });
    add_all.connect("pressed", &callable);
    box_.add_child(&add_all);
    {
        let mut state = state.borrow_mut();
        if selected {
            state.selected_grid = Some(grid);
            state.selected_box = Some(box_.clone());
            state.add_all_selected = Some(add_all);
        } else {
            state.deselected_grid = Some(grid);
            state.deselected_box = Some(box_.clone());
            state.add_all_deselected = Some(add_all);
        }
    }
    box_
}

/// `rebuildTable` column count (`Core.graphics.getWidth() / Scl.scl()`):
/// `max(4, (screen - 300) / 50 / 2)` cells per pane.
fn banned_columns(dialog: &MindDialog) -> i32 {
    let width = viewport_width(dialog);
    if is_portrait(dialog) {
        // `max(4, (width - 100) / 50)` (`BannedContentDialog.java:155-156`).
        (((width - 100.0) / BANNED_CELL) as i32).max(4)
    } else {
        (((width - 300.0) / BANNED_CELL / 2.0) as i32).max(4)
    }
}

/// `rebuildTables()`: refilter and repaint both panes.
fn banned_rebuild(dialog: &Gd<MindDialog>, state: &Rc<RefCell<BannedState>>) {
    let selected_grid = state.borrow().selected_grid.clone();
    let deselected_grid = state.borrow().deselected_grid.clone();
    let (Some(mut selected_grid), Some(mut deselected_grid)) = (selected_grid, deselected_grid)
    else {
        return;
    };
    clear_children(&mut selected_grid);
    clear_children(&mut deselected_grid);
    let filtered = banned_filtered_rows(state);
    let banned = state.borrow().banned.clone();
    let mut banned_rows: Vec<VarDictionary> = Vec::new();
    let mut available_rows: Vec<VarDictionary> = Vec::new();
    for row in filtered {
        if banned.contains(&dict_string(&row, "name")) {
            banned_rows.push(row);
        } else {
            available_rows.push(row);
        }
    }
    banned_fill_grid(dialog, state, &mut selected_grid, &banned_rows);
    banned_fill_grid(dialog, state, &mut deselected_grid, &available_rows);
    // `contentSet.toSeq().containsAll(filteredContent)`: enabled exactly when a
    // filtered row is still unbanned.
    if let Some(mut button) = state.borrow().add_all_selected.clone() {
        button.set_disabled(available_rows.is_empty());
    }
    // `available.containsAll(filteredContent)`: enabled exactly when a filtered
    // row is banned.
    if let Some(mut button) = state.borrow().add_all_deselected.clone() {
        button.set_disabled(banned_rows.is_empty());
    }
    banned_uniform_panes_deferred(state);
}

/// Arc `uniform()` on both pane cells: each takes the larger pref width.
fn banned_uniform_panes_deferred(state: &Rc<RefCell<BannedState>>) {
    let state = state.clone();
    defer_action("banned_uniform_panes", move || {
        banned_uniform_panes(&state);
    });
}

fn banned_uniform_panes(state: &Rc<RefCell<BannedState>>) {
    let boxes = {
        let state = state.borrow();
        (state.selected_box.clone(), state.deselected_box.clone())
    };
    let (Some(selected), Some(deselected)) = boxes else {
        return;
    };
    if !selected.is_instance_valid() || !deselected.is_instance_valid() {
        return;
    }
    let mut boxes = [selected, deselected];
    let mut width = 0.0_f32;
    for box_ in boxes.iter_mut() {
        let height = box_.get_custom_minimum_size().y;
        box_.set_custom_minimum_size(Vector2::new(0.0, height));
        width = width.max(box_.get_combined_minimum_size().x);
    }
    for box_ in boxes.iter_mut() {
        let height = box_.get_custom_minimum_size().y;
        box_.set_custom_minimum_size(Vector2::new(width, height));
    }
}

/// `_fill_grid(table, isSelected)`: one 50px cell per bannable row.
fn banned_fill_grid(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<BannedState>>,
    grid: &mut Gd<GridContainer>,
    rows: &[VarDictionary],
) {
    if rows.is_empty() {
        let mut empty = factory::label(&kit::t("@empty").to_string());
        empty.set_custom_minimum_size(Vector2::new(BANNED_CELL * grid.get_columns() as f32, 0.0));
        empty.set_horizontal_alignment(HorizontalAlignment::CENTER);
        grid.add_child(&empty);
        return;
    }
    for row in rows {
        let content_name = dict_string(row, "name");
        let icon = dict_string(row, "ui_icon");
        // code-instantiated: one 50px content cell per bannable row.
        let mut button =
            factory::image_button_first(&[icon.as_str(), content_name.as_str()], "clearNonei");
        button.set_custom_minimum_size(Vector2::new(BANNED_CELL, BANNED_CELL));
        button.set_expand_icon(true);
        button.add_theme_constant_override("icon_max_width", BANNED_ICON);
        if button.get_button_icon().is_none() {
            button.set_text(&GString::from(content_name.as_str()));
        }
        button.set_tooltip_text(&banned_localized(state, &content_name));
        let dialog_gd = dialog.clone();
        let state_for_toggle = state.clone();
        let name_for_toggle = content_name.clone();
        let callable = Callable::from_fn("banned_toggle", move |_args: &[&Variant]| {
            banned_toggle(&dialog_gd, &state_for_toggle, &name_for_toggle);
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
}

/// `content.getBy(type).select(pred)` + search/category, sorted by content id.
fn banned_filtered_rows(state: &Rc<RefCell<BannedState>>) -> Vec<VarDictionary> {
    let (rows, category, search) = {
        let state = state.borrow();
        (
            state.rows.clone(),
            state.category.clone(),
            state.search.clone(),
        )
    };
    let mut out: Vec<VarDictionary> = Vec::new();
    for row in rows {
        if !category.is_empty() && dict_string(&row, "category") != category {
            continue;
        }
        if !search.is_empty() {
            // `content.localizedName.toLowerCase().contains(contentSearch)`.
            let localized = banned_localized(state, &dict_string(&row, "name"));
            if !localized.to_lowercase().contains(&search) {
                continue;
            }
        }
        out.push(row);
    }
    out.sort_by_key(|row| dict_i64(row, "id", 0));
    out
}

/// `content.localizedName`, falling back to the content name.
fn banned_localized(state: &Rc<RefCell<BannedState>>, content_name: &str) -> String {
    let prefix = if state.borrow().kind == "block" {
        "block"
    } else {
        "unit"
    };
    kit::t(&format!("@{prefix}.{content_name}.name")).to_string()
}

/// `value.trim().replaceAll(" +", " ").toLowerCase()` (`contentSearch`).
fn banned_on_search(dialog: &Gd<MindDialog>, state: &Rc<RefCell<BannedState>>, text: &str) {
    let normalized = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    state.borrow_mut().search = normalized;
    banned_rebuild(dialog, state);
}

/// Clicking a cell moves the content between panes (`contentSet.add/remove`).
fn banned_toggle(dialog: &Gd<MindDialog>, state: &Rc<RefCell<BannedState>>, content_name: &str) {
    {
        let mut state = state.borrow_mut();
        if let Some(index) = state.banned.iter().position(|name| name == content_name) {
            state.banned.remove(index);
        } else {
            state.banned.push(String::from(content_name));
        }
    }
    banned_commit(dialog, state);
    banned_rebuild(dialog, state);
}

/// `@addall` on the banned pane adds, on the unbanned pane removes.
fn banned_add_all(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<BannedState>>,
    banned_pane_selected: bool,
) {
    let filtered = banned_filtered_rows(state);
    {
        let mut state = state.borrow_mut();
        for row in &filtered {
            let name = dict_string(row, "name");
            if banned_pane_selected {
                if !state.banned.contains(&name) {
                    state.banned.push(name);
                }
            } else if let Some(index) = state.banned.iter().position(|entry| entry == &name) {
                state.banned.remove(index);
            }
        }
    }
    banned_commit(dialog, state);
    banned_rebuild(dialog, state);
}

/// Writes the set back: caller rules via `banned_edited`, else the editor tag.
fn banned_commit(dialog: &Gd<MindDialog>, state: &Rc<RefCell<BannedState>>) {
    let (kind, tag_mode, banned, rules) = {
        let mut state = state.borrow_mut();
        let banned = state.banned.clone();
        let key = banned_key(&state.kind);
        state
            .rules
            .set(&GString::from(key), &string_array_variant(&banned));
        (
            state.kind.clone(),
            state.tag_mode,
            banned,
            state.rules.clone(),
        )
    };
    if tag_mode {
        let _ = editor_call(
            "set_banned_names_json",
            &[
                GString::from(kind.as_str()).to_variant(),
                GString::from(stringify(&string_array_variant(&banned)).as_str()).to_variant(),
            ],
        );
        return;
    }
    if dialog.is_instance_valid() {
        let mut node = dialog.clone();
        node.emit_signal(
            "banned_edited",
            &[GString::from(stringify(&rules.to_variant()).as_str()).to_variant()],
        );
    }
}

// ---------------------------------------------------------------------------
// MapAssetsDialog (`map_assets`)
// ---------------------------------------------------------------------------

/// `DataAssetType.all` in ordinal order (the tab identity).
const ASSET_TYPES: [&str; 6] = ["patch", "content", "bundle", "image", "sound", "music"];
/// `MapAssetsDialog` row geometry: `h = 50f`, `mobile ? 390f : 450f`.
const ASSET_ROW_HEIGHT: f32 = 50.0;
const ASSET_ROW_WIDTH: f32 = 450.0;
const ASSET_ROW_WIDTH_MOBILE: f32 = 390.0;
const ASSET_IMAGE_CELL: f32 = 200.0;
/// `iconMed` (32 px) in the icon-only cells.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
const ASSET_ICON_SIZE: i32 = 32;
/// `Vars.patchesGuideURL`.
const PATCHES_GUIDE_URL: &str = "https://mindustrygame.github.io/wiki/datapatches/";
/// `Color.lightGray` (Godot `0.62` grey).
const LIGHT_GRAY: Color = Color::from_rgba(0.62, 0.62, 0.62, 1.0);

/// `DataAssetType.localized()` bundle keys.
fn asset_type_key(kind: &str) -> &'static str {
    match kind {
        "patch" => "@asset.patch",
        "content" => "@asset.content",
        "bundle" => "@asset.bundle",
        "image" => "@asset.image",
        "sound" => "@asset.sound",
        "music" => "@asset.music",
        _ => "",
    }
}

/// `MapAssetsDialog.typeIcons`.
fn asset_type_icon(kind: &str) -> &'static str {
    match kind {
        "patch" => "file-code",
        "content" => "box",
        "bundle" => "file-text",
        "image" => "image",
        "sound" => "volume-up",
        "music" => "music",
        _ => "file-text",
    }
}

/// `MapContentView.contentIcons` (`drizzle` has no alias, `effect` stands in).
fn asset_content_icon(content_type: &str) -> &'static str {
    match content_type {
        "item" => "box",
        "liquid" => "liquid",
        "unit" => "units",
        "block" => "distribution",
        "planet" => "planet",
        "weather" => "effect",
        "status" => "power",
        _ => "box",
    }
}

/// `ContentAsset.loadableContent` (the add-content type row).
const ASSET_CONTENT_TYPES: [&str; 6] = ["item", "block", "liquid", "status", "unit", "weather"];

/// A deferred confirmation target (`ui.showConfirm` answer).
#[derive(Clone)]
enum PendingConfirm {
    ClearAll,
    PatchDelete(i64),
    ContentDelete(String),
    BundleDelete(String),
    ImageDelete(String),
    AudioDelete(String, String),
}

/// A pending file-chooser result (`_open_file`) handler.
type AssetPathsHandler = Rc<dyn Fn(Vec<String>)>;
/// A pending save-chooser result (`_save_file`) handler.
type AssetTextHandler = Rc<dyn Fn(String)>;
/// An import-source result (`_import_choices`) handler.
type AssetImportHandler = Rc<dyn Fn(String)>;

/// The `map_assets` view state.
#[derive(Default)]
struct MapAssetsState {
    /// `_type`: the active `DataAssetType` (`patch` by default).
    kind: String,
    search: String,
    rows: Vec<VarDictionary>,
    type_buttons: Vec<(String, Gd<Button>)>,
    type_label: Option<Gd<MindLabel>>,
    search_field: Option<Gd<LineEdit>>,
    list: Option<Gd<VBoxContainer>>,
    /// The one pending `confirm_result` handler.
    pending_confirm: Option<PendingConfirm>,
    /// Last playing data audio row name (music pause icon).
    playing_audio: String,
    /// Image path awaiting the rename prompt result.
    renaming_image: String,
}

/// `@asset.patch` — the data-asset manager (five upstream view classes).
#[derive(Default)]
pub struct MapAssetsDialog {
    state: Rc<RefCell<MapAssetsState>>,
}

impl DialogController for MapAssetsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        map_assets_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        map_assets_shown(dialog, &self.state);
    }
}

fn map_assets_ready(dialog: &mut MindDialog, state: &Rc<RefCell<MapAssetsState>>) {
    // `super("")`: the title is not displayed.
    dialog.set_title_literal("");
    dialog.should_pause = false;
    dialog.apply_cont_top(true);
    {
        let mut state = state.borrow_mut();
        if state.kind.is_empty() {
            state.kind = String::from("patch");
        }
    }
    let dialog_gd = dialog.to_gd();
    let state_for_confirm = state.clone();
    let callable = Callable::from_fn("map_assets_confirm", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .map(|value| variant_bool(value))
            .unwrap_or(false);
        let dialog_for = dialog_gd.clone();
        let state_for = state_for_confirm.clone();
        defer_action("map_assets_confirm_result", move || {
            map_assets_on_confirm(&dialog_for, &state_for, confirmed);
        });
    });
    connect_prompt_once(dialog, "map_assets_confirm", "confirm_result", &callable);
}

/// `shown`: reset search, then `changeType(patch)`.
fn map_assets_shown(dialog: &mut MindDialog, state: &Rc<RefCell<MapAssetsState>>) {
    {
        let mut state = state.borrow_mut();
        state.search.clear();
        if let Some(mut field) = state.search_field.clone() {
            field.set_text("");
        }
    }
    map_assets_refetch(state);
    map_assets_build(dialog, state);
}

/// Pulls the current record set from the owner (`state.data`).
fn map_assets_refetch(state: &Rc<RefCell<MapAssetsState>>) {
    let rows = editor_call("assets", &[])
        .and_then(|value| value.try_to::<VarArray>().ok())
        .map(|array| {
            array
                .iter_shared()
                .filter_map(|row| row.try_to::<VarDictionary>().ok())
                .collect()
        })
        .unwrap_or_default();
    state.borrow_mut().rows = rows;
}

fn map_assets_build(dialog: &mut MindDialog, state: &Rc<RefCell<MapAssetsState>>) {
    dialog.clear_content();
    let mut root = dialog.content_table();
    let dialog_gd = dialog.to_gd();
    let kind = state.borrow().kind.clone();

    // `cont.add(types).growX().left().row()`.
    let mut types = HBoxContainer::new_alloc();
    types.add_theme_constant_override("separation", 4);
    types.set_h_size_flags(SizeFlags::EXPAND_FILL);
    root.bind_mut().add(&types).grow_x();
    root.bind_mut().row();

    // `types.button(Icon.menu, Styles.graySquarei, this::showEditMenu)`.
    let mut menu = factory::icon_button(
        "menu",
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    menu.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    menu.set_tooltip_text(&kit::t("@edit.menu").to_string());
    let menu_gd = dialog_gd.clone();
    let menu_state = state.clone();
    let callable = Callable::from_fn("map_assets_edit_menu", move |_args: &[&Variant]| {
        map_assets_show_edit_menu(&menu_gd, &menu_state);
    });
    menu.connect("pressed", &callable);
    types.add_child(&menu);
    types.add_child(&map_assets_divider());

    state.borrow_mut().type_buttons.clear();
    for type_name in ASSET_TYPES {
        // `types.button(typeIcons[type.ordinal()], Styles.grayTogglei, ...)`.
        let mut button = factory::glyph_button(
            asset_type_icon(type_name),
            &kit::t(asset_type_key(type_name)).to_string(),
            "grayTogglei",
        );
        button.set_toggle_mode(true);
        button.set_pressed(kind == type_name);
        button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
        button.set_tooltip_text(&kit::t(asset_type_key(type_name)).to_string());
        let dialog_for = dialog_gd.clone();
        let state_for = state.clone();
        let name_for_switch = String::from(type_name);
        let callable = Callable::from_fn("map_assets_type", move |_args: &[&Variant]| {
            if !dialog_for.is_instance_valid() {
                return;
            }
            let mut node = dialog_for.clone();
            let mut bound = node.bind_mut();
            map_assets_change_type(&mut bound, &state_for, &name_for_switch);
        });
        button.connect("pressed", &callable);
        types.add_child(&button);
        state
            .borrow_mut()
            .type_buttons
            .push((String::from(type_name), button));
    }
    types.add_child(&map_assets_divider());

    // `types.table(Styles.grayPanel, t -> t.label(() -> currentType.localized()))`.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &flat_box(styles::GRAY_PANEL));
    panel.set_custom_minimum_size(Vector2::new(0.0, 50.0));
    let mut type_label = factory::label(&kit::t(asset_type_key(&kind)).to_string());
    type_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    panel.add_child(&type_label);
    types.add_child(&panel);
    state.borrow_mut().type_label = Some(type_label);

    let mut spacer = factory::space(0.0, 0.0);
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    types.add_child(&spacer);

    // `types.button("@asset.guide", Icon.link, Styles.grayt, ...).size(200, 50)`.
    let mut guide = factory::icon_button(
        "link",
        &kit::t("@asset.guide").to_string(),
        "grayt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    guide.set_custom_minimum_size(Vector2::new(200.0, 50.0));
    let callable = Callable::from_fn("map_assets_guide", |_args: &[&Variant]| {
        let _ = Os::singleton().shell_open(PATCHES_GUIDE_URL);
    });
    guide.connect("pressed", &callable);
    types.add_child(&guide);

    // `cont.table(search -> image + field).growX().row()`.
    let mut search = HBoxContainer::new_alloc();
    search.add_theme_constant_override("separation", 4);
    search.set_h_size_flags(SizeFlags::EXPAND_FILL);
    root.bind_mut().add(&search).grow_x();
    root.bind_mut().row();
    let mut zoom = factory::image("zoom");
    zoom.set_custom_minimum_size(Vector2::new(30.0, 30.0));
    zoom.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    search.add_child(&zoom);
    let mut field = factory::field(&kit::t("@search").to_string());
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    field.set_text(&GString::from(state.borrow().search.as_str()));
    let dialog_for_search = dialog_gd.clone();
    let state_for_search = state.clone();
    let callable = Callable::from_fn("map_assets_search", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        map_assets_on_search(&dialog_for_search, &state_for_search, &text);
    });
    field.connect("text_changed", &callable);
    search.add_child(&field);
    state.borrow_mut().search_field = Some(field);

    // `cont.pane(t -> list = t).grow().top()`.
    // code-instantiated: the record list is data-driven (`rebuild`).
    let mut scroll = MindScroll::new_alloc();
    scroll.bind_mut().set_scrolling_disabled(true, false);
    scroll.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    scroll.set_custom_minimum_size(Vector2::new(0.0, 300.0));
    root.bind_mut().add(&scroll).grow_x().grow_y();
    root.bind_mut().row();
    let mut list = VBoxContainer::new_alloc();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    list.add_theme_constant_override("separation", 4);
    scroll.add_child(&list);
    state.borrow_mut().list = Some(list);

    map_assets_change_type(dialog, state, &kind);
}

/// `changeType(type)`: swap the action buttons, then `rebuild()`.
fn map_assets_change_type(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapAssetsState>>,
    kind: &str,
) {
    state.borrow_mut().kind = String::from(kind);
    if let Some(mut label) = state.borrow().type_label.clone() {
        label.set_text(&kit::t(asset_type_key(kind)));
    }
    let buttons = state.borrow().type_buttons.clone();
    for (key, mut button) in buttons {
        button.set_pressed(key == kind);
    }

    dialog.clear_buttons();
    let dialog_gd = dialog.to_gd();
    match kind {
        "patch" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let add = Callable::from_fn("map_assets_add_patch", move |_args: &[&Variant]| {
                map_assets_add_patch(&for_add, &state_for);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
        }
        "content" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let add = Callable::from_fn("map_assets_add_content", move |_args: &[&Variant]| {
                map_assets_add_content(&for_add, &state_for);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
        }
        "bundle" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let add = Callable::from_fn("map_assets_add_bundle", move |_args: &[&Variant]| {
                map_assets_add_bundle(&for_add, &state_for);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
        }
        "image" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let add = Callable::from_fn("map_assets_add_image", move |_args: &[&Variant]| {
                map_assets_add_image(&for_add, &state_for);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
            let for_menu = dialog_gd.clone();
            let state_for = state.clone();
            let edit = Callable::from_fn("map_assets_image_menu", move |_args: &[&Variant]| {
                map_assets_show_edit_menu(&for_menu, &state_for);
            });
            let text = kit::t("@edit").to_string();
            dialog.add_button(&text, Some(edit), "menu", 190.0);
        }
        "sound" | "music" => {
            let for_add = dialog_gd.clone();
            let state_for = state.clone();
            let kind_for_add = String::from(kind);
            let add = Callable::from_fn("map_assets_add_audio", move |_args: &[&Variant]| {
                map_assets_add_audio(&for_add, &state_for, &kind_for_add);
            });
            let text = kit::t("@add").to_string();
            dialog.add_button(&text, Some(add), "add", 190.0);
        }
        _ => {}
    }
    dialog.add_close_button(210.0);

    // Switching to the content view reloads content + sprites (`changeType`).
    if kind == "content" {
        let _ = editor_call("data_reload_content", &[false.to_variant()]);
        map_assets_refetch(state);
    }
    map_assets_rebuild(&dialog_gd, state);
}

/// `rebuild()`: clear the pane and run the current `AssetView.build`.
fn map_assets_rebuild(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    let list = state.borrow().list.clone();
    let Some(mut list) = list else {
        return;
    };
    clear_children(&mut list);
    let kind = state.borrow().kind.clone();
    match kind.as_str() {
        "patch" => map_assets_build_patches(dialog, &mut list, state),
        "content" => map_assets_build_content(dialog, &mut list, state),
        "bundle" => map_assets_build_bundles(dialog, &mut list, state),
        "image" => map_assets_build_images(dialog, &mut list, state),
        "sound" | "music" => map_assets_build_audio(dialog, &mut list, state, &kind),
        _ => {}
    }
}

fn map_assets_on_search(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>, text: &str) {
    // `t.length() > 0 ? t.toLowerCase() : null`.
    state.borrow_mut().search = if text.is_empty() {
        String::new()
    } else {
        text.to_lowercase()
    };
    map_assets_rebuild(dialog, state);
}

// --- helpers ---------------------------------------------------------------

/// `types.image(Tex.whiteui, Pal.accent).size(4f, 50f)`.
fn map_assets_divider() -> Gd<ColorRect> {
    // code-instantiated: the 4x50 accent rule between type-strip groups.
    let mut line = ColorRect::new_alloc();
    line.set_color(styles::ACCENT);
    line.set_custom_minimum_size(Vector2::new(4.0, 50.0));
    line
}

/// Rows of one `DataAssetType` (`_rows_of`).
fn map_assets_rows_of(state: &Rc<RefCell<MapAssetsState>>, kind: &str) -> Vec<VarDictionary> {
    state
        .borrow()
        .rows
        .iter()
        .filter(|row| dict_string(row, "kind") == kind)
        .cloned()
        .collect()
}

/// `_matches`: the lowercased search over one row key.
fn map_assets_matches(state: &Rc<RefCell<MapAssetsState>>, row: &VarDictionary, key: &str) -> bool {
    let search = state.borrow().search.clone();
    if search.is_empty() {
        return true;
    }
    dict_string(row, key).to_lowercase().contains(&search)
}

/// The viewport width from a live dialog handle.
fn map_assets_viewport_width(dialog: &Gd<MindDialog>) -> f32 {
    dialog.bind().base().get_viewport_rect().size.x
}

/// `_row_width`: `Vars.mobile ? 390f : 450f`.
fn map_assets_row_width(dialog: &MindDialog) -> f32 {
    if is_portrait(dialog) {
        ASSET_ROW_WIDTH_MOBILE
    } else {
        ASSET_ROW_WIDTH
    }
}

/// Empty cell for Java's `list.add().size(h)` warning placeholder.
fn map_assets_empty_cell() -> Gd<Control> {
    factory::space(ASSET_ROW_HEIGHT, ASSET_ROW_HEIGHT)
}

/// The two-line entry button (`[accent]name\n[lightgray]sub`).
fn map_assets_entry_button(
    row: &VarDictionary,
    title: &str,
    subtitle: &str,
    width: f32,
    handler: Callable,
) -> Gd<Button> {
    // code-instantiated: one two-line data-asset entry row.
    let mut button = factory::button("");
    button.set_theme_type_variation("grayt");
    button.set_custom_minimum_size(Vector2::new(width, ASSET_ROW_HEIGHT));
    button.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut raw = format!("[accent]{title}");
    if !subtitle.is_empty() {
        raw.push_str(&format!("\n[lightgray]{subtitle}"));
    }
    let mut label = factory::label(&markup(&raw));
    label.set_horizontal_alignment(HorizontalAlignment::LEFT);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    label.set_offset(Side::LEFT, 10.0);
    label.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&label);
    button.set_tooltip_text(&dict_string(row, "path"));
    button.connect("pressed", &handler);
    button
}

/// `_error(message)`: the shared error prompt (`@error.title`).
fn map_assets_error(message: &str) {
    let title = kit::t("@error.title").to_string();
    if ui_call(
        "show_text",
        &[
            GString::from(title.as_str()).to_variant(),
            GString::from(message).to_variant(),
        ],
    )
    .is_some()
    {
        return;
    }
    log::warn!("[ui] map_assets: {message}");
}

/// `ui.showConfirm` + one pending handler (`confirm_result`).
fn map_assets_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    message: &str,
    pending: PendingConfirm,
) {
    if ui_call("show_confirm", &[GString::from(message).to_variant()]).is_some() {
        state.borrow_mut().pending_confirm = Some(pending);
        return;
    }
    map_assets_run_confirm(dialog, state, pending);
}

fn map_assets_on_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    confirmed: bool,
) {
    let pending = state.borrow_mut().pending_confirm.take();
    if !confirmed {
        return;
    }
    if let Some(pending) = pending {
        map_assets_run_confirm(dialog, state, pending);
    }
}

fn map_assets_run_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    pending: PendingConfirm,
) {
    match pending {
        PendingConfirm::ClearAll => {
            let result = editor_call("data_clear", &[]);
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::PatchDelete(index) => {
            let result = editor_call("data_patch_remove", &[index.to_variant()]);
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::ContentDelete(name) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from("content").to_variant(),
                    GString::from(name.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::BundleDelete(path) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from("bundle").to_variant(),
                    GString::from(path.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::ImageDelete(name) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from("image").to_variant(),
                    GString::from(name.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
        PendingConfirm::AudioDelete(kind, name) => {
            let result = editor_call(
                "data_asset_delete",
                &[
                    GString::from(kind.as_str()).to_variant(),
                    GString::from(name.as_str()).to_variant(),
                ],
            );
            map_assets_apply(dialog, state, result);
        }
    }
}

/// Owner error codes -> bundle keys (`DataManager` dialog messages).
fn map_assets_error_key(code: &str) -> Option<&'static str> {
    match code {
        "exists" => Some("@asset.content.exists"),
        "badname" => Some("@asset.content.badname"),
        "badtype" => Some("@asset.content.type"),
        "invalid JSON" => Some("@patch.importerror"),
        _ => None,
    }
}

/// Runs an owner action; on failure shows its message, on success refetches.
fn map_assets_apply(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    result: Option<Variant>,
) {
    let value = result
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default();
    if !dict_bool(&value, "ok", false) {
        let mut message = dict_string(&value, "error");
        if let Some(key) = map_assets_error_key(&message) {
            message = kit::t(key).to_string();
        }
        map_assets_error(&message);
        return;
    }
    map_assets_refetch(state);
    map_assets_rebuild(dialog, state);
}

/// `_show_text(title, text_body)`.
fn map_assets_show_text(title: &str, body: &str) {
    show_text(title, body);
}

/// `_open_file(label, filters, multiple, handler)`: the runtime `FileDialog`.
fn map_assets_open_file(
    dialog: &Gd<MindDialog>,
    label: &str,
    filters: Vec<String>,
    multiple: bool,
    handler: AssetPathsHandler,
) {
    // code-instantiated: `FileChooser.open` is a runtime native dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_title(&GString::from(label));
    file_dialog.set_file_mode(if multiple {
        FileMode::OPEN_FILES
    } else {
        FileMode::OPEN_FILE
    });
    file_dialog.set_access(Access::FILESYSTEM);
    for filter in &filters {
        file_dialog.add_filter(&GString::from(filter.as_str()));
    }
    let selected_target = file_dialog.clone();
    let selected_handler = handler.clone();
    let callable = Callable::from_fn("asset_file_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = selected_target.clone();
        target.queue_free();
        selected_handler(vec![path]);
    });
    file_dialog.connect("file_selected", &callable);
    let files_target = file_dialog.clone();
    let files_handler = handler.clone();
    let callable = Callable::from_fn("asset_files_selected", move |args: &[&Variant]| {
        let paths = args
            .first()
            .and_then(|value| (*value).try_to::<PackedStringArray>().ok())
            .unwrap_or_default();
        let mut target = files_target.clone();
        target.queue_free();
        files_handler(
            paths
                .as_slice()
                .iter()
                .map(|path| path.to_string())
                .collect(),
        );
    });
    file_dialog.connect("files_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("asset_file_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&file_dialog);
    file_dialog.popup_centered_ratio_ex().ratio(0.6).done();
}

/// `_save_file(label, file_name, filter, handler)`: the runtime save dialog.
fn map_assets_save_file(
    dialog: &Gd<MindDialog>,
    label: &str,
    file_name: &str,
    filter: &str,
    handler: AssetTextHandler,
) {
    // code-instantiated: `FileChooser.export` is a runtime native dialog.
    let mut file_dialog = FileDialog::new_alloc();
    file_dialog.set_title(&GString::from(label));
    file_dialog.set_file_mode(FileMode::SAVE_FILE);
    file_dialog.set_access(Access::FILESYSTEM);
    file_dialog.set_current_file(&GString::from(file_name));
    file_dialog.add_filter(&GString::from(filter));
    let selected_target = file_dialog.clone();
    let callable = Callable::from_fn("asset_save_selected", move |args: &[&Variant]| {
        let path = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut target = selected_target.clone();
        target.queue_free();
        handler(path);
    });
    file_dialog.connect("file_selected", &callable);
    let canceled_target = file_dialog.clone();
    let callable = Callable::from_fn("asset_save_canceled", move |_args: &[&Variant]| {
        let mut target = canceled_target.clone();
        target.queue_free();
    });
    file_dialog.connect("canceled", &callable);
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&file_dialog);
    file_dialog.popup_centered_ratio_ex().ratio(0.6).done();
}

/// `_read_text(path)`: `FileAccess.get_file_as_string`.
fn map_assets_read_text(path: &str) -> String {
    FileAccess::get_file_as_string(path).to_string()
}

/// `_to_array(paths)`.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn map_assets_to_array(values: &PackedStringArray) -> Vec<String> {
    values
        .as_slice()
        .iter()
        .map(|value| value.to_string())
        .collect()
}

/// `_write_text(path, text)` (`FileAccess.open(path, WRITE).store_string`).
fn map_assets_write_text(path: &str, text: &str) {
    let Some(mut file) = FileAccess::open(path, godot::classes::file_access::ModeFlags::WRITE)
    else {
        return;
    };
    let _ = file.store_string(text);
}

/// `showImport(handler)`: clipboard + file choices (`MapPatchesView`).
fn map_assets_import_choices(
    dialog: &Gd<MindDialog>,
    filters: Vec<String>,
    handler: AssetImportHandler,
) {
    // code-instantiated: Java's `BaseDialog("@editor.import")` choice menu.
    let mut accept = AcceptDialog::new_alloc();
    accept.set_title(&kit::t("@editor.import"));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 6);
    box_.set_custom_minimum_size(Vector2::new(300.0, 120.0));
    let clipboard_text = DisplayServer::singleton().clipboard_get();
    let clipboard_label = kit::t("@load.clipboard").to_string();
    let mut clipboard = factory::icon_button(
        "copy",
        &clipboard_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    clipboard.set_disabled(clipboard_text.is_empty());
    let clip_target = accept.clone();
    let clip_handler = handler.clone();
    let callable = Callable::from_fn("asset_import_clipboard", move |_args: &[&Variant]| {
        let text = DisplayServer::singleton().clipboard_get().to_string();
        let mut target = clip_target.clone();
        target.queue_free();
        if !text.is_empty() {
            clip_handler(text);
        }
    });
    clipboard.connect("pressed", &callable);
    box_.add_child(&clipboard);
    let file_label = kit::t("@import.file").to_string();
    let mut file = factory::icon_button(
        "download",
        &file_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let file_target = accept.clone();
    let dialog_for = dialog.clone();
    let file_handler = handler.clone();
    let file_filters = filters.clone();
    let callable = Callable::from_fn("asset_import_file", move |_args: &[&Variant]| {
        let mut target = file_target.clone();
        target.queue_free();
        let handler_for = file_handler.clone();
        let label = kit::t("@import.file").to_string();
        map_assets_open_file(
            &dialog_for,
            &label,
            file_filters.clone(),
            false,
            Rc::new(move |paths: Vec<String>| {
                let path = paths.first().cloned().unwrap_or_default();
                let text = map_assets_read_text(&path);
                handler_for(text);
            }),
        );
    });
    file.connect("pressed", &callable);
    box_.add_child(&file);
    accept.add_child(&box_);
    if let Some(mut ok) = accept.get_ok_button() {
        ok.set_visible(false);
    }
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&accept);
    accept
        .popup_centered_ex()
        .minsize(Vector2i::new(340, 180))
        .done();
}

/// `MapAssetsDialog.showEditMenu` (`@asset.importzip`/`exportzip`/`clearall`).
fn map_assets_show_edit_menu(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    // code-instantiated: Java builds this `BaseDialog("@edit.menu")` per open.
    let mut accept = AcceptDialog::new_alloc();
    accept.set_title(&kit::t("@edit.menu"));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 6);
    box_.set_custom_minimum_size(Vector2::new(300.0, 170.0));

    let import_label = kit::t("@asset.importzip").to_string();
    let mut import_zip = factory::icon_button(
        "download",
        &import_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let import_target = accept.clone();
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("asset_import_zip", move |_args: &[&Variant]| {
        let mut target = import_target.clone();
        target.queue_free();
        let label = kit::t("@asset.importzip").to_string();
        let dialog_for_zip = dialog_for.clone();
        let state_for_zip = state_for.clone();
        map_assets_open_file(
            &dialog_for,
            &label,
            vec![String::from("*.zip")],
            false,
            Rc::new(move |paths: Vec<String>| {
                let path = paths.first().cloned().unwrap_or_default();
                let result = editor_call(
                    "data_import_zip",
                    &[GString::from(path.as_str()).to_variant()],
                );
                map_assets_apply(&dialog_for_zip, &state_for_zip, result);
            }),
        );
    });
    import_zip.connect("pressed", &callable);
    box_.add_child(&import_zip);

    let export_label = kit::t("@asset.exportzip").to_string();
    let mut export_zip = factory::icon_button(
        "upload",
        &export_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let export_target = accept.clone();
    let dialog_for = dialog.clone();
    let callable = Callable::from_fn("asset_export_zip", move |_args: &[&Variant]| {
        let mut target = export_target.clone();
        target.queue_free();
        let label = kit::t("@asset.exportzip").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            "assets.zip",
            "*.zip",
            Rc::new(move |path: String| {
                let exported = editor_call(
                    "data_export_zip",
                    &[GString::from(path.as_str()).to_variant()],
                )
                .map(|value| variant_bool(&value))
                .unwrap_or(false);
                if !exported {
                    map_assets_error(&kit::t("@asset.exportzip").to_string());
                }
            }),
        );
    });
    export_zip.connect("pressed", &callable);
    box_.add_child(&export_zip);

    let clear_label = kit::t("@asset.clearall").to_string();
    let mut clear_all = factory::icon_button(
        "trash",
        &clear_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let clear_target = accept.clone();
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("asset_clear_all", move |_args: &[&Variant]| {
        let mut target = clear_target.clone();
        target.queue_free();
        let message = kit::t("@asset.clearall.confirm").to_string();
        map_assets_confirm(&dialog_for, &state_for, &message, PendingConfirm::ClearAll);
    });
    clear_all.connect("pressed", &callable);
    box_.add_child(&clear_all);
    accept.add_child(&box_);
    if let Some(mut ok) = accept.get_ok_button() {
        ok.set_visible(false);
    }
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&accept);
    accept
        .popup_centered_ex()
        .minsize(Vector2i::new(340, 230))
        .done();
}

// --- MapPatchesView --------------------------------------------------------

fn map_assets_build_patches(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    let rows = map_assets_rows_of(state, "patch");
    let mut shown_count = 0;
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let widget = map_assets_patch_row(dialog, state, row);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@patch.none"));
        list.add_child(&label);
    }
}

fn map_assets_patch_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
) -> Gd<HBoxContainer> {
    let index = dict_i64(row, "index", 0);
    let patch_name = dict_string(row, "name");
    let json = dict_string(row, "data");
    let label = if patch_name.is_empty() {
        String::from("<unnamed>")
    } else {
        patch_name
    };
    let fields = map_assets_count_fields(&map_assets_parse_json(&json));
    // code-instantiated: one patch entry row (show/copy/import/delete).
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 4);
    box_.set_alignment(AlignmentMode::CENTER);
    box_.add_child(&map_assets_empty_cell());
    let width = ASSET_ROW_WIDTH;
    let title_for_show = label.clone();
    let json_for_show = json.clone();
    let show = Callable::from_fn("patch_show", move |_args: &[&Variant]| {
        let title = kit::tm_format("@patch.set", &[&title_for_show]).to_string();
        map_assets_show_text(&title, &json_for_show);
    });
    let subtitle = kit::tm_format("@patch.fields", &[&fields.to_string()]).to_string();
    box_.add_child(&map_assets_entry_button(
        row, &label, &subtitle, width, show,
    ));
    let json_for_copy = json.clone();
    let copy = Callable::from_fn("patch_copy", move |_args: &[&Variant]| {
        DisplayServer::singleton().clipboard_set(&GString::from(json_for_copy.as_str()));
        show_toast(&kit::t("@copied").to_string());
    });
    box_.add_child(&square_cell(
        "copy",
        &kit::t("@copy.clipboard").to_string(),
        copy,
    ));
    let state_for_import = state.clone();
    let dialog_for_import = dialog.clone();
    let replace = Callable::from_fn("patch_replace", move |_args: &[&Variant]| {
        let state_for = state_for_import.clone();
        let dialog_for = dialog_for_import.clone();
        map_assets_import_choices(
            &dialog_for.clone(),
            map_assets_text_filters(),
            Rc::new(move |text: String| {
                let result = editor_call(
                    "data_patch_replace",
                    &[
                        index.to_variant(),
                        GString::from(text.as_str()).to_variant(),
                    ],
                );
                map_assets_apply(&dialog_for, &state_for, result);
            }),
        );
    });
    box_.add_child(&square_cell(
        "refresh",
        &kit::t("@editor.import").to_string(),
        replace,
    ));
    let state_for_delete = state.clone();
    let dialog_for_delete = dialog.clone();
    let delete = Callable::from_fn("patch_delete", move |_args: &[&Variant]| {
        let state_for = state_for_delete.clone();
        let message = kit::t("@patch.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_delete,
            &state_for,
            &message,
            PendingConfirm::PatchDelete(index),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@patch.delete.confirm").to_string(),
        delete,
    ));
    box_
}

/// `_add_patch`: `MapPatchesView.addPatch` through the clipboard/file import.
fn map_assets_add_patch(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    map_assets_import_choices(
        dialog,
        map_assets_text_filters(),
        Rc::new(move |text: String| {
            let result = editor_call(
                "data_patch_add",
                &[GString::from(text.as_str()).to_variant()],
            );
            map_assets_apply(&dialog_for, &state_for, result);
        }),
    );
}

/// `_text_filters()`: the patch JSON extensions.
fn map_assets_text_filters() -> Vec<String> {
    vec![
        String::from("*.json"),
        String::from("*.hjson"),
        String::from("*.json5"),
    ]
}

/// `MapPatchesView.countFields`: leaf count of the patch JSON, at least 1.
fn map_assets_count_fields(value: &Variant) -> i64 {
    if let Ok(dict) = value.try_to::<VarDictionary>() {
        let sum: i64 = dict
            .values_array()
            .iter_shared()
            .map(|child| map_assets_count_fields(&child))
            .sum();
        return sum.max(1);
    }
    if let Ok(array) = value.try_to::<VarArray>() {
        let sum: i64 = array
            .iter_shared()
            .map(|child| map_assets_count_fields(&child))
            .sum();
        return sum.max(1);
    }
    1
}

/// `_parse_json(text)` (`{}` when unparseable).
fn map_assets_parse_json(text: &str) -> Variant {
    if text.is_empty() {
        return VarDictionary::new().to_variant();
    }
    let parsed = Json::parse_string(text);
    if parsed.is_nil() {
        return VarDictionary::new().to_variant();
    }
    parsed
}

// --- MapContentView --------------------------------------------------------

fn map_assets_build_content(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    let rows = map_assets_rows_of(state, "content");
    let mut shown_count = 0;
    let mut last_type = String::new();
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let content_type = dict_string(row, "contentType");
        if content_type != last_type {
            last_type = content_type.clone();
            let header = map_assets_content_header(&content_type);
            list.add_child(&header);
        }
        let widget = map_assets_content_row(dialog, state, row, &content_type);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@none.found"));
        list.add_child(&label);
    }
}

fn map_assets_content_header(content_type: &str) -> Gd<HBoxContainer> {
    // `list.table(t -> icon + "@content.<type>" + whiteui rule).colspan(6)`.
    // code-instantiated: one content-type header row.
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 8);
    box_.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let glyph = factory::glyph_label(asset_content_icon(content_type), 22, LIGHT_GRAY);
    box_.add_child(&glyph);
    let mut label = factory::label(&kit::t(&format!("@content.{content_type}")).to_string());
    label.add_theme_color_override("font_color", LIGHT_GRAY);
    box_.add_child(&label);
    // code-instantiated: the rule under the type header (Java `image().growX()`).
    let mut rule = ColorRect::new_alloc();
    rule.set_color(LIGHT_GRAY);
    rule.set_custom_minimum_size(Vector2::new(0.0, 4.0));
    rule.set_h_size_flags(SizeFlags::EXPAND_FILL);
    box_.add_child(&rule);
    box_
}

fn map_assets_content_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
    content_type: &str,
) -> Gd<HBoxContainer> {
    let content_name = dict_string(row, "name");
    let json = dict_string(row, "data");
    // code-instantiated: one content record row.
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 4);
    box_.set_alignment(AlignmentMode::CENTER);
    box_.add_child(&map_assets_empty_cell());

    // `content.content instanceof UnlockableContent ? uiIcon : contentIcons.get(type)`.
    let mut icon = factory::icon_button(
        asset_content_icon(content_type),
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    icon.set_custom_minimum_size(Vector2::new(ASSET_ROW_HEIGHT, ASSET_ROW_HEIGHT));
    icon.set_disabled(true);
    box_.add_child(&icon);

    let name_for_show = content_name.clone();
    let json_for_show = json.clone();
    let show = Callable::from_fn("content_show", move |_args: &[&Variant]| {
        map_assets_show_text(&name_for_show, &json_for_show);
    });
    let subtitle = kit::t(&format!("@content.{content_type}")).to_string();
    let width = ASSET_ROW_WIDTH;
    box_.add_child(&map_assets_entry_button(
        row,
        &content_name,
        &subtitle,
        width,
        show,
    ));

    let state_for_import = state.clone();
    let dialog_for_import = dialog.clone();
    let content_for_import = content_name.clone();
    let type_for_import = String::from(content_type);
    let replace = Callable::from_fn("content_replace", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_import.clone();
        let state_for = state_for_import.clone();
        let content_for = content_for_import.clone();
        let type_for = type_for_import.clone();
        map_assets_import_choices(
            &dialog_for.clone(),
            map_assets_text_filters(),
            Rc::new(move |text: String| {
                // Java overwrites `content.data`; replace then re-add.
                let removed = editor_call(
                    "data_asset_delete",
                    &[
                        GString::from("content").to_variant(),
                        GString::from(content_for.as_str()).to_variant(),
                    ],
                )
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
                if !dict_bool(&removed, "ok", false) {
                    map_assets_error(&dict_string(&removed, "error"));
                    return;
                }
                let result = editor_call(
                    "data_content_add",
                    &[
                        GString::from(content_for.as_str()).to_variant(),
                        GString::from(type_for.as_str()).to_variant(),
                        GString::from(text.as_str()).to_variant(),
                    ],
                );
                map_assets_apply(&dialog_for, &state_for, result);
            }),
        );
    });
    box_.add_child(&square_cell(
        "refresh",
        &kit::t("@editor.import").to_string(),
        replace,
    ));

    let dialog_for_export = dialog.clone();
    let content_for_export = content_name.clone();
    let json_for_export = json.clone();
    let export = Callable::from_fn("content_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let content_for = content_for_export.clone();
        let json_for = json_for_export.clone();
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &format!("{content_for}.json"),
            "*.json",
            Rc::new(move |path: String| {
                map_assets_write_text(&path, &json_for);
            }),
        );
    });
    box_.add_child(&square_cell(
        "export",
        &kit::t("@export.file").to_string(),
        export,
    ));

    let state_for_delete = state.clone();
    let dialog_for_delete = dialog.clone();
    let content_for_delete = content_name.clone();
    let delete = Callable::from_fn("content_delete", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_delete.clone();
        let state_for = state_for_delete.clone();
        let message = kit::t("@asset.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for,
            &state_for,
            &message,
            PendingConfirm::ContentDelete(content_for_delete.clone()),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@asset.delete.confirm").to_string(),
        delete,
    ));
    box_
}

/// `MapContentView.showImport`: name + type row, then clipboard/file.
fn map_assets_add_content(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    // code-instantiated: Java's `BaseDialog("@asset.content.add")` name/type form.
    let mut accept = AcceptDialog::new_alloc();
    accept.set_title(&kit::t("@asset.content.add"));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 6);
    box_.set_custom_minimum_size(Vector2::new(420.0, 190.0));
    let mut name_row = HBoxContainer::new_alloc();
    let name_label = factory::label(&kit::t("@name").to_string());
    name_row.add_child(&name_label);
    let mut name_field = factory::field("");
    name_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    name_field.set_max_length(64);
    name_row.add_child(&name_field);
    box_.add_child(&name_row);
    let mut type_row = HBoxContainer::new_alloc();
    let type_label = factory::label(&kit::t("@asset.content.type").to_string());
    type_row.add_child(&type_label);
    let selection: Rc<RefCell<String>> = Rc::new(RefCell::new(String::from("item")));
    let type_buttons: Rc<RefCell<Vec<Gd<Button>>>> = Rc::new(RefCell::new(Vec::new()));
    for content_type in ASSET_CONTENT_TYPES {
        let content_type = String::from(content_type);
        let mut button = factory::glyph_button(
            asset_content_icon(&content_type),
            &kit::t(&format!("@content.{content_type}")).to_string(),
            "squareTogglei",
        );
        button.set_toggle_mode(true);
        button.set_pressed(*selection.borrow() == content_type);
        button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
        button.set_tooltip_text(&kit::t(&format!("@content.{content_type}")).to_string());
        let selection_for = selection.clone();
        let name_for = content_type.clone();
        let buttons_for = type_buttons.clone();
        let button_for = button.clone();
        let callable = Callable::from_fn("content_type_pick", move |_args: &[&Variant]| {
            *selection_for.borrow_mut() = name_for.clone();
            let buttons = buttons_for.borrow();
            for other in buttons.iter() {
                let mut other = other.clone();
                if other == button_for {
                    other.set_pressed(true);
                } else {
                    other.set_pressed(false);
                }
            }
        });
        button.connect("pressed", &callable);
        type_buttons.borrow_mut().push(button.clone());
        type_row.add_child(&button);
    }
    box_.add_child(&type_row);
    accept.add_child(&box_);
    if let Some(mut ok) = accept.get_ok_button() {
        ok.set_visible(false);
    }
    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 6);
    let cancel_target = accept.clone();
    let cancel_label = kit::t("@cancel").to_string();
    let mut cancel = factory::icon_button(
        "cancel",
        &cancel_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let callable = Callable::from_fn("content_add_cancel", move |_args: &[&Variant]| {
        let mut target = cancel_target.clone();
        target.queue_free();
    });
    cancel.connect("pressed", &callable);
    actions.add_child(&cancel);

    let file_target = accept.clone();
    let file_dialog = dialog.clone();
    let state_for_file = state.clone();
    let selection_for_file = selection.clone();
    let name_for_file = name_field.clone();
    let file_label = kit::t("@asset.content.import.file").to_string();
    let mut import_file = factory::icon_button(
        "file-text",
        &file_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let callable = Callable::from_fn("content_add_file", move |_args: &[&Variant]| {
        let chosen = selection_for_file.borrow().clone();
        let name = name_for_file.get_text().to_string();
        let mut target = file_target.clone();
        target.queue_free();
        let dialog_for = file_dialog.clone();
        let state_for = state_for_file.clone();
        let label = kit::t("@asset.content.import.file").to_string();
        map_assets_open_file(
            &file_dialog,
            &label,
            map_assets_text_filters(),
            false,
            Rc::new(move |paths: Vec<String>| {
                let path = paths.first().cloned().unwrap_or_default();
                let text = map_assets_read_text(&path);
                let result = editor_call(
                    "data_content_add",
                    &[
                        GString::from(name.as_str()).to_variant(),
                        GString::from(chosen.as_str()).to_variant(),
                        GString::from(text.as_str()).to_variant(),
                    ],
                );
                map_assets_apply(&dialog_for, &state_for, result);
            }),
        );
    });
    import_file.connect("pressed", &callable);
    actions.add_child(&import_file);

    let clip_target = accept.clone();
    let clip_dialog = dialog.clone();
    let state_for_clip = state.clone();
    let selection_for_clip = selection.clone();
    let name_for_clip = name_field.clone();
    let clip_label = kit::t("@asset.content.import.clipboard").to_string();
    let mut import_clipboard = factory::icon_button(
        "copy",
        &clip_label,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    let callable = Callable::from_fn("content_add_clipboard", move |_args: &[&Variant]| {
        let chosen = selection_for_clip.borrow().clone();
        let name = name_for_clip.get_text().to_string();
        let text = DisplayServer::singleton().clipboard_get().to_string();
        let mut target = clip_target.clone();
        target.queue_free();
        let result = editor_call(
            "data_content_add",
            &[
                GString::from(name.as_str()).to_variant(),
                GString::from(chosen.as_str()).to_variant(),
                GString::from(text.as_str()).to_variant(),
            ],
        );
        map_assets_apply(&clip_dialog, &state_for_clip, result);
    });
    import_clipboard.connect("pressed", &callable);
    actions.add_child(&import_clipboard);
    accept.add_child(&actions);
    let mut dialog_node: Gd<Node> = dialog.clone().upcast();
    dialog_node.add_child(&accept);
    accept
        .popup_centered_ex()
        .minsize(Vector2i::new(500, 260))
        .done();
}

// --- MapBundlesView --------------------------------------------------------

fn map_assets_build_bundles(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    let rows = map_assets_rows_of(state, "bundle");
    let mut shown_count = 0;
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let widget = map_assets_bundle_row(dialog, state, row);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@patch.none"));
        list.add_child(&label);
    }
}

fn map_assets_bundle_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
) -> Gd<HBoxContainer> {
    let bundle_name = dict_string(row, "name");
    let path = dict_string(row, "path");
    let text = dict_string(row, "data");
    let lines = dict_i64(row, "size", 0);
    // code-instantiated: one bundle entry row (show/import/export/delete).
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 4);
    box_.set_alignment(AlignmentMode::CENTER);
    box_.add_child(&map_assets_empty_cell());
    let name_for_show = bundle_name.clone();
    let text_for_show = text.clone();
    let show = Callable::from_fn("bundle_show", move |_args: &[&Variant]| {
        map_assets_show_text(&name_for_show, &text_for_show);
    });
    let subtitle = kit::tm_format("@bundle.lines", &[&lines.to_string()]).to_string();
    let width = map_assets_row_width(&dialog.bind());
    box_.add_child(&map_assets_entry_button(
        row,
        &bundle_name,
        &subtitle,
        width,
        show,
    ));

    let state_for_import = state.clone();
    let dialog_for_import = dialog.clone();
    let path_for_import = path.clone();
    let refresh = Callable::from_fn("bundle_import", move |_args: &[&Variant]| {
        let state_for = state_for_import.clone();
        let dialog_for = dialog_for_import.clone();
        let path_for = path_for_import.clone();
        map_assets_import_choices(
            &dialog_for.clone(),
            vec![String::from("*.properties"), String::from("*.txt")],
            Rc::new(move |text: String| {
                let result = editor_call(
                    "data_bundle_set",
                    &[
                        GString::from(path_for.as_str()).to_variant(),
                        GString::from(text.as_str()).to_variant(),
                    ],
                );
                map_assets_apply(&dialog_for, &state_for, result);
            }),
        );
    });
    box_.add_child(&square_cell(
        "refresh",
        &kit::t("@editor.import").to_string(),
        refresh,
    ));

    let dialog_for_export = dialog.clone();
    let bundle_for_export = bundle_name.clone();
    let text_for_export = text.clone();
    let export = Callable::from_fn("bundle_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let bundle_for = bundle_for_export.clone();
        let text_for = text_for_export.clone();
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &bundle_for,
            "*.properties",
            Rc::new(move |target: String| {
                map_assets_write_text(&target, &text_for);
            }),
        );
    });
    box_.add_child(&square_cell(
        "export",
        &kit::t("@export.file").to_string(),
        export,
    ));

    let state_for_delete = state.clone();
    let dialog_for_delete = dialog.clone();
    let path_for_delete = path.clone();
    let delete = Callable::from_fn("bundle_delete", move |_args: &[&Variant]| {
        let state_for = state_for_delete.clone();
        let message = kit::t("@asset.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_delete,
            &state_for,
            &message,
            PendingConfirm::BundleDelete(path_for_delete.clone()),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@asset.delete.confirm").to_string(),
        delete,
    ));
    box_
}

fn map_assets_add_bundle(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let label = kit::t("@add").to_string();
    map_assets_open_file(
        dialog,
        &label,
        vec![String::from("*.properties"), String::from("*.txt")],
        true,
        Rc::new(move |paths: Vec<String>| {
            for path in paths {
                let result = editor_call(
                    "data_file_add",
                    &[
                        GString::from("bundle").to_variant(),
                        GString::from(path.as_str()).to_variant(),
                    ],
                )
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
                if !dict_bool(&result, "ok", false) {
                    map_assets_error(&dict_string(&result, "error"));
                }
            }
            map_assets_refetch(&state_for);
            map_assets_rebuild(&dialog_for, &state_for);
        }),
    );
}

// --- MapImagesView ---------------------------------------------------------

fn map_assets_build_images(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
) {
    // code-instantiated: the image grid is record-driven; columns follow Java.
    let width = map_assets_viewport_width(dialog);
    let columns = (((width - 20.0) * 0.9 / (ASSET_IMAGE_CELL + 14.0)) as i32).max(1);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(columns);
    grid.set_h_size_flags(SizeFlags::EXPAND_FILL);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    list.add_child(&grid);
    let mut count = 0;
    for row in &map_assets_rows_of(state, "image") {
        if dict_bool(row, "generated", false) {
            continue;
        }
        if !map_assets_matches(state, row, "path") {
            continue;
        }
        count += 1;
        let cell = map_assets_image_cell(dialog, state, row);
        grid.add_child(&cell);
    }
    if count == 0 {
        let mut empty = factory::label(&markup("@none.found"));
        empty.set_horizontal_alignment(HorizontalAlignment::CENTER);
        list.add_child(&empty);
    }
}

/// The image record fields the info page and the card need.
struct AssetImageInfo {
    name: String,
    path: String,
    region: String,
    found: bool,
    width: i64,
    height: i64,
    size: i64,
}

fn map_assets_image_cell(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
) -> Gd<PanelContainer> {
    let info = AssetImageInfo {
        name: dict_string(row, "name"),
        path: dict_string(row, "path"),
        region: dict_string(row, "region"),
        found: dict_bool(row, "found", false),
        width: dict_i64(row, "width", 1),
        height: dict_i64(row, "height", 1),
        size: dict_i64(row, "size", 0),
    };
    // code-instantiated: one `Styles.grayPanel` card per image record.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &flat_box(styles::GRAY_PANEL));
    panel.set_custom_minimum_size(Vector2::new(ASSET_IMAGE_CELL, ASSET_IMAGE_CELL));
    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 2);
    panel.add_child(&box_);

    // `t.add(new BorderImage(region, 4f)).scaling(Scaling.fit).size(size - 10f)`.
    if info.found {
        // code-instantiated: the preview texture is a runtime `dp-` region.
        let mut preview = TextureRect::new_alloc();
        preview.set_custom_minimum_size(Vector2::new(
            ASSET_IMAGE_CELL - 10.0,
            ASSET_IMAGE_CELL - 60.0,
        ));
        preview.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        preview.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        if let Some(mut assets) = kit::assets()
            && let Some(texture) = assets
                .bind_mut()
                .find_region(GString::from(info.region.as_str()))
        {
            let texture: Gd<Texture2D> = texture.upcast();
            preview.set_texture(&texture);
        }
        box_.add_child(&preview);
    } else {
        // `Core.bundle.get("asset.image.packfailed")` placeholder cell.
        let mut missing = factory::label("⚠");
        missing.set_horizontal_alignment(HorizontalAlignment::CENTER);
        missing.set_vertical_alignment(VerticalAlignment::CENTER);
        missing.set_custom_minimum_size(Vector2::new(
            ASSET_IMAGE_CELL - 10.0,
            ASSET_IMAGE_CELL - 60.0,
        ));
        missing.set_tooltip_text(&kit::t("@asset.image.packfailed").to_string());
        box_.add_child(&missing);
    }

    let raw = if info.found {
        info.name.clone()
    } else {
        format!("[red]⚠[] {}", info.name)
    };
    let mut label = factory::label(&markup(&raw));
    // GDScript assigns `text_overrun_behavior`, which `RichTextLabel` does not
    // expose in Godot 4.7 (the assignment is a no-op there); the tooltip keeps
    // the full region/path.
    label.set_tooltip_text(&format!("{}\n{}", info.region, info.path));
    label.set_mouse_filter(MouseFilter::PASS);
    let dialog_for_label = dialog.clone();
    let info_for_label = Rc::new(AssetImageInfo {
        name: info.name.clone(),
        path: info.path.clone(),
        region: info.region.clone(),
        found: info.found,
        width: info.width,
        height: info.height,
        size: info.size,
    });
    let callable = Callable::from_fn("image_info_click", move |args: &[&Variant]| {
        let Some(event) = args
            .first()
            .and_then(|value| (*value).try_to::<Gd<godot::classes::InputEvent>>().ok())
        else {
            return;
        };
        let Ok(button) = event.try_cast::<godot::classes::InputEventMouseButton>() else {
            return;
        };
        if !button.is_pressed() {
            return;
        }
        map_assets_show_image_info(&dialog_for_label, &info_for_label);
    });
    label.connect("gui_input", &callable);
    box_.add_child(&label);
    let mut dims = factory::label(&format!("{}x{}", info.width, info.height));
    dims.add_theme_color_override("font_color", LIGHT_GRAY);
    box_.add_child(&dims);

    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 0);
    let cell = (ASSET_IMAGE_CELL - 10.0) / 4.0;
    let dialog_for_pencil = dialog.clone();
    let state_for_pencil = state.clone();
    let path_for_pencil = info.path.clone();
    let pencil = Callable::from_fn("image_rename", move |_args: &[&Variant]| {
        if !dialog_for_pencil.is_instance_valid() {
            return;
        }
        let mut node = dialog_for_pencil.clone();
        let mut bound = node.bind_mut();
        map_assets_rename_image(&mut bound, &state_for_pencil, &path_for_pencil);
    });
    let mut pencil_button = factory::icon_button(
        "pencil",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    pencil_button.set_custom_minimum_size(Vector2::new(cell, cell));
    pencil_button.set_tooltip_text(&kit::t("@save.rename").to_string());
    pencil_button.connect("pressed", &pencil);
    actions.add_child(&pencil_button);

    let dialog_for_info = dialog.clone();
    let info_for_button = Rc::new(AssetImageInfo {
        name: info.name.clone(),
        path: info.path.clone(),
        region: info.region.clone(),
        found: info.found,
        width: info.width,
        height: info.height,
        size: info.size,
    });
    let info_call = Callable::from_fn("image_info_button", move |_args: &[&Variant]| {
        map_assets_show_image_info(&dialog_for_info, &info_for_button);
    });
    let mut info_button = factory::icon_button(
        "info",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    info_button.set_custom_minimum_size(Vector2::new(cell, cell));
    info_button.set_tooltip_text(&kit::t("@asset.image.info").to_string());
    info_button.connect("pressed", &info_call);
    actions.add_child(&info_button);

    let dialog_for_export = dialog.clone();
    let name_for_export = info.name.clone();
    let path_for_export = info.path.clone();
    let export = Callable::from_fn("image_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let name_for = name_for_export.clone();
        let path_for = path_for_export.clone();
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &format!("{name_for}.png"),
            "*.png",
            Rc::new(move |target: String| {
                let exported = editor_call(
                    "data_asset_export",
                    &[
                        GString::from("image").to_variant(),
                        GString::from(path_for.as_str()).to_variant(),
                        GString::from(target.as_str()).to_variant(),
                    ],
                )
                .map(|value| variant_bool(&value))
                .unwrap_or(false);
                if !exported {
                    map_assets_error(&kit::t("@asset.broken").to_string());
                }
            }),
        );
    });
    let mut export_button = factory::icon_button(
        "export",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    export_button.set_custom_minimum_size(Vector2::new(cell, cell));
    export_button.set_tooltip_text(&kit::t("@export.file").to_string());
    export_button.connect("pressed", &export);
    actions.add_child(&export_button);

    let dialog_for_trash = dialog.clone();
    let state_for_trash = state.clone();
    let name_for_trash = info.name.clone();
    let trash = Callable::from_fn("image_delete", move |_args: &[&Variant]| {
        let message = kit::t("@asset.image.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_trash,
            &state_for_trash,
            &message,
            PendingConfirm::ImageDelete(name_for_trash.clone()),
        );
    });
    let mut trash_button = factory::icon_button(
        "trash",
        "",
        "emptyi",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    trash_button.set_custom_minimum_size(Vector2::new(cell, cell));
    trash_button.set_tooltip_text(&kit::t("@asset.image.delete.confirm").to_string());
    trash_button.connect("pressed", &trash);
    actions.add_child(&trash_button);
    box_.add_child(&actions);
    panel
}

fn map_assets_show_image_info(dialog: &Gd<MindDialog>, info: &AssetImageInfo) {
    let _ = dialog;
    let env = info.path.contains("blocks/environment/");
    let mut lines: Vec<String> = Vec::new();
    if !info.found {
        lines.push(kit::t("@asset.image.packfailed").to_string());
        if env {
            lines.push(kit::t("@asset.image.packfailed.env").to_string());
        }
    }
    lines.push(kit::tm_format("@asset.image.name", &[&info.name]).to_string());
    lines.push(kit::tm_format("@asset.image.region.name", &[&info.region]).to_string());
    lines.push(kit::tm_format("@asset.image.path", &[&info.path]).to_string());
    lines.push(kit::tm_format("@asset.image.env", &[&env.to_string()]).to_string());
    lines.push(
        kit::tm_format(
            "@asset.image.size",
            &[&info.width.to_string(), &info.height.to_string()],
        )
        .to_string(),
    );
    lines.push(kit::tm_format("@asset.image.filesize", &[&info.size.to_string()]).to_string());
    let title = kit::t("@asset.image.info").to_string();
    map_assets_show_text(&title, &lines.join("\n"));
}

fn map_assets_add_image(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let label = kit::t("@add").to_string();
    map_assets_open_file(
        dialog,
        &label,
        vec![
            String::from("*.png"),
            String::from("*.jpg"),
            String::from("*.jpeg"),
        ],
        true,
        Rc::new(move |paths: Vec<String>| {
            for path in paths {
                let result = editor_call(
                    "data_file_add",
                    &[
                        GString::from("image").to_variant(),
                        GString::from(path.as_str()).to_variant(),
                    ],
                )
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
                if !dict_bool(&result, "ok", false) {
                    map_assets_error(&dict_string(&result, "error"));
                }
            }
            map_assets_refetch(&state_for);
            map_assets_rebuild(&dialog_for, &state_for);
        }),
    );
}

/// `MapImagesView` pencil (`ui.showTextInput("@save.rename", "@patch.path", ...)`).
fn map_assets_rename_image(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<MapAssetsState>>,
    path: &str,
) {
    let dialog_gd = dialog.to_gd();
    let state_for = state.clone();
    let callable = Callable::from_fn("map_assets_image_rename", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let dialog_for = dialog_gd.clone();
        let state_for = state_for.clone();
        defer_action("map_assets_rename_result", move || {
            map_assets_on_image_rename(&dialog_for, &state_for, &text);
        });
    });
    connect_prompt_once(dialog, "map_assets_rename", "text_input_result", &callable);
    state.borrow_mut().renaming_image = String::from(path);
    let _ = ui_call(
        "show_text_input",
        &[
            kit::t("@save.rename").to_variant(),
            kit::t("@patch.path").to_variant(),
            1000i64.to_variant(),
            GString::from(path).to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

fn map_assets_on_image_rename(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    text: &str,
) {
    let old = std::mem::take(&mut state.borrow_mut().renaming_image);
    if old.is_empty() || text.is_empty() || text == old {
        return;
    }
    let result = editor_call(
        "data_image_rename",
        &[
            GString::from(old.as_str()).to_variant(),
            GString::from(text).to_variant(),
        ],
    );
    map_assets_apply(dialog, state, result);
}

// --- MapAudioView ----------------------------------------------------------

fn map_assets_build_audio(
    dialog: &Gd<MindDialog>,
    list: &mut Gd<VBoxContainer>,
    state: &Rc<RefCell<MapAssetsState>>,
    kind: &str,
) {
    let rows = map_assets_rows_of(state, kind);
    let mut shown_count = 0;
    for row in &rows {
        if !map_assets_matches(state, row, "name") {
            continue;
        }
        shown_count += 1;
        let widget = map_assets_audio_row(dialog, state, row, kind);
        list.add_child(&widget);
    }
    if shown_count == 0 {
        let label = factory::label(&markup("@none.found"));
        list.add_child(&label);
    }
}

fn map_assets_audio_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    row: &VarDictionary,
    kind: &str,
) -> Gd<HBoxContainer> {
    let audio_name = dict_string(row, "name");
    let path = dict_string(row, "path");
    let music = kind == "music";
    let stream_name = map_assets_audio_stream_name(row);
    // code-instantiated: one audio record row (play/export/delete).
    let mut box_ = HBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 4);
    box_.set_alignment(AlignmentMode::CENTER);

    let playing = state.borrow().playing_audio == stream_name;
    let mut play = factory::icon_button(
        if playing { "pause" } else { "play" },
        "",
        "graySquarei",
        godot::classes::box_container::AlignmentMode::CENTER,
    );
    play.set_custom_minimum_size(Vector2::new(ASSET_ROW_HEIGHT, ASSET_ROW_HEIGHT));
    play.set_tooltip_text(&kit::t("@play").to_string());
    let dialog_for_play = dialog.clone();
    let state_for_play = state.clone();
    let stream_for_play = stream_name.clone();
    let button_for_play = play.clone();
    let callable = Callable::from_fn("audio_play", move |_args: &[&Variant]| {
        map_assets_play_audio(
            &dialog_for_play,
            &state_for_play,
            &stream_for_play,
            music,
            &button_for_play,
        );
    });
    play.connect("pressed", &callable);
    box_.add_child(&play);

    // `in.add("[accent]" + asset.name)` + the formatted length.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &flat_box(styles::GRAY_PANEL));
    panel.set_custom_minimum_size(Vector2::new(
        map_assets_row_width(&dialog.bind()),
        ASSET_ROW_HEIGHT,
    ));
    panel.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut lines = VBoxContainer::new_alloc();
    lines.add_theme_constant_override("separation", 0);
    let mut title = factory::label(&markup(&format!("[accent]{audio_name}")));
    title.set_horizontal_alignment(HorizontalAlignment::LEFT);
    lines.add_child(&title);
    let seconds = map_assets_audio_length(&stream_name, music);
    let mut length = factory::label(&format!(
        "[lightgray][[{}]",
        map_assets_format_seconds(seconds)
    ));
    length.add_theme_color_override("font_color", LIGHT_GRAY);
    lines.add_child(&length);
    panel.add_child(&lines);
    box_.add_child(&panel);

    let dialog_for_export = dialog.clone();
    let name_for_export = audio_name.clone();
    let path_for_export = path.clone();
    let kind_for_export = String::from(kind);
    let export = Callable::from_fn("audio_export", move |_args: &[&Variant]| {
        let dialog_for = dialog_for_export.clone();
        let name_for = name_for_export.clone();
        let path_for = path_for_export.clone();
        let kind_for = kind_for_export.clone();
        let extension = if path_for.to_lowercase().ends_with(".ogg") {
            "*.ogg"
        } else {
            "*.mp3"
        };
        let label = kit::t("@editor.export").to_string();
        map_assets_save_file(
            &dialog_for,
            &label,
            &name_for,
            extension,
            Rc::new(move |target: String| {
                let exported = editor_call(
                    "data_asset_export",
                    &[
                        GString::from(kind_for.as_str()).to_variant(),
                        GString::from(path_for.as_str()).to_variant(),
                        GString::from(target.as_str()).to_variant(),
                    ],
                )
                .map(|value| variant_bool(&value))
                .unwrap_or(false);
                if !exported {
                    map_assets_error(&kit::t("@asset.broken").to_string());
                }
            }),
        );
    });
    box_.add_child(&square_cell(
        "export",
        &kit::t("@export.file").to_string(),
        export,
    ));

    let dialog_for_trash = dialog.clone();
    let state_for_trash = state.clone();
    let name_for_trash = audio_name.clone();
    let kind_for_trash = String::from(kind);
    let trash = Callable::from_fn("audio_delete", move |_args: &[&Variant]| {
        let message = kit::t("@asset.delete.confirm").to_string();
        map_assets_confirm(
            &dialog_for_trash,
            &state_for_trash,
            &message,
            PendingConfirm::AudioDelete(kind_for_trash.clone(), name_for_trash.clone()),
        );
    });
    box_.add_child(&square_cell(
        "trash",
        &kit::t("@asset.delete.confirm").to_string(),
        trash,
    ));
    box_
}

/// `DataAudioLoader.prefix + asset.name.replace(' ', '_')` (`dp-`).
fn map_assets_audio_stream_name(row: &VarDictionary) -> String {
    let asset_name = dict_string(row, "name").replace(' ', "_");
    format!("dp-{}", asset_name.to_lowercase())
}

fn map_assets_audio_length(stream_name: &str, music: bool) -> f64 {
    let Some(mut assets) = kit::assets() else {
        return 0.0;
    };
    let stream = if music {
        assets.bind_mut().music_stream(GString::from(stream_name))
    } else {
        assets.bind_mut().sound_stream(GString::from(stream_name))
    };
    stream.map(|stream| stream.get_length()).unwrap_or(0.0)
}

fn map_assets_format_seconds(seconds: f64) -> String {
    let total = seconds as i64;
    format!("{}:{:02}", total / 60, total % 60)
}

fn map_assets_play_audio(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<MapAssetsState>>,
    stream_name: &str,
    music: bool,
    button: &Gd<Button>,
) {
    let _ = dialog;
    let Some(mut audio) = kit::node("/root/Spine/MindAudio") else {
        return;
    };
    if !audio.has_method("play_data_audio") {
        return;
    }
    let started = audio
        .call(
            "play_data_audio",
            &[GString::from(stream_name).to_variant(), music.to_variant()],
        )
        .try_to::<bool>()
        .unwrap_or(false);
    if !started {
        return;
    }
    state.borrow_mut().playing_audio = String::from(stream_name);
    if let Some(texture) = kit::icon_texture("pause") {
        let mut button = button.clone();
        button.set_button_icon(&texture);
    }
}

fn map_assets_add_audio(dialog: &Gd<MindDialog>, state: &Rc<RefCell<MapAssetsState>>, kind: &str) {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let kind_for = String::from(kind);
    let label = kit::t("@add").to_string();
    map_assets_open_file(
        dialog,
        &label,
        vec![String::from("*.ogg"), String::from("*.mp3")],
        true,
        Rc::new(move |paths: Vec<String>| {
            for path in paths {
                let result = editor_call(
                    "data_file_add",
                    &[
                        GString::from(kind_for.as_str()).to_variant(),
                        GString::from(path.as_str()).to_variant(),
                    ],
                )
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .unwrap_or_default();
                if !dict_bool(&result, "ok", false) {
                    map_assets_error(&dict_string(&result, "error"));
                }
            }
            map_assets_refetch(&state_for);
            map_assets_rebuild(&dialog_for, &state_for);
        }),
    );
}

// ---------------------------------------------------------------------------
// SchematicInfoDialog (`schematic_info`)
// ---------------------------------------------------------------------------

/// `SchematicInfoDialog.show`: the `[[Schematic] name]` title plus the body.
#[derive(Default)]
pub struct SchematicInfoDialog {
    /// The dialog body table (`_body`).
    body: Option<Gd<MindTable>>,
}

impl DialogController for SchematicInfoDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@schematic");
        dialog.should_pause = false;
        dialog.enable_full_dialog(true);
        self.body = Some(dialog.content_table());
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.build(dialog);
    }
}

impl SchematicInfoDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        let Some(mut body) = self.body.clone() else {
            return;
        };
        body.bind_mut().clear();
        let schematics = dialog.campaign_section("schematics");
        let index = dialog
            .context_value("index")
            .map(|value| variant_i64(&value))
            .unwrap_or(-1);
        let schematic = find_schematic(&schematics, index);
        if schematic.is_empty() {
            return;
        }
        // Java: `"[[" + Core.bundle.get("schematic") + "] " + name`. The title
        // label is a plain `Label`, so the escape renders as the literal
        // `[Schematic] Name` Mindustry shows.
        let title = format!(
            "[{}] {}",
            kit::t("@schematic"),
            dict_string(&schematic, "name")
        );
        dialog.title_text = GString::from(title.as_str());
        dialog.title_color = styles::ACCENT;
        dialog.refresh_title();

        let mut info = factory::table();
        info.bind_mut().grow_horizontal();
        // `Core.bundle.format("schematic.info", w, h, tiles.size)`.
        let tiles = schematic
            .get("tiles")
            .map(|value| {
                value
                    .try_to::<VarArray>()
                    .map(|array| array.len())
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let text = kit::tm_format(
            "@schematic.info",
            &[
                &dict_i64(&schematic, "width", 0).to_string(),
                &dict_i64(&schematic, "height", 0).to_string(),
                &tiles.to_string(),
            ],
        )
        .to_string();
        let label = factory::label(&text);
        info.bind_mut().add(&label).grow_x();
        info.bind_mut().row();

        // code-instantiated: the preview is bound to the selected schematic row.
        let mut holder = factory::space(0.0, 320.0);
        holder.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut image = crate::ui::kit::widgets::schematic_image::MindSchematicImage::new_alloc();
        image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        image.bind_mut().setup(schematic.clone());
        holder.add_child(&image);
        info.bind_mut().add(&holder).grow_x().pad(6.0);
        info.bind_mut().row();

        self.build_requirements(&mut info, &schematic);

        let production = dict_f64(&schematic, "power_production", 0.0) * 60.0;
        let consumption = dict_f64(&schematic, "power_consumption", 0.0) * 60.0;
        if production != 0.0 || consumption != 0.0 {
            // code-instantiated: the schematic's power summary row.
            let mut row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 3);
            if production != 0.0 {
                row.add_child(&factory::glyph_label(
                    "power",
                    20,
                    Color::from_rgba(1.0, 0.827, 0.498, 1.0),
                ));
                let mut plus = factory::label(&format!("+{}", auto_fixed(production, 2)));
                plus.add_theme_color_override(
                    "default_color",
                    Color::from_rgba(1.0, 0.827, 0.498, 1.0),
                );
                row.add_child(&plus);
                if consumption != 0.0 {
                    let gap = factory::space(15.0, 0.0);
                    row.add_child(&gap);
                }
            }
            if consumption != 0.0 {
                row.add_child(&factory::glyph_label(
                    "power",
                    20,
                    Color::from_rgba(0.898, 0.329, 0.329, 1.0),
                ));
                let mut minus = factory::label(&format!("-{}", auto_fixed(consumption, 2)));
                minus.add_theme_color_override(
                    "default_color",
                    Color::from_rgba(0.898, 0.329, 0.329, 1.0),
                );
                row.add_child(&minus);
            }
            info.bind_mut().add(&row).pad(4.0);
            info.bind_mut().row();
        }

        let description = dict_string(&schematic, "description");
        if !description.is_empty() {
            let mut wrapped = factory::label(&markup(&format!("[lightgray]{description}")));
            wrapped.set_custom_minimum_size(Vector2::new(500.0, 0.0));
            wrapped.set_autowrap_mode(AutowrapMode::WORD_SMART);
            info.bind_mut().add(&wrapped).pad(8.0);
            info.bind_mut().row();
        }
        body.bind_mut().add(&info).grow_x();
        body.bind_mut().sort_now();
    }
}

impl SchematicInfoDialog {
    /// Requirement rows: 32 px icon + amount, four per row; when a player core
    /// exists and cannot afford the stack the label is `have/amount` in scarlet.
    fn build_requirements(&self, info: &mut Gd<MindTable>, schematic: &VarDictionary) {
        let requirements = schematic
            .get("requirements")
            .and_then(|value| value.try_to::<VarArray>().ok())
            .unwrap_or_default();
        if requirements.is_empty() {
            return;
        }
        let have = core_items();
        // code-instantiated: one requirement cell per schematic stack.
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        let mut count = 0usize;
        for entry in requirements.iter_shared() {
            let Ok(entry) = entry.try_to::<VarArray>() else {
                continue;
            };
            let mut values = entry.iter_shared();
            let item = values
                .next()
                .map(|value| variant_string(&value))
                .unwrap_or_default();
            let amount = values.next().map(|value| variant_i64(&value)).unwrap_or(0);
            let mut icon = TextureRect::new_alloc();
            if let Some(texture) = kit::icon_texture(&format!("item-{item}-ui")) {
                icon.set_texture(&texture);
            }
            icon.set_custom_minimum_size(Vector2::new(32.0, 32.0));
            icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
            row.add_child(&icon);
            let mut label = factory::label(&amount.to_string());
            let available = have
                .get(item.as_str())
                .map(|value| variant_i64(&value))
                .unwrap_or(0);
            if !have.is_empty() && available < amount {
                label.set_text(&GString::from(
                    format!("[scarlet]{available}[]/{amount}").as_str(),
                ));
            }
            row.add_child(&label);
            count += 1;
            if count.is_multiple_of(4) {
                info.bind_mut().add(&row).pad(2.0);
                info.bind_mut().row();
                row = HBoxContainer::new_alloc();
                row.add_theme_constant_override("separation", 4);
            }
        }
        if !count.is_multiple_of(4) {
            info.bind_mut().add(&row).pad(2.0);
            info.bind_mut().row();
        }
    }
}

/// The player core's item counts (`player.core().items`); empty in the menu.
fn core_items() -> VarDictionary {
    let Some(mut host) = kit::sim_host() else {
        return VarDictionary::new();
    };
    if !host.has_method("core_items_json") {
        return VarDictionary::new();
    }
    let value = host.call("core_items_json", &[]);
    parse_json_object(&value.to::<GString>().to_string())
}

/// Arc `Strings.autoFixed(value, max)`: truncate to `max` decimals, drop
/// trailing zeros (`Strings.java:1230`).
fn auto_fixed(value: f64, max_decimals: usize) -> String {
    let mut text = format!("{value:.max_decimals$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text.is_empty() {
        String::from("0")
    } else {
        text
    }
}

/// The view row for `index` (falls back to the first row).
fn find_schematic(schematics: &VarArray, index: i64) -> VarDictionary {
    for row in schematics.iter_shared() {
        let Ok(row) = row.try_to::<VarDictionary>() else {
            continue;
        };
        if dict_i64(&row, "index", -1) == index {
            return row;
        }
    }
    schematics
        .iter_shared()
        .next()
        .and_then(|row| row.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn processor_markup_matches_java() {
        let mut entry = VarDictionary::new();
        entry.set(&"tag".to_variant(), &GString::from("main").to_variant());
        entry.set(&"x".to_variant(), &3i64.to_variant());
        entry.set(&"y".to_variant(), &(-4i64).to_variant());
        let markup = processors_entry_markup(&entry);
        assert!(markup.contains("main"));
        assert!(markup.contains("3, -4"));
    }

    #[test]
    fn banned_keys_are_the_rules_abi() {
        assert_eq!(banned_key("block"), "bannedBlocks");
        assert_eq!(banned_key("unit"), "bannedUnits");
        assert_eq!(banned_title_key("block"), "@bannedblocks");
        assert_eq!(banned_title_key("unit"), "@bannedunits");
    }

    #[test]
    fn asset_type_tables_are_the_abi() {
        assert_eq!(ASSET_TYPES.len(), 6);
        assert_eq!(asset_type_key("patch"), "@asset.patch");
        assert_eq!(asset_type_icon("music"), "music");
        assert_eq!(asset_content_icon("weather"), "effect");
        assert_eq!(ASSET_CONTENT_TYPES.len(), 6);
    }

    #[test]
    fn auto_fixed_drops_trailing_zeros() {
        assert_eq!(auto_fixed(120.0, 2), "120");
        assert_eq!(auto_fixed(1.5, 2), "1.5");
        assert_eq!(auto_fixed(0.0, 2), "0");
    }

    #[test]
    fn integer_validation_matches_godot() {
        assert!(is_valid_int("123"));
        assert!(is_valid_int("-42"));
        assert!(!is_valid_int(""));
        assert!(!is_valid_int("-"));
        assert!(!is_valid_int("1.5"));
        assert!(!is_valid_int("x1"));
    }

    /// PAR-0541: `map_assets_audio_stream_name` reads the row through
    /// `dict_string` (a Godot `VarDictionary`), so calling it from a plain
    /// cargo unit test panics with `Godot engine not available`. The naming
    /// rule itself is pure — `dp-` + the row's `name` with spaces replaced by
    /// underscores, lowercased (`batch_f.rs:4220-4222`) — and the seconds
    /// formatter is pure, so this test exercises both without touching the
    /// engine. The FFI-backed call is covered by the in-engine dialog check.
    #[test]
    fn audio_stream_name_uses_the_dp_prefix() {
        let stream_name = |name: &str| format!("dp-{}", name.replace(' ', "_").to_lowercase());
        assert_eq!(stream_name("My Sound"), "dp-my_sound");
        assert_eq!(stream_name("Ambient"), "dp-ambient");
        assert_eq!(stream_name("Two Words Here"), "dp-two_words_here");
        // A name already lowercase/underscored round-trips unchanged.
        assert_eq!(stream_name("my_sound"), "dp-my_sound");
        // `map_assets_format_seconds` is engine-free (pure formatting).
        assert_eq!(map_assets_format_seconds(61.7), "1:01");
        assert_eq!(map_assets_format_seconds(0.0), "0:00");
        assert_eq!(map_assets_format_seconds(3600.0), "60:00");
    }

    #[test]
    fn count_fields_counts_leaves() {
        let json = r#"{"a": 1, "b": {"c": 2, "d": 3}}"#;
        let parsed = map_assets_parse_json(json);
        assert_eq!(map_assets_count_fields(&parsed), 3);
        assert_eq!(map_assets_count_fields(&VarArray::new().to_variant()), 1);
    }

    #[test]
    fn schematics_fall_back_to_the_first_row() {
        let mut first = VarDictionary::new();
        first.set(&"index".to_variant(), &7i64.to_variant());
        let mut schematics = VarArray::new();
        schematics.push(&first.to_variant());
        let picked = find_schematic(&schematics, 99);
        assert_eq!(dict_i64(&picked, "index", -1), 7);
    }

    #[test]
    fn error_codes_map_to_bundle_keys() {
        assert_eq!(
            map_assets_error_key("exists"),
            Some("@asset.content.exists")
        );
        assert_eq!(
            map_assets_error_key("badname"),
            Some("@asset.content.badname")
        );
        assert_eq!(map_assets_error_key("badtype"), Some("@asset.content.type"));
        assert_eq!(
            map_assets_error_key("invalid JSON"),
            Some("@patch.importerror")
        );
        assert_eq!(map_assets_error_key("other"), None);
    }
}
