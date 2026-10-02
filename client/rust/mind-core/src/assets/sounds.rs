// SPDX-License-Identifier: GPL-3.0-only
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: annotations `AssetsProcess.processSounds` (registry shape, names/ids,
//         duplicate rejection), `core/src/mindustry/Vars.java` (`Sounds.none`/
//         `Music` dummies), `core/src/mindustry/audio/Sounds.java`.

//! Runtime `Sounds`/`Musics` registry (plan 03 §3.3/§6.5/M8).
//!
//! Parses the generated `assets/sounds.index.json` into name/id maps with
//! append-only ids, `none`/`unset` virtual dummies and hard duplicate
//! rejection. File playback (`AudioStream` decode) lives in `mind-gdext`;
//! priority/looping is plan 18 (handshake: registry only).

use std::collections::HashMap;

use serde::Deserialize;

/// One sound entry as written by `mind-tools sounds index`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SoundEntry {
    /// Generated field name (keyword-mangled; parity ABI).
    pub name: String,
    /// Asset path (`sounds/<category>/<file>`), `None` for virtual dummies.
    #[serde(default)]
    pub file: Option<String>,
    /// Dense append-only id, `-1` for virtual dummies.
    pub id: i32,
    /// Top-level sound folder, `None` when root/dummy.
    #[serde(default)]
    pub category: Option<String>,
}

impl SoundEntry {
    /// Whether this is a virtual dummy (`none`/`unset`).
    pub fn is_dummy(&self) -> bool {
        self.id < 0
    }
}

/// One music entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct MusicEntry {
    /// Generated field name (keyword-mangled).
    pub name: String,
    /// Asset path (`music/<file>`).
    pub file: String,
}

/// Parsed `assets/sounds.index.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SoundsIndex {
    /// Format version.
    pub format: u32,
    /// Sounds (dummies first, then dense-id order).
    pub sounds: Vec<SoundEntry>,
    /// Musics in file-name order.
    pub musics: Vec<MusicEntry>,
}

/// Errors from registry construction.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum SoundsLoadError {
    /// JSON parse failure.
    #[error("sounds index parse: {0}")]
    Parse(String),
    /// Unsupported format version.
    #[error("sounds index format {0} unsupported (expected 1)")]
    Format(u32),
    /// Duplicate sound name.
    #[error("duplicate sound name `{0}`")]
    DuplicateName(String),
    /// Duplicate sound id.
    #[error("duplicate sound id {0}")]
    DuplicateId(i32),
    /// Duplicate music name.
    #[error("duplicate music name `{0}`")]
    DuplicateMusic(String),
}

/// `Sounds` registry (name → entry, id → entry); dummies are name-only.
#[derive(Debug, Clone, Default)]
pub struct Sounds {
    entries: Vec<SoundEntry>,
    by_name: HashMap<String, usize>,
    by_id: HashMap<i32, usize>,
}

impl Sounds {
    /// Builds the registry from a parsed index, rejecting duplicates.
    pub fn from_index(index: &SoundsIndex) -> Result<Self, SoundsLoadError> {
        if index.format != 1 {
            return Err(SoundsLoadError::Format(index.format));
        }
        let mut sounds = Sounds {
            entries: index.sounds.clone(),
            ..Sounds::default()
        };
        for (position, entry) in index.sounds.iter().enumerate() {
            if sounds
                .by_name
                .insert(entry.name.clone(), position)
                .is_some()
            {
                return Err(SoundsLoadError::DuplicateName(entry.name.clone()));
            }
            if !entry.is_dummy() && sounds.by_id.insert(entry.id, position).is_some() {
                return Err(SoundsLoadError::DuplicateId(entry.id));
            }
        }
        Ok(sounds)
    }

    /// Parses and builds from `sounds.index.json` text.
    pub fn from_index_json(text: &str) -> Result<Self, SoundsLoadError> {
        let index: SoundsIndex = serde_json::from_str(text)
            .map_err(|error| SoundsLoadError::Parse(error.to_string()))?;
        Sounds::from_index(&index)
    }

    /// Sound entry by name.
    pub fn get(&self, name: &str) -> Option<&SoundEntry> {
        self.by_name
            .get(name)
            .map(|position| &self.entries[*position])
    }

    /// Sound entry by dense id (dummies are not addressable by id).
    pub fn by_id(&self, id: i32) -> Option<&SoundEntry> {
        self.by_id.get(&id).map(|position| &self.entries[*position])
    }

    /// Number of entries (including dummies).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// All entries in index order.
    pub fn entries(&self) -> &[SoundEntry] {
        &self.entries
    }
}

/// `Musics` registry (name → entry).
#[derive(Debug, Clone, Default)]
pub struct Musics {
    entries: Vec<MusicEntry>,
    by_name: HashMap<String, usize>,
}

impl Musics {
    /// Builds the registry from a parsed index, rejecting duplicates.
    pub fn from_index(index: &SoundsIndex) -> Result<Self, SoundsLoadError> {
        if index.format != 1 {
            return Err(SoundsLoadError::Format(index.format));
        }
        let mut musics = Musics {
            entries: index.musics.clone(),
            ..Musics::default()
        };
        for (position, entry) in index.musics.iter().enumerate() {
            if musics
                .by_name
                .insert(entry.name.clone(), position)
                .is_some()
            {
                return Err(SoundsLoadError::DuplicateMusic(entry.name.clone()));
            }
        }
        Ok(musics)
    }

    /// Parses and builds from `sounds.index.json` text.
    pub fn from_index_json(text: &str) -> Result<Self, SoundsLoadError> {
        let index: SoundsIndex = serde_json::from_str(text)
            .map_err(|error| SoundsLoadError::Parse(error.to_string()))?;
        Musics::from_index(&index)
    }

    /// Music entry by name.
    pub fn get(&self, name: &str) -> Option<&MusicEntry> {
        self.by_name
            .get(name)
            .map(|position| &self.entries[*position])
    }

    /// Number of musics.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// All entries in index order.
    pub fn entries(&self) -> &[MusicEntry] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"{
  "format": 1,
  "sounds": [
    {"name":"none","file":null,"id":-1,"category":null},
    {"name":"unset","file":null,"id":-1,"category":null},
    {"name":"click","file":"sounds/ui/click.ogg","id":0,"category":"ui"},
    {"name":"news","file":"sounds/ui/new.ogg","id":1,"category":"ui"}
  ],
  "musics": [{"name":"game1","file":"music/game1.ogg"}]
}"#;

    #[test]
    fn names_ids_and_duplicates() {
        let sounds = Sounds::from_index_json(INDEX).unwrap();
        assert_eq!(sounds.len(), 4);
        assert_eq!(sounds.get("click").unwrap().id, 0);
        assert_eq!(sounds.by_id(1).unwrap().name, "news");
        assert!(sounds.get("none").unwrap().is_dummy());
        assert!(sounds.by_id(-1).is_none());
        assert!(sounds.get("missing").is_none());

        let musics = Musics::from_index_json(INDEX).unwrap();
        assert_eq!(musics.get("game1").unwrap().file, "music/game1.ogg");

        // Duplicate name rejection.
        let dup = INDEX.replace("\"name\":\"news\"", "\"name\":\"click\"");
        assert!(matches!(
            Sounds::from_index_json(&dup),
            Err(SoundsLoadError::DuplicateName(name)) if name == "click"
        ));
        // Duplicate id rejection.
        let dup_id = INDEX.replace("\"id\":1", "\"id\":0");
        assert!(matches!(
            Sounds::from_index_json(&dup_id),
            Err(SoundsLoadError::DuplicateId(0))
        ));
    }
}
