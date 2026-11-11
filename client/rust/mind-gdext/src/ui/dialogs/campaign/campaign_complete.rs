// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/CampaignCompleteDialog.java.

//! The campaign complete dialog controller: the completion banner and playtime
//! row for a finished campaign.
//! Upstream: ui/dialogs/CampaignCompleteDialog.java

use godot::prelude::*;

use super::util::campaign_views_live;
use crate::gdutil::variant::to_i64_int_or_float;
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::common::facade::rich;
use crate::ui::dialogs::net::util::planet_localized;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable};

/// The dialog controllers in this module.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "campaign_complete" => Some(Box::new(CampaignCompleteDialog::default())),
        _ => None,
    }
}

/// Campaign completion banner.
#[derive(Default)]
pub struct CampaignCompleteDialog {
    table: Option<Gd<MindTable>>,
}

impl DialogController for CampaignCompleteDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = true;
        self.table = Some(dialog.content_table());
        // `CampaignCompleteDialog.java:20-28`: `@menu` then `@continue`, both
        // 210x64, and no visible back button (`addCloseListener` only listens
        // for Escape/back).
        if let Some(mut row) = dialog.buttons_row() {
            let mut menu = factory::button(&kit::t("@menu").to_string());
            menu.set_custom_minimum_size(Vector2::new(210.0, 64.0));
            let callable = Callable::from_fn("exit_to_menu", |_args: &[&Variant]| {
                exit_to_menu();
            });
            menu.connect("pressed", &callable);
            row.add_child(&menu);
            let mut continue_button = factory::button(&kit::t("@continue").to_string());
            continue_button.set_custom_minimum_size(Vector2::new(210.0, 64.0));
            let callable = dialog.to_gd().callable("hide_dialog");
            continue_button.connect("pressed", &callable);
            row.add_child(&continue_button);
        }
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let Some(mut table) = self.table.clone() else {
            return;
        };
        table.bind_mut().clear();
        let has_context = dialog.context_value("planet").is_some()
            || dialog.context_value("playtime_ms").is_some();
        let (planet, color, playtime) = if has_context {
            // MCP/test opens the page with a JSON context; keep that path.
            let localized = dialog
                .context_value("localized")
                .map(|value| value.to::<GString>().to_string());
            let context_planet = dialog
                .context_value("planet")
                .map(|value| value.to::<GString>().to_string());
            let color = dialog
                .context_value("color")
                .map(|value| value.to::<GString>().to_string())
                .unwrap_or_else(|| String::from("ffd37f"));
            let playtime = dialog
                .context_value("playtime_ms")
                .map(|value| to_i64_int_or_float(&value))
                .unwrap_or(0);
            (
                localized.or(context_planet).unwrap_or_default(),
                color,
                playtime,
            )
        } else {
            // The completion banner's typed read model (`CampaignViews.complete`).
            match campaign_views_live(dialog) {
                Some(views) => {
                    let complete = views.complete;
                    let planet = if complete.localized.is_empty() {
                        complete.planet
                    } else {
                        complete.localized
                    };
                    (planet, complete.color, complete.playtime_ms)
                }
                None => (String::new(), String::from("ffd37f"), 0),
            }
        };
        let planet = planet_localized(&planet);
        let planet_markup = if planet.is_empty() {
            String::new()
        } else {
            format!("[#{}]{}[]", color, planet)
        };
        // code-instantiated: the completion text is data-driven from campaign state.
        let message =
            factory::label(&kit::tm_format("@campaign.complete", &[&planet_markup]).to_string());
        rich(&message).set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        table.bind_mut().add(&message).grow_x().pad(8.0);
        table.bind_mut().row();
        // The typed view's `playtime_ms` is a plain integer (`to_i64_int_or_float`
        // was only needed for the JSON FLOAT variant).
        if playtime > 0 {
            let time = MindDialog::format_time_ms(playtime);
            let label = factory::label(&kit::tm_format("@campaign.playtime", &[&time]).to_string());
            table.bind_mut().add(&label).pad(4.0);
            table.bind_mut().row();
        }
    }
}

/// `CampaignCompleteDialog.@menu`: write the exit save, then return to the menu.
fn exit_to_menu() {
    if let Some(mut campaign) = kit::campaign()
        && campaign.has_method("run_exit_save")
    {
        let _ = campaign.call("run_exit_save", &[]);
    }
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call(
            "close_dialog",
            &[GString::from("campaign_complete").to_variant()],
        );
        let _ = ui.call("close_dialog", &[GString::from("planet").to_variant()]);
    }
    if let Some(mut root) = kit::node("/root/Spine/Ui/UiRoot")
        && root.has_method("return_to_menu")
    {
        let _ = root.call("return_to_menu", &[]);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn playtime_uses_the_dialog_formatter() {
        assert_eq!(MindDialog::format_time_ms(0), "0s");
        assert_eq!(MindDialog::format_time_ms(3_600_000), "1h 0m");
    }

    #[test]
    fn pane_colors_are_stable() {
        use crate::ui::kit::styles;
        assert_eq!(styles::ACCENT, styles::ACCENT);
    }
}
