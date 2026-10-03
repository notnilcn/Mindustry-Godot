// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicAI` (plan 11 §4.2). The logic controller keeps its state across saves
//! (`keepState = true`) and resets itself after [`LOGIC_TIMEOUT_TICKS`] without a
//! live logic processor driving it (upstream `LogicAI` timeout semantics).
//!
//! Ported from `core/src/mindustry/ai/types/LogicAI.java`. Plan 13 owns the
//! `LUnitControl` setter bridge; this module owns the timer/reset behavior and
//! exposes the public state plan 13 writes.

use bevy_ecs::entity::Entity;

use super::super::ai_controller::AiCtx;
use crate::logic::enums::LUnitControl;
use crate::world::TilePos;
use crate::world::plan::BuildPlan;

/// `LogicAI.timeout` at 60 Hz: 10 seconds (`Time.toSeconds(10)`).
pub const LOGIC_TIMEOUT_TICKS: f32 = 600.0;

/// `LogicAI` controller state.
///
/// The `LUnitControl` setter surface (plan 13's `unit_control`) writes these
/// fields; the movement/behaviour body reads them. `target_id` holds the
/// upstream `Teamc` object id (unit or building) because the bridge passes a
/// numeric id, not an ECS handle.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicAi {
    /// Bound unit.
    pub unit: Option<Entity>,
    /// Building position driving the controller (`controller.pos()`; `i32::MIN`
    /// = none).
    pub building: i32,
    /// Ticks left before the controller invalidates itself.
    pub timeout: f32,
    /// Whether a processor is currently driving it.
    pub controlled: bool,
    /// `targetPos` (world pixels).
    pub target_pos: (f32, f32),
    /// `target` object id (`-1` = none).
    pub target_id: i32,
    /// `itemTarget` (world pixels).
    pub item_target: (f32, f32),
    /// `itemDropTarget` (world pixels).
    pub item_drop_target: (f32, f32),
    /// `itemAmount`.
    pub item_amount: i32,
    /// `unit.flag`.
    pub flag: i32,
    /// `boost`.
    pub boost: bool,
    /// `moving`.
    pub moving: bool,
    /// `mineTile` (packed `TilePos`; `i32::MIN` = none).
    pub mine_tile: i32,
    /// `buildPlan` (place or deconstruct).
    pub build_plan: Option<BuildPlan>,
}

impl Default for LogicAi {
    fn default() -> Self {
        Self {
            unit: None,
            building: i32::MIN,
            timeout: LOGIC_TIMEOUT_TICKS,
            controlled: false,
            target_pos: (0.0, 0.0),
            target_id: -1,
            item_target: (0.0, 0.0),
            item_drop_target: (0.0, 0.0),
            item_amount: 0,
            flag: 0,
            boost: false,
            moving: false,
            mine_tile: i32::MIN,
            build_plan: None,
        }
    }
}

impl LogicAi {
    /// Creates a logic controller bound to `unit` and processor `building`.
    pub fn new(unit: Entity, building: i32) -> Self {
        Self {
            unit: Some(unit),
            building,
            ..Self::default()
        }
    }

    /// Marks the controller as driven and refreshes the timeout
    /// (`LogicAI.control`).
    pub fn control(&mut self) {
        self.controlled = true;
        self.timeout = LOGIC_TIMEOUT_TICKS;
    }

    /// `LogicAI.checkTargetTimer`-adjacent expiration test.
    pub fn expired(&self) -> bool {
        self.timeout <= 0.0
    }

    /// Sets the packet-position fields (`LUnitControl` move family).
    pub fn set_target_pos(&mut self, x: f32, y: f32) {
        self.target_pos = (x, y);
        self.target_id = -1;
        self.moving = true;
    }

    /// Applies one `LUnitControl` setter (plan 13's `UnitControlI` bridge).
    ///
    /// `params` are the instruction operands in upstream order. Returns `false`
    /// only for `unbind` (the controller should be released); the caller owns
    /// the `Rules.logicUnitControl`/`logicUnitBuild` gating.
    pub fn apply_control(&mut self, control: LUnitControl, params: &[f64]) -> bool {
        let f = |i: usize| params.get(i).copied().unwrap_or(0.0) as f32;
        let id = |i: usize| params.get(i).copied().unwrap_or(-1.0) as i32;
        match control {
            LUnitControl::Idle | LUnitControl::Stop => {
                self.moving = false;
                self.target_id = -1;
            }
            LUnitControl::Move | LUnitControl::Approach | LUnitControl::Pathfind => {
                self.set_target_pos(f(0), f(1));
            }
            LUnitControl::AutoPathfind => {
                // Upstream picks the nearest enemy core; the target position is
                // resolved by the movement body from `Rules`.
                self.moving = true;
            }
            LUnitControl::Boost => self.boost = f(0) != 0.0,
            LUnitControl::Flag => self.flag = id(0),
            LUnitControl::Target => self.target_id = id(0),
            LUnitControl::Targetp => self.set_target_pos(f(0), f(1)),
            LUnitControl::ItemDrop => self.item_drop_target = (f(0), f(1)),
            LUnitControl::ItemTake => self.item_target = (f(0), f(1)),
            LUnitControl::PayDrop | LUnitControl::PayTake | LUnitControl::PayEnter => {
                self.item_amount = id(2).max(0);
            }
            LUnitControl::Mine => {
                self.mine_tile = TilePos::new(f(0) as i16, f(1) as i16).pack();
            }
            LUnitControl::Build => {
                self.build_plan = Some(BuildPlan::place(
                    f(0) as i32,
                    f(1) as i32,
                    f(2) as u8,
                    crate::content::BlockId::new(id(3).max(0) as u16),
                ));
            }
            LUnitControl::Deconstruct => {
                self.build_plan = Some(BuildPlan::break_plan(f(0) as i32, f(1) as i32));
            }
            LUnitControl::Within | LUnitControl::GetBlock => {}
            LUnitControl::Unbind => {
                self.controlled = false;
                self.moving = false;
                self.target_id = -1;
                return false;
            }
        }
        true
    }
}

/// `LogicAI.updateUnit`: count the timeout down; the unit idles while driven.
///
/// Returns `false` once the controller should be reset (caller re-evaluates
/// selection), matching upstream's `invalid` controller path.
pub fn update_logic(ctx: &mut AiCtx, unit: Entity, state: &mut LogicAi) -> bool {
    state.timeout -= 1.0;
    if state.expired() {
        ctx.stop_shooting(unit);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::ai::ai_controller::AiCtx;

    #[test]
    fn logic_expires_after_timeout() {
        let mut harness = UnitHarness::new(32, 32, 1);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let mut state = LogicAi::new(unit, i32::MIN);
        let mut alive = true;
        for _ in 0..(LOGIC_TIMEOUT_TICKS as i32) {
            let mut ctx = AiCtx {
                world: &mut harness.build.world,
                grid: &harness.build.grid,
                content: &harness.build.content,
                pathfinder: &mut harness.pathfinder,
                team: 0,
            };
            alive = update_logic(&mut ctx, unit, &mut state);
        }
        assert!(!alive, "logic controller invalidates after timeout");
        // `control()` refreshes it.
        state.control();
        assert!(!state.expired());
    }

    #[test]
    fn apply_control_sets_movement_and_flag() {
        let mut state = LogicAi::default();
        assert!(state.apply_control(LUnitControl::Move, &[10.0, 20.0]));
        assert_eq!(state.target_pos, (10.0, 20.0));
        assert!(state.moving);
        assert!(state.apply_control(LUnitControl::Flag, &[7.0]));
        assert_eq!(state.flag, 7);
        assert!(state.apply_control(LUnitControl::Stop, &[]));
        assert!(!state.moving);
        assert_eq!(state.target_id, -1);
    }

    #[test]
    fn apply_control_build_and_mine_and_unbind() {
        let mut state = LogicAi::default();
        assert!(state.apply_control(LUnitControl::Mine, &[3.0, 4.0]));
        assert_eq!(state.mine_tile, TilePos::new(3, 4).pack());
        assert!(state.apply_control(LUnitControl::Build, &[5.0, 6.0, 1.0, 2.0]));
        let plan = state.build_plan.as_ref().expect("build plan");
        assert_eq!((plan.x, plan.y, plan.rotation), (5, 6, 1));
        assert!(state.apply_control(LUnitControl::Deconstruct, &[8.0, 9.0]));
        assert!(state.build_plan.as_ref().expect("break plan").breaking);
        assert!(!state.apply_control(LUnitControl::Unbind, &[]));
        assert!(!state.controlled);
    }
}
