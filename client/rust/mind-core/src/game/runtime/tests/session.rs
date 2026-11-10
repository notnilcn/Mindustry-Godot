// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Campaign session sync and launch-loadout tests.
//! Upstream: none (port-only tests).

use crate::ecs::TeamId;
use crate::game::runtime::test_support::*;
use crate::game::runtime::*;
use crate::world::TilePos;
use crate::world::block::BlockTable;
use crate::world::modules::ItemModule;

#[test]
fn sync_session_preserves_carried_team_plans() {
    use crate::ecs::BuildingComp;
    use crate::sim::unit_runtime::UnitRuntime;
    use crate::world::WorldGrid;
    use crate::world::limits::{BlockCounter, BuildRules};

    let registry = {
        let mut registry = content();
        registry.init().expect("init");
        registry
    };
    let table = BlockTable::build_default(&registry).expect("table");
    let mut world = World::new();
    let grid = WorldGrid::new(16, 16);
    let runtime_content = {
        let mut content = content();
        content.init().expect("init");
        content
    };
    world.insert_non_send(UnitRuntime::new(runtime_content, &grid, &world, 2, false));
    let core_inst = table.get_named("core-shard").expect("core-shard").clone();
    let core = core_inst.spawn(
        &mut world,
        7,
        TilePos::new(3, 3),
        1,
        0,
        registry.items().len(),
        registry.liquids().len(),
    );
    world.entity_mut(core).insert(BuildingComp {
        pos: TilePos::new(3, 3),
        block: registry.block_id("core-shard").expect("core id"),
        team: TeamId(1),
        rot: 0,
    });
    world.insert_resource(table);
    world.insert_resource(BuildRules::default());
    world.insert_resource(BlockCounter::new());

    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session
        .teams
        .get(TeamId(2))
        .plans
        .push_back(crate::game::teams::BlockPlan {
            x: 9,
            y: 9,
            rotation: 1,
            block: registry.block_id("copper-wall").expect("wall"),
            config: None,
            removed: false,
        });
    let sync = sync_session_with_sim(&mut session, &mut world, &registry, 0, false);
    assert_eq!(sync.player_cores, 1);
    let plans = &session.teams.get_or_null(TeamId(2)).expect("team 2").plans;
    assert_eq!(plans.len(), 1, "carried plans survive the teams rebuild");
    assert_eq!((plans[0].x, plans[0].y), (9, 9));
}

#[test]
fn sync_registers_cores_and_spawns() {
    use crate::ecs::BuildingComp as Comp;
    use crate::world::TilePos;

    let registry = content();
    let mut session = PlaySession::new(Rules {
        waves: true,
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    let mut world = World::new();
    let core = registry
        .block_id("core-shard")
        .expect("core-shard in base content");
    world.spawn(Comp {
        pos: TilePos::new(8, 8),
        block: core,
        team: TeamId(1),
        rot: 0,
    });
    world.spawn(Comp {
        pos: TilePos::new(40, 40),
        block: core,
        team: TeamId(2),
        rot: 0,
    });
    let sync = sync_session_with_sim(&mut session, &mut world, &registry, 1, true);
    assert_eq!(sync.player_cores, 1);
    assert_eq!(sync.wave_cores, 1);
    assert_eq!(sync.spawn_count, 1);
    assert_eq!(session.player_core_count(), 1);
    assert_eq!(session.wave_core_count(), 1);
}

#[test]
fn sync_applies_the_launch_loadout() {
    use crate::ecs::BuildingComp as Comp;
    use crate::io::json::content_serde::JsonItemStack;
    use crate::world::TilePos;

    let registry = content();
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        ..Rules::default()
    });
    session.add_starting_items = true;
    session.rules.loadout = vec![JsonItemStack {
        item: Some("copper".to_owned()),
        amount: 500,
    }];
    let mut world = World::new();
    let core = registry.block_id("core-shard").expect("core");
    let entity = world
        .spawn(Comp {
            pos: TilePos::new(8, 8),
            block: core,
            team: TeamId(1),
            rot: 0,
        })
        .id();
    let lead = registry.item_id("lead").expect("lead");
    {
        let mut module = ItemModule::with_items(registry.items().len());
        module.add(lead, 9, 100);
        world.entity_mut(entity).insert(module);
    }
    let _ = sync_session_with_sim(&mut session, &mut world, &registry, 0, true);
    let copper = registry.item_id("copper").expect("copper");
    let inventory = session
        .teams
        .inventory_ref(TeamId(1))
        .expect("team inventory");
    assert_eq!(inventory.get(copper), 500);
    // Upstream also clears/loads the core building's own storage.
    let module = world.get::<ItemModule>(entity).expect("core module");
    assert_eq!(module.get(copper), 500);
    assert_eq!(module.get(lead), 0, "fresh launch clears core storage");
}

/// `CampaignRuntime::install`: the runtime carries the shared service tracker
/// and the registry's unlocked-content cache.
#[test]
fn install_seeds_the_service_and_unlocked_cache() {
    let registry = content();
    let campaign = campaign(&registry);
    let unlocked = crate::game::tech_tree::unlocked_content_names(&registry);
    assert!(
        !unlocked.is_empty(),
        "vanilla content unlocks some tech nodes"
    );
    let runtime = CampaignRuntime::install(
        PlaySession::new(Rules::default()),
        campaign.clone(),
        crate::game::service::ServiceTracker::new(),
        &registry,
    );
    // `set_unlocked_content` collects into a set (names may repeat across
    // tech trees, e.g. `silicon` in both vanilla trees).
    let expected: std::collections::HashSet<String> = unlocked.into_iter().collect();
    assert_eq!(runtime.unlocked_content, expected);
    assert_eq!(runtime.campaign.planets.len(), campaign.planets.len());
}

/// The live turn context mirrors the session's sector/rules/wave
/// (`pump_runtime`'s two identical builds at `291e30a8`).
#[test]
fn turn_context_reads_the_live_session() {
    let registry = content();
    let campaign = campaign(&registry);
    let planet = campaign.planet_id_by_name("serpulo").unwrap();
    let mut session = PlaySession::new(Rules {
        waves: true,
        attack_mode: false,
        ..Rules::default()
    });
    session.sector = Some((planet, 3));
    session.wave = 9;
    let runtime = CampaignRuntime::new(session, campaign);
    let ctx = runtime.turn_context();
    assert_eq!(ctx.current_planet, Some(planet));
    assert_eq!(ctx.current_sector, Some((planet, 3)));
    assert_eq!(ctx.playing_rules, Some((true, false)));
    assert_eq!(ctx.current_wave, 9);
}

/// `run_due_turn` only runs when the clock flagged a turn and seeds the RNG
/// with `turn + 1`; `run_turn_now` is the manual path that ignores the flag.
#[test]
fn run_due_turn_respects_the_flag_and_advances_the_turn() {
    let registry = content();
    let campaign = campaign(&registry);
    let mut settings = crate::io::settings::SettingsStore::default();
    let mut runtime = CampaignRuntime::new(PlaySession::default(), campaign);
    let before = runtime.campaign.universe.turn;
    assert!(
        runtime.run_due_turn(&registry, &mut settings).is_none(),
        "no turn due"
    );
    assert_eq!(runtime.campaign.universe.turn, before);
    runtime.turn_due = true;
    let report = runtime
        .run_due_turn(&registry, &mut settings)
        .expect("the due turn runs");
    assert_eq!(report.turn, before + 1);
    assert!(!runtime.turn_due, "the flag clears");
    assert_eq!(runtime.campaign.universe.turn, before + 1);
    // The manual path runs regardless of `turn_due`.
    assert!(!runtime.turn_due);
    let report = runtime.run_turn_now(&registry, &mut settings);
    assert_eq!(report.turn, before + 2);
}

/// The resume record path writes the stored record into the session and
/// returns the items the facade puts into the live core.
#[test]
fn apply_resume_record_writes_items_and_rules() {
    let registry = content();
    let mut campaign = campaign(&registry);
    let planet = campaign.planet_id_by_name("serpulo").unwrap();
    let sector_id = registry.sector_by_name("groundZero").unwrap().sector;
    {
        let sector = campaign.sector_mut(planet, sector_id).expect("sector");
        sector.info.info.wave = 8;
        sector.info.info.waves = true;
        sector.info.info.wave_spacing = 33.0;
        sector.info.info.win_wave = 12;
        sector.info.info.attack = false;
        sector.info.info.has_core = true;
        sector.info.info.storage_capacity = 500;
        sector.info.info.items.insert("copper".to_owned(), 42);
    }
    let mut settings = crate::io::settings::SettingsStore::default();
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    session.sector = Some((planet, sector_id));
    let mut runtime = CampaignRuntime::new(session, campaign);
    let items = apply_resume_record(&mut runtime, &registry, &mut settings, planet, sector_id)
        .expect("the resume record applies");
    assert_eq!(items.get("copper"), Some(&42));
    assert_eq!(runtime.session.wave, 8);
    assert!(runtime.session.rules.waves);
    assert_eq!(runtime.session.rules.win_wave, 12);
    assert_eq!(runtime.session.rules.wave_spacing, 33.0);
    assert_eq!(runtime.session.rules.planet, "serpulo");
    let copper = registry.item_id("copper").expect("copper");
    let inventory = runtime
        .session
        .teams
        .inventory_ref(TeamId(1))
        .expect("the default team inventory refilled");
    assert_eq!(inventory.get(copper), 42);
    // An unknown planet has no record to apply.
    assert!(
        apply_resume_record(
            &mut runtime,
            &registry,
            &mut settings,
            crate::content::PlanetId::new(60_000),
            sector_id,
        )
        .is_none()
    );
}
