// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindUi` autoload (plan 14 §3.2): the dialog registry, pause governor and
//! prompt/toast facade.
//!
//! This is the Rust half of `mindustry.core.UI`. It owns the dialog registry
//! (keyed by the manifest name), the active-dialog stack, safe-area margins and
//! the semantic prompt/announce/toast signals the GDScript `UiRoot` renders.
//! Every dialog node is eagerly instantiated by `UiRoot` from
//! `client/ui/dialogs_manifest.json` and registered through `register_dialog`.
//!
//! The modal contents stay in GDScript (layout only, HLP §6.6); this node never
//! reads sim state and never mutates it. In-engine MCP verification is deferred
//! to the orchestrator's single-editor mutex.

use std::collections::HashMap;

use godot::classes::{INode, Node as GdNode, Os};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

/// Path to the sim owner that the pause governor drives.
const SIM_HOST_PATH: &str = "/root/Spine/SimHost";

/// `MindUi` — the UI registry/prompt singleton (`/root/MindUi`).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindUi {
    base: Base<GdNode>,
    dialogs: HashMap<String, Gd<GdNode>>,
    stack: Vec<String>,
    margin_left: i32,
    margin_right: i32,
    margin_top: i32,
    margin_bottom: i32,
    hud_visible: bool,
    mobile: bool,
    /// `--mobile-preview` override flag (plan 14 §2.4 item 10 / plan 22).
    mobile_preview: bool,
    /// Lazily built M5 campaign read-model JSON (plan 14 §3.11 12 seam).
    campaign_cache: Option<String>,
    /// Rust console command registry (plan 14 M7, deviation OD1).
    console: mind_core::ui::console::ConsoleRegistry,
    /// Manifest `pause` flags keyed by dialog name (plan 14 §3.4).
    pause_flags: HashMap<String, bool>,
    /// Reference count of currently-open pause dialogs.
    pause_depth: u32,
    /// Whether the game was already paused before the governor took over.
    was_paused: bool,
}

#[godot_api]
impl INode for MindUi {
    fn init(base: Base<GdNode>) -> Self {
        let mobile_preview = Os::singleton()
            .get_cmdline_args()
            .as_slice()
            .iter()
            .any(|arg| *arg == "--mobile-preview");
        Self {
            base,
            dialogs: HashMap::new(),
            stack: Vec::new(),
            margin_left: 0,
            margin_right: 0,
            margin_top: 0,
            margin_bottom: 0,
            hud_visible: true,
            mobile: Os::singleton().has_feature("mobile") || mobile_preview,
            mobile_preview,
            campaign_cache: None,
            console: mind_core::ui::console::ConsoleRegistry::with_defaults(),
            pause_flags: HashMap::new(),
            pause_depth: 0,
            was_paused: false,
        }
    }

    fn ready(&mut self) {
        log::info!(
            "MindUi ready (mobile={}, mobile_preview={})",
            self.mobile,
            self.mobile_preview
        );
    }
}

#[godot_api]
impl MindUi {
    /// Emitted when a prompt should be rendered as a transient info box.
    #[signal]
    fn show_info(text: GString);

    /// Emitted when a fade info should be rendered.
    #[signal]
    fn show_info_fade(text: GString);

    /// Emitted for a server/client announcement (overlay).
    #[signal]
    fn announce(text: GString, duration: f32);

    /// Emitted for a toast (text + icon region name).
    #[signal]
    fn toast(text: GString, icon: GString);

    /// Emitted when a named UI sound should play (plan 18 consumes).
    #[signal]
    fn play_ui_sound(name: GString);

    /// Emitted when the active-dialog stack changes.
    #[signal]
    fn dialog_stack_changed();

    /// Emitted for a text dialog prompt (`showText`).
    #[signal]
    fn show_text(title: GString, text: GString);

    /// Emitted for a confirm prompt (`showConfirm`).
    #[signal]
    fn show_confirm(text: GString);

    /// Emitted with the confirm prompt result.
    #[signal]
    fn confirm_result(confirmed: bool);

    /// Emitted when a text-input prompt should be shown (`showTextInput`).
    #[signal]
    fn text_input_request(
        title: GString,
        message: GString,
        max_length: i64,
        default_text: GString,
        numeric: bool,
        allow_empty: bool,
    );

    /// Emitted with the text-input result (empty string = cancelled).
    #[signal]
    fn text_input_result(text: GString);

    /// Emitted when the chat fragment wants to send a validated message
    /// (transport is plan 21's relay; `mode` is `normal`/`team`/`admin`).
    #[signal]
    fn chat_message(text: GString, mode: GString);

    /// Emitted when a player-list row requests a relay action (plan 21).
    #[signal]
    fn player_action(player: GString, action: GString);

    /// Registers a dialog node under a manifest name (`UiRoot` boot) and records
    /// its manifest `pause` flag for the governor.
    #[func]
    pub fn register_dialog(&mut self, name: GString, node: Gd<GdNode>, should_pause: bool) {
        let key = name.to_string();
        self.pause_flags.insert(key.clone(), should_pause);
        self.dialogs.insert(key, node);
    }

    /// Opens a registered dialog, hiding any currently active one first.
    #[func]
    pub fn open_dialog(&mut self, name: GString, ctx_json: GString) -> bool {
        let key = name.to_string();
        let Some(mut node) = self.dialogs.get(&key).cloned() else {
            log::warn!("[ui] open_dialog: unknown dialog '{key}'");
            return false;
        };
        if let Some(top) = self.stack.last().cloned()
            && top != key
            && let Some(mut previous) = self.dialogs.get(&top).cloned()
            && previous.has_method("hide_dialog")
        {
            let _ = previous.call("hide_dialog", &[]);
        }
        if node.has_method("set_context_json") && !ctx_json.is_empty() {
            let _ = node.call("set_context_json", &[ctx_json.to_variant()]);
        }
        let _ = node.call("show_dialog", &[]);
        let should_pause = self.pause_flags.get(&key).copied().unwrap_or(false);
        self.stack.retain(|entry| entry != &key);
        self.stack.push(key);
        if should_pause {
            self.push_pause();
        }
        let _ = self.base_mut().emit_signal("dialog_stack_changed", &[]);
        true
    }

    /// Closes a registered dialog if it is open.
    #[func]
    pub fn close_dialog(&mut self, name: GString) -> bool {
        let key = name.to_string();
        if let Some(mut node) = self.dialogs.get(&key).cloned()
            && node.has_method("hide_dialog")
        {
            let _ = node.call("hide_dialog", &[]);
        }
        let before = self.stack.len();
        self.stack.retain(|entry| entry != &key);
        if self.stack.len() != before {
            if self.pause_flags.get(&key).copied().unwrap_or(false) {
                self.pop_pause();
            }
            let _ = self.base_mut().emit_signal("dialog_stack_changed", &[]);
            return true;
        }
        false
    }

    /// Closes the active (top) dialog — the Android back-button path.
    #[func]
    pub fn close_top_dialog(&mut self) -> bool {
        let Some(key) = self.stack.pop() else {
            return false;
        };
        if let Some(mut node) = self.dialogs.get(&key).cloned()
            && node.has_method("hide_dialog")
        {
            let _ = node.call("hide_dialog", &[]);
        }
        if self.pause_flags.get(&key).copied().unwrap_or(false) {
            self.pop_pause();
        }
        let _ = self.base_mut().emit_signal("dialog_stack_changed", &[]);
        true
    }

    /// Whether a dialog is currently visible.
    #[func]
    pub fn is_dialog_shown(&self, name: GString) -> bool {
        let key = name.to_string();
        let Some(mut node) = self.dialogs.get(&key).cloned() else {
            return false;
        };
        if node.has_method("is_shown") {
            return node.call("is_shown", &[]).try_to::<bool>().unwrap_or(false);
        }
        self.stack.iter().any(|entry| entry == &key)
    }

    /// Active dialog names, bottom-to-top.
    #[func]
    pub fn dialog_stack(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for name in &self.stack {
            out.push(&GString::from(name.as_str()));
        }
        out
    }

    /// Sets safe-area margins in UI pixels (`UI.updateMargins`).
    #[func]
    pub fn set_margins(&mut self, left: i32, right: i32, top: i32, bottom: i32) {
        self.margin_left = left;
        self.margin_right = right;
        self.margin_top = top;
        self.margin_bottom = bottom;
    }

    /// Left inset in UI pixels.
    #[func]
    pub fn margin_left(&self) -> i32 {
        self.margin_left
    }

    /// Right inset in UI pixels.
    #[func]
    pub fn margin_right(&self) -> i32 {
        self.margin_right
    }

    /// Top inset in UI pixels.
    #[func]
    pub fn margin_top(&self) -> i32 {
        self.margin_top
    }

    /// Bottom inset in UI pixels.
    #[func]
    pub fn margin_bottom(&self) -> i32 {
        self.margin_bottom
    }

    /// Shows a transient info prompt.
    #[func]
    pub fn show_info(&mut self, text: GString) {
        let _ = self
            .base_mut()
            .emit_signal("show_info", &[text.to_variant()]);
    }

    /// Shows an in-game overlay announcement.
    #[func]
    pub fn announce(&mut self, text: GString, duration: f32) {
        let _ = self
            .base_mut()
            .emit_signal("announce", &[text.to_variant(), duration.to_variant()]);
    }

    /// Shows a toast with an icon region.
    #[func]
    pub fn toast(&mut self, text: GString, icon: GString) {
        let _ = self
            .base_mut()
            .emit_signal("toast", &[text.to_variant(), icon.to_variant()]);
    }

    /// Requests a named UI sound (plan 18).
    #[func]
    pub fn play_ui_sound(&mut self, name: GString) {
        let _ = self
            .base_mut()
            .emit_signal("play_ui_sound", &[name.to_variant()]);
    }

    /// Shows a text popup (`UI.showText`).
    #[func]
    pub fn show_text(&mut self, title: GString, text: GString) {
        let _ = self
            .base_mut()
            .emit_signal("show_text", &[title.to_variant(), text.to_variant()]);
    }

    /// Shows a confirm prompt (`UI.showConfirm`); completes via [`Self::resolve_confirm`].
    #[func]
    pub fn show_confirm(&mut self, text: GString) {
        let _ = self
            .base_mut()
            .emit_signal("show_confirm", &[text.to_variant()]);
    }

    /// Delivers a confirm-prompt answer.
    #[func]
    pub fn resolve_confirm(&mut self, confirmed: bool) {
        let _ = self
            .base_mut()
            .emit_signal("confirm_result", &[confirmed.to_variant()]);
    }

    /// Shows a text-input prompt (`UI.showTextInput`); completes via
    /// [`Self::resolve_text_input`] (empty string = cancelled).
    #[func]
    pub fn show_text_input(
        &mut self,
        title: GString,
        message: GString,
        max_length: i64,
        default_text: GString,
        numeric: bool,
        allow_empty: bool,
    ) {
        let _ = self.base_mut().emit_signal(
            "text_input_request",
            &[
                title.to_variant(),
                message.to_variant(),
                max_length.to_variant(),
                default_text.to_variant(),
                numeric.to_variant(),
                allow_empty.to_variant(),
            ],
        );
    }

    /// Delivers a text-input answer (`""` = cancelled).
    #[func]
    pub fn resolve_text_input(&mut self, text: GString) {
        let _ = self
            .base_mut()
            .emit_signal("text_input_result", &[text.to_variant()]);
    }

    /// Shows/hides the HUD group.
    #[func]
    pub fn hud_set_visible(&mut self, visible: bool) {
        self.hud_visible = visible;
    }

    /// Whether the HUD group is enabled.
    #[func]
    pub fn hud_visible(&self) -> bool {
        self.hud_visible
    }

    /// Whether any dialog is open.
    #[func]
    pub fn has_dialog(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Whether a text field currently has keyboard focus.
    #[func]
    pub fn has_field(&self) -> bool {
        false
    }

    /// Whether the virtual keyboard is open.
    #[func]
    pub fn has_keyboard(&self) -> bool {
        self.mobile && self.has_dialog()
    }

    /// Whether UI input is locked (a dialog or keyboard is up).
    #[func]
    pub fn locked(&self) -> bool {
        self.has_dialog() || self.has_keyboard()
    }

    /// Platform mobile flag (`Vars.mobile`, overridable by plan 22).
    #[func]
    pub fn is_mobile(&self) -> bool {
        self.mobile
    }

    /// Whether the `--mobile-preview` override is active (MCP/tests).
    #[func]
    pub fn is_mobile_preview(&self) -> bool {
        self.mobile_preview
    }

    /// Overrides the mobile flag at runtime (MCP mobile-preview step, plan 22).
    #[func]
    pub fn set_mobile_preview(&mut self, enabled: bool) {
        self.mobile_preview = enabled;
        self.mobile = enabled || Os::singleton().has_feature("mobile");
    }

    /// M5 campaign dialog read models as JSON (`CampaignViews`).
    ///
    /// Reads the deterministic plan-12 fixture snapshot; the live campaign
    /// binding is the documented plan-21/world seam (plan 14 §3.11). Cached
    /// after the first call.
    #[func]
    pub fn campaign_views(&mut self) -> GString {
        if self.campaign_cache.is_none() {
            let views = mind_core::ui::campaign::CampaignViews::vanilla_fixture();
            let json = serde_json::to_string(&views).unwrap_or_else(|_| String::from("{}"));
            self.campaign_cache = Some(json);
        }
        GString::from(self.campaign_cache.as_deref().unwrap_or("{}"))
    }

    /// Validates and forwards a chat message; returns false when the fragment's
    /// guard rejected it. Transport is plan 21 (no server behaviour here).
    #[func]
    pub fn chat_send(&mut self, text: GString, mode: GString) -> bool {
        let message = text.to_string();
        if message.trim().is_empty() {
            return false;
        }
        let _ = self
            .base_mut()
            .emit_signal("chat_message", &[message.to_variant(), mode.to_variant()]);
        true
    }

    /// Documents the player-list relay seam; always false until plan 21 lands
    /// (the shell never fakes a server action).
    #[func]
    pub fn player_action(&mut self, player: GString, action: GString) -> bool {
        log::info!(
            "[ui] player_action {} on {} gated on plan 21 relay",
            action,
            player
        );
        false
    }

    /// Player-list rows JSON. The shell reads rows from this endpoint; until
    /// plan 21 supplies live views it returns an empty list (never faked).
    #[func]
    pub fn player_list_json(&self) -> GString {
        GString::from("[]")
    }

    /// Executes a console line through the Rust command registry (OD1).
    #[func]
    pub fn console_execute(&self, line: GString) -> GString {
        let output = self.console.execute(&line.to_string()).output;
        GString::from(output.as_str())
    }

    /// Registered console command names (parity/test surface).
    #[func]
    pub fn console_commands(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for name in self.console.names() {
            out.push(&GString::from(name));
        }
        out
    }
}

impl MindUi {
    /// Resolves the sim owner the governor drives, if present.
    fn sim_host(&self) -> Option<Gd<GdNode>> {
        self.base().get_node_or_null(SIM_HOST_PATH)
    }

    /// Whether the sim is currently paused (false when no host is up).
    fn sim_paused(&self) -> bool {
        let Some(mut host) = self.sim_host() else {
            return false;
        };
        host.call("is_paused", &[])
            .try_to::<bool>()
            .unwrap_or(false)
    }

    /// Drives `SimHost.set_paused` (the only place the game is paused).
    fn set_sim_paused(&self, paused: bool) {
        let Some(mut host) = self.sim_host() else {
            return;
        };
        let _ = host.call("set_paused", &[paused.to_variant()]);
    }

    /// Reference-counted pause acquire (records `wasPaused` on the first entry).
    fn push_pause(&mut self) {
        if self.pause_depth == 0 {
            self.was_paused = self.sim_paused();
            if !self.was_paused {
                self.set_sim_paused(true);
            }
        }
        self.pause_depth += 1;
    }

    /// Reference-counted pause release (restores only if we paused it).
    fn pop_pause(&mut self) {
        self.pause_depth = self.pause_depth.saturating_sub(1);
        if self.pause_depth == 0 && !self.was_paused {
            self.set_sim_paused(false);
        }
    }
}
