// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicAI`: the controller installed by `ubind`/`ucontrol`/`ulocate`
//! (`core/src/mindustry/ai/types/LogicAI.java`, ported whole).
//!
//! Java fields this keeps: `control`, `moveX/moveY/moveRad`, `controlTimer`,
//! `targetTimer`, `controller`, `plan`, `execCache`, `aimControl`, `boost`,
//! `mainTarget`, `shoot`, `posTarget`, `radars`. The movement body is
//! `LogicAI.updateMovement`; the setters live in the logic executor
//! (`logic::executor::unit_control`).

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use indexmap::{IndexMap, IndexSet};

use super::super::ai_controller::AiCtx;
use crate::entities::comp::Pos;
use crate::entities::comp::unit::comp::{UnitCore, UnitTypeComp};
use crate::entities::comp::unit::weapon_mount::UnitWeapons;
use crate::logic::enums::LUnitControl;
use crate::sim::builder::{BuilderPlans, UnitBuildPlan};
use crate::world::TilePos;
use crate::world::WorldGrid;

/// `LogicAI.logicControlTimeout`: 10 s at 60 Hz.
pub const LOGIC_TIMEOUT_TICKS: f32 = 600.0;
/// `LogicAI.updateMovement` radar-cache refresh period (40 ticks).
pub const LOGIC_TARGET_TIMER: f32 = 40.0;
/// `LogicAI.transferDelay`: 1.5 s at 60 Hz.
pub const TRANSFER_DELAY_TICKS: f32 = 90.0;

/// `UnitLocateI.Cache`: the per-instruction `ulocate` result.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LocateCache {
    /// `cache.x` (tile coordinates).
    pub x: f32,
    /// `cache.y`.
    pub y: f32,
    /// `cache.found`.
    pub found: bool,
    /// `cache.build`.
    pub build: Option<Entity>,
}

/// `LogicAI` controller state.
///
/// `keepState() = true`: the component survives save/load; the executor's
/// `ucontrol` setters write it and [`update_logic`] reads it.
#[derive(Debug, Clone, Component)]
pub struct LogicAi {
    /// `control`.
    pub control: LUnitControl,
    /// `moveX` (world pixels).
    pub move_x: f32,
    /// `moveY` (world pixels).
    pub move_y: f32,
    /// `moveRad` (`approach`).
    pub move_rad: f32,
    /// `controlTimer`.
    pub control_timer: f32,
    /// `targetTimer`.
    pub target_timer: f32,
    /// `controller`: the processor building driving this unit.
    pub controller: Option<Entity>,
    /// `plan`: the running `ucontrol build`/`deconstruct` plan.
    pub plan: UnitBuildPlan,
    /// `execCache`: per-instruction `ulocate` results, keyed by instruction
    /// index in the owning executor's program.
    pub exec_cache: IndexMap<u32, LocateCache>,
    /// `aimControl`.
    pub aim_control: LUnitControl,
    /// `boost`.
    pub boost: bool,
    /// `mainTarget` (`targetp`).
    pub main_target: Option<Entity>,
    /// `shoot`.
    pub shoot: bool,
    /// `posTarget` (`target`), world pixels.
    pub pos_target: (f32, f32),
    /// `radars`: instruction keys seen since the last 40-tick clear.
    pub radars: IndexSet<u32>,
}

impl Default for LogicAi {
    fn default() -> Self {
        Self {
            control: LUnitControl::Idle,
            move_x: 0.0,
            move_y: 0.0,
            move_rad: 0.0,
            control_timer: LOGIC_TIMEOUT_TICKS,
            target_timer: 0.0,
            controller: None,
            plan: UnitBuildPlan::place(0, 0, 0, crate::content::BlockId::AIR),
            exec_cache: IndexMap::new(),
            aim_control: LUnitControl::Stop,
            boost: false,
            main_target: None,
            shoot: false,
            pos_target: (0.0, 0.0),
            radars: IndexSet::new(),
        }
    }
}

impl LogicAi {
    /// A fresh controller bound to `controller` (`new LogicAI()` +
    /// `la.controller = exec.thisv.building()`).
    pub fn new(controller: Option<Entity>) -> Self {
        Self {
            controller,
            ..Self::default()
        }
    }

    /// `LogicAI.checkTargetTimer`: `true` the first time `key` is seen since
    /// the last 40-tick clear.
    pub fn check_target_timer(&mut self, key: u32) -> bool {
        self.radars.insert(key)
    }
}

/// `UnitComp.range()`: `type.maxRange > 0 ? maxRange : range` (world units).
pub fn unit_range(ctx: &AiCtx, unit: Entity) -> f32 {
    ctx.world
        .get::<UnitTypeComp>(unit)
        .and_then(|comp| ctx.content.unit(comp.type_id))
        .map(|def| {
            if def.max_range > 0.0 {
                def.max_range
            } else {
                def.range
            }
        })
        .unwrap_or(0.0)
}

/// `Unit.prefRotation()` (`UnitComp.java:197-208`).
fn pref_rotation(ctx: &AiCtx, unit: Entity) -> f32 {
    let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
        return 0.0;
    };
    let def = ctx
        .world
        .get::<UnitTypeComp>(unit)
        .and_then(|comp| ctx.content.unit(comp.type_id));
    // `activelyBuilding() && type.rotateToBuilding` -> angle to the build plan
    // target (`player_actively_building` returns the active plan's world
    // position, `None` while not building).
    if let Some(def) = def.as_ref()
        && def.rotate_to_building
        && let Some((tx, ty)) = crate::ai::types::player::player_actively_building(
            ctx.world,
            ctx.content,
            unit,
            (pos.x, pos.y),
        )
    {
        return (ty - pos.y).atan2(tx - pos.x).to_degrees();
    }
    // `mineTile != null` -> angle to the mine tile (tile corner `worldx/y`).
    if let Some(mine) = ctx
        .world
        .get::<crate::entities::comp::unit::comp::MinerComp>(unit)
        .and_then(|comp| comp.mine_tile)
    {
        let ts = crate::config::TILESIZE as f32;
        return (mine.y() as f32 * ts - pos.y)
            .atan2(mine.x() as f32 * ts - pos.x)
            .to_degrees();
    }
    // `moving() && type.omniMovement` -> velocity angle.
    if let Some(vel) = ctx.world.get::<crate::entities::comp::Vel>(unit)
        && def.as_ref().is_none_or(|def| def.omni_movement)
        && (vel.x != 0.0 || vel.y != 0.0)
    {
        return vel.y.atan2(vel.x).to_degrees();
    }
    ctx.world
        .get::<UnitCore>(unit)
        .map(|core| core.rotation)
        .unwrap_or(0.0)
}

/// `LogicAI.updateMovement` (`LogicAI.java:41-107`): radar-cache refresh,
/// timeout count-down and the movement/look-at tail.
///
/// Returns `false` when the controller must reset (`unit.resetController()`).
pub fn update_logic(
    ctx: &mut AiCtx,
    unit: Entity,
    state: &mut LogicAi,
    control_path: &mut crate::ai::ControlPathfinder,
) -> bool {
    // `if(targetTimer > 0f) targetTimer -= Time.delta; else { radars.clear();
    //  targetTimer = 40f; }`
    if state.target_timer > 0.0 {
        state.target_timer -= 1.0;
    } else {
        state.radars.clear();
        state.target_timer = LOGIC_TARGET_TIMER;
    }

    // `if(controlTimer > 0 && controller != null && controller.isValid())
    //  controlTimer -= Time.delta; else { unit.resetController(); return; }`
    let controller_valid = state
        .controller
        .is_some_and(|controller| ctx.world.get_entity(controller).is_ok());
    if state.control_timer > 0.0 && controller_valid {
        state.control_timer -= 1.0;
    } else {
        return false;
    }

    let flying = ctx.is_flying(unit);
    let (mx, my, mrad) = (state.move_x, state.move_y, state.move_rad);
    match state.control {
        LUnitControl::Move => {
            crate::ai::types::flying::move_to_point_keep_distance(
                ctx.world,
                ctx.grid,
                ctx.content,
                unit,
                mx,
                my,
                1.0,
                30.0,
                flying,
                false,
            );
        }
        LUnitControl::Approach => {
            crate::ai::types::flying::move_to_point_keep_distance(
                ctx.world,
                ctx.grid,
                ctx.content,
                unit,
                mx,
                my,
                mrad - 7.0,
                7.0,
                true,
                false,
            );
        }
        LUnitControl::Pathfind => {
            if flying {
                crate::ai::types::flying::move_to_point_keep_distance(
                    ctx.world,
                    ctx.grid,
                    ctx.content,
                    unit,
                    mx,
                    my,
                    1.0,
                    30.0,
                    true,
                    false,
                );
            } else {
                let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
                    return true;
                };
                let from = TilePos::new(
                    WorldGrid::to_tile(pos.x) as i16,
                    WorldGrid::to_tile(pos.y) as i16,
                );
                let to = TilePos::new(WorldGrid::to_tile(mx) as i16, WorldGrid::to_tile(my) as i16);
                let result = control_path.get_path_position(from, to);
                // `if(result.move)`.
                if !result.unreachable {
                    let (dest_x, dest_y) = match result.next {
                        Some(tile) => (
                            tile.x() as f32 * crate::config::TILESIZE as f32,
                            tile.y() as f32 * crate::config::TILESIZE as f32,
                        ),
                        None => (mx, my),
                    };
                    // `Tmp.v2.epsilonEquals(result.dest, 4.1f) ? 30f : 0f`
                    // (`Tmp.v2` is the requested destination).
                    let near_final = (dest_x - mx).abs() <= 4.1 && (dest_y - my).abs() <= 4.1;
                    crate::ai::types::flying::move_to_point_keep_distance(
                        ctx.world,
                        ctx.grid,
                        ctx.content,
                        unit,
                        dest_x,
                        dest_y,
                        1.0,
                        if near_final { 30.0 } else { 0.0 },
                        true,
                        false,
                    );
                }
            }
        }
        LUnitControl::AutoPathfind => {
            let Some(pos) = ctx.world.get::<Pos>(unit).copied() else {
                return true;
            };
            let team = ctx
                .world
                .get::<crate::entities::comp::TeamComp>(unit)
                .map(|t| t.team)
                .unwrap_or(0);
            let range = unit_range(ctx, unit);
            let core = crate::sim::unit_runtime::closest_enemy_core(ctx.world, team, pos.x, pos.y);
            let core_close = core.is_some_and(|core| {
                ctx.world.get::<Pos>(core).is_some_and(|cpos| {
                    let dx = cpos.x - pos.x;
                    let dy = cpos.y - pos.y;
                    dx * dx + dy * dy <= (range * 0.5) * (range * 0.5)
                })
            });
            if !core_close {
                // Deviation: the spawner drop-zone stop needs the wave
                // spawner list (`UnitRuntime.wave_spawner`), which is removed
                // from the world while the unit loop runs; the core-walk half
                // is exact.
                if flying {
                    if let Some(core) = core
                        && let Some(cpos) = ctx.world.get::<Pos>(core).copied()
                    {
                        crate::ai::types::flying::move_to_point_keep_distance(
                            ctx.world,
                            ctx.grid,
                            ctx.content,
                            unit,
                            cpos.x,
                            cpos.y,
                            range * 0.5,
                            100.0,
                            true,
                            false,
                        );
                    }
                } else if let Some(core) = core
                    && let Some(cpos) = ctx.world.get::<Pos>(core).copied()
                {
                    ctx.pathfind(
                        unit,
                        TilePos::new(
                            WorldGrid::to_tile(cpos.x) as i16,
                            WorldGrid::to_tile(cpos.y) as i16,
                        ),
                        0.0,
                    );
                }
            }
        }
        LUnitControl::Stop => {
            // `unit.clearBuilding()`.
            if let Some(mut plans) = ctx.world.get_mut::<BuilderPlans>(unit) {
                plans.plans.clear();
            }
        }
        _ => {}
    }

    // `if(unit.type.canBoost && !unit.type.flying)` elevation lerp. The port
    // applies the same `shouldBoost = boost || unit.onSolid()` rule in
    // `movement::update_kinematics` (`UnitComp.updateBoosting`), so mirror
    // `LogicAI.boost` into `UnitCore.boosting` and let the shared pass lift.
    let def = ctx
        .world
        .get::<UnitTypeComp>(unit)
        .and_then(|comp| ctx.content.unit(comp.type_id));
    if let Some(def) = def.as_ref()
        && def.can_boost
        && !def.flying
        && let Some(mut core) = ctx.world.get_mut::<UnitCore>(unit)
    {
        core.boosting = state.boost;
    }

    // `if(!shoot || !unit.type.omniMovement) unit.lookAt(unit.prefRotation());
    //  else if(unit.hasWeapons() && mounts.length > 0 &&
    //  !mounts[0].weapon.ignoreRotation) unit.lookAt(mounts[0].aimX, aimY);`
    let omni = def.as_ref().is_none_or(|def| def.omni_movement);
    if !state.shoot || !omni {
        let angle = pref_rotation(ctx, unit);
        if let Some(mut core) = ctx.world.get_mut::<UnitCore>(unit) {
            core.rotation = angle;
        }
    } else {
        let ignore_rotation = def
            .as_ref()
            .and_then(|def| def.weapons.first())
            .is_some_and(|weapon| weapon.ignore_rotation);
        let mounts = ctx.world.get::<UnitWeapons>(unit);
        if let Some(mounts) = mounts
            && !mounts.mounts.is_empty()
            && !ignore_rotation
        {
            let (ax, ay) = (mounts.mounts[0].aim_x, mounts.mounts[0].aim_y);
            let angle = ctx
                .world
                .get::<Pos>(unit)
                .map(|pos| (ay - pos.y).atan2(ax - pos.x).to_degrees());
            if let (Some(angle), Some(mut core)) = (angle, ctx.world.get_mut::<UnitCore>(unit)) {
                core.rotation = angle;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn logic_expires_after_control_timeout() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let processor = harness.spawn("dagger", 0, 100.0, 100.0, 0.0).expect("p2");
        let mut control_path = crate::ai::ControlPathfinder::new(32, 32);
        let mut state = LogicAi::new(Some(processor));
        let mut alive = true;
        for _ in 0..(LOGIC_TIMEOUT_TICKS as i32) {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            alive = update_logic(&mut ctx, unit, &mut state, &mut control_path);
        }
        assert!(alive, "controller still runs on the timeout tick itself");
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        assert!(
            !update_logic(&mut ctx, unit, &mut state, &mut control_path),
            "logic controller invalidates after timeout"
        );
    }

    #[test]
    fn logic_expires_without_a_controller() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let mut control_path = crate::ai::ControlPathfinder::new(32, 32);
        let mut state = LogicAi::new(None);
        let mut ctx = AiCtx {
            world: &mut harness.build.world,
            grid: &harness.build.grid,
            content: &harness.build.content,
            pathfinder: &mut harness.pathfinder,
            team: 0,
        };
        assert!(!update_logic(&mut ctx, unit, &mut state, &mut control_path));
    }

    #[test]
    fn check_target_timer_dedupes_within_period() {
        let mut state = LogicAi::default();
        assert!(state.check_target_timer(7));
        assert!(!state.check_target_timer(7), "same instruction is cached");
        assert!(state.check_target_timer(8));
        state.target_timer = 0.0;
        state.radars.clear();
        assert!(state.check_target_timer(7));
    }
}
