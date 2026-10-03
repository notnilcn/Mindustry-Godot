// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Constructor` build behavior
//! (`world/blocks/payloads/Constructor.java`) — plan 08 M7.
//!
//! Configurable `BlockProducer`: `recipe` config with `canProduce` size/filter
//! checks; produces a `BuildPayload` when the item requirements are consumed.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind};
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::block::BlockTable;
use crate::world::config::ConfigValue;

use super::PayloadHolder;
use super::block_producer::{
    BlockProducerBehavior, BlockProducerBuild, ensure_producer_state, producer_tick,
};

/// `Constructor` behavior (`constructor`, `large-constructor`).
#[derive(Debug, Clone, Copy)]
pub struct ConstructorBehavior {
    /// Production speed (progress per tick).
    pub build_speed: f32,
    /// `minBlockSize`.
    pub min_block_size: i32,
    /// `maxBlockSize`.
    pub max_block_size: i32,
}

impl ConstructorBehavior {
    /// Vanilla `constructor` (speed 0.6, size 1..2).
    pub const VANILLA: ConstructorBehavior = ConstructorBehavior {
        build_speed: 0.6,
        min_block_size: 1,
        max_block_size: 2,
    };
    /// `large-constructor` (speed 5, size 3..4).
    pub const LARGE: ConstructorBehavior = ConstructorBehavior {
        build_speed: 5.0,
        min_block_size: 3,
        max_block_size: 4,
    };
}

/// `Constructor.canProduce(b)` (`isVisible`/size/banned/env checks omitted until
/// the rules/visibility seams land; the size/core/filter gates are exact).
pub fn can_produce(world: &World, block: BlockId, min: i32, max: i32) -> bool {
    let Some(inst) = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.get(block))
    else {
        return false;
    };
    let size = inst.def.size;
    !inst.def.removed && size >= min && size <= max && inst.def.kind != BlockKind::CoreBlock
}

impl BuildingBehavior for ConstructorBehavior {
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
        ensure_producer_state(world, e);
    }

    fn accept_item(
        &self,
        world: &World,
        e: Entity,
        src: Entity,
        item: crate::content::ItemId,
    ) -> bool {
        BlockProducerBehavior::BASE.accept_item(world, e, src, item)
    }

    fn get_maximum_accepted(&self, world: &World, e: Entity, item: crate::content::ItemId) -> i32 {
        super::block_producer::producer_max_accepted(world, e, item)
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

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        match world.get::<BlockProducerBuild>(e).and_then(|s| s.recipe) {
            Some(block) => ConfigValue::Block(block),
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
        let recipe = match value {
            ConfigValue::Block(block) => {
                if can_produce(world, block, self.min_block_size, self.max_block_size) {
                    Some(block)
                } else {
                    return;
                }
            }
            ConfigValue::None => None,
            _ => return,
        };
        let previous = world.get::<BlockProducerBuild>(e).and_then(|s| s.recipe);
        if previous != recipe
            && let Some(mut state) = world.get_mut::<BlockProducerBuild>(e)
        {
            state.recipe = recipe;
            state.progress = 0.0;
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        super::payload_block::write_base(world, e, w);
        let state = world
            .get::<BlockProducerBuild>(e)
            .copied()
            .unwrap_or_default();
        w.f(state.progress);
        w.s(state.recipe.map(|b| b.raw() as i16).unwrap_or(-1));
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
        let recipe = r.s().unwrap_or(-1);
        if let Some(mut state) = world.get_mut::<BlockProducerBuild>(e) {
            state.progress = progress;
            state.recipe = (recipe >= 0).then(|| BlockId::new(recipe as u16));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;
    use crate::world::modules::ItemModule;

    #[test]
    fn constructor_produces_configured_recipe() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let constructor = harness
            .content()
            .block_id("constructor")
            .expect("constructor");
        let conveyor = harness
            .content()
            .block_id("payload-conveyor")
            .expect("payload-conveyor");
        let recipe = harness
            .content()
            .block_id("copper-wall")
            .expect("copper-wall");
        let copper = harness.content().item_id("copper").expect("copper");
        if let Some(mut rules) = harness
            .world
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.cheat = true;
        }
        // Constructor (size 3) front edge is 2 tiles from center.
        assert!(harness.place(9, 6, conveyor, 0, true));
        assert!(harness.place(6, 6, constructor, 0, true));
        assert!(harness.configure(6, 6, ConfigValue::Block(recipe)));
        let e = harness.build_at(6, 6).expect("constructor");
        if let Some(mut items) = harness.world.get_mut::<ItemModule>(e) {
            items.add(copper, 100, 1000);
        }
        for _ in 0..300 {
            harness.tick();
        }
        let front = harness.build_at(9, 6).expect("front conveyor");
        let payload = harness
            .world
            .get::<super::super::PayloadHolder>(front)
            .and_then(|h| h.payload);
        assert!(
            payload.is_some_and(|p| p.content == recipe.raw()),
            "constructor should have produced the configured block payload"
        );
    }
}
