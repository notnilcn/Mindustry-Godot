// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! FX, effect and draw-part system (plan 17).
//!
//! Godot-free: the effect registry/catalogue, lifecycle pool, `DrawPrim`
//! program resolution, trails, decals, shake and weather-visual math all live
//! here so they are testable with plain `cargo test`. `mind-gdext` only drains
//! the [`sink::FxBus`], owns the pools' view timing and executes programs.
//!
//! Effects are view-only: nothing in this module is serialized, checksummed or
//! synced (HLP §12 C6, plan 17 §2.5).

pub mod angles;
pub mod catalog;
pub mod container;
pub mod custom;
pub mod data;
pub mod decal;
pub mod def;
pub mod parts;
pub mod pool;
pub mod pool_spec;
pub mod resolve;
pub mod shake;
pub mod sink;
pub mod trail;
pub mod weather_fx;

pub use crate::render::draw::DrawProgram;
pub use catalog::{CatalogCounts, build_registry, counts as catalog_counts, order_hash};
pub use container::EffectContainer;
pub use custom::{CustomFxId, FxEmit};
pub use data::{EffectData, EmptySnapshot, Pose, ViewEntityId, ViewSnapshot};
pub use decal::{Decal, DecalPool};
pub use def::{
    CustomParams, EffectDef, EffectKind, EffectRegistry, ExplosionParams, NoiseParams,
    ParticleParams, RadialParams, SoundParams, TriangleParams, WaveParams, WrapParams, registry,
};
pub use parts::{
    EffectSpawnerPartSpec, FlarePartSpec, HaloPartSpec, HoverPartSpec, PartMove, PartParams,
    PartProgressSpec, PartSpec, RegionPartSpec, ShapePartSpec,
};
pub use pool::{EffectState, FxGate, FxPool, PendingSpawn, in_camera};
pub use pool_spec::{DECAL_CAPACITY, DECAL_LIFETIME, DELAYED_SPAWN_CAPACITY, EFFECT_POOL_CAPACITY};
pub use resolve::{build_program, build_program_into};
pub use shake::{ShakeState, shake_falloff, shake_visible};
pub use sink::{FxBus, FxEvent, FxSettings, FxSink, NoopFxSink, emit_named};
pub use trail::TrailRegistry;
pub use weather_fx::{WeatherFx, WeatherKind, WeatherStateView};

/// Converts a plan-16 `Pal`/`Drawf` `[f32; 4]` color to [`crate::content::Rgba`].
impl From<[f32; 4]> for crate::content::Rgba {
    fn from(value: [f32; 4]) -> Self {
        crate::content::Rgba::new(value[0], value[1], value[2], value[3])
    }
}
