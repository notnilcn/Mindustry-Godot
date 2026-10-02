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
    HeatConductor, HeatCrafter, HeatState, calculate_heat, contact_points,
    crafter_efficiency_scale, crafter_heat, orientation_allows,
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
    // Guarded: B already traversed A, so A ignores B's heat (upstream
    // `calculateHeat` cycle branch). B is still recursed/memoized, which is why
    // the two cases use separate fixtures (a conductor's own heat is a cached
    // derivation, not a source).
    let mut guarded = HeatFixture::new();
    let a = guarded.spawn(0, 0, 0.0, Some(false));
    let b = guarded.spawn(1, 0, 9.0, Some(false));
    guarded.link(a, b);
    {
        let mut came = IdSet::new();
        came.add(a.index_u32());
        guarded
            .world
            .get_mut::<HeatConductor>(b)
            .expect("b")
            .came_from = came;
    }
    assert_eq!(guarded.heat_from(a), 0.0);

    // Without the guard, B's pre-recursion heat flows into A.
    let mut unguarded = HeatFixture::new();
    let a = unguarded.spawn(0, 0, 0.0, Some(false));
    let b = unguarded.spawn(1, 0, 9.0, Some(false));
    unguarded.link(a, b);
    let heat = unguarded.heat_from(a);
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

/// Regression: a `HeatCrafter` (consumer, not `HeatBlock`) must never be
/// traversed as a heat source, so a conductor between a producer and a crafter
/// keeps delivering heat frame after frame (upstream `calculateHeat`).
#[test]
fn crafter_pull_keeps_conductor_hot_across_frames() {
    let mut fixture = HeatFixture::new();
    let producer = fixture.spawn(0, 0, 10.0, None);
    let conductor = fixture.spawn(1, 0, 0.0, Some(false));
    let crafter = fixture.spawn(2, 0, 0.0, None);
    fixture.world.entity_mut(crafter).insert(HeatCrafter {
        requirement: 5.0,
        overheat_scale: 1.0,
        max_efficiency: 3.0,
    });
    fixture.link(producer, conductor);
    fixture.link(conductor, crafter);

    let first = crafter_heat(&mut fixture.world, crafter, 1);
    let second = crafter_heat(&mut fixture.world, crafter, 2);
    assert!(second > first, "first={first} second={second}");
    assert!((second - 10.0).abs() < 0.1, "second={second}");
    let came = fixture
        .world
        .get::<HeatConductor>(conductor)
        .expect("conductor");
    assert!(!came.came_from.contains(crafter.index_u32()));
}
