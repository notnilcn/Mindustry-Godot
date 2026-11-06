// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadSource` build behavior (`world/blocks/payloads/PayloadSource.java`) —
//! plan 08 M7 (sandbox source): block and unit payload creation, `commandPos`
//! hand-off and `moveOutPayload` (`PayloadSource.java:130-182`).

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
    /// Configured unit type (`unit`; the carried `UnitPayload` source).
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
        if world.get::<PayloadSourceBuild>(e).is_none() {
            world.entity_mut(e).insert(PayloadSourceBuild::default());
        }
        if world.get::<PayloadHolder>(e).is_none() {
            world.entity_mut(e).insert(PayloadHolder::default());
        }
    }

    fn config_kinds(&self) -> &'static [crate::world::config::ConfigKind] {
        // `PayloadSource.buildConfiguration`: blocks + units.
        &[
            crate::world::config::ConfigKind::Block,
            crate::world::config::ConfigKind::Unit,
        ]
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

    /// `PayloadBlockBuild.onDestroyed` (`PayloadBlock.java:176-180`): the held
    /// payload is destroyed with the block (build payloads run their own
    /// destroy hooks; a carried unit payload is dropped/despawned).
    fn on_destroyed(&self, world: &mut World, e: Entity) {
        if let Some(payload) = world
            .get::<crate::world::blocks::payloads::PayloadHolder>(e)
            .and_then(|holder| holder.payload)
        {
            crate::world::blocks::payloads::payload_destroyed(world, payload);
        }
        if let Some(mut holder) = world.get_mut::<crate::world::blocks::payloads::PayloadHolder>(e)
        {
            holder.payload = None;
        }
    }

    fn on_removed(&self, world: &mut World, e: Entity) {
        // `PayloadBlockBuild.onRemoved` (`PayloadBlock.java:170-174`).
        super::payload_block::on_removed_dump(world, e);
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
            if let Some(unit) = config.unit {
                // `payload = new UnitPayload(unit.create(team))`
                // (`PayloadSource.java:148-158`): the unit is carried, not
                // spawned, and takes `commandPos` when it is commandable.
                let rotation = rot_deg(world, e);
                if let Some(pos) = world.get::<crate::entities::comp::Pos>(e).copied()
                    && let Some(payload) = crate::sim::unit_runtime::create_unit_payload(
                        world, unit, team, pos.x, pos.y, rotation,
                    )
                {
                    if let Some(entity) = payload.entity {
                        if let Some((cx, cy)) = config.command_pos
                            && let Some(mut slot) =
                                world.get_mut::<crate::ai::controller::ControllerSlot>(entity)
                            && slot.kind == crate::ai::controller::AiKind::Command
                        {
                            slot.target = Some(crate::world::TilePos::new(
                                crate::world::WorldGrid::to_tile(cx) as i16,
                                crate::world::WorldGrid::to_tile(cy) as i16,
                            ));
                            world
                                .entity_mut(entity)
                                .insert(crate::ai::types::command::AttackTargetComp(None));
                        }
                        // `Events.fire(new UnitCreateEvent(p, this))`.
                        let name = world
                            .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
                            .and_then(|runtime| runtime.content.unit(unit))
                            .map(|def| def.name.clone());
                        crate::game::stats::notify_unit_created(world, entity, name.as_deref());
                    }
                    crate::world::blocks::payloads::handle_payload(world, e, e, payload);
                    if let Some(mut holder) = world.get_mut::<PayloadHolder>(e) {
                        holder.pay_rotation = rotation;
                    }
                }
            } else if let Some(block) = config.config_block {
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
        if !super::payload_block::read_base(world, e, r) {
            return;
        }
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

    /// Installs the live unit runtime the carried-unit path needs (content +
    /// `UnitRuntime.units`); `BuildHarness` only boots the block runtime.
    fn install_unit_runtime(harness: &mut BuildHarness) {
        let runtime = crate::sim::unit_runtime::UnitRuntime::new(
            harness.content.clone(),
            &harness.grid,
            &harness.world,
            0,
            false,
        );
        harness.world.insert_non_send(runtime);
    }

    /// GAPS #23 / PAR-0518: `PayloadSource.updateTile` creates a *carried* unit
    /// payload from the configured `UnitType` (`PayloadSource.java:148-158`),
    /// then `moveOutPayload` hands it to the front conveyor. The unit never
    /// enters `Groups.unit` while carried.
    #[test]
    fn source_spawns_configured_unit_payload() {
        use crate::world::blocks::payloads::{CarriedUnit, PayloadHolder};

        let mut harness = BuildHarness::new(32, 32, 7);
        install_unit_runtime(&mut harness);
        let conveyor = harness
            .content()
            .block_id("payload-conveyor")
            .expect("payload-conveyor");
        let source = harness
            .content()
            .block_id("payload-source")
            .expect("payload-source");
        let dagger = harness.content().unit_id("dagger").expect("dagger");
        // payload-source is size 5; its front edge is 3 tiles from center and
        // overlaps the size-3 conveyor centred at (10, 6).
        assert!(harness.place(10, 6, conveyor, 0, true));
        assert!(harness.place(6, 6, source, 0, true));
        assert!(harness.configure(6, 6, ConfigValue::Unit(dagger)));

        harness.tick();
        let source_e = harness.build_at(6, 6).expect("source entity");
        let payload = harness
            .world
            .get::<PayloadHolder>(source_e)
            .and_then(|holder| holder.payload)
            .expect("unit payload created");
        assert!(!payload.is_block, "unit payload");
        let entity = payload.entity.expect("payload entity");
        assert_eq!(payload.content, dagger.raw());
        assert!(
            harness
                .world
                .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
                .is_some_and(|comp| comp.type_id == dagger)
        );
        assert!(
            harness.world.get::<CarriedUnit>(entity).is_some(),
            "payload carries the CarriedUnit marker"
        );
        assert!(
            harness
                .world
                .get::<crate::entities::comp::Unit>(entity)
                .is_none(),
            "a carried unit is not in Groups.unit"
        );

        // `moveOutPayload` ejects it onto the front payload conveyor.
        for _ in 0..300 {
            harness.tick();
        }
        let front = harness.build_at(10, 6).expect("front conveyor");
        assert!(
            harness
                .world
                .get::<PayloadHolder>(front)
                .and_then(|holder| holder.payload)
                .is_some_and(|payload| !payload.is_block && payload.content == dagger.raw()),
            "source moved its unit payload onto the front conveyor"
        );
    }

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
