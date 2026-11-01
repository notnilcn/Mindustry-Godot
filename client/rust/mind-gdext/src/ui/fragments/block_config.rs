// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/BlockConfigFragment.java
//         (plan 14 §3.5, M4).
//
// Scale-in configuration panel anchored to a building's tile. The spec JSON is
// produced by `MindInput` (`DesktopBridge::config_spec`): the block title, the
// tile, and the selectable options (item/liquid content names) plus a `text`
// flag for code-configurable blocks. Submitting routes through
// `MindInput.configure_building` (a sim command; this class never mutates sim
// state).

//! `MindBlockConfigFragment` — the building configuration panel.
//!
//! 1:1 port of `client/ui/fragments/block_config_fragment.gd`: one route per
//! `config_spec` kind (link / colour / logic / canvas / text / launch /
//! target-dummy / core teams / item selection / option grid), the per-frame
//! `Building.updateTableAlign` anchor (`BuildingComp.java:1682-1685`) and the
//! 0.07 s scale-in (`BlockConfigFragment.showConfig`). Every value is read
//! through the `MindSimHost`/`MindUi` read models or handed to the `MindInput`
//! command facade; nothing here touches sim state.

use godot::classes::control::SizeFlags;
use godot::classes::line_edit::VirtualKeyboardType;
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{
    Button, CheckBox, Control, GridContainer, HBoxContainer, IControl, Json, Label, LineEdit,
    PanelContainer, ScrollContainer, StyleBoxEmpty, Tween, VBoxContainer,
};
use godot::obj::{NewAlloc, NewGd};
use godot::prelude::*;

use crate::ui::kit;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::widgets::item_command_row::MindItemCommandRow;
use crate::ui::kit::widgets::item_selection::MindItemSelection;

/// Scene size of the titled panel (`block_config_fragment.tscn`) used by the
/// option/text/link/colour specs.
const PANEL_WIDTH: f32 = 300.0;
const PANEL_HEIGHT: f32 = 220.0;

/// `BlockConfigFragment.showConfig` scale-in:
/// `Actions.scaleTo(1f, 1f, 0.07f, Interp.pow3Out)`.
const SCALE_IN_SECONDS: f64 = 0.07;

/// `Tile.SIZE` (`mind_tilesize()`).
const TILESIZE: f64 = 8.0;

/// Picker-node metadata: the tile whose config the next `color_selected`
/// applies to (`ColorPicker.show`'s consumer callable captured it per call).
const CONFIG_COLOR_TARGET_META: &str = "_mind_config_color_target";
/// Picker-node metadata: the one `color_selected` handler is installed.
const CONFIG_COLOR_BOUND_META: &str = "_mind_config_color_bound";

/// `MindBlockConfigFragment` — the building configuration panel.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindBlockConfigFragment {
    base: Base<Control>,

    /// The parsed `config_spec` JSON (`_config`).
    config: VarDictionary,
    /// Tapped tile of the configured building (`config.getSelected()`).
    tile: Vector2i,
    /// The running scale-in tween (`_scale_tween`).
    scale_tween: Option<Gd<Tween>>,
    /// Launch-pad destination planet (the spec's `planet`,
    /// `LaunchPad.java:192`).
    launch_planet: String,
}

#[godot_api]
impl IControl for MindBlockConfigFragment {
    fn ready(&mut self) {
        self.base_mut().set_visible(false);
        // The GDScript member initializer; `#[class(init)]` would zero it.
        self.tile = Vector2i::new(-1, -1);
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        if !self.base().is_visible() {
            return;
        }
        // `table.pack()`: the frameless table is exactly its contents.
        if self.frameless()
            && let Some(panel) = self.panel()
        {
            let size = panel.get_combined_minimum_size();
            self.base_mut().set_size(size);
            self.base_mut().set_pivot_offset(size / 2.0);
        }
        self.align_to_building();
    }
}

#[godot_api]
impl MindBlockConfigFragment {
    /// Opens the panel for a building (`ConfigUiSpec` JSON from `MindInput`).
    /// `world_pos` (the tap point) is only the fallback anchor: the table
    /// follows the building every frame like `Building.updateTableAlign`.
    #[func]
    pub fn configure(&mut self, world_pos: Vector2, spec_json: GString) {
        self.base_mut().set_position(world_pos);
        let parsed = Json::parse_string(&spec_json);
        self.config = parsed.try_to::<VarDictionary>().unwrap_or_default();
        self.tile = Vector2i::new(
            dict_i64(&self.config, "x", -1) as i32,
            dict_i64(&self.config, "y", -1) as i32,
        );
        let frameless = self.frameless();
        let title_text = self.localized(
            &dict_string(&self.config, "title", "").to_string(),
            &dict_string(&self.config, "block", "").to_string(),
        );
        if let Some(mut title) = self.title() {
            title.set_text(&title_text);
            title.set_visible(!frameless);
        }
        if let Some(mut panel) = self.panel() {
            // `BlockConfigFragment.showConfig`: `table.background(null)` —
            // blocks whose `buildConfiguration` only adds buttons show just the
            // buttons, and the `ItemSelection` grid carries its own
            // `Styles.black6` table.
            if frameless {
                panel.add_theme_stylebox_override("panel", &StyleBoxEmpty::new_gd());
            } else {
                panel.remove_theme_stylebox_override("panel");
            }
        }
        self.rebuild();
        let size = match self.panel() {
            Some(panel) if frameless => panel.get_combined_minimum_size(),
            _ => Vector2::new(PANEL_WIDTH, PANEL_HEIGHT),
        };
        self.base_mut().set_size(size);
        self.align_to_building();
        self.base_mut().set_visible(true);
        // `table.setOrigin(Align.center)` + `scaleTo(0f, 1f)` ->
        // `scaleTo(1f, 1f, 0.07f, Interp.pow3Out)`.
        self.base_mut().set_pivot_offset(size / 2.0);
        self.base_mut().set_scale(Vector2::new(0.0, 1.0));
        if let Some(tween) = self.scale_tween.clone()
            && tween.is_valid()
        {
            let mut tween = tween;
            tween.kill();
        }
        let this: Gd<Control> = self.to_gd().upcast();
        let mut tween = self.base_mut().create_tween();
        tween.set_trans(TransitionType::CUBIC);
        tween.set_ease(EaseType::OUT);
        tween.tween_property(&this, "scale", &Vector2::ONE.to_variant(), SCALE_IN_SECONDS);
        self.scale_tween = Some(tween);
    }

    /// `Tile.SIZE` (`mind_tilesize()`): the target-dummy armour fields convert
    /// between blocks and world units with it.
    #[func]
    pub fn mind_tilesize(&self) -> i64 {
        TILESIZE as i64
    }

    /// Hides the panel and clears the input's `config.getSelected()` mirror
    /// (`MindInput.close_block_config`; H02 T6). Exported under its GDScript
    /// name because `MindUiRoot.close_hud_panels` probes for `_close`.
    #[func(rename = _close)]
    pub fn close_panel(&mut self) {
        self.base_mut().set_visible(false);
        let Some(mut input) = kit::node("/root/Spine/Input") else {
            return;
        };
        if input.has_method("close_block_config") {
            let _ = input.call("close_block_config", &[]);
        }
    }

    /// J05 T1: `ItemSelection.buildTable` pick. Picking closes the panel
    /// (`ItemSelection.java:70-71`, `closeSelect`) and re-tapping the checked
    /// icon submits an empty value (`:73`, Java's `null` → `configure(null)`).
    #[func(rename = _on_selection_pick)]
    pub fn on_selection_pick(&mut self, value: GString) {
        let kind = dict_string(&self.config, "pick_kind", "content").to_string();
        if value.is_empty() {
            // `configure(null)`/`configClear`: the re-tap cleared the selection.
            self.submit("none", "");
        } else {
            self.submit(&kind, &value.to_string());
        }
        self.close_panel();
    }

    /// A command-row pick configures the command (`configure(item)`); the
    /// reconstructor also closes the panel (`deselect()`,
    /// `Reconstructor.java:230`) while the factory's row leaves it open
    /// (`UnitFactory.java:311-315`).
    #[func(rename = _on_command_pick)]
    pub fn on_command_pick(&mut self, content_name: GString, row: Gd<Node>) {
        self.submit("content", &content_name.to_string());
        let mut row = row;
        if row.has_method("close_select")
            && row
                .call("close_select", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false)
        {
            self.close_panel();
        }
    }

    /// `sector_select.sector_chosen`: writes the launch destination
    /// (`MindCampaign.set_launch_destination`) and closes the picker.
    #[func(rename = _on_launch_destination_chosen)]
    pub fn on_launch_destination_chosen(&mut self, sector: i64) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call(
                "close_dialog",
                &[GString::from("sector_select").to_variant()],
            );
        }
        let planet = GString::from(self.launch_planet.as_str());
        if let Some(mut campaign) = kit::campaign()
            && campaign.has_method("set_launch_destination")
        {
            let _ = campaign.call(
                "set_launch_destination",
                &[planet.to_variant(), sector.to_variant()],
            );
        }
    }

    /// Reads the submitted text from a `LineEdit` or `TextEdit` (kept for the
    /// GDScript surface; no config route calls it today).
    #[func(rename = _text_of)]
    pub fn text_of(&self, field: Gd<Control>) -> GString {
        if let Ok(text_edit) = field.clone().try_cast::<godot::classes::TextEdit>() {
            return text_edit.get_text();
        }
        if let Ok(line_edit) = field.try_cast::<LineEdit>() {
            return line_edit.get_text();
        }
        GString::new()
    }
}

impl MindBlockConfigFragment {
    /// `Panel` (`@onready var panel`).
    fn panel(&self) -> Option<Gd<PanelContainer>> {
        self.base()
            .get_node_or_null("Panel")?
            .try_cast::<PanelContainer>()
            .ok()
    }

    /// `Panel/Layout/Title`.
    fn title(&self) -> Option<Gd<Label>> {
        self.base()
            .get_node_or_null("Panel/Layout/Title")?
            .try_cast::<Label>()
            .ok()
    }

    /// `Panel/Layout/Body`.
    fn body(&self) -> Option<Gd<VBoxContainer>> {
        self.base()
            .get_node_or_null("Panel/Layout/Body")?
            .try_cast::<VBoxContainer>()
            .ok()
    }

    /// `LogicBlock`/`CanvasBlock.buildConfiguration` add a single pencil button
    /// (`LogicBlock.java:737-739`, `CanvasBlock.java:314-317`): no title, no
    /// background pane, no close button.
    fn buttons_only(&self) -> bool {
        dict_bool(&self.config, "logic", false) || dict_bool(&self.config, "canvas", false)
    }

    /// Panels that draw no title and no own pane background: the buttons-only
    /// configurations above and the `ItemSelection` grid / unit-command row
    /// (J05 T1/T2, whose `Styles.black6` backgrounds sit inside the widgets).
    fn frameless(&self) -> bool {
        self.buttons_only()
            || dict_bool(&self.config, "selection", false)
            || config_is_dictionary(&self.config, "commands")
            || dict_bool(&self.config, "launch", false)
            || dict_bool(&self.config, "target_dummy", false)
            || config_is_array(&self.config, "core_teams")
    }

    /// `Building.updateTableAlign` (`BuildingComp.java:1682-1685`): the table's
    /// top-centre sits at `mouseScreen(x, y - block.size * tilesize / 2f - 1)`,
    /// i.e. just under the building, every frame. A building that is gone hides
    /// the table (`BlockConfigFragment.showConfig` update: `!selected.isValid()`).
    fn align_to_building(&mut self) {
        if self.tile.x < 0 || self.tile.y < 0 {
            return;
        }
        let Some(mut host) = kit::sim_host() else {
            return;
        };
        let Some(mut camera) = kit::node("/root/Spine/World/Camera2D") else {
            return;
        };
        if !host.has_method("building_center_world") || !camera.has_method("world_to_screen") {
            return;
        }
        let center = host.call(
            "building_center_world",
            &[self.tile.x.to_variant(), self.tile.y.to_variant()],
        );
        let Ok(center) = center.try_to::<Vector2>() else {
            if self.base().is_visible() {
                self.close_panel();
            }
            return;
        };
        let mut block_size = 1i64;
        if host.has_method("block_size") {
            block_size = host
                .call(
                    "block_size",
                    &[self.tile.x.to_variant(), self.tile.y.to_variant()],
                )
                .try_to_relaxed::<i64>()
                .unwrap_or(1)
                .max(1);
        }
        let screen = camera.call(
            "world_to_screen",
            &[
                f64::from(center.x).to_variant(),
                (f64::from(center.y) - block_size as f64 * 4.0 - 1.0).to_variant(),
            ],
        );
        if let Ok(point) = screen.try_to::<Vector2>() {
            let size = self.base().get_size();
            self.base_mut()
                .set_position(Vector2::new(point.x - size.x / 2.0, point.y));
        }
    }

    /// Resolves a bundle key through `MindAssets` with a raw fallback (`_t`).
    fn localized(&self, key: &str, fallback: &str) -> GString {
        let resolved = key.strip_prefix('@').unwrap_or(key);
        if resolved.is_empty() {
            return GString::from(fallback);
        }
        let Some(assets) = kit::assets() else {
            return GString::from(fallback);
        };
        let value = assets.bind().bundle_get(GString::from(resolved));
        if value.is_empty() || value == resolved {
            return GString::from(fallback);
        }
        value
    }

    fn rebuild(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        // Detach before freeing so the panel's minimum size (the buttons-only
        // table size) no longer counts the previous rows this frame.
        for child in body.get_children().iter_shared() {
            let mut child = child.clone();
            body.remove_child(&child);
            child.queue_free();
        }
        // Separation resets per route: Java stacks the grid and the command row
        // with no table spacing (`table.row()`, `UnitFactory.java:291`) while
        // the other panels keep the themed spacing.
        body.remove_theme_constant_override("separation");
        // code-instantiated: config rows come from the `config_spec` JSON (item
        // or liquid option list / logic code field / colour picker) produced by
        // `MindInput`; the count is runtime content, not authorable in a scene.
        if dict_bool(&self.config, "link", false) {
            self.build_link_panel();
        } else if dict_bool(&self.config, "color", false) {
            self.build_color_picker();
        } else if dict_bool(&self.config, "logic", false) {
            self.build_logic_panel();
        } else if dict_bool(&self.config, "canvas", false) {
            self.build_canvas_panel();
        } else if dict_bool(&self.config, "text", false) {
            self.build_text_editor();
        } else if dict_bool(&self.config, "launch", false) {
            self.build_launch_panel();
        } else if dict_bool(&self.config, "target_dummy", false) {
            self.build_target_dummy();
        } else if config_is_array(&self.config, "core_teams") {
            self.build_core_teams();
        } else if dict_bool(&self.config, "selection", false)
            || config_is_dictionary(&self.config, "commands")
        {
            body.add_theme_constant_override("separation", 0);
            self.build_selection();
        } else {
            self.build_options();
        }
    }

    /// `LogicBlock.buildConfiguration` (`LogicBlock.java:736-739`): one 40x40
    /// `Icon.pencil` button with `Styles.cleari` opening the logic editor. The
    /// pencil is an icon-font glyph (`MindIcons`), like every upstream `Icon.*`.
    fn build_logic_panel(&mut self) {
        // code-instantiated: Java builds this single pencil button per processor
        // at tap time; it has no scene-authorable per-building shape.
        let mut pencil = self.pencil_button();
        let target = self.to_gd();
        let callable = Callable::from_fn("config_open_logic", move |_args: &[&Variant]| {
            target.clone().bind_mut().open_logic_dialog();
        });
        pencil.connect("pressed", &callable);
        let Some(mut body) = self.body() else {
            return;
        };
        body.add_child(&pencil);
    }

    /// `CanvasBlock.buildConfiguration` (`CanvasBlock.java:314-317`): one 40x40
    /// `Icon.pencil` button opening `CanvasEditDialog`.
    fn build_canvas_panel(&mut self) {
        // code-instantiated: Java builds this single pencil button per canvas at
        // tap time; it has no scene-authorable per-building shape.
        let mut pencil = self.pencil_button();
        let target = self.to_gd();
        let callable = Callable::from_fn("config_open_canvas", move |_args: &[&Variant]| {
            target.clone().bind_mut().open_canvas_dialog();
        });
        pencil.connect("pressed", &callable);
        let Some(mut body) = self.body() else {
            return;
        };
        body.add_child(&pencil);
    }

    /// The shared 40x40 `Icon.pencil` `Styles.cleari` button of the logic,
    /// canvas, message and colour routes (`_build_*_panel`).
    fn pencil_button(&self) -> Gd<Button> {
        let mut pencil = factory::glyph_button("pencil", "edit", "cleari");
        pencil.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        pencil.set_tooltip_text(&self.localized("@edit", "edit"));
        pencil.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        pencil
    }

    /// Opens the canvas editor (`CanvasEditDialog`; T14).
    fn open_canvas_dialog(&mut self) {
        self.open_dialog_at_tile("canvas_edit");
    }

    /// Opens the logic editor dialog for this processor (T7). Until the dialog
    /// is registered `open_dialog` returns false and nothing happens.
    fn open_logic_dialog(&mut self) {
        self.open_dialog_at_tile("logic");
    }

    /// Shared `MindUi.open_dialog(name, JSON.stringify({x, y}))` body of the
    /// logic/canvas pickers.
    fn open_dialog_at_tile(&mut self, dialog_name: &str) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("open_dialog") {
            return;
        }
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from(dialog_name).to_variant(),
                point_spec(
                    dict_i64(&self.config, "x", 0),
                    dict_i64(&self.config, "y", 0),
                )
                .to_variant(),
            ],
        );
    }

    /// H02 T6: link blocks (bridges, conduits, drivers, power nodes) show no
    /// option list; tapping another block of the kind while this panel is open
    /// makes the link. The current link offset is rendered read-only.
    fn build_link_panel(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        // code-instantiated: one current-link row driven by the spec's
        // `current` value; it has no scene-authorable per-building shape.
        let mut text = self.localized("@none", "none").to_string();
        if let Some(current) = self
            .config
            .get("current")
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            && dict_string(&current, "type", "") == "point"
        {
            text = format!(
                "({}, {})",
                dict_i64(&current, "x", 0),
                dict_i64(&current, "y", 0)
            );
        }
        let mut label = Label::new_alloc();
        label.set_text(&GString::from(text.as_str()));
        body.add_child(&label);
        let row = HBoxContainer::new_alloc();
        body.add_child(&row);
        let mut row_control: Gd<Control> = row.clone().upcast();
        self.add_close_button(&mut row_control);
    }

    fn build_text_editor(&mut self) {
        // `MessageBlock.buildConfiguration` (`MessageBlock.java:109-154`,
        // desktop): one 40 px `Icon.pencil` `Styles.cleari` button; it opens the
        // `@editmessage` floating dialog (380x160 TextArea, `maxTextLength`,
        // `maxNewlines`, `n / max` counter, `@ok` 130x60) and `deselect()`s.
        // L05 T2 (PAR-0380): the inline field had no counter or newline cap.
        if !dict_bool(&self.config, "multiline", false) {
            self.build_inline_text();
            return;
        }
        let Some(mut body) = self.body() else {
            return;
        };
        // code-instantiated: one pencil button per message block at tap time.
        let mut pencil = factory::glyph_button("pencil", "edit", "cleari");
        pencil.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        pencil.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        let target = self.to_gd();
        let callable =
            Callable::from_fn("config_open_message_edit", move |_args: &[&Variant]| {
                target.clone().bind_mut().open_message_edit();
            });
        pencil.connect("pressed", &callable);
        body.add_child(&pencil);
    }

    fn open_message_edit(&mut self) {
        let current = dict_string(&self.config, "current", "");
        let max_text = dict_i64(&self.config, "max_text", 400).max(1);
        let max_newlines = dict_i64(&self.config, "max_newlines", 24).max(0);
        let root = kit::node("/root/Spine/Ui/UiRoot");
        self.close_panel();
        let Some(mut root) = root else {
            return;
        };
        if !root.has_method("show_message_edit") {
            return;
        }
        let target = self.to_gd();
        let original = current.to_string();
        let callable = Callable::from_fn("config_message_edit_ok", move |args: &[&Variant]| {
            let Some(text) = args
                .first()
                .and_then(|value| value.try_to::<GString>().ok())
            else {
                return;
            };
            // `if(!a.getText().contentEquals(message)) configure(a.getText())`.
            if text.to_string() == original {
                return;
            }
            target
                .clone()
                .bind_mut()
                .submit("string", &text.to_string());
        });
        let _ = root.call(
            "show_message_edit",
            &[
                current.to_variant(),
                max_text.to_variant(),
                max_newlines.to_variant(),
                callable.to_variant(),
            ],
        );
    }

    /// Single-line text config (no Java block uses it today; kept for the spec).
    fn build_inline_text(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        let mut line = LineEdit::new_alloc();
        line.set_text(&dict_string(&self.config, "current", ""));
        line.set_h_size_flags(SizeFlags::EXPAND_FILL);
        body.add_child(&line);
        let mut row = HBoxContainer::new_alloc();
        body.add_child(&row);
        let mut apply = Button::new_alloc();
        apply.set_text(&self.localized("@ok", "OK"));
        let target = self.to_gd();
        let field = line.clone();
        let callable =
            Callable::from_fn("config_inline_text_apply", move |_args: &[&Variant]| {
                let text = field.get_text().to_string();
                target.clone().bind_mut().submit("string", &text);
            });
        apply.connect("pressed", &callable);
        row.add_child(&apply);
        let mut row_control: Gd<Control> = row.clone().upcast();
        self.add_close_button(&mut row_control);
    }

    /// `LaunchPad.buildConfiguration` (`LaunchPad.java:185-203`): one 40 px
    /// `Icon.upOpen` button opening the sector-destination picker; the choice
    /// writes `rules.sector.info.destination`
    /// (`MindCampaign.set_launch_destination`) and closes the panel
    /// (`deselect()`).
    fn build_launch_panel(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        // code-instantiated: one launch button per launch pad.
        let mut button = factory::glyph_button("up-open", "▲", "cleari");
        button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        button.set_tooltip_text(&self.localized("@launch.select", "select destination"));
        let target = self.to_gd();
        let callable =
            Callable::from_fn("config_open_launch_select", move |_args: &[&Variant]| {
                target.clone().bind_mut().open_launch_select();
            });
        button.connect("pressed", &callable);
        body.add_child(&button);
    }

    fn open_launch_select(&mut self) {
        self.launch_planet = dict_string(&self.config, "planet", "").to_string();
        self.close_panel();
        let ui_present = kit::ui().is_some();
        let root = kit::node("/root/Spine/Ui/UiRoot");
        if !ui_present {
            return;
        }
        if let Some(mut root) = root
            && root.has_method("dialog")
        {
            let dialog = root.call("dialog", &[GString::from("sector_select").to_variant()]);
            if let Ok(mut dialog) = dialog.try_to::<Gd<Node>>()
                && dialog.has_signal("sector_chosen")
            {
                // The launch-loadout flow keeps its own handler on this signal;
                // the launch pad owns the choice while its dialog is up.
                let connections = dialog.get_signal_connection_list("sector_chosen");
                for connection in connections.iter_shared() {
                    if let Some(callable) = connection
                        .get("callable")
                        .and_then(|value| value.try_to::<Callable>().ok())
                    {
                        dialog.disconnect("sector_chosen", &callable);
                    }
                }
                let callable = self.base().callable("_on_launch_destination_chosen");
                if !dialog.is_connected("sector_chosen", &callable) {
                    dialog.connect("sector_chosen", &callable);
                }
            }
        }
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let mut spec = VarDictionary::new();
        spec.set("planet", &GString::from(self.launch_planet.as_str()));
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from("sector_select").to_variant(),
                Json::stringify(&spec.to_variant()).to_variant(),
            ],
        );
    }

    /// `LightBlock.buildConfiguration` (`LightBlock.java:109-114`): one 40 px
    /// `Icon.pencil` `Styles.cleari` button; it opens `ui.picker.show(color.a(0.5),
    /// false, res -> configure(res.rgba()))` and `deselect()`s (L05 T2,
    /// PAR-0380).
    fn build_color_picker(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        // code-instantiated: one pencil button per illuminator at tap time.
        let mut pencil = factory::glyph_button("pencil", "edit", "cleari");
        pencil.set_custom_minimum_size(Vector2::new(40.0, 40.0));
        pencil.set_h_size_flags(SizeFlags::SHRINK_CENTER);
        let target = self.to_gd();
        let callable =
            Callable::from_fn("config_open_color_picker", move |_args: &[&Variant]| {
                target.clone().bind_mut().open_color_picker();
            });
        pencil.connect("pressed", &callable);
        body.add_child(&pencil);
    }

    fn open_color_picker(&mut self) {
        // `color.a = 0.5` only matters to an alpha slider; the call below opens
        // with `alpha = false`, exactly like the GDScript `show_picker(color,
        // false, consumer)`.
        let color = rgba_from_int(dict_i64(&self.config, "current", 0xffd37fff));
        let x = dict_i64(&self.config, "x", 0);
        let y = dict_i64(&self.config, "y", 0);
        let root = kit::node("/root/Spine/Ui/UiRoot");
        self.close_panel();
        let Some(mut root) = root else {
            return;
        };
        if !root.has_method("dialog") {
            return;
        }
        let picker = root.call("dialog", &[GString::from("picker").to_variant()]);
        let Ok(mut picker) = picker.try_to::<Gd<Node>>() else {
            return;
        };
        // `ColorPicker.show(color, alpha, consumer)` has no native equivalent:
        // the picker seeds from the `open_dialog` context and answers on
        // `color_selected` (see `dialogs/batch_g.rs`). The tile being
        // configured rides the picker node's metadata so the one installed
        // handler always applies to the latest call.
        picker.set_meta(
            CONFIG_COLOR_TARGET_META,
            &Vector2i::new(x as i32, y as i32).to_variant(),
        );
        if !picker.has_meta(CONFIG_COLOR_BOUND_META) {
            let picker_node = picker.clone();
            let callable = Callable::from_fn("config_color_picked", move |args: &[&Variant]| {
                let Some(result) = args.first().and_then(|value| value.try_to::<Color>().ok())
                else {
                    return Variant::nil();
                };
                let Some(target) = picker_node
                    .get_meta(CONFIG_COLOR_TARGET_META)
                    .try_to::<Vector2i>()
                    .ok()
                else {
                    return Variant::nil();
                };
                let Some(mut input) = kit::node("/root/Spine/Input") else {
                    return Variant::nil();
                };
                if !input.has_method("configure_building") {
                    return Variant::nil();
                }
                // `res.rgba()` truncates like Arc (`(int)(c * 255)`).
                let packed = arc_rgba(result);
                let _ = input.call(
                    "configure_building",
                    &[
                        target.x.to_variant(),
                        target.y.to_variant(),
                        GString::from("int").to_variant(),
                        GString::from(packed.to_string().as_str()).to_variant(),
                    ],
                );
                Variant::nil()
            });
            picker.connect("color_selected", &callable);
            picker.set_meta(CONFIG_COLOR_BOUND_META, &true.to_variant());
        }
        let mut context = VarDictionary::new();
        context.set(
            "color",
            &GString::from(format!("{:08x}", arc_rgba(color)).as_str()).to_variant(),
        );
        context.set("alpha", &false.to_variant());
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let _ = ui.call(
            "open_dialog",
            &[
                GString::from("picker").to_variant(),
                Json::stringify(&context.to_variant()).to_variant(),
            ],
        );
    }

    /// J05 T1: `ItemSelection.buildTable` blocks (item/liquid/block/unit
    /// pickers).
    ///
    /// J05 T2: unit factories additionally stack the produced unit's command
    /// row (`UnitFactory.java:292-340`); a reconstructor's whole panel is that
    /// row (`Reconstructor.java:215-240`, 5 columns of 44 px cells).
    fn build_selection(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        if dict_bool(&self.config, "selection", false) {
            // code-instantiated: the icon grid is built from the `config_spec`
            // JSON's option list (runtime content); it has no scene-authorable
            // shape.
            let mut selection = MindItemSelection::new_alloc();
            body.add_child(&selection);
            selection.bind_mut().build(self.config.clone());
            let callable = self.base().callable("_on_selection_pick");
            selection.connect("picked", &callable);
        }
        if let Some(commands) = self
            .config
            .get("commands")
            .and_then(|value| value.try_to::<VarDictionary>().ok())
        {
            // code-instantiated: the command row is built from the produced
            // unit's command list in the spec.
            let mut row = MindItemCommandRow::new_alloc();
            body.add_child(&row);
            row.bind_mut().build(commands);
            let row_node: Gd<Node> = row.clone().upcast();
            let mut arguments = Array::<Variant>::new();
            arguments.push(&row_node.to_variant());
            let callable = self.base().callable("_on_command_pick").bindv(&arguments);
            row.connect("picked", &callable);
        }
    }

    /// `TargetDummy.buildConfiguration` (`TargetDummy.java:240-262`): the
    /// enemy-team/flying checks and the armor / reset-time / hit-size fields.
    /// Every edit sends the whole 5-int array (`configureInt`).
    fn build_target_dummy(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        let mut values = vec![0i64; 5];
        if let Some(current) = self
            .config
            .get("current")
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            && let Some(array) = current
                .get("values")
                .and_then(|value| value.try_to::<VarArray>().ok())
        {
            values = array
                .iter_shared()
                .map(|value| value.try_to_relaxed::<i64>().unwrap_or(0))
                .collect();
        }
        while values.len() < 5 {
            values.push(0);
        }
        let mut enemy = CheckBox::new_alloc();
        enemy.set_text(&self.localized("@rules.enemyteam", "Enemy team"));
        enemy.set_pressed(value_at(&values, 0) == 1);
        body.add_child(&enemy);
        let mut flying = CheckBox::new_alloc();
        flying.set_text(&self.localized("@stat.flying", "Flying"));
        flying.set_pressed(value_at(&values, 1) == 1);
        body.add_child(&flying);
        let armor = self.dummy_field(
            &mut body,
            "stat.armor",
            &value_at(&values, 2).to_string(),
            "digits",
        );
        let reset = self.dummy_field(
            &mut body,
            "stat.resettime",
            &fixed(value_at(&values, 3) as f64 / 60.0, 2),
            "floats",
        );
        let mut size_bits = value_at(&values, 4);
        if size_bits == 0 {
            size_bits = float_to_bits(12.0);
        }
        let size_blocks = bits_to_float(size_bits) / TILESIZE;
        let hit = self.dummy_field(&mut body, "stat.hitsize", &fixed(size_blocks, 2), "floats");
        let mut buttons = HBoxContainer::new_alloc();
        body.add_child(&buttons);
        let mut ok = Button::new_alloc();
        ok.set_text(&self.localized("@ok", "OK"));
        buttons.add_child(&ok);
        let mut buttons_control: Gd<Control> = buttons.clone().upcast();
        self.add_close_button(&mut buttons_control);
        let target = self.to_gd();
        let callable = Callable::from_fn("config_target_dummy_ok", move |_args: &[&Variant]| {
            let tile_size = to_float(&hit.get_text().to_string()) * TILESIZE;
            let packed = [
                if enemy.is_pressed() { 1 } else { 0 },
                if flying.is_pressed() { 1 } else { 0 },
                to_int(&armor.get_text().to_string()).clamp(0, 100_000),
                (to_float(&reset.get_text().to_string()) * 60.0) as i64,
                float_to_bits(tile_size),
            ];
            let parts: Vec<String> = packed.iter().map(|value| value.to_string()).collect();
            target.clone().bind_mut().submit("ints", &parts.join(","));
        });
        ok.connect("pressed", &callable);
    }

    fn dummy_field(
        &self,
        body: &mut Gd<VBoxContainer>,
        label_key: &str,
        value: &str,
        filter: &str,
    ) -> Gd<LineEdit> {
        let mut row = HBoxContainer::new_alloc();
        body.add_child(&row);
        let mut label = Label::new_alloc();
        label.set_text(&self.localized(&format!("@{label_key}"), label_key));
        label.set_custom_minimum_size(Vector2::new(130.0, 0.0));
        row.add_child(&label);
        let mut field = LineEdit::new_alloc();
        field.set_text(value);
        field.set_h_size_flags(SizeFlags::EXPAND_FILL);
        field.set_custom_minimum_size(Vector2::new(200.0, 0.0));
        if filter == "digits" {
            field.set_virtual_keyboard_type(VirtualKeyboardType::NUMBER);
        }
        row.add_child(&field);
        field
    }

    /// `CoreBlock.buildConfiguration` (`CoreBlock.java:289-319`): 32 px
    /// team-colour toggles, 3 per row, only while `rules.coreBuildAndConfig` is
    /// on.
    fn build_core_teams(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(3);
        body.add_child(&grid);
        let current = self
            .config
            .get("current")
            .and_then(|value| value.try_to::<VarDictionary>().ok())
            .map(|current| dict_i64(&current, "team", -1))
            .unwrap_or(-1);
        for team in dict_array(&self.config, "core_teams").iter_shared() {
            let Ok(entry) = team.try_to::<VarDictionary>() else {
                continue;
            };
            let team_name = dict_string(&entry, "name", "").to_string();
            let mut button = Button::new_alloc();
            button.set_custom_minimum_size(Vector2::new(32.0, 32.0));
            button
                .set_tooltip_text(&self.localized(&format!("@team.{team_name}.name"), &team_name));
            let color_text = dict_string(&entry, "color", "ffffffff").to_string();
            let mut color = Color::from_string(color_text.as_str()).unwrap_or(Color::BLACK);
            if dict_i64(&entry, "id", -1) == current {
                color = color.lightened(0.35);
            }
            button.set_modulate(color);
            let target = self.to_gd();
            let team_id = dict_i64(&entry, "id", 0).to_string();
            let callable = Callable::from_fn("config_core_team", move |_args: &[&Variant]| {
                target.clone().bind_mut().submit("int", &team_id);
            });
            button.connect("pressed", &callable);
            grid.add_child(&button);
        }
    }

    fn build_options(&mut self) {
        let Some(mut body) = self.body() else {
            return;
        };
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_v_size_flags(SizeFlags::EXPAND_FILL);
        scroll.set_custom_minimum_size(Vector2::new(0.0, 150.0));
        body.add_child(&scroll);
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(4);
        scroll.add_child(&grid);
        let mut clear = Button::new_alloc();
        let clear_text = self.localized("@clear", "clear");
        clear.set_text(&clear_text);
        clear.set_tooltip_text(&clear_text);
        let target = self.to_gd();
        let callable = Callable::from_fn("config_clear", move |_args: &[&Variant]| {
            target.clone().bind_mut().submit("none", "");
        });
        clear.connect("pressed", &callable);
        grid.add_child(&clear);
        let mut grid_control: Gd<Control> = grid.clone().upcast();
        self.add_close_button(&mut grid_control);
        for option in dict_array(&self.config, "options").iter_shared() {
            let Ok(option) = option.try_to::<VarDictionary>() else {
                continue;
            };
            let option_name = dict_string(&option, "name", "").to_string();
            let label =
                self.localized(&dict_string(&option, "label", "").to_string(), &option_name);
            let mut button = Button::new_alloc();
            button.set_text(&label);
            button.set_tooltip_text(&label);
            let target = self.to_gd();
            let content = option_name.clone();
            let callable = Callable::from_fn("config_option", move |_args: &[&Variant]| {
                target.clone().bind_mut().submit("content", &content);
            });
            button.connect("pressed", &callable);
            grid.add_child(&button);
        }
    }

    /// code-instantiated: transient close affordance (one per open).
    fn add_close_button(&mut self, row: &mut Gd<Control>) {
        let mut close = Button::new_alloc();
        close.set_text("X");
        close.set_tooltip_text("close");
        let target = self.to_gd();
        let callable = Callable::from_fn("config_close", move |_args: &[&Variant]| {
            target.clone().bind_mut().close_panel();
        });
        close.connect("pressed", &callable);
        row.add_child(&close);
    }

    fn submit(&mut self, kind: &str, value: &str) {
        let Some(mut input) = kit::node("/root/Spine/Input") else {
            return;
        };
        if !input.has_method("configure_building") {
            return;
        }
        let _ = input.call(
            "configure_building",
            &[
                dict_i64(&self.config, "x", 0).to_variant(),
                dict_i64(&self.config, "y", 0).to_variant(),
                GString::from(kind).to_variant(),
                GString::from(value).to_variant(),
            ],
        );
    }
}

/// `JSON.stringify({"x": x, "y": y})` — the `logic`/`canvas_edit` dialog context.
fn point_spec(x: i64, y: i64) -> GString {
    let mut spec = VarDictionary::new();
    spec.set("x", x);
    spec.set("y", y);
    Json::stringify(&spec.to_variant())
}

/// Reads a `bool` from the spec with GDScript's `bool(...)` coercion.
fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<bool>().ok())
        .unwrap_or(default)
}

/// Reads an `int` from the spec with GDScript's `int(...)` coercion.
fn dict_i64(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<i64>().ok())
        .unwrap_or(default)
}

/// Reads a `String` from the spec with GDScript's `str(...)` coercion.
fn dict_string(dict: &VarDictionary, key: &str, default: &str) -> GString {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<GString>().ok())
        .unwrap_or_else(|| GString::from(default))
}

/// Reads an `Array` from the spec (`[]` when absent or not an array).
fn dict_array(dict: &VarDictionary, key: &str) -> VarArray {
    dict.get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

/// `_config.get(key, null) is Dictionary`.
fn config_is_dictionary(dict: &VarDictionary, key: &str) -> bool {
    dict.get(key)
        .is_some_and(|value| value.try_to::<VarDictionary>().is_ok())
}

/// `_config.get(key, null) is Array`.
fn config_is_array(dict: &VarDictionary, key: &str) -> bool {
    dict.get(key)
        .is_some_and(|value| value.try_to::<VarArray>().is_ok())
}

/// `values[index]` after the five-entry pad in [`MindBlockConfigFragment`].
fn value_at(values: &[i64], index: usize) -> i64 {
    values.get(index).copied().unwrap_or(0)
}

/// `Color.rgba8888` → Godot `Color` (the packed value is `0xRRGGBBAA`).
fn rgba_from_int(value: i64) -> Color {
    let packed = value as u32;
    Color::from_rgba8(
        ((packed >> 24) & 0xff) as u8,
        ((packed >> 16) & 0xff) as u8,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    )
}

/// Arc `Color.rgba()` (`(int)(c * 255)` per channel, `0xRRGGBBAA`).
///
/// The GDScript reached this through `load("res://ui/dialogs/color_picker.gd")`;
/// the port inlines the same arithmetic so the fragment does not depend on a
/// script that is itself being retired.
fn arc_rgba(color: Color) -> i64 {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0) as i64;
    (channel(color.r) << 24) | (channel(color.g) << 16) | (channel(color.b) << 8) | channel(color.a)
}

/// `_float_to_bits`: the IEEE-754 single-precision bit pattern as an int
/// (`StreamPeerBuffer.put_float` + `get_32`).
fn float_to_bits(value: f64) -> i64 {
    i64::from(f32::to_bits(value as f32))
}

/// `_bits_to_float`: reinterpret an IEEE-754 single-precision bit pattern
/// (`StreamPeerBuffer.put_32` + `get_float`).
fn bits_to_float(bits: i64) -> f64 {
    f64::from(f32::from_bits(bits as u32))
}

/// `_fixed(value, decimals)`: `"%.<decimals>f" % value`.
fn fixed(value: f64, decimals: i32) -> String {
    format!("{:.*}", decimals.max(0) as usize, value)
}

/// GDScript `String.to_int()`: the longest leading integer prefix, else `0`.
fn to_int(text: &str) -> i64 {
    let trimmed = text.trim_start();
    let bytes = trimmed.as_bytes();
    let mut index = 0usize;
    let mut sign = 1i64;
    if bytes.first() == Some(&b'-') {
        sign = -1;
        index = 1;
    } else if bytes.first() == Some(&b'+') {
        index = 1;
    }
    let mut value = 0i64;
    let mut digits = 0usize;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        let digit = bytes.get(index).copied().unwrap_or(b'0');
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(digit - b'0'));
        digits += 1;
        index += 1;
    }
    if digits == 0 {
        return 0;
    }
    value.saturating_mul(sign)
}

/// GDScript `String.to_float()`: the longest leading float prefix, else `0`.
fn to_float(text: &str) -> f64 {
    let trimmed = text.trim_start();
    let bytes = trimmed.as_bytes();
    let mut index = 0usize;
    if bytes.first() == Some(&b'-') || bytes.first() == Some(&b'+') {
        index = 1;
    }
    let mut digits = 0usize;
    while bytes.get(index).is_some_and(u8::is_ascii_digit) {
        digits += 1;
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            digits += 1;
            index += 1;
        }
    }
    if digits == 0 {
        return 0.0;
    }
    if bytes.get(index) == Some(&b'e') || bytes.get(index) == Some(&b'E') {
        let mut exponent = index + 1;
        if bytes.get(exponent) == Some(&b'-') || bytes.get(exponent) == Some(&b'+') {
            exponent += 1;
        }
        let mut exponent_digits = 0usize;
        while bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            exponent_digits += 1;
            exponent += 1;
        }
        if exponent_digits > 0 {
            index = exponent;
        }
    }
    trimmed
        .get(..index)
        .and_then(|slice| slice.parse::<f64>().ok())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_bits_round_trip() {
        // `StreamPeerBuffer.put_float`/`get_32` is a plain f32 bit cast.
        assert_eq!(float_to_bits(12.0), i64::from(12.0f32.to_bits()));
        assert_eq!(bits_to_float(float_to_bits(12.0)), 12.0);
        assert_eq!(bits_to_float(0), 0.0);
    }

    #[test]
    fn rgba_packing_matches_arc() {
        assert_eq!(arc_rgba(rgba_from_int(0xffd37fff)), 0xffd37f7f);
        assert_eq!(
            rgba_from_int(0xffd37fff),
            Color::from_rgba8(255, 211, 127, 255)
        );
        // Arc truncates instead of rounding.
        assert_eq!(arc_rgba(Color::from_rgba(0.5, 0.0, 1.0, 1.0)), 0x7f00ffff);
    }

    #[test]
    fn number_parsing_matches_gdscript_prefix_rules() {
        assert_eq!(to_int("42"), 42);
        assert_eq!(to_int("-7abc"), -7);
        assert_eq!(to_int("abc"), 0);
        assert_eq!(to_int(""), 0);
        assert_eq!(to_float("1.5"), 1.5);
        assert_eq!(to_float("-2.25xyz"), -2.25);
        assert_eq!(to_float("1e3"), 1000.0);
        assert_eq!(to_float("nope"), 0.0);
    }

    #[test]
    fn fixed_formats_with_the_requested_precision() {
        assert_eq!(fixed(1.0 / 3.0, 2), "0.33");
        assert_eq!(fixed(12.0, 2), "12.00");
    }
}
