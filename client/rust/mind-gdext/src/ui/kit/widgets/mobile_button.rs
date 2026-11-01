// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/MobileButton.java (plan 14 §3.3).
//
// Icon button with a wrapped centered label below it (`row()` semantics).

//! `MindMobileButton` — the mobile icon button with a wrapped label.
//!
//! `setup` builds the icon+label column from its data (`MobileButton(icon,
//! text, listener)`); the pressed listener is the `Callable` the caller passes.

use godot::classes::box_container::AlignmentMode;
use godot::classes::control::MouseFilter;
use godot::classes::text_server::AutowrapMode;
use godot::classes::texture_rect::StretchMode;
use godot::classes::{Button, Label, TextureRect, VBoxContainer};
use godot::global::HorizontalAlignment;
use godot::prelude::*;

use super::super::icon_texture;

/// `MobileButton` — `Button` whose content is an icon over a wrapped label.
#[derive(GodotClass)]
#[class(base=Button, init)]
pub struct MindMobileButton {
    base: Base<Button>,
    /// `MobileButton.listener` (`Callable()` until `setup`).
    #[init(val = Callable::invalid())]
    listener: Callable,
}

#[godot_api]
impl MindMobileButton {
    /// Builds the icon + wrapped label (`MobileButton(icon, text, listener)`).
    #[func]
    pub fn setup(&mut self, icon_region: GString, label_text: GString, listener: Callable) {
        self.listener = listener;
        let callable = self.base().callable("on_pressed");
        if !self.base().is_connected("pressed", &callable) {
            self.base_mut().connect("pressed", &callable);
        }
        // code-instantiated: the icon/label row is built from data (region+text)
        // and has no static scene (plan 14 §3.3 MobileButton).
        let mut column = VBoxContainer::new_alloc();
        column.set_alignment(AlignmentMode::CENTER);
        column.set_mouse_filter(MouseFilter::IGNORE);
        self.base_mut().add_child(&column);
        // code-instantiated: the icon frame resolves its atlas region at setup.
        let mut icon_rect = TextureRect::new_alloc();
        if let Some(texture) = icon_texture(&icon_region.to_string()) {
            icon_rect.set_texture(&texture);
        }
        icon_rect.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
        icon_rect.set_custom_minimum_size(Vector2::new(48.0, 48.0));
        column.add_child(&icon_rect);
        // code-instantiated: wrapped caption under the icon.
        let mut label = Label::new_alloc();
        label.set_text(&label_text);
        label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        label.set_mouse_filter(MouseFilter::IGNORE);
        column.add_child(&label);
    }

    /// `_on_pressed`: forwards to the stored listener.
    #[func]
    fn on_pressed(&mut self) {
        if !self.listener.is_valid() {
            return;
        }
        let args = Array::<Variant>::new();
        let _ = self.listener.callv(&args);
    }
}
