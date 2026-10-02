// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `FileMapGenerator` — campaign map loading (plan 06 §3.10).
//!
//! Ported from `maps/generators/FileMapGenerator.java`. Candidate resolution,
//! the plan-04 `SaveIo` load into a `FilterContext`, schema/core resolution and
//! loadout placement span plans 04/07/12; this module owns the candidate name
//! resolution and the `All maps must have a core.` invariant.

use std::path::PathBuf;

use crate::assets::file_tree::FileTree;
use crate::content::{BlockKind, ContentRegistry};
use crate::io::fs::SAVE_EXTENSION;
use crate::world::WorldParams;
use crate::world::tiles::Tiles;

use super::WorldGenerator;
use crate::maps::MapError;

/// Resolves campaign map candidates (`FileMapGenerator` constructor).
///
/// Order: `<planet>/<map>`, `<map>`, and (for mod maps) the mod-prefix-stripped
/// variants. Returns the first candidate present in `tree`.
pub fn resolve_candidate(
    tree: &FileTree,
    planet: &str,
    map_name: &str,
    mod_prefix: Option<&str>,
) -> Option<String> {
    let mut candidates = Vec::with_capacity(4);
    candidates.push(format!("{planet}/{map_name}"));
    candidates.push(map_name.to_owned());
    if let Some(prefix) = mod_prefix
        && let Some(base) = map_name.strip_prefix(&format!("{prefix}-"))
    {
        candidates.push(format!("{planet}/{base}"));
        candidates.push(base.to_owned());
    }
    candidates
        .into_iter()
        .find(|name| tree.has(&format!("maps/{name}.{SAVE_EXTENSION}")))
}

/// Checks that a loaded map contains at least one core center
/// (`FileMapGenerator.generate` final check).
pub fn require_core(tiles: &Tiles, content: &ContentRegistry) -> Result<(), MapError> {
    for index in 0..tiles.len() {
        let tile = tiles.geti(index);
        let is_core = content
            .block(tile.block)
            .is_some_and(|def| def.kind == BlockKind::CoreBlock);
        if is_core {
            return Ok(());
        }
    }
    Err(MapError::NoCore)
}

/// A campaign map generator (`FileMapGenerator`).
#[derive(Debug, Clone)]
pub struct FileMapGenerator {
    /// Owning map file, when resolved.
    pub file: Option<PathBuf>,
    /// Map display name.
    pub map_name: String,
    /// Planet content name.
    pub planet: String,
}

impl FileMapGenerator {
    /// Builds a generator for a planet/map pair.
    pub fn new(planet: impl Into<String>, map_name: impl Into<String>) -> Self {
        Self {
            file: None,
            map_name: map_name.into(),
            planet: planet.into(),
        }
    }

    /// Builds a generator for an already-resolved map file.
    pub fn with_file(
        file: PathBuf,
        planet: impl Into<String>,
        map_name: impl Into<String>,
    ) -> Self {
        Self {
            file: Some(file),
            map_name: map_name.into(),
            planet: planet.into(),
        }
    }
}

impl WorldGenerator for FileMapGenerator {
    fn name(&self) -> &'static str {
        "file"
    }

    fn generate(&mut self, tiles: &mut Tiles, _params: &WorldParams, content: &ContentRegistry) {
        // The caller loads the map through plan-04 `SaveIo` + `FilterContext`
        // (which needs plan 07's building runtime); here we enforce the core
        // invariant on the loaded grid.
        if let Err(error) = require_core(tiles, content) {
            log::error!("file map `{}` failed: {error}", self.map_name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::file_tree::AssetFile;

    /// `maps::generators::tests::file_map_requires_core`.
    #[test]
    fn file_map_requires_core() {
        let content = crate::content::test_support::test_registry();
        let empty = Tiles::new(8, 8);
        assert!(matches!(
            require_core(&empty, &content),
            Err(MapError::NoCore)
        ));

        let mut tiles = Tiles::new(8, 8);
        let core = content.block_id("core-shard").unwrap();
        tiles.get_mut(4, 4).block = core;
        assert!(require_core(&tiles, &content).is_ok());
    }

    #[test]
    fn candidate_resolution_order() {
        let mut tree = FileTree::new();
        tree.add_internal(
            "maps/serpulo/groundZero.msav",
            AssetFile::new("maps/serpulo/groundZero.msav", vec![1]),
        );
        assert_eq!(
            resolve_candidate(&tree, "serpulo", "groundZero", None).as_deref(),
            Some("serpulo/groundZero")
        );
        // No `erekir/` or bare candidate exists.
        assert!(resolve_candidate(&tree, "erekir", "groundZero", None).is_none());
        // A bare map in `maps/` resolves as the fallback candidate.
        tree.add_internal(
            "maps/groundZero.msav",
            AssetFile::new("maps/groundZero.msav", vec![2]),
        );
        assert_eq!(
            resolve_candidate(&tree, "erekir", "groundZero", None).as_deref(),
            Some("groundZero")
        );
    }
}
