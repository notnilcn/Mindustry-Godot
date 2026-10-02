// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BasicGenerator` — the shared terrain-generation helper library (plan 06
//! §3.10). Ported from `maps/generators/BasicGenerator.java`.
//!
//! Rust adaptation: helpers take `&mut Tiles` explicitly (Java held `this.tiles`)
//! and `pass` hands the closure a `&mut BasicGenerator` plus a [`Draw`] view so
//! the `floor`/`block`/`ore` mutation semantics are preserved exactly.
//!
//! Deferred (named): `getDarkness`-dependent `trimDark` takes an explicit
//! darkness closure; `decoration`/`pathfind` use plan-06 data only.

use crate::content::{BlockDef, BlockId, ContentRegistry};
use crate::determinism::SimRng;
use crate::math::noise;
use crate::world::tiles::Tiles;

use crate::maps::filters::block_info::is_static;

use super::astar;

/// Whether a tile's block is static (`Block.isStatic`).
fn is_static_block(content: &ContentRegistry, block: BlockId) -> bool {
    content.block(block).is_some_and(is_static)
}

/// A noise source for [`BasicGenerator`] (`BasicGenerator.noise`).
pub trait GenNoise: Send + Sync {
    /// Multi-octave noise normalized to `[0, 1]`, scaled by `mag`.
    fn noise(&self, x: f32, y: f32, octaves: f64, falloff: f64, scl: f64, mag: f64) -> f32;
}

/// Flat simplex 2D noise (tests / non-planet generators).
#[derive(Debug, Clone, Copy)]
pub struct SimplexNoise {
    /// Base seed.
    pub seed: i32,
}

impl GenNoise for SimplexNoise {
    fn noise(&self, x: f32, y: f32, octaves: f64, falloff: f64, scl: f64, mag: f64) -> f32 {
        noise::noise2d(
            self.seed,
            octaves as i32,
            falloff,
            1.0 / scl,
            (x + 10.0) as f64,
            (y + 10.0) as f64,
        ) * mag as f32
    }
}

/// The mutable draw state a `pass` step reads/writes (`floor`/`block`/`ore`).
#[derive(Debug, Clone, Copy)]
pub struct Draw {
    /// Floor id.
    pub floor: BlockId,
    /// Block id.
    pub block: BlockId,
    /// Overlay (ore) id.
    pub ore: BlockId,
}

/// The shared generation helper library (`BasicGenerator`).
pub struct BasicGenerator {
    /// Deterministic generator RNG (deviation §2.3.5).
    pub rand: SimRng,
    /// Grid width.
    pub width: i32,
    /// Grid height.
    pub height: i32,
    /// Current pass floor.
    pub floor: BlockId,
    /// Current pass block.
    pub block: BlockId,
    /// Current pass ore/overlay.
    pub ore: BlockId,
    /// Default launch schematic name (plan 12; informational).
    pub default_loadout: Option<String>,
    noise_source: Box<dyn GenNoise>,
}

impl std::fmt::Debug for BasicGenerator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BasicGenerator")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl BasicGenerator {
    /// Creates a generator with a noise source.
    pub fn new(noise_source: Box<dyn GenNoise>) -> Self {
        Self {
            rand: SimRng::new(0),
            width: 0,
            height: 0,
            floor: BlockId::AIR,
            block: BlockId::AIR,
            ore: BlockId::AIR,
            default_loadout: None,
            noise_source,
        }
    }

    /// Sets a deterministic seed.
    pub fn set_seed(&mut self, seed: u64) {
        self.rand = SimRng::new(seed);
    }

    /// `BasicGenerator.noise(x, y, octaves, falloff, scl, mag)`.
    pub fn noise_oct(&self, x: f32, y: f32, octaves: f64, falloff: f64, scl: f64, mag: f64) -> f32 {
        self.noise_source.noise(x, y, octaves, falloff, scl, mag)
    }

    /// `noise(x, y, octaves, falloff, scl)` (mag = 1).
    pub fn noise_oct1(&self, x: f32, y: f32, octaves: f64, falloff: f64, scl: f64) -> f32 {
        self.noise_oct(x, y, octaves, falloff, scl, 1.0)
    }

    /// `noise(x, y, scl, mag)` (octaves = 1, falloff = 1).
    pub fn noise_simple(&self, x: f32, y: f32, scl: f64, mag: f64) -> f32 {
        self.noise_oct(x, y, 1.0, 1.0, scl, mag)
    }

    /// `BasicGenerator.pass`: load tile state, run the closure, write it back.
    pub fn pass<F>(&mut self, tiles: &mut Tiles, content: &ContentRegistry, mut f: F)
    where
        F: FnMut(&mut BasicGenerator, &mut Draw, i32, i32),
    {
        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let mut draw = {
                let tile = tiles.geti(index);
                Draw {
                    floor: tile.floor,
                    block: tile.block,
                    ore: tile.overlay,
                }
            };
            f(self, &mut draw, x, y);

            let tile = tiles.geti_mut(index);
            tile.floor = draw.floor;
            if tile.block != draw.block {
                tile.block = draw.block;
            }
            tile.overlay = draw.ore;
            let _ = content;
        }
    }

    /// `BasicGenerator.each`: x-outer, y-inner (`Tiles.each`).
    pub fn each<F: FnMut(&mut BasicGenerator, i32, i32)>(
        &mut self,
        width: i32,
        height: i32,
        mut f: F,
    ) {
        for x in 0..width {
            for y in 0..height {
                f(self, x, y);
            }
        }
    }

    /// `BasicGenerator.median`.
    pub fn median(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        radius: i32,
        percentile: f64,
    ) {
        self.median_target(tiles, content, radius, percentile, BlockId::AIR);
    }

    /// `BasicGenerator.median(radius, percentile, targetFloor)`.
    pub fn median_target(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        radius: i32,
        percentile: f64,
        target_floor: BlockId,
    ) {
        let len = tiles.len();
        let mut blocks = vec![0u16; len];
        let mut floors = vec![0u16; len];
        for index in 0..len {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            if target_floor != BlockId::AIR && tiles.geti(index).floor != target_floor {
                continue;
            }
            let mut fbuf = Vec::new();
            let mut bbuf = Vec::new();
            circle(x, y, tiles.width, tiles.height, radius, |cx, cy| {
                let tile = tiles.get(cx, cy);
                fbuf.push(tile.floor.raw());
                bbuf.push(tile.block.raw());
            });
            fbuf.sort_unstable();
            bbuf.sort_unstable();
            let pick = |items: &[u16]| -> u16 {
                let index = ((items.len() as f64 * percentile) as i32)
                    .clamp(0, items.len() as i32 - 1) as usize;
                items[index]
            };
            floors[index] = pick(&fbuf);
            blocks[index] = pick(&bbuf);
        }

        let width = tiles.width;
        self.pass(tiles, content, |_, draw, x, y| {
            let index = (x + y * width) as usize;
            draw.block = BlockId::new(blocks[index]);
            draw.floor = BlockId::new(floors[index]);
        });
    }

    /// `BasicGenerator.ores`.
    pub fn ores(&mut self, tiles: &mut Tiles, content: &ContentRegistry, ores: &[BlockId]) {
        self.pass(tiles, content, |b, draw, x, y| {
            if !has_surface(content, draw.floor) {
                return;
            }
            let offset_x = x - 4;
            let offset_y = y + 23;
            for i in (0..ores.len()).rev() {
                let i = i as i32;
                if (0.5
                    - b.noise_oct1(
                        offset_x as f32,
                        (offset_y + i * 999) as f32,
                        2.0,
                        0.7,
                        (40 + i * 2) as f64,
                    ))
                .abs()
                    > 0.26
                    && (0.5
                        - b.noise_oct1(
                            offset_x as f32,
                            (offset_y - i * 999) as f32,
                            1.0,
                            1.0,
                            (30 + i * 4) as f64,
                        ))
                    .abs()
                        > 0.37
                {
                    draw.ore = ores[i as usize];
                    break;
                }
            }
        });
    }

    /// `BasicGenerator.ore`.
    pub fn ore(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        dest: BlockId,
        src: BlockId,
        i: f32,
        thresh: f32,
    ) {
        self.pass(tiles, content, |b, draw, x, y| {
            if draw.floor != src {
                return;
            }
            if (0.5
                - b.noise_oct1(
                    x as f32,
                    y as f32 + i * 999.0,
                    2.0,
                    0.7,
                    (40.0 + i * 2.0) as f64,
                ))
            .abs()
                > 0.26 * thresh
                && (0.5
                    - b.noise_oct1(
                        x as f32,
                        y as f32 - i * 999.0,
                        1.0,
                        1.0,
                        (30.0 + i * 4.0) as f64,
                    ))
                .abs()
                    > 0.37 * thresh
            {
                draw.ore = dest;
            }
        });
    }

    /// `BasicGenerator.oreAround`.
    #[allow(clippy::too_many_arguments)]
    pub fn ore_around(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        ore: BlockId,
        wall: BlockId,
        radius: i32,
        scl: f32,
        thresh: f32,
    ) {
        let width = tiles.width;
        let height = tiles.height;
        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let tile = tiles.geti(index);
            if tile.block != BlockId::AIR || !has_surface(content, tile.floor) {
                continue;
            }
            if self.noise_simple(
                x as f32,
                y as f32 + ore.raw() as f32 * 999.0,
                scl as f64,
                1.0,
            ) <= thresh
            {
                continue;
            }
            let mut found = false;
            'outer: for dx in -radius..=radius {
                for dy in -radius..=radius {
                    if within(dx, dy, radius) {
                        let wx = x + dx;
                        let wy = y + dy;
                        if wx >= 0
                            && wy >= 0
                            && wx < width
                            && wy < height
                            && tiles.get(wx, wy).block == wall
                        {
                            found = true;
                            break 'outer;
                        }
                    }
                }
            }
            if found {
                tiles.geti_mut(index).overlay = ore;
            }
        }
    }

    /// `BasicGenerator.wallOre`.
    pub fn wall_ore(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        src: BlockId,
        dest: BlockId,
        scl: f32,
        thresh: f32,
    ) {
        let overlay = is_overlay(content, dest);
        let width = tiles.width;
        let height = tiles.height;
        // Inlined `pass` (reads neighbours, so the tile grid must not be
        // captured by a `pass` closure — see `pass`'s borrow contract).
        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let mut draw = {
                let tile = tiles.geti(index);
                Draw {
                    floor: tile.floor,
                    block: tile.block,
                    ore: tile.overlay,
                }
            };
            if draw.block != BlockId::AIR {
                let mut empty = false;
                for (dx, dy) in D8 {
                    let wx = x + dx;
                    let wy = y + dy;
                    if wx >= 0
                        && wy >= 0
                        && wx < width
                        && wy < height
                        && tiles.get(wx, wy).block == BlockId::AIR
                    {
                        empty = true;
                        break;
                    }
                }
                if empty
                    && self.noise_oct((x + 78) as f32, y as f32, 4.0, 0.7, scl as f64, 1.0) > thresh
                    && draw.block == src
                {
                    if overlay {
                        draw.ore = dest;
                    } else {
                        draw.block = dest;
                    }
                }
            }
            let tile = tiles.geti_mut(index);
            tile.floor = draw.floor;
            if tile.block != draw.block {
                tile.block = draw.block;
            }
            tile.overlay = draw.ore;
        }
    }

    /// `BasicGenerator.cliffs`.
    pub fn cliffs(&mut self, tiles: &mut Tiles, content: &ContentRegistry) {
        let width = tiles.width;
        let height = tiles.height;
        let cliff = content.block_id("cliff").unwrap_or(BlockId::AIR);
        let air = BlockId::AIR;
        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let block = tiles.geti(index).block;
            if !is_static_block(content, block) || block == cliff {
                continue;
            }
            let mut rotation = 0i32;
            for (i, (dx, dy)) in D8.iter().enumerate() {
                let (dx, dy) = (*dx, *dy);
                let wx = x + dx;
                let wy = y + dy;
                if wx >= 0
                    && wy >= 0
                    && wx < width
                    && wy < height
                    && !is_static_block(content, tiles.get(wx, wy).block)
                {
                    rotation |= 1 << i;
                }
            }
            if rotation != 0 {
                tiles.geti_mut(index).block = cliff;
            }
            tiles.geti_mut(index).data = rotation as i8;
        }
        for index in 0..tiles.len() {
            let block = tiles.geti(index).block;
            if block != cliff && is_static_block(content, block) {
                tiles.geti_mut(index).block = air;
            }
        }
    }

    /// `BasicGenerator.terrain`.
    pub fn terrain(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        dst: BlockId,
        scl: f32,
        mag: f32,
        cmag: f32,
    ) {
        let width = self.width.max(1) as f32;
        let height = self.height.max(1) as f32;
        self.pass(tiles, content, |b, draw, x, y| {
            let rocks = b.noise_oct1(x as f32, y as f32, 5.0, 0.5, scl as f64) * mag
                + (((x as f32 / width) - 0.5).powi(2) + ((y as f32 / height) - 0.5).powi(2)).sqrt()
                    * cmag;
            let edge_dist = (x)
                .min(y)
                .min((x - (b.width - 1)).abs())
                .min((y - (b.height - 1)).abs()) as f64;
            let transition = 5.0;
            let rocks = if edge_dist < transition {
                rocks + ((transition - edge_dist) / transition / 1.5) as f32
            } else {
                rocks
            };
            if rocks > 0.9 {
                draw.block = dst;
            }
        });
    }

    /// `BasicGenerator.noise(floor, block, octaves, falloff, scl, threshold)`.
    #[allow(clippy::too_many_arguments)]
    pub fn noise_pass(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        floor: BlockId,
        block: BlockId,
        octaves: i32,
        falloff: f32,
        scl: f32,
        threshold: f32,
    ) {
        let air = BlockId::AIR;
        self.pass(tiles, content, |b, draw, x, y| {
            if b.noise_oct1(
                x as f32,
                y as f32,
                octaves as f64,
                falloff as f64,
                scl as f64,
            ) > threshold
            {
                let solid = content.block(draw.block).is_some_and(|def| def.solid);
                if solid {
                    draw.floor = floor;
                    draw.block = block;
                } else {
                    draw.floor = floor;
                }
                let _ = air;
            }
        });
    }

    /// `BasicGenerator.overlay`.
    #[allow(clippy::too_many_arguments)]
    pub fn overlay(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        floor: BlockId,
        block: BlockId,
        chance: f32,
        octaves: i32,
        falloff: f32,
        scl: f32,
        threshold: f32,
    ) {
        self.pass(tiles, content, |b, draw, x, y| {
            if b.noise_oct1(
                x as f32,
                y as f32,
                octaves as f64,
                falloff as f64,
                scl as f64,
            ) > threshold
                && b.rand
                    .chance(crate::determinism::RngStream::MapGen, chance as f64)
                && draw.floor == floor
            {
                draw.ore = block;
            }
        });
    }

    /// `BasicGenerator.tech`.
    pub fn tech(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        floor1: BlockId,
        floor2: BlockId,
        wall: BlockId,
    ) {
        let sec_size = 20;
        self.pass(tiles, content, |b, draw, x, y| {
            if !has_surface(content, draw.floor) {
                return;
            }
            let mx = x % sec_size;
            let my = y % sec_size;
            let sclx = x / sec_size;
            let scly = y / sec_size;
            if b.noise_simple(sclx as f32, scly as f32, 0.2, 1.0) > 0.63
                && b.noise_simple(sclx as f32, (scly + 999) as f32, 200.0, 1.0) > 0.6
                && (mx == 0 || my == 0 || mx == sec_size - 1 || my == sec_size - 1)
            {
                if b.rand.chance(
                    crate::determinism::RngStream::MapGen,
                    b.noise_simple((x + 0x231523) as f32, y as f32, 40.0, 1.0) as f64,
                ) {
                    draw.floor = floor1;
                    if ((mx as f32) - (sec_size as f32 / 2.0))
                        .hypot((my as f32) - (sec_size as f32 / 2.0))
                        > sec_size as f32 / 2.0 + 2.0
                    {
                        draw.floor = floor2;
                    }
                }
                let solid = content.block(draw.block).is_some_and(|def| def.solid);
                if solid && b.rand.chance(crate::determinism::RngStream::MapGen, 0.7) {
                    draw.block = wall;
                }
            }
        });
    }

    /// `BasicGenerator.distort`.
    pub fn distort(&mut self, tiles: &mut Tiles, content: &ContentRegistry, scl: f32, mag: f32) {
        let width = tiles.width;
        let height = tiles.height;
        let len = tiles.len();
        let mut blocks = vec![0u16; len];
        let mut floors = vec![0u16; len];
        for x in 0..width {
            for y in 0..height {
                let idx = (y * width + x) as usize;
                let cx = x as f32
                    + self.noise_simple((x - 155) as f32, (y - 200) as f32, scl as f64, mag as f64)
                    - mag / 2.0;
                let cy = y as f32
                    + self.noise_simple((x + 155) as f32, (y + 155) as f32, scl as f64, mag as f64)
                    - mag / 2.0;
                let ox = (cx as i32).clamp(0, width - 1);
                let oy = (cy as i32).clamp(0, height - 1);
                let other = tiles.get(ox, oy);
                blocks[idx] = other.block.raw();
                floors[idx] = other.floor.raw();
            }
        }
        for i in 0..len {
            let tile = tiles.geti_mut(i);
            tile.floor = BlockId::new(floors[i]);
            tile.block = BlockId::new(blocks[i]);
        }
        let _ = content;
    }

    /// `BasicGenerator.scatter`.
    pub fn scatter(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        target: BlockId,
        dst: BlockId,
        chance: f32,
    ) {
        self.pass(tiles, content, |b, draw, _, _| {
            if !b
                .rand
                .chance(crate::determinism::RngStream::MapGen, chance as f64)
            {
                return;
            }
            if draw.floor == target {
                draw.floor = dst;
            } else if draw.block == target {
                draw.block = dst;
            }
        });
    }

    /// `BasicGenerator.cells`.
    #[allow(clippy::needless_range_loop)]
    pub fn cells(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        iterations: i32,
        birth_limit: i32,
        death_limit: i32,
        cradius: i32,
    ) {
        let width = tiles.width;
        let height = tiles.height;
        let len = tiles.len();
        let mut read = vec![false; len];
        let mut write = vec![false; len];
        for index in 0..len {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            read[(y * width + x) as usize] = tiles.geti(index).block != BlockId::AIR;
        }

        for _ in 0..iterations {
            for x in 0..width {
                for y in 0..height {
                    let mut alive = 0;
                    for cx in -cradius..=cradius {
                        for cy in -cradius..=cradius {
                            if (cx == 0 && cy == 0) || !within(cx, cy, cradius) {
                                continue;
                            }
                            let wx = x + cx;
                            let wy = y + cy;
                            if wx < 0
                                || wy < 0
                                || wx >= width
                                || wy >= height
                                || read[(wy * width + wx) as usize]
                            {
                                alive += 1;
                            }
                        }
                    }
                    let index = (y * width + x) as usize;
                    write[index] = if read[index] {
                        alive >= death_limit
                    } else {
                        alive > birth_limit
                    };
                }
            }
            read.clone_from(&write);
        }

        for index in 0..len {
            if !read[index] {
                tiles.geti_mut(index).block = BlockId::AIR;
            } else {
                let wall = content
                    .block(tiles.geti(index).floor)
                    .and_then(|def| content.block_by_name(&format!("{}-wall", def.name)))
                    .map(|def| def.id)
                    .unwrap_or(BlockId::AIR);
                tiles.geti_mut(index).block = wall;
            }
        }
    }

    /// `BasicGenerator.nearWall` (8-neighbour).
    pub fn near_wall(&self, tiles: &Tiles, content: &ContentRegistry, x: i32, y: i32) -> bool {
        for (dx, dy) in D8 {
            if let Some(tile) = tiles.getn(x + dx, y + dy)
                && tile.block != BlockId::AIR
            {
                return true;
            }
        }
        let _ = content;
        false
    }

    /// `BasicGenerator.nearAir` (4-neighbour).
    pub fn near_air(&self, tiles: &Tiles, x: i32, y: i32) -> bool {
        for (dx, dy) in D4 {
            if tiles
                .getn(x + dx, y + dy)
                .is_some_and(|tile| tile.block == BlockId::AIR)
            {
                return true;
            }
        }
        false
    }

    /// `BasicGenerator.removeWall`.
    pub fn remove_wall<P: Fn(&BlockDef) -> bool>(
        &self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        cx: i32,
        cy: i32,
        rad: i32,
        pred: P,
    ) {
        for x in -rad..=rad {
            for y in -rad..=rad {
                let wx = cx + x;
                let wy = cy + y;
                if within(x, y, rad)
                    && tiles.in_bounds(wx, wy)
                    && content.block(tiles.get(wx, wy).block).is_some_and(&pred)
                {
                    tiles.get_mut(wx, wy).block = BlockId::AIR;
                }
            }
        }
    }

    /// `BasicGenerator.near`.
    pub fn near(&self, tiles: &Tiles, cx: i32, cy: i32, rad: i32, block: BlockId) -> bool {
        for x in -rad..=rad {
            for y in -rad..=rad {
                let wx = cx + x;
                let wy = cy + y;
                if within(x, y, rad) && tiles.in_bounds(wx, wy) && tiles.get(wx, wy).block == block
                {
                    return true;
                }
            }
        }
        false
    }

    /// `BasicGenerator.decoration`.
    pub fn decoration(&mut self, tiles: &mut Tiles, content: &ContentRegistry, chance: f32) {
        let width = tiles.width;
        let height = tiles.height;
        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let mut blocked = false;
            for (dx, dy) in D4 {
                let wx = x + dx;
                let wy = y + dy;
                if wx >= 0
                    && wy >= 0
                    && wx < width
                    && wy < height
                    && tiles.get(wx, wy).block != BlockId::AIR
                {
                    blocked = true;
                    break;
                }
            }
            if blocked {
                continue;
            }
            let floor = tiles.geti(index).floor;
            if self
                .rand
                .chance(crate::determinism::RngStream::MapGen, chance as f64)
                && has_surface(content, floor)
                && tiles.geti(index).block == BlockId::AIR
                && let Some(def) = content.block(floor)
                && let Some(decoration) = crate::maps::filters::block_info::decoration(content, def)
            {
                tiles.geti_mut(index).block = decoration;
            }
        }
    }

    /// `BasicGenerator.blend`.
    pub fn blend(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        floor: BlockId,
        around: BlockId,
        radius: f32,
    ) {
        let r2 = radius * radius;
        let cap = radius.ceil() as i32;
        let width = tiles.width;
        let height = tiles.height;
        let len = tiles.len();
        let around_floor = around;
        for i in 0..len {
            let (x, y) = {
                let tile = tiles.geti(i);
                (tile.x as i32, tile.y as i32)
            };
            if tiles.geti(i).floor != floor && tiles.geti(i).block != floor {
                continue;
            }
            for cx in -cap..=cap {
                for cy in -cap..=cap {
                    if (cx * cx + cy * cy) as f32 <= r2 {
                        let wx = x + cx;
                        let wy = y + cy;
                        if wx >= 0
                            && wy >= 0
                            && wx < width
                            && wy < height
                            && tiles.get(wx, wy).floor != floor
                        {
                            tiles.get_mut(wx, wy).floor = around_floor;
                        }
                    }
                }
            }
        }
        let _ = content;
    }

    /// `BasicGenerator.brush`.
    pub fn brush(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        path: &[(i32, i32)],
        rad: i32,
    ) {
        for (x, y) in path {
            self.erase(tiles, content, *x, *y, rad);
        }
    }

    /// `BasicGenerator.erase`.
    pub fn erase(
        &mut self,
        tiles: &mut Tiles,
        _content: &ContentRegistry,
        cx: i32,
        cy: i32,
        rad: i32,
    ) {
        for x in -rad..=rad {
            for y in -rad..=rad {
                let wx = cx + x;
                let wy = cy + y;
                if within(x, y, rad) && tiles.in_bounds(wx, wy) {
                    tiles.get_mut(wx, wy).block = BlockId::AIR;
                }
            }
        }
    }

    /// `BasicGenerator.pathfind` (deterministic A*).
    pub fn pathfind(&self, tiles: &Tiles, start: (i32, i32), end: (i32, i32)) -> Vec<(i32, i32)> {
        astar::pathfind(
            tiles,
            start,
            end,
            |_, _| 1.0,
            |tile| tile.block == BlockId::AIR,
        )
    }

    /// `BasicGenerator.trimDark`; `darkness` is supplied by the caller (plan 12).
    pub fn trim_dark<D: Fn(i32, i32) -> f32>(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        darkness: D,
    ) {
        let width = tiles.width;
        let height = tiles.height;
        for index in 0..tiles.len() {
            let (x, y) = {
                let tile = tiles.geti(index);
                (tile.x as i32, tile.y as i32)
            };
            let mut any = darkness(x, y) > 0.0;
            for (dx, dy) in D4 {
                if !any {
                    any = darkness(x + dx, y + dy) > 0.0;
                }
            }
            if any {
                let wall = content
                    .block(tiles.geti(index).floor)
                    .and_then(|def| content.block_by_name(&format!("{}-wall", def.name)))
                    .map(|def| def.id)
                    .unwrap_or(BlockId::AIR);
                tiles.geti_mut(index).block = wall;
            }
            let _ = (width, height);
        }
    }

    /// `BasicGenerator.inverseFloodFill`.
    #[allow(clippy::needless_range_loop)]
    pub fn inverse_flood_fill(
        &mut self,
        tiles: &mut Tiles,
        content: &ContentRegistry,
        start: (i32, i32),
    ) {
        let width = tiles.width;
        let height = tiles.height;
        let len = tiles.len();
        let mut used = vec![false; len];
        let mut stack = vec![start];
        while let Some((x, y)) = stack.pop() {
            used[(y * width + x) as usize] = true;
            for (dx, dy) in D4 {
                let nx = x + dx;
                let ny = y + dy;
                if nx < 0 || ny < 0 || nx >= width || ny >= height {
                    continue;
                }
                let index = (ny * width + nx) as usize;
                if tiles.geti(index).block == BlockId::AIR && !used[index] {
                    used[index] = true;
                    stack.push((nx, ny));
                }
            }
        }
        for index in 0..len {
            if !used[index] && tiles.geti(index).block == BlockId::AIR {
                let wall = content
                    .block(tiles.geti(index).floor)
                    .and_then(|def| content.block_by_name(&format!("{}-wall", def.name)))
                    .map(|def| def.id)
                    .unwrap_or(BlockId::AIR);
                tiles.geti_mut(index).block = wall;
            }
        }
    }

    /// The generation entry (`BasicGenerator.generate(Tiles, WorldParams)`).
    pub fn begin(&mut self, tiles: &Tiles) {
        self.width = tiles.width;
        self.height = tiles.height;
    }
}

/// 4-neighbour offsets (`Geometry.d4`): west, north, east, south.
pub const D4: [(i32, i32); 4] = [(-1, 0), (0, -1), (1, 0), (0, 1)];

/// 8-neighbour offsets (`Geometry.d8`).
pub const D8: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// `Mathf.within(x, y, radius)`.
pub fn within(x: i32, y: i32, radius: i32) -> bool {
    (x * x + y * y) <= radius * radius
}

/// `Geometry.circle`: iterate the clamped disc around `(cx, cy)`.
pub fn circle<F: FnMut(i32, i32)>(
    cx: i32,
    cy: i32,
    width: i32,
    height: i32,
    radius: i32,
    mut f: F,
) {
    for x in -radius..=radius {
        for y in -radius..=radius {
            if !within(x, y, radius) {
                continue;
            }
            let wx = cx + x;
            let wy = cy + y;
            if wx >= 0 && wy >= 0 && wx < width && wy < height {
                f(wx, wy);
            }
        }
    }
}

fn has_surface(content: &ContentRegistry, floor: BlockId) -> bool {
    content
        .block(floor)
        .is_some_and(crate::maps::filters::block_info::has_surface)
}

fn is_overlay(content: &ContentRegistry, block: BlockId) -> bool {
    content
        .block(block)
        .is_some_and(crate::maps::filters::block_info::is_overlay)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `maps::generators::tests::basic_pass_writes_fields`.
    #[test]
    fn basic_pass_writes_fields() {
        let content = crate::content::test_support::test_registry();
        let sand = content.block_id("sand-floor").unwrap_or(BlockId::AIR);
        let wall = content.block_id("stone-wall").unwrap_or(BlockId::AIR);
        let ore = content.block_id("ore-copper").unwrap_or(BlockId::AIR);

        let mut tiles = Tiles::new(4, 1);
        let mut generator = BasicGenerator::new(Box::new(SimplexNoise { seed: 1 }));
        generator.begin(&tiles);
        generator.pass(&mut tiles, &content, |_, draw, x, _| {
            if x % 2 == 0 {
                draw.floor = sand;
                draw.block = wall;
                draw.ore = ore;
            }
        });

        assert_eq!(tiles.get(0, 0).floor, sand);
        assert_eq!(tiles.get(0, 0).block, wall);
        assert_eq!(tiles.get(0, 0).overlay, ore);
        assert_eq!(tiles.get(1, 0).block, BlockId::AIR);
    }
}
