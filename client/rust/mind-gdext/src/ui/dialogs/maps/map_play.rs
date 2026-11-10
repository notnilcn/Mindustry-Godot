// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.

//! The map play dialog controller. Filled by S03 (docs/PLANS).
//! Upstream: ui/dialogs/MapPlayDialog.java

use crate::ui::dialog::{DialogController, MindDialog};
use crate::ui::dialogs::campaign::util::*;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::{self, styles};
use godot::builtin::{Side, VarDictionary};
use godot::classes::control::MouseFilter;
use godot::classes::scroll_container::ScrollMode;
use godot::classes::{Button, Control, PanelContainer, StyleBoxEmpty};
use godot::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The dialog controllers in this batch.
pub(super) fn dispatch(id: &str) -> Option<Box<dyn DialogController>> {
    match id {
        "map_play" => Some(Box::new(MapPlayDialog::default())),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// MapPlayDialog
// ---------------------------------------------------------------------------

/// Visible `Gamemode` names in declaration order (`Gamemode.all` minus editor).
const MAP_PLAY_MODES: [&str; 4] = ["survival", "sandbox", "attack", "pvp"];
/// `MapPlayDialog` mode-button size (`size(140f, mobile ? 44f : 54f)`).
const MODE_BUTTON: Vector2 = Vector2::new(140.0, 54.0);
/// `size(mobile && !portrait ? 150f : 250f)`.
const PREVIEW_SIZE: Vector2 = Vector2::new(250.0, 250.0);
/// `buttons.button(...).size(210f, 64f)` (`addCloseButton` + `@play`).
const MAP_PLAY_ACTION_BUTTON: Vector2 = Vector2::new(210.0, 64.0);

/// Controller state for the reused gamemode-selection dialog node.
#[derive(Default)]
struct MapPlayState {
    map_name: String,
    mode: String,
    /// Working rules JSON (`MapPlayDialog.rules`); empty = the map's own rules.
    rules_json: String,
    /// `MapPlayDialog.show(map, playtesting)`: the editor playtest route.
    playtest: bool,
    mode_buttons: Vec<Gd<Button>>,
    button_modes: Vec<String>,
}

impl MapPlayState {
    fn new() -> Self {
        Self {
            mode: String::from("survival"),
            ..Self::default()
        }
    }
}

/// Gamemode selection, the mode help list, Customize Rules, the map preview and
/// the high score (`MapPlayDialog`).
pub struct MapPlayDialog {
    state: Rc<RefCell<MapPlayState>>,
}

impl Default for MapPlayDialog {
    fn default() -> Self {
        Self {
            state: Rc::new(RefCell::new(MapPlayState::new())),
        }
    }
}

impl DialogController for MapPlayDialog {
    fn ready(&mut self, dialog: &mut MindDialog) {
        dialog.should_pause = false;
        map_play_floating_layout(dialog);
    }

    fn context_changed(&mut self, dialog: &mut MindDialog) {
        let map_name = dialog.context_string("map").to_string();
        let requested = dialog.context_string("mode").to_string();
        let rules = dialog.context_string("rules").to_string();
        let playtest = dialog
            .context_value("playtest")
            .map(|value| value.to::<bool>())
            .unwrap_or(false);
        let mut st = self.state.borrow_mut();
        st.map_name = map_name;
        if MAP_PLAY_MODES.contains(&requested.as_str()) {
            st.mode = requested;
        }
        st.rules_json = rules;
        st.playtest = playtest;
    }

    fn shown(&mut self, dialog: &mut MindDialog) {
        let mut st = self.state.borrow_mut();
        if st.map_name.is_empty() {
            st.map_name = dialog.context_string("map").to_string();
        }
        drop(st);
        map_play_build(dialog, &self.state);
    }
}

/// `super(""); setFillParent(false)` with `Styles.defaultDialog`
/// (`background = windowEmpty`, `stageBackground = black9`): the content floats
/// centred over the dim with no boxed panel and no scrolling body.
fn map_play_floating_layout(dialog: &mut MindDialog) {
    if let Some(mut panel) = dialog.panel() {
        panel.set_theme_type_variation("none");
        panel.set_custom_minimum_size(Vector2::ZERO);
        // The floating table still carries `windowEmpty`'s 4 px inset above the
        // (unpadded) title row, and under the buttons the button's own `pad(3)`
        // plus the `buttons` cell's `pad(3)` (`Dialog.java:208-214`).
        let mut inset = StyleBoxEmpty::new_gd();
        inset.set_content_margin(Side::TOP, 4.0);
        inset.set_content_margin(Side::BOTTOM, 4.0 + 3.0 + 3.0);
        panel.add_theme_stylebox_override("panel", &inset);
    }
    if let Some(mut body) = dialog.body() {
        // The body reports its content height so nothing is clipped/scrolled.
        body.set_vertical_scroll_mode(ScrollMode::DISABLED);
        body.set_horizontal_scroll_mode(ScrollMode::DISABLED);
    }
    if let Some(mut buttons) = dialog.buttons_row() {
        buttons.add_theme_constant_override("separation", 8);
    }
    dialog.title_color = styles::ACCENT;
}

fn map_play_build(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>) {
    let (map_name, mut mode, mut rules_json) = {
        let st = state.borrow();
        (st.map_name.clone(), st.mode.clone(), st.rules_json.clone())
    };
    let title = if map_name.is_empty() {
        kit::t("@map.play").to_string()
    } else {
        map_name.clone()
    };
    dialog.set_title_literal(&title);
    // `cont.clearChildren()` (`MapPlayDialog.java:45`): the body is rebuilt per
    // shown map.
    dialog.clear_content();
    let modes = map_play_mode_validity(dialog, &map_name);
    // `if(!selectedGamemode.valid(map)) selectedGamemode = first valid || survival`.
    if !row_bool_or(&modes, &mode, true) {
        let mut picked = String::new();
        for candidate in MAP_PLAY_MODES {
            if row_bool_or(&modes, candidate, false) {
                picked = candidate.to_string();
                break;
            }
        }
        mode = if picked.is_empty() {
            String::from("survival")
        } else {
            picked
        };
    }
    if rules_json.is_empty() {
        rules_json = map_play_mode_rules(dialog, &map_name, &mode);
    }
    {
        let mut st = state.borrow_mut();
        st.mode = mode.clone();
        st.rules_json = rules_json;
    }
    let mut root = dialog.content_table();
    // `Table selmode`: `add("@level.mode").colspan(2)`, then the `Tex.button`
    // mode table and the `?` button on one row (`MapPlayDialog.java:56-71`).
    let mut selmode = factory::table();
    let mode_label = factory::label(&kit::t("@level.mode").to_string());
    selmode.bind_mut().add(&mode_label).colspan(2);
    selmode.bind_mut().row();
    // code-instantiated: `selmode.table(Tex.button, modes -> ...)` frame.
    let mut mode_frame = PanelContainer::new_alloc();
    let mut frame_box = MindDialog::pane_box(Color::BLACK, OUTLINE_GRAY, 4.0, 2.0, -1.0);
    // `Tex.button` pads its content 12 px (the 9-patch splits): the Java mode
    // box is 304x132 around the 2x2 140x54 buttons (java/mapplay-caldera.png).
    for side in [Side::LEFT, Side::RIGHT, Side::TOP, Side::BOTTOM] {
        frame_box.set_content_margin(side, 12.0);
    }
    mode_frame.add_theme_stylebox_override("panel", &frame_box);
    let mut mode_box = factory::table();
    mode_frame.add_child(&mode_box);
    state.borrow_mut().mode_buttons.clear();
    state.borrow_mut().button_modes.clear();
    for (index, candidate) in MAP_PLAY_MODES.iter().enumerate() {
        // code-instantiated: one toggle per visible Gamemode (`Gamemode.all`).
        let mut button = factory::button(&kit::t(&format!("@mode.{candidate}.name")).to_string());
        button.set_theme_type_variation("flatToggleMenut");
        apply_flat_toggle_menu(&mut button);
        button.set_toggle_mode(true);
        button.set_pressed(*candidate == mode);
        button.set_disabled(!row_bool_or(&modes, candidate, true));
        button.set_custom_minimum_size(MODE_BUTTON);
        let target = dialog.to_gd();
        let state_for_mode = state.clone();
        let mode_name = candidate.to_string();
        let callable = Callable::from_fn("map_play_select_mode", move |_args: &[&Variant]| {
            let mut node = target.clone();
            let mut bound = node.bind_mut();
            map_play_select_mode(&mut bound, &state_for_mode, &mode_name);
        });
        button.connect("pressed", &callable);
        mode_box
            .bind_mut()
            .add(&button)
            .width(MODE_BUTTON.x)
            .height(MODE_BUTTON.y);
        if index % 2 == 1 {
            mode_box.bind_mut().row();
        }
        state.borrow_mut().mode_buttons.push(button);
        state.borrow_mut().button_modes.push(candidate.to_string());
    }
    selmode.bind_mut().add(&mode_frame);
    // `selmode.button("?", this::displayGameModeHelp).width(50f).fillY().padLeft(18f)`.
    let mut help = factory::button("?");
    dialog.apply_pane_button(&mut help, OUTLINE_GRAY, 4.0);
    help.set_custom_minimum_size(Vector2::new(50.0, 0.0));
    let target = dialog.to_gd();
    let callable = Callable::from_fn("map_play_help", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        map_play_show_mode_help(&mut bound);
    });
    help.connect("pressed", &callable);
    selmode
        .bind_mut()
        .add(&help)
        .width(50.0)
        .fill_y()
        .pad_left(18.0);
    root.bind_mut().add(&selmode).expand_x();
    root.bind_mut().row();
    // `cont.button("@customize", Icon.settings, ...).height(50f).width(230)`.
    let mut customize = factory::button("");
    dialog.apply_pane_button(&mut customize, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control("settings", 24.0, Color::WHITE).upcast();
    let customize_text = kit::t("@customize").to_string();
    dialog.build_button_content(&mut customize, Some(glyph), &customize_text);
    customize.set_custom_minimum_size(Vector2::new(230.0, 50.0));
    let target = dialog.to_gd();
    let state_for_rules = state.clone();
    let callable = Callable::from_fn("map_play_open_rules", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        map_play_open_rules(&mut bound, &state_for_rules);
    });
    customize.connect("pressed", &callable);
    root.bind_mut().add(&customize).width(230.0).height(50.0);
    root.bind_mut().row();
    // `cont.add(new BorderImage(map.safeTexture(), 3f)).size(250f)
    // .get().setScaling(Scaling.fit)`: the whole preview, fitted.
    if let Some(texture) = preview_texture(&map_name) {
        // code-instantiated: the preview texture comes from the map registry.
        let mut image = border_preview(&texture, PREVIEW_SIZE, 3.0);
        image.set_mouse_filter(MouseFilter::IGNORE);
        root.bind_mut()
            .add(&image)
            .width(PREVIEW_SIZE.x)
            .height(PREVIEW_SIZE.y);
    }
    // `if(Gamemode.survival.valid(map)) cont.label(level.highscore).pad(3f)`.
    if row_bool_or(&modes, "survival", false) {
        root.bind_mut().row();
        let label = factory::label(&map_play_high_score(&map_name));
        root.bind_mut().add(&label).pad(3.0);
    }
    dialog.clear_buttons();
    // `addCloseButton()` (210x64) then `buttons.button("@play", Icon.play, ...)
    // .size(210f, 64f)`.
    let close = dialog.base().callable("close_pressed");
    let back_text = kit::t("@back").to_string();
    map_play_action_button(dialog, &back_text, "left", close);
    let target = dialog.to_gd();
    let state_for_play = state.clone();
    let callable = Callable::from_fn("map_play_start", move |_args: &[&Variant]| {
        let mut node = target.clone();
        let mut bound = node.bind_mut();
        map_play_start(&mut bound, &state_for_play);
    });
    let play_text = kit::t("@play").to_string();
    map_play_action_button(dialog, &play_text, "play", callable);
}

/// A 210x64 `Tex.button` action button with an icon (`BaseDialog` buttons row).
fn map_play_action_button(
    dialog: &mut MindDialog,
    text_value: &str,
    icon_name: &str,
    listener: Callable,
) {
    // code-instantiated: the button row is rebuilt per shown map.
    let mut button = factory::button("");
    button.set_custom_minimum_size(MAP_PLAY_ACTION_BUTTON);
    dialog.apply_pane_button(&mut button, OUTLINE_GRAY, 4.0);
    let glyph: Gd<Control> = glyph_control(icon_name, 24.0, Color::WHITE).upcast();
    dialog.build_button_content(&mut button, Some(glyph), text_value);
    if listener.is_valid() {
        button.connect("pressed", &listener);
    }
    if let Some(mut row) = dialog.buttons_row() {
        row.add_child(&button);
    }
}

/// `Gamemode.valid(map)` per visible mode (`MindPreview.map_modes`).
fn map_play_mode_validity(dialog: &mut MindDialog, map_name: &str) -> VarDictionary {
    let _ = dialog;
    let Some(mut preview) = preview_node() else {
        return VarDictionary::new();
    };
    preview.bind_mut().map_modes(gs(map_name))
}

/// `map.applyRules(mode)` serialized (`MindPreview.map_rules_json`).
fn map_play_mode_rules(dialog: &mut MindDialog, map_name: &str, mode: &str) -> String {
    let _ = dialog;
    let Some(mut preview) = preview_node() else {
        return String::new();
    };
    preview
        .bind_mut()
        .map_rules_json(gs(map_name), gs(mode))
        .to_string()
}

/// `mode.toString()` + the selected index, refreshing the rules.
fn map_play_select_mode(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>, mode: &str) {
    let (buttons, modes, map_name) = {
        let st = state.borrow();
        (
            st.mode_buttons.clone(),
            st.button_modes.clone(),
            st.map_name.clone(),
        )
    };
    for (index, button) in buttons.iter().enumerate() {
        let selected = modes
            .get(index)
            .map(|candidate| candidate == mode)
            .unwrap_or(false);
        set_toggle(button, selected);
    }
    // `rules = map.applyRules(mode)` upstream.
    let rules = map_play_mode_rules(dialog, &map_name, mode);
    let mut st = state.borrow_mut();
    st.mode = mode.to_string();
    st.rules_json = rules;
}

/// `displayGameModeHelp` (`MapPlayDialog.java:91-112`): every visible mode as
/// `[accent]<name>:[] [lightgray]<description>`, width 400, `@ok` 110x50.
fn map_play_show_mode_help(dialog: &mut MindDialog) {
    let _ = dialog;
    let mut lines: Vec<String> = Vec::new();
    for mode in MAP_PLAY_MODES {
        lines.push(format!(
            "[accent]{}:[] [lightgray]{}",
            kit::t(&format!("@mode.{mode}.name")),
            kit::t(&format!("@mode.{mode}.description"))
        ));
    }
    let body = lines.join("\n");
    let _ = ui_call(
        "show_text",
        &[
            kit::t("@mode.help.title").to_variant(),
            kit::markup_text(&body).to_variant(),
        ],
    );
}

/// `CustomRulesDialog(true).show(rules, () -> rules = map.applyRules(mode))`:
/// the dialog edits the working rules; `rules_edited` replaces them.
fn map_play_open_rules(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>) {
    let (rules_json, mode, map_name) = {
        let st = state.borrow();
        (st.rules_json.clone(), st.mode.clone(), st.map_name.clone())
    };
    let current = parse_object(&rules_json);
    let reset = parse_object(&map_play_mode_rules(dialog, &map_name, &mode));
    let mut context = VarDictionary::new();
    context.set("rules", &current.to_variant());
    context.set("reset", &reset.to_variant());
    context.set("show_rule_edit", &false.to_variant());
    open_dialog("custom_rules", &context);
    let Some(rules_dialog) = dialog_node("custom_rules") else {
        return;
    };
    if !rules_dialog.has_signal("rules_edited") {
        return;
    }
    let state_for_rules = state.clone();
    let callable = Callable::from_fn("map_play_rules_edited", move |args: &[&Variant]| {
        let json = args
            .first()
            .map(|value| (*value).to::<GString>().to_string())
            .unwrap_or_default();
        state_for_rules.borrow_mut().rules_json = json;
    });
    connect_once(
        dialog,
        "map_play_rules",
        &rules_dialog,
        "rules_edited",
        &callable,
    );
}

/// `Core.bundle.format("level.highscore", map.getHighScore())`.
fn map_play_high_score(map_name: &str) -> String {
    let default = kit::t("@level.highscore").to_string();
    let Some(preview) = preview_node() else {
        return default;
    };
    let key = preview.bind().map_high_score_key(gs(map_name));
    if key.is_empty() {
        return default;
    }
    let score = setting_get_int(&key.to_string(), 0);
    kit::markup_text(&default.replace("{0}", &score.to_string())).to_string()
}

/// `control.playMap(map, rules, playtesting)` + `ui.custom.hide()`.
fn map_play_start(dialog: &mut MindDialog, state: &Rc<RefCell<MapPlayState>>) {
    let (map_name, mode, playtest, rules_json) = {
        let st = state.borrow();
        (
            st.map_name.clone(),
            st.mode.clone(),
            st.playtest,
            st.rules_json.clone(),
        )
    };
    let mut started = false;
    if !map_name.is_empty()
        && let Some(mut campaign) = dialog.campaign_node()
        && campaign.has_method("start_map_mode")
    {
        // `start_map_mode` loads the world, applies the gamemode + custom rules
        // and starts the session in one call (`Control.playMap`).
        started = campaign
            .call(
                "start_map_mode",
                &[
                    gs(map_name).to_variant(),
                    gs(mode).to_variant(),
                    playtest.to_variant(),
                    gs(rules_json).to_variant(),
                ],
            )
            .try_to::<bool>()
            .unwrap_or(false);
    }
    map_play_close_stack(dialog);
    if !started && playtest {
        // `Control.playMap`: "booted out of map, resume editing"
        // (`Control.java:422-428`): the editor still holds the map the playtest
        // just saved, so re-show it over the error.
        if let Some(mut shell) = kit::node("/root/Spine/Ui/EditorDialog")
            && shell.has_method("show_dialog")
        {
            shell.call("show_dialog", &[]);
        }
        return;
    }
    if started {
        // `MapEditorDialog.playtest`'s `playListener = this::hide`: a playtest
        // returns to the editor, so hide the shell while the map plays.
        if playtest
            && let Some(mut editor) = kit::node("/root/Spine/Ui/EditorDialog")
            && editor.has_method("hide_dialog")
        {
            editor.call("hide_dialog", &[]);
        }
        if let Some(mut root) = ui_root()
            && root.has_method("set_menu_visible")
        {
            root.call("set_menu_visible", &[false.to_variant()]);
        }
    }
}

fn map_play_close_stack(dialog: &mut MindDialog) {
    if kit::ui().is_none() {
        dialog.hide_dialog();
        return;
    }
    close_dialog("map_play");
    close_dialog("custom");
}
