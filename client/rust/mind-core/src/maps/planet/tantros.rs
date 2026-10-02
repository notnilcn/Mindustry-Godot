// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TantrosPlanetGenerator` (plan 06 §3.11). Ported from
//! `maps/planet/TantrosPlanetGenerator.java`.
//!
//! Deferred (named): `addWeather` and `Schematics.placeLaunchLoadout` are
//! plan-12 hooks; `world.getDarkness`-driven wall trim uses a zero darkness
//! (no sector polygon yet), so floors are exact and walls are a subset.

use crate::content::{BlockId, ContentRegistry};
use crate::determinism::RngStream;
use crate::maps::filters::block_info;
use crate::math::noise;
use crate::world::WorldParams;
use crate::world::cached::TileGen;
use crate::world::tiles::Tiles;

use crate::maps::generators::WorldGenerator;
use crate::maps::generators::planet::{FlatSectorView, HexMesher, PlanetGenerator};

/// Tantros' floor matrix (one row).
const ARR: [[&str; 5]; 1] = [["redmat", "redmat", "darksand", "bluemat", "bluemat"]];

/// Tantros planet generator.
#[derive(Debug)]
pub struct TantrosPlanetGenerator {
    /// Shared planet generator.
    pub planet: PlanetGenerator,
    /// Sector view.
    pub sector: FlatSectorView,
    c1: [f32; 3],
    c2: [f32; 3],
}

impl Default for TantrosPlanetGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl TantrosPlanetGenerator {
    /// Creates the generator (`baseSeed = 1`, `getSizeScl = 2000`).
    pub fn new() -> Self {
        let mut planet = PlanetGenerator::new(1);
        planet.size_scl = 2000.0;
        Self {
            planet,
            sector: FlatSectorView::default(),
            c1: [
                0x50 as f32 / 255.0,
                0x57 as f32 / 255.0,
                0xa6 as f32 / 255.0,
            ],
            c2: [
                0x27 as f32 / 255.0,
                0x27 as f32 / 255.0,
                0x66 as f32 / 255.0,
            ],
        }
    }
}

/// `TantrosPlanetGenerator.rawHeight`.
fn raw_height(seed: i32, position: [f32; 3]) -> f32 {
    noise::noise3d(
        seed,
        8,
        0.7,
        1.0,
        position[0] as f64,
        position[1] as f64,
        position[2] as f64,
    )
}

impl HexMesher for TantrosPlanetGenerator {
    fn get_height(&self, _position: [f32; 3]) -> f32 {
        0.0
    }

    fn get_color(&self, position: [f32; 3]) -> [f32; 4] {
        let depth = noise::noise3d(
            self.planet.seed,
            2,
            0.56,
            1.7,
            position[0] as f64,
            position[1] as f64,
            position[2] as f64,
        ) / 2.0;
        let t = (depth / 0.15).round() * 0.15;
        let t = t.clamp(0.0, 1.0);
        [
            self.c1[0] + (self.c2[0] - self.c1[0]) * t,
            self.c1[1] + (self.c2[1] - self.c1[1]) * t,
            self.c1[2] + (self.c2[2] - self.c1[2]) * t,
            0.8,
        ]
    }
}

impl WorldGenerator for TantrosPlanetGenerator {
    fn name(&self) -> &'static str {
        "tantros"
    }

    fn generate(&mut self, tiles: &mut Tiles, params: &WorldParams, content: &ContentRegistry) {
        let redmat = content.block_id("redmat").unwrap_or(BlockId::AIR);
        let bluemat = content.block_id("bluemat").unwrap_or(BlockId::AIR);
        let redweed = content.block_id("redweed").unwrap_or(BlockId::AIR);
        let purbush = content.block_id("pur-bush").unwrap_or(BlockId::AIR);
        let yellowcoral = content.block_id("yellowcoral").unwrap_or(BlockId::AIR);
        let generator = &mut self.planet;

        let sector = self.sector;
        generator.run(
            tiles,
            params,
            &sector,
            |planet, position, brush: &mut TileGen| {
                let floor = get_block_static(planet.seed, content, position);
                brush.floor = floor;
                if floor == redmat && planet.base.rand.chance(RngStream::MapGen, 0.1) {
                    brush.block = redweed;
                } else if floor == bluemat && planet.base.rand.chance(RngStream::MapGen, 0.03) {
                    brush.block = purbush;
                } else if floor == bluemat && planet.base.rand.chance(RngStream::MapGen, 0.002) {
                    brush.block = yellowcoral;
                }
            },
        );

        // `generate()`: trim walls from darkness (zero darkness for now) and
        // place the launch loadout (plan-12 hook).
        generator.base.pass(tiles, content, |planet, draw, x, y| {
            let mut max = 0.0f32;
            for (dx, dy) in crate::maps::generators::basic::D8 {
                max = max.max(sector_darkness(x + dx, y + dy));
            }
            if max > 0.0
                && let Some(def) = content.block(draw.floor)
                && let Some(wall) = block_info::wall(content, def)
            {
                draw.block = wall;
            }
            let _ = planet;
        });
    }
}

/// Free helper mirroring `TantrosPlanetGenerator.getBlock`.
fn get_block_static(seed: i32, content: &ContentRegistry, position: [f32; 3]) -> BlockId {
    let height = (raw_height(seed, position) * 1.2).clamp(0.0, 1.0);
    let scaled = [position[0] * 2.0, position[1] * 2.0, position[2] * 2.0];
    let temp = noise::noise3d(
        seed,
        8,
        0.6,
        0.5,
        scaled[0] as f64,
        (scaled[1] + 99.0) as f64,
        scaled[2] as f64,
    );
    let row = (temp * ARR.len() as f32).clamp(0.0, (ARR.len() - 1) as f32) as usize;
    let col = (height * ARR[0].len() as f32).clamp(0.0, (ARR[0].len() - 1) as f32) as usize;
    content.block_id(ARR[row][col]).unwrap_or(BlockId::AIR)
}

/// Sector darkness at `(x, y)`; plan 12 supplies the real polygon falloff.
fn sector_darkness(_x: i32, _y: i32) -> f32 {
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tantros_has_redmat_and_bluemat() {
        let content = crate::content::test_support::test_registry();
        let mut tiles = Tiles::new(64, 64);
        let mut generator = TantrosPlanetGenerator::new();
        generator.generate(&mut tiles, &WorldParams::default(), &content);

        let redmat = content.block_id("redmat").unwrap();
        let bluemat = content.block_id("bluemat").unwrap();
        assert!(tiles.iter().any(|tile| tile.floor == redmat));
        assert!(tiles.iter().any(|tile| tile.floor == bluemat));
    }

    /// M8 golden (Rust-recorded, OD6-B): `world gen --generator planet --planet
    /// tantros --seed 11 --width 64 --height 64`.
    #[test]
    fn tantros_generation_checksum() {
        use crate::determinism::{Checksum, Hasher};
        let content = crate::content::test_support::test_registry();
        let mut tiles = Tiles::new(64, 64);
        let mut generator = TantrosPlanetGenerator::new();
        let params = WorldParams {
            seed_offset: 11,
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
            "7762880fe0716320"
        );
    }
}
