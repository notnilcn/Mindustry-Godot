// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Turret ammo accept/handle/use (`TurretBuild.acceptItem`/`handleItem`/`useAmmo`).
//! Upstream: core/src/mindustry/world/blocks/defense/turrets/{ItemTurret,LiquidTurret,PowerTurret}.java

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BulletId, ItemId, LiquidId};
use crate::world::PayloadRef;
use crate::world::modules::{LiquidModule, PowerModule};
use crate::world::plan::BuildPlan;

use super::math::{cheating, position};
use super::{
    AmmoEntry, LiquidAmmo, PayloadContent, PayloadStack, TurretAmmo, TurretConfig, TurretKind,
    TurretState, turret,
};

// ---- ammo ----

/// `TurretBuild.peekAmmo`: the bullet that the next `use_ammo` returns.
pub fn peek_ammo(world: &World, e: Entity) -> Option<BulletId> {
    let state = turret(world, e)?;
    peek_ammo_state(state, world, e)
}

pub(super) fn peek_ammo_state(state: &TurretState, world: &World, e: Entity) -> Option<BulletId> {
    match &state.config.ammo {
        TurretAmmo::Item(_) => state.ammo.last().map(|entry| entry.bullet),
        // `LiquidTurretBuild.peekAmmo` / `ContinuousLiquidTurretBuild.peekAmmo`
        // (`LiquidTurret.java:136-139`, `ContinuousLiquidTurret.java:118-121`):
        // `ammoTypes.get(liquids.current())`, independent of the stored amount.
        TurretAmmo::Liquid(ammo) => {
            let liquids = world.get::<LiquidModule>(e)?;
            current_liquid_ammo(liquids, ammo).map(|entry| entry.bullet)
        }
        TurretAmmo::Power(ammo) => Some(ammo.bullet),
        TurretAmmo::Payload(ammo) => ammo
            .iter()
            .find(|entry| {
                state
                    .payloads
                    .iter()
                    .any(|stack| stack.content == entry.content && stack.count > 0)
            })
            .map(|entry| entry.bullet),
    }
}

/// `TurretBuild.peekAmmo().rangeChange` (`BulletType.rangeChange`).
///
/// Mirrors [`peek_ammo_state`]'s selection: the last item entry, the entry of
/// `liquids.current()`, the power bullet def or the first stocked payload entry
/// (`Power` resolves the def from the runtime content because the ammo enum
/// keeps only the bullet id).
pub fn peek_ammo_range_change(world: &World, e: Entity) -> f32 {
    let Some(state) = turret(world, e) else {
        return 0.0;
    };
    peek_ammo_range_change_of(state, world, e)
}

/// [`peek_ammo_range_change`] for callers that already hold the dispatched
/// state (the turret pass takes `TurretState` out of the entity for the
/// duration of `update_turret`).
pub fn peek_ammo_range_change_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    match &state.config.ammo {
        // `peekAmmo()` returns the bullet of the last stack (`peek_ammo_state`);
        // its `rangeChange` comes from the matching config entry.
        TurretAmmo::Item(ammo) => state
            .ammo
            .last()
            .and_then(|entry| ammo.iter().find(|config| config.bullet == entry.bullet))
            .map_or(0.0, |config| config.range_change),
        TurretAmmo::Liquid(ammo) => {
            let Some(liquids) = world.get::<LiquidModule>(e) else {
                return 0.0;
            };
            current_liquid_ammo(liquids, ammo).map_or(0.0, |entry| entry.range_change)
        }
        TurretAmmo::Power(ammo) => ammo.range_change,
        TurretAmmo::Payload(ammo) => ammo
            .iter()
            .find(|entry| {
                state
                    .payloads
                    .iter()
                    .any(|stack| stack.content == entry.content && stack.count > 0)
            })
            .map_or(0.0, |entry| entry.range_change),
    }
}

/// `TurretBuild.peekAmmo().minRangeChange` (`BulletType.minRangeChange`):
/// mirrors [`peek_ammo_range_change_of`]'s ammo selection for the minimum
/// range (`Turret.TurretBuild.minRange()`, `Turret.java:340-344`); `Power`
/// resolves the bullet def from the runtime content like
/// [`peek_ammo_range_change_of`].
pub fn peek_ammo_min_range_change_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    match &state.config.ammo {
        TurretAmmo::Item(ammo) => state
            .ammo
            .last()
            .and_then(|entry| ammo.iter().find(|config| config.bullet == entry.bullet))
            .map_or(0.0, |config| config.min_range_change),
        TurretAmmo::Liquid(ammo) => {
            let Some(liquids) = world.get::<LiquidModule>(e) else {
                return 0.0;
            };
            current_liquid_ammo(liquids, ammo).map_or(0.0, |entry| entry.min_range_change)
        }
        TurretAmmo::Power(ammo) => ammo.min_range_change,
        TurretAmmo::Payload(ammo) => ammo
            .iter()
            .find(|entry| {
                state
                    .payloads
                    .iter()
                    .any(|stack| stack.content == entry.content && stack.count > 0)
            })
            .map_or(0.0, |entry| entry.min_range_change),
    }
}

/// `Turret.TurretBuild.range()` (`Turret.java:348-353`): `range +
/// peekAmmo().rangeChange`, the range used by targeting, the range circle, the
/// fog radius and the logic `@range` sensor.
pub fn turret_range(world: &World, e: Entity) -> f32 {
    turret(world, e).map_or(0.0, |state| state.config.range) + peek_ammo_range_change(world, e)
}

/// [`turret_range`] for callers that already hold the dispatched state.
pub fn turret_range_of(state: &TurretState, world: &World, e: Entity) -> f32 {
    state.config.range + peek_ammo_range_change_of(state, world, e)
}

/// `Turret.logicControlCooldown` (`60 * 2` ticks).
pub const LOGIC_CONTROL_COOLDOWN: f32 = 120.0;

/// `TurretBuild.targetPosition(Posc)` for the logic `shootp` path: the same
/// intercept math as [`target_position`], sourced from the world alone (the
/// logic VM has no `CombatCtx`).
pub fn logic_target_position(
    world: &World,
    e: Entity,
    state: &TurretState,
    target: Entity,
) -> Option<(f32, f32)> {
    let (x, y) = position(world, e)?;
    let (tx, ty) = position(world, target)?;
    let speed = world
        .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
        .and_then(|runtime| {
            peek_ammo_state(state, world, e)
                .and_then(|id| runtime.content.bullet(id))
                .map(|def| def.speed)
        })
        .unwrap_or(0.0);
    let delta = world
        .get::<crate::entities::comp::Vel>(target)
        .copied()
        .unwrap_or(crate::entities::comp::Vel { x: 0.0, y: 0.0 });
    let config = &state.config;
    let offset = if config.accurate_delay && !config.move_while_charging {
        (
            delta.x * config.shoot.first_shot_delay,
            delta.y * config.shoot.first_shot_delay,
        )
    } else {
        (0.0, 0.0)
    };
    let aim = if config.predict_target && speed >= 0.01 {
        crate::combat::targeting::intercept_positions(
            (x, y),
            (0.0, 0.0),
            (tx + offset.0, ty + offset.1),
            (delta.x, delta.y),
            true,
            speed,
        )
    } else {
        (tx, ty)
    };
    Some(if aim.0 == 0.0 && aim.1 == 0.0 {
        (tx, ty)
    } else {
        aim
    })
}

/// `TurretBuild.hasAmmo`, including the "swap a usable entry to the back" rule.
pub fn has_ammo(world: &World, e: Entity) -> bool {
    let Some(state) = turret(world, e) else {
        return false;
    };
    has_ammo_state(state, world, e)
}

pub(super) fn has_ammo_state(state: &TurretState, world: &World, e: Entity) -> bool {
    match &state.config.ammo {
        // `Turret.java:715`: `ammo.size > 0 && (ammo.peek().amount >=
        // ammoPerShot || cheating())`.
        TurretAmmo::Item(_) => state
            .ammo
            .last()
            .is_some_and(|entry| entry.amount >= state.config.ammo_per_shot || cheating(world, e)),
        TurretAmmo::Liquid(ammo) => {
            let Some(liquids) = world.get::<LiquidModule>(e) else {
                return false;
            };
            // `ContinuousLiquidTurret.hasAmmo` (`:124-126`; T14): the activated
            // latch plus a correct, non-empty current liquid.
            if state.config.kind == TurretKind::Continuous {
                return state.activated
                    && current_ammo_liquid(liquids, &state.config).is_some()
                    && liquids.current_liquid_amount() > 0.0;
            }
            // `LiquidTurretBuild.hasAmmo` (`LiquidTurret.java:141-144`):
            // `ammoTypes.get(current) != null && currentAmount() >= 1 /
            // ammoTypes.get(current).ammoMultiplier`.
            current_liquid_ammo(liquids, ammo)
                .is_some_and(|entry| liquids.current_liquid_amount() >= 1.0 / entry.ammo_multiplier)
        }
        TurretAmmo::Power(_) => true,
        TurretAmmo::Payload(ammo) => ammo.iter().any(|entry| {
            state
                .payloads
                .iter()
                .any(|stack| stack.content == entry.content && stack.count > 0)
        }),
    }
}

/// `ammoTypes.get(liquids.current())`: the ammo entry of the module's current
/// liquid (`LiquidModule.current()`, the last liquid added/removed/loaded,
/// `LiquidModule.java:89-91`), or `None` when that liquid is not ammo. Java
/// never picks the most-filled slot.
pub(super) fn current_liquid_ammo<'a>(
    liquids: &LiquidModule,
    ammo: &'a [LiquidAmmo],
) -> Option<&'a LiquidAmmo> {
    let current = liquids.current_liquid();
    ammo.iter().find(|entry| entry.liquid == current)
}

/// `liquids.current()` when `ammoTypes` contains it (`ContinuousLiquidTurret`
/// reads `liquids.current()`/`currentAmount`, T14).
pub(super) fn current_ammo_liquid(
    liquids: &LiquidModule,
    config: &TurretConfig,
) -> Option<crate::content::LiquidId> {
    let TurretAmmo::Liquid(ammo) = &config.ammo else {
        return None;
    };
    current_liquid_ammo(liquids, ammo).map(|entry| entry.liquid)
}

/// `ConsumeLiquidFilter.getConsumed` (`ConsumeLiquidFilter.java:66-80`): the
/// current liquid when it passes the ammo filter and is non-empty, else the
/// lowest-id ammo liquid with a positive amount.
pub(super) fn consumed_ammo_liquid(
    liquids: &LiquidModule,
    config: &TurretConfig,
) -> Option<crate::content::LiquidId> {
    if liquids.current_liquid_amount() > 0.0
        && let Some(liquid) = current_ammo_liquid(liquids, config)
    {
        return Some(liquid);
    }
    let TurretAmmo::Liquid(ammo) = &config.ammo else {
        return None;
    };
    ammo.iter()
        .map(|entry| entry.liquid)
        .filter(|liquid| liquids.get(*liquid) > 0.0)
        .min_by_key(|liquid| liquid.index())
}

/// `TurretBuild.canConsume`: power/heat gates (item/liquid gate handled by
/// `has_ammo`).
pub(super) fn can_consume(
    state: &TurretState,
    world: &World,
    e: Entity,
    config: &TurretConfig,
) -> bool {
    if config.heat_requirement > 0.0 && state.heat_req <= 0.0 {
        return false;
    }
    if matches!(config.ammo, TurretAmmo::Power(_)) {
        return world
            .get::<PowerModule>(e)
            .map(|power| power.status > 0.0)
            .unwrap_or(false);
    }
    true
}

/// `ItemTurretBuild.acceptItem`.
pub fn accept_item(world: &World, e: Entity, item: ItemId) -> bool {
    let Some(state) = turret(world, e) else {
        return false;
    };
    let TurretAmmo::Item(ammo) = &state.config.ammo else {
        return false;
    };
    let Some(entry) = ammo.iter().find(|entry| entry.item == item) else {
        return false;
    };
    state.total_ammo + entry.ammo_multiplier <= state.config.max_ammo
}

/// `ItemTurretBuild.handleItem`.
pub fn handle_item(world: &mut World, e: Entity, item: ItemId) {
    let Some(mut state) = world.entity_mut(e).take::<TurretState>() else {
        return;
    };
    let TurretAmmo::Item(ammo) = &state.config.ammo else {
        world.entity_mut(e).insert(state);
        return;
    };
    let Some(entry) = ammo.iter().find(|entry| entry.item == item).copied() else {
        world.entity_mut(e).insert(state);
        return;
    };
    state.total_ammo += entry.ammo_multiplier;
    // Merge into an existing entry and move it to the back (`ammo.swap`).
    if let Some(index) = state.ammo.iter().position(|existing| existing.item == item) {
        state.ammo[index].amount += entry.ammo_multiplier;
        let last = state.ammo.len() - 1;
        state.ammo.swap(index, last);
    } else {
        state.ammo.push(AmmoEntry {
            item,
            bullet: entry.bullet,
            amount: entry.ammo_multiplier,
        });
    }
    world.entity_mut(e).insert(state);
}

/// `LiquidTurretBuild.acceptLiquid`.
pub fn accept_liquid(world: &World, e: Entity, liquid: LiquidId) -> bool {
    let Some(state) = turret(world, e) else {
        return false;
    };
    let TurretAmmo::Liquid(ammo) = &state.config.ammo else {
        // `Building.acceptLiquid`: `hasLiquids && consumesLiquid(liquid)` —
        // the coolant consumer's filter (PAR-0087).
        return state.config.coolant_liquids.contains(&liquid)
            && world.get::<LiquidModule>(e).is_some();
    };
    if !ammo.iter().any(|entry| entry.liquid == liquid) {
        return false;
    }
    let Some(liquids) = world.get::<LiquidModule>(e) else {
        return false;
    };
    // `LiquidTurret.java:151-155` / `ContinuousLiquidTurret.java:138-142`:
    // `current == liquid || !ammoTypes.containsKey(current) ||
    // liquids.get(current) <= 1 / ammoTypes.get(current).ammoMultiplier + 0.001`.
    let current = liquids.current_liquid();
    if current == liquid {
        return true;
    }
    match current_liquid_ammo(liquids, ammo) {
        Some(entry) => liquids.get(current) <= 1.0 / entry.ammo_multiplier + 0.001,
        None => true,
    }
}

/// `LiquidTurretBuild.handleLiquid` (only fills the storage module).
pub fn handle_liquid(world: &mut World, e: Entity, liquid: LiquidId, amount: f32) {
    // `Building.handleLiquid` fills up to the block's `liquidCapacity`
    // (duo 20, wave 10, tsunami 40; PAR-0087). Fixture turrets without a
    // placed block fall back to the old 60-unit liquid-ammo capacity.
    let block_capacity = world
        .get::<crate::entities::comp::Building>(e)
        .map(|b| b.block)
        .and_then(|block| {
            world
                .get_resource::<crate::world::block::BlockTable>()
                .and_then(|table| table.instance(block))
        })
        .map(|inst| inst.def.liquid_capacity);
    let capacity = match block_capacity {
        Some(capacity) if capacity > 0.0 => capacity,
        _ => turret(world, e)
            .map(|state| match &state.config.ammo {
                TurretAmmo::Liquid(_) => 60.0f32,
                _ if state.config.uses_coolant() => 20.0,
                _ => 0.0,
            })
            .unwrap_or(0.0),
    };
    if let Some(mut liquids) = world.get_mut::<LiquidModule>(e) {
        liquids.add(liquid, amount, capacity);
    }
}

/// `PayloadTurretBuild.acceptPayload`.
pub fn accept_payload(world: &World, e: Entity, payload: PayloadRef) -> bool {
    let Some(state) = turret(world, e) else {
        return false;
    };
    let TurretAmmo::Payload(ammo) = &state.config.ammo else {
        return false;
    };
    let content = PayloadContent::from_payload(payload);
    let total: i32 = state.payloads.iter().map(|stack| stack.count).sum();
    total < state.config.max_ammo && ammo.iter().any(|entry| entry.content == content)
}

/// `PayloadTurretBuild.handlePayload`.
pub fn handle_payload(world: &mut World, e: Entity, payload: PayloadRef) {
    let Some(mut state) = world.entity_mut(e).take::<TurretState>() else {
        return;
    };
    let content = PayloadContent::from_payload(payload);
    if let TurretAmmo::Payload(ammo) = &state.config.ammo
        && ammo.iter().any(|entry| entry.content == content)
    {
        if let Some(stack) = state
            .payloads
            .iter_mut()
            .find(|stack| stack.content == content)
        {
            stack.count += 1;
        } else {
            state.payloads.push(PayloadStack { content, count: 1 });
        }
    }
    world.entity_mut(e).insert(state);
}

/// Queues a build plan on a `BuildTurret` (`unit.addBuild`; plan 11/15 seam).
pub fn add_build_plan(world: &mut World, e: Entity, plan: BuildPlan) -> bool {
    let Some(mut state) = world.entity_mut(e).take::<TurretState>() else {
        return false;
    };
    if state.config.kind != TurretKind::Build {
        world.entity_mut(e).insert(state);
        return false;
    }
    state.build_plans.push(plan);
    world.entity_mut(e).insert(state);
    true
}

/// Removes one shot's worth of ammo and returns the fired bullet
/// (`TurretBuild.useAmmo`).
pub(super) fn use_ammo(state: &mut TurretState, world: &mut World, e: Entity) -> Option<BulletId> {
    let bullet = peek_ammo_state(state, world, e)?;
    // `Turret.java:683-686` / `LiquidTurret.java:128-131`: a cheating team
    // fires without consuming ammo (the item stack or the liquid charge).
    if matches!(
        &state.config.ammo,
        TurretAmmo::Item(_) | TurretAmmo::Liquid(_)
    ) && cheating(world, e)
    {
        return Some(bullet);
    }
    let mut liquid_remove: Option<(LiquidId, f32)> = None;
    match &state.config.ammo {
        TurretAmmo::Item(_) => {
            if let Some(entry) = state.ammo.last_mut() {
                entry.amount -= state.config.ammo_per_shot;
                if entry.amount <= 0 {
                    state.ammo.pop();
                }
            }
            state.total_ammo = (state.total_ammo - state.config.ammo_per_shot).max(0);
        }
        // `LiquidTurretBuild.useAmmo` (`LiquidTurret.java:128-133`):
        // `liquids.remove(current, 1 / type.ammoMultiplier)`.
        // `ContinuousLiquidTurretBuild.useAmmo` does not consume (`:112-116`).
        TurretAmmo::Liquid(ammo) => {
            if state.config.kind != TurretKind::Continuous
                && let Some(liquids) = world.get::<LiquidModule>(e)
                && let Some(entry) = current_liquid_ammo(liquids, ammo)
            {
                let amount = 1.0 / entry.ammo_multiplier.max(0.001);
                liquid_remove = Some((entry.liquid, amount));
            }
        }
        TurretAmmo::Power(_) => {}
        TurretAmmo::Payload(ammo) => {
            let content = ammo
                .iter()
                .find(|entry| {
                    state
                        .payloads
                        .iter()
                        .any(|stack| stack.content == entry.content && stack.count > 0)
                })
                .map(|entry| entry.content);
            if let Some(content) = content {
                if let Some(stack) = state
                    .payloads
                    .iter_mut()
                    .find(|stack| stack.content == content)
                {
                    stack.count -= 1;
                }
                state.payloads.retain(|stack| stack.count > 0);
            }
        }
    }
    if let Some((liquid, amount)) = liquid_remove
        && let Some(mut liquids) = world.get_mut::<LiquidModule>(e)
    {
        liquids.remove(liquid, amount);
    }
    Some(bullet)
}
