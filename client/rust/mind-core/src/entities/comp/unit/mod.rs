// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit-side entity components, lifecycle and queries (plan 11 §3.1/§3.6).
//!
//! The component structs live in [`comp`]; [`defs`] owns the plan-11
//! `ComponentKind`/12-def vocabulary; [`lifecycle`] is the single spawn/remove
//! path; [`queries`] ports the `entities/Units.java` set helpers.

pub mod comp;
pub mod crawl;
pub mod defs;
pub mod environment;
pub mod inverse_kinematics;
pub mod legs;
pub mod lifecycle;
pub mod mech;
pub mod miner;
pub mod movement;
pub mod queries;
pub mod tank;
pub mod tether;
pub mod velocity;
pub mod water_move;
pub mod weapon_mount;

pub use comp::{
    BlockUnitComp, BuilderComp, BuildingTetherComp, ChildComp, CrawlComp, ElevationMoveComp,
    HitboxComp, ItemsComp, Leg, LegsComp, MechComp, MinerComp, OwnerComp, PayloadComp, PhysicsComp,
    SegmentComp, ShieldComp, ShielderComp, StatusComp, StatusEntry, TankComp, TargetDummyComp,
    TimedComp, TimedKillComp, UnderwaterMoveComp, UnitCore, UnitTetherComp, UnitTypeComp,
    WaterCrawlComp, WaterMoveComp,
};
pub use defs::{
    ALL_KINDS, BASE_CLOSURE, ComponentKind, KIND_COUNT, UNIT_DEFS, UnitDefSpec, def_by_name,
    kind_of,
};
pub use lifecycle::{
    ai_kind_of, controller_of, kill_unit, remove_unit, set_move_target, spawn_unit, spawn_unit_def,
    sync_weapon_state, unit_type_of,
};
pub use queries::{
    UnitSnapshot, all, best, can_create, closest, count, get_cap, in_radius, snapshot,
};
pub use weapon_mount::{WeaponMount, WeaponsComp, setup_weapons};

#[cfg(test)]
mod tests {
    use super::comp::*;
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::lifecycle::{ai_kind_of, unit_type_of};
    use crate::entities::comp::unit::queries;

    #[test]
    fn spawn_inserts_per_kind_components() {
        let mut harness = UnitHarness::new(32, 32, 3);
        let dagger = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        assert!(harness.build.world.get::<MechComp>(dagger).is_some());
        assert!(unit_type_of(&harness.build.world, dagger).is_some());
        assert!(ai_kind_of(&harness.build.world, dagger).is_some());

        let risso = harness.spawn("risso", 0, 64.0, 64.0, 0.0).expect("risso");
        assert!(harness.build.world.get::<WaterMoveComp>(risso).is_some());

        let mega = harness.spawn("mega", 0, 64.0, 64.0, 0.0).expect("mega");
        assert!(harness.build.world.get::<PayloadComp>(mega).is_some());

        assert_eq!(harness.unit_count(), 3);
    }

    /// GAPS #61: `WaterCrawl`/`UnderwaterMove`/`Shielder` component kinds now
    /// spawn their runtime components (the water-crawl predicates live in
    /// [`crate::entities::comp::unit::water_move`]).
    #[test]
    fn aquatic_and_shielder_components_spawn() {
        use crate::content::registries::units::{UnitComponent, UnitTypeDef};
        use crate::entities::comp::unit::lifecycle::spawn_unit_def;

        let mut harness = UnitHarness::new(32, 32, 11);
        let extra: &'static [UnitComponent] = Box::leak(
            vec![
                UnitComponent::Unit,
                UnitComponent::Crawl,
                UnitComponent::WaterCrawl,
                UnitComponent::UnderwaterMove,
                UnitComponent::Shielder,
            ]
            .into_boxed_slice(),
        );
        let mut def: UnitTypeDef = harness
            .content()
            .unit_by_name("latum")
            .expect("latum")
            .clone();
        def.entity_def.components = extra;
        let entity = spawn_unit_def(&mut harness.build.world, 700, &def, 0, 64.0, 64.0, 0.0);
        assert!(harness.build.world.get::<CrawlComp>(entity).is_some());
        assert!(harness.build.world.get::<WaterCrawlComp>(entity).is_some());
        assert!(
            harness
                .build
                .world
                .get::<UnderwaterMoveComp>(entity)
                .is_some()
        );
        let mut shielder = harness
            .build
            .world
            .get::<ShielderComp>(entity)
            .copied()
            .expect("ShielderComp");
        shielder.absorb();
        assert_eq!(shielder.absorbed, 0.0);

        // `WaterCrawlComp.floorSpeedMultiplier`/`UnderwaterMoveComp` class
        // constants (`WaterCrawlComp.java:30-33`, `UnderwaterMoveComp.java:19-32`).
        assert_eq!(
            crate::entities::comp::unit::water_move::water_crawl_floor_speed_multiplier(true, 1.0),
            1.0
        );
        assert_eq!(
            crate::entities::comp::unit::water_move::water_crawl_floor_speed_multiplier(false, 1.0),
            1.3
        );
        const {
            assert!(
                !crate::entities::comp::unit::water_move::UNDERWATER_HITTABLE
                    && !crate::entities::comp::unit::water_move::UNDERWATER_TARGETABLE
                    && crate::entities::comp::unit::water_move::UNDERWATER_COLLISION_LAYER == 3
            );
        }
    }

    /// GAPS #61: `LaunchCoreComp` carries the launched block plus the Java
    /// progress curves (`LaunchCoreComp.java:31,57,84-90`).
    #[test]
    fn launch_core_progress_curves() {
        use crate::entities::comp::building::LaunchCoreComp;
        let core = LaunchCoreComp { block: 3 };
        assert_eq!(LaunchCoreComp::alpha(0.0), 1.0);
        assert_eq!(LaunchCoreComp::alpha(1.0), 0.0);
        assert!(core.cy(0.0, 1.0) >= 85.0);
        assert!(core.cx(0.0, 1.0) >= 10.0);
    }

    #[test]
    fn queries_count_and_closest_are_deterministic() {
        let mut harness = UnitHarness::new(64, 64, 5);
        let a = harness.spawn("dagger", 0, 40.0, 40.0, 0.0).expect("a");
        let _b = harness.spawn("dagger", 1, 400.0, 400.0, 0.0).expect("b");
        assert_eq!(queries::count(&mut harness.build.world, None), 2);
        assert_eq!(queries::count(&mut harness.build.world, Some(1)), 1);
        let near = queries::closest(&mut harness.build.world, 44.0, 44.0, 100.0, Some(0));
        assert_eq!(near, Some(a));
        let radius = queries::in_radius(&mut harness.build.world, 40.0, 40.0, 32.0, None);
        assert_eq!(radius, vec![a]);
    }

    #[test]
    fn segmented_spawn_builds_a_linked_chain() {
        use crate::content::registries::units::UnitTypeDef;
        use crate::entities::comp::unit::lifecycle::spawn_unit_def;

        let mut harness = UnitHarness::new(64, 64, 9);
        // Vanilla `segmentUnits` is 1; synthesize a 3-child chain to exercise the
        // Java `UnitType.spawn` segmented branch (plan 11 §3.4).
        let mut def: UnitTypeDef = harness
            .content()
            .unit_by_name("latum")
            .expect("latum")
            .clone();
        def.segment_units = 3;
        def.segment_spacing = 12.0;
        let head = spawn_unit_def(&mut harness.build.world, 500, &def, 0, 128.0, 128.0, 0.0);

        assert!(harness.build.world.get::<SegmentComp>(head).is_none());
        // Count the chain children directly (deterministic entity-order scan).
        let children: Vec<_> = crate::entities::comp::unit::queries::all(&mut harness.build.world)
            .into_iter()
            .filter(|e| harness.build.world.get::<ChildComp>(*e).is_some())
            .collect();
        assert_eq!(children.len(), 3, "head + 3 segment units");
        for (index, child) in children.iter().enumerate() {
            let segment = harness.build.world.get::<SegmentComp>(*child).unwrap();
            assert_eq!(segment.index as usize, index + 1);
            assert!(segment.parent.is_some());
        }
    }
}
