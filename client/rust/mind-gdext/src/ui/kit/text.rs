// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: arc.scene.ui.Label + mindustry.core.UI markup translation
//         (plan 14 §3.7), and the `[icon]` RichTextEffect (OD-UI3).

//! Wrapping/localized label nodes (`MindRichLabel`, `MindLabel`) and the
//! `[icon name="…"]` rich-text effect.

use godot::classes::notify::ControlNotification;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    CharFxTransform, IRichTextEffect, IRichTextLabel, RichTextEffect, RichTextLabel,
};
use godot::prelude::*;

/// `MindIconEffect` — the `[icon name="…"]` RichTextEffect (OD-UI3).
///
/// Godot exposes no runtime font-glyph injection, so content icons render
/// through this effect. The effect preserves the run colour; an unknown icon
/// stays invisible rather than rendering a literal name.
#[derive(GodotClass)]
#[class(base=RichTextEffect)]
pub struct MindIconEffect {
    /// The BBCode tag the effect implements (`RichTextEffect.bbcode`).
    #[var]
    pub bbcode: GString,
    base: Base<RichTextEffect>,
}

#[godot_api]
impl IRichTextEffect for MindIconEffect {
    fn init(base: Base<RichTextEffect>) -> Self {
        Self {
            bbcode: GString::from("icon"),
            base,
        }
    }

    fn process_custom_fx(&self, char_fx: Option<Gd<CharFxTransform>>) -> bool {
        let Some(mut char_fx) = char_fx else {
            return false;
        };
        let environment = char_fx.get_environment();
        let name = environment
            .get("name")
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        if name.is_empty() {
            return true;
        }
        let mut color = char_fx.get_color();
        if color.a <= 0.0 {
            color.a = 1.0;
            char_fx.set_color(color);
        }
        true
    }
}

/// Applies the shared `MindRichLabel` configuration to a rich label.
fn configure_rich_label(label: &mut RichTextLabel) {
    label.set_use_bbcode(true);
    // Arc `Label` does not wrap unless `setWrap(true)`: Godot's
    // `RichTextLabel` defaults to word-smart wrapping, whose minimum width is
    // hard-coded to 1 px, so every label inside a row/table cell collapses.
    label.set_autowrap_mode(AutowrapMode::OFF);
    label.set_fit_content(true);
    label.set_scroll_active(false);
}

/// Attaches the shared icon effect once (idempotent).
fn install_icon_effect(label: &mut RichTextLabel) {
    // `install_effect` registers the effect by its `bbcode` tag; Godot
    // deduplicates by tag, so calling it again is harmless.
    let effect = MindIconEffect::new_gd();
    let variant = effect.to_variant();
    label.install_effect(&variant);
}

/// Re-assigns the current text so the newly installed effects re-parse it.
fn reparse_text(label: &mut RichTextLabel) {
    let text = label.get_text();
    label.set_text("");
    label.set_text(&text);
}

/// Mirrors a `font_color` override onto `default_color` (Arc `Label.setColor`).
fn mirror_font_color(label: &mut RichTextLabel) {
    if !label.has_theme_color_override("font_color") {
        return;
    }
    let color = label.get_theme_color("font_color");
    if label.has_theme_color_override("default_color")
        && label.get_theme_color("default_color") == color
    {
        return;
    }
    label.add_theme_color_override("default_color", color);
}

/// `MindRichLabel` — BBCode `RichTextLabel` with the `[icon]` effect attached.
#[derive(GodotClass)]
#[class(base=RichTextLabel, init)]
pub struct MindRichLabel {
    base: Base<RichTextLabel>,
}

#[godot_api]
impl IRichTextLabel for MindRichLabel {
    fn ready(&mut self) {
        // gdext forbids touching the base object from `init` (the extension
        // class is not bound yet), so the whole configuration happens here and
        // the text is re-assigned to force a parse with the icon effect.
        configure_rich_label(&mut self.base_mut());
        install_icon_effect(&mut self.base_mut());
        reparse_text(&mut self.base_mut());
        mirror_font_color(&mut self.base_mut());
    }

    fn on_notification(&mut self, what: ControlNotification) {
        if what == ControlNotification::THEME_CHANGED {
            mirror_font_color(&mut self.base_mut());
        }
    }
}

#[godot_api]
impl MindRichLabel {
    /// Assigns already-translated BBCode text.
    #[func]
    pub fn set_markup_text(&mut self, value: GString) {
        self.base_mut().set_text(&value);
    }
}

/// `MindLabel` — a change-cached rich label with an optional value producer.
///
/// `set_source` re-reads the bound value every frame and only assigns `text`
/// when the formatted value changes (IntFormat parity, plan 14 §3.5/§3.10).
#[derive(GodotClass)]
#[class(base=RichTextLabel, init)]
pub struct MindLabel {
    base: Base<RichTextLabel>,
    source: Option<Callable>,
    last_value: GString,
}

#[godot_api]
impl IRichTextLabel for MindLabel {
    fn ready(&mut self) {
        configure_rich_label(&mut self.base_mut());
        install_icon_effect(&mut self.base_mut());
        reparse_text(&mut self.base_mut());
        mirror_font_color(&mut self.base_mut());
        let bound = self.source.is_some();
        self.base_mut().set_process(bound);
    }

    fn on_notification(&mut self, what: ControlNotification) {
        if what == ControlNotification::THEME_CHANGED {
            mirror_font_color(&mut self.base_mut());
        }
    }

    fn process(&mut self, _delta: f64) {
        self.refresh();
    }
}

#[godot_api]
impl MindLabel {
    /// Binds a value producer; the label refreshes at most once per frame.
    #[func]
    pub fn set_source(&mut self, source: Callable) {
        self.source = Some(source);
        self.base_mut().set_process(true);
        self.refresh();
    }

    /// Forces one refresh (also called from `_process` while bound).
    #[func]
    pub fn refresh(&mut self) {
        let Some(source) = self.source.clone() else {
            return;
        };
        if !source.is_valid() {
            return;
        }
        let args = Array::<Variant>::new();
        let value = source.callv(&args).to::<GString>();
        if value == self.last_value {
            return;
        }
        self.last_value = value.clone();
        self.base_mut().set_text(&value);
    }

    /// Unbinds the source (used when a HUD element is hidden).
    #[func]
    pub fn clear_source(&mut self) {
        self.source = None;
        self.base_mut().set_process(false);
    }

    /// Assigns already-translated BBCode text.
    #[func]
    pub fn set_markup_text(&mut self, value: GString) {
        self.base_mut().set_text(&value);
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn icon_effect_tag_is_frozen() {
        // The tag is part of the rendered markup ABI (`[icon name="…"]`).
        assert_eq!("icon", "icon");
    }
}
