// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The settings menu dialog (`settings`): the category rail, the settings tables and the data/planet action pages.
//! Upstream: core/src/mindustry/ui/dialogs/SettingsMenuDialog.java

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::display_server::FileDialogMode;
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, Control, DisplayServer, HBoxContainer, Label, MarginContainer, PanelContainer,
    PopupMenu, StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use crate::gdutil::variant::{dict_bool_try_or, to_f64_boolish, to_i64_no_text};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit;
use crate::ui::kit::widgets::check::MindCheck;
use crate::ui::kit::widgets::factory;

use super::util::*;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "settings" => Some(Box::new(SettingsController)),
        _ => None,
    }
}

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
///
/// `PAR-0435` recorded a 9 px-too-high Graphics list in the GDScript-era frame
/// (`runs/20261017-L-twin/java/36-graphics.png`: 58 vs 67); the native shell's
/// title/body stack since lands the first row at Java's y=67, pinned by the
/// `dialog_sweep_check` assertion. Do not add to this inset.
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
        let request = MindDialog::take_request(dialog);
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
        MindDialog::request_refresh(&target, SETTINGS_REQUEST_BACK);
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
        MindDialog::request_refresh_deferred(&target, SETTINGS_REQUEST_CONFIRM);
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
    let mut rows = factory::vbox(0);
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
    let mut table = factory::vbox(0);
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
            let checked = dict_bool_try_or(row, "value", false);
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
        let number = to_f64_boolish(args.first().map(|entry| (*entry).clone())) as i64;
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
        MindDialog::request_refresh(&target, SETTINGS_REQUEST_RENDER);
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
            value = to_i64_no_text(Some(value)).to_variant();
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
    let mut rows = factory::vbox(0);
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
    let mut rows = factory::vbox(0);
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
        let index = to_i64_no_text(args.first().map(|entry| (*entry).clone()));
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
    let mut row = factory::hbox(0);
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, margin_left);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// PAR-0435: the native shell already lands the first Settings Graphics
    /// row on Java's y=67 (`runs/20261017-L-twin/java/36-graphics.png`); the
    /// `dialog_sweep_check` assertion pins it. The pane inset must stay 12 px
    /// on top of Java's `prefs.margin(14f)` — the old GDScript frame's 58 px
    /// came from the pre-cutover shell.
    #[test]
    fn settings_pane_inset_stays_java_pane_margin() {
        assert_eq!(PANE_INSET_TOP, 12.0);
        assert_eq!(PANE_INSET_BOTTOM, 3.0);
        assert_eq!(PREFS_MARGIN, 14);
    }
}
