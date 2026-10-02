// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Heat network (Erekir) — `HeatBlock`/`HeatProducer`/`HeatConsumer`/
//! `HeatConductor` and `BuildingComp.calculateHeat`
//! (`world/blocks/heat/*.java`, `production/HeatCrafter.java`).
//!
//! Pull-based: consumers call [`calculate_heat`] inside `updateTile`; conductors
//! memoize by `GameState.update_id` and carry a per-conductor `came_from` cycle
//! guard (plan 09 §3.9). This file is the plan's `heat/{mod,calculate,producer,
//! conductor}.rs` consolidated; explosion/effect hooks stay plan 10/17.

pub mod behavior;

pub use behavior::register;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Resource;
use bevy_ecs::world::World;

use crate::entities::comp::Building;
use crate::util::IdSet;
use crate::world::blocks::liquid::movement::relative_to_dir;

/// Heat-bearing runtime state (producer/conductor/crafter/reactor).
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct HeatState {
    /// Current heat output (`HeatBlock.heat()`).
    pub heat: f32,
    /// Smoothed heat progress (`HeatBlock.heatFrac()` numerator).
    pub heat_progress: f32,
    /// Maximum heat output per side (`HeatProducer.heatOutput`).
    pub heat_output: f32,
    /// Whether the block must face its heat source (`block.rotate`).
    pub rotate: bool,
}

/// `HeatConductorBuild` memoization/cycle state.
#[derive(Debug, Clone, Component)]
pub struct HeatConductor {
    /// `HeatConductor.splitHeat` (routers split across 3 surfaces).
    pub split_heat: bool,
    /// Last `GameState.update_id` this conductor was pulled (`lastHeatUpdate`;
    /// upstream initializes to `-1` so the first pull always computes).
    pub update_id: u64,
    /// Traversed cycle guard (`cameFrom`).
    pub came_from: IdSet,
    /// Per-side heat from the last computation.
    pub side_heat: [f32; 4],
}

impl Default for HeatConductor {
    fn default() -> Self {
        Self {
            split_heat: false,
            update_id: u64::MAX,
            came_from: IdSet::new(),
            side_heat: [0.0; 4],
        }
    }
}

/// `HeatCrafter` knobs (`HeatCrafter` / `HeatConsumer`).
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct HeatCrafter {
    /// `HeatCrafter.heatRequirement`.
    pub requirement: f32,
    /// `HeatCrafter.overheatScale`.
    pub overheat_scale: f32,
    /// `HeatCrafter.maxEfficiency`.
    pub max_efficiency: f32,
}

/// Heat recursion scratch (plan 09 §3.2 `HeatScratch`).
#[derive(Debug, Clone, Copy, Default, Resource)]
pub struct HeatScratch {
    /// Debug recursion depth (R11).
    pub depth: u32,
}

/// `BuildingComp.calculateHeat`: contact-point heat from `HeatBlock` neighbors.
///
/// Returns the total heat and fills `side_heat` (indexed by relative direction).
pub fn calculate_heat(
    world: &mut World,
    entity: Entity,
    side_heat: &mut [f32; 4],
    came_from: &mut IdSet,
    update_id: u64,
    depth: u32,
) -> f32 {
    side_heat.fill(0.0);
    came_from.clear();
    if depth > 256 {
        log::error!("heat recursion depth exceeded at {entity:?}");
        return 0.0;
    }

    let Some(self_building) = world.get::<Building>(entity).cloned() else {
        return 0.0;
    };
    let self_team = world
        .get::<crate::entities::comp::TeamComp>(entity)
        .map(|team| team.team);
    let self_size = block_size(world, entity);
    let self_id = entity.index_u32();

    // `self_building` is an owned clone, so its inline `proximity` SmallVec can
    // be iterated directly; no per-call heap buffer (plan 09 §3.10).
    let proximity = self_building.proximity.clone();
    let mut total = 0.0;

    for other in proximity {
        if other == entity {
            continue;
        }
        let Some(other_state) = world.get::<HeatState>(other).copied() else {
            continue;
        };
        // Upstream's proximity loop only considers `HeatBlock` instances; a
        // `HeatCrafter` exposes `heat` but is a consumer, not a source (it is
        // never traversed and never contributes `cameFrom`).
        if world.get::<HeatCrafter>(other).is_some() {
            continue;
        }
        let other_team = world
            .get::<crate::entities::comp::TeamComp>(other)
            .map(|team| team.team);
        if self_team.is_some() && other_team != self_team {
            continue;
        }
        let Some(other_building) = world.get::<Building>(other).cloned() else {
            continue;
        };
        let other_size = block_size(world, other);
        let other_id = other.index_u32();
        let is_conductor = world.get::<HeatConductor>(other).is_some();
        let split = world
            .get::<HeatConductor>(other)
            .is_some_and(|conductor| conductor.split_heat);

        let dx = (other_building.tile.x() as i32 - self_building.tile.x() as i32).abs();
        let dy = (other_building.tile.y() as i32 - self_building.tile.y() as i32).abs();
        let diff = dx.min(dy);
        let relative = relative_to_dir(
            world,
            entity,
            other_building.tile.x(),
            other_building.tile.y(),
        );

        if !orientation_allows(other_state.rotate, split, relative, other_building.rotation) {
            continue;
        }

        // Cycle gate: a conductor that already traversed us contributes no
        // heat this frame. Crucially (upstream `calculateHeat`), the conductor
        // is still recursed and still contributes to `cameFrom`.
        let cycle = is_conductor
            && world
                .get::<HeatConductor>(other)
                .is_some_and(|conductor| conductor.came_from.contains(self_id));
        if !cycle {
            let contact = contact_points(self_size, other_size, diff);
            let mut add = other_state.heat / other_size as f32 * contact as f32;
            if split {
                add /= 3.0;
            }
            side_heat[(relative % 4) as usize] += add;
            total += add;
        }

        came_from.add(other_id);
        if let Some(conductor) = world.get::<HeatConductor>(other) {
            came_from.union_with(&conductor.came_from);
        }

        // Push the conductor's own heat (memoized by update_id).
        if is_conductor {
            let needs_update = world
                .get::<HeatConductor>(other)
                .is_some_and(|conductor| conductor.update_id != update_id);
            if needs_update {
                let mut conductor_came = world
                    .get_mut::<HeatConductor>(other)
                    .map(|mut conductor| std::mem::take(&mut conductor.came_from))
                    .unwrap_or_default();
                let mut conductor_side = [0.0; 4];
                let result = calculate_heat(
                    world,
                    other,
                    &mut conductor_side,
                    &mut conductor_came,
                    update_id,
                    depth + 1,
                );
                if let Some(mut conductor) = world.get_mut::<HeatConductor>(other) {
                    conductor.came_from = conductor_came;
                    conductor.side_heat = conductor_side;
                    conductor.update_id = update_id;
                }
                let enabled = world
                    .get::<Building>(other)
                    .map(|building| building.enabled)
                    .unwrap_or(false);
                if let Some(mut state) = world.get_mut::<HeatState>(other) {
                    state.heat = if enabled { result } else { 0.0 };
                }
            }
        }
    }

    total
}

/// `contactPoints = min((size/2 + otherSize/2 - diff) as i32, min(other, size))`.
pub fn contact_points(size: i32, other_size: i32, diff: i32) -> i32 {
    ((size as f32 / 2.0 + other_size as f32 / 2.0 - diff as f32) as i32).min(other_size.min(size))
}

/// Orientation gate from `calculateHeat` (plan 09 §3.9).
pub fn orientation_allows(rotate: bool, split: bool, relative: u8, other_rotation: u8) -> bool {
    !rotate
        || (!split && (relative + 2) % 4 == other_rotation)
        || (split && relative != other_rotation)
}

/// `HeatProducer`/`HeaterGenerator` ramp: `heat` approaches `output * efficiency`.
pub fn heat_producer_step(
    heat: f32,
    heat_output: f32,
    efficiency: f32,
    warmup_rate: f32,
    delta: f32,
) -> f32 {
    super::power::reactors::approach_delta(heat, heat_output * efficiency, warmup_rate, delta)
}

/// `HeatCrafter.updateTile`: `heat` from neighbors.
pub fn crafter_heat(world: &mut World, entity: Entity, update_id: u64) -> f32 {
    let mut side_heat = [0.0; 4];
    let mut came_from = IdSet::new();
    calculate_heat(world, entity, &mut side_heat, &mut came_from, update_id, 0)
}

/// `HeatCrafter.efficiencyScale` overheat formula.
///
/// `min(clamp(heat/req) + max(heat-req,0)/req * overheatScale, maxEfficiency)`.
pub fn crafter_efficiency_scale(
    heat: f32,
    requirement: f32,
    overheat_scale: f32,
    max_efficiency: f32,
) -> f32 {
    if requirement <= 0.0 {
        return 1.0;
    }
    let base = (heat / requirement).clamp(0.0, 1.0);
    let overheat = ((heat - requirement).max(0.0) / requirement) * overheat_scale;
    (base + overheat).min(max_efficiency)
}

/// `HeatCrafter.shouldConsume`: `heat > 0 && super`.
pub fn crafter_should_consume(heat: f32, enabled: bool) -> bool {
    heat > 0.0 && enabled
}

/// `HeatBlock.heatFrac`.
pub fn heat_frac(heat: f32, heat_output: f32) -> f32 {
    if heat_output > 0.0 {
        heat / heat_output
    } else {
        0.0
    }
}

fn block_size(world: &World, entity: Entity) -> i32 {
    world
        .get::<Building>(entity)
        .and_then(|building| {
            world
                .get_resource::<crate::world::block::BlockTable>()
                .and_then(|table| table.get(building.block))
                .map(|instance| instance.def.size.max(1))
        })
        .unwrap_or(1)
}

#[cfg(test)]
mod tests;
