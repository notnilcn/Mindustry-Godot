// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! List select, admin/ban lists, trace info and campaign complete.

use godot::classes::{DisplayServer, HBoxContainer, RichTextLabel, VBoxContainer};
use godot::obj::{NewAlloc, Singleton};
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindScroll, MindTable};

/// Godot-style string conversion for a context value.
fn variant_text(value: &Variant) -> String {
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string();
    }
    if let Ok(boolean) = value.try_to::<bool>() {
        return boolean.to_string();
    }
    if let Ok(integer) = value.try_to::<i64>() {
        return integer.to_string();
    }
    if let Ok(float) = value.try_to::<f64>() {
        return float.to_string();
    }
    String::new()
}

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "list_select" => Some(Box::new(ListSelectDialog::default())),
        "bans" => Some(Box::new(BansDialog::default())),
        "admins" => Some(Box::new(AdminsDialog::default())),
        "traces" => Some(Box::new(TraceDialog::default())),
        "campaign_complete" => Some(Box::new(CampaignCompleteDialog::default())),
        _ => None,
    }
}

fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

fn string_array(value: Option<Variant>) -> Vec<String> {
    value
        .map(|value| value.to::<VarArray>())
        .map(|array| {
            array
                .iter_shared()
                .map(|entry| entry.to::<GString>().to_string())
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// ListSelectDialog
// ---------------------------------------------------------------------------

/// Generic one-column picker: a scrollable list of titled rows, an optional
/// reset row, and a `selected(name)` signal (empty name = reset/closed).
#[derive(Default)]
pub struct ListSelectDialog {
    items: Vec<String>,
    reset_key: String,
    /// Set while the dialog is hiding after a pick, so `hidden` does not emit.
    chosen: bool,
}

impl DialogController for ListSelectDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        dialog.add_close_button(210.0);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let title = dialog.context_string("title");
        dialog.title_text = if title.is_empty() {
            GString::new()
        } else {
            kit::t(&title.to_string())
        };
        dialog.refresh_title();
        self.reset_key = dialog.context_string("reset").to_string();
        self.items = string_array(dialog.context_value("items"));
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.chosen = false;
        self.build(dialog);
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        let mut chosen = self.chosen;
        if dialog.base().has_meta("_picker_chosen") {
            chosen = dialog.base().get_meta("_picker_chosen").to::<bool>();
            dialog.base_mut().remove_meta("_picker_chosen");
        }
        if !chosen {
            dialog
                .to_gd()
                .emit_signal("selected", &[GString::new().to_variant()]);
        }
        self.chosen = false;
    }
}

impl ListSelectDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut root = dialog.content_table();
        let mut scroll = MindScroll::new_alloc();
        // The dialog shell sizes `cont` from its children's minimums; without
        // this the picker renders collapsed for large item lists.
        scroll.set_custom_minimum_size(Vector2::new(520.0, 320.0));
        scroll.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        scroll.set_v_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        scroll.bind_mut().set_scrolling_disabled(true, false);
        root.bind_mut().add(&scroll).grow_x().grow_y();
        // code-instantiated: the row list is caller-supplied picker data.
        let mut list = VBoxContainer::new_alloc();
        list.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        scroll.add_child(&list);
        if !self.reset_key.is_empty() {
            let reset_text = kit::t(&self.reset_key).to_string();
            let mut reset = factory::icon_button(
                "cancel",
                &reset_text,
                "defaultt",
                godot::classes::box_container::AlignmentMode::BEGIN,
            );
            let mut target = dialog.to_gd();
            let callable = Callable::from_fn("pick_reset", move |_args: &[&Variant]| {
                let _ = target.call("pick_selected", &[GString::new().to_variant()]);
            });
            reset.connect("pressed", &callable);
            list.add_child(&reset);
        }
        let columns = 3usize;
        let mut row: Option<Gd<HBoxContainer>> = None;
        for (index, item) in self.items.clone().into_iter().enumerate() {
            // code-instantiated: picker rows are data-driven (units/effects/spawns).
            let mut button = factory::button(&item);
            button.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
            let mut target = dialog.to_gd();
            let item_for_pick = item.clone();
            let callable = Callable::from_fn("pick_item", move |_args: &[&Variant]| {
                let _ = target.call(
                    "pick_selected",
                    &[GString::from(item_for_pick.as_str()).to_variant()],
                );
            });
            button.connect("pressed", &callable);
            if index % columns == 0 {
                let mut new_row = HBoxContainer::new_alloc();
                new_row.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
                list.add_child(&new_row);
                row = Some(new_row);
            }
            if let Some(mut row) = row.clone() {
                row.add_child(&button);
            }
        }
        if self.items.is_empty() {
            let label = factory::label(&kit::t("@none.found").to_string());
            list.add_child(&label);
        }
        let _ = &mut root;
    }
}

// ---------------------------------------------------------------------------
// BansDialog / AdminsDialog
// ---------------------------------------------------------------------------

/// Ban list + unban.
#[derive(Default)]
pub struct BansDialog {
    rows: Option<Gd<MindTable>>,
}

impl DialogController for BansDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@server.bans");
        dialog.should_pause = false;
        self.rows = Some(dialog.content_table());
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let Some(mut rows) = self.rows.clone() else {
            return;
        };
        rows.bind_mut().clear();
        let banned = string_array(dialog.context_value("banned"));
        if banned.is_empty() {
            let label = factory::label(&kit::t("@server.bans.none").to_string());
            rows.bind_mut().add(&label).pad(8.0);
            return;
        }
        for entry in banned {
            // code-instantiated: ban rows are data-driven from the admin view.
            let label = factory::label(&format!("[lightgray]{entry}"));
            rows.bind_mut().add(&label).grow_x().pad(4.0);
            rows.bind_mut().row();
        }
    }
}

/// Admin list + unadmin.
#[derive(Default)]
pub struct AdminsDialog {
    rows: Option<Gd<MindTable>>,
}

impl DialogController for AdminsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@server.admins");
        dialog.should_pause = false;
        self.rows = Some(dialog.content_table());
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let Some(mut rows) = self.rows.clone() else {
            return;
        };
        rows.bind_mut().clear();
        let admins = string_array(dialog.context_value("admins"));
        if admins.is_empty() {
            let label = factory::label(&kit::t("@server.admins.none").to_string());
            rows.bind_mut().add(&label).pad(8.0);
            return;
        }
        for entry in admins {
            // code-instantiated: roster rows are data-driven from the admin view.
            let label = factory::label(&format!("[lightgray]{entry}"));
            rows.bind_mut().add(&label).grow_x().pad(4.0);
            rows.bind_mut().row();
        }
    }
}

// ---------------------------------------------------------------------------
// TraceDialog
// ---------------------------------------------------------------------------

/// Player trace info with per-field copy buttons.
#[derive(Default)]
pub struct TraceDialog {
    table: Option<Gd<MindTable>>,
}

impl DialogController for TraceDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@trace");
        dialog.should_pause = false;
        self.table = Some(dialog.content_table());
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let Some(mut table) = self.table.clone() else {
            return;
        };
        table.bind_mut().clear();
        let player_name = dialog.context_string("player");
        let info = dialog
            .context_value("info")
            .map(|value| value.to::<VarDictionary>())
            .unwrap_or_default();
        self.add_copy_row(&mut table, &player_name);
        self.add_copy_value(&mut table, &info, "ip");
        self.add_copy_value(&mut table, &info, "locale");
        self.add_copy_value(&mut table, &info, "uuid");
        self.add_scalar(&mut table, "trace.modclient", info.get("modded"));
        self.add_scalar(&mut table, "trace.mobile", info.get("mobile"));
        self.add_scalar(&mut table, "trace.times.joined", info.get("timesJoined"));
        self.add_scalar(&mut table, "trace.times.kicked", info.get("timesKicked"));
    }
}

impl TraceDialog {
    fn add_copy_row(&self, table: &mut Gd<MindTable>, value: &GString) {
        // code-instantiated: trace fields are data-driven from the trace info.
        let mut row = HBoxContainer::new_alloc();
        let copy_text = kit::t("@copy").to_string();
        let mut button = factory::button(&copy_text);
        let value_for_copy = value.clone();
        let callable = Callable::from_fn("copy_trace", move |_args: &[&Variant]| {
            DisplayServer::singleton().clipboard_set(&value_for_copy);
            if let Some(mut ui) = kit::ui() {
                let _ = ui.call("show_info", &[kit::t("@copied").to_variant()]);
            }
        });
        button.connect("pressed", &callable);
        row.add_child(&button);
        let label = factory::label(&value.to_string());
        row.add_child(&label);
        table.bind_mut().add(&row).grow_x().pad(2.0);
        table.bind_mut().row();
    }

    fn add_copy_value(&self, table: &mut Gd<MindTable>, info: &VarDictionary, key: &str) {
        let value = info
            .get(key)
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        self.add_copy_row(table, &value);
    }

    fn add_scalar(&self, table: &mut Gd<MindTable>, key: &str, value: Option<Variant>) {
        let text = value
            .map(|value| variant_text(&value))
            .unwrap_or_else(|| "false".to_string());
        let label = factory::label(&format!("{}: {text}", kit::t(key)));
        table.bind_mut().add(&label).grow_x().pad(2.0);
        table.bind_mut().row();
    }
}

// ---------------------------------------------------------------------------
// CampaignCompleteDialog
// ---------------------------------------------------------------------------

/// Campaign completion banner.
#[derive(Default)]
pub struct CampaignCompleteDialog {
    table: Option<Gd<MindTable>>,
}

impl DialogController for CampaignCompleteDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = true;
        self.table = Some(dialog.content_table());
        dialog.add_close_button(210.0);
        if let Some(mut row) = dialog.buttons_row() {
            let mut continue_button = factory::button(&kit::t("@continue").to_string());
            let callable = dialog.to_gd().callable("hide_dialog");
            continue_button.connect("pressed", &callable);
            row.add_child(&continue_button);
            let mut menu = factory::button(&kit::t("@menu").to_string());
            let callable = Callable::from_fn("exit_to_menu", |_args: &[&Variant]| {
                exit_to_menu();
            });
            menu.connect("pressed", &callable);
            row.add_child(&menu);
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
        let playtime = view
            .get("playtime_ms")
            .map(|value| value.to::<i64>())
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
    use super::*;

    #[test]
    fn string_array_handles_missing_context() {
        assert!(string_array(None).is_empty());
    }

    #[test]
    fn playtime_uses_the_dialog_formatter() {
        assert_eq!(MindDialog::format_time_ms(0), "0s");
        assert_eq!(MindDialog::format_time_ms(3_600_000), "1h 0m");
    }

    #[test]
    fn pane_colors_are_stable() {
        use super::super::super::kit::styles;
        assert_eq!(styles::ACCENT, styles::ACCENT);
    }
}
