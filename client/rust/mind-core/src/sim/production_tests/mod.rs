// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-runtime production/logistics regressions (PAR-0075/0076/0077/0078):
//! blocks placed through `Sim` with the block runtime installed.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::determinism::SimCommand;
use crate::world::TilePos;
use crate::world::blocks::distribution::transfer;

use super::Sim;

mod campaign_accelerator;
mod configure;
mod core_placement_rules;
mod crafting_power;
mod extractors_drills;
mod factories;
mod fill_items_stats;
mod generators;
mod heat_attribute_crafters;
mod kinds_env;
mod liquids_sources;
mod nodes_menders_sounds;
mod sensor_control;
mod t14_blocks;
mod wall_crafters;
mod world_processor_a;
mod world_processor_b;

pub(super) fn set_floor(sim: &mut Sim, x: i32, y: i32, name: &str) {
    let floor = BlockId::new(sim.world_block_id(name).expect("floor"));
    sim.grid.tiles.get_mut(x, y).floor = floor;
}

pub(super) fn emitted(sim: &Sim, e: Entity) -> u32 {
    sim.ecs
        .0
        .get::<crate::entities::comp::SandboxState>(e)
        .map(|state| state.emitted)
        .unwrap_or(0)
}

/// Sets the block at `(x, y)` to a content block by name.
pub(super) fn set_block(sim: &mut Sim, x: i32, y: i32, name: &str) {
    let block = BlockId::new(sim.world_block_id(name).expect("block"));
    sim.grid.tiles.get_mut(x, y).block = block;
}

/// Re-runs `WallCrafter.getEfficiency` for `e` (the per-tick facing pass needs
/// the grid, which the behavior cannot see).
pub(super) fn refresh_wall_crafter(sim: &mut Sim, e: Entity) {
    let crate::sim::Sim {
        grid,
        ecs,
        block_runtime,
        ..
    } = sim;
    let content = block_runtime.as_ref().expect("runtime").content();
    crate::world::behavior::production::refresh_drill_ore(&mut ecs.0, grid, content, e);
}

/// Clones the drill state (progress/warmup/lastEfficiency/totalTime).
pub(super) fn wall_crafter_state(sim: &Sim, e: Entity) -> crate::entities::comp::DrillState {
    sim.ecs
        .0
        .get::<crate::entities::comp::DrillState>(e)
        .cloned()
        .expect("drill state")
}

pub(super) fn building_efficiency(sim: &Sim, e: Entity) -> f32 {
    sim.ecs
        .0
        .get::<crate::entities::comp::Building>(e)
        .map(|building| building.efficiency)
        .unwrap_or(0.0)
}

/// Ticks until the power graph is live (`efficiency > 0`). A fresh building
/// starts at `efficiency = 1` before its first consumption pass, so this always
/// advances at least one tick.
pub(super) fn tick_until_efficient(sim: &mut Sim, e: Entity) {
    for _ in 0..10 {
        sim.tick().expect("tick");
        if building_efficiency(sim, e) > 0.0 {
            return;
        }
    }
    panic!("building never reached a live power graph");
}

/// `BlockId` of a content block by name through the live runtime table.
pub(super) fn crusher_block_id(sim: &Sim, name: &str) -> crate::content::BlockId {
    sim.block_runtime()
        .expect("runtime")
        .content()
        .block_id(name)
        .expect("block")
}

/// Whether a unit of type `unit` exists in the sim.
pub(super) fn spawned_unit(sim: &Sim, unit: crate::content::UnitTypeId) -> bool {
    sim.ecs.0.iter_entities().any(|entity| {
        entity
            .get::<crate::entities::comp::unit::UnitTypeComp>()
            .map(|comp| comp.type_id)
            == Some(unit)
    })
}

// ---- G06 T4 (PAR-0104): live world-processor bridge ----

/// H02 T3: a placement query with the full flag set for the live world.
#[cfg(test)]
pub(super) fn place_query(
    sim: &Sim,
    x: i16,
    y: i16,
    block: &str,
    rot: u8,
    team: u8,
    flags: crate::world::build::PlaceFlags,
) -> bool {
    let runtime = sim.block_runtime().expect("runtime");
    let rules = sim
        .ecs
        .0
        .get_resource::<crate::world::limits::BuildRules>()
        .cloned()
        .unwrap_or_default();
    let table = sim
        .ecs
        .0
        .get_resource::<crate::world::BlockTable>()
        .expect("table");
    let counter = sim
        .ecs
        .0
        .get_resource::<crate::world::limits::BlockCounter>()
        .expect("counter");
    let id = runtime.content().block_id(block).expect("block");
    crate::world::build::valid_place_query(
        runtime.content(),
        table,
        &rules,
        counter,
        &sim.grid,
        Some(&sim.ecs.0),
        id,
        team,
        rot,
        i32::from(x),
        i32::from(y),
        flags,
    )
}

pub(super) fn place_team(sim: &mut Sim, x: i16, y: i16, block: &str, rot: i8, team: u8) -> Entity {
    let id = sim.world_block_id(block).expect("block");
    sim.command(SimCommand::Place {
        x,
        y,
        block: id,
        rotation: rot,
        team,
        player: None,
    })
    .expect("place");
    sim.grid
        .tiles
        .get(i32::from(x), i32::from(y))
        .build
        .expect("built")
}
