// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/Styles.java (plan 14 §3.3).

//! Builds the single runtime `Theme` from `styles_manifest.json`.
//!
//! `default*` names become type defaults and the rest become theme type
//! variations keyed by the Java field name, so an MSUI `style: "grayt"` and a
//! widget `theme_type_variation` resolve identically. Ninepatch atlas margins
//! replace the flat placeholders at M1 (plan 14 §7d/R7).

use godot::builtin::Side;
use godot::classes::{
    FileAccess, Font, StyleBox, StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, Theme,
};
use godot::obj::NewGd;
use godot::prelude::*;
use mind_core::ui::manifest::StylesManifest;

use super::icon_texture;
use super::styles::{self, GROUP_BASE_TYPES};

const STYLES_MANIFEST: &str = "res://ui/styles_manifest.json";

/// `Styles.defaultPane`: a 24 px 60%-black scroll track; the bar width moves
/// the content (L01 T3 measured Java cards at x=39 vs x=48 with Godot's 8 px
/// default bar).
pub const SCROLL_BAR_WIDTH: f32 = 24.0;
/// `sliderKnob` height (29x42 region): Arc's slider pref height.
pub const SLIDER_ROW_HEIGHT: f32 = 42.0;
/// `Styles.defaultField` vertical padding (Arc 12 + 21 + 12 = 45 px on
/// Godot's 19 px line box).
pub const FIELD_PAD_TOP: f32 = 14.0;
/// `Styles.defaultField` bottom padding.
pub const FIELD_PAD_BOTTOM: f32 = 12.0;

/// An empty manifest (no assets/checked-out manifest in a bare test host).
fn empty_manifest() -> StylesManifest {
    StylesManifest {
        format: 1,
        drawables: Vec::new(),
        text_buttons: Vec::new(),
        buttons: Vec::new(),
        image_buttons: Vec::new(),
        panes: Vec::new(),
        sliders: Vec::new(),
        labels: Vec::new(),
        fields: Vec::new(),
        checks: Vec::new(),
        dialogs: Vec::new(),
        trees: Vec::new(),
    }
}

/// Loads `styles_manifest.json` (an empty manifest when absent/unparseable).
pub fn load_manifest() -> StylesManifest {
    if !FileAccess::file_exists(STYLES_MANIFEST) {
        return empty_manifest();
    }
    let text = FileAccess::get_file_as_string(STYLES_MANIFEST).to_string();
    StylesManifest::from_json(&text).unwrap_or_else(|error| {
        log::warn!("[ui] styles_manifest.json parse failed: {error}");
        empty_manifest()
    })
}

/// Builds the UI theme (`AssetsReadyEvent`).
pub fn build() -> Gd<Theme> {
    let mut theme = Theme::new_gd();
    theme.set_default_font_size(18);
    if let Some(assets) = super::assets()
        && let Some(font) = assets.bind().default_font()
    {
        theme.set_default_font(&(font.upcast::<Font>()));
    }
    apply_base_styles(&mut theme);
    apply_manifest(&mut theme, &load_manifest());
    theme
}

/// Boot check: every manifest name resolves to a theme type (plan 14 §7a.3).
pub fn verify(theme: &Gd<Theme>, manifest: &StylesManifest) -> bool {
    let mut ok = true;
    let types = theme.get_type_list();
    for (_, base) in GROUP_BASE_TYPES {
        let kind = styles::kind_for_base_type(base);
        let names = match kind {
            "drawables" => &manifest.drawables,
            "text_buttons" => &manifest.text_buttons,
            "buttons" => &manifest.buttons,
            "image_buttons" => &manifest.image_buttons,
            "panes" => &manifest.panes,
            "sliders" => &manifest.sliders,
            "labels" => &manifest.labels,
            "fields" => &manifest.fields,
            "checks" => &manifest.checks,
            "dialogs" => &manifest.dialogs,
            _ => &manifest.trees,
        };
        for name in names {
            if !types.contains(&GString::from(name.as_str())) {
                log::warn!("[ui] style not registered: {name}");
                ok = false;
            }
        }
    }
    ok
}

/// Flat stylebox with the standard content margins.
fn flat(color: Color) -> Gd<StyleBoxFlat> {
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(color);
    style.set_content_margin(Side::LEFT, 8.0);
    style.set_content_margin(Side::RIGHT, 8.0);
    style.set_content_margin(Side::TOP, 4.0);
    style.set_content_margin(Side::BOTTOM, 4.0);
    style.set_corner_radius_all(3);
    style
}

/// `whiteui` tint drawable: a flat box with square corners.
fn square(mut style: Gd<StyleBoxFlat>) -> Gd<StyleBoxFlat> {
    style.set_corner_radius_all(0);
    style
}

/// A packed 9-patch region with uniform texture margins (the pack `splits`).
fn nine_patch(region: &str, margin: f32, content_margin: Option<f32>) -> Gd<StyleBox> {
    let Some(texture) = icon_texture(region) else {
        return flat(Color::from_rgba(0.08, 0.08, 0.10, 1.0)).upcast::<StyleBox>();
    };
    let mut style = StyleBoxTexture::new_gd();
    style.set_texture(&(texture));
    style.set_texture_margin(Side::LEFT, margin);
    style.set_texture_margin(Side::TOP, margin);
    style.set_texture_margin(Side::RIGHT, margin);
    style.set_texture_margin(Side::BOTTOM, margin);
    if let Some(content_margin) = content_margin {
        style.set_content_margin_all(content_margin);
    }
    style.upcast::<StyleBox>()
}

/// `Tex.button*` nine-patch (12 px texture margins on all four edges).
fn button_nine_patch(region: &str) -> Gd<StyleBox> {
    nine_patch(region, 12.0, None)
}

/// `Tex.buttonSelect` nine-patch (accent border, splits 4).
fn checked_frame() -> Gd<StyleBox> {
    let Some(texture) = icon_texture("button-select") else {
        return flat(Color::from_rgba(0.13, 0.15, 0.18, 1.0)).upcast::<StyleBox>();
    };
    let mut style = StyleBoxTexture::new_gd();
    style.set_texture(&(texture));
    style.set_texture_margin(Side::LEFT, 4.0);
    style.set_texture_margin(Side::TOP, 4.0);
    style.set_texture_margin(Side::RIGHT, 4.0);
    style.set_texture_margin(Side::BOTTOM, 4.0);
    style.upcast::<StyleBox>()
}

/// `Styles.flatDown`: the `flat-down-base` 9-patch (4 px accent border around
/// a black fill) with zero padding.
fn flat_down_frame() -> Gd<StyleBox> {
    let Some(texture) = icon_texture("flat-down-base") else {
        let mut style = flat(Color::BLACK);
        style.set_border_color(styles::ACCENT);
        style.set_border_width_all(4);
        style.set_content_margin_all(0.0);
        return style.upcast::<StyleBox>();
    };
    let mut style = StyleBoxTexture::new_gd();
    style.set_texture(&(texture));
    style.set_texture_margin(Side::LEFT, 12.0);
    style.set_texture_margin(Side::TOP, 12.0);
    style.set_texture_margin(Side::RIGHT, 12.0);
    style.set_texture_margin(Side::BOTTOM, 12.0);
    style.set_content_margin_all(0.0);
    style.upcast::<StyleBox>()
}

/// A flat stylebox with square corners in the standard hover tint.
fn flat_over() -> Gd<StyleBoxFlat> {
    flat(styles::FLAT_OVER)
}

/// Applies the unvaried base styles shared by every control.
fn apply_base_styles(theme: &mut Gd<Theme>) {
    theme.set_stylebox(
        "panel",
        "PanelContainer",
        &(flat(styles::GRAY_PANEL).upcast::<StyleBox>()),
    );
    theme.set_stylebox(
        "normal",
        "Button",
        &(flat(Color::from_rgba(0.18, 0.20, 0.24, 1.0)).upcast::<StyleBox>()),
    );
    theme.set_stylebox(
        "hover",
        "Button",
        &(flat(Color::from_rgba(0.24, 0.27, 0.32, 1.0)).upcast::<StyleBox>()),
    );
    theme.set_stylebox(
        "pressed",
        "Button",
        &(flat(Color::from_rgba(0.13, 0.15, 0.18, 1.0)).upcast::<StyleBox>()),
    );
    theme.set_stylebox(
        "disabled",
        "Button",
        &(flat(Color::from_rgba(0.12, 0.13, 0.15, 1.0)).upcast::<StyleBox>()),
    );
    // Arc has no keyboard focus frame; an empty one keeps Godot from painting
    // a focus ring on mouse-driven buttons.
    theme.set_stylebox(
        "focus",
        "Button",
        &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
    );
    theme.set_color(
        "font_color",
        "Button",
        Color::from_rgba(0.92, 0.92, 0.92, 1.0),
    );
    theme.set_color("font_hover_color", "Button", Color::WHITE);
    theme.set_color(
        "font_disabled_color",
        "Button",
        Color::from_rgba(0.5, 0.5, 0.5, 1.0),
    );
    theme.set_color(
        "default_color",
        "RichTextLabel",
        Color::from_rgba(0.92, 0.92, 0.92, 1.0),
    );
    theme.set_color(
        "font_color",
        "Label",
        Color::from_rgba(0.92, 0.92, 0.92, 1.0),
    );
    theme.set_stylebox(
        "panel",
        "Panel",
        &(flat(styles::GRAY_PANEL_DARK).upcast::<StyleBox>()),
    );
    apply_scroll_bar(theme);

    // `Styles.areaField`: a `TextArea` on the same `underline` 9-patch as the
    // fields, no box, no focus frame.
    theme.set_stylebox("normal", "TextEdit", &(nine_patch("underline", 12.0, None)));
    theme.set_stylebox(
        "read_only",
        "TextEdit",
        &(nine_patch("underline-disabled", 12.0, None)),
    );
    theme.set_stylebox(
        "focus",
        "TextEdit",
        &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
    );
    theme.set_color("font_color", "TextEdit", Color::WHITE);
    theme.set_color(
        "background_color",
        "TextEdit",
        Color::from_rgba(0.0, 0.0, 0.0, 0.0),
    );
}

/// `Styles.defaultPane` scroll bar: a 24 px track and a #454545 knob.
fn apply_scroll_bar(theme: &mut Gd<Theme>) {
    let mut track = nine_patch("scroll", 6.0, None);
    track.set_content_margin(Side::LEFT, SCROLL_BAR_WIDTH * 0.5);
    track.set_content_margin(Side::RIGHT, SCROLL_BAR_WIDTH * 0.5);
    track.set_content_margin(Side::TOP, 0.0);
    track.set_content_margin(Side::BOTTOM, 0.0);
    let mut knob: Gd<StyleBox> = flat(styles::FLAT_OVER).upcast::<StyleBox>();
    if let Some(texture) = icon_texture("scroll-knob-vertical-black") {
        let mut style = StyleBoxTexture::new_gd();
        style.set_texture(&(texture));
        knob = style.upcast::<StyleBox>();
    }
    knob.set_content_margin(Side::LEFT, SCROLL_BAR_WIDTH * 0.5);
    knob.set_content_margin(Side::RIGHT, SCROLL_BAR_WIDTH * 0.5);
    for slot in ["scroll", "scroll_focus"] {
        theme.set_stylebox(slot, "VScrollBar", &(track.clone()));
    }
    for slot in ["grabber", "grabber_highlight", "grabber_pressed"] {
        theme.set_stylebox(slot, "VScrollBar", &(knob.clone()));
    }
}

/// Registers every manifest style as a theme type variation.
fn apply_manifest(theme: &mut Gd<Theme>, manifest: &StylesManifest) {
    for (group, base) in GROUP_BASE_TYPES {
        let names = match group {
            "drawables" => &manifest.drawables,
            "text_buttons" => &manifest.text_buttons,
            "buttons" => &manifest.buttons,
            "image_buttons" => &manifest.image_buttons,
            "panes" => &manifest.panes,
            "sliders" => &manifest.sliders,
            "labels" => &manifest.labels,
            "fields" => &manifest.fields,
            "checks" => &manifest.checks,
            "dialogs" => &manifest.dialogs,
            _ => &manifest.trees,
        };
        for name in names {
            theme.add_type(name.as_str());
            theme.set_type_variation(name.as_str(), base);
            style_variation(theme, base, name);
        }
    }
}

/// Per-variation presentation values (the Java `Styles` field bodies).
fn style_variation(theme: &mut Gd<Theme>, base: &str, name: &str) {
    match base {
        "Button" => {
            if name == "nonet" {
                for slot in ["normal", "hover", "pressed", "disabled"] {
                    theme.set_stylebox(slot, name, &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()));
                }
            } else if name == "defaultb" || name == "defaultt" {
                // `Styles.defaultb`/`defaultt`: `Tex.button` with `buttonOver`
                // on hover and `buttonDown` while pressed.
                theme.set_stylebox("normal", name, &(button_nine_patch("button")));
                theme.set_stylebox("hover", name, &(button_nine_patch("button-over")));
                theme.set_stylebox("pressed", name, &(button_nine_patch("button-down")));
                theme.set_stylebox("hover_pressed", name, &(button_nine_patch("button-down")));
                theme.set_stylebox("disabled", name, &(button_nine_patch("button-disabled")));
                theme.set_color("font_color", name, Color::WHITE);
                theme.set_color("font_hover_color", name, Color::WHITE);
                theme.set_color("font_pressed_color", name, Color::WHITE);
                theme.set_color("font_disabled_color", name, styles::ARC_GRAY);
            } else if name == "grayt" {
                // `Styles.grayt`: `up = grayPanel`, `over`/`down = flatOver`,
                // `disabled = grayPanelDark` — square corners.
                theme.set_stylebox(
                    "normal",
                    name,
                    &(square(flat(styles::GRAY_PANEL)).upcast::<StyleBox>()),
                );
                theme.set_stylebox("hover", name, &(square(flat_over()).upcast::<StyleBox>()));
                theme.set_stylebox("pressed", name, &(square(flat_over()).upcast::<StyleBox>()));
                theme.set_stylebox(
                    "disabled",
                    name,
                    &(square(flat(styles::GRAY_PANEL_DARK)).upcast::<StyleBox>()),
                );
            } else if name == "flati" {
                // `Styles.flati`: `up = black`, `over`/`down = flatOver`.
                theme.set_stylebox("normal", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
                theme.set_stylebox("hover", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("pressed", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("disabled", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
            } else if name == "squareTogglei" {
                // `Styles.squareTogglei`: `up = black`, `over = flatOver`,
                // `down`/`checked = flatDown` (the accent frame).
                theme.set_stylebox("normal", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
                theme.set_stylebox("hover", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("pressed", name, &(flat_down_frame()));
                theme.set_stylebox("hover_pressed", name, &(flat_down_frame()));
                theme.set_stylebox("disabled", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
            } else if name == "cleari" {
                // `Styles.cleari`: `up`/`disabled = black6`, `over = flatOver`,
                // `down = flatDown`; image white, light gray when disabled.
                theme.set_stylebox(
                    "normal",
                    name,
                    &(flat(styles::BLACK_6).upcast::<StyleBox>()),
                );
                theme.set_stylebox("hover", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("pressed", name, &(flat_down_frame()));
                theme.set_stylebox(
                    "disabled",
                    name,
                    &(flat(styles::BLACK_6).upcast::<StyleBox>()),
                );
                theme.set_color("icon_normal_color", name, Color::WHITE);
                theme.set_color("icon_disabled_color", name, styles::ARC_LIGHT_GRAY);
            } else if name == "flatt" {
                // `Styles.flatt`: `up = black`, `over`/`down = flatOver`.
                theme.set_stylebox("normal", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
                theme.set_stylebox("hover", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("pressed", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox(
                    "disabled",
                    name,
                    &(flat(styles::GRAY_PANEL_DARK).upcast::<StyleBox>()),
                );
            } else if name == "flatTogglet" {
                // `Styles.flatTogglet`: `up`/`disabled = black`,
                // `over = flatOver`, `down`/`checked = flatDown`; gray disabled
                // text.
                theme.set_stylebox("normal", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
                theme.set_stylebox("hover", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("pressed", name, &(flat_down_frame()));
                theme.set_stylebox("hover_pressed", name, &(flat_down_frame()));
                theme.set_stylebox("disabled", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
                theme.set_color("font_disabled_color", name, styles::ARC_GRAY);
            } else if name == "squareTogglet" {
                // `Styles.squareTogglet`: `up = pane`, `over`/`down`/`checked =
                // flatOver`, `disabled = black`.
                theme.set_stylebox("normal", name, &(nine_patch("pane", 12.0, None)));
                theme.set_stylebox("hover", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("pressed", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("hover_pressed", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("disabled", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
                theme.set_color("font_disabled_color", name, styles::ARC_GRAY);
            } else if name == "fullTogglet" {
                // `Styles.fullTogglet`: `up`/`disabled = black`,
                // `over`/`down`/`checked = flatOver`; gray disabled text.
                theme.set_stylebox("normal", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
                theme.set_stylebox("hover", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("pressed", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("hover_pressed", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("disabled", name, &(flat(Color::BLACK).upcast::<StyleBox>()));
                theme.set_color("font_disabled_color", name, styles::ARC_GRAY);
            } else if name == "selecti" {
                // `Styles.selecti`: `checked = buttonSelect` (the accent
                // 9-patch frame), `up = none`.
                theme.set_stylebox(
                    "normal",
                    name,
                    &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
                );
                theme.set_stylebox(
                    "hover",
                    name,
                    &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
                );
                theme.set_stylebox(
                    "disabled",
                    name,
                    &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
                );
                theme.set_stylebox("pressed", name, &(checked_frame()));
                theme.set_stylebox("hover_pressed", name, &(checked_frame()));
            } else if name == "clearTogglei" {
                // `Styles.clearTogglei`: `up = black6`, `over = flatOver`,
                // `down = checked = flatDown`.
                theme.set_stylebox(
                    "normal",
                    name,
                    &(square(flat(styles::BLACK_6)).upcast::<StyleBox>()),
                );
                theme.set_stylebox("hover", name, &(square(flat_over()).upcast::<StyleBox>()));
                theme.set_stylebox(
                    "disabled",
                    name,
                    &(square(flat(styles::BLACK_6)).upcast::<StyleBox>()),
                );
                theme.set_stylebox("pressed", name, &(flat_down_frame()));
                theme.set_stylebox("hover_pressed", name, &(flat_down_frame()));
            } else if name == "emptyi" {
                // `Styles.emptyi`: no background; white icon, lightGray on
                // hover, accent on down.
                for slot in ["normal", "hover", "pressed", "disabled"] {
                    theme.set_stylebox(slot, name, &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()));
                }
                theme.set_color("icon_normal_color", name, Color::WHITE);
                theme.set_color(
                    "icon_hover_color",
                    name,
                    Color::from_rgba(0.69, 0.69, 0.69, 1.0),
                );
                theme.set_color("icon_pressed_color", name, styles::ACCENT);
                theme.set_color("icon_disabled_color", name, Color::WHITE);
            } else if name == "emptyTogglei" {
                // `Styles.emptyTogglei`: no background; gray icon at rest,
                // white while checked/down, darkGray when disabled.
                for slot in ["normal", "hover", "pressed", "disabled"] {
                    theme.set_stylebox(slot, name, &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()));
                }
                theme.set_color(
                    "icon_normal_color",
                    name,
                    Color::from_rgba(0.498, 0.498, 0.498, 1.0),
                );
                theme.set_color(
                    "icon_hover_color",
                    name,
                    Color::from_rgba(0.498, 0.498, 0.498, 1.0),
                );
                theme.set_color("icon_pressed_color", name, Color::WHITE);
                theme.set_color(
                    "icon_disabled_color",
                    name,
                    Color::from_rgba(0.247, 0.247, 0.247, 1.0),
                );
            } else if name == "clearNonei" || name == "clearNoneTogglei" {
                // `Styles.clearNonei`/`clearNoneTogglei`: `up = none`,
                // `over = flatOver`, `down`/`checked = flatDown`.
                theme.set_stylebox(
                    "normal",
                    name,
                    &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
                );
                theme.set_stylebox("hover", name, &(flat_over().upcast::<StyleBox>()));
                theme.set_stylebox("pressed", name, &(flat_down_frame()));
                theme.set_stylebox("hover_pressed", name, &(flat_down_frame()));
                theme.set_stylebox(
                    "disabled",
                    name,
                    &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
                );
            } else {
                theme.set_stylebox(
                    "normal",
                    name,
                    &(flat(Color::from_rgba(0.18, 0.20, 0.24, 1.0)).upcast::<StyleBox>()),
                );
                theme.set_stylebox(
                    "hover",
                    name,
                    &(flat(Color::from_rgba(0.24, 0.27, 0.32, 1.0)).upcast::<StyleBox>()),
                );
                theme.set_stylebox(
                    "pressed",
                    name,
                    &(flat(Color::from_rgba(0.13, 0.15, 0.18, 1.0)).upcast::<StyleBox>()),
                );
            }
            // No keyboard focus frame (Arc draws none).
            theme.set_stylebox(
                "focus",
                name,
                &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
            );
            theme.set_color("font_color", name, Color::from_rgba(0.92, 0.92, 0.92, 1.0));
        }
        "PanelContainer" => {
            if name == "fullDialog" || name == "defaultDialog" {
                // `Styles.defaultDialog`/`fullDialog`: `background =
                // windowEmpty` — no visible box.
                let mut empty = StyleBoxEmpty::new_gd();
                empty.set_content_margin(Side::LEFT, 8.0);
                empty.set_content_margin(Side::RIGHT, 8.0);
                empty.set_content_margin(Side::TOP, 7.0);
                empty.set_content_margin(Side::BOTTOM, 8.0);
                theme.set_stylebox("panel", name, &(empty.upcast::<StyleBox>()));
            } else if name == "none" {
                theme.set_stylebox(
                    "panel",
                    name,
                    &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
                );
            } else {
                theme.set_stylebox(
                    "panel",
                    name,
                    &(flat(styles::drawable_color(name)).upcast::<StyleBox>()),
                );
            }
        }
        "RichTextLabel" => {
            theme.set_color(
                "default_color",
                name,
                Color::from_rgba(0.92, 0.92, 0.92, 1.0),
            );
            if name == "outlineLabel" {
                // `Styles.outlineLabel`: white `Fonts.outline` (2 px darkGray
                // border). Godot's stroke radius is `outline_size / 4` px, so 8
                // gives Java's 2 px border.
                theme.set_color("default_color", name, Color::WHITE);
                theme.set_color("font_outline_color", name, styles::ARC_DARK_GRAY);
                theme.set_constant("outline_size", name, 8);
            }
        }
        "HSlider" => {
            if name == "defaultSlider" {
                // `Styles.defaultSlider`: the `sliderBack` 9-patch track with
                // the `sliderKnob*` grabbers; no filled progress area.
                let mut back = nine_patch("slider-back", 4.0, Some(4.0));
                back.set_content_margin(Side::TOP, SLIDER_ROW_HEIGHT * 0.5);
                back.set_content_margin(Side::BOTTOM, SLIDER_ROW_HEIGHT * 0.5);
                theme.set_stylebox("slider", name, &(back));
                theme.set_stylebox(
                    "grabber_area",
                    name,
                    &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
                );
                theme.set_stylebox(
                    "grabber_area_highlight",
                    name,
                    &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
                );
                if let Some(knob) = icon_texture("slider-knob") {
                    theme.set_icon("grabber", name, &(knob.clone()));
                    theme.set_icon("grabber_disabled", name, &(knob));
                }
                if let Some(knob_over) = icon_texture("slider-knob-over") {
                    theme.set_icon("grabber_highlight", name, &(knob_over));
                }
            } else {
                theme.set_stylebox(
                    "slider",
                    name,
                    &(flat(Color::from_rgba(0.12, 0.13, 0.15, 1.0)).upcast::<StyleBox>()),
                );
                theme.set_stylebox(
                    "grabber_area",
                    name,
                    &(flat(styles::ACCENT).upcast::<StyleBox>()),
                );
            }
        }
        "LineEdit" => {
            // `Styles.defaultField`: the `underline` 9-patch (transparent body,
            // 4 px #454545 bottom rule, 12 px splits).
            for (slot, region) in [("normal", "underline"), ("read_only", "underline-disabled")] {
                let mut field_box = nine_patch(region, 12.0, None);
                field_box.set_content_margin(Side::TOP, FIELD_PAD_TOP);
                field_box.set_content_margin(Side::BOTTOM, FIELD_PAD_BOTTOM);
                theme.set_stylebox(slot, name, &(field_box));
            }
            theme.set_stylebox(
                "focus",
                name,
                &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()),
            );
            theme.set_color("font_color", name, Color::WHITE);
            theme.set_color("font_placeholder_color", name, styles::ARC_GRAY);
        }
        "CheckBox" => {
            // `Styles.defaultCheck`: round `checkOn`/`checkOff` textures with
            // their disabled variants.
            for (slot, region) in [
                ("checked", "check-on"),
                ("unchecked", "check-off"),
                ("checked_disabled", "check-on-disabled"),
                ("unchecked_disabled", "check-disabled"),
            ] {
                if let Some(texture) = icon_texture(region) {
                    theme.set_icon(slot, name, &(texture));
                }
            }
            theme.set_color("font_color", name, Color::from_rgba(0.92, 0.92, 0.92, 1.0));
            // Arc `CheckBox` draws no background in any state.
            for slot in [
                "normal",
                "hover",
                "pressed",
                "hover_pressed",
                "disabled",
                "focus",
            ] {
                theme.set_stylebox(slot, name, &(StyleBoxEmpty::new_gd().upcast::<StyleBox>()));
            }
        }
        "Tree" => {
            theme.set_stylebox(
                "panel",
                name,
                &(flat(styles::GRAY_PANEL_DARK).upcast::<StyleBox>()),
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_manifest_has_no_styles() {
        let manifest = empty_manifest();
        assert_eq!(manifest.format, 1);
        assert!(manifest.text_buttons.is_empty());
        assert!(manifest.drawables.is_empty());
    }

    #[test]
    fn field_padding_matches_arc_metrics() {
        // Arc `TextField.getPrefHeight` = 12 + 21 + 12 = 45 px.
        assert_eq!(FIELD_PAD_TOP + 19.0 + FIELD_PAD_BOTTOM, 45.0);
    }
}
