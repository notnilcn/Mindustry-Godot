// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: arc.scene.ui.* widget constructors (plan 14 §3.3).

//! Themed widget factories (`MindWidgets`).
//!
//! Dialogs never hand-build themed widgets: every helper applies the manifest
//! type variation (`theme_type_variation`) expected by the theme builder, and
//! text/icon lookup goes through the `MindAssets` autoload.

use godot::builtin::Side;
use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, Control, Font, HBoxContainer, HSlider, Label, LineEdit, RichTextLabel, TextureRect,
};
use godot::obj::NewAlloc;
use godot::prelude::*;

use super::super::icons;
use super::super::scroll::MindScroll;
use super::super::table::{MindStack, MindTable};
use super::super::text::MindLabel;
use super::super::{assets, icon_texture};

/// A `MindTable`.
pub fn table() -> Gd<MindTable> {
    MindTable::new_alloc()
}

/// A `MindStack`.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub fn stack() -> Gd<MindStack> {
    MindStack::new_alloc()
}

/// A `MindScroll` with its content table created.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub fn scroll() -> Gd<MindScroll> {
    let mut pane = MindScroll::new_alloc();
    pane.bind_mut().content();
    pane
}

/// A `MindLabel` with `text` assigned.
pub fn label(text: &str) -> Gd<MindLabel> {
    let node = MindLabel::new_alloc();
    let mut rich: Gd<RichTextLabel> = node.clone().upcast();
    rich.set_text(text);
    node
}

/// A `MindLabel` carrying a theme type variation.
pub fn styled_label(text: &str, style: &str) -> Gd<MindLabel> {
    let node = label(text);
    if !style.is_empty() {
        let mut rich: Gd<RichTextLabel> = node.clone().upcast();
        rich.set_theme_type_variation(style);
    }
    node
}

/// A themed text button (`defaultt`).
pub fn button(text: &str) -> Gd<Button> {
    let mut result = Button::new_alloc();
    result.set_text(text);
    result.set_theme_type_variation("defaultt");
    result
}

/// A themed image button resolved from one atlas region.
pub fn image_button(region_name: &str, style: &str) -> Gd<Button> {
    let mut result = Button::new_alloc();
    result.set_theme_type_variation(style);
    if let Some(texture) = icon_texture(region_name) {
        result.set_button_icon(&texture);
    }
    result
}

/// Image button resolved from the first available region in `regions`
/// (`core.atlas.find` fallback chain; e.g. `block-x-ui` then `block-x-full`).
pub fn image_button_first(regions: &[&str], style: &str) -> Gd<Button> {
    let mut result = Button::new_alloc();
    result.set_theme_type_variation(style);
    for region_name in regions {
        if let Some(texture) = icon_texture(region_name) {
            result.set_button_icon(&texture);
            break;
        }
    }
    result
}

/// An icon-font-glyph toggle button (`PlacementFragment` category rail);
/// falls back to `fallback_text` when the glyph is unknown.
pub fn glyph_button(icon_name: &str, fallback_text: &str, style: &str) -> Gd<Button> {
    let mut result = Button::new_alloc();
    result.set_theme_type_variation(style);
    let glyph_text = icons::glyph(icon_name);
    if glyph_text.is_empty() {
        result.set_text(fallback_text);
        return result;
    }
    // code-instantiated: centered glyph label inside the themed toggle button.
    let mut icon = glyph_label(icon_name, 28, Color::WHITE);
    icon.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    icon.set_mouse_filter(MouseFilter::IGNORE);
    result.add_child(&icon);
    result
}

/// An action button with an icon-font glyph followed by a label.
///
/// Mirrors upstream `TextButton` + `Icon.*` drawables (`Table.button(text,
/// image, runnable)`): the icon is the first cell at the glyph's natural size
/// and the label grows over the remaining width, centring its text.
pub fn icon_button(
    icon_name: &str,
    text_value: &str,
    style: &str,
    row_alignment: godot::classes::box_container::AlignmentMode,
) -> Gd<Button> {
    let mut result = Button::new_alloc();
    result.set_theme_type_variation(style);
    let glyph_text = icons::glyph(icon_name);
    if glyph_text.is_empty() {
        result.set_text(text_value);
        return result;
    }
    // code-instantiated: data-driven icon+label row inside the themed button.
    let icon_only = text_value.is_empty();
    let mut row = HBoxContainer::new_alloc();
    // An icon-only entry is Java's `button(Icon.x, style, size, ...)` image
    // button (icon centred); a text button keeps the icon cell at the left.
    row.set_alignment(if icon_only {
        godot::classes::box_container::AlignmentMode::CENTER
    } else {
        row_alignment
    });
    row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    row.add_theme_constant_override("separation", 0);
    row.set_mouse_filter(MouseFilter::IGNORE);
    result.add_child(&row);
    // The icon cell: the glyph at the icon font's 30 px size fills its own
    // cell (Java does not reserve `iconMed` for text buttons).
    let mut icon = glyph_label(icon_name, 30, Color::WHITE);
    icon.set_v_size_flags(SizeFlags::SHRINK_CENTER);
    icon.set_mouse_filter(MouseFilter::IGNORE);
    row.add_child(&icon);
    if icon_only {
        let row_for_min = row.clone();
        let result_for_min = result.clone();
        let callable = Callable::from_fn("icon_only_min_size", move |_args: &[&Variant]| {
            sync_button_minimum(&result_for_min, &row_for_min);
        });
        row.connect("ready", &callable);
        return result;
    }
    let mut text_label = Label::new_alloc();
    text_label.set_text(text_value);
    text_label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    text_label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
    text_label.set_h_size_flags(SizeFlags::EXPAND_FILL);
    text_label.set_mouse_filter(MouseFilter::IGNORE);
    // `TextButton` wraps its label (`TextButton.java:30-34`: `add(label)
    // .grow().wrap()`; `Cell.java:260-272` -> `Label.setWrap(true)`), so a
    // label wider than the button cell folds onto a second line instead of
    // overflowing (PAR-0433: the logic editor's 160x64 "Built-in Variables"
    // cell). The wrap only affects the label inside the cell; the cell size
    // itself stays fixed, which the `ready` hook below protects by measuring
    // the label's font metrics, not its wrapped minimum.
    text_label.set_autowrap_mode(WRAP_WORD_SMART);
    // Apply the default font up front so the row reports a real minimum width
    // (the theme font is not available before the node enters the tree).
    if let Some(assets) = assets()
        && let Some(font) = assets.bind().default_font()
    {
        let font: Gd<Font> = font.upcast();
        text_label.add_theme_font_override("font", &font);
    }
    text_label.add_theme_font_size_override("font_size", 18);
    row.add_child(&text_label);
    // The text is a child Label, so it never sees the button's disabled state:
    // Arc `TextButtonStyle.disabledFontColor` only reaches it through this hook.
    let result_for_draw = result.clone();
    let label_for_draw = text_label.clone();
    let draw_callable = Callable::from_fn("icon_button_disabled", move |_args: &[&Variant]| {
        let mut button = result_for_draw.clone();
        let disabled = button.is_disabled();
        if button.has_meta("_shown_disabled")
            && button.get_meta("_shown_disabled").to::<bool>() == disabled
        {
            return;
        }
        button.set_meta("_shown_disabled", &disabled.to_variant());
        let mut label = label_for_draw.clone();
        if disabled {
            let color = button.get_theme_color("font_disabled_color");
            label.add_theme_color_override("font_color", color);
        } else {
            label.remove_theme_color_override("font_color");
        }
    });
    result.connect("draw", &draw_callable);
    // Arc `Button` is a `Table` whose background drawable pads its cells, so the
    // icon cell starts inside the border and the label centres in the padded
    // width. Inset the row by the style box's content margins.
    inset_button_content(&result, &row);
    let result_for_theme = result.clone();
    let row_for_theme = row.clone();
    let theme_callable = Callable::from_fn("icon_button_inset", move |_args: &[&Variant]| {
        inset_button_content(&result_for_theme, &row_for_theme);
    });
    result.connect("theme_changed", &theme_callable);
    // A `Button` is not a container, so children do not contribute to its
    // minimum size; size it from the padded icon+label row once metrics resolve.
    let result_for_ready = result.clone();
    let row_for_ready = row.clone();
    let icon_for_ready = icon.clone();
    let label_for_ready = text_label.clone();
    let ready_callable = Callable::from_fn("icon_button_ready", move |_args: &[&Variant]| {
        inset_button_content(&result_for_ready, &row_for_ready);
        sync_text_button_minimum(
            &result_for_ready,
            &row_for_ready,
            &icon_for_ready,
            &label_for_ready,
        );
    });
    result.connect("ready", &ready_callable);
    result
}

/// Applies [`inset_to_content`] to a `Button` + `HBoxContainer` pair.
fn inset_button_content(button: &Gd<Button>, row: &Gd<HBoxContainer>) {
    let mut button: Gd<Control> = button.clone().upcast();
    let mut row: Gd<Control> = row.clone().upcast();
    inset_to_content(&mut button, &mut row);
}

/// Raises a button's `custom_minimum_size` to its padded content row minimum.
fn sync_button_minimum(button: &Gd<Button>, row: &Gd<HBoxContainer>) {
    if !button.is_instance_valid() || !row.is_instance_valid() {
        return;
    }
    let row_min = row.get_combined_minimum_size();
    apply_button_minimum(button, row_min);
}

/// [`sync_button_minimum`] for the icon+label row: a wrapped `Label` reports
/// the height it needs *at its current width*, which is still degenerate on
/// `ready` (the layout has not run yet) and would grow the shared button past
/// its declared cell (`PAR-0433`: the logic editor's 160x64 row grew to 151
/// px). Measure the label's unwrapped single-line size from the font metrics
/// instead — those never depend on the current layout.
fn sync_text_button_minimum(
    button: &Gd<Button>,
    row: &Gd<HBoxContainer>,
    icon: &Gd<Label>,
    label: &Gd<Label>,
) {
    if !button.is_instance_valid() || !row.is_instance_valid() {
        return;
    }
    let row_min = match label_single_line_size(label) {
        Some(text) => text_button_row_minimum(icon.get_combined_minimum_size(), text),
        None => row.get_combined_minimum_size(),
    };
    apply_button_minimum(button, row_min);
}

/// The icon+label row minimum: the icon cell plus the label's single-line font
/// metrics (`PAR-0433`).
fn text_button_row_minimum(icon_min: Vector2, label_single_line: Vector2) -> Vector2 {
    Vector2::new(
        icon_min.x + label_single_line.x,
        icon_min.y.max(label_single_line.y),
    )
}

/// The label's single-line size from its resolved font metrics, or `None`
/// while the theme font is unresolved.
fn label_single_line_size(label: &Gd<Label>) -> Option<Vector2> {
    let font = label.get_theme_font("font")?;
    let font_size = label.get_theme_font_size("font_size");
    let width = font
        .get_string_size_ex(&label.get_text())
        .alignment(godot::global::HorizontalAlignment::LEFT)
        .width(-1.0)
        .font_size(font_size)
        .done()
        .x;
    Some(Vector2::new(
        width,
        font.get_height_ex().font_size(font_size).done(),
    ))
}

/// Applies a padded content minimum to a button's `custom_minimum_size`.
fn apply_button_minimum(button: &Gd<Button>, row_min: Vector2) {
    if !button.is_instance_valid() {
        return;
    }
    let mut button = button.clone();
    let box_ = button.get_theme_stylebox("normal");
    let mut pad = Vector2::ZERO;
    if let Some(box_) = box_ {
        pad = Vector2::new(
            box_.get_margin(Side::LEFT) + box_.get_margin(Side::RIGHT),
            box_.get_margin(Side::TOP) + box_.get_margin(Side::BOTTOM),
        );
    }
    let current = button.get_custom_minimum_size();
    button.set_custom_minimum_size(Vector2::new(
        current.x.max(row_min.x + pad.x),
        current.y.max(row_min.y + pad.y),
    ));
}

/// Insets a full-rect content `row` inside `button` by the button's `normal`
/// style box content margins (Arc's background padding).
pub fn inset_to_content(btn: &mut Gd<Control>, row: &mut Gd<Control>) {
    if !btn.is_instance_valid() || !row.is_instance_valid() {
        return;
    }
    row.set_h_grow_direction(GrowDirection::BOTH);
    row.set_v_grow_direction(GrowDirection::BOTH);
    // The themed variation only resolves inside the tree.
    if !btn.is_inside_tree() {
        return;
    }
    let Some(box_) = btn.get_theme_stylebox("normal") else {
        return;
    };
    row.set_offset(Side::LEFT, box_.get_margin(Side::LEFT));
    row.set_offset(Side::TOP, box_.get_margin(Side::TOP));
    row.set_offset(Side::RIGHT, -box_.get_margin(Side::RIGHT));
    row.set_offset(Side::BOTTOM, -box_.get_margin(Side::BOTTOM));
}

/// A standalone icon-glyph label (section headers, rail entries).
pub fn glyph_label(icon_name: &str, font_size: i32, color: Color) -> Gd<Label> {
    // code-instantiated: icon runs use the icon font, which no themed label
    // carries.
    let mut result = Label::new_alloc();
    result.set_text(&icons::glyph(icon_name));
    let mut control: Gd<Control> = result.clone().upcast();
    icons::apply_icon_font(&mut control);
    result.add_theme_font_size_override("font_size", font_size);
    result.add_theme_color_override("font_color", color);
    result.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    result.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
    result
}

/// A themed, non-expanding `TextureRect` for an atlas region.
pub fn image(region_name: &str) -> Gd<TextureRect> {
    let mut result = TextureRect::new_alloc();
    if let Some(texture) = icon_texture(region_name) {
        result.set_texture(&texture);
    }
    result.set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
    result.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    result
}

/// A themed text field (`defaultField`).
pub fn field(placeholder: &str) -> Gd<LineEdit> {
    let mut result = LineEdit::new_alloc();
    result.set_placeholder(placeholder);
    result.set_theme_type_variation("defaultField");
    // Arc draws `messageText` only while the field is unfocused; Godot keeps the
    // placeholder over a focused empty field.
    let placeholder_text = GString::from(placeholder);
    let field_for_focus = result.clone();
    let placeholder_for_focus = placeholder_text.clone();
    let entered = Callable::from_fn("field_focus_entered", move |_args: &[&Variant]| {
        let mut field = field_for_focus.clone();
        field.set_placeholder("");
        let _ = &placeholder_for_focus;
    });
    result.connect("focus_entered", &entered);
    let field_for_blur = result.clone();
    let exited = Callable::from_fn("field_focus_exited", move |_args: &[&Variant]| {
        let mut field = field_for_blur.clone();
        field.set_placeholder(&placeholder_text);
    });
    result.connect("focus_exited", &exited);
    result
}

/// A themed slider (`defaultSlider`).
pub fn slider(minimum: f64, maximum: f64, step: f64) -> Gd<HSlider> {
    let mut result = HSlider::new_alloc();
    result.set_min(minimum);
    result.set_max(maximum);
    result.set_step(step);
    result.set_theme_type_variation("defaultSlider");
    // Arc `Slider` has no `scrolled` handler: the wheel falls through to the
    // surrounding pane and never moves a slider.
    result.set_scrollable(false);
    result
}

/// A sizing-only spacer.
pub fn space(width: f32, height: f32) -> Gd<Control> {
    // code-instantiated: spacers are sizing-only layout cells with no scene.
    let mut result = Control::new_alloc();
    result.set_custom_minimum_size(Vector2::new(width, height));
    result
}

/// Fades a canvas item in over `duration` (`Actions.fadeIn`).
pub fn fade_in<T>(node: &mut Gd<T>, duration: f32)
where
    T: Inherits<godot::classes::CanvasItem>,
{
    let mut canvas: Gd<godot::classes::CanvasItem> = node.clone().upcast();
    let mut modulate = canvas.get_modulate();
    modulate.a = 0.0;
    canvas.set_modulate(modulate);
    let mut tween = canvas.create_tween();
    tween.tween_property(&canvas, "modulate:a", &1.0f32.to_variant(), duration as f64);
}

/// Fades a canvas item out over `duration` (`Actions.fadeOut`).
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub fn fade_out<T>(node: &mut Gd<T>, duration: f32)
where
    T: Inherits<godot::classes::CanvasItem>,
{
    let mut canvas: Gd<godot::classes::CanvasItem> = node.clone().upcast();
    let mut tween = canvas.create_tween();
    tween.tween_property(&canvas, "modulate:a", &0.0f32.to_variant(), duration as f64);
}

/// Frees a node after `delay` seconds.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub fn remove_after(node: &mut Gd<Node>, delay: f32) {
    let target = node.clone();
    let mut tween = node.create_tween();
    tween.tween_interval(delay as f64);
    let callable = Callable::from_fn("remove_after", move |_args: &[&Variant]| {
        let mut node = target.clone();
        node.queue_free();
    });
    tween.tween_callback(&callable);
}

/// Content icon control (`content.uiIcon`): the packed `sector-<name>-ui` /
/// `item-<name>-ui` region, else the `terrain` glyph for sector presets, else
/// `None` (callers draw `Icon.lock`).
pub fn content_icon(icon_region: &str, type_name: &str, size: f32) -> Option<Gd<Control>> {
    if let Some(texture) = icon_texture(icon_region) {
        // code-instantiated: content icons are data-driven (atlas lookup).
        let mut image_rect = TextureRect::new_alloc();
        image_rect.set_texture(&texture);
        image_rect.set_custom_minimum_size(Vector2::new(size, size));
        // A `TextureRect`'s minimum size is its texture size; ignore the source
        // size so the requested cell wins (`b.imageDraw(...).size(iconMed)`).
        image_rect.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
        image_rect
            .set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_CENTERED);
        return Some(image_rect.upcast());
    }
    if type_name == "sector" || type_name == "sectorpreset" {
        // `SectorPreset.uiIcon` falls back to `Icon.terrain`.
        return Some(glyph_label("terrain", size as i32, Color::WHITE).upcast());
    }
    None
}

/// Autowrap mode used by wrapped Arc labels.
#[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
pub const WRAP_WORD_SMART: godot::classes::text_server::AutowrapMode =
    godot::classes::text_server::AutowrapMode::WORD_SMART;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_alignment_maps_like_box_container() {
        // The GDScript passed `BoxContainer.ALIGNMENT_*` ints; keep the mapping.
        assert_eq!(
            WRAP_WORD_SMART,
            godot::classes::text_server::AutowrapMode::WORD_SMART
        );
    }

    #[test]
    fn text_button_minimum_uses_the_single_line_metrics() {
        // The icon cell and the unwrapped label width add up; the row height is
        // the taller cell (`PAR-0433`).
        let min = text_button_row_minimum(Vector2::new(30.0, 36.0), Vector2::new(120.0, 18.0));
        assert_eq!(min, Vector2::new(150.0, 36.0));
        // A label wider than the icon dominates the width, not the height.
        let min = text_button_row_minimum(Vector2::new(30.0, 36.0), Vector2::new(200.0, 18.0));
        assert_eq!(min, Vector2::new(230.0, 36.0));
    }
}
