// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! [`SimDynamicConsume`]: the live per-building data source for the Java
//! `Func<Building, …>` dynamic consumers.
//!
//! Upstream: core/src/mindustry/world/consumers/ConsumeItemDynamic.java (check).

use super::*;

/// Per-building data source for the Java `Func<Building, …>` dynamic consumers
/// (`ConsumeItemDynamic`/`ConsumePayloadDynamic`/`ConsumeLiquidsDynamic`).
///
/// Java expresses these consumers as per-building closures; the port's content
/// model has none, so the live sim resolves them from ECS state plus content
/// metadata:
///
/// * `ConsumeItemDynamic` — the `UnitFactory` selected plan's requirements
///   (`UnitFactory.java:84`) and the `BlockProducer` recipe requirements
///   (`BlockProducer.java:40-56`);
/// * `ConsumePayloadDynamic` — the `UnitAssembler` plan's payload requirements
///   (`UnitAssembler.java:162`).
///
/// `AssemblerUnitPlan.itemReq`/`liquidReq` (`UnitAssembler.java:163-164`) have
/// no port fields yet (`BlockDef.assembler_plans` carries payloads only), and
/// `ConsumePowerDynamic`/`ConsumePowerCondition` have no vanilla users; those
/// keep the trait defaults.
///
/// `world::consumers::runtime_efficiency`/`runtime_update`/`runtime_trigger`
/// accept `Option<&dyn DynamicConsume>`: the call site that passes this bridge
/// lives in `world/update.rs::update_consumption` (GAPS #58 cross-file half).
pub struct SimDynamicConsume<'a> {
    /// Live content registry (block defs carry the plans/requirements).
    pub content: &'a ContentRegistry,
}

impl crate::world::consumers::DynamicConsume for SimDynamicConsume<'_> {
    fn items(&self, world: &World, entity: Entity) -> Vec<crate::content::ItemStack> {
        let Some(block) = world
            .get::<crate::entities::comp::Building>(entity)
            .map(|building| building.block)
        else {
            return Vec::new();
        };
        // `UnitFactory.consume(new ConsumeItemDynamic(e -> e.currentPlan != -1
        // ? plans.get(...).requirements : ItemStack.empty))`.
        if let Some(state) = world.get::<crate::world::blocks::units::UnitFactoryBuild>(entity) {
            let plan = usize::try_from(state.current_plan)
                .ok()
                .and_then(|index| self.content.block(block)?.unit_plans.get(index));
            return plan
                .map(|plan| plan.requirements.clone())
                .unwrap_or_default();
        }
        // `BlockProducer.consume(new ConsumeItemDynamic(e -> recipe().requirements
        // * buildCostMultiplier))` — the multiplier is the consumer's, applied by
        // `runtime_efficiency`.
        if let Some(state) =
            world.get::<crate::world::blocks::payloads::block_producer::BlockProducerBuild>(entity)
        {
            return state
                .recipe
                .and_then(|recipe| self.content.block(recipe))
                .map(|def| def.requirements.clone())
                .unwrap_or_default();
        }
        Vec::new()
    }

    fn payloads(
        &self,
        world: &World,
        entity: Entity,
    ) -> Vec<crate::world::consumers::PayloadStack> {
        // `UnitAssembler.consume(consPayload = new ConsumePayloadDynamic(build
        // -> build.plan().requirements))`.
        let Some(state) = world.get::<crate::world::blocks::units::UnitAssembler>(entity) else {
            return Vec::new();
        };
        state
            .plan()
            .map(|plan| {
                plan.payloads
                    .iter()
                    .map(|payload| crate::world::consumers::PayloadStack {
                        content: payload.content,
                        is_block: payload.is_block,
                        count: payload.amount,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
