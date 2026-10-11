// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime production/logistics regressions (PAR-0075/0076/0077/0078):
//! blocks placed through `Sim` with the block runtime installed.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::world::TilePos;

use super::Sim;
use crate::determinism::SimCommand;
use crate::world::blocks::distribution::transfer;

fn place(sim: &mut Sim, name: &str, x: i16, y: i16) -> Entity {
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

fn item(sim: &Sim, name: &str) -> crate::content::ItemId {
    sim.block_runtime()
        .expect("runtime")
        .content()
        .item_id(name)
        .expect("item")
}

/// PAR-0075: a combustion generator accepts coal (not copper) and makes
/// power from it.
#[test]
fn combustion_generator_accepts_coal_and_generates() {
    let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    let generator = place(&mut sim, "combustion-generator", 8, 8);
    let coal = item(&sim, "coal");
    let copper = item(&sim, "copper");
    let world = &mut sim.ecs.0;
    assert!(transfer::dispatch_accept_item(
        world, generator, generator, coal
    ));
    assert!(!transfer::dispatch_accept_item(
        world, generator, generator, copper
    ));
    transfer::dispatch_handle_item(world, generator, generator, coal);
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let production = sim
        .ecs
        .0
        .get::<crate::world::blocks::power::PowerProduction>(generator)
        .map(|p| p.0)
        .unwrap_or(0.0);
    assert!(production > 0.0, "production {production}");
}
