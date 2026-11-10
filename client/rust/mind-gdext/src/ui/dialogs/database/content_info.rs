// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The content info dialog (`content`): the content stats and description panel.
//! Upstream: core/src/mindustry/ui/dialogs/ContentInfoDialog.java

use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::ExpandMode;
use godot::classes::{
    Control, HBoxContainer, PanelContainer, RichTextLabel, StyleBoxFlat, VBoxContainer,
};
use godot::global::VerticalAlignment;
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::gdutil::variant::{dict_bool_try_or, to_text_stringify};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::{factory, tooltip};
use crate::ui::kit::{self, MindLabel, MindTable, styles};

use crate::ui::dialogs::settings::util::*;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "content" => Some(Box::new(ContentController::default())),
        _ => None,
    }
}

/// `ContentInfoDialog` header icon edge (`title1.image(...).size(iconXLarge)`).
const INFO_HEADER_ICON: f32 = 48.0;
/// Description label width (`description.width(500f)`).
const INFO_DESCRIPTION_WIDTH: f32 = 500.0;
/// Details label width (`details.width(400f)`).
const INFO_DETAIL_WIDTH: f32 = 400.0;
/// The unlockable content types probed by the bundle-only fallback.
const INFO_CONTENT_TYPES: [&str; 8] = [
    "block", "item", "unit", "liquid", "status", "weather", "sector", "planet",
];

/// `@info.title` — the content stats/description panel over the live
/// `MindCampaign.content_info_json(content)` payload, with the `MindAssets`
/// bundle fallback.
#[derive(Default)]
pub struct ContentController {
    table: Option<Gd<MindTable>>,
}

impl DialogController for ContentController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@info.title");
        dialog.should_pause = false;
        // Java `ContentInfoDialog extends BaseDialog` (`setFillParent(true)`).
        dialog.fill_panel();
        dialog.clear_content();
        let mut table = dialog.content_table();
        {
            let mut control: Gd<Control> = table.clone().upcast();
            control.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        }
        // `table.margin(10)` (+ room for the pane's scroll bar).
        table.bind_mut().margin(10.0);
        self.table = Some(table);
        dialog.add_default_close_button();
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let Some(mut table) = self.table.clone() else {
            return;
        };
        table.bind_mut().clear();
        // `ContentInfoDialog.show` rebuilds `cont.pane(...)` per open, so every
        // page starts at the top; the reused body starts at scroll 0.
        if let Some(mut body) = dialog.body() {
            body.set_v_scroll(0);
        }
        let content_name = match dialog.context_value("content") {
            Some(value) => to_text_stringify(&value),
            None => dialog.context_string("name").to_string(),
        };
        if content_name.is_empty() {
            let label = factory::label(&kit::t("@none").to_string());
            table.bind_mut().add(&label).pad(8.0);
            return;
        }
        let mut info = dialog.campaign_json(
            "content_info_json",
            &[GString::from(content_name.as_str()).to_variant()],
        );
        if info.is_empty() {
            info = content_bundle_info(&content_name);
        }
        content_show_header(&mut table, &info, &content_name);
        if dict_bool_try_or(&info, "patched", false) {
            let label = factory::label(&kit::tm("@database.patched").to_string());
            table.bind_mut().add(&label).pad(4.0).left();
            table.bind_mut().row();
        }
        let categories = dict_array(&info, "categories");
        let stats = dict_array(&info, "stats");
        let has_stats = !categories.is_empty() || !stats.is_empty();
        let use_categories = dict_bool_try_or(&info, "use_categories", false);
        let description = dict_text(&info, "description");
        if !description.is_empty() {
            if has_stats {
                // `table.add("@category.purpose").color(Pal.accent).fillX().padTop(10)`
                let accent = content_accent_label(&kit::t("@category.purpose").to_string());
                table.bind_mut().add(&accent).grow_x().pad_top(10.0).left();
                table.bind_mut().row();
            }
            // code-instantiated: description text comes from the live content.
            let description_label =
                factory::label(&kit::markup_text(&format!("[lightgray]{description}")).to_string());
            {
                let mut rich_label: Gd<RichTextLabel> = description_label.clone().upcast();
                rich_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
            }
            {
                let mut control: Gd<Control> = description_label.clone().upcast();
                let mut min = control.get_custom_minimum_size();
                min.x = INFO_DESCRIPTION_WIDTH;
                control.set_custom_minimum_size(min);
            }
            table
                .bind_mut()
                .add(&description_label)
                .grow_x()
                .pad_left(if has_stats { 10.0 } else { 0.0 })
                .left();
            table.bind_mut().row();
            if !use_categories && has_stats {
                let accent = content_accent_label(&kit::t("@category.general").to_string());
                table.bind_mut().add(&accent).grow_x().left();
                table.bind_mut().row();
            }
        }
        let node = dialog.to_gd();
        if !categories.is_empty() {
            content_render_categories(&node, &mut table, &categories, use_categories);
        } else {
            content_render_stats(&node, &mut table, &stats);
        }
        let details = dict_text(&info, "details");
        let hide_details = dict_bool_try_or(&info, "hide_details", false);
        let locked = dict_bool_try_or(&info, "locked", false);
        if !details.is_empty() {
            let mut text = kit::markup_text(&format!("[gray]{details}")).to_string();
            if locked && hide_details {
                // Java `Iconc.lock + " " + Core.bundle.get("unlock.incampaign")`.
                text = format!("[icon name=\"lock\"] {}", kit::tm("@unlock.incampaign"));
            }
            // code-instantiated: details text comes from the live content.
            let detail_label = factory::label(&text);
            {
                let mut rich_label: Gd<RichTextLabel> = detail_label.clone().upcast();
                rich_label.set_autowrap_mode(AutowrapMode::WORD_SMART);
            }
            {
                let mut control: Gd<Control> = detail_label.clone().upcast();
                let mut min = control.get_custom_minimum_size();
                min.x = INFO_DETAIL_WIDTH;
                control.set_custom_minimum_size(min);
            }
            table
                .bind_mut()
                .add(&detail_label)
                .grow_x()
                .pad(6.0)
                .pad_top(20.0)
                .left();
            table.bind_mut().row();
        }
        let credit = dict_text(&info, "credit");
        if !credit.is_empty() {
            let label = factory::label(
                &kit::tm_format("@content.createdby", &[credit.as_str()]).to_string(),
            );
            table.bind_mut().add(&label).pad_top(40.0).left();
            table.bind_mut().row();
        }
    }
}

/// `Dictionary.get(key, fallback)` with GDScript `str()` coercion.
fn dict_text_or(dict: &VarDictionary, key: &str, fallback: &str) -> String {
    match dict.get(key) {
        Some(value) => to_text_stringify(&value),
        None => fallback.to_string(),
    }
}

/// Header row: `title1.image(content.uiIcon).size(iconXLarge)` + `[accent]` name.
fn content_show_header(table: &mut Gd<MindTable>, info: &VarDictionary, content_name: &str) {
    // code-instantiated: the icon/name header is parameterized by the content.
    let mut header = factory::hbox(5);
    let icon_region = dict_text_or(info, "icon", content_name);
    let mut icon = factory::image(&icon_region);
    icon.set_custom_minimum_size(Vector2::new(INFO_HEADER_ICON, INFO_HEADER_ICON));
    icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
    icon.set_mouse_filter(MouseFilter::IGNORE);
    header.add_child(&icon);
    let localized = dict_text_or(info, "localized", content_name);
    // `title1.add("[accent]" + content.localizedName).padLeft(5)`.
    let name_label = factory::label(&kit::markup_text(&format!("[accent]{localized}")).to_string());
    {
        let mut rich_name: Gd<RichTextLabel> = name_label.clone().upcast();
        rich_name.set_autowrap_mode(AutowrapMode::OFF);
        rich_name.set_fit_content(true);
    }
    {
        let mut control: Gd<Control> = name_label.clone().upcast();
        control.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    }
    header.add_child(&name_label);
    // `table.table(title1 -> ...)` has no alignment: the header sits centred.
    table.bind_mut().add(&header).pad(4.0);
    table.bind_mut().row();
}

/// A default-font label tinted `Pal.accent` (`table.add(text).color(Pal.accent)`).
fn content_accent_label(text: &str) -> Gd<MindLabel> {
    let label = factory::label(text);
    {
        let mut control: Gd<Control> = label.clone().upcast();
        control.set_modulate(styles::ACCENT);
    }
    label
}

/// One accent category label, then the inset stat rows (`useCategories`).
fn content_render_categories(
    dialog: &Gd<MindDialog>,
    table: &mut Gd<MindTable>,
    categories: &VarArray,
    use_categories: bool,
) {
    for category_value in categories.iter_shared() {
        let Ok(category) = category_value.try_to::<VarDictionary>() else {
            continue;
        };
        let stats = dict_array(&category, "stats");
        if stats.is_empty() {
            continue;
        }
        if use_categories {
            // `table.add(cat.localized()).color(Pal.accent).fillX()`.
            let accent = content_accent_label(&dict_text(&category, "label"));
            table.bind_mut().add(&accent).grow_x().left();
            table.bind_mut().row();
        }
        for stat in stats.iter_shared() {
            content_render_stat(dialog, table, &stat);
        }
    }
}

/// Legacy flat `[{category,label,value}]` rows (bundle fallback paths).
fn content_render_stats(_dialog: &Gd<MindDialog>, table: &mut Gd<MindTable>, stats: &VarArray) {
    let mut current_category = String::new();
    for stat_value in stats.iter_shared() {
        let Ok(stat) = stat_value.try_to::<VarDictionary>() else {
            continue;
        };
        let category = dict_text(&stat, "category");
        if !category.is_empty() && category != current_category {
            current_category = category.clone();
            let accent = content_accent_label(&content_stat_text(&category));
            table.bind_mut().add(&accent).grow_x().left();
            table.bind_mut().row();
        }
        // code-instantiated: one label/value row per computed stat.
        let mut row = factory::hbox(8);
        let label = factory::label(
            &kit::markup_text(&format!(
                "[lightgray]{}:[] ",
                content_stat_text(&dict_text(&stat, "label"))
            ))
            .to_string(),
        );
        row.add_child(&label);
        let value = factory::label(&dict_text(&stat, "value"));
        row.add_child(&value);
        table.bind_mut().add(&row).grow_x().pad(1.0).left();
        table.bind_mut().row();
    }
}

/// `table.table(inset -> { inset.left(); stats.statInfo(...); ... })`.
fn content_render_stat(dialog: &Gd<MindDialog>, table: &mut Gd<MindTable>, stat_value: &Variant) {
    let Ok(stat) = stat_value.try_to::<VarDictionary>() else {
        return;
    };
    // code-instantiated: one stat row per computed stat (data-driven).
    let mut column = factory::vbox(2);
    // Arc cells have no spacing: the label carries its own trailing space and
    // values are split by the explicit 10 px cells.
    let mut line = factory::hbox(0);
    column.add_child(&line);
    let label = factory::label(
        &kit::markup_text(&format!("[lightgray]{}:[] ", dict_text(&stat, "label"))).to_string(),
    );
    {
        let mut rich_label: Gd<RichTextLabel> = label.clone().upcast();
        rich_label.set_vertical_alignment(VerticalAlignment::TOP);
    }
    if let Some(tooltip_value) = stat.get("tooltip").filter(|value| !value.is_nil()) {
        let provider = kit::t(&format!("@{}", to_text_stringify(&tooltip_value))).to_variant();
        let mut control: Gd<Control> = label.clone().upcast();
        tooltip::attach(&mut control, &provider, false);
    }
    line.add_child(&label);
    let values = dict_array(&stat, "values");
    for (index, value_row) in values.iter_shared().enumerate() {
        if index > 0 {
            line.add_child(&content_spacer(10.0));
        }
        let Ok(cells) = value_row.try_to::<VarArray>() else {
            continue;
        };
        for cell in cells.iter_shared() {
            line = content_add_cell(dialog, &mut column, &line, &cell);
        }
    }
    table
        .bind_mut()
        .add(&column)
        .grow_x()
        .pad(1.0)
        .pad_left(10.0)
        .left();
    table.bind_mut().row();
}

/// Adds one `Cell` and returns the line container the next cell belongs to
/// (`row` cells start a new line).
fn content_add_cell(
    dialog: &Gd<MindDialog>,
    column: &mut Gd<VBoxContainer>,
    line: &Gd<HBoxContainer>,
    cell_value: &Variant,
) -> Gd<HBoxContainer> {
    let Ok(cell) = cell_value.try_to::<VarDictionary>() else {
        return line.clone();
    };
    match dict_text(&cell, "k").as_str() {
        "text" => {
            let label = factory::label(&kit::markup_text(&dict_text(&cell, "t")).to_string());
            let mut line = line.clone();
            line.add_child(&label);
        }
        "space" => {
            let mut line = line.clone();
            line.add_child(&content_spacer(dict_float(&cell, "w", 0.0)));
        }
        "fill" => {
            // `Cell::Fill`: Java `.right().grow()` pushes the rest right.
            let mut fill = content_spacer(0.0);
            fill.set_h_size_flags(SizeFlags::EXPAND_FILL);
            let mut line = line.clone();
            line.add_child(&fill);
        }
        "row" => {
            // code-instantiated: multi-line stat values split into extra lines.
            let next = factory::hbox(0);
            column.add_child(&next);
            return next;
        }
        "icon" => {
            let icon = content_icon_cell(&cell);
            let mut line = line.clone();
            line.add_child(&icon);
        }
        "panel" => {
            let panel = content_panel_cell(dialog, &cell);
            let mut line = line.clone();
            line.add_child(&panel);
        }
        _ => {}
    }
    line.clone()
}

/// `StatValues.stack`: a 32 px content icon with an outline amount and a click
/// through to the item/liquid info dialog. Unit icons (`power`, `liquid`,
/// `waves`) have no atlas region and draw the icon-font glyph instead.
fn content_icon_cell(cell: &VarDictionary) -> Gd<Control> {
    let icon_size = dict_float(cell, "size", 32.0);
    let region = dict_text(cell, "region");
    if kit::icon_texture(&region).is_none() {
        // code-instantiated: Iconc unit glyphs are font glyphs, not atlas regions.
        let mut holder = Control::new_alloc();
        holder.set_custom_minimum_size(Vector2::new(icon_size, icon_size));
        holder.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        let mut glyph = factory::glyph_label(&region, icon_size as i32, Color::WHITE);
        glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        holder.add_child(&glyph);
        return holder;
    }
    // code-instantiated: content icons are data-driven stat cells.
    let mut button = factory::image_button(&region, "cleari");
    button.set_custom_minimum_size(Vector2::new(icon_size, icon_size));
    button.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    let content = dict_text(cell, "content");
    if !content.is_empty() {
        let callable = Callable::from_fn("content_stat_icon", move |_args: &[&Variant]| {
            open_content_info(&content);
        });
        button.connect("pressed", &callable);
    }
    // `Cell::Icon.amount` serializes `None` as JSON null; `str(null)` is the
    // literal "<null>", so read the value and skip null explicitly.
    let amount = match cell.get("amount") {
        Some(value) if !value.is_nil() => to_text_stringify(&value),
        _ => String::new(),
    };
    if !amount.is_empty() {
        let mut label = factory::styled_label(&amount, "outlineLabel");
        label.set_anchors_preset(LayoutPreset::BOTTOM_LEFT);
        label.set_offset(Side::TOP, -20.0);
        label.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&label);
    }
    let control: Gd<Control> = button.upcast();
    control
}

/// `Cell::Panel`: nested cells inside a flat gray box (reconstructor upgrades).
fn content_panel_cell(dialog: &Gd<MindDialog>, cell: &VarDictionary) -> Gd<Control> {
    // code-instantiated: panel cells are data-driven (upgrade pairs).
    let mut panel = PanelContainer::new_alloc();
    panel.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    panel.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut box_ = StyleBoxFlat::new_gd();
    // `Styles.grayPanel = whiteui.tint(Pal.darkestGray)`.
    box_.set_bg_color(Color::from_rgba(0.1, 0.1, 0.1, 1.0));
    box_.set_content_margin(Side::LEFT, 10.0);
    box_.set_content_margin(Side::RIGHT, 10.0);
    box_.set_content_margin(Side::TOP, 10.0);
    box_.set_content_margin(Side::BOTTOM, 10.0);
    panel.add_theme_stylebox_override("panel", &box_);
    // Nested cells may start their own rows (`Cell::Row`): the panel is a VBox
    // of HBox lines so a row cell lands below, not beside.
    let mut inner = factory::vbox(2);
    panel.add_child(&inner);
    let mut line = factory::hbox(4);
    inner.add_child(&line);
    for nested in dict_array(cell, "cells").iter_shared() {
        line = content_add_cell(dialog, &mut inner, &line, &nested);
    }
    let control: Gd<Control> = panel.upcast();
    control
}

/// `inset.add().size(10f)`: a sizing-only cell gap.
fn content_spacer(width: f32) -> Gd<Control> {
    // code-instantiated: cell gaps are data-driven (Java `inset.add().size(10f)`).
    let mut spacer = Control::new_alloc();
    spacer.set_custom_minimum_size(Vector2::new(width, 0.0));
    spacer.set_mouse_filter(MouseFilter::IGNORE);
    spacer
}

/// Resolves a stat/category label: bundle keys (`category.purpose`) resolve
/// through `MindAssets`, plain strings pass through.
fn content_stat_text(value: &str) -> String {
    if value.is_empty() {
        return value.to_string();
    }
    kit::t(&format!("@{value}")).to_string()
}

/// Bundle-only fallback: `<type>.<name>.name|description|details|credit` for
/// the first unlockable type that has the name.
fn content_bundle_info(content_name: &str) -> VarDictionary {
    let mut info = VarDictionary::new();
    info.set("content", &GString::from(content_name));
    info.set("localized", &GString::from(content_name));
    for type_name in INFO_CONTENT_TYPES {
        let name_key = format!("{type_name}.{content_name}.name");
        let localized = kit::t(&name_key).to_string();
        if localized.is_empty() || localized == name_key {
            continue;
        }
        info.set("localized", &GString::from(localized.as_str()));
        info.set("type", &GString::from(type_name));
        for field in ["description", "details", "credit"] {
            let value = content_bundle_value(&format!("{type_name}.{content_name}.{field}"));
            info.set(field, &GString::from(value.as_str()));
        }
        return info;
    }
    info
}

/// `_bundle_value`: the bundle text, empty when the key is absent.
fn content_bundle_value(key: &str) -> String {
    bundle_or(key, "")
}
