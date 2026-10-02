// SPDX-License-Identifier: GPL-3.0-only

//! Lazy `AudioStream` cache (plan 03 §3.6/M8).
//!
//! Builds the `Sounds`/`Musics` registries from `assets/sounds.index.json` and
//! decodes `AudioStreamOggVorbis`/`AudioStreamMp3` on demand, keyed by name.
//! Playback, looping and priority are plan 18 (handshake: this registry only).

use std::collections::HashMap;

use godot::classes::{AudioStream, AudioStreamMp3, AudioStreamOggVorbis, FileAccess};
use godot::prelude::*;

use mind_core::assets::sounds::{MusicEntry, Musics, SoundEntry, Sounds};

/// Audio registry + lazy stream cache for one asset root.
#[derive(Default)]
pub struct AudioRegistry {
    assets_dir: String,
    sounds: Sounds,
    musics: Musics,
    streams: HashMap<String, Gd<AudioStream>>,
    music_streams: HashMap<String, Gd<AudioStream>>,
}

impl AudioRegistry {
    /// Loads `assets/sounds.index.json` (empty registry when absent).
    pub fn load(assets_dir: &str) -> Self {
        let mut registry = AudioRegistry {
            assets_dir: assets_dir.to_owned(),
            ..AudioRegistry::default()
        };
        let path = format!("{assets_dir}/sounds.index.json");
        if !FileAccess::file_exists(&path) {
            return registry;
        }
        let text = FileAccess::get_file_as_string(&path).to_string();
        match Sounds::from_index_json(&text) {
            Ok(sounds) => {
                if let Ok(musics) = Musics::from_index_json(&text) {
                    registry.musics = musics;
                }
                registry.sounds = sounds;
            }
            Err(error) => log::error!("[assets] sounds index rejected: {error}"),
        }
        registry
    }

    /// Sound entry by name.
    pub fn sound(&self, name: &str) -> Option<&SoundEntry> {
        self.sounds.get(name)
    }

    /// Music entry by name.
    pub fn music(&self, name: &str) -> Option<&MusicEntry> {
        self.musics.get(name)
    }

    /// Number of sound entries (including dummies).
    pub fn sound_count(&self) -> usize {
        self.sounds.len()
    }

    /// Number of music entries.
    pub fn music_count(&self) -> usize {
        self.musics.len()
    }

    /// Lazily decodes and caches a sound's stream (`None` for dummies/missing).
    pub fn sound_stream(&mut self, name: &str) -> Option<Gd<AudioStream>> {
        if let Some(stream) = self.streams.get(name) {
            return Some(stream.clone());
        }
        let file = self.sounds.get(name)?.file.clone()?;
        let stream = decode_stream(&self.assets_dir, &file)?;
        self.streams.insert(name.to_owned(), stream.clone());
        Some(stream)
    }

    /// Lazily decodes and caches a music's stream.
    pub fn music_stream(&mut self, name: &str) -> Option<Gd<AudioStream>> {
        if let Some(stream) = self.music_streams.get(name) {
            return Some(stream.clone());
        }
        let file = self.musics.get(name)?.file.clone();
        let stream = decode_stream(&self.assets_dir, &file)?;
        self.music_streams.insert(name.to_owned(), stream.clone());
        Some(stream)
    }
}

/// Decodes an `.ogg`/`.mp3` file into an `AudioStream` (`None` on failure).
fn decode_stream(assets_dir: &str, file: &str) -> Option<Gd<AudioStream>> {
    let path = format!("{assets_dir}/{file}");
    if !FileAccess::file_exists(&path) {
        return None;
    }
    let bytes = FileAccess::get_file_as_bytes(&path);
    if file.ends_with(".ogg") {
        AudioStreamOggVorbis::load_from_buffer(&bytes).map(|stream| stream.upcast::<AudioStream>())
    } else if file.ends_with(".mp3") {
        AudioStreamMp3::load_from_buffer(&bytes).map(|stream| stream.upcast::<AudioStream>())
    } else {
        None
    }
}
