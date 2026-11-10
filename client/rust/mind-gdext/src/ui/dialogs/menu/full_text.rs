// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The full text dialog (`full_text`): a scrollable plain-text body with an optional `@ok` button.
//! Upstream: core/src/mindustry/ui/dialogs/FullTextDialog.java

use godot::classes::Control;
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindLabel, styles};

use super::util::rich;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "full_text" => Some(Box::new(FullTextDialog::default())),
        _ => None,
    }
}

/// Scrollable plain-text dialog (`shouldPause = true`).
#[derive(Default)]
pub struct FullTextDialog {
    content: Option<Gd<MindLabel>>,
}

impl DialogController for FullTextDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = true;
        // `FullTextDialog.java`: `super("")` + `addCloseButton()`; the text floats
        // over the defaultDialog backdrop (no boxed panel).
        dialog.enable_full_dialog(true);
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let text = dialog.context_string("text");
        if !text.is_empty() {
            self.set_content(
                dialog,
                &dialog.context_string("title").to_string(),
                &text.to_string(),
            );
        }
        // `PlanetDialog.java:216-222`: the Campaign Changes Notice uses an `@ok`
        // button instead of the generic close.
        if dialog
            .context_value("ok")
            .map(|value| value.to::<bool>())
            .unwrap_or(false)
        {
            dialog.clear_buttons();
            let callable = dialog.to_gd().callable("hide_dialog");
            let ok = kit::t("@ok").to_string();
            dialog.add_button(&ok, Some(callable), "ok", 280.0);
        }
    }
}

impl FullTextDialog {
    /// Sets the title/body and opens the dialog (`show(titleText, text)`).
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn show_full_text(&mut self, dialog: &mut MindDialog, title: &str, text: &str) {
        self.set_content(dialog, title, text);
        dialog.show_dialog();
    }

    fn set_content(&mut self, dialog: &mut MindDialog, title: &str, text: &str) {
        dialog.title_text = GString::from(title);
        dialog.title_color = styles::ACCENT;
        dialog.refresh_title();
        dialog.clear_content();
        // code-instantiated: the body text is data-driven (bundle text supplied
        // by the caller); it cannot be authored in the static scene.
        let label = factory::label(text);
        let mut rich_label = rich(&label);
        rich_label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        rich_label.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        if dialog
            .context_value("ok")
            .map(|value| value.to::<bool>())
            .unwrap_or(false)
        {
            rich_label.set_custom_minimum_size(Vector2::new(500.0, 0.0));
        }
        // `FullTextDialog.java:19`: `.labelAlign(Align.center)`.
        rich_label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        if let Some(mut content) = dialog.content() {
            let control: Gd<Control> = label.clone().upcast();
            content.add_child(&control);
        }
        self.content = Some(label);
    }
}
