// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// One-shot voice pool; Arc `Sound.at/play` + `Audio.defaultSoundMaxConcurrent`.

//! Fixed one-shot voice pool (plan 18 §3.6). All nodes are created once at
//! `MindAudio` ready (no runtime node creation, §7c assertion).

use godot::classes::{AudioStream, AudioStreamPlayer, AudioStreamPlayer2D, Node, Node2D};
use godot::prelude::*;

/// World one-shot voices (`MAX_VOICES`).
pub const MAX_VOICES: usize = 128;
/// UI one-shot voices (`MAX_UI_VOICES`).
pub const MAX_UI_VOICES: usize = 16;
/// Looping world voices (aggregated loops + `SoundLoop` instances).
pub const MAX_LOOP_VOICES: usize = 32;

/// Pooled non-positional + positional one-shot voices.
pub struct VoicePool {
    world: Vec<Gd<AudioStreamPlayer2D>>,
    ui: Vec<Gd<AudioStreamPlayer>>,
    world_cursor: usize,
    ui_cursor: usize,
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
        }
    }

    /// Number of currently playing world + UI voices.
    pub fn active(&self) -> usize {
        self.world
            .iter()
            .filter(|player| player.is_playing())
            .count()
            + self.ui.iter().filter(|player| player.is_playing()).count()
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
        let player = &mut self.world[index];
        player.set_stream(&stream);
        player.set_volume_linear(volume.clamp(0.0, 1.0));
        player.set_pitch_scale(pitch.clamp(0.0001, 10.0));
        // Pan via node offset from the listener (attenuation 0); plan 18 §2.4 dev 5.
        let x = listener.0 + pan * viewport_width / 2.0;
        player.set_position(Vector2::new(x, listener.1));
        player.play();
        index as i64
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
