// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live builder-unit construction: the player unit's build-plan queue and the
//! per-tick `BuilderComp.updateBuildLogic` pass that drives
//! `ConstructBlock.ConstructBuild` progress (PAR-0098).
//!
//! Ported from `core/src/mindustry/entities/comp/BuilderComp.java`
//! (`validatePlans`, `updateBuildLogic`, `shouldSkip`, `addBuild`),
//! `core/src/mindustry/world/Build.java` (`beginPlace`/`beginBreak`) and
//! `core/src/mindustry/world/blocks/ConstructBlock.java`
//! (`construct`/`deconstruct`/`constructFinish`/`deconstructFinish`).
//!
//! Player input (`SimCommand::Place`/`Break` carrying `player: Some(_)`)
//! queues plans on the player unit instead of mutating tiles; the builder pass
//! then swaps the footprint to a `build<size>` construct block, advances it at
//! `1 / buildCost * buildSpeed * buildSpeedMultiplier * rules.buildSpeed(team)`
//! per tick while the unit is within `buildRange`, draws the requirements from
//! the team core as progress accrues (refunding on deconstruction), and sets
//! the real block when done. API placements (`player: None`, MCP/tests/AI)
//! keep the instant-build convention.

use std::collections::VecDeque;

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{BlockId, BlockKind, ContentRegistry};
use crate::ecs::{BuildingComp, EntitySeq};
use crate::entities::comp::unit::comp::{HitboxComp, StatusComp, UnitTypeComp};
use crate::entities::comp::{Building, Pos, TeamComp};
use crate::event::{BlockBrokenEvent, BlockPlacedEvent, SimEvent};
use crate::world::blocks::distribution::item_bridge::ItemBridgeBuild;
use crate::world::blocks::storage::core_block::CoreBuild;
use crate::world::construct::{BuildEconomy, CoreInventory};
use crate::world::modules::ItemModule;
use crate::world::{BlockTable, ConstructState, TilePos, WorldGrid};

use super::Sim;
use super::runtime::SimRuntime;

/// Team id of `Team.derelict`.
const DERELICT: u8 = 0;

/// `Block.bounds(x, y)` (`Block.java:1023-1025`): the world rect a block
/// footprint covers, centered on `(x, y) * tilesize + offset`.
fn block_bounds(
    content: Option<&crate::content::ContentRegistry>,
    x: i32,
    y: i32,
    block: BlockId,
) -> (f32, f32, f32, f32) {
    let (size, offset) = content
        .and_then(|content| content.block(block))
        .map(|def| (def.size, def.offset))
        .unwrap_or((1, 0.0));
    let unit = crate::config::TILESIZE as f32;
    let s = size as f32 * unit;
    (
        x as f32 * unit + offset - s / 2.0,
        y as f32 * unit + offset - s / 2.0,
        s,
        s,
    )
}

/// ARC `Rect.overlaps`: strict edge comparison (`Rect.java`).
fn rects_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 && a.0 + a.2 > b.0 && a.1 < b.1 + b.3 && a.1 + a.3 > b.1
}

/// `Logic.checkOverlappingPlans` (`Logic.java:255-266`): a finished non-breaking
/// placement removes team ghost plans whose bounds overlap its own.
fn check_overlapping_ghosts(
    world: &mut World,
    content: &crate::content::ContentRegistry,
    team: u8,
    x: i32,
    y: i32,
    block: BlockId,
) {
    let Some(mut campaign) = world.get_resource_mut::<crate::game::runtime::CampaignRuntime>()
    else {
        return;
    };
    let bounds = block_bounds(Some(content), x, y, block);
    let data = campaign.session.teams.get(crate::ecs::TeamId(team));
    for plan in data.plans.iter_mut() {
        if rects_overlap(
            bounds,
            block_bounds(Some(content), plan.x as i32, plan.y as i32, plan.block),
        ) {
            plan.removed = true;
        }
    }
    data.plans.retain(|plan| !plan.removed);
}

/// One queued build plan (`BuildPlan` with its transient builder fields).
#[derive(Debug, Clone, PartialEq)]
pub struct UnitBuildPlan {
    /// Target tile x (block centre).
    pub x: i16,
    /// Target tile y (block centre).
    pub y: i16,
    /// Placement rotation.
    pub rotation: u8,
    /// Block to place (`AIR` for a deconstruction).
    pub block: BlockId,
    /// `BuildPlan.config`; applied on finish
    /// (`ConstructBlock.constructFinish`, `ConstructBlock.java:72-118`).
    pub config: crate::world::config::ConfigValue,
    /// Whether this is a deconstruction order.
    pub breaking: bool,
    /// `BuildPlan.initialized`: the construct block was begun.
    pub initialized: bool,
    /// `BuildPlan.stuck`: no progress on the last attempt.
    pub stuck: bool,
    /// `BuildPlan.progress` mirror of the construct progress.
    pub progress: f32,
}

impl UnitBuildPlan {
    /// A placement plan.
    pub fn place(x: i16, y: i16, rotation: u8, block: BlockId) -> Self {
        Self {
            x,
            y,
            rotation,
            block,
            config: crate::world::config::ConfigValue::None,
            breaking: false,
            initialized: false,
            stuck: false,
            progress: 0.0,
        }
    }

    /// A deconstruction plan.
    pub fn break_plan(x: i16, y: i16) -> Self {
        Self {
            breaking: true,
            ..Self::place(x, y, 0, BlockId::AIR)
        }
    }

    fn tile(&self) -> TilePos {
        TilePos::new(self.x, self.y)
    }
}

/// `BuilderComp.plans` + `buildCounter` on a builder unit.
#[derive(Debug, Clone, Component)]
pub struct BuilderPlans {
    /// The plan queue; the head is the active plan.
    pub plans: VecDeque<UnitBuildPlan>,
    /// `buildCounter` (ticks of build work banked, capped at 10).
    pub build_counter: f32,
    /// `BuilderComp.updateBuilding`: whether the queue advances at all
    /// (`BuilderComp.updateBuildLogic` returns early when false; PAR-0098 §2b).
    pub update_building: bool,
    /// `ItemBridge.lastBuild`: the last bridge this builder placed (T11).
    pub last_bridge: Option<(BlockId, TilePos)>,
}

impl Default for BuilderPlans {
    fn default() -> Self {
        Self {
            plans: VecDeque::new(),
            build_counter: 0.0,
            // `BuilderComp.updateBuilding = true` upstream.
            update_building: true,
            last_bridge: None,
        }
    }
}

impl BuilderPlans {
    /// `BuilderComp.addBuild(place, tail)`: replaces a same-position plan.
    pub fn add_build(&mut self, plan: UnitBuildPlan, tail: bool) {
        self.plans
            .retain(|other| other.x != plan.x || other.y != plan.y);
        if tail {
            self.plans.push_back(plan);
        } else {
            self.plans.push_front(plan);
        }
    }
}

/// The rule reads `updateBuildLogic` needs, resolved once per tick.
#[derive(Debug, Clone, Copy)]
struct BuildRuleView {
    /// `state.rules.infiniteResources`.
    state_infinite: bool,
    /// `rules.instantBuild`.
    instant_build: bool,
    /// `state.rules.buildSpeed(team)`.
    build_speed: f32,
    economy: BuildEconomy,
}

impl BuildRuleView {
    fn read(world: &World, team: u8) -> Self {
        if let Some(runtime) = world.get_resource::<crate::game::runtime::CampaignRuntime>() {
            let rules = &runtime.session.rules;
            return Self {
                state_infinite: rules.infinite_resources,
                instant_build: rules.instant_build,
                build_speed: rules.build_speed(team),
                economy: BuildEconomy {
                    cost_multiplier: rules.build_cost_multiplier,
                    refund_multiplier: rules.deconstruct_refund_multiplier,
                    infinite: rules.is_infinite_resources(team),
                },
            };
        }
        let rules = world
            .get_resource::<crate::world::limits::BuildRules>()
            .cloned()
            .unwrap_or_default();
        let infinite = rules.infinite_resources || rules.cheat;
        Self {
            state_infinite: infinite,
            instant_build: rules.instant_build,
            build_speed: 1.0,
            economy: BuildEconomy {
                cost_multiplier: rules.build_cost_multiplier,
                refund_multiplier: rules.deconstruct_refund_multiplier,
                infinite,
            },
        }
    }
}

/// `Unit.core()` → `team.core()`: the team's first core (lowest entity seq).
fn team_core(world: &World, team: u8) -> Option<Entity> {
    let mut best: Option<(u64, Entity)> = None;
    for entity in world.iter_entities() {
        if entity.get::<CoreBuild>().is_none()
            || entity.get::<TeamComp>().map(|t| t.team) != Some(team)
        {
            continue;
        }
        let seq = entity.get::<EntitySeq>().map(|s| s.0).unwrap_or(u64::MAX);
        if best.is_none_or(|(best_seq, _)| seq < best_seq) {
            best = Some((seq, entity.id()));
        }
    }
    best.map(|(_, entity)| entity)
}

fn building_team(world: &World, entity: Entity) -> u8 {
    world
        .get::<TeamComp>(entity)
        .map(|t| t.team)
        .unwrap_or(DERELICT)
}

/// `BuilderComp.validatePlans`: drops plans that are already satisfied.
fn validate_plans(
    plans: &mut BuilderPlans,
    world: &World,
    grid: &WorldGrid,
    content: &ContentRegistry,
) {
    plans.plans.retain(|plan| {
        let pos = plan.tile();
        if grid.index(pos).is_err() {
            return false;
        }
        let block = grid.block_at(pos).unwrap_or(BlockId::AIR);
        if plan.breaking {
            return block != BlockId::AIR;
        }
        let build = grid.entity_at(pos);
        let same_derelict = build.is_some_and(|entity| {
            block == plan.block
                && world.get::<Building>(entity).map(|b| b.tile) == Some(pos)
                && building_team(world, entity) == DERELICT
        });
        let rotates = content
            .block(plan.block)
            .is_some_and(|def| crate::world::block::kind_rotates(def.kind));
        let same_rotation = build.is_some_and(|entity| {
            world.get::<Building>(entity).map(|b| b.rotation) == Some(plan.rotation)
        });
        let done = ((same_rotation && !same_derelict) || !rotates)
            && block == plan.block
            && !same_derelict;
        !done
    });
}

/// `BuilderComp.shouldSkip`: a plan the core cannot fund right now.
fn should_skip(
    plan: &UnitBuildPlan,
    core: Option<&ItemModule>,
    content: &ContentRegistry,
    rules: &BuildRuleView,
) -> bool {
    let Some(core) = core else {
        return false;
    };
    if rules.economy.infinite || plan.breaking {
        return false;
    }
    let Some(def) = content.block(plan.block) else {
        return false;
    };
    let has_all = def
        .requirements
        .iter()
        .all(|stack| core.get(stack.item) >= stack.amount);
    (plan.stuck && !has_all)
        || def.requirements.iter().any(|stack| {
            core.get(stack.item) < stack.amount.min(15)
                && (stack.amount as f32 * rules.economy.cost_multiplier).round() as i32 > 0
        })
}

/// Tile centre in world pixels (`Tile.worldx()`).
fn tile_world(pos: TilePos) -> (f32, f32) {
    let unit = crate::config::TILESIZE as f32;
    (f32::from(pos.x()) * unit, f32::from(pos.y()) * unit)
}

fn within(ux: f32, uy: f32, pos: TilePos, range: f32) -> bool {
    let (tx, ty) = tile_world(pos);
    let (dx, dy) = (tx - ux, ty - uy);
    dx * dx + dy * dy <= range * range
}

/// `Build.checkNoUnitOverlap(type, x, y)`: whether no ground unit's tile hitbox
/// overlaps the block footprint (`Units.anyEntities` with the default ground
/// filter; `HitboxComp.hitboxTile` is `min(hitSize * 0.66, 7.8)`).
pub(crate) fn no_unit_overlap(
    world: &World,
    content: &crate::content::ContentRegistry,
    pos: TilePos,
    block: BlockId,
) -> bool {
    let Some(def) = content.block(block) else {
        return true;
    };
    if !def.solid {
        // `(!type.solid && !type.solidifes)` short-circuits the query.
        return true;
    }
    !unit_overlap_at(world, content, pos, block)
}

/// `Units.anyEntities(tile)` with the default ground filter (T13 door tap):
/// whether a grounded, non-legged unit's tile hitbox overlaps the footprint.
pub fn unit_overlap_at(
    world: &World,
    content: &crate::content::ContentRegistry,
    pos: TilePos,
    block: BlockId,
) -> bool {
    let Some(def) = content.block(block) else {
        return false;
    };
    let unit = crate::config::TILESIZE as f32;
    let size = def.size.max(1) as f32;
    // `x * tilesize + offset - size * tilesize / 2` (Arc `Rect` strict overlap).
    let offset = ((size as i32 + 1) % 2) as f32 * unit / 2.0;
    let rect_x = f32::from(pos.x()) * unit + offset - size * unit / 2.0;
    let rect_y = f32::from(pos.y()) * unit + offset - size * unit / 2.0;
    let rect_size = size * unit;

    for entity in world.iter_entities() {
        let Some(type_comp) = entity.get::<UnitTypeComp>() else {
            continue;
        };
        // `Units.anyEntities(.., ground = true)`: `isGrounded()` is
        // `elevation < 0.001` (`UnitComp`/`ElevationMoveComp`) and legged units
        // never block (`!unit.type.allowLegStep`, `Units.java:29-35`).
        let grounded = entity
            .get::<crate::entities::comp::unit::comp::UnitCore>()
            .is_some_and(|core| core.elevation < 0.001);
        if !grounded {
            continue;
        }
        if content
            .unit(type_comp.type_id)
            .is_some_and(|def| def.allow_leg_step)
        {
            continue;
        }
        let Some(unit_pos) = entity.get::<Pos>() else {
            continue;
        };
        let hit_size = entity
            .get::<HitboxComp>()
            .map(|hitbox| hitbox.hit_size)
            .unwrap_or(0.0);
        let hit = (hit_size * 0.66).min(7.8);
        let hx = unit_pos.x - hit / 2.0;
        let hy = unit_pos.y - hit / 2.0;
        if rect_x < hx + hit
            && rect_x + rect_size > hx
            && rect_y < hy + hit
            && rect_y + rect_size > hy
        {
            return true;
        }
    }
    false
}

/// Result of one builder pass.
#[derive(Default)]
struct BuilderOutcome {
    /// The grid changed (construct begun/finished/removed or rotated).
    changed: bool,
    events: Vec<SimEvent>,
    /// `Trigger.derelictBreak` (G05 T4b): the player deconstructed a team-0
    /// (derelict) building.
    derelict_break: bool,
}

/// `ItemBridge.playerPlaced` (`ItemBridge.java:224-233`; T11): the previously
/// placed bridge of the same block links itself to the newly built bridge
/// (`findLink` requires the old bridge to be still unlinked and in range).
///
/// Only `ItemBridge`/`BufferedItemBridge` chain; Java's `LiquidBridge` has no
/// `playerPlaced`/`findLink`.
fn chain_bridge(
    world: &mut World,
    grid: &WorldGrid,
    runtime: &SimRuntime,
    placed: Entity,
    plans: &mut BuilderPlans,
    block: BlockId,
    pos: TilePos,
) {
    let Some(def) = runtime.content().block(block) else {
        return;
    };
    if !matches!(
        def.kind,
        BlockKind::ItemBridge | BlockKind::BufferedItemBridge
    ) {
        return;
    }
    let previous = plans.last_bridge.filter(|(last_block, last_pos)| {
        *last_block == block
            && *last_pos != pos
            && grid.entity_at(*last_pos).is_some_and(|entity| {
                world
                    .get::<ItemBridgeBuild>(entity)
                    .is_some_and(|bridge| bridge.link == -1)
            })
    });
    if let Some((_, last_pos)) = previous
        && let Some(last_entity) = grid.entity_at(last_pos)
        && let Some(table) = world.get_resource::<BlockTable>()
        && let Some(inst) = table.instance(block)
        && inst.behavior.link_valid_between(world, placed, last_entity)
    {
        // `link.build.configure(tile.pos())`: the old bridge points at the new
        // one (packed absolute position, the `Number` config path).
        inst.behavior.configured(
            world,
            last_entity,
            None,
            crate::world::config::ConfigValue::Number(f64::from(pos.pack())),
        );
    }
    plans.last_bridge = Some((block, pos));
}

/// `BuilderComp.updateBuildLogic` for one builder unit.
fn update_builder(
    world: &mut World,
    grid: &mut WorldGrid,
    runtime: &mut SimRuntime,
    unit: Entity,
    plans: &mut BuilderPlans,
) -> BuilderOutcome {
    let mut out = BuilderOutcome::default();
    let Some(type_id) = world.get::<UnitTypeComp>(unit).map(|comp| comp.type_id) else {
        return out;
    };
    let Some((build_speed, build_range)) = runtime
        .content()
        .unit(type_id)
        .map(|def| (def.build_speed, def.build_range))
    else {
        return out;
    };
    if build_speed <= 0.0 {
        return out;
    }
    validate_plans(plans, world, grid, runtime.content());
    if !plans.update_building {
        // `BuilderComp.updateBuildLogic`: `if(!updateBuilding) return;`.
        return out;
    }

    let speed_multiplier = world
        .get::<StatusComp>(unit)
        .map(|status| status.build_speed_multiplier)
        .unwrap_or(1.0);
    if speed_multiplier <= 0.0 {
        return out;
    }
    let team = building_team(world, unit);
    let rules = BuildRuleView::read(world, team);
    let place_dst = if rules.state_infinite {
        f32::MAX.sqrt()
    } else {
        build_range
    };

    plans.build_counter += 1.0;
    if !plans.build_counter.is_finite() {
        plans.build_counter = 0.0;
    }
    plans.build_counter = plans.build_counter.min(10.0);

    let instant = rules.instant_build && rules.state_infinite;
    let max_per_frame = if instant { plans.plans.len() } else { 10 };
    let mut count = 0usize;

    let core = team_core(world, team);
    if core.is_none() && !rules.economy.infinite {
        return out;
    }
    let Some((ux, uy)) = world.get::<Pos>(unit).map(|pos| (pos.x, pos.y)) else {
        return out;
    };

    while (plans.build_counter >= 1.0 || instant)
        && count < max_per_frame
        && !plans.plans.is_empty()
    {
        count += 1;
        plans.build_counter -= 1.0;

        // Find the next build plan: rotate the queue to the first in-range
        // plan the core can fund (`shouldSkip`).
        if plans.plans.len() > 1 {
            let size = plans.plans.len();
            let mut total = 0usize;
            let mut best_dst = f32::MAX;
            let mut found_any = false;
            let mut best_index: Option<usize> = None;
            while total < size {
                let Some(plan) = plans.plans.front().cloned() else {
                    break;
                };
                let (tx, ty) = tile_world(plan.tile());
                let dst = (tx - ux) * (tx - ux) + (ty - uy) * (ty - uy);
                let in_range = dst <= place_dst * place_dst;
                let core_items = core.and_then(|core| world.get::<ItemModule>(core));
                if in_range && !should_skip(&plan, core_items, runtime.content(), &rules) {
                    found_any = true;
                    break;
                } else if in_range && dst < best_dst {
                    best_index = Some(total);
                    best_dst = dst;
                }
                plans.plans.rotate_left(1);
                total += 1;
            }
            if !found_any
                && let Some(best_index) = best_index.filter(|index| *index > 0)
                && let Some(head) = plans.plans.front()
                && !within(ux, uy, head.tile(), place_dst)
            {
                plans.plans.rotate_left(best_index);
            }
        }

        let Some(mut current) = plans.plans.front().cloned() else {
            break;
        };
        let pos = current.tile();
        if !within(ux, uy, pos, place_dst) {
            continue;
        }

        let allow_current = current.breaking || runtime.placement_allowed(current.block);
        let construct_at = |world: &World, grid: &WorldGrid| {
            grid.entity_at(pos)
                .filter(|entity| world.get::<ConstructState>(*entity).is_some())
        };

        match construct_at(world, grid) {
            None => {
                if !current.initialized && !current.breaking && allow_current {
                    // Quick rotation of an existing same block
                    // (`Build.beginPlace` auto-rotate branch).
                    if let Some(entity) = grid.entity_at(pos)
                        && grid.block_at(pos) == Some(current.block)
                        && building_team(world, entity) == team
                        && runtime
                            .content()
                            .block(current.block)
                            .is_some_and(|def| def.quick_rotate)
                    {
                        if let Some(mut building) = world.get_mut::<Building>(entity) {
                            building.rotation = current.rotation;
                        }
                        if let Some(mut comp) = world.get_mut::<BuildingComp>(entity) {
                            comp.rot = current.rotation;
                        }
                        runtime.refresh_proximity(world, grid, pos.x(), pos.y());
                        crate::world::update::no_sleep(world, entity);
                        plans.plans.pop_front();
                        out.changed = true;
                        continue;
                    }
                    if !runtime.can_place(
                        world,
                        grid,
                        pos.x(),
                        pos.y(),
                        current.block,
                        current.rotation,
                        team,
                        crate::world::build::PlaceFlags::PLAYER,
                    ) {
                        plans.plans.pop_front();
                        continue;
                    }
                    // `Build.checkNoUnitOverlap`: a ground unit standing on the
                    // footprint rotates the plan to the back of the queue.
                    if !no_unit_overlap(world, runtime.content(), pos, current.block) {
                        plans.plans.rotate_left(1);
                        out.changed = true;
                        continue;
                    }
                    let has_all = rules.economy.infinite
                        || runtime.content().block(current.block).is_some_and(|def| {
                            let items = core.and_then(|core| world.get::<ItemModule>(core));
                            def.requirements.iter().all(|stack| {
                                let need = ((stack.amount as f32 * rules.economy.cost_multiplier)
                                    .round() as i32)
                                    .min(1);
                                items.is_some_and(|items| items.get(stack.item) >= need)
                            })
                        });
                    if has_all {
                        if runtime.begin_place(
                            world,
                            grid,
                            pos.x(),
                            pos.y(),
                            current.block,
                            current.rotation,
                            team,
                            crate::world::build::PlaceFlags::PLAYER,
                            rules.economy.cost_multiplier,
                        ) {
                            // `BuilderComp` passes `current.config` into the
                            // construct (`Call.beginPlace` L173 / `construct`
                            // L217); `ConstructBuild.construct` stores it as
                            // `lastConfig` (`ConstructBlock.java:278-288`).
                            if !current.breaking
                                && let Some(construct) = grid.entity_at(pos)
                                && let Some(mut state) = world.get_mut::<ConstructState>(construct)
                            {
                                state.last_config = current.config.clone();
                            }
                            out.changed = true;
                        }
                    } else {
                        current.stuck = true;
                        if let Some(head) = plans.plans.front_mut() {
                            head.stuck = true;
                        }
                    }
                } else if !current.initialized
                    && current.breaking
                    && grid.entity_at(pos).is_some_and(|entity| {
                        let owner = building_team(world, entity);
                        owner == team || owner == DERELICT
                    })
                {
                    if runtime.begin_break(
                        world,
                        grid,
                        pos.x(),
                        pos.y(),
                        team,
                        rules.economy.cost_multiplier,
                    ) {
                        out.changed = true;
                    } else {
                        plans.plans.pop_front();
                        continue;
                    }
                } else {
                    plans.plans.pop_front();
                    continue;
                }
            }
            Some(entity) => {
                let owner = building_team(world, entity);
                let state_current = world
                    .get::<ConstructState>(entity)
                    .map(|state| state.current);
                let anchor = world.get::<Building>(entity).map(|b| b.tile);
                if (owner != team && owner != DERELICT)
                    || (!current.breaking
                        && (state_current != Some(current.block) || anchor != Some(pos)))
                {
                    plans.plans.pop_front();
                    continue;
                }
            }
        }

        let Some(entity) = construct_at(world, grid) else {
            continue;
        };
        if !current.initialized {
            current.initialized = true;
        }
        let Some(mut state) = world.get::<ConstructState>(entity).cloned() else {
            continue;
        };
        let requirements = runtime
            .content()
            .block(state.current)
            .map(|def| def.requirements.clone())
            .unwrap_or_default();
        let cost = if state.build_cost > 0.0 {
            state.build_cost
        } else {
            1.0
        };
        let bs = 1.0 / cost * build_speed * speed_multiplier * rules.build_speed;
        let capacity = core
            .and_then(|core| world.get::<CoreBuild>(core))
            .map(|build| build.storage_capacity)
            .unwrap_or(0);

        let finished = {
            let mut module = core.and_then(|core| world.get_mut::<ItemModule>(core));
            let inventory = module
                .as_deref_mut()
                .map(|items| CoreInventory { items, capacity });
            if current.breaking {
                state.deconstruct_step(
                    &requirements,
                    inventory,
                    bs,
                    &rules.economy,
                    rules.state_infinite,
                )
            } else if allow_current {
                state.construct_step(
                    &requirements,
                    inventory,
                    bs,
                    &rules.economy,
                    rules.state_infinite,
                )
            } else {
                false
            }
        };
        current.stuck = (current.progress - state.progress).abs() < f32::EPSILON;
        current.progress = state.progress;
        let anchor = world.get::<Building>(entity).map(|b| b.tile).unwrap_or(pos);
        let rotation = world
            .get::<Building>(entity)
            .map(|b| b.rotation)
            .unwrap_or(current.rotation);
        if !current.breaking {
            // `BuilderComp.java:217`: the builder passes its plan config on
            // every `construct` step; `ConstructBuild.construct` stores it as
            // `lastConfig` (`ConstructBlock.java:288`).
            state.last_config = current.config.clone();
        }
        world.entity_mut(entity).insert(state.clone());
        if let Some(head) = plans.plans.front_mut() {
            *head = current.clone();
        }

        if finished {
            if current.breaking {
                // `ConstructBlock.deconstructFinish` -> `tile.remove()`.
                // `Trigger.derelictBreak` (G05 T4b): the removed building's
                // team, read before removal.
                let removed_team = grid
                    .entity_at(anchor)
                    .and_then(|entity| world.get::<BuildingComp>(entity))
                    .map(|comp| comp.team.0);
                if runtime
                    .remove_block_raw(world, grid, anchor.x(), anchor.y())
                    .is_some()
                {
                    if removed_team == Some(DERELICT) {
                        out.derelict_break = true;
                    }
                    emit_construct_fx(
                        world,
                        runtime,
                        state.current,
                        (anchor.x() as i32, anchor.y() as i32),
                        true,
                    );
                    out.events
                        .push(SimEvent::BlockBrokenEvent(BlockBrokenEvent {
                            x: anchor.x(),
                            y: anchor.y(),
                            block: state.current,
                            // `deconstructFinish` (break sound).
                            destroyed: false,
                            team,
                        }));
                }
            } else {
                // `ConstructBlock.constructFinish` -> `tile.setBlock(block, team, rotation)`
                // then `tile.build.placed()` (`ConstructBlock.java:151-156`).
                runtime.set_block_raw(
                    world,
                    grid,
                    anchor.x(),
                    anchor.y(),
                    state.current,
                    rotation,
                    team,
                );
                if let Some(placed) = grid.entity_at(anchor) {
                    let grid_ref: &WorldGrid = grid;
                    runtime.placed_hooks(world, grid_ref, placed);
                    // `ConstructBlock.constructFinish` (`ConstructBlock.java:72-118`):
                    // after `tile.setBlock`, a non-null config is applied.
                    if !matches!(current.config, crate::world::config::ConfigValue::None) {
                        let _ = crate::world::config::configure(
                            world,
                            placed,
                            Some(unit),
                            current.config.clone(),
                        );
                    }
                    chain_bridge(
                        world,
                        grid_ref,
                        runtime,
                        placed,
                        plans,
                        state.current,
                        anchor,
                    );
                }
                emit_construct_fx(
                    world,
                    runtime,
                    state.current,
                    (anchor.x() as i32, anchor.y() as i32),
                    false,
                );
                out.events
                    .push(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                        x: anchor.x(),
                        y: anchor.y(),
                        block: state.current,
                        team,
                    }));
                // `Logic` `BlockBuildEndEvent` -> `checkOverlappingPlans`
                // (`Logic.java:53-58`, `255-266`).
                check_overlapping_ghosts(
                    world,
                    runtime.content(),
                    team,
                    anchor.x() as i32,
                    anchor.y() as i32,
                    state.current,
                );
            }
            out.changed = true;
        }
    }
    out
}

impl Sim {
    /// Whether `SimCommand::Place`/`Break` from player input should queue on
    /// the player unit (`InputHandler.flushPlans`) rather than apply
    /// instantly: a live block runtime and a player unit that can build.
    pub fn player_can_build(&self) -> bool {
        let (Some(unit), Some(runtime)) = (self.player_unit, self.block_runtime.as_ref()) else {
            return false;
        };
        self.ecs
            .0
            .get::<UnitTypeComp>(unit)
            .and_then(|comp| runtime.content().unit(comp.type_id))
            .is_some_and(|def| def.build_speed > 0.0)
    }

    /// Whether `team` owns a core. Player plans with no build-capable unit
    /// are dropped in any world where the player's team has a core (Java:
    /// `Nulls.unit.addBuild` is a no-op, `BuilderComp.java` `addBuild`
    /// `if(!canBuild()) return;`). Core-less dev worlds keep the port's
    /// instant placement (no Java counterpart; spine/determinism scenarios).
    pub(crate) fn team_has_core(&self, team: u8) -> bool {
        team_core(&self.ecs.0, team).is_some()
    }

    /// Team of player-issued commands that carry no team (`Break`): the
    /// session `rules.defaultTeam` (`Vars.player.team()` in campaign).
    pub(crate) fn player_command_team(&self) -> u8 {
        crate::game::runtime::rules_of(&self.ecs.0).map_or(1, |rules| rules.default_team)
    }

    /// Queues a placement on the player unit (`InputHandler.flushPlans` ->
    /// `player.unit().addBuild(plan)`); invalid placements are dropped.
    pub fn queue_player_place(&mut self, x: i16, y: i16, block: BlockId, rotation: u8) -> bool {
        self.queue_player_place_config(
            x,
            y,
            block,
            rotation,
            crate::world::config::ConfigValue::None,
        )
    }

    /// Queues a placement carrying a config (`InputHandler.updateLine` plan
    /// configs, schematic copies, rebuild ghosts; the value is applied when the
    /// construct completes).
    pub fn queue_player_place_config(
        &mut self,
        x: i16,
        y: i16,
        block: BlockId,
        rotation: u8,
        config: crate::world::config::ConfigValue,
    ) -> bool {
        let Some(unit) = self.player_unit else {
            return false;
        };
        let team = self.live_default_team();
        let Some(runtime) = self.block_runtime.as_ref() else {
            return false;
        };
        if !runtime.can_place(
            &self.ecs.0,
            &self.grid,
            x,
            y,
            block,
            rotation,
            team,
            crate::world::build::PlaceFlags::PLAYER,
        ) {
            return false;
        }
        let mut plan = UnitBuildPlan::place(x, y, rotation, block);
        plan.config = config;
        // `addBuild`: carry the progress of an in-flight construct block.
        if let Some(entity) = self.grid.entity_at(TilePos::new(x, y))
            && let Some(state) = self.ecs.0.get::<ConstructState>(entity)
        {
            plan.progress = state.progress;
        }
        self.with_player_plans(unit, |plans| plans.add_build(plan, true));
        true
    }

    /// `InputHandler.rebuildArea` (`InputHandler.java:2318-2330`): re-queues
    /// every live team ghost plan whose block bounds overlap the selection
    /// rect. Returns the queued count. (`tryRepairDerelict` over the rect,
    /// `InputHandler.java:2331-2341`, is deferred: it needs the G02 T8
    /// derelict-repair branches in `Build.validPlace`.)
    pub fn queue_team_rebuild(&mut self, x1: i32, y1: i32, x2: i32, y2: i32) -> usize {
        let team = self.live_default_team();
        let r = crate::input::placement::normalize_area(x1, y1, x2, y2, 0, false, 999_999_999);
        let rect = (
            (r.x * 8) as f32,
            (r.y * 8) as f32,
            ((r.x2 - r.x) * 8) as f32,
            ((r.y2 - r.y) * 8) as f32,
        );
        let content = self.block_runtime.as_ref().map(|runtime| runtime.content());
        // `for(BlockPlan plan : player.team().data().plans)` (front to back).
        let ghosts: Vec<crate::game::teams::BlockPlan> = self
            .ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .and_then(|campaign| campaign.session.teams.get_or_null(crate::ecs::TeamId(team)))
            .map(|data| {
                data.plans
                    .iter()
                    .filter(|plan| {
                        rects_overlap(
                            block_bounds(content, plan.x as i32, plan.y as i32, plan.block),
                            rect,
                        )
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut queued = 0usize;
        for plan in ghosts {
            let rotation = plan.rotation.rem_euclid(4) as u8;
            if self.queue_player_place_config(
                plan.x,
                plan.y,
                plan.block,
                rotation,
                plan.config.clone().unwrap_or_default(),
            ) {
                queued += 1;
            }
        }
        queued
    }

    /// `InputHandler.removeSelection`'s plan half (`InputHandler.java:1905-1935`):
    /// remove non-breaking player plans whose bounds overlap the clamped rect,
    /// and drop team ghost plans overlapping it (`plan.removed = true`).
    /// Runs even when no building sits in the rect.
    pub fn remove_plans_in(&mut self, x1: i32, y1: i32, x2: i32, y2: i32, max_length: i32) {
        let r = crate::input::placement::normalize_area(x1, y1, x2, y2, 0, false, max_length);
        let rect = (
            (r.x * 8) as f32,
            (r.y * 8) as f32,
            ((r.x2 - r.x) * 8) as f32,
            ((r.y2 - r.y) * 8) as f32,
        );
        // Collect first (content borrows), then mutate the queue.
        let plan_positions: Vec<(i16, i16)> = if let Some(unit) = self.player_unit {
            let content = self.block_runtime.as_ref().map(|runtime| runtime.content());
            self.ecs
                .0
                .get::<BuilderPlans>(unit)
                .map(|plans| {
                    plans
                        .plans
                        .iter()
                        .filter(|plan| !plan.breaking)
                        .filter(|plan| {
                            rects_overlap(
                                block_bounds(content, plan.x as i32, plan.y as i32, plan.block),
                                rect,
                            )
                        })
                        .map(|plan| (plan.x, plan.y))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if let Some(unit) = self.player_unit {
            self.with_player_plans(unit, |plans| {
                plans
                    .plans
                    .retain(|plan| !plan_positions.contains(&(plan.x, plan.y)));
            });
        }
        if let Some(mut campaign) = self
            .ecs
            .0
            .get_resource_mut::<crate::game::runtime::CampaignRuntime>()
        {
            let content = self.block_runtime.as_ref().map(|runtime| runtime.content());
            let team = campaign.session.rules.default_team;
            let data = campaign.session.teams.get(crate::ecs::TeamId(team));
            for plan in data.plans.iter_mut() {
                if rects_overlap(
                    block_bounds(content, plan.x as i32, plan.y as i32, plan.block),
                    rect,
                ) {
                    plan.removed = true;
                }
            }
            data.plans.retain(|plan| !plan.removed);
        }
    }

    /// `BuilderComp.activelyBuilding` for the live player unit
    /// (`BuilderComp.java:301-309`): the `canShoot` gate for left presses.
    pub fn player_actively_building(&self) -> bool {
        let Some(unit) = self.player_unit else {
            return false;
        };
        let Some(pos) = self.ecs.0.get::<Pos>(unit).map(|pos| (pos.x, pos.y)) else {
            return false;
        };
        let Some(runtime) = self.unit_runtime() else {
            return false;
        };
        crate::ai::types::player::player_actively_building(&self.ecs.0, &runtime.content, unit, pos)
            .is_some()
    }

    /// `InputHandler.validPlace(x, y, block, rot)` for the local team: the
    /// placement-preview validity check, no side effects and nothing queued
    /// (`InputHandler.java:2029-2032` calls `Build.validPlace`).
    pub fn preview_can_place(&self, x: i16, y: i16, block: BlockId, rotation: u8) -> bool {
        self.valid_place_player(x, y, block, rotation)
    }

    /// Queues a deconstruction on the player unit
    /// (`InputHandler.removeSelection`); a queued placement covering the tile
    /// is removed instead.
    pub fn queue_player_break(&mut self, x: i16, y: i16) -> bool {
        let Some(unit) = self.player_unit else {
            return false;
        };
        let content_sizes: Option<Vec<(i16, i16, i32)>> = {
            let runtime = self.block_runtime.as_ref();
            self.ecs.0.get::<BuilderPlans>(unit).map(|plans| {
                plans
                    .plans
                    .iter()
                    .filter(|plan| !plan.breaking)
                    .map(|plan| {
                        let size = runtime
                            .and_then(|runtime| runtime.content().block(plan.block))
                            .map(|def| def.size)
                            .unwrap_or(1);
                        (plan.x, plan.y, size)
                    })
                    .collect()
            })
        };
        if let Some(sizes) = content_sizes {
            let hit = sizes.into_iter().find(|(px, py, size)| {
                let lo = -((size - 1) / 2);
                let hi = size / 2;
                let (dx, dy) = (i32::from(x - px), i32::from(y - py));
                dx >= lo && dx <= hi && dy >= lo && dy <= hi
            });
            if let Some((px, py, _)) = hit {
                self.with_player_plans(unit, |plans| {
                    plans
                        .plans
                        .retain(|plan| plan.breaking || plan.x != px || plan.y != py);
                });
                return true;
            }
        }
        let pos = TilePos::new(x, y);
        if self.grid.index(pos).is_err() {
            return false;
        }
        let Some(entity) = self.grid.entity_at(pos) else {
            return false;
        };
        let anchor = self
            .ecs
            .0
            .get::<Building>(entity)
            .map(|b| b.tile)
            .unwrap_or(pos);
        let team = self.live_default_team();
        let rules = self
            .ecs
            .0
            .get_resource::<crate::world::limits::BuildRules>()
            .cloned()
            .unwrap_or_default();
        let Some(runtime) = self.block_runtime.as_ref() else {
            return false;
        };
        if !crate::world::build::valid_break(
            runtime.content(),
            &self.grid,
            Some(&self.ecs.0),
            &rules,
            team,
            i32::from(anchor.x()),
            i32::from(anchor.y()),
        ) {
            return false;
        }
        self.with_player_plans(unit, |plans| {
            plans.add_build(UnitBuildPlan::break_plan(anchor.x(), anchor.y()), true);
        });
        true
    }

    fn with_player_plans(&mut self, unit: Entity, f: impl FnOnce(&mut BuilderPlans)) {
        let Ok(mut entity) = self.ecs.0.get_entity_mut(unit) else {
            return;
        };
        match entity.get_mut::<BuilderPlans>() {
            Some(mut plans) => f(&mut plans),
            None => {
                let mut plans = BuilderPlans::default();
                f(&mut plans);
                entity.insert(plans);
            }
        }
    }

    /// The player unit's queued plans in queue order (head = active).
    pub fn player_build_plans(&self) -> Vec<UnitBuildPlan> {
        self.player_unit
            .and_then(|unit| self.ecs.0.get::<BuilderPlans>(unit))
            .map(|plans| plans.plans.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// `player.unit().plans.size` (`BuilderComp.plans`; the `buildautopause`
    /// auto-pause and deselect-mining gates, `DesktopInput.java:593/609`)
    /// without cloning the queue.
    pub fn player_plan_count(&self) -> usize {
        self.player_unit
            .and_then(|unit| self.ecs.0.get::<BuilderPlans>(unit))
            .map(|plans| plans.plans.len())
            .unwrap_or(0)
    }

    /// `build.isCommandable()` own buildings (`DesktopInput.java:346-357`):
    /// cores, unit factories, reconstructors, assemblers and payload sources,
    /// in `EntitySeq` order (`(x, y)` anchor tiles).
    pub fn commandable_buildings(&self, team: u8) -> Vec<(i16, i16)> {
        use crate::content::BlockKind;
        let Some(runtime) = self.block_runtime.as_ref() else {
            return Vec::new();
        };
        let content = runtime.content();
        self.ecs
            .entities_by_seq()
            .into_iter()
            .filter(|(_, _, comp)| {
                comp.team.0 == team
                    && content.block(comp.block).is_some_and(|def| {
                        matches!(
                            def.kind,
                            BlockKind::CoreBlock
                                | BlockKind::UnitFactory
                                | BlockKind::Reconstructor
                                | BlockKind::UnitAssembler
                                | BlockKind::PayloadSource
                        )
                    })
            })
            .map(|(_, _, comp)| (comp.pos.x(), comp.pos.y()))
            .collect()
    }

    /// Clears the player unit's queue (`BuilderComp.clearBuilding`).
    pub fn clear_player_build_plans(&mut self) {
        if let Some(unit) = self.player_unit
            && let Some(mut plans) = self.ecs.0.get_mut::<BuilderPlans>(unit)
        {
            plans.plans.clear();
        }
    }

    /// `BuilderComp.removeBuild`-style removal for the delete-plans command:
    /// drops every queued plan whose [`TilePos::pack`] is in `positions`
    /// (`InputHandler.deletePlans` over the player's own queue).
    pub fn delete_player_plans(&mut self, positions: &[i32]) {
        if let Some(unit) = self.player_unit
            && let Some(mut plans) = self.ecs.0.get_mut::<BuilderPlans>(unit)
        {
            plans
                .plans
                .retain(|plan| !positions.contains(&TilePos::new(plan.x, plan.y).pack()));
        }
    }

    /// `BuilderComp.updateBuilding` (`InputHandler.update`:
    /// `player.unit().updateBuilding = isBuilding`; PAR-0098 §2b).
    pub fn set_player_building_paused(&mut self, paused: bool) {
        if let Some(unit) = self.player_unit {
            // The component is inserted lazily by the first queued plan, so the
            // pause flag must create it when absent.
            self.with_player_plans(unit, |plans| plans.update_building = !paused);
        }
    }

    /// Construct progress at `(x, y)` as `(current block, progress,
    /// deconstructing)`, if the tile holds a construct block.
    pub fn construct_progress(&self, x: i16, y: i16) -> Option<(BlockId, f32, bool)> {
        let entity = self.grid.entity_at(TilePos::new(x, y))?;
        let state = self.ecs.0.get::<ConstructState>(entity)?;
        Some((state.current, state.progress, state.active_deconstruct))
    }

    /// Runs `BuilderComp.updateBuildLogic` for every builder unit with plans
    /// (today the player unit). Called once per tick from [`Sim::tick`].
    pub(crate) fn update_builders(&mut self) {
        // `Groups.unit` (G01 T10c): every unit with a non-empty plan queue
        // advances its own `BuilderComp`; iteration is `EntitySeq` order and
        // the player unit is not special-cased (its events additionally feed
        // the hint sets).
        let player = self
            .player_unit
            .filter(|unit| self.ecs.0.get_entity(*unit).is_ok());
        let mut units: Vec<Entity> = self
            .ecs
            .0
            .get_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            .map(|runtime| runtime.units.clone())
            .unwrap_or_default();
        units.sort_unstable_by_key(|entity| {
            self.ecs
                .0
                .get::<crate::ecs::EntitySeq>(*entity)
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX)
        });
        for unit in units {
            if self.ecs.0.get_entity(unit).is_err() {
                continue;
            }
            let has_plans = self
                .ecs
                .0
                .get::<BuilderPlans>(unit)
                .is_some_and(|plans| !plans.plans.is_empty());
            if !has_plans {
                continue;
            }
            let Some(mut plans) = self
                .ecs
                .0
                .get_mut::<BuilderPlans>(unit)
                .map(|mut plans| std::mem::take(&mut *plans))
            else {
                continue;
            };
            let Some(mut runtime) = self.block_runtime.take() else {
                if self.player_unit == Some(unit) {
                    self.with_player_plans(unit, |slot| *slot = plans);
                } else if let Some(mut stored) = self.ecs.0.get_mut::<BuilderPlans>(unit) {
                    *stored = plans;
                }
                return;
            };
            let outcome = update_builder(
                &mut self.ecs.0,
                &mut self.grid,
                &mut runtime,
                unit,
                &mut plans,
            );
            self.block_runtime = Some(runtime);
            if self.player_unit == Some(unit) {
                self.with_player_plans(unit, |slot| *slot = plans);
            } else if let Some(mut stored) = self.ecs.0.get_mut::<BuilderPlans>(unit) {
                *stored = plans;
            }
            let is_player = player == Some(unit);
            for event in outcome.events {
                // `HintsFragment.java:74-102`: placed/break events feed the hint
                // sets before the event is dispatched (player unit only).
                if is_player {
                    match &event {
                        SimEvent::BlockPlacedEvent(placed) => {
                            self.hint_events.placed_by_player.insert(placed.block);
                        }
                        SimEvent::BlockBrokenEvent(_) => {
                            self.hint_events.events.insert("break");
                        }
                        _ => {}
                    }
                }
                self.push_event(event);
            }
            // `Trigger.derelictBreak` (G05 T4b): the player finished
            // deconstructing a team-0 (derelict) building.
            if outcome.derelict_break {
                self.hint_events.events.insert("derelictbreak");
            }
            if outcome.changed {
                self.refresh_unit_runtime();
            }
        }
    }
}

/// `ConstructBlock.constructFinish`/`deconstructFinish` FX (`ConstructBlock
/// .java:59-116`): `block.placeEffect`/`breakEffect` at the tile's draw centre
/// with the block size as the rotation. Fog is not ported, so the visibility
/// gate is skipped (documented).
fn emit_construct_fx(
    world: &World,
    runtime: &crate::sim::runtime::SimRuntime,
    block: BlockId,
    anchor: (i32, i32),
    active_deconstruct: bool,
) {
    use crate::content::EffectId;
    use crate::content::registries::fx_meta::EffectRef;
    let Some(def) = runtime.content().block(block) else {
        return;
    };
    let Some(unit_runtime) = world.get_non_send::<crate::sim::unit_runtime::UnitRuntime>() else {
        return;
    };
    let cx = anchor.0 as f32 * 8.0 + def.offset;
    let cy = anchor.1 as f32 * 8.0 + def.offset;
    let (id, color) = if active_deconstruct {
        (
            EffectId::BREAK_BLOCK,
            def.map_color.unwrap_or(crate::content::Rgba::WHITE),
        )
    } else {
        (EffectId::PLACE_BLOCK, crate::content::Rgba::WHITE)
    };
    unit_runtime
        .fx
        .effect(&EffectRef::Named(id), cx, cy, def.size as f32, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::determinism::SimCommand;

    fn sim_with_player() -> (Sim, Entity, u16, BlockId) {
        let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let core_id = sim.world_block_id("core-shard").expect("core");
        sim.command(SimCommand::Place {
            x: 10,
            y: 10,
            block: core_id,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("core placed");
        let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
        let unit = sim
            .spawn_player_unit("alpha", 1, 80.0, 80.0, 0.0)
            .expect("player unit");
        // The local player is team 1 (`sharded`); sync the placement guard like
        // a campaign load does (`sync_build_rules`), so PLAYER-flag queries use
        // the real player team.
        sim.ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
            .expect("build rules")
            .default_team = 1;
        (sim, unit, core_id, wall)
    }

    fn core_copper(sim: &Sim) -> i32 {
        let runtime = sim.block_runtime().expect("runtime");
        let copper = runtime.content().item_id("copper").expect("copper");
        let core = team_core(&sim.ecs.0, 1).expect("core");
        sim.ecs
            .0
            .get::<ItemModule>(core)
            .map(|m| m.get(copper))
            .unwrap_or(0)
    }

    fn set_core_copper(sim: &mut Sim, amount: i32) {
        let copper = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .item_id("copper")
            .expect("copper");
        let core = team_core(&sim.ecs.0, 1).expect("core");
        let mut module = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
        let have = module.get(copper);
        module.remove(copper, have);
        module.add(copper, amount, 100_000);
    }

    fn set_core_item(sim: &mut Sim, name: &str, amount: i32) {
        let item = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .item_id(name)
            .expect("item");
        let core = team_core(&sim.ecs.0, 1).expect("core");
        let mut module = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
        let have = module.get(item);
        module.remove(item, have);
        module.add(item, amount, 100_000);
    }

    /// Inserts a live `CampaignRuntime` with `default_team = 1` in `Playing`
    /// (`sim::combat_tests::destroyed_building_leaves_rebuild_plan` shape).
    fn with_campaign(sim: &mut Sim) {
        use crate::game::planet::EmptyNeighborhood;
        use crate::game::play::PlaySession;
        use crate::game::rules::Rules;
        use crate::game::runtime::CampaignRuntime;
        use crate::game::universe::Campaign;

        let campaign = {
            let content = &sim.unit_runtime().expect("runtime").content;
            Campaign::from_registry(content, &EmptyNeighborhood)
        };
        let mut session = PlaySession::new(Rules {
            default_team: 1,
            ..Rules::default()
        });
        session.phase = crate::game::State::Playing;
        sim.ecs
            .0
            .insert_resource(CampaignRuntime::new(session, campaign));
    }

    /// Team ghost rebuild plans (queue order).
    fn team_ghosts(sim: &Sim, team: u8) -> Vec<crate::game::teams::BlockPlan> {
        sim.ecs
            .0
            .get_resource::<crate::game::runtime::CampaignRuntime>()
            .and_then(|campaign| campaign.session.teams.get_or_null(crate::ecs::TeamId(team)))
            .map(|data| data.plans.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Builds a team-1 wall at `(x, y)` through the player plan queue (so the
    /// finish branch runs) and returns its entity.
    fn build_wall(sim: &mut Sim, x: i16, y: i16, wall: BlockId) -> Entity {
        assert!(sim.queue_player_place(x, y, wall, 0));
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        assert_eq!(
            sim.grid.block_at(TilePos::new(x, y)),
            Some(wall),
            "wall finished at ({x}, {y})"
        );
        sim.grid.entity_at(TilePos::new(x, y)).expect("wall entity")
    }

    /// Kills a building (`Health.health = 0`); the next tick's `resolve_deaths`
    /// destroys it and pushes its team ghost plan.
    fn kill_building(sim: &mut Sim, entity: Entity) {
        sim.ecs
            .0
            .get_mut::<crate::entities::comp::Health>(entity)
            .expect("health")
            .health = 0.0;
        sim.tick().expect("tick");
    }

    #[test]
    fn player_place_builds_over_time_and_spends_core_items() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 100);
        sim.command(SimCommand::Place {
            x: 14,
            y: 10,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: Some(0),
        })
        .expect("queued");
        assert_eq!(sim.player_build_plans().len(), 1, "plan queued, not placed");
        assert_ne!(sim.grid.block_at(TilePos::new(14, 10)), Some(wall));
        sim.tick().expect("tick");
        let (current, progress, _) = sim
            .construct_progress(14, 10)
            .expect("construct block begun");
        assert_eq!(current, wall);
        assert!(progress > 0.0 && progress < 1.0, "progress {progress}");
        // copper-wall: 6 copper, buildTime = 6 * 0.5 = 3 ticks... alpha 0.5
        // speed -> ~6 ticks.
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        assert_eq!(
            sim.grid.block_at(TilePos::new(14, 10)),
            Some(wall),
            "finished"
        );
        assert!(sim.player_build_plans().is_empty());
        assert_eq!(core_copper(&sim), 94, "requirements drawn from the core");
    }

    #[test]
    fn player_plan_config_applies_on_finish() {
        use crate::world::config::{ConfigValue, read_config};

        let (mut sim, _unit, _core, _wall) = sim_with_player();
        // sorter = 2 lead + 2 copper (build_cost_multiplier 3).
        set_core_item(&mut sim, "copper", 500);
        set_core_item(&mut sim, "lead", 500);
        let sorter = BlockId::new(sim.world_block_id("sorter").expect("sorter"));
        let copper = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .item_id("copper")
            .expect("copper");
        assert!(sim.queue_player_place_config(14, 10, sorter, 0, ConfigValue::Item(copper)));
        for _ in 0..120 {
            sim.tick().expect("tick");
        }
        assert_eq!(
            sim.grid.block_at(TilePos::new(14, 10)),
            Some(sorter),
            "sorter finished"
        );
        let entity = sim
            .grid
            .entity_at(TilePos::new(14, 10))
            .expect("sorter entity");
        assert_eq!(
            read_config(&sim.ecs.0, entity),
            ConfigValue::Item(copper),
            "plan config applied on finish"
        );
    }

    #[test]
    fn plan_without_config_keeps_default() {
        use crate::world::config::{ConfigValue, read_config};

        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 100);
        assert!(sim.queue_player_place(14, 10, wall, 0));
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        assert_eq!(sim.grid.block_at(TilePos::new(14, 10)), Some(wall));
        let entity = sim
            .grid
            .entity_at(TilePos::new(14, 10))
            .expect("wall entity");
        assert_eq!(read_config(&sim.ecs.0, entity), ConfigValue::None);
    }

    /// T2 (PAR-0215): with no player unit, a `player: Some` place in a world
    /// whose team has a core changes nothing (Java `Nulls.unit.addBuild` is a
    /// no-op; `BuilderComp.addBuild` returns at `if(!canBuild()) return;`).
    #[test]
    fn player_place_without_unit_is_dropped_when_team_has_core() {
        let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let core_id = sim.world_block_id("core-shard").expect("core");
        sim.command(SimCommand::Place {
            x: 10,
            y: 10,
            block: core_id,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("core placed");
        let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
        set_core_copper(&mut sim, 100);
        sim.command(SimCommand::Place {
            x: 20,
            y: 20,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: Some(0),
        })
        .expect("dropped");
        for _ in 0..120 {
            sim.tick().expect("tick");
        }
        assert_eq!(sim.grid.entity_at(TilePos::new(20, 20)), None);
        assert!(sim.player_build_plans().is_empty(), "no plan queued");
        assert_eq!(core_copper(&sim), 100, "no spend, no refund");
    }

    /// T2 (PAR-0215): the same for removal — `Break { player: Some(_) }` with
    /// no build-capable unit is dropped when the player team owns a core.
    #[test]
    fn player_break_without_unit_is_dropped_when_team_has_core() {
        let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let core_id = sim.world_block_id("core-shard").expect("core");
        let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
        sim.command(SimCommand::Place {
            x: 10,
            y: 10,
            block: core_id,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("core placed");
        sim.command(SimCommand::Place {
            x: 20,
            y: 20,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("wall placed");
        assert_eq!(sim.grid.block_at(TilePos::new(20, 20)), Some(wall));
        sim.command(SimCommand::Break {
            x: 20,
            y: 20,
            player: Some(0),
        })
        .expect("dropped");
        for _ in 0..120 {
            sim.tick().expect("tick");
        }
        assert_eq!(
            sim.grid.block_at(TilePos::new(20, 20)),
            Some(wall),
            "wall survives the dead-player break"
        );
    }

    /// Core-less dev worlds (spine/determinism scenarios) keep the port-only
    /// instant placement affordance (no Java counterpart).
    #[test]
    fn player_place_without_core_stays_instant() {
        let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let wall = BlockId::new(sim.world_block_id("copper-wall").expect("wall"));
        sim.command(SimCommand::Place {
            x: 20,
            y: 20,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: Some(0),
        })
        .expect("instant");
        sim.tick().expect("tick");
        assert_eq!(sim.grid.block_at(TilePos::new(20, 20)), Some(wall));
    }

    #[test]
    fn rebuild_select_queues_ghost() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        with_campaign(&mut sim);
        set_core_copper(&mut sim, 100);
        let entity = build_wall(&mut sim, 14, 10, wall);
        kill_building(&mut sim, entity);
        assert!(
            team_ghosts(&sim, 1)
                .iter()
                .any(|plan| plan.x == 14 && plan.y == 10),
            "ghost queued"
        );

        assert_eq!(sim.queue_team_rebuild(13, 9, 15, 11), 1);
        let plans = sim.player_build_plans();
        assert_eq!(plans.len(), 1);
        assert_eq!((plans[0].x, plans[0].y, plans[0].block), (14, 10, wall));

        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        assert_eq!(
            sim.grid.block_at(TilePos::new(14, 10)),
            Some(wall),
            "rebuilt"
        );
        assert!(
            !team_ghosts(&sim, 1)
                .iter()
                .any(|plan| plan.x == 14 && plan.y == 10),
            "ghost cleared by the rebuild (checkOverlappingPlans)"
        );
    }

    #[test]
    fn rebuild_select_single_tile_rect() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        with_campaign(&mut sim);
        set_core_copper(&mut sim, 100);
        let entity = build_wall(&mut sim, 14, 10, wall);
        kill_building(&mut sim, entity);
        // Zero-width rect still overlaps the block bounds (strict inequalities).
        assert_eq!(sim.queue_team_rebuild(14, 10, 14, 10), 1);
    }

    #[test]
    fn remove_plans_in_cancels_queued_and_ghosts() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        with_campaign(&mut sim);
        // Empty core: the queued plan at (20,20) stays queued.
        assert!(sim.queue_player_place(20, 20, wall, 0));
        assert_eq!(sim.player_build_plans().len(), 1);
        // Ghost at (22,20) from a killed API-placed wall.
        sim.command(SimCommand::Place {
            x: 22,
            y: 20,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("api wall");
        let entity = sim.grid.entity_at(TilePos::new(22, 20)).expect("wall");
        kill_building(&mut sim, entity);
        assert!(
            team_ghosts(&sim, 1)
                .iter()
                .any(|plan| plan.x == 22 && plan.y == 20)
        );

        sim.remove_plans_in(19, 19, 23, 21, 100);
        assert!(sim.player_build_plans().is_empty(), "queued plan removed");
        assert!(
            team_ghosts(&sim, 1).is_empty(),
            "ghost removed from the team queue"
        );
    }

    #[test]
    fn adjacent_block_does_not_clear_ghost() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        with_campaign(&mut sim);
        set_core_copper(&mut sim, 100);
        let entity = build_wall(&mut sim, 14, 10, wall);
        kill_building(&mut sim, entity);
        // A neighbour finishes right next to the ghost: strict bounds do not
        // overlap, so the ghost stays.
        let _ = build_wall(&mut sim, 15, 10, wall);
        assert!(
            team_ghosts(&sim, 1)
                .iter()
                .any(|plan| plan.x == 14 && plan.y == 10),
            "adjacent block leaves the ghost"
        );
    }

    #[test]
    fn out_of_range_plans_wait_for_the_builder() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 100);
        // 220 px build range = 27.5 tiles; the unit is at tile (10, 10).
        sim.command(SimCommand::Place {
            x: 50,
            y: 50,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: Some(0),
        })
        .expect("queued");
        for _ in 0..30 {
            sim.tick().expect("tick");
        }
        assert!(sim.construct_progress(50, 50).is_none());
        assert_eq!(sim.player_build_plans().len(), 1);
    }

    /// PAR-0098 §2a: `DeletePlans` drops the matching player queue entries
    /// (`InputHandler.deletePlans`) and `clear_building` clears the queue.
    #[test]
    fn delete_plans_removes_player_plan() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 100);
        for (x, y) in [(14, 10), (16, 10)] {
            sim.command(SimCommand::Place {
                x,
                y,
                block: wall.raw(),
                rotation: 0,
                team: 1,
                player: Some(0),
            })
            .expect("queued");
        }
        assert_eq!(sim.player_build_plans().len(), 2);
        sim.command(SimCommand::DeletePlans {
            positions: smallvec::smallvec![TilePos::new(14, 10).pack()],
        })
        .expect("delete plans");
        assert_eq!(sim.player_build_plans().len(), 1, "one plan removed");
        assert_eq!(sim.player_build_plans()[0].x, 16);
        sim.clear_player_build_plans();
        assert!(sim.player_build_plans().is_empty(), "queue cleared");
    }

    /// PAR-0098 §2b: `updateBuilding = false` freezes the queue
    /// (`BuilderComp.updateBuildLogic` returns early).
    #[test]
    fn pausing_building_holds_construct_progress() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 100);
        // Pause before the first plan: the queue is created paused and the head
        // never starts a construct block.
        sim.set_player_building_paused(true);
        sim.command(SimCommand::Place {
            x: 14,
            y: 10,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: Some(0),
        })
        .expect("queued");
        for _ in 0..30 {
            sim.tick().expect("tick");
        }
        assert!(
            sim.construct_progress(14, 10).is_none(),
            "no construction while paused"
        );
        assert_eq!(sim.player_build_plans().len(), 1, "plan waits");
        // Resume: the plan completes.
        sim.set_player_building_paused(false);
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        assert_eq!(
            sim.grid.block_at(TilePos::new(14, 10)),
            Some(wall),
            "resumed and finished"
        );
    }

    /// PAR-0098 §2c: a ground unit standing on the plan's footprint rotates the
    /// plan to the back of the queue (`Build.checkNoUnitOverlap`); once it
    /// moves away the plan proceeds.
    #[test]
    fn unit_overlap_delays_construction() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 100);
        // A ground (`flying == false`) unit standing on the target tile.
        let runtime = sim.block_runtime().expect("runtime");
        let dagger = runtime.content().unit_id("dagger").expect("dagger");
        let def = runtime.content().unit(dagger).cloned().expect("dagger def");
        let blocker = crate::entities::comp::unit::lifecycle::spawn_unit_def(
            &mut sim.ecs.0,
            42,
            &def,
            1,
            14.0 * 8.0,
            10.0 * 8.0,
            90.0,
        );
        // Keep it in place (`CommandAI` with no target idles).
        if let Some(mut slot) = sim
            .ecs
            .0
            .get_mut::<crate::ai::controller::ControllerSlot>(blocker)
        {
            slot.kind = crate::ai::controller::AiKind::Command;
            slot.target = None;
        }
        sim.command(SimCommand::Place {
            x: 14,
            y: 10,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: Some(0),
        })
        .expect("queued");
        for _ in 0..30 {
            sim.tick().expect("tick");
        }
        assert!(
            sim.construct_progress(14, 10).is_none(),
            "unit blocks the plan"
        );
        assert_eq!(sim.player_build_plans().len(), 1, "plan waits");

        // Move the unit away: the plan starts and completes.
        if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(blocker) {
            pos.x = 20.0 * 8.0;
            pos.y = 20.0 * 8.0;
        }
        for _ in 0..120 {
            sim.tick().expect("tick");
        }
        assert_eq!(
            sim.grid.block_at(TilePos::new(14, 10)),
            Some(wall),
            "built after the unit moved"
        );
    }

    /// T6 (`Units.java:29-35`): a legged unit (`atrax`, `allowLegStep`) on the
    /// plan's footprint does not block construction.
    #[test]
    fn legged_unit_does_not_block_construction() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 100);
        let runtime = sim.block_runtime().expect("runtime");
        let atrax = runtime.content().unit_id("atrax").expect("atrax");
        let def = runtime.content().unit(atrax).cloned().expect("atrax def");
        assert!(def.allow_leg_step, "atrax is a legged unit");
        let blocker = crate::entities::comp::unit::lifecycle::spawn_unit_def(
            &mut sim.ecs.0,
            43,
            &def,
            1,
            14.0 * 8.0,
            10.0 * 8.0,
            90.0,
        );
        if let Some(mut slot) = sim
            .ecs
            .0
            .get_mut::<crate::ai::controller::ControllerSlot>(blocker)
        {
            slot.kind = crate::ai::controller::AiKind::Command;
            slot.target = None;
        }
        sim.command(SimCommand::Place {
            x: 14,
            y: 10,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: Some(0),
        })
        .expect("queued");
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        assert_eq!(
            sim.grid.block_at(TilePos::new(14, 10)),
            Some(wall),
            "the legged unit does not block the wall"
        );
    }

    #[test]
    fn no_items_means_no_construction() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 0);
        sim.command(SimCommand::Place {
            x: 14,
            y: 10,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: Some(0),
        })
        .expect("queued");
        for _ in 0..30 {
            sim.tick().expect("tick");
        }
        assert_ne!(sim.grid.block_at(TilePos::new(14, 10)), Some(wall));
        assert_eq!(sim.player_build_plans().len(), 1, "plan stays queued");
    }

    /// T11 (`ItemBridge.playerPlaced`, `ItemBridge.java:224-233`): two
    /// player-built bridge-conveyors 3 tiles apart chain on construction.
    #[test]
    fn bridges_chain_when_player_placed() {
        let (mut sim, _unit, _core, _wall) = sim_with_player();
        set_core_copper(&mut sim, 10_000);
        // bridge-conveyor needs lead 6 + copper 6 (`registries/blocks/distribution.rs`).
        let lead = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .item_id("lead")
            .expect("lead");
        let core = team_core(&sim.ecs.0, 1).expect("core");
        if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(core) {
            items.add(lead, 1_000, 100_000);
        }
        let bridge = BlockId::new(
            sim.world_block_id("bridge-conveyor")
                .expect("bridge-conveyor"),
        );
        assert!(
            sim.queue_player_place(20, 20, bridge, 0),
            "first bridge queued"
        );
        assert!(
            sim.queue_player_place(23, 20, bridge, 0),
            "second bridge queued"
        );
        for _ in 0..600 {
            sim.tick().expect("tick");
            let finished = sim
                .grid
                .entity_at(TilePos::new(23, 20))
                .is_some_and(|entity| sim.ecs.0.get::<ItemBridgeBuild>(entity).is_some());
            if finished {
                break;
            }
        }
        let first = sim
            .grid
            .entity_at(TilePos::new(20, 20))
            .expect("first bridge built");
        let second = sim
            .grid
            .entity_at(TilePos::new(23, 20))
            .expect("second bridge built");
        let link = sim
            .ecs
            .0
            .get::<ItemBridgeBuild>(first)
            .map(|bridge| bridge.link)
            .unwrap_or(-1);
        assert_eq!(
            link,
            TilePos::new(23, 20).pack(),
            "the first bridge links to the second"
        );
        let second_link = sim
            .ecs
            .0
            .get::<ItemBridgeBuild>(second)
            .map(|bridge| bridge.link)
            .unwrap_or(-1);
        assert_eq!(second_link, -1, "the second waits for a later bridge");
    }

    /// `HintsFragment.java:74-102`: player placements and breaks feed the hint
    /// sets (`placedBlocks` / `"break"`).
    #[test]
    fn hint_events_record_player_placements() {
        // Player-queued placement (`BlockBuildEndEvent` with `!breaking`).
        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 100);
        assert!(sim.queue_player_place(14, 10, wall, 0), "plan queued");
        for _ in 0..120 {
            sim.tick().expect("tick");
            if sim.hint_events.placed_by_player.contains(&wall) {
                break;
            }
        }
        assert!(
            sim.hint_events.placed_by_player.contains(&wall),
            "the player wall placement is recorded"
        );

        // Player-queued deconstruction of an instant-placed wall fires "break".
        let (mut sim, _unit, _core, wall) = sim_with_player();
        sim.command(SimCommand::Place {
            x: 14,
            y: 10,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("instant API place");
        // The local player is team 1 (`sharded`); sync the guard like a
        // campaign load does (`sync_build_rules`).
        sim.ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
            .expect("build rules")
            .default_team = 1;
        sim.command(SimCommand::Break {
            x: 14,
            y: 10,
            player: Some(0),
        })
        .expect("queued break");
        for _ in 0..120 {
            sim.tick().expect("tick");
            if sim.hint_events.events.contains("break") {
                break;
            }
        }
        assert!(
            sim.hint_events.events.contains("break"),
            "the break event is recorded"
        );
    }

    /// `Trigger.fireCreate` (G05 T4b): a fire born during a tick arms the
    /// `waveFire` hint event.
    #[test]
    fn fire_creation_records_hint_event() {
        let (mut sim, _unit, _core, _wall) = sim_with_player();
        let mut rng = crate::determinism::SimRng::new(3);
        assert!(crate::combat::fires::create(&mut sim.ecs.0, 5, 5, &mut rng).is_some());
        sim.tick().expect("tick");
        assert!(sim.hint_events.events.contains("fire"));
    }

    /// T18: a queued player placement keeps its rotation through the plan queue
    /// and the finished block.
    #[test]
    fn queue_player_place_keeps_rotation() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        set_core_copper(&mut sim, 100);
        assert!(sim.queue_player_place(14, 10, wall, 1), "plan queued");
        let plans = sim.player_build_plans();
        assert_eq!(plans.first().map(|plan| plan.rotation), Some(1));
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        let entity = sim
            .grid
            .entity_at(TilePos::new(14, 10))
            .expect("wall built");
        assert_eq!(
            sim.ecs
                .0
                .get::<Building>(entity)
                .map(|building| building.rotation),
            Some(1),
            "built block keeps the plan rotation"
        );
    }

    #[test]
    fn player_break_deconstructs_and_refunds() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        sim.command(SimCommand::Place {
            x: 14,
            y: 10,
            block: wall.raw(),
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("instant API place");
        assert_eq!(sim.grid.block_at(TilePos::new(14, 10)), Some(wall));
        set_core_copper(&mut sim, 10);
        // The local player is team 1 (`sharded`); sync the guard like a
        // campaign load does (`sync_build_rules`).
        sim.ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
            .expect("build rules")
            .default_team = 1;
        sim.command(SimCommand::Break {
            x: 14,
            y: 10,
            player: Some(0),
        })
        .expect("queued break");
        assert_eq!(
            sim.grid.block_at(TilePos::new(14, 10)),
            Some(wall),
            "not instant"
        );
        for _ in 0..60 {
            sim.tick().expect("tick");
        }
        assert_eq!(sim.grid.block_at(TilePos::new(14, 10)), Some(BlockId::AIR));
        // 6 copper * 0.5 refund.
        assert_eq!(core_copper(&sim), 13);
    }

    /// G03 T22: finishing a construct emits `PLACE_BLOCK`, deconstructing
    /// `BREAK_BLOCK` (rotation argument = the block size).
    #[test]
    fn construct_finish_emits_place_fx() {
        use std::sync::{Arc, Mutex};

        #[derive(Default)]
        struct RecordingSink(Mutex<Vec<(crate::content::EffectId, f32, f32, f32)>>);

        impl crate::combat::view::FxSink for RecordingSink {
            fn effect(
                &self,
                effect: &crate::content::registries::fx_meta::EffectRef,
                x: f32,
                y: f32,
                rotation: f32,
                _color: crate::content::Rgba,
            ) {
                if let crate::content::registries::fx_meta::EffectRef::Named(id) = effect {
                    self.0.lock().expect("lock").push((*id, x, y, rotation));
                }
            }
        }

        let (mut sim, _unit, _core, wall) = sim_with_player();
        let sink = Arc::new(RecordingSink::default());
        if let Some(mut runtime) = sim.unit_runtime_mut() {
            runtime.fx = sink.clone();
        }
        set_core_copper(&mut sim, 100);
        // The local player is team 1 (`sharded`); sync the guard like a
        // campaign load does (`sync_build_rules`).
        sim.ecs
            .0
            .get_resource_mut::<crate::world::limits::BuildRules>()
            .expect("build rules")
            .default_team = 1;

        assert!(sim.queue_player_place(20, 20, wall, 1), "wall queued");
        for _ in 0..120 {
            sim.tick().expect("tick");
        }
        {
            let events = sink.0.lock().expect("lock");
            let place = events
                .iter()
                .find(|(id, ..)| *id == crate::content::EffectId::PLACE_BLOCK)
                .unwrap_or_else(|| panic!("PLACE_BLOCK missing: {events:?}"));
            assert!((place.1 - 160.0).abs() < 1e-3, "cx: {place:?}");
            assert!((place.2 - 160.0).abs() < 1e-3, "cy: {place:?}");
            assert!((place.3 - 1.0).abs() < 1e-3, "rotation = size: {place:?}");
        }
        assert!(
            sim.command(SimCommand::Break {
                x: 20,
                y: 20,
                player: Some(0),
            }) == Ok(())
        );
        for _ in 0..120 {
            sim.tick().expect("tick");
        }
        let events = sink.0.lock().expect("lock");
        let count = events
            .iter()
            .filter(|(id, ..)| *id == crate::content::EffectId::BREAK_BLOCK)
            .count();
        assert!(count >= 1, "BREAK_BLOCK missing: {events:?}");
    }

    /// G03 T2: the preview validity check agrees with the queued placement.
    #[test]
    fn preview_can_place_matches_queue() {
        let (mut sim, _unit, _core, wall) = sim_with_player();
        sim.tick().expect("tick");
        assert!(
            sim.preview_can_place(20, 20, wall, 0),
            "empty tile is placeable in preview"
        );
        assert!(
            !sim.preview_can_place(10, 10, wall, 0),
            "the core footprint is not placeable"
        );
        assert!(
            sim.queue_player_place(20, 20, wall, 0),
            "the queued placement agrees"
        );
        assert!(
            !sim.queue_player_place(10, 10, wall, 0),
            "the queued placement rejects the core tile"
        );
    }

    /// H02 T4: `CoreBlock.placeBegan` (`CoreBlock.java:205-239`): upgrading a
    /// shard to a foundation over the same tile is instant and keeps the old
    /// items minus the requirements.
    #[test]
    fn core_upgrade_keeps_items() {
        let (mut sim, _unit, _core, _wall) = sim_with_player();
        let foundation = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .block_id("core-foundation")
            .expect("core-foundation");
        // Shard capacity 4000 per item (`Blocks.java:3152`); foundation costs
        // copper 3000 / lead 3000 / silicon 2000 (`Blocks.java:3160`).
        let (copper, lead, silicon) = {
            let content = &sim.block_runtime().expect("runtime").content();
            (
                content.item_id("copper").expect("copper"),
                content.item_id("lead").expect("lead"),
                content.item_id("silicon").expect("silicon"),
            )
        };
        {
            let core = team_core(&sim.ecs.0, 1).expect("core");
            let mut items = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
            for item in [copper, lead, silicon] {
                items.add(item, 4000, 4000);
            }
        }
        assert!(
            sim.queue_player_place(10, 10, foundation, 0),
            "upgrade plan queued"
        );
        for _ in 0..600 {
            sim.tick().expect("tick");
            if sim.player_build_plans().is_empty() {
                break;
            }
        }
        assert!(
            sim.player_build_plans().is_empty(),
            "the upgrade plan finished"
        );
        assert_eq!(
            sim.grid.block_at(TilePos::new(10, 10)),
            Some(foundation),
            "core upgraded instantly"
        );
        let core = team_core(&sim.ecs.0, 1).expect("core");
        let items = sim.ecs.0.get::<ItemModule>(core).expect("items");
        assert_eq!(items.get(copper), 1000, "copper kept minus cost");
        assert_eq!(items.get(lead), 1000);
        assert_eq!(items.get(silicon), 2000);
    }

    /// H02 T4: a core can only be placed over a smaller own core
    /// (`CoreBlock.canPlaceOn`, `CoreBlock.java:185-203`).
    #[test]
    fn core_needs_existing_core() {
        let (sim, _unit, _core, _wall) = sim_with_player();
        let foundation = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .block_id("core-foundation")
            .expect("core-foundation");
        assert!(
            !sim.valid_place_player(30, 30, foundation, 0),
            "foundation on free ground rejected"
        );
        // Over the shard without the requirements: rejected (`core == null` or
        // `!items.has(requirements)`).
        assert!(
            !sim.valid_place_player(10, 10, foundation, 0),
            "upgrade without items rejected"
        );
    }

    /// H02 T4: the upgrade plan needs every requirement in the team core
    /// (`CoreBlock.canPlaceOn`).
    #[test]
    fn core_upgrade_needs_items() {
        let (mut sim, _unit, _core, _wall) = sim_with_player();
        let foundation = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .block_id("core-foundation")
            .expect("core-foundation");
        let copper = sim
            .block_runtime()
            .expect("runtime")
            .content()
            .item_id("copper")
            .expect("copper");
        {
            let core = team_core(&sim.ecs.0, 1).expect("core");
            let mut items = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
            items.add(copper, 100, 4000);
        }
        assert!(
            !sim.valid_place_player(10, 10, foundation, 0),
            "copper 100 < 3000"
        );
        let (lead, silicon) = {
            let content = &sim.block_runtime().expect("runtime").content();
            (
                content.item_id("lead").expect("lead"),
                content.item_id("silicon").expect("silicon"),
            )
        };
        {
            let core = team_core(&sim.ecs.0, 1).expect("core");
            let mut items = sim.ecs.0.get_mut::<ItemModule>(core).expect("items");
            items.add(copper, 3000, 4000);
            items.add(lead, 3000, 4000);
            items.add(silicon, 2000, 4000);
        }
        assert!(
            sim.valid_place_player(10, 10, foundation, 0),
            "all requirements present"
        );
    }

    /// H08 T2: the placement-preview validity is the exact
    /// `queue_player_place_config` check (`Build.validPlace` under
    /// `PlaceFlags::PLAYER`; `MindSimHost::can_place_player` forwards here).
    /// The seam is H02 T3's `Sim::valid_place_player`.
    #[test]
    fn can_place_player_matches_queue() {
        let (sim, _unit, _core, wall) = sim_with_player();
        assert!(
            sim.valid_place_player(14, 10, wall, 0),
            "free ground accepts a copper wall"
        );
        // The core occupies (10,10); the same gate `queue_player_place_config`
        // runs rejects the wall there.
        assert!(
            !sim.valid_place_player(10, 10, wall, 0),
            "core tile rejects a copper wall"
        );
    }
}
