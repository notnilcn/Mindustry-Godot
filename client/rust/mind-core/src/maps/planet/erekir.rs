// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ErekirPlanetGenerator` (plan 06 §3.11). Ported from
//! `maps/planet/ErekirPlanetGenerator.java`.
//!
//! Deviations (OD6-A/OD6-B, structural parity): `state.rules.*`,
//! `Schematics.placeLaunchLoadout` and `Waves.generate` are plan-11/12 hooks and
//! are not applied (no tile effect); `world.getDarkness` is zero until plan 12's
//! sector polygon lands, and the `steam` floor attribute is absent from plan 02,
//! so vent placement does not skip steam tiles.

use crate::content::{BlockDef, BlockId, BlockKind, ContentRegistry};
use crate::determinism::{RngStream, SimRng};
use crate::maps::filters::block_info;
use crate::math::{noise, ridged};
use crate::world::WorldParams;
use crate::world::cached::TileGen;
use crate::world::tiles::Tiles;

use crate::maps::generators::WorldGenerator;
use crate::maps::generators::astar;
use crate::maps::generators::basic::{self, planet_wall};
use crate::maps::generators::planet::{FlatSectorView, HexMesher, PlanetGenerator, SectorView};

/// `SteamVent.offsets` after the static `sub(1, 1)` adjustment.
const STEAM_VENT_OFFSETS: [(i32, i32); 9] = [
    (-1, -1),
    (0, -1),
    (0, 0),
    (-1, 0),
    (-2, 0),
    (-2, -1),
    (-2, -2),
    (-1, -2),
    (0, -2),
];

/// Resolved content ids used by the Erekir generator.
#[derive(Debug, Clone, Copy, Default)]
struct Ids {
    air: BlockId,
    regolith: BlockId,
    yellow_stone: BlockId,
    rhyolite: BlockId,
    rough_rhyolite: BlockId,
    rhyolite_crater: BlockId,
    carbon_stone: BlockId,
    crystalline_stone: BlockId,
    crystal_floor: BlockId,
    berylllic_stone: BlockId,
    red_stone: BlockId,
    dense_red_stone: BlockId,
    red_ice: BlockId,
    arkycite_floor: BlockId,
    arkyic_stone: BlockId,
    arkyic_wall: BlockId,
    slag: BlockId,
    yellow_stone_plates: BlockId,
    red_stone_wall: BlockId,
    carbon_wall: BlockId,
    graphitic_wall: BlockId,
    regolith_wall: BlockId,
    wall_ore_beryllium: BlockId,
    ore_tungsten: BlockId,
    ore_crystal_thorium: BlockId,
    crystal_cluster: BlockId,
    vibrant_crystal_cluster: BlockId,
    crystal_orbs: BlockId,
    crystal_blocks: BlockId,
    rhyolite_vent: BlockId,
    carbon_vent: BlockId,
    arkyic_vent: BlockId,
    yellow_stone_vent: BlockId,
    red_stone_vent: BlockId,
    spawn: BlockId,
}

impl Ids {
    fn load(content: &ContentRegistry) -> Self {
        let id = |name: &str| content.block_id(name).unwrap_or(BlockId::AIR);
        Self {
            air: BlockId::AIR,
            regolith: id("regolith"),
            yellow_stone: id("yellow-stone"),
            rhyolite: id("rhyolite"),
            rough_rhyolite: id("rough-rhyolite"),
            rhyolite_crater: id("rhyolite-crater"),
            carbon_stone: id("carbon-stone"),
            crystalline_stone: id("crystalline-stone"),
            crystal_floor: id("crystal-floor"),
            berylllic_stone: id("beryllic-stone"),
            red_stone: id("red-stone"),
            dense_red_stone: id("dense-red-stone"),
            red_ice: id("red-ice"),
            arkycite_floor: id("arkycite-floor"),
            arkyic_stone: id("arkyic-stone"),
            arkyic_wall: id("arkyic-wall"),
            slag: id("molten-slag"),
            yellow_stone_plates: id("yellow-stone-plates"),
            red_stone_wall: id("red-stone-wall"),
            carbon_wall: id("carbon-wall"),
            graphitic_wall: id("graphitic-wall"),
            regolith_wall: id("regolith-wall"),
            wall_ore_beryllium: id("ore-wall-beryllium"),
            ore_tungsten: id("ore-tungsten"),
            ore_crystal_thorium: id("ore-crystal-thorium"),
            crystal_cluster: id("crystal-cluster"),
            vibrant_crystal_cluster: id("vibrant-crystal-cluster"),
            crystal_orbs: id("crystal-orbs"),
            crystal_blocks: id("crystal-blocks"),
            rhyolite_vent: id("rhyolite-vent"),
            carbon_vent: id("carbon-vent"),
            arkyic_vent: id("arkyic-vent"),
            yellow_stone_vent: id("yellow-stone-vent"),
            red_stone_vent: id("red-stone-vent"),
            spawn: id("spawn"),
        }
    }

    fn terrain(&self) -> [BlockId; 8] {
        [
            self.regolith,
            self.regolith,
            self.regolith,
            self.regolith,
            self.yellow_stone,
            self.rhyolite,
            self.rhyolite,
            self.carbon_stone,
        ]
    }
}

/// `ErekirPlanetGenerator` (`maps/planet/ErekirPlanetGenerator.java`).
#[derive(Debug)]
pub struct ErekirPlanetGenerator {
    /// Shared planet helper (`baseSeed = 2`).
    pub planet: PlanetGenerator,
    /// Sector view.
    pub sector: FlatSectorView,
    ids: Ids,
    /// Height scale (`heightScl`).
    pub height_scl: f32,
    /// Noise octaves.
    pub octaves: i32,
    /// Simplex persistence.
    pub persistence: f32,
    /// Height power.
    pub height_pow: f32,
    /// Height multiplier.
    pub height_mult: f32,
}

impl Default for ErekirPlanetGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl ErekirPlanetGenerator {
    /// Creates the generator (`baseSeed = 2`, `getSizeScl` override).
    pub fn new() -> Self {
        let mut planet = PlanetGenerator::new(2);
        planet.size_scl = 2000.0 * 1.07 * 6.0 / 5.0;
        Self {
            planet,
            sector: FlatSectorView::default(),
            ids: Ids::default(),
            height_scl: 0.9,
            octaves: 8,
            persistence: 0.7,
            height_pow: 3.0,
            height_mult: 1.6,
        }
    }
}

/// `ErekirPlanetGenerator.rawHeight`.
fn raw_height(
    seed: i32,
    octaves: i32,
    persistence: f32,
    height_scl: f32,
    position: [f32; 3],
) -> f32 {
    noise::noise3d(
        seed,
        octaves,
        persistence as f64,
        1.0 / height_scl as f64,
        (10.0 + position[0]) as f64,
        (10.0 + position[1]) as f64,
        (10.0 + position[2]) as f64,
    )
}

/// `ErekirPlanetGenerator.rawTemp`.
fn raw_temp(seed: i32, position: [f32; 3]) -> f32 {
    let dx = position[0];
    let dy = position[1];
    let dz = position[2] - 1.0;
    (dx * dx + dy * dy + dz * dz).sqrt() * 2.2
        - noise::noise3d(
            seed,
            8,
            0.54,
            1.4,
            (10.0 + position[0]) as f64,
            (10.0 + position[1]) as f64,
            (10.0 + position[2]) as f64,
        ) * 2.9
}

/// `ErekirPlanetGenerator.getBlock`.
fn get_block(seed: i32, position: [f32; 3], ids: &Ids) -> BlockId {
    let px = position[0];
    let py = position[1];
    let pz = position[2];

    let ice = raw_temp(seed, position);
    let mut height = raw_height(seed, 8, 0.7, 0.9, position);
    height *= 1.2;
    height = height.clamp(0.0, 1.0);

    let terrain = ids.terrain();
    let idx = ((height * terrain.len() as f32) as i32).clamp(0, terrain.len() as i32 - 1) as usize;
    let mut result = terrain[idx];

    let crystal = ridged::noise3d(
        seed + 8,
        (px + 4.0) as f64,
        (py + 8.0) as f64,
        (pz + 1.0) as f64,
        2,
        0.9,
    );
    if ice < 0.3 + crystal.abs() * 0.3 {
        return ids.crystalline_stone;
    }

    if ice < 0.6 && (result == ids.rhyolite || result == ids.yellow_stone || result == ids.regolith)
    {
        return ids.carbon_stone;
    }

    let ark = ridged::noise3d(
        seed + 7,
        (px + 2.0) as f64,
        (py + 8.0) as f64,
        (pz + 1.0) as f64,
        2,
        0.83,
    );
    if ice < 3.1 - 0.3 && ark > 0.28 {
        result = ids.berylllic_stone;
    }

    if ice > 3.1 {
        result = ids.red_stone;
    } else if ice > 3.1 - 0.4 {
        result = ids.regolith;
    }

    result
}

impl HexMesher for ErekirPlanetGenerator {
    fn get_height(&self, position: [f32; 3]) -> f32 {
        raw_height(
            self.planet.seed,
            self.octaves,
            self.persistence,
            self.height_scl,
            position,
        )
        .powf(self.height_pow)
            * self.height_mult
    }

    fn get_color(&self, position: [f32; 3]) -> [f32; 4] {
        // `Block.mapColor` with `1 - albedo` alpha. The color table is a plan-16
        // render detail; the block selection still runs so plan 16 can consume
        // it (kept as an opaque placeholder here).
        let mut _block = get_block(self.planet.seed, position, &self.ids);
        if _block == self.ids.crystalline_stone {
            _block = self.ids.crystal_floor;
        }
        [0.0, 0.0, 0.0, 1.0]
    }
}

impl WorldGenerator for ErekirPlanetGenerator {
    fn name(&self) -> &'static str {
        "erekir"
    }

    fn generate(&mut self, tiles: &mut Tiles, params: &WorldParams, content: &ContentRegistry) {
        let ids = Ids::load(content);
        self.ids = ids;

        // `genTile` loop.
        self.planet.run(
            tiles,
            params,
            &self.sector,
            |planet, position, brush: &mut TileGen| {
                let mut floor = get_block(planet.seed, position, &ids);
                if floor == ids.rhyolite && planet.base.rand.chance(RngStream::MapGen, 0.01) {
                    floor = ids.rhyolite_crater;
                }
                brush.floor = floor;
                brush.block = planet_wall(content, floor);
                if ridged::noise3d(
                    planet.seed + 1,
                    position[0] as f64,
                    position[1] as f64,
                    position[2] as f64,
                    2,
                    14.0,
                ) > 0.13
                {
                    brush.block = ids.air;
                }
                if ridged::noise3d(
                    planet.seed + 2,
                    position[0] as f64,
                    (position[1] + 4.0) as f64,
                    position[2] as f64,
                    3,
                    6.0,
                ) > 0.6
                {
                    brush.floor = ids.carbon_stone;
                }
            },
        );

        let width = tiles.width;
        let height = tiles.height;
        let seed = self.planet.seed;
        let temp = raw_temp(seed, self.sector.tile_v());

        // Hot biome (red ice -> slag / yellow stone).
        if temp > 0.7 {
            self.planet.base.pass(tiles, content, |b, draw, x, y| {
                if draw.floor != ids.red_ice {
                    let n = b.noise_oct((x + 782) as f32, y as f32, 7.0, 0.8, 280.0, 1.0);
                    if n > 0.62 {
                        if n > 0.635 {
                            draw.floor = ids.slag;
                        } else {
                            draw.floor = ids.yellow_stone;
                        }
                        draw.ore = ids.air;
                    }
                    if n > 0.55 && draw.floor == ids.berylllic_stone {
                        draw.floor = ids.yellow_stone;
                    }
                }
            });
        }

        self.planet.base.cells(tiles, content, 4, 16, 16, 3);

        // Regolith walls.
        self.planet.base.pass(tiles, content, |b, draw, x, y| {
            if draw.floor == ids.regolith
                && b.noise_oct(x as f32, y as f32, 3.0, 0.4, 13.0, 1.0) > 0.59
            {
                draw.block = planet_wall(content, ids.regolith);
            }
        });

        // Spawn -> end corridor.
        let length = width as f32 / 2.6;
        let angle = self.planet.base.rand.range(RngStream::MapGen, 0.0, 360.0);
        let trns_x = angle.to_radians().cos() * length;
        let trns_y = angle.to_radians().sin() * length;
        let spawn_x = (trns_x + width as f32 / 2.0) as i32;
        let spawn_y = (trns_y + height as f32 / 2.0) as i32;
        let end_x = (-trns_x + width as f32 / 2.0) as i32;
        let end_y = (-trns_y + height as f32 / 2.0) as i32;
        let center_x = width as f32 / 2.0;
        let center_y = height as f32 / 2.0;
        let maxd = (center_x * center_x + center_y * center_y).sqrt();

        self.planet.base.erase(tiles, content, spawn_x, spawn_y, 15);
        let path = astar::pathfind(
            tiles,
            (spawn_x, spawn_y),
            (end_x, end_y),
            |_, to| {
                (if to.solid(content) { 300.0 } else { 0.0 }) + maxd
                    - basic::dst(to.x as f32, to.y as f32, center_x, center_y) / 10.0
            },
            |_| true,
        );
        self.planet.base.brush(tiles, content, &path, 9);
        self.planet.base.erase(tiles, content, end_x, end_y, 15);

        // Arkycite.
        let len = tiles.len();
        for index in 0..len {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let mut floor = tiles.geti(index).floor;
            if floor != ids.berylllic_stone {
                continue;
            }
            let n = self
                .planet
                .base
                .noise_oct(x as f32, (y + 500) as f32, 5.0, 0.6, 40.0, 1.0);
            if (n - 0.5).abs() < 0.09 {
                floor = ids.arkyic_stone;
            }
            if self.planet.base.near_wall(tiles, content, x, y) {
                tiles.geti_mut(index).floor = floor;
                continue;
            }
            let n = self.planet.base.noise_oct(
                (x + 300) as f32,
                y as f32 - x as f32 * 1.6 + 100.0,
                4.0,
                0.8,
                87.0,
                1.0,
            );
            if n > 0.64 {
                floor = ids.arkycite_floor;
            }
            tiles.geti_mut(index).floor = floor;
        }

        self.planet
            .base
            .median_target(tiles, content, 2, 0.6, ids.arkycite_floor);
        self.planet
            .base
            .blend(tiles, content, ids.arkycite_floor, ids.arkyic_stone, 4.0);
        self.planet
            .base
            .blend(tiles, content, ids.slag, ids.yellow_stone_plates, 4.0);
        self.planet.base.distort(tiles, content, 10.0, 12.0);
        self.planet.base.distort(tiles, content, 5.0, 7.0);
        self.planet
            .base
            .median_target(tiles, content, 2, 0.6, ids.arkycite_floor);
        self.planet
            .base
            .median_target(tiles, content, 3, 0.6, ids.slag);

        // Rough rhyolite / plates / walls.
        for index in 0..len {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let mut floor = tiles.geti(index).floor;
            let mut block = tiles.geti(index).block;

            if self
                .planet
                .base
                .noise_oct((x + 600 + y) as f32, y as f32, 5.0, 0.86, 60.0, 1.0)
                < 0.41
                && floor == ids.rhyolite
            {
                floor = ids.rough_rhyolite;
            }

            if floor == ids.slag {
                let n = self
                    .planet
                    .base
                    .noise_oct(x as f32, y as f32, 2.0, 0.8, 9.0, 15.0);
                if basic::within_f(x, y, spawn_x, spawn_y, 30.0 + n) {
                    floor = ids.yellow_stone_plates;
                }
            }

            if (floor == ids.arkycite_floor || floor == ids.arkyic_stone)
                && is_static(content, block)
            {
                block = ids.arkyic_wall;
            }

            if floor == ids.yellow_stone_plates
                && self
                    .planet
                    .base
                    .noise_oct((x + 78 + y) as f32, y as f32, 3.0, 0.8, 6.0, 1.0)
                    > 0.44
            {
                floor = ids.yellow_stone;
            }
            if floor == ids.red_stone
                && self
                    .planet
                    .base
                    .noise_oct((x + 78 - y) as f32, y as f32, 4.0, 0.73, 19.0, 1.0)
                    > 0.63
            {
                floor = ids.dense_red_stone;
            }

            let tile = tiles.geti_mut(index);
            tile.floor = floor;
            tile.block = block;
        }

        self.planet
            .base
            .inverse_flood_fill(tiles, content, (spawn_x, spawn_y));
        self.planet
            .base
            .blend(tiles, content, ids.red_stone_wall, ids.dense_red_stone, 4.0);
        self.planet.base.erase(tiles, content, end_x, end_y, 6);

        if tiles.in_bounds(end_x, end_y) {
            tiles.get_mut(end_x, end_y).overlay = ids.spawn;
        }

        // Ores.
        for index in 0..len {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let mut floor = tiles.geti(index).floor;
            let mut block = tiles.geti(index).block;
            let mut ore = tiles.geti(index).overlay;

            if block != ids.air {
                if self.planet.base.near_air(tiles, x, y) {
                    if block == ids.carbon_wall
                        && self.planet.base.noise_oct(
                            (x + 78) as f32,
                            y as f32,
                            4.0,
                            0.7,
                            33.0,
                            1.0,
                        ) > 0.52
                    {
                        block = ids.graphitic_wall;
                    } else if block != ids.carbon_wall
                        && self.planet.base.noise_oct(
                            (x + 782) as f32,
                            y as f32,
                            4.0,
                            0.8,
                            38.0,
                            1.0,
                        ) > 0.665
                    {
                        ore = ids.wall_ore_beryllium;
                    }
                }
            } else if !self.planet.base.near_wall(tiles, content, x, y) {
                if self.planet.base.noise_oct(
                    (x + 150) as f32,
                    (y + x * 2 + 100) as f32,
                    4.0,
                    0.8,
                    55.0,
                    1.0,
                ) > 0.76
                {
                    ore = ids.ore_tungsten;
                }
                if self.planet.base.noise_oct(
                    (x + 999) as f32,
                    (y + 600 - x) as f32,
                    4.0,
                    0.63,
                    45.0,
                    1.0,
                ) < 0.27
                    && floor == ids.crystalline_stone
                {
                    ore = ids.ore_crystal_thorium;
                }
            }

            if self.planet.base.noise_oct(
                (x + 999) as f32,
                (y + 600 - x) as f32,
                5.0,
                0.8,
                45.0,
                1.0,
            ) < 0.44
                && floor == ids.crystalline_stone
            {
                floor = ids.crystal_floor;
            }

            if block == ids.air
                && (floor == ids.crystalline_stone || floor == ids.crystal_floor)
                && self.planet.base.rand.chance(RngStream::MapGen, 0.09)
                && self.planet.base.near_wall(tiles, content, x, y)
                && !self.planet.base.near(tiles, x, y, 4, ids.crystal_cluster)
                && !self
                    .planet
                    .base
                    .near(tiles, x, y, 4, ids.vibrant_crystal_cluster)
            {
                block = if floor == ids.crystal_floor {
                    ids.vibrant_crystal_cluster
                } else {
                    ids.crystal_cluster
                };
                ore = ids.air;
            }

            if block == ids.arkyic_wall
                && self.planet.base.rand.chance(RngStream::MapGen, 0.23)
                && self.planet.base.near_air(tiles, x, y)
                && !self.planet.base.near(tiles, x, y, 3, ids.crystal_orbs)
            {
                block = ids.crystal_orbs;
                ore = ids.air;
            }

            if block == ids.regolith_wall
                && self.planet.base.rand.chance(RngStream::MapGen, 0.3)
                && self.planet.base.near_air(tiles, x, y)
                && !self.planet.base.near(tiles, x, y, 3, ids.crystal_blocks)
            {
                block = ids.crystal_blocks;
                ore = ids.air;
            }

            let tile = tiles.geti_mut(index);
            tile.floor = floor;
            tile.block = block;
            tile.overlay = ore;
        }

        // Remove tall props near ores.
        for index in 0..len {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let block = tiles.geti(index).block;
            let ore = tiles.geti(index).overlay;
            let ore_wall = content.block(ore).map(|def| def.wall_ore).unwrap_or(false);
            let block_drop = content
                .block(block)
                .map(|def| def.item_drop.is_some())
                .unwrap_or(false);
            if ore_wall || block_drop || (block == ids.air && ore != ids.air) {
                self.planet
                    .base
                    .remove_wall(tiles, content, x, y, 3, |def| {
                        def.kind == BlockKind::TallBlock
                    });
            }
        }

        self.planet.base.trim_dark(tiles, content, |_, _| 0.0);

        // Steam vents.
        let mut vent_count = 0;
        let min_vents = rand_range_int(&mut self.planet.base.rand, 6, 9);

        for index in 0..len {
            let (tx, ty) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let floor = tiles.geti(index).floor;
            if (floor != ids.rhyolite && floor != ids.rough_rhyolite)
                || !self.planet.base.rand.chance(RngStream::MapGen, 0.002)
            {
                continue;
            }
            let radius = 2;
            let mut ok = true;
            'chk: for dx in -radius..=radius {
                for dy in -radius..=radius {
                    match tiles.getn(tx + dx, ty + dy) {
                        Some(other) => {
                            if (other.floor != ids.rhyolite && other.floor != ids.rough_rhyolite)
                                || is_solid(content, other.block)
                            {
                                ok = false;
                                break 'chk;
                            }
                        }
                        None => {
                            ok = false;
                            break 'chk;
                        }
                    }
                }
            }
            if !ok {
                continue;
            }
            vent_count += 1;
            for (px, py) in STEAM_VENT_OFFSETS {
                let wx = px + tx + 1;
                let wy = py + ty + 1;
                if tiles.in_bounds(wx, wy) {
                    tiles.get_mut(wx, wy).floor = ids.rhyolite_vent;
                }
            }
        }

        let mut iterations = 0;
        while vent_count < min_vents && iterations < 5 {
            iterations += 1;
            'tile: for index in 0..len {
                let (tx, ty) = {
                    let tile = tiles.geti(index);
                    (tile.x as i32, tile.y as i32)
                };
                if !self
                    .planet
                    .base
                    .rand
                    .chance(RngStream::MapGen, 0.00018 * (1.0 + iterations as f64))
                {
                    continue;
                }
                if basic::within_f(tx, ty, spawn_x, spawn_y, 5.0) {
                    continue;
                }
                let tfloor = tiles.geti(index).floor;
                if tfloor == ids.crystalline_stone || tfloor == ids.crystal_floor {
                    continue;
                }
                let radius = 1;
                for dx in -radius..=radius {
                    for dy in -radius..=radius {
                        match tiles.getn(tx + dx, ty + dy) {
                            Some(other) => {
                                if is_solid(content, other.block)
                                    || steam_attr(content, other.floor) != 0.0
                                    || other.floor == ids.slag
                                    || other.floor == ids.arkycite_floor
                                {
                                    continue 'tile;
                                }
                            }
                            None => continue 'tile,
                        }
                    }
                }

                let mut floor = ids.rhyolite;
                let mut second_floor = ids.rhyolite_crater;
                let mut vent = ids.rhyolite_vent;
                let mut x_dir = 1.0f32;
                if tfloor == ids.berylllic_stone || tfloor == ids.arkyic_stone {
                    floor = ids.arkyic_stone;
                    second_floor = ids.arkyic_stone;
                    vent = ids.arkyic_vent;
                } else if tfloor == ids.yellow_stone
                    || tfloor == ids.yellow_stone_plates
                    || tfloor == ids.regolith
                {
                    floor = ids.yellow_stone;
                    second_floor = ids.yellow_stone_plates;
                    vent = ids.yellow_stone_vent;
                } else if tfloor == ids.red_stone || tfloor == ids.dense_red_stone {
                    floor = ids.dense_red_stone;
                    second_floor = ids.red_stone;
                    vent = ids.red_stone_vent;
                    x_dir = -1.0;
                } else if tfloor == ids.carbon_stone {
                    floor = ids.carbon_stone;
                    second_floor = ids.carbon_stone;
                    vent = ids.carbon_vent;
                }

                vent_count += 1;
                for (px, py) in STEAM_VENT_OFFSETS {
                    let wx = px + tx + 1;
                    let wy = py + ty + 1;
                    if tiles.in_bounds(wx, wy) {
                        tiles.get_mut(wx, wy).floor = vent;
                    }
                }

                let crad = rand_range_int(&mut self.planet.base.rand, 6, 14);
                let crad2 = crad * crad;
                for cx in -crad..=crad {
                    for cy in -crad..=crad {
                        let rx = cx + tx;
                        let ry = cy + ty;
                        let rcy = cy as f32 + cx as f32 * 0.9;
                        let n = self.planet.base.noise_oct(
                            rx as f32,
                            ry as f32 + rx as f32 * 2.0 * x_dir,
                            2.0,
                            0.7,
                            8.0,
                            crad2 as f64 * 1.1,
                        );
                        if (cx * cx) as f32 + rcy * rcy <= crad2 as f32 - n {
                            let (dest_floor, dest_block) = match tiles.getn(rx, ry) {
                                Some(dest) => (dest.floor, dest.block),
                                None => continue,
                            };
                            if steam_attr(content, dest_floor) == 0.0
                                && dest_floor != ids.rough_rhyolite
                                && dest_floor != ids.arkycite_floor
                                && dest_floor != ids.slag
                            {
                                let new_floor =
                                    if self.planet.base.rand.chance(RngStream::MapGen, 0.08) {
                                        second_floor
                                    } else {
                                        floor
                                    };
                                if is_static(content, dest_block) {
                                    tiles.get_mut(rx, ry).block = planet_wall(content, floor);
                                }
                                tiles.get_mut(rx, ry).floor = new_floor;
                            }
                        }
                    }
                }
            }
        }

        // Clear invalid overlays.
        for index in 0..len {
            let floor = tiles.geti(index).floor;
            let overlay = tiles.geti(index).overlay;
            let needs = content
                .block(overlay)
                .map(block_info::needs_surface)
                .unwrap_or(false);
            let surface = content
                .block(floor)
                .map(block_info::has_surface)
                .unwrap_or(false);
            if needs && !surface {
                tiles.geti_mut(index).overlay = ids.air;
            }
        }

        self.planet.base.decoration(tiles, content, 0.017);

        // `Schematics.placeLaunchLoadout`, `state.rules.*` and `Waves.generate`
        // are plan-11/12 hooks (no tile effect).
    }
}

/// Arc `Rand.random(int min, int max)` (inclusive).
fn rand_range_int(rand: &mut SimRng, min: i32, max: i32) -> i32 {
    min + rand.random(RngStream::MapGen, max - min + 1)
}

fn is_solid(content: &ContentRegistry, block: BlockId) -> bool {
    content.block(block).is_some_and(|def| def.solid)
}

fn is_static(content: &ContentRegistry, block: BlockId) -> bool {
    content.block(block).is_some_and(block_info::is_static)
}

fn steam_attr(content: &ContentRegistry, floor: BlockId) -> f32 {
    content
        .block(floor)
        .and_then(|def: &BlockDef| {
            def.attributes
                .iter()
                .find(|(name, _)| name == "steam")
                .map(|(_, value)| *value)
        })
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ContentRegistry {
        crate::content::test_support::test_registry()
    }

    #[test]
    fn erekir_ids_resolve() {
        let content = registry();
        let ids = Ids::load(&content);
        assert_ne!(ids.regolith, BlockId::AIR);
        assert_ne!(ids.yellow_stone, BlockId::AIR);
        assert_ne!(ids.rhyolite, BlockId::AIR);
        assert_ne!(ids.carbon_stone, BlockId::AIR);
        assert_ne!(ids.rhyolite_vent, BlockId::AIR);
    }

    #[test]
    fn erekir_has_terrain_and_vent() {
        let content = registry();
        let mut tiles = Tiles::new(128, 128);
        let mut generator = ErekirPlanetGenerator::new();
        generator.generate(&mut tiles, &WorldParams::default(), &content);

        let ids = Ids::load(&content);
        assert!(
            tiles.iter().any(|tile| tile.floor == ids.regolith
                || tile.floor == ids.crystalline_stone
                || tile.floor == ids.yellow_stone),
            "Erekir terrain present"
        );
        // Vents or crystals are seeded by the generator.
        let vent = tiles.iter().any(|tile| {
            tile.floor == ids.rhyolite_vent
                || tile.floor == ids.carbon_vent
                || tile.floor == ids.yellow_stone_vent
                || tile.floor == ids.arkyic_vent
        });
        let crystal = tiles
            .iter()
            .any(|tile| tile.block == ids.crystal_cluster || tile.block == ids.crystal_orbs);
        assert!(vent || crystal, "at least one vent or crystal cluster");
    }

    /// M8 golden (Rust-recorded, OD6-B): `world gen --planet erekir --seed 7
    /// --width 128 --height 128`.
    #[test]
    fn erekir_generation_checksum() {
        use crate::determinism::{Checksum, Hasher};
        let content = registry();
        let mut tiles = Tiles::new(128, 128);
        let mut generator = ErekirPlanetGenerator::new();
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
            "b8fd9327d1d0b300"
        );
    }
}
