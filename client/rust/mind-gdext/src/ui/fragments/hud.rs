// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/ui/fragments/HudFragment.java (plan 14 §3.5, M4).
//
// In-game HUD. The named regions from §3.5 are declared statically in
// `hud_fragment.tscn` (tscn-first); this class binds them to the read-only
// `MindHud` surface (wave/enemy/status text, core items, toasts) and forwards
// interaction to the Rust facade. Per §3.10 rule 2 it never reads sim state
// directly.

//! `MindHudFragment` — the in-game HUD (`HudFragment`).
//!
//! 1:1 port of `client/ui/fragments/hud_fragment.gd`: the wave/status table,
//! the FPS/memory/ping box, the core-info column (items, under-attack banner,
//! boss bar), the minimap box, the paused/waiting/near-point/saving banners,
//! the mobile offsets, the unlock toast and the toast/announce/hud-text relays.
//!
//! Every value comes from the read-only `MindHud`/`MindUi`/`MindSimHost` read
//! models; the only writes are presentation (visibility, colours, label text,
//! tweens) and the camera intents the minimap widget already emits. No
//! simulation state is touched.
//!
//! Child nodes that are native widgets now (`CoreItems` → [`CoreItemsDisplay`],
//! `BossBar/Bar` → [`MindBar`], `Minimap` → [`MindMinimapWidget`]) are looked up
//! with `get_node_or_null` and cast to their Rust type, matching the former
//! `@onready` typed references.

use std::collections::{HashMap, HashSet};

use godot::builtin::Side;
use godot::classes::box_container::AlignmentMode;
use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter, SizeFlags};
use godot::classes::texture_rect::{ExpandMode, StretchMode};
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{
    Button, ButtonGroup, Control, GridContainer, HBoxContainer, IControl, InputEvent,
    InputEventMouseButton, Json, Label, LineEdit, MarginContainer, Node, PanelContainer,
    RichTextLabel, ScrollContainer, StyleBoxEmpty, StyleBoxFlat, StyleBoxTexture, TextureRect,
    Time, VBoxContainer,
};
use godot::global::{MouseButton, VerticalAlignment};
use godot::obj::{NewAlloc, NewGd, Singleton};
use godot::prelude::*;

use crate::input::desktop::WaveGate;
use crate::sim_host::MindSimHost;
use crate::ui::kit;
use crate::ui::kit::MindRichLabel;
use crate::ui::kit::icons;
use crate::ui::kit::styles;
use crate::ui::kit::widgets::bar::MindBar;
use crate::ui::kit::widgets::core_items_display::CoreItemsDisplay;
use crate::ui::kit::widgets::factory;
use crate::ui::kit::widgets::minimap::MindMinimapWidget;

/// HUD refresh cadence in seconds (change-cached labels; §3.10 rule 6).
const REFRESH_INTERVAL: f64 = 0.2;
/// `HudFragment.makeStatusTable` skip-button glyph.
const SKIP_ICON: &str = "play";
/// Logic `setHud` poll cadence in frames (`HudFragment.java:1239`).
const LOGIC_HUD_INTERVAL: i32 = 10;
/// Core-attack banner duration in ticks (`HudFragment` uses 240 ticks).
const CORE_ATTACK_TICKS: f64 = 240.0;
/// `Pal.health` (`ff341c`), the boss-bar value colour (`HudFragment.java:633-656`).
const BOSS_HEALTH_COLOR: Color = Color::from_rgba8(0xff, 0x34, 0x1c, 0xff);
/// `orange` — the left end of the core-attack label flash.
const CORE_ATTACK_ORANGE: Color = Color::from_rgba8(0xff, 0xa5, 0x00, 0xff);
/// `scarlet` — the right end of the core-attack label flash.
const CORE_ATTACK_SCARLET: Color = Color::from_rgba8(0xff, 0x24, 0x00, 0xff);
/// `HudFragment.showUnlock`: the `8 * 6` content icon cell.
const UNLOCK_ICON_SIZE: f32 = 48.0;
/// `HudFragment.showUnlock`: `col = 3`.
const UNLOCK_COLS: i32 = 3;
/// `HudFragment.showUnlock`: `cap = col * col - 1`.
const UNLOCK_CAP: i32 = UNLOCK_COLS * UNLOCK_COLS - 1;
/// `Actions.translateBy(0, -prefHeight, 1f, Interp.fade)` (in and out).
const UNLOCK_SLIDE_SECONDS: f64 = 1.0;
/// The unlock box's hold before sliding back out.
const UNLOCK_HOLD_SECONDS: f64 = 2.5;
/// `HudFragment.addInfoTable`: one 32x32 icon per applied status effect.
const STATUS_ICON_SIZE: f32 = 32.0;
/// `Logic.java:435-440`: `Time.run(60 * 3, () -> ui.restart.show(winner))`.
const GAME_OVER_DELAY: f64 = 3.0;

/// `MindHudFragment` — the in-game HUD in `HudGroup`.
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct MindHudFragment {
    base: Base<Control>,

    /// `/root/MindHud` (`_hud`), cached at ready like the `@onready` lookup.
    hud: Option<Gd<Node>>,
    /// Seconds accumulated since the last refresh (`_accum`).
    accum: f64,
    /// Whether the game-over dialog was already requested (`_game_over_shown`).
    game_over_shown: bool,
    /// `_skip_button`: the icon-only skip-wave button inside the status row.
    skip_button: Option<Gd<Button>>,
    /// `_minimap_last_zoom`: the widget's previous absolute zoom ratio.
    #[init(val = 1.0)]
    minimap_last_zoom: f64,
    /// Per-core healths keyed `"x,y"` (`_core_healths`).
    core_healths: HashMap<String, f64>,
    /// Countdown in ticks before the under-attack banner hides.
    core_attack_time: f64,
    /// Total player-core health at the previous refresh (`_core_attack_total`).
    core_attack_total: f64,
    /// Last core whose health dropped (`_last_damaged_core`).
    last_damaged_core: Vector2,
    /// Near-point banner alpha (`_near_alpha`, `WaveSpawner.playerNear` lerp).
    near_alpha: f64,
    /// Applied status names signature (`_status_names`, icon rebuild cache).
    status_names: String,
    /// First-use `ui-hidden` announce latch (`_ui_hidden_warned`).
    ui_hidden_warned: bool,
    /// Whether the 3 s game-over timer is in flight (`_game_over_pending`).
    game_over_pending: bool,
    /// Logic `setHud` poll tick (`_logic_hud_tick`).
    logic_hud_tick: i32,
    /// Logic `setHud false` latch (`_logic_hud_hidden`).
    logic_hud_hidden: bool,
    /// Visibility to restore when the logic HUD hide clears (`_logic_hud_prev_visible`).
    #[init(val = true)]
    logic_hud_prev_visible: bool,
    /// The live unlock box (`_unlock_box`, Java `lastUnlockTable`).
    unlock_box: Option<Gd<PanelContainer>>,
    /// The unlock box's icon grid (`_unlock_grid`, Java `lastUnlockLayout`).
    unlock_grid: Option<Gd<GridContainer>>,
    /// Local HUD visibility (`HudFragment.shown`, `HudFragment.java:45`).
    #[init(val = true)]
    menus_shown: bool,
    /// Whether `_refresh_loading` raised the overlay (`_io_loading_shown`).
    io_loading_shown: bool,
    /// `PlayerComp.drawPing` marker overlay (`pingX/pingY/pingTime/pingText`);
    /// created lazily so the scene stays tscn-first.
    ping_overlay: Option<Gd<MindPingOverlay>>,
    /// `HudFragment.editorMain` (`HudFragment.java:505-549`): the editor-only
    /// team-selector + block-palette panel (created lazily).
    editor_panel: Option<Gd<PanelContainer>>,
    /// `editorMain.teams` row with the `Team.baseTeams` buttons.
    editor_team_row: Option<Gd<HBoxContainer>>,
    /// The collapsible block-palette box (`t.collapser(this::addBlockSelection,
    /// () -> settings.getBool("editor-blocks-shown"))`).
    editor_palette: Option<Gd<VBoxContainer>>,
    /// `editor/search` field.
    editor_search: Option<Gd<LineEdit>>,
    /// `blockSelection` grid (`rebuildBlockSelection`).
    editor_blocks: Option<Gd<GridContainer>>,
    /// `favoriteBlocks` (`Core.settings "editor-block-favorites"`).
    editor_favorites: Vec<String>,
    /// `settings "editor-blocks-shown"` (the collapser state).
    editor_blocks_shown: bool,
    /// Last applied `state.isEditor()` gate (`editorMain.visible`).
    editor_mode: bool,
    /// Team/block buttons for the checked-state refresh.
    editor_team_buttons: Vec<Gd<Button>>,
    editor_block_buttons: Vec<(String, Gd<Button>)>,
    /// `editorMain` collapser toggle (`Icon.downOpen`/`upOpen`).
    editor_toggle: Option<Gd<Button>>,
}

#[godot_api]
impl IControl for MindHudFragment {
    fn ready(&mut self) {
        self.hud = kit::hud();
        self._localize_banners();
        self._build_wave_buttons();
        self._setup_status_style();
        self._setup_ping_overlay();
        self._setup_editor_panel();
        if let Some(hud) = self.hud.clone() {
            for (signal, method) in [
                ("hud_text", "_on_hud_text"),
                ("toast", "_on_toast"),
                ("announce", "_on_announce"),
                ("unlock", "_on_unlock"),
                ("wave_event", "_on_wave_event"),
                ("sector_event", "_on_sector_event"),
            ] {
                self.connect_node(&hud, signal, method);
            }
        }
        self._connect_minimap();
        // `DesktopInput.java:455-457`: F6 toggles the `blockstatus` setting (the
        // input layer only emits the hotkey; the HUD owns the setting write).
        if let Some(input) = kit::node("/root/Spine/Input") {
            self.connect_node(&input, "hotkey", "_on_hotkey");
        }
        if let Some(mut core_attack) = self.node_as::<PanelContainer>("CoreInfo/CoreAttack") {
            core_attack.set_visible(false);
            let callable = self.base().callable("_on_core_attack_input");
            if !core_attack.is_connected("gui_input", &callable) {
                let _ = core_attack.connect("gui_input", &callable);
            }
        }
        self.refresh();
        self.base_mut().set_process(true);
    }

    fn process(&mut self, delta: f64) {
        self._tick_banners(delta);
        self.logic_hud_tick += 1;
        if self.logic_hud_tick >= LOGIC_HUD_INTERVAL {
            self.logic_hud_tick = 0;
            self._update_logic_hud();
        }
        self.accum += delta;
        if self.accum < REFRESH_INTERVAL {
            return;
        }
        self.accum = 0.0;
        self.refresh();
    }
}

#[godot_api]
impl MindHudFragment {
    // ---- input + visibility ----------------------------------------------

    /// Creates the `PlayerComp.drawPing` marker overlay when absent (the HUD
    /// scene stays tscn-first: the overlay is a pure presentation child).
    fn _setup_ping_overlay(&mut self) {
        if let Some(current) = &self.ping_overlay
            && current.is_instance_valid()
        {
            return;
        }
        let mut overlay = MindPingOverlay::new_alloc();
        overlay.set_name("PingOverlay");
        overlay.set_anchors_preset(LayoutPreset::FULL_RECT);
        overlay.set_mouse_filter(MouseFilter::IGNORE);
        self.base_mut().add_child(&overlay);
        self.ping_overlay = Some(overlay);
    }

    /// `Call.pingLocation` -> `PlayerComp.pingX/pingY/pingTime/pingText`
    /// (`InputHandler.java:286-302`) and the world marker
    /// (`PlayerComp.drawPing`). The fullscreen-minimap copy
    /// (`MinimapRenderer.java:173`) needs the minimap widget/render hook.
    #[func]
    pub fn show_ping(&mut self, x: f32, y: f32, text: GString) {
        self._setup_ping_overlay();
        let Some(mut ui) = kit::ui() else {
            return;
        };
        // `Renderer.java:176` `showPings = settings.getBool("showpings", true)`.
        let enabled = ui
            .call(
                "settings_get_flag_default",
                &[GString::from("showpings").to_variant(), true.to_variant()],
            )
            .try_to::<bool>()
            .unwrap_or(true);
        if !enabled {
            return;
        }
        // `PlayerComp.name` (`Core.settings.getString("name")`); Java's draw
        // guard skips the marker when the name is null.
        let name = ui
            .call(
                "settings_get_text",
                &[
                    GString::from("name").to_variant(),
                    GString::new().to_variant(),
                ],
            )
            .try_to::<GString>()
            .unwrap_or_default();
        // `PlayerComp.color` = the player team's colour.
        let color = self
            .host()
            .and_then(|mut host| host.call("player_team_color", &[]).try_to::<i64>().ok())
            .map(packed_rgba_color)
            .unwrap_or(Color::WHITE);
        let Some(mut overlay) = self.ping_overlay.clone() else {
            return;
        };
        overlay.bind_mut().show_at(
            Vector2::new(x, y),
            text.to_string().as_str(),
            name.to_string().as_str(),
            color,
        );
    }

    /// `planConfig.showConfig(splan)` (`DesktopInput.java:800-812`): opens the
    /// sibling `HudGroup/plan_config` fragment with a point-config spec. The
    /// fragment's `configure` is a Rust method (not `#[func]`-exposed), so the
    /// HUD owns this sibling call.
    #[func]
    pub fn open_plan_config(&mut self, spec: GString) -> bool {
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&spec.to_string()) else {
            return false;
        };
        let Some(parent) = self.base().get_parent() else {
            return false;
        };
        let Some(node) = parent.get_node_or_null("plan_config") else {
            return false;
        };
        let Ok(mut fragment) =
            node.try_cast::<crate::ui::fragments::plan_config::MindPlanConfigFragment>()
        else {
            return false;
        };
        let mut config = VarDictionary::new();
        let title = parsed
            .get("title")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        config.set("title", &kit::t(title).to_variant());
        let mut options = VarArray::new();
        if let Some(list) = parsed.get("options").and_then(serde_json::Value::as_array) {
            for option in list {
                let name = option
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let label_key = option
                    .get("label")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(name);
                let mut row = VarDictionary::new();
                row.set("name", &GString::from(name).to_variant());
                row.set("label", &kit::t(label_key).to_variant());
                options.push(&row.to_variant());
            }
        }
        config.set("options", &options.to_variant());
        if let Some(value) = parsed.get("value").and_then(serde_json::Value::as_str) {
            config.set("value", &GString::from(value).to_variant());
        }
        fragment.bind_mut().configure(config);
        true
    }

    /// `planConfig.hide()` (`DesktopInput.java:809/812`).
    #[func]
    pub fn hide_plan_config(&mut self) {
        let Some(parent) = self.base().get_parent() else {
            return;
        };
        if let Some(mut node) = parent.get_node_or_null("plan_config") {
            node.set("visible", &false.to_variant());
        }
    }

    // ---- editor HUD (GAPS #42, PAR-0556) ---------------------------------

    /// `HudFragment.editorMain.table(Tex.buttonEdge4, ...)`
    /// (`HudFragment.java:505-549`): the `Team.baseTeams` selector (45 px,
    /// team-coloured) and the collapsible block palette with `editor/search`
    /// and right-click favourites. Built in code because the HUD scene is
    /// tscn-first and the panel is editor-only.
    fn _setup_editor_panel(&mut self) {
        if let Some(current) = &self.editor_panel
            && current.is_instance_valid()
        {
            return;
        }
        let mut panel = PanelContainer::new_alloc();
        panel.set_name("EditorPanel");
        panel.set_theme_type_variation("defaultPane");
        panel.set_visible(false);
        panel.set_anchor(Side::LEFT, 1.0);
        panel.set_anchor(Side::RIGHT, 1.0);
        panel.set_offset(Side::LEFT, -250.0);
        panel.set_offset(Side::RIGHT, -10.0);
        panel.set_offset(Side::TOP, 64.0);
        let mut column = VBoxContainer::new_alloc();
        column.set_name("Column");
        panel.add_child(&column);
        let mut teams = HBoxContainer::new_alloc();
        teams.set_name("Teams");
        column.add_child(&teams);
        self.editor_team_row = Some(teams);
        // `teams.button(Icon.downOpen, ..., () -> settings.put(
        // "editor-blocks-shown", !...))`.
        let mut toggle = factory::glyph_button("downOpen", "v", "clearTogglei");
        toggle.set_name("ToggleBlocks");
        toggle.set_tooltip_text(&kit::t("editor-blocks"));
        toggle.set_custom_minimum_size(Vector2::new(45.0, 45.0));
        let callable = self.base().callable("_on_editor_blocks_toggle");
        let _ = toggle.connect("pressed", &callable);
        if let Some(mut row) = self.editor_team_row.clone() {
            row.add_child(&toggle);
        }
        self.editor_toggle = Some(toggle);
        let mut palette = VBoxContainer::new_alloc();
        palette.set_name("Palette");
        column.add_child(&palette);
        let mut search = LineEdit::new_alloc();
        search.set_name("Search");
        search.set_placeholder(&kit::t("players.search"));
        let callable = self.base().callable("_on_editor_search_changed");
        let _ = search.connect("text_changed", &callable);
        palette.add_child(&search);
        let mut scroll = ScrollContainer::new_alloc();
        scroll.set_name("Scroll");
        scroll.set_custom_minimum_size(Vector2::new(0.0, 320.0));
        palette.add_child(&scroll);
        let mut blocks = GridContainer::new_alloc();
        blocks.set_name("Blocks");
        blocks.set_columns(5);
        scroll.add_child(&blocks);
        self.editor_palette = Some(palette);
        self.editor_search = Some(search);
        self.editor_blocks = Some(blocks);
        self.base_mut().add_child(&panel);
        self.editor_panel = Some(panel);
        self._build_editor_teams();
        self.load_editor_favorites();
        self.editor_blocks_shown = self._editor_setting_bool("editor-blocks-shown", false);
        self._apply_editor_collapse();
    }

    /// `Team.baseTeams` buttons: `Call.setPlayerTeamEditor(player, team)` on
    /// press, checked while `player.team() == team`.
    fn _build_editor_teams(&mut self) {
        let Some(mut row) = self.editor_team_row.clone() else {
            return;
        };
        let group = ButtonGroup::new_gd();
        for team in mind_core::game::team::Team::base_teams() {
            let mut button = Button::new_alloc();
            button.set_name(&format!("team-{}", team.name));
            button.set_toggle_mode(true);
            button.set_button_group(&group);
            button.set_custom_minimum_size(Vector2::new(45.0, 45.0));
            // `button.getStyle().imageUpColor = team.color` (`Tex.whiteui`
            // tinted by the team colour).
            button.set_self_modulate(Color::from_rgba(
                team.color.r,
                team.color.g,
                team.color.b,
                1.0,
            ));
            button.set_tooltip_text(&kit::t(&team.bundle_key()));
            let mut arguments = Array::<Variant>::new();
            arguments.push(&i64::from(team.id).to_variant());
            let callable = self.base().callable("_on_editor_team").bindv(&arguments);
            let _ = button.connect("pressed", &callable);
            row.add_child(&button);
            self.editor_team_buttons.push(button);
        }
    }

    /// `editorMain` visibility (`shown() && state.isEditor()`) plus the team
    /// and block checked states (`button.update(() -> setChecked(...))`).
    fn _refresh_editor_panel(&mut self) {
        let editor = self._editor_mode();
        if editor != self.editor_mode {
            self.editor_mode = editor;
            if editor {
                self.load_editor_favorites();
                self.editor_blocks_shown = self._editor_setting_bool("editor-blocks-shown", false);
                self._apply_editor_collapse();
                self._rebuild_editor_blocks();
            }
        }
        let Some(mut panel) = self.editor_panel.clone() else {
            return;
        };
        panel.set_visible(editor && self.menus_shown);
        if !editor {
            return;
        }
        self._refresh_editor_teams();
        let selected = kit::sim_host()
            .and_then(|node| node.try_cast::<MindSimHost>().ok())
            .map(|host| host.bind().selected_block().to_string())
            .unwrap_or_default();
        for (name, button) in self.editor_block_buttons.iter_mut() {
            let pressed = *name == selected;
            if button.is_pressed() != pressed {
                button.set_pressed(pressed);
            }
        }
    }

    /// Checked team button (`button.update` in `HudFragment.java:511-517`).
    fn _refresh_editor_teams(&mut self) {
        let current = kit::sim_host()
            .and_then(|node| node.try_cast::<MindSimHost>().ok())
            .map(|host| host.bind().player_team());
        let Some(current) = current else {
            return;
        };
        for (index, button) in self.editor_team_buttons.iter_mut().enumerate() {
            let pressed = mind_core::game::team::Team::base_teams()
                .get(index)
                .is_some_and(|team| team.id == current);
            if button.is_pressed() != pressed {
                button.set_pressed(pressed);
            }
        }
    }

    /// `Call.setPlayerTeamEditor(player, team)` (`HudFragment.java:714-718`);
    /// `MindSimHost::set_player_team` re-checks `state.isEditor()`.
    #[func]
    fn _on_editor_team(&mut self, team: i64) {
        if let Some(mut host) = kit::sim_host().and_then(|node| node.try_cast::<MindSimHost>().ok())
            && host.bind_mut().set_player_team(team)
        {
            self._refresh_editor_teams();
        }
    }

    /// `Core.settings.put("editor-blocks-shown", ...)` collapser toggle.
    #[func]
    fn _on_editor_blocks_toggle(&mut self) {
        self.editor_blocks_shown = !self.editor_blocks_shown;
        self._editor_set_bool("editor-blocks-shown", self.editor_blocks_shown);
        self._apply_editor_collapse();
    }

    /// Applies the collapser state (`t.collapser(...)`).
    fn _apply_editor_collapse(&mut self) {
        if let Some(mut palette) = self.editor_palette.clone() {
            palette.set_visible(self.editor_blocks_shown);
        }
        if let Some(mut toggle) = self.editor_toggle.clone() {
            let icon = if self.editor_blocks_shown {
                "upOpen"
            } else {
                "downOpen"
            };
            toggle.set_text(&icons::glyph(icon));
        }
    }

    /// `search.field("", text -> rebuildBlockSelection(blockSelection, text))`.
    #[func]
    fn _on_editor_search_changed(&mut self, _text: GString) {
        self._rebuild_editor_blocks();
    }

    /// Palette cell click: `control.input.block = block`.
    #[func]
    fn _on_editor_block(&mut self, name: GString) {
        if let Some(mut input) = kit::node("/root/Spine/Input") {
            let _ = input.call("select_block_by_name", &[name.to_variant()]);
        }
    }

    /// `button.clicked(KeyCode.mouseRight, () -> toggleFavorite(block))`
    /// (`HudFragment.java:666-680`).
    #[func]
    fn _on_editor_block_input(&mut self, event: Gd<InputEvent>, name: GString) {
        let Ok(button) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !button.is_pressed() || button.get_button_index() != MouseButton::RIGHT {
            return;
        }
        self._toggle_editor_favorite(name.to_string().as_str());
    }

    /// `HudFragment.toggleFavorite` (`HudFragment.java:686-698`): flip the
    /// persisted `editor-block-favorites` entry and rebuild the grid.
    fn _toggle_editor_favorite(&mut self, name: &str) {
        if let Some(index) = self.editor_favorites.iter().position(|entry| entry == name) {
            self.editor_favorites.remove(index);
        } else {
            self.editor_favorites.push(name.to_owned());
        }
        self.save_editor_favorites();
        self._play_editor_sound();
        self._rebuild_editor_blocks();
    }

    /// `Sounds.uiFavorite.play(...)` (`HudFragment.java:691-697`).
    fn _play_editor_sound(&self) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("play_ui_sound", &[GString::from("uiFavorite").to_variant()]);
        }
    }

    /// `rebuildBlockSelection` (`HudFragment.java:700-776`): favourites first,
    /// then `removeOre`/`removeWall`, carrying the search filter.
    fn _rebuild_editor_blocks(&mut self) {
        let Some(mut grid) = self.editor_blocks.clone() else {
            return;
        };
        for child in grid.get_children().iter_shared() {
            let mut child = child.clone();
            child.queue_free();
        }
        self.editor_block_buttons.clear();
        let query = self
            .editor_search
            .as_ref()
            .map(|search| search.get_text().to_string().to_lowercase())
            .unwrap_or_default();
        let selected = kit::sim_host()
            .and_then(|node| node.try_cast::<MindSimHost>().ok())
            .map(|host| host.bind().selected_block().to_string())
            .unwrap_or_default();
        let group = ButtonGroup::new_gd();
        for (name, label_key) in self._editor_catalog() {
            let label = kit::t(&label_key);
            if !query.is_empty() {
                let name_lower = name.to_lowercase();
                let label_lower = label.to_string().to_lowercase();
                if !name_lower.contains(&query) && !label_lower.contains(&query) {
                    continue;
                }
            }
            let regions = [
                format!("block-{name}-ui"),
                format!("block-{name}-full"),
                name.clone(),
            ];
            let region_refs: Vec<&str> = regions.iter().map(String::as_str).collect();
            let mut button = factory::image_button_first(&region_refs, "selecti");
            button.set_name(&format!("editor-block-{name}"));
            button.set_tooltip_text(&label);
            button.set_toggle_mode(true);
            button.set_button_group(&group);
            button.set_custom_minimum_size(Vector2::new(40.0, 40.0));
            button.set_pressed(name == selected);
            button.set_expand_icon(true);
            button.add_theme_constant_override("icon_max_width", 28);
            if button.get_button_icon().is_none() {
                button.set_text(&label);
            }
            let mut arguments = Array::<Variant>::new();
            arguments.push(&GString::from(name.as_str()).to_variant());
            let callable = self.base().callable("_on_editor_block").bindv(&arguments);
            let _ = button.connect("pressed", &callable);
            let mut arguments = Array::<Variant>::new();
            arguments.push(&GString::from(name.as_str()).to_variant());
            let callable = self
                .base()
                .callable("_on_editor_block_input")
                .bindv(&arguments);
            let _ = button.connect("gui_input", &callable);
            grid.add_child(&button);
            self.editor_block_buttons.push((name, button));
        }
    }

    /// The editor palette list (`HudFragment.rebuildBlockSelection`'s filter):
    /// `inEditor` content plus `removeWall`/`removeOre`, minus `debugOnly` and
    /// other-planet blocks, favourites first.
    fn _editor_catalog(&self) -> Vec<(String, String)> {
        let Some(host) = kit::sim_host().and_then(|node| node.try_cast::<MindSimHost>().ok())
        else {
            return Vec::new();
        };
        let guard = host.bind();
        let Some(content) = guard.content_registry() else {
            return Vec::new();
        };
        let rules = guard.live_rules().unwrap_or_default();
        let planet = content.planet_id(&rules.planet);
        let favorites: HashSet<&str> = self.editor_favorites.iter().map(String::as_str).collect();
        let mut rows: Vec<(u8, String, String)> = Vec::new();
        for def in content.blocks() {
            // `!block.inEditor && !(block instanceof RemoveWall) && !(RemoveOre)`.
            let remove = matches!(
                def.kind,
                mind_core::content::BlockKind::RemoveWall
                    | mind_core::content::BlockKind::RemoveOre
            );
            if !def.in_editor && !remove {
                continue;
            }
            // `block.buildVisibility == BuildVisibility.debugOnly`.
            if def.build_visibility == mind_core::content::BuildVisibility::DebugOnly {
                continue;
            }
            // `!block.isOnPlanet(state.rules.planet)`.
            if let Some(planet) = planet
                && !def.unlock.shown_planets.is_empty()
                && !def.unlock.shown_planets.contains(&planet)
            {
                continue;
            }
            let rank = if remove {
                0
            } else if favorites.contains(def.name.as_str()) {
                1
            } else {
                2
            };
            rows.push((rank, def.name.clone(), format!("block.{}.name", def.name)));
        }
        // Stable sort keeps content order inside each rank.
        rows.sort_by_key(|(rank, _, _)| *rank);
        rows.into_iter()
            .map(|(_, name, label)| (name, label))
            .collect()
    }

    /// `state.isEditor()` (`Rules.editor`).
    fn _editor_mode(&self) -> bool {
        kit::sim_host()
            .and_then(|node| node.try_cast::<MindSimHost>().ok())
            .is_some_and(|host| host.bind().live_rules().is_some_and(|rules| rules.editor))
    }

    /// `Core.settings.getBool(key)` with a Java-style default.
    fn _editor_setting_bool(&self, key: &str, default: bool) -> bool {
        kit::ui()
            .and_then(|mut ui| {
                ui.call(
                    "settings_get_flag_default",
                    &[GString::from(key).to_variant(), default.to_variant()],
                )
                .try_to::<bool>()
                .ok()
            })
            .unwrap_or(default)
    }

    /// `Core.settings.put(key, value)`.
    fn _editor_set_bool(&self, key: &str, value: bool) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call(
                "settings_put_flag",
                &[GString::from(key).to_variant(), value.to_variant()],
            );
        }
    }

    /// `HudFragment.loadFavorites` (`HudFragment.java:113-116`):
    /// `Core.settings.getJson("editor-block-favorites", ObjectSet.class,
    /// String.class, ObjectSet::new)`.
    fn load_editor_favorites(&mut self) {
        self.editor_favorites.clear();
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let Ok(json) = ui
            .call(
                "settings_get_text",
                &[
                    GString::from("editor-block-favorites").to_variant(),
                    GString::new().to_variant(),
                ],
            )
            .try_to::<GString>()
        else {
            return;
        };
        let Ok(serde_json::Value::Array(entries)) =
            serde_json::from_str::<serde_json::Value>(&json.to_string())
        else {
            return;
        };
        for entry in entries {
            if let Some(name) = entry.as_str() {
                self.editor_favorites.push(name.to_owned());
            }
        }
    }

    /// `HudFragment.saveFavorites` (`HudFragment.java:118-120`).
    fn save_editor_favorites(&self) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        let json = serde_json::to_string(&self.editor_favorites).unwrap_or_default();
        let _ = ui.call(
            "settings_put_text",
            &[
                GString::from("editor-block-favorites").to_variant(),
                GString::from(json.as_str()).to_variant(),
            ],
        );
    }

    /// View-toggle hotkeys owned by the HUD fragment (`Renderer.drawStatus`).
    #[func]
    fn _on_hotkey(&mut self, action: GString) {
        match action.to_string().as_str() {
            "toggle_block_status" => {
                let Some(mut ui) = kit::ui() else {
                    return;
                };
                if !ui.has_method("settings_get_bool") {
                    return;
                }
                let current = ui
                    .call(
                        "settings_get_bool",
                        &[GString::from("blockstatus").to_variant()],
                    )
                    .try_to_relaxed::<bool>()
                    .unwrap_or(false);
                let _ = ui.call(
                    "settings_set",
                    &[
                        GString::from("blockstatus").to_variant(),
                        (!current).to_variant(),
                    ],
                );
            }
            "toggle_menus" => self._toggle_menus(),
            _ => {}
        }
    }

    /// `HudFragment.toggleMenus` (`HudFragment.java:875-877` + `:455-461`): flip
    /// every HUD table; the first press announces the bind once
    /// (`Settings.getBoolOnce("ui-hidden")`).
    #[func]
    fn _toggle_menus(&mut self) {
        self.menus_shown = !self.menus_shown;
        if let Some(mut ui) = kit::ui()
            && !self.ui_hidden_warned
        {
            self.ui_hidden_warned = true;
            if ui.has_method("announce") {
                let key_display = self._bind_display(GString::from("toggle_menus"));
                let format = kit::t("showui").to_string();
                let text = format.replace("{0}", key_display.to_string().as_str());
                let _ = ui.call(
                    "announce",
                    &[
                        GString::from(text.as_str()).to_variant(),
                        5.0f32.to_variant(),
                    ],
                );
            }
        }
        // The tables are children of this fragment; `placement` is a sibling
        // fragment under the HUD group.
        for path in ["WavesStack", "CoreInfo", "MinimapBox", "FpsBox"] {
            if let Some(node) = self.base().get_node_or_null(path)
                && let Ok(mut control) = node.try_cast::<Control>()
            {
                control.set_visible(self.menus_shown);
            }
        }
        if let Some(hud) = kit::node("/root/Spine/Ui/UiRoot/HudGroup")
            && let Some(placement) = hud.get_node_or_null("placement")
            && let Ok(mut placement) = placement.try_cast::<Control>()
        {
            placement.set_visible(self.menus_shown);
        }
    }

    /// Display name of one binding (`keybinds_json`), for the `showui` announce.
    #[func]
    fn _bind_display(&self, action: GString) -> GString {
        let Some(mut input) = kit::node("/root/Spine/Input") else {
            return GString::from("C");
        };
        if !input.has_method("keybinds_json") {
            return GString::from("C");
        }
        let Ok(payload) = input.call("keybinds_json", &[]).try_to::<GString>() else {
            return GString::from("C");
        };
        let Ok(entries) = Json::parse_string(&payload).try_to::<VarArray>() else {
            return GString::from("C");
        };
        let wanted = action.to_string();
        for entry in entries.iter_shared() {
            let Ok(row) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            let name = row
                .get("name")
                .map(|value| format!("{value}"))
                .unwrap_or_default();
            if name != wanted {
                continue;
            }
            return row
                .get("display")
                .map(|value| GString::from(format!("{value}").as_str()))
                .unwrap_or_else(|| GString::from("C"));
        }
        GString::from("C")
    }

    // ---- signal wiring ----------------------------------------------------

    /// `HudFragment.java:336-351`: the embedded minimap pans/zooms the camera
    /// and left-tap opens the fullscreen fragment.
    #[func]
    fn _connect_minimap(&mut self) {
        let Some(minimap) = self.minimap_widget() else {
            return;
        };
        let node: Gd<Node> = minimap.upcast();
        self.connect_node(&node, "pan_requested", "_on_minimap_pan");
        self.connect_node(&node, "zoom_changed", "_on_minimap_zoom");
        self.connect_node(&node, "tapped", "_on_minimap_tapped");
    }

    /// `HudFragment.makeStatusTable`: the status table draws on `Tex.wavepane`
    /// (36x27 ninepatch, splits 12). Built from the atlas region so the
    /// objective panel matches the Java top-left readout.
    #[func]
    fn _setup_status_style(&mut self) {
        let Some(mut status_row) = self.node_as::<PanelContainer>("WavesStack/WavesMain/StatusRow")
        else {
            return;
        };
        let Some(texture) = kit::icon_texture("wavepane") else {
            return;
        };
        let mut style = StyleBoxTexture::new_gd();
        style.set_texture(&texture);
        for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
            style.set_texture_margin(side, 12.0);
        }
        // `table.marginTop(0).marginBottom(4).marginLeft(4)`; the 120x80 player
        // portrait cell sits inside, the status label to its right.
        style.set_content_margin(Side::LEFT, 4.0);
        style.set_content_margin(Side::TOP, 0.0);
        style.set_content_margin(Side::RIGHT, 8.0);
        style.set_content_margin(Side::BOTTOM, 4.0);
        status_row.add_theme_stylebox_override("panel", &style);
        if let Some(mut status_label) =
            self.node_as::<RichTextLabel>("WavesStack/WavesMain/StatusRow/Row/Status")
        {
            let empty = StyleBoxEmpty::new_gd();
            status_label.add_theme_stylebox_override("normal", &empty);
        }
    }

    /// `HudFragment.shown() = shown && !logicHideHud` (`HudFragment.java:1239`):
    /// a world-processor `setHud false` hides the whole HUD until `setHud true`.
    /// Reads are 10-frame throttled; the pre-hide visibility is restored, and
    /// the property is only touched when the flag actually flips.
    #[func]
    fn _update_logic_hud(&mut self) {
        let Some(mut host) = self.host() else {
            return;
        };
        if !host.has_method("logic_cutscene_json") {
            return;
        }
        let Ok(payload) = host.call("logic_cutscene_json", &[]).try_to::<GString>() else {
            return;
        };
        let Ok(parsed) = Json::parse_string(&payload).try_to::<VarDictionary>() else {
            return;
        };
        let should_hide = dict_bool(&parsed, "hideHud");
        if should_hide == self.logic_hud_hidden {
            return;
        }
        self.logic_hud_hidden = should_hide;
        if should_hide {
            self.logic_hud_prev_visible = self.base().is_visible();
            self.base_mut().set_visible(false);
        } else {
            let restore = self.logic_hud_prev_visible;
            self.base_mut().set_visible(restore);
        }
    }

    /// Per-frame banner timers (core-attack countdown, near-point fade; Java
    /// runs both outside the 5 Hz table refresh).
    #[func]
    fn _tick_banners(&mut self, delta: f64) {
        let paused = self.hud_bool("paused");
        if self.core_attack_time > 0.0 && !paused {
            self.core_attack_time = (self.core_attack_time - delta * 60.0).max(0.0);
        }
        if let Some(mut core_attack) = self.node_as::<PanelContainer>("CoreInfo/CoreAttack") {
            core_attack.set_visible(self.core_attack_time > 0.0);
            if self.core_attack_time > 0.0
                && let Some(mut label) = self.node_as::<Label>("CoreInfo/CoreAttack/Label")
            {
                // `orange.lerp(scarlet, absin(Time.time, 2, 1))` (`Java absin`).
                let ticks = Time::singleton().get_ticks_msec() as f64 * 0.06;
                let weight = ((ticks / 4.0).sin() + 1.0) * 0.5;
                let color = CORE_ATTACK_ORANGE.lerp(CORE_ATTACK_SCARLET, weight);
                label.add_theme_color_override("font_color", color);
            }
        }
        // `table.alpha` lerp toward `playerNear() ? 1 : 0` by 0.1/tick.
        let near = match self.host() {
            Some(mut host) if host.has_method("player_near_drop_zone") => host
                .call("player_near_drop_zone", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false),
            _ => false,
        };
        let target = if near { 1.0 } else { 0.0 };
        self.near_alpha += (target - self.near_alpha) * (1.0 - 0.9_f64.powf(delta * 60.0));
        if let Some(mut near_point) = self.node_as::<PanelContainer>("NearPoint") {
            near_point.set_visible(self.near_alpha > 0.005);
            let mut modulate = near_point.get_modulate();
            modulate.a = self.near_alpha as f32;
            near_point.set_modulate(modulate);
        }
    }

    /// Banner texts from the bundle (`@paused`/`@waiting.players`), replacing
    /// the hardcoded English defaults in the scene.
    #[func]
    fn _localize_banners(&mut self) {
        if let Some(mut banner) = self.node_as::<RichTextLabel>("PausedBanner") {
            banner.set_text(&format!("[center]{}", kit::tm("@paused")));
        }
        if let Some(mut banner) = self.node_as::<RichTextLabel>("WaitingBanner") {
            banner.set_text(&format!("[center]{}", kit::tm("@waiting.players")));
        }
        if let Some(mut label) = self.node_as::<Label>("CoreInfo/CoreAttack/Label") {
            label.set_text(&kit::tm("@coreattack"));
        }
        if let Some(mut label) = self.node_as::<RichTextLabel>("NearPoint/Label") {
            label.set_text(&format!("[center]{}", kit::tm("@nearpoint")));
        }
        if let Some(mut label) = self.node_as::<RichTextLabel>("Saving") {
            label.set_text(&format!("[center]{}", kit::tm("@saving")));
        }
    }

    /// `HudFragment.java:491-510`: the icon-only skip-wave button lives inside
    /// the status table's row, right side, 40 px wide with `growY`
    /// (`s.button(Icon.play, rightStyle, 30f, ...).growY().fillX().right().width(40f)`).
    #[func]
    fn _build_wave_buttons(&mut self) {
        if self.skip_button.is_some() {
            return;
        }
        let Some(mut row) = self.node_as::<HBoxContainer>("WavesStack/WavesMain/StatusRow/Row")
        else {
            return;
        };
        // code-instantiated: the button's icon-font glyph is resolved through
        // `kit::icon_texture`/`kit::glyph` and has no scene form.
        let mut skip = factory::icon_button(SKIP_ICON, "", "defaultt", AlignmentMode::BEGIN);
        skip.set_name("skip");
        // `rightStyle` (HudFragment.java:493-500): `up = buttonRight`, `over =
        // buttonRightOver`, `down = buttonRightDown`, `disabled =
        // buttonRightDisabled` — the 9-patch with the bevelled bottom-right
        // corner, not the generic `defaultt` pill.
        for (state, region) in [
            ("normal", "button-right"),
            ("hover", "button-right-over"),
            ("pressed", "button-right-down"),
            ("hover_pressed", "button-right-down"),
            ("disabled", "button-right-disabled"),
        ] {
            let Some(texture) = kit::icon_texture(region) else {
                continue;
            };
            let mut style = StyleBoxTexture::new_gd();
            style.set_texture(&texture);
            for side in [Side::LEFT, Side::TOP, Side::RIGHT, Side::BOTTOM] {
                style.set_texture_margin(side, 12.0);
                style.set_content_margin(side, 0.0);
            }
            skip.add_theme_stylebox_override(state, &style);
        }
        let focus = StyleBoxEmpty::new_gd();
        skip.add_theme_stylebox_override("focus", &focus);
        skip.set_custom_minimum_size(Vector2::new(40.0, 0.0));
        skip.set_v_size_flags(SizeFlags::FILL);
        let callable = self.base().callable("_on_skip_wave");
        let _ = skip.connect("pressed", &callable);
        row.add_child(&skip);
        self.skip_button = Some(skip);
    }

    // ---- refresh ----------------------------------------------------------

    /// Pulls the typed HUD properties and drives the widgets that are not
    /// simple label binds (core items, skip button, game-over watcher).
    #[func]
    fn refresh(&mut self) {
        if !self.has_hud() {
            self._set_placeholder();
            return;
        }
        // `HudFragment.showUnlock`'s `table.update`: entering the menu drops the
        // live unlock box (`lastUnlockTable/lastUnlockLayout = null`).
        if let Some(current) = self.unlock_box.clone()
            && current.is_instance_valid()
            && self._in_menu()
        {
            let mut current = current;
            current.queue_free();
            self.unlock_box = None;
            self.unlock_grid = None;
        }
        let status = self.hud_string("status_text");
        if let Some(mut label) =
            self.node_as::<RichTextLabel>("WavesStack/WavesMain/StatusRow/Row/Status")
        {
            // `table.labelWrap(...)` (HudFragment.java:1066): an Arc `Label`
            // with wrap on and the default left alignment.
            label.set_text(&status);
            label.set_visible(!status.is_empty());
        }
        if let Some(mut info) = self.node_as::<RichTextLabel>("WavesStack/WavesMain/Info") {
            info.set_visible(false);
        }
        if let Some(mut fps) = self.node_as::<Label>("FpsBox/Fps") {
            fps.set_text(&format!("FPS: {}", self.hud_i64("fps")));
        }
        if let Some(mut memory) = self.node_as::<Label>("FpsBox/Memory") {
            memory.set_text(&format!("{} MB", self.hud_i64("memory_mb")));
        }
        if let Some(mut ping) = self.node_as::<Label>("FpsBox/Ping") {
            ping.set_text(&format!("{} ms", self.hud_i64("ping")));
        }
        if let Some(mut banner) = self.node_as::<RichTextLabel>("PausedBanner") {
            banner.set_visible(self.hud_bool("paused"));
            let mut key = self.hud_string("paused_text");
            if key.is_empty() {
                key = GString::from("@paused");
            }
            banner.set_text(&format!("[center]{}", kit::tm(&key.to_string())));
        }
        if let Some(mut banner) = self.node_as::<RichTextLabel>("WaitingBanner") {
            banner.set_visible(self.hud_bool("waiting"));
        }
        if let Some(mut label) = self.node_as::<RichTextLabel>("MinimapBox/Position") {
            let position_text = self.hud_string("position_text");
            label.set_text(&format!("[center]{}", position_text));
            label.set_visible(!position_text.is_empty());
        }
        self._refresh_core_items();
        self._refresh_skip_button();
        self._refresh_boss();
        self._refresh_core_attack();
        self._refresh_status_icons();
        self._refresh_saving();
        self._refresh_loading();
        self._check_game_over();
        self._refresh_editor_panel();
    }

    /// No `MindHud` (bare test host): clear the state-driven banners.
    #[func]
    fn _set_placeholder(&mut self) {
        if let Some(mut banner) = self.node_as::<RichTextLabel>("PausedBanner") {
            banner.set_visible(false);
        }
        if let Some(mut banner) = self.node_as::<RichTextLabel>("WaitingBanner") {
            banner.set_visible(false);
        }
        if let Some(mut skip) = self.skip_button.clone() {
            skip.set_visible(false);
        }
    }

    /// `HudFragment.canSkipWave` (client half): waves enabled, wave sending on,
    /// no enemies left, not mid-spawn (`state.rules.waveSending` is false in the
    /// tutorial map, so the skip stays hidden there - PAR-0030). The same
    /// `WaveGate` backs the `skip_wave` binding (`input/desktop.rs`).
    #[func]
    fn _refresh_skip_button(&mut self) {
        let Some(mut skip) = self.skip_button.clone() else {
            return;
        };
        let gate = WaveGate {
            waves: self.hud_bool("waves"),
            wave_sending: self.hud_bool("wave_sending"),
            is_spawning: self.hud_bool("is_spawning"),
            enemies: self.hud_i64("enemies"),
            has_core: self.hud_bool("has_core"),
        };
        skip.set_visible(gate.can_skip_wave());
    }

    /// `Logic.skipWave` via the existing campaign facade (`run_wave`).
    #[func]
    fn _on_skip_wave(&mut self) {
        if let Some(mut campaign) = kit::campaign()
            && campaign.has_method("run_wave")
        {
            let _ = campaign.call("run_wave", &[]);
        }
        self.refresh();
    }

    /// `HudFragment` core-info table (`HudFragment.java:579-596`): anchored
    /// top-centre; a `pauseHeight` (36) spacer drops it below the paused bar
    /// while paused (and not waiting for players); the core-items table fed from
    /// the live sim core inventory sits first.
    #[func]
    fn _refresh_core_items(&mut self) {
        let Some(mut core_items) = self.core_items() else {
            return;
        };
        if !self.has_hud() {
            return;
        }
        if let Some(mut spacer) = self.node_as::<Control>("CoreInfo/PauseSpacer") {
            spacer.set_visible(self.hud_bool("paused") && !self.hud_bool("waiting"));
        }
        // `HudFragment.java:594`: the table is gated on
        // `settings.getBool("coreitems")`.
        let mut enabled = true;
        if let Some(mut ui) = kit::ui()
            && ui.has_method("settings_get_bool")
        {
            enabled = ui
                .call(
                    "settings_get_bool",
                    &[GString::from("coreitems").to_variant()],
                )
                .try_to_relaxed::<bool>()
                .unwrap_or(false);
        }
        if let Some(mut hud) = self.hud_node()
            && let Ok(payload) = hud.call("core_items_json", &[]).try_to::<GString>()
            && let Ok(items) = Json::parse_string(&payload).try_to::<VarDictionary>()
        {
            core_items.bind_mut().update_items(items);
        }
        if let Some(mut panel) = self.node_as::<PanelContainer>("CoreInfo/CoreItemsPanel") {
            let has_items = core_items.bind().has_items();
            panel.set_visible(enabled && has_items);
            // `collapser(coreItems).fillX()`: the column is as wide as its
            // widest row, and Arc's `Collapser.getPrefWidth` counts the
            // collapsed "core under attack" banner, so the bar spans at least
            // that width.
            let mut width = 0.0f32;
            if let Some(core_attack) = self.node_as::<PanelContainer>("CoreInfo/CoreAttack") {
                width = core_attack.get_combined_minimum_size().x;
            }
            let mut minimum = panel.get_custom_minimum_size();
            minimum.x = width;
            panel.set_custom_minimum_size(minimum);
        }
    }

    /// `LoadingFragment` driver: shows the overlay while the Rust IO queue has
    /// pending save/load work (`MindSimHost.io_pending`) and hides it after.
    #[func]
    fn _refresh_loading(&mut self) {
        let Some(mut host) = self.host() else {
            return;
        };
        if !host.has_method("io_pending") {
            return;
        }
        let pending = host
            .call("io_pending", &[])
            .try_to_relaxed::<i64>()
            .unwrap_or(0);
        let saving = host.has_method("io_saving")
            && host
                .call("io_saving", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false);
        let Some(mut loading) = kit::node("/root/Spine/Ui/UiRoot/LoadingLayer/loading") else {
            return;
        };
        let shown = loading.has_method("is_shown")
            && loading
                .call("is_shown", &[])
                .try_to_relaxed::<bool>()
                .unwrap_or(false);
        // Saves show the small bottom `@saving` label, loads the full overlay
        // (`HudFragment.java:668-673`; previously any IO covered the screen).
        if pending > 0 && !saving && !shown {
            let _ = loading.call(
                "set_progress",
                &[0.0f32.to_variant(), GString::new().to_variant()],
            );
            self.io_loading_shown = true;
        } else if (pending == 0 || saving) && shown && self.io_loading_shown {
            // Only the overlay this driver raised: `ui.loadAnd` (editor generate)
            // owns its own show/hide.
            let _ = loading.call("hide_loading", &[]);
            self.io_loading_shown = false;
        } else if !shown {
            self.io_loading_shown = false;
        }
    }

    /// Opens the game-over dialog on the campaign loss (`gameOver`) or sector
    /// capture edge; the dialog owns the continue/menu routing.
    #[func]
    fn _check_game_over(&mut self) {
        if !self.has_hud() {
            return;
        }
        // T16: `Logic.java:435-440` opens the dialog 3 s after `GameOverEvent`
        // (`Time.run(60 * 3, () -> ui.restart.show(winner))`); captures show the
        // `@sector.capture.current` announce instead (T9).
        if self.hud_bool("game_over") {
            if !self.game_over_shown {
                self.game_over_shown = true;
                self._schedule_game_over();
            }
        } else {
            self.game_over_shown = false;
            self.game_over_pending = false;
        }
    }

    /// 3 s timer before the restart dialog (`Time.run(60 * 3, ...)`); cancelled
    /// when `gameOver` flips back.
    #[func]
    fn _schedule_game_over(&mut self) {
        self.game_over_pending = true;
        let callable = self.base().callable("_open_game_over_now");
        let mut tree = self.base().get_tree();
        let mut timer = tree.create_timer(GAME_OVER_DELAY);
        let _ = timer.connect("timeout", &callable);
    }

    #[func]
    fn _open_game_over_now(&mut self) {
        if !self.has_hud() || !self.hud_bool("game_over") {
            self.game_over_pending = false;
            return;
        }
        self.game_over_pending = false;
        self._open_game_over(false);
    }

    #[func]
    fn _open_game_over(&mut self, _captured: bool) {
        let Some(mut ui) = kit::ui() else {
            return;
        };
        if !ui.has_method("open_dialog") {
            return;
        }
        let mut stats = VarDictionary::new();
        if let Some(mut campaign) = kit::campaign()
            && campaign.has_method("game_stats_json")
            && let Ok(payload) = campaign.call("game_stats_json", &[]).try_to::<GString>()
            && let Ok(parsed) = Json::parse_string(&payload).try_to::<VarDictionary>()
        {
            stats = parsed;
        }
        let sector = self.hud_string("sector_name");
        let mut context = VarDictionary::new();
        context.set("winner", &sector);
        context.set("campaign", self.hud_bool("campaign"));
        context.set("sector", &sector);
        context.set("stats", &stats);
        let json = Json::stringify(&context.to_variant());
        let _ = ui.call(
            "open_dialog",
            &[GString::from("restart").to_variant(), json.to_variant()],
        );
    }

    // ---- MindHud signals --------------------------------------------------

    #[func]
    fn _on_hud_text(&mut self, text: GString) {
        // code-instantiated: server-sent HUD text is data-driven (`setHudText`).
        if let Some(mut label) = self.node_as::<RichTextLabel>("CoreInfo/HudText") {
            label.set_text(&format!("[center]{text}"));
            label.set_visible(!text.is_empty());
        }
    }

    #[func]
    fn _on_toast(&mut self, text: GString, icon: GString) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("toast", &[text.to_variant(), icon.to_variant()]);
        }
    }

    #[func]
    fn _on_announce(&mut self, text: GString, duration: f32) {
        if let Some(mut ui) = kit::ui() {
            let _ = ui.call("announce", &[text.to_variant(), duration.to_variant()]);
        }
    }

    /// `HudFragment.showUnlock(content)` (`HudFragment.java:786-871`): a
    /// top-centre `Tex.button` box (margin 12) holding the content icon (48 px =
    /// `8 * 6`, pad 2) and `@unlocked`; slides in over 1 s (`Interp.fade`),
    /// holds 2.5 s and slides back out. Unlocks that arrive while the box lives
    /// merge into it as a 3-column icon grid (`col = 3`, cap = `col * col - 1` =
    /// 8; the 9th cell is `Icon.add`, later unlocks are dropped; cell size
    /// `48 / min(n + 1, 3)`).
    #[func]
    fn _on_unlock(&mut self, content_name: GString) {
        // `if(state.isMenu()) return;`
        if self._in_menu() {
            return;
        }
        if let Some(mut ui) = kit::ui() {
            // `Sounds.uiNotify.play()` (`UI.java:146`).
            let _ = ui.call("play_ui_sound", &[GString::from("uiNotify").to_variant()]);
        }
        if self
            .unlock_box
            .as_ref()
            .is_some_and(|box_| box_.is_instance_valid())
        {
            self._merge_unlock_icon(content_name);
            return;
        }
        self._build_unlock_box(content_name);
    }

    /// `state.isMenu()` through the UI root's menu group.
    ///
    /// `MindUiRoot` keeps its layer nodes private (no `#[var]`/`#[func]`
    /// accessor), so the port reads the same `MenuGroup` child the GDScript
    /// reached through `ui_root.menu_group.visible`.
    #[func]
    fn _in_menu(&self) -> bool {
        let Some(ui_root) = kit::node("/root/Spine/Ui/UiRoot") else {
            return false;
        };
        let Some(menu_group) = ui_root.get_node_or_null("MenuGroup") else {
            return false;
        };
        let Ok(menu_group) = menu_group.try_cast::<Control>() else {
            return false;
        };
        menu_group.is_visible()
    }

    /// First unlock: builds the box, its 48 px icon cell and the slide-in/out
    /// tween (`HudFragment.java:792-826`; the Java `scheduleToast` 3.5 s slot
    /// shared with other toasts is not ported: the port shows it at once when no
    /// box lives).
    #[func]
    fn _build_unlock_box(&mut self, content_name: GString) {
        // code-instantiated: unlock toasts are transient, high-churn overlay
        // boxes.
        let mut panel = PanelContainer::new_alloc();
        let mut style = StyleBoxFlat::new_gd();
        // `new Table(Tex.button)`: the dark gray panel with margin 12.
        style.set_bg_color(styles::GRAY_PANEL);
        style.set_corner_radius_all(4);
        style.set_content_margin_all(12.0);
        panel.add_theme_stylebox_override("panel", &style);
        panel.set_mouse_filter(MouseFilter::IGNORE);
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 8);
        // `table.add(in).padRight(8)`: the icon table becomes the merge grid.
        let mut grid = GridContainer::new_alloc();
        grid.set_columns(UNLOCK_COLS);
        let icon = self._unlock_icon(content_name, UNLOCK_ICON_SIZE);
        grid.add_child(&icon);
        row.add_child(&grid);
        let label = MindRichLabel::new_alloc();
        let mut label_rich: Gd<RichTextLabel> = label.upcast();
        label_rich.set_text(&kit::tm("@unlocked"));
        label_rich.set_vertical_alignment(VerticalAlignment::CENTER);
        row.add_child(&label_rich);
        panel.add_child(&row);
        // `container.top()`: top-centre, translated down by its own height, then
        // `Actions.translateBy(0, -table.getPrefHeight(), 1f, Interp.fade)`.
        panel.set_anchors_preset(LayoutPreset::CENTER_TOP);
        panel.set_h_grow_direction(GrowDirection::BOTH);
        let mut layer: Gd<Node> = match kit::node("/root/Spine/Ui/UiRoot/OverlayLayer") {
            Some(layer) => layer,
            None => self.to_gd().upcast(),
        };
        layer.add_child(&panel);
        let slide = -panel.get_combined_minimum_size().y;
        let position = panel.get_position();
        panel.set_position(Vector2::new(position.x, slide));
        self.unlock_box = Some(panel.clone());
        self.unlock_grid = Some(grid);
        let control: Gd<Control> = panel.clone().upcast();
        let this = self.to_gd();
        let box_for_free = panel;
        let mut tween = self.base_mut().create_tween();
        tween.set_trans(TransitionType::QUAD);
        tween.set_ease(EaseType::OUT);
        tween.tween_property(
            &control,
            "position:y",
            &0.0f32.to_variant(),
            UNLOCK_SLIDE_SECONDS,
        );
        tween.tween_interval(UNLOCK_HOLD_SECONDS);
        tween.tween_property(
            &control,
            "position:y",
            &slide.to_variant(),
            UNLOCK_SLIDE_SECONDS,
        );
        let free = Callable::from_fn("unlock_box_free", move |_args: &[&Variant]| {
            // `lastUnlockTable = null; lastUnlockLayout = null;` before removal.
            if this.is_instance_valid() {
                let mut fragment = this.clone();
                fragment.bind_mut().clear_unlock(box_for_free.clone());
            }
            Variant::nil()
        });
        tween.tween_callback(&free);
    }

    /// Merge half (`HudFragment.java:828-869`): re-flow the grid at the Java
    /// cell size and append the new icon (or the `Icon.add` placeholder at the
    /// cap).
    #[func]
    fn _merge_unlock_icon(&mut self, content_name: GString) {
        let Some(mut grid) = self.unlock_grid.clone() else {
            return;
        };
        if !grid.is_instance_valid() {
            return;
        }
        // `int cap = col * col - 1` (8); `if(esize > cap) return;`.
        let count = grid.get_child_count();
        if count > UNLOCK_CAP {
            return;
        }
        // `float size = 48f / Math.min(elements.size + 1, col)`.
        let cell = UNLOCK_ICON_SIZE / (count + 1).min(UNLOCK_COLS) as f32;
        for child in grid.get_children().iter_shared() {
            if let Ok(mut control) = child.try_cast::<Control>() {
                control.set_custom_minimum_size(Vector2::new(cell, cell));
            }
        }
        if count < UNLOCK_CAP {
            let icon = self._unlock_icon(content_name, cell);
            grid.add_child(&icon);
        } else {
            // `else lastUnlockLayout.image(Icon.add)`.
            let mut add = factory::glyph_label("add", cell as i32, Color::WHITE);
            add.set_custom_minimum_size(Vector2::new(cell, cell));
            grid.add_child(&add);
        }
    }

    /// `new Image(content.uiIcon)`: the content's `<type>-<name>-ui` atlas
    /// region (48 px cell, `Scaling.fit`), pad 2 (`in.add(image).size(8 * 6).pad(2)`);
    /// the lock glyph stands in when no region exists (`UnlockableContent.uiIcon`).
    #[func]
    fn _unlock_icon(&self, content_name: GString, icon_size: f32) -> Gd<Control> {
        // code-instantiated: unlock icons are content-driven (atlas lookup).
        let mut cell = MarginContainer::new_alloc();
        for side in ["left", "top", "right", "bottom"] {
            cell.add_theme_constant_override(format!("margin_{side}").as_str(), 2);
        }
        cell.set_custom_minimum_size(Vector2::new(icon_size, icon_size));
        cell.set_mouse_filter(MouseFilter::IGNORE);
        let name = content_name.to_string();
        let mut texture = None;
        for type_name in ["block", "item", "unit", "liquid", "status"] {
            texture = kit::icon_texture(&format!("{type_name}-{name}-ui"));
            if texture.is_some() {
                break;
            }
        }
        if let Some(texture) = texture {
            let mut image = TextureRect::new_alloc();
            image.set_texture(&texture);
            image.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            image.set_expand_mode(ExpandMode::IGNORE_SIZE);
            image.set_h_size_flags(SizeFlags::EXPAND_FILL);
            image.set_v_size_flags(SizeFlags::EXPAND_FILL);
            cell.add_child(&image);
        } else {
            let glyph =
                factory::glyph_label("lock", (icon_size - 4.0).max(8.0) as i32, Color::WHITE);
            cell.add_child(&glyph);
        }
        cell.upcast()
    }

    #[func]
    fn _on_wave_event(&mut self) {
        self.refresh();
    }

    #[func]
    fn _on_sector_event(&mut self, _kind: GString, _name: GString) {
        self.refresh();
    }

    // ---- minimap ----------------------------------------------------------

    #[func]
    fn _on_minimap_pan(&mut self, world_delta: Vector2) {
        let Some(mut camera) = self.camera() else {
            return;
        };
        if !camera.has_method("pan_to") {
            return;
        }
        let position = camera
            .get("position")
            .try_to::<Vector2>()
            .unwrap_or_default();
        let x = (position.x + world_delta.x) as f64;
        let y = (position.y + world_delta.y) as f64;
        let _ = camera.call("pan_to", &[x.to_variant(), y.to_variant()]);
    }

    /// `Renderer.scaleCamera` step from the widget's absolute zoom ratio.
    #[func]
    fn _on_minimap_zoom(&mut self, factor: f32) {
        let Some(mut camera) = self.camera() else {
            return;
        };
        if !camera.has_method("zoom_by") {
            return;
        }
        let step = (factor as f64 / self.minimap_last_zoom.max(0.0001) - 1.0) * 4.0;
        self.minimap_last_zoom = factor as f64;
        if step.abs() > 0.0001 {
            let _ = camera.call("zoom_by", &[step.to_variant()]);
        }
    }

    /// `Minimap.clicked` -> `ui.minimapfrag.toggle()`.
    #[func]
    fn _on_minimap_tapped(&mut self) {
        if let Some(mut fragment) = kit::node("/root/Spine/Ui/UiRoot/HudGroup/minimap")
            && fragment.has_method("toggle")
        {
            let _ = fragment.call("toggle", &[]);
        }
    }

    // ---- boss bar + core attack ------------------------------------------

    /// `HudFragment` boss bar (`:633-656`): visible while `rules.waves` and the
    /// wave team has bosses; text = up to 6 boss emojis + `@guardian`, colour
    /// `Pal.health` (`ff341c`), value = sum(health)/sum(max).
    #[func]
    fn _refresh_boss(&mut self) {
        let Some(mut boss_bar) = self.node_as::<MarginContainer>("CoreInfo/BossBar") else {
            return;
        };
        let Some(mut host) = self.host() else {
            boss_bar.set_visible(false);
            return;
        };
        if !host.has_method("boss_json") {
            boss_bar.set_visible(false);
            return;
        }
        let Ok(payload) = host.call("boss_json", &[]).try_to::<GString>() else {
            boss_bar.set_visible(false);
            return;
        };
        let Ok(state) = Json::parse_string(&payload).try_to::<VarDictionary>() else {
            boss_bar.set_visible(false);
            return;
        };
        let count = dict_i64(&state, "count");
        let visible = self.hud_bool("waves") && count > 0;
        boss_bar.set_visible(visible);
        if !visible {
            return;
        }
        if let Some(mut bar) = self.boss_mind_bar() {
            let prefix = if count > 6 { "[accent]+[] " } else { "" };
            let text = format!("{prefix}{}", kit::tm("@guardian"));
            let value = dict_f64(&state, "value").clamp(0.0, 1.0);
            bar.bind_mut()
                .setup(GString::from(text.as_str()), BOSS_HEALTH_COLOR, value);
        }
    }

    /// Core-attack banner (`HudFragment.java:596-631`): a health drop between
    /// refreshes shows the flashing `< Core is under attack! >` for 240 ticks;
    /// the tap pans to the damaged core.
    #[func]
    fn _refresh_core_attack(&mut self) {
        let Some(mut host) = self.host() else {
            return;
        };
        if !host.has_method("player_core_healths_json") {
            return;
        }
        let Ok(payload) = host
            .call("player_core_healths_json", &[])
            .try_to::<GString>()
        else {
            return;
        };
        let Ok(parsed) = Json::parse_string(&payload).try_to::<VarArray>() else {
            return;
        };
        let mut next: HashMap<String, f64> = HashMap::new();
        let mut total = 0.0f64;
        for entry in parsed.iter_shared() {
            let Ok(entry) = entry.try_to::<VarDictionary>() else {
                continue;
            };
            let x = dict_f64(&entry, "x");
            let y = dict_f64(&entry, "y");
            let key = format!("{},{}", godot_float_text(x), godot_float_text(y));
            let health = dict_f64(&entry, "health");
            next.insert(key.clone(), health);
            total += health;
            if let Some(previous) = self.core_healths.get(&key)
                && health < *previous - 0.001
            {
                self.last_damaged_core = Vector2::new(x as f32, y as f32);
                self.core_attack_time = CORE_ATTACK_TICKS;
            }
        }
        if total < self.core_attack_total - 0.001 && !self.core_healths.is_empty() {
            self.core_attack_time = CORE_ATTACK_TICKS;
        }
        self.core_attack_total = total;
        self.core_healths = next;
    }

    /// Tapping the core-attack banner pans the camera to the damaged core
    /// (`HudFragment.java:596-631`).
    #[func]
    fn _on_core_attack_input(&mut self, event: Gd<InputEvent>) {
        let Ok(button) = event.try_cast::<InputEventMouseButton>() else {
            return;
        };
        if !button.is_pressed() {
            return;
        }
        if button.get_button_index() != MouseButton::LEFT {
            return;
        }
        let Some(mut camera) = self.camera() else {
            return;
        };
        if !camera.has_method("pan_to") {
            return;
        }
        let target = self.last_damaged_core;
        let _ = camera.call(
            "pan_to",
            &[
                (target.x as f64).to_variant(),
                (target.y as f64).to_variant(),
            ],
        );
    }

    /// `HudFragment.addInfoTable` (`:1205-1231`): one 32x32 icon per applied
    /// status effect with its localized name.
    #[func]
    fn _refresh_status_icons(&mut self) {
        let Some(mut status_icons) =
            self.node_as::<HBoxContainer>("WavesStack/WavesMain/StatusIcons")
        else {
            return;
        };
        let Some(mut host) = self.host() else {
            return;
        };
        if !host.has_method("player_unit_status") {
            return;
        }
        let Ok(status) = host
            .call("player_unit_status", &[])
            .try_to::<VarDictionary>()
        else {
            return;
        };
        let mut list = VarArray::new();
        if let Some(value) = status.get("statuses")
            && let Ok(text) = value.try_to::<GString>()
            && let Ok(parsed) = Json::parse_string(&text).try_to::<VarArray>()
        {
            list = parsed;
        }
        let mut names: Vec<String> = Vec::new();
        for entry in list.iter_shared() {
            if let Ok(row) = entry.try_to::<VarDictionary>() {
                names.push(dict_string(&row, "name").to_string());
            }
        }
        let signature = names.join(",");
        if signature == self.status_names {
            return;
        }
        self.status_names = signature;
        for child in status_icons.get_children().iter_shared() {
            let mut child = child;
            child.queue_free();
        }
        for status_name in &names {
            if status_name.is_empty() {
                continue;
            }
            // code-instantiated: one icon per applied status effect (T9).
            let mut icon = TextureRect::new_alloc();
            icon.set_custom_minimum_size(Vector2::new(STATUS_ICON_SIZE, STATUS_ICON_SIZE));
            icon.set_expand_mode(ExpandMode::IGNORE_SIZE);
            icon.set_stretch_mode(StretchMode::KEEP_ASPECT_CENTERED);
            let mut texture = kit::icon_texture(&format!("status-{status_name}-ui"));
            if texture.is_none() {
                texture = kit::icon_texture(&format!("status-{status_name}"));
            }
            if let Some(texture) = texture {
                icon.set_texture(&texture);
            }
            icon.set_tooltip_text(&kit::tm(&format!("@status.{status_name}.name")));
            status_icons.add_child(&icon);
        }
    }

    /// `@saving` label while a save is queued (`control.saves.isSaving()`).
    #[func]
    fn _refresh_saving(&mut self) {
        let Some(mut label) = self.node_as::<RichTextLabel>("Saving") else {
            return;
        };
        let saving = self.host().is_some_and(|mut host| {
            host.has_method("io_saving")
                && host
                    .call("io_saving", &[])
                    .try_to_relaxed::<bool>()
                    .unwrap_or(false)
        });
        label.set_visible(saving);
    }
}

impl MindHudFragment {
    /// One `@onready`-style node lookup with the GDScript cast semantics.
    fn node_as<T>(&self, path: &str) -> Option<Gd<T>>
    where
        T: Inherits<Node>,
    {
        self.base().get_node_or_null(path)?.try_cast::<T>().ok()
    }

    /// `/root/MindHud` while it is alive (`_hud`).
    fn hud_node(&self) -> Option<Gd<Node>> {
        self.hud
            .as_ref()
            .filter(|hud| hud.is_instance_valid())
            .cloned()
    }

    fn has_hud(&self) -> bool {
        self.hud_node().is_some()
    }

    /// `_hud.get(key)` (a nil variant when the HUD is absent).
    fn hud_variant(&self, key: &str) -> Variant {
        match self.hud_node() {
            Some(hud) => hud.get(key),
            None => Variant::nil(),
        }
    }

    fn hud_bool(&self, key: &str) -> bool {
        self.hud_variant(key)
            .try_to_relaxed::<bool>()
            .unwrap_or(false)
    }

    fn hud_i64(&self, key: &str) -> i64 {
        self.hud_variant(key).try_to_relaxed::<i64>().unwrap_or(0)
    }

    fn hud_string(&self, key: &str) -> GString {
        self.hud_variant(key)
            .try_to_relaxed::<GString>()
            .unwrap_or_else(|_| GString::new())
    }

    /// `/root/Spine/SimHost` (`MindSimHost` read model).
    fn host(&self) -> Option<Gd<Node>> {
        kit::sim_host()
    }

    /// `/root/Spine/World/Camera2D`.
    fn camera(&self) -> Option<Gd<Node>> {
        kit::node("/root/Spine/World/Camera2D")
    }

    /// The unlock tween tail (`HudFragment.showUnlock`'s `tweenCallback`): drop
    /// the handles when the box is still the current one, then free the box
    /// (`lastUnlockTable = null; lastUnlockLayout = null;` + `queue_free()`).
    fn clear_unlock(&mut self, unlock: Gd<PanelContainer>) {
        let current = self.unlock_box.as_ref().is_some_and(|handle| {
            handle.is_instance_valid() && handle.instance_id() == unlock.instance_id()
        });
        if current {
            self.unlock_box = None;
            self.unlock_grid = None;
        }
        if unlock.is_instance_valid() {
            let mut unlock = unlock;
            unlock.queue_free();
        }
    }

    /// `CoreInfo/CoreItemsPanel/CoreItems` as the native table widget.
    fn core_items(&self) -> Option<Gd<CoreItemsDisplay>> {
        self.node_as("CoreInfo/CoreItemsPanel/CoreItems")
    }

    /// `CoreInfo/BossBar/Bar` as the native bar widget.
    fn boss_mind_bar(&self) -> Option<Gd<MindBar>> {
        self.node_as("CoreInfo/BossBar/Bar")
    }

    /// `MinimapBox/Minimap` as the native minimap widget.
    fn minimap_widget(&self) -> Option<Gd<MindMinimapWidget>> {
        self.node_as("MinimapBox/Minimap")
    }

    /// Connects one signal to a `#[func]` when the signal exists and the
    /// connection is not already live (the GDScript `is_connected` guards).
    fn connect_node(&self, node: &Gd<Node>, signal: &str, method: &str) {
        let mut node = node.clone();
        if !node.has_signal(signal) {
            return;
        }
        let callable = self.base().callable(method);
        if node.is_connected(signal, &callable) {
            return;
        }
        let _ = node.connect(signal, &callable);
    }
}

/// Decodes `Color.to_rgba32` (`0xRRGGBBAA`, the packed form `MindSimHost`
/// returns for team colours).
fn packed_rgba_color(packed: i64) -> Color {
    let packed = packed as u32;
    Color::from_rgba8(
        ((packed >> 24) & 0xFF) as u8,
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    )
}

/// Reads a `bool` from a JSON dictionary (`false` when absent).
fn dict_bool(dict: &VarDictionary, key: &str) -> bool {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<bool>().ok())
        .unwrap_or(false)
}

/// Reads an `int` from a JSON dictionary (`0` when absent).
fn dict_i64(dict: &VarDictionary, key: &str) -> i64 {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<i64>().ok())
        .unwrap_or(0)
}

/// Reads a `float` from a JSON dictionary (`0.0` when absent).
fn dict_f64(dict: &VarDictionary, key: &str) -> f64 {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<f64>().ok())
        .unwrap_or(0.0)
}

/// Reads a `String` from a JSON dictionary (`""` when absent).
fn dict_string(dict: &VarDictionary, key: &str) -> GString {
    dict.get(key)
        .and_then(|value| value.try_to_relaxed::<GString>().ok())
        .unwrap_or_default()
}

/// GDScript `str(float)` for the core-health dictionary key (`HudFragment` keys
/// cores by `"x,y"`); Godot prints whole floats with a trailing `.0`.
fn godot_float_text(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 1.0e16 {
        format!("{value:.1}")
    } else {
        format!("{value}")
    }
}

// ---- minimap/world ping marker (GAPS #46, PAR-0554) -----------------------

/// `PlayerComp.pingDuration` (`PlayerComp.java:37`): `20f * 60f` ticks = 20 s.
const PING_DURATION_SECONDS: f32 = 20.0;
/// `Drawf.square(pingX, pingY, 2f * scaling * s, 45f, ...)` marker radius at
/// the settled scale (screen pixels; Java scales by `Scl.scl(4)` and the
/// display scale, which this HUD overlay approximates with a fixed size).
const PING_MARKER_RADIUS: f32 = 10.0;

/// `Interp.pow5Out` (`Interp.java`).
fn pow5_out(t: f32) -> f32 {
    let inv = 1.0 - t.clamp(0.0, 1.0);
    1.0 - inv * inv * inv * inv * inv
}

/// `Interp.pow5In` (`Interp.java`).
fn pow5_in(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * t * t
}

/// `Mathf.map` (`c + (x - a) / (b - a) * (d - c)`); `Interp.apply` clamps.
fn map_lerp(x: f32, a: f32, b: f32, c: f32, d: f32) -> f32 {
    if (b - a).abs() < f32::EPSILON {
        return c;
    }
    c + (x - a) / (b - a) * (d - c)
}

/// `PlayerComp.drawPing` (`PlayerComp.java:363-388`) fade + pop scale for the
/// remaining lifetime in `1.0` (fresh) ..= `0.0` (expired).
///
/// `alpha = min(pow5Out(map(pingTime, 1/20, 0, 1, 0)),
///              pow5Out(map(pingTime, 1, 0.98, 0, 1)))`;
/// `scaling = 1 + clamp(pow5In(map(pingTime, 1, 0.96, 1, 0))) * 3`.
pub fn ping_marker(ping_time: f32) -> (f32, f32) {
    let ping_time = ping_time.clamp(0.0, 1.0);
    let alpha = pow5_out(map_lerp(ping_time, 1.0 / 20.0, 0.0, 1.0, 0.0))
        .min(pow5_out(map_lerp(ping_time, 1.0, 0.98, 0.0, 1.0)));
    let scaling = 1.0 + pow5_in(map_lerp(ping_time, 1.0, 0.96, 1.0, 0.0)) * 3.0;
    (alpha, scaling)
}

/// `PlayerComp.drawPing` world marker overlay: the team-coloured diamond +
/// pointer + the pinging player's name/label, projected through the camera.
///
/// Java draws this in the world renderer (`Layer.playerName`), which this
/// packet may not edit (`gx/render/**`); the HUD owns the equivalent screen
/// overlay so the ping is visible without a render-layer change. The fullscreen
/// minimap copy (`MinimapRenderer.java:173`) still needs the minimap widget /
/// render hook (reported).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct MindPingOverlay {
    base: Base<Control>,
    /// Remaining lifetime (`PlayerComp.pingTime`); `<= 0` hides the marker.
    ping_time: f32,
    /// Ping world position (`PlayerComp.pingX/pingY`).
    world: Vector2,
    /// Projected screen position, refreshed each frame from the camera.
    screen: Vector2,
    /// `PlayerComp.pingText` (empty = no label).
    ping_text: String,
    /// `PlayerComp.name`.
    player_name: String,
    /// `PlayerComp.color` (team colour).
    color: Color,
    /// Seconds since the overlay started (the `hover` bob).
    elapsed: f32,
    /// Marker name label.
    name_label: Option<Gd<Label>>,
    /// Marker ping-text label.
    text_label: Option<Gd<Label>>,
}

#[godot_api]
impl IControl for MindPingOverlay {
    fn init(base: Base<Control>) -> Self {
        Self {
            base,
            ping_time: 0.0,
            world: Vector2::ZERO,
            screen: Vector2::ZERO,
            ping_text: String::new(),
            player_name: String::new(),
            color: Color::WHITE,
            elapsed: 0.0,
            name_label: None,
            text_label: None,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_anchors_preset(LayoutPreset::FULL_RECT);
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
        self.base_mut().set_visible(false);
        self.base_mut().set_process(true);
        let mut name = Label::new_alloc();
        name.set_mouse_filter(MouseFilter::IGNORE);
        self.base_mut().add_child(&name);
        self.name_label = Some(name);
        let mut text = Label::new_alloc();
        text.set_mouse_filter(MouseFilter::IGNORE);
        text.add_theme_font_size_override("font_size", 12);
        self.base_mut().add_child(&text);
        self.text_label = Some(text);
    }

    fn process(&mut self, delta: f64) {
        if self.ping_time <= 0.0 {
            if self.base().is_visible() {
                self.base_mut().set_visible(false);
            }
            return;
        }
        self.elapsed += delta as f32;
        self.ping_time = (self.ping_time - delta as f32 / PING_DURATION_SECONDS).max(0.0);
        if self.ping_time <= 0.0 {
            self.base_mut().set_visible(false);
            return;
        }
        self.project();
        self.place_labels();
        self.base_mut().queue_redraw();
    }

    fn draw(&mut self) {
        if self.ping_time <= 0.0 {
            return;
        }
        let (alpha, scaling) = ping_marker(self.ping_time);
        if alpha <= 0.0 {
            return;
        }
        let center = self.screen;
        let radius = PING_MARKER_RADIUS * scaling;
        let hover = (self.elapsed * 5.0).sin().abs();
        let mut fill = self.color;
        // `Tmp.c3.set(Color.darkGray).mul(color).a(Tmp.c1.a)`.
        fill = Color::from_rgba(fill.r * 0.3, fill.g * 0.3, fill.b * 0.3, alpha.min(fill.a));
        let stroke = Color::from_rgba(self.color.r, self.color.g, self.color.b, alpha);
        let points = vec![
            center + Vector2::new(0.0, -radius),
            center + Vector2::new(radius, 0.0),
            center + Vector2::new(0.0, radius),
            center + Vector2::new(-radius, 0.0),
        ];
        let diamond: PackedVector2Array = points.iter().copied().collect();
        self.base_mut().draw_colored_polygon(&diamond, fill);
        let mut outline = points.clone();
        outline.push(points[0]);
        let outline: PackedVector2Array = outline.into_iter().collect();
        self.base_mut()
            .draw_polyline_ex(&outline, stroke)
            .width(2.0)
            .done();
        // `Drawf.fillPoly(pingX, pingY + 9f * s + hover * s, 3, 3f * s, -90f)`.
        let tip = center + Vector2::new(0.0, 9.0 + hover);
        let tri: PackedVector2Array = [
            tip + Vector2::new(0.0, -7.0),
            tip + Vector2::new(6.0, 5.0),
            tip + Vector2::new(-6.0, 5.0),
        ]
        .into_iter()
        .collect();
        self.base_mut().draw_colored_polygon(&tri, stroke);
    }
}

#[godot_api]
impl MindPingOverlay {
    /// Shows the marker at a world point (`Call.pingLocation` ->
    /// `PlayerComp.pingX/pingY/pingTime/pingText`).
    pub fn show_at(&mut self, world: Vector2, ping_text: &str, name: &str, color: Color) {
        self.world = world;
        self.ping_text = ping_text.to_owned();
        self.player_name = name.to_owned();
        self.color = color;
        self.ping_time = 1.0;
        self.elapsed = 0.0;
        self.project();
        self.place_labels();
        self.base_mut().set_visible(true);
        self.base_mut().queue_redraw();
    }

    /// Debug read model (`{time, x, y, text, name, visible}`).
    #[func]
    pub fn ping_state(&self) -> VarDictionary {
        let mut out = VarDictionary::new();
        out.set(&GString::from("time"), &self.ping_time.to_variant());
        out.set(&GString::from("x"), &self.world.x.to_variant());
        out.set(&GString::from("y"), &self.world.y.to_variant());
        out.set(
            &GString::from("text"),
            &GString::from(self.ping_text.as_str()).to_variant(),
        );
        out.set(
            &GString::from("name"),
            &GString::from(self.player_name.as_str()).to_variant(),
        );
        out.set(
            &GString::from("visible"),
            &self.base().is_visible().to_variant(),
        );
        out
    }

    /// Projects [`Self::world`] through the camera (`world_to_screen`).
    fn project(&mut self) {
        let Some(mut camera) = kit::node("/root/Spine/World/Camera2D") else {
            return;
        };
        if !camera.has_method("world_to_screen") {
            return;
        }
        if let Ok(screen) = camera
            .call(
                "world_to_screen",
                &[
                    f64::from(self.world.x).to_variant(),
                    f64::from(self.world.y).to_variant(),
                ],
            )
            .try_to::<Vector2>()
        {
            self.screen = screen;
        }
    }

    /// Positions the name/text labels under the marker.
    fn place_labels(&mut self) {
        let Some(mut name) = self.name_label.clone() else {
            return;
        };
        let width = name.get_minimum_size().x;
        name.set_position(self.screen + Vector2::new(-width / 2.0, 16.0));
        name.set_text(self.player_name.as_str());
        name.set_visible(!self.player_name.is_empty());
        name.remove_theme_color_override("font_color");
        name.add_theme_color_override("font_color", self.color);
        let Some(mut text) = self.text_label.clone() else {
            return;
        };
        let width = text.get_minimum_size().x;
        text.set_position(self.screen + Vector2::new(-width / 2.0, 34.0));
        text.set_text(self.ping_text.as_str());
        text.set_visible(!self.ping_text.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_marker_matches_player_comp_curve() {
        // `PlayerComp.pingDuration` is 20 s (`20f * 60f` ticks).
        assert!((PING_DURATION_SECONDS - 20.0).abs() < f32::EPSILON);
        // Fresh ping: transparent, popped to 4x (the marker shrinks in).
        let (alpha, scaling) = ping_marker(1.0);
        assert!(alpha < 0.0001, "fresh ping fades in, alpha {alpha}");
        assert!((scaling - 4.0).abs() < f32::EPSILON, "scale {scaling}");
        // Settled: fully opaque once the 0.4 s fade and the 0.8 s pop finish.
        let (alpha, scaling) = ping_marker(0.5);
        assert!((alpha - 1.0).abs() < f32::EPSILON);
        assert!(
            (scaling - 1.0).abs() < f32::EPSILON,
            "settled scale {scaling}"
        );
        // Expiring: fades out over the last second.
        let (alpha, scaling) = ping_marker(0.0);
        assert!(alpha < 0.0001);
        assert!((scaling - 1.0).abs() < f32::EPSILON);
        // Never NaN outside the range.
        let (alpha, scaling) = ping_marker(2.5);
        assert!(alpha.is_finite() && scaling.is_finite());
    }

    #[test]
    fn cadence_and_timings_match_the_fragment() {
        assert!((REFRESH_INTERVAL - 0.2).abs() < f64::EPSILON);
        assert_eq!(SKIP_ICON, "play");
        assert_eq!(LOGIC_HUD_INTERVAL, 10);
        assert!((CORE_ATTACK_TICKS - 240.0).abs() < f64::EPSILON);
        assert!((GAME_OVER_DELAY - 3.0).abs() < f64::EPSILON);
        assert!((UNLOCK_SLIDE_SECONDS - 1.0).abs() < f64::EPSILON);
        assert!((UNLOCK_HOLD_SECONDS - 2.5).abs() < f64::EPSILON);
        assert!((STATUS_ICON_SIZE - 32.0).abs() < f32::EPSILON);
    }

    #[test]
    fn unlock_grid_cap_is_the_ninth_cell() {
        assert_eq!(UNLOCK_COLS, 3);
        assert_eq!(UNLOCK_CAP, 8);
        assert_eq!(UNLOCK_ICON_SIZE, 48.0);
        // `48 / min(n + 1, 3)`: 1 -> 24 px, 2 -> 16 px, 8 -> 8 px.
        assert_eq!(UNLOCK_ICON_SIZE / 2.0, 24.0);
        assert_eq!(UNLOCK_ICON_SIZE / 3.0, 16.0);
        assert_eq!(UNLOCK_ICON_SIZE / 6.0, 8.0);
    }

    #[test]
    fn core_health_keys_match_gdscript_str() {
        assert_eq!(godot_float_text(10.0), "10.0");
        assert_eq!(godot_float_text(-3.0), "-3.0");
        assert_eq!(godot_float_text(10.5), "10.5");
        assert_eq!(
            format!("{},{}", godot_float_text(160.0), godot_float_text(96.0)),
            "160.0,96.0"
        );
    }

    #[test]
    fn boss_and_core_attack_colours_are_the_java_hex_values() {
        assert_eq!(BOSS_HEALTH_COLOR, Color::from_rgba8(0xff, 0x34, 0x1c, 0xff));
        assert_eq!(
            CORE_ATTACK_ORANGE,
            Color::from_rgba8(0xff, 0xa5, 0x00, 0xff)
        );
        assert_eq!(
            CORE_ATTACK_SCARLET,
            Color::from_rgba8(0xff, 0x24, 0x00, 0xff)
        );
    }
}
