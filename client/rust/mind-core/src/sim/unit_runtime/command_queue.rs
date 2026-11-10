// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live command-queue waypoint advance for `CommandAI` units.
//! Upstream: core/src/mindustry/ai/types/CommandAI.java (finishPath)

use super::*;

/// Pops the next queued waypoint onto the unit's target
/// (`CommandAI.finishPath`: `targetPos = null` then the queue head,
/// `CommandAI.java:476-490`; I01 T13).
pub(super) fn pop_live_command_queue(world: &mut World, grid: &WorldGrid, entity: Entity) {
    use crate::ai::types::command::{AttackTargetComp, CommandQueueComp, CommandQueueEntry};
    let next = {
        let Some(mut queued) = world.get_mut::<CommandQueueComp>(entity) else {
            return;
        };
        if queued.0.is_empty() {
            return;
        }
        queued.0.remove(0)
    };
    match next {
        CommandQueueEntry::Position(x, y) => {
            let tile = TilePos::new(WorldGrid::to_tile(x) as i16, WorldGrid::to_tile(y) as i16);
            if let Some(mut slot) = world.get_mut::<ControllerSlot>(entity) {
                slot.target = Some(tile);
            }
        }
        CommandQueueEntry::Unit(id) => {
            let target = unit_by_id(world, id);
            if let Some(mut stored) = world.get_mut::<AttackTargetComp>(entity) {
                stored.0 = target;
            } else {
                world.entity_mut(entity).insert(AttackTargetComp(target));
            }
            let tile = target.and_then(|target| {
                world.get::<crate::entities::comp::Pos>(target).map(|pos| {
                    TilePos::new(
                        WorldGrid::to_tile(pos.x) as i16,
                        WorldGrid::to_tile(pos.y) as i16,
                    )
                })
            });
            if let Some(mut slot) = world.get_mut::<ControllerSlot>(entity) {
                slot.target = tile;
            }
        }
        CommandQueueEntry::Building(packed) => {
            let pos = WorldGrid::unpack(packed);
            let target = grid
                .tiles
                .getn(pos.x() as i32, pos.y() as i32)
                .and_then(|tile| tile.build);
            if let Some(mut stored) = world.get_mut::<AttackTargetComp>(entity) {
                stored.0 = target;
            } else {
                world.entity_mut(entity).insert(AttackTargetComp(target));
            }
            if let Some(mut slot) = world.get_mut::<ControllerSlot>(entity) {
                slot.target = Some(pos);
            }
        }
    }
}
