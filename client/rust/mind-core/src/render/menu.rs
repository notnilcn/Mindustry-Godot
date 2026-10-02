// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `MenuRenderer` procedural menu world (`graphics/MenuRenderer.java`, plan 16
//! §3.10/M7). Ports the deterministic terrain-selection logic (Simplex/Ridged
//! thresholds, ores, heat, tech grid, tendrils) onto menu-only tile records and
//! the flyer sampling math. `--menu-seed` pins the view RNG (OD16-F); upstream
//! seeds randomly per launch.
//!
//! The menu world is 100×50 (desktop) / 60×40 (mobile), matching upstream. Tile
//! block names are the plan-02 ABI strings, resolved by the gdext host.

use crate::math::{noise, ridged};

/// `MenuRenderer.darkness`.
pub const DARKNESS: f32 = 0.3;

/// One menu tile (floor/wall/overlay block names).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuTile {
    /// Floor block name.
    pub floor: &'static str,
    /// Wall block name (`air` = none).
    pub wall: &'static str,
    /// Ore overlay block name (`air` = none).
    pub overlay: &'static str,
}

const AIR: &str = "air";
const FLOOR_SETS: [(&str, &str); 9] = [
    ("sand", "sandWall"),
    ("shale", "shaleWall"),
    ("ice", "iceWall"),
    ("sand", "sandWall"),
    ("shale", "shaleWall"),
    ("ice", "iceWall"),
    ("moss", "sporePine"),
    ("dirt", "dirtWall"),
    ("dacite", "daciteWall"),
];
const FLOOR_SETS2: [(&str, &str); 6] = [
    ("basalt", "duneWall"),
    ("basalt", "duneWall"),
    ("stone", "stoneWall"),
    ("stone", "stoneWall"),
    ("moss", "sporeWall"),
    ("salt", "saltWall"),
];
const ORES: [&str; 6] = [
    "oreCopper",
    "oreLead",
    "oreScrap",
    "oreCoal",
    "oreTitanium",
    "oreThorium",
];

/// The generated menu world (`MenuRenderer.generate`).
#[derive(Clone, Debug)]
pub struct MenuWorld {
    /// Width in tiles.
    pub width: usize,
    /// Height in tiles.
    pub height: usize,
    /// Row-major tiles (`x + y * width`).
    pub tiles: Vec<MenuTile>,
    /// Number of flyers.
    pub flyers: usize,
    /// The pinned seed.
    pub seed: i32,
}

impl MenuWorld {
    /// Tile at `(x, y)`, if in bounds.
    pub fn tile(&self, x: usize, y: usize) -> Option<&MenuTile> {
        if x < self.width && y < self.height {
            self.tiles.get(x + y * self.width)
        } else {
            None
        }
    }

    /// `MenuRenderer.flyers` position for flyer `i` at `time`.
    pub fn flyer_position(&self, i: usize, time: f32, flyer_rot: f32, speed: f32) -> [f32; 2] {
        crate::render::menu::flyer_position(self, i, time, flyer_rot, speed)
    }
}

/// `MenuRenderer` RNG (a small deterministic LCG for the view-only menu).
#[derive(Clone, Debug)]
pub struct MenuRng(u64);

impl MenuRng {
    /// Seeds the RNG; `0` is remapped to a nonzero constant.
    pub fn new(seed: i32) -> Self {
        let s = seed as u32 as u64;
        Self(if s == 0 { 0x9e3779b97f4a7c15 } else { s })
    }

    fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    /// `[0, 1)`.
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// `Mathf.random(min, max)`.
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next_f32()
    }

    /// `Mathf.random(int range)`.
    pub fn int(&mut self, range: u32) -> u32 {
        if range == 0 {
            0
        } else {
            (self.next_f32() * range as f32) as u32
        }
    }

    /// `Mathf.chance(p)`.
    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }
}

/// `MenuRenderer.generate`: builds the deterministic menu tile grid.
pub fn generate(seed: i32, mobile: bool) -> MenuWorld {
    let width = if mobile { 60 } else { 100 };
    let height = if mobile { 40 } else { 50 };
    let mut rng = MenuRng::new(seed);

    let offset = rng.int(100000) as i32;
    let s1 = offset;
    let s2 = offset + 1;
    let s3 = offset + 2;

    let set = FLOOR_SETS[rng.int(FLOOR_SETS.len() as u32) as usize];
    let set2 = FLOOR_SETS2[rng.int(FLOOR_SETS2.len() as u32) as usize];
    let idx1 = rng.int(ORES.len() as u32) as usize;
    let ore1 = ORES[idx1];
    // Remove ore1, then pick ore2 from the remainder.
    let mut remaining: Vec<&str> = Vec::new();
    for (i, ore) in ORES.iter().enumerate() {
        if i != idx1 {
            remaining.push(ore);
        }
    }
    let ore2 = remaining[rng.int(remaining.len() as u32) as usize];

    let tr1 = rng.range(0.65, 0.85) as f64;
    let tr2 = rng.range(0.65, 0.85) as f64;
    let doheat = rng.chance(0.25);
    let tendrils = rng.chance(0.25);
    let tech = rng.chance(0.25);
    let sec_size = 10i32;

    let (floord, walld) = set;
    let (floord2, walld2) = set2;

    let mut tiles = Vec::with_capacity(width * height);
    for x in 0..width {
        for y in 0..height {
            let xf = x as f64;
            let yf = y as f64;
            let mut floor = floord;
            let mut ore = AIR;
            let mut wall = AIR;

            if noise::noise2d(s1, 3, 0.5, 1.0 / 20.0, xf, yf) > 0.5 {
                wall = walld;
            }
            if noise::noise2d(s3, 3, 0.5, 1.0 / 20.0, xf, yf) > 0.5 {
                floor = floord2;
                if wall != AIR {
                    wall = walld2;
                }
            }
            if noise::noise2d(s2, 3, 0.3, 1.0 / 30.0, xf, yf) as f64 > tr1 {
                ore = ore1;
            }
            if noise::noise2d(s2, 2, 0.2, 1.0 / 15.0, xf, yf + 99999.0) as f64 > tr2 {
                ore = ore2;
            }
            if doheat {
                let heat = noise::noise2d(s3, 4, 0.6, 1.0 / 50.0, xf, yf + 9999.0) as f64;
                let base = 0.65;
                if heat > base {
                    ore = AIR;
                    wall = AIR;
                    floor = "basalt";
                    if heat > base + 0.1 {
                        floor = "hotrock";
                        if heat > base + 0.15 {
                            floor = "magmarock";
                        }
                    }
                }
            }
            if tech {
                let mx = x as i32 % sec_size;
                let my = y as i32 % sec_size;
                let sclx = (x as i32 / sec_size) as f64;
                let scly = (y as i32 / sec_size) as f64;
                if noise::noise2d(s1, 2, 1.0 / 10.0, 0.5, sclx, scly) > 0.4
                    && (mx == 0 || my == 0 || mx == sec_size - 1 || my == sec_size - 1)
                {
                    floor = "darkPanel3";
                    let dx = (mx - sec_size / 2) as f32;
                    let dy = (my - sec_size / 2) as f32;
                    if (dx * dx + dy * dy).sqrt() > sec_size as f32 / 2.0 + 1.0 {
                        floor = "darkPanel4";
                    }
                    if wall != AIR && rng.chance(0.7) {
                        wall = "darkMetal";
                    }
                }
            }
            if tendrils && ridged::noise2d_freq(1 + offset, xf, yf, 1.0 / 17.0) > 0.0 {
                floor = if rng.chance(0.2) { "sporeMoss" } else { "moss" };
                if wall != AIR {
                    wall = "sporeWall";
                }
            }

            tiles.push(MenuTile {
                floor,
                wall,
                overlay: ore,
            });
        }
    }

    let flyers = if rng.chance(0.2) {
        rng.int(35) as usize
    } else {
        rng.int(15) as usize
    };

    MenuWorld {
        width,
        height,
        tiles,
        flyers,
        seed,
    }
}

/// Arc `Mathf.randomSeed` (deterministic `[-0.5, 0.5)`).
fn random_seed(seed: i64) -> f32 {
    let mut h = seed
        .wrapping_mul(0x5851_f42d_4c95_7f2d)
        .wrapping_add(0x1405_7b7e_f767_814f);
    h ^= h >> 33;
    ((h as u32) as f32 / (1u32 << 24) as f32) - 0.5
}

/// Arc `Mathf.randomSeedRange(seed, range)`.
fn random_seed_range(seed: i64, range: f32) -> f32 {
    random_seed(seed) * range
}

/// Arc `Mathf.absin(time, scl, mag)` (best-effort port).
fn absin(time: f32, scl: f32, mag: f32) -> f32 {
    (1.0 + (time / scl * std::f32::consts::TAU).sin()) * 0.5 * mag
}

/// `MenuRenderer.flyers(cons)` position for flyer `i`.
pub fn flyer_position(
    world: &MenuWorld,
    i: usize,
    time: f32,
    flyer_rot: f32,
    speed: f32,
) -> [f32; 2] {
    let tilesize = crate::config::TILESIZE as f32;
    let tw = world.width as f32 * tilesize + tilesize;
    let th = world.height as f32 * tilesize + tilesize;
    let range = 500.0f32;
    let offset = -100.0f32;
    let angle = flyer_rot.to_radians();
    let vx = angle.cos() * time * speed;
    let vy = angle.sin() * time * speed;
    let i = i as i64;
    let x = (random_seed_range(i, range)
        + vx
        + absin(time + random_seed_range(i + 2, range), 10.0, 3.4)
        + offset)
        .rem_euclid(tw + random_seed(i + 5) * 500.0 + 250.0);
    let y = (random_seed_range(i + 1, range)
        + vy
        + absin(time + random_seed_range(i + 3, range), 10.0, 3.4)
        + offset)
        .rem_euclid(th);
    [x, y]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_is_deterministic_for_seed() {
        let a = generate(1234, false);
        let b = generate(1234, false);
        assert_eq!(a.tiles, b.tiles);
        assert_eq!(a.flyers, b.flyers);
        assert_eq!(a.width, 100);
        assert_eq!(a.height, 50);
        assert_eq!(a.tiles.len(), 100 * 50);
        let c = generate(4321, false);
        assert_ne!(a.tiles, c.tiles);
    }

    #[test]
    fn mobile_dimensions() {
        let world = generate(1234, true);
        assert_eq!((world.width, world.height), (60, 40));
        assert_eq!(world.tiles.len(), 60 * 40);
    }

    #[test]
    fn flyers_are_finite_and_bounded() {
        let world = generate(1234, false);
        assert!(world.flyers <= 35);
        for i in 0..world.flyers {
            let p = world.flyer_position(i, 1.5, 45.0, 1.0);
            assert!(p[0].is_finite() && p[1].is_finite());
        }
    }
}
