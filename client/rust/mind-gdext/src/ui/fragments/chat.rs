// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/ChatFragment.java (plan 14 M7).
//
// In-game chat: modes (normal/team/admin), history and message rendering. The
// message text is validated/forwarded through `MindUi.chat_send` (transport is
// plan 21's relay; the shell never fakes a server round trip). The rolling
// buffer + fade timing mirror the Rust `ui::chat` model, which is unit-tested.

//! In-game chat fragment (`ChatFragment`).

use godot::builtin::Side;
use godot::classes::control::LayoutPreset;
use godot::classes::{
    Button, Control, HBoxContainer, IControl, LineEdit, RichTextLabel, VBoxContainer,
};
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::kit;
use crate::ui::kit::margins;
use crate::ui::kit::widgets::factory;

/// `ChatFragment.messagesShown`.
const MESSAGES_SHOWN: usize = 10;
/// `ChatMode` names in declaration order (the mode-cycle ABI).
const MODES: [&str; 3] = ["normal", "team", "admin"];
/// `ChatMode.prefix` per mode.
const PREFIXES: [&str; 3] = ["", "/t", "/a"];
/// `Core.settings.getInt("chatopacity")` slider default.
const CHAT_OPACITY_DEFAULT: i64 = 100;

/// `MindChatFragment` — the HUD chat overlay.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindChatFragment {
    base: Base<Control>,
    messages: Vec<String>,
    mode: usize,
    fade: f64,
    shown: bool,
    panel: Option<Gd<VBoxContainer>>,
    field: Option<Gd<LineEdit>>,
    log: Option<Gd<RichTextLabel>>,
    mode_button: Option<Gd<Button>>,
}

#[godot_api]
impl IControl for MindChatFragment {
    fn ready(&mut self) {
        self.base_mut()
            .set_anchors_preset(LayoutPreset::BOTTOM_LEFT);
        self.build();
        self.base_mut().set_visible(false);
        self.base_mut().set_process(true);
    }

    fn process(&mut self, delta: f64) {
        if self.fade > 0.0 && !self.shown {
            self.fade = 0.0_f64.max(self.fade - delta / 3.0);
        }
        // `ChatFragment.draw` (`ChatFragment.java:149-187`): message text and
        // shadow alpha scale by `chatopacity / 100` (full while the field is
        // shown, the fade factor otherwise).
        let fade = if self.shown {
            1.0
        } else {
            self.fade.clamp(0.0, 1.0)
        };
        let Some(mut log) = self.log.clone() else {
            return;
        };
        let mut modulate = log.get_modulate();
        modulate.a = (fade * chat_opacity()) as f32;
        log.set_modulate(modulate);
    }
}

#[godot_api]
impl MindChatFragment {
    /// Adds a message to the rolling buffer (called by the HUD/relay).
    #[func]
    pub fn add_message(&mut self, message: GString) {
        if message.is_empty() {
            return;
        }
        let message = message.to_string();
        self.messages.insert(0, message);
        if self.messages.len() > MESSAGES_SHOWN {
            self.messages.pop();
        }
        self.fade = (self.fade + 1.0).min(MESSAGES_SHOWN as f64) + 1.0;
        self.refresh_log();
        if !self.shown {
            self.fade = self.fade.max(1.0);
        }
    }

    /// Shows/hides the field and focuses it (`ChatFragment` toggle).
    #[func]
    pub fn toggle(&mut self) {
        self.shown = !self.shown;
        let shown = self.shown;
        self.base_mut().set_visible(shown);
        if let Some(mut field) = self.field.clone() {
            if shown {
                field.grab_focus();
            } else {
                field.release_focus();
            }
        }
    }

    /// Whether the chat field is open.
    #[func]
    pub fn is_open(&self) -> bool {
        self.shown
    }

    /// Cycles the chat mode (normal -> team; the admin entry is never reached).
    #[func]
    fn next_mode(&mut self) {
        let previous = PREFIXES[self.mode];
        self.mode = next_mode_index(self.mode);
        let next_prefix = PREFIXES[self.mode];
        if let Some(mut field) = self.field.clone() {
            let text = field.get_text().to_string();
            if let Some(rest) = text.strip_prefix(previous) {
                field.set_text(&GString::from(format!("{next_prefix}{rest}").as_str()));
            }
        }
        self.refresh_mode();
    }

    /// `LineEdit.text_submitted`: forwards the field text to the send path.
    #[func]
    fn submit(&mut self, _text: GString) {
        self.request_send();
    }
}

impl MindChatFragment {
    fn build(&mut self) {
        // code-instantiated: chat is a persistent HUD fragment whose
        // rows/messages are runtime data, so the layout is built once here
        // (plan 14 §3.10).
        let mut panel = VBoxContainer::new_alloc();
        panel.set_anchor(Side::LEFT, 0.0);
        panel.set_anchor(Side::TOP, 1.0);
        panel.set_anchor(Side::BOTTOM, 1.0);
        panel.set_offset(Side::BOTTOM, -8.0);
        panel.set_offset(Side::LEFT, margins::left());
        panel.set_offset(Side::RIGHT, 720.0);
        self.base_mut().add_child(&panel);

        let mut log = RichTextLabel::new_alloc();
        log.set_use_bbcode(true);
        log.set_fit_content(true);
        log.set_scroll_active(false);
        panel.add_child(&log);

        let mut row = HBoxContainer::new_alloc();
        let mut mode_button = factory::button("");
        let callable = self.base().callable("next_mode");
        mode_button.connect("pressed", &callable);
        row.add_child(&mode_button);

        let mut field = factory::field(&kit::t("@chat.message").to_string());
        let callable = self.base().callable("submit");
        field.connect("text_submitted", &callable);
        row.add_child(&field);
        panel.add_child(&row);

        if mobile() {
            panel.set_offset(Side::BOTTOM, -105.0);
            panel.set_offset(Side::RIGHT, 480.0);
        }

        self.panel = Some(panel);
        self.log = Some(log);
        self.mode_button = Some(mode_button);
        self.field = Some(field);
        self.refresh_mode();
        self.refresh_log();
    }

    /// `actionButton`: the prefix glyph for normal, else the mode name.
    fn refresh_mode(&mut self) {
        let text = if PREFIXES[self.mode].is_empty() {
            "@"
        } else {
            MODES[self.mode]
        };
        if let Some(mut button) = self.mode_button.clone() {
            button.set_text(text);
        }
    }

    fn refresh_log(&mut self) {
        let text = GString::from(self.messages.join("\n").as_str());
        if let Some(mut log) = self.log.clone() {
            log.set_text(&text);
        }
    }

    /// Prefixes the field text with the mode command and hands it to `MindUi`.
    fn request_send(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("chat_send") {
            return;
        }
        let Some(mut field) = self.field.clone() else {
            return;
        };
        let text = format!("{}{}", PREFIXES[self.mode], field.get_text());
        let _ = ui.call(
            "chat_send",
            &[
                GString::from(text.as_str()).to_variant(),
                GString::from(MODES[self.mode]).to_variant(),
            ],
        );
        field.set_text("");
    }
}

/// `ChatFragment` cycle button: advance one mode, skipping `admin`.
///
/// The GDScript guard (`admin`) was a constant `false` (the client has no
/// admin flag), so the cycle stays `normal <-> team`; the skip is kept 1:1.
fn next_mode_index(mode: usize) -> usize {
    let mut index = (mode + 1) % MODES.len();
    while MODES[index] == "admin" {
        index = (index + 1) % MODES.len();
    }
    index
}

/// The mobile HUD layout branch (`MindUi.is_mobile`).
fn mobile() -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    ui.call("is_mobile", &[]).try_to::<bool>().unwrap_or(false)
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
            &[GString::from("chatopacity").to_variant()],
        )
        .try_to::<i64>()
        .unwrap_or(CHAT_OPACITY_DEFAULT);
    (value as f64 / 100.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_mode_is_skipped_by_the_cycle_button() {
        // normal -> team -> normal: `admin` is never selectable (the GDScript
        // guard was a constant false).
        assert_eq!(next_mode_index(0), 1);
        assert_eq!(next_mode_index(1), 0);
        assert_eq!(next_mode_index(2), 0);
    }

    #[test]
    fn mode_table_matches_upstream_order() {
        assert_eq!(MODES, ["normal", "team", "admin"]);
        assert_eq!(PREFIXES, ["", "/t", "/a"]);
    }
}
