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

pub(super) fn place(sim: &mut Sim, name: &str, x: i16, y: i16) -> Entity {
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

pub(super) fn item(sim: &Sim, name: &str) -> crate::content::ItemId {
    sim.block_runtime()
        .expect("runtime")
        .content()
        .item_id(name)
        .expect("item")
}

pub(super) fn liquid(sim: &Sim, name: &str) -> crate::content::LiquidId {
    sim.block_runtime()
        .expect("runtime")
        .content()
        .liquid_id(name)
        .expect("liquid")
}

pub(super) fn set_floor(sim: &mut Sim, x: i32, y: i32, name: &str) {
    let floor = BlockId::new(sim.world_block_id(name).expect("floor"));
    sim.grid.tiles.get_mut(x, y).floor = floor;
}

pub(super) fn stored_liquid(sim: &Sim, e: Entity, liquid: crate::content::LiquidId) -> f32 {
    sim.ecs
        .0
        .get::<crate::world::modules::LiquidModule>(e)
        .map(|m| m.get(liquid))
        .unwrap_or(0.0)
}

pub(super) fn runtime_sim() -> Sim {
    let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime().expect("runtime");
    sim
}

pub(super) fn configure(sim: &mut Sim, x: i16, y: i16, value: crate::determinism::ConfigValue) {
    sim.command(SimCommand::Configure { x, y, value })
        .expect("configure");
}

pub(super) fn stored_item(sim: &Sim, e: Entity, item: crate::content::ItemId) -> i32 {
    sim.ecs
        .0
        .get::<crate::world::modules::ItemModule>(e)
        .map(|module| module.get(item))
        .unwrap_or(0)
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

/// Builds a runtime over a caller-patched content registry (the liquid-booster
/// case has no vanilla block, so the test injects the consumer itself).
pub(super) fn runtime_sim_with_content(content: crate::content::ContentRegistry) -> Sim {
    let mut sim = Sim::new(3, 32, 32, BlockId::AIR, BlockId::AIR);
    let stone = BlockId::new(sim.world_block_id("stone").expect("stone"));
    sim.grid.fill(stone, BlockId::AIR);
    sim.install_block_runtime_from(crate::sim::runtime::SimRuntimeConfig::default(), content)
        .expect("runtime");
    sim
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

/// Installs a campaign session (the live rules/markers/wave source) on `sim`.
pub(super) fn install_campaign_session(sim: &mut Sim, rules: crate::game::rules::Rules) {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    let content = crate::world::harness::BuildHarness::load_content();
    let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
}

/// Live-client logic resources (what `gdext` `install_sim_seams` inserts).
pub(super) fn install_logic_world(sim: &mut Sim) {
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicWorldState {
            headless: false,
            ..crate::logic::world::LogicWorldState::new()
        });
    sim.ecs
        .0
        .insert_resource(crate::logic::world::MessageState::default());
    let content = crate::world::harness::BuildHarness::load_content();
    sim.ecs
        .0
        .insert_resource(crate::logic::world::LogicContentIndex::from_content(
            &content,
        ));
}

/// Places a configurable world processor running `code`.
pub(super) fn place_world_processor(sim: &mut Sim, x: i16, y: i16, code: &str) {
    let id = sim
        .world_block_id("world-processor")
        .expect("world-processor");
    sim.command(SimCommand::Place {
        x,
        y,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    let bytes = crate::logic::blocks::logic_block::compress(code, &[]);
    configure(sim, x, y, crate::determinism::ConfigValue::Bytes(bytes));
}

pub(super) fn test_rules() -> crate::game::rules::Rules {
    crate::game::rules::Rules {
        default_team: 1,
        allow_edit_world_processors: true,
        ..Default::default()
    }
}

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
