// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/MenuFragment.java (the fill pass)
//         and graphics/MenuRenderer.java.

//! Full-screen menu backdrop (`MenuBackground`).
//!
//! Asks `MindRender.build_menu_texture` for the procedurally generated menu
//! world baked to a texture and darkens it by `MenuRenderer.darkness` (0.3).

use godot::classes::{ITextureRect, Texture2D, TextureRect};
use godot::prelude::*;

use crate::ui::kit;

/// `1 - MenuRenderer.darkness`.
const DARKNESS: f32 = 0.7;

/// `MindMenuBackground` — the baked menu-world backdrop.
#[derive(GodotClass)]
#[class(base=TextureRect, init)]
pub struct MindMenuBackground {
    base: Base<TextureRect>,
}

#[godot_api]
impl ITextureRect for MindMenuBackground {
    fn ready(&mut self) {
        self.base_mut()
            .set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        self.base_mut()
            .set_stretch_mode(godot::classes::texture_rect::StretchMode::KEEP_ASPECT_COVERED);
        self.base_mut()
            .set_texture_filter(godot::classes::canvas_item::TextureFilter::LINEAR);
        self.base_mut()
            .set_modulate(Color::from_rgba(DARKNESS, DARKNESS, DARKNESS, 1.0));
        // Deferred so the renderer/sim autoloads have finished `_ready` and
        // loaded their atlas + content snapshot.
        self.base_mut().call_deferred("generate", &[]);
    }
}

#[godot_api]
impl MindMenuBackground {
    /// Bakes (or re-bakes) the menu world texture.
    #[func]
    pub fn generate(&mut self) {
        let Some(mut render) = self.base().get_node_or_null("/root/Spine/MindRender") else {
            return;
        };
        if !render.has_method("build_menu_texture") {
            return;
        }
        let mobile = kit::ui()
            .and_then(|mut ui| ui.call("is_mobile", &[]).try_to::<bool>().ok())
            .unwrap_or(false);
        let seed = godot::global::randi();
        let generated = render.call(
            "build_menu_texture",
            &[seed.to_variant(), mobile.to_variant()],
        );
        if let Ok(texture) = generated.try_to::<Gd<Texture2D>>() {
            self.base_mut().set_texture(&texture);
        } else {
            log::warn!("[menu] background bake unavailable");
        }
    }
}
