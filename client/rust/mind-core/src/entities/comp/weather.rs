// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Weather state runtime (`Weather.WeatherStateComp` sim half).
//!
//! Ports the gameplay half of `core/src/mindustry/type/Weather.java`: the
//! per-tick `updateEffect` status application and the `WeatherStateComp.update`
//! lifetime/opacity bookkeeping. Drawing (`drawOver`/`drawUnder`) stays in
//! [`crate::fx::weather_fx`]. Weathers without a status (vanilla's `snowing`,
//! `sandstorm`, `fog`, `suspend-particles`) have no sim effect; `rain` applies
//! `wet` and `sporestorm` applies `spore-slowed` to air units only.
//!
//! The weather scheduler (game layer) owns live [`WeatherRuntime`] entities:
//! spawn one with the marker [`crate::entities::comp::WeatherState`] when the
//! weather starts and call [`update_weather_state`] once per tick; the status
//! path needs an installed [`crate::combat::damage::StatusApplier`].

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

use crate::content::{ContentRegistry, StatusId, WeatherId};
use crate::ecs::EntitySeq;
use crate::entities::comp::unit::comp::PhysicsComp;
use crate::entities::comp::{TeamComp, Unit};

/// `Weather.WeatherStateComp.fadeTime`: opacity fade-in window (ticks).
pub const WEATHER_FADE_TIME: f32 = 60.0 * 4.0;

/// `Weather.updateEffect`: `state.effectTimer = statusDuration - 5f`.
pub const WEATHER_STATUS_MARGIN: f32 = 5.0;

/// Weather state runtime (`WeatherStateComp` fields; draw fields are the
/// view's).
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub struct WeatherRuntime {
    /// Active weather content id (`WeatherState.weather`).
    pub weather: WeatherId,
    /// Intensity `0..1` (`WeatherState.intensity`).
    pub intensity: f32,
    /// Current opacity `0..1` (`WeatherState.opacity`).
    pub opacity: f32,
    /// Remaining lifetime in ticks (`WeatherState.life`).
    pub life: f32,
    /// Status re-apply countdown (`WeatherState.effectTimer`).
    pub effect_timer: f32,
    /// Wind vector (`WeatherState.windVector`; view input only).
    pub wind_x: f32,
    pub wind_y: f32,
}

impl WeatherRuntime {
    /// `Weather.create(intensity, duration)`.
    pub fn new(weather: WeatherId, intensity: f32, life: f32) -> Self {
        Self {
            weather,
            intensity: intensity.clamp(0.0, 1.0),
            opacity: 0.0,
            life,
            effect_timer: 0.0,
            wind_x: 0.0,
            wind_y: 0.0,
        }
    }
}

/// Applies the weather's status to every matching unit (`Weather.updateEffect`).
///
/// `checkTarget(statusAir, statusGround)`: air units take air statuses, ground
/// units take ground statuses.
pub fn apply_weather_status(
    world: &mut World,
    content: &ContentRegistry,
    weather: WeatherId,
    status: StatusId,
    status_duration: f32,
) -> usize {
    let Some(def) = content.weather(weather) else {
        return 0;
    };
    if status == StatusId::NONE || status_duration <= 0.0 {
        return 0;
    }
    let mut units: Vec<(u64, Entity)> = world
        .iter_entities()
        .filter(|entity_ref| entity_ref.get::<Unit>().is_some())
        .map(|entity_ref| {
            let seq = entity_ref
                .get::<EntitySeq>()
                .map(|seq| seq.0)
                .unwrap_or(u64::MAX);
            (seq, entity_ref.id())
        })
        .collect();
    units.sort_by_key(|(seq, entity)| (*seq, entity.index()));
    let mut applied = 0usize;
    for (_, unit) in units {
        let flying = world
            .get::<PhysicsComp>(unit)
            .map(|physics| physics.flying)
            .unwrap_or(false);
        let targeted = (flying && def.status_air) || (!flying && def.status_ground);
        if !targeted {
            continue;
        }
        if world.get::<TeamComp>(unit).is_none() {
            continue;
        }
        crate::combat::damage::apply_status(world, unit, status, status_duration);
        applied += 1;
    }
    applied
}

/// Advances one weather state entity one tick (`WeatherStateComp.update`).
///
/// Returns `false` (and despawns the entity) once `life < 0`.
pub fn update_weather_state(world: &mut World, content: &ContentRegistry, entity: Entity) -> bool {
    let Some(mut runtime) = world.get::<WeatherRuntime>(entity).copied() else {
        return false;
    };
    if runtime.life < WEATHER_FADE_TIME {
        runtime.opacity = runtime
            .opacity
            .min((runtime.life / WEATHER_FADE_TIME).max(0.0));
    } else {
        runtime.opacity += (1.0 - runtime.opacity) * 0.004;
    }

    runtime.life -= 1.0;

    // `Weather.update` is a no-op for every vanilla weather; only the status
    // half of `Weather.updateEffect` is simulated.
    if let Some(def) = content.weather(runtime.weather)
        && def.status != StatusId::NONE
    {
        if runtime.effect_timer <= 0.0 {
            runtime.effect_timer = def.status_duration - WEATHER_STATUS_MARGIN;
            apply_weather_status(
                world,
                content,
                runtime.weather,
                def.status,
                def.status_duration,
            );
        } else {
            runtime.effect_timer -= 1.0;
        }
    }

    if runtime.life < 0.0 {
        let _ = world.despawn(entity);
        return false;
    }
    if let Some(mut stored) = world.get_mut::<WeatherRuntime>(entity) {
        *stored = runtime;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::UnitHarness;
    use crate::combat::damage::status::install_status_applier;
    use crate::entities::comp::unit::comp::StatusComp;

    fn harness(seed: u64) -> UnitHarness {
        let mut harness = UnitHarness::new(32, 32, seed);
        install_status_applier(&mut harness.build.world, &harness.build.content);
        harness
    }

    #[test]
    fn rain_applies_wet_to_ground_and_air() {
        let mut harness = harness(7);
        let ground = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let air = harness.spawn("flare", 0, 80.0, 80.0, 0.0).expect("flare");
        let wet = harness.content().status_id("wet").expect("wet");
        let rain = harness.content().weather_by_name("rain").expect("rain").id;
        let state = harness
            .build
            .world
            .spawn(WeatherRuntime::new(rain, 1.0, 600.0))
            .id();
        assert!(update_weather_state(
            &mut harness.build.world,
            &harness.build.content,
            state
        ));
        for unit in [ground, air] {
            assert!(
                harness
                    .build
                    .world
                    .get::<StatusComp>(unit)
                    .expect("status")
                    .has_effect_of(wet),
                "rain applied wet"
            );
        }
    }

    #[test]
    fn sporestorm_only_affects_air_units() {
        let mut harness = harness(11);
        let ground = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let air = harness.spawn("flare", 0, 80.0, 80.0, 0.0).expect("flare");
        let spore = harness.content().status_id("spore-slowed").expect("spore");
        let storm = harness
            .content()
            .weather_by_name("sporestorm")
            .expect("sporestorm")
            .id;
        let state = harness
            .build
            .world
            .spawn(WeatherRuntime::new(storm, 1.0, 600.0))
            .id();
        update_weather_state(&mut harness.build.world, &harness.build.content, state);
        assert!(
            harness
                .build
                .world
                .get::<StatusComp>(air)
                .expect("status")
                .has_effect_of(spore)
        );
        assert!(
            !harness
                .build
                .world
                .get::<StatusComp>(ground)
                .expect("status")
                .has_effect_of(spore),
            "statusGround = false skips ground units"
        );
    }

    #[test]
    fn effect_timer_reapplies_at_status_duration_minus_five() {
        let mut harness = harness(13);
        let unit = harness.spawn("dagger", 0, 64.0, 64.0, 0.0).expect("dagger");
        let wet = harness.content().status_id("wet").expect("wet");
        let rain = harness.content().weather_by_name("rain").expect("rain");
        let duration = rain.status_duration;
        let id = rain.id;
        let state = harness
            .build
            .world
            .spawn(WeatherRuntime::new(id, 1.0, 600.0))
            .id();
        // First update applies `wet` for `statusDuration`, then sets the timer
        // to `statusDuration - 5`.
        update_weather_state(&mut harness.build.world, &harness.build.content, state);
        let applied = harness
            .build
            .world
            .get::<StatusComp>(unit)
            .expect("status")
            .get_duration(wet);
        assert_eq!(applied, duration);
        let timer = harness
            .build
            .world
            .get::<WeatherRuntime>(state)
            .expect("runtime")
            .effect_timer;
        assert_eq!(timer, duration - WEATHER_STATUS_MARGIN);
    }

    #[test]
    fn expired_weather_is_removed() {
        let mut harness = harness(17);
        let rain = harness.content().weather_by_name("rain").expect("rain").id;
        let state = harness
            .build
            .world
            .spawn(WeatherRuntime::new(rain, 1.0, 0.0))
            .id();
        assert!(!update_weather_state(
            &mut harness.build.world,
            &harness.build.content,
            state
        ));
        assert!(harness.build.world.get_entity(state).is_err());
    }
}
