// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Build-rule reads and plan validation/predicates shared by the builder passes.
//!
//! Upstream: core/src/mindustry/world/Build.java (validPlace, checkNoUnitOverlap);
//! core/src/mindustry/entities/comp/BuilderComp.java (validatePlans, shouldSkip).

use super::*;

/// The rule reads `updateBuildLogic` needs, resolved once per tick.
#[derive(Debug, Clone, Copy)]
pub(super) struct BuildRuleView {
    /// `state.rules.infiniteResources`.
    pub(super) state_infinite: bool,
    /// `rules.instantBuild`.
    pub(super) instant_build: bool,
    /// `state.rules.buildSpeed(team)`.
    pub(super) build_speed: f32,
    pub(super) economy: BuildEconomy,
}

impl BuildRuleView {
    pub(super) fn read(world: &World, team: u8) -> Self {
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
pub(super) fn team_core(world: &World, team: u8) -> Option<Entity> {
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

pub(super) fn building_team(world: &World, entity: Entity) -> u8 {
    world
        .get::<TeamComp>(entity)
        .map(|t| t.team)
        .unwrap_or(DERELICT)
}

/// `BuilderComp.validatePlans`: drops plans that are already satisfied.
pub(super) fn validate_plans(
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
pub(super) fn should_skip(
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
pub(super) fn tile_world(pos: TilePos) -> (f32, f32) {
    let unit = crate::config::TILESIZE as f32;
    (f32::from(pos.x()) * unit, f32::from(pos.y()) * unit)
}

pub(super) fn within(ux: f32, uy: f32, pos: TilePos, range: f32) -> bool {
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
    if !def.solid && !def.solidifies {
        // `(!type.solid && !type.solidifes)` short-circuits the query
        // (`Build.java:178-180`).
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
