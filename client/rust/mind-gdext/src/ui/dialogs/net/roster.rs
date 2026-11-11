// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Shared bans/admins roster state: the player entries, the row builder and the
//! confirm/action flow used by the bans and admins dialogs.
//! Upstream: none (port-only; shared bans/admins state)

use std::cell::RefCell;
use std::rc::Rc;

use godot::builtin::Side;
use godot::classes::control::SizeFlags;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{Control, PanelContainer, RichTextLabel, StyleBox, StyleBoxTexture};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::ui::dialog::MindDialog;
use crate::ui::dialogs::common::defer::defer_action;
use crate::ui::kit;
use crate::ui::kit::widgets::factory;

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
pub(super) enum RosterKind {
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

    /// The relay verb carried by `MindUi.admin_action`
    /// (`MindNet.player_action`; Java `unbanPlayerID` / `unAdminPlayer`).
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
pub(super) struct RosterState {
    entries: Vec<RosterEntry>,
    /// The identity waiting for the `@confirm` answer.
    pending: String,
    /// Identities whose `unban`/`unadmin` reducer was accepted: Java
    /// `setup()` re-reads `netServer.admins` after the call
    /// (`BansDialog.java:37-42`, `AdminsDialog.java:37-46`), so the row stays
    /// gone even when the static dialog context still lists it.
    removed: std::collections::BTreeSet<String>,
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
pub(super) fn dict_text(dict: &VarDictionary, keys: &[&str]) -> String {
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

/// `BansDialog`/`AdminsDialog` constructor tail: title, close button and the
/// confirm answer hook. Both upstream dialogs are `addCloseButton()` +
/// `shown(this::setup)` only.
pub(super) fn roster_ready(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<RosterState>>,
    kind: RosterKind,
) {
    dialog.set_title_key(kind.title_key());
    dialog.should_pause = false;
    dialog.clear_buttons();
    dialog.add_default_close_button();
    roster_connect_confirm(dialog, state, kind);
}

/// `shown(this::setup)`: reload the roster from the dialog context and draw it.
pub(super) fn roster_shown(
    dialog: &mut MindDialog,
    state: &Rc<RefCell<RosterState>>,
    kind: RosterKind,
) {
    let entries = roster_entries(dialog.context_value(kind.context_key()));
    {
        let mut st = state.borrow_mut();
        // An accepted unban/unadmin already dropped its row; the context is a
        // snapshot, so re-shown dialogs filter it through the removal set.
        let removed = st.removed.clone();
        st.entries = entries
            .into_iter()
            .filter(|entry| !removed.contains(&entry.id))
            .collect();
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
    let mut row = factory::hbox(0);
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

/// Whether an answered confirm removes the roster row on the redraw.
///
/// Java only re-reads the roster when the unban/unadmin call actually ran
/// (`BansDialog.java:37-42` / `AdminsDialog.java:37-46`); a cancelled confirm
/// or a dispatch the relay never accepted (no `MindNet`, single-player) must
/// keep the row, because the shell never fakes a server action.
fn roster_row_removed(confirmed: bool, accepted: bool, identifier: &str) -> bool {
    confirmed && accepted && !identifier.is_empty()
}

/// Applies an answered confirm: `unbanPlayerID`/`unAdminPlayer` through the
/// `MindUi.admin_action` relay seam (`MindNet.player_action`), then re-draws
/// the roster like `setup()`.
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
    let accepted = confirmed && !pending.is_empty() && admin_action(kind.action(), &pending);
    if roster_row_removed(confirmed, accepted, &pending) {
        // The relay listener accepted the request: drop the row like the Java
        // `setup()` re-read of `netServer.admins`.
        let mut st = state.borrow_mut();
        st.removed.insert(pending.clone());
        st.entries.retain(|entry| entry.id != pending);
    }
    if !dialog.is_instance_valid() {
        return;
    }
    let mut node = dialog.clone();
    let mut bound = node.bind_mut();
    roster_render(&mut bound, state, kind);
}

/// Forwards an admin roster action to `MindUi.admin_action` (the
/// `MindNet.player_action` reducer seam); false when no relay accepted it (the
/// shell never fakes a server action).
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

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

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
    fn roster_rows_drop_only_after_a_confirmed_accepted_action() {
        // Accepted unban/unadmin: the row leaves the redrawn roster and stays
        // gone on re-show.
        assert!(roster_row_removed(true, true, "uuid-1"));
        // A cancelled confirm sends nothing (`ui.showConfirm` answer false).
        assert!(!roster_row_removed(false, true, "uuid-1"));
        // No relay/listener accepted the reducer: keep the row (never fake).
        assert!(!roster_row_removed(true, false, "uuid-1"));
        // A bare-name context has no identity to unban.
        assert!(!roster_row_removed(true, true, ""));
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
}
