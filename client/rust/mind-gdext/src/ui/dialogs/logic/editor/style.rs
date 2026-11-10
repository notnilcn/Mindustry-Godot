// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The logic editor's `style` support code.
//! Upstream: logic/LCanvas.java

use super::*;

// ---------------------------------------------------------------------------
// region / style helpers
// ---------------------------------------------------------------------------

/// Atlas region lookup cached per dialog (`MindAssets.find_region`).
pub(super) fn region(state: &Rc<RefCell<LogicState>>, region_name: &str) -> Option<Gd<Texture2D>> {
    if let Some(texture) = state.borrow().region_cache.get(region_name) {
        return texture.clone();
    }
    let texture = kit::icon_texture(region_name);
    if texture.is_some() {
        state
            .borrow_mut()
            .region_cache
            .insert(region_name.to_string(), texture.clone());
    }
    texture
}

/// `Tex.whitePane` (5 px white 9-patch border, 12/11 splits) tinted with the
/// element colour; `margin(0f)` drops the drawable padding except the
/// `marginBottom(7)`.
pub(super) fn white_pane_box(state: &Rc<RefCell<LogicState>>, color: Color) -> Gd<StyleBox> {
    if let Some(texture) = region(state, "white-pane") {
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        box_.set_texture_margin(Side::LEFT, 12.0);
        box_.set_texture_margin(Side::TOP, 12.0);
        box_.set_texture_margin(Side::RIGHT, 11.0);
        box_.set_texture_margin(Side::BOTTOM, 11.0);
        box_.set_modulate(color);
        box_.set_content_margin(Side::LEFT, 0.0);
        box_.set_content_margin(Side::TOP, 0.0);
        box_.set_content_margin(Side::RIGHT, 0.0);
        box_.set_content_margin(Side::BOTTOM, 7.0);
        return box_.upcast();
    }
    let mut flat = StyleBoxFlat::new_gd();
    flat.set_draw_center(false);
    flat.set_border_color(color);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        flat.set_border_width(side, 5);
    }
    flat.set_content_margin(Side::BOTTOM, 7.0);
    flat.upcast()
}

/// `Tex.button` 9-patch (12/11 splits) for the Add/Edit content tables.
pub(super) fn button_box(state: &Rc<RefCell<LogicState>>) -> Gd<StyleBox> {
    if let Some(texture) = region(state, "button") {
        let mut box_ = StyleBoxTexture::new_gd();
        box_.set_texture(&texture);
        box_.set_texture_margin(Side::LEFT, 12.0);
        box_.set_texture_margin(Side::TOP, 12.0);
        box_.set_texture_margin(Side::RIGHT, 11.0);
        box_.set_texture_margin(Side::BOTTOM, 11.0);
        return box_.upcast();
    }
    MindDialog::pane_box(Color::BLACK, styles::PANE_OUTLINE_GRAY, 4.0, 2.0, -1.0).upcast()
}
