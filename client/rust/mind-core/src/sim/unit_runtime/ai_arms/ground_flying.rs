// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Ground/flying fallback dispatch arm: the final `else` of the chain, which
//! `GroundAI` and `FlyingAI` share.
//! Upstream: core/src/mindustry/ai/types/GroundAI.java
//! core/src/mindustry/ai/types/FlyingAI.java

use super::super::ai::flying::update_flying_ai;
use super::super::ai::ground::{GroundAiRules, update_ground_ai};
use super::*;

/// The `else` arm: a `Flying` slot (or a flying body) runs `FlyingAI`, every
/// other kind runs `GroundAI`.
#[allow(clippy::too_many_arguments)]
pub(in crate::sim::unit_runtime) fn ground_flying_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
    slot: ControllerSlot,
    ground_rules: GroundAiRules,
    core_target_cache: &mut BTreeMap<u8, Vec<TilePos>>,
) {
    let flying = slot.kind == AiKind::Flying
        || world
            .get::<PhysicsComp>(entity)
            .is_some_and(|physics| physics.flying);
    if flying {
        // `FlyingAI` (FlyingAI.java:16-72): main target via
        // `findMainTarget` (core in range, else `targetFlags`, else the
        // core), circle/close on it, shuttle to the spawner when idle
        // on the default team.
        update_flying_ai(
            world,
            &mut *runtime,
            queries,
            entity,
            team,
            slot,
            ground_rules,
        );
    } else {
        // `GroundAI.updateMovement` (GroundAI.java:21-85): field walk to
        // the enemy-core set with the Java targeting cadence.
        update_ground_ai(
            world,
            &mut *runtime,
            queries,
            entity,
            team,
            slot,
            ground_rules,
            &mut *core_target_cache,
        );
    }
}
