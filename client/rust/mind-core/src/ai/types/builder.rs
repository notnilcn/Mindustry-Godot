// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuilderAI` (plan G01 T10c), ported from
//! `core/src/mindustry/ai/types/BuilderAI.java:18-262`. The controller pulls
//! `BlockPlan`s from the team's ghost queue into the unit's `BuilderPlans`
//! (`BuilderComp`), moves toward the active plan and retreats to the core when
//! idle under fire. The actual construct progress runs in
//! `Sim::update_builders` (`sim/builder.rs`), now for every builder unit.
//!
//! Deferred (documented): the assist-`following`/`assistFollowing` tracking
//! (Java `BuilderAI.java:67-100,145-190`), the build-turret hold-position scan
//! and `PrebuildAI` (T17 spawns the defenders, behaviour deferred by the plan).

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::entities::comp::Pos;
use crate::sim::builder::{BuilderPlans, UnitBuildPlan};
use crate::world::TilePos;

use super::super::ai_controller::AiCtx;

/// `BuilderAI.defaultRebuildPeriod` (120 ticks; 10 with the `buildAi` team
/// rule).
pub const DEFAULT_REBUILD_PERIOD: f32 = 120.0;
/// `BuilderAI.buildRadius`: maximum plan distance.
pub const BUILD_RADIUS: f32 = 1500.0;
/// `BuilderAI.retreatDst`.
pub const RETREAT_DST: f32 = 110.0;
/// `BuilderAI.retreatDelay` (`Time.toSeconds * 2`).
pub const RETREAT_DELAY: f32 = 120.0;

/// `BuilderAI` per-unit state (`following`/`assistFollowing` are deferred).
#[derive(Debug, Clone, Copy, bevy_ecs::component::Component)]
pub struct BuilderAiState {
    /// Nearest enemy while idle (`enemy`).
    pub enemy: Option<Entity>,
    /// Idle timer (`retreatTimer`).
    pub retreat_timer: f32,
    /// `timer.get(timerTarget4, 40)` countdown for the enemy scan.
    pub timer40: f32,
    /// `timer.get(timerTarget3, rebuildPeriod)` countdown for plan pickup.
    pub rebuild_timer: f32,
    /// `onlyAssist` (`BuilderAI(true, 400)` + the `assist` command).
    pub only_assist: bool,
    /// `alwaysFlee` (AI-team core builders).
    pub always_flee: bool,
    /// `fleeRange` (370 command rebuild; 400/500 for core builders).
    pub flee_range: f32,
    /// `rebuildPeriod` (`init`: 10 with the `buildAi` rule).
    pub rebuild_period: f32,
    /// Last picked plan (`lastPlan` position; validity gate).
    pub last_plan: Option<TilePos>,
}

impl Default for BuilderAiState {
    fn default() -> Self {
        Self {
            enemy: None,
            retreat_timer: 0.0,
            timer40: 40.0,
            rebuild_timer: DEFAULT_REBUILD_PERIOD,
            only_assist: false,
            always_flee: false,
            flee_range: 370.0,
            rebuild_period: DEFAULT_REBUILD_PERIOD,
            last_plan: None,
        }
    }
}

impl BuilderAiState {
    /// `new BuilderAI(alwaysFlee, fleeRange)`.
    pub fn with_flee(always_flee: bool, flee_range: f32) -> Self {
        Self {
            always_flee,
            flee_range,
            ..Self::default()
        }
    }
}

/// `Build.validPlace(block, team, x, y, rotation)` for the live world.
fn valid_place(ctx: &AiCtx, team: u8, block: BlockId, x: i32, y: i32, rot: u8) -> bool {
    let (Some(table), Some(counter)) = (
        ctx.world.get_resource::<crate::world::BlockTable>(),
        ctx.world
            .get_resource::<crate::world::limits::BlockCounter>(),
    ) else {
        return false;
    };
    let rules = ctx
        .world
        .get_resource::<crate::world::limits::BuildRules>()
        .cloned()
        .unwrap_or_default();
    crate::world::build::valid_place_query(
        ctx.content,
        table,
        &rules,
        counter,
        ctx.grid,
        Some(ctx.world),
        block,
        team,
        rot,
        x,
        y,
        crate::world::build::PlaceFlags::PLAYER,
    )
}

/// `Build.validBreak(team, x, y)` for the live world.
fn valid_break(ctx: &AiCtx, team: u8, x: i32, y: i32) -> bool {
    let rules = ctx
        .world
        .get_resource::<crate::world::limits::BuildRules>()
        .cloned()
        .unwrap_or_default();
    crate::world::build::valid_break(ctx.content, ctx.grid, Some(ctx.world), &rules, team, x, y)
}

/// `BuilderAI.updateMovement` (`BuilderAI.java:48-262`), the plan-queue half.
pub fn update_builder_ai(ctx: &mut AiCtx, unit: Entity, state: &mut BuilderAiState) {
    let Some(def) = ctx
        .unit_type(unit)
        .and_then(|id| ctx.content.unit(id))
        .cloned()
    else {
        return;
    };
    let team = ctx.team;
    let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
        return;
    };

    // `unit.updateBuilding = true` (`BuilderComp.updateBuildLogic` gate).
    if let Some(mut plans) = ctx.world.get_mut::<BuilderPlans>(unit) {
        plans.update_building = true;
    }

    // `init()`: rebuild much faster with the buildAi team rule.
    if state.rebuild_period == DEFAULT_REBUILD_PERIOD {
        let build_ai = crate::game::runtime::rules_of(ctx.world)
            .map(|rules| rules.team_rule(team).build_ai)
            .unwrap_or(false);
        if build_ai {
            state.rebuild_period = 10.0;
        }
    }

    // `boolean hold = hasStance(UnitStance.holdPosition)` (T11).
    let hold = ctx
        .world
        .get::<crate::ai::unit_stance_runtime::UnitStancesComp>(unit)
        .is_some_and(|stances| stances.has_name(ctx.content, "holdposition"));

    let has_plan = ctx
        .world
        .get::<BuilderPlans>(unit)
        .is_some_and(|plans| !plans.plans.is_empty());

    // `(unit.buildPlan() == null || alwaysFlee) && !hold`: retreat when idle.
    if (!has_plan || state.always_flee) && !hold {
        state.timer40 -= 1.0;
        if state.timer40 <= 0.0 {
            state.timer40 = 40.0;
            state.enemy = ctx.find_target(unit, state.flee_range, true, true);
        }
        state.retreat_timer += 1.0;
        if (state.retreat_timer >= RETREAT_DELAY || state.always_flee)
            && state.enemy.is_some()
            && let Some((_, cx, cy)) =
                crate::entities::comp::unit::miner::closest_core(ctx.world, pos.x, pos.y, team)
        {
            let dx = cx - pos.x;
            let dy = cy - pos.y;
            if dx * dx + dy * dy > RETREAT_DST * RETREAT_DST {
                crate::ai::types::flying::move_to_point(
                    ctx.world,
                    ctx.grid,
                    ctx.content,
                    unit,
                    cx,
                    cy,
                    RETREAT_DST,
                    100.0,
                    false,
                );
            }
        }
    }

    if has_plan {
        if !state.always_flee {
            state.retreat_timer = 0.0;
        }
        // `BuildPlan req = unit.buildPlan()`: validate, then approach.
        let plan = ctx
            .world
            .get::<BuilderPlans>(unit)
            .and_then(|plans| plans.plans.front().cloned());
        let Some(plan) = plan else {
            return;
        };
        // `boolean valid = ... (construct of the same block) || (breaking ?
        // validBreak : validPlace)`.
        let construct_ok = ctx
            .grid
            .tiles
            .getn(plan.x as i32, plan.y as i32)
            .and_then(|tile| tile.build)
            .is_some_and(|entity| {
                ctx.world
                    .get::<crate::world::ConstructState>(entity)
                    .is_some_and(|state| state.current == plan.block)
            });
        let valid = construct_ok
            || if plan.breaking {
                valid_break(ctx, team, plan.x as i32, plan.y as i32)
            } else {
                valid_place(
                    ctx,
                    team,
                    plan.block,
                    plan.x as i32,
                    plan.y as i32,
                    plan.rotation,
                )
            };
        if !valid {
            // `discard invalid plan`.
            if let Some(mut plans) = ctx.world.get_mut::<BuilderPlans>(unit) {
                plans.plans.pop_front();
            }
            state.last_plan = None;
            return;
        }
        if !hold {
            // `moveTo(req.tile(), min(buildRange - hitSize*2, buildRadius), 20)`.
            let range = (def.build_range - def.hit_size * 2.0).min(BUILD_RADIUS);
            let (tx, ty) = (
                plan.x as f32 * crate::config::TILESIZE as f32,
                plan.y as f32 * crate::config::TILESIZE as f32,
            );
            let dx = tx - pos.x;
            let dy = ty - pos.y;
            let within = dx * dx + dy * dy <= range * range;
            if !within {
                crate::ai::types::flying::move_to_point(
                    ctx.world,
                    ctx.grid,
                    ctx.content,
                    unit,
                    tx,
                    ty,
                    range,
                    20.0,
                    false,
                );
            }
        }
        return;
    }

    // `if(!onlyAssist && !team.plans.isEmpty() && timer.get(timerTarget3, rebuildPeriod))`:
    // pick up the next ghost plan.
    if state.only_assist {
        return;
    }
    state.rebuild_timer -= 1.0;
    if state.rebuild_timer > 0.0 {
        return;
    }
    state.rebuild_timer = state.rebuild_period;

    let Some(head) = ctx
        .world
        .get_resource::<crate::game::runtime::CampaignRuntime>()
        .and_then(|campaign| campaign.session.teams.get_or_null(crate::ecs::TeamId(team)))
        .and_then(|data| data.plans.front().cloned())
    else {
        return;
    };

    let rotate_head = |ctx: &mut AiCtx, team: u8| {
        if let Some(mut campaign) = ctx
            .world
            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
            && let Some(plan) = campaign
                .session
                .teams
                .get(crate::ecs::TeamId(team))
                .plans
                .pop_front()
        {
            campaign
                .session
                .teams
                .get(crate::ecs::TeamId(team))
                .plans
                .push_back(plan);
        }
    };

    // `if(world.tile(block.x, block.y).block() == block.block)` -> removeFirst.
    let existing = ctx
        .grid
        .tiles
        .getn(head.x as i32, head.y as i32)
        .map(|tile| tile.block);
    if existing == Some(head.block) {
        if let Some(mut campaign) = ctx
            .world
            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
        {
            campaign
                .session
                .teams
                .get(crate::ecs::TeamId(team))
                .plans
                .pop_front();
        }
        return;
    }
    let valid = valid_place(
        ctx,
        team,
        head.block,
        head.x as i32,
        head.y as i32,
        head.rotation.rem_euclid(4) as u8,
    );
    if valid && (!state.always_flee || state.enemy.is_none()) {
        // `unit.addBuild(new BuildPlan(...))`; rotate the team queue head to
        // the tail so the next unit builds something else.
        state.last_plan = Some(TilePos::new(head.x, head.y));
        let mut plan = UnitBuildPlan::place(
            head.x,
            head.y,
            head.rotation.rem_euclid(4) as u8,
            head.block,
        );
        if let Some(config) = head.config.clone() {
            plan.config = config;
        }
        if let Some(mut plans) = ctx.world.get_mut::<BuilderPlans>(unit) {
            plans.add_build(plan, false);
        }
        rotate_head(ctx, team);
    } else {
        // `blocks.addLast(blocks.removeFirst())`: try something else next time.
        rotate_head(ctx, team);
    }
}
