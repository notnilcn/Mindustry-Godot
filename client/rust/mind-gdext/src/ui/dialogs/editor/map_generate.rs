// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map generate dialog controller.
//! Upstream: editor/MapGenerateDialog.java

use godot::builtin::Side;
use godot::classes::Button;
use godot::classes::ColorRect;
use godot::classes::Control;
use godot::classes::DisplayServer;
use godot::classes::GridContainer;
use godot::classes::HBoxContainer;
use godot::classes::InputEvent;
use godot::classes::InputEventMouseButton;
use godot::classes::Node;
use godot::classes::PanelContainer;
use godot::classes::Texture2D;
use godot::classes::TextureRect;
use godot::classes::VBoxContainer;
use godot::classes::control::LayoutPreset;
use godot::classes::control::MouseFilter;
use godot::classes::control::SizeFlags;
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::ExpandMode;
use godot::classes::texture_rect::StretchMode;
use godot::global::HorizontalAlignment;
use godot::global::VerticalAlignment;
use godot::obj::NewAlloc;
use godot::prelude::*;

use crate::ui::dialog::DialogController;
use crate::ui::dialog::MindDialog;
use crate::ui::kit;
use crate::ui::kit::MindScroll;
use crate::ui::kit::MindTable;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

use super::util_e::*;

use super::map_info::{GEN_APPLIED_META, close_dialog};

// ---------------------------------------------------------------------------
// MapGenerateDialog (`map_generate`, `MapGenerateDialog.java`)
// ---------------------------------------------------------------------------

/// `MapGenerateDialog.rebuildFilters`: `table(...).width(280f)`.
const GEN_CARD_WIDTH: f32 = 280.0;
/// Header icon buttons (`t.defaults().size(42f)`).
const GEN_HEADER_ICON: f32 = 42.0;
/// `FilterOption.BlockOption` picker: 10 cells per row and a 44 px cell.
const GEN_PICKER_COLUMNS: i32 = 10;
const GEN_PICKER_CELL: f32 = 44.0;
/// `Pal.gray` (`Pal.java:82`) header tint.
const GEN_HEADER_GRAY: Color = Color::from_rgba8(0x45, 0x45, 0x45, 0xff);
/// Working filter stack (`MapGenerateDialog.filters`).
const GEN_FILTERS_META: &str = "mind_gen_filters";
/// `filter_options_json` rows (option descriptors + resolved values).
const GEN_ROWS_META: &str = "mind_gen_rows";
/// Pending standalone logic-code edit target (`pendingCode`).
const GEN_CODE_INDEX_META: &str = "mind_gen_code_index";
const GEN_CODE_FIELD_META: &str = "mind_gen_code_field";
/// Guard so the `logic` `code_edited` signal binds exactly once.
const GEN_CODE_BOUND_META: &str = "mind_gen_code_bound";
/// The one overlay slot of this dialog (`_overlay`).
const GEN_OVERLAY: &str = "GenerateOverlay";
/// Preview/loading node names looked up by the preview callbacks.
const GEN_PREVIEW_NODE: &str = "GeneratePreview";
const GEN_LOADING_NODE: &str = "GenerateLoading";

/// Requests handled by [`MapGenerateDialog::shown`].
const GEN_REQUEST_LIST: &str = "list";
const GEN_REQUEST_PREVIEW: &str = "preview";

/// `@editor.generate` — the filter-card list with per-filter `FilterOption`
/// editors, the live preview and the `@edit`/`@add` menus.
#[derive(Default)]
pub struct MapGenerateDialog {
    /// The filter card column (`filterTable` in the GDScript).
    filter_table: Option<Gd<MindTable>>,
}

impl DialogController for MapGenerateDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@editor.generate");
        dialog.should_pause = false;
        let mut node = dialog.to_gd();
        if !node.has_meta(GEN_APPLIED_META) {
            set_meta_bool(&mut node, GEN_APPLIED_META, true);
        }
        set_meta_int(&mut node, GEN_CODE_INDEX_META, -1);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let request = take_request(dialog);
        self.build_buttons(dialog);
        match request.as_str() {
            GEN_REQUEST_LIST => {
                self.rebuild_filters(dialog);
                self.refresh_preview(dialog);
            }
            GEN_REQUEST_PREVIEW => self.refresh_preview(dialog),
            // `shown(this::setup)`: read the tag filters, then build preview + cards.
            _ => self.setup(dialog),
        }
    }

    fn hidden(&mut self, dialog: &mut MindDialog) {
        close_overlay(dialog, GEN_OVERLAY);
        // `MapInfoDialog.java:93-96`: reset the seed to 0 so it is not written.
        let node = dialog.to_gd();
        if gen_applied(&node) || kit::editor().is_none() {
            return;
        }
        let mut filters = meta_array(&node, GEN_FILTERS_META);
        for index in 0..filters.len() {
            let Ok(mut filter) = filters.at(index).try_to::<VarDictionary>() else {
                continue;
            };
            filter.set("seed", &0i64.to_variant());
            filters.set(index, &filter.to_variant());
        }
        gen_write(&node, &filters);
    }
}

impl MapGenerateDialog {
    /// `buttons.button("@back"…, "@editor.apply"…, randomize/edit/add)`.
    fn build_buttons(&mut self, dialog: &mut MindDialog) {
        dialog.clear_buttons();
        let target = dialog.to_gd();
        let back = kit::t("@back").to_string();
        dialog.add_button(
            &back,
            Some(dialog.to_gd().callable("close_pressed")),
            "left",
            180.0,
        );
        if gen_applied(&target) {
            let text = kit::t("@editor.apply").to_string();
            let apply_target = target.clone();
            let callable = Callable::from_fn("map_gen_apply", move |_args: &[&Variant]| {
                let apply_target = apply_target.clone();
                let run = Callable::from_fn("map_gen_apply_now", move |_args: &[&Variant]| {
                    gen_apply_now(&apply_target);
                });
                ui_root_load_and(run);
            });
            dialog.add_button(&text, Some(callable), "ok", 180.0);
        }
        let randomize = kit::t("@editor.randomize").to_string();
        let randomize_target = target.clone();
        let callable = Callable::from_fn("map_gen_randomize", move |_args: &[&Variant]| {
            gen_randomize_all(&randomize_target);
        });
        dialog.add_button(&randomize, Some(callable), "refresh", 180.0);
        let edit = kit::t("@edit").to_string();
        let edit_target = target.clone();
        let callable = Callable::from_fn("map_gen_edit", move |_args: &[&Variant]| {
            gen_show_edit_menu(&edit_target);
        });
        dialog.add_button(&edit, Some(callable), "edit", 180.0);
        let add = kit::t("@add").to_string();
        let add_target = target;
        let callable = Callable::from_fn("map_gen_add", move |_args: &[&Variant]| {
            gen_show_add_menu(&add_target);
        });
        dialog.add_button(&add, Some(callable), "add", 180.0);
    }

    /// `shown(this::setup)`.
    fn setup(&mut self, dialog: &mut MindDialog) {
        if kit::editor().is_none() {
            return;
        }
        let node = dialog.to_gd();
        let rows = gen_read_rows();
        let mut filters = gen_read_filters(&rows);
        if !gen_applied(&node) {
            // `MapInfoDialog.java:88-90`: randomize on open so the filters are
            // not all the same seed.
            for index in 0..filters.len() {
                let Ok(mut filter) = filters.at(index).try_to::<VarDictionary>() else {
                    continue;
                };
                filter.set("seed", &random_seed().to_variant());
                filters.set(index, &filter.to_variant());
            }
            gen_write(&node, &filters);
        }
        set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
        set_meta_array(&mut node.clone(), GEN_FILTERS_META, &filters);
        self.build(dialog);
        self.refresh_preview(dialog);
    }

    /// `setup()`: `cont.table(t -> { preview | filter pane }).grow()`.
    fn build(&mut self, dialog: &mut MindDialog) {
        if dialog.content().is_none() {
            return;
        }
        dialog.clear_content();
        let mut root = dialog.content_table();
        let mut columns = HBoxContainer::new_alloc();
        columns.set_h_size_flags(SizeFlags::EXPAND_FILL);
        columns.set_v_size_flags(SizeFlags::EXPAND_FILL);
        columns.add_theme_constant_override("separation", 10);
        root.bind_mut().add(&columns).grow_x().grow_y();
        columns.add_child(&build_generate_preview());
        columns.add_child(&build_generate_filter_list(&mut self.filter_table));
        self.rebuild_filters(dialog);
    }

    /// `rebuildFilters()`: one `Tex.pane` card per filter (or `@filters.empty`).
    fn rebuild_filters(&mut self, dialog: &mut MindDialog) {
        let Some(mut list) = self.filter_table.clone() else {
            return;
        };
        list.bind_mut().clear();
        let node = dialog.to_gd();
        let filters = meta_array(&node, GEN_FILTERS_META);
        let rows = meta_array(&node, GEN_ROWS_META);
        if filters.is_empty() {
            // The bundle text carries `[lightgray]` Arc markup: translate it.
            let mut empty = factory::label(&kit::tm("@filters.empty").to_string());
            empty.set_custom_minimum_size(Vector2::new(200.0, 0.0));
            let _ = list.bind_mut().add(&empty).pad(4.0);
            list.bind_mut().row();
            return;
        }
        for index in 0..filters.len() {
            let card = build_generate_card(&node, index as i64, &rows);
            let _ = list.bind_mut().add(&card).pad(2.0);
            list.bind_mut().row();
        }
    }

    /// `update()`: regenerate the preview from the editor grid + tag filters.
    fn refresh_preview(&mut self, dialog: &mut MindDialog) {
        let node = dialog.to_gd();
        gen_refresh_preview(&node);
    }
}

/// The left preview column (`t.stack(preview, ...).uniformX().grow()`).
fn build_generate_preview() -> Gd<Control> {
    // code-instantiated: preview box + loading stack.
    let mut box_ = Control::new_alloc();
    box_.set_h_size_flags(SizeFlags::EXPAND_FILL);
    box_.set_v_size_flags(SizeFlags::EXPAND_FILL);
    box_.set_custom_minimum_size(Vector2::new(320.0, 320.0));
    box_.set_clip_contents(true);
    // code-instantiated: the preview texture is produced by the editor.
    let mut preview = TextureRect::new_alloc();
    preview.set_name(GEN_PREVIEW_NODE);
    preview.set_expand_mode(ExpandMode::IGNORE_SIZE);
    preview.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
    preview.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    preview.set_mouse_filter(MouseFilter::IGNORE);
    box_.add_child(&preview);
    // `t.stack(preview, new Stack(){ black8 + Icon.refresh })`: the loading
    // overlay (`generating && !updateEditorOnChange && dragging == null`).
    let mut loading = Control::new_alloc();
    loading.set_name(GEN_LOADING_NODE);
    loading.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    loading.set_mouse_filter(MouseFilter::IGNORE);
    let mut dim = ColorRect::new_alloc();
    dim.set_color(Color::from_rgba(0.0, 0.0, 0.0, 0.8));
    dim.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    dim.set_mouse_filter(MouseFilter::IGNORE);
    loading.add_child(&dim);
    let mut spinner = factory::glyph_label("refresh", 48, Color::WHITE);
    spinner.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    spinner.set_horizontal_alignment(HorizontalAlignment::CENTER);
    spinner.set_vertical_alignment(VerticalAlignment::CENTER);
    spinner.set_mouse_filter(MouseFilter::IGNORE);
    loading.add_child(&spinner);
    loading.set_visible(false);
    box_.add_child(&loading);
    box_
}

/// The right filter pane (`t.pane(p -> filterTable = ...).scrollX(false)`).
fn build_generate_filter_list(filter_table: &mut Option<Gd<MindTable>>) -> Gd<Control> {
    // code-instantiated: the card list count is the filter stack size.
    let mut scroll = MindScroll::new_alloc();
    scroll.bind_mut().set_scrolling_disabled(true, false);
    let mut control = as_control(&scroll);
    control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    control.set_v_size_flags(SizeFlags::EXPAND_FILL);
    control.set_custom_minimum_size(Vector2::new(GEN_CARD_WIDTH + 40.0, 320.0));
    let table = scroll.bind_mut().content();
    *filter_table = Some(table);
    control
}

/// One filter card: the gray header row and the option rows.
fn build_generate_card(node: &Gd<MindDialog>, index: i64, rows: &VarArray) -> Gd<Control> {
    let row = rows
        .get(index.max(0) as usize)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default();
    // code-instantiated: one card per filter; the stack is runtime-sized.
    let mut card = PanelContainer::new_alloc();
    card.add_theme_stylebox_override(
        "panel",
        &pane_frame(Color::BLACK, styles::PANE_OUTLINE_GRAY, 3.0),
    );
    card.set_custom_minimum_size(Vector2::new(GEN_CARD_WIDTH, 0.0));
    card.set_h_size_flags(SizeFlags::FILL);
    let mut body = VBoxContainer::new_alloc();
    body.add_theme_constant_override("separation", 0);
    card.add_child(&body);
    body.add_child(&build_generate_card_header(node, index, &row));
    let mut options = VBoxContainer::new_alloc();
    options.add_theme_constant_override("separation", 2);
    for option in dict_array(&row, "options").iter_shared() {
        let Ok(option) = option.try_to::<VarDictionary>() else {
            continue;
        };
        options.add_child(&build_generate_option(node, index, &option, rows));
    }
    body.add_child(&options);
    card.upcast()
}

/// `c.table(Tex.whiteui, t -> { name + refresh/up/down/copy/remove })`.
fn build_generate_card_header(
    node: &Gd<MindDialog>,
    index: i64,
    row: &VarDictionary,
) -> Gd<Control> {
    let mut header = PanelContainer::new_alloc();
    let mut style = flat_box(GEN_HEADER_GRAY);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        style.set_content_margin(
            side,
            if side == Side::TOP || side == Side::BOTTOM {
                2.0
            } else {
                4.0
            },
        );
    }
    header.add_theme_stylebox_override("panel", &style);
    // code-instantiated: header buttons are per-filter action rows.
    let mut line = HBoxContainer::new_alloc();
    line.add_theme_constant_override("separation", 0);
    header.add_child(&line);
    let filter_name = dict_text(row, "name");
    let mut label = factory::label(&kit::t(&format!("@filter.{filter_name}")).to_string());
    label.set_custom_minimum_size(Vector2::new(100.0, 0.0));
    rich(&label).set_autowrap_mode(AutowrapMode::WORD_SMART);
    line.add_child(&as_control(&label));
    let mut spacer = factory::space(0.0, 0.0);
    spacer.set_h_size_flags(SizeFlags::EXPAND_FILL);
    line.add_child(&spacer);
    line.add_child(&card_icon(
        node,
        index,
        "refresh",
        "@editor.randomize",
        GenAction::Randomize,
    ));
    if index > 0 {
        line.add_child(&card_icon(
            node,
            index,
            "up-open",
            "@editor.moveup",
            GenAction::Move(-1),
        ));
    }
    let count = meta_array(node, GEN_FILTERS_META).len() as i64;
    if index < count - 1 {
        line.add_child(&card_icon(
            node,
            index,
            "down-open",
            "@editor.movedown",
            GenAction::Move(1),
        ));
    }
    line.add_child(&card_icon(
        node,
        index,
        "copy",
        "@editor.copy",
        GenAction::Copy,
    ));
    line.add_child(&card_icon(
        node,
        index,
        "cancel",
        "@waves.remove",
        GenAction::Remove,
    ));
    header.upcast()
}

/// Per-card header actions.
#[derive(Clone, Copy)]
enum GenAction {
    Randomize,
    Move(i64),
    Copy,
    Remove,
}

/// One 42 px header icon button (`t.defaults().size(42f).padLeft(-5f)`).
fn card_icon(
    node: &Gd<MindDialog>,
    index: i64,
    icon_name: &str,
    tooltip_key: &str,
    action: GenAction,
) -> Gd<Button> {
    // code-instantiated: per-filter header actions are data-driven.
    let mut button = factory::glyph_button(icon_name, icon_name, "clearNonei");
    button.set_custom_minimum_size(Vector2::new(GEN_HEADER_ICON, GEN_HEADER_ICON));
    button.set_h_size_flags(SizeFlags::SHRINK_END);
    button.set_tooltip_text(&kit::t(tooltip_key));
    let target = node.clone();
    let callable = Callable::from_fn(
        "map_gen_card_action",
        move |_args: &[&Variant]| match action {
            GenAction::Randomize => gen_randomize_index(&target, index),
            GenAction::Move(direction) => gen_move(&target, index, direction),
            GenAction::Copy => gen_copy(&target, index),
            GenAction::Remove => gen_remove(&target, index),
        },
    );
    button.connect("pressed", &callable);
    button
}

/// `FilterOption.build` dispatch: slider / block / toggle / custom.
fn build_generate_option(
    node: &Gd<MindDialog>,
    index: i64,
    option: &VarDictionary,
    rows: &VarArray,
) -> Gd<Control> {
    match dict_text(option, "kind").as_str() {
        "slider" => gen_slider_row(node, index, option, rows),
        "block" => gen_block_row(node, index, option, rows),
        "toggle" => gen_toggle_row(node, index, option, rows),
        "custom" => gen_custom_row(node, index, option),
        _ => factory::label("?").upcast(),
    }
}

/// The resolved value behind one option field (`_value_of`).
fn gen_option_value(index: i64, field: &str, option: &VarDictionary, rows: &VarArray) -> Variant {
    if let Some(row) = rows
        .get(index.max(0) as usize)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
    {
        let values = dict_dict(&row, "values");
        if let Some(value) = values.get(field) {
            return value;
        }
    }
    let _ = option;
    Variant::nil()
}

fn gen_option_float(index: i64, field: &str, option: &VarDictionary, rows: &VarArray) -> f64 {
    as_float(Some(gen_option_value(index, field, option, rows)))
}

fn gen_option_string(index: i64, field: &str, option: &VarDictionary, rows: &VarArray) -> String {
    let value = gen_option_value(index, field, option, rows);
    if value.get_type() == VariantType::NIL {
        String::new()
    } else {
        variant_text(&value)
    }
}

fn gen_option_bool(index: i64, field: &str, option: &VarDictionary, rows: &VarArray) -> bool {
    as_bool(Some(gen_option_value(index, field, option, rows)))
}

/// `SliderOption.build`: the label/value row stacked on the slider; the change
/// fires on release (`updateEditorOnChange` is false).
fn gen_slider_row(
    node: &Gd<MindDialog>,
    index: i64,
    option: &VarDictionary,
    rows: &VarArray,
) -> Gd<Control> {
    let field = dict_text(option, "field");
    let value = gen_option_float(index, &field, option, rows);
    let integer = dict_bool(option, "integer");
    // code-instantiated: one slider cell per descriptor (runtime row).
    let mut holder = Control::new_alloc();
    holder.set_custom_minimum_size(Vector2::new(0.0, 46.0));
    holder.set_h_size_flags(SizeFlags::EXPAND_FILL);
    let mut slider = factory::slider(
        dict_float(option, "min", 0.0) as f64,
        dict_float(option, "max", 1.0) as f64,
        dict_float(option, "step", 0.00001) as f64,
    );
    slider.set_name((format!("FilterSlider_{field}")).as_str());
    slider.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    slider.set_offset(Side::TOP, 10.0);
    slider.set_offset(Side::BOTTOM, -10.0);
    slider.set_value(value);
    holder.add_child(&slider);
    let mut overlay = HBoxContainer::new_alloc();
    overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    overlay.set_mouse_filter(MouseFilter::IGNORE);
    let mut label = factory::label(
        &kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string(),
    );
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&as_control(&label));
    let mut readout = factory::label(&auto_fixed(value));
    readout.set_vertical_alignment(VerticalAlignment::CENTER);
    readout.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&as_control(&readout));
    holder.add_child(&overlay);
    let readout_for_change = readout.clone();
    let changed = Callable::from_fn("map_gen_slider_value", move |args: &[&Variant]| {
        let value = as_float(args.first().map(|value| (*value).clone()));
        rich(&readout_for_change).set_text(&GString::from(auto_fixed(value).as_str()));
    });
    slider.connect("value_changed", &changed);
    let target = node.clone();
    let field_for_drag = field.clone();
    let slider_for_drag = slider.clone();
    let drag = Callable::from_fn("map_gen_slider_drag", move |args: &[&Variant]| {
        let changed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        if !changed {
            return;
        }
        let slider = slider_for_drag.clone();
        let value = slider.get_value();
        gen_commit_field(&target, index, &field_for_drag, value.to_variant(), integer);
    });
    slider.connect("drag_ended", &drag);
    holder.upcast()
}

/// `BlockOption.build`: the icon button (click -> picker) + the option label.
fn gen_block_row(
    node: &Gd<MindDialog>,
    index: i64,
    option: &VarDictionary,
    rows: &VarArray,
) -> Gd<Control> {
    let field = dict_text(option, "field");
    let block_name = gen_option_string(index, &field, option, rows);
    let predicate = dict_text(option, "predicate");
    // code-instantiated: the block cell is data-driven (filter option value).
    let mut line = HBoxContainer::new_alloc();
    line.add_theme_constant_override("separation", 6);
    let mut button = block_button(&block_name);
    button.set_name((format!("BlockOption_{field}")).as_str());
    button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
    let target = node.clone();
    let option_for_press = option.clone();
    let press = Callable::from_fn("map_gen_block_picker", move |_args: &[&Variant]| {
        gen_show_block_picker(&target, index, &option_for_press);
    });
    button.connect("pressed", &press);
    let target = node.clone();
    let field_for_click = field.clone();
    let block_for_copy = block_name.clone();
    let predicate_for_paste = predicate;
    let gui_input = Callable::from_fn("map_gen_block_clicks", move |args: &[&Variant]| {
        let Some(event) = args.first() else {
            return;
        };
        let Ok(event) = event.try_to::<Gd<InputEvent>>() else {
            return;
        };
        let Ok(mouse) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !mouse.is_pressed() {
            return;
        }
        match mouse.get_button_index() {
            godot::global::MouseButton::MIDDLE => {
                // `button.clicked(KeyCode.mouseMiddle, ...)`: copy the block name.
                DisplayServer::singleton().clipboard_set(&GString::from(block_for_copy.as_str()));
                ui_toast(&kit::t("@copied"), "info");
            }
            godot::global::MouseButton::RIGHT => {
                // `button.clicked(KeyCode.mouseRight, ...)`: paste a valid block.
                let pasted = DisplayServer::singleton()
                    .clipboard_get()
                    .to_string()
                    .trim()
                    .to_string();
                if gen_valid_block(&predicate_for_paste, &pasted) {
                    gen_commit_field(
                        &target,
                        index,
                        &field_for_click,
                        GString::from(pasted.as_str()).to_variant(),
                        false,
                    );
                    request_refresh(&target, GEN_REQUEST_LIST);
                }
            }
            _ => {}
        }
    });
    button.connect("gui_input", &gui_input);
    line.add_child(&button);
    let mut label = factory::label(
        &kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string(),
    );
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    line.add_child(&as_control(&label));
    line.upcast()
}

/// `ToggleOption.build`: the centered check row.
fn gen_toggle_row(
    node: &Gd<MindDialog>,
    index: i64,
    option: &VarDictionary,
    rows: &VarArray,
) -> Gd<Control> {
    let field = dict_text(option, "field");
    let value = gen_option_bool(index, &field, option, rows);
    let text = kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string();
    let target = node.clone();
    let field_for_toggle = field.clone();
    let listener = Callable::from_fn("map_gen_toggle", move |args: &[&Variant]| {
        let pressed = args
            .first()
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        gen_commit_field(
            &target,
            index,
            &field_for_toggle,
            pressed.to_variant(),
            false,
        );
    });
    let mut check = mind_check(&text, value, listener);
    check.set_name((format!("FilterToggle_{field}")).as_str());
    check.set_custom_minimum_size(Vector2::new(0.0, 40.0));
    check.upcast()
}

/// The `LogicFilter` code option: `Icon.pencil` button + the option label.
fn gen_custom_row(node: &Gd<MindDialog>, index: i64, option: &VarDictionary) -> Gd<Control> {
    // code-instantiated: custom actions are descriptor-driven (logic code).
    let mut line = HBoxContainer::new_alloc();
    line.add_theme_constant_override("separation", 6);
    let action = dict_text(option, "action");
    let mut button = factory::glyph_button("pencil", &kit::t("@edit").to_string(), "clearNonei");
    button.set_name((format!("CustomOption_{action}")).as_str());
    button.set_custom_minimum_size(Vector2::new(48.0, 48.0));
    let target = node.clone();
    let action_for_press = action.clone();
    let field = dict_text(option, "field");
    let press = Callable::from_fn("map_gen_custom", move |_args: &[&Variant]| {
        gen_open_custom_action(&target, &action_for_press, index, &field);
    });
    button.connect("pressed", &press);
    line.add_child(&button);
    let mut label = factory::label(
        &kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string(),
    );
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    line.add_child(&as_control(&label));
    line.upcast()
}

/// The current value's `uiIcon` button (`b.image(supplier.get().uiIcon)`).
fn block_button(block_name: &str) -> Gd<Button> {
    if block_name == "air" {
        let mut air = factory::glyph_button("cancel", &kit::t("@none").to_string(), "clearNonei");
        air.set_expand_icon(true);
        return air;
    }
    let ui_region = format!("block-{block_name}-ui");
    let full_region = format!("block-{block_name}-full");
    let mut button =
        factory::image_button_first(&[ui_region.as_str(), full_region.as_str()], "clearNonei");
    if button.get_button_icon().is_none() {
        button.set_text(&GString::from(block_name));
    }
    button.set_expand_icon(true);
    button.add_theme_constant_override("icon_max_width", 36);
    button.set_tooltip_text(&kit::t(&format!("@block.{block_name}.name")));
    button
}

/// `Strings.autoFixed(value, 2)`.
fn auto_fixed(value: f64) -> String {
    format!("{value:.2}")
}

/// `randi() % 999999999` (`_random_seed`).
fn random_seed() -> i64 {
    godot::global::randi() % 999_999_999
}

/// `MapGenerateDialog.filters`: the tag stack, or the class defaults.
fn gen_read_filters(rows: &VarArray) -> VarArray {
    if let Some(text) = editor_json("filters_json", &[]) {
        let parsed = json_value(&text);
        if let Ok(array) = parsed.try_to::<VarArray>() {
            return array;
        }
    }
    let mut out = VarArray::new();
    for row in dict_rows(rows) {
        let values = dict_dict(&row, "values");
        if !values.is_empty() {
            out.push(&values.duplicate_deep().to_variant());
        }
    }
    out
}

/// `filter_options_json` rows.
fn gen_read_rows() -> VarArray {
    editor_json("filter_options_json", &[])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// `_read_types()` (`Maps.allFilterTypes` rows for the add picker).
fn gen_read_types() -> VarArray {
    editor_json("filter_types_json", &[])
        .map(|text| json_array(&text))
        .unwrap_or_default()
}

/// `_write(filters)`: the `set_filters_json` tag write.
fn gen_write(node: &Gd<MindDialog>, filters: &VarArray) {
    set_meta_array(&mut node.clone(), GEN_FILTERS_META, filters);
    let _ = editor_write("set_filters_json", &json_string(&filters.to_variant()));
}

/// The working filters from metadata.
fn gen_filters(node: &Gd<MindDialog>) -> VarArray {
    meta_array(node, GEN_FILTERS_META)
}

/// `_commit_field`: one value into the working filters + the mirror rows.
fn gen_commit_field(node: &Gd<MindDialog>, index: i64, field: &str, value: Variant, integer: bool) {
    let mut filters = gen_filters(node);
    if index < 0 || index >= filters.len() as i64 {
        return;
    }
    let Ok(mut filter) = filters.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    // `if(integer && value instanceof Float) value = Mathf.round(value)`.
    let mut stored = value.clone();
    if integer && let Ok(number) = value.try_to::<f64>() {
        stored = (number.round() as i64).to_variant();
    }
    filter.set(field, &stored);
    filters.set(index as usize, &filter.to_variant());
    let mut rows = meta_array(node, GEN_ROWS_META);
    if index < rows.len() as i64
        && let Ok(mut row) = rows.at(index as usize).try_to::<VarDictionary>()
    {
        let mut values = dict_dict(&row, "values");
        values.set(field, &stored);
        row.set("values", &values.to_variant());
        rows.set(index as usize, &row.to_variant());
        set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    }
    gen_write(node, &filters);
    gen_refresh_preview(node);
}

/// The `@editor.randomize` toolbar button: `for(filter : filters) filter.randomize()`.
fn gen_randomize_all(node: &Gd<MindDialog>) {
    let mut filters = gen_filters(node);
    for index in 0..filters.len() {
        let Ok(mut filter) = filters.at(index).try_to::<VarDictionary>() else {
            continue;
        };
        filter.set("seed", &random_seed().to_variant());
        filters.set(index, &filter.to_variant());
    }
    gen_write(node, &filters);
    gen_refresh_preview(node);
}

/// The per-card refresh (`filter.randomize(); update()`).
fn gen_randomize_index(node: &Gd<MindDialog>, index: i64) {
    let mut filters = gen_filters(node);
    if index < 0 || index >= filters.len() as i64 {
        return;
    }
    let Ok(mut filter) = filters.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    filter.set("seed", &random_seed().to_variant());
    filters.set(index as usize, &filter.to_variant());
    gen_write(node, &filters);
    gen_refresh_preview(node);
}

/// The per-card up/down swap (`filters.swap(idx, idx +/- 1)`).
fn gen_move(node: &Gd<MindDialog>, index: i64, direction: i64) {
    let other = index + direction;
    let mut filters = gen_filters(node);
    let count = filters.len() as i64;
    if index < 0 || other < 0 || index >= count || other >= count {
        return;
    }
    let first = filters.at(index as usize);
    let second = filters.at(other as usize);
    filters.set(index as usize, &second);
    filters.set(other as usize, &first);
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// The per-card copy (`copy.randomize(); filters.insert(idx + 1, copy)`).
fn gen_copy(node: &Gd<MindDialog>, index: i64) {
    let mut filters = gen_filters(node);
    if index < 0 || index >= filters.len() as i64 {
        return;
    }
    let Ok(mut copy) = filters.at(index as usize).try_to::<VarDictionary>() else {
        return;
    };
    copy.set("seed", &random_seed().to_variant());
    filters.insert(index as usize + 1, &copy.to_variant());
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// The per-card remove (`filters.remove(filter)`).
fn gen_remove(node: &Gd<MindDialog>, index: i64) {
    let mut filters = gen_filters(node);
    if index < 0 || index >= filters.len() as i64 {
        return;
    }
    filters.remove(index as usize);
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// `buttons.button("@editor.apply", ...)`: write the tag, then `apply_filters()`.
fn gen_apply_now(node: &Gd<MindDialog>) {
    if kit::editor().is_none() {
        return;
    }
    let filters = gen_filters(node);
    gen_write(node, &filters);
    let _ = editor_call("apply_filters", &[]);
    close_dialog(node);
}

/// `_refresh_preview()`: regenerate the preview from the editor grid + filters.
fn gen_refresh_preview(node: &Gd<MindDialog>) {
    let Some(mut preview) =
        find_named(node, GEN_PREVIEW_NODE).and_then(|child| child.try_cast::<TextureRect>().ok())
    else {
        return;
    };
    let Some(texture) = editor_call("generate_preview_texture", &[])
        .and_then(|value| value.try_to::<Gd<Texture2D>>().ok())
    else {
        return;
    };
    preview.set_texture(&texture);
    if let Some(mut loading) =
        find_named(node, GEN_LOADING_NODE).and_then(|child| child.try_cast::<Control>().ok())
    {
        loading.set_visible(false);
    }
}

/// `showAdd()`: every `Maps.allFilterTypes` entry (post filters hidden in
/// applied mode) plus the `@filter.defaultores` row.
fn gen_show_add_menu(node: &Gd<MindDialog>) {
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    let applied = gen_applied(node);
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(3);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    for row in dict_rows(&gen_read_types()) {
        if dict_bool(&row, "post") && applied {
            continue;
        }
        let mut button = icon_text_button(
            &dict_text(&row, "icon"),
            &kit::t(&format!("@filter.{}", dict_text(&row, "name"))).to_string(),
            "",
        );
        button.set_custom_minimum_size(Vector2::new(195.0, 56.0));
        let target = node.clone();
        let class_tag = dict_text(&row, "class");
        let callable = Callable::from_fn("map_gen_add_filter", move |_args: &[&Variant]| {
            gen_add_filter(&target, &class_tag);
        });
        button.connect("pressed", &callable);
        grid.add_child(&button);
    }
    let mut default_ores =
        icon_text_button("", &kit::t("@filter.defaultores").to_string(), "refresh");
    default_ores.set_custom_minimum_size(Vector2::new(195.0, 56.0));
    let target = node.clone();
    let callable = Callable::from_fn("map_gen_default_ores", move |_args: &[&Variant]| {
        gen_add_default_ores(&target);
    });
    default_ores.connect("pressed", &callable);
    grid.add_child(&default_ores);
    let title = kit::t("@add").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        GEN_OVERLAY,
        &title,
        640.0,
        380.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.7),
    ) {
        overlay.add_content(&grid);
    }
}

/// `showAdd()`'s `filter.randomize(); filters.add(filter);`.
fn gen_add_filter(node: &Gd<MindDialog>, class_tag: &str) {
    if class_tag.is_empty() {
        return;
    }
    let mut filters = gen_filters(node);
    let mut filter = VarDictionary::new();
    filter.set("class", &GString::from(class_tag).to_variant());
    filter.set("seed", &random_seed().to_variant());
    filters.push(&filter);
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    free_named(node, GEN_OVERLAY);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// `@filter.defaultores`: `maps.addDefaultOres(filters)`.
fn gen_add_default_ores(node: &Gd<MindDialog>) {
    if kit::editor().is_none() {
        return;
    }
    let Some(text) = editor_json("default_ores_json", &[]) else {
        return;
    };
    let parsed = json_value(&text);
    let Ok(extra) = parsed.try_to::<VarArray>() else {
        return;
    };
    let mut filters = gen_filters(node);
    for value in extra.iter_shared() {
        filters.push(&value);
    }
    gen_write(node, &filters);
    let rows = gen_read_rows();
    set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
    free_named(node, GEN_OVERLAY);
    request_refresh(node, GEN_REQUEST_LIST);
}

/// `buttons.button("@edit", ...)`: the clipboard/clear/reset rows.
fn gen_show_edit_menu(node: &Gd<MindDialog>) {
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    // code-instantiated: menu rows depend on the clipboard/applied flags.
    let mut rows = VBoxContainer::new_alloc();
    rows.set_custom_minimum_size(Vector2::new(320.0, 0.0));
    let clipboard_empty = DisplayServer::singleton()
        .clipboard_get()
        .to_string()
        .trim()
        .is_empty();
    gen_menu_button(
        node,
        &mut rows,
        "copy.clipboard",
        "copy",
        GenMenu::Copy,
        false,
    );
    gen_menu_button(
        node,
        &mut rows,
        "load.clipboard",
        "download",
        GenMenu::Load,
        clipboard_empty,
    );
    gen_menu_button(node, &mut rows, "clear", "cancel", GenMenu::Clear, false);
    let mut count = 3.0;
    if !gen_applied(node) {
        gen_menu_button(
            node,
            &mut rows,
            "settings.reset",
            "refresh",
            GenMenu::Reset,
            false,
        );
        count += 1.0;
    }
    let title = kit::t("@editor.export").to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        GEN_OVERLAY,
        &title,
        320.0,
        count * 60.0 + 4.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.7),
    ) {
        overlay.add_content(&rows);
    }
}

/// The `@edit` menu actions.
#[derive(Clone, Copy)]
enum GenMenu {
    Copy,
    Load,
    Clear,
    Reset,
}

/// One `@edit` menu row.
fn gen_menu_button(
    node: &Gd<MindDialog>,
    rows: &mut Gd<VBoxContainer>,
    key: &str,
    icon: &str,
    action: GenMenu,
    disabled: bool,
) {
    let mut button = factory::icon_button(
        icon,
        &kit::t(&format!("@{key}")).to_string(),
        "flatt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    button.set_custom_minimum_size(Vector2::new(280.0, 60.0));
    button.set_disabled(disabled);
    let target = node.clone();
    let callable = Callable::from_fn("map_gen_menu", move |_args: &[&Variant]| {
        gen_menu_action(&target, action);
    });
    button.connect("pressed", &callable);
    rows.add_child(&button);
}

/// Runs one `@edit` menu action.
fn gen_menu_action(node: &Gd<MindDialog>, action: GenMenu) {
    match action {
        GenMenu::Copy => {
            // `Core.app.setClipboardText(JsonIO.write(filters))`.
            let filters = gen_filters(node);
            DisplayServer::singleton().clipboard_set(&json_string(&filters.to_variant()));
            free_named(node, GEN_OVERLAY);
            ui_toast(&kit::t("@copied"), "info");
        }
        GenMenu::Load => {
            // `filters.set(JsonIO.read(Seq.class, Core.app.getClipboardText()))`.
            let text = DisplayServer::singleton().clipboard_get();
            let parsed = json_value(&text);
            let Ok(filters) = parsed.try_to::<VarArray>() else {
                free_named(node, GEN_OVERLAY);
                ui_toast(&kit::t("@save.import.invalid"), "error");
                return;
            };
            gen_write(node, &filters);
            let rows = gen_read_rows();
            set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
            free_named(node, GEN_OVERLAY);
            request_refresh(node, GEN_REQUEST_LIST);
        }
        GenMenu::Clear => {
            let filters = VarArray::new();
            gen_write(node, &filters);
            set_meta_array(&mut node.clone(), GEN_ROWS_META, &VarArray::new());
            free_named(node, GEN_OVERLAY);
            request_refresh(node, GEN_REQUEST_LIST);
        }
        GenMenu::Reset => {
            // `filters.set(maps.readFilters(""))`.
            let _ = editor_write("set_filters_json", &GString::new());
            let rows = gen_read_rows();
            let filters = gen_read_filters(&rows);
            set_meta_array(&mut node.clone(), GEN_ROWS_META, &rows);
            set_meta_array(&mut node.clone(), GEN_FILTERS_META, &filters);
            free_named(node, GEN_OVERLAY);
            request_refresh(node, GEN_REQUEST_LIST);
        }
    }
}

/// `FilterOption.BlockOption` picker: the block grid filtered by the predicate.
fn gen_show_block_picker(node: &Gd<MindDialog>, index: i64, option: &VarDictionary) {
    let predicate = dict_text(option, "predicate");
    let field = dict_text(option, "field");
    let root = node.clone();
    let Ok(mut dialog) = root.try_cast::<MindDialog>() else {
        return;
    };
    // code-instantiated: picker cells are content rows.
    let mut grid = GridContainer::new_alloc();
    grid.set_columns(GEN_PICKER_COLUMNS);
    grid.add_theme_constant_override("h_separation", 4);
    grid.add_theme_constant_override("v_separation", 4);
    for row in dict_rows(&editor_array_arg("filter_blocks_json", &predicate)) {
        let block_name = dict_text(&row, "name");
        let mut cell = block_button(&block_name);
        cell.set_custom_minimum_size(Vector2::new(GEN_PICKER_CELL, GEN_PICKER_CELL));
        if block_name == "air" {
            cell.set_tooltip_text(&kit::t("@none"));
        }
        let target = node.clone();
        let field_for_pick = field.clone();
        let block_for_pick = block_name.clone();
        let callable = Callable::from_fn("map_gen_pick_block", move |_args: &[&Variant]| {
            gen_commit_field(
                &target,
                index,
                &field_for_pick,
                GString::from(block_for_pick.as_str()).to_variant(),
                false,
            );
            free_named(&target, GEN_OVERLAY);
            // The button icon reflects the new value.
            request_refresh(&target, GEN_REQUEST_LIST);
        });
        cell.connect("pressed", &callable);
        grid.add_child(&cell);
    }
    let title = kit::t(&format!("@filter.option.{}", dict_text(option, "name"))).to_string();
    if let Some(overlay) = begin_overlay(
        &mut dialog.bind_mut(),
        GEN_OVERLAY,
        &title,
        900.0,
        420.0,
        Color::from_rgba(0.0, 0.0, 0.0, 0.7),
    ) {
        overlay.add_content(&grid);
    }
}

/// `_valid_block(predicate, name)`.
fn gen_valid_block(predicate: &str, block_name: &str) -> bool {
    if block_name.is_empty() {
        return false;
    }
    dict_rows(&editor_array_arg("filter_blocks_json", predicate))
        .iter()
        .any(|row| dict_text(row, "name") == block_name)
}

/// `ui.logic.show(code, null, true, code -> ...)`: the shared logic editor in
/// its standalone mode (context `filter` + `code`); it echoes the edited code
/// back through `code_edited`.
fn gen_open_custom_action(node: &Gd<MindDialog>, action: &str, index: i64, field: &str) {
    if action != "logiccode" {
        return;
    }
    if kit::ui().is_none() {
        return;
    }
    let mut meta = node.clone();
    set_meta_int(&mut meta, GEN_CODE_INDEX_META, index);
    set_meta_text(&mut meta, GEN_CODE_FIELD_META, field);
    gen_bind_code_edited(node);
    let rows = meta_array(node, GEN_ROWS_META);
    let option = gen_option_for(node, index, field);
    let code = gen_option_string(index, field, &option, &rows);
    let mut context = VarDictionary::new();
    context.set("filter", &true.to_variant());
    context.set("privileged", &true.to_variant());
    context.set("code", &GString::from(code.as_str()).to_variant());
    ui_open_dialog("logic", &context);
}

/// The option descriptor that owns `field` on filter `index`.
fn gen_option_for(node: &Gd<MindDialog>, index: i64, field: &str) -> VarDictionary {
    let rows = meta_array(node, GEN_ROWS_META);
    if let Some(row) = rows
        .get(index.max(0) as usize)
        .and_then(|value| value.try_to::<VarDictionary>().ok())
    {
        for option in dict_array(&row, "options").iter_shared() {
            let Ok(option) = option.try_to::<VarDictionary>() else {
                continue;
            };
            if dict_text(&option, "field") == field {
                return option;
            }
        }
    }
    VarDictionary::new()
}

/// Binds the `logic` dialog's `code_edited` signal (once).
fn gen_bind_code_edited(node: &Gd<MindDialog>) {
    if node.has_meta(GEN_CODE_BOUND_META) {
        return;
    }
    let Some(mut logic) = dialog_node("logic") else {
        return;
    };
    if !logic.has_signal("code_edited") {
        return;
    }
    let target = node.clone();
    let callable = Callable::from_fn("map_gen_code_edited", move |args: &[&Variant]| {
        let code = args
            .first()
            .map(|value| value.to::<GString>())
            .unwrap_or_default();
        let mut meta = target.clone();
        let index = meta_int(&meta, GEN_CODE_INDEX_META, -1);
        if index < 0 {
            return;
        }
        let field = meta_text(&meta, GEN_CODE_FIELD_META);
        set_meta_int(&mut meta, GEN_CODE_INDEX_META, -1);
        gen_commit_field(&target, index, &field, code.to_variant(), false);
        request_refresh(&target, GEN_REQUEST_LIST);
    });
    logic.connect("code_edited", &callable);
    node.clone()
        .set_meta(GEN_CODE_BOUND_META, &true.to_variant());
}

/// An icon + left-aligned label button (the `@add` picker rows).
fn icon_text_button(icon_region: &str, text_value: &str, glyph: &str) -> Gd<Button> {
    // code-instantiated: icon+label picker rows are data-driven (filter types).
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("defaultt");
    let mut row = HBoxContainer::new_alloc();
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.set_offset(Side::LEFT, 8.0);
    row.set_offset(Side::RIGHT, -4.0);
    row.add_theme_constant_override("separation", 6);
    row.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&row);
    if !glyph.is_empty() {
        row.add_child(&factory::glyph_label(glyph, 24, Color::WHITE));
    } else if !icon_region.is_empty() {
        let region = format!("block-{icon_region}-ui");
        if let Some(icon) = factory::content_icon(&region, "block", 28.0) {
            row.add_child(&icon);
        }
    }
    let mut label = factory::label(text_value);
    rich(&label).set_autowrap_mode(AutowrapMode::OFF);
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_horizontal_alignment(HorizontalAlignment::LEFT);
    label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    label.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&as_control(&label));
    button
}

/// `MapGenerateDialog(false)` when opened from the map-info route.
fn gen_applied(node: &Gd<MindDialog>) -> bool {
    if !node.has_meta(GEN_APPLIED_META) {
        return true;
    }
    meta_bool(node, GEN_APPLIED_META)
}

/// The first descendant named `name` (runtime nodes are not scene-owned).
pub(super) fn find_named(node: &Gd<MindDialog>, name: &str) -> Option<Gd<Node>> {
    let root: Gd<Node> = node.clone().upcast();
    root.find_child_ex(name).owned(false).done()
}

/// Frees the first descendant named `name`.
pub(super) fn free_named(node: &Gd<MindDialog>, name: &str) {
    if let Some(mut child) = find_named(node, name) {
        child.queue_free();
    }
}

/// Dispatches this leaf's dialog id to its controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_generate" => Some(Box::new(MapGenerateDialog::default())),
        _ => None,
    }
}
