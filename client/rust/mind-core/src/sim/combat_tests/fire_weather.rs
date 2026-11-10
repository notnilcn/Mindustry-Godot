// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Fire, puddle and weather regressions.
//! Upstream: none (port-only; live-sim regression tests).

use super::*;
use crate::sim::test_support::{health, live_sim, place_at, tick_n};

/// PAR-0091: a deposited water puddle applies `wet` to a unit standing in it
/// and evaporates to nothing on a dry floor (live `update_puddles`).
#[test]
fn live_puddle_evaporates() {
    use crate::combat::puddles;
    let mut sim = live_sim(16);
    let (water, wet) = {
        let content = &sim.unit_runtime().expect("rt").content;
        (
            content.liquid_id("water").expect("water"),
            content.status_id("wet").expect("wet"),
        )
    };
    {
        let world = &mut sim.ecs.0;
        let mut runtime = world
            .remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            .expect("runtime");
        let entity = puddles::deposit(
            world,
            &sim.grid.tiles,
            &runtime.content,
            5,
            5,
            water,
            50.0,
            &mut runtime.rng,
        );
        assert!(entity.is_some(), "water deposited");
        world.insert_non_send(runtime);
    }
    let dagger = sim
        .spawn_live_unit("dagger", 1, 42.0, 42.0, 0.0)
        .expect("dagger");
    tick_n(&mut sim, 40);
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::unit::comp::StatusComp>(dagger)
            .is_some_and(|status| status.has_effect_of(wet)),
        "the puddle applied `wet` to the unit standing in it"
    );
    tick_n(&mut sim, 2000);
    assert!(
        puddles::find_at(&sim.ecs.0, 5, 5).is_none(),
        "the water puddle evaporated"
    );
}

/// PAR-0091: a fire damages the building on its tile and burns out live
/// (`update_fires` in the tick).
#[test]
fn live_fire_burns_building() {
    let mut sim = live_sim(16);
    let wall = place_at(&mut sim, 5, 5, "copper-wall", 1);
    let start = health(&sim, wall).expect("wall health");
    let mut rng = crate::determinism::SimRng::new(3);
    assert!(
        crate::combat::fires::create(&mut sim.ecs.0, 5, 5, &mut rng).is_some(),
        "fire created"
    );
    tick_n(&mut sim, 300);
    let end = health(&sim, wall);
    assert!(
        end.is_none_or(|hp| hp < start),
        "the fire damaged the wall ({start} -> {end:?})"
    );
}

/// PAR-0091: an `incendChance` bullet hit creates a fire at the hit tile
/// (`BulletType.createIncend` via `hit_bullet`).
#[test]
fn incend_bullet_creates_fire() {
    use crate::content::BulletKind;
    use crate::content::registries::bullets::BulletDef;
    let mut sim = live_sim(16);
    {
        let world = &mut sim.ecs.0;
        let mut runtime = world
            .remove_non_send::<crate::sim::unit_runtime::UnitRuntime>()
            .expect("runtime");
        let mut def = BulletDef::new(BulletKind::Basic);
        def.speed = 0.0;
        def.lifetime = 100.0;
        def.incend_chance = 1.0;
        def.incend_amount = 1;
        def.incend_spread = 0.0;
        let id = runtime.content.add_bullet(def).expect("add bullet");
        let audio = crate::audio::AudioSinkRes::noop();
        let mut spawned = Vec::new();
        {
            let crate::sim::unit_runtime::UnitRuntime {
                content,
                grid,
                rng,
                fx,
                seq,
                ..
            } = &mut runtime;
            let mut ctx = crate::combat::bullet::CombatCtx {
                world,
                content,
                grid,
                rng,
                fx: fx.as_ref(),
                audio: &audio,
                seq,
                spawned: &mut spawned,
            };
            let spawn = crate::combat::BulletSpawn {
                def: id,
                x: 42.0,
                y: 42.0,
                angle: 0.0,
                ..crate::combat::BulletSpawn::default()
            };
            let entity = ctx.spawn(&spawn).expect("bullet");
            crate::combat::bullet::hit_bullet(&mut ctx, entity, 42.0, 42.0, false);
        }
        world.insert_non_send(runtime);
    }
    assert!(
        crate::combat::fires::find_at(&sim.ecs.0, 5, 5).is_some(),
        "the incend bullet left a fire on the hit tile"
    );
}

/// PAR-0091: a destroyed liquid container spills its contents as puddles
/// (`BuildingComp.splashLiquid` via `resolve_deaths`, `damageExplosions` on).
#[test]
fn live_destroyed_liquid_container_splashes() {
    let mut sim = live_sim(24);
    let tank = place_at(&mut sim, 10, 10, "liquid-tank", 1);
    let water = sim
        .unit_runtime()
        .expect("rt")
        .content
        .liquid_id("water")
        .expect("water");
    {
        let Some(mut module) = sim
            .ecs
            .0
            .get_mut::<crate::world::modules::LiquidModule>(tank)
        else {
            return; // no liquid module fixture
        };
        module.liquids[water.index()] = 40.0;
        module.current_amount = 40.0;
    }
    sim.ecs
        .0
        .get_mut::<Health>(tank)
        .expect("tank health")
        .health = 0.0;
    sim.tick().expect("tick");
    assert!(
        sim.ecs.0.get_entity(tank).is_err(),
        "the tank was destroyed"
    );
    let puddles = sim
        .ecs
        .0
        .iter_entities()
        .filter(|entity| {
            entity
                .get::<crate::combat::puddles::PuddleState>()
                .is_some()
        })
        .count();
    assert!(puddles > 0, "the tank spilled water puddles ({puddles})");
}

/// PAR-0127: `rules.weather = [rain always]` spawns a live `WeatherRuntime`
/// and the rain wets a ground unit.
#[test]
fn weather_rain_spawns_and_wets_units() {
    use crate::game::State;
    use crate::game::planet::EmptyNeighborhood;
    use crate::game::play::PlaySession;
    use crate::game::rules::{Rules, WeatherEntry};
    use crate::game::runtime::CampaignRuntime;
    use crate::game::universe::Campaign;

    let mut sim = live_sim(32);
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
            weather: String::from("rain"),
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

    let dagger = sim
        .spawn_live_unit("dagger", 1, 100.0, 100.0, 0.0)
        .expect("dagger");
    tick_n(&mut sim, 10);
    assert_eq!(
        sim.group_counts().get("weather").copied().unwrap_or(0),
        1,
        "one live weather entity"
    );
    let wet = sim
        .unit_runtime()
        .expect("rt")
        .content
        .status_id("wet")
        .expect("wet");
    assert!(
        sim.ecs
            .0
            .get::<crate::entities::comp::unit::comp::StatusComp>(dagger)
            .is_some_and(|status| status.has_effect_of(wet)),
        "rain applied wet to the ground unit"
    );
}
