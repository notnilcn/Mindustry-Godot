// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Player` dispatch arm: the host-uploaded movement axis drives the unit.
//! Upstream: core/src/mindustry/input/DesktopInput.java (updateMovement)
//! core/src/mindustry/entities/comp/UnitComp.java (movePref/moveAt/rotateMove)

use super::*;

/// `Player` possession dispatch (`UnitComp.update`, player branch).
pub(in crate::sim::unit_runtime) fn player_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    entity: Entity,
) {
    // `Player` possession: the host-uploaded axis drives the unit
    // (`DesktopInput.updateMovement`); no target acquisition or pathing.
    crate::ai::types::player::update_player(world, &runtime.grid, &runtime.content, entity);
}
