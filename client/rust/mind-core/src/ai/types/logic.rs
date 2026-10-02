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

/// `LogicAI.timeout` at 60 Hz: 10 seconds (`Time.toSeconds(10)`).
pub const LOGIC_TIMEOUT_TICKS: f32 = 600.0;

/// `LogicAI` controller state.
#[derive(Debug, Clone, Copy, PartialEq)]
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
}

impl Default for LogicAi {
    fn default() -> Self {
        Self {
            unit: None,
            building: i32::MIN,
            timeout: LOGIC_TIMEOUT_TICKS,
            controlled: false,
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
}
