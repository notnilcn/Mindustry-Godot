// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Block behavior side table (`Block` runtime half).
//!
//! Ported from `core/src/mindustry/world/Block.java` (`init`, `afterPatch`,
//! `createIcons`, placement flags) and the `BlockView` read API of plan 07
//! §3.2. Plan 02 owns [`crate::content::BlockDef`] metadata; this module adds the
//! per-[`BlockId`] runtime instance (behavior, consumers, bars, draw/sound
//! descriptors) and the derived-value validation pass.

use std::sync::Arc;

use bevy_ecs::prelude::Resource;
use smallvec::SmallVec;

use crate::content::{
    BarSpec, BlockDef, BlockFlag, BlockGroup, BlockId, ContentRegistry, EnvMask, Rgba,
};

use super::behavior::{BehaviorRegistry, BuildingBehavior, BuildingKind, resolve_behavior};
use super::block_kind_data::BlockKindData;
use super::config::{ConfigHandlers, ConfigKind};
use super::consumers::Consumers;
use super::draw::DrawSpec;

/// `Block.tilesize`.
pub const TILE_SIZE: f32 = crate::content::TILE_SIZE;

/// Fields `@NoPatch` rejects in plan 20's `DataPatcher` (plan 07 §8 R11).
pub const PATCH_DENIED: &[&str] = &[
    "size",
    "consumes",
    "buildType",
    "itemFilter",
    "liquidFilter",
    "kind",
    "region",
    "draw",
    "bars",
];

/// Sound handles for a block's lifecycle (`Block.placeSound` etc.; plan 18).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockSounds {
    /// Placement sound.
    pub place: Option<String>,
    /// Break-in-progress sound (`breakSound`).
    pub break_sound: Option<String>,
    /// Destruction sound (`destroySound`).
    pub destroy: Option<String>,
    /// Ambient loop.
    pub ambient: Option<String>,
}

/// One block's runtime behavior instance (plan 07 §3.2).
pub struct BlockInstance {
    /// Content id.
    pub id: BlockId,
    /// Content name (parity ABI).
    pub name: String,
    /// Owned metadata snapshot (so behaviors never need the full registry).
    pub def: Arc<BlockDef>,
    /// Building family tag (grouping/IO/revision).
    pub building: BuildingKind,
    /// Registered behavior.
    pub behavior: Arc<dyn BuildingBehavior>,
    /// Typed family data.
    pub kind_data: BlockKindData,
    /// Lowered consumers.
    pub consumers: Consumers,
    /// Config handlers.
    pub configs: ConfigHandlers,
    /// Static bar descriptors.
    pub bars: SmallVec<[BarSpec; 4]>,
    /// Draw descriptor.
    pub draw: DrawSpec,
    /// Lifecycle sounds.
    pub sounds: BlockSounds,
    /// Icon region descriptors (03 consumes).
    pub icon_descriptors: SmallVec<[String; 4]>,
    /// Derived rotation capability (`Block.rotate`; plan-04 plan predicate).
    pub rotate: bool,
}

impl std::fmt::Debug for BlockInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockInstance")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("building", &self.building)
            .field("family", &self.kind_data.family_name())
            .field("consumers", &self.consumers.len())
            .field("rotate", &self.rotate)
            .finish()
    }
}

impl BlockInstance {
    /// Builds the runtime instance from metadata + an optional behavior override.
    pub fn from_def(def: &BlockDef, registry: &BehaviorRegistry) -> Result<Self, BlockError> {
        let kind_data = BlockKindData::from_def(def);
        let building = building_kind(&kind_data);
        let behavior = resolve_behavior(def, registry);
        let consumers = Consumers::build(&def.consumes);
        assert_derivations(def)?;
        let mut bars: SmallVec<[BarSpec; 4]> = SmallVec::new();
        if def.health > 0 {
            bars.push(BarSpec::Health);
        }
        if def.has_power {
            bars.push(BarSpec::Power);
        }
        if def.has_items {
            bars.push(BarSpec::Items);
        }
        if def.has_liquids {
            bars.push(BarSpec::Liquid);
        }
        for bar in &def.bars {
            if !bars.contains(bar) {
                bars.push(bar.clone());
            }
        }
        let sounds = default_sounds(def);
        let mut icon_descriptors = SmallVec::new();
        if !def.region.is_empty() {
            icon_descriptors.push(def.region.clone());
        }
        Ok(Self {
            id: def.id,
            name: def.name.clone(),
            def: Arc::new(def.clone()),
            building,
            behavior,
            kind_data,
            consumers,
            configs: ConfigHandlers::default(),
            bars,
            draw: DrawSpec::Default,
            sounds,
            icon_descriptors,
            rotate: kind_rotates(def.kind),
        })
    }

    /// Whether the block has a building entity (`Block.hasBuilding`).
    pub fn has_building(&self) -> bool {
        !matches!(
            self.kind_data.family(),
            super::block_kind_data::BlockFamily::Environment
        )
    }

    /// Config value kinds the block's behavior accepts
    /// (`Block.configurations` keys; `Block.config(...)`).
    pub fn config_kinds(&self) -> &'static [ConfigKind] {
        self.behavior.config_kinds()
    }

    /// Consumer count.
    pub fn consumer_count(&self) -> usize {
        self.consumers.len()
    }
}

/// Errors raised while building the block table.
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
pub enum BlockError {
    /// A derived field did not match plan 02's value.
    #[error("block `{name}` derivation mismatch: {field} expected {expected}, found {found}")]
    Derivation {
        /// Block name.
        name: String,
        /// Field.
        field: &'static str,
        /// Expected value.
        expected: f32,
        /// Actual value.
        found: f32,
    },
    /// The registry had no blocks.
    #[error("content registry has no blocks")]
    Empty,
}

/// Per-block runtime table (`Blocks` resource; plan 07 §3.2).
///
/// Instances are stored behind `Arc` so behavior code can hold block data across
/// a mutable ECS borrow without cloning per tick.
#[derive(Resource, Clone)]
pub struct BlockTable {
    instances: Vec<Option<Arc<BlockInstance>>>,
    by_name: indexmap::IndexMap<String, BlockId>,
    count: usize,
}

impl std::fmt::Debug for BlockTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockTable")
            .field("count", &self.count)
            .finish()
    }
}

/// Back-compat alias matching the plan's `Blocks` resource name.
pub type Blocks = BlockTable;

impl BlockTable {
    /// Builds the table from a content registry and behavior overrides.
    pub fn build(
        content: &ContentRegistry,
        registry: &BehaviorRegistry,
    ) -> Result<Self, BlockError> {
        let defs = content.blocks();
        if defs.is_empty() {
            return Err(BlockError::Empty);
        }
        // Dense id order: `BlockDef.id.index()` is the raw content id.
        let max = defs.iter().map(|def| def.id.index()).max().unwrap_or(0);
        let mut instances: Vec<Option<Arc<BlockInstance>>> = (0..=max).map(|_| None).collect();
        let mut by_name = indexmap::IndexMap::new();
        let mut count = 0;
        for def in defs {
            let mut instance = BlockInstance::from_def(def, registry)?;
            instance.kind_data.apply_vanilla_knobs(content, &def.name);
            let instance = Arc::new(instance);
            by_name.insert(def.name.clone(), def.id);
            instances[def.id.index()] = Some(instance);
            count += 1;
        }
        Ok(Self {
            instances,
            by_name,
            count,
        })
    }

    /// Builds with the default plan-07/08 behavior registry (logistics families).
    pub fn build_default(content: &ContentRegistry) -> Result<Self, BlockError> {
        Self::build(content, &super::blocks::default_registry(content))
    }

    /// Instance by id.
    pub fn get(&self, id: BlockId) -> Option<&Arc<BlockInstance>> {
        self.instances.get(id.index()).and_then(Option::as_ref)
    }

    /// Cloned instance handle (cheap `Arc` clone) for use across mutable borrows.
    pub fn instance(&self, id: BlockId) -> Option<Arc<BlockInstance>> {
        self.get(id).cloned()
    }

    /// Resolves a content name to an id.
    pub fn id_of(&self, name: &str) -> Option<BlockId> {
        self.by_name.get(name).copied()
    }

    /// Instance by content name.
    pub fn get_named(&self, name: &str) -> Option<&Arc<BlockInstance>> {
        let id = self.id_of(name)?;
        self.get(id)
    }

    /// Number of blocks in the table.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the table has no blocks (never for a real registry).
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Iterates instances in id order.
    pub fn iter(&self) -> impl Iterator<Item = &Arc<BlockInstance>> + '_ {
        self.instances.iter().filter_map(Option::as_ref)
    }

    /// Whether a block rotates (`Block.rotate` predicate for plan 04's
    /// `write_client_plans`).
    pub fn rotate(&self, id: BlockId) -> bool {
        self.get(id).is_some_and(|instance| instance.rotate)
    }

    /// Returns a [`BlockView`] combining metadata + runtime.
    pub fn view(&self, id: BlockId) -> Option<BlockView<'_>> {
        let inst = self.get(id)?;
        Some(BlockView {
            def: inst.def.as_ref(),
            inst,
        })
    }
}

/// Combined metadata + runtime view (`BlockView`).
#[derive(Debug, Clone, Copy)]
pub struct BlockView<'a> {
    /// Metadata record.
    pub def: &'a BlockDef,
    /// Runtime instance.
    pub inst: &'a BlockInstance,
}

impl BlockView<'_> {
    /// Content id.
    pub fn id(&self) -> BlockId {
        self.def.id
    }

    /// Content name.
    pub fn name(&self) -> &str {
        &self.def.name
    }

    /// Size in tiles.
    pub fn size(&self) -> i32 {
        self.def.size
    }

    /// `Block.offset`.
    pub fn offset(&self) -> f32 {
        self.def.offset
    }

    /// `Block.sizeOffset`.
    pub fn size_offset(&self) -> i32 {
        self.def.size_offset
    }

    /// Health.
    pub fn health(&self) -> i32 {
        self.def.health
    }

    /// Build time.
    pub fn build_time(&self) -> f32 {
        self.def.build_time
    }

    /// Item capacity.
    pub fn item_capacity(&self) -> i32 {
        self.def.item_capacity
    }

    /// Liquid capacity.
    pub fn liquid_capacity(&self) -> f32 {
        self.def.liquid_capacity
    }

    /// `Block.hasItems`.
    pub fn has_items(&self) -> bool {
        self.def.has_items
    }

    /// `Block.hasLiquids`.
    pub fn has_liquids(&self) -> bool {
        self.def.has_liquids
    }

    /// `Block.hasPower`.
    pub fn has_power(&self) -> bool {
        self.def.has_power
    }

    /// `Block.outputsPower`.
    pub fn outputs_power(&self) -> bool {
        self.def.outputs_power
    }

    /// `Block.consumesPower`.
    pub fn consumes_power(&self) -> bool {
        self.def.consumes_power
    }

    /// `Block.solid`.
    pub fn solid(&self) -> bool {
        self.def.solid
    }

    /// `Block.update`.
    pub fn update(&self) -> bool {
        self.def.update
    }

    /// Whether the block is "active" for status purposes (plan 07 status).
    pub fn is_active(&self) -> bool {
        self.def.update && (self.def.has_items || self.def.has_liquids || self.def.has_power)
    }

    /// `Block.rotate`.
    pub fn rotate(&self) -> bool {
        self.inst.rotate
    }

    /// `Block.configurable`.
    pub fn configurable(&self) -> bool {
        self.def.configurable
    }

    /// Replace group.
    pub fn group(&self) -> BlockGroup {
        self.def.group
    }

    /// Block flags.
    pub fn flags(&self) -> &[BlockFlag] {
        &self.def.flags
    }

    /// Whether the block has a flag.
    pub fn has_flag(&self, flag: BlockFlag) -> bool {
        self.def.flags.contains(&flag)
    }

    /// Required environment mask.
    pub fn env_required(&self) -> &EnvMask {
        &self.def.env_required
    }

    /// Enabled environment mask.
    pub fn env_enabled(&self) -> &EnvMask {
        &self.def.env_enabled
    }

    /// Disabled environment mask.
    pub fn env_disabled(&self) -> &EnvMask {
        &self.def.env_disabled
    }

    /// `Block.placeablePlayer`.
    pub fn placeable_player(&self) -> bool {
        self.def.placeable_player
    }

    /// `Block.placeableLiquid`.
    pub fn placeable_liquid(&self) -> bool {
        self.def.placeable_liquid
    }

    /// `Block.placeableOn`.
    pub fn placeable_on(&self) -> bool {
        self.def.placeable_on
    }

    /// `Block.destructible`.
    pub fn destructible(&self) -> bool {
        self.def.destructible
    }

    /// Barriers/limits (`Block.canBeBuilt` metadata half).
    pub fn can_be_built(&self) -> bool {
        !matches!(
            self.def.build_visibility,
            crate::content::BuildVisibility::Hidden | crate::content::BuildVisibility::DebugOnly
        )
    }

    /// Consumers.
    pub fn consumers(&self) -> &Consumers {
        &self.inst.consumers
    }

    /// Typed family data.
    pub fn kind_data(&self) -> &BlockKindData {
        &self.inst.kind_data
    }

    /// Building family tag.
    pub fn building(&self) -> BuildingKind {
        self.inst.building
    }

    /// Registered behavior.
    pub fn behavior(&self) -> &Arc<dyn BuildingBehavior> {
        &self.inst.behavior
    }

    /// Bar descriptors.
    pub fn bars(&self) -> &[BarSpec] {
        &self.inst.bars
    }

    /// Map color, when set.
    pub fn map_color(&self) -> Option<Rgba> {
        self.def.map_color
    }
}

/// Maps typed family data to the building family tag.
pub fn building_kind(data: &BlockKindData) -> BuildingKind {
    use BlockKindData as D;
    match data {
        D::Construct { .. } => BuildingKind::CONSTRUCT,
        D::Crafter(_) | D::AttributeCrafter(_) | D::Incinerator(_) => BuildingKind::CRAFTER,
        D::Drill(_) | D::BurstDrill(_) | D::BeamDrill(_) | D::WallCrafter(_) => BuildingKind::DRILL,
        D::Separator(_) => BuildingKind::SEPARATOR,
        D::LiquidSource(_) => BuildingKind::LIQUID_SOURCE,
        D::Accelerator(_) => BuildingKind::ACCELERATOR,
        D::LaunchPad(_) => BuildingKind::LAUNCH_PAD,
        D::Wall(_) => BuildingKind::WALL,
        D::Door(_) => BuildingKind::DOOR,
        D::Logic(_) => BuildingKind::LOGIC,
        D::Memory(_) => BuildingKind::MEMORY,
        D::Switch(_) => BuildingKind::SWITCH,
        D::Message(_) => BuildingKind::MESSAGE,
        D::Display(_) => BuildingKind::DISPLAY,
        D::TileableDisplay(_) => BuildingKind::TILEABLE_DISPLAY,
        D::Canvas(_) => BuildingKind::CANVAS,
        _ => BuildingKind::GENERIC,
    }
}

/// Whether a block kind rotates by default (`Block.rotate` derived default).
///
/// Plan 02 owns `BlockDef.rotate`; until that field exists this is the derived
/// predicate the plan-04 `write_client_plans` seam consumes. The list is the
/// rotation-capable set of this plan's families plus the transport/turret kinds
/// owned by plans 08/10.
pub fn kind_rotates(kind: crate::content::BlockKind) -> bool {
    use crate::content::BlockKind as K;
    matches!(
        kind,
        K::Conveyor
            | K::StackConveyor
            | K::ArmoredConveyor
            | K::Junction
            | K::BufferedItemBridge
            | K::ItemBridge
            | K::Sorter
            | K::Router
            | K::OverflowGate
            | K::MassDriver
            | K::Duct
            | K::DuctRouter
            | K::OverflowDuct
            | K::DuctBridge
            | K::DirectionalUnloader
            | K::StackRouter
            | K::DirectionLiquidBridge
            | K::Conduit
            | K::ArmoredConduit
            | K::LiquidJunction
            | K::LiquidBridge
            | K::LiquidRouter
            | K::Drill
            | K::BurstDrill
            | K::WallCrafter
            | K::Door
            | K::AutoDoor
            | K::Thruster
            | K::ItemTurret
            | K::LiquidTurret
            | K::PowerTurret
            | K::TractorBeamTurret
            | K::PointDefenseTurret
            | K::LaserTurret
            | K::ContinuousLiquidTurret
            | K::ContinuousTurret
            | K::PayloadConveyor
            | K::PayloadRouter
            | K::PayloadMassDriver
            | K::PayloadDeconstructor
            | K::Constructor
            | K::PayloadLoader
            | K::PayloadUnloader
            | K::PowerNode
            | K::PowerDiode
            | K::BeamNode
    )
}

/// Default lifecycle sounds by size (`Block.init`).
pub fn default_sounds(def: &BlockDef) -> BlockSounds {
    let area = def.size * def.size;
    BlockSounds {
        place: Some(String::from("place")),
        break_sound: Some(String::from("break")),
        destroy: Some(if area >= 4 {
            String::from("explosion")
        } else {
            String::from("break")
        }),
        ambient: None,
    }
}

/// Asserts plan 02's derived equations (`Block.init`) and returns a structured
/// error on drift (plan 07 §7a `blocks_derivation_matches`).
pub fn assert_derivations(def: &BlockDef) -> Result<(), BlockError> {
    let size = def.size as f32;
    let expected_offset = ((def.size + 1) % 2) as f32 * TILE_SIZE / 2.0;
    if (def.offset - expected_offset).abs() > f32::EPSILON {
        return Err(BlockError::Derivation {
            name: def.name.clone(),
            field: "offset",
            expected: expected_offset,
            found: def.offset,
        });
    }
    let expected_size_offset = -((def.size - 1) / 2) as f32;
    if (def.size_offset as f32 - expected_size_offset).abs() > f32::EPSILON {
        return Err(BlockError::Derivation {
            name: def.name.clone(),
            field: "size_offset",
            expected: expected_size_offset,
            found: def.size_offset as f32,
        });
    }
    // Health formula guard: positive blocks must have a computed health.
    if def.health < 0 && size > 0.0 {
        return Err(BlockError::Derivation {
            name: def.name.clone(),
            field: "health",
            expected: 1.0,
            found: def.health as f32,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_support::test_registry;

    fn table() -> (ContentRegistry, BlockTable) {
        let content = test_registry();
        let table = BlockTable::build_default(&content).expect("table");
        (content, table)
    }

    #[test]
    fn table_covers_registry_and_view_reads() {
        let (content, table) = table();
        assert_eq!(table.len(), content.blocks().len());
        let wall = content.block_id("copper-wall").expect("copper-wall");
        let view = table.view(wall).expect("view");
        assert_eq!(view.size(), 1);
        assert!(!view.has_items());
        assert!(view.solid());
        assert_eq!(view.health(), 320);
        assert!(!view.rotate());
        assert_eq!(view.offset(), 0.0);
    }

    #[test]
    fn derivation_matches_def() {
        let (content, _table) = table();
        for def in content.blocks() {
            assert_derivations(def).unwrap_or_else(|error| panic!("{error}"));
        }
    }

    #[test]
    fn rotate_predicate_wires_plan04_seam() {
        let (content, table) = table();
        let door = content.block_id("door").expect("door");
        assert!(table.rotate(door));
        let wall = content.block_id("copper-wall").expect("copper-wall");
        assert!(!table.rotate(wall));
    }

    #[test]
    fn consumers_are_lowered_in_declaration_order() {
        let (content, table) = table();
        let smelter = content
            .block_id("silicon-smelter")
            .expect("silicon-smelter");
        let view = table.view(smelter).expect("view");
        assert!(!view.consumers().is_empty());
        assert!(view.consumers().cons_power.is_some());
    }
}
