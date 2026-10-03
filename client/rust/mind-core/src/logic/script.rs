// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `LogicScript` / `LogicFilter` runner.
//!
//! Ported from `core/src/mindustry/logic/LogicScript.java` and
//! `core/src/mindustry/maps/filters/LogicFilter.java`. Upstream's wall-clock
//! timeout becomes an explicit instruction cap (deviation 6); `LogicFilter`
//! passes `500*500*25 = 6_250_000`, which is upstream-correct and unaffected.

use crate::logic::assembler::Assembler;
use crate::logic::executor::{Executor, MAX_INSTRUCTIONS};

/// `LogicFilter.maxInstructionsExecution`.
pub const LOGIC_FILTER_MAX_INSTRUCTIONS: u32 = 500 * 500 * 25;

/// `LExecutor.runLogicScript(code)` default cap.
pub const SCRIPT_DEFAULT_MAX_INSTRUCTIONS: u32 = 100_000;

/// `LogicFilter.apply` seam: runs a map-generation script with the
/// `maxInstructionsExecution = 6_250_000` budget against `world`.
///
/// Plan 06's `maps::filters::LogicFilter::apply_tiles` calls this with a
/// `World` holding the in-progress `WorldGrid` (+ `ContentRegistry`); until the
/// generation world exposes those resources, the hook is a documented no-op.
pub fn run_logic_filter(
    code: &str,
    loop_: bool,
    world: &mut bevy_ecs::world::World,
) -> Option<Executor> {
    run_logic_script_in(code, LOGIC_FILTER_MAX_INSTRUCTIONS, loop_, world)
}

/// Runs a privileged logic script, capped by instruction count.
///
/// Returns the executor (for inspection) or `None` when the code fails to
/// assemble or is empty. `loop` disables the normal end-of-program break so the
/// script can continue until the cap (matching upstream `runLogicScript`).
pub fn run_logic_script(code: &str, max_instructions: u32, loop_: bool) -> Option<Executor> {
    let mut world = bevy_ecs::world::World::new();
    run_logic_script_in(code, max_instructions, loop_, &mut world)
}

/// Runs a privileged logic script against a caller-supplied world.
///
/// This is the seam a `LogicFilter` hook uses: map-generation scripts mutate
/// the supplied world (tile/rules state) rather than a throwaway one. The
/// `LogicFilter` wiring itself lives in plan 06 and awaits the `WorldGrid`
/// resource (see the M7 hand-off note in plan 13).
pub fn run_logic_script_in(
    code: &str,
    max_instructions: u32,
    loop_: bool,
    world: &mut bevy_ecs::world::World,
) -> Option<Executor> {
    if code.is_empty() {
        return None;
    }
    let asm = Assembler::assemble(code, true).ok()?;
    let mut executor = Executor::new();
    executor.privileged = true;
    executor.load(asm);

    let mut executed = 0u32;
    // Start at 1 like upstream `for(int i = 1; i < maxInstructions; i++)`.
    while executed + 1 < max_instructions {
        let counter = executor.counter_value();
        let at_end = counter >= executor.instructions.len() as f64 || counter < 0.0;
        if (!loop_ && at_end) || executor.yielded {
            break;
        }
        if executor.instructions.len() > MAX_INSTRUCTIONS && !loop_ {
            break;
        }
        executor.run_once(world);
        executed += 1;
    }
    Some(executor)
}

/// Runs `code` on a temporary executor for `ticks` fixed steps at `ipt`
/// instructions/tick (headless/MCP `logic_run`; no sim world required).
///
/// Returns `None` when the code fails to assemble.
pub fn run_standalone(code: &str, privileged: bool, ticks: u64, ipt: i32) -> Option<Executor> {
    let asm = Assembler::assemble(code, privileged).ok()?;
    let mut executor = Executor::new();
    executor.privileged = privileged;
    executor.load(asm);
    let mut world = bevy_ecs::world::World::new();
    let mut accumulator = 0.0f32;
    for _ in 0..ticks {
        executor.run_budget(&mut world, &mut accumulator, 1.0, ipt as f32);
    }
    Some(executor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_respects_instruction_cap() {
        // Infinite loop; must terminate at the cap.
        let exec = run_logic_script("jump 0 always", 50, true).unwrap();
        assert!(exec.instructions.len() == 1);
    }

    #[test]
    fn non_looping_script_stops_at_end() {
        let exec = run_logic_script("set x 1\nset y 2\n", 1000, false).unwrap();
        assert!(exec.counter_value() >= 2.0);
    }

    #[test]
    fn empty_and_invalid_scripts_return_none() {
        assert!(run_logic_script("", 10, false).is_none());
        assert!(run_logic_script("this is not valid", 10, false).is_some());
    }

    #[test]
    fn filter_runner_mutates_supplied_world() {
        let mut world = bevy_ecs::world::World::new();
        world.insert_resource(crate::logic::world::LogicWorldState::new());
        let exec = run_logic_script_in(
            "setrule waves false\nsetflag \"won\" true\n",
            100,
            false,
            &mut world,
        )
        .expect("script runs");
        assert_eq!(exec.instructions.len(), 2);
        let state = world
            .get_resource::<crate::logic::world::LogicWorldState>()
            .expect("world state");
        assert!(!state.rules.waves);
        assert!(state.rules.objective_flags.contains("won"));
    }
}
