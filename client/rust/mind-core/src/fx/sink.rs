// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! The plan-17 FX playback seam (HLP §12 C6).
//!
//! Simulation code calls the plan-10 [`FxSink`] trait (`fx.effect(...)`,
//! `fx.shake(...)`, …) unconditionally; the concrete [`FxBus`] queues
//! append-only view notifications that `mind-gdext` drains after each
//! `Sim::tick()`. No sim system ever reads the sink, and the queue is excluded
//! from checksums/snapshots. Headless installs [`NoopFxSink`].

use std::sync::Mutex;

use crate::content::registries::fx_meta::EffectRef;
use crate::content::registries::sound_meta::SoundId;
use crate::content::{EffectId, Rgba};
use crate::render::RegionKey;

use super::data::TrailChannelId;

pub use crate::combat::view::FxSink;

/// Settings mirrored from `Renderer.enableEffects` + weather/screenshake
/// (plan 17 §6.4). Headless ignores all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxSettings {
    /// `effects` setting (default true).
    pub effects: bool,
    /// `screenshake` setting `0..=4` (default 4).
    pub screenshake: i32,
    /// `showweather` setting (default true).
    pub showweather: bool,
}

impl Default for FxSettings {
    fn default() -> Self {
        Self {
            effects: true,
            screenshake: 4,
            showweather: true,
        }
    }
}

/// A queued view notification.
#[derive(Clone, Debug, PartialEq)]
pub enum FxEvent {
    /// `Effect.at` (named or inline).
    Effect {
        /// Effect reference.
        effect: EffectRef,
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Rotation in degrees.
        rotation: f32,
        /// Tint.
        color: Rgba,
    },
    /// `Effect.shake`.
    Shake {
        /// Intensity.
        intensity: f32,
        /// Duration in ticks.
        duration: f32,
    },
    /// A light request (`Drawf.light`).
    Light {
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Radius.
        radius: f32,
        /// Tint.
        color: Rgba,
        /// Opacity.
        opacity: f32,
    },
    /// A one-shot sound (`Sound.at`).
    Sound {
        /// Sound id.
        sound: SoundId,
        /// Volume.
        volume: f32,
        /// Pitch.
        pitch: f32,
    },
    /// A trail update.
    Trail {
        /// Registry channel (`Some` for `FxSink::trail_channel`, plan 17 §3.9).
        channel: Option<TrailChannelId>,
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Rotation in degrees.
        rotation: f32,
        /// Tint.
        color: Rgba,
        /// Width.
        width: f32,
        /// Length (points).
        length: f32,
    },
    /// A decal (`Effect.decal`/`scorch`/`rubble`).
    Decal {
        /// Region name.
        region: RegionKey,
        /// World x.
        x: f32,
        /// World y.
        y: f32,
        /// Rotation in degrees.
        rotation: f32,
        /// Lifetime in ticks.
        lifetime: f32,
        /// Tint.
        color: Rgba,
    },
}

/// The default headless sink: records nothing.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopFxSink;

impl FxSink for NoopFxSink {}

/// Recording/queue sink. `Send + Sync` via an internal mutex; only used on the
/// view thread in practice.
#[derive(Debug, Default)]
pub struct FxBus {
    state: Mutex<FxBusState>,
}

/// Interior queue state.
#[derive(Debug, Default)]
pub struct FxBusState {
    /// Queued events in emission order.
    pub events: Vec<FxEvent>,
    /// Counter of events dropped after a cap (never expected in practice).
    pub dropped: u64,
    /// Upper bound; `0` = unbounded.
    pub cap: usize,
}

impl FxBus {
    /// Unbounded bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bounded bus (drops oldest beyond `cap`).
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            state: Mutex::new(FxBusState {
                events: Vec::new(),
                dropped: 0,
                cap,
            }),
        }
    }

    /// Pushes an event.
    pub fn push(&self, event: FxEvent) {
        if let Ok(mut state) = self.state.lock() {
            if state.cap > 0 && state.events.len() >= state.cap {
                state.events.remove(0);
                state.dropped += 1;
            }
            state.events.push(event);
        }
    }

    /// Drains all queued events (FIFO).
    pub fn drain(&self) -> Vec<FxEvent> {
        match self.state.lock() {
            Ok(mut state) => std::mem::take(&mut state.events),
            Err(_) => Vec::new(),
        }
    }

    /// Number of queued events.
    pub fn len(&self) -> usize {
        self.state.lock().map(|s| s.events.len()).unwrap_or(0)
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drops all queued events.
    pub fn clear(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.events.clear();
        }
    }

    /// Number of dropped events.
    pub fn dropped(&self) -> u64 {
        self.state.lock().map(|s| s.dropped).unwrap_or(0)
    }

    /// Queues a decal (view-side helper for `Effect.decal`).
    #[allow(clippy::too_many_arguments)]
    pub fn decal(
        &self,
        region: RegionKey,
        x: f32,
        y: f32,
        rotation: f32,
        lifetime: f32,
        color: Rgba,
    ) {
        self.push(FxEvent::Decal {
            region,
            x,
            y,
            rotation,
            lifetime,
            color,
        });
    }
}

impl FxSink for FxBus {
    fn effect(&self, effect: &EffectRef, x: f32, y: f32, rotation: f32, color: Rgba) {
        self.push(FxEvent::Effect {
            effect: effect.clone(),
            x,
            y,
            rotation,
            color,
        });
    }

    fn shake(&self, amount: f32) {
        self.push(FxEvent::Shake {
            intensity: amount,
            duration: 0.0,
        });
    }

    fn light(&self, x: f32, y: f32, radius: f32, color: Rgba) {
        self.push(FxEvent::Light {
            x,
            y,
            radius,
            color,
            opacity: 1.0,
        });
    }

    fn sound(&self, sound: SoundId, volume: f32, pitch: f32) {
        self.push(FxEvent::Sound {
            sound,
            volume,
            pitch,
        });
    }

    fn trail(&self, x: f32, y: f32, rotation: f32, color: Rgba, width: f32, length: f32) {
        self.push(FxEvent::Trail {
            channel: None,
            x,
            y,
            rotation,
            color,
            width,
            length,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn trail_channel(
        &self,
        channel: TrailChannelId,
        x: f32,
        y: f32,
        rotation: f32,
        color: Rgba,
        width: f32,
        length: f32,
    ) {
        self.push(FxEvent::Trail {
            channel: Some(channel),
            x,
            y,
            rotation,
            color,
            width,
            length,
        });
    }
}

/// Convenience: the plan-17 named effect-id form of [`FxSink::effect`].
pub fn emit_named(sink: &dyn FxSink, id: EffectId, x: f32, y: f32, rotation: f32, color: Rgba) {
    sink.effect(&EffectRef::Named(id), x, y, rotation, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_records_and_drains_in_order() {
        let bus = FxBus::new();
        emit_named(&bus, EffectId::SMOKE, 1.0, 2.0, 0.0, Rgba::WHITE);
        bus.shake(4.0);
        assert_eq!(bus.len(), 2);
        let events = bus.drain();
        assert!(bus.is_empty());
        assert!(matches!(
            events[0],
            FxEvent::Effect {
                effect: EffectRef::Named(EffectId::SMOKE),
                ..
            }
        ));
        assert!(matches!(events[1], FxEvent::Shake { intensity, .. } if intensity == 4.0));
    }

    #[test]
    fn bounded_bus_drops_oldest() {
        let bus = FxBus::with_capacity(2);
        emit_named(&bus, EffectId::SMOKE, 0.0, 0.0, 0.0, Rgba::WHITE);
        emit_named(&bus, EffectId::SMOKE, 1.0, 0.0, 0.0, Rgba::WHITE);
        emit_named(&bus, EffectId::SMOKE, 2.0, 0.0, 0.0, Rgba::WHITE);
        assert_eq!(bus.len(), 2);
        assert_eq!(bus.dropped(), 1);
        let events = bus.drain();
        if let FxEvent::Effect { x, .. } = events[0] {
            assert_eq!(x, 1.0);
        } else {
            panic!("expected effect event");
        }
    }

    #[test]
    fn noop_sink_accepts_calls() {
        let sink = NoopFxSink;
        emit_named(&sink, EffectId::SMOKE, 0.0, 0.0, 0.0, Rgba::WHITE);
        sink.shake(1.0);
    }

    #[test]
    fn trail_channel_is_carried() {
        let bus = FxBus::new();
        bus.trail_channel(TrailChannelId(7), 1.0, 2.0, 0.0, Rgba::WHITE, 3.0, 4.0);
        // The legacy entry point still records an unkeyed trail.
        bus.trail(5.0, 6.0, 0.0, Rgba::WHITE, 1.0, 2.0);
        let events = bus.drain();
        assert!(matches!(
            events[0],
            FxEvent::Trail {
                channel: Some(TrailChannelId(7)),
                ..
            }
        ));
        assert!(matches!(events[1], FxEvent::Trail { channel: None, .. }));
    }
}
