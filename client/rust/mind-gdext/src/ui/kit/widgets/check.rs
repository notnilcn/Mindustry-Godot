// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Elems.java (`Elems.check`; plan 14 §3.3).

//! `MindCheck` — the fancy check button (`Elems.check`).
//!
//! Not a real checkbox: the background toggles `grayPanel`/`flatOver` and the
//! check/over drawable switches between the `check-on`/`check-off` atlas
//! regions, mirroring `Elems.check`'s dynamic image.

use godot::builtin::Side;
use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::texture_rect::StretchMode;
use godot::classes::{Button, HBoxContainer, IButton, Label, Texture2D, TextureRect};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::obj::NewAlloc;
use godot::prelude::*;

use super::super::icon_texture;

/// `box.margin(10f)` plus the image cell's `padLeft(-4f)` (`Elems.java:24,27`):
/// the indicator starts 6 px into the row, the label ends 10 px before it.
const DEFAULT_INSET_LEFT: f32 = 6.0;
/// The right half of [`DEFAULT_INSET_LEFT`].
const DEFAULT_INSET_RIGHT: f32 = 10.0;
/// `row.add_theme_constant_override("separation", 8)` (`Elems.java:24`).
const ROW_SEPARATION: i32 = 8;
/// The 32 px `checkOn`/`checkOff` indicator cell.
const ICON_SIZE: f32 = 32.0;

/// `MindCheck` — `Elems.check(text, checked, listener)` as a native toggle.
#[derive(GodotClass)]
#[class(rename = RustMindCheck, base=Button)]
pub struct MindCheck {
    base: Base<Button>,
    icon: Option<Gd<TextureRect>>,
    row: Option<Gd<HBoxContainer>>,
    /// Arc `Button.isPressed()` (the pointer is held down on the button),
    /// tracked from `button_down`/`button_up`: Godot has no getter for it in
    /// toggle mode.
    pressing: bool,
    /// Horizontal content inset (default: `Elems.check`'s margin). A port of a
    /// plain Arc `CheckBox` (no table margin) passes `0, 0`.
    inset_left: f32,
    inset_right: f32,
    /// The `toggled`-driven icon refresh, shared by the `toggled`/hover
    /// connections so a repeated [`MindCheck::setup`] cannot stack duplicates.
    icon_refresh: Option<Callable>,
}

#[godot_api]
impl IButton for MindCheck {
    fn init(base: Base<Button>) -> Self {
        Self {
            base,
            icon: None,
            row: None,
            pressing: false,
            inset_left: DEFAULT_INSET_LEFT,
            inset_right: DEFAULT_INSET_RIGHT,
            icon_refresh: None,
        }
    }

    /// Arc `Button.draw()` (`scene/ui/Button.java:218-228`) with
    /// `Styles.grayt` (`ui/Styles.java:157-165`): `disabled` -> `grayPanelDark`,
    /// pressed -> `down` (`flatOver`), over -> `over` (`flatOver`), else `up`
    /// (`grayPanel`). `grayt` has no `checked` drawable, so the checked state
    /// never changes the row background — only the `checkOn`/`checkOff` image
    /// marks it (`Elems.java:26`). Godot's toggle Button draws its
    /// `pressed`/`hover_pressed` box for the checked state (the lighter #454545
    /// row), so the Arc background is painted over the native one here; the
    /// icon row is a child and draws on top.
    fn draw(&mut self) {
        let state = if self.base().is_disabled() {
            "disabled"
        } else if self.pressing || self.base().is_hovered() {
            "hover"
        } else {
            "normal"
        };
        let size = self.base().get_size();
        if let Some(style) = self.base().get_theme_stylebox(state) {
            self.base_mut()
                .draw_style_box(&style, Rect2::new(Vector2::ZERO, size));
        }
    }
}

#[godot_api]
impl MindCheck {
    /// Builds the check (`Elems.check(text, checked, listener)`).
    #[func]
    pub fn setup(&mut self, label_text: GString, is_checked: bool, listener: Callable) {
        self.base_mut().set_toggle_mode(true);
        self.base_mut().set_pressed(is_checked);
        self.base_mut().set_theme_type_variation("grayt");
        // `alignment = HORIZONTAL_ALIGNMENT_LEFT` (the 4.7 `text_alignment`).
        self.base_mut()
            .set_text_alignment(HorizontalAlignment::LEFT);
        // code-instantiated: the icon+label row is data-driven and has no
        // static scene (plan 14 §3.3 `Elems.check`).
        if let Some(mut previous) = self.row.take() {
            self.base_mut().remove_child(&previous);
            previous.queue_free();
        }
        let mut row = HBoxContainer::new_alloc();
        row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        row.set_offset(Side::LEFT, self.inset_left);
        row.set_offset(Side::RIGHT, -self.inset_right);
        row.add_theme_constant_override("separation", ROW_SEPARATION);
        row.set_mouse_filter(MouseFilter::IGNORE);
        self.base_mut().add_child(&row);
        let sync = self.base().callable("sync_row_minimum");
        row.connect("minimum_size_changed", &sync);
        self.row = Some(row.clone());
        let mut icon = TextureRect::new_alloc();
        icon.set_custom_minimum_size(Vector2::new(ICON_SIZE, ICON_SIZE));
        icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        icon.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&icon);
        self.icon = Some(icon.clone());
        let mut label = Label::new_alloc();
        label.set_text(&label_text);
        label.set_vertical_alignment(VerticalAlignment::CENTER);
        label.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&label);
        let button: Gd<Button> = self.to_gd().upcast();
        refresh_icon(&mut icon, &button);
        // A `Button` is not a container, so its children do not contribute to
        // its minimum size; size it from the 32px indicator so every cell that
        // only grows the row leaves the whole row clickable.
        let current = self.base().get_custom_minimum_size();
        self.base_mut().set_custom_minimum_size(Vector2::new(
            current.x,
            current.y.max(icon.get_custom_minimum_size().y),
        ));
        if self.icon_refresh.is_none() {
            let refresh = icon_refresh_callable(icon.clone(), button);
            self.base_mut().connect("toggled", &refresh);
            self.base_mut().connect("mouse_entered", &refresh);
            self.base_mut().connect("mouse_exited", &refresh);
            self.icon_refresh = Some(refresh);
        }
        if listener.is_valid() && !self.base().is_connected("toggled", &listener) {
            self.base_mut().connect("toggled", &listener);
        }
        self.connect_once("button_down", "on_button_down");
        self.connect_once("button_up", "on_button_up");
    }

    /// Sets the checked state; the `toggled` connection refreshes the icon and
    /// notifies the listener exactly as a click would.
    #[func]
    pub fn set_checked(&mut self, value: bool) {
        self.base_mut().set_pressed(value);
        self.refresh_icon();
    }

    /// Horizontal content inset (default: `Elems.check`'s margin).
    #[func]
    pub fn set_content_inset(&mut self, left: f32, right: f32) {
        self.inset_left = left;
        self.inset_right = right;
        if let Some(mut row) = self.row.clone() {
            row.set_offset(Side::LEFT, left);
            row.set_offset(Side::RIGHT, -right);
            self.sync_row_minimum();
        }
    }

    /// The icon + label row is an anchored child, which a Button does not
    /// measure (and Button's native `get_minimum_size` ignores a script
    /// `_get_minimum_size`). Raise `custom_minimum_size` to the row's minimum
    /// so a check cell that does not grow keeps its content width (Arc sizes
    /// `Elems.check`/`CheckBox` to the image + label, L01 T3).
    #[func]
    pub fn sync_row_minimum(&mut self) {
        let Some(row) = self.row.clone() else {
            return;
        };
        if !row.is_instance_valid() {
            return;
        }
        let row_min = row.get_combined_minimum_size();
        let current = self.base().get_custom_minimum_size();
        let inset = self.inset_left + self.inset_right;
        self.base_mut().set_custom_minimum_size(Vector2::new(
            current.x.max(row_min.x + inset),
            current.y.max(row_min.y),
        ));
    }

    /// Arc `Button.isPressed()` press edge (the row redraws with `flatOver`).
    #[func]
    pub fn on_button_down(&mut self) {
        self.pressing = true;
        self.base_mut().queue_redraw();
    }

    /// Arc `Button.isPressed()` release edge.
    #[func]
    pub fn on_button_up(&mut self) {
        self.pressing = false;
        self.base_mut().queue_redraw();
    }
}

impl MindCheck {
    /// Re-reads the indicator texture for the current checked/hover state.
    fn refresh_icon(&mut self) {
        let Some(mut icon) = self.icon.clone() else {
            return;
        };
        let button: Gd<Button> = self.to_gd().upcast();
        refresh_icon(&mut icon, &button);
    }

    /// Connects `method` to `signal` once (the GDScript `is_connected` guard).
    fn connect_once(&mut self, signal: &str, method: &str) {
        let callable = self.base().callable(method);
        if !self.base().is_connected(signal, &callable) {
            self.base_mut().connect(signal, &callable);
        }
    }
}

/// Sets the `check-on`/`check-off` (over variants while hovered) indicator.
fn refresh_icon(icon: &mut Gd<TextureRect>, button: &Gd<Button>) {
    let pressed = button.is_pressed();
    let mut region = if pressed { "check-on" } else { "check-off" };
    if button.is_inside_tree() && button.is_hovered() {
        region = if pressed {
            "check-on-over"
        } else {
            "check-over"
        };
    }
    match icon_texture(region) {
        Some(texture) => icon.set_texture(&texture),
        None => icon.set_texture(Gd::<Texture2D>::null_arg()),
    }
}

/// The `toggled`/hover icon refresh as a custom callable.
///
/// It deliberately captures the icon and the button instead of binding
/// `MindCheck`: `set_checked` writes `button_pressed`, and the engine emits
/// `toggled` synchronously from that write, so a handler that re-binds the
/// check would panic on the active `&mut self` borrow.
fn icon_refresh_callable(icon: Gd<TextureRect>, button: Gd<Button>) -> Callable {
    Callable::from_fn("mind_check_refresh_icon", move |_args: &[&Variant]| {
        refresh_icon(&mut icon.clone(), &button);
    })
}
