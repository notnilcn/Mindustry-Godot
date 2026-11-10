// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The Discord dialog (`discord`): the invite banner and the copy/open link buttons.
//! Upstream: core/src/mindustry/ui/dialogs/DiscordDialog.java

use godot::builtin::Side;
use godot::classes::{
    Button, ColorRect, Control, DisplayServer, Label, Os, PanelContainer, StyleBox, StyleBoxFlat,
    StyleBoxTexture,
};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, styles};

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "discord" => Some(Box::new(DiscordDialog)),
        _ => None,
    }
}

/// Discord invite banner + copy/open link buttons.
pub struct DiscordDialog;

const DISCORD_URL: &str = "https://discord.gg/mindustry";
/// `DiscordDialog.java:18` `float h = 70f`.
const DISCORD_H: f32 = 70.0;
/// `DiscordDialog.java:22-26` the colour column: 40 px wide, `h - 5` + 5 px.
const DISCORD_COLOR_W: f32 = 40.0;
/// `DiscordDialog.java:45` `.size(520f, h)`.
const DISCORD_BANNER_W: f32 = 520.0;
/// `DiscordDialog.java:47` `buttons.defaults().size(170f, 50f)`.
const DISCORD_BUTTON_W: f32 = 170.0;
const DISCORD_BUTTON_H: f32 = 50.0;
/// `DiscordDialog.java:19-26` `Color.valueOf("7289da")` and its 80 % strip.
const DISCORD_BLUE: Color = Color::from_rgba8(0x72, 0x89, 0xda, 0xff);
const DISCORD_BLUE_DIM: Color = Color::from_rgba8(0x5b, 0x6d, 0xae, 0xff);
/// The icon font is generated at 30 px (`Fonts.java:76`); every `Icon.*`
/// drawable is a glyph rasterized at that size, so the UI draws glyphs at 30
/// regardless of the text label around them.
const ICON_FONT_SIZE: i32 = 30;
/// `Tex.button` nine-patch split: Arc insets a `TextButton`'s content by the
/// background's left/right widths.
const BUTTON_SPLIT: f32 = 12.0;
/// The shell's `defaultDialog` panel content margins (`kit/theme.rs`).
const PANEL_MARGIN_X: f32 = 8.0;
const PANEL_MARGIN_TOP: f32 = 7.0;
const PANEL_MARGIN_BOTTOM: f32 = 8.0;
/// Arc `Dialog.defaults().pad(3)` on each side of the shell's body/buttons rows.
const ROW_GAP: f32 = 6.0;
/// Packed content size. Java's bare `Dialog` keeps an empty 23 px title row, so
/// the fixed content is 552 wide and, with the button row 31 px below the
/// banner box, 206 tall; the port shell centres the panel (top at 221 at
/// 1152x648), and its hidden-title lift puts the body at y=225 — the banner's
/// body-local y is therefore 268 - 225 = 43, which reproduces the reference
/// frame's banner at (316, 268) and buttons at (315, 369).
const BODY_W: f32 = 552.0;
const BODY_H: f32 = 135.0;
const BANNER_X: f32 = 16.0;
const BANNER_Y: f32 = 43.0;
/// Godot centres the font line box in a `Label`; Arc places the baseline from
/// the font metrics, so the 18 px accent text sits 2 px higher here — nudge it
/// back onto the reference ink box (`297..314` at 1152x648).
const TEXT_OFFSET_Y: f32 = 2.0;

impl DialogController for DiscordDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        // `DiscordDialog.java:14`: `super("")` — the dialog is a bare Arc
        // `Dialog`, so it has no title text and (unlike `BaseDialog`) no accent
        // rule; the empty title row only offsets the packed content.
        dialog.set_title_literal("");
        dialog.should_pause = false;
        build_discord(dialog);
    }

    /// `DiscordDialog.java` never calls `closeOnBack`; Arc's modal `Dialog`
    /// consumes escape, so only the `@back` button hides it.
    fn blocks_escape(&self) -> bool {
        true
    }
}

/// Builds the upstream fixed banner cell and the 170x50 link buttons.
fn build_discord(dialog: &mut MindDialog) {
    let panel_w = BODY_W + PANEL_MARGIN_X * 2.0;
    let panel_h = PANEL_MARGIN_TOP + BODY_H + ROW_GAP + DISCORD_BUTTON_H + PANEL_MARGIN_BOTTOM;
    if let Some(mut panel) = dialog.panel() {
        panel.set_custom_minimum_size(Vector2::new(panel_w, panel_h));
    }
    if let Some(mut body) = dialog.body() {
        body.set_custom_minimum_size(Vector2::new(BODY_W, BODY_H));
    }
    let Some(mut content) = dialog.content() else {
        return;
    };
    // code-instantiated: the banner box and the fixed button row are pure
    // upstream geometry (`cont.table(Tex.button, ...)`), so they are built here
    // instead of a static scene.
    let mut stage = Control::new_alloc();
    stage.set_custom_minimum_size(Vector2::new(BODY_W, BODY_H));
    stage.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    content.add_child(&stage);

    let mut banner = PanelContainer::new_alloc();
    banner.set_position(Vector2::new(BANNER_X, BANNER_Y));
    banner.set_size(Vector2::new(DISCORD_BANNER_W, DISCORD_H));
    banner.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    banner.add_theme_stylebox_override("panel", &banner_style());
    stage.add_child(&banner);

    let mut inner = Control::new_alloc();
    inner.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    banner.add_child(&inner);

    // `t.table(img -> { height(h - 5).width(40).color(color); row();
    // height(5).width(40).color(color.mul(0.8f)); }).expandY()`.
    let mut bar = ColorRect::new_alloc();
    bar.set_color(DISCORD_BLUE);
    bar.set_position(Vector2::ZERO);
    bar.set_size(Vector2::new(DISCORD_COLOR_W, DISCORD_H - 5.0));
    bar.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    inner.add_child(&bar);
    let mut strip = ColorRect::new_alloc();
    strip.set_color(DISCORD_BLUE_DIM);
    strip.set_position(Vector2::new(0.0, DISCORD_H - 5.0));
    strip.set_size(Vector2::new(DISCORD_COLOR_W, 5.0));
    strip.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    inner.add_child(&strip);

    // `t.table(i -> i.image(Icon.discord)).size(h).left()`: the 70x70 cell holds
    // the glyph at the icon font's native 30 px (`Icon.discord` is the baked
    // 30 px bitmap, not a scaled 70 px image).
    let mut icon = factory::glyph_label("discord", ICON_FONT_SIZE, Color::WHITE);
    icon.set_position(Vector2::new(DISCORD_COLOR_W, 0.0));
    icon.set_size(Vector2::new(DISCORD_H, DISCORD_H));
    icon.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    inner.add_child(&icon);

    // `t.add("@discord").color(Pal.accent).growX().padLeft(10f)`.
    let mut text = Label::new_alloc();
    text.set_text(&kit::t("@discord"));
    text.add_theme_color_override("font_color", styles::ACCENT);
    text.set_horizontal_alignment(godot::global::HorizontalAlignment::LEFT);
    text.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    text.set_position(Vector2::new(
        DISCORD_COLOR_W + DISCORD_H + 10.0,
        TEXT_OFFSET_Y,
    ));
    text.set_size(Vector2::new(
        DISCORD_BANNER_W - DISCORD_COLOR_W - DISCORD_H - 10.0,
        DISCORD_H,
    ));
    text.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    inner.add_child(&text);

    // `buttons.button("@back", Icon.left, this::hide)` and the two link actions.
    let Some(mut row) = dialog.buttons_row() else {
        return;
    };
    row.add_theme_constant_override("separation", ROW_GAP as i32);
    let close = dialog.to_gd().callable("close_pressed");
    let mut back = link_button("left", &kit::t("@back").to_string());
    back.connect("pressed", &close);
    row.add_child(&back);
    let mut copy = link_button("copy", &kit::t("@copylink").to_string());
    let callable = Callable::from_fn("copy_link", |_args: &[&Variant]| {
        DisplayServer::singleton().clipboard_set(DISCORD_URL);
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("show_info_fade", &[kit::t("@copied").to_variant()]);
        }
    });
    copy.connect("pressed", &callable);
    row.add_child(&copy);
    let mut open = link_button("discord", &kit::t("@openlink").to_string());
    let callable = Callable::from_fn("open_link", |_args: &[&Variant]| {
        if Os::singleton().shell_open(DISCORD_URL) != godot::global::Error::OK {
            DisplayServer::singleton().clipboard_set(DISCORD_URL);
            if let Some(mut ui) = kit::ui() {
                let _ = ui.call(
                    "show_error_key",
                    &[
                        GString::from("@linkfail").to_variant(),
                        GString::new().to_variant(),
                    ],
                );
            }
        }
    });
    open.connect("pressed", &callable);
    row.add_child(&open);
}

/// `Tex.button` drawn as the banner's background with `margin(0)`.
///
/// Arc tables inset children by the background's padding unless the table sets
/// an explicit `margin`; the banner does (`.margin(0)`), so the colour column
/// covers the nine-patch's left border and the icon/text cells keep the
/// upstream 40/70/10 px sizes.
fn banner_style() -> Gd<StyleBox> {
    let Some(texture) = kit::icon_texture("button") else {
        let mut flat = StyleBoxFlat::new_gd();
        flat.set_bg_color(styles::GRAY_PANEL_DARK);
        return flat.upcast();
    };
    let mut style = StyleBoxTexture::new_gd();
    style.set_texture(&texture);
    for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
        style.set_texture_margin(side, BUTTON_SPLIT);
        style.set_content_margin(side, 0.0);
    }
    style.upcast()
}

/// One `buttons.defaults().size(170f, 50f)` link button: a `defaultt`
/// nine-patch with a left icon-font glyph and a centred label.
///
/// The icon+label row is a plain `Control`, not an `HBoxContainer`: a container
/// reports the glyph's 30 px font line box as its minimum height, which would
/// grow the button past the upstream 50 px cell.
fn link_button(icon: &str, text: &str) -> Gd<Button> {
    let mut button = Button::new_alloc();
    button.set_theme_type_variation("defaultt");
    button.set_custom_minimum_size(Vector2::new(DISCORD_BUTTON_W, DISCORD_BUTTON_H));
    // code-instantiated: the icon cell + centred label row of the upstream
    // `TextButton` (`Table.button(text, image, style, image.imageSize())`).
    let mut row = Control::new_alloc();
    row.set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
    row.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    button.add_child(&row);
    let icon_w = ICON_FONT_SIZE as f32;
    let mut glyph = factory::glyph_label(icon, ICON_FONT_SIZE, Color::WHITE);
    glyph.set_position(Vector2::new(BUTTON_SPLIT, 0.0));
    glyph.set_size(Vector2::new(icon_w, DISCORD_BUTTON_H));
    glyph.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    row.add_child(&glyph);
    let mut label = Label::new_alloc();
    label.set_text(text);
    label.add_theme_color_override("font_color", Color::WHITE);
    label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
    label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    label.set_position(Vector2::new(BUTTON_SPLIT + icon_w, 0.0));
    label.set_size(Vector2::new(
        DISCORD_BUTTON_W - BUTTON_SPLIT * 2.0 - icon_w,
        DISCORD_BUTTON_H,
    ));
    label.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
    row.add_child(&label);
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_url_is_upstream() {
        assert_eq!(DISCORD_URL, "https://discord.gg/mindustry");
    }
}
