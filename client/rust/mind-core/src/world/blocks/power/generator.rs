// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Generator family (`world/blocks/power/{PowerGenerator,ConsumeGenerator,
//! ThermalGenerator,SolarGenerator,ImpactReactor,NuclearReactor,
//! VariableReactor,HeaterGenerator}.java`).
//!
//! Plan 07's `BlockDef`/`BlockKindData` does not carry the per-class generator
//! knobs (`powerProduction`, `itemDuration`, filters, reactor rates), so plan 09
//! owns [`GeneratorConfig`]/[`GeneratorState`] components (the same seam used by
//! `PowerNodeInfo`). The `ConsumeGenerator.updateTile` algorithm and the
//! `PowerGenerator`/`ConsumeGenerator` `getPowerProduction`/`warmup` rules are
//! ported verbatim.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ContentRegistry, ItemId, LiquidId};
use crate::entities::comp::Building;
use crate::world::modules::{ItemModule, LiquidModule};

use super::PowerProduction;

/// A generator's item/liquid filter (`ConsumeItemFlammable`/
/// `ConsumeLiquidFlammable`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GeneratorFilter {
    /// `ConsumeItemFlammable(min)`.
    ItemFlammable {
        /// Minimum flammability accepted.
        min: f32,
    },
    /// `ConsumeLiquidFlammable(amount, min)`.
    LiquidFlammable {
        /// Amount per tick at full efficiency.
        amount: f32,
        /// Minimum flammability accepted.
        min: f32,
    },
}

/// Generator knobs (`PowerGenerator`/`ConsumeGenerator` fields).
#[derive(Debug, Clone, PartialEq, Component)]
pub struct GeneratorConfig {
    /// `PowerGenerator.powerProduction` (per tick at efficiency 1).
    pub power_production: f32,
    /// `ConsumeGenerator.itemDuration` (ticks per item).
    pub item_duration: f32,
    /// `ConsumeGenerator.warmupSpeed`.
    pub warmup_speed: f32,
    /// `ConsumeGenerator.outputLiquid`.
    pub output_liquid: Option<(LiquidId, f32)>,
    /// `ConsumeGenerator.explodeOnFull`.
    pub explode_on_full: bool,
    /// `consume(new ConsumeItemFlammable(..))` present.
    pub filter_item: Option<GeneratorFilter>,
    /// `consume(new ConsumeLiquidFlammable(..))` present.
    pub filter_liquid: Option<GeneratorFilter>,
    /// `Block.liquidCapacity`.
    pub liquid_capacity: f32,
    /// `PowerGenerator.explosionMinWarmup`.
    pub explosion_min_warmup: f32,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            power_production: 0.0,
            item_duration: 120.0,
            warmup_speed: 0.05,
            output_liquid: None,
            explode_on_full: false,
            filter_item: None,
            filter_liquid: None,
            liquid_capacity: 0.0,
            explosion_min_warmup: 0.0,
        }
    }
}

/// `GeneratorBuild`/`ConsumeGeneratorBuild` runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct GeneratorState {
    /// `ConsumeGeneratorBuild.warmup`.
    pub warmup: f32,
    /// `ConsumeGeneratorBuild.totalTime`.
    pub total_time: f32,
    /// `ConsumeGeneratorBuild.efficiencyMultiplier`.
    pub efficiency_multiplier: f32,
    /// `ConsumeGeneratorBuild.itemDurationMultiplier`.
    pub item_duration_multiplier: f32,
    /// `GeneratorBuild.generateTime`.
    pub generate_time: f32,
    /// `GeneratorBuild.productionEfficiency`.
    pub production_efficiency: f32,
}

impl GeneratorState {
    /// A generator that has not ticked yet (`efficiencyMultiplier = 1`).
    pub fn new() -> Self {
        Self {
            efficiency_multiplier: 1.0,
            item_duration_multiplier: 1.0,
            ..Self::default()
        }
    }
}

/// Arc `Mathf.lerpDelta(from, to, progress)` with an explicit `delta`.
fn lerp_delta(from: f32, to: f32, progress: f32, delta: f32) -> f32 {
    from + (to - from) * (progress * delta).clamp(0.0, 1.0)
}

/// Item flammabilities indexed by `ItemId` (content order), for behaviors.
pub fn item_flammabilities(content: &ContentRegistry) -> Vec<f32> {
    content
        .items()
        .iter()
        .map(|item| item.flammability)
        .collect()
}

/// Liquid flammabilities indexed by `LiquidId` (content order), for behaviors.
pub fn liquid_flammabilities(content: &ContentRegistry) -> Vec<f32> {
    content
        .liquids()
        .iter()
        .map(|liquid| liquid.flammability)
        .collect()
}

/// First item in ascending id order accepted by `filter` and stored.
pub fn consumed_item(
    flammability: &[f32],
    module: &ItemModule,
    filter: GeneratorFilter,
) -> Option<ItemId> {
    let GeneratorFilter::ItemFlammable { min } = filter else {
        return None;
    };
    flammability
        .iter()
        .enumerate()
        .find(|(index, value)| module.get(ItemId::new(*index as u16)) > 0 && **value >= min)
        .map(|(index, _)| ItemId::new(index as u16))
}

/// First liquid in ascending id order accepted by `filter` and stored.
pub fn consumed_liquid(
    flammability: &[f32],
    module: &LiquidModule,
    filter: GeneratorFilter,
) -> Option<LiquidId> {
    let GeneratorFilter::LiquidFlammable { min, .. } = filter else {
        return None;
    };
    flammability
        .iter()
        .enumerate()
        .find(|(index, value)| module.get(LiquidId::new(*index as u16)) > 0.0 && **value >= min)
        .map(|(index, _)| LiquidId::new(index as u16))
}

/// `GeneratorBuild.getPowerProduction()`.
pub fn get_power_production(world: &World, entity: Entity) -> f32 {
    let config = world.get::<GeneratorConfig>(entity);
    let state = world.get::<GeneratorState>(entity);
    let enabled = world
        .get::<Building>(entity)
        .map(|building| building.enabled)
        .unwrap_or(false);
    match (config, state, enabled) {
        (Some(config), Some(state), true) => config.power_production * state.production_efficiency,
        _ => 0.0,
    }
}

/// `PowerGenerator.GeneratorBuild.warmup()`.
pub fn generator_warmup(world: &World, entity: Entity) -> f32 {
    let enabled = world
        .get::<Building>(entity)
        .map(|building| building.enabled)
        .unwrap_or(false);
    if enabled {
        world
            .get::<GeneratorState>(entity)
            .map(|state| state.production_efficiency)
            .unwrap_or(0.0)
    } else {
        0.0
    }
}

/// `PowerGenerator.GeneratorBuild.shouldExplode()`.
pub fn should_explode(world: &World, entity: Entity) -> bool {
    let min_warmup = world
        .get::<GeneratorConfig>(entity)
        .map(|config| config.explosion_min_warmup)
        .unwrap_or(0.0);
    generator_warmup(world, entity) >= min_warmup
}

/// `ConsumeGeneratorBuild.updateTile` + `updateEfficiencyMultiplier` +
/// `updateConsumption`'s single-filter efficiency pass.
///
/// `delta` is `Time.delta` (D8); the building `timeScale` is applied as in
/// `Building.delta()` where the Java code calls `delta()`.
pub fn update_generator(world: &mut World, entity: Entity, delta: f32, content: &ContentRegistry) {
    let item_flammability = item_flammabilities(content);
    let liquid_flammability = liquid_flammabilities(content);
    update_generator_with(
        world,
        entity,
        delta,
        &item_flammability,
        &liquid_flammability,
    );
}

/// [`update_generator`] with precomputed flammability tables (behaviors avoid
/// rebuilding them per tick).
pub fn update_generator_with(
    world: &mut World,
    entity: Entity,
    delta: f32,
    item_flammability: &[f32],
    liquid_flammability: &[f32],
) {
    let Some(config) = world.get::<GeneratorConfig>(entity).cloned() else {
        return;
    };
    let enabled = world
        .get::<Building>(entity)
        .map(|building| building.enabled)
        .unwrap_or(false);
    let mut state = world
        .get::<GeneratorState>(entity)
        .copied()
        .unwrap_or_else(GeneratorState::new);

    // `updateConsumption` pass: the filter is the only consumer, and the
    // efficiency pass assumes `efficiency == 1` (so `edelta = delta`).
    let mut efficiency = if enabled { 1.0 } else { 0.0 };
    if enabled {
        if let Some(filter) = config.filter_liquid {
            let amount = match filter {
                GeneratorFilter::LiquidFlammable { amount, .. } => amount,
                _ => 0.0,
            };
            efficiency = world
                .get::<LiquidModule>(entity)
                .and_then(|module| {
                    consumed_liquid(liquid_flammability, module, filter).map(|liquid| {
                        if amount > 0.0 && delta > 0.000_000_1 {
                            (module.get(liquid) / (amount * delta)).min(1.0)
                        } else {
                            0.0
                        }
                    })
                })
                .unwrap_or(0.0);
        } else if let Some(filter) = config.filter_item {
            let module = world.get::<ItemModule>(entity);
            let has_item = module
                .and_then(|module| consumed_item(item_flammability, module, filter))
                .is_some();
            efficiency = if has_item || state.generate_time > 0.0 {
                1.0
            } else {
                0.0
            };
        }
    }
    if let Some(mut building) = world.get_mut::<Building>(entity) {
        building.efficiency = efficiency;
    }

    let valid = efficiency > 0.0;

    // `updateEfficiencyMultiplier`: only overwrite when the new value is > 0.
    if let Some(filter) = config.filter_item {
        let multiplier = world
            .get::<ItemModule>(entity)
            .and_then(|module| consumed_item(item_flammability, module, filter))
            .and_then(|item| item_flammability.get(item.index()).copied())
            .unwrap_or(0.0);
        if multiplier > 0.0 {
            state.efficiency_multiplier = multiplier;
        }
    } else if let Some(filter) = config.filter_liquid {
        let multiplier = world
            .get::<LiquidModule>(entity)
            .and_then(|module| consumed_liquid(liquid_flammability, module, filter))
            .and_then(|liquid| liquid_flammability.get(liquid.index()).copied())
            .unwrap_or(0.0);
        if multiplier > 0.0 {
            state.efficiency_multiplier = multiplier;
        }
    }

    state.warmup = lerp_delta(
        state.warmup,
        if valid { 1.0 } else { 0.0 },
        config.warmup_speed,
        delta,
    );
    let production_efficiency = efficiency * state.efficiency_multiplier;
    state.production_efficiency = production_efficiency;
    state.total_time += state.warmup * delta;

    // Take in items periodically (`hasItems && valid && generateTime <= 0`).
    if let Some(filter) = config.filter_item
        && valid
        && state.generate_time <= 0.0
    {
        let consumed = world
            .get::<ItemModule>(entity)
            .and_then(|module| consumed_item(item_flammability, module, filter));
        if let Some(item) = consumed
            && let Some(mut module) = world.get_mut::<ItemModule>(entity)
        {
            module.remove(item, 1);
        }
        state.generate_time = 1.0;
    }

    // Liquid consumer `update`: `amount * edelta`.
    if let Some(GeneratorFilter::LiquidFlammable { amount, .. }) = config.filter_liquid {
        let consumed = world.get::<LiquidModule>(entity).and_then(|module| {
            consumed_liquid(
                liquid_flammability,
                module,
                GeneratorFilter::LiquidFlammable { amount, min: 0.0 },
            )
        });
        if let Some(liquid) = consumed
            && let Some(mut module) = world.get_mut::<LiquidModule>(entity)
        {
            module.remove(liquid, amount * efficiency * delta);
        }
    }

    // Output liquid.
    if let Some((liquid, amount)) = config.output_liquid {
        let capacity = config.liquid_capacity;
        let added = (production_efficiency * delta * amount).min(
            capacity
                - world
                    .get::<LiquidModule>(entity)
                    .map(|module| module.get(liquid))
                    .unwrap_or(0.0),
        );
        if let Some(mut module) = world.get_mut::<LiquidModule>(entity) {
            module.add(liquid, added.max(0.0), capacity);
        }
        if config.explode_on_full {
            let full = world
                .get::<LiquidModule>(entity)
                .map(|module| module.get(liquid) >= capacity - 0.01)
                .unwrap_or(false);
            if full {
                crate::world::blocks::power::sandbox::kill_building(world, entity);
            }
        }
    }

    // Generation time always decreases.
    if config.item_duration > 0.0 {
        state.generate_time -=
            delta / (config.item_duration * state.item_duration_multiplier.max(f32::EPSILON));
    }

    world.entity_mut(entity).insert(state);
    let production = if enabled {
        config.power_production * production_efficiency
    } else {
        0.0
    };
    world.entity_mut(entity).insert(PowerProduction(production));
}

/// `ConsumeGeneratorBuild.consumeTriggerValid()`.
pub fn consume_trigger_valid(world: &World, entity: Entity) -> bool {
    world
        .get::<GeneratorState>(entity)
        .map(|state| state.generate_time > 0.0)
        .unwrap_or(false)
}

/// `PowerGenerator.onDestroyed` trigger gate (explosion effects are plan 10/17).
pub fn on_generator_destroyed(world: &World, entity: Entity, reactor_explosions: bool) -> bool {
    reactor_explosions && should_explode(world, entity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_delta_matches_arc() {
        assert_eq!(lerp_delta(0.0, 1.0, 0.5, 1.0), 0.5);
        assert_eq!(lerp_delta(0.0, 1.0, 1.0, 1.0), 1.0);
        // Progress is clamped by delta.
        assert_eq!(lerp_delta(0.0, 1.0, 2.0, 0.5), 1.0);
    }
}
