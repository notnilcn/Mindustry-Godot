// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map info dialog controller.
//! Upstream: editor/MapInfoDialog.java

use godot::builtin::Side;
use godot::classes::Button;
use godot::classes::HBoxContainer;
use godot::classes::PanelContainer;
use godot::classes::control::SizeFlags;
use godot::classes::scroll_container::ScrollMode;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindScroll;
use crate::ui::kit::MindTable;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

use super::util::*;
use crate::gdutil::variant::to_text_stringify;

// ---------------------------------------------------------------------------
// MapInfoDialog (`map_info`, `MapInfoDialog.java`)
// ---------------------------------------------------------------------------

/// `MapInfoDialog.setup` route grid: label key, icon alias, target dialog;
/// `wide` spans both columns (`Icon.fileCode`'s 460px row).
const MAP_INFO_ROUTES: [(&str, &str, &str, bool); 7] = [
    ("@editor.rules", "list", "rules", false),
    ("@editor.waves", "units", "wave_info", false),
    ("@editor.objectives", "info", "map_objectives", false),
    ("@editor.generation", "terrain", "map_generate", false),
    ("@editor.locales", "file-text", "map_locales", false),
    ("@editor.worldprocessors", "logic", "map_processors", false),
    ("@asset.title", "tags", "map_assets", true),
];

/// Guard so the `custom_rules` `rules_edited` signal binds exactly once.
const MAP_INFO_RULES_META: &str = "mind_map_info_rules_bound";
/// Set on the `map_generate` dialog by [`MapInfoDialog`]: `MapGenerateDialog(false)`.
pub(super) const GEN_APPLIED_META: &str = "mind_map_generate_applied";

/// `@editor.mapinfo` — the map-name/description/author fields and the seven
/// routes into the other editor dialogs (`MapInfoDialog.java:33-126`).
pub struct MapInfoDialog;

impl DialogController for MapInfoDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.mapinfo");
        dialog.should_pause = false;
        dialog.add_default_close_button();
        // `cont.pane(...)`: the pane is the body, so the shell must not scroll.
        if let Some(mut body) = dialog.body() {
            body.set_vertical_scroll_mode(ScrollMode::DISABLED);
        }
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let _ = MindDialog::take_request(dialog);
        self.build(dialog);
    }
}

impl MapInfoDialog {
    /// `MapInfoDialog.setup`: the field table inside a pane, then the route
    /// grid inside one `Tex.button` frame (2-column / wide rows).
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        let mut root = dialog.content_table();
        // `cont.pane(t -> ...).growX().scrollX(false)`.
        // code-instantiated: the pane wraps the runtime-built field table.
        let mut pane = MindScroll::new_alloc();
        pane.bind_mut().set_scrolling_disabled(true, false);
        let mut inner = pane.bind_mut().content();
        {
            let mut inner_bound = inner.bind_mut();
            inner_bound.grow_horizontal();
            inner_bound.set_table_top(true);
            // `t.margin(16f)`.
            inner_bound.margin(16.0);
            inner_bound.defaults().pad_top = 15.0;
        }
        let mut pane_control = as_control(&pane);
        pane_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let mut inner_control = as_control(&inner);
        inner_control.set_h_size_flags(SizeFlags::EXPAND_FILL);
        root.bind_mut().add(&pane).grow();
        self.add_field(
            dialog,
            &mut inner,
            "@editor.mapname",
            "name",
            50,
            "@unknown",
            false,
            0.0,
        );
        self.add_field(
            dialog,
            &mut inner,
            "@editor.description",
            "description",
            1000,
            "",
            true,
            15.0,
        );
        self.add_field(
            dialog,
            &mut inner,
            "@editor.author",
            "author",
            50,
            "@unknown",
            false,
            15.0,
        );
        // code-instantiated: `t.table(Tex.button, r -> ...)` frame around the routes.
        let mut frame = PanelContainer::new_alloc();
        let mut box_ = pane_frame(Color::BLACK, styles::PANE_OUTLINE_GRAY, 2.0);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            box_.set_content_margin(side, 12.0);
        }
        frame.add_theme_stylebox_override("panel", &box_);
        let mut grid = factory::table();
        frame.add_child(&as_control(&grid));
        let mut column = 0usize;
        for (key, icon, action, wide) in MAP_INFO_ROUTES {
            // code-instantiated: one route button per `MapInfoDialog` entry.
            let text = kit::t(key).to_string();
            let mut button = factory::icon_button(
                icon,
                &text,
                "flatt",
                godot::classes::box_container::AlignmentMode::BEGIN,
            );
            inset_icon_row(&mut button, 10.0);
            let target = dialog.to_gd();
            let action_for_press = String::from(action);
            let callable = Callable::from_fn("map_info_route", move |_args: &[&Variant]| {
                open_map_info_route(&target, &action_for_press);
            });
            button.connect("pressed", &callable);
            {
                let mut grid_bound = grid.bind_mut();
                let cell = grid_bound.add(&button);
                let cell = if wide {
                    cell.width(460.0)
                } else {
                    cell.width(230.0)
                };
                let cell = cell.height(60.0);
                let _ = cell.colspan(if wide { 2 } else { 1 });
            }
            if wide {
                grid.bind_mut().row();
                column = 0;
            } else {
                column += 1;
                if column.is_multiple_of(2) {
                    grid.bind_mut().row();
                }
            }
        }
        if !column.is_multiple_of(2) {
            grid.bind_mut().row();
        }
        root.bind_mut().add(&frame).colspan(2);
        root.bind_mut().row();
    }

    /// One label + field row (`t.add(key).padRight(8).left(); t.field(...)`).
    #[allow(clippy::too_many_arguments)]
    fn add_field(
        &mut self,
        dialog: &mut MindDialog,
        table: &mut Gd<MindTable>,
        label_key: &str,
        tag: &str,
        max_length: i32,
        placeholder: &str,
        multiline: bool,
        label_pad_top: f32,
    ) {
        let label = factory::label(&kit::t(label_key).to_string());
        {
            let mut bound = table.bind_mut();
            let _ = bound
                .add(&label)
                .left()
                .pad_right(8.0)
                .pad_top(label_pad_top);
        }
        let tags = editor_tags();
        let current = tags
            .get(tag)
            .map(|value| to_text_stringify(&value))
            .unwrap_or_default();
        if multiline {
            // code-instantiated: the description is the only multi-line field.
            let mut area = text_area(&current);
            area.set_custom_minimum_size(Vector2::new(400.0, 140.0));
            let area_for_edit = area.clone();
            let tag_for_edit = String::from(tag);
            let callable = Callable::from_fn("map_info_area", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>().to_string())
                    .unwrap_or_default();
                let text = truncate_chars(&text, max_length);
                if text != area_for_edit.get_text().to_string() {
                    let mut area = area_for_edit.clone();
                    area.set_text(&GString::from(text.as_str()));
                }
                editor_set_tag(&tag_for_edit, &text);
            });
            area.connect("text_changed", &callable);
            let _ = table.bind_mut().add(&area).width(400.0).height(140.0);
        } else {
            let placeholder_text = if placeholder.is_empty() {
                String::new()
            } else {
                kit::t(placeholder).to_string()
            };
            let mut field = factory::field(&placeholder_text);
            field.set_text(&GString::from(current.as_str()));
            field.set_max_length(max_length);
            field.set_custom_minimum_size(Vector2::new(400.0, 55.0));
            let tag_for_edit = String::from(tag);
            let callable = Callable::from_fn("map_info_field", move |args: &[&Variant]| {
                let text = args
                    .first()
                    .map(|value| value.to::<GString>().to_string())
                    .unwrap_or_default();
                editor_set_tag(&tag_for_edit, &text);
            });
            field.connect("text_changed", &callable);
            let _ = table.bind_mut().add(&field).width(400.0).height(55.0);
        }
        table.bind_mut().row();
        let _ = dialog;
    }
}

/// `.marginLeft(10f)`: the icon row starts 10 px in from the button edge.
fn inset_icon_row(button: &mut Gd<Button>, margin: f32) {
    for child in button.get_children().iter_shared() {
        if let Ok(row) = child.clone().try_cast::<HBoxContainer>() {
            let mut row = row;
            row.set_offset(Side::LEFT, margin);
        }
    }
}

/// `MapInfoDialog.setup` routes (`_open_route`): `rules` edits
/// `state.rules` via the Custom Rules dialog; the rest open their dialog.
fn open_map_info_route(dialog: &Gd<MindDialog>, action: &str) {
    if action == "rules" {
        open_map_info_rules(dialog);
        return;
    }
    if action == "map_generate" {
        // From map info the filters are written to the `genfilters` tag
        // (`MapGenerateDialog(false)`), not applied to the editor.
        if let Some(mut generate) = dialog_node("map_generate") {
            generate.set_meta(GEN_APPLIED_META, &false.to_variant());
            let _ = generate.call_deferred("refresh_controller", &[]);
        }
    }
    ui_open_dialog(action, &VarDictionary::new());
    close_dialog(dialog);
}

/// `ruleInfo.show(Vars.state.rules, () -> Vars.state.rules = new Rules())`:
/// the working rules live in the editor's `rules` tag.
fn open_map_info_rules(dialog: &Gd<MindDialog>) {
    let tags = editor_tags();
    let current = tags
        .get("rules")
        .map(|value| to_text_stringify(&value))
        .map(|text| json_object(&GString::from(text.as_str())))
        .unwrap_or_default();
    let reset = crate::ui::kit::campaign()
        .and_then(|mut campaign| {
            if !campaign.has_method("get_rules_json") {
                return None;
            }
            campaign
                .call("get_rules_json", &[])
                .try_to::<GString>()
                .ok()
        })
        .map(|text| json_object(&text))
        .unwrap_or_default();
    let mut context = VarDictionary::new();
    context.set("rules", &current.to_variant());
    context.set("reset", &reset.to_variant());
    context.set("show_rule_edit", &true.to_variant());
    ui_open_dialog("custom_rules", &context);
    connect_rules_edited(dialog);
    close_dialog(dialog);
}

/// Connects `custom_rules.rules_edited(json)` to the `rules` tag.
fn connect_rules_edited(dialog: &Gd<MindDialog>) {
    if dialog.has_meta(MAP_INFO_RULES_META) {
        return;
    }
    let Some(mut rules) = dialog_node("custom_rules") else {
        return;
    };
    if !rules.has_signal("rules_edited") {
        return;
    }
    let callable = Callable::from_fn("map_info_rules_edited", move |args: &[&Variant]| {
        let Some(json) = args.first() else {
            return;
        };
        editor_set_tag("rules", &to_text_stringify(json));
    });
    rules.connect("rules_edited", &callable);
    dialog
        .clone()
        .set_meta(MAP_INFO_RULES_META, &true.to_variant());
}

/// `_close_pressed`: closes through `MindUi` (stack + pause governor).
pub(super) fn close_dialog(dialog: &Gd<MindDialog>) {
    let mut node = dialog.clone();
    let _ = node.call("close_pressed", &[]);
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_info" => Some(Box::new(MapInfoDialog)),
        _ => None,
    }
}
