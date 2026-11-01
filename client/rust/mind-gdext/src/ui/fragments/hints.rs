// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/HintsFragment.java (45-160, 169-304).
//
// Hint catalogue + display driver. Hints never time out: they complete on their
// upstream predicate or the Skip button (which persists `<name>-hint-done`), a
// hint whose `shown` predicate turns false is hidden without persisting, and
// the display gate is `saves.getTotalPlaytime() > 8000` (total, not session,
// and only when not in a cutscene). Input completions consume
// `MindInput.take_hint_keys()` (G08 T15); `derelictmouse` is host-detected from
// the hovered tile's team.

//! Hint overlay (`HintsFragment`): the hint catalogue, the queue/state machine
//! and the `Panel/Layout/Box/Label` + `Skip` presentation.
//!
//! One poll per frame (`MindSimHost.hint_state_json`, which sweeps the
//! buildings and the grid) supplies every predicate input; the campaign facade
//! supplies the sector state, the total playtime and the unlock gates. This
//! node only reads view state (`MindUi`, `MindCampaign`, `MindSimHost`,
//! `MindInput`, the camera) and never mutates the sim.

use std::collections::HashSet;

use godot::classes::text_server::AutowrapMode;
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{Button, Camera2D, Control, IControl, Json, RichTextLabel};
use godot::prelude::*;

use crate::ui::kit;

/// Upstream `DefaultHint` declaration order (`HintsFragment.java:169-304`;
/// `respawn` is mobile-only and dropped on desktop).
const HINTS: [&str; 26] = [
    "desktopMove",
    "zoom",
    "breaking",
    "desktopShoot",
    "depositItems",
    "desktopPause",
    "unitControl",
    "unitSelectControl",
    "launch",
    "schematicSelect",
    "conveyorPathfind",
    "boost",
    "blockInfo",
    "derelict",
    "payloadPickup",
    "payloadDrop",
    "waveFire",
    "rebuildSelect",
    "guardian",
    "cannotUpgrade",
    "factoryControl",
    "coreUpgrade",
    "serpuloCoreZone",
    "presetLaunch",
    "presetDifficulty",
    "coreIncinerate",
];

/// `HintsFragment` display gate: `saves.getTotalPlaytime() > 8000` ms.
const PLAYTIME_GATE_MS: i64 = 8000;

/// `HintsFragment.hide()`: fade duration (s).
const HIDE_FADE_SECONDS: f64 = 0.6;

/// `HintsFragment.hide()`: the panel slides 200 px while it fades out.
const HIDE_SLIDE_Y: f32 = 200.0;

/// `HintsFragment.display()`: fade-in duration (s).
const DISPLAY_FADE_SECONDS: f64 = 1.0;

/// `Binding.zoom` completion: the camera zoom must have changed by more than
/// this since the last poll.
const ZOOM_EPSILON: f32 = 0.01;

/// `MindHintsFragment` — the hint overlay.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindHintsFragment {
    base: Base<Control>,
    /// `_current`: the hint on screen (empty when none).
    current: String,
    /// Hints persisted as finished (`DefaultHint.finish`; Skip/complete paths).
    done: HashSet<String>,
    /// Hints removed for this session (`hints.remove(hint)`; never persisted).
    finished: HashSet<String>,
    /// The `MindUi` autoload, cached at ready (`_ui`).
    ui: Option<Gd<Node>>,
    /// Last camera zoom seen by the `zoom` completion predicate.
    last_zoom: f32,
    /// Pending input triggers from `MindInput.take_hint_keys()` (G08 T15).
    hint_keys: Vec<String>,
    /// Last probed hover tile (`derelictmouse`).
    mouse_tile: Vector2i,
}

#[godot_api]
impl IControl for MindHintsFragment {
    fn ready(&mut self) {
        self.base_mut().set_visible(false);
        self.ui = kit::ui();
        // `MindRichLabel` opts into wrapping explicitly, like every wrapped
        // Arc label.
        if let Some(mut label) = self.label() {
            label.set_autowrap_mode(AutowrapMode::WORD_SMART);
        }
        if let Some(mut button) = self.skip_button() {
            button.set_text(&kit::t("@hint.skip"));
            let callable = self.base().callable("complete_hint");
            if !button.is_connected("pressed", &callable) {
                button.connect("pressed", &callable);
            }
        }
        self.snapshot_zoom();
        // The GDScript member initializer; `#[class(init)]` would zero it.
        self.mouse_tile = Vector2i::new(-1, -1);
        self.base_mut().set_process(true);
    }

    fn process(&mut self, _delta: f64) {
        // `HintsFragment.java:45`: the fragment shows only with the `hints`
        // setting on and the HUD up.
        if !self.hints_enabled() || !self.in_game() {
            self.base_mut().set_visible(false);
            return;
        }
        self.poll_hint_keys();
        // One poll per frame (`hint_state_json` sweeps the buildings and grid).
        let hints = self.hint_state();
        let state = self.sector_state();
        // `HintsFragment.java:46-68` driver (the current hint first).
        let current = self.current.clone();
        if !current.is_empty() {
            let cutscene = self.is_cutscene();
            self.base_mut().set_visible(!cutscene);
            if self.evaluate_complete(&current, &state, &hints) {
                self.complete_hint();
            } else if !self.evaluate_shown(&current, &state, &hints) {
                // `hide()`: not persisted; the hint stays in the list.
                self.hide_current();
            }
            return;
        }
        self.base_mut().set_visible(false);
        self.poll_derelict_mouse();
        // `else if(!hints.isEmpty())`: first hint in ordinal order whose
        // `show()` holds. A hint already complete is removed silently (never
        // persisted).
        for hint_name in HINTS {
            if self.done.contains(hint_name)
                || self.finished.contains(hint_name)
                || self.hint_finished(hint_name)
            {
                continue;
            }
            if !self.evaluate_shown(hint_name, &state, &hints) {
                continue;
            }
            if self.evaluate_complete(hint_name, &state, &hints) {
                self.finished.insert(hint_name.to_string());
                continue;
            }
            // `renderer.isCutscene() || !state.isGame()`: wait (no display).
            if self.is_cutscene() {
                return;
            }
            if self.total_playtime_ms() <= PLAYTIME_GATE_MS {
                return;
            }
            self.show_hint(GString::from(hint_name));
            return;
        }
    }
}

#[godot_api]
impl MindHintsFragment {
    /// `HintsFragment.display()`: fades the panel in over 1 s, then shows
    /// `hint_name`.
    #[func]
    pub fn show_hint(&mut self, hint_name: GString) {
        let hint_name = hint_name.to_string();
        self.current.clone_from(&hint_name);
        self.finished.remove(&hint_name);
        if let Some(mut panel) = self.panel() {
            let mut tween = panel.create_tween();
            let mut property = tween.tween_property(
                &panel,
                "modulate:a",
                &1.0f32.to_variant(),
                DISPLAY_FADE_SECONDS,
            );
            property.from(&0.0f32.to_variant());
        }
        if let Some(mut label) = self.label() {
            // `HintsFragment.java:349-355`: desktop replaces tap -> click.
            let text = kit::tm(&format!("@hint.{hint_name}"))
                .to_string()
                .replace("tap", "click")
                .replace("Tap", "Click");
            label.set_text(&GString::from(text.as_str()));
        }
        self.base_mut().set_visible(true);
        self.snapshot_zoom();
    }

    /// `HintsFragment.complete()`: persists the hint and removes it (the
    /// `Skip` button target).
    #[func]
    pub fn complete_hint(&mut self) {
        if !self.current.is_empty() {
            self.done.insert(self.current.clone());
            if let Some(mut ui) = self.ui.clone()
                && ui.has_method("hint_finish")
            {
                let name = GString::from(self.current.as_str()).to_variant();
                let _ = ui.call("hint_finish", &[name]);
            }
        }
        self.current.clear();
        self.base_mut().set_visible(false);
        if let Some(mut panel) = self.panel() {
            let mut modulate = panel.get_modulate();
            modulate.a = 1.0;
            panel.set_modulate(modulate);
        }
    }

    /// `Skip` button action (`_skip.pressed` is wired to [`Self::complete_hint`];
    /// this alias keeps the button action addressable by name).
    #[func]
    pub fn skip(&mut self) {
        self.complete_hint();
    }

    /// The `HINTS` catalogue in upstream declaration order. The GDScript
    /// exposed it as a `const`, but GDExtension constants cannot be arrays, so
    /// the parity table check reads the catalogue through this call.
    #[func]
    pub fn hint_names(&self) -> PackedStringArray {
        let mut names = PackedStringArray::new();
        for hint_name in HINTS {
            names.push(&GString::from(hint_name));
        }
        names
    }

    /// `HintsFragment.java:169-304` per-hint `shown()` predicates. `state` is
    /// the `MindCampaign.get_sector_state()` projection, `hints` the
    /// `hint_state_json` poll (both fetched once per frame).
    ///
    /// Exported as `_hint_shown` (its GDScript name) so the predicate table
    /// check can drive it directly.
    #[func(rename = _hint_shown)]
    pub fn hint_shown(
        &self,
        hint_name: GString,
        state: VarDictionary,
        hints: VarDictionary,
    ) -> bool {
        self.evaluate_shown(&hint_name.to_string(), &state, &hints)
    }

    /// The matching `complete()` predicates (`HintsFragment.java:169-304`),
    /// exported as `_hint_complete` (its GDScript name).
    #[func(rename = _hint_complete)]
    pub fn hint_complete(
        &mut self,
        hint_name: GString,
        state: VarDictionary,
        hints: VarDictionary,
    ) -> bool {
        self.evaluate_complete(&hint_name.to_string(), &state, &hints)
    }

    /// `HintsFragment.hide()` tween tail: undo the slide/fade unless a hint
    /// with the same name took the panel over in the meantime.
    #[func]
    fn finish_hide(&mut self, name_done: GString) {
        if name_done == self.current.as_str() {
            return;
        }
        let Some(mut panel) = self.panel() else {
            return;
        };
        let mut position = panel.get_position();
        position.y -= HIDE_SLIDE_Y;
        panel.set_position(position);
        let mut modulate = panel.get_modulate();
        modulate.a = 1.0;
        panel.set_modulate(modulate);
    }
}

impl MindHintsFragment {
    /// `Panel` (`@onready var _panel: Control`).
    fn panel(&self) -> Option<Gd<Control>> {
        self.base()
            .get_node_or_null("Panel")?
            .try_cast::<Control>()
            .ok()
    }

    /// `Panel/Layout/Box/Label` — the scene node is the `MindRichLabel` class;
    /// the driver only needs its `RichTextLabel` base (`set_text`/autowrap).
    fn label(&self) -> Option<Gd<RichTextLabel>> {
        self.base()
            .get_node_or_null("Panel/Layout/Box/Label")?
            .try_cast::<RichTextLabel>()
            .ok()
    }

    /// `Panel/Layout/Skip`.
    fn skip_button(&self) -> Option<Gd<Button>> {
        self.base()
            .get_node_or_null("Panel/Layout/Skip")?
            .try_cast::<Button>()
            .ok()
    }

    /// `/root/Spine/SimHost` (`_host()`).
    fn host(&self) -> Option<Gd<Node>> {
        kit::sim_host()
    }

    /// `/root/Spine/Input`.
    fn input_node(&self) -> Option<Gd<Node>> {
        kit::node("/root/Spine/Input")
    }

    /// `/root/Spine/World/Camera2D` (`_camera()`).
    fn camera(&self) -> Option<Gd<Node>> {
        kit::node("/root/Spine/World/Camera2D")
    }

    /// The camera's `zoom.x` (`camera.get("zoom").x`; `None` before it exists).
    fn camera_zoom(&self) -> Option<f32> {
        let camera = self.camera()?.try_cast::<Camera2D>().ok()?;
        Some(camera.get_zoom().x)
    }

    /// `HintsFragment.java:169-304` per-hint `shown()` predicates.
    fn evaluate_shown(
        &self,
        hint_name: &str,
        state: &VarDictionary,
        hints: &VarDictionary,
    ) -> bool {
        let preset = dict_string(state, "preset", "");
        let campaign = dict_bool(state, "campaign", false);
        let planet = dict_string(state, "planet", "");
        let tutorial = preset == "groundZero";
        let serpulo = dict_bool(hints, "serpulo", true);
        let dead = dict_bool(hints, "player_dead", true);
        match hint_name {
            "desktopMove" | "zoom" => true,
            // `isTutorial && team conveyor count > 5`.
            "breaking" => tutorial && dict_int(hints, "conveyors", 0) > 5,
            // `isSerpulo && enemies > 0`.
            "desktopShoot" => serpulo && dict_int(hints, "enemies", 0) > 0,
            "depositItems" => !dead && dict_bool(hints, "player_has_item", false),
            // `isTutorial && !net.active() && state.wave >= 2`.
            "desktopPause" => tutorial && dict_int(state, "wave", 0) >= 2,
            "unitControl" => serpulo && dict_int(hints, "team_units", 0) > 2 && !dead,
            "unitSelectControl" => {
                serpulo
                    && dict_int(hints, "team_units", 0) > 3
                    && !dead
                    && preset != "fungalPass"
                    && !dict_bool(hints, "fungal_pass_saved", false)
            }
            "launch" => {
                (preset == "groundZero" || preset == "onset") && dict_bool(state, "captured", false)
            }
            "schematicSelect" => placed_has(hints, &["router", "duct-router"]),
            "conveyorPathfind" => self.selected_block() == "titanium-conveyor",
            "boost" => !dead && dict_bool(hints, "player_can_boost", false),
            "blockInfo" => self.selected_block() == "graphite-press",
            "derelict" => has_event(hints, "derelictmouse") && !tutorial,
            "payloadPickup" => serpulo && !dead && dict_int(hints, "player_payload", -1) == 0,
            "payloadDrop" => !dead && dict_int(hints, "player_payload", -1) > 0,
            "waveFire" => has_event(hints, "fire") && self.content_unlocked("wave"),
            "rebuildSelect" => dict_int(hints, "team_rebuild_plans", 0) >= 10,
            "guardian" => {
                let boss = boss(hints);
                !boss.is_empty()
                    && serpulo
                    && dict_float(&boss, "armor", 0.0) >= 4.0
                    && self.content_unlocked("salvo")
                    && !dict_bool(&boss, "flying", false)
            }
            "cannotUpgrade" => has_event(hints, "cannotupgrade"),
            "factoryControl" => {
                !(campaign && preset == "onset") && dict_int(hints, "fabricators", 0) > 0
            }
            "coreUpgrade" => {
                campaign
                    && planet == "serpulo"
                    && self.content_unlocked("core-foundation")
                    && dict_string(hints, "core_block", "") == "core-shard"
                    && dict_bool(hints, "core_has_foundation_cost", false)
            }
            "serpuloCoreZone" => {
                campaign
                    && planet == "serpulo"
                    && dict_bool(hints, "core_zone_present", false)
                    && (!dict_bool(hints, "attack_mode", false)
                        || dict_int(hints, "cores_destroyed", 0) > 0)
            }
            "presetLaunch" => campaign && preset.is_empty() && state.contains_key("sector"),
            "presetDifficulty" => {
                campaign
                    && preset.is_empty()
                    && dict_float(hints, "sector_threat", 0.0) >= 0.5
                    && !dict_bool(hints, "tar_fields_captured", true)
            }
            "coreIncinerate" => {
                campaign
                    && !dict_string(hints, "core_block", "").is_empty()
                    && dict_int(hints, "core_copper", 0) >= dict_int(hints, "core_capacity", 0) - 10
            }
            _ => false,
        }
    }

    /// The matching `complete()` predicates (`HintsFragment.java:169-304`).
    fn evaluate_complete(
        &mut self,
        hint_name: &str,
        state: &VarDictionary,
        hints: &VarDictionary,
    ) -> bool {
        match hint_name {
            "desktopMove" => self.player_unit_moved(),
            // `Binding.zoom`: the camera zoom changed since the last poll.
            "zoom" => {
                let Some(zoom) = self.camera_zoom() else {
                    return false;
                };
                let zoomed = (zoom - self.last_zoom).abs() > ZOOM_EPSILON;
                self.last_zoom = zoom;
                zoomed
            }
            "breaking" => has_event(hints, "break"),
            "desktopShoot" => self.consume_hint_key("shoot"),
            "depositItems" => {
                !dict_bool(hints, "player_dead", true)
                    && !dict_bool(hints, "player_has_item", false)
            }
            "desktopPause" => self.is_paused(),
            "unitControl" => {
                !dict_bool(hints, "player_dead", true)
                    && !dict_bool(hints, "player_spawned_by_core", true)
            }
            // `commandMode && selectedUnits > 0 && first CommandAI.targetPos
            // != null` (`HintsFragment.java:199-203`).
            "unitSelectControl" => {
                self.input_command_mode()
                    && self.selection_size() > 0
                    && self.first_selection_has_target()
            }
            "launch" => self.dialog_shown("planet"),
            "schematicSelect" => {
                self.consume_hint_key("schematic_select_release")
                    || self.consume_hint_key("pick_tap")
            }
            "conveyorPathfind" => self.consume_hint_key("diagonal_release"),
            "boost" => self.consume_hint_key("boost_down"),
            "blockInfo" => self.dialog_shown("content"),
            "derelict" => has_event(hints, "derelictbreak"),
            "payloadPickup" => dict_int(hints, "player_payload", -1) > 0,
            "payloadDrop" => dict_int(hints, "player_payload", -1) == 0,
            "waveFire" => dict_int(hints, "extinguishers", 0) > 0,
            "rebuildSelect" => self.consume_hint_key("rebuild_selecting"),
            "guardian" => dict_int(&boss_root(hints), "count", 0) == 0,
            // `complete: () -> false` (`HintsFragment.java:266-269`): these
            // hints are only ever dismissed manually.
            "cannotUpgrade" | "coreIncinerate" => false,
            "factoryControl" => has_event(hints, "factorycontrol"),
            "coreUpgrade" => placed_has(hints, &["core-foundation"]),
            "serpuloCoreZone" => dict_int(hints, "cores", 0) > 1,
            // `state.getSector().preset == SectorPresets.frozenForest`
            // (`HintsFragment.java:289-292`): only reaching Frozen Forest
            // completes it.
            "presetLaunch" => {
                dict_bool(state, "campaign", false)
                    && dict_string(state, "preset", "") == "frozenForest"
            }
            "presetDifficulty" => {
                dict_bool(state, "campaign", false) && !dict_string(state, "preset", "").is_empty()
            }
            _ => false,
        }
    }

    /// `HintsFragment.hide()` (fade 0.6 s + 200 px slide; not persisted).
    fn hide_current(&mut self) {
        let name_done = std::mem::take(&mut self.current);
        self.base_mut().set_visible(false);
        let Some(mut panel) = self.panel() else {
            return;
        };
        let mut tween = panel.create_tween();
        tween.set_trans(TransitionType::SINE);
        tween.set_ease(EaseType::IN_OUT);
        tween.tween_property(
            &panel,
            "modulate:a",
            &0.0f32.to_variant(),
            HIDE_FADE_SECONDS,
        );
        let slide_to = panel.get_position().y + HIDE_SLIDE_Y;
        let mut parallel = tween.parallel();
        parallel.tween_property(
            &panel,
            "position:y",
            &slide_to.to_variant(),
            HIDE_FADE_SECONDS,
        );
        // The tween callback carries this hide's own name (`name_done`), so an
        // overlapping hide cannot be mistaken for it.
        let mut arguments = VarArray::new();
        arguments.push(&GString::from(name_done.as_str()).to_variant());
        if let Ok(arguments) = arguments.to_variant().try_to::<AnyArray>() {
            let callable = self.base().callable("finish_hide").bindv(&arguments);
            tween.tween_callback(&callable);
        }
    }

    /// `MindInput.take_hint_keys()` (G08 T15) buffered into `hint_keys`.
    fn poll_hint_keys(&mut self) {
        let Some(mut input) = self.input_node() else {
            return;
        };
        if !input.has_method("take_hint_keys") {
            return;
        }
        let Ok(keys) = input
            .call("take_hint_keys", &[])
            .try_to::<PackedStringArray>()
        else {
            return;
        };
        for key in keys.as_slice() {
            let key = key.to_string();
            if !self.hint_keys.contains(&key) {
                self.hint_keys.push(key);
            }
        }
    }

    /// Consumes a pending input trigger (one-shot; `keyTap`/`keyRelease` parity).
    fn consume_hint_key(&mut self, key: &str) -> bool {
        let Some(index) = self.hint_keys.iter().position(|pending| pending == key) else {
            return false;
        };
        self.hint_keys.remove(index);
        true
    }

    /// `HintsFragment.java:100-102`: while no hint is up, hovering a team-0
    /// (derelict) building adds `derelictmouse`.
    fn poll_derelict_mouse(&mut self) {
        let Some(mut camera) = self.camera() else {
            return;
        };
        let Some(mut host) = self.host() else {
            return;
        };
        if !host.has_method("building_team_at") {
            return;
        }
        if !camera.has_method("screen_to_tile") {
            return;
        }
        let Some(viewport) = self.base().get_viewport() else {
            return;
        };
        let mouse = viewport.get_mouse_position();
        let tile = camera.call(
            "screen_to_tile",
            &[mouse.x.to_variant(), mouse.y.to_variant()],
        );
        let Ok(tile) = tile.try_to::<Vector2i>() else {
            return;
        };
        if tile.x < 0 || tile.y < 0 || tile == self.mouse_tile {
            return;
        }
        self.mouse_tile = tile;
        let team = host.call(
            "building_team_at",
            &[tile.x.to_variant(), tile.y.to_variant()],
        );
        if team.try_to_relaxed::<i64>().unwrap_or(-1) == 0 {
            let event = GString::from("derelictmouse").to_variant();
            let _ = host.call("hint_event", &[event]);
        }
    }

    /// `MindCampaign.get_sector_state()` (`{}` when the campaign is absent).
    fn sector_state(&self) -> VarDictionary {
        let Some(mut campaign) = kit::campaign() else {
            return VarDictionary::new();
        };
        if !campaign.has_method("get_sector_state") {
            return VarDictionary::new();
        }
        campaign
            .call("get_sector_state", &[])
            .try_to::<VarDictionary>()
            .unwrap_or_default()
    }

    /// `MindSimHost.hint_state_json()` (one sweep per frame).
    fn hint_state(&self) -> VarDictionary {
        let Some(mut host) = self.host() else {
            return VarDictionary::new();
        };
        if !host.has_method("hint_state_json") {
            return VarDictionary::new();
        }
        let Ok(payload) = host
            .call("hint_state_json", &[])
            .try_to_relaxed::<GString>()
        else {
            return VarDictionary::new();
        };
        Json::parse_string(&payload)
            .try_to::<VarDictionary>()
            .unwrap_or_default()
    }

    /// `DefaultHint.finished`: `Core.settings.getBool(name + "-hint-done")`.
    fn hint_finished(&self, hint_name: &str) -> bool {
        let Some(mut ui) = self.ui.clone() else {
            return false;
        };
        if !ui.has_method("hint_finished") {
            return false;
        }
        ui.call("hint_finished", &[GString::from(hint_name).to_variant()])
            .try_to_relaxed::<bool>()
            .unwrap_or(false)
    }

    /// `saves.getTotalPlaytime() > 8000` (total, not session time).
    fn total_playtime_ms(&self) -> i64 {
        let Some(mut campaign) = kit::campaign() else {
            return 0;
        };
        if !campaign.has_method("get_total_playtime") {
            return 0;
        }
        campaign
            .call("get_total_playtime", &[])
            .try_to_relaxed::<i64>()
            .unwrap_or(0)
    }

    /// `settings.getBool("hints", true)`.
    fn hints_enabled(&self) -> bool {
        let Some(mut ui) = self.ui.clone() else {
            return true;
        };
        if !ui.has_method("settings_get_flag_default") {
            return true;
        }
        ui.call(
            "settings_get_flag_default",
            &[GString::from("hints").to_variant(), true.to_variant()],
        )
        .try_to_relaxed::<bool>()
        .unwrap_or(false)
    }

    /// The hint overlay shows only during a live (non-menu) game and when the
    /// HUD is up (`ui.hudfrag.shown()`).
    fn in_game(&self) -> bool {
        if let Some(mut ui) = self.ui.clone() {
            if !ui.has_method("hud_visible") {
                return false;
            }
            let hud_visible = ui
                .call("hud_visible", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false);
            if !hud_visible {
                return false;
            }
        }
        let Some(mut host) = self.host() else {
            return false;
        };
        if !host.has_method("get_state") {
            return false;
        }
        host.call("get_state", &[])
            .try_to_relaxed::<GString>()
            .is_ok_and(|state| state == "playing")
    }

    /// `MindSimHost.is_paused()`.
    fn is_paused(&self) -> bool {
        let Some(mut host) = self.host() else {
            return false;
        };
        host.has_method("is_paused")
            && host
                .call("is_paused", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false)
    }

    /// `renderer.isCutscene()`: no hint triggers or advances during a cutscene.
    fn is_cutscene(&self) -> bool {
        let Some(mut camera) = self.camera() else {
            return false;
        };
        camera.has_method("is_cutscene")
            && camera
                .call("is_cutscene", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false)
    }

    /// `DesktopHint.complete`: the player unit's move axis is non-zero.
    fn player_unit_moved(&self) -> bool {
        let Some(mut host) = self.host() else {
            return false;
        };
        host.has_method("player_unit_moved")
            && host
                .call("player_unit_moved", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false)
    }

    /// `UnlockableContent.unlockedHost()` by name (G05 T4b gates).
    fn content_unlocked(&self, content_name: &str) -> bool {
        let Some(mut campaign) = kit::campaign() else {
            return true;
        };
        if !campaign.has_method("content_unlocked") {
            return true;
        }
        campaign
            .call(
                "content_unlocked",
                &[GString::from(content_name).to_variant()],
            )
            .try_to_relaxed::<bool>()
            .unwrap_or(false)
    }

    /// `state.block.name` (`MindSimHost.selected_block`).
    fn selected_block(&self) -> String {
        let Some(mut host) = self.host() else {
            return String::new();
        };
        if !host.has_method("selected_block") {
            return String::new();
        }
        host.call("selected_block", &[])
            .try_to_relaxed::<GString>()
            .map(|name| name.to_string())
            .unwrap_or_default()
    }

    /// `input.commandMode` (G08).
    fn input_command_mode(&self) -> bool {
        let Some(mut input) = self.input_node() else {
            return false;
        };
        input.has_method("command_mode")
            && input
                .call("command_mode", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false)
    }

    /// The selected-unit id list (`selection_json`), or `None` when absent.
    fn selection(&self) -> Option<VarArray> {
        let mut input = self.input_node()?;
        if !input.has_method("selection_json") {
            return None;
        }
        let payload = input
            .call("selection_json", &[])
            .try_to_relaxed::<GString>()
            .ok()?;
        Json::parse_string(&payload).try_to::<VarArray>().ok()
    }

    /// Selected unit count (`control.input.selectedUnits.size`).
    fn selection_size(&self) -> usize {
        self.selection()
            .map(|selection| selection.len())
            .unwrap_or(0)
    }

    /// `selectedUnits.first().controller() instanceof CommandAI ai && ai.targetPos
    /// != null` (`HintsFragment.java:199-203`): the first selected unit id
    /// (`selection_json` order) must have a live command target row
    /// (`SimHost.command_targets_json` merges the move target and attack
    /// target).
    fn first_selection_has_target(&self) -> bool {
        let Some(selection) = self.selection() else {
            return false;
        };
        let Some(first) = selection.get(0) else {
            return false;
        };
        let first_id = first.try_to_relaxed::<i64>().unwrap_or_default();
        let Some(mut host) = self.host() else {
            return false;
        };
        if !host.has_method("command_targets_json") {
            return false;
        }
        let Ok(payload) = host
            .call("command_targets_json", &[])
            .try_to_relaxed::<GString>()
        else {
            return false;
        };
        let Ok(targets) = Json::parse_string(&payload).try_to::<VarArray>() else {
            return false;
        };
        targets.iter_shared().any(|row| {
            row.try_to::<VarDictionary>()
                .is_ok_and(|row| dict_int(&row, "id", -1) == first_id)
        })
    }

    /// `MindUi.is_dialog_shown(name)`.
    fn dialog_shown(&self, dialog_name: &str) -> bool {
        let Some(mut ui) = self.ui.clone() else {
            return false;
        };
        if !ui.has_method("is_dialog_shown") {
            return false;
        }
        ui.call(
            "is_dialog_shown",
            &[GString::from(dialog_name).to_variant()],
        )
        .try_to_relaxed::<bool>()
        .unwrap_or(false)
    }

    /// Records the camera zoom for the `zoom` completion predicate.
    fn snapshot_zoom(&mut self) {
        if let Some(zoom) = self.camera_zoom() {
            self.last_zoom = zoom;
        }
    }
}

/// Reads a `bool` from a poll dictionary with GDScript's `bool(...)` coercion.
fn dict_bool(dict: &VarDictionary, key: &str, default: bool) -> bool {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<bool>().ok())
        .unwrap_or(default)
}

/// Reads an `int` from a poll dictionary with GDScript's `int(...)` coercion.
fn dict_int(dict: &VarDictionary, key: &str, default: i64) -> i64 {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<i64>().ok())
        .unwrap_or(default)
}

/// Reads a `float` from a poll dictionary with GDScript's `float(...)` coercion.
fn dict_float(dict: &VarDictionary, key: &str, default: f64) -> f64 {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<f64>().ok())
        .unwrap_or(default)
}

/// Reads a `String` from a poll dictionary with GDScript's `str(...)` coercion.
fn dict_string(dict: &VarDictionary, key: &str, default: &str) -> GString {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<GString>().ok())
        .unwrap_or(GString::from(default))
}

/// Reads an `Array` from a poll dictionary (`[]` when absent or not an array).
fn dict_array(dict: &VarDictionary, key: &str) -> VarArray {
    dict.get(key)
        .and_then(|value| value.try_to::<VarArray>().ok())
        .unwrap_or_default()
}

/// Whether `placedBlocks` contains any of `names` (`HintsFragment.schematicSelect`
/// and `coreUpgrade` predicates).
fn placed_has(hints: &VarDictionary, names: &[&str]) -> bool {
    let placed = dict_array(hints, "placed");
    names.iter().any(|name| placed.contains(*name))
}

/// Whether the event set contains `name`.
fn has_event(hints: &VarDictionary, event_name: &str) -> bool {
    dict_array(hints, "events").contains(event_name)
}

/// The first boss entry (`boss_json.first`), or `{}`.
fn boss(hints: &VarDictionary) -> VarDictionary {
    boss_root(hints)
        .get("first")
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

/// The whole `boss_json` object (count/names/value/first).
fn boss_root(hints: &VarDictionary) -> VarDictionary {
    hints
        .get("boss")
        .and_then(|value| value.try_to::<VarDictionary>().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hint_table_matches_upstream_declaration_order() {
        // `HintsFragment.java:169-304`; `respawn` is `visibleMobile` and stays
        // out of the desktop port.
        assert_eq!(HINTS.len(), 26);
        assert_eq!(HINTS[0], "desktopMove");
        assert_eq!(HINTS[1], "zoom");
        assert_eq!(HINTS[HINTS.len() - 1], "coreIncinerate");
        assert!(!HINTS.contains(&"respawn"));
    }

    #[test]
    fn display_gate_is_8000_ms_total_playtime() {
        assert_eq!(PLAYTIME_GATE_MS, 8000);
    }

    #[test]
    fn timings_match_the_fragment() {
        assert!((HIDE_FADE_SECONDS - 0.6).abs() < f64::EPSILON);
        assert!((DISPLAY_FADE_SECONDS - 1.0).abs() < f64::EPSILON);
        assert_eq!(HIDE_SLIDE_Y, 200.0);
        assert!((ZOOM_EPSILON - 0.01).abs() < f32::EPSILON);
    }
}
