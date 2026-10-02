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
