// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Cross-family block helper traits (`world/blocks/{ControlBlock,RotBlock,
//! UnitTetherBlock,LaunchAnimator,ExplosionShield}.java`).
//!
//! Plan 07 §3.12: these are Rust traits implemented by family modules and later
//! plans (10/11/12). They carry behavior hooks only; state lives on the owning
//! plan's components.

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;

/// `ControlBlock` — blocks that can control a unit.
pub trait ControlBlock {
    /// The controlled unit, if any.
    fn unit(&self, _world: &World, _e: Entity) -> Option<Entity> {
        None
    }
    /// Whether a unit is being controlled.
    fn is_controlled(&self, _world: &World, _e: Entity) -> bool {
        false
    }
    /// Whether `unit` may be controlled.
    fn can_control(&self, _world: &World, _e: Entity, _unit: Entity) -> bool {
        false
    }
    /// Whether the block should auto-target.
    fn should_auto_target(&self, _world: &World, _e: Entity) -> bool {
        true
    }
}

/// `RotBlock` — overrides effective build rotation (e.g. mirrored rotors).
pub trait RotBlock {
    /// Effective rotation for a raw build rotation.
    fn build_rotation(&self, _rotation: u8, _size: i32) -> u8 {
        _rotation
    }
}

/// `UnitTetherBlock` — tracks a spawned unit by id (`spawned(id)`).
pub trait UnitTetherBlock {
    /// Whether `id` is the currently tethered unit.
    fn spawned(&self, _id: u32) -> bool {
        false
    }
}

/// `LaunchAnimator` — shared launch animation state (`Accelerator`/`LaunchPad`).
pub trait LaunchAnimator {
    /// Launch duration in ticks (`launchDuration`).
    fn launch_duration(&self) -> f32 {
        0.0
    }
    /// Camera zoom during launch.
    fn zoom(&self) -> f32 {
        0.0
    }
    /// Whether the launch has begun.
    fn is_launching(&self) -> bool {
        false
    }
}

/// `ExplosionShield` — absorbs explosions over a radius.
pub trait ExplosionShield {
    /// Whether an explosion at `(x, y)` is absorbed.
    fn absorb_explosion(&self, _x: f32, _y: f32) -> bool {
        false
    }
    /// Tile radius protected.
    fn shield_radius(&self) -> i32 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct IdentityRot;
    impl RotBlock for IdentityRot {}

    #[test]
    fn default_helper_traits_are_inert() {
        assert_eq!(IdentityRot.build_rotation(2, 2), 2);
        struct NoShield;
        impl ExplosionShield for NoShield {}
        assert!(!NoShield.absorb_explosion(0.0, 0.0));
        assert_eq!(NoShield.shield_radius(), 0);
    }
}
