// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Opt-in live simulation runtime: the plan-07 building stack on `Sim`.
//!
//! The P0 `Sim` path stores a bare `BuildingComp` and a grid cell per placed
//! block, so every building is inert (gap5 GAP-1, inventory R0-2). This module
//! provisions the real runtime — [`BlockTable`], `BuildRules`, `BuildClock`,
//! `TileBuilds`, `ModuleDims` and [`PowerGrids`] — into the `Sim` ECS and routes
//! `Place`/`Break` through the plan-06 [`WorldCtx`] tile ops so entities carry
//! the full plan-07 component set and the schedule's `UpdateBuildings`/
//! `UpdatePowerGraph` systems have something to run.
//!
//! Ported from `core/src/mindustry/core/Logic.java` (`updateEntities`),
//! `entities/comp/BuildingComp.java` (`update`/`updateConsumption`,
//! `onProximityAdded`/`onProximityRemoved`) and `world/Tile.java` (`setBlock`).
//!
//! The runtime is explicitly opt-in: a `Sim` that never calls
//! [`Sim::install_block_runtime`] is byte-identical to the P0 spine (no ECS
//! resources, no extra checksum parts), so the plan-05 checkpoint goldens stay
//! frozen. Placement uses the harness instant-build convention (there is no
//! builder-unit progress path in the live spine yet).

use std::sync::atomic::{AtomicU64, Ordering};

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{
    Attribute, BlockId, BlockKind, BuildVisibility, ContentError, ContentRegistry, ContentType,
    ItemId, LiquidId, MemoryBundle, MemoryUnlockStore, UnitTypeId, create_base_content,
};
use crate::determinism::{Checksum, Checksummer, CommandError};
use crate::ecs::{BuildingComp, TeamId};
use crate::game::runtime::CampaignRuntime;
use crate::world::block::{BlockError, BlockTable};
use crate::world::blocks::power::{
    PowerGrids, PowerNodeConfig, power_graph_removed, read_power_info, update_power_graph,
};
use crate::world::building_io::{self, DecodedBase};
use crate::world::config::ConfigValue as WorldConfigValue;
use crate::world::limits::{BlockCounter, BuildRules};
use crate::world::modules::{ItemModule, LiquidModule, LiquidTraits, ModuleDims, PowerModule};
use crate::world::ops::{WorldCtx, WorldEventLog};
use crate::world::proximity::{remove_from_proximity, update_proximity};
use crate::world::update::BuildClock;
use crate::world::{NewBuilding, NoopRenderHooks, TileBuilds, TilePos, WorldGrid, WorldHooks};

use super::Sim;

/// Errors raised while installing the live runtime.
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum RuntimeError {
    /// Content bootstrap failed.
    #[error("live runtime content boot failed: {0}")]
    Content(#[from] ContentError),
    /// The block behavior table could not be built.
    #[error("live runtime block table failed: {0}")]
    BlockTable(#[from] BlockError),
}

/// Live-runtime configuration (all gating is default-off).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SimRuntimeConfig {
    /// Gate placement on `BuildVisibility`/unlock state (`Block.canBeBuilt`,
    /// `UnlockableContent.unlockedNow`; gap inventory K-9). Off by default so a
    /// vanilla custom game can build everything.
    pub placement_gate: bool,
    /// Whether the match is a campaign (`state.isCampaign()`); when set (with
    /// `placement_gate`), locked content cannot be placed.
    pub campaign: bool,
}

impl SimRuntimeConfig {
    /// Enables the placement gate.
    pub const fn with_placement_gate(mut self, placement_gate: bool) -> Self {
        self.placement_gate = placement_gate;
        self
    }

    /// Marks the match as a campaign (locked content is unplaceable).
    pub const fn campaign(mut self, campaign: bool) -> Self {
        self.campaign = campaign;
        self
    }
}

/// Content name → dense-id index for scheduled systems that resolve content
/// names over the ECS (`ObjectiveEnv.team_has_item`/`placed_block_count`);
/// built once by the runtime install from its content registry.
#[derive(bevy_ecs::prelude::Resource, Debug, Default)]
pub struct ContentIndex {
    /// Item name → dense id (`ContentRegistry::item_id`).
    pub item_ids: std::collections::HashMap<String, u16>,
    /// Dense item id → name (id-indexed; `ContentRegistry::items` order).
    pub item_names: Vec<String>,
    /// Dense item id → `Item.buildable` (`CoreBlock.incinerateNonBuildable`).
    pub item_buildable: Vec<bool>,
    /// Block name → dense id (`ContentRegistry::block_id`).
    pub block_ids: std::collections::HashMap<String, u16>,
    /// Unit name → dense id (`ContentRegistry::unit_id`; `UnitCountObjective`).
    pub unit_ids: std::collections::HashMap<String, u16>,
    /// Dense unit id → name (id-indexed; `ContentRegistry::units` order).
    pub unit_names: Vec<String>,
}

/// Spawns runtime buildings for [`WorldCtx`] tile ops.
struct SimHooks {
    seq: AtomicU64,
    item_count: usize,
    liquid_count: usize,
}

impl WorldHooks for SimHooks {
    fn new_building(&self, world: &mut World, request: NewBuilding) -> Option<Entity> {
        let inst = world
            .get_resource::<BlockTable>()
            .and_then(|table| table.instance(request.block))?;
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let tile = TilePos::new(request.x, request.y);
        let entity = inst.spawn(
            world,
            seq,
            tile,
            request.team,
            request.rot,
            self.item_count,
            self.liquid_count,
        );
        // The P0 checksum/dump stream walks `BuildingComp` entities; keeping it
        // on runtime entities makes `Sim::checksum`/`entities_by_seq` cover the
        // live path without a second entity index.
        world.entity_mut(entity).insert(BuildingComp {
            pos: tile,
            block: request.block,
            team: TeamId(request.team),
            rot: request.rot,
        });
        // K-3/T2: `PowerNodeBehavior::create_state` already inserts the
        // behavior's real `PowerNodeConfig`; only fall back to the default when
        // the kind has no registered behavior (mod nodes). The old
        // unconditional insert overwrote the knobs after `spawn` (PAR: node
        // large/surge tower dropped to 3 links / range 6).
        if matches!(
            inst.def.kind,
            BlockKind::PowerNode | BlockKind::LongPowerNode
        ) && world.get::<PowerNodeConfig>(entity).is_none()
        {
            world.entity_mut(entity).insert(PowerNodeConfig::default());
        }
        Some(entity)
    }

    /// `BuildingComp.onRemoved` (`BuildingComp.java:1145-1150`): dispatch the
    /// block behavior's removal hook (the turret's detached `block` unit proxy
    /// is dropped here, I01 T12).
    fn on_removed(&self, world: &mut World, entity: Entity) {
        if let Some(behavior) = world.get_resource::<BlockTable>().and_then(|table| {
            world
                .get::<crate::entities::comp::Building>(entity)
                .and_then(|building| table.instance(building.block))
                .map(|inst| inst.behavior.clone())
        }) {
            behavior.on_removed(world, entity);
        }
    }
}

/// The live building runtime attached to a [`Sim`].
///
/// Owns the content registry (block defs, unlock state) and the per-team block
/// counter; every ECS-side resource lives in the `Sim` world so the scheduled
/// update systems can read it.
pub struct SimRuntime {
    config: SimRuntimeConfig,
    content: ContentRegistry,
    hooks: SimHooks,
    log: WorldEventLog,
    render: NoopRenderHooks,
    /// Entities whose facing edge is rescanned every tick (`BeamDrill`/
    /// `WallCrafter`; T5). Stale entries are dropped by the refresh pass.
    facing_drills: Vec<Entity>,
    /// Live `ThermalGenerator` buildings whose floor-attribute sum is rescanned
    /// every tick (the behavior hook cannot see `Sim`'s grid). Stale entries are
    /// dropped by the refresh pass.
    thermal_generators: Vec<Entity>,
    /// Live `BeamNode` buildings whose directional links are rebuilt whenever
    /// `WorldGrid.tile_changes` moves (H02 T15). Stale entries drop out.
    beam_nodes: Vec<Entity>,
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
    fn drop_deconstruct_liquid(
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

    /// Shared `Tile.remove` half of break/destroy.
    /// `BeamDrill.updateFacing`/`WallCrafter.getEfficiency` (T5): whether this
    /// building's facing edge is rescanned every tick.
    fn is_facing_drill(&self, world: &World, entity: Entity) -> bool {
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
    fn thermal_attribute(&self, world: &World, entity: Entity) -> Option<Attribute> {
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
    fn is_beam_node(&self, world: &World, entity: Entity) -> bool {
        world
            .get::<crate::world::blocks::power::nodes::BeamNodeState>(entity)
            .is_some()
    }

    #[allow(clippy::too_many_arguments)]
    fn remove_building(
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

    fn refresh_proximity_at(&self, world: &mut World, grid: &WorldGrid, x: i16, y: i16) {
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
    fn remove_power_graph(&mut self, world: &mut World, grid: &WorldGrid, entity: Entity) {
        if world.get::<PowerModule>(entity).is_none() {
            return;
        }
        let Some(mut graphs) = world.remove_resource::<PowerGrids>() else {
            return;
        };
        power_graph_removed(&mut graphs, world, grid, entity);
        world.insert_resource(graphs);
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

/// Boots the vanilla content registry the way the harness does.
pub(crate) fn load_vanilla_content() -> Result<ContentRegistry, RuntimeError> {
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)?;
    content.init()?;
    content.post_init()?;
    // `Blocks.java` turret `ammo(...)` tables: the anonymous ammo bullets join
    // the registry so placed turrets resolve a live `TurretBehavior` (PAR-0086).
    crate::world::blocks::defense::turrets::install_turret_ammo(&mut content);
    Ok(content)
}

/// One building captured for the lost-sector replay (`Control.playSector`'s
/// `previousBuildings`/`previousDerelicts` payload).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReplayBuilding {
    /// Tile x.
    pub x: i16,
    /// Tile y.
    pub y: i16,
    /// Block content id.
    pub block: crate::content::BlockId,
    /// Rotation.
    pub rot: u8,
    /// Current health.
    pub health: f32,
}

impl Sim {
    /// Whether the opt-in live building runtime is installed.
    pub fn has_block_runtime(&self) -> bool {
        self.block_runtime.is_some()
    }

    /// Read-only live runtime, if installed.
    pub fn block_runtime(&self) -> Option<&SimRuntime> {
        self.block_runtime.as_ref()
    }

    /// Mutable live runtime, if installed.
    pub fn block_runtime_mut(&mut self) -> Option<&mut SimRuntime> {
        self.block_runtime.as_mut()
    }

    /// `BeamDrill.updateFacing`/`WallCrafter.getEfficiency` (T5): rescans the
    /// facing edge of every registered facing drill once per tick.
    ///
    /// The behaviors cannot see the grid (`update_tile` takes only `&mut World`),
    /// so `Sim::tick` runs this pass after the scheduled systems and before the
    /// builder pass.
    pub(crate) fn refresh_facing_drills(&mut self) {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return;
        };
        if runtime.facing_drills.is_empty() {
            return;
        }
        let drills = std::mem::take(&mut runtime.facing_drills);
        for entity in drills {
            if self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .is_none()
            {
                continue;
            }
            if runtime.is_facing_drill(&self.ecs.0, entity) {
                crate::world::behavior::production::refresh_drill_ore(
                    &mut self.ecs.0,
                    &self.grid,
                    runtime.content(),
                    entity,
                );
                runtime.facing_drills.push(entity);
            }
        }
    }

    /// `UnitAssemblerBuild.updateTile`'s `wasOccupied = checkSolid(spawn,
    /// false)` (`UnitAssembler.java:515`), computed before the scheduled
    /// building pass because the behavior cannot see the live grid (L06 T1).
    pub(crate) fn refresh_assembler_occupancy(&mut self) {
        use crate::world::blocks::units::UnitAssembler;
        use crate::world::blocks::units::behavior::{assembler_check_solid, assembler_spawn_at};
        let assemblers: Vec<Entity> = {
            let mut query = self
                .ecs
                .0
                .query_filtered::<Entity, bevy_ecs::query::With<UnitAssembler>>();
            query.iter(&self.ecs.0).collect()
        };
        if assemblers.is_empty() {
            return;
        }
        let mut results: Vec<(Entity, bool)> = Vec::with_capacity(assemblers.len());
        {
            let world = &self.ecs.0;
            let Some(runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            else {
                return;
            };
            let content = &runtime.content;
            let table = world.get_resource::<crate::world::block::BlockTable>();
            for entity in assemblers {
                let Some(state) = world.get::<UnitAssembler>(entity) else {
                    continue;
                };
                let Some(output) = state.plan().and_then(|plan| content.unit(plan.unit)) else {
                    continue;
                };
                let size = world
                    .get::<crate::entities::comp::Building>(entity)
                    .and_then(|building| table.and_then(|table| table.get(building.block)))
                    .map_or(1, |instance| instance.def.size);
                let Some(spawn) = assembler_spawn_at(world, entity, state.area_size, size) else {
                    continue;
                };
                let occupied =
                    assembler_check_solid(world, &self.grid, content, output, spawn, false);
                results.push((entity, occupied));
            }
        }
        for (entity, occupied) in results {
            if let Some(mut state) = self.ecs.0.get_mut::<UnitAssembler>(entity) {
                state.was_occupied = occupied;
            }
        }
    }

    /// `ThermalGeneratorBuild.onProximityAdded` rescan (T14): the behavior hook
    /// cannot see the live grid (`Sim.grid` is not an ECS resource), so the
    /// whole floor-attribute sum is recomputed for every registered thermal
    /// generator once per tick.
    pub(crate) fn refresh_thermal_generators(&mut self) {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return;
        };
        if runtime.thermal_generators.is_empty() {
            return;
        }
        let generators = std::mem::take(&mut runtime.thermal_generators);
        for entity in generators {
            if self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .is_none()
            {
                continue;
            }
            if let Some(attribute) = runtime.thermal_attribute(&self.ecs.0, entity) {
                crate::world::blocks::power::behavior::refresh_thermal_sum_grid(
                    &mut self.ecs.0,
                    &self.grid,
                    entity,
                    attribute,
                );
                runtime.thermal_generators.push(entity);
            }
        }
    }

    /// `ConsumeItemExplode.update` (`ConsumeItemExplode.java:30-42`): an
    /// explosive item (`explosiveness >= threshold`) stored in a generator
    /// damages it by `damage` (4) with chance
    /// `build.delta() * baseChance * clamp(explosiveness - threshold)` and
    /// fires `Trigger.blastGenerator`, only while `rules.reactorExplosions`
    /// (`Blocks.java:2537,2571` use both consumers).
    pub(crate) fn update_item_explode_consumers(&mut self) {
        use crate::world::blocks::power::generator::{
            GeneratorConfig, first_explosive_item, item_explosiveness, reactor_explosions_enabled,
        };

        if !reactor_explosions_enabled(&self.ecs.0) {
            return;
        }
        // Collect candidates first: the body mutates health, draws RNG and
        // fires the trigger.
        let candidates: Vec<(
            Entity,
            crate::world::blocks::power::generator::ItemExplodeConfig,
        )> = {
            let mut query = self.ecs.0.query::<(Entity, &GeneratorConfig)>();
            query
                .iter(&self.ecs.0)
                .filter_map(|(entity, config)| config.explode_items.map(|config| (entity, config)))
                .collect()
        };
        if candidates.is_empty() {
            return;
        }
        let Some(runtime) = self.block_runtime() else {
            return;
        };
        let explosiveness = item_explosiveness(runtime.content());
        for (entity, explode) in candidates {
            // `Building.updateConsumption` runs `updateConsumers` only while
            // enabled with `shouldConsume() && productionValid()` and
            // `efficiency > 0` (`Building.java:2166-2204`); the generator's
            // same-tick efficiency comes from `update_generator_with`.
            let (enabled, efficiency) = self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .map(|building| (building.enabled, building.efficiency))
                .unwrap_or((false, 0.0));
            if !enabled || efficiency <= 0.0 {
                continue;
            }
            let Some(module) = self.ecs.0.get::<ItemModule>(entity) else {
                continue;
            };
            let Some(item) = first_explosive_item(module, &explosiveness, explode.threshold) else {
                continue;
            };
            let value = explosiveness.get(item.index()).copied().unwrap_or(0.0);
            let delta = crate::world::update::delta(&self.ecs.0, entity);
            let chance = explode.chance(delta, value);
            // `Mathf.chance(d)`: `d >= 1f || rand.nextFloat() < d`.
            let fired = chance >= 1.0
                || self
                    .rng_streams
                    .chance(crate::determinism::RngStream::Sim, chance as f64);
            if !fired {
                continue;
            }
            if let Some(mut health) = self.ecs.0.get_mut::<crate::entities::comp::Health>(entity) {
                health.health -= explode.damage;
            }
            // `Fx.generatespark.at(x + range, y + range)` (`range` = the
            // building's `size * tilesize / 2`, Java `ConsumeItemExplode`).
            let (x, y) = self
                .ecs
                .0
                .get::<crate::entities::comp::Pos>(entity)
                .map(|pos| (pos.x, pos.y))
                .unwrap_or((0.0, 0.0));
            crate::world::blocks::power::explosion::generatespark(&mut self.ecs.0, x, y);
            self.triggers
                .fire(super::Trigger::BlastGenerator, &mut self.ecs.0);
        }
    }

    /// `PowerGenerator.onExplosion`'s **sim half** (J04 T4,
    /// `PowerGenerator.java:149-201`): radius damage, ignition fires (delayed
    /// by `dst / explosionSpeed` through the `SimClock`), boulder/prop
    /// breaking, fireball bullets and liquid puddles. The view half
    /// (`explodeEffect`/`explodeSound`, shake, scorch decal) stays with
    /// `ExplosionSink`.
    pub(crate) fn apply_generator_explosions(&mut self) {
        use crate::world::blocks::power::explosion::PendingGeneratorExplosions;

        // Due ignition fires (`Time.run(dst / explosionSpeed, ...)`): the
        // delay is scheduled as a tick count, matching the `SimClock` unit.
        let tick = self.state.tick;
        let due_fires: Vec<(i16, i16)> = {
            let mut due = Vec::new();
            if let Some(mut pending) = self
                .ecs
                .0
                .get_resource_mut::<crate::world::blocks::power::explosion::PendingExplosionFires>(
            ) {
                let mut rest = Vec::new();
                for (due_tick, tx, ty) in pending.entries.drain(..) {
                    if due_tick <= tick {
                        due.push((tx, ty));
                    } else {
                        rest.push((due_tick, tx, ty));
                    }
                }
                pending.entries = rest;
            }
            due
        };
        for (tx, ty) in due_fires {
            let _ = crate::combat::fires::create(&mut self.ecs.0, tx, ty, &mut self.rng_streams);
        }

        let pending = self
            .ecs
            .0
            .get_resource_mut::<PendingGeneratorExplosions>()
            .map(|mut pending| std::mem::take(&mut pending.0))
            .unwrap_or_default();
        if pending.is_empty() {
            return;
        }

        let Some(runtime) = self.block_runtime.as_ref() else {
            return;
        };
        let content = &runtime.content;
        let hooks = &runtime.hooks;
        let render = &runtime.render;
        let ts = crate::config::TILESIZE as f32;
        let (width, height) = (self.grid.tiles.width, self.grid.tiles.height);
        let mut broke_prop = false;
        for explosion in pending {
            // `if(explosionDamage > 0) Damage.damage(x, y, explosionRadius *
            // tilesize, explosionDamage)`.
            if explosion.damage > 0.0 {
                let _ = crate::combat::damage::area::damage_area(
                    &mut self.ecs.0,
                    content,
                    None,
                    explosion.x,
                    explosion.y,
                    explosion.radius * ts,
                    explosion.damage,
                    crate::combat::damage::area::DamageOptions::default(),
                );
            }

            // `Geometry.circle(tileX(), tileY(), explosionRadius, ...)`:
            // ignition fires (delayed by `dst / explosionSpeed`) and prop
            // breaking (`unitMoveBreakable`).
            if explosion.ignition_chance > 0.0 || explosion.breaks_props {
                let cx = (explosion.x / ts) as i32;
                let cy = (explosion.y / ts) as i32;
                let radius = explosion.radius.max(0.0) as i32;
                let mut breaks: Vec<(i16, i16)> = Vec::new();
                let mut fires: Vec<(f32, i16, i16)> = Vec::new();
                crate::maps::generators::basic::circle(cx, cy, width, height, radius, |tx, ty| {
                    let dst = (((cx - tx).pow(2) + (cy - ty).pow(2)) as f32).sqrt();
                    if explosion.ignition_chance > 0.0 {
                        let chance = explosion.ignition_chance
                            * if explosion.scale_ignition_chance {
                                1.0 - (dst / explosion.radius.max(0.0001)).sqrt()
                            } else {
                                1.0
                            };
                        if self.rng_streams.chance(
                            crate::determinism::RngStream::Sim,
                            chance.clamp(0.0, 1.0) as f64,
                        ) {
                            fires.push((dst / explosion.speed.max(0.0001), tx as i16, ty as i16));
                        }
                    }
                    if explosion.breaks_props
                        && self
                            .grid
                            .tiles
                            .getn(tx, ty)
                            .map(|tile| tile.block)
                            .and_then(|block| content.block(block))
                            .is_some_and(|def| def.unit_move_breakable)
                    {
                        breaks.push((tx as i16, ty as i16));
                    }
                });
                if !breaks.is_empty() {
                    // `ConstructBlock.deconstructFinish(t, t.block(), null)`:
                    // props have no building entity, so the tile op removes
                    // them directly.
                    let mut log = crate::world::ops::WorldEventLog::default();
                    for (tx, ty) in breaks {
                        let mut ctx = crate::world::ops::WorldCtx {
                            grid: &mut self.grid,
                            content,
                            ecs: &mut self.ecs.0,
                            hooks,
                            render,
                            log: &mut log,
                        };
                        ctx.set_air(tx, ty);
                        broke_prop = true;
                    }
                }
                for (delay, tx, ty) in fires {
                    // `Time.run(dst / explosionSpeed, () -> Fires.create(t))`.
                    let due = tick + delay.max(0.0).ceil() as u64;
                    self.ecs
                        .0
                        .get_resource_or_insert_with(
                            crate::world::blocks::power::explosion::PendingExplosionFires::default,
                        )
                        .entries
                        .push((due, tx, ty));
                }
            }

            // `if(explosionFireballs > 0)`: `Mathf.random(1, explosionFireballs)`
            // `Bullets.fireball.createNet(Team.derelict, ...)` — queued for the
            // fire/puddle pass, which owns the bullet spawn seam.
            if explosion.fireballs > 0 {
                let amount = 1 + self.rng_streams.random(
                    crate::determinism::RngStream::Sim,
                    explosion.fireballs.max(1),
                );
                for _ in 0..amount {
                    self.ecs
                        .0
                        .get_resource_or_insert_with(
                            crate::combat::puddles::PendingFireballs::default,
                        )
                        .0
                        .push((explosion.x, explosion.y));
                }
            }

            // `if(explosionPuddleLiquid != null)`: `explosionPuddles` deposits
            // at `Tmp.v1.trns(Mathf.random(360f), Mathf.random(range))`.
            if let Some(liquid) = explosion.puddle_liquid {
                for _ in 0..explosion.puddles.max(0) {
                    let angle =
                        self.rng_streams
                            .range(crate::determinism::RngStream::Sim, 0.0, 360.0);
                    let dist = self.rng_streams.range(
                        crate::determinism::RngStream::Sim,
                        0.0,
                        explosion.puddle_range,
                    );
                    let px = explosion.x + angle.to_radians().cos() * dist;
                    let py = explosion.y + angle.to_radians().sin() * dist;
                    let tx = (px / ts) as i32;
                    let ty = (py / ts) as i32;
                    if !self.grid.tiles.in_bounds(tx, ty) {
                        continue;
                    }
                    let _ = crate::combat::puddles::deposit(
                        &mut self.ecs.0,
                        &self.grid.tiles,
                        content,
                        tx as i16,
                        ty as i16,
                        liquid,
                        explosion.puddle_amount,
                        &mut self.rng_streams,
                    );
                }
            }
        }
        if broke_prop {
            // Prop breaking mutated blocks; rebuild placement/proximity caches.
            self.refresh_unit_runtime();
        }
    }

    /// `BeamNode.updateTile` -> `updateDirections` (T15): rebuilds the four
    /// directional links of every registered beam node whose
    /// `WorldGrid.tile_changes` view is stale.
    pub(crate) fn refresh_beam_nodes(&mut self) {
        let Some(runtime) = self.block_runtime.as_mut() else {
            return;
        };
        if runtime.beam_nodes.is_empty() {
            return;
        }
        let changes = self.grid.tile_changes;
        let nodes = std::mem::take(&mut runtime.beam_nodes);
        for entity in nodes {
            if self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .is_none()
            {
                continue;
            }
            let stale = self
                .ecs
                .0
                .get::<crate::world::blocks::power::nodes::BeamNodeState>(entity)
                .is_some_and(|state| state.last_change != changes);
            if stale && let Some(mut graphs) = self.ecs.0.remove_resource::<PowerGrids>() {
                crate::world::blocks::power::nodes::update_beam_directions(
                    &mut graphs,
                    &mut self.ecs.0,
                    &self.grid,
                    entity,
                );
                self.ecs.0.insert_resource(graphs);
            }
            runtime.beam_nodes.push(entity);
        }
    }

    /// Installs the live building runtime with default configuration (no
    /// placement gate). Content is booted from the vanilla registry.
    pub fn install_block_runtime(&mut self) -> Result<(), RuntimeError> {
        self.install_block_runtime_with(SimRuntimeConfig::default())
    }

    /// Installs the live building runtime with explicit configuration.
    pub fn install_block_runtime_with(
        &mut self,
        config: SimRuntimeConfig,
    ) -> Result<(), RuntimeError> {
        let content = load_vanilla_content()?;
        self.install_block_runtime_from(config, content)
    }

    /// Installs the live building runtime over a prebuilt content registry
    /// (host unlock/research state; `MindSimHost`'s content snapshot).
    ///
    /// Inserts the resources the scheduled runtime systems read. Resource
    /// insertion is idempotent-safe (`init_resource` keeps a host-supplied
    /// `BuildRules`/`BuildClock`), and building it before any entity exists
    /// keeps ECS entity indices stable.
    pub fn install_block_runtime_from(
        &mut self,
        config: SimRuntimeConfig,
        content: ContentRegistry,
    ) -> Result<(), RuntimeError> {
        if self.block_runtime.is_some() {
            return Ok(());
        }
        let table = BlockTable::build_default(&content)?;
        let item_count = content.items().len();
        let liquid_count = content.liquids().len();
        let mut content_index = ContentIndex::default();
        for item in content.items() {
            content_index
                .item_ids
                .insert(item.name.clone(), item.id.index() as u16);
        }
        content_index.item_names = content
            .items()
            .iter()
            .map(|item| item.name.clone())
            .collect();
        content_index.item_buildable = content.items().iter().map(|item| item.buildable).collect();
        for block in content.blocks() {
            content_index
                .block_ids
                .insert(block.name.clone(), block.id.index() as u16);
        }
        for unit in content.units() {
            content_index
                .unit_ids
                .insert(unit.name.clone(), unit.id.index() as u16);
        }
        content_index.unit_names = content
            .units()
            .iter()
            .map(|unit| unit.name.clone())
            .collect();
        self.ecs.0.insert_resource(content_index);
        self.ecs.0.insert_resource(table);
        self.ecs.0.init_resource::<BuildRules>();
        self.ecs.0.init_resource::<BuildClock>();
        self.ecs.0.init_resource::<TileBuilds>();
        self.ecs.0.insert_resource(ModuleDims {
            items: item_count,
            liquids: liquid_count,
        });
        // T1: coolant/incinerable/heat-capacity vectors for behaviors that run
        // without a content-registry borrow (`ConsumeCoolant`, `Incinerator`,
        // mixing reactions, force-projector coolant).
        self.ecs
            .0
            .insert_resource(LiquidTraits::from_content(&content));
        self.ecs.0.insert_resource(PowerGrids::new());
        // I04 T3: the reconstructor's `Trigger.cannotUpgrade` marker (`&World`
        // hook, interior mutability); drained by `Sim::tick`.
        self.ecs
            .0
            .init_resource::<crate::world::blocks::units::behavior::CannotUpgradeFired>();
        // The live per-team block counter (`Build.validPlace` limits + the
        // runtime checksum) is a world resource so scheduled systems (the
        // campaign team AI) can read it (PAR-0139 round 2).
        self.ecs.0.insert_resource(BlockCounter::new());
        self.block_runtime = Some(SimRuntime {
            config,
            content,
            hooks: SimHooks {
                seq: AtomicU64::new(0),
                item_count,
                liquid_count,
            },
            log: WorldEventLog::default(),
            render: NoopRenderHooks,
            facing_drills: Vec::new(),
            thermal_generators: Vec::new(),
            beam_nodes: Vec::new(),
        });
        // A host may install the runtime after loading a grid with pre-placed
        // power buildings (`World.endMapLoad` rebuild).
        self.rebuild_power_graphs();
        Ok(())
    }

    /// Deterministic digest of the live runtime state; empty when not installed.
    pub fn runtime_checksum(&self) -> Checksum {
        match &self.block_runtime {
            Some(runtime) => runtime.checksum(&self.ecs.0),
            None => Checksummer::new().finish(),
        }
    }

    /// `BuildingComp.changeTeam` for `Call.setTeams` (`BuildingComp.java`):
    /// sets the ECS team comps, moves the block in the team counter and
    /// re-merges the power graph.
    pub fn set_building_team(&mut self, entity: Entity, team: u8) {
        let Some(block) = self
            .ecs
            .0
            .get::<crate::entities::comp::Building>(entity)
            .map(|building| building.block)
        else {
            return;
        };
        let old_team = self
            .ecs
            .0
            .get::<crate::entities::comp::TeamComp>(entity)
            .map(|comp| comp.team)
            .unwrap_or(0);
        if old_team == team {
            return;
        }
        if let Some(mut comp) = self
            .ecs
            .0
            .get_mut::<crate::entities::comp::TeamComp>(entity)
        {
            comp.team = team;
        }
        if let Some(mut comp) = self.ecs.0.get_mut::<BuildingComp>(entity) {
            comp.team = TeamId(team);
        }
        if let Some(mut counter) = self.ecs.0.get_resource_mut::<BlockCounter>() {
            counter.remove(old_team, block, 1);
            counter.add(team, block, 1);
        }
        let Sim {
            ecs,
            grid,
            block_runtime,
            ..
        } = self;
        if let Some(runtime) = block_runtime.as_mut() {
            runtime.remove_power_graph(&mut ecs.0, grid, entity);
            runtime.merge_power_graph(&mut ecs.0, grid, entity);
        }
    }

    /// `TeamData.destroyToDerelict` (`Teams.java:331-362`): a captured team's
    /// cores die, its other buildings convert to derelict (team 0) except a
    /// 20% `rng` roll that is destroyed, its units die and its plans clear.
    pub fn destroy_team_to_derelict(&mut self, team: u8, rng: &mut crate::determinism::SimRng) {
        let team_id = crate::ecs::TeamId(team);
        // Plans clear first (`TeamData.destroyToDerelict` `plans.clear()`).
        if let Some(mut campaign) = self.ecs.0.get_resource_mut::<CampaignRuntime>() {
            campaign.session.teams.get(team_id).plans.clear();
        }
        for entity in self.live_buildings_by_seq() {
            let entity_team = self
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|comp| comp.team)
                .unwrap_or(0);
            if entity_team != team {
                continue;
            }
            let privileged = self
                .ecs
                .0
                .get::<crate::entities::comp::Building>(entity)
                .and_then(|building| {
                    self.ecs
                        .0
                        .get_resource::<BlockTable>()
                        .and_then(|table| table.instance(building.block))
                        .map(|inst| inst.def.privileged)
                })
                .unwrap_or(false);
            if privileged {
                continue;
            }
            if self
                .ecs
                .0
                .get::<crate::world::blocks::storage::core_block::CoreBuild>(entity)
                .is_some()
            {
                self.kill_building(entity);
                continue;
            }
            if rng.chance(crate::determinism::RngStream::Sim, 0.2) {
                self.kill_building(entity);
            } else {
                self.set_building_team(entity, 0);
            }
        }
        // Units of the team die (Java runs the kill after a random delay).
        let doomed_units: Vec<Entity> = self
            .ecs
            .0
            .iter_entities()
            .filter(|entity_ref| {
                entity_ref
                    .get::<crate::entities::comp::unit::comp::UnitTypeComp>()
                    .is_some()
                    && entity_ref
                        .get::<crate::entities::comp::TeamComp>()
                        .map(|comp| comp.team)
                        == Some(team)
            })
            .map(|entity_ref| entity_ref.id())
            .collect();
        for entity in doomed_units {
            self.kill_building(entity);
        }
    }

    /// `TeamData.timeDestroy` (`Teams.java:365-397`): every non-privileged
    /// building of `team` within `range` of `(x, y)` that is not within
    /// `range` of another remaining core converts to derelict (20% die).
    pub fn time_destroy(
        &mut self,
        team: u8,
        x: f32,
        y: f32,
        range: f32,
        rng: &mut crate::determinism::SimRng,
    ) {
        let cores = self.team_core_positions(team);
        for entity in self.live_buildings_by_seq() {
            let entity_team = self
                .ecs
                .0
                .get::<crate::entities::comp::TeamComp>(entity)
                .map(|comp| comp.team)
                .unwrap_or(0);
            if entity_team != team {
                continue;
            }
            let Some(building) = self.ecs.0.get::<crate::entities::comp::Building>(entity) else {
                continue;
            };
            let privileged = self
                .ecs
                .0
                .get_resource::<BlockTable>()
                .and_then(|table| table.instance(building.block))
                .map(|inst| inst.def.privileged)
                .unwrap_or(false);
            if privileged {
                continue;
            }
            let (px, py) = crate::world::BuildHarness::tile_center(
                i32::from(building.tile.x()),
                i32::from(building.tile.y()),
            );
            // `build.within(x, y, range)`: circle test in world pixels.
            if (px - x).powi(2) + (py - y).powi(2) > range * range {
                continue;
            }
            // `!cores.contains(c -> c.within(build, range))`.
            let near_core = cores
                .iter()
                .any(|(cx, cy)| (px - cx).powi(2) + (py - cy).powi(2) <= range * range);
            if near_core {
                continue;
            }
            if rng.chance(crate::determinism::RngStream::Sim, 0.2) {
                self.kill_building(entity);
            } else {
                self.set_building_team(entity, 0);
            }
        }
    }

    /// World-pixel centers of every live core of `team`.
    fn team_core_positions(&self, team: u8) -> Vec<(f32, f32)> {
        let mut cores = Vec::new();
        for entity_ref in self.ecs.0.iter_entities() {
            if entity_ref
                .get::<crate::world::blocks::storage::core_block::CoreBuild>()
                .is_none()
            {
                continue;
            }
            if entity_ref
                .get::<crate::entities::comp::TeamComp>()
                .map(|comp| comp.team)
                != Some(team)
            {
                continue;
            }
            let Some(building) = entity_ref.get::<crate::entities::comp::Building>() else {
                continue;
            };
            cores.push(crate::world::BuildHarness::tile_center(
                i32::from(building.tile.x()),
                i32::from(building.tile.y()),
            ));
        }
        cores
    }

    /// Live plan-07 buildings in `(EntitySeq, entity.index())` order.
    fn live_buildings_by_seq(&self) -> Vec<Entity> {
        let mut buildings: Vec<(u64, Entity)> = self
            .ecs
            .0
            .iter_entities()
            .filter_map(|entity_ref| {
                entity_ref.get::<crate::entities::comp::Building>()?;
                let seq = entity_ref
                    .get::<crate::ecs::EntitySeq>()
                    .map(|seq| seq.0)
                    .unwrap_or(u64::MAX);
                Some((seq, entity_ref.id()))
            })
            .collect();
        buildings.sort_by_key(|(seq, entity)| (*seq, entity.index()));
        buildings.into_iter().map(|(_, entity)| entity).collect()
    }

    /// Marks a building or unit dead; the next `resolve_deaths` pass destroys
    /// it (Java `Building.kill()`/`Unit.kill()`).
    fn kill_building(&mut self, entity: Entity) {
        if let Some(mut health) = self.ecs.0.get_mut::<crate::entities::comp::Health>(entity) {
            health.dead = true;
            health.health = 0.0;
        }
    }

    /// Drains `CampaignRuntime` world-request queues that cannot mutate the
    /// world from inside a system: the capture → derelict conversion (T7) and
    /// `coreDestroyClear` `timeDestroy` posts.
    pub(crate) fn apply_campaign_world_requests(&mut self) {
        let (derelict_team, time_destroys, abandon_kill) = {
            let Some(mut runtime) = self.ecs.0.get_resource_mut::<CampaignRuntime>() else {
                return;
            };
            // `abandonSectorConfirm`'s `Time.runTask(7f)` (PlanetDialog.java:
            // 1156-1161): count down, then kill `player.team().cores()`.
            let abandon_kill = match runtime.pending_abandon_kill {
                Some(ticks) if ticks <= 1 => {
                    runtime.pending_abandon_kill = None;
                    let rules = runtime.session.rules.clone();
                    runtime.session.teams.player_cores(&rules).to_vec()
                }
                Some(ticks) => {
                    runtime.pending_abandon_kill = Some(ticks - 1);
                    Vec::new()
                }
                None => Vec::new(),
            };
            (
                runtime.pending_derelict_team.take(),
                std::mem::take(&mut runtime.pending_time_destroy),
                abandon_kill,
            )
        };
        // `core.kill()`: the next death pass destroys them and the campaign
        // `checkGameState` sees no player core.
        for core in abandon_kill {
            self.kill_building(core);
        }
        if derelict_team.is_none() && time_destroys.is_empty() {
            return;
        }
        // Pass the sim's stream through a swap: the methods borrow `self`.
        let mut rng = std::mem::replace(&mut self.rng_streams, crate::determinism::SimRng::new(0));
        if let Some(team) = derelict_team {
            self.destroy_team_to_derelict(team, &mut rng);
        }
        for (team, x, y, range) in time_destroys {
            self.time_destroy(team, x, y, range, &mut rng);
        }
        self.rng_streams = rng;
    }

    /// Drains `CampaignRuntime.rules_world_dirty` into
    /// [`Self::apply_rules_to_world`] (T5: G06 `setrule`, relay `setRules`,
    /// universe invasion of the played sector).
    pub(crate) fn apply_dirty_rules_to_world(&mut self) {
        let rules = {
            let Some(mut runtime) = self.ecs.0.get_resource_mut::<CampaignRuntime>() else {
                return;
            };
            if !runtime.rules_world_dirty {
                return;
            }
            runtime.rules_world_dirty = false;
            runtime.session.rules.clone()
        };
        self.apply_rules_to_world(&rules);
    }

    /// Materializes the ECS buildings for a loaded grid (map/save reader's
    /// decoded payload queue). No-op without the live block runtime; returns the
    /// number of entities spawned.
    pub fn materialize_map_buildings(&mut self, pending: &[(usize, DecodedBase)]) -> usize {
        let Sim {
            grid,
            ecs,
            block_runtime,
            ..
        } = self;
        let Some(runtime) = block_runtime.as_mut() else {
            return 0;
        };
        let spawned = runtime.materialize_buildings(&mut ecs.0, grid, pending);
        // Keep the unit runtime's terrain/path snapshot in sync with the newly
        // linked map buildings.
        self.refresh_unit_runtime();
        spawned
    }

    /// `Control.playSector`'s lost-sector half: `SectorDamage.apply(1f)` over the
    /// loaded (lost) world destroying the marked buildings, then the surviving
    /// default-team buildings and derelicts as the replay payload
    /// (`Control.java:482-537`).
    pub fn capture_lost_sector(
        &mut self,
        default_team: u8,
        wave_team: u8,
    ) -> (Vec<ReplayBuilding>, Vec<ReplayBuilding>) {
        let Sim {
            grid,
            ecs,
            block_runtime,
            ..
        } = self;
        let Some(runtime) = block_runtime.as_mut() else {
            return (Vec::new(), Vec::new());
        };

        // `SectorDamage.apply` state: spawn overlays + every live building.
        let mut spawns: Vec<(i32, i32)> = Vec::new();
        for tile in grid.tiles.array() {
            if runtime
                .content
                .block(tile.overlay)
                .is_some_and(|def| def.kind == crate::content::BlockKind::SpawnBlock)
            {
                spawns.push((i32::from(tile.x), i32::from(tile.y)));
            }
        }
        let mut buildings: Vec<crate::maps::sector_damage::DamageBuilding> = Vec::new();
        let mut owners: Vec<Entity> = Vec::new();
        for (_seq, entity, comp) in ecs.entities_by_seq() {
            let Some(def) = runtime.content.block(comp.block) else {
                continue;
            };
            let health = ecs
                .0
                .get::<crate::entities::comp::health::Health>(entity)
                .map(|entry| entry.health)
                .unwrap_or(def.health as f32);
            buildings.push(crate::maps::sector_damage::DamageBuilding::new(
                i32::from(comp.pos.x()),
                i32::from(comp.pos.y()),
                comp.team.0,
                health,
                def.size,
                def.kind == crate::content::BlockKind::CoreBlock,
            ));
            owners.push(entity);
        }
        let mut state = crate::maps::sector_damage::SectorDamageState {
            default_team,
            wave_team,
            spawns,
            buildings,
        };
        let mut rng = crate::determinism::SimRng::new(u64::from(default_team).wrapping_add(1));
        crate::maps::sector_damage::apply(
            &grid.tiles,
            &runtime.content,
            &mut state,
            1.0,
            &mut rng,
            &crate::maps::sector_damage::NoopDamageFx,
        );
        // Remove the buildings the damage marked (`Control.java:482`'s pass).
        let mut log = crate::world::ops::WorldEventLog::default();
        for (index, building) in state.buildings.iter().enumerate() {
            if !building.removed {
                continue;
            }
            let Some(entity) = owners.get(index).copied() else {
                continue;
            };
            if ecs.0.get_entity(entity).is_err() {
                continue;
            }
            let mut ctx = crate::world::ops::WorldCtx {
                grid: &mut *grid,
                content: &runtime.content,
                ecs: &mut ecs.0,
                hooks: &runtime.hooks,
                render: &runtime.render,
                log: &mut log,
            };
            ctx.clear_building(entity);
        }

        // Survivor payload: default-team buildings (cores excluded) + derelicts.
        let mut survivors = Vec::new();
        let mut derelicts = Vec::new();
        for (_seq, entity, comp) in ecs.entities_by_seq() {
            let Some(def) = runtime.content.block(comp.block) else {
                continue;
            };
            if def.kind == crate::content::BlockKind::CoreBlock {
                continue;
            }
            let health = ecs
                .0
                .get::<crate::entities::comp::health::Health>(entity)
                .map(|entry| entry.health)
                .unwrap_or(def.health as f32);
            let entry = ReplayBuilding {
                x: comp.pos.x(),
                y: comp.pos.y(),
                block: comp.block,
                rot: comp.rot,
                health,
            };
            if comp.team.0 == default_team {
                survivors.push(entry);
            } else if comp.team.0 == crate::game::team::DERELICT.0 {
                derelicts.push(entry);
            }
        }
        (survivors, derelicts)
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::ecs::BuildingComp;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::sync_session_with_sim;

    fn registry() -> ContentRegistry {
        create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
            .expect("content boot")
    }

    /// T4: the reconstructor's command panel submits `("content", "<command>")`;
    /// `resolve_config` must lower the name to the typed `UnitCommand` content
    /// value (`Reconstructor.config(UnitCommand.class, ...)`).
    #[test]
    fn content_config_resolves_unit_commands() {
        let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        let value = sim
            .block_runtime()
            .expect("runtime")
            .resolve_config(&crate::determinism::ConfigValue::Content(
                "rebuild".to_owned(),
            ))
            .expect("resolve");
        match value {
            crate::world::config::ConfigValue::Content(
                crate::content::ContentType::UnitCommand,
                id,
            ) => assert_eq!(
                sim.block_runtime()
                    .expect("runtime")
                    .content()
                    .get_by_name(crate::content::ContentType::UnitCommand, "rebuild")
                    .map(|reference| reference.id),
                Some(id)
            ),
            other => panic!("expected a unit command, got {other:?}"),
        }
    }

    /// `CoreBlock.handleItem` bumps `stats.coreItemCount` for default-team
    /// intakes (belt/dump transfers), while `handleStack` (the player offload
    /// path) does not.
    #[test]
    fn core_item_count_counts_transfers_not_the_player_offload() {
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;
        use crate::world::blocks::distribution::transfer::{
            dispatch_handle_item, dispatch_handle_stack,
        };

        let content = registry();
        let core = content.block_id("core-shard").expect("core-shard");
        let copper = content.item_id("copper").expect("copper");
        let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        sim.ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
            .expect("build rules")
            .default_team = 1;
        let pos = TilePos::new(5, 6);
        assert!(
            sim.apply(crate::command::Command::Place {
                x: 5,
                y: 6,
                block: core,
            })
            .is_ok()
        );
        let core_entity = sim.grid.entity_at(pos).expect("core entity");
        let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
        sim.ecs.0.insert_resource(CampaignRuntime::new(
            PlaySession::new(Rules {
                default_team: 1,
                ..Rules::default()
            }),
            campaign,
        ));

        dispatch_handle_item(&mut sim.ecs.0, core_entity, core_entity, copper);
        let after_item = sim.ecs.0.get_resource::<CampaignRuntime>().map(|runtime| {
            runtime
                .session
                .stats
                .core_item_count
                .get("copper")
                .copied()
                .unwrap_or(0)
        });
        assert_eq!(after_item, Some(1), "belt/dump intake counts");
        dispatch_handle_stack(&mut sim.ecs.0, core_entity, copper, 2);
        let after_stack = sim.ecs.0.get_resource::<CampaignRuntime>().map(|runtime| {
            runtime
                .session
                .stats
                .core_item_count
                .get("copper")
                .copied()
                .unwrap_or(0)
        });
        assert_eq!(after_stack, Some(1), "offload stack path does not count");
    }

    /// A loaded map carries tile blocks with no ECS entities; materializing the
    /// reader's decoded payload queue must link a live building and make the
    /// session sync see the map's default-team core (EV-0037).
    #[test]
    fn materialize_links_loaded_map_buildings() {
        let content = registry();
        let core = content.block_id("core-shard").expect("core-shard");
        let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        let pos = TilePos::new(5, 6);
        // `MapIO`/`Context` writes the block without the entity.
        sim.grid.set_block(pos, core, 1, 0).expect("tile block");
        assert_eq!(sim.group_counts().get("build"), Some(&0));

        let pending = [(
            sim.grid.index(pos).expect("index"),
            DecodedBase {
                team: 1,
                rotation: 0,
                health: 250.0,
                ..DecodedBase::default()
            },
        )];
        assert_eq!(sim.materialize_map_buildings(&pending), 1);
        assert_eq!(sim.group_counts().get("build"), Some(&1));
        let entity = sim.grid.entity_at(pos).expect("entity linked");
        let comp = sim.ecs.0.get::<BuildingComp>(entity).expect("BuildingComp");
        assert_eq!(comp.team.0, 1);
        assert_eq!(comp.block, core);
        assert_eq!(comp.pos, pos);

        let mut session = PlaySession::new(Rules {
            waves: true,
            default_team: 1,
            wave_team: 2,
            ..Rules::default()
        });
        let sync = sync_session_with_sim(&mut session, &mut sim.ecs.0, &content, 0, false);
        assert_eq!(sync.player_cores, 1);
        assert_eq!(session.player_core_count(), 1);
        // Placements carry `session.default_team` (`Team.sharded`), not the
        // `BuildRules` default (derelict).
        assert_eq!(
            sim.ecs
                .0
                .get_resource::<BuildRules>()
                .map(|rules| rules.default_team),
            Some(1),
            "build rules follow the session default team"
        );
    }

    /// A materialized drill must get its grid-derived `dominantItem` back, or a
    /// loaded save's drill stays inert (`Drill.countOre`; EV-0065).
    #[test]
    fn materialize_refreshes_drill_ore() {
        use crate::entities::comp::building::DrillState;
        use crate::world::TilePos;

        let content = registry();
        let drill = content.block_id("mechanical-drill").expect("drill");
        let ore = content.block_id("ore-copper").expect("ore-copper");
        let copper = content.item_id("copper").expect("copper");
        let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        let pos = TilePos::new(5, 5);
        // The reader writes the block and tile data without the ECS entity.
        sim.grid.set_block(pos, drill, 1, 0).expect("tile block");
        let index = sim.grid.index(pos).expect("index");
        sim.grid.tiles.geti_mut(index).overlay = ore;
        let extracted = sim.materialize_map_buildings(&[(
            index,
            DecodedBase {
                team: 1,
                rotation: 0,
                health: 80.0,
                ..DecodedBase::default()
            },
        )]);
        assert_eq!(extracted, 1);
        let entity = sim.grid.entity_at(pos).expect("entity linked");
        let state = sim.ecs.0.get::<DrillState>(entity).expect("DrillState");
        assert_eq!(state.dominant_item, Some(copper));
        assert_eq!(state.dominant_items, 1, "one ore tile under the 2x2 drill");
    }

    /// The evaluator's EV-0047 repro at the core level: load the vendored
    /// Ground Zero map, materialize its buildings, place a mechanical drill on
    /// map ore (stored in `tile.overlay`) beside a core and tick. The drill
    /// must deliver copper into the core's item module. Skips without the
    /// vendored asset (`msav-import` feature only).
    #[cfg(feature = "msav-import")]
    #[test]
    fn ground_zero_drill_feeds_the_materialized_core() {
        use crate::editor::context::EditorContext;
        use crate::io::save::{SaveIo, SaveReadState};
        use crate::world::modules::ItemModule;
        use std::path::Path;

        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../assets/maps/serpulo/groundZero.msav");
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let mut content = registry();
        let mut grid = WorldGrid::new(0, 0);
        grid.begin_map_load();
        let (result, pending) = {
            let mut context = EditorContext::new(&mut grid, &content);
            let (result, pending) = {
                let mut state = SaveReadState {
                    context: Some(&mut context),
                    content: Some(&mut content),
                    ..SaveReadState::default()
                };
                let result = SaveIo::load_bytes(&bytes, &mut state);
                (result, std::mem::take(&mut context.pending_buildings))
            };
            (result, pending)
        };
        result.expect("map imports");
        grid.end_map_load(&content);

        let ore = content.block_id("ore-copper").expect("ore-copper");
        let core_block = content.block_id("core-shard").expect("core");
        let drill = content.block_id("mechanical-drill").expect("drill");
        let width = grid.tiles.width;
        let height = grid.tiles.height;
        let air = BlockId::AIR;
        // 2x2 ore footprint with a clear 6x4 rectangle around it for the core.
        let mut spot = None;
        'outer: for y in 2..(height - 5) {
            for x in 2..(width - 7) {
                let mut ok = true;
                for dx in 0..6 {
                    for dy in 0..4 {
                        let in_ore = (0..2).contains(&dx) && (0..2).contains(&dy);
                        let t = grid.tile(x + dx, y + dy);
                        if t.block != air {
                            ok = false;
                        }
                        if in_ore && t.overlay != ore {
                            ok = false;
                        }
                        if !in_ore && t.overlay != air && dx > 1 {
                            ok = false;
                        }
                    }
                }
                if ok {
                    spot = Some((x, y + 1));
                    break 'outer;
                }
            }
        }
        let Some((x, y)) = spot else {
            panic!("no ore patch with clear core space");
        };

        let mut sim = Sim::new(1, width, height, air, air);
        sim.grid = grid;
        sim.install_block_runtime().expect("runtime");
        assert!(sim.materialize_map_buildings(&pending) > 0);
        // Team 1: stage the core through the API form (`player: None`) so the
        // player core-placement gate (`CoreBlock.canPlaceOn`, H02 T4) does not
        // reject the fixture; a bare `Command::Place` would carry team 0
        // (derelict) and derelict buildings no longer update (H09 T4).
        sim.command(crate::determinism::SimCommand::Place {
            x: (x + 3) as i16,
            y: y as i16,
            block: core_block.raw(),
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("core command");
        sim.place_for_team(x as i16, y as i16, drill, 0, 1);
        let core_entity = sim
            .grid
            .entity_at(TilePos::new((x + 3) as i16, y as i16))
            .expect("core entity");
        let drill_entity = sim
            .grid
            .entity_at(TilePos::new(x as i16, y as i16))
            .expect("drill entity");
        let copper = content.item_id("copper").expect("copper");
        for _ in 0..300 {
            sim.tick().expect("tick");
        }
        let stored = sim
            .ecs
            .0
            .get::<ItemModule>(core_entity)
            .map(|m| m.get(copper))
            .unwrap_or(0);
        assert!(
            stored >= 1,
            "core received no copper from the map-ore drill; drill entity {drill_entity:?}"
        );
    }

    /// Without the live runtime the payload queue is inert (P0 spine unchanged).
    #[test]
    fn materialize_is_a_noop_without_the_runtime() {
        let mut sim = Sim::new(1, 8, 8, BlockId::AIR, BlockId::AIR);
        assert_eq!(
            sim.materialize_map_buildings(&[(0, DecodedBase::default())]),
            0
        );
    }
}
