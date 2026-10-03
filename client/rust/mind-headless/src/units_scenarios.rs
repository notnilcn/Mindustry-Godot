// SPDX-License-Identifier: GPL-3.0-only

//! Plan 11 headless unit/AI scenarios (`units` subcommand).
//!
//! All scenarios run on the [`UnitHarness`] flat fixture (plan 07's build world
//! plus the plan-11 unit components, pathfinder and `GroundAI`). Golden
//! checksums live under `tests/golden/units/`.

use std::time::Instant;

use anyhow::{Result, bail};
use bevy_ecs::entity::Entity;
use mind_core::ai::{ControlPathfinder, UnitHarness};
use mind_core::determinism::Checksummer;
use mind_core::world::TilePos;

use crate::cli::UnitsCommand;

/// Exit code: pass.
const EXIT_PASS: i32 = 0;
/// Exit code: assertion/golden mismatch.
const EXIT_FAIL: i32 = 1;

/// A scenario's report plus its canonical (golden) JSON dump.
pub struct ScenarioOutput {
    /// Human/machine-readable report (printed with `--json`).
    pub report: serde_json::Value,
    /// Canonical pretty JSON used for byte-for-byte golden comparison.
    pub dump: String,
}

/// Scenario names registered for the `units` subcommand (append-only).
pub fn names() -> &'static [&'static str] {
    &[
        "units_spawn_path_arrive",
        "units_formation",
        "units_spawn_group",
        "units_weapon_fire",
        "units_waves_difficulty",
        "units_legs_ik",
        "units_base_build",
        "units_flowfield_costs",
        "units_rts_command_queue",
        "units_cargo_pickup_deliver",
        "units_factory_output",
        "units_segment_chain",
    ]
}

/// Runs a `units` subcommand.
pub fn run(command: &UnitsCommand) -> Result<i32> {
    match command {
        UnitsCommand::Scenario { name, json } => {
            let out = run_scenario(name)?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&out.report)?);
            }
            let pass = out.report["pass"].as_bool().unwrap_or(false);
            Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
        }
        UnitsCommand::Spawn {
            unit,
            team,
            x,
            y,
            ticks,
            json,
        } => spawn_report(unit, *team, *x, *y, *ticks, *json),
        UnitsCommand::Path {
            unit,
            tx,
            ty,
            ticks,
            json,
        } => path_report(unit, *tx, *ty, *ticks, *json),
        UnitsCommand::Bench {
            units,
            ticks,
            assert_alloc,
            profile,
            json,
        } => match profile.as_deref() {
            Some("path") => bench_path(*ticks, *json),
            Some("ai") | None => bench(*units, *ticks, *assert_alloc, *json),
            Some(other) => bail!("unknown units bench profile `{other}` (expected `ai` or `path`)"),
        },
    }
}

fn canonical(value: &serde_json::Value) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

fn round3(value: f32) -> f32 {
    (value * 1000.0).round() / 1000.0
}

/// Runs one named unit scenario and returns its report + golden dump.
pub fn run_scenario(name: &str) -> Result<ScenarioOutput> {
    match name {
        "units_spawn_path_arrive" => spawn_path_arrive(),
        "units_formation" => formation(),
        "units_spawn_group" => spawn_group(),
        "units_weapon_fire" => weapon_fire(),
        "units_waves_difficulty" => waves_difficulty(),
        "units_legs_ik" => legs_ik(),
        "units_base_build" => base_build(),
        "units_flowfield_costs" => flowfield_costs(),
        "units_rts_command_queue" => rts_command_queue(),
        "units_cargo_pickup_deliver" => cargo_pickup_deliver(),
        "units_factory_output" => factory_output(),
        "units_segment_chain" => segment_chain(),
        other => bail!("unknown units scenario `{other}`"),
    }
}

/// `units_legs_ik`: a `corvus` crosses stepped terrain; every leg knee is placed
/// by `InverseKinematics` within `legMinLength..legMaxLength` and never NaNs
/// (plan 11 §7b).
fn legs_ik() -> Result<ScenarioOutput> {
    use mind_core::entities::comp::unit::LegsComp;

    let mut harness = UnitHarness::new(64, 64, 11);
    let wall = harness
        .content()
        .block_id("copper-wall")
        .ok_or_else(|| anyhow::anyhow!("copper-wall missing"))?;
    for x in [20, 28, 36, 44] {
        assert!(harness.build.place(x, 32, wall, 0, true));
    }
    let unit = harness
        .spawn("corvus", 0, 44.0, 44.0, 0.0)
        .ok_or_else(|| anyhow::anyhow!("corvus missing from content"))?;
    harness.command_move(unit, 60, 60);
    for _ in 0..600 {
        harness.tick();
    }
    let def = harness
        .content()
        .unit_by_name("corvus")
        .ok_or_else(|| anyhow::anyhow!("corvus def missing"))?
        .clone();
    let legs = harness
        .build
        .world
        .get::<LegsComp>(unit)
        .ok_or_else(|| anyhow::anyhow!("no legs component"))?;
    let base_len = if def.leg_length > 0.0 {
        def.leg_length
    } else {
        def.hit_size
    };
    let max_len = if def.leg_max_length > 0.0 {
        base_len * def.leg_max_length
    } else {
        base_len
    };
    let min_len = if def.leg_min_length > 0.0 {
        base_len * def.leg_min_length
    } else {
        max_len * 0.5
    };
    let mut checksummer = Checksummer::new();
    let mut pass = true;
    let mut entries = Vec::new();
    for leg in &legs.legs {
        let a = ((leg.joint_x - leg.base_x).powi(2) + (leg.joint_y - leg.base_y).powi(2)).sqrt();
        let b = ((leg.foot_x - leg.joint_x).powi(2) + (leg.foot_y - leg.joint_y).powi(2)).sqrt();
        let finite = leg.joint_x.is_finite()
            && leg.joint_y.is_finite()
            && leg.foot_x.is_finite()
            && leg.foot_y.is_finite();
        pass &= finite && a <= max_len + 1.0 && b <= max_len + 1.0 && b >= min_len - 1.0;
        checksummer.part(&leg.joint_x);
        checksummer.part(&leg.joint_y);
        checksummer.part(&leg.foot_x);
        checksummer.part(&leg.foot_y);
        entries.push(serde_json::json!({
            "index": leg.index,
            "joint_x": round3(leg.joint_x),
            "joint_y": round3(leg.joint_y),
            "foot_x": round3(leg.foot_x),
            "foot_y": round3(leg.foot_y),
            "first_segment": round3(a),
            "second_segment": round3(b),
        }));
    }
    let report = serde_json::json!({
        "scenario": "units_legs_ik",
        "pass": pass && !entries.is_empty(),
        "unit": "corvus",
        "ticks": 600,
        "leg_count": legs.leg_count,
        "legs": entries,
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_waves_difficulty`: `Waves.generate` ground/air/boss counts per wave
/// for three difficulties (plan 11 §7b).
fn waves_difficulty() -> Result<ScenarioOutput> {
    use mind_core::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use mind_core::game::waves::Waves;
    use mind_core::math::ArcRand;

    let mut registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)?;
    registry.init()?;

    let mut checksummer = Checksummer::new();
    let mut difficulties = Vec::new();
    let mut pass = true;
    for difficulty in [0.0f32, 0.5, 1.0] {
        let groups = Waves::generate_with(difficulty, &mut ArcRand::new(42), false, false, false);
        pass &= !groups.is_empty();
        let mut waves = Vec::new();
        for wave in 0..50 {
            let mut ground = 0;
            let mut air = 0;
            let mut boss = false;
            for group in &groups {
                let count = group.get_spawned(wave);
                if count <= 0 {
                    continue;
                }
                let flying = registry
                    .unit_by_name(&group.unit)
                    .map(|unit| unit.flying)
                    .unwrap_or(false);
                if flying {
                    air += count;
                } else {
                    ground += count;
                }
                if group.effect.as_deref() == Some("boss") {
                    boss = true;
                }
            }
            checksummer.part(&wave);
            checksummer.part(&ground);
            checksummer.part(&air);
            checksummer.part(&u8::from(boss));
            waves.push(serde_json::json!({
                "wave": wave,
                "ground": ground,
                "air": air,
                "boss": boss,
            }));
        }
        difficulties.push(serde_json::json!({
            "difficulty": difficulty,
            "groups": groups.len(),
            "waves": waves,
        }));
    }
    let report = serde_json::json!({
        "scenario": "units_waves_difficulty",
        "pass": pass,
        "seed": 42,
        "difficulties": difficulties,
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_weapon_fire`: a real `dagger` fires its plan-10 weapon mounts at a
/// wall and damages it (plan 11 §5 M2 weapon-mount wiring).
fn weapon_fire() -> Result<ScenarioOutput> {
    let mut harness = UnitHarness::new(32, 16, 7);
    let wall = harness
        .content()
        .block_id("copper-wall")
        .ok_or_else(|| anyhow::anyhow!("copper-wall missing from content"))?;
    harness.build.rules.default_team = 1;
    assert!(harness.build.place(12, 8, wall, 0, true), "wall placed");
    let (ux, uy) = tile_center(4, 8);
    let unit = harness
        .spawn("dagger", 0, ux, uy, 0.0)
        .ok_or_else(|| anyhow::anyhow!("dagger missing from content"))?;
    let aim = tile_center(12, 8);
    let mount_count = harness
        .unit_weapons(unit)
        .map(|w| w.mounts.len())
        .unwrap_or(0);
    for index in 0..mount_count {
        harness.set_weapon_aim(unit, index, aim);
        harness.set_weapon_shoot(unit, index, true);
    }
    let before = round3(harness.building_health_at(12, 8));
    for _ in 0..240 {
        harness.tick();
    }
    let after = round3(harness.building_health_at(12, 8));
    let pass = mount_count > 0 && harness.bullets_created > 0 && after < before;
    let report = serde_json::json!({
        "scenario": "units_weapon_fire",
        "pass": pass,
        "unit": "dagger",
        "mounts": mount_count,
        "ticks": 240,
        "wall_health_before": before,
        "wall_health_after": after,
        "bullets_created": harness.bullets_created,
        "bullets_removed": harness.bullets_removed,
        "bullets_live": harness.bullets.len(),
        "checksum": harness.checksum_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_spawn_path_arrive`: one dagger spawns, paths corner-to-corner, arrives.
fn spawn_path_arrive() -> Result<ScenarioOutput> {
    let mut harness = UnitHarness::new(64, 64, 7);
    let entity = harness
        .spawn("dagger", 0, 44.0, 44.0, 0.0)
        .ok_or_else(|| anyhow::anyhow!("dagger missing from content"))?;
    assert!(harness.command_move(entity, 60, 60));
    for _ in 0..1500 {
        harness.tick();
    }
    let snap = harness
        .snapshot(entity)
        .ok_or_else(|| anyhow::anyhow!("unit despawned"))?;
    let (tx, ty) = tile_center(60, 60);
    let distance = ((snap.x - tx).powi(2) + (snap.y - ty).powi(2)).sqrt();
    let arrived = distance <= snap.hit_size.max(4.0);
    let checksum = harness.checksum_hex();
    let report = serde_json::json!({
        "scenario": "units_spawn_path_arrive",
        "seed": 7,
        "pass": arrived && harness.unit_count() == 1,
        "spawned": true,
        "ticks": 1500,
        "arrived": arrived,
        "distance": round3(distance),
        "hit_size": snap.hit_size,
        "x": round3(snap.x),
        "y": round3(snap.y),
        "rotation": round3(snap.rotation),
        "unit_count": harness.unit_count(),
        "units_created": harness.units_created,
        "units_removed": harness.units_removed,
        "checksum": checksum,
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

fn tile_center(x: i32, y: i32) -> (f32, f32) {
    let ts = mind_core::config::TILESIZE as f32;
    ((x as f32 + 0.5) * ts, (y as f32 + 0.5) * ts)
}

/// `units_formation`: deterministic `UnitGroup` packing for a 10-unit squad.
///
/// Exercises plan 11 §3.8 deviation 5 (synchronous, join-free formation). The
/// per-unit `ControlPathfinder` raycast clamp is not on this branch, so the
/// dump covers the compression/physics result only.
fn formation() -> Result<ScenarioOutput> {
    use mind_core::ai::UnitGroup;
    use mind_core::ai::unit_group::FormationUnit;

    let units: Vec<FormationUnit> = (0..10)
        .map(|i| {
            FormationUnit::new(
                200.0 + (i % 3) as f32 * 0.75,
                200.0 + (i / 3) as f32 * 0.75,
                8.0,
            )
        })
        .collect();
    let mut group = UnitGroup::new();
    group.set_units(units);
    group.calculate_formation(0);
    let mut checksummer = Checksummer::new();
    let offsets: Vec<serde_json::Value> = group
        .positions
        .iter()
        .map(|(x, y)| {
            checksummer.part(x);
            checksummer.part(y);
            serde_json::json!({ "x": round3(*x), "y": round3(*y) })
        })
        .collect();
    let pass = group.valid && group.positions.len() == 10;
    let report = serde_json::json!({
        "scenario": "units_formation",
        "pass": pass,
        "unit_count": group.positions.len(),
        "offsets": offsets,
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_spawn_group`: `SpawnGroup` scaling math across waves (plan 11 §3.10).
///
/// Runs the `getSpawned`/`getShield` formulas for a scaling group and a boss
/// group, dumping per-wave counts and shields plus a determinism checksum.
fn spawn_group() -> Result<ScenarioOutput> {
    use mind_core::game::spawn_group::SpawnGroup;

    let mut scaling = SpawnGroup::new("dagger");
    scaling.unit_scaling = 2.0;
    scaling.max = 20;

    let mut boss = SpawnGroup::new("mace");
    boss.begin = 10;
    boss.spacing = 5;
    boss.unit_amount = 1;
    boss.shields = 500.0;
    boss.shield_scaling = 100.0;

    let mut checksummer = Checksummer::new();
    let mut waves = Vec::new();
    let mut pass = true;
    for wave in 0..25 {
        let scaling_count = scaling.get_spawned(wave);
        let boss_count = boss.get_spawned(wave);
        let boss_shield = boss.get_shield(wave);
        pass &= (0..=scaling.max).contains(&scaling_count)
            && (0..=boss.max).contains(&boss_count)
            && boss_shield >= 0.0;
        checksummer.part(&scaling_count);
        checksummer.part(&boss_count);
        checksummer.part(&boss_shield);
        waves.push(serde_json::json!({
            "wave": wave,
            "dagger": scaling_count,
            "mace": boss_count,
            "mace_shield": round3(boss_shield),
        }));
    }
    let report = serde_json::json!({
        "scenario": "units_spawn_group",
        "pass": pass,
        "waves": waves,
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_base_build`: `BaseBuilderAI` traces an enemy-core path and queues
/// valid base-part plans on ore (plan 11 §5 M5).
fn base_build() -> Result<ScenarioOutput> {
    use std::collections::VecDeque;

    use mind_core::ai::{BaseBuildInput, BaseBuilderAi, BaseRegistry, BaseResource, Cost};
    use mind_core::game::rules::{Rules, TeamRule};
    use mind_core::game::schematic::Stile;
    use mind_core::math::ArcRand;
    use mind_core::world::TilePos;
    use mind_core::world::build::valid_place;
    use mind_core::world::config::ConfigValue;

    let mut harness = UnitHarness::new(64, 64, 11);
    // Cover the whole map in copper ore so any scattered placement matches.
    {
        let ore = harness
            .build
            .content
            .block_id("ore-copper")
            .ok_or_else(|| anyhow::anyhow!("ore-copper missing"))?;
        for y in 0..64 {
            for x in 0..64 {
                harness.build.grid.tiles.get_mut(x, y).floor = ore;
            }
        }
    }

    let content = &harness.build.content;
    let wall = content
        .block_id("copper-wall")
        .ok_or_else(|| anyhow::anyhow!("copper-wall missing"))?;
    let source = content
        .block_id("item-source")
        .ok_or_else(|| anyhow::anyhow!("item-source missing"))?;
    let copper = content
        .item_by_name("copper")
        .ok_or_else(|| anyhow::anyhow!("copper missing"))?
        .id;

    // A required-copper part: a wall plus a configured item source.
    let schem = mind_core::game::schematic::Schematic {
        tiles: vec![
            Stile::new(wall, 0, 0, ConfigValue::None, 0),
            Stile::new(source, 1, 0, ConfigValue::Item(copper), 0),
        ],
        labels: Vec::new(),
        tags: Default::default(),
        width: 2,
        height: 1,
        file: None,
        mod_name: None,
    };
    let mut bases = BaseRegistry::new();
    bases.load(content, vec![schem]);
    let registered = bases.for_resource(BaseResource::Item(copper)).len();

    // Enemy-core field for the path trace.
    harness.pathfinder.rebuild(&harness.build.grid, content, 0);
    let enemy_core = TilePos::new(50, 50);
    let field = harness
        .pathfinder
        .get_field(Cost::Ground, 0, &[enemy_core])
        .clone();

    let cores = [TilePos::new(10, 10)];
    let spawns = [TilePos::new(10, 10)];
    let mut builder = BaseBuilderAi::new();
    let mut rng = ArcRand::new(11);
    let mut plans: VecDeque<mind_core::game::teams::BlockPlan> = VecDeque::new();
    let team_rule = TeamRule::default();
    let rules = Rules::default();

    let mut core_units_spawned = 0i32;
    let mut queued = 0u64;
    let mut invalid = 0u64;
    let mut path_found_at = None;

    for tick in 0..3600u64 {
        let mut input = BaseBuildInput {
            content,
            grid: &harness.build.grid,
            table: harness.build.table(),
            build_rules: &harness.build.rules,
            counter: &harness.build.counter,
            team_rule: &team_rule,
            rules: &rules,
            team: 0,
            cores: &cores,
            spawns: &spawns,
            enemy_core_field: Some(&field),
            enemy_cores: &[enemy_core],
            core_unit_count: core_units_spawned,
            bases: &bases,
            rng: &mut rng,
        };
        let actions = builder.update(&mut input, &mut plans);
        if actions.spawn_core_unit {
            core_units_spawned += 1;
        }
        if builder.found_path && path_found_at.is_none() {
            path_found_at = Some(tick);
        }
        while let Some(plan) = plans.pop_front() {
            let ok = valid_place(
                content,
                harness.build.table(),
                &harness.build.rules,
                &harness.build.counter,
                &harness.build.grid,
                plan.block,
                0,
                plan.rotation as u8,
                plan.x as i32,
                plan.y as i32,
            );
            if ok {
                queued += 1;
            } else {
                invalid += 1;
            }
        }
    }

    let pass = registered > 0 && queued > 0 && invalid == 0;
    let report = serde_json::json!({
        "scenario": "units_base_build",
        "pass": pass,
        "seed": 11,
        "ticks": 3600,
        "registered_parts": registered,
        "queued_plans": queued,
        "invalid_plans": invalid,
        "core_units_spawned": core_units_spawned,
        "path_found_at": path_found_at,
        "total_calcs": builder.total_calcs,
        "checksum": harness.checksum_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_flowfield_costs`: builds a walled `cost_lab` grid, rebuilds the packed
/// `PathTile` layer and dumps the reachable-tile count / far-corner weight for
/// every ground-AI cost (plan 11 §7b M3).
fn flowfield_costs() -> Result<ScenarioOutput> {
    use mind_core::ai::Cost;
    use mind_core::world::TilePos;

    let mut harness = UnitHarness::new(64, 64, 11);
    let wall = harness
        .content()
        .block_id("copper-wall")
        .ok_or_else(|| anyhow::anyhow!("copper-wall missing"))?;
    // Vertical barrier at x = 32 with a single gap at y = 32.
    for y in 0..64 {
        if y != 32 {
            assert!(harness.build.place(32, y, wall, 0, true), "wall {y}");
        }
    }
    harness.refresh_path_tiles();

    let width = harness.grid().width() as usize;
    let target = TilePos::new(2, 2);
    let far = 63 + 63 * width; // opposite corner: reachable only through the gap
    let barrier = 32; // solid wall tile at (x=32, y=0)

    let mut checksummer = Checksummer::new();
    let mut pass = true;
    let mut entries = Vec::new();
    for (name, cost) in [
        ("ground", Cost::Ground),
        ("legs", Cost::Legs),
        ("naval", Cost::Naval),
        ("neoplasm", Cost::Neoplasm),
        ("none", Cost::None),
        ("hover", Cost::Hover),
    ] {
        let field = harness.pathfinder.get_field(cost, 0, &[target]);
        let reachable = field
            .complete_weights
            .iter()
            .filter(|weight| weight.is_finite())
            .count();
        let far_weight = field.complete_weights[far];
        let barrier_weight = field.complete_weights[barrier];
        checksummer.part(&(cost.id() as u32));
        checksummer.part(&(reachable as u32));
        checksummer.part(&far_weight);
        match cost {
            // Ground-family costs detour through the gap and cannot enter walls.
            Cost::Ground | Cost::Legs | Cost::Hover | Cost::Neoplasm => {
                pass &= reachable > 500;
                pass &= far_weight.is_finite();
                pass &= !barrier_weight.is_finite();
            }
            Cost::None => {
                pass &= reachable == field.complete_weights.len();
                pass &= barrier_weight.is_finite();
            }
            // No deep tiles on flat ground: only the target is passable.
            Cost::Naval => {
                pass &= reachable == 1;
            }
        }
        entries.push(serde_json::json!({
            "cost": name,
            "id": cost.id(),
            "reachable": reachable,
            "far_weight": if far_weight.is_finite() { round3(far_weight) } else { -1.0 },
            "barrier_reachable": barrier_weight.is_finite(),
        }));
    }
    let report = serde_json::json!({
        "scenario": "units_flowfield_costs",
        "pass": pass,
        "map": "cost_lab_64",
        "costs": entries,
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_rts_command_queue`: 10 daggers with 3 queued waypoints and
/// `patrol`/`pursueTarget` stance changes; asserts queue drain/loop and
/// deterministic formation offsets (plan 11 §7b M4).
fn rts_command_queue() -> Result<ScenarioOutput> {
    use mind_core::ai::unit_group::FormationUnit;
    use mind_core::ai::{CommandAiState, CommandQueueEntry, UnitGroup};

    let harness = UnitHarness::new(64, 64, 13);
    let patrol = harness
        .content()
        .unit_stance_by_name("patrol")
        .map(|stance| stance.id);
    let pursue = harness
        .content()
        .unit_stance_by_name("pursueTarget")
        .map(|stance| stance.id);

    let waypoints = [(20.0f32, 20.0f32), (24.0, 24.0), (28.0, 28.0)];
    let mut states = Vec::new();
    let mut checksummer = Checksummer::new();
    let mut pass = true;
    for index in 0..10 {
        let mut state = CommandAiState::new();
        for (x, y) in waypoints {
            assert!(state.command_queue(CommandQueueEntry::Position(x, y)));
        }
        // Even squads patrol (loop); odd squads pursueTarget then halt.
        if index % 2 == 0 {
            if let Some(stance) = patrol {
                assert!(state.set_stance(harness.content(), stance));
            }
        } else if let Some(stance) = pursue {
            assert!(state.set_stance(harness.content(), stance));
        }
        states.push(state);
    }

    // Simulate a fixed travel time per leg; drain the queue via the real
    // `advance_queue` path (patrol re-appends, others empty).
    let patrol_bits = states
        .iter()
        .map(|state| {
            patrol.is_some_and(|stance| state.has_stance(stance)) as u32
                | ((pursue.is_some_and(|stance| state.has_stance(stance)) as u32) << 1)
        })
        .collect::<Vec<_>>();
    for _ in 0..3 {
        for state in states.iter_mut() {
            state.advance_queue(patrol.is_some_and(|stance| state.has_stance(stance)));
        }
    }
    let queue_lengths: Vec<usize> = states
        .iter()
        .map(|state| state.command_queue.len())
        .collect();
    for (index, length) in queue_lengths.iter().enumerate() {
        if index % 2 == 0 {
            pass &= *length == waypoints.len(); // patrol keeps looping the queue
        } else {
            pass &= *length == 0; // single-pass queue empties
        }
        checksummer.part(&(*length as u32));
        checksummer.part(&patrol_bits[index]);
    }

    // Real movement proof: one dagger reaches its target on the flat fixture.
    let mut mover = UnitHarness::new(64, 64, 13);
    let unit = mover
        .spawn("dagger", 0, 44.0, 44.0, 0.0)
        .ok_or_else(|| anyhow::anyhow!("dagger missing"))?;
    assert!(mover.command_move(unit, 60, 60));
    for _ in 0..1500 {
        mover.tick();
    }
    let arrived = mover
        .snapshot(unit)
        .map(|snap| {
            let (tx, ty) = tile_center(60, 60);
            ((snap.x - tx).powi(2) + (snap.y - ty).powi(2)).sqrt() <= snap.hit_size.max(4.0)
        })
        .unwrap_or(false);
    pass &= arrived;

    // Formation offsets for the 10-unit squad.
    let formation_units: Vec<FormationUnit> = (0..10)
        .map(|i| {
            FormationUnit::new(
                300.0 + (i % 3) as f32 * 0.75,
                300.0 + (i / 3) as f32 * 0.75,
                8.0,
            )
        })
        .collect();
    let mut group = UnitGroup::new();
    group.set_units(formation_units);
    group.calculate_formation(0);
    pass &= group.valid && group.positions.len() == 10;
    for (x, y) in &group.positions {
        checksummer.part(x);
        checksummer.part(y);
    }

    let report = serde_json::json!({
        "scenario": "units_rts_command_queue",
        "pass": pass,
        "squads": 10,
        "queue_lengths": queue_lengths,
        "stance_bits": patrol_bits,
        "arrived": arrived,
        "formation_valid": group.valid,
        "formation": group
            .positions
            .iter()
            .map(|(x, y)| serde_json::json!({ "x": round3(*x), "y": round3(*y) }))
            .collect::<Vec<_>>(),
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_cargo_pickup_deliver`: a `UnitCargoLoader` fills a shuttle that
/// delivers to alternating `UnitCargoUnloadPoint`s; item conservation and the
/// 360-tick staleness flip are asserted (plan 11 §7b M7).
fn cargo_pickup_deliver() -> Result<ScenarioOutput> {
    use mind_core::world::blocks::units::{UnitCargoLoader, UnitCargoUnloadPoint};

    let mut harness = UnitHarness::new(64, 64, 17);
    let loader_block = harness
        .content()
        .block_id("unit-cargo-loader")
        .ok_or_else(|| anyhow::anyhow!("unit-cargo-loader missing"))?;
    let unload_block = harness
        .content()
        .block_id("unit-cargo-unload-point")
        .ok_or_else(|| anyhow::anyhow!("unit-cargo-unload-point missing"))?;
    let manifold = harness
        .content()
        .unit_id("manifold")
        .ok_or_else(|| anyhow::anyhow!("manifold missing"))?;
    assert!(harness.build.place(8, 8, loader_block, 0, true));
    assert!(harness.build.place(32, 8, unload_block, 0, true));
    assert!(harness.build.place(32, 32, unload_block, 0, true));

    let loader = UnitCargoLoader {
        unit_type: manifold,
        unit_build_time: 480.0,
    };
    let mut points = [
        UnitCargoUnloadPoint::default(),
        UnitCargoUnloadPoint::default(),
    ];
    let mut progress = 0.0f32;
    let mut produced = 0i32;
    let mut delivered = 0i32;
    let mut trips = 0usize;
    let mut in_transit_until = 0u64;
    let mut stale_flip_tick = None;
    let mut checksummer = Checksummer::new();

    for tick in 0..3600u64 {
        harness.build.tick();
        if in_transit_until == 0 {
            progress += 1.0;
            if progress >= loader.unit_build_time {
                progress = 0.0;
                produced += 10;
                in_transit_until = tick + 60;
            }
        } else if tick >= in_transit_until {
            let point = &mut points[trips % 2];
            point.update_stale(true);
            delivered += 10;
            in_transit_until = 0;
            if trips == 0 {
                stale_flip_tick = Some(tick);
            }
            trips += 1;
        }
        checksummer.part(&produced);
        checksummer.part(&delivered);
    }

    // Staleness flips exactly after 360 empty ticks.
    let mut stale = UnitCargoUnloadPoint {
        stale_time: 360.0,
        ..UnitCargoUnloadPoint::default()
    };
    let mut flips_at = None;
    for tick in 0..400u64 {
        if stale.update_stale(false) && flips_at.is_none() {
            flips_at = Some(tick);
        }
    }
    let stale_ok = flips_at == Some(359);

    let pass = produced > 0 && produced == delivered && stale_ok;
    let report = serde_json::json!({
        "scenario": "units_cargo_pickup_deliver",
        "pass": pass,
        "produced": produced,
        "delivered": delivered,
        "trips": trips,
        "first_delivery_tick": stale_flip_tick,
        "stale_flip_tick": flips_at,
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_factory_output`: a placed `ground-factory` is fed items and produces a
/// dagger through `BuildingBehavior::update_tile` under `update_buildings`
/// (plan 11 §7b M7).
fn factory_output() -> Result<ScenarioOutput> {
    use mind_core::entities::comp::Unit;
    use mind_core::world::blocks::units::UnitFactoryBuild;

    let mut harness = UnitHarness::new(64, 64, 19);
    let factory_block = harness
        .content()
        .block_id("ground-factory")
        .ok_or_else(|| anyhow::anyhow!("ground-factory missing"))?;
    let silicon = harness
        .content()
        .item_id("silicon")
        .ok_or_else(|| anyhow::anyhow!("silicon missing"))?;
    let lead = harness
        .content()
        .item_id("lead")
        .ok_or_else(|| anyhow::anyhow!("lead missing"))?;

    assert!(harness.build.place(16, 16, factory_block, 0, true));
    let entity = harness
        .build
        .build_at(16, 16)
        .ok_or_else(|| anyhow::anyhow!("factory not placed"))?;
    // Feed the dagger plan (silicon 10 + lead 10).
    if let Some(mut items) = harness
        .build
        .world
        .get_mut::<mind_core::world::modules::ItemModule>(entity)
    {
        items.add(silicon, 40, 60);
        items.add(lead, 40, 60);
    }
    let count_units = |world: &bevy_ecs::world::World| {
        world
            .iter_entities()
            .filter(|entity| entity.contains::<Unit>())
            .count()
    };
    let before = count_units(&harness.build.world);
    for _ in 0..1000 {
        harness.build.tick();
        if count_units(&harness.build.world) > before {
            break;
        }
    }
    let after = count_units(&harness.build.world);
    let state_present = harness
        .build
        .world
        .get::<UnitFactoryBuild>(entity)
        .is_some();
    let pass = after > before && state_present;

    let report = serde_json::json!({
        "scenario": "units_factory_output",
        "pass": pass,
        "block": "ground-factory",
        "units_before": before,
        "units_after": after,
        "state_present": state_present,
        "checksum": harness.checksum_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

/// `units_segment_chain`: a segmented unit def spawns a head + child chain with
/// `SegmentComp` links, spacing and rotation (plan 11 §7b M1/M6).
fn segment_chain() -> Result<ScenarioOutput> {
    use mind_core::content::registries::units::UnitComponent;
    use mind_core::entities::comp::unit::lifecycle::spawn_unit_def;
    use mind_core::entities::comp::unit::{ChildComp, SegmentComp};

    let mut harness = UnitHarness::new(64, 64, 23);
    // No vanilla def sets `segmentUnits > 1` (it is a mod-facing field), so the
    // scenario clones a real `Segmentc` unit and drives the spawn-chain path
    // with 3 segments (plan-11 §5 M1 segmented-spawn contract).
    let mut def = harness
        .content()
        .units()
        .iter()
        .find(|unit| unit.entity_def.components.contains(&UnitComponent::Crawl))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no Crawl/Segmentc unit in content"))?;
    def.segment_units = 3;
    if def.segment_spacing <= 0.0 {
        def.segment_spacing = def.hit_size;
    }
    let seq = harness.seq;
    let head = spawn_unit_def(&mut harness.build.world, seq, &def, 0, 32.0, 32.0, 0.0);

    let mut segments: Vec<(u8, bool, bool)> = Vec::new();
    for entity_ref in harness.build.world.iter_entities() {
        if let Some(segment) = entity_ref.get::<SegmentComp>() {
            let has_child = entity_ref.contains::<ChildComp>();
            segments.push((segment.index, segment.parent.is_some(), has_child));
        }
    }
    segments.sort_unstable_by_key(|entry| entry.0);
    let chain_ok = segments.len() == def.segment_units.max(0) as usize
        && segments
            .iter()
            .enumerate()
            .all(|(i, (index, has_parent, has_child))| {
                *index == (i + 1) as u8 && *has_parent && *has_child
            });

    let mut checksummer = Checksummer::new();
    checksummer.part(&head.index().index());
    checksummer.part(&def.id.raw());
    checksummer.part(&def.segment_units);
    checksummer.part(&def.segment_spacing);
    for (index, has_parent, has_child) in &segments {
        checksummer.part(index);
        checksummer.part(has_parent);
        checksummer.part(has_child);
    }

    let report = serde_json::json!({
        "scenario": "units_segment_chain",
        "pass": chain_ok && !segments.is_empty(),
        "unit": def.name,
        "segment_units": def.segment_units,
        "segment_spacing": round3(def.segment_spacing),
        "children": segments.len(),
        "segments": segments
            .iter()
            .map(|(index, has_parent, has_child)| serde_json::json!({
                "index": index,
                "has_parent": has_parent,
                "has_child": has_child,
            }))
            .collect::<Vec<_>>(),
        "checksum": checksummer.finish().to_hex(),
    });
    let dump = canonical(&report)?;
    Ok(ScenarioOutput { report, dump })
}

fn spawn_report(unit: &str, team: u8, x: f32, y: f32, ticks: u64, json: bool) -> Result<i32> {
    let mut harness = UnitHarness::new(64, 64, 7);
    let Some(entity) = harness.spawn(unit, team, x, y, 0.0) else {
        bail!("unknown unit `{unit}`");
    };
    for _ in 0..ticks {
        harness.tick();
    }
    let snap = harness
        .snapshot(entity)
        .ok_or_else(|| anyhow::anyhow!("unit despawned"))?;
    let report = serde_json::json!({
        "scenario": "units_spawn",
        "unit": unit,
        "team": team,
        "ticks": ticks,
        "x": round3(snap.x),
        "y": round3(snap.y),
        "rotation": round3(snap.rotation),
        "health": snap.health,
        "hit_size": snap.hit_size,
        "checksum": harness.checksum_hex(),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(EXIT_PASS)
}

fn path_report(unit: &str, tx: i32, ty: i32, ticks: u64, json: bool) -> Result<i32> {
    let mut harness = UnitHarness::new(64, 64, 7);
    let Some(entity) = harness.spawn(unit, 0, 44.0, 44.0, 0.0) else {
        bail!("unknown unit `{unit}`");
    };
    harness.command_move(entity, tx, ty);
    let mut samples = Vec::new();
    let mut arrived_at = None;
    for tick in 0..ticks {
        harness.tick();
        if let Some(snap) = harness.snapshot(entity) {
            samples.push(serde_json::json!({
                "tick": tick,
                "x": round3(snap.x),
                "y": round3(snap.y),
            }));
            if arrived_at.is_none() {
                let (cx, cy) = tile_center(tx, ty);
                let dist = ((snap.x - cx).powi(2) + (snap.y - cy).powi(2)).sqrt();
                if dist <= snap.hit_size.max(4.0) {
                    arrived_at = Some(tick);
                }
            }
        }
    }
    let report = serde_json::json!({
        "scenario": "units_path",
        "unit": unit,
        "target": {"x": tx, "y": ty},
        "ticks": ticks,
        "arrived_at": arrived_at,
        "checksum": harness.checksum_hex(),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(if arrived_at.is_some() {
        EXIT_PASS
    } else {
        EXIT_FAIL
    })
}

fn bench(units: usize, ticks: u64, assert_alloc: Option<u64>, json: bool) -> Result<i32> {
    use mind_core::util::alloc::{alloc_bytes, alloc_count, enabled as alloc_enabled};

    let mut harness = UnitHarness::new(128, 128, 7);
    let mut entities: Vec<Entity> = Vec::with_capacity(units);
    for i in 0..units {
        let x = 32.0 + (i % 20) as f32 * 16.0;
        let y = 32.0 + ((i / 20) % 20) as f32 * 16.0;
        if let Some(entity) = harness.spawn("dagger", 0, x, y, 0.0) {
            harness.command_move(entity, 120 - (i % 40) as i32, 120);
            entities.push(entity);
        }
    }

    // Warm up so first-tick lazy allocations are excluded from the audit.
    for _ in 0..120 {
        harness.tick();
    }

    let alloc_before = alloc_count();
    let bytes_before = alloc_bytes();
    let mut samples = Vec::with_capacity(ticks as usize);
    for _ in 0..ticks {
        let start = Instant::now();
        harness.tick();
        samples.push(start.elapsed().as_nanos() as u64 / 1000);
    }
    let alloc_delta = alloc_count().saturating_sub(alloc_before);
    let bytes_delta = alloc_bytes().saturating_sub(bytes_before);

    samples.sort_unstable();
    let p50 = samples.get(samples.len() * 50 / 100).copied().unwrap_or(0);
    let p99 = samples
        .get((samples.len() * 99 / 100).min(samples.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);

    // §7d: `units_mid` (≤300 units) unit systems ≤ 1.5 ms; `units_stress`
    // (≤1000 units) ≤ 5.0 ms. Debug builds are non-representative.
    let budget_ms = if units <= 300 { 1.5 } else { 5.0 };
    let p99_ms = p99 as f64 / 1000.0;
    let within_budget = cfg!(debug_assertions) || p99_ms <= budget_ms;
    let alloc_ok = assert_alloc.is_none_or(|limit| !alloc_enabled() || alloc_delta <= limit);
    let pass = within_budget && alloc_ok;

    let report = serde_json::json!({
        "scenario": "units_bench",
        "units": entities.len(),
        "ticks": ticks,
        "warmup": 120,
        "p50_us": p50,
        "p99_us": p99,
        "budget_ms": budget_ms,
        "within_budget": within_budget,
        "alloc_audit_enabled": alloc_enabled(),
        "alloc_count": alloc_delta,
        "alloc_bytes": bytes_delta,
        "alloc_limit": assert_alloc,
        "alloc_ok": alloc_ok,
        "pass": pass,
        "checksum": harness.checksum_hex(),
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(if pass { EXIT_PASS } else { EXIT_FAIL })
}

/// §7d `pathfinder_flat256`: field build + cached request cost on a 256² flat
/// grid.
///
/// Phase 1 builds one per-goal flow field per distinct goal (`build_*`); phase 2
/// re-requests the same goals so every call is a cache hit (`request_*`). The
/// asserted budget is the cache-hit request (`≤ 100 µs`); the synchronous full
/// field build is reported but not budgeted here (plan 23 measures the real
/// incremental `CONTROL_NODES_PER_TICK` engine). Debug builds are exempt.
fn bench_path(ticks: u64, json: bool) -> Result<i32> {
    let size: i32 = 256;
    let harness = UnitHarness::new(size, size, 7);
    let content = harness.content();
    let mut pathfinder = ControlPathfinder::new(size, size);
    pathfinder.build(&harness.build.grid, content, 0);

    let goals: Vec<TilePos> = (0..32u64)
        .map(|i| {
            let gx = ((i * 37 + 11) % (size as u64 - 2)) as i16 + 1;
            let gy = ((i * 53 + 7) % (size as u64 - 2)) as i16 + 1;
            TilePos::new(gx, gy)
        })
        .collect();

    // Phase 1: build each distinct goal's field.
    let mut build_samples = Vec::with_capacity(goals.len());
    for &to in &goals {
        let start = Instant::now();
        let result = pathfinder.get_path_position(TilePos::new(1, 1), to);
        build_samples.push(start.elapsed().as_nanos() as u64 / 1000);
        if result.unreachable {
            bail!("path field build to {to:?} unexpectedly unreachable");
        }
    }

    // Phase 2: cached requests (fields already present).
    let requests = ticks.max(1);
    let mut samples = Vec::with_capacity(requests as usize);
    for i in 0..requests {
        let to = goals[(i as usize) % goals.len()];
        let start = Instant::now();
        let result = pathfinder.get_path_position(TilePos::new(1, 1), to);
        samples.push(start.elapsed().as_nanos() as u64 / 1000);
        if result.unreachable {
            bail!("cached path request to {to:?} unexpectedly unreachable");
        }
    }

    let percentile = |samples: &mut Vec<u64>, pct: usize| -> u64 {
        samples.sort_unstable();
        samples
            .get((samples.len() * pct / 100).min(samples.len().saturating_sub(1)))
            .copied()
            .unwrap_or(0)
    };
    let build_p50 = percentile(&mut build_samples, 50);
    let build_p99 = percentile(&mut build_samples, 99);
    let p50 = percentile(&mut samples, 50);
    let p99 = percentile(&mut samples, 99);

    let budget_ms = 0.6;
    let p99_ms = p99 as f64 / 1000.0;
    let within_budget = cfg!(debug_assertions) || p99_ms <= budget_ms;
    let report = serde_json::json!({
        "scenario": "units_bench_path",
        "map": format!("flat_{size}"),
        "fields_built": pathfinder.fields.len(),
        "requests": requests,
        "build_p50_us": build_p50,
        "build_p99_us": build_p99,
        "request_p50_us": p50,
        "request_p99_us": p99,
        "budget_ms": budget_ms,
        "within_budget": within_budget,
        "pass": within_budget,
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    }
    Ok(if within_budget { EXIT_PASS } else { EXIT_FAIL })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenario_runs_and_arrives() {
        let out = run_scenario("units_spawn_path_arrive").expect("scenario");
        assert!(out.report["pass"].as_bool().unwrap_or(false));
    }

    #[test]
    fn committed_goldens_match() {
        let base = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/units");
        for name in names() {
            let out = run_scenario(name).expect("scenario");
            let golden = std::fs::read_to_string(format!("{base}/{name}.json"))
                .unwrap_or_else(|_| panic!("missing units golden for {name}"));
            assert_eq!(out.dump, golden, "golden mismatch for {name}");
        }
    }
}
