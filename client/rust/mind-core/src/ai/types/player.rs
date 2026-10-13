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

use crate::content::ContentRegistry;
use crate::entities::comp::unit::comp::{
    MechComp, MinerComp, PhysicsComp, StatusComp, UnitCore, UnitTypeComp,
};
use crate::entities::comp::{Pos, Vel};
use crate::sim::builder::BuilderPlans;
use crate::weapons::UnitWeapons;

use crate::world::{TilePos, WorldGrid};

use super::ground::normalize_angle;

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
    /// `player.shooting` (left button held on a free tile;
    /// `DesktopInput.java:701-721`).
    pub shooting: bool,
    /// Cursor world position (`input.mouseWorldX/Y`), the aim point
    /// (`DesktopInput.java:1004-1005`).
    pub aim_x: f32,
    /// Cursor world position, Y half.
    pub aim_y: f32,
    /// `player.boosting = keyDown(Binding.boost)` (`DesktopInput.java:1010`).
    pub boosting: bool,
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
    let unit_def = world
        .get::<UnitTypeComp>(entity)
        .and_then(|comp| content.unit(comp.type_id));
    let (omni, rotate_speed) = unit_def
        .map(|def| (def.omni_movement, def.rotate_speed))
        .unwrap_or((false, 0.0));
    let (aim_dst, has_weapons, face_target, rotate_to_building) = unit_def
        .map(|def| {
            (
                def.aim_dst,
                def.has_weapons(),
                def.face_target,
                def.rotate_to_building,
            )
        })
        .unwrap_or((0.0, false, false, false));

    // `DesktopInput.updateMovement` (`DesktopInput.java:975-1010`): aim/facing
    // decisions run before `movePref`.
    let boosted = world.get::<MechComp>(entity).is_some()
        && crate::combat::bullet::unit_is_flying(world, entity);
    let building = player_actively_building(world, content, entity, (pos.x, pos.y));
    let mining = input.mine_tile.is_some() && building.is_none();
    // `InputHandler.canShoot` sim half (`InputHandler.java:2269-2272`).
    let shoot = input.shooting && !boosted && building.is_none() && !mining;
    let mouse_angle = angle_deg(input.aim_x - pos.x, input.aim_y - pos.y);
    let aim_cursor = omni && shoot && has_weapons && face_target && !boosted;
    // `UnitComp.prefRotation` (`UnitComp.java:197-206`).
    let pref = if let (Some((bx, by)), true) = (building, rotate_to_building) {
        angle_deg(bx - pos.x, by - pos.y)
    } else if let Some(tile) = input.mine_tile {
        // `angleTo(Tile)` uses the tile corner (`Tile.worldx` = `x * tilesize`).
        let unit = crate::config::TILESIZE as f32;
        angle_deg(
            tile.x() as f32 * unit - pos.x,
            tile.y() as f32 * unit - pos.y,
        )
    } else if omni && vel.x * vel.x + vel.y * vel.y >= 0.01 {
        angle_deg(vel.x, vel.y)
    } else {
        core.rotation
    };
    let target = if aim_cursor { mouse_angle } else { pref };
    let speed_mult = world
        .get::<StatusComp>(entity)
        .map_or(1.0, |status| status.speed_multiplier);
    // `UnitComp.lookAt` (`UnitComp.java:491-493`).
    core.rotation =
        crate::weapons::angles_move_toward(core.rotation, target, rotate_speed * speed_mult);

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

    // `PlayerComp.update` (`PlayerComp.java:229-232`): the boost key lifts a
    // `canBoost` mech; `update_kinematics` consumes `UnitCore.boosting`.
    core.boosting = input.boosting;

    if let Some(mut stored) = world.get_mut::<Pos>(entity) {
        *stored = pos;
    }
    if let Some(mut stored) = world.get_mut::<Vel>(entity) {
        *stored = vel;
    }
    if let Some(mut stored) = world.get_mut::<UnitCore>(entity) {
        *stored = core;
    }
    // `updateMovement`: `unit.aim(mouseWorldX, mouseWorldY, true)` then
    // `unit.controlWeapons(true, player.shooting && !boosted)`
    // (`WeaponsComp.aim`/`controlWeapons`, `WeaponsComp.java:39-74`).
    if let Some(mut weapons) = world.get_mut::<UnitWeapons>(entity) {
        let mut vx = input.aim_x - pos.x;
        let mut vy = input.aim_y - pos.y;
        let len = (vx * vx + vy * vy).sqrt();
        // Arc `Vec2.setLength` is a no-op for a zero vector.
        if len > 0.0 && len < aim_dst {
            let s = aim_dst / len;
            vx *= s;
            vy *= s;
        }
        let w = &mut *weapons;
        for (weapon, mount) in w.weapons.iter().zip(w.mounts.iter_mut()) {
            if weapon.controllable {
                mount.target = None;
                mount.aim_x = pos.x + vx;
                mount.aim_y = pos.y + vy;
                mount.rotate = true;
                mount.shoot = shoot;
            }
        }
    }
    // `MinerComp.update`: progress the ordered mine tile and yield the ore.
    update_mining(world, grid, content, entity, &mut input);
    if moving {
        input.moved = true;
    }
    world.insert_resource(input);
    moving
}

/// `BuilderComp.activelyBuilding` (`BuilderComp.java:301-309`): the head plan
/// exists, `updateBuilding` is on and (unless `rules.infiniteResources`) the
/// unit is within `type.buildRange` of the plan tile. Returns the plan's tile
/// corner (Java `Tile.worldx/worldy`); `None` when not actively building.
pub(crate) fn player_actively_building(
    world: &World,
    content: &ContentRegistry,
    entity: Entity,
    pos: (f32, f32),
) -> Option<(f32, f32)> {
    let plans = world.get::<BuilderPlans>(entity)?;
    if !plans.update_building {
        return None;
    }
    let plan = plans.plans.front()?;
    let unit = crate::config::TILESIZE as f32;
    let (bx, by) = (plan.x as f32 * unit, plan.y as f32 * unit);
    let infinite =
        crate::game::runtime::rules_of(world).is_some_and(|rules| rules.infinite_resources);
    if !infinite {
        let range = world
            .get::<UnitTypeComp>(entity)
            .and_then(|comp| content.unit(comp.type_id))
            .map_or(0.0, |def| def.build_range);
        let dx = bx - pos.0;
        let dy = by - pos.1;
        if dx * dx + dy * dy > range * range {
            return None;
        }
    }
    Some((bx, by))
}

/// `Angles.angle(x, y)`: `atan2` in degrees, normalised to `[0, 360)`.
fn angle_deg(x: f32, y: f32) -> f32 {
    let angle = y.atan2(x).to_degrees();
    if angle < 0.0 { angle + 360.0 } else { angle }
}

/// `InputHandler.canMine`: the tile holds a drop the unit's tier can mine
/// (`core/src/mindustry/input/InputHandler.java:2045`), via the shared
/// `MinerComp.getMineResult` helper.
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
    crate::entities::comp::unit::miner::get_mine_result(content, def, grid, tile).is_some()
}

/// `MinerComp.update` wrapper for the player unit (G01 T10a): the ordered tile
/// (`PlayerInput.mine_tile`) is mirrored into the shared [`MinerComp`], the
/// generic port in `entities/comp/unit/miner.rs` runs for every unit, and the
/// produced count feeds the tutorial counter.
fn update_mining(
    world: &mut World,
    grid: &WorldGrid,
    content: &ContentRegistry,
    entity: Entity,
    input: &mut PlayerInput,
) {
    if let Some(mut comp) = world.get_mut::<MinerComp>(entity) {
        comp.mine_tile = input.mine_tile;
        comp.mine_timer = input.mine_timer;
    }
    crate::entities::comp::unit::miner::update_miner_comp(world, grid, content, entity);
    if let Some(comp) = world.get::<MinerComp>(entity).copied() {
        input.mine_tile = comp.mine_tile;
        input.mine_timer = comp.mine_timer;
        let produced = comp.mined as i64;
        if produced > input.mined {
            input.mined = produced;
        }
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
    use crate::content::BlockId;
    use crate::entities::comp::unit::comp::UnitCore;
    use crate::sim::Sim;

    /// Player unit rotation (`UnitCore.rotation`).
    fn player_rotation(sim: &Sim) -> f32 {
        let entity = sim.player_unit().expect("player unit");
        sim.ecs
            .0
            .get::<UnitCore>(entity)
            .expect("unit core")
            .rotation
    }

    /// Player unit elevation (`UnitCore.elevation`).
    fn player_elevation(sim: &Sim) -> f32 {
        let entity = sim.player_unit().expect("player unit");
        sim.ecs
            .0
            .get::<UnitCore>(entity)
            .expect("unit core")
            .elevation
    }

    /// Bullets created so far by the live unit runtime.
    fn bullets_created(sim: &Sim) -> u64 {
        sim.unit_runtime().expect("unit runtime").bullets_created
    }

    #[test]
    fn shooting_player_turns_to_cursor() {
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_unit_runtime().expect("unit runtime");
        sim.spawn_player_unit("alpha", 0, 100.0, 100.0, 90.0)
            .expect("player spawns");
        sim.set_player_control(0.0, 0.0, true, 150.0, 100.0, false);
        sim.tick().expect("tick");
        assert!(
            (player_rotation(&sim) - 75.0).abs() < 0.01,
            "one 15-degree step east: {}",
            player_rotation(&sim)
        );
        for _ in 0..5 {
            sim.tick().expect("tick");
        }
        assert!(
            (player_rotation(&sim) - 0.0).abs() < 0.01,
            "six ticks reach the cursor angle: {}",
            player_rotation(&sim)
        );
    }

    #[test]
    fn shooting_player_fires() {
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_unit_runtime().expect("unit runtime");
        sim.spawn_player_unit("alpha", 0, 100.0, 100.0, 0.0)
            .expect("player spawns");
        sim.set_player_control(0.0, 0.0, true, 150.0, 100.0, false);
        for _ in 0..40 {
            sim.tick().expect("tick");
        }
        assert!(
            bullets_created(&sim) >= 1,
            "held mouse fires: {}",
            bullets_created(&sim)
        );

        let mut idle = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        idle.install_unit_runtime().expect("unit runtime");
        idle.spawn_player_unit("alpha", 0, 100.0, 100.0, 0.0)
            .expect("player spawns");
        idle.set_player_control(0.0, 0.0, false, 150.0, 100.0, false);
        for _ in 0..40 {
            idle.tick().expect("tick");
        }
        assert_eq!(bullets_created(&idle), 0, "no shooting without the button");
    }

    #[test]
    fn omni_player_faces_velocity() {
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_unit_runtime().expect("unit runtime");
        sim.spawn_player_unit("alpha", 0, 100.0, 100.0, 0.0)
            .expect("player spawns");
        sim.set_player_control(0.0, 1.0, false, 0.0, 0.0, false);
        for _ in 0..30 {
            sim.tick().expect("tick");
        }
        assert!(
            (player_rotation(&sim) - 90.0).abs() < 0.01,
            "omni unit faces its velocity (south): {}",
            player_rotation(&sim)
        );
    }

    #[test]
    fn boosting_mech_cannot_shoot() {
        let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_unit_runtime().expect("unit runtime");
        sim.spawn_player_unit("nova", 0, 100.0, 100.0, 0.0)
            .expect("player spawns");
        sim.set_player_control(0.0, 0.0, true, 150.0, 100.0, true);
        for _ in 0..30 {
            sim.tick().expect("tick");
        }
        let mid = bullets_created(&sim);
        let elevation = player_elevation(&sim);
        for _ in 0..30 {
            sim.tick().expect("tick");
        }
        assert!(elevation > 0.09, "mech lifted off: {elevation}");
        assert_eq!(
            bullets_created(&sim),
            mid,
            "no new bullets while flying (elevation {elevation})"
        );
    }

    #[test]
    fn building_player_does_not_shoot() {
        let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
        sim.install_block_runtime().expect("block runtime");
        sim.install_unit_runtime().expect("unit runtime");
        let core = sim.world_block_id("core-shard").expect("core");
        sim.command(crate::determinism::SimCommand::Place {
            x: 10,
            y: 10,
            block: core,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("core placed");
        let wall = sim.world_block_id("copper-wall").expect("wall");
        sim.spawn_player_unit("alpha", 1, 80.0, 80.0, 0.0)
            .expect("player spawns");
        // Empty core: the plan stays queued, so the unit is actively building.
        assert!(sim.queue_player_place(14, 10, BlockId::new(wall), 0));
        sim.set_player_control(0.0, 0.0, true, 150.0, 80.0, false);
        for _ in 0..30 {
            sim.tick().expect("tick");
        }
        assert_eq!(
            bullets_created(&sim),
            0,
            "actively building units cannot shoot"
        );
    }

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
