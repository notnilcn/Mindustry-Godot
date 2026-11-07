// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Inline (`EffectRef::Inline`) effect playback (plan 17 §3.2).
//!
//! Java content constructs anonymous composites inline
//! (`drillEffect = new MultiEffect(Fx.mineImpact, Fx.drillSteam,
//! Fx.mineImpactWave.wrap(Pal.redLight, 40f))`, `Blocks.java:3094`). The port
//! keeps that data verbatim in [`crate::content::registries::fx_meta::EffectSpec`];
//! this module is the *play* half: it flattens a reference into the named `Fx`
//! spawns the view pool actually consumes.
//!
//! The expansion mirrors the upstream `create` overrides exactly:
//!
//! * `MultiEffect.create` (`MultiEffect.java:20-27`) forwards
//!   `(x, y, rotation, color, data)` to every child.
//! * `WrapEffect.create` (`WrapEffect.java:43-46`) replaces the rotation and
//!   colour with its own (`effect.create(x, y, this.rotation, this.color, data)`)
//!   rather than accumulating them.
//! * Every other inline class owns a renderer body (plan 17 draw code); their
//!   leaves are not namable here, so they produce no spawn.

use bevy_ecs::world::World;
use smallvec::SmallVec;

use crate::content::EffectId;
use crate::content::Rgba;
use crate::content::registries::fx_meta::{EffectRef, EffectSpec, InlineEffectKind};
use crate::determinism::{RngStream, SimRng};

use super::sink::FxSink;

/// One named spawn produced by flattening an [`EffectRef`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxEmission {
    /// Named `Fx` effect.
    pub effect: EffectId,
    /// Rotation in degrees (the `WrapEffect.rotation` override when wrapped).
    pub rotation: f32,
    /// Tint (the `WrapEffect.color` override when wrapped).
    pub color: Rgba,
}

/// `Fx.<name>` reference.
pub fn named(effect: EffectId) -> EffectRef {
    EffectRef::Named(effect)
}

/// `new MultiEffect(children...)` (`MultiEffect.java:11-17`). The lifetime is
/// the longest child's (`MultiEffect.init` leaves the `Effect` default; the
/// spawn-time expansion is what matters).
pub fn multi(children: Vec<EffectRef>) -> EffectRef {
    let lifetime = children
        .iter()
        .map(EffectRef::lifetime)
        .fold(0.0_f32, f32::max);
    EffectRef::Inline(Box::new(EffectSpec::multi(lifetime, children)))
}

/// `new MultiEffect(...)`: the common `Fx`-only case.
pub fn multi_of(effects: &[EffectId]) -> EffectRef {
    multi(effects.iter().copied().map(named).collect())
}

/// `effect.wrap(color, rotation)` (`Effect.java`, `WrapEffect.java:33-37`):
/// the child is created at `rotation` with `color` instead of the caller's.
pub fn wrap(child: EffectRef, color: Rgba, rotation: f32) -> EffectRef {
    EffectRef::Inline(Box::new(EffectSpec::wrap_rotated(child, color, rotation)))
}

/// Flattens `effect` into the named spawns it produces at `(rotation, color)`.
pub fn flatten(effect: &EffectRef, rotation: f32, color: Rgba) -> SmallVec<[FxEmission; 4]> {
    let mut out = SmallVec::new();
    push_flattened(effect, rotation, color, &mut out);
    out
}

fn push_flattened(
    effect: &EffectRef,
    rotation: f32,
    color: Rgba,
    out: &mut SmallVec<[FxEmission; 4]>,
) {
    match effect {
        EffectRef::Named(id) => out.push(FxEmission {
            effect: *id,
            rotation,
            color,
        }),
        EffectRef::Inline(spec) => match spec.kind {
            // `MultiEffect.create`: parallel children, same transform.
            InlineEffectKind::MultiEffect => {
                for child in &spec.children {
                    push_flattened(child, rotation, color, out);
                }
            }
            // `WrapEffect.create`: the wrapper's own rotation/colour win.
            InlineEffectKind::WrapEffect => {
                if let Some(child) = spec.children.first() {
                    push_flattened(child, spec.rotation, spec.color.unwrap_or(color), out);
                }
            }
            // A leaf inline body (plain `Effect`/`ExplosionEffect`/...) has a
            // renderer half owned by the draw code, not a named `Fx` id.
            _ => {}
        },
    }
}

/// `Effect.at(x, y, rotation, color)` for a possibly-inline reference: queues
/// one named spawn per flattened leaf.
pub fn play(sink: &dyn FxSink, effect: &EffectRef, x: f32, y: f32, rotation: f32, color: Rgba) {
    for emission in flatten(effect, rotation, color) {
        sink.effect(
            &EffectRef::Named(emission.effect),
            x,
            y,
            emission.rotation,
            emission.color,
        );
    }
}

/// View-only RNG for `Mathf.range`-style effect offsets (`BurstDrill`'s
/// `drillEffectRnd`, `BurstDrill.java:113-118`). Java draws these from the
/// unseeded global `Mathf.rand`; the port keeps them off the gameplay stream
/// (`RngStream::Fx`, never in the checksum) exactly like
/// [`crate::audio::sim::AudioViewRng`].
#[derive(bevy_ecs::resource::Resource, Debug, Clone)]
pub struct FxViewRng(pub SimRng);

impl Default for FxViewRng {
    fn default() -> Self {
        Self(SimRng::new(0))
    }
}

/// `Mathf.range(amount)` (`i32 range(amount)`: `[-amount, amount)`), two
/// independent draws for the x/y offset. `amount <= 0` is the identity and
/// never touches the stream.
pub fn jitter(world: &mut World, amount: f32) -> (f32, f32) {
    if amount <= 0.0 {
        return (0.0, 0.0);
    }
    let mut rng = world.get_resource_or_insert_with(FxViewRng::default);
    let mut draw = || (rng.0.next_float(RngStream::Fx) * 2.0 - 1.0) * amount;
    let dx = draw();
    let dy = draw();
    (dx, dy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::registries::pal;

    fn red() -> Rgba {
        pal::RED_LIGHT
    }

    /// `Blocks.java:3094`: `new MultiEffect(Fx.mineImpact, Fx.drillSteam,
    /// Fx.mineImpactWave.wrap(Pal.redLight, 40f))` flattens to three named
    /// spawns, the last with the wrap's colour and rotation.
    #[test]
    fn multi_with_wrap_flattens_to_named_children() {
        let blue = Rgba::from_rgba8888(0x123456ff);
        let effect = multi(vec![
            named(EffectId::MINE_IMPACT),
            named(EffectId::DRILL_STEAM),
            wrap(named(EffectId::MINE_IMPACT_WAVE), red(), 40.0),
        ]);
        let out = flatten(&effect, 15.0, blue);
        assert_eq!(out.len(), 3, "{out:?}");
        assert_eq!(out[0].effect, EffectId::MINE_IMPACT);
        assert_eq!(out[0].rotation, 15.0);
        assert_eq!(out[0].color, blue);
        assert_eq!(out[1].effect, EffectId::DRILL_STEAM);
        assert_eq!(out[2].effect, EffectId::MINE_IMPACT_WAVE);
        assert_eq!(out[2].rotation, 40.0, "WrapEffect.rotation replaces");
        assert_eq!(out[2].color, red(), "WrapEffect.color replaces");
    }

    /// `MultiEffect.java:20-27` forwards the transform; nesting recurses.
    #[test]
    fn nested_multi_and_wrap_recurse() {
        let effect = multi(vec![
            multi_of(&[EffectId::MINE_IMPACT, EffectId::DRILL_STEAM]),
            wrap(
                multi_of(&[EffectId::DYNAMIC_SPIKES]),
                Rgba::from_rgba8888(0x9eabf7ff),
                30.0,
            ),
        ]);
        let out = flatten(&effect, 0.0, Rgba::WHITE);
        let ids: Vec<u16> = out.iter().map(|e| e.effect.raw()).collect();
        assert_eq!(
            ids,
            vec![
                EffectId::MINE_IMPACT.raw(),
                EffectId::DRILL_STEAM.raw(),
                EffectId::DYNAMIC_SPIKES.raw()
            ]
        );
        assert_eq!(out[2].rotation, 30.0);
        assert_eq!(out[2].color, Rgba::from_rgba8888(0x9eabf7ff));
    }

    /// A named reference is its own single spawn (the pool applies
    /// `Effect.baseRotation` later, so the rotation passes through raw).
    #[test]
    fn named_reference_is_a_single_spawn() {
        let out = flatten(&named(EffectId::SHOCKWAVE), 90.0, red());
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0],
            FxEmission {
                effect: EffectId::SHOCKWAVE,
                rotation: 90.0,
                color: red()
            }
        );
    }

    /// Leaf inline bodies have no named id: nothing is spawned here (their
    /// draw half is the renderer's).
    #[test]
    fn plain_inline_leaf_has_no_named_spawn() {
        let leaf = EffectRef::Inline(Box::new(EffectSpec::plain(30.0)));
        assert!(flatten(&leaf, 0.0, Rgba::WHITE).is_empty());
    }

    /// `play` queues every flattened leaf through the view sink.
    #[test]
    fn play_queues_each_leaf() {
        let bus = super::super::sink::FxBus::new();
        let effect = multi(vec![
            named(EffectId::MINE_IMPACT),
            wrap(named(EffectId::MINE_IMPACT_WAVE), red(), 40.0),
        ]);
        let color = Rgba::from_rgba8888(0xff0000ff);
        play(&bus, &effect, 100.0, 200.0, 0.0, color);
        let events = bus.drain();
        assert_eq!(events.len(), 2, "{events:?}");
        let super::super::sink::FxEvent::Effect {
            effect,
            x,
            y,
            color: tint,
            ..
        } = &events[0]
        else {
            panic!("expected an effect event: {events:?}");
        };
        assert_eq!(effect, &EffectRef::Named(EffectId::MINE_IMPACT));
        assert_eq!((*x, *y), (100.0, 200.0));
        assert_eq!(*tint, color);
        match &events[1] {
            super::super::sink::FxEvent::Effect {
                effect,
                rotation,
                color,
                ..
            } => {
                assert_eq!(effect, &EffectRef::Named(EffectId::MINE_IMPACT_WAVE));
                assert_eq!(*rotation, 40.0);
                assert_eq!(*color, red());
            }
            other => panic!("expected the wrapped effect: {other:?}"),
        }
    }

    /// `Mathf.range(drillEffectRnd)`: a zero radius is the exact identity and
    /// a positive radius stays inside `[-amount, amount)`.
    #[test]
    fn jitter_is_identity_at_zero_and_bounded() {
        let mut world = World::new();
        assert_eq!(jitter(&mut world, 0.0), (0.0, 0.0));
        assert!(!world.contains_resource::<FxViewRng>(), "no RNG touched");
        for _ in 0..32 {
            let (x, y) = jitter(&mut world, 6.0);
            assert!((-6.0..6.0).contains(&x), "x {x}");
            assert!((-6.0..6.0).contains(&y), "y {y}");
        }
        assert!(world.contains_resource::<FxViewRng>());
    }
}
