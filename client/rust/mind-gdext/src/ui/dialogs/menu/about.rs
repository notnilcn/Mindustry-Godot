// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The About dialog (`about`): the `Links.getLinks()` grid and the credits/close buttons.
//! Upstream: core/src/mindustry/ui/dialogs/AboutDialog.java

use godot::classes::{Control, Os};
use godot::obj::Singleton;
use godot::prelude::*;

use crate::gdutil::json::empty_object;
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindTable};

use super::util::rich;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "about" => Some(Box::new(AboutDialog::default())),
        _ => None,
    }
}

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
        let mut table = factory::table();
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
        dialog.add_default_close_button();
        // Upstream `AboutDialog.showCredits` opens the credits dialog.
        let callable = Callable::from_fn("show_credits", |_args: &[&Variant]| {
            if let Some(mut ui) = kit::ui() {
                let _ = ui.call(
                    "open_dialog",
                    &[
                        GString::from("credits").to_variant(),
                        empty_object().to_variant(),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Arc `Strings.capitalize` (`arc/util/Strings.java:446-461`) upper-cases
    /// the first character and any character following `_`/`-`; whitespace does
    /// NOT start a new word. `Links.LinkEntry` (`Links.java:52`) calls it on
    /// `name.replace("-", " ")`, so a kebab-case name whose bundle key is
    /// missing renders e.g. "google-play" as "Google play", not "Google Play".
    #[test]
    fn capitalize_matches_arc_strings() {
        assert_eq!(capitalize("discord"), "Discord");
        assert_eq!(capitalize("google play"), "Google play");
        assert_eq!(capitalize("f droid"), "F droid");
        assert_eq!(capitalize("dev builds"), "Dev builds");
        assert_eq!(capitalize("itch.io"), "Itch.io");
        // The separators Arc's doc comment names ("test_string" -> "Test String").
        assert_eq!(capitalize("test_string"), "Test String");
        assert_eq!(capitalize("google-play"), "Google Play");
    }

    #[test]
    fn link_table_is_the_abi() {
        assert_eq!(LINKS.len(), 12);
        assert_eq!(LINKS[0].0, "discord");
        assert_eq!(LINKS[11].0, "bug");
    }
}
