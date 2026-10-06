// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit-block `BuildingBehavior`s (plan 11 §3.11, M7 wiring half).
//!
//! Ported from `world/blocks/units/*.java`'s `updateTile`/`acceptItem`/
//! `acceptPayload` overrides. Plan 07's [`BuildingBehavior::update_tile`] only
//! receives `&mut World`, so the resolved content (factory plans, upgrade pairs,
//! produced [`UnitTypeDef`]s) is captured into the behavior at registry-build
//! time in [`register`]; the kernels from [`super`] are then driven per tick by
//! `world::update::update_buildings`. Unit spawning uses plan 11's
//! [`spawn_unit_def`], so produced units enter the same lifecycle as any other
//! spawn.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::registries::units::UnitTypeDef;
use crate::content::{BlockKind, ContentRegistry, ItemId, UnitTypeId};
use crate::entities::comp::unit::lifecycle::spawn_unit_def;
use crate::entities::comp::{Pos, TeamComp};
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::modules::ItemModule;

use super::{
    AssemblerPayload, Reconstructor, ReconstructorBuild, RepairTower, RepairTurret, UnitAssembler,
    UnitCargoLoader, UnitCargoUnloadPoint, UnitFactory, UnitFactoryBuild, UnitPlan,
};

/// Monotonic entity-sequence allocator for units produced by blocks.
///
/// Mirrors plan 11's per-harness `seq`; the host/reset inserts or resets it.
#[derive(Debug, Default, bevy_ecs::prelude::Resource)]
pub struct UnitBlockSeq(pub u64);

fn next_unit_seq(world: &mut World) -> u64 {
    if !world.contains_resource::<UnitBlockSeq>() {
        world.insert_resource(UnitBlockSeq(1_500_000));
    }
    let mut seq = world.resource_mut::<UnitBlockSeq>();
    let value = seq.0;
    seq.0 = seq.0.wrapping_add(1);
    value
}

/// Spawns `unit` at the center of building `e` (`UnitBlock` output path).
fn spawn_output_unit(world: &mut World, e: Entity, def: &UnitTypeDef) {
    let Some(pos) = world.get::<Pos>(e).copied() else {
        return;
    };
    let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
    let seq = next_unit_seq(world);
    let _ = spawn_unit_def(world, seq, def, team, pos.x, pos.y, 90.0);
}

/// Resolves every produced unit's [`UnitTypeDef`] from content.
fn resolve_units(
    content: &ContentRegistry,
    ids: impl IntoIterator<Item = UnitTypeId>,
) -> BTreeMap<u16, Arc<UnitTypeDef>> {
    let mut map = BTreeMap::new();
    for id in ids {
        if let Some(def) = content.unit(id) {
            map.insert(id.raw(), Arc::new(def.clone()));
        }
    }
    map
}

// ---------------------------------------------------------------------------
// UnitFactory
// ---------------------------------------------------------------------------

/// `UnitFactoryBuild` behavior (`UnitFactory.updateTile`/`acceptItem`).
pub struct UnitFactoryBehavior {
    /// Resolved factory plans + capacities.
    pub factory: UnitFactory,
    /// Produced unit defs keyed by raw [`UnitTypeId`].
    pub units: BTreeMap<u16, Arc<UnitTypeDef>>,
    /// Number of item slots (`ItemModule.items.len()`).
    pub item_count: usize,
}

impl UnitFactoryBehavior {
    /// Builds a behavior from a block's resolved metadata.
    pub fn from_def(def: &crate::content::BlockDef, content: &ContentRegistry) -> Self {
        let plans: Vec<UnitPlan> = def
            .unit_plans
            .iter()
            .map(|plan| {
                UnitPlan::new(
                    plan.unit,
                    plan.requirements
                        .iter()
                        .map(|stack| (stack.item, stack.amount))
                        .collect(),
                    plan.time,
                )
            })
            .collect();
        let factory = UnitFactory::new(plans, content.items().len());
        let units = resolve_units(content, def.unit_plans.iter().map(|plan| plan.unit));
        Self {
            factory,
            units,
            item_count: content.items().len(),
        }
    }
}

impl BuildingBehavior for UnitFactoryBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(UnitFactoryBuild::new(
            self.factory.plans.len(),
            self.item_count,
        ));
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<UnitFactoryBuild>() else {
            return;
        };
        if let Some(items) = world.get::<ItemModule>(e) {
            for (index, slot) in state.items.iter_mut().enumerate() {
                *slot = items.items.get(index).copied().unwrap_or(0);
            }
        }
        let plan_index = state.current_plan;
        if let Some(unit) = state.update(&self.factory, 1.0) {
            // `UnitFactoryBuild.updateTile`: remove the plan's requirements from
            // the building's item storage (`items.remove` per stack).
            if let Some(plan) = self.factory.plans.get(plan_index)
                && let Some(mut items) = world.get_mut::<ItemModule>(e)
            {
                for (item, amount) in &plan.requirements {
                    items.remove(*item, *amount);
                }
            }
            if let Some(def) = self.units.get(&unit.raw()) {
                spawn_output_unit(world, e, def);
            }
        }
        world.entity_mut(e).insert(state);
    }

    fn accept_item(&self, world: &World, e: Entity, _src: Entity, item: ItemId) -> bool {
        let Some(state) = world.get::<UnitFactoryBuild>(e) else {
            return false;
        };
        if !self.factory.accepts(item) {
            return false;
        }
        let held = state.items.get(item.index()).copied().unwrap_or(0);
        held < self.factory.capacity(item)
    }

    fn get_maximum_accepted(&self, _world: &World, _e: Entity, item: ItemId) -> i32 {
        self.factory.capacity(item)
    }

    fn accept_stack(
        &self,
        world: &World,
        e: Entity,
        item: ItemId,
        amount: i32,
        _source: Option<Entity>,
    ) -> i32 {
        if !self.accept_item(world, e, Entity::PLACEHOLDER, item) {
            return 0;
        }
        let held = world
            .get::<UnitFactoryBuild>(e)
            .and_then(|state| state.items.get(item.index()).copied())
            .unwrap_or(0);
        let free = (self.factory.capacity(item) - held).max(0);
        free.min(amount)
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            let capacity = self.factory.capacity(item);
            items.add(item, amount, capacity);
        }
    }
}

// ---------------------------------------------------------------------------
// Reconstructor
// ---------------------------------------------------------------------------

/// `ReconstructorBuild` behavior (`Reconstructor.updateTile`/`acceptPayload`).
pub struct ReconstructorBehavior {
    /// Resolved upgrade pairs + construct time.
    pub reconstructor: Reconstructor,
    /// Upgrade result defs keyed by raw [`UnitTypeId`].
    pub units: BTreeMap<u16, Arc<UnitTypeDef>>,
}

impl ReconstructorBehavior {
    /// Builds a behavior from a block's resolved metadata.
    pub fn from_def(def: &crate::content::BlockDef, content: &ContentRegistry) -> Self {
        let reconstructor = Reconstructor::new(def.reconstructor_upgrades.clone(), 120.0);
        let units = resolve_units(
            content,
            def.reconstructor_upgrades.iter().map(|(_, to)| *to),
        );
        Self {
            reconstructor,
            units,
        }
    }
}

impl BuildingBehavior for ReconstructorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(ReconstructorBuild::default());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<ReconstructorBuild>() else {
            return;
        };
        if let Some(target) = state.update(&self.reconstructor, 1.0)
            && let Some(def) = self.units.get(&target.raw())
        {
            spawn_output_unit(world, e, def);
        }
        world.entity_mut(e).insert(state);
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        _source: Entity,
        payload: PayloadRef,
    ) -> bool {
        if payload.is_block {
            return false;
        }
        let unit = UnitTypeId::new(payload.content);
        if self.reconstructor.can_upgrade(unit).is_none() {
            return false;
        }
        let Some(state) = world.get::<ReconstructorBuild>(e) else {
            return false;
        };
        state.payload.is_none()
    }

    fn handle_payload(&self, world: &mut World, e: Entity, _source: Entity, payload: PayloadRef) {
        if payload.is_block {
            return;
        }
        if let Some(mut state) = world.get_mut::<ReconstructorBuild>(e) {
            let _ = state.accept_payload(&self.reconstructor, UnitTypeId::new(payload.content));
        }
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        let state = world.get::<ReconstructorBuild>(e)?;
        let unit = state.payload?;
        Some(PayloadRef {
            entity: None,
            content: unit.raw(),
            is_block: false,
        })
    }
}

// ---------------------------------------------------------------------------
// UnitAssembler / module
// ---------------------------------------------------------------------------

/// `UnitAssemblerBuild` behavior (tier/module rules + payload assembly).
pub struct AssemblerBehavior {
    /// Resolved assembler configuration.
    pub assembler: UnitAssembler,
    /// Produced unit defs keyed by raw [`UnitTypeId`].
    pub units: BTreeMap<u16, Arc<UnitTypeDef>>,
    /// Block size in tiles (spawn offset math).
    pub size: i32,
}

impl AssemblerBehavior {
    /// Builds a behavior from a block's resolved metadata
    /// (`UnitAssembler.plans`).
    pub fn from_def(def: &crate::content::BlockDef, content: &ContentRegistry) -> Self {
        let plans: Vec<super::AssemblerUnitPlan> = def
            .assembler_plans
            .iter()
            .map(|plan| super::AssemblerUnitPlan {
                unit: plan.unit,
                payloads: plan
                    .payloads
                    .iter()
                    .map(|payload| super::AssemblerPayload {
                        is_block: payload.item.type_ == crate::content::ContentType::Block,
                        content: payload.item.id,
                        amount: payload.amount,
                    })
                    .collect(),
                time: plan.time,
                drones: 4,
            })
            .collect();
        let units = resolve_units(content, def.assembler_plans.iter().map(|plan| plan.unit));
        Self {
            assembler: UnitAssembler::new(plans, 240.0),
            units,
            size: def.size,
        }
    }

    /// `UnitAssemblerBuild.getUnitSpawn()`: the unit spawn point is
    /// `TILE_SIZE * (areaSize + size)/2` along the building rotation.
    fn unit_spawn(&self, world: &World, e: Entity) -> Option<(f32, f32)> {
        let pos = world.get::<Pos>(e)?;
        let rotation = world
            .get::<crate::entities::comp::Building>(e)
            .map(|building| building.rotation)
            .unwrap_or(0);
        let len =
            crate::world::block::TILE_SIZE * (self.assembler.area_size + self.size) as f32 / 2.0;
        let (dx, dy) = crate::world::blocks::autotiler::d4(rotation);
        Some((pos.x + dx as f32 * len, pos.y + dy as f32 * len))
    }
}

impl BuildingBehavior for AssemblerBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.assembler.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut assembler) = world.entity_mut(e).take::<UnitAssembler>() else {
            return;
        };
        // `UnitAssemblerBuild.updateTile` gates progress on `efficiency > 0`
        // (`shouldConsume` power/liquid).
        let efficiency = world
            .get::<crate::entities::comp::Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        if efficiency > 0.0
            && let Some(unit) = assembler.update(efficiency)
            && let Some(def) = self.units.get(&unit.raw())
            && let Some((x, y)) = self.unit_spawn(world, e)
        {
            let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
            let seq = next_unit_seq(world);
            let _ = spawn_unit_def(world, seq, def, team, x, y, 90.0);
        }
        world.entity_mut(e).insert(assembler);
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        _source: Entity,
        payload: PayloadRef,
    ) -> bool {
        let Some(assembler) = world.get::<UnitAssembler>(e) else {
            return false;
        };
        assembler.accepts_payload(AssemblerPayload {
            is_block: payload.is_block,
            content: payload.content,
            amount: 1,
        })
    }

    fn handle_payload(&self, world: &mut World, e: Entity, _source: Entity, payload: PayloadRef) {
        let Some(mut assembler) = world.get_mut::<UnitAssembler>(e) else {
            return;
        };
        assembler.store_payload(AssemblerPayload {
            is_block: payload.is_block,
            content: payload.content,
            amount: 1,
        });
    }

    fn get_payload(&self, _world: &World, _e: Entity) -> Option<PayloadRef> {
        None
    }
}

/// `UnitAssemblerModuleBuild` behavior (registers its tier with the assembler).
pub struct AssemblerModuleBehavior;

impl AssemblerModuleBehavior {
    /// `basic-assembler-module` tier (`UnitAssemblerModule.tier` default 1).
    pub const TIER: i32 = 1;
}

impl BuildingBehavior for AssemblerModuleBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<super::UnitAssemblerModule>(e).is_none() {
            world.entity_mut(e).insert(super::UnitAssemblerModule {
                tier: Self::TIER,
                assembler: None,
            });
        }
    }

    /// `UnitAssemblerModuleBuild.findLink`/`updateModules`: an adjacent
    /// assembler takes the module's tier (the module is always built against
    /// the assembler's build area).
    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let tier = world
            .get::<super::UnitAssemblerModule>(e)
            .map(|module| module.tier)
            .unwrap_or(Self::TIER);
        let neighbors: Vec<Entity> = world
            .get::<crate::entities::comp::Building>(e)
            .map(|building| building.proximity.iter().copied().collect())
            .unwrap_or_default();
        for other in neighbors {
            if world.get::<UnitAssembler>(other).is_none() {
                continue;
            }
            if let Some(mut assembler) = world.get_mut::<UnitAssembler>(other) {
                assembler.module_tier = assembler.module_tier.max(tier);
            }
            let packed = world
                .get::<crate::entities::comp::Building>(other)
                .map(|building| building.tile.pack());
            if let Some(mut module) = world.get_mut::<super::UnitAssemblerModule>(e) {
                module.assembler = packed;
            }
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// RepairTower / RepairTurret
// ---------------------------------------------------------------------------

/// `RepairTowerBuild` behavior (6-tick target refresh; aura heal in `shields`).
pub struct RepairTowerBehavior {
    /// Resolved tower configuration.
    pub tower: RepairTower,
}

impl BuildingBehavior for RepairTowerBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.tower.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        if let Some(mut tower) = world.get_mut::<RepairTower>(e) {
            let _ = tower.update_refresh();
        }
    }
}

/// `RepairPointBuild` behavior (60-tick target reacquisition interval).
pub struct RepairTurretBehavior {
    /// Resolved turret configuration.
    pub turret: RepairTurret,
}

impl BuildingBehavior for RepairTurretBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.turret.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        if let Some(mut turret) = world.get_mut::<RepairTurret>(e) {
            let _ = turret.update_target();
        }
    }
}

// ---------------------------------------------------------------------------
// UnitCargoLoader / UnitCargoUnloadPoint
// ---------------------------------------------------------------------------

/// `UnitCargoLoaderBuild` behavior (tether unit spawn; `CargoAI` is plan 11 M2).
pub struct CargoLoaderBehavior {
    /// Resolved loader configuration.
    pub loader: UnitCargoLoader,
    /// Spawned `manifold` unit def keyed by raw [`UnitTypeId`].
    pub units: BTreeMap<u16, Arc<UnitTypeDef>>,
}

impl BuildingBehavior for CargoLoaderBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.loader.clone());
        if world.get::<super::CargoLoaderState>(e).is_none() {
            world
                .entity_mut(e)
                .insert(super::CargoLoaderState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let Some(mut state) = world.entity_mut(e).take::<super::CargoLoaderState>() else {
            return;
        };
        let unit_alive = state.unit.is_some_and(|unit| {
            world
                .get::<crate::entities::comp::unit::UnitCore>(unit)
                .is_some_and(|core| !core.dead)
        });
        let delta = crate::world::update::edelta(world, e);
        if let Some(unit) = state.update(
            delta,
            self.loader.unit_build_time,
            self.loader.unit_type,
            unit_alive,
        ) && let Some(def) = self.units.get(&unit.raw())
            && let Some(pos) = world.get::<Pos>(e).copied()
        {
            let team = world.get::<TeamComp>(e).map(|t| t.team).unwrap_or(0);
            let seq = next_unit_seq(world);
            state.unit = Some(spawn_unit_def(world, seq, def, team, pos.x, pos.y, 90.0));
        }
        world.entity_mut(e).insert(state);
    }
}

/// `UnitCargoUnloadPointBuild` behavior (`updateTile` stale timer).
pub struct CargoUnloadPointBehavior {
    /// Resolved unload-point configuration.
    pub point: UnitCargoUnloadPoint,
}

impl BuildingBehavior for CargoUnloadPointBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        world.entity_mut(e).insert(self.point.clone());
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let has_items = world
            .get::<ItemModule>(e)
            .map(|items| items.total > 0)
            .unwrap_or(false);
        if let Some(mut point) = world.get_mut::<UnitCargoUnloadPoint>(e) {
            let _ = point.update_stale(has_items);
        }
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Registers every vanilla unit-block behavior into plan 07's registry.
///
/// Runs inside [`crate::world::blocks::default_registry`] so a placed unit block
/// updates through `world::update::update_buildings` with no harness override.
pub fn register(
    registry: &mut crate::world::behavior::BehaviorRegistry,
    content: &ContentRegistry,
) {
    for def in content.blocks() {
        let behavior: Option<Arc<dyn BuildingBehavior>> = match def.kind {
            BlockKind::UnitFactory => Some(Arc::new(UnitFactoryBehavior::from_def(def, content))),
            BlockKind::Reconstructor => {
                Some(Arc::new(ReconstructorBehavior::from_def(def, content)))
            }
            BlockKind::UnitAssembler => Some(Arc::new(AssemblerBehavior::from_def(def, content))),
            BlockKind::UnitAssemblerModule => Some(Arc::new(AssemblerModuleBehavior)),
            BlockKind::RepairTower => Some(Arc::new(RepairTowerBehavior {
                tower: RepairTower::default(),
            })),
            BlockKind::RepairTurret => Some(Arc::new(RepairTurretBehavior {
                turret: RepairTurret::default(),
            })),
            BlockKind::UnitCargoLoader => {
                // `Blocks.java unit-cargo-loader` spawns `UnitTypes.manifold`.
                let unit_type = content.unit_id("manifold").unwrap_or(UnitTypeId::new(0));
                Some(Arc::new(CargoLoaderBehavior {
                    loader: UnitCargoLoader {
                        unit_type,
                        unit_build_time: 480.0,
                    },
                    units: resolve_units(content, [unit_type]),
                }))
            }
            BlockKind::UnitCargoUnloadPoint => Some(Arc::new(CargoUnloadPointBehavior {
                point: UnitCargoUnloadPoint::default(),
            })),
            _ => None,
        };
        if let Some(behavior) = behavior {
            registry.register(def.id, behavior);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::{MemoryBundle, MemoryUnlockStore};
    use crate::entities::comp::Unit;
    use crate::world::block::BlockTable;
    use crate::world::blocks::default_registry;
    use crate::world::limits::BuildRules;
    use crate::world::update::update_buildings;
    use bevy_ecs::world::World;

    fn content() -> ContentRegistry {
        let mut registry = crate::content::create_base_content(
            &MemoryBundle::new(),
            &MemoryUnlockStore::new(),
            true,
        )
        .expect("content");
        registry.init().expect("init");
        registry
    }

    #[test]
    fn default_registry_installs_unit_block_behaviors() {
        let content = content();
        let registry = default_registry(&content);
        for kind in [
            BlockKind::UnitFactory,
            BlockKind::Reconstructor,
            BlockKind::UnitAssembler,
            BlockKind::UnitAssemblerModule,
            BlockKind::RepairTower,
            BlockKind::RepairTurret,
            BlockKind::UnitCargoLoader,
            BlockKind::UnitCargoUnloadPoint,
        ] {
            let def = content
                .blocks()
                .iter()
                .find(|def| def.kind == kind)
                .unwrap_or_else(|| panic!("vanilla block for {kind:?}"));
            assert!(
                registry.get(def.id).is_some(),
                "override missing for {}",
                def.name
            );
        }
    }

    /// gap5 GAP-4 / K-4: an assembler reads `def.assembler_plans`, accepts the
    /// planned payloads and spawns the tier-0 unit through `update_buildings`.
    #[test]
    fn assembler_reads_plans_and_spawns_tier_unit() {
        use crate::entities::comp::unit::UnitTypeComp;
        use crate::world::PayloadRef;
        use crate::world::modules::{LiquidModule, PowerModule};

        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("tank-assembler")
            .expect("tank-assembler")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(6, 6),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        // Tier-0 plan is `vanquish`: stell x4 + tungsten-wall-large x10.
        let stell = content.unit_id("stell").expect("stell");
        let wall = content.block_id("tungsten-wall-large").expect("wall");
        for _ in 0..4 {
            inst.behavior.handle_payload(
                &mut world,
                entity,
                entity,
                PayloadRef {
                    entity: None,
                    content: stell.raw(),
                    is_block: false,
                },
            );
        }
        for _ in 0..10 {
            inst.behavior.handle_payload(
                &mut world,
                entity,
                entity,
                PayloadRef {
                    entity: None,
                    content: wall.raw(),
                    is_block: true,
                },
            );
        }
        // Keep power + cyanogen efficiency at 1 while building.
        if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
            power.status = 1.0;
        }
        if let Some(cyanogen) = content.liquid_id("cyanogen")
            && let Some(mut liquids) = world.get_mut::<LiquidModule>(entity)
        {
            liquids.add(cyanogen, 1000.0, 1000.0);
        }
        let vanquish = content.unit_id("vanquish").expect("vanquish");
        let mut spawned = false;
        for _ in 0..3200 {
            if let Some(mut power) = world.get_mut::<PowerModule>(entity) {
                power.status = 1.0;
            }
            update_buildings(&mut world);
            if world.iter_entities().any(|entity_ref| {
                entity_ref.get::<UnitTypeComp>().map(|comp| comp.type_id) == Some(vanquish)
            }) {
                spawned = true;
                break;
            }
        }
        assert!(spawned, "tank-assembler produced vanquish");
    }

    /// gap5 GAP-4 / K-4: `basic-assembler-module` registers its tier with the
    /// adjacent assembler (`UnitAssemblerModuleBuild.findLink`/`checkTier`).
    #[test]
    fn assembler_module_registers_tier() {
        use crate::world::BuildHarness;

        let mut harness = BuildHarness::new(20, 20, 7);
        let assembler = harness
            .content()
            .block_id("tank-assembler")
            .expect("tank-assembler");
        let module = harness
            .content()
            .block_id("basic-assembler-module")
            .expect("basic-assembler-module");
        assert!(harness.place(7, 7, assembler, 0, true));
        assert!(harness.place(12, 7, module, 0, true));
        let entity = harness.build_at(7, 7).expect("assembler");
        let tier = harness
            .world
            .get::<UnitAssembler>(entity)
            .map(|assembler| assembler.module_tier)
            .unwrap_or(0);
        assert_eq!(tier, 1, "module raised the assembler tier");
    }

    /// gap5 GAP-12 / K-8: `unit-cargo-loader` is configured with `manifold` and
    /// spawns it through the normal behavior tick.
    #[test]
    fn cargo_loader_spawns_manifold() {
        use crate::entities::comp::unit::UnitTypeComp;

        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("unit-cargo-loader")
            .expect("unit-cargo-loader")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        let manifold = content.unit_id("manifold").expect("manifold");
        assert_eq!(
            world
                .get::<UnitCargoLoader>(entity)
                .map(|loader| loader.unit_type),
            Some(manifold),
            "loader unit type is manifold"
        );
        // `unit-cargo-loader` consumes power (0.1333) and nitrogen (0.1667).
        if let Some(mut power) = world.get_mut::<crate::world::modules::PowerModule>(entity) {
            power.status = 1.0;
        }
        if let Some(nitrogen) = content.liquid_id("nitrogen")
            && let Some(mut liquids) = world.get_mut::<crate::world::modules::LiquidModule>(entity)
        {
            liquids.add(nitrogen, 1000.0, 1000.0);
        }
        let mut spawned = false;
        for _ in 0..600 {
            if let Some(mut power) = world.get_mut::<crate::world::modules::PowerModule>(entity) {
                power.status = 1.0;
            }
            update_buildings(&mut world);
            if world.iter_entities().any(|entity_ref| {
                entity_ref.get::<UnitTypeComp>().map(|comp| comp.type_id) == Some(manifold)
            }) {
                spawned = true;
                break;
            }
        }
        assert!(spawned, "unit-cargo-loader spawned manifold");
    }

    #[test]
    fn placed_factory_produces_unit_through_update_buildings() {
        let content = content();
        let table = BlockTable::build_default(&content).expect("table");
        let inst = table
            .get_named("ground-factory")
            .expect("ground-factory")
            .clone();
        let mut world = World::new();
        world.insert_resource(BuildRules::default());
        world.insert_resource(table);
        let entity = inst.spawn(
            &mut world,
            1,
            crate::world::TilePos::new(4, 4),
            0,
            0,
            content.items().len(),
            content.liquids().len(),
        );
        assert!(
            world.get::<UnitFactoryBuild>(entity).is_some(),
            "factory state inserted by create_state"
        );
        // Feed the dagger plan (silicon 10 + lead 10).
        let silicon = content.item_id("silicon").expect("silicon");
        let lead = content.item_id("lead").expect("lead");
        if let Some(mut items) = world.get_mut::<ItemModule>(entity) {
            items.add(silicon, 20, 40);
            items.add(lead, 20, 40);
        }
        // `ground-factory` dagger plan time is 900 ticks.
        for _ in 0..1000 {
            update_buildings(&mut world);
            if world
                .iter_entities()
                .any(|entity_ref| entity_ref.contains::<Unit>())
            {
                break;
            }
        }
        assert!(
            world
                .iter_entities()
                .any(|entity_ref| entity_ref.contains::<Unit>()),
            "ground-factory produced a dagger through update_buildings"
        );
    }
}
