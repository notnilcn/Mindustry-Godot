// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Cross-family block helper traits (`world/blocks/{ControlBlock,RotBlock,
//! UnitTetherBlock,LaunchAnimator,ExplosionShield}.java`).
//!
//! Plan 07 §3.12: these are Rust traits implemented by family modules and later
//! plans (10/11/12). They carry behavior hooks only; state lives on the owning
//! plan's components.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

/// `ControlBlock` — blocks that can control a unit.
pub trait ControlBlock {
    /// The controlled unit, if any.
    fn unit(&self, _world: &World, _e: Entity) -> Option<Entity> {
        None
    }
    /// Whether a unit is being controlled.
    fn is_controlled(&self, _world: &World, _e: Entity) -> bool {
        false
    }
    /// Whether `unit` may be controlled.
    fn can_control(&self, _world: &World, _e: Entity, _unit: Entity) -> bool {
        false
    }
    /// Whether the block should auto-target.
    fn should_auto_target(&self, _world: &World, _e: Entity) -> bool {
        true
    }
}

/// `RotBlock` — overrides effective build rotation (e.g. mirrored rotors).
pub trait RotBlock {
    /// Effective rotation for a raw build rotation.
    fn build_rotation(&self, _rotation: u8, _size: i32) -> u8 {
        _rotation
    }
}

/// `UnitTetherBlock` — tracks a spawned unit by id (`spawned(id)`).
pub trait UnitTetherBlock {
    /// Whether `id` is the currently tethered unit.
    fn spawned(&self, _id: u32) -> bool {
        false
    }
}

/// `LaunchAnimator` — shared launch animation state (`Accelerator`/`LaunchPad`).
pub trait LaunchAnimator {
    /// Launch duration in ticks (`launchDuration`).
    fn launch_duration(&self) -> f32 {
        0.0
    }
    /// Camera zoom during launch.
    fn zoom(&self) -> f32 {
        0.0
    }
    /// Whether the launch has begun.
    fn is_launching(&self) -> bool {
        false
    }
}

/// `ExplosionShield` — absorbs explosions over a radius.
pub trait ExplosionShield {
    /// Whether an explosion at `(x, y)` is absorbed.
    fn absorb_explosion(&self, _x: f32, _y: f32) -> bool {
        false
    }
    /// Tile radius protected.
    fn shield_radius(&self) -> i32 {
        0
    }
}

/// `Block.consumesItem(item)` over the lowered generic consumers
/// (`ConsumeItems` stacks; `Block.itemFilter`).
pub fn block_consumes_item(world: &World, e: Entity, item: crate::content::ItemId) -> bool {
    use crate::world::consumers::ConsumeInstanceKind;
    instance(world, e).is_some_and(|inst| {
        inst.consumers.all.iter().any(|consumer| {
            matches!(&consumer.kind, ConsumeInstanceKind::Items(stacks)
                if stacks.iter().any(|stack| stack.item == item))
        })
    })
}

/// `Block.consumesLiquid(liquid)` over the lowered generic consumers
/// (`ConsumeLiquid`/`ConsumeLiquids`/`ConsumeCoolant`; `Block.liquidFilter`).
pub fn block_consumes_liquid(world: &World, e: Entity, liquid: crate::content::LiquidId) -> bool {
    use crate::world::consumers::ConsumeInstanceKind;
    instance(world, e).is_some_and(|inst| {
        inst.consumers
            .all
            .iter()
            .any(|consumer| match &consumer.kind {
                ConsumeInstanceKind::Liquid { liquid: wanted, .. } => *wanted == liquid,
                ConsumeInstanceKind::Liquids(stacks) => {
                    stacks.iter().any(|stack| stack.liquid == liquid)
                }
                _ => false,
            })
    })
}

/// `Block.liquidCapacity` for a building (0 when unknown).
pub fn liquid_capacity(world: &World, e: Entity) -> f32 {
    instance(world, e)
        .map(|inst| inst.def.liquid_capacity.max(0.0))
        .unwrap_or(0.0)
}

/// `BuildingComp.handleLiquid` default: `liquids.add(liquid, amount)`.
pub fn default_handle_liquid(
    world: &mut World,
    e: Entity,
    liquid: crate::content::LiquidId,
    amount: f32,
) {
    let capacity = liquid_capacity(world, e);
    if let Some(mut module) = world.get_mut::<crate::world::modules::LiquidModule>(e) {
        module.add(liquid, amount, capacity);
    }
}

/// `BuildingComp.acceptItem` default: `block.consumesItem(item) &&
/// items.get(item) < getMaximumAccepted(item)` (item capacity).
pub fn consumer_accept_item(world: &World, e: Entity, item: crate::content::ItemId) -> bool {
    use crate::world::blocks::distribution::transfer;
    world.get::<crate::world::modules::ItemModule>(e).is_some()
        && block_consumes_item(world, e, item)
        && transfer::item_count(world, e, item) < transfer::item_capacity(world, e)
}

/// `BuildingComp.acceptLiquid` default: `block.hasLiquids &&
/// block.consumesLiquid(liquid)`.
pub fn consumer_accept_liquid(world: &World, e: Entity, liquid: crate::content::LiquidId) -> bool {
    world
        .get::<crate::world::modules::LiquidModule>(e)
        .is_some()
        && block_consumes_liquid(world, e, liquid)
}

fn instance(
    world: &World,
    e: Entity,
) -> Option<std::sync::Arc<crate::world::block::BlockInstance>> {
    let block = world.get::<crate::entities::comp::Building>(e)?.block;
    world
        .get_resource::<crate::world::block::BlockTable>()?
        .instance(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct IdentityRot;
    impl RotBlock for IdentityRot {}

    #[test]
    fn default_helper_traits_are_inert() {
        assert_eq!(IdentityRot.build_rotation(2, 2), 2);
        struct NoShield;
        impl ExplosionShield for NoShield {}
        assert!(!NoShield.absorb_explosion(0.0, 0.0));
        assert_eq!(NoShield.shield_radius(), 0);
    }
}
