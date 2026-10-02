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

use super::config::ConfigValue;
use super::modules::PowerGraphId;
use super::stats::Stats;

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

    /// IO revision (`ConstructBuild=1`, `DrillBuild=1`, …; default 0).
    pub const fn revision(self) -> u8 {
        match self.0 {
            // ConstructBuild=1, DrillBuild=1, SeparatorBuild=1,
            // LiquidSourceBuild=1, AcceleratorBuild=1, LaunchPadBuild=1.
            1 | 3 | 4 | 5 | 6 | 7 => 1,
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
    fn efficiency_scale(&self, world: &World, e: Entity) -> f32 {
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
    Arc::new(NoopBehavior)
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
}
