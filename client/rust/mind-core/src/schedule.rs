// SPDX-License-Identifier: GPL-3.0-only

//! Simulation schedule labels.
//!
//! Ported from `core/src/mindustry/core/Logic.java` `updateEntities()`
//! (lines ~470–496): the slot order is reserved verbatim so later plans can fill
//! systems in place without re-ordering (D8, plan 05). At P0 only the command
//! application (outside the ECS) and the `Buildings` slot are populated.

use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SingleThreadedExecutor, SystemSet};

/// Reserved `Logic.updateEntities()` slot order.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SimSet {
    /// `Groups.updatePooling()`.
    PoolCleanup,
    /// `Groups.bullet.updatePhysics()`.
    BulletPhysics,
    /// `Groups.unit.updatePhysics()`.
    UnitPhysics,
    /// `Groups.player.update()`.
    Players,
    /// `Groups.effect.update()`.
    Effects,
    /// `Groups.all.update()`.
    EntityGroups,
    /// `Groups.unit.update()`.
    Units,
    /// `Groups.powerGraph.update()`.
    PowerGraph,
    /// `Groups.build.update()`.
    Buildings,
    /// `Groups.bullet.update()`.
    Bullets,
    /// `Groups.bullet.collide()`.
    Collisions,
}

/// Builds the P0 schedule: single-threaded executor (determinism), sets chained in
/// `Logic.updateEntities()` order, one placeholder `Buildings` system.
pub fn build_p0_schedule() -> Schedule {
    let mut schedule = Schedule::default();
    // Deterministic execution: never let bevy's multithreaded executor reorder systems.
    schedule.set_executor(SingleThreadedExecutor::new());
    schedule.configure_sets(
        (
            SimSet::PoolCleanup,
            SimSet::BulletPhysics,
            SimSet::UnitPhysics,
            SimSet::Players,
            SimSet::Effects,
            SimSet::EntityGroups,
            SimSet::Units,
            SimSet::PowerGraph,
            SimSet::Buildings,
            SimSet::Bullets,
            SimSet::Collisions,
        )
            .chain(),
    );
    schedule.add_systems(p0_buildings.in_set(SimSet::Buildings));
    schedule
}

/// Placeholder for the P0 buildings slot (plan 05/07 fill it).
fn p0_buildings() {}

#[cfg(test)]
mod tests {
    use bevy_ecs::world::World;

    use super::*;

    #[test]
    fn p0_schedule_builds_and_runs() {
        let mut schedule = build_p0_schedule();
        let mut world = World::new();
        schedule.run(&mut world);
    }
}
