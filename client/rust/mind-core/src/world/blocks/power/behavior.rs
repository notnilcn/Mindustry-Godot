// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Generator/reactor behaviors as plan-07 [`BuildingBehavior`]s (plan 09 R3;
//! `world/blocks/power/{ConsumeGenerator,PowerGenerator,ThermalGenerator,
//! SolarGenerator,ImpactReactor,NuclearReactor,VariableReactor,
//! HeaterGenerator}.java`).
//!
//! Plan 02's `BlockDef` carries no generator knobs, so each behavior owns a
//! [`GeneratorConfig`] template per vanilla block and precomputes the
//! content-ordered item/liquid tables once at registry build time.
//!
//! Consumer semantics (plan 02 §6.1):
//! * `ConsumeItemFlammable`/`ConsumeItemRadioactive`/`ConsumeLiquidFlammable`
//!   are **not** lowered by plan 02, so [`ConsumeGeneratorBehavior`] supplies
//!   their gate + `efficiencyMultiplier` itself (via [`GeneratorFilter`]).
//! * Generic `consumeItem(s)`/`consumeLiquid(s)` **are** lowered and executed
//!   by plan 07's `updateConsumption`; the behavior only pulls the periodic
//!   `ConsumeItems.trigger` (via `GeneratorConfig.trigger_items`) and the
//!   `ConsumeGenerator` output/liquid state.
//!
//! Environment rules (`Rules.solarMultiplier`/`lighting`/`ambientLight`/`env`)
//! are plan 12's; [`GeneratorEnv`] is the plan-09 seam with upstream defaults
//! (solar multiplier 1, no lighting darkening, no env heat) that plan 12 fills.

use std::sync::Arc;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::content::{ContentRegistry, LiquidId};
use crate::entities::comp::Building;
use crate::world::behavior::{BehaviorRegistry, BuildingBehavior};
use crate::world::blocks::heat::HeatState;
use crate::world::blocks::power::PowerProduction;
use crate::world::blocks::power::generator::{
    GeneratorConfig, GeneratorFilter, GeneratorState, item_flammabilities, item_radioactivities,
    liquid_flammabilities, set_direct_production, update_generator_with,
};
use crate::world::blocks::power::reactors::{
    approach_delta, impact_production_efficiency, impact_warmup_step, solar_production_efficiency,
    variable_reactor_should_kill, variable_reactor_step,
};
use crate::world::modules::{ItemModule, LiquidModule, PowerModule};
use crate::world::update::{build_time, delta};

/// Plan-12 environment/rules seam for generators (upstream `Rules` +
/// `Attribute.env()`); defaults match a lit, non-space, no-heat map.
#[derive(Debug, Clone, Copy, PartialEq, Resource)]
pub struct GeneratorEnv {
    /// `Rules.solarMultiplier`.
    pub solar_multiplier: f32,
    /// `Rules.lighting` (night darkening active).
    pub lighting: bool,
    /// `Rules.ambientLight.a`.
    pub ambient_light_alpha: f32,
    /// `Attribute.light.env()`.
    pub light_env: f32,
    /// `Attribute.heat.env()`.
    pub heat_env: f32,
    /// `Rules.reactorExplosions`.
    pub reactor_explosions: bool,
}

impl Default for GeneratorEnv {
    fn default() -> Self {
        Self {
            solar_multiplier: 1.0,
            lighting: false,
            ambient_light_alpha: 0.0,
            light_env: 0.0,
            heat_env: 0.0,
            reactor_explosions: true,
        }
    }
}

fn generator_env(world: &World) -> GeneratorEnv {
    world
        .get_resource::<GeneratorEnv>()
        .copied()
        .unwrap_or_default()
}

fn ensure_modules(world: &mut World, e: Entity, items: usize, liquids: usize) {
    if world.get::<ItemModule>(e).is_none() {
        world.entity_mut(e).insert(ItemModule::with_items(items));
    }
    if world.get::<LiquidModule>(e).is_none() {
        world
            .entity_mut(e)
            .insert(LiquidModule::with_liquids(liquids));
    }
}

fn ensure_core(
    world: &mut World,
    e: Entity,
    config: &GeneratorConfig,
    items: usize,
    liquids: usize,
) {
    if world.get::<GeneratorConfig>(e).is_none() {
        world.entity_mut(e).insert(config.clone());
    }
    if world.get::<GeneratorState>(e).is_none() {
        world.entity_mut(e).insert(GeneratorState::new());
    }
    if world.get::<PowerProduction>(e).is_none() {
        world.entity_mut(e).insert(PowerProduction(0.0));
    }
    ensure_modules(world, e, items, liquids);
}

/// `ConsumeGenerator` behavior (and `HeaterGenerator`'s generator half).
#[derive(Clone)]
pub struct ConsumeGeneratorBehavior {
    /// Per-block generator knobs.
    pub config: GeneratorConfig,
    /// Item flammability indexed by `ItemId`.
    pub item_flammability: Arc<[f32]>,
    /// Item radioactivity indexed by `ItemId`.
    pub item_radioactivity: Arc<[f32]>,
    /// Liquid flammability indexed by `LiquidId`.
    pub liquid_flammability: Arc<[f32]>,
}

impl BuildingBehavior for ConsumeGeneratorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        ensure_core(
            world,
            e,
            &self.config,
            self.item_flammability.len(),
            self.liquid_flammability.len(),
        );
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let delta = delta(world, e);
        update_generator_with(
            world,
            e,
            delta,
            &self.item_flammability,
            &self.item_radioactivity,
            &self.liquid_flammability,
        );
    }
}

/// `HeaterGenerator` behavior: `ConsumeGenerator` plus the heat ramp
/// (`HeaterGeneratorBuild.updateTile`).
#[derive(Clone)]
pub struct HeaterGeneratorBehavior {
    /// `ConsumeGenerator` half.
    pub generator: ConsumeGeneratorBehavior,
    /// `HeaterGenerator.heatOutput`.
    pub heat_output: f32,
    /// `HeaterGenerator.warmupRate`.
    pub warmup_rate: f32,
}

impl BuildingBehavior for HeaterGeneratorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        self.generator.create_state(world, e);
        if world.get::<HeatState>(e).is_none() {
            world.entity_mut(e).insert(HeatState {
                heat_output: self.heat_output,
                rotate: true,
                ..Default::default()
            });
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        self.generator.update_tile(world, e);
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        let delta = delta(world, e);
        if let Some(mut state) = world.get_mut::<HeatState>(e) {
            state.heat = approach_delta(
                state.heat,
                self.heat_output * efficiency,
                self.warmup_rate,
                delta,
            );
        }
    }
}

/// `ThermalGenerator` behavior (`ThermalGeneratorBuild.updateTile`).
///
/// `sum` is computed by [`refresh_thermal_sum`] (floor attribute sum, plan 06)
/// and stored on [`ThermalState`]; the env half comes from [`GeneratorEnv`].
#[derive(Clone)]
pub struct ThermalGeneratorBehavior {
    /// Per-block generator knobs (`power_production`/`liquid_capacity`).
    pub config: GeneratorConfig,
    /// `ThermalGenerator.outputLiquid`.
    pub output_liquid: Option<(LiquidId, f32)>,
    /// Number of liquid slots for the module.
    pub liquid_count: usize,
}

/// `ThermalGeneratorBuild.sum` (floor heat attribute sum).
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct ThermalState {
    /// `sumAttribute(attribute, x, y)`.
    pub sum: f32,
}

impl BuildingBehavior for ThermalGeneratorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        ensure_core(
            world,
            e,
            &self.config,
            self.config.trigger_items.len(),
            self.liquid_count,
        );
        if world.get::<ThermalState>(e).is_none() {
            world.entity_mut(e).insert(ThermalState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let env = generator_env(world);
        let sum = world
            .get::<ThermalState>(e)
            .map(|state| state.sum)
            .unwrap_or(0.0);
        let production_efficiency = sum + env.heat_env;
        set_direct_production(world, e, production_efficiency, production_efficiency);
        if let Some((liquid, amount)) = self.output_liquid {
            let delta = delta(world, e);
            let capacity = self.config.liquid_capacity;
            let added = (production_efficiency * delta * amount).min(
                capacity
                    - world
                        .get::<LiquidModule>(e)
                        .map(|module| module.get(liquid))
                        .unwrap_or(0.0),
            );
            if let Some(mut module) = world.get_mut::<LiquidModule>(e) {
                module.add(liquid, added.max(0.0), capacity);
            }
        }
    }
}

/// `SolarGenerator` behavior (`SolarGeneratorBuild.updateTile`).
#[derive(Clone)]
pub struct SolarGeneratorBehavior {
    /// Per-block generator knobs.
    pub config: GeneratorConfig,
}

impl BuildingBehavior for SolarGeneratorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        ensure_core(world, e, &self.config, 0, 0);
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let env = generator_env(world);
        let enabled = world
            .get::<Building>(e)
            .map(|building| building.enabled)
            .unwrap_or(false);
        let production_efficiency = solar_production_efficiency(
            enabled,
            env.solar_multiplier,
            env.light_env,
            env.lighting,
            env.ambient_light_alpha,
        );
        set_direct_production(world, e, production_efficiency, production_efficiency);
    }
}

/// `ImpactReactorBuild` state.
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct ImpactReactorState {
    /// `ImpactReactorBuild.warmup`.
    pub warmup: f32,
    /// `ImpactReactorBuild.totalProgress`.
    pub total_progress: f32,
}

/// `ImpactReactor` behavior.
#[derive(Clone)]
pub struct ImpactReactorBehavior {
    /// Per-block generator knobs.
    pub config: GeneratorConfig,
    /// Item slots.
    pub item_count: usize,
    /// Liquid slots.
    pub liquid_count: usize,
}

impl BuildingBehavior for ImpactReactorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        ensure_core(world, e, &self.config, self.item_count, self.liquid_count);
        if world.get::<ImpactReactorState>(e).is_none() {
            world.entity_mut(e).insert(ImpactReactorState::default());
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let delta = delta(world, e);
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        let power_status = world
            .get::<PowerModule>(e)
            .map(|module| module.status)
            .unwrap_or(0.0);
        let time_scale = delta;
        let mut state = world
            .get::<ImpactReactorState>(e)
            .copied()
            .unwrap_or_default();
        let warmup = impact_warmup_step(
            state.warmup,
            efficiency,
            power_status,
            self.config.warmup_speed,
            time_scale,
            delta,
        );
        let production_efficiency = impact_production_efficiency(warmup);
        state.warmup = warmup;
        state.total_progress += warmup * delta;

        // `timer(timerUse, itemDuration / timeScale)` -> `generateTime`.
        let mut generate_time = world
            .get::<GeneratorState>(e)
            .map(|generator| generator.generate_time)
            .unwrap_or(0.0);
        if warmup > 0.0 && self.config.item_duration > 0.0 {
            generate_time -= delta / self.config.item_duration;
            if generate_time <= 0.0 {
                if let Some(mut module) = world.get_mut::<ItemModule>(e) {
                    for (item, amount) in &self.config.trigger_items {
                        if module.get(*item) >= *amount {
                            module.remove(*item, *amount);
                        }
                    }
                }
                generate_time = 1.0;
            }
        }
        if let Some(mut generator) = world.get_mut::<GeneratorState>(e) {
            generator.generate_time = generate_time;
        }
        world.entity_mut(e).insert(state);
        set_direct_production(world, e, production_efficiency, warmup);
    }

    fn on_destroyed(&self, world: &mut World, e: Entity) {
        let env = generator_env(world);
        if env.reactor_explosions {
            let warmup = world
                .get::<ImpactReactorState>(e)
                .map(|state| state.warmup)
                .unwrap_or(0.0);
            let _ = warmup >= self.config.explosion_min_warmup;
        }
    }
}

/// `NuclearReactorBuild` state.
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct NuclearReactorState {
    /// `NuclearReactorBuild.heat`.
    pub heat: f32,
    /// `NuclearReactorBuild.heatLastFrame`.
    pub heat_last_frame: f32,
    /// `NuclearReactorBuild.heatProgress`.
    pub heat_progress: f32,
}

/// `NuclearReactor` behavior (`thorium-reactor`).
#[derive(Clone)]
pub struct NuclearReactorBehavior {
    /// Per-block generator knobs (`item_duration` = fuel duration).
    pub config: GeneratorConfig,
    /// `NuclearReactor.heating`.
    pub heating: f32,
    /// `NuclearReactor.heatOutput`.
    pub heat_output: f32,
    /// `NuclearReactor.heatWarmupRate`.
    pub heat_warmup_rate: f32,
    /// `NuclearReactor.heatConsumeRate`.
    pub heat_consume_rate: f32,
    /// `NuclearReactor.ambientCooldownTime`.
    pub ambient_cooldown_time: f32,
    /// `NuclearReactor.coolantPower`.
    pub coolant_power: f32,
    /// `NuclearReactor.itemCapacity`.
    pub item_capacity: i32,
    /// Item slots.
    pub item_count: usize,
    /// Liquid slots.
    pub liquid_count: usize,
}

impl BuildingBehavior for NuclearReactorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        ensure_core(world, e, &self.config, self.item_count, self.liquid_count);
        if world.get::<NuclearReactorState>(e).is_none() {
            world.entity_mut(e).insert(NuclearReactorState::default());
        }
        if world.get::<HeatState>(e).is_none() {
            world.entity_mut(e).insert(HeatState {
                heat_output: self.heat_output,
                rotate: false,
                ..Default::default()
            });
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let delta = delta(world, e);
        let enabled = world
            .get::<Building>(e)
            .map(|building| building.enabled)
            .unwrap_or(false);
        let fuel = self
            .config
            .trigger_items
            .first()
            .map(|(item, _)| {
                world
                    .get::<ItemModule>(e)
                    .map(|module| module.get(*item))
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let fullness = if self.item_capacity > 0 {
            fuel as f32 / self.item_capacity as f32
        } else {
            0.0
        };
        let mut state = world
            .get::<NuclearReactorState>(e)
            .copied()
            .unwrap_or_default();

        if fuel > 0 && enabled {
            state.heat_last_frame = fullness * self.heating * delta.min(4.0);
            state.heat += state.heat_last_frame;
            // `timer(timerFuel, itemDuration / (timeScale + heat * heatConsumeRate))`.
            let mut generate_time = world
                .get::<GeneratorState>(e)
                .map(|generator| generator.generate_time)
                .unwrap_or(0.0);
            let rate = if delta + state.heat * self.heat_consume_rate > 0.0 {
                delta + state.heat * self.heat_consume_rate
            } else {
                delta.max(f32::EPSILON)
            };
            if self.config.item_duration > 0.0 {
                generate_time -= rate / self.config.item_duration;
                if generate_time <= 0.0 {
                    if let Some(mut module) = world.get_mut::<ItemModule>(e)
                        && let Some((item, amount)) = self.config.trigger_items.first()
                        && module.get(*item) >= *amount
                    {
                        module.remove(*item, *amount);
                    }
                    generate_time = 1.0;
                }
            }
            if let Some(mut generator) = world.get_mut::<GeneratorState>(e) {
                generator.generate_time = generate_time;
            }
        } else {
            state.heat =
                (state.heat - delta / self.ambient_cooldown_time.max(f32::EPSILON)).max(0.0);
        }

        // Coolant removal: `maxUsed = min(currentAmount, heat / coolantPower)`.
        if state.heat > 0.0 && self.coolant_power > 0.0 {
            let (current, liquid) = world
                .get::<LiquidModule>(e)
                .and_then(|module| {
                    module
                        .liquids
                        .iter()
                        .enumerate()
                        .find(|(_, amount)| **amount > 0.0)
                        .map(|(index, amount)| (*amount, LiquidId::new(index as u16)))
                })
                .unwrap_or((0.0, LiquidId::WATER));
            let max_used = current.min(state.heat / self.coolant_power);
            state.heat -= max_used * self.coolant_power;
            if max_used > 0.0
                && let Some(mut module) = world.get_mut::<LiquidModule>(e)
            {
                module.remove(liquid, max_used);
            }
        }

        state.heat = state.heat.clamp(0.0, 1.0);
        let active = if enabled && fullness > 0.0 { 1.0 } else { 0.0 };
        state.heat_progress = if self.heat_output > 0.0 {
            approach_delta(
                state.heat_progress,
                state.heat * self.heat_output * active,
                self.heat_warmup_rate,
                delta,
            )
        } else {
            0.0
        };

        if let Some(mut heat) = world.get_mut::<HeatState>(e) {
            heat.heat = state.heat_progress;
        }
        let explode = state.heat >= 0.999;
        world.entity_mut(e).insert(state);
        set_direct_production(world, e, fullness, fullness);
        if explode {
            crate::world::blocks::power::sandbox::kill_building(world, e);
        }
    }
}

/// `VariableReactorBuild` state (`flux-reactor`).
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct VariableReactorState {
    /// `VariableReactorBuild.heat`.
    pub heat: f32,
    /// `VariableReactorBuild.instability`.
    pub instability: f32,
    /// `VariableReactorBuild.warmup`.
    pub warmup: f32,
    /// `VariableReactorBuild.totalProgress`.
    pub total_progress: f32,
}

/// `VariableReactor` behavior (`flux-reactor`).
#[derive(Clone)]
pub struct VariableReactorBehavior {
    /// Per-block generator knobs.
    pub config: GeneratorConfig,
    /// `VariableReactor.maxHeat`.
    pub max_heat: f32,
    /// `VariableReactor.unstableSpeed`.
    pub unstable_speed: f32,
    /// `VariableReactor.warmupSpeed`.
    pub warmup_speed: f32,
    /// Liquid slots.
    pub liquid_count: usize,
}

impl BuildingBehavior for VariableReactorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        ensure_core(world, e, &self.config, 0, self.liquid_count);
        if world.get::<VariableReactorState>(e).is_none() {
            world.entity_mut(e).insert(VariableReactorState::default());
        }
    }

    /// `VariableReactorBuild.updateEfficiencyMultiplier`: scale efficiency by
    /// `clamp(heat / maxHeat)` (the `HeatConsumer.heatRequirement` target).
    fn efficiency_scale(&self, world: &mut World, e: Entity) -> f32 {
        let update_id = build_time(world) as u64;
        let heat = crate::world::blocks::heat::crafter_heat(world, e, update_id);
        if let Some(mut state) = world.get_mut::<VariableReactorState>(e) {
            state.heat = heat;
        }
        if self.max_heat > 0.0 {
            (heat / self.max_heat).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let delta = delta(world, e);
        // `building.efficiency` is already `coolant * target` (see
        // `efficiency_scale`); recover the coolant fraction for `efficiencyMet`.
        let scaled = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        let mut state = world
            .get::<VariableReactorState>(e)
            .copied()
            .unwrap_or_default();
        let target = if self.max_heat > 0.0 {
            (state.heat / self.max_heat).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let coolant = if target > 0.0 { scaled / target } else { 1.0 };
        let (instability, _) = variable_reactor_step(
            state.instability,
            coolant,
            state.heat,
            self.max_heat,
            self.unstable_speed,
            delta,
        );
        state.instability = instability;
        state.warmup = if scaled > 0.0 {
            state.warmup + (1.0 - state.warmup) * (self.warmup_speed * delta).min(1.0)
        } else {
            state.warmup - state.warmup * (self.warmup_speed * delta).min(1.0)
        };
        state.total_progress += scaled * delta;
        let kill = variable_reactor_should_kill(instability);
        world.entity_mut(e).insert(state);
        set_direct_production(world, e, scaled, state.warmup);
        if kill {
            crate::world::blocks::power::sandbox::kill_building(world, e);
        }
    }

    fn on_destroyed(&self, world: &mut World, e: Entity) {
        let env = generator_env(world);
        let heat = world
            .get::<VariableReactorState>(e)
            .map(|state| state.heat)
            .unwrap_or(0.0);
        let _ = env.reactor_explosions && heat > 0.0;
    }
}

/// Grid-taking `sumAttribute(Attribute.heat, x, y)`: sums the heat attribute of
/// all tiles in the block footprint (plan 06 `WorldGrid`).
pub fn refresh_thermal_sum(
    world: &mut World,
    grid: &crate::world::WorldGrid,
    content: &ContentRegistry,
    e: Entity,
) {
    let Some(block) = world.get::<Building>(e).map(|building| building.block) else {
        return;
    };
    let Some(instance) = world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.instance(block))
    else {
        return;
    };
    let tile = world.get::<Building>(e).map(|building| building.tile);
    let Some(tile) = tile else { return };
    let size = instance.def.size.max(1);
    let mut sum = 0.0f32;
    for dx in 0..size {
        for dy in 0..size {
            let (tx, ty) = (tile.x() as i32 + dx, tile.y() as i32 + dy);
            if !grid.tiles.in_bounds(tx, ty) {
                continue;
            }
            let floor = grid.tile(tx, ty).floor;
            if let Some(def) = content.block(floor) {
                for (name, value) in &def.attributes {
                    if name == "heat" {
                        sum += value;
                    }
                }
            }
        }
    }
    if let Some(mut state) = world.get_mut::<ThermalState>(e) {
        state.sum = sum;
    }
}

/// Registers the vanilla generator/reactor blocks with exact knobs.
pub fn register(registry: &mut BehaviorRegistry, content: &ContentRegistry) {
    let item_flammability: Arc<[f32]> = item_flammabilities(content).into();
    let item_radioactivity: Arc<[f32]> = item_radioactivities(content).into();
    let liquid_flammability: Arc<[f32]> = liquid_flammabilities(content).into();
    let item_count = content.items().len();
    let liquid_count = content.liquids().len();

    let generator = |config: GeneratorConfig| ConsumeGeneratorBehavior {
        config,
        item_flammability: item_flammability.clone(),
        item_radioactivity: item_radioactivity.clone(),
        liquid_flammability: liquid_flammability.clone(),
    };

    let flammable = GeneratorFilter::ItemFlammable { min: 0.2 };

    // `combustion-generator`.
    let combustion = generator(GeneratorConfig {
        power_production: 1.0,
        item_duration: 120.0,
        filter_item: Some(flammable.clone()),
        ..GeneratorConfig::default()
    });
    registry.register_named("combustion-generator", Arc::new(combustion));

    // `steam-generator` (generic water consumer is lowered by plan 02).
    let steam = generator(GeneratorConfig {
        power_production: 5.5,
        item_duration: 90.0,
        filter_item: Some(flammable.clone()),
        liquid_capacity: content
            .block_id("steam-generator")
            .and_then(|id| content.block(id))
            .map(|def| def.liquid_capacity)
            .unwrap_or(0.0),
        ..GeneratorConfig::default()
    });
    registry.register_named("steam-generator", Arc::new(steam));

    // `differential-generator`: generic pyratite + cryofluid consumers.
    let differential = generator(GeneratorConfig {
        power_production: 18.0,
        item_duration: 220.0,
        trigger_items: vec![(content.item_id("pyratite").unwrap_or_default(), 1)],
        ..GeneratorConfig::default()
    });
    registry.register_named("differential-generator", Arc::new(differential));

    // `rtg-generator`: `ConsumeItemRadioactive`.
    let rtg = generator(GeneratorConfig {
        power_production: 4.5,
        item_duration: 840.0,
        filter_item: Some(GeneratorFilter::ItemRadioactive { min: 0.2 }),
        ..GeneratorConfig::default()
    });
    registry.register_named("rtg-generator", Arc::new(rtg));

    // `chemical-combustion-chamber`: `consumeLiquids(ozone, arkycite)` (plan 02
    // lowers an empty `ConsumeLiquids`, so the behavior owns the stacks).
    let chemical = generator(GeneratorConfig {
        power_production: 550.0 / 60.0,
        item_duration: 120.0,
        liquid_capacity: 100.0,
        filter_liquid: Some(GeneratorFilter::Liquids {
            stacks: vec![
                (content.liquid_id("ozone").unwrap_or_default(), 2.0 / 60.0),
                (
                    content.liquid_id("arkycite").unwrap_or_default(),
                    40.0 / 60.0,
                ),
            ],
        }),
        ..GeneratorConfig::default()
    });
    registry.register_named("chemical-combustion-chamber", Arc::new(chemical));

    // `pyrolysis-generator`: slag + arkycite -> water.
    let pyrolysis = generator(GeneratorConfig {
        power_production: 1400.0 / 60.0,
        item_duration: 120.0,
        liquid_capacity: 150.0,
        output_liquid: Some((content.liquid_id("water").unwrap_or_default(), 20.0 / 60.0)),
        filter_liquid: Some(GeneratorFilter::Liquids {
            stacks: vec![
                (content.liquid_id("slag").unwrap_or_default(), 20.0 / 60.0),
                (
                    content.liquid_id("arkycite").unwrap_or_default(),
                    40.0 / 60.0,
                ),
            ],
        }),
        ..GeneratorConfig::default()
    });
    registry.register_named("pyrolysis-generator", Arc::new(pyrolysis));

    // `neoplasia-reactor` (`HeaterGenerator`): arkycite + water + phase-fabric ->
    // neoplasm (generic consumers) with a heat ramp.
    let neoplasia = HeaterGeneratorBehavior {
        generator: generator(GeneratorConfig {
            power_production: 140.0,
            item_duration: 180.0,
            output_liquid: Some((
                content.liquid_id("neoplasm").unwrap_or_default(),
                20.0 / 60.0,
            )),
            explode_on_full: true,
            trigger_items: vec![(content.item_id("phase-fabric").unwrap_or_default(), 1)],
            liquid_capacity: 80.0,
            explosion_min_warmup: 0.5,
            ..GeneratorConfig::default()
        }),
        heat_output: 60.0,
        warmup_rate: 0.15,
    };
    registry.register_named("neoplasia-reactor", Arc::new(neoplasia));

    // `thermal-generator`.
    let thermal = ThermalGeneratorBehavior {
        config: GeneratorConfig {
            power_production: 1.8,
            ..GeneratorConfig::default()
        },
        output_liquid: None,
        liquid_count,
    };
    registry.register_named("thermal-generator", Arc::new(thermal));

    // `turbine-condenser` (`ThermalGenerator`, steam attribute; plan-02 has no
    // steam attribute, so this uses `heat` — recorded seam).
    let turbine = ThermalGeneratorBehavior {
        config: GeneratorConfig {
            power_production: 3.0 / 9.0,
            liquid_capacity: 20.0,
            ..GeneratorConfig::default()
        },
        output_liquid: Some((LiquidId::WATER, 5.0 / 60.0 / 9.0)),
        liquid_count,
    };
    registry.register_named("turbine-condenser", Arc::new(turbine));

    for (name, power) in [("solar-panel", 0.12f32), ("solar-panel-large", 1.6)] {
        registry.register_named(
            name,
            Arc::new(SolarGeneratorBehavior {
                config: GeneratorConfig {
                    power_production: power,
                    ..GeneratorConfig::default()
                },
            }),
        );
    }

    // `thorium-reactor` (`NuclearReactor`).
    registry.register_named(
        "thorium-reactor",
        Arc::new(NuclearReactorBehavior {
            config: GeneratorConfig {
                power_production: 15.0,
                item_duration: 360.0,
                trigger_items: vec![(content.item_id("thorium").unwrap_or_default(), 1)],
                liquid_capacity: 30.0,
                explosion_min_warmup: 0.0,
                ..GeneratorConfig::default()
            },
            heating: 0.005,
            heat_output: 8.0,
            heat_warmup_rate: 1.0,
            heat_consume_rate: 10.0,
            ambient_cooldown_time: 60.0 * 20.0,
            coolant_power: 0.125,
            item_capacity: 30,
            item_count,
            liquid_count,
        }),
    );

    // `impact-reactor`.
    registry.register_named(
        "impact-reactor",
        Arc::new(ImpactReactorBehavior {
            config: GeneratorConfig {
                power_production: 130.0,
                item_duration: 140.0,
                warmup_speed: 0.001,
                trigger_items: vec![(content.item_id("blast-compound").unwrap_or_default(), 1)],
                liquid_capacity: 80.0,
                explosion_min_warmup: 0.3,
                ..GeneratorConfig::default()
            },
            item_count,
            liquid_count,
        }),
    );

    // `flux-reactor` (`VariableReactor`).
    registry.register_named(
        "flux-reactor",
        Arc::new(VariableReactorBehavior {
            config: GeneratorConfig {
                power_production: 18000.0 / 60.0,
                liquid_capacity: 30.0,
                explosion_min_warmup: 0.5,
                ..GeneratorConfig::default()
            },
            max_heat: 150.0,
            unstable_speed: 1.0 / 60.0 / 3.0,
            warmup_speed: 0.1,
            liquid_count,
        }),
    );
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
            let mut items = harness.world.get_mut::<ItemModule>(entity).expect("items");
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
        assert!(production > 0.0, "production={production}");
    }

    #[test]
    fn rtg_generator_runs_on_thorium() {
        let mut harness = BuildHarness::new(8, 8, 7);
        let block = harness
            .content()
            .block_id("rtg-generator")
            .expect("rtg-generator");
        let thorium = harness.content().item_id("thorium").expect("thorium");
        assert!(harness.place(2, 2, block, 0, true));
        let entity = harness.build_at(2, 2).expect("generator");
        {
            let mut items = harness.world.get_mut::<ItemModule>(entity).expect("items");
            items.add(thorium, 5, 100);
        }
        for _ in 0..120 {
            harness.tick();
        }
        let production = harness
            .world
            .get::<PowerProduction>(entity)
            .map(|production| production.0)
            .unwrap_or(0.0);
        assert!(production > 0.0, "production={production}");
    }

    #[test]
    fn solar_panel_produces_in_light() {
        let mut harness = BuildHarness::new(8, 8, 7);
        harness.world.insert_resource(GeneratorEnv::default());
        let block = harness
            .content()
            .block_id("solar-panel")
            .expect("solar-panel");
        assert!(harness.place(2, 2, block, 0, true));
        let entity = harness.build_at(2, 2).expect("solar");
        harness.tick();
        let production = harness
            .world
            .get::<PowerProduction>(entity)
            .map(|production| production.0)
            .unwrap_or(0.0);
        assert!((production - 0.12).abs() < 1e-5, "production={production}");
    }

    #[test]
    fn differential_generator_runs_on_pyratite_and_cryo() {
        let mut harness = BuildHarness::new(8, 8, 7);
        let block = harness
            .content()
            .block_id("differential-generator")
            .expect("differential-generator");
        let pyratite = harness.content().item_id("pyratite").expect("pyratite");
        let cryo = harness.content().liquid_id("cryofluid").expect("cryofluid");
        assert!(harness.place(2, 2, block, 0, true));
        let entity = harness.build_at(2, 2).expect("generator");
        {
            let mut items = harness.world.get_mut::<ItemModule>(entity).expect("items");
            items.add(pyratite, 5, 100);
        }
        {
            let mut liquids = harness
                .world
                .get_mut::<LiquidModule>(entity)
                .expect("liquids");
            liquids.add(cryo, 1000.0, 1000.0);
        }
        for _ in 0..120 {
            harness.tick();
        }
        let production = harness
            .world
            .get::<PowerProduction>(entity)
            .map(|production| production.0)
            .unwrap_or(0.0);
        assert!(production > 0.0, "production={production}");
    }

    #[test]
    fn chemical_combustion_chamber_consumes_liquids() {
        let mut harness = BuildHarness::new(8, 8, 7);
        let block = harness
            .content()
            .block_id("chemical-combustion-chamber")
            .expect("chemical-combustion-chamber");
        let ozone = harness.content().liquid_id("ozone").expect("ozone");
        let arkycite = harness.content().liquid_id("arkycite").expect("arkycite");
        assert!(harness.place(2, 2, block, 0, true));
        let entity = harness.build_at(2, 2).expect("generator");
        {
            let mut liquids = harness
                .world
                .get_mut::<LiquidModule>(entity)
                .expect("liquids");
            liquids.add(ozone, 50.0, 100.0);
            liquids.add(arkycite, 50.0, 100.0);
        }
        for _ in 0..60 {
            harness.tick();
        }
        let production = harness
            .world
            .get::<PowerProduction>(entity)
            .map(|production| production.0)
            .unwrap_or(0.0);
        let remaining = harness
            .world
            .get::<LiquidModule>(entity)
            .map(|module| module.get(arkycite))
            .unwrap_or(0.0);
        assert!(production > 0.0, "production={production}");
        assert!(remaining < 50.0, "arkycite not consumed: {remaining}");
    }

    #[test]
    fn impact_reactor_ramps_under_power_and_fuel() {
        let mut harness = BuildHarness::new(8, 8, 7);
        let block = harness
            .content()
            .block_id("impact-reactor")
            .expect("impact-reactor");
        let blast = harness
            .content()
            .item_id("blast-compound")
            .expect("blast-compound");
        let cryo = harness.content().liquid_id("cryofluid").expect("cryofluid");
        assert!(harness.place(2, 2, block, 0, true));
        let entity = harness.build_at(2, 2).expect("reactor");
        {
            let mut items = harness.world.get_mut::<ItemModule>(entity).expect("items");
            items.add(blast, 10, 10);
        }
        {
            let mut liquids = harness
                .world
                .get_mut::<LiquidModule>(entity)
                .expect("liquids");
            liquids.add(cryo, 80.0, 80.0);
        }
        for _ in 0..1200 {
            if let Some(mut power) = harness.world.get_mut::<PowerModule>(entity) {
                power.status = 1.0;
            }
            harness.tick();
        }
        let warmup = harness
            .world
            .get::<ImpactReactorState>(entity)
            .map(|state| state.warmup)
            .unwrap_or(0.0);
        assert!(warmup > 0.0, "warmup={warmup}");
    }

    #[test]
    fn neoplasia_reactor_heats_and_generates() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let block = harness
            .content()
            .block_id("neoplasia-reactor")
            .expect("neoplasia-reactor");
        let arkycite = harness.content().liquid_id("arkycite").expect("arkycite");
        let water = harness.content().liquid_id("water").expect("water");
        let phase = harness
            .content()
            .item_id("phase-fabric")
            .expect("phase-fabric");
        assert!(harness.place(2, 2, block, 0, true));
        let entity = harness.build_at(2, 2).expect("reactor");
        {
            let mut items = harness.world.get_mut::<ItemModule>(entity).expect("items");
            items.add(phase, 10, 10);
        }
        {
            let mut liquids = harness
                .world
                .get_mut::<LiquidModule>(entity)
                .expect("liquids");
            liquids.add(arkycite, 80.0, 80.0);
            liquids.add(water, 80.0, 80.0);
        }
        for _ in 0..30 {
            harness.tick();
        }
        let heat = harness
            .world
            .get::<HeatState>(entity)
            .map(|state| state.heat)
            .unwrap_or(0.0);
        let production = harness
            .world
            .get::<PowerProduction>(entity)
            .map(|production| production.0)
            .unwrap_or(0.0);
        let efficiency = harness
            .world
            .get::<Building>(entity)
            .map(|building| building.efficiency)
            .unwrap_or(-1.0);
        let arkycite_left = harness
            .world
            .get::<LiquidModule>(entity)
            .map(|module| module.get(arkycite))
            .unwrap_or(-1.0);
        assert!(
            heat > 0.0,
            "heat={heat} production={production} efficiency={efficiency} arkycite={arkycite_left}"
        );
        assert!(production > 0.0, "production={production}");
    }

    #[test]
    fn nuclear_reactor_heats_and_outputs() {
        let mut harness = BuildHarness::new(8, 8, 7);
        let block = harness
            .content()
            .block_id("thorium-reactor")
            .expect("thorium-reactor");
        let thorium = harness.content().item_id("thorium").expect("thorium");
        assert!(harness.place(2, 2, block, 0, true));
        let entity = harness.build_at(2, 2).expect("reactor");
        {
            let mut items = harness.world.get_mut::<ItemModule>(entity).expect("items");
            items.add(thorium, 30, 30);
        }
        // No coolant: heat must build (with cryofluid present, upstream removes
        // heat faster than it accrues).
        for _ in 0..60 {
            harness.tick();
        }
        let production = harness
            .world
            .get::<PowerProduction>(entity)
            .map(|production| production.0)
            .unwrap_or(0.0);
        assert!(production > 0.0, "production={production}");
        let heat = harness
            .world
            .get::<NuclearReactorState>(entity)
            .map(|state| state.heat)
            .unwrap_or(0.0);
        assert!(heat > 0.0, "heat={heat}");
    }
}
