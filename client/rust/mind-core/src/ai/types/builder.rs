// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `BuilderAI` (plan G01 T10c), ported from
//! `core/src/mindustry/ai/types/BuilderAI.java:18-262`. The controller pulls
//! `BlockPlan`s from the team's ghost queue into the unit's `BuilderPlans`
//! (`BuilderComp`), moves toward the active plan, follows/assists another
//! actively-building unit (`following`/`assistFollowing`, H01 T12) and retreats
//! to the core when idle under fire. The actual construct progress runs in
//! `Sim::update_builders` (`sim/builder.rs`), now for every builder unit.
//!
//! Deferred (documented): the build-turret hold-position scan and `PrebuildAI`
//! (T17 spawns the defenders, behaviour deferred by the plan).

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, ContentRegistry};
use crate::entities::comp::Pos;
use crate::sim::builder::{BuilderPlans, UnitBuildPlan};
use crate::world::{ConstructState, TilePos};

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

/// `BuilderAI` per-unit state.
#[derive(Debug, Clone, Copy, bevy_ecs::component::Component)]
pub struct BuilderAiState {
    /// Nearest enemy while idle (`enemy`).
    pub enemy: Option<Entity>,
    /// Idle timer (`retreatTimer`).
    pub retreat_timer: f32,
    /// `timer.get(timerTarget4, 40)` countdown for the enemy scan.
    pub timer40: f32,
    /// `timerTarget2` countdown (the 20-tick builder scan; Java shares the id
    /// with the 40-tick plan-block check, which the single-player port does
    /// not run — see the deviation note on `update_builder_ai`).
    pub timer2: f32,
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
    /// `assistFollowing`: the builder player this unit escorts (`onlyAssist`).
    pub assist_following: Option<Entity>,
    /// `following`: the actively-building unit whose plan is copied.
    pub following: Option<Entity>,
}

impl Default for BuilderAiState {
    fn default() -> Self {
        Self {
            enemy: None,
            retreat_timer: 0.0,
            timer40: 40.0,
            timer2: 20.0,
            rebuild_timer: DEFAULT_REBUILD_PERIOD,
            only_assist: false,
            always_flee: false,
            flee_range: 370.0,
            rebuild_period: DEFAULT_REBUILD_PERIOD,
            last_plan: None,
            assist_following: None,
            following: None,
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

/// `Unit.isValid()` (`HealthComp.java:16`): still added to the world and not
/// dead (`!dead && isAdded()`).
fn unit_valid(world: &World, entity: Entity) -> bool {
    world.get_entity(entity).is_ok()
        && world
            .get::<crate::entities::comp::Health>(entity)
            .is_none_or(|health| !health.dead)
}

/// `BuilderComp.canBuild()` (`BuilderComp.java:33-35`):
/// `type.buildSpeed > 0 && buildSpeedMultiplier > 0`.
fn can_build(world: &World, content: &ContentRegistry, entity: Entity) -> bool {
    let build_speed = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
        .map_or(0.0, |def| def.build_speed);
    let multiplier = world
        .get::<crate::entities::comp::unit::StatusComp>(entity)
        .map_or(1.0, |status| status.build_speed_multiplier);
    build_speed > 0.0 && multiplier > 0.0
}

/// `BuilderComp.activelyBuilding()` (`BuilderComp.java:301-309`) for any unit:
/// a queue with a head plan in `type.buildRange` (skipped with infinite
/// resources) while `updateBuilding` is on.
fn actively_building(ctx: &AiCtx, entity: Entity) -> bool {
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

/// `Units.nearby(team, x, y, buildRadius)` first match for the builder scan
/// (`BuilderAI.java:152-169`), iterated in `EntitySeq` order instead of Java's
/// quadtree order (determinism).
fn scan_follow_builder(
    ctx: &mut AiCtx,
    unit: Entity,
    pos: Pos,
    def: &crate::content::registries::units::UnitTypeDef,
) -> Option<Entity> {
    let candidates = crate::entities::comp::unit::queries::in_radius(
        ctx.world,
        pos.x,
        pos.y,
        BUILD_RADIUS,
        Some(ctx.team),
    );
    let speed = ctx.pref_speed(unit);
    for other in candidates {
        if other == unit || !can_build(ctx.world, ctx.content, other) {
            continue;
        }
        if !actively_building(ctx, other) {
            continue;
        }
        let Some(plan) = ctx
            .world
            .get::<BuilderPlans>(other)
            .and_then(|plans| plans.plans.front().cloned())
        else {
            continue;
        };
        // `world.build(plan.x, plan.y) instanceof ConstructBuild`.
        let Some(build) = ctx
            .grid
            .tiles
            .getn(plan.x as i32, plan.y as i32)
            .and_then(|tile| tile.build)
        else {
            continue;
        };
        let Some(cons_cost) = ctx
            .world
            .get::<ConstructState>(build)
            .map(|cons| cons.build_cost)
        else {
            continue;
        };
        let Some(build_pos) = ctx.world.get::<Pos>(build).copied() else {
            continue;
        };
        let dx = build_pos.x - pos.x;
        let dy = build_pos.y - pos.y;
        // `float dist = Math.min(cons.dst(unit) - unit.type.buildRange, 0);`
        let dist = (dx * dx + dy * dy).sqrt() - def.build_range;
        let dist = dist.min(0.0);
        // `if(dist / unit.speed() < cons.buildCost * 0.9f)`.
        if speed > 0.0 && dist / speed < cons_cost * 0.9 {
            return Some(other);
        }
    }
    None
}

/// `onlyAssist`: the closest alive builder player of `team`
/// (`BuilderAI.java:171-185`); the port has a single local player unit.
fn closest_builder_player(ctx: &mut AiCtx, pos: Pos) -> Option<Entity> {
    let candidates = crate::entities::comp::unit::queries::in_radius(
        ctx.world,
        pos.x,
        pos.y,
        f32::MAX,
        Some(ctx.team),
    );
    let mut best: Option<(f32, Entity)> = None;
    for other in candidates {
        if ctx
            .world
            .get::<crate::entities::comp::Player>(other)
            .is_none()
            || !unit_valid(ctx.world, other)
            || !can_build(ctx.world, ctx.content, other)
        {
            continue;
        }
        let Some(opos) = ctx.world.get::<Pos>(other).copied() else {
            continue;
        };
        // `player.dst2(unit)`.
        let dst2 = (opos.x - pos.x).powi(2) + (opos.y - pos.y).powi(2);
        if best.is_none_or(|(best_dst, _)| dst2 < best_dst) {
            best = Some((dst2, other));
        }
    }
    best.map(|(_, entity)| entity)
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

    // `if(assistFollowing != null && !assistFollowing.isValid()) assistFollowing = null;`
    // `if(following != null && !following.isValid()) following = null;`
    if state
        .assist_following
        .is_some_and(|entity| !unit_valid(ctx.world, entity))
    {
        state.assist_following = None;
    }
    if state
        .following
        .is_some_and(|entity| !unit_valid(ctx.world, entity))
    {
        state.following = None;
    }
    // `if(assistFollowing != null && assistFollowing.activelyBuilding()) following = assistFollowing;`
    if let Some(assist) = state.assist_following
        && actively_building(ctx, assist)
    {
        state.following = Some(assist);
    }

    let mut has_plan = ctx
        .world
        .get::<BuilderPlans>(unit)
        .is_some_and(|plans| !plans.plans.is_empty());

    // `boolean moving = false;` (BuilderAI.java:64) — the boost tail below
    // consumes it.
    let mut moving = false;

    if let Some(following) = state.following {
        // `if(following != null)`: try to follow and mimic someone.
        state.retreat_timer = 0.0;
        if !unit_valid(ctx.world, following) || !actively_building(ctx, following) {
            state.following = None;
            if let Some(mut plans) = ctx.world.get_mut::<BuilderPlans>(unit) {
                plans.plans.clear();
            }
            return;
        }
        // `unit.plans.clear(); unit.plans.addFirst(following.buildPlan());`
        // Java aliases the leader's plan object; the port owns a copy.
        if let Some(plan) = ctx
            .world
            .get::<BuilderPlans>(following)
            .and_then(|plans| plans.plans.front().cloned())
        {
            if let Some(mut plans) = ctx.world.get_mut::<BuilderPlans>(unit) {
                plans.plans.clear();
                plans.plans.push_front(plan);
            }
            has_plan = true;
        }
        state.last_plan = None;
    } else if (!has_plan || state.always_flee) && !hold {
        // `(unit.buildPlan() == null || alwaysFlee) && !hold`: retreat when idle.
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
                moving = true;
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
            // Java falls through to the boost tail (BuilderAI.java:231-233).
            apply_builder_boosting(ctx, unit, &def, moving);
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
            // `moving = !unit.within(req.tile(), range)` (BuilderAI.java:130).
            moving = !within;
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
        // Java falls through to the boost tail (BuilderAI.java:231-233).
        apply_builder_boosting(ctx, unit, &def, moving);
        return;
    }

    // `if(assistFollowing != null && !hold)` (no plan): escort the followed
    // builder player; `moveTo(assistFollowing, hitSize + hitSize/2 + 60)`.
    if let Some(assist) = state.assist_following
        && !hold
        && let Some(apos) = ctx.world.get::<Pos>(assist).copied()
    {
        let assist_hit = ctx
            .world
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(assist)
            .and_then(|comp| ctx.content.unit(comp.type_id))
            .map_or(0.0, |def| def.hit_size);
        // `moving = !unit.within(assistFollowing, hitSize + hitSize/2 + 65)`
        // (BuilderAI.java:145).
        moving = ctx.world.get::<Pos>(unit).is_some_and(|pos| {
            let dx = apos.x - pos.x;
            let dy = apos.y - pos.y;
            dx * dx + dy * dy > (assist_hit + def.hit_size / 2.0 + 65.0).powi(2)
        });
        crate::ai::types::flying::move_to_point(
            ctx.world,
            ctx.grid,
            ctx.content,
            unit,
            apos.x,
            apos.y,
            assist_hit + def.hit_size / 2.0 + 60.0,
            100.0,
            false,
        );
    }

    // `if(timer.get(timerTarget2, 20f))`: find a builder to help, then (with
    // `onlyAssist`) refresh the followed player. Java shares this timer id with
    // a 40-tick `Groups.player` break-plan check; the port has no other
    // players, so that check is omitted (deviation).
    state.timer2 -= 1.0;
    if state.timer2 <= 0.0 {
        state.timer2 = 20.0;
        if let Some(following) = scan_follow_builder(ctx, unit, pos, &def) {
            state.following = Some(following);
        }
        if state.only_assist {
            state.assist_following = closest_builder_player(ctx, pos);
        }
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

    // `if(!unit.type.flying) unit.updateBoosting(unit.type.boostWhenBuilding ||
    // moving || floorOn().isDuct || floorOn().damageTaken > 0f ||
    // floorOn().isDeep())` (BuilderAI.java:231-233).
    apply_builder_boosting(ctx, unit, &def, moving);
}

/// `BuilderAI.updateUnit` boost tail (`BuilderAI.java:231-233`): a non-flying
/// builder boosts while `boostWhenBuilding`, moving, on a duct/damaged/deep
/// floor. Like the MinerAI port (`miner.rs`), the per-kind floor terms are
/// limited to deep floors (the port has no duct/damage floor data yet).
fn apply_builder_boosting(
    ctx: &mut AiCtx,
    unit: Entity,
    def: &crate::content::registries::units::UnitTypeDef,
    moving: bool,
) {
    // `UnitComp.updateBoosting`: `if(!type.canBoost || dead) return;`.
    if def.flying || !def.can_boost {
        return;
    }
    let deep = ctx
        .world
        .get::<Pos>(unit)
        .and_then(|pos| {
            ctx.grid.tiles.getn(
                crate::world::WorldGrid::to_tile(pos.x),
                crate::world::WorldGrid::to_tile(pos.y),
            )
        })
        .and_then(|tile| {
            ctx.world
                .get_resource::<crate::world::block::BlockTable>()
                .and_then(|table| {
                    crate::world::behavior::environment::floor_meta(table, tile.floor)
                })
        })
        .is_some_and(|meta| meta.is_deep);
    let boosting = def.boost_when_building || moving || deep;
    if let Some(mut core) = ctx
        .world
        .get_mut::<crate::entities::comp::unit::comp::UnitCore>(unit)
    {
        core.boosting = boosting;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    /// T16 check 3 (`BuilderAI.java:231-233`): the non-flying builder boost tail
    /// sets `boosting` while the unit is `moving` (a far-away plan), skips
    /// units without `canBoost`, and never touches flying units.
    #[test]
    fn builder_boost_tail_follows_moving() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("stell", 0, 64.0, 64.0, 0.0).expect("stell");
        let mut def = harness
            .build
            .content
            .unit_by_name("stell")
            .expect("stell")
            .clone();
        def.flying = false;
        def.can_boost = true;
        def.boost_when_building = false;

        let run = |harness: &mut UnitHarness,
                   def: &crate::content::registries::units::UnitTypeDef,
                   moving: bool| {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            apply_builder_boosting(&mut ctx, unit, def, moving);
            harness
                .build
                .world
                .get::<crate::entities::comp::unit::comp::UnitCore>(unit)
                .expect("core")
                .boosting
        };

        // `boostWhenBuilding` false and standing still: no boost.
        assert!(!run(&mut harness, &def, false), "idle does not boost");
        assert!(run(&mut harness, &def, true), "moving boosts");

        // `UnitComp.updateBoosting` skips units without `canBoost` (the flag
        // keeps its previous value).
        let mut no_boost_def = def.clone();
        no_boost_def.can_boost = false;
        if let Some(mut core) = harness
            .build
            .world
            .get_mut::<crate::entities::comp::unit::comp::UnitCore>(unit)
        {
            core.boosting = false;
        }
        assert!(
            !run(&mut harness, &no_boost_def, true),
            "non-canBoost units stay grounded"
        );

        // Flying units are excluded by the `!unit.type.flying` guard.
        let mut flying_def = def.clone();
        flying_def.flying = true;
        if let Some(mut core) = harness
            .build
            .world
            .get_mut::<crate::entities::comp::unit::comp::UnitCore>(unit)
        {
            core.boosting = false;
        }
        assert!(
            !run(&mut harness, &flying_def, true),
            "flying builders are untouched by the tail"
        );
    }
}
