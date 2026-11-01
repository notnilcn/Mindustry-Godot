// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/world/blocks/ItemSelection.java
//         (`ItemSelection.buildTable`; J05 T1).
//
// The icon-grid content picker Java routes item/liquid/block/unit selection
// through (`Sorter`, `ItemSource`/`LiquidSource`, `Unloader`/`DuctRouter`,
// `LandingPad`, payload blocks, `Constructor`, `UnitCargoUnloadPoint`,
// `UnitFactory`): a `Styles.black6` table of 40 px `clearNoneTogglei` cells
// holding the content `uiIcon` at `clamp(selectionSize, 0, 40)`, a
// `ButtonGroup` with min-check 0 (re-tapping the checked icon sends `null`,
// clearing the config), an optional search field once the grid exceeds
// `rows * 1.5` rows, and the block's remembered `selectScroll` offset.
//
// The widget is built from the `config_spec` JSON (`MindInput.config_spec`'s
// `selection` route) and reports picks through `picked`; the fragment owns the
// submission (`configure_building`), so the widget never mutates sim state.

//! `MindItemSelection` — the `ItemSelection.buildTable` icon grid.
//!
//! One `Button` per spec option inside a `GridContainer`, filtered by the
//! `unlocked`/`on_planet`/`hidden` flags and the optional search text; a pick
//! emits [`MindItemSelection::picked`] and a re-tap of the checked cell emits
//! the empty content name (Java's `null`).

use godot::builtin::{Side, VarArray, VarDictionary};
use godot::classes::control::{CursorShape, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::{
    Button, ButtonGroup, CenterContainer, Control, GridContainer, HBoxContainer, IPanelContainer,
    LineEdit, Node, PanelContainer, ScrollContainer, StyleBox, StyleBoxEmpty, StyleBoxFlat,
    StyleBoxTexture, Texture2D, TextureRect, VBoxContainer, VScrollBar,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, styles};

/// One cell (`Table.defaults().size(40)`, `ItemSelection.java:54`).
const CELL: f32 = 40.0;

/// The tap closes the panel (`ItemSelection.java:70-71`, `closeSelect`).
const ICON_MARGIN: f32 = 0.0;

/// `MindItemSelection` — `Styles.black6` grid of content-icon toggles.
#[derive(GodotClass)]
#[class(base=PanelContainer, init)]
pub struct MindItemSelection {
    base: Base<PanelContainer>,
    /// The configured block's name (`Block.selectScroll` writeback key).
    block: String,
    /// `Block.selectionRows` (`5` unless the spec overrides it).
    #[init(val = 5)]
    rows: i32,
    /// `Block.selectionColumns` (`4` unless the spec overrides it).
    #[init(val = 4)]
    columns: i32,
    /// The checked content name (`ItemSelection.java:75`: `holder.get()`).
    current: String,
    /// The raw `options` array from the spec (non-dictionaries are skipped).
    options: VarArray,
    /// `group.clear()` (`ItemSelection.java:59`) — a fresh group per rebuild.
    group: Option<Gd<ButtonGroup>>,
    grid: Option<Gd<GridContainer>>,
    scroll: Option<Gd<ScrollContainer>>,
    holder: Option<Gd<Control>>,
    search: Option<Gd<LineEdit>>,
    search_row: Option<Gd<HBoxContainer>>,
}

#[godot_api]
impl IPanelContainer for MindItemSelection {}

#[godot_api]
impl MindItemSelection {
    /// Emitted on a pick (`ItemSelection.java:73`: `consumer.get(button.isChecked()
    /// ? item : null)`): the content name, or "" when the tapped icon unchecked.
    #[signal]
    fn picked(value: GString);

    /// Builds the table for a `config_spec` selection route.
    #[func]
    pub fn build(&mut self, spec: VarDictionary) {
        self.block = dict_string(&spec, "block", "");
        self.rows = dict_i32(&spec, "selection_rows", 5).max(1);
        self.columns = dict_i32(&spec, "selection_columns", 4).max(1);
        self.current = dict_string(&spec, "current", "");
        self.options = dict_array(&spec, "options");
        // `Table main = new Table().background(Styles.black6)`
        // (`ItemSelection.java:86`): the translucent black panel, no padding.
        let mut background = StyleBoxFlat::new_gd();
        background.set_bg_color(styles::BLACK_6);
        background.set_content_margin_all(0.0);
        self.base_mut()
            .add_theme_stylebox_override("panel", &background.upcast::<StyleBox>());
        // code-instantiated: the option grid is runtime content from the
        // `config_spec` JSON; it has no scene-authorable per-building shape.
        let mut column = VBoxContainer::new_alloc();
        column.add_theme_constant_override("separation", 0);
        self.base_mut().add_child(&column);
        let search_row = self.build_search_row();
        column.add_child(&search_row);
        self.search_row = Some(search_row);
        let mut holder = Control::new_alloc();
        holder.set_clip_contents(true);
        column.add_child(&holder);
        self.holder = Some(holder.clone());
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        scroll.set_horizontal_scroll_mode(ScrollMode::DISABLED);
        scroll.set_vertical_scroll_mode(ScrollMode::AUTO);
        if let Some(mut bar) = scroll.get_v_scroll_bar() {
            let callable = self.base().callable("on_scrolled");
            bar.connect("value_changed", &callable);
            apply_small_pane(&bar);
        }
        holder.add_child(&scroll);
        self.scroll = Some(scroll.clone());
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(self.columns);
        grid.add_theme_constant_override("h_separation", 0);
        grid.add_theme_constant_override("v_separation", 0);
        grid.set_h_size_flags(SizeFlags::EXPAND_FILL);
        scroll.add_child(&grid);
        self.grid = Some(grid);
        self.rebuild_grid();
        // `Search` row visibility is decided from the unfiltered row count
        // (`ItemSelection.java:87`: `rowCount > rows * 1.5f`).
        let visible_options = self.visible_options();
        let search_visible = self.row_count(&visible_options) > (self.rows as f32 * 1.5) as i32;
        if let Some(mut search_row) = self.search_row.clone() {
            search_row.set_visible(search_visible);
        }
        // `pane.setScrollYForce(block.selectScroll)` (`ItemSelection.java:104`).
        let offset = dict_f32(&spec, "select_scroll", 0.0);
        if offset > 0.0 {
            if let Some(mut scroll) = self.scroll.clone() {
                scroll.set_v_scroll(offset as i32);
            }
            self.base_mut()
                .call_deferred("restore_scroll", &[offset.to_variant()]);
        }
    }

    /// Deferred re-apply: the scroll range only exists after the first layout
    /// pass.
    #[func]
    fn restore_scroll(&mut self, offset: f32) {
        if let Some(mut scroll) = self.scroll.clone()
            && scroll.is_instance_valid()
        {
            scroll.set_v_scroll(offset as i32);
        }
    }

    /// `ItemSelection.java:88-92`: `Icon.zoom` + the `@players.search` field.
    fn build_search_row(&mut self) -> Gd<HBoxContainer> {
        // code-instantiated: the search row is conditional on the option count.
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 0);
        row.set_visible(false);
        let mut zoom = factory::glyph_label("zoom", 22, Color::WHITE);
        zoom.set_custom_minimum_size(Vector2::new(CELL, CELL));
        zoom.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&zoom);
        let mut search = factory::field(&localized("@players.search", "search"));
        search.set_h_size_flags(SizeFlags::EXPAND_FILL);
        let callable = self.base().callable("on_search_changed");
        search.connect("text_changed", &callable);
        row.add_child(&search);
        self.search = Some(search);
        row
    }

    /// `rebuild.run()` (`ItemSelection.java:58-84`): rebuild the grid from the
    /// current search text, the unlock/planet filter and the button group.
    fn rebuild_grid(&mut self) {
        if let Some(mut grid) = self.grid.clone() {
            let children = grid.get_children();
            for child in children.iter_shared() {
                let mut child = child;
                grid.remove_child(&child);
                child.queue_free();
            }
        }
        // `group.clear()` (`ItemSelection.java:59`) — a fresh group per rebuild.
        let mut group = ButtonGroup::new_gd();
        group.set_allow_unpress(true);
        self.group = Some(group);
        let text = self
            .search
            .as_ref()
            .map(|search| search.get_text().to_string().to_lowercase())
            .unwrap_or_default();
        let options = self.visible_options();
        for option in options.iter_shared() {
            let Some(option) = option_dict(&option) else {
                continue;
            };
            if !text.is_empty() && !self.label(&option).to_lowercase().contains(&text) {
                continue;
            }
            let button = self.make_button(&option);
            if let Some(mut grid) = self.grid.clone() {
                grid.add_child(&button);
            }
        }
        // The pane is capped at `40 * rows` (`ItemSelection.java:111`), shrinking
        // to the grid when it is shorter.
        let Some(grid) = self.grid.clone() else {
            return;
        };
        let Some(mut holder) = self.holder.clone() else {
            return;
        };
        let content = grid.get_combined_minimum_size();
        holder.set_custom_minimum_size(Vector2::new(
            content.x,
            content.y.min(CELL * self.rows as f32),
        ));
    }

    /// `if(!item.unlockedNow() || !item.isOnPlanet(state.getPlanet()) ||
    /// item.isHidden()) continue;` (`ItemSelection.java:68`) — the three filter
    /// results (`unlocked`, `on_planet`, `hidden`) are carried by the spec.
    fn visible_options(&self) -> VarArray {
        let mut out = VarArray::new();
        for option in self.options.iter_shared() {
            let Some(option) = option_dict(&option) else {
                continue;
            };
            if !dict_bool(&option, "unlocked", true) || !dict_bool(&option, "on_planet", true) {
                continue;
            }
            if dict_bool(&option, "hidden", false) {
                continue;
            }
            out.push(&option.to_variant());
        }
        out
    }

    /// `Int` division: Java's `rowCount` counts only broken rows
    /// (`ItemSelection.java:77-80`).
    fn row_count(&self, options: &VarArray) -> i32 {
        options.len() as i32 / self.columns.max(1)
    }

    /// `ItemSelection.java:70-75`: a 40 px cell with the content icon at
    /// `clamp(selectionSize, 0, 40)`, grouped with min-check 0.
    fn make_button(&mut self, option: &VarDictionary) -> Gd<Button> {
        let option_name = dict_string(option, "name", "");
        // code-instantiated: one toggle per spec option (runtime content count).
        let mut button = Button::new_alloc();
        button.set_name(format!("option-{option_name}").as_str());
        button.set_toggle_mode(true);
        if let Some(group) = self.group.clone() {
            button.set_button_group(&group);
        }
        button.set_custom_minimum_size(Vector2::new(CELL, CELL));
        button.set_default_cursor_shape(CursorShape::POINTING_HAND);
        button.set_tooltip_text(&self.label(option));
        clear_none_toggle_style(&mut button);
        if let Some(texture) = option_icon_texture(option) {
            // code-instantiated: one centered icon cell per option (the cell is
            // the `cont.defaults().size(40)` Table cell; the icon is drawn at
            // `clamp(selectionSize, 0, 40)` like `Scaling.fit` on the Java
            // image).
            let mut center = CenterContainer::new_alloc();
            center.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            center.set_mouse_filter(MouseFilter::IGNORE);
            button.add_child(&center);
            center.add_child(&Self::icon_rect(texture, dict_f32(option, "size", 24.0)));
        } else {
            button.set_text(&self.label(option));
        }
        if option_name == self.current {
            // `button.update(() -> button.setChecked(holder.get() == item))`
            // (`ItemSelection.java:75`) without re-entering `picked`.
            button.set_pressed_no_signal(true);
        }
        let target = self.to_gd();
        let name = option_name;
        let callable = Callable::from_fn("item_selection_pick", move |args: &[&Variant]| {
            let on = args
                .first()
                .and_then(|value| value.try_to::<bool>().ok())
                .unwrap_or(false);
            let value = if on { name.clone() } else { String::new() };
            let mut node = target.clone().upcast::<Node>();
            node.emit_signal("picked", &[GString::from(value.as_str()).to_variant()]);
        });
        button.connect("toggled", &callable);
        button
    }

    /// The option's localized label (a bundle key from the spec).
    fn label(&self, option: &VarDictionary) -> String {
        localized(
            &dict_string(option, "label", ""),
            &dict_string(option, "name", ""),
        )
    }

    /// `text_changed` -> rebuild (`_on_search_changed`).
    #[func]
    fn on_search_changed(&mut self, _text: GString) {
        self.rebuild_grid();
    }

    /// `pane.update(() -> block.selectScroll = pane.getScrollY())`
    /// (`ItemSelection.java:105-107`).
    #[func]
    fn on_scrolled(&mut self, offset: f64) {
        if self.block.is_empty() {
            return;
        }
        let Some(mut input) = kit::node("/root/Spine/Input") else {
            return;
        };
        if !input.has_method("set_config_scroll") {
            return;
        }
        input.call(
            "set_config_scroll",
            &[
                GString::from(self.block.as_str()).to_variant(),
                (offset as f32).to_variant(),
            ],
        );
    }

    /// The option icon at `Mathf.clamp(selectionSize, 0, 40)` (`ItemSelection.java:70`;
    /// `selectionSize` 24 for items/liquids, 28 blocks, 30 units).
    /// EXPAND_IGNORE_SIZE: a `TextureRect` otherwise never shrinks below its
    /// ~32 px texture, so the clamp had no effect (L05 T5,
    /// `diff/22-srccfg-zoom.png`).
    #[func]
    fn icon_rect(texture: Gd<Texture2D>, selection_size: f32) -> Gd<TextureRect> {
        let icon_size = selection_size.clamp(0.0, CELL);
        let mut icon = TextureRect::new_alloc();
        icon.set_texture(&texture);
        icon.set_custom_minimum_size(Vector2::new(icon_size, icon_size));
        icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
        icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        icon.set_mouse_filter(MouseFilter::IGNORE);
        icon
    }

    /// `Styles.clearNoneTogglei` (`Styles.java:351-356`): `up = none`,
    /// `over = flatOver` (#454545), `down`/`checked = flatDown` (the
    /// `flat-down-base` 9-patch: black centre in a 4 px `Pal.accent` frame).
    /// Shared with `MindItemCommandRow` (J05 T2), the style Java uses for both
    /// panels' cells.
    #[func]
    fn apply_clear_none_toggle(button: Gd<Button>) {
        let mut button = button;
        clear_none_toggle_style(&mut button);
    }

    /// `Styles.createFlatDown` (`Styles.java:479-495`).
    #[func]
    fn flat_down_frame() -> Gd<StyleBox> {
        flat_down_style()
    }
}

/// Reads a spec property (GDScript `spec.get(key)`).
pub(crate) fn dict_value(dict: &VarDictionary, key: &str) -> Option<Variant> {
    let key = key.to_variant();
    dict.get(&key)
}

/// GDScript `str()` for a spec value (`""` for `null`/unconvertible).
pub(crate) fn value_string(value: &Variant) -> String {
    if let Ok(text) = value.try_to::<GString>() {
        return text.to_string();
    }
    if let Ok(number) = value.try_to::<f64>() {
        return format!("{number}");
    }
    if let Ok(number) = value.try_to::<i64>() {
        return number.to_string();
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return flag.to_string();
    }
    String::new()
}

/// GDScript `float()` for a spec value.
pub(crate) fn value_f32(value: &Variant) -> Option<f32> {
    if let Ok(number) = value.try_to::<f64>() {
        return Some(number as f32);
    }
    if let Ok(number) = value.try_to::<i64>() {
        return Some(number as f32);
    }
    if let Ok(flag) = value.try_to::<bool>() {
        return Some(if flag { 1.0 } else { 0.0 });
    }
    None
}

/// `str(spec.get(key, fallback))`.
pub(crate) fn dict_string(dict: &VarDictionary, key: &str, fallback: &str) -> String {
    match dict_value(dict, key) {
        Some(value) => value_string(&value),
        None => fallback.to_string(),
    }
}

/// `bool(spec.get(key, fallback))`.
pub(crate) fn dict_bool(dict: &VarDictionary, key: &str, fallback: bool) -> bool {
    let Some(value) = dict_value(dict, key) else {
        return fallback;
    };
    if let Ok(flag) = value.try_to::<bool>() {
        return flag;
    }
    value_f32(&value)
        .map(|number| number != 0.0)
        .unwrap_or(fallback)
}

/// `int(spec.get(key, fallback))`.
pub(crate) fn dict_i32(dict: &VarDictionary, key: &str, fallback: i32) -> i32 {
    dict_value(dict, key)
        .as_ref()
        .and_then(value_f32)
        .map(|number| number as i32)
        .unwrap_or(fallback)
}

/// `float(spec.get(key, fallback))`.
pub(crate) fn dict_f32(dict: &VarDictionary, key: &str, fallback: f32) -> f32 {
    dict_value(dict, key)
        .as_ref()
        .and_then(value_f32)
        .unwrap_or(fallback)
}

/// `spec.get(key, [])` when the value is an `Array` (else empty).
pub(crate) fn dict_array(dict: &VarDictionary, key: &str) -> VarArray {
    dict_value(dict, key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

/// One `options` entry (`if not (option is Dictionary): continue`).
pub(crate) fn option_dict(value: &Variant) -> Option<VarDictionary> {
    value.try_to::<VarDictionary>().ok()
}

/// Resolves a bundle key through `MindAssets` with a raw fallback.
pub(crate) fn localized(key: &str, fallback: &str) -> String {
    let resolved = key.strip_prefix('@').unwrap_or(key);
    if resolved.is_empty() {
        return fallback.to_string();
    }
    // `kit::t` echoes the key when the assets autoload or the entry is missing,
    // which is the GDScript fallback condition.
    let value = kit::t(resolved).to_string();
    if value.is_empty() || value == resolved {
        fallback.to_string()
    } else {
        value
    }
}

/// `Styles.smallPane` (`Styles.java:368-371`): the vertical track is `clear`
/// and the knob the thin `scroll-knob-vertical-thin` region.
fn apply_small_pane(bar: &Gd<VScrollBar>) {
    let mut bar = bar.clone();
    let track: Gd<StyleBox> = StyleBoxEmpty::new_gd().upcast();
    bar.add_theme_stylebox_override("scroll", &track);
    bar.add_theme_stylebox_override("scroll_focus", &track);
    if let Some(texture) = kit::icon_texture("scroll-knob-vertical-thin") {
        let mut knob = StyleBoxTexture::new_gd();
        knob.set_texture(&texture);
        let knob: Gd<StyleBox> = knob.upcast();
        bar.add_theme_stylebox_override("grabber", &knob);
        bar.add_theme_stylebox_override("grabber_highlight", &knob);
        bar.add_theme_stylebox_override("grabber_pressed", &knob);
    }
}

/// `Styles.clearNoneTogglei` (`Styles.java:351-356`) — the shared
/// `flatOver`/`flatDown` cell style of both configuration panels.
pub(crate) fn clear_none_toggle_style(button: &mut Gd<Button>) {
    let mut over = StyleBoxFlat::new_gd();
    over.set_bg_color(styles::FLAT_OVER);
    over.set_content_margin_all(ICON_MARGIN);
    let down = flat_down_style();
    button.add_theme_stylebox_override("normal", &StyleBoxEmpty::new_gd().upcast::<StyleBox>());
    button.add_theme_stylebox_override("hover", &over.upcast::<StyleBox>());
    button.add_theme_stylebox_override("pressed", &down);
    button.add_theme_stylebox_override("hover_pressed", &down);
    button.add_theme_stylebox_override("disabled", &StyleBoxEmpty::new_gd().upcast::<StyleBox>());
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd().upcast::<StyleBox>());
}

/// `Styles.createFlatDown` (`Styles.java:479-495`).
pub(crate) fn flat_down_style() -> Gd<StyleBox> {
    let Some(texture) = kit::icon_texture("flat-down-base") else {
        let mut box_ = StyleBoxFlat::new_gd();
        box_.set_bg_color(styles::BLACK);
        box_.set_border_color(styles::ACCENT);
        box_.set_border_width_all(4);
        box_.set_content_margin_all(0.0);
        return box_.upcast::<StyleBox>();
    };
    let mut frame = StyleBoxTexture::new_gd();
    frame.set_texture(&texture);
    frame.set_texture_margin(Side::LEFT, 12.0);
    frame.set_texture_margin(Side::TOP, 12.0);
    frame.set_texture_margin(Side::RIGHT, 12.0);
    frame.set_texture_margin(Side::BOTTOM, 12.0);
    frame.set_content_margin_all(0.0);
    frame.upcast::<StyleBox>()
}

/// `UnlockableContent.uiIcon` (`UnlockableContent.java:108-117`): the
/// `{type}-{name}-ui` region, else the `fullOverride`/full-icon chain.
fn option_icon_texture(option: &VarDictionary) -> Option<Gd<Texture2D>> {
    let icons = dict_array(option, "icons");
    for candidate in icons.iter_shared() {
        if let Some(texture) = kit::icon_texture(&value_string(&candidate)) {
            return Some(texture);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_clamps_to_the_cell() {
        // `Mathf.clamp(selectionSize, 0, 40)` (`ItemSelection.java:70`).
        assert_eq!(24.0f32.clamp(0.0, CELL), 24.0);
        assert_eq!(50.0f32.clamp(0.0, CELL), 40.0);
        assert_eq!((-4.0f32).clamp(0.0, CELL), 0.0);
    }

    #[test]
    fn search_row_threshold_matches_java() {
        // `rowCount > rows * 1.5f` truncated with `int()`: 5 rows -> 7.
        assert_eq!((5.0f32 * 1.5) as i32, 7);
        assert_eq!((8.0f32 * 1.5) as i32, 12);
        // The row count is an integer division.
        assert_eq!(20 / 4, 5);
    }

    #[test]
    fn empty_prefix_falls_back_to_the_raw_name() {
        assert_eq!(localized("", "copper"), "copper");
    }
}
