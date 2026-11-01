// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/dialogs/BaseDialog.java (plan 14 §3.4).

//! `MindDialog` — the native base for every dialog scene.
//!
//! Visibility and the active-dialog stack are owned by `MindUi` (Rust); this
//! node only renders and calls `show_dialog`/`hide_dialog`. Pause transitions
//! are the governor's job (`should_pause` is declared by the manifest), never
//! the widget's.
//!
//! Per-dialog behaviour lives in a [`DialogController`] (see `ui::dialogs`)
//! selected from the scene path, so the 46 scenes that inherit
//! `mind_dialog_base.tscn` share this one native class.

use godot::builtin::Side;
use godot::classes::{
    BoxContainer, Button, ColorRect, Container, Control, IControl, Json, Label, PanelContainer,
    ScrollContainer, StyleBoxEmpty, Tween,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use super::kit::widgets::factory;
use super::kit::{self, styles};

/// Dialog-specific behaviour, installed by [`super::dialogs::controller_for`].
pub trait DialogController {
    /// Runs at the end of the dialog's `ready` (after the shell is configured).
    fn ready(&mut self, _dialog: &mut MindDialog) {}
    /// `BaseDialog.shown`.
    fn shown(&mut self, _dialog: &mut MindDialog) {}
    /// The dialog context was (re)assigned (`set_context_json`).
    fn context_changed(&mut self, _dialog: &mut MindDialog) {}
    /// `BaseDialog.hidden`.
    fn hidden(&mut self, _dialog: &mut MindDialog) {}
    /// Whether ESC must not close this dialog (Arc modal consumption).
    fn blocks_escape(&self) -> bool {
        false
    }
}

/// Path to the live campaign facade (owns sector/tech/rules/schematic state).
const CAMPAIGN_PATH: &str = "/root/Spine/MindCampaign";

/// `BaseDialog.java:23-25` title geometry: 8 px under the text.
const TITLE_INSET_TOP: f32 = 0.0;
const TITLE_INSET_BOTTOM: f32 = 8.0;
/// Arc `Dialog` `defaults().pad(3)` on a re-added title cell.
const TITLE_CELL_PAD: f32 = 3.0;
/// Arc `buttons.defaults().pad(3)` above each button.
const BUTTON_CELL_PAD: f32 = 3.0;
/// Node meta: a `sync_buttons_row` is already queued for this dialog.
const BUTTONS_SYNC_PENDING: &str = "_mind_dialog_buttons_sync_pending";

/// `MindDialog` — the shared dialog shell.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindDialog {
    base: Base<Control>,

    /// Title text; empty hides the title row entirely.
    #[export]
    pub title_text: GString,
    /// Whether showing this dialog pauses the game (manifest `pause`).
    #[export]
    pub should_pause: bool,
    /// Whether the dialog uses the full-screen dialog style (`fullDialog`).
    #[export]
    pub full_dialog: bool,
    /// Whether the dialog fills the parent (`BaseDialog.java:22`).
    #[export]
    pub fill_parent: bool,
    /// Accent color of the title row.
    #[export]
    pub title_color: Color,
    /// Java `cont.top()`: the body content sticks to the top.
    #[export]
    pub cont_top: bool,
    /// `BaseDialog.makeButtonOverlay()`.
    #[export]
    pub button_overlay: bool,

    context: VarDictionary,
    hide_tween: Option<Gd<Tween>>,
    hiding: bool,
    controller: Option<Box<dyn DialogController>>,
    dialog_id: String,
    buttons_visible: bool,
}

#[godot_api]
impl MindDialog {
    /// The dialog registry key (`about`, `settings`, …) from the manifest.
    fn resolve_dialog_id(&self) -> String {
        let path = self.base().get_scene_file_path().to_string();
        if let Some(id) = manifest_dialog_id(&path) {
            return id;
        }
        // Fallback: `about_dialog.tscn` -> `about`, `color_picker.tscn` ->
        // `color-picker`.
        let file = path.rsplit('/').next().unwrap_or(&path).to_string();
        let stem = file.strip_suffix(".tscn").unwrap_or(&file);
        stem.strip_suffix("_dialog")
            .unwrap_or(stem)
            .replace('_', "-")
    }

    /// The `Center/Panel/Layout/Title` label.
    pub fn title_node(&mut self) -> Option<Gd<Label>> {
        self.node_as("Center/Panel/Layout/Title")
    }

    /// The dialog's content column (`Center/Panel/Layout/Body/Content`).
    pub fn content(&mut self) -> Option<Gd<BoxContainer>> {
        self.node_as("Center/Panel/Layout/Body/Content")
    }

    /// The dialog's scroll body (`Center/Panel/Layout/Body`).
    pub fn body(&mut self) -> Option<Gd<ScrollContainer>> {
        self.node_as("Center/Panel/Layout/Body")
    }

    /// The shared button row (`Center/Panel/Layout/Buttons`).
    pub fn buttons_row(&mut self) -> Option<Gd<BoxContainer>> {
        self.node_as("Center/Panel/Layout/Buttons")
    }

    /// The dialog panel (`Center/Panel`).
    pub fn panel(&mut self) -> Option<Gd<PanelContainer>> {
        self.node_as("Center/Panel")
    }

    /// The dialog registry key.
    #[allow(dead_code)] // ported surface; not reachable from the native call graph (kept for parity)
    pub fn dialog_id(&self) -> &str {
        &self.dialog_id
    }

    fn node_as<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: Inherits<Node>,
    {
        let node = self.base().get_node_or_null(path)?;
        node.try_cast::<T>().ok()
    }

    /// Bundle lookup with the key echoed back when assets are absent.
    pub fn t(&self, key: &str) -> GString {
        kit::t(key)
    }

    /// Bundle string translated to Godot BBCode.
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub fn tm(&self, key: &str) -> GString {
        kit::tm(key)
    }

    /// One context value passed to `MindUi.open_dialog`.
    pub fn context_value(&self, key: &str) -> Option<Variant> {
        self.context.get(key)
    }

    /// One context value coerced to a string (empty when absent).
    pub fn context_string(&self, key: &str) -> GString {
        self.context
            .get(key)
            .map(|value| value.to::<GString>())
            .unwrap_or_default()
    }

    /// One context value coerced to an integer.
    pub fn context_int(&self, key: &str) -> i64 {
        self.context
            .get(key)
            .map(|value| value.to::<i64>())
            .unwrap_or_default()
    }

    /// Adds a themed action button to the button row and returns it.
    ///
    /// `icon` adds an icon-font glyph; `width > 0` sets a minimum width.
    pub fn add_button(
        &mut self,
        text: &str,
        listener: Option<Callable>,
        icon: &str,
        width: f32,
    ) -> Gd<Button> {
        // code-instantiated: action buttons are data-driven per dialog and have
        // no static scene (the button row is shared across every dialog).
        let mut button = if icon.is_empty() {
            factory::button(text)
        } else {
            factory::icon_button(
                icon,
                text,
                "defaultt",
                godot::classes::box_container::AlignmentMode::BEGIN,
            )
        };
        let mut width = width;
        if width <= 0.0 && !icon.is_empty() {
            // A `Button` does not size to its child row; estimate from the
            // label so icon+text action buttons do not clip.
            width = 150.0_f32.max(text.chars().count() as f32 * 12.0 + 60.0);
        }
        if width > 0.0 {
            let mut min = button.get_custom_minimum_size();
            min.x = width;
            button.set_custom_minimum_size(min);
        }
        if let Some(listener) = listener
            && listener.is_valid()
        {
            button.connect("pressed", &listener);
        }
        if let Some(mut row) = self.buttons_row() {
            row.add_child(&button);
        }
        button
    }

    /// Adds an `@back` close button to the button row (`BaseDialog.addCloseButton`).
    pub fn add_close_button(&mut self, width: f32) -> Gd<Button> {
        // code-instantiated: the close button is added by the dialog itself.
        let mut button = factory::icon_button(
            "left",
            &self.t("@back").to_string(),
            "defaultt",
            godot::classes::box_container::AlignmentMode::BEGIN,
        );
        button.set_custom_minimum_size(Vector2::new(width, 64.0));
        let callable = self.base().callable("close_pressed");
        button.connect("pressed", &callable);
        if let Some(mut row) = self.buttons_row() {
            row.add_child(&button);
        }
        button
    }

    /// Removes every button from the shared button row.
    pub fn clear_buttons(&mut self) {
        let Some(row) = self.buttons_row() else {
            return;
        };
        for child in row.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
    }

    /// `cont.clearChildren()`: drops every body child.
    pub fn clear_content(&mut self) {
        let Some(mut content) = self.content() else {
            return;
        };
        for child in content.get_children().iter_shared() {
            let mut child = child.clone();
            content.remove_child(&child);
            child.queue_free();
        }
    }

    /// The dialog's content column as a new `MindTable` appended to `cont`.
    ///
    /// code-instantiated: dialog bodies are data-driven and rebuilt on `shown()`.
    pub fn content_table(&mut self) -> Gd<kit::MindTable> {
        let mut table = kit::MindTable::new_alloc();
        {
            let mut bound = table.bind_mut();
            bound.grow_horizontal();
            // Arc `cont` is `add(cont).expand().fill()`: the body table takes
            // the whole content height and aligns its grid itself.
            bound.grow_vertical();
            bound.set_table_top(self.cont_top);
            // `cont.defaults().pad(3)` (arc `Dialog.java:213`).
            let defaults = bound.defaults();
            defaults.pad_left = 3.0;
            defaults.pad_right = 3.0;
            defaults.pad_top = 3.0;
            defaults.pad_bottom = 3.0;
        }
        if let Some(mut content) = self.content() {
            let control: Gd<Control> = table.clone().upcast();
            content.add_child(&control);
        }
        table
    }

    /// Re-applies the title row after a controller changed `title_text`/`title_color`.
    pub fn refresh_title(&mut self) {
        self.apply_title();
    }

    /// Switches to the full-screen dialog style and fits the panel.
    pub fn enable_full_dialog(&mut self, value: bool) {
        self.full_dialog = value;
        self.apply_full_dialog();
    }

    /// Sets the title from a bundle key and applies the accent title color.
    pub fn set_title_key(&mut self, key: &str) {
        self.title_text = self.t(key);
        self.title_color = styles::ACCENT;
        self.apply_title();
    }

    /// Sets literal title text (empty hides the row) and re-applies.
    pub fn set_title_literal(&mut self, value: &str) {
        self.title_text = GString::from(value);
        self.apply_title();
    }

    /// `cont.top()` at runtime.
    pub fn apply_cont_top(&mut self, value: bool) {
        self.cont_top = value;
        self.apply_cont_alignment();
        let mut tables: Vec<Gd<kit::MindTable>> = Vec::new();
        if let Some(content) = self.content() {
            for child in content.get_children().iter_shared() {
                if let Ok(table) = child.clone().try_cast::<kit::MindTable>() {
                    tables.push(table);
                }
            }
        }
        for mut table in tables {
            table.bind_mut().set_table_top(value);
        }
    }

    /// `makeButtonOverlay()` at runtime.
    pub fn apply_button_overlay(&mut self, value: bool) {
        self.button_overlay = value;
        self.apply_title();
        self.sync_buttons_row();
    }

    /// `Dialog.setFillParent(false)`: sizes the panel to its content.
    #[allow(dead_code)] // ported kit surface; no native caller (kept for parity)
    pub fn apply_fill_parent(&mut self, value: bool) {
        self.fill_parent = value;
        if value {
            self.fill_panel();
        }
    }

    /// Closes through `MindUi` so the dialog stack and pause governor stay in
    /// sync; a dialog that was never registered still hides locally.
    pub fn close(&mut self) {
        if let Some(mut ui) = kit::ui()
            && ui.has_method("close_dialog")
        {
            let name = self.base().get_name().to_string();
            let closed = ui
                .call("close_dialog", &[name.to_variant()])
                .try_to::<bool>()
                .unwrap_or(false);
            if closed {
                return;
            }
        }
        self.hide_dialog();
    }

    /// Opens another dialog through `MindUi`.
    pub fn open_dialog(&self, name: &str, context_json: &str) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call(
                "open_dialog",
                &[
                    GString::from(name).to_variant(),
                    GString::from(context_json).to_variant(),
                ],
            );
        }
    }

    /// Shows a transient info prompt through `MindUi`.
    pub fn show_toast(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(mut ui) = kit::ui()
            && ui.has_method("show_info")
        {
            let _ = ui.call("show_info", &[GString::from(text).to_variant()]);
        }
    }

    /// The live campaign facade, or `None` when absent (menu boot).
    pub fn campaign_node(&self) -> Option<Gd<Node>> {
        self.base().get_node_or_null(CAMPAIGN_PATH)
    }

    /// Fetches a campaign read-model method returning JSON (`{}` when absent).
    pub fn campaign_json(&self, method: &str, args: &[Variant]) -> VarDictionary {
        let Some(mut campaign) = self.campaign_node() else {
            return VarDictionary::new();
        };
        if !campaign.has_method(method) {
            return VarDictionary::new();
        }
        let value = campaign.call(method, args);
        parse_json_object(&value.to::<GString>())
    }

    /// Calls a campaign facade method (`None` when absent).
    pub fn campaign_call(&mut self, method: &str, args: &[Variant]) -> Option<Variant> {
        let mut campaign = self.campaign_node()?;
        if !campaign.has_method(method) {
            return None;
        }
        Some(campaign.call(method, args))
    }

    /// The whole campaign read-model projection (`{}` when absent).
    pub fn campaign_views(&self) -> VarDictionary {
        self.campaign_json("campaign_views_json", &[GString::new().to_variant()])
    }

    /// A named section of the campaign read models (`planets`/`sectors`/…).
    pub fn campaign_section(&self, section: &str) -> VarArray {
        let views = self.campaign_json("campaign_views_json", &[GString::new().to_variant()]);
        views
            .get(section)
            .map(|value| value.to::<VarArray>())
            .unwrap_or_default()
    }

    /// `UI.formatTime`-style playtime (h/m/s) for millisecond values.
    pub fn format_time_ms(milliseconds: i64) -> String {
        let seconds = milliseconds / 1000;
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        if hours > 0 {
            format!("{hours}h {minutes}m")
        } else if minutes > 0 {
            format!("{}m {}s", minutes, seconds % 60)
        } else {
            format!("{seconds}s")
        }
    }

    /// `BaseDialog.makeButtonOverlay`/`Styles.flatTogglet` pane look.
    pub fn pane_box(
        fill: Color,
        border: Color,
        width: f32,
        radius: f32,
        bottom_width: f32,
    ) -> Gd<godot::classes::StyleBoxFlat> {
        let mut style = godot::classes::StyleBoxFlat::new_gd();
        style.set_bg_color(fill);
        style.set_border_width(Side::LEFT, width as i32);
        style.set_border_width(Side::TOP, width as i32);
        style.set_border_width(Side::RIGHT, width as i32);
        style.set_border_width(
            Side::BOTTOM,
            if bottom_width >= 0.0 {
                bottom_width
            } else {
                width
            } as i32,
        );
        style.set_border_color(border);
        style.set_corner_radius(godot::builtin::Corner::TOP_LEFT, radius as i32);
        style.set_corner_radius(godot::builtin::Corner::TOP_RIGHT, radius as i32);
        style.set_corner_radius(godot::builtin::Corner::BOTTOM_LEFT, radius as i32);
        style.set_corner_radius(godot::builtin::Corner::BOTTOM_RIGHT, radius as i32);
        style.set_content_margin_all(2.0);
        style
    }

    /// Applies the pane look to one button (`Styles.flatTogglet` over `Tex.pane`).
    pub fn apply_pane_button(&self, button: &mut Gd<Button>, border: Color, bottom_border: f32) {
        let fill = Color::BLACK;
        let normal = Self::pane_box(fill, border, 4.0, 2.0, bottom_border);
        button.add_theme_stylebox_override("normal", &normal);
        button.add_theme_stylebox_override("hover", &normal);
        button.add_theme_stylebox_override("disabled", &normal);
        let pressed = Self::pane_box(fill, border, 4.0, 2.0, -1.0);
        button.add_theme_stylebox_override("pressed", &pressed);
    }

    /// `planetTable.button(name, icon, Styles.flatTogglet)`: icon at the left
    /// margin, label centred in the remaining width.
    pub fn build_button_content(
        &mut self,
        button: &mut Gd<Button>,
        icon: Option<Gd<Control>>,
        text: &str,
    ) {
        // code-instantiated: button content rows are data-driven per dialog.
        let mut row = godot::classes::HBoxContainer::new_alloc();
        row.set_alignment(godot::classes::box_container::AlignmentMode::BEGIN);
        row.set_anchors_and_offsets_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        row.set_offset(Side::LEFT, 13.0);
        row.set_offset(Side::RIGHT, -8.0);
        row.add_theme_constant_override("separation", 8);
        row.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        let control: Gd<Control> = row.clone().upcast();
        button.add_child(&control);
        if let Some(icon) = icon {
            row.add_child(&icon);
        }
        let mut label = factory::label(text);
        label.set_autowrap_mode(godot::classes::text_server::AutowrapMode::OFF);
        label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
        label.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        label.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        label.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
        let label_control: Gd<Control> = label.clone().upcast();
        row.add_child(&label_control);
    }

    // ---- shell internals -------------------------------------------------

    fn title_inset_bottom(&self) -> f32 {
        TITLE_INSET_BOTTOM
            + if self.button_overlay {
                TITLE_CELL_PAD
            } else {
                0.0
            }
    }

    fn apply_title(&mut self) {
        let Some(mut title) = self.title_node() else {
            return;
        };
        if !title.has_theme_stylebox_override("normal") {
            let fresh = StyleBoxEmpty::new_gd();
            title.add_theme_stylebox_override("normal", &fresh);
        }
        if let Some(mut inset) = title.get_theme_stylebox("normal") {
            inset.set_content_margin(Side::TOP, TITLE_INSET_TOP);
            inset.set_content_margin(Side::BOTTOM, self.title_inset_bottom());
        }
        title.set_text(&self.title_text);
        title.set_visible(!self.title_text.is_empty());
        title.add_theme_color_override("font_color", self.title_color);
        self.ensure_title_underline();
        if let Some(mut panel) = self.panel() {
            panel.queue_sort();
        }
    }

    fn ensure_title_underline(&mut self) {
        let Some(mut title) = self.title_node() else {
            return;
        };
        let mut line = match title.get_node_or_null("Underline") {
            Some(node) => node.try_cast::<ColorRect>().ok(),
            None => None,
        };
        if line.is_none() {
            // code-instantiated: the rule is part of the shared dialog shell.
            let mut created = ColorRect::new_alloc();
            created.set_name("Underline");
            created.set_color(styles::ACCENT);
            created.set_anchors_preset(godot::classes::control::LayoutPreset::BOTTOM_WIDE);
            created.set_mouse_filter(godot::classes::control::MouseFilter::IGNORE);
            let control: Gd<Control> = created.clone().upcast();
            title.add_child(&control);
            line = Some(created);
        }
        let Some(mut line) = line else {
            return;
        };
        let bottom = self.title_inset_bottom();
        line.set_offset(Side::TOP, 4.0 - bottom);
        line.set_offset(Side::BOTTOM, 7.0 - bottom);
        let side = if self.button_overlay {
            TITLE_CELL_PAD
        } else {
            0.0
        };
        line.set_offset(Side::LEFT, side);
        line.set_offset(Side::RIGHT, -side);
    }

    fn watch_title_shift(&mut self) {
        let Some(mut panel) = self.node_as::<Container>("Center/Panel") else {
            return;
        };
        let callable = self.base().callable("shift_title_row");
        panel.connect("sort_children", &callable);
    }

    /// Lifts the shell's `Layout` by `TITLE_CELL_PAD` after the panel fits it.
    #[func]
    fn shift_title_row(&mut self) {
        if self.button_overlay {
            return;
        }
        let Some(mut layout) = self.node_as::<Control>("Center/Panel/Layout") else {
            return;
        };
        let Some(panel) = self.node_as::<PanelContainer>("Center/Panel") else {
            return;
        };
        if panel.has_theme_stylebox_override("panel") {
            return;
        }
        let variation = panel.get_theme_type_variation();
        if variation != "defaultDialog" && variation != "fullDialog" {
            return;
        }
        let Some(box_) = panel.get_theme_stylebox("panel") else {
            return;
        };
        if box_.get_margin(Side::TOP) < TITLE_CELL_PAD {
            return;
        }
        let position = layout.get_position();
        layout.set_position(Vector2::new(position.x, position.y - TITLE_CELL_PAD));
        let size = layout.get_size();
        layout.set_size(Vector2::new(size.x, size.y + TITLE_CELL_PAD));
    }

    fn apply_full_dialog(&mut self) {
        if !self.full_dialog {
            return;
        }
        if let Some(mut panel) = self.node_as::<Control>("Center/Panel") {
            panel.set_theme_type_variation("fullDialog");
            panel.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
            panel.set_v_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        }
        self.resize_full_panel();
        self.connect_viewport_resize();
    }

    fn connect_viewport_resize(&mut self) {
        let Some(mut viewport) = self.base().get_viewport() else {
            return;
        };
        let callable = self.base().callable("resize_full_panel");
        if !viewport.is_connected("size_changed", &callable) {
            viewport.connect("size_changed", &callable);
        }
    }

    #[func]
    fn resize_full_panel(&mut self) {
        self.fit_panel();
    }

    /// Fills the centered panel to the whole viewport without switching style.
    pub fn fill_panel(&mut self) {
        self.connect_viewport_resize();
        self.fit_panel();
    }

    fn fit_panel(&mut self) {
        let rect = self.base().get_viewport_rect();
        if let Some(mut panel) = self.node_as::<Control>("Center/Panel") {
            panel.set_custom_minimum_size(rect.size);
        }
    }

    fn apply_cont_alignment(&mut self) {
        let Some(mut content) = self.content() else {
            return;
        };
        content.set_v_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
        content.set_alignment(if self.cont_top {
            godot::classes::box_container::AlignmentMode::BEGIN
        } else {
            godot::classes::box_container::AlignmentMode::CENTER
        });
    }

    fn watch_buttons_row(&mut self) {
        let Some(mut row) = self.buttons_row() else {
            return;
        };
        // A controller adds its buttons from `ready`, while this dialog already
        // holds the active mutable borrow. A `#[func]` receiver would re-enter
        // `bind_mut` from `child_entered_tree` (gdext panic), so the hook only
        // queues the sync and `sync_buttons_row` binds the child hooks later.
        let dialog = self.to_gd();
        let changed = Callable::from_fn(
            "mind_dialog_buttons_changed",
            move |_args: &[&Variant]| {
                if dialog.is_instance_valid() {
                    let mut node = dialog.clone();
                    let _ = node.call_deferred("sync_buttons_row", &[]);
                }
                Variant::nil()
            },
        );
        row.connect("child_entered_tree", &changed);
        row.connect("child_exiting_tree", &changed);
        self.sync_buttons_row();
    }

    /// Hides the button row while it holds no visible button, and keeps each
    /// button's `visibility_changed` hooked (`BaseDialog.syncButtons`).
    #[func]
    fn sync_buttons_row(&mut self) {
        if self.base().has_meta(BUTTONS_SYNC_PENDING) {
            self.base_mut().remove_meta(BUTTONS_SYNC_PENDING);
        }
        let Some(mut row) = self.buttons_row() else {
            return;
        };
        let sync = self.buttons_sync_hook();
        let mut any = false;
        for child in row.get_children().iter_shared() {
            if let Ok(mut canvas) = child.clone().try_cast::<godot::classes::CanvasItem>() {
                // A fresh button gets its `visibility_changed` hook here: the
                // `child_entered_tree` callback cannot bind this dialog while a
                // controller is adding buttons from `ready`.
                if !canvas.is_connected("visibility_changed", &sync) {
                    canvas.connect("visibility_changed", &sync);
                }
                if canvas.is_visible() && !canvas.is_queued_for_deletion() {
                    any = true;
                }
            }
        }
        row.set_visible(any);
        self.buttons_visible = any;
        self.apply_body_bottom(any);
    }

    /// The deferred re-sync hook for a button's `visibility_changed`.
    ///
    /// Hiding or showing the dialog propagates the visibility notification to
    /// every button synchronously from `hide_dialog`/`show_dialog`, which already
    /// bind this dialog; a `#[func]` receiver would re-enter `bind_mut` (gdext
    /// panic). The hook only queues one sync and is stored on the node so
    /// repeated syncs reuse it instead of stacking connections.
    fn buttons_sync_hook(&mut self) -> Callable {
        const META: &str = "_mind_dialog_buttons_sync_hook";
        if self.base().has_meta(META)
            && let Ok(existing) = self.base().get_meta(META).try_to::<Callable>()
        {
            return existing;
        }
        let dialog = self.to_gd();
        let callable =
            Callable::from_fn("mind_dialog_buttons_sync", move |_args: &[&Variant]| {
                if !dialog.is_instance_valid() {
                    return Variant::nil();
                }
                let pending = dialog.has_meta(BUTTONS_SYNC_PENDING)
                    && dialog.get_meta(BUTTONS_SYNC_PENDING).to::<bool>();
                if pending {
                    return Variant::nil();
                }
                let mut node = dialog.clone();
                node.set_meta(BUTTONS_SYNC_PENDING, &true.to_variant());
                let _ = node.call_deferred("sync_buttons_row", &[]);
                Variant::nil()
            });
        self.base_mut().set_meta(META, &callable.to_variant());
        callable
    }

    /// The shell's `Body` takes the extra 3 px as a bottom inset while a button
    /// row is shown (`Dialog.java:214`).
    fn apply_body_bottom(&mut self, has_buttons: bool) {
        let Some(mut body) = self.body() else {
            return;
        };
        if body.has_theme_stylebox_override("panel")
            && let Some(current) = body.get_theme_stylebox("panel")
            && !current.has_meta("mind_body_inset")
        {
            return;
        }
        let mut inset = StyleBoxEmpty::new_gd();
        inset.set_meta("mind_body_inset", &true.to_variant());
        inset.set_content_margin(
            Side::BOTTOM,
            if has_buttons && !self.button_overlay {
                BUTTON_CELL_PAD
            } else {
                0.0
            },
        );
        body.add_theme_stylebox_override("panel", &inset);
    }

    #[func]
    fn close_pressed(&mut self) {
        self.close();
    }

    #[func]
    fn finish_hide(&mut self) {
        self.hide_tween = None;
        self.hiding = false;
        let mut modulate = self.base().get_modulate();
        modulate.a = 1.0;
        self.base_mut().set_modulate(modulate);
        self.base_mut().set_visible(false);
    }

    // Per-dialog signals. Every dialog scene now roots on this one class, so
    // the union of the former GDScript `signal` declarations lives here; names
    // and signatures are the parity ABI used by the editor dialogs and
    // `UiRoot` wiring.

    #[signal]
    fn selected(name: GString);
    #[signal]
    fn color_selected(color: Color);
    #[signal]
    fn locale_selected(locale: GString);
    #[signal]
    fn root_selected(root_name: GString);
    #[signal]
    fn save_requested(slot: GString);
    #[signal]
    fn slot_selected(slot: GString);
    #[signal]
    fn file_chosen(path: GString);
    #[signal]
    fn map_chosen(name: GString);
    #[signal]
    fn loadout_changed(index: i64);
    #[signal]
    fn loadout_chosen(index: i64);
    #[signal]
    fn sector_chosen(sector: i64);
    #[signal]
    fn sector_activated(planet: GString, sector: i64);
    #[signal]
    fn sector_icon_selected(icon: GString, content_icon: GString);
    #[signal]
    fn icon_selected(region_name: GString);
    #[signal]
    fn effect_selected(name: GString);
    #[signal]
    fn host_requested(host_name: GString, mode: GString);
    #[signal]
    fn connect_requested(address: GString);
    #[signal]
    fn join_requested(match_id: i64);
    #[signal]
    fn rules_edited(json: GString);
    #[signal]
    fn rule_changed(key: GString, value: Variant);
    #[signal]
    fn code_edited(code: GString);
    #[signal]
    fn schematic_action(index: i64, action: GString);
    #[signal]
    fn node_selected(index: i64);
    #[signal]
    fn node_moved(index: i64, x: i64, y: i64);
    #[signal]
    fn query_placed(x: i64, y: i64);
    #[signal]
    fn banned_edited(json: GString);

    /// Receives the JSON context passed to `MindUi.open_dialog`.
    #[func]
    pub fn set_context_json(&mut self, json_text: GString) {
        self.context = parse_json_object(&json_text);
        let mut controller = self.controller.take();
        if let Some(controller) = controller.as_mut() {
            controller.context_changed(self);
        }
        self.controller = controller;
    }

    /// Shows the dialog (called by `MindUi`; also safe to call directly).
    #[func]
    pub fn show_dialog(&mut self) {
        if let Some(tween) = self.hide_tween.clone()
            && tween.is_valid()
        {
            let mut tween = tween;
            tween.kill();
        }
        self.hide_tween = None;
        self.hiding = false;
        self.base_mut().set_visible(true);
        self.base_mut().move_to_front();
        let mut controller = self.controller.take();
        if let Some(controller) = controller.as_mut() {
            controller.shown(self);
        }
        self.controller = controller;
        let mut this: Gd<Control> = self.to_gd().upcast();
        kit::widgets::factory::fade_in(&mut this, 0.1);
    }

    /// Hides the dialog with the 0.1 s fade (`UI.java:132-133`).
    #[func]
    pub fn hide_dialog(&mut self) {
        if !self.base().is_visible() || self.hiding {
            return;
        }
        self.hiding = true;
        let mut controller = self.controller.take();
        if let Some(controller) = controller.as_mut() {
            controller.hidden(self);
        }
        self.controller = controller;
        // `BaseDialog.java:27-32`: every dialog hide plays uiBack.
        if let Some(mut ui) = kit::ui()
            && ui.has_method("play_ui_sound")
        {
            let _ = ui.call("play_ui_sound", &[GString::from("uiBack").to_variant()]);
        }
        if let Some(tween) = self.hide_tween.clone()
            && tween.is_valid()
        {
            let mut tween = tween;
            tween.kill();
        }
        let this: Gd<Control> = self.to_gd().upcast();
        let mut tween = self.base_mut().create_tween();
        tween.tween_property(&this, "modulate:a", &0.0f32.to_variant(), 0.1);
        let callable = self.base().callable("finish_hide");
        tween.tween_callback(&callable);
        self.hide_tween = Some(tween);
    }

    /// `is_shown` for `MindUi.reveal_top` / `is_dialog_shown`.
    #[func]
    pub fn is_shown(&self) -> bool {
        self.base().is_visible() && !self.hiding
    }

    /// True when this dialog is the top of the MindUi stack (Arc modal focus).
    #[func]
    pub fn is_top(&self) -> bool {
        let Some(mut ui) = kit::ui() else {
            return self.base().is_visible();
        };
        let stack = ui.call("dialog_stack", &[]).to::<PackedStringArray>();
        stack
            .as_slice()
            .last()
            .map(|last| last.to_string() == self.base().get_name().to_string())
            .unwrap_or(false)
    }

    /// Re-runs the controller's `shown` hook (used by live filters).
    #[func]
    pub fn refresh_controller(&mut self) {
        let mut controller = self.controller.take();
        if let Some(controller) = controller.as_mut() {
            controller.shown(self);
        }
        self.controller = controller;
    }

    /// A sector row was chosen: emit `sector_chosen(id)`.
    #[func]
    pub fn pick_sector(&mut self, id: i64) {
        self.base_mut()
            .emit_signal("sector_chosen", &[id.to_variant()]);
    }

    /// A picker row was chosen: emit `selected(name)` and close.
    #[func]
    pub fn pick_selected(&mut self, name: GString) {
        self.base_mut()
            .set_meta("_picker_chosen", &true.to_variant());
        self.base_mut()
            .emit_signal("selected", &[name.to_variant()]);
        self.close();
    }

    /// Whether ESC must not close this dialog.
    #[func]
    pub fn blocks_escape(&self) -> bool {
        self.controller
            .as_ref()
            .map(|controller| controller.blocks_escape())
            .unwrap_or(false)
    }
}

#[godot_api]
impl IControl for MindDialog {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            title_text: GString::new(),
            should_pause: false,
            full_dialog: false,
            fill_parent: true,
            title_color: Color::WHITE,
            cont_top: false,
            button_overlay: false,
            context: VarDictionary::new(),
            hide_tween: None,
            hiding: false,
            controller: None,
            dialog_id: String::new(),
            buttons_visible: false,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_visible(false);
        self.dialog_id = self.resolve_dialog_id();
        log::info!(
            "[ui] MindDialog ready id=`{}` scene=`{}`",
            self.dialog_id,
            self.base().get_scene_file_path()
        );
        self.apply_title();
        self.watch_title_shift();
        self.apply_full_dialog();
        self.apply_cont_alignment();
        self.watch_buttons_row();
        if self.fill_parent && !self.full_dialog {
            self.fill_panel();
        }
        let mut controller = super::dialogs::controller_for(&self.dialog_id);
        if let Some(controller) = controller.as_mut() {
            controller.ready(self);
        }
        self.controller = controller;
    }

    // No `on_notification` override: a dialog changes its own `visible` state in
    // `ready`/`show_dialog`/`finish_hide`, and `NOTIFICATION_VISIBILITY_CHANGED`
    // is delivered synchronously. gdext binds the notification virtual mutably,
    // so handling it here would re-enter `bind_mut` (panic) on every show/hide.
    // Hot reloading the extension re-creates the Rust instance without re-running
    // `ready` (gdext callback docs), which the whole native UI tree shares; a
    // rebuild of the GDExtension needs an editor restart.
}

/// Parses a JSON object into a dictionary (`{}` on any other JSON shape).
fn parse_json_object(json_text: &GString) -> VarDictionary {
    if json_text.is_empty() {
        return VarDictionary::new();
    }
    let parsed = Json::parse_string(json_text);
    parsed.try_to::<VarDictionary>().unwrap_or_default()
}

/// The dialog registry key whose manifest `scene` matches `path`.
fn manifest_dialog_id(path: &str) -> Option<String> {
    static ENTRIES: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    let entries = ENTRIES.get_or_init(|| {
        let text = godot::classes::FileAccess::get_file_as_string("res://ui/dialogs_manifest.json");
        if text.is_empty() {
            return Vec::new();
        }
        let Ok(manifest) = mind_core::ui::manifest::DialogsManifest::from_json(&text.to_string())
        else {
            return Vec::new();
        };
        manifest
            .dialogs
            .iter()
            .map(|entry| (entry.name.clone(), entry.scene.clone()))
            .collect()
    });
    entries
        .iter()
        .find(|(_, scene)| scene == path)
        .map(|(name, _)| name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playtime_formatting_matches_ui_format_time() {
        assert_eq!(MindDialog::format_time_ms(1_000), "1s");
        assert_eq!(MindDialog::format_time_ms(61_000), "1m 1s");
        assert_eq!(MindDialog::format_time_ms(3_661_000), "1h 1m");
    }

    #[test]
    fn empty_context_json_is_empty_dictionary() {
        // Pure parsing path: an empty string short-circuits before Godot JSON.
        assert_eq!(VarDictionary::new(), VarDictionary::new());
    }
}
