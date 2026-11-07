// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/SettingsMenuDialog.java,
//         core/src/mindustry/ui/dialogs/LanguageDialog.java,
//         core/src/mindustry/ui/dialogs/KeybindDialog.java,
//         core/src/mindustry/ui/dialogs/EffectsDialog.java,
//         core/src/mindustry/ui/dialogs/DatabaseDialog.java,
//         core/src/mindustry/ui/dialogs/ContentInfoDialog.java
//         (plan 14 §3.4, plan 15 §6.1).

//! Dialog controllers batch A: settings, language, controls (rebind),
//! effects, database and content info.
//!
//! Each controller ports one `client/ui/dialogs/*.gd` script. The render code
//! lives in free functions of this module because callbacks (button presses,
//! text changes, the confirm signal, the key/mouse capture overlay) are
//! `Callable::from_fn` closures: they capture the dialog handle and re-enter
//! the controller through [`MindDialog::refresh_controller`] with a request
//! recorded in node metadata ([`REQUEST_META`]). Panels that a closure has to
//! repaint in place (settings pages, the keybind list, the database grid) are
//! therefore rebuilt by `shown` from that request, which keeps every control
//! flow of the GDScript original.

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::display_server::FileDialogMode;
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::ExpandMode;
use godot::classes::{
    Button, ButtonGroup, CenterContainer, ColorRect, Control, DisplayServer, GridContainer,
    HBoxContainer, InputEvent, InputEventKey, InputEventMouseButton, Json, Label, LineEdit,
    MarginContainer, Os, PanelContainer, PopupMenu, RichTextLabel, StyleBoxEmpty, StyleBoxFlat,
    StyleBoxTexture, Time, Timer, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::check::MindCheck;
use super::super::kit::widgets::{factory, tooltip};
use super::super::kit::{self, Align, MindLabel, MindScroll, MindTable, styles};

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "settings" => Some(Box::new(SettingsController)),
        "language" => Some(Box::new(LanguageController)),
        "controls" => Some(Box::new(ControlsController::default())),
        "effects" => Some(Box::new(EffectsController)),
        "database" => Some(Box::new(DatabaseController::default())),
        "content" => Some(Box::new(ContentController::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Shared plumbing
// ---------------------------------------------------------------------------

/// Meta key carrying the request `shown` dispatches on.
const REQUEST_META: &str = "mind_dialog_request";
/// Meta key carrying the `confirm_result` answer until the deferred `shown`.
const CONFIRM_META: &str = "mind_dialog_confirm";

/// Request re-running `shown` synchronously with `request` recorded.
fn request_refresh(node: &Gd<MindDialog>, request: &str) {
    let mut target = node.clone();
    target.set_meta(REQUEST_META, &GString::from(request).to_variant());
    let _ = target.call("refresh_controller", &[]);
}

/// Deferred [`request_refresh`] for callbacks that run inside a `MindUi` borrow.
fn request_refresh_deferred(node: &Gd<MindDialog>, request: &str) {
    let mut target = node.clone();
    target.set_meta(REQUEST_META, &GString::from(request).to_variant());
    let _ = target.call_deferred("refresh_controller", &[]);
}

/// Reads and clears the pending request (`""` when none is queued).
fn take_request(dialog: &mut MindDialog) -> String {
    let mut node = dialog.to_gd();
    if !node.has_meta(REQUEST_META) {
        return String::new();
    }
    let request = variant_text(&node.get_meta(REQUEST_META));
    node.remove_meta(REQUEST_META);
    request
}

fn meta_text(node: &Gd<MindDialog>, key: &str) -> String {
    if !node.has_meta(key) {
        return String::new();
    }
    variant_text(&node.get_meta(key))
}

fn set_meta_text(node: &mut Gd<MindDialog>, key: &str, value: &str) {
    node.set_meta(key, &GString::from(value).to_variant());
}

fn meta_int(node: &Gd<MindDialog>, key: &str, fallback: i64) -> i64 {
    if !node.has_meta(key) {
        return fallback;
    }
    as_int(Some(node.get_meta(key)))
}

fn set_meta_int(node: &mut Gd<MindDialog>, key: &str, value: i64) {
    node.set_meta(key, &value.to_variant());
}

fn meta_bool(node: &Gd<MindDialog>, key: &str) -> bool {
    node.has_meta(key) && as_bool(Some(node.get_meta(key)))
}

/// GDScript `str(value)` for a variant (`<null>` for null, like Godot).
fn variant_text(value: &Variant) -> String {
    value
        .try_to::<GString>()
        .map(|text| text.to_string())
        .unwrap_or_else(|_| value.stringify().to_string())
}

/// GDScript `int(value)` for a variant.
fn as_int(value: Option<Variant>) -> i64 {
    let Some(value) = value else {
        return 0;
    };
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return i64::from(flag);
    }
    0
}

/// GDScript `float(value)` for a variant.
fn as_float(value: Option<Variant>) -> f64 {
    let Some(value) = value else {
        return 0.0;
    };
    if let Ok(number) = value.try_to::<f64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number as f64;
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return if flag { 1.0 } else { 0.0 };
    }
    0.0
}

/// GDScript `bool(value)` for a variant.
fn as_bool(value: Option<Variant>) -> bool {
    let Some(value) = value else {
        return false;
    };
    value.try_to::<bool>().unwrap_or(false)
}

fn dict_text(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_text(&value))
        .unwrap_or_default()
}

fn dict_int(dict: &VarDictionary, key: &str) -> i64 {
    as_int(dict.get(key))
}

fn dict_float(dict: &VarDictionary, key: &str, fallback: f32) -> f32 {
    match dict.get(key) {
        Some(value) => as_float(Some(value)) as f32,
        None => fallback,
    }
}

fn dict_bool(dict: &VarDictionary, key: &str) -> bool {
    as_bool(dict.get(key))
}

fn dict_array(dict: &VarDictionary, key: &str) -> VarArray {
    match dict.get(key) {
        Some(value) => value.try_to::<VarArray>().unwrap_or_default(),
        None => VarArray::new(),
    }
}

#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn dict_dict(dict: &VarDictionary, key: &str) -> VarDictionary {
    match dict.get(key) {
        Some(value) => value.try_to::<VarDictionary>().unwrap_or_default(),
        None => VarDictionary::new(),
    }
}

/// `JSON.parse_string` of a UI JSON payload (`[]` when it is not an array).
fn json_array(text: &GString) -> VarArray {
    if text.is_empty() {
        return VarArray::new();
    }
    Json::parse_string(text)
        .try_to::<VarArray>()
        .unwrap_or_default()
}

/// `JSON.parse_string` of a UI JSON payload (`{}` when it is not an object).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
fn json_object(text: &GString) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(text)
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `MindUi.settings_categories_json` rows.
fn load_categories() -> Vec<VarDictionary> {
    let Some(mut ui) = kit::ui() else {
        return Vec::new();
    };
    if !ui.has_method("settings_categories_json") {
        return Vec::new();
    }
    let text = ui.call("settings_categories_json", &[]).to::<GString>();
    json_array(&text)
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// `MindUi.settings_rows_json(category)` rows.
fn load_rows(category_id: &str) -> Vec<VarDictionary> {
    let Some(mut ui) = kit::ui() else {
        return Vec::new();
    };
    if !ui.has_method("settings_rows_json") {
        return Vec::new();
    }
    let text = ui
        .call(
            "settings_rows_json",
            &[GString::from(category_id).to_variant()],
        )
        .to::<GString>();
    json_array(&text)
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// `Bundle.get(key)` with the raw-key fallback of the GDScript `_bundle_or`.
fn bundle_or(key: &str, fallback: &str) -> String {
    let value = kit::t(key).to_string();
    if value == key || value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

/// `MindDialog.open_content_info` has no native base method; every content
/// link in this batch routes through this helper.
pub fn open_content_info(dialog: &Gd<MindDialog>, content: &str) {
    if content.is_empty() {
        return;
    }
    let mut context = VarDictionary::new();
    context.set("content", &GString::from(content).to_variant());
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from("content").to_variant(),
                Json::stringify(&context.to_variant()).to_variant(),
            ],
        );
    }
    let _ = dialog;
}

/// The dialog's viewport width (`get_viewport_rect().size.x`).
fn viewport_width(dialog: &MindDialog) -> f32 {
    dialog.base().get_viewport_rect().size.x
}

/// A flat `StyleBoxFlat` with a solid fill (`_flat_box`).
fn flat_box(fill: Color) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(fill);
    box_
}

// ---------------------------------------------------------------------------
// SettingsController (`settings`)
// ---------------------------------------------------------------------------

/// `SettingsMenuDialog.DATA_ACTIONS` (desktop order).
const DATA_ACTIONS: [(&str, &str, &str); 9] = [
    ("clear-data", "@settings.cleardata", "trash"),
    ("clear-planet-data", "@settings.clearplanetdata", "trash"),
    ("clear-saves", "@settings.clearsaves", "trash"),
    ("clear-research", "@settings.clearresearch", "trash"),
    (
        "clear-campaign-saves",
        "@settings.clearcampaignsaves",
        "trash",
    ),
    ("export-data", "@data.export", "upload"),
    ("import-data", "@data.import", "download"),
    ("open-folder", "@data.openfolder", "folder"),
    ("export-crash-logs", "@crash.export", "upload"),
];

/// Upstream confirm text for the destructive data actions.
const DATA_CONFIRMS: [(&str, &str); 5] = [
    ("clear-data", "@settings.clearall.confirm"),
    ("clear-saves", "@settings.clearsaves.confirm"),
    ("clear-research", "@settings.clearresearch.confirm"),
    (
        "clear-campaign-saves",
        "@settings.clearcampaignsaves.confirm",
    ),
    ("import-data", "@data.import.confirm"),
];

/// Page ids that are not settings tables.
const PAGE_MENU: &str = "";
const PAGE_DATA: &str = "data";
const PAGE_PLANET_DATA: &str = "planet-data";

/// Meta keys holding the controller state that callbacks must reach.
const SETTINGS_PAGE_META: &str = "mind_settings_page";
const SETTINGS_PLANET_META: &str = "mind_settings_planet";
const SETTINGS_PENDING_META: &str = "mind_settings_pending";
const SETTINGS_UISCALE_META: &str = "mind_settings_uiscale_start";
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
const SETTINGS_KEYHOOK_META: &str = "mind_settings_keyhook";
const SETTINGS_CONFIRM_META: &str = "mind_settings_confirm_bound";

/// `SettingsMenuDialog.REQUEST_*`: the pending controller action.
const SETTINGS_REQUEST_RENDER: &str = "settings-render";
const SETTINGS_REQUEST_BACK: &str = "settings-back";
const SETTINGS_REQUEST_CONFIRM: &str = "settings-confirm";

/// `menu.defaults().size(300f, 60f)` (`rebuildMenu`).
const MENU_BUTTON_SIZE: Vector2 = Vector2::new(300.0, 60.0);
/// `t.defaults().size(280f, 60f)` (`dataDialog`/`planetDataDialog`).
const DATA_BUTTON_SIZE: Vector2 = Vector2::new(280.0, 60.0);
/// `float marg = 8f` (menu rows) / `.marginLeft(4)` (data rows).
const MENU_MARGIN_LEFT: f32 = 8.0;
const DATA_MARGIN_LEFT: f32 = 4.0;
/// `isize = iconMed` (`Scl.scl(32f)`) and the glyph size filling it.
const ICON_CELL: f32 = 32.0;
const ICON_FONT_SIZE: i32 = 28;
/// `prefs.margin(14f)`.
const PREFS_MARGIN: i32 = 14;
/// The pane's inset inside the shared shell body (see [`setup_prefs`]).
const PANE_INSET_TOP: f32 = 12.0;
const PANE_INSET_BOTTOM: f32 = 3.0;
/// `Math.min(Core.graphics.getWidth() / 1.2f / Scl.scl(1f), 500f)`.
const ROW_MAX_WIDTH: f32 = 500.0;
/// `Styles.flatOver` (`whiteui.tint(#454545)`); `flatt.up = black`.
const FLAT_OVER: Color = Color::from_rgba(0.27, 0.27, 0.27, 1.0);
/// `Tex.button` ninepatch splits and the reference row padding.
const BUTTON_SPLIT: f32 = 12.0;
const BUTTON_PAD: f32 = 11.0;
/// The default reset-button label (`bundle.get("settings.reset", …)`).
const RESET_DEFAULT_TEXT: &str = "Reset to Defaults";

/// `@settings` — the category rail, each `SettingsTable` and the data/planet
/// action pages, rendered in the shared dialog's scroll body.
#[derive(Default)]
pub struct SettingsController;

impl DialogController for SettingsController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@settings");
        dialog.should_pause = true;
        // `BaseDialog.setFillParent(true)` over `Styles.defaultDialog`.
        dialog.enable_full_dialog(true);
        let mut node = dialog.to_gd();
        set_meta_text(&mut node, SETTINGS_PAGE_META, PAGE_MENU);
        set_meta_text(&mut node, SETTINGS_PLANET_META, "");
        set_meta_text(&mut node, SETTINGS_PENDING_META, "");
        set_meta_int(&mut node, SETTINGS_UISCALE_META, -1);
        connect_confirm(&node);
        setup_prefs(dialog);
        dialog.clear_buttons();
        add_back_button(dialog);
        install_escape_hook(dialog);
        show_menu(dialog);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let request = take_request(dialog);
        match request.as_str() {
            SETTINGS_REQUEST_BACK => back(dialog),
            SETTINGS_REQUEST_CONFIRM => confirm(dialog),
            SETTINGS_REQUEST_RENDER => render_page(dialog),
            // `SettingsMenuDialog.shown`: `back(); rebuildMenu();`.
            _ => show_menu(dialog),
        }
    }

    /// `keyDown(escape|back)`: never close from a category page.
    fn blocks_escape(&self) -> bool {
        true
    }
}

impl SettingsController {
    /// `SettingsMenuDialog.back()`: a category page goes back to the menu; on
    /// the menu the dialog closes (`_close_pressed`).
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn back(&self, dialog: &mut MindDialog) {
        back(dialog);
    }

    /// `SettingsMenuDialog._show_table` for one category id.
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub fn show_table(&self, dialog: &mut MindDialog, category_id: &str) {
        show_table(dialog, category_id);
    }

    /// `SettingsMenuDialog.currentPage` (`_page`).
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub fn page(&self, dialog: &MindDialog) -> String {
        meta_text(&dialog.to_gd(), SETTINGS_PAGE_META)
    }
}

/// `SettingsMenuDialog._setup_prefs`: the pane fills the shell body and the
/// removed `cont` cell keeps its `expand()`, so a short page centres.
fn setup_prefs(dialog: &mut MindDialog) {
    if let Some(mut body) = dialog.body() {
        body.set_horizontal_scroll_mode(ScrollMode::DISABLED);
        let mut inset = StyleBoxEmpty::new_gd();
        inset.set_content_margin(Side::TOP, PANE_INSET_TOP);
        inset.set_content_margin(Side::BOTTOM, PANE_INSET_BOTTOM);
        body.add_theme_stylebox_override("panel", &inset);
    }
    if let Some(mut content) = dialog.content() {
        content.set_v_size_flags(SizeFlags::EXPAND_FILL);
        content.set_alignment(AlignmentMode::CENTER);
    }
}

/// `addCloseButton` override: `@back` returns to the menu from a category page.
fn add_back_button(dialog: &mut MindDialog) {
    let mut button = dialog.add_close_button(210.0);
    button.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    let close = dialog.base().callable("close_pressed");
    if button.is_connected("pressed", &close) {
        button.disconnect("pressed", &close);
    }
    let target = dialog.to_gd();
    let callable = Callable::from_fn("settings_back", move |_args: &[&Variant]| {
        request_refresh(&target, SETTINGS_REQUEST_BACK);
    });
    button.connect("pressed", &callable);
}

/// Escape/Back hook: the shared shell owns the close key, so a key catcher
/// forwards it to the settings back navigation on category pages.
fn install_escape_hook(_dialog: &mut MindDialog) {
    // The per-dialog ESC key catcher was not ported: `MindUiRoot` closes the
    // top dialog on ESC, and settings back-navigation happens through the
    // dialog's own Back button.
}

/// `_connect_confirm`: `MindUi.confirm_result` -> the settings confirm flow.
fn connect_confirm(node: &Gd<MindDialog>) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal("confirm_result") || node.has_meta(SETTINGS_CONFIRM_META) {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("settings_confirm", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let mut dialog = target.clone();
        dialog.set_meta(CONFIRM_META, &confirmed.to_variant());
        // The signal is emitted inside a `MindUi` mutable borrow.
        request_refresh_deferred(&target, SETTINGS_REQUEST_CONFIRM);
    });
    ui.connect("confirm_result", &callable);
    node.clone()
        .set_meta(SETTINGS_CONFIRM_META, &true.to_variant());
}

/// `_on_back` (`_page` dispatch).
fn back(dialog: &mut MindDialog) {
    let page = meta_text(&dialog.to_gd(), SETTINGS_PAGE_META);
    match page.as_str() {
        PAGE_MENU => dialog.close(),
        PAGE_PLANET_DATA => show_data(dialog),
        _ => show_menu(dialog),
    }
}

/// Re-renders the page recorded in [`SETTINGS_PAGE_META`].
fn render_page(dialog: &mut MindDialog) {
    let page = meta_text(&dialog.to_gd(), SETTINGS_PAGE_META);
    match page.as_str() {
        PAGE_MENU => show_menu(dialog),
        PAGE_DATA => show_data(dialog),
        PAGE_PLANET_DATA => show_planet_data(dialog),
        category => show_table(dialog, category),
    }
}

/// `_on_confirm(confirmed)`: runs the pending destructive action.
fn confirm(dialog: &mut MindDialog) {
    let mut node = dialog.to_gd();
    let confirmed = meta_bool(&node, CONFIRM_META);
    node.remove_meta(CONFIRM_META);
    let pending = meta_text(&node, SETTINGS_PENDING_META);
    set_meta_text(&mut node, SETTINGS_PENDING_META, "");
    if pending.is_empty() || !confirmed {
        return;
    }
    match pending.as_str() {
        "import-data" => pick_file("@data.import", "", true, &["*.zip"], PickKind::ImportData),
        "planet-research" => campaign_action(&node, "clear_planet_research"),
        "planet-campaign-saves" => campaign_action(&node, "clear_planet_campaign_saves"),
        other => run_settings_action(other),
    }
}

/// `_set_page`: replaces the pane content and resets the title.
fn set_page(dialog: &mut MindDialog, page_id: &str, title_key: &str, page: Gd<Control>) {
    set_meta_text(&mut dialog.to_gd(), SETTINGS_PAGE_META, page_id);
    dialog.set_title_key(title_key);
    dialog.clear_content();
    // code-instantiated: the pane holds one runtime-built page at a time
    // (menu / category table / data actions), swapped on every navigation.
    let mut margin = MarginContainer::new_alloc();
    margin.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    for side in ["left", "top", "right", "bottom"] {
        margin.add_theme_constant_override(format!("margin_{side}").as_str(), PREFS_MARGIN);
    }
    margin.add_child(&page);
    if let Some(mut content) = dialog.content() {
        let control: Gd<Control> = margin.clone().upcast();
        content.add_child(&control);
    }
    if let Some(mut body) = dialog.body() {
        body.set_v_scroll(0);
    }
}

/// `back()` + `rebuildMenu()`: the category menu (`SettingsMenuDialog.java:336-365`).
fn show_menu(dialog: &mut MindDialog) {
    // code-instantiated: menu rows follow the model categories.
    let mut rows = VBoxContainer::new_alloc();
    rows.add_theme_constant_override("separation", 0);
    for category in load_categories() {
        let icon = dict_text(&category, "icon");
        let key = dict_text(&category, "key");
        let label = kit::t(&format!("@{key}")).to_string();
        let mut button = flat_button(&icon, &label, MENU_BUTTON_SIZE, MENU_MARGIN_LEFT, false);
        let target = dialog.to_gd();
        let action = dict_text(&category, "action");
        let id = dict_text(&category, "id");
        let callable = Callable::from_fn("settings_category", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            on_category_pressed(&mut bound, &action, &id);
        });
        button.connect("pressed", &callable);
        rows.add_child(&button);
    }
    let page: Gd<Control> = button_panel(rows).upcast();
    set_page(dialog, PAGE_MENU, "@settings", page);
}

/// `_on_category_pressed`: table / language / controls / data.
fn on_category_pressed(dialog: &mut MindDialog, action: &str, id: &str) {
    match action {
        "table" => show_table(dialog, id),
        "language" | "controls" => {
            if let Some(mut ui) = kit::ui() {
                let _ = ui.call(
                    "open_dialog",
                    &[
                        GString::from(action).to_variant(),
                        GString::new().to_variant(),
                    ],
                );
            }
        }
        "data" => show_data(dialog),
        _ => {}
    }
}

/// `visible(index)`: one category's `SettingsTable` (`SettingsTable.rebuild`).
fn show_table(dialog: &mut MindDialog, category_id: &str) {
    // code-instantiated: setting rows are data-driven from the model and the
    // shared store; the row count and keys are not known at scene build time.
    let mut table = VBoxContainer::new_alloc();
    table.add_theme_constant_override("separation", 0);
    table.set_custom_minimum_size(Vector2::new(row_width(dialog), 0.0));
    for row in load_rows(category_id) {
        add_row(dialog, &mut table, &row, category_id);
    }
    add_reset_button(dialog, &mut table, category_id);
    let page: Gd<Control> = table.upcast();
    set_page(dialog, category_id, "@settings", page);
}

/// `Math.min(Core.graphics.getWidth() / 1.2f / Scl.scl(1f), 500f)`.
fn row_width(dialog: &MindDialog) -> f32 {
    ROW_MAX_WIDTH.min(viewport_width(dialog) / 1.2)
}

/// `Setting(name)`: `setting.<name>.name` (raw key fallback).
fn setting_title(key: &str) -> String {
    bundle_or(&format!("setting.{key}.name"), key)
}

/// The optional `setting.<name>.description` tooltip (`ui.addDescTooltip`).
fn setting_description(key: &str) -> String {
    bundle_or(&format!("setting.{key}.description"), "")
}

/// `_add_row`: one `CheckSetting` or `SliderSetting` row.
fn add_row(
    dialog: &mut MindDialog,
    table: &mut Gd<VBoxContainer>,
    row: &VarDictionary,
    category_id: &str,
) {
    let key = dict_text(row, "key");
    if key.is_empty() {
        return;
    }
    let title = setting_title(&key);
    match dict_text(row, "kind").as_str() {
        "check" => {
            // `CheckSetting.add`: `minWidth(min(500, w/1.2)).fillX()
            // .height(45f).left().padTop(7f)`.
            table.add_child(&factory::space(0.0, 7.0));
            let checked = dict_bool(row, "value");
            let key_for_toggle = key.clone();
            let sync_audio = category_id == "sound";
            let callable = Callable::from_fn("settings_check", move |args: &[&Variant]| {
                let value = args
                    .first()
                    .map(|entry| entry.to::<bool>())
                    .unwrap_or(false);
                set_value(&key_for_toggle, value.to_variant(), sync_audio);
            });
            // code-instantiated: one themed check per settings row.
            let mut check = MindCheck::new_alloc();
            check
                .bind_mut()
                .setup(GString::from(title.as_str()), checked, callable);
            check.set_custom_minimum_size(Vector2::new(row_width(dialog), 45.0));
            check.set_h_size_flags(SizeFlags::EXPAND_FILL);
            check.set_tooltip_text(&GString::from(setting_description(&key).as_str()));
            table.add_child(&check);
        }
        "slider" => add_slider_row(dialog, table, row, &key, &title, category_id),
        _ => {}
    }
}

/// `SliderSetting.add`: the slider with a title/value header drawn over it.
fn add_slider_row(
    dialog: &mut MindDialog,
    table: &mut Gd<VBoxContainer>,
    row: &VarDictionary,
    key: &str,
    title: &str,
    category_id: &str,
) {
    let value = dict_int(row, "value");
    // `int[] lastUiScale = {settings.getInt("uiscale", 100)}` is captured once.
    let mut node = dialog.to_gd();
    if key == "uiscale" && meta_int(&node, SETTINGS_UISCALE_META, -1) < 0 {
        set_meta_int(&mut node, SETTINGS_UISCALE_META, value);
    }
    let uiscale_start = meta_int(&node, SETTINGS_UISCALE_META, -1);
    let width = row_width(dialog);
    table.add_child(&factory::space(0.0, 4.0));
    // code-instantiated: the stack/value header is data-driven per row.
    let mut stack = MarginContainer::new_alloc();
    stack.set_custom_minimum_size(Vector2::new(width, 0.0));
    let mut slider = factory::slider(
        dict_float(row, "min", 0.0) as f64,
        dict_float(row, "max", 100.0) as f64,
        dict_float(row, "step", 1.0) as f64,
    );
    slider.set_value(value as f64);
    slider.set_tooltip_text(&GString::from(setting_description(key).as_str()));
    slider.set_h_size_flags(SizeFlags::EXPAND_FILL);
    slider.set_v_size_flags(SizeFlags::EXPAND_FILL);
    stack.add_child(&slider);
    let mut content = MarginContainer::new_alloc();
    content.set_mouse_filter(MouseFilter::IGNORE);
    content.add_theme_constant_override("margin_left", 33);
    content.add_theme_constant_override("margin_right", 33);
    content.add_theme_constant_override("margin_top", 3);
    content.add_theme_constant_override("margin_bottom", 3);
    let mut header = HBoxContainer::new_alloc();
    header.set_mouse_filter(MouseFilter::IGNORE);
    let mut heading = factory::styled_label(title, "outlineLabel");
    heading.set_autowrap_mode(AutowrapMode::WORD_SMART);
    heading.set_vertical_alignment(VerticalAlignment::CENTER);
    heading.set_h_size_flags(SizeFlags::EXPAND_FILL);
    heading.set_mouse_filter(MouseFilter::IGNORE);
    header.add_child(&heading);
    let mut value_label = factory::styled_label(&format_value(row, value), "outlineLabel");
    value_label.set_autowrap_mode(AutowrapMode::OFF);
    value_label.set_vertical_alignment(VerticalAlignment::CENTER);
    value_label.set_mouse_filter(MouseFilter::IGNORE);
    header.add_child(&value_label);
    content.add_child(&header);
    stack.add_child(&content);

    let row_for_label = row.clone();
    let label_for_change = value_label.clone();
    let key_for_change = key.to_string();
    let sync_audio = category_id == "sound";
    let is_uiscale = key == "uiscale";
    let callable = Callable::from_fn("settings_slider", move |args: &[&Variant]| {
        let number = as_float(args.first().map(|entry| (*entry).clone())) as i64;
        let mut label = label_for_change.clone();
        label.set_text(&GString::from(
            format_value(&row_for_label, number).as_str(),
        ));
        set_value(&key_for_change, number.to_variant(), sync_audio);
        // `SettingsMenuDialog.java:432-438`: flag the change so the next boot
        // confirms the scale (`Control.java` prompt).
        if is_uiscale
            && uiscale_start >= 0
            && let Some(mut ui) = kit::ui()
        {
            let _ = ui.call(
                "set_uiscale_changed",
                &[(number != uiscale_start).to_variant()],
            );
        }
    });
    slider.connect("value_changed", &callable);
    table.add_child(&stack);
}

/// `button(bundle.get("settings.reset", "Reset to Defaults"), …)`.
fn add_reset_button(dialog: &mut MindDialog, table: &mut Gd<VBoxContainer>, category_id: &str) {
    table.add_child(&factory::space(0.0, 6.0));
    let mut button = factory::button(&bundle_or("settings.reset", RESET_DEFAULT_TEXT));
    button.set_custom_minimum_size(Vector2::new(240.0, 54.0));
    button.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    let target = dialog.to_gd();
    let category = category_id.to_string();
    let callable = Callable::from_fn("settings_reset", move |_args: &[&Variant]| {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call(
                "settings_reset",
                &[GString::from(category.as_str()).to_variant()],
            );
        }
        if category == "sound" {
            sync_audio_defaults();
        }
        let mut node = target.clone();
        set_meta_text(&mut node, SETTINGS_PAGE_META, &category);
        request_refresh(&target, SETTINGS_REQUEST_RENDER);
    });
    button.connect("pressed", &callable);
    table.add_child(&button);
    table.add_child(&factory::space(0.0, 6.0));
}

/// The audio mixer keeps its own live settings mirror; after a Sound reset push
/// the restored values to it (upstream reads `Core.settings` directly).
fn sync_audio_defaults() {
    let Some(mut audio) = kit::node("/root/Spine/MindAudio") else {
        return;
    };
    for row in load_rows("sound") {
        let mut value = row.get("value").unwrap_or_else(Variant::nil);
        if dict_text(&row, "kind") == "slider" {
            value = as_int(Some(value)).to_variant();
        }
        let _ = audio.call(
            "set_setting",
            &[
                GString::from(dict_text(&row, "key").as_str()).to_variant(),
                value,
            ],
        );
    }
}

/// `dataDialog` (`SettingsMenuDialog.java:172-292`), shown in place.
fn show_data(dialog: &mut MindDialog) {
    // code-instantiated: data actions are a data-driven list.
    let mut rows = VBoxContainer::new_alloc();
    rows.add_theme_constant_override("separation", 0);
    for (id, text, icon) in DATA_ACTIONS {
        let mut button = flat_button(
            icon,
            &kit::t(text).to_string(),
            DATA_BUTTON_SIZE,
            DATA_MARGIN_LEFT,
            false,
        );
        let target = dialog.to_gd();
        let callable = Callable::from_fn("settings_data_action", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            run_data_action(&mut bound, id);
        });
        button.connect("pressed", &callable);
        rows.add_child(&button);
    }
    let page: Gd<Control> = button_panel(rows).upcast();
    set_page(dialog, PAGE_DATA, "@settings.data", page);
}

/// `_run_data_action(action_id)`.
fn run_data_action(dialog: &mut MindDialog, action_id: &str) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    match action_id {
        "clear-planet-data" => show_planet_data(dialog),
        "export-data" => pick_file(
            "@data.export",
            "mindustry-data-export.zip",
            false,
            &["*.zip"],
            PickKind::ExportData,
        ),
        "export-crash-logs" => {
            let available = ui
                .call("crash_logs_available", &[])
                .try_to::<bool>()
                .unwrap_or(false);
            if available {
                pick_file(
                    "@crash.export",
                    "logs.txt",
                    false,
                    &["*.txt"],
                    PickKind::ExportCrashLogs,
                );
            } else {
                let _ = ui.call("show_info", &[kit::t("@crash.none").to_variant()]);
            }
        }
        "import-data"
        | "clear-data"
        | "clear-saves"
        | "clear-research"
        | "clear-campaign-saves" => {
            // Destructive: confirm first (upstream `ui.showConfirm`).
            set_meta_text(&mut dialog.to_gd(), SETTINGS_PENDING_META, action_id);
            let confirm_key = DATA_CONFIRMS
                .iter()
                .find(|(id, _)| *id == action_id)
                .map(|(_, key)| *key)
                .unwrap_or("");
            let _ = ui.call("show_confirm", &[kit::t(confirm_key).to_variant()]);
        }
        other => run_settings_action(other),
    }
}

/// `ui.call("settings_action", action_id)`.
fn run_settings_action(action_id: &str) {
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call("settings_action", &[GString::from(action_id).to_variant()]);
    }
}

/// `planetDataDialog` (`SettingsMenuDialog.java:93-170`), shown in place.
fn show_planet_data(dialog: &mut MindDialog) {
    let planets = selectable_planets();
    if planets.is_empty() {
        show_data(dialog);
        return;
    }
    let mut node = dialog.to_gd();
    let mut planet_name = meta_text(&node, SETTINGS_PLANET_META);
    if !planets
        .iter()
        .any(|row| dict_text(row, "name") == planet_name)
    {
        planet_name = planets
            .first()
            .map(|row| dict_text(row, "name"))
            .unwrap_or_default();
        set_meta_text(&mut node, SETTINGS_PLANET_META, &planet_name);
    }
    // code-instantiated: planet entries are runtime content data, not scene nodes.
    let mut rows = VBoxContainer::new_alloc();
    rows.add_theme_constant_override("separation", 0);
    let mut select = flat_button(
        "planet",
        &planet_select_text(&planets, &planet_name),
        DATA_BUTTON_SIZE,
        DATA_MARGIN_LEFT,
        true,
    );
    let target = dialog.to_gd();
    let callable = Callable::from_fn("settings_planet_select", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        open_planet_menu(&mut bound);
    });
    select.connect("pressed", &callable);
    rows.add_child(&select);
    let mut research = flat_button(
        "trash",
        &kit::t("@settings.clearplanetresearch").to_string(),
        DATA_BUTTON_SIZE,
        DATA_MARGIN_LEFT,
        false,
    );
    let target = dialog.to_gd();
    let callable = Callable::from_fn("settings_planet_research", move |_args: &[&Variant]| {
        confirm_planet_research(&target);
    });
    research.connect("pressed", &callable);
    rows.add_child(&research);
    let mut saves = flat_button(
        "trash",
        &kit::t("@settings.clearplanetcampaignsaves").to_string(),
        DATA_BUTTON_SIZE,
        DATA_MARGIN_LEFT,
        false,
    );
    let target = dialog.to_gd();
    let callable = Callable::from_fn("settings_planet_saves", move |_args: &[&Variant]| {
        confirm_planet_campaign_saves(&target);
    });
    saves.connect("pressed", &callable);
    rows.add_child(&saves);
    let page: Gd<Control> = button_panel(rows).upcast();
    set_page(dialog, PAGE_PLANET_DATA, "@settings.data", page);
}

/// `_open_planet_menu`: the planet popup (`PopupMenu`).
fn open_planet_menu(dialog: &mut MindDialog) {
    let planets = selectable_planets();
    // code-instantiated: planet entries are runtime data.
    let mut menu = PopupMenu::new_alloc();
    for (index, planet) in planets.iter().enumerate() {
        let name = dict_text(planet, "name");
        menu.add_item(&kit::t(&format!("@planet.{name}.name")));
        menu.set_item_id(index as i32, index as i32);
    }
    let target = dialog.to_gd();
    let planets_for_pick = planets.clone();
    let callable = Callable::from_fn("settings_planet_picked", move |args: &[&Variant]| {
        let index = as_int(args.first().map(|entry| (*entry).clone()));
        let Some(row) = planets_for_pick.get(index.max(0) as usize) else {
            return;
        };
        let mut node = target.clone();
        set_meta_text(&mut node, SETTINGS_PLANET_META, &dict_text(row, "name"));
        let mut bound = node.bind_mut();
        show_planet_data(&mut bound);
    });
    menu.connect("id_pressed", &callable);
    let menu_for_close = menu.clone();
    let close = Callable::from_fn("settings_planet_menu_close", move |_args: &[&Variant]| {
        let mut menu = menu_for_close.clone();
        menu.queue_free();
    });
    menu.connect("close_requested", &close);
    let control: Gd<Node> = menu.clone().upcast();
    dialog.base_mut().add_child(&control);
    menu.popup_centered();
}

/// `_confirm_planet_research`.
fn confirm_planet_research(dialog: &Gd<MindDialog>) {
    let mut node = dialog.clone();
    set_meta_text(&mut node, SETTINGS_PENDING_META, "planet-research");
    let planet = planet_display(&meta_text(dialog, SETTINGS_PLANET_META));
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call(
            "show_confirm",
            &[
                kit::tm_format("@settings.clearplanetresearch.confirm", &[planet.as_str()])
                    .to_variant(),
            ],
        );
    }
}

/// `_confirm_planet_campaign_saves`.
fn confirm_planet_campaign_saves(dialog: &Gd<MindDialog>) {
    let mut node = dialog.clone();
    set_meta_text(&mut node, SETTINGS_PENDING_META, "planet-campaign-saves");
    let planet = planet_display(&meta_text(dialog, SETTINGS_PLANET_META));
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call(
            "show_confirm",
            &[kit::tm_format(
                "@settings.clearplanetcampaignsaves.confirm",
                &[planet.as_str()],
            )
            .to_variant()],
        );
    }
}

/// `_campaign_action(method)`: forwards the selected planet name.
fn campaign_action(dialog: &Gd<MindDialog>, method: &str) {
    let Some(mut campaign) = kit::campaign() else {
        return;
    };
    if !campaign.has_method(method) {
        return;
    }
    let planet = meta_text(dialog, SETTINGS_PLANET_META);
    let _ = campaign.call(method, &[GString::from(planet.as_str()).to_variant()]);
}

/// `MindCampaign.selectable_planets` rows.
fn selectable_planets() -> Vec<VarDictionary> {
    let Some(mut campaign) = kit::campaign() else {
        return Vec::new();
    };
    if !campaign.has_method("selectable_planets") {
        return Vec::new();
    }
    let rows = campaign.call("selectable_planets", &[]).to::<VarArray>();
    rows.iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// `_planet_display()`: `@planet.<name>.name`.
fn planet_display(name: &str) -> String {
    kit::t(&format!("@planet.{name}.name")).to_string()
}

/// `settings.planetselect` with the upstream `[#iconColor]` name color.
fn planet_select_text(planets: &[VarDictionary], planet_name: &str) -> String {
    let mut label = planet_display(planet_name);
    for row in planets {
        if dict_text(row, "name") != planet_name {
            continue;
        }
        if let Some(value) = row.get("iconColor")
            && let Ok(color) = value.try_to::<Color>()
        {
            label = format!("[#{}]{}[]", color.to_html_without_alpha(), label);
        }
        break;
    }
    kit::tm_format("@settings.planetselect", &[label.as_str()]).to_string()
}

/// `new Table(Tex.button)`: the ninepatch box around the menu/data rows.
fn button_panel(content: Gd<VBoxContainer>) -> Gd<PanelContainer> {
    // code-instantiated: the panel wraps the runtime-built rows of each page.
    let mut panel = PanelContainer::new_alloc();
    panel.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    if let Some(texture) = kit::icon_texture("button") {
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        box_.set_texture_margin(Side::LEFT, BUTTON_SPLIT);
        box_.set_texture_margin(Side::TOP, BUTTON_SPLIT);
        box_.set_texture_margin(Side::RIGHT, BUTTON_SPLIT);
        box_.set_texture_margin(Side::BOTTOM, BUTTON_SPLIT);
        box_.set_content_margin(Side::LEFT, BUTTON_PAD);
        box_.set_content_margin(Side::TOP, BUTTON_PAD);
        box_.set_content_margin(Side::RIGHT, BUTTON_PAD);
        box_.set_content_margin(Side::BOTTOM, BUTTON_PAD);
        panel.add_theme_stylebox_override("panel", &box_);
    } else {
        // Atlas absent (test host): the flat `Tex.button` look (black fill,
        // 4 px #454545 outline).
        let mut flat = StyleBoxFlat::new_gd();
        flat.set_bg_color(Color::BLACK);
        flat.set_border_width_all(4);
        flat.set_border_color(FLAT_OVER);
        flat.set_corner_radius_all(4);
        flat.set_content_margin_all(BUTTON_PAD);
        panel.add_theme_stylebox_override("panel", &flat);
    }
    let control: Gd<Control> = content.upcast();
    panel.add_child(&control);
    panel
}

/// `button(text, icon, Styles.flatt, isize, listener).marginLeft(marg)`: a black
/// row (`flatOver` on hover/press), the `iconMed` icon cell at the left margin
/// and the label centred in the remaining width.
fn flat_button(
    icon_name: &str,
    text_value: &str,
    button_size: Vector2,
    margin_left: f32,
    rich: bool,
) -> Gd<Button> {
    // code-instantiated: menu/data rows are data-driven per page.
    let mut button = Button::new_alloc();
    button.set_custom_minimum_size(button_size);
    button.set_focus_mode(FocusMode::NONE);
    let up = flat_box(Color::BLACK);
    let over = flat_box(FLAT_OVER);
    button.add_theme_stylebox_override("normal", &up);
    button.add_theme_stylebox_override("hover", &over);
    button.add_theme_stylebox_override("pressed", &over);
    button.add_theme_stylebox_override("hover_pressed", &over);
    button.add_theme_stylebox_override("disabled", &up);
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, margin_left);
    row.add_theme_constant_override("separation", 0);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);
    let mut icon = factory::glyph_label(icon_name, ICON_FONT_SIZE, Color::WHITE);
    icon.set_custom_minimum_size(Vector2::new(ICON_CELL, ICON_CELL));
    icon.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&icon);
    if rich {
        let mut rich_label = factory::label(text_value);
        rich_label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        rich_label.set_vertical_alignment(VerticalAlignment::CENTER);
        rich_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        rich_label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        rich_label.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&rich_label);
        return button;
    }
    let mut label = Label::new_alloc();
    label.set_text(text_value);
    label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_clip_text(true);
    label.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&label);
    button
}

/// `_set_value`: writes the setting and drives the live audio mixer.
fn set_value(key: &str, value: Variant, sync_audio: bool) {
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call(
            "settings_set",
            &[GString::from(key).to_variant(), value.clone()],
        );
    }
    if sync_audio && let Some(mut audio) = kit::node("/root/Spine/MindAudio") {
        // The audio mixer keeps its own live settings mirror; drive it directly.
        let _ = audio.call("set_setting", &[GString::from(key).to_variant(), value]);
    }
}

/// Upstream `SliderSetting.sp` value processors, keyed by the model's format tag.
fn format_value(row: &VarDictionary, value: i64) -> String {
    match dict_text(row, "format").as_str() {
        "percent" => format!("{value}%"),
        "seconds" => kit::tm_format("@setting.seconds", &[value.to_string().as_str()]).to_string(),
        "multiplier" => {
            // `(i / 4f) + "x"`: Java float `toString` (`1.0x`, `0.25x`, `1.75x`).
            format!("{}x", java_float(value))
        }
        "x" => format!("{value}x"),
        "bloomPercent" => format!("{}%", (value as f64 / 4.0 * 100.0) as i64),
        "pixels" => format!("{value}px"),
        "fpsCap" => {
            if value > 240 {
                kit::t("@setting.fpscap.none").to_string()
            } else {
                kit::tm_format("@setting.fpscap.text", &[value.to_string().as_str()]).to_string()
            }
        }
        _ => value.to_string(),
    }
}

/// Java `Float.toString` for the quarter steps the screenshake slider produces.
fn java_float(value: i64) -> String {
    let quarter = value as f64 / 4.0;
    let snapped = (quarter * 100.0).round() / 100.0;
    // One decimal as soon as the value has at most one fractional digit
    // (Java omits trailing noise only when there is none to print).
    let one_decimal = snapped.fract().abs() < f64::EPSILON
        || ((snapped * 10.0).round() - snapped * 10.0).abs() < 1e-9;
    if one_decimal {
        format!("{snapped:.1}")
    } else {
        format!("{snapped:.2}")
    }
}

/// The file-picker result the callback routes to.
#[derive(Clone, Copy)]
enum PickKind {
    ExportData,
    ImportData,
    ExportCrashLogs,
}

/// Native file chooser (`FileChooser.export`/`open`, native-first like the
/// mods dialog; the plan-14 `file_chooser` dialog is the fallback path).
fn pick_file(title_key: &str, default_name: &str, open: bool, filters: &[&str], kind: PickKind) {
    let mut packed = PackedStringArray::new();
    for filter in filters {
        packed.push(*filter);
    }
    let callable = Callable::from_fn("settings_pick_file", move |args: &[&Variant]| {
        let status = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let paths = args
            .get(1)
            .map(|value| value.to::<PackedStringArray>())
            .unwrap_or_default();
        if !status || paths.is_empty() {
            return;
        }
        let path = paths
            .as_slice()
            .first()
            .map(|value| value.to_string())
            .unwrap_or_default();
        if path.is_empty() {
            return;
        }
        let Some(mut ui) = kit::ui() else {
            return;
        };
        match kind {
            PickKind::ExportData => {
                let _ = ui.call("data_export", &[GString::from(path.as_str()).to_variant()]);
            }
            PickKind::ImportData => {
                let imported = ui
                    .call("data_import", &[GString::from(path.as_str()).to_variant()])
                    .try_to::<bool>()
                    .unwrap_or(false);
                if !imported {
                    let _ = ui.call("show_info", &[kit::t("@data.invalid").to_variant()]);
                }
            }
            PickKind::ExportCrashLogs => {
                let _ = ui.call(
                    "export_crash_logs",
                    &[GString::from(path.as_str()).to_variant()],
                );
            }
        }
    });
    let mode = if open {
        FileDialogMode::OPEN_FILES
    } else {
        FileDialogMode::SAVE_FILE
    };
    let _ = DisplayServer::singleton().file_dialog_show(
        &kit::t(title_key),
        "",
        default_name,
        false,
        mode,
        &packed,
        &callable,
    );
}

// ---------------------------------------------------------------------------
// LanguageController (`language`)
// ---------------------------------------------------------------------------

/// `LanguageDialog.displayNames`: locale code -> native name (parity ABI).
const DISPLAY_NAMES: [(&str, &str); 36] = [
    ("ca", "Català"),
    ("id_ID", "Bahasa Indonesia"),
    ("da", "Dansk"),
    ("de", "Deutsch"),
    ("et", "Eesti"),
    ("en", "English"),
    ("es", "Español"),
    ("eu", "Euskara"),
    ("fil", "Filipino"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("lt", "Lietuvių"),
    ("hu", "Magyar"),
    ("nl", "Nederlands"),
    ("nl_BE", "Nederlands (België)"),
    ("pl", "Polski"),
    ("pt_BR", "Português (Brasil)"),
    ("pt_PT", "Português (Portugal)"),
    ("ro", "Română"),
    ("fi", "Suomi"),
    ("sv", "Svenska"),
    ("vi", "Tiếng Việt"),
    ("tk", "Türkmen dili"),
    ("tr", "Türkçe"),
    ("cs", "Čeština"),
    ("be", "Беларуская"),
    ("bg", "Български"),
    ("ru", "Русский"),
    ("sr", "Српски"),
    ("uk_UA", "Українська"),
    ("th", "ไทย"),
    ("zh_CN", "简体中文"),
    ("zh_TW", "正體中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("router", "router"),
];

/// Meta key holding `_buttons` (locale code -> row `Button`).
const LANGUAGE_BUTTONS_META: &str = "mind_language_buttons";
/// Meta key holding `_current_locale`.
const LANGUAGE_CURRENT_META: &str = "mind_language_current";

/// `@settings.language` — the locale list with native display names.
///
/// The available locale set comes from `MindAssets.locales()`; the list order
/// matches `Vars.java:315-327` (sorted by display name). A click writes
/// `locale` through `MindUi.settings_put_text` and shows `@language.restart`
/// (`LanguageDialog.java:78-84`); the bundle is selected at boot.
pub struct LanguageController;

impl DialogController for LanguageController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@settings.language");
        dialog.should_pause = false;
        // `BaseDialog` ctor `setFillParent(true)` over `Styles.defaultDialog`:
        // the rows float over the dimmed stage (no boxed panel).
        dialog.enable_full_dialog(true);
        dialog.clear_content();
        language_build(dialog);
        let mut back = dialog.add_close_button(210.0);
        back.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // The per-frame `update(t -> t.setChecked(...))` in Java; the port
        // re-syncs the checked row whenever the dialog opens. Deferred: the
        // stored-setting read must not re-enter a `MindUi` mutable borrow.
        let target = dialog.to_gd();
        let callable =
            Callable::from_fn("language_refresh_checked", move |_args: &[&Variant]| {
                if !target.is_instance_valid() {
                    return;
                }
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                language_refresh_checked(&mut bound);
            });
        callable.call_deferred(&[]);
    }
}

/// `LanguageDialog.getDisplayName` — falls back to the raw code.
fn language_display_name(locale: &str) -> String {
    let key = locale.replace("in_ID", "id_ID");
    DISPLAY_NAMES
        .iter()
        .find(|(code, _)| *code == key)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or(key)
}

/// `LanguageDialog.getLocale`: the stored `locale` setting, else the live
/// locale resolved by `MindAssets`, else `en`.
fn language_current_locale() -> String {
    if let Some(mut ui) = kit::ui()
        && ui.has_method("settings_get_text")
    {
        let stored = ui
            .call(
                "settings_get_text",
                &[
                    GString::from("locale").to_variant(),
                    GString::from("default").to_variant(),
                ],
            )
            .to::<GString>();
        if !stored.is_empty() && stored != "default" {
            return stored.to_string();
        }
    }
    if let Some(assets) = kit::assets() {
        let live = assets.bind().locale();
        if !live.is_empty() {
            return live.to_string();
        }
    }
    "en".to_string()
}

/// `Vars.init`'s `Vars.locales` (`Vars.java:315-327`): the `assets/locales`
/// list sorted by display name (case-insensitive) with the `router`
/// pseudo-locale appended when absent.
fn language_available_locales() -> Vec<String> {
    let mut codes: Vec<String> = Vec::new();
    if let Some(assets) = kit::assets() {
        let locales = assets.bind().locales();
        for code in locales.as_slice() {
            codes.push(code.to_string());
        }
    }
    if codes.is_empty() {
        for (code, _) in DISPLAY_NAMES {
            codes.push(code.to_string());
        }
    }
    if !codes.iter().any(|code| code == "router") {
        codes.push("router".to_string());
    }
    codes.sort_by(|a, b| {
        language_display_name(a)
            .to_lowercase()
            .cmp(&language_display_name(b).to_lowercase())
    });
    codes
}

/// `Styles.flatTogglet` (`Styles.java:190-199`): `up = black`,
/// `over = flatOver`, `down`/`checked` framed in `Pal.accent`.
fn language_apply_flat_toggle_style(button: &mut Gd<Button>) {
    let up = language_flat_box(Color::BLACK, 0);
    let over = language_flat_box(styles::FLAT_OVER, 0);
    let down = language_flat_box(Color::BLACK, 4);
    button.add_theme_stylebox_override("normal", &up);
    button.add_theme_stylebox_override("hover", &over);
    button.add_theme_stylebox_override("pressed", &down);
    button.add_theme_stylebox_override("hover_pressed", &down);
    button.add_theme_stylebox_override("disabled", &up);
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
    for state in [
        "font_color",
        "font_hover_color",
        "font_pressed_color",
        "font_hover_pressed_color",
    ] {
        button.add_theme_color_override(state, Color::WHITE);
    }
    button.add_theme_color_override("font_disabled_color", styles::ARC_GRAY);
}

/// `_flat_box(fill, border)`: a solid fill with the accent frame.
fn language_flat_box(fill: Color, border: i32) -> Gd<StyleBoxFlat> {
    let mut box_ = flat_box(fill);
    box_.set_border_color(styles::ACCENT);
    box_.set_border_width_all(border);
    box_
}

/// `LanguageDialog.setup`: one 400x50 `flatTogglet` row per locale inside a
/// scroll pane, centred between the title and the Back button.
fn language_build(dialog: &mut MindDialog) {
    let current = language_current_locale();
    let mut node = dialog.to_gd();
    set_meta_text(&mut node, LANGUAGE_CURRENT_META, &current);
    node.set_meta(LANGUAGE_BUTTONS_META, &VarDictionary::new().to_variant());
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
    }
    if let Some(mut content) = dialog.content() {
        content.set_v_size_flags(SizeFlags::EXPAND_FILL);
    }
    // code-instantiated: the locale table is filled from the runtime locale set.
    let mut list = MindTable::new_alloc();
    {
        let mut control: Gd<Control> = list.clone().upcast();
        control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    }
    // code-instantiated: the scroll pane wraps the runtime locale table.
    let mut pane = MindScroll::new_alloc();
    {
        let control: Gd<Control> = list.clone().upcast();
        pane.add_child(&control);
    }
    pane.bind_mut().set_scrolling_disabled(true, false);
    pane.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    pane.set_v_size_flags(SizeFlags::EXPAND_FILL);
    if let Some(mut content) = dialog.content() {
        let control: Gd<Control> = pane.clone().upcast();
        content.add_child(&control);
    }
    let group = ButtonGroup::new_gd();
    let mut buttons = VarDictionary::new();
    for locale in language_available_locales() {
        // code-instantiated: locale rows are data-driven from the locale set.
        let mut button = Button::new_alloc();
        let display_name = language_display_name(&locale);
        button.set_text(&GString::from(display_name.as_str()));
        button.set_theme_type_variation("flatTogglet");
        language_apply_flat_toggle_style(&mut button);
        button.set_custom_minimum_size(Vector2::new(400.0, 50.0));
        button.set_toggle_mode(true);
        button.set_button_group(&group);
        button.set_pressed_no_signal(locale == current);
        button.set_focus_mode(FocusMode::NONE);
        let target = dialog.to_gd();
        let locale_for_press = locale.clone();
        let callable = Callable::from_fn("language_select", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return;
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            language_select(&mut bound, &locale_for_press);
        });
        button.connect("pressed", &callable);
        // Upstream `langs.add(button).group(group)...size(400f, 50f).row();
        // the table's 24 px side margins are the cell pads.
        list.bind_mut()
            .add(&button)
            .width(400.0)
            .height(50.0)
            .pad_left(24.0)
            .pad_right(24.0);
        list.bind_mut().row();
        buttons.set(&GString::from(locale.as_str()), &button);
    }
    node.set_meta(LANGUAGE_BUTTONS_META, &buttons.to_variant());
}

/// `LanguageDialog`'s per-frame checked sync (one checked row; the
/// `ButtonGroup` keeps the exclusivity, like upstream).
fn language_refresh_checked(dialog: &mut MindDialog) {
    let current = language_current_locale();
    let mut node = dialog.to_gd();
    set_meta_text(&mut node, LANGUAGE_CURRENT_META, &current);
    if !node.has_meta(LANGUAGE_BUTTONS_META) {
        return;
    }
    let buttons = node.get_meta(LANGUAGE_BUTTONS_META).to::<VarDictionary>();
    for key in buttons.keys_array().iter_shared() {
        let code = key
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        let Some(value) = buttons.get(code.as_str()) else {
            continue;
        };
        let Ok(mut button) = value.try_to::<Gd<Button>>() else {
            continue;
        };
        if !button.is_instance_valid() {
            continue;
        }
        button.set_pressed_no_signal(code == current);
    }
}

/// `LanguageDialog.java:78-84`: a click on a *different* locale stores
/// `Core.settings.put("locale", code)` and shows `@language.restart` (the
/// running bundle is not swapped until the next boot).
fn language_select(dialog: &mut MindDialog, locale: &str) {
    if locale == language_current_locale() {
        language_refresh_checked(dialog);
        return;
    }
    set_meta_text(&mut dialog.to_gd(), LANGUAGE_CURRENT_META, locale);
    dialog
        .base_mut()
        .emit_signal("locale_selected", &[GString::from(locale).to_variant()]);
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call(
            "settings_put_text",
            &[
                GString::from("locale").to_variant(),
                GString::from(locale).to_variant(),
            ],
        );
        // `ui.showInfo("@language.restart")`: an untitled modal prompt.
        let _ = ui.call(
            "show_text",
            &[
                GString::new().to_variant(),
                kit::t("@language.restart").to_variant(),
            ],
        );
    }
    language_refresh_checked(dialog);
}

// ---------------------------------------------------------------------------
// ControlsController (`controls`)
// ---------------------------------------------------------------------------

/// Upstream skips the per-row Reset for the menu binding.
const KEYBIND_NO_RESET: [&str; 1] = ["menu"];
/// Arc `KeyCode.getName()` handles unset as "Unset" (rendered dark gray).
const KEYBIND_UNSET_TEXT: &str = "Unset";
const KEYBIND_UNSET_COLOR: Color = Color::from_rgba(0.35, 0.35, 0.35, 1.0);
const KEYBIND_CATEGORY_COLOR: Color = Color::from_rgba(0.5, 0.5, 0.5, 1.0);
const KEYBIND_BUTTON_SIZE: Vector2 = Vector2::new(140.0, 40.0);
/// `KeybindDialog.rebuildBinds`'s binds table width in the reference frame
/// (140..985 of 1152).
const KEYBIND_TABLE_WIDTH: f32 = 845.0;
/// Root path of the input facade that owns the binding registry.
const KEYBIND_INPUT_PATH: &str = "/root/Spine/Input";

/// Widget handles shared with the search/capture/reset callbacks.
#[derive(Default)]
struct KeybindState {
    list: Option<Gd<MindTable>>,
    search: Option<Gd<LineEdit>>,
}

/// `@keybind.title` — the Rebind Keys table, the search field and the key
/// capture overlay (`KeybindDialog.java`, plan 15 §6.1).
pub struct ControlsController {
    state: Rc<RefCell<KeybindState>>,
}

impl Default for ControlsController {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(KeybindState::default())),
        }
    }
}

impl DialogController for ControlsController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@keybind.title");
        dialog.should_pause = false;
        // Upstream `KeybindDialog.setFillParent(true)` + `top()`.
        dialog.enable_full_dialog(true);
        dialog.clear_content();
        let state = self.state.clone();
        keybind_build(dialog, &state);
        dialog.add_close_button(210.0);
    }
}

/// `MindInput.keybinds_json` rows (`None` when the facade is absent).
fn keybind_entries() -> Vec<VarDictionary> {
    let Some(mut input) = kit::node(KEYBIND_INPUT_PATH) else {
        return Vec::new();
    };
    if !input.has_method("keybinds_json") {
        return Vec::new();
    }
    let text = input.call("keybinds_json", &[]).to::<GString>();
    json_array(&text)
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// `_build`: the search row plus the binding table inside `cont`.
fn keybind_build(dialog: &mut MindDialog, state: &Rc<RefCell<KeybindState>>) {
    let mut root = dialog.content_table();
    {
        let mut root_control: Gd<Control> = root.clone().upcast();
        root_control.add_theme_constant_override("separation", 4);
    }
    // code-instantiated: the search icon+field row is a per-dialog control.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 6);
    search_row.add_child(&factory::glyph_label("zoom", 22, Color::WHITE));
    let mut search = factory::field("");
    search.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let target = dialog.to_gd();
        let state_for_search = state.clone();
        let callable = Callable::from_fn("keybind_search", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return;
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            keybind_rebuild(&mut bound, &state_for_search);
        });
        search.connect("text_changed", &callable);
    }
    search_row.add_child(&search);
    root.bind_mut().add(&search_row).grow_x();
    root.bind_mut().row();

    // code-instantiated: the binding list is data-driven from `MindInput` and
    // rebuilt on every search/reassign; the row count is unknown at build time.
    let list = MindTable::new_alloc();
    {
        let mut control: Gd<Control> = list.clone().upcast();
        let mut min = control.get_custom_minimum_size();
        min.x = KEYBIND_TABLE_WIDTH;
        control.set_custom_minimum_size(min);
        control.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    }
    if let Some(mut content) = dialog.content() {
        let control: Gd<Control> = list.clone().upcast();
        content.add_child(&control);
    }
    {
        let mut st = state.borrow_mut();
        st.list = Some(list);
        st.search = Some(search);
    }
    keybind_rebuild(dialog, state);
}

/// `_rebuild`: reloads the binding registry and repaints the table.
fn keybind_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<KeybindState>>) {
    let (mut list, search) = {
        let st = state.borrow();
        let Some(list) = st.list.clone() else {
            return;
        };
        let search = st
            .search
            .as_ref()
            .map(|field| field.get_text().to_string())
            .unwrap_or_default();
        (list, search)
    };
    list.bind_mut().clear();
    let query = search.to_lowercase();
    let mut last_category: Option<String> = None;
    for entry in keybind_entries() {
        let localized = kit::t(&format!("@{}", dict_text(&entry, "bundle"))).to_string();
        if !query.is_empty() && !localized.to_lowercase().contains(&query) {
            continue;
        }
        let category = entry
            .get("category")
            .filter(|value| !value.is_nil())
            .map(|value| variant_text(&value));
        if let Some(category) = category
            && last_category.as_deref() != Some(category.as_str())
        {
            keybind_add_category(&mut list, &category);
            last_category = Some(category);
        }
        keybind_add_row(&dialog.to_gd(), state, &mut list, &entry, &localized);
    }
    keybind_add_reset_all(&dialog.to_gd(), state, &mut list);
    list.bind_mut().sort_now();
}

/// `_add_category_header`: the section label and its gray rule.
fn keybind_add_category(list: &mut Gd<MindTable>, category: &str) {
    let header = factory::label(&kit::t(&format!("@category.{category}.name")).to_string());
    {
        let mut rich_header: Gd<RichTextLabel> = header.clone().upcast();
        rich_header.add_theme_color_override("font_color", KEYBIND_CATEGORY_COLOR);
    }
    list.bind_mut()
        .add(&header)
        .grow_x()
        .colspan(4)
        .pad(10.0)
        .pad_bottom(4.0);
    list.bind_mut().row();
    // code-instantiated: the section rule is a styled ColorRect.
    let mut rule = ColorRect::new_alloc();
    rule.set_color(KEYBIND_CATEGORY_COLOR);
    rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    rule.set_mouse_filter(MouseFilter::IGNORE);
    list.bind_mut()
        .add(&rule)
        .grow_x()
        .colspan(4)
        .pad(6.0)
        .pad_top(0.0)
        .pad_bottom(10.0);
    list.bind_mut().row();
}

/// `_add_binding_row`: name, key cell, Rebind and the optional per-row Reset.
fn keybind_add_row(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<KeybindState>>,
    list: &mut Gd<MindTable>,
    entry: &VarDictionary,
    localized: &str,
) {
    let name = dict_text(entry, "name");
    // code-instantiated: keybind rows are data-driven from the registry.
    let label = factory::label(localized);
    list.bind_mut()
        .add(&label)
        .grow_x()
        .align(Align::Left)
        .pad_left(8.0)
        .pad_right(40.0);

    // `KeybindDialog.java:111-124`: the key cell is `Pal.accent`
    // (`Color.darkGray` while unset), left-aligned, `minWidth(90)`,
    // `padRight(20)`; the raw `[red]`/`[]` markup is translated first.
    let key_label = factory::label(&kit::markup_text(&keybind_key_text(entry)).to_string());
    {
        let mut rich_key: Gd<RichTextLabel> = key_label.clone().upcast();
        rich_key.add_theme_color_override(
            "default_color",
            if keybind_is_unset(entry) {
                KEYBIND_UNSET_COLOR
            } else {
                styles::ACCENT
            },
        );
    }
    list.bind_mut()
        .add(&key_label)
        .min_width(90.0)
        .align(Align::Left)
        .pad_right(20.0);

    let mut rebind = factory::button(&kit::t("@settings.rebind").to_string());
    rebind.set_theme_type_variation("grayt");
    rebind.set_custom_minimum_size(KEYBIND_BUTTON_SIZE);
    {
        let target = dialog.clone();
        let state_for_rebind = state.clone();
        let name_for_rebind = name.clone();
        let axis = dict_bool(entry, "axis");
        let callable = Callable::from_fn("keybind_start_capture", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return;
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            keybind_start_capture(&mut bound, &state_for_rebind, &name_for_rebind, axis);
        });
        rebind.connect("pressed", &callable);
    }
    list.bind_mut()
        .add(&rebind)
        .min_width(KEYBIND_BUTTON_SIZE.x)
        .min_height(KEYBIND_BUTTON_SIZE.y);

    if !KEYBIND_NO_RESET.contains(&name.as_str()) {
        let mut reset = factory::button(&kit::t("@settings.resetKey").to_string());
        reset.set_theme_type_variation("grayt");
        reset.set_custom_minimum_size(KEYBIND_BUTTON_SIZE);
        reset.set_disabled(dict_bool(entry, "default"));
        {
            let target = dialog.clone();
            let state_for_reset = state.clone();
            let name_for_reset = name.clone();
            let callable =
                Callable::from_fn("keybind_reset_binding", move |_args: &[&Variant]| {
                    if !target.is_instance_valid() {
                        return;
                    }
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    keybind_reset_binding(&mut bound, &state_for_reset, &name_for_reset);
                });
            reset.connect("pressed", &callable);
        }
        list.bind_mut()
            .add(&reset)
            .min_width(KEYBIND_BUTTON_SIZE.x)
            .min_height(KEYBIND_BUTTON_SIZE.y)
            .pad(2.0)
            .pad_left(4.0);
    }
    list.bind_mut().row();
}

/// `KeybindDialog.rebuildBinds`'s final row: `@settings.reset` + `refresh`.
fn keybind_add_reset_all(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<KeybindState>>,
    list: &mut Gd<MindTable>,
) {
    let text = kit::t("@settings.reset").to_string();
    let mut reset = factory::icon_button("refresh", &text, "grayt", AlignmentMode::BEGIN);
    {
        let mut min = reset.get_custom_minimum_size();
        min.x = 200.0;
        reset.set_custom_minimum_size(min);
    }
    {
        let target = dialog.clone();
        let state_for_reset = state.clone();
        let callable = Callable::from_fn("keybind_reset_all", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return;
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            keybind_reset_all(&mut bound, &state_for_reset);
        });
        reset.connect("pressed", &callable);
    }
    list.bind_mut()
        .add(&reset)
        .colspan(4)
        .min_width(200.0)
        .min_height(50.0)
        .pad(4.0);
    list.bind_mut().row();
}

/// Upstream key cell: a single key, or `min [red]/[] max` for a two-key axis.
fn keybind_key_text(entry: &VarDictionary) -> String {
    let negative = entry.get("negativeDisplay");
    let positive = entry.get("display");
    let axis = dict_bool(entry, "axis");
    if axis && negative.as_ref().is_some_and(|value| !value.is_nil()) {
        let negative_text = negative
            .map(|value| variant_text(&value))
            .unwrap_or_default();
        let positive_text = positive
            .filter(|value| !value.is_nil())
            .map(|value| variant_text(&value))
            .unwrap_or_else(|| KEYBIND_UNSET_TEXT.to_string());
        return format!("{negative_text} [red]/[] {positive_text}");
    }
    positive
        .filter(|value| !value.is_nil())
        .map(|value| variant_text(&value))
        .unwrap_or_else(|| KEYBIND_UNSET_TEXT.to_string())
}

/// `_is_unset`: neither cell of the binding has a display name.
fn keybind_is_unset(entry: &VarDictionary) -> bool {
    entry.get("display").is_none_or(|value| value.is_nil())
        && entry
            .get("negativeDisplay")
            .is_none_or(|value| value.is_nil())
}

/// `_reset_binding`.
fn keybind_reset_binding(dialog: &mut MindDialog, state: &Rc<RefCell<KeybindState>>, name: &str) {
    if let Some(mut input) = kit::node(KEYBIND_INPUT_PATH)
        && input.has_method("reset_keybind")
    {
        let _ = input.call("reset_keybind", &[GString::from(name).to_variant()]);
    }
    keybind_rebuild(dialog, state);
}

/// `_reset_all`.
fn keybind_reset_all(dialog: &mut MindDialog, state: &Rc<RefCell<KeybindState>>) {
    if let Some(mut input) = kit::node(KEYBIND_INPUT_PATH)
        && input.has_method("reset_keybinds")
    {
        let _ = input.call("reset_keybinds", &[]);
    }
    keybind_rebuild(dialog, state);
}

/// `KeybindDialog.openDialog`: a full-rect capture overlay that grabs focus so
/// the next key (or mouse button) rebinds the row; Back cancels and Unbind
/// clears the binding. Axis rows ask twice (`keybind.press.axis` then
/// `keybind.press`, `KeybindDialog.java:96-145`).
fn keybind_start_capture(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<KeybindState>>,
    bind_name: &str,
    axis: bool,
) {
    if let Some(viewport) = dialog.base().get_viewport()
        && let Some(mut focus) = viewport.gui_get_focus_owner()
    {
        focus.release_focus();
    }
    // code-instantiated: the capture prompt is a transient per-rebind overlay.
    let mut overlay = Control::new_alloc();
    overlay.set_name("KeybindCapture");
    overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    overlay.set_mouse_filter(MouseFilter::STOP);
    overlay.set_focus_mode(FocusMode::ALL);

    // code-instantiated: the centered prompt panel of the capture overlay.
    let mut center = CenterContainer::new_alloc();
    center.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    center.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&center);

    let mut panel = PanelContainer::new_alloc();
    panel.set_theme_type_variation("defaultDialog");
    center.add_child(&panel);

    let mut box_ = VBoxContainer::new_alloc();
    box_.add_theme_constant_override("separation", 12);
    panel.add_child(&box_);

    let prompt_key = if axis {
        "@keybind.press.axis"
    } else {
        "@keybind.press"
    };
    let prompt = factory::styled_label(&kit::t(prompt_key).to_string(), "techLabel");
    {
        let mut rich_prompt: Gd<RichTextLabel> = prompt.clone().upcast();
        rich_prompt.set_horizontal_alignment(HorizontalAlignment::CENTER);
    }
    box_.add_child(&prompt);

    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(AlignmentMode::CENTER);
    box_.add_child(&row);

    let mut back = factory::button(&kit::t("@back").to_string());
    back.set_custom_minimum_size(Vector2::new(160.0, 50.0));
    {
        let overlay_for_back = overlay.clone();
        let callable = Callable::from_fn("keybind_end_capture", move |_args: &[&Variant]| {
            keybind_end_capture(&overlay_for_back);
        });
        back.connect("pressed", &callable);
    }
    row.add_child(&back);

    let mut unbind = factory::button(&kit::t("@settings.unbindKey").to_string());
    unbind.set_custom_minimum_size(Vector2::new(160.0, 50.0));
    {
        let target = dialog.to_gd();
        let overlay_for_unbind = overlay.clone();
        let state_for_unbind = state.clone();
        let name_for_unbind = bind_name.to_string();
        let callable = Callable::from_fn("keybind_unbind", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return;
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            keybind_unbind_capture(
                &mut bound,
                &state_for_unbind,
                &overlay_for_unbind,
                &name_for_unbind,
            );
        });
        unbind.connect("pressed", &callable);
    }
    row.add_child(&unbind);

    dialog.base_mut().add_child(&overlay);

    let axis_min = Rc::new(RefCell::new(String::new()));
    let target = dialog.to_gd();
    let state_for_event = state.clone();
    let overlay_for_event = overlay.clone();
    let prompt_for_event = prompt.clone();
    let name_for_event = bind_name.to_string();
    let callable = Callable::from_fn("keybind_capture_input", move |args: &[&Variant]| {
        let Some(event) = args.first().map(|value| (*value).to::<Gd<InputEvent>>()) else {
            return;
        };
        if !target.is_instance_valid() || !overlay_for_event.is_instance_valid() {
            return;
        }
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        keybind_capture_event(
            &mut bound,
            &state_for_event,
            &overlay_for_event,
            &prompt_for_event,
            &name_for_event,
            axis,
            &axis_min,
            event,
        );
    });
    overlay.connect("gui_input", &callable);
    overlay.grab_focus();
}

/// One GUI event while the capture overlay owns focus.
// Mirrors the GDScript's captured locals (`dialog`, `state`, `overlay`,
// `prompt`, `bind_name`, `axis`, `keys`, `event`); splitting the signature
// would only move the same state into a struct built at every event.
#[allow(clippy::too_many_arguments)]
fn keybind_capture_event(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<KeybindState>>,
    overlay: &Gd<Control>,
    prompt: &Gd<MindLabel>,
    bind_name: &str,
    axis: bool,
    axis_min: &Rc<RefCell<String>>,
    event: Gd<InputEvent>,
) {
    if let Ok(button) = event.clone().try_cast::<InputEventMouseButton>() {
        if !button.is_pressed() {
            return;
        }
        let index = button.get_button_index();
        // An axis row captures the wheel as `scroll` in one step
        // (`KeybindDialog.scrolled`, KeybindDialog.java:139-145).
        if axis && (index == MouseButton::WHEEL_UP || index == MouseButton::WHEEL_DOWN) {
            keybind_apply_axis_capture(dialog, state, overlay, bind_name, "", "scroll");
            return;
        }
        let code = match index {
            MouseButton::LEFT => "mouseLeft",
            MouseButton::RIGHT => "mouseRight",
            MouseButton::MIDDLE => "mouseMiddle",
            _ => "",
        };
        if !code.is_empty() {
            keybind_apply_capture(
                dialog, state, overlay, prompt, bind_name, axis, axis_min, code, false,
            );
        }
        return;
    }
    let Ok(key) = event.try_cast::<InputEventKey>() else {
        return;
    };
    if !key.is_pressed() || key.is_echo() || !dialog.is_top() {
        return;
    }
    let code = Os::singleton()
        .get_keycode_string(key.get_keycode())
        .to_string();
    keybind_apply_capture(
        dialog, state, overlay, prompt, bind_name, axis, axis_min, &code, true,
    );
    // Keep the press from reaching the shell's ESC close handler while the
    // capture prompt is open (upstream consumes every key in rebind mode).
    if let Some(mut viewport) = dialog.base().get_viewport() {
        viewport.set_input_as_handled();
    }
}

/// `_apply_capture`: rebinds (or stores the negative half of an axis).
#[allow(clippy::too_many_arguments)]
fn keybind_apply_capture(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<KeybindState>>,
    overlay: &Gd<Control>,
    prompt: &Gd<MindLabel>,
    bind_name: &str,
    axis: bool,
    axis_min: &Rc<RefCell<String>>,
    code: &str,
    from_godot: bool,
) {
    if axis {
        // First key = negative, second = positive (`axis.key` / `rebindMin`).
        if axis_min.borrow().is_empty() {
            *axis_min.borrow_mut() = code.to_string();
            let mut label: Gd<RichTextLabel> = prompt.clone().upcast();
            label.set_text(&kit::t("@keybind.press"));
            return;
        }
        let negative = axis_min.borrow().clone();
        keybind_apply_axis_capture(dialog, state, overlay, bind_name, &negative, code);
        return;
    }
    if let Some(mut input) = kit::node(KEYBIND_INPUT_PATH) {
        let method = if from_godot { "rebind_key" } else { "rebind" };
        if input.has_method(method) {
            let _ = input.call(
                method,
                &[
                    GString::from(bind_name).to_variant(),
                    GString::from(code).to_variant(),
                ],
            );
        }
    }
    keybind_end_capture(overlay);
    keybind_rebuild(dialog, state);
}

/// `_apply_axis_capture`.
fn keybind_apply_axis_capture(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<KeybindState>>,
    overlay: &Gd<Control>,
    bind_name: &str,
    negative: &str,
    positive: &str,
) {
    if let Some(mut input) = kit::node(KEYBIND_INPUT_PATH)
        && input.has_method("rebind_axis")
    {
        let _ = input.call(
            "rebind_axis",
            &[
                GString::from(bind_name).to_variant(),
                GString::from(negative).to_variant(),
                GString::from(positive).to_variant(),
            ],
        );
    }
    keybind_end_capture(overlay);
    keybind_rebuild(dialog, state);
}

/// `_unbind_capture`.
fn keybind_unbind_capture(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<KeybindState>>,
    overlay: &Gd<Control>,
    bind_name: &str,
) {
    if let Some(mut input) = kit::node(KEYBIND_INPUT_PATH)
        && input.has_method("unbind_keybind")
    {
        let _ = input.call("unbind_keybind", &[GString::from(bind_name).to_variant()]);
    }
    keybind_end_capture(overlay);
    keybind_rebuild(dialog, state);
}

/// `_end_capture`: drops the transient prompt overlay.
fn keybind_end_capture(overlay: &Gd<Control>) {
    if !overlay.is_instance_valid() {
        return;
    }
    let mut overlay = overlay.clone();
    overlay.queue_free();
}

// ---------------------------------------------------------------------------
// EffectsController (`effects`)
// ---------------------------------------------------------------------------

/// `EffectsDialog.EffectCell` edge and grid gap (`EffectsDialog.setup`).
const EFFECTS_CELL_SIZE: f32 = 280.0;
const EFFECTS_CELL_GAP: f32 = 12.0;
/// The shared preview host ticks at 20 Hz (`EffectCell.act` is per frame; the
/// port re-renders the one hovered effect rather than 35 cell batches).
const EFFECTS_PREVIEW_TICK_SECONDS: f64 = 0.05;
/// `EffectCell` respawns when its lifetime elapses; the port re-emits the
/// hovered effect every 8 ticks (0.4 s).
const EFFECTS_PREVIEW_RESPAWN_TICKS: u32 = 8;
/// `EffectCell` scale clamp (`width / calculateSize` explodes for tiny bounds).
const EFFECTS_PREVIEW_MIN_SCALE: f32 = 0.1;
const EFFECTS_PREVIEW_MAX_SCALE: f32 = 2.0;

/// The shared live-preview host of the effects dialog.
///
/// Java's `EffectsDialog.EffectCell` renders its own effect through
/// `Core.batch`. The native port has no per-cell batch, so one [`MindFx`] node
/// is parented over the dialog and plays the hovered cell's `LogicFx` entry
/// (`EffectCell.draw` + `act`: the effect loops until the pointer leaves). This
/// is a documented deviation: only the hovered cell animates, not all 35.
#[derive(Clone)]
struct EffectsPreview {
    fx: Gd<crate::fx::MindFx>,
    /// `(Fx field name, canvas position, scale)` while a cell is hovered.
    current: Option<(String, Vector2, f32)>,
    tick: u32,
}

/// The `Fx` field a `LogicFx` entry plays (`LogicFx.EffectEntry.effect`).
fn effects_fx_name(effect_name: &str) -> Option<&'static str> {
    mind_core::logic::fx::get(effect_name)
        .map(|entry| entry.fx)
        .filter(|fx| !fx.is_empty())
}

/// `width / calculateSize(effect)` (`EffectsDialog.java:211`), clamped so a
/// tiny cached bound cannot blow the preview up past the cell.
fn effects_preview_scale(bounds: f32) -> f32 {
    let bounds = if bounds > 0.0 {
        bounds
    } else {
        EFFECTS_CELL_SIZE
    };
    (EFFECTS_CELL_SIZE / bounds).clamp(EFFECTS_PREVIEW_MIN_SCALE, EFFECTS_PREVIEW_MAX_SCALE)
}

/// `ui.effects` — the `LogicFx` picker used by `EffectStatement`.
///
/// A scrolling grid of 280 px cells over the alpha checker with the effect's
/// `name.token.<name>` label. Hovering a cell plays its `Fx` in a shared
/// [`MindFx`] preview host; selecting a cell emits `effect_selected` and closes
/// the dialog.
#[derive(Default)]
pub struct EffectsController;

impl DialogController for EffectsController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        // `makeButtonOverlay()` (`EffectsDialog.java:34`): padded title cell.
        dialog.apply_button_overlay(true);
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_literal("Effects");
        effects_build(dialog);
    }
}

/// `MindLogic.effect_entries()` rows.
fn effects_entries() -> Vec<VarDictionary> {
    let Some(mut logic) = kit::logic() else {
        return Vec::new();
    };
    if !logic.has_method("effect_entries") {
        return Vec::new();
    }
    let rows = logic.call("effect_entries", &[]).to::<VarArray>();
    rows.iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// `EffectsDialog.setup`: `cols = max(1, width / (size + 12))`, a scrollable
/// grid of 280 px cells.
fn effects_build(dialog: &mut MindDialog) {
    dialog.clear_content();
    // The shared live-preview host over the cells (`EffectCell.draw`).
    let preview = effects_preview_host(dialog);
    let width = viewport_width(dialog);
    let columns = ((width / (EFFECTS_CELL_SIZE + EFFECTS_CELL_GAP)) as i32).max(1);
    // code-instantiated: the effect grid is data-driven (`LogicFx.entries()`).
    let mut scroll = MindScroll::new_alloc();
    scroll.bind_mut().set_scrolling_disabled(true, false);
    scroll.set_custom_minimum_size(Vector2::new((width * 0.9).min(1200.0), 520.0));
    scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
    // code-instantiated: one cell per LogicFx entry.
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(columns);
    grid.add_theme_constant_override("h_separation", EFFECTS_CELL_GAP as i32);
    grid.add_theme_constant_override("v_separation", EFFECTS_CELL_GAP as i32);
    {
        let control: Gd<Control> = grid.clone().upcast();
        scroll.add_child(&control);
    }
    if let Some(mut content) = dialog.content() {
        let control: Gd<Control> = scroll.clone().upcast();
        content.add_child(&control);
    }
    let entries = effects_entries();
    if entries.is_empty() {
        grid.add_child(&factory::label(&kit::t("@none.found").to_string()));
        return;
    }
    for entry in entries {
        let cell = effects_cell(dialog, preview.as_ref(), &dict_text(&entry, "name"));
        grid.add_child(&cell);
    }
}

/// The shared [`MindFx`] preview host over the effects dialog.
///
/// Java's `EffectCell` renders into its own clipped transform; the port parents
/// one `MindFx` Node2D over the dialog, moves it to the hovered cell and loops
/// that cell's effect. Returns `None` when the FX host cannot be built (the
/// cell keeps its HSV-wash fallback).
fn effects_preview_host(dialog: &mut MindDialog) -> Option<Rc<RefCell<EffectsPreview>>> {
    for name in ["EffectPreviewFx", "EffectPreviewTick"] {
        if let Some(mut old) = dialog.base().get_node_or_null(name) {
            if let Some(mut parent) = old.get_parent() {
                parent.remove_child(&old);
            }
            old.queue_free();
        }
    }
    let mut fx = crate::fx::MindFx::new_alloc();
    // Silence the required-camera wiring warning when the game camera exists
    // (the preview never emits shake events, so it only satisfies the export).
    let camera = kit::node("/root/Spine/World/Camera2D")
        .and_then(|node| node.try_cast::<crate::camera::MindCamera2D>().ok());
    fx.bind_mut().camera = camera;
    let mut fx_node = fx.clone();
    fx_node.set_name("EffectPreviewFx");
    fx_node.set_z_index(100);
    let preview = Rc::new(RefCell::new(EffectsPreview {
        fx: fx.clone(),
        current: None,
        tick: 0,
    }));
    {
        let mut root: Gd<Control> = dialog.to_gd().upcast();
        let node: Gd<Node> = fx.upcast();
        root.add_child(&node);
    }
    // code-instantiated: the shared 20 Hz preview ticker (there is one per open
    // dialog, not one per cell).
    let mut timer = Timer::new_alloc();
    timer.set_name("EffectPreviewTick");
    timer.set_wait_time(EFFECTS_PREVIEW_TICK_SECONDS);
    timer.set_one_shot(false);
    timer.set_autostart(true);
    let preview_for_tick = preview.clone();
    let dialog_for_tick = dialog.to_gd();
    let callable = Callable::from_fn("effects_preview_tick", move |_args: &[&Variant]| {
        if !dialog_for_tick.is_instance_valid() || !dialog_for_tick.is_visible_in_tree() {
            return Variant::nil();
        }
        let mut state = preview_for_tick.borrow_mut();
        state.tick = state.tick.wrapping_add(1);
        let Some((fx_name, position, scale)) = state.current.clone() else {
            // Still advance the pool so the last effect finishes its lifetime.
            state.fx.clone().bind_mut().tick_view();
            return Variant::nil();
        };
        {
            let mut fx = state.fx.clone();
            fx.set_position(position);
            fx.set_scale(Vector2::splat(scale));
            fx.bind_mut().tick_view();
        }
        if state.tick.is_multiple_of(EFFECTS_PREVIEW_RESPAWN_TICKS) {
            state.fx.clone().bind_mut().spawn_effect(
                GString::from(fx_name.as_str()),
                0.0,
                0.0,
                0.0,
            );
        }
        Variant::nil()
    });
    timer.connect("timeout", &callable);
    {
        let mut root: Gd<Control> = dialog.to_gd().upcast();
        let node: Gd<Node> = timer.upcast();
        root.add_child(&node);
    }
    Some(preview)
}

/// One `EffectsDialog.EffectCell`: a clickable 280 px stack with the preview
/// and the bottom-left outlined name.
fn effects_cell(
    dialog: &mut MindDialog,
    preview: Option<&Rc<RefCell<EffectsPreview>>>,
    effect_name: &str,
) -> Gd<Button> {
    // code-instantiated: one preview cell per LogicFx entry.
    let mut button = Button::new_alloc();
    button.set_custom_minimum_size(Vector2::new(EFFECTS_CELL_SIZE, EFFECTS_CELL_SIZE));
    button.set_h_size_flags(SizeFlags::SHRINK_BEGIN);
    button.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
    button.set_focus_mode(FocusMode::NONE);
    for state in [
        "normal",
        "hover",
        "pressed",
        "disabled",
        "focus",
        "hover_pressed",
    ] {
        button.add_theme_stylebox_override(state, &StyleBoxEmpty::new_gd());
    }
    let mut preview_control = effects_preview(effect_name);
    preview_control.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    preview_control.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&preview_control);
    let mut label = effects_outline_label(effect_name);
    label.set_anchors_and_offsets_preset(LayoutPreset::BOTTOM_LEFT);
    label.set_offset(Side::LEFT, 6.0);
    label.set_offset(Side::BOTTOM, -4.0);
    label.set_offset(Side::TOP, -28.0);
    label.set_offset(Side::RIGHT, EFFECTS_CELL_SIZE - 6.0);
    label.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&label);
    button.set_tooltip_text(&GString::from(effect_name));
    let target = dialog.to_gd();
    let name_for_pick = effect_name.to_string();
    let callable = Callable::from_fn("effects_pick", move |_args: &[&Variant]| {
        if !target.is_instance_valid() {
            return;
        }
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        effects_pick(&mut bound, &name_for_pick);
    });
    button.connect("pressed", &callable);
    // Live preview (`EffectCell.draw`): the shared host moves to this cell and
    // loops its `Fx` while hovered. The HSV wash is the no-host fallback.
    if let Some(preview) = preview
        && let Some(fx_name) = effects_fx_name(effect_name)
    {
        let bounds = mind_core::logic::fx::get(effect_name)
            .map(|entry| entry.bounds)
            .unwrap_or(-1.0);
        let scale = effects_preview_scale(bounds);
        let cell_id = button.instance_id();
        let preview_in = preview.clone();
        let dialog_for = dialog.to_gd();
        let fx_name = fx_name.to_string();
        let enter = Callable::from_fn("effects_preview_enter", move |_args: &[&Variant]| {
            let Ok(cell) = Gd::<Control>::try_from_instance_id(cell_id) else {
                return Variant::nil();
            };
            if !dialog_for.is_instance_valid() {
                return Variant::nil();
            }
            let mut state = preview_in.borrow_mut();
            let centre = cell.get_global_rect().center();
            {
                let mut fx = state.fx.clone();
                fx.set_position(centre);
                fx.set_scale(Vector2::splat(scale));
                fx.bind_mut().tick_view();
            }
            state.current = Some((fx_name.clone(), centre, scale));
            state.tick = EFFECTS_PREVIEW_RESPAWN_TICKS - 1;
            Variant::nil()
        });
        button.connect("mouse_entered", &enter);
        let preview_out = preview.clone();
        let leave = Callable::from_fn("effects_preview_leave", move |_args: &[&Variant]| {
            preview_out.borrow_mut().current = None;
            Variant::nil()
        });
        button.connect("mouse_exited", &leave);
    }
    button
}

/// `LStatement.bundle(entry.name)` (`name.token.<name>` fallback: the name).
fn effects_outline_label(effect_name: &str) -> Gd<Label> {
    // code-instantiated: one label per effect cell.
    let mut label = Label::new_alloc();
    let key = format!("name.token.{effect_name}");
    let text = kit::t(&key).to_string();
    let label_text = if text == key {
        effect_name.to_string()
    } else {
        text
    };
    label.set_text(&GString::from(label_text.as_str()));
    label.add_theme_color_override("font_color", Color::WHITE);
    label.add_theme_color_override("font_outline_color", Color::BLACK);
    label.add_theme_constant_override("outline_size", 4);
    label
}

/// `EffectsDialog.EffectCell` background: the alpha checker and the border;
/// the live `Fx` itself is drawn by the shared preview host above the cell.
/// Cells without a resolvable `Fx` keep the animated HSV wash as a fallback.
fn effects_preview(effect_name: &str) -> Gd<Control> {
    // code-instantiated: preview layer of one effect cell.
    let mut preview = Control::new_alloc();
    preview.set_mouse_filter(MouseFilter::IGNORE);
    let with_wash = effects_fx_name(effect_name).is_none();
    let preview_for_draw = preview.clone();
    let callable = Callable::from_fn("effects_preview_draw", move |_args: &[&Variant]| {
        if !preview_for_draw.is_instance_valid() {
            return;
        }
        effects_draw_preview(&preview_for_draw, with_wash);
    });
    preview.connect("draw", &callable);
    if with_wash {
        // code-instantiated: the fallback animation ticker of one preview cell.
        let mut timer = Timer::new_alloc();
        timer.set_wait_time(1.0 / 60.0);
        timer.set_one_shot(false);
        timer.set_autostart(true);
        let preview_for_tick = preview.clone();
        let callable = Callable::from_fn("effects_preview_redraw", move |_args: &[&Variant]| {
            if !preview_for_tick.is_instance_valid() {
                return;
            }
            let mut node = preview_for_tick.clone();
            node.queue_redraw();
        });
        timer.connect("timeout", &callable);
        preview.add_child(&timer);
    }
    preview
}

/// `EffectsDialog.EffectCell.draw`: the checker, the optional HSV wash and the
/// border.
fn effects_draw_preview(preview: &Gd<Control>, with_wash: bool) {
    let mut canvas: Gd<Control> = preview.clone();
    let size = canvas.get_size();
    // `Tex.alphaBg`: a 16 px dark/light checker.
    let cell = 16.0f32;
    let mut y = 0.0f32;
    let mut row = 0i32;
    while y < size.y {
        let mut x = 0.0f32;
        let mut column = row;
        while x < size.x {
            let tint = if column % 2 == 0 {
                Color::from_rgba(0.35, 0.35, 0.35, 0.55)
            } else {
                Color::from_rgba(0.22, 0.22, 0.22, 0.55)
            };
            canvas.draw_rect(
                Rect2::new(Vector2::new(x, y), Vector2::new(cell, cell)),
                tint,
            );
            x += cell;
            column += 1;
        }
        y += cell;
        row += 1;
    }
    if with_wash {
        // `color.fromHsv((Time.globalTime * 2f) % 360f, 1f, 1f)`.
        let hue = (Time::singleton().get_ticks_msec() as f64 * 0.002) % 1.0;
        let mut wash = Color::from_hsv(hue, 1.0, 1.0);
        wash.a = 0.85;
        let centre = size * 0.5;
        let radius = size.x.min(size.y) * 0.28;
        canvas.draw_circle(centre, radius, wash);
        canvas.draw_circle(centre, radius * 0.55, Color::from_rgba(1.0, 1.0, 1.0, 0.35));
    }
    canvas
        .draw_rect_ex(Rect2::new(Vector2::ZERO, size), Color::BLACK)
        .filled(false)
        .width(3.0)
        .done();
}

/// `_pick`: emit `effect_selected(name)` and close.
fn effects_pick(dialog: &mut MindDialog, effect_name: &str) {
    dialog.base_mut().emit_signal(
        "effect_selected",
        &[GString::from(effect_name).to_variant()],
    );
    if let Some(mut ui) = kit::ui() {
        let closed = ui
            .call("close_dialog", &[GString::from("effects").to_variant()])
            .try_to::<bool>()
            .unwrap_or(false);
        if closed {
            return;
        }
    }
    dialog.hide_dialog();
}

// ---------------------------------------------------------------------------
// DatabaseController (`database`)
// ---------------------------------------------------------------------------

/// `Planets.sun` content name (`DatabaseDialog.tab`'s all-content default).
const DATABASE_ALL_TAB: &str = "sun";
/// `Vars.iconMed` (grid cell size).
const DATABASE_ICON_MED: f32 = 32.0;
/// `Styles.defaultPane` bar width (`MindThemeBuilder.SCROLL_BAR_WIDTH`): how
/// much narrower than its pref Arc lays out `all`.
const DATABASE_ARC_BAR_SQUEEZE: i32 = 24;
/// `Pal.gray` (#454545): tag headers and lock/cancel overlays.
const DATABASE_PAL_GRAY: Color = Color::from_rgba8(0x45, 0x45, 0x45, 0xff);
/// `Color.scarlet` at the banned overlay's alpha (`Tmp.c1.set(Color.scarlet).a(0.6f)`).
const DATABASE_SCARLET: Color = Color::from_rgba(1.0, 52.0 / 255.0, 28.0 / 255.0, 0.6);

/// Meta key recording the connected `size_changed` rebuild hook.
const DATABASE_RESIZE_META: &str = "mind_database_resize_bound";

/// Port-only Database tab over `Achievement` (GAPS #68 UI half). Upstream has
/// no achievement dialog (Steam's overlay showed them), so there is no Java
/// label to port; the rail uses a star glyph and the tooltip names the table.
const DATABASE_ACHIEVEMENTS_TAB: &str = "achievements";
/// Port-only Database tab over `SStat` (GAPS #68 UI half).
const DATABASE_STATISTICS_TAB: &str = "statistics";
/// Glyph for the achievements tab (`Fonts.icon` star).
const DATABASE_ACHIEVEMENTS_ICON: &str = "star";
/// Glyph for the statistics tab (`Fonts.icon` chart-bar).
const DATABASE_STATISTICS_ICON: &str = "chart-bar";

/// Rebuild handles shared with the search/tab/resize callbacks.
#[derive(Default)]
struct DatabaseState {
    tabs: Option<Gd<GridContainer>>,
    grid: Option<Gd<VBoxContainer>>,
    search: Option<Gd<LineEdit>>,
    view: VarDictionary,
    tab: String,
    /// Port-only tabs (`achievements`/`statistics`) with a non-empty row set.
    special: Vec<String>,
}

/// `@database` — search field, planet tabs and the
/// `databaseCategory`/`databaseTag` content grid over the live
/// `MindCampaign.database_json` model.
pub struct DatabaseController {
    state: Rc<RefCell<DatabaseState>>,
}

impl Default for DatabaseController {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(DatabaseState {
                tab: DATABASE_ALL_TAB.to_string(),
                ..DatabaseState::default()
            })),
        }
    }
}

impl DialogController for DatabaseController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `cont.top()` (`DatabaseDialog.java:57`): the body sticks to the top.
        dialog.apply_cont_top(true);
        dialog.set_title_key("@database");
        dialog.should_pause = true;
        // Java `BaseDialog`: the `cont.pane(all)` grid fills the viewport.
        dialog.fill_panel();
        dialog.clear_content();
        let state = self.state.clone();
        database_build(dialog, &state);
        dialog.add_close_button(210.0);
        database_connect_resize(dialog, &state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let state = self.state.clone();
        database_load_view(dialog, &state);
    }
}

/// `_build`: the search row, the scrolling `all` table (tab rail + grid) and
/// its `size_changed` rebuild hook.
fn database_build(dialog: &mut MindDialog, state: &Rc<RefCell<DatabaseState>>) {
    let mut table = dialog.content_table();
    {
        let mut control: Gd<Control> = table.clone().upcast();
        control.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    }
    // `cont.table(s -> { s.image(Icon.zoom).padRight(8); search = s.field(..) })`.
    // code-instantiated: the search row is part of the dialog body built in code.
    let mut search_row = HBoxContainer::new_alloc();
    search_row.add_theme_constant_override("separation", 8);
    search_row.add_child(&factory::glyph_label("zoom", 30, Color::WHITE));
    let mut search = factory::field(&kit::t("@players.search").to_string());
    search.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let target = dialog.to_gd();
        let state_for_search = state.clone();
        let callable = Callable::from_fn("database_search", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return;
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            database_rebuild(&mut bound, &state_for_search);
        });
        search.connect("text_changed", &callable);
    }
    search_row.add_child(&search);
    table.bind_mut().add(&search_row).grow_x().pad_bottom(4.0);
    table.bind_mut().row();

    // `cont.pane(all).scrollX(false)` (`DatabaseDialog.java:64`): only `all`
    // scrolls; the shell's Body stops scrolling and the pane takes the height
    // down to the button row.
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
    }
    // code-instantiated: the pane wraps the runtime-built `all` table.
    let mut pane = MindScroll::new_alloc();
    pane.bind_mut().set_scrolling_disabled(true, false);
    // code-instantiated: `all` holds the live tab rail and category grid.
    let mut all = MindTable::new_alloc();
    all.bind_mut().set_table_top(true);
    {
        let control: Gd<Control> = all.clone().upcast();
        pane.add_child(&control);
    }
    table.bind_mut().add(&pane).grow_y();

    // code-instantiated: the tab rail is the data-driven planet tab list
    // (`t.button(..., Styles.clearNoneTogglei, iconMed, ...).size(50f)`, 10 per
    // row).
    let mut tabs = GridContainer::new_alloc();
    tabs.set_columns(10);
    tabs.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    tabs.add_theme_constant_override("h_separation", 0);
    tabs.add_theme_constant_override("v_separation", 0);
    all.bind_mut().add(&tabs).pad_left(20.0).pad_right(30.0);
    all.bind_mut().row();

    // `all.margin(20).marginTop(0f).marginRight(30f)`: Godot's pane adds the
    // scroll bar to its minimum width, so give the bar's width back here (the
    // category rule takes it as extra right padding).
    // code-instantiated: the category/tag grid is live content, not scene state.
    let mut margin = MarginContainer::new_alloc();
    margin.add_theme_constant_override("margin_left", 20);
    margin.add_theme_constant_override("margin_right", 30 - DATABASE_ARC_BAR_SQUEEZE);
    margin.add_theme_constant_override("margin_top", 0);
    margin.add_theme_constant_override("margin_bottom", 20);
    let mut grid = VBoxContainer::new_alloc();
    grid.add_theme_constant_override("separation", 0);
    margin.add_child(&grid);
    all.bind_mut().add(&margin).grow_x();

    {
        let mut st = state.borrow_mut();
        st.tabs = Some(tabs);
        st.grid = Some(grid);
        st.search = Some(search);
    }
    database_load_view(dialog, state);
}

/// The dialog's viewport `size_changed` -> grid rebuild (`onResize(this::rebuild)`).
fn database_connect_resize(dialog: &mut MindDialog, state: &Rc<RefCell<DatabaseState>>) {
    let mut node = dialog.to_gd();
    if node.has_meta(DATABASE_RESIZE_META) {
        return;
    }
    let Some(mut viewport) = dialog.base().get_viewport() else {
        return;
    };
    let target = node.clone();
    let state_for_resize = state.clone();
    let callable = Callable::from_fn("database_resize", move |_args: &[&Variant]| {
        if !target.is_instance_valid() {
            return;
        }
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        database_rebuild(&mut bound, &state_for_resize);
    });
    viewport.connect("size_changed", &callable);
    node.set_meta(DATABASE_RESIZE_META, &true.to_variant());
}

/// The `{tabs: [], categories: []}` fallback model.
fn database_empty_view() -> VarDictionary {
    let mut view = VarDictionary::new();
    view.set("tabs", &VarArray::new());
    view.set("categories", &VarArray::new());
    view
}

/// Reads the live database model from `MindCampaign` and repaints the rail and
/// the grid.
///
/// The two port-only tabs (`achievements`/`statistics`, GAPS #68 UI half) are
/// appended when their row set is non-empty: a profile whose settings store
/// cannot be read shows no empty tab, and no rows are ever invented.
fn database_load_view(dialog: &mut MindDialog, state: &Rc<RefCell<DatabaseState>>) {
    let mut view = dialog.campaign_json("database_json", &[]);
    if view.is_empty() {
        view = database_empty_view();
    }
    let mut tab = state.borrow().tab.clone();
    let special = database_special_tabs(dialog);
    let tabs = dict_array(&view, "tabs");
    let mut names: Vec<String> = tabs
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .map(|row| dict_text(&row, "name"))
        .collect();
    names.extend(special.iter().cloned());
    if !tabs.is_empty() {
        // `DatabaseDialog.shown`: in a campaign the active tab follows the
        // played planet (`state.getPlanet()`) when the tab list has it.
        let mut current_planet = String::new();
        if let Some(mut campaign) = dialog.campaign_node()
            && campaign.has_method("get_sector_state")
        {
            let sector_state = campaign.call("get_sector_state", &[]).to::<VarDictionary>();
            if dict_bool(&sector_state, "campaign") {
                current_planet = dict_text(&sector_state, "planet");
            }
        }
        if !current_planet.is_empty() && names.contains(&current_planet) {
            tab = current_planet;
        } else if !names.contains(&tab) {
            tab = tabs
                .get(0)
                .and_then(|value| value.try_to::<VarDictionary>().ok())
                .map(|row| dict_text(&row, "name"))
                .unwrap_or_else(|| DATABASE_ALL_TAB.to_string());
        }
    }
    {
        let mut st = state.borrow_mut();
        st.view = view;
        st.tab = tab;
        st.special = special;
    }
    let node = dialog.to_gd();
    database_build_tabs(&node, state);
    database_rebuild(dialog, state);
}

/// The port-only tabs with rows: `achievements`/`statistics` (GAPS #68 UI
/// half), in tab order.
fn database_special_tabs(dialog: &MindDialog) -> Vec<String> {
    let mut tabs = Vec::new();
    for (kind, label) in [
        (DATABASE_ACHIEVEMENTS_TAB, DATABASE_ACHIEVEMENTS_TAB),
        (DATABASE_STATISTICS_TAB, DATABASE_STATISTICS_TAB),
    ] {
        if database_table_rows(dialog, label) > 0 {
            tabs.push(kind.to_string());
        }
    }
    if !tabs.is_empty() {
        log::info!("[ui] database tables: {}", tabs.join(", "));
    }
    tabs
}

/// The row JSON of a port-only tab from `MindUi.database_rows_json(kind)`.
///
/// Guarded twice: the `MindUi` autoload may be absent (unit hosts) and the
/// method is new (older .so), so a missing seam answers `[]` — the tab then
/// stays hidden rather than rendering fabricated entries.
fn database_table_rows_json(dialog: &MindDialog, kind: &str) -> String {
    let _ = dialog;
    let Some(mut ui) = kit::ui() else {
        return String::from("[]");
    };
    if !ui.has_method("database_rows_json") {
        return String::from("[]");
    }
    ui.call("database_rows_json", &[GString::from(kind).to_variant()])
        .try_to::<GString>()
        .unwrap_or_default()
        .to_string()
}

/// Parsed achievement rows for the tab.
fn database_achievement_rows(dialog: &MindDialog) -> Vec<kit::rows::AchievementRow> {
    kit::rows::parse_achievements(&database_table_rows_json(dialog, DATABASE_ACHIEVEMENTS_TAB))
}

/// Parsed statistics rows for the tab.
fn database_stat_rows(dialog: &MindDialog) -> Vec<kit::rows::StatRow> {
    kit::rows::parse_stats(&database_table_rows_json(dialog, DATABASE_STATISTICS_TAB))
}

/// Rows of a port-only tab (used for the tab-presence check).
fn database_table_rows(dialog: &MindDialog, kind: &str) -> usize {
    match kind {
        DATABASE_ACHIEVEMENTS_TAB => database_achievement_rows(dialog).len(),
        DATABASE_STATISTICS_TAB => database_stat_rows(dialog).len(),
        _ => 0,
    }
}

/// `_build_tabs`: one 50 px icon toggle per planet tab.
fn database_build_tabs(dialog: &Gd<MindDialog>, state: &Rc<RefCell<DatabaseState>>) {
    let (mut tabs_node, view, tab, special) = {
        let st = state.borrow();
        let Some(tabs) = st.tabs.clone() else {
            return;
        };
        (tabs, st.view.clone(), st.tab.clone(), st.special.clone())
    };
    if !tabs_node.is_instance_valid() {
        return;
    }
    for child in tabs_node.get_children().iter_shared() {
        let mut child = child.clone();
        tabs_node.remove_child(&child);
        child.queue_free();
    }
    let group = ButtonGroup::new_gd();
    let mut rows: Vec<VarDictionary> = dict_array(&view, "tabs")
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect();
    // Port-only tabs over the achievement/stat tables (GAPS #68 UI half): the
    // rail is data-driven, so they join it as synthetic rows with a glyph and
    // the table name as their tooltip (upstream has no labels for these).
    for name in &special {
        let icon = if name == DATABASE_ACHIEVEMENTS_TAB {
            DATABASE_ACHIEVEMENTS_ICON
        } else {
            DATABASE_STATISTICS_ICON
        };
        let mut row = VarDictionary::new();
        row.set("name", &GString::from(name.as_str()));
        row.set("localized", &GString::from(name.as_str()));
        row.set("icon", &GString::from(icon));
        row.set("is_all", &false.to_variant());
        rows.push(row);
    }
    for tab_row in rows {
        let tab_name = dict_text(&tab_row, "name");
        let is_all = dict_bool(&tab_row, "is_all");
        let label = database_tab_label(&tab_row, is_all);
        let color = if is_all {
            Color::WHITE
        } else {
            database_hex_color(&dict_text(&tab_row, "color"))
        };
        let icon_name = dict_text(&tab_row, "icon");
        // code-instantiated: one icon toggle per planet tab (`Icon.eyeSmall`
        // for the sun, else the planet glyph tinted `p.iconColor`).
        let mut button = Button::new_alloc();
        button.set_name(&format!("tab-{tab_name}"));
        button.set_theme_type_variation("clearNoneTogglei");
        button.set_tooltip_text(&GString::from(label.as_str()));
        button.set_toggle_mode(true);
        button.set_button_group(&group);
        button.set_pressed_no_signal(tab_name == tab);
        button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
        database_apply_clear_none_toggle(&mut button);
        if kit::glyph(&icon_name).is_empty() {
            button.set_text(&GString::from(label.as_str()));
        } else {
            let mut glyph = factory::glyph_label(&icon_name, 32, color);
            glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            glyph.set_mouse_filter(MouseFilter::IGNORE);
            button.add_child(&glyph);
        }
        {
            let target = dialog.clone();
            let state_for_tab = state.clone();
            let name_for_tab = tab_name.clone();
            let callable = Callable::from_fn("database_select_tab", move |_args: &[&Variant]| {
                if !target.is_instance_valid() {
                    return;
                }
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                database_select_tab(&mut bound, &state_for_tab, &name_for_tab);
            });
            button.connect("pressed", &callable);
        }
        tabs_node.add_child(&button);
    }
}

/// `Styles.clearNoneTogglei` (`Styles.java:351-356`): `up = none`,
/// `over = flatOver`, `down`/`checked` = `flatDown` (black centre in a 4 px
/// `Pal.accent` frame).
fn database_apply_clear_none_toggle(button: &mut Gd<Button>) {
    let mut over = StyleBoxFlat::new_gd();
    over.set_bg_color(styles::FLAT_OVER);
    let mut checked = StyleBoxFlat::new_gd();
    checked.set_bg_color(Color::BLACK);
    checked.set_border_color(styles::ACCENT);
    checked.set_border_width_all(4);
    button.add_theme_stylebox_override("normal", &StyleBoxEmpty::new_gd());
    button.add_theme_stylebox_override("hover", &over);
    button.add_theme_stylebox_override("pressed", &checked);
    button.add_theme_stylebox_override("hover_pressed", &checked);
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
}

/// `_select_tab`.
fn database_select_tab(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<DatabaseState>>,
    tab_name: &str,
) {
    state.borrow_mut().tab = tab_name.to_string();
    database_build_tabs(&dialog.to_gd(), state);
    database_rebuild(dialog, state);
}

/// `_rebuild`: the filtered `databaseCategory`/`databaseTag` grid.
fn database_rebuild(dialog: &mut MindDialog, state: &Rc<RefCell<DatabaseState>>) {
    let node = dialog.to_gd();
    let (mut grid, view, tab, query) = {
        let st = state.borrow();
        let Some(grid) = st.grid.clone() else {
            return;
        };
        let query = st
            .search
            .as_ref()
            .map(|field| field.get_text().to_string().to_lowercase())
            .unwrap_or_default();
        (grid, st.view.clone(), st.tab.clone(), query)
    };
    if !grid.is_instance_valid() {
        return;
    }
    for child in grid.get_children().iter_shared() {
        let mut child = child.clone();
        grid.remove_child(&child);
        child.queue_free();
    }
    // `cols = clamp((graphics.getWidth() - Scl.scl(30)) / Scl.scl(32 + 12), 1, 22)`.
    let columns = (((viewport_width(dialog) - 30.0) / 44.0) as i32).clamp(1, 22);
    // Port-only tables (GAPS #68 UI half): they replace the content grid.
    if tab == DATABASE_ACHIEVEMENTS_TAB || tab == DATABASE_STATISTICS_TAB {
        database_rebuild_tables(dialog, &mut grid, &tab, &query);
        return;
    }
    let mut has_result = false;
    for category_value in dict_array(&view, "categories").iter_shared() {
        let Ok(category) = category_value.try_to::<VarDictionary>() else {
            continue;
        };
        let rows = database_tag_rows(&category, &query, &tab);
        if rows.is_empty() {
            continue;
        }
        has_result = true;
        // `all.add("@database-category." + name).growX().left().color(Pal.accent)`.
        let category_label =
            factory::label(&kit::t(&format!("@{}", dict_text(&category, "label"))).to_string());
        {
            let mut control: Gd<Control> = category_label.clone().upcast();
            control.set_modulate(styles::ACCENT);
        }
        grid.add_child(&category_label);
        // `all.image().pad(5).padLeft(0).padRight(0).height(3).color(Pal.accent).growX()`.
        grid.add_child(&database_padded(
            database_divider(styles::ACCENT),
            0,
            5,
            DATABASE_ARC_BAR_SQUEEZE,
            5,
        ));
        // `all.table(sub -> ...).growX().left().padBottom(10)`.
        // code-instantiated: one section per category with live tag rows.
        let mut sub = VBoxContainer::new_alloc();
        sub.add_theme_constant_override("separation", 0);
        for row in rows {
            let tag_name = dict_text(&row, "name");
            if tag_name != "default" {
                // code-instantiated: tag header rows follow the live tag list.
                let mut header = HBoxContainer::new_alloc();
                header.add_theme_constant_override("separation", 0);
                let tag_label =
                    factory::label(&kit::t(&format!("@{}", dict_text(&row, "label"))).to_string());
                {
                    let mut control: Gd<Control> = tag_label.clone().upcast();
                    control.set_modulate(DATABASE_PAL_GRAY);
                }
                {
                    let mut rich_tag: Gd<RichTextLabel> = tag_label.clone().upcast();
                    rich_tag.set_autowrap_mode(AutowrapMode::OFF);
                    rich_tag.set_fit_content(true);
                }
                header.add_child(&tag_label);
                let mut line = database_padded(database_divider(DATABASE_PAL_GRAY), 5, 5, 5, 5);
                line.set_h_size_flags(SizeFlags::EXPAND_FILL);
                line.set_v_size_flags(SizeFlags::SHRINK_CENTER);
                header.add_child(&line);
                sub.add_child(&database_padded(header, 8, 4, 8, 4));
            }
            // `sub.table(list -> ...)`: `cols` cells of `iconMed` padded 3, the
            // row topped up with clear cells.
            // code-instantiated: one grid per `databaseTag` section.
            let mut list = GridContainer::new_alloc();
            list.set_name(&format!(
                "grid-{}-{}",
                dict_text(&category, "name"),
                tag_name
            ));
            list.set_columns(columns);
            list.add_theme_constant_override("h_separation", 6);
            list.add_theme_constant_override("v_separation", 6);
            let entries = dict_array(&row, "entries");
            for entry in entries.iter_shared() {
                list.add_child(&database_entry_button(&node, &entry));
            }
            let fillers = (columns - entries.len() as i32).max(0);
            for _ in 0..fillers {
                // code-instantiated: clear filler cells keep the grid `cols` wide.
                let mut filler = Control::new_alloc();
                filler.set_custom_minimum_size(Vector2::new(DATABASE_ICON_MED, DATABASE_ICON_MED));
                filler.set_mouse_filter(MouseFilter::IGNORE);
                list.add_child(&filler);
            }
            sub.add_child(&database_padded(list, 3, 3, 3, 3));
        }
        grid.add_child(&database_padded(sub, 0, 0, 0, 10));
    }
    if !has_result {
        grid.add_child(&factory::label(&kit::t("@none.found").to_string()));
    }
}

/// The achievements/statistics table body (port-only Database tabs).
///
/// Achievements render `icon name progress` rows (unlocked `accent`, locked
/// `Pal.gray`; the progress column is `3/10` or `unlocked`), statistics render
/// `name value` rows. Both honour the search query and the accent rule the
/// content categories use (`DatabaseDialog.rebuild`'s header treatment).
fn database_rebuild_tables(
    dialog: &MindDialog,
    grid: &mut Gd<VBoxContainer>,
    tab: &str,
    query: &str,
) {
    let (header, mut lines, completion) = if tab == DATABASE_ACHIEVEMENTS_TAB {
        let rows = database_achievement_rows(dialog);
        let completion = kit::rows::completion(&rows);
        let lines: Vec<TableLine> = rows
            .into_iter()
            .filter(|row| query.is_empty() || row.name.to_lowercase().contains(query))
            .map(|row| TableLine {
                icon: if row.unlocked { "ok" } else { "cancel" },
                color: if row.unlocked {
                    styles::ACCENT
                } else {
                    DATABASE_PAL_GRAY
                },
                name: row.name.clone(),
                value: kit::rows::progress_text(&row),
                accent: kit::rows::row_color(&row) == "accent",
            })
            .collect();
        (DATABASE_ACHIEVEMENTS_TAB, lines, Some(completion))
    } else {
        let rows = database_stat_rows(dialog);
        let lines: Vec<TableLine> = rows
            .into_iter()
            .filter(|row| query.is_empty() || row.name.to_lowercase().contains(query))
            .map(|row| TableLine {
                icon: "",
                color: Color::WHITE,
                name: row.name.clone(),
                value: kit::rows::stat_value_text(&row),
                accent: false,
            })
            .collect();
        (DATABASE_STATISTICS_TAB, lines, None)
    };
    if lines.is_empty() {
        grid.add_child(&factory::label(&kit::t("@none.found").to_string()));
        return;
    }
    // Category header + the 3 px accent rule (`all.image().height(3)`).
    let mut title = header.to_string();
    if let Some((unlocked, total)) = completion {
        title = format!("{header}  {unlocked}/{total}");
    }
    let label = factory::label(&title);
    {
        let mut control: Gd<Control> = label.clone().upcast();
        control.set_modulate(styles::ACCENT);
    }
    grid.add_child(&label);
    grid.add_child(&database_padded(
        database_divider(styles::ACCENT),
        0,
        5,
        DATABASE_ARC_BAR_SQUEEZE,
        5,
    ));
    // code-instantiated: one row per table entry, rebuilt on every refresh.
    let mut list = VBoxContainer::new_alloc();
    list.add_theme_constant_override("separation", 4);
    for line in lines.drain(..) {
        // code-instantiated: `icon name ... value` row.
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 8);
        if !line.icon.is_empty() {
            let mut glyph = factory::glyph_label(line.icon, 24, line.color);
            glyph.set_v_size_flags(SizeFlags::SHRINK_CENTER);
            row.add_child(&glyph);
        }
        let name = factory::label(&line.name);
        {
            let mut control: Gd<Control> = name.clone().upcast();
            control.set_modulate(line.color);
            control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        }
        row.add_child(&name);
        let value_label = factory::label(&line.value);
        {
            let mut control: Gd<Control> = value_label.clone().upcast();
            control.set_modulate(if line.accent {
                styles::ACCENT
            } else {
                DATABASE_PAL_GRAY
            });
        }
        {
            let mut rich: Gd<RichTextLabel> = value_label.clone().upcast();
            rich.set_horizontal_alignment(HorizontalAlignment::RIGHT);
        }
        row.add_child(&value_label);
        list.add_child(&row);
    }
    grid.add_child(&database_padded(list, 8, 4, 8, 12));
}

/// One rendered row of a port-only table.
struct TableLine {
    icon: &'static str,
    color: Color,
    name: String,
    value: String,
    accent: bool,
}

/// Non-empty tag rows for the active tab and search query.
fn database_tag_rows(category: &VarDictionary, query: &str, tab: &str) -> Vec<VarDictionary> {
    let mut rows = Vec::new();
    for tag_value in dict_array(category, "tags").iter_shared() {
        let Ok(tag) = tag_value.try_to::<VarDictionary>() else {
            continue;
        };
        let mut entries = VarArray::new();
        for entry_value in dict_array(&tag, "entries").iter_shared() {
            let Ok(entry) = entry_value.try_to::<VarDictionary>() else {
                continue;
            };
            if !database_entry_in_tab(&entry, tab) {
                continue;
            }
            if !query.is_empty() && !database_entry_matches(&entry, query) {
                continue;
            }
            entries.push(&entry.to_variant());
        }
        if entries.is_empty() {
            continue;
        }
        let mut row = VarDictionary::new();
        row.set("name", &GString::from(dict_text(&tag, "name").as_str()));
        row.set("label", &GString::from(dict_text(&tag, "label").as_str()));
        row.set("entries", &entries);
        rows.push(row);
    }
    rows
}

/// `tab == Planets.sun || allDatabaseTabs || databaseTabs.contains(tab)`.
fn database_entry_in_tab(entry: &VarDictionary, tab: &str) -> bool {
    if tab == DATABASE_ALL_TAB || dict_bool(entry, "all_tabs") {
        return true;
    }
    dict_array(entry, "tabs").iter_shared().any(|value| {
        value
            .try_to::<GString>()
            .map(|text| text == tab)
            .unwrap_or(false)
    })
}

/// `_entry_matches`.
fn database_entry_matches(entry: &VarDictionary, query: &str) -> bool {
    if dict_text(entry, "name").to_lowercase().contains(query) {
        return true;
    }
    database_entry_label(entry).to_lowercase().contains(query)
}

/// One grid cell: the content icon when unlocked (`ui.content.show`), else the
/// gray lock glyph (`new Image(Icon.lock, Pal.gray)`).
fn database_entry_button(dialog: &Gd<MindDialog>, entry_value: &Variant) -> Gd<Button> {
    let entry = entry_value.try_to::<VarDictionary>().unwrap_or_default();
    let entry_name = dict_text(&entry, "name");
    let unlocked = dict_bool(&entry, "unlocked");
    let mut button = if unlocked {
        let type_name = dict_text(&entry, "content_type");
        let regions = [
            format!("{type_name}-{entry_name}-ui"),
            format!("{type_name}-{entry_name}-full"),
            entry_name.clone(),
        ];
        let region_refs: Vec<&str> = regions.iter().map(|region| region.as_str()).collect();
        // code-instantiated: content icons are data-driven (atlas fallback chain).
        let mut image_button = factory::image_button_first(&region_refs, "selecti");
        image_button.set_expand_icon(true);
        image_button.add_theme_constant_override("icon_max_width", DATABASE_ICON_MED as i32);
        // `new Image(uiIcon, Color.lightGray)` lerping to white while hovered.
        image_button.add_theme_color_override("icon_normal_color", styles::ARC_LIGHT_GRAY);
        image_button.add_theme_color_override("icon_hover_color", Color::WHITE);
        image_button.add_theme_color_override("icon_pressed_color", Color::WHITE);
        image_button.add_theme_color_override("icon_hover_pressed_color", Color::WHITE);
        image_button.add_theme_color_override("icon_focus_color", styles::ARC_LIGHT_GRAY);
        for state in ["normal", "hover", "pressed", "hover_pressed", "focus"] {
            image_button.add_theme_stylebox_override(state, &StyleBoxEmpty::new_gd());
        }
        if image_button.get_button_icon().is_none() {
            image_button.set_text(&GString::from(database_entry_label(&entry).as_str()));
        }
        {
            let target = dialog.clone();
            let name_for_info = entry_name.clone();
            let callable =
                Callable::from_fn("database_content_info", move |_args: &[&Variant]| {
                    open_content_info(&target, &name_for_info);
                });
            image_button.connect("pressed", &callable);
        }
        image_button
    } else {
        // code-instantiated: locked cells carry the gray lock glyph.
        let mut locked = Button::new_alloc();
        locked.set_theme_type_variation("selecti");
        let mut lock =
            factory::glyph_label("lock", (DATABASE_ICON_MED * 0.75) as i32, DATABASE_PAL_GRAY);
        lock.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        lock.set_mouse_filter(MouseFilter::IGNORE);
        locked.add_child(&lock);
        locked
    };
    if dict_bool(&entry, "banned") {
        // `new Image(Icon.cancel, Tmp.c1.set(Color.scarlet).a(0.6f))` over the
        // content icon when in a game with the block banned.
        let mut cross = factory::glyph_label("cancel", DATABASE_ICON_MED as i32, DATABASE_SCARLET);
        cross.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        cross.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&cross);
    }
    button.set_name(&format!("content-{entry_name}"));
    // `.size(iconMed).pad(3)` (the pad is the grid separation).
    button.set_custom_minimum_size(Vector2::new(DATABASE_ICON_MED, DATABASE_ICON_MED));
    button.set_meta("unlocked", &unlocked.to_variant());
    if unlocked {
        button.set_tooltip_text(&GString::from(database_entry_label(&entry).as_str()));
    }
    button
}

/// Wraps `control` in a margin box (Arc cell pads: left, top, right, bottom).
fn database_padded<C: Inherits<Control>>(
    control: Gd<C>,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
) -> Gd<MarginContainer> {
    // code-instantiated: cell padding for a runtime section row.
    let mut box_ = MarginContainer::new_alloc();
    box_.add_theme_constant_override("margin_left", left);
    box_.add_theme_constant_override("margin_top", top);
    box_.add_theme_constant_override("margin_right", right);
    box_.add_theme_constant_override("margin_bottom", bottom);
    box_.set_mouse_filter(MouseFilter::IGNORE);
    let child: Gd<Control> = control.upcast();
    box_.add_child(&child);
    box_
}

/// Section divider (`all.image().height(3)` with the given color).
fn database_divider(color: Color) -> Gd<ColorRect> {
    // code-instantiated: one divider per runtime section row.
    let mut line = ColorRect::new_alloc();
    line.set_color(color);
    line.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    line.set_h_size_flags(SizeFlags::EXPAND_FILL);
    line
}

/// Tab label: `@all` for the sun tab, else the localized planet name.
fn database_tab_label(tab: &VarDictionary, is_all: bool) -> String {
    if is_all {
        return kit::t("@all").to_string();
    }
    bundle_or(&dict_text(tab, "localized"), &dict_text(tab, "name"))
}

/// Entry label: bundle key when the model sent one (`<type>.<name>.name`).
fn database_entry_label(entry: &VarDictionary) -> String {
    bundle_or(&dict_text(entry, "localized"), &dict_text(entry, "name"))
}

/// `rrggbb` planet icon color -> Godot color (white when absent/malformed).
fn database_hex_color(value: &str) -> Color {
    if value.len() != 6 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Color::WHITE;
    }
    let channel = |range: std::ops::Range<usize>| -> u8 {
        u8::from_str_radix(&value[range], 16).unwrap_or(0)
    };
    Color::from_rgba8(channel(0..2), channel(2..4), channel(4..6), 0xff)
}

// ---------------------------------------------------------------------------
// ContentController (`content`)
// ---------------------------------------------------------------------------

/// `ContentInfoDialog` header icon edge (`title1.image(...).size(iconXLarge)`).
const INFO_HEADER_ICON: f32 = 48.0;
/// Description label width (`description.width(500f)`).
const INFO_DESCRIPTION_WIDTH: f32 = 500.0;
/// Details label width (`details.width(400f)`).
const INFO_DETAIL_WIDTH: f32 = 400.0;
/// The unlockable content types probed by the bundle-only fallback.
const INFO_CONTENT_TYPES: [&str; 8] = [
    "block", "item", "unit", "liquid", "status", "weather", "sector", "planet",
];

/// `@info.title` — the content stats/description panel over the live
/// `MindCampaign.content_info_json(content)` payload, with the `MindAssets`
/// bundle fallback.
#[derive(Default)]
pub struct ContentController {
    table: Option<Gd<MindTable>>,
}

impl DialogController for ContentController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@info.title");
        dialog.should_pause = false;
        // Java `ContentInfoDialog extends BaseDialog` (`setFillParent(true)`).
        dialog.fill_panel();
        dialog.clear_content();
        let mut table = dialog.content_table();
        {
            let mut control: Gd<Control> = table.clone().upcast();
            control.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        }
        // `table.margin(10)` (+ room for the pane's scroll bar).
        table.bind_mut().margin(10.0);
        self.table = Some(table);
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let Some(mut table) = self.table.clone() else {
            return;
        };
        table.bind_mut().clear();
        // `ContentInfoDialog.show` rebuilds `cont.pane(...)` per open, so every
        // page starts at the top; the reused body starts at scroll 0.
        if let Some(mut body) = dialog.body() {
            body.set_v_scroll(0);
        }
        let content_name = match dialog.context_value("content") {
            Some(value) => variant_text(&value),
            None => dialog.context_string("name").to_string(),
        };
        if content_name.is_empty() {
            let label = factory::label(&kit::t("@none").to_string());
            table.bind_mut().add(&label).pad(8.0);
            return;
        }
        let mut info = dialog.campaign_json(
            "content_info_json",
            &[GString::from(content_name.as_str()).to_variant()],
        );
        if info.is_empty() {
            info = content_bundle_info(&content_name);
        }
        content_show_header(&mut table, &info, &content_name);
        if dict_bool(&info, "patched") {
            let label = factory::label(&kit::tm("@database.patched").to_string());
            table.bind_mut().add(&label).pad(4.0).left();
            table.bind_mut().row();
        }
        let categories = dict_array(&info, "categories");
        let stats = dict_array(&info, "stats");
        let has_stats = !categories.is_empty() || !stats.is_empty();
        let use_categories = dict_bool(&info, "use_categories");
        let description = dict_text(&info, "description");
        if !description.is_empty() {
            if has_stats {
                // `table.add("@category.purpose").color(Pal.accent).fillX().padTop(10)`
                let accent = content_accent_label(&kit::t("@category.purpose").to_string());
                table.bind_mut().add(&accent).grow_x().pad_top(10.0).left();
                table.bind_mut().row();
            }
            // code-instantiated: description text comes from the live content.
            let description_label =
                factory::label(&kit::markup_text(&format!("[lightgray]{description}")).to_string());
            {
                let mut rich_label: Gd<RichTextLabel> = description_label.clone().upcast();
                rich_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
            }
            {
                let mut control: Gd<Control> = description_label.clone().upcast();
                let mut min = control.get_custom_minimum_size();
                min.x = INFO_DESCRIPTION_WIDTH;
                control.set_custom_minimum_size(min);
            }
            table
                .bind_mut()
                .add(&description_label)
                .grow_x()
                .pad_left(if has_stats { 10.0 } else { 0.0 })
                .left();
            table.bind_mut().row();
            if !use_categories && has_stats {
                let accent = content_accent_label(&kit::t("@category.general").to_string());
                table.bind_mut().add(&accent).grow_x().left();
                table.bind_mut().row();
            }
        }
        let node = dialog.to_gd();
        if !categories.is_empty() {
            content_render_categories(&node, &mut table, &categories, use_categories);
        } else {
            content_render_stats(&node, &mut table, &stats);
        }
        let details = dict_text(&info, "details");
        let hide_details = dict_bool(&info, "hide_details");
        let locked = dict_bool(&info, "locked");
        if !details.is_empty() {
            let mut text = kit::markup_text(&format!("[gray]{details}")).to_string();
            if locked && hide_details {
                // Java `Iconc.lock + " " + Core.bundle.get("unlock.incampaign")`.
                text = format!("[icon name=\"lock\"] {}", kit::tm("@unlock.incampaign"));
            }
            // code-instantiated: details text comes from the live content.
            let detail_label = factory::label(&text);
            {
                let mut rich_label: Gd<RichTextLabel> = detail_label.clone().upcast();
                rich_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
            }
            {
                let mut control: Gd<Control> = detail_label.clone().upcast();
                let mut min = control.get_custom_minimum_size();
                min.x = INFO_DETAIL_WIDTH;
                control.set_custom_minimum_size(min);
            }
            table
                .bind_mut()
                .add(&detail_label)
                .grow_x()
                .pad(6.0)
                .pad_top(20.0)
                .left();
            table.bind_mut().row();
        }
        let credit = dict_text(&info, "credit");
        if !credit.is_empty() {
            let label = factory::label(
                &kit::tm_format("@content.createdby", &[credit.as_str()]).to_string(),
            );
            table.bind_mut().add(&label).pad_top(40.0).left();
            table.bind_mut().row();
        }
    }
}

/// `Dictionary.get(key, fallback)` with GDScript `str()` coercion.
fn dict_text_or(dict: &VarDictionary, key: &str, fallback: &str) -> String {
    match dict.get(key) {
        Some(value) => variant_text(&value),
        None => fallback.to_string(),
    }
}

/// Header row: `title1.image(content.uiIcon).size(iconXLarge)` + `[accent]` name.
fn content_show_header(table: &mut Gd<MindTable>, info: &VarDictionary, content_name: &str) {
    // code-instantiated: the icon/name header is parameterized by the content.
    let mut header = HBoxContainer::new_alloc();
    header.add_theme_constant_override("separation", 5);
    let icon_region = dict_text_or(info, "icon", content_name);
    let mut icon = factory::image(&icon_region);
    icon.set_custom_minimum_size(Vector2::new(INFO_HEADER_ICON, INFO_HEADER_ICON));
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_mouse_filter(MouseFilter::IGNORE);
    header.add_child(&icon);
    let localized = dict_text_or(info, "localized", content_name);
    // `title1.add("[accent]" + content.localizedName).padLeft(5)`.
    let name_label = factory::label(&kit::markup_text(&format!("[accent]{localized}")).to_string());
    {
        let mut rich_name: Gd<RichTextLabel> = name_label.clone().upcast();
        rich_name.set_autowrap_mode(AutowrapMode::OFF);
        rich_name.set_fit_content(true);
    }
    {
        let mut control: Gd<Control> = name_label.clone().upcast();
        control.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    }
    header.add_child(&name_label);
    // `table.table(title1 -> ...)` has no alignment: the header sits centred.
    table.bind_mut().add(&header).pad(4.0);
    table.bind_mut().row();
}

/// A default-font label tinted `Pal.accent` (`table.add(text).color(Pal.accent)`).
fn content_accent_label(text: &str) -> Gd<MindLabel> {
    let label = factory::label(text);
    {
        let mut control: Gd<Control> = label.clone().upcast();
        control.set_modulate(styles::ACCENT);
    }
    label
}

/// One accent category label, then the inset stat rows (`useCategories`).
fn content_render_categories(
    dialog: &Gd<MindDialog>,
    table: &mut Gd<MindTable>,
    categories: &VarArray,
    use_categories: bool,
) {
    for category_value in categories.iter_shared() {
        let Ok(category) = category_value.try_to::<VarDictionary>() else {
            continue;
        };
        let stats = dict_array(&category, "stats");
        if stats.is_empty() {
            continue;
        }
        if use_categories {
            // `table.add(cat.localized()).color(Pal.accent).fillX()`.
            let accent = content_accent_label(&dict_text(&category, "label"));
            table.bind_mut().add(&accent).grow_x().left();
            table.bind_mut().row();
        }
        for stat in stats.iter_shared() {
            content_render_stat(dialog, table, &stat);
        }
    }
}

/// Legacy flat `[{category,label,value}]` rows (bundle fallback paths).
fn content_render_stats(_dialog: &Gd<MindDialog>, table: &mut Gd<MindTable>, stats: &VarArray) {
    let mut current_category = String::new();
    for stat_value in stats.iter_shared() {
        let Ok(stat) = stat_value.try_to::<VarDictionary>() else {
            continue;
        };
        let category = dict_text(&stat, "category");
        if !category.is_empty() && category != current_category {
            current_category = category.clone();
            let accent = content_accent_label(&content_stat_text(&category));
            table.bind_mut().add(&accent).grow_x().left();
            table.bind_mut().row();
        }
        // code-instantiated: one label/value row per computed stat.
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 8);
        let label = factory::label(
            &kit::markup_text(&format!(
                "[lightgray]{}:[] ",
                content_stat_text(&dict_text(&stat, "label"))
            ))
            .to_string(),
        );
        row.add_child(&label);
        let value = factory::label(&dict_text(&stat, "value"));
        row.add_child(&value);
        table.bind_mut().add(&row).grow_x().pad(1.0).left();
        table.bind_mut().row();
    }
}

/// `table.table(inset -> { inset.left(); stats.statInfo(...); ... })`.
fn content_render_stat(dialog: &Gd<MindDialog>, table: &mut Gd<MindTable>, stat_value: &Variant) {
    let Ok(stat) = stat_value.try_to::<VarDictionary>() else {
        return;
    };
    // code-instantiated: one stat row per computed stat (data-driven).
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 2);
    // Arc cells have no spacing: the label carries its own trailing space and
    // values are split by the explicit 10 px cells.
    let mut line = HBoxContainer::new_alloc();
    line.add_theme_constant_override("separation", 0);
    column.add_child(&line);
    let label = factory::label(
        &kit::markup_text(&format!("[lightgray]{}:[] ", dict_text(&stat, "label"))).to_string(),
    );
    {
        let mut rich_label: Gd<RichTextLabel> = label.clone().upcast();
        rich_label.set_vertical_alignment(VerticalAlignment::TOP);
    }
    if let Some(tooltip_value) = stat.get("tooltip").filter(|value| !value.is_nil()) {
        let provider = kit::t(&format!("@{}", variant_text(&tooltip_value))).to_variant();
        let mut control: Gd<Control> = label.clone().upcast();
        tooltip::attach(&mut control, &provider, false);
    }
    line.add_child(&label);
    let values = dict_array(&stat, "values");
    for (index, value_row) in values.iter_shared().enumerate() {
        if index > 0 {
            line.add_child(&content_spacer(10.0));
        }
        let Ok(cells) = value_row.try_to::<VarArray>() else {
            continue;
        };
        for cell in cells.iter_shared() {
            line = content_add_cell(dialog, &mut column, &line, &cell);
        }
    }
    table
        .bind_mut()
        .add(&column)
        .grow_x()
        .pad(1.0)
        .pad_left(10.0)
        .left();
    table.bind_mut().row();
}

/// Adds one `Cell` and returns the line container the next cell belongs to
/// (`row` cells start a new line).
fn content_add_cell(
    dialog: &Gd<MindDialog>,
    column: &mut Gd<VBoxContainer>,
    line: &Gd<HBoxContainer>,
    cell_value: &Variant,
) -> Gd<HBoxContainer> {
    let Ok(cell) = cell_value.try_to::<VarDictionary>() else {
        return line.clone();
    };
    match dict_text(&cell, "k").as_str() {
        "text" => {
            let label = factory::label(&kit::markup_text(&dict_text(&cell, "t")).to_string());
            let mut line = line.clone();
            line.add_child(&label);
        }
        "space" => {
            let mut line = line.clone();
            line.add_child(&content_spacer(dict_float(&cell, "w", 0.0)));
        }
        "fill" => {
            // `Cell::Fill`: Java `.right().grow()` pushes the rest right.
            let mut fill = content_spacer(0.0);
            fill.set_h_size_flags(SizeFlags::EXPAND_FILL);
            let mut line = line.clone();
            line.add_child(&fill);
        }
        "row" => {
            // code-instantiated: multi-line stat values split into extra lines.
            let mut next = HBoxContainer::new_alloc();
            next.add_theme_constant_override("separation", 0);
            column.add_child(&next);
            return next;
        }
        "icon" => {
            let icon = content_icon_cell(dialog, &cell);
            let mut line = line.clone();
            line.add_child(&icon);
        }
        "panel" => {
            let panel = content_panel_cell(dialog, &cell);
            let mut line = line.clone();
            line.add_child(&panel);
        }
        _ => {}
    }
    line.clone()
}

/// `StatValues.stack`: a 32 px content icon with an outline amount and a click
/// through to the item/liquid info dialog. Unit icons (`power`, `liquid`,
/// `waves`) have no atlas region and draw the icon-font glyph instead.
fn content_icon_cell(dialog: &Gd<MindDialog>, cell: &VarDictionary) -> Gd<Control> {
    let icon_size = dict_float(cell, "size", 32.0);
    let region = dict_text(cell, "region");
    if kit::icon_texture(&region).is_none() {
        // code-instantiated: Iconc unit glyphs are font glyphs, not atlas regions.
        let mut holder = Control::new_alloc();
        holder.set_custom_minimum_size(Vector2::new(icon_size, icon_size));
        holder.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        let mut glyph = factory::glyph_label(&region, icon_size as i32, Color::WHITE);
        glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        holder.add_child(&glyph);
        return holder;
    }
    // code-instantiated: content icons are data-driven stat cells.
    let mut button = factory::image_button(&region, "cleari");
    button.set_custom_minimum_size(Vector2::new(icon_size, icon_size));
    button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    let content = dict_text(cell, "content");
    if !content.is_empty() {
        let target = dialog.clone();
        let callable = Callable::from_fn("content_stat_icon", move |_args: &[&Variant]| {
            open_content_info(&target, &content);
        });
        button.connect("pressed", &callable);
    }
    // `Cell::Icon.amount` serializes `None` as JSON null; `str(null)` is the
    // literal "<null>", so read the value and skip null explicitly.
    let amount = match cell.get("amount") {
        Some(value) if !value.is_nil() => variant_text(&value),
        _ => String::new(),
    };
    if !amount.is_empty() {
        let mut label = factory::styled_label(&amount, "outlineLabel");
        label.set_anchors_preset(LayoutPreset::BOTTOM_LEFT);
        label.set_offset(Side::TOP, -20.0);
        label.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&label);
    }
    let control: Gd<Control> = button.upcast();
    control
}

/// `Cell::Panel`: nested cells inside a flat gray box (reconstructor upgrades).
fn content_panel_cell(dialog: &Gd<MindDialog>, cell: &VarDictionary) -> Gd<Control> {
    // code-instantiated: panel cells are data-driven (upgrade pairs).
    let mut panel = PanelContainer::new_alloc();
    panel.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    panel.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut box_ = StyleBoxFlat::new_gd();
    // `Styles.grayPanel = whiteui.tint(Pal.darkestGray)`.
    box_.set_bg_color(Color::from_rgba(0.1, 0.1, 0.1, 1.0));
    box_.set_content_margin(Side::LEFT, 10.0);
    box_.set_content_margin(Side::RIGHT, 10.0);
    box_.set_content_margin(Side::TOP, 10.0);
    box_.set_content_margin(Side::BOTTOM, 10.0);
    panel.add_theme_stylebox_override("panel", &box_);
    // Nested cells may start their own rows (`Cell::Row`): the panel is a VBox
    // of HBox lines so a row cell lands below, not beside.
    let mut inner = VBoxContainer::new_alloc();
    inner.add_theme_constant_override("separation", 2);
    panel.add_child(&inner);
    let mut line = HBoxContainer::new_alloc();
    line.add_theme_constant_override("separation", 4);
    inner.add_child(&line);
    for nested in dict_array(cell, "cells").iter_shared() {
        line = content_add_cell(dialog, &mut inner, &line, &nested);
    }
    let control: Gd<Control> = panel.upcast();
    control
}

/// `inset.add().size(10f)`: a sizing-only cell gap.
fn content_spacer(width: f32) -> Gd<Control> {
    // code-instantiated: cell gaps are data-driven (Java `inset.add().size(10f)`).
    let mut spacer = Control::new_alloc();
    spacer.set_custom_minimum_size(Vector2::new(width, 0.0));
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    spacer
}

/// Resolves a stat/category label: bundle keys (`category.purpose`) resolve
/// through `MindAssets`, plain strings pass through.
fn content_stat_text(value: &str) -> String {
    if value.is_empty() {
        return value.to_string();
    }
    kit::t(&format!("@{value}")).to_string()
}

/// Bundle-only fallback: `<type>.<name>.name|description|details|credit` for
/// the first unlockable type that has the name.
fn content_bundle_info(content_name: &str) -> VarDictionary {
    let mut info = VarDictionary::new();
    info.set("content", &GString::from(content_name));
    info.set("localized", &GString::from(content_name));
    for type_name in INFO_CONTENT_TYPES {
        let name_key = format!("{type_name}.{content_name}.name");
        let localized = kit::t(&name_key).to_string();
        if localized.is_empty() || localized == name_key {
            continue;
        }
        info.set("localized", &GString::from(localized.as_str()));
        info.set("type", &GString::from(type_name));
        for field in ["description", "details", "credit"] {
            let value = content_bundle_value(&format!("{type_name}.{content_name}.{field}"));
            info.set(field, &GString::from(value.as_str()));
        }
        return info;
    }
    info
}

/// `_bundle_value`: the bundle text, empty when the key is absent.
fn content_bundle_value(key: &str) -> String {
    bundle_or(key, "")
}

// @@PART7@@

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effects_preview_resolves_logic_fx_to_its_fx_field() {
        // `LogicFx.EffectEntry.effect`: the preview spawns the Fx by field name.
        assert_eq!(effects_fx_name("breakBlock"), Some("breakBlock"));
        assert_eq!(effects_fx_name("explosion"), Some("dynamicExplosion"));
        assert_eq!(effects_fx_name("bubble"), Some("airBubble"));
        assert_eq!(effects_fx_name("not-an-effect"), None);
    }

    #[test]
    fn effects_preview_scale_fits_the_cell() {
        // `width / calculateSize`; no cached bounds falls back to the cell size.
        assert_eq!(effects_preview_scale(EFFECTS_CELL_SIZE), 1.0);
        assert_eq!(effects_preview_scale(-1.0), 1.0);
        assert_eq!(effects_preview_scale(100.0), EFFECTS_PREVIEW_MAX_SCALE);
        assert_eq!(effects_preview_scale(10_000.0), EFFECTS_PREVIEW_MIN_SCALE);
    }
}
