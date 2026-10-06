// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Building behavior dispatch (`BuildingComp` method surface).
//!
//! Ported from `core/src/mindustry/entities/comp/BuildingComp.java`'s virtual
//! methods and the per-class `update`/`acceptItem` overrides. Plan 07 deviation
//! §2.4.1: Java reflection over inner `Building` classes is replaced by one
//! registered [`BuildingBehavior`] per [`BlockId`] plus a [`BuildingKind`] tag.
//! Plans 08/09/10/11/20 register their families through [`BehaviorRegistry`].

use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use indexmap::IndexMap;

use crate::content::{BlockDef, BlockId, ItemId, LiquidId};
use crate::io::entity::{EntityReader, EntityWriter};

use super::block::BlockInstance;
use super::config::ConfigValue;
use super::modules::PowerGraphId;
use super::stats::Stats;

pub mod campaign;
pub mod defense;
pub mod environment;
pub mod helpers;
pub mod legacy;
pub mod production;
pub mod sandbox;

/// Plan-07 alias for plan 04's entity writer (`BuildingWriter`).
pub type BuildingWriter<'a> = EntityWriter<'a>;
/// Plan-07 alias for plan 04's entity reader (`BuildingReader`).
pub type BuildingReader<'a> = EntityReader<'a>;

/// Reference to a payload held by a building (`Payload` handle; plan 08 owns the
/// payload system, this is the opaque cross-plan hand-off type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PayloadRef {
    /// Entity carrying the payload, if any.
    pub entity: Option<Entity>,
    /// Payload content id (block or unit), if any.
    pub content: u16,
    /// Whether the payload is a block.
    pub is_block: bool,
}

/// Grouping/IO tag for a building family (`Building` inner-class equivalent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct BuildingKind(pub u16);

impl BuildingKind {
    /// Generic `BuildingComp`.
    pub const GENERIC: BuildingKind = BuildingKind(0);
    /// `ConstructBuild`.
    pub const CONSTRUCT: BuildingKind = BuildingKind(1);
    /// `GenericCrafterBuild`.
    pub const CRAFTER: BuildingKind = BuildingKind(2);
    /// `DrillBuild`.
    pub const DRILL: BuildingKind = BuildingKind(3);
    /// `SeparatorBuild`.
    pub const SEPARATOR: BuildingKind = BuildingKind(4);
    /// `LiquidSourceBuild`.
    pub const LIQUID_SOURCE: BuildingKind = BuildingKind(5);
    /// `AcceleratorBuild`.
    pub const ACCELERATOR: BuildingKind = BuildingKind(6);
    /// `LaunchPadBuild`.
    pub const LAUNCH_PAD: BuildingKind = BuildingKind(7);
    /// `WallBuild`.
    pub const WALL: BuildingKind = BuildingKind(8);
    /// `DoorBuild`.
    pub const DOOR: BuildingKind = BuildingKind(9);
    /// `LogicBuild` (plan 13; revision 5).
    pub const LOGIC: BuildingKind = BuildingKind(10);
    /// `MemoryBuild` (plan 13; revision 1).
    pub const MEMORY: BuildingKind = BuildingKind(11);
    /// `MessageBuild` (plan 13; revision 0).
    pub const MESSAGE: BuildingKind = BuildingKind(12);
    /// `SwitchBuild` (plan 13; revision 1).
    pub const SWITCH: BuildingKind = BuildingKind(13);
    /// `LogicDisplayBuild` (plan 13; revision 1).
    pub const DISPLAY: BuildingKind = BuildingKind(14);
    /// `TileableLogicDisplayBuild` (plan 13; revision 0).
    pub const TILEABLE_DISPLAY: BuildingKind = BuildingKind(15);
    /// `CanvasBuild` (plan 13; revision 0).
    pub const CANVAS: BuildingKind = BuildingKind(16);

    /// IO revision (`ConstructBuild=1`, `DrillBuild=1`, …; default 0).
    pub const fn revision(self) -> u8 {
        match self.0 {
            // ConstructBuild=1, DrillBuild=1, SeparatorBuild=1,
            // LiquidSourceBuild=1, AcceleratorBuild=1, LaunchPadBuild=1.
            1 | 3 | 4 | 5 | 6 | 7 => 1,
            // LogicBuild revision 5 (plan 13 §6.5).
            10 => 5,
            // MemoryBuild / SwitchBuild / LogicDisplayBuild revision 1.
            11 | 13 | 14 => 1,
            _ => 0,
        }
    }

    /// Parity kind name (revision manifests).
    pub const fn name(self) -> &'static str {
        match self.0 {
            0 => "GenericCrafterBuild",
            1 => "ConstructBuild",
            2 => "GenericCrafterBuild",
            3 => "DrillBuild",
            4 => "SeparatorBuild",
            5 => "LiquidSourceBuild",
            6 => "AcceleratorBuild",
            7 => "LaunchPadBuild",
            8 => "WallBuild",
            9 => "DoorBuild",
            10 => "LogicBuild",
            11 => "MemoryBuild",
            12 => "MessageBuild",
            13 => "SwitchBuild",
            14 => "LogicDisplayBuild",
            15 => "TileableLogicDisplayBuild",
            16 => "CanvasBuild",
            _ => "BuildingComp",
        }
    }
}

/// Per-block behavior surface (frozen by plan 08 §3.2).
pub trait BuildingBehavior: Send + Sync {
    /// Per-tick update (`Building.updateTile`).
    fn update_tile(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// Batched per-tick update over a contiguous run of entities that share this
    /// behavior/block (plan 07 §3.4 batch dispatch; additive hook).
    ///
    /// `inst` is this behavior's resolved [`BlockInstance`]. The default
    /// delegates to the per-entity [`Self::update_tile`] via
    /// [`crate::world::update::building_update_with`] so every existing behavior
    /// keeps its exact update sequence; hot logistics families override this
    /// with [`crate::world::update::building_update_no_consumers`].
    fn update_batch(&self, world: &mut World, inst: &BlockInstance, entities: &[Entity]) {
        for &e in entities {
            crate::world::update::building_update_with(world, e, inst);
        }
    }

    /// `noUpdateDisabled` inverse (`Block.noUpdateDisabled`).
    fn always_update_when_disabled(&self) -> bool {
        false
    }

    /// Inserts family state components at spawn.
    fn create_state(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.created()`.
    fn created(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.placed()`.
    fn placed(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.dropped()` (payload drop).
    fn dropped(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.onRemoved()`.
    fn on_removed(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.onDeconstructed(builder)`.
    fn on_deconstructed(&self, world: &mut World, e: Entity, builder: Option<Entity>) {
        let _ = (world, e, builder);
    }

    /// `Building.onDestroyed()`.
    fn on_destroyed(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.afterDestroyed()`.
    fn after_destroyed(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.overwrote(previous)`.
    fn overwrote(&self, world: &mut World, e: Entity, previous: &[Entity]) {
        let _ = (world, e, previous);
    }

    /// `Building.onProximityAdded()`.
    fn on_proximity_added(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.onProximityUpdate()`.
    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.onProximityRemoved()`.
    fn on_proximity_removed(&self, world: &mut World, e: Entity) {
        let _ = (world, e);
    }

    /// `Building.config()` readback.
    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        let _ = (world, e);
        ConfigValue::None
    }

    /// `Building.configured(player, value)`.
    fn configured(&self, world: &mut World, e: Entity, player: Option<Entity>, value: ConfigValue) {
        let _ = (world, e, player, value);
    }

    /// Per-kind IO version (`Building.version()`).
    fn version(&self, world: &World, e: Entity) -> u8 {
        let _ = (world, e);
        0
    }

    /// Per-kind IO write (after `writeBase`).
    fn write(&self, world: &World, e: Entity, w: &mut BuildingWriter) {
        let _ = (world, e, w);
    }

    /// Per-kind IO read (after `readBase`).
    fn read(&self, world: &mut World, e: Entity, r: &mut BuildingReader, revision: u8) {
        let _ = (world, e, r, revision);
    }

    /// Block stats (`Block.setStats` runtime half).
    fn stats(&self) -> Stats {
        Stats::new()
    }

    /// Item acceptance hook (08 overrides).
    fn accept_item(&self, world: &World, e: Entity, src: Entity, item: ItemId) -> bool {
        let _ = (world, e, src, item);
        false
    }

    /// Item handling hook (08 overrides).
    fn handle_item(&self, world: &mut World, e: Entity, src: Entity, item: ItemId) {
        let _ = (world, e, src, item);
    }

    /// Stack acceptance hook (`BuildingComp.acceptStack`; 08 overrides).
    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        amount: i32,
        source: Option<Entity>,
    ) -> i32 {
        let _ = (world, e, item, amount, source);
        0
    }

    /// Stack handling hook (`BuildingComp.handleStack`; 08 overrides).
    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        let _ = (world, e, item, amount);
    }

    /// Stack removal hook (`BuildingComp.removeStack`; 08 overrides).
    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        let _ = (world, e, item, amount);
        0
    }

    /// Maximum accepted amount (`BuildingComp.getMaximumAccepted`; 08 overrides).
    fn get_maximum_accepted(&self, world: &World, e: Entity, item: ItemId) -> i32 {
        let _ = (world, e, item);
        0
    }

    /// Unload hook (`BuildingComp.canUnload`; 08 overrides).
    fn can_unload(&self, world: &World, e: Entity) -> bool {
        let _ = (world, e);
        false
    }

    /// Item-taken notification (`BuildingComp.itemTaken`; 08/12 overrides).
    fn item_taken(&self, world: &mut World, e: Entity, item: ItemId) {
        let _ = (world, e, item);
    }

    /// Production stat hook (`BuildingComp.produced`; 12 overrides).
    fn produced(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        let _ = (world, e, item, amount);
    }

    /// Liquid acceptance hook (08/09 override).
    fn accept_liquid(&self, world: &World, e: Entity, src: Entity, liquid: LiquidId) -> bool {
        let _ = (world, e, src, liquid);
        false
    }

    /// Liquid handling hook (08/09 override).
    fn handle_liquid(
        &self,
        world: &mut World,
        e: Entity,
        src: Entity,
        liquid: LiquidId,
        amount: f32,
    ) {
        let _ = (world, e, src, liquid, amount);
    }

    /// Liquid destination hook (08 override).
    fn get_liquid_destination(
        &self,
        world: &World,
        e: Entity,
        from: Entity,
        liquid: LiquidId,
    ) -> Option<Entity> {
        let _ = (world, e, from, liquid);
        None
    }

    /// Payload take hook (08 override).
    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        let _ = (world, e);
        None
    }

    /// Payload acceptance hook (`Building.acceptPayload`; 08 override).
    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        source: Entity,
        payload: PayloadRef,
    ) -> bool {
        let _ = (world, e, source, payload);
        false
    }

    /// Payload handling hook (`Building.handlePayload`; 08 override).
    fn handle_payload(&self, world: &mut World, e: Entity, source: Entity, payload: PayloadRef) {
        let _ = (world, e, source, payload);
    }

    /// Payload readback (`Building.getPayload`; 08 override).
    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        let _ = (world, e);
        None
    }

    /// Dump one item (`Building.dump`).
    fn dump(&self, world: &mut World, e: Entity, item: Option<ItemId>) -> bool {
        let _ = (world, e, item);
        false
    }

    /// Cumulative dump (`Building.dumpAccumulate`).
    fn dump_accumulate(&self, world: &mut World, e: Entity, item: Option<ItemId>) -> bool {
        let _ = (world, e, item);
        false
    }

    /// Offload an item (`Building.offload`).
    fn offload(&self, world: &mut World, e: Entity, item: ItemId) {
        let _ = (world, e, item);
    }

    /// Move an item forward (`Building.moveForward`).
    fn move_forward(&self, world: &mut World, e: Entity, item: ItemId) -> bool {
        let _ = (world, e, item);
        false
    }

    /// Whether an item can be dumped to `to` (`Building.canDump`).
    fn can_dump(&self, world: &World, e: Entity, to: Entity, item: ItemId) -> bool {
        let _ = (world, e, to, item);
        true
    }

    /// Efficiency scale hook.
    ///
    /// `&mut World` so heat-pull blocks (plan 09 `HeatCrafter`) can run
    /// `calculate_heat` during `updateConsumption`, exactly where upstream
    /// applies `efficiencyScale()`.
    fn efficiency_scale(&self, world: &mut World, e: Entity) -> f32 {
        let _ = (world, e);
        1.0
    }

    /// Power graph id hook (09 reads).
    fn power_graph(&self, world: &World, e: Entity) -> PowerGraphId {
        let _ = (world, e);
        PowerGraphId::NONE
    }
}

/// A no-op behavior used for families not yet implemented by a later plan.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopBehavior;

impl BuildingBehavior for NoopBehavior {}

/// Per-block behavior override registry (mods and later plans).
#[derive(Default)]
pub struct BehaviorRegistry {
    by_block: IndexMap<BlockId, Arc<dyn BuildingBehavior>>,
    by_name: IndexMap<String, Arc<dyn BuildingBehavior>>,
}

impl std::fmt::Debug for BehaviorRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BehaviorRegistry")
            .field("blocks", &self.by_block.len())
            .field("names", &self.by_name.len())
            .finish()
    }
}

impl BehaviorRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a behavior for a resolved block id.
    pub fn register(&mut self, block: BlockId, behavior: Arc<dyn BuildingBehavior>) {
        self.by_block.insert(block, behavior);
    }

    /// Registers a behavior by content name (resolved against `content`).
    pub fn register_named(&mut self, name: &str, behavior: Arc<dyn BuildingBehavior>) -> bool {
        if self.by_name.insert(name.to_owned(), behavior).is_some() {
            log::debug!("behavior for `{name}` was replaced");
        }
        true
    }

    /// Lookup by block id.
    pub fn get(&self, block: BlockId) -> Option<Arc<dyn BuildingBehavior>> {
        self.by_block.get(&block).cloned()
    }

    /// Lookup by content name.
    pub fn get_named(&self, name: &str) -> Option<Arc<dyn BuildingBehavior>> {
        self.by_name.get(name).cloned()
    }

    /// Whether a block has an override.
    pub fn contains(&self, block: BlockId) -> bool {
        self.by_block.contains_key(&block)
    }

    /// Number of registered overrides.
    pub fn len(&self) -> usize {
        self.by_block.len() + self.by_name.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.by_block.is_empty() && self.by_name.is_empty()
    }
}

/// Resolves a block's behavior, preferring a registered override.
pub fn resolve_behavior(def: &BlockDef, registry: &BehaviorRegistry) -> Arc<dyn BuildingBehavior> {
    if let Some(behavior) = registry.get(def.id) {
        return behavior;
    }
    if let Some(behavior) = registry.get_named(&def.name) {
        return behavior;
    }
    default_behavior(def)
}

/// Built-in behavior for a block with no registered override (plan 07 §3.12).
pub fn default_behavior(def: &BlockDef) -> Arc<dyn BuildingBehavior> {
    use crate::content::BlockKind as K;
    match def.kind {
        K::Door => Arc::new(defense::DoorBehavior),
        K::AutoDoor => Arc::new(defense::AutoDoorBehavior),
        K::Wall | K::ShieldWall => Arc::new(defense::WallBehavior),
        K::Radar => Arc::new(defense::RadarBehavior),
        K::Thruster => Arc::new(defense::ThrusterBehavior),
        K::TargetDummy => {
            Arc::new(crate::world::blocks::defense::behaviors::TargetDummyDefenseBehavior)
        }
        // Projector/mine/shield state machines (`world/blocks/defense`); the
        // exact per-block knobs are installed by `defense::register`.
        K::MendProjector => Arc::new(
            crate::world::blocks::defense::behaviors::MendProjectorBehavior {
                state: crate::world::blocks::defense::shields::MendProjectorState::default(),
            },
        ),
        K::OverdriveProjector => Arc::new(
            crate::world::blocks::defense::behaviors::OverdriveProjectorBehavior {
                state: crate::world::blocks::defense::projectors::OverdriveProjectorState::default(
                ),
            },
        ),
        K::ForceProjector => Arc::new(
            crate::world::blocks::defense::behaviors::ForceProjectorBehavior {
                state: crate::world::blocks::defense::shields::ForceProjectorState::default(),
            },
        ),
        K::ShockMine => Arc::new(
            crate::world::blocks::defense::behaviors::ShockMineBehavior {
                state: crate::world::blocks::defense::shields::ShockMineState::default(),
            },
        ),
        K::RegenProjector => Arc::new(
            crate::world::blocks::defense::behaviors::RegenProjectorBehavior {
                state: crate::world::blocks::defense::projectors::RegenProjectorState::default(),
            },
        ),
        K::ShockwaveTower => Arc::new(
            crate::world::blocks::defense::behaviors::ShockwaveTowerBehavior {
                state: crate::world::blocks::defense::projectors::ShockwaveTowerState::default(),
            },
        ),
        K::BaseShield => Arc::new(
            crate::world::blocks::defense::behaviors::BaseShieldBehavior {
                state: crate::world::blocks::defense::projectors::BaseShieldState::default(),
            },
        ),
        // Liquid bridges (`world/blocks/liquid/bridge.rs`); the phase-conduit
        // range (12) is installed by `liquid::register`.
        K::LiquidBridge => {
            Arc::new(crate::world::blocks::liquid::behavior::LiquidBridgeBehavior { range: 4 })
        }
        K::DirectionLiquidBridge => Arc::new(
            crate::world::blocks::liquid::behavior::DirectionLiquidBridgeBehavior { range: 4 },
        ),
        // Power nodes: construction inserts the `PowerNodeConfig` the graph
        // linker reads; exact knobs are installed by `power::register`.
        K::PowerNode | K::LongPowerNode => {
            Arc::new(crate::world::blocks::power::nodes::PowerNodeBehavior {
                max_nodes: 3,
                laser_range: 6.0,
                autolink: true,
                same_block_connection: false,
            })
        }
        // Turret ammo configs need the content bullet fixtures; they are
        // installed by `defense::register` when the fixture map is present.
        K::ItemTurret
        | K::LiquidTurret
        | K::PowerTurret
        | K::ContinuousTurret
        | K::ContinuousLiquidTurret
        | K::LaserTurret
        | K::PointDefenseTurret
        | K::TractorBeamTurret
        | K::BuildTurret => Arc::new(NoopBehavior),
        K::GenericCrafter
        | K::HeatCrafter
        | K::AttributeCrafter
        | K::Separator
        | K::ItemIncinerator => Arc::new(production::CrafterBehavior),
        K::Drill | K::BurstDrill | K::WallCrafter => Arc::new(production::DrillBehavior),
        K::BeamDrill => Arc::new(production::BeamDrillBehavior),
        K::Pump => Arc::new(production::PumpBehavior),
        K::SolidPump | K::Fracker => Arc::new(production::SolidPumpBehavior),
        K::Incinerator => Arc::new(production::IncineratorBehavior),
        K::Accelerator => Arc::new(campaign::AcceleratorBehavior),
        K::LandingPad => Arc::new(campaign::LandingPadBehavior),
        K::LaunchPad => Arc::new(campaign::LaunchPadBehavior),
        K::LegacyMechPad | K::LegacyUnitFactory | K::LegacyCommandCenter => {
            Arc::new(legacy::LegacyBehavior)
        }
        // Plan 13 logic blocks (M3/M4).
        K::LogicBlock => Arc::new(crate::logic::blocks::LogicBlockBehavior),
        K::MemoryBlock => Arc::new(crate::logic::blocks::MemoryBehavior),
        K::SwitchBlock => Arc::new(crate::logic::blocks::SwitchBehavior),
        K::MessageBlock => Arc::new(crate::logic::blocks::MessageBehavior),
        K::LogicDisplay | K::TileableLogicDisplay => {
            Arc::new(crate::logic::blocks::DisplayBehavior)
        }
        K::CanvasBlock => Arc::new(crate::logic::blocks::CanvasBehavior),
        _ => match super::block_kind_data::BlockKindData::from_def(def).family() {
            super::block_kind_data::BlockFamily::Sandbox => Arc::new(sandbox::SandboxBehavior),
            super::block_kind_data::BlockFamily::Environment => {
                Arc::new(environment::EnvironmentBehavior)
            }
            _ => Arc::new(NoopBehavior),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_revisions_match_plan() {
        assert_eq!(BuildingKind::CONSTRUCT.revision(), 1);
        assert_eq!(BuildingKind::DRILL.revision(), 1);
        assert_eq!(BuildingKind::SEPARATOR.revision(), 1);
        assert_eq!(BuildingKind::CRAFTER.revision(), 0);
        assert_eq!(BuildingKind::GENERIC.revision(), 0);
    }

    #[test]
    fn registry_overrides_by_name() {
        let mut registry = BehaviorRegistry::new();
        registry.register_named("copper-wall", Arc::new(NoopBehavior));
        assert!(registry.get_named("copper-wall").is_some());
        assert!(!registry.is_empty());
    }

    /// Ported `ApplicationTests.allBlockTest` (update half): every block in this
    /// plan's owned families spawns, updates once, and reports its own
    /// block/health. Families owned by 08/09/10/11 are excluded.
    #[test]
    fn all_blocks_update_without_panic() {
        use crate::content::test_support::test_registry;
        use crate::entities::comp::{Building, Health};
        use crate::world::TilePos;
        use crate::world::block::BlockTable;
        use crate::world::limits::BuildRules;
        use crate::world::update::update_buildings;
        use bevy_ecs::world::World;

        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        let candidates: Vec<_> = table
            .iter()
            .filter(|inst| {
                matches!(
                    inst.kind_data.family(),
                    super::super::block_kind_data::BlockFamily::Defense
                        | super::super::block_kind_data::BlockFamily::Production
                        | super::super::block_kind_data::BlockFamily::Sandbox
                        | super::super::block_kind_data::BlockFamily::Campaign
                )
            })
            .cloned()
            .collect();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let mut spawned = Vec::new();
        for inst in &candidates {
            let entity = inst.spawn(
                &mut world,
                0,
                TilePos::new(2, 2),
                0,
                0,
                content.items().len(),
                content.liquids().len(),
            );
            spawned.push((inst.clone(), entity));
        }
        update_buildings(&mut world);
        for (inst, entity) in &spawned {
            let building = world.get::<Building>(*entity).expect("building");
            assert_eq!(
                building.block, inst.def.id,
                "block mismatch for {}",
                inst.name
            );
            let health = world.get::<Health>(*entity).expect("health");
            assert_eq!(
                health.health, inst.def.health as f32,
                "health for {}",
                inst.name
            );
        }
        assert!(!spawned.is_empty(), "no owned-family blocks checked");
    }
}
