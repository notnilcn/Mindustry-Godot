// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CargoAI` (plan 11 §4.2). Ported from
//! `core/src/mindustry/ai/types/CargoAI.java`: the tether cargo unit shuttles
//! between its spawning building and the assigned drop point. Payload/item
//! transfer itself is plan 08's `PayloadComp`/`BuildingTether` runtime.

use bevy_ecs::entity::Entity;

use crate::entities::comp::Pos;
use crate::entities::comp::building::Building;
use crate::entities::comp::unit::comp::{BuildingTetherComp, ItemsComp, UnitTetherComp};

use super::super::ai_controller::AiCtx;
use super::super::controller::ControllerSlot;
use super::super::types::ground::tile_center;

/// `CargoAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current destination entity (building or drop point).
    pub target: Option<Entity>,
}

/// `CargoAI.updateUnit`: fly toward the tether building while empty, otherwise
/// toward the assigned drop point. Returns `true` on arrival.
pub fn update_cargo(ctx: &mut AiCtx, unit: Entity, _state: &mut CargoAi) -> bool {
    let carrying = ctx
        .world
        .get::<ItemsComp>(unit)
        .map(|items| items.item.is_some())
        .unwrap_or(false);

    // While carrying a load, prefer the assigned target; otherwise dock at the
    // spawning building (the loader/unload point is plan-08's runtime).
    let dest_tile = if carrying {
        ctx.world
            .get::<ControllerSlot>(unit)
            .and_then(|slot| slot.target)
    } else {
        tether_tile(ctx, unit).or_else(|| {
            ctx.world
                .get::<ControllerSlot>(unit)
                .and_then(|slot| slot.target)
        })
    };

    let Some(tile) = dest_tile else {
        ctx.stop_shooting(unit);
        return false;
    };
    let (cx, cy) = tile_center(tile.x() as i32, tile.y() as i32);
    let arrive = ctx
        .world
        .get::<crate::entities::comp::unit::HitboxComp>(unit)
        .map(|hitbox| hitbox.hit_size.max(4.0))
        .unwrap_or(4.0);
    let arrived = ctx.move_direct(unit, cx, cy, arrive);
    // Drop the payload into the building when docked.
    if arrived && carrying {
        // Owner note (plan 08): `CargoAI`'s drop half is `Call.transferItemTo` /
        // `PayloadComp.tryDropPayload` into the docked `UnitCargoUnloadPointBuild`.
        // Plan 08 exposes the payload/cargo transfer API; this callsite is wired
        // to it once the unload-point building runtime lands (plan 11 §2.3).
    }
    arrived
}

/// Tile of the unit's spawning/docked building, if resolved.
fn tether_tile(ctx: &mut AiCtx, unit: Entity) -> Option<crate::world::TilePos> {
    if let Some(spawner) = ctx
        .world
        .get::<UnitTetherComp>(unit)
        .and_then(|tether| tether.spawner)
        && let Some(building) = ctx.world.get::<Building>(spawner)
    {
        return Some(building.tile);
    }
    if let Some(building_entity) = ctx
        .world
        .get::<BuildingTetherComp>(unit)
        .and_then(|tether| tether.building)
        && let Some(building) = ctx.world.get::<Building>(building_entity)
    {
        return Some(building.tile);
    }
    None
}

#[allow(dead_code)]
fn target_pos(ctx: &mut AiCtx, entity: Entity) -> Option<(f32, f32)> {
    ctx.world.get::<Pos>(entity).map(|pos| (pos.x, pos.y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn cargo_moves_toward_assigned_target() {
        let mut harness = UnitHarness::new(64, 64, 1);
        let unit = harness
            .spawn("manifold", 0, 64.0, 64.0, 0.0)
            .expect("manifold");
        harness.command_move(unit, 80, 80);
        let mut state = CargoAi {
            unit: Some(unit),
            target: None,
        };
        let mut moved = false;
        for _ in 0..600 {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            update_cargo(&mut ctx, unit, &mut state);
            let snap = harness.snapshot(unit).expect("alive");
            if snap.x > 70.0 {
                moved = true;
                break;
            }
        }
        assert!(moved, "cargo unit moved toward the drop point");
    }
}
