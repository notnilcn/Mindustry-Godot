// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Small dialogs: About, Credits, Discord, FullText.

use godot::builtin::Side;
use godot::classes::{
    Button, ColorRect, Control, DisplayServer, Label, Os, PanelContainer, RichTextLabel, StyleBox,
    StyleBoxFlat, StyleBoxTexture,
};
use godot::obj::Singleton;
use godot::prelude::*;

use super::super::dialog::{DialogController, MindDialog};
use super::super::kit::widgets::factory;
use super::super::kit::{self, MindLabel, MindTable, styles};

/// The dialog controllers in this batch.
pub fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "about" => Some(Box::new(AboutDialog::default())),
        "credits" => Some(Box::new(CreditsDialog)),
        "discord" => Some(Box::new(DiscordDialog)),
        "full_text" => Some(Box::new(FullTextDialog::default())),
        _ => None,
    }
}

/// A rich label's engine handle (for text/color/alignment).
fn rich(label: &Gd<MindLabel>) -> Gd<RichTextLabel> {
    label.clone().upcast()
}

// ---------------------------------------------------------------------------
// AboutDialog
// ---------------------------------------------------------------------------

/// `Links.getLinks()` entries (name, link, icon-font glyph, color hex).
const LINKS: [(&str, &str, &str, &str); 12] = [
    (
        "discord",
        "https://discord.gg/mindustry",
        "discord",
        "7289da",
    ),
    (
        "changelog",
        "https://github.com/Anuken/Mindustry/releases",
        "list",
        "ffd37f",
    ),
    (
        "trello",
        "https://trello.com/b/aE2tcUwF",
        "trello",
        "026aa7",
    ),
    (
        "wiki",
        "https://mindustrygame.github.io/wiki/",
        "book",
        "0f142f",
    ),
    (
        "suggestions",
        "https://github.com/Anuken/Mindustry-Suggestions/issues/new/choose/",
        "add",
        "ebebeb",
    ),
    (
        "reddit",
        "https://www.reddit.com/r/Mindustry/",
        "reddit-alien",
        "ee593b",
    ),
    (
        "itch.io",
        "https://anuke.itch.io/mindustry",
        "itchio",
        "fa5c5c",
    ),
    (
        "google-play",
        "https://play.google.com/store/apps/details?id=io.anuke.mindustry",
        "googleplay",
        "689f38",
    ),
    (
        "f-droid",
        "https://f-droid.org/packages/io.anuke.mindustry/",
        "android",
        "026aa7",
    ),
    (
        "github",
        "https://github.com/Anuken/Mindustry/",
        "github",
        "24292e",
    ),
    (
        "dev-builds",
        "https://github.com/AnukenMindustryBuilds",
        "github-square",
        "fafbfc",
    ),
    (
        "bug",
        "https://github.com/Anuken/Mindustry/issues/new",
        "wrench",
        "cbd97f",
    ),
];

/// `@about.button` — links grid + credits/close buttons.
#[derive(Default)]
pub struct AboutDialog {
    grid: Option<Gd<MindTable>>,
}

impl DialogController for AboutDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@about.button");
        self.build(dialog);
    }
}

impl AboutDialog {
    fn build(&mut self, dialog: &mut MindDialog) {
        dialog.clear_content();
        // code-instantiated: link rows come from the Links.getLinks() data table,
        // which is not known at scene-authoring time.
        let mut table = MindTable::new_alloc();
        table.bind_mut().grow_horizontal();
        let mut control: Gd<Control> = table.clone().upcast();
        control.add_theme_constant_override("separation", 2);
        if let Some(mut content) = dialog.content() {
            content.add_child(&control);
        }
        self.grid = Some(table);
        for (name, link, icon, _color) in LINKS {
            self.add_link(dialog, name, link, icon);
        }
        dialog.add_close_button(210.0);
        // Upstream `AboutDialog.showCredits` opens the credits dialog.
        let callable = Callable::from_fn("show_credits", |_args: &[&Variant]| {
            if let Some(mut ui) = kit::ui() {
                let _ = ui.call(
                    "open_dialog",
                    &[
                        GString::from("credits").to_variant(),
                        GString::from("{}").to_variant(),
                    ],
                );
            }
        });
        let credits = kit::t("@credits").to_string();
        dialog.add_button(&credits, Some(callable), "", 200.0);
    }

    fn add_link(&mut self, dialog: &mut MindDialog, name: &str, link: &str, icon: &str) {
        let Some(mut grid) = self.grid.clone() else {
            return;
        };
        let mut button = factory::icon_button(
            icon,
            &Self::link_title(name),
            "flatBordert",
            godot::classes::box_container::AlignmentMode::BEGIN,
        );
        button.set_text_alignment(godot::global::HorizontalAlignment::LEFT);
        let url = GString::from(link);
        let callable = Callable::from_fn("open_link", move |_args: &[&Variant]| {
            let error = Os::singleton().shell_open(&url);
            if error != godot::global::Error::OK
                && let Some(mut ui) = kit::ui()
            {
                let _ = ui.call("show_info", &[kit::t("@linkfail").to_variant()]);
            }
        });
        button.connect("pressed", &callable);
        grid.bind_mut().add(&button).grow_x().pad(2.0);
        grid.bind_mut().row();
        let description = factory::label(&kit::t(&format!("@link.{name}.description")).to_string());
        let mut rich_label = rich(&description);
        rich_label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        rich_label.add_theme_color_override("default_color", Color::from_rgba(0.8, 0.8, 0.8, 1.0));
        grid.bind_mut().add(&description).grow_x().pad(1.0);
        grid.bind_mut().row();
        let _ = dialog;
    }

    /// `Links.LinkEntry.title`: bundle override, else `Strings.capitalize`.
    fn link_title(link_name: &str) -> String {
        let key = format!("link.{link_name}.title");
        let value = kit::t(&key).to_string();
        if value != key {
            return value;
        }
        capitalize(&link_name.replace('-', " "))
    }
}

/// Arc `Strings.capitalize`: upper-cases the first character and any character
/// following `_`/`-` (which themselves become spaces).
fn capitalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut upper_next = true;
    for character in text.chars() {
        if character == '_' || character == '-' {
            out.push(' ');
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(character.to_uppercase());
        } else {
            out.push(character);
        }
        upper_next = false;
    }
    out
}

// ---------------------------------------------------------------------------
// CreditsDialog
// ---------------------------------------------------------------------------

/// `@credits` — blurb, accent rule, contributor grid.
pub struct CreditsDialog;

/// Contributor grid columns.
const COLUMNS: usize = 3;

impl DialogController for CreditsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@credits");
        let mut table = dialog.content_table();
        let blurb = factory::label(&kit::tm("@credits.text").to_string());
        let mut blurb_rich = rich(&blurb);
        blurb_rich.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        blurb_rich.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        blurb_rich.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        table
            .bind_mut()
            .add(&blurb)
            .grow_x()
            .pad(12.0)
            .colspan(COLUMNS as i32);
        table.bind_mut().row();
        // code-instantiated: the accent rule between the blurb and the
        // contributors.
        let mut rule = ColorRect::new_alloc();
        rule.set_color(styles::ACCENT);
        rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
        rule.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        table
            .bind_mut()
            .add(&rule)
            .grow_x()
            .pad(3.0)
            .colspan(COLUMNS as i32);
        table.bind_mut().row();
        let header = factory::label(&kit::t("@contributors").to_string());
        rich(&header).add_theme_color_override("default_color", styles::ACCENT);
        table
            .bind_mut()
            .add(&header)
            .grow_x()
            .pad(4.0)
            .colspan(COLUMNS as i32);
        table.bind_mut().row();
        let mut column = 0usize;
        for contributor in contributors() {
            // code-instantiated: contributor rows are data-driven from the
            // packed assets file.
            let label = factory::label(&contributor);
            rich(&label)
                .add_theme_color_override("default_color", Color::from_rgba(0.8, 0.8, 0.8, 1.0));
            table
                .bind_mut()
                .add(&label)
                .align(kit::Align::Left)
                .pad(3.0)
                .pad_left(6.0)
                .pad_right(6.0);
            column += 1;
            if column.is_multiple_of(COLUMNS) {
                table.bind_mut().row();
            }
        }
        if !column.is_multiple_of(COLUMNS) {
            table.bind_mut().row();
        }
        dialog.add_close_button(210.0);
    }
}

/// Contributor names from the packed `contributors` file.
fn contributors() -> Vec<String> {
    let Some(assets) = kit::assets() else {
        return Vec::new();
    };
    let mut node: Gd<Node> = assets.upcast();
    let value = node.call("contributors", &[]);
    let names = value.to::<PackedStringArray>();
    names
        .as_slice()
        .iter()
        .map(|name| name.to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// DiscordDialog
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// FullTextDialog
// ---------------------------------------------------------------------------

/// Scrollable plain-text dialog (`shouldPause = true`).
#[derive(Default)]
pub struct FullTextDialog {
    content: Option<Gd<MindLabel>>,
}

impl DialogController for FullTextDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = true;
        // `FullTextDialog.java`: `super("")` + `addCloseButton()`; the text floats
        // over the defaultDialog backdrop (no boxed panel).
        dialog.enable_full_dialog(true);
        dialog.add_close_button(210.0);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let text = dialog.context_string("text");
        if !text.is_empty() {
            self.set_content(
                dialog,
                &dialog.context_string("title").to_string(),
                &text.to_string(),
            );
        }
        // `PlanetDialog.java:216-222`: the Campaign Changes Notice uses an `@ok`
        // button instead of the generic close.
        if dialog
            .context_value("ok")
            .map(|value| value.to::<bool>())
            .unwrap_or(false)
        {
            dialog.clear_buttons();
            let callable = dialog.to_gd().callable("hide_dialog");
            let ok = kit::t("@ok").to_string();
            dialog.add_button(&ok, Some(callable), "ok", 280.0);
        }
    }
}

impl FullTextDialog {
    /// Sets the title/body and opens the dialog (`show(titleText, text)`).
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn show_full_text(&mut self, dialog: &mut MindDialog, title: &str, text: &str) {
        self.set_content(dialog, title, text);
        dialog.show_dialog();
    }

    fn set_content(&mut self, dialog: &mut MindDialog, title: &str, text: &str) {
        dialog.title_text = GString::from(title);
        dialog.title_color = styles::ACCENT;
        dialog.refresh_title();
        dialog.clear_content();
        // code-instantiated: the body text is data-driven (bundle text supplied
        // by the caller); it cannot be authored in the static scene.
        let label = factory::label(text);
        let mut rich_label = rich(&label);
        rich_label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        rich_label.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        if dialog
            .context_value("ok")
            .map(|value| value.to::<bool>())
            .unwrap_or(false)
        {
            rich_label.set_custom_minimum_size(Vector2::new(500.0, 0.0));
        }
        // `FullTextDialog.java:19`: `.labelAlign(Align.center)`.
        rich_label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        if let Some(mut content) = dialog.content() {
            let control: Gd<Control> = label.clone().upcast();
            content.add_child(&control);
        }
        self.content = Some(label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalize_matches_arc_strings() {
        assert_eq!(capitalize("discord"), "Discord");
        assert_eq!(capitalize("google play"), "Google Play");
        assert_eq!(capitalize("f droid"), "F Droid");
        assert_eq!(capitalize("dev builds"), "Dev Builds");
        assert_eq!(capitalize("itch.io"), "Itch.io");
    }

    #[test]
    fn discord_url_is_upstream() {
        assert_eq!(DISCORD_URL, "https://discord.gg/mindustry");
    }

    #[test]
    fn link_table_is_the_abi() {
        assert_eq!(LINKS.len(), 12);
        assert_eq!(LINKS[0].0, "discord");
        assert_eq!(LINKS[11].0, "bug");
    }
}
