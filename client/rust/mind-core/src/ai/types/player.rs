// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Player` controller (plan 15 possession): the live input → unit movement
//! bridge. Ports `DesktopInput.updateMovement`
//! (`core/src/mindustry/input/DesktopInput.java:975-1010`) plus
//! `UnitComp.movePref`/`moveAt`/`rotateMove`
//! (`core/src/mindustry/entities/comp/UnitComp.java:101-153`).
//!
//! The host uploads the binding axis every frame through [`PlayerInput`];
//! [`update_player`] runs inside `EntitySet::UpdateUnits` for units carrying
//! `AiKind::Player` and replaces the AI target pass for that tick. Units are in
//! world pixels; `Time.delta` is 1 at the fixed 60 Hz step, so velocity is a
//! per-tick step (`VelComp.move(vel * Time.delta)`).

use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

use crate::content::{ContentRegistry, ItemId};
use crate::entities::comp::unit::comp::{
    ItemsComp, MinerComp, PhysicsComp, UnitCore, UnitTypeComp,
};
use crate::entities::comp::{Building, Pos, TeamComp, Vel};
use crate::world::blocks::distribution::transfer::{
    dispatch_get_maximum_accepted, dispatch_handle_stack, item_count,
};
use crate::world::blocks::storage::core_block::CoreBuild;
use crate::world::{TilePos, WorldGrid};

use super::ground::{normalize_angle, tile_center};

/// Host-uploaded player movement axis (`Core.input.axis(Binding.moveX/Y)`) plus
/// the mining order state (`InputHandler.tryBeginMine` + `MinerComp.mineTimer`).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct PlayerInput {
    /// Horizontal axis in `[-1, 1]` (positive = east).
    pub axis_x: f32,
    /// Vertical axis in `[-1, 1]` (positive = south).
    pub axis_y: f32,
    /// Latched once a non-zero axis reached a player unit (hint trigger).
    pub moved: bool,
    /// Ordered mine tile (`Player.unit().mineTile` via `tryBeginMine`).
    pub mine_tile: Option<TilePos>,
    /// Mining progress in ticks (`MinerComp.mineTimer`).
    pub mine_timer: f32,
    /// Total ore produced by the player unit (evidence/hint hook).
    pub mined: i64,
}

/// Runs one player-controller tick for `entity`; returns `true` when a non-zero
/// axis was applied this tick (`HintsFragment.desktopMove` trigger).
pub fn update_player(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
) -> bool {
    let Some(mut input) = world.get_resource::<PlayerInput>().copied() else {
        return false;
    };
    let Some(mut pos) = world.get::<Pos>(entity).copied() else {
        return false;
    };
    let Some(mut vel) = world.get::<Vel>(entity).copied() else {
        return false;
    };
    let Some(mut core) = world.get::<UnitCore>(entity).copied() else {
        return false;
    };
    let Some(accel) = world
        .get::<PhysicsComp>(entity)
        .map(|physics| physics.accel)
    else {
        return false;
    };
    // `UnitComp.speed()` with floor/boost/strafe multipliers (PAR-0137).
    let speed = crate::entities::comp::unit::movement::unit_speed(world, grid, content, entity);
    let (omni, rotate_speed) = world
        .get::<crate::entities::comp::unit::comp::UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
        .map(|def| (def.omni_movement, def.rotate_speed))
        .unwrap_or((false, 0.0));

    let (ax, ay) = (input.axis_x, input.axis_y);
    let moving = ax != 0.0 || ay != 0.0;
    let input_len = (ax * ax + ay * ay).sqrt().min(1.0);

    // `DesktopInput.updateMovement`: movement = normalized axis * speed().
    let target_len = input_len * speed;
    if !omni && moving {
        // `UnitComp.rotateMove`: face the input at `rotateSpeed` degrees/tick.
        core.rotation = move_toward(core.rotation, ay.atan2(ax).to_degrees(), rotate_speed);
    }
    let (dir_x, dir_y) = if omni {
        if input_len > 0.0 {
            (ax / input_len, ay / input_len)
        } else {
            (0.0, 0.0)
        }
    } else {
        let rad = core.rotation.to_radians();
        (rad.cos(), rad.sin())
    };
    let target = (dir_x * target_len, dir_y * target_len);

    // `UnitComp.moveAt`: `vel += (target - vel).limit(accel * target.len())`.
    let accel_limit = accel * target_len;
    let dx = target.0 - vel.x;
    let dy = target.1 - vel.y;
    let dist = (dx * dx + dy * dy).sqrt();
    if accel_limit > 0.0 && dist > 0.0 {
        let scale = dist.min(accel_limit) / dist;
        vel.x += dx * scale;
        vel.y += dy * scale;
    } else if accel_limit <= 0.0 {
        vel.x = target.0;
        vel.y = target.1;
    }

    // `VelComp.update`: `move(vel * Time.delta)` with `Time.delta == 1`.
    pos.x += vel.x;
    pos.y += vel.y;

    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
    if let Some(mut stored) = world.get_mut::<UnitCore>(entity) {
        *stored = core;
    }
    // `MinerComp.update`: progress the ordered mine tile and yield the ore.
    update_mining(world, grid, content, entity, &mut input);
    if moving {
        input.moved = true;
    }
    world.insert_resource(input);
    moving
}

/// `InputHandler.canMine`: the tile holds a drop the unit's tier can mine
/// (`core/src/mindustry/input/InputHandler.java:2045`).
pub fn can_mine_tile(
    world: &World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    tile: TilePos,
) -> bool {
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return false;
    };
    let (tx, ty) = (tile.x() as i32, tile.y() as i32);
    if tx < 0 || ty < 0 || tx >= grid.width() || ty >= grid.height() {
        return false;
    }
    // `getMineResult`: floor/overlay mining requires the tile block to be air
    // (`tile.block() == Blocks.air`); a placed building covers the ore.
    let cell = grid.tiles.get(tx, ty);
    if cell.block != crate::content::BlockId::AIR {
        return false;
    }
    let drop = cell
        .drop(content)
        .or_else(|| content.block(cell.overlay).and_then(|def| def.item_drop));
    let Some(item) = drop else {
        return false;
    };
    // Tier gate (`CanMine`: drop hardness <= mine tier).
    content.item(item).map(|def| def.hardness).unwrap_or(0) <= def.mine_tier
}

/// `MinerComp.update`: validate the ordered tile, advance the mine timer and
/// add the ore drop to the unit's inventory.
///
/// Ports `core/src/mindustry/entities/comp/MinerComp.java:68-122` (core
/// transfer/offload branches are plan-08/12 logistics; the tutorial's counter
/// counts production, so the produced count is tracked here).
fn update_mining(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    input: &mut PlayerInput,
) {
    let Some(tile) = input.mine_tile else {
        set_mining(world, entity, None, false);
        return;
    };
    if !can_mine_tile(world, grid, content, entity, tile) {
        clear_mine_order(input);
        set_mining(world, entity, None, false);
        return;
    }
    let Some(def) = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id))
    else {
        return;
    };
    let (tx, ty) = (tile.x() as i32, tile.y() as i32);
    let cell = grid.tiles.get(tx, ty);
    let Some(item) = cell
        .drop(content)
        .or_else(|| content.block(cell.overlay).and_then(|def| def.item_drop))
    else {
        return;
    };
    let hardness = content.item(item).map(|def| def.hardness).unwrap_or(0);
    let Some(pos) = world.get::<Pos>(entity).copied() else {
        return;
    };
    let (cx, cy) = tile_center(tx, ty);
    let range = def.mine_range.max(40.0);
    if (cx - pos.x).powi(2) + (cy - pos.y).powi(2) > range * range {
        // Keep the order; the unit is not in range yet.
        set_mining(world, entity, Some(tile), false);
        return;
    }
    // Single-stack inventory (`acceptsItem`): another item or a full stack
    // cancels the order (upstream clears `mineTile`).
    let items = world.get::<ItemsComp>(entity).copied().unwrap_or_default();
    let capacity = def.item_capacity.max(1);
    let accepted = match items.item {
        None => true,
        Some((held, amount)) => held == item && amount < capacity,
    };
    if !accepted {
        clear_mine_order(input);
        set_mining(world, entity, None, false);
        return;
    }
    // `rate = Time.delta * mineSpeed`; threshold `50 + hardness*15` or `50+15`.
    input.mine_timer += def.mine_speed.max(0.0);
    let threshold = 50.0
        + if def.mine_hardness_scaling {
            hardness as f32 * 15.0
        } else {
            15.0
        };
    if input.mine_timer >= threshold {
        input.mine_timer = 0.0;
        input.mined += 1;
        // `MinerComp.update` -> `SectorInfo.handleProduction`: the campaign tick
        // folds the delta into the played sector's stats (Java MinerComp:98).
        if let Some(queues) = world.get_resource::<crate::game::runtime::CampaignCoreQueues>()
            && let Ok(mut state) = queues.0.lock()
        {
            let team = world.get::<TeamComp>(entity).map(|t| t.team);
            if state.is_campaign && team == Some(state.default_team) {
                state.production_deltas.push((item, 1));
            }
        }
        // `MinerComp.update` offload branch: `offloadImmediately()` is true for
        // the player, so the mined item transfers into `closestCore()` when it
        // is within `Vars.mineTransferRange` and accepts the stack.
        if !offload_into_core(world, entity, pos.x, pos.y, item)
            && let Some(mut stored) = world.get_mut::<ItemsComp>(entity)
        {
            stored.add_item(item, 1);
        }
    }
    set_mining(world, entity, Some(tile), true);
}

/// `Vars.mineTransferRange`: the core-transfer radius shared by all units.
const MINE_TRANSFER_RANGE: f32 = 220.0;

/// `MinerComp.update` core-transfer branch for the player unit
/// (`offloadImmediately()` is true for players): the mined item goes into
/// `closestCore()` when the core is within [`MINE_TRANSFER_RANGE`] and accepts
/// one (`core.acceptStack(item, 1, this) == 1`). Returns `true` when the item
/// left the unit.
fn offload_into_core(world: &mut World, entity: Entity, x: f32, y: f32, item: ItemId) -> bool {
    let Some(team) = world.get::<TeamComp>(entity).map(|t| t.team) else {
        return false;
    };
    let Some((core, cx, cy)) = closest_core(world, x, y, team) else {
        return false;
    };
    let dx = cx - x;
    let dy = cy - y;
    if dx * dx + dy * dy > MINE_TRANSFER_RANGE * MINE_TRANSFER_RANGE {
        return false;
    }
    let cap = dispatch_get_maximum_accepted(world, core, item);
    let cur = item_count(world, core, item);
    if cap - cur < 1 {
        return false;
    }
    // `Call.transferItemTo` reaches the core through `handleStack` (the offload
    // does not count toward `stats.coreItemCount`).
    dispatch_handle_stack(world, core, item, 1);
    true
}

/// `Teams.closestCore(x, y, team)`: nearest team core with its world position;
/// ties break by the stable entity index. The core's position is its anchor
/// tile's corner (`Building.x`/`y`), the centre for the odd multiblocks the
/// campaign fields.
fn closest_core(world: &World, x: f32, y: f32, team: u8) -> Option<(Entity, f32, f32)> {
    let mut best: Option<(f32, _, Entity)> = None;
    for entity_ref in world.iter_entities() {
        let id = entity_ref.id();
        if entity_ref.get::<TeamComp>().map(|t| t.team) != Some(team) {
            continue;
        }
        if entity_ref.get::<CoreBuild>().is_none() {
            continue;
        }
        let Some(building) = entity_ref.get::<Building>() else {
            continue;
        };
        let unit = crate::config::TILESIZE as f32;
        let (cx, cy) = (
            building.tile.x() as f32 * unit,
            building.tile.y() as f32 * unit,
        );
        let dx = cx - x;
        let dy = cy - y;
        let dist2 = dx * dx + dy * dy;
        let better = match best {
            None => true,
            Some((best_dist, best_index, _)) => {
                dist2 < best_dist || (dist2 == best_dist && id.index() < best_index)
            }
        };
        if better {
            best = Some((dist2, id.index(), id));
        }
    }
    let (_, _, core) = best?;
    let building = world.get::<Building>(core)?;
    let unit = crate::config::TILESIZE as f32;
    Some((
        core,
        building.tile.x() as f32 * unit,
        building.tile.y() as f32 * unit,
    ))
}

/// Clears the ordered tile and timer.
fn clear_mine_order(input: &mut PlayerInput) {
    input.mine_tile = None;
    input.mine_timer = 0.0;
}

/// Writes the unit's `MinerComp` state (`mineTile`/`mining`).
fn set_mining(world: &mut World, entity: Entity, tile: Option<TilePos>, mining: bool) {
    if let Some(mut stored) = world.get_mut::<MinerComp>(entity) {
        stored.mine_tile = tile;
        stored.mining = mining;
    }
}

/// `Angles.moveToward`: shortest signed angular step toward `to`.
fn move_toward(from: f32, to: f32, max_delta: f32) -> f32 {
    let mut diff = (to - from) % 360.0;
    if diff > 180.0 {
        diff -= 360.0;
    }
    if diff < -180.0 {
        diff += 360.0;
    }
    normalize_angle(from + diff.clamp(-max_delta, max_delta))
}

#[cfg(test)]
mod tests {
    use crate::sim::Sim;

    #[test]
    fn player_axis_moves_an_omni_unit_east() {
        let mut sim = Sim::new(
            1,
            64,
            64,
            crate::content::BlockId::AIR,
            crate::content::BlockId::AIR,
        );
        let Ok(()) = sim.install_unit_runtime() else {
            panic!("unit runtime installs");
        };
        // `flare` is an omni air unit; spawn east-facing with a full axis.
        let Some(_entity) = sim.spawn_player_unit("flare", 0, 100.0, 100.0, 0.0) else {
            panic!("player unit spawns");
        };
        sim.set_player_input(1.0, 0.0);
        for _ in 0..30 {
            let Ok(()) = sim.tick() else {
                panic!("tick");
            };
        }
        let pos = sim.player_unit_position().expect("player position");
        assert!(pos.0 > 100.0, "player moved east: {pos:?}");
        assert!(sim.player_unit_moved(), "moved flag latched");
    }

    #[test]
    fn player_mines_an_ore_overlay() {
        let mut sim = Sim::new(
            1,
            64,
            64,
            crate::content::BlockId::AIR,
            crate::content::BlockId::AIR,
        );
        let Ok(()) = sim.install_unit_runtime() else {
            panic!("unit runtime installs");
        };
        let Some(ore) = sim
            .unit_runtime()
            .and_then(|runtime| runtime.content.block_id("ore-copper"))
        else {
            panic!("ore-copper exists");
        };
        let index = sim.grid.tiles.index(12, 12);
        sim.grid.tiles.geti_mut(index).overlay = ore;
        sim.refresh_unit_runtime();
        let Some(_entity) = sim.spawn_player_unit("alpha", 0, 100.0, 100.0, 0.0) else {
            panic!("player unit spawns");
        };
        sim.set_player_mine_tile(Some(crate::world::TilePos::new(12, 12)));
        for _ in 0..40 {
            let Ok(()) = sim.tick() else {
                panic!("tick");
            };
        }
        assert!(
            sim.player_mined_count() >= 1,
            "mined {}",
            sim.player_mined_count()
        );
    }

    #[test]
    fn no_input_keeps_the_player_still() {
        let mut sim = Sim::new(
            1,
            64,
            64,
            crate::content::BlockId::AIR,
            crate::content::BlockId::AIR,
        );
        let Ok(()) = sim.install_unit_runtime() else {
            panic!("unit runtime installs");
        };
        let Some(_entity) = sim.spawn_player_unit("flare", 0, 100.0, 100.0, 0.0) else {
            panic!("player unit spawns");
        };
        let before = sim.player_unit_position().expect("player position");
        sim.set_player_input(0.0, 0.0);
        for _ in 0..30 {
            let Ok(()) = sim.tick() else {
                panic!("tick");
            };
        }
        let after = sim.player_unit_position().expect("player position");
        assert_eq!(before, after, "no axis -> no movement");
        assert!(!sim.player_unit_moved(), "moved flag stays clear");
    }

    #[test]
    fn player_offloads_mined_ore_into_the_core() {
        use crate::world::blocks::storage::core_block::CoreBuild;
        use crate::world::modules::ItemModule;

        let mut sim = Sim::new(
            1,
            64,
            64,
            crate::content::BlockId::AIR,
            crate::content::BlockId::AIR,
        );
        let Ok(()) = sim.install_unit_runtime() else {
            panic!("unit runtime installs");
        };
        let Ok(()) = sim.install_block_runtime() else {
            panic!("block runtime installs");
        };
        let Some(core) = sim
            .unit_runtime()
            .and_then(|runtime| runtime.content.block_id("core-shard"))
        else {
            panic!("core-shard exists");
        };
        // core-shard is 3x3; anchor (14,14) -> footprint (13,13)..(15,15).
        let Ok(()) = sim.apply(crate::command::Command::Place {
            x: 14,
            y: 14,
            block: core,
        }) else {
            panic!("core places");
        };
        let Some(ore) = sim
            .unit_runtime()
            .and_then(|runtime| runtime.content.block_id("ore-copper"))
        else {
            panic!("ore-copper exists");
        };
        let index = sim.grid.tiles.index(12, 12);
        sim.grid.tiles.geti_mut(index).overlay = ore;
        sim.refresh_unit_runtime();
        // The unit mines at (12,12); the core anchor (14,14) is ~23 units away,
        // well inside `mineTransferRange` (220).
        let Some(entity) = sim.spawn_player_unit("alpha", 0, 100.0, 100.0, 0.0) else {
            panic!("player unit spawns");
        };
        sim.set_player_mine_tile(Some(crate::world::TilePos::new(12, 12)));
        let Some(copper) = sim
            .unit_runtime()
            .and_then(|runtime| runtime.content.item_id("copper"))
        else {
            panic!("copper exists");
        };
        for _ in 0..40 {
            let Ok(()) = sim.tick() else {
                panic!("tick");
            };
        }
        assert!(
            sim.player_mined_count() >= 1,
            "mined {}",
            sim.player_mined_count()
        );
        // `offloadImmediately()` (player): mined items leave the unit.
        let held = sim
            .ecs
            .0
            .get::<crate::entities::comp::unit::comp::ItemsComp>(entity)
            .and_then(|items| items.item)
            .map(|(_, amount)| amount)
            .unwrap_or(0);
        assert_eq!(held, 0, "player offload must not stack");
        let mut core_copper = 0;
        for entity_ref in sim.ecs.0.iter_entities() {
            if entity_ref.get::<CoreBuild>().is_none() {
                continue;
            }
            if let Some(module) = entity_ref.get::<ItemModule>() {
                core_copper = module.get(copper);
            }
        }
        assert!(core_copper >= 1, "core received copper: {core_copper}");
    }

    #[test]
    fn player_cannot_mine_under_a_building() {
        let mut sim = Sim::new(
            1,
            64,
            64,
            crate::content::BlockId::AIR,
            crate::content::BlockId::AIR,
        );
        let Ok(()) = sim.install_unit_runtime() else {
            panic!("unit runtime installs");
        };
        let Ok(()) = sim.install_block_runtime() else {
            panic!("block runtime installs");
        };
        let Some(ore) = sim
            .unit_runtime()
            .and_then(|runtime| runtime.content.block_id("ore-copper"))
        else {
            panic!("ore-copper exists");
        };
        let Some(conveyor) = sim
            .unit_runtime()
            .and_then(|runtime| runtime.content.block_id("conveyor"))
        else {
            panic!("conveyor exists");
        };
        let index = sim.grid.tiles.index(12, 12);
        sim.grid.tiles.geti_mut(index).overlay = ore;
        sim.refresh_unit_runtime();
        // `getMineResult` requires `tile.block() == air`; the building covers it.
        let Ok(()) = sim.apply(crate::command::Command::Place {
            x: 12,
            y: 12,
            block: conveyor,
        }) else {
            panic!("conveyor places over the ore");
        };
        let Some(_entity) = sim.spawn_player_unit("alpha", 0, 100.0, 100.0, 0.0) else {
            panic!("player unit spawns");
        };
        sim.set_player_mine_tile(Some(crate::world::TilePos::new(12, 12)));
        for _ in 0..20 {
            let Ok(()) = sim.tick() else {
                panic!("tick");
            };
        }
        assert_eq!(
            sim.player_mine_tile(),
            None,
            "mine order clears under a building"
        );
        assert_eq!(sim.player_mined_count(), 0, "nothing mined under the block");
    }
}
