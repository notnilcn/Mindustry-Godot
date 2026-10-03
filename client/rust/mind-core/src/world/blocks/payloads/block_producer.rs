// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BlockProducer` core (`world/blocks/payloads/BlockProducer.java`) — plan 08
//! M7. Produces a `BuildPayload` from a recipe block's item requirements.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ItemId};
use crate::entities::comp::Building;
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;
use crate::world::update::edelta;

use super::payload_block::{move_out_payload, payload_block_update};
use super::{PayloadHolder, create_build_payload};

/// `BlockProducer.BlockProducerBuild` state.
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct BlockProducerBuild {
    /// Recipe block being built (`recipe`; `Constructor` sets it via config).
    pub recipe: Option<BlockId>,
    /// Build progress in ticks (`progress`).
    pub progress: f32,
    /// Animation time (`time`).
    pub time: f32,
    /// Smooth activity (`heat`).
    pub heat: f32,
}

/// `BlockProducer` behavior (build progress + payload output).
#[derive(Debug, Clone, Copy)]
pub struct BlockProducerBehavior {
    /// `buildSpeed` (progress per tick).
    pub build_speed: f32,
}

impl BlockProducerBehavior {
    /// Base producer speed (`BlockProducer.buildSpeed = 0.4`).
    pub const BASE: BlockProducerBehavior = BlockProducerBehavior { build_speed: 0.4 };
}

impl Default for BlockProducerBehavior {
    fn default() -> Self {
        Self::BASE
    }
}

/// Recipe item requirements (`Block.requirements`), empty when unknown.
pub fn recipe_requirements(world: &World, recipe: BlockId) -> Vec<(ItemId, i32)> {
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(recipe).map(|inst| inst.def.requirements.clone()))
        .map(|reqs| reqs.into_iter().map(|s| (s.item, s.amount)).collect())
        .unwrap_or_default()
}

/// Recipe build time in ticks.
pub fn recipe_build_time(world: &World, recipe: BlockId) -> f32 {
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(recipe).map(|inst| inst.def.build_time))
        .unwrap_or(1.0)
        .max(f32::EPSILON)
}

/// `BlockProducerBuild.getMaximumAccepted(item)` (`requirement * 2`).
pub fn producer_max_accepted(world: &World, e: Entity, item: ItemId) -> i32 {
    let Some(recipe) = world.get::<BlockProducerBuild>(e).and_then(|s| s.recipe) else {
        return 0;
    };
    recipe_requirements(world, recipe)
        .into_iter()
        .find(|(candidate, _)| *candidate == item)
        .map(|(_, amount)| amount.max(0) * 2)
        .unwrap_or(0)
}

/// Removes the recipe requirements from the building's item module.
fn consume_recipe(world: &mut World, e: Entity, recipe: BlockId) {
    let reqs = recipe_requirements(world, recipe);
    if let Some(mut items) = world.get_mut::<ItemModule>(e) {
        for (item, amount) in reqs {
            items.remove(item, amount);
        }
    }
}

/// One producer tick. Returns whether a payload was created.
pub fn producer_tick(world: &mut World, e: Entity, build_speed: f32) -> Option<PayloadRef> {
    payload_block_update(world, e);
    let recipe = world.get::<BlockProducerBuild>(e).and_then(|s| s.recipe);
    let efficiency = world
        .get::<Building>(e)
        .map(|b| b.efficiency)
        .unwrap_or(0.0);
    let occupied = world
        .get::<PayloadHolder>(e)
        .is_some_and(|h| h.payload.is_some());
    let recipe = recipe?;
    let produce = efficiency > 0.0 && !occupied;

    if produce {
        let edt = edelta(world, e);
        if let Some(mut state) = world.get_mut::<BlockProducerBuild>(e) {
            state.progress += build_speed * edt;
        }
        let progress = world
            .get::<BlockProducerBuild>(e)
            .map(|s| s.progress)
            .unwrap_or(0.0);
        if progress >= recipe_build_time(world, recipe) {
            consume_recipe(world, e, recipe);
            let team = world
                .get::<crate::entities::comp::TeamComp>(e)
                .map(|t| t.team)
                .unwrap_or(0);
            if let Some(payload_entity) = create_build_payload(world, recipe, team) {
                let payload = PayloadRef {
                    entity: Some(payload_entity),
                    content: recipe.raw(),
                    is_block: true,
                };
                if let Some(mut holder) = world.get_mut::<PayloadHolder>(e) {
                    holder.payload = Some(payload);
                    holder.pay_vector = (0.0, 0.0);
                }
                if let Some(mut state) = world.get_mut::<BlockProducerBuild>(e) {
                    state.progress %= 1.0;
                }
            }
        }
    }

    let heat_target = if produce { 1.0 } else { 0.0 };
    let delta = world
        .get::<Building>(e)
        .map(|b| b.time_scale)
        .unwrap_or(0.0);
    if let Some(mut state) = world.get_mut::<BlockProducerBuild>(e) {
        state.heat += (heat_target - state.heat) * 0.15;
        state.time += state.heat * delta;
    }

    let payload = world.get::<PayloadHolder>(e).and_then(|h| h.payload);
    move_out_payload(world, e);
    payload
}

impl BuildingBehavior for BlockProducerBehavior {
    fn update_batch(
        &self,
        world: &mut bevy_ecs::world::World,
        inst: &crate::world::block::BlockInstance,
        entities: &[bevy_ecs::entity::Entity],
    ) {
        // Plan 08 §7.4: allocate-free batched dispatch (empty-consumer fast path).
        for &e in entities {
            crate::world::update::building_update_no_consumers(world, e, inst);
        }
    }

    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<BlockProducerBuild>(e).is_none() {
            world.entity_mut(e).insert(BlockProducerBuild::default());
        }
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
    }

    fn accept_item(&self, world: &World, e: Entity, _src: Entity, item: ItemId) -> bool {
        let max = producer_max_accepted(world, e, item);
        world
            .get::<ItemModule>(e)
            .is_some_and(|m| m.get(item) < max)
    }

    fn get_maximum_accepted(&self, world: &World, e: Entity, item: ItemId) -> i32 {
        producer_max_accepted(world, e, item)
    }

    fn accept_payload(
        &self,
        _world: &World,
        _e: Entity,
        _source: Entity,
        _payload: PayloadRef,
    ) -> bool {
        false
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        world.get::<PayloadHolder>(e).and_then(|h| h.payload)
    }

    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        super::take_payload(world, e)
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        producer_tick(world, e, self.build_speed);
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        super::payload_block::write_base(world, e, w);
        let progress = world
            .get::<BlockProducerBuild>(e)
            .map(|s| s.progress)
            .unwrap_or(0.0);
        w.f(progress);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        super::payload_block::read_base(world, e, r);
        let progress = r.f().unwrap_or(0.0);
        if let Some(mut state) = world.get_mut::<BlockProducerBuild>(e) {
            state.progress = progress;
        }
    }
}

/// Ensures a producer has the base components.
pub fn ensure_producer_state(world: &mut World, e: Entity) {
    if world.get::<BlockProducerBuild>(e).is_none() {
        world.entity_mut(e).insert(BlockProducerBuild::default());
    }
    if world.get::<PayloadHolder>(e).is_none() {
        world.entity_mut(e).insert(PayloadHolder::default());
    }
}
