// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadDeconstructor` build behavior
//! (`world/blocks/payloads/PayloadDeconstructor.java`) — plan 08 M7.
//!
//! Deconstructs a carried build/unit payload back into its item requirements
//! through an accumulation buffer. Liquid spilling calls plan 10's
//! `Puddles` seam (L12) and is a no-op here.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::modules::ItemModule;
use crate::world::update::edelta;

use super::payload_block::{
    accept_payload_base, consume_payload_entity, move_in_payload, payload_block_update,
};
use super::{PayloadHolder, payload_build_time, payload_fits, payload_requirements};

/// `PayloadDeconstructor.PayloadDeconstructorBuild` state.
#[derive(Debug, Clone, Component, Default)]
pub struct PayloadDeconstructorBuild {
    /// Payload currently being deconstructed (`deconstructing`).
    pub deconstructing: Option<PayloadRef>,
    /// Fractional item accumulation per requirement (`accum`).
    pub accum: Vec<f32>,
    /// Deconstruction progress `0..1` (`progress`).
    pub progress: f32,
    /// Animation time (`time`).
    pub time: f32,
}

/// `PayloadDeconstructor` behavior (`small-deconstructor`, `deconstructor`).
#[derive(Debug, Clone, Copy)]
pub struct PayloadDeconstructorBehavior {
    /// `deconstructSpeed`.
    pub deconstruct_speed: f32,
    /// `maxPayloadSize`.
    pub max_payload_size: f32,
    /// `dumpRate`.
    pub dump_rate: i32,
    /// `itemBuffer` (payload requirements at/above this bypass the fullness gate).
    pub item_buffer: i32,
}

impl PayloadDeconstructorBehavior {
    /// `small-deconstructor` (size 3, speed 3).
    pub const SMALL: PayloadDeconstructorBehavior = PayloadDeconstructorBehavior {
        deconstruct_speed: 3.0,
        max_payload_size: 3.0,
        dump_rate: 4,
        item_buffer: 80,
    };
    /// `deconstructor` (size 5, speed 6).
    pub const LARGE: PayloadDeconstructorBehavior = PayloadDeconstructorBehavior {
        deconstruct_speed: 6.0,
        max_payload_size: 4.0,
        dump_rate: 4,
        item_buffer: 300,
    };
}

impl BuildingBehavior for PayloadDeconstructorBehavior {
    fn update_batch(
        &self,
        world: &mut bevy_ecs::world::World,
        inst: &crate::world::block::BlockInstance,
        entities: &[bevy_ecs::entity::Entity],
    ) {
        // Plan 08 §7.4: allocate-free batched dispatch (empty-consumer fast path).
        for &e in entities {
            crate::world::update::building_update_no_consumers(world, e, inst);
        }
    }

    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<PayloadDeconstructorBuild>(e).is_none() {
            world
                .entity_mut(e)
                .insert(PayloadDeconstructorBuild::default());
        }
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        source: Entity,
        payload: PayloadRef,
    ) -> bool {
        let state = world
            .get::<PayloadDeconstructorBuild>(e)
            .cloned()
            .unwrap_or_default();
        if state.deconstructing.is_some()
            || !accept_payload_base(world, e)
            || !payload_fits(world, payload, self.max_payload_size)
        {
            return false;
        }
        let reqs = payload_requirements(world, payload);
        if reqs.is_empty() {
            return false;
        }
        // Fullness gate (only when the payload arrives from another building).
        if source != e {
            let item_capacity = super::super::distribution::transfer::item_capacity(world, e);
            let items = world.get::<ItemModule>(e);
            let mut is_full = false;
            for (item, amount) in &reqs {
                let current = items.map(|m| m.get(*item)).unwrap_or(0);
                if current + *amount >= item_capacity {
                    is_full = true;
                }
                if *amount >= self.item_buffer {
                    is_full = false;
                    break;
                }
            }
            if is_full {
                return false;
            }
        }
        true
    }

    fn handle_payload(&self, world: &mut World, e: Entity, source: Entity, payload: PayloadRef) {
        super::handle_payload(world, e, source, payload);
        if let Some(mut state) = world.get_mut::<PayloadDeconstructorBuild>(e) {
            state.accum.clear();
        }
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        world
            .get::<PayloadDeconstructorBuild>(e)
            .and_then(|s| s.deconstructing)
            .or_else(|| world.get::<PayloadHolder>(e).and_then(|h| h.payload))
    }

    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        let deconstructing = world
            .get_mut::<PayloadDeconstructorBuild>(e)
            .and_then(|mut s| s.deconstructing.take());
        deconstructing.or_else(|| super::take_payload(world, e))
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        payload_block_update(world, e);

        let item_capacity = super::super::distribution::transfer::item_capacity(world, e);
        let total = world.get::<ItemModule>(e).map(|m| m.total).unwrap_or(0);
        if total > 0 {
            for _ in 0..self.dump_rate {
                let _ =
                    crate::world::blocks::distribution::transfer::dump_accumulate(world, e, None);
            }
        }

        let mut state = world
            .get::<PayloadDeconstructorBuild>(e)
            .cloned()
            .unwrap_or_default();
        if state.deconstructing.is_none() {
            state.progress = 0.0;
        }
        let speed = self.deconstruct_speed;
        let dt = edelta(world, e);
        state.time += dt;
        if let Some(mut live) = world.get_mut::<PayloadDeconstructorBuild>(e) {
            live.time = state.time;
            live.progress = state.progress;
        }

        let Some(deconstructing) = state.deconstructing else {
            // No payload being processed: try to ingest one.
            if move_in_payload(world, e, false)
                && let Some(payload) = world.get::<PayloadHolder>(e).and_then(|h| h.payload)
            {
                let reqs = payload_requirements(world, payload);
                if let Some(mut live) = world.get_mut::<PayloadDeconstructorBuild>(e) {
                    live.accum = vec![0.0; reqs.len()];
                    live.deconstructing = Some(payload);
                    live.progress = 0.0;
                }
                if let Some(mut holder) = world.get_mut::<PayloadHolder>(e) {
                    holder.payload = None;
                }
            }
            return;
        };

        let reqs = payload_requirements(world, deconstructing);
        if state.accum.len() != reqs.len() {
            state.accum = vec![0.0; reqs.len()];
        }

        let mut can_progress = true;
        for (item, _) in &reqs {
            let current = world
                .get::<ItemModule>(e)
                .map(|m| m.get(*item))
                .unwrap_or(0);
            if current >= item_capacity {
                can_progress = false;
                break;
            }
        }

        if can_progress {
            let build_time = payload_build_time(world, deconstructing).max(f32::EPSILON);
            let shift = dt * speed / build_time;
            let real_shift = shift.min(1.0 - state.progress);
            state.progress += shift;
            for (index, (_, amount)) in reqs.iter().enumerate() {
                if index < state.accum.len() {
                    state.accum[index] += *amount as f32 * real_shift;
                }
            }
        }

        // Move accumulated integers into the item module.
        for (index, (item, _)) in reqs.iter().enumerate() {
            if index >= state.accum.len() {
                continue;
            }
            let available = item_capacity
                - world
                    .get::<ItemModule>(e)
                    .map(|m| m.get(*item))
                    .unwrap_or(0);
            let taken = (state.accum[index] as i32).min(available).max(0);
            if taken > 0 {
                if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                    items.add(*item, taken, item_capacity.max(0));
                }
                state.accum[index] -= taken as f32;
            }
        }

        if state.progress >= 1.0 {
            let mut finish = true;
            for (index, (item, _)) in reqs.iter().enumerate() {
                if index < state.accum.len() && (state.accum[index] - 1.0).abs() < 0.0001 {
                    let total = world.get::<ItemModule>(e).map(|m| m.total).unwrap_or(0);
                    if total < item_capacity {
                        if let Some(mut items) = world.get_mut::<ItemModule>(e) {
                            items.add(*item, 1, item_capacity.max(0));
                        }
                        state.accum[index] = 0.0;
                    } else {
                        finish = false;
                        break;
                    }
                }
            }
            if finish {
                consume_payload_entity(world, deconstructing);
                state.deconstructing = None;
                state.accum.clear();
            }
        }

        if let Some(mut live) = world.get_mut::<PayloadDeconstructorBuild>(e) {
            *live = state;
        }
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        super::payload_block::write_base(world, e, w);
        let state = world
            .get::<PayloadDeconstructorBuild>(e)
            .cloned()
            .unwrap_or_default();
        w.f(state.progress);
        w.s(state.accum.len() as i16);
        for value in &state.accum {
            w.f(*value);
        }
        super::payload_write(world, state.deconstructing, w);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        _revision: u8,
    ) {
        super::payload_block::read_base(world, e, r);
        let mut state = world
            .get::<PayloadDeconstructorBuild>(e)
            .cloned()
            .unwrap_or_default();
        state.progress = r.f().unwrap_or(0.0);
        let count = r.s().unwrap_or(0).max(0) as usize;
        state.accum = Vec::with_capacity(count);
        for _ in 0..count {
            state.accum.push(r.f().unwrap_or(0.0));
        }
        state.deconstructing = None;
        world.entity_mut(e).insert(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;
    use crate::world::modules::ItemModule;

    #[test]
    fn deconstructor_returns_requirements() {
        let mut harness = BuildHarness::new(24, 24, 7);
        let wall = harness
            .content()
            .block_id("copper-wall")
            .expect("copper-wall");
        let deconstructor = harness
            .content()
            .block_id("small-deconstructor")
            .expect("small-deconstructor");
        let copper = harness.content().item_id("copper").expect("copper");
        if let Some(mut rules) = harness
            .world
            .get_resource_mut::<crate::world::limits::BuildRules>()
        {
            rules.cheat = true;
        }
        assert!(harness.place(6, 6, deconstructor, 0, true));
        let e = harness.build_at(6, 6).expect("deconstructor");
        let payload_entity =
            super::super::create_build_payload(&mut harness.world, wall, 0).expect("payload");
        let payload = PayloadRef {
            entity: Some(payload_entity),
            content: wall.raw(),
            is_block: true,
        };
        super::super::handle_payload(&mut harness.world, e, e, payload);
        for _ in 0..800 {
            harness.tick();
        }
        let copper_count = harness
            .world
            .get::<ItemModule>(e)
            .map(|m| m.get(copper))
            .unwrap_or(0);
        assert!(copper_count > 0, "deconstructor should yield copper");
        assert!(harness.world.get_entity(payload_entity).is_err());
    }
}
