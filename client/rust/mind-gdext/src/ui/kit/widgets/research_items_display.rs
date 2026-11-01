// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/ItemsDisplay.java (the research dialog's
//         `@globalitems` pool; ResearchDialog.java:111-113 stacks it top-left).
//
//! `Tex.button` panel with the `@globalitems` collapser: amount, 24 px item
//! icon and light-gray name instead of the plain chip row. The collapsed state
//! persists across rebuilds (Java `collapsed` field); used items flash accent
//! (`shine` map keyed by item name).

use godot::builtin::{Side, VarArray, VarDictionary, VariantType};
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::{
    Button, HBoxContainer, IPanelContainer, Label, PanelContainer, RichTextLabel, ScrollContainer,
    StyleBox, StyleBoxTexture, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::ui::kit;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::factory;

/// `ItemsDisplay.java:35-37`: the panel's border texture margins (`Tex.button`).
const BORDER_MARGIN: f32 = 12.0;
/// `c.margin(10).marginLeft(12).marginTop(15f).marginRight(12f)`.
const CONTENT_LEFT: f32 = 12.0;
const CONTENT_TOP: f32 = 15.0;
const CONTENT_RIGHT: f32 = 12.0;
const CONTENT_BOTTOM: f32 = 10.0;
/// `.margin(12f).minWidth(200f)` around the icon + label: 48 px tall.
const TOGGLE_MIN_SIZE: Vector2 = Vector2::new(200.0, 48.0);
/// `MindWidgets.icon_button` uses the 30 px icon font (`Fonts.icon`).
const TOGGLE_GLYPH_SIZE: i32 = 30;
/// `Tex.button` row icon (`ItemsDisplay.drawItem`).
const ROW_ICON_SIZE: f32 = 24.0;
/// `Pal.lightGray`-ish name column (`Color(0.75, 0.75, 0.75)`).
const NAME_GRAY: Color = Color::from_rgba(0.75, 0.75, 0.75, 1.0);
/// `Actions.color(Pal.accent -> white, 0.75s, Interp.fade)`.
const SHINE_DURATION: f64 = 0.75;

/// `MindResearchItemsDisplay` — the research dialog's `@globalitems` pool.
#[derive(GodotClass)]
#[class(base=PanelContainer)]
pub struct MindResearchItemsDisplay {
    base: Base<PanelContainer>,
    /// Whether the list is collapsed (Java `ItemsDisplay.collapsed`).
    #[var]
    pub collapsed: bool,
    /// Maximum list height (view height - 120); set by the host dialog.
    #[var]
    pub max_height: f32,
    toggle: Option<Gd<Button>>,
    toggle_glyph: Option<Gd<Label>>,
    scroll: Option<Gd<ScrollContainer>>,
    list: Option<Gd<VBoxContainer>>,
}

#[godot_api]
impl IPanelContainer for MindResearchItemsDisplay {
    fn init(base: Base<PanelContainer>) -> Self {
        // The base object is unbound during `init`; the panel frame is applied
        // in `ready` (PORTING.md).
        Self {
            base,
            collapsed: false,
            max_height: 400.0,
            toggle: None,
            toggle_glyph: None,
            scroll: None,
            list: None,
        }
    }

    fn ready(&mut self) {
        // code-instantiated: the button frame is built here so the panel matches
        // `table(Tex.button, ...)` without a scene.
        let mut panel = StyleBoxTexture::new_gd();
        if let Some(texture) = kit::icon_texture("button") {
            panel.set_texture(&texture);
        }
        panel.set_texture_margin(Side::LEFT, BORDER_MARGIN);
        panel.set_texture_margin(Side::TOP, BORDER_MARGIN);
        panel.set_texture_margin(Side::RIGHT, BORDER_MARGIN);
        panel.set_texture_margin(Side::BOTTOM, BORDER_MARGIN);
        panel.set_content_margin(Side::LEFT, CONTENT_LEFT);
        panel.set_content_margin(Side::TOP, CONTENT_TOP);
        panel.set_content_margin(Side::RIGHT, CONTENT_RIGHT);
        panel.set_content_margin(Side::BOTTOM, CONTENT_BOTTOM);
        let stylebox: Gd<StyleBox> = panel.upcast();
        self.base_mut()
            .add_theme_stylebox_override("panel", &stylebox);
        self.base_mut().set_mouse_filter(MouseFilter::STOP);
        // code-instantiated: toggle + collapser column (`ItemsDisplay.rebuild`).
        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 4);
        self.base_mut().add_child(&column);
        // code-instantiated: the `@globalitems` collapser button.
        let mut toggle = Button::new_alloc();
        toggle.set_theme_type_variation("flatTogglet");
        toggle.set_toggle_mode(true);
        toggle.set_custom_minimum_size(TOGGLE_MIN_SIZE);
        let pressed = self.base().callable("on_toggle_pressed");
        toggle.connect("pressed", &pressed);
        column.add_child(&toggle);
        self.toggle = Some(toggle);
        self.update_toggle_content();
        // code-instantiated: the collapser list (max height set by the host).
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
        column.add_child(&scroll);
        // code-instantiated: one row per pooled item.
        let mut list = VBoxContainer::new_alloc();
        list.set_h_size_flags(SizeFlags::EXPAND_FILL);
        scroll.add_child(&list);
        self.scroll = Some(scroll);
        self.list = Some(list);
        self.update_visibility();
    }
}

#[godot_api]
impl MindResearchItemsDisplay {
    /// Toggles the list (`collapsed = not collapsed`).
    #[func]
    pub fn on_toggle_pressed(&mut self) {
        self.collapsed = !self.collapsed;
        self.update_visibility();
        self.update_toggle_content();
    }

    /// Rebuilds the rows; `items` is `research_tree_json.items` (item-id order)
    /// and `shine` marks item names whose amount just changed (flash accent ->
    /// white).
    #[func]
    pub fn rebuild(&mut self, items: VarArray, shine: VarDictionary) {
        let Some(mut list) = self.list.clone() else {
            return;
        };
        for child in list.get_children().iter_shared() {
            let mut child = child;
            child.queue_free();
        }
        for row_variant in items.iter_shared() {
            let Ok(row_data) = row_variant.try_to::<VarDictionary>() else {
                continue;
            };
            let amount = dict_i64(&row_data, "amount", 0);
            if amount <= 0 {
                continue;
            }
            let item_name = dict_string(&row_data, "item", "");
            // code-instantiated: one row per item with a positive pool amount.
            let mut row = HBoxContainer::new_alloc();
            row.add_theme_constant_override("separation", 4);
            let text = dict_string(&row_data, "text", &amount.to_string());
            let mut amount_label: Gd<RichTextLabel> = factory::label(&text).upcast();
            if shine.contains_key(item_name.as_str()) {
                // `Actions.color(Pal.accent -> white, 0.75s, Interp.fade)`.
                amount_label.set_modulate(styles::ACCENT);
                let mut tween = amount_label.create_tween();
                tween.tween_property(
                    &amount_label,
                    "modulate",
                    &Color::WHITE.to_variant(),
                    SHINE_DURATION,
                );
            }
            row.add_child(&amount_label);
            // code-instantiated: item icon follows the pool row.
            let mut icon = factory::image(&format!("item-{item_name}-ui"));
            icon.set_custom_minimum_size(Vector2::new(ROW_ICON_SIZE, ROW_ICON_SIZE));
            row.add_child(&icon);
            let name = dict_string(&row_data, "name", &item_name);
            let mut name_label: Gd<RichTextLabel> = factory::label(&name).upcast();
            name_label.add_theme_color_override("default_color", NAME_GRAY);
            row.add_child(&name_label);
            list.add_child(&row);
        }
        self.update_size();
    }

    /// The GDScript `rebuild(items)` call (the `shine = {}` default).
    #[func]
    pub fn rebuild_items(&mut self, items: VarArray) {
        self.rebuild(items, VarDictionary::new());
    }

    /// Alias for [`Self::rebuild_items`] (the `set_items` call name).
    #[func]
    pub fn set_items(&mut self, items: VarArray) {
        self.rebuild_items(items);
    }

    /// Alias for [`Self::rebuild_items`] (the `refresh` call name).
    #[func]
    pub fn refresh(&mut self, items: VarArray) {
        self.rebuild_items(items);
    }

    fn update_toggle_content(&mut self) {
        let Some(mut toggle) = self.toggle.clone() else {
            return;
        };
        for child in toggle.get_children().iter_shared() {
            let mut child = child;
            child.queue_free();
        }
        // code-instantiated: glyph + `@globalitems` label row.
        // `c.button("@globalitems", Icon.downOpen, Styles.flatTogglet, ...)
        // .margin(12f)` (`ItemsDisplay.java:57-61`, `Table.java:544-550`): the
        // icon cell at the glyph's natural size, then the label growing over
        // the rest with its text centred, both vertically centred inside the
        // 12 px margin.
        let mut row = HBoxContainer::new_alloc();
        row.set_alignment(AlignmentMode::BEGIN);
        row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        row.set_offset(Side::LEFT, 12.0);
        row.set_offset(Side::TOP, 12.0);
        row.set_offset(Side::RIGHT, -12.0);
        row.set_offset(Side::BOTTOM, -12.0);
        row.set_h_grow_direction(GrowDirection::BOTH);
        row.set_v_grow_direction(GrowDirection::BOTH);
        row.add_theme_constant_override("separation", 0);
        row.set_mouse_filter(MouseFilter::IGNORE);
        toggle.add_child(&row);
        // `Fonts.icon` is a 30 px font (`MindWidgets.icon_button`).
        let mut glyph = factory::glyph_label("down-open", TOGGLE_GLYPH_SIZE, Color::WHITE);
        glyph.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        glyph.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&glyph);
        self.toggle_glyph = Some(glyph);
        let mut label: Gd<RichTextLabel> =
            factory::label(&kit::tm("@globalitems").to_string()).upcast();
        label.add_theme_color_override("default_color", styles::ACCENT);
        label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        label.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&label);
    }

    fn update_visibility(&mut self) {
        if let Some(mut scroll) = self.scroll.clone() {
            scroll.set_visible(!self.collapsed);
        }
        if let Some(mut toggle) = self.toggle.clone() {
            toggle.set_pressed(self.collapsed);
        }
        if let Some(mut glyph) = self.toggle_glyph.clone() {
            // Java: up-open while collapsed, down-open while expanded.
            glyph.set_text(&kit::glyph(if self.collapsed {
                "up-open"
            } else {
                "down-open"
            }));
        }
    }

    fn update_size(&mut self) {
        let (Some(mut scroll), Some(list)) = (self.scroll.clone(), self.list.clone()) else {
            return;
        };
        let content = list.get_combined_minimum_size().y;
        let mut minimum = scroll.get_custom_minimum_size();
        minimum.y = content.min(self.max_height);
        scroll.set_custom_minimum_size(minimum);
    }
}

/// `row.get(key, default)` with the given fallback.
fn dict_string(dict: &VarDictionary, key: &str, fallback: &str) -> String {
    match dict.get(key) {
        Some(value) => variant_text(&value),
        None => fallback.to_string(),
    }
}

/// `int(row.get(key, default))`.
fn dict_i64(dict: &VarDictionary, key: &str, fallback: i64) -> i64 {
    dict.get(key)
        .map(|value| variant_i64(&value))
        .unwrap_or(fallback)
}

/// GDScript `str(value)`.
fn variant_text(value: &Variant) -> String {
    match value.get_type() {
        VariantType::STRING => value
            .try_to::<GString>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::STRING_NAME => value
            .try_to::<StringName>()
            .map(|text| text.to_string())
            .unwrap_or_default(),
        VariantType::INT => value
            .try_to::<i64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number.to_string())
            .unwrap_or_default(),
        _ => format!("{value}"),
    }
}

/// GDScript `int(value)`: JSON numbers (`float`) and numeric strings convert.
fn variant_i64(value: &Variant) -> i64 {
    match value.get_type() {
        VariantType::INT => value.try_to::<i64>().unwrap_or_default(),
        VariantType::FLOAT => value
            .try_to::<f64>()
            .map(|number| number as i64)
            .unwrap_or_default(),
        VariantType::BOOL => {
            if value.try_to::<bool>().unwrap_or(false) {
                1
            } else {
                0
            }
        }
        VariantType::STRING | VariantType::STRING_NAME => {
            variant_text(value).trim().parse().unwrap_or_default()
        }
        _ => 0,
    }
}
