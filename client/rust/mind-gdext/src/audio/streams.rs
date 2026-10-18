// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Audio stream cache; upstream `MusicContainer` lazy access + Arc `Sound`/`Music`.

//! `StreamCache` — name → `AudioStream` with normal/looping variants (plan 18
//! §3.2, deviation 7). Builds on plan 03's `sounds.index.json` registry.

use std::collections::HashMap;

use godot::classes::{AudioStream, AudioStreamMp3, AudioStreamOggVorbis, FileAccess};
use godot::prelude::*;

use mind_core::assets::sounds::Sounds;

/// Lazily decoded audio streams for one asset root.
#[derive(Default)]
pub struct StreamCache {
    assets_dir: String,
    sounds: Sounds,
    streams: HashMap<(String, bool), Gd<AudioStream>>,
    /// Data-asset streams (`dp-<name>`, PAR-0394): name → (music, bytes).
    data: HashMap<String, (bool, Vec<u8>)>,
}

impl StreamCache {
    /// Loads the sound registry from `assets/sounds.index.json`.
    pub fn load(assets_dir: &str) -> Self {
        let mut cache = StreamCache {
            assets_dir: assets_dir.to_owned(),
            ..StreamCache::default()
        };
        let path = format!("{assets_dir}/sounds.index.json");
        if FileAccess::file_exists(&path) {
            let text = FileAccess::get_file_as_string(&path).to_string();
            match Sounds::from_index_json(&text) {
                Ok(sounds) => cache.sounds = sounds,
                Err(error) => log::error!("[audio] sounds index rejected: {error}"),
            }
        }
        cache
    }

    /// Number of registered sounds.
    pub fn sound_count(&self) -> usize {
        self.sounds.len()
    }

    /// Registry file path for a sound (`Sounds` index `file`), for UI-bus
    /// routing (`SoundControl.java:98-104`).
    pub fn file_of(&self, name: &str) -> Option<&str> {
        self.sounds.get(name)?.file.as_deref()
    }

    /// Decodes a `sounds/*` stream (`looping` sets the loop flag), cached.
    pub fn sound(&mut self, name: &str, looping: bool) -> Option<Gd<AudioStream>> {
        if let Some(stream) = self.streams.get(&(name.to_owned(), looping)) {
            return Some(stream.clone());
        }
        if let Some((music, bytes)) = self.data.get(name)
            && !music
        {
            let stream = decode_looping(bytes.as_slice(), looping)?;
            self.streams
                .insert((name.to_owned(), looping), stream.clone());
            return Some(stream);
        }
        let file = self.sounds.get(name)?.file.clone()?;
        let stream = decode_stream(&self.assets_dir, &file, looping)?;
        self.streams
            .insert((name.to_owned(), looping), stream.clone());
        Some(stream)
    }

    /// Decodes a `music/*` stream, cached (loop variant for regular music).
    pub fn music(&mut self, name: &str) -> Option<Gd<AudioStream>> {
        let key = (name.to_owned(), true);
        if let Some(stream) = self.streams.get(&key) {
            return Some(stream.clone());
        }
        if let Some((is_music, bytes)) = self.data.get(name)
            && *is_music
        {
            let stream = decode_looping(bytes.as_slice(), true)?;
            self.streams.insert(key, stream.clone());
            return Some(stream);
        }
        // Music names may or may not carry an extension; try `music/<name>` then
        // the raw name (plan 18 `findMusic` fallbacks).
        let candidates = [format!("music/{name}"), name.to_owned()];
        for candidate in candidates {
            // Normalize once: the decision must be made on the extension-carrying
            // path, and `decode_bytes` needs it too (T1/PAR-0074).
            let file = normalize(&candidate);
            let path = format!("{}/{}", self.assets_dir, file);
            if FileAccess::file_exists(&path) {
                let bytes = FileAccess::get_file_as_bytes(&path);
                let stream = decode_bytes(&file, &bytes, true);
                if let Some(stream) = stream {
                    self.streams.insert(key, stream.clone());
                    return Some(stream);
                }
            }
        }
        None
    }

    /// Registers one data-asset stream (`DataAudioLoader` → `Sound`/`Music`).
    /// Names are the manager's `dp-<name>` entries; the decoded stream is
    /// cached per loop flag and the override beats a base sound of that name.
    pub fn add_data_stream(&mut self, name: &str, bytes: Vec<u8>, music: bool) {
        self.data.insert(name.to_owned(), (music, bytes));
        self.streams
            .retain(|(stream_name, _), _| stream_name != name);
    }

    /// Drops every data-asset stream (`DataManager.unload`).
    pub fn clear_data_streams(&mut self) {
        self.data.clear();
        self.streams.retain(|(name, _), _| !name.starts_with("dp-"));
    }

    /// Number of registered data-asset streams.
    pub fn data_stream_count(&self) -> usize {
        self.data.len()
    }

    /// Whether a name is backed by a data-asset stream (`dp-<name>`).
    pub fn is_data_stream(&self, name: &str) -> bool {
        self.data.contains_key(name)
    }
}

fn normalize(name: &str) -> String {
    if name.ends_with(".ogg") || name.ends_with(".mp3") {
        name.to_owned()
    } else {
        format!("{name}.ogg")
    }
}

/// Decodes an `.ogg`/`.mp3` file into an `AudioStream` (`None` on failure).
fn decode_stream(assets_dir: &str, file: &str, looping: bool) -> Option<Gd<AudioStream>> {
    let path = format!("{assets_dir}/{file}");
    if !FileAccess::file_exists(&path) {
        return None;
    }
    let bytes = FileAccess::get_file_as_bytes(&path);
    decode_bytes(file, &bytes, looping)
}

/// Decodes raw bytes, applying the loop flag.
fn decode_bytes(file: &str, bytes: &PackedByteArray, looping: bool) -> Option<Gd<AudioStream>> {
    if file.ends_with(".ogg") {
        let mut stream = AudioStreamOggVorbis::load_from_buffer(bytes)?;
        stream.set_loop(looping);
        Some(stream.upcast::<AudioStream>())
    } else if file.ends_with(".mp3") {
        let mut stream = AudioStreamMp3::load_from_buffer(bytes)?;
        stream.set_loop(looping);
        Some(stream.upcast::<AudioStream>())
    } else {
        None
    }
}

/// Decodes data-asset bytes without an extension hint (`dp-` assets keep the
/// source name, whose extension may be `.ogg` or `.mp3`); tries OGG first,
/// then MP3, mirroring `DataAudioLoader`'s format sniffing.
fn decode_looping(bytes: &[u8], looping: bool) -> Option<Gd<AudioStream>> {
    let data = PackedByteArray::from(bytes);
    if let Some(mut stream) = AudioStreamOggVorbis::load_from_buffer(&data) {
        stream.set_loop(looping);
        return Some(stream.upcast::<AudioStream>());
    }
    if let Some(mut stream) = AudioStreamMp3::load_from_buffer(&data) {
        stream.set_loop(looping);
        return Some(stream.upcast::<AudioStream>());
    }
    None
}
