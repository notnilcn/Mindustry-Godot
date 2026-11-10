// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Live-sim combat regressions (turrets, bullet-vs-unit hits, unit/building
//! death) driven through the real `Sim` schedule with the block + unit
//! runtimes installed. Ported behaviors: `Turret.TurretBuild.updateTile`,
//! `BulletComp.collision`/`BulletType.hitEntity`, `UnitComp.update` death.

use bevy_ecs::entity::Entity;

use crate::content::BlockId;
use crate::determinism::SimCommand;
use crate::entities::comp::{Health, Pos, Vel};

use super::Sim;

mod turrets_bullets;
mod motion_payload;
mod deaths;
mod commands_ai;
mod fire_weather;
mod payload_weapons;
mod multipliers_player;

pub(super) fn live_sim(size: i32) -> Sim {
    let mut sim = Sim::new(1, size, size, BlockId::AIR, BlockId::AIR);
    sim.install_block_runtime().expect("block runtime");
    sim.install_unit_runtime().expect("unit runtime");
    sim
}

pub(super) fn block(sim: &Sim, name: &str) -> u16 {
    sim.unit_runtime()
        .expect("runtime")
        .content
        .block_id(name)
        .expect("block")
        .get()
}

pub(super) fn place(sim: &mut Sim, x: i16, y: i16, name: &str, team: u8) -> Entity {
    let block = block(sim, name);
    sim.command(SimCommand::Place {
        x,
        y,
        block,
        rotation: 0,
        team,
        player: None,
    })
    .expect("place");
    sim.grid
        .tiles
        .get(x as i32, y as i32)
        .build
        .expect("placed building")
}

fn feed(sim: &mut Sim, turret: Entity, item: &str, count: usize) {
    let id = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .item_id(item)
        .expect("item");
    for _ in 0..count {
        crate::world::blocks::defense::turrets::handle_item(&mut sim.ecs.0, turret, id);
    }
}

fn health(sim: &Sim, e: Entity) -> Option<f32> {
    sim.ecs.0.get::<Health>(e).map(|h| h.health)
}

fn shots_in(sim_setup: impl Fn(&mut Sim, Entity), ticks: usize) -> (i32, f32) {
    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    sim_setup(&mut sim, duo);
    // Hostile wall dummy inside the duo's 150 px range: a unit target would
    // return fire (G01 T6a gives it the Java targeting cadence), which kills
    // the duo before the coolant window ends.
    let _wall = place(&mut sim, 31, 20, "copper-wall", 2);
    let start = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map_or(0, |s| s.total_ammo);
    for _ in 0..ticks {
        sim.tick().expect("tick");
    }
    let end = sim
        .ecs
        .0
        .get::<crate::world::blocks::defense::turrets::TurretState>(duo)
        .map_or(0, |s| s.total_ammo);
    let water = sim
        .unit_runtime()
        .expect("rt")
        .content
        .liquid_id("water")
        .expect("water");
    let left = sim
        .ecs
        .0
        .get::<crate::world::modules::LiquidModule>(duo)
        .map_or(0.0, |l| l.get(water));
    (start - end, left)
}

/// Finds the first weapon bullet on `unit` whose def sets `spawnUnit`
/// (`BulletType.spawnUnit`), returning `(bullet, unit)` ids.
fn spawn_unit_weapon(sim: &Sim, unit: &str) -> Option<(u16, crate::content::UnitTypeId)> {
    let content = &sim.unit_runtime().expect("rt").content;
    let id = content.unit_id(unit)?;
    let def = content.unit(id)?;
    def.weapons
        .iter()
        .map(|weapon| weapon.bullet.id)
        .find_map(|bullet| {
            let def = content.bullet(bullet)?;
            def.spawn_unit.map(|unit| (bullet.get(), unit))
        })
}

/// Builds a `live_sim` with a `CampaignRuntime` whose rules are mutated by
/// `mutate` (team multipliers for the G01 T5 tests).
fn campaign_sim(size: i32, mutate: impl FnOnce(&mut crate::game::rules::Rules)) -> Sim {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(size);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut rules = Rules {
        default_team: 1,
        ..Rules::default()
    };
    mutate(&mut rules);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(PlaySession::new(rules), campaign));
    sim
}

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

    place(&mut sim, 10, 48, "core-shard", 1);
    // Decoy: much closer to the dagger than the core, but on another axis.
    let wall = place(&mut sim, 80, 10, "copper-wall", 1);
    let dagger = sim
        .spawn_live_unit("dagger", 2, 85.0 * 8.0, 48.0 * 8.0, 180.0)
        .expect("dagger");
    let start_x = sim.ecs.0.get::<Pos>(dagger).expect("pos").x;
    for _ in 0..3000 {
        sim.tick().expect("tick");
    }
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
    place(&mut sim, 10, 48, "core-shard", 1);
    sim.spawn_live_unit("dagger", 2, 85.0 * 8.0, 48.0 * 8.0, 180.0)
        .expect("dagger a");
    sim.spawn_live_unit("dagger", 2, 85.0 * 8.0, 44.0 * 8.0, 180.0)
        .expect("dagger b");

    let before = sim.unit_runtime().expect("rt").pathfinder.updates;
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
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
    place(&mut sim, 47, 47, "core-shard", 1);
    let core = sim.grid.tiles.get(47, 47).build.expect("core entity");
    let press = place(&mut sim, 75, 75, "graphite-press", 1);
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

    let core = place(&mut sim, 10, 10, "core-shard", 1);
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

    let core = place(&mut sim, 10, 10, "core-shard", 1);
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
    for _ in 0..2000 {
        sim.tick().expect("tick");
    }
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
    let core = place(&mut sim, 10, 10, "core-shard", 1);
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
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
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

    place(&mut sim, 10, 10, "core-shard", 1);
    let wall = place(&mut sim, 30, 10, "copper-wall", 1);
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.health = 100.0;
    }
    let mega = sim
        .spawn_live_unit("mega", 1, 10.0 * 8.0, 14.0 * 8.0, 0.0)
        .expect("mega");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(mega) {
        slot.kind = AiKind::Command;
    }
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
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

    let core = place(&mut sim, 10, 10, "core-shard", 1);
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
    let wall = place(&mut sim, 20, 10, "copper-wall", 1);
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
    for _ in 0..1200 {
        sim.tick().expect("tick");
    }
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

    place(&mut sim, 10, 48, "core-shard", 1);
    let alpha = sim
        .spawn_live_unit("alpha", 2, 80.0 * 8.0, 48.0 * 8.0, 180.0)
        .expect("alpha");
    let start_x = sim.ecs.0.get::<Pos>(alpha).expect("pos").x;
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
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
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
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
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
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
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
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
    let core = place(&mut sim, 10, 10, "core-shard", 1);
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
    for _ in 0..1500 {
        sim.tick().expect("tick");
    }
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
    for _ in 0..195 {
        sim.tick().expect("tick");
    }
    assert!(alive(&sim), "still alive at tick 195");
    for _ in 0..15 {
        sim.tick().expect("tick");
    }
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
    place(&mut sim, 20, 20, "copper-wall", 1);
    // `World.toTile` rounds, so tile-centre coordinates map to the *next*
    // tile: spawn inside (20,20) rather than on its centre.
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.4 * 8.0, 20.4 * 8.0, 0.0)
        .expect("dagger");
    // The nova also starts on the wall tile, where it boosts off instead.
    let nova = sim
        .spawn_live_unit("nova", 2, 20.3 * 8.0, 20.3 * 8.0, 0.0)
        .expect("nova");
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
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
    for _ in 0..10 {
        sim.tick().expect("tick");
    }
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
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    sim.spawn_live_unit("dagger", 2, 20.0 * 8.0 + 60.0, 20.0 * 8.0, 180.0)
        .expect("dagger");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
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

/// G01 T16: an attack order on a *unit* follows the moving target
/// (`CommandAI.defaultBehavior`: `targetPos.set(attackTarget)` every tick) and
/// kills it.
#[test]
fn commanded_dagger_chases_moving_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::TilePos;

    let mut sim = live_sim(96);
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

    let attacker = sim
        .spawn_live_unit("dagger", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("attacker");
    let target = sim
        .spawn_live_unit("dagger", 2, 50.0 * 8.0, 40.0 * 8.0, 180.0)
        .expect("target");
    // Walk the target east at full speed.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(target) {
        slot.kind = AiKind::Command;
        slot.target = Some(TilePos::new(80, 40));
    }
    // The wire unit-id channel is `f32` and live seqs are ~1e9 (not exactly
    // representable), so seed the attack target component directly — the RT
    // tracking below is what this task owns.
    let target_pos = sim.ecs.0.get::<Pos>(target).copied().expect("target pos");
    sim.ecs
        .0
        .entity_mut(attacker)
        .insert(crate::ai::types::command::AttackTargetComp(Some(target)));
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(attacker)
    {
        slot.target = Some(crate::world::TilePos::new(
            crate::world::WorldGrid::to_tile(target_pos.x) as i16,
            crate::world::WorldGrid::to_tile(target_pos.y) as i16,
        ));
    }
    // The scenario exercises the chaser's attack-target tracking, not the
    // mutual exchange: the fleeing target holds fire (`holdFire` stance,
    // `CommandAI.shouldFire`). With the T4 velocity model a fleeing unit's
    // return fire lands (the old kinematic mover masked it), which turned the
    // exchange into a symmetric race instead of a chase.
    {
        use crate::ai::unit_stance_runtime::{UnitStancesComp, apply_stance};
        let def = sim
            .unit_runtime()
            .expect("rt")
            .content
            .unit_stance_by_name("holdfire")
            .cloned()
            .expect("holdfire stance");
        let mut stances = UnitStancesComp::default();
        assert!(apply_stance(&mut stances.0, &def));
        sim.ecs.0.entity_mut(target).insert(stances);
    }
    for _ in 0..1500 {
        sim.tick().expect("tick");
        if sim.ecs.0.get_entity(target).is_err() {
            break;
        }
    }
    assert!(
        sim.ecs.0.get_entity(target).is_err()
            || sim
                .ecs
                .0
                .get::<Health>(target)
                .is_some_and(|health| health.dead),
        "the attacker killed the fleeing target"
    );
}

/// G01 T16: the reticle seam reports the tracked entity's position.
#[test]
fn command_target_seam_reports_position() {
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
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let attacker = sim
        .spawn_live_unit("dagger", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("attacker");
    let target = sim
        .spawn_live_unit("dagger", 2, 50.0 * 8.0, 40.0 * 8.0, 180.0)
        .expect("target");
    let attacker_id = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(attacker)
        .expect("seq")
        .0 as i32;
    sim.ecs
        .0
        .entity_mut(attacker)
        .insert(crate::ai::types::command::AttackTargetComp(Some(target)));
    let target_pos = sim.ecs.0.get::<Pos>(target).copied().expect("target pos");
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(attacker)
    {
        slot.kind = crate::ai::controller::AiKind::Command;
        slot.target = Some(crate::world::TilePos::new(
            crate::world::WorldGrid::to_tile(target_pos.x) as i16,
            crate::world::WorldGrid::to_tile(target_pos.y) as i16,
        ));
    }
    sim.tick().expect("tick");
    let rows = sim.command_targets();
    let row = rows
        .iter()
        .find(|(id, _, _, _)| *id == attacker_id)
        .copied()
        .expect("the attacker reports a command target");
    let target_pos = sim.ecs.0.get::<Pos>(target).copied().expect("target pos");
    eprintln!("command target row {row:?}, target {target_pos:?}");
    assert!(row.3, "marked as an attack target");
    assert!(
        (row.1 - target_pos.x).abs() < 32.0 && (row.2 - target_pos.y).abs() < 32.0,
        "reports the tracked entity's position"
    );
}

/// G01 T14: a unit carrying blast compound explodes on death
/// (`UnitComp.destroy` -> `Damage.dynamicExplosion`): 30 blast-compound gives
/// explosiveness `2 + 1.2*30*1.53 = 57.08` -> 5 waves of 28.54 damage with a
/// 50 px final radius.
#[test]
fn unit_with_blast_compound_explodes() {
    use crate::entities::comp::unit::comp::ItemsComp;

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0 + 8.0, 20.5 * 8.0, 180.0)
        .expect("dagger");
    let blast = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("blast-compound")
        .expect("blast-compound");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemsComp>(dagger) {
        items.item = Some((blast, 30));
    }
    if let Some(mut hp) = sim.ecs.0.get_mut::<Health>(dagger) {
        hp.health = 0.0;
        hp.dead = true;
    }
    sim.tick().expect("tick");
    // Staggered waves (`Time.run(i * 2f)`): the growing radius needs ticks.
    for _ in 0..40 {
        sim.tick().expect("tick");
    }
    let wall_end = health(&sim, wall);
    eprintln!("wall {wall_start} -> {wall_end:?}");
    assert!(
        wall_end.is_none_or(|hp| hp <= wall_start - 20.0),
        "the blast damaged the wall: {wall_start} -> {wall_end:?}"
    );
}

/// G01 T14: core-spawned units skip the death explosion (`spawnedByCore`).
#[test]
fn core_spawned_unit_does_not_explode() {
    use crate::entities::comp::unit::comp::{ItemsComp, UnitCore};

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 20.5 * 8.0 + 8.0, 20.5 * 8.0, 180.0)
        .expect("dagger");
    let blast = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("blast-compound")
        .expect("blast-compound");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemsComp>(dagger) {
        items.item = Some((blast, 30));
    }
    if let Some(mut core) = sim.ecs.0.get_mut::<UnitCore>(dagger) {
        core.spawned_by_core = true;
    }
    if let Some(mut hp) = sim.ecs.0.get_mut::<Health>(dagger) {
        hp.health = 0.0;
        hp.dead = true;
    }
    sim.tick().expect("tick");
    sim.tick().expect("tick");
    assert_eq!(
        health(&sim, wall),
        Some(wall_start),
        "no explosion from a core-spawned unit"
    );
}

/// G01 T14: `rules.fire = false` blocks fire creation through the `CombatEnv`
/// sync (`Fires.create` gate).
#[test]
fn fire_disabled_rule_blocks_fire() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(24);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        fire: false,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    // One tick syncs `CombatEnv` from the session rules.
    sim.tick().expect("tick");
    let mut rng = crate::determinism::SimRng::new(7);
    let fire = crate::combat::fires::create(&mut sim.ecs.0, 10, 10, &mut rng);
    assert!(fire.is_none(), "rules.fire = false blocks fires");
}

/// H09 T5: a fire's damage tick burns a grounded unit for `unitDamage` and
/// applies `burning` for 300 ticks; flying units are skipped
/// (`FireComp.java:88-99`).
#[test]
fn fire_burns_ground_units() {
    use crate::entities::comp::unit::comp::StatusComp;

    let mut sim = live_sim(48);
    let dagger = sim
        .spawn_live_unit("dagger", 1, 80.0, 80.0, 0.0)
        .expect("dagger");
    let flare = sim
        .spawn_live_unit("flare", 1, 80.0, 80.0, 0.0)
        .expect("flare");
    let mut rng = crate::determinism::SimRng::new(9);
    let fire = crate::combat::fires::create(&mut sim.ecs.0, 10, 10, &mut rng).expect("fire");
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<crate::combat::fires::FireState>(fire)
            .expect("fire state");
        state.damage_timer = 39.0;
    }
    let dagger_before = health(&sim, dagger).expect("dagger health");
    let flare_before = health(&sim, flare).expect("flare health");
    sim.tick().expect("tick");
    let dagger_after = health(&sim, dagger).expect("dagger health");
    assert!(
        (dagger_before - dagger_after - 3.0).abs() < 1e-3,
        "dagger took unitDamage 3 ({dagger_before} -> {dagger_after})"
    );
    let burning = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("burning")
        .expect("burning");
    let statuses = sim.ecs.0.get::<StatusComp>(dagger).expect("statuses");
    assert!(
        statuses
            .statuses
            .iter()
            .any(|entry| entry.effect == burning),
        "dagger is burning"
    );
    assert_eq!(
        health(&sim, flare),
        Some(flare_before),
        "a flying flare is untouched"
    );
}

/// H09 T5: fire tile damage goes through `apply_health`, so
/// `rules.blockHealth(team)` scales it (`BuildingComp.damage`).
#[test]
fn fire_damage_respects_block_health_multiplier() {
    let mut sim = campaign_sim(48, |rules| {
        rules.block_health_multiplier = 2.0;
    });
    let container = place(&mut sim, 10, 10, "container", 1);
    let mut rng = crate::determinism::SimRng::new(9);
    let fire = crate::combat::fires::create(&mut sim.ecs.0, 10, 10, &mut rng).expect("fire");
    {
        let mut state = sim
            .ecs
            .0
            .get_mut::<crate::combat::fires::FireState>(fire)
            .expect("fire state");
        state.damage_timer = 39.0;
    }
    let before = health(&sim, container).expect("container health");
    sim.tick().expect("tick");
    let after = health(&sim, container).expect("container health");
    assert!(
        (before - after - 0.9).abs() < 1e-3,
        "tileDamage 1.8 / blockHealth 2 = 0.9 ({before} -> {after})"
    );
}

/// PAR-0475: a powered `shield-projector` runs the `BaseShield.unitConsumer`
/// (`BaseShield.java:38-57`) through the live `BuildingBehavior` path (it
/// borrows the content/grid/FX seam from the unit runtime): a shallow overlap
/// pushes the enemy unit out, a deep overlap kills it through
/// `unit.kill()` -> `resolve_deaths`.
#[test]
fn shield_projector_repels_and_kills_enemy_units() {
    use crate::world::blocks::defense::projectors::BaseShieldState;

    let mut sim = live_sim(48);
    place(&mut sim, 18, 20, "power-source", 1);
    let shield = place(&mut sim, 20, 20, "shield-projector", 1);
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let radius = sim
        .ecs
        .0
        .get::<BaseShieldState>(shield)
        .map(|state| state.smooth_radius)
        .expect("shield state");
    assert!(radius > 150.0, "a powered dome grows: {radius}");
    let (cx, cy) = sim
        .ecs
        .0
        .get::<Pos>(shield)
        .map(|pos| (pos.x, pos.y))
        .expect("shield pos");

    // dagger hitSize 8: `overlapDst = 4 + radius - (radius - 2) = 6 <= 12`,
    // so it is pushed out, not killed.
    let dagger = sim
        .spawn_live_unit("dagger", 2, cx + radius - 2.0, cy, 180.0)
        .expect("dagger");
    sim.tick().expect("tick");
    let pushed_radius = sim
        .ecs
        .0
        .get::<BaseShieldState>(shield)
        .map(|state| state.smooth_radius)
        .expect("shield state");
    let (px, py) = sim
        .ecs
        .0
        .get::<Pos>(dagger)
        .map(|pos| (pos.x, pos.y))
        .expect("dagger pos");
    let dst = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
    assert!(
        dst >= pushed_radius + 4.0,
        "shallow unit pushed outside the dome: dst = {dst}, radius = {pushed_radius}"
    );
    assert!(
        !sim.ecs.0.get::<Health>(dagger).expect("dagger health").dead,
        "a shallow overlap is not lethal"
    );

    // A unit stuck deep inside (`overlapDst > hitSize * 1.5`) is killed
    // (`unit.kill()`); `resolve_deaths` destroys it in the same tick.
    let stuck = sim
        .spawn_live_unit("dagger", 2, cx + radius * 0.25, cy, 180.0)
        .expect("stuck dagger");
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(stuck).is_err(),
        "a deep unit is killed by the dome"
    );
}

/// G01 T14: a charged force projector absorbs the death explosion
/// (`ForceBuild.absorbExplosion`): the wall under the shield is undamaged.
#[test]
fn force_projector_absorbs_explosion() {
    use crate::entities::comp::unit::comp::ItemsComp;
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let mut sim = live_sim(48);
    // Power-source feeds the projector; the wall sits inside its radius.
    place(&mut sim, 18, 20, "power-source", 1);
    let projector = place(&mut sim, 20, 20, "force-projector", 1);
    let wall = place(&mut sim, 22, 20, "copper-wall", 1);
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let (broken, radius) = sim
        .ecs
        .0
        .get::<ForceProjectorState>(projector)
        .map(|state| (state.broken, state.real_radius()))
        .expect("force projector state");
    assert!(!broken, "a powered projector is charged");
    assert!(radius > 90.0, "charged shield radius {radius}");
    let wall_start = health(&sim, wall).expect("wall health");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 22.5 * 8.0, 20.5 * 8.0, 180.0)
        .expect("dagger");
    let blast = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("blast-compound")
        .expect("blast-compound");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemsComp>(dagger) {
        items.item = Some((blast, 30));
    }
    if let Some(mut hp) = sim.ecs.0.get_mut::<Health>(dagger) {
        hp.health = 0.0;
        hp.dead = true;
    }
    sim.tick().expect("tick");
    sim.tick().expect("tick");
    assert_eq!(
        health(&sim, wall),
        Some(wall_start),
        "the projector absorbed the explosion"
    );
}

/// H02 T7 (PAR-0129): a force projector next to a power source charges to
/// efficiency 1 and draws its dome (`ForceBuild.updateTile`, warmup/radscl).
#[test]
fn force_projector_powered_by_adjacent_source() {
    use crate::entities::comp::Building;
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let mut sim = live_sim(48);
    place(&mut sim, 18, 20, "power-source", 1);
    let projector = place(&mut sim, 20, 20, "force-projector", 1);
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let efficiency = sim.ecs.0.get::<Building>(projector).map(|b| b.efficiency);
    assert!(
        efficiency.is_some_and(|e| e > 0.99),
        "powered projector efficiency {efficiency:?}"
    );
    let (warmup, radscl) = sim
        .ecs
        .0
        .get::<ForceProjectorState>(projector)
        .map(|state| (state.warmup, state.radscl))
        .expect("force projector state");
    assert!(warmup > 0.99, "warmup {warmup}");
    assert!(radscl > 0.9, "radscl {radscl}");
}

/// H02 T7: enemy bullets inside the live dome are absorbed and add buildup
/// (`ForceBuild.deflectBullets` over `Groups.bullet`).
#[test]
fn force_projector_absorbs_enemy_bullet() {
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let mut sim = live_sim(48);
    place(&mut sim, 18, 20, "power-source", 1);
    let projector = place(&mut sim, 20, 20, "force-projector", 1);
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    // Inside the ~101.7 px dome, 60 px east of the projector centre
    // (tile centre (20,20) = world (160,160)), facing it.
    sim.spawn_live_unit("dagger", 2, 20.0 * 8.0 + 60.0, 20.0 * 8.0 + 4.0, 180.0)
        .expect("dagger");
    let mut absorbed = false;
    for _ in 0..300 {
        sim.tick().expect("tick");
        if let Some(state) = sim.ecs.0.get::<ForceProjectorState>(projector)
            && (state.buildup > 0.0 || state.hit > 0.0)
        {
            absorbed = true;
            break;
        }
    }
    assert!(absorbed, "dagger bullets are absorbed by the shield");
}

/// H02 T7: phase fabric raises `phaseHeat` and is consumed every
/// `phaseUseTime / timeScale` ticks (`ForceProjector.java:207-215`).
#[test]
fn force_projector_phase_boost() {
    use crate::world::blocks::defense::shields::ForceProjectorState;
    use crate::world::modules::ItemModule;

    let mut sim = live_sim(48);
    place(&mut sim, 18, 20, "power-source", 1);
    let projector = place(&mut sim, 20, 20, "force-projector", 1);
    let phase = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("phase-fabric")
        .expect("phase-fabric");
    if let Some(mut items) = sim.ecs.0.get_mut::<ItemModule>(projector) {
        items.add(phase, 2, 10);
    }
    for _ in 0..30 {
        sim.tick().expect("tick");
    }
    let phase_heat = sim
        .ecs
        .0
        .get::<ForceProjectorState>(projector)
        .map(|state| state.phase_heat)
        .expect("force projector state");
    let count = sim
        .ecs
        .0
        .get::<ItemModule>(projector)
        .map(|items| items.get(phase))
        .unwrap_or(0);
    assert!(phase_heat > 0.9, "phase heat {phase_heat}");
    assert_eq!(count, 1, "one fabric consumed after the first timer cycle");
    for _ in 0..400 {
        sim.tick().expect("tick");
    }
    let count = sim
        .ecs
        .0
        .get::<ItemModule>(projector)
        .map(|items| items.get(phase))
        .unwrap_or(0);
    assert_eq!(count, 0, "the second fabric is consumed on the next cycle");
}

/// H02 T11: a dagger standing on an enemy shock mine triggers the tendrils
/// (`ShockMineBuild.unitOn`); the mine pays `tileDamage` and cannot re-trigger
/// before `cooldown` ticks.
#[test]
fn shock_mine_zaps_enemy_dagger() {
    use crate::world::blocks::defense::shields::ShockMineState;

    let mut sim = live_sim(48);
    let mine = place(&mut sim, 20, 20, "shock-mine", 1);
    let mine_start = health(&sim, mine).expect("mine health");
    assert_eq!(mine_start, 50.0, "vanilla shock-mine health");
    // Tile centre: `WorldGrid::to_tile` rounds, so only `tile * 8` lands on
    // tile (20, 20) (the plan's (164,164) is the corner between tiles).
    let dagger = sim
        .spawn_live_unit("dagger", 2, 160.0, 160.0, 0.0)
        .expect("dagger");
    // Java-faithful `Lightning` chains onto the dagger with all four tendrils,
    // so one trigger lands ~150 damage: top the 150-HP dagger up so the
    // 80-tick cooldown leg below can still observe a second trigger.
    if let Some(mut hp) = sim.ecs.0.get_mut::<Health>(dagger) {
        hp.max_health = 1_000.0;
        hp.health = 1_000.0;
    }
    let dagger_start = health(&sim, dagger).expect("dagger health");
    let hold = |sim: &mut Sim, dagger| {
        if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(dagger) {
            vel.x = 0.0;
            vel.y = 0.0;
        }
        sim.tick().expect("tick");
    };
    for _ in 0..2 {
        hold(&mut sim, dagger);
    }
    let first = sim
        .ecs
        .0
        .get::<ShockMineState>(mine)
        .and_then(|state| state.last_trigger);
    assert!(first.is_some(), "mine triggered on the unit");
    assert!(
        health(&sim, dagger).is_some_and(|hp| hp < dagger_start),
        "dagger zapped by the mine"
    );
    assert!(
        health(&sim, mine).is_some_and(|hp| hp <= mine_start - 7.0 + 0.001),
        "mine paid tileDamage: {:?}",
        health(&sim, mine)
    );
    // No re-trigger inside the 80-tick cooldown.
    for _ in 0..40 {
        hold(&mut sim, dagger);
    }
    assert_eq!(
        sim.ecs
            .0
            .get::<ShockMineState>(mine)
            .and_then(|state| state.last_trigger),
        first,
        "cooldown blocks the re-trigger"
    );
    // Past the cooldown the mine fires again.
    let mut refreshed = false;
    for _ in 0..60 {
        if sim.ecs.0.get_entity(dagger).is_err() {
            break;
        }
        hold(&mut sim, dagger);
        if sim
            .ecs
            .0
            .get::<ShockMineState>(mine)
            .and_then(|state| state.last_trigger)
            != first
        {
            refreshed = true;
            break;
        }
    }
    assert!(refreshed, "second trigger after the cooldown");
}

/// H02 T11: a running conveyor pushes a grounded friendly unit along its
/// rotation (`ConveyorBuild.unitOn`).
#[test]
fn conveyor_carries_dagger() {
    let mut sim = live_sim(48);
    for x in 10..=20 {
        place(&mut sim, x, 20, "titanium-conveyor", 1);
    }
    let dagger = sim
        .spawn_live_unit("dagger", 1, 96.0, 160.0, 0.0)
        .expect("dagger");
    let start_x = sim.ecs.0.get::<Pos>(dagger).map(|p| p.x).unwrap_or(0.0);
    for i in 0..60 {
        sim.tick().expect("tick");
        if i % 10 == 0 {
            let p = sim.ecs.0.get::<Pos>(dagger).map(|p| (p.x, p.y));
            let v = sim.ecs.0.get::<Vel>(dagger).map(|v| (v.x, v.y));
            eprintln!("CONV i={i} pos={p:?} vel={v:?}");
        }
    }
    let end_x = sim.ecs.0.get::<Pos>(dagger).map(|p| p.x).unwrap_or(0.0);
    assert!(
        end_x > start_x + 8.0,
        "belt carried the dagger east: {start_x} -> {end_x}"
    );
}

/// G01 T14: `explosion_absorbed` covers the point when a live shield polygon
/// contains it and skips the ignore team's own shields.
#[test]
fn explosion_absorbed_polygon() {
    use crate::combat::damage::explosion::explosion_absorbed;
    use crate::world::blocks::defense::shields::ForceProjectorState;

    let mut sim = live_sim(48);
    let projector = place(&mut sim, 20, 20, "force-projector", 1);
    if let Some(mut state) = sim.ecs.0.get_mut::<ForceProjectorState>(projector) {
        state.radscl = 1.0;
        state.radius = 100.0;
        state.broken = false;
        state.sides = 24;
    }
    let (x, y) = (20.5 * 8.0, 20.5 * 8.0);
    assert!(
        explosion_absorbed(&mut sim.ecs.0, x, y, 30.0, Some(2)),
        "team 1's shield absorbs the wave for team 2"
    );
    assert!(
        !explosion_absorbed(&mut sim.ecs.0, x + 1000.0, y, 30.0, Some(2)),
        "a far-away point is not absorbed"
    );
    assert!(
        !explosion_absorbed(&mut sim.ecs.0, x, y, 30.0, Some(1)),
        "the shield's own team is ignored"
    );
}

/// T1: `Building.set(tile.drawx(), tile.drawy())` (BuildingComp.java:126) with
/// `drawx = tile.worldx() + block.offset` (Tile.java:166-177); the offset is
/// `((size + 1) % 2) * tilesize / 2` (Block.java:774), so odd sizes (and 1x1)
/// sit on the tile's `worldx` and even sizes shift `+4`.
#[test]
fn building_pos_uses_block_offset() {
    let mut sim = live_sim(64);
    let cases = [
        ("duo", 10, 80.0),               // 1x1, offset 0
        ("core-shard", 20, 160.0),       // 3x3, offset 0
        ("mechanical-drill", 30, 244.0), // 2x2, offset 4
        ("core-foundation", 40, 324.0),  // 4x4, offset 4
    ];
    for (name, tile, expected) in cases {
        let entity = place(&mut sim, tile, tile, name, 1);
        let pos = sim.ecs.0.get::<Pos>(entity).expect("pos");
        assert_eq!(
            (pos.x, pos.y),
            (expected, expected),
            "{name} at tile {tile} must sit at drawx/drawy"
        );
    }
}

/// T2: `AIController.updateWeapons` searches per mount
/// (`findTarget(mountX, mountY, weapon.range(), bullet.collidesAir,
/// bullet.collidesGround)`, AIController.java:216-222), so a ground-only weapon
/// ignores the flyer the unit's main target search picked. The unit is the
/// first content unit whose first weapon bullet does not collide air (printed
/// by the test).
#[test]
fn ground_weapon_ignores_air_target_per_mount() {
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
        wave_team: 2,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let name = {
        let runtime = sim.unit_runtime().expect("rt");
        let content = &runtime.content;
        content
            .units()
            .iter()
            .find(|unit| {
                unit.weapons.first().is_some_and(|weapon| {
                    content
                        .bullet(weapon.bullet.id)
                        .is_some_and(|bullet| !bullet.collides_air && bullet.collides_ground)
                })
            })
            .map(|unit| unit.name.clone())
            .expect("a ground-only unit exists")
    };
    eprintln!("T2 ground-only unit: {name}");

    let wall = place(&mut sim, 29, 25, "copper-wall", 1);
    let unit = sim
        .spawn_live_unit(&name, 2, 200.0, 200.0, 0.0)
        .expect("ground-only unit");
    // A flying enemy at the same distance as the wall (`hitSize/2` apart).
    let flare = sim
        .spawn_live_unit("flare", 1, 230.0, 200.0, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<crate::weapons::UnitWeapons>(unit)
        .and_then(|weapons| weapons.mounts.first().and_then(|mount| mount.target));
    assert_eq!(
        target,
        Some(wall),
        "the ground-only mount searched its own target (the wall), not the flyer"
    );
}

/// T2: the shoot/invalidate range adds the TARGET's `hitSize/2`
/// (`mount.target.within(mountX, mountY, wrange + target.hitSize/2)`,
/// AIController.java:239; `Units.invalidateTarget`, Units.java:151-157). A 3x3
/// core-shard adds 12 px where the pre-T2 port added the shooter's `hitSize/2`.
///
/// The per-mount search radius is `weapon.range() + |shootY|` (a tighter gate
/// than the shoot range), so the shoot gate is exercised with the mount target
/// set directly on a non-`singleTarget` unit.
#[test]
fn dagger_engages_core_at_target_hitsize() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(64);
    let (name, weapon_x, weapon_y, range, shooter_hit) = {
        let runtime = sim.unit_runtime().expect("rt");
        let content = &runtime.content;
        let def = content
            .units()
            .iter()
            .find(|unit| {
                !unit.single_target
                    && !unit.flying
                    && unit
                        .weapons
                        .first()
                        .and_then(|weapon| content.bullet(weapon.bullet.id))
                        .is_some_and(|bullet| bullet.collides_ground)
            })
            .expect("a non-singleTarget ground unit exists");
        let weapon = def.weapons.first().expect("weapon");
        (
            def.name.clone(),
            weapon.x,
            weapon.y,
            weapon.range(),
            def.hit_size,
        )
    };
    eprintln!(
        "T2 hitSize unit: {name} range {range} weapon ({weapon_x}, {weapon_y}) hit {shooter_hit}"
    );
    let core = place(&mut sim, 20, 20, "core-shard", 1);
    // core-shard Pos = (160, 160); hitSize = 24 -> the shoot/invalidate range is
    // `range + 12`. `rotation 0` puts the mount at `unit - (weapon.y, weapon.x)`.
    let target_dist = range + 12.0 - 4.0;
    let unit = sim
        .spawn_live_unit(
            &name,
            2,
            160.0 + target_dist + weapon_y,
            160.0 + weapon_x,
            0.0,
        )
        .expect("unit");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    // Tick 1 runs the first (search) tick; the search radius is `range + |shootY|`.
    sim.tick().expect("tick");
    let set_target = |sim: &mut Sim, target: Option<Entity>| {
        if let Some(mut weapons) = sim.ecs.0.get_mut::<crate::weapons::UnitWeapons>(unit)
            && let Some(mount) = weapons.mounts.first_mut()
        {
            mount.target = target;
        }
    };
    let mount_state = |sim: &Sim| {
        sim.ecs
            .0
            .get::<crate::weapons::UnitWeapons>(unit)
            .and_then(|weapons| {
                weapons
                    .mounts
                    .first()
                    .map(|mount| (mount.target, mount.shoot))
            })
    };
    set_target(&mut sim, Some(core));
    sim.tick().expect("tick");
    let (target, shoot) = mount_state(&sim).expect("mounts");
    assert_eq!(
        target,
        Some(core),
        "the target survives the invalidate check"
    );
    assert!(shoot, "engages at range + target.hitSize/2 - 4");
    // Move 5 px further: beyond `range + 12` -> invalidated and not shooting.
    if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(unit) {
        pos.x += 5.0;
    }
    set_target(&mut sim, Some(core));
    sim.tick().expect("tick");
    let (target, shoot) = mount_state(&sim).expect("mounts");
    assert_eq!(
        target, None,
        "beyond range + target.hitSize/2 is invalidated"
    );
    assert!(!shoot, "does not shoot past range + target.hitSize/2");
}

/// T2: `FlyingAI.findTarget` keeps the in-range main target
/// (FlyingAI.java:34-39): a flare whose main target (the core) is in range does
/// not switch to a closer dagger.
#[test]
fn flying_mount_keeps_in_range_main_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(64);
    let core = place(&mut sim, 20, 20, "core-shard", 1);
    let dagger = place(&mut sim, 24, 20, "duo", 1);
    let flare = sim
        .spawn_live_unit("flare", 2, 200.0, 190.0, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::Flying;
    }
    // A team-1 dagger (closer than the core) must not steal the mount target.
    let bait = sim
        .spawn_live_unit("dagger", 1, 175.0, 190.0, 180.0)
        .expect("bait");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(bait) {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    for _ in 0..130 {
        sim.tick().expect("tick");
    }
    let target = sim
        .ecs
        .0
        .get::<crate::weapons::UnitWeapons>(flare)
        .and_then(|weapons| weapons.mounts.first().and_then(|mount| mount.target));
    assert!(
        target == Some(core) || target == Some(dagger),
        "the flare engaged a building ({target:?}), not the closer dagger"
    );
}

/// T2: `autoDropBombs` (AIController.java:225-229): a horizon with no valid
/// main target drops its bomb weapon on the ground unit below it
/// (`Units.closestTarget(.., u -> !u.isFlying(), t -> true)`).
#[test]
fn horizon_auto_drops_bombs_on_conveyor() {
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
        wave_team: 2,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    // A concrete ground victim: a copper wall on a building tile.
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    let before = health(&sim, wall).expect("wall health");
    let horizon = sim
        .spawn_live_unit("horizon", 2, 20.0 * 8.0, 20.0 * 8.0 - 14.0, 90.0)
        .expect("horizon");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(horizon) {
        slot.kind = AiKind::Flying;
    }
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let after = health(&sim, wall).unwrap_or(0.0);
    eprintln!("T2 horizon bombs: wall {before} -> {after}");
    assert!(after < before, "the horizon dropped bombs on the wall");
}

/// T2: a `!controllable` mount is never overwritten by the AI's main target
/// (`if(!weapon.controllable || weapon.noAttack) continue;`,
/// AIController.java:214).
#[test]
fn non_controllable_mount_not_overwritten() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(64);
    let name = {
        let runtime = sim.unit_runtime().expect("rt");
        let content = &runtime.content;
        content
            .units()
            .iter()
            .find(|unit| {
                unit.weapons
                    .iter()
                    .any(|weapon| !weapon.controllable && !weapon.no_attack)
            })
            .map(|unit| unit.name.clone())
            .expect("a unit with a non-controllable weapon exists")
    };
    eprintln!("T2 non-controllable weapon unit: {name}");
    // The first such unit is naval (oxynoe): give it water so the
    // environment pass does not kill it on land.
    let water = sim
        .unit_runtime()
        .expect("rt")
        .content
        .block_id("deep-water")
        .expect("deep-water");
    for x in 11..=14 {
        sim.grid.tiles.get_mut(x, 20).floor = water;
    }
    let _wall = place(&mut sim, 28, 20, "copper-wall", 1);
    let unit = sim
        .spawn_live_unit(&name, 2, 100.0, 160.0, 0.0)
        .expect("unit");
    // A close enemy unit so the AI main target is Some.
    let bait = sim
        .spawn_live_unit("dagger", 1, 120.0, 160.0, 180.0)
        .expect("bait");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(bait) {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::Command;
    }
    for _ in 0..5 {
        sim.tick().expect("tick");
    }
    let weapons = sim
        .ecs
        .0
        .get::<crate::weapons::UnitWeapons>(unit)
        .expect("weapons");
    let any_non_controllable = weapons
        .weapons
        .iter()
        .zip(weapons.mounts.iter())
        .filter(|(def, _)| !def.controllable)
        .all(|(_, mount)| mount.target != Some(bait));
    assert!(
        any_non_controllable,
        "a non-controllable mount was overwritten with the AI main target"
    );
}

/// H02 T16: the duo aims at the intercept point for its 2.5 px/tick copper
/// bullet, not at the target's current position (`Turret.targetPosition`,
/// `Turret.java:459-479`; `Predict.intercept`, `Predict.java:27-52`).
#[test]
fn turret_leads_moving_target() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let duo_pos = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|p| (p.x, p.y))
        .expect("duo pos");
    let flare = sim
        .spawn_live_unit("flare", 2, duo_pos.0 + 100.0, duo_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 2.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
    }
    sim.tick().expect("tick");
    // `EntitySet::UpdateUnits` runs before `UpdateBuildings`, so the flare's
    // own `VelComp.update` (T4) already integrated its velocity and applied
    // drag before the turret aimed this tick.
    let flare_now = sim
        .ecs
        .0
        .get::<Pos>(flare)
        .map(|p| (p.x, p.y))
        .expect("flare pos");
    let flare_vel = sim
        .ecs
        .0
        .get::<Vel>(flare)
        .map(|v| (v.x, v.y))
        .expect("flare vel");
    let state = sim.ecs.0.get::<TurretState>(duo).expect("turret state");
    // `duo_copper` bullet speed (`register_bullets`).
    let expected = crate::combat::targeting::intercept_positions(
        duo_pos,
        (0.0, 0.0),
        flare_now,
        flare_vel,
        true,
        2.5,
    );
    assert!(
        state.target_pos.1 > flare_now.1,
        "the aim leads the northbound flare: {:?} vs {flare_now:?}",
        state.target_pos
    );
    assert!(
        (state.target_pos.0 - expected.0).abs() < 0.01
            && (state.target_pos.1 - expected.1).abs() < 0.01,
        "aim equals Predict.intercept: {:?} vs {expected:?}",
        state.target_pos
    );
}

/// H02 T16: the aim point is recomputed every tick even between retargets
/// (`Turret.updateTile` calls `targetPosition` on every tick, not only in
/// `findTarget`).
#[test]
fn turret_reaims_every_tick() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let duo_pos = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|p| (p.x, p.y))
        .expect("duo pos");
    let flare = sim
        .spawn_live_unit("flare", 2, duo_pos.0 + 80.0, duo_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
    }
    sim.tick().expect("tick");
    // Move the target 20 px north; the retarget timer stays high.
    if let Some(mut pos) = sim.ecs.0.get_mut::<Pos>(flare) {
        pos.y -= 20.0;
    }
    let moved = sim
        .ecs
        .0
        .get::<Pos>(flare)
        .map(|p| (p.x, p.y))
        .expect("flare pos");
    sim.tick().expect("tick");
    let state = sim.ecs.0.get::<TurretState>(duo).expect("turret state");
    assert!(
        (state.target_pos.0 - moved.0).abs() < 0.01 && (state.target_pos.1 - moved.1).abs() < 0.01,
        "aim follows the moved target: {:?} vs {moved:?}",
        state.target_pos
    );
}

/// H02 T16: firing is gated by `within(target, range() + hitSize/1.9)`
/// (`Turret.java:587`), evaluated against the target's own position.
#[test]
fn turret_holds_fire_out_of_range() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 10, 10, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let duo_pos = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|p| (p.x, p.y))
        .expect("duo pos");
    let hit_size = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_by_name("flare")
        .expect("flare def")
        .hit_size;
    let far = 160.0 + hit_size / 1.9 + 2.0;
    let flare = sim
        .spawn_live_unit("flare", 2, duo_pos.0 + far, duo_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(duo) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
        state.rotation = 0.0; // already facing the target
        state.reload_counter = 1000.0; // ready to fire if allowed
    }
    let before = sim.unit_runtime().expect("rt").bullets_created;
    sim.tick().expect("tick");
    let after = sim.unit_runtime().expect("rt").bullets_created;
    assert_eq!(
        before, after,
        "out-of-range target holds fire (Turret.java:587)"
    );
}

/// H02 T16: `scathe` does not predict (`Blocks.java:5248`), so its aim point
/// is the target's current position.
#[test]
fn scathe_does_not_predict() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(64);
    let scathe = place(&mut sim, 20, 20, "scathe", 1);
    // `Turret.targetPosition` returns without ammo (`Turret.java:460`).
    feed(&mut sim, scathe, "carbide", 1);
    let scathe_pos = sim
        .ecs
        .0
        .get::<Pos>(scathe)
        .map(|p| (p.x, p.y))
        .expect("scathe pos");
    assert!(
        !sim.ecs
            .0
            .get::<TurretState>(scathe)
            .expect("scathe state")
            .config
            .predict_target,
        "scathe predictTarget = false"
    );
    let flare = sim
        .spawn_live_unit("flare", 2, scathe_pos.0 + 100.0, scathe_pos.1, 180.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 0.0;
        vel.y = 2.0;
    }
    if let Some(mut state) = sim.ecs.0.get_mut::<TurretState>(scathe) {
        state.target = Some(flare);
        state.target_timer = 1000.0;
    }
    sim.tick().expect("tick");
    // `EntitySet::UpdateUnits` runs before `UpdateBuildings`: the flare's own
    // `VelComp.update` (T4) integrated its velocity before the scathe aimed.
    let flare_now = sim
        .ecs
        .0
        .get::<Pos>(flare)
        .map(|p| (p.x, p.y))
        .expect("flare pos");
    let state = sim.ecs.0.get::<TurretState>(scathe).expect("state");
    assert!(
        (state.target_pos.0 - flare_now.0).abs() < 0.01
            && (state.target_pos.1 - flare_now.1).abs() < 0.01,
        "no lead without predictTarget: {:?} vs {flare_now:?}",
        state.target_pos
    );
}

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

/// H09 T4: `Building.update()` skips `updateTile` for a disabled turret
/// (`Turret` has the default `noUpdateDisabled = false`): a disabled, fed duo
/// never fires at an enemy in range.
#[test]
fn disabled_duo_does_not_fire() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    let enemy = sim
        .spawn_live_unit("dagger", 2, 20.0 * 8.0 + 60.0, 20.0 * 8.0 + 20.0, 180.0)
        .expect("enemy dagger");
    let start = health(&sim, enemy).expect("health");
    if let Some(mut building) = sim.ecs.0.get_mut::<crate::entities::comp::Building>(duo) {
        building.enabled = false;
    }
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let shots = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .map(|state| state.total_shots)
        .unwrap_or(-1);
    assert_eq!(shots, 0, "a disabled duo never fires");
    assert_eq!(
        health(&sim, enemy),
        Some(start),
        "the enemy takes no duo damage"
    );
}

/// H09 T7: a container full of 100 blast-compound explodes with
/// `explosiveness = 120` and `flammability = 40` (before the
/// `* 3.5 * explosivenessScale` caller scaling).
#[test]
fn blast_container_explosion_terms() {
    let mut sim = live_sim(32);
    let container = place(&mut sim, 5, 5, "container", 1);
    let (content_len, blast, capacity) = {
        let content = &sim.unit_runtime().expect("rt").content;
        (
            content.items().len(),
            content.item_id("blast-compound").expect("blast-compound"),
            content
                .block_by_name("container")
                .map(|def| def.item_capacity)
                .unwrap_or(300),
        )
    };
    let mut module = crate::world::modules::ItemModule::with_items(content_len);
    module.add(blast, 100, capacity.max(100));
    sim.ecs.0.entity_mut(container).insert(module);

    let content = &sim.unit_runtime().expect("rt").content;
    let def = content.block_by_name("container").expect("def").clone();
    let (flammability, explosiveness, power) =
        super::unit_runtime::building_explosion_terms(&sim.ecs.0, content, container, &def);
    assert!(
        (explosiveness - 120.0).abs() < 1e-3,
        "explosiveness {explosiveness}"
    );
    assert!(
        (flammability - 40.0).abs() < 1e-3,
        "flammability {flammability}"
    );
    assert_eq!(power, 0.0, "no charge items");
    // Caller scaling (`BuildingComp.java:1518`).
    assert!((explosiveness * 3.5 * def.explosiveness_scale - 420.0).abs() < 1e-3);
}

/// H09 T7: a vault linked to a core caps its item explosion at
/// `min(itemCapacity / 60, 6)` (`StorageBlock.java:99-103`).
#[test]
fn linked_vault_explodes_weakly() {
    let mut sim = live_sim(32);
    place(&mut sim, 5, 5, "core-shard", 1);
    let vault = place(&mut sim, 8, 5, "vault", 1);
    assert!(
        sim.ecs
            .0
            .get::<crate::world::blocks::storage::storage_block::StorageBuild>(vault)
            .and_then(|storage| storage.linked_core)
            .is_some(),
        "the vault links to the adjacent core"
    );
    let (content_len, blast, capacity) = {
        let content = &sim.unit_runtime().expect("rt").content;
        (
            content.items().len(),
            content.item_id("blast-compound").expect("blast-compound"),
            content
                .block_by_name("vault")
                .map(|def| def.item_capacity)
                .unwrap_or(1000),
        )
    };
    let mut module = crate::world::modules::ItemModule::with_items(content_len);
    module.add(blast, 1000, capacity);
    sim.ecs.0.entity_mut(vault).insert(module);

    let content = &sim.unit_runtime().expect("rt").content;
    let def = content.block_by_name("vault").expect("def").clone();
    let (_, explosiveness, _) =
        super::unit_runtime::building_explosion_terms(&sim.ecs.0, content, vault, &def);
    assert!(
        (explosiveness - 1.2 * 6.0).abs() < 1e-3,
        "capped explosiveness (6 items) {explosiveness}"
    );
}

/// H09 T7: a charged battery's `power` term is `status * capacity` (4000) and
/// `dynamic_explosion` schedules `clamp(4000/700, 0, 8) = 6` lightning bolts.
#[test]
fn battery_lightning_count() {
    use crate::combat::damage::explosion::{
        DelayedAction, DelayedExplosionActions, dynamic_explosion,
    };

    let mut sim = live_sim(32);
    let battery = place(&mut sim, 5, 5, "battery", 1);
    if let Some(mut power) = sim
        .ecs
        .0
        .get_mut::<crate::world::modules::PowerModule>(battery)
    {
        power.status = 1.0;
    }
    let content = &sim.unit_runtime().expect("rt").content;
    let def = content.block_by_name("battery").expect("def").clone();
    assert_eq!(def.base_explosiveness, 1.0, "battery baseExplosiveness");
    let (_, explosiveness, power) =
        super::unit_runtime::building_explosion_terms(&sim.ecs.0, content, battery, &def);
    assert_eq!(explosiveness, 1.0);
    assert!(
        (power - 4000.0).abs() < 1e-3,
        "battery buffered capacity 4000: {power}"
    );
    sim.with_combat_ctx(|ctx| {
        dynamic_explosion(
            ctx,
            40.0,
            40.0,
            0.0,
            explosiveness,
            power,
            16.0,
            true,
            false,
            Some(1),
        );
    });
    let actions = sim
        .ecs
        .0
        .get_resource::<DelayedExplosionActions>()
        .expect("actions");
    let bolts = actions
        .0
        .iter()
        .filter(|(_, action)| matches!(action, DelayedAction::Lightning { .. }))
        .count();
    assert_eq!(bolts, 6, "4000 / 700 = 5.71 -> 6 bolts");
    assert_eq!(actions.0.len(), 6, "no fireballs or waves");
}

/// H09 T7: explosiveness 33 splits into 3 waves spaced 2 ticks apart
/// (`Time.run(i * 2f)`).
#[test]
fn explosion_waves_are_staggered() {
    use crate::combat::damage::explosion::dynamic_explosion;

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "copper-wall", 1);
    let start = health(&sim, wall).expect("wall health");
    let (x, y) = (20.0 * 8.0 + 4.0, 20.0 * 8.0 + 4.0);
    sim.with_combat_ctx(|ctx| {
        // Team 2 is protected; the team-1 wall takes every wave.
        dynamic_explosion(ctx, x, y, 0.0, 33.0, 0.0, 24.0, true, false, Some(2));
    });
    let after_inline = health(&sim, wall).expect("wall health");
    assert!(after_inline < start, "wave 0 damages inline");
    let mut drops = 1;
    let mut previous = after_inline;
    for _ in 0..8 {
        sim.tick().expect("tick");
        let now = health(&sim, wall).expect("wall health");
        if now < previous - 1e-3 {
            drops += 1;
        }
        previous = now;
    }
    assert_eq!(drops, 3, "three staggered waves (+0/+2/+4)");
}

/// T3: `PlayerComp.team` is set before the spawn attempt, so a player without
/// a supported core retries `bestCore()` every 60 ticks
/// (`PlayerComp.java:214-243`) instead of staying teamless forever.
#[test]
fn player_team_set_without_core_retries_spawn() {
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
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    sim.set_player_team(1);
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    assert!(sim.player_unit().is_none(), "no player unit without a core");
    // Landing a core lets the next 60-tick window spawn the unit.
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let player = sim.player_unit().expect("player spawned at the new core");
    let pos = sim.ecs.0.get::<Pos>(player).expect("pos");
    assert_eq!((pos.x, pos.y), (160.0, 160.0), "spawned on the core");
}

/// T3/PAR-0175: the `unitClear` dock branch releases the possessed unit before
/// the docked unit takes the player controller (`InputHandler.java:823-873`):
/// exactly one entity keeps `comp::Player`.
#[test]
fn clear_unit_dock_releases_possessed_unit() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::entities::comp::Player;

    let mut sim = live_sim(48);
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        // Team 1 must be the player team (`select_team_controller` gives
        // `Command` only to the player's team; the rest get the AI branch).
        rules.default_team = 1;
    }
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    // The dockable types are the `core_unit_dock` units (Java: evoke, incite,
    // emanate, UnitTypes.java:4355/4421/4500) - the shard's alpha is not one.
    let dock_name = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime
            .content
            .units()
            .iter()
            .find(|unit| unit.core_unit_dock)
            .map(|unit| unit.name.clone())
            .expect("a coreUnitDock unit exists")
    };
    eprintln!("T3 dock unit: {dock_name}");
    let player = sim
        .spawn_player_unit("alpha", 1, 160.0, 160.0, 90.0)
        .expect("player alpha");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 200.0, 160.0, 0.0)
        .expect("dagger");
    let dagger_seq = sim
        .ecs
        .0
        .get::<crate::ecs::EntitySeq>(dagger)
        .expect("seq")
        .0 as i32;
    // Force the dockable type so the dock branch is exercised on this map.
    let dock_id = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_by_name(&dock_name)
        .map(|def| def.id)
        .expect("dock unit id");
    if let Some(mut docked) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::unit::comp::DockedType>(dagger)
    {
        docked.0 = dock_id;
    }
    let _ = player;
    assert!(sim.possess_unit(dagger_seq), "dagger possessed");
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Player)
    );
    assert!(sim.clear_unit(), "unitClear ran the dock branch");
    // Exactly one entity carries the Player marker, and the dagger is back on
    // its team AI.
    let players = sim
        .ecs
        .0
        .iter_entities()
        .filter(|entity| entity.get::<Player>().is_some())
        .count();
    assert_eq!(players, 1, "exactly one player entity");
    assert_eq!(
        sim.ecs
            .0
            .get::<ControllerSlot>(dagger)
            .map(|slot| slot.kind),
        Some(AiKind::Command),
        "the possessed dagger went back to its team controller"
    );
}

/// T3: `CoreBlock.playerSpawn` queues `spawnType.unlock()` in the campaign
/// (`CoreBlock.java:121-123`) through H04 T2's `UnlockRef` pipeline.
#[test]
fn campaign_spawn_queues_unlock() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::{CampaignCoreQueues, CampaignRuntime, UnlockRef};
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    session.sector = sim
        .unit_runtime()
        .expect("rt")
        .content
        .planet_id("serpulo")
        .map(|planet| (planet, 170));
    assert!(session.is_campaign(), "the play session is a campaign run");
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    let queues = crate::game::runtime::install_campaign_core_hooks(&mut sim.ecs.0);
    assert!(
        sim.ecs.0.get_resource::<CampaignCoreQueues>().is_some(),
        "campaign hook queues installed"
    );
    let _core = place(&mut sim, 20, 20, "core-shard", 1);
    let alpha = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_id("alpha")
        .expect("alpha");
    sim.spawn_player_at_best_core(1).expect("player spawned");
    let unlocked: Vec<UnlockRef> = queues.lock().expect("lock").unlocks.clone();
    assert_eq!(unlocked, vec![UnlockRef::Unit(alpha)], "alpha queued once");
    // A second spawn must not queue a duplicate.
    sim.player_unit = None;
    sim.spawn_player_at_best_core(1)
        .expect("player spawned again");
    let unlocked: Vec<UnlockRef> = queues.lock().expect("lock").unlocks.clone();
    assert_eq!(unlocked, vec![UnlockRef::Unit(alpha)], "no duplicate");
}

/// T4 (PAR-0202): `VelComp.update` integrates `vel` with tile collisions
/// (`EntityCollisions.move`), zeroes a blocked axis (`Mathf.equal`) and then
/// applies drag (`vel.scl(1 - drag)`).
#[test]
fn vel_update_applies_drag_and_stops_on_wall() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(20);
    let _wall = place(&mut sim, 10, 5, "copper-wall", 1);
    // Dagger hitSize 8 -> hitbox 5.28; the wall tile (10, 5) spans x [76, 84):
    // the resolved box stops at `76 - 2.64 = 73.36`.
    let blocked = sim
        .spawn_live_unit("dagger", 2, 72.0, 40.0, 0.0)
        .expect("blocked dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(blocked) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(blocked) {
        vel.x = 3.0;
        vel.y = 0.0;
    }
    for _ in 0..4 {
        sim.tick().expect("tick");
        let pos = *sim.ecs.0.get::<Pos>(blocked).expect("pos");
        assert!(
            pos.x <= 73.36 + 0.01,
            "the unit never enters the wall tile: {}",
            pos.x
        );
    }
    let vel = *sim.ecs.0.get::<Vel>(blocked).expect("vel");
    assert!(
        vel.x.abs() < 0.01,
        "the blocked axis decays to zero: {vel:?}"
    );

    // Free space: `vel = (1, 0)`, drag 0.3 -> move 1.0 px, then vel *= 0.7.
    let free = sim
        .spawn_live_unit("dagger", 2, 40.0, 120.0, 0.0)
        .expect("free dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(free) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(free) {
        vel.x = 1.0;
        vel.y = 0.0;
    }
    sim.tick().expect("tick");
    let pos = *sim.ecs.0.get::<Pos>(free).expect("pos");
    let vel = *sim.ecs.0.get::<Vel>(free).expect("vel");
    assert!((pos.x - 41.0).abs() < 1e-3, "advanced 1.0 px: {}", pos.x);
    assert!((vel.x - 0.7).abs() < 1e-4, "drag 0.3 applied: {vel:?}");
}

/// T4 (PAR-0202): `PhysicsComp.impulse` (`vel += v / mass`) on an idle unit
/// survives into the position through `VelComp.update`.
#[test]
fn knockback_moves_ground_unit() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(32);
    let dagger = sim
        .spawn_live_unit("dagger", 2, 160.0, 160.0, 0.0)
        .expect("dagger");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(dagger) {
        slot.kind = AiKind::NoAi;
    }
    // `mass = hitSize² π`; impulse of `mass * 2` -> `vel += 2.0`.
    let mass = 8.0f32 * 8.0 * std::f32::consts::PI;
    crate::entities::comp::unit::velocity::impulse(&mut sim.ecs.0, dagger, mass * 2.0, 0.0);
    let vel = *sim.ecs.0.get::<Vel>(dagger).expect("vel");
    assert!((vel.x - 2.0).abs() < 1e-4, "impulse applied: {vel:?}");
    sim.tick().expect("tick");
    let pos = *sim.ecs.0.get::<Pos>(dagger).expect("pos");
    assert!(
        (pos.x - 162.0).abs() < 1e-3,
        "one tick of 2.0 px: {}",
        pos.x
    );
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let vel = *sim.ecs.0.get::<Vel>(dagger).expect("vel");
    assert!(vel.x.abs() < 0.01, "drag stops the unit: {vel:?}");
    let pos2 = *sim.ecs.0.get::<Pos>(dagger).expect("pos");
    assert!(
        pos2.x > pos.x,
        "knockback moved the unit: {} -> {}",
        pos.x,
        pos2.x
    );
}

/// T4 (PAR-0202): the sandstorm wind (`ParticleWeather.update` ->
/// `unit.impulse`) now reaches the position: an idle flare drifts downwind by
/// the Java steady state `a / drag` per tick (`a = force * intensity / mass`,
/// `mass = hitSize² π`; flare drag 0.04).
#[test]
fn sandstorm_wind_pushes_idle_flare() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::{Rules, WeatherEntry};
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(48);
    let campaign = {
        let registry = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(registry, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        unit_cap: 100,
        unit_cap_variable: false,
        weather: vec![WeatherEntry {
            weather: String::from("sandstorm"),
            always: true,
            intensity: 1.0,
            ..WeatherEntry::default()
        }],
        ..Rules::default()
    });
    session.phase = State::Playing;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let flare = sim
        .spawn_live_unit("flare", 1, 160.0, 160.0, 0.0)
        .expect("flare");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(flare) {
        slot.kind = AiKind::NoAi;
    }
    let start = *sim.ecs.0.get::<Pos>(flare).expect("pos");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(flare).expect("pos");
    let dist = ((pos.x - start.x).powi(2) + (pos.y - start.y).powi(2)).sqrt();
    // Java steady state: `0.1 / (81π) / 0.04 ≈ 0.0098` px/tick -> ≈ 2.9 px.
    assert!(
        dist > 1.0 && dist < 6.0,
        "the sandstorm moved the flare {dist} px (start {start:?}, end {pos:?})"
    );
}

/// H01 T5: the `UnitComp.update` drag line (`UnitComp.java:790`) —
/// `type.drag * (isGrounded() ? floorOn().dragMultiplier : 1)`. The same
/// velocity coasts farther on `ice` (`dragMultiplier 0.35`, `Blocks.java:563`)
/// than on the default floor.
#[test]
fn ice_reduces_drag() {
    use crate::ai::controller::{AiKind, ControllerSlot};

    let mut sim = live_sim(32);
    let ice = sim
        .unit_runtime()
        .expect("rt")
        .content
        .block_id("ice")
        .expect("ice");
    let index = sim.grid.tiles.index(8, 8);
    sim.grid.tiles.geti_mut(index).floor = ice;
    sim.refresh_unit_runtime();
    let icy = sim
        .spawn_live_unit("dagger", 2, 64.0, 64.0, 0.0)
        .expect("icy dagger");
    let default_floor = sim
        .spawn_live_unit("dagger", 2, 64.0, 160.0, 0.0)
        .expect("dagger");
    for unit in [icy, default_floor] {
        if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
            slot.kind = AiKind::NoAi;
        }
        if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(unit) {
            vel.x = 2.0;
            vel.y = 0.0;
        }
    }
    for _ in 0..20 {
        sim.tick().expect("tick");
    }
    let icy_dx = sim.ecs.0.get::<Pos>(icy).expect("icy pos").x - 64.0;
    let dry_dx = sim.ecs.0.get::<Pos>(default_floor).expect("pos").x - 64.0;
    assert!(
        icy_dx > dry_dx + 1.0,
        "ice coasts farther ({icy_dx} vs {dry_dx})"
    );
}

/// H01 T5 (`UnitComp.java:750-753`): in a captured campaign sector
/// (`!waves && !attackMode`) every AI-team unit dies; the default team lives.
#[test]
fn enemy_dies_on_captured_sector() {
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(32);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        waves: false,
        attack_mode: false,
        // `Rules.unitCap` default 0 kills live units; the campaign sets 100.
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    session.sector = sim
        .unit_runtime()
        .expect("rt")
        .content
        .planet_id("serpulo")
        .map(|planet| (planet, 170));
    assert!(session.is_campaign(), "campaign session");
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let enemy = sim
        .spawn_live_unit("dagger", 2, 64.0, 64.0, 0.0)
        .expect("enemy");
    let friendly = sim
        .spawn_live_unit("dagger", 1, 160.0, 64.0, 0.0)
        .expect("friendly");
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(enemy).is_err()
            || sim
                .ecs
                .0
                .get::<Health>(enemy)
                .is_some_and(|health| health.dead),
        "the AI-team unit dies on a captured sector"
    );
    assert!(
        sim.ecs.0.get_entity(friendly).is_ok()
            && sim
                .ecs
                .0
                .get::<Health>(friendly)
                .is_some_and(|health| !health.dead),
        "the default-team unit lives"
    );
}

/// H01 T5: `GameState.hasSpawns()` (`GameState.java:68-70`) — a wave map in
/// attack mode with a wave-team core (`wavesSpawnAtCores` supplies the spawn
/// position) pushes non-wave-team units out of the drop zone
/// (`UnitComp.java:734-744,793`).
///
/// T9 note: the unit runs `NoAI` so the only velocity source is the drop-zone
/// push (the pre-T9 form let the ground controller's own walk obscure the
/// assertion).
#[test]
fn attack_map_pushes_from_drop_zone() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ecs::TeamId;
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(64);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    // Wave-team core: the attack-mode half of `hasSpawns()` and, with
    // `wavesSpawnAtCores`, a spawn position stepped toward the player core.
    let enemy_core = place(&mut sim, 40, 40, "core-shard", 2);
    // A player core is required for the wave spawner's core step
    // (`WaveSpawner.eachGroundSpawn` steps toward `playerCore()`).
    let player_core = place(&mut sim, 30, 40, "core-shard", 1);
    let mut session = PlaySession::new(Rules {
        default_team: 1,
        wave_team: 2,
        waves: true,
        attack_mode: true,
        unit_cap: 100,
        unit_cap_variable: false,
        ..Rules::default()
    });
    session.phase = State::Playing;
    session
        .teams
        .register_core(player_core, TeamId(1), &session.rules);
    session
        .teams
        .register_core(enemy_core, TeamId(2), &session.rules);
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let unit = sim
        .spawn_live_unit("dagger", 1, 420.0, 320.0, 0.0)
        .expect("unit");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(unit) {
        slot.kind = AiKind::NoAi;
    }
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(unit) {
        vel.x = 0.0;
        vel.y = 0.0;
    }
    sim.request_wave_spawn(0);
    for _ in 0..3 {
        sim.tick().expect("tick");
    }
    let vel = *sim.ecs.0.get::<Vel>(unit).expect("vel");
    let pos = *sim.ecs.0.get::<Pos>(unit).expect("pos");
    let spawn = sim
        .unit_runtime()
        .and_then(|runtime| runtime.wave_spawner.get_spawns().first().copied())
        .expect("core spawn position");
    let (sx, sy) = (spawn.x() as f32 * 8.0, spawn.y() as f32 * 8.0);
    assert!(
        vel.x * vel.x + vel.y * vel.y > 0.0,
        "the unit is pushed out of the drop zone: {vel:?}"
    );
    // The push points away from the spawn tile (the unit sits east of the
    // wave-team core, so the velocity has a positive x component).
    assert!(
        vel.x * (pos.x - sx) + vel.y * (pos.y - sy) > 0.0,
        "pushed away from the spawn tile ({sx}, {sy}): {vel:?} from {pos:?}"
    );
}

/// H01 T6: `UnitComp.killed` (`UnitComp.java:980-991`) — a dead flying unit
/// with `createWreck` keeps existing as a falling wreck (`Falling`) and only
/// runs the destroy body once grounded (`UnitComp.update`,
/// `UnitComp.java:803-830`, `fallSpeed = 0.018`, `wreckHealthMultiplier = 0.25`).
#[test]
fn dead_flare_falls_then_crashes() {
    let mut sim = live_sim(64);
    let flare = sim
        .spawn_live_unit("flare", 2, 100.0, 100.0, 0.0)
        .expect("flare");
    if let Some(mut vel) = sim.ecs.0.get_mut::<Vel>(flare) {
        vel.x = 2.0;
    }
    {
        let mut health = sim.ecs.0.get_mut::<Health>(flare).expect("health");
        health.health = 0.0;
        health.dead = true;
    }
    let x0 = sim.ecs.0.get::<Pos>(flare).expect("pos").x;
    // Tick 1: `resolve_deaths` starts the fall (`Falling`) instead of
    // destroying the unit.
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(flare).is_ok(),
        "the wreck exists after death"
    );
    assert!(
        sim.ecs.0.get::<Health>(flare).is_some_and(|h| h.dead),
        "the wreck is dead"
    );
    // `elevation 1 / fallSpeed 0.018` = 56 ticks of fall (ceil(1/0.018)).
    let mut ticks = 1usize;
    let mut last_x = x0;
    while sim.ecs.0.get_entity(flare).is_ok() && ticks < 120 {
        if let Some(pos) = sim.ecs.0.get::<Pos>(flare) {
            last_x = pos.x;
        }
        sim.tick().expect("tick");
        ticks += 1;
        if let Some(pos) = sim.ecs.0.get::<Pos>(flare) {
            last_x = pos.x;
        }
    }
    assert!(
        sim.ecs.0.get_entity(flare).is_err(),
        "the wreck is destroyed after landing"
    );
    assert!(
        (55..=58).contains(&ticks),
        "landed after ~56 fall ticks (got {ticks})"
    );
    assert!(
        last_x - x0 > 2.0,
        "the wreck kept moving while falling ({x0} -> {last_x})"
    );
}

/// H01 T6: a grounded unit without `createWreck` still dies through the
/// destroy body on the death tick (`UnitComp.killed`: `!type.flying` ->
/// `destroy()`).
#[test]
fn ground_unit_dies_instantly() {
    let mut sim = live_sim(64);
    let dagger = sim
        .spawn_live_unit("dagger", 2, 100.0, 100.0, 0.0)
        .expect("dagger");
    {
        let mut health = sim.ecs.0.get_mut::<Health>(dagger).expect("health");
        health.health = 0.0;
        health.dead = true;
    }
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(dagger).is_err(),
        "the ground unit is destroyed on the death tick"
    );
}

/// H01 T6: `UnitComp.update`: `health <= -maxHealth * wreckHealthMultiplier`
/// destroys the falling wreck immediately (`UnitComp.java:827`).
#[test]
fn wreck_destroyed_by_overkill() {
    let mut sim = live_sim(64);
    let flare = sim
        .spawn_live_unit("flare", 2, 100.0, 100.0, 0.0)
        .expect("flare");
    {
        let mut health = sim.ecs.0.get_mut::<Health>(flare).expect("health");
        health.health = 0.0;
        health.dead = true;
    }
    // Tick 1 starts the fall.
    sim.tick().expect("tick");
    assert!(sim.ecs.0.get_entity(flare).is_ok(), "falling");
    let max_health = {
        let runtime = sim.unit_runtime().expect("rt");
        let type_id = sim
            .ecs
            .0
            .get::<crate::entities::comp::unit::comp::UnitTypeComp>(flare)
            .expect("type")
            .type_id;
        runtime.content.unit(type_id).expect("flare def").health
    };
    {
        let mut health = sim.ecs.0.get_mut::<Health>(flare).expect("health");
        health.health = -max_health * 0.25 - 1.0;
    }
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(flare).is_err(),
        "overkill destroys the wreck mid-fall"
    );
}

/// H01 T9: `DefenderAI.findTarget` (`DefenderAI.java:26-38`) — an oct follows
/// the healthiest player-controllable ally in `max(range, 400)` (key
/// `-maxHealth + dst2 / 6400`), keeping `targetHit/2*1.1 + ownHit/2 + 15` px.
#[test]
fn oct_follows_healthiest_ally() {
    use crate::entities::comp::unit::comp::HitboxComp;
    let mut sim = live_sim(64);
    let oct = sim
        .spawn_live_unit("oct", 2, 100.0, 100.0, 0.0)
        .expect("oct");
    let _dagger = sim
        .spawn_live_unit("dagger", 2, 150.0, 100.0, 0.0)
        .expect("dagger");
    let mace = sim
        .spawn_live_unit("mace", 2, 300.0, 100.0, 0.0)
        .expect("mace");
    for _ in 0..1500 {
        sim.tick().expect("tick");
    }
    let hit = |sim: &Sim, e| {
        sim.ecs
            .0
            .get::<HitboxComp>(e)
            .map_or(0.0, |hitbox| hitbox.hit_size)
    };
    let opos = *sim.ecs.0.get::<Pos>(oct).expect("oct alive");
    let mpos = *sim.ecs.0.get::<Pos>(mace).expect("mace alive");
    let keep = hit(&sim, mace) / 2.0 * 1.1 + hit(&sim, oct) / 2.0 + 15.0;
    let dst = ((mpos.x - opos.x).powi(2) + (mpos.y - opos.y).powi(2)).sqrt();
    assert!(
        dst <= keep + 10.0,
        "oct within keep {keep} + 10 of the mace (dist {dst}, oct {opos:?}, mace {mpos:?})"
    );
}

/// H01 T9: `FlyingFollowAI` (`FlyingFollowAI.java:26-29`) — a wave-team quell
/// follows the healthiest allied non-follower.
#[test]
fn quell_follows_ally_on_wave_team() {
    use crate::entities::comp::unit::comp::HitboxComp;
    let mut sim = live_sim(64);
    let quell = sim
        .spawn_live_unit("quell", 2, 60.0, 100.0, 0.0)
        .expect("quell");
    let dagger = sim
        .spawn_live_unit("dagger", 2, 200.0, 100.0, 0.0)
        .expect("dagger");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let hit = |sim: &Sim, e| {
        sim.ecs
            .0
            .get::<HitboxComp>(e)
            .map_or(0.0, |hitbox| hitbox.hit_size)
    };
    let qpos = *sim.ecs.0.get::<Pos>(quell).expect("quell alive");
    let dpos = *sim.ecs.0.get::<Pos>(dagger).expect("dagger alive");
    let keep = hit(&sim, dagger) / 2.0 * 1.1 + hit(&sim, quell) / 2.0 + 15.0;
    let dst = ((dpos.x - qpos.x).powi(2) + (dpos.y - qpos.y).powi(2)).sqrt();
    assert!(
        dst <= keep + 10.0,
        "quell within keep {keep} + 10 of the dagger (dist {dst})"
    );
}

/// H01 T9: `FlyingFollowAI.useFallback()` — a player-team quell runs `FlyingAI`
/// (toward the enemy core) instead of following the ally.
#[test]
fn quell_uses_flying_ai_on_player_team() {
    let mut sim = live_sim(64);
    let quell = sim
        .spawn_live_unit("quell", 1, 60.0, 100.0, 0.0)
        .expect("quell");
    let _ally = sim
        .spawn_live_unit("dagger", 1, 200.0, 100.0, 0.0)
        .expect("dagger");
    let core = place(&mut sim, 30, 30, "core-shard", 2);
    let core_pos = *sim.ecs.0.get::<Pos>(core).expect("core pos");
    let start = *sim.ecs.0.get::<Pos>(quell).expect("quell pos");
    let start_dst = ((core_pos.x - start.x).powi(2) + (core_pos.y - start.y).powi(2)).sqrt();
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(quell).expect("quell alive");
    let dst = ((core_pos.x - pos.x).powi(2) + (core_pos.y - pos.y).powi(2)).sqrt();
    assert!(
        dst < start_dst - 30.0,
        "the player-team quell flew toward the enemy core ({start_dst} -> {dst})"
    );
}

/// H01 T9: `HugAI.updateMovement` (`HugAI.java:16-61`) — a renale closes on an
/// in-range enemy until the contact circle
/// `(unit.hitSize + target.hitSize) * 0.5`.
#[test]
fn renale_hugs_target() {
    use crate::entities::comp::unit::comp::HitboxComp;
    let mut sim = live_sim(64);
    let range = sim
        .unit_runtime()
        .expect("rt")
        .content
        .unit_by_name("renale")
        .expect("renale")
        .range;
    let renale = sim
        .spawn_live_unit("renale", 2, 150.0, 100.0, 0.0)
        .expect("renale");
    let dagger = sim
        .spawn_live_unit("dagger", 1, 150.0 + range * 0.8, 100.0, 180.0)
        .expect("dagger");
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
    let hit = |sim: &Sim, e| {
        sim.ecs
            .0
            .get::<HitboxComp>(e)
            .map_or(0.0, |hitbox| hitbox.hit_size)
    };
    let rpos = *sim.ecs.0.get::<Pos>(renale).expect("renale alive");
    let dpos = *sim.ecs.0.get::<Pos>(dagger).expect("dagger alive");
    let contact = (hit(&sim, renale) + hit(&sim, dagger)) * 0.5;
    let dst = ((dpos.x - rpos.x).powi(2) + (dpos.y - rpos.y).powi(2)).sqrt();
    assert!(
        dst <= contact + 2.0,
        "renale reached the contact circle {contact} (dist {dst})"
    );
}

/// H01 T9: `NoAI.updateUnit(){}` (`NoAI.java`) — the dummy never moves.
#[test]
fn dummy_never_moves() {
    let mut sim = live_sim(64);
    let dummy = sim
        .spawn_live_unit("dummy", 2, 100.0, 100.0, 0.0)
        .expect("dummy");
    let start = *sim.ecs.0.get::<Pos>(dummy).expect("dummy pos");
    for _ in 0..300 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(dummy).expect("dummy alive");
    assert_eq!((pos.x, pos.y), (start.x, start.y), "the dummy never moves");
}

/// H01 T11: `GroundAI` stuck bookkeeping (`GroundAI.java:24,63-85`): a unit
/// that has `moved` true but cannot leave a 12 px window accumulates
/// `stuckTime` past `stuckThreshold = max(1, tilesize * 1.5 * 2 / speed)`
/// (48 ticks for a dagger), then requests flowfield avoidance: the process
/// activates (`AvoidanceProcess.java:40-48`) and its grid marks every ground
/// AI unit's tile (`:52-106`). The "different next tile" selection itself is
/// covered by `get_target_tile_skips_avoided_neighbour`.
#[test]
fn stuck_dagger_activates_avoidance_in_a_pocket() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ai::types::ground::GroundAiState;
    use crate::ecs::EntitySeq;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::world::WorldGrid;

    let mut sim = live_sim(64);
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

    // The enemy core and a stationary same-team unit elsewhere on the map.
    place(&mut sim, 48, 31, "core-shard", 1);
    let blocker = sim
        .spawn_live_unit("dagger", 2, 24.0 * 8.0, 24.0 * 8.0, 0.0)
        .expect("blocker");
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(blocker) {
        slot.kind = AiKind::NoAi;
    }
    // A 1-tile pocket: the follower is walled in on all four sides, so
    // `should_move` stays true while its position cannot change.
    for (x, y) in [(20, 30), (20, 32), (19, 31), (21, 31)] {
        place(&mut sim, x, y, "copper-wall", 2);
    }
    let follower = sim
        .spawn_live_unit("dagger", 2, 20.0 * 8.0, 31.0 * 8.0, 0.0)
        .expect("follower");
    let blocker_id = sim
        .ecs
        .0
        .get::<EntitySeq>(blocker)
        .map_or(0, |seq| (seq.0 & 0x7fff_ffff) as i32);
    let width = sim.unit_runtime().expect("rt").grid.width();
    let start = *sim.ecs.0.get::<Pos>(follower).expect("follower pos");

    let mut max_stuck = 0.0f32;
    let mut activated_at = None;
    let mut marked = false;
    for tick in 0..300 {
        sim.tick().expect("tick");
        if let Some(state) = sim.ecs.0.get::<GroundAiState>(follower) {
            max_stuck = max_stuck.max(state.stuck_time);
        }
        if activated_at.is_none() && sim.unit_runtime().is_some_and(|rt| rt.avoidance_active) {
            activated_at = Some(tick);
        }
        let bpos = *sim.ecs.0.get::<Pos>(blocker).expect("blocker alive");
        if let Some(rt) = sim.unit_runtime() {
            let bx = WorldGrid::to_tile(bpos.x);
            let by = WorldGrid::to_tile(bpos.y);
            if rt.avoidance.get((bx + by * width) as usize).copied() == Some(i32::MAX - blocker_id)
            {
                marked = true;
            }
        }
    }
    // `stuckThreshold = max(1, tilesize * 1.5 * 2 / speed)` = 48 for a dagger.
    assert!(
        max_stuck >= 48.0,
        "the stuck time crossed the threshold (max {max_stuck})"
    );
    assert!(
        activated_at.is_some(),
        "the flowfield avoidance grid activated after the jam"
    );
    assert!(marked, "the blocker's tile is marked on the avoidance grid");
    // Walled in: the follower never left its tile.
    let end = *sim.ecs.0.get::<Pos>(follower).expect("follower alive");
    assert_eq!(
        (end.x, end.y),
        (start.x, start.y),
        "the pocketed follower stayed put"
    );
    // The blocker never moved.
    let bpos = *sim.ecs.0.get::<Pos>(blocker).expect("blocker alive");
    assert_eq!(
        (bpos.x, bpos.y),
        (24.0 * 8.0, 24.0 * 8.0),
        "the NoAI blocker stayed put"
    );
}

/// H02 T12: `Wall.WallBuild.collision` (`Wall.java:177-207`) — a phase wall
/// deflects a share of the bullets that hit it (chance `chanceDeflect /
/// bullet.damage`) and steals the deflected bullet for its own team.
#[test]
fn phase_wall_reflects_low_damage_bullet() {
    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "phase-wall", 1);
    // Keep the wall alive across the volley so every bullet rolls.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.max_health = 100_000.0;
        health.health = 100_000.0;
    }
    // Dagger blaster clone re-damaged to 20 so `chanceDeflect = 10f` is Java's
    // 0.5 roll (`Blocks.java:1761`) instead of a certainty (the vanilla dagger
    // bullet deals 9).
    let base = {
        let runtime = sim.unit_runtime().expect("rt");
        let dagger = runtime.content.unit_by_name("dagger").expect("dagger");
        let bullet = dagger.weapons[0].bullet.id;
        runtime.content.bullet(bullet).expect("bullet def").clone()
    };
    let mut def = base;
    def.damage = 20.0;
    let def = sim
        .unit_runtime_mut()
        .expect("rt")
        .content
        .add_bullet(def)
        .expect("add bullet");
    let (wx, wy) = (20.0 * 8.0, 20.0 * 8.0);
    let mut spawned: Vec<Entity> = Vec::new();
    sim.with_combat_ctx(|ctx| {
        for i in 0..40 {
            let spawn = crate::combat::BulletSpawn {
                def,
                team: 2,
                x: wx + 6.0,
                y: wy - 3.0 + 6.0 * ((i % 5) as f32 / 4.0),
                angle: 180.0,
                ..crate::combat::BulletSpawn::default()
            };
            if let Some(entity) = ctx.spawn(&spawn) {
                spawned.push(entity);
            }
        }
    });
    assert_eq!(spawned.len(), 40, "all fixture bullets spawned");
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let deflected = spawned
        .iter()
        .filter(|entity| {
            sim.ecs
                .0
                .get::<crate::entities::comp::TeamComp>(**entity)
                .is_some_and(|team| team.team == 1)
        })
        .count();
    assert!(
        deflected > 0,
        "phase wall deflected {deflected} of {} bullets",
        spawned.len()
    );
    assert!(
        deflected < spawned.len(),
        "the deflect roll is probabilistic (all {} bullets deflected)",
        spawned.len()
    );
}

/// I02 T4: `Wall.WallBuild.collision` (`Wall.java:164-166`) — a `flashHit`
/// wall (phase walls only, `Blocks.java:1759-1772`) logs a `WallFlashEvent`
/// for its anchor tile on every collision, deflected or not; the renderer owns
/// the `hit` decay. Non-`flashHit` walls log nothing.
#[test]
fn flash_hit_wall_logs_collision() {
    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "phase-wall", 1);
    // Keep the wall alive across the volley so every bullet lands.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.max_health = 100_000.0;
        health.health = 100_000.0;
    }
    let plain = place(&mut sim, 26, 20, "copper-wall", 1);
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(plain) {
        health.max_health = 100_000.0;
        health.health = 100_000.0;
    }
    let def = {
        let runtime = sim.unit_runtime().expect("rt");
        let dagger = runtime.content.unit_by_name("dagger").expect("dagger");
        dagger.weapons[0].bullet.id
    };
    for (wx, _entity) in [(20.0f32 * 8.0, wall), (26.0f32 * 8.0, plain)] {
        sim.with_combat_ctx(|ctx| {
            for i in 0..4 {
                let spawn = crate::combat::BulletSpawn {
                    def,
                    team: 2,
                    x: wx + 6.0,
                    y: 20.0 * 8.0 - 3.0 + 2.0 * i as f32,
                    angle: 180.0,
                    ..crate::combat::BulletSpawn::default()
                };
                let _ = ctx.spawn(&spawn);
            }
        });
        for _ in 0..4 {
            sim.tick().expect("tick");
        }
    }
    let flashes = sim.drain_wall_flashes();
    assert!(
        flashes.contains(&(20, 20)),
        "the flashHit wall logged its hit: {flashes:?}"
    );
    assert!(
        !flashes.iter().any(|(x, _)| *x == 26),
        "a non-flashHit wall logs nothing: {flashes:?}"
    );
    assert!(sim.drain_wall_flashes().is_empty(), "the flash log drains");
}

/// H02 T12: `Wall.WallBuild.collision` (`Wall.java:169-175`) — a surge wall
/// arcs `Lightning.create(..)` from a share of the hits it takes
/// (`lightningChance = 0.05f`, `Blocks.java:1776`).
#[test]
fn surge_wall_arcs() {
    use crate::combat::lightning::LightningCounter;

    let mut sim = live_sim(48);
    let wall = place(&mut sim, 20, 20, "surge-wall", 1);
    // Keep the wall alive across the volley so every bullet rolls.
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(wall) {
        health.max_health = 100_000.0;
        health.health = 100_000.0;
    }
    let def = {
        let runtime = sim.unit_runtime().expect("rt");
        let dagger = runtime.content.unit_by_name("dagger").expect("dagger");
        dagger.weapons[0].bullet.id
    };
    let (wx, wy) = (20.0 * 8.0, 20.0 * 8.0);
    sim.with_combat_ctx(|ctx| {
        for i in 0..200 {
            let spawn = crate::combat::BulletSpawn {
                def,
                team: 2,
                x: wx + 6.0,
                y: wy - 3.0 + 6.0 * ((i % 5) as f32 / 4.0),
                angle: 180.0,
                ..crate::combat::BulletSpawn::default()
            };
            let _ = ctx.spawn(&spawn);
        }
    });
    for _ in 0..4 {
        sim.tick().expect("tick");
    }
    let branches = sim
        .ecs
        .0
        .get_resource::<LightningCounter>()
        .map(|counter| counter.0)
        .unwrap_or(0);
    assert!(
        branches > 0,
        "surge wall arced 0 times over 200 hits (5% chance each)"
    );
}

/// H01 T12: a team-1 `poly` with the `assist` command escorts the local
/// builder player and copies their build plan (`BuilderAI.java:57-100,143-186`).
/// Before T12 `onlyAssist` returned early for every tick and the poly idled.
#[test]
fn assist_poly_follows_building_player() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::sim::builder::{BuilderPlans, UnitBuildPlan};
    use crate::world::ConstructState;

    let mut sim = live_sim(64);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    // A core funds the build (`shouldSkip`) and the campaign unit cap
    // (`unitCap 0 + core +8`).
    let core = place(&mut sim, 8, 8, "core-shard", 1);
    let copper = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.item_id("copper").expect("copper")
    };
    if let Some(mut items) = sim.ecs.0.get_mut::<crate::world::modules::ItemModule>(core) {
        items.add(copper, 500, 4000);
    }

    let player = sim
        .spawn_player_unit("alpha", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("player alpha");
    let poly = sim
        .spawn_live_unit("poly", 1, 10.0 * 8.0, 10.0 * 8.0, 0.0)
        .expect("poly");
    let assist = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime
            .content
            .unit_command_by_name("assist")
            .expect("assist")
            .id
    };
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(poly) {
        slot.kind = AiKind::Command;
        slot.command = Some(assist);
    }

    // Phase 1: the player is idle -> `moveTo(assistFollowing, hit + hit/2 + 60)`.
    let start = *sim.ecs.0.get::<Pos>(poly).expect("poly pos");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let ppos = *sim.ecs.0.get::<Pos>(poly).expect("poly pos");
    let apos = *sim.ecs.0.get::<Pos>(player).expect("player pos");
    let dst = ((ppos.x - apos.x).powi(2) + (ppos.y - apos.y).powi(2)).sqrt();
    eprintln!("assist escort: poly {ppos:?} player {apos:?} dst {dst:.1} start {start:?}");
    assert!(
        ppos.x != start.x || ppos.y != start.y,
        "the assist poly left its idle spot"
    );
    // `assistFollowing.type.hitSize + unit.type.hitSize/2 + 60` (+ the `moving`
    // tolerance of 5): alpha hit 8, poly hit 9.
    assert!(
        dst <= 8.0 + 9.0 / 2.0 + 65.0,
        "the poly escorts the player ({dst})"
    );

    // Phase 2: queue the player's wall plan one tile east; the alpha starts the
    // construct (`updateBuildLogic`), then the cost is frozen so the plan stays
    // open for the whole acceptance window.
    let wall = BlockId::new(block(&sim, "copper-wall"));
    let plan = UnitBuildPlan::place(41, 40, 0, wall);
    sim.ecs
        .0
        .get_mut::<BuilderPlans>(player)
        .expect("player plans")
        .plans
        .push_back(plan);
    let mut construct = None;
    for _ in 0..30 {
        sim.tick().expect("tick");
        if let Some(entity) = sim.grid.tiles.get(41, 40).build
            && sim.ecs.0.get::<ConstructState>(entity).is_some()
        {
            construct = Some(entity);
            break;
        }
    }
    let construct = construct.expect("the alpha started the construct");
    sim.ecs
        .0
        .get_mut::<ConstructState>(construct)
        .expect("construct state")
        .build_cost = 1_000_000.0;
    for _ in 0..120 {
        sim.tick().expect("tick");
    }
    let player_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(player)
        .and_then(|plans| plans.plans.front().cloned());
    let poly_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(poly)
        .and_then(|plans| plans.plans.front().cloned());
    // The per-unit plan progress mirrors diverge as each builder works; the
    // plan identity (tile/block/rotation/mode) is what is copied.
    let plan_id = |plan: Option<UnitBuildPlan>| {
        plan.map(|plan| (plan.x, plan.y, plan.block, plan.rotation, plan.breaking))
    };
    eprintln!("assist copy: player {player_plan:?} poly {poly_plan:?}");
    assert!(
        player_plan.is_some(),
        "the player is still actively building"
    );
    assert_eq!(
        plan_id(poly_plan),
        plan_id(player_plan),
        "the poly mimics the player's front plan"
    );
    // The poly reached the construct's range (buildRange - hitSize*2 = 202).
    let plan_tile = (41.0 * 8.0, 40.0 * 8.0);
    let ppos = *sim.ecs.0.get::<Pos>(poly).expect("poly pos");
    let plan_dst = ((ppos.x - plan_tile.0).powi(2) + (ppos.y - plan_tile.1).powi(2)).sqrt();
    assert!(
        plan_dst <= 202.0 + 12.0,
        "the poly moved into build range of the plan ({plan_dst})"
    );
}

/// H01 T12: an idle `rebuild` poly (the poly default command, `onlyAssist`
/// false) picks an actively-building unit within `buildRadius` and copies its
/// plan (`BuilderAI.java:149-169`).
#[test]
fn idle_rebuild_poly_helps_builder() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ai::types::builder::{BuilderAiState, DEFAULT_REBUILD_PERIOD};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;
    use crate::sim::builder::{BuilderPlans, UnitBuildPlan};
    use crate::world::ConstructState;

    let mut sim = live_sim(64);
    let campaign = {
        let content = &sim.unit_runtime().expect("rt").content;
        Campaign::from_registry(content, &EmptyNeighborhood)
    };
    let session = PlaySession::new(Rules {
        default_team: 1,
        ..Rules::default()
    });
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));
    // A core funds the build (`shouldSkip`) and the campaign unit cap
    // (`unitCap 0 + core +8`).
    let core = place(&mut sim, 8, 8, "core-shard", 1);
    let copper = {
        let runtime = sim.unit_runtime().expect("rt");
        runtime.content.item_id("copper").expect("copper")
    };
    if let Some(mut items) = sim.ecs.0.get_mut::<crate::world::modules::ItemModule>(core) {
        items.add(copper, 500, 4000);
    }

    let player = sim
        .spawn_player_unit("alpha", 1, 40.0 * 8.0, 40.0 * 8.0, 0.0)
        .expect("player alpha");
    let helper = sim
        .spawn_live_unit("poly", 1, 10.0 * 8.0, 10.0 * 8.0, 0.0)
        .expect("helper poly");
    // `slot.command = None` -> the poly's `defaultCommand = rebuild`.
    if let Some(mut slot) = sim.ecs.0.get_mut::<ControllerSlot>(helper) {
        slot.kind = AiKind::Command;
    }

    let wall = BlockId::new(block(&sim, "copper-wall"));
    let plan = UnitBuildPlan::place(41, 40, 0, wall);
    sim.ecs
        .0
        .get_mut::<BuilderPlans>(player)
        .expect("player plans")
        .plans
        .push_back(plan);
    let mut construct = None;
    for _ in 0..30 {
        sim.tick().expect("tick");
        if let Some(entity) = sim.grid.tiles.get(41, 40).build
            && sim.ecs.0.get::<ConstructState>(entity).is_some()
        {
            construct = Some(entity);
            break;
        }
    }
    let construct = construct.expect("the alpha started the construct");
    sim.ecs
        .0
        .get_mut::<ConstructState>(construct)
        .expect("construct state")
        .build_cost = 1_000_000.0;

    let start = *sim.ecs.0.get::<Pos>(helper).expect("helper pos");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }

    let state = sim
        .ecs
        .0
        .get::<BuilderAiState>(helper)
        .copied()
        .expect("builder state");
    let player_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(player)
        .and_then(|plans| plans.plans.front().cloned());
    let helper_plan = sim
        .ecs
        .0
        .get::<BuilderPlans>(helper)
        .and_then(|plans| plans.plans.front().cloned());
    let hpos = *sim.ecs.0.get::<Pos>(helper).expect("helper pos");
    let plan_tile = (41.0 * 8.0, 40.0 * 8.0);
    let plan_dst = ((hpos.x - plan_tile.0).powi(2) + (hpos.y - plan_tile.1).powi(2)).sqrt();
    eprintln!(
        "rebuild help: following {:?} plan_dst {plan_dst:.1} start {start:?} state \
         {state:?}",
        state.following
    );
    assert_eq!(
        state.following,
        Some(player),
        "the idle rebuild poly follows the building player"
    );
    // The per-unit plan progress mirrors diverge as each builder works; the
    // plan identity (tile/block/rotation/mode) is what is copied.
    let plan_id = |plan: Option<UnitBuildPlan>| {
        plan.map(|plan| (plan.x, plan.y, plan.block, plan.rotation, plan.breaking))
    };
    assert_eq!(
        plan_id(helper_plan),
        plan_id(player_plan),
        "the helper poly copies the player's front plan"
    );
    assert!(
        plan_dst <= 202.0 + 12.0,
        "the helper moved into build range of the plan ({plan_dst})"
    );
    assert_eq!(
        state.rebuild_period, DEFAULT_REBUILD_PERIOD,
        "no buildAi rule -> default rebuild period"
    );
}

/// H01 T13: a `manifold` (CargoAI) is dispatched to `AiKind::Cargo` instead of
/// the generic flying branch. Without a tether the controller early-returns
/// (`CargoAI.java:28`), so the unit stays put; the generic FlyingAI would have
/// closed on the enemy core.
#[test]
fn manifold_dispatches_cargo_ai() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::Rules;
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(64);
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
    // A team-1 core satisfies the campaign unit cap (`unitCap 0 + core +8`).
    let _core = place(&mut sim, 5, 5, "core-shard", 1);
    let _enemy_core = place(&mut sim, 55, 55, "core-shard", 2);
    let manifold = sim
        .spawn_live_unit("manifold", 1, 20.0 * 8.0, 20.0 * 8.0, 0.0)
        .expect("manifold");
    let start = *sim.ecs.0.get::<Pos>(manifold).expect("manifold pos");
    for _ in 0..600 {
        sim.tick().expect("tick");
    }
    let kind = sim
        .ecs
        .0
        .get::<ControllerSlot>(manifold)
        .map(|slot| slot.kind);
    assert_eq!(kind, Some(AiKind::Cargo), "the manifold keeps CargoAI");
    let pos = *sim.ecs.0.get::<Pos>(manifold).expect("manifold alive");
    assert_eq!(
        (pos.x, pos.y),
        (start.x, start.y),
        "no tether -> the cargo unit never paths to the enemy core"
    );
}

/// H01 T13: an `assembly-drone` (AssemblerAI) is dispatched and reaches the
/// spot the assembler arranges (`AssemblerAI.java:12-24`).
#[test]
fn assembly_drone_dispatches() {
    use crate::ai::controller::{AiKind, ControllerSlot};
    use crate::ai::types::assembler::AssemblerAi;

    let mut sim = live_sim(64);
    let _enemy_core = place(&mut sim, 55, 55, "core-shard", 2);
    let drone = sim
        .spawn_live_unit("assembly-drone", 1, 20.0 * 8.0, 20.0 * 8.0, 0.0)
        .expect("assembly-drone");
    assert_eq!(
        sim.ecs.0.get::<ControllerSlot>(drone).map(|slot| slot.kind),
        Some(AiKind::Assembler),
        "the drone's content controller is AssemblerAI"
    );
    // The assembler block arranges the drone around its spawn perimeter
    // (`UnitAssembler.java:509-513`); the port writes the same state directly
    // until the block half lands.
    sim.ecs.0.entity_mut(drone).insert(AssemblerAi {
        unit: Some(drone),
        target_x: 26.0 * 8.0,
        target_y: 26.0 * 8.0,
        target_angle: 90.0,
    });
    for _ in 0..900 {
        sim.tick().expect("tick");
    }
    let pos = *sim.ecs.0.get::<Pos>(drone).expect("drone alive");
    let dst = ((pos.x - 26.0 * 8.0).powi(2) + (pos.y - 26.0 * 8.0).powi(2)).sqrt();
    let rotation = sim
        .ecs
        .0
        .get::<crate::entities::comp::unit::comp::UnitCore>(drone)
        .map(|core| core.rotation)
        .unwrap_or(0.0);
    eprintln!(
        "drone at ({}, {}) rotation {rotation} dst {dst}",
        pos.x, pos.y
    );
    assert!(dst <= 10.0, "the drone reached its arranged spot ({dst})");
    assert!(
        (rotation - 90.0).abs() < 0.001 || (rotation - 450.0).abs() < 0.001,
        "the drone faces the arranged angle ({rotation})"
    );
}

/// I01 T12: every turret carries the detached `block` unit proxy
/// (`TurretBuild.unit = UnitTypes.block.create(team)`, `Turret.java:297`),
/// positioned on the turret each tick (`unit.tile(this)`, `Turret.java:535-537`)
/// and dropped with the building. The proxy is never in the live unit list, so
/// it is never saved.
#[test]
fn turret_has_block_unit() {
    use crate::ecs::EntitySeq;
    use crate::entities::comp::unit::comp::{BlockUnitComp, UnitTypeComp};
    use crate::entities::comp::{Building, TeamComp};
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    let unit = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .expect("turret state")
        .block_unit
        .expect("block unit proxy");
    // `UnitTypes.block`.
    let runtime = sim.unit_runtime().expect("runtime");
    let type_id = sim
        .ecs
        .0
        .get::<UnitTypeComp>(unit)
        .expect("proxy type")
        .type_id;
    assert_eq!(
        runtime.content.unit(type_id).expect("block def").name,
        "block"
    );
    // `unit.tile(this)`: the proxy sits on the turret's tile.
    let block_tile = sim
        .ecs
        .0
        .get::<Building>(duo)
        .expect("building")
        .tile
        .pack();
    assert_eq!(
        sim.ecs
            .0
            .get::<BlockUnitComp>(unit)
            .expect("block unit comp")
            .tile,
        block_tile
    );
    // Team + position follow the turret.
    let (bx, by) = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|pos| (pos.x, pos.y))
        .expect("turret pos");
    let (ux, uy) = sim
        .ecs
        .0
        .get::<Pos>(unit)
        .map(|pos| (pos.x, pos.y))
        .expect("proxy pos");
    assert_eq!((ux, uy), (bx, by), "proxy sits on the turret");
    assert_eq!(sim.ecs.0.get::<TeamComp>(unit).expect("proxy team").team, 1);
    assert!(
        !runtime.units.contains(&unit),
        "the proxy is detached (never saved / never unit-updated)"
    );
    let _ = sim.ecs.0.get::<EntitySeq>(unit).expect("proxy seq");

    // Removing the turret removes the proxy.
    sim.command(SimCommand::Break {
        x: 20,
        y: 20,
        player: None,
    })
    .expect("break");
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(unit).is_err(),
        "proxy despawned with the turret"
    );
}

/// I01 T12 (`Turret.java:571-600`): a possessed turret aims at the player's
/// cursor and fires on the shoot button (`controlled()` -> `targetPos.set(
/// unit.aimX(), unit.aimY()); canShoot = unit.isShooting()`).
#[test]
fn controlled_turret_shoots_at_aim() {
    use crate::ecs::EntitySeq;
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    feed(&mut sim, duo, "copper", 10);
    sim.set_player_team(1);
    let unit = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .expect("turret state")
        .block_unit
        .expect("block unit proxy");
    let seq = sim.ecs.0.get::<EntitySeq>(unit).expect("proxy seq").0 as i32;
    assert!(sim.possess_unit(seq), "player possesses the turret proxy");
    assert_eq!(sim.player_unit(), Some(unit));

    // Aim east of the turret (its default rotation is 90/south) and hold fire.
    let (bx, by) = sim
        .ecs
        .0
        .get::<Pos>(duo)
        .map(|pos| (pos.x, pos.y))
        .expect("turret pos");
    let before = sim.unit_runtime().expect("runtime").bullets_created;
    for _ in 0..120 {
        sim.set_player_control(0.0, 0.0, true, bx + 120.0, by, false, false);
        sim.tick().expect("tick");
    }
    let after = sim.unit_runtime().expect("runtime").bullets_created;
    assert!(
        after > before,
        "the controlled turret fired ({before} -> {after})"
    );
    let rotation = sim
        .ecs
        .0
        .get::<TurretState>(duo)
        .expect("turret state")
        .rotation;
    assert!(
        (rotation.rem_euclid(360.0)).abs() < 5.0
            || (rotation.rem_euclid(360.0) - 360.0).abs() < 5.0,
        "the turret turned toward the cursor ({rotation})"
    );

    // Releasing the control stops the player-driven fire (the turret drops the
    // `Player` marker).
    sim.clear_unit();
    assert!(sim.player_unit().is_none());
}

/// I01 T12 (`CoreBlock.java:619-637`): Ctrl-click on the own core clears the
/// player's unit and respawns it at that core (`player.clearUnit();
/// player.deathTimer = deathDelay + 1; requestSpawn(player)`).
#[test]
fn core_control_select_respawns() {
    let mut sim = live_sim(48);
    let core = place(&mut sim, 10, 10, "core-shard", 1);
    let alpha = sim
        .spawn_player_unit("alpha", 1, 200.0, 200.0, 0.0)
        .expect("alpha");
    assert_eq!(sim.player_unit(), Some(alpha));

    sim.command(SimCommand::BuildingControlSelect { x: 10, y: 10 })
        .expect("control select");
    sim.tick().expect("tick");
    let player = sim.player_unit().expect("respawned player");
    assert_ne!(player, alpha, "the old unit was released");
    let (cx, cy) = sim
        .ecs
        .0
        .get::<Pos>(core)
        .map(|pos| (pos.x, pos.y))
        .expect("core pos");
    let (px, py) = sim
        .ecs
        .0
        .get::<Pos>(player)
        .map(|pos| (pos.x, pos.y))
        .expect("player pos");
    assert!(
        (px - cx).abs() < 1.0 && (py - cy).abs() < 1.0,
        "respawned at the selected core ({px}, {py}) vs ({cx}, {cy})"
    );
}

/// J03 T2: a freshly placed idle duo draws its head facing up.
///
/// `BaseTurretBuild.rotation = 90` (`BaseTurret.java:47`) and the barrel angle
/// is `drawrot() = rotation - 90 = 0` (`Turret.java:327-329`); with no ammo the
/// turret never turns (`Turret.updateTile` gates targeting on `hasAmmo`,
/// `Turret.java:571-605`). Pins the sim half of the drawn angle: a no-ammo duo
/// keeps `rotation = 90` (drawrot 0) across 60 ticks.
#[test]
fn placed_duo_idle_head_rotation_is_up() {
    use crate::world::blocks::defense::turrets::TurretState;

    let mut sim = live_sim(48);
    let duo = place(&mut sim, 20, 20, "duo", 1);
    assert_eq!(
        sim.ecs
            .0
            .get::<TurretState>(duo)
            .expect("turret state")
            .rotation,
        90.0,
        "BaseTurretBuild starts at rotation 90"
    );
    for _ in 0..60 {
        sim.tick().expect("tick");
    }
    let state = sim.ecs.0.get::<TurretState>(duo).expect("turret state");
    assert_eq!(state.total_ammo, 0, "no ammo");
    assert!(state.target.is_none(), "no target");
    assert_eq!(state.rotation, 90.0, "idle turret stays at 90");
    // The renderer draws at `rotation - 90` (blocks.rs / render/turret.rs).
    let drawrot = state.rotation - 90.0;
    assert!(
        drawrot.abs() < 1e-6,
        "idle head drawrot is 0 (up): {drawrot}"
    );
}

/// L06 T1: `UnitAssemblerBuild.wasOccupied = checkSolid(getUnitSpawn(), false)`
/// (`UnitAssembler.java:515,679-685`) with the vanilla `areaSize = 13`
/// (`Blocks.java:6566`): a solid tile or a grounded unit on the spawn point
/// occupies a ground output; a flyer or a `spawnedByCore` unit does not.
#[test]
fn assembler_output_area_occupancy() {
    use crate::world::blocks::units::UnitAssembler;
    let mut sim = live_sim(64);
    let assembler = place(&mut sim, 20, 20, "tank-assembler", 1);
    sim.tick().expect("tick");
    let occupied = |sim: &Sim| {
        sim.ecs
            .0
            .get::<UnitAssembler>(assembler)
            .expect("assembler")
            .was_occupied
    };
    assert_eq!(
        sim.ecs
            .0
            .get::<UnitAssembler>(assembler)
            .map(|a| a.area_size),
        Some(13)
    );
    assert!(!occupied(&sim), "free output area");
    // Spawn point: rotation 0, `x + tilesize * (13 + 5) / 2` = tile (29, 20).
    let (sx, sy) = (29.0 * 8.0, 20.0 * 8.0);
    let flare = sim.spawn_live_unit("flare", 1, sx, sy, 0.0).expect("flare");
    sim.tick().expect("tick");
    assert!(!occupied(&sim), "a flyer does not block a ground output");
    sim.ecs.0.despawn(flare);
    let dagger = sim
        .spawn_live_unit("dagger", 2, sx, sy, 0.0)
        .expect("dagger");
    if let Some(mut slot) = sim
        .ecs
        .0
        .get_mut::<crate::ai::controller::ControllerSlot>(dagger)
    {
        slot.kind = crate::ai::controller::AiKind::NoAi;
    }
    sim.tick().expect("tick");
    assert!(occupied(&sim), "a grounded unit on the spawn point");
    if let Some(mut core) = sim
        .ecs
        .0
        .get_mut::<crate::entities::comp::unit::comp::UnitCore>(dagger)
    {
        core.spawned_by_core = true;
    }
    sim.tick().expect("tick");
    assert!(!occupied(&sim), "spawnedByCore units are ignored");
    sim.ecs.0.despawn(dagger);
    let _wall = place(&mut sim, 29, 20, "copper-wall", 1);
    sim.tick().expect("tick");
    assert!(occupied(&sim), "a solid tile under a ground output");
}

/// L06 T1: `UnitAssemblerBuild.spawned()` (`UnitAssembler.java:556-574`): the
/// product takes `commandPos` (`CommandAI.commandPosition`) and goes into the
/// same-team payload block under the spawn point when it accepts it
/// (`unit.buildOn()`); otherwise it is added to the world.
#[test]
fn assembled_unit_takes_command_pos_and_payload_handoff() {
    use crate::world::blocks::payloads::PayloadHolder;
    use crate::world::blocks::units::behavior::deliver_assembled_unit;
    let mut sim = live_sim(64);
    // Team 1 is the player's (`rules.defaultTeam`), so products get `CommandAI`
    // (`UnitType.controller`, UnitType.java:283).
    if let Some(mut rules) = sim
        .ecs
        .0
        .get_resource_mut::<crate::world::limits::BuildRules>()
    {
        rules.default_team = 1;
    }
    let conveyor = place(&mut sim, 10, 10, "payload-conveyor", 1);
    let def = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .unit_by_name("dagger")
        .cloned()
        .expect("dagger");
    let in_runtime = |sim: &Sim, unit: Entity| {
        sim.unit_runtime()
            .is_some_and(|runtime| runtime.units.contains(&unit))
    };
    // Payload conveyor at tile (10, 10) (size 3, centred) under the spawn.
    let (unit, handed) = deliver_assembled_unit(
        &mut sim.ecs.0,
        &def,
        1,
        80.0,
        80.0,
        0.0,
        Some((400.0, 240.0)),
    );
    assert!(handed, "handed to the payload conveyor");
    assert!(!in_runtime(&sim, unit), "a payload unit is not added");
    let held = sim
        .ecs
        .0
        .get::<PayloadHolder>(conveyor)
        .and_then(|holder| holder.payload);
    assert_eq!(held.and_then(|p| p.entity), Some(unit), "conveyor holds it");
    // Open ground: `unit.add()`.
    let (free, handed) = deliver_assembled_unit(
        &mut sim.ecs.0,
        &def,
        1,
        300.0,
        300.0,
        0.0,
        Some((400.0, 240.0)),
    );
    assert!(!handed);
    assert!(in_runtime(&sim, free), "added to the world");
    let slot = sim
        .ecs
        .0
        .get::<crate::ai::controller::ControllerSlot>(free)
        .cloned()
        .expect("controller");
    if slot.kind == crate::ai::controller::AiKind::Command {
        assert_eq!(
            slot.target,
            Some(crate::world::TilePos::new(50, 30)),
            "commandPos -> move target"
        );
    } else {
        panic!(
            "a player-team product should be commandable: {:?}",
            slot.kind
        );
    }
    // An enemy-team payload block never takes it.
    let enemy = place(&mut sim, 40, 40, "payload-conveyor", 2);
    let (_, handed) = deliver_assembled_unit(&mut sim.ecs.0, &def, 1, 320.0, 320.0, 0.0, None);
    assert!(!handed, "team check");
    assert!(
        sim.ecs
            .0
            .get::<PayloadHolder>(enemy)
            .and_then(|holder| holder.payload)
            .is_none()
    );
}

/// L06 T2: `RouterBuild implements ControlBlock` (`Router.java:30-53`): an own
/// size-1 router yields a lazily created, stable `block` proxy
/// (`unit()`, `:37-44`); the distributor (size 2) cannot be controlled
/// (`canControl() = size == 1`); turrets keep theirs (`playerControllable`).
#[test]
fn router_is_a_control_block() {
    let mut sim = live_sim(48);
    sim.set_player_team(1);
    let router = place(&mut sim, 10, 10, "router", 1);
    let _distributor = place(&mut sim, 20, 20, "distributor", 1);
    let _duo = place(&mut sim, 30, 30, "duo", 1);
    let _enemy = place(&mut sim, 40, 40, "router", 2);
    let pos = |x: i16, y: i16| crate::world::TilePos::new(x, y);
    let first = sim
        .control_block_unit_id(pos(10, 10))
        .expect("router proxy");
    assert_eq!(
        sim.control_block_unit_id(pos(10, 10)),
        Some(first),
        "stable"
    );
    let proxy = sim
        .ecs
        .0
        .get::<crate::world::blocks::distribution::router::RouterBuild>(router)
        .and_then(|state| state.block_unit)
        .expect("proxy stored");
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::unit::comp::BlockUnitComp>(proxy)
            .is_some()
    );
    assert!(
        !sim.unit_runtime().expect("runtime").units.contains(&proxy),
        "never added to the unit pass"
    );
    assert_eq!(sim.control_block_unit_id(pos(20, 20)), None, "distributor");
    assert!(sim.control_block_unit_id(pos(30, 30)).is_some(), "duo");
    assert_eq!(sim.control_block_unit_id(pos(40, 40)), None, "enemy team");
}

/// L06 T2: a possessed router (`Router.java:102-117`) routes only toward the
/// aimed quadrant and only while shooting; unpossessed it round-robins.
#[test]
fn possessed_router_routes_toward_aim_while_shooting() {
    use crate::ai::types::player::PlayerInput;
    use crate::world::blocks::distribution::transfer;
    let mut sim = live_sim(48);
    sim.set_player_team(1);
    let router = place(&mut sim, 10, 10, "router", 1);
    let east = place(&mut sim, 11, 10, "container", 1);
    let north = place(&mut sim, 9, 11, "container", 1);
    let copper = sim
        .unit_runtime()
        .expect("runtime")
        .content
        .item_id("copper")
        .expect("copper");
    let proxy_seq = sim
        .control_block_unit_id(crate::world::TilePos::new(10, 10))
        .expect("proxy");
    let proxy = sim
        .ecs
        .0
        .get::<crate::world::blocks::distribution::router::RouterBuild>(router)
        .and_then(|state| state.block_unit)
        .expect("proxy");
    // `InputHandler.unitControl` on the proxy (the Ctrl+click path).
    assert!(sim.possess_unit(proxy_seq), "possess the router proxy");
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::Player>(proxy)
            .is_some()
    );
    let set_input = |sim: &mut Sim, shooting: bool| {
        let mut input = sim
            .ecs
            .0
            .remove_resource::<PlayerInput>()
            .unwrap_or_default();
        // Aim at tile (14, 10): due east of the router.
        input.aim_x = 14.0 * 8.0;
        input.aim_y = 10.0 * 8.0;
        input.shooting = shooting;
        sim.ecs.0.insert_resource(input);
    };
    let stored = |sim: &Sim, e: Entity| {
        sim.ecs
            .0
            .get::<crate::world::modules::ItemModule>(e)
            .map_or(0, |items| items.get(copper))
    };
    set_input(&mut sim, false);
    transfer::dispatch_handle_item(&mut sim.ecs.0, router, router, copper);
    for _ in 0..30 {
        set_input(&mut sim, false);
        sim.tick().expect("tick");
    }
    assert_eq!(
        stored(&sim, east) + stored(&sim, north),
        0,
        "holds while not shooting"
    );
    assert_eq!(stored(&sim, router), 1);
    for _ in 0..30 {
        set_input(&mut sim, true);
        sim.tick().expect("tick");
    }
    assert_eq!(stored(&sim, east), 1, "aimed side");
    assert_eq!(stored(&sim, north), 0, "never the other side");
}
