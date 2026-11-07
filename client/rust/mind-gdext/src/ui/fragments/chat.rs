// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/ChatFragment.java,
//         core/src/mindustry/core/NetServer.java (`ChatFormatter`),
//         core/src/mindustry/core/NetClient.java (`sendMessage`/`sendChatMessage`).

//! In-game chat fragment (`ChatFragment`) + its `my_match_chat` row feed.
//!
//! Java renders chat lines as pre-formatted strings pushed through
//! `NetClient.sendMessage` → `ui.chatfrag.addMessage`: `ChatFormatter` wraps the
//! author as `[coral][[<coloredName>][coral]]:[white] <text>`, team chat adds a
//! `[#<teamColor>]<T>` prefix and the admin channel a `[#ff4000]<A>` prefix
//! (`NetServer.java:376,387`). This port receives structured
//! `mind-stdb` `ChatLine` rows instead (PAR-0121/0125), so the same markup is
//! rebuilt here from the row columns.
//!
//! The input state machine (modes, history, ten-message buffer, ping parse,
//! scroll clamp) lives in the unit-tested `mind_core::ui::chat::ChatState`; this
//! module only owns the Godot side and the row → markup mapping.
//!
//! Visibility: the module's `my_match_chat` view already filters server-side
//! (`chat_visible`). [`ChatFeed`] re-applies `ChatLine::visible_to` when the
//! local viewer identity is known (an admin revoke or team switch must drop
//! retained rows immediately), and trusts the view while it is not — the relay
//! roster exposes no local identity yet (roster deviation D2), and dropping
//! the rows there would hide the player's own team chat and private notices.
//! This is deliberately *not* `mind_stdb::ChatLog`: `ChatLog::set_viewer(None,
//! ..)` treats an unknown viewer as "sees nothing private/team", which is the
//! wrong answer for a pre-filtered view.

use std::collections::{BTreeMap, HashMap};

use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, ButtonGroup, Control, HBoxContainer, IControl, LineEdit, RichTextLabel, VBoxContainer,
};
use godot::obj::NewAlloc;
use godot::prelude::*;
use mind_core::ui::chat::{ChatMode, ChatState, MESSAGES_SHOWN};
use mind_stdb::module_bindings::ChatKind;
use mind_stdb::{CHAT_HISTORY_LIMIT, ChatLine};

use crate::ui::kit;
use crate::ui::kit::margins;
use crate::ui::kit::roster::{self, RosterName};
use crate::ui::kit::widgets::factory;

/// `Pal.adminChat` (`ff4000`), the admin-channel prefix colour
/// (`NetServer.java:387`).
const ADMIN_CHAT_COLOR: &str = "#ff4000";
/// `Pal.accent` (`ffd37f`), the fallback author colour.
const ACCENT_COLOR: &str = "#ffd37f";
/// `Pal.gray` (`454545`)… Java's system lines are pre-coloured; the port's
/// system/notice rows use the muted `lightgray` Arc colour.
const NOTICE_COLOR: &str = "lightgray";
/// `Color.scarlet`: denied chat (mute/spam notices).
const DENY_COLOR: &str = "scarlet";
/// `Core.settings.getInt("chatopacity")` default (the settings row default).
const CHAT_OPACITY_DEFAULT: i64 = 100;
/// Width of the message column (`min(graphics.getWidth()/1.5f, Scl.scl(700f))`
/// at the 1152 px reference viewport).
const CHAT_WIDTH: f32 = 700.0;
/// `ChatFragment.chatspace`/`offsety` at the reference scale.
const CHAT_ROW_SPACING: f32 = 10.0;
/// `Core.settings` key for the chat opacity slider.
const CHAT_OPACITY_KEY: &str = "chatopacity";

/// Channel names in `ChatMode` declaration order (`normal`, `team`, `admin`).
const MODE_NAMES: [&str; 3] = ["normal", "team", "admin"];

/// `ChatMode` declaration index (the ABI order of [`MODE_NAMES`]).
fn mode_index(mode: ChatMode) -> usize {
    match mode {
        ChatMode::Normal => 0,
        ChatMode::Team => 1,
        ChatMode::Admin => 2,
    }
}

/// The ported client command hint shown next to the field (`NetServer.java:347-510`
/// registers `/help`, `/t`, `/a`, `/votekick`, `/vote`, `/sync`).
const COMMAND_HINT: &str = "/t  /a  /help  /vote  /votekick";

/// Stable channel name → `ChatKind` (the `chat::kind_name` ABI, reversed).
pub fn kind_from_name(name: &str) -> Option<ChatKind> {
    match name {
        "all" => Some(ChatKind::All),
        "team" => Some(ChatKind::Team),
        "system" => Some(ChatKind::System),
        "admin" => Some(ChatKind::Admin),
        _ => None,
    }
}

/// Parses one `mind_stdb::ChatLine::to_json` row (`None` when malformed).
pub fn parse_chat_line(value: &serde_json::Value) -> Option<ChatLine> {
    let kind = kind_from_name(value.get("kind")?.as_str()?)?;
    Some(ChatLine {
        chat_id: value.get("id")?.as_u64()?,
        match_id: value.get("matchId").and_then(serde_json::Value::as_u64)?,
        sender: value.get("sender")?.as_str()?.to_owned(),
        kind,
        team: value
            .get("team")
            .and_then(serde_json::Value::as_u64)
            .and_then(|team| u8::try_from(team).ok()),
        text: value
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        sent_at_micros: value
            .get("sentAt")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
        target: value
            .get("target")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    })
}

/// Parses a chat-row payload: a JSON array of rows (or a single row object).
pub fn parse_rows(json: &str) -> Vec<ChatLine> {
    match serde_json::from_str::<serde_json::Value>(json) {
        Ok(serde_json::Value::Array(rows)) => rows.iter().filter_map(parse_chat_line).collect(),
        Ok(value @ serde_json::Value::Object(_)) => parse_chat_line(&value).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Bounded, de-duplicated mirror of `my_match_chat` (the fragment's data feed).
#[derive(Debug, Clone)]
pub struct ChatFeed {
    lines: BTreeMap<u64, ChatLine>,
    viewer: Option<String>,
    viewer_team: Option<u8>,
    admin: bool,
    limit: usize,
    revision: u64,
}

impl Default for ChatFeed {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // the feed's documented surface: the fragment reads rows through JSON, tests and the net push path use the accessors.
impl ChatFeed {
    /// Empty feed with the relay history limit.
    pub fn new() -> Self {
        Self {
            lines: BTreeMap::new(),
            viewer: None,
            viewer_team: None,
            admin: false,
            limit: CHAT_HISTORY_LIMIT,
            revision: 0,
        }
    }

    /// Empty feed retaining at most `limit` rows (clamped to at least one).
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            ..Self::new()
        }
    }

    /// Sets the local viewer (identity hex) used to re-filter retained rows.
    ///
    /// With no identity the feed trusts the server view (see the module note)
    /// but still filters by team/admin, which the client always knows.
    pub fn set_viewer(&mut self, identity: Option<&str>, team: Option<u8>, admin: bool) {
        self.viewer = identity
            .map(str::trim)
            .filter(|identity| !identity.is_empty())
            .map(|identity| identity.to_ascii_lowercase());
        self.viewer_team = team;
        self.admin = admin;
        self.retain_visible();
    }

    /// Local viewer identity, when set.
    pub fn viewer(&self) -> Option<&str> {
        self.viewer.as_deref()
    }

    /// Viewer team, when set.
    pub fn viewer_team(&self) -> Option<u8> {
        self.viewer_team
    }

    /// Whether the viewer is an admin (the admin channel/private rows).
    pub fn viewer_admin(&self) -> bool {
        self.admin
    }

    /// Retention limit.
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Applies a JSON row payload; returns the number of rows retained.
    pub fn apply_json(&mut self, rows_json: &str) -> usize {
        for line in parse_rows(rows_json) {
            self.apply_line(line);
        }
        self.lines.len()
    }

    /// Applies one row (insert/update in place, then trim).
    pub fn apply_line(&mut self, line: ChatLine) {
        let changed = self.lines.get(&line.chat_id) != Some(&line);
        if self.visible(&line) {
            self.lines.insert(line.chat_id, line);
            self.trim();
        } else {
            self.lines.remove(&line.chat_id);
        }
        if changed {
            self.revision = self.revision.saturating_add(1);
        }
    }

    /// Drops every retained row (disconnect, `/sync`).
    pub fn clear(&mut self) {
        if !self.lines.is_empty() {
            self.revision = self.revision.saturating_add(1);
        }
        self.lines.clear();
    }

    /// Monotonic change counter (the fragment re-reads only when it moves).
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// One retained row by id.
    pub fn line(&self, chat_id: u64) -> Option<&ChatLine> {
        self.lines.get(&chat_id)
    }

    /// Retained rows in chronological (`chat_id`) order.
    pub fn lines(&self) -> Vec<&ChatLine> {
        self.lines.values().collect()
    }

    /// Greatest retained id, if any (the fragment's replay watermark).
    pub fn last_id(&self) -> Option<u64> {
        self.lines.keys().next_back().copied()
    }

    /// Retained row count.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Whether no row is retained.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Whether `line` may render for the current viewer.
    pub fn visible(&self, line: &ChatLine) -> bool {
        // No identity: the view already filtered; apply only what the client
        // knows for sure (team while assigned, admin flag).
        let identity = self.viewer.as_deref();
        let team = self.viewer_team;
        if line.private() {
            return match identity {
                Some(viewer) => line
                    .target
                    .as_deref()
                    .is_some_and(|target| target.eq_ignore_ascii_case(viewer)),
                // The view emits private rows only to their target, so a
                // private row that arrives without a local identity is ours.
                None => true,
            };
        }
        match line.kind {
            ChatKind::All | ChatKind::System => true,
            ChatKind::Team => line.team.is_some() && line.team == team,
            ChatKind::Admin => self.admin,
        }
    }

    /// The whole feed as `ChatLine` JSON (what the fragment consumes).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::Value::Array(self.lines.values().map(ChatLine::to_json).collect())
    }

    fn retain_visible(&mut self) {
        let before = self.lines.len();
        let lines = std::mem::take(&mut self.lines);
        for (id, line) in lines {
            if self.visible(&line) {
                self.lines.insert(id, line);
            }
        }
        self.trim();
        if self.lines.len() != before {
            self.revision = self.revision.saturating_add(1);
        }
    }

    fn trim(&mut self) {
        while self.lines.len() > self.limit {
            let Some(oldest) = self.lines.keys().next().copied() else {
                break;
            };
            self.lines.remove(&oldest);
        }
    }
}

/// Whether `text` is a server denial notice (mute/spam), rendered scarlet.
///
/// `server/spacetimedb/src/chat/mod.rs` sends "You are muted for another N
/// second(s)." and "You may not send the same message twice." as private system
/// rows; Java colours these rejections `[scarlet]`.
pub fn is_denial_notice(text: &str) -> bool {
    let text = text.trim_start();
    text.starts_with("You are muted") || text.starts_with("You may not")
}

/// The `<T>`/`<A>` channel prefix (`NetServer.java:376,387`).
pub fn channel_prefix(line: &ChatLine) -> Option<String> {
    match line.kind {
        ChatKind::Team => Some(format!(
            "[#{}]<T> ",
            line.team
                .map(|team| mind_core::game::team::Team::get(team).hex())
                .unwrap_or_else(|| ACCENT_COLOR.trim_start_matches('#').to_owned())
        )),
        ChatKind::Admin => Some(format!("[{ADMIN_CHAT_COLOR}]<A> ")),
        ChatKind::All | ChatKind::System => None,
    }
}

/// One chat row as Java markup (the string `ChatFragment.addMessage` receives).
///
/// System notices (and private rows, which the module only sends as system
/// notices) render muted; mute/spam denials render scarlet. Everything else
/// uses the upstream `ChatFormatter` shape.
pub fn format_line(line: &ChatLine, names: &HashMap<String, RosterName>) -> String {
    let text = line.text.trim_end();
    if line.kind == ChatKind::System || line.private() {
        let color = if is_denial_notice(text) {
            DENY_COLOR
        } else {
            NOTICE_COLOR
        };
        return format!("[{color}]{text}[]");
    }
    let author = roster::colored_name(&line.sender, names);
    let message = format!("[coral][[{author}[coral]]:[white] {}", text);
    match channel_prefix(line) {
        Some(prefix) => format!("{prefix}{message}"),
        None => message,
    }
}

/// `MindChatFragment` — the HUD chat overlay (`ChatFragment`).
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindChatFragment {
    base: Base<Control>,
    /// `ChatFragment` input/history/buffer state (unit-tested in `mind-core`).
    state: ChatState,
    /// Replay watermark into `MindUi.chat_rows_json()`.
    last_chat_id: u64,
    /// `MindUi.chat_revision()` last consumed.
    revision: i64,
    /// `Core.settings.getInt("chatopacity") / 100f` cache.
    opacity: f64,
    /// Roster names for the author prefixes (refreshed when it changes).
    names: HashMap<String, RosterName>,
    panel: Option<Gd<VBoxContainer>>,
    message_box: Option<Gd<VBoxContainer>>,
    field: Option<Gd<LineEdit>>,
    prompt: Option<Gd<RichTextLabel>>,
    hint: Option<Gd<RichTextLabel>>,
    tabs: Vec<Gd<Button>>,
    /// Per-message labels in *chronological* render order (newest last).
    labels: Vec<Gd<RichTextLabel>>,
    /// Newest-first `messages` index per rendered label (the `fadetime - i`
    /// fade index used by `ChatFragment.draw`).
    label_indices: Vec<usize>,
    /// Whether the local player is an admin (`ChatMode.admin` validity).
    admin: bool,
}

#[godot_api]
impl IControl for MindChatFragment {
    fn ready(&mut self) {
        self.base_mut().set_anchors_preset(LayoutPreset::FULL_RECT);
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
        self.build();
        self.base_mut().set_visible(false);
        self.base_mut().set_process(true);
        log::info!(
            "[ui] MindChatFragment ready (net={}, tabs={})",
            net_active(),
            self.tabs.len()
        );
    }

    fn process(&mut self, delta: f64) {
        // `visible(() -> net.active() && ui.hudfrag.shown())`.
        let active = net_active();
        let hud = hud_visible();
        self.base_mut().set_visible(active && hud);
        if !active {
            // `if(!net.active() && messages.size > 0){ clearMessages(); if(shown) hide(); }`.
            if !self.state.messages.is_empty() {
                self.state.clear_messages();
                self.rebuild_messages();
            }
            if self.state.shown {
                self.hide_input();
            }
            return;
        }
        self.admin = local_admin();
        self.refresh_feed();
        self.opacity = chat_opacity();
        // `if(fadetime > 0 && !shown) fadetime -= Time.delta / 180f;`.
        if self.state.shown {
            self.state.fadetime = MESSAGES_SHOWN as f32;
        } else if self.state.fadetime > 0.0 {
            self.state.fadetime = 0.0_f32.max(self.state.fadetime - (delta / 3.0) as f32);
        }
        self.apply_fade();
        self.refresh_tabs();
    }
}

#[godot_api]
impl MindChatFragment {
    /// Adds a pre-formatted message to the rolling buffer (`ChatFragment.addMessage`).
    ///
    /// The relay path uses [`Self::refresh_feed`]; this stays for callers that
    /// already hold a `ChatFormatter` string (campaign/system notices).
    #[func]
    pub fn add_message(&mut self, message: GString) {
        if message.is_empty() {
            return;
        }
        self.state.add_message(message.to_string());
        self.rebuild_messages();
    }

    /// Shows/hides the input line and focuses it (`ChatFragment.toggle`).
    ///
    /// Java defers the send by two ticks when closing (`ChatFragment.java:260`);
    /// the port sends immediately, which is the same observable behaviour for a
    /// field whose text is only changed by the player.
    #[func]
    pub fn toggle(&mut self) {
        // `ChatFragment.java:65`: `net.active() && input.keyTap(Binding.chat)
        // && !hasOtherFocus && !lastFrameHadFocus && !ui.consolefrag.shown()`.
        // `MindInput` already filters text focus/dialogs before the key edge
        // reaches this method; the net and console gates stay here so the
        // input seam and a direct `toggle()` call agree.
        if !net_active() || console_shown() {
            return;
        }
        if self.state.shown {
            self.submit_current();
            self.hide_input();
        } else {
            self.state.shown = true;
            self.set_input_visible(true);
            if let Some(mut field) = self.field.clone() {
                field.grab_focus();
            }
            self.reset_field();
        }
    }

    /// Closes the input line without sending (`ChatFragment.hide`).
    ///
    /// Named `hide_input` because `CanvasItem.hide()` already exists (Godot
    /// warns about the shadowed method and a GDScript `hide()` call would
    /// resolve to the engine one).
    #[func]
    pub fn hide_input(&mut self) {
        self.state.shown = false;
        self.state.clear_input();
        self.state.scroll_pos = 0;
        if let Some(mut field) = self.field.clone() {
            field.set_text("");
            field.release_focus();
        }
        self.set_input_visible(false);
    }

    /// Whether the input line is open (`ChatFragment.shown`).
    #[func]
    pub fn is_open(&self) -> bool {
        self.state.shown
    }

    /// Cycles the chat mode (normal -> team -> admin for admins).
    #[func]
    fn next_mode(&mut self) {
        let input = self
            .field
            .as_ref()
            .map(|field| field.get_text().to_string())
            .unwrap_or_default();
        let next = self.state.next_mode(self.admin, &input);
        if let Some(mut field) = self.field.clone() {
            field.set_text(&GString::from(next.as_str()));
            field.set_caret_column(next.chars().count() as i32);
        }
        self.refresh_tabs();
    }

    /// Selects one mode tab (`normal`/`team`/`admin`).
    #[func]
    pub fn set_mode(&mut self, mode: GString) {
        let Some(index) = MODE_NAMES
            .iter()
            .position(|name| *name == mode.to_string().as_str())
        else {
            return;
        };
        let target = ChatMode::ALL[index];
        if !target.is_valid(self.admin) {
            return;
        }
        let previous = self.state.mode;
        self.state.mode = target;
        let input = self
            .field
            .as_ref()
            .map(|field| field.get_text().to_string())
            .unwrap_or_default();
        let text = match input.strip_prefix(previous.normalized_prefix()) {
            Some(rest) => format!("{}{}", target.normalized_prefix(), rest),
            None if input.is_empty() => target.normalized_prefix().to_owned(),
            None => input,
        };
        if let Some(mut field) = self.field.clone() {
            field.set_text(&GString::from(text.as_str()));
            field.set_caret_column(text.chars().count() as i32);
        }
        self.refresh_tabs();
        self.refresh_hint();
    }

    /// Recalls an older input (`Binding.chatHistoryPrev`).
    #[func]
    pub fn history_prev(&mut self) {
        let current = self
            .field
            .as_ref()
            .map(|field| field.get_text().to_string())
            .unwrap_or_default();
        if self.state.history_prev(&current) {
            self.apply_history();
        }
    }

    /// Recalls a newer input (`Binding.chatHistoryNext`).
    #[func]
    pub fn history_next(&mut self) {
        if self.state.history_next() {
            self.apply_history();
        }
    }

    /// Scrolls the message window (`Binding.chatScroll` axis).
    #[func]
    pub fn scroll(&mut self, delta: i32) {
        self.state.scroll(delta);
        self.rebuild_messages();
    }

    /// `LineEdit.text_submitted`: sends the field text.
    #[func]
    fn submit(&mut self, _text: GString) {
        self.submit_current();
        self.hide_input();
    }

    /// `LineEdit.text_changed`: keeps the `/` hint in sync.
    #[func]
    fn on_text_changed(&mut self, _text: GString) {
        self.refresh_hint();
    }
}

impl MindChatFragment {
    fn build(&mut self) {
        // code-instantiated: chat is a persistent HUD fragment whose
        // rows/messages are runtime data, so the layout is built once here.
        let mut panel = VBoxContainer::new_alloc();
        panel.set_anchor(Side::LEFT, 0.0);
        panel.set_anchor(Side::TOP, 1.0);
        panel.set_anchor(Side::BOTTOM, 1.0);
        panel.set_offset(Side::BOTTOM, -8.0);
        panel.set_offset(Side::LEFT, margins::left());
        panel.set_offset(Side::RIGHT, CHAT_WIDTH);
        panel.set_mouse_filter(MouseFilter::IGNORE);
        panel.add_theme_constant_override("separation", CHAT_ROW_SPACING as i32);
        self.base_mut().add_child(&panel);

        // code-instantiated: the message list is the runtime chat buffer.
        let mut messages = VBoxContainer::new_alloc();
        messages.set_mouse_filter(MouseFilter::IGNORE);
        messages.add_theme_constant_override("separation", 2);
        panel.add_child(&messages);

        // `>` field label + the `/` command hint row (`ChatFragment.fieldlabel`).
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 6);
        let prompt_label = factory::label(">");
        let mut prompt: Gd<RichTextLabel> = prompt_label.upcast();
        prompt.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        row.add_child(&prompt);
        let mut field = factory::field("");
        field.set_h_size_flags(SizeFlags::EXPAND_FILL);
        // `chatfield.setMaxLength(Vars.maxTextLength)` (`Vars.java:109`).
        field.set_max_length(150);
        let callable = self.base().callable("submit");
        field.connect("text_submitted", &callable);
        let callable = self.base().callable("on_text_changed");
        field.connect("text_changed", &callable);
        row.add_child(&field);
        panel.add_child(&row);

        let hint_label = factory::label(COMMAND_HINT);
        let mut hint: Gd<RichTextLabel> = hint_label.upcast();
        hint.set_tooltip_text(COMMAND_HINT);
        hint.set_mouse_filter(MouseFilter::IGNORE);
        panel.add_child(&hint);

        // Mode tabs (port addition; Java has one cycle button): the command
        // prefixes make the channel explicit.
        let mut tabs_row = HBoxContainer::new_alloc();
        tabs_row.add_theme_constant_override("separation", 2);
        let group = ButtonGroup::new_gd();
        let mut tabs = Vec::new();
        for (index, label) in ["all", "/t", "/a"].into_iter().enumerate() {
            // code-instantiated: tabs are fixed HUD chrome, one per `ChatMode`.
            let mut button = Button::new_alloc();
            button.set_text(label);
            button.set_tooltip_text(&GString::from(MODE_NAMES[index]));
            button.set_toggle_mode(true);
            button.set_button_group(&group);
            button.set_theme_type_variation("clearNoneTogglei");
            button.set_custom_minimum_size(Vector2::new(44.0, 26.0));
            button.set_pressed_no_signal(index == 0);
            let target = self.to_gd();
            let mode = GString::from(MODE_NAMES[index]);
            let select = Callable::from_fn("chat_mode_tab", move |_args: &[&Variant]| {
                if target.is_instance_valid() {
                    let mut node = target.clone();
                    let mut bound = node.bind_mut();
                    bound.set_mode(mode.clone());
                }
                Variant::nil()
            });
            button.connect("pressed", &select);
            tabs_row.add_child(&button);
            tabs.push(button);
        }
        panel.add_child(&tabs_row);

        if mobile() {
            panel.set_offset(Side::BOTTOM, -105.0);
            panel.set_offset(Side::RIGHT, 480.0);
        }

        self.panel = Some(panel);
        self.message_box = Some(messages);
        self.field = Some(field);
        self.prompt = Some(prompt);
        self.hint = Some(hint);
        self.tabs = tabs;
        self.set_input_visible(false);
        self.refresh_hint();
        self.refresh_tabs();
    }

    /// Shows/hides the input chrome (`chatfield.visible = shown`).
    fn set_input_visible(&mut self, visible: bool) {
        if let Some(mut field) = self.field.clone() {
            field.set_visible(visible);
        }
        if let Some(mut prompt) = self.prompt.clone() {
            prompt.set_visible(visible);
        }
        if let Some(mut hint) = self.hint.clone() {
            hint.set_visible(visible);
        }
        for tab in &mut self.tabs {
            tab.set_visible(visible);
        }
    }

    /// Resets the field to the active mode's prefix (`clearChatInput`).
    fn reset_field(&mut self) {
        let prefix = self.state.mode.normalized_prefix().to_owned();
        if let Some(mut field) = self.field.clone() {
            field.set_text(&GString::from(prefix.as_str()));
            field.set_caret_column(prefix.chars().count() as i32);
        }
        self.refresh_hint();
    }

    fn apply_history(&mut self) {
        let text = self.state.history_text();
        if let Some(mut field) = self.field.clone() {
            field.set_text(&GString::from(text.as_str()));
            field.set_caret_column(text.chars().count() as i32);
        }
        self.refresh_hint();
    }

    /// `ChatFragment.sendMessage` + `MindUi.chat_send`: trim, guard, history,
    /// ping parse, then the relay.
    fn submit_current(&mut self) {
        let raw = self
            .field
            .as_ref()
            .map(|field| field.get_text().to_string())
            .unwrap_or_default();
        if raw.trim().is_empty() {
            self.state.clear_input();
            return;
        }
        let admin = self.admin;
        let map = map_size();
        let Some(send) = self.state.send(&raw, admin, map) else {
            return;
        };
        if let Some(ping) = send.ping.as_ref() {
            // `Call.pingLocation`: the relay has no ping reducer yet and no
            // `#[func]` exposes the world grid, so the bounds are the whole
            // `i32` range (`check_ping` still parses the ping); the message
            // text already carries `x,y` for the chat line (documented
            // deviation, PAR-0121 client remainder).
            log::info!("[chat] ping {},{} `{}`", ping.x, ping.y, ping.text);
        }
        let mode = MODE_NAMES[mode_index(self.state.mode)];
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("chat_send") {
            return;
        }
        let sent = ui
            .call(
                "chat_send",
                &[
                    GString::from(send.message.as_str()).to_variant(),
                    GString::from(mode).to_variant(),
                ],
            )
            .try_to::<bool>()
            .unwrap_or(false);
        if !sent {
            log::info!("[chat] no relay for `{mode}` chat; message not sent");
        }
    }

    /// Pulls new rows from `MindUi.chat_poll()`/`MindUi.chat_rows_json()` and
    /// appends their markup.
    fn refresh_feed(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("chat_rows_json") || !ui.has_method("chat_poll") {
            return;
        }
        let revision = ui
            .call("chat_poll", &[])
            .try_to::<i64>()
            .unwrap_or(self.revision);
        // The roster supplies author names; refresh only on a revision change.
        if revision != self.revision {
            self.revision = revision;
            if ui.has_method("player_list_json") {
                let rows = ui
                    .call("player_list_json", &[])
                    .try_to::<GString>()
                    .unwrap_or_default();
                self.names = roster::roster_index(&roster::roster_names(&rows.to_string()));
            }
            // A cleared feed (`/sync`, disconnect) drops the buffer with it.
            if ui.call("chat_len", &[]).try_to::<i64>().unwrap_or(0) == 0
                && !self.state.messages.is_empty()
            {
                self.state.clear_messages();
                self.rebuild_messages();
            }
        }
        let rows = ui
            .call("chat_rows_json", &[])
            .try_to::<GString>()
            .unwrap_or_default();
        let lines = parse_rows(&rows.to_string());
        let mut changed = false;
        for line in lines {
            if line.chat_id <= self.last_chat_id {
                continue;
            }
            self.last_chat_id = line.chat_id;
            self.state.add_message(format_line(&line, &self.names));
            changed = true;
        }
        if changed {
            self.rebuild_messages();
        }
    }

    /// Rebuilds the per-message labels (`messagesShown` window, newest last).
    fn rebuild_messages(&mut self) {
        let Some(mut box_node) = self.message_box.clone() else {
            return;
        };
        for label in self.labels.drain(..) {
            let mut label: Gd<RichTextLabel> = label;
            if label.is_instance_valid() {
                box_node.remove_child(&label);
                label.queue_free();
            }
        }
        self.label_indices.clear();
        let end = self
            .state
            .messages
            .len()
            .saturating_sub(self.state.scroll_pos);
        let start = end.saturating_sub(MESSAGES_SHOWN);
        // `messages` is newest-first; render oldest-first so the newest line
        // sits at the bottom like `ChatFragment.draw`'s bottom-up pass.
        for index in (start..end).rev() {
            let Some(text) = self.state.messages.get(index) else {
                continue;
            };
            // code-instantiated: one label per retained chat line (per-line fade).
            let mut label = RichTextLabel::new_alloc();
            label.set_use_bbcode(true);
            label.set_fit_content(true);
            label.set_scroll_active(false);
            label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
            label.set_custom_minimum_size(Vector2::new(CHAT_WIDTH, 0.0));
            label.set_mouse_filter(MouseFilter::IGNORE);
            label.set_text(&kit::markup_text(text));
            box_node.add_child(&label);
            self.labels.push(label);
            self.label_indices.push(index);
        }
        self.apply_fade();
    }

    /// `opacity` + the per-message fade (`ChatFragment.draw`).
    fn apply_fade(&mut self) {
        for (position, label) in self.labels.iter_mut().enumerate() {
            let fade_index = self.label_indices.get(position).copied().unwrap_or(0);
            let mut alpha = self.opacity as f32;
            if !self.state.shown {
                let remaining = self.state.fadetime - fade_index as f32;
                alpha *= remaining.clamp(0.0, 1.0);
            }
            let mut modulate = label.get_modulate();
            modulate.a = alpha.clamp(0.0, 1.0);
            label.set_modulate(modulate);
        }
    }

    /// Highlights the active mode tab and hides the admin tab without rights.
    fn refresh_tabs(&mut self) {
        let mode = self.state.mode;
        for (index, tab) in self.tabs.iter_mut().enumerate() {
            tab.set_pressed_no_signal(ChatMode::ALL[index] == mode);
            if index == 2 {
                tab.set_visible(self.admin);
            }
        }
    }

    /// The `/` command hint: the active channel prefix while typing, else the
    /// ported command list (`ChatFragment` has no hint; the relay dispatches
    /// `/t`, `/a`, `/help`, `/vote`, `/votekick`, `/sync`).
    fn refresh_hint(&mut self) {
        let Some(mut hint) = self.hint.clone() else {
            return;
        };
        let text = self
            .field
            .as_ref()
            .map(|field| field.get_text().to_string())
            .unwrap_or_default();
        let label = if !text.is_empty() && text.starts_with('/') {
            let prefix = self.state.mode.normalized_prefix();
            if prefix.is_empty() {
                COMMAND_HINT.to_owned()
            } else {
                format!("{prefix}…  {COMMAND_HINT}")
            }
        } else {
            COMMAND_HINT.to_owned()
        };
        hint.set_text(&GString::from(label.as_str()));
    }
}

/// `Vars.mobile` (the fragment's bigger bottom margin branch).
fn mobile() -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    ui.call("is_mobile", &[]).try_to::<bool>().unwrap_or(false)
}

/// `net.active()`: a relay session exists (`MindNet.session_state()`).
fn net_active() -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    if !ui.has_method("net_active") {
        return false;
    }
    ui.call("net_active", &[]).try_to::<bool>().unwrap_or(false)
}

/// `ui.hudfrag.shown()` (`MindUi.hud_visible`).
fn hud_visible() -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    ui.call("hud_visible", &[])
        .try_to::<bool>()
        .unwrap_or(false)
}

/// `player.admin` (`MindNet.local_admin`).
fn local_admin() -> bool {
    let Some(mut net) = kit::net() else {
        return false;
    };
    if !net.has_method("local_admin") {
        return false;
    }
    net.call("local_admin", &[])
        .try_to::<bool>()
        .unwrap_or(false)
}

/// `ui.consolefrag.shown()`: the console HUD fragment is open, so the chat key
/// must not toggle the input (`ChatFragment.java:65`).
fn console_shown() -> bool {
    kit::node("/root/Spine/Ui/UiRoot/HudGroup/console")
        .and_then(|node| node.try_cast::<Control>().ok())
        .is_some_and(|console| console.is_visible_in_tree())
}

/// `world.tiles` bounds for the ping parse.
///
/// No `#[func]` exposes the live grid size (`SimHost.world_size` is a plain
/// Rust method), so this answers the full `i32` range: `check_ping` still
/// parses the ping, and an out-of-bounds tile would only be dropped by the
/// relay ping path, which does not exist yet.
fn map_size() -> (i32, i32) {
    (i32::MAX, i32::MAX)
}

/// `Core.settings.getInt("chatopacity") / 100f` (slider default 100).
fn chat_opacity() -> f64 {
    let Some(mut ui) = kit::ui() else {
        return 1.0;
    };
    if !ui.has_method("settings_get_i32") {
        return 1.0;
    }
    let value = ui
        .call(
            "settings_get_i32",
            &[GString::from(CHAT_OPACITY_KEY).to_variant()],
        )
        .try_to::<i64>()
        .unwrap_or(CHAT_OPACITY_DEFAULT);
    (value as f64 / 100.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn line(id: u64, kind: ChatKind, team: Option<u8>, target: Option<u8>) -> ChatLine {
        ChatLine {
            chat_id: id,
            match_id: 7,
            sender: "aa11bb22cc33dd44".to_owned(),
            kind,
            team,
            text: format!("line {id}"),
            sent_at_micros: 0,
            target: target.map(|byte| format!("{byte:02x}")),
        }
    }

    fn row_json(id: u64, kind: &str, team: Option<u8>, target: Option<&str>) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "matchId": 7,
            "sender": "aa11",
            "kind": kind,
            "team": team,
            "text": format!("line {id}"),
            "sentAt": 0,
            "target": target,
            "admin": kind == "admin",
            "private": target.is_some(),
        })
    }

    #[test]
    fn kind_names_round_trip() {
        for (name, kind) in [
            ("all", ChatKind::All),
            ("team", ChatKind::Team),
            ("system", ChatKind::System),
            ("admin", ChatKind::Admin),
        ] {
            assert_eq!(kind_from_name(name), Some(kind));
            assert_eq!(mind_stdb::chat_kind_name(kind), name);
        }
        assert_eq!(kind_from_name("bogus"), None);
        assert_eq!(MODE_NAMES, ["normal", "team", "admin"]);
        assert_eq!(mode_index(ChatMode::Normal), 0);
        assert_eq!(mode_index(ChatMode::Team), 1);
        assert_eq!(mode_index(ChatMode::Admin), 2);
    }

    #[test]
    fn parses_chat_line_json() {
        let raw = row_json(3, "team", Some(2), None);
        let line = parse_chat_line(&raw).expect("valid row");
        assert_eq!(line.chat_id, 3);
        assert_eq!(line.match_id, 7);
        assert_eq!(line.kind, ChatKind::Team);
        assert_eq!(line.team, Some(2));
        assert_eq!(line.text, "line 3");
        assert!(!line.private());
        // Malformed rows are skipped rather than panicking.
        assert!(parse_chat_line(&serde_json::json!({"kind": "all"})).is_none());
        assert!(parse_chat_line(&serde_json::json!({"kind": "nope", "id": 1})).is_none());
        // A single object payload is accepted like an array of one.
        assert_eq!(parse_rows(&raw.to_string()).len(), 1);
        assert_eq!(parse_rows(&format!("[{}]", raw)).len(), 1);
        assert!(parse_rows("null").is_empty());
    }

    #[test]
    fn feed_keeps_only_what_the_viewer_can_see() {
        let mut feed = ChatFeed::new();
        feed.apply_line(line(1, ChatKind::All, None, None));
        feed.apply_line(line(2, ChatKind::Admin, None, None));
        feed.apply_line(line(3, ChatKind::Team, Some(1), None));
        assert_eq!(feed.len(), 1, "no viewer: admin/team filtered");

        feed.set_viewer(Some("aa11bb22"), Some(1), false);
        assert_eq!(feed.len(), 1);
        feed.apply_line(line(2, ChatKind::Admin, None, None));
        assert_eq!(feed.len(), 1, "admin channel needs the flag");
        feed.apply_line(line(3, ChatKind::Team, Some(1), None));
        assert_eq!(feed.len(), 2, "own team's chat is visible");
        feed.apply_line(line(4, ChatKind::Team, Some(2), None));
        assert_eq!(feed.len(), 2, "other team's chat is filtered");

        // Team switch re-filters the retained rows.
        feed.set_viewer(Some("aa11bb22"), Some(2), false);
        assert_eq!(feed.len(), 1);
        assert_eq!(feed.last_id(), Some(1));

        // Gaining admin reveals the admin channel.
        feed.set_viewer(Some("aa11bb22"), Some(2), true);
        feed.apply_line(line(2, ChatKind::Admin, None, None));
        assert_eq!(feed.len(), 2);
        assert_eq!(
            feed.lines()
                .iter()
                .map(|row| row.chat_id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    #[test]
    fn private_rows_reach_their_target_with_or_without_a_local_identity() {
        let mut feed = ChatFeed::new();
        feed.apply_line(line(1, ChatKind::System, None, Some(0x22)));
        // No identity: the view only emits target rows to their target.
        assert_eq!(feed.len(), 1);
        feed.set_viewer(Some("aa11"), None, false);
        assert_eq!(feed.len(), 0, "a known non-target drops it");
        let mut targeted = ChatFeed::new();
        targeted.set_viewer(Some("22"), None, false);
        targeted.apply_line(line(1, ChatKind::System, None, Some(0x22)));
        assert_eq!(targeted.len(), 1);
        assert!(targeted.line(1).expect("row").private());
    }

    #[test]
    fn feed_dedupes_orders_and_bounds() {
        let mut feed = ChatFeed::with_limit(3);
        feed.set_viewer(Some("aa11"), None, true);
        feed.apply_line(line(3, ChatKind::All, None, None));
        feed.apply_line(line(1, ChatKind::All, None, None));
        feed.apply_line(line(2, ChatKind::All, None, None));
        assert_eq!(
            feed.lines()
                .iter()
                .map(|row| row.chat_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        let revision = feed.revision();
        feed.apply_line(line(2, ChatKind::All, None, None));
        assert_eq!(feed.revision(), revision, "same row: no change");
        feed.apply_line(ChatLine {
            text: "edited".to_owned(),
            ..line(2, ChatKind::All, None, None)
        });
        assert_eq!(
            feed.revision(),
            revision + 1,
            "an update bumps the revision"
        );
        assert_eq!(feed.line(2).map(|row| row.text.as_str()), Some("edited"));
        feed.apply_line(line(4, ChatKind::All, None, None));
        assert_eq!(
            feed.lines()
                .iter()
                .map(|row| row.chat_id)
                .collect::<Vec<_>>(),
            vec![2, 3, 4],
            "newest beyond the limit evicts the oldest"
        );
        feed.apply_json(&format!("[{}]", row_json(5, "all", None, None)));
        assert_eq!(feed.last_id(), Some(5));
        assert_eq!(feed.to_json().as_array().map(Vec::len), Some(3));
        feed.clear();
        assert!(feed.is_empty());
    }

    #[test]
    fn markup_matches_the_java_chat_formatter() {
        let names = roster::roster_index(&roster::roster_names(
            r#"[{"identity":"aa11bb22cc33dd44","name":"Alice","team":1,"admin":false,"connected":true}]"#,
        ));
        let all = line(1, ChatKind::All, None, None);
        assert_eq!(
            format_line(&all, &names),
            "[coral][[[#ffd37f]Alice[coral]]:[white] line 1"
        );
        let team = line(2, ChatKind::Team, Some(2), None);
        assert_eq!(
            format_line(&team, &names),
            "[#f25555]<T> [coral][[[#ffd37f]Alice[coral]]:[white] line 2"
        );
        let admin = line(3, ChatKind::Admin, None, None);
        assert!(format_line(&admin, &names).starts_with("[#ff4000]<A> "));
        let system = ChatLine {
            text: "Server restarting".to_owned(),
            ..line(4, ChatKind::System, None, None)
        };
        assert_eq!(
            format_line(&system, &names),
            "[lightgray]Server restarting[]"
        );
        let mute = ChatLine {
            text: "You are muted for another 30 second(s).".to_owned(),
            ..line(5, ChatKind::System, None, Some(0x22))
        };
        assert_eq!(
            format_line(&mute, &names),
            "[scarlet]You are muted for another 30 second(s).[]"
        );
        assert!(is_denial_notice("You may not send the same message twice."));
        assert!(!is_denial_notice("hello"));
        // An unknown sender stays addressable via the identity prefix.
        let unknown = ChatLine {
            sender: "ffee0011".to_owned(),
            ..line(6, ChatKind::All, None, None)
        };
        assert!(format_line(&unknown, &names).contains("ffee0011"));
    }

    #[test]
    fn channel_prefixes_carry_team_and_admin_colours() {
        assert_eq!(channel_prefix(&line(1, ChatKind::All, None, None)), None);
        assert_eq!(
            channel_prefix(&line(2, ChatKind::Team, Some(1), None)).as_deref(),
            Some("[#ffd37f]<T> ")
        );
        assert_eq!(
            channel_prefix(&line(3, ChatKind::Admin, None, None)).as_deref(),
            Some("[#ff4000]<A> ")
        );
        // A team row without an id uses Pal.accent.
        assert_eq!(
            channel_prefix(&line(4, ChatKind::Team, None, None)).as_deref(),
            Some("[#ffd37f]<T> ")
        );
    }

    #[test]
    fn format_line_markup_survives_the_render_path() {
        // The Java markup the fragment hands to `MindAssets.markup_text` must
        // translate without dangling tags (the `[[` escape is the bracket).
        let names = std::collections::HashMap::new();
        let markup = format_line(&line(1, ChatKind::All, None, None), &names);
        let icons = mind_core::assets::icons::Iconc::default();
        let rendered = mind_core::ui::text::render_markup(&markup, &icons);
        assert!(rendered.contains("aa11bb22"), "author rendered: {rendered}");
        assert!(rendered.contains("line 1"));
        assert!(rendered.contains("[lb]"), "the `[[` escape becomes `[lb]`");
        // No literal Java colour tags survive the translation.
        assert!(!rendered.contains("[coral]"));
    }
}
