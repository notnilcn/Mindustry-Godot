// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `CargoAI` (plan 11 §4.2, dispatched in H01 T13). Ported from
//! `core/src/mindustry/ai/types/CargoAI.java`: the tether cargo unit shuttles
//! between its spawning building (`UnitCargoLoaderBuild`) and a configured
//! `UnitCargoUnloadPoint`. The item transfer itself runs through
//! `world::blocks::distribution::transfer` (`Call.takeItems`/
//! `Call.transferItemTo`).
//!
//! Deviations (documented): the flagged-building scan walks the world's live
//! `UnitCargoUnloadPoint` entities in entity-index order (Java's
//! `indexer.getFlagged` order is the indexer's insertion order); the unload
//! point's `stale` read uses the port's `stale_time <= 0` model (the block
//! behavior in `world/blocks/units` owns the Java `staleTimer`/`stale` pair);
//! `isPayload()` is always false (the port's unload point is never a payload
//! block).

use bevy_ecs::entity::Entity;

use crate::content::ItemId;
use crate::entities::comp::Pos;
use crate::entities::comp::building::Building;
use crate::entities::comp::unit::comp::{BuildingTetherComp, ItemsComp, UnitTetherComp};
use crate::world::blocks::units::UnitCargoUnloadPoint;
use crate::world::modules::ItemModule;

use super::super::ai_controller::AiCtx;

/// `CargoAI.moveRange`: arrival distance while docking.
pub const MOVE_RANGE: f32 = 6.0;
/// `CargoAI.transferRange`: range at which item transfer happens.
pub const TRANSFER_RANGE: f32 = 20.0;
/// `CargoAI.moveSmoothing` (`moveTo(.., moveRange, moveSmoothing)`).
pub const MOVE_SMOOTHING: f32 = 20.0;
/// `CargoAI.emptyWaitTime` (`60 * 2`).
pub const EMPTY_WAIT_TIME: f32 = 120.0;
/// `CargoAI.dropSpacing` (`60 * 1.5`).
pub const DROP_SPACING: f32 = 90.0;
/// `retarget()`: `timer.get(timerTarget, target == null ? 40 : 90)`; the
/// CargoAI `AIController.target` stays null, so 40 ticks.
pub const RETARGET_INTERVAL: f32 = 40.0;

/// `CargoAI` controller state (H01 T13; persisted per unit).
#[derive(Debug, Default, Clone, Copy, bevy_ecs::component::Component)]
pub struct CargoAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// `AIController.target` (always `None` for CargoAI).
    pub target: Option<Entity>,
    /// Unload point currently targeted while carrying (`unloadTarget`).
    pub unload_target: Option<Entity>,
    /// Item the unit is shuttling (`itemTarget`).
    pub item_target: Option<ItemId>,
    /// `noDestTimer`: time spent waiting for space at the current target.
    pub no_dest_timer: f32,
    /// `targetIndex`: next destination search offset.
    pub target_index: i32,
    /// `timer.get(timerTarget, 40)` countdown.
    pub retarget_timer: f32,
    /// `timer.get(timerTarget2, dropSpacing)` countdown.
    pub drop_timer: f32,
}

/// `CargoAI.updateMovement` (`CargoAI.java:26-97`): shuttle items between the
/// tether loader and the assigned unload point. Returns `true` when docked.
pub fn update_cargo(ctx: &mut AiCtx, unit: Entity, state: &mut CargoAi) -> bool {
    // `if(!(unit instanceof BuildingTetherc tether) || tether.building() == null) return;`
    let Some(loader) = tether_building(ctx, unit) else {
        return false;
    };
    // `if(build.items == null) return;`
    if ctx.world.get::<ItemModule>(loader).is_none() {
        return false;
    }
    let team = ctx
        .world
        .get::<crate::entities::comp::TeamComp>(unit)
        .map(|t| t.team)
        .unwrap_or(0);
    let carried = ctx
        .world
        .get::<ItemsComp>(unit)
        .and_then(|items| items.item);
    let Some((lx, ly)) = building_center(ctx, loader) else {
        return false;
    };

    if carried.is_none() {
        // Empty, approach the loader, even if there's nothing to pick up.
        crate::ai::types::flying::move_to_point(
            ctx.world,
            ctx.grid,
            ctx.content,
            unit,
            lx,
            ly,
            MOVE_RANGE,
            MOVE_SMOOTHING,
            false,
        );

        let loader_has_items = ctx
            .world
            .get::<ItemModule>(loader)
            .is_some_and(|items| items.total > 0);
        if loader_has_items && within(ctx, unit, lx, ly, TRANSFER_RANGE) {
            state.retarget_timer -= 1.0;
            if state.retarget_timer <= 0.0 {
                state.retarget_timer = RETARGET_INTERVAL;
                find_any_target(ctx, team, loader, state);

                // Target found, grab items and go.
                if let Some(item) = state.item_target
                    && state.unload_target.is_some()
                {
                    let available = ctx
                        .world
                        .get::<ItemModule>(loader)
                        .map(|items| items.get(item))
                        .unwrap_or(0);
                    let capacity = unit_item_capacity(ctx, unit);
                    pickup(ctx, unit, loader, item, available.min(capacity));
                }
            }
        }
    } else {
        let Some((item, stack)) = carried else {
            return false;
        };

        if state.unload_target.is_none() {
            state.retarget_timer -= 1.0;
            if state.retarget_timer <= 0.0 {
                state.retarget_timer = RETARGET_INTERVAL;
                find_drop_target(ctx, team, item, 0, state);

                // If there is not even a single place to unload, dump items.
                if state.unload_target.is_none() {
                    clear_item(ctx, unit);
                }
            }
        } else {
            let target = state.unload_target.unwrap_or(loader);

            // The target may have been reconfigured or picked up while the
            // unit was moving (`CargoAI.java:64-67`).
            let point_item = ctx
                .world
                .get::<UnitCargoUnloadPoint>(target)
                .and_then(|point| point.item);
            if point_item != state.item_target {
                state.unload_target = None;
                return false;
            }

            let Some((tx, ty)) = building_center(ctx, target) else {
                state.unload_target = None;
                return false;
            };
            crate::ai::types::flying::move_to_point(
                ctx.world,
                ctx.grid,
                ctx.content,
                unit,
                tx,
                ty,
                MOVE_RANGE,
                MOVE_SMOOTHING,
                false,
            );

            // Deposit in bursts, unloading can take a while.
            if within(ctx, unit, tx, ty, TRANSFER_RANGE) {
                state.drop_timer -= 1.0;
                if state.drop_timer <= 0.0 {
                    state.drop_timer = DROP_SPACING;
                    let max = crate::world::blocks::distribution::transfer::default_accept_stack(
                        ctx.world,
                        target,
                        item,
                        stack,
                        Some(unit),
                    );
                    if max > 0 {
                        deposit(ctx, unit, target, item, max);
                    }

                    let has_item = ctx
                        .world
                        .get::<ItemsComp>(unit)
                        .is_some_and(|items| items.item.is_some());
                    state.no_dest_timer += DROP_SPACING;
                    if !has_item || state.no_dest_timer >= EMPTY_WAIT_TIME {
                        // Out of space: wait, then try the next destination.
                        state.target_index =
                            find_drop_target(ctx, team, item, state.target_index, state);
                        state.no_dest_timer = 0.0;
                        if state.unload_target.is_none() {
                            clear_item(ctx, unit);
                        }
                    }
                }
            }
        }
    }
    false
}

/// `CargoAI.findAnyTarget(Building build)`: choose the loader item that has an
/// unload point, preferring non-stale destinations.
fn find_any_target(ctx: &mut AiCtx, team: u8, loader: Entity, state: &mut CargoAi) {
    state.unload_target = None;
    state.item_target = None;

    let base = unload_points(ctx, team);
    if base.is_empty() {
        return;
    }
    let Some(items) = ctx.world.get::<ItemModule>(loader).cloned() else {
        return;
    };
    // `orderedItems.sort(i -> -build.items.get(i))`: most items first; stable
    // for equal counts (content order).
    let mut ordered: Vec<(ItemId, i32)> = ctx
        .content
        .items()
        .iter()
        .map(|def| def.id)
        .filter_map(|item| {
            let amount = items.get(item);
            (amount > 0).then_some((item, amount))
        })
        .collect();
    ordered.sort_by_key(|a| std::cmp::Reverse(a.1));

    let mut last_stale: Option<Entity> = None;
    'outer: for (item, _) in ordered {
        let targets: Vec<Entity> = base
            .iter()
            .copied()
            .filter(|target| {
                ctx.world
                    .get::<UnitCargoUnloadPoint>(*target)
                    .is_some_and(|point| point.item == Some(item))
            })
            .collect();
        if !targets.is_empty() {
            state.item_target = Some(item);
        }
        let len = targets.len() as i32;
        for i in 0..len {
            let target = targets[((i + state.target_index).rem_euclid(len)) as usize];
            last_stale = Some(target);
            if !point_stale(ctx, target) {
                state.unload_target = Some(target);
                break 'outer;
            }
        }
    }
    // If the only thing found was a stale target, at least try that.
    if state.unload_target.is_none()
        && let Some(target) = last_stale
    {
        state.unload_target = Some(target);
    }
}

/// `CargoAI.findDropTarget(item, offset, ignore)`: the next unload point for
/// `item` after `offset`, preferring non-stale destinations. Returns the new
/// `targetIndex`.
fn find_drop_target(
    ctx: &mut AiCtx,
    team: u8,
    item: ItemId,
    offset: i32,
    state: &mut CargoAi,
) -> i32 {
    state.unload_target = None;
    state.item_target = Some(item);

    let targets: Vec<Entity> = unload_points(ctx, team)
        .into_iter()
        .filter(|target| {
            ctx.world
                .get::<UnitCargoUnloadPoint>(*target)
                .is_some_and(|point| point.item == Some(item))
        })
        .collect();
    if targets.is_empty() {
        return 0;
    }
    let len = targets.len() as i32;
    for i in 0..len {
        let index = (i + offset + 1).rem_euclid(len);
        let target = targets[index as usize];
        if !point_stale(ctx, target) {
            state.unload_target = Some(target);
            return index;
        }
    }
    // A stale target is better than nothing.
    state.unload_target = Some(targets[0]);
    0
}

/// `indexer.getFlagged(team, BlockFlag.unitCargoUnloadPoint)` over the live
/// world, in entity-index order.
fn unload_points(ctx: &mut AiCtx, team: u8) -> Vec<Entity> {
    let mut query = ctx.world.query::<(
        Entity,
        &UnitCargoUnloadPoint,
        &crate::entities::comp::TeamComp,
    )>();
    let mut out: Vec<Entity> = query
        .iter(ctx.world)
        .filter(|(_, _, owner)| owner.team == team)
        .map(|(entity, _, _)| entity)
        .collect();
    out.sort_by_key(|entity| entity.index());
    out
}

/// `UnitCargoUnloadPointBuild.stale` (port model: the stale timer ran out).
fn point_stale(ctx: &AiCtx, target: Entity) -> bool {
    ctx.world
        .get::<UnitCargoUnloadPoint>(target)
        .is_some_and(|point| point.stale_time <= 0.0)
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

/// `Call.takeItems(build, item, amount, unit)`: move `amount` of `item` from
/// the loader into the unit.
fn pickup(ctx: &mut AiCtx, unit: Entity, building: Entity, item: ItemId, amount: i32) -> i32 {
    let unit_capacity = unit_item_capacity(ctx, unit);
    let held = ctx
        .world
        .get::<ItemsComp>(unit)
        .and_then(|items| items.item)
        .map(|(_, amount)| amount)
        .unwrap_or(0);
    let room = (unit_capacity - held).max(0);
    let available = ctx
        .world
        .get::<ItemModule>(building)
        .map(|items| items.get(item))
        .unwrap_or(0);
    let take = amount.max(0).min(available).min(room);
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

/// `unit.clearItem()`.
fn clear_item(ctx: &mut AiCtx, unit: Entity) {
    if let Some(mut items) = ctx.world.get_mut::<ItemsComp>(unit) {
        items.clear_item();
    }
}

fn unit_item_capacity(ctx: &AiCtx, unit: Entity) -> i32 {
    ctx.unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .map(|def| def.item_capacity.max(0))
        .unwrap_or(0)
}

fn within(ctx: &AiCtx, unit: Entity, x: f32, y: f32, range: f32) -> bool {
    ctx.world.get::<Pos>(unit).is_some_and(|pos| {
        let dx = pos.x - x;
        let dy = pos.y - y;
        dx * dx + dy * dy <= range * range
    })
}

/// Centre (`Building.x/y`) of a building entity.
fn building_center(ctx: &AiCtx, entity: Entity) -> Option<(f32, f32)> {
    ctx.world.get::<Pos>(entity).map(|pos| (pos.x, pos.y))
}

/// `BuildingTetherComp.building` (or the `UnitTetherComp.spawner` fallback).
fn tether_building(ctx: &AiCtx, unit: Entity) -> Option<Entity> {
    if let Some(building) = ctx
        .world
        .get::<BuildingTetherComp>(unit)
        .and_then(|tether| tether.building)
        && ctx.world.get::<Building>(building).is_some()
    {
        return Some(building);
    }
    if let Some(spawner) = ctx
        .world
        .get::<UnitTetherComp>(unit)
        .and_then(|tether| tether.spawner)
        && ctx.world.get::<Building>(spawner).is_some()
    {
        return Some(spawner);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;
    use crate::entities::comp::unit::comp::UnitTetherComp;

    /// Places loader + configured unload point and returns both entities.
    fn cargo_fixture(harness: &mut UnitHarness, unload_item: &str) -> (Entity, Entity, Entity) {
        let loader_id = harness
            .content()
            .block_id("unit-cargo-loader")
            .expect("loader");
        assert!(harness.build.place(10, 10, loader_id, 0, true));
        let loader = harness.build.build_at(10, 10).expect("loader build");
        let point_id = harness
            .content()
            .block_id("unit-cargo-unload-point")
            .expect("unload point");
        assert!(harness.build.place(20, 10, point_id, 0, true));
        let point = harness.build.build_at(20, 10).expect("unload build");
        let item = harness
            .content()
            .item_by_name(unload_item)
            .expect("item")
            .id;
        harness
            .build
            .world
            .get_mut::<UnitCargoUnloadPoint>(point)
            .expect("point state")
            .item = Some(item);
        let unit = harness
            .spawn("manifold", 0, 64.0, 64.0, 0.0)
            .expect("manifold");
        // `UnitCargoLoaderBuild` binds the spawned unit (`bt.building(this)`).
        harness
            .build
            .world
            .entity_mut(unit)
            .insert(BuildingTetherComp {
                building: Some(loader),
            });
        harness.build.world.entity_mut(unit).insert(UnitTetherComp {
            spawner: Some(loader),
        });
        (loader, point, unit)
    }

    #[test]
    fn cargo_moves_toward_tethered_loader() {
        let mut harness = UnitHarness::new(64, 64, 1);
        let (_, _, unit) = cargo_fixture(&mut harness, "copper");
        let mut state = CargoAi {
            unit: Some(unit),
            ..Default::default()
        };
        let mut moved = false;
        for _ in 0..600 {
            harness.vel_update(unit);
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            update_cargo(&mut ctx, unit, &mut state);
            if harness.snapshot(unit).expect("alive").x > 70.0 {
                moved = true;
                break;
            }
        }
        assert!(moved, "cargo unit moved toward its tethered loader");
    }

    #[test]
    fn cargo_deposits_item_into_unload_point() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let (_, point, unit) = cargo_fixture(&mut harness, "copper");
        let copper = harness.content().item_by_name("copper").expect("copper").id;
        harness.build.world.get_mut::<ItemsComp>(unit).unwrap().item = Some((copper, 5));
        // Move the unit onto the unload point so the transfer range passes.
        harness.build.world.get_mut::<Pos>(unit).unwrap().x = 160.0;
        harness.build.world.get_mut::<Pos>(unit).unwrap().y = 80.0;

        let mut state = CargoAi {
            unit: Some(unit),
            ..Default::default()
        };
        for _ in 0..3 {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            update_cargo(&mut ctx, unit, &mut state);
        }

        let stored = harness
            .build
            .world
            .get::<ItemModule>(point)
            .map(|items| items.get(copper))
            .unwrap_or(0);
        assert_eq!(stored, 5, "item deposited into the unload point");
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
        assert_eq!(state.unload_target, Some(point));
    }

    #[test]
    fn cargo_picks_item_up_from_loader() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let (loader, point, unit) = cargo_fixture(&mut harness, "copper");
        let copper = harness.content().item_by_name("copper").expect("copper").id;
        harness
            .build
            .world
            .get_mut::<ItemModule>(loader)
            .unwrap()
            .add(copper, 40, 100);
        // Move the unit onto the loader centre (80, 80) so the transfer
        // range passes.
        harness.build.world.get_mut::<Pos>(unit).unwrap().x = 80.0;
        harness.build.world.get_mut::<Pos>(unit).unwrap().y = 80.0;

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
            .get::<ItemModule>(loader)
            .map(|items| items.get(copper))
            .unwrap_or(0);
        assert_eq!(remaining, 40 - amount, "loader debited by the taken amount");
        assert_eq!(state.item_target, Some(copper));
        assert_eq!(state.unload_target, Some(point));
    }

    /// No tether: `CargoAI` early-returns and never dumps the load
    /// (`CargoAI.java:28`).
    #[test]
    fn cargo_without_tether_keeps_its_item() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness
            .spawn("manifold", 0, 80.0, 80.0, 0.0)
            .expect("manifold");
        let copper = harness.content().item_by_name("copper").expect("copper").id;
        harness.build.world.get_mut::<ItemsComp>(unit).unwrap().item = Some((copper, 5));
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
        assert_eq!(
            harness.build.world.get::<ItemsComp>(unit).unwrap().item,
            Some((copper, 5)),
            "no tether -> the unit keeps its item"
        );
    }
}
