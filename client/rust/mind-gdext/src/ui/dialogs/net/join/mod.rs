// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/JoinDialog.java (77-775).

//! The join dialog controller: the saved-server browser, the community list and
//! the server probe/version gate integration; `version_gate` holds the version
//! classification helpers.
//! Upstream: ui/dialogs/JoinDialog.java

use std::cell::RefCell;
use std::rc::Rc;

use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, ColorRect, Control, HBoxContainer, Json, LineEdit, Timer, VBoxContainer,
};
use godot::global::HorizontalAlignment;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::gdutil::variant::{
    dict_i64_or, dict_text_or, parse_array, parse_json_object, to_bool_lenient, to_text_lenient,
};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::game::util::{
    dialog_node, open_named_dialog, ui_show_confirm, ui_show_info,
};
use crate::ui::dialogs::net::util::{
    DEFAULT_DB_NAME, DEFAULT_SERVER_ADDRESS, DEFAULT_SERVER_PORT, IP_SETTING, JOIN_GENERATION_META,
    PLAYER_COLOR_SETTING, PLAYER_NAME_MAX_LENGTH, PLAYER_NAME_SETTING, PROBE_POLL_SECONDS,
    SERVERS_SETTING, clear_table_nodes, close_named_dialog, color_rgba32, dict_string_or,
    emit_dialog_signal, generation_live, hide_dialog_node, next_generation, push_player_identity,
    settings_bool, settings_put_int, settings_put_text, settings_text, stored_player_color,
    stored_player_name, ui_has_method,
};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable, styles};

use crate::ui::dialogs::net::join::version_gate::{join_abort_on_version, join_version_string};

pub mod version_gate;

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "join" => Some(Box::new(JoinDialogController::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// JoinDialogController (manifest id `join`)
// ---------------------------------------------------------------------------

/// One persisted server (`Java JoinDialog.Server` shape, `{"ip","port"}`).
#[derive(Clone, Default)]
struct JoinServer {
    ip: String,
    port: i64,
}

/// `JoinDialog` state: the persisted servers, the header/search handles and the
/// last rendered probe signature (`_servers`/`_server_signature`/…).
#[derive(Default)]
struct JoinState {
    /// Player colour shown in the header swatch (`player.color()`); seeded from
    /// `color-0` while the header row is built.
    color: Color,
    table: Option<Gd<MindTable>>,
    name_field: Option<Gd<LineEdit>>,
    swatch_tint: Option<Gd<ColorRect>>,
    add_panel: Option<Gd<VBoxContainer>>,
    ip_field: Option<Gd<LineEdit>>,
    local_body: Option<Gd<VBoxContainer>>,
    remote_body: Option<Gd<VBoxContainer>>,
    global_list: Option<Gd<MindTable>>,
    /// Header/search/body controls of the community section (`_global_nodes`).
    global_nodes: Vec<Gd<Control>>,
    /// Persisted servers (`[{"ip": String, "port": int}]`).
    servers: Vec<JoinServer>,
    /// Signature of the last rendered probe state (`_server_signature`).
    signature: String,
    /// Add-panel target: `-1` = append, else the server index being edited.
    editing_index: i64,
    /// Pending delete confirmation target (`_pending_delete`).
    pending_delete: i64,
    server_search: String,
    generation: i64,
}

impl Default for JoinDialogController {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(JoinState::default())),
        }
    }
}

/// `JoinDialog` — server browser over SpacetimeDB addresses.
///
/// Saved servers persist under the Java `servers` settings JSON as
/// `[{"ip": ..., "port": N}]`; each row probes its address through
/// `MindNet.server_probe_json` and lists the public `all_matches` rows (Java's
/// `net.pingHost` + `setupServer`). "Community" is the active server's match
/// list: the port never fetches the Java community URLs because they serve Java
/// servers the relay cannot talk to (I08 deviation).
pub struct JoinDialogController {
    state: Rc<RefCell<JoinState>>,
}

impl DialogController for JoinDialogController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@joingame");
        dialog.should_pause = false;
        // `makeButtonOverlay()` (`JoinDialog.java:59`): the padded title cell.
        dialog.apply_button_overlay(true);
        dialog.enable_full_dialog(true);
        let dialog_gd = dialog.to_gd();
        self.state.borrow_mut().generation = next_generation(&dialog_gd, JOIN_GENERATION_META);
        join_load_servers(&self.state);
        join_build(dialog, &self.state);
        join_connect_confirm(&dialog_gd, &self.state);
        join_hook_picker(&dialog_gd, &self.state);
        join_install_poll(&dialog_gd, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let dialog_gd = dialog.to_gd();
        join_shown(&dialog_gd, &self.state);
    }
}

/// `JoinDialog._build`.
fn join_build(dialog: &mut MindDialog, state: &Rc<RefCell<JoinState>>) {
    dialog.clear_content();
    dialog.clear_buttons();
    let dialog_gd = dialog.to_gd();
    let mut table = dialog.content_table();
    table.add_theme_constant_override("separation", 8);
    state.borrow_mut().table = Some(table);
    join_build_header(&dialog_gd, state);
    join_build_add_panel(&dialog_gd, state);
    join_build_sections(&dialog_gd, state);

    let dialog_for_back = dialog_gd.clone();
    let back = Callable::from_fn("join_close", move |_args: &[&Variant]| {
        join_close(&dialog_for_back);
        Variant::nil()
    });
    dialog.add_button(&kit::t("@back").to_string(), Some(back), "left", 210.0);

    // `add_button(_t("@server.add"), _toggle_add, "add")` passes no width, so
    // the entry keeps its natural size (`MindWidgets.icon_button`).
    // code-instantiated: the add entry toggles the inline form at runtime.
    let mut add = factory::icon_button(
        "add",
        &kit::t("@server.add").to_string(),
        "defaultt",
        AlignmentMode::BEGIN,
    );
    {
        let dialog_for = dialog_gd.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("join_toggle_add", move |_args: &[&Variant]| {
            join_toggle_add(&dialog_for, &state_for);
            Variant::nil()
        });
        add.connect("pressed", &callable);
    }
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&add);
    }

    let dialog_for_info = dialog_gd.clone();
    let info = Callable::from_fn("join_info", move |_args: &[&Variant]| {
        let _ = &dialog_for_info;
        join_show_info();
        Variant::nil()
    });
    dialog.add_button("?", Some(info), "", 60.0);
}

/// The `@name` header row: label, remembered name and the square colour swatch
/// (`JoinDialog.setup`).
fn join_build_header(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let name_hint = kit::t("@name").to_string();
    // code-instantiated: the name/colour header row is data-driven and has no
    // static scene counterpart.
    let mut header = HBoxContainer::new_alloc();
    header.add_theme_constant_override("separation", 6);
    header.add_child(&factory::label(&name_hint));
    let mut name_field = factory::field(&name_hint);
    name_field.set_max_length(PLAYER_NAME_MAX_LENGTH);
    // `JoinDialog.java:341`: the field opens with the remembered name.
    name_field.set_text(&GString::from(stored_player_name().as_str()));
    name_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("join_store_name", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| to_text_lenient(value))
                .unwrap_or_default();
            join_store_name(&state_for, &text);
            Variant::nil()
        });
        name_field.connect("text_changed", &callable);
    }
    header.add_child(&name_field);

    // code-instantiated: the square colour swatch is tinted at runtime by the
    // colour chosen in the `picker` dialog.
    let mut swatch = Button::new_alloc();
    swatch.set_theme_type_variation("squarei");
    swatch.set_custom_minimum_size(Vector2::new(54.0, 54.0));
    let color = stored_player_color();
    let mut tint = ColorRect::new_alloc();
    tint.set_color(color);
    tint.set_mouse_filter(MouseFilter::IGNORE);
    let tint_control: Gd<Control> = tint.clone().upcast();
    swatch.add_child(&tint_control);
    tint.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    {
        let dialog_for = dialog.clone();
        let callable = Callable::from_fn("join_open_picker", move |_args: &[&Variant]| {
            open_named_dialog("picker", "{}");
            let _ = &dialog_for;
            Variant::nil()
        });
        swatch.connect("pressed", &callable);
    }
    header.add_child(&swatch);

    let mut st = state.borrow_mut();
    st.color = color;
    st.name_field = Some(name_field);
    st.swatch_tint = Some(tint);
    if let Some(mut table) = st.table.clone() {
        table.bind_mut().add(&header).grow_x().pad(4.0);
        table.bind_mut().row();
    }
}

/// The inline add/edit-server form (`JoinDialog.add`, `JoinDialog.java:77-112`).
fn join_build_add_panel(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let ip_hint = kit::t("@joingame.ip").to_string();
    // code-instantiated: the inline add-server form is revealed by `@server.add`
    // and toggled at runtime.
    let mut panel = VBoxContainer::new_alloc();
    panel.set_visible(false);
    panel.add_theme_constant_override("separation", 4);
    let mut ip_row = HBoxContainer::new_alloc();
    ip_row.add_theme_constant_override("separation", 6);
    ip_row.add_child(&factory::label(&ip_hint));
    let mut ip_field = factory::field(&ip_hint);
    ip_field.set_max_length(100);
    ip_field.set_h_size_flags(SizeFlags::EXPAND_FILL);
    {
        // `JoinDialog.java:81-83`: the field remembers the last address typed.
        let callable = Callable::from_fn("join_store_ip", |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| to_text_lenient(value))
                .unwrap_or_default();
            settings_put_text(IP_SETTING, &text);
            Variant::nil()
        });
        ip_field.connect("text_changed", &callable);
    }
    ip_row.add_child(&ip_field);
    panel.add_child(&ip_row);
    let mut actions = HBoxContainer::new_alloc();
    actions.add_theme_constant_override("separation", 6);
    let mut ok = factory::button(&kit::t("@ok").to_string());
    {
        let dialog_for = dialog.clone();
        let state_for = state.clone();
        let callable = Callable::from_fn("join_add_ok", move |_args: &[&Variant]| {
            join_submit_add(&dialog_for, &state_for);
            Variant::nil()
        });
        ok.connect("pressed", &callable);
    }
    actions.add_child(&ok);
    let mut cancel = factory::button(&kit::t("@cancel").to_string());
    {
        let state_for = state.clone();
        let callable = Callable::from_fn("join_add_cancel", move |_args: &[&Variant]| {
            join_hide_add(&state_for);
            Variant::nil()
        });
        cancel.connect("pressed", &callable);
    }
    actions.add_child(&cancel);
    panel.add_child(&actions);
    let mut st = state.borrow_mut();
    st.add_panel = Some(panel.clone());
    st.ip_field = Some(ip_field);
    if let Some(mut table) = st.table.clone() {
        table.bind_mut().add(&panel).grow_x().pad(4.0);
        table.bind_mut().row();
    }
}

/// The three server groups (`JoinDialog.section`): local, remote, global.
fn join_build_sections(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    if let Some(local) = join_add_section(dialog, state, "@servers.local", false) {
        state.borrow_mut().local_body = Some(local);
    }
    if let Some(remote) = join_add_section(dialog, state, "@servers.remote", false) {
        state.borrow_mut().remote_body = Some(remote);
    }
    join_rebuild_servers(state, dialog);

    let first_global = state
        .borrow()
        .table
        .as_ref()
        .map(|table| table.get_child_count())
        .unwrap_or(0);
    if let Some(mut global) = join_add_section(dialog, state, "@servers.global", true) {
        // The community section's header/search/body nodes, hidden while
        // `communityservers` is off (`JoinDialog.java:329-331`).
        let mut nodes = Vec::new();
        if let Some(table) = state.borrow().table.clone() {
            let count = table.get_child_count();
            for index in first_global..count {
                if let Some(child) = table.get_child(index)
                    && let Ok(control) = child.try_cast::<Control>()
                {
                    nodes.push(control);
                }
            }
        }
        state.borrow_mut().global_nodes = nodes;
        // code-instantiated: the community body lists the active server's public
        // `all_matches` rows; it is Java's community list minus the remote fetch.
        let mut list = factory::table();
        list.set_h_size_flags(SizeFlags::EXPAND_FILL);
        global.add_child(&list);
        state.borrow_mut().global_list = Some(list);
    }
    join_rebuild_public(dialog, state);
}

/// One section header (accent title + collapse glyph, optional search row) plus
/// its collapsible body container.
fn join_add_section(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    label_key: &str,
    with_search: bool,
) -> Option<Gd<VBoxContainer>> {
    // code-instantiated: section header + collapsible body repeat for the three
    // server groups and are toggled at runtime.
    let mut section_body = VBoxContainer::new_alloc();
    section_body.add_theme_constant_override("separation", 4);
    let mut header = HBoxContainer::new_alloc();
    header.add_theme_constant_override("separation", 4);
    let mut title = factory::styled_label(&kit::tm(label_key).to_string(), "outlineLabel");
    title.set_h_size_flags(SizeFlags::EXPAND_FILL);
    title.set_horizontal_alignment(HorizontalAlignment::LEFT);
    title.add_theme_color_override("default_color", styles::ACCENT);
    header.add_child(&title);
    let mut toggle = Button::new_alloc();
    toggle.set_theme_type_variation("emptyi");
    toggle.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    // code-instantiated: the collapse glyph swaps between up-open/down-open.
    let toggle_glyph = factory::glyph_label("up-open", 22, styles::ACCENT);
    toggle.add_child(&toggle_glyph);
    {
        let body_for = section_body.clone();
        let glyph_for = toggle_glyph.clone();
        let callable = Callable::from_fn("join_toggle_section", move |_args: &[&Variant]| {
            let mut body = body_for.clone();
            let visible = !body.is_visible();
            body.set_visible(visible);
            let mut glyph = glyph_for.clone();
            glyph.set_text(&kit::glyph(if visible { "up-open" } else { "down-open" }));
            Variant::nil()
        });
        toggle.connect("pressed", &callable);
    }
    header.add_child(&toggle);

    let mut table = state.borrow().table.clone()?;
    table.bind_mut().add(&header).grow_x().pad(4.0);
    table.bind_mut().row();

    if with_search {
        // code-instantiated: the community search/refresh row is data-driven
        // (`JoinDialog.java:417-424`).
        let search_hint = kit::t("@search").to_string();
        let mut search_row = HBoxContainer::new_alloc();
        search_row.add_theme_constant_override("separation", 6);
        search_row.add_child(&factory::label(&search_hint));
        let mut search = factory::field(&search_hint);
        search.set_max_length(50);
        search.set_h_size_flags(SizeFlags::EXPAND_FILL);
        {
            let dialog_for = dialog.clone();
            let state_for = state.clone();
            let callable = Callable::from_fn("join_search", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| to_text_lenient(value))
                    .unwrap_or_default();
                join_on_search_changed(&dialog_for, &state_for, &text);
                Variant::nil()
            });
            search.connect("text_changed", &callable);
        }
        search_row.add_child(&search);
        let mut zoom = factory::icon_button("zoom", "", "emptyi", AlignmentMode::CENTER);
        zoom.set_custom_minimum_size(Vector2::new(54.0, 54.0));
        {
            let dialog_for = dialog.clone();
            let state_for = state.clone();
            let callable = Callable::from_fn("join_public_refresh", move |_args: &[&Variant]| {
                join_rebuild_public(&dialog_for, &state_for);
                Variant::nil()
            });
            zoom.connect("pressed", &callable);
        }
        search_row.add_child(&zoom);
        table.bind_mut().add(&search_row).grow_x().pad(4.0);
        table.bind_mut().row();
    }

    table.bind_mut().add(&section_body).grow_x().pad(4.0);
    table.bind_mut().row();
    Some(section_body)
}

/// Rebuilds both server sections from the live probe data.
fn join_rebuild_servers(state: &Rc<RefCell<JoinState>>, dialog: &Gd<MindDialog>) {
    let (local, remote, servers) = {
        let st = state.borrow();
        (
            st.local_body.clone(),
            st.remote_body.clone(),
            st.servers.clone(),
        )
    };
    if let Some(mut body) = local
        && body.is_instance_valid()
    {
        kit::clear_children(&mut body);
        join_build_server_row(state, dialog, &mut body, DEFAULT_SERVER_ADDRESS, -1);
    }
    if let Some(mut body) = remote
        && body.is_instance_valid()
    {
        kit::clear_children(&mut body);
        for (index, entry) in servers.iter().enumerate() {
            join_build_server_row(state, dialog, &mut body, &entry.ip, index as i64);
        }
    }
}

/// One server row: address header + Java's action buttons
/// (`JoinDialog.java:150-204`) followed by the probe content.
fn join_build_server_row(
    state: &Rc<RefCell<JoinState>>,
    dialog: &Gd<MindDialog>,
    parent: &mut Gd<VBoxContainer>,
    address: &str,
    index: i64,
) {
    // code-instantiated: one server row per saved server plus the local default;
    // rows rebuild whenever a probe's state changes.
    let mut row = VBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 2);
    let mut header = HBoxContainer::new_alloc();
    header.add_theme_constant_override("separation", 4);
    let mut label = factory::styled_label(address, "outlineLabel");
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_horizontal_alignment(HorizontalAlignment::LEFT);
    label.add_theme_color_override("default_color", styles::ACCENT);
    header.add_child(&label);
    if index >= 0 {
        header.add_child(&join_row_action(
            "up-open",
            join_move_action(dialog, state, index, -1),
        ));
        header.add_child(&join_row_action(
            "down-open",
            join_move_action(dialog, state, index, 1),
        ));
    }
    header.add_child(&join_row_action(
        "refresh",
        join_refresh_action(dialog, state, address),
    ));
    if index >= 0 {
        header.add_child(&join_row_action(
            "pencil",
            join_edit_action(dialog, state, index),
        ));
        header.add_child(&join_row_action(
            "trash",
            join_remove_action(dialog, state, index),
        ));
    }
    row.add_child(&header);
    join_build_server_content(state, dialog, &mut row, address);
    parent.add_child(&row);
}

/// One row action button (`Styles.emptyi` + icon glyph).
fn join_row_action(icon: &str, listener: Callable) -> Gd<Button> {
    // code-instantiated: per-server row action buttons (reorder/refresh/edit/remove).
    let mut button = factory::icon_button(icon, "", "emptyi", AlignmentMode::CENTER);
    button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
    button.connect("pressed", &listener);
    button
}

fn join_move_action(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    index: i64,
    direction: i64,
) -> Callable {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    Callable::from_fn("join_move_server", move |_args: &[&Variant]| {
        join_move_server(&dialog_for, &state_for, index, direction);
        Variant::nil()
    })
}

fn join_refresh_action(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    address: &str,
) -> Callable {
    let state_for = state.clone();
    let address_for = address.to_string();
    let _ = dialog;
    Callable::from_fn("join_refresh_server", move |_args: &[&Variant]| {
        join_refresh_server(&state_for, &address_for);
        Variant::nil()
    })
}

fn join_edit_action(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    index: i64,
) -> Callable {
    let state_for = state.clone();
    let _ = dialog;
    Callable::from_fn("join_edit_server", move |_args: &[&Variant]| {
        join_edit_server(&state_for, index);
        Variant::nil()
    })
}

fn join_remove_action(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    index: i64,
) -> Callable {
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    Callable::from_fn("join_remove_server", move |_args: &[&Variant]| {
        join_remove_server(&dialog_for, &state_for, index);
        Variant::nil()
    })
}

/// Appends one server's probe content to its row.
fn join_build_server_content(
    state: &Rc<RefCell<JoinState>>,
    dialog: &Gd<MindDialog>,
    parent: &mut Gd<VBoxContainer>,
    address: &str,
) {
    let probe = parse_json_object(&join_probe_json(address));
    let state_name = dict_string_or(&probe, "state", "none");
    let ping = dict_i64_or(&probe, "ping_ms", -1);
    let matches = probe
        .get("matches")
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default();
    match state_name.as_str() {
        "ready" => {
            // `buildServer` shows the host's `getVersionString` (version and
            // the `@server.outdated` badge when the build differs; GAPS #45).
            if let Some(first) = matches
                .iter_shared()
                .find_map(|entry| entry.try_to::<VarDictionary>().ok())
            {
                let (build, protocol) = join_match_version(&first);
                let version = join_version_string(&build, protocol);
                if !version.is_empty() {
                    parent.add_child(&factory::styled_label(&version, "outlineLabel"));
                }
            }
            if ping > 0 {
                // `buildServer` shows `host.ping + "ms"` under the host info.
                parent.add_child(&factory::styled_label(&format!("{ping}ms"), "outlineLabel"));
            }
            if matches.is_empty() {
                parent.add_child(&factory::styled_label(
                    &kit::tm("@hosts.none").to_string(),
                    "outlineLabel",
                ));
            } else {
                for entry in matches.iter_shared() {
                    let Ok(row) = entry.try_to::<VarDictionary>() else {
                        continue;
                    };
                    let match_id = dict_i64_or(&row, "match_id", 0);
                    if match_id <= 0 {
                        continue;
                    }
                    let label = format!(
                        "{}  {}  {}/{}",
                        dict_text_or(&row, "map_id"),
                        dict_text_or(&row, "mode_name"),
                        dict_i64_or(&row, "player_count", 0),
                        dict_i64_or(&row, "max_players", 0),
                    );
                    let mut button = factory::button(&label);
                    let dialog_for = dialog.clone();
                    let state_for = state.clone();
                    let address_for = address.to_string();
                    let callable =
                        Callable::from_fn("join_server_match", move |_args: &[&Variant]| {
                            join_on_server(&dialog_for, &state_for, &address_for, match_id);
                            Variant::nil()
                        });
                    button.connect("pressed", &callable);
                    parent.add_child(&button);
                }
            }
        }
        "connecting" => {
            // `refreshServer`: `@server.refreshing` while the ping is in flight.
            parent.add_child(&factory::styled_label(
                &kit::tm("@server.refreshing").to_string(),
                "outlineLabel",
            ));
        }
        _ => {
            // `refreshServer` failure: `@host.invalid`.
            parent.add_child(&factory::styled_label(
                &kit::tm("@host.invalid").to_string(),
                "outlineLabel",
            ));
        }
    }
}

/// Global group body: the active server's public `all_matches` rows, filtered by
/// the community search field.
fn join_rebuild_public(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let list = state.borrow().global_list.clone();
    let Some(mut list) = list else {
        return;
    };
    if !list.is_instance_valid() {
        return;
    }
    clear_table_nodes(&mut list);
    let matches = join_public_matches();
    let search = state.borrow().server_search.clone();
    for row_variant in matches.iter_shared() {
        let Ok(row) = row_variant.try_to::<VarDictionary>() else {
            continue;
        };
        let match_id = dict_i64_or(&row, "match_id", 0);
        if match_id <= 0 {
            continue;
        }
        let label = format!(
            "{}  {}  {}/{}",
            dict_text_or(&row, "map_id"),
            dict_text_or(&row, "mode_name"),
            dict_i64_or(&row, "player_count", 0),
            dict_i64_or(&row, "max_players", 0),
        );
        if !search.is_empty() && !label.to_lowercase().contains(&search) {
            continue;
        }
        // code-instantiated: public match rows come from the server view.
        let mut button = factory::button(&label);
        {
            let dialog_for = dialog.clone();
            let state_for = state.clone();
            let callable = Callable::from_fn("join_public_match", move |_args: &[&Variant]| {
                join_selected(&dialog_for, &state_for, match_id);
                Variant::nil()
            });
            button.connect("pressed", &callable);
        }
        list.bind_mut().add(&button).grow_x().pad(3.0);
        list.bind_mut().row();
    }
    if list.get_child_count() == 0 {
        // code-instantiated: empty-state row carries the refresh control.
        let mut empty = HBoxContainer::new_alloc();
        empty.add_theme_constant_override("separation", 6);
        let mut empty_label =
            factory::styled_label(&kit::tm("@hosts.none").to_string(), "outlineLabel");
        empty_label.set_autowrap_mode(AutowrapMode::OFF);
        empty_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        empty.add_child(&empty_label);
        let mut refresh = factory::icon_button("refresh", "", "emptyi", AlignmentMode::CENTER);
        refresh.set_custom_minimum_size(Vector2::new(70.0, 70.0));
        {
            let dialog_for = dialog.clone();
            let state_for = state.clone();
            let callable = Callable::from_fn("join_public_refresh", move |_args: &[&Variant]| {
                join_rebuild_public(&dialog_for, &state_for);
                Variant::nil()
            });
            refresh.connect("pressed", &callable);
        }
        empty.add_child(&refresh);
        list.bind_mut().add(&empty).grow_x().pad(4.0);
        list.bind_mut().row();
    }
    list.bind_mut().sort_now();
    let _ = list.call_deferred("sort_now", &[]);
}

/// Live `all_matches` rows from `MindNet` (empty when absent or offline).
fn join_public_matches() -> VarArray {
    let Some(mut net) = kit::net() else {
        return VarArray::new();
    };
    if !net.has_method("get_public_matches_json") {
        return VarArray::new();
    }
    let text = net
        .call("get_public_matches_json", &[])
        .try_to::<GString>()
        .unwrap_or_default();
    parse_array(&text.to_string())
}

/// Polls every row's probe and returns one signature over their rendered state.
fn join_poll_once(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    if !dialog.is_instance_valid() || !dialog.is_visible() {
        return;
    }
    let signature = {
        let st = state.borrow();
        join_signature(&st)
    };
    let changed = {
        let mut st = state.borrow_mut();
        if st.signature == signature {
            false
        } else {
            st.signature = signature;
            true
        }
    };
    if changed {
        join_rebuild_servers(state, dialog);
    }
}

/// The rendered probe signature (`_poll_server_signature`).
fn join_signature(st: &JoinState) -> String {
    let mut signature = format!(
        "{DEFAULT_SERVER_ADDRESS}={};",
        join_probe_json(DEFAULT_SERVER_ADDRESS)
    );
    for entry in &st.servers {
        signature.push_str(&format!("{}={};", entry.ip, join_probe_json(&entry.ip)));
    }
    signature
}

/// One address's probe state as compact JSON (empty when `MindNet` is absent).
fn join_probe_json(address: &str) -> String {
    let Some(mut net) = kit::net() else {
        return "{}".to_string();
    };
    if !net.has_method("server_probe_json") {
        return "{}".to_string();
    }
    net.call("server_probe_json", &[GString::from(address).to_variant()])
        .try_to::<GString>()
        .unwrap_or_default()
        .to_string()
}

/// The first `RelayMatch` of an address's probe (`None` before the probe is
/// ready); carries `build_id`/`protocol_version` for the version gate.
fn join_probe_first_match(address: &str) -> Option<VarDictionary> {
    let probe = parse_json_object(&join_probe_json(address));
    probe
        .get("matches")
        .and_then(|value| value.try_to::<VarArray>().ok())?
        .iter_shared()
        .find_map(|entry| entry.try_to::<VarDictionary>().ok())
}

/// The `all_matches` row with `match_id` (public/browse connect path).
fn join_public_match(match_id: i64) -> Option<VarDictionary> {
    join_public_matches()
        .iter_shared()
        .filter_map(|entry| entry.try_to::<VarDictionary>().ok())
        .find(|row| dict_i64_or(row, "match_id", 0) == match_id)
}

/// `RelayMatch.build_id`/`protocol_version` of one row.
fn join_match_version(row: &VarDictionary) -> (String, i64) {
    (
        dict_text_or(row, "build_id"),
        dict_i64_or(row, "protocol_version", 0),
    )
}

/// Installs the probe poll (`_process`: every 10 frames while visible).
fn join_install_poll(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    // code-instantiated: the async server probes need a periodic poll; the
    // former GDScript used `_process` (the dialog class has no process hook).
    if let Some(mut old) = dialog.get_node_or_null("ProbePoll") {
        let mut node = dialog.clone();
        node.remove_child(&old);
        old.queue_free();
    }
    let mut timer = Timer::new_alloc();
    timer.set_name("ProbePoll");
    timer.set_wait_time(PROBE_POLL_SECONDS);
    timer.set_one_shot(false);
    timer.set_autostart(true);
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("join_probe_poll", move |_args: &[&Variant]| {
        join_poll_once(&dialog_for, &state_for);
        Variant::nil()
    });
    timer.connect("timeout", &callable);
    let mut node = dialog.clone();
    node.add_child(&timer);
}

/// `JoinDialog.shown`: rebuild rows and re-apply the community visibility.
fn join_shown(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    join_rebuild_servers(state, dialog);
    let community = join_community_servers();
    let nodes = state.borrow().global_nodes.clone();
    for node in nodes {
        if !node.is_instance_valid() {
            continue;
        }
        let mut node = node;
        node.set_visible(community);
    }
    if community {
        join_rebuild_public(dialog, state);
    }
}

/// `Core.settings.getBool("communityservers", true)` (Game settings row).
fn join_community_servers() -> bool {
    settings_bool("communityservers").unwrap_or(true)
}

/// `_on_search_changed`: trim, collapse spaces, lowercase (`JoinDialog.java:419-421`).
fn join_on_search_changed(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>, text: &str) {
    let normalized = text
        .trim()
        .to_lowercase()
        .split(' ')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    state.borrow_mut().server_search = normalized;
    join_rebuild_public(dialog, state);
}

/// `JoinDialog.safeConnect` intent: join the picked public match.
fn join_selected(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>, match_id: i64) {
    // `safeConnect(host.address, host.port, host.version)`: a differing build
    // shows the scarlet reason + `@server.versions` instead of joining.
    if let Some(row) = join_public_match(match_id) {
        let (build, protocol) = join_match_version(&row);
        if join_abort_on_version(&build, protocol) {
            return;
        }
    }
    join_push_identity(state);
    emit_dialog_signal(dialog, "join_requested", &[match_id.to_variant()]);
    join_close(dialog);
}

/// Clicking a match under a server row: connect first unless the connector
/// already points there (`JoinDialog.java:150-158`).
fn join_on_server(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    address: &str,
    match_id: i64,
) {
    if match_id <= 0 {
        return;
    }
    // `safeConnect(host.address, host.port, host.version)` (GAPS #45).
    if let Some(row) = join_probe_first_match(address) {
        let (build, protocol) = join_match_version(&row);
        if join_abort_on_version(&build, protocol) {
            return;
        }
    }
    join_push_identity(state);
    let mut current = String::new();
    if let Some(mut net) = kit::net()
        && net.has_method("current_server_address")
    {
        current = net
            .call("current_server_address", &[])
            .try_to::<GString>()
            .unwrap_or_default()
            .to_string();
    }
    if current != join_normalized_address(address) {
        emit_dialog_signal(
            dialog,
            "connect_requested",
            &[GString::from(address).to_variant()],
        );
    }
    emit_dialog_signal(dialog, "join_requested", &[match_id.to_variant()]);
    join_close(dialog);
}

/// Normalizes an address to `host[:port]/db` (no scheme), matching
/// `MindNet.current_server_address` and the Rust parser defaults.
fn join_normalized_address(address: &str) -> String {
    let value = address.trim();
    let value = value.strip_prefix("http://").unwrap_or(value);
    let value = value.strip_prefix("https://").unwrap_or(value);
    let (authority_part, database_part) = match value.split_once('/') {
        Some((authority, rest)) => (authority, rest.split('/').next().unwrap_or("")),
        None => (value, ""),
    };
    let mut authority = authority_part.to_string();
    let database = if database_part.is_empty() {
        DEFAULT_DB_NAME
    } else {
        database_part
    };
    if !authority.contains(':') {
        authority = format!("{authority}:{DEFAULT_SERVER_PORT}");
    }
    format!("{authority}/{database}")
}

/// `JoinDialog.java:83`: the add panel remembers the address field.
fn join_push_identity(state: &Rc<RefCell<JoinState>>) {
    let st = state.borrow();
    let Some(field) = st.name_field.clone() else {
        return;
    };
    let name = field.get_text().to_string();
    push_player_identity(&name, st.color);
}

/// `JoinDialog.java:342-346`: every name edit updates `player.name` and the
/// `name` setting immediately.
fn join_store_name(state: &Rc<RefCell<JoinState>>, text: &str) {
    settings_put_text(PLAYER_NAME_SETTING, text);
    join_push_identity(state);
}

/// `JoinDialog.java:348-354`: picking a colour updates `player.color()` and the
/// `color-0` setting immediately.
fn join_store_color(state: &Rc<RefCell<JoinState>>, color: Color) {
    {
        let mut st = state.borrow_mut();
        st.color = color;
        if let Some(mut tint) = st.swatch_tint.clone() {
            tint.set_color(color);
        }
    }
    settings_put_int(PLAYER_COLOR_SETTING, color_rgba32(color));
    join_push_identity(state);
}

/// Hooks the `picker` dialog's `color_selected` (once per controller generation).
fn join_hook_picker(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let Some(mut picker) = dialog_node("picker") else {
        return;
    };
    if !picker.has_signal("color_selected") {
        return;
    }
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("join_color_selected", move |args: &[&Variant]| {
        let generation = state_for.borrow().generation;
        if !generation_live(&dialog_for, JOIN_GENERATION_META, generation) {
            return Variant::nil();
        }
        let color = args
            .first()
            .and_then(|value| value.try_to::<Color>().ok())
            .unwrap_or(Color::WHITE);
        join_store_color(&state_for, color);
        Variant::nil()
    });
    picker.connect("color_selected", &callable);
}

/// `JoinDialog._on_confirm_result`: answers the `@server.delete` confirm.
fn join_connect_confirm(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal("confirm_result") {
        return;
    }
    let dialog_for = dialog.clone();
    let state_for = state.clone();
    let callable = Callable::from_fn("join_confirm_result", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .map(|value| to_bool_lenient(value))
            .unwrap_or(false);
        let generation = state_for.borrow().generation;
        if !generation_live(&dialog_for, JOIN_GENERATION_META, generation) {
            return Variant::nil();
        }
        let index = {
            let mut st = state_for.borrow_mut();
            let index = st.pending_delete;
            st.pending_delete = -1;
            index
        };
        if confirmed {
            join_delete_server(&dialog_for, &state_for, index);
        }
        Variant::nil()
    });
    ui.connect("confirm_result", &callable);
}

/// `_toggle_add`: reveal the inline form (or hide it when already open).
fn join_toggle_add(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let (visible, panel, ip_field) = {
        let st = state.borrow();
        (
            st.add_panel
                .as_ref()
                .map(|panel| panel.is_visible())
                .unwrap_or(false),
            st.add_panel.clone(),
            st.ip_field.clone(),
        )
    };
    let Some(mut panel) = panel else {
        return;
    };
    if visible {
        join_hide_add(state);
        return;
    }
    state.borrow_mut().editing_index = -1;
    if let Some(text) = settings_text(IP_SETTING, "")
        && let Some(mut field) = ip_field.clone()
    {
        field.set_text(&GString::from(text.as_str()));
    }
    panel.set_visible(true);
    if let Some(mut field) = ip_field {
        field.grab_focus();
    }
    let _ = dialog;
}

/// `_edit_server`: open the form on an existing entry.
fn join_edit_server(state: &Rc<RefCell<JoinState>>, index: i64) {
    let (panel, field, ip) = {
        let st = state.borrow();
        if index < 0 || index >= st.servers.len() as i64 {
            return;
        }
        (
            st.add_panel.clone(),
            st.ip_field.clone(),
            st.servers[index as usize].ip.clone(),
        )
    };
    let Some(mut panel) = panel else {
        return;
    };
    state.borrow_mut().editing_index = index;
    if let Some(mut field) = field {
        field.set_text(&GString::from(ip.as_str()));
        field.grab_focus();
    }
    panel.set_visible(true);
}

/// `_hide_add`: collapse the inline form.
fn join_hide_add(state: &Rc<RefCell<JoinState>>) {
    let mut st = state.borrow_mut();
    if let Some(mut panel) = st.add_panel.clone() {
        panel.set_visible(false);
    }
    st.editing_index = -1;
}

/// `JoinDialog.java:92-101` OK handler: append or replace, save, rebuild rows.
fn join_submit_add(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>) {
    let (address, editing) = {
        let st = state.borrow();
        (
            st.ip_field
                .as_ref()
                .map(|field| field.get_text().to_string().trim().to_string())
                .unwrap_or_default(),
            st.editing_index,
        )
    };
    if address.is_empty() {
        return;
    }
    {
        let mut st = state.borrow_mut();
        if editing >= 0 && editing < st.servers.len() as i64 {
            st.servers[editing as usize].ip = address;
        } else {
            st.servers.push(JoinServer {
                ip: address,
                port: 0,
            });
        }
    }
    join_save_servers(state);
    join_rebuild_servers(state, dialog);
    join_hide_add(state);
}

/// Java `moveRemote` (`JoinDialog.java:206-222`): reorder and persist.
fn join_move_server(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<JoinState>>,
    index: i64,
    direction: i64,
) {
    {
        let mut st = state.borrow_mut();
        let target = index + direction;
        if index < 0 || target < 0 || target >= st.servers.len() as i64 {
            return;
        }
        let entry = st.servers.remove(index as usize);
        st.servers.insert(target as usize, entry);
    }
    join_save_servers(state);
    join_rebuild_servers(state, dialog);
}

/// Java confirm flow (`JoinDialog.java:194-202`) then remove + persist.
fn join_remove_server(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>, index: i64) {
    if !ui_has_method("show_confirm") {
        join_delete_server(dialog, state, index);
        return;
    }
    state.borrow_mut().pending_delete = index;
    ui_show_confirm(&kit::t("@server.delete").to_string());
}

/// Removes the server at `index` and persists (`_delete_server`).
fn join_delete_server(dialog: &Gd<MindDialog>, state: &Rc<RefCell<JoinState>>, index: i64) {
    {
        let mut st = state.borrow_mut();
        if index < 0 || index >= st.servers.len() as i64 {
            return;
        }
        st.servers.remove(index as usize);
    }
    join_save_servers(state);
    join_rebuild_servers(state, dialog);
}

/// `JoinDialog.refresh` row action: drop the probe so the next poll re-pings.
fn join_refresh_server(state: &Rc<RefCell<JoinState>>, address: &str) {
    if let Some(mut net) = kit::net()
        && net.has_method("refresh_server")
    {
        let _ = net.call("refresh_server", &[GString::from(address).to_variant()]);
    }
    // force the next poll to rebuild the rows
    state.borrow_mut().signature.clear();
}

/// `loadServers` (`JoinDialog.java:689-694`): `Core.settings.getJson("servers")`.
fn join_load_servers(state: &Rc<RefCell<JoinState>>) {
    let mut servers = Vec::new();
    if let Some(text) = settings_text(SERVERS_SETTING, "")
        && !text.is_empty()
    {
        for entry in parse_array(&text).iter_shared() {
            let Ok(row) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            let ip = dict_text_or(&row, "ip");
            if ip.is_empty() {
                continue;
            }
            servers.push(JoinServer {
                ip,
                port: dict_i64_or(&row, "port", 0),
            });
        }
    }
    state.borrow_mut().servers = servers;
}

/// `saveServers` (`JoinDialog.java:773-775`): `Core.settings.putJson`.
fn join_save_servers(state: &Rc<RefCell<JoinState>>) {
    let servers = {
        let st = state.borrow();
        let mut array = VarArray::new();
        for entry in &st.servers {
            let mut row = VarDictionary::new();
            row.set("ip", &GString::from(entry.ip.as_str()).to_variant());
            row.set("port", &entry.port.to_variant());
            array.push(&row.to_variant());
        }
        Json::stringify(&array.to_variant()).to_string()
    };
    settings_put_text(SERVERS_SETTING, &servers);
}

/// `_show_info`: the `@join.info` help prompt.
fn join_show_info() {
    ui_show_info(&kit::t("@join.info").to_string());
}

/// Closes through `MindUi` so the dialog stack and pause governor stay in sync.
fn join_close(dialog: &Gd<MindDialog>) {
    if kit::ui().is_some() {
        let _ = close_named_dialog("join");
        return;
    }
    hide_dialog_node(dialog);
}
