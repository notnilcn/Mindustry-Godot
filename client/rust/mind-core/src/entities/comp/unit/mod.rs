// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit-side entity components, lifecycle and queries (plan 11 §3.1/§3.6).
//!
//! The component structs live in [`comp`]; [`defs`] owns the plan-11
//! `ComponentKind`/12-def vocabulary; [`lifecycle`] is the single spawn/remove
//! path; [`queries`] ports the `entities/Units.java` set helpers.

pub mod comp;
pub mod defs;
pub mod lifecycle;
pub mod queries;

pub use comp::{
    BlockUnitComp, BuildingTetherComp, CrawlComp, ElevationMoveComp, HitboxComp, LegsComp,
    MechComp, PayloadComp, PhysicsComp, SegmentComp, TankComp, TargetDummyComp, TimedComp,
    TimedKillComp, UnitCore, UnitTypeComp, WaterMoveComp,
};
pub use defs::{
    ALL_KINDS, BASE_CLOSURE, ComponentKind, KIND_COUNT, UNIT_DEFS, UnitDefSpec, def_by_name,
    kind_of,
};
pub use lifecycle::{
    ai_kind_of, controller_of, kill_unit, remove_unit, set_move_target, spawn_unit, spawn_unit_def,
    unit_type_of,
};
pub use queries::{UnitSnapshot, all, closest, count, in_radius, snapshot};

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
}
