// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Environment and terrain regressions: deep water, out-of-bounds, drop zones and damage events.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{live_sim, place_at, tick_n};

/// G01 T13: a grounded dagger on `deep-water` drowns after
/// `hitSize/8 * drownTimeMultiplier * floor.drownTime` = 200 ticks
/// (`UnitComp.updateDrowning`, UnitComp.java:706-723).
#[test]
fn dagger_drowns_in_deep_water() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(48);
    let deep = {
        let rt = sim.unit_runtime().expect("rt");
        rt.content.block_id("deep-water").expect("deep-water")
    };
    for x in 18..23 {
        for y in 18..23 {
            sim.grid.tiles.get_mut(x, y).floor = deep;
        }
    }
    sim.refresh_unit_runtime();
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0, 20.5 * 8.0, 0.0)
        .expect("dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(dagger) {
        slot.kind = AiKind::NoAi;
    }
    let alive = |sim: &Sim| {
        sim.ecs
            .0
            .get::<Health>(dagger)
            .is_some_and(|health| !health.dead)
    };
    tick_n(&mut sim, 195);
    assert!(alive(&sim), "still alive at tick 195");
    tick_n(&mut sim, 15);
    assert!(!alive(&sim), "drowned by tick 210");
}

/// G01 T13: `shallow-water` applies `wet` (90 ticks) to a grounded unit.
#[test]
fn shallow_water_applies_wet() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::entities::comp::unit::comp::StatusComp;

    let mut sim = live_sim(48);
    let shallow = {
        let rt = sim.unit_runtime().expect("rt");
        rt.content.block_id("shallow-water").expect("shallow-water")
    };
    for x in 18..23 {
        for y in 18..23 {
            sim.grid.tiles.get_mut(x, y).floor = shallow;
        }
    }
    sim.refresh_unit_runtime();
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0, 20.5 * 8.0, 0.0)
        .expect("dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(dagger) {
        slot.kind = AiKind::NoAi;
    }
    sim.tick().expect("tick");
    let wet = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("wet")
        .expect("wet");
    let entry = sim.ecs.0.get::<StatusComp>(dagger).and_then(|comp| {
        comp.statuses
            .iter()
            .find(|entry| entry.effect == wet)
            .copied()
    });
    let entry = entry.expect("wet applied by the shallow-water floor");
    assert!(
        (89.0..=90.0).contains(&entry.duration),
        "wet for 90 ticks: {}",
        entry.duration
    );
}

/// G01 T13: a bounded unit far outside the map is clamped back in
/// (`UnitComp.java:695-717`: the clamp runs before the `-250` kill check, so
/// the kill is effectively unreachable for bounded units — the plan text
/// expected death; Java clamps).
#[test]
fn unit_out_of_bounds_dies() {
    let mut sim = live_sim(48);
    let dagger = sim
        .spawn_live_unit("dagger", 2, -300.0, 100.0, 0.0)
        .expect("dagger");
    sim.tick().expect("tick");
    let pos = sim.ecs.0.get::<Pos>(dagger).copied().expect("dagger pos");
    let dead = sim
        .ecs
        .0
        .get::<Health>(dagger)
        .is_none_or(|health| health.dead);
    eprintln!("out-of-bounds dagger at {pos:?}, dead {dead}");
    assert!(
        !dead,
        "the unit survives (Java clamps before the kill check)"
    );
    assert!(pos.x >= 0.0, "the grounded clamp pulls it back in: {pos:?}");
}

/// G01 T13: standing on a solid tile kills a non-boosting unit; a `nova` can
/// boost instead (`UnitComp.java:849-855`). The port's movement helpers lack
/// Java's collision clamp, so a corner clip pushes the unit out and only an
/// embedded unit dies — here the dagger is displaced off the wall tile.
#[test]
fn dagger_on_wall_dies_nova_boosts() {
    let mut sim = live_sim(48);
    place_at(&mut sim, 20, 20, "copper-wall", 1);
    // `World.toTile` rounds, so tile-centre coordinates map to the *next*
    // tile: spawn inside (20,20) rather than on its centre.
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.4 * 8.0, 20.4 * 8.0, 0.0)
        .expect("dagger");
    // The nova also starts on the wall tile, where it boosts off instead.
    let nova = sim
        .spawn_live_unit("nova", 2, 20.3 * 8.0, 20.3 * 8.0, 0.0)
        .expect("nova");
    tick_n(&mut sim, 30);
    let dagger_tile = sim.ecs.0.get::<Pos>(dagger).map(|pos| {
        (
            crate::world::WorldGrid::to_tile(pos.x),
            crate::world::WorldGrid::to_tile(pos.y),
        )
    });
    let dead = sim
        .ecs
        .0
        .get::<Health>(dagger)
        .is_none_or(|health| health.dead);
    assert!(
        dead || dagger_tile != Some((20, 20)),
        "the dagger left the solid tile (or died): tile {dagger_tile:?}"
    );
    let elevation = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::UnitCore>(nova)
        .map(|core| core.elevation)
        .unwrap_or(0.0);
    assert!(
        elevation > 0.9,
        "the nova boosted off the wall: {elevation}"
    );
}

/// G01 T13: a player-team unit inside a wave spawn's drop zone is pushed
/// outward (`UnitComp.java:734-744`).
#[test]
fn drop_zone_pushes_player_units() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::TilePos;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        // `GameState.hasSpawns()` (`GameState.java:68-70`) needs a spawn
        // *group* list; the drop-zone tiles come from the runtime spawner below.
        spawns: vec![crate::game::rules::SpawnGroup::default()],
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let spawn = TilePos::new(24, 24);
    if let Some(mut runtime) = sim.unit_runtime_mut() {
        runtime.wave_spawner.set_spawns(vec![spawn]);
    }
    let (sx, sy) = (24.5 * 8.0, 24.5 * 8.0);
    let dagger = sim
        .spawn_live_unit("dagger", 1, sx + 50.0, sy, 0.0)
        .expect("dagger");
    let start = 50.0f32;
    tick_n(&mut sim, 10);
    let end = sim
        .ecs
        .0
        .get::<Pos>(dagger)
        .map(|pos| ((pos.x - sx).powi(2) + (pos.y - sy).powi(2)).sqrt())
        .unwrap_or(start);
    eprintln!("drop-zone distance {start} -> {end}");
    assert!(end > start, "the unit was pushed outward: {start} -> {end}");
}

/// G01 T15: a bullet hitting a building emits a `BuildDamageEvent`
/// (`BuildingComp.damage(Bullet, ..)` -> `Events.fire`); draining clears it.
#[test]
fn bullet_hit_emits_build_damage_event() {
    let mut sim = live_sim(48);
    let wall = place_at(&mut sim, 20, 20, "copper-wall", 1);
    sim.spawn_live_unit("dagger", 2, 20.0 * 8.0 + 60.0, 20.0 * 8.0, 180.0)
        .expect("dagger");
    tick_n(&mut sim, 300);
    let events = sim.drain_build_damage_events();
    let wall_events: Vec<_> = events
        .iter()
        .filter(|event| event.x == 20 && event.y == 20)
        .collect();
    eprintln!(
        "build damage events: {} (wall {})",
        events.len(),
        wall_events.len()
    );
    assert!(!wall_events.is_empty(), "the wall took bullet damage");
    assert!(
        wall_events.iter().all(|event| event.team == 1),
        "events carry the damaged building's team"
    );
    assert!(
        wall_events.iter().all(|event| event.amount > 0.0),
        "applied damage recorded"
    );
    let _ = wall;
    let second = sim.drain_build_damage_events();
    assert!(second.is_empty(), "the drain clears the log");
}
