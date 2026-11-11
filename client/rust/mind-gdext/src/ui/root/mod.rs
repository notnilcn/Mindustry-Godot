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

mod accessors;
mod boot;
mod escape;
mod free;
mod messages;
mod net_intents;
mod prompts;
mod ui_scale;

#[cfg(test)]
mod tests;

use std::cell::RefCell;
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
    StyleBoxFlat, TextEdit, Time, VBoxContainer,
};
use godot::global::{HorizontalAlignment, Key};
use godot::obj::{InstanceId, NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use super::kit::widgets::factory;
use super::kit::{self, MindRichLabel, icons, styles};
use crate::campaign::MindCampaign;
use crate::gdutil::variant::{
    dict_bool_try_or, dict_f64_relaxed_or, dict_i64_relaxed_or, dict_text_try_or,
};
use crate::platform::MindPlatform;
use crate::ui::editor_views::MindMapEditorDialog;
use crate::ui::menu_background::MindMenuBackground;
use crate::wiring;

/// The dialog/fragment inventory this root binds (`_DIALOGS_MANIFEST`).
const DIALOGS_MANIFEST: &str = "res://ui/dialogs_manifest.json";

/// `Control.java:640` UI-scale confirmation countdown (`60 * 11` frames).
const UISCALE_REVERT_SECONDS: i32 = 11;

/// `HudFragment.java:730` `(int)(3.5 * 1000)` toast slot width.
const TOAST_SLOT_MS: f64 = 3500.0;

/// `FadeInFragment.start` default duration (the former GDScript default).
const BOOT_FADE_SECONDS: f64 = 0.5;

/// `Control.java:177` `Time.run(60f * 2f, ...)`: 120 ticks at 60 Hz before the
/// campaign-complete overlay opens.
const CAMPAIGN_COMPLETE_DELAY_SECONDS: f64 = 2.0;

thread_local! {
    /// Buttons that entered the tree while a `MindUiRoot` method was already
    /// bound, waiting for the next `process` drain (`UI.java:146` click sound).
    static PENDING_NODE_HOOKS: RefCell<Vec<InstanceId>> = const { RefCell::new(Vec::new()) };
}

/// Queues one node that entered the tree for the global click-sound hook.
///
/// `Tree.node_added` fires synchronously from `Node::add_child`, and the UI
/// itself adds nodes from *bound* `MindUiRoot` methods (`announce_box`,
/// `toast_label`, the prompt builders). A `#[func]` signal receiver would take
/// a second mutable borrow of the root and panic with
/// `Gd<T>::bind_mut() failed, already bound` — the landing cutscene's
/// `announce` did exactly that (PAR-0019). The listener therefore records only
/// the instance id (no Godot borrow) and `process` hooks it.
fn queue_node_hook(node: &Gd<Node>) {
    push_node_hook(node.instance_id());
}

/// The borrow-free half of [`queue_node_hook`] (unit-testable without an engine).
fn push_node_hook(id: InstanceId) {
    PENDING_NODE_HOOKS.with(|queue| queue.borrow_mut().push(id));
}

/// Takes the nodes queued since the last drain, in arrival order.
fn take_node_hooks() -> Vec<InstanceId> {
    PENDING_NODE_HOOKS.with(|queue| std::mem::take(&mut *queue.borrow_mut()))
}

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
#[class(base=Control)]
pub struct MindUiRoot {
    base: Base<Control>,

    /// `Ui/MenuBackground` — optional; the standalone `ui_root.tscn` has none.
    #[export]
    pub menu_background: Option<Gd<MindMenuBackground>>,
    /// `Ui/EditorDialog` — optional; the full-screen map-editor shell.
    #[export]
    pub editor_dialog: Option<Gd<MindMapEditorDialog>>,
    /// `Spine/MindPlatform` — optional; `finish_launch` on the first menu show.
    #[export]
    pub platform: Option<Gd<MindPlatform>>,
    /// `Spine/MindCampaign` — optional; campaign reset/playtest/map state.
    #[export]
    pub campaign: Option<Gd<MindCampaign>>,

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
    /// The installed `Tree.node_added` listener (the borrow-free queue hook).
    node_added_hook: Option<Callable>,
}

#[godot_api]
impl IControl for MindUiRoot {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            menu_background: None,
            editor_dialog: None,
            platform: None,
            campaign: None,
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
            node_added_hook: None,
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
        // Scene-wired optional spine deps: `game.tscn` sets them on the
        // instance root; a standalone `ui_root.tscn` has none and stays silent.
        // No `on_notification` re-adopt here (re-entrant `bind_mut` hazard).
        self.menu_background = wiring::optional(&self.menu_background);
        self.editor_dialog = wiring::optional(&self.editor_dialog);
        self.platform = wiring::optional(&self.platform);
        self.campaign = wiring::optional(&self.campaign);
        self.menu_group = self.control_at("MenuGroup");
        self.hud_group = self.control_at("HudGroup");
        self.dialog_layer = self.control_at("DialogLayer");
        self.overlay_layer = self.control_at("OverlayLayer");
        self.loading_layer = self.control_at("LoadingLayer");
        self.bind_manifest();
        self.connect_prompts();
        self.connect_net();
        self.connect_campaign();
        self.apply_ui_scale();
        self.boot();
        self.check_uiscale_changed();
        self.check_mod_init_failure();
        self.install_global_hooks();
    }

    fn process(&mut self, delta: f64) {
        self.drain_logic_messages();
        self.drain_save_failures();
        self.drain_event_toasts();
        self.refresh_join_loading();
        self.tick_mod_init_failure();
        self.tick_uiscale(delta);
        // Nodes queued by `Tree.node_added` — including the ones the calls
        // above added while this object was bound — are hooked here, outside
        // the borrow (`UI.java:146` click sound).
        self.drain_node_hooks();
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
        if let Some(mut campaign) = self.campaign.clone() {
            campaign.bind_mut().reset_to_menu();
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
        if let Some(mut background) = self.menu_background.clone() {
            background.set_visible(menu_visible && !editor_open);
        }
        self.hide_loading();
        if menu_visible && !self.launch_finished {
            self.launch_finished = true;
            // `Vars.finishLaunch` (ClientLauncher.java:253): the main menu is
            // up, so the launch succeeded — drop the crash marker and record
            // lastBuild.
            if let Some(mut platform) = self.platform.clone() {
                let _ = platform.bind_mut().finish_launch();
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
        if let Some(mut background) = self.menu_background.clone() {
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
        let Some(mut campaign) = self.campaign.clone() else {
            return false;
        };
        // Guard is a temporary: it never spans the UI calls below.
        let result = campaign.bind_mut().check_playtest();
        let playtest = dict_bool_try_or(&result, "playtest", false);
        if !playtest {
            return false;
        }
        let resumed = dict_bool_try_or(&result, "resumed", false);
        let mut shown = false;
        if resumed && let Some(mut shell) = self.editor_dialog.clone() {
            // `beginEditMap` -> `show()`; the shell hides the standalone menu.
            shell.bind_mut().show_dialog();
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
}
