// SPDX-License-Identifier: GPL-3.0-only
//
// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// Source: core/src/mindustry/world/blocks/power/PowerGenerator.java
//         (createExplosion/onExplosion view half) and
//         core/src/mindustry/entities/Effect.java (scorch).

//! `ExplosionSink` host: turns a sim reactor/generator explosion into the
//! `MindFx` effect/decal/shake stream and the `MindAudio` sound event
//! (PAR-0399, L04 T5).

use bevy_ecs::world::World;
use std::sync::Arc;

use mind_core::audio::{AudioEvent, AudioSinkRes};
use mind_core::content::Rgba;
use mind_core::content::registries::fx_meta::EffectId;
use mind_core::content::registries::sound_meta::SoundId;
use mind_core::fx::{FxBus, FxEvent, intern_region};
use mind_core::world::blocks::power::explosion::{ExplosionSink, ReactorExplosion};

/// `Pal.rubble` (`Effect.scorch` tint).
const RUBBLE: Rgba = Rgba::from_rgba8888(0x4d4e58ff);

/// Host implementation installed by `MindSimHost` on every world load.
pub struct HostExplosionSink {
    /// `MindFx` event bus.
    pub bus: Arc<FxBus>,
}

impl ExplosionSink for HostExplosionSink {
    fn explode(&self, world: &mut World, explosion: ReactorExplosion) {
        // `PowerGenerator.java:176-177`: `explodeEffect.at(this)` then
        // `explodeSound.at(this)`.
        for effect in explosion.explode_effects {
            if effect == EffectId::NONE {
                continue;
            }
            self.bus.push(FxEvent::Effect {
                effect: mind_core::content::registries::fx_meta::EffectRef::Named(effect),
                x: explosion.x,
                y: explosion.y,
                rotation: 0.0,
                color: Rgba::WHITE,
            });
        }
        if explosion.shake > 0.0 {
            self.bus.push(FxEvent::Shake {
                intensity: explosion.shake,
                duration: explosion.shake_duration,
            });
        }
        if explosion.scorch_size > 0 {
            // `Effect.scorch(x, y, size)`: `scorch-<clamp(size, 0, 9)>-<0|1>`,
            // `random(4) * 90` rotation, lifetime 3600, `Pal.rubble`.
            let size = explosion.scorch_size.clamp(0, 9);
            let seed = (explosion.x as i32)
                .wrapping_mul(31)
                .wrapping_add(explosion.y as i32) as u32;
            let variant = seed & 1;
            let rotation = ((seed >> 1) & 3) as f32 * 90.0;
            let name = format!("scorch-{size}-{variant}");
            self.bus.push(FxEvent::Decal {
                region: intern_region(&name),
                x: explosion.x,
                y: explosion.y,
                rotation,
                lifetime: 3600.0,
                color: RUBBLE,
            });
        }
        if explosion.explode_sound != SoundId::NONE
            && let Some(sink) = world.get_resource::<AudioSinkRes>()
        {
            sink.emit(AudioEvent::At {
                sound: explosion.explode_sound,
                x: explosion.x,
                y: explosion.y,
                pitch: 1.0,
                volume: 1.0,
                check_frame: true,
            });
        }
    }

    fn generatespark(&self, _world: &mut World, x: f32, y: f32) {
        // `ConsumeItemExplode.java:20-42`: `Fx.generatespark.at(build.x +
        // range, build.y + range)`.
        self.bus.push(FxEvent::Effect {
            effect: mind_core::content::registries::fx_meta::EffectRef::Named(
                EffectId::GENERATESPARK,
            ),
            x,
            y,
            rotation: 0.0,
            color: Rgba::WHITE,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mind_core::world::blocks::power::explosion::ReactorExplosion;

    fn explosion() -> ReactorExplosion {
        ReactorExplosion {
            x: 64.0,
            y: 96.0,
            team: Some(2),
            radius: 19.0,
            damage: 5000.0,
            fire: true,
            puddles: 0,
            puddle_range: 16.0,
            puddle_amount: 0.0,
            puddle_liquid: None,
            shake: 6.0,
            shake_duration: 16.0,
            breaks_props: true,
            scorch_size: 5,
            ignition_chance: 0.0,
            scale_ignition_chance: false,
            speed: 0.4,
            fireballs: 0,
            explode_effects: [EffectId::REACTOR_EXPLOSION, EffectId::NONE, EffectId::NONE],
            explode_sound: SoundId::EXPLOSION_REACTOR,
        }
    }

    #[test]
    fn host_sink_emits_effect_shake_scorch_and_sound() {
        let bus = Arc::new(FxBus::default());
        let sink = HostExplosionSink { bus: bus.clone() };
        let mut world = World::new();
        world.insert_resource(AudioSinkRes::noop());
        sink.explode(&mut world, explosion());
        let events = bus.drain();
        assert!(events.iter().any(|event| matches!(
            event,
            FxEvent::Effect {
                effect: mind_core::content::registries::fx_meta::EffectRef::Named(
                    EffectId::REACTOR_EXPLOSION
                ),
                ..
            }
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            FxEvent::Shake {
                intensity,
                duration
            } if *intensity == 6.0 && *duration == 16.0
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            FxEvent::Decal { region, lifetime, .. }
                if region.0.starts_with("scorch-5-") && *lifetime == 3600.0
        )));
    }

    #[test]
    fn generatespark_uses_the_generator_effect() {
        let bus = Arc::new(FxBus::default());
        let sink = HostExplosionSink { bus: bus.clone() };
        let mut world = World::new();
        sink.generatespark(&mut world, 8.0, 16.0);
        let events = bus.drain();
        assert!(events.iter().any(|event| matches!(
            event,
            FxEvent::Effect {
                effect: mind_core::content::registries::fx_meta::EffectRef::Named(
                    EffectId::GENERATESPARK
                ),
                x,
                y,
                ..
            } if *x == 8.0 && *y == 16.0
        )));
    }
}
