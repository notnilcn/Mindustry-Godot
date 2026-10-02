// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `HeatProducer`/`HeatCrafter`/`HeatConductor` as plan-07 [`BuildingBehavior`]s
//! (plan 09 R3; `world/blocks/heat/{HeatProducer,HeatConductor}.java`,
//! `world/blocks/production/HeatCrafter.java`).
//!
//! The heat math lives in [`super`] ([`calculate_heat`]/[`crafter_efficiency_scale`]/
//! [`heat_producer_step`]); these wrappers wire it into the per-block update
//! schedule and own the vanilla knob table (`BlockDef` carries no heat fields —
//! same seam as [`crate::world::blocks::power::generator`]).
//!
//! `HeatProducer`/`HeatCrafter` extend `GenericCrafter`, so the craft loop is
//! delegated to plan 07's [`production::CrafterBehavior`]; only the heat
//! ramp/pull and the overheat `efficiencyScale` are added here.

use std::sync::Arc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::ContentRegistry;
use crate::entities::comp::Building;
use crate::util::IdSet;
use crate::world::behavior::{BehaviorRegistry, BuildingBehavior};
use crate::world::update::build_time;

use super::{
    HeatConductor, HeatCrafter, HeatState, calculate_heat, crafter_efficiency_scale,
    heat_producer_step,
};

/// `HeatProducer` behavior (`HeatProducerBuild.updateTile`).
#[derive(Debug, Clone, Copy)]
pub struct HeatProducerBehavior {
    /// `HeatProducer.heatOutput`.
    pub heat_output: f32,
    /// `HeatProducer.warmupRate`.
    pub warmup_rate: f32,
}

impl BuildingBehavior for HeatProducerBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        crate::world::behavior::production::CrafterBehavior.create_state(world, e);
        match world.get_mut::<HeatState>(e) {
            Some(mut state) => {
                state.heat_output = self.heat_output;
                state.rotate = true;
            }
            None => {
                world.entity_mut(e).insert(HeatState {
                    heat_output: self.heat_output,
                    rotate: true,
                    ..Default::default()
                });
            }
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        crate::world::behavior::production::CrafterBehavior.update_tile(world, e);
        let efficiency = world
            .get::<Building>(e)
            .map(|building| building.efficiency)
            .unwrap_or(0.0);
        let delta = crate::world::update::delta(world, e);
        if let Some(mut state) = world.get_mut::<HeatState>(e) {
            state.heat = heat_producer_step(
                state.heat,
                self.heat_output,
                efficiency,
                self.warmup_rate,
                delta,
            );
        }
    }
}

/// `HeatCrafter` behavior (`HeatCrafterBuild.updateTile`/`efficiencyScale`).
#[derive(Debug, Clone, Copy)]
pub struct HeatCrafterBehavior {
    /// `HeatCrafter.heatRequirement`.
    pub requirement: f32,
    /// `HeatCrafter.overheatScale`.
    pub overheat_scale: f32,
    /// `HeatCrafter.maxEfficiency`.
    pub max_efficiency: f32,
}

impl BuildingBehavior for HeatCrafterBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        crate::world::behavior::production::CrafterBehavior.create_state(world, e);
        if world.get::<HeatCrafter>(e).is_none() {
            world.entity_mut(e).insert(HeatCrafter {
                requirement: self.requirement,
                overheat_scale: self.overheat_scale,
                max_efficiency: self.max_efficiency,
            });
        }
        if world.get::<HeatState>(e).is_none() {
            world.entity_mut(e).insert(HeatState::default());
        }
    }

    /// `HeatCrafterBuild.efficiencyScale()` — pulled during `updateConsumption`.
    fn efficiency_scale(&self, world: &mut World, e: Entity) -> f32 {
        let update_id = build_time(world) as u64;
        let heat = super::crafter_heat(world, e, update_id);
        if let Some(mut state) = world.get_mut::<HeatState>(e) {
            state.heat = heat;
        }
        crafter_efficiency_scale(
            heat,
            self.requirement,
            self.overheat_scale,
            self.max_efficiency,
        )
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        crate::world::behavior::production::CrafterBehavior.update_tile(world, e);
    }
}

/// `HeatConductor` behavior (`HeatConductorBuild.updateHeat`).
#[derive(Debug, Clone, Copy)]
pub struct HeatConductorBehavior {
    /// `HeatConductor.splitHeat`.
    pub split_heat: bool,
    /// `HeatConductor.visualMaxHeat`.
    pub visual_max_heat: f32,
}

impl BuildingBehavior for HeatConductorBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<HeatConductor>(e).is_none() {
            world.entity_mut(e).insert(HeatConductor {
                split_heat: self.split_heat,
                ..Default::default()
            });
        }
        if world.get::<HeatState>(e).is_none() {
            world.entity_mut(e).insert(HeatState {
                heat_output: self.visual_max_heat,
                rotate: true,
                ..Default::default()
            });
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let update_id = build_time(world) as u64;
        let needs_update = world
            .get::<HeatConductor>(e)
            .is_some_and(|conductor| conductor.update_id != update_id);
        if !needs_update {
            return;
        }
        let (mut side_heat, mut came_from) = world
            .get_mut::<HeatConductor>(e)
            .map(|mut conductor| {
                (
                    conductor.side_heat,
                    std::mem::take(&mut conductor.came_from),
                )
            })
            .unwrap_or(([0.0; 4], IdSet::new()));
        let heat = calculate_heat(world, e, &mut side_heat, &mut came_from, update_id, 0);
        let enabled = world
            .get::<Building>(e)
            .is_some_and(|building| building.enabled);
        if let Some(mut conductor) = world.get_mut::<HeatConductor>(e) {
            conductor.side_heat = side_heat;
            conductor.came_from = came_from;
            conductor.update_id = update_id;
        }
        if let Some(mut state) = world.get_mut::<HeatState>(e) {
            state.heat = if enabled { heat } else { 0.0 };
        }
    }
}

/// Registers the vanilla heat blocks (`world/blocks/crafting` Erekir set).
pub fn register(registry: &mut BehaviorRegistry, _content: &ContentRegistry) {
    for (name, heat_output, warmup_rate) in [
        ("oxidation-chamber", 5.0f32, 0.15f32),
        ("electric-heater", 3.0, 0.15),
        ("slag-heater", 8.0, 0.15),
        ("phase-heater", 15.0, 0.15),
        ("heat-reactor", 10.0, 0.15),
        ("heat-source", 1000.0, 1000.0),
    ] {
        registry.register_named(
            name,
            Arc::new(HeatProducerBehavior {
                heat_output,
                warmup_rate,
            }),
        );
    }

    for (name, requirement, max_efficiency) in [
        ("atmospheric-concentrator", 24.0f32, 1.0f32),
        ("carbide-crucible", 40.0, 1.0),
        ("surge-crucible", 40.0, 1.0),
        ("cyanogen-synthesizer", 20.0, 1.0),
        ("phase-synthesizer", 32.0, 1.0),
    ] {
        registry.register_named(
            name,
            Arc::new(HeatCrafterBehavior {
                requirement,
                overheat_scale: 1.0,
                max_efficiency,
            }),
        );
    }

    registry.register_named(
        "heat-redirector",
        Arc::new(HeatConductorBehavior {
            split_heat: false,
            visual_max_heat: 15.0,
        }),
    );
    registry.register_named(
        "small-heat-redirector",
        Arc::new(HeatConductorBehavior {
            split_heat: false,
            visual_max_heat: 15.0,
        }),
    );
    registry.register_named(
        "heat-router",
        Arc::new(HeatConductorBehavior {
            split_heat: true,
            visual_max_heat: 15.0,
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn producer_warms_conductor_and_crafter() {
        let mut harness = BuildHarness::new(16, 16, 7);
        let producer = harness
            .content()
            .block_id("heat-source")
            .expect("heat-source");
        let crafter = harness
            .content()
            .block_id("carbide-crucible")
            .expect("carbide-crucible");
        // `heat-source` is size 1 at (0,0); `carbide-crucible` is size 3 and
        // anchors at (2,1) (footprint (1,0)..(3,2)), orthogonally touching the
        // source and facing +x so the orientation gate passes.
        assert!(harness.place(0, 0, producer, 0, true));
        assert!(harness.place(2, 1, crafter, 0, true));

        for _ in 0..600 {
            harness.tick();
        }

        let producer_heat = harness
            .build_at(0, 0)
            .and_then(|e| harness.world.get::<HeatState>(e))
            .map(|state| state.heat)
            .unwrap_or(0.0);
        let crafter_heat = harness
            .build_at(2, 1)
            .and_then(|e| harness.world.get::<HeatState>(e))
            .map(|state| state.heat)
            .unwrap_or(0.0);

        assert!(producer_heat > 1.0, "producer_heat={producer_heat}");
        assert!(crafter_heat > 0.0, "crafter_heat={crafter_heat}");
    }

    #[test]
    fn heat_source_ramps_fast() {
        let mut harness = BuildHarness::new(8, 8, 7);
        let source = harness
            .content()
            .block_id("heat-source")
            .expect("heat-source");
        assert!(harness.place(1, 1, source, 0, true));
        for _ in 0..3 {
            harness.tick();
        }
        let heat = harness
            .build_at(1, 1)
            .and_then(|e| harness.world.get::<HeatState>(e))
            .map(|state| state.heat)
            .unwrap_or(0.0);
        assert!(heat > 0.0, "heat={heat}");
    }
}
