// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// One-shot voice pool; Arc `Sound.at/play` + `Audio.defaultSoundMaxConcurrent`.

//! Fixed one-shot voice pool (plan 18 §3.6). All nodes are created once at
//! `MindAudio` ready (no runtime node creation, §7c assertion).

use godot::classes::{AudioStream, AudioStreamPlayer, AudioStreamPlayer2D, Node, Node2D};
use godot::prelude::*;
use indexmap::IndexMap;

use mind_core::audio::math::{Listener, SOUND_LOOP_FADE};
use mind_core::audio::{ActiveVoice, SoundId, SoundLoopAction, SoundLoopState};

/// World one-shot voices (`MAX_VOICES`).
pub const MAX_VOICES: usize = 128;
/// UI one-shot voices (`MAX_UI_VOICES`).
pub const MAX_UI_VOICES: usize = 16;
/// Looping world voices (aggregated loops + `SoundLoop` instances).
pub const MAX_LOOP_VOICES: usize = 32;

/// Per-slot policy metadata for the world one-shot pool (`Sound` state).
///
/// `sound: None` marks a slot played outside the policy (`play_world`), which
/// `active_voices` does not report to the admission table.
#[derive(Debug, Clone, Copy, Default)]
struct SlotMeta {
    sound: Option<SoundId>,
    started_ms: f64,
    priority: f32,
    group: i32,
    min_interrupt: f32,
    volume: f32,
}

/// Pooled non-positional + positional one-shot voices.
pub struct VoicePool {
    world: Vec<Gd<AudioStreamPlayer2D>>,
    ui: Vec<Gd<AudioStreamPlayer>>,
    world_cursor: usize,
    ui_cursor: usize,
    /// Policy metadata per world slot.
    meta: Vec<SlotMeta>,
    /// Voice id generation per world slot (ids are `(generation << 8) | slot`).
    generation: Vec<u32>,
}

impl VoicePool {
    /// Allocates the pools under a code-instantiated container.
    pub fn new(base: &mut Gd<Node>, sound_bus: &str, ui_bus: &str) -> Self {
        // code-instantiated: pooled positional voices are high-churn, fixed-count
        // and carry per-Play state; a .tscn cannot parameterize 128 players.
        let mut world_parent = Node2D::new_alloc();
        world_parent.set_name("WorldVoices");
        base.add_child(&world_parent);

        let mut world = Vec::with_capacity(MAX_VOICES);
        for _ in 0..MAX_VOICES {
            let mut player = AudioStreamPlayer2D::new_alloc();
            player.set_bus(&StringName::from(sound_bus));
            player.set_attenuation(0.0);
            player.set_max_distance(1.0e9);
            player.set_panning_strength(1.0);
            world_parent.add_child(&player);
            world.push(player);
        }

        let mut ui = Vec::with_capacity(MAX_UI_VOICES);
        for _ in 0..MAX_UI_VOICES {
            let mut player = AudioStreamPlayer::new_alloc();
            player.set_bus(&StringName::from(ui_bus));
            base.add_child(&player);
            ui.push(player);
        }

        VoicePool {
            world,
            ui,
            world_cursor: 0,
            ui_cursor: 0,
            meta: vec![SlotMeta::default(); MAX_VOICES],
            generation: vec![1; MAX_VOICES],
        }
    }

    /// Splits a voice id into `(slot, generation)` (`MAX_VOICES < 256`).
    fn split_voice(voice: u64) -> (usize, u32) {
        ((voice & 0xFF) as usize, (voice >> 8) as u32)
    }

    fn position_for(pan: f32, listener: (f32, f32), width: f32) -> Vector2 {
        Vector2::new(listener.0 + pan * width / 2.0, listener.1)
    }

    /// Number of currently playing world + UI voices.
    pub fn active(&self) -> usize {
        self.world
            .iter()
            .filter(|player| player.is_playing())
            .count()
            + self.ui.iter().filter(|player| player.is_playing()).count()
    }

    /// Policy-visible world voices (`SoundPriority` admission input).
    pub fn active_voices(&self, out: &mut Vec<ActiveVoice>) {
        out.clear();
        for (slot, player) in self.world.iter().enumerate() {
            if !player.is_playing() {
                continue;
            }
            let Some(sound) = self.meta[slot].sound else {
                continue;
            };
            out.push(ActiveVoice {
                voice: (u64::from(self.generation[slot]) << 8) | slot as u64,
                sound,
                group: self.meta[slot].group,
                priority: self.meta[slot].priority,
                started_ms: self.meta[slot].started_ms,
                min_interrupt: self.meta[slot].min_interrupt,
                volume: self.meta[slot].volume,
            });
        }
    }

    /// Stops a UI voice by the index `play_ui` returned (`PlaySoundStatement`'s
    /// preview stop; the UI pool has no reusable voice ids).
    pub fn stop_ui(&self, voice: i64) {
        let index = voice.max(0) as usize % self.ui.len().max(1);
        if let Some(player) = self.ui.get(index) {
            player.clone().stop();
        }
    }

    /// Plays a non-positional UI voice; returns the voice index (always `>= 0`).
    pub fn play_ui(&mut self, stream: Gd<AudioStream>, volume: f32, pitch: f32) -> i64 {
        let index = self.ui_cursor % self.ui.len();
        self.ui_cursor = self.ui_cursor.wrapping_add(1);
        let player = &mut self.ui[index];
        player.set_stream(&stream);
        player.set_volume_linear(volume.clamp(0.0, 1.0));
        player.set_pitch_scale(pitch.clamp(0.0001, 10.0));
        player.play();
        index as i64
    }

    /// Plays a positional world voice using the deviation-5 pan mapping.
    ///
    /// Keeps the id/meta bookkeeping (`sound: None`, not visible to the
    /// admission policy) so a stale id can never address a recycled slot.
    #[allow(clippy::too_many_arguments)]
    pub fn play_world(
        &mut self,
        stream: Gd<AudioStream>,
        volume: f32,
        pitch: f32,
        pan: f32,
        listener: (f32, f32),
        viewport_width: f32,
    ) -> i64 {
        let index = self.world_cursor % self.world.len();
        self.world_cursor = self.world_cursor.wrapping_add(1);
        self.generation[index] = self.generation[index].wrapping_add(1);
        self.meta[index] = SlotMeta::default();
        let player = &mut self.world[index];
        player.set_stream(&stream);
        player.set_volume_linear(volume.clamp(0.0, 1.0));
        player.set_pitch_scale(pitch.clamp(0.0001, 10.0));
        // Pan via node offset from the listener (attenuation 0); plan 18 §2.4 dev 5.
        player.set_position(Self::position_for(pan, listener, viewport_width));
        player.play();
        ((u64::from(self.generation[index]) << 8) | index as u64) as i64
    }

    /// Plays a policy-admitted world voice.
    ///
    /// `replace` reuses that voice's slot when its generation still matches
    /// (the evicted player is stopped first); otherwise the first idle slot is
    /// used. `meta` carries `(sound, started_ms, priority, group,
    /// min_interrupt)`; the id generation is bumped before the id is returned.
    /// `None` means the pool is full (the caller treats it as a denied play).
    #[allow(clippy::too_many_arguments)]
    pub fn play_world_meta(
        &mut self,
        replace: Option<u64>,
        stream: Gd<AudioStream>,
        volume: f32,
        pitch: f32,
        pan: f32,
        listener: (f32, f32),
        width: f32,
        meta: (SoundId, f64, f32, i32, f32),
    ) -> Option<u64> {
        let slot = match replace {
            Some(voice) => {
                let (slot, generation) = Self::split_voice(voice);
                if slot < self.world.len() && self.generation[slot] == generation {
                    self.world[slot].stop();
                    slot
                } else {
                    self.world.iter().position(|player| !player.is_playing())?
                }
            }
            None => self.world.iter().position(|player| !player.is_playing())?,
        };
        self.generation[slot] = self.generation[slot].wrapping_add(1);
        let player = &mut self.world[slot];
        player.set_stream(&stream);
        player.set_volume_linear(volume.clamp(0.0, 1.0));
        player.set_pitch_scale(pitch.clamp(0.0001, 10.0));
        player.set_position(Self::position_for(pan, listener, width));
        player.play();
        let (sound, started_ms, priority, group, min_interrupt) = meta;
        self.meta[slot] = SlotMeta {
            sound: Some(sound),
            started_ms,
            priority,
            group,
            min_interrupt,
            volume,
        };
        Some((u64::from(self.generation[slot]) << 8) | slot as u64)
    }

    /// Sets a running world voice's volume/pan (stale ids are ignored).
    pub fn set_voice_volume(
        &mut self,
        voice: u64,
        volume: f32,
        pan: f32,
        listener: (f32, f32),
        width: f32,
    ) {
        let (slot, generation) = Self::split_voice(voice);
        if slot >= self.world.len() || self.generation[slot] != generation {
            return;
        }
        let player = &mut self.world[slot];
        player.set_volume_linear(volume.clamp(0.0, 1.0));
        player.set_position(Self::position_for(pan, listener, width));
        self.meta[slot].volume = volume;
    }

    /// Pauses/resumes every world+UI voice (plan 18 §3.5 pause mirror).
    pub fn set_paused(&self, paused: bool) {
        for player in &self.world {
            player.clone().set_stream_paused(paused);
        }
        for player in &self.ui {
            player.clone().set_stream_paused(paused);
        }
    }

    /// Stops every voice.
    pub fn stop_all(&self) {
        for player in &self.world {
            player.clone().stop();
        }
        for player in &self.ui {
            player.clone().stop();
        }
    }
}

/// Pooled looping positional voices (`LoopMixer`/`SoundLoop` outputs).
///
/// Voice ids encode `(generation << 8) | slot` so a stale id can never address
/// a recycled slot; `0` is the "no voice" sentinel.
pub struct LoopVoicePool {
    players: Vec<Gd<AudioStreamPlayer2D>>,
    generation: Vec<u32>,
}

impl LoopVoicePool {
    /// Allocates the pool under a code-instantiated container.
    pub fn new(base: &mut Gd<Node>, sound_bus: &str) -> Self {
        // code-instantiated: fixed-count pooled loop voices (same rationale as
        // `VoicePool`: high-churn, per-Play state, not .tscn-expressible).
        let mut parent = Node2D::new_alloc();
        parent.set_name("LoopVoices");
        base.add_child(&parent);

        let mut players = Vec::with_capacity(MAX_LOOP_VOICES);
        for _ in 0..MAX_LOOP_VOICES {
            let mut player = AudioStreamPlayer2D::new_alloc();
            player.set_bus(&StringName::from(sound_bus));
            player.set_attenuation(0.0);
            player.set_max_distance(1.0e9);
            player.set_panning_strength(1.0);
            parent.add_child(&player);
            players.push(player);
        }
        LoopVoicePool {
            players,
            generation: vec![1; MAX_LOOP_VOICES],
        }
    }

    fn split_voice(voice: u64) -> (usize, u32) {
        ((voice & 0xFF) as usize, (voice >> 8) as u32)
    }

    fn position_for(pan: f32, listener: (f32, f32), width: f32) -> Vector2 {
        Vector2::new(listener.0 + pan * width / 2.0, listener.1)
    }

    /// Starts a loop voice in the first idle slot; `0` when the pool is full.
    pub fn start(
        &mut self,
        stream: Gd<AudioStream>,
        volume: f32,
        pitch: f32,
        pan: f32,
        listener: (f32, f32),
        width: f32,
    ) -> u64 {
        let Some(slot) = self.players.iter().position(|player| !player.is_playing()) else {
            return 0;
        };
        let generation = self.generation[slot];
        let player = &mut self.players[slot];
        player.set_stream(&stream);
        player.set_volume_linear(volume.clamp(0.0, 1.0));
        player.set_pitch_scale(pitch.clamp(0.0001, 10.0));
        player.set_position(Self::position_for(pan, listener, width));
        player.play();
        (u64::from(generation) << 8) | slot as u64
    }

    /// Updates a running loop voice (stale generations are ignored).
    pub fn update(
        &mut self,
        voice: u64,
        volume: f32,
        pitch: f32,
        pan: f32,
        listener: (f32, f32),
        width: f32,
    ) {
        let (slot, generation) = Self::split_voice(voice);
        if slot >= self.players.len() || self.generation[slot] != generation {
            return;
        }
        let player = &mut self.players[slot];
        player.set_volume_linear(volume.clamp(0.0, 1.0));
        player.set_pitch_scale(pitch.clamp(0.0001, 10.0));
        player.set_position(Self::position_for(pan, listener, width));
    }

    /// Stops a loop voice and invalidates its id (bumps the generation).
    pub fn stop(&mut self, voice: u64) {
        let (slot, generation) = Self::split_voice(voice);
        if slot >= self.players.len() || self.generation[slot] != generation {
            return;
        }
        self.players[slot].stop();
        self.generation[slot] = self.generation[slot].wrapping_add(1);
    }

    /// Whether the id still names a playing voice.
    pub fn is_playing(&self, voice: u64) -> bool {
        let (slot, generation) = Self::split_voice(voice);
        self.generation.get(slot) == Some(&generation)
            && self
                .players
                .get(slot)
                .is_some_and(|player| player.is_playing())
    }

    /// Pauses/resumes every loop voice.
    pub fn set_paused(&self, paused: bool) {
        for player in &self.players {
            player.clone().set_stream_paused(paused);
        }
    }

    /// Stops every loop voice and invalidates every id.
    pub fn stop_all(&mut self) {
        for (slot, player) in self.players.iter_mut().enumerate() {
            player.stop();
            self.generation[slot] = self.generation[slot].wrapping_add(1);
        }
    }
}

/// One per-instance `SoundLoop.update(x, y, play[, volumeScl])` input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundLoopInput {
    /// Stable per-instance voice key (`Turret`/weapon-mount/block entity).
    pub key: u64,
    /// Looped sound.
    pub sound: SoundId,
    /// Base volume from the `SoundLoop` constructor (`loopSoundVolume` for
    /// turrets, `1f` for continuous weapon mounts).
    pub base_volume: f32,
    /// World x (px).
    pub x: f32,
    /// World y (px).
    pub y: f32,
    /// Whether the loop should be playing (fade in) this frame.
    pub play: bool,
    /// Call-site `volumeScl` (turret `activeSoundVolume()`, `1` for weapons).
    pub volume_scl: f32,
    /// Voice pitch. Java `SoundLoop.update` always starts at `1f`; the field
    /// exists for the host hook and is held constant while fading (Java's
    /// `Core.audio.set(id, pan, volume)` does not touch pitch).
    pub pitch: f32,
    /// Frames elapsed since the previous update (`Time.delta`, 60 fps units):
    /// `SoundLoop.update` scales `fadeSpeed` by it (`SoundLoop.java:38-40`).
    pub delta_frames: f32,
}

/// Voice operations required by [`SoundLoopTable`].
pub trait SoundLoopVoice {
    /// Starts a loop voice; `0` means the pool/stream refused (retry next frame).
    fn start(&mut self, sound: SoundId, volume: f32, pitch: f32, pan: f32) -> u64;
    /// Updates a running voice's volume/pitch/pan.
    fn update(&mut self, voice: u64, volume: f32, pitch: f32, pan: f32);
    /// Stops a voice.
    fn stop(&mut self, voice: u64);
}

#[derive(Debug, Clone)]
struct SoundLoopEntry {
    state: SoundLoopState,
    /// Fixed pitch for the voice (Java never re-pitches an existing `SoundLoop`).
    pitch: f32,
    /// Updated this frame (`SoundLoop.update` called for the key).
    seen: bool,
}

/// `Time.delta` for the fade step; a non-finite/non-positive frame delta
/// counts as one 60 Hz frame instead of freezing or reversing the fade.
fn fade_delta(delta_frames: f32) -> f32 {
    if delta_frames.is_finite() && delta_frames > 0.0 {
        delta_frames
    } else {
        1.0
    }
}

/// Per-key `SoundLoop` fade voices (`SoundLoop.java`).
///
/// Java keeps one `SoundLoop` per turret building and one per continuous weapon
/// mount, each with its own `id`, `volume` fade and `baseVolume`. Folding the
/// instances into the aggregated `LoopMixer` loses that per-key lifecycle (and
/// its pitch/volume independence), so this table owns one [`SoundLoopState`]
/// per key and drives an independent pooled voice through [`SoundLoopVoice`].
///
/// The table is driven by both client paths: the per-frame turret/weapon scan
/// and the drained `AudioEvent::LoopInstance` events. Call [`begin_frame`] once
/// per frame, [`update`] for every reported key, then [`sweep`]; a key that is
/// not reported in a frame is stopped (`SoundLoop.stop()` on teardown).
///
/// [`begin_frame`]: SoundLoopTable::begin_frame
/// [`update`]: SoundLoopTable::update
/// [`sweep`]: SoundLoopTable::sweep
pub struct SoundLoopTable {
    entries: IndexMap<u64, SoundLoopEntry>,
}

impl Default for SoundLoopTable {
    fn default() -> Self {
        SoundLoopTable::new()
    }
}

impl SoundLoopTable {
    /// Creates an empty per-key table.
    pub fn new() -> Self {
        SoundLoopTable {
            entries: IndexMap::with_capacity(8),
        }
    }

    /// Number of live per-key loops (including fully-faded-out ones awaiting
    /// their key to stop being reported).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no per-key loop is tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Live voice id for a key (`None` before the first `SoundLoop.update`).
    pub fn voice(&self, key: u64) -> Option<u64> {
        self.entries.get(&key).and_then(|entry| entry.state.voice)
    }

    /// Current fade volume for a key (`0..1`, `SoundLoop.volume`).
    pub fn fade_volume(&self, key: u64) -> Option<f32> {
        self.entries.get(&key).map(|entry| entry.state.volume)
    }

    /// Clears the frame's "reported" marks; call at the top of `process`.
    pub fn begin_frame(&mut self) {
        for entry in self.entries.values_mut() {
            entry.seen = false;
        }
    }

    /// `SoundLoop.update(x, y, play[, volumeScl])` for one key.
    ///
    /// Starts the voice on the first playing frame at `volume = 0` (Java's
    /// `sound.loop(...)` runs before the first `fadeSpeed` step), fades it in
    /// or out at `SoundLoop.fadeSpeed * Time.delta`, then updates volume/pan. A
    /// changed sound or `baseVolume` for the same key replaces the instance
    /// (the old voice is stopped first).
    pub fn update(
        &mut self,
        input: SoundLoopInput,
        listener: Listener,
        out: &mut impl SoundLoopVoice,
    ) {
        if input.base_volume <= 0.0 || input.sound.is_none_or_unset() {
            return;
        }
        let entry = self
            .entries
            .entry(input.key)
            .or_insert_with(|| SoundLoopEntry {
                state: SoundLoopState::new(input.sound, input.base_volume, input.key),
                pitch: input.pitch,
                seen: false,
            });
        if entry.state.sound != input.sound || entry.state.base_volume != input.base_volume {
            if let Some(voice) = entry.state.stop() {
                out.stop(voice);
            }
            entry.state = SoundLoopState::new(input.sound, input.base_volume, input.key);
        }
        entry.pitch = input.pitch;
        entry.seen = true;
        // `volume = clamp(volume + fadeSpeed * Time.delta)` (`SoundLoop.java:38-40`).
        entry.state.fade_speed = SOUND_LOOP_FADE * fade_delta(input.delta_frames);
        let action = entry
            .state
            .update(input.x, input.y, input.play, input.volume_scl, listener);
        match action {
            SoundLoopAction::Start { volume, pan, .. } => {
                let voice = out.start(input.sound, volume, entry.pitch, pan);
                if voice != 0 {
                    entry.state.voice = Some(voice);
                }
            }
            SoundLoopAction::Update { voice, volume, pan } => {
                out.update(voice, volume, entry.pitch, pan);
            }
            SoundLoopAction::Stop { voice } => out.stop(voice),
            SoundLoopAction::None => {}
        }
    }

    /// Stops every key that was not reported this frame (`SoundLoop.stop` on
    /// `remove()`/`onDestroyed`).
    pub fn sweep(&mut self, out: &mut impl SoundLoopVoice) {
        self.entries.retain(|_, entry| {
            if entry.seen {
                return true;
            }
            if let Some(voice) = entry.state.stop() {
                out.stop(voice);
            }
            false
        });
    }

    /// Stops and drops every per-key loop (`ResetEvent`/leaving the game).
    pub fn stop_all(&mut self, out: &mut impl SoundLoopVoice) {
        for entry in self.entries.values_mut() {
            if let Some(voice) = entry.state.stop() {
                out.stop(voice);
            }
        }
        self.entries.clear();
    }

    /// Drops every per-key loop without touching voices (the caller already
    /// stopped the whole pool).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingVoices {
        started: Vec<(u64, SoundId, f32, f32, f32)>,
        updated: Vec<(u64, f32, f32, f32)>,
        stopped: Vec<u64>,
        next: u64,
        refuse_next_start: bool,
    }

    impl SoundLoopVoice for RecordingVoices {
        fn start(&mut self, sound: SoundId, volume: f32, pitch: f32, pan: f32) -> u64 {
            if self.refuse_next_start {
                self.refuse_next_start = false;
                return 0;
            }
            self.next += 1;
            let voice = self.next;
            self.started.push((voice, sound, volume, pitch, pan));
            voice
        }

        fn update(&mut self, voice: u64, volume: f32, pitch: f32, pan: f32) {
            self.updated.push((voice, volume, pitch, pan));
        }

        fn stop(&mut self, voice: u64) {
            self.stopped.push(voice);
        }
    }

    fn listener() -> Listener {
        Listener::new(0.0, 0.0, 200.0)
    }

    fn input(key: u64, play: bool) -> SoundLoopInput {
        SoundLoopInput {
            key,
            sound: SoundId::LOOP_DRILL,
            base_volume: 0.8,
            x: 0.0,
            y: 0.0,
            play,
            volume_scl: 0.5,
            pitch: 1.0,
            delta_frames: 1.0,
        }
    }

    /// `SoundLoop.java:26-46`: the first playing frame creates a voice at the
    /// current (zero) fade volume, then each frame fades by `fadeSpeed` and
    /// `Core.audio.set`s the volume; `play = false` fades out and stops at
    /// `0.001`, routing `calcFalloff * volume * baseVolume * volumeScl`.
    #[test]
    fn per_key_fade_in_out_and_volume() {
        let mut table = SoundLoopTable::new();
        let mut out = RecordingVoices::default();
        let mut listener = listener();
        listener.x = 0.0;

        table.begin_frame();
        table.update(input(9, true), listener, &mut out);
        assert_eq!(out.started.len(), 1, "first playing frame starts a voice");
        assert_eq!(out.started[0].2, 0.0, "Java starts the loop at volume 0");
        assert_eq!(out.started[0].3, 1.0, "Java passes pitch 1f");
        assert_eq!(table.voice(9), Some(1));

        // 20 fade steps of `SOUND_LOOP_FADE` reach full volume.
        for _ in 0..19 {
            table.begin_frame();
            table.update(input(9, true), listener, &mut out);
        }
        // The 20th step lands exactly on 1.0 (float accumulation aside).
        table.begin_frame();
        table.update(input(9, true), listener, &mut out);
        let fade = table.fade_volume(9).unwrap_or(-1.0);
        assert!((fade - 1.0).abs() < 1.0e-5, "fade {fade}");
        // `calcVolume(0,0) = 1` at the listener: 1 * 0.8 * 0.5 = 0.4.
        let (_, volume, pitch, pan) = *out.updated.last().unwrap_or(&(0, -1.0, 0.0, 0.0));
        assert!((volume - 0.4).abs() < 1.0e-5, "volume {volume}");
        assert_eq!(pitch, 1.0);
        assert_eq!(pan, 0.0);

        // Fade out: 20 steps of 0.05 take the volume from 1.0 to 0, and the
        // state stops the voice when it reaches `SOUND_LOOP_STOP_VOLUME`.
        let mut stop_frame = 0;
        for frame in 0..25 {
            table.begin_frame();
            table.update(input(9, false), listener, &mut out);
            if !out.stopped.is_empty() {
                stop_frame = frame;
                break;
            }
        }
        assert!(stop_frame >= 1, "faded out before a frame");
        assert_eq!(table.voice(9), None);
        assert_eq!(out.stopped, vec![1]);
        // The key is still reported, so the entry survives the sweep with no
        // voice (so a later `play = true` fades it back in).
        table.sweep(&mut out);
        assert_eq!(table.len(), 1);
    }

    /// `SoundLoop.java:38-40`: the fade step is `fadeSpeed * Time.delta`, so a
    /// 30 fps frame (delta 2.0) moves twice as far as a 60 fps frame; a hostile
    /// delta falls back to one frame instead of freezing the fade.
    #[test]
    fn fade_step_scales_with_delta() {
        let mut table = SoundLoopTable::new();
        let mut out = RecordingVoices::default();
        let mut frame = input(4, true);
        frame.delta_frames = 2.0;

        table.begin_frame();
        table.update(frame, listener(), &mut out);
        assert_eq!(table.fade_volume(4), Some(0.0), "start frame stays at 0");

        table.begin_frame();
        table.update(frame, listener(), &mut out);
        assert!((table.fade_volume(4).unwrap_or(-1.0) - 0.1).abs() < 1.0e-6);

        frame.delta_frames = 0.5;
        table.begin_frame();
        table.update(frame, listener(), &mut out);
        assert!((table.fade_volume(4).unwrap_or(-1.0) - 0.125).abs() < 1.0e-6);

        frame.delta_frames = f32::NAN;
        table.begin_frame();
        table.update(frame, listener(), &mut out);
        assert!((table.fade_volume(4).unwrap_or(-1.0) - 0.175).abs() < 1.0e-6);

        frame.delta_frames = 0.0;
        table.begin_frame();
        table.update(frame, listener(), &mut out);
        assert!((table.fade_volume(4).unwrap_or(-1.0) - 0.225).abs() < 1.0e-6);
    }

    /// Keys own independent voices/volumes: updating one key must not restart,
    /// stop or otherwise touch another (`SoundLoop` is per entity/mount).
    #[test]
    fn keys_own_independent_voices() {
        let mut table = SoundLoopTable::new();
        let mut out = RecordingVoices::default();

        table.begin_frame();
        table.update(input(1, true), listener(), &mut out);
        table.update(input(2, true), listener(), &mut out);
        assert_eq!(out.started.len(), 2);
        assert_ne!(table.voice(1), table.voice(2));

        // Fade key 1 only; key 2 stays at fade volume 0 with its own voice.
        for _ in 0..20 {
            table.begin_frame();
            table.update(input(1, true), listener(), &mut out);
        }
        assert!((table.fade_volume(1).unwrap_or(-1.0) - 1.0).abs() < 1.0e-5);
        assert_eq!(table.fade_volume(2), Some(0.0));
        assert_eq!(table.voice(2), Some(2));
        // No update addressed voice 2 after its start.
        assert!(out.updated.iter().all(|(voice, ..)| *voice == 1));
    }

    /// Continuous weapon mounts use `new SoundLoop(sound, 1f)`: base volume 1
    /// with the start pitch held for every later `Core.audio.set`.
    #[test]
    fn weapon_mount_base_volume_and_pitch() {
        let mut table = SoundLoopTable::new();
        let mut out = RecordingVoices::default();
        let mut weapon = SoundLoopInput {
            key: 1 << 63,
            sound: SoundId::SHOOT_LASER,
            base_volume: 1.0,
            pitch: 1.25,
            volume_scl: 1.0,
            ..input(0, true)
        };

        table.begin_frame();
        table.update(weapon, listener(), &mut out);
        assert_eq!(out.started.len(), 1);
        assert_eq!(out.started[0].3, 1.25, "host pitch is used on start");
        assert_eq!(out.started[0].2, 0.0);

        for _ in 0..20 {
            table.begin_frame();
            table.update(weapon, listener(), &mut out);
        }
        let (voice, volume, pitch, _) = *out.updated.last().unwrap_or(&(0, -1.0, 0.0, 0.0));
        assert_eq!(voice, 1);
        assert!((volume - 1.0).abs() < 1.0e-5, "volume {volume}");
        assert_eq!(pitch, 1.25, "pitch is held through the fade");

        // `baseVolume <= 0` short-circuits (`SoundLoop.java:22`).
        weapon.key = 4;
        weapon.base_volume = 0.0;
        table.begin_frame();
        table.update(weapon, listener(), &mut out);
        assert_eq!(out.started.len(), 1);
        assert_eq!(table.len(), 1);
    }

    /// A destroyed/removed instance stops being reported: `sweep` must call
    /// `SoundLoop.stop()` and drop the key (no leaked voice).
    #[test]
    fn sweep_stops_unreported_keys() {
        let mut table = SoundLoopTable::new();
        let mut out = RecordingVoices::default();

        table.begin_frame();
        table.update(input(1, true), listener(), &mut out);
        table.update(input(2, true), listener(), &mut out);
        table.sweep(&mut out);
        assert_eq!(table.len(), 2);

        // Only key 2 is reported this frame.
        table.begin_frame();
        table.update(input(2, true), listener(), &mut out);
        table.sweep(&mut out);
        assert_eq!(table.len(), 1);
        assert_eq!(out.stopped, vec![1]);
        assert_eq!(table.voice(2), Some(2));

        // No key reported: every remaining voice stops.
        table.begin_frame();
        table.sweep(&mut out);
        assert!(table.is_empty());
        assert_eq!(out.stopped, vec![1, 2]);
    }

    /// A full loop pool (`start` returns 0) must leave the key voiceless and
    /// retry on the next frame instead of pinning a `Some(0)` id.
    #[test]
    fn pool_full_start_retries_next_frame() {
        let mut table = SoundLoopTable::new();
        let mut out = RecordingVoices {
            refuse_next_start: true,
            ..RecordingVoices::default()
        };

        table.begin_frame();
        table.update(input(3, true), listener(), &mut out);
        assert!(out.started.is_empty());
        assert_eq!(table.voice(3), None);

        table.begin_frame();
        table.update(input(3, true), listener(), &mut out);
        assert_eq!(out.started.len(), 1);
        assert_eq!(table.voice(3), Some(1));
    }

    /// A key whose sound/base volume changes (turret config restart, reused
    /// entity slot) gets a fresh `SoundLoop`: the old voice stops first.
    #[test]
    fn changed_sound_replaces_voice() {
        let mut table = SoundLoopTable::new();
        let mut out = RecordingVoices::default();

        table.begin_frame();
        table.update(input(5, true), listener(), &mut out);
        let first = table.voice(5);

        table.begin_frame();
        let mut changed = input(5, true);
        changed.sound = SoundId::LOOP_MINE_BEAM;
        table.update(changed, listener(), &mut out);
        assert_eq!(out.stopped, vec![first.unwrap_or(0)]);
        assert_eq!(out.started.len(), 2);
        assert_eq!(out.started[1].1, SoundId::LOOP_MINE_BEAM);
        assert_ne!(table.voice(5), first);
    }

    /// `stop_all` (leaving the game / `ResetEvent`) stops every voice; `clear`
    /// is the pool-already-stopped variant.
    #[test]
    fn stop_all_and_clear() {
        let mut table = SoundLoopTable::new();
        let mut out = RecordingVoices::default();
        table.begin_frame();
        table.update(input(1, true), listener(), &mut out);
        table.update(input(2, true), listener(), &mut out);

        table.stop_all(&mut out);
        assert_eq!(out.stopped, vec![1, 2]);
        assert!(table.is_empty());

        table.begin_frame();
        table.update(input(7, true), listener(), &mut out);
        table.clear();
        assert!(table.is_empty());
        assert_eq!(table.voice(7), None);
    }

    /// Silent sentinels are ignored (sim events may carry `Sounds.none`).
    #[test]
    fn unset_sound_is_ignored() {
        let mut table = SoundLoopTable::new();
        let mut out = RecordingVoices::default();
        let mut silent = input(8, true);
        silent.sound = SoundId::NONE;
        table.begin_frame();
        table.update(silent, listener(), &mut out);
        assert!(table.is_empty());
        assert!(out.started.is_empty());
    }
}
