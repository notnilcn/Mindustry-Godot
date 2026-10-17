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
pub use serpulo::{SectorGlow, SerpuloPlanetGenerator};
pub use tantros::TantrosPlanetGenerator;

use crate::content::BlockId;
use crate::content::ContentRegistry;
use crate::game::rules::Rules;
use crate::maps::generators::planet::{FlatSectorView, SectorRect, make_rect};
use crate::maps::generators::{BlankPlanetGenerator, WorldGenerator};
use crate::render::g3d::grid::PlanetGrid;
use crate::world::{WorldGrid, WorldParams};

/// `Block.mapColor` + `1 - albedo` alpha per block id (the hex-mesh
/// `getColor` table). Blocks without a map colour read as Java's default black
/// (`Block.java:304` `new Color(0, 0, 0, 1)`), and the default albedo is `0`.
#[derive(Debug, Clone, Default)]
pub struct PlanetColorTable {
    colors: Vec<([f32; 3], f32)>,
}

impl PlanetColorTable {
    /// Builds the table from the block registry plus the `sprites/block_colors
    /// .png` row (`ContentLoader.loadColors.java:140-152`: a non-0/non-255
    /// sprite-centre pixel overwrites `Block.mapColor` and forces alpha `1`).
    pub fn load(content: &ContentRegistry, sprite_colors: &[u32]) -> Self {
        let mut colors = vec![([0.0, 0.0, 0.0], 0.0f32); content.blocks().len()];
        for block in content.blocks() {
            let mut rgb = block
                .map_color
                .map(|color| [color.r, color.g, color.b])
                .unwrap_or([0.0, 0.0, 0.0]);
            if let Some(&packed) = sprite_colors.get(block.id.raw() as usize)
                && packed != 0
                && packed != 255
            {
                rgb = [
                    f32::from(((packed >> 24) & 0xff) as u8) / 255.0,
                    f32::from(((packed >> 16) & 0xff) as u8) / 255.0,
                    f32::from(((packed >> 8) & 0xff) as u8) / 255.0,
                ];
            }
            if let Some(entry) = colors.get_mut(block.id.raw() as usize) {
                *entry = (rgb, block.albedo);
            }
        }
        Self { colors }
    }

    /// `out.set(block.mapColor).a(1f - block.albedo)` for a block id.
    pub fn rgba(&self, block: BlockId) -> [f32; 4] {
        let (rgb, albedo) = self
            .colors
            .get(block.raw() as usize)
            .copied()
            .unwrap_or(([0.0, 0.0, 0.0], 0.0));
        [rgb[0], rgb[1], rgb[2], 1.0 - albedo]
    }
}

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

/// `PlanetGenerator.getSectorSize(sector)` for one cell — the editor's
/// `[ NxN ]` size label (`Sector.getSize`, `Sector.java:203-205`). `None` for
/// an unknown planet or one without a generator.
pub fn sector_size(planet: &str, sector: u16, content: &ContentRegistry) -> Option<i32> {
    let def = content.planet_by_name(planet)?;
    let planet_grid = PlanetGrid::create(def.sector_tiles as usize);
    let rect = make_rect(&planet_grid, sector as usize, def.radius);
    let generator = generator_for(planet, 0)?;
    Some(sector_size_of(generator.as_ref(), rect.radius))
}

/// `PlanetGenerator.getSectorSize` math (`generate_sector` shares it).
fn sector_size_of(generator: &dyn WorldGenerator, radius: f32) -> i32 {
    generator
        .sector_size_scl()
        .map(|size_scl| {
            let res = (radius * size_scl) as i32;
            if res % 2 == 0 { res } else { res + 1 }
        })
        .unwrap_or(128)
}

/// Generates one campaign sector's tile grid (`Control.playNewSector` world
/// half + `PlanetGenerator.generate`). Returns `None` for an unknown planet.
///
/// `World.loadSector`'s default `seedOffset = 0`; callers that take a seed use
/// [`generate_sector_with_seed`] (`SectorGenerateDialog`'s seed field).
pub fn generate_sector(
    planet: &str,
    sector: u16,
    content: &ContentRegistry,
    extras: SectorGenExtras,
) -> Option<GeneratedSector> {
    generate_sector_with_seed(planet, sector, content, extras, 0)
}

/// [`generate_sector`] with `WorldParams.seedOffset` set (`SectorGenerateDialog`
/// `params.seedOffset = seed`, `:98-99`).
///
/// `PlanetGenerator.generate`: `rand.setSeed(sec.id + params.seedOffset +
/// baseSeed)`; the tile position projects through the sector's own rect and the
/// size comes from `getSectorSize(sector)` (`PlanetGenerator.java:173-196`).
pub fn generate_sector_with_seed(
    planet: &str,
    sector: u16,
    content: &ContentRegistry,
    extras: SectorGenExtras,
    seed_offset: u64,
) -> Option<GeneratedSector> {
    let def = content.planet_by_name(planet)?;
    let planet_grid = PlanetGrid::create(def.sector_tiles as usize);
    let rect = make_rect(&planet_grid, sector as usize, def.radius);
    let mut generator = generator_for(planet, 0)?;
    let size = sector_size_of(generator.as_ref(), rect.radius);
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
        seed_offset,
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
    fn sector_size_matches_generation_and_seed_shifts() {
        // `SectorGenerateDialog`'s `[ NxN ]` label: `sector_size` is the grid
        // width the generator produces (`Sector.getSize`).
        let content = content();
        for planet in ["serpulo", "erekir"] {
            let generated = generate_sector(planet, 0, &content, SectorGenExtras::default())
                .expect("generated");
            assert_eq!(
                sector_size(planet, 0, &content),
                Some(generated.grid.tiles.width)
            );
        }
        assert_eq!(sector_size("does-not-exist", 0, &content), None);
        // `WorldParams.seedOffset = seed`: a different seed changes the terrain.
        let a = generate_sector_with_seed("serpulo", 1, &content, SectorGenExtras::default(), 0)
            .expect("a");
        let b = generate_sector_with_seed("serpulo", 1, &content, SectorGenExtras::default(), 7)
            .expect("b");
        let floors_a: Vec<BlockId> = a.grid.tiles.iter().map(|tile| tile.floor).collect();
        let floors_b: Vec<BlockId> = b.grid.tiles.iter().map(|tile| tile.floor).collect();
        assert_ne!(floors_a, floors_b, "seed 7 differs from seed 0");
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

    #[test]
    fn generated_sector_reports_spawn() {
        // H05 T4: the generator exposes the spawn `placeLaunchLoadout` uses
        // (`SerpuloPlanetGenerator.spawn`).
        let content = content();
        let generated =
            generate_sector("serpulo", 1, &content, SectorGenExtras::default()).expect("serpulo");
        let Some((x, y)) = generated.generator.player_spawn() else {
            panic!("serpulo sector 1 has a spawn");
        };
        let tile = generated.grid.tile(x, y);
        let solid = content
            .block(tile.block)
            .map(|def| def.solid)
            .unwrap_or(false);
        assert!(!solid, "spawn tile ({x},{y}) is not solid");
    }

    #[test]
    fn place_structures_places_shard() {
        // H05 T4: `Schematics.placeLaunchLoadout` places the shard at the
        // spawn and stocks it with the launch resources.
        use crate::game::schematics::Schematics;
        use crate::maps::generators::planet::{LaunchLoadout, StructureCtx};
        use crate::world::blocks::storage::CoreBuild;
        use crate::world::modules::ItemModule;

        let content = content();
        let generated =
            generate_sector("serpulo", 1, &content, SectorGenExtras::default()).expect("serpulo");
        let Some((spawn_x, spawn_y)) = generated.generator.player_spawn() else {
            panic!("serpulo spawn");
        };
        let GeneratedSector {
            grid,
            rules,
            mut generator,
            ..
        } = generated;
        let (width, height) = (grid.tiles.width, grid.tiles.height);
        let mut sim = crate::sim::Sim::new(1, width, height, BlockId::AIR, BlockId::AIR);
        sim.grid = grid;
        sim.install_block_runtime().expect("live runtime");

        let mut schematics = Schematics::new();
        schematics.load_loadouts(&content);
        let shard = content.block_id("core-shard").expect("core-shard");
        let index = schematics
            .get_default_loadout(shard)
            .expect("basicShard registered");
        let schematic = schematics.all[index].clone();
        let copper = content.item_id("copper").expect("copper");
        let loadout = LaunchLoadout {
            schematic,
            stacks: vec![(copper, 100)],
        };
        let mut rules = rules;
        let mut ctx = StructureCtx {
            sim: &mut sim,
            content: &content,
            bases: None,
            loadout: Some(&loadout),
            rules: &mut rules,
            player_team: 1,
            wave_team: 2,
        };
        generator.place_structures(&mut ctx);

        let entity = sim
            .grid
            .tile(spawn_x, spawn_y)
            .build
            .expect("core placed at the spawn");
        let building = sim
            .ecs
            .0
            .get::<crate::ecs::BuildingComp>(entity)
            .expect("building");
        assert_eq!(building.block, shard, "core-shard at the spawn");
        assert!(sim.ecs.0.get::<CoreBuild>(entity).is_some(), "CoreBuild");
        assert_eq!(
            sim.ecs
                .0
                .get::<ItemModule>(entity)
                .expect("items")
                .get(copper),
            100,
            "launch resources stocked"
        );
    }

    /// L06 T6.2: the Erekir structures pass applies the generator's rules
    /// (`ErekirPlanetGenerator.java:423-433`): `env = defaultEnv`,
    /// `placeRangeCheck = true`, `waves = false`, `hideSpawns = false`.
    #[test]
    fn erekir_structures_apply_generator_rules() {
        use crate::maps::generators::planet::StructureCtx;
        let content = content();
        let def = content.planet_by_name("erekir").expect("erekir");
        let id = (0..def.sectors.len() as u16)
            .find(|id| def.sectors[*id as usize].preset.is_none())
            .expect("a numbered erekir sector");
        let generated =
            generate_sector("erekir", id, &content, SectorGenExtras::default()).expect("erekir");
        let GeneratedSector {
            grid,
            rules,
            mut generator,
            ..
        } = generated;
        let (width, height) = (grid.tiles.width, grid.tiles.height);
        let mut sim = crate::sim::Sim::new(1, width, height, BlockId::AIR, BlockId::AIR);
        sim.grid = grid;
        sim.install_block_runtime().expect("live runtime");
        let mut rules = rules;
        rules.waves = true;
        rules.hide_spawns = true;
        rules.place_range_check = false;
        let mut ctx = StructureCtx {
            sim: &mut sim,
            content: &content,
            bases: None,
            loadout: None,
            rules: &mut rules,
            player_team: 1,
            wave_team: 2,
        };
        generator.place_structures(&mut ctx);
        assert!(!rules.waves, "waves = false");
        assert!(!rules.hide_spawns, "hideSpawns = false");
        assert!(rules.place_range_check, "placeRangeCheck = true");
        assert_eq!(
            rules.env,
            crate::game::planet::env_mask(&def.default_env),
            "env = defaultEnv"
        );
    }

    #[test]
    fn ruins_placed_when_roll_positive() {
        // H05 T6: on a numbered sector whose ruin roll is > 0, the structures
        // pass scatters team-0 derelict parts (none within 18 tiles of the
        // spawn). Uses the real `assets/baseparts` catalog and panics (never
        // skips) when it is missing or does not decode.
        use crate::maps::generators::planet::StructureCtx;

        let content = content();
        let bases = crate::ai::base_registry::real_catalog_for_tests(&content);

        let def = content.planet_by_name("serpulo").expect("serpulo");
        for id in 1..30u16 {
            if def
                .sectors
                .get(id as usize)
                .is_none_or(|cell| cell.preset.is_some())
            {
                continue;
            }
            let generated = generate_sector("serpulo", id, &content, SectorGenExtras::default())
                .expect("serpulo");
            let Some((spawn_x, spawn_y)) = generated.generator.player_spawn() else {
                continue;
            };
            let GeneratedSector {
                grid,
                rules,
                mut generator,
                ..
            } = generated;
            let (width, height) = (grid.tiles.width, grid.tiles.height);
            let mut sim = crate::sim::Sim::new(1, width, height, BlockId::AIR, BlockId::AIR);
            sim.grid = grid;
            sim.install_block_runtime().expect("live runtime");
            let mut rules = rules;
            let mut ctx = StructureCtx {
                sim: &mut sim,
                content: &content,
                bases: Some(&bases),
                loadout: None,
                rules: &mut rules,
                player_team: 1,
                wave_team: 2,
            };
            generator.place_structures(&mut ctx);

            let mut ruin_tiles: Vec<(i32, i32)> = Vec::new();
            for (_, _, comp) in sim.ecs.entities_by_seq() {
                if comp.team.0 == 0 {
                    ruin_tiles.push((i32::from(comp.pos.x()), i32::from(comp.pos.y())));
                }
            }
            if ruin_tiles.is_empty() {
                continue;
            }
            for &(x, y) in &ruin_tiles {
                let dx = x - spawn_x;
                let dy = y - spawn_y;
                assert!(
                    dx * dx + dy * dy >= 18 * 18,
                    "sector {id}: team-0 building at ({x},{y}) too close to the spawn"
                );
            }
            println!(
                "sector {id}: {} derelict building(s) from the ruins pass",
                ruin_tiles.len()
            );
            return;
        }
        panic!("no sector in 1..30 rolled ruins; the ruins pass was never exercised");
    }
}
