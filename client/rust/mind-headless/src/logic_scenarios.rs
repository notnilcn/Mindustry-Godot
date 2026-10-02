// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `mind-headless logic` scenarios (plan 13 §7b).
//!
//! These run the mlog VM on a temporary privileged executor — no world/entities
//! required — so `logic_arith`/`logic_strings`/`logic_budget`/`logic_globals`
//! are deterministic and golden-checkable in plain CI.

use std::path::Path;

use anyhow::{Context, Result};
use mind_core::io::wire::{WireReader, WireWriter};
use mind_core::logic::assembler::Assembler;
use mind_core::logic::blocks::io::CellValue;
use mind_core::logic::blocks::logic_block::compress;
use mind_core::logic::blocks::{
    LogicBlockState, LogicDisplayState, LogicRulesApi, LogicRulesRes, MemoryBlockState,
};
use mind_core::logic::executor::Executor;
use mind_core::logic::statement::Statement;
use mind_core::logic::world::{LogicWorldEvent, LogicWorldState};
use mind_core::world::ConfigValue;
use mind_core::world::building_io::BuildingCodec;
use mind_core::world::harness::BuildHarness;

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
    let mut world = bevy_ecs::world::World::new();
    for _ in 0..ticks {
        exec.run_budget(&mut world, &mut accumulator, edelta, ipt);
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

/// `logic_link_sensor` report.
pub struct LinkSensorReport {
    /// Deterministic checksum.
    pub checksum: String,
    /// Cell slots 0 and 1.
    pub memory: [CellValue; 2],
    /// `r` variable value.
    pub result: f64,
    /// Valid link count.
    pub links: u64,
}

/// Runs the link/sensor scenario.
pub fn link_sensor(ticks: u64) -> Result<LinkSensorReport> {
    let mut harness = BuildHarness::new(16, 16, 1);
    let processor = harness
        .content()
        .block_id("micro-processor")
        .context("micro-processor content")?;
    let memory = harness
        .content()
        .block_id("memory-cell")
        .context("memory-cell content")?;
    assert!(harness.place(4, 4, processor, 0, true));
    assert!(harness.place(5, 4, memory, 0, true));
    let pe = harness.build_at(4, 4).context("processor entity")?;
    let me = harness.build_at(5, 4).context("memory entity")?;

    let code =
        "write 123 cell1 0\nwrite 1 cell1 1\nread r cell1 0\nop add r r 1\nwrite r cell1 1\nstop\n";
    assert!(harness.configure(4, 4, ConfigValue::Bytes(compress(code, &[]).into())));
    assert!(harness.configure(4, 4, ConfigValue::Point2(5, 4)));
    for _ in 0..ticks {
        harness.tick();
    }

    let state = harness
        .world
        .get::<LogicBlockState>(pe)
        .context("processor state")?;
    let mem_state = harness
        .world
        .get::<MemoryBlockState>(me)
        .context("memory state")?;
    let mem0 = mem_state.read(0);
    let mem1 = mem_state.read(1);
    let result = state
        .executor
        .optional_var("r")
        .map(|id| state.executor.arena.get(id).num())
        .unwrap_or(f64::NAN);
    let links = state.executor.links.len() as u64;

    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_f64(mem0.num());
    hasher.write_u8(u8::from(mem0.is_obj()));
    hasher.write_f64(mem1.num());
    hasher.write_u8(u8::from(mem1.is_obj()));
    hasher.write_f64(result);
    hasher.write_u64(links);
    let checksum = hasher.finish().to_hex();
    Ok(LinkSensorReport {
        checksum,
        memory: [mem0, mem1],
        result,
        links,
    })
}

/// Prints the link/sensor scenario.
fn run_link_sensor(ticks: u64, json: bool) -> Result<i32> {
    let report = link_sensor(ticks)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": "logic_link_sensor",
                "ticks": ticks,
                "checksum": report.checksum,
                "memory": [cell_json(&report.memory[0]), cell_json(&report.memory[1])],
                "result": report.result,
                "links": report.links,
            })
        );
    } else {
        println!(
            "logic_link_sensor: checksum={} memory=({:?}, {:?}) result={} links={}",
            report.checksum, report.memory[0], report.memory[1], report.result, report.links
        );
    }
    Ok(0)
}

/// `logic_save_load` report.
pub struct SaveLoadReport {
    /// Deterministic checksum.
    pub checksum: String,
    /// `done` variable value.
    pub done: f64,
    /// Accumulator after the final run.
    pub accumulator: f32,
    /// Restored code.
    pub code: String,
}

/// Runs the save/load scenario.
pub fn save_load(ticks: u64) -> Result<SaveLoadReport> {
    let mut harness = BuildHarness::new(16, 16, 1);
    let processor = harness
        .content()
        .block_id("micro-processor")
        .context("micro-processor content")?;
    assert!(harness.place(4, 4, processor, 0, true));
    let pe = harness.build_at(4, 4).context("processor entity")?;

    let code = "wait 1.5\nset done 1\n";
    assert!(harness.configure(4, 4, ConfigValue::Bytes(compress(code, &[]).into())));
    for _ in 0..ticks {
        harness.tick();
    }

    let mut buf = Vec::new();
    {
        let mut writer = WireWriter::new(&mut buf);
        BuildingCodec::write(&harness.world, pe, &mut writer, false)?;
    }

    assert!(harness.place(8, 8, processor, 0, true));
    let pe2 = harness.build_at(8, 8).context("processor entity 2")?;
    let mut reader = WireReader::new(&buf);
    BuildingCodec::read(&mut harness.world, pe2, &mut reader, 3)?;

    // Run long enough for the preserved wait plus its remainder.
    for _ in 0..(ticks + 200) {
        harness.tick();
    }

    let state = harness
        .world
        .get::<LogicBlockState>(pe2)
        .context("processor state 2")?;
    let done = state
        .executor
        .optional_var("done")
        .map(|id| state.executor.arena.get(id).num())
        .unwrap_or(f64::NAN);
    let accumulator = state.accumulator;

    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_f64(done);
    hasher.write_f32(accumulator);
    hasher.write(state.code.as_bytes());
    let checksum = hasher.finish().to_hex();
    Ok(SaveLoadReport {
        checksum,
        done,
        accumulator,
        code: state.code.clone(),
    })
}

/// Prints the save/load scenario.
fn run_save_load(ticks: u64, json: bool) -> Result<i32> {
    let report = save_load(ticks)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": "logic_save_load",
                "ticks": ticks,
                "checksum": report.checksum,
                "done": report.done,
                "accumulator": report.accumulator,
                "code": report.code,
            })
        );
    } else {
        println!(
            "logic_save_load: checksum={} done={}",
            report.checksum, report.done
        );
    }
    Ok(0)
}

/// `logic_draw`/`logic_draw_headless` report.
pub struct DrawReport {
    /// Deterministic checksum.
    pub checksum: String,
    /// Command count.
    pub commands: usize,
    /// `operations` counter.
    pub operations: u64,
    /// Text buffer (should be empty after `drawflush`).
    pub text: String,
}

/// Runs the draw scenario (`capture` disables the headless skip flag).
pub fn draw_report(capture: bool, ticks: u64) -> Result<DrawReport> {
    let mut harness = BuildHarness::new(16, 16, 1);
    let processor = harness
        .content()
        .block_id("micro-processor")
        .context("micro-processor content")?;
    let display = harness
        .content()
        .block_id("logic-display")
        .context("logic-display content")?;
    assert!(harness.place(4, 4, processor, 0, true));
    assert!(harness.place(8, 4, display, 0, true));
    let pe = harness.build_at(4, 4).context("processor entity")?;
    let de = harness.build_at(8, 4).context("display entity")?;

    let code = "draw clear 0 0 0\ndraw col %ff0000\ndraw stroke 2\ndraw line 0 0 80 80\ndraw rect 10 10 20 20\ndraw poly 40 40 3 10 0\nprint \"hi\"\ndraw print 20 20 @center\ndraw rotate 45\ndraw scale 2 2\ndrawflush display1\nstop\n";
    assert!(harness.configure(4, 4, ConfigValue::Bytes(compress(code, &[]).into())));
    assert!(harness.configure(4, 4, ConfigValue::Point2(8, 4)));
    if !capture && let Some(mut state) = harness.world.get_mut::<LogicBlockState>(pe) {
        state.executor.skip_draw_pack = true;
    }
    for _ in 0..ticks {
        harness.tick();
    }

    let display_state = harness
        .world
        .get::<LogicDisplayState>(de)
        .context("display state")?;
    let commands = display_state.commands.len();
    let operations = display_state.operations;
    let text = harness
        .world
        .get::<LogicBlockState>(pe)
        .map(|state| state.executor.text_buffer.clone())
        .unwrap_or_default();

    let mut hasher = mind_core::determinism::Hasher::new();
    for command in &display_state.commands {
        hasher.write_u64(*command);
    }
    hasher.write_u64(operations);
    let checksum = hasher.finish().to_hex();
    Ok(DrawReport {
        checksum,
        commands,
        operations,
        text,
    })
}

/// `logic_sensor_access` report (M5: `sensor`/`control`/`setprop`).
pub struct SensorAccessReport {
    /// Deterministic checksum.
    pub checksum: String,
    /// `enabled` after `control enabled switch1 0`.
    pub disabled: f64,
    /// `enabled` after `control enabled switch1 1`.
    pub enabled: f64,
    /// `sensor @size`.
    pub size: f64,
    /// `sensor @type` object display, if any.
    pub type_name: String,
}

/// Runs the sensor/control scenario against a linked processor + switch.
pub fn sensor_access(ticks: u64) -> Result<SensorAccessReport> {
    let mut harness = BuildHarness::new(16, 16, 1);
    let processor = harness
        .content()
        .block_id("micro-processor")
        .context("micro-processor content")?;
    let switch = harness
        .content()
        .block_id("switch")
        .context("switch content")?;
    assert!(harness.place(4, 4, processor, 0, true));
    assert!(harness.place(5, 4, switch, 0, true));
    let pe = harness.build_at(4, 4).context("processor entity")?;

    let code = "control enabled switch1 0\nsensor disabled switch1 @enabled\ncontrol enabled switch1 1\nsensor enabled switch1 @enabled\nsensor size switch1 @size\nsensor type switch1 @type\nstop\n";
    assert!(harness.configure(4, 4, ConfigValue::Bytes(compress(code, &[]).into())));
    assert!(harness.configure(4, 4, ConfigValue::Point2(5, 4)));
    for _ in 0..ticks {
        harness.tick();
    }

    let state = harness
        .world
        .get::<LogicBlockState>(pe)
        .context("processor state")?;
    let value = |name: &str| {
        state
            .executor
            .optional_var(name)
            .map(|id| state.executor.arena.get(id).num())
            .unwrap_or(f64::NAN)
    };
    let type_name = state
        .executor
        .optional_var("type")
        .and_then(|id| state.executor.arena.get(id).obj.clone())
        .map(|obj| obj.display())
        .unwrap_or_default();
    let disabled = value("disabled");
    let enabled = value("enabled");
    let size = value("size");

    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_f64(disabled);
    hasher.write_f64(enabled);
    hasher.write_f64(size);
    hasher.write(type_name.as_bytes());
    let checksum = hasher.finish().to_hex();
    Ok(SensorAccessReport {
        checksum,
        disabled,
        enabled,
        size,
        type_name,
    })
}

/// Prints the sensor scenario.
fn run_sensor_access(ticks: u64, json: bool) -> Result<i32> {
    let report = sensor_access(ticks)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": "logic_sensor_access",
                "ticks": ticks,
                "checksum": report.checksum,
                "disabled": report.disabled,
                "enabled": report.enabled,
                "size": report.size,
                "type": report.type_name,
            })
        );
    } else {
        println!(
            "logic_sensor_access: checksum={} disabled={} enabled={} size={}",
            report.checksum, report.disabled, report.enabled, report.size
        );
    }
    Ok(0)
}

/// `logic_radar_filters` report (M5 radar filter/sort table).
pub struct RadarFiltersReport {
    /// Deterministic checksum.
    pub checksum: String,
    /// Enemy nearest entity index.
    pub enemy: Option<u32>,
    /// Ground enemy entity index.
    pub ground: Option<u32>,
    /// Farthest-by-health enemy entity index.
    pub farthest: Option<u32>,
}

/// Runs the radar filter scenario over a fixed candidate fixture.
#[allow(clippy::expect_used)]
pub fn radar_filters() -> RadarFiltersReport {
    use mind_core::logic::enums::{RadarSort, RadarTarget};
    use mind_core::logic::executor::radar::{self, RadarCandidate};
    let candidate = |entity: u32, team: u8, x: f32, flying: bool, health: f32| RadarCandidate {
        entity: bevy_ecs::entity::Entity::from_raw_u32(entity).expect("entity"),
        team,
        x,
        y: 0.0,
        hit_size: 1.0,
        health,
        max_health: health,
        shield: 0.0,
        armor: 0.0,
        flying,
        player: false,
        dead: false,
    };
    let derelict = mind_core::game::team::DERELICT.0;
    let list = vec![
        candidate(1, 0, 5.0, false, 10.0),
        candidate(2, derelict, 1.0, false, 10.0),
        candidate(3, 1, 8.0, true, 30.0),
        candidate(4, 1, 2.0, false, 40.0),
        candidate(5, 1, 20.0, false, 20.0),
    ];
    let any = [RadarTarget::Enemy, RadarTarget::Any, RadarTarget::Any];
    let enemy = radar::find(&list, 0, any, RadarSort::Distance, true, 0.0, 0.0);
    let ground = radar::find(
        &list,
        0,
        [RadarTarget::Enemy, RadarTarget::Ground, RadarTarget::Any],
        RadarSort::Distance,
        true,
        0.0,
        0.0,
    );
    let farthest = radar::find(&list, 0, any, RadarSort::Health, false, 0.0, 0.0);
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_u64(enemy.map(|e| e.index().index() as u64).unwrap_or(u64::MAX));
    hasher.write_u64(ground.map(|e| e.index().index() as u64).unwrap_or(u64::MAX));
    hasher.write_u64(
        farthest
            .map(|e| e.index().index() as u64)
            .unwrap_or(u64::MAX),
    );
    RadarFiltersReport {
        checksum: hasher.finish().to_hex(),
        enemy: enemy.map(|e| e.index().index()),
        ground: ground.map(|e| e.index().index()),
        farthest: farthest.map(|e| e.index().index()),
    }
}

/// Prints the radar filter scenario.
fn run_radar_filters(json: bool) -> Result<i32> {
    let report = radar_filters();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": "logic_radar_filters",
                "checksum": report.checksum,
                "enemy": report.enemy,
                "ground": report.ground,
                "farthest": report.farthest,
            })
        );
    } else {
        println!("logic_radar_filters: checksum={}", report.checksum);
    }
    Ok(0)
}

/// `logic_unit_control_gating` report (M5 `ubind`/`checkLogicAI` gating).
pub struct UnitGatingReport {
    /// Deterministic checksum.
    pub checksum: String,
    /// Whether `logicUnitControl=false` blocks installation.
    pub blocked: bool,
    /// Whether `logicUnitControl=true` installs the logic controller.
    pub installed: bool,
    /// Number of units the binding cursor can cycle.
    pub bind_count: usize,
}

/// Custom rules seam exercising the unit-control gates.
struct GatingRules {
    control: bool,
}

impl mind_core::logic::blocks::LogicRulesApi for GatingRules {
    fn editor(&self) -> bool {
        false
    }
    fn allow_edit_world_processors(&self) -> bool {
        false
    }
    fn disable_world_processors(&self) -> bool {
        false
    }
    fn logic_unit_control(&self) -> bool {
        self.control
    }
}

/// Runs the unit-control gating scenario.
#[allow(clippy::unwrap_used)]
pub fn unit_control_gating() -> UnitGatingReport {
    use bevy_ecs::world::World;
    use mind_core::ai::{AiKind, ControllerSlot};
    use mind_core::entities::comp::unit::{UnitCore, UnitTypeComp};
    use mind_core::entities::comp::{TeamComp, Unit};
    use mind_core::logic::blocks::LogicRulesRes;
    use mind_core::logic::executor::unit_control::{bind_next, check_logic_ai};

    let mut world = World::new();
    let unit = world
        .spawn((
            Unit,
            TeamComp { team: 0 },
            UnitCore::new(0.0),
            UnitTypeComp {
                type_id: mind_core::content::UnitTypeId::new(1),
            },
            ControllerSlot::new(AiKind::Ground),
        ))
        .id();

    // Gate closed: the VM skips `ubind`/`ucontrol` entirely (no install).
    world.insert_resource(LogicRulesRes(Box::new(GatingRules { control: false })));
    let gate = mind_core::logic::blocks::rules_ref(&world).logic_unit_control();
    let blocked = if !gate {
        true
    } else {
        check_logic_ai(&mut world, 0, false, unit, Some(unit), true).is_none()
    };
    // Reset the slot and retry with the gate open.
    world.get_mut::<ControllerSlot>(unit).unwrap().kind = AiKind::Ground;
    world.insert_resource(LogicRulesRes(Box::new(GatingRules { control: true })));
    let gate = mind_core::logic::blocks::rules_ref(&world).logic_unit_control();
    let installed = gate
        && check_logic_ai(&mut world, 0, false, unit, Some(unit), true).is_some()
        && world.get::<ControllerSlot>(unit).unwrap().kind == AiKind::Logic;

    let mut cursor = 0u32;
    let first = bind_next(&mut world, 1, 0, &mut cursor);
    let second = bind_next(&mut world, 1, 0, &mut cursor);
    let bind_count = [first, second].iter().filter(|u| u.is_some()).count();

    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_u8(u8::from(blocked));
    hasher.write_u8(u8::from(installed));
    hasher.write_u64(bind_count as u64);
    UnitGatingReport {
        checksum: hasher.finish().to_hex(),
        blocked,
        installed,
        bind_count,
    }
}

/// Prints the unit-control gating scenario.
fn run_unit_control_gating(json: bool) -> Result<i32> {
    let report = unit_control_gating();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": "logic_unit_control_gating",
                "checksum": report.checksum,
                "blocked": report.blocked,
                "installed": report.installed,
                "bind_count": report.bind_count,
            })
        );
    } else {
        println!("logic_unit_control_gating: checksum={}", report.checksum);
    }
    Ok(0)
}

/// Prints the draw scenario.
fn run_draw(capture: bool, ticks: u64, json: bool) -> Result<i32> {
    let name = if capture {
        "logic_draw"
    } else {
        "logic_draw_headless"
    };
    let report = draw_report(capture, ticks)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": name,
                "ticks": ticks,
                "capture": capture,
                "checksum": report.checksum,
                "commands": report.commands,
                "operations": report.operations,
                "text": report.text,
            })
        );
    } else {
        println!(
            "{name}: checksum={} commands={} operations={}",
            report.checksum, report.commands, report.operations
        );
    }
    Ok(0)
}

fn cell_json(cell: &CellValue) -> serde_json::Value {
    match cell {
        CellValue::Num(n) => {
            if n.fract() == 0.0 && n.is_finite() {
                serde_json::json!(*n as i64)
            } else {
                serde_json::json!(*n)
            }
        }
        CellValue::Obj(None) => serde_json::Value::Null,
        CellValue::Obj(Some(obj)) => serde_json::Value::String(obj.display()),
    }
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
    match name {
        "logic_link_sensor" => return run_link_sensor(ticks.unwrap_or(10), json),
        "logic_save_load" => return run_save_load(ticks.unwrap_or(20), json),
        "logic_draw" => return run_draw(true, ticks.unwrap_or(3), json),
        "logic_draw_headless" => return run_draw(false, ticks.unwrap_or(3), json),
        "logic_sensor_access" => return run_sensor_access(ticks.unwrap_or(20), json),
        "logic_radar_filters" => return run_radar_filters(json),
        "logic_unit_control_gating" => return run_unit_control_gating(json),
        "logic_privileged_world" => return run_privileged_world(ticks.unwrap_or(10), json),
        "logic_markers_smoke" => return run_markers_smoke(json),
        "logic_sync_event" => return run_sync_event(ticks.unwrap_or(13), json),
        _ => {}
    }
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

/// Rules seam that permits configuring a world processor in tests.
struct PermissiveRules;

impl LogicRulesApi for PermissiveRules {
    fn editor(&self) -> bool {
        true
    }
    fn allow_edit_world_processors(&self) -> bool {
        true
    }
    fn disable_world_processors(&self) -> bool {
        false
    }
}

/// Installs the world state + permissive rules and returns the processor entity.
fn world_processor_rig(
    code: &str,
    width: i32,
    height: i32,
) -> Result<(BuildHarness, bevy_ecs::entity::Entity)> {
    let mut harness = BuildHarness::new(width, height, 1);
    harness
        .world
        .insert_resource(LogicRulesRes(Box::new(PermissiveRules)));
    let mut state = LogicWorldState::new();
    state.map_width = width;
    state.map_height = height;
    harness.world.insert_resource(state);
    let processor = harness
        .content()
        .block_id("world-processor")
        .context("world-processor content")?;
    assert!(harness.place(4, 4, processor, 0, true));
    let pe = harness.build_at(4, 4).context("processor entity")?;
    assert!(harness.configure(4, 4, ConfigValue::Bytes(compress(code, &[]).into())));
    Ok((harness, pe))
}

/// `logic_privileged_world` report.
pub struct PrivilegedWorldReport {
    /// Deterministic checksum.
    pub checksum: String,
    /// `rules.waves` after `setrule`.
    pub waves: bool,
    /// `rules.waveSpacing` after `setrule`.
    pub wave_spacing: f32,
    /// `rules.objectiveFlags` contains `captured`.
    pub captured: bool,
    /// `rules.mission`.
    pub mission: String,
    /// `getblock block` result key.
    pub block_result: String,
    /// Event counts by kind.
    pub counts: Vec<(&'static str, usize)>,
}

/// Runs the privileged world-instruction scenario.
#[allow(clippy::expect_used)]
pub fn privileged_world(ticks: u64) -> Result<PrivilegedWorldReport> {
    let code = "print \"missiontext\"\n\
        message mission 3 @wait\n\
        setrule waves false\n\
        setrule waveSpacing 4 0 0 100 100\n\
        setflag \"captured\" true\n\
        getflag captured_flag \"captured\"\n\
        setblock block @copper-wall 7 7 0 0\n\
        getblock block block_result 6 6\n\
        spawn @dagger 40 40 90 0 spawned true\n\
        bullet bullet_result @dagger 0 40 40 0 0 null -1 1 1 -1 -1\n\
        spawnwave 10 10 false\n\
        effect warn 10 10 2 %ffaaff \"\"\n\
        explosion 0 40 40 3 20 true true false true\n\
        query circle unit 0 40 40 20 20\n\
        fetch unitCount fetch_result 0 0 @conveyor\n\
        playsound false @sfx-shoot 1 1 0 40 40 true\n\
        playmusic \"game1\" true\n\
        localeprint \"name\"\n\
        set x 5\n\
        sync x\n\
        stop\n";
    let (mut harness, pe) = world_processor_rig(code, 16, 16)?;
    for _ in 0..ticks {
        harness.tick();
    }

    let executor = &harness
        .world
        .get::<LogicBlockState>(pe)
        .context("processor state")?
        .executor;
    let block_result = executor
        .optional_var("block_result")
        .and_then(|id| executor.arena.get(id).obj.clone())
        .map(|obj| mind_core::logic::world::object_key(&obj))
        .unwrap_or_default();

    let state = harness
        .world
        .get_resource::<LogicWorldState>()
        .context("world state")?;
    let kinds = [
        "setblock",
        "spawn",
        "bullet",
        "spawnwave",
        "effect",
        "explosion",
        "query",
        "fetch",
        "playsound",
        "playmusic",
        "localeprint",
        "sync",
    ];
    let counts: Vec<(&'static str, usize)> = kinds
        .iter()
        .map(|kind| (*kind, state.count(kind)))
        .collect();

    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_u8(u8::from(state.rules.waves));
    hasher.write_f32(state.rules.wave_spacing);
    hasher.write_u8(u8::from(state.rules.objective_flags.contains("captured")));
    hasher.write(state.rules.mission.clone().unwrap_or_default().as_bytes());
    hasher.write(block_result.as_bytes());
    for (kind, count) in &counts {
        hasher.write(kind.as_bytes());
        hasher.write_u64(*count as u64);
    }
    Ok(PrivilegedWorldReport {
        checksum: hasher.finish().to_hex(),
        waves: state.rules.waves,
        wave_spacing: state.rules.wave_spacing,
        captured: state.rules.objective_flags.contains("captured"),
        mission: state.rules.mission.clone().unwrap_or_default(),
        block_result,
        counts,
    })
}

/// Prints the privileged world scenario.
fn run_privileged_world(ticks: u64, json: bool) -> Result<i32> {
    let report = privileged_world(ticks)?;
    let counts: serde_json::Map<String, serde_json::Value> = report
        .counts
        .iter()
        .map(|(kind, count)| ((*kind).to_owned(), serde_json::json!(*count)))
        .collect();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": "logic_privileged_world",
                "ticks": ticks,
                "checksum": report.checksum,
                "waves": report.waves,
                "waveSpacing": report.wave_spacing,
                "captured": report.captured,
                "mission": report.mission,
                "blockResult": report.block_result,
                "counts": counts,
            })
        );
    } else {
        println!(
            "logic_privileged_world: checksum={} waves={} captured={}",
            report.checksum, report.waves, report.captured
        );
    }
    Ok(0)
}

/// `logic_markers_smoke` report.
pub struct MarkersSmokeReport {
    /// Deterministic checksum.
    pub checksum: String,
    /// Marker count.
    pub size: usize,
    /// Marker 1 present.
    pub has_one: bool,
    /// World index vector contains marker 1.
    pub world_visible: bool,
    /// Point x (world pixels).
    pub x: f32,
    /// Point y (world pixels).
    pub y: f32,
    /// Point radius.
    pub radius: f32,
}

/// Runs the markers scenario (plan 12 `MapMarkers`).
#[allow(clippy::expect_used)]
pub fn markers_smoke() -> Result<MarkersSmokeReport> {
    let code = "makemarker shape 1 10 10 true\n\
        setmarker pos 1 3 4 0\n\
        setmarker radius 1 7 -1 -1\n\
        setmarker world 1 1 -1 -1\n\
        stop\n";
    let (mut harness, _pe) = world_processor_rig(code, 16, 16)?;
    for _ in 0..8 {
        harness.tick();
    }
    let state = harness
        .world
        .get_resource::<LogicWorldState>()
        .context("world state")?;
    let has_one = state.markers.has(1);
    let world_visible = state.markers.world_markers.contains(&1);
    let (x, y, radius) = match state.markers.get(1) {
        Some(mind_core::io::json::objectives::ObjectiveMarker::Shape(shape)) => {
            (shape.pos.x, shape.pos.y, shape.radius)
        }
        _ => (0.0, 0.0, 0.0),
    };
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_u64(state.markers.size() as u64);
    hasher.write_u8(u8::from(has_one));
    hasher.write_u8(u8::from(world_visible));
    hasher.write_f32(x);
    hasher.write_f32(y);
    hasher.write_f32(radius);
    Ok(MarkersSmokeReport {
        checksum: hasher.finish().to_hex(),
        size: state.markers.size(),
        has_one,
        world_visible,
        x,
        y,
        radius,
    })
}

/// Prints the markers scenario.
fn run_markers_smoke(json: bool) -> Result<i32> {
    let report = markers_smoke()?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": "logic_markers_smoke",
                "checksum": report.checksum,
                "size": report.size,
                "hasOne": report.has_one,
                "worldVisible": report.world_visible,
                "x": report.x,
                "y": report.y,
                "radius": report.radius,
            })
        );
    } else {
        println!(
            "logic_markers_smoke: checksum={} size={}",
            report.checksum, report.size
        );
    }
    Ok(0)
}

/// `logic_sync_event` report.
pub struct SyncEventReport {
    /// Deterministic checksum.
    pub checksum: String,
    /// `sync` event count.
    pub events: usize,
    /// First event variable name.
    pub var_name: String,
    /// First event object flag.
    pub is_obj: bool,
}

/// Runs the `sync` throttle scenario (deviation 4: every 3 ticks).
#[allow(clippy::expect_used)]
pub fn sync_event(ticks: u64) -> Result<SyncEventReport> {
    let code = "loop:\nop add x x 1\nsync x\njump loop always\n";
    let (mut harness, _pe) = world_processor_rig(code, 8, 8)?;
    for tick in 0..ticks {
        if let Some(mut state) = harness.world.get_resource_mut::<LogicWorldState>() {
            state.tick = tick;
        }
        harness.tick();
    }
    let state = harness
        .world
        .get_resource::<LogicWorldState>()
        .context("world state")?;
    let events = state.count("sync");
    let (var_name, is_obj) = state
        .events
        .iter()
        .find_map(|event| match event {
            LogicWorldEvent::Sync(sync) => Some((sync.var_name.clone(), sync.is_obj)),
            _ => None,
        })
        .unwrap_or_default();
    let mut hasher = mind_core::determinism::Hasher::new();
    hasher.write_u64(events as u64);
    hasher.write(var_name.as_bytes());
    hasher.write_u8(u8::from(is_obj));
    Ok(SyncEventReport {
        checksum: hasher.finish().to_hex(),
        events,
        var_name,
        is_obj,
    })
}

/// Prints the sync scenario.
fn run_sync_event(ticks: u64, json: bool) -> Result<i32> {
    let report = sync_event(ticks)?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scenario": "logic_sync_event",
                "ticks": ticks,
                "checksum": report.checksum,
                "events": report.events,
                "varName": report.var_name,
                "isObj": report.is_obj,
            })
        );
    } else {
        println!(
            "logic_sync_event: checksum={} events={}",
            report.checksum, report.events
        );
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
    fn harness_scenario_goldens() {
        let link = link_sensor(10).expect("link_sensor");
        assert_eq!(link.checksum, "ef79a8557eadddb4");
        assert_eq!(link.result, 124.0);
        assert_eq!(link.links, 1);
        assert_eq!(link.memory[0], CellValue::Num(123.0));
        assert_eq!(link.memory[1], CellValue::Num(124.0));

        let save = save_load(20).expect("save_load");
        assert_eq!(save.checksum, "8f4d87c924126091");
        assert_eq!(save.done, 1.0);
        assert_eq!(save.code, "wait 1.5\nset done 1\n");

        let draw = draw_report(true, 30).expect("logic_draw");
        assert_eq!(draw.checksum, "82572433be79fab9");
        assert_eq!(draw.commands, 10);
        assert_eq!(draw.operations, 1);
        assert_eq!(draw.text, "");

        let headless = draw_report(false, 30).expect("logic_draw_headless");
        assert_eq!(headless.checksum, "89cd31291d2aefa4");
        assert_eq!(headless.commands, 0);
        assert_eq!(headless.operations, 1);
    }

    #[test]
    fn capability_scenario_goldens() {
        let sensor = sensor_access(20).expect("sensor_access");
        assert_eq!(sensor.checksum, "bec7f9853c77e02a");
        assert_eq!(sensor.disabled, 0.0);
        assert_eq!(sensor.enabled, 1.0);
        assert_eq!(sensor.size, 1.0);

        let radar = radar_filters();
        assert_eq!(radar.checksum, "a740d5e42b2bb201");
        assert_eq!(radar.enemy, Some(4));
        assert_eq!(radar.ground, Some(4));
        assert_eq!(radar.farthest, Some(4));

        let gating = unit_control_gating();
        assert_eq!(gating.checksum, "a6a2ae141c632f15");
        assert!(gating.blocked);
        assert!(gating.installed);
        assert_eq!(gating.bind_count, 2);
    }

    #[test]
    fn privileged_scenario_goldens() {
        let world = privileged_world(10).expect("privileged_world");
        assert_eq!(world.checksum, "90f42f956b9be89b");
        assert!(!world.waves);
        assert_eq!(world.wave_spacing, 240.0);
        assert!(world.captured);
        assert_eq!(world.mission, "missiontext");
        assert_eq!(world.block_result, "content:block:0");
        for (kind, count) in &world.counts {
            if *kind == "sync" {
                assert_eq!(*count, 0, "sync should not fire before tick 3");
            } else {
                assert_eq!(*count, 1, "expected one {kind} event");
            }
        }

        let markers = markers_smoke().expect("markers_smoke");
        assert_eq!(markers.checksum, "d6e1700a676796c7");
        assert_eq!(markers.size, 1);
        assert!(markers.has_one);
        assert!(markers.world_visible);
        assert_eq!(markers.x, 24.0);
        assert_eq!(markers.y, 32.0);
        assert_eq!(markers.radius, 7.0);

        let sync = sync_event(13).expect("sync_event");
        assert_eq!(sync.checksum, "072353be139f4d21");
        assert_eq!(sync.events, 4);
        assert_eq!(sync.var_name, "x");
        assert!(!sync.is_obj);
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
