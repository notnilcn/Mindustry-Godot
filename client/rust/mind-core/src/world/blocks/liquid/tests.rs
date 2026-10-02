// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 09 §7a liquid tests: `transfer_flow_formula`,
//! `junction_destination_recursion`, `conduit_leak`, `router_dumps_current`,
//! `flow_window_rates`.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, LiquidId};
use crate::entities::comp::{Building, TeamComp};
use crate::world::modules::LiquidModule;
use crate::world::{TilePos, WorldGrid};

use super::{LiquidFlowCache, LiquidNode, current_liquid, move_liquid, update_router};

struct LiquidFixture {
    world: World,
    grid: WorldGrid,
}

impl LiquidFixture {
    fn new() -> Self {
        Self {
            world: World::new(),
            grid: WorldGrid::new(16, 16),
        }
    }

    fn spawn(&mut self, x: i16, y: i16, node: LiquidNode) -> Entity {
        let entity = self
            .world
            .spawn((
                Building::new(TilePos::new(x, y), BlockId::AIR, 0),
                TeamComp { team: 0 },
                LiquidModule::with_liquids(2),
                node,
            ))
            .id();
        self.grid.tiles.get_mut(x as i32, y as i32).build = Some(entity);
        entity
    }

    fn water(&self, entity: Entity) -> f32 {
        self.world
            .get::<LiquidModule>(entity)
            .map(|module| module.get(LiquidId::WATER))
            .unwrap_or(0.0)
    }

    fn add_water(&mut self, entity: Entity, amount: f32) {
        let capacity = self
            .world
            .get::<LiquidNode>(entity)
            .map(|node| node.capacity)
            .unwrap_or(0.0);
        if let Some(mut module) = self.world.get_mut::<LiquidModule>(entity) {
            module.add(LiquidId::WATER, amount, capacity);
        }
    }

    fn connect(&mut self, a: Entity, b: Entity) {
        let mut building = self.world.get_mut::<Building>(a).expect("a");
        if !building.proximity.contains(&b) {
            building.proximity.push(b);
        }
    }
}

fn conduit(capacity: f32) -> LiquidNode {
    LiquidNode {
        capacity,
        accepts: true,
        pressure: 1.0,
        ..LiquidNode::default()
    }
}

/// `transfer_flow_formula`: `moveLiquid` uses the fraction difference.
#[test]
fn transfer_flow_formula() {
    let mut fixture = LiquidFixture::new();
    let source = fixture.spawn(0, 0, conduit(100.0));
    let dest = fixture.spawn(1, 0, conduit(100.0));
    fixture.add_water(source, 50.0);

    let moved = move_liquid(
        &mut fixture.world,
        &fixture.grid,
        source,
        dest,
        LiquidId::WATER,
    );
    assert_eq!(moved, 50.0);
    assert_eq!(fixture.water(source), 0.0);
    assert_eq!(fixture.water(dest), 50.0);

    // Partial equalization: source full, dest at 25%.
    let source = fixture.spawn(2, 0, conduit(100.0));
    let dest = fixture.spawn(3, 0, conduit(100.0));
    fixture.add_water(source, 100.0);
    fixture.add_water(dest, 25.0);
    let moved = move_liquid(
        &mut fixture.world,
        &fixture.grid,
        source,
        dest,
        LiquidId::WATER,
    );
    assert_eq!(moved, 75.0);
    assert_eq!(fixture.water(source), 25.0);
    assert_eq!(fixture.water(dest), 100.0);
}

/// `junction_destination_recursion`: a junction passes through to the far block.
#[test]
fn junction_destination_recursion() {
    let mut fixture = LiquidFixture::new();
    let source = fixture.spawn(0, 0, conduit(100.0));
    let junction = fixture.spawn(
        1,
        0,
        LiquidNode {
            junction: true,
            ..LiquidNode::default()
        },
    );
    let dest = fixture.spawn(2, 0, conduit(100.0));
    fixture.add_water(source, 50.0);

    let resolved = super::movement::get_liquid_destination(
        &fixture.world,
        &fixture.grid,
        junction,
        source,
        LiquidId::WATER,
    );
    assert_eq!(resolved, dest);

    let moved = move_liquid(
        &mut fixture.world,
        &fixture.grid,
        source,
        junction,
        LiquidId::WATER,
    );
    assert_eq!(moved, 50.0);
    assert_eq!(fixture.water(source), 0.0);
    assert_eq!(fixture.water(junction), 0.0);
    assert_eq!(fixture.water(dest), 50.0);
}

/// `conduit_leak`: an unconnected leaking conduit loses `stored / 1.5`.
#[test]
fn conduit_leak() {
    let mut fixture = LiquidFixture::new();
    let conduit = fixture.spawn(
        0,
        0,
        LiquidNode {
            leakable: true,
            ..conduit(100.0)
        },
    );
    fixture.add_water(conduit, 30.0);

    let (moved, leaked) = super::movement::move_liquid_forward(
        &mut fixture.world,
        &fixture.grid,
        conduit,
        None,
        true,
        LiquidId::WATER,
    );
    assert_eq!(moved, 0.0);
    assert_eq!(leaked, 20.0);
    assert_eq!(fixture.water(conduit), 10.0);
}

/// `router_dumps_current`: a router transfers the fraction difference.
#[test]
fn router_dumps_current() {
    let mut fixture = LiquidFixture::new();
    let router = fixture.spawn(
        0,
        0,
        LiquidNode {
            router: true,
            ..conduit(100.0)
        },
    );
    let dest = fixture.spawn(1, 0, conduit(100.0));
    fixture.connect(router, dest);
    fixture.add_water(router, 50.0);
    assert_eq!(
        current_liquid(fixture.world.get::<LiquidModule>(router).expect("module")),
        Some(LiquidId::WATER)
    );

    let grid = fixture.grid.clone();
    update_router(&mut fixture.world, &grid, router);
    assert_eq!(fixture.water(dest), 25.0);
    assert_eq!(fixture.water(router), 25.0);
}

/// `flow_window_rates`: 6 samples of 10/tick at a 10-tick poll -> 60 u/s.
#[test]
fn flow_window_rates() {
    let mut cache = LiquidFlowCache::new(2);
    cache.active = true;
    for _ in 0..59 {
        cache.record(LiquidId::WATER, 1.0);
        cache.tick();
    }
    // Not enough data yet (5 samples, display refreshed at 30/60).
    assert!(cache.get_flow_rate(LiquidId::WATER) < 0.0);
    cache.record(LiquidId::WATER, 1.0);
    cache.tick();
    // 60 ticks: 6 samples, refresh fires -> mean 10 / 10 * 60 = 60 u/s.
    assert!((cache.get_flow_rate(LiquidId::WATER) - 60.0).abs() < 0.001);
    assert!(cache.has_flow_liquid(LiquidId::WATER));
}
