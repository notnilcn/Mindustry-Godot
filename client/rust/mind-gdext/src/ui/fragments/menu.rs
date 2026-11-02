// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/MenuFragment.java.
//
// Standalone menu. `Logo`/`Version`/`Sidebar`/`Submenu`/`Discord` are declared
// in `menu_fragment.tscn`; this class lays out the logo, builds the desktop
// button tree (upstream `buildDesktop`), fades submenus in and routes each
// button to `MindUi.open_dialog`, an info prompt, or quit. Icons are the
// Mindustry icon-font glyphs (`icon_codes.json`) rendered through the
// `MindAssets` icon font.

//! `MindMenuFragment` — the standalone main menu (`MenuFragment`).
//!
//! 1:1 port of `client/ui/fragments/menu_fragment.gd`: the `MINDISTRY` logo,
//! the left sidebar button tree with icons, the fade-in submenus, the Discord
//! banner and quit. Every user-visible string is a bundle key resolved through
//! [`kit::t`]; each dialog button forwards to the `MindUi` autoload with the
//! same name/args the GDScript used.

use godot::builtin::Side;
use godot::classes::control::{FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::{
    Button, Control, FontFile, GridContainer, HBoxContainer, IControl, Label, PanelContainer,
    StyleBox, StyleBoxFlat, StyleBoxTexture, TextureRect, VBoxContainer,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, icons, styles};

const BUTTON_W: f32 = 230.0;
const BUTTON_H: f32 = 70.0;

/// Upstream `MenuFragment.fadeInMenu`: `Actions.alpha(1f, 0.15f, Interp.fade)`.
const SUBMENU_FADE: f32 = 0.15;

/// One upstream `MenuFragment.MenuButton`.
///
/// The optional fields mirror the former dictionary keys (`has("submenu")`):
/// `submenu` opens the second column, `dialog` opens a manifest dialog, `info`
/// shows a bundle prompt and `action` runs a plain menu action.
struct MenuEntry {
    text: &'static str,
    icon: &'static str,
    submenu: Option<&'static [MenuEntry]>,
    dialog: Option<&'static str>,
    info: Option<&'static str>,
    action: Option<&'static str>,
}

impl MenuEntry {
    /// A leaf/branch entry; the builders below attach what the click does.
    const fn new(text: &'static str, icon: &'static str) -> Self {
        Self {
            text,
            icon,
            submenu: None,
            dialog: None,
            info: None,
            action: None,
        }
    }

    const fn submenu(mut self, entries: &'static [MenuEntry]) -> Self {
        self.submenu = Some(entries);
        self
    }

    const fn dialog(mut self, name: &'static str) -> Self {
        self.dialog = Some(name);
        self
    }

    const fn action(mut self, name: &'static str) -> Self {
        self.action = Some(name);
        self
    }
}

/// `MenuFragment.buildDesktop` -> `@play`.
const PLAY_SUBMENU: &[MenuEntry] = &[
    MenuEntry::new("@campaign", "play").dialog("planet"),
    MenuEntry::new("@joingame", "add").dialog("join"),
    MenuEntry::new("@customgame", "terrain").dialog("custom"),
    MenuEntry::new("@loadgame", "download").dialog("load"),
];

/// `MenuFragment.buildDesktop` -> `@database.button`.
const DATABASE_SUBMENU: &[MenuEntry] = &[
    MenuEntry::new("@schematics", "paste").dialog("schematics"),
    MenuEntry::new("@database", "book").dialog("database"),
    MenuEntry::new("@about.button", "info").dialog("about"),
];

/// Upstream `MenuFragment.desktopButtons` (top level).
const DESKTOP_BUTTONS: &[MenuEntry] = &[
    MenuEntry::new("@play", "play").submenu(PLAY_SUBMENU),
    MenuEntry::new("@database.button", "menu").submenu(DATABASE_SUBMENU),
    MenuEntry::new("@editor", "terrain").dialog("editor_maps"),
    MenuEntry::new("@mods", "book").dialog("mods"),
    MenuEntry::new("@settings", "settings").dialog("settings"),
    MenuEntry::new("@quit", "exit").action("quit"),
];

/// Mobile grid (`MenuFragment.buildMobile`): a flat 3-column set.
const MOBILE_BUTTONS: &[MenuEntry] = &[
    MenuEntry::new("@campaign", "play").dialog("planet"),
    MenuEntry::new("@joingame", "add").dialog("join"),
    MenuEntry::new("@customgame", "terrain").dialog("custom"),
    MenuEntry::new("@loadgame", "download").dialog("load"),
    MenuEntry::new("@editor", "terrain").dialog("editor_maps"),
    MenuEntry::new("@settings", "settings").dialog("settings"),
    MenuEntry::new("@mods", "book").dialog("mods"),
    MenuEntry::new("@quit", "exit").action("quit"),
];

/// `MindMenuFragment` — the standalone menu in `MenuGroup`.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindMenuFragment {
    base: Base<Control>,

    /// `Sidebar/Buttons`.
    buttons: Option<Gd<VBoxContainer>>,
    /// `Submenu`.
    submenu: Option<Gd<PanelContainer>>,
    /// `Submenu/Buttons`.
    submenu_buttons: Option<Gd<VBoxContainer>>,
    /// `MindAssets.icon_font()` (`_icon_font`).
    icon_font: Option<Gd<FontFile>>,
    /// The top-level button whose submenu is open (`_active_button`).
    active_button: Option<Gd<Button>>,
}

#[godot_api]
impl IControl for MindMenuFragment {
    fn ready(&mut self) {
        self.buttons = self.node_as::<VBoxContainer>("Sidebar/Buttons");
        self.submenu = self.node_as::<PanelContainer>("Submenu");
        self.submenu_buttons = self.node_as::<VBoxContainer>("Submenu/Buttons");
        self.icon_font = self.load_icon_font();
        self.apply_version();
        self.setup_discord();
        if self.is_mobile() {
            self.build_mobile();
        } else {
            self.build_desktop();
        }
        self.apply_layout();
        let callable = self.base().callable("apply_layout");
        if let Some(mut viewport) = self.base().get_viewport()
            && !viewport.is_connected("size_changed", &callable)
        {
            viewport.connect("size_changed", &callable);
        }
        if let Some(mut submenu) = self.submenu.clone() {
            submenu.set_visible(false);
        }
    }
}

#[godot_api]
impl MindMenuFragment {
    /// Positions the logo (top-center), version label, sidebar/submenu columns
    /// and the button-size defaults, re-run on viewport resize (`ResizeEvent`).
    #[func]
    fn apply_layout(&mut self) {
        let viewport_w = self.base().get_viewport_rect().size.x;
        let mut logo_bottom = 0.0f32;
        if let Some(mut logo) = self.node_as::<TextureRect>("Logo") {
            // Upstream draws the logo at
            // `min(logo.width * Scl.scl(1), width - Scl.scl(20))`, 6 px below
            // the top edge (MenuFragment.java:114-121). The port has no
            // separate UI scale; the native atlas region is the 768x107 `logo`
            // sprite.
            let texture = kit::icon_texture("logo");
            logo.set_texture(texture.as_ref());
            let mut native_w = 768.0f32;
            let mut native_h = 107.0f32;
            if let Some(texture) = &texture
                && texture.get_width() > 0
            {
                native_w = texture.get_width() as f32;
                native_h = texture.get_height() as f32;
            }
            let w = native_w.min(viewport_w - 20.0);
            let h = w * native_h / native_w;
            logo.set_offset(Side::LEFT, -w * 0.5);
            logo.set_offset(Side::RIGHT, w * 0.5);
            logo.set_offset(Side::TOP, 6.0);
            logo.set_offset(Side::BOTTOM, 6.0 + h);
            logo_bottom = 6.0 + h;
        }
        if let Some(mut version) = self.node_as::<Label>("Version") {
            let top = logo_bottom + 2.0;
            version.set_offset(Side::LEFT, -160.0);
            version.set_offset(Side::RIGHT, 160.0);
            version.set_offset(Side::TOP, top);
            version.set_offset(Side::BOTTOM, top + 24.0);
        }
        let x = viewport_w / 10.0;
        if let Some(mut sidebar) = self.node_as::<PanelContainer>("Sidebar") {
            sidebar.set_offset(Side::LEFT, x);
            sidebar.set_offset(Side::RIGHT, x + BUTTON_W);
        }
        if let Some(mut submenu) = self.submenu.clone() {
            submenu.set_offset(Side::LEFT, x + BUTTON_W);
            submenu.set_offset(Side::RIGHT, x + BUTTON_W * 2.0);
        }
    }

    /// Starts the Discord button (`MenuFragment.java:63-65`).
    #[func]
    fn open_discord(&mut self) {
        self.open("discord");
    }

    /// Shared click body (`MenuFragment.buttons`): a branch entry toggles the
    /// submenu column, a leaf entry fades it out and runs its action.
    #[func]
    fn on_menu_button(&mut self, path: GString, button: Gd<Button>) {
        let path = path.to_string();
        let Some(entry) = resolve_entry(&path) else {
            return;
        };
        match entry.submenu {
            Some(entries) => self.toggle_submenu(&path, entries, &button),
            None => self.run_entry(entry),
        }
    }
}

impl MindMenuFragment {
    /// One `@onready`-style node lookup with the GDScript cast semantics.
    fn node_as<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: Inherits<Node>,
    {
        self.base().get_node_or_null(path)?.try_cast::<T>().ok()
    }

    /// `_is_mobile`: the `MindUi` platform flag (`Vars.mobile`).
    fn is_mobile(&self) -> bool {
        let Some(mut ui) = kit::ui() else {
            return false;
        };
        ui.call("is_mobile", &[]).try_to::<bool>().unwrap_or(false)
    }

    /// `_load_icon_font`: the `MindAssets` icon font (the only `FontFile`).
    fn load_icon_font(&self) -> Option<Gd<FontFile>> {
        kit::assets()?.bind().icon_font()
    }

    /// Applies `_icon_font` through the kit's `Label`/`RichTextLabel` hook.
    ///
    /// Falls back to [`icons::apply_icon_font`], which performs the same
    /// `MindAssets.icon_font()` lookup when the stored font is absent.
    fn apply_icon_font_to(&self, target: &mut Gd<Control>) {
        match self.icon_font.as_ref() {
            Some(font) => icons::apply_font(target, font),
            None => icons::apply_icon_font(target),
        }
    }

    /// `_apply_version`: `Version.java` combined string + build-dependent tint.
    fn apply_version(&mut self) {
        let Some(mut label) = self.node_as::<Label>("Version") else {
            return;
        };
        let info = self.build_info();
        let build = info
            .get("build")
            .and_then(|value| value.try_to::<i64>().ok())
            .unwrap_or(-1);
        // `MenuFragment.java:112`: custom builds (build == -1) draw the combined
        // version string in `#fc8140aa`, stamped builds in `#ffffffba`.
        let color = if build == -1 {
            Color::from_rgba8(0xfc, 0x81, 0x40, 0xaa)
        } else {
            Color::from_rgba8(0xff, 0xff, 0xff, 0xba)
        };
        label.add_theme_color_override("font_color", color);
        let combined = info
            .get("combined")
            .and_then(|value| value.try_to::<GString>().ok())
            .unwrap_or_else(|| GString::from("custom build"));
        label.set_text(&combined);
    }

    /// The embedded `Version.java` build report (`MindPlatform.get_build_info`).
    fn build_info(&self) -> VarDictionary {
        if let Some(mut platform) = kit::node("/root/Spine/MindPlatform")
            && platform.has_method("get_build_info")
            && let Some(info) = platform
                .call("get_build_info", &[])
                .try_to::<VarDictionary>()
                .ok()
        {
            return info;
        }
        let mut fallback = VarDictionary::new();
        fallback.set("build", -1i64);
        fallback.set("combined", "custom build");
        fallback
    }

    /// `_setup_discord`: the banner nine-patch, centred glyph and tooltip.
    fn setup_discord(&mut self) {
        let Some(mut discord) = self.node_as::<Button>("Discord") else {
            return;
        };
        let Some(texture) = kit::icon_texture("discord-banner") else {
            discord.set_visible(false);
            return;
        };
        let mut banner = StyleBoxTexture::new_gd();
        banner.set_texture(&texture);
        discord.add_theme_stylebox_override("normal", &banner);
        discord.add_theme_stylebox_override("hover", &banner);
        discord.add_theme_stylebox_override("pressed", &banner);
        // `MenuFragment.java:63-65`: `button(Icon.discord, style{up =
        // discordBanner})` is an ImageButton — the white `Icon.discord` glyph
        // centred in the 84x45 cell inside `marginTop(9)`/`marginLeft(10)` (the
        // `Icon` label's offsets), the cell flush with the bottom-right corner.
        if let Some(mut icon) = self
            .base()
            .get_node_or_null("Discord/Icon")
            .and_then(|node| node.try_cast::<Label>().ok())
        {
            icon.set_text(&kit::glyph("discord"));
            let mut icon_control: Gd<Control> = icon.clone().upcast();
            self.apply_icon_font_to(&mut icon_control);
        }
        discord.set_tooltip_text(&kit::t("@discord"));
        let callable = self.base().callable("open_discord");
        discord.connect("pressed", &callable);
    }

    /// `_build_desktop`: the left sidebar tree (`MenuFragment.buildDesktop`).
    fn build_desktop(&mut self) {
        let Some(mut container) = self.buttons.clone() else {
            return;
        };
        for (index, entry) in DESKTOP_BUTTONS.iter().enumerate() {
            let mut button = self.make_button(entry, Vector2::new(BUTTON_W, BUTTON_H));
            let path = format!("desktop/{index}");
            let callable = self.bound_callable(&path, &button);
            button.connect("pressed", &callable);
            container.add_child(&button);
        }
    }

    /// Mobile variant (`MenuFragment.buildMobile`): the top-level set in a
    /// centered 3-column grid instead of the left sidebar.
    fn build_mobile(&mut self) {
        if let Some(mut sidebar) = self.node_as::<PanelContainer>("Sidebar") {
            sidebar.set_visible(false);
        }
        // code-instantiated: mobile grid is a runtime layout variant chosen from
        // `Vars.mobile`; the entries reuse the shared desktop set.
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(3);
        grid.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        self.base_mut().add_child(&grid);
        for (index, entry) in MOBILE_BUTTONS.iter().enumerate() {
            let mut button = self.make_button(entry, Vector2::new(150.0, 150.0));
            let path = format!("mobile/{index}");
            let callable = self.bound_callable(&path, &button);
            button.connect("pressed", &callable);
            grid.add_child(&button);
        }
    }

    /// Builds one icon+label menu button.
    ///
    /// code-instantiated: the icon glyph + label row is data-driven from the
    /// button table and has no static scene (the desktop set is `width`/`height`
    /// uniform, upstream `buttons(Table, MenuButton...)`).
    fn make_button(&self, entry: &MenuEntry, button_size: Vector2) -> Gd<Button> {
        let mut button = Button::new_alloc();
        button.set_custom_minimum_size(button_size);
        button.set_focus_mode(FocusMode::NONE);
        button.set_toggle_mode(entry.submenu.is_some());
        button.add_theme_stylebox_override(
            "normal",
            &menu_style(Color::from_rgba(0.0, 0.0, 0.0, 0.0)),
        );
        // `Styles.flatToggleMenut` (`ui/Styles.java:210-219`): `up = clear`,
        // `over = flatOver` (`#454545`) and `down`/`checked = flatDown` (the
        // accent `flat-down-base` frame), so the open submenu's parent stays
        // checked with the accent border (`MenuFragment.java:271,291`) instead
        // of a hover tint.
        button.add_theme_stylebox_override("hover", &menu_style(styles::FLAT_OVER));
        button.add_theme_stylebox_override("pressed", &flat_down_frame());
        button.add_theme_stylebox_override("hover_pressed", &flat_down_frame());
        // code-instantiated: the icon + label row inside the button has no
        // static scene (uniform desktop cells, upstream `button(text, icon)`).
        let mut row = HBoxContainer::new_alloc();
        row.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        row.set_offset(Side::LEFT, 11.0);
        row.add_theme_constant_override("separation", 8);
        row.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&row);
        let mut icon = Label::new_alloc();
        icon.set_text(&kit::glyph(entry.icon));
        let mut icon_control: Gd<Control> = icon.clone().upcast();
        self.apply_icon_font_to(&mut icon_control);
        // `Fonts.java:76`: the icon font is generated at 30 px and `Icon.*`
        // drawables are glyphs rasterized at that size (`Table.button(text,
        // icon, ...)` draws the image at `imageSize()`, not at 26 px).
        icon.add_theme_font_size_override("font_size", 30);
        icon.set_custom_minimum_size(Vector2::new(30.0, 0.0));
        icon.set_horizontal_alignment(HorizontalAlignment::CENTER);
        icon.set_vertical_alignment(VerticalAlignment::CENTER);
        icon.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&icon);
        let mut label = Label::new_alloc();
        label.set_text(&kit::t(entry.text));
        label.set_vertical_alignment(VerticalAlignment::CENTER);
        // Arc's `TextButton` centres its label; the icon sits to the left
        // (`MenuFragment.java:267-271` `.marginLeft(11f)`).
        label.set_horizontal_alignment(HorizontalAlignment::CENTER);
        label.set_h_size_flags(SizeFlags::EXPAND_FILL);
        label.set_mouse_filter(MouseFilter::IGNORE);
        row.add_child(&label);
        button
    }

    /// `_on_menu_button` submenu half: open `entries` under `path`, or close.
    fn toggle_submenu(&mut self, path: &str, entries: &'static [MenuEntry], button: &Gd<Button>) {
        if self.is_active(button) {
            self.hide_submenu();
        } else {
            self.show_submenu(path, entries, button);
        }
    }

    /// `_show_submenu`: rebuild the second column and check the source button.
    fn show_submenu(&mut self, path: &str, entries: &'static [MenuEntry], source: &Gd<Button>) {
        let (Some(mut submenu), Some(mut submenu_buttons)) =
            (self.submenu.clone(), self.submenu_buttons.clone())
        else {
            return;
        };
        for child in submenu_buttons.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        // code-instantiated: the submenu top spacer aligns its first row with
        // the clicked top-level button (upstream `submenu.add().height(...)`).
        let mut spacer = Control::new_alloc();
        spacer.set_custom_minimum_size(Vector2::new(0.0, source.get_position().y));
        submenu_buttons.add_child(&spacer);
        for (index, entry) in entries.iter().enumerate() {
            let mut button = self.make_button(entry, Vector2::new(BUTTON_W, BUTTON_H));
            let child_path = format!("{path}/{index}");
            let callable = self.bound_callable(&child_path, &button);
            button.connect("pressed", &callable);
            submenu_buttons.add_child(&button);
        }
        submenu.set_visible(true);
        // `MenuFragment.fadeInMenu`: Arc fades the column in over 0.15 s.
        factory::fade_in(&mut submenu, SUBMENU_FADE);
        if let Some(mut active) = self.active_button.clone()
            && active.is_instance_valid()
            && !same_object(&active, source)
        {
            active.set_pressed(false);
        }
        self.active_button = Some(source.clone());
        let mut source = source.clone();
        source.set_pressed(true);
    }

    /// `_hide_submenu`: close the column, drop its rows, uncheck the parent.
    fn hide_submenu(&mut self) {
        if let Some(mut submenu) = self.submenu.clone() {
            submenu.set_visible(false);
        }
        if let Some(buttons) = self.submenu_buttons.clone() {
            for child in buttons.get_children().iter_shared() {
                let mut child = child.clone();
                child.queue_free();
            }
        }
        if let Some(mut active) = self.active_button.take()
            && active.is_instance_valid()
        {
            active.set_pressed(false);
        }
    }

    /// Leaf buttons: the menu returns to its base state, then the action runs.
    fn run_entry(&mut self, entry: &MenuEntry) {
        // Upstream `MenuFragment.buttons` fades the open submenu out before
        // running a leaf button's runnable, so the menu is back to its base
        // state behind the dialog.
        self.hide_submenu();
        if let Some(dialog) = entry.dialog {
            self.open(dialog);
        } else if let Some(info) = entry.info {
            self.show_info(info);
        } else if entry.action == Some("quit") {
            self.quit_game();
        }
    }

    /// `_open`: forwards to `MindUi.open_dialog(name, "{}")`.
    fn open(&mut self, dialog_name: &str) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from(dialog_name).to_variant(),
                GString::from("{}").to_variant(),
            ],
        );
    }

    /// `_info`: forwards a bundle prompt to `MindUi.show_info`.
    fn show_info(&mut self, key: &str) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let text = kit::t(key);
        let _ = ui.call("show_info", &[text.to_variant()]);
    }

    /// `_quit`.
    fn quit_game(&mut self) {
        let mut tree = self.base().get_tree();
        tree.quit();
    }

    /// Whether `button` is the row whose submenu is currently open.
    fn is_active(&self, button: &Gd<Button>) -> bool {
        self.active_button
            .as_ref()
            .is_some_and(|active| same_object(active, button))
    }

    /// `Callable.bind(path, button)`: the handler receives the entry path and
    /// the pressed button (the `pressed` signal itself carries no arguments).
    fn bound_callable(&self, path: &str, button: &Gd<Button>) -> Callable {
        let mut arguments = Array::<Variant>::new();
        arguments.push(&path.to_variant());
        arguments.push(&button.to_variant());
        self.base().callable("on_menu_button").bindv(&arguments)
    }
}

/// `_menu_style`: transparent/flat menu row with Arc's `marginLeft(11f)`.
fn menu_style(color: Color) -> Gd<StyleBoxFlat> {
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(color);
    style.set_content_margin(Side::LEFT, 11.0);
    style
}

/// `Styles.flatDown`: the `flat-down-base` 9-patch (4 px accent border around
/// a black fill) with zero padding.
///
/// `kit::theme` owns the canonical builder, but it is module-private and this
/// fragment must not reach into the theme builder's internals.
fn flat_down_frame() -> Gd<StyleBox> {
    if let Some(texture) = kit::icon_texture("flat-down-base") {
        let mut style = StyleBoxTexture::new_gd();
        style.set_texture(&texture);
        style.set_texture_margin(Side::LEFT, 12.0);
        style.set_texture_margin(Side::TOP, 12.0);
        style.set_texture_margin(Side::RIGHT, 12.0);
        style.set_texture_margin(Side::BOTTOM, 12.0);
        style.set_content_margin_all(0.0);
        return style.upcast::<StyleBox>();
    }
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(Color::BLACK);
    style.set_border_color(styles::ACCENT);
    style.set_border_width_all(4);
    style.set_content_margin_all(0.0);
    style.upcast::<StyleBox>()
}

/// Godot object identity (`a == b` in GDScript) without `Gd`'s dead-handle
/// panic on `PartialEq`.
fn same_object(a: &Gd<Button>, b: &Gd<Button>) -> bool {
    a.is_instance_valid() && b.is_instance_valid() && a.instance_id() == b.instance_id()
}

/// Resolves a bound menu path (`desktop/1/2`) back to its static entry.
///
/// The path replaces the GDScript's bound dictionary, so the handler stays one
/// `#[func]` for every column depth (upstream recurses `buttons(submenu, ...)`).
fn resolve_entry(path: &str) -> Option<&'static MenuEntry> {
    let mut parts = path.split('/');
    let table = match parts.next()? {
        "desktop" => DESKTOP_BUTTONS,
        "mobile" => MOBILE_BUTTONS,
        _ => return None,
    };
    let mut entry = table.get(parts.next()?.parse::<usize>().ok()?)?;
    for part in parts {
        entry = entry.submenu?.get(part.parse::<usize>().ok()?)?;
    }
    Some(entry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_resolve_to_the_static_tables() {
        assert_eq!(
            resolve_entry("desktop/0").map(|entry| entry.text),
            Some("@play")
        );
        assert_eq!(
            resolve_entry("desktop/0/0").map(|entry| entry.text),
            Some("@campaign")
        );
        assert_eq!(
            resolve_entry("desktop/1/2").map(|entry| entry.dialog),
            Some(Some("about"))
        );
        assert_eq!(
            resolve_entry("mobile/7").map(|entry| entry.action),
            Some(Some("quit"))
        );
        assert!(resolve_entry("desktop/99").is_none());
        assert!(resolve_entry("desktop/2/0").is_none());
        assert!(resolve_entry("unknown/0").is_none());
    }

    #[test]
    fn every_dialog_entry_names_a_manifest_dialog() {
        for entry in DESKTOP_BUTTONS.iter().chain(MOBILE_BUTTONS.iter()) {
            if let Some(dialog) = entry.dialog {
                assert!(!dialog.is_empty(), "{} has an empty dialog", entry.text);
            }
        }
    }

    #[test]
    fn branches_are_the_two_submenu_parents() {
        let branches = DESKTOP_BUTTONS
            .iter()
            .filter(|entry| entry.submenu.is_some())
            .count();
        assert_eq!(branches, 2);
    }
}
