// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// `SoundControl`/`SoundPriority`/`AmbientSource`/`SoundLoop`/`MusicContainer`.

//! `/root/Spine/MindAudio` — the client audio driver (plan 18 §3.1/§3.10).
//!
//! Buses, stream cache and the voice pool are Godot objects; the music/loop
//! state machines live in `mind_core::audio`. The in-engine §7c MCP sweep is
//! deferred to the single-editor mutex; this exposes the stable `#[func]` test
//! API from the plan.

pub mod buses;
pub mod decorator;
pub mod hooks;
pub mod streams;
pub mod voices;

use godot::classes::{AudioListener2D, AudioServer, AudioStreamPlayer, INode, Node as GdNode};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::audio::frame::{AudioGameState, LoopPos, LoopSource, TurretLoopSource};
use mind_core::audio::math::{Listener, MIN_INTERVAL_MS, MIN_PLAY_VOLUME, calc_falloff, calc_pan};
use mind_core::audio::{
    ActiveVoice, AmbientPoller, AmbientSnapshot, AudioEvent, AudioRng, LoopMixer, LoopOutput,
    MusicContext, MusicOutput, MusicPlayer, MusicRef, OneShotPlan, PlanetMusic, SeededAudioRng,
    SharedAudioLog, SoundId, SoundPlayState, SoundPriorityTable, TickedAudioEvent, bus_for_sound,
    plan_one_shot, settings_keys,
};

use crate::wiring;

use buses::{BusLayout, LOWPASS_DRY_HZ, LOWPASS_WET_HZ};
use streams::StreamCache;
use voices::{LoopVoicePool, SoundLoopInput, SoundLoopTable, SoundLoopVoice, VoicePool};

/// The audio node.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindAudio {
    base: Base<GdNode>,
    /// Sibling `SimHost` (`../SimHost`) — optional; phase and audio game state.
    #[export]
    pub sim_host: Option<Gd<crate::sim_host::MindSimHost>>,
    /// Sibling `World/Camera2D` (`../World/Camera2D`) — optional; the listener.
    #[export]
    pub camera: Option<Gd<crate::camera::MindCamera2D>>,
    /// `Ui/UiRoot` (`../Ui/UiRoot`) — optional; HUD group visibility.
    #[export]
    pub ui_root: Option<Gd<crate::ui::root::MindUiRoot>>,
    /// `Ui/MenuBackground` (`../Ui/MenuBackground`) — optional; menu music.
    #[export]
    pub menu_background: Option<Gd<crate::ui::menu_background::MindMenuBackground>>,
    /// `Ui/EditorDialog` (`../Ui/EditorDialog`) — optional; editor music.
    #[export]
    pub editor_dialog: Option<Gd<crate::ui::editor_views::MindMapEditorDialog>>,
    ready: bool,
    bus: BusLayout,
    streams: StreamCache,
    pool: Option<VoicePool>,
    /// Persistent loop voices (`LoopMixer` output).
    loop_pool: Option<LoopVoicePool>,
    /// 20 Hz ambient poller (T5 fills its snapshot).
    poller: AmbientPoller,
    /// Live ambient source snapshot (rebuilt at the poll rate).
    ambient: AmbientSnapshot,
    /// Per-frame `control.sound.loop` sources (T6).
    frame_loops: Vec<LoopSource>,
    /// Per-frame turret `SoundLoop` inputs (T6).
    turret_loop_sources: Vec<TurretLoopSource>,
    /// Live per-key `SoundLoop` fade voices (`SoundLoop.java`): the per-frame
    /// turret/weapon scan and the `AudioEvent::LoopInstance` sink share one
    /// lifecycle so each key keeps its own pitch/volume/fade/stop.
    sound_loops: SoundLoopTable,
    /// Previous frame's `in_game` (`ResetEvent` mirror, T7).
    last_in_game: Option<bool>,
    /// Previous observed wave (wave-event edge detection, T7).
    last_wave: Option<i32>,
    /// Previous sim tick (backwards tick = fresh sector, T7).
    last_tick: i64,
    /// `debug_hold_loop` test seam: `(sound, x, y, volume, frames_left)`.
    debug_loop_hold: Option<(SoundId, f32, f32, f32, i64)>,
    music_player: Option<Gd<AudioStreamPlayer>>,
    /// `Musics.land` player (`Trigger.newGame` landing cue, T7).
    land_player: Option<Gd<AudioStreamPlayer>>,
    music: MusicPlayer,
    rng: SeededAudioRng,
    mixer: LoopMixer,
    table: SoundPriorityTable,
    settings: Settings,
    current: Option<MusicRef>,
    music_override: bool,
    lowpass_target: f32,
    lowpass_wet: f32,
    sound_paused: bool,
    audition: bool,
    /// `PlaySoundStatement.lastPreview` voice (`showSoundSelect` stops the
    /// previous preview before playing the next one).
    preview_voice: Option<i64>,
    /// `_process` dispatch counter (T1 liveness probe / PAR-0072 gate).
    frames: u64,
    /// Plan-18 sim→client sink, fetched from the wired `sim_host`.
    audio_log: Option<SharedAudioLog>,
    /// Events applied through the sink drain (inspector stat).
    events_applied: u64,
    /// `ConstructBlock.shouldPlay` wall clock (`ConstructBlock.java:58-69`).
    construct_last_played_ms: f64,
    /// `calcPitch` run window (`ConstructBlock.java:71-120`).
    construct_last_time_ms: f64,
    /// `calcPitch` sequence counter.
    construct_pitch_seq: i32,
    /// Per-sound `Sound.lastTimePlayed`/`lastVoice`/`lastVolume` (`checkFrame`).
    /// Lookup-only map (never iterated; audio is view-only).
    play_state: std::collections::HashMap<SoundId, SoundPlayState>,
    /// Scratch for `VoicePool::active_voices`.
    active_scratch: Vec<ActiveVoice>,
    /// Wall clock captured at the top of `process()` (ms).
    now_ms: f64,
    log: Vec<String>,
    missing: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct Settings {
    musicvol: i32,
    sfxvol: i32,
    ambientvol: i32,
    alwaysmusic: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            musicvol: 100,
            sfxvol: 100,
            ambientvol: 100,
            alwaysmusic: false,
        }
    }
}

#[godot_api]
impl INode for MindAudio {
    fn init(base: Base<GdNode>) -> Self {
        MindAudio {
            base,
            sim_host: None,
            camera: None,
            ui_root: None,
            menu_background: None,
            editor_dialog: None,
            ready: false,
            bus: BusLayout::default(),
            streams: StreamCache::default(),
            pool: None,
            loop_pool: None,
            poller: AmbientPoller::new(50.0),
            ambient: AmbientSnapshot::default(),
            frame_loops: Vec::with_capacity(16),
            turret_loop_sources: Vec::with_capacity(8),
            sound_loops: SoundLoopTable::new(),
            last_in_game: None,
            last_wave: None,
            last_tick: 0,
            debug_loop_hold: None,
            music_player: None,
            land_player: None,
            music: MusicPlayer::new(),
            rng: SeededAudioRng::new(0),
            mixer: LoopMixer::new(),
            table: SoundPriorityTable::build(),
            settings: Settings::default(),
            current: None,
            music_override: false,
            lowpass_target: 0.0,
            lowpass_wet: 0.0,
            sound_paused: false,
            audition: false,
            preview_voice: None,
            frames: 0,
            audio_log: None,
            events_applied: 0,
            construct_last_played_ms: 0.0,
            construct_last_time_ms: 0.0,
            construct_pitch_seq: 0,
            play_state: std::collections::HashMap::new(),
            active_scratch: Vec::with_capacity(64),
            now_ms: 0.0,
            log: Vec::new(),
            missing: Vec::new(),
        }
    }

    fn ready(&mut self) {
        // Scene-wired optional spine deps: `game.tscn` sets them on the
        // instance root; an audio-only scene has none and stays silent.
        self.sim_host = wiring::optional(&self.sim_host);
        self.camera = wiring::optional(&self.camera);
        self.ui_root = wiring::optional(&self.ui_root);
        self.menu_background = wiring::optional(&self.menu_background);
        self.editor_dialog = wiring::optional(&self.editor_dialog);

        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos() as u64)
            .unwrap_or(1);
        self.rng = SeededAudioRng::new(seed);

        self.bus = buses::ensure_buses();

        let assets_dir = crate::assets::loader::resolve_assets_dir();
        self.streams = StreamCache::load(&assets_dir);

        // code-instantiated: the pool is a fixed, preallocated set of pooled
        // high-churn players (plan 18 §3.6); not expressible as static nodes.
        let mut self_node: Gd<GdNode> = self.base_mut().clone().upcast::<GdNode>();
        let pool = VoicePool::new(&mut self_node, "Sound", "UI");
        let loop_pool = LoopVoicePool::new(&mut self_node, "Sound");
        if !self.bus.is_ready() {
            log::warn!("[audio] bus layout incomplete: {:?}", self.bus);
        }
        // The music player is a single pooled voice on the Music bus.
        let mut music_player = AudioStreamPlayer::new_alloc();
        music_player.set_bus(&StringName::from("Music"));
        self.base_mut().add_child(&music_player);
        // `Musics.land` (`Trigger.newGame` landing cue) gets its own player so
        // it does not fight the `SoundControl` music state machine (T7).
        let mut land_player = AudioStreamPlayer::new_alloc();
        land_player.set_bus(&StringName::from("Music"));
        self.base_mut().add_child(&land_player);
        pool.stop_all();
        self.pool = Some(pool);
        self.loop_pool = Some(loop_pool);
        self.music_player = Some(music_player);
        self.land_player = Some(land_player);

        // Audio listener on the spine camera (plan 18 §3.10).
        if let Some(mut camera) = wiring::optional(&self.camera) {
            let mut listener = AudioListener2D::new_alloc();
            listener.make_current();
            camera.add_child(&listener);
        }

        self.ready = true;
        log::info!(
            "[audio] ready (buses {:?}, {} sounds, dir {})",
            self.bus,
            self.streams.sound_count(),
            assets_dir
        );
        // G07 T13: `UI.java:146`/`BaseDialog`/`HudFragment` UI sounds — the
        // `MindUi.play_ui_sound` signal had no listener.
        if let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") {
            let callable = self.base().callable("on_ui_sound");
            let _ = ui.connect("play_ui_sound", &callable);
        }
        // `Core.settings` is one global store; pull the sound keys once so the
        // sliders are live before the first frame (PAR-0074).
        self.refresh_settings();
        // `Control.update` drives `sound.update()` every frame
        // (`SoundControl.java:89`); GDExtension nodes must enable `_process`
        // explicitly. Deferred: the node is mutably borrowed right now
        // (see the `ready()`-only invariant in `mind-gdext/AGENTS.md`).
        self.base_mut()
            .call_deferred("set_process", &[true.to_variant()]);
    }

    fn process(&mut self, delta: f64) {
        if !self.ready {
            return;
        }
        self.frames = self.frames.wrapping_add(1);
        if self.frames.is_multiple_of(30) {
            self.refresh_settings();
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as f64)
            .unwrap_or(0.0);
        self.now_ms = now_ms;
        let delta_frames = (delta * 60.0) as f32;

        // Lowpass tween (0.4 s, plan 18 §3.5).
        let rate = (delta as f32 / 0.4).max(0.0);
        if (self.lowpass_target - self.lowpass_wet).abs() <= rate {
            self.lowpass_wet = self.lowpass_target;
        } else if self.lowpass_target > self.lowpass_wet {
            self.lowpass_wet += rate;
        } else {
            self.lowpass_wet -= rate;
        }
        self.apply_lowpass();

        // `Core.audio.setPaused(soundBus.id, state.isPaused())`
        // (`SoundControl.java:151`).
        if let Some(host) = wiring::optional(&self.sim_host) {
            self.sound_paused = host.bind().is_paused();
        }

        // Pause mirror (Sound bus voices only; Music/UI unaffected).
        if let Some(pool) = &self.pool {
            pool.set_paused(self.sound_paused);
        }
        if let Some(loop_pool) = &self.loop_pool {
            loop_pool.set_paused(self.sound_paused);
        }

        // Plan-18 sink drain: fetch the sim log lazily (the `SimHost` may
        // finish `_ready` after `MindAudio`) and apply every queued event.
        if self.audio_log.is_none() {
            self.audio_log = wiring::optional(&self.sim_host).map(|host| host.bind().audio_log());
        }
        let events = self
            .audio_log
            .as_ref()
            .map(SharedAudioLog::take_events)
            .unwrap_or_default();
        // Mark every per-key `SoundLoop` unreported; both the event sink and
        // the per-frame turret/weapon scan re-report their live keys below.
        self.sound_loops.begin_frame();
        self.apply_events(&events, now_ms, delta_frames);

        let context = self.music_context(now_ms, delta_frames);

        // T7 `ResetEvent` mirror (`SoundControl.java:85-89`): the first in-game
        // frame, leaving the game, or a backwards sim tick (a fresh sector)
        // resets `lastPlayed` and stops the sound bus + loops.
        let in_game = !context.is_menu;
        let tick = wiring::optional(&self.sim_host)
            .map(|host| host.bind().get_tick())
            .unwrap_or(0);
        let in_game_changed = self.last_in_game != Some(in_game);
        if in_game_changed || tick < self.last_tick {
            self.music.last_played_ms = now_ms;
            if let Some(pool) = &self.pool {
                pool.stop_all();
            }
            if let Some(loop_pool) = &mut self.loop_pool {
                loop_pool.stop_all();
            }
            self.mixer = LoopMixer::new();
            self.sound_loops.clear();
            self.last_wave = None;
        }
        self.last_in_game = Some(in_game);
        self.last_tick = tick;

        // `WaveEvent` (`SoundControl.java:56-83`, `Control.java:123-130`):
        // `waveSpawn` plays on the UI bus and the delayed wave music is armed.
        if in_game {
            let wave = context.wave;
            if let Some(last) = self.last_wave
                && wave > last
            {
                self.music.on_wave_event(now_ms, &mut self.rng);
                self.play_sound("waveSpawn", self.sfx_scale(), 1.0, 0.0, true);
            }
            self.last_wave = Some(wave);
        }

        // T4: per-frame loop mixing (`SoundControl.updateLoops`,
        // `SoundControl.java:410-469`). Leaving the game stops every loop
        // voice instead of just clearing the data.
        let listener = self.listener();
        let avol = self.settings.ambientvol as f32 / 100.0;
        // T5: 20 Hz ambient source refresh (`AudioThread.update`,
        // `SoundControl.java:475-503`). The snapshot is only rebuilt when the
        // poll is due; the poller keeps the last aggregate rows between polls.
        let delta_ms = delta * 1000.0;
        let due = self.poller.accum_ms + delta_ms >= self.poller.interval_ms;
        if due && in_game {
            if let Some(host) = wiring::optional(&self.sim_host) {
                host.bind().ambient_snapshot(&mut self.ambient);
            } else {
                self.ambient.entries.clear();
            }
        }
        self.poller.update(
            delta_ms,
            context.is_menu,
            in_game && !context.paused,
            &self.ambient,
            listener,
        );

        // T6: per-frame loop sources (builder/miner/unit/tank/fire/weather/
        // tractor beam) and turret `SoundLoop` instances
        // (`Turret.updateTile`, `Turret.java:304,504`).
        if in_game && !context.paused {
            if let Some(host) = wiring::optional(&self.sim_host) {
                let host = host.bind();
                host.audio_frame_loops(&mut self.frame_loops);
                host.audio_turret_loops(&mut self.turret_loop_sources);
            } else {
                self.frame_loops.clear();
                self.turret_loop_sources.clear();
            }
            for source in &self.frame_loops {
                let (x, y) = match source.pos {
                    LoopPos::World(x, y) => (x, y),
                    LoopPos::Camera => (listener.x, listener.y),
                };
                self.mixer
                    .accumulate(source.sound, x, y, source.volume, source.pitch, listener);
            }
            // `SoundLoop` instances (turret + continuous weapon mounts):
            // volumes include `sfxvol` (Java `calcVolume`), unlike the
            // `SoundControl.loop` aggregation. The per-key table owns the
            // fade lifecycle; `sweep` stops keys that vanished this frame
            // (`remove()`/`onDestroyed` -> `soundLoop.stop()`).
            let sfx = self.sfx_scale();
            if let Some(pool) = self.loop_pool.as_mut() {
                let mut voice = GodotSoundLoopVoice {
                    pool,
                    streams: &mut self.streams,
                    listener,
                    sfx,
                };
                for source in &self.turret_loop_sources {
                    self.sound_loops.update(
                        SoundLoopInput {
                            key: source.key,
                            sound: source.sound,
                            base_volume: source.base_volume,
                            x: source.x,
                            y: source.y,
                            play: source.play,
                            volume_scl: source.volume_scl,
                            pitch: 1.0,
                            delta_frames,
                        },
                        listener,
                        &mut voice,
                    );
                }
                self.sound_loops.sweep(&mut voice);
            }
        } else if !self.sound_loops.is_empty() {
            // Leaving the game (or pausing into a teardown): stop every
            // `SoundLoop` voice.
            let sfx = self.sfx_scale();
            if let Some(pool) = self.loop_pool.as_mut() {
                let mut voice = GodotSoundLoopVoice {
                    pool,
                    streams: &mut self.streams,
                    listener,
                    sfx,
                };
                self.sound_loops.stop_all(&mut voice);
            } else {
                self.sound_loops.clear();
            }
            self.turret_loop_sources.clear();
        }

        if let Some((sound, x, y, volume, remaining)) = &mut self.debug_loop_hold
            && *remaining > 0
        {
            self.mixer
                .accumulate(*sound, *x, *y, *volume, 1.0, listener);
            *remaining -= 1;
        }
        if let Some(pool) = self.loop_pool.as_mut() {
            let mut out = GodotLoopOutput {
                pool,
                streams: &mut self.streams,
                listener,
            };
            if in_game {
                self.mixer
                    .update(avol, true, self.sound_paused, listener, &mut out);
            } else {
                self.mixer.stop_all(&mut out);
            }
        }
        self.mixer.merge_ambient(self.poller.last_output());

        if self.music_override {
            return;
        }

        let Some(player) = self.music_player.clone() else {
            return;
        };
        let mut output = GodotMusicOutput {
            player,
            streams: &mut self.streams,
            current: &mut self.current,
            lowpass_target: &mut self.lowpass_target,
            sound_paused: self.sound_paused,
        };
        self.music.update(&context, &mut self.rng, &mut output);
    }
}

impl MindAudio {
    /// Current camera listener (`calcFalloff`/`calcPan` frame).
    ///
    /// `Sound.calcPan` divides by `camera.width / 2` in **world units**
    /// (`ARC/.../Sound.java:166-170`); the viewport size is screen pixels, so
    /// divide by the camera zoom (T1).
    fn listener(&self) -> Listener {
        let viewport_width = self
            .base()
            .get_viewport()
            .map(|viewport| viewport.get_visible_rect().size.x)
            .filter(|width| *width > 0.0)
            .unwrap_or(1.0);
        let (x, y, zoom) = wiring::optional(&self.camera)
            .as_ref()
            .map(|camera| {
                let position = camera.get_global_position();
                let zoom = camera.get_zoom().x.abs().max(0.01);
                (position.x, position.y, zoom)
            })
            .unwrap_or((0.0, 0.0, 1.0));
        Listener::new(x, y, viewport_width / zoom)
    }

    /// Derives the plan-18 `MusicContext` from the live `MindSimHost` phase.
    ///
    /// `is_menu` also accepts a visible menu background (`Control` boots in
    /// `State::Playing`, so the sim state alone reports "game" at the main
    /// menu) and the planet dialog opened from the menu (it hides the
    /// `MenuBackground` while `HudGroup` stays hidden; PAR-0072). `dialog` is
    /// `state.isGame() && Core.scene.hasDialog()` (`SoundControl.java:120,130-132`).
    fn music_context(&self, now_ms: f64, delta_frames: f32) -> MusicContext {
        let (state, paused, game) = match wiring::optional(&self.sim_host) {
            Some(host) => {
                let host = host.bind();
                (
                    host.get_state().to_string(),
                    host.is_paused(),
                    host.audio_game_state(),
                )
            }
            None => (String::from("menu"), false, AudioGameState::default()),
        };
        let menu_background = self
            .menu_background
            .as_ref()
            .is_some_and(|background| background.is_visible());
        // Read the dialog stack once per frame.
        let stack = self.dialog_stack();
        let hud_visible = self.hud_group_visible();
        let planet_dialog = stack.iter().any(|name| name == "planet");
        let is_menu = state == "menu" || menu_background || (planet_dialog && !hud_visible);
        MusicContext {
            is_menu,
            is_game: !is_menu,
            paused,
            dialog: !is_menu && !stack.is_empty(),
            editor_rules: game.editor,
            planet_ui_visible: is_menu && planet_dialog,
            editor_ui_visible: is_menu && self.editor_dialog_visible(),
            // `Planet.launchMusic = Musics.launch` (`Planet.java:143`).
            planet_music: PlanetMusic {
                launch: Some(MusicRef::new("launch")),
                ..PlanetMusic::default()
            },
            rules_music: game.rules.clone(),
            core_hp_fraction: game.core_hp_fraction,
            wave: game.wave,
            enemies: game.enemies,
            live_boss: game.live_boss,
            boss_spawn_group: game.boss_spawn_group,
            musicvol_setting: self.settings.musicvol,
            always_music_setting: self.settings.alwaysmusic,
            now_ms,
            delta_frames,
        }
    }

    /// `Core.scene.hasDialog()` stack names (`MindUi.dialog_stack`).
    fn dialog_stack(&self) -> Vec<String> {
        let Some(mut ui) = self.base().try_get_node_as::<GdNode>("/root/MindUi") else {
            return Vec::new();
        };
        if !ui.has_method("dialog_stack") {
            return Vec::new();
        }
        ui.call("dialog_stack", &[])
            .try_to::<PackedStringArray>()
            .map(|stack| {
                stack
                    .as_slice()
                    .iter()
                    .map(|name| name.to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// `Ui/UiRoot/HudGroup.visible` (hidden at the main menu).
    fn hud_group_visible(&self) -> bool {
        self.ui_root
            .as_ref()
            .and_then(|root| root.bind().hud_group_node())
            .is_some_and(|group| group.is_visible())
    }

    /// `Ui/EditorDialog.visible` (`ui.editor.isShown()`).
    fn editor_dialog_visible(&self) -> bool {
        self.editor_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.is_visible())
    }

    /// Pulls the sound settings from the `MindUi` store (`Core.settings` is
    /// one global store, `ARC/.../Settings.java`). Dynamic `call` on purpose:
    /// `MindUi` is an autoload this module does not bind by type.
    fn refresh_settings(&mut self) {
        let Some(mut ui) = self.base().get_node_or_null("/root/MindUi") else {
            return;
        };
        let int = |ui: &mut Gd<GdNode>, key: &str| {
            ui.call("settings_get_i32", &[GString::from(key).to_variant()])
                .try_to::<i64>()
                .unwrap_or(100) as i32
        };
        self.settings.musicvol = int(&mut ui, settings_keys::MUSIC_VOLUME).clamp(0, 100);
        self.settings.sfxvol = int(&mut ui, settings_keys::SFX_VOLUME).clamp(0, 100);
        self.settings.ambientvol = int(&mut ui, settings_keys::AMBIENT_VOLUME).clamp(0, 100);
        self.settings.alwaysmusic = ui
            .call(
                "settings_get_bool",
                &[GString::from(settings_keys::ALWAYS_MUSIC).to_variant()],
            )
            .try_to::<bool>()
            .unwrap_or(false);
    }

    fn sfx_scale(&self) -> f32 {
        self.settings.sfxvol as f32 / 100.0
    }

    /// `Sound.at(x, y, pitch, volume)` on the Sound bus: per-sound
    /// `falloffOffset` falloff, `sfxvol`, pan and the 16 ms `checkFrame` gate.
    fn play_at_world(&mut self, name: &str, x: f32, y: f32, pitch: f32, volume: f32) {
        let listener = self.listener();
        let offset = SoundId::by_name(name)
            .map(|id| self.table.spec(id).falloff_offset)
            .unwrap_or(0.0);
        let scaled = calc_falloff(x, y, listener, offset) * self.sfx_scale() * volume;
        if scaled < MIN_PLAY_VOLUME {
            return;
        }
        let pan = calc_pan(x, listener);
        self.play_sound(name, scaled, pitch, pan, true);
    }

    /// Applies the drained sim events (plan 18 §3.3): one-shots to the voice
    /// pool, loop aggregation into the `LoopMixer`, per-key `SoundLoop` fade
    /// voices into [`SoundLoopTable`], and music control through the
    /// `MusicPlayer`.
    fn apply_events(&mut self, events: &[TickedAudioEvent], now_ms: f64, delta_frames: f32) {
        if events.is_empty() {
            return;
        }
        let listener = self.listener();
        let sfx = self.sfx_scale();
        for ticked in events {
            self.events_applied = self.events_applied.wrapping_add(1);
            match &ticked.event {
                AudioEvent::At {
                    sound,
                    x,
                    y,
                    pitch,
                    volume,
                    check_frame,
                } => {
                    // `Sound.falloffOffset` (`explosionCore` 100 px, T1).
                    let spec = self.table.spec(*sound);
                    let scaled = calc_falloff(*x, *y, listener, spec.falloff_offset)
                        * self.sfx_scale()
                        * *volume;
                    if scaled < MIN_PLAY_VOLUME {
                        continue;
                    }
                    self.play_sound(
                        sound.name(),
                        scaled,
                        *pitch,
                        calc_pan(*x, listener),
                        *check_frame,
                    );
                }
                AudioEvent::Play {
                    sound,
                    volume,
                    pitch,
                    pan,
                    check_frame,
                    ..
                } => {
                    // `Sound.play` call sites pass `volume * sfxVolume`
                    // (`CoreBlock.java:712`, `LExecutor.java:2256`); the sim
                    // stores the raw volume (T3).
                    self.play_sound(
                        sound.name(),
                        *volume * self.sfx_scale(),
                        *pitch,
                        *pan,
                        *check_frame,
                    );
                }
                AudioEvent::LoopAdd {
                    sound,
                    x,
                    y,
                    volume,
                    pitch,
                } => {
                    self.mixer
                        .accumulate(*sound, *x, *y, *volume, *pitch, listener);
                }
                AudioEvent::LoopInstance {
                    key,
                    sound,
                    x,
                    y,
                    play,
                    volume_scl,
                } => {
                    // `SoundLoop.update(x, y, play, volumeScl)` per instance:
                    // the event carries no constructor `baseVolume`, so the
                    // `new SoundLoop(sound, 1f)` default (continuous weapon
                    // mounts, `Weapon.java:392`) applies. Keep the per-key
                    // fade voice instead of folding it into the aggregate mix.
                    if let Some(pool) = self.loop_pool.as_mut() {
                        let mut voice = GodotSoundLoopVoice {
                            pool,
                            streams: &mut self.streams,
                            listener,
                            sfx,
                        };
                        self.sound_loops.update(
                            SoundLoopInput {
                                key: *key,
                                sound: *sound,
                                base_volume: 1.0,
                                x: *x,
                                y: *y,
                                play: *play,
                                volume_scl: *volume_scl,
                                pitch: 1.0,
                                delta_frames,
                            },
                            listener,
                            &mut voice,
                        );
                    }
                }
                AudioEvent::LoopCamera { sound, volume } => {
                    self.mixer
                        .accumulate(*sound, listener.x, listener.y, *volume, 1.0, listener);
                }
                AudioEvent::MusicPlay { name, interrupt } => {
                    let context = self.music_context(now_ms, delta_frames);
                    let Some(player) = self.music_player.clone() else {
                        continue;
                    };
                    let mut output = GodotMusicOutput {
                        player,
                        streams: &mut self.streams,
                        current: &mut self.current,
                        lowpass_target: &mut self.lowpass_target,
                        sound_paused: self.sound_paused,
                    };
                    self.music
                        .play_music(Some(name.clone()), *interrupt, &context, &mut output);
                    self.music_override = true;
                }
                AudioEvent::MusicStop => self.stop_music(),
                AudioEvent::KeepSilent => self.music.keep_silent(),
                AudioEvent::StopLoops => {
                    // `ResetEvent` teardown: stop every loop voice.
                    if let Some(loop_pool) = &mut self.loop_pool {
                        loop_pool.stop_all();
                    }
                    self.sound_loops.clear();
                    self.mixer = LoopMixer::new();
                }
                AudioEvent::Construct {
                    sound,
                    x,
                    y,
                    place,
                    pitch_change,
                } => {
                    // `ConstructBlock.shouldPlay` gates on the call, before
                    // `calcPitch` (`ConstructBlock.java:58-120`).
                    if now_ms - self.construct_last_played_ms < 32.0 {
                        continue;
                    }
                    self.construct_last_played_ms = now_ms;
                    let pitch = if !pitch_change {
                        1.0
                    } else if now_ms - self.construct_last_time_ms < 16.0 * 30.0 {
                        self.construct_last_time_ms = now_ms;
                        self.construct_pitch_seq += 1;
                        if self.construct_pitch_seq > 30 {
                            self.construct_pitch_seq = 0;
                        }
                        1.0 + (self.construct_pitch_seq as f32 / 30.0).clamp(0.0, 1.0)
                            * if *place { 1.9 } else { -0.4 }
                    } else {
                        self.construct_pitch_seq = 0;
                        self.construct_last_time_ms = now_ms;
                        0.7 + self.rng.next_f32() * 0.6
                    };
                    let scaled =
                        calc_falloff(*x, *y, listener, self.table.spec(*sound).falloff_offset)
                            * self.sfx_scale();
                    if scaled >= MIN_PLAY_VOLUME {
                        self.play_sound(sound.name(), scaled, pitch, calc_pan(*x, listener), true);
                    }
                }
            }
        }
    }

    /// Plays one one-shot through the UI or world voice pool.
    ///
    /// World voices run Arc `Sound.play` `checkFrame` intensification plus
    /// SoLoud admission (`OneShotPlan`); the UI pool keeps its 16 voices
    /// round-robin and only the 16 ms replay window applies (no intensify
    /// slot bookkeeping there).
    fn play_sound(&mut self, name: &str, volume: f32, pitch: f32, pan: f32, check_frame: bool) {
        let file = self.streams.file_of(name).map(str::to_owned);
        let Some(stream) = self.streams.sound(name, false) else {
            self.log_missing(name);
            return;
        };
        let Some(id) = SoundId::by_name(name) else {
            self.log_missing(name);
            return;
        };
        let now = self.now_ms;
        if bus_for_sound(name, file.as_deref()) == mind_core::audio::BusKind::Ui {
            let state = self.play_state.entry(id).or_default();
            if check_frame && state.within_min_interval(now, MIN_INTERVAL_MS) {
                return;
            }
            let Some(pool) = self.pool.as_mut() else {
                return;
            };
            let voice = pool.play_ui(stream, volume, pitch);
            state.record(voice as u64, volume, now);
            return;
        }

        let spec = self.table.spec(id);
        let length = stream.get_length() as f32;
        if let Some(pool) = self.pool.as_ref() {
            pool.active_voices(&mut self.active_scratch);
        } else {
            self.active_scratch.clear();
        }
        let plan = {
            let state = self.play_state.entry(id).or_default();
            plan_one_shot(
                state,
                &self.active_scratch,
                &spec,
                id,
                volume,
                length,
                now,
                check_frame,
            )
        };
        match plan {
            OneShotPlan::Skip | OneShotPlan::Deny => {}
            OneShotPlan::Intensify { voice, volume } => {
                // `Core.audio.set(lastVoice, pan, lastVolume)`.
                let listener = self.listener();
                if let Some(pool) = self.pool.as_mut() {
                    pool.set_voice_volume(
                        voice,
                        volume,
                        pan,
                        (listener.x, listener.y),
                        listener.width,
                    );
                }
            }
            OneShotPlan::Play | OneShotPlan::Replace { .. } => {
                let replace = match plan {
                    OneShotPlan::Replace { voice } => Some(voice),
                    _ => None,
                };
                let listener = self.listener();
                if let Some(pool) = self.pool.as_mut()
                    && let Some(new_id) = pool.play_world_meta(
                        replace,
                        stream,
                        volume,
                        pitch,
                        pan,
                        (listener.x, listener.y),
                        listener.width,
                        (
                            id,
                            now,
                            spec.priority,
                            spec.group,
                            spec.min_interrupt(length),
                        ),
                    )
                {
                    self.play_state
                        .entry(id)
                        .or_default()
                        .record(new_id, volume, now);
                }
            }
        }
    }

    fn apply_lowpass(&mut self) {
        let mut server = AudioServer::singleton();
        if let Some(mut filter) = buses::sound_lowpass(&mut server, self.bus.sound) {
            let cutoff = LOWPASS_DRY_HZ + (LOWPASS_WET_HZ - LOWPASS_DRY_HZ) * self.lowpass_wet;
            filter.set_cutoff(cutoff);
        }
    }

    fn log_missing(&mut self, name: &str) {
        if self.missing.iter().any(|existing| existing == name) {
            return;
        }
        self.missing.push(name.to_owned());
        let message = format!("[E] missing audio {name}");
        log::error!("{message}");
        self.log.push(message);
    }
}

/// `MusicOutput` backed by the pooled Music-bus player.
struct GodotMusicOutput<'a> {
    player: Gd<AudioStreamPlayer>,
    streams: &'a mut StreamCache,
    current: &'a mut Option<MusicRef>,
    lowpass_target: &'a mut f32,
    sound_paused: bool,
}

impl MusicOutput for GodotMusicOutput<'_> {
    fn play(&mut self, track: &MusicRef, volume: f32, looping: bool) {
        if let Some(stream) = self.streams.music(track.name()) {
            self.player.set_stream(&stream);
        }
        // `playOnce` plays non-looping so the finished-track detector clears
        // `current`; `play` (menu/editor fade) loops (`SoundControl.java:288,327`).
        if let Some(stream) = self.player.get_stream() {
            if let Ok(mut ogg) = stream
                .clone()
                .try_cast::<godot::classes::AudioStreamOggVorbis>()
            {
                ogg.set_loop(looping);
            } else if let Ok(mut mp3) = stream.try_cast::<godot::classes::AudioStreamMp3>() {
                mp3.set_loop(looping);
            }
        }
        self.player.set_volume_linear(volume.clamp(0.0, 1.0));
        self.player.play();
        *self.current = Some(track.clone());
    }

    fn set_volume(&mut self, volume: f32) {
        self.player.set_volume_linear(volume.clamp(0.0, 1.0));
    }

    fn stop(&mut self) {
        self.player.stop();
        *self.current = None;
    }

    fn is_playing(&self) -> bool {
        self.player.is_playing()
    }

    fn fade_filter(&mut self, wet: f32, _duration_s: f32) {
        *self.lowpass_target = wet;
    }

    fn set_paused(&mut self, _paused: bool) {
        // The pause mirror is applied to the voice pool, not the music voice.
        let _ = self.sound_paused;
    }

    fn on_game_state_change(&mut self, _playing: bool) {
        // `SoundControl.update`'s game-state branch (`SoundControl.java:135-151`)
        // resets the **Sound** bus only (`soundBus.stop()` / `play()` +
        // `musicBus.play()`); it never stops the music player. Leaving a game
        // must reach the `isMenu()` branch (`:156`) and let the menu track's
        // `play()` fade the outgoing track out (`SoundControl.java:263-311`).
        // The port's sound-bus reset (`pool.stop_all` + loop stops) lives in
        // `MindAudio::process`; this hook stays music-neutral.
    }
}

/// `LoopOutput` backed by the pooled looping 2D voices.
struct GodotLoopOutput<'a> {
    pool: &'a mut LoopVoicePool,
    streams: &'a mut StreamCache,
    listener: Listener,
}

impl LoopOutput for GodotLoopOutput<'_> {
    fn start_loop(&mut self, sound: SoundId, volume: f32, pitch: f32, pan: f32) -> u64 {
        match self.streams.sound(sound.name(), true) {
            Some(stream) => self.pool.start(
                stream,
                volume,
                pitch,
                pan,
                (self.listener.x, self.listener.y),
                self.listener.width,
            ),
            None => 0,
        }
    }

    fn update_loop(&mut self, voice: u64, volume: f32, pitch: f32, pan: f32) {
        self.pool.update(
            voice,
            volume,
            pitch,
            pan,
            (self.listener.x, self.listener.y),
            self.listener.width,
        );
    }

    fn stop_loop(&mut self, voice: u64) {
        self.pool.stop(voice);
    }

    fn is_playing(&self, voice: u64) -> bool {
        voice != 0 && self.pool.is_playing(voice)
    }
}

/// [`SoundLoopVoice`] backed by the pooled looping 2D voices.
///
/// Applies the `sfxvol` multiplier the existing `SoundLoop` path used (Java
/// `Sound.calcVolume`); the per-key table itself stays pure
/// `SoundLoop` math so it can be unit-tested without a Godot runtime.
struct GodotSoundLoopVoice<'a> {
    pool: &'a mut LoopVoicePool,
    streams: &'a mut StreamCache,
    listener: Listener,
    sfx: f32,
}

impl SoundLoopVoice for GodotSoundLoopVoice<'_> {
    fn start(&mut self, sound: SoundId, volume: f32, pitch: f32, pan: f32) -> u64 {
        match self.streams.sound(sound.name(), true) {
            Some(stream) => self.pool.start(
                stream,
                volume * self.sfx,
                pitch,
                pan,
                (self.listener.x, self.listener.y),
                self.listener.width,
            ),
            None => 0,
        }
    }

    fn update(&mut self, voice: u64, volume: f32, pitch: f32, pan: f32) {
        self.pool.update(
            voice,
            volume * self.sfx,
            pitch,
            pan,
            (self.listener.x, self.listener.y),
            self.listener.width,
        );
    }

    fn stop(&mut self, voice: u64) {
        self.pool.stop(voice);
    }
}

#[godot_api]
impl MindAudio {
    /// Whether the audio driver finished `_ready`.
    #[func]
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Bus index by name (`-1` when missing).
    #[func]
    pub fn bus_index(&self, name: GString) -> i32 {
        AudioServer::singleton().get_bus_index(&StringName::from(&name))
    }

    /// Bus volume in dB.
    #[func]
    pub fn bus_volume_db(&self, name: GString) -> f64 {
        let server = AudioServer::singleton();
        let index = server.get_bus_index(&StringName::from(&name));
        if index < 0 {
            return 0.0;
        }
        server.get_bus_volume_db(index) as f64
    }

    /// Current music track name (`""` when none).
    #[func]
    pub fn current_track(&self) -> GString {
        self.current
            .as_ref()
            .map(|track| GString::from(track.name()))
            .unwrap_or_default()
    }

    /// Whether a music track is currently playing.
    #[func]
    pub fn current_track_playing(&self) -> bool {
        self.music_player
            .as_ref()
            .is_some_and(|player| player.is_playing())
    }

    /// Number of data-asset streams registered by the PAR-0394 decorator.
    #[func]
    pub fn data_audio_count(&self) -> i64 {
        self.streams.data_stream_count() as i64
    }

    /// Whether a `dp-<name>` data-asset stream is registered.
    #[func]
    pub fn has_data_audio(&self, name: GString) -> bool {
        self.streams.is_data_stream(&name.to_string())
    }

    /// T7 inspector: in-game music context state.
    #[func]
    pub fn music_debug(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        let game = wiring::optional(&self.sim_host)
            .map(|host| host.bind().audio_game_state())
            .unwrap_or_default();
        let context = self.music_context(0.0, 1.0);
        dict.set(&key("wave"), &(game.wave as i64).to_variant());
        dict.set(
            &key("core_hp"),
            &(game.core_hp_fraction.unwrap_or(-1.0) as f64).to_variant(),
        );
        dict.set(&key("live_boss"), &game.live_boss.to_variant());
        dict.set(
            &key("last_played_ms"),
            &self.music.last_played_ms.to_variant(),
        );
        dict.set(
            &key("wave_music_due_ms"),
            &self.music.wave_music_due_ms.unwrap_or(-1.0).to_variant(),
        );
        dict.set(&key("current"), &self.current_track().to_variant());
        dict.set(
            &key("in_game"),
            &self.last_in_game.unwrap_or(false).to_variant(),
        );
        dict.set(&key("land_playing"), &self.land_playing().to_variant());
        // T10: menu/dialog selection inputs.
        dict.set(&key("is_menu"), &context.is_menu.to_variant());
        dict.set(
            &key("planet_ui_visible"),
            &context.planet_ui_visible.to_variant(),
        );
        dict.set(
            &key("editor_ui_visible"),
            &context.editor_ui_visible.to_variant(),
        );
        dict
    }

    /// Whether the landing music voice is playing (T7 inspector).
    #[func]
    pub fn land_playing(&self) -> bool {
        self.land_player
            .as_ref()
            .is_some_and(|player| player.is_playing())
    }

    /// T7: `Trigger.newGame` landing cue (`Control.java:198-222`) —
    /// `coreLand.at(camera.position)` (`CoreBlock.java:375-376`) plus
    /// `Musics.land` at `musicvol / 100`.
    #[func]
    pub fn play_landing(&mut self) {
        self.play_sound("coreLand", self.sfx_scale(), 1.0, 0.0, true);
        if self.settings.musicvol > 0
            && let Some(player) = self.land_player.as_mut()
            && let Some(stream) = self.streams.music("land")
        {
            player.set_stream(&stream);
            player.set_volume_linear(self.settings.musicvol as f32 / 100.0);
            player.play();
        }
    }

    /// Java `BurstDrill` burst cue (`BurstDrill.java:116`):
    /// `drillSound.at(x, y, 1f + range(0.1), 0.6)` with `Sounds.drillImpact`.
    ///
    /// The burst trigger lives in `mind-core`'s BurstDrill update; this is its
    /// audio half (`Effect.shake` and `drillEffect` stay render-side).
    #[func]
    pub fn play_burstdrill_burst(&mut self, x: f64, y: f64) {
        let pitch = hooks::pitch_with_rand(&mut self.rng, hooks::BURST_DRILL_PITCH_RAND);
        self.play_at_world(
            "drillImpact",
            x as f32,
            y as f32,
            pitch,
            hooks::BURST_DRILL_SOUND_VOLUME,
        );
    }

    /// `BurstDrill.ambientVolume()` audio half
    /// (`BurstDrill.java:123-124`): `ambientSoundVolume * ambientVolume() *
    /// pow(progress, 4)`. The ambient snapshot is assembled in `mind-core`
    /// (`audio/ambient.rs`); exposed for the pending core hook and the twin
    /// harness.
    #[func]
    pub fn burstdrill_ambient_volume(&self, progress: f64, base_volume: f64) -> f64 {
        f64::from(hooks::burst_drill_ambient_factor(progress as f32)) * base_volume
    }

    /// Java `LaunchPad` launch cue (`LaunchPad.java:149`):
    /// `launchSound.at(x, y, 1f + range(0.1))` with `Sounds.padLaunch`.
    ///
    /// The launch trigger and payload release hook live in `mind-core`'s
    /// LaunchPad behavior; this is its audio half.
    #[func]
    pub fn play_launch_pad(&mut self, x: f64, y: f64) {
        let pitch = hooks::pitch_with_rand(&mut self.rng, hooks::LAUNCH_PAD_PITCH_RAND);
        self.play_at_world("padLaunch", x as f32, y as f32, pitch, 1.0);
    }

    /// Per-key `SoundLoop` count (inspector; `SoundLoop.java` instances).
    #[func]
    pub fn sound_loop_count(&self) -> i64 {
        self.sound_loops.len() as i64
    }

    /// Live voice id for a per-key `SoundLoop` (`-1` when none; inspector).
    #[func]
    pub fn sound_loop_voice(&self, key: i64) -> i64 {
        self.sound_loops
            .voice(key as u64)
            .map_or(-1, |voice| voice as i64)
    }

    /// Current per-key `SoundLoop` fade volume (`0..1`, `0` when unknown;
    /// inspector).
    #[func]
    pub fn sound_loop_fade(&self, key: i64) -> f64 {
        self.sound_loops
            .fade_volume(key as u64)
            .map_or(0.0, f64::from)
    }

    /// Forces a track by name (`findMusic`-style), optionally interrupting.
    #[func]
    pub fn force_music(&mut self, name: GString, interrupt: bool) -> bool {
        let reference = MusicRef::new(name.to_string());
        let Some(player) = self.music_player.clone() else {
            return false;
        };
        let mut output = GodotMusicOutput {
            player,
            streams: &mut self.streams,
            current: &mut self.current,
            lowpass_target: &mut self.lowpass_target,
            sound_paused: self.sound_paused,
        };
        let context = MusicContext {
            musicvol_setting: self.settings.musicvol,
            ..MusicContext::default()
        };
        self.music
            .play_music(Some(reference), interrupt, &context, &mut output);
        self.music_override = true;
        self.current.is_some()
    }

    /// Stops music and clears the override.
    #[func]
    pub fn stop_music(&mut self) {
        self.music_override = false;
        if let Some(player) = &self.music_player {
            player.clone().stop();
        }
        self.current = None;
    }

    /// `SoundControl.keepSilent`.
    #[func]
    pub fn keep_silent(&mut self) {
        self.music.keep_silent();
    }

    /// Synthetic lowpass wet value (`0..1`).
    #[func]
    pub fn lowpass_wet(&self) -> f64 {
        self.lowpass_wet as f64
    }

    /// Current `Sound`-bus lowpass cutoff in Hz.
    #[func]
    pub fn lowpass_cutoff(&self) -> f64 {
        (LOWPASS_DRY_HZ + (LOWPASS_WET_HZ - LOWPASS_DRY_HZ) * self.lowpass_wet) as f64
    }

    /// Whether the Sound-bus voices are paused.
    #[func]
    pub fn sound_bus_paused(&self) -> bool {
        self.sound_paused
    }

    /// Active one-shot voice count.
    #[func]
    pub fn voice_count(&self) -> i64 {
        self.pool.as_ref().map_or(0, |pool| pool.active() as i64)
    }

    /// T1: sound name -> number of playing world voices (policy-visible only).
    #[func]
    pub fn active_voice_counts(&self) -> VarDictionary {
        let mut counts: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
        if let Some(pool) = self.pool.as_ref() {
            let mut voices = Vec::with_capacity(64);
            pool.active_voices(&mut voices);
            for voice in &voices {
                *counts.entry(voice.sound.name().to_owned()).or_insert(0) += 1;
            }
        }
        let mut dict = VarDictionary::new();
        for (name, count) in counts {
            dict.set(&GString::from(name.as_str()), &count.to_variant());
        }
        dict
    }

    /// Aggregated loop volume for a sound name.
    #[func]
    pub fn loop_volume(&self, sound: GString) -> f64 {
        SoundId::by_name(&sound.to_string())
            .and_then(|id| self.mixer.data(id))
            .map_or(0.0, |data| data.cur_volume as f64)
    }

    /// Whether a loop voice is live for a sound name.
    #[func]
    pub fn loop_voice_count(&self, sound: GString) -> i64 {
        SoundId::by_name(&sound.to_string())
            .and_then(|id| self.mixer.data(id))
            .map_or(0, |data| i64::from(data.voice.is_some()))
    }

    /// Un-drained events still queued in the sim audio sink.
    #[func]
    pub fn pending_events(&self) -> i64 {
        self.audio_log.as_ref().map_or(0, |log| log.len() as i64)
    }

    /// Missing-audio + error log lines.
    #[func]
    pub fn drain_log(&self) -> PackedStringArray {
        let mut out = PackedStringArray::new();
        for line in &self.log {
            out.push(line);
        }
        out
    }

    /// Plays a data-asset sound/music (`dp-<name>`, PAR-0394). The listener
    /// half of `MapAudioView`: sounds go to the UI pool, music to the Music
    /// player (replacing the track, like Java's `Music.play`).
    #[func]
    pub fn play_data_audio(&mut self, name: GString, music: bool) -> bool {
        let name = name.to_string();
        if music {
            let Some(mut player) = self.music_player.clone() else {
                return false;
            };
            let Some(stream) = self.streams.music(&name) else {
                return false;
            };
            player.set_stream(&stream);
            player.set_volume_linear(self.settings.musicvol as f32 / 100.0);
            player.play();
            self.music_override = true;
            self.current = Some(MusicRef::new(name));
            true
        } else {
            let Some(stream) = self.streams.sound(&name, false) else {
                return false;
            };
            let sfx = self.sfx_scale();
            let Some(pool) = self.pool.as_mut() else {
                return false;
            };
            pool.play_ui(stream, sfx, 1.0) != 0
        }
    }

    /// Plays a one-shot on the UI bus (or the world pool for non-UI sounds).
    #[func]
    pub fn play_oneshot(&mut self, sound: GString, volume: f64, pitch: f64, pan: f64) -> i64 {
        let name = sound.to_string();
        let Some(id) = SoundId::by_name(&name) else {
            self.log_missing(&name);
            return -1;
        };
        let file = self.streams.file_of(&name).map(str::to_owned);
        let Some(stream) = self.streams.sound(&name, false) else {
            self.log_missing(&name);
            return -1;
        };
        let Some(pool) = self.pool.as_mut() else {
            return -1;
        };
        let _ = id;
        if bus_for_sound(&name, file.as_deref()) == mind_core::audio::BusKind::Ui {
            pool.play_ui(stream, volume as f32, pitch as f32)
        } else {
            pool.play_world(
                stream,
                volume as f32,
                pitch as f32,
                pan as f32,
                (0.0, 0.0),
                1.0,
            )
        }
    }

    /// T13: `MindUi.play_ui_sound` listener (`UI.java:146` — every button
    /// click; dialog back/notify/unlock call sites route through it).
    #[func]
    pub fn on_ui_sound(&mut self, name: GString) {
        let volume = self.sfx_scale() as f64;
        let _ = self.play_oneshot(name, volume, 1.0, 0.0);
    }

    /// `PlaySoundStatement.previewSound`: play a sound picker row on the UI bus
    /// (`sound.play(..., control.sound.uiBus)`) and stop the previous preview.
    #[func]
    pub fn preview_sound(&mut self, name: GString, volume: f64) -> bool {
        self.stop_sound_preview();
        let name = name.to_string();
        let Some(stream) = self.streams.sound(&name, false) else {
            self.log_missing(&name);
            return false;
        };
        let scaled = (volume as f32) * self.sfx_scale();
        let Some(pool) = self.pool.as_mut() else {
            return false;
        };
        let voice = pool.play_ui(stream, scaled, 1.0);
        self.preview_voice = Some(voice);
        true
    }

    /// Stops the picker preview (`showSoundSelect`'s hide callback).
    #[func]
    pub fn stop_sound_preview(&mut self) {
        if let Some(voice) = self.preview_voice.take()
            && let Some(pool) = self.pool.as_ref()
        {
            pool.stop_ui(voice);
        }
    }

    /// T13: UI sound with an explicit volume/pitch (`HudFragment` favourite:
    /// `uiFavorite.play(0.35, 0.67, 0)` / `play(0.6, 2.0, 0)`).
    #[func]
    pub fn on_ui_sound_ex(&mut self, name: GString, volume: f64, pitch: f64) {
        let scaled = volume * self.sfx_scale() as f64;
        let _ = self.play_oneshot(name, scaled, pitch, 0.0);
    }

    /// T14: positional one-shot forced onto the UI pool
    /// (`configureSound.at(build.x, build.y, 1, 1, control.sound.uiBus)`).
    #[func]
    pub fn play_at_ui(&mut self, name: GString, x: f64, y: f64, volume: f64, pitch: f64) {
        let name = name.to_string();
        let listener = self.listener();
        // `Sound.at` with the per-sound `falloffOffset` (T1).
        let offset = SoundId::by_name(&name)
            .map(|id| self.table.spec(id).falloff_offset)
            .unwrap_or(0.0);
        let scaled =
            calc_falloff(x as f32, y as f32, listener, offset) * self.sfx_scale() * volume as f32;
        if scaled < MIN_PLAY_VOLUME {
            return;
        }
        let Some(stream) = self.streams.sound(&name, false) else {
            self.log_missing(&name);
            return;
        };
        if let Some(pool) = self.pool.as_mut() {
            let _ = pool.play_ui(stream, scaled, pitch as f32);
        }
    }

    /// Debug loop injection (plan 18 §7c step 7).
    #[func]
    pub fn debug_emit_loop(&mut self, sound: GString, x: f64, y: f64, volume: f64) {
        if let Some(id) = SoundId::by_name(&sound.to_string()) {
            let listener = self.listener();
            self.mixer
                .accumulate(id, x as f32, y as f32, volume as f32, 1.0, listener);
        }
    }

    /// Test seam: re-accumulates a loop source for `frames` frames
    /// (audio can't be asserted any other way; `Call.createWeather`-style).
    #[func]
    pub fn debug_hold_loop(&mut self, sound: GString, x: f64, y: f64, volume: f64, frames: i64) {
        if let Some(id) = SoundId::by_name(&sound.to_string()) {
            self.debug_loop_hold = Some((id, x as f32, y as f32, volume as f32, frames));
        }
    }

    /// Inspector stats (`plan 18 §3.10`).
    #[func]
    pub fn stats(&self) -> VarDictionary {
        let mut dict = VarDictionary::new();
        let key = |name: &str| GString::from(name);
        dict.set(&key("frames"), &(self.frames as i64).to_variant());
        dict.set(
            &key("ambient_sources"),
            &(self.ambient.entries.len() as i64).to_variant(),
        );
        dict.set(&key("voices"), &(self.voice_count().to_variant()));
        dict.set(&key("loop_sounds"), &(self.mixer.len() as i64).to_variant());
        dict.set(
            &key("sound_loops"),
            &(self.sound_loops.len() as i64).to_variant(),
        );
        dict.set(&key("events"), &(self.pending_events().to_variant()));
        dict.set(
            &key("events_applied"),
            &(self.events_applied as i64).to_variant(),
        );
        dict.set(&key("allocs"), &0i64.to_variant());
        dict.set(
            &key("tracks_played"),
            &(self.music.last_random_played.is_some() as i64).to_variant(),
        );
        dict.set(&key("current_track"), &self.current_track().to_variant());
        dict.set(&key("lowpass_wet"), &self.lowpass_wet.to_variant());
        dict.set(&key("sound_paused"), &self.sound_paused.to_variant());
        dict.set(&key("audition"), &self.audition.to_variant());
        dict.set(&key("musicvol"), &self.settings.musicvol.to_variant());
        dict.set(&key("sfxvol"), &self.settings.sfxvol.to_variant());
        dict.set(&key("ambientvol"), &self.settings.ambientvol.to_variant());
        dict
    }

    /// Sets a plan-18 setting (`musicvol`/`sfxvol`/`ambientvol`/`alwaysmusic`).
    #[func]
    pub fn set_setting(&mut self, name: GString, value: Variant) {
        match name.to_string().as_str() {
            settings_keys::MUSIC_VOLUME => {
                self.settings.musicvol = value.try_to::<i32>().unwrap_or(100).clamp(0, 100);
            }
            settings_keys::SFX_VOLUME => {
                self.settings.sfxvol = value.try_to::<i32>().unwrap_or(100).clamp(0, 100);
            }
            settings_keys::AMBIENT_VOLUME => {
                self.settings.ambientvol = value.try_to::<i32>().unwrap_or(100).clamp(0, 100);
            }
            settings_keys::ALWAYS_MUSIC => {
                self.settings.alwaysmusic = value.try_to::<bool>().unwrap_or(false);
            }
            _ => {}
        }
    }

    /// Sets the Sound-bus pause pause mirror (plan 18 §7c step 5).
    #[func]
    pub fn set_sound_paused(&mut self, paused: bool) {
        self.sound_paused = paused;
    }

    /// Starts an editor audition (`MapAudioView`); `dp-` prefix falls back to the
    /// bare name until plan 20's `DataAudioLoader` overlay lands.
    #[func]
    pub fn audition_play(&mut self, name: GString) -> bool {
        let raw = name.to_string();
        let candidate = raw.strip_prefix("dp-").unwrap_or(raw.as_str()).to_owned();
        let Some(stream) = self.streams.music(&candidate) else {
            self.log_missing(&candidate);
            return false;
        };
        self.stop_music();
        if let Some(player) = &self.music_player {
            let mut player = player.clone();
            player.set_stream(&stream);
            player.set_volume_linear(1.0);
            player.play();
        }
        self.audition = true;
        self.music_override = true;
        true
    }

    /// Stops an editor audition.
    #[func]
    pub fn audition_stop(&mut self) {
        self.audition = false;
        self.music_override = false;
        if let Some(player) = &self.music_player {
            player.clone().stop();
        }
    }

    /// Whether an audition is playing.
    #[func]
    pub fn audition_playing(&self) -> bool {
        self.music_player.as_ref().is_some_and(|p| p.is_playing())
    }

    /// Audition playback position (seconds).
    #[func]
    pub fn audition_position(&self) -> f64 {
        self.music_player
            .as_ref()
            .map_or(0.0, |player| player.get_playback_position() as f64)
    }

    /// Audition length (seconds).
    #[func]
    pub fn audition_length(&self) -> f64 {
        self.music_player
            .as_ref()
            .and_then(|player| player.get_stream())
            .map_or(0.0, |stream| stream.get_length())
    }

    /// Priority spec summary for a sound (inspection helper).
    #[func]
    pub fn priority(&self, sound: GString) -> VarDictionary {
        let mut dict = VarDictionary::new();
        if let Some(id) = SoundId::by_name(&sound.to_string()) {
            let spec = self.table.spec(id);
            dict.set(&GString::from("priority"), &spec.priority.to_variant());
            dict.set(
                &GString::from("max_concurrent"),
                &i64::from(spec.max_concurrent).to_variant(),
            );
            dict.set(&GString::from("group"), &i64::from(spec.group).to_variant());
            dict.set(
                &GString::from("falloff_offset"),
                &spec.falloff_offset.to_variant(),
            );
        }
        dict
    }
}
