// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! Small dialogs: About, Credits, Discord, FullText.

use godot::classes::{ColorRect, Control, DisplayServer, Os, RichTextLabel};
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

impl DialogController for DiscordDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@discord");
        dialog.should_pause = false;
        let mut table = dialog.content_table();
        let banner = factory::label(&format!("[accent]{}", kit::t("@discord")));
        rich(&banner).set_horizontal_alignment(godot::global::HorizontalAlignment::LEFT);
        table.bind_mut().add(&banner).grow_x().pad(12.0);
        table.bind_mut().row();
        dialog.add_close_button(210.0);
        // code-instantiated: the button row is composed per dialog.
        if let Some(mut row) = dialog.buttons_row() {
            let mut copy = factory::button(&kit::t("@copylink").to_string());
            let callable = Callable::from_fn("copy_link", |_args: &[&Variant]| {
                DisplayServer::singleton().clipboard_set(DISCORD_URL);
                if let Some(mut ui) = kit::ui() {
                    let _ = ui.call("show_info", &[kit::t("@copied").to_variant()]);
                }
            });
            copy.connect("pressed", &callable);
            row.add_child(&copy);
            let mut open = factory::button(&kit::t("@openlink").to_string());
            let callable = Callable::from_fn("open_link", |_args: &[&Variant]| {
                if Os::singleton().shell_open(DISCORD_URL) != godot::global::Error::OK {
                    DisplayServer::singleton().clipboard_set(DISCORD_URL);
                    if let Some(mut ui) = kit::ui() {
                        let _ = ui.call("show_info", &[kit::t("@linkfail").to_variant()]);
                    }
                }
            });
            open.connect("pressed", &callable);
            row.add_child(&open);
        }
    }
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
