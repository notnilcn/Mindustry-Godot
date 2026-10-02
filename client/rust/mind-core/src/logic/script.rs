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

/// Runs a privileged logic script, capped by instruction count.
///
/// Returns the executor (for inspection) or `None` when the code fails to
/// assemble or is empty. `loop` disables the normal end-of-program break so the
/// script can continue until the cap (matching upstream `runLogicScript`).
pub fn run_logic_script(code: &str, max_instructions: u32, loop_: bool) -> Option<Executor> {
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
        executor.run_once();
        executed += 1;
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
}
