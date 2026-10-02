// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Liquid block behaviors (`world/blocks/liquid/{Conduit,ArmoredConduit,
//! LiquidRouter,LiquidJunction}.java`) as plan-07 [`BuildingBehavior`]s.
//!
//! Plan 07's `updateTile` API does not expose a `WorldGrid`, so these behaviors
//! use the proximity variants in [`super::movement`] (`nearby_proximity`) whose
//! neighbor lookup walks `Building.proximity`. Conduit autotiling reuses plan
//! 08's [`Autotiler`](crate::world::blocks::autotiler) with the exact
//! `Conduit.blends`/`ArmoredConduit.blends` predicates, matching the conveyor
//! implementation so masks stay identical.

use std::sync::Arc;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ContentRegistry};
use crate::entities::comp::Building;
use crate::world::behavior::{BehaviorRegistry, BuildingBehavior};
use crate::world::block::BlockTable;
use crate::world::blocks::autotiler::{
    BlendNeighbor, BlendWorld, blends_armored, build_blending, looking_at, looking_at_either,
    relative_to,
};
use crate::world::blocks::liquid::movement::{
    dump_liquid_proximity, move_liquid_forward_proximity, nearby_proximity,
};
use crate::world::blocks::liquid::{LiquidNode, current_liquid};
use crate::world::modules::LiquidModule;

/// `ConduitBuild` runtime/render-blend state.
#[derive(Debug, Clone, Copy, PartialEq, Default, Component)]
pub struct LiquidConduitState {
    /// `ConduitBuild.smoothLiquid` (render lerp).
    pub smooth_liquid: f32,
    /// `ConduitBuild.blendbits` (`buildBlending[0]`).
    pub blend_bits: i32,
    /// `ConduitBuild.xscl`.
    pub xscl: i32,
    /// `ConduitBuild.yscl`.
    pub yscl: i32,
    /// `ConduitBuild.blending` (`buildBlending[4]`).
    pub blending: i32,
    /// `ConduitBuild.capped`.
    pub capped: bool,
    /// `ConduitBuild.backCapped`.
    pub back_capped: bool,
}

/// `Conduit`/`ArmoredConduit` behavior.
#[derive(Debug, Clone, Copy)]
pub struct LiquidConduitBehavior {
    /// `Conduit.leaks`.
    pub leakable: bool,
    /// `ArmoredConduit` (armored blend predicate).
    pub armored: bool,
}

struct ConduitBlendWorld<'a> {
    table: &'a BlockTable,
    armored: bool,
    dirs: [Option<BlendNeighbor>; 4],
}

impl BlendWorld for ConduitBlendWorld<'_> {
    fn blends_block(
        &self,
        source: crate::world::TilePos,
        rotation: u8,
        other_x: i32,
        other_y: i32,
        other_rot: u8,
        other_block: BlockId,
    ) -> bool {
        let Some(def) = self.table.get(other_block).map(|inst| inst.def.clone()) else {
            return false;
        };
        if self.armored {
            return (def.outputs_liquid
                && blends_armored(
                    self,
                    source,
                    rotation,
                    other_x,
                    other_y,
                    other_rot,
                    other_block,
                ))
                || (looking_at(self, source, rotation, other_x, other_y, other_block)
                    && def.has_liquids)
                || def.kind == BlockKind::LiquidJunction;
        }
        def.has_liquids
            && (def.outputs_liquid
                || looking_at(self, source, rotation, other_x, other_y, other_block))
            && looking_at_either(
                self,
                source,
                rotation,
                other_x,
                other_y,
                other_rot,
                other_block,
            )
    }

    fn near_build(&self, direction: u8, _source: crate::world::TilePos) -> Option<BlendNeighbor> {
        self.dirs.get(direction as usize).copied().flatten()
    }

    fn square_sprite(&self, block: BlockId) -> bool {
        self.table
            .get(block)
            .is_some_and(|inst| inst.def.square_sprite)
    }

    fn rotated_output(&self, block: BlockId) -> bool {
        self.table.get(block).is_some_and(|inst| inst.rotate)
    }

    fn block_size(&self, block: BlockId) -> i32 {
        self.table.get(block).map(|inst| inst.def.size).unwrap_or(1)
    }
}

fn neighbor_dirs(world: &World, e: Entity) -> [Option<BlendNeighbor>; 4] {
    let mut dirs = [None; 4];
    let Some(building) = world.get::<Building>(e) else {
        return dirs;
    };
    let source = building.tile;
    for other in &building.proximity {
        let Some(other_building) = world.get::<Building>(*other) else {
            continue;
        };
        let real = relative_to(
            source.x() as i32,
            source.y() as i32,
            other_building.tile.x() as i32,
            other_building.tile.y() as i32,
        );
        if (0..4).contains(&real) {
            dirs[real as usize] = Some(BlendNeighbor {
                x: other_building.tile.x() as i32,
                y: other_building.tile.y() as i32,
                rotation: other_building.rotation,
                block: other_building.block,
            });
        }
    }
    dirs
}

fn capacity_of(world: &World, e: Entity) -> f32 {
    let Some(block) = world.get::<Building>(e).map(|building| building.block) else {
        return 0.0;
    };
    world
        .get_resource::<BlockTable>()
        .and_then(|table| table.instance(block))
        .map(|instance| instance.def.liquid_capacity.max(0.0))
        .unwrap_or(0.0)
}

fn ensure_node(world: &mut World, e: Entity, node: LiquidNode) {
    if world.get::<LiquidNode>(e).is_none() {
        world.entity_mut(e).insert(node);
    }
}

impl BuildingBehavior for LiquidConduitBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        let capacity = capacity_of(world, e);
        ensure_node(
            world,
            e,
            LiquidNode {
                capacity,
                accepts: true,
                leakable: self.leakable,
                junction: false,
                router: false,
                reject_from_output: true,
                pressure: 1.0,
                filter: Default::default(),
            },
        );
        if world.get::<LiquidConduitState>(e).is_none() {
            world.entity_mut(e).insert(LiquidConduitState {
                xscl: 1,
                yscl: 1,
                ..Default::default()
            });
        }
        if world.get::<LiquidModule>(e).is_none() {
            let liquids = world
                .get_resource::<crate::world::modules::ModuleDims>()
                .map(|dims| dims.liquids)
                .unwrap_or(0);
            world
                .entity_mut(e)
                .insert(LiquidModule::with_liquids(liquids));
        }
    }

    fn on_proximity_update(&self, world: &mut World, e: Entity) {
        let Some((tile, rotation)) = world
            .get::<Building>(e)
            .map(|building| (building.tile, building.rotation))
        else {
            return;
        };
        let dirs = neighbor_dirs(world, e);
        let table = world.get_resource::<BlockTable>().cloned();
        let (blend_bits, blending, xscl, yscl) = match &table {
            Some(table) => {
                let adapter = ConduitBlendWorld {
                    table,
                    armored: self.armored,
                    dirs,
                };
                let bits = build_blending(&adapter, tile, rotation, &[None; 4], true);
                (bits[0], bits[4], bits[1], bits[2])
            }
            None => (0, 0, 1, 1),
        };
        let (front, back) = (
            nearby_proximity(world, e, rotation),
            nearby_proximity(world, e, (rotation + 2) % 4),
        );
        let is_open = |other: Option<Entity>| match other {
            Some(other) => {
                let same_team = world
                    .get::<crate::entities::comp::TeamComp>(other)
                    .zip(world.get::<crate::entities::comp::TeamComp>(e))
                    .is_some_and(|(a, b)| a.team == b.team);
                same_team
                    && world
                        .get::<LiquidNode>(other)
                        .is_some_and(|node| node.accepts)
            }
            None => false,
        };
        let capped = !is_open(front);
        let back_capped = blend_bits == 0 && !is_open(back);
        if let Some(mut state) = world.get_mut::<LiquidConduitState>(e) {
            state.blend_bits = blend_bits;
            state.blending = blending;
            state.xscl = xscl;
            state.yscl = yscl;
            state.capped = capped;
            state.back_capped = back_capped;
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        let delta = crate::world::update::delta(world, e);
        let Some((amount, capacity)) = world.get::<LiquidModule>(e).map(|module| {
            (
                module.current_amount,
                world
                    .get::<LiquidNode>(e)
                    .map(|node| node.capacity)
                    .unwrap_or(0.0),
            )
        }) else {
            return;
        };
        if let Some(mut state) = world.get_mut::<LiquidConduitState>(e) {
            state.smooth_liquid = if capacity > 0.0 {
                state.smooth_liquid
                    + (amount / capacity - state.smooth_liquid) * (0.05 * delta).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        if amount > 0.0001
            && let Some(liquid) = world.get::<LiquidModule>(e).and_then(current_liquid)
        {
            let rotation = world
                .get::<Building>(e)
                .map(|building| building.rotation)
                .unwrap_or(0);
            let next = nearby_proximity(world, e, rotation);
            move_liquid_forward_proximity(world, e, next, self.leakable, liquid);
        }
    }
}

/// `LiquidRouter`/container/tank behavior (`LiquidRouterBuild.updateTile`).
#[derive(Debug, Clone, Copy)]
pub struct LiquidRouterBehavior;

impl BuildingBehavior for LiquidRouterBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        let capacity = capacity_of(world, e);
        ensure_node(
            world,
            e,
            LiquidNode {
                capacity,
                accepts: true,
                leakable: false,
                junction: false,
                router: true,
                reject_from_output: false,
                pressure: 1.0,
                filter: Default::default(),
            },
        );
        if world.get::<LiquidModule>(e).is_none() {
            let liquids = world
                .get_resource::<crate::world::modules::ModuleDims>()
                .map(|dims| dims.liquids)
                .unwrap_or(0);
            world
                .entity_mut(e)
                .insert(LiquidModule::with_liquids(liquids));
        }
    }

    fn update_tile(&self, world: &mut World, e: Entity) {
        if let Some(liquid) = world.get::<LiquidModule>(e).and_then(current_liquid) {
            dump_liquid_proximity(world, e, liquid, 2.0, -1);
        }
    }

    fn always_update_when_disabled(&self) -> bool {
        true
    }
}

/// `LiquidJunction` behavior (`getLiquidDestination` pass-through).
#[derive(Debug, Clone, Copy)]
pub struct LiquidJunctionBehavior;

impl BuildingBehavior for LiquidJunctionBehavior {
    fn create_state(&self, world: &mut World, e: Entity) {
        ensure_node(
            world,
            e,
            LiquidNode {
                capacity: capacity_of(world, e),
                accepts: true,
                leakable: false,
                junction: true,
                router: false,
                reject_from_output: false,
                pressure: 1.0,
                filter: Default::default(),
            },
        );
        if world.get::<LiquidModule>(e).is_none() {
            let liquids = world
                .get_resource::<crate::world::modules::ModuleDims>()
                .map(|dims| dims.liquids)
                .unwrap_or(0);
            world
                .entity_mut(e)
                .insert(LiquidModule::with_liquids(liquids));
        }
    }
}

/// Registers the vanilla conduit/router/junction blocks.
pub fn register(registry: &mut BehaviorRegistry, _content: &ContentRegistry) {
    for name in ["conduit", "pulse-conduit"] {
        registry.register_named(
            name,
            Arc::new(LiquidConduitBehavior {
                leakable: true,
                armored: false,
            }),
        );
    }
    for name in ["plated-conduit", "reinforced-conduit"] {
        registry.register_named(
            name,
            Arc::new(LiquidConduitBehavior {
                leakable: false,
                armored: true,
            }),
        );
    }
    for name in [
        "liquid-router",
        "liquid-container",
        "liquid-tank",
        "reinforced-liquid-router",
        "reinforced-liquid-container",
    ] {
        registry.register_named(name, Arc::new(LiquidRouterBehavior));
    }
    for name in ["liquid-junction", "reinforced-liquid-junction"] {
        registry.register_named(name, Arc::new(LiquidJunctionBehavior));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::LiquidId;
    use crate::world::BuildHarness;

    #[test]
    fn conduit_transfers_to_next_conduit() {
        let mut harness = BuildHarness::new(8, 8, 7);
        let conduit = harness.content().block_id("conduit").expect("conduit");
        assert!(harness.place(0, 0, conduit, 0, true));
        assert!(harness.place(1, 0, conduit, 0, true));
        let source = harness.build_at(0, 0).expect("source");
        let tank = harness.build_at(1, 0).expect("tank");
        {
            let mut liquids = harness
                .world
                .get_mut::<LiquidModule>(source)
                .expect("liquids");
            liquids.add(LiquidId::WATER, 20.0, 20.0);
        }
        for _ in 0..5 {
            harness.tick();
        }
        let moved = harness
            .world
            .get::<LiquidModule>(tank)
            .map(|module| module.get(LiquidId::WATER))
            .unwrap_or(0.0);
        assert!(moved > 0.0, "moved={moved}");
    }

    #[test]
    fn liquid_router_dumps_to_neighbor() {
        let mut harness = BuildHarness::new(8, 8, 7);
        let router = harness.content().block_id("liquid-router").expect("router");
        let container = harness
            .content()
            .block_id("liquid-container")
            .expect("container");
        assert!(harness.place(0, 0, router, 0, true));
        assert!(harness.place(2, 0, container, 0, true));
        // A conduit joins them so they are in proximity.
        let conduit = harness.content().block_id("conduit").expect("conduit");
        assert!(harness.place(1, 0, conduit, 0, true));
        let source = harness.build_at(0, 0).expect("router");
        let conduit_e = harness.build_at(1, 0).expect("conduit");
        {
            let mut liquids = harness
                .world
                .get_mut::<LiquidModule>(source)
                .expect("liquids");
            liquids.add(LiquidId::WATER, 100.0, 120.0);
        }
        for _ in 0..10 {
            harness.tick();
        }
        let moved = harness
            .world
            .get::<LiquidModule>(conduit_e)
            .map(|module| module.get(LiquidId::WATER))
            .unwrap_or(0.0);
        assert!(moved > 0.0, "moved={moved}");
    }
}
