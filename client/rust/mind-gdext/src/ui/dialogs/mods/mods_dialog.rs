// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The mods dialog controller. Filled by S10 (docs/PLANS).
//! Upstream: ui/dialogs/ModsDialog.java
//!
//! Dialog controller batch g: `picker`, `mods` and `mod_browser`.
//!
//! | registry id   | controller             | upstream Java           | GDScript                           |
//! |---------------|------------------------|-------------------------|------------------------------------|
//! | `picker`      | [`ColorPickerDialog`]  | `ColorPicker.java`      | `ui/dialogs/color_picker.gd`       |
//! | `mods`        | [`ModsDialog`]         | `ModsDialog.java`       | `ui/dialogs/mods_dialog.gd`        |
//! | `mod_browser` | [`ModBrowserDialog`]   | `ModBrowserDialog.java` | `ui/dialogs/mod_browser_dialog.gd` |
//!
//! `picker` is the `ColorPicker.show(color, alpha, consumer)` seam. The
//! GDScript took a `Callable` consumer and invoked it after emitting
//! `color_selected`; a `Callable` handed to a native controller has no stable
//! lifetime across the GDExtension boundary, so the consumer is dropped: the
//! controller remembers the confirmed colour in [`ColorPickerDialog`]'s
//! `chosen` state and only emits `color_selected(color)`. Callers seed the
//! dialog through the `MindUi.open_dialog` context (`{"color": "#rrggbbaa",
//! "alpha": true}`); without context the controller keeps its previous state,
//! exactly like the GDScript node did between opens. Both context keys are
//! optional.
//!
//! `mods`/`mod_browser` keep every mod read/write on the `MindMods` autoload
//! (`list`/`details`/`errors`/`set_enabled`/`import_mod`/`remove_mod`/
//! `mods_dir`/`requires_reload`/`parse_listing`/`matching_release`/
//! `game_build`/`game_revision` and the `mods_changed`/`mod_error`/
//! `mod_import_progress` signals). Two former cross-dialog node methods have no
//! native `MindDialog` equivalent (this batch must not extend the dialog ABI):
//!
//! * `mods.set_updates(updates)` — the browser writes the
//!   `internal name -> remote version` dictionary to the mods dialog node's
//!   [`MODS_UPDATES_META`] metadata and re-runs its controller; the mods
//!   controller reads that metadata at the start of every `shown()`.
//! * `mod_browser.install_repos(items)` — the mods dialog writes the batch into
//!   the browser node's [`BROWSER_INSTALL_META`] metadata and re-runs its
//!   controller; the browser drains that queue in `shown()` before fetching,
//!   which is where upstream's `install_repos` starts downloading.
//!
//! URL/folder opening goes through `platform::uri::open_uri`
//! (`Core.app.openURI`/`openFolder`), the `.zip`/`.jar` chooser through
//! `platform::dialogs::show_file_dialog` (`FileChooser.open`). The browser's
//! GitHub HTTP flow (listings, repository, release, zipball) uses
//! `HTTPRequest` children; the former `_process` byte-progress poll is a 0.1 s
//! `Timer` because a `DialogController` has no per-frame hook.

use super::mods_detail::{mods_on_confirm, mods_restore_failed};
use super::mods_import::{
    mods_hide_import_dialog, mods_import_file, mods_import_github, mods_on_text_input,
    mods_open_folder, mods_open_url, mods_show_error, mods_show_errors, mods_update_all,
};
use super::mods_list::{
    mods_build_action_row, mods_button_row, mods_defer_fit, mods_errors, mods_fit_pane,
    mods_gray_button, mods_list, mods_rebuild_list, mods_requires_reload, mods_row_label,
};
use super::util::{
    apply_pane_button, bm, bt, build_button_content, defer_action, dict_bool, flat_style,
    mods_node, variant_bool, variant_string,
};
use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, MindLabel, MindScroll, MindTable, styles};
use godot::builtin::Side;
use godot::classes::control::{FocusMode, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::scroll_container::ScrollMode;
use godot::classes::{
    BoxContainer, Button, CenterContainer, ColorRect, Control, HBoxContainer, Label, LineEdit,
    MarginContainer, PanelContainer, ProgressBar, ScrollContainer, StyleBoxEmpty,
};
use godot::global::{HorizontalAlignment, VerticalAlignment};
use godot::prelude::*;
use std::{cell::RefCell, rc::Rc};

/// The `ModsDialog` controller.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "mods" => Some(Box::new(ModsDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// ModsDialog (`mods`, `ModsDialog.java`)
// ---------------------------------------------------------------------------

/// Ported from `Vars.modGuideURL` (`Vars.java:83`).
pub(super) const MOD_GUIDE_URL: &str = "https://mindustrygame.github.io/wiki/modding/1-modding/";
/// `Iconc.file` (`core/assets-raw/fontgen/config.json`, code 61787); not in
/// `MindIcons.CODES`, so the import chooser renders the code directly.
pub(super) const ICON_FILE_CODE: u32 = 61787;
/// `ModsDialog.setup`: `float h = 110f` (mod row height).
pub(super) const ROW_HEIGHT: f32 = 110.0;
/// `mods.set_updates(updates)` replacement: browser-published update versions.
pub(super) const MODS_UPDATES_META: &str = "_mind_mod_updates";
/// Guard so the `MindMods`/`MindUi` signal bindings run once per dialog node.
pub(super) const MODS_SIGNALS_META: &str = "_mind_mods_signals_bound";
/// Guard for the viewport `size_changed` refresh (`onResize(this::setup)`).
pub(super) const MODS_RESIZE_META: &str = "_mind_mods_resize_bound";

/// `ModsDialog` view state (`_root`/`_list`/`_pane`/`_search`/…).
#[derive(Default)]
pub(super) struct ModsState {
    /// `cont` root table (`_root`).
    pub(super) root: Option<Gd<MindTable>>,
    /// The mod pane's table (`pane[0]`).
    pub(super) list: Option<Gd<MindTable>>,
    pub(super) pane: Option<Gd<MindScroll>>,
    pub(super) search: Option<Gd<LineEdit>>,
    pub(super) banner: Option<Gd<MindLabel>>,
    pub(super) errors_button: Option<Gd<Button>>,
    pub(super) progress: Option<Gd<ProgressBar>>,
    /// `@mods.restore` / `@mods.guide` (the ctor shows one or the other).
    pub(super) restore_button: Option<Gd<Button>>,
    pub(super) guide_button: Option<Gd<Button>>,
    /// The dialog's content column (`cont`).
    pub(super) cont: Option<Gd<BoxContainer>>,
    /// The shell's scrolling body (`body`).
    pub(super) body: Option<Gd<ScrollContainer>>,
    /// internal name -> remote version from the browser's listings.
    pub(super) updates: Vec<(String, String)>,
    /// internal name whose details block is expanded (`showMod`).
    pub(super) open_details: String,
    /// pending async-confirm target (`mods.removeMod`).
    pub(super) pending_delete: String,
    /// A `@mod.import.github` text prompt of ours is open.
    pub(super) pending_github: bool,
    /// `boolean anyDisabled` — one per `setup()`, shared by every rebuild.
    pub(super) any_disabled: bool,
    /// The `@mod.import` chooser overlay.
    pub(super) import_overlay: Option<Gd<Control>>,
    /// Internal name whose `@mods.viewcontent` grid is expanded
    /// (`ModsDialog.java:461-481` `currentContent`).
    pub(super) open_content: String,
}

/// `@mods` — the loaded-mod list (`ModsDialog.java`).
#[derive(Default)]
pub struct ModsDialog {
    state: Rc<RefCell<ModsState>>,
}

impl DialogController for ModsDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.set_title_key("@mods");
        dialog.should_pause = false;
        // `BaseDialog` is `setFillParent(true)`: the content floats over the
        // dimmed stage (no boxed panel), like the other fill-parent dialogs.
        dialog.enable_full_dialog(true);
        {
            let mut st = self.state.borrow_mut();
            st.cont = dialog.content();
            st.body = dialog.body();
        }
        mods_build_shell(dialog, &self.state);
        mods_build_buttons(dialog, &self.state);
        mods_build_import_overlay(dialog, &self.state);
        mods_connect(dialog, &self.state);
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        self.state.borrow_mut().updates = mods_read_updates(&dialog.to_gd());
        mods_refresh(&self.state);
    }

    fn hidden(&mut self, _dialog: &mut MindDialog) {
        mods_hide_import_dialog(&self.state);
    }

    /// The `@mod.import` chooser is its own `BaseDialog` with `closeOnBack`
    /// (`addCloseButton`): Esc closes it, not the mods dialog underneath.
    fn blocks_escape(&self) -> bool {
        let overlay = self.state.borrow().import_overlay.clone();
        if let Some(mut overlay) = overlay
            && overlay.is_instance_valid()
            && overlay.is_visible()
        {
            overlay.set_visible(false);
            return true;
        }
        false
    }
}

/// `_build_shell()`: `cont` centered between the title and the button bar.
pub(super) fn mods_build_shell(dialog: &mut MindDialog, state: &Rc<RefCell<ModsState>>) {
    // `onResize(this::setup)`: the lasting connections bind once per node
    // (`ready()` runs again after an extension reload).
    let mut node = dialog.to_gd();
    if node.has_meta(MODS_RESIZE_META) {
        return;
    }
    if let Some(mut body) = dialog.body() {
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
        let slot = state.clone();
        let resized = Callable::from_fn("mods_body_resized", move |_args: &[&Variant]| {
            let slot = slot.clone();
            defer_action("mods_fit_pane", move || {
                if let Ok(mut st) = slot.try_borrow_mut() {
                    mods_fit_pane(&mut st);
                }
            });
            Variant::nil()
        });
        body.connect("resized", &resized);
    }
    if let Some(mut cont) = dialog.content() {
        cont.set_v_size_flags(SizeFlags::EXPAND_FILL);
        cont.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    }
    let Some(mut viewport) = dialog.base().get_viewport() else {
        return;
    };
    let target = node.clone();
    let slot = state.clone();
    let resized = Callable::from_fn("mods_on_resize", move |_args: &[&Variant]| {
        if !target.is_instance_valid() || !target.is_visible() {
            return Variant::nil();
        }
        let slot = slot.clone();
        defer_action("mods_resize_refresh", move || mods_refresh(&slot));
        Variant::nil()
    });
    viewport.connect("size_changed", &resized);
    node.set_meta(MODS_RESIZE_META, &true.to_variant());
}

/// Ctor button bar (`ModsDialog.java:52-66`): `addCloseButton()` (210x64),
/// then `@mods.restore` (Icon.power) while any mod is marked failed, else
/// `@mods.guide` (Icon.link), then `@mods.openfolder` (Icon.link).
pub(super) fn mods_build_buttons(dialog: &mut MindDialog, state: &Rc<RefCell<ModsState>>) {
    let mut back = dialog.add_close_button(210.0);
    back.set_custom_minimum_size(Vector2::new(210.0, 64.0));

    let mut restore = {
        let slot = state.clone();
        let handler = Callable::from_fn("mods_restore", move |_args: &[&Variant]| {
            let slot = slot.clone();
            defer_action("mods_restore_failed", move || mods_restore_failed(&slot));
            Variant::nil()
        });
        mods_add_bar_button(dialog, "@mods.restore", "power", handler)
    };
    let guide = {
        let handler = Callable::from_fn("mods_guide", move |_args: &[&Variant]| {
            mods_open_url(MOD_GUIDE_URL);
            Variant::nil()
        });
        mods_add_bar_button(dialog, "@mods.guide", "link", handler)
    };
    {
        let handler = Callable::from_fn("mods_openfolder", move |_args: &[&Variant]| {
            defer_action("mods_open_folder", mods_open_folder);
            Variant::nil()
        });
        mods_add_bar_button(dialog, "@mods.openfolder", "link", handler);
    }

    restore.set_visible(false);
    let mut st = state.borrow_mut();
    st.restore_button = Some(restore);
    st.guide_button = Some(guide);
}

/// A `buttons.button(text, icon, listener)` cell over the shared button row,
/// drawn like the close button.
pub(super) fn mods_add_bar_button(
    dialog: &mut MindDialog,
    key: &str,
    icon: &str,
    handler: Callable,
) -> Gd<Button> {
    // code-instantiated: the dialog's own button-bar entries (shared shell row).
    let mut button = factory::button("");
    button.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    apply_pane_button(&mut button, styles::PANE_OUTLINE_GRAY, 4.0);
    let icon_label = factory::glyph_label(icon, 20, Color::WHITE);
    build_button_content(&mut button, Some(icon_label.upcast()), &bt(key));
    button.connect("pressed", &handler);
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&button);
    }
    button
}

/// `@mod.import` (`:145-198`): a `Tex.button` table of two 300x70
/// `Styles.flatt` buttons (`@mod.import.file` / Icon.file,
/// `@mod.import.github` / Icon.github, margin 12) and a closed `@back` bar.
pub(super) fn mods_build_import_overlay(dialog: &mut MindDialog, state: &Rc<RefCell<ModsState>>) {
    if let Some(mut existing) = dialog.base().get_node_or_null("ImportDialog") {
        existing.queue_free();
    }
    // code-instantiated: the transient import chooser has no manifest entry.
    let mut overlay = Control::new_alloc();
    overlay.set_name("ImportDialog");
    overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    overlay.set_mouse_filter(MouseFilter::STOP);
    overlay.set_visible(false);
    dialog.base_mut().add_child(&overlay);

    // Its own `Styles.defaultDialog` stage background (black9).
    // code-instantiated: the overlay scrim dims the mods dialog beneath.
    let mut dim = ColorRect::new_alloc();
    dim.set_color(styles::BLACK_9);
    dim.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    dim.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&dim);

    // Same insets as the fill-parent dialog shell.
    // code-instantiated: overlay frame insets (8 px sides/bottom).
    let mut frame = MarginContainer::new_alloc();
    frame.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
    frame.add_theme_constant_override("margin_left", 8);
    frame.add_theme_constant_override("margin_right", 8);
    frame.add_theme_constant_override("margin_top", 0);
    frame.add_theme_constant_override("margin_bottom", 8);
    frame.set_mouse_filter(MouseFilter::IGNORE);
    overlay.add_child(&frame);
    // code-instantiated: the overlay column (title, chooser, back bar).
    let mut layout = factory::vbox(8);
    layout.set_mouse_filter(MouseFilter::IGNORE);
    frame.add_child(&layout);

    // Title row + accent rule (`BaseDialog.java:22-25`).
    // code-instantiated: the overlay title (a plain Label, not a MindLabel).
    let mut title = Label::new_alloc();
    title.set_text(&bt("@mod.import"));
    title.set_horizontal_alignment(HorizontalAlignment::CENTER);
    title.add_theme_color_override("font_color", styles::ACCENT);
    layout.add_child(&title);
    // code-instantiated: the accent underline of the overlay title.
    let mut rule = ColorRect::new_alloc();
    rule.set_color(styles::ACCENT);
    rule.set_anchors_preset(LayoutPreset::BOTTOM_WIDE);
    rule.set_offset(Side::TOP, -3.0);
    rule.set_offset(Side::BOTTOM, 0.0);
    rule.set_mouse_filter(MouseFilter::IGNORE);
    title.add_child(&rule);

    // code-instantiated: centers the import chooser panel.
    let mut center = CenterContainer::new_alloc();
    center.set_v_size_flags(SizeFlags::EXPAND_FILL);
    center.set_mouse_filter(MouseFilter::IGNORE);
    layout.add_child(&center);
    // `dialog.cont.table(Tex.button, t -> { t.margin(12f); … })`.
    // code-instantiated: the Tex.button chooser frame.
    let mut panel = PanelContainer::new_alloc();
    let mut panel_box =
        MindDialog::pane_box(Color::BLACK, styles::PANE_OUTLINE_GRAY, 4.0, 2.0, -1.0);
    for side in [Side::LEFT, Side::RIGHT, Side::TOP, Side::BOTTOM] {
        panel_box.set_content_margin(side, 12.0);
    }
    panel.add_theme_stylebox_override("panel", &panel_box);
    center.add_child(&panel);
    // code-instantiated: the two chooser entries.
    let mut column = factory::vbox(0);
    panel.add_child(&column);

    // `Iconc.file` is a raw code point, not an alias.
    // code-instantiated: the file chooser glyph (`Iconc.file`, 61787).
    let mut file_icon = Label::new_alloc();
    if let Some(character) = char::from_u32(ICON_FILE_CODE) {
        let mut buffer = [0u8; 4];
        file_icon.set_text(&GString::from(&*character.encode_utf8(&mut buffer)));
    }
    {
        let mut control: Gd<Control> = file_icon.clone().upcast();
        kit::icons::apply_icon_font(&mut control);
    }
    file_icon.add_theme_font_size_override("font_size", 24);
    file_icon.set_vertical_alignment(VerticalAlignment::CENTER);
    file_icon.set_mouse_filter(MouseFilter::IGNORE);
    {
        let mut file_button = mods_flat_button("@mod.import.file", Some(file_icon.upcast()));
        let slot = state.clone();
        let callable = Callable::from_fn("mods_import_file", move |_args: &[&Variant]| {
            let slot = slot.clone();
            defer_action("mods_import_file_now", move || mods_import_file(&slot));
            Variant::nil()
        });
        file_button.connect("pressed", &callable);
        column.add_child(&file_button);
    }
    {
        let glyph = factory::glyph_label("github", 24, Color::WHITE);
        let mut github_button = mods_flat_button("@mod.import.github", Some(glyph.upcast()));
        let slot = state.clone();
        let callable = Callable::from_fn("mods_import_github", move |_args: &[&Variant]| {
            let slot = slot.clone();
            defer_action("mods_import_github_now", move || mods_import_github(&slot));
            Variant::nil()
        });
        github_button.connect("pressed", &callable);
        column.add_child(&github_button);
    }

    // `dialog.addCloseButton()`: `@back` (Icon.left) 210x64.
    // code-instantiated: the overlay's own back bar.
    let mut bar = HBoxContainer::new_alloc();
    bar.set_alignment(godot::classes::box_container::AlignmentMode::CENTER);
    bar.set_mouse_filter(MouseFilter::IGNORE);
    layout.add_child(&bar);
    // code-instantiated: the overlay close button (no shared shell row here).
    let mut back = factory::button("");
    back.set_custom_minimum_size(Vector2::new(210.0, 64.0));
    apply_pane_button(&mut back, styles::PANE_OUTLINE_GRAY, 4.0);
    let back_icon = factory::glyph_label("left", 20, Color::WHITE);
    build_button_content(&mut back, Some(back_icon.upcast()), &bt("@back"));
    {
        let slot = state.clone();
        let callable = Callable::from_fn("mods_import_close", move |_args: &[&Variant]| {
            mods_hide_import_dialog(&slot);
            Variant::nil()
        });
        back.connect("pressed", &callable);
    }
    bar.add_child(&back);

    state.borrow_mut().import_overlay = Some(overlay);
}

/// `Styles.flatt` (`Styles.java:149-156`): `up = black`, `over`/`down` =
/// `flatOver`; `t.defaults().size(300f, 70f)`, `.margin(12f)`.
pub(super) fn mods_flat_button(key: &str, icon: Option<Gd<Control>>) -> Gd<Button> {
    // code-instantiated: chooser entries (see `mods_build_import_overlay`).
    let mut button = Button::new_alloc();
    button.set_custom_minimum_size(Vector2::new(300.0, 70.0));
    button.set_focus_mode(FocusMode::NONE);
    button.add_theme_stylebox_override("normal", &flat_style(Color::BLACK));
    button.add_theme_stylebox_override("hover", &flat_style(styles::FLAT_OVER));
    button.add_theme_stylebox_override("pressed", &flat_style(styles::FLAT_OVER));
    button.add_theme_stylebox_override("focus", &StyleBoxEmpty::new_gd());
    build_button_content(&mut button, icon, &bt(key));
    button
}

/// `_connect_mods()`: the `MindMods` state signals and the two `MindUi`
/// prompt results, bound once per dialog node.
pub(super) fn mods_connect(dialog: &mut MindDialog, state: &Rc<RefCell<ModsState>>) {
    let mut node = dialog.to_gd();
    if node.has_meta(MODS_SIGNALS_META) {
        return;
    }
    if let Some(mut mods) = mods_node() {
        if mods.has_signal("mods_changed") {
            let slot = state.clone();
            let callable = Callable::from_fn("mods_changed", move |_args: &[&Variant]| {
                let slot = slot.clone();
                defer_action("mods_changed_refresh", move || mods_refresh(&slot));
                Variant::nil()
            });
            mods.connect("mods_changed", &callable);
        }
        if mods.has_signal("mod_error") {
            let callable = Callable::from_fn("mods_mod_error", move |args: &[&Variant]| {
                let name = args
                    .first()
                    .map(|value| variant_string(value))
                    .unwrap_or_default();
                let details = args
                    .get(1)
                    .map(|value| variant_string(value))
                    .unwrap_or_default();
                defer_action("mods_mod_error_now", move || {
                    mods_show_error(&format!("{name}: {details}"));
                });
                Variant::nil()
            });
            mods.connect("mod_error", &callable);
        }
        if mods.has_signal("mod_import_progress") {
            let slot = state.clone();
            let callable = Callable::from_fn("mods_import_progress", move |args: &[&Variant]| {
                let ratio = args
                    .get(1)
                    .and_then(|value| value.try_to::<f32>().ok())
                    .unwrap_or(0.0);
                let slot = slot.clone();
                defer_action("mods_import_progress_now", move || {
                    if let Ok(st) = slot.try_borrow()
                        && let Some(mut progress) = st.progress.clone()
                        && progress.is_instance_valid()
                    {
                        progress.set_visible(ratio < 1.0);
                        progress.set_value(f64::from(ratio));
                    }
                });
                Variant::nil()
            });
            mods.connect("mod_import_progress", &callable);
        }
    }
    if let Some(mut ui) = kit::ui() {
        if ui.has_signal("confirm_result") {
            let slot = state.clone();
            let callable = Callable::from_fn("mods_confirm_result", move |args: &[&Variant]| {
                let confirmed = args
                    .first()
                    .map(|value| variant_bool(value))
                    .unwrap_or(false);
                let slot = slot.clone();
                defer_action("mods_confirm_now", move || {
                    mods_on_confirm(&slot, confirmed)
                });
                Variant::nil()
            });
            ui.connect("confirm_result", &callable);
        }
        if ui.has_signal("text_input_result") {
            let slot = state.clone();
            let callable =
                Callable::from_fn("mods_text_input_result", move |args: &[&Variant]| {
                    let text = args
                        .first()
                        .map(|value| variant_string(value))
                        .unwrap_or_default();
                    let slot = slot.clone();
                    defer_action("mods_text_input_now", move || {
                        mods_on_text_input(&slot, &text);
                    });
                    Variant::nil()
                });
            ui.connect("text_input_result", &callable);
        }
    }
    node.set_meta(MODS_SIGNALS_META, &true.to_variant());
}

/// The browser's `set_updates(updates)` replacement (see module docs).
pub(super) fn mods_read_updates(node: &Gd<MindDialog>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if !node.has_meta(MODS_UPDATES_META) {
        return out;
    }
    let value = node.get_meta(MODS_UPDATES_META);
    let Ok(dict) = value.try_to::<VarDictionary>() else {
        return out;
    };
    for (key, value) in dict.iter_shared() {
        out.push((variant_string(&key), variant_string(&value)));
    }
    out
}

/// `ModsDialog.setup()` (`:130-353`): rebuilds `cont`.
pub(super) fn mods_refresh(state: &Rc<RefCell<ModsState>>) {
    let Ok(mut st) = state.try_borrow_mut() else {
        log::warn!("[mods] refresh skipped: controller state already borrowed");
        return;
    };
    mods_refresh_state(state, &mut st);
}

/// [`mods_refresh`] body (the caller owns the state borrow).
pub(super) fn mods_refresh_state(state: &Rc<RefCell<ModsState>>, st: &mut ModsState) {
    let Some(mut root) = mods_ensure_root(st) else {
        return;
    };
    let mods = mods_node();
    let entries = mods_list(&mods);

    // Ctor (`:55-61`): `mods.list().contains(LoadedMod::failed)` -> Restore,
    // else Guide.
    let any_failed = entries
        .iter()
        .any(|entry| dict_bool(entry, "failed", false));
    if let Some(mut restore) = st.restore_button.clone()
        && restore.is_instance_valid()
    {
        restore.set_visible(any_failed);
    }
    if let Some(mut guide) = st.guide_button.clone()
        && guide.is_instance_valid()
    {
        guide.set_visible(!any_failed);
    }

    root.bind_mut().clear();
    st.pane = None;
    st.list = None;
    st.search = None;
    st.any_disabled = false;

    let viewport_width = match st.body.clone() {
        Some(body) if body.is_instance_valid() => body.get_viewport_rect().size.x,
        _ => 0.0,
    };
    // `w = min(width / 1.05 - 28, 520)`; `cont.defaults().width(min(width / 1.05, 556)).pad(4)`.
    let w = (viewport_width / 1.05 - 28.0).min(520.0);
    let cw = (viewport_width / 1.05).min(556.0);

    // `cont.add("@mod.reloadrequired").visible(mods::requiresReload).center()`:
    // an invisible Arc cell keeps its space, so fade instead of hiding.
    // code-instantiated: the reload banner is rebuilt per `setup()`.
    let mut banner = factory::label(&format!("[center]{}[/center]", bm("@mod.reloadrequired")));
    let requires_reload = mods.as_ref().is_some_and(|_| mods_requires_reload());
    let mut modulate = banner.get_modulate();
    modulate.a = if requires_reload { 1.0 } else { 0.0 };
    banner.set_modulate(modulate);
    {
        let mut bound = root.bind_mut();
        bound.add(&banner).width(cw).pad(4.0);
        bound.row();
    }
    st.banner = Some(banner);

    let action_row = mods_build_action_row();
    {
        let mut bound = root.bind_mut();
        bound.add(&action_row).width(w).height(60.0).pad(4.0);
        bound.row();
    }

    // Port-only (OD1): content errors / unsupported scripts summary button.
    let errors = mods_errors(&mods);
    // code-instantiated: only present when `MindMods.errors()` is non-empty.
    let mut errors_button = factory::button(&bt("@mod.errors"));
    errors_button.set_visible(!errors.is_empty());
    {
        let callable = Callable::from_fn("mods_show_errors", move |_args: &[&Variant]| {
            defer_action("mods_show_errors_now", mods_show_errors);
            Variant::nil()
        });
        errors_button.connect("pressed", &callable);
    }
    {
        let mut bound = root.bind_mut();
        bound.add(&errors_button).width(cw).pad(4.0);
        bound.row();
    }
    st.errors_button = Some(errors_button);

    // Port-only: `MindMods.mod_import_progress` bar.
    // code-instantiated: transient progress widget hidden unless an import runs.
    let mut progress = ProgressBar::new_alloc();
    progress.set_min(0.0);
    progress.set_max(1.0);
    progress.set_visible(false);
    {
        let mut bound = root.bind_mut();
        bound.add(&progress).width(cw).pad(4.0);
        bound.row();
    }
    st.progress = Some(progress);

    if entries.is_empty() {
        // `cont.table(Styles.black6, t -> t.add("@mods.none")).height(80f)`.
        // code-instantiated: the empty-state box replaces the pane in `setup()`.
        let mut none_box = PanelContainer::new_alloc();
        none_box.set_theme_type_variation("black6");
        let mut none_label = factory::label(&format!("[center]{}[/center]", bm("@mods.none")));
        none_label.set_v_size_flags(SizeFlags::SHRINK_CENTER);
        none_box.add_child(&none_label);
        let mut bound = root.bind_mut();
        bound.add(&none_box).width(cw).height(80.0).pad(4.0);
        bound.row();
    } else {
        // `search.image(Icon.zoom).padRight(8f); search.field("", rebuild).growX();`
        // code-instantiated: the search row is rebuilt with `cont` (`setup()`).
        let mut search_row = factory::hbox(8);
        search_row.add_child(&factory::glyph_label("zoom", 22, Color::WHITE));
        // code-instantiated: the search field filters the runtime mod rows.
        let mut search = factory::field("");
        search.set_h_size_flags(SizeFlags::EXPAND_FILL);
        {
            let slot = state.clone();
            let callable = Callable::from_fn("mods_search", move |args: &[&Variant]| {
                let query = args
                    .first()
                    .map(|value| variant_string(value))
                    .unwrap_or_default();
                if let Ok(mut st) = slot.try_borrow_mut() {
                    mods_rebuild_list(&slot, &mut st, &query);
                }
                Variant::nil()
            });
            search.connect("text_changed", &callable);
        }
        search_row.add_child(&search);
        {
            let mut bound = root.bind_mut();
            bound.add(&search_row).width(cw).pad(4.0);
            bound.row();
        }
        st.search = Some(search);

        // `cont.pane(table1 -> pane[0] = table1.margin(10f).top()).scrollX(false)`.
        // code-instantiated: the mod rows are data-driven from `MindMods.list()`.
        let mut list = factory::table();
        list.set_h_size_flags(SizeFlags::EXPAND_FILL);
        list.bind_mut().margin(10.0);
        // code-instantiated: the scroll pane wraps the runtime mod table.
        let mut pane = MindScroll::new_alloc();
        pane.add_child(&list);
        pane.bind_mut().set_scrolling_disabled(true, false);
        {
            let mut bound = root.bind_mut();
            bound.add(&pane).width(cw).pad(4.0);
            bound.row();
        }
        st.list = Some(list);
        st.pane = Some(pane);
        mods_rebuild_list(state, st, "");

        // `withUpdates.size > 1`: the `@mods.update.all` row (w x 60, padTop 12).
        if st.updates.len() > 1 {
            // code-instantiated: the batch update row lists the pending updates.
            let mut update_all = mods_gray_button();
            let mut update_row = mods_button_row(
                &update_all,
                godot::classes::box_container::AlignmentMode::CENTER,
            );
            update_row.add_child(&factory::glyph_label("download", 24, Color::WHITE));
            update_row.add_child(&mods_row_label(&bm("@mods.update.all")));
            update_row.add_child(&factory::glyph_label("download", 24, Color::WHITE));
            {
                let slot = state.clone();
                let callable = Callable::from_fn("mods_update_all", move |_args: &[&Variant]| {
                    let slot = slot.clone();
                    defer_action("mods_update_all_now", move || {
                        if let Ok(st) = slot.try_borrow() {
                            mods_update_all(&st);
                        }
                    });
                    Variant::nil()
                });
                update_all.connect("pressed", &callable);
            }
            let mut bound = root.bind_mut();
            bound
                .add(&update_all)
                .width(w)
                .height(60.0)
                .pad(4.0)
                .pad_top(12.0);
            bound.row();
        }
    }

    root.bind_mut().sort_now();
    mods_defer_fit(state);
}

/// The `cont` root table, created once (`content_table()` replacement).
pub(super) fn mods_ensure_root(state: &mut ModsState) -> Option<Gd<MindTable>> {
    if let Some(root) = state.root.clone() {
        if root.is_instance_valid() {
            return Some(root);
        }
        state.root = None;
    }
    let mut cont = state.cont.clone()?;
    if !cont.is_instance_valid() {
        return None;
    }
    // code-instantiated: the dialog body is rebuilt from `MindMods.list()`.
    let mut table = factory::table();
    {
        let mut bound = table.bind_mut();
        bound.grow_horizontal();
        bound.grow_vertical();
        bound.set_table_top(false);
        // `cont.defaults().pad(3)` (arc `Dialog.java:213`).
        let defaults = bound.defaults();
        defaults.pad_left = 3.0;
        defaults.pad_right = 3.0;
        defaults.pad_top = 3.0;
        defaults.pad_bottom = 3.0;
    }
    // Java tables center their content; `MindTable` packs from the left, so the
    // column itself is centered in `cont`.
    table.set_h_size_flags(SizeFlags::SHRINK_CENTER);
    cont.add_child(&table);
    state.root = Some(table.clone());
    Some(table)
}
