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
                            rules.economy.cost_multiplier,
                        ) {
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
        world.entity_mut(entity).insert(state.clone());
        if let Some(head) = plans.plans.front_mut() {
            *head = current.clone();
        }

        if finished {
            if current.breaking {
                // `ConstructBlock.deconstructFinish` -> `tile.remove()`.
                if runtime
                    .remove_block_raw(world, grid, anchor.x(), anchor.y())
                    .is_some()
                {
                    out.events
                        .push(SimEvent::BlockBrokenEvent(BlockBrokenEvent {
                            x: anchor.x(),
                            y: anchor.y(),
                            block: state.current,
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
                out.events
                    .push(SimEvent::BlockPlacedEvent(BlockPlacedEvent {
                        x: anchor.x(),
                        y: anchor.y(),
                        block: state.current,
                        team,
                    }));
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

    /// Queues a placement on the player unit (`InputHandler.flushPlans` ->
    /// `player.unit().addBuild(plan)`); invalid placements are dropped.
    pub fn queue_player_place(&mut self, x: i16, y: i16, block: BlockId, rotation: u8) -> bool {
        let Some(unit) = self.player_unit else {
            return false;
        };
        let team = self.live_default_team();
        let Some(runtime) = self.block_runtime.as_ref() else {
            return false;
        };
        if !runtime.can_place(&self.ecs.0, &self.grid, x, y, block, rotation, team) {
            return false;
        }
        let mut plan = UnitBuildPlan::place(x, y, rotation, block);
        // `addBuild`: carry the progress of an in-flight construct block.
        if let Some(entity) = self.grid.entity_at(TilePos::new(x, y))
            && let Some(state) = self.ecs.0.get::<ConstructState>(entity)
        {
            plan.progress = state.progress;
        }
        self.with_player_plans(unit, |plans| plans.add_build(plan, true));
        true
    }

    /// `InputHandler.validPlace(x, y, block, rot)` for the local team: the
    /// placement-preview validity check, no side effects and nothing queued
    /// (`InputHandler.java:2029-2032` calls `Build.validPlace`).
    pub fn preview_can_place(&self, x: i16, y: i16, block: BlockId, rotation: u8) -> bool {
        let team = self.live_default_team();
        self.block_runtime.as_ref().is_some_and(|runtime| {
            runtime.can_place(&self.ecs.0, &self.grid, x, y, block, rotation, team)
        })
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
        let Some(unit) = self.player_unit else {
            return;
        };
        if self.ecs.0.get_entity(unit).is_err() {
            return;
        }
        let Some(mut plans) = self
            .ecs
            .0
            .get_mut::<BuilderPlans>(unit)
            .map(|mut p| std::mem::take(&mut *p))
        else {
            return;
        };
        if plans.plans.is_empty() {
            self.with_player_plans(unit, |slot| *slot = plans);
            return;
        }
        let Some(mut runtime) = self.block_runtime.take() else {
            self.with_player_plans(unit, |slot| *slot = plans);
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
        self.with_player_plans(unit, |slot| *slot = plans);
        for event in outcome.events {
            self.push_event(event);
        }
        if outcome.changed {
            self.refresh_unit_runtime();
        }
    }
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
}
