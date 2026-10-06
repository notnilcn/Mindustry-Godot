// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PowerNode`/`LongPowerNode`/`PowerDiode`/`Battery` semantics
//! (`world/blocks/power/{PowerNode,LongPowerNode,PowerDiode,Battery}.java`).
//!
//! Reconciliation (plan 09 R4): `getPotentialLinks` uses the default
//! deterministic scan over `Groups.build` (sorted by node-ness then squared
//! distance, tie-broken by entity index) instead of the Arc `TeamData` quad
//! tree. This is behaviorally identical for the node linker except on exact
//! distance ties across insertion boundaries; the plan-records UD-09-1 default
//! is the exact tree and the orchestrator reconciles at the 11/12 merge.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::Building;
use crate::world::behavior::BuildingBehavior;
use crate::world::block::TILE_SIZE;
use crate::world::modules::{PowerGraphId, PowerModule};
use crate::world::{TilePos, WorldGrid};

use super::graph::{PowerGrids, set_status};
use super::module::read_power_info;
use super::{PowerNodeConfig, PowerProduction};

/// `PowerNode`/`LongPowerNode` construction behavior: inserts the
/// [`PowerNodeConfig`] the graph linker (`get_potential_links`/`link_valid`)
/// reads. The `UpdatePowerGraph` schedule slot then merges graphs
/// (`Logic.updateEntities`); this behavior owns only the construction half.
#[derive(Debug, Clone, Copy)]
pub struct PowerNodeBehavior {
    /// `PowerNode.maxNodes`.
    pub max_nodes: u8,
    /// `PowerNode.laserRange` (tiles).
    pub laser_range: f32,
    /// `PowerNode.autolink`.
    pub autolink: bool,
    /// `PowerNode.sameBlockConnection`.
    pub same_block_connection: bool,
}

impl BuildingBehavior for PowerNodeBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<PowerNodeConfig>(e).is_none() {
            world.entity_mut(e).insert(PowerNodeConfig {
                max_nodes: self.max_nodes,
                laser_range: self.laser_range,
                autolink: self.autolink,
                same_block_connection: self.same_block_connection,
            });
        }
    }
}

/// Circle/axis-aligned-rect overlap (Arc `Intersector.overlaps(Circle, Rect)`).
pub fn circle_rect_overlap(cx: f32, cy: f32, radius: f32, rect: [f32; 4]) -> bool {
    let [rx, ry, rw, rh] = rect;
    let closest_x = cx.clamp(rx, rx + rw);
    let closest_y = cy.clamp(ry, ry + rh);
    let dx = cx - closest_x;
    let dy = cy - closest_y;
    dx * dx + dy * dy <= radius * radius
}

/// Pixel hitbox of a placed block (`Tile.getHitbox` shape; center + size).
pub fn hitbox(world: &World, entity: Entity) -> Option<[f32; 4]> {
    let building = world.get::<Building>(entity)?;
    let size = block_size(world, entity)? as f32;
    let cx = (building.tile.x() as f32 + 0.5) * TILE_SIZE;
    let cy = (building.tile.y() as f32 + 0.5) * TILE_SIZE;
    Some([
        cx - size * TILE_SIZE / 2.0,
        cy - size * TILE_SIZE / 2.0,
        size * TILE_SIZE,
        size * TILE_SIZE,
    ])
}

/// Block size of a building's block (from the module's block def).
pub fn block_size(world: &World, entity: Entity) -> Option<i32> {
    let building = world.get::<Building>(entity)?;
    world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.get(building.block))
        .map(|instance| instance.def.size.max(1))
}

/// Whether the block is a power node (`Block instanceof PowerNode`).
pub fn is_power_node(world: &World, entity: Entity) -> bool {
    world.get::<PowerNodeConfig>(entity).is_some()
}

/// `PowerNode.overlaps(src, other, range)`: circle around `src` against the
/// target block's hitbox.
pub fn overlaps(world: &World, source: Entity, other: Entity, range_world: f32) -> bool {
    let Some(source_building) = world.get::<Building>(source) else {
        return false;
    };
    let cx = (source_building.tile.x() as f32 + 0.5) * TILE_SIZE;
    let cy = (source_building.tile.y() as f32 + 0.5) * TILE_SIZE;
    let Some(target) = hitbox(world, other) else {
        return false;
    };
    circle_rect_overlap(cx, cy, range_world, target)
}

/// `PowerNode.insulated(x, y, x2, y2)`: any insulated building on the ray.
pub fn insulated(world: &World, grid: &WorldGrid, from: TilePos, to: TilePos) -> bool {
    let mut hit = false;
    crate::world::raycast::raycast(
        from.x() as i32,
        from.y() as i32,
        to.x() as i32,
        to.y() as i32,
        |x, y| {
            if !grid.tiles.in_bounds(x, y) {
                return false;
            }
            let Some(entity) = grid.tile(x, y).build else {
                return false;
            };
            let Some(building) = world.get::<Building>(entity) else {
                return false;
            };
            let is_insulated = world
                .get_resource::<crate::world::block::BlockTable>()
                .and_then(|table| table.get(building.block))
                .is_some_and(|instance| instance.def.insulated);
            if is_insulated {
                hit = true;
                true
            } else {
                false
            }
        },
    );
    hit
}

/// `PowerNode.linkValid(tile, link, checkMaxNodes)`.
pub fn link_valid(
    world: &World,
    grid: &WorldGrid,
    tile: Entity,
    link: Entity,
    check_max: bool,
) -> bool {
    if tile == link {
        return false;
    }
    let (Some(tile_params), Some(link_params)) =
        (read_power_info(world, tile), read_power_info(world, link))
    else {
        return false;
    };
    if !link_params.has_power || tile_params.team != link_params.team {
        return false;
    }
    let (Some(tile_building), Some(link_building)) =
        (world.get::<Building>(tile), world.get::<Building>(link))
    else {
        return false;
    };
    if let Some(config) = world.get::<PowerNodeConfig>(tile)
        && config.same_block_connection
        && tile_building.block != link_building.block
    {
        return false;
    }

    let node_config = world.get::<PowerNodeConfig>(tile);
    let range = node_config.map(|config| config.laser_range).unwrap_or(6.0) * TILE_SIZE;
    let in_range = overlaps(world, tile, link, range)
        || (is_power_node(world, link) && {
            let link_range = world
                .get::<PowerNodeConfig>(link)
                .map(|config| config.laser_range)
                .unwrap_or(6.0)
                * TILE_SIZE;
            overlaps(world, link, tile, link_range)
        });
    if !in_range {
        return false;
    }
    if insulated(world, grid, tile_building.tile, link_building.tile) {
        return false;
    }
    if check_max && let Some(config) = world.get::<PowerNodeConfig>(link) {
        let links = world
            .get::<PowerModule>(link)
            .map(|module| module.links.len())
            .unwrap_or(0);
        return links < config.max_nodes as usize
            || world
                .get::<PowerModule>(link)
                .is_some_and(|module| module.links.contains(&tile_building.tile.pack()));
    }
    true
}

/// `PowerNode.getPotentialLinks(tile, team, others)` — deterministic scan.
///
/// Returns candidate entities in `(is node, dst2, entity index)` order.
pub fn get_potential_links(world: &World, grid: &WorldGrid, tile: Entity, out: &mut Vec<Entity>) {
    out.clear();
    let Some(tile_building) = world.get::<Building>(tile) else {
        return;
    };
    let Some(tile_params) = read_power_info(world, tile) else {
        return;
    };
    let Some(config) = world.get::<PowerNodeConfig>(tile) else {
        return;
    };
    if !config.autolink {
        return;
    }
    let range = config.laser_range * TILE_SIZE;
    let tx = tile_building.tile.x() as f32;
    let ty = tile_building.tile.y() as f32;

    // Exclude graphs already represented by adjacent ties (Java `graphs` set).
    let mut represented: Vec<PowerGraphId> = Vec::new();
    if let Some(module) = world.get::<PowerModule>(tile) {
        represented.push(module.graph);
    }
    for offset in crate::world::edges::edges(block_size(world, tile).unwrap_or(1)) {
        let p = TilePos::new(
            tile_building.tile.x() + offset.x(),
            tile_building.tile.y() + offset.y(),
        );
        if let Some(other) = grid.entity_at(p)
            && let Some(module) = world.get::<PowerModule>(other)
            && !represented.contains(&module.graph)
        {
            represented.push(module.graph);
        }
    }

    let mut candidates: Vec<Entity> = Vec::new();
    for entity_ref in world.iter_entities() {
        let Some(building) = entity_ref.get::<Building>() else {
            continue;
        };
        let other = entity_ref.id();
        if other == tile {
            continue;
        }
        let Some(other_params) = read_power_info(world, other) else {
            continue;
        };
        if other_params.team != tile_params.team {
            continue;
        }
        if !other_params.has_power
            || (!other_params.outputs && !other_params.consumes && !is_power_node(world, other))
        {
            continue;
        }
        if let Some(module) = world.get::<PowerModule>(other)
            && represented.contains(&module.graph)
        {
            continue;
        }
        if !overlaps(world, tile, other, range) {
            continue;
        }
        if insulated(world, grid, tile_building.tile, building.tile) {
            continue;
        }
        if let Some(other_config) = world.get::<PowerNodeConfig>(other) {
            let links = world
                .get::<PowerModule>(other)
                .map(|module| module.links.len())
                .unwrap_or(0);
            if links >= other_config.max_nodes as usize {
                continue;
            }
        }
        // Do not link to adjacent buildings.
        let adjacent = crate::world::edges::edges(block_size(world, tile).unwrap_or(1))
            .iter()
            .any(|offset| {
                let p = TilePos::new(
                    tile_building.tile.x() + offset.x(),
                    tile_building.tile.y() + offset.y(),
                );
                grid.entity_at(p) == Some(other)
            });
        if adjacent {
            continue;
        }
        candidates.push(other);
    }

    let dist2 = |entity: Entity| -> f32 {
        let Some(building) = world.get::<Building>(entity) else {
            return f32::MAX;
        };
        let dx = building.tile.x() as f32 - tx;
        let dy = building.tile.y() as f32 - ty;
        dx * dx + dy * dy
    };
    candidates.sort_by(|a, b| {
        let node_a = is_power_node(world, *a);
        let node_b = is_power_node(world, *b);
        node_b
            .cmp(&node_a)
            .then_with(|| dist2(*a).total_cmp(&dist2(*b)))
            .then_with(|| a.index().cmp(&b.index()))
    });
    out.extend(candidates.into_iter().take(config.max_nodes as usize));
}

/// Node `Integer` config: link when absent, unlink + reflow when present.
pub fn configure_link(
    graphs: &mut PowerGrids,
    world: &mut World,
    grid: &WorldGrid,
    tile: Entity,
    packed: i32,
) {
    let contains = world
        .get::<PowerModule>(tile)
        .is_some_and(|module| module.links.contains(&packed));
    let other = TilePos::from_pack(packed);
    let other_entity = grid.entity_at(other);

    if contains {
        if let Some(mut module) = world.get_mut::<PowerModule>(tile) {
            module.links.retain(|value| *value != packed);
        }
        let tile_pack = crate::world::blocks::power::module::pack(world, tile);
        if let Some(other_entity) = other_entity
            && let Some(mut module) = world.get_mut::<PowerModule>(other_entity)
        {
            module.links.retain(|value| *value != tile_pack);
        }
        graphs.reflow(world, grid, tile);
        if let Some(other_entity) = other_entity {
            let same = world
                .get::<PowerModule>(other_entity)
                .zip(world.get::<PowerModule>(tile))
                .is_some_and(|(a, b)| a.graph == b.graph);
            if !same {
                graphs.reflow(world, grid, other_entity);
            }
        }
    } else if let Some(other_entity) = other_entity
        && link_valid(world, grid, tile, other_entity, true)
    {
        let max_nodes = world
            .get::<PowerNodeConfig>(tile)
            .map(|config| config.max_nodes as usize)
            .unwrap_or(usize::MAX);
        let within_limit = world
            .get::<PowerModule>(tile)
            .is_some_and(|module| module.links.len() < max_nodes);
        if !within_limit {
            return;
        }
        let other_pack = crate::world::blocks::power::module::pack(world, other_entity);
        if let Some(mut module) = world.get_mut::<PowerModule>(tile)
            && !module.links.contains(&packed)
        {
            module.links.push(packed);
        }
        let same_team = world
            .get::<crate::entities::comp::TeamComp>(tile)
            .map(|team| team.team)
            == world
                .get::<crate::entities::comp::TeamComp>(other_entity)
                .map(|team| team.team);
        if same_team
            && let Some(mut module) = world.get_mut::<PowerModule>(other_entity)
            && !module.links.contains(&other_pack)
        {
            module.links.push(other_pack);
        }
        let own = world
            .get::<PowerModule>(tile)
            .map(|module| module.graph)
            .unwrap_or(PowerGraphId::NONE);
        let other_graph = world
            .get::<PowerModule>(other_entity)
            .map(|module| module.graph)
            .unwrap_or(PowerGraphId::NONE);
        if own != PowerGraphId::NONE && other_graph != PowerGraphId::NONE {
            graphs.add_graph(world, own, other_graph);
        }
    }
}

/// Node `Point2[]` config: clear old links then apply each relative offset.
pub fn configure_links(
    graphs: &mut PowerGrids,
    world: &mut World,
    grid: &WorldGrid,
    tile: Entity,
    points: &[i32],
) {
    let old: Vec<i32> = world
        .get::<PowerModule>(tile)
        .map(|module| module.links.iter().copied().collect())
        .unwrap_or_default();
    for link in old {
        configure_link(graphs, world, grid, tile, link);
    }
    let Some(building) = world.get::<Building>(tile) else {
        return;
    };
    let origin = building.tile;
    for point in points {
        let (dx, dy) = crate::world::pos::unpack(*point);
        let packed = TilePos::new(origin.x() + dx, origin.y() + dy).pack();
        configure_link(graphs, world, grid, tile, packed);
    }
}

/// `LongPowerNode`/`PowerNode` warmup for links (sim state). Returns the target.
pub fn link_warmup(world: &mut World, tile: Entity, target: f32, speed: f32) -> f32 {
    let _ = (world, tile, target, speed);
    target
}

/// `Battery.overwrote(previous)`: absorb stored fractions of buffered blocks.
pub fn battery_overwrote(world: &World, tile: Entity, previous: &[Entity]) -> f32 {
    let capacity = read_power_info(world, tile)
        .map(|params| params.capacity)
        .unwrap_or(0.0);
    let mut status = world
        .get::<PowerModule>(tile)
        .map(|module| module.status)
        .unwrap_or(0.0);
    for other in previous {
        if let (Some(module), Some(params)) = (
            world.get::<PowerModule>(*other),
            read_power_info(world, *other),
        ) && params.buffered
            && capacity > 0.0
        {
            let amount = params.capacity * module.status;
            status = (status + amount / capacity).clamp(0.0, 1.0);
        }
    }
    status
}

/// `PowerDiode.updateTile`: move half the stored-fraction difference across a
/// front/back graph pair.
pub fn update_diode(world: &mut World, front: Entity, back: Entity) {
    let Some(params) = read_power_info(world, front) else {
        return;
    };
    if !params.has_power {
        return;
    }
    let Some(back_params) = read_power_info(world, back) else {
        return;
    };
    if !back_params.has_power || params.team != back_params.team {
        return;
    }
    let front_graph = params.graph;
    let back_graph = back_params.graph;
    if front_graph == back_graph
        || front_graph == PowerGraphId::NONE
        || back_graph == PowerGraphId::NONE
    {
        return;
    }

    // Temporarily own the arena so graph reads may also borrow `world`.
    let graphs = world.remove_resource::<PowerGrids>().unwrap_or_default();
    let (back_stored, back_capacity, front_stored, front_capacity) = {
        let back_graph_ref = graphs.graph(back_graph);
        let front_graph_ref = graphs.graph(front_graph);
        (
            back_graph_ref
                .map(|graph| graph.get_battery_stored(world))
                .unwrap_or(0.0),
            back_graph_ref
                .map(|graph| graph.get_total_battery_capacity(world))
                .unwrap_or(0.0),
            front_graph_ref
                .map(|graph| graph.get_battery_stored(world))
                .unwrap_or(0.0),
            front_graph_ref
                .map(|graph| graph.get_total_battery_capacity(world))
                .unwrap_or(0.0),
        )
    };
    world.insert_resource(graphs);

    if back_capacity <= 0.0 || front_capacity <= 0.0 {
        return;
    }
    if back_stored / back_capacity <= front_stored / front_capacity {
        return;
    }
    let target = (front_stored + back_stored) / (front_capacity + back_capacity);
    let amount =
        ((target * front_capacity - front_stored) / 2.0).clamp(0.0, front_capacity - front_stored);

    let mut graphs = world.remove_resource::<PowerGrids>().unwrap_or_default();
    if let Some(graph) = graphs.graph_mut(back_graph) {
        graph.transfer_power(world, -amount);
    }
    if let Some(graph) = graphs.graph_mut(front_graph) {
        graph.transfer_power(world, amount);
    }
    world.insert_resource(graphs);
}

/// Sandbox `PowerSource` production (plan 09 §3.7).
pub fn set_power_source(world: &mut World, entity: Entity, production: f32) {
    world.entity_mut(entity).insert(PowerProduction(production));
}

/// Sandbox `PowerVoid` drain: consume `f32::MAX` and set status 1.
pub fn set_power_void(world: &mut World, entity: Entity) {
    set_status(world, entity, 1.0);
}

/// Blocks that are "connected power" for auto-link (`Block.connectedPower`).
pub fn connected_power(world: &World, entity: Entity) -> bool {
    read_power_info(world, entity).is_some_and(|params| {
        params.has_power && (params.outputs || params.consumes || is_power_node(world, entity))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_rect_overlap_matches_arc() {
        assert!(circle_rect_overlap(0.0, 0.0, 2.0, [1.0, 1.0, 2.0, 2.0]));
        assert!(!circle_rect_overlap(0.0, 0.0, 0.5, [1.0, 1.0, 2.0, 2.0]));
        assert!(circle_rect_overlap(2.0, 2.0, 2.0, [1.0, 1.0, 2.0, 2.0]));
    }
}
