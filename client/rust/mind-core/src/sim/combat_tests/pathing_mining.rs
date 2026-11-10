// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Pathing, mining, repair/rebuild and unit-stance regressions.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{health, live_sim, place_at, tick_n};

/// G01 T6a: a wave ground unit walks the enemy-core flowfield
/// (`Pathfinder.fieldCore`, GroundAI.java:46-60) instead of detouring to the
/// nearest player building, and stops at `type.range * 0.5`.
#[test]
fn ground_wave_paths_to_core_not_nearest_building() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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

    place_at(&mut sim, 10, 48, "core-shard", 1);
    // Decoy: much closer to the dagger than the core, but on another axis.
    let wall = place_at(&mut sim, 80, 10, "copper-wall", 1);
    let dagger = sim
        .spawn_live_unit("dagger", 2, 85.0 * 8.0, 48.0 * 8.0, 180.0)
        .expect("dagger");
    let start_x = sim.ecs.0.get::<Pos>(dagger).expect("pos").x;
    tick_n(&mut sim, 3000);
    let x = sim
        .ecs
        .0
        .get::<Pos>(dagger)
        .map(|pos| pos.x)
        .unwrap_or(f32::MAX);
    eprintln!(
        "dagger x {x} (start {start_x}), wall health {:?}",
        health(&sim, wall)
    );
    assert!(
        start_x - x > 300.0,
        "the dagger walked toward the enemy core (start x {start_x}, now {x})"
    );
    let wall_max = sim
        .ecs
        .0
        .get::<Health>(wall)
        .expect("wall health")
        .max_health;
    assert_eq!(
        health(&sim, wall),
        Some(wall_max),
        "the decoy wall was never attacked"
    );
}

/// G01 T6a: every unit of a team shares one `(team, cost)` flowfield per tick
/// (plan §6.9): two daggers must not rebuild it per unit.
#[test]
fn field_is_stable_across_units() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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
    place_at(&mut sim, 10, 48, "core-shard", 1);
    sim.spawn_live_unit("dagger", 2, 85.0 * 8.0, 48.0 * 8.0, 180.0)
        .expect("dagger a");
    sim.spawn_live_unit("dagger", 2, 85.0 * 8.0, 44.0 * 8.0, 180.0)
        .expect("dagger b");

    let before = sim.unit_runtime().expect("rt").pathfinder.updates;
    tick_n(&mut sim, 10);
    let after = sim.unit_runtime().expect("rt").pathfinder.updates;
    assert!(
        after - before <= 2,
        "one flowfield build per (team, cost) per tick: {before} -> {after}"
    );
}

/// G01 T6b: a wave `horizon` (targetFlags `{factory, null}`) bombs the closest
/// active factory even though an enemy core is on the map
/// (`FlyingAI.findMainTarget`, FlyingAI.java:40-72).
#[test]
fn horizon_prefers_factory() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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

    // Core 147 px from the horizon (out of its 140 px range) but still nearer
    // than the factory at 170 px: the `{factory, null}` flag must win over the
    // old nearest-building anchor.
    place_at(&mut sim, 47, 47, "core-shard", 1);
    let core = sim.grid.tiles.get(47, 47).build.expect("core entity");
    let press = place_at(&mut sim, 75, 75, "graphite-press", 1);
    // `targetFlagActive` needs `items.any()` in the port's model.
    let coal = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("coal")
        .expect("coal");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(press) {
        items.add(coal, 1, 10);
    }
    let _horizon = sim
        .spawn_live_unit("horizon", 2, 60.0 * 8.0, 60.0 * 8.0, 180.0)
        .expect("horizon");
    let core_max = sim
        .ecs
        .0
        .get::<Health>(core)
        .expect("core health")
        .max_health;
    let mut destroyed_at = None;
    for tick in 0..900 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(press).is_err() {
            destroyed_at = Some(tick);
            break;
        }
    }
    let core_now = health(&sim, core);
    eprintln!("factory destroyed at {destroyed_at:?}, core {core_now:?}/{core_max}");
    assert!(
        destroyed_at.is_some(),
        "the factory was bombed (core {core_now:?}/{core_max})"
    );
    assert_eq!(
        core_now,
        Some(core_max),
        "the horizon never targeted the nearer core while the factory died"
    );
}

/// G01 T6b: a `flare` (`targetFlags {generator, null}`) with no generators
/// falls back to the enemy core (`FlyingAI.findMainTarget`'s final `return core`).
#[test]
fn flare_falls_back_to_core() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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

    let core = place_at(&mut sim, 10, 10, "core-shard", 1);
    let flare = sim
        .spawn_live_unit("flare", 2, 60.0 * 8.0, 60.0 * 8.0, 180.0)
        .expect("flare");
    // The flare's `circleAttack` orbit is chaotic over long windows; record the
    // closest approach to the core instead of the final position.
    let mut min_dist = f32::MAX;
    for _ in 0..900 {
        sim.tick().expect("tick");
        if let (Some(a), Some(b)) = (
            sim.ecs.0.get::<Pos>(flare).copied(),
            sim.ecs.0.get::<Pos>(core).copied(),
        ) {
            min_dist = min_dist.min(((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt());
        }
    }
    let a = sim.ecs.0.get::<Pos>(flare).copied().expect("flare pos");
    let b = sim.ecs.0.get::<Pos>(core).copied().expect("core pos");
    let end = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
    eprintln!("flare-core final {end}, closest {min_dist}");
    assert!(
        min_dist < 100.0,
        "the flare flew to the core (closest approach {min_dist})"
    );
}

/// G01 T10a: a team-1 `mono` (default command `mine`) picks copper ore next to
/// its core, mines it (`MinerAI.updateMovement` + `MinerComp.update`) and
/// delivers a full stack to the core.
#[test]
fn mono_mines_and_delivers() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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

    let core = place_at(&mut sim, 10, 10, "core-shard", 1);
    let ore = {
        let rt = sim.unit_runtime().expect("rt");
        rt.content.block_id("ore-copper").expect("ore-copper")
    };
    for i in 0..3 {
        sim.grid.tiles.get_mut(20 + i, 10).overlay = ore;
    }
    sim.refresh_unit_runtime();
    let mono = sim
        .spawn_live_unit("mono", 1, 10.5 * 8.0, 10.5 * 8.0, 0.0)
        .expect("mono");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(mono) {
        slot.kind = AiKind::Command;
    }
    tick_n(&mut sim, 2000);
    let copper = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("copper")
        .expect("copper");
    let delivered = sim
        .ecs
        .0
        .get::<ItemModule>(core)
        .map_or(0, |items| items.get(copper));
    let mined = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::MinerComp>(mono)
        .map_or(0, |comp| comp.mined);
    eprintln!("mono mined {mined}, core copper {delivered}");
    assert!(delivered >= 20, "the mono delivered copper: {delivered}");
}

/// G01 T10a: titanium ore (hardness 3) is above the mono's `mineTier` 1, so the
/// target-item selection skips it and nothing is mined.
#[test]
fn miner_comp_respects_tier() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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
    let core = place_at(&mut sim, 10, 10, "core-shard", 1);
    let ore = {
        let rt = sim.unit_runtime().expect("rt");
        rt.content.block_id("ore-titanium").expect("ore-titanium")
    };
    for i in 0..3 {
        sim.grid.tiles.get_mut(20 + i, 10).overlay = ore;
    }
    sim.refresh_unit_runtime();
    let mono = sim
        .spawn_live_unit("mono", 1, 10.5 * 8.0, 10.5 * 8.0, 0.0)
        .expect("mono");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(mono) {
        slot.kind = AiKind::Command;
    }
    tick_n(&mut sim, 600);
    let titanium = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("titanium")
        .expect("titanium");
    let mined = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::MinerComp>(mono)
        .map_or(0, |comp| comp.mined);
    let delivered = sim
        .ecs
        .0
        .get::<ItemModule>(core)
        .map_or(0, |items| items.get(titanium));
    assert_eq!(mined, 0, "titanium is above the mono's tier");
    assert_eq!(delivered, 0, "nothing reaches the core");
}

/// G01 T10b: a team-1 `mega` (default command `repair`) flies to a damaged
/// friendly wall, aims its heal bolts at it and restores its health
/// (`RepairAI.updateTargeting/updateMovement`, RepairAI.java:19-80 + T4's
/// `hitTile` heals).
#[test]
fn mega_repairs_damaged_wall() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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

    place_at(&mut sim, 10, 10, "core-shard", 1);
    let wall = place_at(&mut sim, 30, 10, "copper-wall", 1);
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.health = 100.0;
    }
    let mega = sim
        .spawn_live_unit("mega", 1, 10.0 * 8.0, 14.0 * 8.0, 0.0)
        .expect("mega");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(mega) {
        slot.kind = AiKind::Command;
    }
    tick_n(&mut sim, 900);
    let healed = health(&sim, wall).unwrap_or(0.0);
    let (mp, wp) = (
        sim.ecs.0.get::<Pos>(mega).copied().expect("mega pos"),
        sim.ecs.0.get::<Pos>(wall).copied().expect("wall pos"),
    );
    let dist = ((mp.x - wp.x).powi(2) + (mp.y - wp.y).powi(2)).sqrt();
    eprintln!("wall {healed} hp, mega {dist} px away");
    assert!(healed > 100.0, "the mega healed the wall: {healed}");
    assert!(dist < 178.0, "the mega stays within range: {dist}");
}

/// G01 T10c: a team-1 `poly` (default command `rebuild`) pulls the destroyed
/// wall's ghost plan from the team queue and rebuilds it, paying the block
/// cost from the core (`BuilderAI` + `Sim::update_builders`).
#[test]
fn poly_rebuilds_destroyed_wall() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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

    let core = place_at(&mut sim, 10, 10, "core-shard", 1);
    let copper = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("copper")
        .expect("copper");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(core) {
        items.add(copper, 500, 4000);
    }
    let before = sim
        .ecs
        .0
        .get::<ItemModule>(core)
        .map_or(0, |items| items.get(copper));
    let wall = place_at(&mut sim, 20, 10, "copper-wall", 1);
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.health = 0.0;
        health.dead = true;
    }
    sim.tick().expect("tick");
    let poly = sim
        .spawn_live_unit("poly", 1, 10.5 * 8.0, 10.5 * 8.0, 0.0)
        .expect("poly");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(poly) {
        slot.kind = AiKind::Command;
    }
    tick_n(&mut sim, 1200);
    let rebuilt = sim.grid.tiles.get(20, 10).build;
    let after = sim
        .ecs
        .0
        .get::<ItemModule>(core)
        .map_or(0, |items| items.get(copper));
    eprintln!("rebuilt {rebuilt:?}, core copper {before} -> {after}");
    let entity = rebuilt.expect("the wall tile holds a building again");
    assert_eq!(
        sim.ecs
            .0
            .get::<crate::ecs::BuildingComp>(entity)
            .map(|c| c.block.get()),
        Some(216),
        "the rebuilt block is a copper-wall"
    );
    assert!(
        after <= before - 6,
        "the core paid 6 copper ({before} -> {after})"
    );
}

/// G01 T10c: a wave-team core builder falls back to the ground AI
/// (`BuilderAI.fallback`, BuilderAI.java:264-278): the alpha walks the
/// enemy-core field instead of running the builder logic.
#[test]
fn wave_alpha_falls_back_to_ground_ai() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(96);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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

    place_at(&mut sim, 10, 48, "core-shard", 1);
    let alpha = sim
        .spawn_live_unit("alpha", 2, 80.0 * 8.0, 48.0 * 8.0, 180.0)
        .expect("alpha");
    let start_x = sim.ecs.0.get::<Pos>(alpha).expect("pos").x;
    tick_n(&mut sim, 900);
    let x = sim
        .ecs
        .0
        .get::<Pos>(alpha)
        .map(|pos| pos.x)
        .unwrap_or(f32::MAX);
    eprintln!("alpha x {start_x} -> {x}");
    assert!(
        start_x - x > 200.0,
        "the wave alpha walked toward the enemy core ({start_x} -> {x})"
    );
}

/// G01 T11: the `holdfire` stance stops a commanded dagger from shooting
/// (`CommandAI.shouldFire`, CommandAI.java:527).
#[test]
fn hold_fire_stops_shooting() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let shooter = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("shooter");
    let target = sim
        .spawn_live_unit("dagger", 2, 200.0, 100.0, 180.0)
        .expect("target");
    // Dummy target: the `Player` controller receives no input, so it never
    // shoots back while the shooter's fire counts are measured.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(target) {
        slot.kind = AiKind::Player;
    }
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(shooter) {
        slot.kind = AiKind::Command;
    }
    let shots = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::weapons::UnitWeapons>(shooter)
            .map(|weapons| weapons.mounts.iter().map(|m| m.total_shots).sum::<i32>())
            .unwrap_or(0)
    };
    tick_n(&mut sim, 300);
    let before = shots(&sim);
    assert!(before > 0, "the dagger fired before the stance: {before}");
    let id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(shooter)
        .expect("seq")
        .0 as i32;
    let hold_fire = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_stance_by_name("holdfire")
        .expect("holdfire")
        .id
        .raw();
    sim.command(SimCommand::UnitStance {
        units: smallvec::smallvec![id],
        stance: hold_fire,
        enabled: true,
    })
    .expect("stance");
    tick_n(&mut sim, 300);
    let after = shots(&sim);
    assert_eq!(after, before, "holdFire stops firing ({before} -> {after})");
}

/// G01 T11: the `boost` stance lifts a `nova` (`CommandAI.shouldBoost` +
/// `UnitComp.updateBoosting`).
#[test]
fn boost_stance_lifts_nova() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        waves: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let nova = sim
        .spawn_live_unit("nova", 1, 100.0, 100.0, 0.0)
        .expect("nova");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(nova) {
        slot.kind = AiKind::Command;
    }
    let id = sim.ecs.0.get::<crate::ecs::EntitySeq>(nova).expect("seq").0 as i32;
    let boost = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_stance_by_name("boost")
        .expect("boost")
        .id
        .raw();
    sim.command(SimCommand::UnitStance {
        units: smallvec::smallvec![id],
        stance: boost,
        enabled: true,
    })
    .expect("stance");
    tick_n(&mut sim, 60);
    let elevation = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::UnitCore>(nova)
        .map(|core| core.elevation)
        .unwrap_or(0.0);
    assert!(
        elevation > 0.5,
        "the boost stance lifts the nova: {elevation}"
    );
}

/// G01 T11: an item stance restricts the mono's target item
/// (`MinerAI`'s `ItemUnitStance` branch): only copper is mined although lead
/// ore is available.
#[test]
fn mono_item_stance_filters_item() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
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
    let core = place_at(&mut sim, 10, 10, "core-shard", 1);
    let (copper_ore, lead_ore) = {
        let rt = sim.unit_runtime().expect("rt");
        (
            rt.content.block_id("ore-copper").expect("ore-copper"),
            rt.content.block_id("ore-lead").expect("ore-lead"),
        )
    };
    for i in 0..3 {
        sim.grid.tiles.get_mut(20 + i, 10).overlay = copper_ore;
        sim.grid.tiles.get_mut(20 + i, 14).overlay = lead_ore;
    }
    sim.refresh_unit_runtime();
    let mono = sim
        .spawn_live_unit("mono", 1, 10.5 * 8.0, 10.5 * 8.0, 0.0)
        .expect("mono");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(mono) {
        slot.kind = AiKind::Command;
    }
    let id = sim.ecs.0.get::<crate::ecs::EntitySeq>(mono).expect("seq").0 as i32;
    let copper_stance = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_stance_by_name("item-copper")
        .expect("item-copper")
        .id
        .raw();
    sim.command(SimCommand::UnitStance {
        units: smallvec::smallvec![id],
        stance: copper_stance,
        enabled: true,
    })
    .expect("stance");
    tick_n(&mut sim, 1500);
    let (copper, lead) = {
        let rt = sim.unit_runtime().expect("rt");
        let items = sim.ecs.0.get::<ItemModule>(core);
        (
            items.map_or(0, |i| i.get(rt.content.item_id("copper").expect("copper"))),
            items.map_or(0, |i| i.get(rt.content.item_id("lead").expect("lead"))),
        )
    };
    eprintln!("copper {copper}, lead {lead}");
    assert!(copper > 0, "the mono mined copper: {copper}");
    assert_eq!(lead, 0, "the item stance kept the miner off lead");
}
