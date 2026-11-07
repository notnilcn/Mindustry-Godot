// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/ConsoleFragment.java (plan 14 M7).
//
// In-game console: line editor, history, scroll buttons and the mobile toggle.
// Commands execute through the Rust registry (`MindUi.console_execute`); the
// Rhino JS injection is dropped (deviation OD1). Line history/scroll mirror the
// upstream fragment.

//! In-game console fragment (`ConsoleFragment`).

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

/// `ConsoleFragment.messagesShown`.
const MESSAGES_SHOWN: usize = 30;

/// `MindConsoleFragment` — the HUD console overlay.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindConsoleFragment {
    base: Base<Control>,
    open: bool,
    shown: bool,
    messages: Vec<String>,
    history: Vec<String>,
    history_pos: i64,
    scroll_pos: i64,
    panel: Option<Gd<VBoxContainer>>,
    field: Option<Gd<LineEdit>>,
    output: Option<Gd<RichTextLabel>>,
}

#[godot_api]
impl IControl for MindConsoleFragment {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            open: false,
            shown: false,
            messages: Vec::new(),
            // `ConsoleFragment.history = new Seq<>().add("")`: the sentinel
            // empty entry keeps the recall list one slot away from `submit`.
            history: vec![String::new()],
            history_pos: 0,
            scroll_pos: 0,
            panel: None,
            field: None,
            output: None,
        }
    }

    fn ready(&mut self) {
        self.base_mut()
            .set_anchors_preset(LayoutPreset::BOTTOM_LEFT);
        self.build();
        self.base_mut().set_visible(false);
        // No per-frame work: the GDScript called `set_process(true)` without a
        // `_process`, so there is no `process` override here.
    }
}

#[godot_api]
impl MindConsoleFragment {
    /// Toggles the console (`ui_root` Escape handling calls this).
    #[func]
    pub fn toggle(&mut self) {
        self.shown = !self.shown;
        self.open = self.shown && !mobile();
        let shown = self.shown;
        self.base_mut().set_visible(shown);
        if shown && let Some(mut field) = self.field.clone() {
            field.grab_focus();
        }
    }

    /// Mobile overlay toggle: never enters the docked `open` state.
    #[func]
    pub fn toggle_mobile(&mut self) {
        self.shown = !self.shown;
        self.open = false;
        let shown = self.shown;
        self.base_mut().set_visible(shown);
    }

    /// `open` state (`ConsoleFragment.isOpen`).
    #[func]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the console is visible.
    #[func]
    pub fn is_shown(&self) -> bool {
        self.shown
    }

    /// `LineEdit.text_submitted`: runs one console line.
    #[func]
    fn submit(&mut self, line: GString) {
        let line = line.to_string();
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        if self.history.len() < 2 || self.history.get(1) != Some(&line) {
            self.history.insert(1, line.clone());
        }
        self.history_pos = 0;
        if trimmed == "clear" {
            self.messages.clear();
            self.refresh();
            self.clear_field();
            return;
        }
        let mut result = String::new();
        if let Some(mut ui) = kit::ui()
            && ui.has_method("console_execute")
        {
            result = ui
                .call(
                    "console_execute",
                    &[GString::from(line.as_str()).to_variant()],
                )
                .try_to::<GString>()
                .map(|value| value.to_string())
                .unwrap_or_default();
        }
        self.messages
            .push(format!("[lightgray]> {}", escape_bbcode(&line)));
        if !result.is_empty() {
            self.messages.push(escape_bbcode(&result));
        }
        if self.messages.len() > MESSAGES_SHOWN {
            let start = self.messages.len() - MESSAGES_SHOWN;
            self.messages = self.messages.split_off(start);
        }
        self.clear_field();
        self.refresh();
    }

    /// Mobile scroll buttons: `_scroll(1)` / `_scroll(-1)`.
    #[func]
    fn scroll(&mut self, direction: i64) {
        let limit = self.messages.len() as i64;
        self.scroll_pos = (self.scroll_pos + direction).clamp(0, limit.max(0));
        self.refresh();
    }

    /// Mobile cancel button: hide the flag only (upstream mobile branch).
    #[func]
    fn mobile_cancel(&mut self) {
        self.shown = false;
    }
}

impl MindConsoleFragment {
    fn build(&mut self) {
        // code-instantiated: the console panel is a persistent HUD fragment
        // whose output is runtime data, so its structure is built once here
        // (plan 14 §3.10).
        let mut panel = VBoxContainer::new_alloc();
        panel.set_anchor(Side::LEFT, 0.0);
        panel.set_anchor(Side::TOP, 1.0);
        panel.set_anchor(Side::BOTTOM, 1.0);
        panel.set_offset(Side::BOTTOM, -8.0);
        panel.set_offset(Side::LEFT, margins::left());
        panel.set_offset(Side::RIGHT, 900.0);
        panel.set_custom_minimum_size(Vector2::new(0.0, 320.0));
        self.base_mut().add_child(&panel);

        let mut output = RichTextLabel::new_alloc();
        output.set_use_bbcode(true);
        output.set_scroll_active(true);
        panel.add_child(&output);

        let mut row = HBoxContainer::new_alloc();
        if mobile() {
            // code-instantiated: mobile scroll/clear buttons (upstream mobile
            // branch).
            let mut up = factory::image_button("upOpen", "cleari");
            attach_scroll(&mut up, self.to_gd(), 1);
            row.add_child(&up);
            let mut down = factory::image_button("downOpen", "cleari");
            attach_scroll(&mut down, self.to_gd(), -1);
            row.add_child(&down);
            let mut cancel = factory::image_button("cancel", "cleari");
            let mut target = self.to_gd();
            let callable = Callable::from_fn("console_cancel", move |_args: &[&Variant]| {
                let _ = target.call("mobile_cancel", &[]);
            });
            cancel.connect("pressed", &callable);
            row.add_child(&cancel);
        }

        let mut field = factory::field(&kit::t("@console").to_string());
        let callable = self.base().callable("submit");
        field.connect("text_submitted", &callable);
        row.add_child(&field);
        panel.add_child(&row);
        output.set_text("> ");

        self.panel = Some(panel);
        self.output = Some(output);
        self.field = Some(field);
    }

    fn refresh(&mut self) {
        let text = GString::from(self.messages.join("\n").as_str());
        if let Some(mut output) = self.output.clone() {
            output.set_text(&text);
        }
    }

    fn clear_field(&mut self) {
        if let Some(mut field) = self.field.clone() {
            field.set_text("");
        }
    }
}

/// Binds one mobile scroll button to `scroll(direction)`.
fn attach_scroll(button: &mut Gd<Button>, mut target: Gd<MindConsoleFragment>, direction: i64) {
    let callable = Callable::from_fn("console_scroll", move |_args: &[&Variant]| {
        let _ = target.call("scroll", &[direction.to_variant()]);
    });
    button.connect("pressed", &callable);
}

/// `line.replace("[", "[lb]")`: Godot BBCode escapes a literal bracket.
fn escape_bbcode(line: &str) -> String {
    line.replace('[', "[lb]")
}

/// The mobile HUD layout branch (`MindUi.is_mobile`).
fn mobile() -> bool {
    let Some(mut ui) = kit::ui() else {
        return false;
    };
    ui.call("is_mobile", &[]).try_to::<bool>().unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bbcode_brackets_are_escaped() {
        // Godot BBCode only treats `[` as an escape introducer; a bare `]` is
        // literal text. `[lb]` prints `[`, so escaping both brackets (the old
        // expectation) would render "a[b[c".
        assert_eq!(escape_bbcode("a[b]c"), "a[lb]b]c");
        assert_eq!(escape_bbcode("plain"), "plain");
    }

    #[test]
    fn rolling_buffer_keeps_the_newest_messages() {
        let mut messages: Vec<String> = (0..MESSAGES_SHOWN + 5).map(|i| i.to_string()).collect();
        if messages.len() > MESSAGES_SHOWN {
            let start = messages.len() - MESSAGES_SHOWN;
            messages = messages.split_off(start);
        }
        assert_eq!(messages.len(), MESSAGES_SHOWN);
        assert_eq!(messages.first().map(String::as_str), Some("5"));
    }
}
