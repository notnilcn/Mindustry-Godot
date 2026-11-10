// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The palette dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/PaletteDialog.java

use crate::ui::dialog::{DialogController, MindDialog};
use godot::classes::{Button, Control};
use godot::prelude::*;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "palette" => Some(Box::new(PaletteDialog)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// PaletteDialog
// ---------------------------------------------------------------------------

/// `Vars.playerColors` — names/values are the upstream ABI.
const PLAYER_COLORS: [&str; 16] = [
    "82759a", "c0c1c5", "ffffff", "7d2953", "ff074e", "ff072a", "ff76a6", "a95238", "ffa108",
    "feeb2c", "ffcaa8", "008551", "00e339", "423c7b", "4b5ef1", "2cabfe",
];

/// A 4-column grid of the 16 vanilla player colors.
pub struct PaletteDialog;

impl DialogController for PaletteDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@color");
        dialog.should_pause = false;
        let mut grid = dialog.content_table();
        {
            let mut control: Gd<Control> = grid.clone().upcast();
            control.add_theme_constant_override("separation", 4);
        }
        let mut column = 0usize;
        for hex in PLAYER_COLORS {
            let color = Color::from_string(hex).unwrap_or(Color::WHITE);
            // code-instantiated: palette entries come from the playerColors table.
            let mut swatch = Button::new_alloc();
            swatch.set_theme_type_variation("squareTogglei");
            swatch.set_custom_minimum_size(Vector2::new(48.0, 48.0));
            swatch.set_modulate(color);
            let mut target = dialog.to_gd();
            let callable = Callable::from_fn("choose_color", move |_args: &[&Variant]| {
                let _ = target.emit_signal("color_selected", &[color.to_variant()]);
                let _ = target.call("hide_dialog", &[]);
            });
            swatch.connect("pressed", &callable);
            grid.bind_mut().add(&swatch).size(48.0);
            column += 1;
            if column.is_multiple_of(4) {
                grid.bind_mut().row();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn player_colors_are_the_upstream_table() {
        assert_eq!(PLAYER_COLORS.len(), 16);
        assert_eq!(PLAYER_COLORS[0], "82759a");
        assert_eq!(PLAYER_COLORS[15], "2cabfe");
    }
}
