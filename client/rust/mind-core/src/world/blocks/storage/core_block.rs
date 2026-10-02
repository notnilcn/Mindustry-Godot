// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CoreBlock` build behavior (`world/blocks/storage/CoreBlock.java`) — plan 08
//! M5. Ports the `storageCapacity` unification, linked-storage ownership,
//! accept/handle incineration and the rev 1 `commandPos` IO.
//!
//! **R2 (NEEDS USER DECISION).** Java aliases the `items` module pointer across
//! a team's cores; ECS cannot alias components. This pass implements the
//! single-core-per-team semantics exactly (each core owns its `ItemModule`) and
//! computes the shared `storageCapacity` across all team cores. Cross-core item
//! aliasing via a `TeamInventory` resource (plan 12 `Teams`) remains the R2
//! default follow-up; the observable API used by 12/14 is unchanged.

use std::sync::Arc;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::content::{BlockKind, ItemId};
use crate::world::behavior::BuildingBehavior;
use crate::world::block::BlockTable;
use crate::world::config::ConfigValue;
use crate::world::modules::ItemModule;

use super::storage_block::{StorageBuild, incinerate_effect};

/// Campaign/core policy hooks owned by plan 12 (`Rules`/`SectorInfo`).
pub trait CoreCampaignHooks: Send + Sync {
    /// `Rules.coreIncinerates`.
    fn core_incinerates(&self) -> bool {
        false
    }
    /// `state.isCampaign()`.
    fn is_campaign(&self) -> bool {
        false
    }
    /// `Rules.defaultTeam`.
    fn default_team(&self) -> u8 {
        0
    }
    /// `Rules.allowCoreUnloaders`.
    fn allow_core_unloaders(&self) -> bool {
        true
    }
    /// `state.rules.sector.info.handleCoreItem(item, amount)`.
    fn handle_core_item(&self, _item: ItemId, _amount: i32) {}
}

/// No-op default hooks (headless/tests until plan 12 wires its `Rules`).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopCoreCampaignHooks;

impl CoreCampaignHooks for NoopCoreCampaignHooks {}

/// Registered core hooks resource.
#[derive(Resource)]
pub struct CoreHooks(pub Arc<dyn CoreCampaignHooks>);

impl Default for CoreHooks {
    fn default() -> Self {
        Self(Arc::new(NoopCoreCampaignHooks))
    }
}

/// `CoreBlock.CoreBuild` state.
#[derive(Debug, Clone, Component)]
pub struct CoreBuild {
    /// Unified storage capacity (`storageCapacity`).
    pub storage_capacity: i32,
    /// Incineration effect latch (`noEffect`).
    pub no_effect: bool,
    /// Damage immunity timer (`iframes`).
    pub iframes: f32,
    /// Launch thruster time (`thrusterTime`).
    pub thruster_time: f32,
    /// Command position (`commandPos`; nullable vec).
    pub command_pos: Option<(f32, f32)>,
}

impl Default for CoreBuild {
    fn default() -> Self {
        Self {
            storage_capacity: 0,
            no_effect: false,
            iframes: -1.0,
            thruster_time: 0.0,
            command_pos: None,
        }
    }
}

/// `CoreBlock` behavior (all six cores).
#[derive(Debug, Default, Clone, Copy)]
pub struct CoreBehavior;

fn item_capacity(world: &World, e: Entity) -> i32 {
    world
        .get::<crate::entities::comp::Building>(e)
        .zip(world.get_resource::<BlockTable>())
        .and_then(|(building, table)| table.get(building.block).map(|inst| inst.def.item_capacity))
        .unwrap_or(0)
}

fn same_team(world: &World, a: Entity, b: Entity) -> bool {
    world
        .get::<crate::entities::comp::TeamComp>(a)
        .zip(world.get::<crate::entities::comp::TeamComp>(b))
        .is_some_and(|(x, y)| x.team == y.team)
}

fn hooks(world: &World) -> Arc<dyn CoreCampaignHooks> {
    world
        .get_resource::<CoreHooks>()
        .map(|h| h.0.clone())
        .unwrap_or_else(|| Arc::new(NoopCoreCampaignHooks))
}

/// `CoreBuild.owns(tile)`: an owned/unlinked `StorageBlock` with `coreMerge`.
pub fn owns(world: &World, core: Entity, tile: Entity) -> bool {
    let Some(storage) = world.get::<StorageBuild>(tile) else {
        return false;
    };
    let is_storage = world
        .get::<crate::entities::comp::Building>(tile)
        .zip(world.get_resource::<BlockTable>())
        .is_some_and(|(building, table)| {
            table
                .get(building.block)
                .is_some_and(|inst| inst.def.kind == BlockKind::StorageBlock)
        });
    same_team(world, core, tile)
        && is_storage
        && (storage.linked_core == Some(core) || storage.linked_core.is_none())
}

impl BuildingBehavior for CoreBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        let cap = item_capacity(world, e);
        if world.get::<CoreBuild>(e).is_none() {
            world.entity_mut(e).insert(CoreBuild {
                storage_capacity: cap,
                ..CoreBuild::default()
            });
        }
        if !world.contains_resource::<CoreHooks>() {
            world.insert_resource(CoreHooks::default());
        }
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let Some(building) = world.get::<crate::entities::comp::Building>(e) else {
            return;
        };
        let proximity: Vec<Entity> = building.proximity.iter().copied().collect();
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        let base = item_capacity(world, e);

        // Owned adjacent storage: link it and fold its capacity in.
        let mut capacity = base;
        for other in &proximity {
            if owns(world, e, *other) {
                capacity += item_capacity(world, *other);
                if let Some(mut storage) = world.get_mut::<StorageBuild>(*other) {
                    storage.linked_core = Some(e);
                }
            }
        }

        // All team cores share the summed capacity (R2 deferred: items not aliased).
        let cores: Vec<Entity> = world
            .iter_entities()
            .filter_map(|entity_ref| {
                let id = entity_ref.id();
                if entity_ref.get::<CoreBuild>().is_some()
                    && team.is_some_and(|t| {
                        world
                            .get::<crate::entities::comp::TeamComp>(id)
                            .map(|c| c.team)
                            == Some(t)
                    })
                {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();
        let mut total = capacity;
        for core in &cores {
            if *core == e {
                continue;
            }
            total += item_capacity(world, *core);
            if let Some(other_building) = world.get::<crate::entities::comp::Building>(*core) {
                let other_prox: Vec<Entity> = other_building.proximity.iter().copied().collect();
                for adjacent in other_prox {
                    if owns(world, *core, adjacent) {
                        total += item_capacity(world, adjacent);
                    }
                }
            }
        }

        // Clamp items to the new capacity (not while generating).
        for core in &cores {
            if let Some(mut module) = world.get_mut::<ItemModule>(*core) {
                module.items.iter_mut().for_each(|amount| {
                    *amount = (*amount).min(total);
                });
                module.total = module.items.iter().sum();
            }
            if let Some(mut core_build) = world.get_mut::<CoreBuild>(*core) {
                core_build.storage_capacity = total;
            }
        }
        if let Some(mut core_build) = world.get_mut::<CoreBuild>(e) {
            core_build.storage_capacity = total;
        }
    }

    fn accept_item(&self, world: &World, e: Entity, _source: Entity, item: ItemId) -> bool {
        let h = hooks(world);
        if h.core_incinerates() {
            return true;
        }
        let cap = world
            .get::<CoreBuild>(e)
            .map(|c| c.storage_capacity)
            .unwrap_or(0);
        world.get::<ItemModule>(e).map(|m| m.get(item)).unwrap_or(0) < cap
    }

    fn get_maximum_accepted(&self, world: &World, e: Entity, _item: ItemId) -> i32 {
        if hooks(world).core_incinerates() {
            i32::MAX / 2
        } else {
            world
                .get::<CoreBuild>(e)
                .map(|c| c.storage_capacity)
                .unwrap_or(0)
        }
    }

    fn handle_item(&self, world: &mut World, e: Entity, source: Entity, item: ItemId) {
        let cap = world
            .get::<CoreBuild>(e)
            .map(|c| c.storage_capacity)
            .unwrap_or(0);
        let current = world.get::<ItemModule>(e).map(|m| m.get(item)).unwrap_or(0);
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        let h = hooks(world);
        if current >= cap {
            if !world.get::<CoreBuild>(e).is_some_and(|c| c.no_effect) {
                incinerate_effect(world, e, source);
            }
            if let Some(mut core) = world.get_mut::<CoreBuild>(e) {
                core.no_effect = false;
            }
            return;
        }
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, 1, cap.max(0));
        }
        if team == Some(h.default_team()) && h.is_campaign() {
            h.handle_core_item(item, 1);
        }
    }

    fn handle_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) {
        let cap = world
            .get::<CoreBuild>(e)
            .map(|c| c.storage_capacity)
            .unwrap_or(0);
        let current = world.get::<ItemModule>(e).map(|m| m.get(item)).unwrap_or(0);
        let real = amount.min((cap - current).max(0));
        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
            items.add(item, real, cap.max(0));
        }
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        let h = hooks(world);
        if team == Some(h.default_team()) && h.is_campaign() {
            h.handle_core_item(item, amount);
        }
    }

    fn remove_stack(&self, world: &mut World, e: Entity, item: ItemId, amount: i32) -> i32 {
        let result =
            super::super::distribution::transfer::default_remove_stack(world, e, item, amount);
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        let h = hooks(world);
        if team == Some(h.default_team()) && h.is_campaign() {
            h.handle_core_item(item, -result);
        }
        result
    }

    fn item_taken(&self, world: &mut World, e: Entity, item: ItemId) {
        let team = world
            .get::<crate::entities::comp::TeamComp>(e)
            .map(|t| t.team);
        let h = hooks(world);
        if team == Some(h.default_team()) && h.is_campaign() {
            h.handle_core_item(item, -1);
        }
    }

    fn can_unload(&self, world: &World, _e: Entity) -> bool {
        hooks(world).allow_core_unloaders()
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        match world.get::<CoreBuild>(e).and_then(|c| c.command_pos) {
            Some((x, y)) => ConfigValue::Point2(x as i32, y as i32),
            None => ConfigValue::None,
        }
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        if let Some(mut core) = world.get_mut::<CoreBuild>(e) {
            core.command_pos = match value {
                ConfigValue::Point2(x, y) => Some((x as f32, y as f32)),
                _ => None,
            };
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        let command = world.get::<CoreBuild>(e).and_then(|c| c.command_pos);
        match command {
            Some((x, y)) => {
                w.bool(true);
                w.f(x);
                w.f(y);
            }
            None => w.bool(false),
        }
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        let mut core = world.get::<CoreBuild>(e).cloned().unwrap_or_default();
        if revision >= 1
            && let Ok(present) = r.bool()
            && present
        {
            let x = r.f().unwrap_or(0.0);
            let y = r.f().unwrap_or(0.0);
            core.command_pos = Some((x, y));
        }
        world.entity_mut(e).insert(core);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;
    use crate::world::modules::ItemModule;

    #[test]
    fn core_capacity_unifies_adjacent_storage() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let core = harness
            .content()
            .block_id("core-shard")
            .expect("core-shard");
        let container = harness.content().block_id("container").expect("container");
        // core-shard is 3x3 centered at (6,6) -> footprint (5,5)..(7,7).
        assert!(harness.place(6, 6, core, 0, true));
        // container 2x2 at (8,6) -> footprint (8,6)-(9,7), adjacent to (7,6)/(7,7).
        assert!(harness.place(8, 6, container, 0, true));
        let core_e = harness.build_at(6, 6).expect("core");
        let cap = harness
            .world
            .get::<CoreBuild>(core_e)
            .map(|c| c.storage_capacity)
            .unwrap_or(0);
        // core-shard base 4000 + container 300.
        assert_eq!(cap, 4300, "storage capacity");
        let linked = harness
            .build_at(8, 6)
            .and_then(|e| harness.world.get::<StorageBuild>(e))
            .and_then(|s| s.linked_core);
        assert_eq!(linked, Some(core_e));
        // Deposit above capacity is rejected when not incinerating.
        let copper = harness.content().item_id("copper").expect("copper");
        {
            let mut items = harness.world.get_mut::<ItemModule>(core_e).expect("items");
            items.add(copper, 5000, 10000);
        }
        let accepted = crate::world::blocks::distribution::transfer::dispatch_accept_item(
            &harness.world,
            core_e,
            core_e,
            copper,
        );
        assert!(!accepted, "over-capacity core must reject");
    }

    #[test]
    fn core_incinerates_when_rule_set() {
        #[derive(Debug)]
        struct Incinerating;
        impl CoreCampaignHooks for Incinerating {
            fn core_incinerates(&self) -> bool {
                true
            }
        }
        let mut harness = BuildHarness::new(16, 16, 7);
        let core = harness
            .content()
            .block_id("core-shard")
            .expect("core-shard");
        assert!(harness.place(5, 5, core, 0, true));
        harness
            .world
            .insert_resource(CoreHooks(std::sync::Arc::new(Incinerating)));
        let core_e = harness.build_at(5, 5).expect("core");
        let max = crate::world::blocks::distribution::transfer::dispatch_get_maximum_accepted(
            &harness.world,
            core_e,
            ItemId::new(0),
        );
        assert_eq!(max, i32::MAX / 2);
    }
}
