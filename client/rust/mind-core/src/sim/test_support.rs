// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Shared `sim` test fixtures copied verbatim from the per-module originals
//! (`combat_tests`, `production_tests`) until W2 adopts them.
//! Upstream: none (port-only test support).
#![allow(dead_code)] // adopted in W2

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::determinism::SimCommand;
use crate::world::TilePos;

use super::Sim;

/// AIR floor/overlay, seed `seed`, block + unit runtime installed.
/// `live_sim_seeded(1, n)` == `combat_tests::live_sim(n)` (`combat_tests.rs:17`).
pub(crate) fn live_sim_seeded(seed: u64, size: i32) -> Sim {
    let mut sim = Sim::new(seed, size, size, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim
}

/// The combat fixture: `live_sim_seeded(1, n)`.
pub(crate) fn live_sim(size: i32) -> Sim {
    live_sim_seeded(1, size)
}

/// Block id via the **unit runtime's** content (`combat_tests::block`, :24).
/// NOT `Sim::world_block_id`, which takes `&mut self` and lazily installs
/// `WorldApplyRuntime` (that changes `world_apply_checksum`).
pub(crate) fn unit_content_block(sim: &Sim, name: &str) -> u16 {
    sim.unit_runtime()
        .expect("runtime")
        .content
        .block_id(name)
        .expect("block")
        .get()
}

/// `combat_tests::place` (:33): SimCommand::Place, rotation 0, player None;
/// returns `tiles.get(x,y).build`.
pub(crate) fn place_at(sim: &mut Sim, x: i16, y: i16, name: &str, team: u8) -> Entity {
    let block = unit_content_block(sim, name);
    sim.command(SimCommand::Place {
        x,
        y,
        block,
        rotation: 0,
        team,
        player: None,
    })
    .expect("place");
    sim.grid
        .tiles
        .get(x as i32, y as i32)
        .build
        .expect("placed building")
}

/// `production_tests::place` (:16): team 1, argument order (name, x, y);
/// returns via `grid.entity_at`.
pub(crate) fn place_team1(sim: &mut Sim, name: &str, x: i16, y: i16) -> Entity {
    let id = sim.world_block_id(name).expect("block");
    sim.command(SimCommand::Place {
        x,
        y,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    sim.grid.entity_at(TilePos::new(x, y)).expect("entity")
}

/// `production_tests::runtime_sim` (:445): seed 3, stone fill, block runtime
/// only.
pub(crate) fn runtime_sim() -> Sim {
    let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    sim
}

/// `for _ in 0..n { sim.tick().expect("tick"); }` (101 inline copies in
/// `combat_tests`, 117 in `production_tests`).
pub(crate) fn tick_n(sim: &mut Sim, n: usize) {
    for _ in 0..n {
        sim.tick().expect("tick");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The helper and the original `combat_tests::live_sim` must build the same
    /// sim: equal checksums after 60 ticks.
    #[test]
    fn live_sim_matches_combat_fixture() {
        let mut helper = live_sim(48);
        let mut original = super::super::combat_tests::live_sim(48);
        for _ in 0..60 {
            helper.tick().expect("tick");
            original.tick().expect("tick");
        }
        assert_eq!(helper.checksum(), original.checksum());
    }

    /// The helper and the original `production_tests::runtime_sim` must build
    /// the same sim: equal checksums after 60 ticks.
    #[test]
    fn runtime_sim_matches_production_fixture() {
        let mut helper = runtime_sim();
        let mut original = super::super::production_tests::runtime_sim();
        for _ in 0..60 {
            helper.tick().expect("tick");
            original.tick().expect("tick");
        }
        assert_eq!(helper.checksum(), original.checksum());
    }
}
