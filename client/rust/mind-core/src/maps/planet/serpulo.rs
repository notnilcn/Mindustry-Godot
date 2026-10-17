// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `SerpuloPlanetGenerator` (plan 06 §3.11). Ported from
//! `maps/planet/SerpuloPlanetGenerator.java`.
//!
//! Deviations (OD6-A/OD6-B, structural parity): `state.rules.*`,
//! `Schematics.placeLaunchLoadout`, `Waves.generate`, `BaseGenerator` ruins and
//! `Planets.serpulo.sectors` reads are plan-11/12 hooks and are not applied
//! (they do not change the base terrain); `world.getDarkness` is zero until
//! plan 12's sector polygon lands; plan 02 lacks `Floor.liquidDrop`, so the
//! name-keyed `block_info::is_liquid` table stands in for `liquidDrop != null`.

use crate::content::{BlockDef, BlockId, ContentRegistry};
use crate::determinism::{RngStream, SimRng};
use crate::maps::filters::block_info;
use crate::math::{noise, ridged};
use crate::world::WorldParams;
use crate::world::cached::TileGen;
use crate::world::tiles::Tiles;

use crate::maps::generators::WorldGenerator;
use crate::maps::generators::astar;
use crate::maps::generators::base::BaseRand;
use crate::maps::generators::basic::{self, D4, GenNoise, planet_wall};
use crate::maps::generators::planet::{
    FlatSectorView, HexMesher, PlanetGenerator, SectorRect, SectorView, StructureCtx,
    place_launch_loadout,
};

/// The Serpulo floor matrix (`SerpuloPlanetGenerator.arr`), as content names.
const ARR_NAMES: [[&str; 13]; 13] = [
    [
        "shallow-water",
        "darksand-water",
        "darksand",
        "darksand",
        "darksand",
        "darksand",
        "sand-floor",
        "sand-floor",
        "sand-floor",
        "sand-floor",
        "darksand-tainted-water",
        "stone",
        "stone",
    ],
    [
        "shallow-water",
        "darksand-water",
        "darksand",
        "darksand",
        "sand-floor",
        "sand-floor",
        "sand-floor",
        "sand-floor",
        "sand-floor",
        "darksand-tainted-water",
        "stone",
        "stone",
        "stone",
    ],
    [
        "shallow-water",
        "darksand-water",
        "darksand",
        "sand-floor",
        "salt",
        "sand-floor",
        "sand-floor",
        "sand-floor",
        "sand-floor",
        "darksand-tainted-water",
        "stone",
        "stone",
        "stone",
    ],
    [
        "shallow-water",
        "sand-water",
        "sand-floor",
        "salt",
        "salt",
        "salt",
        "sand-floor",
        "stone",
        "stone",
        "stone",
        "snow",
        "ice-snow",
        "ice",
    ],
    [
        "deep-water",
        "shallow-water",
        "sand-water",
        "sand-floor",
        "salt",
        "sand-floor",
        "sand-floor",
        "basalt",
        "snow",
        "snow",
        "snow",
        "snow",
        "ice",
    ],
    [
        "deep-water",
        "shallow-water",
        "sand-water",
        "sand-floor",
        "sand-floor",
        "sand-floor",
        "moss",
        "ice-snow",
        "snow",
        "snow",
        "ice",
        "snow",
        "ice",
    ],
    [
        "deep-water",
        "sand-water",
        "sand-floor",
        "sand-floor",
        "moss",
        "moss",
        "snow",
        "basalt",
        "basalt",
        "basalt",
        "ice",
        "snow",
        "ice",
    ],
    [
        "deep-tainted-water",
        "darksand-tainted-water",
        "darksand",
        "darksand",
        "basalt",
        "moss",
        "basalt",
        "hotrock",
        "basalt",
        "ice",
        "snow",
        "ice",
        "ice",
    ],
    [
        "darksand-water",
        "darksand",
        "darksand",
        "darksand",
        "moss",
        "spore-moss",
        "snow",
        "basalt",
        "basalt",
        "ice",
        "snow",
        "ice",
        "ice",
    ],
    [
        "darksand-water",
        "darksand",
        "darksand",
        "spore-moss",
        "ice",
        "ice",
        "snow",
        "snow",
        "snow",
        "snow",
        "ice",
        "ice",
        "ice",
    ],
    [
        "deep-tainted-water",
        "darksand-tainted-water",
        "darksand",
        "spore-moss",
        "spore-moss",
        "ice",
        "ice",
        "snow",
        "snow",
        "ice",
        "ice",
        "ice",
        "ice",
    ],
    [
        "tainted-water",
        "darksand-tainted-water",
        "darksand",
        "spore-moss",
        "moss",
        "spore-moss",
        "ice-snow",
        "snow",
        "ice",
        "ice",
        "ice",
        "ice",
        "ice",
    ],
    [
        "darksand-water",
        "darksand",
        "snow",
        "ice",
        "ice-snow",
        "snow",
        "snow",
        "snow",
        "ice",
        "ice",
        "ice",
        "ice",
        "ice",
    ],
];

/// Resolved content ids used by the Serpulo generator.
#[derive(Debug, Clone, Copy)]
struct Ids {
    arr: [[BlockId; 13]; 13],
    spore_cluster: BlockId,
    shale: BlockId,
    tainted_water: BlockId,
    darksand_tainted_water: BlockId,
    shallow_water: BlockId,
    deep_water: BlockId,
    sand_water: BlockId,
    darksand_water: BlockId,
    sand: BlockId,
    salt: BlockId,
    snow: BlockId,
    ice: BlockId,
    ice_snow: BlockId,
    darksand: BlockId,
    moss: BlockId,
    spore_moss: BlockId,
    tar: BlockId,
    hotrock: BlockId,
    magmarock: BlockId,
    basalt: BlockId,
    dark_panel_3: BlockId,
    dark_panel_4: BlockId,
    dark_panel_6: BlockId,
    dark_metal: BlockId,
    metal_floor: BlockId,
    metal_floor_damaged: BlockId,
    snow_wall: BlockId,
    ice_wall: BlockId,
    white_tree: BlockId,
    white_tree_dead: BlockId,
    ore_copper: BlockId,
    ore_lead: BlockId,
    ore_coal: BlockId,
    ore_titanium: BlockId,
    ore_thorium: BlockId,
    ore_scrap: BlockId,
    spawn: BlockId,
}

impl Ids {
    fn load(content: &ContentRegistry) -> Self {
        let id = |name: &str| content.block_id(name).unwrap_or(BlockId::AIR);
        let mut arr = [[BlockId::AIR; 13]; 13];
        for (row, names) in ARR_NAMES.iter().enumerate() {
            for (col, name) in names.iter().enumerate() {
                arr[row][col] = id(name);
            }
        }
        Self {
            arr,
            spore_cluster: id("spore-cluster"),
            shale: id("shale"),
            tainted_water: id("tainted-water"),
            darksand_tainted_water: id("darksand-tainted-water"),
            shallow_water: id("shallow-water"),
            deep_water: id("deep-water"),
            sand_water: id("sand-water"),
            darksand_water: id("darksand-water"),
            sand: id("sand-floor"),
            salt: id("salt"),
            snow: id("snow"),
            ice: id("ice"),
            ice_snow: id("ice-snow"),
            darksand: id("darksand"),
            moss: id("moss"),
            spore_moss: id("spore-moss"),
            tar: id("tar"),
            hotrock: id("hotrock"),
            magmarock: id("magmarock"),
            basalt: id("basalt"),
            dark_panel_3: id("dark-panel-3"),
            dark_panel_4: id("dark-panel-4"),
            dark_panel_6: id("dark-panel-6"),
            dark_metal: id("dark-metal"),
            metal_floor: id("metal-floor"),
            metal_floor_damaged: id("metal-floor-damaged"),
            snow_wall: id("snow-wall"),
            ice_wall: id("ice-wall"),
            white_tree: id("white-tree"),
            white_tree_dead: id("white-tree-dead"),
            ore_copper: id("ore-copper"),
            ore_lead: id("ore-lead"),
            ore_coal: id("ore-coal"),
            ore_titanium: id("ore-titanium"),
            ore_thorium: id("ore-thorium"),
            ore_scrap: id("ore-scrap"),
            spawn: id("spawn"),
        }
    }

    fn dec(&self, floor: BlockId) -> Option<BlockId> {
        if floor == self.spore_moss || floor == self.moss {
            Some(self.spore_cluster)
        } else if floor == self.tainted_water {
            Some(self.shallow_water)
        } else if floor == self.darksand_tainted_water {
            Some(self.darksand_water)
        } else {
            None
        }
    }

    fn tar(&self, floor: BlockId) -> Option<BlockId> {
        if floor == self.spore_moss || floor == self.moss {
            Some(self.shale)
        } else {
            None
        }
    }
}

/// Serpulo's `noise` override: projected 3D simplex with plot scale `5`.
#[derive(Debug, Clone, Copy)]
struct SerpuloNoise {
    seed: i32,
    rect: SectorRect,
}

impl GenNoise for SerpuloNoise {
    fn noise(&self, x: f32, y: f32, octaves: f64, falloff: f64, scl: f64, mag: f64) -> f32 {
        let p = self.rect.project(x, y);
        noise::noise3d(
            self.seed,
            octaves as i32,
            falloff,
            1.0 / scl,
            (p[0] * 5.0) as f64,
            (p[1] * 5.0) as f64,
            (p[2] * 5.0) as f64,
        ) * mag as f32
    }
}

fn serpulo_noise_source(seed: i32, rect: SectorRect) -> Box<dyn GenNoise> {
    Box::new(SerpuloNoise { seed, rect })
}

/// One carved room (`SerpuloPlanetGenerator.Room`).
#[derive(Debug, Clone)]
struct Room {
    x: i32,
    y: i32,
    radius: i32,
    connected: Vec<usize>,
}

/// `SerpuloPlanetGenerator` (`maps/planet/SerpuloPlanetGenerator.java`).
#[derive(Debug)]
pub struct SerpuloPlanetGenerator {
    /// Shared planet helper.
    pub planet: PlanetGenerator,
    /// Sector view.
    pub sector: FlatSectorView,
    /// Player spawn (`spawn.x`, `spawn.y`).
    pub spawn: (i32, i32),
    /// Enemy spawn tiles.
    pub enemy_spawns: Vec<(i32, i32)>,
    /// Mega-base position (`basePos`).
    pub base_pos: [f32; 3],
    /// Height Y offset (`heightYOffset`).
    pub height_y_offset: f32,
    /// Plot scale (`scl`).
    pub scl: f32,
    /// Water offset (`waterOffset`).
    pub water_offset: f32,
    /// Height scale (`heightScl`).
    pub height_scl: f32,
    /// Whether the generated sector is predominantly water (`naval`; feeds
    /// `Waves.generate`'s naval flag in [`Self::generate_rules`]).
    pub naval: bool,
    /// Block color table for the hex-mesh `getColor` (built with the ids).
    colors: super::PlanetColorTable,
    /// Per-sector globe state (`getEmissiveColor` + the megabase patch).
    sector_glow: Vec<SectorGlow>,
    ids: Ids,
}

/// One sector's globe-glow state (`SerpuloPlanetGenerator.getEmissiveColor`
/// and the `getBlock(p, true)` megabase patch).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectorGlow {
    /// `Sector.tile.v` (unit-sphere position).
    pub v: [f32; 3],
    /// `Sector.hasEnemyBase()`.
    pub has_enemy_base: bool,
    /// `Sector.isCaptured()`.
    pub captured: bool,
    /// `Sector.hasBase()`.
    pub has_base: bool,
    /// Whether the sector has a preset (`preset != null`).
    pub has_preset: bool,
    /// `preset.difficulty` (0-10; ignored for presetless sectors).
    pub difficulty: f32,
    /// `SectorInfo.lightCoverage`.
    pub light_coverage: f32,
}

impl Default for SerpuloPlanetGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl SerpuloPlanetGenerator {
    /// Creates the generator (`baseSeed = 0`).
    pub fn new() -> Self {
        let mut planet = PlanetGenerator::new(0);
        planet.set_noise_factory(serpulo_noise_source);
        Self {
            planet,
            sector: FlatSectorView::default(),
            spawn: (0, 0),
            enemy_spawns: Vec::new(),
            base_pos: [0.9341721, 0.0, 0.3568221],
            height_y_offset: 42.7,
            scl: 5.0,
            water_offset: 0.04,
            height_scl: 1.01,
            naval: false,
            colors: super::PlanetColorTable::default(),
            sector_glow: Vec::new(),
            ids: Ids::load_empty(),
        }
    }

    /// Sets the per-sector globe state for `getEmissiveColor` and the megabase
    /// patch (`MindCampaign.planet_mesh` projects the live campaign).
    pub fn set_sector_glow(&mut self, sectors: Vec<SectorGlow>) {
        self.sector_glow = sectors;
    }

    /// Loads the content ids and the hex-mesh color table for
    /// `MindCampaign.planet_mesh` (`sprite_colors` is the
    /// `sprites/block_colors.png` row; empty when the pack output is absent).
    pub fn load_content(&mut self, content: &ContentRegistry, sprite_colors: &[u32]) {
        self.ids = Ids::load(content);
        self.colors = super::PlanetColorTable::load(content, sprite_colors);
    }

    /// `SerpuloPlanetGenerator.getBlock(p, true)` (`:193-231`): the generated
    /// floor plus the derelict-megabase metal/dark-panel patch near `basePos`
    /// (only outside the tar branch and within `0.65` of the megabase).
    fn visual_block(&self, position: [f32; 3]) -> BlockId {
        let (block, tarred) = get_block_split(&self.planet, position, &self.ids);
        let base_dst = distance(position, self.base_pos);
        if tarred || base_dst >= 0.65 {
            return block;
        }
        let mut dst = 999.0f32;
        for sector in &self.sector_glow {
            if sector.has_enemy_base {
                dst = dst.min(distance(position, sector.v));
            }
        }
        let freq = 0.05f32;
        let freq2 = 0.07f32;
        let [x, y, z] = position;
        if dst * 0.85
            + noise::noise3d(
                self.planet.seed,
                3,
                0.4,
                5.5,
                x as f64,
                (y + 200.0) as f64,
                z as f64,
            ) * 0.015
            + if (base_dst % freq) < freq / 2.0 {
                1.0
            } else {
                0.0
            } * 0.07
            < 0.15
        {
            return if (base_dst + 0.01) % freq2 < freq2 * 0.65 {
                self.ids.metal_floor
            } else {
                self.ids.dark_panel_6
            };
        }
        block
    }
}

impl Ids {
    fn load_empty() -> Self {
        let mut arr = [[BlockId::AIR; 13]; 13];
        for (row, names) in ARR_NAMES.iter().enumerate() {
            for (col, _) in names.iter().enumerate() {
                arr[row][col] = BlockId::AIR;
            }
        }
        Self {
            arr,
            spore_cluster: BlockId::AIR,
            shale: BlockId::AIR,
            tainted_water: BlockId::AIR,
            darksand_tainted_water: BlockId::AIR,
            shallow_water: BlockId::AIR,
            deep_water: BlockId::AIR,
            sand_water: BlockId::AIR,
            darksand_water: BlockId::AIR,
            sand: BlockId::AIR,
            salt: BlockId::AIR,
            snow: BlockId::AIR,
            ice: BlockId::AIR,
            ice_snow: BlockId::AIR,
            darksand: BlockId::AIR,
            moss: BlockId::AIR,
            spore_moss: BlockId::AIR,
            tar: BlockId::AIR,
            hotrock: BlockId::AIR,
            magmarock: BlockId::AIR,
            basalt: BlockId::AIR,
            dark_panel_3: BlockId::AIR,
            dark_panel_4: BlockId::AIR,
            dark_panel_6: BlockId::AIR,
            dark_metal: BlockId::AIR,
            metal_floor: BlockId::AIR,
            metal_floor_damaged: BlockId::AIR,
            snow_wall: BlockId::AIR,
            ice_wall: BlockId::AIR,
            white_tree: BlockId::AIR,
            white_tree_dead: BlockId::AIR,
            ore_copper: BlockId::AIR,
            ore_lead: BlockId::AIR,
            ore_coal: BlockId::AIR,
            ore_titanium: BlockId::AIR,
            ore_thorium: BlockId::AIR,
            ore_scrap: BlockId::AIR,
            spawn: BlockId::AIR,
        }
    }
}

/// `SerpuloPlanetGenerator.rawHeight`.
fn raw_height(
    seed: i32,
    position: [f32; 3],
    height_y_offset: f32,
    scl: f32,
    height_scl: f32,
) -> f32 {
    let n = noise::noise3d(
        seed,
        7,
        0.5,
        1.0 / 3.0,
        (position[0] * scl) as f64,
        (position[1] * scl + height_y_offset) as f64,
        (position[2] * scl) as f64,
    );
    ((n * height_scl).powf(2.3) + 0.04) / (1.0 + 0.04)
}

impl HexMesher for SerpuloPlanetGenerator {
    fn get_height(&self, position: [f32; 3]) -> f32 {
        let height = raw_height(
            self.planet.seed,
            position,
            self.height_y_offset,
            self.scl,
            self.height_scl,
        );
        height.max(2.0 / 13.0)
    }

    fn get_color(&self, position: [f32; 3]) -> [f32; 4] {
        // `SerpuloPlanetGenerator.getColor` (`:133-138`): `Block.mapColor` with
        // `1 - albedo` alpha (salt shown as sand; `getBlock(p, true)` includes
        // the megabase patch).
        let mut block = self.visual_block(position);
        if block == self.ids.salt {
            block = self.ids.sand;
        }
        self.colors.rgba(block)
    }

    fn get_emissive_color(&self, position: [f32; 3]) -> [f32; 4] {
        // `SerpuloPlanetGenerator.getEmissiveColor` (`:141-183`): enemy-base
        // crux glow and captured-sector sharded glow (`info.lightCoverage`).
        let mut dst = 999.0f32;
        let mut capture_dst = 999.0f32;
        let mut light_scl = 0.0f32;
        for sector in &self.sector_glow {
            if sector.has_enemy_base && !sector.captured {
                let offset = if sector.has_preset {
                    sector.difficulty / 10.0 * 0.03 - 0.03
                } else {
                    0.0
                };
                dst = dst.min(distance(position, sector.v) - offset);
            } else if sector.has_base {
                let cdst = distance(position, sector.v);
                if cdst < capture_dst {
                    capture_dst = cdst;
                    light_scl = sector.light_coverage;
                }
            }
        }
        light_scl = (light_scl / 50000.0).min(1.3);
        if light_scl < 1.0 {
            light_scl = crate::math::interp::Interp::Pow5Out.apply(light_scl);
        }
        let seed = self.planet.seed;
        let [x, y, z] = position;
        let base_dst = distance(position, self.base_pos);
        // `Team.crux` (f25555) / `Team.sharded` (ffd37f) colours.
        let crux = [
            0xf2 as f32 / 255.0,
            0x55 as f32 / 255.0,
            0x55 as f32 / 255.0,
        ];
        let sharded = [
            0xff as f32 / 255.0,
            0xd3 as f32 / 255.0,
            0x7f as f32 / 255.0,
        ];
        let metal_dst_scl = 0.25f32;
        let freq = 0.05f32;
        let enemy_glow = if base_dst < 0.3 {
            dst * metal_dst_scl
                + noise::noise3d(
                    seed + 1,
                    3,
                    0.4,
                    5.5,
                    x as f64,
                    (y + 200.0) as f64,
                    z as f64,
                ) * 0.08
                + if (base_dst % freq) < freq / 2.0 {
                    1.0
                } else {
                    0.0
                } * 0.07
                < 0.08
        } else {
            dst * metal_dst_scl
                + noise::noise3d(seed, 3, 0.4, 9.0, x as f64, (y + 370.0) as f64, z as f64) * 0.06
                < 0.045
        };
        if enemy_glow {
            let scale = 0.8
                + noise::noise3d(seed, 1, 1.0, 9.0, x as f64, (y + 99.0) as f64, z as f64) * 0.4;
            let mix =
                0.2 * noise::noise3d(seed, 1, 1.0, 9.0, x as f64, (y + 999.0) as f64, z as f64);
            blend(crux, sharded, mix, scale)
        } else if capture_dst * metal_dst_scl
            + noise::noise3d(seed, 3, 0.4, 9.0, x as f64, (y + 600.0) as f64, z as f64) * 0.07
            < 0.05 * light_scl
        {
            let scale = 0.7
                + noise::noise3d(seed, 1, 1.0, 9.0, x as f64, (y + 99.0) as f64, z as f64) * 0.4;
            let mix =
                0.3 * noise::noise3d(seed, 1, 1.0, 9.0, x as f64, (y + 999.0) as f64, z as f64);
            blend(sharded, crux, mix, scale)
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    fn is_emissive(&self) -> bool {
        true
    }

    fn reseed(&mut self) {
        // `MeshBuilder.buildHex`: `generator.seed = generator.baseSeed`.
        self.planet.seed = self.planet.base_seed;
    }
}

/// Euclidean distance between two points (`Vec3.dst`).
fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// `out.set(c1).mul(scale).lerp(c2, mix)` on rgb (alpha stays `1`).
fn blend(c1: [f32; 3], c2: [f32; 3], mix: f32, scale: f32) -> [f32; 4] {
    [
        (c1[0] + (c2[0] - c1[0]) * mix) * scale,
        (c1[1] + (c2[1] - c1[1]) * mix) * scale,
        (c1[2] + (c2[2] - c1[2]) * mix) * scale,
        1.0,
    ]
}

fn dst2(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x1 - x2;
    let dy = y1 - y2;
    dx * dx + dy * dy
}

fn limit2(v: [f32; 2], len: f32) -> [f32; 2] {
    let m = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if m > len && m > 0.0 {
        [v[0] / m * len, v[1] / m * len]
    } else {
        v
    }
}

/// `Room.join`.
fn room_join(
    generator: &mut SerpuloPlanetGenerator,
    tiles: &mut Tiles,
    content: &ContentRegistry,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) {
    let nscl = generator
        .planet
        .base
        .rand
        .range(RngStream::MapGen, 100.0, 140.0)
        * 6.0;
    let stroke = rand_range_int(&mut generator.planet.base.rand, 3, 9);
    let base = &generator.planet.base;
    let path = astar::pathfind(
        tiles,
        (x1, y1),
        (x2, y2),
        |_, to| {
            (if to.solid(content) { 50.0 } else { 0.0 })
                + base.noise_oct1(to.x as f32, to.y as f32, 2.0, 0.4, 1.0 / nscl as f64) * 500.0
        },
        |_| true,
    );
    generator.planet.base.brush(tiles, content, &path, stroke);
}

/// `Room.joinLiquid`.
fn room_join_liquid(
    generator: &mut SerpuloPlanetGenerator,
    tiles: &mut Tiles,
    content: &ContentRegistry,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) {
    let nscl = generator
        .planet
        .base
        .rand
        .range(RngStream::MapGen, 100.0, 140.0)
        * 6.0;
    let rad = rand_range_int(&mut generator.planet.base.rand, 7, 11);
    let avoid = 2 + rad;
    let base = &generator.planet.base;
    let path = astar::pathfind(
        tiles,
        (x1, y1),
        (x2, y2),
        |_, to| {
            let liquid = content
                .block(to.floor)
                .map(block_info::is_liquid)
                .unwrap_or(false);
            (if to.solid(content) || !liquid {
                70.0
            } else {
                0.0
            }) + base.noise_oct1(to.x as f32, to.y as f32, 2.0, 0.4, 1.0 / nscl as f64) * 500.0
        },
        |_| true,
    );

    let width = tiles.width;
    let height = tiles.height;
    for (tx, ty) in &path {
        if dst2(*tx as f32, *ty as f32, x2 as f32, y2 as f32) <= (avoid * avoid) as f32 {
            continue;
        }
        for x in -rad..=rad {
            for y in -rad..=rad {
                let wx = tx + x;
                let wy = ty + y;
                if wx < 0 || wy < 0 || wx >= width || wy >= height || !basic::within(x, y, rad) {
                    continue;
                }
                let (floor, _block) = {
                    let tile = tiles.get(wx, wy);
                    (tile.floor, tile.block)
                };
                let shallow = content
                    .block(floor)
                    .map(block_info::is_liquid)
                    .unwrap_or(false);
                tiles.get_mut(wx, wy).block = BlockId::AIR;
                if basic::within(x, y, rad - 1) && !shallow {
                    let new_floor = if floor == generator.ids.sand || floor == generator.ids.salt {
                        generator.ids.sand_water
                    } else {
                        generator.ids.darksand_tainted_water
                    };
                    tiles.get_mut(wx, wy).floor = new_floor;
                }
            }
        }
    }
}

/// `Room.connect`.
fn room_connect(
    generator: &mut SerpuloPlanetGenerator,
    tiles: &mut Tiles,
    content: &ContentRegistry,
    rooms: &mut [Room],
    from: usize,
    to: usize,
) {
    if from == to || rooms[from].connected.contains(&to) {
        return;
    }
    rooms[from].connected.push(to);

    let (fx, fy) = (rooms[from].x as f32, rooms[from].y as f32);
    let (tx, ty) = (rooms[to].x as f32, rooms[to].y as f32);
    let mut midpoint = [(tx + fx) * 0.5, (ty + fy) * 0.5];
    generator.planet.base.rand.next_float(RngStream::MapGen);
    // `setToRandomDirection(rand)`.
    let angle = generator
        .planet
        .base
        .rand
        .range(RngStream::MapGen, 0.0, 360.0);
    let dist = ((tx - fx).powi(2) + (ty - fy).powi(2)).sqrt();
    midpoint[0] += angle.to_radians().cos() * dist;
    midpoint[1] += angle.to_radians().sin() * dist;

    let width = tiles.width as f32;
    let height = tiles.height as f32;
    let limit = width / 2.0 / 3.0_f32.sqrt();
    let mut rel = [midpoint[0] - width / 2.0, midpoint[1] - height / 2.0];
    rel = limit2(rel, limit);
    midpoint = [rel[0] + width / 2.0, rel[1] + height / 2.0];

    let mx = midpoint[0] as i32;
    let my = midpoint[1] as i32;
    room_join(
        generator,
        tiles,
        content,
        rooms[from].x,
        rooms[from].y,
        mx,
        my,
    );
    room_join(generator, tiles, content, mx, my, rooms[to].x, rooms[to].y);
}

/// `Room.connectLiquid`.
fn room_connect_liquid(
    generator: &mut SerpuloPlanetGenerator,
    tiles: &mut Tiles,
    content: &ContentRegistry,
    rooms: &[Room],
    from: usize,
    to: usize,
) {
    if from == to {
        return;
    }
    let (fx, fy) = (rooms[from].x as f32, rooms[from].y as f32);
    let (tx, ty) = (rooms[to].x as f32, rooms[to].y as f32);
    let mut midpoint = [(tx + fx) * 0.5, (ty + fy) * 0.5];
    generator.planet.base.rand.next_float(RngStream::MapGen);
    let angle = generator
        .planet
        .base
        .rand
        .range(RngStream::MapGen, 0.0, 360.0);
    let dist = ((tx - fx).powi(2) + (ty - fy).powi(2)).sqrt();
    midpoint[0] += angle.to_radians().cos() * dist;
    midpoint[1] += angle.to_radians().sin() * dist;

    let width = tiles.width as f32;
    let height = tiles.height as f32;
    let limit = width / 2.0 / 3.0_f32.sqrt();
    let mut rel = [midpoint[0] - width / 2.0, midpoint[1] - height / 2.0];
    rel = limit2(rel, limit);
    midpoint = [rel[0] + width / 2.0, rel[1] + height / 2.0];

    let mx = midpoint[0] as i32;
    let my = midpoint[1] as i32;
    room_join_liquid(
        generator,
        tiles,
        content,
        rooms[from].x,
        rooms[from].y,
        mx,
        my,
    );
    room_join_liquid(generator, tiles, content, mx, my, rooms[to].x, rooms[to].y);
}

/// Arc `Rand.random(int min, int max)` (inclusive).
fn rand_range_int(rand: &mut SimRng, min: i32, max: i32) -> i32 {
    min + rand.random(RngStream::MapGen, max - min + 1)
}

impl SerpuloPlanetGenerator {
    /// Derelict ruins (`SerpuloPlanetGenerator.java:633-711`): 0-4 derelict
    /// base parts near ores with scrap ore around them and debris damage.
    /// Runs on the same `rand` state as the decoration pass (nothing draws from
    /// `planet.base.rand` between it and the end of `generate`).
    fn place_ruins(&mut self, ctx: &mut StructureCtx) {
        use crate::ai::base_registry::BaseResource;
        use crate::maps::generators::base::{get_frac, try_place};

        let Some(bases) = ctx.bases else {
            return;
        };
        let width = ctx.sim.grid.tiles.width;
        let height = ctx.sim.grid.tiles.height;
        let (spawn_x, spawn_y) = self.spawn;
        let difficulty = self.sector.threat;
        let rand = &mut self.planet.base.rand;

        // `rand.random(-2, 4)`.
        let ruin_count = rand_range_int(rand, -2, 4);
        if ruin_count <= 0 {
            return;
        }

        let padding = 25;
        let mut ints: Vec<(i32, i32)> = Vec::new();
        for x in padding..width - padding {
            for y in padding..height - padding {
                let Some(tile) = ctx.sim.grid.tiles.getn(x, y) else {
                    continue;
                };
                let solid = ctx
                    .content
                    .block(tile.block)
                    .map(|def| def.solid)
                    .unwrap_or(false);
                let overlay_drop = ctx
                    .content
                    .block(tile.overlay)
                    .and_then(|def| def.item_drop);
                let floor_liquid = ctx
                    .content
                    .block(tile.floor)
                    .and_then(|def| def.liquid_drop);
                if !solid && (overlay_drop.is_some() || floor_liquid.is_some()) {
                    ints.push((x, y));
                }
            }
        }

        // `IntSeq.shuffle(rand)`: `for i = size-1 downTo 0`.
        for i in (0..ints.len()).rev() {
            let ii = rand.random(RngStream::MapGen, i as i32 + 1) as usize;
            ints.swap(i, ii);
        }

        let ore_scrap = ctx.content.block_id("ore-scrap");
        let mut placed = 0;
        for &(x, y) in &ints {
            if placed >= ruin_count {
                break;
            }
            // Do not overwrite the player spawn (`Mathf.within(... 18f)`).
            let dx = x - spawn_x;
            let dy = y - spawn_y;
            if dx * dx + dy * dy < 18 * 18 {
                continue;
            }

            let range = difficulty + rand.next_float(RngStream::MapGen) * 0.4;

            let Some(tile) = ctx.sim.grid.tiles.getn(x, y) else {
                continue;
            };
            let overlay_drop = ctx
                .content
                .block(tile.overlay)
                .and_then(|def| def.item_drop);
            let floor_liquid = ctx
                .content
                .block(tile.floor)
                .and_then(|def| def.liquid_drop);
            let part = if let Some(drop) = overlay_drop {
                // `tile.drop()`: the overlay's drop wins over the floor's.
                let resource = drop;
                get_frac(bases.for_resource(BaseResource::Item(resource)), range)
            } else if floor_liquid.is_some() && rand.chance(RngStream::MapGen, 0.05) {
                let liquid = floor_liquid.unwrap_or_default();
                get_frac(bases.for_resource(BaseResource::Liquid(liquid)), range)
            } else if rand.chance(RngStream::MapGen, 0.05) {
                get_frac(&bases.parts, range)
            } else {
                None
            };
            let Some(part) = part else {
                continue;
            };

            // The ore-scrap callback (`posc`) writes overlays and draws the
            // d8 chances inline, in Java order.
            let mut posc = |wctx: &mut crate::world::ops::WorldCtx<'_>,
                            rand: &mut dyn BaseRand,
                            cx: i32,
                            cy: i32| {
                let floor = {
                    let Some(other) = wctx.grid.tiles.getn(cx, cy) else {
                        return;
                    };
                    other.floor
                };
                let surface = wctx
                    .content
                    .block(floor)
                    .map(block_info::has_surface)
                    .unwrap_or(false);
                if !surface {
                    return;
                }
                if let Some(ore_scrap) = ore_scrap {
                    set_overlay(wctx, cx, cy, ore_scrap);
                }
                for j in 1..=2 {
                    for (ddx, ddy) in basic::D8 {
                        let tx = cx + ddx * j;
                        let ty = cy + ddy * j;
                        if !wctx.grid.tiles.in_bounds(tx, ty) {
                            continue;
                        }
                        let floor = wctx.grid.tile(tx, ty).floor;
                        let surface = wctx
                            .content
                            .block(floor)
                            .map(block_info::has_surface)
                            .unwrap_or(false);
                        if surface
                            && rand.chance(if j == 1 { 0.4 } else { 0.2 })
                            && let Some(ore_scrap) = ore_scrap
                        {
                            set_overlay(wctx, tx, ty, ore_scrap);
                        }
                    }
                }
            };
            let success = try_place(ctx, part, x, y, 0, rand, Some(&mut posc));
            if !success {
                continue;
            }
            placed += 1;
            log::info!(
                "ruins: derelict part placed at ({x}, {y}) [{} total]",
                placed
            );

            // Debris: damage or remove derelict buildings in the circle.
            let debris_radius = part.schematic.width.max(part.schematic.height) / 2 + 3;
            basic::circle(x, y, width, height, debris_radius, |cx, cy| {
                let dst = (((cx - x) * (cx - x) + (cy - y) * (cy - y)) as f32).sqrt();
                let remove_chance = 0.05 + (0.5 - 0.05) * (dst / debris_radius as f32);
                let Some(entity) = ctx.sim.grid.tiles.getn(cx, cy).and_then(|tile| tile.build)
                else {
                    return;
                };
                let center_pos = crate::world::TilePos::new(cx as i16, cy as i16);
                let Some(comp) = ctx.sim.ecs.0.get::<crate::ecs::BuildingComp>(entity) else {
                    return;
                };
                if comp.pos != center_pos {
                    return;
                }
                let team = comp.team.0;
                if team == 0 && rand.chance(RngStream::MapGen, remove_chance as f64) {
                    if let (Ok(cx), Ok(cy)) = (i16::try_from(cx), i16::try_from(cy)) {
                        ctx.sim.with_world_ctx(|wctx| wctx.remove_block(cx, cy));
                    }
                } else if rand.chance(RngStream::MapGen, 0.5) {
                    let health = ctx
                        .sim
                        .ecs
                        .0
                        .get::<crate::entities::comp::Health>(entity)
                        .map(|health| health.health)
                        .unwrap_or(0.0);
                    let damage = rand.next_float(RngStream::MapGen) * (health * 0.9);
                    if let Some(mut health) = ctx
                        .sim
                        .ecs
                        .0
                        .get_mut::<crate::entities::comp::Health>(entity)
                    {
                        health.health -= damage;
                    }
                }
            });
        }
    }
}

/// `WorldCtx.set_overlay` shorthand for the ruins callback.
fn set_overlay(
    wctx: &mut crate::world::ops::WorldCtx<'_>,
    x: i32,
    y: i32,
    overlay: crate::content::BlockId,
) {
    let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) else {
        return;
    };
    wctx.set_overlay(x, y, overlay);
}

impl WorldGenerator for SerpuloPlanetGenerator {
    fn name(&self) -> &'static str {
        "serpulo"
    }

    fn set_sector(&mut self, view: FlatSectorView) {
        self.sector = view;
    }

    fn sector_size_scl(&self) -> Option<f32> {
        Some(self.planet.size_scl)
    }

    fn player_spawn(&self) -> Option<(i32, i32)> {
        Some(self.spawn)
    }

    /// `Schematics.placeLaunchLoadout` runs inside generation (H05 T4/T6):
    /// derelict ruins first (`SerpuloPlanetGenerator.java:633-711`), then the
    /// launch loadout (`:719`), then the enemy base (`:721-731`, I04 T17).
    fn place_structures(&mut self, ctx: &mut StructureCtx) {
        self.place_ruins(ctx);
        place_launch_loadout(ctx, self.spawn.0, self.spawn.1);

        // Enemy base (`BaseGenerator`; `SerpuloPlanetGenerator.java:721-731`).
        // The enemy-room spawn overlays ran in `generate` (`:721-723`);
        // `sector.info.attack` is persisted by `SectorInfo.write` once the wave
        // team holds a core.
        if self.sector.has_enemy_base() && ctx.bases.is_some() {
            use crate::game::waves::Waves;
            use crate::math::rand_arc::ArcRand;

            let cores = self.enemy_spawns.clone();
            let state = crate::maps::generators::base::generate(
                ctx,
                &cores,
                self.spawn,
                ctx.wave_team,
                self.sector.id,
                self.sector.threat,
            );
            // `state.rules.attackMode = sector.info.attack = true;`
            ctx.rules.attack_mode = true;
            crate::maps::generators::base::post_generate(ctx, &state);

            // `postGenerate` (`:745-752`): no ground spawns left -> regenerate
            // the wave table air-only.
            let spawn_block = self.ids.spawn;
            let ground_spawns = ctx
                .sim
                .grid
                .tiles
                .iter()
                .filter(|tile| tile.overlay == spawn_block)
                .count();
            if ground_spawns == 0 {
                let attack = ctx.rules.attack_mode;
                let runtime = Waves::generate_with(
                    self.sector.threat,
                    &mut ArcRand::new(self.sector.id as u64),
                    attack,
                    true,
                    false,
                );
                ctx.rules.spawns = runtime.iter().map(spawn_group_to_json).collect();
            }
        }
    }

    fn generate(&mut self, tiles: &mut Tiles, params: &WorldParams, content: &ContentRegistry) {
        let ids = Ids::load(content);
        self.ids = ids;
        let seed = self.planet.seed_formula(params.seed_offset);

        // `genTile` loop.
        self.planet.run(
            tiles,
            params,
            &self.sector,
            |planet, position, brush: &mut TileGen| {
                let mut floor = get_block_with(planet, position, &ids);
                if floor == ids.dark_panel_6 {
                    floor = ids.dark_panel_3;
                }
                brush.floor = floor;
                brush.block = planet_wall(content, floor);
                if ridged::noise3d(
                    planet.seed + 1,
                    position[0] as f64,
                    position[1] as f64,
                    position[2] as f64,
                    2,
                    22.0,
                ) > 0.31
                {
                    brush.block = BlockId::AIR;
                }
            },
        );

        let width = tiles.width;
        let height = tiles.height;

        // Room graph.
        self.planet.base.cells(tiles, content, 4, 16, 16, 3);
        self.planet.base.distort(tiles, content, 10.0, 12.0);

        let constraint = 1.3_f32;
        let radius = width as f32 / 2.0 / 3.0_f32.sqrt();
        let room_count = rand_range_int(&mut self.planet.base.rand, 2, 5);
        let mut rooms: Vec<Room> = Vec::new();
        for _ in 0..room_count {
            let angle = self.planet.base.rand.range(RngStream::MapGen, 0.0, 360.0);
            let dist = self
                .planet
                .base
                .rand
                .range(RngStream::MapGen, 0.0, radius / constraint);
            let rx = width as f32 / 2.0 + angle.to_radians().cos() * dist;
            let ry = height as f32 / 2.0 + angle.to_radians().sin() * dist;
            let maxrad = radius - dist;
            let rrad = self
                .planet
                .base
                .rand
                .range(RngStream::MapGen, 9.0, maxrad / 2.0)
                .min(30.0);
            rooms.push(Room {
                x: rx as i32,
                y: ry as i32,
                radius: rrad as i32,
                connected: Vec::new(),
            });
        }

        // Spawn selection.
        let mut spawn_index: Option<usize> = None;
        let mut enemy_indices: Vec<usize> = Vec::new();
        let enemy_spawns = rand_range_int(
            &mut self.planet.base.rand,
            1,
            ((self.sector.threat * 4.0) as i32).max(1),
        );
        let offset = self.planet.base.rand.random(RngStream::MapGen, 360);
        let length =
            width as f32 / 2.55 - self.planet.base.rand.range(RngStream::MapGen, 13.0, 23.0);
        let angle_step = 5;
        let water_check_rad = 5;
        let mut i = 0;
        while i < 360 {
            let angle = (offset + i) as f32;
            let cx = (width as f32 / 2.0 + angle.to_radians().cos() * length) as i32;
            let cy = (height as f32 / 2.0 + angle.to_radians().sin() * length) as i32;

            let mut water_tiles = 0;
            for rx in -water_check_rad..=water_check_rad {
                for ry in -water_check_rad..=water_check_rad {
                    match tiles.getn(cx + rx, cy + ry) {
                        Some(tile) => {
                            if floor_is_liquid(content, tile.floor) {
                                water_tiles += 1;
                            }
                        }
                        None => water_tiles += 1,
                    }
                }
            }

            if water_tiles <= 4 || i + angle_step >= 360 {
                let spawn_radius = rand_range_int(&mut self.planet.base.rand, 8, 15);
                rooms.push(Room {
                    x: cx,
                    y: cy,
                    radius: spawn_radius,
                    connected: Vec::new(),
                });
                spawn_index = Some(rooms.len() - 1);
                for _ in 0..enemy_spawns {
                    let enemy_offset = self.planet.base.rand.range(RngStream::MapGen, 0.0, 60.0);
                    let rot = (180.0 + enemy_offset).to_radians();
                    let ex = (cx as f32 - width as f32 / 2.0) * rot.cos()
                        - (cy as f32 - height as f32 / 2.0) * rot.sin()
                        + width as f32 / 2.0;
                    let ey = (cx as f32 - width as f32 / 2.0) * rot.sin()
                        + (cy as f32 - height as f32 / 2.0) * rot.cos()
                        + height as f32 / 2.0;
                    let er = rand_range_int(&mut self.planet.base.rand, 8, 16);
                    rooms.push(Room {
                        x: ex as i32,
                        y: ey as i32,
                        radius: er,
                        connected: Vec::new(),
                    });
                    enemy_indices.push(rooms.len() - 1);
                }
                break;
            }
            i += angle_step;
        }
        let Some(spawn_index) = spawn_index else {
            return;
        };
        self.spawn = (rooms[spawn_index].x, rooms[spawn_index].y);
        self.enemy_spawns = enemy_indices
            .iter()
            .map(|&index| (rooms[index].x, rooms[index].y))
            .collect();

        // Clear room radii.
        for room in &rooms {
            self.planet
                .base
                .erase(tiles, content, room.x, room.y, room.radius);
        }

        // Random connections.
        let connections = rand_range_int(
            &mut self.planet.base.rand,
            (room_count - 1).max(1),
            room_count + 3,
        );
        for _ in 0..connections {
            let a = self
                .planet
                .base
                .rand
                .random(RngStream::MapGen, rooms.len() as i32) as usize;
            let b = self
                .planet
                .base
                .rand
                .random(RngStream::MapGen, rooms.len() as i32) as usize;
            room_connect(self, tiles, content, &mut rooms, a, b);
        }
        for index in 0..rooms.len() {
            room_connect(self, tiles, content, &mut rooms, spawn_index, index);
        }

        self.planet.base.cells(tiles, content, 1, 16, 16, 3);

        // Naval determination.
        let mut total = 0u64;
        let mut waters = 0u64;
        for index in 0..tiles.len() {
            let tile = tiles.geti(index);
            if tile.block == BlockId::AIR {
                total += 1;
                if floor_is_water(content, tile.floor) {
                    waters += 1;
                }
            }
        }
        let naval = total > 0 && (waters as f32 / total as f32) >= 0.19;
        self.naval = naval;

        if naval {
            for &enemy in &enemy_indices {
                room_connect_liquid(self, tiles, content, &rooms, enemy, spawn_index);
            }
        }

        self.planet.base.distort(tiles, content, 10.0, 6.0);

        // Rivers.
        let spawn = self.spawn;
        let sector_id = self.sector.id as i32;
        let rect = self.sector.rect();
        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let mut floor = tiles.geti(index).floor;
            let block_solid = tiles.geti(index).solid(content);
            if block_solid {
                continue;
            }
            let v = rect.project(x as f32, y as f32);
            let rr = noise::noise2d(sector_id, 2, 0.6, 1.0 / 7.0, x as f64, y as f64) * 0.1;
            let value =
                ridged::noise3d(2, v[0] as f64, v[1] as f64, v[2] as f64, 1, 1.0 / 55.0) + rr;
            let rrscl = rr * 44.0 - 2.0;
            if value > 0.17 && !basic::within_f(x, y, spawn.0, spawn.1, 12.0 + rrscl) {
                let deep = value > 0.27 && !basic::within_f(x, y, spawn.0, spawn.1, 15.0 + rrscl);
                let spore = floor != self.ids.sand && floor != self.ids.salt;
                let liquid = floor_is_liquid(content, floor);
                let frozen =
                    floor == self.ids.ice || floor == self.ids.ice_snow || floor == self.ids.snow;
                if !frozen && !liquid {
                    floor = if spore {
                        if deep {
                            self.ids.tainted_water
                        } else {
                            self.ids.darksand_tainted_water
                        }
                    } else if deep {
                        self.ids.shallow_water
                    } else if floor == self.ids.sand || floor == self.ids.salt {
                        self.ids.sand_water
                    } else {
                        self.ids.darksand_water
                    };
                }
            }
            tiles.geti_mut(index).floor = floor;
        }

        // Shoreline setup.
        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let floor = tiles.geti(index).floor;
            if floor_is_liquid(content, floor) && floor_is_shallow(content, floor) {
                let deep_radius = 3;
                let mut skip = false;
                for cx in -deep_radius..=deep_radius {
                    for cy in -deep_radius..=deep_radius {
                        if cx * cx + cy * cy <= deep_radius * deep_radius {
                            match tiles.getn(x + cx, y + cy) {
                                Some(other) => {
                                    if !floor_is_liquid(content, other.floor)
                                        || other.block != BlockId::AIR
                                    {
                                        skip = true;
                                    }
                                }
                                None => skip = true,
                            }
                        }
                    }
                }
                if !skip {
                    let new_floor = if floor == self.ids.darksand_tainted_water {
                        self.ids.tainted_water
                    } else {
                        self.ids.shallow_water
                    };
                    tiles.geti_mut(index).floor = new_floor;
                }
            }
        }

        if naval {
            let deep_radius = 2;
            for index in 0..tiles.len() {
                let (x, y) = {
                    let tile = tiles.geti(index);
                    (tile.x as i32, tile.y as i32)
                };
                let floor = tiles.geti(index).floor;
                let deep = content
                    .block(floor)
                    .map(block_info::is_deep)
                    .unwrap_or(false);
                if floor_is_liquid(content, floor) && !deep && !floor_is_shallow(content, floor) {
                    let mut skip = false;
                    for cx in -deep_radius..=deep_radius {
                        for cy in -deep_radius..=deep_radius {
                            if cx * cx + cy * cy <= deep_radius * deep_radius {
                                match tiles.getn(x + cx, y + cy) {
                                    Some(other) => {
                                        if floor_is_shallow(content, other.floor)
                                            || !floor_is_liquid(content, other.floor)
                                        {
                                            skip = true;
                                        }
                                    }
                                    None => skip = true,
                                }
                            }
                        }
                    }
                    if !skip {
                        let new_floor = if floor == self.ids.shallow_water {
                            self.ids.deep_water
                        } else {
                            self.ids.tainted_water
                        };
                        tiles.geti_mut(index).floor = new_floor;
                    }
                }
            }
        }

        // Ores.
        let mut ores = vec![self.ids.ore_copper, self.ids.ore_lead];
        let poles = self.sector.tile_v[1].abs();
        let nmag = 0.5_f32;
        let oscl = 1.0_f32;
        let addscl = 1.3_f32;
        let tv = self.sector.tile_v;
        if noise::noise3d(
            seed,
            2,
            0.5,
            oscl as f64,
            tv[0] as f64,
            tv[1] as f64,
            tv[2] as f64,
        ) * nmag
            + poles
            > 0.25 * addscl
        {
            ores.push(self.ids.ore_coal);
        }
        if noise::noise3d(
            seed,
            2,
            0.5,
            oscl as f64,
            (tv[0] + 1.0) as f64,
            tv[1] as f64,
            tv[2] as f64,
        ) * nmag
            + poles
            > 0.5 * addscl
        {
            ores.push(self.ids.ore_titanium);
        }
        if noise::noise3d(
            seed,
            2,
            0.5,
            oscl as f64,
            (tv[0] + 2.0) as f64,
            tv[1] as f64,
            tv[2] as f64,
        ) * nmag
            + poles
            > 0.7 * addscl
            && self.sector.id != 218
        {
            ores.push(self.ids.ore_thorium);
        }
        if self.planet.base.rand.chance(RngStream::MapGen, 0.25) {
            ores.push(self.ids.ore_scrap);
        }
        let mut frequencies = Vec::with_capacity(ores.len());
        for i in 0..ores.len() {
            frequencies.push(
                self.planet.base.rand.range(RngStream::MapGen, -0.1, 0.01) - i as f32 * 0.01
                    + poles * 0.04,
            );
        }

        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let floor = tiles.geti(index).floor;
            let mut ore = tiles.geti(index).overlay;
            if !content
                .block(floor)
                .map(block_info::has_surface)
                .unwrap_or(false)
            {
                continue;
            }
            let offset_x = x - 4;
            let offset_y = y + 23;
            for i in (0..ores.len()).rev() {
                let i = i as i32;
                let a = self.planet.base.noise_oct1(
                    offset_x as f32,
                    (offset_y + i * 999) as f32,
                    2.0,
                    0.7,
                    (40 + i * 2) as f64,
                );
                let b = self.planet.base.noise_oct1(
                    offset_x as f32,
                    (offset_y - i * 999) as f32,
                    1.0,
                    1.0,
                    (30 + i * 4) as f64,
                );
                if (a - 0.5).abs() > 0.22 + i as f32 * 0.01
                    && (b - 0.5).abs() > 0.37 + frequencies[i as usize]
                {
                    ore = ores[i as usize];
                    break;
                }
            }
            let mut floor = floor;
            if ore == self.ids.ore_scrap && self.planet.base.rand.chance(RngStream::MapGen, 0.33) {
                floor = self.ids.metal_floor_damaged;
            }
            let tile = tiles.geti_mut(index);
            tile.floor = floor;
            tile.overlay = ore;
        }

        self.planet.base.trim_dark(tiles, content, |_, _| 0.0);
        self.planet.base.median(tiles, content, 2, 0.5);
        self.planet
            .base
            .inverse_flood_fill(tiles, content, (spawn.0, spawn.1));
        self.planet.base.tech(
            tiles,
            content,
            self.ids.dark_panel_3,
            self.ids.dark_panel_4,
            self.ids.dark_metal,
        );

        // Moss / tar / hotrock / trees / decorations.
        let rooms_ref = rooms.clone();
        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let mut floor = tiles.geti(index).floor;
            let mut block = tiles.geti(index).block;

            if floor == self.ids.spore_moss {
                let n = self
                    .planet
                    .base
                    .noise_oct1((x - 90) as f32, y as f32, 4.0, 0.8, 65.0);
                if (0.5 - n).abs() > 0.02 {
                    floor = self.ids.moss;
                }
            }

            if floor == self.ids.darksand {
                let a = self
                    .planet
                    .base
                    .noise_oct1((x - 40) as f32, y as f32, 2.0, 0.7, 80.0);
                let b = self.planet.base.noise_oct1(
                    x as f32,
                    y as f32 + sector_id as f32 * 10.0,
                    1.0,
                    1.0,
                    60.0,
                );
                let near_room = rooms_ref
                    .iter()
                    .any(|r| basic::within_f(x, y, r.x, r.y, 30.0));
                if (0.5 - a).abs() > 0.25 && (0.5 - b).abs() > 0.41 && !near_room {
                    floor = self.ids.tar;
                }
            }

            if floor == self.ids.hotrock {
                let n = self
                    .planet
                    .base
                    .noise_oct1((x - 90) as f32, y as f32, 4.0, 0.8, 80.0);
                if (0.5 - n).abs() > 0.035 {
                    floor = self.ids.basalt;
                } else {
                    let mut all = true;
                    for (dx, dy) in D4 {
                        match tiles.getn(x + dx, y + dy) {
                            Some(other) => {
                                if other.floor != self.ids.hotrock
                                    && other.floor != self.ids.magmarock
                                {
                                    all = false;
                                }
                            }
                            None => all = false,
                        }
                    }
                    if all {
                        floor = self.ids.magmarock;
                    }
                }
            }

            if self.planet.base.rand.chance(RngStream::MapGen, 0.0075) {
                let mut any = false;
                let mut all = true;
                for (dx, dy) in D4 {
                    match tiles.getn(x + dx, y + dy) {
                        Some(other) => {
                            if other.block == BlockId::AIR {
                                any = true;
                            } else {
                                all = false;
                            }
                        }
                        None => all = false,
                    }
                }
                let tree_ok = (block == self.ids.snow_wall || block == self.ids.ice_wall)
                    || (all
                        && block == BlockId::AIR
                        && floor == self.ids.snow
                        && self.planet.base.rand.chance(RngStream::MapGen, 0.03));
                if any && tree_ok {
                    block = if self.planet.base.rand.chance(RngStream::MapGen, 0.5) {
                        self.ids.white_tree
                    } else {
                        self.ids.white_tree_dead
                    };
                }
            }

            // Decorations.
            let mut blocked = false;
            for (dx, dy) in D4 {
                if tiles
                    .getn(x + dx, y + dy)
                    .is_some_and(|other| other.block != BlockId::AIR)
                {
                    blocked = true;
                }
            }
            if !blocked
                && self.planet.base.rand.chance(RngStream::MapGen, 0.01)
                && content
                    .block(floor)
                    .map(block_info::has_surface)
                    .unwrap_or(false)
                && block == BlockId::AIR
                && let Some(def) = content.block(floor)
            {
                // `block = dec.get(floor, floor.asFloor().decoration)`
                // (`SerpuloPlanetGenerator.java:628`): the `dec` map first
                // (spore moss / moss -> spore cluster), else the floor's
                // explicit decoration (L02 T1; `Blocks.air` when none).
                block = ids.dec(floor).unwrap_or(block_info::decoration(def));
            }

            let tile = tiles.geti_mut(index);
            tile.floor = floor;
            tile.block = block;
        }

        // Ruins (`basegen.generate`) are skipped: the plan-11 `BaseRegistry` is
        // empty in mind-core, and upstream places nothing without loaded parts.

        // Remove invalid ores.
        for index in 0..tiles.len() {
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
                tiles.geti_mut(index).overlay = BlockId::AIR;
            }
        }

        // `Schematics.placeLaunchLoadout` is a plan-12 hook (no tile effect).
        for &(ex, ey) in &self.enemy_spawns {
            if tiles.in_bounds(ex, ey) {
                tiles.get_mut(ex, ey).overlay = self.ids.spawn;
            }
        }

        // `basegen.generate` (ruins) stays a named residual: placing schematics
        // needs the plan-07 `WorldCtx`/ECS, which `WorldGenerator::generate`
        // (tiles only) cannot carry; `postGenerate`'s wall fixes land with it.
    }

    /// `state.rules.*` + `Waves.generate` half of `SerpuloPlanetGenerator.generate`
    /// (plan 06 §3.10 R3). Ported from the tail of
    /// `SerpuloPlanetGenerator.java`: the caller owns the runtime `Rules`, so the
    /// generator writes it explicitly instead of mutating `state.rules`.
    fn generate_rules(
        &mut self,
        rules: &mut crate::game::rules::Rules,
        _params: &WorldParams,
        _content: &ContentRegistry,
    ) {
        use crate::game::waves::Waves;
        use crate::math::rand_arc::ArcRand;

        let difficulty = self.sector.threat;
        let attack = self.sector.has_enemy_base();

        rules.waves = true;
        // `Mathf.lerp(60*65*2, 60*60, max(difficulty - 0.4, 0))`.
        let dec = (difficulty - 0.4).max(0.0);
        rules.wave_spacing = 60.0 * 65.0 * 2.0 + (60.0 * 60.0 - 60.0 * 65.0 * 2.0) * dec;
        rules.enemy_core_build_radius = 600.0;

        if attack {
            rules.attack_mode = true;
        } else {
            rules.win_wave = 10 + 5 * ((difficulty * 10.0) as i32).max(1);
        }

        // Upstream `spawner.countGroundSpawns() == 0` on a fresh generation, so
        // `airOnly == attack`.
        let runtime = Waves::generate_with(
            difficulty,
            &mut ArcRand::new(self.sector.id as u64),
            attack,
            attack,
            self.naval,
        );
        rules.spawns = runtime.iter().map(spawn_group_to_json).collect();
    }
}

/// Projects the runtime wave [`crate::game::spawn_group::SpawnGroup`] onto the
/// persisted `Rules.spawns` shape (plan 04 §6.7 upstream camelCase ABI).
fn spawn_group_to_json(
    group: &crate::game::spawn_group::SpawnGroup,
) -> crate::io::json::rules::SpawnGroup {
    crate::io::json::rules::SpawnGroup {
        type_: group.unit.clone(),
        begin: group.begin,
        end: group.end,
        spacing: group.spacing,
        max: group.max,
        unit_scaling: group.unit_scaling,
        shields: group.shields,
        shield_scaling: group.shield_scaling,
        unit_amount: group.unit_amount,
        effect: group.effect.clone(),
        spawn: group.spawn,
        payloads: group.payloads.clone(),
        items: group
            .items
            .as_ref()
            .map(|stack| crate::io::json::JsonItemStack {
                item: Some(stack.item.clone()),
                amount: stack.amount,
            }),
        team: group.team,
    }
}

/// `getBlock` against an explicit id table (shared by `genTile` and `getBlock`).
fn get_block_with(planet: &PlanetGenerator, position: [f32; 3], ids: &Ids) -> BlockId {
    get_block_split(planet, position, ids).0
}

/// The `getBlock(p, false)` lookup plus whether the tar branch applied (the
/// `getBlock(p, true)` megabase patch runs only outside it).
fn get_block_split(planet: &PlanetGenerator, position: [f32; 3], ids: &Ids) -> (BlockId, bool) {
    let seed = planet.seed;
    let scl = 5.0_f32;
    let height_y_offset = 42.7_f32;
    let height_scl = 1.01_f32;

    let mut height = raw_height(seed, position, height_y_offset, scl, height_scl);
    let px = position[0] * scl;
    let py = position[1] * scl;
    let pz = position[2] * scl;

    let rad = scl;
    let mut temp = (py.abs() * 2.0 / rad).clamp(0.0, 1.0);
    let tnoise = noise::noise3d(
        seed,
        7,
        0.56,
        1.0 / 3.0,
        px as f64,
        (py + 999.0 - 0.1) as f64,
        pz as f64,
    );
    temp = temp + (tnoise - temp) * 0.5;
    height *= 1.2;
    height = height.clamp(0.0, 1.0);

    let dst3 = (px * px + py * py + (pz - 1.0) * (pz - 1.0)).sqrt();
    let tar = noise::noise3d(
        seed,
        4,
        0.55,
        0.5,
        px as f64,
        (py + 999.0) as f64,
        pz as f64,
    ) * 0.3
        + dst3 * 0.2;

    let row = ((temp * 13.0) as i32).clamp(0, 12) as usize;
    let col = ((height * 13.0) as i32).clamp(0, 12) as usize;
    let res = ids.arr[row][col];
    if tar > 0.5 {
        (ids.tar(res).unwrap_or(res), true)
    } else {
        (res, false)
    }
}

fn floor_is_liquid(content: &ContentRegistry, floor: BlockId) -> bool {
    content
        .block(floor)
        .map(block_info::is_liquid)
        .unwrap_or(false)
}

fn floor_is_shallow(content: &ContentRegistry, floor: BlockId) -> bool {
    content
        .block(floor)
        .map(|def: &BlockDef| def.kind == crate::content::BlockKind::ShallowLiquid)
        .unwrap_or(false)
}

fn floor_is_water(content: &ContentRegistry, floor: BlockId) -> bool {
    floor_is_liquid(content, floor)
        && !matches!(
            content.block(floor).map(|def| def.name.as_str()),
            Some("tar" | "molten-slag" | "pooled-cryofluid" | "arkycite-floor")
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ContentRegistry {
        crate::content::test_support::test_registry()
    }

    #[test]
    fn serpulo_ids_resolve() {
        let content = registry();
        let ids = Ids::load(&content);
        assert_ne!(ids.arr[0][0], BlockId::AIR);
        assert_ne!(ids.ore_copper, BlockId::AIR);
        assert_ne!(ids.dark_panel_3, BlockId::AIR);
    }

    /// T13: `SerpuloPlanetGenerator.getColor` returns `Block.mapColor` with
    /// `1 - albedo` alpha (`SerpuloPlanetGenerator.java:133-138`; albedos from
    /// `Blocks.java:220-282,558-611`).
    #[test]
    fn serpulo_color_alpha_from_albedo() {
        let content = registry();
        let mut generator = SerpuloPlanetGenerator::new();
        generator.load_content(&content, &[]);
        let alpha = |name: &str| {
            let id = content.block_id(name).expect("block");
            generator.colors.rgba(id)[3]
        };
        assert!((alpha("deep-water") - 0.1).abs() < 1e-4, "albedo 0.9");
        assert!((alpha("arkycite-floor") - 0.1).abs() < 1e-4, "albedo 0.9");
        assert!((alpha("snow") - 0.3).abs() < 1e-4, "albedo 0.7");
        assert!((alpha("ice") - 0.35).abs() < 1e-4, "albedo 0.65");
        assert!((alpha("ice-snow") - 0.4).abs() < 1e-4, "albedo 0.6");
        assert!((alpha("ice-wall") - 0.4).abs() < 1e-4, "albedo 0.6");
        assert!((alpha("moss") - 1.0).abs() < 1e-4, "default albedo 0");
        // A generated tile that resolves to deep water carries the same alpha.
        let grid = crate::render::g3d::grid::PlanetGrid::create(6);
        for tile in &grid.tiles {
            if get_block_with(&generator.planet, tile.v, &generator.ids) == generator.ids.deep_water
            {
                let color = generator.get_color(tile.v);
                assert!((color[3] - 0.1).abs() < 1e-4, "deep-water tile alpha");
                return;
            }
        }
        panic!("no deep-water tile on the divisions=6 grid");
    }

    /// T14: `getEmissiveColor` lights captured sectors (sharded) and enemy
    /// bases (crux) from the live sector table; no sectors means no glow.
    #[test]
    fn serpulo_emissive_returns_sector_glow() {
        let content = registry();
        let mut generator = SerpuloPlanetGenerator::new();
        generator.load_content(&content, &[]);
        assert_eq!(
            generator.get_emissive_color([0.3, 0.2, 0.9]),
            [0.0, 0.0, 0.0, 0.0],
            "no sectors -> no glow"
        );
        generator.set_sector_glow(vec![SectorGlow {
            v: [0.0, 0.0, 1.0],
            has_enemy_base: false,
            captured: false,
            has_base: true,
            has_preset: false,
            difficulty: 0.0,
            light_coverage: 60_000.0,
        }]);
        let lit = (0..20).any(|i| {
            let z = 0.85 + i as f32 * 0.005;
            generator.get_emissive_color([0.0, 0.0, z])[3] > 0.0
        });
        assert!(lit, "a captured sector glows near its tile");
    }

    #[test]
    fn serpulo_has_core_region() {
        let content = registry();
        let mut tiles = Tiles::new(128, 128);
        let mut generator = SerpuloPlanetGenerator::new();
        generator.generate(&mut tiles, &WorldParams::default(), &content);

        // The spawn room is cleared: the spawn tile and its 8-neighbours are
        // passable (not solid).
        let (sx, sy) = generator.spawn;
        for (dx, dy) in basic::D8 {
            let tile = tiles.get((sx + dx).clamp(0, 127), (sy + dy).clamp(0, 127));
            assert!(
                !tile.solid(&content),
                "spawn neighbourhood should be cleared"
            );
        }
    }

    #[test]
    fn serpulo_ore_present() {
        let content = registry();
        let mut tiles = Tiles::new(128, 128);
        let mut generator = SerpuloPlanetGenerator::new();
        generator.generate(&mut tiles, &WorldParams::default(), &content);

        let ids = Ids::load(&content);
        assert!(
            tiles
                .iter()
                .any(|tile| tile.overlay == ids.ore_copper || tile.overlay == ids.ore_lead),
            "copper/lead ore should be generated"
        );
        assert!(
            tiles.iter().any(|tile| tile.floor == ids.sand
                || tile.floor == ids.shallow_water
                || tile.floor == ids.darksand),
            "Serpulo terrain floors present"
        );
    }

    /// M8 golden (Rust-recorded, OD6-B): `world gen --planet serpulo --sector 0
    /// --seed 42 --width 128 --height 128`.
    #[test]
    fn serpulo_generation_checksum() {
        use crate::determinism::{Checksum, Hasher};
        let content = registry();
        let mut tiles = Tiles::new(128, 128);
        let mut generator = SerpuloPlanetGenerator::new();
        let params = WorldParams {
            seed_offset: 42,
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
            "55fcac31fc269e11"
        );
    }

    /// Plan 06 §3.10 R3: `generate_rules` writes the serpulo survival/attack
    /// rules and the seeded wave table.
    #[test]
    fn serpulo_generate_rules_writes_waves_and_spawns() {
        let content = registry();
        let mut tiles = Tiles::new(128, 128);
        let mut generator = SerpuloPlanetGenerator::new();
        let params = WorldParams {
            seed_offset: 42,
            ..WorldParams::default()
        };
        generator.generate(&mut tiles, &params, &content);

        let mut rules = crate::game::rules::Rules::default();
        generator.generate_rules(&mut rules, &params, &content);
        assert!(rules.waves, "waves enabled");
        assert_eq!(rules.enemy_core_build_radius, 600.0);
        // Non-attack sector: win wave formula; waveSpacing lerped from threat 0.
        assert!(!rules.attack_mode);
        assert_eq!(rules.win_wave, 10 + 5);
        assert_eq!(rules.wave_spacing, 60.0 * 65.0 * 2.0);
        assert!(!rules.spawns.is_empty(), "wave table generated");
        // Deterministic across two runs at the same seed.
        let mut again = crate::game::rules::Rules::default();
        let mut generator2 = SerpuloPlanetGenerator::new();
        generator2.generate(&mut tiles, &params, &content);
        generator2.generate_rules(&mut again, &params, &content);
        assert_eq!(rules.spawns.len(), again.spawns.len());
        for (a, b) in rules.spawns.iter().zip(&again.spawns) {
            assert_eq!(a.type_, b.type_);
            assert_eq!(a.unit_amount, b.unit_amount);
        }
    }
}
