// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Flat simplex-noise generator (`world gen --generator simplex`).
//!
//! The plan's "flat `Simplex2D` implementation used by tests/filters" (§3.10).
//! Produces a deterministic floor/wall/ore field for the M6 golden.

use crate::content::{BlockId, ContentRegistry};
use crate::math::noise;
use crate::world::WorldParams;
use crate::world::tiles::Tiles;

use super::WorldGenerator;
use super::basic::{BasicGenerator, SimplexNoise};

/// A flat simplex-noise terrain generator (test/oracle generator).
pub struct SimplexGenerator {
    /// Shared helper library.
    pub base: BasicGenerator,
    /// Base seed.
    pub seed: i32,
}

impl SimplexGenerator {
    /// Builds a generator from a seed.
    pub fn new(seed: u64) -> Self {
        let mut base = BasicGenerator::new(Box::new(SimplexNoise { seed: seed as i32 }));
        base.set_seed(seed);
        Self {
            base,
            seed: seed as i32,
        }
    }
}

impl WorldGenerator for SimplexGenerator {
    fn name(&self) -> &'static str {
        "simplex"
    }

    fn generate(&mut self, tiles: &mut Tiles, _params: &WorldParams, content: &ContentRegistry) {
        self.base.begin(tiles);
        let seed = self.seed;
        let stone = content.block_id("stone").unwrap_or(BlockId::AIR);
        let sand = content.block_id("sand-floor").unwrap_or(stone);
        let wall = content.block_id("stone-wall").unwrap_or(BlockId::AIR);

        self.base.pass(tiles, content, |_, draw, x, y| {
            let n = noise::noise2d(seed, 3, 0.5, 1.0 / 48.0, x as f64, y as f64);
            draw.floor = if n > 0.55 { sand } else { stone };
            draw.block = if n < 0.34 { wall } else { BlockId::AIR };
        });

        let ores: Vec<BlockId> = ["ore-copper", "ore-lead", "ore-titanium"]
            .iter()
            .filter_map(|name| content.block_id(name))
            .collect();
        if !ores.is_empty() {
            self.base.ores(tiles, content, &ores);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::determinism::{Checksum, Hasher};

    fn checksum(tiles: &Tiles) -> String {
        let mut hasher = Hasher::new();
        hasher.write_u32(tiles.width as u32);
        hasher.write_u32(tiles.height as u32);
        for index in 0..tiles.len() {
            let tile = tiles.geti(index);
            hasher.write_u16(tile.block.raw());
            hasher.write_u16(tile.floor.raw());
            hasher.write_u16(tile.overlay.raw());
        }
        Checksum(hasher.finish().value()).to_hex()
    }

    /// `maps::generators::tests::simplex_generation_checksum` — the M6 golden.
    ///
    /// Golden recorded from the Rust build (plan 06 §7b; structural parity only,
    /// OD6-B): `world gen --generator simplex --seed 7 --width 128 --height 128`.
    #[test]
    fn simplex_generation_checksum() {
        let content = crate::content::test_support::test_registry();
        let mut tiles = Tiles::new(128, 128);
        let mut generator = SimplexGenerator::new(7);
        generator.generate(&mut tiles, &crate::world::WorldParams::default(), &content);
        assert_eq!(checksum(&tiles), "bc1c505dc6481a14");
        // Structurally valid: not all air, has at least one wall and one ore.
        assert!(tiles.iter().any(|tile| tile.block != BlockId::AIR));
        assert!(tiles.iter().any(|tile| tile.overlay != BlockId::AIR));
    }
}
