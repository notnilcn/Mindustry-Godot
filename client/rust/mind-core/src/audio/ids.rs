// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SoundId`/`MusicRef` runtime ids and `findMusic` resolution (plan 18 §3.2).
//!
//! `SoundId` is re-exported from the plan-02 seed table: it is already the
//! content-facing id, serializes by name and carries the `none`/`unset`
//! sentinels, so plan 18 does not fork a second id space (reconciliation note in
//! the plan Changelog). `MusicRef` is the serializable lazy handle and is
//! byte-compatible with plan 04's `MusicContainer` name string.

use serde::{Deserialize, Serialize};

pub use crate::content::registries::sound_meta::SoundId;

/// Stable per-instance loop voice key (turret/unit/weather instance).
pub type VoiceKey = u64;

/// Serializable, lazily-resolved music handle (`MusicContainer` equivalent).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MusicRef(pub String);

impl MusicRef {
    /// Creates a handle from a name.
    pub fn new(name: impl Into<String>) -> Self {
        MusicRef(name.into())
    }

    /// The music name.
    pub fn name(&self) -> &str {
        &self.0
    }
}

impl From<&str> for MusicRef {
    fn from(value: &str) -> Self {
        MusicRef(value.to_owned())
    }
}

impl From<String> for MusicRef {
    fn from(value: String) -> Self {
        MusicRef(value)
    }
}

impl std::fmt::Display for MusicRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Registry-facing bus routing (plan 18 §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BusKind {
    /// `Sound` bus (all `sounds/*` one-shots, loops, ambient).
    #[default]
    Sound,
    /// `UI` bus (`sounds/ui/*` + forced `coreLaunch`).
    Ui,
    /// `Music` bus (music + audition).
    Music,
}

impl BusKind {
    /// Parity name of the bus node (`Master` is Godot's own).
    pub const fn name(self) -> &'static str {
        match self {
            BusKind::Sound => "Sound",
            BusKind::Ui => "UI",
            BusKind::Music => "Music",
        }
    }
}

/// Whether a registry sound file routes to the `UI` bus
/// (`file.parent().name().equals("ui")`). `coreLaunch` is forced by
/// `SoundPriority.init` (plan 18 §6.3).
pub fn bus_for_sound(name: &str, file: Option<&str>) -> BusKind {
    if name == "coreLaunch" {
        return BusKind::Ui;
    }
    match file {
        Some(path)
            if path.starts_with("sounds/ui/")
                || path.starts_with("ui/")
                || path.starts_with("sounds\\ui\\") =>
        {
            BusKind::Ui
        }
        _ => BusKind::Sound,
    }
}

/// A name→music-path catalog (`Musics` registry) used by `findMusic`.
pub trait MusicCatalog {
    /// Whether a music name is known.
    fn contains(&self, name: &str) -> bool;
}

impl MusicCatalog for crate::assets::sounds::Musics {
    fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

/// Port of `SoundControl.findMusic` (plan 18 §3.2/§3.9).
///
/// Tries the exact registry name, `name.ogg`, `name.mp3`, `music/name.ogg`,
/// `music/name.mp3`, then (only with mods) the `music/name` mod-tree path.
/// Returns the resolved handle, or `None` on a miss (Java returns `null`).
pub fn find_music_chain(
    name: &str,
    catalog: &impl MusicCatalog,
    has_mods: bool,
) -> Option<MusicRef> {
    if name.is_empty() {
        return None;
    }
    if catalog.contains(name) {
        return Some(MusicRef::new(name));
    }
    for candidate in [
        format!("{name}.ogg"),
        format!("{name}.mp3"),
        format!("music/{name}.ogg"),
        format!("music/{name}.mp3"),
    ] {
        if catalog.contains(&candidate) {
            return Some(MusicRef::new(candidate));
        }
    }
    if has_mods {
        // `FileTree.getAudioPath("music/" + name)` marker; the 20 lane resolves
        // the actual overlay stream. Represented here as the mod-tree path.
        return Some(MusicRef::new(format!("music/{name}")));
    }
    None
}

/// `Core.settings` music/sound keys (plan 18 §6.4; names are ABI).
pub mod settings_keys {
    /// Music volume, int 0..100.
    pub const MUSIC_VOLUME: &str = "musicvol";
    /// SFX volume, int 0..100.
    pub const SFX_VOLUME: &str = "sfxvol";
    /// Ambient/loop volume, int 0..100.
    pub const AMBIENT_VOLUME: &str = "ambientvol";
    /// Always-play-music toggle.
    pub const ALWAYS_MUSIC: &str = "alwaysmusic";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::sounds::Musics;

    #[test]
    fn find_music_chain_resolution() {
        let mut index = crate::assets::sounds::SoundsIndex {
            format: 1,
            sounds: Vec::new(),
            musics: Vec::new(),
        };
        index.musics.push(crate::assets::sounds::MusicEntry {
            name: "game1".to_owned(),
            file: "music/game1.ogg".to_owned(),
        });
        index.musics.push(crate::assets::sounds::MusicEntry {
            name: "music/beta.ogg".to_owned(),
            file: "music/beta.ogg".to_owned(),
        });
        index.musics.push(crate::assets::sounds::MusicEntry {
            name: "alpha.mp3".to_owned(),
            file: "music/alpha.mp3".to_owned(),
        });
        let catalog = Musics::from_index(&index).unwrap();

        assert_eq!(
            find_music_chain("game1", &catalog, false),
            Some(MusicRef::new("game1"))
        );
        assert_eq!(
            find_music_chain("music/beta.ogg", &catalog, false),
            Some(MusicRef::new("music/beta.ogg"))
        );
        assert_eq!(
            find_music_chain("alpha.mp3", &catalog, false),
            Some(MusicRef::new("alpha.mp3"))
        );
        // Miss without mods.
        assert_eq!(find_music_chain("missing", &catalog, false), None);
        // Mod fallback path.
        assert_eq!(
            find_music_chain("missing", &catalog, true),
            Some(MusicRef::new("music/missing"))
        );
        assert_eq!(find_music_chain("", &catalog, true), None);
    }

    #[test]
    fn bus_routing() {
        assert_eq!(
            bus_for_sound("uiButton", Some("sounds/ui/uiButton.ogg")),
            BusKind::Ui
        );
        assert_eq!(
            bus_for_sound("coreLaunch", Some("sounds/block/coreLaunch.ogg")),
            BusKind::Ui
        );
        assert_eq!(
            bus_for_sound("loopConveyor", Some("sounds/loops/loopConveyor.ogg")),
            BusKind::Sound
        );
        assert_eq!(bus_for_sound("none", None), BusKind::Sound);
    }
}
