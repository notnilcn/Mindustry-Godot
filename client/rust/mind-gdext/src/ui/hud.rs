// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MindHud` autoload (plan 14 §3.5/M4): the read-only per-frame HUD surface.
//!
//! The GDScript `HudFragment` binds to these typed properties/signals only; it
//! never reads sim state directly (plan 14 §3.10 rule 2). Values are recomputed
//! at most once per frame from the read-only `MindSimHost`/`MindCampaign`
//! surfaces; the campaign half is polled on a short cadence and composed through
//! the Godot-free `mind_core::ui::hud_text` model (`HudFragment` status text).

use godot::classes::notify::NodeNotification;
use godot::classes::{Engine, INode, Node as GdNode, Os};
use godot::meta::ToGodot;
use godot::obj::{Base, WithBaseField};
use godot::prelude::*;

use mind_core::assets::bundle::Bundle;
use mind_core::assets::icons::Iconc;
use mind_core::config::TILESIZE;
use mind_core::game::map_objectives::{MapObjectivesRuntime, ObjectiveLocale};
use mind_core::game::rules::Rules;
use mind_core::ui::hud_text::{self, HudStatus};

use crate::assets::bundle::{load_bundle, load_iconc};
use crate::assets::loader::{resolve_assets_dir, resolve_locale};

/// Path to the sim owner the HUD reads (plan 00 §3.5).
const SIM_HOST_PATH: &str = "/root/Spine/SimHost";
/// Path to the campaign facade (wave/objective/sector state, gap3 C-10).
const CAMPAIGN_PATH: &str = "/root/Spine/MindCampaign";
/// Path to the UI singleton (graphics settings for the position label).
const UI_PATH: &str = "/root/MindUi";
/// Path to the camera rig (`position`/`mouseposition` labels).
const CAMERA_PATH: &str = "/root/Spine/World/Camera2D";
/// Campaign-field poll cadence in seconds (`HudFragment.REFRESH_INTERVAL`).
const CAMPAIGN_REFRESH_SECONDS: f64 = 0.2;

/// `MindHud` — the typed per-frame HUD state surface (`/root/MindHud`).
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindHud {
    base: Base<GdNode>,
    host: Option<Gd<GdNode>>,
    campaign_node: Option<Gd<GdNode>>,
    ui: Option<Gd<GdNode>>,
    camera: Option<Gd<GdNode>>,
    bundle: Bundle,
    iconc: Iconc,
    /// Seconds since the last campaign refresh.
    campaign_accum: f64,
    /// Whether a campaign baseline has been observed (transition edges).
    campaign_seen: bool,
    /// Last observed `state.wave`.
    last_wave: i32,
    /// Last observed sector-captured flag.
    last_captured: bool,
    /// Last observed `player_core_count() > 0`.
    last_had_core: bool,
    /// Last observed sector key (`planet:index`), resets transitions on launch.
    last_sector: String,
    /// `state.phase` name (`menu`/`playing`/`paused`/…).
    #[var]
    state_name: GString,
    /// `state.isPaused()`.
    #[var]
    paused: bool,
    /// `netServer.isWaitingForPlayers()` (plan 21; false offline).
    #[var]
    waiting: bool,
    /// Completed ticks.
    #[var]
    tick: i64,
    /// `state.wave` (1-based: `PlaySession.wave + 1`, `GameState.java:17`).
    #[var]
    wave: i32,
    /// `state.enemies` (campaign read model).
    #[var]
    enemies: i32,
    /// `state.wavetime` in ticks.
    #[var]
    wavetime: f32,
    /// `state.rules.winWave`.
    #[var]
    win_wave: i32,
    /// `state.rules.waves`.
    #[var]
    waves: bool,
    /// `state.rules.waveTimer`.
    #[var]
    wave_timer: bool,
    /// `state.isCampaign()`.
    #[var]
    campaign: bool,
    /// `state.gameOver`.
    #[var]
    game_over: bool,
    /// `state.won`.
    #[var]
    won: bool,
    /// `state.teams.playerCores().size > 0`.
    #[var]
    has_core: bool,
    /// `state.rules.sector.info.wasCaptured`.
    #[var]
    was_captured: bool,
    /// Enemy core count for attack-mode status text.
    #[var]
    enemy_cores: i32,
    /// Display name of the active sector (toasts / game-over dialog).
    #[var]
    sector_name: GString,
    /// Engine frames per second.
    #[var]
    fps: i32,
    /// Static memory in MiB (`OS.get_static_memory_usage`).
    #[var]
    memory_mb: i32,
    /// Network ping in ms (plan 21; 0 offline).
    #[var]
    ping: i32,
    /// Target sim ticks per second (fixed 60 Hz).
    #[var]
    tps: i32,
    /// Cursor/position text (`position`/`mouseposition` settings).
    #[var]
    position_text: GString,
    /// Composed wave/status text (`ui::hud_text`).
    #[var]
    status_text: GString,
}

#[godot_api]
impl INode for MindHud {
    fn init(base: Base<GdNode>) -> Self {
        Self {
            base,
            host: None,
            campaign_node: None,
            ui: None,
            camera: None,
            bundle: Bundle::default(),
            iconc: Iconc::default(),
            campaign_accum: 0.0,
            campaign_seen: false,
            last_wave: 0,
            last_captured: false,
            last_had_core: false,
            last_sector: String::new(),
            state_name: GString::from("menu"),
            paused: false,
            waiting: false,
            tick: 0,
            wave: 0,
            enemies: 0,
            wavetime: 0.0,
            win_wave: 0,
            waves: false,
            wave_timer: false,
            campaign: false,
            game_over: false,
            won: false,
            has_core: false,
            was_captured: false,
            enemy_cores: 0,
            sector_name: GString::new(),
            fps: 0,
            memory_mb: 0,
            ping: 0,
            tps: 60,
            position_text: GString::new(),
            status_text: GString::new(),
        }
    }

    fn ready(&mut self) {
        self.bootstrap();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        // `ready()` is not re-run on hot reload; rebuild Godot-derived state.
        if what == NodeNotification::EXTENSION_RELOADED {
            self.bootstrap();
        }
    }

    fn process(&mut self, delta: f64) {
        // Re-acquire the sim if it appeared after boot (scenario load).
        self.resolve_nodes();
        self.refresh_engine();
        self.campaign_accum += delta;
        if self.campaign_accum >= CAMPAIGN_REFRESH_SECONDS {
            self.campaign_accum = 0.0;
            self.refresh_campaign();
        }
    }
}

#[godot_api]
impl MindHud {
    /// Emitted when server-sent HUD text changes (`HudText` relay payload).
    #[signal]
    fn hud_text(text: GString);

    /// Emitted for a toast (text + icon region).
    #[signal]
    fn toast(text: GString, icon: GString);

    /// Emitted when a content item is unlocked.
    #[signal]
    fn unlock(name: GString);

    /// Emitted for an announcement overlay.
    #[signal]
    fn announce(text: GString, duration: f32);

    /// Emitted when a new wave starts (`Logic.runWave`).
    #[signal]
    fn wave_event();

    /// Emitted on sector capture/lose/invasion (`kind`, sector name).
    #[signal]
    fn sector_event(kind: GString, name: GString);

    /// Emits server-sent HUD text (`setHudText`); shown by `CoreInfo/HudText`.
    #[func]
    pub fn set_hud_text(&mut self, text: GString) {
        let _ = self
            .base_mut()
            .emit_signal("hud_text", &[text.to_variant()]);
    }

    /// Emits a toast on behalf of the relay (`Menus.infoToast`).
    #[func]
    pub fn push_toast(&mut self, text: GString, icon: GString) {
        let _ = self
            .base_mut()
            .emit_signal("toast", &[text.to_variant(), icon.to_variant()]);
    }

    /// Emits an announcement on behalf of the relay (`Menus.announce`).
    #[func]
    pub fn push_announce(&mut self, text: GString, duration: f32) {
        let _ = self
            .base_mut()
            .emit_signal("announce", &[text.to_variant(), duration.to_variant()]);
    }

    /// Emits an unlock event (`Menus.…`/`UnlockEvent`).
    #[func]
    pub fn push_unlock(&mut self, name: GString) {
        let _ = self.base_mut().emit_signal("unlock", &[name.to_variant()]);
    }

    /// Live player-core item counts JSON (`{item-name: amount}`) for the
    /// `CoreItemsDisplay` widget, delegated to `MindSimHost.core_items_json`.
    #[func]
    pub fn core_items_json(&self) -> GString {
        let Some(mut host) = self
            .host
            .clone()
            .or_else(|| self.base().get_node_or_null(SIM_HOST_PATH))
        else {
            return GString::from("{}");
        };
        host.call("core_items_json", &[])
            .try_to::<GString>()
            .unwrap_or_else(|_| GString::from("{}"))
    }
}

impl MindHud {
    /// Rebuilds the read-only surface from the (possibly reloaded) node; the
    /// host lookup and `refresh_*` overwrite, so a re-run never duplicates.
    fn bootstrap(&mut self) {
        let assets_dir = resolve_assets_dir();
        let locale = resolve_locale();
        self.bundle = load_bundle(&assets_dir, &locale);
        self.iconc = load_iconc(&assets_dir);
        self.host = self.base().get_node_or_null(SIM_HOST_PATH);
        if self.host.is_none() {
            log::warn!("MindHud: no MindSimHost at {SIM_HOST_PATH}");
        }
        self.campaign_node = None;
        self.ui = None;
        self.camera = None;
        self.campaign_seen = false;
        self.refresh_engine();
        self.refresh_campaign();
    }

    /// Lazily resolves the sibling nodes (autoloads boot before the main scene).
    fn resolve_nodes(&mut self) {
        if self.host.is_none() {
            self.host = self.base().get_node_or_null(SIM_HOST_PATH);
        }
        if self.campaign_node.is_none() {
            self.campaign_node = self.base().get_node_or_null(CAMPAIGN_PATH);
        }
        if self.ui.is_none() {
            self.ui = self.base().get_node_or_null(UI_PATH);
        }
        if self.camera.is_none() {
            self.camera = self.base().get_node_or_null(CAMERA_PATH);
        }
    }

    /// Recomputes the per-frame engine monitors (cheap, no JSON).
    fn refresh_engine(&mut self) {
        self.fps = Engine::singleton().get_frames_per_second() as i32;
        self.memory_mb = (Os::singleton().get_static_memory_usage() / 1_048_576) as i32;
        let Some(mut host) = self.host.clone() else {
            return;
        };
        self.paused = host
            .call("is_paused", &[])
            .try_to::<bool>()
            .unwrap_or(false);
        self.tick = host.call("get_tick", &[]).try_to::<i64>().unwrap_or(0);
        self.state_name = host
            .call("get_state", &[])
            .try_to::<GString>()
            .unwrap_or_default();
    }

    /// Recomputes the campaign half: wave/enemy/timer fields, the composed
    /// status text, transition signals and the position label.
    fn refresh_campaign(&mut self) {
        self.resolve_nodes();
        let Some(mut campaign) = self.campaign_node.clone() else {
            self.status_text = GString::new();
            return;
        };
        let Some(state) = campaign
            .call("get_hud_state", &[])
            .try_to::<VarDictionary>()
            .ok()
        else {
            return;
        };

        let campaign_mode = dict_bool(&state, "campaign");
        let attack_mode = dict_bool(&state, "attack");
        let wait_enemies = dict_bool(&state, "waitEnemies");
        let win_wave = dict_i64(&state, "winWave") as i32;
        // `PlaySession.wave` is the 0-based spawn index; the HUD shows Java's
        // `state.wave`, which starts at 1 (`GameState.java:17`) and is what the
        // `wave.cap`/`isWaitingWave` comparisons use.
        let wave = dict_i64(&state, "wave") as i32 + 1;
        let enemies = dict_i64(&state, "enemies") as i32;
        let wavetime = dict_f32(&state, "wavetime");
        let wave_timer = dict_bool(&state, "waveTimer");
        let waves = dict_bool(&state, "waves");
        let game_over = dict_bool(&state, "gameOver");
        let after_game_over = dict_bool(&state, "afterGameOver");
        let won = dict_bool(&state, "won");
        let has_core = dict_bool(&state, "hasCore");
        let was_captured = dict_bool(&state, "wasCaptured");
        let enemy_cores = dict_i64(&state, "enemyCores") as i32;
        let sector_name = dict_string(&state, "sectorName");
        let sector_key = format!(
            "{}:{}",
            dict_string(&state, "planet"),
            dict_i64(&state, "sector")
        );
        let waiting_wave = (wait_enemies || (win_wave > 0 && wave >= win_wave)) && enemies > 0;

        // Emit wave/sector transitions only within one sector session, so a
        // fresh launch resets the baselines without replaying toasts.
        let sector_changed = !self.campaign_seen || sector_key != self.last_sector;
        if !sector_changed {
            if wave != self.last_wave {
                let _ = self.base_mut().emit_signal("wave_event", &[]);
            }
            if was_captured && !self.last_captured {
                let text = self
                    .bundle
                    .format("sector.capture", &[sector_name.as_str()]);
                self.push_toast(GString::from(text.as_str()), GString::from("ok"));
                let _ = self.base_mut().emit_signal(
                    "sector_event",
                    &[
                        GString::from("capture").to_variant(),
                        GString::from(sector_name.as_str()).to_variant(),
                    ],
                );
            }
            if self.last_had_core && !has_core && campaign_mode {
                let text = self.bundle.format("sector.lost", &[sector_name.as_str()]);
                self.push_toast(GString::from(text.as_str()), GString::from("warning"));
                let _ = self.base_mut().emit_signal(
                    "sector_event",
                    &[
                        GString::from("lost").to_variant(),
                        GString::from(sector_name.as_str()).to_variant(),
                    ],
                );
            }
        }
        self.last_sector = sector_key;
        self.last_wave = wave;
        self.last_captured = was_captured;
        self.last_had_core = has_core;
        self.campaign_seen = true;

        self.campaign = campaign_mode;
        self.wave = wave;
        self.enemies = enemies;
        self.wavetime = wavetime;
        self.win_wave = win_wave;
        self.waves = waves;
        self.wave_timer = wave_timer;
        self.game_over = game_over;
        self.won = won;
        self.has_core = has_core;
        self.was_captured = was_captured;
        self.enemy_cores = enemy_cores;
        self.sector_name = GString::from(sector_name.as_str());

        // Objective/mission text comes from the live rules; the Godot-free
        // `hud_text` model filters/format-icons the qualified rows.
        let mut mission = String::new();
        let mut objectives = Vec::new();
        if let Ok(json) = campaign.call("get_rules_json", &[]).try_to::<GString>()
            && let Ok(rules) = serde_json::from_str::<Rules>(json.to_string().as_str())
        {
            mission = rules.mission.clone().unwrap_or_default();
            let runtime = MapObjectivesRuntime::from_rules(&rules);
            let locale = HudLocale {
                bundle: &self.bundle,
            };
            for index in 0..runtime.len() {
                if !runtime.qualified(index) {
                    continue;
                }
                let Some(objective) = runtime.get(index) else {
                    continue;
                };
                if objective.common().hidden {
                    continue;
                }
                if let Some(text) = runtime.text(index, &locale)
                    && !text.is_empty()
                {
                    objectives.push(text);
                }
            }
        }

        let status = HudStatus {
            mission,
            objectives,
            unit_activation_remaining: None,
            waves: self.waves,
            attack_mode,
            enemy_cores: self.enemy_cores,
            after_game_over_campaign: after_game_over && self.campaign,
            campaign: self.campaign,
            win_wave: self.win_wave,
            wave: self.wave,
            enemies: self.enemies,
            wave_timer: self.wave_timer,
            waiting_wave,
            wavetime_ticks: self.wavetime,
        };
        let composed = hud_text::status_text(&status, &self.bundle, &self.iconc);
        // The HUD labels are `MindRichLabel` (BBCode); translate Arc markup.
        let bbcode = mind_core::ui::text::render_markup(&composed, &self.iconc);
        self.status_text = GString::from(bbcode.as_str());

        self.refresh_position();
    }

    /// `HudFragment` position label: player/camera tile when `position` is on,
    /// the mouse tile (light gray) when `mouseposition` is on.
    fn refresh_position(&mut self) {
        let (show_position, show_mouse) = self.graphics_flags();
        if !show_position && !show_mouse {
            self.position_text = GString::new();
            return;
        }
        let unit = TILESIZE as f32;
        let camera = self.camera.clone();
        let mut text = String::new();
        if show_position {
            let position = camera
                .clone()
                .and_then(|camera| camera.get("position").try_to::<Vector2>().ok())
                .unwrap_or_default();
            text.push_str(&format!(
                "{},{}",
                (position.x / unit).floor() as i32,
                (position.y / unit).floor() as i32
            ));
        }
        if show_mouse && let Some(mut camera) = camera {
            let mouse = self
                .base()
                .get_viewport()
                .map(|viewport| viewport.get_mouse_position())
                .unwrap_or_default();
            if let Ok(tile) = camera
                .call(
                    "screen_to_tile",
                    &[mouse.x.to_variant(), mouse.y.to_variant()],
                )
                .try_to::<Vector2i>()
            {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&format!("[lightgray]{},{}", tile.x, tile.y));
            }
        }
        self.position_text =
            GString::from(mind_core::ui::text::render_markup(&text, &self.iconc).as_str());
    }

    /// Reads the `graphics` settings rows once for the two position flags.
    fn graphics_flags(&self) -> (bool, bool) {
        let Some(mut ui) = self
            .ui
            .clone()
            .or_else(|| self.base().get_node_or_null(UI_PATH))
        else {
            return (false, false);
        };
        let Ok(rows) = ui
            .call(
                "settings_rows_json",
                &[GString::from("graphics").to_variant()],
            )
            .try_to::<GString>()
        else {
            return (false, false);
        };
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(rows.to_string().as_str())
        else {
            return (false, false);
        };
        let read = |key: &str| {
            parsed
                .as_array()
                .and_then(|rows| {
                    rows.iter()
                        .find(|row| row.get("key").and_then(|value| value.as_str()) == Some(key))
                })
                .and_then(|row| row.get("value"))
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        };
        (read("position"), read("mouseposition"))
    }
}

/// `Objective.text()` localization over the client bundle.
struct HudLocale<'a> {
    bundle: &'a Bundle,
}

impl ObjectiveLocale for HudLocale<'_> {
    fn fetch_text(&self, text: &str) -> String {
        match text.strip_prefix('@') {
            Some(key) => self.bundle.get(key).to_owned(),
            None => text.to_owned(),
        }
    }

    fn format(&self, key: &str, args: &[&str]) -> String {
        self.bundle.format(key, args)
    }

    fn get(&self, key: &str) -> String {
        self.bundle.get(key).to_owned()
    }
}

/// Reads a boolean field from a `VarDictionary` (`false` when absent).
fn dict_bool(dict: &VarDictionary, key: &str) -> bool {
    dict.get(&GString::from(key).to_variant())
        .and_then(|value| value.try_to::<bool>().ok())
        .unwrap_or(false)
}

/// Reads an integer field from a `VarDictionary` (`0` when absent).
fn dict_i64(dict: &VarDictionary, key: &str) -> i64 {
    dict.get(&GString::from(key).to_variant())
        .and_then(|value| value.try_to::<i64>().ok())
        .unwrap_or(0)
}

/// Reads a float field from a `VarDictionary` (`0.0` when absent).
fn dict_f32(dict: &VarDictionary, key: &str) -> f32 {
    dict.get(&GString::from(key).to_variant())
        .and_then(|value| value.try_to::<f32>().ok())
        .unwrap_or(0.0)
}

/// Reads a string field from a `VarDictionary` (`""` when absent).
fn dict_string(dict: &VarDictionary, key: &str) -> String {
    dict.get(&GString::from(key).to_variant())
        .and_then(|value| value.try_to::<GString>().ok())
        .map(|value| value.to_string())
        .unwrap_or_default()
}
