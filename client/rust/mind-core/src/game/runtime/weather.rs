// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `TickSet::WeatherAndAi` weather half: `Logic.updateWeather`, the weather
//! env attributes and the water-attribute query.
//! Upstream: core/src/mindustry/type/Weather.java,
//! core/src/mindustry/core/Logic.java (`updateWeather`)

use super::*;
use crate::content::ContentRegistry;
use crate::game::State;
use bevy_ecs::entity::Entity;

/// `Logic.updateWeather` (`Logic.java:376-391`) plus the `PlayEvent` cooldown
/// initialization (`Logic.java:90-99`): weather entries re-arm after their
/// duration + frequency and spawn a live [`WeatherRuntime`] entity; every
/// active weather advances its lifetime/status one tick.
///
/// Runs in `TickSet::WeatherAndAi` before `team_ai_system`; no-op unless the
/// campaign is playing and the live `UnitRuntime` (content + rng) is
/// installed. Cooldowns are initialized on the first tick where every entry
/// reads `0` (the persisted initial state), with the same Fisher-Yates +
/// cumulative-sum assignment as upstream's shuffled `PlayEvent` handler.
pub fn weather_system(world: &mut World) {
    use crate::entities::comp::weather::{WeatherRuntime, update_weather_state};

    let Some(mut campaign) = world.remove_resource::<CampaignRuntime>() else {
        return;
    };
    if campaign.session.phase != State::Playing || campaign.is_client {
        world.insert_resource(campaign);
        return;
    }
    let Some(mut runtime) = world.remove_non_send::<crate::sim::unit_runtime::UnitRuntime>() else {
        world.insert_resource(campaign);
        return;
    };

    // `state.rules.weather.removeAll(w -> w.weather == null)`: drop names that
    // do not resolve to content.
    campaign
        .session
        .rules
        .weather
        .retain(|entry| runtime.content.weather_by_name(&entry.weather).is_some());
    if !campaign.session.rules.weather.is_empty() && campaign.session.weather_reset_pending {
        // `PlayEvent`: shuffle then `cooldown = sum + random(maxFrequency)`.
        let mut order: Vec<usize> = (0..campaign.session.rules.weather.len()).collect();
        for i in (1..order.len()).rev() {
            let j = runtime
                .rng
                .random(crate::determinism::RngStream::Sim, (i + 1) as i32)
                as usize;
            order.swap(i, j);
        }
        let mut sum = 0.0f32;
        for &index in &order {
            let max_frequency = campaign.session.rules.weather[index].max_frequency;
            let cooldown = sum
                + runtime
                    .rng
                    .range(crate::determinism::RngStream::Sim, 0.0, max_frequency);
            campaign.session.rules.weather[index].cooldown = cooldown;
            sum += cooldown;
        }
        campaign.session.weather_reset_pending = false;
    }

    for index in 0..campaign.session.rules.weather.len() {
        let entry = &mut campaign.session.rules.weather[index];
        // `update cooldown`.
        entry.cooldown -= 1.0;
        let Some(weather) = runtime
            .content
            .weather_by_name(&entry.weather)
            .map(|def| def.id)
        else {
            continue;
        };
        let (always, intensity, min_duration, max_duration, min_frequency, max_frequency) = (
            entry.always,
            entry.intensity,
            entry.min_duration,
            entry.max_duration,
            entry.min_frequency,
            entry.max_frequency,
        );
        let active = world.iter_entities().any(|entity| {
            entity
                .get::<WeatherRuntime>()
                .is_some_and(|state| state.weather == weather)
        });
        if (entry.cooldown < 0.0 || always) && !active {
            let duration = if always {
                f32::INFINITY
            } else {
                runtime.rng.range(
                    crate::determinism::RngStream::Sim,
                    min_duration,
                    max_duration,
                )
            };
            entry.cooldown = duration
                + runtime.rng.range(
                    crate::determinism::RngStream::Sim,
                    min_frequency,
                    max_frequency,
                );
            let angle = runtime
                .rng
                .range(crate::determinism::RngStream::Sim, 0.0, 360.0)
                .to_radians();
            let seq = runtime.seq;
            runtime.seq = seq.wrapping_add(1);
            crate::entities::comp::weather::create_weather(
                world,
                seq,
                weather,
                intensity,
                duration,
                (angle.cos(), angle.sin()),
            );
        }
    }

    // Update every active weather in sequence order.
    let mut states: Vec<(u64, Entity)> = world
        .iter_entities()
        .filter(|entity| entity.get::<WeatherRuntime>().is_some())
        .map(|entity| {
            (
                entity
                    .get::<crate::ecs::EntitySeq>()
                    .map_or(u64::MAX, |seq| seq.0),
                entity.id(),
            )
        })
        .collect();
    states.sort_by_key(|(seq, entity)| (*seq, entity.index()));
    for (_, entity) in states {
        update_weather_state(world, &runtime.content, entity);
    }

    // `Logic.java:603-606`: `state.envAttrs` = rules attributes + Σ weather
    // attrs × opacity; consumers (solar/thermal/steam/heat) read `GeneratorEnv`,
    // which nothing else inserts in live play (T9). Java computes envAttrs
    // after `updateWeather` and before `updateEntities`; generators run in the
    // next tick's `update_buildings` (one-tick lag, accepted).
    {
        use crate::content::registries::weathers::Attribute;
        use crate::world::blocks::power::behavior::GeneratorEnv;
        let rules = &campaign.session.rules;
        let env = |attribute: Attribute| {
            let mut total = rules.attributes.get(attribute);
            for entity in world.iter_entities() {
                let Some(weather) = entity.get::<WeatherRuntime>() else {
                    continue;
                };
                if let Some(def) = runtime.content.weather(weather.weather) {
                    total += def.attr(attribute) * weather.opacity;
                }
            }
            total
        };
        world.insert_resource(GeneratorEnv {
            solar_multiplier: rules.solar_multiplier,
            lighting: rules.lighting,
            ambient_light_alpha: rules.ambient_light.0.a,
            light_env: env(Attribute::Light),
            heat_env: env(Attribute::Heat),
            steam_env: env(Attribute::Steam),
            water_env: env(Attribute::Water),
            spores_env: env(Attribute::Spores),
            reactor_explosions: rules.reactor_explosions,
        });
    }

    world.insert_non_send(runtime);
    world.insert_resource(campaign);
}

/// `state.envAttrs.get(Attribute.water)` from the active weathers: sum of
/// `Weather.attrs[water] * opacity` (`Logic.java:603-606`). Drives fire
/// extinguish speed and unit wetness in the view/sim.
pub fn weather_water_attr(world: &World, content: &ContentRegistry) -> f32 {
    use crate::content::registries::weathers::Attribute;
    // Java's envAttrs starts from `rules.attributes` (T9).
    let mut total = world
        .get_resource::<CampaignRuntime>()
        .map(|runtime| runtime.session.rules.attributes.get(Attribute::Water))
        .unwrap_or(0.0);
    for entity in world.iter_entities() {
        let Some(runtime) = entity.get::<crate::entities::comp::weather::WeatherRuntime>() else {
            continue;
        };
        let Some(def) = content.weather(runtime.weather) else {
            continue;
        };
        for (attribute, value) in &def.attrs {
            if *attribute == Attribute::Water {
                total += value * runtime.opacity;
            }
        }
    }
    total
}
