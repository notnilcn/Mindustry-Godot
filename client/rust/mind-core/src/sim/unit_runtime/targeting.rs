// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Target queries: core/flag/unit selection, target validity, targeting
//! geometry and facing.
//! Upstream: core/src/mindustry/entities/units/AIController.java
//! core/src/mindustry/entities/units/Units.java

use super::ai::ground::GroundAiRules;
use super::*;

/// `unit.lookAt(x, y)` (`Angles.angle(x, y, unit.x, unit.y)`).
pub(super) fn look_at_world(world: &mut World, entity: Entity, x: f32, y: f32) {
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let angle = (y - pos.y).atan2(x - pos.x).to_degrees();
    if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
        core.rotation = angle;
    }
}

/// `FlyingAI.findMainTarget` (FlyingAI.java:40-72): the closest enemy core when
/// it is within `range`; otherwise the first `targetFlags` entry that yields a
/// target (`null` = closest in-range target, a flag = closest active flagged
/// building, any distance); finally the core (possibly out of range).
#[allow(clippy::too_many_arguments)]
pub(super) fn flying_main_target(
    world: &World,
    queries: &TargetQueries,
    team: u8,
    x: f32,
    y: f32,
    range: f32,
    target_air: bool,
    target_ground: bool,
    target_under_blocks: bool,
    target_flags: &[Option<crate::content::BlockFlag>],
) -> Option<Entity> {
    let core = closest_enemy_core(world, team, x, y);
    if let Some(core) = core {
        let in_range = world.get::<Pos>(core).is_some_and(|pos| {
            let dx = pos.x - x;
            let dy = pos.y - y;
            dx * dx + dy * dy <= range * range
        });
        if in_range {
            return Some(core);
        }
    }
    for flag in target_flags {
        match flag {
            None => {
                if let Some(result) = queries.closest_target(
                    world,
                    team,
                    x,
                    y,
                    range,
                    target_air,
                    target_ground,
                    true,
                    target_under_blocks,
                ) {
                    return Some(result);
                }
            }
            Some(flag) if target_ground => {
                if let Some(result) = target_flag_active(world, queries, team, x, y, *flag) {
                    return Some(result);
                }
            }
            Some(_) => {}
        }
    }
    core
}

/// `Block.targetable` (`Block.java:308`) for a live building.
///
/// Every vanilla block is targetable except the four world blocks that set
/// `targetable = false` (`Blocks.java:7002,7014,7023,7030`:
/// `world-processor`, `world-cell`, `world-message`, `world-switch`). The
/// `BlockDef` field itself belongs to `mc/content/**`; the same four blocks are
/// exactly the `BuildVisibility.worldProcessorOnly` set, so keying off the
/// visibility keeps the rule without a content ABI change.
pub(super) fn block_targetable(world: &World, entity: Entity) -> bool {
    let Some(block) = world
        .get::<crate::entities::comp::Building>(entity)
        .map(|building| building.block)
    else {
        return true;
    };
    world
        .get_resource::<crate::world::block::BlockTable>()
        .and_then(|table| table.get(block))
        .is_none_or(|inst| {
            inst.def.build_visibility
                != crate::content::registries::blocks::BuildVisibility::WorldProcessorOnly
        })
}

/// `AIController.targetFlagActive(x, y, flag, enemy=true)`: the closest enemy
/// building carrying `flag` that is active. Java's predicate is
/// `((items != null && items.any()) || status() != BlockStatus.noInput) && block.targetable`
/// (AIController.java:277-284); `status()` is
/// [`crate::world::status::java_block_status`] (`BuildingComp.java:638-652`).
pub(super) fn target_flag_active(
    world: &World,
    queries: &TargetQueries,
    team: u8,
    x: f32,
    y: f32,
    flag: crate::content::BlockFlag,
) -> Option<Entity> {
    // `state.tick` for the `status()` pulse (`LogicWorldState` is refreshed
    // every tick by `Sim::update_logic_globals`).
    let tick = world
        .get_resource::<crate::logic::world::LogicWorldState>()
        .map_or(0, |state| state.tick);
    let mut best: Option<(f32, Entity)> = None;
    for entity in queries.indexer.enemy_flagged(world, team, flag) {
        if !block_targetable(world, entity) {
            continue;
        }
        let active = world
            .get::<crate::world::modules::ItemModule>(entity)
            .is_some_and(|items| items.any())
            || crate::world::status::java_block_status(world, entity, tick)
                != crate::world::status::BlockStatus::NoInput;
        if !active {
            continue;
        }
        let Some(pos) = world.get::<Pos>(entity).copied() else {
            continue;
        };
        let dx = pos.x - x;
        let dy = pos.y - y;
        let dist2 = dx * dx + dy * dy;
        let id = entity;
        let replace = match best {
            None => true,
            Some((best_dist, best_entity)) => {
                dist2 < best_dist || (dist2 == best_dist && id.index() < best_entity.index())
            }
        };
        if replace {
            best = Some((dist2, id));
        }
    }
    best.map(|(_, entity)| entity)
}

/// `unit.within(target, range)` on world positions.
pub(super) fn entity_pos_within(world: &World, entity: Entity, target: Entity, range: f32) -> bool {
    let (Some(a), Some(b)) = (
        world.get::<Pos>(entity).copied(),
        world.get::<Pos>(target).copied(),
    ) else {
        return false;
    };
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    dx * dx + dy * dy <= range * range
}

/// `UnitType.initPathType` (UnitType.java:891-898): the flowfield cost derived
/// from the movement kind when the spec leaves `flowfieldPathType` at -1.
pub(super) fn unit_flowfield_cost(def: &crate::content::registries::units::UnitTypeDef) -> Cost {
    if def.flowfield_path_type >= 0 {
        return Cost::from_id(def.flowfield_path_type as u8).unwrap_or(Cost::Ground);
    }
    if def.naval {
        Cost::Naval
    } else if def.allow_leg_step {
        Cost::Legs
    } else if def.flying {
        Cost::None
    } else if def.hovering {
        Cost::Hover
    } else {
        Cost::Ground
    }
}

/// `unit.type.pathCostId` (`UnitType.initPathType`, UnitType.java:901-910):
/// the `ControlPathfinder` cost of `entity`'s unit type (ground when the type
/// is unresolved).
pub(super) fn unit_control_cost(
    world: &World,
    content: &ContentRegistry,
    entity: Entity,
) -> crate::ai::control_pathfinder::ControlCost {
    world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
        .map_or(crate::ai::control_pathfinder::ControlCost::Ground, |def| {
            crate::ai::control_pathfinder::ControlCost::for_unit(
                def.naval,
                def.allow_leg_step,
                def.hovering,
            )
        })
}

/// `Pathfinder.EnemyCoreField.getPositions` (Pathfinder.java:555-588): enemy
/// core tiles, plus the spawn tiles when `rules.waves && team == defaultTeam`.
///
/// Sorted by `(y, x)` so the `(team, cost)` field cache is stable across the
/// units of a team within a tick (plan G01 §6.9).
pub(super) fn enemy_core_targets(
    world: &World,
    runtime: &UnitRuntime,
    team: u8,
    rules: Option<GroundAiRules>,
) -> Vec<TilePos> {
    let mut out: Vec<TilePos> = Vec::new();
    for entity in world.iter_entities() {
        let Some(comp) = entity.get::<BuildingComp>() else {
            continue;
        };
        if comp.team.0 == team || crate::ai::block_indexer::is_derelict(world, comp.team.0) {
            continue;
        }
        if entity.get::<CoreBuild>().is_none() {
            continue;
        }
        // `Building.tile` is the footprint anchor; the port stores the cursor
        // tile (`assign_footprint`'s `-(size-1)/2` offset, world/ops.rs:233).
        let size = runtime
            .content
            .block(comp.block)
            .map(|def| def.size)
            .unwrap_or(1)
            .max(1);
        let offset = (-(size - 1) / 2) as i16;
        out.push(TilePos::new(comp.pos.x() + offset, comp.pos.y() + offset));
    }
    out.sort_by_key(|tile| (tile.y(), tile.x()));
    out.dedup();
    if rules.is_some_and(|rules| rules.waves && rules.default_team == team) {
        out.extend(runtime.wave_spawner.get_spawns().iter().copied());
        out.dedup();
    }
    out
}

/// `AIController.targetFlag(x, y, BlockFlag.core, enemy=true)` /
/// `closestEnemyCore`: the closest enemy core entity; ties break by entity
/// index.
pub(crate) fn closest_enemy_core(world: &World, team: u8, x: f32, y: f32) -> Option<Entity> {
    let mut best: Option<(f32, Entity)> = None;
    for entity in world.iter_entities() {
        let Some(comp) = entity.get::<BuildingComp>() else {
            continue;
        };
        if comp.team.0 == team || crate::ai::block_indexer::is_derelict(world, comp.team.0) {
            continue;
        }
        if entity.get::<CoreBuild>().is_none() {
            continue;
        }
        let Some(pos) = entity.get::<Pos>() else {
            continue;
        };
        let dx = pos.x - x;
        let dy = pos.y - y;
        let dist2 = dx * dx + dy * dy;
        let id = entity.id();
        let replace = match best {
            None => true,
            Some((best_dist, best_entity)) => {
                dist2 < best_dist || (dist2 == best_dist && id.index() < best_entity.index())
            }
        };
        if replace {
            best = Some((dist2, id));
        }
    }
    best.map(|(_, entity)| entity)
}

/// `unit.closestCore()` (`TeamData.closestCore`): the own-team core closest to
/// `(x, y)`; ties break by entity index.
pub(crate) fn closest_core_of_team(world: &World, team: u8, x: f32, y: f32) -> Option<Entity> {
    let mut best: Option<(f32, Entity)> = None;
    for entity_ref in world.iter_entities() {
        if entity_ref.get::<CoreBuild>().is_none() {
            continue;
        }
        if team_of(world, entity_ref.id()) != Some(team) {
            continue;
        }
        let Some(pos) = entity_ref.get::<Pos>() else {
            continue;
        };
        let dx = pos.x - x;
        let dy = pos.y - y;
        let dist2 = dx * dx + dy * dy;
        let id = entity_ref.id();
        let replace = match best {
            None => true,
            Some((best_dist, best_entity)) => {
                dist2 < best_dist || (dist2 == best_dist && id.index() < best_entity.index())
            }
        };
        if replace {
            best = Some((dist2, id));
        }
    }
    best.map(|(_, entity)| entity)
}

/// `AIController.getClosestSpawner()`: nearest spawn tile (`Geometry.findClosest`;
/// ties keep the first, i.e. the spawn order).
pub(super) fn closest_spawn_tile(runtime: &UnitRuntime, x: f32, y: f32) -> Option<TilePos> {
    let mut best: Option<(f32, TilePos)> = None;
    for &tile in runtime.wave_spawner.get_spawns() {
        // `Tile.worldx()/worldy()` (Tile.java:166-171).
        let cx = tile.x() as f32 * TILE_SIZE;
        let cy = tile.y() as f32 * TILE_SIZE;
        let dx = cx - x;
        let dy = cy - y;
        let dist2 = dx * dx + dy * dy;
        if best.is_none_or(|(best_dist, _)| dist2 < best_dist) {
            best = Some((dist2, tile));
        }
    }
    best.map(|(_, tile)| tile)
}

/// `unit.within(Tile spawner, radius)`: distance to the spawn tile's `worldx`.
pub(super) fn within_spawn(tile: TilePos, x: f32, y: f32, radius: f32) -> bool {
    // `Tile.worldx()/worldy()` (Tile.java:166-171).
    let cx = tile.x() as f32 * TILE_SIZE;
    let cy = tile.y() as f32 * TILE_SIZE;
    let dx = cx - x;
    let dy = cy - y;
    dx * dx + dy * dy <= radius * radius
}

/// `Units.invalidateTarget(target, team, x, y)` (Units.java:1049-1056): dead or
/// despawned, own team, or a non-targetable unit. No distance check — the
/// 3-argument `AIController.invalid` passes `Float.MAX_VALUE`.
pub(crate) fn invalid_target(
    world: &World,
    content: &ContentRegistry,
    team: u8,
    target: Entity,
) -> bool {
    let Some(health) = world.get::<Health>(target) else {
        return true;
    };
    if health.dead {
        return true;
    }
    if team_of(world, target) == Some(team) {
        return true;
    }
    if let Some(comp) = world.get::<UnitTypeComp>(target) {
        // `UnitType.targetable(unit, targeter)` (UnitType.java:637-639).
        let targetable = content.unit(comp.type_id).is_some_and(|def| {
            def.targetable
                || (def.vulnerable_with_payloads
                    && world
                        .get::<crate::entities::comp::unit::comp::UnitPayloads>(target)
                        .is_some_and(|payloads| payloads.has_payload()))
        });
        if !targetable {
            return true;
        }
    }
    false
}

/// World-pixel center of a live building (`Pos`, else the tile's `worldx`).
pub(super) fn building_center(world: &World, entity: Entity) -> Option<(f32, f32)> {
    if let Some(pos) = world.get::<Pos>(entity) {
        return Some((pos.x, pos.y));
    }
    world
        .get::<BuildingComp>(entity)
        .map(|comp| tile_world(comp.pos))
        .or_else(|| {
            world
                .get::<Building>(entity)
                .map(|building| tile_world(building.tile))
        })
}

/// `Tile.worldx()`/`worldy()` (Tile.java:166-171): the tile's corner-based
/// world coordinate, `x * tilesize`. The tile spans `[x*8 - 4, x*8 + 4)` and a
/// building on it sits at `drawx = worldx + block.offset`.
pub(super) fn tile_world(tile: TilePos) -> (f32, f32) {
    (tile.x() as f32 * TILE_SIZE, tile.y() as f32 * TILE_SIZE)
}

/// `AIController.faceTarget` (AIController.java:121-128): omni-movement
/// units and mechs turn toward a valid in-range target when `type.faceTarget`
/// and the type has weapons (`unit.lookAt`: `moveToward(rotation, angle,
/// rotateSpeed * speedMultiplier)`, UnitComp.java:491). Moving units keep
/// facing their velocity (already applied by the movement pass). The
/// `Predict.intercept` lead is applied through
/// `combat::targeting::intercept_positions` (AIController.java:124).
pub(super) fn face_target(
    world: &mut World,
    content: &ContentRegistry,
    entity: Entity,
    target: Option<Entity>,
) {
    let Some(target) = target else {
        return;
    };
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return;
    };
    let mech = world
        .get::<crate::entities::comp::unit::comp::MechComp>(entity)
        .is_some();
    if !(def.omni_movement || mech) || !def.face_target || def.weapons.is_empty() {
        return;
    }
    let (Some(pos), Some(tpos)) = (
        world.get::<Pos>(entity).copied(),
        world.get::<Pos>(target).copied(),
    ) else {
        return;
    };
    let speed_mult = world
        .get::<crate::entities::comp::unit::comp::StatusComp>(entity)
        .map_or(1.0, |status| status.speed_multiplier);
    // `Predict.intercept(unit, target, unit.type.weapons.first().bullet)`
    // (AIController.java:124).
    let src_vel = world
        .get::<Vel>(entity)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });
    let dst_vel = world
        .get::<Vel>(target)
        .copied()
        .unwrap_or(Vel { x: 0.0, y: 0.0 });
    let (bullet_speed, keep_velocity) = def
        .weapons
        .first()
        .and_then(|weapon| content.bullet(weapon.bullet.id))
        .map(|bullet| (bullet.speed, bullet.keep_velocity))
        .unwrap_or((0.0, true));
    let aim = crate::combat::targeting::intercept_positions(
        (pos.x, pos.y),
        (src_vel.x, src_vel.y),
        (tpos.x, tpos.y),
        (dst_vel.x, dst_vel.y),
        keep_velocity,
        bullet_speed,
    );
    let angle = (aim.1 - pos.y).atan2(aim.0 - pos.x).to_degrees();
    if let Some(mut core) = world.get_mut::<UnitCore>(entity) {
        core.rotation = crate::ai::types::missile::move_toward_angle(
            core.rotation,
            angle,
            def.rotate_speed * speed_mult,
        );
    }
}

/// `Sized.hitSize()` of a target: a unit's `HitboxComp.hit_size`, a building's
/// `size * tilesize` (`BuildingComp.hitSize`, BuildingComp.java:2054-2056),
/// else `0`.
pub(crate) fn target_hit_size_of(world: &World, content: &ContentRegistry, target: Entity) -> f32 {
    target_hit_size(world, content, target)
}

pub(super) fn target_hit_size(world: &World, content: &ContentRegistry, target: Entity) -> f32 {
    if let Some(hitbox) = world.get::<HitboxComp>(target) {
        return hitbox.hit_size;
    }
    if let Some(building) = world.get::<Building>(target) {
        return content
            .block(building.block)
            .map(|def| def.size as f32 * TILE_SIZE)
            .unwrap_or(0.0);
    }
    0.0
}

/// `Units.invalidateTarget(target, team, x, y, range)` (Units.java:151-157):
/// `null || !within(x, y, range + target.hitSize/2) || own team || !isValid
/// || (Unit && !targetable)`. `range = f32::MAX` skips the distance term.
pub(crate) fn invalidate_target_entity(
    world: &World,
    content: &ContentRegistry,
    team: u8,
    target: Entity,
    x: f32,
    y: f32,
    range: f32,
) -> bool {
    invalidate_target(world, content, team, Some(target), x, y, range)
}

pub(super) fn invalidate_target(
    world: &World,
    content: &ContentRegistry,
    team: u8,
    target: Option<Entity>,
    x: f32,
    y: f32,
    range: f32,
) -> bool {
    let Some(target) = target else {
        return true;
    };
    if invalid_target(world, content, team, target) {
        return true;
    }
    if range == f32::MAX {
        return false;
    }
    let Some(pos) = world.get::<Pos>(target) else {
        return true;
    };
    let r = range + target_hit_size(world, content, target) / 2.0;
    let dx = pos.x - x;
    let dy = pos.y - y;
    dx * dx + dy * dy > r * r
}
