// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The 16 vanilla generation filters (plan 06 §3.8).
//!
//! Ported from `maps/filters/{Noise,Scatter,Terrain,Distort,RiverNoise,Ore,
//! OreMedian,Median,Blend,Mirror,Clear,CoreSpawn,EnemySpawn,SpawnPath,Logic,
//! RandomItem}Filter.java`. `apply` bodies match upstream exactly; post filters
//! whose data lives in plans 07/11/12/13 are documented no-op hooks.
//!
//! Module-layout deviation: the plan sketched one file per filter; this module
//! groups them (no ABI/behavior change) to keep the port reviewable.

#![allow(clippy::too_many_arguments)]

use serde::{Deserialize, Serialize};

use crate::content::{BlockId, ContentRegistry};
use crate::determinism::RngStream;
use crate::determinism::SimRng;
use crate::world::tiles::Tiles;

use super::{
    GenerateFilter, GenerateInput, filter_chance, filter_noise, filter_noise_xy, filter_rnoise_oct,
};

fn block_name(registry: &ContentRegistry, id: BlockId) -> String {
    registry
        .block(id)
        .map(|def| def.name.clone())
        .unwrap_or_else(|| "air".to_owned())
}

fn block_id(registry: &ContentRegistry, name: &str) -> BlockId {
    registry
        .block_by_name(name)
        .map(|def| def.id)
        .unwrap_or(BlockId::AIR)
}

/// Serializable filter payload (`genfilters` element, plan 06 §6.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "class")]
pub enum FilterJson {
    /// `NoiseFilter`.
    #[serde(rename = "noise")]
    Noise {
        seed: i32,
        scl: f32,
        threshold: f32,
        octaves: f32,
        falloff: f32,
        tilt: f32,
        floor: String,
        block: String,
        target: String,
    },
    /// `ScatterFilter`.
    #[serde(rename = "scatter")]
    Scatter {
        seed: i32,
        chance: f32,
        flooronto: String,
        floor: String,
        block: String,
    },
    /// `TerrainFilter`.
    #[serde(rename = "terrain")]
    Terrain {
        seed: i32,
        scl: f32,
        threshold: f32,
        octaves: f32,
        falloff: f32,
        magnitude: f32,
        #[serde(rename = "circleScl")]
        circle_scl: f32,
        tilt: f32,
        floor: String,
        block: String,
    },
    /// `DistortFilter`.
    #[serde(rename = "distort")]
    Distort { seed: i32, scl: f32, mag: f32 },
    /// `RiverNoiseFilter`.
    #[serde(rename = "riverNoise")]
    RiverNoise {
        seed: i32,
        scl: f32,
        threshold: f32,
        threshold2: f32,
        octaves: f32,
        falloff: f32,
        floor: String,
        floor2: String,
        block: String,
        target: String,
    },
    /// `OreFilter`.
    #[serde(rename = "ore")]
    Ore {
        seed: i32,
        scl: f32,
        threshold: f32,
        octaves: f32,
        falloff: f32,
        tilt: f32,
        ore: String,
        target: String,
    },
    /// `OreMedianFilter`.
    #[serde(rename = "oreMedian")]
    OreMedian {
        seed: i32,
        radius: f32,
        percentile: f32,
    },
    /// `MedianFilter`.
    #[serde(rename = "median")]
    Median {
        seed: i32,
        radius: f32,
        percentile: f32,
    },
    /// `BlendFilter`.
    #[serde(rename = "blend")]
    Blend {
        seed: i32,
        radius: f32,
        block: String,
        floor: String,
        ignore: String,
    },
    /// `MirrorFilter`.
    #[serde(rename = "mirror")]
    Mirror {
        seed: i32,
        angle: i32,
        rotate: bool,
        #[serde(rename = "axisX")]
        axis_x: f32,
        #[serde(rename = "axisY")]
        axis_y: f32,
    },
    /// `ClearFilter`.
    #[serde(rename = "clear")]
    Clear {
        seed: i32,
        target: String,
        replace: String,
        ignore: String,
    },
    /// `CoreSpawnFilter`.
    #[serde(rename = "coreSpawn")]
    CoreSpawn { seed: i32, amount: i32 },
    /// `EnemySpawnFilter`.
    #[serde(rename = "enemySpawn")]
    EnemySpawn { seed: i32, amount: i32 },
    /// `SpawnPathFilter`.
    #[serde(rename = "spawnPath")]
    SpawnPath {
        seed: i32,
        radius: i32,
        block: String,
    },
    /// `LogicFilter`.
    #[serde(rename = "logic")]
    Logic {
        seed: i32,
        code: Option<String>,
        #[serde(rename = "loop")]
        loop_enabled: bool,
    },
}

impl FilterJson {
    /// The class-default payload for a tag (missing JSON fields fall back here).
    pub fn default_for(tag: &str) -> Option<FilterJson> {
        let json = match tag {
            "noise" => FilterJson::Noise {
                seed: 0,
                scl: 40.0,
                threshold: 0.5,
                octaves: 3.0,
                falloff: 0.5,
                tilt: 0.0,
                floor: "stone".to_owned(),
                block: "stone-wall".to_owned(),
                target: "air".to_owned(),
            },
            "scatter" => FilterJson::Scatter {
                seed: 0,
                chance: 0.013,
                flooronto: "air".to_owned(),
                floor: "air".to_owned(),
                block: "air".to_owned(),
            },
            "terrain" => FilterJson::Terrain {
                seed: 0,
                scl: 40.0,
                threshold: 0.9,
                octaves: 3.0,
                falloff: 0.5,
                magnitude: 1.0,
                circle_scl: 2.1,
                tilt: 0.0,
                floor: "air".to_owned(),
                block: "stone-wall".to_owned(),
            },
            "distort" => FilterJson::Distort {
                seed: 0,
                scl: 40.0,
                mag: 5.0,
            },
            "riverNoise" => FilterJson::RiverNoise {
                seed: 0,
                scl: 40.0,
                threshold: 0.0,
                threshold2: 0.1,
                octaves: 1.0,
                falloff: 0.5,
                floor: "shallow-water".to_owned(),
                floor2: "deep-water".to_owned(),
                block: "sand-wall".to_owned(),
                target: "air".to_owned(),
            },
            "ore" => FilterJson::Ore {
                seed: 0,
                scl: 23.0,
                threshold: 0.81,
                octaves: 2.0,
                falloff: 0.3,
                tilt: 0.0,
                ore: "ore-copper".to_owned(),
                target: "air".to_owned(),
            },
            "oreMedian" => FilterJson::OreMedian {
                seed: 0,
                radius: 2.0,
                percentile: 0.5,
            },
            "median" => FilterJson::Median {
                seed: 0,
                radius: 2.0,
                percentile: 0.5,
            },
            "blend" => FilterJson::Blend {
                seed: 0,
                radius: 2.0,
                block: "sand-floor".to_owned(),
                floor: "sand-water".to_owned(),
                ignore: "air".to_owned(),
            },
            "mirror" => FilterJson::Mirror {
                seed: 0,
                angle: 45,
                rotate: false,
                axis_x: 0.5,
                axis_y: 0.5,
            },
            "clear" => FilterJson::Clear {
                seed: 0,
                target: "stone".to_owned(),
                replace: "air".to_owned(),
                ignore: "air".to_owned(),
            },
            "coreSpawn" => FilterJson::CoreSpawn { seed: 0, amount: 1 },
            "enemySpawn" => FilterJson::EnemySpawn { seed: 0, amount: 1 },
            "spawnPath" => FilterJson::SpawnPath {
                seed: 0,
                radius: 3,
                block: "air".to_owned(),
            },
            "logic" => FilterJson::Logic {
                seed: 0,
                code: None,
                loop_enabled: false,
            },
            _ => return None,
        };
        Some(json)
    }

    /// Builds the runtime filter, resolving content names through `registry`.
    pub fn into_filter(
        self,
        registry: &ContentRegistry,
    ) -> Result<Box<dyn GenerateFilter>, super::FilterError> {
        let bid = |name: &str| block_id(registry, name);
        Ok(match self {
            FilterJson::Noise {
                seed,
                scl,
                threshold,
                octaves,
                falloff,
                tilt,
                floor,
                block,
                target,
            } => Box::new(NoiseFilter {
                seed,
                scl,
                threshold,
                octaves,
                falloff,
                tilt,
                floor: bid(&floor),
                block: bid(&block),
                target: bid(&target),
            }),
            FilterJson::Scatter {
                seed,
                chance,
                flooronto,
                floor,
                block,
            } => Box::new(ScatterFilter {
                seed,
                chance,
                flooronto: bid(&flooronto),
                floor: bid(&floor),
                block: bid(&block),
            }),
            FilterJson::Terrain {
                seed,
                scl,
                threshold,
                octaves,
                falloff,
                magnitude,
                circle_scl,
                tilt,
                floor,
                block,
            } => Box::new(TerrainFilter {
                seed,
                scl,
                threshold,
                octaves,
                falloff,
                magnitude,
                circle_scl,
                tilt,
                floor: bid(&floor),
                block: bid(&block),
            }),
            FilterJson::Distort { seed, scl, mag } => Box::new(DistortFilter { seed, scl, mag }),
            FilterJson::RiverNoise {
                seed,
                scl,
                threshold,
                threshold2,
                octaves,
                falloff,
                floor,
                floor2,
                block: block_name_,
                target,
            } => Box::new(RiverNoiseFilter {
                seed,
                scl,
                threshold,
                threshold2,
                octaves,
                falloff,
                floor: bid(&floor),
                floor2: bid(&floor2),
                block: bid(&block_name_),
                target: bid(&target),
            }),
            FilterJson::Ore {
                seed,
                scl,
                threshold,
                octaves,
                falloff,
                tilt,
                ore,
                target,
            } => Box::new(OreFilter {
                seed,
                scl,
                threshold,
                octaves,
                falloff,
                tilt,
                ore: bid(&ore),
                target: bid(&target),
            }),
            FilterJson::OreMedian {
                seed,
                radius,
                percentile,
            } => Box::new(OreMedianFilter {
                seed,
                radius,
                percentile,
            }),
            FilterJson::Median {
                seed,
                radius,
                percentile,
            } => Box::new(MedianFilter {
                seed,
                radius,
                percentile,
            }),
            FilterJson::Blend {
                seed,
                radius,
                block: block_,
                floor,
                ignore,
            } => Box::new(BlendFilter {
                seed,
                radius,
                block: bid(&block_),
                floor: bid(&floor),
                ignore: bid(&ignore),
            }),
            FilterJson::Mirror {
                seed,
                angle,
                rotate,
                axis_x,
                axis_y,
            } => Box::new(MirrorFilter {
                seed,
                angle,
                rotate,
                axis_x,
                axis_y,
            }),
            FilterJson::Clear {
                seed,
                target,
                replace,
                ignore,
            } => Box::new(ClearFilter {
                seed,
                target: bid(&target),
                replace: bid(&replace),
                ignore: bid(&ignore),
            }),
            FilterJson::CoreSpawn { seed, amount } => Box::new(CoreSpawnFilter { seed, amount }),
            FilterJson::EnemySpawn { seed, amount } => Box::new(EnemySpawnFilter { seed, amount }),
            FilterJson::SpawnPath {
                seed,
                radius,
                block: block_,
            } => Box::new(SpawnPathFilter {
                seed,
                radius,
                block: bid(&block_),
            }),
            FilterJson::Logic {
                seed,
                code,
                loop_enabled,
            } => Box::new(LogicFilter {
                seed,
                code,
                loop_enabled,
            }),
        })
    }
}

/// Implements the shared `GenerateFilter` surface for a concrete filter.
macro_rules! impl_filter {
    ($ty:ty, $simple:literal, $tag:literal, $buffered:literal) => {
        impl GenerateFilter for $ty {
            fn simple_name(&self) -> &'static str {
                $simple
            }

            fn class_tag(&self) -> &'static str {
                $tag
            }

            fn seed(&self) -> i32 {
                self.seed
            }

            fn set_seed(&mut self, seed: i32) {
                self.seed = seed;
            }

            fn is_buffered(&self) -> bool {
                $buffered
            }

            fn apply(&mut self, input: &mut GenerateInput<'_>) {
                <$ty>::apply_impl(self, input);
            }

            fn to_json(&self, registry: &ContentRegistry) -> serde_json::Value {
                serde_json::to_value(self.to_filter_json(registry))
                    .unwrap_or(serde_json::Value::Null)
            }
        }

        impl $ty {
            /// The frozen JSON class tag.
            pub fn class_tag_static() -> &'static str {
                $tag
            }
        }
    };
}

/// `NoiseFilter`.
#[derive(Debug, Clone)]
pub struct NoiseFilter {
    /// Seed.
    pub seed: i32,
    /// Noise scale.
    pub scl: f32,
    /// Threshold.
    pub threshold: f32,
    /// Octaves.
    pub octaves: f32,
    /// Persistence.
    pub falloff: f32,
    /// Tilt.
    pub tilt: f32,
    /// Replacement floor.
    pub floor: BlockId,
    /// Replacement wall.
    pub block: BlockId,
    /// Target block (air = any).
    pub target: BlockId,
}

impl Default for NoiseFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            scl: 40.0,
            threshold: 0.5,
            octaves: 3.0,
            falloff: 0.5,
            tilt: 0.0,
            floor: BlockId::AIR,
            block: BlockId::AIR,
            target: BlockId::AIR,
        }
    }
}

impl NoiseFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        let n = filter_noise_xy(
            self.seed,
            input.x as f32,
            input.y as f32 + input.x as f32 * self.tilt,
            self.scl,
            1.0,
            self.octaves,
            self.falloff,
        );
        if n > self.threshold
            && (self.target == BlockId::AIR
                || input.floor == self.target
                || input.block == self.target)
        {
            if self.floor != BlockId::AIR {
                input.floor = self.floor;
            }
            if self.block != BlockId::AIR
                && input.block != BlockId::AIR
                && !input.block_synthetic(input.block)
            {
                input.block = self.block;
            }
        }
    }

    fn to_filter_json(&self, registry: &ContentRegistry) -> FilterJson {
        FilterJson::Noise {
            seed: self.seed,
            scl: self.scl,
            threshold: self.threshold,
            octaves: self.octaves,
            falloff: self.falloff,
            tilt: self.tilt,
            floor: block_name(registry, self.floor),
            block: block_name(registry, self.block),
            target: block_name(registry, self.target),
        }
    }
}

impl_filter!(NoiseFilter, "noise", "noise", false);

/// `ScatterFilter`.
#[derive(Debug, Clone)]
pub struct ScatterFilter {
    /// Seed.
    pub seed: i32,
    /// Placement chance.
    pub chance: f32,
    /// Required floor (air = any).
    pub flooronto: BlockId,
    /// Replacement floor.
    pub floor: BlockId,
    /// Wall or overlay to scatter.
    pub block: BlockId,
}

impl Default for ScatterFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            chance: 0.013,
            flooronto: BlockId::AIR,
            floor: BlockId::AIR,
            block: BlockId::AIR,
        }
    }
}

impl ScatterFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        let on_floor = input.floor == self.flooronto || self.flooronto == BlockId::AIR;
        if self.block != BlockId::AIR
            && on_floor
            && input.block == BlockId::AIR
            && filter_chance(input.x, input.y, self.seed) <= self.chance
        {
            if !input.block_is_overlay(self.block) {
                input.block = self.block;
            } else {
                input.overlay = self.block;
            }
        }
        if self.floor != BlockId::AIR
            && on_floor
            && filter_chance(input.x, input.y, self.seed) <= self.chance
        {
            input.floor = self.floor;
        }
    }

    fn to_filter_json(&self, registry: &ContentRegistry) -> FilterJson {
        FilterJson::Scatter {
            seed: self.seed,
            chance: self.chance,
            flooronto: block_name(registry, self.flooronto),
            floor: block_name(registry, self.floor),
            block: block_name(registry, self.block),
        }
    }
}

impl_filter!(ScatterFilter, "scatter", "scatter", false);

/// `TerrainFilter`.
#[derive(Debug, Clone)]
pub struct TerrainFilter {
    /// Seed.
    pub seed: i32,
    /// Noise scale.
    pub scl: f32,
    /// Threshold.
    pub threshold: f32,
    /// Octaves.
    pub octaves: f32,
    /// Persistence.
    pub falloff: f32,
    /// Noise magnitude.
    pub magnitude: f32,
    /// Circular falloff scale.
    pub circle_scl: f32,
    /// Tilt.
    pub tilt: f32,
    /// Replacement floor.
    pub floor: BlockId,
    /// Replacement wall.
    pub block: BlockId,
}

impl Default for TerrainFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            scl: 40.0,
            threshold: 0.9,
            octaves: 3.0,
            falloff: 0.5,
            magnitude: 1.0,
            circle_scl: 2.1,
            tilt: 0.0,
            floor: BlockId::AIR,
            block: BlockId::AIR,
        }
    }
}

impl TerrainFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        let nx = input.x as f32 / input.width.max(1) as f32;
        let ny = input.y as f32 / input.height.max(1) as f32;
        let circle = ((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt();
        let n = filter_noise_xy(
            self.seed,
            input.x as f32,
            input.y as f32 + input.x as f32 * self.tilt,
            self.scl,
            self.magnitude,
            self.octaves,
            self.falloff,
        ) + circle * self.circle_scl;

        if self.floor != BlockId::AIR {
            input.floor = self.floor;
        }
        if n >= self.threshold {
            input.block = self.block;
        }
    }

    fn to_filter_json(&self, registry: &ContentRegistry) -> FilterJson {
        FilterJson::Terrain {
            seed: self.seed,
            scl: self.scl,
            threshold: self.threshold,
            octaves: self.octaves,
            falloff: self.falloff,
            magnitude: self.magnitude,
            circle_scl: self.circle_scl,
            tilt: self.tilt,
            floor: block_name(registry, self.floor),
            block: block_name(registry, self.block),
        }
    }
}

impl_filter!(TerrainFilter, "terrain", "terrain", false);

/// `DistortFilter` (buffered).
#[derive(Debug, Clone)]
pub struct DistortFilter {
    /// Seed.
    pub seed: i32,
    /// Noise scale.
    pub scl: f32,
    /// Distortion magnitude.
    pub mag: f32,
}

impl Default for DistortFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            scl: 40.0,
            mag: 5.0,
        }
    }
}

impl DistortFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        let dx = filter_noise(input, self.seed, self.scl, self.mag) - self.mag / 2.0;
        let dy = filter_noise(input, self.seed + 1, self.scl, self.mag) - self.mag / 2.0;
        let tile = input.tile(input.x as f32 + dx, input.y as f32 + dy);
        input.floor = tile.floor;
        if !input.block_synthetic(tile.block) && !input.block_synthetic(input.block) {
            input.block = tile.block;
        }
        input.overlay = tile.overlay;
    }

    fn to_filter_json(&self, _registry: &ContentRegistry) -> FilterJson {
        FilterJson::Distort {
            seed: self.seed,
            scl: self.scl,
            mag: self.mag,
        }
    }
}

impl_filter!(DistortFilter, "distort", "distort", true);

/// `RiverNoiseFilter`.
#[derive(Debug, Clone)]
pub struct RiverNoiseFilter {
    /// Seed.
    pub seed: i32,
    /// Noise scale.
    pub scl: f32,
    /// Primary threshold.
    pub threshold: f32,
    /// River-core threshold.
    pub threshold2: f32,
    /// Octaves.
    pub octaves: f32,
    /// Persistence.
    pub falloff: f32,
    /// River floor.
    pub floor: BlockId,
    /// River-core floor.
    pub floor2: BlockId,
    /// River wall.
    pub block: BlockId,
    /// Target (air = any).
    pub target: BlockId,
}

impl Default for RiverNoiseFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            scl: 40.0,
            threshold: 0.0,
            threshold2: 0.1,
            octaves: 1.0,
            falloff: 0.5,
            floor: BlockId::AIR,
            floor2: BlockId::AIR,
            block: BlockId::AIR,
            target: BlockId::AIR,
        }
    }
}

impl RiverNoiseFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        let n = filter_rnoise_oct(
            self.seed,
            input.x as f32,
            input.y as f32,
            self.octaves as i32,
            self.scl,
            self.falloff,
            1.0,
        );
        if n >= self.threshold
            && (self.target == BlockId::AIR
                || input.floor == self.target
                || input.block == self.target)
        {
            if self.floor != BlockId::AIR {
                input.floor = self.floor;
            }
            if input.block_solid(input.block)
                && self.block != BlockId::AIR
                && input.block != BlockId::AIR
            {
                input.block = self.block;
            }
            if n >= self.threshold2 && self.floor2 != BlockId::AIR {
                input.floor = self.floor2;
            }
        }
    }

    fn to_filter_json(&self, registry: &ContentRegistry) -> FilterJson {
        FilterJson::RiverNoise {
            seed: self.seed,
            scl: self.scl,
            threshold: self.threshold,
            threshold2: self.threshold2,
            octaves: self.octaves,
            falloff: self.falloff,
            floor: block_name(registry, self.floor),
            floor2: block_name(registry, self.floor2),
            block: block_name(registry, self.block),
            target: block_name(registry, self.target),
        }
    }
}

impl_filter!(RiverNoiseFilter, "rivernoise", "riverNoise", false);

/// `OreFilter`.
#[derive(Debug, Clone)]
pub struct OreFilter {
    /// Seed.
    pub seed: i32,
    /// Noise scale.
    pub scl: f32,
    /// Threshold.
    pub threshold: f32,
    /// Octaves.
    pub octaves: f32,
    /// Persistence.
    pub falloff: f32,
    /// Tilt.
    pub tilt: f32,
    /// Ore overlay.
    pub ore: BlockId,
    /// Target (air = any).
    pub target: BlockId,
}

impl Default for OreFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            scl: 23.0,
            threshold: 0.81,
            octaves: 2.0,
            falloff: 0.3,
            tilt: 0.0,
            ore: BlockId::AIR,
            target: BlockId::AIR,
        }
    }
}

impl OreFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        let n = filter_noise_xy(
            self.seed,
            input.x as f32,
            input.y as f32 + input.x as f32 * self.tilt,
            self.scl,
            1.0,
            self.octaves,
            self.falloff,
        );
        let spawn = input.named("spawn").unwrap_or(BlockId::AIR);
        if n > self.threshold
            && input.overlay != spawn
            && (self.target == BlockId::AIR
                || input.floor == self.target
                || input.overlay == self.target)
            && input.floor_has_surface(input.floor)
        {
            input.overlay = self.ore;
        }
    }

    fn to_filter_json(&self, registry: &ContentRegistry) -> FilterJson {
        FilterJson::Ore {
            seed: self.seed,
            scl: self.scl,
            threshold: self.threshold,
            octaves: self.octaves,
            falloff: self.falloff,
            tilt: self.tilt,
            ore: block_name(registry, self.ore),
            target: block_name(registry, self.target),
        }
    }
}

impl_filter!(OreFilter, "ore", "ore", false);

/// `OreMedianFilter` (buffered).
#[derive(Debug, Clone)]
pub struct OreMedianFilter {
    /// Seed.
    pub seed: i32,
    /// Median radius.
    pub radius: f32,
    /// Percentile.
    pub percentile: f32,
}

impl Default for OreMedianFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            radius: 2.0,
            percentile: 0.5,
        }
    }
}

impl OreMedianFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        let spawn = input.named("spawn").unwrap_or(BlockId::AIR);
        if input.overlay == spawn {
            return;
        }

        let cx = (input.x / 2) * 2;
        let cy = (input.y / 2) * 2;
        if input.overlay != BlockId::AIR {
            let overlay = input.overlay;
            let t00 = input.tile(cx as f32, cy as f32);
            let t10 = input.tile((cx + 1) as f32, cy as f32);
            let t01 = input.tile(cx as f32, (cy + 1) as f32);
            let t11 = input.tile((cx + 1) as f32, (cy + 1) as f32);
            let uniform = t00.overlay == overlay
                && t10.overlay == overlay
                && t01.overlay == overlay
                && t11.overlay == overlay
                && !input.block_is_static(t00.block)
                && !input.block_is_static(t10.block)
                && !input.block_is_static(t01.block)
                && !input.block_is_static(t11.block);
            if !uniform {
                input.overlay = BlockId::AIR;
            }
        }

        let rad = self.radius as i32;
        let mut blocks: Vec<u16> = Vec::new();
        for x in -rad..=rad {
            for y in -rad..=rad {
                if x * x + y * y > rad * rad {
                    continue;
                }
                let tile = input.tile((input.x + x) as f32, (input.y + y) as f32);
                if tile.overlay != spawn {
                    blocks.push(tile.overlay.raw());
                }
            }
        }
        if blocks.is_empty() {
            input.overlay = BlockId::AIR;
            return;
        }
        blocks.sort_unstable();
        let index = (((blocks.len() as f32) * self.percentile) as i32)
            .min(blocks.len() as i32 - 1)
            .max(0);
        input.overlay = BlockId::new(blocks[index as usize]);
    }

    fn to_filter_json(&self, _registry: &ContentRegistry) -> FilterJson {
        FilterJson::OreMedian {
            seed: self.seed,
            radius: self.radius,
            percentile: self.percentile,
        }
    }
}

impl_filter!(OreMedianFilter, "oremedian", "oreMedian", true);

/// `MedianFilter` (buffered).
#[derive(Debug, Clone)]
pub struct MedianFilter {
    /// Seed.
    pub seed: i32,
    /// Median radius.
    pub radius: f32,
    /// Percentile.
    pub percentile: f32,
}

impl Default for MedianFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            radius: 2.0,
            percentile: 0.5,
        }
    }
}

impl MedianFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        let rad = self.radius as i32;
        let mut blocks: Vec<u16> = Vec::new();
        let mut floors: Vec<u16> = Vec::new();
        for x in -rad..=rad {
            for y in -rad..=rad {
                if x * x + y * y > rad * rad {
                    continue;
                }
                let tile = input.tile((input.x + x) as f32, (input.y + y) as f32);
                blocks.push(tile.block.raw());
                floors.push(tile.floor.raw());
            }
        }
        floors.sort_unstable();
        blocks.sort_unstable();
        let len = floors.len() as i32;
        let index = (((len as f32) * self.percentile) as i32)
            .min(len - 1)
            .max(0) as usize;
        input.floor = BlockId::new(floors[index]);
        let block = BlockId::new(blocks[index]);
        if !input.block_synthetic(block) && !input.block_synthetic(input.block) {
            input.block = block;
        }
    }

    fn to_filter_json(&self, _registry: &ContentRegistry) -> FilterJson {
        FilterJson::Median {
            seed: self.seed,
            radius: self.radius,
            percentile: self.percentile,
        }
    }
}

impl_filter!(MedianFilter, "median", "median", true);

/// `BlendFilter` (buffered).
#[derive(Debug, Clone)]
pub struct BlendFilter {
    /// Seed.
    pub seed: i32,
    /// Search radius.
    pub radius: f32,
    /// Source block to search for.
    pub block: BlockId,
    /// Replacement block/floor.
    pub floor: BlockId,
    /// Ignore block.
    pub ignore: BlockId,
}

impl Default for BlendFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            radius: 2.0,
            block: BlockId::AIR,
            floor: BlockId::AIR,
            ignore: BlockId::AIR,
        }
    }
}

impl BlendFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        if input.floor == self.block
            || self.block == BlockId::AIR
            || input.floor == self.ignore
            || (!input.block_is_floor(self.floor)
                && (input.block == self.block || input.block == self.ignore))
        {
            return;
        }

        let rad = self.radius as i32;
        let mut found = false;
        'outer: for x in -rad..=rad {
            for y in -rad..=rad {
                if x * x + y * y > rad * rad {
                    continue;
                }
                let tile = input.tile((input.x + x) as f32, (input.y + y) as f32);
                if tile.floor == self.block
                    || tile.block == self.block
                    || tile.overlay == self.block
                {
                    found = true;
                    break 'outer;
                }
            }
        }

        if found {
            if !input.block_is_floor(self.floor) {
                input.block = self.floor;
            } else {
                input.floor = self.floor;
            }
        }
    }

    fn to_filter_json(&self, registry: &ContentRegistry) -> FilterJson {
        FilterJson::Blend {
            seed: self.seed,
            radius: self.radius,
            block: block_name(registry, self.block),
            floor: block_name(registry, self.floor),
            ignore: block_name(registry, self.ignore),
        }
    }
}

impl_filter!(BlendFilter, "blend", "blend", true);

/// `MirrorFilter`.
#[derive(Debug, Clone)]
pub struct MirrorFilter {
    /// Seed.
    pub seed: i32,
    /// Mirror angle in degrees.
    pub angle: i32,
    /// Rotational (point) symmetry instead of reflection.
    pub rotate: bool,
    /// Normalized pivot x.
    pub axis_x: f32,
    /// Normalized pivot y.
    pub axis_y: f32,
}

impl Default for MirrorFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            angle: 45,
            rotate: false,
            axis_x: 0.5,
            axis_y: 0.5,
        }
    }
}

impl MirrorFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        let angle = (self.angle - 90) as f32;
        let mut v1 = Vec2f::trns_exact(angle, 1.0);
        let mut v2 = Vec2f { x: -v1.x, y: -v1.y };
        let pivot_x = coord(self.axis_x, input.width);
        let pivot_y = coord(self.axis_y, input.height);
        v1.x += pivot_x;
        v1.y += pivot_y;
        v2.x += pivot_x;
        v2.y += pivot_y;

        let mut p = Vec2f {
            x: input.x as f32,
            y: input.y as f32,
        };

        if !left(&v1, &v2, &p) {
            mirror(
                input.width,
                input.height,
                &mut p,
                v1.x,
                v1.y,
                v2.x,
                v2.y,
                self.angle,
                self.rotate,
            );
            let tile = input.tile(p.x, p.y);
            input.floor = tile.floor;
            if !input.block_synthetic(tile.block) {
                input.block = tile.block;
            }
            input.overlay = tile.overlay;
            input.packed_data = tile.packed_data;
        }
    }

    fn to_filter_json(&self, _registry: &ContentRegistry) -> FilterJson {
        FilterJson::Mirror {
            seed: self.seed,
            angle: self.angle,
            rotate: self.rotate,
            axis_x: self.axis_x,
            axis_y: self.axis_y,
        }
    }
}

impl_filter!(MirrorFilter, "mirror", "mirror", false);

/// `ClearFilter`.
#[derive(Debug, Clone)]
pub struct ClearFilter {
    /// Seed.
    pub seed: i32,
    /// Block to clear.
    pub target: BlockId,
    /// Replacement.
    pub replace: BlockId,
    /// Ignore block.
    pub ignore: BlockId,
}

impl Default for ClearFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            target: BlockId::AIR,
            replace: BlockId::AIR,
            ignore: BlockId::AIR,
        }
    }
}

impl ClearFilter {
    fn apply_impl(&mut self, input: &mut GenerateInput<'_>) {
        if self.ignore != BlockId::AIR
            && (input.block == self.ignore
                || input.floor == self.ignore
                || input.overlay == self.ignore)
        {
            return;
        }
        if input.block == self.target
            || input.floor == self.target
            || (input.block_is_overlay(self.target) && input.overlay == self.target)
        {
            if self.replace == BlockId::AIR {
                if input.overlay == self.target {
                    input.overlay = BlockId::AIR;
                } else {
                    input.block = BlockId::AIR;
                }
            } else if input.block_is_overlay(self.replace) {
                input.overlay = self.replace;
            } else if input.block_is_floor(self.replace) {
                input.floor = self.replace;
            } else {
                input.block = self.replace;
            }
        }
    }

    fn to_filter_json(&self, registry: &ContentRegistry) -> FilterJson {
        FilterJson::Clear {
            seed: self.seed,
            target: block_name(registry, self.target),
            replace: block_name(registry, self.replace),
            ignore: block_name(registry, self.ignore),
        }
    }
}

impl_filter!(ClearFilter, "clear", "clear", false);

/// `CoreSpawnFilter` (post; needs plan 12's team/rules + plan 07's buildings).
#[derive(Debug, Clone)]
pub struct CoreSpawnFilter {
    /// Seed.
    pub seed: i32,
    /// Cores to keep.
    pub amount: i32,
}

impl Default for CoreSpawnFilter {
    fn default() -> Self {
        Self { seed: 0, amount: 1 }
    }
}

impl GenerateFilter for CoreSpawnFilter {
    fn simple_name(&self) -> &'static str {
        "corespawn"
    }
    fn class_tag(&self) -> &'static str {
        "coreSpawn"
    }
    fn is_post(&self) -> bool {
        true
    }
    fn seed(&self) -> i32 {
        self.seed
    }
    fn set_seed(&mut self, seed: i32) {
        self.seed = seed;
    }
    fn to_json(&self, _registry: &ContentRegistry) -> serde_json::Value {
        serde_json::to_value(FilterJson::CoreSpawn {
            seed: self.seed,
            amount: self.amount,
        })
        .unwrap_or(serde_json::Value::Null)
    }
    fn apply_tiles(
        &mut self,
        _tiles: &mut Tiles,
        _input: &mut GenerateInput<'_>,
        _content: &ContentRegistry,
        _rng: &mut SimRng,
    ) {
        // Cores are buildings (plan 07) on team `rules.defaultTeam` (plan 12);
        // both land after 06, so this is a documented hook.
        log::debug!("CoreSpawnFilter is a plan-07/12 hook; no-op in mind-core M5");
    }
}

/// `EnemySpawnFilter` (post).
#[derive(Debug, Clone)]
pub struct EnemySpawnFilter {
    /// Seed.
    pub seed: i32,
    /// Spawns to keep.
    pub amount: i32,
}

impl Default for EnemySpawnFilter {
    fn default() -> Self {
        Self { seed: 0, amount: 1 }
    }
}

impl GenerateFilter for EnemySpawnFilter {
    fn simple_name(&self) -> &'static str {
        "enemyspawn"
    }
    fn class_tag(&self) -> &'static str {
        "enemySpawn"
    }
    fn is_post(&self) -> bool {
        true
    }
    fn seed(&self) -> i32 {
        self.seed
    }
    fn set_seed(&mut self, seed: i32) {
        self.seed = seed;
    }
    fn to_json(&self, _registry: &ContentRegistry) -> serde_json::Value {
        serde_json::to_value(FilterJson::EnemySpawn {
            seed: self.seed,
            amount: self.amount,
        })
        .unwrap_or(serde_json::Value::Null)
    }
    fn apply_tiles(
        &mut self,
        tiles: &mut Tiles,
        _input: &mut GenerateInput<'_>,
        content: &ContentRegistry,
        rng: &mut SimRng,
    ) {
        let Some(spawn) = content.block_id("spawn") else {
            return;
        };
        let mut spawns: Vec<usize> = Vec::new();
        for index in 0..tiles.len() {
            if tiles.geti(index).overlay == spawn {
                spawns.push(index);
            }
        }
        // Deterministic Fisher-Yates on the MapGen stream (deviation §2.3.5).
        for i in (1..spawns.len()).rev() {
            let j = rng.random(RngStream::MapGen, (i + 1) as i32) as usize;
            spawns.swap(i, j);
        }
        let used = (spawns.len() as i32).min(self.amount).max(0) as usize;
        for index in spawns.into_iter().skip(used) {
            tiles.geti_mut(index).overlay = BlockId::AIR;
        }
    }
}

/// `SpawnPathFilter` (post; needs plan 11's A* + plan 12's teams).
#[derive(Debug, Clone)]
pub struct SpawnPathFilter {
    /// Seed.
    pub seed: i32,
    /// Path radius.
    pub radius: i32,
    /// Wall to place.
    pub block: BlockId,
}

impl Default for SpawnPathFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            radius: 3,
            block: BlockId::AIR,
        }
    }
}

impl GenerateFilter for SpawnPathFilter {
    fn simple_name(&self) -> &'static str {
        "spawnpath"
    }
    fn class_tag(&self) -> &'static str {
        "spawnPath"
    }
    fn is_post(&self) -> bool {
        true
    }
    fn seed(&self) -> i32 {
        self.seed
    }
    fn set_seed(&mut self, seed: i32) {
        self.seed = seed;
    }
    fn to_json(&self, registry: &ContentRegistry) -> serde_json::Value {
        serde_json::to_value(FilterJson::SpawnPath {
            seed: self.seed,
            radius: self.radius,
            block: block_name(registry, self.block),
        })
        .unwrap_or(serde_json::Value::Null)
    }
    fn apply_tiles(
        &mut self,
        _tiles: &mut Tiles,
        _input: &mut GenerateInput<'_>,
        _content: &ContentRegistry,
        _rng: &mut SimRng,
    ) {
        log::debug!("SpawnPathFilter is a plan-11/12 hook; no-op in mind-core M5");
    }
}

/// `LogicFilter` (post; needs plan 13's script runner).
#[derive(Debug, Clone, Default)]
pub struct LogicFilter {
    /// Seed.
    pub seed: i32,
    /// Logic code.
    pub code: Option<String>,
    /// Loop the script.
    pub loop_enabled: bool,
}

impl GenerateFilter for LogicFilter {
    fn simple_name(&self) -> &'static str {
        "logic"
    }
    fn class_tag(&self) -> &'static str {
        "logic"
    }
    fn is_post(&self) -> bool {
        true
    }
    fn seed(&self) -> i32 {
        self.seed
    }
    fn set_seed(&mut self, seed: i32) {
        self.seed = seed;
    }
    fn to_json(&self, _registry: &ContentRegistry) -> serde_json::Value {
        serde_json::to_value(FilterJson::Logic {
            seed: self.seed,
            code: self.code.clone(),
            loop_enabled: self.loop_enabled,
        })
        .unwrap_or(serde_json::Value::Null)
    }
    fn apply_tiles(
        &mut self,
        tiles: &mut Tiles,
        _input: &mut GenerateInput<'_>,
        content: &ContentRegistry,
        _rng: &mut SimRng,
    ) {
        let Some(code) = self.code.clone() else {
            return;
        };
        if code.is_empty() {
            return;
        }

        // Upstream `LogicFilter.apply` ignores the filter's local `Tiles` and
        // runs a privileged script against the live generation `World`
        // (`LExecutor.runLogicScript`). Mirror that: move the in-progress grid
        // into a temporary `WorldGrid` resource so `getblock`/`setblock` read
        // and mutate the same tiles, then copy the result back.
        let owned = std::mem::take(tiles);
        let width = owned.width;
        let height = owned.height;
        let mut grid = crate::world::WorldGrid::new(width, height);
        grid.tiles = owned;

        let mut world = bevy_ecs::world::World::new();
        world.insert_resource(grid);
        world.insert_resource(crate::logic::globals::GlobalVars::with_content(content));
        world.insert_resource(crate::logic::world::LogicContentIndex::from_content(
            content,
        ));
        let mut state = crate::logic::world::LogicWorldState::new();
        state.is_host = true;
        state.map_width = width;
        state.map_height = height;
        world.insert_resource(state);

        let _ = crate::logic::script::run_logic_filter(&code, self.loop_enabled, &mut world);

        if let Some(grid) = world.remove_resource::<crate::world::WorldGrid>() {
            *tiles = grid.tiles;
        }
    }
}

/// `RandomItemFilter` (ported; not registered, matching upstream).
#[derive(Debug, Clone)]
pub struct RandomItemFilter {
    /// Seed.
    pub seed: i32,
    /// Item drops (name/id pairs are plan 02 items; runtime in plan 07/08).
    pub drops: Vec<(String, i32)>,
    /// Drop chance.
    pub chance: f32,
}

impl Default for RandomItemFilter {
    fn default() -> Self {
        Self {
            seed: 0,
            drops: Vec::new(),
            chance: 0.3,
        }
    }
}

impl GenerateFilter for RandomItemFilter {
    fn simple_name(&self) -> &'static str {
        "randomitem"
    }
    fn class_tag(&self) -> &'static str {
        "randomItem"
    }
    fn is_post(&self) -> bool {
        true
    }
    fn seed(&self) -> i32 {
        self.seed
    }
    fn set_seed(&mut self, seed: i32) {
        self.seed = seed;
    }
    fn to_json(&self, _registry: &ContentRegistry) -> serde_json::Value {
        serde_json::json!({
            "class": self.class_tag(),
            "seed": self.seed,
            "chance": self.chance,
        })
    }
    fn apply_tiles(
        &mut self,
        _tiles: &mut Tiles,
        _input: &mut GenerateInput<'_>,
        _content: &ContentRegistry,
        _rng: &mut SimRng,
    ) {
        log::debug!("RandomItemFilter is a plan-07/08 hook; no-op in mind-core M5");
    }
}

/// Small local 2D vector for the mirror math (`arc.math.geom.Vec2`).
#[derive(Debug, Clone, Copy)]
struct Vec2f {
    x: f32,
    y: f32,
}

impl Vec2f {
    fn trns_exact(degrees: f32, len: f32) -> Vec2f {
        let radians = degrees.to_radians();
        Vec2f {
            x: radians.cos() * len,
            y: radians.sin() * len,
        }
    }
}

fn coord(axis: f32, size: i32) -> f32 {
    if size <= 1 {
        0.0
    } else {
        axis.clamp(0.0, 1.0) * (size - 1) as f32
    }
}

fn left(a: &Vec2f, b: &Vec2f, c: &Vec2f) -> bool {
    (b.x - a.x) * (c.y - a.y) > (b.y - a.y) * (c.x - a.x)
}

#[allow(clippy::too_many_arguments)]
fn mirror(
    width: i32,
    height: i32,
    p: &mut Vec2f,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    angle: i32,
    rotate: bool,
) {
    if (width != height && angle % 90 != 0) || rotate {
        p.x = width as f32 - p.x - 1.0;
        p.y = height as f32 - p.y - 1.0;
    } else {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let a = (dx * dx - dy * dy) / (dx * dx + dy * dy);
        let b = 2.0 * dx * dy / (dx * dx + dy * dy);
        let px = a * (p.x - x0) + b * (p.y - y0) + x0;
        let py = b * (p.x - x0) - a * (p.y - y0) + y0;
        p.x = px;
        p.y = py;
    }
}

/// Named-block helper used by [`GenerateInput`] convenience methods.
impl GenerateInput<'_> {
    /// Resolves a content block name.
    pub fn named(&self, name: &str) -> Option<BlockId> {
        self.content.and_then(|content| content.block_id(name))
    }
}
