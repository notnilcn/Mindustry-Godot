// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! H06 T7: early-sector world-processor conformance suite. Every test runs a
//! shipped Serpulo map program verbatim against the live sim and asserts the
//! Java effect (`J/logic/LExecutor.java`). The programs were dumped from
//! `assets/maps/serpulo/*.msav` on 2026-10-14 (`H06-logic.md` §0); processor
//! tiles come from Ground Zero (GZ-1 (94,42), GZ-2 (95,42), GZ-3 (96,42),
//! GZ-4 (94,43), GZ-5 (96,43)).

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::content::registries::fx_meta::{EffectRef, effect_by_name};
use crate::logic::blocks::LogicBlockState;
use crate::logic::blocks::LogicDisplayState;
use crate::logic::blocks::LogicLink;
use crate::logic::value::LogicObject;
use crate::world::TilePos;
use crate::world::blocks::distribution::transfer;

use super::Sim;
use crate::sim::test_support::{
    assert_events_drained, campaign_runtime, campaign_runtime_mut, campaign_sim, configure,
    feed_turret, finish_sim, item, place_team1, place_world_processor, runtime_sim, test_rules,
    tick_n, wide_sim,
};

mod displays;
mod links;
mod maps;
mod sensors;
mod turrets_radar;
mod units;
mod world_effects;

// ---- shipped programs (verbatim, `H06-logic.md` §0) ----

/// Ground Zero GZ-1.
const GZ1: &str = "getflag wave \"spawnWave\"\njump 0 equal wave false\nsetflag \"spawnWave\" false\nspawnwave natural 10 true\n";

/// Ground Zero GZ-2.
const GZ2: &str = "getflag waves \"allowWaves\"\njump 0 equal waves false\nsetflag \"allowWaves\" false\nsetrule waveSending true 0 0 100 100\nsetrule waveTimer true 0 0 100 100\n";

/// Ground Zero GZ-3.
const GZ3: &str = "fetch build d @sharded i @scatter\nfetch buildCount scatters @sharded 0 @scatter\nop add i i 1\nop mod i i scatters\nsensor amount d @ammo\njump 0 equal amount 0\nsetflag \"scatterAmmo\" true\nstop\n";

/// Ground Zero GZ-4.
const GZ4: &str = "getflag mineOver \"mineOver\"\njump 0 equal mineOver false\nsetflag \"mineOver\" false\nsetrule mapArea 10 0 0 256 256\ngetflag end \"spawnOver\"\njump 4 equal end false\ncutscene pan 135 230 0.03 0\nwait 11\ncutscene stop 100 100 0.06 0\nstop\n";

/// Ground Zero GZ-5.
const GZ5: &str = "fetch build d @sharded i @duo\nsensor amount d @ammo\nop add i i 1\nfetch buildCount totalDuos @sharded 0 @duo\nop mod i i totalDuos\njump 0 equal amount 0\nsetflag \"duoAmmo\" true\nstop\n";

/// frozenForest FF-1.
const FF1: &str = "fetch buildCount gens @sharded 0 @combustion-generator\nset i 0\nfetch build gen @sharded i @combustion-generator\nsensor eff gen @efficiency\njump 7 lessThanEq eff 0\nsetflag \"power\" true\nend\njump 2 lessThan x gens\n";

/// frozenForest FF-2.
const FF2: &str = "setrate 1\ngetflag result \"done\"\nsetrule waveTimer result 0 0 100 100\n";

/// frontier block 1.
const FRONTIER: &str = "setrule waves false 0 0 100 100\ngetflag f \"revealMap\"\njump 1 notEqual f true\nsetrule mapArea 10 0 0 1000 1000\nsetrule waves true 0 0 100 100\nstop\n";

/// fungalPass block 1.
const FUNGAL: &str = "getflag result \"unitsBuilt\"\nsetrule waveTimer result 0 0 100 100\n";

/// extractionOutpost block 1.
const EXTRACTION: &str = "control enabled factory1 0 0 0 0\nwait 720\ncontrol enabled factory1 1 0 0 0\njump 2 always x false\n";

/// overgrowth block 1.
const OVERGROWTH: &str = "fetch buildCount count @crux 0 @air-factory\nop add i i 1\nop mod i i count\nfetch build build @crux i @air-factory\nop greaterThanEq var @waveNumber 2\ncontrol enabled build var 0 0 0\n";

// ---- helpers (the live-sim shape the `gdext` host installs) ----

fn session_wave(sim: &Sim) -> i32 {
    campaign_runtime(sim).session.wave
}

fn has_flag(sim: &Sim, flag: &str) -> bool {
    campaign_runtime(sim)
        .session
        .rules
        .objective_flags
        .contains(flag)
}

fn set_flags(sim: &mut Sim, flags: &[&str]) {
    let mut rt = campaign_runtime_mut(sim);
    for flag in flags {
        rt.session.rules.objective_flags.insert((*flag).to_owned());
    }
}

fn set_wave(sim: &mut Sim, wave: i32) {
    campaign_runtime_mut(sim).session.wave = wave;
}

fn unit_count(sim: &Sim, name: &str, team: u8) -> usize {
    let id = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .unit_id(name)
        .expect("unit");
    let units = sim.unit_runtime().expect("unit runtime");
    units
        .units
        .iter()
        .filter(|e| {
            sim.ecs
                .0
                .get::<crate::entities::comp::unit::UnitTypeComp>(**e)
                .map(|c| c.type_id)
                == Some(id)
                && sim
                    .ecs
                    .0
                    .get::<crate::entities::comp::TeamComp>(**e)
                    .map(|t| t.team)
                    == Some(team)
        })
        .count()
}

/// Adds a spawn rule to the live session (`Rules.spawns`).
fn add_spawn_group(sim: &mut Sim, unit: &str, amount: i32) {
    campaign_runtime_mut(sim)
        .session
        .rules
        .spawns
        .push(crate::io::json::SpawnGroup {
            type_: unit.to_owned(),
            begin: 0,
            unit_amount: amount,
            spawn: -1,
            ..Default::default()
        });
}

/// Puts a `spawn` overlay on the grid (`run_wave_system` reads it).
fn add_spawn_tile(sim: &mut Sim, x: i32, y: i32) {
    let spawn = sim
        .block_runtime()
        .expect("runtime")
        .content()
        .block_id("spawn")
        .expect("spawn");
    let index = sim.grid.tiles.index(x, y);
    sim.grid.tiles.geti_mut(index).overlay = spawn;
}

/// Places a world processor whose code carries one link (`LogicBlock` link
/// coordinates are relative to the processor; `relativeConnections`).
fn place_linked_world_processor(sim: &mut Sim, x: i16, y: i16, code: &str, links: &[LogicLink]) {
    let id = sim
        .world_block_id("world-processor")
        .expect("world-processor");
    sim.command(crate::determinism::SimCommand::Place {
        x,
        y,
        block: id,
        rotation: 0,
        team: 1,
        player: None,
    })
    .expect("placed");
    let bytes = crate::logic::blocks::logic_block::compress(code, links);
    configure(sim, x, y, crate::determinism::ConfigValue::Bytes(bytes));
}

fn building_enabled(sim: &Sim, e: Entity) -> bool {
    sim.ecs
        .0
        .get::<crate::entities::comp::Building>(e)
        .expect("building")
        .enabled
}

fn efficiency(sim: &Sim, e: Entity) -> f32 {
    sim.ecs
        .0
        .get::<crate::entities::comp::Building>(e)
        .expect("building")
        .efficiency
}
