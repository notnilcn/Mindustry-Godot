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
//!
//! Generic item/liquid consumers lowered by plan 02 (`consumeItem`,
//! `consumeLiquid`, `consumeLiquids`) are executed by plan 07's
//! `updateConsumption`; the behavior only multiplies in the filter gate
//! (`ConsumeItemFlammable`/`ConsumeItemRadioactive`/`ConsumeLiquidFlammable`,
//! which plan 02 does not lower) and the `ConsumeGenerator`-specific
//! `efficiencyMultiplier`/`itemDuration`/output-liquid state. This is why
//! [`update_generator_with`] reads the `Building.efficiency` that
//! `updateConsumption` already produced instead of re-deriving it.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ContentRegistry, ItemId, LiquidId};
use crate::entities::comp::Building;
use crate::world::block::BlockTable;
use crate::world::modules::{ItemModule, LiquidModule};

use super::PowerProduction;

/// A generator's item/liquid filter (`ConsumeItemFlammable`/
/// `ConsumeItemRadioactive`/`ConsumeLiquidFlammable`) or specific consumer
/// (`consumeItem(s)`/`consumeLiquid(s)`; used where plan 02's generic consumer
/// is not lowered).
#[derive(Debug, Clone, PartialEq)]
pub enum GeneratorFilter {
    /// `ConsumeItemFlammable(min)`.
    ItemFlammable {
        /// Minimum flammability accepted.
        min: f32,
    },
    /// `ConsumeItemRadioactive(min)`.
    ItemRadioactive {
        /// Minimum radioactivity accepted.
        min: f32,
    },
    /// `ConsumeLiquidFlammable(amount, min)`.
    LiquidFlammable {
        /// Amount per tick at full efficiency.
        amount: f32,
        /// Minimum flammability accepted.
        min: f32,
    },
    /// `consumeItem`/`consumeItems` (specific stacks).
    Items {
        /// Required stacks `(item, amount)`.
        stacks: Vec<(ItemId, i32)>,
    },
    /// `consumeLiquid` (specific).
    Liquid {
        /// Liquid consumed.
        liquid: LiquidId,
        /// Amount per tick.
        amount: f32,
    },
    /// `consumeLiquids` (specific stacks).
    Liquids {
        /// Required stacks `(liquid, amount)`.
        stacks: Vec<(LiquidId, f32)>,
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
    /// Items consumed by `consume()` each `itemDuration` (`ConsumeItems.trigger`)
    /// for blocks whose item consumer is lowered generically.
    pub trigger_items: Vec<(ItemId, i32)>,
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
            trigger_items: Vec::new(),
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

/// Item radioactivities indexed by `ItemId` (content order), for behaviors.
pub fn item_radioactivities(content: &ContentRegistry) -> Vec<f32> {
    content
        .items()
        .iter()
        .map(|item| item.radioactivity)
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

/// First item in ascending id order accepted by `filter` and stored, with its
/// efficiency multiplier (`itemEfficiencyMultiplier`).
pub fn filtered_item(
    filter: &GeneratorFilter,
    module: &ItemModule,
    item_flammability: &[f32],
    item_radioactivity: &[f32],
) -> Option<(ItemId, f32)> {
    let (values, min) = match filter {
        GeneratorFilter::ItemFlammable { min } => (item_flammability, *min),
        GeneratorFilter::ItemRadioactive { min } => (item_radioactivity, *min),
        GeneratorFilter::Items { stacks } => {
            let item = stacks
                .iter()
                .find(|(item, amount)| module.get(*item) < *amount)
                .map(|(item, _)| *item);
            // `ConsumeItems.efficiency` = 1 iff every stack is present.
            return match item {
                Some(item) => Some((item, 1.0)),
                None => stacks.first().map(|(item, _)| (*item, 1.0)),
            };
        }
        _ => return None,
    };
    values
        .iter()
        .enumerate()
        .find(|(index, value)| module.get(ItemId::new(*index as u16)) > 0 && **value >= min)
        .map(|(index, value)| (ItemId::new(index as u16), *value))
}

/// First stored item accepted by `filter` (amount > 0), or `None`.
fn any_filtered_item(
    filter: &GeneratorFilter,
    module: &ItemModule,
    item_flammability: &[f32],
    item_radioactivity: &[f32],
) -> Option<ItemId> {
    match filter {
        GeneratorFilter::ItemFlammable { .. } | GeneratorFilter::ItemRadioactive { .. } => {
            filtered_item(filter, module, item_flammability, item_radioactivity)
                .map(|(item, _)| item)
        }
        GeneratorFilter::Items { stacks } => stacks
            .iter()
            .find(|(item, _)| module.get(*item) > 0)
            .map(|(item, _)| *item),
        _ => None,
    }
}

/// First liquid in ascending id order accepted by `filter` and stored.
pub fn consumed_liquid(
    flammability: &[f32],
    module: &LiquidModule,
    filter: &GeneratorFilter,
) -> Option<LiquidId> {
    let GeneratorFilter::LiquidFlammable { min, .. } = filter else {
        return None;
    };
    flammability
        .iter()
        .enumerate()
        .find(|(index, value)| module.get(LiquidId::new(*index as u16)) > 0.0 && **value >= *min)
        .map(|(index, _)| LiquidId::new(index as u16))
}

/// `ConsumeGeneratorBuild.updateEfficiencyMultiplier` gate for item filters.
fn item_gate(
    filter: &GeneratorFilter,
    module: &ItemModule,
    item_flammability: &[f32],
    item_radioactivity: &[f32],
    generate_time: f32,
) -> f32 {
    let has = match filter {
        GeneratorFilter::ItemFlammable { min } => item_flammability
            .iter()
            .enumerate()
            .any(|(index, value)| module.get(ItemId::new(index as u16)) > 0 && *value >= *min),
        GeneratorFilter::ItemRadioactive { min } => item_radioactivity
            .iter()
            .enumerate()
            .any(|(index, value)| module.get(ItemId::new(index as u16)) > 0 && *value >= *min),
        GeneratorFilter::Items { stacks } => stacks
            .iter()
            .all(|(item, amount)| module.get(*item) >= *amount),
        _ => true,
    };
    if has || generate_time > 0.0 { 1.0 } else { 0.0 }
}

/// `ConsumeLiquidFilter.efficiency` for a specific liquid list.
fn liquid_gate(
    filter: &GeneratorFilter,
    module: &LiquidModule,
    liquid_flammability: &[f32],
    delta: f32,
) -> f32 {
    if delta <= 0.000_000_1 {
        return 0.0;
    }
    match filter {
        GeneratorFilter::LiquidFlammable { amount, min: _ } => {
            consumed_liquid(liquid_flammability, module, filter)
                .map(|liquid| (module.get(liquid) / (amount * delta)).clamp(0.0, 1.0))
                .unwrap_or(0.0)
        }
        GeneratorFilter::Liquid { liquid, amount } => {
            (module.get(*liquid) / (amount * delta)).clamp(0.0, 1.0)
        }
        GeneratorFilter::Liquids { stacks } => {
            let mut min = 1.0f32;
            for (liquid, amount) in stacks {
                min = min.min(module.get(*liquid) / (amount * delta));
            }
            min.clamp(0.0, 1.0)
        }
        _ => 1.0,
    }
}

/// `ConsumeGeneratorBuild.updateEfficiencyMultiplier` value.
fn filter_multiplier(
    filter: &GeneratorFilter,
    module: &ItemModule,
    item_flammability: &[f32],
    item_radioactivity: &[f32],
) -> Option<f32> {
    match filter {
        GeneratorFilter::ItemFlammable { .. } => {
            filtered_item(filter, module, item_flammability, item_radioactivity)
                .map(|(_, value)| value)
        }
        GeneratorFilter::ItemRadioactive { .. } => {
            filtered_item(filter, module, item_flammability, item_radioactivity)
                .map(|(_, value)| value)
        }
        _ => None,
    }
}

/// Whether a block has generic consumers that `updateConsumption` executes.
fn has_consumers(world: &World, entity: Entity) -> bool {
    let Some(block) = world.get::<Building>(entity).map(|building| building.block) else {
        return false;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
        .map(|instance| !instance.consumers.is_empty())
        .unwrap_or(false)
}

/// Whether a `BlockTable` is available (i.e. the plan-07 runtime is installed).
fn has_table(world: &World) -> bool {
    world.get_resource::<BlockTable>().is_some()
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
/// `updateConsumption`'s filter efficiency pass.
///
/// `delta` is `Time.delta` (D8); the building `timeScale` is applied as in
/// `Building.delta()` where the Java code calls `delta()`.
pub fn update_generator(world: &mut World, entity: Entity, delta: f32, content: &ContentRegistry) {
    let item_flammability = item_flammabilities(content);
    let item_radioactivity = item_radioactivities(content);
    let liquid_flammability = liquid_flammabilities(content);
    update_generator_with(
        world,
        entity,
        delta,
        &item_flammability,
        &item_radioactivity,
        &liquid_flammability,
    );
}

/// [`update_generator`] with precomputed tables (behaviors avoid rebuilding them
/// per tick).
pub fn update_generator_with(
    world: &mut World,
    entity: Entity,
    delta: f32,
    item_flammability: &[f32],
    item_radioactivity: &[f32],
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

    // `efficiency` comes from `updateConsumption` when the block has generic
    // consumers (plan 02-lowered item/liquid stacks); otherwise there are no
    // consumers and the generator runs at the enabled gate (1.0). The filter
    // gate (`ConsumeItemFlammable` etc., not lowered by plan 02) is applied on
    // top here.
    let mut efficiency = if !enabled {
        0.0
    } else if has_consumers(world, entity) {
        world
            .get::<Building>(entity)
            .map(|building| building.efficiency)
            .unwrap_or(1.0)
    } else {
        1.0
    };

    if enabled {
        if let Some(filter) = config.filter_liquid.as_ref() {
            let gate = match liquid_module(world, entity) {
                Some(module) => liquid_gate(filter, module, liquid_flammability, delta),
                None => 0.0,
            };
            efficiency *= gate;
        }
        if let Some(filter) = config.filter_item.as_ref() {
            let gate = match item_module(world, entity) {
                Some(module) => item_gate(
                    filter,
                    module,
                    item_flammability,
                    item_radioactivity,
                    state.generate_time,
                ),
                None => {
                    if state.generate_time > 0.0 {
                        1.0
                    } else {
                        0.0
                    }
                }
            };
            efficiency *= gate;
        }
    }
    if let Some(mut building) = world.get_mut::<Building>(entity) {
        building.efficiency = efficiency;
    }

    let valid = efficiency > 0.0;

    // `updateEfficiencyMultiplier`: only overwrite when the new value is > 0.
    if let (Some(filter), Some(module)) = (config.filter_item.as_ref(), item_module(world, entity))
        && let Some(multiplier) =
            filter_multiplier(filter, module, item_flammability, item_radioactivity)
        && multiplier > 0.0
    {
        state.efficiency_multiplier = multiplier;
    } else if let Some(filter @ GeneratorFilter::LiquidFlammable { .. }) =
        config.filter_liquid.as_ref()
    {
        let multiplier = liquid_module(world, entity)
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
    if valid && state.generate_time <= 0.0 {
        let mut consumed = false;
        let filtered = match (config.filter_item.as_ref(), item_module(world, entity)) {
            (Some(filter), Some(module)) => {
                any_filtered_item(filter, module, item_flammability, item_radioactivity)
            }
            _ => None,
        };
        if let Some(item) = filtered {
            if let Some(mut module) = world.get_mut::<ItemModule>(entity) {
                module.remove(item, 1);
            }
            consumed = true;
        }
        if !config.trigger_items.is_empty()
            && let Some(mut module) = world.get_mut::<ItemModule>(entity)
        {
            for (item, amount) in &config.trigger_items {
                if module.get(*item) >= *amount {
                    module.remove(*item, *amount);
                    consumed = true;
                }
            }
        }
        if consumed {
            state.generate_time = 1.0;
        }
    }

    // Specific/filtered liquid consumer `update`: `amount * edelta`.
    if let Some(filter) = config.filter_liquid.as_ref() {
        drain_filter_liquid(
            world,
            entity,
            filter,
            efficiency * delta,
            liquid_flammability,
        );
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

fn item_module(world: &World, entity: Entity) -> Option<&ItemModule> {
    world.get::<ItemModule>(entity)
}

fn liquid_module(world: &World, entity: Entity) -> Option<&LiquidModule> {
    world.get::<LiquidModule>(entity)
}

/// `ConsumeLiquidFilter.update`/`ConsumeLiquid.update`: remove
/// `amount * edelta` of the consumed liquid(s).
fn drain_filter_liquid(
    world: &mut World,
    entity: Entity,
    filter: &GeneratorFilter,
    edelta: f32,
    liquid_flammability: &[f32],
) {
    let consumed: Vec<(LiquidId, f32)> = match filter {
        GeneratorFilter::LiquidFlammable { amount, .. } => {
            let module = liquid_module(world, entity);
            match module.and_then(|module| consumed_liquid(liquid_flammability, module, filter)) {
                Some(liquid) => vec![(liquid, *amount)],
                None => Vec::new(),
            }
        }
        GeneratorFilter::Liquid { liquid, amount } => vec![(*liquid, *amount)],
        GeneratorFilter::Liquids { stacks } => stacks.clone(),
        _ => Vec::new(),
    };
    if consumed.is_empty() {
        return;
    }
    if let Some(mut module) = world.get_mut::<LiquidModule>(entity) {
        for (liquid, amount) in consumed {
            module.remove(liquid, amount * edelta);
        }
    }
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

/// Ensures the shared generator state components exist (used by reactor
/// behaviors that are not `ConsumeGenerator`s).
pub fn ensure_generator_components(
    world: &mut World,
    entity: Entity,
    power_production: f32,
    item_duration: f32,
    warmup_speed: f32,
    liquid_capacity: f32,
) {
    if world.get::<GeneratorConfig>(entity).is_none() {
        world.entity_mut(entity).insert(GeneratorConfig {
            power_production,
            item_duration,
            warmup_speed,
            liquid_capacity,
            ..GeneratorConfig::default()
        });
    }
    if world.get::<GeneratorState>(entity).is_none() {
        world.entity_mut(entity).insert(GeneratorState::new());
    }
    if world.get::<PowerProduction>(entity).is_none() {
        world.entity_mut(entity).insert(PowerProduction(0.0));
    }
}

/// Writes `productionEfficiency`/`PowerProduction` for a `PowerGenerator`
/// subclass whose efficiency is computed directly (not via `ConsumeGenerator`).
pub fn set_direct_production(
    world: &mut World,
    entity: Entity,
    production_efficiency: f32,
    warmup: f32,
) {
    let power_production = world
        .get::<GeneratorConfig>(entity)
        .map(|config| config.power_production)
        .unwrap_or(0.0);
    let enabled = world
        .get::<Building>(entity)
        .map(|building| building.enabled)
        .unwrap_or(false);
    if let Some(mut state) = world.get_mut::<GeneratorState>(entity) {
        state.production_efficiency = production_efficiency;
        state.warmup = warmup;
    }
    let production = if enabled {
        power_production * production_efficiency
    } else {
        0.0
    };
    world.entity_mut(entity).insert(PowerProduction(production));
}

/// Reads `BlockDef.liquid_capacity` (0 if absent).
pub fn block_liquid_capacity(world: &World, entity: Entity) -> f32 {
    let Some(block) = world.get::<Building>(entity).map(|building| building.block) else {
        return 0.0;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
        .map(|instance| instance.def.liquid_capacity.max(0.0))
        .unwrap_or(0.0)
}

/// Reads `BlockDef.item_capacity` (0 if absent).
pub fn block_item_capacity(world: &World, entity: Entity) -> i32 {
    let Some(block) = world.get::<Building>(entity).map(|building| building.block) else {
        return 0;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
        .map(|instance| instance.def.item_capacity.max(0))
        .unwrap_or(0)
}

/// Whether the plan-07 runtime table is installed (behavior seam; plan 12 fills
/// environment rules in that case).
pub fn runtime_installed(world: &World) -> bool {
    has_table(world)
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
