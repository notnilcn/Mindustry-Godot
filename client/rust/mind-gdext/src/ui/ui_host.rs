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
}

#[godot_api]
impl INode for MindUi {
    fn init(base: Base<GdNode>) -> Self {
        Self {
            base,
            dialogs: HashMap::new(),
            stack: Vec::new(),
            margin_left: 0,
            margin_right: 0,
            margin_top: 0,
            margin_bottom: 0,
            hud_visible: true,
            mobile: Os::singleton().has_feature("mobile"),
        }
    }

    fn ready(&mut self) {
        log::info!("MindUi ready (mobile={})", self.mobile);
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

    /// Registers a dialog node under a manifest name (`UiRoot` boot).
    #[func]
    pub fn register_dialog(&mut self, name: GString, node: Gd<GdNode>) {
        self.dialogs.insert(name.to_string(), node);
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
        self.stack.retain(|entry| entry != &key);
        self.stack.push(key);
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
}
