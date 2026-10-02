// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! Health component (`core/src/mindustry/entities/comp/HealthComp.java`).
//!
//! Plan 05's component set did not include `Health`; plan 07 adds it because the
//! building runtime owns damage/health for placed blocks. Units reuse the same
//! component (plan 11).

use bevy_ecs::component::Component;
use bevy_ecs::world::World;
use mind_macros::SimComponent;

/// Health/max-health/dead state (`HealthComp`).
#[derive(Debug, Clone, Copy, PartialEq, Component, SimComponent)]
#[sim(component, base)]
pub struct Health {
    /// Current health (`HealthComp.health`).
    pub health: f32,
    /// Maximum health (`HealthComp.maxHealth`).
    pub max_health: f32,
    /// Whether the entity is dead (`HealthComp.dead`).
    pub dead: bool,
}

impl Health {
    /// Creates a full-health component.
    pub fn new(max_health: f32) -> Self {
        Self {
            health: max_health,
            max_health,
            dead: false,
        }
    }

    /// Current health fraction (0 when max is 0).
    pub fn fraction(&self) -> f32 {
        if self.max_health <= 0.0 {
            0.0
        } else {
            (self.health / self.max_health).clamp(0.0, 1.0)
        }
    }

    /// Whether the entity took any damage (`health < maxHealth - 0.001`).
    pub fn damaged(&self) -> bool {
        self.health < self.max_health - 0.001
    }
}

/// Current health of an entity (`health::hp`).
pub fn hp(world: &World, entity: bevy_ecs::entity::Entity) -> f32 {
    world
        .get::<Health>(entity)
        .map(|health| health.health)
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_fraction_and_damaged() {
        let mut health = Health::new(100.0);
        assert_eq!(health.fraction(), 1.0);
        assert!(!health.damaged());
        health.health = 50.0;
        assert_eq!(health.fraction(), 0.5);
        assert!(health.damaged());
        health.max_health = 0.0;
        assert_eq!(health.fraction(), 0.0);
    }
}
