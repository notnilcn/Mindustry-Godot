// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Esc handling and the background-pause path.
//! Upstream: core/src/mindustry/core/UI.java

use super::*;

impl MindUiRoot {
    // ---- Esc / background pause -------------------------------------------

    /// `_on_escape`: loading overlay, console, minimap, top dialog, then the
    /// in-game pause menu. Returns whether the key was consumed.
    pub(super) fn on_escape(&mut self) -> bool {
        let Some(mut ui) = kit::ui() else {
            return false;
        };
        if let Some(mut loading) = self.base().get_node_or_null("LoadingLayer/loading")
            && let Ok(loading_control) = loading.clone().try_cast::<Control>()
            && loading_control.is_visible()
        {
            if loading.has_method("hide_loading") {
                let _ = loading.call("hide_loading", &[]);
            }
            return true;
        }
        if let Some(mut console) = self.base().get_node_or_null("HudGroup/console")
            && let Ok(console_control) = console.clone().try_cast::<Control>()
            && console_control.is_visible()
            && console.has_method("toggle")
        {
            let _ = console.call("toggle", &[]);
            return true;
        }
        if let Some(mut minimap) = self.hud_child("minimap")
            && minimap.is_visible()
        {
            minimap.set_visible(false);
            return true;
        }
        if ui.call("has_dialog", &[]).try_to::<bool>().unwrap_or(false) {
            let stack = ui
                .call("dialog_stack", &[])
                .try_to::<PackedStringArray>()
                .unwrap_or_default();
            let top = stack
                .as_slice()
                .last()
                .map(ToString::to_string)
                .unwrap_or_default();
            if top == "restart" || top == "campaign_complete" {
                return false;
            }
            if let Some(mut top_dialog) = self
                .base()
                .get_node_or_null(format!("DialogLayer/{top}").as_str())
                && top_dialog.has_method("blocks_escape")
                && top_dialog
                    .call("blocks_escape", &[])
                    .try_to::<bool>()
                    .unwrap_or(false)
            {
                return false;
            }
            let _ = ui.call("close_top_dialog", &[]);
            return true;
        }
        if self
            .menu_group
            .as_ref()
            .is_some_and(|group| group.is_visible())
        {
            return false;
        }
        // The editor shell owns keys while it is open (`MapEditorDialog.doInput`);
        // its Escape opens the editor menu, never the pause menu.
        if self
            .editor_dialog
            .as_ref()
            .is_some_and(|editor_dialog| editor_dialog.is_visible())
        {
            return false;
        }
        // `Control.java:733-736`: Esc pauses the sim (`state.set(paused)`) when
        // not in a network game and `pauseDisabled` is off.
        let opened = ui
            .call(
                "open_dialog",
                &[
                    GString::from("paused").to_variant(),
                    GString::new().to_variant(),
                ],
            )
            .try_to::<bool>()
            .unwrap_or(false);
        if opened
            && let Some(mut host) = kit::sim_host()
            && host.has_method("set_paused")
        {
            let _ = host.call("set_paused", &[true.to_variant()]);
        }
        opened
    }

    /// Arc's `closeOnBack` key listener still sees ESC while a text field inside
    /// it has keyboard focus. A focused Godot `LineEdit`/`TextEdit` accepts the
    /// key in `gui_input`, so drop its focus before the GUI pass and let
    /// `unhandled_key_input` close the dialog.
    pub(super) fn release_field_on_escape(&mut self, event: &Gd<InputEvent>) {
        let Ok(key) = event.clone().try_cast::<InputEventKey>() else {
            return;
        };
        if !key.is_pressed() || key.is_echo() || key.get_keycode() != Key::ESCAPE {
            return;
        }
        let Some(viewport) = self.base().get_viewport() else {
            return;
        };
        let Some(mut focus) = viewport.gui_get_focus_owner() else {
            return;
        };
        if focus.clone().try_cast::<LineEdit>().is_ok()
            || focus.clone().try_cast::<TextEdit>().is_ok()
        {
            focus.release_focus();
        }
    }

    /// `settings.getBool("backgroundpause", true) && !net.active() &&
    /// !state.rules.pauseDisabled`.
    pub(super) fn background_pause_allowed(&self) -> bool {
        let Some(mut ui) = kit::ui() else {
            return false;
        };
        if !ui
            .call(
                "settings_get_bool",
                &[GString::from("backgroundpause").to_variant()],
            )
            .try_to::<bool>()
            .unwrap_or(false)
        {
            return false;
        }
        if let Some(mut net) = kit::net()
            && net.has_method("session_state")
            && net
                .call("session_state", &[])
                .try_to::<GString>()
                .unwrap_or_default()
                != "offline"
        {
            return false;
        }
        let Some(mut host) = kit::sim_host() else {
            return false;
        };
        if host.has_method("rules_pause_disabled")
            && host
                .call("rules_pause_disabled", &[])
                .try_to::<bool>()
                .unwrap_or(false)
        {
            return false;
        }
        true
    }

    /// `if(state.is(State.playing)) state.set(State.paused)`: only an in-game
    /// session (the menu has no running game) pauses on focus loss.
    pub(super) fn background_pause(&mut self) {
        if self
            .menu_group
            .as_ref()
            .map(|group| group.is_visible())
            .unwrap_or(true)
            || !self.background_pause_allowed()
        {
            return;
        }
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        self.background_paused = true;
        self.background_was_paused = host
            .call("is_paused", &[])
            .try_to::<bool>()
            .unwrap_or(false);
        if !self.background_was_paused {
            let _ = host.call("set_paused", &[true.to_variant()]);
        }
    }

    pub(super) fn background_resume(&mut self) {
        if !self.background_paused {
            return;
        }
        self.background_paused = false;
        if self.background_was_paused || !self.background_pause_allowed() {
            return;
        }
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if host
            .call("is_paused", &[])
            .try_to::<bool>()
            .unwrap_or(false)
        {
            let _ = host.call("set_paused", &[false.to_variant()]);
        }
    }
}
