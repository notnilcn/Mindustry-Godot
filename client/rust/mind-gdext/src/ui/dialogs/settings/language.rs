// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The language dialog (`language`): the locale list with native display names.
//! Upstream: core/src/mindustry/ui/dialogs/LanguageDialog.java

use godot::classes::control::{FocusMode, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::{Button, ButtonGroup, Control, StyleBoxEmpty, StyleBoxFlat};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::{self, MindScroll, MindTable, styles};

use super::util::*;

pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "language" => Some(Box::new(LanguageController)),
        _ => None,
    }
}

/// `LanguageDialog.displayNames`: locale code -> native name (parity ABI).
const DISPLAY_NAMES: [(&str, &str); 36] = [
    ("ca", "Català"),
    ("id_ID", "Bahasa Indonesia"),
    ("da", "Dansk"),
    ("de", "Deutsch"),
    ("et", "Eesti"),
    ("en", "English"),
    ("es", "Español"),
    ("eu", "Euskara"),
    ("fil", "Filipino"),
    ("fr", "Français"),
    ("it", "Italiano"),
    ("lt", "Lietuvių"),
    ("hu", "Magyar"),
    ("nl", "Nederlands"),
    ("nl_BE", "Nederlands (België)"),
    ("pl", "Polski"),
    ("pt_BR", "Português (Brasil)"),
    ("pt_PT", "Português (Portugal)"),
    ("ro", "Română"),
    ("fi", "Suomi"),
    ("sv", "Svenska"),
    ("vi", "Tiếng Việt"),
    ("tk", "Türkmen dili"),
    ("tr", "Türkçe"),
    ("cs", "Čeština"),
    ("be", "Беларуская"),
    ("bg", "Български"),
    ("ru", "Русский"),
    ("sr", "Српски"),
    ("uk_UA", "Українська"),
    ("th", "ไทย"),
    ("zh_CN", "简体中文"),
    ("zh_TW", "正體中文"),
    ("ja", "日本語"),
    ("ko", "한국어"),
    ("router", "router"),
];

/// Meta key holding `_buttons` (locale code -> row `Button`).
const LANGUAGE_BUTTONS_META: &str = "mind_language_buttons";
/// Meta key holding `_current_locale`.
const LANGUAGE_CURRENT_META: &str = "mind_language_current";

/// `@settings.language` — the locale list with native display names.
///
/// The available locale set comes from `MindAssets.locales()`; the list order
/// matches `Vars.java:315-327` (sorted by display name). A click writes
/// `locale` through `MindUi.settings_put_text` and shows `@language.restart`
/// (`LanguageDialog.java:78-84`); the bundle is selected at boot.
pub struct LanguageController;

impl DialogController for LanguageController {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@settings.language");
        dialog.should_pause = false;
        // `BaseDialog` ctor `setFillParent(true)` over `Styles.defaultDialog`:
        // the rows float over the dimmed stage (no boxed panel).
        dialog.enable_full_dialog(true);
        dialog.clear_content();
        language_build(dialog);
        let mut back = dialog.add_close_button(210.0);
        back.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        // The per-frame `update(t -> t.setChecked(...))` in Java; the port
        // re-syncs the checked row whenever the dialog opens. Deferred: the
        // stored-setting read must not re-enter a `MindUi` mutable borrow.
        let target = dialog.to_gd();
        let callable =
            Callable::from_fn("language_refresh_checked", move |_args: &[&Variant]| {
                if !target.is_instance_valid() {
                    return;
                }
                let mut node = target.clone();
                let mut bound = node.bind_mut();
                language_refresh_checked(&mut bound);
            });
        callable.call_deferred(&[]);
    }
}

/// `LanguageDialog.getDisplayName` — falls back to the raw code.
fn language_display_name(locale: &str) -> String {
    let key = locale.replace("in_ID", "id_ID");
    DISPLAY_NAMES
        .iter()
        .find(|(code, _)| *code == key)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or(key)
}

/// `LanguageDialog.getLocale`: the stored `locale` setting, else the live
/// locale resolved by `MindAssets`, else `en`.
fn language_current_locale() -> String {
    if let Some(mut ui) = kit::ui()
        && ui.has_method("settings_get_text")
    {
        let stored = ui
            .call(
                "settings_get_text",
                &[
                    GString::from("locale").to_variant(),
                    GString::from("default").to_variant(),
                ],
            )
            .to::<GString>();
        if !stored.is_empty() && stored != "default" {
            return stored.to_string();
        }
    }
    if let Some(assets) = kit::assets() {
        let live = assets.bind().locale();
        if !live.is_empty() {
            return live.to_string();
        }
    }
    "en".to_string()
}

/// `Vars.init`'s `Vars.locales` (`Vars.java:315-327`): the `assets/locales`
/// list sorted by display name (case-insensitive) with the `router`
/// pseudo-locale appended when absent.
fn language_available_locales() -> Vec<String> {
    let mut codes: Vec<String> = Vec::new();
    if let Some(assets) = kit::assets() {
        let locales = assets.bind().locales();
        for code in locales.as_slice() {
            codes.push(code.to_string());
        }
    }
    if codes.is_empty() {
        for (code, _) in DISPLAY_NAMES {
            codes.push(code.to_string());
        }
    }
    if !codes.iter().any(|code| code == "router") {
        codes.push("router".to_string());
    }
    codes.sort_by(|a, b| {
        language_display_name(a)
            .to_lowercase()
            .cmp(&language_display_name(b).to_lowercase())
    });
    codes
}

/// `Styles.flatTogglet` (`Styles.java:190-199`): `up = black`,
/// `over = flatOver`, `down`/`checked` framed in `Pal.accent`.
fn language_apply_flat_toggle_style(button: &mut Gd<Button>) {
    let up = language_flat_box(Color::BLACK, 0);
    let over = language_flat_box(styles::FLAT_OVER, 0);
    let down = language_flat_box(Color::BLACK, 4);
    button.add_theme_stylebox_override("normal", &up);
    button.add_theme_stylebox_override("hover", &over);
    button.add_theme_stylebox_override("pressed", &down);
    button.add_theme_stylebox_override("hover_pressed", &down);
    button.add_theme_stylebox_override("disabled", &up);
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
    for state in [
        "font_color",
        "font_hover_color",
        "font_pressed_color",
        "font_hover_pressed_color",
    ] {
        button.add_theme_color_override(state, Color::WHITE);
    }
    button.add_theme_color_override("font_disabled_color", styles::ARC_GRAY);
}

/// `_flat_box(fill, border)`: a solid fill with the accent frame.
fn language_flat_box(fill: Color, border: i32) -> Gd<StyleBoxFlat> {
    let mut box_ = flat_box(fill);
    box_.set_border_color(styles::ACCENT);
    box_.set_border_width_all(border);
    box_
}

/// `LanguageDialog.setup`: one 400x50 `flatTogglet` row per locale inside a
/// scroll pane, centred between the title and the Back button.
fn language_build(dialog: &mut MindDialog) {
    let current = language_current_locale();
    let mut node = dialog.to_gd();
    set_meta_text(&mut node, LANGUAGE_CURRENT_META, &current);
    node.set_meta(LANGUAGE_BUTTONS_META, &VarDictionary::new().to_variant());
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
    }
    if let Some(mut content) = dialog.content() {
        content.set_v_size_flags(SizeFlags::EXPAND_FILL);
    }
    // code-instantiated: the locale table is filled from the runtime locale set.
    let mut list = MindTable::new_alloc();
    {
        let mut control: Gd<Control> = list.clone().upcast();
        control.set_h_size_flags(SizeFlags::EXPAND_FILL);
    }
    // code-instantiated: the scroll pane wraps the runtime locale table.
    let mut pane = MindScroll::new_alloc();
    {
        let control: Gd<Control> = list.clone().upcast();
        pane.add_child(&control);
    }
    pane.bind_mut().set_scrolling_disabled(true, false);
    pane.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    pane.set_v_size_flags(SizeFlags::EXPAND_FILL);
    if let Some(mut content) = dialog.content() {
        let control: Gd<Control> = pane.clone().upcast();
        content.add_child(&control);
    }
    let group = ButtonGroup::new_gd();
    let mut buttons = VarDictionary::new();
    for locale in language_available_locales() {
        // code-instantiated: locale rows are data-driven from the locale set.
        let mut button = Button::new_alloc();
        let display_name = language_display_name(&locale);
        button.set_text(&GString::from(display_name.as_str()));
        button.set_theme_type_variation("flatTogglet");
        language_apply_flat_toggle_style(&mut button);
        button.set_custom_minimum_size(Vector2::new(400.0, 50.0));
        button.set_toggle_mode(true);
        button.set_button_group(&group);
        button.set_pressed_no_signal(locale == current);
        button.set_focus_mode(FocusMode::NONE);
        let target = dialog.to_gd();
        let locale_for_press = locale.clone();
        let callable = Callable::from_fn("language_select", move |_args: &[&Variant]| {
            if !target.is_instance_valid() {
                return;
            }
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            language_select(&mut bound, &locale_for_press);
        });
        button.connect("pressed", &callable);
        // Upstream `langs.add(button).group(group)...size(400f, 50f).row();
        // the table's 24 px side margins are the cell pads.
        list.bind_mut()
            .add(&button)
            .width(400.0)
            .height(50.0)
            .pad_left(24.0)
            .pad_right(24.0);
        list.bind_mut().row();
        buttons.set(&GString::from(locale.as_str()), &button);
    }
    node.set_meta(LANGUAGE_BUTTONS_META, &buttons.to_variant());
}

/// `LanguageDialog`'s per-frame checked sync (one checked row; the
/// `ButtonGroup` keeps the exclusivity, like upstream).
fn language_refresh_checked(dialog: &mut MindDialog) {
    let current = language_current_locale();
    let mut node = dialog.to_gd();
    set_meta_text(&mut node, LANGUAGE_CURRENT_META, &current);
    if !node.has_meta(LANGUAGE_BUTTONS_META) {
        return;
    }
    let buttons = node.get_meta(LANGUAGE_BUTTONS_META).to::<VarDictionary>();
    for key in buttons.keys_array().iter_shared() {
        let code = key
            .try_to::<GString>()
            .map(|value| value.to_string())
            .unwrap_or_default();
        let Some(value) = buttons.get(code.as_str()) else {
            continue;
        };
        let Ok(mut button) = value.try_to::<Gd<Button>>() else {
            continue;
        };
        if !button.is_instance_valid() {
            continue;
        }
        button.set_pressed_no_signal(code == current);
    }
}

/// `LanguageDialog.java:78-84`: a click on a *different* locale stores
/// `Core.settings.put("locale", code)` and shows `@language.restart` (the
/// running bundle is not swapped until the next boot).
fn language_select(dialog: &mut MindDialog, locale: &str) {
    if locale == language_current_locale() {
        language_refresh_checked(dialog);
        return;
    }
    set_meta_text(&mut dialog.to_gd(), LANGUAGE_CURRENT_META, locale);
    dialog
        .base_mut()
        .emit_signal("locale_selected", &[GString::from(locale).to_variant()]);
    if let Some(mut ui) = kit::ui() {
        let _ = ui.call(
            "settings_put_text",
            &[
                GString::from("locale").to_variant(),
                GString::from(locale).to_variant(),
            ],
        );
        // `ui.showInfo("@language.restart")`: an untitled modal prompt.
        let _ = ui.call(
            "show_text",
            &[
                GString::new().to_variant(),
                kit::t("@language.restart").to_variant(),
            ],
        );
    }
    language_refresh_checked(dialog);
}
