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
pub mod streams;
pub mod voices;

use godot::classes::{
    AudioListener2D, AudioServer, AudioStreamPlayer, Camera2D, INode, Node as GdNode,
};
use godot::obj::{Base, Singleton};
use godot::prelude::*;

use mind_core::audio::math::{Listener, MIN_PLAY_VOLUME, calc_falloff, calc_pan};
use mind_core::audio::{
    AmbientPoller, AudioEvent, AudioRng, LoopMixer, LoopOutput, MusicContext, MusicOutput,
    MusicPlayer, MusicRef, SeededAudioRng, SharedAudioLog, SoundId, SoundPriorityTable,
    TickedAudioEvent, bus_for_sound, settings_keys,
};

use buses::{BusLayout, LOWPASS_DRY_HZ, LOWPASS_WET_HZ};
use streams::StreamCache;
use voices::{LoopVoicePool, VoicePool};

/// The audio node.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct MindAudio {
    base: Base<GdNode>,
    ready: bool,
    bus: BusLayout,
    streams: StreamCache,
    pool: Option<VoicePool>,
    /// Persistent loop voices (`LoopMixer` output).
    loop_pool: Option<LoopVoicePool>,
    /// 20 Hz ambient poller (T5 fills its snapshot).
    poller: AmbientPoller,
    /// `debug_hold_loop` test seam: `(sound, x, y, volume, frames_left)`.
    debug_loop_hold: Option<(SoundId, f32, f32, f32, i64)>,
    music_player: Option<Gd<AudioStreamPlayer>>,
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
    /// `_process` dispatch counter (T1 liveness probe / PAR-0072 gate).
    frames: u64,
    /// Plan-18 sim→client sink, fetched from `/root/Spine/SimHost`.
    audio_log: Option<SharedAudioLog>,
    /// Events applied through the sink drain (inspector stat).
    events_applied: u64,
    /// `ConstructBlock.shouldPlay` wall clock (`ConstructBlock.java:58-69`).
    construct_last_played_ms: f64,
    /// `calcPitch` run window (`ConstructBlock.java:71-120`).
    construct_last_time_ms: f64,
    /// `calcPitch` sequence counter.
    construct_pitch_seq: i32,
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
            ready: false,
            bus: BusLayout::default(),
            streams: StreamCache::default(),
            pool: None,
            loop_pool: None,
            poller: AmbientPoller::new(50.0),
            debug_loop_hold: None,
            music_player: None,
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
            frames: 0,
            audio_log: None,
            events_applied: 0,
            construct_last_played_ms: 0.0,
            construct_last_time_ms: 0.0,
            construct_pitch_seq: 0,
            log: Vec::new(),
            missing: Vec::new(),
        }
    }

    fn ready(&mut self) {
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
        pool.stop_all();
        self.pool = Some(pool);
        self.loop_pool = Some(loop_pool);
        self.music_player = Some(music_player);

        // Audio listener on the spine camera (plan 18 §3.10).
        if let Some(camera) = self.find_camera() {
            let mut listener = AudioListener2D::new_alloc();
            listener.make_current();
            let mut camera = camera;
            camera.add_child(&listener);
        }

        self.ready = true;
        log::info!(
            "[audio] ready (buses {:?}, {} sounds, dir {})",
            self.bus,
            self.streams.sound_count(),
            assets_dir
        );
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
        if let Some(host) = self.sim_host() {
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
            self.audio_log = self.sim_host().map(|host| host.bind().audio_log());
        }
        let events = self
            .audio_log
            .as_ref()
            .map(SharedAudioLog::take_events)
            .unwrap_or_default();
        self.apply_events(&events, now_ms, delta_frames);

        let context = self.music_context(now_ms, delta_frames);

        // T4: per-frame loop mixing (`SoundControl.updateLoops`,
        // `SoundControl.java:410-469`). Leaving the game stops every loop
        // voice instead of just clearing the data.
        let listener = self.listener();
        let in_game = !context.is_menu;
        let avol = self.settings.ambientvol as f32 / 100.0;
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
    fn sim_host(&self) -> Option<Gd<crate::sim_host::MindSimHost>> {
        let base = self.base();
        let node = base.get_node_or_null("../SimHost")?;
        node.try_cast::<crate::sim_host::MindSimHost>().ok()
    }

    fn find_camera(&self) -> Option<Gd<Camera2D>> {
        let base = self.base();
        let node = base.get_node_or_null("../World/Camera2D")?;
        node.try_cast::<Camera2D>().ok()
    }

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
        let (x, y, zoom) = self
            .find_camera()
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
    /// menu); `dialog` is `state.isGame() && Core.scene.hasDialog()`
    /// (`SoundControl.java:120,130-132`).
    fn music_context(&self, now_ms: f64, delta_frames: f32) -> MusicContext {
        let (state, paused) = match self.sim_host() {
            Some(host) => {
                let host = host.bind();
                (host.get_state().to_string(), host.is_paused())
            }
            None => (String::from("menu"), false),
        };
        let menu_background = self
            .base()
            .get_node_or_null("/root/Spine/Ui/MenuBackground")
            .and_then(|node| node.get("visible").try_to::<bool>().ok())
            .unwrap_or(false);
        let is_menu = state == "menu" || menu_background;
        MusicContext {
            is_menu,
            is_game: !is_menu,
            paused,
            dialog: !is_menu && self.ui_has_dialog(),
            musicvol_setting: self.settings.musicvol,
            always_music_setting: self.settings.alwaysmusic,
            now_ms,
            delta_frames,
            ..MusicContext::default()
        }
    }

    /// `Core.scene.hasDialog()` — non-empty `MindUi` dialog stack
    /// (`gdext/input/mod.rs::dialog_open` pattern).
    fn ui_has_dialog(&self) -> bool {
        let Some(mut ui) = self.base().try_get_node_as::<GdNode>("/root/MindUi") else {
            return false;
        };
        if !ui.has_method("dialog_stack") {
            return false;
        }
        ui.call("dialog_stack", &[])
            .try_to::<PackedStringArray>()
            .map(|stack| !stack.is_empty())
            .unwrap_or(false)
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

    /// Applies the drained sim events (plan 18 §3.3): one-shots to the voice
    /// pool, loop aggregation into the `LoopMixer`, and music control through
    /// the `MusicPlayer`. Persistent loop voices and the in-engine §7c sweep
    /// are the documented deferred half.
    fn apply_events(&mut self, events: &[TickedAudioEvent], now_ms: f64, delta_frames: f32) {
        if events.is_empty() {
            return;
        }
        let listener = self.listener();
        for ticked in events {
            self.events_applied = self.events_applied.wrapping_add(1);
            match &ticked.event {
                AudioEvent::At {
                    sound,
                    x,
                    y,
                    pitch,
                    volume,
                    ..
                } => {
                    let scaled = calc_falloff(*x, *y, listener, 0.0) * self.sfx_scale() * *volume;
                    if scaled < MIN_PLAY_VOLUME {
                        continue;
                    }
                    self.play_sound(sound.name(), scaled, *pitch, calc_pan(*x, listener));
                }
                AudioEvent::Play {
                    sound,
                    volume,
                    pitch,
                    pan,
                    ..
                } => {
                    // `Sound.play` call sites pass `volume * sfxVolume`
                    // (`CoreBlock.java:712`, `LExecutor.java:2256`); the sim
                    // stores the raw volume (T3).
                    self.play_sound(sound.name(), *volume * self.sfx_scale(), *pitch, *pan);
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
                    sound,
                    x,
                    y,
                    volume_scl,
                    ..
                } => {
                    // SoundLoop instances accumulate into the same mix; their
                    // per-key fade voices are the deferred in-engine half.
                    self.mixer
                        .accumulate(*sound, *x, *y, *volume_scl, 1.0, listener);
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
                    let scaled = calc_falloff(*x, *y, listener, 0.0) * self.sfx_scale();
                    if scaled >= MIN_PLAY_VOLUME {
                        self.play_sound(sound.name(), scaled, pitch, calc_pan(*x, listener));
                    }
                }
            }
        }
    }

    /// Plays one one-shot through the UI or world voice pool.
    fn play_sound(&mut self, name: &str, volume: f32, pitch: f32, pan: f32) {
        let file = self.streams.file_of(name).map(str::to_owned);
        let Some(stream) = self.streams.sound(name, false) else {
            self.log_missing(name);
            return;
        };
        let ui = bus_for_sound(name, file.as_deref()) == mind_core::audio::BusKind::Ui;
        let listener = self.listener();
        let Some(pool) = self.pool.as_mut() else {
            return;
        };
        if ui {
            let _ = pool.play_ui(stream, volume, pitch);
        } else {
            let _ = pool.play_world(
                stream,
                volume,
                pitch,
                pan,
                (listener.x, listener.y),
                listener.width,
            );
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

    fn on_game_state_change(&mut self, playing: bool) {
        if !playing {
            self.player.stop();
        }
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
        dict.set(&key("voices"), &(self.voice_count().to_variant()));
        dict.set(&key("loop_sounds"), &(self.mixer.len() as i64).to_variant());
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
