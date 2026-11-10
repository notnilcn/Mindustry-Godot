// SPDX-License-Identifier: GPL-3.0-only

//! World config application (`Building.configure`) and the placed-hooks/power-graph
//! helpers.
//!
//! Upstream: core/src/mindustry/world/blocks/power/PowerNode.java (configure).

use crate::sim::*;

impl Sim {
    /// Applies one `world::config::ConfigValue` to a building: the power-node
    /// graph path first (`PowerNode.java:419-441`, `config(Integer.class)` =
    /// toggle link, `config(Point2[].class)` = replace all links), then the
    /// behavior `configured` path — the exact order of the
    /// `SimCommand::Configure` arm. Used by the command arm and by the
    /// generator `postGenerate` power-node reset (I04 T17).
    pub(crate) fn apply_world_config(
        &mut self,
        entity: Entity,
        value: crate::world::config::ConfigValue,
    ) -> bool {
        if self.configure_power_node(entity, &value) {
            return true;
        }
        // `CoreBlock.java:322-332`: with `coreBuildAndConfig`, an integer
        // config sets the player's team (and the core's).
        if let crate::world::config::ConfigValue::Number(number) = &value
            && (crate::game::runtime::rules_of(&self.ecs.0)
                .is_some_and(|rules| rules.core_build_and_config)
                || self
                    .ecs
                    .0
                    .get_resource::<crate::world::limits::BuildRules>()
                    .is_some_and(|rules| rules.core_build_and_config))
            && let Some(building) = self.ecs.0.get::<crate::entities::comp::Building>(entity)
        {
            let is_core = self
                .block_runtime
                .as_ref()
                .and_then(|runtime| runtime.content().block(building.block))
                .is_some_and(|def| def.kind == crate::content::BlockKind::CoreBlock);
            if is_core {
                let team = (*number as i64 & 0xFF) as u8;
                if let Some(mut comp) = self
                    .ecs
                    .0
                    .get_mut::<crate::entities::comp::TeamComp>(entity)
                {
                    comp.team = team;
                }
                if let Some(unit) = self.player_unit()
                    && let Some(mut comp) =
                        self.ecs.0.get_mut::<crate::entities::comp::TeamComp>(unit)
                {
                    comp.team = team;
                }
                self.set_player_team(team);
                return true;
            }
        }
        let applied = crate::world::config::configure(&mut self.ecs.0, entity, None, value);
        // A door toggle changes `Tile.solid()` (Java calls
        // `pathfinder.updateTile(entity.tile)` inline, `Door.java:56`); mirror
        // the new `checkSolid` into the grid and re-pack when it moved.
        if applied && crate::world::sync_dynamic_solid(&mut self.grid, &self.ecs.0) {
            self.refresh_unit_runtime();
        }
        applied
    }

    /// Runs the `placed()` hooks for a building placed outside the construction
    /// path (`BuildingComp.placed` + `PowerNode.placed`; `BaseGenerator.postGenerate`
    /// calls `tile.build.placed()` explicitly, `BaseGenerator.java:180`).
    pub fn run_placed_hooks(&mut self, entity: Entity) {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return;
        };
        runtime.placed_hooks(&mut self.ecs.0, &self.grid, entity);
    }

    /// Returns `false` for a non-node (no `PowerNodeConfig`), so the caller
    /// falls through to the normal behavior config path.
    pub(in crate::sim) fn configure_power_node(
        &mut self,
        entity: Entity,
        value: &crate::world::config::ConfigValue,
    ) -> bool {
        use crate::world::blocks::power::nodes;
        use crate::world::config::ConfigValue as V;
        if self
            .ecs
            .0
            .get::<crate::world::blocks::power::PowerNodeConfig>(entity)
            .is_none()
        {
            return false;
        }
        match value {
            V::Number(number) => {
                let packed = *number as i32;
                self.with_power_graphs(|graphs, world, grid| {
                    nodes::configure_link(graphs, world, grid, entity, packed);
                });
                true
            }
            V::Point2(dx, dy) => {
                let Some(building) = self.ecs.0.get::<crate::entities::comp::Building>(entity)
                else {
                    return false;
                };
                let packed = TilePos::new(
                    building.tile.x() + *dx as i16,
                    building.tile.y() + *dy as i16,
                )
                .pack();
                self.with_power_graphs(|graphs, world, grid| {
                    nodes::configure_link(graphs, world, grid, entity, packed);
                });
                true
            }
            V::Point2Array(points) => {
                let packed: Vec<i32> = points.iter().copied().collect();
                self.with_power_graphs(|graphs, world, grid| {
                    nodes::configure_links(graphs, world, grid, entity, &packed);
                });
                true
            }
            _ => false,
        }
    }

    /// Takes `PowerGrids` out of the ECS for `f` and always re-inserts it (the
    /// `SimRuntime::merge_power_graph` pattern).
    pub(in crate::sim) fn with_power_graphs(
        &mut self,
        f: impl FnOnce(
            &mut crate::world::blocks::power::PowerGrids,
            &mut bevy_ecs::world::World,
            &WorldGrid,
        ),
    ) {
        let Some(mut graphs) = self
            .ecs
            .0
            .remove_resource::<crate::world::blocks::power::PowerGrids>()
        else {
            return;
        };
        f(&mut graphs, &mut self.ecs.0, &self.grid);
        self.ecs.0.insert_resource(graphs);
    }
}
