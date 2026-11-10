// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The [`SimRuntime`] container: content/config storage, placement and config
//! resolution, and the runtime's own determinism digest.
//!
//! Upstream: core/src/mindustry/world/Build.java (validPlace, place);
//! core/src/mindustry/entities/comp/BuildingComp.java (placed).

use super::*;

/// The live building runtime attached to a [`Sim`].
///
/// Owns the content registry (block defs, unlock state) and the per-team block
/// counter; every ECS-side resource lives in the `Sim` world so the scheduled
/// update systems can read it.
pub struct SimRuntime {
    pub(super) config: SimRuntimeConfig,
    pub(super) content: ContentRegistry,
    pub(super) hooks: SimHooks,
    pub(super) log: WorldEventLog,
    pub(super) render: NoopRenderHooks,
    /// Entities whose facing edge is rescanned every tick (`BeamDrill`/
    /// `WallCrafter`; T5). Stale entries are dropped by the refresh pass.
    pub(super) facing_drills: Vec<Entity>,
    /// Live `ThermalGenerator` buildings whose floor-attribute sum is rescanned
    /// every tick (the behavior hook cannot see `Sim`'s grid). Stale entries are
    /// dropped by the refresh pass.
    pub(super) thermal_generators: Vec<Entity>,
    /// Live `BeamNode` buildings whose directional links are rebuilt whenever
    /// `WorldGrid.tile_changes` moves (H02 T15). Stale entries drop out.
    pub(super) beam_nodes: Vec<Entity>,
}

impl SimRuntime {
    /// Read-only content registry backing the runtime.
    pub fn content(&self) -> &ContentRegistry {
        &self.content
    }

    /// Mutable content registry (host unlock/research state; tests).
    pub fn content_mut(&mut self) -> &mut ContentRegistry {
        &mut self.content
    }

    /// Clears runtime bookkeeping after [`Sim::reset`] (entities are gone).
    pub(crate) fn reset(&mut self, world: &mut World) {
        world.insert_resource(BlockCounter::new());
    }

    /// Whether placement is allowed by the `BuildVisibility`/unlock gate.
    ///
    /// `Block.canBeBuilt()` excludes hidden/debug-only; the campaign gate also
    /// excludes sandbox/editor-only content and locked content, matching the
    /// task's placement rules (K-9). Outside a campaign `unlockedNow()` is
    /// always true, so only the visibility half applies.
    pub(crate) fn placement_allowed(&self, block: BlockId) -> bool {
        if !self.config.placement_gate {
            return true;
        }
        let Some(def) = self.content.block(block) else {
            return false;
        };
        if matches!(
            def.build_visibility,
            BuildVisibility::Hidden
                | BuildVisibility::DebugOnly
                | BuildVisibility::EditorOnly
                | BuildVisibility::SandboxOnly
        ) {
            return false;
        }
        !self.config.campaign || def.unlock.unlocked()
    }

    /// Lowers a wire [`crate::determinism::ConfigValue`] to the building config
    /// model (`TypeIO` whitelist; `from_p0(SelectBlock)` encodes raw block ids).
    pub fn resolve_config(
        &self,
        value: &crate::determinism::ConfigValue,
    ) -> Result<WorldConfigValue, CommandError> {
        use crate::determinism::ConfigValue as Wire;
        Ok(match value {
            Wire::None => WorldConfigValue::None,
            Wire::Bool(value) => WorldConfigValue::Bool(*value),
            Wire::Int(value) => WorldConfigValue::Number(f64::from(*value)),
            Wire::Float(value) => WorldConfigValue::Number(f64::from(*value)),
            Wire::Bytes(bytes) => WorldConfigValue::Bytes(bytes.iter().copied().collect()),
            Wire::Text(text) => WorldConfigValue::String(text.clone()),
            Wire::Point2(x, y) => WorldConfigValue::Point2(*x, *y),
            Wire::Point2Array(points) => {
                WorldConfigValue::Point2Array(points.iter().copied().collect())
            }
            Wire::IntArray(values) => WorldConfigValue::IntArray(values.clone()),
            Wire::ContentId { kind, id } => {
                let content_type = ContentType::ALL
                    .iter()
                    .copied()
                    .find(|type_| *type_ as u8 == *kind)
                    .ok_or(CommandError::UnknownContent(*id))?;
                let Some(reference) = self.content.get_by_id(content_type, i32::from(*id)) else {
                    return Err(CommandError::UnknownContent(*id));
                };
                match reference.type_ {
                    ContentType::Item => WorldConfigValue::Item(ItemId::new(reference.id)),
                    ContentType::Liquid => WorldConfigValue::Liquid(LiquidId::new(reference.id)),
                    ContentType::Block => WorldConfigValue::Block(BlockId::new(reference.id)),
                    ContentType::Unit => WorldConfigValue::Unit(UnitTypeId::new(reference.id)),
                    other => WorldConfigValue::Content(other, reference.id),
                }
            }
            Wire::Content(name) => match self.content.by_name(name) {
                Some(reference) => match reference.type_ {
                    ContentType::Item => WorldConfigValue::Item(ItemId::new(reference.id)),
                    ContentType::Liquid => WorldConfigValue::Liquid(LiquidId::new(reference.id)),
                    ContentType::Block => WorldConfigValue::Block(BlockId::new(reference.id)),
                    ContentType::Unit => WorldConfigValue::Unit(UnitTypeId::new(reference.id)),
                    other => WorldConfigValue::Content(other, reference.id),
                },
                None => {
                    let raw: u16 = name
                        .parse()
                        .map_err(|_| CommandError::UnknownContent(u16::MAX))?;
                    if usize::from(raw) < self.content.blocks().len() {
                        WorldConfigValue::Block(BlockId::new(raw))
                    } else {
                        return Err(CommandError::UnknownContent(raw));
                    }
                }
            },
        })
    }

    /// Places a block through the real tile ops (`Tile.setBlock`).
    ///
    /// Replaces a compatible occupied tile (`Block.canReplace`) instead of
    /// rejecting it; validates through `Build.validPlace`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn place(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        x: i16,
        y: i16,
        block: BlockId,
        rot: u8,
        team: u8,
        flags: crate::world::build::PlaceFlags,
    ) -> bool {
        if !self.can_place(world, grid, x, y, block, rot, team, flags) {
            return false;
        }
        self.set_block_raw(world, grid, x, y, block, rot, team);
        // Instant placement runs the `placed()` hooks too (T11: the plan's
        // acceptance uses `place_block`, and Java's non-construction placement
        // sites - `BaseGenerator` - call `placed()` explicitly).
        if let Some(entity) = grid.entity_at(TilePos::new(x, y)) {
            let grid_ref: &WorldGrid = grid;
            self.placed_hooks(world, grid_ref, entity);
        }
        true
    }

    /// `BuildingComp.placed()` + `PowerNode.placed()` (T11).
    ///
    /// Runs the behavior hook, then the two power auto-link halves:
    /// `BuildingComp.java:1359-1370` (a power block links each node in range to
    /// itself) and `PowerNode.java:400-411` (a fresh node with no links grabs
    /// its potential links). Never called for map materialization or for the
    /// construct block itself.
    pub(crate) fn placed_hooks(&mut self, world: &mut World, grid: &WorldGrid, entity: Entity) {
        if let Some(behavior) = world.get_resource::<BlockTable>().and_then(|table| {
            world
                .get::<crate::entities::comp::Building>(entity)
                .and_then(|building| table.instance(building.block))
                .map(|inst| inst.behavior.clone())
        }) {
            behavior.placed(world, entity);
        }

        // `BuildingComp.placed`: `(consumesPower || outputsPower) && hasPower
        // && connectedPower` (`connectedPower` defaults true and is not modeled).
        if let Some(params) = read_power_info(world, entity)
            && params.has_power
            && (params.consumes || params.outputs)
        {
            let Some(mut graphs) = world.remove_resource::<PowerGrids>() else {
                return;
            };
            let mut nodes: Vec<Entity> = Vec::new();
            crate::world::blocks::power::nodes::get_node_links(world, grid, entity, &mut nodes);
            let pack = crate::world::blocks::power::module::pack(world, entity);
            for node in nodes {
                let already = world
                    .get::<PowerModule>(node)
                    .is_some_and(|module| module.links.contains(&pack));
                if !already {
                    crate::world::blocks::power::nodes::configure_link(
                        &mut graphs,
                        world,
                        grid,
                        node,
                        pack,
                    );
                }
            }
            world.insert_resource(graphs);
        }

        // `PowerNode.placed`: `!client && power.links.size == 0`.
        let Some(config) = world.get::<PowerNodeConfig>(entity) else {
            return;
        };
        let links = world
            .get::<PowerModule>(entity)
            .map(|module| module.links.len())
            .unwrap_or(0);
        if links > 0 || !config.autolink {
            return;
        }
        let Some(mut graphs) = world.remove_resource::<PowerGrids>() else {
            return;
        };
        let mut candidates: Vec<Entity> = Vec::new();
        crate::world::blocks::power::nodes::get_potential_links(
            world,
            grid,
            entity,
            &mut candidates,
        );
        for other in candidates {
            let pack = crate::world::blocks::power::module::pack(world, other);
            crate::world::blocks::power::nodes::configure_link(
                &mut graphs,
                world,
                grid,
                entity,
                pack,
            );
        }
        world.insert_resource(graphs);
    }

    /// `Build.validPlace` plus the visibility/unlock gate
    /// (`BuilderComp.allowBuildCurrent`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn can_place(
        &self,
        world: &World,
        grid: &WorldGrid,
        x: i16,
        y: i16,
        block: BlockId,
        rot: u8,
        team: u8,
        flags: crate::world::build::PlaceFlags,
    ) -> bool {
        if !self.placement_allowed(block) {
            return false;
        }
        let rules = world
            .get_resource::<BuildRules>()
            .cloned()
            .unwrap_or_default();
        let Some(table) = world.get_resource::<BlockTable>() else {
            return false;
        };
        let Some(counter) = world.get_resource::<BlockCounter>() else {
            return false;
        };
        crate::world::build::valid_place_query(
            &self.content,
            table,
            &rules,
            counter,
            grid,
            Some(world),
            block,
            team,
            rot,
            i32::from(x),
            i32::from(y),
            flags,
        )
    }

    /// `Schematics.placeLoadout` against the live runtime (launcher branch,
    /// `game/Schematics.java:465-500`): places `schem` centered on its core
    /// tile at `(x, y)` and returns the placed core entities.
    #[allow(clippy::too_many_arguments)]
    pub fn place_schematic(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        schem: &crate::game::schematic::Schematic,
        x: i16,
        y: i16,
        team: u8,
        check: bool,
    ) -> Vec<Entity> {
        let mut ctx = WorldCtx {
            grid,
            content: &self.content,
            ecs: world,
            hooks: &self.hooks,
            render: &self.render,
            log: &mut self.log,
        };
        crate::game::schematics::place_loadout(
            &mut ctx,
            schem,
            i32::from(x),
            i32::from(y),
            team,
            check,
        )
    }

    /// Runs `f` with the live [`crate::world::ops::WorldCtx`] (same
    /// construction as `place_schematic`; H05 T5/T6 generator structure
    /// passes).
    pub fn with_world_ctx<R>(
        &mut self,
        world: &mut World,
        grid: &mut WorldGrid,
        f: impl FnOnce(&mut crate::world::ops::WorldCtx<'_>) -> R,
    ) -> R {
        let mut ctx = crate::world::ops::WorldCtx {
            grid,
            content: &self.content,
            ecs: world,
            hooks: &self.hooks,
            render: &self.render,
            log: &mut self.log,
        };
        f(&mut ctx)
    }

    /// Deterministic digest of the live runtime state (ascending seq order).
    ///
    /// Not folded into [`Sim::checksum`]: the P0 stream stays byte-identical,
    /// and this is the runtime's own determinism gate.
    pub fn checksum(&self, world: &World) -> Checksum {
        let mut c = Checksummer::new();
        let mut buildings: Vec<(u64, Entity)> = world
            .iter_entities()
            .filter_map(|entity_ref| {
                entity_ref.get::<crate::entities::comp::Building>()?;
                let seq = entity_ref.get::<crate::ecs::EntitySeq>()?.0;
                Some((seq, entity_ref.id()))
            })
            .collect();
        buildings.sort_by_key(|(seq, entity)| (*seq, entity.index()));
        for (seq, entity) in buildings {
            c.part(&seq);
            if let Some(building) = world.get::<crate::entities::comp::Building>(entity) {
                c.part(&building.block.raw());
                c.part(&building.rotation);
                c.part(&u8::from(building.enabled));
                c.part(&building.efficiency.to_bits());
                c.part(&building.time_scale.to_bits());
            }
            if let Some(power) = world.get::<PowerModule>(entity) {
                c.part(&power.status.to_bits());
                c.part(&power.graph.slot);
                c.part(&power.graph.generation);
            }
            if let Some(items) = world.get::<ItemModule>(entity) {
                c.part(&items.total);
            }
            if let Some(liquids) = world.get::<LiquidModule>(entity) {
                c.part(&liquids.current().to_bits());
            }
        }
        let counter_len = world
            .get_resource::<BlockCounter>()
            .map_or(0, |counter| counter.counts.len());
        c.part(&(counter_len as u64));
        c.finish()
    }
}
