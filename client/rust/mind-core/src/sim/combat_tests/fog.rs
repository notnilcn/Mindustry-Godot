// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Fog-of-war regressions: unit reveal, core fog radii, team vision and static-fog placement.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;

/// H09 T1: `Logic.java:533-535` fog update — a player-team unit reveals its
/// radius, discovers tiles permanently and does not distinguish hidden tiles.
#[test]
fn fog_unit_reveals_and_discovers() {
    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        // Team 1 has no core in this fixture; without the cap exception the
        // spawned dagger is unit-cap-killed at spawn (`spawn_unit_def`).
        rules.disable_unit_cap = true;
    });
    sim.reset_fog();
    let dagger = sim
        .spawn_live_unit("dagger", 1, 20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0, 0.0)
        .expect("dagger");
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    // Dagger fog radius 174/8 = 21.75 -> event radius 21, drawn +1.
    assert!(
        sim.fog_visible_tile(1, 30, 20),
        "tile 10 tiles away is visible"
    );
    assert!(
        !sim.fog_visible_tile(1, 50, 20),
        "tile 30 tiles away is not visible"
    );
    assert!(sim.fog_discovered(1, 30, 20), "seen tile is discovered");
    assert!(
        !sim.fog_discovered(1, 50, 20),
        "unseen tile is not discovered"
    );
    // Only-AI teams ignore fog (`FogControl.isVisibleTile`).
    assert!(
        sim.fog_visible_tile(2, 60, 60),
        "the wave team is AI and sees everything"
    );

    // Moving away keeps the explored tile discovered but not visible.
    let mut pos = sim.ecs.0.get_mut::<Pos>(dagger).expect("dagger pos");
    pos.x = 50.0 * 8.0 + 4.0;
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    assert!(
        !sim.fog_visible_tile(1, 20, 20),
        "the left-behind tile is no longer visible"
    );
    assert!(
        sim.fog_discovered(1, 20, 20),
        "the left-behind tile stays discovered"
    );
}

/// H09 T1: with `rules.fog` off there is no `FogControl` and every probe is
/// true (Serpulo pays nothing).
#[test]
fn fog_off_everything_visible() {
    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
    });
    sim.reset_fog();
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::game::fog::FogControl>()
            .is_none(),
        "no fog resource without rules.fog"
    );
    assert!(sim.fog_visible_tile(1, 60, 60));
    assert!(sim.fog_discovered(1, 60, 60));
    assert!(sim.fog_visible_tile(2, 60, 60));
    assert!(sim.fog_discovered(2, 60, 60));
}

/// H09 T1: the sector-start path applies the world rules before the campaign
/// runtime exists, so no `FogControl` resource was created; the first tick must
/// create and size it (`FogControl.update`: `fog == null` -> new array).
#[test]
fn fog_update_creates_the_resource_when_missing() {
    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        rules.disable_unit_cap = true;
    });
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::game::fog::FogControl>()
            .is_none()
    );
    sim.spawn_live_unit("dagger", 1, 20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0, 0.0)
        .expect("dagger");
    sim.tick().expect("tick");
    assert!(
        sim.ecs
            .0
            .get_resource::<crate::game::fog::FogControl>()
            .is_some(),
        "the first tick creates the fog resource"
    );
    assert!(sim.fog_visible_tile(1, 25, 20));
    assert!(sim.fog_discovered(1, 25, 20));
}

/// H09 T2: a player turret cannot target a unit hidden in fog; a friendly unit
/// that reveals it restores the target (`Units.closestEnemy` `inFogTo`).
#[test]
fn turret_ignores_unit_in_fog() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.disable_unit_cap = true;
    });
    // Placed before the fog reset: after it, the static-fog mask
    // (all-zero until something is discovered) would reject the placement,
    // mirroring Java's `fogControl.isDiscovered == false` for every tile.
    let duo = place(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    sim.reset_fog();
    // T3 makes every turret a fog source covering its range; zero the
    // multiplier so the duo reveals nothing (the pre-T3 viewer state this gate
    // test needs).
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        let mut config = (*state.config).clone();
        config.fog_radius_multiplier = 0.0;
        state.config = std::sync::Arc::new(config);
    }
    sim.spawn_live_unit("dagger", 2, 25.0 * 8.0 + 4.0, 10.0 * 8.0 + 4.0, 180.0)
        .expect("enemy dagger");
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .and_then(|state| state.target);
    assert!(
        target.is_none(),
        "the duo does not target a unit hidden in fog"
    );

    // A team-1 dagger next to the enemy reveals it (radius 21).
    sim.spawn_live_unit("dagger", 1, 24.0 * 8.0 + 4.0, 10.0 * 8.0 + 4.0, 0.0)
        .expect("friendly dagger");
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .and_then(|state| state.target);
    assert!(target.is_some(), "the revealed dagger becomes the target");
}

/// H09 T2: AI teams ignore fog (`FogControl.isVisibleTile`: `team.isAI()`).
#[test]
fn ai_team_sees_through_fog() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        rules.disable_unit_cap = true;
    });
    sim.reset_fog();
    let duo = place(&mut sim, 20, 20, "duo", 2);
    feed(&mut sim, duo, "copper", 10);
    sim.spawn_live_unit("dagger", 1, 35.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0, 180.0)
        .expect("player dagger");
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .and_then(|state| state.target);
    assert!(target.is_some(), "the wave team (AI) sees through fog");
}

/// H09 T2: `BlockIndexer.find_enemy_tile` skips enemy buildings on
/// undiscovered tiles (`BlockIndexer.java:468`); a revealing unit restores it.
#[test]
fn find_enemy_tile_skips_undiscovered() {
    use crate::ai::block_indexer::{BlockIndexer, BlockPriority};
    use crate::sim::unit_runtime::UnitRuntime;

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        rules.disable_unit_cap = true;
    });
    sim.reset_fog();
    place(&mut sim, 40, 40, "core-shard", 2);
    for _ in 0..5 {
        sim.tick().expect("tick");
    }

    let runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("rt");
    let mut indexer = BlockIndexer::new();
    indexer.rebuild(&mut sim.ecs.0, &runtime.content);
    let before = indexer
        .find_enemy_tile(
            &sim.ecs.0,
            &runtime.content,
            1,
            10.0 * 8.0,
            10.0 * 8.0,
            4000.0,
            &[BlockPriority::Core],
        )
        .map(|tile| (tile.x(), tile.y()));
    sim.ecs.0.insert_non_send(runtime);
    assert_eq!(before, None, "undiscovered enemy core is skipped");

    sim.spawn_live_unit("dagger", 1, 40.0 * 8.0 + 4.0, 40.0 * 8.0 + 4.0, 0.0)
        .expect("dagger");
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("rt");
    let mut indexer = BlockIndexer::new();
    indexer.rebuild(&mut sim.ecs.0, &runtime.content);
    let after = indexer
        .find_enemy_tile(
            &sim.ecs.0,
            &runtime.content,
            1,
            10.0 * 8.0,
            10.0 * 8.0,
            4000.0,
            &[BlockPriority::Core],
        )
        .map(|tile| (tile.x(), tile.y()));
    sim.ecs.0.insert_non_send(runtime);
    assert_eq!(after, Some((40, 40)), "the discovered core is found");
}

/// H09 T2: `Build.validPlace` rejects tiles the player team has not discovered
/// (`Build.java:252`); a revealing unit unlocks the tile.
#[test]
fn placement_rejected_in_static_fog() {
    use crate::sim::unit_runtime::UnitRuntime;
    use crate::world::build::valid_place;
    use crate::world::limits::{BlockCounter, BuildRules};

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.static_fog = true;
        rules.disable_unit_cap = true;
    });
    sim.reset_fog();
    sim.tick().expect("tick");

    let runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("rt");
    let table = sim
        .ecs
        .0
        .get_resource::<crate::world::BlockTable>()
        .expect("block table");
    let rules = sim.ecs.0.get_resource::<BuildRules>().expect("build rules");
    let counter = sim.ecs.0.get_resource::<BlockCounter>().expect("counter");
    let wall = runtime.content.block_id("copper-wall").expect("wall");
    let before = valid_place(
        &runtime.content,
        table,
        rules,
        counter,
        &sim.grid,
        wall,
        1,
        0,
        30,
        30,
    );
    sim.ecs.0.insert_non_send(runtime);
    assert!(!before, "an undiscovered tile rejects the placement");

    sim.spawn_live_unit("dagger", 1, 30.0 * 8.0 + 4.0, 30.0 * 8.0 + 4.0, 0.0)
        .expect("dagger");
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let runtime = sim.ecs.0.remove_non_send::<UnitRuntime>().expect("rt");
    let table = sim
        .ecs
        .0
        .get_resource::<crate::world::BlockTable>()
        .expect("block table");
    let rules = sim.ecs.0.get_resource::<BuildRules>().expect("build rules");
    let counter = sim.ecs.0.get_resource::<BlockCounter>().expect("counter");
    let after = valid_place(
        &runtime.content,
        table,
        rules,
        counter,
        &sim.grid,
        wall,
        1,
        0,
        30,
        30,
    );
    sim.ecs.0.insert_non_send(runtime);
    assert!(after, "the revealed tile accepts the placement");
}

/// H09 T3: a powered `radar` discovers its radius in `discoveryTime` = 600
/// ticks (`Radar.java:18,59-72`).
#[test]
fn radar_discovers_in_600_ticks() {
    use crate::entities::comp::building::RadarState;

    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.disable_unit_cap = true;
    });
    // Placed before the fog reset (the all-zero static mask else rejects).
    let radar = place(&mut sim, 20, 20, "radar", 1);
    place(&mut sim, 21, 20, "power-source", 1);
    sim.reset_fog();
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let progress = sim
        .ecs
        .0
        .get::<RadarState>(radar)
        .map(|state| state.progress)
        .unwrap_or(0.0);
    assert!(
        (progress - 0.5).abs() < 0.02,
        "progress ~0.5 at 300 ticks: {progress}"
    );
    // One tick is lost while the power graph links (efficiency 0 on the first
    // tick), so full discovery completes on tick 601.
    for _ in 0..301 {
        sim.tick().expect("tick");
    }
    let progress = sim
        .ecs
        .0
        .get::<RadarState>(radar)
        .map(|state| state.progress)
        .unwrap_or(0.0);
    assert!(
        (progress - 1.0).abs() < 1e-3,
        "progress 1.0 at 600 ticks: {progress}"
    );
}

/// H09 T3: every turret reveals its range (`BaseTurret.java:59`,
/// `Turret.java:426-428`): a `duo` (range 160) reveals 20 tiles.
#[test]
fn duo_reveals_its_range() {
    let mut sim = campaign_sim(64, |rules| {
        rules.waves = true;
        rules.fog = true;
        rules.disable_unit_cap = true;
    });
    // Placed before the fog reset (the all-zero static mask else rejects).
    place(&mut sim, 10, 10, "duo", 1);
    sim.reset_fog();
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    // `Mathf.round(160 / 8) = 20` tiles, drawn with the +1 visual margin.
    assert!(sim.fog_visible_tile(1, 30, 10), "a tile at +20 is visible");
    assert!(
        !sim.fog_visible_tile(1, 32, 10),
        "a tile at +22 is not visible"
    );
    assert!(
        sim.fog_discovered(1, 20, 10),
        "revealed tiles are discovered"
    );
}

/// H09 T3: core fog radius = `max(fogRadius, (int)((30 + 20*size) / 8 * 3) + 13)`
/// (`CoreBlock.java:158-159`).
#[test]
fn core_fog_radius_values() {
    let sim = live_sim(8);
    let content = &sim.unit_runtime().expect("rt").content;
    let radius = |name: &str| {
        content
            .block_id(name)
            .and_then(|id| content.block(id))
            .map(|def| def.fog_radius)
    };
    assert_eq!(radius("core-shard"), Some(46), "size 3");
    assert_eq!(radius("core-bastion"), Some(54), "size 4");
    assert_eq!(radius("core-acropolis"), Some(69), "size 6");
}
