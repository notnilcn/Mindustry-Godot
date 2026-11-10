// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The keybind dialog (`controls`): the rebind table, its search field and the key/mouse capture overlay.
//! Upstream: core/src/mindustry/ui/dialogs/KeybindDialog.java

use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    CenterContainer, ColorRect, Control, HBoxContainer, InputEvent, InputEventKey,
    InputEventMouseButton, LineEdit, Os, PanelContainer, RichTextLabel, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton};
use godot::obj::{NewAlloc, Singleton};
use godot::prelude::*;

use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, Align, MindLabel, MindTable, styles};

use super::util::*;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "controls" => Some(Box::new(ControlsController::default())),
        _ => None,
    }
}

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
