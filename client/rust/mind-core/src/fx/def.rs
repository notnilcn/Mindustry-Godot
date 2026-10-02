// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `EffectDef`, `EffectKind` and the declarative parameter records
//! (plan 17 §3.2/§6.1).
//!
//! The vanilla catalogue lives in [`crate::fx::catalog`]; this module owns the
//! type surface and the append-only registry accessor. Names/ids/lifetimes that
//! make up the parity ABI live in [`crate::content::registries::fx_meta`].

use smallvec::SmallVec;
use std::sync::OnceLock;

use crate::content::registries::sound_meta::SoundId;
use crate::content::{EFFECT_COUNT, EffectId, Rgba};
use crate::math::Interp;
use crate::render::layer::Layer;

use super::custom::CustomFxId;

/// The dead upstream `Effect.layerDuration` field (kept for data compatibility,
/// never read — plan 17 §2.4 #8).
pub const UNUSED_LAYER_DURATION: f32 = 0.0;

/// `EffectKind` — the parameterized effect behaviour class.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum EffectKind {
    /// `Fx.none` / empty effect.
    #[default]
    None,
    /// A [`ParticleParams`] effect (`ParticleEffect`).
    Particle(ParticleParams),
    /// An [`ExplosionParams`] effect (`ExplosionEffect`).
    Explosion(ExplosionParams),
    /// A [`WaveParams`] effect (`WaveEffect`).
    Wave(WaveParams),
    /// A [`TriangleParams`] effect (`TriangleEffect`).
    Triangle(TriangleParams),
    /// A [`NoiseParams`] effect (`NoiseEffect`).
    Noise(NoiseParams),
    /// A [`SoundParams`] effect (`SoundEffect`).
    Sound(SoundParams),
    /// `MultiEffect` — parallel children (plan 17 §3.2).
    Multi(SmallVec<[EffectId; 4]>),
    /// `SeqEffect` — sequential children.
    Seq(SmallVec<[EffectId; 4]>),
    /// `RadialEffect`.
    Radial(RadialParams),
    /// `WrapEffect`.
    Wrap(WrapParams),
    /// A hand-ported Rust body in [`crate::fx::custom`].
    Custom(CustomFxId, CustomParams),
    /// Not yet ported (tracked by the `fx` ledger/audit; see plan 17 §6.3).
    Unported,
}

/// `ParticleEffect` fields (`entities/effect/ParticleEffect.java`).
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleParams {
    /// `colorFrom`.
    pub color_from: Rgba,
    /// `colorTo`.
    pub color_to: Rgba,
    /// `particles`.
    pub particles: i32,
    /// `randLength`.
    pub rand_length: bool,
    /// `casingFlip`.
    pub casing_flip: bool,
    /// `cone` (degrees).
    pub cone: f32,
    /// `length`.
    pub length: f32,
    /// `baseLength`.
    pub base_length: f32,
    /// `interp`.
    pub interp: Interp,
    /// `sizeInterp` (defaults to `interp`).
    pub size_interp: Interp,
    /// `colorInterp` (defaults to `interp`).
    pub color_interp: Interp,
    /// `offsetX`.
    pub offset_x: f32,
    /// `offsetY`.
    pub offset_y: f32,
    /// `lightScl`.
    pub light_scl: f32,
    /// `lightOpacity`.
    pub light_opacity: f32,
    /// `lightColor`.
    pub light_color: Option<Rgba>,
    /// `spin` (degrees/tick).
    pub spin: f32,
    /// `sizeFrom`.
    pub size_from: f32,
    /// `sizeTo`.
    pub size_to: f32,
    /// `sizeChangeStart`.
    pub size_change_start: f32,
    /// `widthChangeStart`.
    pub width_change_start: f32,
    /// `heightChangeStart`.
    pub height_change_start: f32,
    /// `useRotation`.
    pub use_rotation: bool,
    /// `offset` (degrees).
    pub offset: f32,
    /// `region`.
    pub region: &'static str,
    /// `widthFrom`.
    pub width_from: f32,
    /// `widthTo`.
    pub width_to: f32,
    /// `heightFrom`.
    pub height_from: f32,
    /// `heightTo`.
    pub height_to: f32,
    /// `widthInterp` (defaults to `sizeInterp`).
    pub width_interp: Interp,
    /// `heightInterp` (defaults to `sizeInterp`).
    pub height_interp: Interp,
    /// `line`.
    pub line: bool,
    /// `strokeFrom`.
    pub stroke_from: f32,
    /// `strokeTo`.
    pub stroke_to: f32,
    /// `lenFrom`.
    pub len_from: f32,
    /// `lenTo`.
    pub len_to: f32,
    /// `cap`.
    pub cap: bool,
}

impl Default for ParticleParams {
    fn default() -> Self {
        Self {
            color_from: Rgba::WHITE,
            color_to: Rgba::WHITE,
            particles: 6,
            rand_length: true,
            casing_flip: false,
            cone: 180.0,
            length: 20.0,
            base_length: 0.0,
            interp: Interp::Linear,
            size_interp: Interp::Linear,
            color_interp: Interp::Linear,
            offset_x: 0.0,
            offset_y: 0.0,
            light_scl: 2.0,
            light_opacity: 0.6,
            light_color: None,
            spin: 0.0,
            size_from: 2.0,
            size_to: 0.0,
            size_change_start: 0.0,
            width_change_start: 0.0,
            height_change_start: 0.0,
            use_rotation: true,
            offset: 0.0,
            region: "circle",
            width_from: 1.0,
            width_to: 1.0,
            height_from: 1.0,
            height_to: 1.0,
            width_interp: Interp::Linear,
            height_interp: Interp::Linear,
            line: false,
            stroke_from: 2.0,
            stroke_to: 0.0,
            len_from: 4.0,
            len_to: 2.0,
            cap: true,
        }
    }
}

/// `ExplosionEffect` fields (`entities/effect/ExplosionEffect.java`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExplosionParams {
    /// `waveColor`.
    pub wave_color: Rgba,
    /// `smokeColor`.
    pub smoke_color: Rgba,
    /// `sparkColor`.
    pub spark_color: Rgba,
    /// `waveLife`.
    pub wave_life: f32,
    /// `waveStroke`.
    pub wave_stroke: f32,
    /// `waveRad`.
    pub wave_rad: f32,
    /// `waveRadBase`.
    pub wave_rad_base: f32,
    /// `sparkStroke`.
    pub spark_stroke: f32,
    /// `sparkRad`.
    pub spark_rad: f32,
    /// `sparkLen`.
    pub spark_len: f32,
    /// `smokeSize`.
    pub smoke_size: f32,
    /// `smokeSizeBase`.
    pub smoke_size_base: f32,
    /// `smokeRad`.
    pub smoke_rad: f32,
    /// `smokes`.
    pub smokes: i32,
    /// `sparks`.
    pub sparks: i32,
}

impl Default for ExplosionParams {
    fn default() -> Self {
        Self {
            wave_color: Rgba::from(crate::render::drawf::pal::MISSILE_YELLOW),
            smoke_color: Rgba::GRAY,
            spark_color: Rgba::from(crate::render::drawf::pal::MISSILE_YELLOW_BACK),
            wave_life: 6.0,
            wave_stroke: 3.0,
            wave_rad: 15.0,
            wave_rad_base: 2.0,
            spark_stroke: 1.0,
            spark_rad: 23.0,
            spark_len: 3.0,
            smoke_size: 4.0,
            smoke_size_base: 0.5,
            smoke_rad: 23.0,
            smokes: 5,
            sparks: 4,
        }
    }
}

/// `WaveEffect` fields (`entities/effect/WaveEffect.java`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveParams {
    /// `colorFrom`.
    pub color_from: Rgba,
    /// `colorTo`.
    pub color_to: Rgba,
    /// `lightColor`.
    pub light_color: Option<Rgba>,
    /// `sizeFrom`.
    pub size_from: f32,
    /// `sizeTo`.
    pub size_to: f32,
    /// `lightScl`.
    pub light_scl: f32,
    /// `lightOpacity`.
    pub light_opacity: f32,
    /// `sides` (`<= 0` uses `circleVertices`).
    pub sides: i32,
    /// `rotation` (degrees).
    pub rotation: f32,
    /// `strokeFrom`.
    pub stroke_from: f32,
    /// `strokeTo`.
    pub stroke_to: f32,
    /// `interp`.
    pub interp: Interp,
    /// `lightInterp`.
    pub light_interp: Interp,
    /// `offsetX`.
    pub offset_x: f32,
    /// `offsetY`.
    pub offset_y: f32,
}

impl Default for WaveParams {
    fn default() -> Self {
        Self {
            color_from: Rgba::WHITE,
            color_to: Rgba::WHITE,
            light_color: None,
            size_from: 0.0,
            size_to: 100.0,
            light_scl: 3.0,
            light_opacity: 0.8,
            sides: -1,
            rotation: 0.0,
            stroke_from: 2.0,
            stroke_to: 0.0,
            interp: Interp::Linear,
            light_interp: Interp::Reverse,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }
}

/// `TriangleEffect` fields (`entities/effect/TriangleEffect.java`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriangleParams {
    /// `colorFrom`.
    pub color_from: Rgba,
    /// `colorTo`.
    pub color_to: Rgba,
    /// `flippable`.
    pub flippable: bool,
    /// `interp`.
    pub interp: Interp,
    /// `widthInterp`.
    pub width_interp: Interp,
    /// `heightInterp`.
    pub height_interp: Interp,
    /// `colorInterp`.
    pub color_interp: Interp,
    /// `startX`.
    pub start_x: f32,
    /// `startY`.
    pub start_y: f32,
    /// `endX`.
    pub end_x: f32,
    /// `endY`.
    pub end_y: f32,
    /// `lightScl`.
    pub light_scl: f32,
    /// `lightOpacityFrom`.
    pub light_opacity_from: f32,
    /// `lightOpacityTo`.
    pub light_opacity_to: f32,
    /// `lightColor`.
    pub light_color: Option<Rgba>,
    /// `widthFrom`.
    pub width_from: f32,
    /// `widthTo`.
    pub width_to: f32,
    /// `heightFrom`.
    pub height_from: f32,
    /// `heightTo`.
    pub height_to: f32,
    /// `useRotation`.
    pub use_rotation: bool,
    /// `spin`.
    pub spin: i32,
    /// `offset`.
    pub offset: f32,
}

impl Default for TriangleParams {
    fn default() -> Self {
        Self {
            color_from: Rgba::WHITE,
            color_to: Rgba::WHITE,
            flippable: false,
            interp: Interp::Linear,
            width_interp: Interp::Linear,
            height_interp: Interp::Linear,
            color_interp: Interp::Linear,
            start_x: 0.0,
            start_y: 0.0,
            end_x: 0.0,
            end_y: 0.0,
            light_scl: 8.0,
            light_opacity_from: 0.6,
            light_opacity_to: 0.0,
            light_color: None,
            width_from: 4.0,
            width_to: 0.0,
            height_from: 4.0,
            height_to: 4.0,
            use_rotation: true,
            spin: 0,
            offset: 0.0,
        }
    }
}

/// `NoiseEffect` fields (`entities/effect/NoiseEffect.java`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseParams {
    /// `noisePath`.
    pub noise_path: &'static str,
    /// `color`.
    pub color: Option<Rgba>,
    /// `noiseScl`.
    pub noise_scl: f32,
    /// `opacity`.
    pub opacity: f32,
    /// `baseSpeed`.
    pub base_speed: f32,
    /// `intensity`.
    pub intensity: f32,
    /// `windX`.
    pub wind_x: f32,
    /// `windY`.
    pub wind_y: f32,
    /// `layers`.
    pub layers: i32,
    /// `layerSpeedMul`.
    pub layer_speed_mul: f32,
    /// `layerAlphaMul`.
    pub layer_alpha_mul: f32,
    /// `layerSclMul`.
    pub layer_scl_mul: f32,
    /// `layerColorMul`.
    pub layer_color_mul: f32,
}

impl Default for NoiseParams {
    fn default() -> Self {
        Self {
            noise_path: "sprites/distortAlpha.png",
            color: None,
            noise_scl: 1000.0,
            opacity: 0.3,
            base_speed: 0.4,
            intensity: 1.0,
            wind_x: 1.0,
            wind_y: 0.0,
            layers: 4,
            layer_speed_mul: -1.3,
            layer_alpha_mul: 0.7,
            layer_scl_mul: 0.8,
            layer_color_mul: 0.9,
        }
    }
}

/// `SoundEffect` fields (`entities/effect/SoundEffect.java`).
#[derive(Clone, Debug, PartialEq)]
pub struct SoundParams {
    /// `sound`.
    pub sound: SoundId,
    /// `minPitch`.
    pub min_pitch: f32,
    /// `maxPitch`.
    pub max_pitch: f32,
    /// `minVolume`.
    pub min_volume: f32,
    /// `maxVolume`.
    pub max_volume: f32,
    /// Child effect (possibly `Fx.none`).
    pub effect: EffectId,
}

impl Default for SoundParams {
    fn default() -> Self {
        Self {
            sound: SoundId::NONE,
            min_pitch: 0.8,
            max_pitch: 1.2,
            min_volume: 1.0,
            max_volume: 1.0,
            effect: EffectId::NONE,
        }
    }
}

/// `RadialEffect` fields (`entities/effect/RadialEffect.java`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialParams {
    /// `effect`.
    pub effect: EffectId,
    /// `amount`.
    pub amount: i32,
    /// `rotationSpacing`.
    pub rotation_spacing: f32,
    /// `rotationOffset`.
    pub rotation_offset: f32,
    /// `effectRotationOffset`.
    pub effect_rotation_offset: f32,
    /// `lengthOffset`.
    pub length_offset: f32,
}

impl Default for RadialParams {
    fn default() -> Self {
        Self {
            effect: EffectId::NONE,
            amount: 4,
            rotation_spacing: 90.0,
            rotation_offset: 0.0,
            effect_rotation_offset: 0.0,
            length_offset: 0.0,
        }
    }
}

/// `WrapEffect` fields (`entities/effect/WrapEffect.java`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WrapParams {
    /// Child effect.
    pub effect: EffectId,
    /// `color`.
    pub color: Rgba,
    /// `rotation` (degrees).
    pub rotation: f32,
}

impl Default for WrapParams {
    fn default() -> Self {
        Self {
            effect: EffectId::NONE,
            color: Rgba::WHITE,
            rotation: 0.0,
        }
    }
}

/// Per-entry parameters for a hand-ported custom body.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CustomParams {
    /// Body colors.
    pub colors: [Rgba; 2],
    /// Free float 1.
    pub f1: f32,
    /// Free float 2.
    pub f2: f32,
    /// Free float 3.
    pub f3: f32,
    /// Free int.
    pub extra: u32,
}

impl Default for CustomParams {
    fn default() -> Self {
        Self {
            colors: [Rgba::WHITE, Rgba::WHITE],
            f1: 0.0,
            f2: 0.0,
            f3: 0.0,
            extra: 0,
        }
    }
}

/// A registered effect definition. Ids are declaration order, append-only.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectDef {
    /// Registry index (`Effect.id`).
    pub id: EffectId,
    /// Upstream field name (parity/mod ABI).
    pub name: &'static str,
    /// `Effect.lifetime` (ticks).
    pub lifetime: f32,
    /// `Effect.clip` (radius).
    pub clip: f32,
    /// `Effect.startDelay` (ticks).
    pub start_delay: f32,
    /// `Effect.baseRotation` (degrees).
    pub base_rotation: f32,
    /// `Effect.followParent`.
    pub follow_parent: bool,
    /// `Effect.rotWithParent`.
    pub rot_with_parent: bool,
    /// `Effect.layer` (`Layer.effect` by default).
    pub layer: f32,
    /// Dead upstream field (`Effect.layerDuration`).
    pub layer_duration: f32,
    /// Behaviour.
    pub kind: EffectKind,
}

impl EffectDef {
    /// A `none`-like default for the seed table index.
    pub fn blank(id: EffectId, name: &'static str, lifetime: f32, clip: f32) -> Self {
        Self {
            id,
            name,
            lifetime,
            clip,
            start_delay: 0.0,
            base_rotation: 0.0,
            follow_parent: true,
            rot_with_parent: false,
            layer: Layer::Effect.z(),
            layer_duration: UNUSED_LAYER_DURATION,
            kind: EffectKind::None,
        }
    }

    /// Whether this is the `none` effect (id 0).
    pub fn is_none(&self) -> bool {
        self.id == EffectId::NONE
    }
}

/// The immutable effect registry, built once from the catalogue.
#[derive(Debug)]
pub struct EffectRegistry {
    defs: Vec<EffectDef>,
}

impl EffectRegistry {
    /// Wraps an already-built catalogue.
    pub fn from_defs(defs: Vec<EffectDef>) -> Self {
        debug_assert_eq!(defs.len(), EFFECT_COUNT, "Fx catalogue length drift");
        Self { defs }
    }

    /// `Effect.get(id)`.
    pub fn get(&self, id: EffectId) -> &EffectDef {
        let index = (id.raw() as usize).min(self.defs.len().saturating_sub(1));
        &self.defs[index]
    }

    /// Number of registered effects.
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// Iterates all definitions in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = &EffectDef> {
        self.defs.iter()
    }

    /// Looks up by upstream name.
    pub fn by_name(&self, name: &str) -> Option<&EffectDef> {
        self.defs.iter().find(|d| d.name == name)
    }
}

/// The process-wide registry (built lazily from [`crate::fx::catalog`]).
pub fn registry() -> &'static EffectRegistry {
    static REGISTRY: OnceLock<EffectRegistry> = OnceLock::new();
    REGISTRY.get_or_init(|| EffectRegistry::from_defs(super::catalog::build_registry()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_full_and_ordered() {
        let reg = registry();
        assert_eq!(reg.len(), EFFECT_COUNT);
        assert_eq!(reg.get(EffectId(0)).name, "none");
        assert!(reg.get(EffectId(0)).is_none());
        assert_eq!(reg.get(EffectId::HIT_BULLET_SMALL).name, "hitBulletSmall");
        assert_eq!(reg.get(EffectId(266)).name, "debugRect");
    }

    #[test]
    fn names_are_unique_and_match_seed() {
        let reg = registry();
        for def in reg.iter() {
            assert_eq!(
                crate::content::registries::fx_meta::EFFECTS[def.id.raw() as usize].name,
                def.name
            );
            assert_eq!(
                crate::content::registries::fx_meta::EFFECTS[def.id.raw() as usize].lifetime,
                def.lifetime
            );
        }
    }
}
