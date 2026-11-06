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

/// `ControlBlock` (`world/blocks/ControlBlock.java`) — a block with a proxy
/// unit a player can control.
///
/// Implemented by [`crate::world::blocks::distribution::RouterBehavior`]
/// (`Router.java:30-53`); the turret families own the same interface in
/// `blocks/defense/turrets/**` (`Turret.java:283`).
pub trait ControlBlock {
    /// `ControlBlock.unit()`: the proxy unit, if created.
    fn unit(&self, world: &World, e: Entity) -> Option<Entity> {
        let _ = (world, e);
        None
    }
    /// `ControlBlock.isControlled()`: `unit().isPlayer()` (`ControlBlock.java:9-11`).
    fn is_controlled(&self, world: &World, e: Entity) -> bool {
        self.unit(world, e)
            .is_some_and(|unit| world.get::<crate::entities::comp::Player>(unit).is_some())
    }
    /// `ControlBlock.canControl()` (`ControlBlock.java:13-15`).
    fn can_control(&self) -> bool {
        true
    }
    /// `ControlBlock.shouldAutoTarget()` (`ControlBlock.java:17-19`).
    fn should_auto_target(&self) -> bool {
        true
    }
}

/// `RotBlock` (`world/blocks/RotBlock.java`) — a block that has 360-degree
/// build rotation.
///
/// Every upstream implementor returns the build rotation unchanged except
/// `BuildTurretBuild` (the controlled unit's rotation,
/// `BuildTurret.java:108-110`), which needs the per-entity unit and therefore
/// belongs to the turret behavior. The port's shared behavior instance sees only
/// the raw rotation, so the identity body is the exact class behavior for
/// `BaseTurret`/`MassDriver`/`PayloadMassDriver`/`RepairTurret`.
pub trait RotBlock {
    /// `RotBlock.buildRotation()`.
    fn build_rotation(&self, rotation: u8) -> u8 {
        rotation
    }
}

/// Component recording the raw unit id a `UnitTetherBlock` was told about
/// (`UnitTetherBlock.spawned(int id)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, bevy_ecs::component::Component)]
pub struct UnitTetherTarget {
    /// Raw unit id (`readUnitId`), resolved on the next client update.
    pub unit_id: u32,
}

/// `UnitTetherBlock` (`world/blocks/UnitTetherBlock.java`) — a block whose
/// spawned unit is read back from a save (`UnitFactory`, `UnitCargoLoader`,
/// `TargetDummy`).
///
/// The live port tracks the tether by entity
/// ([`crate::entities::comp::unit::comp::UnitTetherComp`]); this hook records
/// the raw id so the save-read path can resolve it.
pub trait UnitTetherBlock {
    /// `UnitTetherBlock.spawned(int id)`: called after the block's unit is
    /// read from a save.
    fn spawned(&self, world: &mut World, e: Entity, id: u32) {
        world.entity_mut(e).insert(UnitTetherTarget { unit_id: id });
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

/// `ExplosionShield` (`world/blocks/ExplosionShield.java`) — absorbs
/// explosions over a radius.
///
/// Implemented by [`crate::world::blocks::defense::behaviors::ForceProjectorBehavior`]
/// (`ForceProjector.java:165`, `:270-278`).
pub trait ExplosionShield {
    /// `ExplosionShield.absorbExplosion(ex, ey, damage)`: whether the blast at
    /// `(x, y)` was absorbed (applying the damage to the shield when true).
    fn absorb_explosion(&self, world: &mut World, e: Entity, x: f32, y: f32, damage: f32) -> bool {
        let _ = (world, e, x, y, damage);
        false
    }
}

/// `Block.consumesItem(item)` over the lowered generic consumers
/// (`ConsumeItems` stacks plus the GAPS #58 filter kinds; `Block.itemFilter`).
pub fn block_consumes_item(world: &World, e: Entity, item: crate::content::ItemId) -> bool {
    instance(world, e).is_some_and(|inst| {
        inst.consumers.consumes_item(
            item,
            world.get_resource::<crate::world::consumers::ItemTraits>(),
        )
    })
}

/// `Block.consumesLiquid(liquid)` over the lowered generic consumers
/// (`ConsumeLiquid`/`ConsumeLiquids`/`ConsumeCoolant` plus the GAPS #58 filter
/// kinds; `Block.liquidFilter`).
pub fn block_consumes_liquid(world: &World, e: Entity, liquid: crate::content::LiquidId) -> bool {
    instance(world, e).is_some_and(|inst| {
        inst.consumers.consumes_liquid(
            liquid,
            world.get_resource::<crate::world::modules::LiquidTraits>(),
        )
    })
}

/// `BuildingComp.canUnload` default: `block.unloadable` (`Block.java:108`,
/// `unloadable = true` upstream).
///
/// False for the logistics classes that set `unloadable = false` in their
/// constructor (T1): Unloader, Junction, OverflowDuct, Duct, Sorter,
/// DuctJunction (no port kind yet), Router, ItemBridge (+BufferedItemBridge,
/// +LiquidBridge, which extends ItemBridge), DuctRouter, DirectionalUnloader,
/// Conveyor (+ArmoredConveyor), OverflowGate.
pub fn default_can_unload(world: &World, e: Entity) -> bool {
    if world.get::<crate::world::modules::ItemModule>(e).is_none() {
        return false;
    }
    let Some(inst) = instance(world, e) else {
        return false;
    };
    !matches!(
        inst.def.kind,
        crate::content::BlockKind::Unloader
            | crate::content::BlockKind::Junction
            | crate::content::BlockKind::OverflowDuct
            | crate::content::BlockKind::Duct
            | crate::content::BlockKind::Sorter
            | crate::content::BlockKind::Router
            | crate::content::BlockKind::ItemBridge
            | crate::content::BlockKind::BufferedItemBridge
            | crate::content::BlockKind::LiquidBridge
            | crate::content::BlockKind::DuctRouter
            | crate::content::BlockKind::DirectionalUnloader
            | crate::content::BlockKind::Conveyor
            | crate::content::BlockKind::ArmoredConveyor
            | crate::content::BlockKind::OverflowGate
    )
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
        assert_eq!(IdentityRot.build_rotation(2), 2);
        struct NoShield;
        impl ExplosionShield for NoShield {}
        let mut world = World::new();
        let e = world.spawn_empty().id();
        assert!(!NoShield.absorb_explosion(&mut world, e, 0.0, 0.0, 10.0));
        struct NoControl;
        impl ControlBlock for NoControl {}
        assert!(!NoControl.is_controlled(&world, e));
        assert!(NoControl.can_control());
        assert!(NoControl.should_auto_target());
        struct Tether;
        impl UnitTetherBlock for Tether {}
        Tether.spawned(&mut world, e, 7);
        assert_eq!(
            world
                .get::<UnitTetherTarget>(e)
                .map(|target| target.unit_id),
            Some(7)
        );
    }
}
