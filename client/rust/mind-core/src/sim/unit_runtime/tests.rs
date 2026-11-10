// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Test-only live unit-runtime suites.
//! Upstream: none (port-only test support).

use super::targeting::{block_targetable, target_flag_active};

use super::*;
use crate::content::BlockId;
use crate::determinism::SimCommand;

fn install(sim: &mut Sim) {
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
}

fn block_id(sim: &Sim, name: &str) -> u16 {
    sim.unit_runtime()
        .expect("runtime")
        .content
        .block_id(name)
        .expect("block exists")
        .get()
}

/// The evaluator repro at the core level: request a wave on a live world
/// with a player core and a `spawn` overlay; units must spawn, move and
/// fire bullets at the core.
#[test]
fn run_wave_spawns_moving_firing_units() {
    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    let core = block_id(&sim, "core-shard");
    let spawn = block_id(&sim, "spawn");
    assert!(
        sim.command(SimCommand::Place {
            x: 52,
            y: 52,
            block: core,
            rotation: 0,
            team: 1,
            player: None,
        })
        .is_ok(),
        "player core placed"
    );
    let index = sim.grid.tiles.index(6, 6);
    sim.grid.tiles.geti_mut(index).overlay = BlockId::new(spawn);

    sim.set_unit_wave_team(2);
    sim.request_wave_spawn(0);
    // One tick spawns the wave; the same tick's `UpdateUnits` already runs.
    sim.tick().expect("tick");
    assert!(
        sim.group_counts().get("unit").copied().unwrap_or(0) >= 1,
        "wave spawned a unit: {:?}",
        sim.group_counts()
    );

    let start = sim
        .unit_runtime()
        .expect("runtime")
        .units
        .first()
        .and_then(|entity| sim.ecs.0.get::<Pos>(*entity).copied())
        .map(|pos| (pos.x, pos.y));
    // 1200 ticks: the spawn `unmoving` (30, disarmed) + `invincible` (60)
    // effects and the 40/90-tick retarget cadence delay the first shots
    // (G01 T7).
    for _ in 0..1200 {
        sim.tick().expect("tick");
    }
    let runtime = sim.unit_runtime().expect("runtime");
    assert!(runtime.bullets_created > 0, "wave unit fired bullets");
    assert!(
        runtime.units_removed + runtime.bullets_removed > 0,
        "combat resolved"
    );
    let moved = runtime
        .units
        .iter()
        .filter_map(|entity| sim.ecs.0.get::<Pos>(*entity))
        .any(|pos| {
            start.is_some_and(|(sx, sy)| {
                (pos.x - sx).powi(2) + (pos.y - sy).powi(2) > (TILE_SIZE * 2.0).powi(2)
            })
        });
    assert!(moved, "spawned unit left its spawn tile");
}

/// G01 T7: `WaveSpawner.doShockwave` (WaveSpawner.java:104-106) kills every
/// unit and building of other teams inside `rules.dropZoneRadius` when the
/// wave spawns; far-away buildings survive.
#[test]
fn wave_shockwave_destroys_drop_zone_building() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    let campaign = {
        let content = &sim.unit_runtime().expect("runtime").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let spawn = block_id(&sim, "spawn");
    let wall = block_id(&sim, "copper-wall");
    let index = sim.grid.tiles.index(10, 10);
    sim.grid.tiles.geti_mut(index).overlay = BlockId::new(spawn);
    // 2 tiles from the spawn corner (inside the 300 px drop zone) and far.
    for (x, y) in [(12, 10), (60, 60)] {
        assert!(
            sim.command(SimCommand::Place {
                x,
                y,
                block: wall,
                rotation: 0,
                team: 1,
                player: None,
            })
            .is_ok(),
            "wall placed at ({x},{y})"
        );
    }
    sim.request_wave_spawn(0);
    sim.tick().expect("tick");
    assert!(
        sim.grid.tiles.get(12, 10).build.is_none(),
        "the drop-zone wall was destroyed by the shockwave"
    );
    assert!(
        sim.grid.tiles.get(60, 60).build.is_some(),
        "the far wall survived"
    );
}

/// G01 T7: `WaveSpawner.spawnEffect` applies `unmoving` (30) and
/// `invincible` (60) to every spawned unit (WaveSpawner.java:228-235).
#[test]
fn spawned_wave_unit_is_invincible() {
    use crate::entities::comp::unit::comp::StatusComp;

    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    let spawn = block_id(&sim, "spawn");
    let index = sim.grid.tiles.index(10, 10);
    sim.grid.tiles.geti_mut(index).overlay = BlockId::new(spawn);
    sim.set_unit_wave_team(2);
    sim.request_wave_spawn(0);
    sim.tick().expect("tick");
    let runtime = sim.unit_runtime().expect("rt");
    let entity = *runtime.units.first().expect("wave unit spawned");
    let unmoving = runtime.content.status_id("unmoving").expect("unmoving");
    let invincible = runtime.content.status_id("invincible").expect("invincible");
    let statuses = sim
        .ecs
        .0
        .get::<StatusComp>(entity)
        .expect("status component");
    let duration = |status| {
        statuses
            .statuses
            .iter()
            .find(|entry| entry.effect == status)
            .map(|entry| entry.duration)
    };
    let unmoving_time = duration(unmoving).expect("unmoving applied");
    let invincible_time = duration(invincible).expect("invincible applied");
    assert!(
        (28.0..=30.0).contains(&unmoving_time),
        "unmoving for 30 ticks, {unmoving_time} left"
    );
    assert!(
        (58.0..=60.0).contains(&invincible_time),
        "invincible for 60 ticks, {invincible_time} left"
    );
}

/// Uninstalled runtime: the wave request and systems are inert (P0 spine).
#[test]
fn no_runtime_is_a_noop() {
    let mut sim = Sim::new(1, 16, 16, BlockId::AIR, BlockId::AIR);
    sim.request_wave_spawn(0);
    sim.tick().expect("tick");
    assert_eq!(sim.group_counts().get("unit"), Some(&0));
    assert_eq!(sim.group_counts().get("bullet"), Some(&0));
    assert!(!sim.has_unit_runtime());
}

/// Two identical live worlds stay bit-identical (position/health/checksum).
#[test]
fn live_units_are_deterministic() {
    fn run() -> String {
        let mut sim = Sim::new(7, 64, 64, BlockId::AIR, BlockId::AIR);
        install(&mut sim);
        let core = block_id(&sim, "core-shard");
        let spawn = block_id(&sim, "spawn");
        assert!(
            sim.command(SimCommand::Place {
                x: 50,
                y: 50,
                block: core,
                rotation: 0,
                team: 1,
                player: None,
            })
            .is_ok()
        );
        let index = sim.grid.tiles.index(8, 8);
        sim.grid.tiles.geti_mut(index).overlay = BlockId::new(spawn);
        sim.set_unit_wave_team(2);
        sim.request_wave_spawn(0);
        for _ in 0..300 {
            sim.tick().expect("tick");
        }
        sim.unit_runtime_checksum().to_hex()
    }
    assert_eq!(run(), run());
}

/// `SimCommand::SpawnUnit` spawns through the live runtime when installed.
#[test]
fn spawn_unit_command_uses_the_live_runtime() {
    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_by_name("dagger")
        .expect("dagger")
        .id
        .get();
    assert!(
        sim.command(SimCommand::SpawnUnit {
            unit: dagger,
            x: 16.0,
            y: 16.0,
            team: 2,
        })
        .is_ok()
    );
    assert_eq!(sim.group_counts().get("unit"), Some(&1));
    assert!(sim.unit_runtime().expect("runtime").units_created == 1);
}

/// PAR-0140: a wire `UnitCommand` (`id` = `EntitySeq`) orders a live
/// default-team unit; other teams and the relay path stay untouched.
#[test]
fn live_unit_command_sets_a_move_target() {
    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        rules.default_team = 1;
    }
    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_by_name("dagger")
        .expect("dagger")
        .id
        .get();
    sim.command(SimCommand::SpawnUnit {
        unit: dagger,
        x: 32.0,
        y: 32.0,
        team: 1,
    })
    .expect("spawn friendly");
    sim.command(SimCommand::SpawnUnit {
        unit: dagger,
        x: 64.0,
        y: 64.0,
        team: 2,
    })
    .expect("spawn enemy");
    let friendly = sim.unit_runtime().expect("runtime").units[0];
    let enemy = sim.unit_runtime().expect("runtime").units[1];
    let friendly_id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(friendly)
        .expect("seq")
        .0 as i32;
    let enemy_id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(enemy)
        .expect("seq")
        .0 as i32;

    // Move order on both ids: only the default-team unit accepts.
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![friendly_id, enemy_id],
        command: 0,
        x: 200.0,
        y: 200.0,
        queue: false,
    })
    .expect("command");
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::ai::controller::ControllerSlot>(friendly)
            .expect("slot")
            .target,
        Some(TilePos::new(25, 25)),
        "the friendly unit is ordered at the wire target tile"
    );
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::ai::controller::ControllerSlot>(enemy)
            .expect("slot")
            .target,
        None,
        "an enemy id is not commandable"
    );

    // The order drives movement.
    let before = sim.ecs.0.get::<Pos>(friendly).expect("pos").x;
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let after = sim.ecs.0.get::<Pos>(friendly).expect("pos").x;
    assert!(
        after > before,
        "the ordered unit moves toward the target ({before} -> {after})"
    );
}

/// I01 T13: `commandUnits(..., queueCommand)` (middle mouse) appends to the
/// unit's command queue when a target already exists; the queue head is the
/// current target and the next waypoint is popped on arrival
/// (`CommandAI.commandQueue`/`finishPath`, `CommandAI.java:476-514`).
#[test]
fn queued_orders_execute_in_sequence() {
    use crate::ai::types::command::CommandQueueEntry;

    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        rules.default_team = 1;
    }
    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_by_name("dagger")
        .expect("dagger")
        .id
        .get();
    sim.command(SimCommand::SpawnUnit {
        unit: dagger,
        x: 32.0,
        y: 32.0,
        team: 1,
    })
    .expect("spawn");
    let unit = sim.unit_runtime().expect("runtime").units[0];
    let id = sim.ecs.0.get::<crate::ecs::EntitySeq>(unit).expect("seq").0 as i32;
    let readback = |sim: &Sim| {
        let slot = sim
            .ecs
            .0
            .get::<crate::ai::controller::ControllerSlot>(unit)
            .expect("slot");
        (
            slot.target,
            sim.ecs
                .0
                .get::<crate::ai::types::command::CommandQueueComp>(unit)
                .map(|queued| queued.0.clone())
                .unwrap_or_default(),
        )
    };

    // First queued order: no target exists, so it applies directly
    // (`CommandAI.commandQueue` head rule). Target = the unit's own tile.
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 32.0,
        y: 32.0,
        queue: true,
    })
    .expect("first queued order");
    let (target, queued) = readback(&sim);
    assert_eq!(target, Some(TilePos::new(4, 4)), "head applies directly");
    assert!(queued.is_empty(), "nothing queued yet");

    // Second queued order: a target exists, so it appends.
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 128.0,
        y: 128.0,
        queue: true,
    })
    .expect("second queued order");
    let (target, queued) = readback(&sim);
    assert_eq!(target, Some(TilePos::new(4, 4)), "head unchanged");
    assert_eq!(
        queued,
        vec![CommandQueueEntry::Position(128.0, 128.0)],
        "second order appended"
    );

    // The unit already stands on the head: one tick arrives at it and pops
    // the next waypoint (`CommandAI.finishPath`).
    sim.tick().expect("tick");
    let (target, queued) = readback(&sim);
    assert_eq!(
        target,
        Some(TilePos::new(16, 16)),
        "the queued waypoint becomes the target"
    );
    assert!(queued.is_empty(), "queue drained in order");

    // A direct order clears a non-empty queue (`ai.commandQueue.clear()`).
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 64.0,
        y: 64.0,
        queue: true,
    })
    .expect("queued again");
    assert_eq!(readback(&sim).1.len(), 1);
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x: 96.0,
        y: 96.0,
        queue: false,
    })
    .expect("direct order");
    let (target, queued) = readback(&sim);
    assert_eq!(target, Some(TilePos::new(12, 12)), "direct order applied");
    assert!(queued.is_empty(), "direct order clears the queue");
}

/// PAR-0140: the selectable snapshot lists live units by `EntitySeq` with
/// the player-team `commandable` flag.
#[test]
fn selectable_units_json_lists_live_units() {
    let mut sim = Sim::new(1, 32, 32, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        rules.default_team = 1;
    }
    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_by_name("dagger")
        .expect("dagger")
        .id
        .get();
    sim.command(SimCommand::SpawnUnit {
        unit: dagger,
        x: 32.0,
        y: 32.0,
        team: 1,
    })
    .expect("spawn friendly");
    sim.command(SimCommand::SpawnUnit {
        unit: dagger,
        x: 64.0,
        y: 64.0,
        team: 2,
    })
    .expect("spawn enemy");
    let friendly = sim.unit_runtime().expect("runtime").units[0];
    let friendly_id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(friendly)
        .expect("seq")
        .0 as i32;

    let rows: serde_json::Value = serde_json::from_str(&sim.selectable_units_json()).expect("json");
    let rows = rows.as_array().expect("array");
    assert_eq!(rows.len(), 2, "both live units are listed");
    let friendly_row = rows
        .iter()
        .find(|row| row["id"].as_i64() == Some(i64::from(friendly_id)))
        .expect("friendly row");
    assert_eq!(friendly_row["team"].as_u64(), Some(1));
    assert_eq!(friendly_row["commandable"].as_bool(), Some(true));
    assert!(
        rows.iter()
            .any(|row| row["commandable"].as_bool() == Some(false)),
        "the enemy unit is listed but not commandable"
    );
}

/// H08 T6a: selectable rows carry `payload` (`instanceof Payloadc`) and
/// `select_global` (`UnitType.controlSelectGlobal`, false only for mono).
#[test]
fn selectable_rows_carry_payload_and_select_global() {
    let (mut sim, dagger) = player_team_world(32);
    let type_id = |sim: &Sim, name: &str| {
        sim.unit_runtime()
            .expect("runtime")
            .content
            .unit_by_name(name)
            .expect(name)
            .id
            .get()
    };
    let mega = type_id(&sim, "mega");
    let mono = type_id(&sim, "mono");
    for unit in [dagger, mega, mono] {
        sim.command(SimCommand::SpawnUnit {
            unit,
            x: 32.0,
            y: 32.0,
            team: 1,
        })
        .expect("spawn");
    }
    let rows: serde_json::Value = serde_json::from_str(&sim.selectable_units_json()).expect("json");
    let rows = rows.as_array().expect("array");
    let row = |type_id: u16| {
        rows.iter()
            .find(|row| row["type"].as_i64() == Some(i64::from(type_id)))
            .unwrap_or_else(|| panic!("row for {type_id}"))
    };
    assert_eq!(row(mega)["payload"].as_bool(), Some(true), "mega payload");
    assert_eq!(
        row(dagger)["payload"].as_bool(),
        Some(false),
        "dagger has no payload module"
    );
    assert_eq!(
        row(mono)["select_global"].as_bool(),
        Some(false),
        "mono excluded from select-all"
    );
    assert_eq!(row(dagger)["select_global"].as_bool(), Some(true));
}

/// Live world with a block runtime and `defaultTeam = 1`; returns the
/// dagger content id.
fn player_team_world(size: i32) -> (Sim, u16) {
    let mut sim = Sim::new(1, size, size, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        rules.default_team = 1;
    }
    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_by_name("dagger")
        .expect("dagger")
        .id
        .get();
    (sim, dagger)
}

fn spawn_dagger(sim: &mut Sim, dagger: u16, x: f32, y: f32, team: u8) -> (Entity, i32) {
    sim.command(SimCommand::SpawnUnit {
        unit: dagger,
        x,
        y,
        team,
    })
    .expect("spawn");
    let entity = *sim
        .unit_runtime()
        .expect("runtime")
        .units
        .last()
        .expect("spawned");
    let id = sim.ecs.0.get::<EntitySeq>(entity).expect("seq").0 as i32;
    (entity, id)
}

fn move_order(sim: &mut Sim, id: i32, x: f32, y: f32) {
    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![id],
        command: 0,
        x,
        y,
        queue: false,
    })
    .expect("command");
}

/// PAR-0140 (UnitType.java:283): a player-team dagger gets `CommandAI`
/// and idles without an order; a wave-team dagger keeps `GroundAI`.
#[test]
fn player_team_units_get_command_ai_and_idle() {
    let (mut sim, dagger) = player_team_world(48);
    let (friendly, _) = spawn_dagger(&mut sim, dagger, 160.0, 160.0, 1);
    let (enemy, _) = spawn_dagger(&mut sim, dagger, 320.0, 320.0, 2);
    let kind = |sim: &Sim, entity| {
        sim.ecs
            .0
            .get::<crate::ai::controller::ControllerSlot>(entity)
            .expect("slot")
            .kind
    };
    assert_eq!(kind(&sim, friendly), AiKind::Command);
    assert_eq!(kind(&sim, enemy), AiKind::Ground, "AI team keeps GroundAI");

    let before = *sim.ecs.0.get::<Pos>(friendly).expect("pos");
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let after = *sim.ecs.0.get::<Pos>(friendly).expect("pos");
    assert_eq!(
        (before.x, before.y),
        (after.x, after.y),
        "an unordered CommandAI unit does not walk to the core anchor"
    );
}

/// PAR-0140: a MOVE order ~10 tiles away on open ground is reached
/// within the `CommandAI` arrival radius and the order clears.
#[test]
fn command_ai_move_order_arrives_and_clears() {
    let (mut sim, dagger) = player_team_world(48);
    let (unit, id) = spawn_dagger(&mut sim, dagger, 80.0, 80.0, 1);
    // Tile (20, 16): world (160, 128) (`World.unconv`).
    move_order(&mut sim, id, 160.0, 128.0);
    let slot = |sim: &Sim| {
        *sim.ecs
            .0
            .get::<crate::ai::controller::ControllerSlot>(unit)
            .expect("slot")
    };
    assert_eq!(slot(&sim).target, Some(TilePos::new(20, 16)));
    // Dagger speed 0.5 px/tick over ~89 px: well under 400 ticks.
    let mut arrived_at = None;
    for tick in 0..400 {
        sim.tick().expect("tick");
        if slot(&sim).target.is_none() {
            arrived_at = Some(tick);
            break;
        }
    }
    assert!(arrived_at.is_some(), "the order finished");
    let pos = *sim.ecs.0.get::<Pos>(unit).expect("pos");
    let dist = ((pos.x - 160.0).powi(2) + (pos.y - 128.0).powi(2)).sqrt();
    assert!(
        dist <= crate::ai::types::command::command_arrival_radius(8.0, false) + 1e-3,
        "arrived within the arrival radius ({dist})"
    );
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let rest = *sim.ecs.0.get::<Pos>(unit).expect("pos");
    assert_eq!((pos.x, pos.y), (rest.x, rest.y), "idles after arrival");
}

/// PAR-0140: ground orders route around a wall through the control
/// pathfinder instead of stalling against it.
#[test]
fn command_ai_ground_order_routes_around_walls() {
    let (mut sim, dagger) = player_team_world(48);
    let wall = block_id(&sim, "copper-wall");
    // A vertical wall at x = 20 spanning y = 4..=28, open above.
    for y in 4..=28 {
        assert!(
            sim.command(SimCommand::Place {
                x: 20,
                y,
                block: wall,
                rotation: 0,
                team: 1,
                player: None,
            })
            .is_ok(),
            "wall placed"
        );
    }
    let (unit, id) = spawn_dagger(&mut sim, dagger, 12.0 * 8.0, 16.0 * 8.0, 1);
    // Destination north of the wall's top end (row 4): the straight line
    // (12,16)->(28,4) crosses the wall at (20,10), so the order must route
    // around the north end. (A destination *south* of the wall end made the
    // unit round the corner at exactly its rounded-tile boundary, where the
    // T4 `EntityCollisions` box is caught by the wall corner — recorded as
    // the follow-up corner-jam item in the H01 summary.)
    move_order(&mut sim, id, 28.0 * 8.0, 4.0 * 8.0);
    let mut done = false;
    for _ in 0..3000 {
        sim.tick().expect("tick");
        let slot = sim
            .ecs
            .0
            .get::<crate::ai::controller::ControllerSlot>(unit)
            .expect("slot");
        if slot.target.is_none() {
            done = true;
            break;
        }
    }
    let pos = *sim.ecs.0.get::<Pos>(unit).expect("pos");
    assert!(done, "the detour order finished (at {}, {})", pos.x, pos.y);
    assert!(pos.x > 21.0 * 8.0, "crossed to the far side of the wall");
}

/// H01 T11 corner-jam regression (pass-2 open item): the original
/// `routes_around_walls` geometry — destination east of the wall on the
/// same row, so the detour rounds a wall end where the unit's rounded
/// tile flips. Covered by the T4 `line_clear`/`segment_clear` fixes;
/// eight sub-tile/end-row variants were re-probed and none jams
/// (2026-10-15, session 3).
#[test]
fn command_ai_ground_order_around_wall_mid_row() {
    let (mut sim, dagger) = player_team_world(48);
    let wall = block_id(&sim, "copper-wall");
    for y in 4..=28 {
        assert!(
            sim.command(SimCommand::Place {
                x: 20,
                y,
                block: wall,
                rotation: 0,
                team: 1,
                player: None,
            })
            .is_ok(),
            "wall placed"
        );
    }
    let (unit, id) = spawn_dagger(&mut sim, dagger, 12.0 * 8.0, 16.0 * 8.0, 1);
    move_order(&mut sim, id, 28.0 * 8.0, 16.0 * 8.0);
    let mut done = false;
    let mut last = (0.0, 0.0);
    for _ in 0..6000 {
        sim.tick().expect("tick");
        let slot = sim
            .ecs
            .0
            .get::<crate::ai::controller::ControllerSlot>(unit)
            .expect("slot");
        if let Some(pos) = sim.ecs.0.get::<Pos>(unit) {
            last = (pos.x, pos.y);
        }
        if slot.target.is_none() {
            done = true;
            break;
        }
    }
    assert!(done, "the wall-end order finished (at {:?})", last);
    assert!(last.0 > 21.0 * 8.0, "crossed past the wall (at {:?})", last);
}

/// PAR-0140: the player's own unit (`Player` controller) is listed but
/// not commandable, and a wire order naming it is ignored.
#[test]
fn player_unit_is_not_commandable() {
    let (mut sim, dagger) = player_team_world(32);
    let player = sim
        .spawn_player_unit("alpha", 1, 64.0, 64.0, 0.0)
        .expect("player unit");
    let player_id = sim.ecs.0.get::<EntitySeq>(player).expect("seq").0 as i32;
    let (friendly, friendly_id) = spawn_dagger(&mut sim, dagger, 96.0, 96.0, 1);

    let rows: serde_json::Value = serde_json::from_str(&sim.selectable_units_json()).expect("json");
    let row = |id: i32| {
        rows.as_array()
            .expect("array")
            .iter()
            .find(|row| row["id"].as_i64() == Some(i64::from(id)))
            .cloned()
            .expect("row")
    };
    assert_eq!(row(player_id)["commandable"].as_bool(), Some(false));
    assert_eq!(row(friendly_id)["commandable"].as_bool(), Some(true));

    sim.command(SimCommand::UnitCommand {
        units: smallvec::smallvec![player_id, friendly_id],
        command: 0,
        x: 200.0,
        y: 200.0,
        queue: false,
    })
    .expect("command");
    let target = |entity| {
        sim.ecs
            .0
            .get::<crate::ai::controller::ControllerSlot>(entity)
            .expect("slot")
            .target
    };
    assert_eq!(target(player), None, "the player unit ignores orders");
    assert_eq!(target(friendly), Some(TilePos::new(25, 25)));
}

/// The live campaign timer (`TickSet::Campaign` -> `runWave`) drives the
/// same spawn path and updates `state.enemies`/`spawner.isSpawning`.
#[test]
fn campaign_wave_timer_spawns_live_units() {
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
    let mut session = PlaySession::new(Rules {
        waves: true,
        wave_timer: true,
        wave_spacing: 0.05,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    session.spawn_count = 1;
    session.wavetime = 0.001;
    // Stable player core so `checkGameState` does not end the match.
    let core_entity = Entity::from_raw_u32(1).expect("entity");
    session
        .teams
        .register_core(core_entity, crate::ecs::TeamId(1), &session.rules);

    let mut sim = Sim::new(1, 48, 48, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    let spawn = block_id(&sim, "spawn");
    let index = sim.grid.tiles.index(5, 5);
    sim.grid.tiles.geti_mut(index).overlay = BlockId::new(spawn);
    sim.set_unit_wave_team(2);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    sim.refresh_unit_runtime();

    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let runtime = sim
        .ecs
        .0
        .remove_resource::<CampaignRuntime>()
        .expect("campaign runtime");
    assert!(runtime.session.wave >= 1, "wave timer fired");
    assert!(
        runtime.session.enemies >= 1,
        "live enemy count mirrored into the session"
    );
    assert!(
        sim.group_counts().get("unit").copied().unwrap_or(0) >= 1,
        "campaign timer spawned a live unit"
    );
    sim.ecs.0.insert_resource(runtime);
}

/// Attack-mode waves spawn near the wave team's cores (`WaveSpawner`
/// `wavesSpawnAtCores`; PAR-0139 prerequisite: attack sectors author no
/// spawn-overlay tiles, so enemy units only enter this way).
#[test]
fn attack_mode_waves_spawn_near_enemy_cores() {
    use crate::content::{MemoryBundle, MemoryUnlockStore, create_base_content};
    use crate::ecs::BuildingComp;
    use crate::ecs::TeamId;
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::block::BlockTable as Table;

    let registry = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let campaign = Campaign::from_registry(&registry, &EmptyNeighborhood);
    let mut session = PlaySession::new(Rules {
        waves: false,
        attack_mode: true,
        waves_spawn_at_cores: true,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;

    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");

    // Materialize a player core and the wave team's core (the spawn anchor).
    let table: Table = sim.ecs.0.get_resource::<Table>().expect("table").clone();
    let core_inst = table.get_named("core-shard").expect("core-shard").clone();
    let core_block = registry.block_id("core-shard").expect("core-shard id");
    let spawn_core = |sim: &mut Sim, seq: u64, tile: TilePos, team: u8| -> Entity {
        let entity = core_inst.spawn(
            &mut sim.ecs.0,
            seq,
            tile,
            team,
            0,
            registry.items().len(),
            registry.liquids().len(),
        );
        sim.ecs.0.entity_mut(entity).insert(BuildingComp {
            pos: tile,
            block: core_block,
            team: TeamId(team),
            rot: 0,
        });
        entity
    };
    let player_core = spawn_core(&mut sim, 10, TilePos::new(12, 32), 1);
    let enemy_core = spawn_core(&mut sim, 11, TilePos::new(48, 32), 2);
    session
        .teams
        .register_core(player_core, TeamId(1), &session.rules);
    session
        .teams
        .register_core(enemy_core, TeamId(2), &session.rules);

    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    sim.set_unit_wave_team(2);
    sim.refresh_unit_runtime();
    sim.request_wave_spawn(0);
    for _ in 0..10 {
        sim.tick().expect("tick");
    }

    let ts = crate::config::TILESIZE as f32;
    let runtime = sim.unit_runtime().expect("runtime");
    let mut spawned_near_core = 0;
    for &entity in &runtime.units {
        if let Some(pos) = sim.ecs.0.get::<Pos>(entity)
            && let Some(team) = sim.ecs.0.get::<TeamComp>(entity)
            && team.team == 2
            && pos.x > 32.0 * ts
        {
            spawned_near_core += 1;
        }
    }
    assert!(
        spawned_near_core >= 1,
        "wave units spawned near the enemy core (units: {}, near: {})",
        runtime.units.len(),
        spawned_near_core
    );
}

/// H01 T8: `AIController.targetFlagActive` accepts a building whose Java
/// `status()` is not `noInput` even with no items
/// (`((items != null && items.any()) || status() != noInput)`,
/// `AIController.java:277-284`; `BuildingComp.java:638-652`): an empty
/// `vault` (storage flag, efficiency 1.0 -> active pulse) is a target, an
/// unpowered one (efficiency 0 -> noInput) is not.
#[test]
fn target_flag_active_uses_java_block_status() {
    use crate::entities::comp::Building;

    let mut sim = Sim::new(1, 64, 64, BlockId::AIR, BlockId::AIR);
    install(&mut sim);
    let vault = block_id(&sim, "vault");
    assert!(
        sim.command(SimCommand::Place {
            x: 20,
            y: 20,
            block: vault,
            rotation: 0,
            team: 1,
            player: None,
        })
        .is_ok(),
        "vault placed"
    );
    let entity = sim.grid.tiles.get(20, 20).build.expect("vault built");
    let runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("runtime");
    let queries = TargetQueries::build(&mut sim.ecs.0, &runtime.content);
    let (x, y) = (20.5 * TILE_SIZE, 20.5 * TILE_SIZE);
    let flag = crate::content::BlockFlag::Storage;
    assert_eq!(
        target_flag_active(&sim.ecs.0, &queries, 2, x, y, flag),
        Some(entity),
        "an empty vault with efficiency 1.0 is active (status != noInput)"
    );
    // Blocked/unpowered: efficiency 0 -> noInput, not a target.
    sim.ecs
        .0
        .get_mut::<Building>(entity)
        .expect("building")
        .efficiency = 0.0;
    assert_eq!(
        target_flag_active(&sim.ecs.0, &queries, 2, x, y, flag),
        None,
        "efficiency 0 is noInput (not a target)"
    );
    sim.ecs.0.insert_non_send(runtime);
}

/// GAPS #67: `Block.targetable` (`Block.java:308`) is false only for the
/// four `worldProcessorOnly` world blocks (`Blocks.java:7002,7014,7023,7030`).
#[test]
fn world_logic_blocks_are_not_targetable() {
    use crate::world::TilePos;
    use crate::world::harness::BuildHarness;

    let mut harness = BuildHarness::new(8, 8, 1);
    let spawn = |harness: &mut BuildHarness, name: &str| {
        let inst = harness.table().get_named(name).expect(name).clone();
        let (items, liquids) = (
            harness.content.items().len(),
            harness.content.liquids().len(),
        );
        let entity = inst.spawn(
            &mut harness.world,
            0,
            TilePos::new(2, 2),
            0,
            0,
            items,
            liquids,
        );
        inst.behavior.create_state(&mut harness.world, entity);
        entity
    };
    let processor = spawn(&mut harness, "world-processor");
    let cell = spawn(&mut harness, "world-cell");
    let wall = spawn(&mut harness, "copper-wall");
    assert!(!block_targetable(&harness.world, processor));
    assert!(!block_targetable(&harness.world, cell));
    assert!(block_targetable(&harness.world, wall));
}

/// GAPS #10: `LegsComp.java:196-202` — a legged unit deals
/// `legSplashDamage` in `legSplashRange` at a stepping foot's base.
#[test]
fn legged_unit_splash_damages_nearby_enemy_units() {
    use crate::ai::UnitHarness;
    use crate::entities::comp::Health;
    use crate::entities::comp::TeamComp;
    use crate::entities::comp::unit::LegsComp;

    let mut harness = UnitHarness::new(32, 32, 7);
    let (x, y) = crate::world::BuildHarness::tile_center(10, 10);
    let arkyid = harness.spawn("arkyid", 2, x, y, 0.0).expect("arkyid");
    let target = harness.spawn("dagger", 1, x + 1.0, y, 0.0).expect("dagger");
    let Some(mut before) = harness.build.world.get::<LegsComp>(arkyid).cloned() else {
        panic!("arkyid carries LegsComp");
    };
    // Pretend every foot re-stepped this tick.
    for leg in &mut before.legs {
        leg.foot_x += 100.0;
    }
    let health_before = harness
        .build
        .world
        .get::<Health>(target)
        .map(|health| health.health)
        .unwrap_or(0.0);
    apply_movement_damage(
        &mut harness.build.world,
        &harness.build.grid,
        &harness.build.content,
        arkyid,
        (0.0, 0.0),
        Some(&before),
    );
    let health_after = harness
        .build
        .world
        .get::<Health>(target)
        .map(|health| health.health)
        .unwrap_or(0.0);
    assert!(
        health_after < health_before,
        "leg splash hit the enemy unit ({health_after} < {health_before})"
    );
    // A disarmed legged unit deals no splash damage.
    harness
        .build
        .world
        .get_mut::<crate::entities::comp::unit::comp::StatusComp>(arkyid)
        .expect("status comp")
        .disarmed = true;
    let health = harness.build.world.get::<Health>(target).unwrap().health;
    apply_movement_damage(
        &mut harness.build.world,
        &harness.build.grid,
        &harness.build.content,
        arkyid,
        (0.0, 0.0),
        Some(&before),
    );
    assert_eq!(
        harness.build.world.get::<Health>(target).unwrap().health,
        health,
        "disarmed units cannot splash"
    );
    assert_eq!(
        harness.build.world.get::<TeamComp>(target).map(|t| t.team),
        Some(1)
    );
}

/// GAPS #10: `CrawlComp.java:56-93` — a moving crawler damages enemy
/// buildings under its crawl circle, idle crawlers do not.
#[test]
fn crawler_crushes_enemy_buildings_while_moving() {
    use crate::ai::UnitHarness;
    use crate::entities::comp::Health;
    use crate::entities::comp::TeamComp;

    let mut harness = UnitHarness::new(32, 32, 7);
    let wall = harness
        .build
        .content()
        .block_id("copper-wall")
        .expect("copper-wall");
    assert!(harness.build.place(10, 10, wall, 0, true));
    let build = harness.build.build_at(10, 10).expect("wall entity");
    let building_team = harness
        .build
        .world
        .get::<TeamComp>(build)
        .map(|team| team.team)
        .unwrap_or(0);
    let unit_team = if building_team == 1 { 2 } else { 1 };
    let (x, y) = crate::world::BuildHarness::tile_center(10, 10);
    let latum = harness.spawn("latum", unit_team, x, y, 0.0).expect("latum");
    let before = harness
        .build
        .world
        .get::<Health>(build)
        .map(|health| health.health)
        .unwrap_or(0.0);
    apply_movement_damage(
        &mut harness.build.world,
        &harness.build.grid,
        &harness.build.content,
        latum,
        (1.0, 0.0),
        None,
    );
    let moving = harness
        .build
        .world
        .get::<Health>(build)
        .map(|health| health.health)
        .unwrap_or(0.0);
    assert!(
        moving < before,
        "moving latum crushed the wall ({moving} < {before})"
    );
    apply_movement_damage(
        &mut harness.build.world,
        &harness.build.grid,
        &harness.build.content,
        latum,
        (0.0, 0.0),
        None,
    );
    assert_eq!(
        harness.build.world.get::<Health>(build).unwrap().health,
        moving,
        "an idle crawler deals no crush damage"
    );
}
