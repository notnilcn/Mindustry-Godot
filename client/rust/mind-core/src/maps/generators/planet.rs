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
}

/// `PlanetGenerator.addWeather` (`PlanetGenerator.java:102-160`): terrain
/// weather for a freshly generated sector. Ties at equal counts break by block
/// id ascending (Java `ObjectIntMap` order is hash-based; deviation).
///
/// The `World.getDarkness` sector-circle term is omitted (needs Arc 2-D
/// `Noise.noise`, not ported; it only affects edge tiles of numbered sectors).
pub fn add_weather(
    grid: &crate::world::WorldGrid,
    content: &ContentRegistry,
    rules: &crate::io::json::rules::Rules,
) -> Vec<crate::io::json::rules::WeatherEntry> {
    use crate::world::darkness::{border_darkness, get_static_darkness};

    let water = content.liquid_id("water");
    let limit = rules.limit_map_area.then_some((
        rules.limit_x,
        rules.limit_y,
        rules.limit_width,
        rules.limit_height,
    ));
    let mut counts: std::collections::BTreeMap<BlockId, i32> = std::collections::BTreeMap::new();
    let mut has_water = false;
    for tile in grid.tiles.iter() {
        let x = i32::from(tile.x);
        let y = i32::from(tile.y);
        let darkness = border_darkness(grid, rules.border_darkness, limit, x, y)
            .max(get_static_darkness(grid, content, x, y));
        if darkness >= 3.0 {
            continue;
        }
        if let Some(floor) = content.block(tile.floor)
            && floor.liquid_drop == water
            && water.is_some()
        {
            has_water = true;
        }
        // `!tile.block().isStatic()`: only non-static tiles are counted.
        let static_block = content
            .block(tile.block)
            .is_some_and(crate::maps::filters::block_info::is_static);
        if static_block {
            continue;
        }
        *counts.entry(tile.floor).or_insert(0) += 1;
        if tile.overlay != BlockId::AIR {
            *counts.entry(tile.overlay).or_insert(0) += 1;
        }
    }
    let mut entries: Vec<(BlockId, i32)> = counts.into_iter().collect();
    // `entries.sort(e -> -e.value)`; ties resolve by id ascending here.
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    entries.retain(|(_, count)| *count >= 30);
    let name = |id: BlockId| content.block(id).map(|def| def.name.as_str()).unwrap_or("");
    let top = entries.first().map(|(id, _)| *id);
    let has_snow = top.is_some_and(|id| {
        let name = name(id);
        name.contains("ice") || name.contains("snow")
    });
    let has_rain = top.is_some_and(|id| !has_snow && has_water && !name(id).contains("sand"));
    let has_desert = top.is_some_and(|id| !has_snow && !has_rain && name(id) == "sand-floor");
    let has_spores = top.is_some_and(|id| {
        let name = name(id);
        name.contains("spore") || name.contains("moss") || name.contains("tainted")
    });

    let mut weather = Vec::new();
    let mut push = |name: &str| {
        if let Some(def) = content.weather_by_name(name) {
            weather.push(weather_entry(def));
        }
    };
    if has_snow {
        // `Weathers.snow` loads as `snowing` in the port registry.
        push("snowing");
    }
    if has_rain {
        push("rain");
        push("fog");
    }
    if has_desert {
        push("sandstorm");
    }
    if has_spores {
        push("sporestorm");
    }
    weather
}

/// `new WeatherEntry(weather)` (`Weather.java:275-287`): frequency/duration
/// windows from the weather's `duration`. Java randomizes the initial
/// `cooldown` from the global unseeded `Mathf.random(minFrequency,
/// maxFrequency)`; the port uses the deterministic lower bound (deviation).
pub fn weather_entry(
    def: &crate::content::registries::weathers::WeatherDef,
) -> crate::io::json::rules::WeatherEntry {
    let min_frequency = def.duration * 2.0;
    crate::io::json::rules::WeatherEntry {
        weather: def.name.clone(),
        min_frequency,
        max_frequency: def.duration * 6.0,
        min_duration: def.duration / 2.0,
        max_duration: def.duration * 1.5,
        cooldown: min_frequency,
        intensity: 1.0,
        always: false,
    }
}

/// `BlankPlanetGenerator.addWeather`/`TantrosPlanetGenerator.addWeather` are
/// empty overrides: only Serpulo and Erekir add terrain weather.
pub fn planet_adds_weather(planet_name: &str) -> bool {
    matches!(planet_name, "serpulo" | "erekir")
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

    fn weather_grid(content: &ContentRegistry, floor: &str) -> crate::world::WorldGrid {
        let mut grid = crate::world::WorldGrid::new(40, 40);
        let id = content.block_id(floor).expect("floor");
        for tile in grid.tiles.array_mut() {
            tile.floor = id;
        }
        grid
    }

    fn weather_names(entries: &[crate::io::json::rules::WeatherEntry]) -> Vec<String> {
        entries.iter().map(|entry| entry.weather.clone()).collect()
    }

    #[test]
    fn add_weather_snow_floor() {
        let content = crate::content::test_support::test_registry();
        let rules = crate::io::json::rules::Rules::default();
        let grid = weather_grid(&content, "snow");
        let entries = add_weather(&grid, &content, &rules);
        assert_eq!(weather_names(&entries), vec!["snowing".to_owned()]);
        let snow = &entries[0];
        assert_eq!(
            (snow.min_frequency, snow.max_frequency),
            (72000.0, 216000.0)
        );
        assert_eq!((snow.min_duration, snow.max_duration), (18000.0, 54000.0));
        assert_eq!(snow.intensity, 1.0);
        assert!(!snow.always);
    }

    #[test]
    fn add_weather_rain_and_fog() {
        let content = crate::content::test_support::test_registry();
        let rules = crate::io::json::rules::Rules::default();
        let mut grid = weather_grid(&content, "stone");
        let water = content.block_id("shallow-water").expect("shallow-water");
        grid.tiles.get_mut(20, 20).floor = water;
        let entries = add_weather(&grid, &content, &rules);
        assert_eq!(
            weather_names(&entries),
            vec!["rain".to_owned(), "fog".to_owned()]
        );
        assert_eq!(
            (entries[0].min_frequency, entries[0].max_frequency),
            (72000.0, 216000.0)
        );
        assert_eq!(
            (entries[1].min_frequency, entries[1].max_frequency),
            (108000.0, 324000.0)
        );
        assert_eq!(
            (entries[1].min_duration, entries[1].max_duration),
            (27000.0, 81000.0)
        );
    }

    #[test]
    fn add_weather_sand_desert() {
        let content = crate::content::test_support::test_registry();
        let rules = crate::io::json::rules::Rules::default();
        let grid = weather_grid(&content, "sand-floor");
        let entries = add_weather(&grid, &content, &rules);
        assert_eq!(weather_names(&entries), vec!["sandstorm".to_owned()]);
        assert_eq!(
            (entries[0].min_frequency, entries[0].max_frequency),
            (50400.0, 151200.0)
        );
        assert_eq!(
            (entries[0].min_duration, entries[0].max_duration),
            (12600.0, 37800.0)
        );
    }

    #[test]
    fn add_weather_darksand_no_rain() {
        let content = crate::content::test_support::test_registry();
        let rules = crate::io::json::rules::Rules::default();
        let mut grid = weather_grid(&content, "darksand");
        let water = content.block_id("shallow-water").expect("shallow-water");
        grid.tiles.get_mut(20, 20).floor = water;
        // "darksand" contains "sand" (no rain) but is not `sand-floor` (no
        // desert) -> no weather.
        let entries = add_weather(&grid, &content, &rules);
        assert!(entries.is_empty(), "{:?}", weather_names(&entries));
    }

    #[test]
    fn add_weather_moss_spores() {
        let content = crate::content::test_support::test_registry();
        let rules = crate::io::json::rules::Rules::default();
        let grid = weather_grid(&content, "moss");
        let entries = add_weather(&grid, &content, &rules);
        assert_eq!(weather_names(&entries), vec!["sporestorm".to_owned()]);
    }

    #[test]
    fn add_weather_skips_dark_and_static() {
        let content = crate::content::test_support::test_registry();
        let rules = crate::io::json::rules::Rules::default();
        let mut grid = weather_grid(&content, "snow");
        let stone = content.block_id("stone").expect("stone");
        let wall = content.block_id("stone-wall").expect("stone-wall");
        for x in 5..35 {
            for y in 5..35 {
                let tile = grid.tiles.get_mut(x, y);
                tile.floor = stone;
                tile.block = wall;
            }
        }
        // The static-wall interior does not count (900 stone tiles); the snow
        // border ring (700) wins.
        let entries = add_weather(&grid, &content, &rules);
        assert_eq!(weather_names(&entries), vec!["snowing".to_owned()]);
    }

    #[test]
    fn add_weather_needs_30() {
        let content = crate::content::test_support::test_registry();
        let rules = crate::io::json::rules::Rules::default();
        let mut grid = crate::world::WorldGrid::new(40, 40);
        let snow = content.block_id("snow").expect("snow");
        for x in 0..20 {
            grid.tiles.get_mut(x, 0).floor = snow;
        }
        let entries = add_weather(&grid, &content, &rules);
        assert!(entries.is_empty(), "{:?}", weather_names(&entries));
    }
}
