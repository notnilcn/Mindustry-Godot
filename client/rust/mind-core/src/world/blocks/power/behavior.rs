// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `ConsumeGenerator` as a plan-07 [`BuildingBehavior`] (plan 09 R3;
//! `world/blocks/power/ConsumeGenerator.java`).
//!
//! Plan 02's `BlockDef` carries no generator knobs, so the behavior owns a
//! [`GeneratorConfig`] template per vanilla block (same seam as
//! [`super::generator`]) and precomputes the content-ordered flammability tables
//! once at registry build time, then drives [`update_generator_with`] each tick.
//!
//! Covered here are the vanilla flammable-fuel generators; generators whose
//! consumers are item/liquid *specific* (`differential-generator`,
//! `chemical-combustion-chamber`, `pyrolysis-generator`) or radioactive-filtered
//! (`rtg-generator`) need consumer kinds beyond [`GeneratorFilter`] and are
//! tracked as an R3 follow-up.

use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::world::behavior::{BehaviorRegistry, BuildingBehavior};
use crate::world::blocks::power::PowerProduction;
use crate::world::blocks::power::generator::{
    GeneratorConfig, GeneratorFilter, GeneratorState, item_flammabilities, liquid_flammabilities,
    update_generator_with,
};
use crate::world::update::delta;

/// `ConsumeGenerator` behavior for one vanilla block.
#[derive(Clone)]
pub struct ConsumeGeneratorBehavior {
    /// Per-block generator knobs.
    pub config: GeneratorConfig,
    /// Item flammability indexed by `ItemId`.
    pub item_flammability: Arc<[f32]>,
    /// Liquid flammability indexed by `LiquidId`.
    pub liquid_flammability: Arc<[f32]>,
}

impl BuildingBehavior for ConsumeGeneratorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<GeneratorConfig>(e).is_none() {
            world.entity_mut(e).insert(self.config.clone());
        }
        if world.get::<GeneratorState>(e).is_none() {
            world.entity_mut(e).insert(GeneratorState::new());
        }
        if world.get::<PowerProduction>(e).is_none() {
            world.entity_mut(e).insert(PowerProduction(0.0));
        }
        // Plan 02 metadata does not yet lower `ConsumeItemFlammable`/
        // `ConsumeLiquidFlammable` into `Consume` entries or set `hasItems`/
        // `hasLiquids`, so the behavior supplies the modules it consumes from.
        if world.get::<crate::world::modules::ItemModule>(e).is_none() {
            world
                .entity_mut(e)
                .insert(crate::world::modules::ItemModule::with_items(
                    self.item_flammability.len(),
                ));
        }
        if world
            .get::<crate::world::modules::LiquidModule>(e)
            .is_none()
        {
            world
                .entity_mut(e)
                .insert(crate::world::modules::LiquidModule::with_liquids(
                    self.liquid_flammability.len(),
                ));
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let delta = delta(world, e);
        update_generator_with(
            world,
            e,
            delta,
            &self.item_flammability,
            &self.liquid_flammability,
        );
    }
}

/// Registers the vanilla flammable-fuel generators.
pub fn register(registry: &mut BehaviorRegistry, content: &ContentRegistry) {
    let item_flammability: Arc<[f32]> = item_flammabilities(content).into();
    let liquid_flammability: Arc<[f32]> = liquid_flammabilities(content).into();

    let flammable = GeneratorConfig {
        filter_item: Some(GeneratorFilter::ItemFlammable { min: 0.2 }),
        ..GeneratorConfig::default()
    };

    for (name, config) in [
        (
            "combustion-generator",
            GeneratorConfig {
                power_production: 1.0,
                item_duration: 120.0,
                ..flammable.clone()
            },
        ),
        (
            "steam-generator",
            GeneratorConfig {
                power_production: 5.5,
                item_duration: 90.0,
                ..flammable.clone()
            },
        ),
    ] {
        registry.register_named(
            name,
            Arc::new(ConsumeGeneratorBehavior {
                config,
                item_flammability: item_flammability.clone(),
                liquid_flammability: liquid_flammability.clone(),
            }),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn combustion_generator_runs_on_coal() {
        let mut harness = BuildHarness::new(8, 8, 7);
        let block = harness
            .content()
            .block_id("combustion-generator")
            .expect("combustion-generator");
        let coal = harness.content().item_id("coal").expect("coal");
        assert!(harness.place(2, 2, block, 0, true));
        let entity = harness.build_at(2, 2).expect("generator");
        {
            let mut items = harness
                .world
                .get_mut::<crate::world::modules::ItemModule>(entity)
                .expect("items");
            items.add(coal, 10, 100);
        }
        for _ in 0..120 {
            harness.tick();
        }
        let production = harness
            .world
            .get::<PowerProduction>(entity)
            .map(|production| production.0)
            .unwrap_or(0.0);
        let state = harness
            .world
            .get::<GeneratorState>(entity)
            .copied()
            .unwrap_or_default();
        assert!(
            production > 0.0,
            "production={production} efficiency={}",
            state.production_efficiency
        );
    }
}
