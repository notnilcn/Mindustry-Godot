// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit/building tethers (plan 11 §3.6).
//!
//! Ported from `core/src/mindustry/entities/comp/UnitTetherComp.java` and
//! `BuildingTetherComp.java`: a tether unit despawns when the entity it is bound
//! to disappears. ID serialization is plan 04's; the plan-08 tether handshake
//! supplies the spawner id.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::entities::comp::unit::comp::{BuildingTetherComp, UnitTetherComp};
use crate::entities::comp::unit::lifecycle::kill_unit;

/// `UnitTetherComp.update`: kill the unit when its spawner is gone.
///
/// A unit with no spawner set (e.g. a hand-spawned `manifold`) is left alive,
/// matching the "not yet bound" state.
pub fn check_tether(world: &mut World, entity: Entity) -> bool {
    let spawner = world.get::<UnitTetherComp>(entity).and_then(|t| t.spawner);
    if let Some(spawner) = spawner
        && world.get_entity(spawner).is_err()
    {
        kill_unit(world, entity);
        return false;
    }
    let building = world
        .get::<BuildingTetherComp>(entity)
        .and_then(|t| t.building);
    if let Some(building) = building
        && world.get_entity(building).is_err()
    {
        kill_unit(world, entity);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use crate::ai::UnitHarness;
    use crate::entities::comp::unit::comp::UnitTetherComp;

    #[test]
    fn dangling_tether_despawns() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness
            .spawn("manifold", 0, 64.0, 64.0, 0.0)
            .expect("manifold");
        // Bind to an entity that is then despawned.
        let ghost = harness.spawn("dagger", 1, 10.0, 10.0, 0.0).expect("ghost");
        harness.build.world.entity_mut(unit).insert(UnitTetherComp {
            spawner: Some(ghost),
        });
        harness.build.world.despawn(ghost);
        super::check_tether(&mut harness.build.world, unit);
        assert!(!harness.is_alive(unit), "tethered unit despawned");
    }
}
