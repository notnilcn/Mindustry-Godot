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
use crate::maps::generators::planet::{FlatSectorView, SectorRect, make_rect};
use crate::maps::generators::{BlankPlanetGenerator, WorldGenerator};
use crate::render::g3d::grid::PlanetGrid;
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

/// Campaign state the planet generator reads at load time
/// (`state.rules.attackMode`/`sector.hasEnemyBase()`); H05 T2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectorGenExtras {
    /// Sector difficulty (`Sector.threat`).
    pub threat: f32,
    /// Whether the sector generates an enemy base.
    pub has_enemy_base: bool,
    /// Whether the player owns a base here.
    pub has_base: bool,
}

impl Default for SectorGenExtras {
    fn default() -> Self {
        Self {
            threat: 0.0,
            has_enemy_base: false,
            has_base: false,
        }
    }
}

/// A generated sector world plus the rules its generator assigned
/// (`WorldGenerator.generate_rules`); T4-T6 run the generator's structure
/// passes on the live sim through `generator`/`rect`.
pub struct GeneratedSector {
    /// Generated tile grid.
    pub grid: WorldGrid,
    /// Rules the generator wrote (`waves`, `winWave`, `spawns`, …).
    pub rules: Rules,
    /// The generator that built the grid (structure passes run later).
    pub generator: Box<dyn WorldGenerator>,
    /// The projected sector plane (`Sector.makeRect`).
    pub rect: SectorRect,
    /// Tile corner count (`Sector.tile.corners.length`; hexagon = 6).
    pub sides: usize,
}

/// Generates one campaign sector's tile grid (`Control.playNewSector` world
/// half + `PlanetGenerator.generate`). Returns `None` for an unknown planet.
///
/// `PlanetGenerator.generate`: `rand.setSeed(sec.id + params.seedOffset +
/// baseSeed)` with `World.loadSector`'s default `seedOffset = 0`; the tile
/// position projects through the sector's own rect and the size comes from
/// `getSectorSize(sector)` (`PlanetGenerator.java:173-196`).
pub fn generate_sector(
    planet: &str,
    sector: u16,
    content: &ContentRegistry,
    extras: SectorGenExtras,
) -> Option<GeneratedSector> {
    let def = content.planet_by_name(planet)?;
    let planet_grid = PlanetGrid::create(def.sector_tiles as usize);
    let rect = make_rect(&planet_grid, sector as usize, def.radius);
    let mut generator = generator_for(planet, 0)?;
    let size = generator
        .sector_size_scl()
        .map(|size_scl| {
            let res = (rect.radius * size_scl) as i32;
            if res % 2 == 0 { res } else { res + 1 }
        })
        .unwrap_or(128);
    let tile_v = planet_grid
        .tiles
        .get(sector as usize)
        .map(|tile| tile.v)
        .unwrap_or([0.0, 0.0, 0.0]);
    let sides = planet_grid
        .tiles
        .get(sector as usize)
        .map(|tile| tile.corners.len())
        .unwrap_or(0);
    generator.set_sector(FlatSectorView {
        id: sector as u32,
        radius: rect.radius,
        threat: extras.threat,
        tile_v,
        has_enemy_base: extras.has_enemy_base,
        has_base: extras.has_base,
        rect: Some(rect),
    });
    let params = WorldParams {
        seed_offset: 0,
        width: size,
        height: size,
        ..WorldParams::default()
    };
    let mut grid = WorldGrid::new(size, size);
    grid.begin_map_load();
    generator.generate(&mut grid.tiles, &params, content);
    let mut rules = Rules::default();
    generator.generate_rules(&mut rules, &params, content);
    grid.end_map_load(content);
    Some(GeneratedSector {
        grid,
        rules,
        generator,
        rect,
        sides,
    })
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
        let generated =
            generate_sector("serpulo", 0, &content, SectorGenExtras::default()).expect("serpulo");
        // T2: the grid is the sector's own `getSectorSize` (even).
        assert_eq!(generated.grid.tiles.width % 2, 0);
        assert_eq!(generated.grid.tiles.width, generated.grid.tiles.height);
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
        assert!(
            generate_sector("does-not-exist", 0, &content, SectorGenExtras::default()).is_none()
        );
    }

    #[test]
    fn sectors_differ_by_id() {
        // T2 acceptance: two numbered sectors no longer share one 128x128 map.
        let content = content();
        let a = generate_sector("serpulo", 1, &content, SectorGenExtras::default()).expect("a");
        let b = generate_sector("serpulo", 2, &content, SectorGenExtras::default()).expect("b");
        let floors_a: Vec<BlockId> = a.grid.tiles.iter().map(|tile| tile.floor).collect();
        let floors_b: Vec<BlockId> = b.grid.tiles.iter().map(|tile| tile.floor).collect();
        assert_ne!(floors_a, floors_b, "sectors 1 and 2 differ");
    }

    #[test]
    fn sector_size_is_even_and_from_rect() {
        // T2: size = `getSectorSize(rect.radius)` (`PlanetGenerator.java:173-180`).
        let content = content();
        let def = content.planet_by_name("serpulo").expect("serpulo");
        let planet_grid = PlanetGrid::create(def.sector_tiles as usize);
        let rect = make_rect(&planet_grid, 1, def.radius);
        let radius_size = (rect.radius * 3200.0) as i32;
        let expected = if radius_size % 2 == 0 {
            radius_size
        } else {
            radius_size + 1
        };
        let generated =
            generate_sector("serpulo", 1, &content, SectorGenExtras::default()).expect("serpulo");
        assert_eq!(generated.grid.tiles.width % 2, 0);
        assert_eq!(generated.grid.tiles.width, expected);
        println!("serpulo sector 1 size = {expected}");
    }
}
