// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Plan 09 §7a heat tests: `contact_points_math`, `orientation_predicate`,
//! `split_heat_thirds`, `cycle_guard`, `crafter_overheat_scale`.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::BlockId;
use crate::entities::comp::{Building, TeamComp};
use crate::util::IdSet;
use crate::world::TilePos;

use super::{
    HeatConductor, HeatState, calculate_heat, contact_points, crafter_efficiency_scale,
    orientation_allows,
};

struct HeatFixture {
    world: World,
}

impl HeatFixture {
    fn new() -> Self {
        Self {
            world: World::new(),
        }
    }

    fn spawn(&mut self, x: i16, y: i16, heat: f32, conductor: Option<bool>) -> Entity {
        let entity = self
            .world
            .spawn((
                Building::new(TilePos::new(x, y), BlockId::AIR, 0),
                TeamComp { team: 0 },
                HeatState {
                    heat,
                    ..HeatState::default()
                },
            ))
            .id();
        if let Some(split) = conductor {
            self.world.entity_mut(entity).insert(HeatConductor {
                split_heat: split,
                ..HeatConductor::default()
            });
        }
        entity
    }

    fn link(&mut self, a: Entity, b: Entity) {
        let mut building = self.world.get_mut::<Building>(a).expect("a");
        if !building.proximity.contains(&b) {
            building.proximity.push(b);
        }
        let mut other = self.world.get_mut::<Building>(b).expect("b");
        if !other.proximity.contains(&a) {
            other.proximity.push(a);
        }
    }

    fn heat_from(&mut self, entity: Entity) -> f32 {
        let mut side = [0.0; 4];
        let mut came_from = IdSet::new();
        calculate_heat(&mut self.world, entity, &mut side, &mut came_from, 1, 0)
    }
}

#[test]
fn contact_points_math() {
    assert_eq!(contact_points(1, 1, 0), 1);
    assert_eq!(contact_points(2, 1, 0), 1);
    assert_eq!(contact_points(2, 2, 0), 2);
    assert_eq!(contact_points(3, 3, 1), 2);
    // diff larger than the footprints yields a non-positive contact.
    assert!(contact_points(1, 1, 3) <= 0);
}

#[test]
fn orientation_predicate() {
    // Non-rotating blocks always pass.
    assert!(orientation_allows(false, false, 1, 2));
    // Non-split: must face us ((relative+2)%4 == other.rotation).
    assert!(orientation_allows(true, false, 0, 2));
    assert!(!orientation_allows(true, false, 1, 2));
    // Split: must face away.
    assert!(!orientation_allows(true, true, 0, 0));
    assert!(orientation_allows(true, true, 1, 0));
}

#[test]
fn split_heat_thirds() {
    let mut fixture = HeatFixture::new();
    let consumer = fixture.spawn(0, 0, 0.0, None);
    // Split conductor with 9 heat adjacent -> 9 / 3 = 3.
    let conductor = fixture.spawn(1, 0, 9.0, Some(true));
    fixture.link(consumer, conductor);
    let heat = fixture.heat_from(consumer);
    assert!((heat - 3.0).abs() < f32::EPSILON);
}

#[test]
fn cycle_guard() {
    let mut fixture = HeatFixture::new();
    let a = fixture.spawn(0, 0, 0.0, Some(false));
    let b = fixture.spawn(1, 0, 9.0, Some(false));
    fixture.link(a, b);
    // B already came from A -> A must ignore B's heat.
    {
        let mut came = IdSet::new();
        came.add(a.index_u32());
        fixture
            .world
            .get_mut::<HeatConductor>(b)
            .expect("b")
            .came_from = came;
    }
    assert_eq!(fixture.heat_from(a), 0.0);
    // Without the guard, heat flows.
    fixture
        .world
        .get_mut::<HeatConductor>(b)
        .expect("b")
        .came_from
        .clear();
    let heat = fixture.heat_from(a);
    assert!((heat - 9.0).abs() < f32::EPSILON);
}

#[test]
fn crafter_overheat_scale() {
    // Below/at requirement: linear 0..1.
    assert!((crafter_efficiency_scale(0.0, 10.0, 1.0, 2.0) - 0.0).abs() < f32::EPSILON);
    assert!((crafter_efficiency_scale(10.0, 10.0, 1.0, 2.0) - 1.0).abs() < f32::EPSILON);
    // Overheat adds `(heat-req)/req * overheatScale`, capped at maxEfficiency.
    assert!((crafter_efficiency_scale(20.0, 10.0, 0.5, 3.0) - 1.5).abs() < f32::EPSILON);
    assert!((crafter_efficiency_scale(100.0, 10.0, 1.0, 2.0) - 2.0).abs() < f32::EPSILON);
}

#[test]
fn producer_conducts_contact_heat() {
    let mut fixture = HeatFixture::new();
    let consumer = fixture.spawn(0, 0, 0.0, None);
    let producer = fixture.spawn(1, 0, 7.0, None);
    fixture.link(consumer, producer);
    let heat = fixture.heat_from(consumer);
    assert!((heat - 7.0).abs() < f32::EPSILON);
}
