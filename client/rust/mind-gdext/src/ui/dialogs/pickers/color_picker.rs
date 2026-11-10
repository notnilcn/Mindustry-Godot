// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The color picker dialog controller. Filled by S10 (docs/PLANS).
//! Upstream: ui/dialogs/ColorPicker.java

use crate::gdutil::variant::{to_bool_int_only, to_text_lenient};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::mods::util::bt;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, styles};
use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::{
    CanvasItem, ColorRect, Control, HSlider, LineEdit, PanelContainer, TextureRect,
};
use godot::prelude::*;
use std::{cell::RefCell, rc::Rc};

/// The `ColorPickerDialog` controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "picker" => Some(Box::new(ColorPickerDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// ColorPickerDialog (`picker`, `ColorPicker.java`)
// ---------------------------------------------------------------------------

/// `const ROW_SIZE := Vector2(370, 44)` (slider row).
const PICKER_ROW_SIZE: Vector2 = Vector2::new(370.0, 44.0);
/// `ColorPicker.java:27-170` preview square (`t.stack(...).size(200f)`).
const PICKER_PREVIEW_SIZE: f32 = 200.0;
/// `Tex.pane` pads its content 12 px (java/picker.png: frame 228 px around the
/// 200 px preview).
const PICKER_FRAME_PAD: f32 = 12.0;
/// Hex field size (`t.field(...).size(130f, 40f)`).
const PICKER_HEX_SIZE: Vector2 = Vector2::new(130.0, 40.0);
/// Hue gradient steps (the 36-quad approximation of `Tex.hue`).
const PICKER_HUE_STEPS: usize = 36;

/// The picker's HSV(A) state, preview/slider handles and the confirmed colour.
struct PickerState {
    /// Hue in degrees (`value.h * 360.0`).
    h: f32,
    s: f32,
    /// Value starts at 1 (`ColorPicker` opens on white).
    v: f32,
    a: f32,
    /// Whether the alpha row is shown (`ColorPicker.show(color, alpha, …)`).
    alpha: bool,
    /// `current` — the live colour.
    current: Color,
    /// The last confirmed colour (native replacement for the `consumer`).
    chosen: Color,
    preview: Option<Gd<ColorRect>>,
    hex: Option<Gd<LineEdit>>,
    sliders: Vec<(&'static str, Gd<HSlider>)>,
    gradients: Vec<Gd<Control>>,
}

impl Default for PickerState {
    fn default() -> Self {
        Self {
            h: 0.0,
            s: 0.0,
            v: 1.0,
            a: 1.0,
            alpha: true,
            current: Color::WHITE,
            chosen: Color::WHITE,
            preview: None,
            hex: None,
            sliders: Vec::new(),
            gradients: Vec::new(),
        }
    }
}

impl PickerState {
    /// One HSV(A) component read out (`{"h": _h, …}[component]`).
    fn component(&self, component: &str) -> f32 {
        match component {
            "h" => self.h,
            "s" => self.s,
            "v" => self.v,
            _ => self.a,
        }
    }
}

/// `@pickcolor` — the HSV(A) colour picker (`ColorPicker.java`).
pub struct ColorPickerDialog {
    state: Rc<RefCell<PickerState>>,
}

impl Default for ColorPickerDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(PickerState::default())),
        }
    }
}

impl DialogController for ColorPickerDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@pickcolor");
        // `should_pause = false` before `super._ready()`.
        dialog.should_pause = false;
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // The last confirmed colour is kept for the caller-less seam: the
        // consumer `Callable` is gone, so `color_selected` carries the result.
        log::debug!(
            "[picker] shown with last confirmed colour {}",
            arc_hex(self.state.borrow().chosen)
        );
        // `show_picker(color, alpha, consumer)` has no native equivalent; the
        // optional context keys carry the same seed values (see module docs).
        if let Some(value) = dialog.context_value("color") {
            let text = to_text_lenient(&value);
            if let Some(color) = parse_html_color(&text) {
                self.set_color(color);
            }
        }
        if let Some(value) = dialog.context_value("alpha") {
            self.state.borrow_mut().alpha = to_bool_int_only(&value);
        }
        build_picker(dialog, &self.state);
    }
}

impl ColorPickerDialog {
    /// `ColorPicker.show(color, …)`: seeds the HSV(A) state directly (the
    /// GDScript `show_picker` entry point; no scene calls it any more, the
    /// dialog context is the native seeding seam).
    fn set_color(&self, color: Color) {
        set_picker_color(&mut self.state.borrow_mut(), color);
    }
}

/// `Color.html(text)` (`#rrggbb` / `#rrggbbaa`); `None` when invalid.
fn parse_html_color(text: &str) -> Option<Color> {
    let trimmed = text.trim();
    let body = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if body.len() != 6 && body.len() != 8 {
        return None;
    }
    Color::from_html(body)
}

/// `set_color(value)`: `current.set(color)` + `toHsv`.
fn set_picker_color(state: &mut PickerState, color: Color) {
    state.current = color;
    match color.normalized().try_to_hsv() {
        Ok(hsv) => {
            state.h = hsv.h * 360.0;
            state.s = hsv.s;
            state.v = hsv.v;
            state.a = hsv.a;
        }
        Err(error) => {
            log::warn!("[picker] HSV conversion failed: {error}");
            state.h = 0.0;
            state.s = 0.0;
            state.v = 1.0;
            state.a = color.a;
        }
    }
}

/// `ColorPicker._build()`.
fn build_picker(dialog: &mut MindDialog, state: &Rc<RefCell<PickerState>>) {
    dialog.clear_content();
    let mut table = dialog.content_table();
    let alpha = state.borrow().alpha;

    // `t.table(Tex.pane, i -> i.stack(alphaBg, colorImage).size(200f))
    // .colspan(2).padBottom(5)`.
    // code-instantiated: the preview frame is rebuilt per open.
    let mut frame = PanelContainer::new_alloc();
    let mut box_ = MindDialog::pane_box(
        Color::from_rgba(0.0, 0.0, 0.0, 0.0),
        styles::PANE_OUTLINE_GRAY,
        4.0,
        2.0,
        -1.0,
    );
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        box_.set_content_margin(side, PICKER_FRAME_PAD);
    }
    frame.add_theme_stylebox_override("panel", &box_);
    // code-instantiated: the alpha backdrop + live colour square stack.
    let mut stack = Control::new_alloc();
    stack.set_custom_minimum_size(Vector2::new(PICKER_PREVIEW_SIZE, PICKER_PREVIEW_SIZE));
    stack.add_child(&picker_alpha_bg("alpha-bg"));
    // code-instantiated: the preview rect is tinted on every change.
    let mut preview = ColorRect::new_alloc();
    preview.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    preview.set_mouse_filter(MouseFilter::IGNORE);
    stack.add_child(&preview);
    frame.add_child(&stack);
    {
        let mut bound = table.bind_mut();
        bound.add(&frame).pad_bottom(5.0);
        bound.row();
    }
    {
        let mut st = state.borrow_mut();
        st.preview = Some(preview);
        st.sliders.clear();
        st.gradients.clear();
    }

    for (component, maximum) in [("h", 360.0_f64), ("s", 1.0), ("v", 1.0)] {
        let value = f64::from(state.borrow().component(component));
        let row = picker_slider_row(state, component, maximum, value, None);
        let mut bound = table.bind_mut();
        bound
            .add(&row)
            .width(PICKER_ROW_SIZE.x)
            .height(PICKER_ROW_SIZE.y)
            .pad_bottom(6.0);
        bound.row();
    }
    if alpha {
        let value = f64::from(state.borrow().a);
        let row = picker_slider_row(state, "a", 1.0, value, Some("alpha-bg-line"));
        let mut bound = table.bind_mut();
        bound
            .add(&row)
            .width(PICKER_ROW_SIZE.x)
            .height(PICKER_ROW_SIZE.y)
            .pad_bottom(6.0);
        bound.row();
    }

    // `hexField = t.field(current.toString(), …).size(130f, 40f)` with
    // `valid(Color.valueOf)`.
    // code-instantiated: the hex field mirrors the HSV state both ways.
    let mut hex = factory::field("");
    hex.set_custom_minimum_size(PICKER_HEX_SIZE);
    {
        let state_for_hex = state.clone();
        let hex_callable = Callable::from_fn("picker_hex", move |args: &[&Variant]| {
            let text = args
                .first()
                .map(|value| to_text_lenient(value))
                .unwrap_or_default();
            {
                let mut st = state_for_hex.borrow_mut();
                picker_on_hex(&mut st, &text);
            }
            Variant::nil()
        });
        hex.connect("text_changed", &hex_callable);
    }
    {
        let mut bound = table.bind_mut();
        bound
            .add(&hex)
            .width(PICKER_HEX_SIZE.x)
            .height(PICKER_HEX_SIZE.y);
    }
    state.borrow_mut().hex = Some(hex);

    dialog.clear_buttons();
    dialog.add_default_close_button();
    // code-instantiated: `@ok` (`Icon.ok`) applies the picked colour.
    let mut ok = factory::icon_button(
        "ok",
        &bt("@ok"),
        "defaultt",
        godot::classes::box_container::AlignmentMode::BEGIN,
    );
    ok.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    {
        let target = dialog.to_gd();
        let state_for_ok = state.clone();
        let ok_callable = Callable::from_fn("picker_ok", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return Variant::nil();
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            picker_confirm(&mut bound, &state_for_ok);
            Variant::nil()
        });
        ok.connect("pressed", &ok_callable);
    }
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&ok);
    }
    picker_update_color(&mut state.borrow_mut(), true);
}

/// `t.stack(<gradient>, slider)`: the gradient fills the 370x44 row and the
/// `defaultSlider` (transparent `sliderBack` body) draws over it.
fn picker_slider_row(
    state: &Rc<RefCell<PickerState>>,
    component: &'static str,
    maximum: f64,
    value: f64,
    backdrop: Option<&str>,
) -> Gd<Control> {
    // code-instantiated: one stacked slider row per HSV(A) component.
    let mut row = Control::new_alloc();
    row.set_custom_minimum_size(PICKER_ROW_SIZE);
    if let Some(region) = backdrop {
        row.add_child(&picker_alpha_bg(region));
    }
    // code-instantiated: the gradient repaints with the current HSV(A) state.
    let mut gradient = Control::new_alloc();
    gradient.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    gradient.set_mouse_filter(MouseFilter::IGNORE);
    {
        let state_for_draw = state.clone();
        let gradient_for_draw = gradient.clone();
        let draw = Callable::from_fn("picker_gradient", move |_args: &[&Variant]| {
            let Ok(st) = state_for_draw.try_borrow() else {
                return Variant::nil();
            };
            picker_draw_gradient(&gradient_for_draw, &st, component);
            Variant::nil()
        });
        gradient.connect("draw", &draw);
    }
    row.add_child(&gradient);
    state.borrow_mut().gradients.push(gradient);
    // code-instantiated: the transparent `defaultSlider` over the gradient.
    let mut slider = factory::slider(0.0, maximum, 0.0001);
    slider.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    slider.set_value(value);
    {
        let state_for_change = state.clone();
        let changed = Callable::from_fn("picker_slider", move |args: &[&Variant]| {
            let number = args
                .first()
                .and_then(|value| value.try_to::<f64>().ok())
                .unwrap_or(0.0) as f32;
            let mut st = state_for_change.borrow_mut();
            picker_on_slider(&mut st, number, component);
            Variant::nil()
        });
        slider.connect("value_changed", &changed);
    }
    row.add_child(&slider);
    state.borrow_mut().sliders.push((component, slider));
    row
}

/// `Fill.quad(x, y, first, x + w, y, second, …)`: a left-to-right gradient of
/// the component (hue: the stepped hue ramp).
fn picker_draw_gradient(target: &Gd<Control>, state: &PickerState, component: &str) {
    let size = target.get_size();
    let mut canvas: Gd<CanvasItem> = target.clone().upcast();
    if component == "h" {
        for index in 0..PICKER_HUE_STEPS {
            let x0 = size.x * index as f32 / PICKER_HUE_STEPS as f32;
            let x1 = size.x * (index + 1) as f32 / PICKER_HUE_STEPS as f32;
            let c0 = Color::from_hsv(index as f64 / PICKER_HUE_STEPS as f64, 1.0, 1.0);
            let c1 = Color::from_hsv((index + 1) as f64 / PICKER_HUE_STEPS as f64, 1.0, 1.0);
            picker_quad(&mut canvas, x0, x1, size.y, c0, c1);
        }
        return;
    }
    let (first, second) = match component {
        "s" => (
            Color::from_hsv(f64::from(state.h) / 360.0, 0.0, f64::from(state.v)),
            Color::from_hsv(f64::from(state.h) / 360.0, 1.0, f64::from(state.v)),
        ),
        "v" => (
            Color::from_hsv(f64::from(state.h) / 360.0, f64::from(state.s), 0.0),
            Color::from_hsv(f64::from(state.h) / 360.0, f64::from(state.s), 1.0),
        ),
        _ => (
            Color::from_rgba(state.current.r, state.current.g, state.current.b, 0.0),
            Color::from_rgba(state.current.r, state.current.g, state.current.b, 1.0),
        ),
    };
    // `first.a = first.a if component == "a" else 1.0`; `second.a = 1.0`.
    let first = if component == "a" {
        first
    } else {
        first.with_alpha(1.0)
    };
    picker_quad(
        &mut canvas,
        0.0,
        size.x,
        size.y,
        first,
        second.with_alpha(1.0),
    );
}

/// `target.draw_polygon(quad, PackedColorArray(c0, c1, c1, c0))`.
fn picker_quad(target: &mut Gd<CanvasItem>, x0: f32, x1: f32, h: f32, c0: Color, c1: Color) {
    let points = PackedVector2Array::from(
        &[
            Vector2::new(x0, 0.0),
            Vector2::new(x1, 0.0),
            Vector2::new(x1, h),
            Vector2::new(x0, h),
        ][..],
    );
    let colors = PackedColorArray::from(&[c0, c1, c1, c0][..]);
    target.draw_polygon(&points, &colors);
}

/// `Tex.alphaBg` / `Tex.alphaBgLine`: the checkerboard under translucent
/// colours.
fn picker_alpha_bg(region: &str) -> Gd<TextureRect> {
    // code-instantiated: the checkerboard tile follows the preview/slider size.
    let mut rect = TextureRect::new_alloc();
    rect.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    rect.set_mouse_filter(MouseFilter::IGNORE);
    rect.set_expand_mode(godot::classes::texture_rect::ExpandMode::IGNORE_SIZE);
    rect.set_stretch_mode(godot::classes::texture_rect::StretchMode::TILE);
    if let Some(texture) = kit::icon_texture(region) {
        rect.set_texture(&texture);
    }
    rect
}

/// `_on_slider(value, component)`.
fn picker_on_slider(state: &mut PickerState, value: f32, component: &str) {
    match component {
        "h" => state.h = value,
        "s" => state.s = value,
        "v" => state.v = value,
        _ => state.a = value,
    }
    picker_update_color(state, true);
}

/// `Color.valueOf(current, value)` on every valid edit; sliders follow.
fn picker_on_hex(state: &mut PickerState, text: &str) {
    let trimmed = text.trim().trim_start_matches('#');
    if trimmed.len() != 6 && trimmed.len() != 8 {
        return;
    }
    if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
        return;
    }
    let Some(parsed) = Color::from_html(trimmed) else {
        return;
    };
    match parsed.normalized().try_to_hsv() {
        Ok(hsv) => {
            state.h = hsv.h * 360.0;
            state.s = hsv.s;
            state.v = hsv.v;
            state.a = hsv.a;
        }
        Err(error) => {
            log::warn!("[picker] hex HSV conversion failed: {error}");
            return;
        }
    }
    for (component, slider) in state.sliders.clone() {
        if !slider.is_instance_valid() {
            continue;
        }
        let mut slider = slider;
        slider.set_value_no_signal(f64::from(state.component(component)));
    }
    picker_update_color(state, false);
}

/// `updateColor(updateField)`: `current.fromHsv(h, s, v); current.a = a`; the
/// hex text drops the alpha byte while opaque.
fn picker_update_color(state: &mut PickerState, update_field: bool) {
    state.current = Color::from_hsv(
        f64::from(state.h) / 360.0,
        f64::from(state.s),
        f64::from(state.v),
    )
    .with_alpha(state.a);
    if let Some(mut preview) = state.preview.clone()
        && preview.is_instance_valid()
    {
        preview.set_color(state.current);
    }
    for gradient in state.gradients.clone() {
        if gradient.is_instance_valid() {
            let mut gradient = gradient;
            gradient.queue_redraw();
        }
    }
    if update_field
        && let Some(mut hex) = state.hex.clone()
        && hex.is_instance_valid()
    {
        let mut value = arc_hex(state.current);
        if state.current.a >= 0.9999 {
            value.truncate(6);
        }
        hex.set_block_signals(true);
        hex.set_text(&GString::from(value.as_str()));
        hex.set_block_signals(false);
    }
}

/// Arc `Color.toString()` / `rgba8888`: each channel is `(int)(c * 255)`
/// (truncated), so a 0.5 alpha is `7f`, where Godot's `to_html` rounds to `80`.
fn arc_rgba(color: Color) -> i64 {
    let channel = |value: f32| -> i64 { (value.clamp(0.0, 1.0) * 255.0) as i64 };
    (channel(color.r) << 24) | (channel(color.g) << 16) | (channel(color.b) << 8) | channel(color.a)
}

/// Arc `Color.toString()` as eight lowercase hex digits.
fn arc_hex(color: Color) -> String {
    format!("{:08x}", arc_rgba(color))
}

/// `_confirm()`: remember + emit the colour, then close.
fn picker_confirm(dialog: &mut MindDialog, state: &Rc<RefCell<PickerState>>) {
    let color = state.borrow().current;
    state.borrow_mut().chosen = color;
    dialog
        .base_mut()
        .emit_signal("color_selected", &[color.to_variant()]);
    dialog.close();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_rgba_truncates_like_arc() {
        // `(int)(c * 255)` truncation, not rounding: 0.5 alpha is 0x7f.
        assert_eq!(arc_rgba(Color::from_rgba(1.0, 0.0, 0.0, 0.5)), 0xff00007f);
        assert_eq!(
            arc_hex(Color::from_rgba8(0x7f, 0x59, 0x9a, 0xff)),
            "7f599aff"
        );
    }
}
