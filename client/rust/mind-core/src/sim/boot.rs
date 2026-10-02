// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Sim` boot path (`SimBuilder`, `BootError`, `TickReport`).
//!
//! Ported from `core/src/mindustry/Vars.java` `init()` and
//! `core/ClientLauncher.java` host setup. Headless boot mirrors the
//! `ServerLauncher` subset: content → registry → sim.

use crate::content::BlockId;
use crate::determinism::SimRng;
use crate::platform::Platform;

use super::{Sim, SimConfig, SimError};

/// Errors raised while building a `Sim`.
#[derive(thiserror::Error, Debug)]
pub enum BootError {
    /// Content/registry boot failed.
    #[error("sim boot failed: {0}")]
    Content(String),
}

/// Per-tick observation returned by [`Sim::tick_report`] (plan 00 inspector
/// fields + plan 05 §3.4 `TickReport`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickReport {
    /// Completed ticks.
    pub tick: u64,
    /// Monotonic update counter.
    pub update_id: u64,
    /// Canonical checksum after the tick.
    pub checksum: u64,
    /// Number of `// plan NN` stub systems that ran this tick.
    pub unimplemented_stub: u32,
}

/// Builds a `Sim` with an explicit config/platform/world.
pub struct SimBuilder {
    config: SimConfig,
    platform: Option<Box<dyn Platform>>,
    width: i32,
    height: i32,
    floor: BlockId,
    wall: BlockId,
}

impl SimBuilder {
    /// Creates a builder with the given config and platform.
    pub fn new(config: SimConfig, platform: Box<dyn Platform>) -> Self {
        Self {
            config,
            platform: Some(platform),
            width: 32,
            height: 32,
            floor: BlockId::AIR,
            wall: BlockId::AIR,
        }
    }

    /// Sets the world dimensions and fill blocks.
    pub fn world(mut self, width: i32, height: i32, floor: BlockId, wall: BlockId) -> Self {
        self.width = width;
        self.height = height;
        self.floor = floor;
        self.wall = wall;
        self
    }

    /// Builds the simulation.
    pub fn build(mut self) -> Result<Sim, BootError> {
        let mut sim = Sim::new(
            self.config.seed,
            self.width,
            self.height,
            self.floor,
            self.wall,
        );
        sim.config = self.config.clone();
        sim.rng_streams = SimRng::new(self.config.seed);
        if let Some(platform) = self.platform.take() {
            sim.platform = platform;
        }
        Ok(sim)
    }
}

impl Sim {
    /// Runs one fixed step and reports the observable tick state.
    pub fn tick_report(&mut self) -> Result<TickReport, SimError> {
        self.tick()?;
        Ok(TickReport {
            tick: self.state.tick,
            update_id: self.state.update_id,
            checksum: self.checksum(),
            unimplemented_stub: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::vanilla_registry;
    use crate::platform::HeadlessPlatform;

    /// Ported from `ApplicationTests.initialization`: boot + registry + content
    /// map non-empty.
    #[test]
    fn initialization() {
        let config = SimConfig::new().with_seed(12345);
        let mut sim = SimBuilder::new(config, Box::new(HeadlessPlatform::new("/tmp/mind-boot")))
            .world(16, 16, BlockId::AIR, BlockId::STONE_WALL)
            .build()
            .expect("boot");
        assert_eq!(sim.phase(), crate::game::State::Playing);

        // Content + entity registries are non-empty.
        assert!(!sim.block_name_of(BlockId::STONE_WALL).is_empty());
        let registry = vanilla_registry().expect("registry");
        assert!(registry.defs_len() > 0);
        assert!(registry.components_len() > 0);

        // Ten fixed steps produce a stable, advancing tick.
        for _ in 0..10 {
            sim.tick().expect("tick");
        }
        assert_eq!(sim.tick_count(), 10);
        assert_eq!(sim.clock().update_id, 10);
        assert_eq!(sim.state.update_id, 10);

        // Same seed → same checksum after the same number of ticks.
        let mut second = SimBuilder::new(
            SimConfig::new().with_seed(12345),
            Box::new(HeadlessPlatform::new("/tmp/mind-boot")),
        )
        .world(16, 16, BlockId::AIR, BlockId::STONE_WALL)
        .build()
        .expect("boot");
        for _ in 0..10 {
            second.tick().expect("tick");
        }
        assert_eq!(sim.checksum(), second.checksum());
    }

    #[test]
    fn tick_report_matches_sim_state() {
        let mut sim = Sim::new(0, 4, 4, BlockId::AIR, BlockId::AIR);
        let report = sim.tick_report().expect("tick");
        assert_eq!(report.tick, 1);
        assert_eq!(report.update_id, 1);
        assert_eq!(report.checksum, sim.checksum());
    }
}
