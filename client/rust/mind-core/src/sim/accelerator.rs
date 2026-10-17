// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Interplanetary Accelerator launch half (`Accelerator.AcceleratorBuild`
//! `canLaunch` / `buildConfiguration`, world/blocks/campaign/Accelerator.java:
//! 239-291). The planet-launch picker, carry and cutscene are host-side
//! (`MindCampaign.accelerator_launch_to`, `render::cutscene`).

use bevy_ecs::entity::Entity;

use super::Sim;
use crate::content::ItemId;
use crate::entities::comp::{Building, CampaignState};
use crate::game::runtime::CampaignRuntime;
use crate::world::blocks::power::PowerGrids;
use crate::world::modules::{ItemModule, PowerModule};

impl Sim {
    /// `getBatteryStored()` of `entity`'s power graph.
    fn accelerator_battery_stored(&self, entity: Entity) -> f32 {
        let world = &self.ecs.0;
        let Some(graph) = world.get::<PowerModule>(entity).map(|power| power.graph) else {
            return 0.0;
        };
        world
            .get_resource::<PowerGrids>()
            .and_then(|grids| grids.graph(graph))
            .map(|graph| graph.get_battery_stored(world))
            .unwrap_or(0.0)
    }

    /// `AcceleratorBuild.canLaunch()` (Accelerator.java:239-241): `isValid() &&
    /// !net.client() && state.isCampaign() && efficiency > 0 &&
    /// power.graph.getBatteryStored() >= powerBufferRequirement - 0.00001 &&
    /// progress >= 1 && !launching`.
    pub fn accelerator_can_launch(&self, entity: Entity) -> bool {
        let world = &self.ecs.0;
        let Some(building) = world.get::<Building>(entity) else {
            return false;
        };
        let Some(state) = world.get::<CampaignState>(entity) else {
            return false;
        };
        let Some(runtime) = world.get_resource::<CampaignRuntime>() else {
            return false;
        };
        if runtime.is_client || !runtime.session.is_campaign() {
            return false;
        }
        let def = crate::world::behavior::campaign::accelerator_def(world, entity);
        building.efficiency > 0.0
            && self.accelerator_battery_stored(entity) >= def.power_buffer_requirement - 0.00001
            && state.progress >= 1.0
            && !state.launching
    }

    /// `AcceleratorBuild.buildConfiguration` (Accelerator.java:258-291) up to
    /// the picker: `deselect()`, nothing when `!canLaunch()`, else the planet
    /// launch picker opens and `Trigger.acceleratorUse` fires. Returns whether
    /// the picker should open.
    pub fn accelerator_config_tapped(&mut self, entity: Entity) -> bool {
        if !self.accelerator_can_launch(entity) {
            return false;
        }
        self.triggers
            .fire(super::events::Trigger::AcceleratorUse, &mut self.ecs.0);
        true
    }

    /// The picker's confirm callback (Accelerator.java:262-272), re-checking
    /// `canLaunch()`: `consume()`, the stored items become the launch
    /// resources and are cleared, `useBatteries(powerBufferRequirement)`,
    /// `progress = 0`, and `renderer.showLaunch(this)` -> `beginLaunch(true)`
    /// sets `launching`. Returns the carried `(item, amount)` stacks, or `None`
    /// when the accelerator can no longer launch.
    pub fn accelerator_launch(&mut self, entity: Entity) -> Option<Vec<(ItemId, i32)>> {
        if !self.accelerator_can_launch(entity) {
            return None;
        }
        let world = &mut self.ecs.0;
        let def = crate::world::behavior::campaign::accelerator_def(world, entity);
        crate::world::update::trigger_consumers(world, entity);
        let resources: Vec<(ItemId, i32)> = world
            .get::<ItemModule>(entity)
            .map(|items| {
                items
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, amount)| **amount > 0)
                    .map(|(index, amount)| (ItemId::new(index as u16), *amount))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(mut items) = world.get_mut::<ItemModule>(entity) {
            items.clear();
        }
        let graph = world.get::<PowerModule>(entity).map(|power| power.graph);
        if let Some(graph) = graph {
            let mut grids = world.remove_resource::<PowerGrids>().unwrap_or_default();
            if let Some(graph) = grids.graph_mut(graph) {
                graph.use_batteries(world, def.power_buffer_requirement);
            }
            world.insert_resource(grids);
        }
        if let Some(mut state) = world.get_mut::<CampaignState>(entity) {
            state.progress = 0.0;
            state.launching = true;
        }
        Some(resources)
    }

    /// The `Time.runTask(launchDuration() - 6f)` body's first line
    /// (Accelerator.java:274): `launching = false` before the sector switch.
    pub fn accelerator_end_launch(&mut self, entity: Entity) {
        if let Some(mut state) = self.ecs.0.get_mut::<CampaignState>(entity) {
            state.launching = false;
        }
    }
}
