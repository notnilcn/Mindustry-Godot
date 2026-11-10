// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `util` support code.
//! Upstream: logic/LStatement.java

use super::*;

// ---------------------------------------------------------------------------
// dictionary helpers (`Dictionary.get(key, default)` semantics)
// ---------------------------------------------------------------------------

pub(super) fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(key)
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default()
}

pub(super) fn dict_bool(dict: &VarDictionary, key: &str, fallback: bool) -> bool {
    dict.get(key)
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(fallback)
}

pub(super) fn dict_f64(dict: &VarDictionary, key: &str, fallback: f64) -> f64 {
    dict.get(key)
        .and_then(|value| value.try_to::<f64>().ok())
        .unwrap_or(fallback)
}

// ---------------------------------------------------------------------------
// small node helpers
// ---------------------------------------------------------------------------

/// Adds a `Control` child to any `Container` (the GDScript `add_child`).
pub(super) fn add_any<C>(parent: &Gd<Container>, child: &Gd<C>)
where
    C: Inherits<Control>,
{
    let mut parent = parent.clone();
    let control: Gd<Control> = child.clone().upcast();
    parent.add_child(&control);
}

/// A sizing-only cell (`MindWidgets.space`) that never takes pointer input.
pub(super) fn gap(width: f32) -> Gd<Control> {
    let mut gap = factory::space(width, 0.0);
    gap.set_mouse_filter(MouseFilter::IGNORE);
    gap
}

/// `StatementElem.draw`: `Fill.dropShadow` (10 px, 0.9 alpha) under a 30%
/// black fill.
pub(super) fn shadow_box() -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.3));
    box_.set_shadow_color(Color::from_rgba(0.0, 0.0, 0.0, 0.45));
    box_.set_shadow_size(10);
    box_
}

/// `Styles.outlineLabel` tinted `color` (header name/address).
pub(super) fn outline_label(text: &str, color: Color) -> Gd<Label> {
    // code-instantiated: header labels are rebuilt per statement element.
    let mut label = Label::new_alloc();
    label.set_text(&gs(text));
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_mouse_filter(MouseFilter::IGNORE);
    label.add_theme_color_override("font_color", color);
    label.add_theme_color_override("font_outline_color", Color::BLACK);
    label.add_theme_constant_override("outline_size", 4);
    label
}

/// `t.button(Icon.x, Styles.logici, listener).size(24f)`: a black icon, no
/// background.
pub(super) fn logici_button(icon: &str) -> Gd<Button> {
    // code-instantiated: header buttons are rebuilt per statement element.
    let mut button = Button::new_alloc();
    button.set_custom_minimum_size(Vector2::new(24.0, 24.0));
    button.set_focus_mode(FocusMode::NONE);
    for style_name in [
        "normal",
        "hover",
        "pressed",
        "disabled",
        "focus",
        "hover_pressed",
    ] {
        button.add_theme_stylebox_override(style_name, &StyleBoxEmpty::new_gd());
    }
    let mut glyph = factory::glyph_label(icon, 20, Color::BLACK);
    glyph.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    glyph.set_mouse_filter(MouseFilter::IGNORE);
    button.add_child(&glyph);
    button
}

/// `Tex.underlineWhite`: a 4 px bottom rule (`Styles.nodeField`/`logict`).
pub(super) fn underline_box(color: Color) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    box_.set_border_color(color);
    box_.set_border_width(Side::BOTTOM, 4);
    box_.set_content_margin(Side::LEFT, 6.0);
    box_.set_content_margin(Side::RIGHT, 6.0);
    box_.set_content_margin(Side::TOP, 4.0);
    box_.set_content_margin(Side::BOTTOM, 6.0);
    box_
}

pub(super) fn flat_box(color: Color) -> Gd<StyleBoxFlat> {
    flat_box_margins(color, 6.0)
}

pub(super) fn flat_box_margins(color: Color, left: f32) -> Gd<StyleBoxFlat> {
    let mut box_ = StyleBoxFlat::new_gd();
    box_.set_bg_color(color);
    box_.set_content_margin(Side::LEFT, left);
    box_.set_content_margin(Side::RIGHT, 6.0);
    box_.set_content_margin(Side::TOP, 4.0);
    box_.set_content_margin(Side::BOTTOM, 4.0);
    box_
}

/// `Styles.nodeField` (`underlineWhite` background) tinted with the element
/// colour; the text stays white.
pub(super) fn apply_node_field(edit: &mut Gd<LineEdit>, color: Color) {
    edit.add_theme_stylebox_override("normal", &underline_box(color));
    edit.add_theme_stylebox_override("focus", &underline_box(color));
    edit.add_theme_stylebox_override("read_only", &underline_box(styles::ARC_GRAY));
    edit.add_theme_color_override("font_color", Color::WHITE);
}

/// `Styles.logict` (`up = underlineWhite`, `over/down = flatOver`) with the
/// cell's `.color(table.color)` tint on the background only.
pub(super) fn apply_logict(button: &mut Gd<Button>, color: Color) {
    button.add_theme_stylebox_override("normal", &underline_box(color));
    button.add_theme_stylebox_override("hover", &flat_box(styles::FLAT_OVER));
    button.add_theme_stylebox_override("pressed", &flat_box(styles::FLAT_OVER));
    button.add_theme_stylebox_override("hover_pressed", &flat_box(styles::FLAT_OVER));
    button.add_theme_color_override("font_color", Color::WHITE);
}

pub(super) fn field_label(text: &str) -> Gd<Label> {
    // code-instantiated: one label per field-row item (count is layout-driven).
    let mut label = Label::new_alloc();
    label.set_text(&gs(text));
    label.set_vertical_alignment(VerticalAlignment::CENTER);
    label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    label.set_custom_minimum_size(Vector2::new(0.0, 40.0));
    label
}
