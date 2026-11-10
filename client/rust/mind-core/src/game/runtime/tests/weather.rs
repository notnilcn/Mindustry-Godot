// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `Logic.updateWeather` and weather env-attribute tests.
//! Upstream: none (port-only tests).

use crate::game::State;
use crate::game::runtime::test_support::*;
use crate::game::runtime::*;

/// PAR-0127: every `logic_play` re-arms the weather cooldowns (`PlayEvent`
/// shuffle + cumulative assignment, Logic.java:90-99) instead of only when
/// all of them read zero, so a loaded save with pending cooldowns still
/// re-shuffles exactly like Ctrl+Shift+F9/new game.
#[test]
fn weather_cooldowns_reset_on_play() {
    use crate::game::play::logic_play;
    use crate::game::rules::WeatherEntry;
    use crate::sim::unit_runtime::UnitRuntime;
    use crate::world::WorldGrid;

    let registry = {
        let mut registry = content();
        registry.init().expect("init");
        registry
    };
    let grid = WorldGrid::new(32, 32);
    let runtime_content = {
        let mut content = content();
        content.init().expect("init");
        content
    };
    let mut world = World::new();
    world.insert_non_send(UnitRuntime::new(runtime_content, &grid, &world, 2, false));

    let campaign = campaign(&registry);
    let mut session = PlaySession::new(Rules {
        weather: vec![
            WeatherEntry {
                weather: String::from("rain"),
                cooldown: 9999.0,
                max_frequency: 600.0,
                ..WeatherEntry::default()
            },
            WeatherEntry {
                weather: String::from("snowing"),
                cooldown: 9999.0,
                max_frequency: 600.0,
                ..WeatherEntry::default()
            },
        ],
        ..Rules::default()
    });
    session.phase = State::Playing;
    world.insert_resource(CampaignRuntime::new(session, campaign));

    {
        let mut runtime = world
            .remove_resource::<CampaignRuntime>()
            .expect("campaign runtime");
        logic_play(&mut runtime.session);
        world.insert_resource(runtime);
    }
    weather_system(&mut world);

    let runtime = world
        .remove_resource::<CampaignRuntime>()
        .expect("campaign runtime");
    assert!(
        !runtime.session.weather_reset_pending,
        "the reset flag is consumed by the first weather tick"
    );
    for entry in &runtime.session.rules.weather {
        assert!(
            entry.cooldown < 9999.0,
            "{} cooldown re-armed: {}",
            entry.weather,
            entry.cooldown
        );
    }
    world.insert_resource(runtime);
}

/// T9: `Logic.java:603-606` envAttrs -> `GeneratorEnv` (rain light -0.2).
#[test]
fn weather_env_feeds_generator_env() {
    use crate::entities::comp::WeatherState;
    use crate::entities::comp::weather::WeatherRuntime;
    use crate::sim::unit_runtime::UnitRuntime;
    use crate::world::WorldGrid;

    let registry = {
        let mut registry = content();
        registry.init().expect("init");
        registry
    };
    let grid = WorldGrid::new(32, 32);
    let runtime_content = {
        let mut content = content();
        content.init().expect("init");
        content
    };
    let mut world = World::new();
    world.insert_non_send(UnitRuntime::new(runtime_content, &grid, &world, 2, false));
    let campaign = campaign(&registry);
    let mut session = PlaySession::new(Rules {
        solar_multiplier: 1.0,
        ..Rules::default()
    });
    session.phase = State::Playing;
    world.insert_resource(CampaignRuntime::new(session, campaign));

    // One active rain at full opacity (`Weathers.java` light = -0.2).
    let rain = registry.weather_by_name("rain").expect("rain").id;
    world.spawn((
        WeatherRuntime {
            opacity: 1.0,
            ..WeatherRuntime::new(rain, 1.0, 600.0)
        },
        WeatherState,
        crate::ecs::EntitySeq(0),
    ));
    weather_system(&mut world);
    let env = world
        .get_resource::<crate::world::blocks::power::behavior::GeneratorEnv>()
        .expect("generator env inserted");
    assert!(
        (env.light_env - (-0.2)).abs() < 1.0e-6,
        "rain light env {}",
        env.light_env
    );
    assert!(
        (env.water_env - 0.2).abs() < 1.0e-6,
        "rain water env {}",
        env.water_env
    );
    assert!(
        (env.spores_env).abs() < 1.0e-6,
        "rain spores env {}",
        env.spores_env
    );
    assert_eq!(env.solar_multiplier, 1.0);
}
