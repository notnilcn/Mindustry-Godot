// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BaseGenerator` — enemy/ally base decoration (plan 06 §3.10, risk R3).
//!
//! Ported from `maps/generators/BaseGenerator.java`. The full layout depends on
//! plan 11's `BaseRegistry` and plan 12's `Schematics`; both are trait seams
//! here. With an empty registry the generator early-returns exactly like
//! upstream ("don't generate bases when there are no loaded schematics").

use crate::content::ContentRegistry;
use crate::world::tiles::Tiles;

/// Schematic placement hooks (plan 12 `Schematics`).
pub trait SchematicHooks: Send + Sync {
    /// `Schematics.placeLaunchLoadout(x, y)`.
    fn place_launch_loadout(
        &self,
        _tiles: &mut Tiles,
        _x: i32,
        _y: i32,
        _content: &ContentRegistry,
    ) {
    }

    /// `Schematics.placeLoadout(schematic, x, y, team, replace)`.
    fn place_loadout(
        &self,
        _schematic: &str,
        _tiles: &mut Tiles,
        _x: i32,
        _y: i32,
        _team: u8,
        _content: &ContentRegistry,
    ) {
    }

    /// `Schematics.rotate(schematic, rotations)` (returns placed tiles).
    fn rotate(&self, _schematic: &str, _rotation: i32) -> Option<Vec<(i32, i32, u16)>> {
        None
    }
}

/// A no-op [`SchematicHooks`] (core default until plan 12).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopSchematicHooks;

impl SchematicHooks for NoopSchematicHooks {}

/// The base-part/schematic registry (plan 11 `BaseRegistry`).
pub trait BaseRegistryView: Send + Sync {
    /// Loaded core schematic names (`bases.cores`).
    fn cores(&self) -> &[String];

    /// Whether no schematics are loaded (`bases.cores.isEmpty()`).
    fn is_empty(&self) -> bool {
        self.cores().is_empty()
    }

    /// Parts usable for a resource (`bases.forResource`).
    fn for_resource(&self, _resource: &str) -> Vec<String> {
        Vec::new()
    }

    /// Any random parts (`bases.parts`).
    fn parts(&self) -> &[String] {
        &[]
    }
}

/// An empty [`BaseRegistryView`] (core default).
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyBaseRegistry;

impl BaseRegistryView for EmptyBaseRegistry {
    fn cores(&self) -> &[String] {
        &[]
    }
}

/// Ported from `maps/generators/BaseGenerator.java`.
///
/// The full base-layout algorithm requires plan 11's `BaseRegistry` and plan
/// 12's `Schematics`; with an empty registry it early-returns (upstream parity).
#[derive(Debug, Default, Clone, Copy)]
pub struct BaseGenerator;

impl BaseGenerator {
    /// `BaseGenerator.generate(tiles, cores, spawn, team, sector, difficulty)`.
    #[allow(clippy::too_many_arguments)]
    pub fn generate(
        &self,
        tiles: &mut Tiles,
        cores: &[(i32, i32)],
        spawn: (i32, i32),
        team: u8,
        difficulty: f32,
        bases: &dyn BaseRegistryView,
        schematics: &dyn SchematicHooks,
        content: &ContentRegistry,
    ) {
        if bases.is_empty() {
            return;
        }
        // Full base placement lands with plans 11/12 (BaseRegistry + Schematics).
        log::debug!(
            "BaseGenerator full layout is a plan-11/12 hook ({} cores, difficulty {difficulty})",
            cores.len()
        );
        let _ = (tiles, spawn, team, schematics, content);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `maps::generators::tests::base_generator_empty_registry_returns`.
    #[test]
    fn base_generator_empty_registry_returns() {
        let content = crate::content::test_support::test_registry();
        let mut tiles = Tiles::new(8, 8);
        let generator = BaseGenerator;
        generator.generate(
            &mut tiles,
            &[(2, 2)],
            (0, 0),
            0,
            0.5,
            &EmptyBaseRegistry,
            &NoopSchematicHooks,
            &content,
        );
        // Tiles untouched.
        assert!(
            tiles
                .iter()
                .all(|tile| tile.block == crate::content::BlockId::AIR)
        );
    }
}
