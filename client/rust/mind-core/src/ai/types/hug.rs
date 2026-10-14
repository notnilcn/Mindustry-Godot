// Ported from Mindustry (https://github.com/Anuken/Mindustry) — GPL-3.0.
// SPDX-License-Identifier: GPL-3.0-only

//! `HugAI` (`ai/types/HugAI.java`; H01 T9): melee units (renale, latum) hug the
//! closest enemy core, ray-check the approach, close to contact and circle the
//! target. Targeting is `AIController`'s default (`retarget` + `findMainTarget`
//! = `Units.closestTarget`).

use bevy_ecs::entity::Entity;

/// `HugAI` controller state (`AIController.target` + the 40/90-tick
/// `timer(timerTarget)` cadence; same shape as `GroundAiState`'s targeting
/// fields).
#[derive(Debug, Clone, Copy, PartialEq, bevy_ecs::component::Component)]
pub struct HugAiState {
    /// `AIController.timer(timerTarget)` countdown, seeded 39 so the first
    /// acquisition runs on the first tick.
    pub retarget: f32,
    /// `AIController.target`.
    pub target: Option<Entity>,
}

impl Default for HugAiState {
    fn default() -> Self {
        Self {
            retarget: 39.0,
            target: None,
        }
    }
}
