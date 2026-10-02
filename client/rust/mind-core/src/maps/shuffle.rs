// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map shuffle selection (plan 06 §3.9).
//!
//! Ported from `core/src/mindustry/maps/Maps.java` (`ShuffleMode`, `MapProvider`,
//! `next`, `valid`) and the PvP map list. Deviation §2.3.5: the candidate list is
//! shuffled with the caller's seeded [`JavaRandom`] (`SimRng::MapGen`) instead of
//! the process-global `Mathf.rand`, so selection is replay-stable.

use crate::random::JavaRandom;

use super::Map;
use super::PVP_MAPS;

/// Game modes a map can be valid for (plan 12's `Gamemode` seam; R3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameMode {
    /// `Gamemode.survival`.
    Survival,
    /// `Gamemode.sandbox`.
    Sandbox,
    /// `Gamemode.attack`.
    Attack,
    /// `Gamemode.pvp`.
    Pvp,
    /// `Gamemode.editor`.
    Editor,
}

impl GameMode {
    /// Java enum name.
    pub const fn name(self) -> &'static str {
        match self {
            GameMode::Survival => "survival",
            GameMode::Sandbox => "sandbox",
            GameMode::Attack => "attack",
            GameMode::Pvp => "pvp",
            GameMode::Editor => "editor",
        }
    }
}

/// Whether a map is in the built-in PvP list (`Maps.pvpMaps`).
pub fn is_pvp(map: &Map) -> bool {
    if map.custom {
        return false;
    }
    map.file_stem()
        .is_some_and(|stem| PVP_MAPS.iter().any(|name| *name == stem))
}

/// Default gamemode validity (`ShuffleMode.valid`).
pub fn default_valid(mode: GameMode, map: &Map) -> bool {
    let pvp = is_pvp(map);
    match mode {
        GameMode::Survival | GameMode::Attack | GameMode::Sandbox => !pvp,
        GameMode::Pvp => map.custom || pvp,
        GameMode::Editor => true,
    }
}

/// Map selection strategy (`Maps.ShuffleMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShuffleMode {
    /// No automatic selection (`none`).
    None,
    /// Custom maps first, else defaults (`all`).
    #[default]
    All,
    /// Custom only; falls back to defaults when none exist (`custom`).
    Custom,
    /// Built-in maps only (`builtin`).
    Builtin,
}

impl ShuffleMode {
    /// Java enum name.
    pub const fn name(self) -> &'static str {
        match self {
            ShuffleMode::None => "none",
            ShuffleMode::All => "all",
            ShuffleMode::Custom => "custom",
            ShuffleMode::Builtin => "builtin",
        }
    }

    /// Parses a mode name (`ShuffleMode.valueOf`-ish); unknown → [`ShuffleMode::All`].
    pub fn from_name(name: &str) -> ShuffleMode {
        match name {
            "none" => ShuffleMode::None,
            "custom" => ShuffleMode::Custom,
            "builtin" => ShuffleMode::Builtin,
            _ => ShuffleMode::All,
        }
    }

    /// Candidate indices for this mode (`all(custom)`/`custom`/`builtin`).
    pub fn candidates(self, maps: &[Map]) -> Vec<usize> {
        let default: Vec<usize> = maps
            .iter()
            .enumerate()
            .filter(|(_, map)| !map.custom && map.mod_id.is_none())
            .map(|(index, _)| index)
            .collect();
        let custom: Vec<usize> = maps
            .iter()
            .enumerate()
            .filter(|(_, map)| map.custom)
            .map(|(index, _)| index)
            .collect();
        match self {
            ShuffleMode::None => Vec::new(),
            ShuffleMode::All => {
                let mut out = custom;
                out.extend(default);
                out
            }
            ShuffleMode::Custom => {
                if custom.is_empty() {
                    default
                } else {
                    custom
                }
            }
            ShuffleMode::Builtin => default,
        }
    }

    /// `ShuffleMode.next`: shuffle candidates, pick the first valid non-previous.
    ///
    /// `previous` is an index into `maps`; the same map is only returned again
    /// when it is the sole candidate (`maps.size == 1` upstream).
    pub fn next(
        self,
        mode: GameMode,
        previous: Option<usize>,
        maps: &[Map],
        rng: &mut JavaRandom,
    ) -> Option<usize> {
        let mut candidates = self.candidates(maps);
        if candidates.is_empty() {
            return None;
        }
        shuffle(&mut candidates, rng);
        let single = candidates.len() == 1;
        candidates.into_iter().find(|index| {
            (Some(*index) != previous || single) && default_valid(mode, &maps[*index])
        })
    }
}

impl MapProvider for ShuffleMode {
    fn next_index(
        &self,
        mode: GameMode,
        previous: Option<usize>,
        maps: &[Map],
        rng: &mut JavaRandom,
    ) -> Option<usize> {
        self.next(mode, previous, maps, rng)
    }
}

/// A custom map selector (`Maps.MapProvider` / `Maps.setMapProvider`).
pub trait MapProvider: Send + Sync {
    /// Returns the index of the next map, or `None`.
    fn next_index(
        &self,
        mode: GameMode,
        previous: Option<usize>,
        maps: &[Map],
        rng: &mut JavaRandom,
    ) -> Option<usize>;
}

/// In-place Fisher-Yates shuffle seeded by `rng` (Arc `Seq.shuffle` shape).
fn shuffle(values: &mut [usize], rng: &mut JavaRandom) {
    for i in (1..values.len()).rev() {
        let j = rng.next_int_bound((i + 1) as i32) as usize;
        values.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn map(name: &str, custom: bool) -> Map {
        let mut tags = indexmap::IndexMap::new();
        tags.insert("name".to_owned(), name.to_owned());
        Map::new(
            PathBuf::from(format!("/maps/{name}.msav")),
            16,
            16,
            tags,
            custom,
            1,
            -1,
        )
    }

    /// `maps::tests::shuffle_none_all_custom_builtin`.
    #[test]
    fn shuffle_modes_select_candidates() {
        let maps = vec![map("fork", false), map("veins", false), map("my-map", true)];
        let mut rng = JavaRandom::new(7);

        assert!(
            ShuffleMode::None
                .next(GameMode::Survival, None, &maps, &mut rng)
                .is_none()
        );
        // all: custom + default, non-pvp only for survival.
        let all = ShuffleMode::All.next(GameMode::Survival, None, &maps, &mut rng);
        assert!(matches!(all, Some(0) | Some(2)));
        // custom: the single custom map.
        assert_eq!(
            ShuffleMode::Custom.next(GameMode::Survival, None, &maps, &mut rng),
            Some(2)
        );
        // builtin: defaults only.
        assert_eq!(
            ShuffleMode::Builtin.next(GameMode::Survival, None, &maps, &mut rng),
            Some(0)
        );
        // pvp mode: pvp builtin + custom.
        let pvp = ShuffleMode::All.next(GameMode::Pvp, None, &maps, &mut rng);
        assert!(matches!(pvp, Some(1) | Some(2)));
        // a single candidate can repeat the previous map.
        let one = vec![map("fork", false)];
        assert_eq!(
            ShuffleMode::Builtin.next(GameMode::Survival, Some(0), &one, &mut rng),
            Some(0)
        );
    }
}
