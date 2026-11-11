// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Command` dispatch arm: the `CommandAI` controller dispatch (repair/builder
//! controllers, stance boost/pursue, attack tracking, miner/move commands).
//! Upstream: core/src/mindustry/ai/types/CommandAI.java
//! core/src/mindustry/ai/types/RepairAI.java
//! core/src/mindustry/ai/types/BuilderAI.java
//! core/src/mindustry/ai/types/MinerAI.java

use super::super::ai::resolve_command_kind;
use super::super::ai_weapons::command_ai_weapon_pass;
use super::super::command_queue::pop_live_command_queue;
use super::super::targeting::{entity_pos_within, face_target, unit_control_cost};
use super::*;

/// `CommandAI.updateUnit` dispatch (T10a/T10b/T11).
pub(in crate::sim::unit_runtime) fn command_arm(
    world: &mut World,
    runtime: &mut UnitRuntime,
    queries: &TargetQueries,
    entity: Entity,
    team: u8,
    slot: ControllerSlot,
) {
    // `CommandAI.updateUnit` -> `defaultBehavior`/`commandController`
    // (T10a/T10b: MinerAI/RepairAI).
    let command_kind = resolve_command_kind(world, &runtime.content, entity, &slot);
    if command_kind == Some(crate::content::registries::commands::ControllerKind::Repair) {
        // `RepairAI.updateTargeting` replaces the enemy targeting: the
        // closest damaged friendly building, else `super` targeting.
        let mut repair = world
            .get::<crate::ai::types::repair::RepairAiState>(entity)
            .copied()
            .unwrap_or_default();
        {
            let UnitRuntime {
                content,
                grid,
                pathfinder,
                ..
            } = &mut *runtime;
            let mut ctx = crate::ai::ai_controller::AiCtx {
                world,
                grid,
                content,
                pathfinder,
                team,
            };
            crate::ai::types::repair::update_repair_ai(&mut ctx, entity, &mut repair);
        }
        if let Some(mut stored) = world.get_mut::<crate::ai::types::repair::RepairAiState>(entity) {
            *stored = repair;
        } else if world.get_entity(entity).is_ok() {
            world.entity_mut(entity).insert(repair);
        }
    } else if matches!(
        command_kind,
        Some(crate::content::registries::commands::ControllerKind::Builder)
            | Some(crate::content::registries::commands::ControllerKind::BuilderAssist)
    ) {
        // `UnitCommand.loadAll`: rebuild -> `BuilderAI()` (alwaysFlee
        // false, fleeRange 370); assist -> `BuilderAI{onlyAssist=true}`
        // (UnitCommand.java:73-116).
        let only_assist = command_kind
            == Some(crate::content::registries::commands::ControllerKind::BuilderAssist);
        let mut builder = world
            .get::<crate::ai::types::builder::BuilderAiState>(entity)
            .copied()
            .unwrap_or_else(|| {
                let mut state = crate::ai::types::builder::BuilderAiState::with_flee(false, 370.0);
                state.only_assist = only_assist;
                state
            });
        {
            let UnitRuntime {
                content,
                grid,
                pathfinder,
                ..
            } = &mut *runtime;
            let mut ctx = crate::ai::ai_controller::AiCtx {
                world,
                grid,
                content,
                pathfinder,
                team,
            };
            crate::ai::types::builder::update_builder_ai(&mut ctx, entity, &mut builder);
        }
        if let Some(mut stored) = world.get_mut::<crate::ai::types::builder::BuilderAiState>(entity)
        {
            *stored = builder;
        } else if world.get_entity(entity).is_ok() {
            world.entity_mut(entity).insert(builder);
        }
        // `BuilderAI.shouldShoot()`: `!unit.isBuilding() && canAttack`
        // (BuilderAI.java:262-264); `isBuilding` is
        // `buildSpeedMultiplier > 0 && activelyBuilding` (UnitComp.java).
        let is_building = world
            .get::<crate::sim::builder::BuilderPlans>(entity)
            .is_some_and(|plans| plans.update_building && !plans.plans.is_empty());
        let can_attack = world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| runtime.content.unit(comp.type_id))
            .is_some_and(|def| def.can_attack);
        let _ = command_ai_weapon_pass(
            world,
            &runtime.content,
            queries,
            entity,
            team,
            can_attack && !is_building,
        );
    } else {
        // Targeting runs for the weapons (`updateTargeting`, autoTarget);
        // movement only follows an explicit order (`targetPos`); with none
        // the unit idles in place instead of walking to the core anchor.
        let mount_target =
            command_ai_weapon_pass(world, &runtime.content, queries, entity, team, true);
        // `CommandAI.updateUnit`'s stance handling (T11): boost and
        // pursueTarget (edit `slot` before the movement match).
        let mut slot = slot;
        let stances = world
            .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(entity)
            .copied()
            .unwrap_or_default();
        let def_range = world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| runtime.content.unit(comp.type_id))
            .map(|def| def.max_range)
            .unwrap_or(0.0);
        if stances.has_name(&runtime.content, "boost") {
            // `shouldBoost()`: `updateBoosting(false)` while an attack
            // target is in range, else true (CommandAI.java:150-163).
            let in_range = mount_target
                .is_some_and(|target| entity_pos_within(world, entity, target, def_range));
            if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
                core.boosting = !in_range;
            }
        }
        if slot.target.is_none()
            && stances.has_name(&runtime.content, "pursuetarget")
            && let Some(target) = mount_target
            && let Some(pos) = world.get::<Pos>(target).copied()
        {
            // `pursueTarget(target, true)`: `commandTarget(target, false)`.
            slot.target = Some(TilePos::new(
                crate::world::WorldGrid::to_tile(pos.x) as i16,
                crate::world::WorldGrid::to_tile(pos.y) as i16,
            ));
        }
        // `CommandAI.defaultBehavior` attack tracking (G01 T16):
        // `targetPos.set(attackTarget)` every tick, dead targets are
        // cleared, and the unit stops within `range - 10`.
        let mut move_target = slot.target;
        if let Some(target) = world
            .get::<crate::ai::types::command::AttackTargetComp>(entity)
            .and_then(|comp| comp.0)
        {
            let alive = world.get_entity(target).is_ok()
                && world
                    .get::<Health>(target)
                    .is_none_or(|health| !health.dead);
            if !alive {
                world
                    .entity_mut(entity)
                    .insert(crate::ai::types::command::AttackTargetComp(None));
                move_target = None;
                if let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
                    stored.target = None;
                }
            } else if let Some(target_pos) = world.get::<Pos>(target).copied() {
                let within = world.get::<Pos>(entity).copied().is_some_and(|pos| {
                    let dx = target_pos.x - pos.x;
                    let dy = target_pos.y - pos.y;
                    dx * dx + dy * dy <= (def_range - 10.0).max(0.0).powi(2)
                });
                move_target = if within {
                    None
                } else {
                    Some(TilePos::new(
                        crate::world::WorldGrid::to_tile(target_pos.x) as i16,
                        crate::world::WorldGrid::to_tile(target_pos.y) as i16,
                    ))
                };
            }
        }
        if command_kind == Some(crate::content::registries::commands::ControllerKind::Miner) {
            // `CommandAI.updateUnit`: the mine command auto-adds
            // `mineAuto` when no item stance is set (T11).
            let mut stances = world
                .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(entity)
                .copied()
                .unwrap_or_default();
            if !stances.has_name(&runtime.content, "mineauto")
                && !stances.any_item_stance(&runtime.content)
                && let Some(def) = runtime.content.unit_stance_by_name("mineauto")
            {
                crate::ai::unit_stance_runtime::apply_stance(&mut stances.0, def);
                world.entity_mut(entity).insert(stances);
            }
            let mut miner = world
                .get::<crate::ai::types::miner::MinerAiState>(entity)
                .copied()
                .unwrap_or_default();
            {
                let UnitRuntime {
                    content,
                    grid,
                    pathfinder,
                    ..
                } = &mut *runtime;
                let mut ctx = crate::ai::ai_controller::AiCtx {
                    world,
                    grid,
                    content,
                    pathfinder,
                    team,
                };
                crate::ai::types::miner::update_miner_ai(&mut ctx, entity, &mut miner);
            }
            if let Some(mut stored) = world.get_mut::<crate::ai::types::miner::MinerAiState>(entity)
            {
                *stored = miner;
            } else if world.get_entity(entity).is_ok() {
                world.entity_mut(entity).insert(miner);
            }
        } else {
            if world
                .get::<crate::entities::comp::unit::comp::MinerComp>(entity)
                .is_some_and(|comp| comp.mine_tile.is_some())
            {
                // `CommandAI.command`: a non-mine command clears `mineTile`.
                crate::entities::comp::unit::miner::clear_mine_tile(world, entity);
            }
            // The default `defaultBehavior` movement runs for commands
            // without a controller (`None`: move/enterPayload/...) and for
            // the not-yet-dispatched Repair/Builder controllers.
            match move_target {
                Some(target) => {
                    let flying = world
                        .get::<PhysicsComp>(entity)
                        .is_some_and(|physics| physics.flying);
                    // Refresh the lazy control path before borrowing
                    // fields (`unit.type.pathCostId`).
                    let cost = unit_control_cost(world, &runtime.content, entity);
                    runtime.control_path(cost);
                    let UnitRuntime {
                        grid,
                        content,
                        control_paths,
                        ..
                    } = &mut *runtime;
                    let arrived = crate::ai::types::command::update_command_move(
                        world,
                        grid,
                        content,
                        &mut control_paths[usize::from(cost.id())],
                        entity,
                        target,
                        flying,
                    );
                    if arrived {
                        // `finishPath`: `targetPos = null`.
                        if let Some(mut stored) = world.get_mut::<ControllerSlot>(entity) {
                            stored.target = None;
                        }
                        // Then the next queued waypoint becomes the
                        // target (`CommandAI.java:476-490`, I01 T13).
                        pop_live_command_queue(world, &runtime.grid, entity);
                    }
                }
                None => {
                    // Java `CommandAI.defaultBehavior` idles without
                    // touching `vel`; drag brings the unit to a stop.
                }
            }
        }
        // `CommandAI` -> `faceTarget()` (CommandAI.java:400/420).
        face_target(world, &runtime.content, entity, mount_target);
    }
}
