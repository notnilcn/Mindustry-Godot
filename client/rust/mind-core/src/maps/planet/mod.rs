// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Vanilla planet generators (plan 06 §3.11).
//!
//! Ported from `maps/planet/{SerpuloPlanetGenerator,ErekirPlanetGenerator,
//! TantrosPlanetGenerator,AsteroidGenerator}.java`. Structural parity only
//! (OD6-B): goldens are Rust-recorded. Serpulo/Erekir/Asteroid depend on plan
//! 11/12 data (bases, schematics, rules, attributes) and land incrementally.

pub mod asteroid;
pub mod erekir;
pub mod serpulo;
pub mod tantros;

pub use asteroid::AsteroidGenerator;
pub use erekir::ErekirPlanetGenerator;
pub use serpulo::SerpuloPlanetGenerator;
pub use tantros::TantrosPlanetGenerator;

use crate::content::ContentRegistry;
use crate::game::rules::Rules;
use crate::maps::generators::{BlankPlanetGenerator, WorldGenerator};
use crate::world::{WorldGrid, WorldParams};

/// Builds the vanilla planet generator for a planet content name
/// (`Planets.java` generator assignment). `seed` feeds the asteroid generator;
/// the hex planet generators carry their own base seed.
pub fn generator_for(planet: &str, seed: u64) -> Option<Box<dyn WorldGenerator>> {
    let generator: Box<dyn WorldGenerator> = match planet {
        "blank" => Box::new(BlankPlanetGenerator::new(0)),
        "tantros" => Box::new(TantrosPlanetGenerator::new()),
        "asteroid" => Box::new(AsteroidGenerator::new(seed as i32)),
        "erekir" => Box::new(ErekirPlanetGenerator::new()),
        "serpulo" => Box::new(SerpuloPlanetGenerator::new()),
        _ => return None,
    };
    Some(generator)
}

/// A generated sector world plus the rules its generator assigned
/// (`WorldGenerator.generate_rules`).
pub struct GeneratedSector {
    /// Generated tile grid.
    pub grid: WorldGrid,
    /// Rules the generator wrote (`waves`, `winWave`, `spawns`, …).
    pub rules: Rules,
}

/// Generates one campaign sector's tile grid (`Control.playNewSector` world
/// half + `PlanetGenerator.generate`). Returns `None` for an unknown planet.
pub fn generate_sector(
    planet: &str,
    sector: u16,
    seed: u64,
    width: i32,
    height: i32,
    content: &ContentRegistry,
) -> Option<GeneratedSector> {
    let mut generator = generator_for(planet, seed)?;
    let params = WorldParams {
        seed_offset: seed,
        width,
        height,
        ..WorldParams::default()
    };
    let mut grid = WorldGrid::new(width, height);
    grid.begin_map_load();
    generator.generate(&mut grid.tiles, &params, content);
    let mut rules = Rules::default();
    generator.generate_rules(&mut rules, &params, content);
    grid.end_map_load(content);
    // Sector id selects the planet-grid cell upstream; the vanilla generator
    // port does not yet consume it, so the seed fully drives the layout.
    let _ = sector;
    Some(GeneratedSector { grid, rules })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{BlockId, MemoryBundle, MemoryUnlockStore, create_base_content};

    fn content() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content boot")
    }

    #[test]
    fn generate_sector_produces_terrain() {
        let content = content();
        let generated = generate_sector("serpulo", 0, 7, 64, 64, &content).expect("serpulo");
        assert_eq!(generated.grid.tiles.width, 64);
        assert_eq!(generated.grid.tiles.height, 64);
        let floors = generated
            .grid
            .tiles
            .iter()
            .filter(|tile| tile.floor != BlockId::AIR)
            .count();
        assert!(floors > 0, "generated sector has non-air floors");
    }

    #[test]
    fn generate_sector_unknown_planet_is_none() {
        let content = content();
        assert!(generate_sector("does-not-exist", 0, 1, 8, 8, &content).is_none());
    }
}
