// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/PlayerListFragment.java (plan 14 M7).
//
// `PlayerListFragment` (v146): count header, search field, the team-sorted peer
// rows (`iconTable` + coloured name + admin flag + row menu), the Ban/Kick/
// Trace/Team/Admin menu, the votekick hammer row and the
// bans/admins/close footer. Rows come from the relay roster
// (`MindUi.player_list_json` -> `MindNet.players_json`, PAR-0122); every row
// action goes through `MindUi.player_action`/`player_action_with`, and Trace
// opens `TraceDialog` with `MindUi.player_trace_json` context.
//
// Deviations (documented): the relay roster has no per-player colour, icon or
// local-identity column (`mind-stdb` roster D2), so the avatar/name use the
// team colour, the self-only menu clauses cannot be evaluated and the
// allowTeamSwitch rules gate is left to the relay (the server enforces
// admin/host authority). With no relay the list renders Java's empty state.

//! Player-list overlay (`PlayerListFragment`).

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{LayoutPreset, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::{
    Button, CenterContainer, Control, GridContainer, HBoxContainer, IControl, InputEvent,
    InputEventKey, LineEdit, PanelContainer, ScrollContainer, StyleBox, StyleBoxFlat,
    VBoxContainer,
};
use godot::global::Key;
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::ui::dialogs::misc::trace_context_json;
use crate::ui::kit;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

/// `PlayerListFragment` bottom action row, in order.
const MENU_ACTIONS: [&str; 3] = ["server.bans", "server.admins", "close"];
/// `float h = 50f` row/inline icon size (`PlayerListFragment.rebuild`).
const ROW_HEIGHT: f32 = 50.0;
/// `Vars.maxNameLength` (the search field's `maxTextLength`).
const SEARCH_MAX_LENGTH: i32 = 40;
/// `Groups.player.size() >= 3` (the votekick row).
const VOTE_MIN_PLAYERS: usize = 3;
/// `panel.minWidth(360f)`.
const PANEL_MIN_WIDTH: f32 = 360.0;
/// `BaseDialog` menu button size (`t.defaults().size(220f, 55f)`).
const MENU_BUTTON_WIDTH: f32 = 220.0;
const MENU_BUTTON_HEIGHT: f32 = 55.0;
/// `teamSelect.cont.add(b).size(50f)`.
const TEAM_BUTTON_SIZE: f32 = 50.0;
/// `Vars.maxTextLength` (votekick reason prompt).
const VOTE_REASON_MAX: i64 = 200;

/// One row from `MindUi.player_list_json()` (`MindNet.players_json`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PlayerRow {
    /// Identity hex (`Player.uuid`).
    identity: String,
    /// Last-known username (`Player.name`; may be empty before the `player`
    /// row arrives).
    name: String,
    /// Assigned team (`Player.team`; `None` when unassigned).
    team: Option<u8>,
    /// Admin flag (`Player.admin`).
    admin: bool,
    /// Live connection (`Groups.player` only holds connected peers).
    connected: bool,
    /// Membership role (`player`/`spectator`).
    role: String,
    /// Whether a kick reason is recorded on the member row.
    kicked: bool,
}

impl PlayerRow {
    /// `Player.name`, falling back to the identity prefix like the console
    /// `players` listing so an unresolved roster row is still addressable.
    fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        self.identity.chars().take(8).collect()
    }
}

/// A row verb waiting for the `MindUi.confirm_result` answer.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingAction {
    player: String,
    action: String,
}

/// `MindPlayerListFragment` — the multiplayer player list.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindPlayerListFragment {
    base: Base<Control>,
    content: Option<Gd<VBoxContainer>>,
    header: Option<Gd<kit::MindLabel>>,
    search_field: Option<Gd<LineEdit>>,
    search: String,
    rows: Vec<PlayerRow>,
    /// The floating per-player menu (`new BaseDialog(user.coloredName())`).
    menu: Option<Gd<Control>>,
    menu_body: Option<Gd<VBoxContainer>>,
    /// Bottom action row buttons in `MENU_ACTIONS` order (Bans/Admins are
    /// disabled for relay clients like Java's `disabled(b -> net.client())`).
    footer: Vec<Gd<Button>>,
    pending: Option<PendingAction>,
    pending_votekick: Option<String>,
    /// Local relay identity hex (`MindNet.local_identity_hex`); `""` offline.
    local_identity: String,
    /// `net.server()`: the local session hosts the match
    /// (`MindNet.local_admin`, `HostDialog.runHost`).
    is_server: bool,
    /// `player.admin`: the host, or the local identity's roster row carries the
    /// global admin record.
    local_admin: bool,
    /// `player.team()` (`MindNet.my_team`); `None` while unassigned.
    local_team: Option<u8>,
}

#[godot_api]
impl IControl for MindPlayerListFragment {
    fn ready(&mut self) {
        self.base_mut().set_anchors_preset(LayoutPreset::FULL_RECT);
        self.base_mut().set_visible(false);
        // Escape handling (`PlayerListFragment` has no upstream key handler:
        // Java reserves Escape for the pause menu, but the port's overlay
        // covers the HUD, so it consumes the key while up).
        self.base_mut().set_process_unhandled_key_input(true);
        self.build();
        self.connect_prompts();
    }

    /// Escape dismisses the overlay (and its floating row menu first).
    ///
    /// `PlayerListFragment` is a plain HUD fragment in Java and never sees
    /// Escape (`Control.java:746-750` opens the pause menu); PAR-0608's input
    /// contract closes the port's overlay with the same key instead. A dialog,
    /// the console or the fullscreen minimap is above the list, so those win
    /// and `MindUiRoot.on_escape` handles the key (`_unhandled_key_input` must
    /// not mark it handled to keep that path working).
    fn unhandled_key_input(&mut self, event: Gd<InputEvent>) {
        if !self.base().is_visible() {
            return;
        }
        let Ok(key) = event.clone().try_cast::<InputEventKey>() else {
            return;
        };
        if !key.is_pressed() || key.is_echo() || key.get_keycode() != Key::ESCAPE {
            return;
        }
        if self.menu.as_ref().is_some_and(|menu| menu.is_visible()) {
            self.close_menu();
        } else if !self.overlay_above() {
            self.close_overlay();
        } else {
            return;
        }
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
    }
}

#[godot_api]
impl MindPlayerListFragment {
    /// Emitted when the close action (or Escape) dismisses the overlay.
    #[signal]
    fn close_requested();

    /// Rebuilds the rows from `MindUi.player_list_json()` and the header from
    /// `MindUi.player_count()` (`PlayerListFragment.rebuild`).
    #[func]
    pub fn rebuild(&mut self) {
        let mut rows = parse_rows(&self.player_list_payload());
        sort_rows(&mut rows);
        let connected = rows.iter().filter(|row| row.connected).count();
        let count = self
            .live_player_count()
            .map(|count| count.max(0) as usize)
            .unwrap_or(connected);
        self.rows = rows;
        self.refresh_local_player();
        self.sync_footer();
        self.update_header(count);
        let Some(mut content) = self.content.clone() else {
            return;
        };
        clear_children(&content);
        let visible: Vec<PlayerRow> = self
            .rows
            .iter()
            .filter(|row| row.connected && matches_search(&row.display_name(), &self.search))
            .cloned()
            .collect();
        if visible.is_empty() {
            // `if(!found)` (`PlayerListFragment.rebuild`): no live relay rows
            // (single-player/boot) render the same empty state as a filtered
            // list with no matches.
            let label = factory::label(&kit::t("@players.notfound").to_string());
            content.add_child(&label);
            return;
        }
        for row in &visible {
            let control = self.player_row(row, count);
            content.add_child(&control);
        }
    }

    /// Shows/hides the overlay, rebuilding the rows when it opens. Java's
    /// `cont.update` hides it whenever the relay is not in a running match
    /// (`net.active() && state.isGame()`), so `toggle` is a no-op then.
    #[func]
    pub fn toggle(&mut self) {
        let visible = !self.base().is_visible();
        if visible && !relay_in_game() {
            return;
        }
        if visible {
            self.base_mut().set_visible(true);
            self.rebuild();
        } else {
            self.close_overlay();
        }
    }

    /// Whether the overlay is visible.
    #[func]
    pub fn shown(&self) -> bool {
        self.base().is_visible()
    }

    /// Search field filter (`text_changed`): `Strings.stripColors(name)`
    /// lowercased contains the query (`PlayerListFragment.rebuild`).
    #[func]
    fn search_changed(&mut self, text: GString) {
        self.search = text.to_string().to_lowercase();
        self.rebuild();
    }

    /// Opens the row menu (`new BaseDialog(user.coloredName())`).
    #[func]
    fn open_player_menu(&mut self, player: GString) {
        let player = player.to_string();
        let Some(row) = self.row_by_target(&player).cloned() else {
            return;
        };
        self.fill_player_menu(&row);
        self.set_menu_visible(true);
    }

    /// One menu button (`t.button("@player.ban", ...)` etc.). `player` is the
    /// row target ([`row_target`]), not necessarily the display name.
    #[func]
    fn menu_pressed(&mut self, player: GString, action: GString) {
        let player = player.to_string();
        let action = action.to_string();
        match action.as_str() {
            "team" => self.fill_team_menu(&player),
            "trace" => {
                self.close_menu();
                self.open_trace(&player);
            }
            "votekick" => {
                self.close_menu();
                self.open_votekick(&player);
            }
            verb => {
                self.close_menu();
                self.request_confirm(&player, verb);
            }
        }
    }

    /// The team chooser answer (`Call.adminRequest(user, switchTeam, team)`).
    #[func]
    fn choose_team(&mut self, player: GString, team: i64) {
        self.close_menu();
        self.dispatch_player_action(&player.to_string(), "team", team);
    }

    /// Hides the floating menu (`dialog.hide()`).
    #[func]
    fn close_menu(&mut self) {
        self.set_menu_visible(false);
    }

    /// Bottom action row (`server.bans` / `server.admins` / `close`).
    #[func]
    fn menu_action(&mut self, action: GString) {
        match action.to_string().as_str() {
            "server.bans" => self.open_dialog("bans"),
            "server.admins" => self.open_dialog("admins"),
            _ => self.close_overlay(),
        }
    }

    /// `MindUi.confirm_result` answer for a pending Ban/Kick/Admin verb.
    #[func]
    fn apply_confirm(&mut self, confirmed: bool) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if confirmed {
            let sent = self.dispatch_player_action(&pending.player, &pending.action, 0);
            if !sent {
                // No relay (or no authority/rejected reducer): the shell never
                // fakes a server action. Java's list only exists in a live
                // match, so there is no upstream message for this state.
                log::info!(
                    "[ui] player_action {} on {} not sent",
                    pending.action,
                    pending.player
                );
            }
        }
    }

    /// `MindUi.text_input_result` answer for the votekick reason prompt.
    #[func]
    fn apply_votekick_reason(&mut self, reason: GString) {
        let Some(player) = self.pending_votekick.take() else {
            return;
        };
        let reason = reason.to_string();
        if reason.trim().is_empty() {
            return;
        }
        let command = votekick_command(&player, &reason);
        if let Some(mut ui) = kit::ui()
            && ui.has_method("chat_send")
        {
            let _ = ui.call(
                "chat_send",
                &[
                    GString::from(command.as_str()).to_variant(),
                    GString::from("normal").to_variant(),
                ],
            );
        }
    }
}

impl MindPlayerListFragment {
    fn build(&mut self) {
        // code-instantiated: the playerlist panel is a runtime overlay whose
        // rows are server-driven, so its structure is built once in code
        // (plan 14 §3.10).
        let mut center = CenterContainer::new_alloc();
        center.set_anchors_preset(LayoutPreset::FULL_RECT);
        self.base_mut().add_child(&center);

        let mut panel = PanelContainer::new_alloc();
        panel.set_theme_type_variation("defaultDialog");
        panel.set_custom_minimum_size(Vector2::new(PANEL_MIN_WIDTH, 420.0));
        center.add_child(&panel);

        let mut column = VBoxContainer::new_alloc();
        panel.add_child(&column);

        // `pane.label(() -> Core.bundle.format(players...))`.
        let header = factory::label("");
        column.add_child(&header);
        self.header = Some(header);

        let mut field = factory::field(&kit::t("@players.search").to_string());
        field.set_max_length(SEARCH_MAX_LENGTH);
        let callable = self.base().callable("search_changed");
        field.connect("text_changed", &callable);
        column.add_child(&field);
        self.search_field = Some(field);

        // `pane.pane(content).grow().scrollX(false)`.
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_h_size_flags(SizeFlags::EXPAND_FILL);
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
        scroll.set_custom_minimum_size(Vector2::new(0.0, 260.0));
        let mut content = VBoxContainer::new_alloc();
        content.set_h_size_flags(SizeFlags::EXPAND_FILL);
        content.set_v_size_flags(SizeFlags::EXPAND_FILL);
        scroll.add_child(&content);
        column.add_child(&scroll);
        self.content = Some(content);

        let mut menu = HBoxContainer::new_alloc();
        for action in MENU_ACTIONS {
            let mut button = factory::button(&kit::t(&format!("@{action}")).to_string());
            let mut target = self.to_gd();
            let action_name = GString::from(action);
            let callable = Callable::from_fn("player_menu", move |_args: &[&Variant]| {
                let _ = target.call("menu_action", &[action_name.to_variant()]);
            });
            button.connect("pressed", &callable);
            menu.add_child(&button);
            self.footer.push(button);
        }
        column.add_child(&menu);

        self.build_menu_overlay();
        self.rebuild();
    }

    /// The floating player menu: `BaseDialog`'s fill-parent overlay with a
    /// centred `defaultDialog` panel rebuilt per selected player.
    fn build_menu_overlay(&mut self) {
        // code-instantiated: the row menu is a runtime overlay over the list.
        let mut overlay = Control::new_alloc();
        overlay.set_anchors_preset(LayoutPreset::FULL_RECT);
        overlay.set_visible(false);
        let mut center = CenterContainer::new_alloc();
        center.set_anchors_preset(LayoutPreset::FULL_RECT);
        let mut panel = PanelContainer::new_alloc();
        panel.set_theme_type_variation("defaultDialog");
        panel.set_custom_minimum_size(Vector2::new(280.0, 0.0));
        let mut body = VBoxContainer::new_alloc();
        body.set_custom_minimum_size(Vector2::new(MENU_BUTTON_WIDTH + 20.0, 0.0));
        panel.add_child(&body);
        center.add_child(&panel);
        overlay.add_child(&center);
        self.base_mut().add_child(&overlay);
        self.menu = Some(overlay);
        self.menu_body = Some(body);
    }

    /// Builds the Ban/Kick/Trace/Team/Admin menu for one row.
    fn fill_player_menu(&mut self, row: &PlayerRow) {
        let Some(mut body) = self.menu_body.clone() else {
            return;
        };
        clear_children(&body);
        let title = factory::label(&row.display_name());
        body.add_child(&title);
        body.add_child(&accent_divider());
        let mut list = VBoxContainer::new_alloc();
        list.add_theme_constant_override("separation", 3);
        // `allowTeamSwitch` is `!state.isCampaign() && (rules.pvp ||
        // rules.infiniteResources)` upstream; the fragment has no rules feed, so
        // the relay's admin-authority check stands in (documented deviation).
        let is_self = row_is_local(row, &self.local_identity);
        let target = row_target(row);
        for action in menu_actions_for(row, is_self, true, self.is_server) {
            let button = menu_button(action, self.to_gd(), &target);
            list.add_child(&button);
        }
        body.add_child(&list);
        let mut back = factory::icon_button(
            "left",
            &kit::t("@back").to_string(),
            "defaultt",
            AlignmentMode::BEGIN,
        );
        back.set_custom_minimum_size(Vector2::new(MENU_BUTTON_WIDTH, MENU_BUTTON_HEIGHT));
        let callable = self.base().callable("close_menu");
        back.connect("pressed", &callable);
        body.add_child(&back);
    }

    /// The `teamSelect` body: one colour swatch per `Team.baseTeams` entry.
    fn fill_team_menu(&mut self, player: &str) {
        let Some(mut body) = self.menu_body.clone() else {
            return;
        };
        clear_children(&body);
        // `new BaseDialog(Core.bundle.get("player.team") + ": " + user.name)`.
        let label = self.row_label(player);
        let title = format!("{}: {label}", kit::t("@player.team"));
        body.add_child(&factory::label(&title));
        body.add_child(&accent_divider());
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(3);
        for team in mind_core::game::team::Team::base_teams() {
            let mut button = factory::image_button("whiteui", "clearNoneTogglei");
            button.set_custom_minimum_size(Vector2::new(TEAM_BUTTON_SIZE, TEAM_BUTTON_SIZE));
            button.set_tooltip_text(&team.localized());
            button.set_modulate(godot_color(team.color.to_rgba8888()));
            let mut target = self.to_gd();
            let player = GString::from(player);
            let id = i64::from(team.id);
            let callable = Callable::from_fn("player_team", move |_args: &[&Variant]| {
                let _ = target.call("choose_team", &[player.to_variant(), id.to_variant()]);
            });
            button.connect("pressed", &callable);
            grid.add_child(&button);
        }
        body.add_child(&grid);
        let mut back = factory::icon_button(
            "left",
            &kit::t("@back").to_string(),
            "defaultt",
            AlignmentMode::BEGIN,
        );
        back.set_custom_minimum_size(Vector2::new(MENU_BUTTON_WIDTH, MENU_BUTTON_HEIGHT));
        let callable = self.base().callable("close_menu");
        back.connect("pressed", &callable);
        body.add_child(&back);
    }

    /// One `PlayerListFragment` row.
    fn player_row(&mut self, row: &PlayerRow, total: usize) -> Gd<PanelContainer> {
        // code-instantiated: `rebuild` builds one row per live relay player.
        let mut panel = PanelContainer::new_alloc();
        panel.add_theme_stylebox_override("panel", &row_style());
        panel.set_custom_minimum_size(Vector2::new(0.0, ROW_HEIGHT + 14.0));
        let mut line = HBoxContainer::new_alloc();
        line.add_theme_constant_override("separation", 8);
        panel.add_child(&line);

        // `iconTable`: `Lines.rect` in the team colour around the player icon.
        let mut avatar = PanelContainer::new_alloc();
        avatar.add_theme_stylebox_override("panel", &avatar_style(team_color(row.team)));
        avatar.set_custom_minimum_size(Vector2::new(ROW_HEIGHT, ROW_HEIGHT));
        let glyph = factory::glyph_label("players", 26, team_color(row.team));
        avatar.add_child(&glyph);
        line.add_child(&avatar);

        // `labelWrap("[#" + user.color() + "]" + user.name())` — the roster has
        // no player colour, so the name keeps the default text colour.
        let mut name = factory::label(&row.display_name());
        name.set_h_size_flags(SizeFlags::EXPAND_FILL);
        line.add_child(&name);

        // `button.image(Icon.admin)`.
        if row.admin {
            line.add_child(&factory::glyph_label("admin", 20, styles::ACCENT));
        }

        // `if(net.server() || (player.admin && (!user.admin || user == player)))`
        // (`PlayerListFragment.rebuild`): otherwise the row menu is unavailable
        // and the votekick hammer is the only action (`else if` branch).
        let is_self = row_is_local(row, &self.local_identity);
        // `allowTeamSwitch` (rules pvp/infiniteResources) has no fragment feed
        // in the port; the relay's admin gate bounds the visible verbs.
        let allow_team_switch = true;
        let menu_visible = row_menu_visible(
            self.is_server,
            self.local_admin,
            row,
            is_self,
            allow_team_switch,
        );
        if menu_visible {
            let mut menu = factory::glyph_button("menu", "menu", "emptyi");
            menu.set_custom_minimum_size(Vector2::new(ROW_HEIGHT, ROW_HEIGHT));
            let mut target = self.to_gd();
            let player = GString::from(row_target(row).as_str());
            let callable = Callable::from_fn("player_row_menu", move |_args: &[&Variant]| {
                let _ = target.call("open_player_menu", &[player.to_variant()]);
            });
            menu.connect("pressed", &callable);
            line.add_child(&menu);
        } else if votekick_eligible(row, total, is_self, !self.is_server, self.local_team) {
            // `else if(!user.isLocal() && !user.admin && net.client() &&
            // Groups.player.size() >= 3 && player.team() == user.team())`.
            let mut hammer = factory::glyph_button("hammer", "kick", "emptyi");
            hammer.set_custom_minimum_size(Vector2::new(ROW_HEIGHT, ROW_HEIGHT));
            let mut target = self.to_gd();
            let player = GString::from(row_target(row).as_str());
            let callable = Callable::from_fn("player_row_votekick", move |_args: &[&Variant]| {
                let _ = target.call(
                    "menu_pressed",
                    &[player.to_variant(), GString::from("votekick").to_variant()],
                );
            });
            hammer.connect("pressed", &callable);
            line.add_child(&hammer);
        }
        panel
    }

    /// Asks `MindUi.show_confirm` for a Ban/Kick/Admin verb (`player` is the
    /// dispatch target; the message formats the display name like Java's
    /// `Core.bundle.format("confirmban", user.name)`).
    fn request_confirm(&mut self, player: &str, action: &str) {
        let Some(key) = confirm_key(action) else {
            return;
        };
        let label = self.row_label(player);
        let text = kit::tm_format(key, &[label.as_str()]).to_string();
        self.pending = Some(PendingAction {
            player: player.to_owned(),
            action: action.to_owned(),
        });
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("show_confirm", &[GString::from(text.as_str()).to_variant()]);
        }
    }

    /// `Trace` row button: request the server-computed trace
    /// (`Call.adminRequest(user, AdminAction.trace, null)`,
    /// `PlayerListFragment.java:168`) and open `TraceDialog` with the roster
    /// aggregation the relay already mirrors (Java opens it on `Call.traceInfo`).
    fn open_trace(&self, player: &str) {
        // The reducer also publishes the `Call.traceInfo` answer to this client
        // (`MindNet.player_action("trace")` -> `admin_request_trace`); a relay
        // without the target row simply leaves the dialog on the local view.
        self.dispatch_player_action(player, "trace", 0);
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let trace = if ui.has_method("player_trace_json") {
            ui.call("player_trace_json", &[GString::from(player).to_variant()])
                .try_to::<GString>()
                .map(|text| text.to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        // The dialog prints `Player.name`, not the identity it was requested by.
        let label = self.row_label(player);
        let context = trace_context_json(&label, &trace);
        if ui.has_method("open_dialog") {
            let _ = ui.call(
                "open_dialog",
                &[
                    GString::from("traces").to_variant(),
                    GString::from(context.as_str()).to_variant(),
                ],
            );
        }
    }

    /// `ui.showTextInput("@votekick.reason", ...)` (`Icon.hammer` row). The
    /// relay's `/votekick` resolves a current username (not an identity), so the
    /// command uses the display name.
    fn open_votekick(&mut self, player: &str) {
        let label = self.row_label(player);
        self.pending_votekick = Some(label.clone());
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("show_text_input") {
            return;
        }
        let message = kit::tm_format("@votekick.reason.message", &[label.as_str()]).to_string();
        let _ = ui.call(
            "show_text_input",
            &[
                kit::t("@votekick.reason").to_variant(),
                GString::from(message.as_str()).to_variant(),
                VOTE_REASON_MAX.to_variant(),
                GString::new().to_variant(),
                false.to_variant(),
                false.to_variant(),
            ],
        );
    }

    /// Forwards one row verb to `MindUi.player_action`/`player_action_with`.
    fn dispatch_player_action(&self, player: &str, action: &str, value: i64) -> bool {
        let Some(mut ui) = kit::ui() else {
            return false;
        };
        let with_value = action == "team" || value != 0;
        let method = if with_value {
            "player_action_with"
        } else {
            "player_action"
        };
        if !ui.has_method(method) {
            return false;
        }
        let mut args = vec![
            GString::from(player).to_variant(),
            GString::from(action).to_variant(),
        ];
        if with_value {
            args.push(value.to_variant());
        }
        ui.call(method, &args).try_to::<bool>().unwrap_or(false)
    }

    /// `MindUi.player_list_json()` text (`[]` when the facade is absent).
    fn player_list_payload(&self) -> String {
        let Some(mut ui) = kit::ui() else {
            return String::from("[]");
        };
        if !ui.has_method("player_list_json") {
            return String::from("[]");
        }
        ui.call("player_list_json", &[])
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_else(|_| String::from("[]"))
    }

    /// `MindUi.player_count()` (`Groups.player.size()`), when exposed.
    fn live_player_count(&self) -> Option<i64> {
        let mut ui = kit::ui()?;
        if !ui.has_method("player_count") {
            return None;
        }
        ui.call("player_count", &[]).try_to::<i64>().ok()
    }

    /// The row a menu callback targets (`Player.uuid` or, for an unresolved
    /// row, the display name the relay still resolves).
    fn row_by_target(&self, target: &str) -> Option<&PlayerRow> {
        self.rows
            .iter()
            .find(|row| row_target(row) == target || row.display_name() == target)
    }

    /// `Player.name` for a menu target: confirm/prompt text formats the
    /// username like Java, while the action itself carries the identity.
    fn row_label(&self, target: &str) -> String {
        self.row_by_target(target)
            .map(PlayerRow::display_name)
            .unwrap_or_else(|| target.to_owned())
    }

    /// Reads the local relay identity, host/admin rights and team
    /// (`net.server()`, `player.admin`, `player.team()` in
    /// `PlayerListFragment.rebuild`). Values stay empty/false without a net
    /// facade, so no row menu is offered when nothing can serve it.
    fn refresh_local_player(&mut self) {
        self.local_identity = self.net_text("local_identity_hex");
        self.is_server = self.net_bool("local_admin");
        self.local_team = self
            .net_i64("my_team")
            .and_then(|team| u8::try_from(team).ok());
        let identity = self.local_identity.clone();
        let self_admin = self
            .rows
            .iter()
            .any(|row| row_is_local(row, &identity) && row.admin);
        self.local_admin = self.is_server || self_admin;
    }

    /// `disabled(b -> net.client())` on the Bans/Admins footer buttons: only
    /// the hosting session can open the server's roster dialogs.
    fn sync_footer(&mut self) {
        for (index, button) in self.footer.iter_mut().enumerate() {
            if index < 2 {
                button.set_disabled(!self.is_server);
            }
        }
    }

    /// One optional `MindNet` accessor (`has_method`-guarded).
    fn net_call(&self, method: &str) -> Option<Variant> {
        let mut net = kit::net()?;
        if !net.has_method(method) {
            return None;
        }
        Some(net.call(method, &[]))
    }

    fn net_text(&self, method: &str) -> String {
        self.net_call(method)
            .and_then(|value| value.try_to::<GString>().ok())
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    fn net_bool(&self, method: &str) -> bool {
        self.net_call(method)
            .and_then(|value| value.try_to::<bool>().ok())
            .unwrap_or(false)
    }

    fn net_i64(&self, method: &str) -> Option<i64> {
        self.net_call(method)
            .and_then(|value| value.try_to::<i64>().ok())
    }

    /// `pane.label(... "players.single" : "players" ...)`.
    fn update_header(&mut self, count: usize) {
        let Some(mut header) = self.header.clone() else {
            return;
        };
        let count_text = count.to_string();
        let text = kit::tm_format(header_key(count), &[count_text.as_str()]).to_string();
        header.set_text(&text);
    }

    fn set_menu_visible(&mut self, visible: bool) {
        if let Some(mut menu) = self.menu.clone() {
            menu.set_visible(visible);
        }
    }

    /// Clears the search field like `toggle`'s `search.clearText()`.
    fn reset_search(&mut self) {
        self.search.clear();
        if let Some(mut field) = self.search_field.clone() {
            field.clear();
        }
    }

    /// Hides the overlay and resets its transient state (`toggle`'s close half,
    /// the footer `@close` button and Escape). Releasing the search field's
    /// focus matters: a hidden `LineEdit` that still owns keyboard focus would
    /// keep swallowing game input.
    fn close_overlay(&mut self) {
        self.close_menu();
        self.base_mut().set_visible(false);
        self.reset_search();
        if let Some(mut field) = self.search_field.clone() {
            field.release_focus();
        }
        self.base_mut().emit_signal("close_requested", &[]);
    }

    /// Whether a `MindUi` dialog or a sibling HUD overlay (console, fullscreen
    /// minimap) is above the player list. `MindUiRoot.on_escape` closes those
    /// first (`Control.java:746-750`), so the fragment must not consume the
    /// key then.
    fn overlay_above(&self) -> bool {
        if let Some(mut ui) = kit::ui()
            && ui.has_method("has_dialog")
            && ui.call("has_dialog", &[]).try_to::<bool>().unwrap_or(false)
        {
            return true;
        }
        ["console", "minimap"].iter().any(|name| {
            self.base()
                .get_node_or_null(format!("../{name}").as_str())
                .and_then(|node| node.try_cast::<Control>().ok())
                .is_some_and(|control| control.is_visible())
        })
    }

    /// `MindUi.open_dialog(name, ctx)` with the menu row's bundle key mapped to
    /// the manifest registry name (`@server.bans` -> `bans`).
    ///
    /// The Bans/Admins dialogs read their rows from the dialog context
    /// (`BansDialog.setup` re-reads `netServer.admins.getBanned()`), so the
    /// roster projections ride along: `MindNet.bans_json()`/`admins_json()`
    /// (the `all_bans`/`all_admins` subscription rows) land on the `banned`/
    /// `admins` keys `dialogs/misc.rs`'s `roster_entries` parses — without
    /// them the dialogs render their "none" empty state.
    fn open_dialog(&self, key: &str) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let context = self.dialog_context(key);
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from(key).to_variant(),
                GString::from(context.as_str()).to_variant(),
            ],
        );
    }

    /// Context JSON for a dialog opened from the footer: `{"banned": [...]}`
    /// from `MindNet.bans_json()` or `{"admins": [...]}` from
    /// `MindNet.admins_json()`; empty for any other dialog.
    fn dialog_context(&self, key: &str) -> String {
        match key {
            "bans" => roster_dialog_context("banned", &self.net_text("bans_json")),
            "admins" => roster_dialog_context("admins", &self.net_text("admins_json")),
            _ => String::new(),
        }
    }

    /// `confirm_result`/`text_input_result` hooks (once per fragment).
    fn connect_prompts(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if ui.has_signal("confirm_result") {
            let target = self.to_gd();
            let callable = Callable::from_fn("player_list_confirm", move |args: &[&Variant]| {
                let confirmed = args
                    .first()
                    .and_then(|value| value.try_to::<bool>().ok())
                    .unwrap_or(false);
                let target = target.clone();
                // The answer is emitted from inside a `MindUi` mutable borrow.
                defer_action("player_list_confirm_apply", move || {
                    if !target.is_instance_valid() {
                        return;
                    }
                    let mut node = target.clone();
                    let _ = node.call("apply_confirm", &[confirmed.to_variant()]);
                });
                Variant::nil()
            });
            ui.connect("confirm_result", &callable);
        }
        if ui.has_signal("text_input_result") {
            let target = self.to_gd();
            let callable =
                Callable::from_fn("player_list_text_input", move |args: &[&Variant]| {
                    let text = args
                        .first()
                        .and_then(|value| value.try_to::<GString>().ok())
                        .unwrap_or_default();
                    let target = target.clone();
                    defer_action("player_list_text_input_apply", move || {
                        if !target.is_instance_valid() {
                            return;
                        }
                        let mut node = target.clone();
                        let _ = node.call("apply_votekick_reason", &[text.to_variant()]);
                    });
                    Variant::nil()
                });
            ui.connect("text_input_result", &callable);
        }
    }
}

/// One queued one-shot action (`Callable.call_deferred` payload).
type DeferredAction = std::rc::Rc<std::cell::RefCell<Option<Box<dyn FnOnce()>>>>;

/// Queues `action` for the idle phase (`call_deferred`).
fn defer_action(name: &'static str, action: impl FnOnce() + 'static) {
    let slot: DeferredAction = std::rc::Rc::new(std::cell::RefCell::new(Some(Box::new(action))));
    let callable = Callable::from_fn(name, move |_args: &[&Variant]| {
        let queued = slot.borrow_mut().take();
        if let Some(action) = queued {
            action();
        }
        Variant::nil()
    });
    callable.call_deferred(&[]);
}

/// Removes every child of a container at the end of the frame.
fn clear_children(node: &Gd<VBoxContainer>) {
    for child in node.get_children().iter_shared() {
        let mut child = child.clone();
        child.queue_free();
    }
}

/// `MindUi.open_dialog` context for a roster dialog.
///
/// `key` is the context key the dialog controller reads
/// (`RosterKind::context_key`: `banned`/`admins`); `payload` is the roster
/// projection (`MindNet.bans_json`/`admins_json`). Malformed or absent payloads
/// degrade to an empty array, which renders the dialog's `@server.*.none` row.
fn roster_dialog_context(key: &str, payload: &str) -> String {
    let rows = serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .filter(serde_json::Value::is_array)
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let mut context = serde_json::Map::new();
    context.insert(key.to_owned(), rows);
    serde_json::Value::Object(context).to_string()
}

/// Parses `MindNet.players_json()` (`[]` on malformed/absent payloads).
fn parse_rows(json: &str) -> Vec<PlayerRow> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(array) = value.as_array() else {
        return Vec::new();
    };
    array.iter().filter_map(row_from_json).collect()
}

/// One `PlayerEntry::to_json` row (`mind-stdb` roster).
fn row_from_json(value: &serde_json::Value) -> Option<PlayerRow> {
    let identity = value
        .get("identity")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let name = value
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if identity.is_empty() && name.is_empty() {
        return None;
    }
    Some(PlayerRow {
        identity,
        name,
        team: value
            .get("team")
            .and_then(serde_json::Value::as_u64)
            .map(|team| team.min(u64::from(u8::MAX)) as u8),
        admin: value
            .get("admin")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        connected: value
            .get("connected")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        role: value
            .get("role")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("player")
            .to_owned(),
        kicked: value
            .get("kicked")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    })
}

/// `players.sort(comps(comparing(Player::team), comparingBool(p -> !p.admin)))`:
/// team first, admin rows before regular rows, then the roster's name order.
fn sort_rows(rows: &mut [PlayerRow]) {
    rows.sort_by(|left, right| {
        left.team
            .unwrap_or(u8::MAX)
            .cmp(&right.team.unwrap_or(u8::MAX))
            .then_with(|| right.admin.cmp(&left.admin))
            .then_with(|| left.name.cmp(&right.name))
    });
}

/// `Groups.player.size() == 1 ? "players.single" : "players"`.
fn header_key(count: usize) -> &'static str {
    if count == 1 {
        "players.single"
    } else {
        "players"
    }
}

/// The row menu verbs in Java order (`PlayerListFragment.rebuild`). `is_self`
/// hides the peer-only verbs; `can_admin` is Java's `!net.client() && !user.isLocal()`
/// (server-only admin toggle). `allow_team_switch` is the rules gate the
/// fragment cannot read (see [`row_menu_visible`]).
fn menu_actions_for(
    row: &PlayerRow,
    is_self: bool,
    allow_team_switch: bool,
    can_admin: bool,
) -> Vec<&'static str> {
    let mut actions = Vec::new();
    if !is_self {
        actions.extend(["ban", "kick", "trace"]);
    }
    if allow_team_switch || !is_self {
        actions.push("team");
    }
    if can_admin && !is_self {
        actions.push(if row.admin { "unadmin" } else { "admin" });
    }
    actions
}

/// Whether the roster row is the local player (`Player.isLocal()`): the relay
/// roster carries no local column, so the identity `MindNet.local_identity_hex`
/// returns is matched against the row identity.
fn row_is_local(row: &PlayerRow, local_identity: &str) -> bool {
    !local_identity.is_empty() && row.identity.eq_ignore_ascii_case(local_identity)
}

/// The action target for a row: the full identity (`Player.uuid`) so the relay
/// lookup is exact even when two peers share a name; a row whose username has
/// not arrived yet falls back to the display name (`roster.find` accepts both).
fn row_target(row: &PlayerRow) -> String {
    if row.identity.is_empty() {
        row.display_name()
    } else {
        row.identity.clone()
    }
}

/// Java `PlayerListFragment.rebuild` row-menu gate:
/// `net.server() || (player.admin && (!user.admin || user == player))`, then
/// `allowTeamSwitch || user != player` for the menu button itself.
///
/// A plain relay client therefore gets no per-row menu (unavailable state) and
/// falls through to the votekick hammer; the host and admins do.
fn row_menu_visible(
    server: bool,
    self_admin: bool,
    row: &PlayerRow,
    is_self: bool,
    allow_team_switch: bool,
) -> bool {
    (server || (self_admin && (!row.admin || is_self))) && (allow_team_switch || !is_self)
}

/// The icon + bundle key for one menu verb (Java `@player.*` buttons).
fn menu_verb(action: &str) -> (&'static str, &'static str) {
    match action {
        "ban" => ("hammer", "@player.ban"),
        "kick" => ("cancel", "@player.kick"),
        "trace" => ("zoom", "@player.trace"),
        "team" => ("redo", "@player.team"),
        "admin" | "unadmin" => ("admin", "@player.admin"),
        _ => ("none", "@none"),
    }
}

/// Builds one `t.button(text, Icon.x, bstyle, runnable)` menu row.
fn menu_button(action: &str, mut target: Gd<MindPlayerListFragment>, player: &str) -> Gd<Button> {
    let (icon, key) = menu_verb(action);
    let mut button = factory::icon_button(
        icon,
        &kit::t(key).to_string(),
        "defaultt",
        AlignmentMode::BEGIN,
    );
    button.set_custom_minimum_size(Vector2::new(MENU_BUTTON_WIDTH, MENU_BUTTON_HEIGHT));
    let player = GString::from(player);
    let verb = GString::from(action);
    let callable = Callable::from_fn("player_menu_press", move |_args: &[&Variant]| {
        let _ = target.call("menu_pressed", &[player.to_variant(), verb.to_variant()]);
    });
    button.connect("pressed", &callable);
    button
}

/// `ui.showConfirm` bundle key for a row verb.
fn confirm_key(action: &str) -> Option<&'static str> {
    match action {
        "ban" => Some("@confirmban"),
        "kick" => Some("@confirmkick"),
        "admin" => Some("@confirmadmin"),
        "unadmin" => Some("@confirmunadmin"),
        _ => None,
    }
}

/// Java votekick row (`PlayerListFragment.rebuild`, the `else if` of the menu
/// branch): `!user.isLocal() && !user.admin && net.client() &&
/// Groups.player.size() >= 3 && player.team() == user.team()`, on a connected
/// row. The local team/identity come from `MindNet.my_team`/
/// `MindNet.local_identity_hex`.
fn votekick_eligible(
    row: &PlayerRow,
    connected: usize,
    is_self: bool,
    client: bool,
    local_team: Option<u8>,
) -> bool {
    !is_self
        && row.connected
        && !row.admin
        && client
        && connected >= VOTE_MIN_PLAYERS
        && local_team.is_some_and(|team| row.team == Some(team))
}

/// `Call.sendChatMessage("/votekick #" + user.id + " " + reason)`; the relay
/// resolves the target name (`server/.../chat/mod.rs`).
fn votekick_command(name: &str, reason: &str) -> String {
    format!("/votekick {} {}", name.trim(), reason.trim())
}

/// `Team.get(id).color` packed `0xRRGGBBAA`.
fn team_color(team: Option<u8>) -> Color {
    godot_color(
        mind_core::game::team::Team::get(team.unwrap_or_default())
            .color
            .to_rgba8888(),
    )
}

/// Godot colour from a packed `0xRRGGBBAA` value.
fn godot_color(packed: u32) -> Color {
    Color::from_rgba8(
        ((packed >> 24) & 0xff) as u8,
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    )
}

/// `Tex.underline`: translucent black row with a hairline bottom rule.
fn row_style() -> Gd<StyleBox> {
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.25));
    style.set_border_width(Side::BOTTOM, 1);
    style.set_border_color(Color::from_rgba(1.0, 1.0, 1.0, 0.12));
    style.set_content_margin_all(5.0);
    style.upcast()
}

/// `iconTable` border: the team colour around the avatar.
fn avatar_style(team: Color) -> Gd<StyleBox> {
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.35));
    style.set_border_width(Side::LEFT, 2);
    style.set_border_width(Side::TOP, 2);
    style.set_border_width(Side::RIGHT, 2);
    style.set_border_width(Side::BOTTOM, 2);
    style.set_border_color(team);
    style.set_content_margin_all(4.0);
    style.upcast()
}

/// `dialog.cont.image(Tex.whiteui, Pal.accent).fillX().height(3f)`.
fn accent_divider() -> Gd<PanelContainer> {
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(styles::ACCENT);
    let style: Gd<StyleBox> = style.upcast();
    let mut panel = PanelContainer::new_alloc();
    panel.add_theme_stylebox_override("panel", &style);
    panel.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    panel
}

/// The search filter (`PlayerListFragment.rebuild`): `stripColors(name)`
/// lowercased contains the lowercased query.
fn matches_search(player_name: &str, search: &str) -> bool {
    search.is_empty()
        || mind_core::ui::player_list::strip_colors(&player_name.to_lowercase()).contains(search)
}

/// Whether `MindNet` is in a running match (`net.active() && state.isGame()`).
fn relay_in_game() -> bool {
    let Some(mut net) = kit::net() else {
        return false;
    };
    if !net.has_method("session_state") {
        return false;
    }
    net.call("session_state", &[])
        .try_to::<GString>()
        .map(|state| matches!(state.to_string().as_str(), "in_game" | "snapshot_sync"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, team: Option<u8>, admin: bool, connected: bool) -> PlayerRow {
        PlayerRow {
            identity: name.to_owned(),
            name: name.to_owned(),
            team,
            admin,
            connected,
            role: String::from("player"),
            kicked: false,
        }
    }

    #[test]
    fn search_filter_is_case_insensitive() {
        assert!(matches_search("Alice", "ali"));
        assert!(matches_search("Alice", ""));
        assert!(!matches_search("Alice", "bob"));
        // `Strings.stripColors`: tags never take part in the match.
        assert!(matches_search("[red]Bob[]", "bob"));
    }

    #[test]
    fn menu_actions_match_the_upstream_row() {
        assert_eq!(MENU_ACTIONS, ["server.bans", "server.admins", "close"]);
        let peer = row("Bob", Some(1), false, true);
        assert_eq!(
            menu_actions_for(&peer, false, true, true),
            ["ban", "kick", "trace", "team", "admin"]
        );
        assert_eq!(
            menu_actions_for(&peer, false, false, true),
            ["ban", "kick", "trace", "team", "admin"]
        );
        assert_eq!(
            menu_actions_for(&peer, true, false, true),
            Vec::<&str>::new()
        );
        // Java `!net.client()`: a relay client never gets the admin toggle.
        assert_eq!(
            menu_actions_for(&peer, false, true, false),
            ["ban", "kick", "trace", "team"]
        );
    }

    #[test]
    fn menu_admin_verb_toggles_with_the_flag() {
        let admin = row("Admin", Some(1), true, true);
        let actions = menu_actions_for(&admin, false, false, true);
        assert_eq!(actions.last(), Some(&"unadmin"));
        // Teammate-switch-only menus (Java `allowTeamSwitch || user != player`)
        // still expose `admin` when the local player is a server admin.
        assert!(menu_actions_for(&admin, false, false, false).contains(&"team"));
    }

    #[test]
    fn menu_verbs_use_the_java_icons_and_keys() {
        assert_eq!(menu_verb("ban"), ("hammer", "@player.ban"));
        assert_eq!(menu_verb("kick"), ("cancel", "@player.kick"));
        assert_eq!(menu_verb("trace"), ("zoom", "@player.trace"));
        assert_eq!(menu_verb("team"), ("redo", "@player.team"));
        assert_eq!(menu_verb("admin"), ("admin", "@player.admin"));
        assert_eq!(menu_verb("unadmin"), ("admin", "@player.admin"));
    }

    #[test]
    fn confirm_keys_match_the_bundle() {
        assert_eq!(confirm_key("ban"), Some("@confirmban"));
        assert_eq!(confirm_key("kick"), Some("@confirmkick"));
        assert_eq!(confirm_key("admin"), Some("@confirmadmin"));
        assert_eq!(confirm_key("unadmin"), Some("@confirmunadmin"));
        assert_eq!(confirm_key("trace"), None);
    }

    #[test]
    fn header_uses_the_singular_key_for_one_player() {
        assert_eq!(header_key(0), "players");
        assert_eq!(header_key(1), "players.single");
        assert_eq!(header_key(2), "players");
    }

    #[test]
    fn roster_dialog_contexts_carry_the_projection() {
        // PAR-0122/admin blocker: `open_dialog("bans", ctx)` must hand the
        // `BansDialog` its `banned` rows (the dialog's `setup` re-reads
        // `netServer.admins`), otherwise it renders `@server.bans.none`.
        let context = roster_dialog_context(
            "banned",
            r#"[{"id":"aa","lastIP":"","lastName":"Alice"},{"id":"bb","lastIP":"","lastName":"Bob"}]"#,
        );
        let value: serde_json::Value = serde_json::from_str(&context).expect("valid context");
        assert_eq!(value["banned"].as_array().map(Vec::len), Some(2));
        assert_eq!(value["banned"][0]["id"], "aa");
        assert_eq!(value["banned"][1]["lastName"], "Bob");
        // The admins dialog reads the `admins` key.
        let context = roster_dialog_context("admins", r#"[{"id":"cc","lastName":"Carol"}]"#);
        let value: serde_json::Value = serde_json::from_str(&context).expect("valid context");
        assert_eq!(value["admins"][0]["lastName"], "Carol");
        // Absent/malformed payloads degrade to an empty array (the dialog's
        // empty state), never to a missing key or invalid JSON.
        assert_eq!(
            roster_dialog_context("banned", ""),
            r#"{"banned":[]}"#,
            "no relay facade"
        );
        assert_eq!(
            roster_dialog_context("admins", "not-json"),
            r#"{"admins":[]}"#
        );
        assert_eq!(
            roster_dialog_context("banned", r#"{"id":"aa"}"#),
            r#"{"banned":[]}"#,
            "an object payload is not a row array"
        );
    }

    #[test]
    fn rows_parse_the_relay_shape() {
        let json = r#"[
            {"identity":"aa","name":"Alice","team":1,"admin":true,"connected":true,
             "role":"player","kicked":false},
            {"identity":"bb","name":"","team":null,"admin":false,"connected":false,
             "role":"spectator","kicked":true}
        ]"#;
        let rows = parse_rows(json);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "Alice");
        assert_eq!(rows[0].team, Some(1));
        assert!(rows[0].admin);
        assert!(rows[0].connected);
        assert_eq!(rows[1].team, None);
        assert_eq!(rows[1].role, "spectator");
        assert!(rows[1].kicked);
        // Unresolved username: the identity prefix keeps the row addressable.
        assert_eq!(rows[1].display_name(), "bb");
        assert!(parse_rows("[]").is_empty());
        assert!(parse_rows("not-json").is_empty());
        assert!(parse_rows("{}").is_empty());
    }

    #[test]
    fn rows_sort_by_team_then_admin() {
        let mut rows = vec![
            row("bob", Some(2), false, true),
            row("alice", Some(1), false, true),
            row("admin", Some(1), true, true),
            row("none", None, false, true),
        ];
        sort_rows(&mut rows);
        let names: Vec<&str> = rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["admin", "alice", "bob", "none"]);
    }

    #[test]
    fn votekick_needs_three_live_teammates_and_a_client_session() {
        let peer = row("Bob", Some(1), false, true);
        let admin = row("Admin", Some(1), true, true);
        let offline = row("Off", Some(1), false, false);
        // `Groups.player.size() >= 3`.
        assert!(!votekick_eligible(&peer, 2, false, true, Some(1)));
        assert!(votekick_eligible(&peer, 3, false, true, Some(1)));
        // `!user.admin`.
        assert!(!votekick_eligible(&admin, 5, false, true, Some(1)));
        // `!user.isLocal()`.
        assert!(!votekick_eligible(&peer, 5, true, true, Some(1)));
        // `net.client()`.
        assert!(!votekick_eligible(&peer, 5, false, false, Some(1)));
        // `player.team() == user.team()` (unassigned never qualifies).
        assert!(!votekick_eligible(&peer, 5, false, true, Some(2)));
        assert!(!votekick_eligible(&peer, 5, false, true, None));
        assert!(!votekick_eligible(&offline, 5, false, true, Some(1)));
        assert_eq!(
            votekick_command(" Bob ", " griefing "),
            "/votekick Bob griefing"
        );
    }

    #[test]
    fn row_menu_gate_matches_the_java_server_admin_clauses() {
        let peer = row("Bob", Some(1), false, true);
        let admin = row("Admin", Some(1), true, true);
        // `net.server()`: the host sees every row menu.
        assert!(row_menu_visible(true, true, &peer, false, true));
        assert!(row_menu_visible(true, true, &admin, false, true));
        // `player.admin && (!user.admin || user == player)`.
        assert!(row_menu_visible(false, true, &peer, false, true));
        assert!(!row_menu_visible(false, true, &admin, false, true));
        assert!(row_menu_visible(false, true, &admin, true, true));
        // A plain client gets no row menu at all (unavailable state).
        assert!(!row_menu_visible(false, false, &peer, false, true));
        assert!(!row_menu_visible(false, false, &peer, true, true));
        // Self only on `allowTeamSwitch`; the port's rules fallback keeps it on.
        assert!(row_menu_visible(true, true, &peer, true, true));
        assert!(!row_menu_visible(true, true, &peer, true, false));
    }

    #[test]
    fn local_rows_match_the_local_identity_case_insensitively() {
        let mut identity_row = row("Me", Some(1), false, true);
        identity_row.identity = String::from("ABCDEF12");
        assert!(row_is_local(&identity_row, "abcdef12"));
        assert!(!row_is_local(&identity_row, "abcdef"));
        assert!(!row_is_local(&identity_row, ""));
        assert!(!row_is_local(&row("Bob", Some(1), false, true), "abcdef12"));
    }

    #[test]
    fn row_target_prefers_the_identity_and_falls_back_to_the_name() {
        let peer = row("Bob", Some(1), false, true);
        assert_eq!(row_target(&peer), "Bob");
        let mut resolved = row("Bob", Some(1), false, true);
        resolved.identity = String::from("ab12cd34");
        assert_eq!(row_target(&resolved), "ab12cd34");
        // An unresolved username keeps the identity prefix `display_name`
        // derives so the row stays addressable.
        let mut unresolved = row("Bob", Some(1), false, true);
        unresolved.identity = String::from("ab12cd34");
        unresolved.name.clear();
        assert_eq!(row_target(&unresolved), "ab12cd34");
    }
}
