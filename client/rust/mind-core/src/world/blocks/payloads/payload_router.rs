// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `PayloadRouter` build behavior
//! (`world/blocks/payloads/PayloadRouter.java`) — plan 08 M6 remainder.
//!
//! Extends `PayloadConveyor`: sorted-content config, `recDir` memory,
//! `checkMatch` and the deterministic `pickNext` rotation search.

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, UnitTypeId};
use crate::entities::comp::Building;
use crate::world::behavior::{BuildingBehavior, PayloadRef};
use crate::world::block::BlockTable;
use crate::world::config::ConfigValue;

use super::payload_conveyor::{
    PayloadConveyorBehavior, PayloadConveyorBuild, dispatch_accept_payload, resolve_next,
};
use super::{PayloadHolder, PayloadKind, kind_of};

/// `PayloadRouter.sorted` (block or unit reference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadSort {
    /// A block content target.
    Block(BlockId),
    /// A unit type content target.
    Unit(UnitTypeId),
}

/// `PayloadRouter.PayloadRouterBuild` extra state.
#[derive(Debug, Clone, Copy, Component)]
pub struct PayloadRouterBuild {
    /// Sort target (`sorted`).
    pub sorted: Option<PayloadSort>,
    /// Direction the payload arrived from (`recDir`).
    pub rec_dir: u8,
    /// Whether the held payload matches `sorted` (`matches`).
    pub matches: bool,
    /// Logic-control lockout timer (`controlTime`).
    pub control_time: f32,
    /// Whether the match test is inverted (`invert`).
    pub invert: bool,
}

impl Default for PayloadRouterBuild {
    fn default() -> Self {
        Self {
            sorted: None,
            rec_dir: 0,
            matches: false,
            control_time: -1.0,
            invert: false,
        }
    }
}

/// `PayloadRouter` behavior (`payload-router`, `reinforced-payload-router`).
pub struct PayloadRouterBehavior {
    /// Conveyor half (move time / limit).
    pub conveyor: PayloadConveyorBehavior,
}

impl PayloadRouterBehavior {
    /// Vanilla `payload-router`.
    pub const VANILLA: PayloadRouterBehavior = PayloadRouterBehavior {
        conveyor: PayloadConveyorBehavior::VANILLA,
    };
    /// `reinforced-payload-router` (`moveTime = 35`).
    pub const REINFORCED: PayloadRouterBehavior = PayloadRouterBehavior {
        conveyor: PayloadConveyorBehavior::REINFORCED,
    };

    /// `PayloadRouterBuild.checkMatch()`.
    pub fn check_match(&self, world: &mut World, e: Entity) {
        let Some(state) = world.get::<PayloadRouterBuild>(e).copied() else {
            return;
        };
        let payload = world.get::<PayloadHolder>(e).and_then(|h| h.payload);
        let mut matches = match (state.sorted, payload) {
            (Some(PayloadSort::Block(block)), Some(payload))
                if kind_of(payload) == PayloadKind::Build =>
            {
                world
                    .get::<Building>(payload.entity.unwrap_or(e))
                    .is_some_and(|b| b.block == block)
            }
            (Some(PayloadSort::Unit(unit)), Some(payload))
                if kind_of(payload) == PayloadKind::Unit =>
            {
                payload.content == unit.raw()
            }
            _ => false,
        };
        if state.invert {
            matches = !matches;
        }
        if let Some(mut state) = world.get_mut::<PayloadRouterBuild>(e) {
            state.matches = matches;
        }
    }

    /// `PayloadRouterBuild.pickNext()`.
    pub fn pick_next(&self, world: &mut World, e: Entity) {
        let Some((payload, control_time, matches, mut rotation, rec_dir, sorted)) =
            world.get::<PayloadRouterBuild>(e).copied().map(|s| {
                (
                    world.get::<PayloadHolder>(e).and_then(|h| h.payload),
                    s.control_time,
                    s.matches,
                    world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0),
                    s.rec_dir,
                    s.sorted,
                )
            })
        else {
            return;
        };

        if payload.is_some() && control_time <= 0.0 {
            if matches {
                rotation = rec_dir;
                set_rotation(world, e, rotation);
                self.on_proximity_update(world, e);
            } else {
                let mut rotations = 0;
                loop {
                    rotation = (rotation + 1) % 4;
                    // If it doesn't match the sort item and this router is facing
                    // forward, skip this rotation (Java `rotation++`).
                    if !matches && sorted.is_some() && rotation == rec_dir {
                        rotation = (rotation + 1) % 4;
                    }
                    set_rotation(world, e, rotation);
                    self.on_proximity_update(world, e);
                    if let Some(next) = resolve_next(world, e)
                        && world.get::<PayloadConveyorBuild>(next).is_some()
                        && world.get::<PayloadRouterBuild>(next).is_none()
                    {
                        force_update_next(world, next);
                    }
                    let blocked = world
                        .get::<PayloadConveyorBuild>(e)
                        .is_some_and(|c| c.blocked);
                    let next = resolve_next(world, e);
                    let accepted = next.is_some_and(|next| {
                        payload.is_some_and(|payload| {
                            dispatch_accept_payload(world, next, next, payload)
                        })
                    });
                    if !(blocked || next.is_none() || !accepted) {
                        break;
                    }
                    rotations += 1;
                    if rotations >= 4 {
                        break;
                    }
                }
            }
        } else {
            self.on_proximity_update(world, e);
        }
    }
}

fn set_rotation(world: &mut World, e: Entity, rotation: u8) {
    if let Some(mut building) = world.get_mut::<Building>(e) {
        building.rotation = rotation % 4;
    }
}

fn force_update_next(world: &mut World, next: Entity) {
    let Some(block) = world.get::<Building>(next).map(|b| b.block) else {
        return;
    };
    let Some(inst) = world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
    else {
        return;
    };
    inst.behavior.update_tile(world, next);
}

impl BuildingBehavior for PayloadRouterBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        self.conveyor.create_state(world, e);
        if world.get::<PayloadRouterBuild>(e).is_none() {
            let state = PayloadRouterBuild {
                rec_dir: world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0),
                ..Default::default()
            };
            world.entity_mut(e).insert(state);
        }
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        self.conveyor.on_proximity_update(world, e);
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        self.conveyor.update_tile(world, e);
        if let Some(mut state) = world.get_mut::<PayloadRouterBuild>(e) {
            state.control_time -= 1.0;
        }
    }

    fn accept_payload(
        &self,
        world: &World,
        e: Entity,
        source: Entity,
        payload: PayloadRef,
    ) -> bool {
        self.conveyor.accept_payload(world, e, source, payload)
    }

    fn handle_payload(&self, world: &mut World, e: Entity, source: Entity, payload: PayloadRef) {
        self.conveyor.handle_payload(world, e, source, payload);
        let rec_dir = if source == e {
            world.get::<Building>(e).map(|b| b.rotation).unwrap_or(0)
        } else {
            crate::world::blocks::distribution::transfer::relative_dir(world, e, source)
                .rem_euclid(4) as u8
        };
        if let Some(mut state) = world.get_mut::<PayloadRouterBuild>(e)
            && state.control_time < 0.0
        {
            state.rec_dir = rec_dir;
        }
        self.check_match(world, e);
        self.pick_next(world, e);
    }

    fn get_payload(&self, world: &World, e: Entity) -> Option<PayloadRef> {
        self.conveyor.get_payload(world, e)
    }

    fn take_payload(&self, world: &mut World, e: Entity) -> Option<PayloadRef> {
        self.conveyor.take_payload(world, e)
    }

    fn config(&self, world: &World, e: Entity) -> ConfigValue {
        match world.get::<PayloadRouterBuild>(e).and_then(|s| s.sorted) {
            Some(PayloadSort::Block(block)) => ConfigValue::Block(block),
            Some(PayloadSort::Unit(unit)) => ConfigValue::Unit(unit),
            None => ConfigValue::None,
        }
    }

    fn configured(
        &self,
        world: &mut World,
        e: Entity,
        _player: Option<Entity>,
        value: ConfigValue,
    ) {
        let sorted = match value {
            ConfigValue::Block(block) => Some(PayloadSort::Block(block)),
            ConfigValue::Unit(unit) => Some(PayloadSort::Unit(unit)),
            ConfigValue::None => None,
            _ => return,
        };
        if let Some(mut state) = world.get_mut::<PayloadRouterBuild>(e) {
            state.sorted = sorted;
        }
        self.check_match(world, e);
    }

    fn version(&self, _world: &World, _e: Entity) -> u8 {
        1
    }

    fn write(&self, world: &World, e: Entity, w: &mut crate::world::BuildingWriter) {
        self.conveyor.write(world, e, w);
        let state = world
            .get::<PayloadRouterBuild>(e)
            .copied()
            .unwrap_or_default();
        match state.sorted {
            Some(PayloadSort::Block(block)) => {
                w.b(0);
                w.s(block.raw() as i16);
            }
            Some(PayloadSort::Unit(unit)) => {
                w.b(1);
                w.s(unit.raw() as i16);
            }
            None => {
                w.b(-1);
                w.s(-1);
            }
        }
        w.b(state.rec_dir as i8);
    }

    fn read(
        &self,
        world: &mut World,
        e: Entity,
        r: &mut crate::world::BuildingReader,
        revision: u8,
    ) {
        self.conveyor.read(world, e, r, revision);
        if revision >= 1 {
            let ctype = r.b().unwrap_or(-1);
            let id = r.s().unwrap_or(-1);
            let sorted = if ctype == -1 || id < 0 {
                None
            } else if ctype == 0 {
                Some(PayloadSort::Block(BlockId::new(id as u16)))
            } else {
                Some(PayloadSort::Unit(UnitTypeId::new(id as u16)))
            };
            let rec_dir = r.b().unwrap_or(0).max(0) as u8;
            if let Some(mut state) = world.get_mut::<PayloadRouterBuild>(e) {
                state.sorted = sorted;
                state.rec_dir = rec_dir % 4;
            }
        }
        self.check_match(world, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::BuildHarness;

    fn place_router_chain(harness: &mut BuildHarness) -> (Entity, Entity, Entity) {
        let conveyor = harness
            .content()
            .block_id("payload-conveyor")
            .expect("payload-conveyor");
        let router = harness
            .content()
            .block_id("payload-router")
            .expect("payload-router");
        // Downstream belts first so `next` resolves; router at (6,6) facing +x.
        assert!(harness.place(12, 6, conveyor, 0, true));
        assert!(harness.place(9, 6, conveyor, 0, true));
        assert!(harness.place(6, 6, router, 0, true));
        let _ = conveyor;
        (
            harness.build_at(6, 6).expect("router"),
            harness.build_at(9, 6).expect("mid"),
            harness.build_at(12, 6).expect("end"),
        )
    }

    #[test]
    fn payload_router_config_roundtrip() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let (router, _, _) = place_router_chain(&mut harness);
        let block = harness.content().block_id("container").expect("container");
        assert!(harness.configure(6, 6, ConfigValue::Block(block)));
        let read = harness
            .world
            .get_resource::<BlockTable>()
            .and_then(|t| t.get(harness.world.get::<Building>(router).unwrap().block))
            .map(|inst| inst.behavior.config(&harness.world, router));
        assert_eq!(read, Some(ConfigValue::Block(block)));
    }

    #[test]
    fn payload_router_matching_forwards() {
        let mut harness = BuildHarness::new(32, 32, 7);
        let (router, _, _) = place_router_chain(&mut harness);
        let block = harness.content().block_id("container").expect("container");
        assert!(harness.configure(6, 6, ConfigValue::Block(block)));
        let entity =
            super::super::create_build_payload(&mut harness.world, block, 0).expect("payload");
        let payload = PayloadRef {
            entity: Some(entity),
            content: block.raw(),
            is_block: true,
        };
        super::super::dispatch_handle_payload(&mut harness.world, router, router, payload);
        assert!(
            harness
                .world
                .get::<PayloadRouterBuild>(router)
                .is_some_and(|s| s.matches)
        );
        for _ in 0..300 {
            harness.tick();
        }
        let end = harness.build_at(12, 6).expect("end");
        assert!(
            harness
                .world
                .get::<PayloadHolder>(end)
                .is_some_and(|h| h.payload.is_some()),
            "matching payload should reach the end of the belt chain"
        );
    }
}
