// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/HostDialog.java.

//! The host dialog controller: the remembered name/colour and port, and the
//! `host_requested` intent.
//! Upstream: ui/dialogs/HostDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, ColorRect, Control, HBoxContainer, LineEdit, MarginContainer, VBoxContainer,
};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::net::util_h::{
    close_named_dialog, color_rgba32, dialog_node, emit_dialog_signal, generation_live,
    hide_dialog_node, next_generation, open_named_dialog, push_player_identity, settings_int,
    settings_put_int, settings_put_text, stored_player_color, stored_player_name, ui_call,
    ui_show_info, variant_string,
};
use crate::ui::kit;
use crate::ui::kit::widgets::factory;

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "host" => Some(Box::new(HostDialogController::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// HostDialogController (manifest id `host`)
// ---------------------------------------------------------------------------

/// `HostDialog.w`: the name/port tables and the `@host` button width
/// (`HostDialog.java:16`).
const HOST_WIDTH: f32 = 300.0;
/// `HostDialog`'s row height (`HostDialog.java:33,62`): the name/colour and
/// port tables are `width(w).height(70f)`.
const HOST_ROW_HEIGHT: f32 = 70.0;
/// Java's empty spacer column before `@host` (`cont.add().width(65f)`).
const HOST_SPACER_WIDTH: f32 = 65.0;
/// The `?` info button size (`size(65f, 70f).padLeft(6f)`).
const HOST_HELP_WIDTH: f32 = 65.0;
/// Upstream `Core.settings.getInt("port", port)` default (`HostDialog.java:57`).
const HOST_DEFAULT_PORT: i64 = 6567;
/// `maxLength(40)` on the player-name field (`HostDialog.java:24`).
const HOST_NAME_MAX_LENGTH: i32 = 40;
/// `Core.settings` `name` key (`HostDialog.java:24`).
pub(super) const HOST_NAME_SETTING: &str = "name";
/// `Core.settings` `color-0` key (`HostDialog.java:31`).
pub(super) const HOST_COLOR_SETTING: &str = "color-0";
/// `Core.settings` `port` key (`HostDialog.java:56`).
const HOST_PORT_SETTING: &str = "port";
/// Per-controller generation (`ready` re-runs on an extension reload).
const HOST_GENERATION_META: &str = "_mind_batch_h_host_generation";
/// The installed picker `color_selected` callable.
const HOST_PICKER_HOOK: &str = "_mind_batch_h_host_picker_hook";

/// `HostDialog` state: the validated fields and the colour swatch.
struct HostState {
    name_field: Option<Gd<LineEdit>>,
    port_field: Option<Gd<LineEdit>>,
    host_button: Option<Gd<Button>>,
    swatch_tint: Option<Gd<ColorRect>>,
    /// Player colour shown in the swatch (`player.color()`).
    color: Color,
    generation: i64,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            name_field: None,
            port_field: None,
            host_button: None,
            swatch_tint: None,
            color: Color::WHITE,
            generation: 0,
        }
    }
}

/// `HostDialog` — host configuration (name + colour + port) and the `@host`
/// intent. The dialog emits `host_requested`; `UiRoot` turns that into the
/// `MindNet.host_current_world` call (`HostDialog.runHost`).
pub struct HostDialogController {
    state: Rc<RefCell<HostState>>,
}

impl Default for HostDialogController {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(HostState::default())),
        }
    }
}

impl DialogController for HostDialogController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@hostserver");
        dialog.should_pause = false;
        let dialog_gd = dialog.to_gd();
        self.state.borrow_mut().generation = next_generation(&dialog_gd, HOST_GENERATION_META);
        host_build(dialog, &self.state);
        dialog.add_close_button(210.0);
        host_hook_picker(&dialog_gd, &self.state);
    }
}

/// `HostDialog._build`: Java's floating rows — a 300x70 name + colour table, a
/// 300x70 port table, then `[65 spacer][@host 300x70][? 65x70]`
/// (`HostDialog.java:22-73`). The dialog is a `setFillParent(true)` BaseDialog,
/// so the widgets float over the paused menu with no boxed panel.
fn host_build(dialog: &mut MindDialog, state: &Rc<RefCell<HostState>>) {
    dialog.clear_content();
    dialog.clear_buttons();
    // `BaseDialog.setFillParent(true)` (`Dialog.java:22`): full-screen shell,
    // `Styles.defaultDialog` = `windowEmpty`, so there is no visible box.
    dialog.enable_full_dialog(true);
    // Arc's `cont` centres its single `expand().fill()` cell.
    dialog.apply_cont_top(false);
    let dialog_gd = dialog.to_gd();
    let mut table = dialog.content_table();

    // ---- name + colour row: `cont.table(t -> ...).width(w).height(70f)` ----
    let name_hint = kit::t("@name").to_string();
    // code-instantiated: the 300x70 Java row (a code-built composite).
    let mut name_row = HBoxContainer::new_alloc();
    name_row.set_name("host-name-row");
    name_row.set_custom_minimum_size(Vector2::new(HOST_WIDTH, HOST_ROW_HEIGHT));
    name_row.add_theme_constant_override("separation", 0);
    // `t.add("@name").padRight(10)`
    name_row.add_child(&host_label_cell(&name_hint, 10));
    let mut name_field = factory::field(&name_hint);
    name_field.set_name("host-name-field");
    name_field.set_max_length(HOST_NAME_MAX_LENGTH);
    // `HostDialog.java:24`: the field opens with the remembered name.
    name_field.set_text(&GString::from(stored_player_name().as_str()));
    name_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("host_store_name", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| variant_string(value))
                .unwrap_or_default();
            host_store_name(&state_for, &text);
            Variant::nil()
        });
        name_field.connect("text_changed", &callable);
    }
    // `t.field(...).grow().pad(8)`
    name_row.add_child(&host_field_cell(&name_field, 8));
    // `t.button(Tex.whiteui, Styles.squarei, 40, ...).size(54f)`: tinted at
    // runtime by the colour chosen in the `picker` dialog
    // (`HostDialog.java:28-34`).
    // code-instantiated: the square colour swatch is tinted from the setting.
    let mut swatch = Button::new_alloc();
    swatch.set_name("host-swatch");
    swatch.set_theme_type_variation("squarei");
    swatch.set_custom_minimum_size(Vector2::new(54.0, 54.0));
    swatch.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    let color = stored_player_color();
    let mut tint = ColorRect::new_alloc();
    tint.set_color(color);
    tint.set_mouse_filter(MouseFilter::IGNORE);
    let tint_control: Gd<Control> = tint.clone().upcast();
    swatch.add_child(&tint_control);
    tint.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    {
        let dialog_for = dialog_gd.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("host_open_picker", move |_args: &[&Variant]| {
            host_open_picker(&dialog_for, &state_for);
            Variant::nil()
        });
        swatch.connect("pressed", &callable);
    }
    name_row.add_child(&swatch);
    table
        .bind_mut()
        .add(&name_row)
        .width(HOST_WIDTH)
        .height(HOST_ROW_HEIGHT)
        .colspan(3)
        .pad(4.0);
    table.bind_mut().row();
    {
        let mut st = state.borrow_mut();
        st.name_field = Some(name_field);
        st.swatch_tint = Some(tint);
        st.color = color;
    }

    // ---- port row: `cont.table(t -> ...).width(w).height(70f)` ----
    let port_hint = kit::t("@server.port").to_string();
    // code-instantiated: the 300x70 Java port row.
    let mut port_row = HBoxContainer::new_alloc();
    port_row.set_name("host-port-row");
    port_row.set_custom_minimum_size(Vector2::new(HOST_WIDTH, HOST_ROW_HEIGHT));
    port_row.add_theme_constant_override("separation", 0);
    // `t.add("@server.port").padRight(10)`
    port_row.add_child(&host_label_cell(&port_hint, 10));
    let mut port_field = factory::field("");
    port_field.set_name("host-port-field");
    let stored_port =
        settings_int(HOST_PORT_SETTING, HOST_DEFAULT_PORT).unwrap_or(HOST_DEFAULT_PORT);
    let port = if (1..=65535).contains(&stored_port) {
        stored_port
    } else {
        HOST_DEFAULT_PORT
    };
    port_field.set_text(&GString::from(port.to_string().as_str()));
    port_field.set_max_length(5);
    port_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("host_port_changed", move |_args: &[&Variant]| {
            host_on_port_changed(&state_for);
            Variant::nil()
        });
        port_field.connect("text_changed", &callable);
    }
    // `portField[0] = t.field(...).pad(8).grow().maxTextLength(5).valid(...)`
    port_row.add_child(&host_field_cell(&port_field, 8));
    table
        .bind_mut()
        .add(&port_row)
        .width(HOST_WIDTH)
        .height(HOST_ROW_HEIGHT)
        .colspan(3)
        .pad(4.0);
    table.bind_mut().row();
    state.borrow_mut().port_field = Some(port_field);

    // ---- `cont.add().width(65f)` + `@host` + `?` (`HostDialog.java:66-73`) ----
    // code-instantiated: the empty 65 px spacer cell.
    let mut spacer = Control::new_alloc();
    spacer.set_name("host-spacer");
    table.bind_mut().add(&spacer).width(HOST_SPACER_WIDTH);
    // code-instantiated: the host action button is created here because the
    // validated port field gates its disabled state at runtime.
    let mut host_button = factory::button(&kit::t("@host").to_string());
    host_button.set_name("host-button");
    {
        let dialog_for = dialog_gd.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("host_confirm", move |_args: &[&Variant]| {
            host_confirm(&dialog_for, &state_for);
            Variant::nil()
        });
        host_button.connect("pressed", &callable);
    }
    table
        .bind_mut()
        .add(&host_button)
        .width(HOST_WIDTH)
        .height(HOST_ROW_HEIGHT);
    // `if(!steam) cont.button("?", () -> ui.showInfo("@host.info"))...`
    let mut help = factory::button("?");
    help.set_name("host-help");
    {
        let callable = Callable::from_fn("host_info", move |_args: &[&Variant]| {
            let _ = ui_call("show_info", &[kit::t("@host.info").to_variant()]);
            Variant::nil()
        });
        help.connect("pressed", &callable);
    }
    table
        .bind_mut()
        .add(&help)
        .width(HOST_HELP_WIDTH)
        .height(HOST_ROW_HEIGHT)
        .pad_left(6.0);
    state.borrow_mut().host_button = Some(host_button);
    host_on_port_changed(state);
}

/// A Java row label cell (`t.add("@name").padRight(10)`): the label keeps its
/// minimum width and centres vertically in the 70 px row.
fn host_label_cell(text: &str, pad_right: i32) -> Gd<MarginContainer> {
    // code-instantiated: per-open row label cell.
    let mut margin = MarginContainer::new_alloc();
    margin.add_theme_constant_override("margin_right", pad_right);
    // code-instantiated: vertical centring box for the label.
    let mut center = VBoxContainer::new_alloc();
    center.set_alignment(AlignmentMode::CENTER);
    let mut label = factory::label(text);
    label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    center.add_child(&label);
    margin.add_child(&center);
    margin
}

/// A Java field cell (`t.field(...).pad(8).grow()`): 8 px side pads with the
/// field centred in the 70 px row.
fn host_field_cell(field: &Gd<LineEdit>, pad: i32) -> Gd<MarginContainer> {
    // code-instantiated: per-open field cell.
    let mut margin = MarginContainer::new_alloc();
    for side in ["margin_left", "margin_right"] {
        margin.add_theme_constant_override(side, pad);
    }
    margin.set_h_size_flags(SizeFlags::EXPAND_FILL);
    // code-instantiated: vertical centring box for the field.
    let mut center = VBoxContainer::new_alloc();
    center.set_alignment(AlignmentMode::CENTER);
    center.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut field = field.clone();
    field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    center.add_child(&field);
    margin.add_child(&center);
    margin
}

/// `HostDialog.portField.valid`: an integer in `1..=65535`.
fn host_port_valid(st: &HostState) -> bool {
    let Some(field) = st.port_field.clone() else {
        return true;
    };
    let text = field.get_text().to_string();
    match text.parse::<i64>() {
        Ok(port) => (1..=65535).contains(&port),
        Err(_) => false,
    }
}

/// `_on_port_changed`: the host entry is disabled while the port is invalid
/// (`HostDialog.java:56` also persists the parsed port on every edit).
fn host_on_port_changed(state: &Rc<RefCell<HostState>>) {
    let (valid, port, button) = {
        let st = state.borrow();
        let port = st
            .port_field
            .clone()
            .and_then(|field| field.get_text().to_string().parse::<i64>().ok());
        (host_port_valid(&st), port, st.host_button.clone())
    };
    if let Some(mut button) = button {
        button.set_disabled(!valid);
    }
    if let Some(port) = port
        && valid
    {
        settings_put_int(HOST_PORT_SETTING, port);
    }
}

/// `_open_picker`: open the colour picker and make sure its `color_selected`
/// reaches this generation of the dialog.
fn host_open_picker(dialog: &Gd<MindDialog>, state: &Rc<RefCell<HostState>>) {
    host_hook_picker(dialog, state);
    open_named_dialog("picker", "{}");
}

/// `HostDialog.java:24-27`: every name edit updates `player.name` and the
/// `name` setting immediately.
fn host_store_name(state: &Rc<RefCell<HostState>>, text: &str) {
    settings_put_text(HOST_NAME_SETTING, text);
    host_push_identity(state);
}

/// `HostDialog.java:31-33`: picking a colour updates `player.color` and the
/// `color-0` setting immediately.
fn host_store_color(state: &Rc<RefCell<HostState>>, color: Color) {
    {
        let mut st = state.borrow_mut();
        st.color = color;
        if let Some(mut tint) = st.swatch_tint.clone() {
            tint.set_color(color);
        }
    }
    settings_put_int(HOST_COLOR_SETTING, color_rgba32(color));
    host_push_identity(state);
}

/// Pushes `player.name`/`player.color` to the relay identity reducers.
fn host_push_identity(state: &Rc<RefCell<HostState>>) {
    let st = state.borrow();
    let Some(field) = st.name_field.clone() else {
        return;
    };
    let name = field.get_text().to_string();
    push_player_identity(&name, st.color);
}

/// Hooks the `picker` dialog's `color_selected` (once per controller
/// generation; a re-hook replaces the previous callable).
fn host_hook_picker(dialog: &Gd<MindDialog>, state: &Rc<RefCell<HostState>>) {
    let Some(mut picker) = dialog_node("picker") else {
        return;
    };
    if !picker.has_signal("color_selected") {
        return;
    }
    let mut node = dialog.clone();
    if node.has_meta(HOST_PICKER_HOOK) {
        if let Ok(previous) = node.get_meta(HOST_PICKER_HOOK).try_to::<Callable>()
            && picker.is_connected("color_selected", &previous)
        {
            picker.disconnect("color_selected", &previous);
        }
        node.remove_meta(HOST_PICKER_HOOK);
    }
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("host_color_selected", move |args: &[&Variant]| {
        let generation = state_for.borrow().generation;
        if !generation_live(&dialog_for, HOST_GENERATION_META, generation) {
            return Variant::nil();
        }
        let color = args
            .first()
            .and_then(|value| value.try_to::<Color>().ok())
            .unwrap_or(Color::WHITE);
        host_store_color(&state_for, color);
        Variant::nil()
    });
    picker.connect("color_selected", &callable);
    node.set_meta(HOST_PICKER_HOOK, &callable.to_variant());
}

/// `HostDialog._host`: validate, push identity, emit `host_requested`, close.
fn host_confirm(dialog: &Gd<MindDialog>, state: &Rc<RefCell<HostState>>) {
    let (valid, name) = {
        let st = state.borrow();
        (
            host_port_valid(&st),
            st.name_field
                .clone()
                .map(|field| field.get_text().to_string())
                .unwrap_or_default(),
        )
    };
    if !valid {
        return;
    }
    if name.trim().is_empty() {
        // `HostDialog.java:68-71`: an empty name shows `@noname` instead of hosting.
        ui_show_info(&kit::t("@noname").to_string());
        return;
    }
    host_push_identity(state);
    emit_dialog_signal(
        dialog,
        "host_requested",
        // `HostDialog.runHost` hosts the currently loaded world; the second
        // signal argument is kept for ABI compatibility (the mode row is not
        // part of Java's HostDialog).
        &[
            GString::from(name.as_str()).to_variant(),
            GString::new().to_variant(),
        ],
    );
    // `HostDialog.runHost` hides the dialog once the host call is dispatched.
    if !close_named_dialog("host") {
        hide_dialog_node(dialog);
    }
}
