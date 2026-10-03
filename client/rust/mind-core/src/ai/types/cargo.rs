// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CargoAI` (plan 11 §4.2). Ported from
//! `core/src/mindustry/ai/types/CargoAI.java`: the tether cargo unit shuttles
//! between its spawning building and the assigned drop point. Payload/item
//! transfer itself is plan 08's `PayloadComp`/`BuildingTether` runtime.

use bevy_ecs::entity::Entity;

use crate::content::ItemId;
use crate::entities::comp::Pos;
use crate::entities::comp::building::Building;
use crate::entities::comp::unit::comp::{BuildingTetherComp, ItemsComp, UnitTetherComp};
use crate::world::modules::ItemModule;

use super::super::ai_controller::AiCtx;
use super::super::controller::ControllerSlot;
use super::super::types::ground::tile_center;

/// `CargoAI.moveRange`: arrival distance while docking.
pub const MOVE_RANGE: f32 = 6.0;
/// `CargoAI.transferRange`: range at which item transfer happens.
pub const TRANSFER_RANGE: f32 = 20.0;

/// `CargoAI` controller state.
#[derive(Debug, Default, Clone, Copy)]
pub struct CargoAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Current destination entity (building or drop point).
    pub target: Option<Entity>,
    /// Unload point currently targeted while carrying (`unloadTarget`).
    pub unload_target: Option<Entity>,
    /// Item the unit is shuttling (`itemTarget`).
    pub item_target: Option<ItemId>,
}

/// `CargoAI.updateUnit`/`updateMovement`: shuttle items between the spawning
/// loader and the assigned unload point. Returns `true` on arrival.
///
/// The item half (`Call.takeItems`/`Call.transferItemTo` in upstream) is ported
/// here through plan-08's `ItemModule` transfer primitives; the `UnitCargoUnloadPoint`
/// `stale` gating is honoured when the destination carries that component.
pub fn update_cargo(ctx: &mut AiCtx, unit: Entity, state: &mut CargoAi) -> bool {
    let carrying = ctx
        .world
        .get::<ItemsComp>(unit)
        .map(|items| items.item)
        .unwrap_or(None);

    // While carrying a load, prefer the assigned target (an unload point);
    // otherwise dock at the spawning building.
    let dest_tile = if carrying.is_some() {
        ctx.world
            .get::<ControllerSlot>(unit)
            .and_then(|slot| slot.target)
            .or_else(|| tether_tile(ctx, unit))
    } else {
        tether_tile(ctx, unit).or_else(|| {
            ctx.world
                .get::<ControllerSlot>(unit)
                .and_then(|slot| slot.target)
        })
    };

    let Some(tile) = dest_tile else {
        // No destination while carrying: dump the load (`unit.clearItem()`).
        if carrying.is_some()
            && let Some(mut items) = ctx.world.get_mut::<ItemsComp>(unit)
        {
            items.clear_item();
        }
        ctx.stop_shooting(unit);
        return false;
    };
    let dest_build = find_building_at(ctx, tile);
    state.unload_target = dest_build;
    state.item_target = carrying.map(|(item, _)| item);

    let (cx, cy) = tile_center(tile.x() as i32, tile.y() as i32);
    let distance = ctx
        .world
        .get::<Pos>(unit)
        .map(|pos| ((pos.x - cx).powi(2) + (pos.y - cy).powi(2)).sqrt())
        .unwrap_or(f32::MAX);
    let arrived = ctx.move_direct(unit, cx, cy, MOVE_RANGE);

    if distance <= TRANSFER_RANGE
        && let Some(building) = dest_build
    {
        if let Some((item, amount)) = carrying {
            deposit(ctx, unit, building, item, amount);
        } else {
            pickup(ctx, unit, building);
        }
    }
    arrived
}

/// `Call.transferItemTo`: move up to the destination's free capacity into it.
fn deposit(ctx: &mut AiCtx, unit: Entity, building: Entity, item: ItemId, amount: i32) -> i32 {
    let capacity = crate::world::blocks::distribution::transfer::default_accept_stack(
        ctx.world,
        building,
        item,
        amount,
        Some(unit),
    );
    if capacity <= 0 {
        return 0;
    }
    let cap = crate::world::blocks::distribution::transfer::item_capacity(ctx.world, building);
    let moved = if let Some(mut items) = ctx.world.get_mut::<ItemModule>(building) {
        items.add(item, capacity, cap)
    } else {
        0
    };
    if moved > 0
        && let Some(mut unit_items) = ctx.world.get_mut::<ItemsComp>(unit)
    {
        match unit_items.item {
            Some((held, stack)) if held == item => {
                let left = stack - moved;
                if left <= 0 {
                    unit_items.clear_item();
                } else {
                    unit_items.item = Some((item, left));
                }
            }
            _ => {}
        }
    }
    moved
}

/// `Call.takeItems`: move one item from the loader building into the unit.
fn pickup(ctx: &mut AiCtx, unit: Entity, building: Entity) -> i32 {
    let Some(item) = ctx
        .world
        .get::<ItemModule>(building)
        .and_then(|items| items.first())
    else {
        return 0;
    };
    let unit_capacity = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .map(|def| def.item_capacity.max(0))
        .unwrap_or(0);
    let held = ctx
        .world
        .get::<ItemsComp>(unit)
        .and_then(|items| items.item)
        .map(|(_, amount)| amount)
        .unwrap_or(0);
    let room = (unit_capacity - held).max(0);
    if room <= 0 {
        return 0;
    }
    let available = ctx
        .world
        .get::<ItemModule>(building)
        .map(|items| items.get(item))
        .unwrap_or(0);
    let take = available.min(room);
    if take <= 0 {
        return 0;
    }
    if let Some(mut items) = ctx.world.get_mut::<ItemModule>(building) {
        items.remove(item, take);
    }
    if let Some(mut unit_items) = ctx.world.get_mut::<ItemsComp>(unit) {
        unit_items.add_item(item, take);
    }
    take
}

/// Building entity whose center tile is `target`, if any.
fn find_building_at(ctx: &mut AiCtx, target: crate::world::TilePos) -> Option<Entity> {
    let mut query = ctx.world.query::<(Entity, &Building)>();
    query
        .iter(ctx.world)
        .find(|(_, building)| building.tile == target)
        .map(|(entity, _)| entity)
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
            ..Default::default()
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

    #[test]
    fn cargo_deposits_item_into_target_building() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.build.place(10, 10, container, 0, true));
        let building = harness.build.build_at(10, 10).expect("building");
        let copper = harness.content().item_by_name("copper").expect("copper").id;
        let unit = harness
            .spawn("manifold", 0, 80.0, 80.0, 0.0)
            .expect("manifold");
        // Load the unit and issue the drop command to the container tile.
        harness.build.world.get_mut::<ItemsComp>(unit).unwrap().item = Some((copper, 5));
        harness.command_move(unit, 10, 10);
        let before = harness
            .build
            .world
            .get::<ItemModule>(building)
            .map(|items| items.get(copper))
            .unwrap_or(0);

        let mut state = CargoAi {
            unit: Some(unit),
            ..Default::default()
        };
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        update_cargo(&mut ctx, unit, &mut state);

        let after = harness
            .build
            .world
            .get::<ItemModule>(building)
            .map(|items| items.get(copper))
            .unwrap_or(0);
        assert_eq!(after - before, 5, "item deposited into the unload point");
        assert!(
            harness
                .build
                .world
                .get::<ItemsComp>(unit)
                .unwrap()
                .item
                .is_none(),
            "unit emptied"
        );
    }

    #[test]
    fn cargo_picks_item_up_from_loader() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let container = harness.content().block_id("container").expect("container");
        assert!(harness.build.place(10, 10, container, 0, true));
        let building = harness.build.build_at(10, 10).expect("building");
        let copper = harness.content().item_by_name("copper").expect("copper").id;
        harness
            .build
            .world
            .get_mut::<ItemModule>(building)
            .unwrap()
            .add(copper, 40, 100);
        let unit = harness
            .spawn("manifold", 0, 80.0, 80.0, 0.0)
            .expect("manifold");
        // Empty unit, target the loader tile from its spawning building.
        harness.command_move(unit, 10, 10);
        let mut state = CargoAi {
            unit: Some(unit),
            ..Default::default()
        };
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        update_cargo(&mut ctx, unit, &mut state);

        let held = harness.build.world.get::<ItemsComp>(unit).unwrap().item;
        let (item, amount) = held.expect("picked up copper");
        assert_eq!(item, copper);
        assert!(amount > 0 && amount <= 40, "took up to the unit capacity");
        let remaining = harness
            .build
            .world
            .get::<ItemModule>(building)
            .map(|items| items.get(copper))
            .unwrap_or(0);
        assert_eq!(remaining, 40 - amount, "loader debited by the taken amount");
    }
}
