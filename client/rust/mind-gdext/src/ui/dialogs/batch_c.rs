// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/PlanetDialog.java (plan 14 M5).
//
//! Campaign entry: the planet dialog controller.
//!
//! The controller binds the data-driven part of the planet page to the shared
//! [`MindDialog`] shell: the first-run campaign chooser, the planet/difficulty
//! rail, the globe input (drag / wheel / tap), the collapsible sector list and
//! the selected-sector readout with its in-panel action button. Static
//! structure lives in the dialog scene; every row here is rebuilt from the
//! `MindCampaign` read models (`campaign_views_json`, `launch_dialog_json`)
//! and the `MindPreview` map registry, so the controller stays UI-only.

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::{Side, VarArray, VarDictionary, VariantType};
use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, ButtonGroup, CanvasItem, ColorRect, Control, HBoxContainer, Json, Label, LineEdit,
    Node, Panel, PanelContainer, RichTextLabel, ScrollContainer, StyleBoxEmpty, StyleBoxFlat,
    Texture2D, TextureRect, VBoxContainer,
};
use godot::obj::{EngineBitfield, NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindTable, styles};
use crate::editor::MindPreview;

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "planet" => Some(Box::new(PlanetDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
//
// Row readers and settings accessors shared by the campaign dialogs of this
// batch. The planet page does not call every one of them yet, so the unused
// ones carry `allow(dead_code)` instead of being dropped from the port.
// ---------------------------------------------------------------------------

/// `Pal.gray` / `Tex.pane` outline (#454545) used by the pane buttons.
const OUTLINE_GRAY: Color = Color::from_rgba(0.27, 0.27, 0.27, 1.0);
/// `Pal.lightGray` sampled from the reference frames (sector stats cells).
#[allow(dead_code)]
const LIGHT_GRAY: Color = Color::from_rgba(0.62, 0.62, 0.62, 1.0);
/// `Color(0.75, 0.75, 0.75)` (research requirement/objective text).
const TEXT_GRAY: Color = Color::from_rgba(0.75, 0.75, 0.75, 1.0);
/// `Pal.remove` (`Color(1.0, 0.204, 0.11)`), the requirement-red.
const PAL_REMOVE: Color = Color::from_rgba(1.0, 0.204, 0.11, 1.0);
/// The live planet view (`ui/planet.rs`).
const PLANET_VIEW_PATH: &str = "/root/Spine/Planet/PlanetView";
/// The live map registry (`editor/preview.rs`).
const PREVIEW_PATH: &str = "/root/Spine/MindPreview";
/// Stage layers hidden while the planet page covers the screen.
const MENU_GROUP_PATH: &str = "/root/Spine/Ui/UiRoot/MenuGroup";
const MENU_BACKGROUND_PATH: &str = "/root/Spine/Ui/MenuBackground";
const HUD_GROUP_PATH: &str = "/root/Spine/Ui/UiRoot/HudGroup";
const WORLD_PATH: &str = "/root/Spine/World";

/// A rich label's engine handle (text/color/alignment live on `RichTextLabel`).
fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

/// Reads a JSON string field (`""` when missing or null).
fn row_str(row: &VarDictionary, key: &str) -> String {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// Reads a JSON string field (`None` when missing or null).
#[allow(dead_code)]
fn row_opt_str(row: &VarDictionary, key: &str) -> Option<String> {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
}

/// Reads a JSON boolean field (`false` when missing).
fn row_bool(row: &VarDictionary, key: &str) -> bool {
    row_bool_or(row, key, false)
}

/// Reads a JSON boolean field with a default.
fn row_bool_or(row: &VarDictionary, key: &str, default: bool) -> bool {
    row.get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(default)
}

/// Reads a JSON integer field (`0` when missing; floats truncate).
fn row_i64(row: &VarDictionary, key: &str) -> i64 {
    row_i64_or(row, key, 0)
}

/// Reads a JSON integer field with a default.
fn row_i64_or(row: &VarDictionary, key: &str, default: i64) -> i64 {
    let Some(value) = row
        .get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
    else {
        return default;
    };
    if let Ok(integer) = value.try_to::<i64>() {
        return integer;
    }
    value
        .try_to::<f64>()
        .map(|value| value as i64)
        .unwrap_or(default)
}

/// Reads a JSON float field (`0.0` when missing).
#[allow(dead_code)]
fn row_f64(row: &VarDictionary, key: &str) -> f64 {
    let Some(value) = row
        .get(key)
        .filter(|value| value.get_type() != VariantType::NIL)
    else {
        return 0.0;
    };
    if let Ok(number) = value.try_to::<f64>() {
        return number;
    }
    value
        .try_to::<i64>()
        .map(|value| value as f64)
        .unwrap_or(0.0)
}

/// Reads a JSON array field (empty when missing).
fn row_arr(row: &VarDictionary, key: &str) -> VarArray {
    row.get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

/// Reads a JSON object field (empty when missing).
fn row_dict(row: &VarDictionary, key: &str) -> VarDictionary {
    row.get(key)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// Reads the row dictionaries of a JSON array field.
fn row_rows(row: &VarDictionary, key: &str) -> Vec<VarDictionary> {
    row_arr(row, key)
        .iter_shared()
        .filter_map(|value| value.try_to::<VarDictionary>().ok())
        .collect()
}

/// Parses a JSON object (`{}` on any other JSON shape).
fn parse_object(text: &str) -> VarDictionary {
    if text.is_empty() {
        return VarDictionary::new();
    }
    Json::parse_string(&GString::from(text))
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// Calls a `MindUi` method when both the autoload and the method exist.
fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// Calls a `MindUi` method returning a bool (`false` when absent).
fn ui_bool(method: &str, args: &[Variant]) -> bool {
    ui_call(method, args)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(false)
}

/// `Core.settings.getBool(key)` through the `MindUi` store.
fn setting_flag(key: &str) -> bool {
    ui_bool("settings_get_flag", &[GString::from(key).to_variant()])
}

/// `Core.settings.getString(key, default)` through the `MindUi` store.
fn setting_text(key: &str, default: &str) -> String {
    ui_call(
        "settings_get_text",
        &[
            GString::from(key).to_variant(),
            GString::from(default).to_variant(),
        ],
    )
    .and_then(|value| value.try_to::<GString>().ok())
    .map(|value| value.to_string())
    .unwrap_or_else(|| default.to_string())
}

/// `Core.settings.getInt(key, default)` through the `MindUi` store.
#[allow(dead_code)]
fn setting_get_int(key: &str, default: i64) -> i64 {
    ui_call(
        "settings_get_int",
        &[GString::from(key).to_variant(), default.to_variant()],
    )
    .and_then(|value| value.try_to::<i64>().ok())
    .unwrap_or(default)
}

/// `Core.settings.put(key, value)` (bool) through the `MindUi` store.
fn setting_put_flag(key: &str, value: bool) {
    let _ = ui_call(
        "settings_put_flag",
        &[GString::from(key).to_variant(), value.to_variant()],
    );
}

/// `Core.settings.put(key, value)` (string) through the `MindUi` store.
fn setting_put_text(key: &str, value: &str) {
    let _ = ui_call(
        "settings_put_text",
        &[
            GString::from(key).to_variant(),
            GString::from(value).to_variant(),
        ],
    );
}

/// The live map registry node (`None` outside the game scene).
#[allow(dead_code)]
fn preview_node() -> Option<Gd<MindPreview>> {
    kit::node(PREVIEW_PATH)?.try_cast::<MindPreview>().ok()
}

/// The `UiRoot` shell (`/root/Spine/Ui/UiRoot`).
fn ui_root() -> Option<Gd<Node>> {
    kit::node("/root/Spine/Ui/UiRoot")
}

/// One manifest dialog node through `UiRoot.dialog(name)`.
fn dialog_node(name: &str) -> Option<Gd<Node>> {
    let mut root = ui_root()?;
    if !root.has_method("dialog") {
        return None;
    }
    root.call("dialog", &[GString::from(name).to_variant()])
        .try_to::<Gd<Node>>()
        .ok()
}

/// Shows a planed pane style (`_pane_box`: black fill, 4 px border, 2 px radius).
fn pane_box(fill: Color, border: Color, width: i32, radius: i32) -> Gd<StyleBoxFlat> {
    MindDialog::pane_box(fill, border, width as f32, radius as f32, -1.0)
}

/// Forces a themed toggle's visual state without running its listeners.
fn set_toggle(button: &Gd<Button>, pressed: bool) {
    let mut button = button.clone();
    button.set_pressed_no_signal(pressed);
}

/// A sizing-only spacer cell (`Control()` with an expand flag).
fn spacer(expand: bool, width: f32, height: f32) -> Gd<Control> {
    // code-instantiated: layout-only spacer, no static scene owns it.
    let mut control = Control::new_alloc();
    control.set_custom_minimum_size(Vector2::new(width, height));
    if expand {
        control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    }
    control.set_mouse_filter(MouseFilter::IGNORE);
    control
}

/// An icon-font glyph control sized `extent` (`MindWidgets.glyph`).
fn glyph_control(name: &str, extent: f32) -> Gd<Label> {
    factory::glyph_label(name, extent as i32, Color::WHITE)
}

/// `UnlockableContent.uiIcon` region chain (`item-<name>-ui` first).
fn content_texture(content_name: &str) -> Option<Gd<Texture2D>> {
    for prefix in ["item", "liquid", "block", "unit", "status"] {
        if let Some(texture) = kit::icon_texture(&format!("{prefix}-{content_name}-ui")) {
            return Some(texture.upcast());
        }
    }
    None
}

/// A fitted `TextureRect` for an atlas region (`res.image(uiIcon).size(n)`).
fn icon_rect(texture: &Gd<Texture2D>, extent: f32) -> Gd<TextureRect> {
    // code-instantiated: one content icon per data row.
    let mut image = TextureRect::new_alloc();
    image.set_texture(texture);
    image.set_custom_minimum_size(Vector2::new(extent, extent));
    image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    image.set_mouse_filter(MouseFilter::IGNORE);
    image
}

/// A `Picture`/`BorderImage` preview: fitted texture with a `thickness` border.
#[allow(dead_code)]
fn border_image(texture: &Gd<Texture2D>, extent: f32, thickness: f32) -> Gd<PanelContainer> {
    // code-instantiated: map preview textures come from the map registry.
    let mut image = TextureRect::new_alloc();
    image.set_texture(texture);
    image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    image.set_custom_minimum_size(Vector2::new(extent, extent));
    image.set_texture_filter(godot::classes::canvas_item::TextureFilter::NEAREST);
    image.set_mouse_filter(MouseFilter::IGNORE);
    let mut frame = panel_with_box(pane_box(
        Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        OUTLINE_GRAY,
        thickness as i32,
        0,
    ));
    frame.add_child(&image);
    frame
}

/// A `PanelContainer` carrying a stylebox override.
fn panel_with_box(style: Gd<StyleBoxFlat>) -> Gd<PanelContainer> {
    // code-instantiated: themed frame built per data row.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &style);
    panel
}

/// `Strings.stripColors` (`[#rrggbb]`/`[name]` tags removed) lowercased.
#[allow(dead_code)]
fn strip_color_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0usize;
    while index < bytes.len() {
        let character = bytes[index];
        if character != '[' {
            out.push(character);
            index += 1;
            continue;
        }
        let Some(close) = bytes[index..].iter().position(|value| *value == ']') else {
            out.push(character);
            index += 1;
            continue;
        };
        let inner: String = bytes[index + 1..index + close].iter().collect();
        let color_tag = inner
            .strip_prefix('#')
            .map(|hex| {
                (3..=8).contains(&hex.len()) && hex.chars().all(|value| value.is_ascii_hexdigit())
            })
            .unwrap_or(false);
        let name_tag = inner.chars().all(|value| value.is_ascii_lowercase());
        if color_tag || name_tag {
            index += close + 1;
            continue;
        }
        out.push(character);
        index += 1;
    }
    out.to_lowercase()
}

/// Connects `callable` to `signal` once per dialog (`is_connected` cannot match
/// a `Callable::from_fn`, so the dialog node carries the latch).
fn connect_once(
    dialog: &mut MindDialog,
    key: &str,
    target: &Gd<Node>,
    signal: &str,
    callable: &Callable,
) {
    let meta = format!("_batch_c_{key}");
    if dialog.base().has_meta(&meta) {
        return;
    }
    if !target.has_signal(signal) {
        return;
    }
    let mut target = target.clone();
    let _ = target.connect(signal, callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// Runs `action` on the next frame (the deferred seams).
///
/// The queue is drained from a `SceneTree.process_frame` connection installed
/// by [`install_frame_tick`]; `MindDialog` owns no `_process` of its own.
// A boxed one-shot closure; naming the alias would not make the type simpler.
#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
struct Deferred(Rc<RefCell<Vec<Box<dyn FnMut()>>>>);

impl Deferred {
    fn push(&self, action: impl FnMut() + 'static) {
        self.0.borrow_mut().push(Box::new(action));
    }

    fn drain(&self) {
        let queued: Vec<Box<dyn FnMut()>> = self.0.borrow_mut().drain(..).collect();
        for mut action in queued {
            action();
        }
    }
}

/// Installs the per-frame hook once (deferred queue drain + `tick`).
fn install_frame_tick(
    dialog: &mut MindDialog,
    key: &str,
    deferred: Deferred,
    mut tick: impl FnMut() + 'static,
) {
    let mut tree = dialog.base().get_tree();
    let meta = format!("_batch_c_frame_{key}");
    if dialog.base().has_meta(&meta) {
        return;
    }
    // The connection outlives a dialog freed while the tree is alive (editor
    // dialogs, teardown), so it must never touch the node again: the guard runs
    // before the queued actions and the tick, and drops the connection.
    let dialog_gd = dialog.to_gd();
    let tree_for = tree.clone();
    let callable_slot: Rc<RefCell<Option<Callable>>> = Rc::new(RefCell::new(None));
    let slot_for = callable_slot.clone();
    let callable = Callable::from_fn("batch_c_frame", move |_args: &[&Variant]| {
        if !dialog_gd.is_instance_valid() {
            if let Some(entry) = slot_for.borrow_mut().take() {
                let mut tree = tree_for.clone();
                if tree.is_connected("process_frame", &entry) {
                    tree.disconnect("process_frame", &entry);
                }
            }
            return Variant::nil();
        }
        deferred.drain();
        tick();
        Variant::nil()
    });
    *callable_slot.borrow_mut() = Some(callable.clone());
    let _ = tree.connect("process_frame", &callable);
    dialog.base_mut().set_meta(&meta, &true.to_variant());
}

/// `_localized_item`/`_localized_planet` fallback rule: bundle value when the
/// key resolves, else the caller's fallback.
fn bundle_or(key: &str, fallback: &str) -> String {
    let value = kit::t(&format!("@{key}")).to_string();
    if value.is_empty() || value == key {
        fallback.to_string()
    } else {
        value
    }
}

// ---------------------------------------------------------------------------
// PlanetDialog
// ---------------------------------------------------------------------------

/// `PlanetDialog` chooser card size (`CARD_SIZE`).
const CARD_SIZE: Vector2 = Vector2::new(320.0, 320.0);
/// Chooser disc fallback diameter (`DISC_SIZE`).
const DISC_SIZE: f32 = 170.0;
/// Java card outline (`Styles.togglet`).
const CARD_OUTLINE: Color = Color::from_rgba(0.27, 0.27, 0.27, 1.0);
/// `PlanetDialog` footer buttons (`(200, 54)`).
const FOOTER_BUTTON: Vector2 = Vector2::new(200.0, 54.0);

/// Controller state for the reused planet dialog node.
#[derive(Default)]
struct PlanetState {
    root: Option<Gd<MindTable>>,
    selected_planet: String,
    selected_sector: i64,
    ok_button: Option<Gd<Button>>,
    last_planet_written: String,
    chooser_active: bool,
    footer_entries: Vec<Gd<Button>>,
    complete_announced: String,
    sectors_shown: bool,
    sector_search: String,
    sector_list: Option<Gd<VBoxContainer>>,
    sector_list_backdrop: Option<Gd<PanelContainer>>,
    /// node path -> visibility captured when the page first covered the stage.
    saved_layers: Vec<(String, bool)>,
    dragged: bool,
    pressed_position: Vector2,
    press_on_globe: bool,
    rename_target: i64,
    icon_target: i64,
    last_sector_id: i64,
    new_presets: Vec<i64>,
    preset_show: f32,
    reveal_showed: bool,
    launch_mode: String,
    accelerator_x: i64,
    accelerator_y: i64,
    /// Last `Time.get_ticks_msec()` the reveal loop ran (frame delta source).
    last_tick_ms: u64,
    /// `(planet, campaign_views_json)` cache of the dialog's campaign reads.
    views: VarDictionary,
    views_planet: String,
    /// Pending `call_deferred` work drained on the next frame.
    deferred: Deferred,
}

impl PlanetState {
    fn new() -> Self {
        Self {
            rename_target: -1,
            icon_target: -1,
            last_sector_id: -1,
            launch_mode: String::from("look"),
            accelerator_x: -1,
            accelerator_y: -1,
            ..Self::default()
        }
    }
}

/// Campaign entry: the first-run chooser and the planet/sector page.
pub struct PlanetDialog {
    state: Rc<RefCell<PlanetState>>,
}

impl Default for PlanetDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(PlanetState::new())),
        }
    }
}

impl DialogController for PlanetDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        planet_ready(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        planet_shown(dialog, &self.state);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        planet_hidden(dialog, &self.state);
    }

    fn blocks_escape(&self) -> bool {
        self.state.borrow().chooser_active
    }
}

fn planet_ready(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    dialog.set_title_literal("");
    dialog.should_pause = true;
    dialog.enable_full_dialog(true);
    planet_apply_full_dialog_margins(dialog);
    planet_add_space_backdrop(dialog);
    st.root = Some(dialog.content_table());
    planet_show_select(dialog, state, &mut st);

    // `ui.showTextInput` result for the sector rename pencil
    // (`PlanetDialog.java:1200-1206`).
    let target = dialog.to_gd();
    let state_for_input = state.clone();
    let callable = Callable::from_fn("planet_text_input", move |args: &[&Variant]| {
        let text = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_on_text_input(&mut bound, &state_for_input, &text);
    });
    if let Some(ui) = kit::ui() {
        connect_once(dialog, "ui_text_input", &ui, "text_input_result", &callable);
    }

    // Globe drag/wheel/tap: the dialog root Control owns the empty-screen
    // `gui_input` once `Dim` stops swallowing it (the page's screen input).
    if let Some(dim) = dialog.base().get_node_or_null("Dim")
        && let Ok(mut dim) = dim.try_cast::<Control>()
    {
        dim.set_mouse_filter(MouseFilter::IGNORE);
    }
    // `PanelContainer` defaults to STOP, so the bubble from the empty page area
    // (the globe is behind `Center/Panel/Layout/Body`) died at `Panel` before it
    // reached the dialog root — real mouse motion/click never ran `planet_input`
    // (PAR-0018). The planet page's panel is a transparent full-screen layout
    // shell, so let the pointer pass through to the root; the rail, sector list
    // and footer buttons still stop their own events, and `planet_over_ui`
    // additionally rejects their rects.
    if let Some(mut panel) = dialog.panel() {
        panel.set_mouse_filter(MouseFilter::PASS);
    }
    let target = dialog.to_gd();
    let state_for_click = state.clone();
    let callable = Callable::from_fn("planet_gui_input", move |args: &[&Variant]| {
        let Some(event) = args
            .first()
            .map(|value| (*value).to::<Gd<godot::classes::InputEvent>>())
        else {
            return;
        };
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_input(&mut bound, &state_for_click, event);
    });
    dialog.base_mut().connect("gui_input", &callable);

    // The reveal loop and the deferred rebuild queue share one frame hook.
    let target = dialog.to_gd();
    let state_for_frame = state.clone();
    let deferred = st.deferred.clone();
    let tick = move || {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_process(&mut bound, &state_for_frame);
    };
    install_frame_tick(dialog, "planet", deferred, tick);
}

fn planet_shown(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    // `PlanetDialog.show`: no sector selected when the page opens.
    st.selected_sector = -1;
    st.launch_mode = dialog.context_string("mode").to_string();
    if st.launch_mode.is_empty() {
        st.launch_mode = String::from("look");
    }
    st.accelerator_x = dialog.context_int("x");
    st.accelerator_y = dialog.context_int("y");
    planet_adopt_current_sector(dialog, &mut st);

    let mut chooser_shown = false;
    if st.selected_planet.is_empty() {
        let based = planet_planets_with_base(dialog, &mut st);
        if !setting_flag("campaignselect") && based.is_empty() {
            planet_show_select(dialog, state, &mut st);
            chooser_shown = true;
        } else {
            let planet = planet_boot_planet(dialog, &mut st, &based);
            planet_show_planet(dialog, state, &mut st, &planet, false);
        }
    } else {
        let planet = st.selected_planet.clone();
        planet_show_planet(dialog, state, &mut st, &planet, false);
    }

    if !chooser_shown {
        planet_start_preset_reveal(dialog, &mut st);
    }
    if st.launch_mode == "planetLaunch"
        && let Some(mut view) = planet_view()
        && view.has_method("set")
    {
        view.set("zoom", &1.0f32.to_variant());
        view.set("zoom_target", &1.0f32.to_variant());
        view.set("ui_alpha", &0.0f32.to_variant());
    }
    if !chooser_shown {
        st.deferred.push({
            let target = dialog.to_gd();
            move || {
                let mut bound = target.clone();
                let mut bound = bound.bind_mut();
                planet_maybe_show_remap_notice(&mut bound);
            }
        });
    }
}

fn planet_hidden(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    st.launch_mode = String::from("look");
    planet_remove_sector_list(&mut st);
    planet_set_planet_view_active(dialog, &mut st, false);
    planet_restore_stage(&mut st);
    if let Some(mut view) = planet_view() {
        planet_view_call(&mut view, "set_hover", &[(-1i64).to_variant()]);
    }
}

// ---- campaign reads -------------------------------------------------------

/// `refresh_campaign_views(planet)`: the live `MindCampaign` projection.
fn planet_refresh_views(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet: &str,
) -> VarDictionary {
    let views = dialog.campaign_json("campaign_views_json", &[GString::from(planet).to_variant()]);
    if !views.is_empty() {
        st.views = views.clone();
        st.views_planet = planet.to_string();
    }
    st.views.clone()
}

/// Ensures the cached projection was read for `planet`.
fn planet_ensure_views(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet: &str,
) -> VarDictionary {
    if st.views_planet == planet && !st.views.is_empty() {
        return st.views.clone();
    }
    planet_refresh_views(dialog, st, planet)
}

/// The live projection for a planet without touching the dialog's cache.
fn planet_live_views(dialog: &mut MindDialog, planet: &str) -> VarDictionary {
    dialog.campaign_json("campaign_views_json", &[GString::from(planet).to_variant()])
}

fn planet_find_planet(dialog: &mut MindDialog, st: &mut PlanetState, name: &str) -> VarDictionary {
    let views = planet_refresh_views(dialog, st, "");
    if views.is_empty() {
        return VarDictionary::new();
    }
    for planet in row_rows(&views, "planets") {
        if row_str(&planet, "name") == name {
            return planet;
        }
    }
    VarDictionary::new()
}

/// `PlanetDialog` sectors for a planet: the active projection's grid, else the
/// synthesized start sector from the planet row (`Planet.getStartSector`).
fn planet_sectors(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet: &str,
) -> Vec<VarDictionary> {
    let views = planet_ensure_views(dialog, st, planet);
    if row_str(&views, "planet") == planet {
        return row_rows(&views, "sectors");
    }
    let planet_row = {
        let mut out = VarDictionary::new();
        for row in row_rows(&views, "planets") {
            if row_str(&row, "name") == planet {
                out = row;
                break;
            }
        }
        out
    };
    if planet_row.is_empty() {
        return Vec::new();
    }
    let mut row = VarDictionary::new();
    row.set("id", row_i64(&planet_row, "start_sector"));
    let localized = row_str(&planet_row, "localized");
    row.set(
        "name",
        &GString::from(if localized.is_empty() {
            planet
        } else {
            &localized
        }),
    );
    row.set("planet", &GString::from(planet));
    row.set("threat_band", &GString::from("low"));
    row.set("locked", false);
    row.set("has_base", false);
    vec![row]
}

fn planet_find_sector(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet: &str,
    sector_id: i64,
) -> VarDictionary {
    for sector in planet_sectors(dialog, st, planet) {
        if row_i64(&sector, "id") == sector_id {
            return sector;
        }
    }
    VarDictionary::new()
}

fn planet_find_planet_row(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    name: &str,
) -> VarDictionary {
    planet_find_planet(dialog, st, name)
}

/// Localized planet label (`planet.<name>.name`), falling back to the row.
fn planet_localized_planet(planet: &VarDictionary) -> String {
    let name = row_str(planet, "name");
    let localized = bundle_or(&format!("planet.{name}.name"), "");
    if localized.is_empty() {
        let fallback = row_str(planet, "localized");
        return if fallback.is_empty() { name } else { fallback };
    }
    localized
}

/// Localized sector label (`sector.<preset>.name`).
fn planet_localized_sector(sector: &VarDictionary) -> String {
    if row_bool(sector, "named") {
        return row_str(sector, "name");
    }
    let preset = row_str(sector, "preset");
    if !preset.is_empty() {
        let localized = bundle_or(&format!("sector.{preset}.name"), "");
        if !localized.is_empty() {
            return localized;
        }
    }
    row_str(sector, "name")
}

/// `requirement.<kind>` argument localization.
fn planet_localized_objective_arg(kind: &str, argument: &str) -> String {
    if argument.is_empty() {
        return String::from("???");
    }
    match kind {
        "capture" | "onsector" => {
            let key = format!("sector.{argument}.name");
            let value = kit::t(&format!("@{key}")).to_string();
            if value.is_empty() || value == key {
                argument.to_string()
            } else {
                value
            }
        }
        "onplanet" => {
            let key = format!("planet.{argument}.name");
            let value = kit::t(&format!("@{key}")).to_string();
            if value.is_empty() || value == key {
                argument.to_string()
            } else {
                value
            }
        }
        _ => {
            for type_name in ["block", "item", "unit", "liquid", "status"] {
                let key = format!("{type_name}.{argument}.name");
                let value = kit::t(&format!("@{key}")).to_string();
                if !value.is_empty() && value != key {
                    return value;
                }
            }
            argument.to_string()
        }
    }
}

/// Visible state lines in `PlanetDialog.updateSelected` order.
fn planet_sector_state_labels(sector: &VarDictionary) -> Vec<String> {
    let mut out = Vec::new();
    if row_bool(sector, "attacked") {
        out.push(String::from("@sectors.underattack"));
    } else if row_bool(sector, "vulnerable") {
        out.push(String::from("@sectors.vulnerable"));
    } else if !row_bool(sector, "has_base") && row_bool(sector, "has_enemy_base") {
        out.push(String::from("@sectors.enemybase"));
    }
    out
}

// ---- planet view binding --------------------------------------------------

/// The live planet view (`MindPlanetView`, `ui/planet.rs`).
fn planet_view() -> Option<Gd<Node>> {
    kit::node(PLANET_VIEW_PATH)
}

/// Calls a planet-view method when it exists.
fn planet_view_call(view: &mut Gd<Node>, method: &str, args: &[Variant]) -> Option<Variant> {
    if !view.has_method(method) {
        return None;
    }
    Some(view.call(method, args))
}

/// `PlanetView.view_tile(sector)`: the campaign row's remapped grid tile.
fn planet_view_tile(view: &mut Gd<Node>, sector: &VarDictionary) -> i64 {
    planet_view_call(view, "view_tile", &[sector.to_variant()])
        .and_then(|value| value.try_to::<i64>().ok())
        .unwrap_or(-1)
}

/// `PlanetView.pick(viewport_position)` (`-1` when no tile is under the cursor).
fn planet_view_pick(view: &mut Gd<Node>, point: Vector2) -> i64 {
    planet_view_call(view, "pick", &[point.to_variant()])
        .and_then(|value| value.try_to::<i64>().ok())
        .unwrap_or(-1)
}

/// `PlanetView.data` (the `planet_view` grid topology dictionary).
fn planet_view_data(view: &mut Gd<Node>) -> VarDictionary {
    view.get("data")
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `PlanetView.hover_tile`.
fn planet_view_hover(view: &mut Gd<Node>) -> i64 {
    view.get("hover_tile").try_to::<i64>().unwrap_or(-1)
}

// ---- chooser --------------------------------------------------------------

/// `_apply_full_dialog_margins`: `Styles.fullDialog` insets around the page.
fn planet_apply_full_dialog_margins(dialog: &mut MindDialog) {
    let Some(mut panel) = dialog.panel() else {
        return;
    };
    let mut box_ = StyleBoxEmpty::new_gd();
    box_.set_content_margin(Side::LEFT, 3.0);
    box_.set_content_margin(Side::RIGHT, 5.0);
    box_.set_content_margin(Side::TOP, 1.0);
    box_.set_content_margin(Side::BOTTOM, 6.0);
    panel.add_theme_stylebox_override("panel", &box_);
}

/// `_add_space_backdrop`: the full-screen `sprites/space.png` starfield.
fn planet_add_space_backdrop(dialog: &mut MindDialog) {
    if dialog.base().get_node_or_null("SpaceBackdrop").is_some() {
        return;
    }
    let Some(mut assets) = kit::assets() else {
        return;
    };
    let Some(texture) = assets.bind_mut().find_loose(GString::from("space")) else {
        return;
    };
    // code-instantiated: single backdrop image; no static scene owns it yet.
    let mut backdrop = TextureRect::new_alloc();
    backdrop.set_name("SpaceBackdrop");
    let texture: Gd<Texture2D> = texture.upcast();
    backdrop.set_texture(&texture);
    backdrop.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    backdrop.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_COVERED);
    backdrop.set_texture_filter(godot::classes::canvas_item::TextureFilter::LINEAR);
    backdrop.set_mouse_filter(MouseFilter::IGNORE);
    let control: Gd<Control> = backdrop.clone().upcast();
    dialog.base_mut().add_child(&control);
    dialog.base_mut().move_child(&control, 0);
}

/// Clears the dialog body (`_clear_root`).
fn planet_clear_root(st: &mut PlanetState) {
    if let Some(mut root) = st.root.clone() {
        root.bind_mut().clear();
    }
}

/// `_set_body_centered`: centre or top-align the body and rail.
fn planet_set_body_centered(dialog: &mut MindDialog, st: &mut PlanetState, centered: bool) {
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(if centered {
            godot::classes::scroll_container::ScrollMode::DISABLED
        } else {
            godot::classes::scroll_container::ScrollMode::AUTO
        });
    }
    if let Some(mut content) = dialog.content() {
        content.set_alignment(if centered {
            godot::classes::box_container::AlignmentMode::CENTER
        } else {
            godot::classes::box_container::AlignmentMode::BEGIN
        });
        content.set_v_size_flags(if centered {
            SizeFlags::EXPAND_FILL
        } else {
            SizeFlags::FILL
        });
    }
    if let Some(mut root) = st.root.clone() {
        let mut root = root.bind_mut();
        root.set_table_top(!centered);
        root.set_table_left(!centered);
    }
}

/// The two campaign starting choices in reference order (Serpulo, Erekir).
fn planet_campaign_planets(dialog: &mut MindDialog, st: &mut PlanetState) -> Vec<VarDictionary> {
    let views = planet_refresh_views(dialog, st, "");
    let planets = row_rows(&views, "planets");
    let mut out = Vec::new();
    for name in ["serpulo", "erekir"] {
        if let Some(row) = planets.iter().find(|row| row_str(row, "name") == name) {
            out.push(row.clone());
        }
    }
    out
}

/// `_show_select`: the first-run "Select Starting Campaign" chooser.
fn planet_show_select(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
) {
    planet_remove_sector_list(st);
    dialog.set_title_key("@campaign.select");
    planet_clear_root(st);
    planet_set_planet_view_active(dialog, st, false);
    planet_set_body_centered(dialog, st, true);
    st.chooser_active = true;
    planet_refresh_views(dialog, st, "");

    let Some(mut root) = st.root.clone() else {
        return;
    };
    let choices = planet_campaign_planets(dialog, st);

    // code-instantiated: the chooser row is data-driven (planet cards).
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    row.add_theme_constant_override("separation", 6);
    root.bind_mut().add(&row).grow_x().pad_top(4.0);
    root.bind_mut().row();

    if let Some(mut title) = dialog.title_node() {
        title.add_theme_font_size_override("font_size", 21);
    }

    // code-instantiated: the caption is a live label updated by card toggles.
    let caption = factory::label(&kit::tm("@campaign.none").to_string());
    {
        let mut label = rich(&caption);
        label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        label.add_theme_font_size_override("normal_font_size", 20);
        // `PlanetDialog.java:181-214`: the caption is a font-20 `Label` whose
        // line advance is the font's own line height — 20 px in the reference
        // frames (PAR-0010 glyphs 448..461 / 468..485; the selected blurb in
        // PAR-0011 keeps the same 20 px advance). `RichTextLabel` adds
        // `line_separation` on top of that height, so leave it at the theme
        // default: the former -8 override collapsed the 20 px line box to a
        // 12 px pitch and drew the two lines on top of each other.
        label.add_theme_constant_override("line_separation", 0);
    }
    {
        let mut control: Gd<Control> = caption.clone().upcast();
        control.set_custom_minimum_size(Vector2::new(440.0, 0.0));
    }

    let mut group = ButtonGroup::new_gd();
    group.set_allow_unpress(true);
    for planet in &choices {
        let planet_name = row_str(planet, "name");
        let mut card = planet_make_card(planet);
        card.set_button_group(&group);
        set_toggle(&card, planet_name == st.selected_planet);
        let state_for_toggle = state.clone();
        let caption_for_toggle = caption.clone();
        let name_for_toggle = planet_name.clone();
        let callable = Callable::from_fn("planet_card_toggled", move |args: &[&Variant]| {
            let pressed = args
                .first()
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(false);
            if !pressed {
                return;
            }
            let mut st = state_for_toggle.borrow_mut();
            st.selected_planet = name_for_toggle.clone();
            let mut label = rich(&caption_for_toggle);
            label.set_text(&kit::tm(&format!("@campaign.{}", st.selected_planet)));
            if let Some(mut ok) = st.ok_button.clone() {
                ok.set_disabled(false);
            }
        });
        card.connect("toggled", &callable);
        row.add_child(&card);
    }

    // `PlanetDialog.java:181-214`: the caption sits 8 px below the card row
    // (Java glyph top 448 under cards ending at 440). The table cell already
    // contributes 3 px, so a 4 px pad pins the vertically-centred block at the
    // reference card row: y=120 with the two-line chooser caption. (The
    // selected-state blurb is PAR-0011's surface.)
    root.bind_mut()
        .add(&caption)
        .grow_x()
        .max_width(440.0)
        .pad_top(4.0);
    root.bind_mut().row();
    // `PlanetDialog.java:205`: `.width(440f).wrap()`. `MindLabel::ready()` runs on
    // tree entry and `configure_rich_label` resets `autowrap_mode` to OFF, so the
    // wrap must be applied *after* `add()`: with wrap off the fit-content minimum
    // width is the unwrapped paragraph (788 px), `Control.size` clamps up to it and
    // the cell's `max_width(440)` is defeated (PAR-0011: cards y=110 instead of 90).
    // With WORD_SMART the label's minimum width is ~1 px, so the 440 px cell wins
    // and both locale paragraphs wrap to two lines (5 line boxes, 100 px tall).
    {
        let mut label = rich(&caption);
        label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
    }

    dialog.clear_buttons();
    planet_build_dimmed_footer(dialog, st);
    let target = dialog.to_gd();
    let state_for_ok = state.clone();
    let callable = Callable::from_fn("planet_select_ok", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_on_select_ok(&mut bound, &state_for_ok);
    });
    let ok_text = kit::t("@ok").to_string();
    let mut ok = dialog.add_button(&ok_text, Some(callable), "ok", 0.0);
    ok.set_custom_minimum_size(Vector2::new(300.0, 64.0));
    planet_apply_ok_style(&mut ok);
    planet_reflow_ok_row(&mut ok);
    ok.set_disabled(st.selected_planet.is_empty());
    st.ok_button = Some(ok);
}

/// The chooser's `@ok` states (`Styles.button` black fill, 4 px outline).
fn planet_apply_ok_style(button: &mut Gd<Button>) {
    for state in ["normal", "hover", "pressed"] {
        let mut box_ = StyleBoxFlat::new_gd();
        box_.set_bg_color(Color::BLACK);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            box_.set_border_width(side, 4);
        }
        box_.set_border_color(CARD_OUTLINE);
        for corner in [
            godot::builtin::Corner::TOP_LEFT,
            godot::builtin::Corner::TOP_RIGHT,
            godot::builtin::Corner::BOTTOM_LEFT,
            godot::builtin::Corner::BOTTOM_RIGHT,
        ] {
            box_.set_corner_radius(corner, 6);
        }
        box_.set_content_margin(Side::LEFT, 12.0);
        box_.set_content_margin(Side::RIGHT, 12.0);
        button.add_theme_stylebox_override(state, &box_);
    }
    let mut disabled = StyleBoxFlat::new_gd();
    disabled.set_bg_color(Color::from_rgba(0.145, 0.145, 0.145, 1.0));
    for corner in [
        godot::builtin::Corner::TOP_LEFT,
        godot::builtin::Corner::TOP_RIGHT,
        godot::builtin::Corner::BOTTOM_LEFT,
        godot::builtin::Corner::BOTTOM_RIGHT,
    ] {
        disabled.set_corner_radius(corner, 6);
    }
    disabled.set_content_margin(Side::LEFT, 12.0);
    disabled.set_content_margin(Side::RIGHT, 12.0);
    button.add_theme_stylebox_override("disabled", &disabled);
}

/// Java `Button` draws the icon at the left edge and the label centred; the
/// shared icon row centres both, so add the two balancing spacers.
fn planet_reflow_ok_row(button: &mut Gd<Button>) {
    if button.get_child_count() == 0 {
        return;
    }
    let Some(first) = button.get_child(0) else {
        return;
    };
    let Ok(mut row) = first.try_cast::<HBoxContainer>() else {
        return;
    };
    if row.get_child_count() != 2 {
        return;
    }
    if let Some(icon) = row.get_child(0)
        && let Ok(mut icon) = icon.try_cast::<Label>()
    {
        icon.add_theme_font_size_override("font_size", 36);
    }
    row.set_alignment(godot::classes::box_container::AlignmentMode::BEGIN);
    let left = spacer(true, 0.0, 0.0);
    let left_control: Gd<Control> = left.clone().upcast();
    row.add_child(&left_control);
    row.move_child(&left_control, 1);
    let right = spacer(true, 0.0, 0.0);
    let right_control: Gd<Control> = right.clone().upcast();
    row.add_child(&right_control);
}

/// `_build_dimmed_footer`: inert 10 %-brightness copies of the page footer.
fn planet_build_dimmed_footer(dialog: &mut MindDialog, st: &mut PlanetState) {
    planet_clear_dimmed_footer(st);
    st.footer_entries
        .push(planet_footer_entry(dialog, "@back", "left", true));
    st.footer_entries
        .push(planet_footer_entry(dialog, "@techtree", "tree", false));
}

fn planet_clear_dimmed_footer(st: &mut PlanetState) {
    for mut button in st.footer_entries.drain(..) {
        if button.is_instance_valid() {
            button.queue_free();
        }
    }
}

fn planet_footer_entry(
    dialog: &mut MindDialog,
    text_key: &str,
    icon: &str,
    left: bool,
) -> Gd<Button> {
    // code-instantiated: dimmed copies of the planet page's footer entries.
    let text = kit::t(text_key).to_string();
    let mut button = factory::icon_button(
        icon,
        &text,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    button.set_custom_minimum_size(FOOTER_BUTTON);
    button.set_anchors_preset(if left {
        LayoutPreset::BOTTOM_LEFT
    } else {
        LayoutPreset::BOTTOM_RIGHT
    });
    button.set_offset(Side::LEFT, if left { 5.0 } else { -205.0 });
    button.set_offset(Side::RIGHT, if left { 205.0 } else { -5.0 });
    button.set_offset(Side::TOP, -59.0);
    button.set_offset(Side::BOTTOM, -5.0);
    button.set_modulate(Color::from_rgba(0.1, 0.1, 0.1, 1.0));
    button.set_mouse_filter(MouseFilter::IGNORE);
    button.set_focus_mode(godot::classes::control::FocusMode::NONE);
    let control: Gd<Control> = button.clone().upcast();
    dialog.base_mut().add_child(&control);
    // Above the `Dim` overlay, below the centred panel content.
    dialog.base_mut().move_child(&control, 2);
    button
}

/// One `PlanetDialog` chooser card (`Styles.togglet` + planet art).
fn planet_make_card(planet: &VarDictionary) -> Gd<Button> {
    // code-instantiated: planet card layout is data-driven (no static scene).
    let mut card = factory::button("");
    card.set_toggle_mode(true);
    card.set_custom_minimum_size(CARD_SIZE);
    planet_apply_card_style(&mut card);
    let texture = planet_texture(planet);
    if let Some(texture) = texture.clone() {
        // code-instantiated: vendored `sprites/planets/<name>.png`.
        let mut image = TextureRect::new_alloc();
        image.set_texture(&texture);
        image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        image.set_offset(Side::LEFT, 5.0);
        image.set_offset(Side::TOP, 31.0);
        image.set_offset(Side::RIGHT, -5.0);
        image.set_offset(Side::BOTTOM, -12.0);
        image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        image.set_texture_filter(godot::classes::canvas_item::TextureFilter::LINEAR);
        image.set_mouse_filter(MouseFilter::IGNORE);
        let control: Gd<Control> = image.clone().upcast();
        card.add_child(&control);
    }
    // code-instantiated: card name column (label + optional disc).
    let mut column = VBoxContainer::new_alloc();
    column.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    column.set_offset(Side::LEFT, 5.0);
    column.set_offset(Side::TOP, 8.0);
    column.set_offset(Side::RIGHT, -5.0);
    column.set_offset(Side::BOTTOM, -8.0);
    column.set_alignment(godot::classes::box_container::AlignmentMode::BEGIN);
    column.set_mouse_filter(MouseFilter::IGNORE);
    card.add_child(&column);
    let name_label = factory::label(&planet_localized_planet(planet));
    {
        let mut label = rich(&name_label);
        label.add_theme_color_override("default_color", styles::ACCENT);
        label.add_theme_font_size_override("normal_font_size", 21);
        label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        label.set_mouse_filter(MouseFilter::IGNORE);
    }
    column.add_child(&name_label);
    if texture.is_none() {
        // code-instantiated: tinted disc fallback when the art is missing.
        let mut disc = Panel::new_alloc();
        disc.set_custom_minimum_size(Vector2::new(DISC_SIZE, DISC_SIZE));
        disc.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        disc.add_theme_stylebox_override("panel", &planet_disc_style(planet));
        disc.set_mouse_filter(MouseFilter::IGNORE);
        column.add_child(&disc);
    }
    card
}

fn planet_apply_card_style(card: &mut Gd<Button>) {
    card.add_theme_stylebox_override("normal", &planet_card_box(CARD_OUTLINE));
    card.add_theme_stylebox_override("hover", &planet_card_box(styles::ACCENT));
    card.add_theme_stylebox_override("pressed", &planet_card_box(Color::WHITE));
    card.add_theme_stylebox_override("hover_pressed", &planet_card_box(Color::WHITE));
    card.add_theme_stylebox_override("disabled", &planet_card_box(CARD_OUTLINE));
}

fn planet_card_box(border: Color) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(Color::BLACK);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        box_.set_border_width(side, 4);
    }
    box_.set_border_color(border);
    for corner in [
        godot::builtin::Corner::TOP_LEFT,
        godot::builtin::Corner::TOP_RIGHT,
        godot::builtin::Corner::BOTTOM_LEFT,
        godot::builtin::Corner::BOTTOM_RIGHT,
    ] {
        box_.set_corner_radius(corner, 4);
    }
    box_.set_content_margin(Side::LEFT, 5.0);
    box_.set_content_margin(Side::RIGHT, 5.0);
    box_.set_content_margin(Side::TOP, 6.0);
    box_.set_content_margin(Side::BOTTOM, 5.0);
    box_
}

/// Vendored `sprites/planets/<name>.png` (`PlanetDialog.planetTextures`).
fn planet_texture(planet: &VarDictionary) -> Option<Gd<Texture2D>> {
    let mut assets = kit::assets()?;
    let planet_name = row_str(planet, "name");
    let texture = assets
        .bind_mut()
        .find_loose(GString::from(format!("planets/{planet_name}").as_str()))?;
    Some(texture.upcast())
}

/// `_disc_style`: the planet's [`color`] disc with an accent ring.
fn planet_disc_style(planet: &VarDictionary) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    let color = row_str(planet, "color");
    box_.set_bg_color(Color::from_string(&color).unwrap_or(Color::WHITE));
    let radius = (DISC_SIZE * 0.5) as i32;
    for corner in [
        godot::builtin::Corner::TOP_LEFT,
        godot::builtin::Corner::TOP_RIGHT,
        godot::builtin::Corner::BOTTOM_LEFT,
        godot::builtin::Corner::BOTTOM_RIGHT,
    ] {
        box_.set_corner_radius(corner, radius);
    }
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        box_.set_border_width(side, 2);
    }
    box_.set_border_color(styles::ACCENT);
    box_
}

/// `_on_select_ok`: `state.planet = selected; selectSector(startSector)`.
fn planet_on_select_ok(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    if st.selected_planet.is_empty() {
        return;
    }
    setting_put_flag("campaignselect", true);
    let selected_planet = st.selected_planet.clone();
    let planet = planet_find_planet_row(dialog, &mut st, &selected_planet);
    if !planet.is_empty() {
        st.selected_sector = row_i64(&planet, "start_sector");
    }
    let selected = st.selected_planet.clone();
    planet_show_planet(dialog, state, &mut st, &selected, true);
    // `PlanetDialog.java:209-210`: after the starting campaign is chosen, Java
    // centres the camera on the planet's start sector (`lookAt(startSector)`)
    // before selecting it. The port dropped that call, so the fresh chooser
    // path kept the default polar camera and Ground Zero sat near the limb
    // instead of the reference framing (PAR-0018). `planet_show_planet` has
    // already stored the start tile in `selected_tile`.
    if let Some(mut view) = planet_view() {
        let tile = view.get("selected_tile").try_to::<i64>().unwrap_or(-1);
        if tile >= 0 {
            planet_view_call(&mut view, "look_at_tile", &[tile.to_variant()]);
        }
    }
}

/// `PlanetDialog.java:216-222`: the one-time Serpulo remap notice.
fn planet_maybe_show_remap_notice(dialog: &mut MindDialog) {
    let Some(mut campaign) = kit::campaign() else {
        return;
    };
    if !campaign.has_method("take_serpulo_remaps_notice") {
        return;
    }
    if !campaign
        .call("take_serpulo_remaps_notice", &[])
        .try_to::<bool>()
        .unwrap_or(false)
    {
        return;
    }
    let mut context = VarDictionary::new();
    context.set("title", &kit::t("@campaign.rework.title"));
    context.set("text", &kit::tm("@campaign.rework.serpulo"));
    context.set("ok", true);
    context.set("keep_previous", true);
    dialog.open_dialog("full_text", &json_stringify(&context));
}

/// `JSON.stringify` for the `open_dialog` context payloads.
fn json_stringify(value: &VarDictionary) -> String {
    godot::classes::Json::stringify(&value.to_variant()).to_string()
}

// ---- planet page ----------------------------------------------------------

/// `PlanetDialog.show`: adopt the running sector's planet.
fn planet_adopt_current_sector(dialog: &mut MindDialog, st: &mut PlanetState) {
    let Some(mut campaign) = dialog.campaign_node() else {
        return;
    };
    if !campaign.has_method("get_sector_state") {
        return;
    }
    let state = campaign.call("get_sector_state", &[]);
    let Ok(state) = state.try_to::<VarDictionary>() else {
        return;
    };
    if !row_bool(&state, "campaign") {
        return;
    }
    let planet = row_str(&state, "planet");
    if planet.is_empty() {
        return;
    }
    st.selected_planet = planet.clone();
    planet_refresh_views(dialog, st, &planet);
}

/// Names of the landable planets with at least one based sector.
fn planet_planets_with_base(dialog: &mut MindDialog, st: &mut PlanetState) -> Vec<String> {
    let views = planet_refresh_views(dialog, st, "");
    let mut out = Vec::new();
    for planet in row_rows(&views, "planets") {
        let name = row_str(&planet, "name");
        if name.is_empty() || !row_bool_or(&planet, "has_grid", true) {
            continue;
        }
        let live = planet_live_views(dialog, &name);
        for sector in row_rows(&live, "sectors") {
            if row_bool(&sector, "has_base") {
                out.push(name.clone());
                break;
            }
        }
    }
    out
}

/// `_boot_planet`: `lastplanet`, else the first based planet, else serpulo.
fn planet_boot_planet(dialog: &mut MindDialog, st: &mut PlanetState, based: &[String]) -> String {
    let last = setting_text("lastplanet", "serpulo");
    let planet = planet_find_planet(dialog, st, &last);
    if !planet.is_empty()
        && (based.iter().any(|name| name == &last) || row_bool(&planet, "accessible"))
    {
        return last;
    }
    if let Some(first) = based.first() {
        return first.clone();
    }
    String::from("serpulo")
}

/// `refresh_page`: re-runs the planet page for the selected planet.
fn planet_show_planet(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    planet_name: &str,
    select_default: bool,
) {
    if st.last_planet_written != planet_name {
        setting_put_text("lastplanet", planet_name);
        st.last_planet_written = planet_name.to_string();
    }
    st.selected_planet = planet_name.to_string();
    st.chooser_active = false;
    st.new_presets.clear();
    planet_clear_dimmed_footer(st);
    dialog.set_title_literal("");
    planet_clear_root(st);
    planet_set_planet_view_active(dialog, st, true);
    planet_set_body_centered(dialog, st, false);
    planet_refresh_views(dialog, st, planet_name);

    if let Some(mut view) = planet_view() {
        planet_view_call(
            &mut view,
            "set_planet",
            &[GString::from(planet_name).to_variant()],
        );
        let mut packed = VarArray::new();
        for sector in planet_sectors(dialog, st, planet_name) {
            packed.push(&sector.to_variant());
        }
        planet_view_call(&mut view, "set_sectors", &[packed.to_variant()]);
    }

    // code-instantiated: the planet rail is the data-driven Planets list.
    let Some(mut root) = st.root.clone() else {
        return;
    };
    let mut rail = MindTable::new_alloc();
    rail.set_custom_minimum_size(Vector2::new(208.0, 0.0));
    root.bind_mut()
        .add(&rail)
        .pad(0.0)
        .align(kit::Align::Center);
    planet_build_rail(dialog, state, st, &mut rail, planet_name);
    planet_build_sector_list(dialog, state, st, planet_name);

    let mut selected = planet_find_sector(dialog, st, planet_name, st.selected_sector);
    if selected.is_empty() && select_default {
        selected = planet_default_sector(dialog, st, planet_name);
        st.selected_sector = row_i64(&selected, "id");
    }
    if let Some(mut view) = planet_view() {
        if selected.is_empty() {
            planet_view_call(&mut view, "clear_selection", &[]);
        } else {
            let tile = planet_view_tile(&mut view, &selected);
            planet_view_call(&mut view, "focus_tile", &[tile.to_variant()]);
            planet_view_call(
                &mut view,
                "set_sector_icon",
                &[GString::from(row_str(&selected, "preset").as_str()).to_variant()],
            );
        }
    }

    planet_build_action_row(dialog, state, st, &selected);
    let mut rail_sort = rail.clone();
    st.deferred.push(move || {
        rail_sort.bind_mut().sort_now();
    });
    let target = dialog.to_gd();
    let state_for_complete = state.clone();
    st.deferred.push(move || {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_maybe_show_campaign_complete(&mut bound, &state_for_complete);
    });
}

/// `_default_sector`: start sector, else Ground Zero, else the first unlocked.
fn planet_default_sector(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet_name: &str,
) -> VarDictionary {
    let sectors = planet_sectors(dialog, st, planet_name);
    if sectors.is_empty() {
        return VarDictionary::new();
    }
    let mut start = -1i64;
    if let Some(mut view) = planet_view() {
        let data = planet_view_data(&mut view);
        if !data.is_empty() {
            start = row_i64_or(&data, "start_sector", -1);
        }
        for sector in &sectors {
            if planet_view_tile(&mut view, sector) == start {
                return sector.clone();
            }
        }
    }
    if let Some(sector) = sectors
        .iter()
        .find(|sector| row_str(sector, "preset") == "groundZero")
    {
        return sector.clone();
    }
    if let Some(sector) = sectors
        .iter()
        .find(|sector| !row_bool_or(sector, "locked", true))
    {
        return sector.clone();
    }
    sectors[0].clone()
}

/// `_maybe_show_campaign_complete`: the completion screen after the last capture.
fn planet_maybe_show_campaign_complete(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    let planet_name = st.selected_planet.clone();
    let views = planet_ensure_views(dialog, &mut st, &planet_name);
    let last_captured = row_rows(&views, "sectors")
        .iter()
        .any(|sector| row_bool(sector, "is_last") && row_bool(sector, "captured"));
    if !last_captured {
        return;
    }
    let complete = row_dict(&views, "complete");
    let mut planet = row_str(&complete, "planet");
    if planet.is_empty() {
        planet = row_str(&views, "planet");
    }
    if planet.is_empty() || planet == st.complete_announced {
        return;
    }
    if let Some(complete_dialog) = dialog_node("campaign_complete")
        && complete_dialog.has_method("is_shown")
        && complete_dialog
            .clone()
            .call("is_shown", &[])
            .try_to::<bool>()
            .unwrap_or(false)
    {
        return;
    }
    st.complete_announced = planet;
    dialog.open_dialog("campaign_complete", &json_stringify(&complete));
}

/// `_build_sector_list`: the collapsible top-right based-sector list.
fn planet_build_sector_list(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    planet_name: &str,
) {
    planet_remove_sector_list(st);
    let sectors = planet_sectors(dialog, st, planet_name);
    let mut based: Vec<VarDictionary> = Vec::new();
    let mut attacked = 0i64;
    for sector in &sectors {
        if row_bool(sector, "attacked") {
            attacked += 1;
        }
        if row_bool(sector, "has_base") {
            based.push(sector.clone());
        }
    }
    if based.is_empty() {
        return;
    }
    based.sort_by(|a, b| {
        let attacked_a = row_bool(a, "attacked");
        let attacked_b = row_bool(b, "attacked");
        if attacked_a != attacked_b {
            return if attacked_a {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        let playtime_a = row_i64(&row_dict(a, "stats"), "playtime_ms");
        let playtime_b = row_i64(&row_dict(b, "stats"), "playtime_ms");
        playtime_b.cmp(&playtime_a)
    });

    // code-instantiated: rebuilt with the planet view; content is the live
    // based-sector set. Java: `sectorList.background(Styles.black8)`.
    let mut backdrop = PanelContainer::new_alloc();
    backdrop.set_name("SectorListBackdrop");
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(styles::BLACK_8);
    backdrop.add_theme_stylebox_override("panel", &box_);
    backdrop.set_anchors_preset(LayoutPreset::TOP_RIGHT);
    backdrop.set_h_grow_direction(GrowDirection::BEGIN);
    backdrop.set_offset(Side::LEFT, -295.0);
    backdrop.set_offset(Side::RIGHT, -5.0);
    backdrop.set_offset(Side::TOP, 3.0);
    backdrop.set_custom_minimum_size(Vector2::new(290.0, 0.0));
    let control: Gd<Control> = backdrop.clone().upcast();
    dialog.base_mut().add_child(&control);
    st.sector_list_backdrop = Some(backdrop.clone());

    // code-instantiated: the toggle + entry list of the sector list.
    let mut list = VBoxContainer::new_alloc();
    list.set_name("SectorList");
    list.add_theme_constant_override("separation", 0);
    backdrop.add_child(&list);
    st.sector_list = Some(list.clone());

    let mut toggle = Button::new_alloc();
    toggle.set_toggle_mode(true);
    set_toggle(&toggle, st.sectors_shown);
    toggle.set_custom_minimum_size(Vector2::new(290.0, 60.0));
    dialog.apply_pane_button(&mut toggle, OUTLINE_GRAY, 4.0);
    let mut raw = kit::t("@sectorlist").to_string();
    if attacked > 0 {
        let count = format!("[red]{attacked}[]");
        let line = match kit::assets() {
            Some(assets) => {
                let mut args = PackedStringArray::new();
                args.push(&count);
                assets
                    .bind()
                    .bundle_format(GString::from("sectorlist.attacked"), args)
                    .to_string()
            }
            None => count.clone(),
        };
        raw.push_str(&format!("\n[red]\u{26a0}[lightgray] {line}"));
    }
    // code-instantiated: toggle content row (arrow glyph + caption).
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 7.0);
    row.set_offset(Side::RIGHT, -7.0);
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    toggle.add_child(&row);
    let arrow = glyph_control(
        if st.sectors_shown {
            "up-open"
        } else {
            "down-open"
        },
        22.0,
    );
    row.add_child(&arrow);
    let caption = factory::label(&kit::markup_text(&raw).to_string());
    {
        let mut label = rich(&caption);
        label.set_fit_content(true);
        label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
        label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::OFF);
        label.set_mouse_filter(MouseFilter::IGNORE);
    }
    {
        let mut control: Gd<Control> = caption.clone().upcast();
        control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    }
    row.add_child(&caption);
    let target = dialog.to_gd();
    let state_for_toggle = state.clone();
    let callable = Callable::from_fn("planet_sector_list_toggle", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        let mut st = state_for_toggle.borrow_mut();
        st.sectors_shown = !st.sectors_shown;
        let planet = st.selected_planet.clone();
        planet_build_sector_list(&mut bound, &state_for_toggle, &mut st, &planet);
    });
    toggle.connect("pressed", &callable);
    list.add_child(&toggle);
    if !st.sectors_shown {
        return;
    }

    let mut panel = panel_with_box(pane_box(
        Color::from_rgba(0.0, 0.0, 0.0, 0.8),
        Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        0,
        0,
    ));
    list.add_child(&panel);
    // code-instantiated: the search + scrolling entry column.
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 3);
    panel.add_child(&column);
    let mut search_row = HBoxContainer::new_alloc();
    let zoom = glyph_control("zoom", 22.0);
    search_row.add_child(&zoom);
    let mut field = LineEdit::new_alloc();
    field.set_text(&GString::from(st.sector_search.as_str()));
    field.set_custom_minimum_size(Vector2::new(0.0, 50.0));
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    search_row.add_child(&field);
    column.add_child(&search_row);
    let mut scroll = ScrollContainer::new_alloc();
    scroll.set_horizontal_scroll_mode(godot::classes::scroll_container::ScrollMode::DISABLED);
    scroll.set_custom_minimum_size(Vector2::new(0.0, 320.0));
    column.add_child(&scroll);
    // code-instantiated: one underlined row per based sector.
    let mut entries = VBoxContainer::new_alloc();
    entries.set_h_size_flags(SizeFlags::EXPAND_FILL);
    scroll.add_child(&entries);
    let target = dialog.to_gd();
    let state_for_search = state.clone();
    let entries_for_search = entries.clone();
    let based_for_search = based.clone();
    let callable = Callable::from_fn("planet_sector_search", move |args: &[&Variant]| {
        let text = args
            .first()
            .and_then(|value| value.try_to::<GString>().ok())
            .map(|value| value.to_string())
            .unwrap_or_default();
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        let mut st = state_for_search.borrow_mut();
        st.sector_search = text;
        planet_fill_sector_entries(
            &mut bound,
            &state_for_search,
            &mut st,
            &entries_for_search,
            &based_for_search,
        );
    });
    field.connect("text_changed", &callable);
    planet_fill_sector_entries(dialog, state, st, &entries, &based);
}

/// `rebuildList.readd`: one `Styles.underlineb` row per matching based sector.
fn planet_fill_sector_entries(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    entries: &Gd<VBoxContainer>,
    based: &[VarDictionary],
) {
    let mut entries = entries.clone();
    for child in entries.get_children().iter_shared() {
        let mut child = child.clone();
        child.queue_free();
    }
    let needle = st.sector_search.to_lowercase();
    for sector in based {
        let title = planet_localized_sector(sector);
        if !needle.is_empty() && !title.to_lowercase().contains(&needle) {
            continue;
        }
        let mut entry = Button::new_alloc();
        entry.set_toggle_mode(true);
        set_toggle(&entry, row_i64(sector, "id") == st.selected_sector);
        entry.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        dialog.apply_pane_button(&mut entry, OUTLINE_GRAY, 3.0);
        let preset = row_str(sector, "preset");
        let icon: Option<Gd<Control>> = if row_bool(sector, "attacked") {
            Some(factory::glyph_label("warn", 18, styles::ACCENT).upcast())
        } else if !preset.is_empty() {
            let mut image = factory::image(&format!("sector-{preset}"));
            image.set_custom_minimum_size(Vector2::new(20.0, 20.0));
            image.set_mouse_filter(MouseFilter::IGNORE);
            Some(image.upcast())
        } else {
            None
        };
        dialog.build_button_content(&mut entry, icon, &title);
        let sector_id = row_i64(sector, "id");
        let target = dialog.to_gd();
        let state_for_pick = state.clone();
        let callable = Callable::from_fn("planet_sector_entry", move |_args: &[&Variant]| {
            let mut bound = target.clone();
            let mut bound = bound.bind_mut();
            let mut st = state_for_pick.borrow_mut();
            st.selected_sector = sector_id;
            let planet = st.selected_planet.clone();
            planet_show_planet(&mut bound, &state_for_pick, &mut st, &planet, true);
        });
        entry.connect("pressed", &callable);
        entries.add_child(&entry);
    }
    if entries.get_child_count() == 0 {
        let none = factory::label(&kit::tm("@none.found").to_string());
        entries.add_child(&none);
    }
}

fn planet_remove_sector_list(st: &mut PlanetState) {
    if let Some(mut backdrop) = st.sector_list_backdrop.take()
        && backdrop.is_instance_valid()
    {
        backdrop.queue_free();
    }
    st.sector_list = None;
}

/// The left planet/difficulty rail (`PlanetDialog.java:732-739`).
fn planet_build_rail(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    rail: &mut Gd<MindTable>,
    planet_name: &str,
) {
    let views = planet_refresh_views(dialog, st, planet_name);
    let planets = row_rows(&views, "planets");
    let mut first_planet = true;
    for rail_name in ["erekir", "serpulo"] {
        let Some(planet) = planets.iter().find(|row| row_str(row, "name") == rail_name) else {
            continue;
        };
        if !row_bool_or(planet, "visible", true) {
            continue;
        }
        let button = planet_make_planet_button(dialog, state, planet, rail_name == planet_name);
        rail.bind_mut()
            .add(&button)
            .grow_x()
            .pad_left(0.0)
            .pad_right(0.0)
            .pad_bottom(0.0)
            .pad_top(if first_planet { 2.0 } else { 0.0 });
        first_planet = false;
        rail.bind_mut().row();
    }
    // code-instantiated: `@campaign.difficulty` opens the campaign rules dialog.
    let mut difficulty = Button::new_alloc();
    difficulty.set_theme_type_variation("flatTogglet");
    difficulty.set_custom_minimum_size(Vector2::new(208.0, 40.0));
    dialog.apply_pane_button(&mut difficulty, OUTLINE_GRAY, 4.0);
    let glyph = glyph_control("book", 20.0);
    let glyph_control_gd: Gd<Control> = glyph.upcast();
    let label = kit::t("@campaign.difficulty").to_string();
    dialog.build_button_content(&mut difficulty, Some(glyph_control_gd), &label);
    let target = dialog.to_gd();
    let planet_for_rules = planet_name.to_string();
    let callable = Callable::from_fn("planet_campaign_rules", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let bound = bound.bind_mut();
        let mut context = VarDictionary::new();
        context.set("planet", &GString::from(planet_for_rules.as_str()));
        bound.open_dialog("campaign_rules", &json_stringify(&context));
    });
    difficulty.connect("pressed", &callable);
    let selected_planet = planets
        .iter()
        .find(|row| row_str(row, "name") == planet_name)
        .cloned()
        .unwrap_or_default();
    difficulty.set_visible(
        row_bool_or(&selected_planet, "allow_campaign_rules", true)
            && st.launch_mode != "planetLaunch",
    );
    rail.bind_mut()
        .add(&difficulty)
        .grow_x()
        .pad_left(0.0)
        .pad_right(0.0)
        .pad_top(13.0)
        .pad_bottom(2.0);
    rail.bind_mut().row();
}

/// One rail row: planet art + localized name, accent-tinted checked border.
fn planet_make_planet_button(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    planet: &VarDictionary,
    selected: bool,
) -> Gd<Button> {
    // code-instantiated: rail rows carry the vendored planet art plus the
    // localized name, so each entry is built at runtime.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("flatTogglet");
    button.set_toggle_mode(true);
    set_toggle(&button, selected);
    button.set_custom_minimum_size(Vector2::new(208.0, 44.0));
    dialog.apply_pane_button(&mut button, OUTLINE_GRAY, 0.0);
    let planet_color = Color::from_string(&row_str(planet, "color")).unwrap_or(Color::WHITE);
    let checked = Color::from_rgba(
        styles::ACCENT.r * planet_color.r,
        styles::ACCENT.g * planet_color.g,
        styles::ACCENT.b * planet_color.b,
        1.0,
    );
    let checked_box = pane_box(Color::BLACK, checked, 4, 2);
    button.add_theme_stylebox_override("pressed", &checked_box);
    button.add_theme_stylebox_override("hover_pressed", &checked_box);
    let icon: Option<Gd<Control>> = planet_texture(planet).map(|texture| {
        let mut image = TextureRect::new_alloc();
        image.set_texture(&texture);
        image.set_custom_minimum_size(Vector2::new(20.0, 20.0));
        image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        image.set_mouse_filter(MouseFilter::IGNORE);
        image.upcast()
    });
    let label = planet_localized_planet(planet);
    dialog.build_button_content(&mut button, icon, &label);
    let planet_name = row_str(planet, "name");
    let target = dialog.to_gd();
    let state_for_switch = state.clone();
    let callable = Callable::from_fn("planet_rail_switch", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_switch_planet(&mut bound, &state_for_switch, &planet_name);
    });
    button.connect("pressed", &callable);
    button
}

/// `PlanetDialog.viewPlanet`: switching planets clears the selection.
fn planet_switch_planet(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    planet_name: &str,
) {
    let mut st = state.borrow_mut();
    if st.selected_planet == planet_name {
        return;
    }
    st.selected_sector = -1;
    planet_show_planet(dialog, state, &mut st, planet_name, false);
}

// ---- planet page action row + sector panel --------------------------------

/// `PlanetDialog` bottom row: `@back`, the selected-sector readout, `@techtree`.
fn planet_build_action_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    selected: &VarDictionary,
) {
    dialog.clear_buttons();
    let Some(mut buttons) = dialog.buttons_row() else {
        return;
    };
    // The reference row starts 2 px right of the rail's pane edge.
    let lead = spacer(false, 2.0, 0.0);
    let lead_control: Gd<Control> = lead.clone().upcast();
    buttons.add_child(&lead_control);
    buttons.move_child(&lead_control, 0);

    let back_text = kit::t("@back").to_string();
    let mut back = factory::icon_button(
        "left",
        &back_text,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    back.set_custom_minimum_size(Vector2::new(200.0, 54.0));
    back.set_v_size_flags(SizeFlags::SHRINK_END);
    let target = dialog.to_gd();
    let callable = Callable::from_fn("planet_close", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        bound.close();
    });
    back.connect("pressed", &callable);
    buttons.add_child(&back);
    let left_spacer = spacer(true, 0.0, 0.0);
    let left_spacer_control: Gd<Control> = left_spacer.clone().upcast();
    buttons.add_child(&left_spacer_control);
    if !selected.is_empty() {
        let panel = planet_build_sector_panel(dialog, state, st, selected);
        buttons.add_child(&panel);
    }
    let right_spacer = spacer(true, 0.0, 0.0);
    let right_spacer_control: Gd<Control> = right_spacer.clone().upcast();
    buttons.add_child(&right_spacer_control);

    let tech_text = kit::t("@techtree").to_string();
    let mut tech_tree = factory::icon_button(
        "tree",
        &tech_text,
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    tech_tree.set_custom_minimum_size(Vector2::new(200.0, 54.0));
    tech_tree.set_v_size_flags(SizeFlags::SHRINK_END);
    let target = dialog.to_gd();
    let state_for_tree = state.clone();
    let callable = Callable::from_fn("planet_tech_tree", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let bound = bound.bind_mut();
        let planet = state_for_tree.borrow().selected_planet.clone();
        let mut context = VarDictionary::new();
        context.set("planet", &GString::from(planet.as_str()));
        bound.open_dialog("research", &json_stringify(&context));
    });
    tech_tree.connect("pressed", &callable);
    buttons.add_child(&tech_tree);
}

/// The 230 px `Styles.black6` selected-sector readout (`updateSelected`).
fn planet_build_sector_panel(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    sector: &VarDictionary,
) -> Gd<PanelContainer> {
    // code-instantiated: bottom-centre readout rebuilt per selected sector.
    let mut panel = panel_with_box(pane_box(
        Color::from_rgba(0.0, 0.0, 0.0, 0.6),
        Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        0,
        0,
    ));
    panel.set_custom_minimum_size(Vector2::new(230.0, 0.0));
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 0);
    panel.add_child(&column);
    if sector.is_empty() {
        let label = factory::label(&kit::t("@sectors.select").to_string());
        column.add_child(&label);
        return panel;
    }

    // code-instantiated: sector title row (name + rename/icon buttons).
    let mut title_row = HBoxContainer::new_alloc();
    title_row.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    title_row.add_theme_constant_override("separation", 4);
    title_row.set_custom_minimum_size(Vector2::new(0.0, 41.0));
    column.add_child(&title_row);
    let title = factory::label(&planet_localized_sector(sector));
    {
        let mut label = rich(&title);
        label.add_theme_color_override("default_color", styles::ACCENT);
        label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::OFF);
        label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    }
    title_row.add_child(&title);
    let preset = row_str(sector, "preset");
    let sector_id = row_i64(sector, "id");
    if preset.is_empty() || !row_bool_or(sector, "require_unlock", true) {
        let title_spacer = spacer(true, 0.0, 0.0);
        let title_spacer_control: Gd<Control> = title_spacer.clone().upcast();
        title_row.add_child(&title_spacer_control);
        // code-instantiated: per-sector rename pencil (`pencilSmall`).
        let mut pencil = factory::glyph_button("pencil", "", "clearNonei");
        pencil.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        pencil.set_tooltip_text(&kit::t("@sectors.rename"));
        let target = dialog.to_gd();
        let state_for_rename = state.clone();
        let callable = Callable::from_fn("planet_rename_sector", move |_args: &[&Variant]| {
            let mut bound = target.clone();
            let mut bound = bound.bind_mut();
            let mut st = state_for_rename.borrow_mut();
            planet_rename_sector(&mut bound, &state_for_rename, &mut st, sector_id);
        });
        pencil.connect("pressed", &callable);
        title_row.add_child(&pencil);
    }
    // `PlanetDialog.java:1211-1226`: the marker icon button, always present.
    let mut icon_button = Button::new_alloc();
    icon_button.set_theme_type_variation("clearNonei");
    icon_button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    icon_button.set_tooltip_text(&kit::t("@sector.changeicon"));
    let icon_content = planet_sector_icon_content(sector);
    icon_button.add_child(&icon_content);
    let target = dialog.to_gd();
    let state_for_icon = state.clone();
    let callable = Callable::from_fn("planet_sector_icon", move |_args: &[&Variant]| {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        let mut st = state_for_icon.borrow_mut();
        planet_open_sector_icon_dialog(&mut bound, &state_for_icon, &mut st, sector_id);
    });
    icon_button.connect("pressed", &callable);
    title_row.add_child(&icon_button);

    // code-instantiated: the accent divider under the sector name.
    let mut divider = ColorRect::new_alloc();
    divider.set_color(styles::ACCENT);
    divider.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    divider.set_mouse_filter(MouseFilter::IGNORE);
    column.add_child(&divider);

    let has_base = row_bool(sector, "has_base");
    if row_bool(sector, "objectives_locked") {
        let complete_label = factory::label(&kit::tm("@complete").to_string());
        complete_label
            .clone()
            .upcast::<Control>()
            .set_custom_minimum_size(Vector2::new(0.0, 26.0));
        let mut complete = rich(&complete_label);
        complete.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
        column.add_child(&complete_label);
        for objective in row_arr(sector, "objectives").iter_shared() {
            let Ok(objective) = objective.try_to::<VarArray>() else {
                continue;
            };
            let values: Vec<String> = objective
                .iter_shared()
                .map(|value| {
                    value
                        .try_to::<GString>()
                        .map(|v| v.to_string())
                        .unwrap_or_default()
                })
                .collect();
            let kind = values.first().cloned().unwrap_or_default();
            let argument = values.get(1).cloned().unwrap_or_default();
            // code-instantiated: one row per incomplete objective.
            let mut objective_row = HBoxContainer::new_alloc();
            objective_row.add_theme_constant_override("separation", 3);
            let localized = planet_localized_objective_arg(&kind, &argument);
            let key = format!("@requirement.{kind}");
            let text = format!("> {}", kit::tm_format(&key, &[localized.as_str()]));
            let objective_label = factory::label(&text);
            rich(&objective_label).add_theme_color_override("default_color", TEXT_GRAY);
            objective_row.add_child(&objective_label);
            let cancel = factory::glyph_label("cancel", 18, PAL_REMOVE);
            objective_row.add_child(&cancel);
            column.add_child(&objective_row);
        }
    } else if !has_base {
        let mut threat = row_str(sector, "threat_band");
        if threat.is_empty() {
            threat = String::from("low");
        }
        let mut color = row_str(sector, "threat_color");
        if color.is_empty() {
            color = String::from("ffffffff");
        }
        let color: String = color.chars().take(6).collect();
        let text = format!(
            "{} [color=#{}]{}[/color]",
            kit::t("@sectors.threat"),
            color,
            kit::t(&format!("@threat.{threat}"))
        );
        let threat_label = factory::label(&text);
        threat_label
            .clone()
            .upcast::<Control>()
            .set_custom_minimum_size(Vector2::new(0.0, 26.0));
        rich(&threat_label).set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
        column.add_child(&threat_label);
    }

    let state_labels = planet_sector_state_labels(sector);
    for state_key in &state_labels {
        let label = factory::label(&kit::tm(state_key).to_string());
        let mut label = rich(&label);
        label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        column.add_child(&label);
    }

    let stats = row_dict(sector, "stats");
    let resources: Vec<String> = row_arr(&stats, "resources")
        .iter_shared()
        .map(|value| {
            value
                .try_to::<GString>()
                .map(|v| v.to_string())
                .unwrap_or_default()
        })
        .collect();
    if row_bool(sector, "has_save") && !resources.is_empty() {
        // code-instantiated: the stored-resource icon row.
        let mut resource_row = HBoxContainer::new_alloc();
        resource_row.add_theme_constant_override("separation", 3);
        let lead = spacer(false, 7.0, 0.0);
        let lead_control: Gd<Control> = lead.clone().upcast();
        resource_row.add_child(&lead_control);
        let resource_label = factory::label(&kit::t("@sectors.resources").to_string());
        resource_row.add_child(&resource_label);
        for resource in &resources {
            let Some(texture) = content_texture(resource) else {
                continue;
            };
            let image = icon_rect(&texture, 20.0);
            resource_row.add_child(&image);
        }
        column.add_child(&resource_row);
    }

    if has_base {
        let mut stats_button = factory::button("");
        stats_button.set_theme_type_variation("cleart");
        stats_button.set_custom_minimum_size(Vector2::new(0.0, 40.0));
        let glyph = glyph_control("info", 20.0);
        let glyph_gd: Gd<Control> = glyph.upcast();
        let stats_text = kit::t("@stats").to_string();
        dialog.build_button_content(&mut stats_button, Some(glyph_gd), &stats_text);
        let target = dialog.to_gd();
        let state_for_stats = state.clone();
        let callable = Callable::from_fn("planet_sector_stats", move |_args: &[&Variant]| {
            let mut bound = target.clone();
            let bound = bound.bind_mut();
            let planet = state_for_stats.borrow().selected_planet.clone();
            let mut context = VarDictionary::new();
            context.set("planet", &GString::from(planet.as_str()));
            context.set("sector", sector_id);
            bound.open_dialog("sector_stats", &json_stringify(&context));
        });
        stats_button.connect("pressed", &callable);
        column.add_child(&stats_button);
    }

    let action = planet_sector_action(dialog, sector, &st.launch_mode);
    let mut show_action = row_bool_or(sector, "show_action", true);
    if st.launch_mode == "planetLaunch" {
        show_action = row_bool(sector, "accelerator_selectable");
    }
    if show_action {
        // code-instantiated: the accent-framed in-panel action button.
        let mut launch = factory::button("");
        launch.set_custom_minimum_size(Vector2::new(230.0, 53.0));
        for state_name in ["normal", "hover", "pressed", "disabled"] {
            launch.add_theme_stylebox_override(
                state_name,
                &pane_box(Color::BLACK, styles::ACCENT, 4, 2),
            );
        }
        let icon = glyph_control(&row_str(&action, "icon"), 20.0);
        let icon_gd: Gd<Control> = icon.upcast();
        let label = kit::t(&row_str(&action, "label")).to_string();
        dialog.build_button_content(&mut launch, Some(icon_gd), &label);
        launch.set_disabled(row_bool(&action, "disabled"));
        let mut tooltip = String::new();
        for state_key in &state_labels {
            if !tooltip.is_empty() {
                tooltip.push(' ');
            }
            tooltip.push_str(&kit::t(state_key).to_string());
        }
        launch.set_tooltip_text(&GString::from(tooltip.as_str()));
        let mode = row_str(&action, "mode");
        let target = dialog.to_gd();
        let state_for_action = state.clone();
        let callable = Callable::from_fn("planet_sector_action", move |_args: &[&Variant]| {
            let mut bound = target.clone();
            let mut bound = bound.bind_mut();
            let mut st = state_for_action.borrow_mut();
            let planet = st.selected_planet.clone();
            let sector_id = st.selected_sector;
            planet_sector_action_pressed(
                &mut bound,
                &state_for_action,
                &mut st,
                &planet,
                sector_id,
                &mode,
            );
        });
        launch.connect("pressed", &callable);
        column.add_child(&launch);
    }
    panel
}

/// `_sector_icon_content`: `content_icon` texture, chosen glyph, or `Icon.none`.
fn planet_sector_icon_content(sector: &VarDictionary) -> Gd<Control> {
    let content_icon = row_str(sector, "content_icon");
    if !content_icon.is_empty()
        && let Some(texture) = content_texture(&content_icon)
    {
        let mut image = TextureRect::new_alloc();
        image.set_texture(&texture);
        image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        image.set_mouse_filter(MouseFilter::IGNORE);
        image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        return image.upcast();
    }
    let mut icon_name = row_str(sector, "icon");
    if icon_name.is_empty() {
        icon_name = String::from("none");
    }
    let mut glyph = glyph_control(&icon_name, 26.0);
    glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    glyph.set_mouse_filter(MouseFilter::IGNORE);
    glyph.upcast()
}

/// `_rename_sector`: `ui.showTextInput("@sectors.rename", "@name", 32, ...)`.
fn planet_rename_sector(
    dialog: &mut MindDialog,
    _state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    sector_id: i64,
) {
    let planet = st.selected_planet.clone();
    let sector = planet_find_sector(dialog, st, &planet, sector_id);
    if sector.is_empty() {
        return;
    }
    st.rename_target = sector_id;
    let _ = ui_call(
        "show_text_input",
        &[
            kit::t("@sectors.rename").to_variant(),
            kit::t("@name").to_variant(),
            32i64.to_variant(),
            GString::from(planet_localized_sector(&sector).as_str()).to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `_on_text_input_result`: `Sector.setName` then a full page rebuild.
fn planet_on_text_input(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>, text: &str) {
    let mut st = state.borrow_mut();
    if st.rename_target < 0 {
        return;
    }
    let sector_id = st.rename_target;
    st.rename_target = -1;
    if text.trim().is_empty() {
        return;
    }
    let planet = st.selected_planet.clone();
    let Some(mut campaign) = dialog.campaign_node() else {
        return;
    };
    if !campaign.has_method("set_sector_name") {
        return;
    }
    let renamed = campaign
        .call(
            "set_sector_name",
            &[
                GString::from(planet.as_str()).to_variant(),
                sector_id.to_variant(),
                GString::from(text).to_variant(),
            ],
        )
        .try_to::<bool>()
        .unwrap_or(false);
    if renamed {
        planet_show_planet(dialog, state, &mut st, &planet, true);
    }
}

/// `_open_sector_icon_dialog`: the marker picker in `icon_select`'s sector mode.
fn planet_open_sector_icon_dialog(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    sector_id: i64,
) {
    st.icon_target = sector_id;
    let target = dialog.to_gd();
    let state_for_icon = state.clone();
    let callable = Callable::from_fn("planet_sector_icon_selected", move |args: &[&Variant]| {
        let icon = args
            .first()
            .and_then(|value| value.try_to::<GString>().ok())
            .map(|value| value.to_string())
            .unwrap_or_default();
        let content_icon = args
            .get(1)
            .and_then(|value| value.try_to::<GString>().ok())
            .map(|value| value.to_string())
            .unwrap_or_default();
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        planet_on_sector_icon_selected(&mut bound, &state_for_icon, &icon, &content_icon);
    });
    if let Some(selector) = dialog_node("icon_select") {
        connect_once(
            dialog,
            "sector_icon_selected",
            &selector,
            "sector_icon_selected",
            &callable,
        );
    }
    let planet = st.selected_planet.clone();
    let sector = planet_find_sector(dialog, st, &planet, sector_id);
    let mut context = VarDictionary::new();
    context.set("mode", &GString::from("sector"));
    context.set("icon", &GString::from(row_str(&sector, "icon").as_str()));
    context.set(
        "content_icon",
        &GString::from(row_str(&sector, "content_icon").as_str()),
    );
    dialog.open_dialog("icon_select", &json_stringify(&context));
}

/// The picker's choice: write the marker through `MindCampaign.set_sector_icon`.
fn planet_on_sector_icon_selected(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    icon: &str,
    content_icon: &str,
) {
    let mut st = state.borrow_mut();
    if st.icon_target < 0 {
        return;
    }
    let sector_id = st.icon_target;
    st.icon_target = -1;
    let _ = ui_call("close_dialog", &[GString::from("icon_select").to_variant()]);
    let planet = st.selected_planet.clone();
    let Some(mut campaign) = dialog.campaign_node() else {
        return;
    };
    if !campaign.has_method("set_sector_icon") {
        return;
    }
    let changed = campaign
        .call(
            "set_sector_icon",
            &[
                GString::from(planet.as_str()).to_variant(),
                sector_id.to_variant(),
                GString::from(icon).to_variant(),
                GString::from(content_icon).to_variant(),
            ],
        )
        .try_to::<bool>()
        .unwrap_or(false);
    if changed {
        planet_show_planet(dialog, state, &mut st, &planet, true);
    }
}

/// `_sector_action`: label/icon/mode/disabled for the selected sector.
fn planet_sector_action(
    dialog: &mut MindDialog,
    sector: &VarDictionary,
    launch_mode: &str,
) -> VarDictionary {
    let locked = row_bool(sector, "objectives_locked");
    let mut out = VarDictionary::new();
    if launch_mode == "planetLaunch" {
        // `PlanetDialog.java:438`: a locked destination keeps the lock entry
        // (its `disabled` flag and label), everything else launches.
        if locked {
            out.set("label", &GString::from("@locked"));
            out.set("icon", &GString::from("lock"));
            out.set("disabled", true);
        } else {
            out.set("label", &GString::from("@sectors.launch"));
            out.set("icon", &GString::from("play"));
            out.set("disabled", false);
        }
        out.set("mode", &GString::from("planetLaunch"));
        return out;
    }
    let no_candidate = row_bool(sector, "no_candidate");
    let disabled = locked || no_candidate;
    let (label, icon, mode) = if planet_is_being_played(dialog, sector) {
        ("@sectors.resume", "play", "resume")
    } else if row_bool(sector, "has_base") || row_bool(sector, "captured") {
        ("@sectors.go", "play", "go")
    } else if locked {
        ("@locked", "lock", "launch")
    } else if no_candidate {
        ("@sectors.nolaunchcandidate", "play", "launch")
    } else {
        ("@sectors.launch", "play", "launch")
    };
    out.set("label", &GString::from(label));
    out.set("icon", &GString::from(icon));
    out.set("mode", &GString::from(mode));
    out.set("disabled", disabled);
    out
}

/// `Sector.isBeingPlayed` for a row (`MindCampaign.get_sector_state`).
fn planet_is_being_played(dialog: &mut MindDialog, sector: &VarDictionary) -> bool {
    planet_sector_is_being_played(
        dialog,
        &row_str(sector, "planet"),
        row_i64_or(sector, "id", -2),
    )
}

/// `Sector.isBeingPlayed` for the page's selected sector, addressed by
/// planet/id (`MindCampaign.get_sector_state`).
fn planet_sector_is_being_played(
    dialog: &mut MindDialog,
    planet_name: &str,
    sector_id: i64,
) -> bool {
    let Some(mut campaign) = dialog.campaign_node() else {
        return false;
    };
    if !campaign.has_method("get_sector_state") {
        return false;
    }
    let Ok(state) = campaign
        .call("get_sector_state", &[])
        .try_to::<VarDictionary>()
    else {
        return false;
    };
    row_bool(&state, "campaign")
        && row_str(&state, "planet") == planet_name
        && row_i64_or(&state, "sector", -1) == sector_id
}

fn planet_sector_action_pressed(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    planet_name: &str,
    sector_id: i64,
    mode: &str,
) {
    match mode {
        "planetLaunch" => planet_accelerator_launch(dialog, st, planet_name, sector_id),
        // `PlanetDialog.playSelected` (`PlanetDialog.java:1373-1377`):
        // `if(sector.isBeingPlayed()){ hide(); return; }` — resuming the
        // sector that is already live only hides the page. Routing it through
        // the load path reloads the last save, discarding everything built
        // since and resetting the wave timer (CG-UI-01).
        "resume" | "go" if planet_sector_is_being_played(dialog, planet_name, sector_id) => {
            let _ = ui_call("close_dialog", &[GString::from("planet").to_variant()]);
        }
        "resume" | "go" => planet_launch(dialog, st, planet_name, sector_id, -1, true),
        _ => planet_launch_via_launcher(dialog, state, st, planet_name, sector_id),
    }
}

/// `playSelected` in `planetLaunch` mode: the accelerator callback + hide.
fn planet_accelerator_launch(
    dialog: &mut MindDialog,
    st: &PlanetState,
    planet_name: &str,
    sector_id: i64,
) {
    if let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("accelerator_launch_to")
    {
        let _ = campaign.call(
            "accelerator_launch_to",
            &[
                st.accelerator_x.to_variant(),
                st.accelerator_y.to_variant(),
                GString::from(planet_name).to_variant(),
                sector_id.to_variant(),
            ],
        );
    }
    let _ = ui_call("close_dialog", &[GString::from("planet").to_variant()]);
}

/// `playSelected` look mode: launcher loadout dialog, else a free launch.
fn planet_launch_via_launcher(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    planet_name: &str,
    sector_id: i64,
) {
    let launcher = planet_sector_launch_from(dialog, st, planet_name, sector_id);
    if launcher < 0 {
        planet_launch(dialog, st, planet_name, sector_id, -1, false);
        return;
    }
    planet_open_launch_loadout(dialog, state, st, planet_name, sector_id, launcher);
}

/// The view's `launch_from` for a destination (`-1` = free launch).
fn planet_sector_launch_from(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet_name: &str,
    sector_id: i64,
) -> i64 {
    for sector in planet_sectors(dialog, st, planet_name) {
        if row_i64(&sector, "id") != sector_id {
            continue;
        }
        return row_i64_or(&sector, "launch_from", -1);
    }
    -1
}

// ---- launch flow ----------------------------------------------------------

/// `_open_launch_loadout`: asks for a loadout before entering a fresh sector.
fn planet_open_launch_loadout(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    planet_name: &str,
    sector_id: i64,
    launcher_id: i64,
) {
    let Some(loadout_dialog) = dialog_node("launch_loadout") else {
        planet_launch(dialog, st, planet_name, sector_id, -1, false);
        return;
    };
    if !loadout_dialog.has_signal("loadout_chosen") {
        planet_launch(dialog, st, planet_name, sector_id, -1, false);
        return;
    }
    let target = dialog.to_gd();
    let state_for_choice = state.clone();
    let callable = Callable::from_fn("planet_loadout_chosen", move |args: &[&Variant]| {
        let index = args
            .first()
            .and_then(|value| value.try_to::<i64>().ok())
            .unwrap_or(-1);
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        let _ = ui_call(
            "close_dialog",
            &[GString::from("launch_loadout").to_variant()],
        );
        let mut st = state_for_choice.borrow_mut();
        let planet = st.selected_planet.clone();
        let sector = st.selected_sector;
        planet_launch(&mut bound, &mut st, &planet, sector, index, false);
    });
    connect_once(
        dialog,
        "launch_loadout_chosen",
        &loadout_dialog,
        "loadout_chosen",
        &callable,
    );
    let mut context = VarDictionary::new();
    context.set("planet", &GString::from(planet_name));
    context.set("sector", sector_id);
    context.set("launcher", launcher_id);
    dialog.open_dialog("launch_loadout", &json_stringify(&context));
}

/// The `Schematics.all` row for a loadout index.
fn planet_find_loadout_row(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    index: i64,
) -> VarDictionary {
    let planet_name = st.selected_planet.clone();
    let views = planet_ensure_views(dialog, st, &planet_name);
    for loadout in row_rows(&views, "loadouts") {
        if row_i64_or(&loadout, "index", -1) == index {
            return loadout;
        }
    }
    VarDictionary::new()
}

/// `playSelected` launch cost: requirements + staged launch resources.
fn planet_loadout_launch_stacks(
    row: &VarDictionary,
    resources: &VarDictionary,
) -> Vec<VarDictionary> {
    let mut totals: Vec<(String, i64)> = Vec::new();
    let add = |item: &str, amount: i64, totals: &mut Vec<(String, i64)>| {
        if item.is_empty() {
            return;
        }
        if let Some(entry) = totals.iter_mut().find(|entry| entry.0 == item) {
            entry.1 += amount;
        } else {
            totals.push((item.to_string(), amount));
        }
    };
    for requirement in row_arr(row, "requirements").iter_shared() {
        let Ok(requirement) = requirement.try_to::<VarArray>() else {
            continue;
        };
        let mut values = requirement.iter_shared();
        let item = values
            .next()
            .and_then(|value| value.try_to::<GString>().ok())
            .map(|value| value.to_string())
            .unwrap_or_default();
        let amount = values
            .next()
            .and_then(|value| value.try_to::<i64>().ok())
            .unwrap_or(0);
        add(&item, amount, &mut totals);
    }
    for key in resources.keys_array().iter_shared() {
        let item = key
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        let amount = resources
            .get(item.as_str())
            .and_then(|value| value.try_to::<i64>().ok())
            .unwrap_or(0);
        add(&item, amount, &mut totals);
    }
    planet_stacks_of(&totals)
}

/// `[{item, amount}]` stacks from an ordered totals list.
fn planet_stacks_of(totals: &[(String, i64)]) -> Vec<VarDictionary> {
    totals
        .iter()
        .map(|(item, amount)| {
            let mut stack = VarDictionary::new();
            stack.set("item", &GString::from(item.as_str()));
            stack.set("amount", *amount);
            stack
        })
        .collect()
}

/// `[{item, amount}]` as a `VarArray` (the `start_sector_with_loadout` stacks).
fn planet_stacks_array(stacks: &[VarDictionary]) -> VarArray {
    let mut array = VarArray::new();
    for stack in stacks {
        array.push(&stack.to_variant());
    }
    array
}

/// JSON array text for a stack list.
fn planet_stacks_json(stacks: &[VarDictionary]) -> String {
    Json::stringify(&planet_stacks_array(stacks).to_variant()).to_string()
}

/// `MindCampaign.launch_dialog_json` for the launcher launch.
fn planet_launch_plan(
    dialog: &mut MindDialog,
    planet_name: &str,
    launcher: i64,
    sector_id: i64,
    loadout: i64,
) -> VarDictionary {
    dialog.campaign_json(
        "launch_dialog_json",
        &[
            GString::from(planet_name).to_variant(),
            launcher.to_variant(),
            sector_id.to_variant(),
            loadout.to_variant(),
        ],
    )
}

/// `universe.getLaunchResources()` as an item->amount map.
fn planet_launch_resource_totals(dialog: &mut MindDialog) -> VarDictionary {
    let Some(mut campaign) = dialog.campaign_node() else {
        return VarDictionary::new();
    };
    if !campaign.has_method("get_launch_resource_stacks") {
        return VarDictionary::new();
    }
    let text = campaign
        .call("get_launch_resource_stacks", &[])
        .to::<GString>();
    parse_object(&text.to_string())
}

/// `_launch`: enters the sector, either resuming, via a loadout, or free.
fn planet_launch(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    planet_name: &str,
    sector_id: i64,
    loadout: i64,
    resume: bool,
) {
    if sector_id < 0 {
        return;
    }
    dialog.to_gd().emit_signal(
        "sector_activated",
        &[
            GString::from(planet_name).to_variant(),
            sector_id.to_variant(),
        ],
    );
    let mut campaign = dialog.campaign_node();
    if let Some(campaign) = campaign.as_mut()
        && campaign.has_method("prime_sector_load")
    {
        let _ = campaign.call(
            "prime_sector_load",
            &[
                GString::from(planet_name).to_variant(),
                sector_id.to_variant(),
            ],
        );
    }

    let mut launcher_id = -1i64;
    let mut launch_resources = VarDictionary::new();
    let mut launch_row = VarDictionary::new();
    let mut launch_cost: Vec<VarDictionary> = Vec::new();
    if loadout >= 0 && !resume {
        launcher_id = planet_sector_launch_from(dialog, st, planet_name, sector_id);
        let plan = planet_launch_plan(dialog, planet_name, launcher_id, sector_id, loadout);
        launch_resources = row_dict(&plan, "resources");
        if launch_resources.is_empty() {
            launch_resources = planet_launch_resource_totals(dialog);
        }
        launch_row = planet_find_loadout_row(dialog, st, loadout);
        launch_cost = planet_loadout_launch_stacks(&launch_row, &launch_resources);
    }

    // `PlanetDialog.playSelected`'s "save before launch"
    // (`PlanetDialog.java:1380-1386`): write the sector being left BEFORE
    // `MindSimHost.load_sector` replaces the live world, so a switch keeps
    // everything built since that sector's last autosave (CG-SECTOR-02).
    // No-op outside a live match.
    if let Some(campaign) = campaign.as_mut()
        && campaign.has_method("save_outgoing_sector")
    {
        let _ = campaign.call("save_outgoing_sector", &[]);
    }

    // H05: stage the launcher's items before the world load.
    if let Some(mut host) = kit::sim_host() {
        if host.has_method("set_pending_launch") {
            if loadout >= 0 && !resume {
                let stacks =
                    planet_stacks_json(&planet_stacks_of(&planet_dict_totals(&launch_resources)));
                let _ = host.call(
                    "set_pending_launch",
                    &[
                        loadout.to_variant(),
                        GString::from(stacks.as_str()).to_variant(),
                    ],
                );
            } else {
                let _ = host.call(
                    "set_pending_launch",
                    &[(-1i64).to_variant(), GString::from("[]").to_variant()],
                );
            }
        }
        if host.has_method("load_sector") {
            let _ = host.call(
                "load_sector",
                &[
                    GString::from(planet_name).to_variant(),
                    sector_id.to_variant(),
                ],
            );
        }
    }

    let mut started = false;
    if let Some(campaign) = campaign.as_mut() {
        if resume && campaign.has_method("play_sector") {
            started = campaign
                .call(
                    "play_sector",
                    &[
                        GString::from(planet_name).to_variant(),
                        sector_id.to_variant(),
                    ],
                )
                .try_to::<bool>()
                .unwrap_or(false);
        } else if loadout >= 0 && campaign.has_method("start_sector_with_loadout") {
            if launcher_id >= 0 {
                let _ = campaign.call(
                    "remove_sector_items",
                    &[
                        GString::from(planet_name).to_variant(),
                        launcher_id.to_variant(),
                        GString::from(planet_stacks_json(&launch_cost).as_str()).to_variant(),
                    ],
                );
            }
            let core_stacks = planet_stacks_of(&planet_dict_totals(&launch_resources));
            let mut payload = VarDictionary::new();
            payload.set(
                "core",
                &GString::from(row_str(&launch_row, "core").as_str()),
            );
            payload.set("schematic", loadout);
            payload.set("stacks", &planet_stacks_array(&core_stacks).to_variant());
            payload.set("origin", launcher_id);
            started = campaign
                .call(
                    "start_sector_with_loadout",
                    &[
                        GString::from(planet_name).to_variant(),
                        sector_id.to_variant(),
                        GString::from(json_stringify(&payload).as_str()).to_variant(),
                    ],
                )
                .try_to::<bool>()
                .unwrap_or(false);
        } else if campaign.has_method("start_sector") {
            if campaign.has_method("clear_loadout_info") {
                let _ = campaign.call("clear_loadout_info", &[]);
            }
            started = campaign
                .call(
                    "start_sector",
                    &[
                        GString::from(planet_name).to_variant(),
                        sector_id.to_variant(),
                    ],
                )
                .try_to::<bool>()
                .unwrap_or(false);
        }
    }
    // `MindUi.close_dialog` defers the hide, but the dialog's own `hidden()`
    // can still run synchronously when the page was not opened through the UI
    // shell; the launch callback holds the state borrow until it returns, so
    // close on the next frame instead of re-entering the borrow here.
    let target = dialog.to_gd();
    st.deferred.push(move || {
        let mut bound = target.clone();
        let mut bound = bound.bind_mut();
        bound.close();
    });
    if started {
        planet_enter_game(st);
    }
}

/// The item->amount totals of a `{item: amount}` dictionary.
fn planet_dict_totals(dict: &VarDictionary) -> Vec<(String, i64)> {
    let mut out = Vec::new();
    for key in dict.keys_array().iter_shared() {
        let item = key
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        let amount = dict
            .get(item.as_str())
            .and_then(|value| value.try_to::<i64>().ok())
            .unwrap_or(0);
        out.push((item, amount));
    }
    out
}

/// Hides the menu and shows the in-game HUD group (`MindUiRoot`).
fn planet_enter_game(st: &mut PlanetState) {
    for (path, visible) in st.saved_layers.iter_mut() {
        if path == MENU_GROUP_PATH || path == MENU_BACKGROUND_PATH {
            *visible = false;
        } else if path == HUD_GROUP_PATH {
            *visible = true;
        }
    }
    if let Some(mut root) = ui_root()
        && root.has_method("set_menu_visible")
    {
        let _ = root.call("set_menu_visible", &[false.to_variant()]);
    }
}

// ---- stage cover/restore --------------------------------------------------

/// `_set_planet_view_active`: globe layer + the dialog's own backdrop/dim.
fn planet_set_planet_view_active(dialog: &mut MindDialog, st: &mut PlanetState, active: bool) {
    if let Some(mut view) = planet_view() {
        planet_view_call(&mut view, "set_active", &[active.to_variant()]);
    }
    if let Some(backdrop) = dialog.base().get_node_or_null("SpaceBackdrop")
        && let Ok(mut backdrop) = backdrop.try_cast::<CanvasItem>()
    {
        backdrop.set_visible(!active);
    }
    if let Some(dim) = dialog.base().get_node_or_null("Dim")
        && let Ok(mut dim) = dim.try_cast::<CanvasItem>()
    {
        dim.set_visible(!active);
    }
    if !dialog.base().is_visible() {
        return;
    }
    if active || !st.saved_layers.is_empty() || planet_hud_visible() {
        planet_cover_stage(st);
    } else {
        planet_set_visible_at(MENU_GROUP_PATH, true);
        planet_set_visible_at(MENU_BACKGROUND_PATH, true);
    }
}

fn planet_hud_visible() -> bool {
    let Some(hud) = kit::node(HUD_GROUP_PATH) else {
        return false;
    };
    hud.clone()
        .try_cast::<CanvasItem>()
        .map(|hud| hud.is_visible())
        .unwrap_or(false)
}

/// Captures (once) and hides every stage layer under the dialog.
fn planet_cover_stage(st: &mut PlanetState) {
    if st.saved_layers.is_empty() {
        let mut paths: Vec<String> = vec![
            String::from(MENU_GROUP_PATH),
            String::from(MENU_BACKGROUND_PATH),
            String::from(HUD_GROUP_PATH),
            String::from(WORLD_PATH),
        ];
        if let Some(world) = kit::node(WORLD_PATH) {
            let layers = world
                .find_children_ex("*")
                .type_("CanvasLayer")
                .recursive(true)
                .owned(false)
                .done();
            for layer in layers.iter_shared() {
                paths.push(layer.clone().get_path().to_string());
            }
        }
        for path in paths {
            let Some(node) = kit::node(&path) else {
                continue;
            };
            if let Ok(canvas) = node.clone().try_cast::<CanvasItem>() {
                st.saved_layers.push((path, canvas.is_visible()));
            } else if let Ok(layer) = node.clone().try_cast::<godot::classes::CanvasLayer>() {
                st.saved_layers.push((path, layer.is_visible()));
            }
        }
    }
    let saved: Vec<String> = st
        .saved_layers
        .iter()
        .map(|(path, _)| path.clone())
        .collect();
    for path in saved {
        planet_set_visible_at(&path, false);
    }
}

/// Restores exactly the captured visibility (no-op when nothing was covered).
fn planet_restore_stage(st: &mut PlanetState) {
    let saved = st.saved_layers.clone();
    for (path, visible) in saved {
        planet_set_visible_at(&path, visible);
    }
    st.saved_layers.clear();
}

fn planet_set_visible_at(path: &str, value: bool) {
    let Some(node) = kit::node(path) else {
        return;
    };
    if let Ok(mut canvas) = node.clone().try_cast::<CanvasItem>() {
        canvas.set_visible(value);
    } else if let Ok(mut layer) = node.clone().try_cast::<godot::classes::CanvasLayer>() {
        layer.set_visible(value);
    }
}

// ---- reveal loop ----------------------------------------------------------

/// `_start_preset_reveal`: zoom reset, camera to the last sector, new presets.
fn planet_start_preset_reveal(dialog: &mut MindDialog, st: &mut PlanetState) {
    st.new_presets.clear();
    st.preset_show = 0.0;
    st.reveal_showed = false;
    let planet = st.selected_planet.clone();
    if planet.is_empty() {
        return;
    }
    let Some(mut view) = planet_view() else {
        return;
    };
    if view.has_method("set") {
        view.set("zoom", &1.0f32.to_variant());
        view.set("zoom_target", &1.0f32.to_variant());
    }
    let data = planet_view_data(&mut view);
    st.last_sector_id = row_i64_or(&data, "last_sector", -1);
    let last_sector = planet_find_sector(dialog, st, &planet, st.last_sector_id);
    if !last_sector.is_empty() {
        let tile = planet_view_tile(&mut view, &last_sector);
        planet_view_call(&mut view, "look_at_tile", &[tile.to_variant()]);
    }
    let Some(mut campaign) = dialog.campaign_node() else {
        return;
    };
    if !campaign.has_method("take_new_presets") {
        return;
    }
    let collected = campaign
        .call(
            "take_new_presets",
            &[GString::from(planet.as_str()).to_variant()],
        )
        .to::<PackedInt32Array>();
    for id in collected.as_slice() {
        st.new_presets.push(*id as i64);
    }
    if !st.new_presets.is_empty() {
        st.new_presets.push(st.last_sector_id);
        st.new_presets.reverse();
    }
}

/// `_process`: the `newPresets` reveal loop (`PlanetDialog.java:986-1003`).
fn planet_process(dialog: &mut MindDialog, state: &Rc<RefCell<PlanetState>>) {
    let mut st = state.borrow_mut();
    if st.new_presets.is_empty() || !dialog.base().is_visible() {
        return;
    }
    let Some(mut view) = planet_view() else {
        return;
    };
    let now = godot::classes::Time::singleton().get_ticks_msec();
    let delta = if st.last_tick_ms == 0 {
        1.0 / 60.0
    } else {
        ((now.saturating_sub(st.last_tick_ms)) as f32 / 1000.0).clamp(0.0, 0.25)
    };
    st.last_tick_ms = now;
    st.preset_show += delta * 60.0;
    let to_id = st.new_presets.last().copied().unwrap_or(-1);
    let planet = st.selected_planet.clone();
    let sector = planet_find_sector(dialog, &mut st, &planet, to_id);
    if !sector.is_empty() {
        let tile = planet_view_tile(&mut view, &sector);
        planet_view_call(
            &mut view,
            "look_at_tile",
            &[tile.to_variant(), 0.11f32.to_variant()],
        );
    }
    if view.has_method("set") {
        view.set("zoom_target", &0.75f32.to_variant());
    }
    if st.preset_show >= 20.0 && !st.reveal_showed && st.new_presets.len() > 1 {
        st.reveal_showed = true;
        let text = format!(
            "{} [accent]{}",
            kit::glyph("lock-open"),
            planet_localized_sector(&sector)
        );
        let _ = ui_call(
            "announce",
            &[
                GString::from(text.as_str()).to_variant(),
                2.0f32.to_variant(),
            ],
        );
    }
    if st.preset_show > 144.0 {
        st.new_presets.pop();
        st.reveal_showed = false;
        st.preset_show = 0.0;
    }
}

// ---- globe input ----------------------------------------------------------

/// Left-drag orbits, the wheel zooms, a tap selects (`PlanetDialog.java:114`).
fn planet_input(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    event: Gd<godot::classes::InputEvent>,
) {
    if !dialog.base().is_visible() || !dialog.is_top() {
        return;
    }
    let mut st = state.borrow_mut();
    if let Ok(key) = event.clone().try_cast::<godot::classes::InputEventKey>() {
        planet_handle_reveal_escape(dialog, &mut st, &key);
        return;
    }
    if let Ok(motion) = event
        .clone()
        .try_cast::<godot::classes::InputEventMouseMotion>()
    {
        let Some(mut view) = planet_view() else {
            return;
        };
        if !view
            .clone()
            .try_cast::<CanvasItem>()
            .map(|v| v.is_visible())
            .unwrap_or(false)
        {
            return;
        }
        let mask = motion.get_button_mask();
        if mask.ord() & godot::global::MouseButtonMask::LEFT.ord() != 0 {
            if !st.press_on_globe {
                return;
            }
            if !st.new_presets.is_empty()
                && st.new_presets.last().copied() != Some(st.last_sector_id)
            {
                return;
            }
            st.new_presets.clear();
            st.dragged = true;
            planet_view_call(&mut view, "drag", &[motion.get_relative().to_variant()]);
            if let Some(mut viewport) = dialog.base().get_viewport() {
                viewport.set_input_as_handled();
            }
            return;
        }
        planet_update_hover(dialog, &mut st, &mut view, motion.get_position());
        return;
    }
    let Ok(click) = event.try_cast::<godot::classes::InputEventMouseButton>() else {
        return;
    };
    let button = click.get_button_index();
    if button == godot::global::MouseButton::WHEEL_UP
        || button == godot::global::MouseButton::WHEEL_DOWN
    {
        let Some(mut view) = planet_view() else {
            return;
        };
        if !view
            .clone()
            .try_cast::<CanvasItem>()
            .map(|v| v.is_visible())
            .unwrap_or(false)
        {
            return;
        }
        if planet_over_ui(dialog, &st, click.get_position()) {
            return;
        }
        if !click.is_pressed() {
            if let Some(mut viewport) = dialog.base().get_viewport() {
                viewport.set_input_as_handled();
            }
            return;
        }
        let amount = if button == godot::global::MouseButton::WHEEL_UP {
            -1.0f32
        } else {
            1.0f32
        };
        planet_view_call(&mut view, "scroll", &[amount.to_variant()]);
        if let Some(mut viewport) = dialog.base().get_viewport() {
            viewport.set_input_as_handled();
        }
        return;
    }
    if button != godot::global::MouseButton::LEFT {
        return;
    }
    let Some(mut view) = planet_view() else {
        return;
    };
    if !view
        .clone()
        .try_cast::<CanvasItem>()
        .map(|v| v.is_visible())
        .unwrap_or(false)
    {
        return;
    }
    if click.is_pressed() {
        st.pressed_position = click.get_position();
        st.dragged = false;
        st.press_on_globe = !planet_over_ui(dialog, &st, click.get_position());
        return;
    }
    if st.dragged {
        st.dragged = false;
        st.press_on_globe = false;
        return;
    }
    if !st.press_on_globe {
        return;
    }
    st.press_on_globe = false;
    if click.get_position().distance_to(st.pressed_position) > 4.0 {
        return;
    }
    if planet_over_ui(dialog, &st, click.get_position()) {
        return;
    }
    let tile = planet_view_pick(&mut view, click.get_position());
    if tile < 0 {
        return;
    }
    planet_select_view_tile(dialog, state, &mut st, tile);
    planet_update_hover(dialog, &mut st, &mut view, click.get_position());
    if let Some(mut viewport) = dialog.base().get_viewport() {
        viewport.set_input_as_handled();
    }
}

/// Escape during the reveal keeps only the first entry (`PlanetDialog.java:92`).
fn planet_handle_reveal_escape(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    key: &Gd<godot::classes::InputEventKey>,
) {
    if !key.is_pressed() || key.is_echo() || key.get_keycode() != godot::global::Key::ESCAPE {
        return;
    }
    if st.new_presets.len() <= 1 {
        return;
    }
    st.new_presets.truncate(1);
    st.preset_show = 0.0;
    st.reveal_showed = false;
    if let Some(mut viewport) = dialog.base().get_viewport() {
        viewport.set_input_as_handled();
    }
}

/// `_select_view_tile`: selects the campaign row whose grid tile matches.
fn planet_select_view_tile(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<PlanetState>>,
    st: &mut PlanetState,
    tile: i64,
) {
    let planet = st.selected_planet.clone();
    let sectors = planet_sectors(dialog, st, &planet);
    let selection_key = if st.launch_mode == "planetLaunch" {
        "accelerator_selectable"
    } else {
        "selectable"
    };
    for sector in sectors {
        let Some(mut view) = planet_view() else {
            return;
        };
        if planet_view_tile(&mut view, &sector) != tile {
            continue;
        }
        if !row_bool(&sector, selection_key) {
            return;
        }
        st.selected_sector = row_i64(&sector, "id");
        planet_show_planet(dialog, state, st, &planet, true);
        return;
    }
}

/// `_update_hover`: fades the grid in around the sector under the cursor.
fn planet_update_hover(
    dialog: &mut MindDialog,
    st: &mut PlanetState,
    view: &mut Gd<Node>,
    point: Vector2,
) {
    let mut tile = -1i64;
    if !planet_over_ui(dialog, st, point) {
        tile = planet_view_pick(view, point);
    }
    if tile == planet_view_hover(view) {
        return;
    }
    let mut name_text = String::new();
    if tile >= 0 {
        let planet = st.selected_planet.clone();
        for sector in planet_sectors(dialog, st, &planet) {
            let sector_tile = planet_view_tile(view, &sector);
            if sector_tile == tile && !row_bool(&sector, "locked") {
                name_text = planet_localized_sector(&sector);
                break;
            }
        }
    }
    planet_view_call(
        view,
        "set_sector_label",
        &[GString::from(name_text.as_str()).to_variant()],
    );
    planet_view_call(view, "set_hover", &[tile.to_variant()]);
}

/// `_over_ui`: the rail, the sector list and the button row are not the globe.
fn planet_over_ui(dialog: &mut MindDialog, st: &PlanetState, point: Vector2) -> bool {
    if let Some(root) = st.root.clone()
        && root.get_global_rect().contains_point(point)
    {
        return true;
    }
    if let Some(list) = st.sector_list.clone()
        && list.get_global_rect().contains_point(point)
    {
        return true;
    }
    if let Some(buttons) = dialog.buttons_row()
        && buttons.get_global_rect().contains_point(point)
    {
        return true;
    }
    false
}
