// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuildTurret` (plan-building tower) update and build economy helpers.
//! Upstream: core/src/mindustry/world/blocks/defense/turrets/BuildTurret.java

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::combat::bullet::CombatCtx;
use crate::content::ContentRegistry;
use crate::entities::comp::{Pos, TeamComp};
use crate::world::construct::ConstructState;
use crate::world::plan::BuildPlan;

use super::math::lerp_delta;
use super::{TurretConfig, TurretState};

/// `if(checkSuppression()) efficiency = potentialEfficiency = 0`
/// (BuildTurret.java:132-134): a heal-suppressed build tower builds (and
/// moves) at speed 0 (`buildSpeedMultiplier(potentialEfficiency * timeScale)`).
pub fn build_turret_efficiency(world: &World, e: Entity, eff: f32) -> f32 {
    if crate::world::blocks::defense::projectors::is_heal_suppressed(world, e) {
        0.0
    } else {
        eff
    }
}

/// `BuildTurret.timerTarget` interval (`BuildTurret.java:32`).
pub(super) const BUILD_TARGET_INTERVAL: f32 = 15.0;
/// `timer.get(timerTarget2, 30f)` (`BuildTurret.java:196`).
pub(super) const BUILD_BREAK_CHECK_INTERVAL: f32 = 30.0;

/// `within(block.x * tilesize, block.y * tilesize, range)`
/// (`BuildTurret.java:161`): the plan tile centre against the tower's own
/// position.
pub(super) fn build_plan_within(x: f32, y: f32, plan: &BuildPlan, range: f32) -> bool {
    let tile = crate::world::block::TILE_SIZE;
    let dx = plan.x as f32 * tile - x;
    let dy = plan.y as f32 * tile - y;
    dx * dx + dy * dy <= range * range
}

/// `Build.validPlace(type, team, x, y, rot)` (`Build.java:171-174`): the
/// 5-arg form defaults `checkVisible`/`checkCoreRadius` to true and still runs
/// the unit-overlap check (`validPlaceIgnoreUnits` + `checkNoUnitOverlap`).
#[allow(clippy::too_many_arguments)] // mirrors the Java `validPlace` argument bundle
pub(super) fn build_valid_place(
    world: &World,
    content: &ContentRegistry,
    grid: &crate::world::WorldGrid,
    block: crate::content::BlockId,
    team: u8,
    rotation: u8,
    x: i32,
    y: i32,
) -> bool {
    let rules = world
        .get_resource::<crate::world::limits::BuildRules>()
        .cloned()
        .unwrap_or_default();
    let (Some(table), Some(counter)) = (
        world.get_resource::<crate::world::block::BlockTable>(),
        world.get_resource::<crate::world::limits::BlockCounter>(),
    ) else {
        return false;
    };
    crate::world::build::valid_place_query(
        content,
        table,
        &rules,
        counter,
        grid,
        Some(world),
        block,
        team,
        rotation,
        x,
        y,
        crate::world::build::PlaceFlags::PLAYER,
    )
}

/// `Build.validBreak(team, x, y)` (`Build.java:319`).
pub(super) fn build_valid_break(
    world: &World,
    content: &ContentRegistry,
    grid: &crate::world::WorldGrid,
    team: u8,
    x: i32,
    y: i32,
) -> bool {
    let rules = world
        .get_resource::<crate::world::limits::BuildRules>()
        .cloned()
        .unwrap_or_default();
    crate::world::build::valid_break(content, grid, Some(world), &rules, team, x, y)
}

/// `state.rules.infiniteResources || team.rules().infiniteResources` plus
/// `state.rules.buildCostMultiplier` (`BuildTurret.java:166`).
pub(super) fn build_economy(world: &World, team: u8) -> (bool, f32) {
    if let Some(rules) = crate::game::runtime::rules_of(world) {
        (
            rules.is_infinite_resources(team),
            rules.build_cost_multiplier,
        )
    } else {
        let rules = world
            .get_resource::<crate::world::limits::BuildRules>()
            .cloned()
            .unwrap_or_default();
        (
            rules.infinite_resources || rules.cheat,
            rules.build_cost_multiplier,
        )
    }
}

/// `team.items().has(btype.requirements, state.rules.buildCostMultiplier)`
/// (`BuildTurret.java:166`): `ItemModule.has` rounds each amount
/// (`Mathf.round(amount * scale)`), so a sub-0.5 requirement is free.
pub(super) fn build_requirements_met(
    inventory: Option<&crate::world::modules::ItemModule>,
    content: &ContentRegistry,
    block: crate::content::BlockId,
    cost_multiplier: f32,
) -> bool {
    let Some(def) = content.block(block) else {
        return false;
    };
    let Some(inventory) = inventory else {
        // No team inventory (pre-campaign harness): nothing to fund.
        return true;
    };
    def.requirements.iter().all(|stack| {
        let need = (stack.amount as f32 * cost_multiplier).round() as i32;
        inventory.get(stack.item) >= need
    })
}

/// `BuilderComp.canBuild()` (`BuilderComp.java:46-48`): a build-capable unit
/// type with a positive build-speed multiplier.
pub(super) fn unit_can_build(world: &World, content: &ContentRegistry, entity: Entity) -> bool {
    let build_speed = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
        .map_or(0.0, |def| def.build_speed);
    let multiplier = world
        .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
        .map_or(1.0, |status| status.build_speed_multiplier);
    build_speed > 0.0 && multiplier > 0.0
}

/// `u.canBuild() && u.activelyBuilding()` (`BuildTurret.java:181-182`) for any
/// unit (`BuilderComp.activelyBuilding`, `BuilderComp.java:301-309`).
pub(super) fn unit_actively_building(ctx: &mut CombatCtx<'_>, entity: Entity) -> bool {
    if !unit_can_build(ctx.world, ctx.content, entity) {
        return false;
    }
    let Some(pos) = ctx.world.get::<Pos>(entity).copied() else {
        return false;
    };
    crate::ai::types::player::player_actively_building(
        ctx.world,
        ctx.content,
        entity,
        (pos.x, pos.y),
    )
    .is_some()
}

/// `BuildTurret.java:181-192`: the first nearby unit that can build and is
/// actively building a `ConstructBuild` inside the tower's range. Java iterates
/// `Units.nearby` (quadtree order); the port uses `EntitySeq` order for
/// determinism.
pub(super) fn scan_follow_unit(
    ctx: &mut CombatCtx<'_>,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
) -> Option<Entity> {
    let candidates =
        crate::entities::comp::unit::queries::in_radius(ctx.world, x, y, range, Some(team));
    for other in candidates {
        if !unit_actively_building(ctx, other) {
            continue;
        }
        let Some(plan) = ctx
            .world
            .get::<crate::sim::builder::BuilderPlans>(other)
            .and_then(|plans| plans.plans.front().cloned())
        else {
            continue;
        };
        // `world.build(plan.x, plan.y) instanceof ConstructBuild`.
        let Some(build) = ctx
            .grid
            .entity_at(crate::world::TilePos::new(plan.x, plan.y))
        else {
            continue;
        };
        if ctx.world.get::<ConstructState>(build).is_none() {
            continue;
        }
        let Some(bpos) = ctx.world.get::<Pos>(build).copied() else {
            continue;
        };
        let dx = bpos.x - x;
        let dy = bpos.y - y;
        if dx * dx + dy * dy <= range * range {
            return Some(other);
        }
    }
    None
}

/// `BuildPlan` copy of a unit plan (`unit.plans().addFirst(following.buildPlan())`,
/// `BuildTurret.java:189-190`).
pub(super) fn unit_plan_to_build_plan(plan: &crate::sim::builder::UnitBuildPlan) -> BuildPlan {
    BuildPlan {
        x: i32::from(plan.x),
        y: i32::from(plan.y),
        rotation: plan.rotation,
        block: plan.block,
        config: plan.config.clone(),
        breaking: plan.breaking,
    }
}

/// `BuildTurret.java:196-208`: another player of the same team is breaking the
/// tile the tower wants to build on; the plan is dropped and the tile removed
/// from the team rebuild queue.
pub(super) fn player_breaking_plan(ctx: &mut CombatCtx<'_>, team: u8, plan: &BuildPlan) -> bool {
    let mut players: SmallVec<[Entity; 4]> = SmallVec::new();
    if let Some(runtime) = ctx
        .world
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        && let Some(data) = runtime.session.teams.get_or_null(crate::ecs::TeamId(team))
    {
        players.extend(data.players.iter().copied());
    }
    if players.is_empty() {
        players.extend(
            ctx.world
                .iter_entities()
                .filter(|entity| {
                    entity.get::<crate::entities::comp::Player>().is_some()
                        && entity.get::<TeamComp>().map(|t| t.team) == Some(team)
                })
                .map(|entity| entity.id()),
        );
    }
    for player in players {
        if !unit_actively_building(ctx, player) {
            continue;
        }
        let Some(other) = ctx
            .world
            .get::<crate::sim::builder::BuilderPlans>(player)
            .and_then(|plans| plans.plans.front())
        else {
            continue;
        };
        if other.breaking && i32::from(other.x) == plan.x && i32::from(other.y) == plan.y {
            return true;
        }
    }
    false
}

/// `team.data().plans.remove(p -> p.x == req.x && p.y == req.y)`
/// (`BuildTurret.java:205`): arc `Queue.remove(pred)` drops the first match.
pub(super) fn remove_team_plan(world: &mut World, team: u8, plan: &BuildPlan) {
    if let Some(mut runtime) = world.get_resource_mut::<crate::game::runtime::CampaignRuntime>() {
        let plans = &mut runtime.session.teams.get(crate::ecs::TeamId(team)).plans;
        if let Some(index) = plans
            .iter()
            .position(|entry| i32::from(entry.x) == plan.x && i32::from(entry.y) == plan.y)
        {
            plans.remove(index);
        }
    }
}

/// `BuildTurretBuild.updateTile` (`BuildTurret.java:132-260`): proxy-unit plan
/// selection (team rebuild queue + follower scan), per-tick plan validation,
/// the another-player-breaking drop, and `unit.updateBuildLogic()`'s
/// construct/deconstruct progress through plan 07's [`ConstructState`].
///
/// Placement finalisation stays with plan 07 (`BuildHarness::construct_tick`);
/// `Build.beginPlace`/`beginBreak` need the live `SimRuntime` and are not
/// reachable from the combat pass, so a plan whose tile holds no
/// `ConstructState` is discarded (Java would begin the construct/break there).
pub(super) fn update_build(
    ctx: &mut CombatCtx<'_>,
    e: Entity,
    state: &mut TurretState,
    config: &TurretConfig,
    x: f32,
    y: f32,
    eff: f32,
) {
    let eff = build_turret_efficiency(ctx.world, e, eff);
    let team = ctx.team(e);
    let range = config.range;
    let (infinite, cost_multiplier) = build_economy(ctx.world, team);

    // Drop plans outside the grid (`Build.validPlace` rejects out-of-bounds).
    state
        .build_plans
        .retain(|plan| ctx.grid.tiles.in_bounds(plan.x, plan.y));

    // `TurretBuild.updateTile` (`Turret.java:535-537`) keeps the proxy on the
    // tower; `isControlled()` = the proxy carries the `Player` marker
    // (`Turret.java:448-450`).
    let controlled = state.block_unit.is_some_and(|unit| {
        ctx.world
            .get::<crate::entities::comp::Player>(unit)
            .is_some()
    });

    // `warmup = Mathf.lerpDelta(warmup, unit.activelyBuilding() ? efficiency :
    // 0f, 0.1f)` (`BuildTurret.java:141`) uses the queue from before this
    // tick's mutations. `activelyBuilding` is false while player-controlled.
    let was_building = !controlled
        && state
            .build_plans
            .first()
            .is_some_and(|plan| infinite || build_plan_within(x, y, plan, range));
    state.build_warmup = lerp_delta(
        state.build_warmup,
        if was_building { eff } else { 0.0 },
        0.1,
    );

    if !controlled {
        if let Some(following) = state.following {
            // `BuildTurret.java:146-155`: validate the follower and mirror its
            // head plan, or forget everything.
            let still_following = ctx.world.get_entity(following).is_ok()
                && unit_actively_building(ctx, following)
                && ctx
                    .world
                    .get::<crate::sim::builder::BuilderPlans>(following)
                    .and_then(|plans| plans.plans.front())
                    .is_some();
            if !still_following {
                state.following = None;
                state.build_plans.clear();
            } else {
                let plan = ctx
                    .world
                    .get::<crate::sim::builder::BuilderPlans>(following)
                    .and_then(|plans| plans.plans.front())
                    .map(unit_plan_to_build_plan);
                state.build_plans.clear();
                if let Some(plan) = plan {
                    state.build_plans.push(plan);
                }
                state.last_plan = None;
            }
        } else if state.build_plans.is_empty() {
            // `unit.buildPlan() == null && timer(timerTarget, targetInterval)`
            // (`BuildTurret.java:156-194`).
            state.build_target_timer -= 1.0;
            if state.build_target_timer <= 0.0 {
                state.build_target_timer = BUILD_TARGET_INTERVAL;
                // (1) Team rebuild queue: first `!removed` entry that is in
                // range and legal + affordable; rotate it to the tail.
                let queue = ctx
                    .world
                    .get_resource::<crate::game::runtime::CampaignRuntime>()
                    .and_then(|runtime| {
                        runtime
                            .session
                            .teams
                            .get_or_null(crate::ecs::TeamId(team))
                            .map(|data| data.plans.clone())
                    });
                let inventory = ctx
                    .world
                    .get_resource::<crate::game::runtime::CampaignRuntime>()
                    .and_then(|runtime| {
                        runtime
                            .session
                            .teams
                            .team_items(crate::ecs::TeamId(team))
                            .cloned()
                    });
                let selected = queue.as_ref().and_then(|queue| {
                    crate::world::blocks::defense::build_turret_plans::scan_team_rebuild_queue(
                        queue,
                        |plan| build_plan_within(x, y, plan, range),
                        |plan| {
                            build_valid_place(
                                ctx.world,
                                ctx.content,
                                ctx.grid,
                                plan.block,
                                team,
                                plan.rotation,
                                plan.x,
                                plan.y,
                            ) && (infinite
                                || build_requirements_met(
                                    inventory.as_ref(),
                                    ctx.content,
                                    plan.block,
                                    cost_multiplier,
                                ))
                        },
                    )
                });
                if let Some((index, plan)) = selected {
                    if let Some(mut runtime) = ctx
                        .world
                        .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
                        && let Some(entry) =
                            crate::world::blocks::defense::build_turret_plans::rotate_plan_to_tail(
                                &mut runtime.session.teams.get(crate::ecs::TeamId(team)).plans,
                                index,
                            )
                    {
                        state.last_plan = Some(entry);
                    }
                    state.build_plans.push(plan);
                }
                // (2) `BuildTurret.java:178-194`: still not building, mimic a
                // nearby actively-building unit.
                if state.build_plans.is_empty() {
                    state.following = scan_follow_unit(ctx, team, x, y, range);
                }
            }
        } else if let Some(head) = state.build_plans.first().cloned() {
            // `BuildTurret.java:195-226`: validate the current plan.
            // Clear the break plan if another player is breaking the same tile
            // (`timer.get(timerTarget2, 30f)`).
            state.build_break_timer -= 1.0;
            if !head.breaking && state.build_break_timer <= 0.0 {
                state.build_break_timer = BUILD_BREAK_CHECK_INTERVAL;
                if player_breaking_plan(ctx, team, &head) {
                    state.build_plans.remove(0);
                    remove_team_plan(ctx.world, team, &head);
                    // Java `return`s from `updateTile` here, skipping the
                    // build-logic pass for this tick.
                    state.build_target = None;
                    state.is_shooting = false;
                    state.was_shooting = false;
                    state.any = false;
                    return;
                }
            }

            let last_removed = state.last_plan.as_ref().is_some_and(|plan| plan.removed);
            let construct_ok = ctx
                .grid
                .entity_at(crate::world::TilePos::new(head.x as i16, head.y as i16))
                .and_then(|build| {
                    ctx.world
                        .get::<ConstructState>(build)
                        .map(|cons| cons.current)
                })
                == Some(head.block);
            let valid = !last_removed
                && (construct_ok
                    || if head.breaking {
                        build_valid_break(ctx.world, ctx.content, ctx.grid, team, head.x, head.y)
                    } else {
                        build_valid_place(
                            ctx.world,
                            ctx.content,
                            ctx.grid,
                            head.block,
                            team,
                            head.rotation,
                            head.x,
                            head.y,
                        )
                    });
            if !valid {
                // Discard the invalid request (`BuildTurret.java:220-224`).
                state.build_plans.remove(0);
                state.last_plan = None;
            }
        }
    } else {
        // Player-controlled: forget everything (`BuildTurret.java:228-231`).
        state.following = None;
        state.last_plan = None;
    }

    // `unit.plans().remove(b -> b.build() == this)` (`BuildTurret.java:234`):
    // never build over the tower itself.
    state.build_plans.retain(|plan| {
        ctx.grid
            .entity_at(crate::world::TilePos::new(plan.x as i16, plan.y as i16))
            != Some(e)
    });

    // `unit.updateBuildLogic()`: drive the head plan's `ConstructBuild`.
    let mut actively_building = false;
    if let Some(head) = state.build_plans.first().cloned() {
        let target = ctx.grid.tiles.get(head.x, head.y).build;
        if let Some(entity) = target
            && ctx.world.get::<ConstructState>(entity).is_some()
        {
            let Some(mut construct) = ctx.world.get::<ConstructState>(entity).cloned() else {
                return;
            };
            construct.last_builder = Some(e);
            let refund = ctx
                .world
                .get_resource::<crate::world::limits::BuildRules>()
                .map(|rules| rules.deconstruct_refund_multiplier)
                .unwrap_or(0.5);
            let done = if head.breaking {
                // `ConstructBuild.deconstruct` (`BuilderComp.java:244-246`):
                // plan 07's deconstruct path, never construct.
                if construct.active_deconstruct {
                    construct
                        .advance_deconstruct(config.build_speed * eff, refund)
                        .0
                } else {
                    true
                }
            } else {
                construct.advance_construct(config.build_speed * eff)
            };
            ctx.world.entity_mut(entity).insert(construct);
            actively_building = true;
            if done {
                state.build_plans.remove(0);
                state.last_plan = None;
            }
        } else {
            // No `ConstructBuild` on the tile: `Build.beginPlace`/`beginBreak`
            // (plan 07's `SimRuntime`) owns that swap; discard the stale plan.
            state.build_plans.remove(0);
            state.last_plan = None;
        }
    }

    state.build_target = state.build_plans.first().map(|plan| (plan.x, plan.y));
    state.is_shooting = actively_building;
    state.was_shooting = actively_building;
    state.any = actively_building;
}
