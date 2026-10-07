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
    BlockId, BlockKind, BuildVisibility, ContentError, ContentRegistry, ContentType, ItemId,
    LiquidId, MemoryBundle, MemoryUnlockStore, UnitTypeId, create_base_content,
};
use crate::determinism::{Checksum, Checksummer, CommandError};
use crate::ecs::{BuildingComp, TeamId};
use crate::world::block::{BlockError, BlockTable};
use crate::world::blocks::power::{
    PowerGrids, PowerNodeConfig, power_graph_removed, update_power_graph,
};
use crate::world::building_io::{self, DecodedBase};
use crate::world::config::ConfigValue as WorldConfigValue;
use crate::world::limits::{BlockCounter, BuildRules};
use crate::world::modules::{ItemModule, LiquidModule, ModuleDims, PowerModule};
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
        // K-3: `PowerNodeConfig` is never inserted by the block behaviors; the
        // live placement path is the natural insertion point (`PowerNode`).
        if matches!(inst.def.kind, BlockKind::PowerNode) {
            world.entity_mut(entity).insert(PowerNodeConfig::default());
        }
        Some(entity)
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
    counter: BlockCounter,
    hooks: SimHooks,
    log: WorldEventLog,
    render: NoopRenderHooks,
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

    /// Live block counter.
    pub fn counter(&self) -> &BlockCounter {
        &self.counter
    }

    /// Clears runtime bookkeeping after [`Sim::reset`] (entities are gone).
    pub(crate) fn reset(&mut self) {
        self.counter = BlockCounter::new();
    }

    /// Whether placement is allowed by the `BuildVisibility`/unlock gate.
    ///
    /// `Block.canBeBuilt()` excludes hidden/debug-only; the campaign gate also
    /// excludes sandbox/editor-only content and locked content, matching the
    /// task's placement rules (K-9). Outside a campaign `unlockedNow()` is
    /// always true, so only the visibility half applies.
    fn placement_allowed(&self, block: BlockId) -> bool {
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
    ) -> bool {
        if !self.placement_allowed(block) {
            return false;
        }
        let rules = world
            .get_resource::<BuildRules>()
            .cloned()
            .unwrap_or_default();
        {
            let Some(table) = world.get_resource::<BlockTable>() else {
                return false;
            };
            if !crate::world::build::valid_place(
                &self.content,
                table,
                &rules,
                &self.counter,
                grid,
                block,
                team,
                rot,
                i32::from(x),
                i32::from(y),
            ) {
                return false;
            }
        }

        let pos = TilePos::new(x, y);
        if let Some(old) = grid.entity_at(pos) {
            let old_block = grid.block_at(pos).unwrap_or(BlockId::AIR);
            let old_team = world
                .get::<crate::entities::comp::TeamComp>(old)
                .map(|team| team.team)
                .unwrap_or(team);
            let _ = remove_from_proximity(world, old, &self.content);
            self.remove_power_graph(world, grid, old);
            if old_block != BlockId::AIR {
                self.counter.remove(old_team, old_block, 1);
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
        }
        self.counter.add(team, block, 1);
        true
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
            &rules,
            team,
            i32::from(x),
            i32::from(y),
        ) {
            return None;
        }

        let _ = remove_from_proximity(world, entity, &self.content);
        self.remove_power_graph(world, grid, entity);
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
        self.counter.remove(team, block, 1);
        // `Tile.changed` re-runs proximity for the four neighbors.
        for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if let Some(entity) = grid.entity_at(TilePos::new(nx, ny)) {
                let _ = update_proximity(world, grid, &self.content, entity);
            }
        }
        Some(block)
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
            spawned.push(entity);
        }
        grid.generating = generating;
        for entity in &spawned {
            let _ = update_proximity(ecs, grid, &self.content, *entity);
        }
        for entity in &spawned {
            self.merge_power_graph(ecs, grid, *entity);
        }
        spawned.len()
    }

    /// Recomputes proximity for the tile at `(x, y)` and its four neighbors.
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
        c.part(&(self.counter.counts.len() as u64));
        c.finish()
    }
}

/// Boots the vanilla content registry the way the harness does.
fn load_vanilla_content() -> Result<ContentRegistry, RuntimeError> {
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)?;
    content.init()?;
    content.post_init()?;
    Ok(content)
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
        self.ecs.0.insert_resource(table);
        self.ecs.0.init_resource::<BuildRules>();
        self.ecs.0.init_resource::<BuildClock>();
        self.ecs.0.init_resource::<TileBuilds>();
        self.ecs.0.insert_resource(ModuleDims {
            items: item_count,
            liquids: liquid_count,
        });
        self.ecs.0.insert_resource(PowerGrids::new());
        self.block_runtime = Some(SimRuntime {
            config,
            content,
            counter: BlockCounter::new(),
            hooks: SimHooks {
                seq: AtomicU64::new(0),
                item_count,
                liquid_count,
            },
            log: WorldEventLog::default(),
            render: NoopRenderHooks,
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
        runtime.materialize_buildings(&mut ecs.0, grid, pending)
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
    }

    /// The evaluator's EV-0047 repro at the core level: load the vendored
    /// Ground Zero map, materialize its buildings, place a mechanical drill on
    /// map ore (stored in `tile.overlay`) beside a core and tick. The drill
    /// must deliver copper into the core's item module. Skips without the
    /// vendored asset (`msav-import` feature only).
    #[cfg(feature = "msav-import")]
    #[test]
    fn ground_zero_drill_feeds_the_materialized_core() {
        use crate::command::Command;
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
        sim.apply(Command::Place {
            x: (x + 3) as i16,
            y: y as i16,
            block: core_block,
        })
        .expect("core command");
        sim.apply(Command::Place {
            x: x as i16,
            y: y as i16,
            block: drill,
        })
        .expect("drill command");
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
