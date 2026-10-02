// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `DataAudioLoader` driver (plan 20 §3.7): `dp-` name prefixing, the
//! `soundIdOffset = 100_000` id space, the 100 000-byte streaming threshold and
//! the `@sfx-<name>` logic variables added/removed around load/unload.
//!
//! Godot-free and record-only: decoding/playback lives in `mind-gdext` (plan
//! 18). Headless still registers sound ids and logic vars exactly as upstream.

use super::{SOUND_ID_OFFSET, STREAM_THRESHOLD, audio_asset_name};

/// One mod sound/music asset to load (path + byte length is all the driver
/// needs; bytes stay in the asset/cache layer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioAsset {
    /// Root-relative source path (e.g. `sounds/ui/click.ogg`).
    pub path: String,
    /// Payload length in bytes.
    pub byte_len: usize,
}

impl AudioAsset {
    /// Convenience constructor.
    pub fn new(path: impl Into<String>, byte_len: usize) -> Self {
        Self {
            path: path.into(),
            byte_len,
        }
    }
}

/// One registered audio entry (`Sounds`/`Musics` record + id/vars).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioEntry {
    /// Register/region name (`dp-<lowercased stem>`).
    pub name: String,
    /// Raw lowercased stem (used for the `@sfx-` logic var).
    pub raw_name: String,
    /// Source path.
    pub path: String,
    /// Dense sound id (`100_000 +`); `-1` for music (name-addressed).
    pub id: i32,
    /// Whether playback streams from disk (`byte_len > 100_000`).
    pub streaming: bool,
    /// Whether this is a music track.
    pub music: bool,
}

impl AudioEntry {
    /// `Vars.logicVars` key for a sound (`@sfx-<name>`), `None` for music.
    pub fn logic_var(&self) -> Option<String> {
        (!self.music).then(|| format!("@sfx-{}", self.raw_name))
    }
}

/// `DataAudioLoader`: loads mod sounds/music into record form and unloads them.
#[derive(Debug, Default)]
pub struct AudioApplier {
    entries: Vec<AudioEntry>,
    next_sound_id: i32,
}

impl AudioApplier {
    /// Empty applier with the offset id space.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_sound_id: SOUND_ID_OFFSET,
        }
    }

    /// Loads sounds (`DataAudioLoader.load`), assigning dense ids continuing
    /// from `base_count` existing sounds.
    pub fn load_sounds(&mut self, sounds: &[AudioAsset], base_count: usize) -> &[AudioEntry] {
        let start = self.entries.len();
        for (offset, asset) in sounds.iter().enumerate() {
            let raw_name = asset
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&asset.path)
                .rsplit_once('.')
                .map(|(stem, _)| stem)
                .unwrap_or(&asset.path)
                .to_lowercase()
                .replace(' ', "_");
            let name = audio_asset_name(&asset.path);
            self.entries.push(AudioEntry {
                name,
                raw_name,
                path: asset.path.clone(),
                id: SOUND_ID_OFFSET + base_count as i32 + offset as i32,
                streaming: asset.byte_len > STREAM_THRESHOLD,
                music: false,
            });
        }
        &self.entries[start..]
    }

    /// Loads music (`DataAudioLoader.load`); music is name-addressed (`id = -1`).
    pub fn load_music(&mut self, music: &[AudioAsset]) -> &[AudioEntry] {
        let start = self.entries.len();
        for asset in music {
            let raw_name = asset
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&asset.path)
                .rsplit_once('.')
                .map(|(stem, _)| stem)
                .unwrap_or(&asset.path)
                .to_lowercase()
                .replace(' ', "_");
            let name = audio_asset_name(&asset.path);
            self.entries.push(AudioEntry {
                name,
                raw_name,
                path: asset.path.clone(),
                id: -1,
                streaming: asset.byte_len > STREAM_THRESHOLD,
                music: true,
            });
        }
        &self.entries[start..]
    }

    /// Advances the sound id counter past the highest allocated id.
    pub fn reserve_sound_id(&mut self, id: i32) {
        self.next_sound_id = self.next_sound_id.max(id);
    }

    /// All entries.
    pub fn entries(&self) -> &[AudioEntry] {
        &self.entries
    }

    /// Sound entries only.
    pub fn sounds(&self) -> impl Iterator<Item = &AudioEntry> {
        self.entries.iter().filter(|entry| !entry.music)
    }

    /// Logic variables seeded for loaded sounds (`@sfx-<name>`).
    pub fn logic_vars(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(AudioEntry::logic_var)
            .collect()
    }

    /// Number of loaded entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is loaded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Unloads everything, returning the removed entries (vars are dropped by
    /// the caller alongside the register names).
    pub fn unload(&mut self) -> Vec<AudioEntry> {
        self.next_sound_id = SOUND_ID_OFFSET;
        std::mem::take(&mut self.entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plan 20 M4: `assets::audio_id_offset_and_streaming` (driver half).
    #[test]
    fn ids_offset_and_streaming() {
        let mut applier = AudioApplier::new();
        applier.load_sounds(
            &[
                AudioAsset::new("sounds/ui/Click.ogg", 10),
                AudioAsset::new("sounds/Big Sound.ogg", STREAM_THRESHOLD + 1),
            ],
            3,
        );
        let entries = applier.entries();
        assert_eq!(entries[0].name, "dp-click");
        assert_eq!(entries[0].raw_name, "click");
        assert_eq!(entries[0].id, SOUND_ID_OFFSET + 3);
        assert!(!entries[0].streaming);
        assert_eq!(entries[1].name, "dp-big_sound");
        assert_eq!(entries[1].id, SOUND_ID_OFFSET + 4);
        assert!(entries[1].streaming);
        assert_eq!(
            applier.logic_vars(),
            vec!["@sfx-click".to_owned(), "@sfx-big_sound".to_owned()]
        );
    }

    #[test]
    fn music_has_no_id_or_logic_var() {
        let mut applier = AudioApplier::new();
        applier.load_music(&[AudioAsset::new("music/Game 1.ogg", 1)]);
        let entry = &applier.entries()[0];
        assert_eq!(entry.name, "dp-game_1");
        assert_eq!(entry.id, -1);
        assert!(entry.music);
        assert!(entry.logic_var().is_none());
        assert!(applier.logic_vars().is_empty());
    }

    #[test]
    fn unload_clears_and_resets_id_space() {
        let mut applier = AudioApplier::new();
        applier.load_sounds(&[AudioAsset::new("sounds/a.ogg", 1)], 0);
        assert_eq!(applier.len(), 1);
        let removed = applier.unload();
        assert_eq!(removed.len(), 1);
        assert!(applier.is_empty());
        applier.load_sounds(&[AudioAsset::new("sounds/b.ogg", 1)], 0);
        assert_eq!(applier.entries()[0].id, SOUND_ID_OFFSET);
    }
}
