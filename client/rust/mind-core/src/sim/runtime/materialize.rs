// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Map/save materialization and the per-tile proximity/power-graph refresh
//! helpers shared by the tile ops.
//!
//! Upstream: core/src/mindustry/entities/comp/BuildingComp.java (proximity);
//! core/src/mindustry/core/World.java (endMapLoad).

use super::*;

impl SimRuntime {
    /// Shared `Tile.remove` half of break/destroy.
    /// `BeamDrill.updateFacing`/`WallCrafter.getEfficiency` (T5): whether this
    /// building's facing edge is rescanned every tick.
    pub(super) fn is_facing_drill(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<crate::entities::comp::Building>(entity)
            .and_then(|building| {
                world
                    .get_resource::<BlockTable>()
                    .and_then(|table| table.get(building.block))
            })
            .is_some_and(|inst| {
                matches!(
                    inst.kind_data,
                    crate::world::BlockKindData::BeamDrill(_)
                        | crate::world::BlockKindData::WallCrafter(_)
                )
            })
    }

    /// `ThermalGenerator.attribute` for a live thermal generator
    /// (`heat` for `thermal-generator`, `steam` for `turbine-condenser`);
    /// `None` for every other block.
    pub(super) fn thermal_attribute(&self, world: &World, entity: Entity) -> Option<Attribute> {
        world
            .get::<crate::entities::comp::Building>(entity)
            .and_then(|building| {
                world
                    .get_resource::<BlockTable>()
                    .and_then(|table| table.get(building.block))
            })
            .and_then(|inst| {
                (inst.def.kind == crate::content::BlockKind::ThermalGenerator).then(|| {
                    if inst.name == "turbine-condenser" {
                        Attribute::Steam
                    } else {
                        Attribute::Heat
                    }
                })
            })
    }

    /// Whether this building is a `BeamNode` (registered by the power family;
    /// `BeamNodeBehavior::create_state` installs the state marker).
    pub(super) fn is_beam_node(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<crate::world::blocks::power::nodes::BeamNodeState>(entity)
            .is_some()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn remove_building(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        entity: Entity,
        x: i16,
        y: i16,
        team: u8,
        block: BlockId,
    ) {
        // `BuildingComp.removeFromProximity` runs `onProximityRemoved` ->
        // `powerGraphRemoved` FIRST, while `proximity` still lists the
        // neighbours the graph split walks (I01 T14).
        self.remove_power_graph(world, grid, entity);
        let _ = remove_from_proximity(world, entity, &self.content);
        self.facing_drills.retain(|drill| *drill != entity);
        self.thermal_generators
            .retain(|generator| *generator != entity);
        self.beam_nodes.retain(|node| *node != entity);
        {
            let mut ctx = WorldCtx {
                grid: &mut *grid,
                content: &self.content,
                ecs: &mut *world,
                hooks: &self.hooks,
                render: &self.render,
                log: &mut self.log,
            };
            ctx.remove_block(x, y);
        }
        if let Some(mut counter) = world.get_resource_mut::<BlockCounter>() {
            counter.remove(team, block, 1);
        }
        // `Tile.changed` re-runs proximity for the four neighbors.
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if let Some(entity) = grid.entity_at(TilePos::new(nx, ny)) {
                let _ = update_proximity(world, grid, &self.content, entity);
            }
        }
    }

    /// `World.loadMap`'s building half: spawns the ECS building entities for a
    /// grid whose tile blocks were read from a map/save.
    ///
    /// `pending` is the reader's decoded payload queue (`(tile index, base)`);
    /// each entry is a multiblock/single-block center, so the payload drives the
    /// tile-op spawn (team/rotation/health/modules applied from
    /// [`DecodedBase`]). Proximity and power links are rebuilt once afterwards,
    /// mirroring `World.endMapLoad`. Returns the number of entities spawned.
    pub fn materialize_buildings(
        &mut self,
        ecs: &mut World,
        grid: &mut WorldGrid,
        pending: &[(usize, DecodedBase)],
    ) -> usize {
        // `World.endMapLoad` (`World.java:220-230`): legacy blocks must stop
        // existing before any entity is spawned for their tile. The pending
        // decoded bases were read from a save/map that still names the legacy
        // block, so the sweep swaps the tile to its replacement (or air) and
        // despawns the stale legacy entity; the loop below then spawns the
        // replacement under its own block. No-op for an already-swept grid
        // (`WorldGrid::end_map_load` runs the same pass on most load paths).
        crate::world::behavior::legacy::remove_legacy_blocks_live(grid, &self.content, ecs);
        // Tile blocks are already assigned; suppress the tile-change counters
        // while the entity links are attached (`World.generating`).
        let generating = grid.generating;
        grid.generating = true;
        let mut log = WorldEventLog::default();
        let mut spawned: Vec<Entity> = Vec::new();
        for (index, decoded) in pending {
            let Some(tile) = grid.tiles.array().get(*index) else {
                continue;
            };
            if tile.build.is_some() {
                continue;
            }
            let block = tile.block;
            if block == BlockId::AIR || !crate::world::block_has_building(&self.content, block) {
                continue;
            }
            let (x, y) = (tile.x, tile.y);
            {
                let mut ctx = WorldCtx {
                    grid: &mut *grid,
                    content: &self.content,
                    ecs: &mut *ecs,
                    hooks: &self.hooks,
                    render: &self.render,
                    log: &mut log,
                };
                ctx.set_block(x, y, block, decoded.team, decoded.rotation);
            }
            let Some(entity) = grid.entity_at(TilePos::new(x, y)) else {
                continue;
            };
            let max_health = self
                .content
                .block(block)
                .map(|def| def.health.max(0) as f32)
                .unwrap_or(f32::MAX);
            building_io::apply_base(ecs, entity, decoded, max_health);
            // Per-kind body (`BuildingComp.readAll` = `readBase(); read()`;
            // `BuildingComp.java:284-292`), before the proximity/power pass
            // like Java where `readAll` runs during the map pass.
            building_io::apply_body(ecs, entity, decoded);
            spawned.push(entity);
        }
        // Java `World.endMapLoad` (`World.java:220-236`) rebuilds every
        // building's proximity while `generating` is still set, and only then
        // clears it. Behaviors that only see the ECS world read the flag from
        // `WorldGenerating`; `CoreBlock` skips its item clamp during the pass
        // (`CoreBlock.java:773-778`), so a loaded save keeps its item stacks.
        ecs.insert_resource(crate::world::WorldGenerating(true));
        for entity in &spawned {
            let _ = update_proximity(ecs, grid, &self.content, *entity);
        }
        grid.generating = generating;
        ecs.insert_resource(crate::world::WorldGenerating(generating));
        // `Drill.countOre` is grid-derived state the tile reader cannot carry:
        // without it a materialized drill has no `dominantItem` and never mines
        // (`SimRuntime::place` does the same after `Tile.setBlock`).
        for entity in &spawned {
            crate::world::behavior::production::refresh_drill_ore(
                ecs,
                grid,
                &self.content,
                *entity,
            );
        }
        for entity in &spawned {
            self.merge_power_graph(ecs, grid, *entity);
        }
        // Materialized facing drills join the per-tick rescan (T5).
        for entity in &spawned {
            if self.is_facing_drill(ecs, *entity) && !self.facing_drills.contains(entity) {
                self.facing_drills.push(*entity);
            }
        }
        // Materialized thermal generators join the per-tick floor-attribute
        // rescan (T14).
        for entity in &spawned {
            if self.thermal_attribute(ecs, *entity).is_some()
                && !self.thermal_generators.contains(entity)
            {
                self.thermal_generators.push(*entity);
            }
        }
        // Materialized beam nodes join the tile-change rescan (T15).
        for entity in &spawned {
            if self.is_beam_node(ecs, *entity) && !self.beam_nodes.contains(entity) {
                self.beam_nodes.push(*entity);
            }
        }
        spawned.len()
    }

    /// Recomputes proximity for the tile at `(x, y)` and its four neighbors.
    /// Re-links proximity around `(x, y)` (`Building.updateProximity`).
    pub(crate) fn refresh_proximity(&self, world: &mut World, grid: &WorldGrid, x: i16, y: i16) {
        self.refresh_proximity_at(world, grid, x, y);
    }

    pub(super) fn refresh_proximity_at(&self, world: &mut World, grid: &WorldGrid, x: i16, y: i16) {
        if let Some(entity) = grid.entity_at(TilePos::new(x, y)) {
            let _ = update_proximity(world, grid, &self.content, entity);
        }
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if let Some(entity) = grid.entity_at(TilePos::new(nx, ny)) {
                let _ = update_proximity(world, grid, &self.content, entity);
            }
        }
    }

    /// `Building.onProximityAdded` power half: merge the building's graph.
    pub(crate) fn merge_power_graph(
        &mut self,
        world: &mut World,
        grid: &WorldGrid,
        entity: Entity,
    ) {
        if world.get::<PowerModule>(entity).is_none() {
            return;
        }
        let Some(mut graphs) = world.remove_resource::<PowerGrids>() else {
            return;
        };
        update_power_graph(&mut graphs, world, grid, entity);
        world.insert_resource(graphs);
    }

    /// `Building.onProximityRemoved` power half: split the graph.
    pub(super) fn remove_power_graph(
        &mut self,
        world: &mut World,
        grid: &WorldGrid,
        entity: Entity,
    ) {
        if world.get::<PowerModule>(entity).is_none() {
            return;
        }
        let Some(mut graphs) = world.remove_resource::<PowerGrids>() else {
            return;
        };
        power_graph_removed(&mut graphs, world, grid, entity);
        world.insert_resource(graphs);
    }
}
