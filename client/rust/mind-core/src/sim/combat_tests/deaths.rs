// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Unit and building death regressions: crashes, rebuild plans, container explosions and fire starts.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{
    campaign_runtime, health, live_sim, place_at, tick_n, unit_content_block,
};

/// T17: a dying flying unit crashes with area damage around its position
/// (`UnitComp.destroy` -> `Damage.damage(team, x, y, hitSize^0.94 * 1.25,
/// hitSize^0.75 * crashDamageMultiplier * 2.5 * rules.unitCrashDamage(team))`,
/// UnitComp.java:921-928).
#[test]
fn crashing_flare_damages_neighbours() {
    let mut sim = live_sim(32);
    let (wx, wy) = (15, 12);
    let wall = place_at(&mut sim, wx, wy, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let flare = sim
        .spawn_live_unit(
            "flare",
            2,
            wx as f32 * 8.0 + 4.0,
            wy as f32 * 8.0 + 4.0,
            0.0,
        )
        .expect("flare");
    if let Some(mut health) = sim.ecs.0.get_mut::<Health>(flare) {
        health.health = 0.0;
        health.dead = true;
    }
    sim.tick().expect("tick");
    // H01 T6: the flare sinks as a `Falling` wreck for `ceil(1 / 0.018) = 56`
    // ticks before `Call.unitDestroy` runs the destroy body
    // (`UnitComp.killed`, UnitComp.java:980-991; `UnitComp.update`,
    // UnitComp.java:803-830).
    for _ in 0..120 {
        if sim.ecs.0.get_entity(flare).is_err() {
            break;
        }
        sim.tick().expect("tick");
    }
    assert!(
        sim.ecs.0.get_entity(flare).is_err(),
        "the crashed flare was removed"
    );
    assert!(
        health(&sim, wall).is_none_or(|hp| hp < wall_start),
        "the crash damaged the neighbouring wall: {:?}",
        health(&sim, wall)
    );
}

/// T16: destroying a rebuildable block pushes a ghost plan onto its team's
/// queue (`Logic`'s `BlockDestroyEvent` -> `BuildingComp.addPlan(true)`,
/// Logic.java:42-50).
#[test]
fn destroyed_building_leaves_rebuild_plan() {
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
        ..Rules::default()
    });
    session.phase = crate::game::State::Playing;
    sim.ecs
        .0
        .insert_resource(CampaignRuntime::new(session, campaign));

    let wall_block = unit_content_block(&sim, "copper-wall");
    let wall = place_at(&mut sim, 10, 10, "copper-wall", 1);
    sim.ecs
        .0
        .get_mut::<Health>(wall)
        .expect("wall health")
        .health = 0.0;
    sim.tick().expect("tick");

    let campaign = campaign_runtime(&sim);
    let plans = &campaign
        .session
        .teams
        .get_or_null(crate::ecs::TeamId(1))
        .expect("team 1")
        .plans;
    assert!(
        plans
            .front()
            .is_some_and(|plan| plan.block.raw() == wall_block && plan.x == 10 && plan.y == 10),
        "ghost plan queued for the dead wall: {plans:?}"
    );
}

/// T5: a destroyed container full of blast compound explodes with wave damage
/// (`BuildingComp.onDestroyed` -> `Damage.dynamicExplosion`,
/// BuildingComp.java:1490-1518).
#[test]
fn destroyed_blast_container_damages_neighbours() {
    let mut sim = live_sim(48);
    let container = place_at(&mut sim, 10, 10, "container", 1);
    let wall = place_at(&mut sim, 12, 10, "copper-wall", 1);
    let wall_start = health(&sim, wall).expect("wall health");
    let blast = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("blast-compound")
        .expect("blast-compound");
    {
        let Some(mut module) = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(container)
        else {
            return; // no item module fixture
        };
        module.items[blast.index()] = 300;
        module.total = 300;
    }
    sim.ecs
        .0
        .get_mut::<Health>(container)
        .expect("container health")
        .health = 0.0;
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(container).is_err(),
        "the container was destroyed"
    );
    // `Damage.dynamicExplosion` staggers its waves 2 ticks apart
    // (`Time.run(i * 2f)`); the outer waves carry the radius.
    tick_n(&mut sim, 60);
    assert!(
        health(&sim, wall).is_none_or(|hp| hp < wall_start),
        "the explosion damaged the wall 2 tiles away: {:?}",
        health(&sim, wall)
    );
}

/// T5: a destroyed container full of flammable items leaves a fire
/// (`fire = true` on the building-destroy explosion path).
#[test]
fn destroyed_coal_container_starts_fire() {
    let mut sim = live_sim(48);
    let container = place_at(&mut sim, 10, 10, "container", 1);
    let coal = sim
        .unit_runtime()
        .expect("rt")
        .content
        .item_id("coal")
        .expect("coal");
    {
        let Some(mut module) = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::ItemModule>(container)
        else {
            return;
        };
        module.items[coal.index()] = 300;
        module.total = 300;
    }
    sim.ecs
        .0
        .get_mut::<Health>(container)
        .expect("container health")
        .health = 0.0;
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(container).is_err(),
        "the container was destroyed"
    );
    // The fire now comes from the staggered `Bullets.fireball` spawns
    // (`Time.run(i / 2f)`), not an immediate `Fires.create`.
    tick_n(&mut sim, 40);
    let fire = (10..=11)
        .any(|tx| (10..=11).any(|ty| crate::combat::fires::find_at(&sim.ecs.0, tx, ty).is_some()));
    assert!(
        fire,
        "the coal explosion left a fire at the container tile (`World.toTile` of the block centre (84,84) rounds to (11,11) like Java's `Math.round`)"
    );
}
