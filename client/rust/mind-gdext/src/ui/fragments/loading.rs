// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/LoadingFragment.java (44-138).

//! Full-screen loading overlay: progress bar + text + cancel button.
//!
//! The HUD refresh polls `MindSimHost.io_pending` and drives
//! `set_progress`/`hide_loading` while an async save/load is in flight;
//! `loadAnd` (the async completion helper) is owned by the Rust IO flow.

use godot::classes::{Button, Control, IControl, Label, ProgressBar};
use godot::prelude::*;

use crate::ui::kit;
use crate::ui::kit::styles;

/// `MindLoadingFragment` — the loading overlay.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindLoadingFragment {
    base: Base<Control>,
}

#[godot_api]
impl IControl for MindLoadingFragment {
    fn ready(&mut self) {
        self.base_mut().set_visible(false);
        if let Some(mut button) = self.button() {
            button.set_text(&kit::t("@cancel"));
            let callable = self.base().callable("cancel_pressed");
            button.connect("pressed", &callable);
        }
    }
}

impl MindLoadingFragment {
    fn bar(&self) -> Option<Gd<ProgressBar>> {
        self.base()
            .get_node_or_null("Panel/Layout/Bar")?
            .try_cast()
            .ok()
    }

    fn text_label(&self) -> Option<Gd<Label>> {
        self.base()
            .get_node_or_null("Panel/Layout/Text")?
            .try_cast()
            .ok()
    }

    fn button(&self) -> Option<Gd<Button>> {
        self.base()
            .get_node_or_null("Panel/Layout/Button")?
            .try_cast()
            .ok()
    }
}

#[godot_api]
impl MindLoadingFragment {
    /// Emitted when the cancel button is pressed (`cancelListener`).
    #[signal]
    fn cancel_requested();

    /// Shows the overlay at `fraction` (0..1) with an optional `text` message.
    #[func]
    pub fn set_progress(&mut self, fraction: f32, text: GString) {
        self.base_mut().set_visible(true);
        // Undo `show_text`'s text-only layout and colour.
        if let Some(mut button) = self.button() {
            button.set_visible(true);
        }
        if let Some(mut label) = self.text_label() {
            label.remove_theme_color_override("font_color");
        }
        if let Some(mut bar) = self.bar() {
            bar.set_visible(true);
            bar.set_value(fraction.clamp(0.0, 1.0) as f64 * 100.0);
        }
        if !text.is_empty()
            && let Some(mut label) = self.text_label()
        {
            label.set_text(&text);
        }
    }

    /// `show(String text)` (`LoadingFragment.java:111-122`): text only, no bar
    /// and no cancel button.
    #[func]
    pub fn show_text(&mut self, text: GString) {
        self.base_mut().set_visible(true);
        if let Some(mut bar) = self.bar() {
            bar.set_visible(false);
        }
        if let Some(mut button) = self.button() {
            button.set_visible(false);
        }
        self.set_markup(&text);
    }

    /// `nameLabel` is an Arc markup label: `@loading` is `[accent]Loading...`.
    /// The scene's plain `Label` takes the leading colour tag as its font colour
    /// and drops the tags.
    fn set_markup(&mut self, text: &GString) {
        let source = text.to_string();
        let mut color = Color::WHITE;
        if let Some(rest) = source.strip_prefix('[')
            && let Some(end) = rest.find(']')
        {
            let name = &rest[..end];
            if name == "accent" {
                color = styles::ACCENT;
            } else if let Some(parsed) = Color::from_html(name) {
                color = parsed;
            } else if let Some(parsed) = Color::from_string(name) {
                color = parsed;
            }
        }
        if let Some(mut label) = self.text_label() {
            label.add_theme_color_override("font_color", color);
            label.set_text(&strip_markup_tags(&source));
        }
    }

    /// Updates the label only.
    #[func]
    pub fn set_text(&mut self, text: GString) {
        if let Some(mut label) = self.text_label() {
            label.set_text(&text);
        }
    }

    /// Hides the overlay (`hideLoading`).
    #[func]
    pub fn hide_loading(&mut self) {
        self.base_mut().set_visible(false);
    }

    /// Whether the overlay is up.
    #[func]
    pub fn is_shown(&self) -> bool {
        self.base().is_visible()
    }

    /// `LoadingFragment` cancel button: close now and forward the intent.
    #[func]
    fn cancel_pressed(&mut self) {
        self.hide_loading();
        self.base_mut().emit_signal("cancel_requested", &[]);
    }
}

/// Drops every `[...]` markup tag from a string (Arc markup -> plain label).
fn strip_markup_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for character in text.chars() {
        if in_tag {
            if character == ']' {
                in_tag = false;
            }
            continue;
        }
        if character == '[' {
            in_tag = true;
            continue;
        }
        out.push(character);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_arc_markup() {
        assert_eq!(strip_markup_tags("[accent]Loading...[]"), "Loading...");
        assert_eq!(strip_markup_tags("plain"), "plain");
        assert_eq!(strip_markup_tags("[#ff0000]x[/]"), "x");
    }

    #[test]
    fn progress_clamps_to_percent() {
        assert_eq!((0.5_f32.clamp(0.0, 1.0) * 100.0) as f64, 50.0);
        assert_eq!((1.5_f32.clamp(0.0, 1.0) * 100.0) as f64, 100.0);
    }
}
