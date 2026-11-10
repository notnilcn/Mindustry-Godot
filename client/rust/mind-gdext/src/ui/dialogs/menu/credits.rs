// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The Credits dialog (`credits`): the blurb, the accent rule and the contributor grid.
//! Upstream: none (port-only; `client/ui/dialogs/credits_dialog.gd`, AboutDialog.showCredits)

use godot::classes::ColorRect;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, styles};

use super::util::rich;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "credits" => Some(Box::new(CreditsDialog)),
        _ => None,
    }
}

/// `@credits` — blurb, accent rule, contributor grid.
pub struct CreditsDialog;

/// Contributor grid columns.
const COLUMNS: usize = 3;

impl DialogController for CreditsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@credits");
        let mut table = dialog.content_table();
        let blurb = factory::label(&kit::tm("@credits.text").to_string());
        let mut blurb_rich = rich(&blurb);
        blurb_rich.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        blurb_rich.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        blurb_rich.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        table
            .bind_mut()
            .add(&blurb)
            .grow_x()
            .pad(12.0)
            .colspan(COLUMNS as i32);
        table.bind_mut().row();
        // code-instantiated: the accent rule between the blurb and the
        // contributors.
        let mut rule = ColorRect::new_alloc();
        rule.set_color(styles::ACCENT);
        rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
        rule.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        table
            .bind_mut()
            .add(&rule)
            .grow_x()
            .pad(3.0)
            .colspan(COLUMNS as i32);
        table.bind_mut().row();
        let header = factory::label(&kit::t("@contributors").to_string());
        rich(&header).add_theme_color_override("default_color", styles::ACCENT);
        table
            .bind_mut()
            .add(&header)
            .grow_x()
            .pad(4.0)
            .colspan(COLUMNS as i32);
        table.bind_mut().row();
        let mut column = 0usize;
        for contributor in contributors() {
            // code-instantiated: contributor rows are data-driven from the
            // packed assets file.
            let label = factory::label(&contributor);
            rich(&label)
                .add_theme_color_override("default_color", Color::from_rgba(0.8, 0.8, 0.8, 1.0));
            table
                .bind_mut()
                .add(&label)
                .align(kit::Align::Left)
                .pad(3.0)
                .pad_left(6.0)
                .pad_right(6.0);
            column += 1;
            if column.is_multiple_of(COLUMNS) {
                table.bind_mut().row();
            }
        }
        if !column.is_multiple_of(COLUMNS) {
            table.bind_mut().row();
        }
        dialog.add_close_button(210.0);
    }
}

/// Contributor names from the packed `contributors` file.
fn contributors() -> Vec<String> {
    let Some(assets) = kit::assets() else {
        return Vec::new();
    };
    let mut node: Gd<Node> = assets.upcast();
    let value = node.call("contributors", &[]);
    let names = value.to::<PackedStringArray>();
    names
        .as_slice()
        .iter()
        .map(|name| name.to_string())
        .collect()
}
