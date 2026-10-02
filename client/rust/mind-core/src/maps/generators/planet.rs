// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PlanetGenerator` + `HexMesher` (plan 06 §3.10).
//!
//! Ported from `maps/generators/PlanetGenerator.java` and
//! `graphics/g3d/HexMesher.java`. The sector projection (`Sector.SectorRect`)
//! is reproduced exactly: `center + right*(2x-1) + top*(2y-1)`. Plan 12 supplies
//! real `Sector`/`Planet` views; [`SectorView`] is the seam.

use crate::content::{BlockId, ContentRegistry};
use crate::world::WorldParams;
use crate::world::cached::TileGen;
use crate::world::tiles::Tiles;

use super::WorldGenerator;
use super::basic::{BasicGenerator, GenNoise};

/// Hex mesh color/height data consumed by plan 16's g3d renderer.
///
/// Ported from `graphics/g3d/HexMesher.java`; position is normalized `[0,1]` on
/// the sector plane.
pub trait HexMesher {
    /// Mesh height at a position.
    fn get_height(&self, _position: [f32; 3]) -> f32 {
        0.0
    }

    /// Mesh color at a position (rgba).
    fn get_color(&self, _position: [f32; 3]) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    /// Emissive color at a position (rgba).
    fn get_emissive_color(&self, _position: [f32; 3]) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    /// Whether the mesh emits light.
    fn is_emissive(&self) -> bool {
        false
    }

    /// Whether a position is skipped.
    fn skip(&self, _position: [f32; 3]) -> bool {
        false
    }
}

/// The projected sector plane (`Sector.SectorRect`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectorRect {
    /// Center of the plane on the planet sphere.
    pub center: [f32; 3],
    /// "Up" tangent.
    pub top: [f32; 3],
    /// "Right" tangent.
    pub right: [f32; 3],
    /// Plane radius.
    pub radius: f32,
}

impl Default for SectorRect {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0, 0.0],
            top: [0.0, 1.0, 0.0],
            right: [1.0, 0.0, 0.0],
            radius: 1.0,
        }
    }
}

impl SectorRect {
    /// `SectorRect.project(x, y)`: normalized `[0,1]` coordinates → 3D.
    pub fn project(&self, x: f32, y: f32) -> [f32; 3] {
        let nx = (x - 0.5) * 2.0;
        let ny = (y - 0.5) * 2.0;
        [
            self.center[0] + self.right[0] * nx + self.top[0] * ny,
            self.center[1] + self.right[1] * nx + self.top[1] * ny,
            self.center[2] + self.right[2] * nx + self.top[2] * ny,
        ]
    }
}

/// A campaign sector view (plan 12 `Sector`); defaults give a flat plane.
///
/// The vanilla planet generators read `id`/`threat`/`tile_v`/enemy-base flags;
/// plan 12 supplies a real implementation, while [`FlatSectorView`] covers the
/// headless harness and tests.
pub trait SectorView {
    /// Sector id (`Sector.id`).
    fn id(&self) -> u32;

    /// The projected sector plane (`Sector.rect`).
    fn rect(&self) -> SectorRect;

    /// Sector difficulty 0..1 (`Sector.threat`).
    fn threat(&self) -> f32 {
        0.0
    }

    /// Sector center on the planet sphere (`Sector.tile.v`).
    fn tile_v(&self) -> [f32; 3] {
        [0.0, 0.0, 0.0]
    }

    /// Whether this sector generates an enemy base (`Sector.hasEnemyBase`).
    fn has_enemy_base(&self) -> bool {
        false
    }

    /// Whether the player owns a base here (`Sector.hasBase`).
    fn has_base(&self) -> bool {
        false
    }

    /// Whether the sector is under attack (`Sector.isAttacked`).
    fn is_attacked(&self) -> bool {
        false
    }

    /// Whether the sector is captured (`Sector.isCaptured`).
    fn is_captured(&self) -> bool {
        false
    }

    /// Whether the sector allows a launch loadout (`Sector.allowLaunchLoadout`).
    fn allow_launch_loadout(&self) -> bool {
        false
    }

    /// Owning planet content name (`Sector.planet.name`).
    fn planet_name(&self) -> Option<&str> {
        None
    }
}

/// A flat identity [`SectorView`] (blank/test sector).
#[derive(Debug, Clone, Copy)]
pub struct FlatSectorView {
    /// Sector id.
    pub id: u32,
    /// Optional radius override.
    pub radius: f32,
    /// Sector difficulty (`Sector.threat`).
    pub threat: f32,
    /// Sector center on the planet sphere (`Sector.tile.v`).
    pub tile_v: [f32; 3],
    /// Whether this sector generates an enemy base.
    pub has_enemy_base: bool,
    /// Whether the player owns a base here.
    pub has_base: bool,
}

impl Default for FlatSectorView {
    fn default() -> Self {
        Self {
            id: 0,
            radius: 1.0,
            threat: 0.0,
            tile_v: [0.0, 0.0, 0.0],
            has_enemy_base: false,
            has_base: false,
        }
    }
}

impl SectorView for FlatSectorView {
    fn id(&self) -> u32 {
        self.id
    }

    fn rect(&self) -> SectorRect {
        SectorRect {
            radius: self.radius,
            ..SectorRect::default()
        }
    }

    fn threat(&self) -> f32 {
        self.threat
    }

    fn tile_v(&self) -> [f32; 3] {
        self.tile_v
    }

    fn has_enemy_base(&self) -> bool {
        self.has_enemy_base
    }

    fn has_base(&self) -> bool {
        self.has_base
    }
}

/// Planet-projected simplex noise (`PlanetGenerator.noise`).
#[derive(Debug, Clone, Copy)]
struct PlanetNoise {
    rect: SectorRect,
}

impl GenNoise for PlanetNoise {
    fn noise(&self, x: f32, y: f32, octaves: f64, falloff: f64, scl: f64, mag: f64) -> f32 {
        let v = self.rect.project(x, y);
        crate::math::noise::noise3d(
            0,
            octaves as i32,
            falloff,
            1.0 / scl,
            v[0] as f64,
            v[1] as f64,
            v[2] as f64,
        ) * mag as f32
    }
}

/// Builds a per-run noise source from the resolved seed and sector plane.
///
/// The default (`None`) mirrors upstream `PlanetGenerator.noise` (seed `0`).
/// `SerpuloPlanetGenerator` overrides `noise` with the planet seed and a `5x`
/// projection, so it installs a factory here.
pub type NoiseFactory = fn(i32, SectorRect) -> Box<dyn GenNoise>;

/// The planet generation base (`PlanetGenerator`).
pub struct PlanetGenerator {
    /// Shared helper library.
    pub base: BasicGenerator,
    /// Planet base seed (`baseSeed`).
    pub base_seed: i32,
    /// Current run seed (`seed`).
    pub seed: i32,
    /// Current sector plane.
    pub sector: Option<SectorRect>,
    /// Sector-rect scale (`getSizeScl`).
    pub size_scl: f32,
    /// Optional subclass noise-source factory (Serpulo's `noise` override).
    pub noise_factory: Option<NoiseFactory>,
}

impl std::fmt::Debug for PlanetGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanetGenerator")
            .field("base_seed", &self.base_seed)
            .field("seed", &self.seed)
            .finish()
    }
}

impl PlanetGenerator {
    /// Creates a planet generator with a base seed.
    pub fn new(base_seed: i32) -> Self {
        Self {
            base: BasicGenerator::new(Box::new(PlanetNoise {
                rect: SectorRect::default(),
            })),
            base_seed,
            seed: 0,
            sector: None,
            size_scl: 3200.0,
            noise_factory: None,
        }
    }

    /// Installs a subclass noise-source factory (Serpulo's `noise` override).
    pub fn set_noise_factory(&mut self, factory: NoiseFactory) {
        self.noise_factory = Some(factory);
    }

    /// `PlanetGenerator.getSizeScl`.
    pub fn get_size_scl(&self) -> f32 {
        self.size_scl
    }

    /// `PlanetGenerator.getSectorSize` (always even).
    pub fn get_sector_size(&self, sector_radius: f32) -> i32 {
        let res = (sector_radius * self.get_size_scl()) as i32;
        if res % 2 == 0 { res } else { res + 1 }
    }

    /// The seed formula (`seed = seedOffset + baseSeed`).
    pub fn seed_formula(&self, seed_offset: u64) -> i32 {
        seed_offset as i32 + self.base_seed
    }

    /// `PlanetGenerator.noise(x, y, octaves, falloff, scl, mag)`.
    pub fn noise(&self, x: f32, y: f32, octaves: f64, falloff: f64, scl: f64, mag: f64) -> f32 {
        self.base.noise_oct(x, y, octaves, falloff, scl, mag)
    }

    /// Seeds the generator and installs the per-run noise source without
    /// running the tile loop (Asteroid/Serpulo subclasses that build the grid
    /// with `pass` rather than `genTile`).
    pub fn prepare(&mut self, tiles: &Tiles, params: &WorldParams, sector: &dyn SectorView) {
        let rect = sector.rect();
        self.seed = self.seed_formula(params.seed_offset);
        self.sector = Some(rect);
        self.base.width = tiles.width;
        self.base.height = tiles.height;
        self.base
            .set_seed(sector.id() as u64 + params.seed_offset + self.base_seed as u64);
        let source = match self.noise_factory {
            Some(factory) => factory(self.seed, rect),
            None => Box::new(PlanetNoise { rect }),
        };
        self.base.set_noise_source(source);
    }

    /// Runs the per-tile generation loop, calling `gen_tile` for each tile.
    ///
    /// This is the Rust equivalent of `PlanetGenerator.generate(Tiles, sector,
    /// params)`: it seeds the generator, projects normalized coordinates and
    /// writes the resulting [`TileGen`] into the grid. `gen_tile` is the
    /// subclass body (`SerpuloPlanetGenerator.genTile` etc.).
    pub fn run<F>(
        &mut self,
        tiles: &mut Tiles,
        params: &WorldParams,
        sector: &dyn SectorView,
        mut gen_tile: F,
    ) where
        F: FnMut(&mut PlanetGenerator, [f32; 3], &mut TileGen),
    {
        self.prepare(tiles, params, sector);
        let rect = sector.rect();

        let width = tiles.width;
        let height = tiles.height;
        let mut brush = TileGen::default();
        for y in 0..height {
            for x in 0..width {
                brush.reset();
                let position = rect.project(x as f32 / width as f32, y as f32 / height as f32);
                gen_tile(self, position, &mut brush);
                let tile = tiles.get_mut(x, y);
                tile.floor = brush.floor;
                tile.overlay = brush.overlay;
                tile.block = brush.block;
            }
        }
    }

    /// `PlanetGenerator.addWeather` is a plan-12 hook (needs `Rules`/`Weathers`).
    pub fn add_weather(&self, _content: &ContentRegistry) {
        log::debug!("PlanetGenerator.addWeather is a plan-12 hook; no-op in mind-core");
    }
}

/// Blank planet (`BlankPlanetGenerator`): fills tiles and delegates generation.
#[derive(Debug, Default)]
pub struct BlankPlanetGenerator {
    /// Planet base seed.
    pub base_seed: i32,
}

impl BlankPlanetGenerator {
    /// Creates a blank generator.
    pub fn new(base_seed: i32) -> Self {
        Self { base_seed }
    }
}

impl WorldGenerator for BlankPlanetGenerator {
    fn name(&self) -> &'static str {
        "planet"
    }

    fn generate(&mut self, tiles: &mut Tiles, _params: &WorldParams, content: &ContentRegistry) {
        let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
        let wall = content.block_id("stone-wall").unwrap_or(BlockId::AIR);
        for index in 0..tiles.len() {
            let tile = tiles.geti_mut(index);
            tile.floor = stone;
            tile.block = wall;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_positions() {
        let rect = SectorRect::default();
        // Corners map to the plane corners.
        assert_eq!(rect.project(0.0, 0.0), [-1.0, -1.0, 0.0]);
        assert_eq!(rect.project(1.0, 1.0), [1.0, 1.0, 0.0]);
        assert_eq!(rect.project(0.5, 0.5), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn sector_size_even() {
        let generator = PlanetGenerator::new(0);
        // radius 0.5 * 3200 = 1600 (even).
        assert_eq!(generator.get_sector_size(0.5), 1600);
        // 0.5004 * 3200 = 1601.28 -> 1601 (odd) -> 1602.
        assert_eq!(generator.get_sector_size(0.5004), 1602);
    }

    #[test]
    fn seed_formula_matches() {
        let generator = PlanetGenerator::new(42);
        assert_eq!(generator.seed_formula(11), 53);
    }

    #[test]
    fn blank_fills() {
        let content = crate::content::test_support::test_registry();
        let stone = content.block_id("stone").unwrap();
        let wall = content.block_id("stone-wall").unwrap();
        let mut tiles = Tiles::new(4, 4);
        let mut generator = BlankPlanetGenerator::new(0);
        generator.generate(&mut tiles, &WorldParams::default(), &content);
        assert!(
            tiles
                .iter()
                .all(|tile| tile.floor == stone && tile.block == wall)
        );
    }
}
