// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Combat: bullets, damage, turrets and defense (plan 10).
//!
//! This module owns the `Bullet` component and behavior half, the `Damage`
//! system, and — in later milestones — weapons, turrets, defense blocks, fires,
//! puddles and lightning. It is Godot-free and tokio-free; all sim randomness
//! goes through [`crate::determinism`] and all view/FX output through the
//! [`view::FxSink`] seam (plan 17).

pub mod bullet;
pub mod damage;
pub mod fires;
pub mod harness;
pub mod lightning;
pub mod puddles;
pub mod targeting;
pub mod view;

pub use bullet::{Bullet, BulletData, BulletSpawn, create};
pub use harness::CombatHarness;
pub use targeting::TargetQueries;
pub use view::{
    BulletDrawState, CombatFx, FxHandle, FxSink, LaserDrawState, NoopFx, ShieldDrawState,
    TurretDrawState, noop_fx,
};

use bevy_ecs::world::World;

/// Plan-10 plugin registration handle.
///
/// The plan-05 schedule is frozen and must not be edited by plan 10, so this
/// plugin is a thin host seam: hosts (in-engine `MindSimHost`, `CombatHarness`)
/// own the combat systems and call them directly. It exists so the plan name and
/// the registration boundary are explicit.
#[derive(Debug, Default, Clone, Copy)]
pub struct CombatPlugin;

impl CombatPlugin {
    /// Registers combat resources on a host world (currently none; reserved for
    /// the shared bullet runtime used by the Godot host).
    pub fn register(&self, _world: &mut World) {}
}
