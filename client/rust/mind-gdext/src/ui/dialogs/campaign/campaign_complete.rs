// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/CampaignCompleteDialog.java.

//! The campaign complete dialog controller: the completion banner and playtime
//! row for a finished campaign.
//! Upstream: ui/dialogs/CampaignCompleteDialog.java

use godot::prelude::*;

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
        let view = if has_context {
            let mut copy = VarDictionary::new();
            for key in ["localized", "planet", "color", "playtime_ms"] {
                if let Some(value) = dialog.context_value(key) {
                    copy.set(key, &value);
                }
            }
            copy
        } else {
            dialog
                .campaign_views()
                .get("complete")
                .map(|value| value.to::<VarDictionary>())
                .unwrap_or_default()
        };
        let planet = view
            .get("localized")
            .or_else(|| view.get("planet"))
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        // `Planet.localizedName` (`planet.<name>.name`): the campaign read
        // model falls back to the raw content name when the registry's bundle
        // is the empty memory bundle, so resolve the live bundle here.
        let planet = planet_localized(&planet.to_string());
        let color = view
            .get("color")
            .map(|value| value.to::<GString>())
            .unwrap_or_else(|| GString::from("ffd37f"));
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
        // The campaign read models carry JSON numbers, which are FLOAT
        // variants: `to::<i64>()` panics on the type mismatch and aborted the
        // build before the playtime row (`CampaignCompleteDialog.java:37-40`).
        let playtime = view
            .get("playtime_ms")
            .map(|value| to_i64_int_or_float(&value))
            .unwrap_or(0);
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
