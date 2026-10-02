// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SoundControl` music state machine (plan 18 §3.4).
//!
//! Pure state machine (playlists, dark/boss heuristics, fades, menu/planet/editor
//! branches, `WaveEvent` frame-clock delay). Godot playback is supplied through
//! [`MusicOutput`] by `mind-gdext`; tests use a recording output.

use super::ids::MusicRef;
use super::math::{
    AudioRng, FIN_TIME, FOUT_TIME, MUSIC_CHANCE, MUSIC_INTERVAL, MUSIC_WAVE_CHANCE, clamp01,
    random_index_excluding,
};

/// `Rules` music fields (`game/Rules.java`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MusicRules {
    /// `Rules.ambientMusic` override.
    pub ambient: Option<Vec<MusicRef>>,
    /// `Rules.darkMusic` override.
    pub dark: Option<Vec<MusicRef>>,
    /// `Rules.disableMusic`.
    pub disable: bool,
    /// `Rules.alwaysPlayMusic`.
    pub always: bool,
    /// `Rules.musicVolume`, clamped 0..1.
    pub volume: f32,
}

impl MusicRules {
    /// Default rules (`musicVolume = 1`).
    pub fn new() -> Self {
        MusicRules {
            volume: 1.0,
            ..MusicRules::default()
        }
    }
}

/// `Planet` music fields (`type/Planet.java`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlanetMusic {
    /// `Planet.launchMusic`.
    pub launch: Option<MusicRef>,
    /// `Planet.ambientMusic`.
    pub ambient: Option<Vec<MusicRef>>,
    /// `Planet.darkMusic`.
    pub dark: Option<Vec<MusicRef>>,
    /// `Planet.alwaysPlayMusic`.
    pub always: bool,
}

/// Everything `MusicPlayer::update` needs from the client frame.
#[derive(Debug, Clone, PartialEq)]
pub struct MusicContext {
    /// `state.isMenu()`.
    pub is_menu: bool,
    /// `state.isGame()`.
    pub is_game: bool,
    /// `state.isPaused()` (pause mirror).
    pub paused: bool,
    /// `state.isGame() && Core.scene.hasDialog()` (lowpass).
    pub dialog: bool,
    /// `state.rules.editor`.
    pub editor_rules: bool,
    /// Planet UI shown.
    pub planet_ui_visible: bool,
    /// Editor UI shown.
    pub editor_ui_visible: bool,
    /// Planet music fields.
    pub planet_music: PlanetMusic,
    /// Rules music fields.
    pub rules_music: MusicRules,
    /// `Core.settings.getBool("alwaysmusic")`.
    pub always_music_setting: bool,
    /// `Core.settings.getInt("musicvol")`.
    pub musicvol_setting: i32,
    /// Player core HP fraction (`player.team().data().core().healthf()`), if a core exists.
    pub core_hp_fraction: Option<f32>,
    /// `state.wave`.
    pub wave: i32,
    /// `state.enemies`.
    pub enemies: i32,
    /// `state.boss() != null`.
    pub live_boss: bool,
    /// A spawn group at `wave - 2` has the boss status effect.
    pub boss_spawn_group: bool,
    /// Frame clock (ms).
    pub now_ms: f64,
    /// Frame delta in 60 Hz frames (`Time.delta * 60`).
    pub delta_frames: f32,
}

impl Default for MusicContext {
    fn default() -> Self {
        MusicContext {
            is_menu: false,
            is_game: true,
            paused: false,
            dialog: false,
            editor_rules: false,
            planet_ui_visible: false,
            editor_ui_visible: false,
            planet_music: PlanetMusic::default(),
            rules_music: MusicRules::new(),
            always_music_setting: false,
            musicvol_setting: 100,
            core_hp_fraction: None,
            wave: 0,
            enemies: 0,
            live_boss: false,
            boss_spawn_group: false,
            now_ms: 0.0,
            delta_frames: 1.0,
        }
    }
}

/// Playback sink for the music state machine (implemented by `mind-gdext`).
pub trait MusicOutput {
    /// Starts a track at `volume`, optionally looping.
    fn play(&mut self, track: &MusicRef, volume: f32, looping: bool);
    /// Updates the current track volume.
    fn set_volume(&mut self, volume: f32);
    /// Stops the current track.
    fn stop(&mut self);
    /// Whether a track is currently playing.
    fn is_playing(&self) -> bool;
    /// Starts a lowpass wet crossfade (plan 18 §3.5).
    fn fade_filter(&mut self, wet: f32, duration_s: f32);
    /// Pause mirror for the Sound bus.
    fn set_paused(&mut self, paused: bool);
    /// Leaving/entering-game Sound-bus reset.
    fn on_game_state_change(&mut self, playing: bool);
}

/// `SoundControl` music player.
#[derive(Debug, Clone)]
pub struct MusicPlayer {
    /// Fade-in time (delta-frames).
    pub fin_time: f32,
    /// Fade-out time (delta-frames).
    pub fout_time: f32,
    /// Seconds between random ambient tracks (`musicInterval` in seconds).
    pub music_interval_s: f64,
    /// Chance per interval.
    pub music_chance: f32,
    /// Chance after a wave.
    pub music_wave_chance: f32,
    /// Default ambient playlist.
    pub ambient_music: Vec<MusicRef>,
    /// Default dark playlist.
    pub dark_music: Vec<MusicRef>,
    /// Default boss playlist.
    pub boss_music: Vec<MusicRef>,
    /// Menu track.
    pub menu_track: MusicRef,
    /// Editor track.
    pub editor_track: MusicRef,
    /// Current fade `0..1`.
    pub fade: f32,
    /// Current track silenced at next update.
    pub silenced: bool,
    /// `keepSilent()` pending.
    pub keep_silent: bool,
    /// Current track.
    pub current: Option<MusicRef>,
    /// Last randomly-played track (excluded from the next draw).
    pub last_random_played: Option<MusicRef>,
    /// Last scheduled play time (ms).
    pub last_played_ms: f64,
    /// Previous game state for the Sound-bus reset.
    pub was_playing: bool,
    /// Pending `WaveEvent` music time (ms).
    pub wave_music_due_ms: Option<f64>,
    lowpass_accum: f32,
}

impl Default for MusicPlayer {
    fn default() -> Self {
        let mut player = MusicPlayer {
            fin_time: FIN_TIME,
            fout_time: FOUT_TIME,
            music_interval_s: MUSIC_INTERVAL / 60.0,
            music_chance: MUSIC_CHANCE,
            music_wave_chance: MUSIC_WAVE_CHANCE,
            ambient_music: Vec::new(),
            dark_music: Vec::new(),
            boss_music: Vec::new(),
            menu_track: MusicRef::new("menu"),
            editor_track: MusicRef::new("editor"),
            fade: 0.0,
            silenced: false,
            keep_silent: false,
            current: None,
            last_random_played: None,
            last_played_ms: 0.0,
            was_playing: false,
            wave_music_due_ms: None,
            lowpass_accum: 0.0,
        };
        player.reload();
        player
    }
}

impl MusicPlayer {
    /// Creates a player with the default playlists.
    pub fn new() -> Self {
        MusicPlayer::default()
    }

    /// Port of `SoundControl.reload`: clears the current track and restores the
    /// default playlists. `MusicRegisterEvent` is fired by the caller.
    pub fn reload(&mut self) {
        self.current = None;
        self.fade = 0.0;
        self.ambient_music = ["game1", "game3", "game6", "game8", "game9", "fine"]
            .into_iter()
            .map(MusicRef::new)
            .collect();
        self.dark_music = ["game2", "game5", "game7", "game4"]
            .into_iter()
            .map(MusicRef::new)
            .collect();
        self.boss_music = ["boss1", "boss2", "game2", "game5"]
            .into_iter()
            .map(MusicRef::new)
            .collect();
    }

    /// `SoundControl.stop`.
    pub fn stop(&mut self, out: &mut impl MusicOutput) {
        self.silenced = true;
        if self.current.is_some() {
            out.stop();
            self.current = None;
            self.fade = 0.0;
        }
    }

    /// `SoundControl.keepSilent`.
    pub fn keep_silent(&mut self) {
        self.keep_silent = true;
    }

    /// `SoundControl.isPlaying`.
    pub fn is_playing(&self, out: &impl MusicOutput) -> bool {
        self.current.is_some() && out.is_playing()
    }

    /// `SoundControl.shouldPlay` — `musicvol > 0`.
    pub fn should_play(&self, ctx: &MusicContext) -> bool {
        ctx.musicvol_setting > 0
    }

    /// `SoundControl.volumeMultiplier`.
    pub fn volume_multiplier(&self, ctx: &MusicContext) -> f32 {
        ctx.musicvol_setting as f32 / 100.0 * clamp01(ctx.rules_music.volume)
    }

    /// `SoundControl.alwaysPlayMusic`.
    pub fn always_play_music(&self, ctx: &MusicContext) -> bool {
        ctx.rules_music.always || ctx.always_music_setting || ctx.planet_music.always
    }

    /// `SoundControl.getBossMusic` (including the boss→dark fallback quirk).
    pub fn boss_playlist(&self, ctx: &MusicContext) -> Vec<MusicRef> {
        if let Some(override_) = &ctx.rules_music.dark {
            return override_.clone();
        }
        if let Some(planet) = &ctx.planet_music.dark {
            return planet.clone();
        }
        self.boss_music.clone()
    }

    /// `SoundControl.getAmbientMusic`.
    pub fn ambient_playlist(&self, ctx: &MusicContext) -> Vec<MusicRef> {
        if let Some(override_) = &ctx.rules_music.ambient {
            return override_.clone();
        }
        if let Some(planet) = &ctx.planet_music.ambient {
            return planet.clone();
        }
        self.ambient_music.clone()
    }

    /// `SoundControl.getDarkMusic`.
    pub fn dark_playlist(&self, ctx: &MusicContext) -> Vec<MusicRef> {
        if let Some(override_) = &ctx.rules_music.dark {
            return override_.clone();
        }
        if let Some(planet) = &ctx.planet_music.dark {
            return planet.clone();
        }
        self.dark_music.clone()
    }

    /// `SoundControl.isDark`.
    pub fn is_dark(&self, ctx: &MusicContext, rng: &mut impl AudioRng) -> bool {
        if let Some(hp) = ctx.core_hp_fraction
            && hp < 0.85
        {
            return true;
        }
        let p = (((ctx.wave as f32 - 17.0) / 19.0).log10() + 1.0) / 4.0;
        if rng.chance(p) {
            return true;
        }
        rng.chance(ctx.enemies as f32 / 70.0 + 0.1)
    }

    /// `SoundControl.playRandom`.
    pub fn play_random(
        &mut self,
        ctx: &MusicContext,
        rng: &mut impl AudioRng,
        out: &mut impl MusicOutput,
    ) {
        let playlist = if ctx.live_boss {
            self.boss_playlist(ctx)
        } else if self.is_dark(ctx, rng) {
            self.dark_playlist(ctx)
        } else {
            self.ambient_playlist(ctx)
        };
        if playlist.is_empty() {
            return;
        }
        let exclude = self
            .last_random_played
            .as_ref()
            .and_then(|last| playlist.iter().position(|track| track == last));
        let Some(index) = random_index_excluding(rng, playlist.len(), exclude) else {
            return;
        };
        let selected = playlist[index].clone();
        self.last_random_played = Some(selected.clone());
        self.play_once_ctx(Some(selected), ctx, out);
    }

    /// `SoundControl.playMusic(music, interrupt)`.
    pub fn play_music(
        &mut self,
        music: Option<MusicRef>,
        interrupt: bool,
        ctx: &MusicContext,
        out: &mut impl MusicOutput,
    ) {
        if interrupt && self.current.is_some() {
            out.stop();
            self.current = None;
        }
        self.play_once_ctx(music.clone(), ctx, out);
        if music.is_some() && self.current == music {
            self.silenced = true;
        }
    }

    /// `playOnce` with the volume multiplier resolved from `ctx`
    /// (`shouldPlay` gate included).
    pub fn play_once_ctx(
        &mut self,
        music: Option<MusicRef>,
        ctx: &MusicContext,
        out: &mut impl MusicOutput,
    ) {
        if self.current.is_some() || !self.should_play(ctx) {
            return;
        }
        let Some(track) = music else {
            return;
        };
        self.last_random_played = Some(track.clone());
        self.fade = 1.0;
        self.current = Some(track.clone());
        self.silenced = false;
        out.play(&track, self.volume_multiplier(ctx), false);
    }

    /// `SoundControl.play(music)` (fade in/out).
    pub fn play(
        &mut self,
        music: Option<MusicRef>,
        ctx: &MusicContext,
        out: &mut impl MusicOutput,
    ) {
        if !self.should_play(ctx) {
            if self.current.is_some() {
                out.set_volume(0.0);
            }
            self.fade = 0.0;
            return;
        }

        if self.current.is_some() {
            out.set_volume(self.fade * self.volume_multiplier(ctx));
        }

        if self.silenced {
            return;
        }

        if self.current.is_none() {
            if let Some(track) = music {
                self.current = Some(track.clone());
                self.fade = 0.0;
                self.silenced = false;
                out.play(&track, 0.0, true);
            }
        } else if self.current == music && music.is_some() {
            self.fade = clamp01(self.fade + ctx.delta_frames / self.fin_time);
        } else if self.current.is_some() {
            self.fade = clamp01(self.fade - ctx.delta_frames / self.fout_time);
            if self.fade <= 0.01 {
                out.stop();
                self.current = None;
                self.silenced = true;
                if let Some(track) = music {
                    self.current = Some(track.clone());
                    self.fade = 0.0;
                    self.silenced = false;
                    out.play(&track, 0.0, true);
                }
            }
        }
    }

    /// `SoundControl.silence` — `play(null)`.
    pub fn silence(&mut self, ctx: &MusicContext, out: &mut impl MusicOutput) {
        self.play(None, ctx, out);
    }

    /// Schedules the `WaveEvent` delayed music (plan 18 §3.4).
    pub fn on_wave_event(&mut self, now_ms: f64, rng: &mut impl AudioRng) {
        let delay = 8.0 + rng.next_f32() * 7.0;
        self.wave_music_due_ms = Some(now_ms + f64::from(delay) * 1000.0);
    }

    /// Services a pending `WaveEvent` music timer; called each frame.
    pub fn update_wave(
        &mut self,
        ctx: &MusicContext,
        rng: &mut impl AudioRng,
        out: &mut impl MusicOutput,
    ) {
        let Some(due) = self.wave_music_due_ms else {
            return;
        };
        if ctx.now_ms < due {
            return;
        }
        self.wave_music_due_ms = None;
        if ctx.rules_music.disable {
            return;
        }
        if ctx.boss_spawn_group {
            let playlist = self.boss_playlist(ctx);
            if !playlist.is_empty() {
                let exclude = self
                    .last_random_played
                    .as_ref()
                    .and_then(|last| playlist.iter().position(|track| track == last));
                if let Some(index) = random_index_excluding(rng, playlist.len(), exclude) {
                    let selected = playlist[index].clone();
                    self.last_random_played = Some(selected.clone());
                    self.play_once_ctx(Some(selected), ctx, out);
                }
            }
        } else if rng.chance(self.music_wave_chance) {
            self.play_random(ctx, rng, out);
        }
    }

    /// Full `SoundControl.update` state machine, in upstream order.
    pub fn update(
        &mut self,
        ctx: &MusicContext,
        rng: &mut impl AudioRng,
        out: &mut impl MusicOutput,
    ) {
        let playing = ctx.is_game;

        // 1. finished-track detection.
        if self.current.is_some() && !out.is_playing() {
            self.current = None;
            self.fade = 0.0;
        }

        // 2. lowpass poll every 30 delta-frames.
        self.lowpass_accum += ctx.delta_frames;
        if self.lowpass_accum >= 30.0 {
            self.lowpass_accum -= 30.0;
            let wet = if ctx.dialog { 1.0 } else { 0.0 };
            out.fade_filter(wet, 0.4);
        }

        // 3. leaving/entering-game Sound-bus reset.
        if playing != self.was_playing {
            self.was_playing = playing;
            out.on_game_state_change(playing);
        }

        // 4. pause mirror.
        out.set_paused(ctx.paused);

        // 5. selection.
        if self.keep_silent {
            self.keep_silent = false;
            self.stop(out);
        } else if ctx.is_menu {
            self.silenced = false;
            if ctx.planet_ui_visible {
                let track = ctx.planet_music.launch.clone();
                self.play(track, ctx, out);
            } else if ctx.editor_ui_visible {
                let track = self.editor_track.clone();
                self.play(Some(track), ctx, out);
            } else {
                let track = self.menu_track.clone();
                self.play(Some(track), ctx, out);
            }
        } else if ctx.editor_rules {
            self.silenced = false;
            let track = self.editor_track.clone();
            self.play(Some(track), ctx, out);
        } else {
            self.silence(ctx, out);

            if !ctx.rules_music.disable {
                if self.always_play_music(ctx) {
                    if self.current.is_none() {
                        self.play_random(ctx, rng, out);
                    }
                } else if ctx.now_ms - self.last_played_ms > 1000.0 * self.music_interval_s
                    && rng.chance(self.music_chance)
                {
                    self.last_played_ms = ctx.now_ms;
                    self.play_random(ctx, rng, out);
                }
            }
        }

        // Service a pending wave timer (upstream `Time.run` callback).
        self.update_wave(ctx, rng, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::math::SeededAudioRng;

    #[derive(Default)]
    struct RecordingMusicOutput {
        playing: Option<MusicRef>,
        volume: f32,
        looping: bool,
        filter: Vec<(f32, f32)>,
        paused: bool,
        resets: Vec<bool>,
        stopped: usize,
    }

    impl MusicOutput for RecordingMusicOutput {
        fn play(&mut self, track: &MusicRef, volume: f32, looping: bool) {
            self.playing = Some(track.clone());
            self.volume = volume;
            self.looping = looping;
        }
        fn set_volume(&mut self, volume: f32) {
            self.volume = volume;
        }
        fn stop(&mut self) {
            self.playing = None;
            self.stopped += 1;
        }
        fn is_playing(&self) -> bool {
            self.playing.is_some()
        }
        fn fade_filter(&mut self, wet: f32, duration_s: f32) {
            self.filter.push((wet, duration_s));
        }
        fn set_paused(&mut self, paused: bool) {
            self.paused = paused;
        }
        fn on_game_state_change(&mut self, playing: bool) {
            self.resets.push(playing);
        }
    }

    fn ctx() -> MusicContext {
        MusicContext::default()
    }

    #[test]
    fn volume_multiplier_and_should_play() {
        let player = MusicPlayer::new();
        let mut context = ctx();
        context.musicvol_setting = 100;
        context.rules_music.volume = 1.0;
        assert_eq!(player.volume_multiplier(&context), 1.0);
        context.musicvol_setting = 50;
        context.rules_music.volume = 0.5;
        assert!((player.volume_multiplier(&context) - 0.25).abs() < 1.0e-6);
        context.musicvol_setting = 0;
        assert!(!player.should_play(&context));
        context.musicvol_setting = 1;
        assert!(player.should_play(&context));
    }

    #[test]
    fn menu_selection() {
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        let mut context = ctx();
        context.is_menu = true;
        context.is_game = false;
        player.update(&context, &mut SeededAudioRng::new(1), &mut out);
        assert_eq!(out.playing, Some(MusicRef::new("menu")));
        assert!(out.looping);

        // Editor UI overrides menu.
        context.editor_ui_visible = true;
        out.playing = None;
        player.update(&context, &mut SeededAudioRng::new(1), &mut out);
        assert_eq!(out.playing, Some(MusicRef::new("editor")));

        // Planet UI (launch music) takes precedence.
        context.planet_ui_visible = true;
        context.editor_ui_visible = false;
        context.planet_music.launch = Some(MusicRef::new("launchTrack"));
        out.playing = None;
        player.update(&context, &mut SeededAudioRng::new(1), &mut out);
        assert_eq!(out.playing, Some(MusicRef::new("launchTrack")));
    }

    #[test]
    fn editor_selection() {
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        let mut context = ctx();
        context.editor_rules = true;
        player.update(&context, &mut SeededAudioRng::new(1), &mut out);
        assert_eq!(out.playing, Some(MusicRef::new("editor")));
    }

    #[test]
    fn play_once_never_interrupts() {
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        player.play_once_ctx(Some(MusicRef::new("game1")), &ctx(), &mut out);
        assert_eq!(out.playing, Some(MusicRef::new("game1")));
        // Second call is ignored while a track is current.
        player.play_once_ctx(Some(MusicRef::new("game2")), &ctx(), &mut out);
        assert_eq!(out.playing, Some(MusicRef::new("game1")));
        // Zero music volume: no play.
        out.playing = None;
        player.current = None;
        let mut muted = ctx();
        muted.musicvol_setting = 0;
        player.play_once_ctx(Some(MusicRef::new("game3")), &muted, &mut out);
        assert_eq!(out.playing, None);
    }

    #[test]
    fn play_music_interrupt() {
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        player.play_once_ctx(Some(MusicRef::new("game1")), &ctx(), &mut out);
        // Non-interrupt does nothing.
        player.play_music(Some(MusicRef::new("game2")), false, &ctx(), &mut out);
        assert_eq!(out.playing, Some(MusicRef::new("game1")));
        // Interrupt replaces.
        player.play_music(Some(MusicRef::new("game2")), true, &ctx(), &mut out);
        assert_eq!(out.playing, Some(MusicRef::new("game2")));
        assert!(out.stopped >= 1);
    }

    #[test]
    fn fade_crossfade_math() {
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        let mut context = ctx();
        // Start track.
        player.play(Some(MusicRef::new("game1")), &context, &mut out);
        assert_eq!(out.playing, Some(MusicRef::new("game1")));
        // Fade in over finTime frames.
        for _ in 0..120 {
            player.play(Some(MusicRef::new("game1")), &context, &mut out);
        }
        assert!((player.fade - 1.0).abs() < 1.0e-4);
        // Crossfade to another track: fade out, then start at 0.
        context.delta_frames = 60.0;
        player.fade = 1.0;
        for _ in 0..2 {
            player.play(Some(MusicRef::new("game2")), &context, &mut out);
        }
        assert!(player.fade <= 0.01);
        assert_eq!(out.playing, Some(MusicRef::new("game2")));
    }

    #[test]
    fn should_play_zero_volume() {
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        let mut context = ctx();
        context.musicvol_setting = 0;
        player.play(Some(MusicRef::new("game1")), &context, &mut out);
        assert_eq!(out.playing, None);
        assert_eq!(out.volume, 0.0);
    }

    #[test]
    fn dark_heuristic_golden() {
        let player = MusicPlayer::new();
        // Core below 0.85 -> dark regardless of RNG.
        let mut context = ctx();
        context.core_hp_fraction = Some(0.5);
        assert!(player.is_dark(&context, &mut SeededAudioRng::new(1)));

        // Wave <= 17 -> NaN probability -> false (with no core/enemies).
        context.core_hp_fraction = None;
        context.wave = 10;
        context.enemies = 0;
        assert!(!player.is_dark(&context, &mut SeededAudioRng::new(1)));

        // High enemy count -> dark for most seeds.
        context.wave = 18;
        context.enemies = 70;
        let mut hits = 0;
        for seed in 0..100 {
            if player.is_dark(&context, &mut SeededAudioRng::new(seed)) {
                hits += 1;
            }
        }
        // p(enemy branch) = 1.1 -> always true.
        assert_eq!(hits, 100);
    }

    #[test]
    fn playlist_overrides_and_boss_quirk() {
        let player = MusicPlayer::new();
        let mut context = ctx();
        // Defaults.
        assert_eq!(player.ambient_playlist(&context).len(), 6);
        assert_eq!(player.dark_playlist(&context).len(), 4);
        assert_eq!(player.boss_playlist(&context).len(), 4);

        // Planet overrides.
        context.planet_music.ambient = Some(vec![MusicRef::new("pAmbient")]);
        assert_eq!(
            player.ambient_playlist(&context),
            vec![MusicRef::new("pAmbient")]
        );

        // Rules override beats planet.
        context.rules_music.ambient = Some(vec![MusicRef::new("rAmbient")]);
        assert_eq!(
            player.ambient_playlist(&context),
            vec![MusicRef::new("rAmbient")]
        );

        // Boss selection quirk: `rules.darkMusic` -> `planet.darkMusic` -> boss.
        context.rules_music.dark = Some(vec![MusicRef::new("rDark")]);
        assert_eq!(player.boss_playlist(&context), vec![MusicRef::new("rDark")]);
        context.rules_music.dark = None;
        context.planet_music.dark = Some(vec![MusicRef::new("pDark")]);
        assert_eq!(player.boss_playlist(&context), vec![MusicRef::new("pDark")]);
        context.planet_music.dark = None;
        assert_eq!(player.boss_playlist(&context).len(), 4);
    }

    #[test]
    fn game_random_chance_and_interval() {
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        let mut context = ctx();
        context.musicvol_setting = 100;
        // First frame: lastPlayed defaults 0, now 0 -> not > interval, no play.
        player.update(&context, &mut SeededAudioRng::new(1), &mut out);
        assert_eq!(out.playing, None);

        // Past the interval, chance(0.8) fires for most seeds.
        context.now_ms = 1_000_000.0;
        player.update(&context, &mut SeededAudioRng::new(3), &mut out);
        assert!(out.playing.is_some());

        // alwaysPlayMusic ignores the interval.
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        context.rules_music.always = true;
        player.update(&context, &mut SeededAudioRng::new(5), &mut out);
        assert!(out.playing.is_some());
    }

    #[test]
    fn wave_event_delay_and_boss() {
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        let mut context = ctx();
        let mut rng = SeededAudioRng::new(11);
        player.on_wave_event(0.0, &mut rng);
        let due = player.wave_music_due_ms.unwrap();
        assert!((8_000.0..=15_000.0).contains(&due));

        // Not due yet.
        context.now_ms = due - 1.0;
        player.update_wave(&context, &mut rng, &mut out);
        assert_eq!(out.playing, None);

        // Due with boss spawn group -> boss playlist track.
        context.now_ms = due + 1.0;
        context.boss_spawn_group = true;
        player.update_wave(&context, &mut rng, &mut out);
        assert!(out.playing.is_some());
        assert!(player.boss_music.contains(out.playing.as_ref().unwrap()));

        // Disabled music suppresses the wave timer.
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        let mut rng = SeededAudioRng::new(11);
        player.on_wave_event(0.0, &mut rng);
        let due = player.wave_music_due_ms.unwrap();
        context.now_ms = due + 1.0;
        context.rules_music.disable = true;
        player.update_wave(&context, &mut rng, &mut out);
        assert_eq!(out.playing, None);
    }

    #[test]
    fn stop_and_keep_silent() {
        let mut player = MusicPlayer::new();
        let mut out = RecordingMusicOutput::default();
        player.play_once_ctx(Some(MusicRef::new("game1")), &ctx(), &mut out);
        assert!(out.is_playing());
        player.keep_silent();
        player.update(&ctx(), &mut SeededAudioRng::new(1), &mut out);
        assert!(!out.is_playing());
        assert_eq!(out.playing, None);
    }
}
