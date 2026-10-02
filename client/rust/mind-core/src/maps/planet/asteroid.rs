// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `AsteroidGenerator` (plan 06 §3.11). Ported from
//! `maps/planet/AsteroidGenerator.java`.
//!
//! Deviations (OD6-A/OD6-B, structural parity): the upstream seed is
//! `state.rules.sector.planet.id`; the headless harness passes `WorldParams`
//! instead so per-seed goldens are meaningful. `state.rules.*`,
//! `Schematics.placeLaunchLoadout` and `Waves.generate` are plan-11/12 hooks and
//! are not applied here (they do not change tile data).

use crate::content::{BlockId, ContentRegistry};
use crate::determinism::{RngStream, SimRng};
use crate::maps::filters::block_info;
use crate::math::{noise, ridged};
use crate::world::WorldParams;
use crate::world::tiles::Tiles;

use crate::maps::generators::WorldGenerator;
use crate::maps::generators::planet::{FlatSectorView, PlanetGenerator};

/// `Geometry.d8edge` (Arc `Geometry.java`): the four diagonal corner directions.
const D8_EDGE: [(i32, i32); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

/// `AsteroidGenerator` (`maps/planet/AsteroidGenerator.java`).
#[derive(Debug)]
pub struct AsteroidGenerator {
    /// Shared planet helper (noise seed `0`; `BlankPlanetGenerator` base).
    pub planet: PlanetGenerator,
    /// Sector view.
    pub sector: FlatSectorView,
    /// Asteroid count range.
    pub min: i32,
    /// Asteroid count range.
    pub max: i32,
    /// Noise octaves for asteroid shape.
    pub octaves: i32,
    /// Octaves for the random stone noise.
    pub foct: i32,
    /// Asteroid radius range.
    pub rad_min: f32,
    /// Asteroid radius range.
    pub rad_max: f32,
    /// Simplex persistence.
    pub persistence: f32,
    /// Asteroid shape scale.
    pub scale: f32,
    /// Asteroid shape magnitude.
    pub mag: f32,
    /// Asteroid shape threshold.
    pub thresh: f32,
    /// Random-stone noise magnitude.
    pub fmag: f32,
    /// Random-stone noise scale.
    pub fscl: f32,
    /// Random-stone noise persistence.
    pub fper: f32,
    /// Floor probabilities.
    pub stone_chance: f32,
    /// Floor probabilities.
    pub ice_chance: f32,
    /// Floor probabilities.
    pub carbon_chance: f32,
    /// Floor probabilities.
    pub beryl_chance: f32,
    /// Floor probabilities.
    pub ferric_chance: f32,
    /// Ore scale multipliers.
    pub thorium_scl: f32,
    /// Ore scale multipliers.
    pub copper_scale: f32,
    /// Ore scale multipliers.
    pub lead_scale: f32,
    /// Ore scale multipliers.
    pub graphite_scale: f32,
    /// Ore scale multipliers.
    pub beryllium_scale: f32,
    /// Per-run seed (upstream `state.rules.sector.planet.id`).
    seed: i32,
    /// Generator RNG (deviation §2.3.5).
    rand: SimRng,
}

impl Default for AsteroidGenerator {
    fn default() -> Self {
        Self::new(0)
    }
}

impl AsteroidGenerator {
    /// Creates the generator with upstream defaults.
    pub fn new(seed: i32) -> Self {
        Self {
            planet: PlanetGenerator::new(0),
            sector: FlatSectorView::default(),
            min: 20,
            max: 30,
            octaves: 2,
            foct: 3,
            rad_min: 12.0,
            rad_max: 60.0,
            persistence: 0.4,
            scale: 30.0,
            mag: 0.46,
            thresh: 1.0,
            fmag: 0.5,
            fscl: 50.0,
            fper: 0.6,
            stone_chance: 0.0,
            ice_chance: 0.0,
            carbon_chance: 0.0,
            beryl_chance: 0.0,
            ferric_chance: 1.0,
            thorium_scl: 1.0,
            copper_scale: 1.0,
            lead_scale: 1.0,
            graphite_scale: 1.0,
            beryllium_scale: 1.0,
            seed,
            rand: SimRng::new(seed as u64),
        }
    }

    /// Stamps one asteroid disc at `(ax, ay)` (`AsteroidGenerator.asteroid`).
    fn asteroid(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        ax: i32,
        ay: i32,
        radius: i32,
    ) {
        let floor = if self.rand.chance(RngStream::MapGen, self.ice_chance as f64) {
            content.block_id("ice")
        } else if self
            .rand
            .chance(RngStream::MapGen, self.carbon_chance as f64)
        {
            content.block_id("carbon-stone")
        } else if self
            .rand
            .chance(RngStream::MapGen, self.beryl_chance as f64)
        {
            content.block_id("beryllic-stone")
        } else if self
            .rand
            .chance(RngStream::MapGen, self.ferric_chance as f64)
        {
            content.block_id("ferric-stone")
        } else {
            content.block_id("stone")
        }
        .unwrap_or(BlockId::AIR);

        for x in (ax - radius)..=(ax + radius) {
            for y in (ay - radius)..=(ay + radius) {
                if !tiles.in_bounds(x, y) {
                    continue;
                }
                let d =
                    (((x - ax) as f32).powi(2) + ((y - ay) as f32).powi(2)).sqrt() / radius as f32;
                let n = noise::noise2d(
                    self.seed,
                    self.octaves,
                    self.persistence as f64,
                    1.0 / self.scale as f64,
                    x as f64,
                    y as f64,
                );
                if d + n * self.mag < self.thresh {
                    tiles.get_mut(x, y).floor = floor;
                }
            }
        }
    }
}

impl WorldGenerator for AsteroidGenerator {
    fn name(&self) -> &'static str {
        "asteroid"
    }

    fn generate(&mut self, tiles: &mut Tiles, params: &WorldParams, content: &ContentRegistry) {
        // Upstream: `seed = state.rules.sector.planet.id`. Harness: params seed.
        self.seed = params.seed_offset as i32;
        self.rand = SimRng::new(self.seed as u64);
        self.planet.prepare(tiles, params, &self.sector);

        let empty = content.block_id("empty").unwrap_or(BlockId::AIR);
        let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
        let ferric_stone = content.block_id("ferric-stone").unwrap_or(BlockId::AIR);
        let ferric_craters = content.block_id("ferric-craters").unwrap_or(BlockId::AIR);
        // Plan-02 gap: upstream `Blocks.craters` is not registered; fall back to
        // `crater-stone` so the random-crater pass still has a distinct floor.
        let craters = content
            .block_id("craters")
            .or_else(|| content.block_id("crater-stone"))
            .unwrap_or(BlockId::AIR);
        let stone_wall = content.block_id("stone-wall").unwrap_or(BlockId::AIR);
        let ore_lead = content.block_id("ore-lead").unwrap_or(BlockId::AIR);
        let ore_copper = content.block_id("ore-copper").unwrap_or(BlockId::AIR);
        let ore_thorium = content.block_id("ore-thorium").unwrap_or(BlockId::AIR);
        let ore_titanium = content.block_id("ore-titanium").unwrap_or(BlockId::AIR);
        let berylllic_stone = content.block_id("beryllic-stone").unwrap_or(BlockId::AIR);
        let carbon_stone = content.block_id("carbon-stone").unwrap_or(BlockId::AIR);
        let carbon_wall = content.block_id("carbon-wall").unwrap_or(BlockId::AIR);
        let graphitic_wall = content.block_id("graphitic-wall").unwrap_or(BlockId::AIR);
        let berylllic_stone_wall = content
            .block_id("beryllic-stone-wall")
            .unwrap_or(BlockId::AIR);
        let wall_ore_beryllium = content
            .block_id("ore-wall-beryllium")
            .unwrap_or(BlockId::AIR);
        let spawn = content.block_id("spawn").unwrap_or(BlockId::AIR);

        let width = tiles.width;
        let height = tiles.height;
        let sx = width / 2;
        let sy = height / 2;

        // All-empty background.
        for index in 0..tiles.len() {
            tiles.geti_mut(index).floor = empty;
        }

        // Spawn asteroids.
        let r0 = rand_range_int(&mut self.rand, 30, 50);
        self.asteroid(tiles, content, sx, sy, r0);

        let amount = rand_range_int(&mut self.rand, self.min, self.max);
        for _ in 0..amount {
            let radius = self
                .rand
                .range(RngStream::MapGen, self.rad_min, self.rad_max);
            let ax = self
                .rand
                .range(RngStream::MapGen, radius, width as f32 - radius);
            let ay = self
                .rand
                .range(RngStream::MapGen, radius, height as f32 - radius);
            self.asteroid(tiles, content, ax as i32, ay as i32, radius as i32);
        }

        // Tiny asteroids.
        let smalls = rand_range_int(&mut self.rand, self.min, self.max) * 3;
        for _ in 0..smalls {
            let radius = self.rand.range(RngStream::MapGen, 1.0, 8.0);
            let ax = self
                .rand
                .range(RngStream::MapGen, radius, width as f32 - radius);
            let ay = self
                .rand
                .range(RngStream::MapGen, radius, height as f32 - radius);
            self.asteroid(tiles, content, ax as i32, ay as i32, radius as i32);
        }

        let seed = self.seed;
        let foct = self.foct;
        let fper = self.fper as f64;
        let fscl = self.fscl;
        let fmag = self.fmag;
        // Random noise stone.
        self.planet.base.pass(tiles, content, |_, draw, x, y| {
            if draw.floor != empty {
                let a = ridged::noise2d(seed, x as f64, y as f64, foct, fper, 1.0 / fscl as f64);
                let b = ridged::noise2d(seed, x as f64, y as f64, 1, 1.0, 5.0) / 2.7;
                if a - b > fmag {
                    draw.floor = stone;
                }
            }
        });

        // Walls at insides (reads neighbour floors, so `pass` is inlined).
        let len = tiles.len();
        for index in 0..len {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let floor = tiles.geti(index).floor;
            if floor == empty {
                continue;
            }
            if ridged::noise2d(seed + 1, x as f64, y as f64, 4, 0.7, 1.0 / 60.0) > 0.45 {
                continue;
            }
            let extra = ridged::noise2d(seed, x as f64, y as f64, 3, 0.5, 1.0 / 30.0) * 6.0;
            if within_f(x, y, sx, sy, 20.0 + extra) {
                continue;
            }
            let radius = 6;
            let mut blocked = false;
            'scan: for dx in (x - radius)..=(x + radius) {
                for dy in (y - radius)..=(y + radius) {
                    if within_f(dx, dy, x, y, radius as f32 + 0.0001)
                        && tiles.getn(dx, dy).is_some_and(|t| t.floor == empty)
                    {
                        blocked = true;
                        break 'scan;
                    }
                }
            }
            if blocked {
                continue;
            }
            let wall = content
                .block(floor)
                .and_then(|def| block_info::wall(content, def))
                .unwrap_or(BlockId::AIR);
            tiles.geti_mut(index).block = wall;
        }

        // Random craters.
        self.planet.base.pass(tiles, content, |b, draw, _, _| {
            if draw.floor == ferric_stone && b.rand.chance(RngStream::MapGen, 0.02) {
                draw.floor = ferric_craters;
            }
            if draw.floor == stone && b.rand.chance(RngStream::MapGen, 0.02) {
                draw.floor = craters;
            }
        });

        self.planet.base.decoration(tiles, content, 0.017);

        // Lead around stone walls.
        self.planet.base.ore_around(
            tiles,
            content,
            ore_lead,
            stone_wall,
            3,
            70.0,
            0.6 * self.lead_scale,
        );

        // Copper on ferric stone.
        self.planet.base.ore(
            tiles,
            content,
            ore_copper,
            ferric_stone,
            5.0,
            0.8 * self.copper_scale,
        );

        // Thorium on berylllic / graphitic stone.
        self.planet.base.ore(
            tiles,
            content,
            ore_thorium,
            berylllic_stone,
            4.0,
            0.9 * self.thorium_scl,
        );
        self.planet.base.ore(
            tiles,
            content,
            ore_thorium,
            carbon_stone,
            4.0,
            0.9 * self.thorium_scl,
        );

        self.planet.base.wall_ore(
            tiles,
            content,
            carbon_wall,
            graphitic_wall,
            35.0,
            0.57 * self.graphite_scale,
        );
        self.planet.base.wall_ore(
            tiles,
            content,
            berylllic_stone_wall,
            wall_ore_beryllium,
            50.0,
            0.62 * self.beryllium_scale,
        );

        // Titanium.
        self.planet.base.pass(tiles, content, |b, draw, x, y| {
            if draw.floor != stone {
                return;
            }
            let i = 4;
            if (0.5
                - b.noise_oct1(
                    x as f32,
                    (y + i * 999) as f32 - x as f32 * 1.5,
                    2.0,
                    0.65,
                    (60 + i * 2) as f64,
                ))
            .abs()
                > 0.26
            {
                draw.ore = ore_titanium;
            }
        });

        // Spawn marker at the map edge.
        let spawn_side = self.rand.random(RngStream::MapGen, 3) as usize;
        let size_offset = width / 2 - 1;
        let px = size_offset * D8_EDGE[spawn_side].0 + width / 2;
        let py = size_offset * D8_EDGE[spawn_side].1 + height / 2;
        if tiles.in_bounds(px, py) {
            tiles.get_mut(px, py).overlay = spawn;
        }

        // `Schematics.placeLaunchLoadout(sx, sy)`, `state.rules.*` and
        // `Waves.generate` are plan-12 hooks (no tile effect).
    }
}

/// Arc `Rand.random(int min, int max)` (inclusive).
fn rand_range_int(rand: &mut SimRng, min: i32, max: i32) -> i32 {
    min + rand.random(RngStream::MapGen, max - min + 1)
}

/// `Mathf.within(x, y, cx, cy, dist)`.
fn within_f(x: i32, y: i32, cx: i32, cy: i32, dist: f32) -> bool {
    let dx = (x - cx) as f32;
    let dy = (y - cy) as f32;
    dx * dx + dy * dy <= dist * dist
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ContentRegistry {
        crate::content::test_support::test_registry()
    }

    #[test]
    fn asteroid_has_background_and_rock() {
        let content = registry();
        let mut tiles = Tiles::new(128, 128);
        let mut generator = AsteroidGenerator::new(7);
        generator.generate(&mut tiles, &WorldParams::default(), &content);

        let empty = content.block_id("empty").unwrap();
        let ferric = content.block_id("ferric-stone").unwrap();
        assert!(
            tiles.iter().any(|tile| tile.floor == empty),
            "background present"
        );
        assert!(
            tiles.iter().any(
                |tile| tile.floor == ferric || tile.floor == content.block_id("stone").unwrap()
            ),
            "asteroid floor present"
        );
        assert!(
            tiles.iter().any(|tile| tile.overlay != BlockId::AIR),
            "at least one ore/spawn overlay"
        );
    }

    /// M8 golden (Rust-recorded, OD6-B): `world gen --planet asteroid --seed 7
    /// --width 128 --height 128`.
    #[test]
    fn asteroid_generation_checksum() {
        use crate::determinism::{Checksum, Hasher};
        let content = registry();
        let mut tiles = Tiles::new(128, 128);
        let mut generator = AsteroidGenerator::new(7);
        let params = WorldParams {
            seed_offset: 7,
            ..WorldParams::default()
        };
        generator.generate(&mut tiles, &params, &content);
        let mut hasher = Hasher::new();
        hasher.write_u32(tiles.width as u32);
        hasher.write_u32(tiles.height as u32);
        for index in 0..tiles.len() {
            let tile = tiles.geti(index);
            hasher.write_u16(tile.block.raw());
            hasher.write_u16(tile.floor.raw());
            hasher.write_u16(tile.overlay.raw());
        }
        assert_eq!(
            Checksum(hasher.finish().value()).to_hex(),
            "463dbb70d52f29a4"
        );
    }
}
