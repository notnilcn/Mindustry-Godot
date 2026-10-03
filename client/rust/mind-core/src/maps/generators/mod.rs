// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! World generators (`maps/generators/*`, plan 06 §3.10).
//!
//! Ported from `maps/generators/{WorldGenerator,BasicGenerator,BaseGenerator,
//! FileMapGenerator,PlanetGenerator,BlankPlanetGenerator}.java`. The
//! `WorldGenerator` trait is the pluggable surface used by save loading and the
//! headless `world gen` command.

pub mod astar;
pub mod base;
pub mod basic;
pub mod file_map;
pub mod planet;
pub mod simplex;

use crate::content::ContentRegistry;
use crate::game::rules::Rules;
use crate::world::WorldParams;
use crate::world::tiles::Tiles;

pub use base::{BaseGenerator, BaseRegistryView, SchematicHooks};
pub use basic::{BasicGenerator, Draw, GenNoise, SimplexNoise};
pub use file_map::FileMapGenerator;
pub use planet::{BlankPlanetGenerator, HexMesher, PlanetGenerator, SectorRect, SectorView};
pub use simplex::SimplexGenerator;

/// A world generator (`WorldGenerator`).
///
/// Deviation: `content` is passed explicitly instead of a `Vars.content`
/// singleton (plan 06 §2.3.2).
pub trait WorldGenerator {
    /// Stable generator tag (`simplex`, `planet`, `file`, ...).
    fn name(&self) -> &'static str;

    /// Fills `tiles` (`WorldGenerator.generate`).
    fn generate(&mut self, tiles: &mut Tiles, params: &WorldParams, content: &ContentRegistry);

    /// Specialized post-pass; must not modify tiles (`WorldGenerator.postGenerate`).
    fn post_generate(&mut self, _tiles: &mut Tiles, _content: &ContentRegistry) {}

    /// Writes generated rules (`state.rules`) the generator mutates.
    ///
    /// Upstream planet generators assign `state.rules.*` at the end of
    /// `generate` (`waves`, `waveSpacing`, `attackMode`, `enemyCoreBuildRadius`,
    /// `winWave`, `spawns`). The Rust port threads an explicit [`Rules`] so the
    /// caller (sector load / `world gen`) owns the runtime state. Default: no-op.
    fn generate_rules(
        &mut self,
        _rules: &mut Rules,
        _params: &WorldParams,
        _content: &ContentRegistry,
    ) {
    }
}
