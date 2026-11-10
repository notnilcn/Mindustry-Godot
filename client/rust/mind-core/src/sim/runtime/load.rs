// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Save/load world-swap helpers: replay placement, world clearing and power-graph
//! rebuild.
//!
//! Upstream: none (port-only load-path helpers).

use super::*;

impl Sim {
    /// Places one replay building owned by `team` on an empty tile
    /// (`tile.setBlock` in `Control.playSector`'s restore; the `validPlace` gate
    /// is approximated by an air check). Returns whether the block was placed.
    pub fn place_replay_building(
        &mut self,
        x: i16,
        y: i16,
        block: crate::content::BlockId,
        rot: u8,
        team: u8,
        health: f32,
    ) -> bool {
        let Sim {
            grid,
            ecs,
            block_runtime,
            ..
        } = self;
        let Some(runtime) = block_runtime.as_mut() else {
            return false;
        };
        if grid.tiles.get(i32::from(x), i32::from(y)).block != crate::content::BlockId::AIR {
            return false;
        }
        let mut log = crate::world::ops::WorldEventLog::default();
        {
            let mut ctx = crate::world::ops::WorldCtx {
                grid: &mut *grid,
                content: &runtime.content,
                ecs: &mut ecs.0,
                hooks: &runtime.hooks,
                render: &runtime.render,
                log: &mut log,
            };
            ctx.set_block(x, y, block, team, rot);
        }
        if let Some(entity) = grid.entity_at(crate::world::TilePos::new(x, y))
            && let Some(mut hp) = ecs
                .0
                .get_mut::<crate::entities::comp::health::Health>(entity)
        {
            hp.health = health.clamp(1.0, hp.max_health.max(1.0));
        }
        true
    }

    /// Drops every sim entity and resets live-runtime bookkeeping ahead of a
    /// save-load grid swap (`Groups.clear`; the caller assigns the new grid).
    ///
    /// Unlike [`Sim::reset`] the phase/tick header, schedule and the runtime
    /// resources (`BlockTable`, `BuildRules`, `ModuleDims`, `PowerGrids`) stay
    /// installed, so the loaded world can materialize its buildings immediately.
    /// Units/bullets are despawned here; the caller re-snapshots the unit
    /// runtime onto the new grid (`UnitRuntime::reset`/`refresh_grid`).
    pub fn clear_world_for_load(&mut self) {
        let entities: Vec<Entity> = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity| {
                // Sim entities plus the fires/puddles that carry no
                // `EntitySeq` (`Groups.clear` drops every group on a world
                // reset; PAR-0090).
                entity.contains::<crate::ecs::EntitySeq>()
                    || entity.contains::<crate::combat::fires::FireState>()
                    || entity.contains::<crate::combat::puddles::PuddleState>()
            })
            .map(|entity| entity.id())
            .collect();
        for entity in entities {
            self.ecs.0.despawn(entity);
        }
        if let Some(runtime) = self.block_runtime.as_mut() {
            runtime.reset(&mut self.ecs.0);
            self.ecs
                .0
                .insert_resource(crate::world::blocks::power::PowerGrids::new());
        }
        // `ResetEvent` fog half (`FogControl.java:42-44`): the previous world's
        // fog is dropped; the rebuild happens once the new grid + rules are in
        // (`WorldLoadEvent`).
        self.last_fog_flags = None;
        self.reset_fog();
    }

    /// Rebuilds every power graph from proximity (`World.endMapLoad` path).
    pub fn rebuild_power_graphs(&mut self) {
        let Some(mut graphs) = self.ecs.0.remove_resource::<PowerGrids>() else {
            return;
        };
        graphs.rebuild_all(&mut self.ecs.0, &self.grid);
        self.ecs.0.insert_resource(graphs);
    }
}
