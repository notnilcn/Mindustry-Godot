// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/ColorPicker.java,
//         core/src/mindustry/ui/dialogs/ModsDialog.java,
//         core/src/mindustry/ui/dialogs/ModBrowserDialog.java.

//! Dialog controller batch g: `picker`, `mods` and `mod_browser`.
//!
//! | registry id   | controller             | upstream Java           | GDScript                           |
//! |---------------|------------------------|-------------------------|------------------------------------|
//! | `picker`      | [`ColorPickerDialog`]  | `ColorPicker.java`      | `ui/dialogs/color_picker.gd`       |
//! | `mods`        | [`ModsDialog`]         | `ModsDialog.java`       | `ui/dialogs/mods_dialog.gd`        |
//! | `mod_browser` | [`ModBrowserDialog`]   | `ModBrowserDialog.java` | `ui/dialogs/mod_browser_dialog.gd` |
//!
//! `picker` is the `ColorPicker.show(color, alpha, consumer)` seam. The
//! GDScript took a `Callable` consumer and invoked it after emitting
//! `color_selected`; a `Callable` handed to a native controller has no stable
//! lifetime across the GDExtension boundary, so the consumer is dropped: the
//! controller remembers the confirmed colour in [`ColorPickerDialog`]'s
//! `chosen` state and only emits `color_selected(color)`. Callers seed the
//! dialog through the `MindUi.open_dialog` context (`{"color": "#rrggbbaa",
//! "alpha": true}`); without context the controller keeps its previous state,
//! exactly like the GDScript node did between opens. Both context keys are
//! optional.
//!
//! `mods`/`mod_browser` keep every mod read/write on the `MindMods` autoload
//! (`list`/`details`/`errors`/`set_enabled`/`import_mod`/`remove_mod`/
//! `mods_dir`/`requires_reload`/`parse_listing`/`matching_release`/
//! `game_build`/`game_revision` and the `mods_changed`/`mod_error`/
//! `mod_import_progress` signals). Two former cross-dialog node methods have no
//! native `MindDialog` equivalent (this batch must not extend the dialog ABI):
//!
//! * `mods.set_updates(updates)` — the browser writes the
//!   `internal name -> remote version` dictionary to the mods dialog node's
//!   [`MODS_UPDATES_META`] metadata and re-runs its controller; the mods
//!   controller reads that metadata at the start of every `shown()`.
//! * `mod_browser.install_repos(items)` — the mods dialog writes the batch into
//!   the browser node's [`BROWSER_INSTALL_META`] metadata and re-runs its
//!   controller; the browser drains that queue in `shown()` before fetching,
//!   which is where upstream's `install_repos` starts downloading.
//!
//! URL/folder opening goes through `platform::uri::open_uri`
//! (`Core.app.openURI`/`openFolder`), the `.zip`/`.jar` chooser through
//! `platform::dialogs::show_file_dialog` (`FileChooser.open`). The browser's
//! GitHub HTTP flow (listings, repository, release, zipball) uses
//! `HTTPRequest` children; the former `_process` byte-progress poll is a 0.1 s
//! `Timer` because a `DialogController` has no per-frame hook.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::control::{FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    BoxContainer, Button, CanvasItem, CenterContainer, ColorRect, Control, DirAccess, FileAccess,
    HBoxContainer, HSlider, HttpRequest, Json, Label, LineEdit, MarginContainer, Node,
    PanelContainer, ProgressBar, ProjectSettings, ScrollContainer, StyleBoxEmpty, StyleBoxFlat,
    TextureRect, Timer, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::obj::{EngineEnum, NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::border_image::MindBorderImage;
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindScroll, MindTable, styles};

/// The controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "picker" => Some(Box::new(ColorPickerDialog::default())),
        "mods" => Some(Box::new(ModsDialog::default())),
        "mod_browser" => Some(Box::new(ModBrowserDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

/// A queued one-shot action (`Callable.call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase.
///
/// Every controller in this batch keeps its state in an `Rc<RefCell<..>>` that
/// its own buttons and lasting signal handlers share. A `MindMods`/`MindUi`
/// call made while that state is borrowed can re-enter a handler synchronously
/// (`mods_changed`, `mod_import_progress`, `confirm_result`), so the handlers
/// queue their work here instead of borrowing recursively.
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

/// Calls one `MindUi` method (`None` when the facade or method is absent).
fn ui_call(method: &str, args: &[Variant]) -> Option<Variant> {
    let mut ui = kit::ui()?;
    if !ui.has_method(method) {
        return None;
    }
    Some(ui.call(method, args))
}

/// Opens a registered dialog through `MindUi`.
fn open_dialog(name: &str, context_json: &str) {
    let _ = ui_call(
        "open_dialog",
        &[
            GString::from(name).to_variant(),
            GString::from(context_json).to_variant(),
        ],
    );
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

/// The `MindMods` autoload.
fn mods_node() -> Option<Gd<Node>> {
    kit::node("/root/MindMods")
}

/// Re-runs a dialog controller after the current callback unwinds
/// (`set_updates`/`install_repos` replacement; see the module docs).
fn refresh_dialog_deferred(node: &Gd<Node>) {
    let node = node.clone();
    defer_action("batch_g_refresh_dialog", move || {
        if !node.is_instance_valid() {
            return;
        }
        let mut node = node;
        let _ = node.call("refresh_controller", &[]);
    });
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

/// `int(value)` for one variant.
fn variant_i64(value: &Variant) -> i64 {
    if let Ok(number) = value.try_to::<i64>() {
        return number;
    }
    if let Ok(number) = value.try_to::<f64>() {
        return number as i64;
    }
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string().trim().parse().unwrap_or(0);
    }
    0
}

/// `bool(value)` for one variant.
fn variant_bool(value: &Variant) -> bool {
    if let Ok(flag) = value.try_to::<bool>() {
        return flag;
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number != 0;
    }
    false
}

/// A dictionary value coerced to text (`str(dict.get(key, ""))`).
fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .map(|value| variant_string(&value))
        .unwrap_or_default()
}

/// A dictionary value coerced to a boolean.
fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .map(|value| variant_bool(&value))
        .unwrap_or(default)
}

/// A dictionary value coerced to an integer.
fn dict_i64(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(default)
}

/// `PackedStringArray(value)` for a JSON array (`[]` on any other shape).
fn string_list(value: Option<&Variant>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Ok(array) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    array
        .iter_shared()
        .map(|item| variant_string(&item))
        .collect()
}

/// `Core.bundle.get(key)` as plain text.
fn bt(key: &str) -> String {
    kit::t(key).to_string()
}

/// `MindWidgets.markup(key)`: the bundle string translated to BBCode.
fn bm(key: &str) -> String {
    kit::tm(key).to_string()
}

/// `Bundle.format` with `{0}`-style substitution (`_fmt`).
fn fmt_key(key: &str, args: &[&str]) -> String {
    let mut result = bt(key);
    for (index, arg) in args.iter().enumerate() {
        result = result.replace(&format!("{{{index}}}"), arg);
    }
    result
}

/// `Strings.stripColors`: drops `[color]` / `[#rrggbb]` / `[]` markup tags.
///
/// The GDScript regex was `\[(#?[0-9a-fA-F]{6,8}|[a-zA-Z_]*)\]`; this scanner
/// accepts exactly the same tag bodies.
fn strip_colors(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    while index < characters.len() {
        if characters[index] != '[' {
            out.push(characters[index]);
            index += 1;
            continue;
        }
        let Some(close) = characters[index + 1..].iter().position(|c| *c == ']') else {
            out.push(characters[index]);
            index += 1;
            continue;
        };
        let body: String = characters[index + 1..index + 1 + close].iter().collect();
        if is_color_tag(&body) {
            index += close + 2;
            continue;
        }
        out.push(characters[index]);
        index += 1;
    }
    out
}

/// One `[…]` tag body accepted by [`strip_colors`]: `#` plus 6/8 hex digits,
/// or a plain identifier (`[]` included).
fn is_color_tag(body: &str) -> bool {
    if let Some(hex) = body.strip_prefix('#') {
        return (hex.len() == 6 || hex.len() == 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    if body.len() == 6 || body.len() == 8 {
        let hex = body.chars().all(|c| c.is_ascii_hexdigit());
        let name = body.chars().all(|c| c.is_ascii_alphabetic() || c == '_');
        return hex || name;
    }
    body.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
}

/// A solid `StyleBoxFlat` (`_flat`).
fn flat_style(color: Color) -> Gd<StyleBoxFlat> {
    // code-instantiated: the gray/flat card looks are built per data row.
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(color);
    box_
}

/// Local copy of `MindDialog::build_button_content` (the shell helper is a
/// `&mut self` method; the data-row rebuild paths hold no dialog borrow).
fn build_button_content(button: &mut Gd<Button>, icon: Option<Gd<Control>>, text: &str) {
    // code-instantiated: button content rows are data-driven per dialog.
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(godot::classes::box_container::AlignmentMode::BEGIN);
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 13.0);
    row.set_offset(Side::RIGHT, -8.0);
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);
    if let Some(icon) = icon {
        row.add_child(&icon);
    }
    let mut label = factory::label(text);
    label.set_autowrap_mode(AutowrapMode::OFF);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&label);
}

/// Local copy of `MindDialog::apply_pane_button` (`Styles.flatTogglet`).
fn apply_pane_button(button: &mut Gd<Button>, border: Color, bottom_border: f32) {
    let normal = MindDialog::pane_box(Color::BLACK, border, 4.0, 2.0, bottom_border);
    button.add_theme_stylebox_override("normal", &normal);
    button.add_theme_stylebox_override("hover", &normal);
    button.add_theme_stylebox_override("disabled", &normal);
    let pressed = MindDialog::pane_box(Color::BLACK, border, 4.0, 2.0, -1.0);
    button.add_theme_stylebox_override("pressed", &pressed);
}

// ---------------------------------------------------------------------------
// ColorPickerDialog (`picker`, `ColorPicker.java`)
// ---------------------------------------------------------------------------

/// `const ROW_SIZE := Vector2(370, 44)` (slider row).
const PICKER_ROW_SIZE: Vector2 = Vector2::new(370.0, 44.0);
/// `ColorPicker.java:27-170` preview square (`t.stack(...).size(200f)`).
const PICKER_PREVIEW_SIZE: f32 = 200.0;
/// `Tex.pane` pads its content 12 px (java/picker.png: frame 228 px around the
/// 200 px preview).
const PICKER_FRAME_PAD: f32 = 12.0;
/// Hex field size (`t.field(...).size(130f, 40f)`).
const PICKER_HEX_SIZE: Vector2 = Vector2::new(130.0, 40.0);
/// Hue gradient steps (the 36-quad approximation of `Tex.hue`).
const PICKER_HUE_STEPS: usize = 36;

/// The picker's HSV(A) state, preview/slider handles and the confirmed colour.
struct PickerState {
    /// Hue in degrees (`value.h * 360.0`).
    h: f32,
    s: f32,
    /// Value starts at 1 (`ColorPicker` opens on white).
    v: f32,
    a: f32,
    /// Whether the alpha row is shown (`ColorPicker.show(color, alpha, …)`).
    alpha: bool,
    /// `current` — the live colour.
    current: Color,
    /// The last confirmed colour (native replacement for the `consumer`).
    chosen: Color,
    preview: Option<Gd<ColorRect>>,
    hex: Option<Gd<LineEdit>>,
    sliders: Vec<(&'static str, Gd<HSlider>)>,
    gradients: Vec<Gd<Control>>,
}

impl Default for PickerState {
    fn default() -> Self {
        Self {
            h: 0.0,
            s: 0.0,
            v: 1.0,
            a: 1.0,
            alpha: true,
            current: Color::WHITE,
            chosen: Color::WHITE,
            preview: None,
            hex: None,
            sliders: Vec::new(),
            gradients: Vec::new(),
        }
    }
}

impl PickerState {
    /// One HSV(A) component read out (`{"h": _h, …}[component]`).
    fn component(&self, component: &str) -> f32 {
        match component {
            "h" => self.h,
            "s" => self.s,
            "v" => self.v,
            _ => self.a,
        }
    }
}

/// `@pickcolor` — the HSV(A) colour picker (`ColorPicker.java`).
pub struct ColorPickerDialog {
    state: Rc<RefCell<PickerState>>,
}

impl Default for ColorPickerDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(PickerState::default())),
        }
    }
}

impl DialogController for ColorPickerDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@pickcolor");
        // `should_pause = false` before `super._ready()`.
        dialog.should_pause = false;
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // The last confirmed colour is kept for the caller-less seam: the
        // consumer `Callable` is gone, so `color_selected` carries the result.
        log::debug!(
            "[picker] shown with last confirmed colour {}",
            arc_hex(self.state.borrow().chosen)
        );
        // `show_picker(color, alpha, consumer)` has no native equivalent; the
        // optional context keys carry the same seed values (see module docs).
        if let Some(value) = dialog.context_value("color") {
            let text = variant_string(&value);
            if let Some(color) = parse_html_color(&text) {
                self.set_color(color);
            }
        }
        if let Some(value) = dialog.context_value("alpha") {
            self.state.borrow_mut().alpha = variant_bool(&value);
        }
        build_picker(dialog, &self.state);
    }
}

impl ColorPickerDialog {
    /// `ColorPicker.show(color, …)`: seeds the HSV(A) state directly (the
    /// GDScript `show_picker` entry point; no scene calls it any more, the
    /// dialog context is the native seeding seam).
    fn set_color(&self, color: Color) {
        set_picker_color(&mut self.state.borrow_mut(), color);
    }
}

/// `Color.html(text)` (`#rrggbb` / `#rrggbbaa`); `None` when invalid.
fn parse_html_color(text: &str) -> Option<Color> {
    let trimmed = text.trim();
    let body = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if body.len() != 6 && body.len() != 8 {
        return None;
    }
    Color::from_html(body)
}

/// `set_color(value)`: `current.set(color)` + `toHsv`.
fn set_picker_color(state: &mut PickerState, color: Color) {
    state.current = color;
    match color.normalized().try_to_hsv() {
        Ok(hsv) => {
            state.h = hsv.h * 360.0;
            state.s = hsv.s;
            state.v = hsv.v;
            state.a = hsv.a;
        }
        Err(error) => {
            log::warn!("[picker] HSV conversion failed: {error}");
            state.h = 0.0;
            state.s = 0.0;
            state.v = 1.0;
            state.a = color.a;
        }
    }
}

/// `ColorPicker._build()`.
fn build_picker(dialog: &mut MindDialog, state: &Rc<RefCell<PickerState>>) {
    dialog.clear_content();
    let mut table = dialog.content_table();
    let alpha = state.borrow().alpha;

    // `t.table(Tex.pane, i -> i.stack(alphaBg, colorImage).size(200f))
    // .colspan(2).padBottom(5)`.
    // code-instantiated: the preview frame is rebuilt per open.
    let mut frame = PanelContainer::new_alloc();
    let mut box_ = MindDialog::pane_box(
        Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        styles::PANE_OUTLINE_GRAY,
        4.0,
        2.0,
        -1.0,
    );
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        box_.set_content_margin(side, PICKER_FRAME_PAD);
    }
    frame.add_theme_stylebox_override("panel", &box_);
    // code-instantiated: the alpha backdrop + live colour square stack.
    let mut stack = Control::new_alloc();
    stack.set_custom_minimum_size(Vector2::new(PICKER_PREVIEW_SIZE, PICKER_PREVIEW_SIZE));
    stack.add_child(&picker_alpha_bg("alpha-bg"));
    // code-instantiated: the preview rect is tinted on every change.
    let mut preview = ColorRect::new_alloc();
    preview.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    preview.set_mouse_filter(MouseFilter::IGNORE);
    stack.add_child(&preview);
    frame.add_child(&stack);
    {
        let mut bound = table.bind_mut();
        bound.add(&frame).pad_bottom(5.0);
        bound.row();
    }
    {
        let mut st = state.borrow_mut();
        st.preview = Some(preview);
        st.sliders.clear();
        st.gradients.clear();
    }

    for (component, maximum) in [("h", 360.0_f64), ("s", 1.0), ("v", 1.0)] {
        let value = f64::from(state.borrow().component(component));
        let row = picker_slider_row(state, component, maximum, value, None);
        let mut bound = table.bind_mut();
        bound
            .add(&row)
            .width(PICKER_ROW_SIZE.x)
            .height(PICKER_ROW_SIZE.y)
            .pad_bottom(6.0);
        bound.row();
    }
    if alpha {
        let value = f64::from(state.borrow().a);
        let row = picker_slider_row(state, "a", 1.0, value, Some("alpha-bg-line"));
        let mut bound = table.bind_mut();
        bound
            .add(&row)
            .width(PICKER_ROW_SIZE.x)
            .height(PICKER_ROW_SIZE.y)
            .pad_bottom(6.0);
        bound.row();
    }

    // `hexField = t.field(current.toString(), …).size(130f, 40f)` with
    // `valid(Color.valueOf)`.
    // code-instantiated: the hex field mirrors the HSV state both ways.
    let mut hex = factory::field("");
    hex.set_custom_minimum_size(PICKER_HEX_SIZE);
    {
        let state_for_hex = state.clone();
        let hex_callable = Callable::from_fn("picker_hex", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| variant_string(value))
                .unwrap_or_default();
            {
                let mut st = state_for_hex.borrow_mut();
                picker_on_hex(&mut st, &text);
            }
            Variant::nil()
        });
        hex.connect("text_changed", &hex_callable);
    }
    {
        let mut bound = table.bind_mut();
        bound
            .add(&hex)
            .width(PICKER_HEX_SIZE.x)
            .height(PICKER_HEX_SIZE.y);
    }
    state.borrow_mut().hex = Some(hex);

    dialog.clear_buttons();
    dialog.add_close_button(210.0);
    // code-instantiated: `@ok` (`Icon.ok`) applies the picked colour.
    let mut ok = factory::icon_button(
        "ok",
        &bt("@ok"),
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    ok.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    {
        let target = dialog.to_gd();
        let state_for_ok = state.clone();
        let ok_callable = Callable::from_fn("picker_ok", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return Variant::nil();
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            picker_confirm(&mut bound, &state_for_ok);
            Variant::nil()
        });
        ok.connect("pressed", &ok_callable);
    }
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&ok);
    }
    picker_update_color(&mut state.borrow_mut(), true);
}

/// `t.stack(<gradient>, slider)`: the gradient fills the 370x44 row and the
/// `defaultSlider` (transparent `sliderBack` body) draws over it.
fn picker_slider_row(
    state: &Rc<RefCell<PickerState>>,
    component: &'static str,
    maximum: f64,
    value: f64,
    backdrop: Option<&str>,
) -> Gd<Control> {
    // code-instantiated: one stacked slider row per HSV(A) component.
    let mut row = Control::new_alloc();
    row.set_custom_minimum_size(PICKER_ROW_SIZE);
    if let Some(region) = backdrop {
        row.add_child(&picker_alpha_bg(region));
    }
    // code-instantiated: the gradient repaints with the current HSV(A) state.
    let mut gradient = Control::new_alloc();
    gradient.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    gradient.set_mouse_filter(MouseFilter::IGNORE);
    {
        let state_for_draw = state.clone();
        let gradient_for_draw = gradient.clone();
        let draw = Callable::from_fn("picker_gradient", move |_args: &[&Variant]| {
            let Ok(st) = state_for_draw.try_borrow() else {
                return Variant::nil();
            };
            picker_draw_gradient(&gradient_for_draw, &st, component);
            Variant::nil()
        });
        gradient.connect("draw", &draw);
    }
    row.add_child(&gradient);
    state.borrow_mut().gradients.push(gradient);
    // code-instantiated: the transparent `defaultSlider` over the gradient.
    let mut slider = factory::slider(0.0, maximum, 0.0001);
    slider.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    slider.set_value(value);
    {
        let state_for_change = state.clone();
        let changed = Callable::from_fn("picker_slider", move |args: &[&Variant]| {
            let number = args
                .first()
                .and_then(|value| value.try_to::<f64>().ok())
                .unwrap_or(0.0) as f32;
            let mut st = state_for_change.borrow_mut();
            picker_on_slider(&mut st, number, component);
            Variant::nil()
        });
        slider.connect("value_changed", &changed);
    }
    row.add_child(&slider);
    state.borrow_mut().sliders.push((component, slider));
    row
}

/// `Fill.quad(x, y, first, x + w, y, second, …)`: a left-to-right gradient of
/// the component (hue: the stepped hue ramp).
fn picker_draw_gradient(target: &Gd<Control>, state: &PickerState, component: &str) {
    let size = target.get_size();
    let mut canvas: Gd<CanvasItem> = target.clone().upcast();
    if component == "h" {
        for index in 0..PICKER_HUE_STEPS {
            let x0 = size.x * index as f32 / PICKER_HUE_STEPS as f32;
            let x1 = size.x * (index + 1) as f32 / PICKER_HUE_STEPS as f32;
            let c0 = Color::from_hsv(index as f64 / PICKER_HUE_STEPS as f64, 1.0, 1.0);
            let c1 = Color::from_hsv((index + 1) as f64 / PICKER_HUE_STEPS as f64, 1.0, 1.0);
            picker_quad(&mut canvas, x0, x1, size.y, c0, c1);
        }
        return;
    }
    let (first, second) = match component {
        "s" => (
            Color::from_hsv(f64::from(state.h) / 360.0, 0.0, f64::from(state.v)),
            Color::from_hsv(f64::from(state.h) / 360.0, 1.0, f64::from(state.v)),
        ),
        "v" => (
            Color::from_hsv(f64::from(state.h) / 360.0, f64::from(state.s), 0.0),
            Color::from_hsv(f64::from(state.h) / 360.0, f64::from(state.s), 1.0),
        ),
        _ => (
            Color::from_rgba(state.current.r, state.current.g, state.current.b, 0.0),
            Color::from_rgba(state.current.r, state.current.g, state.current.b, 1.0),
        ),
    };
    // `first.a = first.a if component == "a" else 1.0`; `second.a = 1.0`.
    let first = if component == "a" {
        first
    } else {
        first.with_alpha(1.0)
    };
    picker_quad(
        &mut canvas,
        0.0,
        size.x,
        size.y,
        first,
        second.with_alpha(1.0),
    );
}

/// `target.draw_polygon(quad, PackedColorArray(c0, c1, c1, c0))`.
fn picker_quad(target: &mut Gd<CanvasItem>, x0: f32, x1: f32, h: f32, c0: Color, c1: Color) {
    let points = PackedVector2Array::from(
        &[
            Vector2::new(x0, 0.0),
            Vector2::new(x1, 0.0),
            Vector2::new(x1, h),
            Vector2::new(x0, h),
        ][..],
    );
    let colors = PackedColorArray::from(&[c0, c1, c1, c0][..]);
    target.draw_polygon(&points, &colors);
}

/// `Tex.alphaBg` / `Tex.alphaBgLine`: the checkerboard under translucent
/// colours.
fn picker_alpha_bg(region: &str) -> Gd<TextureRect> {
    // code-instantiated: the checkerboard tile follows the preview/slider size.
    let mut rect = TextureRect::new_alloc();
    rect.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    rect.set_mouse_filter(MouseFilter::IGNORE);
    rect.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    rect.set_stretch_mode(godot::classes::texture_rect::StretchMode::TILE);
    if let Some(texture) = kit::icon_texture(region) {
        rect.set_texture(&texture);
    }
    rect
}

/// `_on_slider(value, component)`.
fn picker_on_slider(state: &mut PickerState, value: f32, component: &str) {
    match component {
        "h" => state.h = value,
        "s" => state.s = value,
        "v" => state.v = value,
        _ => state.a = value,
    }
    picker_update_color(state, true);
}

/// `Color.valueOf(current, value)` on every valid edit; sliders follow.
fn picker_on_hex(state: &mut PickerState, text: &str) {
    let trimmed = text.trim().trim_start_matches('#');
    if trimmed.len() != 6 && trimmed.len() != 8 {
        return;
    }
    if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return;
    }
    let Some(parsed) = Color::from_html(trimmed) else {
        return;
    };
    match parsed.normalized().try_to_hsv() {
        Ok(hsv) => {
            state.h = hsv.h * 360.0;
            state.s = hsv.s;
            state.v = hsv.v;
            state.a = hsv.a;
        }
        Err(error) => {
            log::warn!("[picker] hex HSV conversion failed: {error}");
            return;
        }
    }
    for (component, slider) in state.sliders.clone() {
        if !slider.is_instance_valid() {
            continue;
        }
        let mut slider = slider;
        slider.set_value_no_signal(f64::from(state.component(component)));
    }
    picker_update_color(state, false);
}

/// `updateColor(updateField)`: `current.fromHsv(h, s, v); current.a = a`; the
/// hex text drops the alpha byte while opaque.
fn picker_update_color(state: &mut PickerState, update_field: bool) {
    state.current = Color::from_hsv(
        f64::from(state.h) / 360.0,
        f64::from(state.s),
        f64::from(state.v),
    )
    .with_alpha(state.a);
    if let Some(mut preview) = state.preview.clone()
        && preview.is_instance_valid()
    {
        preview.set_color(state.current);
    }
    for gradient in state.gradients.clone() {
        if gradient.is_instance_valid() {
            let mut gradient = gradient;
            gradient.queue_redraw();
        }
    }
    if update_field
        && let Some(mut hex) = state.hex.clone()
        && hex.is_instance_valid()
    {
        let mut value = arc_hex(state.current);
        if state.current.a >= 0.9999 {
            value.truncate(6);
        }
        hex.set_block_signals(true);
        hex.set_text(&GString::from(value.as_str()));
        hex.set_block_signals(false);
    }
}

/// Arc `Color.toString()` / `rgba8888`: each channel is `(int)(c * 255)`
/// (truncated), so a 0.5 alpha is `7f`, where Godot's `to_html` rounds to `80`.
fn arc_rgba(color: Color) -> i64 {
    let channel = |value: f32| -> i64 { (value.clamp(0.0, 1.0) * 255.0) as i64 };
    (channel(color.r) << 24) | (channel(color.g) << 16) | (channel(color.b) << 8) | channel(color.a)
}

/// Arc `Color.toString()` as eight lowercase hex digits.
fn arc_hex(color: Color) -> String {
    format!("{:08x}", arc_rgba(color))
}

/// `_confirm()`: remember + emit the colour, then close.
fn picker_confirm(dialog: &mut MindDialog, state: &Rc<RefCell<PickerState>>) {
    let color = state.borrow().current;
    state.borrow_mut().chosen = color;
    dialog
        .base_mut()
        .emit_signal("color_selected", &[color.to_variant()]);
    dialog.close();
}

// ---------------------------------------------------------------------------
// ModsDialog (`mods`, `ModsDialog.java`)
// ---------------------------------------------------------------------------

/// Ported from `Vars.modGuideURL` (`Vars.java:83`).
const MOD_GUIDE_URL: &str = "https://mindustrygame.github.io/wiki/modding/1-modding/";
/// `Iconc.file` (`core/assets-raw/fontgen/config.json`, code 61787); not in
/// `MindIcons.CODES`, so the import chooser renders the code directly.
const ICON_FILE_CODE: u32 = 61787;
/// `ModsDialog.setup`: `float h = 110f` (mod row height).
const ROW_HEIGHT: f32 = 110.0;
/// `mods.set_updates(updates)` replacement: browser-published update versions.
const MODS_UPDATES_META: &str = "_mind_mod_updates";
/// Guard so the `MindMods`/`MindUi` signal bindings run once per dialog node.
const MODS_SIGNALS_META: &str = "_mind_mods_signals_bound";
/// Guard for the viewport `size_changed` refresh (`onResize(this::setup)`).
const MODS_RESIZE_META: &str = "_mind_mods_resize_bound";

/// `ModsDialog` view state (`_root`/`_list`/`_pane`/`_search`/…).
#[derive(Default)]
struct ModsState {
    /// `cont` root table (`_root`).
    root: Option<Gd<MindTable>>,
    /// The mod pane's table (`pane[0]`).
    list: Option<Gd<MindTable>>,
    pane: Option<Gd<MindScroll>>,
    search: Option<Gd<LineEdit>>,
    banner: Option<Gd<MindLabel>>,
    errors_button: Option<Gd<Button>>,
    progress: Option<Gd<ProgressBar>>,
    /// `@mods.restore` / `@mods.guide` (the ctor shows one or the other).
    restore_button: Option<Gd<Button>>,
    guide_button: Option<Gd<Button>>,
    /// The dialog's content column (`cont`).
    cont: Option<Gd<BoxContainer>>,
    /// The shell's scrolling body (`body`).
    body: Option<Gd<ScrollContainer>>,
    /// internal name -> remote version from the browser's listings.
    updates: Vec<(String, String)>,
    /// internal name whose details block is expanded (`showMod`).
    open_details: String,
    /// pending async-confirm target (`mods.removeMod`).
    pending_delete: String,
    /// A `@mod.import.github` text prompt of ours is open.
    pending_github: bool,
    /// `boolean anyDisabled` — one per `setup()`, shared by every rebuild.
    any_disabled: bool,
    /// The `@mod.import` chooser overlay.
    import_overlay: Option<Gd<Control>>,
    /// Internal name whose `@mods.viewcontent` grid is expanded
    /// (`ModsDialog.java:461-481` `currentContent`).
    open_content: String,
}

/// `@mods` — the loaded-mod list (`ModsDialog.java`).
#[derive(Default)]
pub struct ModsDialog {
    state: Rc<RefCell<ModsState>>,
}

impl DialogController for ModsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@mods");
        dialog.should_pause = false;
        // `BaseDialog` is `setFillParent(true)`: the content floats over the
        // dimmed stage (no boxed panel), like the other fill-parent dialogs.
        dialog.enable_full_dialog(true);
        {
            let mut st = self.state.borrow_mut();
            st.cont = dialog.content();
            st.body = dialog.body();
        }
        mods_build_shell(dialog, &self.state);
        mods_build_buttons(dialog, &self.state);
        mods_build_import_overlay(dialog, &self.state);
        mods_connect(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.state.borrow_mut().updates = mods_read_updates(&dialog.to_gd());
        mods_refresh(&self.state);
    }

    fn hidden(&mut self, _dialog: &mut MindDialog) {
        mods_hide_import_dialog(&self.state);
    }

    /// The `@mod.import` chooser is its own `BaseDialog` with `closeOnBack`
    /// (`addCloseButton`): Esc closes it, not the mods dialog underneath.
    fn blocks_escape(&self) -> bool {
        let overlay = self.state.borrow().import_overlay.clone();
        if let Some(mut overlay) = overlay
            && overlay.is_instance_valid()
            && overlay.is_visible()
        {
            overlay.set_visible(false);
            return true;
        }
        false
    }
}

/// `_build_shell()`: `cont` centered between the title and the button bar.
fn mods_build_shell(dialog: &mut MindDialog, state: &Rc<RefCell<ModsState>>) {
    // `onResize(this::setup)`: the lasting connections bind once per node
    // (`ready()` runs again after an extension reload).
    let mut node = dialog.to_gd();
    if node.has_meta(MODS_RESIZE_META) {
        return;
    }
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
        let slot = state.clone();
        let resized = Callable::from_fn("mods_body_resized", move |_args: &[&Variant]| {
            let slot = slot.clone();
            defer_action("mods_fit_pane", move || {
                if let Ok(mut st) = slot.try_borrow_mut() {
                    mods_fit_pane(&mut st);
                }
            });
            Variant::nil()
        });
        body.connect("resized", &resized);
    }
    if let Some(mut cont) = dialog.content() {
        cont.set_v_size_flags(SizeFlags::EXPAND_FILL);
        cont.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    }
    let Some(mut viewport) = dialog.base().get_viewport() else {
        return;
    };
    let target = node.clone();
    let slot = state.clone();
    let resized = Callable::from_fn("mods_on_resize", move |_args: &[&Variant]| {
        if !target.is_instance_valid() || !target.is_visible() {
            return Variant::nil();
        }
        let slot = slot.clone();
        defer_action("mods_resize_refresh", move || mods_refresh(&slot));
        Variant::nil()
    });
    viewport.connect("size_changed", &resized);
    node.set_meta(MODS_RESIZE_META, &true.to_variant());
}

/// Ctor button bar (`ModsDialog.java:52-66`): `addCloseButton()` (210x64),
/// then `@mods.restore` (Icon.power) while any mod is marked failed, else
/// `@mods.guide` (Icon.link), then `@mods.openfolder` (Icon.link).
fn mods_build_buttons(dialog: &mut MindDialog, state: &Rc<RefCell<ModsState>>) {
    let mut back = dialog.add_close_button(210.0);
    back.set_custom_minimum_size(Vector2::new(210.0, 64.0));

    let mut restore = {
        let slot = state.clone();
        let handler = Callable::from_fn("mods_restore", move |_args: &[&Variant]| {
            let slot = slot.clone();
            defer_action("mods_restore_failed", move || mods_restore_failed(&slot));
            Variant::nil()
        });
        mods_add_bar_button(dialog, "@mods.restore", "power", handler)
    };
    let guide = {
        let handler = Callable::from_fn("mods_guide", move |_args: &[&Variant]| {
            mods_open_url(MOD_GUIDE_URL);
            Variant::nil()
        });
        mods_add_bar_button(dialog, "@mods.guide", "link", handler)
    };
    {
        let handler = Callable::from_fn("mods_openfolder", move |_args: &[&Variant]| {
            defer_action("mods_open_folder", mods_open_folder);
            Variant::nil()
        });
        mods_add_bar_button(dialog, "@mods.openfolder", "link", handler);
    }

    restore.set_visible(false);
    let mut st = state.borrow_mut();
    st.restore_button = Some(restore);
    st.guide_button = Some(guide);
}

/// A `buttons.button(text, icon, listener)` cell over the shared button row,
/// drawn like the close button.
fn mods_add_bar_button(
    dialog: &mut MindDialog,
    key: &str,
    icon: &str,
    handler: Callable,
) -> Gd<Button> {
    // code-instantiated: the dialog's own button-bar entries (shared shell row).
    let mut button = factory::button("");
    button.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    apply_pane_button(&mut button, styles::PANE_OUTLINE_GRAY, 4.0);
    let icon_label = factory::glyph_label(icon, 20, Color::WHITE);
    build_button_content(&mut button, Some(icon_label.upcast()), &bt(key));
    button.connect("pressed", &handler);
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&button);
    }
    button
}

/// `@mod.import` (`:145-198`): a `Tex.button` table of two 300x70
/// `Styles.flatt` buttons (`@mod.import.file` / Icon.file,
/// `@mod.import.github` / Icon.github, margin 12) and a closed `@back` bar.
fn mods_build_import_overlay(dialog: &mut MindDialog, state: &Rc<RefCell<ModsState>>) {
    if let Some(mut existing) = dialog.base().get_node_or_null("ImportDialog") {
        existing.queue_free();
    }
    // code-instantiated: the transient import chooser has no manifest entry.
    let mut overlay = Control::new_alloc();
    overlay.set_name("ImportDialog");
    overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    overlay.set_mouse_filter(MouseFilter::STOP);
    overlay.set_visible(false);
    dialog.base_mut().add_child(&overlay);

    // Its own `Styles.defaultDialog` stage background (black9).
    // code-instantiated: the overlay scrim dims the mods dialog beneath.
    let mut dim = ColorRect::new_alloc();
    dim.set_color(styles::BLACK_9);
    dim.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    dim.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&dim);

    // Same insets as the fill-parent dialog shell.
    // code-instantiated: overlay frame insets (8 px sides/bottom).
    let mut frame = MarginContainer::new_alloc();
    frame.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    frame.add_theme_constant_override("margin_left", 8);
    frame.add_theme_constant_override("margin_right", 8);
    frame.add_theme_constant_override("margin_top", 0);
    frame.add_theme_constant_override("margin_bottom", 8);
    frame.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&frame);
    // code-instantiated: the overlay column (title, chooser, back bar).
    let mut layout = VBoxContainer::new_alloc();
    layout.add_theme_constant_override("separation", 8);
    layout.set_mouse_filter(MouseFilter::IGNORE);
    frame.add_child(&layout);

    // Title row + accent rule (`BaseDialog.java:22-25`).
    // code-instantiated: the overlay title (a plain Label, not a MindLabel).
    let mut title = Label::new_alloc();
    title.set_text(&bt("@mod.import"));
    title.set_horizontal_alignment(HorizontalAlignment::CENTER);
    title.add_theme_color_override("font_color", styles::ACCENT);
    layout.add_child(&title);
    // code-instantiated: the accent underline of the overlay title.
    let mut rule = ColorRect::new_alloc();
    rule.set_color(styles::ACCENT);
    rule.set_anchors_preset(LayoutPreset::BOTTOM_WIDE);
    rule.set_offset(Side::TOP, -3.0);
    rule.set_offset(Side::BOTTOM, 0.0);
    rule.set_mouse_filter(MouseFilter::IGNORE);
    title.add_child(&rule);

    // code-instantiated: centers the import chooser panel.
    let mut center = CenterContainer::new_alloc();
    center.set_v_size_flags(SizeFlags::EXPAND_FILL);
    center.set_mouse_filter(MouseFilter::IGNORE);
    layout.add_child(&center);
    // `dialog.cont.table(Tex.button, t -> { t.margin(12f); … })`.
    // code-instantiated: the Tex.button chooser frame.
    let mut panel = PanelContainer::new_alloc();
    let mut panel_box =
        MindDialog::pane_box(Color::BLACK, styles::PANE_OUTLINE_GRAY, 4.0, 2.0, -1.0);
    for side in [Side::LEFT, Side::RIGHT, Side::TOP, Side::BOTTOM] {
        panel_box.set_content_margin(side, 12.0);
    }
    panel.add_theme_stylebox_override("panel", &panel_box);
    center.add_child(&panel);
    // code-instantiated: the two chooser entries.
    let mut column = VBoxContainer::new_alloc();
    column.add_theme_constant_override("separation", 0);
    panel.add_child(&column);

    // `Iconc.file` is a raw code point, not an alias.
    // code-instantiated: the file chooser glyph (`Iconc.file`, 61787).
    let mut file_icon = Label::new_alloc();
    if let Some(character) = char::from_u32(ICON_FILE_CODE) {
        let mut buffer = [0u8; 4];
        file_icon.set_text(&GString::from(&*character.encode_utf8(&mut buffer)));
    }
    {
        let mut control: Gd<Control> = file_icon.clone().upcast();
        kit::icons::apply_icon_font(&mut control);
    }
    file_icon.add_theme_font_size_override("font_size", 24);
    file_icon.set_vertical_alignment(VerticalAlignment::CENTER);
    file_icon.set_mouse_filter(MouseFilter::IGNORE);
    {
        let mut file_button = mods_flat_button("@mod.import.file", Some(file_icon.upcast()));
        let slot = state.clone();
        let callable = Callable::from_fn("mods_import_file", move |_args: &[&Variant]| {
            let slot = slot.clone();
            defer_action("mods_import_file_now", move || mods_import_file(&slot));
            Variant::nil()
        });
        file_button.connect("pressed", &callable);
        column.add_child(&file_button);
    }
    {
        let glyph = factory::glyph_label("github", 24, Color::WHITE);
        let mut github_button = mods_flat_button("@mod.import.github", Some(glyph.upcast()));
        let slot = state.clone();
        let callable = Callable::from_fn("mods_import_github", move |_args: &[&Variant]| {
            let slot = slot.clone();
            defer_action("mods_import_github_now", move || mods_import_github(&slot));
            Variant::nil()
        });
        github_button.connect("pressed", &callable);
        column.add_child(&github_button);
    }

    // `dialog.addCloseButton()`: `@back` (Icon.left) 210x64.
    // code-instantiated: the overlay's own back bar.
    let mut bar = HBoxContainer::new_alloc();
    bar.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    bar.set_mouse_filter(MouseFilter::IGNORE);
    layout.add_child(&bar);
    // code-instantiated: the overlay close button (no shared shell row here).
    let mut back = factory::button("");
    back.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    apply_pane_button(&mut back, styles::PANE_OUTLINE_GRAY, 4.0);
    let back_icon = factory::glyph_label("left", 20, Color::WHITE);
    build_button_content(&mut back, Some(back_icon.upcast()), &bt("@back"));
    {
        let slot = state.clone();
        let callable = Callable::from_fn("mods_import_close", move |_args: &[&Variant]| {
            mods_hide_import_dialog(&slot);
            Variant::nil()
        });
        back.connect("pressed", &callable);
    }
    bar.add_child(&back);

    state.borrow_mut().import_overlay = Some(overlay);
}

/// `Styles.flatt` (`Styles.java:149-156`): `up = black`, `over`/`down` =
/// `flatOver`; `t.defaults().size(300f, 70f)`, `.margin(12f)`.
fn mods_flat_button(key: &str, icon: Option<Gd<Control>>) -> Gd<Button> {
    // code-instantiated: chooser entries (see `mods_build_import_overlay`).
    let mut button = Button::new_alloc();
    button.set_custom_minimum_size(Vector2::new(300.0, 70.0));
    button.set_focus_mode(FocusMode::NONE);
    button.add_theme_stylebox_override("normal", &flat_style(Color::BLACK));
    button.add_theme_stylebox_override("hover", &flat_style(styles::FLAT_OVER));
    button.add_theme_stylebox_override("pressed", &flat_style(styles::FLAT_OVER));
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
    build_button_content(&mut button, icon, &bt(key));
    button
}

/// `_connect_mods()`: the `MindMods` state signals and the two `MindUi`
/// prompt results, bound once per dialog node.
fn mods_connect(dialog: &mut MindDialog, state: &Rc<RefCell<ModsState>>) {
    let mut node = dialog.to_gd();
    if node.has_meta(MODS_SIGNALS_META) {
        return;
    }
    if let Some(mut mods) = mods_node() {
        if mods.has_signal("mods_changed") {
            let slot = state.clone();
            let callable = Callable::from_fn("mods_changed", move |_args: &[&Variant]| {
                let slot = slot.clone();
                defer_action("mods_changed_refresh", move || mods_refresh(&slot));
                Variant::nil()
            });
            mods.connect("mods_changed", &callable);
        }
        if mods.has_signal("mod_error") {
            let callable = Callable::from_fn("mods_mod_error", move |args: &[&Variant]| {
                let name = args
                    .first()
                    .map(|value| variant_string(value))
                    .unwrap_or_default();
                let details = args
                    .get(1)
                    .map(|value| variant_string(value))
                    .unwrap_or_default();
                defer_action("mods_mod_error_now", move || {
                    mods_show_error(&format!("{name}: {details}"));
                });
                Variant::nil()
            });
            mods.connect("mod_error", &callable);
        }
        if mods.has_signal("mod_import_progress") {
            let slot = state.clone();
            let callable = Callable::from_fn("mods_import_progress", move |args: &[&Variant]| {
                let ratio = args
                    .get(1)
                    .and_then(|value| value.try_to::<f32>().ok())
                    .unwrap_or(0.0);
                let slot = slot.clone();
                defer_action("mods_import_progress_now", move || {
                    if let Ok(st) = slot.try_borrow()
                        && let Some(mut progress) = st.progress.clone()
                        && progress.is_instance_valid()
                    {
                        progress.set_visible(ratio < 1.0);
                        progress.set_value(f64::from(ratio));
                    }
                });
                Variant::nil()
            });
            mods.connect("mod_import_progress", &callable);
        }
    }
    if let Some(mut ui) = kit::ui() {
        if ui.has_signal("confirm_result") {
            let slot = state.clone();
            let callable = Callable::from_fn("mods_confirm_result", move |args: &[&Variant]| {
                let confirmed = args
                    .first()
                    .map(|value| variant_bool(value))
                    .unwrap_or(false);
                let slot = slot.clone();
                defer_action("mods_confirm_now", move || {
                    mods_on_confirm(&slot, confirmed)
                });
                Variant::nil()
            });
            ui.connect("confirm_result", &callable);
        }
        if ui.has_signal("text_input_result") {
            let slot = state.clone();
            let callable =
                Callable::from_fn("mods_text_input_result", move |args: &[&Variant]| {
                    let text = args
                        .first()
                        .map(|value| variant_string(value))
                        .unwrap_or_default();
                    let slot = slot.clone();
                    defer_action("mods_text_input_now", move || {
                        mods_on_text_input(&slot, &text);
                    });
                    Variant::nil()
                });
            ui.connect("text_input_result", &callable);
        }
    }
    node.set_meta(MODS_SIGNALS_META, &true.to_variant());
}

/// The browser's `set_updates(updates)` replacement (see module docs).
fn mods_read_updates(node: &Gd<MindDialog>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if !node.has_meta(MODS_UPDATES_META) {
        return out;
    }
    let value = node.get_meta(MODS_UPDATES_META);
    let Ok(dict) = value.try_to::<VarDictionary>() else {
        return out;
    };
    for (key, value) in dict.iter_shared() {
        out.push((variant_string(&key), variant_string(&value)));
    }
    out
}

/// `ModsDialog.setup()` (`:130-353`): rebuilds `cont`.
fn mods_refresh(state: &Rc<RefCell<ModsState>>) {
    let Ok(mut st) = state.try_borrow_mut() else {
        log::warn!("[mods] refresh skipped: controller state already borrowed");
        return;
    };
    mods_refresh_state(state, &mut st);
}

/// [`mods_refresh`] body (the caller owns the state borrow).
fn mods_refresh_state(state: &Rc<RefCell<ModsState>>, st: &mut ModsState) {
    let Some(mut root) = mods_ensure_root(st) else {
        return;
    };
    let mods = mods_node();
    let entries = mods_list(&mods);

    // Ctor (`:55-61`): `mods.list().contains(LoadedMod::failed)` -> Restore,
    // else Guide.
    let any_failed = entries
        .iter()
        .any(|entry| dict_bool(entry, "failed", false));
    if let Some(mut restore) = st.restore_button.clone()
        && restore.is_instance_valid()
    {
        restore.set_visible(any_failed);
    }
    if let Some(mut guide) = st.guide_button.clone()
        && guide.is_instance_valid()
    {
        guide.set_visible(!any_failed);
    }

    root.bind_mut().clear();
    st.pane = None;
    st.list = None;
    st.search = None;
    st.any_disabled = false;

    let viewport_width = match st.body.clone() {
        Some(body) if body.is_instance_valid() => body.get_viewport_rect().size.x,
        _ => 0.0,
    };
    // `w = min(width / 1.05 - 28, 520)`; `cont.defaults().width(min(width / 1.05, 556)).pad(4)`.
    let w = (viewport_width / 1.05 - 28.0).min(520.0);
    let cw = (viewport_width / 1.05).min(556.0);

    // `cont.add("@mod.reloadrequired").visible(mods::requiresReload).center()`:
    // an invisible Arc cell keeps its space, so fade instead of hiding.
    // code-instantiated: the reload banner is rebuilt per `setup()`.
    let mut banner = factory::label(&format!("[center]{}[/center]", bm("@mod.reloadrequired")));
    let requires_reload = mods.as_ref().is_some_and(|_| mods_requires_reload());
    let mut modulate = banner.get_modulate();
    modulate.a = if requires_reload { 1.0 } else { 0.0 };
    banner.set_modulate(modulate);
    {
        let mut bound = root.bind_mut();
        bound.add(&banner).width(cw).pad(4.0);
        bound.row();
    }
    st.banner = Some(banner);

    let action_row = mods_build_action_row();
    {
        let mut bound = root.bind_mut();
        bound.add(&action_row).width(w).height(60.0).pad(4.0);
        bound.row();
    }

    // Port-only (OD1): content errors / unsupported scripts summary button.
    let errors = mods_errors(&mods);
    // code-instantiated: only present when `MindMods.errors()` is non-empty.
    let mut errors_button = factory::button(&bt("@mod.errors"));
    errors_button.set_visible(!errors.is_empty());
    {
        let callable = Callable::from_fn("mods_show_errors", move |_args: &[&Variant]| {
            defer_action("mods_show_errors_now", mods_show_errors);
            Variant::nil()
        });
        errors_button.connect("pressed", &callable);
    }
    {
        let mut bound = root.bind_mut();
        bound.add(&errors_button).width(cw).pad(4.0);
        bound.row();
    }
    st.errors_button = Some(errors_button);

    // Port-only: `MindMods.mod_import_progress` bar.
    // code-instantiated: transient progress widget hidden unless an import runs.
    let mut progress = ProgressBar::new_alloc();
    progress.set_min(0.0);
    progress.set_max(1.0);
    progress.set_visible(false);
    {
        let mut bound = root.bind_mut();
        bound.add(&progress).width(cw).pad(4.0);
        bound.row();
    }
    st.progress = Some(progress);

    if entries.is_empty() {
        // `cont.table(Styles.black6, t -> t.add("@mods.none")).height(80f)`.
        // code-instantiated: the empty-state box replaces the pane in `setup()`.
        let mut none_box = PanelContainer::new_alloc();
        none_box.set_theme_type_variation("black6");
        let mut none_label = factory::label(&format!("[center]{}[/center]", bm("@mods.none")));
        none_label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        none_box.add_child(&none_label);
        let mut bound = root.bind_mut();
        bound.add(&none_box).width(cw).height(80.0).pad(4.0);
        bound.row();
    } else {
        // `search.image(Icon.zoom).padRight(8f); search.field("", rebuild).growX();`
        // code-instantiated: the search row is rebuilt with `cont` (`setup()`).
        let mut search_row = HBoxContainer::new_alloc();
        search_row.add_theme_constant_override("separation", 8);
        search_row.add_child(&factory::glyph_label("zoom", 22, Color::WHITE));
        // code-instantiated: the search field filters the runtime mod rows.
        let mut search = factory::field("");
        search.set_h_size_flags(SizeFlags::EXPAND_FILL);
        {
            let slot = state.clone();
            let callable = Callable::from_fn("mods_search", move |args: &[&Variant]| {
                let query = args
                    .first()
                    .map(|value| variant_string(value))
                    .unwrap_or_default();
                if let Ok(mut st) = slot.try_borrow_mut() {
                    mods_rebuild_list(&slot, &mut st, &query);
                }
                Variant::nil()
            });
            search.connect("text_changed", &callable);
        }
        search_row.add_child(&search);
        {
            let mut bound = root.bind_mut();
            bound.add(&search_row).width(cw).pad(4.0);
            bound.row();
        }
        st.search = Some(search);

        // `cont.pane(table1 -> pane[0] = table1.margin(10f).top()).scrollX(false)`.
        // code-instantiated: the mod rows are data-driven from `MindMods.list()`.
        let mut list = MindTable::new_alloc();
        list.set_h_size_flags(SizeFlags::EXPAND_FILL);
        list.bind_mut().margin(10.0);
        // code-instantiated: the scroll pane wraps the runtime mod table.
        let mut pane = MindScroll::new_alloc();
        pane.add_child(&list);
        pane.bind_mut().set_scrolling_disabled(true, false);
        {
            let mut bound = root.bind_mut();
            bound.add(&pane).width(cw).pad(4.0);
            bound.row();
        }
        st.list = Some(list);
        st.pane = Some(pane);
        mods_rebuild_list(state, st, "");

        // `withUpdates.size > 1`: the `@mods.update.all` row (w x 60, padTop 12).
        if st.updates.len() > 1 {
            // code-instantiated: the batch update row lists the pending updates.
            let mut update_all = mods_gray_button();
            let mut update_row = mods_button_row(
                &update_all,
                godot::classes::box_container::AlignmentMode::CENTER,
            );
            update_row.add_child(&factory::glyph_label("download", 24, Color::WHITE));
            update_row.add_child(&mods_row_label(&bm("@mods.update.all")));
            update_row.add_child(&factory::glyph_label("download", 24, Color::WHITE));
            {
                let slot = state.clone();
                let callable = Callable::from_fn("mods_update_all", move |_args: &[&Variant]| {
                    let slot = slot.clone();
                    defer_action("mods_update_all_now", move || {
                        if let Ok(st) = slot.try_borrow() {
                            mods_update_all(&st);
                        }
                    });
                    Variant::nil()
                });
                update_all.connect("pressed", &callable);
            }
            let mut bound = root.bind_mut();
            bound
                .add(&update_all)
                .width(w)
                .height(60.0)
                .pad(4.0)
                .pad_top(12.0);
            bound.row();
        }
    }

    root.bind_mut().sort_now();
    mods_defer_fit(state);
}

/// The `cont` root table, created once (`content_table()` replacement).
fn mods_ensure_root(state: &mut ModsState) -> Option<Gd<MindTable>> {
    if let Some(root) = state.root.clone() {
        if root.is_instance_valid() {
            return Some(root);
        }
        state.root = None;
    }
    let mut cont = state.cont.clone()?;
    if !cont.is_instance_valid() {
        return None;
    }
    // code-instantiated: the dialog body is rebuilt from `MindMods.list()`.
    let mut table = MindTable::new_alloc();
    {
        let mut bound = table.bind_mut();
        bound.grow_horizontal();
        bound.grow_vertical();
        bound.set_table_top(false);
        // `cont.defaults().pad(3)` (arc `Dialog.java:213`).
        let defaults = bound.defaults();
        defaults.pad_left = 3.0;
        defaults.pad_right = 3.0;
        defaults.pad_top = 3.0;
        defaults.pad_bottom = 3.0;
    }
    // Java tables center their content; `MindTable` packs from the left, so the
    // column itself is centered in `cont`.
    table.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    cont.add_child(&table);
    state.root = Some(table.clone());
    Some(table)
}

/// One `MindMods.list()` row set (`[]` when the autoload is absent).
fn mods_list(mods: &Option<Gd<Node>>) -> Vec<VarDictionary> {
    let Some(mut mods) = mods.clone() else {
        return Vec::new();
    };
    if !mods.has_method("list") {
        return Vec::new();
    }
    let value = mods.call("list", &[]);
    let Ok(rows) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    rows.iter_shared()
        .filter_map(|row| row.try_to::<VarDictionary>().ok())
        .collect()
}

/// One `MindMods.details(name)` row (`{}` when absent).
fn mods_details(name: &str) -> VarDictionary {
    let Some(mut mods) = mods_node() else {
        return VarDictionary::new();
    };
    if !mods.has_method("details") {
        return VarDictionary::new();
    }
    mods.call("details", &[GString::from(name).to_variant()])
        .try_to::<VarDictionary>()
        .unwrap_or_default()
}

/// `MindMods.errors()` rows.
fn mods_errors(mods: &Option<Gd<Node>>) -> Vec<VarDictionary> {
    let Some(mut mods) = mods.clone() else {
        return Vec::new();
    };
    if !mods.has_method("errors") {
        return Vec::new();
    }
    let value = mods.call("errors", &[]);
    let Ok(rows) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    rows.iter_shared()
        .filter_map(|row| row.try_to::<VarDictionary>().ok())
        .collect()
}

/// `MindMods.requires_reload()`.
fn mods_requires_reload() -> bool {
    let Some(mut mods) = mods_node() else {
        return false;
    };
    if !mods.has_method("requires_reload") {
        return false;
    }
    mods.call("requires_reload", &[]).to::<bool>()
}

/// `cont.table(buttons -> …)` (`:139-201`): `@mod.import` (Icon.add) and
/// `@mods.browser` (Icon.menu), `Styles.flatBordert`, `growX().height(60f)
/// .uniformX()`, `margin(12f)`.
fn mods_build_action_row() -> Gd<HBoxContainer> {
    // code-instantiated: the import/browser pair is rebuilt with `cont`.
    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 0);
    {
        let import_icon = factory::glyph_label("add", 24, Color::WHITE);
        let mut import_button =
            mods_flat_bordered_button("@mod.import", Some(import_icon.upcast()));
        let callable = Callable::from_fn("mods_show_import", move |_args: &[&Variant]| {
            defer_action("mods_show_import_now", mods_show_import_dialog);
            Variant::nil()
        });
        import_button.connect("pressed", &callable);
        actions.add_child(&import_button);
    }
    {
        let browser_icon = factory::glyph_label("menu", 24, Color::WHITE);
        let mut browser_button =
            mods_flat_bordered_button("@mods.browser", Some(browser_icon.upcast()));
        let callable = Callable::from_fn("mods_open_browser", move |_args: &[&Variant]| {
            defer_action("mods_open_browser_now", || open_dialog("mod_browser", "{}"));
            Variant::nil()
        });
        browser_button.connect("pressed", &callable);
        actions.add_child(&browser_button);
    }
    actions
}

/// `Styles.flatBordert` (`Styles.java:174-181`): `up = pane`, `over =
/// flatDownBase` (the white-framed 9-patch), `down = flatOver`.
fn mods_flat_bordered_button(key: &str, icon: Option<Gd<Control>>) -> Gd<Button> {
    // code-instantiated: action buttons are rebuilt with `cont`.
    let mut button = factory::button("");
    button.set_h_size_flags(SizeFlags::EXPAND_FILL);
    button.set_focus_mode(FocusMode::NONE);
    apply_pane_button(&mut button, styles::PANE_OUTLINE_GRAY, 4.0);
    button.add_theme_stylebox_override(
        "hover",
        &MindDialog::pane_box(Color::BLACK, Color::WHITE, 4.0, 2.0, -1.0),
    );
    button.add_theme_stylebox_override(
        "pressed",
        &MindDialog::pane_box(styles::FLAT_OVER, styles::FLAT_OVER, 4.0, 2.0, -1.0),
    );
    build_button_content(&mut button, icon, &bt(key));
    button
}

/// `rebuild.get(query)` (`:209-314`): one `Styles.grayt` card per matching mod,
/// a gray rule before the first disabled one.
fn mods_rebuild_list(state: &Rc<RefCell<ModsState>>, st: &mut ModsState, query: &str) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    if !list.is_instance_valid() {
        return;
    }
    list.bind_mut().clear();
    let entries = mods_list(&mods_node());
    let needle = query.trim().to_lowercase();
    let mut any = false;
    for entry in &entries {
        let display = strip_colors(&mods_display_name(entry));
        // `Strings.matches(query, displayName)`: case-insensitive contains.
        if !needle.is_empty() && !display.to_lowercase().contains(&needle) {
            continue;
        }
        any = true;
        if !dict_bool(entry, "enabled", false) && !st.any_disabled {
            st.any_disabled = true;
            // `cont.image().growX().height(4f).pad(6f).color(Pal.gray)`.
            // code-instantiated: the separator precedes the first disabled row.
            let mut rule = ColorRect::new_alloc();
            rule.set_color(styles::FLAT_OVER);
            rule.set_custom_minimum_size(Vector2::new(0.0, 4.0));
            let mut bound = list.bind_mut();
            bound.add(&rule).grow_x().height(4.0).pad(6.0);
            bound.row();
        }
        mods_add_mod_row(state, st, entry);
    }
    if !any {
        let mut bound = list.bind_mut();
        bound.add(&factory::label(&bm("@none.found"))).pad(4.0);
        bound.row();
    }
    list.bind_mut().sort_now();
    mods_defer_fit(state);
}

/// The pane takes its content height, capped to the space left between the
/// title and the button bar (an Arc `ScrollPane` cell shrinks to fit).
fn mods_fit_pane(state: &mut ModsState) {
    let (Some(mut pane), Some(list), Some(root), Some(body)) = (
        state.pane.clone(),
        state.list.clone(),
        state.root.clone(),
        state.body.clone(),
    ) else {
        return;
    };
    if !pane.is_instance_valid()
        || !list.is_instance_valid()
        || !root.is_instance_valid()
        || !body.is_instance_valid()
    {
        return;
    }
    let content_height = list.get_combined_minimum_size().y;
    let others = root.get_combined_minimum_size().y - pane.get_custom_minimum_size().y;
    let available = (body.get_size().y - others).max(ROW_HEIGHT);
    let target = content_height.min(available);
    if (pane.get_custom_minimum_size().y - target).abs() > 0.0005 {
        pane.set_custom_minimum_size(Vector2::new(0.0, target));
        root.clone().queue_sort();
    }
}

/// Queues one [`mods_fit_pane`] pass after the current callback.
fn mods_defer_fit(state: &Rc<RefCell<ModsState>>) {
    let slot = state.clone();
    defer_action("mods_fit_pane", move || {
        if let Ok(mut st) = slot.try_borrow_mut() {
            mods_fit_pane(&mut st);
        }
    });
}

/// `getStateText` (`:356-375`): the state line under a mod's name.
fn mods_state_text(entry: &VarDictionary) -> String {
    match dict_string(entry, "state").as_str() {
        "incompatible" => bt("@mod.blacklisted"),
        // Port-only state (no JS engine, OD1): the row shows the reason text
        // (`UnsupportedReason::message`, e.g. scripts) that `list()` carries.
        "unsupported" => {
            let reason = dict_string(entry, "reason");
            if reason.is_empty() {
                bt("@mod.blacklisted")
            } else {
                reason
            }
        }
        "circularDependencies" => bt("@mod.circulardependencies"),
        "incompleteDependencies" => bt("@mod.incompletedependencies"),
        "missingDependencies" => bt("@mod.unmetdependencies"),
        "contentErrors" => bt("@mod.erroredcontent"),
        _ => {
            if dict_bool(entry, "hidden", false) {
                bt("@mod.multiplayer.compatible")
            } else {
                String::new()
            }
        }
    }
}

/// `getStateDetails`: the tooltip/details body for a state.
fn mods_state_details(entry: &VarDictionary) -> String {
    let missing = string_list(entry.get("missing_dependencies").as_ref()).join(", ");
    match dict_string(entry, "state").as_str() {
        "incompatible" | "unsupported" => {
            let reason = dict_string(entry, "reason");
            if reason.is_empty() {
                bt("@mod.blacklisted.details")
            } else {
                reason
            }
        }
        "circularDependencies" => bt("@mod.circulardependencies.details"),
        "incompleteDependencies" => {
            fmt_key("@mod.incompletedependencies.details", &[missing.as_str()])
        }
        "missingDependencies" => fmt_key("@mod.missingdependencies.details", &[missing.as_str()]),
        "contentErrors" => bt("@mod.erroredcontent.details"),
        _ => String::new(),
    }
}

/// `hideDisabled = !isSupported() || hasUnmetDependencies() || hasContentErrors()`.
fn mods_hide_disabled(entry: &VarDictionary) -> bool {
    matches!(
        dict_string(entry, "state").as_str(),
        "unsupported"
            | "incompatible"
            | "missingDependencies"
            | "incompleteDependencies"
            | "circularDependencies"
            | "contentErrors"
    )
}

/// The card's two-line title (`:256-258`): `[accent]name` then the short
/// description when enabled (or `hideDisabled`), else `@mod.failed` /
/// `@mod.disabled`.
fn mods_title_markup(entry: &VarDictionary, enabled: bool) -> String {
    let mut text = format!("[accent]{}\n", strip_colors(&mods_display_name(entry)));
    if enabled || mods_hide_disabled(entry) {
        let description = dict_string(entry, "short_description");
        if !description.is_empty() {
            text.push_str("[lightgray]");
            text.push_str(&description);
        }
    } else if dict_bool(entry, "failed", false) {
        text.push_str(&bt("@mod.failed"));
    } else {
        text.push_str(&bt("@mod.disabled"));
    }
    text
}

/// `_display_name(entry)`.
fn mods_display_name(entry: &VarDictionary) -> String {
    for key in ["display_name", "displayName", "name"] {
        if let Some(value) = entry.get(key) {
            let text = variant_string(&value);
            if !text.is_empty() {
                return text;
            }
        }
    }
    String::new()
}

/// One `cont.button(t -> …, Styles.grayt, () -> showMod(mod)).size(w, h)
/// .growX().pad(4f).padTop(8f)` card (`:222-296`).
fn mods_add_mod_row(state: &Rc<RefCell<ModsState>>, st: &mut ModsState, entry: &VarDictionary) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    let mod_name = dict_string(entry, "name");
    let enabled = dict_bool(entry, "enabled", false);
    let mod_state = dict_string(entry, "state");
    // `.disabled(!mod.isSupported())` on the toggle.
    let supported = mod_state != "unsupported" && mod_state != "incompatible";

    // code-instantiated: mod rows are data-driven from `MindMods.list()`.
    let mut card = mods_gray_button();
    // `t.addListener(new Tooltip(… add(stateDetails) …))` (plain-text tooltip).
    card.set_tooltip_text(&strip_colors(&mods_state_details(entry)));
    {
        let slot = state.clone();
        let name = mod_name.clone();
        let callable = Callable::from_fn("mods_toggle_details", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                mods_toggle_details(&slot, &mut st, &name);
            }
            Variant::nil()
        });
        card.connect("pressed", &callable);
    }
    // code-instantiated: the card's content row (`t.margin(12f)`, top-left).
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 4.0);
    row.set_offset(Side::TOP, 4.0);
    row.set_offset(Side::RIGHT, -4.0);
    row.set_offset(Side::BOTTOM, -4.0);
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    card.add_child(&row);

    // `title1.add(new BorderImage()).size(h - 8f)…b.setDrawable(Tex.nomap);
    // b.border(Pal.accent)`. The port has no per-mod icon texture yet
    // (`MindMods.list()` carries none), so every card shows `nomap`.
    // code-instantiated: per-row icon frame.
    let mut icon = MindBorderImage::new_alloc();
    if let Some(texture) = kit::icon_texture("nomap") {
        icon.set_texture(&texture);
    }
    icon.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    icon.set_stretch_mode(godot::classes::texture_rect::StretchMode::SCALE);
    icon.set_custom_minimum_size(Vector2::new(ROW_HEIGHT - 8.0, ROW_HEIGHT - 8.0));
    {
        let mut bound = icon.bind_mut();
        bound.thickness = 4.0;
        bound.border_color = styles::ACCENT;
    }
    icon.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&icon);

    // code-instantiated: per-row title/state column (`title1.table(text -> …)`).
    let mut text_column = VBoxContainer::new_alloc();
    text_column.set_h_size_flags(SizeFlags::EXPAND_FILL);
    text_column.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&text_column);
    // `.ellipsis(true).top().width(300f).growX()`: two clipped lines.
    let title_markup = kit::markup_text(&mods_title_markup(entry, enabled)).to_string();
    let mut title = factory::label(&title_markup);
    title.set_fit_content(false);
    title.set_clip_contents(true);
    title.set_custom_minimum_size(Vector2::new(300.0, 52.0));
    title.set_h_size_flags(SizeFlags::EXPAND_FILL);
    title.set_mouse_filter(MouseFilter::IGNORE);
    text_column.add_child(&title);
    let state_text = mods_state_text(entry);
    if !state_text.is_empty() {
        // `text.labelWrap(state).growX()`.
        let wrapped = kit::markup_text(&state_text).to_string();
        let mut state_label = factory::label(&wrapped);
        state_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        state_label.set_custom_minimum_size(Vector2::new(300.0, 0.0));
        state_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        state_label.set_mouse_filter(MouseFilter::IGNORE);
        text_column.add_child(&state_label);
    }

    // `t.table(right -> …)`: enable toggle + delete, `Styles.clearNonei` 50x50.
    // code-instantiated: per-row action column.
    let mut right = HBoxContainer::new_alloc();
    right.set_v_size_flags(SizeFlags::SHRINK_BEGIN);
    right.add_theme_constant_override("separation", 0);
    row.add_child(&right);
    // code-instantiated: the enable toggle (`Icon.downOpen`/`upOpen`).
    let fallback = if enabled {
        bt("@mod.enabled")
    } else {
        bt("@mod.disabled")
    };
    let mut toggle = factory::glyph_button(
        if enabled { "down-open" } else { "up-open" },
        &fallback,
        "clearNonei",
    );
    toggle.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    toggle.set_focus_mode(FocusMode::NONE);
    toggle.set_disabled(!supported);
    {
        let slot = state.clone();
        let name = mod_name.clone();
        let callable = Callable::from_fn("mods_set_enabled", move |_args: &[&Variant]| {
            let slot = slot.clone();
            let name = name.clone();
            defer_action("mods_set_enabled_now", move || {
                mods_set_enabled(&slot, &name, !enabled);
            });
            Variant::nil()
        });
        toggle.connect("pressed", &callable);
    }
    right.add_child(&toggle);
    // code-instantiated: the per-row delete button.
    let mut remove = factory::glyph_button("trash", &bt("@delete"), "clearNonei");
    remove.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    remove.set_focus_mode(FocusMode::NONE);
    {
        let slot = state.clone();
        let name = mod_name.clone();
        let callable = Callable::from_fn("mods_confirm_delete", move |_args: &[&Variant]| {
            let slot = slot.clone();
            let name = name.clone();
            defer_action("mods_confirm_delete_now", move || {
                mods_confirm_delete(&slot, &name);
            });
            Variant::nil()
        });
        remove.connect("pressed", &callable);
    }
    right.add_child(&remove);

    {
        let mut bound = list.bind_mut();
        bound
            .add(&card)
            .grow_x()
            .height(ROW_HEIGHT)
            .pad(4.0)
            .pad_top(8.0);
        bound.row();
    }

    let update_version = st
        .updates
        .iter()
        .find(|(name, _)| *name == mod_name)
        .map(|(_, version)| version.clone());
    if let Some(version) = update_version {
        // `hasUpdate(mod)`: `Styles.grayt` row, `Icon.download` +
        // `@mods.update.available` (`width(w).height(48f).padTop(-4f)`).
        // code-instantiated: update row from the browser-published set.
        let mut update = mods_gray_button();
        let mut update_row =
            mods_button_row(&update, godot::classes::box_container::AlignmentMode::BEGIN);
        update_row.add_child(&factory::glyph_label(
            "download",
            24,
            styles::ARC_LIGHT_GRAY,
        ));
        update_row.add_child(&mods_row_label(
            &kit::tm_format("@mods.update.available", &[version.as_str()]).to_string(),
        ));
        {
            let name = mod_name.clone();
            let callable = Callable::from_fn("mods_update_mod", move |_args: &[&Variant]| {
                let name = name.clone();
                defer_action("mods_update_mod_now", move || mods_update_mod(&name));
                Variant::nil()
            });
            update.connect("pressed", &callable);
        }
        let mut bound = list.bind_mut();
        bound
            .add(&update)
            .grow_x()
            .height(48.0)
            .pad_left(4.0)
            .pad_right(4.0);
        bound.row();
    }

    if st.open_details == mod_name {
        mods_add_details(state, st, &mod_name);
    }
}

/// `Styles.grayt` (`Styles.java:157-165`): `up = grayPanel`, `over`/`down` =
/// `flatOver`.
fn mods_gray_button() -> Gd<Button> {
    // code-instantiated: cards/rows are data-driven.
    let mut button = Button::new_alloc();
    button.set_focus_mode(FocusMode::NONE);
    let up = flat_style(styles::GRAY_PANEL);
    let over = flat_style(styles::FLAT_OVER);
    button.add_theme_stylebox_override("normal", &up);
    button.add_theme_stylebox_override("hover", &over);
    button.add_theme_stylebox_override("pressed", &over);
    button.add_theme_stylebox_override("hover_pressed", &over);
    button.add_theme_stylebox_override("disabled", &up);
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
    button
}

/// A mouse-transparent content row inside a button (`b.margin(6f)`).
fn mods_button_row(
    button: &Gd<Button>,
    alignment: godot::classes::box_container::AlignmentMode,
) -> Gd<HBoxContainer> {
    // code-instantiated: button content rows are data-driven.
    let mut row = HBoxContainer::new_alloc();
    row.set_alignment(alignment);
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 6.0);
    row.set_offset(Side::RIGHT, -6.0);
    row.add_theme_constant_override("separation", 8);
    row.set_mouse_filter(MouseFilter::IGNORE);
    let mut button = button.clone();
    button.add_child(&row);
    row
}

/// A non-wrapping row label (`_row_label`).
fn mods_row_label(markup: &str) -> Gd<MindLabel> {
    let mut label = factory::label(markup);
    label.set_autowrap_mode(AutowrapMode::OFF);
    label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    label.set_mouse_filter(MouseFilter::IGNORE);
    label
}

/// `showMod`: the expanded details block (upstream opens a BaseDialog).
fn mods_add_details(state: &Rc<RefCell<ModsState>>, st: &mut ModsState, mod_name: &str) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    let details = mods_details(mod_name);
    // code-instantiated: details are data-driven from `MindMods.details(name)`.
    let mut box_ = VBoxContainer::new_alloc();
    box_.set_h_size_flags(SizeFlags::EXPAND_FILL);

    let author = dict_string(&details, "author");
    if !author.is_empty() {
        box_.add_child(&factory::label(&format!(
            "{} {}",
            bt("@editor.author"),
            author
        )));
    }
    let version = dict_string(&details, "version");
    if !version.is_empty() {
        box_.add_child(&factory::label(&format!(
            "{} {}",
            bt("@mod.version"),
            version
        )));
    }
    let description = dict_string(&details, "description");
    if !description.is_empty() {
        let mut body_label = factory::label(&description);
        body_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        box_.add_child(&body_label);
    }
    let state_details = mods_state_details(&details);
    if !state_details.is_empty() {
        box_.add_child(&factory::label(&state_details));
    }

    // Dependency dialog (`@mod.dependencies.downloadall`) delegates to the browser.
    let missing = string_list(details.get("missing_dependencies").as_ref());
    if !missing.is_empty() {
        // code-instantiated: dependency import row only when deps are unmet.
        let mut dependency = factory::button(&bt("@mod.dependencies.downloadall"));
        {
            let missing = missing.clone();
            let callable = Callable::from_fn("mods_download_deps", move |_args: &[&Variant]| {
                let missing = missing.clone();
                defer_action("mods_download_deps_now", move || {
                    mods_download_dependencies(&missing);
                });
                Variant::nil()
            });
            dependency.connect("pressed", &callable);
        }
        box_.add_child(&dependency);
    }

    // code-instantiated: per-mod action row inside the details block.
    let mut actions = HBoxContainer::new_alloc();
    let mut folder = factory::button(&bt("@mods.openfolder"));
    {
        let path = dict_string(&details, "config_folder");
        let callable = Callable::from_fn("mods_details_folder", move |_args: &[&Variant]| {
            mods_open_path(&path);
            Variant::nil()
        });
        folder.connect("pressed", &callable);
    }
    actions.add_child(&folder);
    let repo = dict_string(&details, "repo");
    if !repo.is_empty() {
        let mut open_repo = factory::button(&bt("@mods.github.open"));
        let url = format!("https://github.com/{repo}");
        let callable = Callable::from_fn("mods_details_repo", move |_args: &[&Variant]| {
            mods_open_url(&url);
            Variant::nil()
        });
        open_repo.connect("pressed", &callable);
        actions.add_child(&open_repo);
    }
    box_.add_child(&actions);

    // `Seq<UnlockableContent> all = …` + `if(all.any()) … button("@mods.viewcontent")`
    // (`ModsDialog.java:455-481`): the per-mod content grid. Java opens a second
    // `BaseDialog`; the port expands the grid inside the details block (the same
    // inline pattern as the details themselves).
    let content = mods_content(mod_name);
    if !content.is_empty() {
        // code-instantiated: the view-content label is a bundle string.
        let mut view_content = factory::button(&bt("@mods.viewcontent"));
        view_content.set_custom_minimum_size(Vector2::new(300.0, 50.0));
        {
            let slot = state.clone();
            let name = mod_name.to_string();
            let callable = Callable::from_fn("mods_view_content", move |_args: &[&Variant]| {
                if let Ok(mut st) = slot.try_borrow_mut() {
                    mods_toggle_content(&slot, &mut st, &name);
                }
                Variant::nil()
            });
            view_content.connect("pressed", &callable);
        }
        box_.add_child(&view_content);
        if st.open_content == mod_name {
            box_.add_child(&mods_content_grid(&content));
        }
    }

    {
        let mut bound = list.bind_mut();
        bound.add(&box_).grow_x().pad(6.0);
        bound.row();
    }
}

/// `MindMods.content_for(name)` rows (`{type, name, stem, path}`); empty when
/// the mod ships no content (Java's `all.any()` gate).
fn mods_content(name: &str) -> Vec<VarDictionary> {
    let Some(mut mods) = mods_node() else {
        return Vec::new();
    };
    if !mods.has_method("content_for") {
        return Vec::new();
    }
    mods.call("content_for", &[GString::from(name).to_variant()])
        .try_to::<VarArray>()
        .map(|rows| {
            rows.iter_shared()
                .filter_map(|row| row.try_to::<VarDictionary>().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// `d.cont.pane(cs -> …)` (`ModsDialog.java:466-479`): 50 px icon buttons in
/// rows of 14 (Java: `min(width / Scl.scl(110), 14)`), each opening
/// `ui.content.show(c)`.
fn mods_content_grid(content: &[VarDictionary]) -> Gd<VBoxContainer> {
    // code-instantiated: the grid is data-driven from the mod's content files.
    let mut grid = VBoxContainer::new_alloc();
    grid.set_h_size_flags(SizeFlags::EXPAND_FILL);
    grid.add_theme_constant_override("separation", 4);
    for chunk in content.chunks(14) {
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 4);
        for entry in chunk {
            row.add_child(&mods_content_icon(entry));
        }
        grid.add_child(&row);
    }
    grid
}

/// One content icon (`cs.button(new TextureRegionDrawable(c.uiIcon), Styles.flati,
/// iconMed, () -> ui.content.show(c))`, `ModsDialog.java:467-473`).
fn mods_content_icon(entry: &VarDictionary) -> Gd<Button> {
    let content_name = dict_string(entry, "name");
    let content_type = dict_string(entry, "type");
    // code-instantiated: one icon button per content record.
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("flati");
    button.set_custom_minimum_size(Vector2::new(50.0, 50.0));
    let icon = format!("{content_type}-{content_name}-ui");
    if let Some(texture) = kit::icon_texture(&icon) {
        let mut image = TextureRect::new_alloc();
        image.set_texture(&texture);
        image.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        image.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        image.set_mouse_filter(MouseFilter::IGNORE);
        image.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        button.add_child(&image);
    } else {
        // No shipped icon: fall back to the content-type glyph.
        let glyph_name = match content_type.as_str() {
            "liquid" => "liquid",
            "unit" => "units",
            "block" => "blocks",
            _ => "book",
        };
        let mut glyph = factory::glyph_label(glyph_name, 26, Color::WHITE);
        glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&glyph);
    }
    // `tooltip(c.localizedName)`; the port has no per-mod bundle key here, so
    // the file stem is the honest label.
    button.set_tooltip_text(&GString::from(dict_string(entry, "stem").as_str()));
    {
        let content_name = content_name.clone();
        let callable = Callable::from_fn("mods_content_info", move |_args: &[&Variant]| {
            let name = content_name.clone();
            defer_action("mods_content_info_now", move || {
                if let Some(node) = dialog_node("mods")
                    && let Ok(dialog) = node.try_cast::<MindDialog>()
                {
                    super::batch_a::open_content_info(&dialog, &name);
                }
            });
            Variant::nil()
        });
        button.connect("pressed", &callable);
    }
    button
}

/// `_toggle_content(mod_name)`: expand/collapse one mod's content grid.
fn mods_toggle_content(state: &Rc<RefCell<ModsState>>, st: &mut ModsState, mod_name: &str) {
    st.open_content = if st.open_content == mod_name {
        String::new()
    } else {
        mod_name.to_string()
    };
    let query = st
        .search
        .clone()
        .filter(|search| search.is_instance_valid())
        .map(|search| search.get_text().to_string())
        .unwrap_or_default();
    mods_rebuild_list(state, st, &query);
}

/// `_toggle_details(mod_name)`.
fn mods_toggle_details(state: &Rc<RefCell<ModsState>>, st: &mut ModsState, mod_name: &str) {
    st.open_details = if st.open_details == mod_name {
        String::new()
    } else {
        mod_name.to_string()
    };
    let query = st
        .search
        .clone()
        .filter(|search| search.is_instance_valid())
        .map(|search| search.get_text().to_string())
        .unwrap_or_default();
    mods_rebuild_list(state, st, &query);
}

/// `mods.setEnabled(mod, !mod.enabled()); setup();`.
fn mods_set_enabled(state: &Rc<RefCell<ModsState>>, mod_name: &str, enabled: bool) {
    if let Some(mut mods) = mods_node()
        && mods.has_method("set_enabled")
    {
        let _ = mods.call(
            "set_enabled",
            &[GString::from(mod_name).to_variant(), enabled.to_variant()],
        );
    }
    mods_refresh(state);
}

/// `@mods.restore` (`ModsDialog.java:56-59`): re-enable every failed mod.
fn mods_restore_failed(state: &Rc<RefCell<ModsState>>) {
    let Some(mut mods) = mods_node() else {
        return;
    };
    if !mods.has_method("list") || !mods.has_method("set_enabled") {
        return;
    }
    let failed: Vec<String> = mods_list(&Some(mods.clone()))
        .iter()
        .filter(|entry| dict_bool(entry, "failed", false))
        .map(|entry| dict_string(entry, "name"))
        .collect();
    for name in failed {
        let _ = mods.call(
            "set_enabled",
            &[GString::from(name.as_str()).to_variant(), true.to_variant()],
        );
    }
    mods_refresh(state);
}

/// `ui.showConfirm("@confirm", "@mod.remove.confirm", …)`.
fn mods_confirm_delete(state: &Rc<RefCell<ModsState>>, mod_name: &str) {
    state.borrow_mut().pending_delete = mod_name.to_string();
    if ui_call(
        "show_confirm",
        &[GString::from(bt("@mod.remove.confirm").as_str()).to_variant()],
    )
    .is_none()
    {
        mods_on_confirm(state, true);
    }
}

/// `_on_confirm(confirmed)`.
fn mods_on_confirm(state: &Rc<RefCell<ModsState>>, confirmed: bool) {
    let mod_name = std::mem::take(&mut state.borrow_mut().pending_delete);
    if mod_name.is_empty() || !confirmed {
        return;
    }
    if let Some(mut mods) = mods_node()
        && mods.has_method("remove_mod")
    {
        let result = mods
            .call(
                "remove_mod",
                &[GString::from(mod_name.as_str()).to_variant()],
            )
            .try_to::<VarDictionary>()
            .unwrap_or_default();
        if !dict_bool(&result, "ok", false) {
            let error = dict_string(&result, "error");
            mods_show_error(&if error.is_empty() {
                bt("@mod.delete.error")
            } else {
                error
            });
        }
    }
    state
        .borrow_mut()
        .updates
        .retain(|(name, _)| name != &mod_name);
}

/// `_import_file`: hide the chooser and open the native `.zip`/`.jar` picker.
fn mods_import_file(state: &Rc<RefCell<ModsState>>) {
    mods_hide_import_dialog(state);
    let start = mods_node()
        .filter(|node| node.has_method("mods_dir"))
        .map(|mut node| node.call("mods_dir", &[]).to::<GString>().to_string())
        .unwrap_or_else(|| String::from("user://mods"));
    let mut filters = PackedStringArray::new();
    filters.push("*.zip");
    filters.push("*.jar");
    let slot = state.clone();
    let callable = Callable::from_fn("mods_files_chosen", move |args: &[&Variant]| {
        let status = args
            .first()
            .map(|value| variant_bool(value))
            .unwrap_or(false);
        if !status {
            return Variant::nil();
        }
        let paths = match args.get(1) {
            Some(value) => value.to::<PackedStringArray>(),
            None => PackedStringArray::new(),
        };
        let names: Vec<String> = paths
            .as_slice()
            .iter()
            .map(|path| path.to_string())
            .collect();
        let slot = slot.clone();
        defer_action("mods_import_paths", move || {
            for path in names {
                mods_import_path(&slot, &path);
            }
        });
        Variant::nil()
    });
    let title = bt("@mod.import.file");
    let shown = crate::platform::dialogs::show_file_dialog(
        &title, &start, "", false, false, true, false, &filters, &callable,
    );
    if !shown {
        log::warn!("[mods] native file dialog unavailable; cannot import a file");
    }
}

/// `_import_path(path)`.
fn mods_import_path(state: &Rc<RefCell<ModsState>>, path: &str) {
    let Some(mut mods) = mods_node() else {
        return;
    };
    if !mods.has_method("import_mod") {
        return;
    }
    let result = mods
        .call("import_mod", &[GString::from(path).to_variant()])
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    if !dict_bool(&result, "ok", false) {
        let error = dict_string(&result, "error");
        mods_show_error(&if error.is_empty() {
            bt("@mod.delete.error")
        } else {
            error
        });
    }
    mods_refresh(state);
}

/// `@mod.import.github`: `ui.showTextInput("@mod.import.github", "", 64,
/// Core.settings.getString("lastmod", ""), …)`.
fn mods_import_github(state: &Rc<RefCell<ModsState>>) {
    mods_hide_import_dialog(state);
    let last = ui_call(
        "settings_get_text",
        &[
            GString::from("lastmod").to_variant(),
            GString::from("").to_variant(),
        ],
    )
    .map(|value| value.to::<GString>().to_string())
    .unwrap_or_default();
    state.borrow_mut().pending_github = true;
    let _ = ui_call(
        "show_text_input",
        &[
            GString::from(bt("@mod.import.github").as_str()).to_variant(),
            GString::from("").to_variant(),
            64i64.to_variant(),
            GString::from(last.as_str()).to_variant(),
            false.to_variant(),
            false.to_variant(),
        ],
    );
}

/// `_on_text_input(text)`: empty text = cancelled.
fn mods_on_text_input(state: &Rc<RefCell<ModsState>>, text: &str) {
    {
        let mut st = state.borrow_mut();
        if !st.pending_github {
            return;
        }
        st.pending_github = false;
    }
    if text.trim().is_empty() {
        return;
    }
    let raw = text.to_string();
    defer_action("mods_github_import", move || mods_github_import(&raw));
}

/// Text cleanup + `lastmod` + `githubImportMod` (`:176-191`).
fn mods_github_import(raw: &str) {
    let mut text = raw.trim().replace(' ', "");
    if let Some(rest) = text.strip_prefix("https://github.com/") {
        text = rest.to_string();
    }
    if text.is_empty() {
        return;
    }
    let _ = ui_call(
        "settings_put_text",
        &[
            GString::from("lastmod").to_variant(),
            GString::from(text.as_str()).to_variant(),
        ],
    );
    if let Some(browser) = dialog_node("mod_browser") {
        install_repos_on(&browser, &[text]);
    } else {
        let mut context = VarDictionary::new();
        context.set("repo", &GString::from(text.as_str()).to_variant());
        open_dialog(
            "mod_browser",
            &Json::stringify(&context.to_variant()).to_string(),
        );
    }
}

/// `githubImportMod(mod.getRepo(), …)` via the browser.
fn mods_update_mod(mod_name: &str) {
    let repo = dict_string(&mods_details(mod_name), "repo");
    if repo.is_empty() {
        return;
    }
    let mut context = VarDictionary::new();
    context.set("repo", &GString::from(repo.as_str()).to_variant());
    open_dialog(
        "mod_browser",
        &Json::stringify(&context.to_variant()).to_string(),
    );
}

/// `_update_all()`: reinstall every mod with a published update.
fn mods_update_all(state: &ModsState) {
    let mut repos: Vec<String> = Vec::new();
    for (name, _) in &state.updates {
        let repo = dict_string(&mods_details(name), "repo");
        if !repo.is_empty() {
            repos.push(repo);
        }
    }
    if let Some(browser) = dialog_node("mod_browser") {
        install_repos_on(&browser, &repos);
    } else {
        open_dialog("mod_browser", "{}");
    }
}

/// `_download_dependencies(dependencies)`: browser.show() then install.
fn mods_download_dependencies(dependencies: &[String]) {
    open_dialog("mod_browser", "{}");
    if let Some(browser) = dialog_node("mod_browser") {
        install_repos_on(&browser, dependencies);
    }
}

/// `install_repos(items)` on the browser node (metadata handoff; see the
/// module docs).
fn install_repos_on(browser: &Gd<Node>, items: &[String]) {
    if items.is_empty() {
        return;
    }
    let mut array = VarArray::new();
    for item in items {
        array.push(&GString::from(item.as_str()).to_variant());
    }
    let mut browser = browser.clone();
    browser.set_meta(BROWSER_INSTALL_META, &array.to_variant());
    refresh_dialog_deferred(&browser);
}

/// `Core.app.openFolder(modDirectory.absolutePath())`.
fn mods_open_folder() {
    let path = mods_node()
        .filter(|node| node.has_method("mods_dir"))
        .map(|mut node| node.call("mods_dir", &[]).to::<GString>().to_string())
        .unwrap_or_default();
    mods_open_path(&path);
}

/// `OS.shell_open(path)` through the platform seam.
fn mods_open_path(path: &str) {
    if path.is_empty() {
        return;
    }
    let _ = crate::platform::uri::open_uri(path);
}

/// `Core.app.openURI(url)` through the platform seam.
fn mods_open_url(url: &str) {
    if url.is_empty() {
        return;
    }
    let _ = crate::platform::uri::open_uri(url);
}

/// `_show_errors()`: the `name: details` summary prompt.
fn mods_show_errors() {
    let mut lines: Vec<String> = Vec::new();
    for entry in mods_errors(&mods_node()) {
        lines.push(format!(
            "{}: {}",
            dict_string(&entry, "name"),
            dict_string(&entry, "details")
        ));
    }
    mods_show_error(&lines.join("\n"));
}

/// Deferred: callers include `MindUi` signal handlers, where calling back into
/// the `&mut`-bound `MindUi` would panic.
fn mods_show_error(text: &str) {
    let text = text.to_string();
    defer_action("mods_show_error_now", move || {
        mods_show_error_now(&text);
    });
}

/// `_show_error_now(text)`: `ui.showText("@mods", text)` else `showInfo`.
fn mods_show_error_now(text: &str) {
    if text.is_empty() {
        return;
    }
    if ui_call(
        "show_text",
        &[
            GString::from(bt("@mods").as_str()).to_variant(),
            GString::from(text).to_variant(),
        ],
    )
    .is_some()
    {
        return;
    }
    let _ = ui_call("show_info", &[GString::from(text).to_variant()]);
}

/// `_show_import_dialog()`.
fn mods_show_import_dialog() {
    let Some(node) = dialog_node("mods") else {
        return;
    };
    let Some(overlay) = node.get_node_or_null("ImportDialog") else {
        return;
    };
    let Ok(mut overlay) = overlay.try_cast::<Control>() else {
        return;
    };
    overlay.set_visible(true);
    overlay.move_to_front();
}

/// `_hide_import_dialog()`.
fn mods_hide_import_dialog(state: &Rc<RefCell<ModsState>>) {
    if let Some(mut overlay) = state.borrow().import_overlay.clone()
        && overlay.is_instance_valid()
    {
        overlay.set_visible(false);
    }
}

// ---------------------------------------------------------------------------
// ModBrowserDialog (`mod_browser`, `ModBrowserDialog.java`)
// ---------------------------------------------------------------------------

/// Ported from `Vars.modJsonURLs` (tried in order).
const MOD_JSON_URLS: [&str; 2] = [
    "https://raw.githubusercontent.com/Anuken/MindustryMods/master/mods.json",
    "https://cdn.jsdelivr.net/gh/anuken/mindustrymods/mods.json",
];
/// Ported from `Vars.ghApi`.
const GH_API: &str = "https://api.github.com";
/// TEMP download directory for zipballs (deleted after import).
const CACHE_PATH: &str = "user://modcache";
/// `HTTPRequest.RESULT_SUCCESS`.
const HTTP_RESULT_SUCCESS: i64 = 0;
/// The mods dialog's `install_repos(items)` replacement (metadata handoff).
const BROWSER_INSTALL_META: &str = "_mind_browser_install_repos";
/// Guard so the `mod_import_progress` binding runs once per dialog node.
const BROWSER_SIGNALS_META: &str = "_mind_browser_signals_bound";

/// The browser's HTTP slots (one `HTTPRequest` per concurrent step).
#[derive(Clone, Copy, PartialEq, Eq)]
enum HttpSlot {
    Listings,
    Api,
    Release,
    Zip,
}

/// `ModBrowserDialog` view state (`_listings`/`_installed_repos`/`_queue`/…).
struct BrowserState {
    search: Option<Gd<LineEdit>>,
    repo_field: Option<Gd<LineEdit>>,
    sort_button: Option<Gd<Button>>,
    status: Option<Gd<MindLabel>>,
    list: Option<Gd<MindTable>>,
    listings: Vec<VarDictionary>,
    installed_repos: Vec<(String, String)>,
    /// `_order_date` (sort by date first).
    order_date: bool,
    open_repo: String,
    /// Batch reinstall queue (`_queue`).
    queue: Vec<String>,
    /// Name-only installs deferred until the listings load.
    pending_install: Vec<String>,
    pending_repo: String,
    pending_release: String,
    pending_url_index: usize,
    downloading: bool,
    fetching: bool,
    game_build: i64,
    game_revision: i64,
    list_http: Option<Gd<HttpRequest>>,
    api_http: Option<Gd<HttpRequest>>,
    release_http: Option<Gd<HttpRequest>>,
    zip_http: Option<Gd<HttpRequest>>,
    /// `_process` replacement (streams the zipball percent).
    progress_timer: Option<Gd<Timer>>,
}

impl Default for BrowserState {
    fn default() -> Self {
        Self {
            search: None,
            repo_field: None,
            sort_button: None,
            status: None,
            list: None,
            listings: Vec::new(),
            installed_repos: Vec::new(),
            order_date: true,
            open_repo: String::new(),
            queue: Vec::new(),
            pending_install: Vec::new(),
            pending_repo: String::new(),
            pending_release: String::new(),
            pending_url_index: 0,
            downloading: false,
            fetching: false,
            game_build: 0,
            game_revision: 0,
            list_http: None,
            api_http: None,
            release_http: None,
            zip_http: None,
            progress_timer: None,
        }
    }
}

/// `@mods.browser` — the GitHub mod browser (`ModBrowserDialog.java`).
#[derive(Default)]
pub struct ModBrowserDialog {
    state: Rc<RefCell<BrowserState>>,
}

impl DialogController for ModBrowserDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@mods.browser");
        dialog.should_pause = false;
        // `makeButtonOverlay()` (`ModBrowserDialog.java:61`).
        dialog.apply_button_overlay(true);
        browser_add_http(dialog, &self.state);
        browser_build(dialog, &self.state);
        dialog.add_close_button(210.0);
        browser_connect(dialog, &self.state);
        let mut st = self.state.borrow_mut();
        st.game_build = i64::from(mods_i32("game_build"));
        st.game_revision = i64::from(mods_i32("game_revision"));
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // A batch handed over by the mods dialog (`install_repos`).
        let pending = browser_take_install_meta(&dialog.to_gd());
        {
            let mut st = self.state.borrow_mut();
            if !pending.is_empty() {
                browser_install_repos(&self.state, &mut st, &pending);
            }
            // Lazy fetch on first open (upstream `shown(this::rebuildBrowser)`).
            browser_fetch_listings(&self.state, &mut st);
        }
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let repo = dialog.context_string("repo").to_string();
        if repo.is_empty() {
            return;
        }
        let mut st = self.state.borrow_mut();
        if let Some(mut field) = st.repo_field.clone()
            && field.is_instance_valid()
        {
            field.set_text(&GString::from(repo.as_str()));
        }
        browser_install_repo(&mut st, &repo);
    }
}

/// `MindMods` integer method (`game_build`/`game_revision`).
fn mods_i32(method: &str) -> i32 {
    let Some(mut mods) = mods_node() else {
        return 0;
    };
    if !mods.has_method(method) {
        return 0;
    }
    mods.call(method, &[]).to::<i32>()
}

/// `_http_headers`: GitHub rejects API/raw requests without a `User-Agent`.
fn http_headers() -> PackedStringArray {
    let mut headers = PackedStringArray::new();
    headers.push("User-Agent: Mindustry-Godot");
    headers.push("Accept: application/vnd.github+json");
    headers
}

/// `_add_http()`: one `HTTPRequest` per concurrent step (list/api/release/zip).
fn browser_add_http(dialog: &mut MindDialog, state: &Rc<RefCell<BrowserState>>) {
    // `ready()` runs again after an extension reload; drop the previous nodes.
    for name in [
        "BrowserListHttp",
        "BrowserApiHttp",
        "BrowserReleaseHttp",
        "BrowserZipHttp",
        "BrowserProgressTimer",
    ] {
        if let Some(mut existing) = dialog.base().get_node_or_null(name) {
            existing.queue_free();
        }
    }
    let list_http = browser_make_http("BrowserListHttp", state, HttpSlot::Listings);
    // code-instantiated: the listings request node has no static scene.
    dialog.base_mut().add_child(&list_http);
    let api_http = browser_make_http("BrowserApiHttp", state, HttpSlot::Api);
    // code-instantiated: the repository request node has no static scene.
    dialog.base_mut().add_child(&api_http);
    let release_http = browser_make_http("BrowserReleaseHttp", state, HttpSlot::Release);
    // code-instantiated: the release request node has no static scene.
    dialog.base_mut().add_child(&release_http);
    let zip_http = browser_make_http("BrowserZipHttp", state, HttpSlot::Zip);
    // code-instantiated: the zipball request node has no static scene.
    dialog.base_mut().add_child(&zip_http);

    // `_process` replacement: poll the zipball byte counters while downloading.
    // code-instantiated: a `DialogController` has no per-frame hook.
    let mut timer = Timer::new_alloc();
    timer.set_name("BrowserProgressTimer");
    timer.set_wait_time(0.1);
    timer.set_one_shot(false);
    timer.set_autostart(false);
    {
        let slot = state.clone();
        let tick = Callable::from_fn("browser_progress", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_progress_tick(&mut st);
            }
            Variant::nil()
        });
        timer.connect("timeout", &tick);
    }
    dialog.base_mut().add_child(&timer);

    let mut st = state.borrow_mut();
    st.list_http = Some(list_http);
    st.api_http = Some(api_http);
    st.release_http = Some(release_http);
    st.zip_http = Some(zip_http);
    st.progress_timer = Some(timer);
}

/// Builds one `HTTPRequest` with its `request_completed` handler.
fn browser_make_http(
    name: &str,
    state: &Rc<RefCell<BrowserState>>,
    slot: HttpSlot,
) -> Gd<HttpRequest> {
    // code-instantiated: one HTTPRequest per concurrent HTTP step.
    let mut http = HttpRequest::new_alloc();
    http.set_name(name);
    let state = state.clone();
    let callable = Callable::from_fn("browser_request_completed", move |args: &[&Variant]| {
        let result = args.first().map(|value| variant_i64(value)).unwrap_or(-1);
        let code = args.get(1).map(|value| variant_i64(value)).unwrap_or(0);
        let body = match args.get(3) {
            Some(value) => value.to::<PackedByteArray>(),
            None => PackedByteArray::new(),
        };
        if let Ok(mut st) = state.try_borrow_mut() {
            match slot {
                HttpSlot::Listings => {
                    browser_on_list_completed(&state, &mut st, result, code, &body)
                }
                HttpSlot::Api => browser_on_api_completed(&mut st, result, code, &body),
                HttpSlot::Release => browser_on_release_completed(&mut st, result, code, &body),
                HttpSlot::Zip => browser_on_zip_completed(&state, &mut st, result, code, &body),
            }
        }
        Variant::nil()
    });
    http.connect("request_completed", &callable);
    http
}

/// `_build()`.
fn browser_build(dialog: &mut MindDialog, state: &Rc<RefCell<BrowserState>>) {
    let mut root = dialog.content_table();
    {
        let mut control: Gd<Control> = root.clone().upcast();
        control.add_theme_constant_override("separation", 6);
    }

    // Search + sort row (`ModBrowserDialog` search field / order toggle).
    // code-instantiated: the search row filters the fetched listings.
    let mut search_row = HBoxContainer::new_alloc();
    // code-instantiated: the listing filter field.
    let mut search = factory::field(&bt("@search"));
    search.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_search", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_rebuild(&slot, &mut st);
            }
            Variant::nil()
        });
        search.connect("text_changed", &callable);
    }
    search_row.add_child(&search);
    // code-instantiated: the date/stars order toggle.
    let mut sort_button = factory::image_button("list", "emptyi");
    sort_button.set_tooltip_text(&bt("@mods.browser.sortdate"));
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_sort", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_toggle_order(&slot, &mut st);
            }
            Variant::nil()
        });
        sort_button.connect("pressed", &callable);
    }
    search_row.add_child(&sort_button);
    {
        let mut bound = root.bind_mut();
        bound.add(&search_row).grow_x().pad(4.0);
        bound.row();
    }

    // Direct GitHub repo import fallback (`@mod.import.github`).
    // code-instantiated: the direct `owner/repo` install row.
    let mut repo_row = HBoxContainer::new_alloc();
    // code-instantiated: the `owner/repo` field.
    let mut repo_field = factory::field(&bt("@mods.github.open"));
    repo_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_repo_submit", move |args: &[&Variant]| {
            let repo = args
                .first()
                .map(|value| variant_string(value))
                .unwrap_or_default();
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_install_repo(&mut st, &repo);
            }
            Variant::nil()
        });
        repo_field.connect("text_submitted", &callable);
    }
    repo_row.add_child(&repo_field);
    // code-instantiated: the direct install button.
    let mut install = factory::button(&bt("@mods.browser.add"));
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_repo_add", move |_args: &[&Variant]| {
            let repo = slot
                .try_borrow()
                .ok()
                .and_then(|st| st.repo_field.clone())
                .filter(|field| field.is_instance_valid())
                .map(|field| field.get_text().to_string())
                .unwrap_or_default();
            if let Ok(mut st) = slot.try_borrow_mut() {
                browser_install_repo(&mut st, &repo);
            }
            Variant::nil()
        });
        install.connect("pressed", &callable);
    }
    repo_row.add_child(&install);
    {
        let mut bound = root.bind_mut();
        bound.add(&repo_row).grow_x().pad(4.0);
        bound.row();
    }

    // code-instantiated: the transient status line (`@loading`/errors/percent).
    let mut status = factory::label("");
    status.set_visible(false);
    {
        let mut bound = root.bind_mut();
        bound.add(&status).grow_x().pad(2.0);
        bound.row();
    }

    // code-instantiated: the listing rows are data-driven from `mods.json`.
    let mut list = MindTable::new_alloc();
    list.set_h_size_flags(SizeFlags::EXPAND_FILL);
    list.set_v_size_flags(SizeFlags::EXPAND_FILL);
    {
        let mut bound = root.bind_mut();
        bound.add(&list).grow().pad(4.0);
    }

    let mut st = state.borrow_mut();
    st.search = Some(search);
    st.repo_field = Some(repo_field);
    st.sort_button = Some(sort_button);
    st.status = Some(status);
    st.list = Some(list);
}

/// `_connect_mods()`: the import-progress percent on the status line.
fn browser_connect(dialog: &mut MindDialog, state: &Rc<RefCell<BrowserState>>) {
    let mut node = dialog.to_gd();
    if node.has_meta(BROWSER_SIGNALS_META) {
        return;
    }
    if let Some(mut mods) = mods_node()
        && mods.has_signal("mod_import_progress")
    {
        let slot = state.clone();
        let callable = Callable::from_fn("browser_import_progress", move |args: &[&Variant]| {
            let ratio = args
                .get(1)
                .and_then(|value| value.try_to::<f32>().ok())
                .unwrap_or(0.0);
            let slot = slot.clone();
            defer_action("browser_import_progress_now", move || {
                if let Ok(st) = slot.try_borrow() {
                    browser_on_import_progress(&st, ratio);
                }
            });
            Variant::nil()
        });
        mods.connect("mod_import_progress", &callable);
    }
    node.set_meta(BROWSER_SIGNALS_META, &true.to_variant());
}

/// The `install_repos(items)` payload a mods-dialog handoff left behind.
fn browser_take_install_meta(node: &Gd<MindDialog>) -> Vec<String> {
    if !node.has_meta(BROWSER_INSTALL_META) {
        return Vec::new();
    }
    let mut node = node.clone();
    let value = node.get_meta(BROWSER_INSTALL_META);
    node.remove_meta(BROWSER_INSTALL_META);
    string_list(Some(&value))
}

/// `_fetch_listings()`.
fn browser_fetch_listings(state: &Rc<RefCell<BrowserState>>, st: &mut BrowserState) {
    if !st.listings.is_empty() {
        browser_rebuild(state, st);
        return;
    }
    if st.fetching {
        return;
    }
    st.fetching = true;
    browser_set_status(st, &bt("@loading"));
    browser_fetch_url_index(state, st, 0);
}

/// `_fetch_url_index(index)`.
fn browser_fetch_url_index(state: &Rc<RefCell<BrowserState>>, st: &mut BrowserState, index: usize) {
    if index >= MOD_JSON_URLS.len() {
        st.fetching = false;
        browser_set_status(st, &fmt_key("@connectfail", &["mods.json"]));
        browser_rebuild(state, st);
        return;
    }
    st.pending_url_index = index;
    let url = MOD_JSON_URLS[index];
    if !matches!(
        browser_request(st, HttpSlot::Listings, url),
        Some(godot::global::Error::OK)
    ) {
        browser_fetch_url_index(state, st, index + 1);
    }
}

/// `_on_list_completed`.
fn browser_on_list_completed(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    result: i64,
    code: i64,
    body: &PackedByteArray,
) {
    if result == HTTP_RESULT_SUCCESS && code == 200 {
        let parsed = Json::parse_string(&body.get_string_from_utf8());
        if let Ok(raw) = parsed.try_to::<VarArray>() {
            browser_parse_listings(state, st, &raw);
            return;
        }
    }
    let next = st.pending_url_index + 1;
    browser_fetch_url_index(state, st, next);
}

/// Normalizes each raw listing through `mind-core`'s `ModListing` (camelCase +
/// defaults) so the dialog never re-implements the listing model.
fn browser_parse_listings(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    raw: &VarArray,
) {
    st.fetching = false;
    st.listings.clear();
    let mut mods = mods_node();
    for entry in raw.iter_shared() {
        let Ok(entry) = entry.try_to::<VarDictionary>() else {
            continue;
        };
        let mut listing = entry.clone();
        if let Some(mods) = mods.as_mut()
            && mods.has_method("parse_listing")
        {
            let parsed = mods
                .call(
                    "parse_listing",
                    &[Json::stringify(&entry.to_variant()).to_variant()],
                )
                .try_to::<VarDictionary>()
                .unwrap_or_default();
            if dict_bool(&parsed, "ok", false)
                && let Some(value) = parsed.get("listing")
                && let Ok(row) = value.try_to::<VarDictionary>()
            {
                listing = row;
            }
        }
        st.listings.push(listing);
    }
    if let Some(mut status) = st.status.clone()
        && status.is_instance_valid()
    {
        status.set_visible(false);
    }
    browser_refresh_installed(st);
    browser_rebuild(state, st);
    // A queued dependency install waits for the listings to resolve names.
    if !st.pending_install.is_empty() {
        let pending = std::mem::take(&mut st.pending_install);
        browser_install_repos(state, st, &pending);
    }
}

/// `_refresh_installed()`.
fn browser_refresh_installed(st: &mut BrowserState) {
    st.installed_repos.clear();
    if let Some(mut mods) = mods_node()
        && mods.has_method("list")
        && mods.has_method("details")
    {
        let value = mods.call("list", &[]);
        if let Ok(rows) = value.try_to::<VarArray>() {
            for entry in rows.iter_shared() {
                let Ok(entry) = entry.try_to::<VarDictionary>() else {
                    continue;
                };
                let mod_name = dict_string(&entry, "name");
                let details = mods
                    .call("details", &[GString::from(mod_name.as_str()).to_variant()])
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
                let repo = dict_string(&details, "repo");
                if !repo.is_empty() {
                    st.installed_repos.push((repo, mod_name));
                }
            }
        }
    }
    browser_publish_updates(st);
}

/// Pushes `internal name -> remote version` to the mods dialog (`withUpdates`).
fn browser_publish_updates(st: &mut BrowserState) {
    let Some(mods_dialog) = dialog_node("mods") else {
        return;
    };
    let mut updates = VarDictionary::new();
    for listing in st.listings.clone() {
        let repo = dict_string(&listing, "repo");
        if repo.is_empty() {
            continue;
        }
        let Some(mod_name) = st
            .installed_repos
            .iter()
            .find(|(installed, _)| *installed == repo)
            .map(|(_, name)| name.clone())
        else {
            continue;
        };
        let details = mods_details(&mod_name);
        let latest = browser_latest_version(st, &listing);
        if !latest.is_empty() && browser_version_newer(&latest, &dict_string(&details, "version")) {
            updates.set(mod_name.as_str(), &latest.to_variant());
        }
    }
    let mut mods_dialog = mods_dialog;
    mods_dialog.set_meta(MODS_UPDATES_META, &updates.to_variant());
    refresh_dialog_deferred(&mods_dialog);
}

/// Version that would be installed: the release matching this build, else the
/// listing's latest (`refreshModUpdates` uses the matching release's version).
fn browser_latest_version(st: &BrowserState, listing: &VarDictionary) -> String {
    if let Some(release) = browser_matching_release(st, listing) {
        let version = dict_string(&release, "version");
        if !version.is_empty() {
            return version;
        }
    }
    dict_string(listing, "version")
}

/// Numeric comparison of the dot-separated version prefix (`checkNewerSemver`
/// intent); `"1.2.1"` is newer than `"1.2"`, an equal prefix is not newer.
fn browser_version_newer(newer: &str, older: &str) -> bool {
    if newer == older {
        return false;
    }
    let left = browser_version_parts(newer);
    let right = browser_version_parts(older);
    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    false
}

/// `_version_parts(version)`.
fn browser_version_parts(version: &str) -> Vec<i64> {
    let mut parts = Vec::new();
    for chunk in version.trim().split('.') {
        if chunk.is_empty() {
            continue;
        }
        let digits: String = chunk.chars().take_while(|c| c.is_ascii_digit()).collect();
        parts.push(digits.parse().unwrap_or(0));
    }
    parts
}

/// `_rebuild()`.
fn browser_rebuild(state: &Rc<RefCell<BrowserState>>, st: &mut BrowserState) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    if !list.is_instance_valid() {
        return;
    }
    list.bind_mut().clear();
    if st.listings.is_empty() {
        let mut bound = list.bind_mut();
        bound.add(&factory::label(&bt("@mods.none"))).pad(8.0);
        return;
    }
    let query = st
        .search
        .clone()
        .filter(|search| search.is_instance_valid())
        .map(|search| search.get_text().to_string().to_lowercase())
        .unwrap_or_default();
    let mut visible: Vec<VarDictionary> = Vec::new();
    for listing in &st.listings {
        if query.is_empty()
            || dict_string(listing, "name").to_lowercase().contains(&query)
            || dict_string(listing, "repo").to_lowercase().contains(&query)
        {
            visible.push(listing.clone());
        }
    }
    if st.order_date {
        visible.sort_by_key(|listing| Reverse(dict_string(listing, "last_updated")));
    } else {
        visible.sort_by_key(|listing| Reverse(dict_i64(listing, "stars", 0)));
    }
    if visible.is_empty() {
        let mut bound = list.bind_mut();
        bound.add(&factory::label(&bt("@none.found"))).pad(8.0);
        return;
    }
    for listing in visible {
        browser_add_listing_row(state, st, &listing);
    }
}

/// `_add_listing_row(listing)`.
fn browser_add_listing_row(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    listing: &VarDictionary,
) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    let repo = dict_string(listing, "repo");
    let name = dict_string(listing, "name");
    let mod_name = if name.is_empty() { repo.clone() } else { name };
    // code-instantiated: browser rows are data-driven from the fetched listings.
    let mut row = HBoxContainer::new_alloc();
    row.set_h_size_flags(SizeFlags::EXPAND_FILL);

    // code-instantiated: the info card button (name/version/stars block).
    let mut info = Button::new_alloc();
    info.set_theme_type_variation("flatBordert");
    info.set_text_alignment(HorizontalAlignment::LEFT);
    info.set_h_size_flags(SizeFlags::EXPAND_FILL);
    info.set_text(&format!(
        "{}\n{} {}  \u{2605} {}",
        mod_name,
        bt("@mods.browser.latest"),
        dict_string(listing, "version"),
        dict_i64(listing, "stars", 0),
    ));
    {
        let slot = state.clone();
        let repo = repo.clone();
        let callable = Callable::from_fn("browser_toggle_repo", move |_args: &[&Variant]| {
            if let Ok(mut st) = slot.try_borrow_mut() {
                st.open_repo = if st.open_repo == repo {
                    String::new()
                } else {
                    repo.clone()
                };
                browser_rebuild(&slot, &mut st);
            }
            Variant::nil()
        });
        info.connect("pressed", &callable);
    }
    row.add_child(&info);

    let installed = st
        .installed_repos
        .iter()
        .any(|(installed, _)| *installed == repo);
    // code-instantiated: the per-listing add/reinstall button.
    let mut add = factory::button(&if installed {
        bt("@mods.browser.reinstall")
    } else {
        bt("@mods.browser.add")
    });
    {
        let slot = state.clone();
        let repo = repo.clone();
        let callable = Callable::from_fn("browser_install_row", move |_args: &[&Variant]| {
            let slot = slot.clone();
            let repo = repo.clone();
            defer_action("browser_install_row_now", move || {
                if let Ok(mut st) = slot.try_borrow_mut() {
                    browser_install_repo(&mut st, &repo);
                }
            });
            Variant::nil()
        });
        add.connect("pressed", &callable);
    }
    row.add_child(&add);

    {
        let mut bound = list.bind_mut();
        bound.add(&row).grow_x().pad(3.0);
        bound.row();
    }
    if st.open_repo == repo {
        browser_add_listing_details(state, st, listing);
    }
}

/// `_add_listing_details(listing)`.
fn browser_add_listing_details(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    listing: &VarDictionary,
) {
    let Some(mut list) = st.list.clone() else {
        return;
    };
    // code-instantiated: details come from the fetched listing (GitHub model).
    let mut box_ = VBoxContainer::new_alloc();
    box_.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let author = dict_string(listing, "author");
    if !author.is_empty() {
        box_.add_child(&factory::label(&format!(
            "{} {}",
            bt("@editor.author"),
            author
        )));
    }
    let description = dict_string(listing, "description");
    if !description.is_empty() {
        let mut description_label = factory::label(&description);
        description_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        box_.add_child(&description_label);
    }
    let min_version = {
        let value = dict_string(listing, "min_game_version");
        if value.is_empty() {
            dict_string(listing, "minGameVersion")
        } else {
            value
        }
    };
    if !min_version.is_empty() {
        box_.add_child(&factory::label(&fmt_key(
            "@mod.requiresversion",
            &[min_version.as_str()],
        )));
    }
    let tags = string_list(listing.get("tags").as_ref());
    if !tags.is_empty() {
        box_.add_child(&factory::label(&tags.join(", ")));
    }

    if let Some(release) = browser_matching_release(st, listing) {
        let version = {
            let value = dict_string(&release, "version");
            if value.is_empty() {
                dict_string(&release, "id")
            } else {
                value
            }
        };
        box_.add_child(&factory::label(&format!(
            "{} {}",
            bt("@mods.browser.releases"),
            version
        )));
    }
    let repo = dict_string(listing, "repo");
    // code-instantiated: per-listing action row.
    let mut actions = HBoxContainer::new_alloc();
    let mut open_repo = factory::button(&bt("@mods.github.open"));
    {
        let url = format!("https://github.com/{repo}");
        let callable = Callable::from_fn("browser_open_repo", move |_args: &[&Variant]| {
            mods_open_url(&url);
            Variant::nil()
        });
        open_repo.connect("pressed", &callable);
    }
    actions.add_child(&open_repo);
    let mut add = factory::button(&bt("@mods.browser.add"));
    {
        let slot = state.clone();
        let repo = repo.clone();
        let callable = Callable::from_fn("browser_install_details", move |_args: &[&Variant]| {
            let slot = slot.clone();
            let repo = repo.clone();
            defer_action("browser_install_details_now", move || {
                if let Ok(mut st) = slot.try_borrow_mut() {
                    browser_install_repo(&mut st, &repo);
                }
            });
            Variant::nil()
        });
        add.connect("pressed", &callable);
    }
    actions.add_child(&add);
    box_.add_child(&actions);
    {
        let mut bound = list.bind_mut();
        bound.add(&box_).grow_x().pad(6.0);
        bound.row();
    }
}

/// `ModListing.getMatchingRelease` via `mind-core`.
fn browser_matching_release(st: &BrowserState, listing: &VarDictionary) -> Option<VarDictionary> {
    let mut mods = mods_node()?;
    if !mods.has_method("matching_release") {
        return None;
    }
    let result = mods
        .call(
            "matching_release",
            &[
                Json::stringify(&listing.to_variant()).to_variant(),
                st.game_build.to_variant(),
                st.game_revision.to_variant(),
            ],
        )
        .try_to::<VarDictionary>()
        .unwrap_or_default();
    if dict_bool(&result, "found", false) {
        Some(result)
    } else {
        None
    }
}

/// `_toggle_order()`.
fn browser_toggle_order(state: &Rc<RefCell<BrowserState>>, st: &mut BrowserState) {
    st.order_date = !st.order_date;
    if let Some(mut button) = st.sort_button.clone()
        && button.is_instance_valid()
    {
        button.set_tooltip_text(&if st.order_date {
            bt("@mods.browser.sortdate")
        } else {
            bt("@mods.browser.sortstars")
        });
    }
    browser_rebuild(state, st);
}

/// `install_repos(items)`.
fn browser_install_repos(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    items: &[String],
) {
    let mut deferred: Vec<String> = Vec::new();
    for item in items {
        let repo = browser_repo_for(st, item);
        if repo.is_empty() {
            deferred.push(item.clone());
        } else {
            st.queue.push(repo);
        }
    }
    if !deferred.is_empty() {
        st.pending_install = deferred;
        browser_fetch_listings(state, st);
    }
    if !st.queue.is_empty() {
        browser_order_next(st);
    }
}

/// Resolves an `owner/repo` string or an internal mod name via the listings.
fn browser_repo_for(st: &BrowserState, item: &str) -> String {
    let cleaned = item.trim();
    if cleaned.contains('/') {
        return cleaned.to_string();
    }
    for listing in &st.listings {
        let internal = {
            let value = dict_string(listing, "internal_name");
            if value.is_empty() {
                dict_string(listing, "internalName")
            } else {
                value
            }
        };
        if internal == cleaned {
            return dict_string(listing, "repo");
        }
    }
    String::new()
}

/// Listing for `repo` (case-insensitive), or `None` when absent.
fn browser_find_listing(st: &BrowserState, repo: &str) -> Option<VarDictionary> {
    st.listings
        .iter()
        .find(|listing| dict_string(listing, "repo").to_lowercase() == repo.to_lowercase())
        .cloned()
}

/// `_install_repo(repo)`: downloads the matching zipball then imports it.
fn browser_install_repo(st: &mut BrowserState, repo: &str) {
    let mut cleaned = repo.trim().replace(' ', "");
    if let Some(rest) = cleaned.strip_prefix("https://github.com/") {
        cleaned = rest.to_string();
    }
    cleaned = cleaned
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_string();
    if cleaned.is_empty() || !cleaned.contains('/') {
        return;
    }
    st.pending_repo = cleaned.clone();
    st.pending_release = String::new();
    if let Some(listing) = browser_find_listing(st, &cleaned)
        && let Some(release) = browser_matching_release(st, &listing)
    {
        st.pending_release = dict_string(&release, "id");
    }
    browser_set_status(st, &fmt_key("@mods.downloading", &[cleaned.as_str()]));
    if st.pending_release.is_empty() {
        // No version-specific release: resolve the default branch from the repo.
        let url = format!("{GH_API}/repos/{cleaned}");
        match browser_request(st, HttpSlot::Api, &url) {
            Some(godot::global::Error::OK) => {}
            Some(error) => browser_install_failed(st, &error.ord().to_string()),
            None => browser_install_failed(st, "no api request node"),
        }
        return;
    }
    browser_request_release_zipball(st, &cleaned);
}

/// Fetches the release object (`githubImportBranch` release path) for its
/// zipball.
fn browser_request_release_zipball(st: &mut BrowserState, repo: &str) {
    let url = format!("{GH_API}/repos/{repo}/releases/{}", st.pending_release);
    if !matches!(
        browser_request(st, HttpSlot::Release, &url),
        Some(godot::global::Error::OK)
    ) {
        browser_install_failed(st, &fmt_key("@connectfail", &["release"]));
    }
}

/// `_on_api_completed`.
fn browser_on_api_completed(st: &mut BrowserState, result: i64, code: i64, body: &PackedByteArray) {
    if result != HTTP_RESULT_SUCCESS || code != 200 {
        let message = fmt_key("@connectfail", &[code.to_string().as_str()]);
        browser_install_failed(st, &message);
        return;
    }
    let parsed = Json::parse_string(&body.get_string_from_utf8());
    let mut branch = String::from("master");
    if let Ok(dict) = parsed.try_to::<VarDictionary>() {
        let value = dict_string(&dict, "default_branch");
        if !value.is_empty() {
            branch = value;
        }
    }
    let url = format!("{GH_API}/repos/{}/zipball/{branch}", st.pending_repo);
    browser_request_zipball(st, &url);
}

/// `_on_release_completed`.
fn browser_on_release_completed(
    st: &mut BrowserState,
    result: i64,
    code: i64,
    body: &PackedByteArray,
) {
    if result != HTTP_RESULT_SUCCESS || code != 200 {
        // Release lookup failed; fall back to the default-branch zipball.
        st.pending_release = String::new();
        let url = format!("{GH_API}/repos/{}", st.pending_repo);
        match browser_request(st, HttpSlot::Api, &url) {
            Some(godot::global::Error::OK) => {}
            Some(error) => browser_install_failed(st, &error.ord().to_string()),
            None => browser_install_failed(st, "no api request node"),
        }
        return;
    }
    let parsed = Json::parse_string(&body.get_string_from_utf8());
    let zipball = parsed
        .try_to::<VarDictionary>()
        .map(|dict| dict_string(&dict, "zipball_url"))
        .unwrap_or_default();
    if zipball.is_empty() {
        browser_install_failed(st, &fmt_key("@connectfail", &["zipball"]));
        return;
    }
    browser_request_zipball(st, &zipball);
}

/// `_request_zipball(url)`.
fn browser_request_zipball(st: &mut BrowserState, url: &str) {
    if !matches!(
        browser_request(st, HttpSlot::Zip, url),
        Some(godot::global::Error::OK)
    ) {
        browser_install_failed(st, &fmt_key("@connectfail", &["zipball"]));
        return;
    }
    st.downloading = true;
    let text = format!(
        "{} 0%",
        fmt_key("@mods.downloading", &[st.pending_repo.as_str()])
    );
    browser_set_status(st, &text);
    if let Some(mut timer) = st.progress_timer.clone()
        && timer.is_instance_valid()
    {
        timer.start();
    }
}

/// `_on_zip_completed`.
fn browser_on_zip_completed(
    state: &Rc<RefCell<BrowserState>>,
    st: &mut BrowserState,
    result: i64,
    code: i64,
    body: &PackedByteArray,
) {
    st.downloading = false;
    if let Some(mut timer) = st.progress_timer.clone()
        && timer.is_instance_valid()
    {
        timer.stop();
    }
    if result != HTTP_RESULT_SUCCESS || code != 200 {
        let message = fmt_key("@connectfail", &[code.to_string().as_str()]);
        browser_install_failed(st, &message);
        return;
    }
    let cache = ProjectSettings::singleton().globalize_path(CACHE_PATH);
    let _ = DirAccess::make_dir_recursive_absolute(&cache);
    let path = format!("{CACHE_PATH}/{}.zip", st.pending_repo.replace('/', "_"));
    let global = ProjectSettings::singleton().globalize_path(path.as_str());
    let Some(mut file) = FileAccess::open(&global, godot::classes::file_access::ModeFlags::WRITE)
    else {
        browser_install_failed(st, &bt("@mod.delete.error"));
        return;
    };
    file.store_buffer(body);
    file.close();
    let mut imported = VarDictionary::new();
    if let Some(mut mods) = mods_node()
        && mods.has_method("import_mod")
    {
        imported = mods
            .call("import_mod", &[GString::from(path.as_str()).to_variant()])
            .try_to::<VarDictionary>()
            .unwrap_or_default();
        if !dict_bool(&imported, "ok", false) {
            // Duplicate install (reinstall): drop the existing copy and retry once.
            if !st
                .installed_repos
                .iter()
                .any(|(repo, _)| *repo == st.pending_repo)
            {
                browser_refresh_installed(st);
            }
            let existing = st
                .installed_repos
                .iter()
                .find(|(repo, _)| *repo == st.pending_repo)
                .map(|(_, name)| name.clone())
                .unwrap_or_default();
            if !existing.is_empty() && mods.has_method("remove_mod") {
                let _ = mods.call(
                    "remove_mod",
                    &[GString::from(existing.as_str()).to_variant()],
                );
                imported = mods
                    .call("import_mod", &[GString::from(path.as_str()).to_variant()])
                    .try_to::<VarDictionary>()
                    .unwrap_or_default();
            }
        }
    }
    let _ = DirAccess::remove_absolute(&global);
    if !dict_bool(&imported, "ok", false) {
        let error = dict_string(&imported, "error");
        browser_install_failed(
            st,
            &if error.is_empty() {
                bt("@mod.delete.error")
            } else {
                error
            },
        );
        return;
    }
    if let Some(mut status) = st.status.clone()
        && status.is_instance_valid()
    {
        status.set_visible(false);
    }
    browser_refresh_installed(st);
    browser_rebuild(state, st);
    browser_order_next(st);
}

/// `_install_failed(message)`.
fn browser_install_failed(st: &mut BrowserState, message: &str) {
    st.downloading = false;
    if let Some(mut timer) = st.progress_timer.clone()
        && timer.is_instance_valid()
    {
        timer.stop();
    }
    browser_set_status(st, message);
    let _ = ui_call("show_info", &[GString::from(message).to_variant()]);
    browser_order_next(st);
}

/// `_order_next()`.
fn browser_order_next(st: &mut BrowserState) {
    if st.queue.is_empty() {
        return;
    }
    let next = st.queue.remove(0);
    browser_install_repo(st, &next);
}

/// `_on_import_progress(path, ratio)`.
fn browser_on_import_progress(st: &BrowserState, ratio: f32) {
    let Some(mut status) = st.status.clone() else {
        return;
    };
    if !status.is_instance_valid() || !status.is_visible() {
        return;
    }
    let text = format!(
        "{} {}%",
        fmt_key("@mods.downloading", &[st.pending_repo.as_str()]),
        (ratio * 100.0) as i64
    );
    status.set_text(&GString::from(text.as_str()));
}

/// `_process`: streams the real `HTTPRequest` byte counters while the zipball
/// downloads (timer-driven; see the module docs).
fn browser_progress_tick(st: &mut BrowserState) {
    if !st.downloading {
        return;
    }
    let Some(zip_http) = st.zip_http.clone() else {
        return;
    };
    if !zip_http.is_instance_valid() {
        return;
    }
    let total = zip_http.get_body_size();
    if total <= 0 {
        return;
    }
    let downloaded = zip_http.get_downloaded_bytes();
    let percent = (f64::from(downloaded) / f64::from(total) * 100.0) as i64;
    let text = format!(
        "{} {percent}%",
        fmt_key("@mods.downloading", &[st.pending_repo.as_str()])
    );
    browser_set_status(st, &text);
}

/// The status line (`_status.text = …; _status.visible = true`).
fn browser_set_status(st: &mut BrowserState, text: &str) {
    if let Some(mut status) = st.status.clone()
        && status.is_instance_valid()
    {
        status.set_text(&GString::from(text));
        status.set_visible(true);
    }
}

/// Sends one request on `slot` (`None` when the node is missing).
fn browser_request(
    st: &mut BrowserState,
    slot: HttpSlot,
    url: &str,
) -> Option<godot::global::Error> {
    let headers = http_headers();
    let http = match slot {
        HttpSlot::Listings => st.list_http.as_mut(),
        HttpSlot::Api => st.api_http.as_mut(),
        HttpSlot::Release => st.release_http.as_mut(),
        HttpSlot::Zip => st.zip_http.as_mut(),
    }?;
    Some(http.request_ex(url).custom_headers(&headers).done())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_rgba_truncates_like_arc() {
        // `(int)(c * 255)` truncation, not rounding: 0.5 alpha is 0x7f.
        assert_eq!(arc_rgba(Color::from_rgba(1.0, 0.0, 0.0, 0.5)), 0xff00007f);
        assert_eq!(
            arc_hex(Color::from_rgba8(0x7f, 0x59, 0x9a, 0xff)),
            "7f599aff"
        );
    }

    #[test]
    fn version_compare_is_numeric_per_component() {
        assert!(browser_version_newer("1.2.1", "1.2"));
        assert!(!browser_version_newer("1.2", "1.2.1"));
        assert!(!browser_version_newer("1.2", "1.2"));
        assert!(!browser_version_newer("2.0", "10.0"));
    }

    #[test]
    fn version_parts_ignore_non_numeric_suffixes() {
        assert_eq!(browser_version_parts("1.2.3-beta"), vec![1, 2, 3]);
        assert_eq!(browser_version_parts(""), Vec::<i64>::new());
    }

    #[test]
    fn strip_colors_drops_arc_tags() {
        assert_eq!(strip_colors("[accent]Copper[lightgray] ore"), "Copper ore");
        assert_eq!(strip_colors("[#ff0000]red[]"), "red");
        assert_eq!(strip_colors("no [tags] here"), "no  here");
        assert_eq!(strip_colors("keep [1] digits"), "keep [1] digits");
    }
}
