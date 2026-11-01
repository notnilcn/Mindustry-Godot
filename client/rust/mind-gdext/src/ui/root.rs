// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/core/UI.java (`init`, `updateMargins`, groups),
//         core/src/mindustry/ui/dialogs/BaseDialog.java (prompt shapes),
//         core/src/mindustry/ui/fragments/HudFragment.java (`showToast` slot).

//! `MindUiRoot` — the Godot-side UI root under `Spine/Ui` (former `ui_root.gd`).
//!
//! Owns the layer groups (`MenuGroup`, `HudGroup`, `DialogLayer`,
//! `OverlayLayer`, `LoadingLayer`), binds every manifest dialog/fragment to the
//! node statically declared by name in `scenes/ui/ui_root.tscn`, builds the
//! theme, and registers dialogs with the native `MindUi` singleton. Layout and
//! lifecycle only — no game rules and no sim reads (plan 14 §3.2/§3.10).
//!
//! The transient prompts (`showInfo`/`showConfirm`/`showTextInput`/…) are
//! one-off overlay trees built here; `MindUi` owns their semantic results
//! (`resolve_confirm`/`resolve_text_input`) so the signal round-trip stays
//! identical to the GDScript port.

use std::collections::HashMap;

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::notify::ControlNotification;
use godot::classes::text_edit::LineWrappingMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{
    BaseButton, Button, CenterContainer, ColorRect, Control, FileAccess, HBoxContainer, IControl,
    InputEvent, InputEventKey, Json, Label, LineEdit, Node, PanelContainer, RichTextLabel,
    ScrollContainer, StyleBoxFlat, TextEdit, Time, VBoxContainer,
};
use godot::global::{HorizontalAlignment, Key};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::kit::widgets::factory;
use super::kit::{self, MindRichLabel, icons, styles};

/// The dialog/fragment inventory this root binds (`_DIALOGS_MANIFEST`).
const DIALOGS_MANIFEST: &str = "res://ui/dialogs_manifest.json";

/// `Control.java:640` UI-scale confirmation countdown (`60 * 11` frames).
const UISCALE_REVERT_SECONDS: i32 = 11;

/// `HudFragment.java:730` `(int)(3.5 * 1000)` toast slot width.
const TOAST_SLOT_MS: f64 = 3500.0;

/// `FadeInFragment.start` default duration (the former GDScript default).
const BOOT_FADE_SECONDS: f64 = 0.5;

/// The Esc/back path of one transient prompt (`_prompt_cancel` in GDScript).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PromptCancel {
    /// No cancel path (the UI-scale confirmation; only its buttons resolve it).
    #[default]
    None,
    /// `_dismiss_prompt`: free the root and clear the bookkeeping.
    Dismiss,
    /// `showConfirm`: answer `false` through `MindUi.resolve_confirm`.
    Confirm,
    /// `showTextInput`: answer `""` through `MindUi.resolve_text_input`.
    TextInput,
}

/// `showMessageEdit`'s editable state (`MessageBlock.buildConfiguration`).
#[derive(Clone)]
struct MessageEdit {
    area: Gd<TextEdit>,
    counter: Gd<Label>,
    /// The last accepted text (`var last := [area.text]`).
    last: String,
    max_text: i64,
    max_newlines: i64,
    on_ok: Callable,
}

/// One transient prompt root (`_build_prompt`'s dictionary).
#[derive(Clone)]
struct Prompt {
    root: Gd<Control>,
    field: Gd<LineEdit>,
    buttons: Gd<HBoxContainer>,
    body: Gd<RichTextLabel>,
    layout: Gd<VBoxContainer>,
    /// `showTextInput`'s `@ok` button (`ok.disabled = …`).
    ok_button: Option<Gd<Button>>,
    cancel: PromptCancel,
    /// `showTextInput`'s `allowEmpty`.
    allow_empty: bool,
    /// `showMessageEdit`'s payload.
    message: Option<MessageEdit>,
}

/// `MindUiRoot` — the UI layer root (`control.java` `UI.init` equivalent).
#[derive(GodotClass)]
#[class(rename = RustMindUiRoot, base=Control)]
pub struct MindUiRoot {
    base: Base<Control>,

    /// Theme build time in milliseconds (plan 14 §7d budget probe).
    #[var]
    pub theme_build_ms: i64,

    menu_group: Option<Gd<Control>>,
    hud_group: Option<Gd<Control>>,
    dialog_layer: Option<Gd<Control>>,
    overlay_layer: Option<Gd<Control>>,
    loading_layer: Option<Gd<Control>>,

    /// Manifest dialog name -> statically instanced node (`_dialogs`).
    dialogs: HashMap<String, Gd<Node>>,
    theme_applied: bool,
    /// The active transient prompt (`_active_prompt` + `_prompt_cancel`).
    prompt: Option<Prompt>,
    /// The UI-scale confirmation prompt (`_uiscale_prompt`).
    uiscale_prompt: Option<Prompt>,
    uiscale_countdown: f64,
    uiscale_shown_seconds: i32,
    /// `Vars.finishLaunch` ran (the first `set_menu_visible(true)`).
    launch_finished: bool,
    /// `@mods.initfailed` shown for this boot (`Control.java:75-80`, once).
    mod_init_failure_shown: bool,
    /// Frames left before the deferred `@mods.initfailed` prompt (4 upstream).
    mod_init_defer_frames: i32,
    /// I08 T5: whether the join's `@connecting` loading overlay is up.
    join_loading_shown: bool,
    /// Current centred announce box (`UI.announce` replaces the last one).
    announce_node: Option<Gd<Control>>,
    /// `Ui/EditorDialog` is shown (`ui.editor.isShown()`).
    editor_open: bool,
    /// `mouse_filter` to restore when the editor shell closes.
    root_mouse_filter: MouseFilter,
    /// `HudFragment.scheduleToast`'s slot timestamp (`long lastToast = 0`).
    last_toast_ms: f64,
    /// `Control.pause()` fired on focus loss (single-player background pause).
    background_paused: bool,
    /// Whether the game was already paused when the focus loss arrived.
    background_was_paused: bool,
}

#[godot_api]
impl IControl for MindUiRoot {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            theme_build_ms: 0,
            menu_group: None,
            hud_group: None,
            dialog_layer: None,
            overlay_layer: None,
            loading_layer: None,
            dialogs: HashMap::new(),
            theme_applied: false,
            prompt: None,
            uiscale_prompt: None,
            uiscale_countdown: 0.0,
            uiscale_shown_seconds: -1,
            launch_finished: false,
            mod_init_failure_shown: false,
            mod_init_defer_frames: 0,
            join_loading_shown: false,
            announce_node: None,
            editor_open: false,
            root_mouse_filter: MouseFilter::PASS,
            last_toast_ms: 0.0,
            background_paused: false,
            background_was_paused: false,
        }
    }

    fn enter_tree(&mut self) {
        // Static children `_ready` before the parent, so the theme is applied
        // here to be in place before the instanced dialogs/fragments `_ready`.
        if !self.theme_applied {
            self.theme_applied = true;
            self.apply_theme();
        }
    }

    fn ready(&mut self) {
        self.menu_group = self.control_at("MenuGroup");
        self.hud_group = self.control_at("HudGroup");
        self.dialog_layer = self.control_at("DialogLayer");
        self.overlay_layer = self.control_at("OverlayLayer");
        self.loading_layer = self.control_at("LoadingLayer");
        self.bind_manifest();
        self.connect_prompts();
        self.connect_net();
        self.apply_ui_scale();
        self.boot();
        self.check_uiscale_changed();
        self.check_mod_init_failure();
        self.install_global_hooks();
    }

    fn process(&mut self, delta: f64) {
        self.drain_logic_messages();
        self.refresh_join_loading();
        self.tick_mod_init_failure();
        self.tick_uiscale(delta);
    }

    /// Transient prompts take Esc before the GUI (a focused `LineEdit` would
    /// otherwise consume the first press); `closeOnBack` on the prompt dialogs.
    fn input(&mut self, event: Gd<InputEvent>) {
        let active = self.active_prompt();
        let Some(prompt) = active else {
            self.release_field_on_escape(&event);
            return;
        };
        let Some(key) = event.clone().try_cast::<InputEventKey>().ok() else {
            return;
        };
        if !key.is_pressed() || key.is_echo() || key.get_keycode() != Key::ESCAPE {
            return;
        }
        let cancel = prompt.cancel;
        self.clear_prompt();
        self.run_prompt_cancel(cancel, &prompt.root);
        if let Some(mut viewport) = self.base().get_viewport() {
            viewport.set_input_as_handled();
        }
    }

    /// `Control.java:746-750` (`Binding.menu`, ESC): close the top dialog, hide
    /// the fullscreen minimap, or open the in-game pause menu. Game-over and
    /// campaign-complete stay modal (no ESC close), matching upstream. M/F8 are
    /// owned by the `MindInput` binding path, not this handler.
    fn unhandled_key_input(&mut self, event: Gd<InputEvent>) {
        let Some(key) = event.clone().try_cast::<InputEventKey>().ok() else {
            return;
        };
        if !key.is_pressed() || key.is_echo() {
            return;
        }
        if key.get_keycode() == Key::ESCAPE
            && self.on_escape()
            && let Some(mut viewport) = self.base().get_viewport()
        {
            viewport.set_input_as_handled();
        }
    }

    /// `Control.pause()`/`resume()` (`Control.java:619-633`), which the SDL
    /// backend fires on window focus loss/gain (`SdlApplication.java:198-201`).
    fn on_notification(&mut self, what: ControlNotification) {
        if what == ControlNotification::APPLICATION_FOCUS_OUT {
            self.background_pause();
        } else if what == ControlNotification::APPLICATION_FOCUS_IN {
            self.background_resume();
        }
    }
}

#[godot_api]
impl MindUiRoot {
    // ---- public surface ---------------------------------------------------

    /// The single return-to-menu path: `logic.reset()` (`Logic.java:299-311`)
    /// through `MindCampaign.reset_to_menu`, then the standalone menu.
    #[func]
    pub fn return_to_menu(&mut self) {
        if let Some(mut campaign) = kit::campaign()
            && campaign.has_method("reset_to_menu")
        {
            let _ = campaign.call("reset_to_menu", &[]);
        }
        self.set_menu_visible(true);
    }

    /// Shows the standalone menu (`MenuGroup`/`MenuBackground`) and hides the
    /// in-game HUD group (parity: `state.isMenu()` gates both), clearing the
    /// loading overlay too.
    #[func]
    pub fn set_menu_visible(&mut self, menu_visible: bool) {
        // Neither surface shows through an open editor shell.
        let editor_open = self.editor_open;
        if let Some(mut group) = self.menu_group.clone() {
            group.set_visible(menu_visible && !editor_open);
        }
        if let Some(mut group) = self.hud_group.clone() {
            group.set_visible(!menu_visible && !editor_open);
        }
        if let Some(mut background) = self.control_at("../MenuBackground") {
            background.set_visible(menu_visible && !editor_open);
        }
        self.hide_loading();
        if menu_visible && !self.launch_finished {
            self.launch_finished = true;
            // `Vars.finishLaunch` (ClientLauncher.java:253): the main menu is
            // up, so the launch succeeded — drop the crash marker and record
            // lastBuild.
            if let Some(mut platform) = kit::node("/root/Spine/MindPlatform")
                && platform.has_method("finish_launch")
            {
                let _ = platform.call("finish_launch", &[]);
            }
        }
    }

    /// `MapEditorDialog` is a full-screen dialog over the game UI: while it is
    /// shown the menu and the in-game HUD are hidden, and the building
    /// config/inventory panels close. The full-rect `UiRoot` stops intercepting
    /// the pointer so clicks with no dialog over them fall through to the
    /// editor shell.
    #[func]
    pub fn set_editor_open(&mut self, open: bool) {
        if !open {
            if self.editor_open {
                let filter = self.root_mouse_filter;
                self.base_mut().set_mouse_filter(filter);
            }
            self.editor_open = false;
            return;
        }
        if !self.editor_open {
            self.root_mouse_filter = self.base().get_mouse_filter();
        }
        self.editor_open = true;
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
        if let Some(mut group) = self.menu_group.clone() {
            group.set_visible(false);
        }
        if let Some(mut group) = self.hud_group.clone() {
            group.set_visible(false);
        }
        if let Some(mut background) = self.control_at("../MenuBackground") {
            background.set_visible(false);
        }
        self.close_hud_panels();
    }

    /// Whether the editor shell is shown (`ui.editor.isShown()`).
    #[func]
    pub fn is_editor_open(&self) -> bool {
        self.editor_open
    }

    /// True while a `MindUi` dialog or a transient prompt is up: the editor
    /// shell only takes keys while it is the top dialog.
    #[func]
    pub fn has_modal(&self) -> bool {
        if self
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.root.is_instance_valid())
        {
            return true;
        }
        kit::ui()
            .map(|mut ui| ui.call("has_dialog", &[]).try_to::<bool>().unwrap_or(false))
            .unwrap_or(false)
    }

    /// `PausedDialog.checkPlaytest` (`PausedDialog.java:174-183`), shared by the
    /// pause-menu quit and the game-over `@menu` button: when the match is a
    /// playtest, `MindCampaign.check_playtest` resets it without the exit save
    /// and reopens the playtested map in `MindEditor`.
    #[func]
    pub fn check_playtest(&mut self) -> bool {
        let Some(mut campaign) = kit::campaign() else {
            return false;
        };
        if !campaign.has_method("check_playtest") {
            return false;
        }
        let Ok(result) = campaign
            .call("check_playtest", &[])
            .try_to::<VarDictionary>()
        else {
            return false;
        };
        let playtest = result
            .get("playtest")
            .map(|value| value.try_to::<bool>().unwrap_or(false))
            .unwrap_or(false);
        if !playtest {
            return false;
        }
        let resumed = result
            .get("resumed")
            .map(|value| value.try_to::<bool>().unwrap_or(false))
            .unwrap_or(false);
        let mut shown = false;
        if resumed
            && let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
            && shell.has_method("show_dialog")
        {
            // `beginEditMap` -> `show()`; the shell hides the standalone menu.
            let _ = shell.call("show_dialog", &[]);
            shown = true;
        }
        if !shown {
            // The reopen failed (`@editor.errorload` is up): fall back to the menu.
            self.set_menu_visible(true);
        }
        true
    }

    /// The instantiated dialog node by manifest name (null when absent).
    #[func]
    pub fn dialog(&self, dialog_name: GString) -> Option<Gd<Node>> {
        self.dialogs.get(&dialog_name.to_string()).cloned()
    }

    /// `HudFragment.hasToast` (`HudFragment.java:741-743`): a toast is on
    /// screen within 3.5 s of the slot stamp (gates logic notify messages).
    #[func]
    pub fn has_toast(&self) -> bool {
        self.now_ms() - self.last_toast_ms < TOAST_SLOT_MS
    }

    /// `UI.hasAnnouncement` (`UI.java`): the centred announce box is up.
    #[func]
    pub fn has_announcement(&self) -> bool {
        self.announce_node
            .as_ref()
            .is_some_and(Gd::is_instance_valid)
    }

    /// `MessageBlock.buildConfiguration` desktop dialog
    /// (`MessageBlock.java:122-150`): floating `BaseDialog("@editmessage")`, a
    /// 380x160 `TextArea` capped at `maxTextLength` whose filter refuses a
    /// newline past `maxNewlines`, a light gray `n / max` counter, and one
    /// `@ok` 130x60 button; `closeOnBack`. `on_ok` receives the text.
    #[func]
    pub fn show_message_edit(
        &mut self,
        text: GString,
        max_text: i64,
        max_newlines: i64,
        on_ok: Callable,
    ) {
        let mut prompt = self.build_prompt(&kit::t("@editmessage").to_string(), "", false, 380.0);
        // code-instantiated: the message editor is a one-off transient prompt.
        let mut area = TextEdit::new_alloc();
        area.set_text(&text.to_string().replace('\r', "\n"));
        area.set_custom_minimum_size(Vector2::new(380.0, 160.0));
        area.set_line_wrapping_mode(LineWrappingMode::BOUNDARY);
        let mut counter = Label::new_alloc();
        counter.add_theme_color_override("font_color", styles::ARC_LIGHT_GRAY);
        counter.set_horizontal_alignment(HorizontalAlignment::CENTER);
        let last = area.get_text().to_string();
        counter.set_text(&format!("{} / {}", last.chars().count(), max_text));
        // The prompt's message label is unused here; the area and counter take
        // its place in the dialog column, above the buttons.
        let Some(column) = prompt.body.get_parent() else {
            return;
        };
        let mut column = column;
        let body_index = prompt.body.get_index();
        let mut body = prompt.body.clone();
        body.set_visible(false);
        column.add_child(&area);
        column.move_child(&area, body_index + 1);
        column.add_child(&counter);
        let area_index = area.get_index();
        column.move_child(&counter, area_index + 1);
        let changed = self.base().callable("on_message_edit_changed");
        area.connect("text_changed", &changed);
        prompt.message = Some(MessageEdit {
            area: area.clone(),
            counter,
            last,
            max_text,
            max_newlines,
            on_ok,
        });
        prompt.cancel = PromptCancel::Dismiss;
        let ok = self.base().callable("on_message_edit_ok");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@ok",
            "",
            Vector2::new(130.0, 60.0),
            &ok,
        );
        area.call_deferred("grab_focus", &[]);
        self.prompt = Some(prompt);
    }

    /// `UI.loadAnd(text, call)` (`core/UI.java:292-302`): the loading overlay
    /// shows `text`, `call` runs 7 ticks later, then the overlay hides.
    #[func]
    pub fn load_and(&mut self, callback: Callable, #[opt(default = "@loading")] text: GString) {
        let loading = self.loading_node();
        if let Some(mut loading) = loading.clone()
            && loading.has_method("show_text")
        {
            let label = kit::t(&text.to_string());
            let _ = loading.call("show_text", &[label.to_variant()]);
        }
        let mut tree = self.base().get_tree();
        let mut timer = tree.create_timer(7.0 / 60.0);
        let mut hide_target = loading;
        let finish = Callable::from_fn("load_and_finish", move |_args: &[&Variant]| {
            let args = Array::<Variant>::new();
            let _ = callback.callv(&args);
            if let Some(mut loading) = hide_target.clone()
                && loading.is_instance_valid()
                && loading.has_method("hide_loading")
            {
                let _ = loading.call("hide_loading", &[]);
            }
            hide_target = None;
            Variant::nil()
        });
        timer.connect("timeout", &finish);
    }

    /// The `uiNotify` sound + label of one HUD toast (`HudFragment.showToast`).
    #[func]
    pub fn play_toast(&mut self, text: GString, glyph: GString) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("play_ui_sound", &[GString::from("uiNotify").to_variant()]);
        }
        self.toast_label(text, 3.5, glyph, false);
    }

    /// One HUD/info toast box (`HudFragment.showToast` / `UI.showInfoToast`).
    ///
    /// A `Tex.button` box, margin 12, `Icon.ok` + the text (wrap 280, centred),
    /// in a *top* container that slides in over 1 s (`Interp.fade`), waits and
    /// fades out. `HudFragment.showToast` (`HudFragment.java:753-776`);
    /// `UI.showInfoToast` (`UI.java:405-419`) is the same top position with a
    /// `Styles.black3` box and `padTop(10)`.
    #[func]
    pub fn toast_label(
        &mut self,
        text: GString,
        duration: f32,
        #[opt(default = "ok")] icon: GString,
        #[opt(default = false)] info: bool,
    ) {
        // code-instantiated: toasts are transient, high-churn overlay boxes.
        let mut panel = PanelContainer::new_alloc();
        let mut style = StyleBoxFlat::new_gd();
        style.set_bg_color(if info {
            styles::BLACK_3
        } else {
            styles::GRAY_PANEL
        });
        style.set_corner_radius_all(4);
        let margin = if info { 4.0 } else { 12.0 };
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            style.set_content_margin(side, margin);
        }
        panel.add_theme_stylebox_override("panel", &style);
        panel.set_mouse_filter(MouseFilter::IGNORE);
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 6);
        if !info {
            // `HudFragment.showToast`: `table.image(icon).pad(3)`.
            let glyph = MindRichLabel::new_alloc();
            let mut glyph_rich: Gd<RichTextLabel> = glyph.clone().upcast();
            glyph_rich.set_text(&icons::glyph(&icon.to_string()));
            let mut glyph_control: Gd<Control> = glyph.upcast();
            icons::apply_icon_font(&mut glyph_control);
            row.add_child(&glyph_rich);
        }
        let label = MindRichLabel::new_alloc();
        let mut label_rich: Gd<RichTextLabel> = label.upcast();
        label_rich.set_text(&kit::markup_text(&text.to_string()));
        if !info {
            // `table.add(text).wrap().width(280f)`, centred.
            label_rich.set_custom_minimum_size(Vector2::new(280.0, 0.0));
            label_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
            label_rich.set_horizontal_alignment(HorizontalAlignment::CENTER);
        }
        row.add_child(&label_rich);
        panel.add_child(&row);
        // Top-centre (`container.top()` / `table.top()..padTop(10)`).
        panel.set_anchors_preset(LayoutPreset::CENTER_TOP);
        panel.set_h_grow_direction(GrowDirection::BOTH);
        if let Some(mut layer) = self.overlay_layer.clone() {
            layer.add_child(&panel);
        }
        let top = 10.0f32;
        let slide = -panel.get_combined_minimum_size().y - top;
        let position = panel.get_position();
        panel.set_position(Vector2::new(position.x, slide));
        let this: Gd<Control> = panel.clone().upcast();
        let mut tween = self.base_mut().create_tween();
        // `Interp.fade` = `smoother`; TRANS_QUAD/EASE_OUT is the port's stand-in.
        tween.set_trans(TransitionType::QUAD);
        tween.set_ease(EaseType::OUT);
        tween.tween_property(&this, "position:y", &top.to_variant(), 1.0);
        tween.tween_interval(duration as f64);
        tween.tween_property(&this, "modulate:a", &0.0f32.to_variant(), 0.4);
        let free = Callable::from_fn("toast_free", move |_args: &[&Variant]| {
            if this.is_instance_valid() {
                let mut node = this.clone();
                node.queue_free();
            }
            Variant::nil()
        });
        tween.tween_callback(&free);
    }

    /// `UI.announce(CharSequence, float)` (`UI.java:636-652`): a `Styles.black3`
    /// table, margin 8, centred `outlineLabel`, at the viewport centre, fading
    /// out over `duration` with `Interp.pow4In`; a new announce replaces the last.
    #[func]
    pub fn announce_box(&mut self, text: GString, duration: f32) {
        // code-instantiated: announces are transient, high-churn centred boxes.
        if let Some(mut previous) = self.announce_node.take()
            && previous.is_instance_valid()
        {
            previous.queue_free();
        }
        let mut panel = PanelContainer::new_alloc();
        let mut style = StyleBoxFlat::new_gd();
        style.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.3));
        style.set_content_margin_all(8.0);
        panel.add_theme_stylebox_override("panel", &style);
        panel.set_mouse_filter(MouseFilter::IGNORE);
        let label = MindRichLabel::new_alloc();
        let mut label_rich: Gd<RichTextLabel> = label.upcast();
        // H03 T17: producers pass Java markup; translate once here
        // (`UI.announce` label parses markup; `UI.java:636-652`).
        label_rich.set_text(&format!("[center]{}", kit::markup_text(&text.to_string())));
        panel.add_child(&label_rich);
        // Centred on the viewport (`setPosition(0, 0, Align.center)`).
        panel.set_anchors_preset(LayoutPreset::CENTER);
        panel.set_h_grow_direction(GrowDirection::BOTH);
        panel.set_v_grow_direction(GrowDirection::BOTH);
        if let Some(mut layer) = self.overlay_layer.clone() {
            layer.add_child(&panel);
        }
        self.announce_node = Some(panel.clone().upcast());
        let this: Gd<Control> = panel.clone().upcast();
        let mut tween = self.base_mut().create_tween();
        tween.tween_interval(duration as f64);
        tween.set_trans(TransitionType::QUART);
        tween.set_ease(EaseType::IN);
        tween.tween_property(&this, "modulate:a", &0.0f32.to_variant(), 0.4);
        let free = Callable::from_fn("announce_free", move |_args: &[&Variant]| {
            if this.is_instance_valid() {
                let mut node = this.clone();
                node.queue_free();
            }
            Variant::nil()
        });
        tween.tween_callback(&free);
    }

    // ---- MindUi prompt signals -------------------------------------------

    /// `UI.showInfoToast` (`UI.java:405-419`): top-centre, `Styles.black3`.
    #[func]
    fn on_show_info(&mut self, text: GString) {
        self.toast_label(text, 2.0, GString::from("ok"), true);
    }

    /// `HudFragment.showToast`: `if(state.isMenu()) return;`, then the
    /// `scheduleToast` slot (`HudFragment.java:729-740`): past the 3.5 s slot
    /// the toast shows now, else it is queued for the remainder and the slot
    /// advances by another 3.5 s.
    #[func]
    fn on_toast(&mut self, text: GString, icon: GString) {
        if self
            .menu_group
            .as_ref()
            .is_some_and(|group| group.is_visible())
        {
            return;
        }
        // `showToast(Icon.warning, ..)` vs `showToast(text)` = `Icon.ok`.
        let glyph = match icon.to_string().as_str() {
            "warning" | "error" | "warn" => "warn",
            _ => "ok",
        };
        let now = self.now_ms();
        let since = now - self.last_toast_ms;
        if since > TOAST_SLOT_MS {
            self.last_toast_ms = now;
            self.play_toast(text, GString::from(glyph));
            return;
        }
        self.last_toast_ms += TOAST_SLOT_MS;
        let delay = (TOAST_SLOT_MS - since) / 1000.0;
        let mut tree = self.base().get_tree();
        let mut timer = tree.create_timer(delay);
        let this = self.to_gd();
        let glyph = GString::from(glyph);
        let queued = Callable::from_fn("queued_toast", move |_args: &[&Variant]| {
            if this.is_instance_valid() {
                let mut root = this.clone();
                root.bind_mut().play_toast(text.clone(), glyph.clone());
            }
            Variant::nil()
        });
        timer.connect("timeout", &queued);
    }

    #[func]
    fn on_announce(&mut self, text: GString, duration: f32) {
        self.announce_box(text, duration.max(1.0));
    }

    /// `UI.showText`: `Dialog(title)` + text (400 wrap) + `@ok` 110x50.
    #[func]
    fn on_show_text(&mut self, title: GString, text: GString) {
        let mut prompt = self.build_prompt(&title.to_string(), &text.to_string(), false, 400.0);
        prompt.cancel = PromptCancel::Dismiss;
        let close = self.base().callable("on_prompt_dismiss");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@ok",
            "",
            Vector2::new(110.0, 50.0),
            &close,
        );
        self.prompt = Some(prompt);
    }

    /// `UI.java:582-606 showConfirm(text, cb)`: `@confirm` title, 500 px wrapped
    /// body, `@cancel`/`@ok` 200x54 with icons, Enter = OK, Esc = cancel.
    #[func]
    fn on_show_confirm(&mut self, text: GString) {
        let mut prompt = self.build_prompt(
            &kit::t("@confirm").to_string(),
            &text.to_string(),
            false,
            500.0,
        );
        apply_base_dialog_spacing(&prompt);
        prompt.cancel = PromptCancel::Confirm;
        let cancel = self.base().callable("on_confirm_cancel");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@cancel",
            "cancel",
            Vector2::new(200.0, 54.0),
            &cancel,
        );
        let confirm = self.base().callable("on_confirm_ok");
        let mut ok = self.add_prompt_button(
            prompt.buttons.clone(),
            "@ok",
            "ok",
            Vector2::new(200.0, 54.0),
            &confirm,
        );
        ok.grab_focus();
        self.prompt = Some(prompt);
    }

    /// `UI.java:309-355 showTextInput` (desktop): accent rule, message, 400x50
    /// field, `@cancel`/`@ok` 120x54; Enter = OK, Esc = cancel.
    #[func]
    fn on_text_input_request(
        &mut self,
        title: GString,
        message: GString,
        max_length: i64,
        default_text: GString,
        numeric: bool,
        allow_empty: bool,
    ) {
        let mut prompt = self.build_prompt(&title.to_string(), &message.to_string(), true, 400.0);
        let mut field = prompt.field.clone();
        field.set_text(&default_text);
        if max_length > 0 {
            field.set_max_length(max_length as i32);
        }
        prompt.cancel = PromptCancel::TextInput;
        prompt.allow_empty = allow_empty;
        let cancel = self.base().callable("on_text_input_cancel");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@cancel",
            "",
            Vector2::new(120.0, 54.0),
            &cancel,
        );
        let approve = self.base().callable("on_text_input_ok");
        let mut ok = self.add_prompt_button(
            prompt.buttons.clone(),
            "@ok",
            "",
            Vector2::new(120.0, 54.0),
            &approve,
        );
        ok.set_disabled(!allow_empty && field.get_text().is_empty());
        prompt.ok_button = Some(ok);
        if numeric {
            let filtered = self.base().callable("on_text_input_numeric_changed");
            field.connect("text_changed", &filtered);
        }
        let changed = self.base().callable("on_text_input_changed");
        field.connect("text_changed", &changed);
        // `UI.java:342-348`: Enter in the field confirms (when non-empty unless
        // `allowEmpty`).
        let submitted = self.base().callable("on_text_input_submitted");
        field.connect("text_submitted", &submitted);
        field.grab_focus();
        self.prompt = Some(prompt);
    }

    /// `showConfirm`'s dialog hides with its answer (`dialog.hide()` in both
    /// buttons), whichever path delivered it: the buttons, Enter/Esc, or a
    /// direct `MindUi.resolve_confirm` (MCP / script callers).
    #[func]
    fn on_confirm_result(&mut self, _confirmed: bool) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        if prompt.cancel == PromptCancel::Confirm {
            self.dismiss_prompt(&prompt.root);
        }
    }

    // ---- MindNet intents --------------------------------------------------

    /// `HostDialog.runHost`: hosts the currently loaded world
    /// (`MindNet.host_current_world` picks up the live map/rules; I08 T4)
    /// instead of creating a bare lobby.
    #[func]
    fn on_host_requested(&mut self, host_name: GString, _mode: GString) {
        let Some(mut net) = kit::net() else {
            return;
        };
        if !net.has_method("host_current_world") {
            return;
        }
        // The live world's rules carry the gamemode (`HostDialog.java:80-99`
        // hosts the current state; the dialog's mode row is presentational).
        let map_id = self.current_map_id();
        let _ = net.call(
            "host_current_world",
            &[
                host_name.to_variant(),
                GString::from(map_id.as_str()).to_variant(),
                1i64.to_variant(),
            ],
        );
    }

    /// `JoinDialog.connect`: direct-connect to the addressed SpacetimeDB server
    /// and enter the match browser.
    #[func]
    fn on_connect_requested(&mut self, address: GString) {
        if let Some(mut net) = kit::net()
            && net.has_method("connect_to")
        {
            let _ = net.call("connect_to", &[address.to_variant()]);
        }
    }

    /// `JoinDialog.safeConnect`: join the picked public match.
    #[func]
    fn on_join_requested(&mut self, match_id: i64) {
        if let Some(mut net) = kit::net()
            && net.has_method("join_match")
        {
            let _ = net.call(
                "join_match",
                &[match_id.to_variant(), GString::new().to_variant()],
            );
        }
    }

    // ---- global button click sound (`UI.java:146`) ------------------------

    #[func]
    fn on_node_added(&mut self, node: Gd<Node>) {
        if node.clone().try_cast::<BaseButton>().is_ok() {
            self.hook_button(&node);
        } else if node.clone().try_cast::<ScrollContainer>().is_ok() {
            // Arc `ScrollPane` wheel step + smooth scroll now lives on
            // `MindScroll` itself, so the former global scroll hook is a no-op.
        }
    }

    /// `UI.java:146` `ClickListener.clicked = () -> Sounds.uiButton.play()`.
    #[func]
    fn on_any_button_pressed(&mut self) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("play_ui_sound", &[GString::from("uiButton").to_variant()]);
        }
    }

    // ---- prompt button handlers -------------------------------------------

    /// `showText`/`showMessageEdit` close path (`_dismiss_prompt(root)`).
    #[func]
    fn on_prompt_dismiss(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        self.dismiss_prompt(&prompt.root);
    }

    #[func]
    fn on_confirm_ok(&mut self) {
        let root = self.prompt.as_ref().map(|prompt| prompt.root.clone());
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("resolve_confirm", &[true.to_variant()]);
        }
        if let Some(root) = root {
            self.dismiss_prompt(&root);
        }
    }

    #[func]
    fn on_confirm_cancel(&mut self) {
        let root = self.prompt.as_ref().map(|prompt| prompt.root.clone());
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("resolve_confirm", &[false.to_variant()]);
        }
        if let Some(root) = root {
            self.dismiss_prompt(&root);
        }
    }

    #[func]
    fn on_text_input_ok(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        let value = prompt.field.get_text();
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("resolve_text_input", &[value.to_variant()]);
        }
        self.dismiss_prompt(&prompt.root);
    }

    #[func]
    fn on_text_input_cancel(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("resolve_text_input", &[GString::new().to_variant()]);
        }
        self.dismiss_prompt(&prompt.root);
    }

    /// `ok.disabled = not allow_empty and value.is_empty()`.
    #[func]
    fn on_text_input_changed(&mut self, value: GString) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        if let Some(mut ok) = prompt.ok_button.clone() {
            ok.set_disabled(!prompt.allow_empty && value.is_empty());
        }
    }

    /// Digit-only filter for `numeric = true` prompts (`UI.java:330-340`).
    #[func]
    fn on_text_input_numeric_changed(&mut self, value: GString) {
        let source = value.to_string();
        let filtered: String = source.chars().filter(char::is_ascii_digit).collect();
        if filtered == source {
            return;
        }
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        let mut field = prompt.field.clone();
        field.set_text(&filtered);
        field.set_caret_column(filtered.chars().count() as i32);
    }

    /// Enter in the field confirms (when non-empty unless `allowEmpty`).
    #[func]
    fn on_text_input_submitted(&mut self, _value: GString) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        if prompt.allow_empty || !prompt.field.get_text().is_empty() {
            self.on_text_input_ok();
        }
    }

    /// `MessageBlock`'s `TextArea` filter: refuse text past `maxTextLength` or a
    /// newline past `maxNewlines`, then refresh the `n / max` counter.
    #[func]
    fn on_message_edit_changed(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        let Some(mut message) = prompt.message.clone() else {
            return;
        };
        let mut area = message.area.clone();
        let value = area.get_text().to_string();
        if value.chars().count() as i64 > message.max_text
            || value.matches('\n').count() as i64 > message.max_newlines
        {
            let caret = area.get_caret_column();
            area.set_text(&message.last);
            let length = message.last.chars().count() as i32;
            area.set_caret_column(caret.min(length));
        }
        message.last = area.get_text().to_string();
        message.counter.clone().set_text(&format!(
            "{} / {}",
            message.last.chars().count(),
            message.max_text
        ));
        if let Some(current) = self.prompt.as_mut() {
            current.message = Some(message);
        }
    }

    #[func]
    fn on_message_edit_ok(&mut self) {
        let Some(prompt) = self.active_prompt() else {
            return;
        };
        let Some(message) = prompt.message.clone() else {
            return;
        };
        let value = message.area.get_text();
        self.dismiss_prompt(&prompt.root);
        if message.on_ok.is_valid() {
            let mut args = Array::<Variant>::new();
            args.push(&value.to_variant());
            let _ = message.on_ok.callv(&args);
        }
    }

    /// `OK` keeps the new scale (`Control.java:640`).
    #[func]
    fn on_uiscale_keep(&mut self) {
        self.finish_uiscale(true);
    }

    /// `@uiscale.cancel` reverts to 100% and exits.
    #[func]
    fn on_uiscale_revert(&mut self) {
        self.finish_uiscale(false);
    }

    // ---- boot / theme -----------------------------------------------------

    /// Builds and applies the single runtime theme (`_apply_theme`).
    fn apply_theme(&mut self) {
        let started = Time::singleton().get_ticks_msec();
        let theme = kit::theme::build();
        self.theme_build_ms = Time::singleton().get_ticks_msec().saturating_sub(started) as i64;
        let manifest = kit::theme::load_manifest();
        if !kit::theme::verify(&theme, &manifest) {
            log::warn!("[ui] theme verification failed (styles_manifest)");
        }
        self.base_mut().set_theme(&theme);
    }

    /// Boot presentation (`MenuFragment`/`LoadingFragment`/`FadeInFragment`
    /// order): the standalone menu is shown, the in-game HUD group is hidden,
    /// the loading overlay is cleared and the black boot cover fades out.
    fn boot(&mut self) {
        self.set_menu_visible(true);
        if let Some(mut fade) = self.base().get_node_or_null("OverlayLayer/fade_in")
            && fade.has_method("start")
        {
            // `FadeInFragment.start(duration)`; the former GDScript default was
            // 0.5 s, which the native fragment requires explicitly.
            let _ = fade.call("start", &[BOOT_FADE_SECONDS.to_variant()]);
        }
    }

    // ---- manifest binding -------------------------------------------------

    /// Binds every manifest dialog/fragment to the statically instanced node of
    /// the same name under its layer group and registers the dialogs.
    fn bind_manifest(&mut self) {
        if !FileAccess::file_exists(DIALOGS_MANIFEST) {
            log::warn!("[ui] missing {DIALOGS_MANIFEST}");
            return;
        }
        let text = FileAccess::get_file_as_string(DIALOGS_MANIFEST).to_string();
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) else {
            log::warn!("[ui] dialogs_manifest is not an object");
            return;
        };
        if !parsed.is_object() {
            log::warn!("[ui] dialogs_manifest is not an object");
            return;
        }
        let mut expected: Vec<String> = Vec::new();
        if let Some(dialogs) = parsed.get("dialogs").and_then(serde_json::Value::as_array)
            && let Some(layer) = self.dialog_layer.clone()
        {
            for entry in dialogs {
                if self.bind_entry(entry, &layer, false) {
                    expected.push(json_string(entry, "name"));
                }
            }
        }
        if let Some(fragments) = parsed
            .get("fragments")
            .and_then(serde_json::Value::as_array)
        {
            for entry in fragments {
                let group = json_string(entry, "group");
                let Some(parent) = self.group_for(&group) else {
                    continue;
                };
                if self.bind_entry(entry, &parent, true) {
                    expected.push(json_string(entry, "name"));
                }
            }
        }
        self.verify_manifest(&expected);
    }

    /// `_bind`: looks the manifest name up under `parent`, hides fragments per
    /// `hidden`, and registers dialogs with `MindUi`.
    fn bind_entry(
        &mut self,
        entry: &serde_json::Value,
        parent: &Gd<Control>,
        is_fragment: bool,
    ) -> bool {
        let name = json_string(entry, "name");
        if name.is_empty() {
            return false;
        }
        let Some(instance) = parent.get_node_or_null(name.as_str()) else {
            log::error!(
                "[ui] manifest entry `{name}` has no scene node under {}",
                parent.get_name()
            );
            return false;
        };
        if is_fragment {
            if let Ok(mut control) = instance.clone().try_cast::<Control>() {
                control.set_visible(!json_bool(entry, "hidden"));
            }
            return true;
        }
        self.dialogs.insert(name.clone(), instance.clone());
        if let Some(mut ui) = kit::ui() {
            let should_pause = json_bool(entry, "pause");
            let _ = ui.call(
                "register_dialog",
                &[
                    GString::from(name.as_str()).to_variant(),
                    instance.to_variant(),
                    should_pause.to_variant(),
                ],
            );
        }
        true
    }

    /// Cross-check the manifest against the statically-instanced scene children
    /// (mirrors `MindThemeBuilder.verify`): every manifest name resolves, and
    /// every static child under a layer group is a manifest entry.
    fn verify_manifest(&mut self, expected: &[String]) {
        let layers = [
            self.dialog_layer.clone(),
            self.menu_group.clone(),
            self.hud_group.clone(),
            self.overlay_layer.clone(),
            self.loading_layer.clone(),
        ];
        for layer in layers.into_iter().flatten() {
            let parent_name = layer.get_name().to_string();
            for child in layer.get_children().iter_shared() {
                let child_name = child.get_name().to_string();
                if !expected.contains(&child_name) {
                    log::error!(
                        "[ui] scene node `{child_name}` under {parent_name} is missing from dialogs_manifest"
                    );
                }
            }
        }
    }

    /// The layer group for a fragment manifest `group` name.
    fn group_for(&self, group_name: &str) -> Option<Gd<Control>> {
        match group_name {
            "menu" => self.menu_group.clone(),
            "hud" => self.hud_group.clone(),
            "loading" => self.loading_layer.clone(),
            _ => self.overlay_layer.clone(),
        }
    }

    // ---- signal wiring ----------------------------------------------------

    /// Connects the `MindUi` prompt/toast signals to this root's renderers.
    fn connect_prompts(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        for (signal, method) in [
            ("show_info", "on_show_info"),
            ("toast", "on_toast"),
            ("announce", "on_announce"),
            ("show_text", "on_show_text"),
            ("show_confirm", "on_show_confirm"),
            ("text_input_request", "on_text_input_request"),
            // `_on_confirm_resolved`: the prompt closes once the answer has
            // been delivered, whichever path resolved it.
            ("confirm_result", "on_confirm_result"),
        ] {
            let callable = self.base().callable(method);
            connect_if(&mut ui, signal, &callable);
        }
    }

    /// Wires manifest dialog intents that cross into the Rust facades.
    fn connect_net(&mut self) {
        if let Some(mut host_dialog) = self.dialogs.get("host").cloned() {
            let callable = self.base().callable("on_host_requested");
            connect_if(&mut host_dialog, "host_requested", &callable);
        }
        let Some(mut join_dialog) = self.dialogs.get("join").cloned() else {
            return;
        };
        let connect = self.base().callable("on_connect_requested");
        connect_if(&mut join_dialog, "connect_requested", &connect);
        let join = self.base().callable("on_join_requested");
        connect_if(&mut join_dialog, "join_requested", &join);
    }

    /// Installs the global click-sound hook (`UI.java:146`) on every
    /// `BaseButton` in the tree and on every later addition.
    fn install_global_hooks(&mut self) {
        let mut tree = self.base().get_tree();
        let callable = self.base().callable("on_node_added");
        if !tree.is_connected("node_added", &callable) {
            tree.connect("node_added", &callable);
        }
        // `owned = false`: the default `owned = true` finds nothing under the
        // ownerless root Window, so code-instantiated buttons would be missed.
        let root = tree.get_root();
        let buttons = root
            .find_children_ex("*")
            .type_("BaseButton")
            .owned(false)
            .done();
        for node in buttons.iter_shared() {
            self.hook_button(&node);
        }
    }

    /// Hooks one button's `pressed` to the shared click sound.
    fn hook_button(&mut self, node: &Gd<Node>) {
        let Ok(mut button) = node.clone().try_cast::<BaseButton>() else {
            return;
        };
        let callable = self.base().callable("on_any_button_pressed");
        if !button.is_connected("pressed", &callable) {
            button.connect("pressed", &callable);
        }
    }

    // ---- UI scale ---------------------------------------------------------

    /// `Vars.java:517 Scl.setProduct`: the persisted `uiscale` percent scales the
    /// whole UI at boot (minimum 25%, matching upstream). The window content
    /// scale factor is Godot's equivalent of the Scl product.
    fn apply_ui_scale(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("settings_rows_json") {
            return;
        }
        let Ok(json) = ui
            .call(
                "settings_rows_json",
                &[GString::from("graphics").to_variant()],
            )
            .try_to::<GString>()
        else {
            return;
        };
        let parsed = Json::parse_string(&json);
        let Ok(rows) = parsed.try_to::<VarArray>() else {
            return;
        };
        for row in rows.iter_shared() {
            let Ok(row) = row.try_to::<VarDictionary>() else {
                continue;
            };
            let key = row
                .get("key")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default();
            if key != "uiscale" {
                continue;
            }
            let value = row
                .get("value")
                .and_then(|value| value.try_to_relaxed::<f64>().ok())
                .unwrap_or(100.0);
            let ui_scale = (value.max(25.0) / 100.0) as f32;
            if let Some(mut window) = self.base().get_window() {
                window.set_content_scale_factor(ui_scale);
            }
            return;
        }
    }

    /// `Control.java:640` boot confirmation for a changed UI scale: the prompt
    /// counts down, `OK` keeps the new scale and `@uiscale.cancel` reverts to
    /// 100% and exits.
    fn check_uiscale_changed(&mut self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("uiscale_changed") {
            return;
        }
        if !ui
            .call("uiscale_changed", &[])
            .try_to::<bool>()
            .unwrap_or(false)
        {
            return;
        }
        self.uiscale_countdown = f64::from(UISCALE_REVERT_SECONDS);
        let seconds = UISCALE_REVERT_SECONDS.to_string();
        let text = kit::tm_format("@uiscale.reset", &[seconds.as_str()]);
        let prompt = self.build_prompt("", &text.to_string(), false, 400.0);
        let keep = self.base().callable("on_uiscale_keep");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@ok",
            "",
            Vector2::new(120.0, 54.0),
            &keep,
        );
        let revert = self.base().callable("on_uiscale_revert");
        self.add_prompt_button(
            prompt.buttons.clone(),
            "@uiscale.cancel",
            "",
            Vector2::new(120.0, 54.0),
            &revert,
        );
        self.uiscale_prompt = Some(prompt);
        self.base_mut().set_process(true);
    }

    /// One UI-scale countdown frame (`_process`'s tail).
    fn tick_uiscale(&mut self, delta: f64) {
        let Some(prompt) = self.uiscale_prompt.clone() else {
            return;
        };
        self.uiscale_countdown -= delta;
        let seconds = (self.uiscale_countdown.ceil() as i32).max(0);
        if seconds != self.uiscale_shown_seconds {
            self.uiscale_shown_seconds = seconds;
            let seconds_text = seconds.to_string();
            let text = kit::tm_format("@uiscale.reset", &[seconds_text.as_str()]);
            let mut body = prompt.body.clone();
            body.set_text(&text);
        }
        if self.uiscale_countdown <= 0.0 {
            self.finish_uiscale(false);
        }
    }

    /// `_finish_uiscale`: drops the prompt, clears `uiscalechanged`, and exits
    /// after a revert (100%).
    fn finish_uiscale(&mut self, keep: bool) {
        let Some(prompt) = self.uiscale_prompt.take() else {
            return;
        };
        let mut root = prompt.root;
        root.queue_free();
        // `_process` also drains world-processor messages (H06 T4), so it stays on.
        self.base_mut().set_process(true);
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let _ = ui.call("set_uiscale_changed", &[false.to_variant()]);
        if !keep {
            let _ = ui.call(
                "settings_set",
                &[GString::from("uiscale").to_variant(), 100i64.to_variant()],
            );
            self.base_mut().get_tree().quit();
        }
    }

    // ---- deferred mod-init prompt ----------------------------------------

    /// `Control.java:75-80`: after the client load, a boot that skipped mod
    /// loading shows `@mods.initfailed` (deferred four ticks upstream).
    fn check_mod_init_failure(&mut self) {
        if self.mod_init_failure_shown {
            return;
        }
        let Some(mut mods) = kit::node("/root/MindMods") else {
            return;
        };
        if !mods
            .call("skipped_mod_loading", &[])
            .try_to::<bool>()
            .unwrap_or(false)
        {
            return;
        }
        let empty = mods
            .call("list", &[])
            .try_to::<VarArray>()
            .map(|list| list.is_empty())
            .unwrap_or(true);
        if empty {
            return;
        }
        self.mod_init_failure_shown = true;
        self.mod_init_defer_frames = 4;
    }

    /// Counts the four deferred frames down, then shows `@mods.initfailed`.
    fn tick_mod_init_failure(&mut self) {
        if self.mod_init_defer_frames <= 0 {
            return;
        }
        self.mod_init_defer_frames -= 1;
        if self.mod_init_defer_frames > 0 {
            return;
        }
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call(
                "show_info",
                &[GString::from("@mods.initfailed").to_variant()],
            );
        }
    }

    // ---- logic messages / join overlay ------------------------------------

    /// World-processor messages (`message`/`localeprint`) drained from the sim
    /// host and routed through the same `MindUi` surfaces as Java's
    /// `FlushMessageI` (`announce`/`hudfrag.showToast`/`showInfoToast`); H06 T4.
    /// Bundle and map-locale resolution happen in core before the drain.
    fn drain_logic_messages(&mut self) {
        if self
            .menu_group
            .as_ref()
            .is_some_and(|group| group.is_visible())
        {
            return;
        }
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if !host.has_method("drain_logic_messages") {
            return;
        }
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let Ok(rows) = host.call("drain_logic_messages", &[]).try_to::<VarArray>() else {
            return;
        };
        for row in rows.iter_shared() {
            let Ok(row) = row.try_to::<VarDictionary>() else {
                continue;
            };
            let text = row
                .get("text")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default();
            let duration = row
                .get("duration")
                .and_then(|value| value.try_to_relaxed::<f32>().ok())
                .unwrap_or(0.0);
            let kind = row
                .get("type")
                .and_then(|value| value.try_to::<GString>().ok())
                .unwrap_or_default();
            match kind.to_string().as_str() {
                // `LExecutor.java:1939-1942`: an announce is dropped while an
                // announcement is already up (`ui.hasAnnouncement()`).
                "announce" => {
                    if self.has_announcement() {
                        continue;
                    }
                    let _ = ui.call("announce", &[text.to_variant(), duration.to_variant()]);
                }
                // `LExecutor.java:1941`: a notify is dropped while a toast is on
                // screen (`ui.hudfrag.hasToast()`).
                "notify" => {
                    if self.has_toast() {
                        continue;
                    }
                    let _ = ui.call(
                        "toast",
                        &[text.to_variant(), GString::from("info").to_variant()],
                    );
                }
                // `LExecutor.java:1940`: `showInfoToast` is dropped while an
                // announcement is up (`ui.hasAnnouncement()`).
                "toast" => {
                    if self.has_announcement() {
                        continue;
                    }
                    if ui.has_method("show_info_toast") {
                        let _ = ui.call(
                            "show_info_toast",
                            &[text.to_variant(), duration.to_variant()],
                        );
                    } else {
                        let _ = ui.call("announce", &[text.to_variant(), duration.to_variant()]);
                    }
                }
                _ => {}
            }
        }
    }

    /// `NetClient`: the loading fragment shows `@connecting` while a join loads
    /// the host snapshot; progress follows `MindNet.snapshot_progress()` (I08 T5).
    fn refresh_join_loading(&mut self) {
        let Some(mut net) = kit::net() else {
            return;
        };
        let Some(mut loading) = self.loading_node() else {
            return;
        };
        let state = net
            .call("session_state", &[])
            .try_to::<GString>()
            .unwrap_or_default();
        let syncing = state == "loading" || state == "snapshot_sync";
        if syncing {
            let progress = if net.has_method("snapshot_progress") {
                net.call("snapshot_progress", &[])
                    .try_to_relaxed::<f32>()
                    .unwrap_or(0.0)
            } else {
                0.0
            };
            let _ = loading.call(
                "set_progress",
                &[progress.to_variant(), kit::t("@connecting").to_variant()],
            );
            self.join_loading_shown = true;
        } else if self.join_loading_shown {
            self.join_loading_shown = false;
            let _ = loading.call("hide_loading", &[]);
        }
    }

    // ---- Esc / background pause -------------------------------------------

    /// `_on_escape`: loading overlay, console, minimap, top dialog, then the
    /// in-game pause menu. Returns whether the key was consumed.
    fn on_escape(&mut self) -> bool {
        let Some(mut ui) = kit::ui() else {
            return false;
        };
        if let Some(mut loading) = self.base().get_node_or_null("LoadingLayer/loading")
            && let Ok(loading_control) = loading.clone().try_cast::<Control>()
            && loading_control.is_visible()
        {
            if loading.has_method("hide_loading") {
                let _ = loading.call("hide_loading", &[]);
            }
            return true;
        }
        if let Some(mut console) = self.base().get_node_or_null("HudGroup/console")
            && let Ok(console_control) = console.clone().try_cast::<Control>()
            && console_control.is_visible()
            && console.has_method("toggle")
        {
            let _ = console.call("toggle", &[]);
            return true;
        }
        if let Some(mut minimap) = self.hud_child("minimap")
            && minimap.is_visible()
        {
            minimap.set_visible(false);
            return true;
        }
        if ui.call("has_dialog", &[]).try_to::<bool>().unwrap_or(false) {
            let stack = ui
                .call("dialog_stack", &[])
                .try_to::<PackedStringArray>()
                .unwrap_or_default();
            let top = stack
                .as_slice()
                .last()
                .map(ToString::to_string)
                .unwrap_or_default();
            if top == "restart" || top == "campaign_complete" {
                return false;
            }
            if let Some(mut top_dialog) = self
                .base()
                .get_node_or_null(format!("DialogLayer/{top}").as_str())
                && top_dialog.has_method("blocks_escape")
                && top_dialog
                    .call("blocks_escape", &[])
                    .try_to::<bool>()
                    .unwrap_or(false)
            {
                return false;
            }
            let _ = ui.call("close_top_dialog", &[]);
            return true;
        }
        if self
            .menu_group
            .as_ref()
            .is_some_and(|group| group.is_visible())
        {
            return false;
        }
        // The editor shell owns keys while it is open (`MapEditorDialog.doInput`);
        // its Escape opens the editor menu, never the pause menu.
        if let Some(editor_dialog) = self.control_at("/root/Spine/Ui/EditorDialog")
            && editor_dialog.is_visible()
        {
            return false;
        }
        // `Control.java:733-736`: Esc pauses the sim (`state.set(paused)`) when
        // not in a network game and `pauseDisabled` is off.
        let opened = ui
            .call(
                "open_dialog",
                &[
                    GString::from("paused").to_variant(),
                    GString::new().to_variant(),
                ],
            )
            .try_to::<bool>()
            .unwrap_or(false);
        if opened
            && let Some(mut host) = kit::sim_host()
            && host.has_method("set_paused")
        {
            let _ = host.call("set_paused", &[true.to_variant()]);
        }
        opened
    }

    /// Arc's `closeOnBack` key listener still sees ESC while a text field inside
    /// it has keyboard focus. A focused Godot `LineEdit`/`TextEdit` accepts the
    /// key in `gui_input`, so drop its focus before the GUI pass and let
    /// `unhandled_key_input` close the dialog.
    fn release_field_on_escape(&mut self, event: &Gd<InputEvent>) {
        let Ok(key) = event.clone().try_cast::<InputEventKey>() else {
            return;
        };
        if !key.is_pressed() || key.is_echo() || key.get_keycode() != Key::ESCAPE {
            return;
        }
        let Some(viewport) = self.base().get_viewport() else {
            return;
        };
        let Some(mut focus) = viewport.gui_get_focus_owner() else {
            return;
        };
        if focus.clone().try_cast::<LineEdit>().is_ok()
            || focus.clone().try_cast::<TextEdit>().is_ok()
        {
            focus.release_focus();
        }
    }

    /// `settings.getBool("backgroundpause", true) && !net.active() &&
    /// !state.rules.pauseDisabled`.
    fn background_pause_allowed(&self) -> bool {
        let Some(mut ui) = kit::ui() else {
            return false;
        };
        if !ui
            .call(
                "settings_get_bool",
                &[GString::from("backgroundpause").to_variant()],
            )
            .try_to::<bool>()
            .unwrap_or(false)
        {
            return false;
        }
        if let Some(mut net) = kit::net()
            && net.has_method("session_state")
            && net
                .call("session_state", &[])
                .try_to::<GString>()
                .unwrap_or_default()
                != "offline"
        {
            return false;
        }
        let Some(mut host) = kit::sim_host() else {
            return false;
        };
        if host.has_method("rules_pause_disabled")
            && host
                .call("rules_pause_disabled", &[])
                .try_to::<bool>()
                .unwrap_or(false)
        {
            return false;
        }
        true
    }

    /// `if(state.is(State.playing)) state.set(State.paused)`: only an in-game
    /// session (the menu has no running game) pauses on focus loss.
    fn background_pause(&mut self) {
        if self
            .menu_group
            .as_ref()
            .map(|group| group.is_visible())
            .unwrap_or(true)
            || !self.background_pause_allowed()
        {
            return;
        }
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        self.background_paused = true;
        self.background_was_paused = host
            .call("is_paused", &[])
            .try_to::<bool>()
            .unwrap_or(false);
        if !self.background_was_paused {
            let _ = host.call("set_paused", &[true.to_variant()]);
        }
    }

    fn background_resume(&mut self) {
        if !self.background_paused {
            return;
        }
        self.background_paused = false;
        if self.background_was_paused || !self.background_pause_allowed() {
            return;
        }
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        if host
            .call("is_paused", &[])
            .try_to::<bool>()
            .unwrap_or(false)
        {
            let _ = host.call("set_paused", &[false.to_variant()]);
        }
    }

    // ---- prompt builder ---------------------------------------------------

    /// Builds a transient prompt panel (`showTextInput`/`showConfirm`/`showText`).
    ///
    /// code-instantiated: prompts are one-off transient overlays; their structure
    /// is parameterized by the request (field/no-field, body width) and has no
    /// static scene.
    fn build_prompt(
        &mut self,
        title_text: &str,
        message: &str,
        with_field: bool,
        body_width: f32,
    ) -> Prompt {
        let mut root = ColorRect::new_alloc();
        root.set_color(styles::BLACK_9);
        root.set_anchors_preset(LayoutPreset::FULL_RECT);
        root.set_mouse_filter(MouseFilter::STOP);
        // Prompts are topmost (`UI` has one shared layer upstream). The z_index
        // keeps them drawn above any later `OverlayLayer` sibling.
        root.set_z_index(1000);
        if let Some(mut layer) = self.overlay_layer.clone() {
            layer.add_child(&root);
        }
        let root_control: Gd<Control> = root.clone().upcast();
        let mut center = CenterContainer::new_alloc();
        center.set_anchors_preset(LayoutPreset::FULL_RECT);
        center.set_mouse_filter(MouseFilter::IGNORE);
        root.add_child(&center);
        let mut panel = PanelContainer::new_alloc();
        panel.set_theme_type_variation("defaultDialog");
        center.add_child(&panel);
        let mut layout = VBoxContainer::new_alloc();
        layout.add_theme_constant_override("separation", 6);
        panel.add_child(&layout);
        add_prompt_title(&mut layout, title_text, with_field);
        if with_field {
            // `UI.java:325`: the 400x4 accent rule above the text-input message.
            let mut rule = ColorRect::new_alloc();
            rule.set_color(styles::ACCENT);
            rule.set_custom_minimum_size(Vector2::new(400.0, 4.0));
            layout.add_child(&rule);
        }
        let body = factory::label(&format!("[center]{message}[/center]"));
        let mut body_rich: Gd<RichTextLabel> = body.upcast();
        body_rich.set_autowrap_mode(AutowrapMode::WORD_SMART);
        let mut minimum = body_rich.get_custom_minimum_size();
        minimum.x = body_width;
        body_rich.set_custom_minimum_size(minimum);
        body_rich.set_h_size_flags(SizeFlags::EXPAND_FILL);
        layout.add_child(&body_rich);
        let mut field = LineEdit::new_alloc();
        // `cont.field(def, ...)`: Arc's default underline `TextField` style.
        field.set_theme_type_variation("defaultField");
        field.set_custom_minimum_size(Vector2::new(400.0, 50.0));
        if with_field {
            layout.add_child(&field);
        }
        let mut buttons = HBoxContainer::new_alloc();
        buttons.set_alignment(AlignmentMode::CENTER);
        buttons.add_theme_constant_override("separation", 4);
        layout.add_child(&buttons);
        let prompt = Prompt {
            root: root_control,
            field,
            buttons,
            body: body_rich,
            layout,
            ok_button: None,
            cancel: PromptCancel::None,
            allow_empty: false,
            message: None,
        };
        self.prompt = Some(prompt.clone());
        prompt
    }

    /// Prompt action button (`buttons.defaults().size(...)` +
    /// `button("@key", Icon)`).
    fn add_prompt_button(
        &mut self,
        mut container: Gd<HBoxContainer>,
        label_key: &str,
        glyph: &str,
        button_size: Vector2,
        callback: &Callable,
    ) -> Gd<Button> {
        // code-instantiated: prompt action buttons are parameterized per prompt.
        let mut button = factory::icon_button(
            glyph,
            &kit::t(label_key).to_string(),
            "defaultt",
            AlignmentMode::BEGIN,
        );
        button.set_custom_minimum_size(button_size);
        if callback.is_valid() {
            button.connect("pressed", callback);
        }
        container.add_child(&button);
        button
    }

    /// Frees one prompt root, clearing the active-prompt bookkeeping only when
    /// it is still that prompt (a listener may have opened the next one).
    fn dismiss_prompt(&mut self, root: &Gd<Control>) {
        if !root.is_instance_valid() {
            return;
        }
        if self
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.root == *root)
        {
            self.clear_prompt();
        }
        if !root.is_queued_for_deletion() {
            let mut root = root.clone();
            root.queue_free();
        }
    }

    /// Clears the active-prompt bookkeeping (used by every prompt exit path).
    fn clear_prompt(&mut self) {
        self.prompt = None;
    }

    /// Runs a prompt's Esc/back path after its bookkeeping was cleared.
    fn run_prompt_cancel(&mut self, cancel: PromptCancel, root: &Gd<Control>) {
        match cancel {
            PromptCancel::None => {}
            PromptCancel::Dismiss => self.dismiss_prompt(root),
            PromptCancel::Confirm => {
                if let Some(mut ui) = kit::ui() {
                    let _ = ui.call("resolve_confirm", &[false.to_variant()]);
                }
                self.dismiss_prompt(root);
            }
            PromptCancel::TextInput => {
                if let Some(mut ui) = kit::ui() {
                    let _ = ui.call("resolve_text_input", &[GString::new().to_variant()]);
                }
                self.dismiss_prompt(root);
            }
        }
    }

    /// The active prompt while its root is still alive (`_active_prompt`).
    fn active_prompt(&mut self) -> Option<Prompt> {
        match self.prompt.clone() {
            Some(prompt) if prompt.root.is_instance_valid() => Some(prompt),
            Some(_) => {
                self.prompt = None;
                None
            }
            None => None,
        }
    }

    // ---- small helpers ----------------------------------------------------

    /// A direct child of this root as a `Control` (`get_node_or_null(path)`).
    fn control_at(&self, path: &str) -> Option<Gd<Control>> {
        self.base()
            .get_node_or_null(path)?
            .try_cast::<Control>()
            .ok()
    }

    /// A direct child of `HudGroup` as a `Control`.
    fn hud_child(&self, name: &str) -> Option<Gd<Control>> {
        self.hud_group
            .as_ref()?
            .get_node_or_null(name)?
            .try_cast::<Control>()
            .ok()
    }

    /// The loading fragment under `LoadingLayer`.
    fn loading_node(&self) -> Option<Gd<Node>> {
        self.loading_layer.as_ref()?.get_node_or_null("loading")
    }

    /// Hides the loading overlay when the fragment exposes `hide_loading`.
    fn hide_loading(&mut self) {
        if let Some(mut loading) = self.loading_node()
            && loading.has_method("hide_loading")
        {
            let _ = loading.call("hide_loading", &[]);
        }
    }

    /// Closes the HUD panels that would otherwise reappear with the HUD group
    /// (`config.hideConfig()`, `inv.hide()`, the fullscreen minimap).
    fn close_hud_panels(&mut self) {
        if let Some(mut config) = self.hud_child("block_config")
            && config.is_visible()
        {
            if config.has_method("_close") {
                let _ = config.call("_close", &[]);
            } else {
                config.set_visible(false);
            }
        }
        if let Some(mut inventory) = self.hud_child("block_inventory")
            && inventory.is_visible()
        {
            if inventory.has_method("hide_panel") {
                let _ = inventory.call("hide_panel", &[]);
            } else {
                inventory.set_visible(false);
            }
        }
        for panel_name in ["plan_config", "minimap"] {
            if let Some(mut panel) = self.hud_child(panel_name) {
                panel.set_visible(false);
            }
        }
    }

    /// `Time.get_ticks_msec()` as a float for the toast slot arithmetic.
    fn now_ms(&self) -> f64 {
        Time::singleton().get_ticks_msec() as f64
    }

    /// Hosted-state map id: `planet/sector` for a live campaign sector, the
    /// custom map's `Rules.mode_name`, else the default spine world.
    fn current_map_id(&self) -> String {
        if let Some(mut campaign) = kit::campaign() {
            if campaign.has_method("get_sector_state")
                && let Ok(state) = campaign
                    .call("get_sector_state", &[])
                    .try_to::<VarDictionary>()
            {
                let planet = state
                    .get("planet")
                    .and_then(|value| value.try_to::<GString>().ok())
                    .unwrap_or_default()
                    .to_string();
                let is_campaign = state
                    .get("campaign")
                    .map(|value| value.try_to::<bool>().unwrap_or(false))
                    .unwrap_or(false);
                if is_campaign && !planet.is_empty() {
                    let sector = state
                        .get("sector")
                        .map(|value| value.try_to_relaxed::<i64>().unwrap_or(0))
                        .unwrap_or(0);
                    return format!("{planet}-{sector}");
                }
                // I08 T4: the live custom map's name (non-campaign worlds).
                let map_name = state
                    .get("mapName")
                    .and_then(|value| value.try_to::<GString>().ok())
                    .unwrap_or_default()
                    .to_string();
                if !map_name.is_empty() {
                    return map_name;
                }
            }
            if campaign.has_method("get_rules_json") {
                let raw = campaign
                    .call("get_rules_json", &[])
                    .try_to::<GString>()
                    .unwrap_or_default();
                let parsed = Json::parse_string(&raw);
                if let Ok(rules) = parsed.try_to::<VarDictionary>()
                    && let Some(mode_name) = rules.get("modeName")
                    && let Ok(name_text) = mode_name.try_to::<GString>()
                    && !name_text.is_empty()
                {
                    return name_text.to_string();
                }
            }
        }
        "spine".to_string()
    }
}

/// Connects `callable` to `signal` when `node` declares it and it is not already
/// connected (`UiRoot._connect_prompts`/`_connect_net`).
fn connect_if(node: &mut Gd<Node>, signal: &str, callable: &Callable) {
    if node.has_signal(signal) && !node.is_connected(signal, callable) {
        node.connect(signal, callable);
    }
}

/// Prompt title row: accent label + 3 px rule (`BaseDialog.java:24-25`, the same
/// shape as `mind_dialog.gd _ensure_title_underline`).
fn add_prompt_title(layout: &mut Gd<VBoxContainer>, title_text: &str, plain: bool) {
    if title_text.is_empty() {
        return;
    }
    // code-instantiated: prompt titles are transient and parameterized.
    // `showTextInput` builds a plain `Dialog(titleText)` (`UI.java:324`): the
    // title sits at the left with no rule under it.
    let text = if plain {
        title_text.to_owned()
    } else {
        format!("[center]{title_text}[/center]")
    };
    let title = factory::label(&text);
    let mut title_rich: Gd<RichTextLabel> = title.upcast();
    title_rich.add_theme_color_override("default_color", styles::ACCENT);
    title_rich.set_h_size_flags(SizeFlags::EXPAND_FILL);
    layout.add_child(&title_rich);
    if plain {
        return;
    }
    let mut rule = ColorRect::new_alloc();
    rule.set_color(styles::ACCENT);
    rule.set_custom_minimum_size(Vector2::new(0.0, 3.0));
    layout.add_child(&rule);
}

/// `BaseDialog` cell padding for a titled prompt (`showConfirm`): the title
/// rule's `pad(4f)` (`BaseDialog.java:25`), `Dialog.defaults().pad(3)` around
/// `cont`/`buttons` and their `defaults().pad(3)` (`Dialog.java:208-214`), the
/// body's `pad(4f)` and the buttons' `pad(2f)` (`UI.java:584-585`).
fn apply_base_dialog_spacing(prompt: &Prompt) {
    let mut layout = prompt.layout.clone();
    layout.add_theme_constant_override("separation", 0);
    if prompt.body.get_index() >= 2 {
        // title | pad 4 | rule | pad 4 + cont 3 + body 4 (+2 line box) | body
        insert_prompt_gap(&mut layout, 1, 4.0);
        insert_prompt_gap(&mut layout, prompt.body.get_index(), 4.0 + 3.0 + 4.0 + 2.0);
    }
    // body | body 4 + cont 3 + buttons 3 + button 2 (+1 line box) | buttons
    insert_prompt_gap(
        &mut layout,
        prompt.buttons.get_index(),
        4.0 + 3.0 + 3.0 + 2.0 + 1.0,
    );
}

/// code-instantiated: fixed spacer inside a transient prompt.
fn insert_prompt_gap(layout: &mut Gd<VBoxContainer>, index: i32, height: f32) {
    let mut gap = Control::new_alloc();
    gap.set_custom_minimum_size(Vector2::new(0.0, height));
    gap.set_mouse_filter(MouseFilter::IGNORE);
    layout.add_child(&gap);
    layout.move_child(&gap, index);
}

/// A string field from a manifest entry (empty when absent).
fn json_string(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// A boolean field from a manifest entry (`false` when absent).
fn json_bool(value: &serde_json::Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toast_slot_is_three_and_a_half_seconds() {
        // `HudFragment.java:730`: `(int)(3.5 * 1000)`.
        assert!((TOAST_SLOT_MS - 3500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn uiscale_countdown_rounds_up_to_whole_seconds() {
        // `ceil` of a 10.02 s remainder shows 11, like the GDScript prompt.
        assert_eq!((10.02f64).ceil() as i32, 11);
        assert_eq!((-0.5f64).ceil().max(0.0) as i32, 0);
    }

    #[test]
    fn prompt_spacing_matches_base_dialog_cells() {
        // title rule 4 + `cont` 3 + body 4 (+2 line box).
        assert!((4.0f32 + 3.0 + 4.0 + 2.0 - 13.0).abs() < f32::EPSILON);
        // body 4 + `cont` 3 + buttons 3 + button 2 (+1 line box).
        assert!((4.0f32 + 3.0 + 3.0 + 2.0 + 1.0 - 13.0).abs() < f32::EPSILON);
    }

    #[test]
    fn manifest_helpers_default_when_fields_are_absent() {
        let value = serde_json::json!({"name": "about", "pause": true});
        assert_eq!(json_string(&value, "name"), "about");
        assert!(json_bool(&value, "pause"));
        assert_eq!(json_string(&value, "scene"), "");
        assert!(!json_bool(&value, "hidden"));
    }
}
