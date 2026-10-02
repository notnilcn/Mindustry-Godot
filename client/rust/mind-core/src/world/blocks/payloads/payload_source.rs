// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadSource` build behavior (`world/blocks/payloads/PayloadSource.java`) —
//! plan 08 M7 (sandbox source). Unit creation (`new UnitPayload(unit.create)`)
//! and `commandPos` hand-off belong to plan 11; this port implements the block
//! path and the config surface.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, UnitTypeId};
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::config::ConfigValue;

use super::payload_block::{move_out_payload, payload_block_update, rot_deg};
use super::{PayloadHolder, create_build_payload};

/// `PayloadSource.PayloadSourceBuild` state.
#[derive(Debug, Clone, Copy, Component, Default)]
pub struct PayloadSourceBuild {
    /// Configured unit type (`unit`; consumed by plan 11).
    pub unit: Option<UnitTypeId>,
    /// Configured block (`configBlock`).
    pub config_block: Option<BlockId>,
    /// Command position (`commandPos`).
    pub command_pos: Option<(f32, f32)>,
    /// Spawn scale animation (`scl`).
    pub scl: f32,
}

/// `PayloadSource` behavior (`payload-source`).
#[derive(Debug, Clone, Copy, Default)]
pub struct PayloadSourceBehavior;

impl PayloadSourceBehavior {
    /// Sandbox `payload-source`.
    pub const VANILLA: PayloadSourceBehavior = PayloadSourceBehavior;
}

impl BuildingBehavior for PayloadSourceBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        if world.get::<PayloadSourceBuild>(e).is_none() {
            world.entity_mut(e).insert(PayloadSourceBuild::default());
        }
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
    }

    fn accept_payload(
        &self,
        _world: &World,
        _e: Entity,
        _source: Entity,
        _payload: PayloadRef,
    ) -> bool {
        false
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        world.get::<PayloadHolder>(e).and_then(|h| h.payload)
    }

    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        super::take_payload(world, e)
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        payload_block_update(world, e);
        let has_payload = world
            .get::<PayloadHolder>(e)
            .is_some_and(|h| h.payload.is_some());
        if !has_payload {
            if let Some(mut state) = world.get_mut::<PayloadSourceBuild>(e) {
                state.scl = 0.0;
            }
            let config = world
                .get::<PayloadSourceBuild>(e)
                .copied()
                .unwrap_or_default();
            let team = world
                .get::<crate::entities::comp::TeamComp>(e)
                .map(|t| t.team)
                .unwrap_or(0);
            if let Some(block) = config.config_block {
                let rotation = rot_deg(world, e);
                if let Some(payload_entity) = create_build_payload(world, block, team) {
                    let payload = PayloadRef {
                        entity: Some(payload_entity),
                        content: block.raw(),
                        is_block: true,
                    };
                    if let Some(mut holder) = world.get_mut::<PayloadHolder>(e) {
                        holder.payload = Some(payload);
                        holder.pay_vector = (0.0, 0.0);
                        holder.pay_rotation = rotation;
                    }
                }
            }
            // Unit config creation is plan 11 (`UnitType.create` +
            // `commandPos`); no-op until units land.
        }
        if let Some(mut state) = world.get_mut::<PayloadSourceBuild>(e) {
            state.scl += (1.0 - state.scl) * 0.1;
        }
        move_out_payload(world, e);
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        let state = world
            .get::<PayloadSourceBuild>(e)
            .copied()
            .unwrap_or_default();
        match (state.unit, state.config_block) {
            (_, Some(block)) => ConfigValue::Block(block),
            (Some(unit), _) => ConfigValue::Unit(unit),
            _ => ConfigValue::None,
        }
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        match value {
            ConfigValue::Block(block) => {
                if let Some(mut state) = world.get_mut::<PayloadSourceBuild>(e) {
                    state.config_block = Some(block);
                    state.unit = None;
                    state.payload_reset();
                }
            }
            ConfigValue::Unit(unit) => {
                if let Some(mut state) = world.get_mut::<PayloadSourceBuild>(e) {
                    state.unit = Some(unit);
                    state.config_block = None;
                    state.payload_reset();
                }
            }
            ConfigValue::None => {
                if let Some(mut state) = world.get_mut::<PayloadSourceBuild>(e) {
                    state.config_block = None;
                    state.unit = None;
                    state.payload_reset();
                }
            }
            _ => {}
        }
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        super::payload_block::write_base(world, e, w);
        let state = world
            .get::<PayloadSourceBuild>(e)
            .copied()
            .unwrap_or_default();
        w.s(state.unit.map(|u| u.raw() as i16).unwrap_or(-1));
        w.s(state.config_block.map(|b| b.raw() as i16).unwrap_or(-1));
        match state.command_pos {
            Some((x, y)) => {
                w.bool(true);
                w.f(x);
                w.f(y);
            }
            None => w.bool(false),
        }
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        super::payload_block::read_base(world, e, r);
        let unit = r.s().unwrap_or(-1);
        let block = r.s().unwrap_or(-1);
        let command_pos = if revision >= 1 && r.bool().unwrap_or(false) {
            Some((r.f().unwrap_or(0.0), r.f().unwrap_or(0.0)))
        } else {
            None
        };
        if let Some(mut state) = world.get_mut::<PayloadSourceBuild>(e) {
            state.unit = (unit >= 0).then(|| UnitTypeId::new(unit as u16));
            state.config_block = (block >= 0).then(|| BlockId::new(block as u16));
            state.command_pos = command_pos;
        }
    }
}

impl PayloadSourceBuild {
    fn payload_reset(&mut self) {
        self.scl = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    #[test]
    fn source_spawns_configured_block_payload() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let container = harness.content().block_id("container").expect("container");
        let conveyor = harness
            .content()
            .block_id("payload-conveyor")
            .expect("payload-conveyor");
        let source = harness
            .content()
            .block_id("payload-source")
            .expect("payload-source");
        // payload-source is size 5; its front edge is 3 tiles from center.
        assert!(harness.place(10, 6, conveyor, 0, true));
        assert!(harness.place(6, 6, source, 0, true));
        assert!(harness.configure(6, 6, ConfigValue::Block(container)));
        for _ in 0..300 {
            harness.tick();
        }
        let front = harness.build_at(10, 6).expect("front conveyor");
        let payload = harness
            .world
            .get::<PayloadHolder>(front)
            .and_then(|h| h.payload);
        assert!(
            payload.is_some_and(|p| p.content == container.raw() && p.is_block),
            "source should have produced and moved out a container payload"
        );
    }
}
