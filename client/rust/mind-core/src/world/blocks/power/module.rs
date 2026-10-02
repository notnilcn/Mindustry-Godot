// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Power module accessors and connections
//! (`entities/comp/BuildingComp.java` power paths + `world/modules/PowerModule`).
//!
//! Plan 07 owns the `PowerModule` data layout; plan 09 implements the network
//! operations over ECS entities. Block metadata (`BlockDef.outputs_power`,
//! `consumes_power`, `conductive_power`, `insulated`, the single `ConsumePower`)
//! is read through [`crate::world::block::BlockTable`]. Fixtures and synthetic
//! tests may instead attach a [`PowerNodeInfo`] override so the graph algorithms
//! stay content-free.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::entities::comp::{Building, TeamComp};
use crate::world::WorldGrid;
use crate::world::block::BlockTable;
use crate::world::modules::{PowerGraphId, PowerModule};

use super::{PowerNodeInfo, PowerProduction};

/// Resolved power parameters for one building (`Building` + `Block` + module).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerParams {
    /// `Block.hasPower`.
    pub has_power: bool,
    /// `Block.outputsPower`.
    pub outputs: bool,
    /// `Block.consumesPower`.
    pub consumes: bool,
    /// `ConsumePower.buffered`.
    pub buffered: bool,
    /// `ConsumePower.capacity`.
    pub capacity: f32,
    /// `ConsumePower.usage`.
    pub usage: f32,
    /// `Block.conductivePower`.
    pub conductive: bool,
    /// `Block.insulated`.
    pub insulated: bool,
    /// `Building.enabled`.
    pub enabled: bool,
    /// `TeamComp.team`.
    pub team: u8,
    /// `Building.timeScale`.
    pub time_scale: f32,
    /// `Building.shouldConsumePower` (`updateConsumption` output).
    pub should_consume_power: bool,
    /// `getPowerProduction()` (behavior-supplied via [`PowerProduction`]).
    pub production: f32,
    /// Owning graph handle stored on the module.
    pub graph: PowerGraphId,
    /// Whether the module graph was initialized.
    pub init: bool,
}

/// Resolves a building's power parameters, preferring a [`PowerNodeInfo`]
/// override (fixtures/synthetic content) over `BlockDef` metadata.
pub fn read_power_info(world: &World, entity: Entity) -> Option<PowerParams> {
    let module = world.get::<PowerModule>(entity)?;
    let building = world.get::<Building>(entity)?;
    let team = world
        .get::<TeamComp>(entity)
        .map(|team| team.team)
        .unwrap_or(0);
    let production = world
        .get::<PowerProduction>(entity)
        .map(|production| production.0)
        .unwrap_or(0.0);

    let (outputs, consumes, buffered, capacity, usage, conductive, insulated) =
        if let Some(info) = world.get::<PowerNodeInfo>(entity) {
            (
                info.outputs,
                info.consumes,
                info.buffered,
                info.capacity,
                info.usage,
                info.conductive,
                info.insulated,
            )
        } else {
            let def = world
                .get_resource::<BlockTable>()
                .and_then(|table| table.get(building.block))
                .map(|instance| instance.def.clone())?;
            let (buffered, capacity, usage) = def
                .cons_power
                .and_then(|index| def.consumes.get(index))
                .map(|consumer| match &consumer.consume {
                    crate::content::Consume::Power { usage, buffered } => {
                        (*buffered > 0.0, *buffered, *usage)
                    }
                    _ => (false, 0.0, 0.0),
                })
                .unwrap_or((false, 0.0, 0.0));
            (
                def.outputs_power,
                def.consumes_power,
                buffered,
                capacity,
                usage,
                def.conductive_power,
                def.insulated,
            )
        };

    Some(PowerParams {
        has_power: true,
        outputs,
        consumes,
        buffered,
        capacity,
        usage,
        conductive,
        insulated,
        enabled: building.enabled,
        team,
        time_scale: building.time_scale,
        should_consume_power: building.should_consume_power,
        production,
        graph: module.graph,
        init: module.init,
    })
}

/// `Building.getPowerConnections(out)`.
///
/// Proximity neighbors first (`Edges` order), then configured links. The filter
/// is exactly the Java predicate (plan 09 §3.6): same team, power present,
/// not two mutually-unpowered consumers, both directions conductive, and the
/// link not already configured.
pub fn get_power_connections(
    world: &World,
    grid: &WorldGrid,
    entity: Entity,
    out: &mut Vec<Entity>,
) {
    out.clear();
    let Some(module) = world.get::<PowerModule>(entity) else {
        return;
    };
    let Some(params) = read_power_info(world, entity) else {
        return;
    };

    if let Some(building) = world.get::<Building>(entity) {
        for &other in &building.proximity {
            let Some(other_params) = read_power_info(world, other) else {
                continue;
            };
            if other_params.team != params.team {
                continue;
            }
            if params.consumes
                && other_params.consumes
                && !params.outputs
                && !other_params.outputs
                && !params.conductive
                && !other_params.conductive
            {
                continue;
            }
            if params.insulated || other_params.insulated {
                continue;
            }
            if module.links.contains(&pack(world, other)) {
                continue;
            }
            if !out.contains(&other) {
                out.push(other);
            }
        }
    }

    for &link in &module.links {
        let pos = crate::world::TilePos::from_pack(link);
        let Some(other) = grid.entity_at(pos) else {
            continue;
        };
        let Some(other_params) = read_power_info(world, other) else {
            continue;
        };
        if other_params.team != params.team {
            continue;
        }
        if !out.contains(&other) {
            out.push(other);
        }
    }
}

/// Packed tile position of a building (Java `Building.pos()`).
pub fn pack(world: &World, entity: Entity) -> i32 {
    world
        .get::<Building>(entity)
        .map(|building| building.tile.pack())
        .unwrap_or(-1)
}

/// `Building.updatePowerGraph()`: merge each connected building's graph.
///
/// Seeds a fresh graph when the building has none (Java's per-building default
/// graph), then merges neighbor graphs. `module.graph` is re-read each
/// iteration because a merge may reassign it (Java does the same).
pub fn update_power_graph(
    graphs: &mut super::graph::PowerGrids,
    world: &mut World,
    grid: &WorldGrid,
    entity: Entity,
) {
    let current = world.get::<PowerModule>(entity).map(|module| module.graph);
    let own = match current {
        Some(graph) if graph != PowerGraphId::NONE && graphs.contains(graph) => graph,
        _ => {
            let graph = graphs.alloc();
            graphs.add(world, graph, entity);
            graph
        }
    };
    graphs.check_add(world, own);

    let mut connections: SmallVec<[Entity; 8]> = SmallVec::new();
    let mut scratch = std::mem::take(&mut graphs.scratch.temp);
    get_power_connections(world, grid, entity, &mut scratch);
    for other in scratch.iter().copied() {
        if world
            .get::<PowerModule>(other)
            .is_some_and(|module| module.graph != PowerGraphId::NONE)
        {
            connections.push(other);
        }
    }
    graphs.scratch.temp = scratch;

    for other in connections {
        let own = world
            .get::<PowerModule>(entity)
            .map(|module| module.graph)
            .unwrap_or(PowerGraphId::NONE);
        let other_graph = world
            .get::<PowerModule>(other)
            .map(|module| module.graph)
            .unwrap_or(PowerGraphId::NONE);
        if other_graph != PowerGraphId::NONE && other_graph != own {
            graphs.add_graph(world, own, other_graph);
        }
    }
    if let Some(graph) = world.get::<PowerModule>(entity).map(|module| module.graph) {
        graphs.check_add(world, graph);
    }
}

/// `Building.powerGraphRemoved()`: split the graph and clear link metadata.
pub fn power_graph_removed(
    graphs: &mut super::graph::PowerGrids,
    world: &mut World,
    grid: &WorldGrid,
    entity: Entity,
) {
    let (own, links) = {
        let Some(module) = world.get::<PowerModule>(entity) else {
            return;
        };
        (module.graph, module.links.clone())
    };
    if own == PowerGraphId::NONE {
        return;
    }
    graphs.remove(world, grid, entity);

    // Drop reciprocal links pointing at this tile.
    let pos = pack(world, entity);
    for link in links {
        let link_pos = crate::world::TilePos::from_pack(link);
        if let Some(other) = grid.entity_at(link_pos)
            && let Some(mut other_module) = world.get_mut::<PowerModule>(other)
        {
            other_module.links.retain(|value| *value != pos);
        }
    }
    if let Some(mut module) = world.get_mut::<PowerModule>(entity) {
        module.links.clear();
        module.graph = PowerGraphId::NONE;
        module.init = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;
    use crate::entities::comp::{Building, TeamComp};
    use crate::world::block::BlockTable;
    use crate::world::{BuildRules, TilePos, WorldGrid};
    use bevy_ecs::world::World as EcsWorld;

    fn world_with_copper_wall() -> (EcsWorld, WorldGrid) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let mut world = EcsWorld::new();
        world.insert_resource(table);
        world.insert_resource(BuildRules::default());
        (world, WorldGrid::new(8, 8))
    }

    #[test]
    fn params_fall_back_to_block_def() {
        let (mut world, _grid) = world_with_copper_wall();
        let table = world.get_resource::<BlockTable>().expect("table");
        let battery = table.get_named("battery").expect("battery").clone();
        let e = battery.spawn(&mut world, 0, TilePos::new(2, 2), 0, 0, 4, 2);
        let params = read_power_info(&world, e).expect("params");
        assert!(params.has_power);
        assert!(params.outputs);
        assert!(params.consumes);
        assert!(params.buffered);
        assert!(params.capacity > 0.0);
    }

    #[test]
    fn connections_require_same_team() {
        let (mut world, grid) = world_with_copper_wall();
        let table = world.get_resource::<BlockTable>().expect("table");
        let node = table.get_named("power-node").expect("power-node").clone();
        let a = node.spawn(&mut world, 0, TilePos::new(1, 1), 0, 0, 4, 2);
        let b = node.spawn(&mut world, 1, TilePos::new(2, 1), 1, 0, 4, 2);
        world.get_mut::<Building>(a).expect("a").proximity.push(b);
        let mut out = Vec::new();
        get_power_connections(&world, &grid, a, &mut out);
        assert!(out.is_empty());
        world.get_mut::<TeamComp>(b).expect("b").team = 0;
        get_power_connections(&world, &grid, a, &mut out);
        assert_eq!(out, vec![b]);
    }
}
