// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! List select, admin/ban lists, trace info and campaign complete.

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::control::SizeFlags;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Control, DisplayServer, HBoxContainer, PanelContainer, RichTextLabel, StyleBox,
    StyleBoxTexture, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindScroll, MindTable};

/// Godot-style string conversion for a context value.
fn variant_text(value: &Variant) -> String {
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string();
    }
    if let Ok(boolean) = value.try_to::<bool>() {
        return boolean.to_string();
    }
    if let Ok(integer) = value.try_to::<i64>() {
        return integer.to_string();
    }
    if let Ok(float) = value.try_to::<f64>() {
        return float.to_string();
    }
    String::new()
}

/// `Planet.localizedName` (`planet.<name>.name`) for the completion banner.
fn planet_localized(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    let key = format!("planet.{name}.name");
    let localized = kit::t(&format!("@{key}")).to_string();
    if localized.is_empty() || localized == key {
        name.to_string()
    } else {
        localized
    }
}

/// Reads a JSON-derived number as an integer (JSON numbers are `FLOAT`).
fn variant_i64(value: &Variant) -> i64 {
    if let Ok(integer) = value.try_to::<i64>() {
        return integer;
    }
    value
        .try_to::<f64>()
        .map(|number| number as i64)
        .unwrap_or(0)
}

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "list_select" => Some(Box::new(ListSelectDialog::default())),
        "bans" => Some(Box::new(BansDialog::default())),
        "admins" => Some(Box::new(AdminsDialog::default())),
        "traces" => Some(Box::new(TraceDialog::default())),
        "campaign_complete" => Some(Box::new(CampaignCompleteDialog::default())),
        _ => None,
    }
}

fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

fn string_array(value: Option<Variant>) -> Vec<String> {
    value
        .map(|value| value.to::<VarArray>())
        .map(|array| {
            array
                .iter_shared()
                .map(|entry| entry.to::<GString>().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// ListSelectDialog
// ---------------------------------------------------------------------------

/// Generic one-column picker: a scrollable list of titled rows, an optional
/// reset row, and a `selected(name)` signal (empty name = reset/closed).
#[derive(Default)]
pub struct ListSelectDialog {
    items: Vec<String>,
    reset_key: String,
    /// Set while the dialog is hiding after a pick, so `hidden` does not emit.
    chosen: bool,
}

impl DialogController for ListSelectDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let title = dialog.context_string("title");
        dialog.title_text = if title.is_empty() {
            GString::new()
        } else {
            kit::t(&title.to_string())
        };
        dialog.refresh_title();
        self.reset_key = dialog.context_string("reset").to_string();
        self.items = string_array(dialog.context_value("items"));
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.chosen = false;
        self.build(dialog);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        let mut chosen = self.chosen;
        if dialog.base().has_meta("_picker_chosen") {
            chosen = dialog.base().get_meta("_picker_chosen").to::<bool>();
            dialog.base_mut().remove_meta("_picker_chosen");
        }
        if !chosen {
            dialog
                .to_gd()
                .emit_signal("selected", &[GString::new().to_variant()]);
        }
        self.chosen = false;
    }
}

impl ListSelectDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut root = dialog.content_table();
        let mut scroll = MindScroll::new_alloc();
        // The dialog shell sizes `cont` from its children's minimums; without
        // this the picker renders collapsed for large item lists.
        scroll.set_custom_minimum_size(Vector2::new(520.0, 320.0));
        scroll.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        scroll.set_v_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        scroll.bind_mut().set_scrolling_disabled(true, false);
        root.bind_mut().add(&scroll).grow_x().grow_y();
        // code-instantiated: the row list is caller-supplied picker data.
        let mut list = VBoxContainer::new_alloc();
        list.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        scroll.add_child(&list);
        if !self.reset_key.is_empty() {
            let reset_text = kit::t(&self.reset_key).to_string();
            let mut reset = factory::icon_button(
                "cancel",
                &reset_text,
                "defaultt",
                godot::classes::box_container::AlignmentMode::BEGIN,
            );
            let mut target = dialog.to_gd();
            let callable = Callable::from_fn("pick_reset", move |_args: &[&Variant]| {
                let _ = target.call("pick_selected", &[GString::new().to_variant()]);
            });
            reset.connect("pressed", &callable);
            list.add_child(&reset);
        }
        let columns = 3usize;
        let mut row: Option<Gd<HBoxContainer>> = None;
        for (index, item) in self.items.clone().into_iter().enumerate() {
            // code-instantiated: picker rows are data-driven (units/effects/spawns).
            let mut button = factory::button(&item);
            button.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
            let mut target = dialog.to_gd();
            let item_for_pick = item.clone();
            let callable = Callable::from_fn("pick_item", move |_args: &[&Variant]| {
                let _ = target.call(
                    "pick_selected",
                    &[GString::from(item_for_pick.as_str()).to_variant()],
                );
            });
            button.connect("pressed", &callable);
            if index % columns == 0 {
                let mut new_row = HBoxContainer::new_alloc();
                new_row.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
                list.add_child(&new_row);
                row = Some(new_row);
            }
            if let Some(mut row) = row.clone() {
                row.add_child(&button);
            }
        }
        if self.items.is_empty() {
            let label = factory::label(&kit::t("@none.found").to_string());
            list.add_child(&label);
        }
        let _ = &mut root;
    }
}

// ---------------------------------------------------------------------------
// BansDialog / AdminsDialog (`BansDialog.java`, `AdminsDialog.java`)
// ---------------------------------------------------------------------------

/// `float w = 400f, h = 80f` (`BansDialog.java:22`, `AdminsDialog.java:22`).
const ROSTER_ROW_WIDTH: f32 = 400.0;
const ROSTER_ROW_HEIGHT: f32 = 80.0;
/// `width(w - h - 24f)`: the wrapped label cell inside the 80 px row.
const ROSTER_LABEL_WIDTH: f32 = ROSTER_ROW_WIDTH - ROSTER_ROW_HEIGHT - 24.0;
/// `res.margin(14f)` (`BansDialog.java:31`, `AdminsDialog.java:31`).
const ROSTER_MARGIN: f32 = 14.0;
/// `button(Icon.cancel, ...).size(h).pad(-14f)`: the 14 px negative pad leaves
/// an `80 - 2 * 14 = 52` px icon inside the 80 px cell.
const ROSTER_CANCEL_SIZE: f32 = ROSTER_ROW_HEIGHT - ROSTER_MARGIN * 2.0;
/// `Tex.button`'s nine-patch split (the theme builder's 12 px `button`).
const ROSTER_BUTTON_SPLIT: f32 = 12.0;
/// The one-time `confirm_result` hook installed on a roster dialog node.
const ROSTER_CONFIRM_META: &str = "_mind_misc_roster_confirm";

/// One `Administration.PlayerInfo` roster row: the identity the unban/unadmin
/// action takes plus the last IP/name the dialog prints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct RosterEntry {
    id: String,
    ip: String,
    name: String,
}

/// Which roster a dialog renders (`BansDialog` or `AdminsDialog`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RosterKind {
    Bans,
    Admins,
}

impl RosterKind {
    /// `super("@server.bans")` / `super("@server.admins")`.
    fn title_key(self) -> &'static str {
        match self {
            Self::Bans => "@server.bans",
            Self::Admins => "@server.admins",
        }
    }

    /// `@server.bans.none` / `@server.admins.none` (the empty-list row).
    fn empty_key(self) -> &'static str {
        match self {
            Self::Bans => "@server.bans.none",
            Self::Admins => "@server.admins.none",
        }
    }

    /// The context key the roster rows ride on (`banned` / `admins`).
    fn context_key(self) -> &'static str {
        match self {
            Self::Bans => "banned",
            Self::Admins => "admins",
        }
    }

    /// The `MindUi.admin_action` verb (`unbanPlayerID` / `unAdminPlayer`).
    fn action(self) -> &'static str {
        match self {
            Self::Bans => "unban",
            Self::Admins => "unadmin",
        }
    }

    /// `ui.showConfirm` body (`@confirmunban` / `@confirmunadmin`).
    fn confirm_text(self, entry: &RosterEntry) -> String {
        match self {
            Self::Bans => kit::t("@confirmunban").to_string(),
            Self::Admins => kit::tm_format("@confirmunadmin", &[entry.name.as_str()]).to_string(),
        }
    }

    /// The row label markup (`BansDialog.java:34`, `AdminsDialog.java:34`).
    fn row_text(self, entry: &RosterEntry) -> String {
        kit::markup_text(&self.row_markup(entry)).to_string()
    }

    /// [`Self::row_text`] before the Mindustry markup is translated to BBCode.
    fn row_markup(self, entry: &RosterEntry) -> String {
        match self {
            Self::Bans => format!(
                "IP: [lightgray]{}\n[]Name: [lightgray]{}",
                entry.ip, entry.name
            ),
            Self::Admins => format!("[lightgray]{}", entry.name),
        }
    }
}

/// The roster view state (the dialog re-reads its context on every `shown`).
#[derive(Default)]
struct RosterState {
    entries: Vec<RosterEntry>,
    /// The identity waiting for the `@confirm` answer.
    pending: String,
}

/// Reads `banned`/`admins` context rows: `PlayerInfo` dictionaries
/// (`id`/`lastIP`/`lastName`; `uuid`/`ip`/`name` accepted) or bare names.
fn roster_entries(value: Option<Variant>) -> Vec<RosterEntry> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Ok(rows) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for row in rows.iter_shared() {
        if let Ok(info) = row.try_to::<VarDictionary>() {
            let entry = RosterEntry {
                id: dict_text(&info, &["id", "uuid"]),
                ip: dict_text(&info, &["lastIP", "ip"]),
                name: dict_text(&info, &["lastName", "name"]),
            };
            if !entry.id.is_empty() || !entry.name.is_empty() {
                entries.push(entry);
            }
            continue;
        }
        if let Ok(name) = row.try_to::<GString>()
            && !name.is_empty()
        {
            entries.push(RosterEntry {
                name: name.to_string(),
                ..RosterEntry::default()
            });
        }
    }
    entries
}

/// The first non-empty `GString` field among `keys`.
fn dict_text(dict: &VarDictionary, keys: &[&str]) -> String {
    for key in keys {
        if let Some(value) = dict.get(*key)
            && let Ok(text) = value.try_to::<GString>()
        {
            return text.to_string();
        }
    }
    String::new()
}

/// `Tex.button` row background: the 12 px nine-patch split with the table's
/// `margin(14f)` content inset.
fn roster_style() -> Gd<StyleBox> {
    let mut style = StyleBoxTexture::new_gd();
    if let Some(texture) = kit::icon_texture("button") {
        style.set_texture(&texture);
    }
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        style.set_texture_margin(side, ROSTER_BUTTON_SPLIT);
        style.set_content_margin(side, ROSTER_MARGIN);
    }
    style.upcast()
}

/// `BansDialog`: the ban list with one `Icon.cancel` unban button per row.
#[derive(Default)]
pub struct BansDialog {
    state: Rc<RefCell<RosterState>>,
}

impl DialogController for BansDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        roster_ready(dialog, &self.state, RosterKind::Bans);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        roster_shown(dialog, &self.state, RosterKind::Bans);
    }
}

/// `AdminsDialog`: the admin roster with one `Icon.cancel` unadmin button.
#[derive(Default)]
pub struct AdminsDialog {
    state: Rc<RefCell<RosterState>>,
}

impl DialogController for AdminsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        roster_ready(dialog, &self.state, RosterKind::Admins);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        roster_shown(dialog, &self.state, RosterKind::Admins);
    }
}

/// `BansDialog`/`AdminsDialog` constructor tail: title, close button and the
/// confirm answer hook. Both upstream dialogs are `addCloseButton()` +
/// `shown(this::setup)` only.
fn roster_ready(dialog: &mut MindDialog, state: &Rc<RefCell<RosterState>>, kind: RosterKind) {
    dialog.set_title_key(kind.title_key());
    dialog.should_pause = false;
    dialog.clear_buttons();
    dialog.add_close_button(210.0);
    roster_connect_confirm(dialog, state, kind);
}

/// `shown(this::setup)`: reload the roster from the dialog context and draw it.
fn roster_shown(dialog: &mut MindDialog, state: &Rc<RefCell<RosterState>>, kind: RosterKind) {
    let entries = roster_entries(dialog.context_value(kind.context_key()));
    {
        let mut st = state.borrow_mut();
        st.entries = entries;
        st.pending.clear();
    }
    roster_render(dialog, state, kind);
}

/// `setup()`: the empty-list label or one 400x80 `Tex.button` row per entry.
fn roster_render(dialog: &mut MindDialog, state: &Rc<RefCell<RosterState>>, kind: RosterKind) {
    dialog.clear_content();
    let mut table = dialog.content_table();
    let entries = state.borrow().entries.clone();
    if entries.is_empty() {
        let label = factory::label(&kit::t(kind.empty_key()).to_string());
        table.bind_mut().add(&label).pad(8.0);
        return;
    }
    for entry in entries {
        let row = roster_row(dialog, state, kind, &entry);
        table
            .bind_mut()
            .add(&row)
            .width(ROSTER_ROW_WIDTH)
            .height(ROSTER_ROW_HEIGHT);
        table.bind_mut().row();
    }
}

/// One `Table res = new Table(Tex.button)` row: the wrapped IP/name label, the
/// growing spacer and the `Icon.cancel` action.
fn roster_row(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<RosterState>>,
    kind: RosterKind,
    entry: &RosterEntry,
) -> Gd<PanelContainer> {
    // code-instantiated: roster rows are server-driven admin views.
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &roster_style());
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 0);
    let label = factory::label(&kind.row_text(entry));
    let mut label_rich: Gd<RichTextLabel> = label.clone().upcast();
    label_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
    label_rich.set_custom_minimum_size(Vector2::new(ROSTER_LABEL_WIDTH, 0.0));
    label_rich.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    row.add_child(&label);
    // `res.add().growX()`.
    let mut spacer = Control::new_alloc();
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    row.add_child(&spacer);
    let mut cancel = factory::glyph_button("cancel", "X", "emptyi");
    cancel.set_custom_minimum_size(Vector2::new(ROSTER_CANCEL_SIZE, ROSTER_CANCEL_SIZE));
    let target = dialog.to_gd();
    let state_for = state.clone();
    let entry_for = entry.clone();
    let callable = Callable::from_fn("roster_unlist", move |_args: &[&Variant]| {
        roster_confirm(&target, &state_for, kind, &entry_for);
        Variant::nil()
    });
    cancel.connect("pressed", &callable);
    row.add_child(&cancel);
    panel.add_child(&row);
    panel
}

/// The `Icon.cancel` action: ask `@confirm` first (`showConfirm`) and answer
/// through [`roster_connect_confirm`].
fn roster_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<RosterState>>,
    kind: RosterKind,
    entry: &RosterEntry,
) {
    let _ = dialog;
    state.borrow_mut().pending = entry.id.clone();
    let text = kind.confirm_text(entry);
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call("show_confirm", &[text.to_variant()]);
    }
}

/// `confirm_result` -> the pending unban/unadmin, forwarded on the relay seam.
fn roster_connect_confirm(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<RosterState>>,
    kind: RosterKind,
) {
    let Some(mut ui) = kit::ui() else {
        return;
    };
    if !ui.has_signal("confirm_result") || dialog.base().has_meta(ROSTER_CONFIRM_META) {
        return;
    }
    let target = dialog.to_gd();
    let state_for = state.clone();
    let callable = Callable::from_fn("roster_confirm_result", move |args: &[&Variant]| {
        let confirmed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let target_for = target.clone();
        let state_for = state_for.clone();
        // The answer is emitted from inside a `MindUi` mutable borrow.
        defer_action("roster_confirm_apply", move || {
            roster_apply_confirm(&target_for, &state_for, kind, confirmed);
        });
        Variant::nil()
    });
    ui.connect("confirm_result", &callable);
    dialog
        .base_mut()
        .set_meta(ROSTER_CONFIRM_META, &true.to_variant());
}

/// Applies an answered confirm: `unbanPlayerID`/`unAdminPlayer` through the
/// `MindUi.admin_action` relay seam, then re-draws the roster like `setup()`.
fn roster_apply_confirm(
    dialog: &Gd<MindDialog>,
    state: &Rc<RefCell<RosterState>>,
    kind: RosterKind,
    confirmed: bool,
) {
    let pending = {
        let mut st = state.borrow_mut();
        std::mem::take(&mut st.pending)
    };
    if confirmed && !pending.is_empty() && admin_action(kind.action(), &pending) {
        // The relay listener accepted the request: drop the row like the Java
        // `setup()` re-read of `netServer.admins`.
        state
            .borrow_mut()
            .entries
            .retain(|entry| entry.id != pending);
    }
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    let mut bound = node.bind_mut();
    roster_render(&mut bound, state, kind);
}

/// Forwards an admin roster action to `MindUi.admin_action`; false when the
/// relay seam has no listener (the shell never fakes a server action).
fn admin_action(action: &str, identifier: &str) -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    if !ui.has_method("admin_action") {
        return false;
    }
    ui.call(
        "admin_action",
        &[
            GString::from(action).to_variant(),
            GString::from(identifier).to_variant(),
        ],
    )
    .try_to::<bool>()
    .unwrap_or(false)
}

/// A queued one-shot action (`Callable.call_deferred` payload).
type DeferredAction = Rc<RefCell<Option<Box<dyn FnOnce()>>>>;

/// A `Callable` that queues `action` for the idle phase (`call_deferred`).
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

// ---------------------------------------------------------------------------
// TraceDialog (`TraceDialog.java`)
// ---------------------------------------------------------------------------

/// `Table table = new Table(Tex.clear); table.margin(14)` (`TraceDialog.java:21`).
const TRACE_MARGIN: f32 = 14.0;
/// `float s = 28f` (`TraceDialog.java:29`): the copy-button cell.
const TRACE_COPY_SIZE: f32 = 28.0;
/// `table.pane(...).padLeft(20f)` for the IP/name history lists.
const TRACE_LIST_INDENT: f32 = 20.0;

/// One `TraceDialog` row (`TraceDialog.java:31-63`).
#[derive(Debug, Default, PartialEq, Eq)]
struct TraceFields {
    player: String,
    ip: String,
    locale: String,
    uuid: String,
    modded: bool,
    mobile: bool,
    joined: i64,
    kicked: i64,
    ips: Vec<String>,
    names: Vec<String>,
}

impl TraceFields {
    /// The Java `PlayerInfo` context dictionary (`TraceDialog.show(player, info)`).
    fn from_context(player: &str, info: &VarDictionary) -> Self {
        Self {
            player: player.to_owned(),
            ip: dict_text(info, &["ip"]),
            locale: dict_text(info, &["locale"]),
            uuid: dict_text(info, &["uuid"]),
            modded: bool_value(info.get("modded")),
            mobile: bool_value(info.get("mobile")),
            joined: count_value(info.get("timesJoined")),
            kicked: count_value(info.get("timesKicked")),
            ips: list_strings(info.get("ips")),
            names: list_strings(info.get("names")),
        }
    }
}

/// `MindUi.player_trace_json` (`mind-stdb` `TraceInfo.to_json`) -> dialog rows.
///
/// `PlayerListFragment`'s Trace button reads the roster aggregation instead of
/// asking the server (`AdminAction.trace`); the relay schema has no IP, locale
/// or custom-client columns (roster deviation D2), so those Java rows keep
/// their empty/false defaults. `sessions`/`kicks` feed the joined/kicked
/// counts and `names` the `@trace.names` pane.
fn trace_fields_from_roster(player: &str, trace_json: &str) -> Option<TraceFields> {
    let value = serde_json::from_str::<serde_json::Value>(trace_json).ok()?;
    let object = value.as_object()?;
    let identity = object
        .get("identity")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let sessions = object.get("sessions").and_then(serde_json::Value::as_i64);
    let names: Vec<String> = object
        .get("names")
        .and_then(serde_json::Value::as_array)
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if identity.is_empty() && sessions.is_none() && names.is_empty() {
        return None;
    }
    Some(TraceFields {
        player: player.to_owned(),
        uuid: identity.to_owned(),
        joined: sessions.unwrap_or(0),
        kicked: object
            .get("kicks")
            .and_then(serde_json::Value::as_array)
            .map(|kicks| kicks.len() as i64)
            .unwrap_or(0),
        names,
        ..TraceFields::default()
    })
}

/// `MindUi.open_dialog("traces", ctx)` context: the roster trace payload is
/// passed verbatim (as a string) so [`TraceDialog`] owns the field mapping.
pub(crate) fn trace_context_json(player: &str, trace_json: &str) -> String {
    serde_json::json!({ "player": player, "trace": trace_json }).to_string()
}

/// Player trace info (`TraceDialog`): the copyable player/IP/language/ID rows,
/// the formatted scalar rows and the IP/name history lists.
#[derive(Default)]
pub struct TraceDialog;

impl DialogController for TraceDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@trace");
        dialog.should_pause = false;
        dialog.clear_buttons();
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut table = dialog.content_table();
        table.bind_mut().margin(TRACE_MARGIN);
        let player = dialog.context_string("player").to_string();
        // `PlayerListFragment`'s Trace button passes the relay roster trace
        // (`MindUi.player_trace_json`); a direct Java-style caller passes the
        // `info` `PlayerInfo` dictionary instead.
        let trace_text = dialog.context_string("trace").to_string();
        let fields = match trace_fields_from_roster(&player, &trace_text) {
            Some(fields) => fields,
            None => {
                let info = dialog
                    .context_value("info")
                    .and_then(|value| value.try_to::<VarDictionary>().ok())
                    .unwrap_or_default();
                TraceFields::from_context(&player, &info)
            }
        };
        // `TraceDialog.java:31-45`: one copyable `bundle.format` line each.
        for (key, value) in
            trace_field_rows(&fields.player, &fields.ip, &fields.locale, &fields.uuid)
        {
            trace_copy_row(&mut table, key, &value);
        }
        // `TraceDialog.java:47-50`: the four scalar lines.
        for (key, value) in
            trace_scalar_rows(fields.modded, fields.mobile, fields.joined, fields.kicked)
        {
            let text = kit::tm_format(key, &[value.as_str()]).to_string();
            let label = factory::label(&text);
            table.bind_mut().add(&label).pad(1.0).left();
            table.bind_mut().row();
        }
        // `TraceDialog.java:52-63`: `@trace.ips` / `@trace.names` panes.
        for (key, values) in trace_list_rows(fields.ips, fields.names) {
            let header = factory::label(&kit::t(key).to_string());
            table.bind_mut().add(&header).pad(1.0).left();
            table.bind_mut().row();
            // code-instantiated: the IP/name history is trace data.
            let mut column = VBoxContainer::new_alloc();
            for value in values {
                let label = factory::label(&format!("[lightgray]{value}"));
                column.add_child(&label);
            }
            table
                .bind_mut()
                .add(&column)
                .pad(1.0)
                .pad_left(TRACE_LIST_INDENT)
                .left();
            table.bind_mut().row();
        }
    }
}

/// One `TraceDialog` copy row: `Icon.copySmall` 28x28 then the formatted text.
fn trace_copy_row(table: &mut Gd<MindTable>, key: &str, value: &str) {
    // code-instantiated: trace fields are data-driven from the trace info.
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 4);
    let mut button = factory::glyph_button("copy", "@copy", "emptyi");
    button.set_custom_minimum_size(Vector2::new(TRACE_COPY_SIZE, TRACE_COPY_SIZE));
    let value_for_copy = GString::from(value);
    let callable = Callable::from_fn("copy_trace", move |_args: &[&Variant]| {
        DisplayServer::singleton().clipboard_set(&value_for_copy);
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("show_info_fade", &[kit::t("@copied").to_variant()]);
        }
    });
    button.connect("pressed", &callable);
    row.add_child(&button);
    let text = kit::tm_format(key, &[value]).to_string();
    let mut label = factory::label(&text);
    label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    row.add_child(&label);
    table.bind_mut().add(&row).pad(1.0).left();
    table.bind_mut().row();
}

/// The copyable trace lines in Java order (`TraceDialog.java:31-45`):
/// `trace.playername`, `trace.ip`, `trace.language`, `trace.id`.
fn trace_field_rows(
    player: &str,
    ip: &str,
    locale: &str,
    uuid: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("@trace.playername", player.to_string()),
        ("@trace.ip", ip.to_string()),
        ("@trace.language", locale.to_string()),
        ("@trace.id", uuid.to_string()),
    ]
}

/// The scalar trace lines in Java order (`TraceDialog.java:47-50`).
fn trace_scalar_rows(
    modded: bool,
    mobile: bool,
    joined: i64,
    kicked: i64,
) -> Vec<(&'static str, String)> {
    vec![
        ("@trace.modclient", modded.to_string()),
        ("@trace.mobile", mobile.to_string()),
        ("@trace.times.joined", joined.to_string()),
        ("@trace.times.kicked", kicked.to_string()),
    ]
}

/// The IP/name history lists in Java order (`TraceDialog.java:52-63`).
fn trace_list_rows(ips: Vec<String>, names: Vec<String>) -> Vec<(&'static str, Vec<String>)> {
    vec![("@trace.ips", ips), ("@trace.names", names)]
}

/// A `String[]` trace field as text rows.
fn list_strings(value: Option<Variant>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    let Ok(array) = value.try_to::<VarArray>() else {
        return Vec::new();
    };
    array
        .iter_shared()
        .map(|entry| variant_text(&entry))
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// A Java `boolean` trace field (`false` when absent).
fn bool_value(value: Option<Variant>) -> bool {
    match value {
        Some(value) => {
            if let Ok(flag) = value.try_to::<bool>() {
                flag
            } else if let Ok(number) = value.try_to::<i64>() {
                number != 0
            } else {
                variant_text(&value) == "true"
            }
        }
        None => false,
    }
}

/// A Java `int` trace field (`0` when absent).
fn count_value(value: Option<Variant>) -> i64 {
    match value {
        Some(value) => {
            if let Ok(number) = value.try_to::<i64>() {
                number
            } else if let Ok(number) = value.try_to::<f64>() {
                number as i64
            } else {
                0
            }
        }
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// CampaignCompleteDialog
// ---------------------------------------------------------------------------

/// Campaign completion banner.
#[derive(Default)]
pub struct CampaignCompleteDialog {
    table: Option<Gd<MindTable>>,
}

impl DialogController for CampaignCompleteDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = true;
        self.table = Some(dialog.content_table());
        // `CampaignCompleteDialog.java:20-28`: `@menu` then `@continue`, both
        // 210x64, and no visible back button (`addCloseListener` only listens
        // for Escape/back).
        if let Some(mut row) = dialog.buttons_row() {
            let mut menu = factory::button(&kit::t("@menu").to_string());
            menu.set_custom_minimum_size(Vector2::new(210.0, 64.0));
            let callable = Callable::from_fn("exit_to_menu", |_args: &[&Variant]| {
                exit_to_menu();
            });
            menu.connect("pressed", &callable);
            row.add_child(&menu);
            let mut continue_button = factory::button(&kit::t("@continue").to_string());
            continue_button.set_custom_minimum_size(Vector2::new(210.0, 64.0));
            let callable = dialog.to_gd().callable("hide_dialog");
            continue_button.connect("pressed", &callable);
            row.add_child(&continue_button);
        }
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let Some(mut table) = self.table.clone() else {
            return;
        };
        table.bind_mut().clear();
        let has_context = dialog.context_value("planet").is_some()
            || dialog.context_value("playtime_ms").is_some();
        let view = if has_context {
            let mut copy = VarDictionary::new();
            for key in ["localized", "planet", "color", "playtime_ms"] {
                if let Some(value) = dialog.context_value(key) {
                    copy.set(key, &value);
                }
            }
            copy
        } else {
            dialog
                .campaign_views()
                .get("complete")
                .map(|value| value.to::<VarDictionary>())
                .unwrap_or_default()
        };
        let planet = view
            .get("localized")
            .or_else(|| view.get("planet"))
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        // `Planet.localizedName` (`planet.<name>.name`): the campaign read
        // model falls back to the raw content name when the registry's bundle
        // is the empty memory bundle, so resolve the live bundle here.
        let planet = planet_localized(&planet.to_string());
        let color = view
            .get("color")
            .map(|value| value.to::<GString>())
            .unwrap_or_else(|| GString::from("ffd37f"));
        let planet_markup = if planet.is_empty() {
            String::new()
        } else {
            format!("[#{}]{}[]", color, planet)
        };
        // code-instantiated: the completion text is data-driven from campaign state.
        let message =
            factory::label(&kit::tm_format("@campaign.complete", &[&planet_markup]).to_string());
        rich(&message).set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        table.bind_mut().add(&message).grow_x().pad(8.0);
        table.bind_mut().row();
        // The campaign read models carry JSON numbers, which are FLOAT
        // variants: `to::<i64>()` panics on the type mismatch and aborted the
        // build before the playtime row (`CampaignCompleteDialog.java:37-40`).
        let playtime = view
            .get("playtime_ms")
            .map(|value| variant_i64(&value))
            .unwrap_or(0);
        if playtime > 0 {
            let time = MindDialog::format_time_ms(playtime);
            let label = factory::label(&kit::tm_format("@campaign.playtime", &[&time]).to_string());
            table.bind_mut().add(&label).pad(4.0);
            table.bind_mut().row();
        }
    }
}

/// `CampaignCompleteDialog.@menu`: write the exit save, then return to the menu.
fn exit_to_menu() {
    if let Some(mut campaign) = kit::campaign()
        && campaign.has_method("run_exit_save")
    {
        let _ = campaign.call("run_exit_save", &[]);
    }
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call(
            "close_dialog",
            &[GString::from("campaign_complete").to_variant()],
        );
        let _ = ui.call("close_dialog", &[GString::from("planet").to_variant()]);
    }
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("return_to_menu")
    {
        let _ = root.call("return_to_menu", &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_array_handles_missing_context() {
        assert!(string_array(None).is_empty());
    }

    #[test]
    fn playtime_uses_the_dialog_formatter() {
        assert_eq!(MindDialog::format_time_ms(0), "0s");
        assert_eq!(MindDialog::format_time_ms(3_600_000), "1h 0m");
    }

    #[test]
    fn pane_colors_are_stable() {
        use super::super::super::kit::styles;
        assert_eq!(styles::ACCENT, styles::ACCENT);
    }

    #[test]
    fn roster_kinds_match_the_upstream_actions() {
        assert_eq!(RosterKind::Bans.context_key(), "banned");
        assert_eq!(RosterKind::Admins.context_key(), "admins");
        assert_eq!(RosterKind::Bans.action(), "unban");
        assert_eq!(RosterKind::Admins.action(), "unadmin");
        assert_eq!(RosterKind::Bans.empty_key(), "@server.bans.none");
        assert_eq!(RosterKind::Admins.empty_key(), "@server.admins.none");
    }

    #[test]
    fn roster_row_markup_matches_the_java_strings() {
        let entry = RosterEntry {
            id: "uuid-1".into(),
            ip: "1.2.3.4".into(),
            name: "Alice".into(),
        };
        assert_eq!(
            RosterKind::Bans.row_markup(&entry),
            "IP: [lightgray]1.2.3.4\n[]Name: [lightgray]Alice"
        );
        assert_eq!(RosterKind::Admins.row_markup(&entry), "[lightgray]Alice");
    }

    #[test]
    fn trace_rows_follow_the_java_order() {
        let fields = trace_field_rows("Alice", "1.2.3.4", "en", "u-1");
        let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
        assert_eq!(
            keys,
            [
                "@trace.playername",
                "@trace.ip",
                "@trace.language",
                "@trace.id"
            ]
        );
        assert_eq!(fields[0].1, "Alice");
        assert_eq!(fields[1].1, "1.2.3.4");
        assert_eq!(fields[2].1, "en");
        assert_eq!(fields[3].1, "u-1");
        let scalars = trace_scalar_rows(false, true, 3, 1);
        let values: Vec<&str> = scalars.iter().map(|(_, value)| value.as_str()).collect();
        assert_eq!(values, ["false", "true", "3", "1"]);
    }

    #[test]
    fn trace_lists_read_ips_and_names() {
        let lists = trace_list_rows(
            vec!["1.1.1.1".to_string(), "2.2.2.2".to_string()],
            vec!["Alice".to_string()],
        );
        assert_eq!(lists[0].0, "@trace.ips");
        assert_eq!(lists[0].1, ["1.1.1.1", "2.2.2.2"]);
        assert_eq!(lists[1].0, "@trace.names");
        assert_eq!(lists[1].1, ["Alice"]);
    }

    #[test]
    fn trace_scalars_default_to_java_primitive_values() {
        let scalars = trace_scalar_rows(false, false, 0, 0);
        let values: Vec<&str> = scalars.iter().map(|(_, value)| value.as_str()).collect();
        assert_eq!(values, ["false", "false", "0", "0"]);
    }

    #[test]
    fn roster_trace_maps_into_the_trace_dialog_rows() {
        let trace = r#"{
            "identity": "ab12",
            "names": ["Alice", "alice_old"],
            "sessions": 4,
            "connected": true,
            "lastJoin": 123,
            "banned": true,
            "whitelisted": false,
            "kicks": [{"reason": "grief", "by": "cd", "at": 9}],
            "actions": [{"kind": "kick", "detail": "", "at": 9}]
        }"#;
        let Some(fields) = trace_fields_from_roster("Alice", trace) else {
            panic!("roster trace should map");
        };
        assert_eq!(fields.player, "Alice");
        assert_eq!(fields.uuid, "ab12");
        assert_eq!(fields.joined, 4);
        assert_eq!(fields.kicked, 1);
        assert_eq!(fields.names, ["Alice", "alice_old"]);
        // The relay schema has no IP/locale/custom-client columns (D2).
        assert_eq!(fields.ip, "");
        assert_eq!(fields.locale, "");
        assert!(!fields.modded);
        assert!(!fields.mobile);
        assert!(fields.ips.is_empty());
        // The Java order is preserved when rendered.
        assert_eq!(
            trace_field_rows(&fields.player, &fields.ip, &fields.locale, &fields.uuid)[3],
            ("@trace.id", String::from("ab12"))
        );
    }

    #[test]
    fn malformed_or_empty_trace_falls_back_to_the_java_context() {
        assert!(trace_fields_from_roster("Alice", "not-json").is_none());
        assert!(trace_fields_from_roster("Alice", "{}").is_none());
        assert!(trace_fields_from_roster("Alice", "").is_none());
    }

    #[test]
    fn trace_context_carries_the_player_and_payload() {
        let context = trace_context_json("Alice", r#"{"identity":"ab"}"#);
        let value: serde_json::Value =
            serde_json::from_str(&context).unwrap_or(serde_json::Value::Null);
        assert_eq!(value["player"], "Alice");
        assert_eq!(value["trace"], r#"{"identity":"ab"}"#);
    }
}
