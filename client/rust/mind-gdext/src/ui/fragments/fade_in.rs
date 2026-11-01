// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/FadeInFragment.java (plan 14 §3.5/M2).

//! The boot/landing black (and white) covers (`FadeInFragment`).

use godot::classes::{ColorRect, IColorRect};
use godot::prelude::*;

/// `MindFadeInFragment` — the full-screen cover that fades out.
#[derive(GodotClass)]
#[class(base=ColorRect, init)]
pub struct MindFadeInFragment {
    base: Base<ColorRect>,
}

#[godot_api]
impl IColorRect for MindFadeInFragment {
    fn ready(&mut self) {
        self.base_mut()
            .set_color(Color::from_rgba(0.0, 0.0, 0.0, 1.0));
    }
}

#[godot_api]
impl MindFadeInFragment {
    /// Fades the black cover out (`FadeInFragment`).
    #[func]
    pub fn start(&mut self, #[opt(default = 0.5_f64)] duration: f64) {
        self.fade_out_from(Color::from_rgba(0.0, 0.0, 0.0, 1.0), duration);
    }

    /// `CoreBlock.beginLaunch(false)`: a full-screen white cover fading out
    /// over 35 ticks (`CoreBlock.java:392-396`).
    #[func]
    pub fn start_white(&mut self, #[opt(default = 0.583_333_3_f64)] duration: f64) {
        self.fade_out_from(Color::from_rgba(1.0, 1.0, 1.0, 1.0), duration);
    }

    fn fade_out_from(&mut self, color: Color, duration: f64) {
        self.base_mut().set_color(color);
        self.base_mut().set_visible(true);
        let this: Gd<ColorRect> = self.to_gd().upcast();
        let mut tween = self.base_mut().create_tween();
        tween.tween_property(&this, "color:a", &0.0f32.to_variant(), duration);
        let callable = self.base().callable("finish_fade");
        tween.tween_callback(&callable);
    }

    #[func]
    fn finish_fade(&mut self) {
        self.base_mut().set_visible(false);
    }
}
