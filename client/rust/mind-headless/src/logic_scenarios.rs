// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `mind-headless logic` scenarios (plan 13 §7b).
//!
//! These run the mlog VM on a temporary privileged executor — no world/entities
//! required — so `logic_arith`/`logic_strings`/`logic_budget`/`logic_globals`
//! are deterministic and golden-checkable in plain CI.

use std::path::Path;

use anyhow::{Context, Result};
use mind_core::logic::assembler::Assembler;
use mind_core::logic::executor::Executor;
use mind_core::logic::statement::Statement;

use crate::cli::LogicCommand;

/// Registered headless scenarios.
pub struct Scenario {
    /// Name.
    pub name: &'static str,
    /// Program source.
    pub code: &'static str,
    /// Instructions per tick.
    pub ipt: i32,
    /// Privileged executor.
    pub privileged: bool,
    /// Default ticks.
    pub ticks: u64,
}

/// All plan-13 headless scenarios.
#[rustfmt::skip]
pub const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "logic_arith",
        code: "set a 2\nop add a a 3\nop mul b a 4\nop sub c b a\n",
        ipt: 2,
        privileged: true,
        ticks: 3,
    },
    Scenario {
        name: "logic_strings",
        code: "print \"value: \"\nprint 5\nprintchar 33\nend\n",
        ipt: 2,
        privileged: true,
        ticks: 3,
    },
    Scenario {
        name: "logic_budget",
        code: "op add n n 1\njump 0 always\n",
        ipt: 2,
        privileged: true,
        ticks: 600,
    },
    Scenario {
        name: "logic_globals",
        code: "set pi @pi\nset e @e\nset ctrl @ctrlProcessor\n",
        ipt: 2,
        privileged: true,
        ticks: 3,
    },
];

/// Finds a scenario by name.
pub fn scenario(name: &str) -> Option<&'static Scenario> {
    SCENARIOS.iter().find(|s| s.name == name)
}

/// Runs a scenario for `ticks` at 60 Hz and returns the executor.
pub fn run_scenario(scenario: &Scenario, ticks: u64) -> Executor {
    let asm = match Assembler::assemble(scenario.code, scenario.privileged) {
        Ok(asm) => asm,
        Err(_) => return Executor::new(),
    };
    let mut exec = Executor::new();
    exec.privileged = scenario.privileged;
    exec.load(asm);
    let ipt = scenario.ipt as f32;
    // `edelta` is `Time.delta * 60` (1.0 at the fixed 60 Hz step). Upstream bumps
    // the accumulator only after the run loop, so the first tick executes nothing.
    let edelta = 1.0f32;
    let mut accumulator = 0.0f32;
    for _ in 0..ticks {
        exec.run_budget(&mut accumulator, edelta, ipt);
    }
    exec
}

/// Deterministic checksum over non-constant variables and buffers (FNV-1a).
pub fn checksum(exec: &Executor) -> String {
    let mut hasher = mind_core::determinism::Hasher::new();
    for cell in &exec.arena.cells {
        if cell.constant {
            continue;
        }
        hasher.write(cell.name.as_bytes());
        hasher.write_u8(u8::from(cell.is_obj));
        if cell.is_obj {
            match &cell.obj {
                None => hasher.write_u8(0),
                Some(obj) => {
                    hasher.write_u8(1);
                    hasher.write(obj.display().as_bytes());
                }
            }
        } else {
            hasher.write_f64(cell.num);
        }
    }
    hasher.write(exec.text_buffer.as_bytes());
    for value in &exec.graphics_buffer {
        hasher.write_u64(*value);
    }
    hasher.finish().to_hex()
}

fn var_report(exec: &Executor) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for cell in &exec.arena.cells {
        if cell.constant {
            continue;
        }
        let value = if cell.is_obj {
            match &cell.obj {
                None => serde_json::Value::Null,
                Some(obj) => serde_json::Value::String(obj.display()),
            }
        } else if cell.num.fract() == 0.0 && cell.num.is_finite() {
            serde_json::json!(cell.num as i64)
        } else {
            serde_json::json!(cell.num)
        };
        map.insert(cell.name.clone(), value);
    }
    serde_json::Value::Object(map)
}

/// Executes a `logic` subcommand.
pub fn run(command: &LogicCommand) -> Result<i32> {
    match command {
        LogicCommand::Assemble {
            file,
            out,
            privileged,
            json,
        } => assemble(file, out.as_deref(), *privileged, *json),
        LogicCommand::Run { name, ticks, json } => run_named(name, *ticks, *json),
        LogicCommand::Dump { file, json } => dump(file, *json),
    }
}

fn assemble(file: &Path, out: Option<&Path>, privileged: bool, json: bool) -> Result<i32> {
    let source =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let statements = Assembler::read(&source, privileged)?;
    let normalized = Assembler::write(&statements);
    if let Some(out) = out {
        std::fs::write(out, normalized.as_bytes())
            .with_context(|| format!("writing {}", out.display()))?;
    }
    if json {
        println!(
            "{}",
            serde_json::json!({
                "file": file.display().to_string(),
                "statements": statements.len(),
                "normalized": normalized,
            })
        );
    } else {
        print!("{normalized}");
    }
    Ok(0)
}

fn run_named(name: &str, ticks: Option<u64>, json: bool) -> Result<i32> {
    let scenario = scenario(name).with_context(|| format!("unknown logic scenario: {name}"))?;
    let ticks = ticks.unwrap_or(scenario.ticks);
    let exec = run_scenario(scenario, ticks);
    let checksum = self::checksum(&exec);
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": name,
                "ticks": ticks,
                "ipt": scenario.ipt,
                "checksum": checksum,
                "vars": var_report(&exec),
                "text": exec.text_buffer,
                "graphics": exec.graphics_buffer.len(),
            })
        );
    } else {
        println!("{name}: checksum={checksum}");
        println!("  text={:?}", exec.text_buffer);
    }
    Ok(0)
}

fn dump(file: &Path, json: bool) -> Result<i32> {
    let source =
        std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
    let statements = Assembler::read(&source, true)?;
    if json {
        let rows: Vec<serde_json::Value> = statements
            .iter()
            .map(|statement: &Statement| {
                serde_json::json!({
                    "name": statement.registered_name(),
                    "fields": statement
                        .fields()
                        .iter()
                        .map(|field| serde_json::json!({"name": field.name, "kind": format!("{:?}", field.kind)}))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        println!("{}", serde_json::json!({ "statements": rows }));
    } else {
        for statement in &statements {
            println!("{}", statement.registered_name());
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenarios_are_deterministic() {
        for scenario in SCENARIOS {
            let first = run_scenario(scenario, scenario.ticks);
            let second = run_scenario(scenario, scenario.ticks);
            assert_eq!(
                checksum(&first),
                checksum(&second),
                "scenario {} is not deterministic",
                scenario.name
            );
        }
    }

    #[test]
    fn arith_scenario_values() {
        let scenario = scenario("logic_arith").unwrap();
        let exec = run_scenario(scenario, scenario.ticks);
        let value = |name: &str| {
            exec.optional_var(name)
                .map(|id| exec.arena.get(id).num())
                .unwrap_or(f64::NAN)
        };
        assert_eq!(value("a"), 5.0);
        assert_eq!(value("b"), 20.0);
        assert_eq!(value("c"), 15.0);
    }

    #[test]
    fn strings_scenario_buffer() {
        let scenario = scenario("logic_strings").unwrap();
        let exec = run_scenario(scenario, scenario.ticks);
        assert_eq!(exec.text_buffer, "value: 5!");
    }

    #[test]
    fn scenario_goldens_match() {
        // Committed scenario checksums (plan 13 §7b); regenerate deliberately.
        let goldens = [
            ("logic_arith", "13f8b1eccdfff995"),
            ("logic_strings", "7458e26c1c008d74"),
            ("logic_budget", "2eb20edbd2660fe4"),
            ("logic_globals", "21d7c51ee221bb5f"),
        ];
        for (name, expected) in goldens {
            let scenario = scenario(name).unwrap();
            let exec = run_scenario(scenario, scenario.ticks);
            assert_eq!(checksum(&exec), expected, "golden mismatch for {name}");
        }
    }
}
