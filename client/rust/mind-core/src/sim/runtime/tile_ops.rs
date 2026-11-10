// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Tile.setBlock`/`Tile.remove` tile operations: raw set/remove, construct-block
//! place/break begin, and building destroy/pickup.
//!
//! Upstream: core/src/mindustry/world/Tile.java (setBlock, remove);
//! core/src/mindustry/world/blocks/ConstructBlock.java (construct/deconstruct).

use super::*;

impl SimRuntime {
    /// `Tile.setBlock` without validation: replaces whatever occupies the
    /// footprint, then relinks proximity, drill ore, power and the counter.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn set_block_raw(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        x: i16,
        y: i16,
        block: BlockId,
        rot: u8,
        team: u8,
    ) {
        let pos = TilePos::new(x, y);
        if let Some(old) = grid.entity_at(pos) {
            let old_block = grid.block_at(pos).unwrap_or(BlockId::AIR);
            let old_team = world
                .get::<crate::entities::comp::TeamComp>(old)
                .map(|team| team.team)
                .unwrap_or(team);
            // `BuildingComp.removeFromProximity` runs `onProximityRemoved` ->
            // `powerGraphRemoved` FIRST, while `proximity` still lists the
            // neighbours the graph split walks (I01 T14).
            self.remove_power_graph(world, grid, old);
            let _ = remove_from_proximity(world, old, &self.content);
            if old_block != BlockId::AIR
                && let Some(mut counter) = world.get_resource_mut::<BlockCounter>()
            {
                counter.remove(old_team, old_block, 1);
            }
        }

        {
            let mut ctx = WorldCtx {
                grid: &mut *grid,
                content: &self.content,
                ecs: &mut *world,
                hooks: &self.hooks,
                render: &self.render,
                log: &mut self.log,
            };
            ctx.set_block(x, y, block, team, rot);
        }

        self.refresh_proximity_at(world, grid, x, y);
        if let Some(entity) = grid.entity_at(pos) {
            crate::world::behavior::production::refresh_drill_ore(
                world,
                grid,
                &self.content,
                entity,
            );
            self.merge_power_graph(world, grid, entity);
            // Facing drills rescan every tick (`BeamDrill.updateFacing` /
            // `WallCrafter.getEfficiency`; T5).
            if self.is_facing_drill(world, entity) && !self.facing_drills.contains(&entity) {
                self.facing_drills.push(entity);
            }
            // Thermal generators rescan their floor-attribute sum every tick
            // (`ThermalGeneratorBuild.onProximityAdded`; T14 - the behavior hook
            // cannot see `Sim`'s grid).
            if self.thermal_attribute(world, entity).is_some()
                && !self.thermal_generators.contains(&entity)
            {
                self.thermal_generators.push(entity);
            }
            // Beam nodes rebuild their four directional links when the tile
            // change counter moves (`BeamNode.updateTile`; T15).
            if self.is_beam_node(world, entity) && !self.beam_nodes.contains(&entity) {
                self.beam_nodes.push(entity);
            }
        }
        if let Some(mut counter) = world.get_resource_mut::<BlockCounter>() {
            counter.add(team, block, 1);
        }
        // `BuildingComp.create()` -> `checkAllowUpdate` (`BuildingComp.java:136`):
        // a freshly created building starts disabled when it may not update.
        if let Some(entity) = grid.entity_at(pos) {
            let (env, limit_area, disable_outside_area, editor) =
                match crate::game::runtime::rules_of(world) {
                    Some(rules) => (
                        rules.env,
                        rules.limit_map_area.then_some((
                            rules.limit_x,
                            rules.limit_y,
                            rules.limit_width,
                            rules.limit_height,
                        )),
                        rules.disable_outside_area,
                        rules.editor,
                    ),
                    None => (crate::game::rules::DEFAULT_ENV, None, true, false),
                };
            if !editor {
                crate::world::allow_update::check_allow_update(
                    world,
                    entity,
                    env,
                    limit_area,
                    disable_outside_area,
                );
            }
        }
    }

    /// `ConstructBlock.get(size)`: the `build<size>` singleton.
    pub(crate) fn construct_block(&self, size: i32) -> Option<BlockId> {
        self.content.block_id(&format!("build{size}"))
    }

    /// `Build.beginPlace`: swaps the footprint to the matching `ConstructBlock`
    /// and attaches a fresh [`crate::world::ConstructState`]. Returns `false`
    /// when the placement is invalid.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin_place(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        x: i16,
        y: i16,
        block: BlockId,
        rot: u8,
        team: u8,
        flags: crate::world::build::PlaceFlags,
        cost_multiplier: f32,
    ) -> bool {
        if !self.can_place(world, grid, x, y, block, rot, team, flags) {
            return false;
        }
        let Some(def) = self.content.block(block) else {
            return false;
        };
        // `CoreBlock.placeBegan` + `beforePlaceBegan` (`CoreBlock.java:205-239`):
        // placing a core over a same-team core replaces it immediately,
        // keeping the old items minus the requirements.
        if def.kind == crate::content::BlockKind::CoreBlock
            && let Some(old) = grid.entity_at(TilePos::new(x, y))
            && world
                .get::<crate::entities::comp::TeamComp>(old)
                .map(|t| t.team)
                == Some(team)
            && world
                .get::<crate::world::blocks::storage::CoreBuild>(old)
                .is_some()
        {
            let mut next_items = world.get::<ItemModule>(old).cloned();
            let infinite = world
                .get_resource::<BuildRules>()
                .map(|rules| rules.infinite_resources)
                .unwrap_or(false);
            if !infinite && let Some(items) = next_items.as_mut() {
                for stack in &def.requirements {
                    let amount = ((stack.amount as f32) * cost_multiplier).round() as i32;
                    items.remove(stack.item, amount);
                }
            }
            self.set_block_raw(world, grid, x, y, block, rot, team);
            let Some(new_entity) = grid.entity_at(TilePos::new(x, y)) else {
                return false;
            };
            // `items.set(nextItems)` force-sets the totals.
            if let Some(items) = next_items {
                world.entity_mut(new_entity).insert(items);
            }
            let grid_ref: &WorldGrid = grid;
            self.placed_hooks(world, grid_ref, new_entity);
            return true;
        }
        let (size, build_time, requirements) = (def.size, def.build_time, def.requirements.clone());
        let Some(sub) = self.construct_block(size) else {
            // No construct singleton for this size: fall back to the
            // instant-build convention.
            self.set_block_raw(world, grid, x, y, block, rot, team);
            return true;
        };
        let pos = TilePos::new(x, y);
        let previous = grid.block_at(pos).unwrap_or(BlockId::AIR);
        let previous_size = self
            .content
            .block(previous)
            .map(|def| def.size)
            .unwrap_or(1);
        let previous = if previous != BlockId::AIR && previous_size == size {
            previous
        } else {
            BlockId::AIR
        };
        self.set_block_raw(world, grid, x, y, sub, rot, team);
        let Some(entity) = grid.entity_at(pos) else {
            return false;
        };
        let mut state = crate::world::ConstructState::construct(previous, block, 0.0);
        state.set_construct(previous, block, build_time, &requirements, cost_multiplier);
        world.entity_mut(entity).insert(state);
        true
    }

    /// `Build.beginBreak`: swaps the building to its `ConstructBlock` in the
    /// deconstruct state. Returns `false` when the break is invalid.
    pub(crate) fn begin_break(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        x: i16,
        y: i16,
        team: u8,
        cost_multiplier: f32,
    ) -> bool {
        let pos = TilePos::new(x, y);
        let Some(entity) = grid.entity_at(pos) else {
            return false;
        };
        let rules = world
            .get_resource::<BuildRules>()
            .cloned()
            .unwrap_or_default();
        if !crate::world::build::valid_break(
            &self.content,
            grid,
            Some(world),
            &rules,
            team,
            i32::from(x),
            i32::from(y),
        ) {
            return false;
        }
        let previous = grid.block_at(pos).unwrap_or(BlockId::AIR);
        let Some(def) = self.content.block(previous) else {
            return false;
        };
        let (size, build_time, requirements) = (def.size, def.build_time, def.requirements.clone());
        let building = world
            .get::<crate::entities::comp::Building>(entity)
            .map(|b| b.tile);
        let anchor = building.unwrap_or(pos);
        let rot = world
            .get::<BuildingComp>(entity)
            .map(|comp| comp.rot)
            .unwrap_or(0);
        // `tile.build.onDeconstructed(unit); tile.build.dead = true;`
        // (`Build.java:50-53`), before the swap to the construct block. The
        // per-kind hook may override the base liquid drop (`BuildingComp.java:
        // 1480-1487`).
        self.drop_deconstruct_liquid(world, grid, entity, def);
        if let Some(inst) = world
            .get_resource::<BlockTable>()
            .and_then(|table| table.instance(previous))
        {
            inst.behavior.on_deconstructed(world, entity, None);
        }
        let Some(sub) = self.construct_block(size) else {
            return self.break_block(world, grid, x, y).is_some();
        };
        self.set_block_raw(world, grid, anchor.x(), anchor.y(), sub, rot, team);
        let Some(entity) = grid.entity_at(anchor) else {
            return false;
        };
        let mut state = crate::world::ConstructState::deconstruct(previous, 0.0);
        state.set_deconstruct(previous, build_time, &requirements, cost_multiplier);
        world.entity_mut(entity).insert(state);
        true
    }

    /// `BuildingComp.onDeconstructed` (`BuildingComp.java:1478-1487`): a block
    /// holding liquid drops `currentAmount / (size * size) * 2` on every
    /// footprint tile unless the liquid is incinerable and the block does not
    /// set `deconstructDropAllLiquid`.
    pub(super) fn drop_deconstruct_liquid(
        &self,
        world: &mut World,
        grid: &WorldGrid,
        entity: Entity,
        def: &crate::content::registries::blocks::BlockDef,
    ) {
        let Some(module) = world.get::<crate::world::modules::LiquidModule>(entity) else {
            return;
        };
        let amount = module.current();
        let Some(liquid) = crate::world::blocks::liquid::current_liquid(module) else {
            return;
        };
        if amount <= 0.0 {
            return;
        }
        let incinerable = self
            .content
            .liquid(liquid)
            .map(|liquid| liquid.incinerable)
            .unwrap_or(false);
        if incinerable && !def.deconstruct_drop_all_liquid {
            return;
        }
        let Some(building) = world.get::<crate::entities::comp::Building>(entity) else {
            return;
        };
        let anchor = building.tile;
        let size = def.size.max(1);
        let per_cell = amount / (size * size) as f32 * 2.0;
        // The deposit rng is the live unit runtime's, like the fire/puddle pass.
        let Some(mut runtime) = world.remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        else {
            return;
        };
        for oy in 0..size {
            for ox in 0..size {
                let tx = anchor.x() + ox as i16 + def.size_offset as i16;
                let ty = anchor.y() + oy as i16 + def.size_offset as i16;
                if !grid.tiles.in_bounds(tx as i32, ty as i32) {
                    continue;
                }
                let _ = crate::combat::puddles::deposit(
                    world,
                    &grid.tiles,
                    &self.content,
                    tx,
                    ty,
                    liquid,
                    per_cell,
                    &mut runtime.rng,
                );
            }
        }
        world.insert_non_send(runtime);
    }

    /// Breaks the block at `(x, y)` through the real tile ops (`Tile.remove`).
    pub(crate) fn break_block(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        x: i16,
        y: i16,
    ) -> Option<BlockId> {
        let pos = TilePos::new(x, y);
        let entity = grid.entity_at(pos)?;
        let block = grid.block_at(pos).unwrap_or(BlockId::AIR);
        if block == BlockId::AIR {
            return None;
        }
        let team = world
            .get::<crate::entities::comp::TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);
        let rules = world
            .get_resource::<BuildRules>()
            .cloned()
            .unwrap_or_default();
        if !crate::world::build::valid_break(
            &self.content,
            grid,
            Some(world),
            &rules,
            team,
            i32::from(x),
            i32::from(y),
        ) {
            return None;
        }
        self.remove_building(world, grid, entity, x, y, team, block);
        Some(block)
    }

    /// `Tile.remove` without validation (`ConstructBlock.deconstructFinish`).
    pub(crate) fn remove_block_raw(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        x: i16,
        y: i16,
    ) -> Option<BlockId> {
        let pos = TilePos::new(x, y);
        let entity = grid.entity_at(pos)?;
        let block = grid.block_at(pos).unwrap_or(BlockId::AIR);
        if block == BlockId::AIR {
            return None;
        }
        let team = world
            .get::<crate::entities::comp::TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);
        self.remove_building(world, grid, entity, x, y, team, block);
        Some(block)
    }

    /// `Building.kill` -> `Call.buildDestroyed` server half: runs
    /// `onDestroyed`/`afterDestroyed` and removes the building through the
    /// tile ops without the break-permission gate. Returns the destroyed
    /// block and its center tile (PAR-0141/PAR-0086 combat deaths).
    pub(crate) fn destroy_building(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        entity: Entity,
    ) -> Option<(i16, i16, BlockId)> {
        let building = world.get::<crate::entities::comp::Building>(entity)?;
        let (pos, block) = (building.tile, building.block);
        if grid.entity_at(pos) != Some(entity) || block == BlockId::AIR {
            return None;
        }
        let team = world
            .get::<crate::entities::comp::TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);
        if let Some(inst) = world
            .get_resource::<BlockTable>()
            .and_then(|table| table.instance(block))
        {
            inst.behavior.on_destroyed(world, entity);
            inst.behavior.after_destroyed(world, entity);
        }
        let (x, y) = (pos.x(), pos.y());
        self.remove_building(world, grid, entity, x, y, team, block);
        Some((x, y, block))
    }

    /// `Building.pickedUp` -> `tile.remove()` without the `onDestroyed`
    /// refunds (PAR-0138 unit payload pickup). Returns the block and center
    /// tile, like [`Self::destroy_building`].
    pub(crate) fn pickup_building(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        entity: Entity,
    ) -> Option<(i16, i16, BlockId)> {
        let building = world.get::<crate::entities::comp::Building>(entity)?;
        let (pos, block) = (building.tile, building.block);
        if grid.entity_at(pos) != Some(entity) || block == BlockId::AIR {
            return None;
        }
        let team = world
            .get::<crate::entities::comp::TeamComp>(entity)
            .map(|team| team.team)
            .unwrap_or(0);
        let (x, y) = (pos.x(), pos.y());
        self.remove_building(world, grid, entity, x, y, team, block);
        Some((x, y, block))
    }
}
