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

use godot::classes::notify::NodeNotification;
use godot::classes::{INode, Node as GdNode, Os};
use godot::obj::{Base, Singleton};
use godot::prelude::*;
use mind_core::io::{FileSystem, NativeFs, Paths, SettingsStore};
use mind_core::ui::settings::{self, SettingKind};

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
    /// Lazily built placement-palette catalog JSON (`PlacementFragment`).
    block_catalog_cache: Option<String>,
    /// Rust console command registry (plan 14 M7, deviation OD1).
    console: mind_core::ui::console::ConsoleRegistry,
    /// Plan-04 settings KV store behind the settings dialog.
    settings: SettingsStore,
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
            block_catalog_cache: None,
            console: mind_core::ui::console::ConsoleRegistry::with_defaults(),
            settings: SettingsStore::new(),
            pause_flags: HashMap::new(),
            pause_depth: 0,
            was_paused: false,
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; re-run the boot logging.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
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
        // Re-opening an already-stacked dialog must not double-count its pause.
        let was_open = self.stack.iter().any(|entry| entry == &key);
        self.stack.retain(|entry| entry != &key);
        self.stack.push(key);
        if should_pause && !was_open {
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
        let was_top = self.stack.last().is_some_and(|entry| entry == &key);
        let before = self.stack.len();
        self.stack.retain(|entry| entry != &key);
        if self.stack.len() != before {
            if self.pause_flags.get(&key).copied().unwrap_or(false) {
                self.pop_pause();
            }
            if was_top {
                self.reveal_top();
            }
            let _ = self.base_mut().emit_signal("dialog_stack_changed", &[]);
            return true;
        }
        false
    }

    /// Re-shows the current top of the stack after a close (`BaseDialog`
    /// restoration; the parent was hidden when the child opened).
    fn reveal_top(&mut self) {
        let Some(top) = self.stack.last().cloned() else {
            return;
        };
        if let Some(mut node) = self.dialogs.get(&top).cloned()
            && node.has_method("show_dialog")
        {
            let _ = node.call("show_dialog", &[]);
        }
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
        self.reveal_top();
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
            let mut views = mind_core::ui::campaign::CampaignViews::vanilla_fixture();
            // The fixture carries no map registry; feed the built-in map rows so
            // `CustomGameDialog`/`EditorMapsDialog` render the default map grid.
            views.maps = mind_core::ui::campaign::default_map_entries();
            let json = serde_json::to_string(&views).unwrap_or_else(|_| String::from("{}"));
            self.campaign_cache = Some(json);
        }
        GString::from(self.campaign_cache.as_deref().unwrap_or("{}"))
    }

    /// Placement-palette catalog JSON (`PlacementFragment` block list): the
    /// non-empty `Category` groups with their buildable blocks and
    /// `database-tag.*` label keys. Cached after the first build.
    #[func]
    pub fn block_catalog_json(&mut self) -> GString {
        if self.block_catalog_cache.is_none() {
            let catalog = mind_core::ui::campaign::block_catalog();
            let json = serde_json::to_string(&catalog).unwrap_or_else(|_| String::from("{}"));
            self.block_catalog_cache = Some(json);
        }
        GString::from(self.block_catalog_cache.as_deref().unwrap_or("{}"))
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

    /// Settings category rail JSON (`SettingsMenuDialog.rebuildMenu` order).
    #[func]
    pub fn settings_categories_json(&self) -> GString {
        let categories: Vec<serde_json::Value> = settings::CATEGORIES
            .iter()
            .map(|category| {
                serde_json::json!({
                    "id": category.id,
                    "key": category.key,
                    "icon": category.icon,
                    "action": match category.action {
                        settings::SettingsAction::Table => "table",
                        settings::SettingsAction::Language => "language",
                        settings::SettingsAction::Controls => "controls",
                        settings::SettingsAction::Data => "data",
                    },
                })
            })
            .collect();
        json_string(&serde_json::Value::Array(categories))
    }

    /// Settings table rows JSON with current store values (`None` table is `[]`).
    #[func]
    pub fn settings_rows_json(&self, category: GString) -> GString {
        let Some(rows) = settings::table_rows(&category.to_string()) else {
            return GString::from("[]");
        };
        let values: Vec<serde_json::Value> = rows.iter().map(|row| self.row_json(row)).collect();
        json_string(&serde_json::Value::Array(values))
    }

    /// Writes one known setting key through the plan-04 store and persists it.
    #[func]
    pub fn settings_set(&mut self, key: GString, value: Variant) -> bool {
        let key = key.to_string();
        let Some(row) = settings::all_rows().find(|row| row.key == key) else {
            log::warn!("[ui] settings_set: unknown key '{key}'");
            return false;
        };
        match row.kind {
            SettingKind::Check => {
                let Ok(enabled) = value.try_to::<bool>() else {
                    return false;
                };
                self.settings.put_bool(&key, enabled);
            }
            SettingKind::Slider { min, max, .. } => {
                let Ok(number) = value.try_to::<i32>() else {
                    return false;
                };
                self.settings.put_i32(&key, number.clamp(min, max));
            }
        }
        self.persist_settings();
        true
    }

    /// Removes every key of one table, restoring the model defaults.
    #[func]
    pub fn settings_reset(&mut self, category: GString) -> bool {
        let Some(rows) = settings::table_rows(&category.to_string()) else {
            return false;
        };
        for row in rows {
            self.settings.remove(row.key);
        }
        self.persist_settings();
        true
    }

    /// `Control.java:640` `uiscalechanged`: whether the UI scale was changed
    /// since the last confirmed boot.
    #[func]
    pub fn uiscale_changed(&self) -> bool {
        self.settings.get_bool("uiscalechanged", false)
    }

    /// Sets the `uiscalechanged` boot-confirmation flag (`Control.java`).
    #[func]
    pub fn set_uiscale_changed(&mut self, changed: bool) -> bool {
        self.settings.put_bool("uiscalechanged", changed);
        self.persist_settings();
        true
    }

    /// Runs a data-category action (`clear-saves`, `open-folder`).
    ///
    /// Actions whose backing subsystem is not ported are not exposed by the
    /// model and return false here rather than faking success.
    #[func]
    pub fn settings_action(&mut self, action: GString) -> bool {
        match action.to_string().as_str() {
            "clear-saves" => {
                let paths = Paths::resolve(None);
                let fs = NativeFs;
                let mut removed = 0usize;
                for dir in [paths.saves(), paths.previews()] {
                    let Ok(files) = fs.walk(&dir) else {
                        continue;
                    };
                    for file in files {
                        if fs.delete(&file).is_ok() {
                            removed += 1;
                        }
                    }
                }
                log::info!("[ui] settings_action clear-saves removed {removed} file(s)");
                true
            }
            "open-folder" => {
                let root = Paths::resolve(None).root().display().to_string();
                let error = Os::singleton().shell_open(&GString::from(root.as_str()));
                error == godot::global::Error::OK
            }
            other => {
                log::info!("[ui] settings_action '{other}' is not ported");
                false
            }
        }
    }
}

impl MindUi {
    /// Re-runs boot logging (runs from `ready()` and on
    /// `EXTENSION_RELOADED`, which does not re-run `ready()`).
    fn bootstrap(&mut self) {
        log::info!(
            "MindUi ready (mobile={}, mobile_preview={})",
            self.mobile,
            self.mobile_preview
        );
        self.settings = SettingsStore::load(&NativeFs, &Paths::resolve(None));
    }

    /// One settings row as JSON (current store value + model default/bounds).
    fn row_json(&self, row: &settings::SettingRow) -> serde_json::Value {
        match row.kind {
            SettingKind::Check => serde_json::json!({
                "key": row.key,
                "kind": "check",
                "value": self.settings.get_bool(row.key, row.default_bool()),
                "default": row.default_bool(),
            }),
            SettingKind::Slider { min, max, step } => serde_json::json!({
                "key": row.key,
                "kind": "slider",
                "value": self.settings.get_i32(row.key, row.default_i32()),
                "default": row.default_i32(),
                "min": min,
                "max": max,
                "step": step,
                "format": format_tag(row.format),
            }),
        }
    }

    /// Atomically writes the settings store (writes are small; no debounce pump
    /// exists in this node).
    fn persist_settings(&mut self) {
        if let Err(error) = self.settings.force_save(&NativeFs, &Paths::resolve(None)) {
            log::warn!("[ui] settings save failed: {error}");
        }
    }

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

/// Serializes a JSON value, falling back to an empty container on failure.
fn json_string(value: &serde_json::Value) -> GString {
    match serde_json::to_string(value) {
        Ok(json) => GString::from(json.as_str()),
        Err(error) => {
            log::warn!("[ui] settings JSON encode failed: {error}");
            GString::from("[]")
        }
    }
}

/// The widget-layer format tag for a slider row.
fn format_tag(format: settings::SettingFormat) -> &'static str {
    match format {
        settings::SettingFormat::Plain => "plain",
        settings::SettingFormat::Percent => "percent",
        settings::SettingFormat::Seconds => "seconds",
        settings::SettingFormat::Multiplier => "multiplier",
        settings::SettingFormat::X => "x",
        settings::SettingFormat::BloomPercent => "bloomPercent",
        settings::SettingFormat::Pixels => "pixels",
        settings::SettingFormat::FpsCap => "fpsCap",
    }
}
