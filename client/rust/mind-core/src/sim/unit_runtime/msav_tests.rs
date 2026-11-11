// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Test-only MSAV import suites for the live unit runtime.
//! Upstream: none (port-only test support).

use super::*;
use crate::content::{BlockId, MemoryBundle, MemoryUnlockStore, create_base_content};
use crate::editor::context::EditorContext;
use crate::io::save::{SaveIo, SaveReadState};
use crate::sim::test_support::{install_live_runtimes, tick_n};
use crate::world::building_io::DecodedBase;
use std::path::Path;

/// Evaluator repro over the vendored Ground Zero preset: a real sector with
/// its core materialized spawns wave units at the map's `spawn` overlay.
#[test]
fn ground_zero_wave_spawns_live_units() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo/groundZero.msav");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let pending: Vec<(usize, DecodedBase)> = {
        let mut context = EditorContext::new(&mut grid, &content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut content),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
        std::mem::take(&mut context.pending_buildings)
    };
    grid.end_map_load(&content);
    let spawn = content.block_id("spawn").expect("spawn");
    let spawns = grid
        .tiles
        .array()
        .iter()
        .filter(|tile| tile.overlay == spawn)
        .count();
    assert!(spawns > 0, "groundZero carries spawn overlay tiles");

    let (width, height) = (grid.tiles.width, grid.tiles.height);
    let air = BlockId::AIR;
    let mut sim = Sim::new(1, width, height, air, air);
    sim.grid = grid;
    install_live_runtimes(&mut sim);
    assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");
    sim.set_unit_wave_team(2);
    sim.request_wave_spawn(0);
    tick_n(&mut sim, 120);
    assert!(
        sim.group_counts().get("unit").copied().unwrap_or(0) >= 1,
        "groundZero wave spawned a live unit: {:?}",
        sim.group_counts()
    );
}

/// H06 T20: the map's world processors must assemble with the logic globals
/// present (the fixed engine load order installs `GlobalVars` before the
/// building decode), so no executor keeps a non-constant `@`-named cell
/// (`@counter` excepted: `LAssembler.var` resolves `@sharded`/`@duo`/...
/// from `Vars.logicVars`, `J/logic/LAssembler.java:59-61`).
#[test]
fn ground_zero_processors_resolve_constants() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo/groundZero.msav");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let pending: Vec<(usize, DecodedBase)> = {
        let mut context = EditorContext::new(&mut grid, &content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut content),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
        std::mem::take(&mut context.pending_buildings)
    };
    grid.end_map_load(&content);

    let (width, height) = (grid.tiles.width, grid.tiles.height);
    let air = BlockId::AIR;
    let mut sim = Sim::new(1, width, height, air, air);
    sim.grid = grid;
    install_live_runtimes(&mut sim);
    // Fixed load order: the globals exist before the map is decoded
    // (`Vars.init` before any map read, `Vars.java:369`).
    crate::logic::globals::GlobalVars::install_world(&mut sim.ecs.0, &content);
    assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");

    let mut checked = 0;
    for (_seq, entity, _comp) in sim.ecs.entities_by_seq() {
        let Some(state) = sim
            .ecs
            .0
            .get::<crate::logic::blocks::LogicBlockState>(entity)
        else {
            continue;
        };
        checked += 1;
        for id in &state.executor.var_ids {
            let cell = state.executor.arena.get(*id);
            assert!(
                cell.constant || !cell.name.starts_with('@') || cell.name == "@counter",
                "processor {entity:?} kept unresolved `{}`",
                cell.name
            );
        }
    }
    assert!(
        checked >= 5,
        "groundZero ships 5 world processors, linked {checked}"
    );
}

/// T15: the Ground Zero wave-1 dagger walks around the deep pool near
/// world (1088, 1100) instead of entering it and drowning
/// (`Pathfinder.costGround`: deep `+6000`, all-deep impassable,
/// Pathfinder.java:53-62; `getTargetTile` refuses a dangerous step).
#[test]
fn ground_zero_wave_dagger_avoids_deep_water() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo/groundZero.msav");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let pending: Vec<(usize, DecodedBase)> = {
        let mut context = EditorContext::new(&mut grid, &content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut content),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
        std::mem::take(&mut context.pending_buildings)
    };
    grid.end_map_load(&content);

    let (width, height) = (grid.tiles.width, grid.tiles.height);
    let air = BlockId::AIR;
    let mut sim = Sim::new(1, width, height, air, air);
    sim.grid = grid;
    install_live_runtimes(&mut sim);
    assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");

    // Per-tile deep map + the `wet` status id, read before the tick loop
    // (the runtime borrow must end before `sim.tick`).
    let (deep, wet, dagger_id) = {
        let runtime = sim.unit_runtime().expect("runtime");
        let deep: Vec<bool> = (0..sim.grid.tiles.len())
            .map(|index| {
                let tile = sim.grid.tiles.geti(index);
                tile.block == BlockId::AIR
                    && runtime
                        .content
                        .block(tile.floor)
                        .is_some_and(|def| def.is_deep)
            })
            .collect();
        (
            deep,
            runtime.content.status_id("wet").expect("wet"),
            runtime.content.unit_id("dagger").expect("dagger"),
        )
    };
    assert!(deep.iter().any(|&d| d), "groundZero has deep water");

    sim.set_unit_wave_team(2);
    sim.request_wave_spawn(0);
    let mut max_drown = 0.0f32;
    let mut wet_samples = 0usize;
    let mut sampled = 0usize;
    for tick in 0..3000 {
        sim.tick().expect("tick");
        if tick % 30 != 0 {
            continue;
        }
        for entity in sim.ecs.0.iter_entities() {
            if entity.get::<TeamComp>().map(|t| t.team) != Some(2) {
                continue;
            }
            if entity.get::<UnitTypeComp>().map(|u| u.type_id) != Some(dagger_id) {
                continue;
            }
            let Some(pos) = entity.get::<Pos>().copied() else {
                continue;
            };
            sampled += 1;
            let (tx, ty) = (
                crate::world::WorldGrid::to_tile(pos.x),
                crate::world::WorldGrid::to_tile(pos.y),
            );
            if let Some(&is_deep) = deep.get(sim.grid.tiles.index(tx, ty)) {
                assert!(
                    !is_deep,
                    "dagger entered deep water at tick {tick} ({tx}, {ty})"
                );
            }
            if let Some(drown) = entity.get::<crate::entities::comp::unit::environment::DrownComp>()
            {
                max_drown = max_drown.max(drown.0);
            }
            if entity
                .get::<crate::entities::comp::unit::comp::StatusComp>()
                .is_some_and(|status| status.has_effect_of(wet))
            {
                wet_samples += 1;
            }
        }
    }
    assert!(sampled > 0, "the wave-1 dagger was sampled");
    assert_eq!(max_drown, 0.0, "the dagger never started drowning");
    eprintln!("T15 probe: samples={sampled} wet={wet_samples} max_drown={max_drown}");
}

/// T2b: resuming at wave 1 must spawn `getSpawned(0)` BEFORE the counter
/// increments (`Logic.java:319-321`, `WaveSpawner.java:74`). Ground Zero's
/// wave-1 group is ONE dagger; the flare group has `begin: 1`.
#[test]
fn ground_zero_wave_one_is_one_dagger() {
    use crate::ecs::TeamId;
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo/groundZero.msav");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let pending: Vec<(usize, DecodedBase)> = {
        let mut context = EditorContext::new(&mut grid, &content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut content),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
        std::mem::take(&mut context.pending_buildings)
    };
    grid.end_map_load(&content);

    let (width, height) = (grid.tiles.width, grid.tiles.height);
    let air = BlockId::AIR;
    let mut sim = Sim::new(1, width, height, air, air);
    sim.grid = grid;
    install_live_runtimes(&mut sim);
    assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");

    let campaign = {
        let content = &sim.unit_runtime().expect("runtime").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        waves: true,
        wave_timer: true,
        wave_spacing: 1000.0,
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
        .register_core(core_entity, TeamId(1), &session.rules);
    assert_eq!(session.wave, 1, "resumed session starts at wave 1");
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    tick_n(&mut sim, 120);
    let runtime = sim
        .ecs
        .0
        .remove_resource::<CampaignRuntime>()
        .expect("campaign runtime");
    assert_eq!(runtime.session.wave, 2, "wave 1 fired and incremented");
    sim.ecs.0.insert_resource(runtime);

    let dagger = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_id("dagger")
        .expect("dagger");
    let mut wave_units: Vec<u16> = Vec::new();
    for entity in sim.ecs.0.iter_entities() {
        if entity.get::<TeamComp>().map(|t| t.team) != Some(2) {
            continue;
        }
        if let Some(unit_type) = entity.get::<UnitTypeComp>() {
            wave_units.push(unit_type.type_id.raw());
        }
    }
    assert_eq!(
        wave_units,
        vec![dagger.raw()],
        "wave 1 is exactly one dagger (getSpawned(0))"
    );
}

/// PAR-0086/PAR-0141 end-to-end on Ground Zero: a mechanical drill on
/// copper ore feeds an adjacent duo, which shoots down wave daggers.
#[test]
fn ground_zero_drill_fed_duo_kills_wave_daggers() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo/groundZero.msav");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let pending: Vec<(usize, DecodedBase)> = {
        let mut context = EditorContext::new(&mut grid, &content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut content),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
        std::mem::take(&mut context.pending_buildings)
    };
    grid.end_map_load(&content);
    let ore = content.block_id("ore-copper").expect("ore");
    let (width, height) = (grid.tiles.width, grid.tiles.height);
    let air = BlockId::AIR;
    let mut sim = Sim::new(1, width, height, air, air);
    sim.grid = grid;
    install_live_runtimes(&mut sim);
    assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");
    let core_block = content.block_id("core-shard").expect("core");
    let core = sim
        .grid
        .tiles
        .array()
        .iter()
        .find(|t| t.block == core_block)
        .map(|t| (i32::from(t.x), i32::from(t.y)))
        .expect("core tile");
    // A free 2x2 copper patch (drill) with a free tile to its right (duo).
    let free = |sim: &Sim, x: i32, y: i32| {
        sim.grid
            .tiles
            .getn(x, y)
            .is_some_and(|t| t.block == air && t.build.is_none())
    };
    let is_ore =
        |sim: &Sim, x: i32, y: i32| sim.grid.tiles.getn(x, y).is_some_and(|t| t.overlay == ore);
    let mut site = None;
    'search: for r in 2..40 {
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (core.0 + dx, core.1 + dy);
                let cells = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)];
                if cells
                    .iter()
                    .all(|&(cx, cy)| free(&sim, cx, cy) && is_ore(&sim, cx, cy))
                    && free(&sim, x + 2, y)
                {
                    site = Some((x, y));
                    break 'search;
                }
            }
        }
    }
    let (dx, dy) = site.expect("copper patch near the core");
    eprintln!(
        "ground zero core {core:?} drill site ({dx}, {dy}) duo ({}, {dy})",
        dx + 2
    );
    let drill = content.block_id("mechanical-drill").expect("drill").get();
    let duo = content.block_id("duo").expect("duo").get();
    for (x, y, block) in [(dx, dy, drill), (dx + 2, dy, duo)] {
        sim.command(crate::determinism::SimCommand::Place {
            x: x as i16,
            y: y as i16,
            block,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("place");
    }
    let duo_entity = sim.grid.tiles.get(dx + 2, dy).build.expect("duo built");
    // Let the drill stock the duo (mechanical drill ~ one copper / 9 s).
    tick_n(&mut sim, 1800);
    let ammo = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo_entity)
        .map(|s| s.total_ammo)
        .unwrap_or(0);
    assert!(ammo > 0, "the drill fed copper ammo into the duo");
    let (px, py) = (
        (dx + 2) as f32 * TILE_SIZE + 4.0,
        dy as f32 * TILE_SIZE + 4.0,
    );
    let dagger = sim
        .spawn_live_unit("dagger", 2, px + 70.0, py, 180.0)
        .expect("dagger");
    for _ in 0..1200 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
    }
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "the drill-fed duo killed the dagger (ammo {ammo}, health {:?})",
        sim.ecs.0.get::<Health>(dagger).map(|h| h.health)
    );
}

/// §7 (PAR-0139-adjacent): an undefended Ground Zero core with low health is
/// destroyed by wave daggers and the campaign reports game over.
#[test]
fn ground_zero_wave_destroys_undefended_core() {
    use crate::ecs::TeamId;
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo/groundZero.msav");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let pending: Vec<(usize, DecodedBase)> = {
        let mut context = EditorContext::new(&mut grid, &content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut content),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
        std::mem::take(&mut context.pending_buildings)
    };
    grid.end_map_load(&content);
    let (width, height) = (grid.tiles.width, grid.tiles.height);
    let air = BlockId::AIR;
    let mut sim = Sim::new(1, width, height, air, air);
    sim.grid = grid;
    install_live_runtimes(&mut sim);
    assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");
    // Bullet-vs-building collision reads the runtime grid snapshot; refresh
    // it after materializing the map's buildings (bullets otherwise fly
    // through them).
    sim.refresh_unit_runtime();
    let core_block = content.block_id("core-shard").expect("core");
    let (core_x, core_y, core) = sim
        .grid
        .tiles
        .array()
        .iter()
        .find(|t| t.block == core_block)
        .and_then(|t| t.build.map(|build| (i32::from(t.x), i32::from(t.y), build)))
        .expect("core tile + entity");
    // Keep the fight short (`Health.health = 50`); the check needs the core
    // to die, not the whole base to fall.
    sim.ecs
        .0
        .get_mut::<Health>(core)
        .expect("core health")
        .health = 50.0;

    let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
    let mut session = PlaySession::new(Rules {
        // `waves: true`: the sector is not captured, so the wave-team
        // daggers live (`Sector.isCaptured()` = `!waves && !attackMode`,
        // `Sector.java:181-184`; `UnitComp.java:757`).
        waves: true,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    // A sector makes `is_campaign()` true: wave-team units get their AI
    // (`GroundAI`) instead of idling `CommandAI`, and `checkGameState`
    // reports the sector loss when the core dies.
    session.sector = content.planet_id("serpulo").map(|planet| (planet, 170));
    session.teams.register_core(core, TeamId(1), &session.rules);
    // The hand-built session does not decode the map's spawn overlays;
    // `spawn_count` keeps `check_game_state`'s "no enemy spawns -> waves
    // disabled" rule from flipping the sector to captured, which would kill
    // the wave-team daggers (`Sector.isCaptured()` + `UnitComp.java:757`).
    session.spawn_count = 1;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    // Three wave daggers ~10 tiles from the core walk in and shoot it.
    // Find an open 3-tile column on passable terrain (the crater walls are
    // solid rock; the tile-center deadlock at a boundary must be avoided).
    let free_tile = |sim: &Sim, x: i32, y: i32| {
        sim.grid
            .tiles
            .getn(x, y)
            .is_some_and(|t| !t.solid(&content) && t.build.is_none())
    };
    let mut spawn_column = None;
    'search: for r in 4..20 {
        for (dx, dy) in [
            (r, 0),
            (-r, 0),
            (0, r),
            (0, -r),
            (r, r),
            (r, -r),
            (-r, r),
            (-r, -r),
        ] {
            let (x, y) = (core_x + dx, core_y + dy);
            if (-1..=1).all(|offset| free_tile(&sim, x, y + offset)) {
                spawn_column = Some((x, y));
                break 'search;
            }
        }
    }
    let (sx, sy) = spawn_column.expect("open column near the core");
    eprintln!("wave spawn column ({sx}, {sy}) core ({core_x}, {core_y})");
    for offset in -1..=1 {
        let _ = sim
            .spawn_live_unit(
                "dagger",
                2,
                sx as f32 * TILE_SIZE + 4.0,
                (sy + offset) as f32 * TILE_SIZE + 4.5,
                180.0,
            )
            .expect("dagger");
    }
    let mut core_destroyed_at = None;
    for tick in 0..3000 {
        sim.tick().expect("tick");
        if sim.grid.tiles.get(core_x, core_y).build.is_none() {
            core_destroyed_at = Some(tick);
            break;
        }
    }
    // The campaign liveness sweep + `checkGameState` run on the tick after
    // the core is removed.
    tick_n(&mut sim, 10);
    let game_over = sim
        .ecs
        .0
        .get_resource::<CampaignRuntime>()
        .is_some_and(|runtime| runtime.session.game_over);
    eprintln!(
        "ground zero core destroyed at {core_destroyed_at:?}, game over {game_over}, units {:?}",
        sim.group_counts()
    );
    assert!(
        core_destroyed_at.is_some(),
        "the wave destroyed the undefended core"
    );
    assert!(game_over, "the campaign reported game over (sector lost)");
}

/// The authored attack map pins its spawn groups to the map's spawn
/// overlay tile with Arc `Point2.pack` order; the inverted `TilePos::pack`
/// silently dropped every pinned group (PAR-0139).
#[test]
fn hidden_225_pinned_spawn_matches_the_arc_pack_order() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo/hidden/225.msav");
    let bytes = std::fs::read(&path).expect("map");
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let rules = {
        let mut context = EditorContext::new(&mut grid, &content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut content),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("import");
        state.rules
    };
    let rules = rules.expect("rules");
    let spawn_id = content.block_id("spawn").expect("spawn");
    let spawn_tile = grid
        .tiles
        .array()
        .iter()
        .find(|tile| tile.overlay == spawn_id)
        .expect("authored spawn overlay");
    assert_eq!(
        (spawn_tile.x, spawn_tile.y),
        (95, 318),
        "the map's spawn overlay tile"
    );
    let game = SpawnGroup::from_json(&serde_json::to_value(&rules.spawns[0]).expect("json"))
        .expect("group");
    assert_eq!(
        game.spawn,
        TilePos::new(95, 318).pack(),
        "the authored pin matches the Arc pack order"
    );
}

/// H01 T14: the real Ground Zero wave-1 fight. The plan's acceptance: a
/// drill-fed `duo` next to the core kills the wave and the core survives.
/// Prints the wave composition and the survivors so a failure can be
/// recorded without tuning the fixture.
#[test]
fn ground_zero_wave_one_dies_to_drill_fed_duo() {
    use crate::ecs::TeamId;
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../assets/maps/serpulo/groundZero.msav");
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let mut content = create_base_content(&MemoryBundle::new(), &MemoryUnlockStore::new(), true)
        .expect("content");
    let mut grid = WorldGrid::new(0, 0);
    grid.begin_map_load();
    let pending: Vec<(usize, DecodedBase)> = {
        let mut context = EditorContext::new(&mut grid, &content);
        let mut state = SaveReadState {
            context: Some(&mut context),
            content: Some(&mut content),
            ..SaveReadState::default()
        };
        SaveIo::load_bytes(&bytes, &mut state).expect("map imports");
        std::mem::take(&mut context.pending_buildings)
    };
    grid.end_map_load(&content);
    let ore = content.block_id("ore-copper").expect("ore");
    let (width, height) = (grid.tiles.width, grid.tiles.height);
    let air = BlockId::AIR;
    let mut sim = Sim::new(1, width, height, air, air);
    sim.grid = grid;
    install_live_runtimes(&mut sim);
    assert!(sim.materialize_map_buildings(&pending) > 0, "core linked");
    // Bullet-vs-building collision reads the runtime grid snapshot.
    sim.refresh_unit_runtime();
    let core_block = content.block_id("core-shard").expect("core");
    let (core_x, core_y, core) = sim
        .grid
        .tiles
        .array()
        .iter()
        .find(|t| t.block == core_block)
        .and_then(|t| t.build.map(|build| (i32::from(t.x), i32::from(t.y), build)))
        .expect("core tile + entity");

    let campaign = Campaign::from_registry(&content, &EmptyNeighborhood);
    let mut session = PlaySession::new(Rules {
        // The sector is not captured, so the wave-team units live
        // (`Sector.isCaptured()` = `!waves && !attackMode`).
        waves: true,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.phase = State::Playing;
    session.sector = content.planet_id("serpulo").map(|planet| (planet, 170));
    session.teams.register_core(core, TeamId(1), &session.rules);
    // The hand-built session has no decoded spawn overlays; `spawn_count`
    // keeps `check_game_state` from flipping the sector to captured.
    session.spawn_count = 1;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    // A drill on a free 2x2 copper patch with a free tile to its right.
    let free = |sim: &Sim, x: i32, y: i32| {
        sim.grid
            .tiles
            .getn(x, y)
            .is_some_and(|t| t.block == air && t.build.is_none())
    };
    let is_ore =
        |sim: &Sim, x: i32, y: i32| sim.grid.tiles.getn(x, y).is_some_and(|t| t.overlay == ore);
    let mut site = None;
    'search: for r in 2..40 {
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (core_x + dx, core_y + dy);
                let cells = [(x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1)];
                if cells
                    .iter()
                    .all(|&(cx, cy)| free(&sim, cx, cy) && is_ore(&sim, cx, cy))
                    && free(&sim, x + 2, y)
                {
                    site = Some((x, y));
                    break 'search;
                }
            }
        }
    }
    let (dx, dy) = site.expect("copper patch near the core");
    let drill = content.block_id("mechanical-drill").expect("drill").get();
    let duo = content.block_id("duo").expect("duo").get();
    for (x, y, block) in [(dx, dy, drill), (dx + 2, dy, duo)] {
        sim.command(crate::determinism::SimCommand::Place {
            x: x as i16,
            y: y as i16,
            block,
            rotation: 0,
            team: 1,
            player: None,
        })
        .expect("place");
    }
    let duo_entity = sim.grid.tiles.get(dx + 2, dy).build.expect("duo built");
    tick_n(&mut sim, 1800);
    let ammo = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo_entity)
        .map(|state| state.total_ammo)
        .unwrap_or(0);
    assert!(ammo > 0, "the drill fed copper ammo into the duo");

    sim.request_wave_spawn(0);
    let mut wave_composition: Vec<String> = Vec::new();
    let mut ticks = 0usize;
    for tick in 0..6000 {
        sim.tick().expect("tick");
        ticks = tick;
        let runtime = sim.unit_runtime().expect("runtime");
        if tick < 5 {
            for &entity in &runtime.units {
                if sim.ecs.0.get::<TeamComp>(entity).map(|t| t.team) != Some(2) {
                    continue;
                }
                let name = sim
                    .ecs
                    .0
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
                    .map(|def| def.name.clone())
                    .unwrap_or_default();
                if !wave_composition.contains(&name) {
                    wave_composition.push(name);
                }
            }
        }
        let enemies = runtime
            .units
            .iter()
            .filter(|&&entity| {
                sim.ecs.0.get::<TeamComp>(entity).map(|t| t.team) == Some(2)
                    && sim
                        .ecs
                        .0
                        .get::<Health>(entity)
                        .is_none_or(|health| !health.dead)
            })
            .count();
        if enemies == 0 {
            break;
        }
    }
    eprintln!(
        "T14 wave composition {wave_composition:?} after {ticks} ticks; core {:?} duo {:?}",
        sim.ecs.0.get::<Health>(core).map(|health| health.health),
        sim.ecs
            .0
            .get::<Health>(duo_entity)
            .map(|health| health.health),
    );
    let runtime = sim.unit_runtime().expect("runtime");
    let alive_enemies: Vec<String> = runtime
        .units
        .iter()
        .filter_map(|&entity| {
            if sim.ecs.0.get::<TeamComp>(entity).map(|t| t.team) != Some(2) {
                return None;
            }
            let alive = sim
                .ecs
                .0
                .get::<Health>(entity)
                .is_none_or(|health| !health.dead);
            if !alive {
                return None;
            }
            Some(
                sim.ecs
                    .0
                    .get::<UnitTypeComp>(entity)
                    .and_then(|comp| runtime.content.unit(comp.type_id))
                    .map(|def| def.name.clone())
                    .unwrap_or_default(),
            )
        })
        .collect();
    assert!(
        alive_enemies.is_empty(),
        "the drill-fed duo killed the wave (alive: {alive_enemies:?})"
    );
    assert!(
        sim.ecs.0.get_entity(core).is_ok() && sim.grid.tiles.get(core_x, core_y).build.is_some(),
        "the core survived the wave"
    );
    assert!(
        sim.ecs.0.get_entity(duo_entity).is_ok(),
        "the duo survived the wave"
    );
}
